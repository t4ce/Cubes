# Camera-relative sun angle

The existing single directional light is now expressed in the player's camera
frame: `0.35 * right + 0.80 * up + 0.45 * backward`, normalized by the same PS
operation as before. This points **toward** a sun above, behind, and slightly
right of the player, so it illuminates the scene from the viewing side.

The direction follows camera rotation, including pitch and roll. It is not a
stationary world-space sun. Camera translation, distance to a cube, and field of
view do not affect the direction. Camera axes avoid hard-coding either the
room's screen-down world Y or the world's +Y-up convention.

This applies to the shared cube shader in modes 1-5 and the Key-7 material
showcase. Ambient/sky, diffuse and specular equations, brightness coefficients,
roughness/metallic presets, alpha, geometry, and scene controls are unchanged.
This does not add lights, shadows, textures, a new pass, or a frame submission.

## Shader interface

The DS already reads the camera view matrix. It now extracts its world-space
right/up/back axes and supplies `surfaceLight` at location 3. The PS only changes
its light-direction input. World-space normals and view vectors stay in their
existing coordinate space; in particular, the sky term still uses world normal Y.

The exporter expects four PS varying inputs and six DS VUE slots rather than
three/five. The fourth varying still fits the existing two 32-byte SBE read units
and two 64-byte DS URB entry units. Existing binding maps, no-scratch/no-push
requirements, and URB-partition checks remain strict; the native rebake must
confirm these expectations. This adds a small DS calculation and a varying, not
an assertion of literally identical GPU cost.

The public retained input/layout-9 contract has not changed. DS and PS must be
rebaked/exported together; no generated machine code or successful native
validation is fabricated by this source change.

## Validation and deployment

Host tests, independent of the pinned compiler:

```sh
python3 tools/test_camera_sun.py -v
python3 tools/test_bake_patch_cube.py
python3 tools/test_patch_overlay.py
```

The focused sun tests check camera yaw/pitch/roll (including vertical views),
translation independence, DS/PS interface agreement, and the complete unchanged
pixel-lighting body after substituting the light-direction expression. They use
a tiny geometry fixture for source generation, not a GPU rendering simulation.

On the existing pinned compiler checkout, rebake for the actual target (`4680`
for the ADL-S lane, or `a780` for that admitted device):

```sh
python3 tools/bake_patch_cube.py --out target/patch-cube --device-id 4680
python3 tools/export_patch_driver.py target/patch-cube \
    ../TRUEOS/crates/trueos-shader/generated_patch_cube.rs
```

Rebuild/deploy TRUEOS with the newly exported bundle and build/package Cubes as
usual. Until that kernel bundle is updated, the installed shader keeps the old
world-space angle. Do not merely change export metadata around old binaries.

On the rig, orbit Key 7 through a full turn and check all six finishes. Check a
room mode and world walking/flight too: the source should stay above/behind the
viewpoint, including a rolled camera, with no brightness or material setting
changes. This PR's host tests do not establish bare-metal image correctness or
performance.
