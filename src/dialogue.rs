use std::collections::VecDeque;
use std::time::{Duration, Instant};

const MAX_LINES: usize = 2;
const DIALOGUE_GAP: Duration = Duration::from_secs(8);
// Leave room for the current subtitle and response in the 1024-token context.
const MAX_HISTORY_BYTES: usize = 384;

#[derive(Default)]
pub struct DialogueHistory {
    lines: VecDeque<String>,
    last_visible: Option<Instant>,
}

impl DialogueHistory {
    fn expired(&self, now: Instant) -> bool {
        self.last_visible
            .is_some_and(|t| now.saturating_duration_since(t) > DIALOGUE_GAP)
    }

    pub fn visible(&mut self, now: Instant) {
        if self.expired(now) {
            // Following a capture stall, only the line seen again is still known.
            let keep = self.lines.len().saturating_sub(1);
            self.lines.drain(..keep);
        }
        self.last_visible = Some(now);
    }

    pub fn discard_last_version(&mut self) {
        self.lines.pop_back();
    }

    pub fn previous(&mut self, now: Instant) -> String {
        if self.expired(now) {
            self.lines.clear();
            self.last_visible = None;
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
        self.last_visible = Some(now);
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
    fn held_frames_refresh_the_gap_and_revisions_keep_the_previous_speaker() {
        let mut h = DialogueHistory::default();
        let t = Instant::now();
        h.confirm("Meet me outside.", t);
        h.confirm("You're coming.", t);
        for second in 1..=10 {
            h.visible(t + Duration::from_secs(second));
        }
        let revised_at = t + Duration::from_secs(10);
        h.discard_last_version();
        assert_eq!(h.previous(revised_at), "Meet me outside.");
        h.confirm("You're coming?", revised_at);
        assert_eq!(h.previous(revised_at), "Meet me outside.\nYou're coming?");
        assert!(h.previous(revised_at + Duration::from_secs(9)).is_empty());
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
