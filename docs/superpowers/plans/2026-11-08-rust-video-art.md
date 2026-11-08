# Rust Video Art Implementation Plan

**Goal:** Convert videos into stable pixel art and ASCII animations in Rust.

**Architecture:** FFmpeg streams small RGB frames into bounded Rayon batches.
Ordered stabilization feeds rendering and an FFmpeg encoder with staged output.

**Tech Stack:** Rust/Cargo, Rayon, clap, anyhow, serde_json, font8x8, FFmpeg.

**Spec:** `docs/superpowers/specs/2026-11-08-rust-video-art-design.md`

## Global constraints

Preserve existing Python pieces. No model or GPU requirement. Silent, fixed FPS
exports. Bounded memory. Explicit overwrite. No automatic commits or publishing.

## Review focus

Truncated decoder output; unavailable codecs; paths with spaces; scene cuts and
slow motion; odd dimensions and short or invalid inputs.

## Tasks

1. [x] Add Cargo CLI and validated geometry/configuration (`engine/src/config.rs`).
   Test aspect preservation, limits, and format detection with `cargo test`.
2. [x] Add conversion and rendering (`palette.rs`, `render.rs`). Test palette,
   dithering, motion hysteresis, scene cuts, and glyph orientation/dimensions.
3. [x] Add streaming FFmpeg lifecycle and staging (`video.rs`, `main.rs`). Test
   partial frames, error cleanup, overwrite protection, and ordered worker output.
4. [x] Add README examples and integration smoke checks (`engine/tests/video.rs`).
   Run fmt, clippy, tests, release build; generate and inspect both styles;
   record measured performance without extrapolating to all videos.

Execute directly in the current workspace. Dependencies/toolchain may be cached
under /tmp because Rust is not preinstalled. Review the final implementation and
record any remaining limits in the engine README.

## Execution record

The release binary builds. Fourteen unit tests and five FFmpeg integration tests
pass. Formatting and Clippy with warnings denied pass. Both styles were rendered
from a six-second generated test video and the user's `giphy.gif`, with previews
visually inspected. Detailed variants retain a finer grid and lift ASCII shadows.

Independent review found two issues: diagnostic loading read a full log before
truncating it, and PNG publication used a nonexclusive check-then-rename. Both
were fixed with regression tests: diagnostics now consume at most 4096 bytes;
Unix directory publication uses rustix's atomic NOREPLACE rename. New tests were
first run red, then green with the changes; the complete suite passed afterward.

PNG directories intentionally require a new destination even with --overwrite;
this protects unrelated directory contents. Exports are silent, fixed-FPS and
CPU-only. Thread-count comparisons use the same batch size. Peak resident memory
has not been measured: the sandbox denied the system query used by BSD time.
