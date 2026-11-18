mod stability;
use serde_json::{json, Value};
use stability::Stabilizer;
use std::{
    env, fs,
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use windows::{
    core::{HSTRING, PCWSTR},
    Globalization::Language,
    Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap},
    Media::Ocr::OcrEngine,
    Security::Cryptography::CryptographicBuffer,
    Win32::{
        Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING},
        System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED},
    },
};
const WIDTH: i32 = 960;
const HEIGHT: i32 = 144;
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
struct Translator {
    _process: OwnedChild,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}
impl Translator {
    fn start() -> Result<Self> {
        let mut p = OwnedChild(
            Command::new(setting("SUB_PYTHON")?)
                .arg("-u")
                .arg(setting("SUB_WORKER")?)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()?,
        );
        let input = pipe(p.0.stdin.take())?;
        let output = BufReader::new(pipe(p.0.stdout.take())?);
        let mut worker = Self {
            _process: p,
            input,
            output,
        };
        let meta = worker.line()?;
        eprintln!("Worker {meta}");
        Ok(worker)
    }
    fn line(&mut self) -> Result<Value> {
        let mut s = String::new();
        if self.output.read_line(&mut s)? == 0 {
            return Err(AppError::Config("Translation worker stopped".into()));
        }
        Ok(serde_json::from_str(&s)?)
    }
    fn translate(&mut self, s: &str) -> Result<String> {
        serde_json::to_writer(&mut self.input, s)?;
        writeln!(self.input)?;
        self.input.flush()?;
        self.line()?
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| AppError::Config("Empty translation".into()))
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
fn recognize(engine: &OcrEngine, pixels: &[u8]) -> Result<String> {
    let buf = CryptographicBuffer::CreateFromByteArray(pixels)?;
    let bmp = SoftwareBitmap::CreateCopyFromBuffer(&buf, BitmapPixelFormat::Gray8, WIDTH, HEIGHT)?;
    Ok(engine
        .RecognizeAsync(&bmp)?
        .join()?
        .Text()?
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" "))
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
struct Line {
    start: Instant,
    text: String,
    ocr_ms: f64,
}
fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}
fn capture(tx: mpsc::Sender<Result<Line>>) -> Result<()> {
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
    // Sample at about 10 Hz before scaling. Keep the original for dim or coloured text.
    // Expand the bright red channel: white strokes stand out against blue snow.
    let filter = format!(
        r"select=isnan(prev_selected_t)+gte(t-prev_selected_t\,0.095),crop={roi},split[raw][color];[raw]scale={WIDTH}:{HEIGHT},format=gray[gray];[color]format=gbrp,extractplanes=r,lut=y=clip((val-165)*5\,0\,255),scale={WIDTH}:{HEIGHT},format=gray[bright];[gray][bright]vstack"
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
            reading = recognize(&ocr, &cleaned)?;
            if reading.trim().is_empty() && !unchanged {
                reading = recognize(&ocr, bright)?;
            }
            if reading.trim().is_empty() {
                reading = recognize(&ocr, gray)?;
            }
            last_frame.clone_from(&pixels);
            held = unchanged;
        }
        if let Some(text) = stable.observe(&reading, start) {
            let line = Line {
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
    thread::spawn(move || {
        if let Err(e) = capture(tx.clone()) {
            let _ = tx.send(Err(e));
        }
    });
    let mut pending = None;
    loop {
        let line = match pending.take() {
            Some(v) => v,
            None => rx
                .recv()
                .map_err(|_| AppError::Config("Capture ended".into()))??,
        };
        let translate_start = Instant::now();
        let text = if line.text.is_empty() {
            String::new()
        } else {
            worker.translate(&line.text)?
        };
        if let Some(newer) = latest(&rx)? {
            pending = Some(newer);
            continue;
        }
        let translation_ms = ms(translate_start);
        let publish_start = Instant::now();
        let id = out.publish(&text)?;
        let write_ms = ms(publish_start);
        let publish_ms = ms(line.start);
        let obs_tick_ms = (check_obs && out.acknowledgement(&id)).then(|| ms(line.start));
        println!(
            "{}",
            json!({"en":line.text,"uk":text,"ocr_ms":line.ocr_ms,"translation_ms":translation_ms,"write_ms":write_ms,"publish_ms":publish_ms,"obs_tick_ms":obs_tick_ms})
        );
    }
}
fn latest(rx: &Receiver<Result<Line>>) -> Result<Option<Line>> {
    rx.try_iter().last().transpose()
}
fn bench(root: &Path, worker: &mut Translator, out: &mut Overlay) -> Result<()> {
    let ocr = engine()?;
    let cases = fs::read_to_string(root.join("lines.txt"))?;
    let check_obs = env::var("SUB_ACK").is_ok_and(|s| s == "1");
    for (i, expected) in cases.lines().enumerate() {
        let pixels = fs::read(root.join(format!("{i}.gray")))?;
        if pixels.len() != FRAME_BYTES {
            return Err(AppError::Config("Fixture size mismatch".into()));
        }
        let start = Instant::now();
        let en = recognize(&ocr, &pixels)?;
        let ocr_ms = ms(start);
        let tr = Instant::now();
        let uk = if en.is_empty() {
            String::new()
        } else {
            worker.translate(&en)?
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
            json!({"case":i,"expected":expected,"en":en,"uk":uk,"ocr_ms":ocr_ms,"translation_ms":translation_ms,"publish_ms":publish_ms,"obs_tick_ms":ack})
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
    let mut worker = Translator::start()?;
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
