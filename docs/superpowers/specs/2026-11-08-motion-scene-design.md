# Motion-model scene export

## Intent and approved approach

The user wants a compact executable description of a detailed colored ASCII
animation, inspired by hand-written procedural ASCII artwork. They approved a reference
image plus fitted mathematical motion, with corrections to retain almost-original
detail. This extends the Rust converter; existing video exports stay available.

Two approaches were considered: complete semantic 3D reconstruction (not justified
for arbitrary GIFs), and a reference/patch-motion/correction model. Implement the
latter as a clearly labeled experimental prototype. Pure embedded frame playback
does not meet the objective; the exporter must actually estimate and fit motion.

## Representation

- Operate on the existing detailed ASCII cell grid, after palette/tone conversion
  and ordered stabilization. Exact fidelity means exact glyph and RGB color per
  cell at the sampled FPS, not lossless reproduction of the original RGB input.
- Split time into fixed 24-frame reference spans. Keep each span's
  first cell grid and track fixed 8x8-cell patches against its source RGB grid.
- Search backward translations within six cells, using RGB error.
- Fit dx(u)=a*u+b*u^2 and dy(u)=c*u+d*u^2, u in [0,1], with fixed-point coefficients.
  Stationary patches need no coefficients. Keep fitted curves only when their
  exact-cell prediction saves at least five corrected cells (ten literal bytes
  versus eight coefficient bytes). Full gzip byte cost is not optimized.
- Reconstruct from the reference and fitted curves; run-length encode sparse
  exact glyph/color corrections. Periodic references bound drift and visibility
  changes. System gzip compresses repeated payload bytes.
- Export one standalone `.rs` file with visible prediction equations, an embedded
  palette/font, compressed reference/model/correction blocks, and a std-only
  decoder/player. All embedded bytes count toward reported size.
- Generated programs support verification, RGB output, and playback through
  ffplay. rustc is needed to compile; gzip is needed to reconstruct. FFmpeg is
  only needed for source decoding or graphical playback, not verification or RGB
  reconstruction. No additional Rust dependencies are introduced.

## Bounds and safety

Reference spans contain up to 24 frames (the final span can be one frame). Reject estimated
scene workspace above 256 MiB and oversized decoded blocks. Export incrementally
to staging so clip duration does not grow fitting memory. Validate compressed
data, lengths, correction runs and palette indices when decoding. Existing
atomic publication and overwrite protections apply to generated sources too.

## Validation and limits

Test known translation trajectories, quadratic fits, cuts/occlusion corrections,
zero-motion spans, single-frame final spans, palette packing, malformed gzip/base64
and truncation. Compile exported source and verify its cell hash and frame count.
Compare its complete RGB stream byte-for-byte with ordinary PNG export for a
generated clip. Test the user's giphy.gif at 176 columns and RGB332/shadow settings,
record sizes and motion-only prediction coverage, and render reconstructed video.

This is local translational motion fitting, not inferred bones, full optical flow,
3D reconstruction or a learned model. It does not promise arbitrary detailed GIFs
will reduce to a few hundred lines. If corrections dominate, report that result.

## Ultra-mode scope decision

The user invoked ponytail ultra during implementation. Reuse system gzip and
FFplay instead of writing a custom codec or GUI. Keep fitting controls as constants
instead of expanding CLI flags. Do not add semantic object detection, bones,
neural models or unrelated refactors. Preserve validation and existing exports.
