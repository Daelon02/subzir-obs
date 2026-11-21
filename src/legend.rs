mod anchor;
use super::*;
use anchor::GlyphAnchor;
use std::collections::HashMap;

const POSITION_SLOP: i32 = 6;
const CLEAR_AFTER_MISSES: u8 = 3;
const CACHE_LIMIT: usize = 256;
const MAX_ITEMS: usize = 64;
const NEW_LABELS_PER_FRAME: usize = 2;
const MAX_BATCH_LABELS: usize = 4;
const MAX_LABEL_WORDS: usize = 5;
const MAX_LABEL_BYTES: usize = 64;
const DISCOVERY_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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

impl Item {
    fn matches(&self, other: &Self) -> bool {
        self.text.eq_ignore_ascii_case(&other.text) && self.bounds.near(other.bounds)
    }
}

// A single OCR miss must not erase a visible label. Keep the original bounds
// while it is matched so OCR jitter cannot change the fitted font size.
fn stable_rows(
    shown: &[Item],
    misses: &mut Vec<u8>,
    current: &[Item],
    previous: &[Item],
) -> (Vec<Item>, bool, bool) {
    let mut used = vec![false; current.len()];
    let mut held = false;
    let mut next = Vec::new();
    for (old, count) in shown.iter().zip(misses.iter()) {
        if let Some((index, _)) = current
            .iter()
            .enumerate()
            .find(|(i, item)| !used[*i] && old.matches(item))
        {
            used[index] = true;
            next.push((old.clone(), 0));
        } else if count + 1 < CLEAR_AFTER_MISSES {
            next.push((old.clone(), count + 1));
        }
    }
    let mut partial = false;
    for (_, item) in current.iter().enumerate().filter(|(i, _)| !used[*i]) {
        if previous.iter().any(|p| p.matches(item)) {
            // A confirmed replacement in this slot supersedes the held label.
            let moved = current
                .iter()
                .filter(|r| r.text.eq_ignore_ascii_case(&item.text))
                .count()
                == 1
                && shown
                    .iter()
                    .filter(|r| r.text.eq_ignore_ascii_case(&item.text))
                    .count()
                    == 1;
            next.retain(|(old, n)| {
                *n == 0
                    || !(old.bounds.overlaps_word(item.bounds)
                        || moved && old.text.eq_ignore_ascii_case(&item.text))
            });
            next.push((item.clone(), 0));
        } else {
            partial = true;
        }
    }
    next.sort_by_key(|(r, _)| (r.bounds.y, r.bounds.x));
    misses.clear();
    let rows = next
        .into_iter()
        .map(|(item, count)| {
            held |= count > 0;
            misses.push(count);
            item
        })
        .collect();
    (rows, partial, held)
}

