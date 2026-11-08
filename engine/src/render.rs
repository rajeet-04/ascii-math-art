use crate::{
    config::{Args, Layout, Mode},
    palette::Palette,
};
use font8x8::{BASIC_FONTS, UnicodeFonts};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub source: [u8; 3],
    pub color: [u8; 3],
    pub glyph: u8,
}

fn luma(c: &[u8]) -> i16 {
    ((54 * u32::from(c[0]) + 183 * u32::from(c[1]) + 19 * u32::from(c[2])) >> 8) as i16
}

pub struct Tone([u8; 256]);

impl Tone {
    pub fn new(gamma: f64) -> Self {
        Self(std::array::from_fn(|i| {
            ((i as f64 / 255.0).powf(gamma) * 255.0).round() as u8
        }))
    }
    pub fn color(&self, rgb: &[u8]) -> [u8; 3] {
        [
            self.0[rgb[0] as usize],
            self.0[rgb[1] as usize],
            self.0[rgb[2] as usize],
        ]
    }
}

pub fn analyze(
    rgb: &[u8],
    layout: &Layout,
    args: &Args,
    palette: &Palette,
    tone: &Tone,
) -> Vec<Cell> {
    const RAMP: &[u8] = b" .,:;irsXA253hMHGS#9B&@";
    let at = |x: usize, y: usize| luma(&tone.color(&rgb[(y * layout.cols + x) * 3..][..3]));
    rgb.as_chunks::<3>()
        .0
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let (x, y) = (i % layout.cols, i / layout.cols);
            let source = [p[0], p[1], p[2]];
            let corrected = tone.color(p);
            let value = luma(&corrected);
            let mut glyph = RAMP[value as usize * (RAMP.len() - 1) / 255];
            if args.mode == Mode::Ascii && args.edge_threshold < 255 {
                let gx = at((x + 1).min(layout.cols - 1), y) - at(x.saturating_sub(1), y);
                let gy = at(x, (y + 1).min(layout.rows - 1)) - at(x, y.saturating_sub(1));
                if gx.abs() + gy.abs() > i16::from(args.edge_threshold) {
                    glyph = edge_glyph(gx, gy);
                }
            }
            let color = if args.monochrome && args.mode == Mode::Ascii {
                [value as u8; 3]
            } else {
                palette.color(corrected, x, y, args.dither)
            };
            Cell {
                source,
                color,
                glyph,
            }
        })
        .collect()
}

fn edge_glyph(gx: i16, gy: i16) -> u8 {
    if gx.abs() > 2 * gy.abs() {
        b'|'
    } else if gy.abs() > 2 * gx.abs() {
        b'-'
    } else if i32::from(gx) * i32::from(gy) > 0 {
        b'/'
    } else {
        b'\\'
    }
}

#[derive(Default)]
pub struct Stabilizer {
    accepted: Vec<Cell>,
}

impl Stabilizer {
    pub fn apply(&mut self, cells: &mut [Cell], threshold: u8) {
        if self.accepted.len() == cells.len() && threshold > 0 {
            // Reset at large cuts; never blend unrelated scenes together.
            let changed = cells
                .iter()
                .zip(&self.accepted)
                .filter(|(a, b)| {
                    a.source
                        .iter()
                        .zip(b.source)
                        .map(|(x, y)| u32::from(x.abs_diff(y)))
                        .sum::<u32>()
                        > 144
                })
                .count();
            if changed <= cells.len() / 2 {
                for (cell, previous) in cells.iter_mut().zip(&self.accepted) {
                    if cell
                        .source
                        .iter()
                        .zip(previous.source)
                        .all(|(a, b)| a.abs_diff(b) < threshold)
                    {
                        // Keep the accepted source too, so slow changes accumulate.
                        *cell = *previous;
                    }
                }
            }
        }
        self.accepted.clear();
        self.accepted.extend_from_slice(cells);
    }
}

pub struct Glyphs([[u8; 8]; 128]);

impl Glyphs {
    pub fn bitmap(&self) -> &[[u8; 8]; 128] {
        &self.0
    }
    pub fn new() -> Self {
        Self(std::array::from_fn(|i| {
            BASIC_FONTS.get(char::from(i as u8)).unwrap_or([0; 8])
        }))
    }

