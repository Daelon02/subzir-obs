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
    fn overlaps_word(self, other: Self) -> bool {
        (self.b.min(other.b) - self.y.max(other.y)) * 2 >= (self.b - self.y).min(other.b - other.y)
            && (self.r.min(other.r) - self.x.max(other.x)) * 2
                >= (self.r - self.x).min(other.r - other.x)
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
    fn read(
        ocr: &OcrEngine,
        pixels: &[u8],
        tile: [i32; 2],
        crop: [u32; 4],
        panel: bool,
    ) -> Result<Self> {
        let [w, h, x, y] = crop;
        let [sw, sh] = tile;
        let mut fresh = Self::default();
        let mut word_bounds = Vec::new();
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
                    if panel {
                        word_bounds.push(bounds);
                    }
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
        if panel {
            fresh.bands = word_bounds;
        }
        fresh.rows.truncate(MAX_ITEMS);
        Ok(fresh)
    }
    // Wrapped prose has comparable line heights, a shared text column and a
    // small baseline gap. Keep short independent controls and headings separate.
    fn join_panel_paragraphs(&mut self) {
        let mut groups: Vec<Item> = Vec::new();
        let mut previous: Option<Bounds> = None;
        for item in self.rows.drain(..) {
            let b = item.bounds;
            let continues = previous.is_some_and(|a| {
                let h = (a.b - a.y).min(b.b - b.y);
                let overlap = a.r.min(b.r) - a.x.max(b.x);
                let gap = b.y - a.b;
                gap >= 0
                    && gap <= h
                    && ((a.b - a.y) - (b.b - b.y)).abs() <= h / 2
                    && overlap >= (a.r - a.x).min(b.r - b.x) * 3 / 4
            });
            previous = Some(b);
            if let Some(group) = groups.last_mut().filter(|g| {
                continues
                    && g.text.split_whitespace().take(4).count() >= 4
                    && !g
                        .text
                        .chars()
                        .filter(char::is_ascii_alphabetic)
                        .all(|c| c.is_uppercase())
                    && (item.text.split_whitespace().take(3).count() >= 3
                        || !g.text.ends_with(['.', '!', '?']))
            }) {
                group.text.push(' ');
                group.text.push_str(&item.text);
                group.bounds = group.bounds.union(b);
            } else {
                groups.push(item);
            }
        }
        self.rows = groups;
    }
    fn line(&mut self, words: Vec<Item>) {
        if let Some(band) = words.iter().map(|w| w.bounds).reduce(Bounds::union) {
            self.bands.push(band);
        }
        let mut group: Option<Item> = None;
        let count = words.len();
        for (index, word) in words.into_iter().enumerate() {
            let trailing_badge = index + 1 == count
                && group.as_ref().is_some_and(|label| {
                    let uppercase = label.text.chars().any(|c| c.is_ascii_uppercase())
                        && !label.text.chars().any(|c| c.is_ascii_lowercase());
                    uppercase
                        && (word.text.len() == 1
                            && word.text.bytes().all(|c| c.is_ascii_lowercase())
                            || !word.text.chars().any(char::is_alphabetic)
                                && word.bounds.x - label.bounds.r
                                    >= (word.bounds.b - word.bounds.y) / 2)
                });
            let is_icon = trailing_badge
                || is_button(&word.text)
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

    fn recover_sparse(&mut self, other: Self) {
        // Both passes may find the same faint word with different boxes. Use the
        // high-contrast box once, so it does not consume the next label's gap.
        self.bands
            .retain(|b| !other.bands.iter().any(|v| b.overlaps_word(*v)));
        for candidate in other.rows {
            if !candidate.text.chars().any(char::is_alphabetic) {
                continue;
            }
            let b = candidate.bounds;
            let same_line = |a: Bounds| {
                let overlap_y = a.b.min(b.b) - a.y.max(b.y);
                overlap_y * 2 >= (a.b - a.y).min(b.b - b.y) && a.r.min(b.r) > a.x.max(b.x)
            };
            if let Some(current) = self.rows.iter_mut().find(|v| same_line(v.bounds)) {
                // Isolated lowercase fragments are a common OCR failure on
                // contact banners where a bright logo touches a short name.
                let weak = current
                    .text
                    .split_whitespace()
                    .next()
                    .is_some_and(|s| s.len() == 1 && s.bytes().all(|c| c.is_ascii_lowercase()));
                if current.text == candidate.text
                    || weak && candidate.text.split_whitespace().all(|w| w.len() > 1)
                {
                    *current = candidate;
                }
            } else {
                self.rows.push(candidate);
            }
        }
        self.bands.extend(other.bands);
        self.rows.sort_by_key(|v| (v.bounds.y, v.bounds.x));
        self.rows.truncate(MAX_ITEMS);
    }
}

#[derive(Default)]
pub(super) struct Legend {
    panel: bool,
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
    pub(super) fn has_pending_items(&self) -> bool {
        self.pending_context || self.queued_labels
    }
    pub(super) fn panel() -> Self {
        Self {
            panel: true,
            ..Self::default()
        }
    }
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
            fresh = Scan::read(ocr, pixels, tile, crop, self.panel)?;
            if self.panel {
                if fresh.rows.len() <= 2 {
                    let enhanced: Vec<u8> = pixels
                        .iter()
                        .map(|&v| ((i32::from(v) * 5 / 2) - 121).clamp(0, 255) as u8)
                        .collect();
                    fresh.recover_sparse(Scan::read(ocr, &enhanced, tile, crop, true)?);
                }
                // Keep clocks and counters visible and protected in the source.
                fresh
                    .rows
                    .retain(|r| r.text.chars().any(char::is_alphabetic));
                fresh.join_panel_paragraphs();
            }
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
                let request = json!({"zone":zone,"text":item.text,"previous":"","hud":true,"legend":!self.panel,"panel":self.panel,"nearby":key.1});
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
    #[test]
    fn hud_badges_do_not_reach_translation_but_stay_protected() {
        for (label, badge) in [("REPS", "518"), ("SET DESTINATION", "e")] {
            let mut scan = Scan::default();
            scan.line(vec![word(label, 0, 50), word(badge, 62, 15)]);
            assert_eq!(scan.rows.len(), 1);
            assert_eq!(scan.rows[0].text, label);
            assert_eq!(scan.bands[0].r, 77);
        }
    }

    #[test]
    fn panel_keeps_title_and_control_separate_from_wrapped_prose() {
        let mut scan = Scan::default();
        for (text, y) in [
            ("RUNIC ATTACKS", 10),
            ("Gems give Kratos access to", 120),
            ("light or heavy attacks.", 138),
            ("CLOSE", 182),
        ] {
            let mut item = word(text, 30, 140);
            item.bounds.y = y;
            item.bounds.b = y + 12;
            scan.rows.push(item);
        }
        scan.join_panel_paragraphs();
        assert_eq!(
            scan.rows
                .iter()
                .map(|v| v.text.as_str())
                .collect::<Vec<_>>(),
            [
                "RUNIC ATTACKS",
                "Gems give Kratos access to light or heavy attacks.",
                "CLOSE"
            ]
        );
        assert_eq!(scan.rows[1].bounds.array(), [30, 120, 140, 30]);
    }
}