fn similar_bands(left: &[Bounds], right: &[Bounds]) -> bool {
    left.len() == right.len() && left.iter().all(|a| right.iter().any(|b| a.near(*b)))
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
    fn split_controls(&mut self, alternate: &Self) {
        let mut rows = Vec::new();
        for item in self.rows.drain(..) {
            let parts: Vec<_> = alternate
                .rows
                .iter()
                .filter(|r| {
                    item.bounds.overlaps_word(r.bounds)
                        && r.text.len() >= 2
                        && item.text.contains(&r.text)
                        && r.text.bytes().all(|c| c.is_ascii_uppercase() || c == b' ')
                })
                .collect();
            let separated = parts.len() >= 2
                && parts
                    .windows(2)
                    .all(|p| p[1].bounds.x - p[0].bounds.r >= item.bounds.b - item.bounds.y);
            if separated {
                rows.extend(parts.into_iter().cloned());
            } else {
                rows.push(item);
            }
        }
        self.rows = rows;
    }
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
    misses: Vec<u8>,
    labels: Vec<String>,
    pending_context: bool,
    queued_labels: bool,
    bands: Vec<Bounds>,
    anchors: HashMap<Bounds, (String, GlyphAnchor)>,
    last_scan: Option<Instant>,
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
        geometry: ([i32; 2], [u32; 4]),
        worker: &mut Translator,
        publication: (usize, bool),
        progress: &mut impl FnMut(Value) -> Result<()>,
    ) -> Result<Option<Value>> {
        let (tile, crop) = geometry;
        let (zone, live) = publication;
        let changed = self.previous_pixels != pixels;
        let mut fresh = Scan::default();
        if changed {
            let visible: Vec<_> = self
                .shown
                .iter()
                .filter(|item| {
                    self.anchors
                        .get(&item.bounds)
                        .is_some_and(|(_, a)| a.visible(pixels, tile, crop))
                })
                .collect();
            if !self.shown.is_empty()
                && visible.len() == self.shown.len()
                && !self.pending_context
                && !self.queued_labels
                && self.misses.iter().all(|&n| n == 0)
                && self
                    .last_scan
                    .is_some_and(|t| t.elapsed() < DISCOVERY_INTERVAL)
            {
                return Ok(None);
            }
            fresh = Scan::read(ocr, pixels, tile, crop, self.panel)?;
            if fresh.rows.iter().any(|r| {
                r.text.contains(' ')
                    && r.text.bytes().all(|c| c.is_ascii_uppercase() || c == b' ')
                    && !visible.iter().any(|old| old.matches(r))
            }) {
                // A bright controller icon can be joined to the preceding word.
                // Accept a split only if the second reading finds separated labels.
                let binary: Vec<_> = pixels
                    .iter()
                    .map(|&p| if p > 155 { 255 } else { 0 })
                    .collect();
                fresh.split_controls(&Scan::read(ocr, &binary, tile, crop, self.panel)?);
            }
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
            for row in &mut fresh.rows {
                let clean = row.text.trim_start_matches(['-', '—', '–']);
                if clean.len() != row.text.len()
                    && clean.len() >= 2
                    && clean.bytes().all(|c| c.is_ascii_uppercase())
                {
                    row.text = clean.to_owned();
                }
            }
            for old in visible {
                // A separate new word can extend a heading. Let OCR confirm that
                // replacement; a misread letter or attached controller glyph is noise.
                let extended = fresh.rows.iter().any(|r| {
                    old.bounds.overlaps_word(r.bounds)
                        && !r.bounds.near(old.bounds)
                        && r.text.split_whitespace().count() > old.text.split_whitespace().count()
                        && r.text
                            .split_whitespace()
                            .last()
                            .is_some_and(|s| s.chars().filter(|c| c.is_alphabetic()).count() > 1)
                });
                if !extended {
                    fresh.rows.retain(|r| !old.bounds.overlaps_word(r.bounds));
                    fresh.rows.push(old.clone());
                }
            }
            fresh.rows.sort_by_key(|r| (r.bounds.y, r.bounds.x));
            fresh.rows.truncate(MAX_ITEMS);
            self.last_scan = Some(Instant::now());
            self.previous_pixels.clear();
            self.previous_pixels.extend_from_slice(pixels);
        }
        let current = if changed { &fresh } else { &self.scan };
        let (next, partial, held) = stable_rows(
            &self.shown,
            &mut self.misses,
            &current.rows,
            &self.scan.rows,
        );
        let settled = self.pending_context && !partial;
        self.pending_context = partial;
        let rows_changed = next.len() != self.shown.len()
            || next
                .iter()
                .zip(&self.shown)
                .any(|(a, b)| a.text != b.text || a.bounds != b.bounds);
        let mut bands_changed = !held
            && !partial
            && (rows_changed || similar_bands(&current.bands, &self.scan.bands))
            && !similar_bands(&current.bands, &self.bands);
        if bands_changed {
            // Keep the collision geometry of an unchanged neighbour when a
            // different control appears or its icon temporarily escapes OCR.
            let bands: Vec<_> = current
                .bands
                .iter()
                .map(|b| {
                    self.bands
                        .iter()
                        .copied()
                        .find(|old| {
                            old.near(*b)
                                || (!self.panel
                                    && old.x <= b.x + POSITION_SLOP
                                    && old.r >= b.r
                                    && (old.y - b.y).abs() <= POSITION_SLOP
                                    && (old.b - b.b).abs() <= POSITION_SLOP
                                    && next.iter().any(|r| {
                                        self.anchors.contains_key(&r.bounds)
                                            && old.overlaps_word(r.bounds)
                                            && b.overlaps_word(r.bounds)
                                    }))
                        })
                        .unwrap_or(*b)
                })
                .collect();
            bands_changed = self.bands != bands;
            self.bands = bands;
        }
        if changed {
            self.scan = fresh;
        }
        if !rows_changed && !bands_changed && !settled && !self.queued_labels {
            return Ok(None);
        }
        self.queued_labels = false;
        let mut pending = Vec::new();
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
            if !previous_label.is_empty() {
                labels.push(previous_label.to_owned());
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
            } else {
                pending.push((i, key));
                previous_label.to_owned()
            };
            labels.push(label);
        }
        let count = if worker.hud_batch {
            pending
                .iter()
                .take(MAX_BATCH_LABELS)
                .take_while(|(_, (s, _))| {
                    s.len() <= MAX_LABEL_BYTES
                        && s.split_whitespace().count() <= MAX_LABEL_WORDS
                        && !s.ends_with(['.', '!', '?'])
                })
                .count()
        } else {
            0
        };
        let batch: Option<Vec<String>> = if count >= 2 {
            let sources: Vec<_> = pending[..count].iter().map(|(_, (s, _))| s).collect();
            let contexts: Vec<_> = pending[..count].iter().map(|(_, (_, ctx))| ctx).collect();
            let mut provisional = labels.clone();
            let mut displayed = false;
            let response = worker.exchange_progress(
                &json!({"hud_labels":sources,"map":!self.panel,"zone":zone,"contexts":contexts,"stream":live}),
                |event| {
                    if let (Some(index), Some(text)) = (event["row"].as_u64().and_then(|n| usize::try_from(n).ok()),
                                                        event["text"].as_str().filter(|s| !s.is_empty())) {
                        if let Some((i, _)) = pending[..count].get(index) {
                            provisional[*i] = text.to_owned();
                            progress(overlay(&next, &provisional, &self.bands))?;
                            displayed = true;
                        }
                    }
                    Ok(())
                })?;
            if response.is_null() && displayed {
                progress(overlay(&next, &labels, &self.bands))?;
            }
            serde_json::from_value(response)
                .ok()
                .filter(|out: &Vec<String>| {
                    out.len() == count && out.iter().all(|s| !s.trim().is_empty())
                })
        } else {
            None
        };
        let limit = batch.as_ref().map(Vec::len).unwrap_or(NEW_LABELS_PER_FRAME);
        self.queued_labels = pending.len() > limit;
        for (n, (i, key)) in pending.into_iter().take(limit).enumerate() {
            let translated = if let Some(out) = &batch {
                out[n].to_owned()
            } else if let Some(value) = self.cache.get(&key) {
                value.to_owned()
            } else {
                worker.request(&json!({"zone":zone,"text":key.0,"previous":"","hud":true,
                    "legend":!self.panel,"panel":self.panel,"nearby":key.1}))?
            };
            if self.cache.len() >= CACHE_LIMIT {
                self.cache.clear();
            }
            labels[i].clone_from(&translated);
            self.cache.insert(key, translated);
        }
        let mut previous_anchors = std::mem::take(&mut self.anchors);
        for (item, misses) in next.iter().zip(&self.misses) {
            if let Some(entry) = previous_anchors
                .remove(&item.bounds)
                .filter(|(text, _)| text == &item.text)
            {
                self.anchors.insert(item.bounds, entry);
            } else if *misses == 0 {
                if let Some(anchor) = GlyphAnchor::capture(pixels, tile, crop, item.bounds) {
                    self.anchors
                        .insert(item.bounds, (item.text.to_owned(), anchor));
                }
            }
        }
        self.shown = next;
        self.labels = labels;
        Ok(Some(overlay(&self.shown, &self.labels, &self.bands)))
    }
}

