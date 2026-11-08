# Equation-only animation target

This supersedes the representation goal of the reference/correction scene
prototype. Preserve that prototype as experimental work; do not describe it as
solving this target.

## User requirements

- Reconstruct the full scene, including the background.
- Fit mathematical functions describing appearance and motion over time.
- Generate executable scene code in the visual style of hand-written procedural
  ASCII artworks (see `DIWALI/diwali.py`): structured characters, shape-aware edges, lighting and
  deliberate motion.
- No embedded input frames, reference-image payloads, correction streams or
  per-frame pixel/glyph arrays disguised as source code.
- Preserve detail close to the original. Fit quality and compactness must be
  measured rather than assumed.

## Acceptance boundary

At playback time, scene functions and their fitted coefficients generate the
animation. Count all constants, coefficients and any model weights in the size.
Measure reconstruction error, source/model bytes, parameter count, fitting time
and playback speed. Compare against the existing detailed reference render.

Two representations require a user choice before implementation: readable shape
and motion equations like the original procedural scripts, or a compact learned
continuous function with numeric weights. The former requires fitting an explicit
scene model; the latter can represent detailed video without discovering semantic
objects. Neither guarantees a few hundred lines or near-original detail at a tiny
parameter count. Mathematical representation and artistic rendering are separate
requirements; a fitted RGB function alone does not supply 3D geometry or lighting.

## Selected experiment

The user accepts either representation provided it preserves detail and stays
small. Fit a sinusoidal coordinate network F(x,y,t) -> RGB, initially three hidden
layers of 128 units. MLX provides local GPU training; the engine and exported
evaluator remain Rust. No pretrained model or external service is required.

Use a bounded decoded grid, sample training coordinates, and evaluate error on
every decoded frame. Quantize fitted coefficients to signed 16-bit integers with
per-layer scales; measure the quantized reconstruction, not just training loss.
Emit plain numeric arrays and mathematical layer evaluation in a standalone Rust
file. No base64, gzip, embedded frames, reference images or residual streams.

The initial feasibility target is at least 28 dB full-grid PSNR, at most 512 KiB
complete Rust source, and readable full-scene previews. These are experimental
acceptance targets chosen for measurement, not claims already achieved. Report
failure honestly rather than relaxing quality or silently storing frames. Fitting
can take minutes and is distinct from the old sub-second residual exporter.

Ponytail ultra scope: reuse existing FFmpeg input/staging and terminal rendering,
use MLX autodiff instead of custom backpropagation, no 3D reconstruction or GUI,
and no model infrastructure beyond this local experiment.

## Current continuation state

The selected implemented model differs from the initial trial: sixteen spatial
coordinate functions, each with four hidden sine layers of width 64. It has
206,896 quantized coefficients. The working result is 800,038 bytes of complete
Rust source, 27.57 dB full-grid PSNR at 128x164, and ~34–36 native evaluation FPS.
It misses the initial 28 dB / 512 KiB acceptance targets. See root `HANDOFF.md`
for complete measured findings, reproduction commands, remaining gaps and the
user's request to preserve knowledge for continuation before credits finish.
