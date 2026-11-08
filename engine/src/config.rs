use anyhow::{Result, bail, ensure};
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    Pixel,
    Ascii,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum PaletteKind {
    Pico8,
    Grayscale,
    Rgb332,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Mp4,
    Gif,
    Png,
    Scene,
    Equations,
}

#[derive(Debug, Parser)]
#[command(version, about = "Convert a video into pixel art or animated ASCII")]
pub struct Args {
    /// Video, animation, or image to read
    pub input: PathBuf,
    /// MP4/GIF file, Rust scene (.rs), or PNG directory with --format png
    pub output: PathBuf,
    #[arg(long, value_enum, default_value = "pixel")]
    pub mode: Mode,
    /// Number of art cells horizontally
    #[arg(long, default_value_t = 120)]
    pub columns: usize,
    /// Art rows; by default preserve display aspect ratio
    #[arg(long)]
    pub rows: Option<usize>,
    /// Integer enlargement (default: pixel 6, ASCII 1)
    #[arg(long)]
    pub scale: Option<usize>,
    #[arg(long, default_value_t = 24)]
    pub fps: u32,
    #[arg(long, value_enum, default_value = "pico8")]
    pub palette: PaletteKind,
    /// Screen-anchored Bayer dithering
    #[arg(long)]
    pub dither: bool,
    /// Hold cells until an RGB channel changes by this amount; 0 disables
    #[arg(long, default_value_t = 12)]
    pub stability: u8,
    /// ASCII gradient threshold; 255 disables directional edges
    #[arg(long, default_value_t = 40)]
    pub edge_threshold: u8,
    /// Tone curve exponent: below 1 lifts shadows; above 1 darkens
    #[arg(long, default_value_t = 1.0)]
    pub gamma: f64,
    /// Render ASCII in grayscale instead of palette colors
    #[arg(long)]
    pub monochrome: bool,
    /// Rust workers (default: available cores, capped at 8)
    #[arg(long)]
    pub threads: Option<usize>,
    /// Frames per batch (default: worker count, capped at 8)
    #[arg(long)]
    pub batch_size: Option<usize>,
    /// FFmpeg codec and filter workers per process
    #[arg(long, default_value_t = 1)]
    pub ffmpeg_threads: usize,
    /// Stop after this many output frames
    #[arg(long)]
    pub frames: Option<usize>,
    /// Infer mp4/gif/scene from extension; PNG requires an output directory
    #[arg(long, value_enum)]
    pub format: Option<Format>,
    #[arg(long)]
    pub overwrite: bool,
    /// Equation-only MLX fitting iterations; higher values take longer
    #[arg(long, default_value_t = 45000)]
    pub fit_steps: usize,
    /// Re-export existing numeric equation coefficients instead of fitting again
    #[arg(long)]
    pub equation_model: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub cols: usize,
    pub rows: usize,
    pub cell_w: usize,
    pub cell_h: usize,
    pub width: usize,
    pub height: usize,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub threads: usize,
    pub batch: usize,
}

impl Args {
    pub fn output_format(&self) -> Result<Format> {
        if let Some(format) = self.format {
            return Ok(format);
        }
        match self.output.extension().and_then(|s| s.to_str()) {
            Some(s) if s.eq_ignore_ascii_case("mp4") => Ok(Format::Mp4),
            Some(s) if s.eq_ignore_ascii_case("gif") => Ok(Format::Gif),
            Some(s) if s.eq_ignore_ascii_case("rs") => Ok(Format::Scene),
            _ => bail!("use an .mp4/.gif/.rs output or --format png for a frame directory"),
        }
    }

