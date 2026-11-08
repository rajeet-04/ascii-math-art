mod config;
#[allow(dead_code)]
mod equation_runtime;
mod equations;
mod palette;
mod render;
mod scene;
#[allow(dead_code)] // The same module is also compiled into exported players.
mod scene_runtime;
mod terminal;
mod video;

use anyhow::{Context, Result, ensure};
use clap::Parser;
use config::Args;
use palette::Palette;
use rayon::prelude::*;
use render::{Glyphs, Stabilizer, Tone};
use std::{
    io::{BufReader, BufWriter, Write},
    time::Instant,
};

fn run(args: Args) -> Result<()> {
    let started = Instant::now();
    let format = args.output_format()?;
    let layout = args.layout(video::probe_aspect(&args.input)?)?;
    let stage = video::Stage::new(&args, format)?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(layout.threads)
        .build()?;
    if format == config::Format::Equations {
        return equations::export(&args, &layout, &stage, started);
    }
    ensure!(
        args.equation_model.is_none(),
        "--equation-model requires --format equations"
    );
    let palette = Palette::new(args.palette);
    let tone = Tone::new(args.gamma);
    if format == config::Format::Scene {
        return scene::export(&args, &layout, &stage, &pool, &palette, &tone, started);
    }
    let glyphs = Glyphs::new();
    let mut history = Stabilizer::default();
    eprintln!(
        "{:?}: {}x{} cells → {}x{} RGB, {} FPS, {} workers, batch {}",
        args.mode,
        layout.cols,
        layout.rows,
        layout.width,
        layout.height,
        args.fps,
        layout.threads,
        layout.batch
    );

    let mut decoder = video::decoder(&args, &layout, &stage)?;
    let mut input = BufReader::new(
        decoder
            .child()
            .stdout
            .take()
            .context("decoder stdout missing")?,
    );
    let mut encoder = video::encoder(&args, &layout, format, &stage)?;
    let mut output = BufWriter::new(
        encoder
            .child()
            .stdin
            .take()
            .context("encoder stdin missing")?,
    );
    let mut frames = 0_u64;
    let mut conversion_seconds = 0.0;
    loop {
        let mut raw = Vec::with_capacity(layout.batch);
        for _ in 0..layout.batch {
            match video::read_frame(&mut input, layout.input_bytes)? {
                Some(frame) => raw.push(frame),
                None => break,
            }
        }
        if raw.is_empty() {
            break;
        }
        let conversion_start = Instant::now();
        // Indexed parallel iterators preserve frame ordering on collect.
        let mut cells: Vec<_> = pool.install(|| {
            raw.par_iter()
                .map(|rgb| render::analyze(rgb, &layout, &args, &palette, &tone))
                .collect()
        });
        drop(raw);
        for frame in &mut cells {
            history.apply(frame, args.stability);
        }
        let rendered: Vec<_> = pool.install(|| {
            cells
                .into_par_iter()
                .map(|frame| glyphs.render(&frame, &layout, args.mode))
                .collect()
        });
        conversion_seconds += conversion_start.elapsed().as_secs_f64();
        for frame in rendered {
            output
                .write_all(&frame)
                .with_context(|| format!("write encoded frame: {}", encoder.diagnostic()))?;
            frames += 1;
        }
    }
    drop(input);
    decoder.finish()?;
    output
        .flush()
        .with_context(|| format!("flush encoder: {}", encoder.diagnostic()))?;
    drop(output); // EOF lets FFmpeg finalize its container.
    encoder.finish()?;
    ensure!(frames > 0, "input produced no frames at the selected FPS");
    let bytes = stage.bytes()?;
    stage.publish()?;
    let elapsed = started.elapsed().as_secs_f64();
    eprintln!(
        "Done: {frames} frames, {elapsed:.3}s, {:.1} end-to-end FPS, {:.1} conversion FPS, {bytes} output bytes → {}",
        frames as f64 / elapsed,
        frames as f64 / conversion_seconds.max(1e-9),
        args.output.display()
    );
    Ok(())
}

fn main() {
    if let Err(error) = run(Args::parse()) {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
