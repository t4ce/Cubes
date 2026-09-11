# Key5 asset placement

The corner asset inset is removed. With the placement tool off, middle-click
opens the [asset picker](CAROUSEL.md). W/S selects a group and the wheel or A/D
selects an asset. Left-click confirms the centred asset and returns to the same
world and camera; that confirmation click does not place anything.

While the tool is active, the world wheel wraps within the selected group and
left-click places on the aimed face. A world-space ghost follows that target.
Middle-click disables the tool and clears the ghost. The next middle-click
reopens the picker at the last group and asset. Key4 is unassigned. The separate
R-toggle Rubik companion is unchanged.

All exported assets retain their authored c1 sizes and colours. The existing
Key7 cube group can also be selected, retaining each cube's tier and material.
The base snaps to the aimed face and its up axis follows that face's normal.
Collision, portal clearance, world bounds and overlap checks still apply.
Placements remain session-local to their world across trips and Key5 cycling.

Placed cubes retain the shared 333 ms initial delay, 1,600 starts/s, 32 starts
per frame and 700 ms Bounce + Uniform growth. Opening/closing the picker does
not recreate the world or reset its reveal controller. Collision geometry uses
full authored size from placement onward.

Key9 limits still apply. The removed inset reserves no seeds. The placement
ghost uses at most 768 marker seeds; the world companion also reserves its own
cube/face seeds. Remaining world geometry uses the existing frustum, occlusion,
distance and marker-reduction rules.

Checks: `tools/test_asset_picker.py`, `tools/test_carousel.py`,
`tools/test_world_permutation.py`, and `cargo check --offline`.
Native pointer feel and placement appearance still need a device check.
