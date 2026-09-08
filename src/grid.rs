//! Fixed seed lattice and screen-space activation, independent of UI/input APIs.
pub const COLS: usize = 10;
pub const ROWS: usize = 10;
pub const COUNT: usize = 6 * COLS * ROWS;
// Ten-by-ten lattice on each of six walls, viewed from the room center.
pub const SPACING: f32 = 0.8;
/// Key-1 room walls sit farther from the origin than the compact puzzle.
pub const ROOM_HALF_EXTENT: f32 = 6.0;
pub const CUBE_GRID_AXIS: usize = 3;
pub const CUBE_GRID_COUNT: usize = CUBE_GRID_AXIS * CUBE_GRID_AXIS * CUBE_GRID_AXIS;
/// Key 3 fills the renderer's single retained draw group exactly.
pub const SPHERE_COUNT: usize = 1024;
pub const SPHERE_RADIUS: f32 = 6.0;
/// The expanding cursor circle covers 10% of the current viewport area.
pub const SPHERE_CURSOR_AREA_FRACTION: f32 = 0.10;
/// Retained V3 now permits 2,048 seeds. Key 3 intentionally keeps its
/// original 1,024-sphere scene; Key 5 uses the additional world budget.
pub const MAX_SEED_COUNT: usize = 2048;
pub const CUBE_GRID_SPACING: f32 = 2.2;
pub const CUBE_GRID_SCALE: f32 = 0.55;
/// Compact Rubik layout: one percent of a cubie's 1.1-unit side length.
/// This avoids coplanar face overlap without changing cube scale or expansion.
pub const CUBE_COMPACT_GAP: f32 = 2.0 * CUBE_GRID_SCALE * 0.01;
pub const CUBE_COMPACT_SPACING: f32 = 2.0 * CUBE_GRID_SCALE + CUBE_COMPACT_GAP;
pub const CUBE_SCALE: f32 = 0.24;
pub const CUBE_LOCAL_RADIUS: f32 = 1.74;
/// The cursor activation circle extends this many rendered cube radii.
pub const CURSOR_RADIUS_IN_CUBE_RADII: f32 = 2.75;
// HS interprets scales below 0.001 as flat marker half-size / 1000.
pub const MARKER_WIDTH_PX: f32 = 9.0;
pub fn marker_scale(depth: f32, projection_y: f32, height: u32) -> f32 {
    (MARKER_WIDTH_PX * depth.abs() / (height.max(1) as f32 * projection_y.abs()) / 1000.0)
        .clamp(0.0000001, 0.0009)
}
/// Convert an expanded cube's world-space radius to the matching screen-space
/// cursor activation radius. Keeping `cube_scale` explicit makes the circle
/// grow whenever the expanded-cube scale changes.
pub fn cursor_radius_px(cube_scale: f32, depth: f32, projection_y: f32, height: u32) -> f32 {
    cube_scale.abs()
        * CUBE_LOCAL_RADIUS
        * CURSOR_RADIUS_IN_CUBE_RADII
        * projection_y.abs()
        * height.max(1) as f32
        / (2.0 * depth.abs().max(0.001))
}

pub fn position(index: usize) -> [f32; 3] {
    let face = index / 100;
    let i = index % 100;
    let a = ((i % 10) as f32 - 4.5) * SPACING;
    let b = ((i / 10) as f32 - 4.5) * SPACING;
    match face {
        0 => [ROOM_HALF_EXTENT, a, b],
        1 => [-ROOM_HALF_EXTENT, a, b],
        2 => [a, ROOM_HALF_EXTENT, b],
        3 => [a, -ROOM_HALF_EXTENT, b],
        4 => [a, b, ROOM_HALF_EXTENT],
        _ => [a, b, -ROOM_HALF_EXTENT],
    }
}

/// A static 3×3×3 lattice, centered on the origin and deliberately spaced
/// wide enough for its individual cubes to read as a volume.
pub fn cube_position(index: usize) -> [f32; 3] {
    let x = index % CUBE_GRID_AXIS;
    let y = (index / CUBE_GRID_AXIS) % CUBE_GRID_AXIS;
    let z = index / (CUBE_GRID_AXIS * CUBE_GRID_AXIS);
    let center = (CUBE_GRID_AXIS - 1) as f32 * 0.5;
    [
        (x as f32 - center) * CUBE_GRID_SPACING,
        (y as f32 - center) * CUBE_GRID_SPACING,
        (z as f32 - center) * CUBE_GRID_SPACING,
    ]
}

/// Evenly distributes the Key-3 seeds across the inside of a radius-six sphere
/// without a vertex buffer or pole clustering.
pub fn sphere_position(index: usize) -> [f32; 3] {
    let t = (index as f32 + 0.5) / SPHERE_COUNT as f32;
    let y = 1.0 - 2.0 * t;
    let radial = libm::sqrtf((1.0 - y * y).max(0.0));
    let azimuth = index as f32 * 2.399_963_1;
    [
        SPHERE_RADIUS * radial * libm::cosf(azimuth),
        SPHERE_RADIUS * y,
        SPHERE_RADIUS * radial * libm::sinf(azimuth),
    ]
}

