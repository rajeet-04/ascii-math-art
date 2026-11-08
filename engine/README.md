# Video to pixel art and ASCII

A Rust command-line engine that converts videos, GIFs and images into animated
pixel art or colored ASCII. No model, GPU, Python runtime or image editor is
required. The existing Python artworks are independent of this tool.

## Build

Install Rust 1.88+ and FFmpeg with `ffprobe` and the `libx264` encoder on PATH.
From the repository root:

```sh
cargo build --release --locked --manifest-path engine/Cargo.toml
engine/target/release/ascii-math-art --help
```

The compiled binary can be run directly. FFmpeg and FFprobe must remain on PATH.

## Convert

```sh
# Retro pixel animation, fixed 16-color PICO-8 palette
engine/target/release/ascii-math-art input.mp4 pixel.mp4 --mode pixel

# Colored ASCII; choose a 256-color palette for photographic material
engine/target/release/ascii-math-art input.mp4 ascii.mp4 \
  --mode ascii --palette rgb332 --columns 160

# More detail in giphy.gif: finer blocks and more colors
engine/target/release/ascii-math-art giphy.gif pixel-detail.gif \
  --mode pixel --columns 176 --scale 2 --palette rgb332 --fps 20 --stability 4

# More detail in dark subjects: denser characters and lifted shadows
engine/target/release/ascii-math-art giphy.gif ascii-detail.mp4 \
  --mode ascii --columns 176 --palette rgb332 --gamma 0.55 \
  --edge-threshold 80 --fps 20 --stability 4

# Lossless PNG sequence in a new directory
engine/target/release/ascii-math-art input.mp4 frames --format png --mode ascii

# Monochrome ASCII; optional stable ordered dithering for palette modes
engine/target/release/ascii-math-art input.gif mono.gif --mode ascii --monochrome
engine/target/release/ascii-math-art input.gif dither.gif --dither
```

## Math-equation ASCII art: video or image

`--format equations` turns any input FFmpeg can read into one standalone Rust
file whose picture comes only from fitted equations (sine coordinate networks,
numeric coefficients, no stored frames). Each input gets its own equations.

```sh
export ASCII_ART_PYTHON=/path/to/python-with-mlx   # fitting only
# Video/GIF -> animated equations (<=120 frames, <=16 MiB decoded RGB)
engine/target/release/ascii-math-art clip.gif clip.rs --format equations \
  --columns 256 --rows 256 --scale 1 --fps 20 --fit-steps 45000
# Image -> equations (one frame; fits near-losslessly in under a minute)
engine/target/release/ascii-math-art photo.png photo.rs --format equations \
  --columns 256 --rows 256 --scale 1 --fit-steps 20000
rustc --edition 2024 -O clip.rs -o clip && ./clip --terminal
```

- `--terminal`: artistic ASCII (glyph tiers, outlines, trail, glints).
  `--terminal-blocks`: faithful colour blocks. `--raw`, `--verify`, `--play`.
- The equations are continuous, so the player evaluates them at your terminal's
  size. **Zoom the terminal out for smaller, more detailed characters.** It
  loops until Ctrl-C; still images keep shimmering.
- More fit pixels (`--columns/--rows`, up to 256x512) keep small text legible.
- Coefficients are kept as `<output>-model.json`; re-export with
  `--equation-model <file>` without refitting.
- Measured: Diwali GIF 256x256, 58 frames 32.78 dB; Diwali still 256x256
  55.03 dB; birthday GIF 128x164 27.57 dB. About 800 KB source each.

## Legacy motion scene export

Export one runnable Rust source file that uses reference images, fitted motion
equations and exact corrections. This is the prototype for inferring a compact
animation description from an existing video.

```sh
engine/target/release/ascii-math-art giphy.gif scene.rs \
  --mode ascii --columns 176 --fps 20 --palette rgb332 --gamma 0.55 \
  --edge-threshold 80 --stability 4
rustc --edition 2024 --crate-name scene -O scene.rs -o scene
./scene --verify
./scene --terminal
./scene --terminal-blocks
./scene --play
```

