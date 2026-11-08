//! These integration tests require ffmpeg (with libx264) and ffprobe on PATH.
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ascii-art test {} {stamp} {serial}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let this = Self(root);
        check(
            Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=96x64:rate=8:duration=1",
                    "-c:v",
                    "libx264",
                    "-threads",
                    "1",
                    "-pix_fmt",
                    "yuv420p",
                ])
                .arg(this.input())
                .output()
                .expect("tests require FFmpeg"),
        );
        this
    }
    fn input(&self) -> PathBuf {
        self.0.join("source with spaces.mp4")
    }
    fn convert(&self, destination: &Path, extra: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_ascii-math-art"));
        c.arg(self.input()).arg(destination);
        for (flag, value) in [("--fps", "8"), ("--columns", "24"), ("--scale", "2")] {
            if !extra.contains(&flag) {
                c.args([flag, value]);
            }
        }
        c.args(extra);
        c
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn check(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
fn metadata(path: &Path) -> Value {
    let out = check(
        Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-count_frames",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=width,height,nb_read_frames",
                "-of",
                "json",
            ])
            .arg(path)
            .output()
            .unwrap(),
    );
    serde_json::from_slice::<Value>(&out.stdout).unwrap()["streams"][0].clone()
}

#[test]
fn exports_both_styles_and_all_formats() {
    let f = Fixture::new();
    for mode in ["pixel", "ascii"] {
        for extension in ["mp4", "gif"] {
            let dest = f.0.join(format!("{mode}.{extension}"));
            check(
                f.convert(&dest, &["--mode", mode, "--threads", "2"])
                    .output()
                    .unwrap(),
            );
            let m = metadata(&dest);
            assert_eq!(m["nb_read_frames"], "8", "{mode} {extension}");
            let (w, h) = if mode == "pixel" {
                (48, 32)
            } else {
                (384, 256)
            };
            assert_eq!(m["width"], w);
            assert_eq!(m["height"], h);
        }
        let dest = f.0.join(format!("{mode} frames"));
        check(
            f.convert(&dest, &["--mode", mode, "--format", "png"])
                .output()
                .unwrap(),
        );
        assert_eq!(fs::read_dir(&dest).unwrap().count(), 8);
        assert!(dest.join("000000.png").is_file());
    }
}

#[test]
fn worker_counts_and_batch_boundaries_preserve_exact_frames() {
    let f = Fixture::new();
    for mode in ["pixel", "ascii"] {
        let a = f.0.join(format!("{mode}-one"));
        let b = f.0.join(format!("{mode}-four"));
        check(
            f.convert(
                &a,
                &[
                    "--mode",
                    mode,
                    "--format",
                    "png",
                    "--threads",
                    "1",
                    "--batch-size",
                    "1",
                    "--dither",
                    "--gamma",
                    "0.55",
                ],
            )
            .output()
            .unwrap(),
        );
        check(
            f.convert(
                &b,
                &[
                    "--mode",
                    mode,
                    "--format",
                    "png",
                    "--threads",
                    "4",
                    "--batch-size",
                    "3",
                    "--dither",
                    "--gamma",
                    "0.55",
                ],
            )
            .output()
            .unwrap(),
        );
        for i in 0..8 {
            let name = format!("{i:06}.png");
            assert_eq!(
                fs::read(a.join(&name)).unwrap(),
                fs::read(b.join(name)).unwrap()
            );
        }
    }
}

#[test]
fn protects_existing_outputs_and_handles_invalid_inputs() {
    let f = Fixture::new();
    let dest = f.0.join("existing.mp4");
    fs::write(&dest, b"keep me").unwrap();
    let result = f.convert(&dest, &[]).output().unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read(&dest).unwrap(), b"keep me");
    check(
        f.convert(&dest, &["--overwrite", "--frames", "3"])
            .output()
            .unwrap(),
    );
    assert_eq!(metadata(&dest)["nb_read_frames"], "3");
    assert!(
        !f.convert(&f.input(), &["--overwrite"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let mut bad = Command::new(env!("CARGO_BIN_EXE_ascii-math-art"));
    assert!(
        !bad.arg(f.0.join("missing.mp4"))
            .arg(f.0.join("bad.mp4"))
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(!f.0.join("bad.mp4").exists());
    let dir = f.0.join("already frames");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("keep.txt"), b"keep").unwrap();
    assert!(
        !f.convert(&dir, &["--format", "png", "--overwrite"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(dir.join("keep.txt").exists());
    assert!(fs::read_dir(&f.0).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".ascii-art-")
    }));
}

#[cfg(unix)]
#[test]
fn encoder_failure_keeps_destination_and_removes_staging() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let real = check(Command::new("which").arg("ffmpeg").output().unwrap());
    let real = String::from_utf8(real.stdout).unwrap();
    let scripts = f.0.join("bin");
    fs::create_dir(&scripts).unwrap();
    let wrapper = scripts.join("ffmpeg");
    let escaped = real.trim().replace('\'', "'\\''");
    fs::write(&wrapper, format!("#!/bin/sh\ncase \" $* \" in *' pipe:0 '*) echo 'forced encoder failure' >&2; exit 7;; esac\nexec '{escaped}' \"$@\"\n")).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(
        std::iter::once(scripts).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let dest = f.0.join("protected.mp4");
    fs::write(&dest, b"original").unwrap();
    let output = f
        .convert(&dest, &["--overwrite"])
        .env("PATH", path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(fs::read(&dest).unwrap(), b"original");
    assert!(String::from_utf8_lossy(&output.stderr).contains("forced encoder failure"));
    assert!(fs::read_dir(&f.0).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".ascii-art-")
    }));
}

