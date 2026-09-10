# Key5 asset placement

The enlarged top-left preview selects individual assets from the same 49-asset catalog
shown together in Key4. Scroll the mouse wheel to cycle; left-click to place
on the face of the cube outlined at the screen center. Middle-click toggles a
world-space placement preview made entirely of flat point-like markers. It follows
the aimed face and wheel selection, and disappears when no cube is targeted.
The preview adds no collision or stored cubes; left-click performs placement.
This works while walking
or flying. The existing top-right Rubik companion retains its R toggle.

Placement uses quarter-size cubes (0.05-unit steps) and authored colors, centers the asset's
base on the target face, and turns its up axis toward that face's normal.
Placed cubes join collision and subsequent outline/placement picking. Overlaps,
camera intersections, world-envelope overflow, and obstructing portal arrival
areas reject placement. Placements remain associated with their world for the
running session, including portal trips and Key5 cycling; they are not written
back to the authored `.cubes` files.

Placed cubes use Key4's reveal controller: 333 ms initial wait, at most 1,600
starts per second and 32 per frame. Adding another asset preserves existing
reveal state. Each world stores at most 16,384 added cubes.

`src/reveal.rs::PLACED_BOUNCE_UNIFORM_GROWTH` enables the Key5 growth experiment
(default `true`). After admission, solid cubes grow uniformly on all axes about
their centers over `GROWTH_MS = 700`: uniform acceleration/deceleration followed
by two diminishing bounce dips. The scale never exceeds the authored size.
Growing cubes do not provide occlusion coverage until settled. Collision geometry
is placed at full size immediately. LOD uses the authored size throughout;
distant dots retain their existing size/merging behaviour. Disable the flag to
restore full-size pop-in with the same delay and rate limits. Key4 keeps pop-in.

The 8,192-seed frame limit includes both previews. The asset inset uses at most
384 evenly sampled cubes for larger assets. The placement ghost reserves up to
768 evenly sampled marker seeds within that same total budget. Up to 3,840 nearest eligible world
cubes use full geometry; smaller-than-two-pixel and excess cubes use the existing
flat-marker shader path. Marker widths follow projected cube size, clamped to
1–9 pixels. Further seeds beyond the total frame budget are culled. Markers do
not occlude using the full cube footprint. This reuses the retained shader's
point-like flat markers; it does not introduce a hardware point-list pass.

Validation: `cargo check`, `tools/test_world_permutation.py`, and the walker
harness with its original camera-reference HTML. Check on device: cycle and place
on a floor and wall; revisit through a portal; back away from a populated area
to inspect the reveal and marker appearance.
