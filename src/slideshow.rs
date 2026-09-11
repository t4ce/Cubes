//! Image-wall geometry. Visual cells are shader normals, never cube seeds.
use alloc::vec::Vec;
pub const SIDE: usize = 512;
pub const HALF_EXTENT: f32 = 72.;
pub const DISTANCE: f32 = 240.;
pub const INDICES: [u32; 6] = [0, 1, 2, 0, 2, 3];
/// Position, normal, UV, tangent (including glTF handedness).
pub const VERTICES: [[f32; 12]; 4] = [
    [
        -HALF_EXTENT,
        HALF_EXTENT,
        -DISTANCE,
        0.,
        0.,
        1.,
        0.,
        0.,
        1.,
        0.,
        0.,
        -1.,
    ],
    [
        -HALF_EXTENT,
        -HALF_EXTENT,
        -DISTANCE,
        0.,
        0.,
        1.,
        0.,
        1.,
        1.,
        0.,
        0.,
        -1.,
    ],
    [
        HALF_EXTENT,
        -HALF_EXTENT,
        -DISTANCE,
        0.,
        0.,
        1.,
        1.,
        1.,
        1.,
        0.,
        0.,
        -1.,
    ],
    [
        HALF_EXTENT,
        HALF_EXTENT,
        -DISTANCE,
        0.,
        0.,
        1.,
        1.,
        0.,
        1.,
        0.,
        0.,
        -1.,
    ],
];
pub fn vertices() -> Vec<u8> {
    VERTICES
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect()
}
pub fn indices() -> Vec<u8> {
    INDICES.iter().flat_map(|v| v.to_le_bytes()).collect()
}
/// Mip-0 material maps should not shimmer when their cells are subpixel.
/// Fade relief at distance; preserve the full-resolution base image.
pub fn relief(eye: [f32; 3], projection_y: f32, height: u32) -> f32 {
    let distance =
        libm::sqrtf(eye[0] * eye[0] + eye[1] * eye[1] + (eye[2] + DISTANCE) * (eye[2] + DISTANCE))
            .max(0.01);
    let pixels =
        (2. * HALF_EXTENT / SIDE as f32) * projection_y.abs() * height as f32 / (2. * distance);
    ((pixels - 1.) / 2.).clamp(0., 1.)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wall_has_two_front_facing_triangles_and_image_aligned_tangents() {
        assert_eq!(vertices().len(), 4 * 48);
        assert_eq!(indices().len(), 6 * 4);
        for tri in INDICES.chunks_exact(3) {
            let [a, b, c] = [
                VERTICES[tri[0] as usize],
                VERTICES[tri[1] as usize],
                VERTICES[tri[2] as usize],
            ];
            assert!((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]) > 0.);
        }
        assert_eq!(&VERTICES[0][6..8], &[0., 0.]);
        assert_eq!(&VERTICES[2][6..8], &[1., 1.]);
        assert!(VERTICES.iter().all(|v| v[11] == -1.));
    }
    #[test]
    fn subpixel_bevels_fade_but_resolve_when_approached() {
        assert_eq!(relief([0.; 3], 2.63, 441), 0.);
        assert_eq!(relief([0., 0., -200.], 2.63, 441), 1.);
    }
}
