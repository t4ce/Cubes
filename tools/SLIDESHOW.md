# Key-8 six-face image gallery

The old flat panel and per-image-pixel normal/occlusion maps have been removed.
Key 8 now displays six nearby real beveled c1 cube slabs at 10% of the standard
world radius. A white 3×3×3 c4 landmark occupies the origin. An upright sparse
32×32 c1 cube asset plays above a temporary server-spawned terrain block at 150 ms per frame, and the player connects
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

The server spawns a c4 terrain block every 3 seconds within 5–10 blocks of the
center. Its top is level with the landmark. After 500 ms, one 16-frame VFX loop
plays above it, then both disappear. The block uses the world's terrain colour
and walking collision. Complete VFX frames display immediately, without the
placement growth delay. Opaque terrain and VFX use group zero; transparent
navigation alone uses group one. Capacity reserves one extra opaque spawn slot.

Key8 now receives world1 (`world_01_sky.cubes`) through the normal CUB1 world
welcome/request/chunk exchange before publishing the gallery. The ordinary level
decoder, walker terrain collision and nearest-first visibility selection supply
the terrain; its cube instances share depth with the images, landmark and VFX.
The player still starts on the central landmark. Rebuild both apps.

Key8 navigation uses the normal landing highlight in flight and the Tab target
and pathchain when walking. These are rendered as a sorted transparent cube
group after opaque terrain/Holy, sharing depth with the images. The landmark
is also a highlight source. This requires the updated TRUEOS mixed-draw renderer.

The textured shader's compiled clip-Y inversion is compensated by its viewport
Y scale and front winding in TRUEOS. Cube geometry keeps its existing viewport
convention, so both use the same unmodified world camera. Rebuild TRUEOS and Cubes.
