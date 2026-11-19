use super::*;
use std::fmt::Write as _;

const CANVAS_W: u32 = 1280;
const CANVAS_H: u32 = 720;
const MAX_ZONES: usize = 8;

struct Zone {
    crop: [u32; 4],
    hud: bool,
    stable: Stabilizer,
    history: DialogueHistory,
    previous: Vec<u8>,
    reading: String,
}
impl Zone {
    fn load(row: &Value) -> Result<Self> {
        let invalid = || AppError::Config("Invalid zone rectangle".into());
        let number = |key: &str| {
            row[key]
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(invalid)
        };
        let (x, y, w, h) = (number("Left")?, number("Top")?, number("W")?, number("H")?);
        if w < 8
            || h < 8
            || x.checked_add(w).is_none_or(|n| n > CANVAS_W)
            || y.checked_add(h).is_none_or(|n| n > CANVAS_H)
        {
            return Err(invalid());
        }
        Ok(Self {
            crop: [w, h, x, y],
            hud: row["Hud"].as_bool().unwrap_or_default(),
            stable: Stabilizer::default(),
            history: DialogueHistory::default(),
            previous: Vec::new(),
            reading: String::new(),
        })
    }
    fn observe(&mut self, ocr: &OcrEngine, pixels: &[u8], now: Instant) -> Result<Option<String>> {
        if self.previous != pixels {
            let prior = if self.previous.len() == pixels.len() {
                self.previous.as_slice()
            } else {
                pixels
            };
            let high: Vec<u8> = pixels
                .iter()
                .zip(prior)
                .map(|(&current, &previous)| {
                    current.min(previous).saturating_sub(130).saturating_mul(4)
                })
                .collect();
            self.reading = recognize(ocr, &high)?;
            if self.reading.trim().is_empty() {
                self.reading = recognize(ocr, pixels)?;
            }
            if self.reading.trim().is_empty() {
                self.reading = retry_short(ocr, &high)?;
            }
            self.previous.clear();
            self.previous.extend_from_slice(pixels);
        }
        match self.stable.observe(&self.reading, now) {
            Observation::Confirmed(text) => Ok(Some(text)),
            Observation::Revised(text) => {
                self.history.discard_last_version();
                Ok(Some(text))
            }
            Observation::Cleared => Ok(Some(String::new())),
            Observation::Held => {
                self.history.visible(now);
                Ok(None)
            }
            Observation::Pending => Ok(None),
        }
    }
}
fn filter(zones: &[Zone]) -> String {
    let mut graph = format!(
        "fps=10,scale={CANVAS_W}:{CANVAS_H},format=gray,split={}",
        zones.len()
    );
    for i in 0..zones.len() {
        let _ = write!(graph, "[r{i}]");
    }
    for (i, zone) in zones.iter().enumerate() {
        let [w, h, x, y] = zone.crop;
        let _=write!(graph,";[r{i}]crop={w}:{h}:{x}:{y},scale={WIDTH}:{HEIGHT}:force_original_aspect_ratio=decrease,pad={WIDTH}:{HEIGHT}:0:0,setsar=1[o{i}]");
    }
    graph.push(';');
    for i in 0..zones.len() {
        let _ = write!(graph, "[o{i}]");
    }
    if zones.len() == 1 {
        graph.push_str("null");
    } else {
        let _ = write!(graph, "vstack=inputs={}", zones.len());
    }
    graph
}
pub(super) fn run(worker: &mut Translator, recording: Option<String>) -> Result<()> {
    let path = PathBuf::from(setting("SUB_RUNTIME")?).join("zones.json");
    let config: Value = serde_json::from_slice(&fs::read(path)?)?;
    let rows = config
        .as_array()
        .filter(|a| (1..=MAX_ZONES).contains(&a.len()))
        .ok_or_else(|| AppError::Config("Choose 1–8 zones".into()))?;
    let mut zones = rows.iter().map(Zone::load).collect::<Result<Vec<_>>>()?;
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-nostdin",
        "-hide_banner",
        "-loglevel",
        "error",
        "-filter_complex_threads",
        "1",
    ]);
    if let Some(file) = &recording {
        cmd.args(["-re", "-i", file]);
    } else {
        cmd.args([
            "-rtbufsize",
            "48M",
            "-f",
            "dshow",
            "-i",
            &format!("video={}", setting("SUB_DEVICE")?),
        ]);
    }
    let mut capture = OwnedChild(
        cmd.args([
            "-filter_complex",
            &filter(&zones),
            "-an",
            "-threads:v",
            "1",
            "-pix_fmt",
            "gray",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .spawn()?,
    );
    let mut pipe = pipe(capture.0.stdout.take())?;
    let (send, frames) = mpsc::sync_channel(1);
    let bytes = FRAME_BYTES * zones.len();
    // Drain the device even while inference is busy. Each message contains every ROI from one frame.
    thread::spawn(move || {
        let mut buffer = vec![0; bytes];
        loop {
            if let Err(e) = pipe.read_exact(&mut buffer) {
                if e.kind() != io::ErrorKind::UnexpectedEof {
                    let _ = send.send(Err(e));
                }
                break;
            }
            match send.try_send(Ok(buffer)) {
                Ok(()) => buffer = vec![0; bytes],
                Err(mpsc::TrySendError::Full(Ok(reuse))) => buffer = reuse,
                Err(_) => break,
            }
        }
    });
    let ocr = engine()?;
    let mut captions = vec![String::new(); zones.len()];
    for frame in frames {
        let frame = frame?;
        let now = Instant::now();
        for (i, (zone, pixels)) in zones
            .iter_mut()
            .zip(frame.as_chunks::<FRAME_BYTES>().0)
            .enumerate()
        {
            let Some(en) = zone.observe(&ocr, pixels, now)? else {
                continue;
            };
            let previous = zone.history.previous(now);
            let uk = if en.is_empty() {
                String::new()
            } else {
                worker.request(&json!({"zone":i,"text":en,"previous":previous,"hud":zone.hud}))?
            };
            zone.history.confirm(&en, now);
            captions[i].clone_from(&uk);
            println!("{}", json!({"zone":i,"en":en,"uk":uk,"captions":captions}));
        }
    }
    let status = capture.0.wait()?;
    if !status.success() || recording.is_none() {
        return Err(AppError::Config(format!("Capture ended ({status})")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crops_share_one_frame_and_keep_editor_order() -> Result<()> {
        let rows = [
            json!({"Left":515,"Top":4,"W":254,"H":36}),
            json!({"Left":923,"Top":220,"W":350,"H":105}),
        ];
        let zones = rows.iter().map(Zone::load).collect::<Result<Vec<_>>>()?;
        let graph = filter(&zones);
        assert!(graph.contains("split=2[r0][r1]"));
        assert!(graph.contains("[r0]crop=254:36:515:4"));
        assert!(graph.contains("[r1]crop=350:105:923:220"));
        assert!(graph.ends_with("[o0][o1]vstack=inputs=2"));
        assert!(filter(&zones[..1]).ends_with("[o0]null"));
        Ok(())
    }
    #[test]
    fn rejects_invalid_saved_rectangles() {
        for row in [
            json!({"Left":900,"Top":0,"W":400,"H":80}),
            json!({"Left":0,"Top":-1,"W":200,"H":40}),
            json!({"Left":0,"Top":0,"W":0,"H":30}),
            json!({"Left":0,"Top":0,"W":"200","H":20}),
        ] {
            assert!(Zone::load(&row).is_err());
        }
    }
}
