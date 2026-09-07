//! Fixed seed lattice and screen-space activation, independent of UI/input APIs.
pub const COLS: usize = 16;
pub const ROWS: usize = 9;
pub const COUNT: usize = COLS * ROWS;
pub const SPACING: f32 = 0.8;
pub const CUBE_SCALE: f32 = 0.24;
// HS interprets scales below 0.001 as flat marker half-size / 1000.
pub fn marker_scale(depth: f32, projection_y: f32, height: u32) -> f32 {
    (3.0 * depth.abs() / (height.max(1) as f32 * projection_y.abs()) / 1000.0)
        .clamp(0.0000001, 0.0009)
}
pub const RADIUS_PX: f32 = 58.0;

pub fn position(index: usize) -> [f32; 3] {
    [
        ((index % COLS) as f32 - 7.5) * SPACING,
        ((index / COLS) as f32 - 4.0) * SPACING,
        0.0,
    ]
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

pub fn near(point: [f32; 2], cursor: [i32; 2], width: u32, height: u32) -> bool {
    if cursor[0] < 0 || cursor[1] < 0 || cursor[0] >= width as i32 || cursor[1] >= height as i32 {
        return false;
    }
    let dx = point[0] - cursor[0] as f32;
    let dy = point[1] - cursor[1] as f32;
    dx * dx + dy * dy <= RADIUS_PX * RADIUS_PX
}

pub fn move_camera(z: f32, forward: bool, backward: bool, delta: f32) -> f32 {
    (z + (forward as i32 - backward as i32) as f32 * delta.clamp(0.0, 0.1) * 3.0).clamp(-30.0, -1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn marker_is_three_pixels_at_different_camera_distances() {
        for depth in [1.0, 7.5, 30.0] {
            let scale = marker_scale(depth, 1.732, 441);
            assert!(scale > 0.0 && scale < 0.001);
            let pixels = scale * 1000.0 * 1.732 / depth * 441.0;
            assert!((pixels - 3.0).abs() < 0.0001);
        }
    }
    #[test]
    fn lattice_has_144_unique_xy_seeds() {
        for i in 0..COUNT {
            for j in 0..i {
                assert_ne!(position(i), position(j));
            }
        }
        assert_eq!(position(0), [-6.0, -3.2, 0.0]);
        assert_eq!(position(COUNT - 1), [6.0, 3.2, 0.0]);
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
        assert!(cursors.iter().any(|&c| near([690., 300.], c, 800, 450)));
        assert!(!cursors.iter().any(|&c| near([400., 225.], c, 800, 450)));
        assert!(!near([0., 0.], [-1, 0], 800, 450));
    }
    #[test]
    fn camera_is_bounded_and_opposite_keys_cancel() {
        assert_eq!(move_camera(-7.5, true, true, 0.1), -7.5);
        assert!(move_camera(-7.5, true, false, 0.1) > -7.5);
        assert_eq!(move_camera(-1., true, false, 5.), -1.);
        assert_eq!(move_camera(-30., false, true, 5.), -30.);
    }
}
