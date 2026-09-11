//! Build-time conversion of one `.cubes` world into one exposed-face PBR mesh.
//! The authored records remain the source of truth for walking and interaction.

pub const VERTEX_STRIDE: usize = 48;

pub struct Mesh {
    pub vertices: Vec<u8>,
    pub indices: Vec<u8>,
    pub source_cubes: usize,
    pub quads: usize,
}

#[derive(Clone, Copy)]
struct Cube {
    origin: [i32; 3],
    side: i32,
    color: usize,
}

pub fn build(bytes: &[u8]) -> Mesh {
    assert_eq!(&bytes[..4], b"CUBE");
    assert_eq!(bytes[4], 2);
    assert_eq!(bytes[7], 12);
    let unit = f32::from_le_bytes(bytes[12..16].try_into().unwrap());
    let palette_count = bytes[10] as usize;
    let cubes = source_cubes(bytes, palette_count);

    // A sorted packed-key set is substantially smaller than a HashSet for the
    // large c8 plateaus. It is only a build-time visibility mask and never
    // enters the Blueprint image.
    let mut occupied = Vec::new();
    for cube in &cubes {
        for x in 0..cube.side {
            for y in 0..cube.side {
                for z in 0..cube.side {
                    occupied.push(cell_key([
                        cube.origin[0] + x,
                        cube.origin[1] + y,
                        cube.origin[2] + z,
                    ]));
                }
            }
        }
    }
    occupied.sort_unstable();
    occupied.dedup();

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut quads = 0;
    for cube in &cubes {
        for axis in 0..3 {
            let uv_axes = [(axis + 1) % 3, (axis + 2) % 3];
            for sign in [-1, 1] {
                let side = cube.side as usize;
                let mut exposed = vec![false; side * side];
                for u in 0..side {
                    for v in 0..side {
                        let mut adjacent = cube.origin;
                        adjacent[axis] += if sign < 0 { -1 } else { cube.side };
                        adjacent[uv_axes[0]] += u as i32;
                        adjacent[uv_axes[1]] += v as i32;
                        exposed[u * side + v] =
                            occupied.binary_search(&cell_key(adjacent)).is_err();
                    }
                }
                for rect in greedy_rectangles(&mut exposed, side) {
                    push_quad(
                        &mut vertices,
                        &mut indices,
                        cube,
                        axis,
                        sign,
                        uv_axes,
                        rect,
                        unit,
                        palette_count,
                    );
                    quads += 1;
                }
            }
        }
    }
    assert!(cubes.len() < u32::MAX as usize);
    assert!(vertices.len() / VERTEX_STRIDE < u32::MAX as usize);
    assert!(indices.len() / 4 < u32::MAX as usize);
    Mesh {
        vertices,
        indices,
        source_cubes: cubes.len(),
        quads,
    }
}

fn source_cubes(bytes: &[u8], palette_count: usize) -> Vec<Cube> {
    let start = 16 + palette_count * 4;
    let mut cubes = Vec::new();
    for raw in bytes[start..].chunks_exact(12) {
        let origin: [i32; 3] = core::array::from_fn(|axis| {
            i16::from_le_bytes([raw[axis * 2], raw[axis * 2 + 1]]) as i32
        });
        let packed_side = raw[6] as i32;
        let color = raw[7] as usize;
        let side = raw[9] as i32;
        assert!(side > 0 && packed_side >= side && packed_side % side == 0);
        assert!(color < palette_count);
        let count = packed_side / side;
        for x in 0..count {
            for y in 0..count {
                for z in 0..count {
                    cubes.push(Cube {
                        origin: [
                            origin[0] + x * side,
                            origin[1] + y * side,
                            origin[2] + z * side,
                        ],
                        side,
                        color,
                    });
                }
            }
        }
    }
    cubes
}

fn cell_key([x, y, z]: [i32; 3]) -> u64 {
    debug_assert!((-1025..=1024).contains(&x));
    debug_assert!((-1025..=1024).contains(&y));
    debug_assert!((-1025..=1024).contains(&z));
    const WIDTH: u64 = 2050;
    (((x + 1025) as u64 * WIDTH + (y + 1025) as u64) * WIDTH) + (z + 1025) as u64
}

