use crate::config::{Args, Format, Layout};
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use std::{
    fs::{self, File},
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn probe_aspect(input: &Path) -> Result<f64> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,sample_aspect_ratio:stream_side_data=rotation",
            "-of",
            "json",
        ])
        .arg(input)
        .output()
        .context("run ffprobe (install FFmpeg and put it on PATH)")?;
    ensure!(
        output.status.success(),
        "ffprobe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).context("parse ffprobe metadata")?;
    let stream = data["streams"]
        .as_array()
        .and_then(|s| s.first())
        .context("input has no video stream")?;
    let w = stream["width"].as_u64().context("missing video width")?;
    let h = stream["height"].as_u64().context("missing video height")?;
    ensure!(w > 0 && h > 0, "invalid video dimensions");
    let sar = stream["sample_aspect_ratio"]
        .as_str()
        .and_then(|s| s.split_once(':'))
        .and_then(|(a, b)| Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?)))
        .filter(|(a, b)| *a > 0.0 && *b > 0.0)
        .map_or(1.0, |(a, b)| a / b);
    let rotation = stream["side_data_list"]
        .as_array()
        .and_then(|a| a.iter().find_map(|s| s["rotation"].as_i64()))
        .unwrap_or(0)
        .rem_euclid(180);
    let aspect = w as f64 * sar / h as f64;
    Ok(if rotation == 90 { 1.0 / aspect } else { aspect })
}

/// Dropping a failed pipeline always reaps its children (including broken pipes).
pub struct Process {
    child: Option<Child>,
    log: PathBuf,
    label: &'static str,
}

impl Process {
    fn spawn(mut command: Command, log: PathBuf, label: &'static str) -> Result<Self> {
        command.stderr(Stdio::from(File::create(&log)?));
        let child = command
            .spawn()
            .with_context(|| format!("start {label}; ensure FFmpeg is installed"))?;
        Ok(Self {
            child: Some(child),
            log,
            label,
        })
    }

    pub fn child(&mut self) -> &mut Child {
        self.child.as_mut().unwrap()
    }

    pub fn finish(&mut self) -> Result<()> {
        let status = self.child.as_mut().unwrap().wait()?;
        self.child.take();
        ensure!(
            status.success(),
            "{} failed ({status}): {}",
            self.label,
            self.diagnostic()
        );
        Ok(())
    }

    pub fn diagnostic(&self) -> String {
        File::open(&self.log)
            .map(diagnostic_prefix)
            .unwrap_or_default()
    }
}

fn diagnostic_prefix(reader: impl Read) -> String {
    let mut bytes = Vec::new();
    let _ = reader.take(4096).read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(unix)]
fn rename_noreplace(from: &Path, to: &Path) -> Result<()> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};
    renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE)?;
    Ok(())
}

#[cfg(not(unix))]
fn rename_noreplace(from: &Path, to: &Path) -> Result<()> {
    // Windows rename fails when the destination directory already exists.
    fs::rename(from, to)?;
    Ok(())
}

impl Drop for Process {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub struct Stage {
    pub root: PathBuf,
    pub artifact: PathBuf,
    destination: PathBuf,
    format: Format,
    overwrite: bool,
}

impl Stage {
    pub fn new(args: &Args, format: Format) -> Result<Self> {
        ensure!(
            args.input.is_file(),
            "input is not a file: {}",
            args.input.display()
        );
        if let Ok(meta) = fs::symlink_metadata(&args.output) {
            ensure!(
                !meta.file_type().is_symlink(),
                "refusing to replace an output symlink"
            );
            if let (Ok(input), Ok(output)) = (args.input.canonicalize(), args.output.canonicalize())
            {
                ensure!(input != output, "input and output must be different paths");
            }
            ensure!(
                format != Format::Png,
                "PNG directory already exists; choose a new output directory"
            );
            ensure!(meta.is_file(), "output is not a regular file");
            ensure!(
                args.overwrite,
                "output exists; use --overwrite to replace it"
            );
        }
        let parent = args
            .output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).context("create output parent directory")?;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = parent.join(format!(".ascii-art-{}-{stamp}", std::process::id()));
        fs::create_dir(&root).context("create staging directory")?;
        let artifact = root.join(match format {
            Format::Mp4 => "output.mp4",
            Format::Gif => "output.gif",
            Format::Png => "frames",
            Format::Scene => "scene.rs",
            Format::Equations => "equations.rs",
        });
        let stage = Self {
            root,
            artifact,
            destination: args.output.clone(),
            format,
            overwrite: args.overwrite,
        };
        if format == Format::Png {
            fs::create_dir(&stage.artifact)?;
        }
        Ok(stage)
    }

    pub fn bytes(&self) -> Result<u64> {
        if self.format == Format::Png {
            fs::read_dir(&self.artifact)?
                .map(|e| Ok(e?.metadata()?.len()))
                .sum()
        } else {
            Ok(fs::metadata(&self.artifact)?.len())
        }
    }

