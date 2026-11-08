use crate::{
    config::{Args, Layout, Mode},
    palette::Palette,
    render::{self, Cell, Glyphs, Stabilizer, Tone},
    scene_runtime::{self as runtime, TILE, Track},
    video,
};
use anyhow::{Context, Result, ensure};
use rayon::prelude::*;
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufReader, BufWriter, Write},
    process::Command,
    time::Instant,
};

const SPAN: usize = 24;
const RADIUS: isize = 6;

fn fit_curve(samples: &[isize]) -> [i16; 2] {
    if samples.len() < 2 {
        return [0, 0];
    }
    let (mut s2, mut s3, mut s4, mut v1, mut v2) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (i, value) in samples.iter().enumerate() {
        let u = i as f64 / (samples.len() - 1) as f64;
        s2 += u * u;
        s3 += u * u * u;
        s4 += u * u * u * u;
        v1 += u * *value as f64;
        v2 += u * u * *value as f64;
    }
    let determinant: f64 = s2 * s4 - s3 * s3;
    let (a, b) = if determinant.abs() < 1e-12 {
        (v1 / s2, 0.0)
    } else {
        (
            (v1 * s4 - v2 * s3) / determinant,
            (v2 * s2 - v1 * s3) / determinant,
        )
    };
    [a, b].map(|v| {
        (v * 16.0)
            .round()
            .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
    })
}

fn estimate_tracks(
    frames: &[Vec<Cell>],
    codes: &[Vec<u16>],
    cols: usize,
    rows: usize,
    radius: isize,
) -> Vec<Track> {
    let tiles_x = cols.div_ceil(TILE);
    let mut candidates: Vec<_> = (-radius..=radius)
        .flat_map(|dy| (-radius..=radius).map(move |dx| (dx, dy)))
        .collect();
    candidates.sort_by_key(|(dx, dy)| (dx.abs() + dy.abs(), *dy, *dx));
    (0..tiles_x * rows.div_ceil(TILE))
        .into_par_iter()
        .map(|tile| {
            let (x0, y0) = (tile % tiles_x * TILE, tile / tiles_x * TILE);
            let (x1, y1) = ((x0 + TILE).min(cols), (y0 + TILE).min(rows));
            let mut xs = vec![0];
            let mut ys = vec![0];
            for frame in &frames[1..] {
                let (mut best, mut shift) = (u64::MAX, (0, 0));
                for &(dx, dy) in &candidates {
                    let mut error = 0_u64;
                    'sample: for y in (y0..y1).step_by(2) {
                        for x in (x0..x1).step_by(2) {
                            let (sx, sy) = (x as isize + dx, y as isize + dy);
                            let reference =
                                if sx >= 0 && sy >= 0 && sx < cols as isize && sy < rows as isize {
                                    frames[0][sy as usize * cols + sx as usize].source
                                } else {
                                    [0; 3]
                                };
                            let current = frame[y * cols + x].source;
                            error += current
                                .iter()
                                .zip(reference)
                                .map(|(a, b)| {
                                    let d = i32::from(*a) - i32::from(b);
                                    (d * d) as u64
                                })
                                .sum::<u64>();
                            if error >= best {
                                break 'sample;
                            }
                        }
                    }
                    if error < best {
                        best = error;
                        shift = (dx, dy);
                    }
                    if best == 0 {
                        break;
                    }
                }
                xs.push(shift.0);
                ys.push(shift.1);
            }
            let [a, b] = fit_curve(&xs);
            let [c, d] = fit_curve(&ys);
            let track = Track {
                coeff: [a, b, c, d],
            };
            let mut saved = 0_isize;
            for (f, frame) in codes.iter().enumerate().skip(1) {
                let (dx, dy) = track.offset(f, codes.len());
                for y in y0..y1 {
                    for x in x0..x1 {
                        let i = y * cols + x;
                        let (sx, sy) = (x as isize + dx, y as isize + dy);
                        let predicted =
                            if sx >= 0 && sy >= 0 && sx < cols as isize && sy < rows as isize {
                                codes[0][sy as usize * cols + sx as usize]
                            } else {
                                0
                            };
                        saved += isize::from(predicted == frame[i])
                            - isize::from(codes[0][i] == frame[i]);
                    }
                }
            }
            // shortcut: select by literal-cell savings; compare full encoded cost if model overhead dominates.
            if saved * 2 > 8 {
                track
            } else {
                Track::default()
            }
        })
        .collect()
}

fn varint(mut value: usize, output: &mut Vec<u8>) {
    while value >= 128 {
        output.push((value as u8 & 127) | 128);
        value >>= 7;
    }
    output.push(value as u8);
}

