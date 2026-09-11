//! Distance-based averaging of visible world markers, after detail/occlusion selection.
//! Four fixed radix passes; buffers are reused and no per-marker square root is needed.
use crate::{
    asset_brush,
    orchard::{CUSTOM_RGB555, Cube},
};
use alloc::vec::Vec;

/// Fractions of the world diagonal where groups grow to 2, 4, 8 and 16 dots.
pub const DISTANCE_STEPS: [f32; 4] = [0.125, 0.375, 0.625, 0.875];
pub const MAX_GROUP: usize = 16;

#[derive(Clone, Copy)]
struct Entry {
    cube: Cube,
    key: u32,
}

pub struct Reducer {
    pub cubes: Vec<Cube>,
    pub dots_before: usize,
    pub dots_after: usize,
    entries: Vec<Entry>,
    scratch: Vec<Entry>,
    minimum: [f32; 3],
    inverse_extent: f32,
    diagonal_squared: f32,
}

impl Reducer {
    pub fn new() -> Self {
        let side = 2048. * crate::subcubes::C1;
        Self {
            cubes: Vec::new(),
            dots_before: 0,
            dots_after: 0,
            entries: Vec::new(),
            scratch: Vec::new(),
            minimum: [-side * 0.5; 3],
            inverse_extent: 1. / side,
            diagonal_squared: 3. * side * side,
        }
    }

    /// Recompute only on world selection or placement, never during camera motion.
    pub fn set_bounds(&mut self, cubes: &[Cube]) {
        if cubes.is_empty() {
            return;
        }
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for cube in cubes {
            for a in 0..3 {
                lo[a] = lo[a].min(cube.center[a] - cube.scale);
                hi[a] = hi[a].max(cube.center[a] + cube.scale);
            }
        }
        let sides: [f32; 3] = core::array::from_fn(|a| (hi[a] - lo[a]).max(0.001));
        self.minimum = lo;
        self.inverse_extent = 1. / sides.into_iter().fold(0., f32::max);
        self.diagonal_squared = sides.into_iter().map(|s| s * s).sum();
    }

    fn level(&self, distance_squared: f32) -> u32 {
        DISTANCE_STEPS
            .into_iter()
            .filter(|step| distance_squared >= self.diagonal_squared * step * step)
            .count() as u32
    }

    fn key(&self, center: [f32; 3], level: u32) -> u32 {
        let cell: [u32; 3] = core::array::from_fn(|a| {
            ((center[a] - self.minimum[a]) * self.inverse_extent * 512.).clamp(0., 511.) as u32
        });
        (level << 27) | spread(cell[0]) | (spread(cell[1]) << 1) | (spread(cell[2]) << 2)
    }

    pub fn prepare(
        &mut self,
        source: &[Cube],
        visible: &[usize],
        eye: [f32; 3],
        view: &[f32; 16],
        projection_y: f32,
        height: u32,
    ) {
        self.prepare_with_growth(source, visible, eye, view, projection_y, height, |_| 1.);
    }

    pub fn prepare_with_growth(
        &mut self,
        source: &[Cube],
        visible: &[usize],
        eye: [f32; 3],
        view: &[f32; 16],
        projection_y: f32,
        height: u32,
        growth: impl Fn(usize) -> f32,
    ) {
        self.prepare_with_budget(
            source,
            visible,
            eye,
            view,
            projection_y,
            height,
            asset_brush::FULL_WORLD_SEEDS,
            growth,
        );
    }

    pub fn prepare_with_budget(
        &mut self,
        source: &[Cube],
        visible: &[usize],
        eye: [f32; 3],
        view: &[f32; 16],
        projection_y: f32,
        height: u32,
        detail_budget: usize,
        growth: impl Fn(usize) -> f32,
    ) {
        self.prepare_with_solids(source, visible, eye, view, projection_y, height, detail_budget, growth, |_| false);
    }

