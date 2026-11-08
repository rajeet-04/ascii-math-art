# Rust video art engine

Build a standalone Cargo CLI in `engine/` that converts videos into animated
pixel art or colored ASCII. The existing Python artworks remain independent.
The user selected both styles and authorized starting with Rust.

## Approaches

Use direct conversion in Rust with FFmpeg decoding and encoding. Compared with
Go this gives explicit buffer ownership and Rayon data parallelism; compared
with AI stylization it is deterministic and needs no model or GPU. No speedup
claim is made before measurement.

## First version

- FFprobe supplies display aspect ratio; FFmpeg samples FPS and resizes before
  frames enter Rust as RGB24.
- Fixed clip-wide palettes, optional screen-anchored ordered dithering, and
  color/character hysteresis reduce flicker. Large scene changes reset history.
- Rayon processes independent frames in bounded batches. Stabilization consumes
  frames in order. Pixel enlargement and cached bitmap ASCII glyphs produce RGB.
- CLI controls mode, columns/rows, scale, FPS, palette, dithering, stability,
  worker count, and overwrite behavior.
- Detail tuning increases columns rather than enlargement scale. A precomputed
  gamma curve lifts existing shadow detail before character and palette mapping;
  raw source values still drive temporal thresholds. User requested finer detail
  after testing `giphy.gif`; detailed examples use 176 columns and RGB332 colors.
- Output MP4, GIF, or a directory of PNG frames. First version exports silent
  animation; audio and variable-frame-rate preservation are outside scope.
- FFmpeg worker count is separately limited to avoid CPU oversubscription.
- Never replace existing output without `--overwrite`; stage output beside its
  destination and publish only after both subprocesses succeed. Reject truncated
  frame data and clean child processes and temporary output on failure.
- Use checked dimensions and bounded batches; memory is bounded by configuration,
  not clip duration. Report frames, elapsed time, FPS and output bytes.

## Validation

Unit tests cover palette mapping, deterministic dithering, hysteresis, aspect
ratio, ASCII edges and dimensions. End-to-end tests generate a short FFmpeg clip,
verify every export format with FFprobe/decoded frames, compare single/multiple
worker output, and exercise invalid input and output protection. Benchmark the
same source/settings at one and multiple workers. Visually inspect both styles.
