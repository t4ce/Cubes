//! Five scrolling asset instances plus two fixed neighbouring-group previews.
//! Group metadata is exported from AssetShowcase.
use crate::{
    orchard::{self, Cube},
    reveal,
    subcubes::C1,
};
use alloc::vec::Vec;
pub const SLIDE_MS: u64 = 333;
pub const FRAME_CYCLE_MS: u64 = 4000;
pub const PITCH: f32 = 3.6;
pub const DISPLAY_SIDE: f32 = 2.4;
pub const GROUP_PITCH: f32 = 3.6;
const ROW_SLOTS: usize = 5;
const SLOT_COUNT: usize = 7;
// RGB palette index (9 bits), translucency marker, opacity class, showcase class.
// Uses the existing sorted group-1 contract, without changing renderer/server APIs.
pub const FLAGS: u32 = 24576 | 512;
pub const PALETTE_MATERIAL: u32 = 1 << 12;
pub struct Orbit {
    yaw: f32,
    pitch: f32,
}
impl Default for Orbit {
    fn default() -> Self {
        Self {
            yaw: core::f32::consts::PI,
            pitch: 0.,
        }
    }
}
impl Orbit {
    pub fn look(&mut self, dx: f32, dy: f32) {
        self.yaw = (self.yaw + dx * 0.003) % core::f32::consts::TAU;
        self.pitch = (self.pitch - dy * 0.003).clamp(-1.35, 1.35);
    }
    pub fn position(&self, width: u32, height: u32, yfov: f32) -> [f32; 3] {
        let aspect = width.max(1) as f32 / height.max(1) as f32;
        let tan_half = libm::tanf(yfov * 0.5);
        let radius = ((12.4 / (aspect * tan_half)).max(4.) + 2.) * (2. / 3.);
        // Keep the two group previews inside the default view on wide windows.
        let radius = radius.max((GROUP_PITCH + DISPLAY_SIDE * 0.5) / tan_half + DISPLAY_SIDE * 0.5);
        [
            radius * libm::cosf(self.pitch) * libm::sinf(self.yaw),
            radius * libm::sinf(self.pitch),
            radius * libm::cosf(self.pitch) * libm::cosf(self.yaw),
        ]
    }
}
#[derive(Clone, Copy)]
struct Slot {
    asset: usize,
    key: usize,
    from: f32,
}
#[derive(Clone, Copy)]
pub struct DrawCube {
    pub cube: Cube,
    pub opacity: u32,
}
pub struct Carousel {
    sources: &'static [(&'static str, &'static [u8])],
    pages: orchard::Pages,
    palette: Vec<orchard::Asset>,
    source_count: usize,
    groups: &'static [(&'static str, &'static [usize])],
    pub group: usize,
    pub selected: usize,
    slots: Vec<Slot>,
    reveal: reveal::Reveal,
    stride: usize,
    slide_start: Option<u64>,
    pending: i32,
    held_keys: u8,
    pub orbit: Orbit,
    pub view_forward: [f32; 3],
    frame: Vec<Cube>,
    pub drawn: Vec<DrawCube>,
}
impl Carousel {
    pub fn new(
        sources: &'static [(&'static str, &'static [u8])],
        groups: &'static [(&'static str, &'static [usize])],
    ) -> Self {
        let stride = sources
            .iter()
            .map(|(_, b)| u16::from_le_bytes([b[8], b[9]]) as usize)
            .max()
            .unwrap_or(1);
        Self {
            sources,
            pages: orchard::Pages::new(sources, false),
            palette: crate::subcubes::Demo::new()
                .blocks
                .iter()
                .map(|block| {
                    let (_, scale) = block.pose();
                    let tier = crate::subcubes::SIDES
                        .iter()
                        .position(|&side| side == block.side)
                        .unwrap();
                    orchard::Asset {
                        name: crate::subcubes::NAMES[tier],
                        cubes: alloc::vec![Cube {
                            center: [0.; 3],
                            scale,
                            flags: PALETTE_MATERIAL | block.material,
                        }],
                        radius: scale * 1.74,
                    }
                })
                .collect(),
            source_count: sources.len(),
            groups,
            group: 0,
            selected: 0,
            slots: Vec::new(),
            reveal: reveal::Reveal::new(),
            stride,
            slide_start: None,
            pending: 0,
            held_keys: 0,
            orbit: Orbit::default(),
            view_forward: [0., 0., 1.],
            frame: frame_cubes(),
            drawn: Vec::new(),
        }
    }
    pub fn name(&self) -> &'static str {
        if self.group == self.groups.len() {
            return "Key7 cubes: 7 sizes x 6 materials";
        }
        self.groups[self.group].0
    }
    pub fn asset_name(&self) -> &'static str {
        self.asset(self.slots[2].asset).name
    }
    pub fn group_count(&self) -> usize {
        self.groups.len() + 1
    }
    fn asset(&self, id: usize) -> &orchard::Asset {
        if id >= self.source_count {
            &self.palette[id - self.source_count]
        } else {
            &self.pages[id]
        }
    }
    fn load(&mut self, id: usize) -> Result<(), &'static str> {
        if id < self.source_count {
            self.pages.load(id)?;
        }
        Ok(())
    }
    pub fn group_len(&self) -> usize {
        if self.group == self.groups.len() {
            return self.palette.len();
        }
        self.groups[self.group].1.len()
    }
    fn group_asset_at(&self, group: usize, index: i32) -> usize {
        if group == self.groups.len() {
            return self.source_count + index.rem_euclid(self.palette.len() as i32) as usize;
        }
        let ids = self.groups[group].1;
        ids[index.rem_euclid(ids.len() as i32) as usize]
    }
    fn asset_at(&self, offset: i32) -> usize {
        self.group_asset_at(self.group, self.selected as i32 + offset)
    }
    pub fn select_group(&mut self, group: usize) -> Result<(), &'static str> {
        self.select_item(group, 0)
    }
    pub fn select_item(&mut self, group: usize, selected: usize) -> Result<(), &'static str> {
        self.group = group % self.group_count();
        self.selected = selected % self.group_len();
        self.pending = 0;
        self.slide_start = None;
        self.slots.clear();
        for i in 0..5 {
            let asset = self.asset_at(i as i32 - 2);
            self.load(asset)?;
            self.slots.push(Slot {
                asset,
                key: i,
                from: i as f32 - 2.,
            });
        }
        // W selects the upper group, S the lower one. Both preview its first
        // asset, exactly what select_group will centre. Horizontal input never
        // rotates these slots or resets their reveal state.
        for (i, direction) in [1, -1].into_iter().enumerate() {
            let asset = self.group_asset_at(self.adjacent_group(direction), 0);
            self.load(asset)?;
            self.slots.push(Slot { asset, key: ROW_SLOTS + i, from: 0. });
        }
        self.reveal.reset();
        self.reveal.append(SLOT_COUNT * self.stride);
        Ok(())
    }
    pub fn selected_id(&self) -> usize { self.asset_at(0) }
    /// World-wheel selection is immediate and wraps only within the chosen group.
    pub fn cycle_selection(&mut self, wheel: i16) -> Result<(), &'static str> {
        let selected = (self.selected as i32 - wheel.signum() as i32).rem_euclid(self.group_len() as i32) as usize;
        self.select_item(self.group, selected)
    }
    pub fn placement(&self, point: [f32; 3], normal: [f32; 3], material_flags: u32) -> Vec<Cube> {
        let id = self.selected_id();
        if id < self.source_count {
            return crate::asset_brush::place(self.sources[id].1, point, normal);
        }
        // The existing Key7 group is also selectable. Preserve its real tier
        // and material, snapping the base and both tangents to the c1 lattice.
        let original = self.asset(id).cubes[0];
        let side = libm::roundf(original.scale * 2. / C1) as i32;
        alloc::vec![Cube {
            center: core::array::from_fn(|a| libm::roundf(point[a]/C1)*C1
                + if normal[a].abs()>0.5 { normal[a]*side as f32*C1*0.5 }
                  else { (side%2) as f32*C1*0.5 }),
            scale: original.scale,
            flags: material_flags | (original.flags & 7),
        }]
    }
    pub fn wheel(&mut self, step: i32) {
        self.pending = (self.pending + step.signum()).clamp(-32, 32);
    }
    // A/D/W/S bits, tracked outside the picker too so held movement keys don't
    // unexpectedly select an asset when entering the carousel.
    pub fn key_input(&mut self, held: u8, active: bool) -> (i32, i32) {
        let pressed = held & !self.held_keys;
        self.held_keys = held;
        if !active {
            return (0, 0);
        }
        (
            ((pressed & 2 != 0) as i32 - (pressed & 1 != 0) as i32),
            ((pressed & 4 != 0) as i32 - (pressed & 8 != 0) as i32),
        )
    }
    pub fn adjacent_group(&self, direction: i32) -> usize {
        (self.group as i32 + direction).rem_euclid(self.group_count() as i32) as usize
    }
    fn step(&mut self, direction: i32, now: u64) -> Result<(), &'static str> {
        self.selected =
            (self.selected as i32 + direction).rem_euclid(self.group_len() as i32) as usize;
        if direction > 0 {
            self.slots[..ROW_SLOTS].rotate_left(1);
        } else {
            self.slots[..ROW_SLOTS].rotate_right(1);
        }
        let incoming = if direction > 0 { 4 } else { 0 };
        let asset = self.asset_at(incoming as i32 - 2);
        self.load(asset)?;
        self.slots[incoming].asset = asset;
        let start = self.slots[incoming].key * self.stride;
        self.reveal.reset_range(start..start + self.stride);
        for (i, slot) in self.slots[..ROW_SLOTS].iter_mut().enumerate() {
            slot.from = i as f32 - 2. + direction as f32;
        }
        self.slide_start = Some(now);
        Ok(())
    }
    /// Keep the closest visible geometry when a global budget is reduced.
    /// Admission/growth remains unchanged, so raising the cap restores it.
    pub fn limit(&mut self, count: usize) {
        if self.drawn.len() <= count { return; }
        let distance = |d: &DrawCube| d.cube.center.iter().map(|v| v*v).sum::<f32>();
        self.drawn.sort_unstable_by(|a,b| distance(a).total_cmp(&distance(b)));
        self.drawn.truncate(count);
        let depth = |d: &DrawCube| (0..3).map(|a| d.cube.center[a]*self.view_forward[a]).sum::<f32>();
        self.drawn.sort_unstable_by(|a,b| depth(b).total_cmp(&depth(a)));
    }
    pub fn prepare(&mut self, now: u64) -> Result<(), &'static str> {
        if self
            .slide_start
            .is_some_and(|start| now.saturating_sub(start) >= SLIDE_MS)
        {
            self.slide_start = None;
        }
        if self.slide_start.is_none() && self.pending != 0 {
            let direction = self.pending.signum();
            self.pending -= direction;
            self.step(direction, now)?;
        }
        self.reveal.begin_frame(now, SLOT_COUNT * self.stride);
        self.drawn.clear();
        let t = self.slide_start.map_or(1., |start| {
            (now.saturating_sub(start) as f32 / SLIDE_MS as f32).min(1.)
        });
        let t = t * t * (3. - 2. * t);
        let poses: [_; SLOT_COUNT] = core::array::from_fn(|i| {
            let slot = self.slots[i];
            let (center, normalization) = if slot.asset >= self.source_count {
                ([0.; 3], 1.) // Preserve Key7's actual tier sizes, including c1.
            } else {
                asset_pose(&self.asset(slot.asset).cubes)
            };
            let offset = match i {
                5 => [0., -GROUP_PITCH, 0.],
                6 => [0., GROUP_PITCH, 0.],
                _ => [(slot.from + (i as f32 - 2. - slot.from) * t) * PITCH, 0., 0.],
            };
            (slot, center, normalization, offset)
        });
        // Interleave all seven assets under one admission budget,
        // so a dense centre asset cannot starve either group preview.
        for j in 0..self.stride {
            for i in [2usize, 5, 6, 1, 3, 0, 4] {
                let (slot, center, normalization, offset) = poses[i];
                let Some(original) = self.asset(slot.asset).cubes.get(j).copied() else {
                    continue;
                };
                let id = slot.key * self.stride + j;
                if !self.reveal.admit(id) {
                    continue;
                }
                let mut cube = original;
                cube.center = core::array::from_fn(|a| {
                    (cube.center[a] - center[a]) * normalization + offset[a]
                });
                cube.scale =
                    (cube.scale * normalization * self.reveal.growth_scale(id)).max(0.00101);
                self.drawn.push(DrawCube {
                    cube,
                    opacity: match i {
                        2 => 0,
                        1 | 3 | 5 | 6 => 1,
                        _ => 2,
                    },
                });
            }
        }
        self.reveal.end_frame();
        // A travelling half-cage is always present. Its clock and geometry are
        // independent of scrolling, group changes, and asset spawn admission.
        for original in &self.frame {
            let factor = frame_scale(original.center, now);
            if factor > 0. {
                let cube = Cube {
                    scale: (original.scale * factor).max(0.00101),
                    ..*original
                };
                self.drawn.push(DrawCube { cube, opacity: 0 });
            }
        }
        if self.drawn.len() + 1 > 8192 {
            return Err("carousel-seed-budget");
        }
        // Orbit changes depth order across slots as well as within each asset.
        let depth = |cube: &DrawCube| {
            (0..3)
                .map(|a| cube.cube.center[a] * self.view_forward[a])
                .sum::<f32>()
        };
        self.drawn
            .sort_unstable_by(|a, b| depth(b).total_cmp(&depth(a)));
        Ok(())
    }
}
fn frame_scale(center: [f32; 3], now: u64) -> f32 {
    // Clockwise position on the square XZ perimeter; vertical edges share the
    // phase of their corners, keeping the moving wireframe spatially connected.
    let half = 8. * C1;
    let [x, _, z] = center;
    let distance = if z <= -half {
        x + half
    } else if x >= half {
        2. * half + z + half
    } else if z >= half {
        4. * half + half - x
    } else {
        6. * half + half - z
    };
    let phase =
        (distance / (8. * half) + (now % FRAME_CYCLE_MS) as f32 / FRAME_CYCLE_MS as f32) % 1.;
    // Half the perimeter is active. Shared Bounce + Uniform growth over 320ms,
    // with a linear shrink at the trailing end.
    reveal::bounce_uniform(phase / 0.08)
        .min(((0.5 - phase) / 0.08).clamp(0., 1.))
}
fn asset_pose(cubes: &[Cube]) -> ([f32; 3], f32) {
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for c in cubes {
        for a in 0..3 {
            lo[a] = lo[a].min(c.center[a] - c.scale);
            hi[a] = hi[a].max(c.center[a] + c.scale);
        }
    }
    let side = (0..3).map(|a| hi[a] - lo[a]).fold(0.001, f32::max);
    (
        core::array::from_fn(|a| (lo[a] + hi[a]) * 0.5),
        DISPLAY_SIDE / side,
    )
}
fn frame_cubes() -> Vec<Cube> {
    let mut out = Vec::new();
    // Hollow cubic cage, 16 c1 across. c2 corners, alternating c1/c2 edge pieces.
    let half = 8. * C1;
    for corner in 0..8 {
        let p = core::array::from_fn(|a| if corner & (1 << a) == 0 { -half } else { half });
        out.push(Cube {
            center: p,
            scale: C1 * 0.99,
            flags: orchard::CUSTOM_RGB555 | 0x7fff,
        });
        for axis in 0..3 {
            if corner & (1 << axis) != 0 {
                continue;
            }
            let mut along = -7.;
            let mut side: f32 = 1.;
            while along < 7. {
                side = side.min(7. - along);
                let mut center = p;
                center[axis] = (along + side * 0.5) * C1;
                out.push(Cube {
                    center,
                    scale: side * C1 * 0.495,
                    flags: orchard::CUSTOM_RGB555 | 0x7fff,
                });
                along += side;
                side = 3. - side;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extra_group_contains_each_key7_cube_once_at_its_original_size_and_finish() {
        let mut c = Carousel::new(crate::ASSETS, crate::GROUPS);
        assert_eq!(c.group_count(), crate::GROUPS.len() + 1);
        c.select_group(c.group_count() - 1).unwrap();
        let reference = crate::subcubes::Demo::new();
        assert_eq!(c.group_len(), reference.blocks.len());
        assert_eq!(c.group_len(), 42);
        for (index, block) in reference.blocks.iter().enumerate() {
            assert_eq!(c.selected, index);
            let cube = c.asset(c.slots[2].asset).cubes[0];
            assert_eq!(cube.center, [0.; 3]);
            assert_eq!(cube.scale, block.pose().1);
            assert_eq!(cube.flags, PALETTE_MATERIAL | block.material);
            let now = index as u64 * 3000;
            for time in (now..now + 2000).step_by(16) {
                c.prepare(time).unwrap();
            }
            assert!(c.drawn.iter().any(|d| d.cube.center == [0.; 3]
                && (d.cube.scale - cube.scale).abs() < 1e-6
                && d.cube.flags == cube.flags
                && d.opacity == 0));
            c.wheel(1);
            c.prepare(now + 2000).unwrap();
        }
        assert_eq!(c.selected, 0);
        c.select_group(c.adjacent_group(1)).unwrap();
        assert_eq!(c.group, 0);
    }
    #[test]
    fn navigation_steps_once_per_press_only_in_carousel_and_wraps_groups() {
        let mut c = Carousel::new(crate::ASSETS, crate::GROUPS);
        c.select_group(0).unwrap();
        for (key, expected) in [(1, (-1, 0)), (2, (1, 0)), (4, (0, 1)), (8, (0, -1))] {
            assert_eq!(c.key_input(key, false), (0, 0));
            assert_eq!(c.key_input(key, true), (0, 0));
            c.key_input(0, true);
            assert_eq!(c.key_input(key, true), expected);
            assert_eq!(c.key_input(key, true), (0, 0));
            c.key_input(0, true);
        }
        assert_eq!(c.key_input(15, true), (0, 0));
        assert_eq!(c.adjacent_group(-1), c.group_count() - 1);
        c.select_group(c.group_count() - 1).unwrap();
        assert_eq!(c.adjacent_group(1), 0);
    }
    #[test]
    fn mouse_orbit_keeps_closer_radius_and_survives_group_changes() {
        let mut c = Carousel::new(crate::ASSETS, crate::GROUPS);
        let fov = core::f32::consts::FRAC_PI_3;
        for (width, height) in [(784, 441), (441, 784), (2000, 400)] {
            let old = (12.4 / (width as f32 / height as f32 * libm::tanf(fov / 2.))).max(4.) + 2.;
            let radius = |p: [f32; 3]| libm::sqrtf(p.iter().map(|x| x * x).sum());
            let expected = (old * 2. / 3.).max((GROUP_PITCH + DISPLAY_SIDE * 0.5) / libm::tanf(fov / 2.) + DISPLAY_SIDE * 0.5);
            assert!((radius(c.orbit.position(width, height, fov)) - expected).abs() < 1e-5);
            c.orbit.look(200., 150.);
            assert!((radius(c.orbit.position(width, height, fov)) - expected).abs() < 1e-5);
        }
        let before = c.orbit.position(784, 441, fov);
        c.select_group(1).unwrap();
        assert_eq!(before, c.orbit.position(784, 441, fov));
        c.orbit.look(1e6, 1e6);
        assert_eq!(c.orbit.pitch, -1.35);
        assert!(
            c.orbit
                .position(784, 441, fov)
                .iter()
                .all(|v| v.is_finite())
        );
    }
    #[test]
    fn transparent_cubes_sort_by_orbit_view_including_frame() {
        let mut c = Carousel::new(crate::ASSETS, crate::GROUPS);
        c.select_group(0).unwrap();
        for now in (0..4000).step_by(16) {
            c.prepare(now).unwrap();
        }
        for forward in [[1., 0., 0.], [0., 0., -1.], [0.6, 0.3, 0.7]] {
            c.view_forward = forward;
            c.prepare(4000).unwrap();
            let depth = |d: &DrawCube| (0..3).map(|a| d.cube.center[a] * forward[a]).sum::<f32>();
            assert!(
                c.drawn
                    .windows(2)
                    .all(|pair| depth(&pair[0]) >= depth(&pair[1]))
            );
        }
    }
    #[test]
    fn five_slots_wrap_every_exported_group_in_both_directions() {
        let mut c = Carousel::new(crate::ASSETS, crate::GROUPS);
        for group in 0..c.group_count() {
            c.select_group(group).unwrap();
            assert_eq!(c.slots.len(), SLOT_COUNT);
            assert_eq!(c.selected, 0);
            for direction in [1, -1] {
                for n in 0..c.group_len() * 2 {
                    let previous = c.selected;
                    c.wheel(direction);
                    c.prepare(n as u64 * 1000 + if direction < 0 { 100000 } else { 0 })
                        .unwrap();
                    assert_eq!(
                        c.selected,
                        (previous as i32 + direction).rem_euclid(c.group_len() as i32) as usize
                    );
                    assert_eq!(c.slots.len(), SLOT_COUNT);
                    for i in 0..5 {
                        assert_eq!(c.slots[i].asset, c.asset_at(i as i32 - 2));
                    }
                }
                c.slide_start = None;
            }
        }
    }
    #[test]
    fn shared_spawn_budget_reaches_all_seven_slots_then_exact_geometry_and_opacity() {
        let mut c = Carousel::new(crate::ASSETS, crate::GROUPS);
        for group in 0..c.group_count() {
            c.select_group(group).unwrap();
            c.prepare(0).unwrap();
            assert_eq!(asset_count(&c), 0);
            c.prepare(reveal::DELAY_MS - 1).unwrap();
            assert_eq!(asset_count(&c), 0);
            c.prepare(reveal::DELAY_MS).unwrap();
            let available: usize = c.slots.iter().map(|s| c.asset(s.asset).cubes.len()).sum();
            assert_eq!(
                asset_count(&c),
                available.min(reveal::MAX_STARTS_PER_FRAME as usize)
            );
            for slot in -2..=2 {
                assert!(
                    c.drawn
                        .iter()
                        .any(|d| (d.cube.center[0] - slot as f32 * PITCH).abs()
                            < DISPLAY_SIDE * 0.5 + 0.01),
                    "slot {slot} starved"
                );
            }
            for now in (reveal::DELAY_MS + 16..12000).step_by(16) {
                c.prepare(now).unwrap();
            }
            let expected = c
                .slots
                .iter()
                .map(|s| c.asset(s.asset).cubes.len())
                .sum::<usize>()
                + frame_drawn(&c).len();
            assert_eq!(c.drawn.len(), expected);
            assert!(expected < 8192);
            for (opacity, slots) in [(0, vec![2]), (1, vec![1, 3, 5, 6]), (2, vec![0, 4])] {
                let expected = slots
                    .iter()
                    .map(|&i| c.asset(c.slots[i].asset).cubes.len())
                    .sum::<usize>()
                    + if opacity == 0 {
                        frame_drawn(&c).len()
                    } else {
                        0
                    };
                assert_eq!(
                    c.drawn.iter().filter(|d| d.opacity == opacity).count(),
                    expected
                );
            }
            assert!(
                c.drawn
                    .windows(2)
                    .all(|d| d[0].cube.center[2] >= d[1].cube.center[2])
            );
        }
    }
    #[test]
    fn group_previews_match_w_s_destinations_and_ignore_horizontal_input() {
        let mut c = Carousel::new(crate::ASSETS, crate::GROUPS);
        for group in 0..c.group_count() {
            c.select_group(group).unwrap();
            for now in (0..12000).step_by(16) { c.prepare(now).unwrap(); }
            let previews = [c.slots[5].asset, c.slots[6].asset];
            let snapshot = |c: &Carousel| {
                let mut out: Vec<_> = c.drawn.iter().filter(|d| d.cube.center[1].abs() > GROUP_PITCH - DISPLAY_SIDE * 0.5 - 0.01)
                    .map(|d| (d.cube.center, d.cube.scale, d.cube.flags, d.opacity)).collect();
                out.sort_by(|a,b| a.0[0].total_cmp(&b.0[0]).then(a.0[1].total_cmp(&b.0[1])).then(a.0[2].total_cmp(&b.0[2])));
                out
            };
            let original = snapshot(&c);
            assert_eq!(original.len(), previews.iter().map(|&id| c.asset(id).cubes.len()).sum::<usize>());
            assert!(original.iter().all(|d| d.3 == 1)); // Existing 50% alpha class.
            for (i, direction) in [1, -1].into_iter().enumerate() {
                let mut destination = Carousel::new(crate::ASSETS, crate::GROUPS);
                destination.select_group(c.adjacent_group(direction)).unwrap();
                assert_eq!(previews[i], destination.slots[2].asset);
            }
            let mut now = 12000;
            // Wheel and both A/D directions use the same pending horizontal step.
            for key in [0, 1, 2] {
                let direction = if key == 0 { 1 } else {
                    c.key_input(0, true); c.key_input(key, true).0
                };
                c.wheel(direction);
                for _ in 0..30 {
                    c.prepare(now).unwrap(); now += 16;
                    assert_eq!([c.slots[5].asset, c.slots[6].asset], previews);
                    assert_eq!(snapshot(&c), original, "group previews moved or restarted growth");
                }
            }
            // Every possible horizontal selection fits the retained seed budget,
            // even counting the entire cage instead of its half-visible portion.
            for selected in 0..c.group_len() {
                c.selected = selected;
                let row: usize = (-2..=2).map(|offset| {
                    let id = c.asset_at(offset); c.load(id).unwrap(); c.asset(id).cubes.len()
                }).sum();
                assert!(row + original.len() + c.frame.len() + 1 <= 8192);
            }
        }
    }
    #[test]
    fn runtime_cap_limits_geometry_and_restores_it_without_resetting_growth() {
        let mut c = Carousel::new(crate::ASSETS, crate::GROUPS);
        for group in 0..c.group_count() {
            c.select_group(group).unwrap();
            for now in (0..20000).step_by(16) { c.prepare(now).unwrap(); }
            c.prepare(20000).unwrap();
            let baseline=c.drawn.len();
            for cap in [768,1024,2048,3840,8191] {
                c.limit(cap);
                assert!(c.drawn.len()<=cap);
                assert!(c.drawn.windows(2).all(|d|d[0].cube.center[2]>=d[1].cube.center[2]));
                c.prepare(20000).unwrap();
                assert_eq!(c.drawn.len(),baseline);
            }
        }
    }
    #[test]
    fn placement_selection_stays_in_group_and_reopens_at_last_item() {
        let mut c=Carousel::new(crate::ASSETS,crate::GROUPS);
        for group in 0..c.group_count() {
            c.select_item(group, c.group_len()).unwrap();
            for direction in [-1,1] {
                for _ in 0..c.group_len()+1 {
                    let old=c.selected;
                    c.cycle_selection(direction).unwrap();
                    assert_eq!(c.group,group);
                    assert_eq!(c.selected,(old as i32-direction as i32).rem_euclid(c.group_len() as i32) as usize);
                    let id=c.selected_id(); let selected=c.selected;
                    c.select_item(c.group,c.selected).unwrap();
                    assert_eq!((c.selected_id(),c.selected),(id,selected));
                    for axis in 0..3 { for sign in [-1.,1.] {
                        let mut normal=[0.;3];normal[axis]=sign;
                        let placed=c.placement([0.;3],normal,24576);
                        assert_eq!(placed.len(),c.asset(id).cubes.len());
                        assert!(placed.iter().all(|cube|cube.center[axis]*sign-cube.scale>=-0.00001));
                        if id>=c.source_count {
                            assert_eq!(placed[0].flags,24576|(c.asset(id).cubes[0].flags&7));
                            assert_eq!(placed[0].scale,c.asset(id).cubes[0].scale);
                        }
                    }}
                }
            }
        }
    }
    fn frame_drawn(c: &Carousel) -> Vec<([f32; 3], f32)> {
        c.drawn
            .iter()
            .filter(|d| c.frame.iter().any(|f| f.center == d.cube.center))
            .map(|d| (d.cube.center, d.cube.scale))
            .collect()
    }
    fn asset_count(c: &Carousel) -> usize {
        c.drawn.len() - frame_drawn(c).len()
    }
    #[test]
    fn frame_stays_present_and_scroll_and_group_changes_do_not_reset_it() {
        let mut stationary = Carousel::new(crate::ASSETS, crate::GROUPS);
        let mut scrolling = Carousel::new(crate::ASSETS, crate::GROUPS);
        stationary.select_group(0).unwrap();
        scrolling.select_group(0).unwrap();
        assert!(
            stationary
                .frame
                .iter()
                .all(|cube| [C1 * 0.495, C1 * 0.99].contains(&cube.scale))
        );
        for now in (0..8000).step_by(40) {
            if now % 400 == 0 {
                scrolling.wheel(if now < 4000 { 1 } else { -1 });
            }
            if now == 4000 {
                scrolling.select_group(1).unwrap();
            }
            stationary.prepare(now).unwrap();
            scrolling.prepare(now).unwrap();
            let a = frame_drawn(&stationary);
            let b = frame_drawn(&scrolling);
            assert!(!a.is_empty());
            assert_eq!(a.len(), b.len());
            // Equal-depth sorting may reorder cubes as assets come and go.
            assert!(a.iter().all(|cube| b.contains(cube)));
        }
    }
    #[test]
    fn frame_half_duty_cycle_bounce_growth_and_linear_shrink_loop() {
        let cubes = frame_cubes();
        for cube in &cubes {
            let showing = (0..FRAME_CYCLE_MS)
                .step_by(10)
                .filter(|&now| frame_scale(cube.center, now) > 0.)
                .count();
            assert!((showing as f32 / 400. - 0.5).abs() < 0.01);
            for now in [0, 160, 999, 1840, 3333] {
                assert_eq!(
                    frame_scale(cube.center, now),
                    frame_scale(cube.center, now + FRAME_CYCLE_MS)
                );
            }
        }
        let corner = [-8. * C1; 3];
        for (now, expected) in [
            (0, 0.),
            (160, reveal::bounce_uniform(0.5)),
            (320, 1.),
            (1000, 1.),
            (1840, 0.5),
            (2000, 0.),
            (3000, 0.),
        ] {
            assert!((frame_scale(corner, now) - expected).abs() < 1e-6);
        }
        // The leading end must include the same bounce dip as asset growth.
        assert!(frame_scale(corner, 242) < frame_scale(corner, 208));
        assert!(frame_scale(corner, 298) > frame_scale(corner, 242));
    }
}