    pub fn prepare_with_solids(
        &mut self, source: &[Cube], visible: &[usize], eye: [f32; 3],
        view: &[f32; 16], projection_y: f32, height: u32, detail_budget: usize,
        growth: impl Fn(usize) -> f32, solid: impl Fn(usize) -> bool,
    ) {
        self.cubes.clear();
        self.entries.clear();
        self.dots_before = 0;
        self.dots_after = 0;
        for (rank, &id) in visible.iter().enumerate() {
            let cube = source[id];
            let distance_squared = asset_brush::lod_distance_squared(cube.center, eye, view);
            if rank < detail_budget && (solid(id) || asset_brush::detailed_with_budget(
                rank,
                cube,
                distance_squared,
                projection_y,
                height,
                detail_budget,
            )) {
                // Classify LOD using authored size. Keep growing solids above
                // the shader's 0.001 flat-dot threshold, including their first frame.
                self.cubes.push(Cube {
                    scale: (cube.scale * growth(id)).max(0.00101),
                    ..cube
                });
                continue;
            }
            self.dots_before += 1;
            let level = self.level(distance_squared);
            // Only RGB555 world colors are averaged; semantic material IDs are never mixed.
            if level == 0 || cube.flags & CUSTOM_RGB555 == 0 {
                self.push_marker(cube, view, projection_y, height);
            } else {
                self.entries.push(Entry {
                    cube,
                    key: self.key(cube.center, level),
                });
            }
        }
        if self.entries.is_empty() {
            return;
        }
        self.scratch.resize(self.entries.len(), self.entries[0]);
        // Band first, then Morton spatial order. Counting buckets replace comparison sorting.
        for shift in [0, 8, 16, 24] {
            let mut offsets = [0usize; 256];
            for entry in &self.entries {
                offsets[((entry.key >> shift) & 255) as usize] += 1;
            }
            let mut at = 0;
            for count in &mut offsets {
                let next = at + *count;
                *count = at;
                at = next;
            }
            for entry in &self.entries {
                let offset = &mut offsets[((entry.key >> shift) & 255) as usize];
                self.scratch[*offset] = *entry;
                *offset += 1;
            }
            core::mem::swap(&mut self.entries, &mut self.scratch);
        }
        let mut first = 0;
        while first < self.entries.len() {
            let key = self.entries[first].key;
            let group_size = (1usize << (key >> 27)).min(MAX_GROUP);
            let mut end = first + 1;
            // Never bridge distant islands: keep each group inside a world-anchored
            // tile of 1/32 of its side. Partial/sparse tiles retain extra markers.
            while end < self.entries.len()
                && end - first < group_size
                && self.entries[end].key >> 12 == key >> 12
            {
                end += 1;
            }
            let mut center = [0.; 3];
            let mut color = [0u32; 3];
            let mut scale = 0.;
            for entry in &self.entries[first..end] {
                for a in 0..3 {
                    center[a] += entry.cube.center[a];
                    color[a] += (entry.cube.flags >> (a * 5)) & 31;
                }
                scale += entry.cube.scale;
            }
            let count = (end - first) as u32;
            let inverse = 1. / count as f32;
            let flags = CUSTOM_RGB555
                | color.into_iter().enumerate().fold(0, |rgb, (a, sum)| {
                    rgb | (((sum + count / 2) / count) << (a * 5))
                });
            self.push_marker(
                Cube {
                    center: center.map(|v| v * inverse),
                    scale: scale * inverse,
                    flags,
                },
                view,
                projection_y,
                height,
            );
            first = end;
        }
    }

    fn push_marker(&mut self, cube: Cube, view: &[f32; 16], projection_y: f32, height: u32) {
        let depth = -(view[2] * cube.center[0]
            + view[6] * cube.center[1]
            + view[10] * cube.center[2]
            + view[14]);
        self.cubes.push(Cube {
            scale: asset_brush::marker_scale(cube.scale, depth, projection_y, height),
            ..cube
        });
        self.dots_after += 1;
    }


}

fn spread(mut value: u32) -> u32 {
    value = (value | (value << 16)) & 0x030000ff;
    value = (value | (value << 8)) & 0x0300f00f;
    value = (value | (value << 4)) & 0x030c30c3;
    (value | (value << 2)) & 0x09249249
}