`gzip` must be on PATH for export and reconstruction; `--play` also needs FFplay
(included in many FFmpeg installations). The generated source uses only the Rust
standard library. It needs no Cargo project or external assets. `./scene --raw`
writes an RGB24 stream to stdout for piping into FFmpeg. `--help` lists playback
options. The existing output-protection rules also apply to `.rs` files.

`--terminal` displays shaded ASCII with foreground/background colors at the scene
FPS. It averages colors before selecting simple density characters so shrinking
the grid does not turn thin features into unrelated letters. `--terminal-blocks`
uses Unicode half-blocks with two vertical colors per terminal cell for clearer
image detail. Both modes fit to `COLUMNS`/`LINES` or the initial terminal size from
`stty size`. Enlarge the terminal or reduce its font size before starting to retain
more detail. Ctrl-C stops playback. Both modes require an ASCII scene and ANSI
truecolor; block mode also requires the `▀` glyph. Terminal display resamples the
reconstructed color grid; `--raw` and graphical playback keep the encoded glyphs.

The more detailed local example was encoded with:

```sh
engine/target/release/ascii-math-art giphy.gif engine/output/giphy-scene-detail.rs \
  --mode ascii --columns 352 --fps 20 --palette rgb332 --gamma 0.8 \
  --edge-threshold 80 --stability 2 --threads 4
rustc --edition 2024 --crate-name giphy_scene_detail -O \
  engine/output/giphy-scene-detail.rs -o engine/output/giphy-scene-detail
./engine/output/giphy-scene-detail --terminal-blocks
```

That example has a 352x225 color/character grid and about four times the cells of
the initial export. Its generated source is 4,795,920 bytes including embedded
data. More encoded detail costs storage; the terminal's dimensions remain the
limit on visible resolution.

The fitter keeps a reference every 24 frames and measures backward translation
of 8x8-cell patches within six cells. For each patch it fits
`dx(u)=a*u+b*u*u`, `dy(u)=c*u+d*u*u`, where `u` is normalized span time. It keeps
curves that save enough corrected cells to cover coefficient literals. It then
stores run-length encoded corrections and compresses reference/model/correction
blocks with gzip. This selection is a literal-byte heuristic, not a globally
optimal compression search. Fitting workspace is estimated and limited to 256 MiB.

Reconstruction is **exact relative to the converted ASCII frames**, including
glyphs and palette colors. It does not restore RGB detail removed by the initial
ASCII conversion. Newly visible surfaces and motion the quadratic patches cannot
explain are retained as correction data. All embedded payload bytes count toward
the source size; the code is not automatically as small as a hand-designed flower.

On `giphy.gif`, the detailed export uses 236 moving patch curves and four reference
spans. It produces 73 frames, and every reconstructed RGB frame matches the
ordinary PNG baseline. Source size is 1,008,580 bytes including 729,550 compressed
payload bytes. Motion/reference prediction explains 44.4% of non-reference cells
exactly; corrections supply the rest. See [measured results](SCENE-RESULTS.md).

Output format is inferred from `.mp4` or `.gif`. Use `--format png` for a frame
directory, or explicitly select another format with `--format`. Outputs are
silent and sampled at a fixed frame rate (default 24 FPS); GIF timing is rounded
to its centisecond clock. MP4 uses lossy H.264/YUV420 encoding, so its decoded
colors may differ from the exact palette. PNG is the reference for exact pixels.
MP4 adds at most one black row/column to satisfy even-dimension requirements.

## Detail and motion

- `--columns`: actual horizontal art resolution, default 120. Increase this to
  retain faces, fur and text. Doubling columns also approximately doubles rows.
- `--rows`: override automatic aspect-preserving row count. ASCII accounts for
  its tall 8x16 character cells; pixel mode uses square cells.
