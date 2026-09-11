# Key-8 image wall

The network slideshow uses the existing Picasso retained PBR shader with a
`POS_NORMAL_UV_TANGENT` panel: four vertices, six indices, one model instance and
one V2 submission. It branches before dynamic-cube visibility, marker reduction,
seed upload or patch expansion. Local Cubes modes keep their existing renderer.

A received PNG/JPEG is decoded in the networking worker through
`trueos::vmedia::decode_retained` using the scene's device. Its texture ID is
published only after upload into the retained carrier completes. The scene
thread swaps the ready texture between completed frames. A partial transfer or
decode failure keeps the old image visible. Key 8 reconnects; another numbered
mode disconnects. All transmitted data remains the original encoded image.

The material uses three slots: the incoming base-color image, one shared normal
map, and one shared occlusion map. The two static data maps repeat a four-texel
beveled-cell pattern over all 512×512 source-image cells. The PBR shader already
handles tangent-space normals, directional lighting, roughness and view-dependent
reflections, so no kernel shader change or new shader package is required.

The panel faces +Z at Z=-240, spans X/Y ±72, and maps image top-left to world
(-72,+72). Its tangent is +X and handedness -1, making texture +V point down the
image. Both faces are visible. Normal/occlusion strength fades for subpixel cells
to reduce mip-0 aliasing. The full image remains present at all distances.

This approximates a beveled surface; it does not create extruded silhouettes or
per-cell parallax. Surface walking uses a separate coarse collision grid whose
front lies on the panel. Mouse look, flight, Space and Home retain their normal
camera behavior. Placement and the R companion cube remain local-world tools.

Generate the deterministic static maps with:

```
python3 tools/prepare_slideshow_material.py
```

Only compact standard PNG maps are checked in. The resident normal and occlusion
maps each occupy 16 MiB, are cached once per client device, and survive reconnects.
A 512×512 base image occupies 1 MiB; during a swap the current and incoming base
textures coexist. Material texture handles and mesh buffers are released by their
owners. No CPU image readback or image-dependent geometry rebuild occurs.

Validation:

```
cargo check --offline
python3 tools/test_slideshow_network.py
python3 -B tools/test_slideshow_material.py
python3 tools/test_walker_camera.py
```

These are host checks of geometry, protocol, material descriptors, map data and
collision behavior. Native texture upload, rendered appearance, frame timing and
peak memory still require a live TRUEOS run.
