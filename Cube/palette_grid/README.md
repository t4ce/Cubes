# Native Palette Grid Glow background

Key2 now uses authenticated ShaderToy program 4 directly at the window's native
resolution. There is no cloud frame cache, replay loop, sampling cap, or upscale
pass. The independent background worker publishes the latest procedural time;
it does not queue old camera frames. Key5's Mandelbox environment is unchanged.

The Palette Grid source keeps its palette, grid modulation, glow and animation.
Only the cell-radius metric changes: `max(abs(x), abs(y), (abs(x)+abs(y))/1.8)`
gives square contours with straight 20% corner bevels, matching the cube face.
This updates the shared program 4 package, including the ShaderToy gallery's
Palette Grid example. Canonical source and baked artifacts remain in
`TRUEOS-Blueprints/apps/shadertoy/assets/palette_grid`; Cubes embeds a copy of
that package and keeps the GLSL alongside it for inspection.

The shader was rebuilt with the locked ADL-S relaxed toolchain and passed the
bakery's reproducibility check. Run `python3 tools/test_palette_background.py`
from Cubes and `python3 tools/shadertoy-cpp-offline/package_blueprint.py --check`
from TRUEOS to verify the package chain. Rebuild TRUEOS and Cubes; rebuild the
ShaderToy Blueprint too if using its gallery. On-device timing is not yet measured
for the new beveled variant.
