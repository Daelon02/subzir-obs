use std::time::{Duration, Instant};

const MATCH_PENDING: f32 = 0.76;
const MATCH_DISPLAY: f32 = 0.60;
const CLEAR_AFTER: Duration = Duration::from_millis(380);
const NOISY_TIMEOUT: Duration = Duration::from_millis(1400);
const EXACT_REPLACEMENT_FRAMES: u8 = 4;

fn ending(text: &str) -> Option<char> {
    text.trim_end_matches(|c: char| {
        c.is_whitespace() || matches!(c, '"' | '”' | '’' | '\'' | ')' | ']')
    })
    .chars()
    .next_back()
    .filter(|c| matches!(c, '.' | '?' | '!'))
}

#[derive(Debug, PartialEq)]
pub enum Observation {
    Pending,
    Held,
    Confirmed(String),
    Revised(String),
    Cleared,
}

struct Reading {
    text: String,
    key: String,
    pairs: Vec<[u8; 2]>,
}

impl Reading {
    fn new(text: &str) -> Self {
        let key: String = text
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect();
        let mut pairs: Vec<_> = key.as_bytes().windows(2).map(|p| [p[0], p[1]]).collect();
        pairs.sort_unstable();
        Self {
            text: text.to_owned(),
            key,
            pairs,
        }
    }

    fn similarity(&self, other: &Self) -> f32 {
        if self.key == other.key {
            return 1.0;
        }
        let (mut a, mut b, mut shared) = (0, 0, 0);
        while a < self.pairs.len() && b < other.pairs.len() {
            match self.pairs[a].cmp(&other.pairs[b]) {
                std::cmp::Ordering::Less => a += 1,
                std::cmp::Ordering::Greater => b += 1,
                std::cmp::Ordering::Equal => {
                    shared += 1;
                    a += 1;
                    b += 1;
                }
            }
        }
        2.0 * shared as f32 / (self.pairs.len() + other.pairs.len()).max(1) as f32
    }

    fn score(&self) -> i32 {
        // Prefer complete phrases over fragments when two observations agree.
        let punctuation = i32::from(self.text.ends_with(['.', '?', '!']));
        let intact = self
            .text
            .split_whitespace()
            .filter(|w| {
                w.chars()
                    .all(|c| c.is_ascii_alphabetic() || ".,!?:’'-".contains(c))
            })
            .count();
        (2 * intact as i32) + 3 * punctuation
    }
}

struct Candidate {
    best: Reading,
    previous_key: String,
    previous_ending: Option<char>,
    exact: u8,
    hits: u8,
}

#[derive(Default)]
pub struct Stabilizer {
    shown: Option<Reading>,
    pending: Option<Candidate>,
    last_seen: Option<Instant>,
}

impl Stabilizer {
    pub fn first_candidate(&self) -> Option<&str> {
        self.pending
            .as_ref()
            .filter(|p| p.hits == 1)
            .map(|p| p.best.text.as_str())
    }

    pub fn observe(&mut self, text: &str, now: Instant) -> Observation {
        if self.shown.as_ref().is_some_and(|s| s.text == text) {
            self.last_seen = Some(now);
            self.pending = None;
            return Observation::Held;
        }
        let reading = Reading::new(text);
        if reading.key.is_empty() {
            self.pending = None;
            return self.expire(now, CLEAR_AFTER);
        }
        let resembles_shown = self
            .shown
            .as_ref()
            .is_some_and(|s| s.similarity(&reading) >= MATCH_DISPLAY);
        let same_words = self.shown.as_ref().is_some_and(|s| s.key == reading.key);
        let new_ending = ending(text);
        if resembles_shown {
            self.last_seen = Some(now);
            if same_words
                && (new_ending.is_none()
                    || self.shown.as_ref().and_then(|s| ending(&s.text)) == new_ending)
            {
                self.pending = None;
                return Observation::Held;
            }
        }

        let candidate = match self.pending.as_mut() {
            Some(p) if p.best.similarity(&reading) >= MATCH_PENDING => {
                p.hits = p.hits.saturating_add(1);
                p.exact = if p.previous_key == reading.key && p.previous_ending == new_ending {
                    p.exact.saturating_add(1)
                } else {
                    1
                };
                p.previous_key.clone_from(&reading.key);
                p.previous_ending = new_ending;
                if p.exact >= EXACT_REPLACEMENT_FRAMES || reading.score() > p.best.score() {
                    p.best = reading;
                }
                p
            }
            _ => self.pending.insert(Candidate {
                previous_key: reading.key.clone(),
                previous_ending: new_ending,
                best: reading,
                hits: 1,
                exact: 1,
            }),
        };
        if candidate.hits >= 2 && (!resembles_shown || candidate.exact >= EXACT_REPLACEMENT_FRAMES)
        {
            if let Some(candidate) = self.pending.take() {
                let update = candidate.best.text.clone();
                self.shown = Some(candidate.best);
                self.last_seen = Some(now);
                return if same_words {
                    Observation::Revised(update)
                } else {
                    Observation::Confirmed(update)
                };
            }
        }
        if same_words {
            return Observation::Held;
        }
        self.expire(now, NOISY_TIMEOUT)
    }

