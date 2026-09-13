# Key-8 six-face image gallery

The old flat panel and per-image-pixel normal/occlusion maps have been removed.
Key 8 now displays six nearby real beveled c1 cube slabs at 10% of the standard
world radius. A white 3×3×3 c4 landmark occupies the origin. Six sparse
32×32 c1 cube effects billboard toward the viewport above temporary server-spawned terrain blocks, and the player connects
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
mesh buffers and texture lifetime. `src/vfx_stream.rs` caches immutable VFX1 pixel-lifetime assets by revision.
Six server-timed slots evaluate these locally; unchanged pixels survive across
N frames without retransmission. Transparent pixels end their lifetimes.
VFX pixels use the same bounded grow-in/two-bounce curve as Key4/Key5 placement,
plus eased shrink-out within each compressed lifetime. Growth is capped at
700 ms or half the lifetime; shrink-out at 150 ms or half the lifetime.
Short VFX runs bypass placement's admission delay/queue. Unchanged pixels keep
their progress across frame boundaries, with no extra downloads or post-expiry
geometry. This is render-only scaling, not alpha blending; supports stay full-sized.
GPU seeds still update for billboarding and growth; meshes remain resident. Same-tier replacements
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

Each batch spawns six c4 terrain blocks: four cardinal positions,
one above and one below. Each independently rolls one effect from all 150 entries.
The size demo assigns +X=c1, +Z=c2, -X=r1, -Z=c3, above=r2, below=c4:
pixel sides are 1, 2, 3, 4, 6, 8 c1 units. The renderer also supports r3 (12).
Grid spacing scales with pixel cubes, giving full effect widths from 6.4 to
51.2 renderer units; terrain cubes remain c4. Positions are fixed on the six image directions at exactly twice the image
radius: (±41,0,0), (0,0,±41), (0,±41,0) in renderer units. They do not vary
with timer cycle or pixel size. Large effects can extend back toward the images.
All effects keep their bottom-center anchor at their terrain cube's top.
Size is a one-byte scene field; the same compressed asset/cache entry works at
every size without resampling, rebaking or another download.
After 500 ms, each plays one complete loop at exactly 500 ms/frame.
The next batch's first frame is one second after the longest loop expires.
New terrain bases appear halfway through that gap, retaining their 500 ms lead-in.
No strip is accelerated or cut off to fit a fixed cycle. Rebuild both apps:
snapshot age is now u32 (120-byte body) for long sequences.
Each cube disappears with its effect. The blocks use world terrain colour and
walking collision; local expiry still removes both when packets are lost.
Only VFX positions and rotations follow camera right/up; terrain stays upright.
Opaque terrain and VFX use group zero; navigation alone uses group one.
Capacity reserves 6×1024 pixels, six terrain cubes, 27 landmark cubes and 129
navigation slots, leaving 26462 nearest-world terrain seeds within the 32768 limit.
Rebuild TRUEOS as well as both apps: the SDK and native row limits changed;
other Cubes modes keep their existing 8192-seed UI budget.
The former standalone Holy path and per-frame downloads are removed.

Key8 now receives world1 (`world_01_sky.cubes`) through the normal CUB1 world
welcome/request/chunk exchange before publishing the gallery. The ordinary level
decoder, walker terrain collision and nearest-first visibility selection supply
the terrain; its cube instances share depth with the images, landmark and VFX.
The player still starts on the central landmark. Rebuild both apps.

Key8 navigation uses the normal landing highlight in flight and the Tab target
and pathchain when walking. These are rendered as a sorted transparent cube
group after opaque terrain/VFX, sharing depth with the images. The landmark
is also a highlight source. This requires the updated TRUEOS mixed-draw renderer.

The textured shader's compiled clip-Y inversion is compensated by its viewport
Y scale and front winding in TRUEOS. Cube geometry keeps its existing viewport
convention, so both use the same unmodified world camera. Rebuild TRUEOS and Cubes.