#[test]
fn odd_dimensions_are_padded_for_h264() {
    let f = Fixture::new();
    let dest = f.0.join("odd.mp4");
    check(
        f.convert(
            &dest,
            &[
                "--columns",
                "23",
                "--rows",
                "15",
                "--scale",
                "1",
                "--frames",
                "1",
            ],
        )
        .output()
        .unwrap(),
    );
    let m = metadata(&dest);
    assert_eq!(m["width"], 24);
    assert_eq!(m["height"], 16);
}

#[test]
fn exported_rust_scene_compiles_and_matches_every_baseline_pixel() {
    let f = Fixture::new();
    let source = f.0.join("art scene.rs");
    let second = f.0.join("second.rs");
    let settings = ["--mode", "ascii", "--palette", "rgb332", "--gamma", "0.55"];
    let mut one = f.convert(&source, &settings);
    check(one.args(["--threads", "1"]).output().unwrap());
    let mut four = f.convert(&second, &settings);
    check(four.args(["--threads", "4"]).output().unwrap());
    assert_eq!(fs::read(&source).unwrap(), fs::read(&second).unwrap());

    let executable = f.0.join(format!("scene{}", std::env::consts::EXE_SUFFIX));
    check(
        Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .args(["--edition", "2024", "--crate-name", "scene"])
            .arg(&source)
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("scene tests require rustc on PATH"),
    );
    let verification = check(Command::new(&executable).arg("--verify").output().unwrap());
    assert!(String::from_utf8_lossy(&verification.stdout).contains("Verified 8 frames"));
    let started = std::time::Instant::now();
    let terminal = check(
        Command::new(&executable)
            .arg("--terminal")
            .env("COLUMNS", "24")
            .env("LINES", "9")
            .output()
            .unwrap(),
    );
    assert_eq!(
        terminal
            .stdout
            .windows(3)
            .filter(|s| *s == b"\x1b[H")
            .count(),
        8
    );
    assert!(terminal.stdout.windows(7).any(|s| s == b"\x1b[38;2;"));
    assert!(terminal.stdout.windows(6).any(|s| s == b";48;2;"));
    assert!(terminal.stdout.ends_with(b"\x1b[0m\n"));
    assert!(started.elapsed().as_secs_f64() >= 0.8);
    let blocks = check(
        Command::new(&executable)
            .arg("--terminal-blocks")
            .env("COLUMNS", "12")
            .env("LINES", "5")
            .output()
            .unwrap(),
    );
    assert_eq!(
        blocks.stdout.windows(3).filter(|s| *s == b"\x1b[H").count(),
        8
    );
    assert!(String::from_utf8(blocks.stdout).unwrap().contains('▀'));

    let frames = f.0.join("baseline");
    let mut png = f.convert(&frames, &settings);
    check(png.args(["--format", "png"]).output().unwrap());
    let baseline = check(
        Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(frames.join("%06d.png"))
            .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
            .output()
            .unwrap(),
    );
    let reconstructed = check(Command::new(&executable).arg("--raw").output().unwrap());
    assert_eq!(reconstructed.stdout, baseline.stdout);

    let single = f.0.join("single.rs");
    check(
        f.convert(&single, &["--mode", "ascii", "--frames", "1"])
            .output()
            .unwrap(),
    );
    assert!(
        fs::read_to_string(single)
            .unwrap()
            .starts_with("// Motion scene: 1 frames")
    );
    let before = fs::read(&source).unwrap();
    assert!(
        !f.convert(&source, &settings)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(fs::read(source).unwrap(), before);
}

#[test]
fn equation_source_compiles_and_generates_known_math_without_frames() {
    let f = Fixture::new();
    let model = f.0.join("coefficients.json");
    let mut description = serde_json::json!({"width":24,"height":16,"frames":8,"fps":8,"tiles":4,"omega":30.0,
        "layers":[{"inputs":3,"outputs":1,"scale":0.01,"weights":([100,0,0].repeat(16)),"bias":vec![0;16]},
            {"inputs":1,"outputs":3,"scale":0.01,"weights":([100,100,100].repeat(16)),"bias":vec![0;48]}]});
    fs::write(&model, serde_json::to_vec(&description).unwrap()).unwrap();
    let source = f.0.join("equations.rs");
    let settings = [
        "--format",
        "equations",
        "--equation-model",
        model.to_str().unwrap(),
        "--rows",
        "16",
        "--scale",
        "1",
    ];
    check(f.convert(&source, &settings).output().unwrap());
    let text = fs::read_to_string(&source).unwrap();
    assert!(text.contains("static LAYERS"));
    for forbidden in ["BLOCKS", "base64", "inflate", "decode_block"] {
        assert!(!text.contains(forbidden));
    }
    let executable =
        f.0.join(format!("equations{}", std::env::consts::EXE_SUFFIX));
    check(
        Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
            .args(["--edition", "2024", "--crate-name", "equations"])
            .arg(&source)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap(),
    );
    let raw = check(Command::new(&executable).arg("--raw").output().unwrap());
    assert_eq!(raw.stdout.len(), 8 * 24 * 16 * 3);
    for (i, rgb) in raw.stdout.as_chunks::<3>().0.iter().enumerate() {
        let x = (i % 24) % 6;
        let input = x as f32 * 2.0 / 5.0 - 1.0;
        let expected = (((input * 30.0).sin() + 1.0) * 127.5).round() as u8;
        assert_eq!(*rgb, [expected; 3]);
    }
    let before = fs::read(&source).unwrap();
    description["layers"][0]["scale"] = serde_json::json!(0);
    fs::write(&model, serde_json::to_vec(&description).unwrap()).unwrap();
    assert!(
        !f.convert(&source, &settings)
            .arg("--overwrite")
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(fs::read(source).unwrap(), before);
}