    fn expire(&mut self, now: Instant, timeout: Duration) -> Observation {
        if self
            .last_seen
            .is_some_and(|t| now.duration_since(t) >= timeout)
        {
            self.last_seen = None;
            if self.shown.take().is_some() {
                return Observation::Cleared;
            }
        }
        Observation::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_single_reading_can_start_translation_but_cannot_publish() {
        let mut stable = Stabilizer::default();
        let now = Instant::now();
        assert_eq!(stable.observe("one", now), Observation::Pending);
        assert_eq!(stable.first_candidate(), Some("one"));
        assert_eq!(
            stable.observe("", now + Duration::from_millis(100)),
            Observation::Pending
        );
        assert_eq!(stable.first_candidate(), None);
        assert_eq!(
            stable.observe("two", now + Duration::from_millis(200)),
            Observation::Pending
        );
        assert_eq!(
            stable.observe("two", now + Duration::from_millis(300)),
            Observation::Confirmed("two".into())
        );
        assert_eq!(stable.first_candidate(), None);
    }
    #[test]
    fn holds_a_misread_and_clears_on_sustained_silence() {
        let mut s = Stabilizer::default();
        let t = Instant::now();
        let mut read = |text: &str, ms| s.observe(text, t + Duration::from_millis(ms));
        assert_eq!(read("Keep moving!", 0), Observation::Pending);
        assert_eq!(
            read("Keep moving!", 100),
            Observation::Confirmed("Keep moving!".into())
        );
        assert_eq!(read("Keop movlng!", 200), Observation::Pending);
        assert_eq!(read("", 300), Observation::Pending);
        assert_eq!(read("Keep moving!", 400), Observation::Held);
        assert_eq!(read("", 900), Observation::Cleared);
        assert_eq!(read("", 1000), Observation::Pending);
    }
    #[test]
    fn similar_successive_lines_can_replace_each_other() {
        let mut s = Stabilizer::default();
        let t = Instant::now();
        for i in 0..2 {
            s.observe(
                "Please turn left at the next corner!",
                t + Duration::from_millis(100 * i),
            );
        }
        for i in 2..5 {
            s.observe(
                "Please turn right at the next corner!",
                t + Duration::from_millis(100 * i),
            );
        }
        assert_eq!(
            s.observe(
                "Please turn right at the next corner!",
                t + Duration::from_millis(500)
            ),
            Observation::Confirmed("Please turn right at the next corner!".into())
        );
    }

    #[test]
    fn only_four_consecutive_matching_endings_revise_a_caption() {
        let mut s = Stabilizer::default();
        let t = Instant::now();
        s.observe("You're coming.", t);
        s.observe("You're coming.", t);
        for text in [
            "You're coming?",
            "You're coming?",
            "You're coming!",
            "You're coming?",
            "You're coming",
            "You're coming?",
            "You're coming.",
            "You're coming?",
            "You're coming?",
            "You're coming?",
        ] {
            assert_eq!(s.observe(text, t), Observation::Held);
        }
        assert_eq!(
            s.observe("You're coming?", t),
            Observation::Revised("You're coming?".into())
        );
        assert_eq!(s.observe("You're coming?", t), Observation::Held);
    }
}
