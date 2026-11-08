// Shared with standalone exports. The only scene data are affine/sine coefficients.
use std::io::{self, Write};

pub struct LayerQ<'a> {
    pub inputs: usize,
    pub outputs: usize,
    pub scale: f32,
    pub weights: &'a [i16],
    pub bias: &'a [i16],
}

pub struct Description<'a> {
    pub width: usize,
    pub height: usize,
    pub frames: usize,
    pub fps: u32,
    pub layers: &'a [LayerQ<'a>],
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

struct Layer {
    inputs: usize,
    outputs: usize,
    weights: Vec<f32>,
    bias: Vec<f32>,
}
pub struct Model {
    width: usize,
    height: usize,
    frames: usize,
    fps: u32,
    layers: Vec<Layer>,
    scratch: usize,
}

impl Model {
    pub fn new(d: &Description<'_>) -> io::Result<Self> {
        if !(16..=256).contains(&d.width)
            || !(16..=512).contains(&d.height)
            || !d.width.is_multiple_of(4)
            || !d.height.is_multiple_of(4)
            || !(1..=120).contains(&d.frames)
            || !(1..=120).contains(&d.fps)
            || !(2..=8).contains(&d.layers.len())
        {
            return Err(invalid("invalid coordinate model dimensions or layers"));
        }
        let mut inputs = 3;
        let mut parameters = 0;
        let mut scratch = 3;
        let mut layers = Vec::new();
        for l in d.layers {
            if l.inputs != inputs
                || !(1..=256).contains(&l.outputs)
                || !l.scale.is_finite()
                || l.scale <= 0.0
                || l.scale > 1.0
                || l.weights.len() != 16 * l.inputs * l.outputs
                || l.bias.len() != 16 * l.outputs
            {
                return Err(invalid("invalid coordinate model coefficients"));
            }
            parameters += l.weights.len() + l.bias.len();
            if parameters > 1_000_000 {
                return Err(invalid("coordinate model exceeds parameter limit"));
            }
            scratch = scratch.max(l.outputs);
            layers.push(Layer {
                inputs: l.inputs,
                outputs: l.outputs,
                weights: l.weights.iter().map(|q| f32::from(*q) * l.scale).collect(),
                bias: l.bias.iter().map(|q| f32::from(*q) * l.scale).collect(),
            });
            inputs = l.outputs;
        }
        if inputs != 3 {
            return Err(invalid("coordinate model must output RGB"));
        }
        Ok(Self {
            width: d.width,
            height: d.height,
            frames: d.frames,
            fps: d.fps,
            layers,
            scratch,
        })
    }

    fn color(
        &self,
        tile: usize,
        coordinates: [f32; 3],
        a: &mut Vec<f32>,
        b: &mut Vec<f32>,
    ) -> [u8; 3] {
        a[..3].copy_from_slice(&coordinates);
        for (index, layer) in self.layers.iter().enumerate() {
            for (neuron, output) in b[..layer.outputs].iter_mut().enumerate() {
                let offset = (tile * layer.outputs + neuron) * layer.inputs;
                let weights = &layer.weights[offset..offset + layer.inputs];
                // Independent lanes let the compiler vectorize the dot product.
                let mut sums = [0.0; 4];
                let chunks = layer.inputs / 4 * 4;
                for j in (0..chunks).step_by(4) {
                    for k in 0..4 {
                        sums[k] += weights[j + k] * a[j + k];
                    }
                }
                let mut sum =
                    sums.into_iter().sum::<f32>() + layer.bias[tile * layer.outputs + neuron];
                for j in chunks..layer.inputs {
                    sum += weights[j] * a[j];
                }
                *output = if index + 1 < self.layers.len() {
                    (sum * 30.0).sin()
                } else {
                    sum
                };
            }
            std::mem::swap(a, b);
        }
        std::array::from_fn(|i| ((a[i] + 1.0) * 127.5).clamp(0.0, 255.0).round() as u8)
    }