struct Span {
    bytes: Vec<u8>,
    reference_bytes: usize,
    model_bytes: usize,
    correction_bytes: usize,
    correction_cells: usize,
    baseline_corrections: usize,
}

fn encode_span(frames: &[Vec<u16>], tracks: &[Track], cols: usize, rows: usize) -> Span {
    let mut bytes = (frames.len() as u16).to_le_bytes().to_vec();
    for code in &frames[0] {
        bytes.extend_from_slice(&code.to_le_bytes());
    }
    let reference_bytes = frames[0].len() * 2;
    let before = bytes.len();
    for track in tracks {
        let moving = track.coeff != [0; 4];
        bytes.push(u8::from(moving));
        if moving {
            for c in track.coeff {
                bytes.extend_from_slice(&c.to_le_bytes());
            }
        }
    }
    let model_bytes = bytes.len() - before;
    let before = bytes.len();
    let (mut correction_cells, mut baseline_corrections) = (0, 0);
    for (f, frame) in frames.iter().enumerate().skip(1) {
        let prediction = runtime::predict(&frames[0], tracks, f, frames.len(), cols, rows);
        baseline_corrections += frame.iter().zip(&frames[0]).filter(|(a, b)| a != b).count();
        let mut cursor = 0;
        while cursor < frame.len() {
            let start = cursor;
            while cursor < frame.len() && frame[cursor] == prediction[cursor] {
                cursor += 1;
            }
            varint(cursor - start, &mut bytes);
            if cursor == frame.len() {
                break;
            }
            let start = cursor;
            while cursor < frame.len() && frame[cursor] != prediction[cursor] {
                cursor += 1;
            }
            varint(cursor - start, &mut bytes);
            correction_cells += cursor - start;
            for code in &frame[start..cursor] {
                bytes.extend_from_slice(&code.to_le_bytes());
            }
        }
    }
    let correction_bytes = bytes.len() - before;
    Span {
        bytes,
        reference_bytes,
        model_bytes,
        correction_bytes,
        correction_cells,
        baseline_corrections,
    }
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for q in bytes.chunks(3) {
        let v = (u32::from(q[0]) << 16)
            | (u32::from(*q.get(1).unwrap_or(&0)) << 8)
            | u32::from(*q.get(2).unwrap_or(&0));
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            out.push(if i > q.len() {
                '='
            } else {
                TABLE[((v >> shift) & 63) as usize] as char
            });
        }
    }
    out
}

