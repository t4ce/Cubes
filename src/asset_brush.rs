//! Session-local Key5 asset catalog and grid-aligned placement.
extern crate alloc;
use crate::orchard::{self, Cube};
use alloc::vec::Vec;
pub const PLACEMENT_SCALE: f32 = 0.25;
pub const PREVIEW_SEEDS: usize = 384;
pub const GHOST_SEEDS: usize = 768;
pub const FULL_WORLD_SEEDS: usize = 768;
pub const MAX_PLACED_CUBES: usize = 16_384;

pub struct Brush {
    pub catalog: orchard::Pages,
    pub selected: usize,
    pub placement_preview: bool,
    ghost_key: Option<(usize, [f32; 3], [f32; 3])>,
    pub ghost: Vec<Cube>,
    pub worlds: Vec<Vec<Cube>>,
}
impl Brush {
    pub fn new(sources: &'static [(&'static str, &'static [u8])]) -> Self {
        Self {
            catalog: orchard::Pages::new(sources, false),
            selected: 0,
            placement_preview: false,
            ghost_key: None,
            ghost: Vec::new(),
            worlds: (0..27).map(|_| Vec::new()).collect(),
        }
    }
    pub fn update_preview(&mut self, target: Option<([f32; 3], [f32; 3])>, bytes: &[u8]) {
        let key = if self.placement_preview {
            target.map(|(p, n)| (self.selected, p, n))
        } else {
            None
        };
        if key == self.ghost_key {
            return;
        }
        self.ghost_key = key;
        self.ghost = key.map_or_else(Vec::new, |(_, p, n)| place(bytes, p, n));
    }
    pub fn cycle(&mut self, wheel: i16) -> Result<(), &'static str> {
        let step = if wheel > 0 {
            -1
        } else if wheel < 0 {
            1
        } else {
            0
        };
        self.selected =
            (self.selected as isize + step).rem_euclid(self.catalog.len() as isize) as usize;
        self.catalog.load(self.selected)?;
        Ok(())
    }
}
/// Preserve authored cube sizes and color, placing the base on the selected face.
pub fn place(bytes: &[u8], point: [f32; 3], normal: [f32; 3]) -> Vec<Cube> {
    let unit = f32::from_le_bytes(bytes[12..16].try_into().unwrap()) * PLACEMENT_SCALE;
    let records = &bytes[16 + 4 * bytes[10] as usize..];
    let origin = |r: &[u8]| {
        [
            r[0] as i8 as f32,
            r[1] as i8 as f32,
            -(r[2] as i8 as f32) - r[3] as f32,
        ]
    };
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for r in records.chunks_exact(8) {
        let p = origin(r);
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a] + r[3] as f32);
        }
    }
    let x = if normal[0].abs() > 0.5 {
        [0., 0., 1.]
    } else {
        [1., 0., 0.]
    };
    let z = [
        x[1] * normal[2] - x[2] * normal[1],
        x[2] * normal[0] - x[0] * normal[2],
        x[0] * normal[1] - x[1] * normal[0],
    ];
    let anchor = point.map(|v| libm::roundf(v / unit));
    records
        .chunks_exact(8)
        .map(|r| {
            let p = origin(r);
            let local = [
                p[0] - libm::floorf((lo[0] + hi[0]) * 0.5) + r[3] as f32 * 0.5,
                p[1] - lo[1] + r[3] as f32 * 0.5,
                p[2] - libm::floorf((lo[2] + hi[2]) * 0.5) + r[3] as f32 * 0.5,
            ];
            let color = &bytes[16 + r[4] as usize * 4..];
            Cube {
                center: core::array::from_fn(|a| {
                    (anchor[a] + x[a] * local[0] + normal[a] * local[1] + z[a] * local[2]) * unit
                }),
                scale: (r[3] as f32 - bytes[6] as f32 / 100.) * unit * 0.5,
                flags: orchard::CUSTOM_RGB555
                    | ((color[0] as u32 * 31 + 127) / 255)
                    | (((color[1] as u32 * 31 + 127) / 255) << 5)
                    | (((color[2] as u32 * 31 + 127) / 255) << 10),
            }
        })
        .collect()
}
/// The native flat-marker shader interprets tiny scales as half-width / 1000.
pub fn marker_scale(scale: f32, depth: f32, projection_y: f32, height: u32) -> f32 {
    let pixels =
        (scale * height as f32 * projection_y.abs() / depth.abs().max(0.001)).clamp(1., 9.);
    (pixels * depth.abs() / (height.max(1) as f32 * projection_y.abs().max(0.001)) / 1000.)
        .clamp(0.0000001, 0.0009)
}
pub fn detailed(rank: usize, cube: Cube, eye: [f32; 3], projection_y: f32, height: u32) -> bool {
    let distance = libm::sqrtf(
        (0..3)
            .map(|a| {
                let d = cube.center[a] - eye[a];
                d * d
            })
            .sum::<f32>(),
    )
    .max(0.001);
    rank < FULL_WORLD_SEEDS && cube.scale * height as f32 * projection_y.abs() / distance >= 2.
}

