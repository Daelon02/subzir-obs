use super::*;
use std::fmt::Write as _;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const CANVAS_W: u32 = 1280;
const CANVAS_H: u32 = 720;
const MAX_ZONES: usize = 8;
// Persisted CaptionAnchor values. Both directions share OCR coordinates.
const UNDER_EACH: u64 = 3;
const ABOVE_EACH: u64 = 4;

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
    preview_reading: Option<String>,
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
            legend: if matches!(row["Anchor"].as_u64(), Some(UNDER_EACH | ABOVE_EACH))
                && row["Output"].is_null()
            {
                Some(legend::Legend::panel())
            } else if row["Legend"] == true {
                Some(legend::Legend::default())
            } else {
                None
            },
            stable: Stabilizer::default(),
            history: DialogueHistory::default(),
            shown: ShownCaption::default(),
            previous: Vec::new(),
            recheck_mask: false,
            reading: String::new(),
            upper_end: None,
            preview_reading: None,
        })
    }
    fn scaled(&self) -> [i32; 2] {
        let [w, h, _, _] = self.crop;
        let max_height = if self.legend.is_some() {
            1080
        } else if self.hud {
            // A whole menu panel can contain small labels separated by artwork.
            // Keep their native canvas resolution rather than shrinking the entire panel to one subtitle strip.
            HEIGHT.max(h as i32)
        } else {
            HEIGHT
        };
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
            self.scaled()[1].max(HEIGHT)
        }
    }
    fn observe(
        &mut self,
        ocr: &OcrEngine,
        pixels: &[u8],
        now: Instant,
    ) -> Result<Option<(String, Card)>> {
        let changed = self.previous != pixels;
        let tile_height = self.tile_height();
        if tile_height > HEIGHT {
            // HUD panels can mix dark text on gold and white text on black.
            if changed {
                self.reading = recognized_text(&recognize_tile(ocr, pixels, tile_height)?)?;
            }
        } else if changed || self.recheck_mask {
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
        }
        if changed {
            self.previous.clear();
            self.previous.extend_from_slice(pixels);
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

impl Zone {
    fn dialogue(&self) -> bool {
        !self.hud && self.legend.is_none()
    }
    fn update(
        &mut self,
        ocr: &OcrEngine,
        pixels: &[u8],
        worker: &mut Translator,
        i: usize,
        timing: (Mode, Instant),
        mut progress: impl FnMut(Value) -> Result<()>,
    ) -> Result<Option<Value>> {
        let (mode, now) = timing;
        let scaled = self.scaled();
        if let Some(legend) = self.legend.as_mut() {
            return legend.observe(
                ocr,
                pixels,
                (scaled, self.crop),
                worker,
                (i, mode == Mode::Live),
                &mut progress,
            );
        }
        let observed = self.observe(ocr, pixels, now)?;
        if self
            .preview_reading
            .as_ref()
            .is_some_and(|text| text != &self.reading)
        {
            self.preview_reading = None;
        }
        let upper_end = match &observed {
            Some((_, Card::Appended(end))) => Some(*end),
            _ => None,
        };
        let ready = match observed {
            Some((raw, _)) if self.hud || mode == Mode::Snapshot => Some((raw, String::new())),
            Some((raw, kind)) => self.history.present(raw, now, kind),
            None => self.history.due(&self.reading, now),
        };
        let Some((en, previous)) = ready else {
            let early = mode == Mode::Live
                && !self.hud
                && worker.early_zones
                && self.preview_reading.is_none();
            if let Some(raw) = self
                .stable
                .first_candidate()
                .filter(|s| early && *s == self.reading)
            {
                let append = self
                    .upper_end
                    .filter(|&end| raw.get(..end) == self.stable.shown_text());
                let pair = match append {
                    Some(end) => Some((raw.to_owned(), self.history.append_context(raw, end, now))),
                    None => self.history.present(raw.to_owned(), now, Card::Draft),
                };
                if let Some((source, context)) = pair {
                    let target = self.shown.target(&source, append);
                    if worker.start_zone_preview(
                        &json!({"zone":i,"text":target,"previous":context,"zone_preview":true,"stream_sentence":target==source}),
                    )? {
                        self.preview_reading = Some(raw.to_owned());
                    }
                }
            }
            return Ok(None);
        };
        let allow_preview = self.preview_reading.take().is_some();
        let target = self.shown.target(&en, upper_end);
        let mut first = None;
        let translated = if target.is_empty() {
            String::new()
        } else if mode == Mode::Live && self.dialogue() && target == en {
            let reply = worker.exchange_progress(
                &json!({"zone":i,"text":target,"previous":previous,
                "stream_sentence":true,"accept_preview":allow_preview}),
                |packet| {
                    if let Some(prefix) = packet["sentence"].as_str() {
                        first = Some(prefix.to_owned());
                        progress(json!({"first":prefix,"rest":""}))?;
                    }
                    Ok(())
                },
            )?;
            reply
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| AppError::Config("Empty translation".into()))?
        } else {
            worker.request(&json!({"zone":i,"text":target,"previous":previous,"hud":self.hud,"accept_preview":allow_preview}))?
        };
        let uk = self.shown.assemble(target, &en, translated);

        let caption = first
            .as_deref()
            .and_then(|prefix| {
                uk.strip_prefix(prefix)
                    .map(|tail| json!({"first":prefix,"rest":tail.trim_start()}))
            })
            .unwrap_or_else(|| json!(uk));
        self.shown.remember(en, uk);
        Ok(Some(caption))
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
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Mode {
    Live,
    Snapshot,
}
fn publish(i: usize, captions: &[Value], zone: &Zone) {
    if zone.legend.is_some() {
        println!("{}", json!({"zone":i,"captions":captions}));
    } else {
        println!(
            "{}",
            json!({"zone":i,"en":zone.shown.source(),"uk":zone.shown.translation(),"captions":captions})
        );
    }
}
struct LaneZone {
    index: usize,
    pixels: Range<usize>,
    zone: Zone,
}
struct LaneFrame {
    pixels: Arc<Vec<u8>>,
    priority: Priority,
}
// Suppress intermediate lines when a newer capture already shows a different panel.
fn panel_stamp(pixels: &[u8]) -> u64 {
    pixels.iter().step_by(13).fold(1u64, |hash, p| {
        hash.wrapping_mul(31).wrapping_add((p >> 5) as u64)
    })
}
enum Priority {
    Dialogue(mpsc::SyncSender<()>),
    Hud(Receiver<()>),
}
fn process_lane(
    mut zones: Vec<LaneZone>,
    frames: Receiver<LaneFrame>,
    worker: &mut Translator,
    captions: &Mutex<Vec<Value>>,
    latest: &[AtomicU64],
) -> Result<()> {
    let ocr = engine()?;
    for frame in frames {
        let release = match frame.priority {
            Priority::Dialogue(tx) => Some(tx),
            Priority::Hud(rx) => {
                worker.hud_admission = Some(rx);
                None
            }
        };
        for z in &mut zones {
            if let Some(value) = z.zone.update(
                &ocr,
                &frame.pixels[z.pixels.clone()],
                worker,
                z.index,
                (Mode::Live, Instant::now()),
                |partial| {
                    if panel_stamp(&frame.pixels[z.pixels.clone()])
                        == latest[z.index].load(Ordering::Relaxed)
                    {
                        let mut snapshot = captions
                            .lock()
                            .map_err(|_| AppError::Config("Zone publisher stopped".into()))?;
                        snapshot[z.index] = partial;
                        println!("{}", json!({"zone":z.index,"captions":*snapshot}));
                    }
                    Ok(())
                },
            )? {
                let mut snapshot = captions
                    .lock()
                    .map_err(|_| AppError::Config("Zone publisher stopped".into()))?;
                snapshot[z.index] = value;
                publish(z.index, &snapshot, &z.zone);
            }
        }
        if let Some(release) = release {
            let _ = release.try_send(());
        }
    }
    Ok(())
}
fn run_lanes(
    zones: Vec<Zone>,
    frames: Receiver<io::Result<Vec<u8>>>,
    dialogue: &mut Translator,
    hud: &mut Translator,
    captions: Vec<Value>,
    latest: &[AtomicU64],
) -> Result<()> {
    let captions = Mutex::new(captions);
    let mut offset = 0;
    let (speech, controls) = zones
        .into_iter()
        .enumerate()
        .map(|(index, zone)| {
            let start = offset;
            offset += (WIDTH * zone.tile_height()) as usize;
            LaneZone {
                index,
                pixels: start..offset,
                zone,
            }
        })
        .partition(|z| z.zone.dialogue());
    let (to_dialogue, dialogue_frames) = mpsc::sync_channel(1);
    let (to_hud, hud_frames) = mpsc::sync_channel(1);
    thread::scope(|scope| {
        let front =
            scope.spawn(|| process_lane(speech, dialogue_frames, dialogue, &captions, latest));
        let background = scope.spawn(|| process_lane(controls, hud_frames, hud, &captions, latest));
        for frame in frames {
            let frame = match frame {
                Ok(f) => Arc::new(f),
                Err(e) => {
                    drop((to_dialogue, to_hud));
                    return Err(e.into());
                }
            };
            // A full HUD queue must never slow capture or block a ready dialogue frame.
            let (release, admission) = mpsc::sync_channel(1);
            if matches!(
                to_dialogue.try_send(LaneFrame {
                    pixels: Arc::clone(&frame),
                    priority: Priority::Dialogue(release)
                }),
                Err(mpsc::TrySendError::Disconnected(_))
            ) || matches!(
                to_hud.try_send(LaneFrame {
                    pixels: frame,
                    priority: Priority::Hud(admission)
                }),
                Err(mpsc::TrySendError::Disconnected(_))
            ) {
                break;
            }
        }
        drop((to_dialogue, to_hud));
        for task in [front, background] {
            task.join()
                .map_err(|_| AppError::Config("Zone worker stopped".into()))??;
        }
        Ok(())
    })
}
fn load_zones() -> Result<Vec<Zone>> {
    let path = PathBuf::from(setting("SUB_RUNTIME")?).join("zones.json");
    let config: Value = serde_json::from_slice(&fs::read(path)?)?;
    let rows = config
        .as_array()
        .filter(|a| (1..=MAX_ZONES).contains(&a.len()))
        .ok_or_else(|| AppError::Config("Choose 1–8 zones".into()))?;
    rows.iter().map(Zone::load).collect()
}
pub(super) fn mixed_layout() -> Result<bool> {
    let zones = load_zones()?;
    Ok(zones.iter().any(Zone::dialogue) && zones.iter().any(|z| !z.dialogue()))
}
pub(super) fn run(worker: &mut Translator, recording: Option<String>, mode: Mode) -> Result<()> {
    let mut zones = load_zones()?;
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
        if mode == Mode::Live {
            cmd.arg("-re");
        }
        cmd.args(["-i", file]);
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
    if mode == Mode::Snapshot {
        cmd.args(["-frames:v", "1"]);
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
    let latest: Arc<Vec<AtomicU64>> =
        Arc::new(zones.iter().map(|_| AtomicU64::default()).collect());
    let stamps = Arc::clone(&latest);
    let mut start = 0;
    let panels: Vec<_> = zones
        .iter()
        .enumerate()
        .filter_map(|(i, z)| {
            let end = start + (WIDTH * z.tile_height()) as usize;
            let span = start..end;
            start = end;
            (mode == Mode::Live
                && (z.dialogue()
                    || (z.legend.is_some()
                        && worker.hud_batch
                        && env::var("SUB_HUD_STREAM").as_deref() != Ok("0"))))
            .then_some((i, span))
        })
        .collect();
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
            for (i, span) in &panels {
                stamps[*i].store(panel_stamp(&buffer[span.clone()]), Ordering::Relaxed);
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
    order.sort_by_key(|(i, _)| (!zones[*i].dialogue(), zones[*i].legend.is_some()));
    if let Some(mut hud) = worker.hud_worker.take() {
        run_lanes(zones, frames, worker, &mut hud, captions, &latest)?;
        let status = capture.0.wait()?;
        return if status.success() && recording.is_some() {
            Ok(())
        } else {
            Err(AppError::Config(format!("Capture ended ({status})")))
        };
    }
    let snapshot = if mode == Mode::Snapshot {
        Some(
            frames
                .recv_timeout(Duration::from_secs(12))
                .map_err(|_| AppError::Config("No frame from the selected source".into()))??,
        )
    } else {
        None
    };
    let mut confirmed_frame = false;
    loop {
        let live_frame;
        let frame = if let Some(frame) = &snapshot {
            frame
        } else {
            let Ok(next) = frames.recv() else {
                break;
            };
            live_frame = next?;
            &live_frame
        };
        let now = Instant::now();
        for &(i, ref span) in &order {
            let zone = &mut zones[i];
            let pixels = &frame[span.start..span.end];
            if let Some(value) = zone.update(&ocr, pixels, worker, i, (mode, now), |partial| {
                if panel_stamp(pixels) == latest[i].load(Ordering::Relaxed) {
                    captions[i] = partial;
                    println!("{}", json!({"zone":i,"captions":captions}));
                }
                Ok(())
            })? {
                captions[i] = value;
                if mode == Mode::Live {
                    publish(i, &captions, zone);
                }
            }
        }
        if mode == Mode::Snapshot {
            if confirmed_frame
                && !zones
                    .iter()
                    .filter_map(|z| z.legend.as_ref())
                    .any(legend::Legend::has_pending_items)
            {
                println!("{}", json!({"captions":captions}));
                break;
            }
            confirmed_frame = true;
        }
    }
    let status = capture.0.wait()?;
    if !status.success() || (mode == Mode::Live && recording.is_none()) {
        return Err(AppError::Config(format!("Capture ended ({status})")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tall_hud_keeps_panel_text_at_native_size_and_small_zones_keep_their_strip() -> Result<()> {
        let mut row = json!({"Left":857,"Top":163,"W":423,"H":397,"Hud":true});
        let panel = Zone::load(&row)?;
        assert_eq!(panel.scaled(), [423, 397]);
        assert_eq!(panel.tile_height(), 397);
        row["Hud"] = json!(false);
        assert_eq!(Zone::load(&row)?.tile_height(), HEIGHT);
        row["Hud"] = json!(true);
        row["H"] = json!(38);
        assert_eq!(Zone::load(&row)?.tile_height(), HEIGHT);
        Ok(())
    }
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