/// Cover the exposed portion of one face without overlapping an occupied
/// neighbour. Normal world faces collapse to one rectangle; this also handles
/// mixed-tier partial coverage deterministically.
fn greedy_rectangles(mask: &mut [bool], side: usize) -> Vec<[usize; 4]> {
    let mut out = Vec::new();
    for u in 0..side {
        for v in 0..side {
            if !mask[u * side + v] {
                continue;
            }
            let mut width = 1;
            while v + width < side && mask[u * side + v + width] {
                width += 1;
            }
            let mut height = 1;
            'height: while u + height < side {
                for x in v..v + width {
                    if !mask[(u + height) * side + x] {
                        break 'height;
                    }
                }
                height += 1;
            }
            for y in u..u + height {
                for x in v..v + width {
                    mask[y * side + x] = false;
                }
            }
            out.push([u, v, height, width]);
        }
    }
    out
}

fn push_quad(
    vertices: &mut Vec<u8>,
    indices: &mut Vec<u8>,
    cube: &Cube,
    axis: usize,
    sign: i32,
    uv_axes: [usize; 2],
    [u, v, height, width]: [usize; 4],
    unit: f32,
    palette_count: usize,
) {
    const TILE: f32 = 96.0;
    let atlas = TILE * palette_count as f32;
    let atlas_uv = [
        [(cube.color as f32 * TILE + 0.5) / atlas, 0.5 / TILE],
        [
            ((cube.color + 1) as f32 * TILE - 0.5) / atlas,
            (TILE - 0.5) / TILE,
        ],
    ];
    let mut normal = [0.0; 3];
    normal[axis] = sign as f32;
    let mut tangent = [0.0; 3];
    tangent[uv_axes[0]] = 1.0;
    normal[2] = -normal[2];
    tangent[2] = -tangent[2];
    let fixed = cube.origin[axis] + if sign < 0 { 0 } else { cube.side };
    let corners = [
        [u, v],
        [u + height, v],
        [u + height, v + width],
        [u, v + width],
    ];
    let base = (vertices.len() / VERTEX_STRIDE) as u32;
    for (corner, [du, dv]) in corners.into_iter().enumerate() {
        let mut p = cube.origin;
        p[axis] = fixed;
        p[uv_axes[0]] += du as i32;
        p[uv_axes[1]] += dv as i32;
        let position = [
            p[0] as f32 * unit,
            p[1] as f32 * unit,
            -(p[2] as f32) * unit,
        ];
        let uv = match corner {
            0 => [atlas_uv[0][0], atlas_uv[0][1]],
            1 => [atlas_uv[1][0], atlas_uv[0][1]],
            2 => [atlas_uv[1][0], atlas_uv[1][1]],
            _ => [atlas_uv[0][0], atlas_uv[1][1]],
        };
        for value in position
            .into_iter()
            .chain(normal)
            .chain(uv)
            .chain(tangent)
            .chain([1.0])
        {
            vertices.extend_from_slice(&value.to_le_bytes());
        }
    }
    let order = if sign > 0 {
        [0, 2, 1, 0, 3, 2]
    } else {
        [0, 1, 2, 0, 2, 3]
    };
    for index in order {
        indices.extend_from_slice(&(base + index).to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_world_is_one_exposed_surface_with_no_internal_cube_faces() {
        let bytes = include_bytes!("../Cube/lvl27/world_01_sky.cubes");
        let mesh = build(bytes);
        assert_eq!(mesh.source_cubes, 15_024);
        assert_eq!(mesh.quads, 27_388);
        assert_eq!(mesh.vertices.len(), mesh.quads * 4 * VERTEX_STRIDE);
        assert_eq!(mesh.indices.len(), mesh.quads * 6 * 4);
        assert!(mesh.quads < mesh.source_cubes * 2);
    }

    #[test]
    fn greedy_rectangles_preserve_a_partial_face_exactly() {
        let mut mask = vec![true, true, false, true, false, false, true, true, true];
        let rectangles = greedy_rectangles(&mut mask, 3);
        let area: usize = rectangles.iter().map(|r| r[2] * r[3]).sum();
        assert_eq!(area, 6);
        assert!(mask.iter().all(|&cell| !cell));
    }
}
