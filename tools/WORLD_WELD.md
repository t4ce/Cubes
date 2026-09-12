# Key5 opaque-neighbor experiment

Status: host tests and native shader compilation pass; on-device appearance and
FPS remain unverified. Rebuild **both TRUEOS and Cubes** before trying this.
The changed generated shader lives in
`../TRUEOS/crates/trueos-shader/generated_patch_cube.rs`.

Enter Key5, then press **J** to toggle welding. Startup uses the original rendering.
The toggle persists when cycling worlds. Compare the same camera position with
J off/on; try a portal frame, a broad plateau, a pathway, and newly placed palette
cubes after their growth finishes. Move around edges and trigger a portal route
change to inspect transitions. J restores the original view without reloading.

The existing periodic counter report adds `world-weld`: enabled state, joined
cube count, removed internal faces, and estimated DS triangles saved. Existing
submitted seed/patch counts retain their original meaning. Record FPS alongside
these counts for the same view; fewer output triangles do not guarantee more FPS.

## Geometry

The reference cube has 44 triangles, including 32 bevel triangles. For equal-size,
fully face-adjacent cubes with the **same exact opaque palette color**, the patch:

- Recovers the authored grid bounds and closes the decorative spacing.
- Expands the twelve square triangles to sharp cube corners.
- Sets tessellation levels to zero for the bevels and **both** internal faces.

Two matching cubes produce **20 instead of 88 triangles**. A solid 3×3×3 block
produces **108 instead of 1,188**; its center emits no triangles. The visible
corners of joined cubes coincide, but there is no shared indexed vertex buffer:
this renderer generates geometry from retained seeds. External coplanar quads
remain separate. This experiment changes joined cubes' outer silhouette from
beveled to square; isolated cubes keep the original geometry.

The ABI still submits 44 patches per seed and runs their hull shaders. Savings
are in tessellation output, domain shading and rasterization. Neighbor matching
adds CPU work (roughly 1.6–1.8 ms for 8,192 cubes in the local warm host benchmark;
this is not rig timing). Buffers are reused; matching uses sorted keys and three
neighbor lookups per eligible cube.

## Boundaries

Only Key5's final submitted solid cubes participate, after CPU visibility,
admission and marker reduction. Markers, ghosts and transparent indicators do not.
Matching requires equal original scale, equal color/material identity, aligned
face bounds on the existing quarter-c1 asset grid, and no more than 0.004 renderer
units of decorative spacing (plus float tolerance). Other colors, sizes, partial
contacts and larger gaps retain their original geometry. No nearest-color match
is used: RGB555 theme colors keep their exact existing diffuse shading; semantic
palette materials retain their imported finish.

Only settled placed cubes participate. Moving portal pieces are excluded until
their route transition finishes. Seeds behind the eye cannot supply coverage,
matching the vertex shader. Adjacency is rebuilt each frame: removing a neighbor,
changing LOD, or delaying its reveal restores the remaining cube's face. Authored
assets, collision, walking, picking and placement are unchanged. The render-only
replacement hulls for distant platforms can participate if they satisfy the same
bounds/color rules.

The low flag word reserves `0x1800..0x1fff` for welding: color index in bits 0–3,
local +X/-X/+Y/-Y/+Z/-Z hidden-face mask in bits 4–9, and semantic-material marker
in bit 10. The tag excludes every valid Rubik face ID (0–5). The original
`0x1000` tag collided with transparent Rubik faces 4/5; rebuild both TRUEOS and
Cubes with this correction, including when welding is toggled off. The upper
16-bit retained slot is untouched. VS transports the mask as
an exactly representable binary fraction of its existing instance-ID varying;
HS restores the integer ID for DS. No additional descriptor or native URB layout
is needed. The complete native bundle fits the existing 40 KiB resident slot.

## Reproduce

From Cubes:

```sh
python3 tools/test_world_weld.py
python3 tools/test_bake_patch_cube.py
python3 tools/test_camera_sun.py
python3 tools/test_marker_lod.py
python3 tools/test_world_permutation.py
cargo check
python3 tools/bake_patch_cube.py --out target/patch-cube-weld
python3 tools/export_patch_driver.py target/patch-cube-weld ../TRUEOS/crates/trueos-shader/generated_patch_cube.rs
python3 ../TRUEOS/tools/test_patch_cube_capacity.py
```

The host test exercises all 27 exported worlds and reports full-scene geometry
counts before visibility/LOD, plus warm CPU cost. Those counts are not live frame
measurements. Native bakes were checked for both supported device IDs, `a780` and
`4680`, without submitting GPU work. Build/export checks reject a stale compact
world-color table. See `README.md` for the existing compiler lane and build context.
