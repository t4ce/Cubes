# World walker camera

`src/CubesWalkerCam.rs` replaces the flat world-plane controller in World View.
Use **5 or F5** to enter/cycle worlds. Arrival is one authored voxel in front
of the portal's inner plane, standing on its 2×2 connector and facing the world
center. World cycling uses the north portal; Void uses its central portal and
faces -Z. If there is no nearby support, arrival stays in drift.

Key2 clicks use the selected sticker's equivalent Leave portal. Local sticker
axes are ranked X/Y/Z: pure worlds use top; duo worlds use bottom/top; trio worlds
use west/bottom/top. This mapping survives puzzle rotations.

Entering a world-linked portal fades out the world and reverses the cube-entry
flight into Key2. Exactly one quarter-turn moves both connected cubies together
while the puzzle expands. The camera then approaches the destination's matching
return face and fades into that world. Generic Leave portals finish in Key2
without a turn or re-entry. Mode hotkeys cancel automatic travel. Portal entry
is blocked for five seconds after arrival; Space push-off/approach has no cooldown.
The trigger tests a crossing of the opening, not contact with its decorative ring.

- Mouse: look; WASD or arrows: walk; either Shift: twice normal walking speed.
- Key4/Key5, attached to a surface: Tab toggles path preview. Aim at a cube
  to reuse the flight landing highlight and show a dashed surface route.
  Space locks a ready route and follows it at 58 voxels/s (twice Shift speed).
  The view follows travel; WASD/arrows cancel it, and Tab cancels and exits
  path mode. Opening the picker or changing modes also cancels travel.
  Tab does nothing in flight, Key7 or the network image wall.
- Flight speed is twice the original; Shift retains the 2.8× flight boost.
- Space held at an outside edge, or pressed during its turn: push off 18 voxels
  at twice current walking speed while turning the view back toward the edge.
- In drift, press Space to approach the aimed cube at 48 voxels/s and attach.
  There is no range limit or cooldown; another press can retarget even during
  push-off. Holding Space through launch does not immediately trigger a return.
- Q/E while walking: visual lean up to ±13.5° (15% of a quarter-turn), returning
  quickly on release without affecting movement. In flight: roll at 90°/s.
  Flight mouse-look and A/D strafing follow the rolled screen axes, with unrestricted local turns.
  Home resets roll.
- F and Ctrl have no camera binding.
- In flight, aim at a cube: a half-size translucent cube on the aimed face marks
  the Space destination. Ordinary walking has no landing indicator; Tab path
  mode reuses it. Placement previews and left-click placement pause during
  path mode and resume when it closes; see `ASSET_PLACEMENT.md`.
- Home: align the view to the current surface. R retains the world-cube control.

Camera behavior is owned by Cubes; world imports contain only `.cubes` geometry
and material records. Settings: 41.625-degree FOV (7.5% narrower), 0.003 rad/pixel look,
0.825-voxel eye height (10% higher), 14.5/29 voxels per second walking (normal/Shift), 1.8-voxel quarter-turn
travel, 100% camera assist, 45-degree soft perch, and exponential camera smoothing.
Stop to hold an edge, reverse to return, or move sideways along it. A one-cell
step is traversed automatically; larger walls become walkable faces. At a
three-face vertex, choose a face by moving across the edge.

The authored c1 cell size is 0.2 renderer units in v2 exports. The camera's movement
unit is four c1 (0.8 renderer units), with collision sampled on the c1 grid.
Only c3, r2, c4 and r3 affect walking, drift collision and Space attachment;
c1, c2 and r1 are pass-through geometry. Tiny visual gaps
and bevels do not split the walking surface. The occupancy includes the full
page, independent of streamed/culled seeds, and retains the authored portal
footprint during portal recoloring/scatter. No world population is changed.

The flight indicator is one actual beveled cube at half the visible constituent
cube's side length and 35% opacity. Its color uses the target's shared material;
RGB555 world shades and placed assets map to the nearest of the six theme colors.
It grows out of the aimed face over 700 ms with a damped physical spring (four
oscillations, decay five), then holds until the target changes, disappears, or the
camera lands. It never creates collision or persistent geometry. Key7 uses it only
for Space flight targeting, without mining guides.

