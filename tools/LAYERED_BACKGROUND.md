# Resident Chroma background

Cubes opens one UI4 layered window. Picasso renders the transparent foreground;
a native worker owns the independently published background. Keys 1–4 retain a
neutral slate shade. Key 5 uses a complete spherical environment, including the
lower hemisphere, with opaque visible world patches and transparent gaps. Neutral modes retain
background-only opacity 128/255.

Each Key 5 selection also sets the primary display's opaque hardware bottom
color once through `Frame::set_display_bottom_color`. Pure worlds use their
authored theme RGB; dual/trio worlds use the arithmetic mean of the two/three
authored sRGB byte values, rounded to nearest; Void uses `#D83CFF`. This adds
no texture, plane, or per-frame rendering work. The existing transparent plane
stack exposes that color wherever no intervening opaque content covers it.
It is shared Pipe A display state, like UI4's color picker: last writer wins,
and leaving/closing Cubes does not restore a previous color. An unavailable
register/backend is logged without aborting the world.
The new Blueprint export requires an updated TRUEOS kernel as well as Cubes.

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

Each world captures its entry camera orientation once. The background view
is rendered and published once after baking, then UI4 retains that completed
image. Walking, mouse look and frame time do not update the background command
or enqueue any background GPU work. A world/mode change or resize/projection
change requests a new image; resize/projection changes reuse the existing bake.
The foreground camera continues to move independently.

The view pass preserves the foreground lens at entry, rotates its ray by the
captured orientation, chooses a cube face, masks its face centers/corners and
performs bilinear lookup including border texels. The worker retains its
existing command deduplication and scheduling, but the rotation follower is
no longer used. This makes the low-work frozen state the normal steady state.
The previous published image remains displayed while the next world bakes.

## Geometry and color provenance

`Cube/Mandelbox.html` is the supplied MIT-licensed reference and is
preserved unchanged. `tools/export_chroma.py` derives the shader port directly
from its geometry, lighting and material mixing; the license travels with the
GLSL. Folded Core is exclusive to world 27, using authored Void `#D83CFF`.
The other 26 worlds use Box Cathedral with one, two or three theme colors.
`Cube/mandelbox/theme_shape.glsl` also assigns each theme a fixed fold direction.
The active 1–3 directions are averaged once per baked pixel, then rotate each
recursive Cathedral level by an increasing amount before spatial repetition.
This changes apertures, intersections, surface normals and AO, rather than
only recoloring a shared shape. All 26 combinations have distinct directions.
The unit-quaternion rotation preserves each level's distance bound. The
reference's Folded Core function remains unchanged for Void.

These shape parameters derive solely from the existing palette/count/preset
cache key. Camera rotation, masking and resizing add no geometry generation.
Foreground camera projection, world geometry, palette colors and follower
dynamics are unchanged.

`src/environment.rs` uses the exact seven sRGB values authored in
the exported world assets.
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

Packing, deployment and runtime rig testing are performed by the user. Both
the kernel and Cubes need rebuilding together for the resident environment ABI. In the rig test,
look for one `Chroma environment ready` record per Key 5 selection and none
from camera motion or maximize/restore. Check both hemispheres, single/dual/trio
worlds, Void, the first mouse movement, rotation settling, and stop while loading.

The host probe `tools/benchmark_mandelbox.c` was executed on the local UHD 770.
The three Cathedral palettes took 198–217 ms to bake; Folded Core took 1377 ms.
Cached 640×360 lookup took approximately 0.18 ms. These are host OpenCL timings,
not TRUEOS end-to-end frame timings. Readbacks of all four directions were
produced, and Cathedral/Void front views were inspected without the rig's stripes.
It reports bake time separately from cached lookup time and writes four views
for single/dual/trio Cathedral and Void. Build with
`cc -O2 -std=c11 tools/benchmark_mandelbox.c -o /tmp/benchmark_chroma -l:libOpenCL.so.1 -l:libX11.so.6 -lm`.
Create an output directory, then pass `kernel.spv output-directory width height`.
Use the pinned TRUEOS OpenCL ICD/toolchain environment when needed.

## Stripe regression (2026-09-09)

