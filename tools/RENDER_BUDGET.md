# Retained scene budgets and marker rendering

The 8,192-seed cap and 3,840 detailed-world-cube budget are separate from a
measured frame-rate guarantee. The total includes previews, markers, and the
81-seed companion when visible. Source assets can contain more cubes than one
frame admits.

The September 10 startup rejection was a capacity mismatch: the V3 public ABI
advertised 8,192 rows, but Helio's transform allocator accepted only 4,096. Mesh
creation requests storage for the advertised maximum. It therefore lost its GPU
transform path before even the 81-seed opening puzzle submitted its first frame.
The capture recorded `churn-transform-row-capacity`, then `frame-submit: -95`.
The generic Churn CPU fallback does not supply Cubes' retained V3 transform path.

The GPU row capacity is now 8,192, and compile-time checks keep both the public
ABI and Cubes' budget within the downstream capacity. Updating the app alone
cannot repair an already booted kernel with the old allocator. The GPU compute
kernel already receives a runtime row count; this change does not change its
native shader artifact or the cube shader.

## Proposed real point markers

Current markers still go through the 44-index CubePatchList1 draw. The hull
shader emits two triangles for primitive IDs 0 and 1 and zero tessellation levels
for the other 42. It avoids cube-corner evaluation, but not the patch processing.

| Per collapsed marker | Current implementation | Proposed point stream |
|---|---|---|
| Submitted primitive references | 44 patches | 1 point |
| Visible geometry | 2 triangles | 1 rasterized point square |
| Hull/tessellation/domain stages | Enabled | Disabled |
| Size | Projected, clamped 1–9 pixels | Same size rule |
| Occlusion | Shared scene depth | Preserve shared scene depth |

At 8,192 collapsed markers the current path submits 360,448 patch references.
Changing that to 8,192 points reduces front-end work; it is not a claim of a 44×
frame-rate improvement. CPU visibility, transforms, memory traffic, pixel work,
background rendering, synchronization, and presentation still contribute.

TRUEOS already exposes POINT_LIST and integer per-draw point widths through its
mixed-topology API. Cubes' retained V3 path currently binds one mesh topology and
its dedicated cube pipeline; its additional static primitives are restricted to
lines. The useful change is a dedicated marker stream/pipeline inside the same
retained render submission, with shared camera, palette colors, target, and depth.
A vertex-provided point size would avoid a draw for every width; alternatively,
1–9 pixel widths can be bucketed into a few batched point draws. Preserve sRGB
color handling and check viewport-edge clipping against today's square markers.
The point shader can consume compact position/color/size data; per-marker model
and normal matrices need not remain in that eventual format.

This proposal is not implemented by the capacity repair. Benchmark the existing
and proposed paths at the same camera, visible seed counts, resolution, and GPU
clock setting. Record CPU selection time, GPU transform/draw time, and completed
frame intervals. The current loop waits for GPU completion and then sleeps 16 ms;
that sleep adds to rendering time, so it is not a 60 Hz deadline-based scheduler.

Host regression: `python3 ../TRUEOS/tools/test_retained_scene.py` checks ABI/SDK
agreement, the real allocator capacity helper, and the full 8,192-slot draw plan.