    pub fn layout(&self, aspect: f64) -> Result<Layout> {
        ensure!(
            aspect.is_finite() && aspect > 0.0,
            "invalid video aspect ratio"
        );
        ensure!(
            (1..=2048).contains(&self.columns),
            "columns must be 1..2048"
        );
        ensure!((1..=120).contains(&self.fps), "fps must be 1..120");
        ensure!(
            (1..=200000).contains(&self.fit_steps),
            "fit-steps must be 1..200000"
        );
        ensure!(self.frames != Some(0), "frames must be positive");
        ensure!(self.stability <= 64, "stability must be 0..64");
        ensure!(
            self.gamma.is_finite() && (0.2..=3.0).contains(&self.gamma),
            "gamma must be 0.2..3.0"
        );
        ensure!(
            (1..=64).contains(&self.ffmpeg_threads),
            "ffmpeg-threads must be 1..64"
        );
        let threads = self
            .threads
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get().min(8)));
        ensure!((1..=64).contains(&threads), "threads must be 1..64");
        let batch = self.batch_size.unwrap_or(threads.min(8));
        ensure!((1..=64).contains(&batch), "batch-size must be 1..64");
        let scale = self
            .scale
            .unwrap_or(if self.mode == Mode::Pixel { 6 } else { 1 });
        ensure!((1..=16).contains(&scale), "scale must be 1..16");
        let (cw, ch) = if self.mode == Mode::Pixel {
            (1, 1)
        } else {
            (8, 16)
        };
        let rows = self.rows.unwrap_or_else(|| {
            (self.columns as f64 * cw as f64 / ch as f64 / aspect)
                .round()
                .max(1.0) as usize
        });
        ensure!((1..=2048).contains(&rows), "rows must be 1..2048");
        let width = self.columns * cw * scale;
        let height = rows * ch * scale;
        ensure!(
            width <= 8192 && height <= 8192,
            "rendered dimensions must be at most 8192x8192"
        );
        let input_bytes = self.columns * rows * 3;
        let output_bytes = width * height * 3;
        // Includes input, accepted history, candidates, and rendered batch buffers.
        let memory = (output_bytes + self.columns * rows * 24) * batch;
        ensure!(
            memory <= 512 * 1024 * 1024,
            "batch working buffers exceed 512 MiB; lower scale, rows, columns or batch-size"
        );
        Ok(Layout {
            cols: self.columns,
            rows,
            cell_w: cw * scale,
            cell_h: ch * scale,
            width,
            height,
            input_bytes,
            output_bytes,
            threads,
            batch,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(extra: &[&str]) -> Args {
        Args::parse_from(
            ["art", "in.mp4", "out.mp4"]
                .into_iter()
                .chain(extra.iter().copied()),
        )
    }
    #[test]
    fn geometry_accounts_for_tall_ascii_cells() {
        let pixel = args(&[]).layout(16.0 / 9.0).unwrap();
        let ascii = args(&["--mode", "ascii"]).layout(16.0 / 9.0).unwrap();
        assert_eq!((pixel.cols, pixel.rows, pixel.width), (120, 68, 720));
        assert_eq!((ascii.cols, ascii.rows, ascii.width), (120, 34, 960));
    }
    #[test]
    fn invalid_and_excessive_allocations_are_rejected() {
        for extra in [
            vec!["--columns", "0"],
            vec!["--fps", "0"],
            vec!["--threads", "0"],
            vec!["--rows", "0"],
            vec!["--frames", "0"],
            vec!["--scale", "0"],
            vec!["--gamma", "NaN"],
            vec!["--gamma", "0"],
            vec![
                "--columns",
                "2048",
                "--rows",
                "2048",
                "--scale",
                "4",
                "--batch-size",
                "64",
            ],
        ] {
            assert!(args(&extra).layout(1.0).is_err(), "{extra:?}");
        }
        assert!(args(&[]).layout(f64::NAN).is_err());
    }
    #[test]
    fn explicit_png_format_supports_directory_names() {
        let a = Args::parse_from(["art", "in.mp4", "frames", "--format", "png"]);
        assert_eq!(a.output_format().unwrap(), Format::Png);
        assert!(
            Args::parse_from(["art", "in", "out.xyz"])
                .output_format()
                .is_err()
        );
    }
}