#[cfg(test)]
mod tests {
    use super::*;
    const TREE: &[u8] = include_bytes!("../Cube/plant_pine.cubes");
    #[test]
    fn placement_preserves_grid_size_and_attaches_base_on_all_six_faces() {
        for axis in 0..3 {
            for sign in [-1., 1.] {
                let mut normal = [0.; 3];
                normal[axis] = sign;
                let pieces = place(TREE, [0.; 3], normal);
                assert!(!pieces.is_empty());
                let unit = f32::from_le_bytes(TREE[12..16].try_into().unwrap()) * PLACEMENT_SCALE;
                let mut nearest = f32::INFINITY;
                for c in pieces {
                    let half = libm::roundf(c.scale * 2. / unit) * unit * 0.5;
                    nearest = nearest.min(c.center[axis] * sign - half);
                    for a in 0..3 {
                        let lo = (c.center[a] - half) / unit;
                        assert!((lo - libm::roundf(lo)).abs() < 0.001);
                    }
                    assert_ne!(c.flags & orchard::CUSTOM_RGB555, 0);
                }
                assert!(nearest.abs() < 0.001);
            }
        }
    }
    #[test]
    fn placement_preview_is_visual_only_and_follows_toggle_and_target() {
        static SOURCES: &[(&str, &[u8])] = &[("tree", TREE)];
        let mut b = Brush::new(SOURCES);
        let target = Some(([0.; 3], [0., 1., 0.]));
        b.update_preview(target, TREE);
        assert!(b.ghost.is_empty());
        b.placement_preview = true;
        b.update_preview(target, TREE);
        let expected = place(TREE, [0.; 3], [0., 1., 0.]);
        assert_eq!(b.ghost.len(), expected.len());
        for (a, c) in b.ghost.iter().zip(expected) {
            assert_eq!(a.center, c.center);
            assert_eq!(a.scale, c.scale);
            assert!(marker_scale(c.scale, 10., 1., 1000) < 0.001);
        }
        let ptr = b.ghost.as_ptr();
        b.update_preview(target, TREE);
        assert_eq!(b.ghost.as_ptr(), ptr);
        assert!(b.worlds.iter().all(Vec::is_empty));
        b.update_preview(None, TREE);
        assert!(b.ghost.is_empty());
        b.update_preview(target, TREE);
        b.placement_preview = false;
        b.update_preview(target, TREE);
        assert!(b.ghost.is_empty());
    }
    #[test]
    fn wheel_wraps_and_marker_width_tracks_projected_cube_size() {
        static SOURCES: &[(&str, &[u8])] = &[("a", TREE), ("b", TREE)];
        let mut b = Brush::new(SOURCES);
        b.cycle(1).unwrap();
        assert_eq!(b.selected, 1);
        b.cycle(-1).unwrap();
        assert_eq!(b.selected, 0);
        let small = marker_scale(0.01, 10., 1., 1000);
        let large = marker_scale(0.08, 10., 1., 1000);
        assert!(large > small);
        assert!((large * 1000. * 1000. / 10. - 8.).abs() < 0.001);
        let c = Cube {
            center: [0., 0., 1.],
            scale: 1.,
            flags: 0,
        };
        assert!(detailed(FULL_WORLD_SEEDS - 1, c, [0.; 3], 1., 1000));
        assert!(!detailed(FULL_WORLD_SEEDS, c, [0.; 3], 1., 1000));
    }
}
