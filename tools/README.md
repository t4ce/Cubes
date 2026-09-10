# Interactive seed-grid tessellation experiment

Status: **previous interactive puzzle user-verified; two-pass transparency and room transition await bare-metal validation**.
Cubes now requests the dedicated one-seed HS/TE/DS contract. The updated
TRUEOS kernel and Cubes application must both be rebuilt. An older kernel
rejects this contract rather than falling back to the imported triangle mesh.

The approved `Cube/cube.glb` contains a beveled cube: 24 unique positions,
96 stored render vertices, 44 triangles, and 26 face normals. Mesh-local
coordinates match the original importer; the GLB node translation is deliberately not
applied, just as in the baseline importer.

The experiment stores one 12-byte origin seed. Forty-four zero indices
submit 44 `PATCHLIST_1` patches referencing that seed. HS primitive ID and
invocation ID first map each triangle corner to one of the reference's 24
canonical positions, then select the per-corner normal from instruction
immediates. The 132-corner triangle mapping remains exact, while geometric
position state is explicitly 24 points. Each patch writes three control
points and unit tessellation factors. The triangle-domain DS interpolates the patch
and uses the existing camera matrix at byte 128. DS supplies world-space surface
data; PS keeps the baseline diffuse result for established modes and evaluates
the Key-7 metallic/roughness response. This is baked reference geometry, not
a general-purpose cube-generation or arbitrary-asset shader API.

## Checks and bake

Run from Cubes:

```sh
python3 tools/test_bake_patch_cube.py
python3 tools/test_patch_overlay.py
python3 tools/bake_patch_cube.py --out target/patch-cube --source-only
python3 tools/bake_patch_cube.py --out target/patch-cube
python3 tools/export_patch_driver.py target/patch-cube ../TRUEOS/crates/trueos-shader/generated_patch_cube.rs
```

Native compilation requires the sibling TRUEOS repository's pinned
`.codex_tmp/trueos-adj-instrumented-rpls` Mesa/glslang lane, including its
existing executable and adjacency capture instrumentation. The additional
instrumentation is preserved in `mesa-tessellation-capture.patch`; apply it
to that Mesa source and rebuild `libvulkan_intel.so` if the tessellation
capture files are missing. The patch is already applied in the current
workspace's compiler lane.

The bake requires the no-op DRM shim, records a Vulkan draw to expose
dynamic URB/TE state, and returns **before queue submission**. A fresh
`native-*` directory is allocated per attempt. `manifest.json` names that
directory and hashes the SPIR-V and captured state. Capture validation
rejects missing state, HS/DS scratch, and HS A64 geometry-table loads.
These checks do not validate all relocation types or prove rendering.

The HS/DS packet captures contain compiler-process kernel offsets, not
addresses that can be copied into a TRUEOS batch. The TE capture describes
dynamic state; it is not a complete ready-to-submit TE packet.

## Runtime contract

The retained ABI pairs `RETAINED_VERTEX_LAYOUT_CUBE_PATCH_SEED` with
`RETAINED_TOPOLOGY_CUBE_PATCHLIST_1`. Only a 12-byte zero seed and exactly
44 zero indices are accepted. The built-in shader bundle is restricted to
0xA780 and 0x4680, with no scratch, push-data or shader-data relocations.
HS and DS are uploaded after the ordinary shader ranges, and their KSPs
are relocated into the draw's instruction allocation. DS uses camera BTI1.
Per-draw state slots are 32 KiB, including a descriptor page after aligned
shader code; the former 20 KiB slots are too small for the canonical-position
hull shader.
Captured URB partitions and triangle-domain TE state are programmed; ordinary
draws explicitly disable HS/TE/DS and restore their ordinary URB allocation.

Contract version 7 instances six 10×10 room walls, a rotating 3×3×3 puzzle, or
a 1,024-seed sphere via V3's buffered transform seeds. VS reads
camera/instances/compacted IDs at BTI1/2/3.
Only translations, quaternion rotations, positive uniform scales and the
full 44-patch range are accepted. V3's material envelope must remain default;
the shader uses its baked palette/material table. Vertex layout ID 9 prevents
older translation-only kernels from accepting the oriented app. Rebuild both.
VS passes original instance identity through HS; DS uses BTI2 to apply the
instance matrix to generated positions and normals. DS supplies base color,
normal, view vector, roughness, metallic, and alpha in three vec4 varyings.

Key 2 is the default and starts ordered and compact (1.111-unit center spacing):
the 0.011-unit separation is exactly 1% of a cubie's 1.1-unit side, preventing
coplanar face overlap without scaling or expanding the cubies.
After three seconds without routed mouse movement or a held WASD key, its camera
slowly orbits the unselected compact puzzle; either input stops the orbit and
restarts that idle timer.
All 26 outer cubies are selectable, including the six face centers; the core is
excluded. Picking intersects the reference bevel's 26 convex planes and retains
the clicked local sticker face through subsequent layer turns.
Routed N-Mouse primary presses select at most one piece.

A correct click eases the entire puzzle to the existing 2.2-unit spacing over
one second. The camera first eases toward the selected piece's radial direction,
then tracks it through exactly three consecutive 1-second layer turns. Edge and
corner turns change axes to avoid immediate inverses; face centers use their
single available axis with a consistent turn direction. WASD orbits a fixed-radius
sphere when unlocked; mouse-look and Q/E are disabled in Key 2. Pressing Key 2
again resets the selection phase. Both orbital angles wrap without clamps, with
a pole-safe rotating up vector.

