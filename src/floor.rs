//! Clipped Key2 orientation floor in normalized device coordinates.
// Preserve the retained static-buffer capacity across all scene modes.
pub const VERTICES: usize = 128;
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
