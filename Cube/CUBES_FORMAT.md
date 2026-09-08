# CUBES strict-grid nature format v1

This is the compact source format for the nature builder. Trees, rocks/ores, and bushes all use the same clean baseline:

- one reusable procedural cube shape
- strict `1x` integer placement grid
- legal cube sizes only: `1x`, `2x`, `3x`, `4x`
- no occupied grid cell may be used by two cubes
- every rendered cube is uniformly scaled
- a `1%` gap of one base grid unit is removed from the rendered side length so neighboring faces never coincide
- no rigs, bones, free-positioned pieces, or animal-specific data

The file stores no repeated mesh geometry. Each cube is represented by one compact grid record plus palette metadata.

## File layout

All multi-byte values are little-endian.

### Header — 16 bytes

| Offset | Type | Meaning |
|---:|---|---|
| 0 | `u8[4]` | ASCII `CUBE` |
| 4 | `u8` | Version = `1` |
| 5 | `u8` | Flags = `0` for strict-grid static nature asset |
| 6 | `u8` | Gap percent of one `1x` grid unit; demo writes `1` |
| 7 | `u8` | Cube record size = `8` |
| 8 | `u16` | Cube count |
| 10 | `u8` | Palette entry count |
| 11 | `u8` | Maximum legal size tier = `4` |
| 12 | `f32` | World-space size of one `1x` grid cell (`grid_unit`) |

### Palette — 4 bytes per entry

Each palette entry is `RGBA8`.

### Cube record — 8 bytes

| Offset | Type | Meaning |
|---:|---|---|
| 0 | `i8` | Minimum occupied grid cell X |
| 1 | `i8` | Minimum occupied grid cell Y |
| 2 | `i8` | Minimum occupied grid cell Z |
| 3 | `u8` | Size tier: `1..4` |
| 4 | `u8` | Palette index |
| 5 | `u8` | Semantic part id |
| 6 | `u8` | Reserved flags, currently `0` |
| 7 | `u8` | Reserved, currently `0` |

For a record `(gx, gy, gz, size)`, the exact cube center is reconstructed without storing floats:

```text
center_grid = [gx, gy, gz] + [size, size, size] * 0.5
center_world = center_grid * grid_unit
```

The nominal cube side is:

```text
nominal_side = size * grid_unit
```

The builder's visual separation is:

```text
gap_world = grid_unit * gap_percent / 100
rendered_side = nominal_side - gap_world
```

The same subtraction is used for every size tier, so the visible separation between adjacent logical cubes stays tied to the smallest grid unit.

## Occupancy rule

A size-`N` cube starting at `(gx, gy, gz)` reserves exactly `N³` smallest grid cells:

```text
x = gx .. gx + N - 1
y = gy .. gy + N - 1
z = gz .. gz + N - 1
```

A placement is legal only when none of those cells has already been reserved. A `4x` cube therefore reserves exactly `4 × 4 × 4 = 64` smallest cells.

## Current semantic part ids

```text
0  generic cube
1  trunk / stem
2  branch
3  foliage / crown / tip
4  rock
5  ore-bearing rock
6  bush / shrub foliage
7  berry
8  grass / grass base
```

These ids are only metadata. Rendering still uses the palette index and the one reusable cube primitive.

## Mapping to `t4ce/Cubes`

The compact file is an asset representation, not a replacement for the renderer's retained runtime seed. Each 8-byte cube record can expand to one runtime cube instance:

```text
translation = center_world
scale       = [rendered_side * 0.5; 3]
rotation    = identity
color       = palette[palette_index]
```

The `* 0.5` assumes the procedural reference cube spans two mesh-local units across, matching the current `Cubes` procedural-cube path.

This keeps the authored asset extremely small while preserving the exact strict-grid layout.

## Validation and runtime profile

The nature builder and Cubes Demo use this one layout, not the earlier
half-grid-center/bone layout which also carried version byte 1. Header bytes
7 and 11 must be exactly 8 and 4. Older layouts must be explicitly converted
or re-exported, never guessed by the runtime.

The demo accepts 1–1024 cubes, 1–255 opaque RGBA palette entries, positive finite
grid units, gap percentages 1–99 (builder default 1), and rendered half-scales
of at least 0.001. Header flags and both reserved record bytes must be zero.
Semantic part IDs are metadata and do not change placement. Cell occupancy is
validated before rendering; invalid assets fail the build with their filename.
Asset +Y is mapped to demo -Y and the assembled asset is centered for orbiting;
neither operation changes relative positions or sizes.