pub fn export(
    args: &Args,
    layout: &Layout,
    stage: &video::Stage,
    pool: &rayon::ThreadPool,
    palette: &Palette,
    tone: &Tone,
    started: Instant,
) -> Result<()> {
    let n = layout.cols * layout.rows;
    ensure!(
        n * SPAN * 32 + 2 * 1024 * 1024 <= 256 * 1024 * 1024,
        "scene fitting workspace exceeds 256 MiB; reduce columns or rows"
    );
    let colors: Vec<_> = if args.monochrome && args.mode == Mode::Ascii {
        (0..=255).map(|v| [v; 3]).collect()
    } else {
        palette.colors().to_vec()
    };
    let color_ids: HashMap<_, _> = colors
        .iter()
        .enumerate()
        .map(|(i, c)| (*c, i as u16))
        .collect();
    let mut decoder = video::decoder(args, layout, stage)?;
    let mut input = BufReader::new(
        decoder
            .child()
            .stdout
            .take()
            .context("decoder stdout missing")?,
    );
    let body_path = stage.root.join("blocks.rs");
    let mut body = BufWriter::new(File::create(&body_path)?);
    let mut history = Stabilizer::default();
    let (mut count, mut hash, mut chunks, mut moving) = (0_usize, runtime::HASH_START, 0, 0);
    let (
        mut compressed,
        mut references,
        mut models,
        mut corrections,
        mut corrected_cells,
        mut stationary_cells,
        mut predicted_cells,
    ) = (0, 0, 0, 0, 0, 0, 0);
    loop {
        let mut raw = Vec::with_capacity(SPAN);
        for _ in 0..SPAN {
            match video::read_frame(&mut input, layout.input_bytes)? {
                Some(frame) => raw.push(frame),
                None => break,
            }
        }
        if raw.is_empty() {
            break;
        }
        let mut frames: Vec<_> = pool.install(|| {
            raw.par_iter()
                .map(|rgb| render::analyze(rgb, layout, args, palette, tone))
                .collect()
        });
        drop(raw);
        for frame in &mut frames {
            history.apply(frame, args.stability);
        }
        let codes: Vec<Vec<u16>> = frames
            .iter()
            .map(|frame| {
                frame
                    .iter()
                    .map(|cell| {
                        (color_ids[&cell.color] << 7)
                            | u16::from(if args.mode == Mode::Ascii {
                                cell.glyph
                            } else {
                                b' '
                            })
                    })
                    .collect()
            })
            .collect();
        for frame in &codes {
            runtime::hash_cells(&mut hash, frame);
        }
        let tracks =
            pool.install(|| estimate_tracks(&frames, &codes, layout.cols, layout.rows, RADIUS));
        let span = encode_span(&codes, &tracks, layout.cols, layout.rows);
        // Check the actual emitted equations and corrections before publication.
        let font = Glyphs::new();
        let header = runtime::Header {
            cols: layout.cols,
            rows: layout.rows,
            cell_w: layout.cell_w,
            cell_h: layout.cell_h,
            fps: args.fps,
            ascii: args.mode == Mode::Ascii,
            palette: &colors,
            font: font.bitmap(),
        };
        ensure!(
            runtime::decode_block(&span.bytes, &header)? == codes,
            "internal scene reconstruction mismatch"
        );
        let path = stage.root.join("span.raw");
        fs::write(&path, &span.bytes)?;
        let packed = Command::new("gzip")
            .arg("-cn")
            .arg(&path)
            .output()
            .context("run gzip (required for scene export)")?;
        ensure!(
            packed.status.success(),
            "gzip failed: {}",
            String::from_utf8_lossy(&packed.stderr)
        );
        writeln!(
            body,
            "// Reference span {chunks}, frames {count}..{}",
            count + codes.len() - 1
        )?;
        for (tile, track) in tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.coeff != [0; 4])
            .take(3)
        {
            let [a, b, c, d] = track.coeff.map(|v| f64::from(v) / 16.0);
            writeln!(
                body,
                "// Patch {tile}: dx(u)={a:.4}*u{b:+.4}*u*u; dy(u)={c:.4}*u{d:+.4}*u*u"
            )?;
        }
        writeln!(body, "r\"")?;
        let encoded = base64(&packed.stdout);
        for line in encoded.as_bytes().chunks(100) {
            body.write_all(line)?;
            body.write_all(b"\n")?;
        }
        writeln!(body, "\",")?;
        compressed += packed.stdout.len();
        references += span.reference_bytes;
        models += span.model_bytes;
        corrections += span.correction_bytes;
        corrected_cells += span.correction_cells;
        stationary_cells += span.baseline_corrections;
        predicted_cells += n * (codes.len() - 1);
        count += codes.len();
        chunks += 1;
        moving += tracks.iter().filter(|t| t.coeff != [0; 4]).count();
        eprintln!(
            "Fitted {count} frames: {moving} moving patch curves, {compressed} compressed bytes"
        );
    }
    drop(input);
    decoder.finish()?;
    ensure!(count > 0, "input produced no scene frames");
    body.flush()?;
    drop(body);
    let font = Glyphs::new();
    let mut source = BufWriter::new(File::create(&stage.artifact)?);
    writeln!(
        source,
        "// Motion scene: {count} frames, {}x{} cells, {} FPS",
        layout.cols, layout.rows, args.fps
    )?;
    writeln!(
        source,
        "// Exact converted-cell hash: {hash:016x}. Embedded data counts toward source size."
    )?;
    writeln!(
        source,
        "// {references} reference bytes; {models} model bytes; {corrections} correction bytes before gzip."
    )?;
    writeln!(
        source,
        "mod terminal {{\n{}\n}}",
        include_str!("terminal.rs")
    )?;
    writeln!(
        source,
        "mod scene_runtime {{\n{}\n}}",
        include_str!("scene_runtime.rs")
    )?;
    writeln!(source, "static PALETTE: &[[u8;3]] = &{colors:?};")?;
    writeln!(source, "static FONT: [[u8;8];128] = {:?};", font.bitmap())?;
    writeln!(source, "static BLOCKS: &[&str] = &[")?;
    std::io::copy(&mut File::open(body_path)?, &mut source)?;
    writeln!(
        source,
        "];\nfn main() {{\n let h=scene_runtime::Header {{ cols:{}, rows:{}, cell_w:{}, cell_h:{}, fps:{}, ascii:{}, palette:PALETTE, font:&FONT }};\n if let Err(e)=scene_runtime::play(BLOCKS,&h,{count},{hash}) {{ eprintln!(\"error: {{e}}\"); std::process::exit(1); }}\n}}",
        layout.cols,
        layout.rows,
        layout.cell_w,
        layout.cell_h,
        args.fps,
        args.mode == Mode::Ascii
    )?;
    source.flush()?;
    drop(source);
    let bytes = stage.bytes()?;
    stage.publish()?;
    let coverage = if predicted_cells == 0 {
        100.0
    } else {
        100.0 * (predicted_cells - corrected_cells) as f64 / predicted_cells as f64
    };
    eprintln!(
        "Scene: {count} frames, {chunks} references, {moving} moving curves; {coverage:.1}% cells predicted exactly before corrections"
    );
    eprintln!(
        "Data: {references} reference + {models} motion + {corrections} correction bytes → {compressed} gzip bytes; {bytes} total source bytes; {} raw grid bytes",
        count * n * 2
    );
    eprintln!(
        "Motion saved {} corrected cells versus stationary references; exact hash {hash:016x}; {:.3}s → {}",
        stationary_cells.saturating_sub(corrected_cells),
        started.elapsed().as_secs_f64(),
        args.output.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        render::Cell,
        scene_runtime::{self as runtime, Header},
    };

    fn fixture() -> (Vec<Vec<Cell>>, Vec<Vec<u16>>) {
        let reference: Vec<_> = (0..512)
            .map(|i| {
                let v = ((i % 32 * 13 + i / 32 * 37) % 256) as u8;
                Cell {
                    source: [v; 3],
                    color: [v; 3],
                    glyph: b'A',
                }
            })
            .collect();
        let frames: Vec<_> = (0..4)
            .map(|f| {
                (0..512)
                    .map(|i| {
                        if i % 32 + f < 32 {
                            reference[i + f]
                        } else {
                            Cell {
                                source: [0; 3],
                                color: [0; 3],
                                glyph: b' ',
                            }
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let codes = frames
            .iter()
            .map(|f| {
                f.iter()
                    .map(|c| (u16::from(c.color[0]) << 7) | u16::from(c.glyph))
                    .collect()
            })
            .collect();
        (frames, codes)
    }

    #[test]
    fn quadratic_fit_recovers_translation_and_acceleration() {
        assert_eq!(fit_curve(&[0, 1, 2, 3]), [48, 0]);
        assert_eq!(fit_curve(&[0, 1, 4, 9]), [0, 144]);
        assert_eq!(fit_curve(&[0]), [0, 0]);
    }

    #[test]
    fn fitted_motion_saves_corrections_and_reconstructs_exact_cells() {
        let (frames, mut codes) = fixture();
        codes[2][64] = 0; // New appearance that motion alone cannot predict.
        let tracks = estimate_tracks(&frames, &codes, 32, 16, 3);
        assert!(tracks.iter().any(|t| t.coeff != [0; 4]));
        let span = encode_span(&codes, &tracks, 32, 16);
        assert!(span.correction_cells < span.baseline_corrections);
        let palette: Vec<_> = (0..=255).map(|v| [v; 3]).collect();
        let font = [[0; 8]; 128];
        let header = Header {
            cols: 32,
            rows: 16,
            cell_w: 8,
            cell_h: 16,
            fps: 20,
            ascii: true,
            palette: &palette,
            font: &font,
        };
        assert_eq!(runtime::decode_block(&span.bytes, &header).unwrap(), codes);
        let mut bad = span.bytes.clone();
        bad[2..4].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(runtime::decode_block(&bad, &header).is_err());
        let mut bad = span.bytes.clone();
        bad[2 + 512 * 2] = 2;
        assert!(runtime::decode_block(&bad, &header).is_err());
        let mut bad = span.bytes[..2 + 512 * 2 + span.model_bytes].to_vec();
        bad.extend_from_slice(&[0, 0]);
        assert!(runtime::decode_block(&bad, &header).is_err());
        assert!(runtime::decode_block(&span.bytes[..span.bytes.len() - 1], &header).is_err());
        let single = encode_span(&codes[..1], &[runtime::Track::default(); 8], 32, 16);
        assert_eq!(
            runtime::decode_block(&single.bytes, &header).unwrap(),
            codes[..1]
        );
    }

    #[test]
    fn base64_roundtrip_and_corrupt_payload_detection() {
        for input in [vec![], vec![b'f'], vec![b'f', b'o'], (0..=255).collect()] {
            let text = base64(&input);
            assert_eq!(runtime::base64_decode(&text).unwrap(), input);
        }
        assert_eq!(base64(b"foo"), "Zm9v");
        assert!(runtime::base64_decode("Zg=").is_err());
        assert!(runtime::base64_decode("====").is_err());
        assert!(runtime::inflate(vec![0, 1, 2]).is_err());
    }

    #[test]
    fn terminal_downsampling_keeps_average_color_instead_of_one_cell() {
        let colors = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255]];
        assert_eq!(crate::terminal::sample(&colors, 2, 2, 0, 0, 1, 1), [127; 3]);
        assert_eq!(
            crate::terminal::sample(&colors, 2, 2, 0, 0, 2, 4),
            [255, 0, 0]
        );
        assert_eq!(crate::terminal::sample(&colors, 2, 2, 1, 3, 2, 4), [255; 3]);
    }
}
