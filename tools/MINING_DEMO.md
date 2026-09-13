# Cubes Key7 mining demo

Key7 has ten sizes in total: the existing eight plus C1/2 and R1/2.
Six palette columns contain nine size rows: R1/2=1/6, C1/2=1/2, c1=1,
c2=2, r1=3, c3=4, r2=6, r3=8, c4=16 c1. Each c1 is 0.2 renderer units.
A 2×2×2 group of C1/2 fills c1; a 3×3×3 group of R1/2 fills C1/2.
Coordinates use exact integer sixth-c1 ticks.

The nine display rows contain 54 cubes. Below them is the existing 6×2×1
floor of twelve 64×64×64 c1 cubes, centered at y=-32 c1 with its top at y=0.
Each depth row contains red, orange, yellow, green, blue, and violet once.
Palette RGB, roughness, and metallic values come from
`Cube/subcubes-materials.json`.

- Mouse: look; WASD: move; Shift: boost; Q/E: flight roll.
- Space: push away / approach a walkable face; Home: align the walk view.
- Wheel: cycle off → 64-to-16 → 16-to-4 → off (reverse wheel reverses).
  Entry and reset start off.
- Left click: commit the previewed removal.
- Right click: restore all 66 blocks and the starting view.

This first mining pass targets intact 64-c1 cubes and 16-c1 (c4) cubes.
A 64-c1 cube is divided into 4×4×4 children of side 16 c1. The ray selects
one child on the hit face, snapped relative to that parent. A 16-c1 cube is
removed whole, whether it is an existing display cube or a child exposed by
previous mining. Other sizes block selection through them but cannot be mined
in this mode.

The second tool targets c4 (16 c1), r3 (8 c1), and c3 (4 c1), in every color.
Its cut still snaps to the 4×4×4 grid of c3 inside c4, but storage and preview
split only the affected branch: seven intact r3 cubes plus seven c3 cubes
remain after one cut. An r3 is a 2×2×2 group of c3; cutting it leaves seven c3.
Existing c3 cubes, including mined fragments, are removed whole without further
subdivision. It uses the same centered pulsing opaque preview and
parent-relative snapping as the first tool, with a 4-c1 removal volume.

The preview renders the retained chunks as opaque cubes and marks the selected
child with a centered opaque cube pulsing sinusoidally between 75% and 95%
of the child's linear size over two seconds, in its original material.
The full child remains the removal volume (16 c1 for the
first tool, 4 c1 for the second).
It does not attach an extra cube outside the surface.
Aiming away or disabling the tool restores the intact display. Preview never
changes collision or stored blocks. Clicking removes exactly that previewed
child and retains the other chunks with their original material. Targeting an
existing child cube shows the same pulsing marker and removes that whole cube
without further subdivision. No intermediate
sizes are added. With no tool selected, the normal animated flycam marker
returns for Space-to-snap. Enabling the mining tool hides that flight marker.
On intact 64-c1 cubes, flycam targeting divides each face into 8×8 landing
areas, each 8 c1 wide. The half-size flight marker follows the selected area;
Space approaches its surface center. Mining still uses the separate 4×4×4
grid of 16-c1 removal cubes.

The support cubes and tiers at least c3 remain walkable; the smaller display
tiers are visible but non-solid. Mining stays local to Key7 and resets on
reentry. The shared world/VFX size grid is unchanged.

Host checks:

```sh
cargo check
python3 tools/test_world_permutation.py
python3 tools/test_walker_camera.py
```
