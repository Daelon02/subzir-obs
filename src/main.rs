mod dialogue;
mod dialogue_ocr;
mod legend;
mod stability;
mod zones;
use dialogue::{Card, DialogueHistory, ShownCaption};
use serde_json::{json, Value};
use stability::{Observation, Stabilizer};
use std::{
    env, fs,
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use windows::{
    core::{HSTRING, PCWSTR},
    Globalization::Language,
    Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap},
    Media::Ocr::{OcrEngine, OcrResult},
    Security::Cryptography::CryptographicBuffer,
    Win32::{
        Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING},
        System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED},
    },
};
const WIDTH: i32 = 960;
const HEIGHT: i32 = 144;
const CAPTURE_FPS: u8 = 15;
fn capture_fps() -> u8 {
    match env::var("SUB_CAPTURE_FPS").as_deref() {
        Ok("10") => 10,
        _ => CAPTURE_FPS,
    }
}
const FRAME_BYTES: usize = (WIDTH * HEIGHT) as usize;
const POLL: Duration = Duration::from_millis(2);
const ACK_LIMIT: Duration = Duration::from_millis(700);
#[derive(Debug, Error)]
enum AppError {
    #[error("{0}")]
    Config(String),
    #[error("I/O: {0}")]
    Io(#[from] io::Error),
    #[error("Windows OCR or file update: {0}")]
    Windows(#[from] windows::core::Error),
    #[error("Worker protocol: {0}")]
    Json(#[from] serde_json::Error),
}
type Result<T> = std::result::Result<T, AppError>;
fn setting(key: &str) -> Result<String> {
    env::var(key).map_err(|_| AppError::Config(format!("Set {key}")))
}
fn pipe<T>(p: Option<T>) -> Result<T> {
    p.ok_or_else(|| AppError::Config("Child pipe unavailable".into()))
}
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
const HY_WEIGHTS: &str = "Hy-MT2-1.8B-Q6_K.gguf";
const CUSTOM_PREFIX: &str = "gguf:";

fn gguf_path(app: &Path, model: &str) -> Result<PathBuf> {
    if model == "hy" {
        return Ok(app.join("hy-runtime").join(HY_WEIGHTS));
    }
    let name = model.strip_prefix(CUSTOM_PREFIX).unwrap_or_default();
    let file = Path::new(name);
    if file.file_name().and_then(|s| s.to_str()) != Some(name)
        || name.contains([':', '/', '\\'])
        || !file
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("gguf"))
    {
        return Err(AppError::Config(
            "Select a GGUF file inside app/models".into(),
        ));
    }
    Ok(app.join("models").join(file))
}

fn start_hy(app: &Path, model: &str, parallel: bool) -> Result<(OwnedChild, String, String)> {
    use std::net::{Ipv4Addr, TcpListener};
    let address = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?.local_addr()?;
    let key = CryptographicBuffer::EncodeToHexString(&CryptographicBuffer::GenerateRandom(32)?)?
        .to_string();
    let runtime = app
        .join("hy-runtime")
        .canonicalize()
        .map_err(|e| AppError::Config(format!("Hy-MT2 runtime: {e}")))?;
    let weights = gguf_path(app, model)?.canonicalize()?;
    let server = Command::new(runtime.join("llama-server.exe"))
        .current_dir(&runtime)
        .arg("--model")
        .arg(weights)
        .args([
            "--host",
            &address.ip().to_string(),
            "--port",
            &address.port().to_string(),
            "--api-key",
            &key,
            "--device",
            "CUDA0",
            "-ngl",
            "99",
            "-c",
            if parallel { "2048" } else { "1024" },
            "-np",
            if parallel { "2" } else { "1" },
            "--no-webui",
            "--no-warmup",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| AppError::Config(format!("Hy-MT2 server: {e}")))?;
    Ok((OwnedChild(server), format!("http://{address}"), key))
}
struct Translator {
    _process: OwnedChild,
    hy_server: Option<OwnedChild>,
    hud_worker: Option<Box<Translator>>,
    hud_admission: Option<Receiver<()>>,
    input: ChildStdin,
    output: Receiver<Result<Value>>,
    preview_in_flight: bool,
    early_zones: bool,
    hud_batch: bool,
}
impl Translator {
    fn start(parallel: bool) -> Result<Self> {
        let path = PathBuf::from(setting("SUB_WORKER")?);
        let python = setting("SUB_PYTHON")?;
        let mut command = Command::new(&python);
        command.arg("-u");
        let mut hud = None;
        let model = env::var("SUB_MODEL").unwrap_or_default();
        let hy_server = if model == "hy" || model.starts_with(CUSTOM_PREFIX) {
            let app = path
                .parent()
                .ok_or_else(|| AppError::Config("Missing app directory".into()))?;
            let (server, url, key) = start_hy(app, &model, parallel)?;
            if parallel {
                let mut side = Command::new(&python);
                side.arg("-u")
                    .arg(app.join("hy_worker.py"))
                    .env("SUB_HY_URL", &url)
                    .env("SUB_HY_KEY", &key);
                hud = Some(Box::new(Self::connect(side, None)?));
            }
            command
                .arg(app.join("hy_worker.py"))
                .env("SUB_HY_URL", url)
                .env("SUB_HY_KEY", key);
            Some(server)
        } else {
            command.arg(path);
            None
        };
        let mut main = Self::connect(command, hy_server)?;
        main.hud_worker = hud;
        Ok(main)
    }
    fn connect(mut command: Command, hy_server: Option<OwnedChild>) -> Result<Self> {
        let mut p = OwnedChild(
            command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()?,
        );
        let input = pipe(p.0.stdin.take())?;
        let stream = BufReader::new(pipe(p.0.stdout.take())?);
        let (send, output) = mpsc::sync_channel(1);
        thread::spawn(move || {
            for line in stream.lines() {
                let response = line
                    .map_err(AppError::from)
                    .and_then(|s| Ok(serde_json::from_str(&s)?));
                let failed = response.is_err();
                if send.send(response).is_err() || failed {
                    break;
                }
            }
        });
        let early_zones = hy_server.is_some() && env::var("SUB_EARLY").as_deref() != Ok("0");
        let mut worker = Self {
            _process: p,
            hy_server,
            hud_worker: None,
            hud_admission: None,
            input,
            output,
            preview_in_flight: false,
            early_zones,
            hud_batch: false,
        };
        let meta = worker.line()?;
        worker.hud_batch =
            meta["hud_batch"] == true && env::var("SUB_HUD_BATCH").as_deref() != Ok("0");
        eprintln!("Worker {meta}");
        Ok(worker)
    }
    fn line(&mut self) -> Result<Value> {
        self.output
            .recv()
            .map_err(|_| AppError::Config("Translation worker stopped".into()))?
    }
    fn translate(&mut self, s: &str) -> Result<String> {
        self.request(&json!(s))
    }
    fn request(&mut self, message: &Value) -> Result<String> {
        self.exchange(message)?
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| AppError::Config("Empty translation".into()))
    }
    fn exchange(&mut self, message: &Value) -> Result<Value> {
        self.exchange_progress(message, |_| Ok(()))
    }
    fn exchange_progress(
        &mut self,
        message: &Value,
        mut progress: impl FnMut(&Value) -> Result<()>,
    ) -> Result<Value> {
        if let Some(admission) = self.hud_admission.take() {
            // Dialogue releases this frame when it has published its updates.
            // Long speech must not monopolize both inference slots.
            const MAX_DIALOGUE_HEAD_START: Duration = Duration::from_millis(200);
            let _ = admission.recv_timeout(MAX_DIALOGUE_HEAD_START);
        }
        if self.preview_in_flight {
            if message["accept_preview"] == true {
                self.read_progress(message, &mut progress)?;
            } else {
                self.read_progress(message, &mut |_| Ok(()))?;
            }
            self.preview_in_flight = false;
        }
        self.send(message)?;
        self.read_progress(message, &mut progress)
    }
    fn read_progress(
        &mut self,
        message: &Value,
        progress: &mut impl FnMut(&Value) -> Result<()>,
    ) -> Result<Value> {
        loop {
            let value = self.line()?;
            if value.get("sentence").is_some() {
                if message["stream_sentence"] == true
                    && ["zone", "text", "previous"]
                        .iter()
                        .all(|key| value[key] == message[key])
                {
                    progress(&value)?;
                }
            } else if value.get("row").is_some() {
                progress(&value)?;
            } else {
                return Ok(value);
            }
        }
    }
    fn start_zone_preview(&mut self, message: &Value) -> Result<bool> {
        if self.preview_in_flight {
            loop {
                match self.output.try_recv() {
                    Ok(reply) => {
                        if reply?.get("sentence").is_none() {
                            break;
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => return Ok(false),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err(AppError::Config("Translation worker stopped".into()))
                    }
                }
            }
        }
        self.send(message)?;
        self.preview_in_flight = true;
        Ok(true)
    }
    fn send(&mut self, message: &Value) -> Result<()> {
        serde_json::to_writer(&mut self.input, message)?;
        writeln!(self.input)?;
        self.input.flush()?;
        Ok(())
    }
}
fn engine() -> Result<OcrEngine> {
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED)?;
    }
    Ok(OcrEngine::TryCreateFromLanguage(
        &Language::CreateLanguage(&HSTRING::from("en-US"))?,
    )?)
}
fn recognize_result(engine: &OcrEngine, pixels: &[u8]) -> Result<OcrResult> {
    recognize_tile(engine, pixels, HEIGHT)
}
fn recognize_tile(engine: &OcrEngine, pixels: &[u8], height: i32) -> Result<OcrResult> {
    let buf = CryptographicBuffer::CreateFromByteArray(pixels)?;
    let bmp = SoftwareBitmap::CreateCopyFromBuffer(&buf, BitmapPixelFormat::Gray8, WIDTH, height)?;
    Ok(engine.RecognizeAsync(&bmp)?.join()?)
}
fn recognize(engine: &OcrEngine, pixels: &[u8]) -> Result<String> {
    recognized_text(&recognize_result(engine, pixels)?)
}
fn recognized_text(result: &OcrResult) -> Result<String> {
    Ok(result
        .Text()?
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" "))
}

