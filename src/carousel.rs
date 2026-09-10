//! Five live asset instances. Group metadata is exported from AssetShowcase.
use crate::{
    orchard::{self, Cube},
    reveal,
    subcubes::C1,
};
use alloc::vec::Vec;
pub const SLIDE_MS: u64 = 333;
pub const FRAME_SHRINK_MS: u64 = 166;
pub const PITCH: f32 = 3.6;
pub const DISPLAY_SIDE: f32 = 2.4;
// RGB palette index (9 bits), translucency marker, opacity class, showcase class.
// Uses the existing sorted group-1 contract, without changing renderer/server APIs.
pub const FLAGS: u32 = 24576 | 512;
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
    pages: orchard::Pages,
    groups: &'static [(&'static str, &'static [usize])],
    pub group: usize,
    pub selected: usize,
    slots: Vec<Slot>,
    reveal: reveal::Reveal,
    stride: usize,
    slide_start: Option<u64>,
    pending: i32,
    frame: Vec<Cube>,
    frame_shrink: Option<u64>,
    frame_previous: Vec<Cube>,
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
            pages: orchard::Pages::new(sources, false),
            groups,
            group: 0,
            selected: 0,
            slots: Vec::new(),
            reveal: reveal::Reveal::new(),
            stride,
            slide_start: None,
            pending: 0,
            frame: frame_cubes(),
            frame_shrink: None,
            frame_previous: Vec::new(),
            drawn: Vec::new(),
        }
    }
    pub fn name(&self) -> &'static str {
        self.groups[self.group].0
    }
    pub fn asset_name(&self) -> &'static str {
        self.pages[self.slots[2].asset].name
    }
    pub fn group_len(&self) -> usize {
        self.groups[self.group].1.len()
    }
    fn asset_at(&self, offset: i32) -> usize {
        let ids = self.groups[self.group].1;
        ids[(self.selected as i32 + offset).rem_euclid(ids.len() as i32) as usize]
    }
    pub fn select_group(&mut self, group: usize) -> Result<(), &'static str> {
        self.group = group % self.groups.len();
        self.selected = 0;
        self.pending = 0;
        self.slide_start = None;
        self.slots.clear();
        for i in 0..5 {
            let asset = self.asset_at(i as i32 - 2);
            self.pages.load(asset)?;
            self.slots.push(Slot {
                asset,
                key: i,
                from: i as f32 - 2.,
            });
        }
        self.reveal.reset();
        self.reveal.append(5 * self.stride + self.frame.len());
        self.frame_previous.clear();
        self.frame_shrink = None;
        Ok(())
    }
    pub fn wheel(&mut self, step: i32) {
        self.pending = (self.pending + step.signum()).clamp(-32, 32);
    }
    fn step(&mut self, direction: i32, now: u64) -> Result<(), &'static str> {
        self.selected =
            (self.selected as i32 + direction).rem_euclid(self.group_len() as i32) as usize;
        if direction > 0 {
            self.slots.rotate_left(1);
        } else {
            self.slots.rotate_right(1);
        }
        let incoming = if direction > 0 { 4 } else { 0 };
        let asset = self.asset_at(incoming as i32 - 2);
        self.pages.load(asset)?;
        self.slots[incoming].asset = asset;
        let start = self.slots[incoming].key * self.stride;
        self.reveal.reset_range(start..start + self.stride);
        for (i, slot) in self.slots.iter_mut().enumerate() {
            slot.from = i as f32 - 2. + direction as f32;
        }
        self.slide_start = Some(now);
        self.frame_shrink = Some(now);
        Ok(())
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
        let frame_base = 5 * self.stride;
        if self
            .frame_shrink
            .is_some_and(|start| now.saturating_sub(start) >= FRAME_SHRINK_MS)
        {
            self.frame_shrink = None;
            self.frame_previous.clear();
            self.reveal
                .reset_range(frame_base..frame_base + self.frame.len());
        }
        self.reveal.begin_frame(now, frame_base + self.frame.len());
        self.drawn.clear();
        if let Some(start) = self.frame_shrink {
            let factor = 1. - now.saturating_sub(start) as f32 / FRAME_SHRINK_MS as f32;
            for cube in &self.frame_previous {
                self.drawn.push(DrawCube {
                    cube: Cube {
                        scale: (cube.scale * factor).max(0.00101),
                        ..*cube
                    },
                    opacity: 0,
                });
            }
        } else {
            self.frame_previous.clear();
        }
        let t = self.slide_start.map_or(1., |start| {
            (now.saturating_sub(start) as f32 / SLIDE_MS as f32).min(1.)
        });
        let t = t * t * (3. - 2. * t);
        let poses: [_; 5] = core::array::from_fn(|i| {
            let slot = self.slots[i];
            let (center, normalization) = asset_pose(&self.pages[slot.asset].cubes);
            let x = (slot.from + (i as f32 - 2. - slot.from) * t) * PITCH;
            (slot, center, normalization, x)
        });
        // Interleave all five assets and the frame under one admission budget,
        // so a dense centre asset cannot starve the other four slots.
        for j in 0..self.stride.max(self.frame.len()) {
            for i in [2usize, 1, 3, 0, 4] {
                let (slot, center, normalization, x) = poses[i];
                let Some(original) = self.pages[slot.asset].cubes.get(j) else {
                    continue;
                };
                let id = slot.key * self.stride + j;
                if !self.reveal.admit(id) {
                    continue;
                }
                let mut cube = *original;
                cube.center = core::array::from_fn(|a| {
                    (cube.center[a] - center[a]) * normalization + if a == 0 { x } else { 0. }
                });
                cube.scale =
                    (cube.scale * normalization * self.reveal.growth_scale(id)).max(0.00101);
                self.drawn.push(DrawCube {
                    cube,
                    opacity: match i {
                        2 => 0,
                        1 | 3 => 1,
                        _ => 2,
                    },
                });
            }
            if self.frame_shrink.is_none()
                && j < self.frame.len()
                && self.reveal.admit(frame_base + j)
            {
                let cube = Cube {
                    scale: (self.frame[j].scale * self.reveal.linear_scale(frame_base + j))
                        .max(0.00101),
                    ..self.frame[j]
                };
                self.frame_previous.push(cube);
                self.drawn.push(DrawCube { cube, opacity: 0 });
            }
        }
        self.reveal.end_frame();
        // Fixed camera looks down +Z. Blend every cube back-to-front, including frame.
        self.drawn
            .sort_unstable_by(|a, b| b.cube.center[2].total_cmp(&a.cube.center[2]));
        Ok(())
    }
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
            let mut side = 1.;
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
