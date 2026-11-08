# Initial conversion measurements

Measured on the local arm64 macOS environment, release build, FFmpeg 9.0.1.
Input: repository-root `giphy.gif`, 352x450, 55 frames, 3.67 seconds. All runs
sampled 20 FPS, producing 73 output frames. These are short-clip measurements,
not general performance guarantees.

For each mode and worker count, three sequential runs used the same batch size
of 8, FFmpeg thread limit 1, 176 columns, RGB332 palette and stability 4. Pixel
scale was 2 (352x450 output). ASCII scale was 1 (1408x1808 output), gamma 0.55,
edge threshold 80. Outputs were MP4. Worker counts alternated between trials,
and no other conversion or build was launched during this run. Values are medians.

| Mode | Rust workers | End-to-end FPS | Conversion-only FPS |
|---|---:|---:|---:|
| Pixel | 1 | 302.7 | 2853.7 |
| Pixel | 4 | 306.2 | 3940.1 |
| ASCII | 1 | 38.1 | 358.5 |
| ASCII | 4 | 38.4 | 812.5 |

ASCII conversion benefits from parallel rendering (about 2.3x in this sample),
but FFmpeg encoding and startup dominate total time. Four workers improved overall
throughput only slightly on this small clip. The CLI exposes worker/batch controls
instead of assuming more threads are always faster.

Detailed exports were decoded again with FFmpeg and checked with FFprobe: both
GIFs and both MP4s contain 73 frames and show motion. GIFs preserve 55 unique
decoded frames, matching the source's count; fixed-FPS sampling repeats frames.
Lossy MP4 encoding makes even repeated source frames potentially differ slightly.

| Output | Bytes |
|---|---:|
| Pixel GIF | 3,276,490 |
| Pixel MP4 | 1,117,832 |
| ASCII GIF | 30,363,746 |
| ASCII MP4 | 12,445,128 |

The detailed ASCII image has four times the cell count of the initial 88-column
version and a much larger raster. Higher detail costs storage and encoding time.
Output frames at one second were visually inspected for both styles. Full playback
has not been inspected in a video player.

Peak RSS was not measured: the sandbox denied the system query used by BSD
`time -l`. The configured 512 MiB allocation guard concerns Rust frame buffers,
not combined process memory. Machine core/model queries were also unavailable.

Raw local measurements, validation JSON, sample animations and preview PNGs are
under ignored `engine/output/`. Reproduce the quality settings with commands in
the engine README, holding `--batch-size 8` fixed while varying `--threads`.
