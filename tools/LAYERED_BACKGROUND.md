# Mandelbox background

Cubes opens one UI4 layered window. Picasso draws the foreground; a native
worker publishes Mandelbox on its independent background target. The worker
uses a 10 Hz ceiling and renders when the world, camera or extent changes.
Key 5 enables the upper sky hemisphere for the 27 worlds, using the six colors
from `Cube/cube_tree_builder_world_ramps.html`. Other modes and below-horizon
pixels use a neutral slate shade. UI4 applies background-only opacity 128/255.
The foreground retains its opacity. Looking completely below the sky retains
the shade without dispatching again when only camera or theme changes.

Each new extent first receives a cheap shade publication; the pair can commit
its resize before the next detailed render. UI4 exempts the first replacement
publication from its visual cadence deadline. An already submitted GPU job must
still retire before resize allocations can replace its target.

The shader is derived from the MIT-licensed `Cube/mandelbox.html`; its license
is preserved in `Cube/mandelbox/input.glsl`. This port uses the existing native
ShaderToy Image dispatch. It matches camera orientation and field of view with
a fixed fractal origin. The HTML's cached cubemap optimization is not included.
The tuned preset uses 7 folds, 72 march steps and 2 occlusion samples, with hit
tolerance `max(0.0025, travel * 0.00085)`. Rays below +Y=0 return before any
distance estimates. This saves about half the ray work at a level horizon;
the visible sky fraction changes with camera pitch.

Run `python3 tools/bake_mandelbox.py` after shader edits. This reproducibly bakes
and packages the shader and updates the sibling TRUEOS kernel's admission
hashes and ABI contract. `python3 tools/bake_mandelbox.py --check` verifies those
artifacts without changing them. `python3 tools/test_background.py` also checks
the actual 27-world theme mapping and palette provenance.

Both the kernel and Blueprint need rebuilding for the new layered ABI and
authenticated ShaderToy program 16. See
[`TRUEOS/tools/docs/UI4_LAYERED_FRAMES.md`](../../TRUEOS/tools/docs/UI4_LAYERED_FRAMES.md)
for plane budgeting, paired resize, lifecycle and validation limits.

## Host performance probe

`tools/benchmark_mandelbox.c` renders shade, horizon, upward and downward views
using the baked SPIR-V through host OpenCL. It writes PPM images and averages
five warm dispatch-and-finish timings per view. Build it from Cubes with
`cc -O2 -std=c11 tools/benchmark_mandelbox.c -o /tmp/benchmark_mandelbox -l:libOpenCL.so.1 -l:libX11.so.6 -lm`.
Create an output directory, then pass `kernel.spv output-directory width height`.
Use the pinned TRUEOS OpenCL ICD/toolchain environment when needed.

Measured on the host UHD 770, at 1920×1080: horizon 91.8 → 33.3 ms, full sky
129.8 → 85.0 ms, cheap shade 0.33 ms. At 784×441: horizon 18.4 → 6.8 ms,
full sky 24.4 → 15.6 ms. These are single-run host comparisons with warm samples,
not TRUEOS display, scheduling or maximize-latency measurements.
