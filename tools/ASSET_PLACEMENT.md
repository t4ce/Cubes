# Key5 asset placement

The top-left preview selects individual assets from the same 49-asset catalog
shown together in Key4. Scroll the mouse wheel to cycle; left-click to place
on the face of the cube outlined at the screen center. This works while walking
or flying. The existing top-right Rubik companion retains its R toggle.

Placement preserves native 0.2-unit grid sizes and colors, centers the asset's
base on the target face, and turns its up axis toward that face's normal.
Placed cubes join collision and subsequent outline/placement picking. Overlaps,
camera intersections, world-envelope overflow, and obstructing portal arrival
areas reject placement. Placements remain associated with their world for the
running session, including portal trips and Key5 cycling; they are not written
back to the authored `.cubes` files.

Placed cubes use Key4's reveal controller: 120 ms initial wait, at most 1,600
starts per second and 96 per frame. Adding another asset preserves existing
reveal state. Each world stores at most 16,384 added cubes.

The 2,048-seed frame limit includes both previews. The asset inset uses at most
384 evenly sampled cubes for larger assets. Up to 768 nearest eligible world
cubes use full geometry; smaller-than-two-pixel and excess cubes use the existing
flat-marker shader path. Marker widths follow projected cube size, clamped to
1–9 pixels. Further seeds beyond the total frame budget are culled. Markers do
not occlude using the full cube footprint. This reuses the retained shader's
point-like flat markers; it does not introduce a hardware point-list pass.

Validation: `cargo check`, `tools/test_world_permutation.py`, and the walker
harness with its original camera-reference HTML. Check on device: cycle and place
on a floor and wall; revisit through a portal; back away from a populated area
to inspect the reveal and marker appearance.
