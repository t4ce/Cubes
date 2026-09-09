# World walker camera

`src/CubesWalkerCam.rs` replaces the flat world-plane controller in World View.
Use **5 or F5** to enter/cycle worlds. Each page starts inside its north standing
portal, facing the world center. Void starts in its center portal, facing -Z.
World cycling has no source/arrival portal, so entry uses this deterministic rule.

- Mouse: look; WASD or arrows: walk; Shift: faster.
- F: toggle surface grip/free drift (once per press).
- During drift, Space rises and Q/left Ctrl descends along the current up axis.
- Aim at a cube: a white wireframe means F can grip it; black means out of reach.
  The first aimed surface wins, with a nearby-surface fallback when aiming empty.
- Home: align the view to the current surface. R retains the world-cube control.

The HTML Explore defaults are retained: 45-degree FOV, 0.003 rad/pixel look,
0.75-voxel eye height, 2.9/4.5 voxels per second walking, 1.8-voxel quarter-turn
travel, 100% camera assist, 45-degree soft perch, and exponential camera smoothing.
Stop to hold an edge, reverse to return, or move sideways along it. A one-cell
step is traversed automatically; larger walls become walkable faces. At a
three-face vertex, choose a face by moving across the edge.

The authored cell size is 0.2 renderer units in the current exports. Movement
and eye height scale with that cell size; mouse and transition timing do not.
Collision expands packed cubes into their ideal cell union. Tiny visual gaps
and bevels do not split the walking surface. The occupancy includes the full
page, independent of streamed/culled seeds, and retains the authored portal
footprint during portal recoloring/scatter. No world population is changed.

The black/white outline reuses the existing static line-list pass and outlines
the complete visible packed cube. F uses the same rendered-view probe as the
outline (12-voxel aimed reach; 6-voxel fallback). This deliberately prevents a
black aimed cube from silently snapping to a different, nearby cube.

## Validation

Run `python3 tools/test_walker_camera.py` and `cargo check` from Cubes.
The host tests execute the HTML's actual edge controller to produce comparison
traces, then check the Rust implementation, seam support, steps, inner corners,
reverse/perch, assistance, drift collision, F/outline agreement, and all 27
real portal entries. They do not establish GPU rendering or subjective feel.

Three useful on-device checks:

1. Enter World View: verify portal entry, inward heading, eye height and walking
   speed; cross several visible cube seams without camera bumps.
2. Approach a floor/ramp edge: keep walking around it, stop near 45 degrees,
   move along the edge, then reverse. Compare the motion with HTML Explore.
3. Press F, use Space/Q to drift, and aim at a cube from far away then nearby:
   black should become white. Press F to grip the white target; holding F must
   not repeatedly toggle. Home should level the view on the new face.
