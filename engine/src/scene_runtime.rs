// This module is also emitted verbatim into standalone scene programs.
// It deliberately depends only on the Rust standard library.
use std::io::{self, Read, Write};

const MAX_BLOCK: usize = 256 * 1024 * 1024;
pub const TILE: usize = 8;
pub const HASH_START: u64 = 0xcbf29ce484222325;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub struct Header<'a> {
    pub cols: usize,
    pub rows: usize,
    pub cell_w: usize,
    pub cell_h: usize,
    pub fps: u32,
    pub ascii: bool,
    pub palette: &'a [[u8; 3]],
    pub font: &'a [[u8; 8]; 128],
}

impl Header<'_> {
    pub fn validate(&self) -> io::Result<()> {
        if !(1..=2048).contains(&self.cols)
            || !(1..=2048).contains(&self.rows)
            || !(1..=128).contains(&self.cell_w)
            || !(1..=256).contains(&self.cell_h)
            || self.cols * self.cell_w > 8192
            || self.rows * self.cell_h > 8192
            || !(1..=120).contains(&self.fps)
            || self.palette.is_empty()
            || self.palette.len() > 256
        {
            return Err(invalid("invalid scene geometry, FPS or palette"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Track {
    /// Fixed point, 1/16 cell: dx(u)=a*u+b*u², dy(u)=c*u+d*u².
    pub coeff: [i16; 4],
}

impl Track {
    pub fn offset(&self, frame: usize, count: usize) -> (isize, isize) {
        let u = frame as f64 / count.saturating_sub(1).max(1) as f64;
        let [a, b, c, d] = self.coeff.map(f64::from);
        (
            ((a * u + b * u * u) / 16.0).round() as isize,
            ((c * u + d * u * u) / 16.0).round() as isize,
        )
    }
}

pub fn predict(
    reference: &[u16],
    tracks: &[Track],
    frame: usize,
    count: usize,
    cols: usize,
    rows: usize,
) -> Vec<u16> {
    let tiles_x = cols.div_ceil(TILE);
    let mut result = vec![0; cols * rows];
    for (i, code) in result.iter_mut().enumerate() {
        let (x, y) = (i % cols, i / cols);
        let (dx, dy) = tracks[y / TILE * tiles_x + x / TILE].offset(frame, count);
        let (sx, sy) = (x as isize + dx, y as isize + dy);
        if sx >= 0 && sy >= 0 && sx < cols as isize && sy < rows as isize {
            *code = reference[sy as usize * cols + sx as usize];
        }
    }
    result
}

pub fn hash_cells(hash: &mut u64, cells: &[u16]) {
    for cell in cells {
        for byte in cell.to_le_bytes() {
            *hash = (*hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
}

struct Reader<'a> {
    data: &'a [u8],
    cursor: usize,
}
impl<'a> Reader<'a> {
    fn bytes(&mut self, count: usize) -> io::Result<&'a [u8]> {
        let end = self
            .cursor
            .checked_add(count)
            .filter(|n| *n <= self.data.len())
            .ok_or_else(|| invalid("truncated scene block"))?;
        let result = &self.data[self.cursor..end];
        self.cursor = end;
        Ok(result)
    }
    fn byte(&mut self) -> io::Result<u8> {
        Ok(self.bytes(1)?[0])
    }
    fn word(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(2)?.try_into().unwrap()))
    }
    fn varint(&mut self) -> io::Result<usize> {
        let mut result = 0;
        for shift in (0..35).step_by(7) {
            let b = self.byte()?;
            if shift == 28 && b > 15 {
                return Err(invalid("oversized correction run"));
            }
            result |= usize::from(b & 127) << shift;
            if b < 128 {
                return Ok(result);
            }
        }
        Err(invalid("invalid correction run"))
    }
}

pub fn decode_block(data: &[u8], header: &Header<'_>) -> io::Result<Vec<Vec<u16>>> {
    header.validate()?;
    let mut r = Reader { data, cursor: 0 };
    let count = usize::from(r.word()?);
    let n = header.cols * header.rows;
    if !(1..=120).contains(&count) || n * count * 2 > MAX_BLOCK {
        return Err(invalid("scene span exceeds decoding limits"));
    }
    let check = |code: u16| -> io::Result<u16> {
        if usize::from(code >> 7) < header.palette.len() {
            Ok(code)
        } else {
            Err(invalid("invalid cell palette index"))
        }
    };
    let reference = (0..n)
        .map(|_| check(r.word()?))
        .collect::<io::Result<Vec<_>>>()?;
    let tiles = header.cols.div_ceil(TILE) * header.rows.div_ceil(TILE);
    let mut tracks = Vec::with_capacity(tiles);
    for _ in 0..tiles {
        tracks.push(match r.byte()? {
            0 => Track::default(),
            1 => Track {
                coeff: [
                    r.word()? as i16,
                    r.word()? as i16,
                    r.word()? as i16,
                    r.word()? as i16,
                ],
            },
            _ => return Err(invalid("invalid motion track type")),
        });
    }
    let mut frames = Vec::with_capacity(count);
    frames.push(reference.clone());
    for f in 1..count {
        let mut frame = predict(&reference, &tracks, f, count, header.cols, header.rows);
        let mut cursor: usize = 0;
        while cursor < n {
            cursor = cursor
                .checked_add(r.varint()?)
                .filter(|p| *p <= n)
                .ok_or_else(|| invalid("correction skip outside frame"))?;
            if cursor == n {
                break;
            }
            let run = r.varint()?;
            let end = cursor
                .checked_add(run)
                .filter(|p| run > 0 && *p <= n)
                .ok_or_else(|| invalid("correction run outside frame"))?;
            for code in &mut frame[cursor..end] {
                *code = check(r.word()?)?;
            }
            cursor = end;
        }
        frames.push(frame);
    }
    if r.cursor != data.len() {
        return Err(invalid("trailing scene block data"));
    }
    Ok(frames)
}

pub fn inflate(packed: Vec<u8>) -> io::Result<Vec<u8>> {
    use std::process::{Command, Stdio};
    let mut child = Command::new("gzip")
        .arg("-dc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut input = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || input.write_all(&packed));
    let mut out = Vec::new();
    let read = child
        .stdout
        .take()
        .unwrap()
        .take(MAX_BLOCK as u64 + 1)
        .read_to_end(&mut out);
    if read.is_err() || out.len() > MAX_BLOCK {
        let _ = child.kill();
    }
    let status = child.wait();
    let write = writer.join().map_err(|_| invalid("gzip writer failed"))?;
    read?;
    if out.len() > MAX_BLOCK {
        return Err(invalid("inflated block exceeds memory limit"));
    }
    if !status?.success() {
        return Err(invalid("invalid gzip scene block"));
    }
    write?;
    Ok(out)
}

pub fn base64_decode(text: &str) -> io::Result<Vec<u8>> {
    let compact: Vec<_> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !compact.len().is_multiple_of(4) {
        return Err(invalid("truncated base64 data"));
    }
    let value = |b: u8| -> io::Result<u32> {
        Ok(match b {
            b'A'..=b'Z' => u32::from(b - b'A'),
            b'a'..=b'z' => u32::from(b - b'a') + 26,
            b'0'..=b'9' => u32::from(b - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(invalid("invalid base64 data")),
        })
    };
    let mut result = Vec::with_capacity(compact.len() / 4 * 3);
    for (i, q) in compact.as_chunks::<4>().0.iter().enumerate() {
        let padding = if q[2] == b'=' {
            2
        } else {
            usize::from(q[3] == b'=')
        };
        if padding > 0 && (i + 1 != compact.len() / 4 || q[3] != b'=') {
            return Err(invalid("invalid base64 padding"));
        }
        let v = (value(q[0])? << 18)
            | (value(q[1])? << 12)
            | (if padding < 2 { value(q[2])? } else { 0 } << 6)
            | if padding == 0 { value(q[3])? } else { 0 };
        result.push((v >> 16) as u8);
        if padding < 2 {
            result.push((v >> 8) as u8);
        }
        if padding == 0 {
            result.push(v as u8);
        }
    }
    Ok(result)
}

pub fn render(cells: &[u16], h: &Header<'_>) -> Vec<u8> {
    let (w, height) = (h.cols * h.cell_w, h.rows * h.cell_h);
    let mut rgb = vec![0; w * height * 3];
    for y in 0..height {
        for x in 0..w {
            let code = cells[y / h.cell_h * h.cols + x / h.cell_w];
            let glyph = h.font[usize::from(code & 127)];
            if !h.ascii
                || glyph[y % h.cell_h * 8 / h.cell_h] & (1 << (x % h.cell_w * 8 / h.cell_w)) != 0
            {
                rgb[(y * w + x) * 3..][..3].copy_from_slice(&h.palette[usize::from(code >> 7)]);
            }
        }
    }
    rgb
}

fn terminal_frame(
    cells: &[u16],
    h: &Header<'_>,
    columns: usize,
    rows: usize,
    blocks: bool,
    phase: usize,
    trail: &mut Vec<[u8; 3]>,
) -> Vec<u8> {
    let colors: Vec<_> = cells
        .iter()
        .map(|code| h.palette[usize::from(code >> 7)])
        .collect();
    crate::terminal::frame(&colors, h.cols, h.rows, columns, rows, blocks, phase, trail)
}

pub fn play(
    blocks: &[&str],
    h: &Header<'_>,
    expected_frames: usize,
    expected_hash: u64,
) -> io::Result<()> {
    use std::process::{Command, Stdio};
    h.validate()?;
    let mode = std::env::args().nth(1).unwrap_or_else(|| "--play".into());
    if mode == "--help" {
        println!(
            "--terminal: shaded ASCII; --terminal-blocks: detailed Unicode color blocks; --play (default): ffplay window; --verify: check exact cells; --raw: RGB24 stream"
        );
        return Ok(());
    }
    if ![
        "--play",
        "--terminal",
        "--terminal-blocks",
        "--verify",
        "--raw",
    ]
    .contains(&mode.as_str())
    {
        return Err(invalid(
            "use --terminal, --terminal-blocks, --play, --verify, --raw, or --help",
        ));
    }
    let block_mode = mode == "--terminal-blocks";
    if (mode == "--terminal" || block_mode) && !h.ascii {
        return Err(invalid("terminal playback requires an ASCII scene"));
    }
    let terminal = if mode == "--terminal" || block_mode {
        Some(crate::terminal::size(h.cols, h.rows, 1.0))
    } else {
        None
    };
    let mut child = if mode == "--play" {
        Some(
            Command::new("ffplay")
                .args([
                    "-v",
                    "error",
                    "-autoexit",
                    "-f",
                    "rawvideo",
                    "-pixel_format",
                    "rgb24",
                    "-video_size",
                ])
                .arg(format!("{}x{}", h.cols * h.cell_w, h.rows * h.cell_h))
                .arg("-framerate")
                .arg(h.fps.to_string())
                .args(["-i", "pipe:0"])
                .stdin(Stdio::piped())
                .spawn()?,
        )
    } else {
        None
    };
    let sink: Box<dyn Write> = if let Some(c) = &mut child {
        Box::new(c.stdin.take().unwrap())
    } else if mode == "--verify" {
        Box::new(io::sink())
    } else {
        Box::new(io::stdout())
    };
    let mut output = io::BufWriter::new(sink);
    let mut hash = HASH_START;
    let mut count = 0;
    let mut trail = Vec::new();
    let started = std::time::Instant::now();
    let result = (|| -> io::Result<()> {
        if terminal.is_some() {
            output.write_all(b"\x1b[2J")?;
        }
        for block in blocks {
            let packed = base64_decode(block)?;
            let raw = inflate(packed)?;
            for frame in decode_block(&raw, h)? {
                count += 1;
                if count > expected_frames {
                    return Err(invalid("too many scene frames"));
                }
                hash_cells(&mut hash, &frame);
                if let Some((columns, rows)) = terminal {
                    let deadline = started
                        + std::time::Duration::from_secs_f64((count - 1) as f64 / f64::from(h.fps));
                    std::thread::sleep(
                        deadline.saturating_duration_since(std::time::Instant::now()),
                    );
                    output.write_all(&terminal_frame(
                        &frame,
                        h,
                        columns,
                        rows,
                        block_mode,
                        (count - 1) * 4 / h.fps as usize,
                        &mut trail,
                    ))?;
                    output.flush()?;
                } else if mode != "--verify" {
                    output.write_all(&render(&frame, h))?;
                }
            }
        }
        if count != expected_frames || hash != expected_hash {
            return Err(invalid("scene reconstruction checksum mismatch"));
        }
        if terminal.is_some() {
            output.write_all(b"\n")?;
        }
        output.flush()
    })();
    drop(output);
    if let Some(mut c) = child {
        if result.is_err() {
            let _ = c.kill();
        }
        let status = c.wait()?;
        if result.is_ok() && !status.success() {
            return Err(invalid("ffplay failed"));
        }
    }
    result?;
    if mode == "--verify" {
        println!(
            "Verified {count} frames, {}x{} cells, hash {hash:016x}",
            h.cols, h.rows
        );
    }
    Ok(())
}