The indicator uses the existing transparent retained cube group with depth testing
and no depth writes. The group retains an invisible slot after landing, keeping
the draw contract stable. With the world companion visible it joins the sorted
transparent seeds. No post-render UI4 outline overlay remains.

Space uses the rendered view's first surface hit, matching the center of the
screen during smoothing. Approaches stop safely if another surface obstructs
travel; no instant long-distance snap is used.

`src/cubepathfind.rs` provides reusable incremental A* on oriented voxel faces,
with four cardinal outgoing directions per face. The camera probes each edge
through its existing walker, so occupancy, step heights, headroom, inside/outside
corners and directed drops match manual movement. Costs include the walker's
step/turn travel, at millivoxel precision. The shortest grid route connects face
centers; a short initial segment joins the current foot position to that grid.
The destination is the aimed cell on the selected cube's face.

Search runs up to 128 queue visits per frame with a 65,536-node memory limit.
Unreachable or limited searches show no route and Space does not launch travel.
The `surface-path` log reports searching, ready, travelling, unreachable or
limited states. Retargeting, geometry edits and mode changes discard stale paths.
Standing partway through a step/turn requires finishing it before previewing.

The preview samples the actual walking trajectory, then places at most 512
separated beveled cubes along its full length. Their material and opacity match
the destination indicator. They stay above the marker-size encoding and reserve
full geometry before world LOD admission; long routes widen their dash spacing.
They share depth-sorted transparency with the target and world companion, and
never enter occupancy or saved placements.

The near plane is 0.01 renderer units. Key5's far plane covers the diagonal of
the camera's drift envelope, including a small margin (about 799 units for the
current 409.6-unit-wide worlds). The former fixed 100-unit plane clipped before
the world center when viewed from a boundary entrance. CPU visibility and GPU
projection use the same range. Key7 also derives its range from its camera bounds.

Frustum clipping precedes detail/marker selection. Of the admitted visible seeds,
the first 3,840 are detailed if their projected size is at least two pixels;
others use the existing marker shader, within the total seed budget. Increasing
the view range makes distant geometry eligible for this selection; it does not
force all distant cubes to become dots. World selection logs the projection range.

Unexpanded world dots then use distance-based spatial averaging, reaching groups
of up to 16 in the far distance band. Close dots remain individual.
Expansion and grouping measure distance in a camera-aligned ellipsoid with
twice the reach along the viewing axis as along either lateral axis.
See `MARKER_LOD.md` for the thresholds, sparse-area preservation and CPU benchmark.

## Validation

Run `python3 tools/test_walker_camera.py` and `cargo check` from Cubes.
The host tests exercise the Rust implementation, seam support, steps, inner corners,
reverse/perch, assistance, drift collision, Space/indicator agreement, and all 27
real portal entries. They also check all 27 worlds against the far-plane bounds
and verify distant cubes reach detail/marker selection. They do not establish
GPU rendering or subjective feel.

Path regressions cover shortest weighted search, all six faces, step/inside-corner
travel, fine-grid extensions, long diagonal routes, disconnected solids,
retargeting, edit invalidation, entry connectors and Tab/Space/cancel behavior.
`python3 tools/test_plateau.py` also exercises preview and travel on the actual
generated terrace; `python3 tools/test_carousel_submit.py` checks retained seeds.

Three useful on-device checks:

1. Enter World View: verify portal entry, inward heading, eye height and walking
   speed; cross several visible cube seams without camera bumps.
2. Approach a floor/ramp edge: keep walking around it, stop near 45 degrees,
   move along the edge, then reverse.
3. Hold Space while walking off an outside edge, then try pressing it midway
   through a turn. Check the short push and backward look. Aim at a distant cube
   and press Space again: verify fast travel and attachment without passing
   through terrain. Repeat immediately to check that there is no cooldown.

Walking uses 5× the original 2.9-voxel/s base speed, or 10× while Shift is
held. Flight uses 24 voxels/s (originally 12). Collision substeps remain at
most 0.055 voxel, including fast walking, so speed does not skip seams/edges.

Single-cell rises and drops now spend walking distance on the vertical travel
instead of instantly changing height. The contact/view frame stays upright;
continuing crosses the step, releasing movement holds position, and reversing
retraces it. Normal/Shift speed applies to this travel as well. The path stays
outside the riser, so it cannot take a diagonal shortcut through the solid.
