use std::collections::VecDeque;
use std::time::{Duration, Instant};

const MAX_LINES: usize = 2;
const DIALOGUE_GAP: Duration = Duration::from_secs(8);
// Leave room for the current subtitle and response in the 1024-token context.
const MAX_HISTORY_BYTES: usize = 384;

#[derive(Default)]
pub struct DialogueHistory {
    lines: VecDeque<String>,
    last_confirmed: Option<Instant>,
}

impl DialogueHistory {
    pub fn previous(&mut self, now: Instant) -> String {
        if self
            .last_confirmed
            .is_some_and(|t| now.saturating_duration_since(t) > DIALOGUE_GAP)
        {
            self.lines.clear();
            self.last_confirmed = None;
        }
        self.lines
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn confirm(&mut self, text: &str, now: Instant) {
        if text.is_empty() {
            return;
        }
        self.last_confirmed = Some(now);
        if text.len() > MAX_HISTORY_BYTES {
            self.lines.clear();
            return;
        }
        if self.lines.back().is_some_and(|s| s == text) {
            return;
        }
        if self.lines.len() == MAX_LINES {
            self.lines.pop_front();
        }
        if self
            .lines
            .back()
            .is_some_and(|s| s.len() + 1 + text.len() > MAX_HISTORY_BYTES)
        {
            self.lines.clear();
        }
        self.lines.push_back(text.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_two_confirmations_in_order_and_expires_the_whole_dialogue() {
        let mut h = DialogueHistory::default();
        let t = Instant::now();
        h.confirm("first", t);
        h.confirm("second", t);
        h.confirm("second", t);
        h.confirm("", t);
        assert_eq!(h.previous(t), "first\nsecond");
        h.confirm("third", t);
        assert_eq!(h.previous(t + DIALOGUE_GAP), "second\nthird");
        assert!(h
            .previous(t + DIALOGUE_GAP + Duration::from_millis(1))
            .is_empty());
    }
    #[test]
    fn keeps_complete_lines_within_budget() {
        let mut h = DialogueHistory::default();
        let t = Instant::now();
        h.confirm("first", t);
        let full = "a".repeat(MAX_HISTORY_BYTES);
        h.confirm(&full, t);
        assert_eq!(h.previous(t), full);
        h.confirm(&"a".repeat(MAX_HISTORY_BYTES + 1), t);
        assert!(h.previous(t).is_empty());
    }
}
