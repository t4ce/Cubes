//! Picking intersects the actual convex bevel shape, including occluding centers.
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
pub fn pick(origin: [f32; 3], dir: [f32; 3], spacing: f32, scale: f32) -> Option<usize> {
    let mut nearest = f32::INFINITY;
    let mut result = None;
    for id in 0..27 {
        let cell = [
            (id % 3) as f32 - 1.0,
            ((id / 3) % 3) as f32 - 1.0,
            (id / 9) as f32 - 1.0,
        ];
        let o: [f32; 3] = core::array::from_fn(|i| (origin[i] - cell[i] * spacing) / scale);
        let d = dir.map(|x| x / scale);
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
                    let bound = match count {
                        1 => 1.0,
                        2 => 1.8,
                        _ => 2.6,
                    };
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
