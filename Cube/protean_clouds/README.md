# Key2 Protean Clouds background

Protean Clouds by nimitz / @stormoid:
https://www.shadertoy.com/view/3l23Rh

`protean_clouds.stpkg` is an unchanged copy of TRUEOS-Blueprints' authenticated
ShaderToy program 6 package. It includes its source and provenance alongside the
executable. The original source and existing lighting optimization are preserved.

Cubes requests the optional program-6 cache flag (4) only in Key2. The background
worker presents at up to 24 fps independently of camera/input updates. Key5 keeps
its Mandelbox environment. Other modes keep the neutral shade.

TRUEOS lazily shades 96 RGBA8 frames, at approximately 57,600 pixels each (320×180
for 16:9), then uses the existing bilinear resolve pass for playback. At 16:9 the
aligned allocations total about 21.4 MiB per window once warm. Cache entries
survive Key2/Key5 switching. A changed sample extent invalidates them; allocation
is bounded to 96 frames and a maximum 640-pixel sample dimension.

The source interval is t=8 through t≈11.96. Playback reverses at each endpoint,
producing a repeating ≈7.9-second forward/backward loop without a hard time jump.
This trades continuous forward travel and fine screen detail for cheap runtime
playback. Each frame costs a cloud evaluation only on its first use; warm frames
perform the resolve alone. No steady-state raymarch is hidden on the foreground
lane. Actual on-device frame time and appearance still need validation.

Run `python3 tools/test_cloud_background.py` in Cubes and
`python3 tools/test_shadertoy_dispatch.py` plus
`python3 tools/test_shadertoy_catalog.py` in TRUEOS. Rebuild TRUEOS and Cubes:
the new host flag/cache requires the kernel change, but no shader re-bake.
