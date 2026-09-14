# Cubes Key7 mining demo

Key7 has eleven sizes in total, including the large base cubes.
Six palette columns contain ten size rows: C1/4=1/4, R1/2=1/3, C1/2=1/2, c1=1,
c2=2, r1=3, c3=4, r2=6, r3=8, c4=16 c1. Each c1 is 0.2 renderer units.
A 2×2×2 group of C1/2, 3×3×3 group of R1/2, or 4×4×4 group of C1/4 fills c1.
R1/2 retains its name but is redesigned as one third of c1.
Coordinates use exact integer 1/24-c1 ticks; network wire coordinates remain sixths.

The ten display rows contain 60 cubes. Below them is the existing 6×2×1
floor of twelve 64×64×64 c1 cubes, centered at y=-32 c1 with its top at y=0.
Each depth row contains red, orange, yellow, green, blue, and violet once.
Palette RGB, roughness, and metallic values come from
`Cube/subcubes-materials.json`.

- Mouse: look; WASD: move; Shift: boost; Q/E: flight roll.
- Space: push away / approach a walkable face; Home: align the walk view.
- Wheel: cycle off → 1 → 2 → 3 → 3.5 → 4 → 6 → off (reverse wheel reverses).
  Entry and reset start off.
- Left click: commit on release, only if the same removal cube stayed targeted
  throughout the press. Aiming away cancels the click.
- Hold left for two seconds on the target: start auto-mining the currently
  aimed cube every 100 ms. Release stops it without an extra cut. Slow frames
  do not trigger catch-up bursts. Wheel changes, reset, leaving Key7, and lost
  input focus cancel the gesture.
- Right click: restore all 72 blocks and the starting view.
  This also clears in-flight collection pieces and the collected total.

The top-left readout uses Key9's cube-pixel number font: `tool ID : spawned`.
Tool 5 is removed; other IDs remain unchanged, including 3.5 and 6. Off is `0 : 0`. The count is the
number of replacement cubes the current cut would create, excluding the
removed cube, preview marker, and untouched scene blocks. For example,
cutting intact c1 shows `4 : 7`; collecting C1/2 shows `6 : 63`.
Removing a whole cube or aiming at no eligible target shows a zero count.
The readout updates before clicking and stays fixed at the top left.
It uses camera-relative white cube pixels, following Key5's R-toggle companion
placement approach, with a 32-pixel inset and 20-pixel font pitch (four times
the original size, reduced only to fit narrow windows). The bottom-right
collection counter keeps its original size. Its seeds are
reserved before scene admission; it does not use the static floor-line path.

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

The third tool targets c3 (4 c1), c2 (2 c1), and c1, in every palette color.
It keeps the same 4×4×4 c1 snap grid inside c3, but splits only the affected
branch: one cut leaves seven intact c2 chunks and seven c1 cubes. Targeting
c2 splits it into 2×2×2 c1, removing one and retaining seven. Existing c1 cubes
are removed whole. Larger cubes and other sizes cannot be targeted by this tool.

Tool 3.5 targets only r1 (3 c1) and r2 (6 c1). On r1 it removes a c2-sized
corner (2×2×2 c1) and leaves 19 separate c1 cubes. The cut snaps to one of
eight corners, with offsets 0 or 1 c1 on each axis. On r2 the r1-sized cut
snaps in c1 steps: offsets 0, 1, 2, or 3 c1 on each axis, constrained to fit
inside the parent. Untouched r1 chunks stay intact; partially intersected r1
chunks split into c1 cubes, with all cells inside the cut omitted. Preview
and commit use the same decomposition. Face cuts leave 7, 33, or 85 cubes
depending on how many r1 chunks they cross, and the readout reflects that.
The r1 readout remains `3.5 : 19`. It works on all palette colors and generated r1
fragments; its pieces stay in the scene, with no Rubik collection.

Tool 4 targets c2, c1, and C1/2. On c2 it preserves seven intact c1 cubes;
only the targeted c1 splits into halves, with one removed and seven retained.
The c2 readout is therefore `4 : 14` (seven c1 plus seven C1/2).
On c1 it splits 2×2×2, removes one half, and leaves seven C1/2 cubes.
Existing C1/2 cubes can be removed whole. Cubes larger than c2 are ineligible.
Tool 6 targets c1 and C1/2. On c1, seven intact C1/2 chunks remain in the scene;
only the targeted half splits 4×4×4 into 1/8-c1 pieces, with one removed and
63 collected. Preview shows all 63 flying candidates plus the seven intact
halves (`6 : 70`). On C1/2, it removes one tiny piece and collects the other
63 (`6 : 63`). Collection animation expands around the targeted half's center,
and the arrival counter increases by 63 in either case. The smaller pieces are transient;
the existing display rows (including R1/2=1/3 and C1/4=1/4) keep their sizes.
R1/2 and C1/4 cannot currently be mined by any tool. All colors are supported.

On committing tool 6, those 63 surviving pieces are collected
instead of remaining in the scene. First they expand outward around the source
cube's center for 500 ms, using a damped overshoot-and-settle curve inspired by
the reference's bottom-center physical curve, settling at 3× their original
spacing. Then they fly toward the center of the camera-relative Rubik companion
in the top right along slightly different curved paths, preserving their palette material and growing smoothly to
one-quarter of the former c1-sized peak by 40% of the eased flight. The initial
expansion and original fragment size are unchanged. Over the remaining flight, they fade
smoothly to zero alpha while shrinking with a Physical: 4, 5-inspired damped
spring. Undershoot becomes small size rebounds; scales stay positive and above
the renderer's tiny-marker threshold until disappearance. Collection pieces use the sorted
transparent pass alongside the companion faces; expansion stays opaque.
Key7 always shows this
companion, using the Key5 R-companion geometry and placement. It opens over
120 ms, stays expanded during collection, then closes over 180 ms. Key5's
existing R toggle and animation are unchanged.
Each flight has a stable software-RNG bend derived from its source position and
start time. Curvature stays within 8% of the trip distance (capped at 0.3 renderer
units), joins both endpoints exactly, and never re-rolls between frames.

After expansion, flight lasts 700–886 ms, so pieces arrive 1.2–1.386 seconds
after the cut and disappear individually; only then does the
bottom-right numeric total increase. The initially cut-away piece does not
count, so a completed tool-6 collection adds 63. Flying
pieces are visual only, with no collision or targeting. Preview alone never
starts collection or increments the total. Tools 1–4 still leave their chunks.
Both click-release and auto-mining use this collection path. At most 378 pieces
can be in flight; a cut exceeding that budget is deferred without altering its
parent. Auto-mining can retry on its next tick. Re-entering Key7 resets the total.

The preview renders the retained chunks as opaque cubes and marks the selected
child with a centered opaque cube pulsing sinusoidally between 75% and 95%
of the child's linear size over two seconds, in its original material.
The full child remains the removal volume (16 c1 for the
first tool, 4 c1 for the second, 1 c1 for the third, 1/2 for tool 4, and 1/8 c1 for tool 6).
It does not attach an extra cube outside the surface.
Aiming away or disabling the tool restores the intact display. Preview never
changes collision or stored blocks. Clicking removes exactly that previewed
child and retains the other chunks with their original material. Targeting an
eligible existing child cube shows the same pulsing marker and removes that whole cube
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
