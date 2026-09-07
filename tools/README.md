# Interactive seed-grid tessellation experiment

Status: **single-cube rendering user-verified; interactive grid awaits bare-metal validation**.
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
and uses the existing camera matrix at byte 128. The fragment shader uses
the baseline's material-0 lighting. This is baked reference geometry, not
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
Per-draw state slots are 28 KiB, including a descriptor page after aligned
shader code; the former 20 KiB slots are too small for the canonical-position
hull shader.
Captured URB partitions and triangle-domain TE state are programmed; ordinary
draws explicitly disable HS/TE/DS and restore their ordinary URB allocation.

Contract version 3 instances a 24×12 XY lattice or static 3×3×3 lattice via
V3's buffered transform seeds. VS reads camera/instances/compacted IDs at BTI1/2/3.
Only translations and positive uniform scales with identity rotation and the
full 44-patch range are accepted. V3's material envelope must remain default;
the shader still uses baked material 0. Vertex layout ID 4 deliberately prevents
an old single-object kernel from accepting this instanced app. Rebuild both.

Each routed, focused N-Mouse cursor activates cubes within a radius linked to
the expanded cube's projected scale.
Outside every cursor radius, HS emits two flat triangles as a 3-pixel seed
marker and culls the other 42 patches. These are indicators, not tiny cubes
or hardware point-list primitives. The marker scale compensates for camera
distance. Active seeds expand to the original 44-triangle beveled cube.
The default Picasso fly camera uses WASD movement and middle-mouse look.
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
