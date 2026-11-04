//! Donut chart rendered locally to PNG. Slice colours match the coloured
//! square emoji used in the caption legend, so no fonts are needed.

pub(super) const LEGEND: [&str; 8] = ["🟥", "🟧", "🟨", "🟩", "🟦", "🟪", "🟫", "⬛"];
const COLORS: [[u8; 3]; 8] = [
    [221, 46, 68],
    [244, 144, 12],
    [253, 203, 88],
    [120, 177, 89],
    [85, 172, 238],
    [170, 142, 214],
    [193, 105, 79],
    [49, 55, 61],
];

const SIZE: u32 = 640;

/// Renders slices (at most 8, non-negative values) as a PNG donut.
pub(super) fn donut_png(values: &[i64]) -> Result<Vec<u8>, String> {
    let values: Vec<i64> = values.iter().take(8).map(|v| (*v).max(0)).collect();
    let total: i128 = values.iter().map(|v| *v as i128).sum();
    if total <= 0 {
        return Err("nothing to chart".into());
    }
    // Cumulative slice boundaries as fractions of the full turn.
    let mut bounds = Vec::with_capacity(values.len());
    let mut acc = 0i128;
    for v in &values {
        acc += *v as i128;
        bounds.push(acc as f64 / total as f64);
    }
    let center = SIZE as f64 / 2.0;
    let (outer, inner) = (center * 0.9, center * 0.48);
    let bg = [255u8, 255, 255];
    let mut data = Vec::with_capacity((SIZE * SIZE * 3) as usize);
    const SUB: [f64; 2] = [0.25, 0.75];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let mut rgb = [0u32; 3];
            for sy in SUB {
                for sx in SUB {
                    let dx = x as f64 + sx - center;
                    let dy = y as f64 + sy - center;
                    let r = (dx * dx + dy * dy).sqrt();
                    let color = if r > outer || r < inner {
                        bg
                    } else {
                        // Clockwise from 12 o'clock.
                        let turn = (dx.atan2(-dy) / std::f64::consts::TAU).rem_euclid(1.0);
                        let i = bounds
                            .iter()
                            .position(|b| turn < *b)
                            .unwrap_or(values.len() - 1);
                        COLORS[i]
                    };
                    for c in 0..3 {
                        rgb[c] += color[c] as u32;
                    }
                }
            }
            data.extend(rgb.iter().map(|c| (c / 4) as u8));
        }
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, SIZE, SIZE);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&data).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_png() {
        let png = donut_png(&[500, 300, 200]).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert!(donut_png(&[0, 0]).is_err());
        assert!(donut_png(&[]).is_err());
    }
}
