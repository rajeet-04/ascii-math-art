# Motion scene prototype: giphy.gif

Settings: ASCII, 176 columns, RGB332 palette, gamma 0.55, edge threshold 80,
stability 4, 20 FPS, four fitting workers. Fixed 8x8-cell patches, six-cell search,
24-frame references. Input is the user's repository-root GIF. Generated assets
are under ignored `engine/output/`.

| Measurement | Result |
|---|---:|
| Cell grid | 176 x 113 |
| Output raster | 1408 x 1808 |
| Reconstructed frames | 73 |
| Reference spans | 4 |
| Moving patch curves | 236 |
| Cells predicted exactly before corrections (excluding references) | 44.4% |
| Corrected cells avoided versus stationary references | 25,947 |
| Uncompressed reference data | 159,104 bytes |
| Uncompressed motion data, including stationary flags | 3,208 bytes |
| Uncompressed correction data | 1,861,059 bytes |
| Compressed payload | 729,550 bytes |
| Complete generated Rust source, including terminal player | 1,008,580 bytes |
| Uncompressed target cell stream | 2,903,648 bytes |

The complete source is about 65% smaller than the uncompressed two-byte cell
stream. This comparison is not a claim to beat a video codec or gzip applied to
the original frames: no such compression ablation was run. Most scene detail is
still carried by corrections, not the fitted equations. A few hundred lines of
fully procedural geometry cannot be promised for this detailed input.

Verification performed:

- Exported source compiled with rustc and passed its complete cell-stream hash
  check: `6415d332945424b8`, 73 frames.
- All 73 RGB frames from the compiled generated program were compared by frame
  MD5 against the ordinary lossless PNG export; every frame matches.
- Reconstructed RGB was encoded as `giphy-scene-reconstructed.mp4` for viewing.
- The automated integration test also compares complete raw RGB bytes on a short
  generated clip and verifies identical source exports at one/four workers.
- Independent review found no material correctness or lifecycle issues.

ANSI terminal playback was checked in a 100x30 pseudo-terminal: all 73 frames,
automatic fitting and frame-rate pacing. Graphical FFplay interaction and peak
resident memory were not measured. Exact
fidelity refers to the converted detailed ASCII grid, not the original RGB GIF.
The model handles local translation only; it does not infer a skeleton or 3D scene.

## Terminal readability update

Terminal display now averages colors before resizing, uses shaded ASCII with
foreground/background color, and offers Unicode half-blocks for two vertical
colors per cell. Encoded-cell reconstruction and full RGB output are unchanged.

An additional detailed export uses 352x225 cells, gamma 0.8 and stability 2. It
contains 1,462 moving curves, predicts 42.7% of non-reference cells exactly before
corrections, and has cell hash `c380cffb59af41fa`. Fitting/export took 0.544 seconds
in the recorded run. Compressed data is 3,541,935 bytes and complete source is
4,795,920 bytes. Source size counts the larger grid and all embedded correction
data, not just equations.

Both terminal modes were checked in a pseudo-terminal at 100 columns / 61 lines:
93x60 displayed cells, all 73 frames, about 3.61 seconds. Previews reconstructed
from captured ANSI foreground/background sequences were visually inspected using
the embedded bitmap font. Actual terminal fonts can differ. The half-block preview
keeps the cat and birthday text readable; increasing the terminal size reveals
more detail. New checks cover area averaging and both terminal modes. All 24 tests
pass.
