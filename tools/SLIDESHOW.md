# Key-8 six-face image gallery

The old flat panel and per-image-pixel normal/occlusion maps have been removed.
Key 8 now displays six nearby real beveled c1 cube slabs at 10% of the standard
world radius. A white 3×3×3 c4 landmark occupies the origin. An upright sparse
48×48 c1 cube asset plays above it at 100 ms per frame, and the player connects
on the +Z part of the top face looking toward it. The scene combines one retained textured mesh and PNG atlas (native PBR with
nearest filtering) with ordinary cube-patch instances in one depth-tested frame.

The complete importer, tier, placement, transport and rebuild instructions are
in [CubeSrv's README](../../TRUEOS-Blueprints/apps/cubesrv/README.md).

`src/slideshow.rs` builds tier geometry from `src/image_cube.rs`, exported from
the same GLB as CubeImage.html. After changing that reference, run
`python3 -B tools/prepare_image_cube.py`. The build checks the GLB digest;
`--check` verifies the complete generated export without rewriting it.

`src/network.rs` receives and validates the complete gallery package before
asking the native media API to decode its atlas. `src/slideshow_gpu.rs` owns
mesh buffers and texture lifetime. Complete Holy frames replace the dynamic
static asset's cube list; each record supplies a position and shared-palette
color. Transparent PNG pixels never become geometry. Same-tier replacements
reuse the mesh; changed-tier replacements create the new mesh before releasing the old one.
Failures preserve the old scene. Another numbered mode disconnects and releases
the gallery. The v8 package describes integer c1 cube grids, their 10%-radius
placement and their atlas; each image cube is 0.2 renderer units across. Like
other c1 detail, the images have no walking collision or Space-snap targets.
The 27 center c4 cubes use ordinary walking collision, while the standard
4×4×4-chunk movement envelope remains in effect.

Validation:

```sh
cargo check --offline
python3 -B tools/prepare_image_cube.py --check
python3 -B tools/test_slideshow_network.py
python3 -B tools/test_walker_camera.py
```

The network harness exercises all three geometry tiers, face orientation,
internal-face removal, bounds, package validation, chunk reordering/duplicates
and material submission. Native XeLP rendering and frame timing remain untested
until the rebuilt TRUEOS kernel, CubeSrv and Cubes are run together.

Holy placement experiment: frame updates retain each cell's identity by its
48×48 grid coordinate. The client uses the same `Reveal` implementation as placed
assets: 333 ms admission delay, rate-limited starts, and 700 ms grow-and-bounce.
Growth advances on every rendered frame; server announcements remain at 100 ms.
Continuing cells keep their growth through color changes and sparse-list reorderings.
Transparent cells disappear immediately; absence uses placement's 240 ms rearm
policy. Brief pixels can disappear before admission, so this experiment changes
the visible animation. The center landmark remains full size. This shares the
placement reveal behavior, not terrace editing, collision or visibility culling.
