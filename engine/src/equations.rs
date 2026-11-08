use crate::{
    config::{Args, Layout, Mode},
    equation_runtime::{Description, LayerQ, Model},
    video,
};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
    process::Command,
    time::Instant,
};

struct Layer {
    inputs: usize,
    outputs: usize,
    scale: f32,
    weights: Vec<i16>,
    bias: Vec<i16>,
}
struct Fitted {
    width: usize,
    height: usize,
    frames: usize,
    fps: u32,
    layers: Vec<Layer>,
}

fn integer(v: &Value, key: &str) -> Result<usize> {
    usize::try_from(
        v[key]
            .as_u64()
            .with_context(|| format!("missing integer {key}"))?,
    )
    .context("integer out of range")
}
fn coefficients(v: &Value) -> Result<Vec<i16>> {
    let a = v.as_array().context("coefficient array required")?;
    ensure!(a.len() <= 1_000_000, "too many coefficients");
    a.iter()
        .map(|n| {
            i16::try_from(n.as_i64().context("coefficient must be an integer")?)
                .context("coefficient exceeds i16")
        })
        .collect()
}
fn load(path: &Path) -> Result<Fitted> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 16 * 1024 * 1024, "model JSON exceeds 16 MiB");
    let v: Value = serde_json::from_slice(&bytes).context("parse numeric equation model")?;
    ensure!(
        integer(&v, "tiles")? == 4 && v["omega"].as_f64() == Some(30.0),
        "unsupported coordinate architecture"
    );
    let layers = v["layers"].as_array().context("model layers required")?;
    ensure!(
        (2..=8).contains(&layers.len()),
        "model requires 2..8 layers"
    );
    let layers = layers
        .iter()
        .map(|l| {
            Ok(Layer {
                inputs: integer(l, "inputs")?,
                outputs: integer(l, "outputs")?,
                scale: l["scale"].as_f64().context("layer scale required")? as f32,
                weights: coefficients(&l["weights"])?,
                bias: coefficients(&l["bias"])?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Fitted {
        width: integer(&v, "width")?,
        height: integer(&v, "height")?,
        frames: integer(&v, "frames")?,
        fps: u32::try_from(integer(&v, "fps")?)?,
        layers,
    })
}

fn array(output: &mut impl Write, values: &[i16]) -> Result<()> {
    write!(output, "&[")?;
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            write!(output, ",")?;
        }
        if i % 128 == 0 {
            writeln!(output)?;
        }
        write!(output, "{v}")?;
    }
    writeln!(output, "]")?;
    Ok(())
}

fn write_source(path: &Path, f: &Fitted, psnr: f64, minimum: f64) -> Result<usize> {
    let parameters = f
        .layers
        .iter()
        .map(|l| l.weights.len() + l.bias.len())
        .sum();
    let mut out = BufWriter::new(File::create(path)?);
    writeln!(
        out,
        "// Equation-only full-scene animation: F(x,y,t) -> RGB."
    )?;
    writeln!(
        out,
        "// {parameters} fitted coefficients; no stored frames, references, or residuals."
    )?;
    writeln!(
        out,
        "// Measured Rust reconstruction: {psnr:.3} dB; worst frame {minimum:.3} dB."
    )?;
    writeln!(out, "mod terminal {{\n{}\n}}", include_str!("terminal.rs"))?;
    writeln!(
        out,
        "mod equation_runtime {{\n{}\n}}",
        include_str!("equation_runtime.rs")
    )?;
    writeln!(
        out,
        "static LAYERS: &[equation_runtime::LayerQ<'static>] = &["
    )?;
    for l in &f.layers {
        writeln!(
            out,
            "equation_runtime::LayerQ {{ inputs:{}, outputs:{}, scale:{:.9e}f32, weights:",
            l.inputs, l.outputs, l.scale
        )?;
        array(&mut out, &l.weights)?;
        writeln!(out, ",bias:")?;
        array(&mut out, &l.bias)?;
        writeln!(out, "}},")?;
    }
    writeln!(
        out,
        "];\nfn main() {{ let d=equation_runtime::Description {{width:{},height:{},frames:{},fps:{},layers:LAYERS}}; if let Err(e)=equation_runtime::play(&d) {{eprintln!(\"error: {{e}}\");std::process::exit(1);}} }}",
        f.width, f.height, f.frames, f.fps
    )?;
    out.flush()?;
    Ok(parameters)
}