#[cfg(test)]
mod tests {
    use super::*;
    fn view(distance: f32) -> [f32; 16] {
        [
            1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., -distance, 1.,
        ]
    }
    fn patch() -> Vec<Cube> {
        (0..16)
            .map(|i| Cube {
                center: [1. + (i % 4) as f32 * 0.2, 1. + (i / 4) as f32 * 0.2, 0.],
                scale: 0.01,
                flags: CUSTOM_RGB555 | if i < 8 { 31 } else { 31 << 10 },
            })
            .collect()
    }
    #[test]
    fn growth_keeps_authored_lod_and_does_not_change_source_or_colour() {
        let cubes = [Cube {
            center: [0.; 3],
            scale: 0.1,
            flags: CUSTOM_RGB555 | 31,
        }];
        let mut reducer = Reducer::new();
        for factor in [0., 0.5, 1.] {
            reducer.prepare_with_growth(&cubes, &[0], [0., 0., 2.], &view(2.), 2.414, 441, |_| {
                factor
            });
            let drawn = reducer.cubes[0];
            assert_eq!(drawn.center, cubes[0].center);
            assert_eq!(drawn.flags, cubes[0].flags);
            assert!(drawn.scale >= 0.001);
            assert_eq!(reducer.dots_before, 0);
            if factor > 0. {
                assert_eq!(drawn.scale, cubes[0].scale * factor);
            }
        }
        assert_eq!(cubes[0].scale, 0.1);
    }

