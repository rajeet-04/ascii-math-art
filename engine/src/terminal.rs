use std::io::{self, Write};

/// Fit `cols`x`rows` to the terminal, keeping aspect, scaled by at most `max_scale`.
pub fn size(cols: usize, rows: usize, max_scale: f64) -> (usize, usize) {
    let environment = std::env::var("COLUMNS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .zip(
            std::env::var("LINES")
                .ok()
                .and_then(|s| s.parse::<usize>().ok()),
        );
    let size = environment
        .or_else(|| {
            use std::{
                io::IsTerminal,
                process::{Command, Stdio},
            };
            if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
                return None;
            }
            let result = Command::new("stty")
                .arg("size")
                .stdin(Stdio::inherit())
                .stderr(Stdio::null())
                .output()
                .ok()?;
            if !result.status.success() {
                return None;
            }
            let text = String::from_utf8(result.stdout).ok()?;
            let mut numbers = text.split_whitespace();
            let rows = numbers.next()?.parse().ok()?;
            let columns = numbers.next()?.parse().ok()?;
            Some((columns, rows))
        })
        .filter(|(columns, rows)| *columns > 0 && *rows > 1)
        .unwrap_or((cols, rows + 1));
    let scale = (size.0 as f64 / cols as f64)
        .min((size.1 - 1) as f64 / rows as f64)
        .min(max_scale);
    (
        (cols as f64 * scale).floor().max(1.0) as usize,
        (rows as f64 * scale).floor().max(1.0) as usize,
    )
}

