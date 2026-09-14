//! Key7 uses an exact twelfth-c1 lattice and block-local subdivision.
use alloc::vec::Vec;

pub const C1: f32 = 0.2;
/// Existing world/VFX contract, in c1 units.
pub const SIDES: [i32; 7] = [1, 2, 3, 4, 6, 8, 12];
pub const TICKS_PER_C1: i32 = 12;
pub const UNIT: f32 = C1 / TICKS_PER_C1 as f32;
/// Display tiers, in twelfth-c1 ticks; R1/2 is now one third of c1.
pub const MINING_SIDES: [i32; 10] = [3, 4, 6, 12, 24, 36, 48, 72, 96, 192];
pub const MINING_NAMES: [&str; 10] = ["C1/4", "R1/2", "C1/2", "c1", "c2", "r1", "c3", "r2", "r3", "c4"];
/// Maximum target sizes and removal sizes, in twelfth-c1 ticks.
pub const TOOLS: [i32; 6] = [768, 192, 48, 24, 12, 12];
pub const CUT_SIDES: [i32; 6] = [192, 48, 12, 6, 4, 3];
pub const TOOL_NAMES: [&str; 6] = ["64 c1 -> c4 (16 c1)", "c4 (16 c1) -> c3 (4 c1)", "c3 (4 c1) -> c1", "c2 / c1 -> C1/2", "c1 -> R1/2 (1/3)", "c1 -> C1/4"];
pub const NO_TOOL: usize = TOOLS.len();
pub const MINING_BASE_SIDE: i32 = 64 * TICKS_PER_C1;

/// Camera regression fixture; production world geometry comes from CubeSrv.
#[cfg(test)]
pub fn empty_world_blocks() -> Vec<Block> {
    let mut blocks = Vec::with_capacity(27);
    for x in -1..=1 { for y in -1..=1 { for z in -1..=1 {
        blocks.push(Block {
            min: [x,y,z].map(|v| v * MINING_BASE_SIDE - MINING_BASE_SIDE/2),
            side: MINING_BASE_SIDE,
            material: 0,
        });
    }}}
    blocks
}

