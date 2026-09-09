# Resident Chroma background

Cubes opens one UI4 layered window. Picasso renders the transparent foreground;
a native worker owns the independently published background. Keys 1–4 retain a
neutral slate shade. Key 5 uses a complete spherical environment, including the
lower hemisphere, with background-only opacity 128/255.

Every Key 5 world selection advances a generation, even when revisiting a
previously loaded world. The kernel bakes that generation once into a resident
six-face RGBA8 cubemap: 1024×1024 useful texels per face, with a one-texel border
on each edge, packed into a 3078×2052 atlas (about 24.2 MiB including row/page
alignment). Each face uses the reference's full quality: 120 march steps,
nine Folded Core recurrences or four Cathedral scales, and three AO samples.

The expensive bake runs on the background worker in bounded GPU batches. UI4
retains the previous published image until all six faces have retired. No
partial map is sampled. The next world reuses this allocation; there is no
27-world GPU cache and no CPU readback in the app's rendering path.

Camera motion, field-of-view changes, and resizing only sample the retained
map. They do not invalidate it. The view pass rotates a ray, chooses a face,
and performs bilinear lookup, including the border texels. The background has
a 60 Hz ceiling and submits only when its command changes. Once the camera's
rotation follower settles, the background stops submitting work.

The follower uses normalized quaternions, shortest-arc rotation error and a
damped angular velocity (10 rad/s natural frequency, damping ratio 0.74).
It trails the foreground camera, overshoots slightly, and settles to the same
orientation. Frame-time subdivision keeps it stable. Entering a world resets
both camera and follower to the same +Y-up heading; the first mouse event
uses that same yaw/pitch basis, avoiding the old +Z-to−Z half-turn.

## Geometry and color provenance

`Cube/the_one_cube_chroma.html` is the supplied MIT-licensed reference and is
preserved unchanged. `tools/export_chroma.py` derives the shader port directly
from its geometry, lighting and material mixing; the license travels with the
GLSL. Folded Core is exclusive to world 27, using authored Void `#D83CFF`.
The other 26 worlds use Box Cathedral with one, two or three theme colors.

`src/environment.rs` uses the exact seven sRGB values authored in
`Cube/cube_tree_builder_world_ramps.html` and exported in the world assets.
The reference HTML's sample UI palette is not substituted for the world colors.
Material-weighted color mixing, linear-light conversion, fog and tone mapping
happen during the bake. Runtime panning needs only the resulting color texture.

## Artifact and runtime contract

The existing authenticated ShaderToy program 16 now uses the reviewed
three-pointer source-atlas layout, also used by the focused ShaderToy path.
The custom sampler epilogue is `Cube/mandelbox/environment.clcpp`. The resource
belongs to the window's ShaderToy state and survives frame-ring replacement.
All reads/writes retire before normal release; existing quarantine behavior
retains storage on an unretired GPU submission. Native worker cancellation
continues to drain before the owning Frame closes.

The program-specific control block uses `frame` as world generation,
`time_seconds` as enabled, `mouse/click` as quaternion XYZW, `delta_seconds` as
tan(vertical FOV/2), `sample_rate` as color count, `date_year/month/day` as packed
24-bit sRGB colors, and `date_seconds` as preset (0 Folded Core, 1 Cathedral).
Only the generation, palette/count and preset form the cache key. This changes
program 16's authenticated payload, not the public UI4 frame ABI.

Run `python3 tools/bake_mandelbox.py` after reference or sampler edits. This
reproducibly compiles the native shader and updates its app-owned `.stpkg`,
source hash, kernel admission hash and generated ABI contract together.
It does not pack or publish the Cubes Blueprint. `--check` verifies these
artifacts without changing them.

## Validation for this change

- Cubes and TRUEOS compile checks.
- `python3 tools/test_background.py`: all 27 palettes against exported assets,
  quaternion settling/overshoot/wrap/frame-time behavior, initial mouse heading,
  and generated shader provenance.
- TRUEOS `python3 tools/test_shadertoy_dispatch.py`: actual cache/orchestration
  with mocked GPU submissions; camera/resize reuse, world replacement, failure
  before complete bake, batch bounds, and the generated payload pointer layout.
- TRUEOS `python3 tools/shadertoy-cpp-offline/test_blueprint_packages.py`: package
  authentication and hardware-contract admission.

Packing, deployment and runtime GPU/rig testing are left to the user for this
change. Both the kernel and Cubes need rebuilding together. In the rig test,
look for one `Chroma environment ready` record per Key 5 selection and none
from camera motion or maximize/restore. Check both hemispheres, single/dual/trio
worlds, Void, the first mouse movement, rotation settling, and stop while loading.

The optional host probe `tools/benchmark_mandelbox.c` has been updated and
compile-checked for the new ABI, but has not been executed for this change.
It reports bake time separately from cached lookup time and writes four views
for single/dual/trio Cathedral and Void. Build with
`cc -O2 -std=c11 tools/benchmark_mandelbox.c -o /tmp/benchmark_chroma -l:libOpenCL.so.1 -l:libX11.so.6 -lm`.
Create an output directory, then pass `kernel.spv output-directory width height`.
Use the pinned TRUEOS OpenCL ICD/toolchain environment when needed.
