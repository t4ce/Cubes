# World asset picker

Middle-click in Key5 with the placement tool off opens this picker. Left-click
confirms the centred asset and returns to the same live world and camera.
Key4 is unassigned. The wheel slides through the current group's circular list.
A/D step backward/forward through assets;
W/S step to the next/previous group, wrapping once per press. Seven asset instances are kept live. The five horizontal slots keep:
centre opacity 1, immediate neighbours .5, outer neighbours .25. Short groups
repeat assets to fill all five horizontal slots. Two fixed previews sit above
and below the centre, each at 50% opacity: above shows the first asset of the W
(next) group, below the first asset of the S (previous) group, including wraparound
and the Key7 cube group. A/D and the wheel leave their selection, position and
reveal state unchanged. W/S refresh them for the newly selected group. Mouse movement orbits the centred selection
with pitch limited to avoid the poles. The camera radius is two-thirds of the
previous distance, with a minimum distance that fits the vertical previews, and adapts to viewport aspect ratio; the closer view can crop
the outer slots. Group changes preserve the orbit. Slides take 333 ms.

`Cube/asset-groups.json` contains only labels and asset filenames. Regenerate it
from the HTML optgroups with `node tools/export_asset_groups.cjs`, or check it
with `--check`. Only existing `.cubes` files are included; currently the bird
preview has no exported files, and the two extra portal assets form Other assets.
Build validation rejects duplicates, unknown filenames and ungrouped assets.
No generator camera or animation controller is imported into Cubes.

One additional runtime group, `Key7 cubes: 7 sizes x 6 materials`, follows the
exported groups. Its 42 entries come directly from `SubCubes::Demo`, each once
per loop. These single cubes retain their Key7 scale and palette material finish;
they do not use the asset fit-to-box normalization. W/S includes this
group; wheel and A/D browse its cubes with the same five-slot opacity and frame.

Each asset is centered in its slot and uniformly fitted to a 2.4-unit box.
Its authored cube proportions and RGB555 colours are preserved. New instances
share the placement reveal controller: 333 ms initial delay, 1,600 starts/s,
32 starts/frame, 700 ms Bounce + Uniform growth. Admission is interleaved across
all seven assets; moving existing instances keeps their reveal state.

The selected slot has a hollow cubic frame made only of c1 and c2 cubes. A
half-visible band travels around its XZ perimeter every four seconds, with shared
Bounce + Uniform growth over 320 ms at one end and linear shrink at the other. The frame is present immediately
and runs independently of wheel input, group changes and asset reveal budgets.
Its size and position stay fixed. The four retained assets
slide smoothly to their next positions; the outgoing edge instance is recycled
as the incoming one. Wheel input during a slide is queued (up to 32 steps).

Key9's session rendering limits also apply here. If the current scene exceeds
its full-cube or retained-seed budget, the closest cubes to the selection are
kept. Increasing the budget restores admitted geometry without restarting growth.
One retained slot is reserved for the required opaque anchor.

All carousel cubes use the existing back-to-front group-1 alpha pass, sorted
against the current orbit view direction. The opaque
group contains a clipped anchor required by the retained API. RGB555 colours from
all `.cubes` palettes are baked into a relocation-free lookup and validated by
`build.rs`; re-exporting new colours requires rebaking/exporting the cube shader.
The palette index uses bits 0–8, alpha class uses 10–11, and flags 24576|512 select
the carousel path. Existing material and world flags remain distinct. No input
layout, server API, or draw-pass contract changes are needed. The complete native
shader now needs a 40 KiB state slot including its descriptor page.
Within the carousel path, bit 12 selects the shared Key7 material instead of
an RGB555 palette entry; bits 0–2 then carry the material index.

The picker remembers the last group and asset, including changes made with the
world wheel. Middle-click with the tool active disables placement; the following
middle-click reopens the picker. Entering and leaving the picker preserves the
world, walker, placed cubes and placement reveal progress.

Checks:

```sh
node tools/export_asset_groups.cjs --check
python3 tools/test_carousel.py
python3 tools/test_asset_picker.py
python3 tools/test_bake_patch_cube.py
python3 tools/test_camera_sun.py
python3 tools/test_patch_overlay.py
python3 ../TRUEOS/tools/test_patch_cube_capacity.py
cargo check
```

Native appearance still needs a device check: every group, both wheel directions,
rapid scrolling, narrow/wide windows, translucent neighbours and the continuous half-frame animation.
Both the app and the newly baked TRUEOS shader bundle must be deployed together.