// Small isolated words can fall below the OCR detector's size threshold.
// Retry a compact bright region only after the ordinary passes returned no text.
fn short_line_bitmap(pixels: &[u8]) -> Option<Vec<u8>> {
    const STROKE: u8 = 96;
    const MAX_WIDTH: usize = 180;
    const MAX_HEIGHT: usize = 56;
    const MARGIN: usize = 12;
    let stride = WIDTH as usize;
    let (mut left, mut right, mut top, mut bottom) = (stride, 0, HEIGHT as usize, 0);
    for (y, row) in pixels.chunks_exact(stride).enumerate() {
        if let Some(first) = row.iter().position(|&p| p > STROKE) {
            left = left.min(first);
            right = right.max(row.iter().rposition(|&p| p > STROKE)?);
            top = top.min(y);
            bottom = y;
            if right - left >= MAX_WIDTH || bottom - top >= MAX_HEIGHT {
                return None;
            }
        }
    }
    if right <= left || bottom <= top || bottom - top < 4 {
        return None;
    }
    let mut enlarged = vec![0; FRAME_BYTES];
    for y in top..=bottom {
        let start = (MARGIN + (y - top) * 2) * stride + MARGIN;
        for (i, &value) in pixels[y * stride + left..=y * stride + right]
            .iter()
            .enumerate()
        {
            enlarged[start + i * 2..start + i * 2 + 2].fill(value);
        }
        enlarged.copy_within(start..start + 2 * (right - left + 1), start + stride);
    }
    Some(enlarged)
}
fn retry_short(engine: &OcrEngine, pixels: &[u8]) -> Result<String> {
    match short_line_bitmap(pixels) {
        Some(enlarged) => recognize(engine, &enlarged),
        None => Ok(String::new()),
    }
}
struct Overlay {
    dir: PathBuf,
    session: u128,
    seq: u64,
}
impl Overlay {
    fn new() -> Result<Self> {
        let dir = PathBuf::from(setting("SUB_RUNTIME")?);
        fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            session: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| AppError::Config(e.to_string()))?
                .as_nanos(),
            seq: 0,
        })
    }
    fn publish(&mut self, text: &str) -> Result<String> {
        self.seq += 1;
        let id = format!("{}-{}", self.session, self.seq);
        let tmp = self.dir.join(format!("{}.tmp", self.session));
        let dst = self.dir.join("caption.txt");
        fs::write(&tmp, format!("{id}\n{text}"))?;
        let from = HSTRING::from(tmp.as_os_str());
        let to = HSTRING::from(dst.as_os_str());
        for attempt in 0..5 {
            match unsafe {
                MoveFileExW(
                    PCWSTR(from.as_ptr()),
                    PCWSTR(to.as_ptr()),
                    MOVEFILE_REPLACE_EXISTING,
                )
            } {
                Ok(()) => return Ok(id),
                Err(err) if attempt == 4 => return Err(err.into()),
                Err(_) => thread::sleep(POLL),
            }
        }
        Err(AppError::Config("Could not publish".into()))
    }
    fn acknowledgement(&self, id: &str) -> bool {
        let start = Instant::now();
        while start.elapsed() < ACK_LIMIT {
            if fs::read_to_string(self.dir.join("ack.txt")).is_ok_and(|v| v == id) {
                return true;
            }
            thread::sleep(POLL);
        }
        false
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Draft,
    Confirmed,
}
struct Line {
    upper_end: Option<usize>,
    stage: Stage,
    context: String,
    start: Instant,
    text: String,
    ocr_ms: f64,
}
fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}
fn capture(
    tx: mpsc::Sender<Result<Line>>,
    speculate: bool,
    mut history: DialogueHistory,
) -> Result<()> {
    let ocr = engine()?;
    let device = setting("SUB_DEVICE")?;
    let roi = setting("SUB_CROP")?;
    let dims: Vec<u32> = roi
        .split(':')
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| AppError::Config("Crop is width:height:x:y".into()))?;
    if dims.len() != 4 || dims[0] == 0 || dims[1] == 0 {
        return Err(AppError::Config("Invalid crop".into()));
    }
    // Sample before scaling. Keep the original for dim or coloured text.
    // Expand the bright red channel: white strokes stand out against blue snow.
    let filter = format!(
        r"fps={},crop={roi},split[raw][color];[raw]scale={WIDTH}:{HEIGHT},format=gray[gray];[color]format=gbrp,extractplanes=r,lut=y=clip((val-165)*5\,0\,255),scale={WIDTH}:{HEIGHT},format=gray[bright];[gray][bright]vstack",
        capture_fps()
    );
    let mut child = OwnedChild(
        Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-loglevel",
                "error",
                "-rtbufsize",
                "48M",
                "-f",
                "dshow",
                "-i",
                &format!("video={device}"),
                "-vf",
                &filter,
                "-threads:v",
                "1",
                "-flush_packets",
                "1",
                "-fps_mode",
                "passthrough",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .spawn()?,
    );
    let mut frames = pipe(child.0.stdout.take())?;
    let mut pixels = vec![0; FRAME_BYTES * 2];
    let mut last_frame = Vec::new();
    let mut cleaned = vec![0; FRAME_BYTES];
    let mut reading = String::new();
    let mut upper_end = None;
    let mut held = false;
    let mut stable = Stabilizer::default();
    loop {
        frames.read_exact(&mut pixels)?;
        let start = Instant::now();
        let (gray, bright) = pixels.split_at(FRAME_BYTES);
        // Reuse OCR on a held frame, but still advance confirmation and expiry.
        let unchanged = pixels == last_frame;
        if !unchanged || !held {
            let previous = last_frame.get(FRAME_BYTES..).unwrap_or(bright);
            for ((out, &current), &prior) in cleaned.iter_mut().zip(bright).zip(previous) {
                *out = current.min(prior);
            }
            (reading, upper_end) = dialogue_ocr::read(&ocr, &cleaned, stable.shown_text())?;
            if reading.trim().is_empty() && !unchanged {
                (reading, upper_end) = dialogue_ocr::read(&ocr, bright, stable.shown_text())?;
            }
            if reading.trim().is_empty() {
                (reading, upper_end) = dialogue_ocr::read(&ocr, gray, stable.shown_text())?;
            } else {
                let (text, boundary) = dialogue_ocr::recover(&ocr, reading, gray, &cleaned)?;
                reading = text;
                upper_end = boundary.or(upper_end);
            }
            // The high-contrast mask can split a letter in a short word. On
            // isolated text prefer the original strokes, then the enlarged crop.
            if reading.len() <= 8 && !reading.contains(' ') {
                if let Some(enlarged) = short_line_bitmap(gray) {
                    let original = if reading.is_empty() {
                        String::new() // The normal gray pass just returned empty.
                    } else {
                        recognize(&ocr, gray)?
                    };
                    let candidate = if original.is_empty() {
                        recognize(&ocr, &enlarged)?
                    } else {
                        original
                    };
                    if !candidate.is_empty() {
                        reading = candidate;
                        upper_end = None;
                    }
                }
                if reading.is_empty() {
                    reading = retry_short(&ocr, bright)?;
                }
            }
            last_frame.clone_from(&pixels);
            held = unchanged;
        }
        let event = match stable.observe(&reading, upper_end, start) {
            Observation::Held => {
                history.visible(start);
                None
            }
            Observation::Revised(text) => Some((Stage::Confirmed, Card::Revised, text)),
            Observation::Confirmed(text) => Some((Stage::Confirmed, Card::New, text)),
            Observation::Appended(text, end) => Some((Stage::Confirmed, Card::Appended(end), text)),
            Observation::Cleared => Some((Stage::Confirmed, Card::New, String::new())),
            Observation::Pending if speculate => stable
                .first_candidate()
                .map(|s| (Stage::Draft, Card::Draft, s.to_owned())),
            Observation::Pending => None,
        };
        let append = match &event {
            Some((_, Card::Appended(end), _)) => Some(*end),
            Some((Stage::Draft, _, _)) => upper_end,
            _ => None,
        };
        let ready = match (event, append) {
            (Some((Stage::Draft, _, raw)), Some(end)) if raw.get(..end) == stable.shown_text() => {
                let context = history.append_context(&raw, end, start);
                Some((Stage::Draft, (raw, context)))
            }
            (Some((stage, kind, raw)), _) => {
                history.present(raw, start, kind).map(|pair| (stage, pair))
            }
            (None, _) => history
                .due(&reading, start)
                .map(|pair| (Stage::Confirmed, pair)),
        };
        if let Some((stage, (text, context))) = ready {
            let line = Line {
                upper_end: append,
                stage,
                context,
                start,
                text,
                ocr_ms: ms(start),
            };
            if tx.send(Ok(line)).is_err() {
                return Ok(());
            }
        }
    }
}

