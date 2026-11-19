use std::collections::{BTreeSet, VecDeque};
use std::time::{Duration, Instant};

const MAX_LINES: usize = 6;
const DIALOGUE_GAP: Duration = Duration::from_secs(8);
// Leave room for the current subtitle and response in the 1024-token context.
const MAX_HISTORY_BYTES: usize = 384;
const CARD_GAP: Duration = Duration::from_secs(2);
const PARTIAL_WAIT: Duration = Duration::from_millis(750);

// English function words and acknowledgments carry little information about the topic.
const NEUTRAL_WORDS: &str = "a an and are as at be been but by did do does for from had has have he her here him his how i if in into is it its just me my no of oh okay on or our she so that the their them then there they this to uh was we well were what with would yeah yes you your";

fn topic_words(text: &str) -> BTreeSet<String> {
    speaker(text)
        .1
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| w.len() >= 3)
        .map(str::to_ascii_lowercase)
        .filter(|w| {
            !NEUTRAL_WORDS
                .split_ascii_whitespace()
                .any(|neutral| neutral == w)
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq)]
pub enum Card {
    Draft,
    New,
    Revised,
    Appended(usize),
}

#[derive(Default)]
pub struct ShownCaption {
    source: String,
    translation: String,
}

impl ShownCaption {
    pub fn target<'a>(&self, source: &'a str, upper_end: Option<usize>) -> &'a str {
        match upper_end.and_then(|end| source.split_at_checked(end)) {
            Some((upper, rest)) if upper == self.source && !self.translation.is_empty() => rest
                .strip_prefix(' ')
                .filter(|s| !s.is_empty())
                .unwrap_or(source),
            _ => source,
        }
    }

    pub fn assemble(&self, target: &str, source: &str, translated: String) -> String {
        if target.len() == source.len() {
            translated
        } else {
            format!("{}\n{}", self.translation, translated)
        }
    }

    pub fn remember(&mut self, source: String, translation: String) {
        self.source = source;
        self.translation = translation;
    }
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
    pub fn append_context(&mut self, raw: &str, end: usize, now: Instant) -> String {
        self.expire_history(now);
        if let Some(upper) = raw.get(..end) {
            if !self.lines.back().is_some_and(|last| last == upper) {
                let start = upper.ceil_char_boundary(upper.len().saturating_sub(MAX_HISTORY_BYTES));
                return upper[start..].to_owned();
            }
        }
        self.select(raw.get(end..).unwrap_or(raw), self.lines.len())
    }

    pub fn present(&mut self, raw: String, now: Instant, kind: Card) -> Option<(String, String)> {
        if let Card::Appended(end) = kind {
            let context = self.append_context(&raw, end, now);
            if self
                .lines
                .back()
                .is_some_and(|last| raw.get(..end) == Some(last.as_str()))
            {
                self.discard_last_version();
            }
            self.pending = None;
            self.joined_prefix = None;
            self.confirm(&raw, now);
            return Some((raw, context));
        }
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
        self.expire_history(now);
        if raw.is_empty() {
            return (raw, self.select("", self.lines.len()));
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
        let context = self.select(
            merged.as_deref().unwrap_or(&raw),
            self.lines.len().saturating_sub(usize::from(replace)),
        );
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

    fn expire_history(&mut self, now: Instant) {
        if self.expired(now) {
            self.lines.clear();
            self.joined_prefix = None;
            self.last_visible = None;
        }
    }

    fn select(&self, current: &str, count: usize) -> String {
        let mut earlier = self.lines.iter().take(count).rev();
        let Some(latest) = earlier.next() else {
            return String::new();
        };
        let Some(default) = earlier.clone().next() else {
            return latest.to_owned();
        };
        let fits = |line: &str| line.len() + 1 + latest.len() <= MAX_HISTORY_BYTES;
        let mut choice = default;
        if count > 2 && !current.is_empty() {
            let query = topic_words(current);
            let recent = topic_words(latest);
            let mut best = 0;
            let mut tied = false;
            let mut only_feedback = recent.len() <= 1;
            for line in earlier {
                let words = topic_words(line);
                let overlap =
                    2 * words.intersection(&query).count() + words.intersection(&recent).count();
                if fits(line) {
                    if overlap > best || (best == 0 && only_feedback && words.len() >= 2) {
                        choice = line;
                        best = overlap;
                        tied = false;
                    } else if overlap > 0 && overlap == best {
                        tied = true;
                    }
                }
                only_feedback &= words.is_empty();
            }
            if tied {
                choice = default;
            }
        }
        if fits(choice) {
            format!("{choice}\n{latest}")
        } else {
            latest.to_owned()
        }
    }

    #[cfg(test)]
    fn previous(&mut self, now: Instant) -> String {
        self.expire_history(now);
        self.select("", self.lines.len())
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
        self.lines.push_back(text.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ambiguous_older_matches_leave_the_recent_pair_in_place() {
        let mut h = DialogueHistory::default();
        let now = Instant::now();
        for s in [
            "The bank needs cash.",
            "The store takes cash.",
            "Take the train.",
            "Look at me.",
        ] {
            h.confirm(s, now);
        }
        assert_eq!(
            h.card("Got the cash?".into(), now, Card::Draft).1,
            "Take the train.\nLook at me."
        );
    }
    #[test]
    fn selection_stays_within_turn_and_byte_limits_and_skips_revised_text() {
        let t = Instant::now();
        let mut h = DialogueHistory::default();
        for line in [
            "The red pickup broke down.",
            "Check the battery.",
            "Meet outside.",
            "Tow the pickup.",
        ] {
            h.card(line.into(), t, Card::New);
        }
        let (_, context) = h.card("Who owns the pickup?".into(), t, Card::Draft);
        assert_eq!(context, "The red pickup broke down.\nTow the pickup.");
        assert_eq!(
            h.card("Meet outside now.".into(), t, Card::Revised)
                .1
                .lines()
                .last(),
            Some("Meet outside.")
        );
        for _ in 0..2 {
            for line in ["Hello.", "Yes.", "Okay.", "Yeah."] {
                h.confirm(line, t);
            }
        }
        assert_eq!(h.lines.len(), MAX_LINES);
        assert!(!h.select("That pickup?", MAX_LINES).contains("pickup"));
        assert_eq!(
            h.card(
                "New scene.".into(),
                t + DIALOGUE_GAP + Duration::from_secs(1),
                Card::New
            )
            .1,
            ""
        );
        h.confirm(&"b".repeat(MAX_HISTORY_BYTES), t);
        assert_eq!(
            h.select("Is it ready?", h.lines.len()).len(),
            MAX_HISTORY_BYTES
        );
    }
    #[test]
    fn appended_row_uses_only_a_published_prefix_and_keeps_one_history_entry() {
        let top = "Lucia: Fits me better, anyway.";
        let both = "Lucia: Fits me better, anyway. Fuck you!";
        let upper_uk = "Мені так краще.";
        let now = Instant::now();
        let mut history = DialogueHistory::default();
        let mut shown = ShownCaption::default();
        history.present(top.into(), now, Card::New);
        assert_eq!(shown.target(both, Some(top.len())), both);
        shown.remember(top.into(), upper_uk.into());
        assert_eq!(shown.target(both, None), both);
        let target = shown.target(both, Some(top.len()));
        assert_eq!(target, "Fuck you!");
        assert_eq!(
            shown.assemble(target, both, "Пішов ти!".into()),
            "Мені так краще.\nПішов ти!"
        );
        assert_eq!(
            history.present(both.into(), now, Card::Appended(top.len())),
            Some((both.into(), top.into()))
        );
        assert_eq!(history.previous(now), both);
        shown.remember(String::new(), String::new());
        assert_eq!(shown.target(both, Some(top.len())), both);
    }
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
