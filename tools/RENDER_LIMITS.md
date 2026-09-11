# Key9 runtime cube sliders

Key9 presents a static front-facing camera and ordinary pointer. Click a strip
or drag its white marker cube. Both sliders snap to 16 positions, including the
endpoints. Labels and current values are drawn from cubes too.

- Upper, cyan: full-geometry cube cap, 768–3,840.
- Lower, amber: total retained foreground seeds per frame, 2,048–8,192.

Steps interpolate evenly with integer rounding. Both start at maximum and
persist across mode/world changes during this app session. Key5 uses the chosen
budgets immediately on entry; there is no save file or shader rebake.

The seed cap includes previews, placement ghost markers, the world companion,
its separate face seeds, and any required retained anchor. Those are reserved
before admitting world cubes. Full-cube allocation is also constrained by the
available seed slots. Platform hull/detail requests cannot bypass the geometry
cap; excess world cubes use the existing marker reduction. The frustum,
occlusion and distance rules remain active. Key1 expansion obeys the geometry
cap; Key7 limits visible geometry. Key4 keeps the closest geometry within both
caps and preserves its existing growth state for later restoration.

Both caps apply to the Cubes foreground retained scene. The circle background,
Key2 shader, collision, picking and authored asset data are unchanged. Full
geometry counts cube instances; the Rubik's separate face-only overlays also
consume retained seeds. The Key6 world-point experiment is removed and Key6
is unassigned. No server or GPU ABI changes are involved.

Checks:

```sh
python3 tools/test_render_limits.py
python3 tools/test_render_limits_submit.py
python3 tools/test_carousel.py
python3 tools/test_platform_lod.py
python3 tools/test_pointlist.py
cargo check --offline
```

The logs report each slider change and the world's active budgets. Native
pointer feel and visual appearance still require a device check.