fn live(worker: &mut Translator, out: &mut Overlay) -> Result<()> {
    let check_obs = env::var("SUB_ACK").is_ok_and(|v| v == "1");
    let (tx, rx) = mpsc::channel();
    let speculate = worker.hy_server.is_some() && env::var("SUB_EARLY").as_deref() != Ok("0");
    let history = DialogueHistory::default();
    thread::spawn(move || {
        if let Err(e) = capture(tx.clone(), speculate, history) {
            let _ = tx.send(Err(e));
        }
    });
    if worker.hy_server.is_some() {
        return live_context(worker, out, rx, check_obs);
    }
    let mut pending = None;
    let mut shown = ShownCaption::default();
    loop {
        let line = match pending.take() {
            Some(v) => v,
            None => rx
                .recv()
                .map_err(|_| AppError::Config("Capture ended".into()))??,
        };
        let translate_start = Instant::now();
        let target = shown.target(&line.text, line.upper_end);
        let translated = if target.is_empty() {
            String::new()
        } else {
            worker.translate(target)?
        };
        if let Some(newer) = latest(&rx)? {
            pending = Some(newer);
            continue;
        }
        let translation_ms = ms(translate_start);
        let publish_start = Instant::now();
        let text = shown.assemble(target, &line.text, translated);
        let id = out.publish(&text)?;
        let write_ms = ms(publish_start);
        let publish_ms = ms(line.start);
        let obs_tick_ms = (check_obs && out.acknowledgement(&id)).then(|| ms(line.start));
        println!(
            "{}",
            json!({"en":line.text,"uk":text,"ocr_ms":line.ocr_ms,"translation_ms":translation_ms,"write_ms":write_ms,"publish_ms":publish_ms,"obs_tick_ms":obs_tick_ms})
        );
        shown.remember(line.text, text);
    }
}
// A draft may use idle GPU time, but only the stabilizer can authorize publication.
fn live_context(
    worker: &mut Translator,
    out: &mut Overlay,
    rx: Receiver<Result<Line>>,
    check_obs: bool,
) -> Result<()> {
    use std::collections::VecDeque;
    let mut queued: VecDeque<Line> = VecDeque::new();
    let mut draft: Option<(String, String, String)> = None;
    let mut shown = ShownCaption::default();
    loop {
        let line = match queued.pop_front() {
            Some(line) => line,
            None => rx
                .recv()
                .map_err(|_| AppError::Config("Capture ended".into()))??,
        };
        let context = line.context;
        let start = Instant::now();
        let target = shown.target(&line.text, line.upper_end);
        let cached = draft
            .take()
            .filter(|(en, prior, _)| en == target && prior == &context);
        let reused = cached.is_some();
        let text = match cached {
            Some((_, _, translation)) => translation,
            None if line.text.is_empty() => String::new(),
            None if line.stage == Stage::Draft => {
                let reply =
                    worker.exchange(&json!({"text":target,"previous":context,"draft":true}))?;
                if reply.is_null() {
                    continue;
                }
                serde_json::from_value(reply)?
            }
            None => worker.request(&json!({"text":target,"previous":context}))?,
        };
        let translation_ms = ms(start);
        // Coalesce obsolete work, preserving the newest confirmation and any draft after it.
        for item in rx.try_iter() {
            let next = item?;
            if next.stage == Stage::Confirmed {
                queued.clear();
            } else if queued.back().is_some_and(|v| v.stage == Stage::Draft) {
                queued.pop_back();
            }
            queued.push_back(next);
        }
        if line.stage == Stage::Draft {
            draft = Some((target.to_owned(), context, text));
            continue;
        }
        if queued.iter().any(|next| next.stage == Stage::Confirmed) {
            continue;
        }
        let write = Instant::now();
        let text = shown.assemble(target, &line.text, text);
        let id = out.publish(&text)?;
        let write_ms = ms(write);
        let publish_ms = ms(line.start);
        let ack = (check_obs && out.acknowledgement(&id)).then(|| ms(line.start));
        println!(
            "{}",
            json!({"en":line.text,"uk":text,"ocr_ms":line.ocr_ms,"translation_ms":translation_ms,
            "reused_draft":reused,"context_lines":context.lines().count(),"write_ms":write_ms,"publish_ms":publish_ms,"obs_tick_ms":ack})
        );
        shown.remember(line.text, text);
    }
}
fn latest(rx: &Receiver<Result<Line>>) -> Result<Option<Line>> {
    rx.try_iter().last().transpose()
}
fn bench(root: &Path, worker: &mut Translator, out: &mut Overlay) -> Result<()> {
    let ocr = engine()?;
    let cases = fs::read_to_string(root.join("lines.txt"))?;
    let mut history = DialogueHistory::default();
    let check_obs = env::var("SUB_ACK").is_ok_and(|s| s == "1");
    for (i, expected) in cases.lines().enumerate() {
        let pixels = fs::read(root.join(format!("{i}.gray")))?;
        if pixels.len() != FRAME_BYTES {
            return Err(AppError::Config("Fixture size mismatch".into()));
        }
        let start = Instant::now();
        let mut en = recognize(&ocr, &pixels)?;
        if en.is_empty() {
            en = retry_short(&ocr, &pixels)?;
        }
        let ocr_ms = ms(start);
        let tr = Instant::now();
        let (source, context) = history.card(en.to_owned(), start, Card::New);
        let uk = if source.is_empty() {
            String::new()
        } else if worker.hy_server.is_some() {
            worker.request(&json!({"text":source,"previous":context}))?
        } else {
            worker.translate(&source)?
        };
        let translation_ms = ms(tr);
        let id = out.publish(&uk)?;
        let publish_ms = ms(start);
        let ack = if check_obs && out.acknowledgement(&id) {
            Some(ms(start))
        } else {
            None
        };
        println!(
            "{}",
            json!({"case":i,"expected":expected,"en":en,"source":source,"context":context,"uk":uk,"ocr_ms":ocr_ms,"translation_ms":translation_ms,"publish_ms":publish_ms,"obs_tick_ms":ack})
        );
    }
    Ok(())
}
const DEMO_HOLD: Duration = Duration::from_secs(4);

