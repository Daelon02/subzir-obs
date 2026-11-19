use std::collections::VecDeque;
use std::time::{Duration, Instant};

const MAX_LINES: usize = 2;
const DIALOGUE_GAP: Duration = Duration::from_secs(8);
// Leave room for the current subtitle and response in the 1024-token context.
const MAX_HISTORY_BYTES: usize = 384;
const CARD_GAP: Duration = Duration::from_secs(2);
const PARTIAL_WAIT: Duration = Duration::from_millis(750);

#[derive(Clone, Copy, PartialEq)]
pub enum Card {
    Draft,
    New,
    Revised,
}

fn speaker(text: &str) -> (Option<&str>, &str) {
    match text.split_once(':') {
        Some((label, body))
            if !label.is_empty()
                && label.len() <= 36
                && label
                    .chars()
                    .all(|c| c.is_alphabetic() || " -.'’".contains(c)) =>
        {
            (Some(label.trim()), body.trim_start())
        }
        _ => (None, text),
    }
}

fn continuation(prefix: &str, next: &str) -> Option<String> {
    let (who, first) = speaker(prefix);
    let (other, second) = speaker(next);
    let same = match (who, other) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        (None, None) => true,
        _ => false,
    };
    if !same
        || second.is_empty()
        || first == second
        || prefix.len() + second.len() + 1 > MAX_HISTORY_BYTES
    {
        return None;
    }
    if !unfinished(first) {
        return None;
    }
    if second.starts_with(first) {
        return Some(next.to_owned());
    }
    Some(format!("{prefix} {second}"))
}

fn unfinished(first: &str) -> bool {
    let lower = first.to_ascii_lowercase();
    // Require an unfinished clause. Missing final punctuation alone is frequent OCR noise.
    let dangling = [
        "now that",
        "even though",
        "as soon as",
        "so that",
        "because",
        "whether",
        "in case",
        "instead of",
        "as long as",
        "in order to",
        "about to",
    ];
    dangling
        .iter()
        .any(|tail| lower == *tail || lower.strip_suffix(tail).is_some_and(|s| s.ends_with(' ')))
}

#[derive(Default)]
pub struct DialogueHistory {
    lines: VecDeque<String>,
    last_visible: Option<Instant>,
    // Needed when OCR corrects the second card of a joined subtitle.
    joined_prefix: Option<String>,
    pending: Option<(Instant, String, (String, String))>,
}

impl DialogueHistory {
    pub fn present(&mut self, raw: String, now: Instant, kind: Card) -> Option<(String, String)> {
        let wait = unfinished(speaker(&raw).1);
        if wait && kind == Card::Draft {
            return None;
        }
        let original = wait.then(|| raw.to_owned());
        let pair = self.card(raw, now, kind);
        if kind != Card::Draft {
            self.pending = None;
        }
        if let Some(key) = original {
            self.pending = Some((now, key, pair));
            return None;
        }
        Some(pair)
    }
    pub fn due(&mut self, reading: &str, now: Instant) -> Option<(String, String)> {
        if self.pending.as_ref().is_some_and(|(since, key, _)| {
            key == reading && now.saturating_duration_since(*since) >= PARTIAL_WAIT
        }) {
            return self.pending.take().map(|(_, _, pair)| pair);
        }
        None
    }
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

    pub fn card(&mut self, raw: String, now: Instant, kind: Card) -> (String, String) {
        let all = self.previous(now);
        if raw.is_empty() {
            return (raw, all);
        }
        let recent = self
            .last_visible
            .is_some_and(|t| now.saturating_duration_since(t) <= CARD_GAP);
        let prefix = if kind == Card::Revised {
            self.joined_prefix.as_ref()
        } else {
            self.lines.back()
        };
        let merged = prefix
            .filter(|_| recent)
            .and_then(|p| continuation(p, &raw));
        let replace = kind == Card::Revised || merged.is_some();
        let context = if replace {
            self.lines
                .iter()
                .take(self.lines.len().saturating_sub(1))
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            all
        };
        if kind != Card::Draft {
            if merged.is_none() {
                self.joined_prefix = None;
            } else if kind != Card::Revised {
                self.joined_prefix = self.lines.back().cloned();
            }
            if replace {
                self.discard_last_version();
            }
        }
        let text = merged.unwrap_or(raw);
        if kind != Card::Draft {
            self.confirm(&text, now);
        }
        (text, context)
    }

    pub fn previous(&mut self, now: Instant) -> String {
        if self.expired(now) {
            self.lines.clear();
            self.joined_prefix = None;
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
    fn waits_only_for_a_visibly_unfinished_card() {
        let t = Instant::now();
        let mut h = DialogueHistory::default();
        let a = "Cal: What happens now that";
        let b = "Cal: Brian is leaving?";
        assert!(h.present(a.into(), t, Card::New).is_none());
        assert!(h.due(a, t + Duration::from_millis(500)).is_none());
        assert_eq!(
            h.present(b.into(), t + Duration::from_millis(500), Card::New),
            Some((
                "Cal: What happens now that Brian is leaving?".into(),
                String::new()
            ))
        );
        assert!(h.due(a, t + PARTIAL_WAIT).is_none());
        assert!(h.present(a.into(), t + CARD_GAP, Card::New).is_none());
        assert!(h.due(a, t + CARD_GAP + PARTIAL_WAIT).is_some());
    }
    #[test]
    fn assembles_adjacent_cards_without_repeating_the_speaker_or_polluting_history() {
        let t = Instant::now();
        let prior = "Jay: I'm still thinking.";
        let a = "Cal: So, Jay, you worked out what you're gonna do now that";
        let b = "Cal: Brian's winding down the import bizz?";
        let joined = format!("{a} Brian's winding down the import bizz?");
        let mut history = DialogueHistory::default();
        history.card(prior.into(), t, Card::New);
        history.card(a.into(), t, Card::New);
        assert_eq!(
            history.card(b.into(), t, Card::Draft),
            (joined.clone(), prior.into())
        );
        assert_eq!(history.card(b.into(), t, Card::New), (joined, prior.into()));
        assert_eq!(
            history
                .card(
                    "Cal: Brian's winding down the import biz?".into(),
                    t,
                    Card::Revised
                )
                .0,
            format!("{a} Brian's winding down the import biz?")
        );
        for (first, second, elapsed) in [
            (a, "Jay: I know.", 0),
            (a, b, 3),
            ("Cal: Remember that?", b, 0),
        ] {
            let mut h = DialogueHistory::default();
            h.card(first.into(), t, Card::New);
            assert_eq!(
                h.card(second.into(), t + Duration::from_secs(elapsed), Card::New)
                    .0,
                second
            );
        }
    }
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
