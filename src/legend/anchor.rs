use super::{Bounds, WIDTH};

// Compare the pattern of light and dark strokes inside the original word.
// Normalized correlation also matches the disabled (dimmed) form of a button.
pub(super) struct GlyphAnchor {
    samples: Vec<(usize, i16)>,
    stripe: usize,
}

impl GlyphAnchor {
    pub(super) fn capture(
        pixels: &[u8],
        tile: [i32; 2],
        crop: [u32; 4],
        b: Bounds,
    ) -> Option<Self> {
        let [w, h, x, y] = crop;
        let map = |v: i32, origin: u32, extent: u32, scaled: i32| {
            ((v - origin as i32) * scaled / extent as i32).clamp(0, scaled)
        };
        let left = map(b.x, x, w, tile[0]);
        let right = map(b.r, x, w, tile[0]);
        let top = map(b.y, y, h, tile[1]);
        let bottom = map(b.b, y, h, tile[1]);
        let cols = (right - left).min(96);
        let rows = (bottom - top).min(12);
        if cols < 3 || rows < 3 {
            return None;
        }
        let mut samples = Vec::with_capacity((rows * cols) as usize);
        for col in 0..cols {
            let px = left + (2 * col + 1) * (right - left) / (2 * cols);
            for row in 0..rows {
                let py = top + (2 * row + 1) * (bottom - top) / (2 * rows);
                let index = (py * WIDTH + px) as usize;
                samples.push((index, i16::from(*pixels.get(index)?)));
            }
        }
        let count = samples.len() as f64;
        let mean = (samples.iter().map(|s| f64::from(s.1)).sum::<f64>() / count).round() as i16;
        for (_, value) in &mut samples {
            *value -= mean;
        }
        let energy = samples.iter().map(|s| f64::from(s.1).powi(2)).sum::<f64>();
        // A changed digit in a long price or title should not match solely
        // because most of the rest of the label is identical.
        let stripes = ((b.r - b.x) / 16).clamp(1, 8);
        let stripe = ((cols + stripes - 1) / stripes * rows) as usize;
        (energy > count * 64.).then_some(Self { samples, stripe })
    }
    pub(super) fn visible(&self, pixels: &[u8], tile: [i32; 2], crop: [u32; 4]) -> bool {
        // One canvas pixel of subpixel resampling is common between captures.
        let dx = (tile[0] / crop[0] as i32).max(1);
        let dy = (tile[1] / crop[1] as i32).max(1);
        [
            (0, 0),
            (-dx, 0),
            (dx, 0),
            (0, -dy),
            (0, dy),
            (-dx, -dy),
            (dx, -dy),
            (-dx, dy),
            (dx, dy),
        ]
        .iter()
        .any(|&(x, y)| {
            let mut strong = false;
            for stripe in self.samples.chunks(self.stripe) {
                let (mut sum, mut squares, mut cross, mut ref_sum, mut ref_squares) =
                    (0i64, 0i64, 0i64, 0i64, 0i64);
                for &(index, reference) in stripe {
                    let col = index as i32 % WIDTH + x;
                    let row = index as i32 / WIDTH + y;
                    if col < 0 || col >= tile[0] || row < 0 || row >= tile[1] {
                        return false;
                    }
                    let a = i64::from(reference);
                    let v = i64::from(pixels[(row * WIDTH + col) as usize]);
                    sum += v;
                    squares += v * v;
                    cross += a * v;
                    ref_sum += a;
                    ref_squares += a * a;
                }
                let n = stripe.len() as f64;
                let reference = ref_squares as f64 - (ref_sum as f64).powi(2) / n;
                if reference < n * 64. || stripe.len() < 12 {
                    continue;
                }
                let variance = squares as f64 - (sum as f64).powi(2) / n;
                let covariance = cross as f64 - ref_sum as f64 * sum as f64 / n;
                let score = covariance / (reference * variance).sqrt();
                if variance < n * 36. || score < 0.68 {
                    return false;
                }
                strong |= score >= 0.82;
            }
            strong
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_dimmed_strokes_but_rejects_a_missing_or_partly_replaced_word(
    ) -> Result<(), &'static str> {
        let tile = [100, 24];
        let crop = [100, 24, 410, 290];
        let b = Bounds {
            x: 420,
            y: 294,
            r: 500,
            b: 310,
        };
        let mut frame = vec![24; (WIDTH * tile[1]) as usize];
        for y in 4..20 {
            for x in 10..90 {
                frame[(y * WIDTH + x) as usize] = if (x + 3 * y) % 7 < 3 { 210 } else { 30 };
            }
        }
        let anchor = GlyphAnchor::capture(&frame, tile, crop, b).ok_or("missing anchor")?;
        let dim = frame.iter().map(|p| p / 3 + 15).collect::<Vec<_>>();
        assert!(anchor.visible(&dim, tile, crop));
        for y in 4..20 {
            frame[(y * WIDTH + 38) as usize..(y * WIDTH + 58) as usize].fill(52);
        }
        assert!(!anchor.visible(&frame, tile, crop));
        frame.fill(52);
        assert!(!anchor.visible(&frame, tile, crop));
        Ok(())
    }
}
