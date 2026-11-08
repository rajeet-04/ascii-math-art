# Coordinate Equations Experiment

**Goal:** Fit an equation-only full-scene animation and measure whether it is
both detailed and compact.

**Spec:** `docs/superpowers/specs/2026-11-08-equation-only-target.md`

**Architecture:** An optional MLX fitting script learns sinusoidal RGB functions
from FFmpeg-decoded training samples. Rust exports validated quantized numeric
coefficients into a std-only evaluator and reuses existing terminal display.

## Tasks

1. [ ] Prove fitting quality on giphy.gif in an isolated trial. Record full-grid
   quantized PSNR, coefficient count, fitting time and actual source size.
2. [x] Add minimal equation export path in Rust and local fitting script; preserve
   existing outputs. Validate dimensions, parameter shapes and finite values.
3. [x] Compile exported source and compare Rust outputs with the fitted evaluator.
   Test synthetic coefficients and malformed models, then run the existing suite.
4. [ ] Inspect previews and measure playback. Document achieved results and any
   failed acceptance targets. Do not label a blurry model successful.

## Boundaries

No embedded frame/reference/residual data in the output; count all coefficients.
Keep temporary data and models under staging or ignored engine/output. GPU fitting
may require running outside the filesystem sandbox; source data remains local.
Use an independent review for any new parser/evaluator code before completion.

## Handoff status

User requested a continuation handoff before credits finish. Root `HANDOFF.md`
is the authoritative continuation record. The measured model/compiled player and
previews exist, 27 tests and Clippy pass, and independent review found no material
issues. Quality/size targets remain unmet, so Task 1 is not marked achieved.
Task 4 has raw playback/preview/size measurements but still lacks the new model's
terminal pacing check and integrated embedded-script GPU smoke test. No further
fitting experiments were left running.
