use super::*;
use std::collections::HashMap;

const POSITION_SLOP: i32 = 3;
const CACHE_LIMIT: usize = 256;
const MAX_ITEMS: usize = 64;
const NEW_LABELS_PER_FRAME: usize = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Bounds {
    x: i32,
    y: i32,
    r: i32,
    b: i32,
}

impl Bounds {
    fn union(self, o: Self) -> Self {
        Self {
            x: self.x.min(o.x),
            y: self.y.min(o.y),
            r: self.r.max(o.r),
            b: self.b.max(o.b),
        }
    }
    fn near(self, o: Self) -> bool {
        [self.x - o.x, self.y - o.y, self.r - o.r, self.b - o.b]
            .iter()
            .all(|n| n.abs() <= POSITION_SLOP)
    }
    fn array(self) -> [i32; 4] {
        [self.x, self.y, self.r - self.x, self.b - self.y]
    }
}

#[derive(Clone)]
struct Item {
    text: String,
    bounds: Bounds,
}

#[derive(Default)]
struct Scan {
    rows: Vec<Item>,
    bands: Vec<Bounds>,
}

fn is_button(word: &str) -> bool {
    let key = word.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    key.is_empty()
        || matches!(
            key.to_ascii_uppercase().as_str(),
            "X" | "O"
                | "L1"
                | "L2"
                | "L3"
                | "R1"
                | "R2"
                | "R3"
                | "LB"
                | "RB"
                | "LT"
                | "RT"
                | "B"
                | "Y"
                | "L3R3"
        )
}

impl Scan {
    fn read(ocr: &OcrEngine, pixels: &[u8], tile: [i32; 2], crop: [u32; 4]) -> Result<Self> {
        let [w, h, x, y] = crop;
        let [sw, sh] = tile;
        let mut fresh = Self::default();
        let buf = CryptographicBuffer::CreateFromByteArray(pixels)?;
        let bmp = SoftwareBitmap::CreateCopyFromBuffer(&buf, BitmapPixelFormat::Gray8, WIDTH, sh)?;
        let result = ocr.RecognizeAsync(&bmp)?.join()?;
        let map_x =
            |v: f32| x as i32 + (v.clamp(0., sw as f32) * w as f32 / sw as f32).round() as i32;
        let map_y =
            |v: f32| y as i32 + (v.clamp(0., sh as f32) * h as f32 / sh as f32).round() as i32;
        for line in result.Lines()? {
            let mut words = Vec::new();
            for word in line.Words()? {
                let b = word.BoundingRect()?;
                let bounds = Bounds {
                    x: map_x(b.X),
                    y: map_y(b.Y),
                    r: map_x(b.X + b.Width),
                    b: map_y(b.Y + b.Height),
                };
                if bounds.r > bounds.x && bounds.b > bounds.y {
                    words.push(Item {
                        text: word.Text()?.to_string(),
                        bounds,
                    });
                }
            }
            words.sort_by_key(|w| w.bounds.x);
            fresh.line(words);
        }
        fresh.rows.sort_by_key(|i| (i.bounds.y, i.bounds.x));
        fresh.rows.truncate(MAX_ITEMS);
        Ok(fresh)
    }
    fn line(&mut self, words: Vec<Item>) {
        if let Some(band) = words.iter().map(|w| w.bounds).reduce(Bounds::union) {
            self.bands.push(band);
        }
        let mut group: Option<Item> = None;
        let count = words.len();
        for (index, word) in words.into_iter().enumerate() {
            let is_icon = is_button(&word.text)
                || (index + 1 == count && word.text.eq_ignore_ascii_case("A"));
            let gap = group
                .as_ref()
                .is_some_and(|g| word.bounds.x - g.bounds.r > 2 * (g.bounds.b - g.bounds.y).max(1));
            if is_icon || gap {
                self.rows.extend(group.take());
            }
            if is_icon {
                continue;
            }
            if let Some(current) = group.as_mut() {
                current.text.push(' ');
                current.text.push_str(&word.text);
                current.bounds = current.bounds.union(word.bounds);
            } else {
                group = Some(word);
            }
        }
        self.rows.extend(group);
    }
}

#[derive(Default)]
pub(super) struct Legend {
    previous_pixels: Vec<u8>,
    scan: Scan,
    shown: Vec<Item>,
    labels: Vec<String>,
    pending_context: bool,
    queued_labels: bool,
    bands: Vec<Bounds>,
    cache: HashMap<(String, String), String>,
}