    pub fn render(&self, cells: &[Cell], layout: &Layout, mode: Mode) -> Vec<u8> {
        let mut rgb = vec![0; layout.output_bytes];
        for y in 0..layout.height {
            let cy = y / layout.cell_h;
            let gy = (y % layout.cell_h) * 8 / layout.cell_h;
            let row = &mut rgb[y * layout.width * 3..(y + 1) * layout.width * 3];
            for (cx, cell) in cells[cy * layout.cols..(cy + 1) * layout.cols]
                .iter()
                .enumerate()
            {
                let bits = self.0[cell.glyph as usize][gy];
                for dx in 0..layout.cell_w {
                    let gx = dx * 8 / layout.cell_w;
                    if mode == Mode::Pixel || bits & (1 << gx) != 0 {
                        let x = (cx * layout.cell_w + dx) * 3;
                        row[x..x + 3].copy_from_slice(&cell.color);
                    }
                }
            }
        }
        rgb
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    #[test]
    fn gamma_lifts_shadows_without_clipping_endpoints() {
        let tone = Tone::new(0.6);
        assert_eq!(tone.color(&[0; 3]), [0; 3]);
        assert_eq!(tone.color(&[255; 3]), [255; 3]);
        assert!(tone.color(&[32; 3])[0] > 65);
        assert_eq!(Tone::new(1.0).color(&[32, 128, 240]), [32, 128, 240]);
    }
    fn cell(v: u8) -> Cell {
        Cell {
            source: [v; 3],
            color: [v; 3],
            glyph: b'A',
        }
    }
    #[test]
    fn hysteresis_holds_noise_but_allows_accumulated_motion() {
        let mut s = Stabilizer::default();
        let mut a = [cell(100)];
        s.apply(&mut a, 12);
        let mut b = [cell(105)];
        s.apply(&mut b, 12);
        assert_eq!(b[0], cell(100));
        let mut c = [cell(113)];
        s.apply(&mut c, 12);
        assert_eq!(c[0], cell(113));
        let mut d = [cell(114)];
        s.apply(&mut d, 0);
        assert_eq!(d[0], cell(114));
    }
    #[test]
    fn cuts_reset_history_including_small_changes() {
        let mut s = Stabilizer::default();
        let mut old = [cell(0); 3];
        s.apply(&mut old, 12);
        let mut new = [cell(255), cell(255), cell(3)];
        s.apply(&mut new, 12);
        assert_eq!(new[2].source, [3; 3]);
    }
    #[test]
    fn outlines_follow_gradient_direction() {
        assert_eq!(edge_glyph(100, 0), b'|');
        assert_eq!(edge_glyph(0, 100), b'-');
        assert_eq!(edge_glyph(100, 100), b'/');
        assert_eq!(edge_glyph(-100, 100), b'\\');
    }
    #[test]
    fn pixel_render_is_exact_nearest_neighbor() {
        let args = Args::parse_from([
            "art",
            "in",
            "out.mp4",
            "--columns",
            "2",
            "--rows",
            "1",
            "--scale",
            "2",
        ]);
        let l = args.layout(2.0).unwrap();
        let data = Glyphs::new().render(&[cell(10), cell(240)], &l, Mode::Pixel);
        assert_eq!(data.len(), 4 * 2 * 3);
        assert_eq!(&data[0..6], &[10; 6]);
        assert_eq!(&data[6..12], &[240; 6]);
        assert_eq!(&data[..12], &data[12..]);
    }
    #[test]
    fn ascii_matches_cached_font_orientation() {
        let args = Args::parse_from([
            "art",
            "in",
            "out.mp4",
            "--mode",
            "ascii",
            "--columns",
            "1",
            "--rows",
            "1",
        ]);
        let l = args.layout(1.0).unwrap();
        let glyphs = Glyphs::new();
        let mut c = cell(255);
        c.glyph = b'F';
        let data = glyphs.render(&[c], &l, Mode::Ascii);
        for y in 0..16 {
            for x in 0..8 {
                let expected = if BASIC_FONTS.get('F').unwrap()[y / 2] & (1 << x) != 0 {
                    255
                } else {
                    0
                };
                assert_eq!(data[(y * 8 + x) * 3], expected);
            }
        }
    }
}