    #[test]
    fn close_markers_stay_individual_and_distance_steps_reach_sixteen_to_one() {
        let cubes = patch();
        let ids: Vec<_> = (0..cubes.len()).collect();
        let mut reducer = Reducer::new();
        let diagonal = libm::sqrtf(reducer.diagonal_squared);
        for (fraction, count) in [(0.05, 16), (0.25, 8), (0.5, 4), (0.75, 2), (1., 1)] {
            let distance = diagonal * fraction * asset_brush::LOD_FORWARD_REACH;
            reducer.prepare(
                &cubes,
                &ids,
                [0., 0., distance],
                &view(distance),
                2.414,
                441,
            );
            assert_eq!(reducer.dots_before, 16);
            assert_eq!(reducer.dots_after, count);
            assert_eq!(reducer.cubes.len(), count);
            assert!(
                reducer
                    .cubes
                    .iter()
                    .all(|c| c.scale > 0. && c.scale < 0.001)
            );
            if count == 16 {
                for (before, after) in cubes.iter().zip(&reducer.cubes) {
                    assert_eq!(before.center, after.center);
                    assert_eq!(before.flags, after.flags);
                }
            }
        }
        let result = reducer.cubes[0];
        assert!((result.center[0] - 1.3).abs() < 1e-5);
        assert!((result.center[1] - 1.3).abs() < 1e-5);
        assert_eq!(result.flags, CUSTOM_RGB555 | 16 | (16 << 10));
    }
    #[test]
    fn full_cubes_and_non_rgb_materials_are_not_merged_or_promoted() {
        let mut cubes = patch();
        cubes.insert(
            0,
            Cube {
                center: [0.; 3],
                scale: 10.,
                flags: CUSTOM_RGB555 | 123,
            },
        );
        cubes.push(Cube {
            center: [0.; 3],
            scale: 0.01,
            flags: 24576 | 5,
        });
        let ids: Vec<_> = (0..cubes.len()).collect();
        let mut reducer = Reducer::new();
        reducer.prepare(&cubes, &ids, [0., 0., 1420.], &view(1420.), 2.414, 441);
        assert_eq!(reducer.cubes.len(), 3);
        assert_eq!(reducer.cubes[0].center, cubes[0].center);
        assert_eq!(reducer.cubes[0].scale, cubes[0].scale);
        assert_eq!(reducer.cubes[0].flags, cubes[0].flags);
        assert!(reducer.cubes.iter().any(|c| c.flags == 24576 | 5));
        assert_eq!((reducer.dots_before, reducer.dots_after), (17, 2));
        // Reordered visibility changes neither the single group's mean nor its material.
        let reverse: Vec<_> = (1..17).rev().collect();
        reducer.prepare(&cubes, &reverse, [0., 0., 1420.], &view(1420.), 2.414, 441);
        assert_eq!(reducer.cubes.len(), 1);
        assert_eq!(reducer.cubes[0].flags, CUSTOM_RGB555 | 16 | (16 << 10));
    }
    #[test]
    fn sparse_tiles_and_partial_groups_never_drop_source_dots() {
        let mut cubes = patch();
        cubes.push(Cube {
            center: [150., 150., 0.],
            ..cubes[0]
        });
        let ids: Vec<_> = (0..cubes.len()).collect();
        let mut reducer = Reducer::new();
        reducer.prepare(&cubes, &ids, [0., 0., 1420.], &view(1420.), 2.414, 441);
        assert_eq!((reducer.dots_before, reducer.dots_after), (17, 2));
        assert!(reducer.cubes.iter().any(|c| c.center == [150., 150., 0.]));
        reducer.prepare(&cubes, &ids[..5], [0., 0., 1420.], &view(1420.), 2.414, 441);
        assert_eq!((reducer.dots_before, reducer.dots_after), (5, 1));
        let average_x = cubes[..5].iter().map(|c| c.center[0]).sum::<f32>() / 5.;
        assert!((reducer.cubes[0].center[0] - average_x).abs() < 1e-5);
    }
    #[test]
    fn empty_views_clear_output_and_warm_frames_reuse_capacity() {
        let cubes = patch();
        let ids: Vec<_> = (0..cubes.len()).collect();
        let mut reducer = Reducer::new();
        reducer.prepare(&cubes, &ids, [0., 0., 1420.], &view(1420.), 2.414, 441);
        let capacity = (
            reducer.entries.capacity(),
            reducer.scratch.capacity(),
            reducer.cubes.capacity(),
        );
        for _ in 0..8 {
            reducer.prepare(&cubes, &ids, [0., 0., 1420.], &view(1420.), 2.414, 441);
            assert_eq!(
                capacity,
                (
                    reducer.entries.capacity(),
                    reducer.scratch.capacity(),
                    reducer.cubes.capacity()
                )
            );
        }
        reducer.prepare(&cubes, &[], [0., 0., 1420.], &view(1420.), 2.414, 441);
        assert!(reducer.cubes.is_empty());
        assert_eq!((reducer.dots_before, reducer.dots_after), (0, 0));
    }
    #[test]
    fn only_visible_admitted_markers_enter_groups_and_keep_the_input_budget() {
        let asset = crate::orchard::Asset {
            name: "marker-grid",
            radius: 10.,
            cubes: (0..64)
                .map(|i| Cube {
                    center: [1. + (i % 8) as f32 * 0.2, 1. + (i / 8) as f32 * 0.2, -710.],
                    scale: 0.01,
                    flags: CUSTOM_RGB555 | 31,
                })
                .collect(),
        };
        let mut visibility = crate::orchard::VisibilityScratch::new();
        let projection = [
            1.,
            0.,
            0.,
            0.,
            0.,
            1.,
            0.,
            0.,
            0.,
            0.,
            800. / (0.01 - 800.),
            -1.,
            0.,
            0.,
            8. / (0.01 - 800.),
            0.,
        ];
        let (ids, stats) = crate::orchard::visible_with_lod(
            &mut visibility,
            &asset,
            [0.; 3],
            &projection,
            32,
            |id| id % 2 == 0,
            |id, rank| {
                asset_brush::detailed(
                    rank,
                    asset.cubes[id],
                    asset_brush::lod_distance_squared(asset.cubes[id].center, [0.; 3], &view(0.)),
                    1.,
                    441,
                )
            },
        );
        assert_eq!(ids.len(), 32);
        assert_eq!(stats.occluded, 0);
        assert!(stats.pending > 0);
        let mut reducer = Reducer::new();
        reducer.prepare(&asset.cubes, ids, [0.; 3], &view(0.), 1., 441);
        assert_eq!((reducer.dots_before, reducer.dots_after), (32, 8));
    }
    #[test]
    fn world_bounds_scale_thresholds_and_morton_order_is_unique() {
        let mut reducer = Reducer::new();
        reducer.set_bounds(&[
            Cube {
                center: [-10.; 3],
                scale: 1.,
                flags: CUSTOM_RGB555,
            },
            Cube {
                center: [10.; 3],
                scale: 1.,
                flags: CUSTOM_RGB555,
            },
        ]);
        assert_eq!(reducer.diagonal_squared, 22. * 22. * 3.);
        assert_eq!(reducer.level(reducer.diagonal_squared), 4);
        let mut keys = alloc::collections::BTreeSet::new();
        for x in 0..16 {
            for y in 0..16 {
                for z in 0..16 {
                    assert!(keys.insert(spread(x) | spread(y) << 1 | spread(z) << 2));
                }
            }
        }
    }
}