pub fn sphere_cursor_radius_px(width: u32, height: u32) -> f32 {
    libm::sqrtf(
        SPHERE_CURSOR_AREA_FRACTION * width.max(1) as f32 * height.max(1) as f32
            / core::f32::consts::PI,
    )
}

pub fn project(matrix: &[f32; 16], point: [f32; 3], width: u32, height: u32) -> Option<[f32; 2]> {
    let mut clip = [0.0; 4];
    for row in 0..4 {
        clip[row] = matrix[row] * point[0]
            + matrix[4 + row] * point[1]
            + matrix[8 + row] * point[2]
            + matrix[12 + row];
    }
    if width == 0
        || height == 0
        || clip.iter().any(|x| !x.is_finite())
        || clip[3] <= 0.0
        || clip[2] < 0.0
        || clip[2] > clip[3]
    {
        return None;
    }
    Some([
        (clip[0] / clip[3] * 0.5 + 0.5) * width as f32,
        (0.5 - clip[1] / clip[3] * 0.5) * height as f32,
    ])
}

pub fn near(point: [f32; 2], cursor: [i32; 2], width: u32, height: u32, radius_px: f32) -> bool {
    if cursor[0] < 0 || cursor[1] < 0 || cursor[0] >= width as i32 || cursor[1] >= height as i32 {
        return false;
    }
    let dx = point[0] - cursor[0] as f32;
    let dy = point[1] - cursor[1] as f32;
    dx * dx + dy * dy <= radius_px * radius_px
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn marker_is_nine_pixels_at_different_camera_distances() {
        for depth in [1.0, 7.5, 30.0] {
            let scale = marker_scale(depth, 1.732, 441);
            assert!(scale > 0.0 && scale < 0.001);
            let pixels = scale * 1000.0 * 1.732 / depth * 441.0;
            assert!((pixels - 9.0).abs() < 0.0001);
        }
    }
    #[test]
    fn cursor_radius_tracks_expanded_cube_scale() {
        let small = cursor_radius_px(0.24, 7.5, 1.732, 441);
        let large = cursor_radius_px(0.48, 7.5, 1.732, 441);
        assert!((small - 58.0).abs() < 1.0);
        assert!((large - small * 2.0).abs() < 0.0001);
    }
    #[test]
    fn room_has_600_unique_wall_seeds() {
        assert_eq!(COUNT, 600);
        for i in 0..COUNT {
            for j in 0..i {
                assert_ne!(position(i), position(j));
            }
        }
        assert_eq!(position(0), [6.0, -3.6000001, -3.6000001]);
        for i in 0..COUNT {
            assert_eq!(
                position(i)
                    .iter()
                    .filter(|x| x.abs() == ROOM_HALF_EXTENT)
                    .count(),
                1
            );
        }
    }
    #[test]
    fn cube_lattice_has_27_centered_unique_seeds() {
        assert_eq!(CUBE_GRID_COUNT, 27);
        assert_eq!(cube_position(0), [-2.2, -2.2, -2.2]);
        assert_eq!(cube_position(13), [0.0, 0.0, 0.0]);
        assert_eq!(cube_position(CUBE_GRID_COUNT - 1), [2.2, 2.2, 2.2]);
        for i in 0..CUBE_GRID_COUNT {
            for j in 0..i {
                assert_ne!(cube_position(i), cube_position(j));
            }
        }
    }
    #[test]
    fn sphere_has_1024_radius_six_seeds() {
        assert_eq!(SPHERE_COUNT, 1024);
        for i in 0..SPHERE_COUNT {
            let p = sphere_position(i);
            let radius = libm::sqrtf(p.iter().map(|v| v * v).sum::<f32>());
            assert!((radius - SPHERE_RADIUS).abs() < 0.000_01);
        }
    }
    #[test]
    fn sphere_cursor_covers_ten_percent_of_viewport() {
        let width = 784;
        let height = 441;
        let radius = sphere_cursor_radius_px(width, height);
        let fraction = core::f32::consts::PI * radius * radius / (width * height) as f32;
        assert!((fraction - SPHERE_CURSOR_AREA_FRACTION).abs() < 0.000_001);
    }
    #[test]
    fn compact_puzzle_spacing_has_a_one_percent_cube_gap() {
        assert!((CUBE_COMPACT_GAP - 0.011).abs() < 0.000001);
        assert!((CUBE_COMPACT_SPACING - 1.111).abs() < 0.000001);
    }
    #[test]
    fn projection_matches_negative_y_viewport() {
        let m = [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
        ];
        assert_eq!(project(&m, [0., 0., 0.5], 800, 450), Some([400., 225.]));
        assert_eq!(project(&m, [0., 1., 0.5], 800, 450), Some([400., 0.]));
        assert_eq!(project(&m, [0., 0., -1.], 800, 450), None);
    }
    #[test]
    fn independent_cursors_and_outside_window() {
        let cursors = [[100, 100], [700, 300]];
        assert!(
            cursors
                .iter()
                .any(|&c| near([690., 300.], c, 800, 450, 58.0))
        );
        assert!(
            !cursors
                .iter()
                .any(|&c| near([400., 225.], c, 800, 450, 58.0))
        );
        assert!(!near([0., 0.], [-1, 0], 800, 450, 58.0));
    }
}