pub fn walkable(side: i32) -> bool {
    matches!(side, 4 | 6 | 8 | 12 | 16 | 64)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    /// Coordinates and side length in twelfth-c1 ticks.
    pub min: [i32; 3],
    pub side: i32,
    pub material: u32,
}
impl Block {
    pub fn pose(self) -> ([f32; 3], f32) {
        (
            self.min.map(|v| (v as f32 + self.side as f32 * 0.5) * UNIT),
            self.side as f32 * UNIT * 0.5 - C1 * 0.005,
        )
    }
    pub fn walkable(self) -> bool {
        self.side % TICKS_PER_C1 == 0 && walkable(self.side / TICKS_PER_C1)
    }
    fn children(self) -> Vec<Block> {
        let side = if matches!(self.side, 192 | 96 | 48 | 24 | 12) {
            Some(self.side / 2)
        } else if TOOLS.contains(&self.side) {
            Some(self.side / 4)
        } else {
            MINING_SIDES
                .into_iter()
                .rev()
                .find(|&s| s < self.side && self.side % s == 0 && matches!(self.side / s, 2 | 3))
        };
        let Some(side) = side else {
            return alloc::vec![self];
        };
        let n = self.side / side;
        let mut blocks = Vec::with_capacity((n * n * n) as usize);
        for x in 0..n {
            for y in 0..n {
                for z in 0..n {
                    blocks.push(Block {
                        min: core::array::from_fn(|a| self.min[a] + [x, y, z][a] * side),
                        side,
                        material: self.material,
                    });
                }
            }
        }
        blocks
    }
    /// Split only the branch containing the cut, retaining intact siblings.
    fn without(self, cut: Block, out: &mut Vec<Block>) {
        if self == cut { return; }
        for child in self.children() {
            if (0..3).all(|a| cut.min[a] >= child.min[a]
                && cut.min[a] + cut.side <= child.min[a] + child.side) {
                child.without(cut, out);
            } else {
                out.push(child);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MiningTarget {
    pub parent: Block,
    pub cut: Block,
}

/// A click is armed on press and committed on release; a hold repeats on frames.
#[derive(Default)]
pub struct MiningGesture {
    pressed: Option<(MiningTarget, u64)>,
    automatic: bool,
    next_mine: u64,
}
impl MiningGesture {
    pub fn cancel(&mut self) { *self = Self::default(); }
    pub fn press(&mut self, target: Option<MiningTarget>, now: u64) {
        self.cancel();
        self.pressed = target.map(|t| (t, now));
    }
    pub fn observe(&mut self, target: Option<MiningTarget>) {
        if !self.automatic && self.pressed.is_some_and(|(t, _)| Some(t) != target) {
            self.cancel();
        }
    }
    pub fn release(&mut self, target: Option<MiningTarget>) -> Option<MiningTarget> {
        let cut = self.pressed.and_then(|(t, _)|
            (!self.automatic && Some(t) == target).then_some(t));
        self.cancel();
        cut
    }
    pub fn tick(&mut self, target: Option<MiningTarget>, now: u64) -> Option<MiningTarget> {
        self.observe(target);
        let (_, started) = self.pressed?;
        if !self.automatic {
            if now.saturating_sub(started) < 2000 { return None; }
            self.automatic = true;
            self.next_mine = now;
        }
        if now < self.next_mine { return None; }
        // No catch-up bursts after a slow frame or a period without a target.
        self.next_mine = now.saturating_add(100);
        target
    }
}

pub struct Demo {
    pub blocks: Vec<Block>,
    pub tool: usize,
}
impl Demo {
    pub fn new() -> Self {
        let mut blocks = Vec::new();
        for (row, side) in MINING_SIDES.into_iter().enumerate() {
            for color in 0..6 {
                blocks.push(Block {
                    min: [(color * 18 - 51) * TICKS_PER_C1, 0, (row as i32 * 18 - 75) * TICKS_PER_C1],
                    side,
                    material: color as u32,
                });
            }
        }
        for (material, x) in (0..6).zip(-3..3) {
            for z in [-1, 0] {
                blocks.push(Block {
                    min: [
                        x * MINING_BASE_SIDE,
                        -MINING_BASE_SIDE,
                        z * MINING_BASE_SIDE,
                    ],
                    side: MINING_BASE_SIDE,
                    material,
                });
            }
        }
        Self {
            blocks,
            tool: NO_TOOL,
        }
    }
    pub fn cycle(&mut self, wheel: i32) {
        self.tool =
            (self.tool as i32 + wheel.signum()).rem_euclid((TOOLS.len() + 1) as i32) as usize;
    }
    pub fn tool_side(&self) -> Option<i32> {
        TOOLS.get(self.tool).copied()
    }
    pub fn cut_side(&self) -> Option<i32> {
        CUT_SIDES.get(self.tool).copied()
    }
    pub fn tool_name(&self) -> &'static str {
        TOOL_NAMES.get(self.tool).copied().unwrap_or("none")
    }
    fn accepts(&self, side: i32) -> bool {
        let Some(selected) = self.tool_side() else { return false; };
        side == selected
            || (self.tool < 4 && self.cut_side() == Some(side))
            || (matches!(self.tool, 1 | 2 | 3) && side == selected / 2)
    }

    pub fn target_details(&self, origin: [f32; 3], direction: [f32; 3]) -> Option<MiningTarget> {
        self.tool_side()?;
        let origin = origin.map(|x| x / UNIT);
        let mut best = f32::INFINITY;
        let mut hit = None;
        // Resolve the first physical surface even when its size is disabled.
        for &block in &self.blocks {
            let mut near = f32::NEG_INFINITY;
            let mut far = f32::INFINITY;
            for a in 0..3 {
                let lo = block.min[a] as f32;
                let hi = lo + block.side as f32;
                if direction[a].abs() < 1e-7 {
                    if origin[a] < lo || origin[a] > hi {
                        far = f32::NEG_INFINITY;
                        break;
                    }
                } else {
                    let t0 = (lo - origin[a]) / direction[a];
                    let t1 = (hi - origin[a]) / direction[a];
                    near = near.max(t0.min(t1));
                    far = far.min(t0.max(t1));
                }
            }
            if far < 0. || near > far {
                continue;
            }
            let distance = near.max(0.);
            if distance >= best {
                continue;
            }
            best = distance;
            hit = if near < 0. { None } else { Some((block, near)) };
        }
        let (parent, distance) = hit?;
        if !self.accepts(parent.side) { return None; }
        let side = self.cut_side()?;
        if parent.side == side {
            return Some(MiningTarget { parent, cut: parent });
        }
        let min = core::array::from_fn(|a| {
            let p = origin[a] + direction[a] * (distance + 0.0001) - parent.min[a] as f32;
            let cell = ((p / side as f32) as i32).clamp(0, parent.side / side - 1);
            parent.min[a] + cell * side
        });
        Some(MiningTarget {
            parent,
            cut: Block {
                min,
                side,
                material: parent.material,
            },
        })
    }
    /// Opaque preview replaces just the parent; occupancy stays intact.
    pub fn preview_blocks(&self, target: Option<MiningTarget>) -> Vec<Block> {
        let mut out = Vec::with_capacity(self.blocks.len() + 63);
        for &block in &self.blocks {
            if let Some(t) = target.filter(|t| t.parent == block) {
                if t.cut != block {
                    if block.side == TICKS_PER_C1 {
                        // Tools 4–6 split c1 directly into their own uniform tier.
                        let n = block.side / t.cut.side;
                        for x in 0..n { for y in 0..n { for z in 0..n {
                            let child = Block {
                                min: core::array::from_fn(|a| block.min[a] + [x,y,z][a] * t.cut.side),
                                side: t.cut.side, material: block.material,
                            };
                            if child != t.cut { out.push(child); }
                        }}}
                    } else { block.without(t.cut, &mut out); }
                }
            } else {
                out.push(block);
            }
        }
        out
    }
    pub fn mine(&mut self, target: MiningTarget) {
        // Reject stale previews or targets from a different wheel selection.
        if !self.accepts(target.parent.side) { return; }
        let Some(cut_side) = self.cut_side() else { return; };
        let valid = if target.parent.side == cut_side {
            target.cut == target.parent
        } else {
            target.cut.side == cut_side
                && target.cut.material == target.parent.material
                && (0..3).all(|a| {
                    let offset = target.cut.min[a] - target.parent.min[a];
                    offset >= 0 && offset + target.cut.side <= target.parent.side
                        && offset % target.cut.side == 0
                })
        };
        if !valid || !self.blocks.contains(&target.parent) {
            return;
        }
        self.blocks = self.preview_blocks(Some(target));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn gesture_target(x: i32) -> MiningTarget {
        let parent = Block { min: [x,0,0], side: 12, material: 0 };
        MiningTarget { parent, cut: parent }
    }
    #[test]
    fn click_commits_only_on_release_and_aiming_away_cancels_it() {
        let a = Some(gesture_target(0));
        let b = Some(gesture_target(6));
        let mut g = MiningGesture::default();
        g.press(a, 100);
        assert!(g.tick(a, 101).is_none());
        assert_eq!(g.release(a), a);
        assert!(g.release(a).is_none());
        for away in [None, b] {
            g.press(a, 100);
            g.observe(away);
            assert!(g.release(a).is_none());
            assert!(g.tick(a, 3000).is_none());
        }
        g.press(None, 100);
        assert!(g.release(a).is_none());
        g.press(a, 100);
        assert!(g.release(b).is_none());
    }
    #[test]
    fn hold_starts_at_two_seconds_and_tracks_targets_without_release_extra_cut() {
        let a = Some(gesture_target(0));
        let b = Some(gesture_target(6));
        let mut g = MiningGesture::default();
        g.press(a, 100);
        assert!(g.tick(a, 2099).is_none());
        assert_eq!(g.tick(a, 2100), a);
        assert!(g.tick(b, 2199).is_none());
        assert_eq!(g.tick(b, 2200), b);
        assert!(g.tick(None, 2300).is_none());
        assert_eq!(g.tick(a, 2400), a);
        assert!(g.release(a).is_none());
        assert!(g.tick(a, 3000).is_none());
    }
    #[test]
    fn cancelled_holds_and_slow_frames_never_burst() {
        let a = Some(gesture_target(0));
        let mut g = MiningGesture::default();
        g.press(a, 0);
        assert_eq!(g.tick(a, 5000), a);
        assert!(g.tick(a, 5000).is_none());
        assert!(g.tick(a, 5099).is_none());
        assert_eq!(g.tick(a, 5100), a);
        g.cancel();
        assert!(g.tick(a, 10000).is_none());
        assert!(g.release(a).is_none());
    }
    #[test]
    fn wheel_visits_every_size_and_off_in_both_directions() {
        let mut d = Demo::new();
        for side in TOOLS {
            d.cycle(1);
            assert_eq!(d.tool_side(), Some(side));
        }
        d.cycle(1);
        assert_eq!(d.tool, NO_TOOL);
        for side in TOOLS.into_iter().rev() {
            d.cycle(-1);
            assert_eq!(d.tool_side(), Some(side));
        }
        d.cycle(-1);
        assert_eq!(d.tool, NO_TOOL);
    }
    #[test]
    fn subdivision_preview_and_commit_agree_on_every_face_and_size() {
        for (tool, side) in TOOLS.into_iter().enumerate() {
            let parent = Block {
                min: [-side; 3],
                side,
                material: 5,
            };
            for axis in 0..3 {
                for sign in [-1., 1.] {
                    let mut d = Demo {
                        blocks: alloc::vec![parent],
                        tool,
                    };
                    let mut origin = [-side as f32 * 0.4 * UNIT; 3];
                    origin[axis] = if sign > 0. {
                        -2. * side as f32 * UNIT
                    } else {
                        side as f32 * UNIT
                    };
                    let mut direction = [0.; 3];
                    direction[axis] = sign;
                    let target = d.target_details(origin, direction).unwrap();
                    let preview = d.preview_blocks(Some(target));
                    assert_eq!(d.blocks, [parent]);
                    assert_eq!(
                        preview.iter().map(|b| b.side.pow(3) as i64).sum::<i64>()
                            + target.cut.side.pow(3) as i64,
                        side.pow(3) as i64
                    );
                    assert!(preview.iter().all(|b| b.material == 5 && *b != target.cut));
                    let n = side / target.cut.side;
                    assert_eq!(n, [4,4,4,4,3,4][tool]);
                    assert_eq!(preview.len(), [63, 14, 14, 14, 26, 63][tool]);
                    d.mine(target);
                    assert_eq!(d.blocks, preview);
                    let after = d.blocks.clone();
                    d.mine(target);
                    assert_eq!(d.blocks, after);
                }
            }
        }
    }
    #[test]
    fn c1_split_tools_keep_uniform_children_and_enforce_target_sizes() {
        for material in 0..6 { for tool in 3..6 {
            let parent = Block { min: [-12;3], side: 12, material };
            let mut d = Demo { blocks: alloc::vec![parent], tool };
            let cut_side = CUT_SIDES[tool];
            let n = 12 / cut_side;
            for axis in 0..3 { for u in 0..n { for v in 0..n {
                let mut origin = [-12. + 0.5;3];
                origin[axis] = -13.;
                origin[(axis+1)%3] += (u*cut_side) as f32;
                origin[(axis+2)%3] += (v*cut_side) as f32;
                let mut direction = [0.;3]; direction[axis] = 1.;
                let target = d.target_details(origin.map(|x|x*UNIT), direction).unwrap();
                let preview = d.preview_blocks(Some(target));
                assert_eq!(preview.len(), (n*n*n-1) as usize);
                assert!(preview.iter().all(|b| b.side == cut_side && b.material == material));
                d.mine(target);
                assert_eq!(d.blocks, preview);
                d.blocks = alloc::vec![parent];
                d.tool = if tool == 5 { 3 } else { tool+1 };
                d.mine(target);
                assert_eq!(d.blocks, [parent]); // stale wheel target
                d.tool = tool;
            }}}
            for side in MINING_SIDES.into_iter().chain([MINING_BASE_SIDE]) {
                let block = Block { min: [0;3], side, material };
                d.blocks = alloc::vec![block];
                let hit = d.target_details([UNIT*0.5,UNIT*0.5,-UNIT], [0.,0.,1.]);
                assert_eq!(hit.is_some(), side == 12 || (tool == 3 && matches!(side, 6 | 24)));
                if tool == 3 && side == 6 {
                    d.mine(hit.unwrap());
                    assert!(d.blocks.is_empty());
                } else if side != 12 {
                    // Even a fabricated whole-piece deletion must be rejected.
                    d.mine(MiningTarget { parent:block, cut:block });
                    assert_eq!(d.blocks, [block]);
                }
            }
        }}
    }
    #[test]
    fn fourth_tool_c2_preserves_seven_c1_chunks_and_seven_halves() {
        for material in 0..6 {
            let parent = Block { min: [-24;3], side: 24, material };
            for axis in 0..3 { for sign in [-1.,1.] { for u in 0..4 { for v in 0..4 {
                let mut d = Demo { blocks: alloc::vec![parent], tool: 3 };
                let mut origin = [-23.5;3];
                origin[axis] = if sign > 0. { -25. } else { 1. };
                origin[(axis+1)%3] += (u*6) as f32;
                origin[(axis+2)%3] += (v*6) as f32;
                let mut direction = [0.;3]; direction[axis] = sign;
                let target = d.target_details(origin.map(|x|x*UNIT), direction).unwrap();
                assert_eq!(target.cut.side, 6);
                let preview = d.preview_blocks(Some(target));
                assert_eq!(preview.len(), 14);
                assert_eq!(preview.iter().filter(|b| b.side == 12).count(), 7);
                assert_eq!(preview.iter().filter(|b| b.side == 6).count(), 7);
                assert!(preview.iter().all(|b| b.material == material));
                assert_eq!(preview.iter().map(|b| b.side.pow(3)).sum::<i32>(), 24i32.pow(3)-6i32.pow(3));
                assert_eq!(d.blocks, [parent]);
                d.mine(target);
                assert_eq!(d.blocks, preview);
            }}}}
        }
    }
    #[test]
    fn third_tool_removes_c1_and_rejects_larger_than_c3_in_all_colors() {
        for material in 0..6 {
            let parent = Block { min: [0; 3], side: 48, material };
            let mut d = Demo { blocks: alloc::vec![parent], tool: 2 };
            let origin = [UNIT, UNIT, -UNIT];
            let direction = [0., 0., 1.];
            let target = d.target_details(origin, direction).unwrap();
            assert_eq!(target.cut.side, TICKS_PER_C1);
            let preview = d.preview_blocks(Some(target));
            assert_eq!(preview.len(), 14);
            assert_eq!(preview.iter().filter(|b| b.side == 24).count(), 7);
            assert_eq!(preview.iter().filter(|b| b.side == 12).count(), 7);
            assert_eq!(d.blocks, [parent]);
            d.mine(target);
            assert_eq!(d.blocks, preview);
            assert!(d.blocks.iter().all(|b| matches!(b.side, 12 | 24) && b.material == material));
            let child = d.target_details(origin, direction).unwrap();
            assert_eq!(child.cut, child.parent);
            d.mine(child);
            assert_eq!(d.blocks.len(), 13);
            let c2 = d.target_details(origin, direction).unwrap();
            assert_eq!(c2.parent.side, 24);
            assert_eq!(c2.cut.side, 12);
            let before_volume = d.blocks.iter().map(|b| b.side.pow(3)).sum::<i32>();
            d.mine(c2);
            assert_eq!(d.blocks.len(), 19);
            assert_eq!(before_volume - d.blocks.iter().map(|b| b.side.pow(3)).sum::<i32>(), 12i32.pow(3));
            d.blocks = alloc::vec![Block { min: [0; 3], side: 24, material }];
            let c2 = d.target_details(origin, direction).unwrap();
            d.mine(c2);
            assert_eq!(d.blocks.len(), 7);
            assert!(d.blocks.iter().all(|b| b.side == 12 && b.material == material));
            d.blocks = alloc::vec![Block { min: [0; 3], side: 12, material }];
            let child = d.target_details(origin, direction).unwrap();
            d.mine(child);
            assert!(d.blocks.is_empty());
            for side in [72, 96, 192, MINING_BASE_SIDE] {
                d.blocks = alloc::vec![Block { min: [0; 3], side, material }];
                assert!(d.target_details(origin, direction).is_none());
            }
        }
    }
    #[test]
    fn second_tool_splits_c4_and_removes_c3_whole_in_every_color() {
        for material in 0..6 {
            let parent = Block { min: [0; 3], side: 16 * TICKS_PER_C1, material };
            let mut d = Demo { blocks: alloc::vec![parent], tool: 1 };
            let origin = [UNIT, UNIT, -UNIT];
            let direction = [0., 0., 1.];
            let target = d.target_details(origin, direction).unwrap();
            assert_eq!(target.cut.side, 4 * TICKS_PER_C1);
            let preview = d.preview_blocks(Some(target));
            assert_eq!(preview.len(), 14);
            assert_eq!(preview.iter().filter(|b| b.side == 96).count(), 7);
            assert_eq!(preview.iter().filter(|b| b.side == 48).count(), 7);
            assert_eq!(d.blocks, [parent]);
            // Switching to the first tool must reject this subdivision target.
            d.tool = 0;
            d.mine(target);
            assert_eq!(d.blocks, [parent]);
            d.tool = 1;
            d.mine(target);
            assert_eq!(d.blocks, preview);
            let child = d.target_details(origin, direction).unwrap();
            assert_eq!(child.cut, child.parent);
            d.mine(child);
            assert_eq!(d.blocks.len(), 13);
            assert!(d.blocks.iter().all(|b| matches!(b.side, 48 | 96) && b.material == material));
            // Continue along the same ray into the next intact r3 chunk.
            let r3 = d.target_details(origin, direction).unwrap();
            assert_eq!(r3.parent.side, 96);
            assert_eq!(r3.cut.side, 48);
            let before = d.blocks.clone();
            d.mine(r3);
            assert_eq!(d.blocks.len(), before.len() + 6);
            assert_eq!(before.iter().map(|b| b.side.pow(3)).sum::<i32>()
                - d.blocks.iter().map(|b| b.side.pow(3)).sum::<i32>(), 48i32.pow(3));
            d.blocks = alloc::vec![Block { min: [0; 3], side: 4 * TICKS_PER_C1, material }];
            let standalone = d.target_details(origin, direction).unwrap();
            assert_eq!(standalone.cut, standalone.parent);
            d.mine(standalone);
            assert!(d.blocks.is_empty());
        }
    }
    #[test]
    fn first_tool_removes_generated_and_standalone_c4_blocks_whole() {
        let parent = Block { min: [0; 3], side: MINING_BASE_SIDE, material: 3 };
        let mut d = Demo { blocks: alloc::vec![parent], tool: 0 };
        let origin = [UNIT, UNIT, -UNIT];
        let direction = [0., 0., 1.];
        let first = d.target_details(origin, direction).unwrap();
        d.mine(first);
        assert_eq!(d.blocks.len(), 63);
        // The same ray reaches the next exposed child through the new hole.
        let next = d.target_details(origin, direction).unwrap();
        assert_eq!(next.parent.side, 16 * TICKS_PER_C1);
        assert_eq!(next.cut, next.parent);
        let preview = d.preview_blocks(Some(next));
        assert_eq!(preview.len(), 62);
        assert_eq!(d.blocks.len(), 63);
        d.mine(next);
        assert_eq!(d.blocks, preview);
        assert!(d.blocks.iter().all(|b| b.side == 16 * TICKS_PER_C1 && b.material == 3));

        d.blocks = alloc::vec![Block { min: [0; 3], side: 16 * TICKS_PER_C1, material: 5 }];
        let target = d.target_details(origin, direction).unwrap();
        d.tool = NO_TOOL;
        d.mine(target);
        assert_eq!(d.blocks.len(), 1);
        d.tool = 0;
        d.mine(target);
        assert!(d.blocks.is_empty());
    }
    #[test]
    fn wrong_size_occludes_matching_size_and_off_disables_preview() {
        let front = Block {
            min: [0; 3],
            side: 12,
            material: 0,
        };
        let back = Block {
            min: [0, 0, 24],
            side: MINING_BASE_SIDE,
            material: 1,
        };
        let mut d = Demo {
            blocks: alloc::vec![back, front],
            tool: 0,
        };
        assert!(
            d.target_details([UNIT, UNIT, -UNIT], [0., 0., 1.])
                .is_none()
        );
        d.blocks.pop();
        assert!(
            d.target_details([UNIT, UNIT, -UNIT], [0., 0., 1.])
                .is_some()
        );
        d.tool = NO_TOOL;
        assert!(
            d.target_details([UNIT, UNIT, -UNIT], [0., 0., 1.])
                .is_none()
        );
    }
    #[test]
    fn every_face_snaps_to_sixteen_cells_without_sliding_within_a_cell() {
        let parent = Block {
            min: [-768, -768, -768],
            side: 768,
            material: 4,
        };
        let d = Demo {
            blocks: alloc::vec![parent],
            tool: 0,
        };
        for axis in 0..3 {
            for sign in [-1., 1.] {
                for u in 0..4 {
                    for v in 0..4 {
                        let mut expected = parent.min;
                        expected[axis] += if sign > 0. { 0 } else { 576 };
                        expected[(axis + 1) % 3] += u * 192;
                        expected[(axis + 2) % 3] += v * 192;
                        for fraction in [0.01, 0.25, 0.99] {
                            let mut origin = expected.map(|x| (x as f32 + fraction * 192.) * UNIT);
                            origin[axis] = if sign > 0. { -800. * UNIT } else { 16. * UNIT };
                            let mut direction = [0.; 3];
                            direction[axis] = sign;
                            let target = d.target_details(origin, direction).unwrap();
                            assert_eq!(
                                target.cut,
                                Block {
                                    min: expected,
                                    side: 192,
                                    material: 4
                                }
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn fine_tiers_have_exact_two_and_three_subdivisions() {
        let c1 = Block {
            min: [0; 3],
            side: 12,
            material: 2,
        };
        assert_eq!(c1.children().len(), 8);
        assert_eq!(TICKS_PER_C1 / 3, 4);
        assert_eq!(TICKS_PER_C1 / 4, 3);
        let d = Demo::new();
        assert_eq!(d.blocks.len(), 72);
        for side in MINING_SIDES {
            for material in 0..6 {
                assert_eq!(
                    d.blocks
                        .iter()
                        .filter(|b| b.side == side && b.material == material)
                        .count(),
                    1
                );
            }
        }
    }
}