impl Legend {
    pub(super) fn observe(
        &mut self,
        ocr: &OcrEngine,
        pixels: &[u8],
        tile: [i32; 2],
        crop: [u32; 4],
        worker: &mut Translator,
        zone: usize,
    ) -> Result<Option<Value>> {
        let changed = self.previous_pixels != pixels;
        let mut fresh = Scan::default();
        if changed {
            fresh = Scan::read(ocr, pixels, tile, crop)?;
            self.previous_pixels.clear();
            self.previous_pixels.extend_from_slice(pixels);
        }
        let current = if changed { &fresh } else { &self.scan };
        let mut next = Vec::new();
        for item in &current.rows {
            let matches = |v: &&Item| v.text == item.text && v.bounds.near(item.bounds);
            if let Some(old) = self.shown.iter().find(matches) {
                next.push(old.clone());
            } else if !changed || self.scan.rows.iter().any(|v| matches(&v)) {
                next.push(item.clone());
            }
        }
        let partial = next.len() != current.rows.len();
        let settled = self.pending_context && !partial;
        self.pending_context = partial;
        let bands_changed = current.bands.len() != self.bands.len()
            || current
                .bands
                .iter()
                .zip(&self.bands)
                .any(|(a, b)| !a.near(*b));
        let rows_changed = next.len() != self.shown.len()
            || next
                .iter()
                .zip(&self.shown)
                .any(|(a, b)| a.text != b.text || a.bounds != b.bounds);
        if bands_changed {
            self.bands.clone_from(&current.bands);
        }
        if changed {
            self.scan = fresh;
        }
        if !rows_changed && !bands_changed && !settled && !self.queued_labels {
            return Ok(None);
        }
        self.queued_labels = false;
        let mut budget = NEW_LABELS_PER_FRAME;
        let mut rows = Vec::new();
        let mut labels = Vec::with_capacity(next.len());
        for (i, item) in next.iter().enumerate() {
            let previous_label = self
                .shown
                .iter()
                .zip(&self.labels)
                .find(|(old, label)| {
                    !label.is_empty() && old.text == item.text && old.bounds.near(item.bounds)
                })
                .map(|(_, label)| label.as_str())
                .unwrap_or_default();
            // Keep unaffected labels on screen while a changed control is being confirmed.
            if partial && !previous_label.is_empty() {
                labels.push(previous_label.to_owned());
                rows.push(
                    json!({"bounds":item.bounds.array(),"source":item.text,"text":previous_label}),
                );
                continue;
            }
            let nearby = next[i.saturating_sub(2)..(i + 3).min(next.len())]
                .iter()
                .filter(|other| other.text != item.text)
                .map(|other| other.text.as_str())
                .collect::<Vec<_>>()
                .join("\n")
                .chars()
                .take(240)
                .collect::<String>();
            let key = (item.text.to_owned(), nearby);
            let label = if let Some(value) = self.cache.get(&key) {
                value.to_owned()
            } else if budget == 0 {
                self.queued_labels = true;
                previous_label.to_owned()
            } else {
                budget -= 1;
                let request = json!({"zone":zone,"text":item.text,"previous":"","hud":true,"legend":true,"nearby":key.1});
                let translated = worker.request(&request)?;
                if self.cache.len() >= CACHE_LIMIT {
                    self.cache.clear();
                }
                self.cache.insert(key, translated.to_owned());
                translated
            };
            rows.push(json!({"bounds":item.bounds.array(),"source":item.text,"text":label}));
            labels.push(label);
        }
        self.shown = next;
        self.labels = labels;
        Ok(Some(
            json!({"rows":rows,"bands":self.bands.iter().map(|b|b.array()).collect::<Vec<_>>()}),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, x: i32, width: i32) -> Item {
        Item {
            text: text.to_owned(),
            bounds: Bounds {
                x,
                y: 10,
                r: x + width,
                b: 22,
            },
        }
    }

    #[test]
    fn separates_columns_and_omits_buttons_while_retaining_words_and_numbers() {
        let mut scan = Scan::default();
        scan.line(vec![
            word("L3", 0, 13),
            word("ADD", 18, 20),
            word("A", 42, 8),
            word("MARKER", 54, 42),
            word("ZOOM", 150, 32),
            word("2", 186, 8),
            word("R2", 200, 15),
        ]);
        assert_eq!(
            scan.rows
                .iter()
                .map(|i| i.text.as_str())
                .collect::<Vec<_>>(),
            ["ADD A MARKER", "ZOOM 2"]
        );
        assert_eq!(scan.rows[0].bounds.array(), [18, 10, 78, 12]);
        assert_eq!(scan.bands[0].array(), [0, 10, 215, 12]);
    }
    #[test]
    fn an_icon_only_row_is_protected_without_requesting_a_translation() {
        let mut scan = Scan::default();
        scan.line(vec![word("□", 20, 14)]);
        assert!(scan.rows.is_empty());
        assert_eq!(scan.bands.len(), 1);
    }
}
