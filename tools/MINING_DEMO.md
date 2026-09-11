# Cubes Key7 mining demo

Key7 shows six material columns and seven size rows, from c1 at the front to r3
at the back: c1=1, c2=2, r1=3, c3=4, r2=6, c4=8, r3=12 c1. Each c1 is 0.2
renderer units. All 42 cubes start on the same base plane.

Columns use the imported `Cube/subcubes-materials.json` baseline: red, orange,
yellow, green, blue, violet, with each record's RGB, roughness and metallic values.
Every size and mined fragment retains its material ID. All Key7 surfaces are
opaque (alpha 1). See `README.md` for rebaking after a palette re-export.

- Mouse: look; WASD: move; Shift: boost; Q/E: flight roll.
- Space: push away / approach a walkable face; Home: align the walk view.
- Wheel: cycle none → c1 → c2 → c3 → c4 → none (reverse wheel reverses).
  Entry and reset start with no tool; left click then does nothing.
- Left click: remove the outlined volume, extending inward from the aimed face.
- Right click: restore all 42 blocks and the starting view.

With a tool enabled, a white swatch shows its size even when aiming into empty
space. Over a target face, a 75%-opacity wireframe shows the exact cut volume,
and a c1 grid extends two cells beyond its footprint in each tangent direction.
The grid and cut share the mining ray hit on all six faces and move in c1
increments for every tool. The Space approach target retains its own outline,
including when no mining tool is selected. Guides are clipped to the viewport
before drawing over the scene, so geometry cannot bury them. Both Key7 guides
and the Key5 Space outline use `src/interaction_overlay.rs`: three-pixel white
UI4 strokes with a one-pixel dark border, composited after the retained render
retires and before publication. They no longer depend on the static LINE_LIST
pass. Long strokes are segmented with a total budget of 256 quads (one hardware
worklist) to bound the overlay's dispatch rectangles and submission count;
empty guides skip the overlay entirely. `Cubes: guides` logs report tool, landing
target presence and submitted quad count once per reporting interval.
Mining and the regional grid stay in Key7. Mine any of the six faces,
including edges, corners, existing cavities, and fragments. Each cut removes only
its intersection with existing geometry. `src/SubCubes.rs` packs each affected
block's remaining c1 cells in descending tier order, preserving its material.
This is deterministic greedy packing, not a global minimum-piece solver. Separate
blocks are not merged. Unaffected blocks remain intact.

Only c3, r2, c4, and r3 affect walking, flight collision, or Space approach. c1,
c2, and r1 remain visible and mineable but the camera passes through them. Key5
uses the same size policy; compacted terrain retains its constituent c4 identity.
Placement uses the assets' c1 scale. Mining stays local to Key7 and resets upon
reentry. Camera and behavior belong to Cubes, not the exported geometry files.

Host checks:

```sh
cargo check
python3 tools/test_world_permutation.py
python3 tools/test_walker_camera.py
rustc --edition=2024 --test tools/test_world_cycle.rs -o /tmp/cubes-world-cycle
/tmp/cubes-world-cycle
```
