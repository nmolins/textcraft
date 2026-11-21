// The app's icon, drawn in code (no picture file needed): a terracotta tile with a
// cream notebook page, ruled ink lines, a red margin and a pen.
// Also included by build.rs to make the .exe icon, so it uses only `std`.

#[allow(dead_code)]
fn sd_round_rect(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> f32 {
    let cx = (x0 + x1) * 0.5;
    let cy = (y0 + y1) * 0.5;
    let hx = (x1 - x0) * 0.5 - r;
    let hy = (y1 - y0) * 0.5 - r;
    let dx = (px - cx).abs() - hx;
    let dy = (py - cy).abs() - hy;
    let ox = dx.max(0.0);
    let oy = dy.max(0.0);
    (ox * ox + oy * oy).sqrt() + dx.max(dy).min(0.0) - r
}

#[allow(dead_code)]
fn sd_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (pax, pay) = (px - ax, py - ay);
    let (bax, bay) = (bx - ax, by - ay);
    let h = ((pax * bax + pay * bay) / (bax * bax + bay * bay)).clamp(0.0, 1.0);
    let (dx, dy) = (pax - bax * h, pay - bay * h);
    (dx * dx + dy * dy).sqrt()
}

/// Colour of one point of the icon (unit square), or None when outside.
#[allow(dead_code)]
fn icon_sample(x: f32, y: f32) -> Option<[f32; 3]> {
    const TERRACOTTA: [f32; 3] = [0.769, 0.392, 0.227];
    const DARK_CLAY: [f32; 3] = [0.62, 0.30, 0.17];
    const PAPER: [f32; 3] = [0.984, 0.965, 0.918];
    const INK: [f32; 3] = [0.18, 0.165, 0.14];
    const MARGIN: [f32; 3] = [0.91, 0.647, 0.596];
    const RULE: [f32; 3] = [0.86, 0.80, 0.70];

    let mut c: Option<[f32; 3]> = None;
    // tile with a darker rim
    if sd_round_rect(x, y, 0.03, 0.03, 0.97, 0.97, 0.21) <= 0.0 {
        c = Some(DARK_CLAY);
    }
    if sd_round_rect(x, y, 0.06, 0.06, 0.94, 0.94, 0.18) <= 0.0 {
        c = Some(TERRACOTTA);
    }
    // page
    if sd_round_rect(x, y, 0.22, 0.15, 0.76, 0.85, 0.05) <= 0.0 {
        c = Some(PAPER);
        // ruled lines
        for ly in [0.40f32, 0.52, 0.64, 0.76] {
            if (y - ly).abs() < 0.008 {
                c = Some(RULE);
            }
        }
        // margin
        if (x - 0.31).abs() < 0.012 {
            c = Some(MARGIN);
        }
        // title line in terracotta, text lines in ink
        if sd_segment(x, y, 0.37, 0.28, 0.64, 0.28) < 0.032 {
            c = Some(TERRACOTTA);
        }
        for (ly, len) in [(0.40f32, 0.62f32), (0.52, 0.66), (0.64, 0.52)] {
            if sd_segment(x, y, 0.37, ly - 0.035, len, ly - 0.035) < 0.018 {
                c = Some(INK);
            }
        }
    }
    // pen across the corner
    let (ax, ay, bx, by) = (0.55f32, 0.82f32, 0.86f32, 0.40f32);
    if sd_segment(x, y, ax + 0.05, ay - 0.068, bx, by) < 0.058 {
        c = Some(PAPER); // light outline
    }
    if sd_segment(x, y, ax + 0.05, ay - 0.068, bx, by) < 0.04 {
        c = Some(INK);
    }
    if sd_segment(x, y, ax, ay, ax + 0.05, ay - 0.068) < 0.022 {
        c = Some(TERRACOTTA); // nib
    }
    c
}

/// RGBA pixels of the icon at `size`×`size`, smoothed with 4×4 samples per pixel.
#[allow(dead_code)]
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let n = 4;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for sy in 0..n {
                for sx in 0..n {
                    let x = (px as f32 + (sx as f32 + 0.5) / n as f32) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / n as f32) / size as f32;
                    if let Some(c) = icon_sample(x, y) {
                        r += c[0];
                        g += c[1];
                        b += c[2];
                        a += 1.0;
                    }
                }
            }
            if a > 0.0 {
                out.extend_from_slice(&[(r / a * 255.0) as u8, (g / a * 255.0) as u8, (b / a * 255.0) as u8, (a / (n * n) as f32 * 255.0) as u8]);
            } else {
                out.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    out
}