After the third turn, a 3.5-second eased cubic-Bezier flight curves toward the
clicked sticker face in its final orientation. The final approach is perpendicular
to that face and ends at its center. Its initial view is preserved and blends into the flight heading over
450 ms. The window fades out over the last 700 ms, switches directly to the
matching Key5 world, and fades in over 900 ms. This is a fade-through, not a
simultaneous two-scene crossfade. Arrival uses the clicked sticker's equivalent
Leave portal, one voxel in front on its connector, facing the world center.
World-linked portals reverse the flight into Key2, perform exactly one shared
layer turn while expanding, and enter the destination's reciprocal face. Generic
Leave portals exit to Key2 without turning. A five-second arrival cooldown prevents
immediate re-entry. The core/Void has no direct Key2 click entry. On-device portal
journey feel remains to be verified.

Key 1 places 100 seeds on each of six walls at ±6 units, with a 75° vertical
field of view (Key 2 remains 60°). This makes the surrounding room cubes read
farther away and smaller. Whole expanded cubes
on +X/-X/+Y/-Y/+Z/-Z use the matching opaque red/orange/white/yellow/green/blue
palette colour; they do not use the puzzle's translucent sticker pass. Its camera
stays at the origin; W/S look up/down and A/D look left/right. Mouse-look and
Q/E are disabled.

Press Key 1 again to toggle to 1,024 Fibonacci-distributed seeds on a radius-six sphere around
the camera. It uses Key 1's centered WASD look controls; every seed uses an RGB
gradient from its sphere position, both as a dot and as an expanded cube. Routed
cursor movement expands the nearby seeds with a screen-space circle whose area
is 10% of the current viewport. Another Key 1 press returns to the room;
from other modes Key 1 enters the room. Key 3 is unassigned.

Key 7 places six identical expanded cubes in one centered row. They retain the
red, orange, white, yellow, green, and blue palette and share one texture-free
GGX/Smith/Schlick pixel-shader path. Their data-only presets are matte paint,
satin plastic, glossy plastic, rough metal, satin metal, and polished metal.
The camera orbits as in Key 4; changing finish does not change geometry, draw
count, texture sampling, shader executable, or material-specific passes.

Counts are logged once per second in four 250 ms samples; no HUD task or
counter window is used. Modes 1–3 report expanded cubes and compact markers.
Key 4 reports all authored source cubes, frustum survivors, occluded cubes,
pending pop-ins, visible submissions, and submitted/avoided cube patch references. See
`COUNTERS.md` and `ORCHARD.md` for visibility behavior and host validation.

A gray 22-segment LINE_LIST floor at Y=3.5 provides orientation. It is one
retained static draw with CPU homogeneous line clipping and vertex refreshes;
the kernel's former one-segment restriction is widened to at most 64 segments.
Rebuild TRUEOS as well as Cubes for this static-line admission change.

Integer cell
coordinates and orientation bases are committed at turn boundaries to avoid drift.
Seed flag 0x100 enables six stickers by original cubie ID and local axial normal:
+X red, -X orange, +Y white, -Y yellow, +Z green, -Z blue. Only the 54 original
outward square faces receive these colors (two triangles each); bevels and
inward faces remain baseline mid-gray. Only mode 2's six palette sticker faces use
35% straight alpha; every baseline surface and mode 1 stay opaque.
Group 0 emits only opaque surfaces with depth writes enabled, followed by the
floor. Group 1 emits the 54 individual sticker faces sorted far-to-near each
frame, with depth testing but no depth writes. This is face-center sorting,
not general order-independent transparency for intersecting geometry.
Original cubie identity is retained in flag bits 0..4; bit 9 selects the
transparent pass and bits 10..12 select its single local face. The room uses
600 opaque seed rows and a culled dummy row to keep both groups stable.
The V3 seed limit is now 2048 in both kernel and SDK.
Colors rotate with cubies, not world axes.
Expanded spacing, cubie scale and puzzle origin are unchanged.

Each routed, focused N-Mouse cursor activates cubes within a radius linked to
the expanded cube's projected scale.
Outside every cursor radius, HS emits two camera-facing triangles as a 9-pixel seed
marker and culls the other 42 patches. These are indicators, not tiny cubes
or hardware point-list primitives. The marker scale compensates for camera
distance. Active seeds expand to the original 44-triangle beveled cube.
The retained frame's existing transform/indirect compute dispatch remains;
it does not expand the cube mesh. Geometry expansion is performed by HS.

`build.rs` checks the reference GLB hash against the app-local exported
`Cube/cube_driver_manifest.rs` (written alongside the driver bundle)
instead of expanding imported vertices. The source GLB is retained as a
reference asset, but is no longer used as runtime draw geometry.
This works inside Blueprint source overlays without a sibling TRUEOS tree.
The overlay test also verifies that a stale asset is rejected. The manifest
does not attest which kernel is currently booted.

Host checks: `cargo check` in TRUEOS and Cubes, plus standalone shader tests:

```sh
# From TRUEOS:
rustc --edition=2024 --test src/intel/shader.rs -A warnings -o /tmp/trueos-patch-cube-tests
/tmp/trueos-patch-cube-tests
```

CPU tests compare the 132 corner-to-24-canonical-position mapping and every
per-corner normal bit with the approved reference, but cannot prove GPU output, tessellator ordering, rasterization,
or bare-metal stability. Compare a rendered frame against the reference
before claiming verified 1:1 presentation. No rig deployment or reboot was performed.
