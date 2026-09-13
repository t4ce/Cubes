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
geometry. This is render-only scaling, not alpha blending.
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

Each batch plays six VFX at fixed locations: four cardinal positions,
one above and one below. No temporary terrain cubes or collision are created. Each independently rolls one effect from all 150 entries.
Each slot independently rolls c1, c2, r1 or c3 per batch (pixel sides 1, 2, 3, 4
c1 units); duplicates are allowed. Larger presets remain supported but are not rolled.
Grid spacing scales with pixel cubes, giving full effect widths from 6.4 to
25.6 renderer units. Positions are fixed on the six image directions at exactly twice the image
radius: (±41,0,0), (0,0,±41), (0,±41,0) in renderer units. They do not vary
with timer cycle or pixel size. Large effects can extend back toward the images.
Each effect's canvas center is its exact server coordinate, without an upward
support offset. Billboard rotation applies only to centered pixel offsets.
After 500 ms, each plays one complete loop at exactly 400 ms/frame.
The next batch's first frame is one second after the longest loop expires.
The next effects are announced halfway through that gap, retaining their 500 ms preparation lead-in.
No strip is accelerated or cut off to fit a fixed cycle. Rebuild both apps:
snapshot age is now u32 (120-byte body) for long sequences.
Each effect expires locally even when packets are lost; it adds no walking collision.
Only VFX positions and rotations follow camera right/up; terrain stays upright.
Opaque terrain and VFX use group zero; navigation alone uses group one.
Capacity reserves 6×1024 pixels, 27 landmark cubes and 129
navigation slots, leaving 26468 nearest-world terrain seeds within the 32768 limit.
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

### Server snake

Key8 receives the five-c1-cube center snake through the same UDP connection as
VFX. Its 200 ms updates replace one stable tail slot with the new head; surviving
slots keep both transforms and colors. The client requests a snapshot on join
or a sequence gap and publishes each complete replacement atomically. Sprite
asset downloads continue to process snake packets.

Snake seeds occupy five reserved slots after the center landmark, before VFX.
They are axis-aligned, full-size c1 cubes in four fixed shades of theme 1's color.
The renderer compares each inactive seed buffer with its cached contents and
uploads only dirty row ranges; idle snake frames require no seed upload when
nothing else changes. The normal two-buffer upload still preserves the last
complete frame after an upload error. The terrain budget reserves five slots so
six full VFX planes, the snake and all navigation markers fit together.

### Server worm

The independent nine-segment worm uses the same stable slots and four-shade
rendering as the snake, selecting theme 2. Surface travel stays straight with
edge folds; an occasional inward move places its head on the opposite face
immediately, retaining its tangent heading. Body slots follow the route, and
self-overlap or overlap with the snake is permitted. Both creatures step every
200 ms with independent stream state and loss recovery.

Nine worm slots follow the five snake slots. The terrain budget now reserves
fourteen creature slots in total, preserving capacity for six full VFX planes
and all navigation overlays. Unchanged worm and snake seeds retain their colors
and transforms while either creature advances.

## Key8 preview / empty toggle

The first Key8 press connects to preview (world ID 1). After preview has been
installed, each new press switches between preview and empty (world ID 2).
Holding the key does not repeat. Swaps reuse the same UDP socket and native
worker; no additional transport, runtime or indexed-world loader is created.

An empty-world Hello receives the existing welcome envelope with world ID 2,
zero bytes and zero chunks. Only that matching acknowledgement clears the client
view. The server suppresses preview geometry, gallery, VFX, snake and worm traffic
for empty peers. A periodic Hello keeps the peer alive while empty. The client
uses the existing GPU surface-clear operation and removes its walker/targets;
preview GPU resources remain cached but submit no geometry while empty.
Returning to preview repeats the original binary world and gallery transfer.
Key5 remains local. Rebuild both Cubes and CubeSrv for this extension.

Host tests exercise preview → empty → preview repeatedly on one real UDP socket,
including stale welcomes. GPU presentation and dual-VM host responsiveness still
require a recoverable target test; host tests do not establish fault containment.

Key2 click/Space world entry now requests server empty explicitly after its entry
animation, regardless of which cubie/portal was selected. A fresh connection
starts with world ID 2; an existing worker is reused. The frame enters empty only
after acknowledgement, without requiring local terrain or a preview gallery.
Key8 can then switch to preview. Number-key navigation cancels a pending entry.
