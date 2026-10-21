//! Picks a vivid accent colour from an image, for theming the UI.

use image::imageops::FilterType;
use std::path::Path;

fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max == 0.0 { 0.0 } else { d / max };
    ((h + 360.0) % 360.0, s, max)
}

/// Returns `#rrggbb`. Buckets pixels by hue and picks the bucket with the most
/// "colourful mass" so a small vivid area can beat a large grey one.
pub fn from_file(path: &Path) -> Option<String> {
    let img = image::open(path).ok()?.resize(96, 96, FilterType::Triangle).to_rgb8();
    const BUCKETS: usize = 24;
    let mut weight = [0f32; BUCKETS];
    let mut sum = [[0f32; 3]; BUCKETS];
    let mut gray = [0f32; 4];

    for p in img.pixels() {
        let (r, g, b) = (p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0);
        let (h, s, v) = rgb_to_hsv(r, g, b);
        gray[0] += r;
        gray[1] += g;
        gray[2] += b;
        gray[3] += 1.0;
        if s < 0.18 || v < 0.15 || v > 0.98 {
            continue;
        }
        let w = s * s * v;
        let i = ((h / 360.0) * BUCKETS as f32) as usize % BUCKETS;
        weight[i] += w;
        sum[i][0] += r * w;
        sum[i][1] += g * w;
        sum[i][2] += b * w;
    }

    let (best, &bw) = weight.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
    let (r, g, b) = if bw > 1.0 {
        (sum[best][0] / bw, sum[best][1] / bw, sum[best][2] / bw)
    } else {
        // Nearly monochrome image: fall back to its average colour.
        (gray[0] / gray[3], gray[1] / gray[3], gray[2] / gray[3])
    };
    Some(format!(
        "#{:02x}{:02x}{:02x}",
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8
    ))
}
