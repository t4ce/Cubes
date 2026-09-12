# Key-8 six-face image gallery

The old flat panel and per-image-pixel normal/occlusion maps have been removed.
Key 8 now displays six real beveled cube slabs, with one image centered on each
inward-facing world face. The scene has one retained indexed mesh and one PNG
atlas, using the native PBR shader with nearest filtering.

The complete importer, tier, placement, transport and rebuild instructions are
in [CubeSrv's README](../../TRUEOS-Blueprints/apps/cubesrv/README.md).

`src/slideshow.rs` builds tier geometry from `src/image_cube.rs`, exported from
the same GLB as CubeImage.html. After changing that reference, run
`python3 -B tools/prepare_image_cube.py`. The build checks the GLB digest;
`--check` verifies the complete generated export without rewriting it.

`src/network.rs` receives and validates the complete gallery package before
asking the native media API to decode its atlas. `src/slideshow_gpu.rs` owns
mesh buffers and texture lifetime. Same-tier replacements reuse the mesh;
changed-tier replacements create the new mesh before releasing the old one.
Failures preserve the old scene. Another numbered mode disconnects and releases
the gallery. The walker uses six analytic collision volumes; small decorative
bevel recesses are intentionally solid for navigation.

Validation:

```sh
cargo check --offline
python3 -B tools/prepare_image_cube.py --check
python3 -B tools/test_slideshow_network.py
python3 -B tools/test_walker_camera.py
```

The network harness exercises all four geometry tiers, face orientation,
internal-face removal, bounds, package validation, chunk reordering/duplicates
and material submission. Native XeLP rendering and frame timing remain untested
until the rebuilt TRUEOS kernel, CubeSrv and Cubes are run together.
