# Key4 asset carousel

Key5 remains the world view. Key4 selects the next generator group; the wheel
slides through that group's circular list. Five asset instances are kept live:
centre opacity 1, immediate neighbours .85, outer neighbours .5. Short groups
repeat assets to fill all five slots. The fixed camera reserves room for the
incoming slot during a 333 ms slide and adapts to viewport aspect ratio.

`Cube/asset-groups.json` contains only labels and asset filenames. Regenerate it
from the HTML optgroups with `node tools/export_asset_groups.cjs`, or check it
with `--check`. Only existing `.cubes` files are included; currently the bird
preview has no exported files, and the two extra portal assets form Other assets.
Build validation rejects duplicates, unknown filenames and ungrouped assets.
No generator camera or animation controller is imported into Cubes.

Each asset is centered in its slot and uniformly fitted to a 2.4-unit box.
Its authored cube proportions and RGB555 colours are preserved. New instances
share the placement reveal controller: 333 ms initial delay, 1,600 starts/s,
32 starts/frame, 700 ms Bounce + Uniform growth. Admission is interleaved across
all five assets and the frame; moving existing instances keeps their reveal state.

The selected slot has a hollow cubic frame made only of c1 and c2 cubes. On a
selection change its visible cubes shrink linearly over 166 ms, then the new frame
rearms the reveal delay and grows linearly over 700 ms. The four retained assets
slide smoothly to their next positions; the outgoing edge instance is recycled
as the incoming one. Wheel input during a slide is queued (up to 32 steps).

All carousel cubes use the existing back-to-front group-1 alpha pass. The opaque
group contains a clipped anchor required by the retained API. RGB555 colours from
all `.cubes` palettes are baked into a relocation-free lookup and validated by
`build.rs`; re-exporting new colours requires rebaking/exporting the cube shader.
The palette index uses bits 0–8, alpha class uses 10–11, and flags 24576|512 select
the carousel path. Existing material and world flags remain distinct. No input
layout, server API, or draw-pass contract changes are needed. The complete native
shader now needs a 40 KiB state slot including its descriptor page.

Checks:

```sh
node tools/export_asset_groups.cjs --check
python3 tools/test_carousel.py
python3 tools/test_bake_patch_cube.py
python3 tools/test_camera_sun.py
python3 tools/test_patch_overlay.py
python3 ../TRUEOS/tools/test_patch_cube_capacity.py
cargo check
```

Native appearance still needs a device check: every group, both wheel directions,
rapid scrolling, narrow/wide windows, translucent neighbours and frame regrowth.
Both the app and the newly baked TRUEOS shader bundle must be deployed together.
