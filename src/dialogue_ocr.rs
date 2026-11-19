use super::{recognize_result, recognized_text, OcrEngine, OcrResult, Result, WIDTH};

const EDGE_STEP: usize = 2;
const MIN_RAW_EDGE: u8 = 24;
const MIN_EDGES: usize = 5;
const MIN_ROWS: usize = 4;

// A faded line can have clear strokes on gray while the bright mask is flat.
fn missing_strokes(raw: &[u8], mask: &[u8]) -> bool {
    let mut streak = 0;
    let (rows, _) = raw.as_chunks::<{ WIDTH as usize }>();
    let (high_rows, _) = mask.as_chunks::<{ WIDTH as usize }>();
    for (row, high) in rows.iter().zip(high_rows) {
        let mut raw_edges = 0;
        let mut retained = 0;
        for x in (EDGE_STEP..row.len()).step_by(EDGE_STEP) {
            if row[x].saturating_sub(row[x - EDGE_STEP]) >= MIN_RAW_EDGE {
                raw_edges += 1;
                if high[x].abs_diff(high[x - EDGE_STEP]) >= MIN_RAW_EDGE {
                    retained += 1;
                }
            }
        }
        if raw_edges >= MIN_EDGES && retained * 2 < raw_edges {
            streak += 1;
            if streak >= MIN_ROWS {
                return true;
            }
        } else {
            streak = 0;
        }
    }
    false
}

fn tokens(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
}

// Accept the gray reading only if it reproduces the masked text and supplies
// at least one additional physical line. A spelling change alone is insufficient.
fn has_extra_line(masked: &str, lines: &[String]) -> bool {
    if lines.len() < 2 {
        return false;
    }
    let target: Vec<_> = tokens(masked).collect();
    if target.is_empty() {
        return false;
    }
    let mut row_ids = Vec::new();
    let mut words = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        for word in tokens(line) {
            words.push(word);
            row_ids.push(index);
        }
    }
    words
        .windows(target.len())
        .enumerate()
        .any(|(start, part)| {
            part == target
                && (row_ids.first() != row_ids.get(start)
                    || row_ids.last() != row_ids.get(start + part.len() - 1))
        })
}

fn physical_lines(result: &OcrResult) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    for line in result.Lines()? {
        let text = line
            .Text()?
            .to_string()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !text.is_empty() {
            lines.push(text);
        }
    }
    Ok(lines)
}

// Ask for row boundaries on the existing OCR result only for a possible append.
pub(super) fn read(
    ocr: &OcrEngine,
    pixels: &[u8],
    shown: Option<&str>,
) -> Result<(String, Option<usize>)> {
    let result = recognize_result(ocr, pixels)?;
    let text = recognized_text(&result)?;
    let upper = shown.filter(|upper| text.strip_prefix(upper).is_some_and(|s| s.starts_with(' ')));
    let end = if let Some(upper) = upper {
        let lines = physical_lines(&result)?;
        lines
            .split_last()
            .filter(|(_, prior)| prior.join(" ") == upper)
            .map(|_| upper.len())
    } else {
        None
    };
    Ok((text, end))
}

// The byte offset marks the end of the physical rows above the recovered last row.
pub(super) fn recover(
    ocr: &OcrEngine,
    masked: String,
    raw: &[u8],
    high: &[u8],
) -> Result<(String, Option<usize>)> {
    if !missing_strokes(raw, high) {
        return Ok((masked, None));
    }
    let lines = physical_lines(&recognize_result(ocr, raw)?)?;
    if has_extra_line(&masked, &lines) {
        let text = lines.join(" ");
        let upper_end = lines.last().map(|last| text.len() - last.len() - 1);
        Ok((text, upper_end))
    } else {
        Ok((masked, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keeps_an_additional_row_only_when_the_original_text_is_still_present() {
        let lines = ["Lucia: Fits me better, anyway.".into(), "Fuck you!".into()];
        assert!(has_extra_line("Fits me better anyway", &lines));
        assert!(has_extra_line("Fuck you!", &lines));
        assert!(!has_extra_line(
            "Lucia: Fits me better, anyway. Fuck you!",
            &lines
        ));
        assert!(!has_extra_line("different dialogue", &lines));
        assert!(!has_extra_line("Fits me better", &lines[..1]));
    }
    #[test]
    fn gray_edges_trigger_a_retry_only_if_the_mask_loses_strokes() {
        let row: Vec<_> = (0..WIDTH)
            .map(|x| if x % 8 < 4 { 40 } else { 110 })
            .collect();
        let raw = row.repeat(MIN_ROWS);
        assert!(!missing_strokes(&raw, &raw));
        assert!(missing_strokes(&raw, &vec![0; raw.len()]));
    }
}