    pub fn frame(&self, frame: usize, width: usize, height: usize, workers: usize) -> Vec<[u8; 3]> {
        let mut result = vec![[0; 3]; width * height];
        let per_worker = height.div_ceil(workers.clamp(1, height));
        let pw = self.width / 4;
        let ph = self.height / 4;
        let t = frame as f32 * 2.0 / self.frames.saturating_sub(1).max(1) as f32 - 1.0;
        std::thread::scope(|scope| {
            for (part, pixels) in result.chunks_mut(per_worker * width).enumerate() {
                scope.spawn(move || {
                    let (mut a, mut b) = (vec![0.0; self.scratch], vec![0.0; self.scratch]);
                    for (i, pixel) in pixels.iter_mut().enumerate() {
                        let gx = (i % width) as f32 * (self.width - 1) as f32
                            / width.saturating_sub(1).max(1) as f32;
                        let gy = (part * per_worker + i / width) as f32 * (self.height - 1) as f32
                            / height.saturating_sub(1).max(1) as f32;
                        let tx = (gx as usize / pw).min(3);
                        let ty = (gy as usize / ph).min(3);
                        let coordinates = [
                            (gx - (tx * pw) as f32) * 2.0 / (pw - 1) as f32 - 1.0,
                            (gy - (ty * ph) as f32) * 2.0 / (ph - 1) as f32 - 1.0,
                            t,
                        ];
                        *pixel = self.color(ty * 4 + tx, coordinates, &mut a, &mut b);
                    }
                });
            }
        });
        result
    }
}

pub fn play(d: &Description<'_>) -> io::Result<()> {
    use std::process::{Command, Stdio};
    let model = Model::new(d)?;
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "--terminal".into());
    if mode == "--help" {
        println!(
            "--terminal (default), --terminal-blocks, --play (ffplay), --raw (RGB24), --verify"
        );
        return Ok(());
    }
    if ![
        "--terminal",
        "--terminal-blocks",
        "--play",
        "--raw",
        "--verify",
    ]
    .contains(&mode.as_str())
    {
        return Err(invalid("unknown playback mode"));
    }
    let terminal = mode.starts_with("--terminal");
    let blocks = mode == "--terminal-blocks";
    let (columns, rows) = // Continuous equations: zooming the terminal out gives more, smaller characters.
    crate::terminal::size(d.width, d.height.div_ceil(2), 8.0);
    let (width, height) = if terminal {
        (columns, if blocks { rows * 2 } else { rows })
    } else {
        (d.width, d.height)
    };
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get().min(8));
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
                .arg(format!("{}x{}", d.width, d.height))
                .arg("-framerate")
                .arg(d.fps.to_string())
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
    let started = std::time::Instant::now();
    let mut hash = 0xcbf29ce484222325_u64;
    let result = (|| -> io::Result<()> {
        if terminal {
            use std::io::IsTerminal;
            output.write_all(b"\x1b[2J")?;
            // Evaluate each frame once at terminal resolution, then loop the cache in a
            // real terminal (a single image keeps shimmering); pipes get one pass.
            let ticks = if io::stdout().is_terminal() {
                usize::MAX
            } else {
                d.frames
            };
            let mut cache = Vec::with_capacity(d.frames);
            let mut trail = Vec::new();
            for tick in 0..ticks {
                let frame = tick % d.frames;
                if cache.len() == frame {
                    cache.push(model.frame(frame, width, height, workers));
                }
                let deadline = started
                    + std::time::Duration::from_secs_f64(tick as f64 / f64::from(model.fps));
                std::thread::sleep(deadline.saturating_duration_since(std::time::Instant::now()));
                output.write_all(&crate::terminal::frame(
                    &cache[frame],
                    width,
                    height,
                    columns,
                    rows,
                    blocks,
                    tick * 4 / model.fps as usize,
                    &mut trail,
                ))?;
                output.flush()?;
            }
        } else {
            for frame in 0..d.frames {
                for rgb in model.frame(frame, width, height, workers) {
                    for byte in rgb {
                        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
                        if mode != "--verify" {
                            output.write_all(&[byte])?;
                        }
                    }
                }
            }
        }
        if terminal {
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
            "Evaluated {} equation-only frames, hash {hash:016x}, {:.1} FPS",
            d.frames,
            d.frames as f64 / started.elapsed().as_secs_f64()
        );
    }
    Ok(())
}
