# Key 4: compact opaque cube assets

Put `.cubes` files in `Cube/` and rebuild Cubes. The build discovers them in
filename order; runtime decodes each once at startup. Key 4 enters the first
asset and subsequent Key-4 presses cycle assets. No placeholder copies are
created for missing assets. Each asset may contain 1–1024 cubes. This stage
accepts version-1 opaque static assets only; rigs, alpha palettes, unsupported
flags, malformed records and unsupported scales fail explicitly.

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
Only those seeds enter the existing VS/HS/TE/DS path; no CPU cube mesh is built.

Visibility rejects cubes outside the camera frustum, then tests whether an
entire candidate's outer box lies in another cube's shadow volume. Occluders
use a box with 0.8 times the cube half-scale, strictly inside the actual bevel.
This is conservative: partially exposed cubes remain, and occlusion by a union
of multiple blockers is not detected. Remaining hidden surfaces are handled by
ordinary GPU depth testing. This is not GPU HiZ feedback or per-face HS culling.
The bounded pairwise CPU test may cost more than it saves on sparse scenes;
performance and visual behavior still need bare-metal validation.

The custom color flag takes precedence over overlapping Rubik/room/sphere flag
bits in both shader decoding and driver admission. Rebuild TRUEOS and Cubes:
contract/layout 8 prevents old kernels from silently misreading these colors.
Precompiled shader artifacts are checked in; no runtime shader compiler needed.

Checks: `cargo check`, `python3 tools/test_bake_patch_cube.py`,
`python3 tools/test_patch_overlay.py`, and
`rustc --edition=2024 --test src/orchard.rs -o /tmp/cubes-orchard-tests`
followed by `/tmp/cubes-orchard-tests`.
