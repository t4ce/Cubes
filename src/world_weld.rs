//! Render-only, frame-local welding of equal opaque palette cubes.
//! The retained slot stays in flags[31:16]; this class uses only the low word.
use crate::orchard::{CUSTOM_RGB555, Cube};
use alloc::vec::Vec;

// 0x1000..0x17ff overlaps Rubik transparent faces 4/5 (face << 10).
// This tag requires both face bits 11/12; no valid Rubik face ID (0..5) does.
pub const CLASS: u32 = 0x1800;
pub const MATERIAL: u32 = 0x0400;
// Geometry lives on the same c1 grid as world collision. Never bridge a real gap.
const UNIT: f32 = crate::subcubes::C1;
const GRID: f32 = UNIT * 0.25; // smallest authored asset cell; centers use half this step
const EPS: f32 = 0.00005;

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
struct Key {
    color: u32,
    scale: u32,
    side: i32,
    center: [i32; 3], // half-GRID units
}
#[derive(Clone, Copy)]
struct Entry {
    key: Key,
    output: usize,
}
#[derive(Default)]
pub struct Welder {
    pub enabled: bool, // J toggles the experiment; baseline on startup.
    held: bool,
    entries: Vec<Entry>,
    masks: Vec<u32>,
    pub joined: usize,
    pub removed_faces: usize,
}
impl Welder {
    pub fn key(&mut self, held: bool, active: bool) {
        if held && !self.held && active {
            self.enabled = !self.enabled;
        }
        self.held = held;
    }
    pub fn prepare(
        &mut self,
        cubes: &mut [Cube],
        solids: &[(usize, usize)],
        colors: &[u32],
        ready: impl Fn(usize) -> bool,
    ) {
        self.joined = 0;
        self.removed_faces = 0;
        self.entries.clear();
        self.masks.clear();
        if !self.enabled {
            return;
        }
        self.masks.resize(cubes.len(), 0);
        for &(output, source) in solids {
            let cube = cubes[output];
            if !ready(source) {
                continue;
            }
            let flags = cube.flags & 0xffff;
            let color = if flags & CUSTOM_RGB555 != 0 {
                let Some(index) = colors.iter().position(|&rgb| rgb == flags & 0x7fff) else {
                    continue;
                };
                if index >= 16 {
                    continue;
                }
                index as u32
            } else if flags & !7 == 24576 && flags & 7 < 6 {
                MATERIAL | (flags & 7)
            } else {
                continue;
            };
            let side = libm::roundf(cube.scale * 2. / GRID) as i32;
            if side < 1 || !cube.scale.is_finite() {
                continue;
            }
            let full = side as f32 * GRID;
            let gap = full - cube.scale * 2.;
            if gap < -EPS || gap > UNIT * 0.02 + EPS {
                continue;
            }
            let center = cube.center.map(|v| libm::roundf(v / (GRID * 0.5)) as i32);
            if !(0..3).all(|a| {
                cube.center[a].is_finite()
                    && (cube.center[a] - center[a] as f32 * GRID * 0.5).abs() <= EPS
            }) {
                continue;
            }
            self.entries.push(Entry {
                key: Key {
                    color,
                    scale: cube.scale.to_bits(),
                    side,
                    center,
                },
                output,
            });
        }
        self.entries.sort_unstable_by_key(|e| e.key);
        // Ambiguous duplicate cells cannot supply reliable coverage.
        let unique = |index: usize| {
            (index == 0 || self.entries[index - 1].key != self.entries[index].key)
                && (index + 1 == self.entries.len()
                    || self.entries[index + 1].key != self.entries[index].key)
        };
        for (i, entry) in self.entries.iter().enumerate() {
            if !unique(i) {
                continue;
            }
            for axis in 0..3 {
                let mut neighbor = entry.key;
                let Some(next) = entry
                    .key
                    .side
                    .checked_mul(2)
                    .and_then(|step| neighbor.center[axis].checked_add(step))
                else {
                    continue;
                };
                neighbor.center[axis] = next;
                if let Ok(j) = self.entries.binary_search_by_key(&neighbor, |e| e.key) {
                    if !unique(j) {
                        continue;
                    }
                    // WORLD_ROTATION is a half turn around X: world +Y/+Z are local -Y/-Z.
                    let face = axis * 2 + usize::from(axis != 0);
                    self.masks[entry.output] |= 1 << face;
                    self.masks[self.entries[j].output] |= 1 << (face ^ 1);
                }
            }
        }
        for entry in &self.entries {
            let mask = self.masks[entry.output];
            if mask == 0 {
                continue;
            }
            let cube = &mut cubes[entry.output];
            cube.scale = entry.key.side as f32 * GRID * 0.5;
            cube.flags = CLASS | entry.key.color | (mask << 4);
            self.joined += 1;
            self.removed_faces += mask.count_ones() as usize;
        }
    }
    /// DS triangles, not submitted patches: the ABI still submits 44 per seed.
    pub fn triangles_saved(&self) -> usize {
        self.joined * 32 + self.removed_faces * 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cube(x: f32, y: f32, z: f32) -> Cube {
        Cube {
            center: [x, y, z].map(|v| v * UNIT),
            scale: UNIT * 0.493,
            flags: CUSTOM_RGB555 | 123,
        }
    }
    fn run(cubes: &mut [Cube]) -> Welder {
        let mut w = Welder::default();
        w.enabled = true;
        let solids: Vec<_> = (0..cubes.len()).map(|i| (i, i)).collect();
        w.prepare(cubes, &solids, &[123, 456], |_| true);
        w
    }
    fn mask(c: Cube) -> u32 {
        (c.flags >> 4) & 63
    }
    #[test]
    fn pairs_on_every_axis_remove_both_faces_and_meet_exactly() {
        for axis in 0..3 {
            let mut cubes = [cube(0., 0., 0.), cube(0., 0., 0.)];
            cubes[1].center[axis] = UNIT;
            let w = run(&mut cubes);
            assert_eq!(
                (w.joined, w.removed_faces, 88 - w.triangles_saved()),
                (2, 2, 20)
            );
            let face = axis * 2 + usize::from(axis != 0);
            assert_eq!(mask(cubes[0]), 1 << face);
            assert_eq!(mask(cubes[1]), 1 << (face ^ 1));
            assert_eq!(
                cubes[0].center[axis] + cubes[0].scale,
                cubes[1].center[axis] - cubes[1].scale
            );
        }
    }
    #[test]
    fn quarter_c1_asset_cells_and_carousel_spacing_are_supported() {
        for (side, gap) in [(UNIT * 0.25, 0.002), (UNIT * 2., 0.004)] {
            let mut cubes = [cube(0., 0., 0.), cube(0., 0., 0.)];
            for c in &mut cubes {
                c.scale = (side - gap) * 0.5;
            }
            cubes[1].center[0] = side;
            assert_eq!(run(&mut cubes).joined, 2);
            assert_eq!(cubes[0].scale, side * 0.5);
        }
    }
    #[test]
    fn block_keeps_only_its_outer_shell_and_culls_the_center() {
        let mut cubes = Vec::new();
        for z in 0..3 {
            for y in 0..3 {
                for x in 0..3 {
                    cubes.push(cube(x as f32, y as f32, z as f32));
                }
            }
        }
        let w = run(&mut cubes);
        assert_eq!((w.joined, w.removed_faces), (27, 108));
        assert_eq!(27 * 44 - w.triangles_saved(), 108);
        assert_eq!(mask(cubes[13]), 63);
    }
    #[test]
    fn different_colors_sizes_partial_contacts_and_real_gaps_are_preserved() {
        for variant in 0..7 {
            let mut cubes = [cube(0., 0., 0.), cube(1., 0., 0.)];
            match variant {
                0 => cubes[1].flags = CUSTOM_RGB555 | 456,
                1 => cubes[1].scale *= 2.,
                2 => cubes[1].center[1] += UNIT * 0.5,
                3 => cubes[1].center[0] += UNIT,
                4 => cubes[1].scale *= 0.9,
                5 => cubes[1].center[0] += 0.0005,
                _ => cubes[1].flags = 25088 | 4096 | (3 << 10), // transparent landing cube
            }
            let before = cubes;
            assert_eq!(run(&mut cubes).joined, 0);
            for i in 0..2 {
                assert_eq!(
                    (cubes[i].scale, cubes[i].flags),
                    (before[i].scale, before[i].flags)
                );
            }
        }
    }
    #[test]
    fn markers_missing_or_growing_neighbors_do_not_cut_holes() {
        let original = [cube(0., 0., 0.), cube(1., 0., 0.)];
        let mut w = Welder::default();
        w.enabled = true;
        for solids in [vec![(0, 10)], vec![(0, 10), (1, 11)]] {
            let mut cubes = original;
            w.prepare(&mut cubes, &solids, &[123], |id| id == 10);
            assert_eq!(w.joined, 0);
        }
        let mut cubes = original;
        w.prepare(&mut cubes, &[(0, 10), (1, 11)], &[123], |_| true);
        assert_eq!(w.joined, 2);
        // Rebuild from authored cubes each frame; removing a neighbor restores its cap.
        let mut cubes = [original[0]];
        w.prepare(&mut cubes, &[(0, 10)], &[123], |_| true);
        assert_eq!(cubes[0].flags, original[0].flags);
    }
    #[test]
    fn duplicate_cells_do_not_supply_coverage() {
        let mut cubes = [cube(0., 0., 0.), cube(1., 0., 0.), cube(1., 0., 0.)];
        assert_eq!(run(&mut cubes).joined, 0);
    }
    #[test]
    fn material_identity_is_preserved_and_not_mixed_with_rgb() {
        let mut cubes = [cube(0., 0., 0.), cube(1., 0., 0.)];
        for c in &mut cubes {
            c.flags = 24576 | 5;
        }
        assert_eq!(run(&mut cubes).joined, 2);
        assert!(
            cubes
                .iter()
                .all(|c| c.flags & MATERIAL != 0 && c.flags & 15 == 5)
        );
        let mut cubes = [cube(0., 0., 0.), cube(1., 0., 0.)];
        cubes[1].flags = 24576;
        assert_eq!(run(&mut cubes).joined, 0);
    }
    #[test]
    fn toggle_is_edge_triggered_and_disabled_restores_baseline() {
        let mut w = Welder::default();
        w.key(true, false);
        assert!(!w.enabled);
        w.key(false, true);
        w.key(true, true);
        assert!(w.enabled);
        w.key(true, true);
        assert!(w.enabled);
        w.key(false, true);
        w.key(true, true);
        assert!(!w.enabled);
        let mut cubes = [cube(0., 0., 0.), cube(1., 0., 0.)];
        w.prepare(&mut cubes, &[(0, 0), (1, 1)], &[123], |_| true);
        assert_eq!(w.joined, 0);
        assert!(cubes.iter().all(|c| c.flags == CUSTOM_RGB555 | 123));
    }
}
