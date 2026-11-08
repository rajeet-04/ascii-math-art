# Motion Scene Implementation Plan

**Goal:** Export detailed animated art as standalone Rust source using fitted
patch motion, references and exact corrections.

**Architecture:** A new scene path shares FFmpeg decoding, analysis and temporal
history with the video path. It processes bounded reference spans and streams
compressed blocks into a generated source file. The same std-only decoding and
prediction module is included verbatim in the exported program.

**Tech stack:** Existing Rust dependencies, standard-library generated player,
system gzip and optional FFplay.

**Spec:** `docs/superpowers/specs/2026-11-08-motion-scene-design.md`

## Global constraints

Exact converted ASCII cells; actual fitted motion; count all embedded data;
bounded reference spans; safe output publication; preserve existing exports.

## Review focus

Partial or single-frame spans; invisible/newly exposed details; malformed blocks;
paths with spaces; deterministic motion search and matching generated playback.

## Tasks

1. [x] Add std-only scene decoder/predictor with system gzip in `scene_runtime.rs` and
   fitting/packing in `scene.rs`; test motion, exact corrections and malformed data.
2. [x] Add `scene` output format to `config.rs`, stage `.rs` output
   in `video.rs`, and route scene export in `main.rs`. Stream spans and report size,
   correction coverage and target hash.
3. [x] Generate a standalone Rust source player with embedded palette, font and
   compressed blocks. Compile it and compare full RGB output with PNG baseline in
   `tests/video.rs`; verify worker-independent source reconstruction.
4. [x] Run fmt, Clippy, tests and release build. Fit the user's GIF, compile the
   generated program, verify its hash and inspect reconstructed previews. Document
   measured compactness and limits in the engine README and a results note.

Execute directly in this workspace under the user's instruction to proceed. Run
an independent code review after implementation; fix material findings with tests.

## Execution record

Ponytail ultra scope: reuse gzip/FFplay, fixed fitting constants, no custom codec,
GUI or new dependencies. Known cost heuristic is documented beside track selection.
Seventeen unit and six integration tests pass, including compiling exported source,
worker-independent source bytes and exact RGB comparison to ordinary PNG export.
Independent read-only review found no material issues.

The user's GIF at 176 columns/20 FPS produces 73 exact reconstructed frames, four
reference spans and 236 moving patch curves. All 73 full RGB frame hashes match
the PNG baseline. Generated source is 1,008,580 bytes including 729,550 compressed
payload bytes. Motion alone predicts 44.4% of non-reference cells exactly; remaining
detail is supplied by corrections. This is not a hand-authored procedural recreation.

Terminal playback was added at the user's request: ANSI truecolor, initial terminal
size fitting, and frame-rate pacing through `--terminal`. The generated source and
binary were refreshed and verified in a pseudo-terminal, with the same cell hash.

Readability follow-up: resample colors before selecting terminal glyphs, shade
foreground/background, and offer Unicode half-blocks. The higher-detail example
has 352x225 cells and 4,795,920 source bytes. Both modes were verified across 73
frames in a pseudo-terminal and captured previews were visually inspected. The
test suite is now 18 unit tests and six integration tests.
