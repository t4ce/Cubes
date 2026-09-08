# CUBES compact asset format v1

This format is intended for assets made entirely from the single procedural cube used by `t4ce/Cubes`.
The asset file does **not** duplicate cube mesh vertices. Each placed cube is one compact point/seed plus metadata.

## Design goals

- One logical point per cube.
- Cube sizes are restricted to `1x`, `2x`, `3x`, or `4x`.
- Positions remain snapped to the smallest `1x` grid.
- Trees use a `-1%` smallest-cell visual bias to preserve a tiny gap.
- Animals use a small positive visual bias so adjacent rigid cubes can clip at joints without giving up the logical non-overlap grid.
- A quadruped asset may assign each cube rigidly to one bone. No skin weights are needed because a cube belongs to exactly one bone.

## File layout

All multi-byte values are little-endian.

### Header: 16 bytes

| Offset | Type | Meaning |
|---:|---|---|
| 0 | `u8[4]` | ASCII `CUBE` |
| 4 | `u8` | Version, currently `1` |
| 5 | `u8` | Asset type: `0=static/tree`, `1=quadruped` |
| 6 | `i8` | Visual side bias in percent of one smallest grid cell (`-1` tree gap, `+10` animal clip in the demo) |
| 7 | `u8` | Bone count |
| 8 | `u16` | Cube count |
| 10 | `u8` | Palette entry count |
| 11 | `u8` | Cube record size, currently `8` |
| 12 | `f32` | Smallest grid-unit side length |

### Palette: 4 bytes per entry

`RGBA8`.

### Bone record: 8 bytes per bone

| Byte | Type | Meaning |
|---:|---|---|
| 0 | `i8` | Parent bone index, `-1` for root |
| 1 | `i8` | Pivot X in half-grid units |
| 2 | `i8` | Pivot Y in half-grid units |
| 3 | `i8` | Pivot Z in half-grid units |
| 4 | `u8` | Bone flags, reserved in v1 |
| 5..7 | `u8[3]` | Reserved |

The demo uses the fixed quadruped bone order:

```text
0  root
1  spine
2  chest
3  neck
4  head
5  frontLUpper
6  frontLLower
7  frontRUpper
8  frontRLower
9  hindLUpper
10 hindLLower
11 hindRUpper
12 hindRLower
13 tail
```

### Cube record: 8 bytes per cube

| Byte | Type | Meaning |
|---:|---|---|
| 0 | `i8` | Cube-center X in half-grid units |
| 1 | `i8` | Cube-center Y in half-grid units |
| 2 | `i8` | Cube-center Z in half-grid units |
| 3 | `u8` | Size tier: `1..4` |
| 4 | `u8` | Palette index |
| 5 | `u8` | Bone index, or `255` for an unrigged/static cube |
| 6 | `u8` | Semantic part id (body, leaf, head, leg, etc.) |
| 7 | `u8` | Cube flags; bit 0 currently marks the animal visual-clip mode |

The point position is reconstructed exactly as:

```text
center = vec3(cx2, cy2, cz2) * (grid_unit * 0.5)
```

The procedural cube in the `Cubes` runtime is mesh-local with a side length of 2, so the positive uniform half-scale is:

```text
rendered_side = size_tier * grid_unit + visual_bias_percent/100 * grid_unit
uniform_scale = rendered_side * 0.5
```

This preserves equal-sided cubes at every tier.

## Mapping to the current `Cubes` retained path

The compact file should be treated as an **asset/source format**, not as a replacement for the current retained runtime ABI.
On load, each 8-byte cube record can be expanded to one existing `RetainedTransformSeed`:

```text
translation = decoded center (or bone-transformed center)
scale       = [uniform_scale; 3]
rotation    = identity for static assets, bone rotation for rigged animals
local_radius = existing cube local radius
previous_translation = translation initially
draw_group  = 0
flags       = group-local slot metadata + custom colour bits
```

For a first implementation, this keeps the current GPU contract untouched while making source assets dramatically smaller.

### Compact colour without adding another GPU buffer

The current pipeline already uses the upper 16 bits of `seed.flags` for the group-local seed slot, so leave those bits alone.
For custom static/tree/animal assets, the currently unused lower-bit mode can reserve bit 15 as a custom-colour marker and use bits 0..14 as RGB555:

```text
bit 15      CUSTOM_RGB555
bits 10..14 blue 5-bit
bits 5..9   green 5-bit
bits 0..4   red 5-bit
```

The loader converts the exported palette's RGBA8 entry to RGB555 when it creates the runtime seed. The domain shader then decodes RGB555 when `CUSTOM_RGB555` is set. This avoids an extra per-instance colour buffer and leaves the upper 16-bit slot numbering intact.

## Rigging model

Animals use rigid cube-to-bone assignment rather than vertex skin weights:

```text
cube -> one bone index -> bone matrix -> cube transform
```

That is sufficient because each cube is already an independent rigid primitive. A simple walk cycle only needs to update the 14 bone transforms; each cube inherits the transform of its assigned bone.

For the first runtime version it is reasonable to generate/update `RetainedTransformSeed`s on the CPU. If large animated crowds become important later, the same compact records can stay resident and a small bone-matrix buffer can drive the retained transform compute path on the GPU.

## Current demo limits

- Cube centers are signed `i8` half-grid values, giving an asset-local range of roughly `-64 .. +63.5` grid cells per axis.
- Cube count is `u16`, although the current `Cubes` retained renderer has a much smaller practical per-frame seed ceiling.
- The v1 format assumes one rigid bone per cube and no per-cube non-uniform scaling.
