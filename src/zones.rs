use super::*;
use std::fmt::Write as _;

const CANVAS_W: u32 = 1280;
const CANVAS_H: u32 = 720;
const MAX_ZONES: usize = 8;

struct Zone {
    crop: [u32; 4],
    hud: bool,
    legend: Option<legend::Legend>,
    stable: Stabilizer,
    history: DialogueHistory,
    shown: ShownCaption,
    previous: Vec<u8>,
    recheck_mask: bool,
    reading: String,
    upper_end: Option<usize>,
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
            legend: row["Legend"]
                .as_bool()
                .unwrap_or_default()
                .then(legend::Legend::default),
            stable: Stabilizer::default(),
            history: DialogueHistory::default(),
            shown: ShownCaption::default(),
            previous: Vec::new(),
            recheck_mask: false,
            reading: String::new(),
            upper_end: None,
        })
    }
    fn scaled(&self) -> [i32; 2] {
        let [w, h, _, _] = self.crop;
        let max_height = if self.legend.is_some() { 1080 } else { HEIGHT };
        let ratio = (f64::from(WIDTH) / f64::from(w)).min(f64::from(max_height) / f64::from(h));
        let ratio = if self.legend.is_some() {
            ratio.min(3.)
        } else {
            ratio
        };
        [
            (f64::from(w) * ratio).round() as i32,
            (f64::from(h) * ratio).round() as i32,
        ]
    }
    fn tile_height(&self) -> i32 {
        if self.legend.is_some() {
            self.scaled()[1]
        } else {
            HEIGHT
        }
    }
    fn observe(
        &mut self,
        ocr: &OcrEngine,
        pixels: &[u8],
        now: Instant,
    ) -> Result<Option<(String, Card)>> {
        let changed = self.previous != pixels;
        if changed || self.recheck_mask {
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
            let shown = self.stable.shown_text().filter(|_| !self.hud);
            (self.reading, self.upper_end) = dialogue_ocr::read(ocr, &high, shown)?;
            // On a hard cut the intersection of two different captions may itself
            // read as text. A held frame must then be read with its settled mask
            // before its cached OCR can count as a second observation.
            self.recheck_mask = changed
                && !self.reading.trim().is_empty()
                && pixels
                    .iter()
                    .zip(&high)
                    .any(|(&raw, &masked)| raw.saturating_sub(130).saturating_mul(4) != masked);
            if self.reading.trim().is_empty() {
                (self.reading, self.upper_end) = dialogue_ocr::read(ocr, pixels, shown)?;
            } else if !self.hud {
                let (text, boundary) =
                    dialogue_ocr::recover(ocr, std::mem::take(&mut self.reading), pixels, &high)?;
                self.reading = text;
                self.upper_end = boundary.or(self.upper_end);
                if boundary.is_some() {
                    self.recheck_mask = false;
                }
            }
            if self.reading.trim().is_empty() {
                self.reading = retry_short(ocr, &high)?;
            }
            if changed {
                self.previous.clear();
                self.previous.extend_from_slice(pixels);
            }
        }
        match self.stable.observe(&self.reading, self.upper_end, now) {
            Observation::Confirmed(text) => Ok(Some((text, Card::New))),
            Observation::Appended(text, end) => Ok(Some((text, Card::Appended(end)))),
            Observation::Revised(text) => Ok(Some((text, Card::Revised))),
            Observation::Cleared => Ok(Some((String::new(), Card::New))),
            Observation::Held => {
                self.history.visible(now);
                Ok(None)
            }
            Observation::Pending => Ok(None),
        }
    }
}
fn filter(zones: &[Zone]) -> String {
    let mut graph = format!("fps={},split={}", capture_fps(), zones.len());
    for i in 0..zones.len() {
        let _ = write!(graph, "[r{i}]");
    }
    for (i, zone) in zones.iter().enumerate() {
        let [w, h, x, y] = zone.crop;
        // Round both edges on the input grid so adjacent editor rectangles meet.
        let left = format!("round(iw*{x}/{CANVAS_W})");
        let top = format!("round(ih*{y}/{CANVAS_H})");
        let right = format!("round(iw*{}/{CANVAS_W})", x + w);
        let bottom = format!("round(ih*{}/{CANVAS_H})", y + h);
        // The editor stretches its preview to 1280x720 even for non-16:9 input.
        let [scaled_w, scaled_h] = zone.scaled();
        let height = zone.tile_height();
        let _ = write!(
            graph,
            ";[r{i}]crop=w='max(1,{right}-{left})':h='max(1,{bottom}-{top})':x={left}:y={top}:exact=1,format=gray,scale={scaled_w}:{scaled_h},pad={WIDTH}:{height}:0:0,setsar=1[o{i}]"
        );
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
    let bytes = zones
        .iter()
        .map(|z| (WIDTH * z.tile_height()) as usize)
        .sum();
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
    let mut captions: Vec<_> = zones
        .iter()
        .map(|z| {
            if z.legend.is_some() {
                json!({"rows":[],"bands":[]})
            } else {
                json!("")
            }
        })
        .collect();
    let mut offset = 0;
    let mut order: Vec<_> = zones
        .iter()
        .enumerate()
        .map(|(i, z)| {
            let start = offset;
            offset += (WIDTH * z.tile_height()) as usize;
            (i, start..offset)
        })
        .collect();
    order.sort_by_key(|(i, _)| zones[*i].legend.is_some());
    for frame in frames {
        let frame = frame?;
        let now = Instant::now();
        for &(i, ref span) in &order {
            let zone = &mut zones[i];
            let pixels = &frame[span.start..span.end];
            let scaled = zone.scaled();
            if let Some(legend) = zone.legend.as_mut() {
                if let Some(snapshot) =
                    legend.observe(&ocr, pixels, scaled, zone.crop, worker, i)?
                {
                    captions[i] = snapshot;
                    println!("{}", json!({"zone":i,"captions":captions}));
                }
                continue;
            }
            let observed = zone.observe(&ocr, pixels, now)?;
            let upper_end = match &observed {
                Some((_, Card::Appended(end))) => Some(*end),
                _ => None,
            };
            let ready = match observed {
                Some((raw, _)) if zone.hud => Some((raw, String::new())),
                Some((raw, kind)) => zone.history.present(raw, now, kind),
                None => zone.history.due(&zone.reading, now),
            };
            let Some((en, previous)) = ready else {
                continue;
            };
            let target = zone.shown.target(&en, upper_end);
            let translated = if target.is_empty() {
                String::new()
            } else {
                worker
                    .request(&json!({"zone":i,"text":target,"previous":previous,"hud":zone.hud}))?
            };
            let uk = zone.shown.assemble(target, &en, translated);
            captions[i] = json!(uk);
            println!("{}", json!({"zone":i,"en":en,"uk":uk,"captions":captions}));
            zone.shown.remember(en, uk);
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
        assert!(graph.starts_with(&format!("fps={},split=2", capture_fps())));
        assert!(graph.contains("round(iw*769/1280)-round(iw*515/1280)"));
        assert!(graph.contains("round(ih*325/720)-round(ih*220/720)"));
        assert!(graph.contains("exact=1,format=gray,scale=480:144"));
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
