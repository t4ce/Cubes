# World marker averaging

Key5/world-view uses `src/marker_lod.rs` after visibility, reveal admission and
the detailed-cube decision. Expanded cubes keep their exact geometry and color.
Only unexpanded RGB555 dots are grouped; this
does not modify authored geometry, collision, mining, portals or server data.

The distance reference is the selected world's bounding-box diagonal, computed
on selection/placement. Current lvl27 worlds are about 409.6 renderer units per
side, with a 709.4-unit diagonal. `DISTANCE_STEPS` controls the transition bands:

Expansion and grouping share a camera-centered 2:1:1 ellipsoid. Its squared
distance is `sideways² + up² + (forward / 2)²`; the viewing axis rotates with the
camera. Straight-ahead cubes therefore retain detail and individual dots twice
as far away as along the lateral axes. `asset_brush::LOD_FORWARD_REACH` controls
this factor. Marker pixel size still uses actual projection depth, and the
frustum/far plane and seed limits are unchanged.

| Ellipsoid distance / world diagonal | Maximum dots represented by one marker |
|---|---:|
| Below 1/8 | 1 (individual dots) |
| 1/8–3/8 | 2 |
| 3/8–5/8 | 4 |
| 5/8–7/8 | 8 |
| 7/8 and beyond | 16 |

Four fixed radix passes order eligible dots by distance band and Morton spatial
key. Consecutive neighbors share their arithmetic mean position, RGB555 color
(rounded per channel) and physical size. The existing 1–9 pixel marker sizing
then applies to that representative. Average size is used, not accumulated area.
Groups never span a world-anchored tile larger than 1/32 of the longest side.
Incomplete groups and sparse tiles retain extra dots, so 16:1 is a populated-area
target rather than forced deletion of isolated features. Quantized bands and
group membership can produce visible transitions while moving the camera.

Buffers grow as needed and are reused on subsequent frames. Selection uses
squared distances, and the existing detail-size comparison also avoids square
roots. The CPU visibility pass and its input seed limit remain in place; saved
slots are not refilled with more distant cubes. This change reduces submitted
marker seeds and their 44-patch work, not the cost of initially culling the world.

Once-per-second `marker-lod` logs report the current frame's `dots_before`,
`dots_after` and `seeds_submitted`. Existing visibility counters still describe
the original source/admission pass.

Run `python3 tools/test_marker_lod.py` for semantic checks and a warm-buffer host
benchmark. An 8,192-dot fixture one world diagonal straight ahead reduces to
2,048 markers with the ellipsoid (the 4:1 band). The maximum grouping remains
16:1 at the ellipsoid's far band; along the viewing axis that requires twice
the physical distance. On the development host, marker preparation measured
about 170 microseconds versus 22 microseconds without grouping over 1,000 frames.
This adds roughly 0.15 ms of CPU work in that fixture; it is not a GPU/frame-rate
measurement. On-device frame time and visual transitions still need evaluation.
