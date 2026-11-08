use crate::config::PaletteKind;

pub struct Palette {
    colors: Vec<[u8; 3]>,
    // 5 bits per channel: one table lookup per cell, independent of palette size.
    lookup: Vec<u8>,
}

impl Palette {
    pub fn colors(&self) -> &[[u8; 3]] {
        &self.colors
    }
    pub fn new(kind: PaletteKind) -> Self {
        let colors = match kind {
            PaletteKind::Pico8 => vec![
                [0, 0, 0],
                [29, 43, 83],
                [126, 37, 83],
                [0, 135, 81],
                [171, 82, 54],
                [95, 87, 79],
                [194, 195, 199],
                [255, 241, 232],
                [255, 0, 77],
                [255, 163, 0],
                [255, 236, 39],
                [0, 228, 54],
                [41, 173, 255],
                [131, 118, 156],
                [255, 119, 168],
                [255, 204, 170],
            ],
            PaletteKind::Grayscale => (0..16).map(|i| [i * 17; 3]).collect(),
            PaletteKind::Rgb332 => (0..256_u16)
                .map(|i| {
                    [
                        (((i >> 5) & 7) * 255 / 7) as u8,
                        (((i >> 2) & 7) * 255 / 7) as u8,
                        ((i & 3) * 255 / 3) as u8,
                    ]
                })
                .collect(),
        };
        let lookup = (0..32768)
            .map(|i| {
                let rgb = [
                    ((i >> 10) & 31) * 8 + 4,
                    ((i >> 5) & 31) * 8 + 4,
                    (i & 31) * 8 + 4,
                ];
                colors
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, c)| {
                        let d = [
                            rgb[0] - c[0] as i32,
                            rgb[1] - c[1] as i32,
                            rgb[2] - c[2] as i32,
                        ];
                        2 * d[0] * d[0] + 3 * d[1] * d[1] + d[2] * d[2]
                    })
                    .unwrap()
                    .0 as u8
            })
            .collect();
        Self { colors, lookup }
    }

    pub fn color(&self, rgb: [u8; 3], x: usize, y: usize, dither: bool) -> [u8; 3] {
        const BAYER: [[i16; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
        let offset = if dither {
            (BAYER[y % 4][x % 4] * 2 - 15) * 2
        } else {
            0
        };
        let c = rgb.map(|v| (i16::from(v) + offset).clamp(0, 255) as usize >> 3);
        self.colors[self.lookup[c[0] << 10 | c[1] << 5 | c[2]] as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints_and_palette_membership() {
        for kind in [
            PaletteKind::Pico8,
            PaletteKind::Grayscale,
            PaletteKind::Rgb332,
        ] {
            let p = Palette::new(kind);
            assert_eq!(p.color([0; 3], 0, 0, false), [0; 3]);
            assert!(p.colors.contains(&p.color([123, 58, 210], 0, 0, false)));
        }
        assert_eq!(
            Palette::new(PaletteKind::Grayscale).color([255; 3], 0, 0, false),
            [255; 3]
        );
    }
    #[test]
    fn dithering_is_screen_anchored_and_repeatable() {
        let p = Palette::new(PaletteKind::Grayscale);
        assert_eq!(p.color([100; 3], 1, 2, true), p.color([100; 3], 5, 6, true));
        assert_ne!(p.color([100; 3], 0, 0, true), p.color([100; 3], 0, 1, true));
    }
}