The 3078-pixel atlas exposed an execution-mask error in TRUEOS's ShaderToy
walker. The global width remainder (six) was used as RightExecutionMask, leaving
ten columns unwritten in every SIMD16 workgroup. The mask describes local lanes
in each group; this is also how [Intel's Gen12LP runtime programs it](https://github.com/intel/compute-runtime/blob/master/opencl/source/gen12lp/gpgpu_walker_gen12lp.cpp#L105-L120).
The fixed dispatcher launches every lane and retains the shader's existing
width/height bounds guard. Odd-sized windows benefit from the same correction.

`tools/test_shadertoy_dispatch.py` in TRUEOS now replays the emitted walker
against sentinel-padded rows, including the guttered atlas and odd window widths.
It failed on the previous mask and passes with complete coverage and untouched
padding. All 13 dispatch tests pass. The user subsequently confirmed on the rig
that the shader is clean, accurately projected, and performant.

This correction is in the TRUEOS kernel. It does not change the shader package,
face resolution, or procedural detail. Keep the current quality for this pass:
filling the missing lanes already changes actual bake work.

## Theme fold validation (2026-09-09)

Rebuilt the authenticated native shader reproducibly (BC/SPIR-V/Zebin identical
across both compiler runs). Updated the app package and both generated TRUEOS
admission/ABI artifacts together. Both Cubes and TRUEOS must be rebuilt to use
this shader payload; no deployment or rig execution was performed.

`test_background.py` checks all authored theme identifiers against the shape
table and confirms 26 distinct, nonzero combination signatures. Background
palette/follower checks, Cubes `cargo check`, TRUEOS dispatch tests and Blueprint
package authentication tests pass.

Local UHD 770 readbacks at 640×360 show different recursive structures for
single/dual/trio Cathedral worlds. Cached views remain around 0.18 ms. The
one-time Cathedral bake measured 281–313 ms versus 198–228 ms before this
change; Void remains about 1.38 s. These are local host timings, not rig timings.
Before/after views and a comparison sheet are in `target/chroma-variation/`.

## Cube-face patch mask (replaces the zoomed crop)

The cached 360-degree cubemap is now sampled only inside six face-center
squares and eight corner patches. On each face, UV spans [-1,1]: the center
uses `abs(u),abs(v) <= 0.4/sqrt(2)`; a corner uses both `>= 0.7`. Each cube corner
joins three adjacent face sections. This retains 8% center area + 9% corner
area = 17% of the total cube surface, with transparent gaps over the other
83%. These are environment-space patches; they remain attached to the six
cube axes and eight corner directions as the camera turns. No screen-space
magnification remains. Existing damped rotation following is retained.

Masked pixels bypass the ShaderToy packer's forced opaque alpha and write
RGBA zero, so lower layers/display color remain visible through the gaps.
Visible world patches use background layer opacity 255/255; neutral modes
retain 128/255. Opacity changes only on mode changes, not camera updates.
All six source faces are still baked once per Key5 selection; masking and
sampling happen only in the cheap view pass. Theme-specific folds are retained.

`benchmark_mandelbox.c` now also writes raw `.rgba` readbacks and includes
edge/corner orientations. `python3 tools/test_chroma_patches.py
 target/chroma-patches` verified all 24 GPU views against an independent
cube-projection mask, checked palette-independent coverage and transparent
black pixels. Preview: `target/chroma-patches/patch-preview.png` (checkerboard
indicates transparency, not an image drawn by the app). On-device composition
has not been tested. Rebuild both TRUEOS and Cubes for the new admission hash.

## Smaller centers and cheaper gap pixels (2026-09-10)

Each face-center rectangle has half its previous area: both dimensions scale
by 1/sqrt(2). The eight joined corner patches are unchanged. The view shader
now tests the rotated ray against the mask before choosing a cube face or
performing UV division. Rejected pixels write transparent zero and return,
skipping atlas lookup, interpolation and color packing. Every output pixel
still needs a write to clear old patches during rotation; buffer allocation,
full-frame dispatch, follower timing and presentation cadence are unchanged.
The bake still occurs once per selection, and settled cameras submit no work.

Host probe data for before/after at 1280×720 is in `target/chroma-cost/`.

Across the 24 local UHD 770 cached views, median sample time decreased from
0.484 ms to 0.381 ms (about 21%). This includes both smaller center coverage
and earlier rejection, not an isolated attribution to either change. It is
GPU view-pass timing, not end-to-end compositor/rig timing. The front-center
pixel coverage ratio was 0.4996 (raster rounding); retained visible RGB pixels
matched exactly in front/corner comparisons. All 24 new masks match the
independent projection test. Source/package validation, dispatch/admission
tests and Cubes `cargo check` pass. No rig deployment was performed.

## Static background by default (2026-09-10)

`Background::select_world` captures orientation; `Background::update` no longer
accepts camera rotation or elapsed time. Consequently foreground motion cannot
invalidate the retained background. There is no per-frame background sampling,
upload or publication after it completes. UI4 display/composition and the
worker's existing idle polling still occur; this does not mean all graphics
work stops. World switching, masked alpha, full world opacity and foreground
motion are retained. No shader or kernel change is needed for this behavior;
rebuild Cubes (plus any still-pending shader/kernel changes from earlier work).