pub fn sample(
    colors: &[[u8; 3]],
    cols: usize,
    rows: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> [u8; 3] {
    let (x0, y0) = (x * cols / width, y * rows / height);
    let (x1, y1) = (
        ((x + 1) * cols / width).max(x0 + 1),
        ((y + 1) * rows / height).max(y0 + 1),
    );
    let mut sum = [0_u64; 3];
    for sy in y0..y1 {
        for sx in x0..x1 {
            let color = colors[sy * cols + sx];
            for (total, value) in sum.iter_mut().zip(color) {
                *total += u64::from(value);
            }
        }
    }
    sum.map(|s| (s / ((x1 - x0) * (y1 - y0)) as u64) as u8)
}

// Glyph tiers from faint to dense; a cell picks its tier by brightness.
const POOLS: [&[u8]; 4] = [b".,'`:;", b"~-=+<>^\"", b"ocaesuzrv", b"#%&@$MW8"];
// ponytail: fixed thresholds tuned on two scenes; expose flags if a scene needs its own.
const EDGE: i32 = 100; // strongest per-channel change across two cells
const GLOW: i32 = 160; // only cells at least this bright leave a trail
const GLINTS: usize = 4; // twinkles per frame

fn luma(c: [u8; 3]) -> i32 {
    (54 * i32::from(c[0]) + 183 * i32::from(c[1]) + 19 * i32::from(c[2])) >> 8
}

fn scaled(c: [u8; 3], num: u16, den: u16, add: u16) -> [u8; 3] {
    c.map(|v| (u16::from(v) * num / den + add).min(255) as u8)
}

/// Signed change in whichever channel changes most, so hue-only edges count.
fn change(a: [u8; 3], b: [u8; 3]) -> i32 {
    (0..3)
        .map(|k| i32::from(a[k]) - i32::from(b[k]))
        .max_by_key(|d| d.abs())
        .unwrap()
}

/// `phase` counts quarter-seconds (shimmer and twinkle); `trail` carries glow
/// between frames and starts empty.
#[allow(clippy::too_many_arguments)]
pub fn frame(
    colors: &[[u8; 3]],
    source_cols: usize,
    source_rows: usize,
    columns: usize,
    rows: usize,
    blocks: bool,
    phase: usize,
    trail: &mut Vec<[u8; 3]>,
) -> Vec<u8> {
    let at = |x, y, height| sample(colors, source_cols, source_rows, x, y, columns, height);
    let mut cells: Vec<([u8; 3], [u8; 3], &[u8])> = Vec::with_capacity(columns * rows);
    if blocks {
        for y in 0..rows {
            for x in 0..columns {
                cells.push((
                    at(x, y * 2, rows * 2),
                    at(x, y * 2 + 1, rows * 2),
                    "▀".as_bytes(),
                ));
            }
        }
    } else {
        let mut grid: Vec<_> = (0..columns * rows)
            .map(|i| at(i % columns, i / columns, rows))
            .collect();
        // Trail: bright cells fade at half strength per frame.
        if trail.len() == grid.len() {
            for (c, t) in grid.iter_mut().zip(trail.iter()) {
                if luma(*t) >= GLOW {
                    *c = [0, 1, 2].map(|k| c[k].max(t[k] / 2));
                }
            }
        }
        trail.clone_from(&grid);
        let g = |x: usize, y: usize| grid[y.min(rows - 1) * columns + x.min(columns - 1)];
        let hash = |x: usize, y: usize| ((x * 73_856_093) ^ (y * 19_349_663)) % 9973;
        for y in 0..rows {
            for x in 0..columns {
                let color = g(x, y);
                let gx = change(g(x + 1, y), g(x.saturating_sub(1), y));
                let gy = change(g(x, y + 1), g(x, y.saturating_sub(1)));
                let (ax, ay) = (gx.abs(), gy.abs());
                cells.push(if ax.max(ay) > EDGE {
                    let glyph: &[u8] = if ax > 2 * ay {
                        b"|"
                    } else if ay > 2 * ax {
                        if gy < 0 { b"_" } else { b"-" }
                    } else if gx * gy > 0 {
                        b"/"
                    } else {
                        b"\\"
                    };
                    (scaled(color, 3, 2, 48), scaled(color, 3, 4, 0), glyph)
                } else if luma(color) < 16 {
                    (color, scaled(color, 3, 4, 0), b" ")
                } else {
                    let pool = POOLS[(luma(color) as usize * 4 / 256).min(3)];
                    let h = hash(x, y);
                    let i = (h + phase + h % 11) % pool.len();
                    (
                        scaled(color, 5, 4, 24),
                        scaled(color, 3, 4, 0),
                        &pool[i..=i],
                    )
                });
            }
        }
        // Glints: twinkling stars with short rays on the brightest strict local peaks.
        let mut peaks: Vec<_> = (1..rows.saturating_sub(1))
            .flat_map(|y| (1..columns.saturating_sub(1)).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let l = luma(g(x, y));
                l >= 200
                    && [(0, 1), (2, 1), (1, 0), (1, 2)]
                        .iter()
                        .all(|&(dx, dy)| luma(g(x + dx - 1, y + dy - 1)) < l)
            })
            .collect();
        peaks.sort_by_key(|&(x, y)| {
            std::cmp::Reverse(luma(g(x, y)) * 16 + ((hash(x, y) + phase * 7) % 16) as i32)
        });
        for &(x, y) in peaks.iter().take(GLINTS) {
            let reach = 1 + (hash(x, y) + phase) % 3;
            cells[y * columns + x].0 = [255; 3];
            cells[y * columns + x].2 = b"*";
            for d in 1..=reach {
                let fade = (255 - 160 * d / (reach + 1)) as u8;
                for (dx, dy, ray) in [(1, 0, b"-"), (-1, 0, b"-"), (0, 1, b"|"), (0, -1, b"|")] {
                    let (xx, yy) = (
                        x as isize + dx * d as isize * 2,
                        y as isize + dy * d as isize,
                    );
                    if (0..columns as isize).contains(&xx) && (0..rows as isize).contains(&yy) {
                        let c = &mut cells[yy as usize * columns + xx as usize];
                        c.0 = [fade; 3];
                        c.2 = ray;
                    }
                }
            }
        }
    }
    let mut out = b"\x1b[H".to_vec();
    let mut last = None;
    for (i, &(foreground, background, glyph)) in cells.iter().enumerate() {
        if i > 0 && i % columns == 0 {
            out.extend_from_slice(b"\r\n");
        }
        if last != Some((foreground, background)) {
            write!(
                out,
                "\x1b[38;2;{};{};{};48;2;{};{};{}m",
                foreground[0],
                foreground[1],
                foreground[2],
                background[0],
                background[1],
                background[2]
            )
            .unwrap();
            last = Some((foreground, background));
        }
        out.extend_from_slice(glyph);
    }
    out.extend_from_slice(b"\x1b[0m");
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn hue_edges_and_glints() {
        // Equal-ish luma, different hue: left half red, right half green-ish blue.
        let (w, h) = (16, 8);
        let mut colors: Vec<_> = (0..w * h)
            .map(|i| {
                if i % w < w / 2 {
                    [200, 40, 40]
                } else {
                    [40, 60, 200]
                }
            })
            .collect();
        colors[3 * w + 3] = [255; 3]; // one bright peak
        let mut trail = Vec::new();
        let text =
            String::from_utf8(super::frame(&colors, w, h, w, h, false, 0, &mut trail)).unwrap();
        assert!(text.contains('|'), "hue-only edge should be outlined");
        assert!(text.contains('*'), "bright peak should glint");
        assert_eq!(trail.len(), w * h);
    }
}
