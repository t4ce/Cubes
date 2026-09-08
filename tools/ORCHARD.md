# Key 4: compact opaque cube assets

Put `.cubes` files in `Cube/` and rebuild Cubes. The build discovers them in
filename order; runtime decodes each once at startup. Key 4 shows adjacent
pairs of assets side by side; subsequent presses cycle pairs. Each pair is
limited to 1024 total seeds. The orchard and pine use 900 together, retain
their authored scales/colors, align their bases, and have a one-world-unit gap.
Camera bounds and visibility cover the combined pair. No placeholder copies are
created for missing assets. Each asset may contain 1–1024 cubes. This stage
accepts version-1 opaque static assets only; rigs, alpha palettes, unsupported
flags, malformed records and unsupported scales fail explicitly.

## Key 5: streamed lvl27 worlds

`Cube/lvl27/` contains the 27 standalone world exports. Key 5 enters this
mode; every further Key-5 press advances one world and wraps after World 27.
World assets may contain up to 4,096 authored cubes, but the retained renderer
stays capped at 1,024 seeds. Each frame uses the existing conservative
frustum/occlusion pass, orders survivors nearest-first, and admits only the
first 1,024 before the existing VS/HS/TE/DS path. Orbiting changes that
camera-relative selection; no CPU mesh expansion, extra renderer submission,
or hull-shader change is involved.

WASD orbits the centered asset; after three seconds without camera input it
orbits automatically. Mouse position does not control expansion. Asset +Y is
mapped to demo -Y (up). Framing uses asset bounds without resizing the cubes.
Only the strict-grid nature v1 layout in `Cube/CUBES_FORMAT.md` is accepted.
Coordinates are minimum occupied cells: center = (origin + size/2) * grid_unit.
Rendered side = (size - gap_percent/100) * grid_unit. Gap must be 1..99 percent;
the builder uses 1 percent. Overlapping occupied cells and nonzero reserved
fields are rejected. The same decoder validates every asset at build time.
There is no runtime legacy-format detection. The old orchard was migrated
losslessly; its original is retained as `Cube/cube_orchard.center-v1-backup`,
which is not packaged as an asset. `tools/migrate_center_cubes.py` is an explicit
offline converter, not a supported alternate runtime format.
Palette
colors are quantized to RGB555 and use the existing DS lighting, alpha 1.

One logical seed per cube remains the source. Submitted geometry is the
existing 44 PATCHLIST_1 references, not a POINT_LIST masquerading as patches.
Visible seeds are compacted and sorted near-to-far on the CPU before upload.
Only admitted visible seeds enter the existing VS/HS/TE/DS path; no CPU cube
mesh is built.

Newly exposed cubes now wait 120 ms, then pop directly to their full authored
size and color. Admission follows the culler's nearest-first order at up to
1,600 new cubes per second, capped at 96 starts in any one frame. Waiting cubes
have no submitted seed or marker and do not provide occlusion coverage. A cube
that becomes hidden leaves the draw immediately; if it reappears within 240 ms
of the first observed hidden frame, its admitted state is reused. Longer
absences rearm the delay. Waiting cubes lose accumulated exposure time whenever
they become hidden. This avoids repeating the effect for brief culling changes
while suppressing fleeting new exposures. History uses authored IDs, never
compacted upload positions. Every Key4 entry/page change resets it.

This is intentional temporary pop-in. Once a stationary view catches up, it
uses exactly the former cube IDs, order, scales, colors and patch counts. Extra
savings occur during entry and visibility changes, not after settling. The
parameters are together at the top of `src/reveal.rs`. A stalled render cannot
bank more than the 96-start burst cap or count as an observed hidden frame.
At very low FPS the per-frame cap reduces the effective admission rate.

Visibility rejects cubes outside the camera frustum, then sorts survivors by
nearest projected outer-box depth. A persistent 320×180 CPU buffer accumulates
coverage from the union of accepted cubes. A cell is filled only when its whole
area lies strictly inside the projected convex hull of a cube's inner box
(0.8 times the half-scale). Its stored depth is the inner box's farthest depth.
A candidate is removed only when every cell touched by its padded outer screen
rectangle has proven coverage at a strictly nearer depth. The outer box contains
the bevel; the inner box is strictly inside it. Near-plane intersections,
camera-inside views, invalid projections and partial coverage retain the seed;
uncertain cubes cannot supply occlusion coverage. The buffer resets each frame.
Reveal admission happens after the visibility test and before stamping either
the collective buffer or the single-blocker list, so pending cubes never hide
an already revealed background cube.

The exact single-blocker shadow-volume test remains as a fallback, with cheap
projected bounds rejection before ray tests. This matters for these small cubes:
a buffer-only 160×90 test removed none of the 477 orchard seeds in the existing
frontal test, and even 320×180 submitted more than the old culler at the paired
asset's default orbit distance. Combining the buffer with the exact fallback
adds collective occlusion without losing sub-cell single-blocker coverage in
the tested views. Scratch vectors are reused, with no new allocations after
capacity warmup. The depth buffer uses 230,400 bytes; projected and blocker
records use additional reusable storage. The fallback retains an O(N²) worst
case, so this is not a claim of eliminating all pairwise CPU work.

Host regression checks cover collective occlusion by four disjoint blockers,
pixel/cell cracks, incomplete coverage, wrong depth, near-plane/inside-camera
cases, invalid transforms, empty views, and all four authored sizes. The inner
box is checked against the picking bevel planes, which are checked against the
baked GLB by the shader-source tests. An independent bevel-ray oracle checks
96×54 sample rays at 24 orbit views against all source cubes. In the 12 sampled
default-distance views, 84–243 of the 900 seeds were skipped, with 0–45 additional
seeds skipped versus the old single-blocker pass. This is sampled host evidence;
GPU image equivalence and runtime performance still need bare-metal validation.
With pop-in enabled, the same 12 stationary views reached the exact baseline
submission in 536–670 ms at simulated 67 ms frame intervals (about 15 FPS).
A moving orbit simulation after startup averaged 12.6 pending cubes and 12.3
fewer submitted cubes than immediate admission, then matched the baseline when
the camera stopped. These are host simulations, not measurements of the new
behavior on hardware.

Counters retain all authored instances in the denominator: `S` source, `F`
after frustum, `O` removed by occlusion, `Q` waiting for reveal, `V` actual
visible submissions, `P = V×44` patch references, and `X = (S−V)×44` avoided
patch references. See `COUNTERS.md`.
These are application submission counts, not measured hardware invocations.
Each surviving cube still submits the existing 44 patches. Remaining hidden
surfaces use ordinary GPU depth testing; this is not per-face HS culling or GPU
HiZ feedback. Conservative uncertainty intentionally keeps some hidden cubes.

The custom color flag takes precedence over overlapping Rubik/room/sphere flag
bits in both shader decoding and driver admission. Rebuild TRUEOS and Cubes:
contract/layout 8 prevents old kernels from silently misreading these colors.
Precompiled shader artifacts are checked in; no runtime shader compiler needed.

Checks: `cargo check`, `python3 tools/test_bake_patch_cube.py`,
`python3 tools/test_patch_overlay.py`, and
`rustc --edition=2024 -O --test src/orchard.rs -o /tmp/cubes-orchard-tests`
followed by `/tmp/cubes-orchard-tests`.