fn demo(worker: &mut Translator, out: &mut Overlay) -> Result<()> {
    let dir = PathBuf::from(setting("SUB_DEMO")?);
    let pixels = fs::read(dir.join("0.gray"))?;
    let expected = fs::read_to_string(dir.join("lines.txt"))?;
    if pixels.len() != FRAME_BYTES {
        return Err(AppError::Config("DEMO_FRAME".into()));
    }
    let en = recognize(&engine()?, &pixels)?;
    if en != expected.trim() {
        return Err(AppError::Config("DEMO_OCR".into()));
    }
    let uk = worker.translate(&en)?;
    let id = out.publish(&uk)?;
    let acknowledged = out.acknowledgement(&id);
    println!("{}", json!({"en": en, "uk": uk, "obs": acknowledged}));
    thread::sleep(DEMO_HOLD);
    Ok(())
}
fn run() -> Result<()> {
    let mut args = env::args().skip(1);
    let mode = args.next();
    let mut out = Overlay::new()?;
    let parallel = mode.as_deref() == Some("zones")
        && env::var("SUB_PARALLEL_ZONES").as_deref() == Ok("1")
        && zones::mixed_layout()?;
    let mut worker = Translator::start(parallel)?;
    out.publish("")?;
    let result = match mode.as_deref() {
        Some("bench") => bench(
            Path::new(
                &args
                    .next()
                    .ok_or_else(|| AppError::Config("bench needs a fixture directory".into()))?,
            ),
            &mut worker,
            &mut out,
        ),
        Some("demo") => demo(&mut worker, &mut out),
        Some("live") => live(&mut worker, &mut out),
        Some("zones") => zones::run(&mut worker, args.next(), zones::Mode::Live),
        Some("zones-test") => return zones::run(&mut worker, args.next(), zones::Mode::Snapshot),
        _ => Err(AppError::Config("Use live or bench DIR".into())),
    };
    let _ = out.publish("");
    result
}
fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1)
    }
}
