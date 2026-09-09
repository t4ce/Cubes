//! Picking intersects the actual convex bevel shape, including occluding centers.
/// Axis, edge and corner plane supports of the baked convex reference cube.
/// The orchard occluder regression checks this same definition.
pub(crate) const PLANE_BOUNDS: [f32; 4] = [0.0, 1.0, 1.8, 2.6];
pub fn ray(inv: &[f32; 16], x: i32, y: i32, w: u32, h: u32) -> Option<([f32; 3], [f32; 3])> {
    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
        return None;
    }
    let unproject = |z| {
        let v = [
            2.0 * (x as f32 + 0.5) / w as f32 - 1.0,
            1.0 - 2.0 * (y as f32 + 0.5) / h as f32,
            z,
            1.0,
        ];
        let p: [f32; 4] = core::array::from_fn(|r| (0..4).map(|c| inv[c * 4 + r] * v[c]).sum());
        if p[3].abs() < 1e-8 {
            None
        } else {
            Some([p[0] / p[3], p[1] / p[3], p[2] / p[3]])
        }
    };
    let a = unproject(0.0)?;
    let b = unproject(1.0)?;
    Some((a, core::array::from_fn(|i| b[i] - a[i])))
}
#[cfg(test)]
pub fn pick(origin: [f32; 3], dir: [f32; 3], spacing: f32, scale: f32) -> Option<usize> {
    pick_poses(origin, dir, spacing, scale, |id| {
        (
            [
                (id % 3) as f32 - 1.0,
                ((id / 3) % 3) as f32 - 1.0,
                (id / 9) as f32 - 1.0,
            ],
            [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
        )
    })
}

/// Intersect the same beveled seed in its current cubie-local orientation.
pub fn pick_poses(
    origin: [f32; 3],
    dir: [f32; 3],
    spacing: f32,
    scale: f32,
    pose: impl Fn(usize) -> ([f32; 3], [[f32; 3]; 3]),
) -> Option<usize> {
    let mut nearest = f32::INFINITY;
    let mut result = None;
    for id in 0..27 {
        let (cell, basis) = pose(id);
        let translated: [f32; 3] = core::array::from_fn(|i| origin[i] - cell[i] * spacing);
        let o: [f32; 3] =
            basis.map(|axis| (0..3).map(|i| axis[i] * translated[i]).sum::<f32>() / scale);
        let d: [f32; 3] = basis.map(|axis| (0..3).map(|i| axis[i] * dir[i]).sum::<f32>() / scale);
        let mut near = 0.0f32;
        let mut far = f32::INFINITY;
        for x in -1i32..=1 {
            for y in -1i32..=1 {
                for z in -1i32..=1 {
                    let n = [x, y, z];
                    let count = x.abs() + y.abs() + z.abs();
                    if count == 0 {
                        continue;
                    }
                    let bound = PLANE_BOUNDS[count as usize];
                    let distance = bound - (0..3).map(|i| n[i] as f32 * o[i]).sum::<f32>();
                    let denom = (0..3).map(|i| n[i] as f32 * d[i]).sum::<f32>();
                    if denom.abs() < 1e-8 {
                        if distance < 0.0 {
                            far = -1.0;
                        }
                    } else if denom > 0.0 {
                        far = far.min(distance / denom);
                    } else {
                        near = near.max(distance / denom);
                    }
                }
            }
        }
        if near <= far && near < nearest {
            nearest = near;
            result = Some(id);
        }
    }
    result.filter(|&id| {
        [id % 3, (id / 3) % 3, id / 9]
            .iter()
            .filter(|&&x| x != 1)
            .count()
            >= 2
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn face_centers_occlude_and_do_not_click_through() {
        assert_eq!(pick([0., 0., -10.], [0., 0., 1.], 1.1, 0.55), None);
        assert_eq!(pick([-1.1, -1.1, -10.], [0., 0., 1.], 1.1, 0.55), Some(0));
        assert_eq!(pick([0., -1.1, -10.], [0., 0., 1.], 1.1, 0.55), Some(1));
        assert_eq!(pick([8., 0., -10.], [0., 0., 1.], 1.1, 0.55), None);
    }
    #[test]
    fn exactly_twenty_selectable_cubies() {
        assert_eq!(
            (0..27)
                .filter(|&id| [id % 3, (id / 3) % 3, id / 9]
                    .iter()
                    .filter(|&&x| x != 1)
                    .count()
                    >= 2)
                .count(),
            20
        );
    }
}