pub fn export(args: &Args, layout: &Layout, stage: &video::Stage, started: Instant) -> Result<()> {
    let mut grid = layout.clone();
    grid.cols = grid.cols / 4 * 4;
    grid.rows = (grid.rows * if args.mode == Mode::Ascii { 2 } else { 1 }) / 4 * 4;
    ensure!(
        (16..=256).contains(&grid.cols) && (16..=512).contains(&grid.rows),
        "equation grid must fit within 256x512, at least 16x16"
    );
    grid.input_bytes = grid.cols * grid.rows * 3;
    if let Some(model) = &args.equation_model
        && let (Ok(a), Ok(b)) = (model.canonicalize(), args.output.canonicalize())
    {
        ensure!(a != b, "output must not replace input model");
    }
    let raw_path = stage.root.join("training.rgb");
    let mut raw = BufWriter::new(File::create(&raw_path)?);
    let mut decoder = video::decoder(args, &grid, stage)?;
    let mut input = BufReader::new(
        decoder
            .child()
            .stdout
            .take()
            .context("decoder stdout missing")?,
    );
    let mut frames = 0;
    while let Some(frame) = video::read_frame(&mut input, grid.input_bytes)? {
        frames += 1;
        ensure!(
            frames <= 120 && frames * grid.input_bytes <= 16 * 1024 * 1024,
            "equation prototype supports at most 120 frames / 16 MiB RGB; use --frames or fewer columns"
        );
        raw.write_all(&frame)?;
    }
    drop(input);
    decoder.finish()?;
    raw.flush()?;
    drop(raw);
    ensure!(frames > 0, "input produced no frames");
    let model_path = if let Some(path) = &args.equation_model {
        path.clone()
    } else {
        let path = stage.root.join("coefficients.json");
        let trainer = stage.root.join("fit.py");
        std::fs::write(&trainer, include_str!("../fit_equations.py"))?;
        let python = std::env::var_os("ASCII_ART_PYTHON").unwrap_or_else(|| "python3".into());
        let status = Command::new(python)
            .arg(&trainer)
            .arg(&raw_path)
            .arg(&path)
            .arg("--width")
            .arg(grid.cols.to_string())
            .arg("--height")
            .arg(grid.rows.to_string())
            .arg("--frames")
            .arg(frames.to_string())
            .arg("--fps")
            .arg(args.fps.to_string())
            .arg("--steps")
            .arg(args.fit_steps.to_string())
            .status()
            .context(
                "start local MLX fitter; set ASCII_ART_PYTHON to a Python with mlx/numpy installed",
            )?;
        ensure!(
            status.success(),
            "MLX fitting failed; existing output was preserved"
        );
        path
    };
    let f = load(&model_path)?;
    ensure!(
        f.width == grid.cols && f.height == grid.rows && f.frames == frames && f.fps == args.fps,
        "model metadata does not match decoded input"
    );
    let quant: Vec<_> = f
        .layers
        .iter()
        .map(|l| LayerQ {
            inputs: l.inputs,
            outputs: l.outputs,
            scale: l.scale,
            weights: &l.weights,
            bias: &l.bias,
        })
        .collect();
    let d = Description {
        width: f.width,
        height: f.height,
        frames: f.frames,
        fps: f.fps,
        layers: &quant,
    };
    let model = Model::new(&d)?;
    let workers = layout.threads;
    let mut reference = BufReader::new(File::open(raw_path)?);
    let mut target = vec![0; grid.input_bytes];
    let (mut error, mut worst) = (0.0_f64, 0.0_f64);
    for frame in 0..frames {
        reference.read_exact(&mut target)?;
        let pixels = model.frame(frame, grid.cols, grid.rows, workers);
        let sse = pixels
            .iter()
            .flatten()
            .zip(&target)
            .map(|(a, b)| {
                let delta = f64::from(*a) - f64::from(*b);
                delta * delta
            })
            .sum::<f64>();
        error += sse;
        worst = worst.max(sse / grid.input_bytes as f64);
    }
    let psnr = 10.0
        * (255.0_f64.powi(2) / (error / (frames * grid.input_bytes) as f64).max(1e-12)).log10();
    let minimum = 10.0 * (255.0_f64.powi(2) / worst.max(1e-12)).log10();
    let parameters = write_source(&stage.artifact, &f, psnr, minimum)?;
    let bytes = stage.bytes()?;
    stage.publish()?;
    if args.equation_model.is_none() {
        // Keep fitted coefficients so re-exports never need another GPU fit.
        let mut name = args.output.file_stem().unwrap_or_default().to_owned();
        name.push("-model.json");
        let kept = args.output.with_file_name(name);
        std::fs::copy(&model_path, &kept)?;
        eprintln!("Fitted coefficients kept at {}", kept.display());
    }
    eprintln!(
        "Equations: {frames} frames, {}x{}, {parameters} coefficients, {bytes} complete source bytes",
        grid.cols, grid.rows
    );
    eprintln!(
        "Measured Rust RGB PSNR: {psnr:.2} dB; worst frame {minimum:.2} dB; {:.2}s total",
        started.elapsed().as_secs_f64()
    );
    if psnr < 28.0 {
        eprintln!("Experimental: below the 28 dB quality target; detail will look soft.");
    }
    if bytes > 512 * 1024 {
        eprintln!("Note: source exceeds the 512 KiB size target (quality is prioritised).");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coefficient_parser_rejects_nonintegers_and_overflow() {
        assert!(coefficients(&serde_json::json!([1.5])).is_err());
        assert!(coefficients(&serde_json::json!([40000])).is_err());
        assert_eq!(
            coefficients(&serde_json::json!([-2, 0, 3])).unwrap(),
            [-2, 0, 3]
        );
    }
    #[test]
    fn coordinate_equations_and_parallel_evaluation_match_known_function() {
        let first = LayerQ {
            inputs: 3,
            outputs: 1,
            scale: 0.01,
            weights: &[100, 0, 0].repeat(16),
            bias: &[0; 16],
        };
        let last = LayerQ {
            inputs: 1,
            outputs: 3,
            scale: 0.01,
            weights: &[100, 100, 100].repeat(16),
            bias: &[0; 48],
        };
        let d = Description {
            width: 16,
            height: 16,
            frames: 2,
            fps: 20,
            layers: &[first, last],
        };
        let m = Model::new(&d).unwrap();
        let a = m.frame(0, 16, 16, 1);
        let b = m.frame(0, 16, 16, 4);
        assert_eq!(a, b);
        let expected = ((-30.0_f32).sin().mul_add(127.5, 127.5)).round() as u8;
        assert_eq!(a[0], [expected; 3]);
        let bad = Description { width: 15, ..d };
        assert!(Model::new(&bad).is_err());
    }
}
