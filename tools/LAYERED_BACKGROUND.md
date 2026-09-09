# Mandelbox background

Cubes opens one UI4 layered window. Picasso draws the foreground; a native
worker publishes Mandelbox on its independent background target. The worker
uses a 10 Hz ceiling and renders when the world, camera or extent changes.
Key 5 enables the background for the 27 worlds, using the six colors from
`Cube/cube_tree_builder_world_ramps.html`. Other modes clear it to transparent.

The shader is derived from the MIT-licensed `Cube/mandelbox.html`; its license
is preserved in `Cube/mandelbox/input.glsl`. This port uses the existing native
ShaderToy Image dispatch. It matches camera orientation and field of view with
a fixed fractal origin. The HTML's cached cubemap optimization is not included.

Run `python3 tools/bake_mandelbox.py` after shader edits. This reproducibly bakes
and packages the shader and updates the sibling TRUEOS kernel's admission
hashes and ABI contract. `python3 tools/bake_mandelbox.py --check` verifies those
artifacts without changing them. `python3 tools/test_background.py` also checks
the actual 27-world theme mapping and palette provenance.

Both the kernel and Blueprint need rebuilding for the new layered ABI and
authenticated ShaderToy program 16. See
[`TRUEOS/tools/docs/UI4_LAYERED_FRAMES.md`](../../TRUEOS/tools/docs/UI4_LAYERED_FRAMES.md)
for plane budgeting, paired resize, lifecycle and validation limits.