    pub fn publish(&self) -> Result<()> {
        if self.format == Format::Png {
            rename_noreplace(&self.artifact, &self.destination)
                .context("publish PNG directory without replacing an existing destination")?;
        } else if self.overwrite {
            // Atomic replacement on Unix. On platforms without replacement rename,
            // this returns an error and preserves the previous destination.
            fs::rename(&self.artifact, &self.destination)?;
        } else {
            // An atomic no-clobber publish, even if another job created the target.
            fs::hard_link(&self.artifact, &self.destination)
                .context("publish output without replacing an existing file")?;
        }
        Ok(())
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub fn decoder(args: &Args, layout: &Layout, stage: &Stage) -> Result<Process> {
    let mut c = Command::new("ffmpeg");
    c.args(["-v", "error", "-nostdin", "-threads"])
        .arg(args.ffmpeg_threads.to_string())
        .arg("-filter_threads")
        .arg(args.ffmpeg_threads.to_string())
        .arg("-i")
        .arg(&args.input)
        .args(["-map", "0:v:0", "-an", "-sn", "-dn", "-vf"])
        .arg(format!(
            "fps={},scale={}:{}:flags=area,setsar=1",
            args.fps, layout.cols, layout.rows
        ));
    if let Some(frames) = args.frames {
        c.arg("-frames:v").arg(frames.to_string());
    }
    c.args([
        "-threads", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1",
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped());
    Process::spawn(c, stage.root.join("decoder.log"), "FFmpeg decoder")
}

pub fn encoder(args: &Args, layout: &Layout, format: Format, stage: &Stage) -> Result<Process> {
    let mut c = Command::new("ffmpeg");
    c.args(["-v", "error", "-nostdin", "-y", "-filter_threads"])
        .arg(args.ffmpeg_threads.to_string())
        .arg("-filter_complex_threads")
        .arg(args.ffmpeg_threads.to_string())
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-video_size"])
        .arg(format!("{}x{}", layout.width, layout.height))
        .arg("-framerate")
        .arg(args.fps.to_string())
        .args(["-i", "pipe:0", "-an", "-threads"])
        .arg(args.ffmpeg_threads.to_string());
    match format {
        Format::Scene | Format::Equations => {
            bail!("Rust source output uses its exporter, not FFmpeg encoding")
        }
        Format::Mp4 => {
            c.args([
                "-vf",
                "pad=ceil(iw/2)*2:ceil(ih/2)*2",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "18",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
            ])
            .arg(&stage.artifact);
        }
        Format::Gif => {
            // Single-frame palettes keep the filtergraph streaming. Full-clip
            // palettegen behind split would buffer the entire second branch.
            c.args(["-filter_complex", "[0:v]split[a][b];[a]palettegen=stats_mode=single[p];[b][p]paletteuse=new=1:dither=none",
                "-loop", "0"]).arg(&stage.artifact);
        }
        Format::Png => {
            c.args(["-c:v", "png", "-start_number", "0"])
                .arg(stage.artifact.join("%06d.png"));
        }
    }
    c.stdin(Stdio::piped()).stdout(Stdio::null());
    Process::spawn(c, stage.root.join("encoder.log"), "FFmpeg encoder")
}

pub fn read_frame(reader: &mut impl Read, bytes: usize) -> Result<Option<Vec<u8>>> {
    let mut frame = vec![0; bytes];
    let mut filled = 0;
    while filled < bytes {
        match reader.read(&mut frame[filled..]) {
            Ok(0) if filled == 0 => return Ok(None),
            Ok(0) => bail!("truncated RGB frame: got {filled} of {bytes} bytes"),
            Ok(n) => filled += n,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e).context("read decoded RGB frame"),
        }
    }
    Ok(Some(frame))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn diagnostic_reader_consumes_only_a_bounded_prefix() {
        let mut input = Cursor::new(vec![b'x'; 1024 * 1024]);
        assert_eq!(diagnostic_prefix(&mut input).len(), 4096);
        assert_eq!(input.position(), 4096);
    }
    #[test]
    fn atomic_directory_publish_refuses_an_existing_empty_directory() {
        let root = std::env::temp_dir().join(format!(
            "ascii-art-rename-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let from = root.join("from");
        let to = root.join("to");
        fs::create_dir(&from).unwrap();
        fs::create_dir(&to).unwrap();
        fs::write(from.join("frame.png"), b"frame").unwrap();
        assert!(rename_noreplace(&from, &to).is_err());
        assert!(from.join("frame.png").exists());
        assert_eq!(fs::read_dir(&to).unwrap().count(), 0);
        fs::remove_dir(&to).unwrap();
        rename_noreplace(&from, &to).unwrap();
        assert!(to.join("frame.png").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn detects_truncated_frames_and_clean_eof() {
        assert!(read_frame(&mut Cursor::new([1, 2]), 3).is_err());
        assert!(read_frame(&mut Cursor::new([]), 3).unwrap().is_none());
        assert_eq!(
            read_frame(&mut Cursor::new([1, 2, 3]), 3).unwrap().unwrap(),
            [1, 2, 3]
        );
    }
}