fn overlay(items: &[Item], labels: &[String], bands: &[Bounds]) -> Value {
    let rows: Vec<_> = items
        .iter()
        .zip(labels)
        .map(|(item, label)| json!({"bounds":item.bounds.array(),"source":item.text,"text":label}))
        .collect();
    json!({"rows":rows,"bands":bands.iter().map(|b|b.array()).collect::<Vec<_>>()})
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
    fn hud_holds_geometry_and_short_dropouts_but_replaces_or_clears_confirmed_rows() {
        let original = word("FLIP", 80, 30);
        let shifted = word("Flip", 84, 31);
        let new = word("READ", 80, 35);
        let mut shown = vec![original.clone()];
        let mut misses = vec![0];
        let mut previous = vec![original.clone()];
        for (frame, expected) in [
            (vec![shifted], Some("FLIP")),
            (vec![], Some("FLIP")),
            (vec![], Some("FLIP")),
            (vec![original.clone()], Some("FLIP")),
            (vec![new.clone()], Some("FLIP")),
            (vec![new], Some("READ")),
            (vec![], Some("READ")),
            (vec![], Some("READ")),
            (vec![], None),
        ] {
            (shown, _, _) = stable_rows(&shown, &mut misses, &frame, &previous);
            assert_eq!(shown.len(), usize::from(expected.is_some()));
            assert_eq!(shown.first().map(|row| row.text.as_str()), expected);
            if expected == Some("FLIP") {
                assert_eq!(shown[0].bounds, original.bounds);
            }
            previous = frame;
        }
    }
    #[test]
    fn a_confirmed_move_replaces_the_old_anchor_but_two_visible_copies_remain() {
        let old = word("BACK", 10, 30);
        let moved = word("BACK", 140, 30);
        let both = [old.clone(), moved.clone()];
        let (next, _, _) = stable_rows(
            std::slice::from_ref(&old),
            &mut vec![0],
            std::slice::from_ref(&moved),
            std::slice::from_ref(&moved),
        );
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].bounds, moved.bounds);
        let (next, _, _) = stable_rows(std::slice::from_ref(&old), &mut vec![0], &both, &both);
        assert_eq!(next.len(), 2);
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
