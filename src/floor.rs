//! Orientation and interaction guides as one indexed line-list.
// 12 snap edges + 12 tool edges + 26 local grid lines at the largest tool.
pub const VERTICES: usize = 128;
pub const GRID_MARGIN: i32 = 2;
fn empty() -> [u8; VERTICES * 12] {
    let mut bytes = [0; VERTICES * 12];
    for vertex in bytes.chunks_exact_mut(12) {
        vertex[8..12].copy_from_slice(&2f32.to_le_bytes());
    }
    bytes
}
pub fn vertices(matrix: &[f32; 16], visible: bool) -> [u8; VERTICES * 12] {
    let mut bytes = empty();
    for line in 0..22 {
        let t = (line % 11) as f32 * 2.0 - 10.0;
        let (a, b) = if line < 11 {
            ([-10., 3.5, t], [10., 3.5, t])
        } else {
            ([t, 3.5, -10.], [t, 3.5, 10.])
        };
        let project = |p: [f32; 3]| -> [f32; 4] {
            core::array::from_fn(|r| {
                matrix[r] * p[0] + matrix[4 + r] * p[1] + matrix[8 + r] * p[2] + matrix[12 + r]
            })
        };
        let pair = if visible {
            clip(project(a), project(b))
        } else {
            None
        };
        for (j, p) in pair.unwrap_or([[0., 0., 2.]; 2]).iter().enumerate() {
            for k in 0..3 {
                let offset = ((line * 2 + j) * 3 + k) * 4;
                bytes[offset..offset + 4].copy_from_slice(&p[k].to_le_bytes());
            }
        }
    }
    bytes
}
fn clip(a: [f32; 4], b: [f32; 4]) -> Option<[[f32; 3]; 2]> {
    let planes = |p: [f32; 4]| {
        [
            p[3] + p[0],
            p[3] - p[0],
            p[3] + p[1],
            p[3] - p[1],
            p[2],
            p[3] - p[2],
        ]
    };
    let (pa, pb) = (planes(a), planes(b));
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for i in 0..6 {
        if pa[i] < 0.0 && pb[i] < 0.0 {
            return None;
        }
        if pa[i] < 0.0 {
            lo = lo.max(pa[i] / (pa[i] - pb[i]));
        }
        if pb[i] < 0.0 {
            hi = hi.min(pa[i] / (pa[i] - pb[i]));
        }
    }
    if lo > hi {
        return None;
    }
    let mut out = [[0.; 3]; 2];
    for (j, t) in [lo, hi].iter().enumerate() {
        let p: [f32; 4] = core::array::from_fn(|i| a[i] + t * (b[i] - a[i]));
        if p[3] <= 1e-6 || p.iter().any(|v| !v.is_finite()) {
            return None;
        }
        out[j] = [p[0] / p[3], p[1] / p[3], p[2] / p[3]];
    }
    Some(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clips_near_and_side_planes_without_infinities() {
        assert_eq!(
            clip([-2., 0., 0.5, 1.], [2., 0., 0.5, 1.]),
            Some([[-1., 0., 0.5], [1., 0., 0.5]])
        );
        assert_eq!(clip([0., 0., -2., 1.], [0., 0., -1., 1.]), None);
        assert!(clip([0., 0., -1., -1.], [0., 0., 0.5, 1.]).is_some());
    }
}

/// Local interaction guides are an overlay. Clip in world space first, then
/// use near depth so the retained scene cannot bury the highlighted selection.
/// Static draws already run after opaque geometry and use straight-alpha blending.
pub struct Overlay {
    pub bytes: [u8; VERTICES * 12],
    lines: usize,
}
impl Overlay {
    pub fn new() -> Self {
        Self {
            bytes: empty(),
            lines: 0,
        }
    }
    fn line(&mut self, matrix: &[f32; 16], a: [f32; 3], b: [f32; 3]) {
        assert!(self.lines * 2 + 2 <= VERTICES);
        let project = |p: [f32; 3]| {
            core::array::from_fn(|r| {
                matrix[r] * p[0] + matrix[4 + r] * p[1] + matrix[8 + r] * p[2] + matrix[12 + r]
            })
        };
        if let Some(pair) = clip(project(a), project(b)) {
            for (j, p) in pair.iter().enumerate() {
                for (axis, value) in [p[0], p[1], 0.].iter().enumerate() {
                    let offset = ((self.lines * 2 + j) * 3 + axis) * 4;
                    self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
                }
            }
        }
        self.lines += 1;
    }
    pub fn cube(&mut self, matrix: &[f32; 16], lo: [f32; 3], hi: [f32; 3]) {
        for corner in 0..8 {
            for axis in 0..3 {
                if corner & (1 << axis) != 0 {
                    continue;
                }
                let a =
                    core::array::from_fn(|i| if corner & (1 << i) == 0 { lo[i] } else { hi[i] });
                let mut b = a;
                b[axis] = hi[axis];
                self.line(matrix, a, b);
            }
        }
    }
    pub fn mining_grid(&mut self, matrix: &[f32; 16], target: crate::subcubes::MiningTarget) {
        use crate::subcubes::C1;
        let cut = target.cut;
        self.cube(
            matrix,
            cut.min.map(|x| x as f32 * C1),
            cut.min.map(|x| (x + cut.side) as f32 * C1),
        );
        let u = (target.face_axis + 1) % 3;
        let v = (target.face_axis + 2) % 3;
        // All tiers share the same c1 lattice. Two extra cells expose the next
        // snap positions around the exact volume that a click will remove.
        for (along, across) in [(u, v), (v, u)] {
            for cell in -GRID_MARGIN..=cut.side + GRID_MARGIN {
                let mut a = cut.min.map(|x| x as f32 * C1);
                a[target.face_axis] = target.face as f32 * C1;
                a[along] = (cut.min[along] + cell) as f32 * C1;
                a[across] = (cut.min[across] - GRID_MARGIN) as f32 * C1;
                let mut b = a;
                b[across] = (cut.min[across] + cut.side + GRID_MARGIN) as f32 * C1;
                self.line(matrix, a, b);
            }
        }
    }
}

pub fn cube_outline(matrix: &[f32; 16], lo: [f32; 3], hi: [f32; 3]) -> [u8; VERTICES * 12] {
    let mut overlay = Overlay::new();
    overlay.cube(matrix, lo, hi);
    overlay.bytes
}

#[cfg(test)]
mod overlay_tests {
    use super::*;
    use crate::subcubes::{Block, C1, MiningTarget, TOOLS};
    // Large visible orthographic box, including negative world coordinates.
    const MATRIX: [f32; 16] = [
        0.1, 0., 0., 0., 0., 0.1, 0., 0., 0., 0., 0.05, 0., 0., 0., 0.5, 1.,
    ];
    fn point(bytes: &[u8], vertex: usize) -> [f32; 3] {
        core::array::from_fn(|axis| {
            let at = vertex * 12 + axis * 4;
            f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
        })
    }
    #[test]
    fn all_tools_and_faces_have_local_c1_grid_and_visible_overlay_depth() {
        for side in TOOLS {
            for axis in 0..3 {
                for positive in [false, true] {
                    let min = [-4; 3];
                    let face = min[axis] + if positive { side } else { 0 };
                    let mut overlay = Overlay::new();
                    overlay.cube(&MATRIX, [-1.; 3], [1.; 3]);
                    overlay.mining_grid(
                        &MATRIX,
                        MiningTarget {
                            cut: Block {
                                min,
                                side,
                                material: 0,
                            },
                            face_axis: axis,
                            face,
                        },
                    );
                    assert_eq!(overlay.lines, 24 + 2 * (side + 5) as usize);
                    assert!(overlay.lines * 2 <= VERTICES);
                    for vertex in 0..overlay.lines * 2 {
                        let p = point(&overlay.bytes, vertex);
                        assert_eq!(p[2], 0., "overlays must remain in front of scene depth");
                        assert!(p.iter().all(|x| x.is_finite()));
                    }
                    assert_eq!(point(&overlay.bytes, overlay.lines * 2)[2], 2.);
                    // A face normal to Z projects both tangent axes without loss.
                    if axis == 2 {
                        let a = point(&overlay.bytes, 48);
                        let b = point(&overlay.bytes, 49);
                        assert!((a[0] - (-6. * C1 * 0.1)).abs() < 1e-6);
                        assert!((a[1] - (-6. * C1 * 0.1)).abs() < 1e-6);
                        assert!((b[1] - ((side - 2) as f32 * C1 * 0.1)).abs() < 1e-6);
                        let next = point(&overlay.bytes, 50);
                        assert!((next[0] - a[0] - C1 * 0.1).abs() < 1e-6);
                    }
                }
            }
        }
    }
    #[test]
    fn behind_camera_overlay_is_fully_clipped_before_depth_override() {
        let mut overlay = Overlay::new();
        overlay.cube(&MATRIX, [0., 0., -30.], [1., 1., -29.]);
        for vertex in 0..VERTICES {
            assert_eq!(point(&overlay.bytes, vertex)[2], 2.);
        }
    }
}