- `--scale`: enlargement only, default pixel 6 / ASCII 1. Increasing scale makes
  the same cells bigger; it does **not** recover image detail.
- `--palette pico8|grayscale|rgb332`: fixed 16-color retro, 16-gray or 256-color
  RGB palette. A fixed palette avoids palette changes between frames.
- `--gamma`: tone exponent, default 1. Values below 1 lift shadows before glyph
  selection and color mapping; above 1 darken. This reveals existing detail,
  without inventing detail absent from the source.
- `--edge-threshold`: default 40. Higher values retain more density characters;
  lower values emphasize directional edge strokes. 255 disables edge strokes.
- `--stability`: default 12. Hold cells until any source RGB channel changes by
  this amount. Use 3–6 for fine movement, or 0 to disable. Slow changes accumulate
  against the last accepted source, and large cuts reset history.
- `--dither`: screen-anchored Bayer pattern; optional because it can add texture
  that competes with fine details.
- `--frames`: cap output frames for quick previews.

## Threads and memory

```sh
engine/target/release/ascii-math-art input.mp4 result.mp4 \
  --mode ascii --threads 4 --batch-size 8 --ffmpeg-threads 1
```

FFmpeg resizes before handing RGB frames to Rust. Rust analyzes independent
frames in parallel, applies temporal decisions in frame order, then renders the
batch in parallel and writes it in order. ASCII uses a cached bitmap font and
palette mapping uses a 32,768-entry lookup table. Tone correction is also a lookup.
There is no per-pixel floating-point gamma calculation in the conversion loop.

The default Rust worker count is available cores capped at 8; default batch size
is workers capped at 8. A single-frame batch cannot exploit frame parallelism.
Keep the batch size identical when comparing worker counts. FFmpeg has separate
thread limits; the CLI does not automatically assign every core to each stage.
Decoding, conversion and encoding proceed in bounded batches, rather than storing
a whole clip. GIF palette generation also operates per frame to avoid whole-clip
buffering.

Configuration rejects batch working buffers above an estimated 512 MiB and output
dimensions above 8192x8192. This is a Rust frame-buffer budget, not a hard process
RSS limit; FFmpeg codecs, thread stacks and runtime allocations use more memory.
Subprocess error logs use temporary disk space and are removed with staging.

The summary reports end-to-end FPS (including setup, decoding and encoding),
conversion-only FPS, elapsed time, frame count and output bytes. Short clips have
substantial startup overhead; more workers need not improve end-to-end speed.

## Output protection

Existing MP4/GIF files require `--overwrite`. The input cannot also be the output,
and output symlinks are rejected. PNG exports always require a new directory,
including when `--overwrite` is given. Output is staged beside the destination
and published only after decoding and encoding succeed; failures remove staging
and reap both subprocesses. No-clobber publication uses an atomic exclusive rename
for directories and a hard link for files. On filesystems that do not support
these operations publication fails safely. Replacement rename with `--overwrite`
is atomic on Unix; Windows may reject replacement and retain the old file.

## Verify

```sh
cargo fmt --manifest-path engine/Cargo.toml -- --check
cargo clippy --locked --manifest-path engine/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path engine/Cargo.toml
```

Integration tests require FFmpeg/FFprobe, rustc and gzip. They verify both styles and all three
formats, frame counts, byte-identical PNGs across worker counts and batch sizes,
paths containing spaces, odd MP4 dimensions, frame limits, existing-output
protection, forced encoder failure and staging cleanup. Unit tests cover palette
mapping, gamma, dithering, temporal history, cuts, glyph orientation, configuration
limits, bounded diagnostics and exclusive directory publication.

Scene tests also check quadratic fitting, exact correction reconstruction,
malformed data, generated-source compilation and byte-identical RGB playback.

First-version limits: CPU rendering, bitmap font only, fixed FPS, no audio, no
motion compensation, no AI reconstruction, and no automatic optimal palette
learning. ASCII has less spatial detail than square pixels at the same column
count; dark footage benefits from gamma adjustment and more columns.
