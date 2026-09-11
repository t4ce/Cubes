# Native point background and Key6 world experiment

Cubes links PotatoStamps' library geometry. It does not launch PotatoStamps,
open another window or add another VM. The existing background worker displays
Key1's four 64-point circles continuously, with no timed swap. Positions,
colors and circle point widths come from PotatoStamps' scene
module. Key2 retains its current Palette Grid compute shader. The background
uses the existing XYZ immediate-color `IndexedDrawBatchV2` POINT_LIST renderer.

Every successful resize increments a producer generation, even for A -> B -> A.
That forces a background publication immediately even though the circle
scene is otherwise idle. An empty world-point scene submits a transparent,
clipped point so its clear/publication still completes UI4's two-layer resize
barrier. Transient begin/import/submit contention is retried with the appropriate
lease ownership; tests run the actual worker against a lease/resize mock.

Key6 cycles the same 27-world sequence as Key5. Camera, portals, placement,
mining-size collision rules, and world content are shared. Key5 retains its
existing hull-generated marker path as the comparison mode. Key6 admits up to
32,768 visible candidates, retains distance averaging, and removes every marker
from the HS/DS seed list. Up to 7,680 eligible near cubes can become solid,
subject to the 8,192-seed limit after reserving previews, ghosts and companion.
The 2:1:1 camera-facing ellipsoid and pixel-size eligibility rule are unchanged.

Key6 projects marker centres on the CPU and submits real two-pixel native points
in the rear layer. Behind-eye and out-of-viewport points are discarded. The rear
layer does not share a depth buffer with the foreground: its points are always
behind foreground cubes. It is a far-field experiment, not a general mixed-depth
renderer. The worker receives a coherent latest snapshot; it does not queue old
camera frames. One background frame can lag the foreground during camera motion.

Colors remain exact unless a frame exceeds the native 600-draw limit, in which
case a bounded 8x8x8 RGB grid is used. Each color draw uses local sequential
indices plus `base_vertex`, avoiding repeated uploads of earlier color groups.
The background layer is at full opacity in Key6; decorative PotatoStamps and
Key2 backgrounds remain at 50%. Logs distinguish HS seeds, native marker count,
near detail budget, background pattern and resize generation.

Validation:

```sh
python3 tools/test_pointlist.py
python3 tools/test_marker_lod.py
python3 tools/test_world_permutation.py
python3 tools/test_patch_overlay.py
cargo check
```

These are host tests and build checks. Compare Key5/Key6 GPU timings and visual
appearance on the device before claiming a frame-rate improvement. No kernel,
server, shader ABI or native shader rebake is required for this change.
