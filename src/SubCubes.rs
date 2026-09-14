//! Key7 uses an exact 1/24-c1 lattice and block-local subdivision.
use alloc::vec::Vec;

pub const C1: f32 = 0.2;
/// Existing world/VFX contract, in c1 units.
pub const SIDES: [i32; 7] = [1, 2, 3, 4, 6, 8, 12];
pub const TICKS_PER_C1: i32 = 24;
pub const UNIT: f32 = C1 / TICKS_PER_C1 as f32;
/// Display tiers, in 1/24-c1 ticks; R1/2 is one third of c1.
pub const MINING_SIDES: [i32; 10] = [6, 8, 12, 24, 48, 72, 96, 144, 192, 384];
pub const MINING_NAMES: [&str; 10] = ["C1/4", "R1/2", "C1/2", "c1", "c2", "r1", "c3", "r2", "r3", "c4"];
/// Maximum target sizes and removal sizes, in 1/24-c1 ticks.
pub const TOOLS: [i32; 6] = [1536, 384, 96, 48, 24, 144];
pub const CUT_SIDES: [i32; 6] = [384, 96, 24, 12, 3, 72];
pub const TOOL_NAMES: [&str; 6] = ["64 c1 -> c4 (16 c1)", "c4 (16 c1) -> c3 (4 c1)", "c3 (4 c1) -> c1", "c2 / c1 -> C1/2", "c1 / C1/2 -> collect 1/8 c1", "3.5: r2 -> r1 / r1 -> c2"];
pub const TOOL_6: usize = 4;
pub const TOOL_35: usize = 5;
const WHEEL_ORDER: [usize; 7] = [0,1,2,TOOL_35,3,TOOL_6,NO_TOOL];
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
    /// Coordinates and side length in 1/24-c1 ticks.
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
        let side = if matches!(self.side, 384 | 192 | 96 | 48 | 24) {
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

    /// Tool 3.5 cuts can cross several r1 chunks. Keep untouched chunks,
    /// discard covered ones, and resolve partial overlaps on the c1 lattice.
    fn without_r_cut(self, cut: Block, out: &mut Vec<Block>) {
        let intersects = (0..3).all(|a| self.min[a] < cut.min[a]+cut.side
            && self.min[a]+self.side > cut.min[a]);
        if !intersects { out.push(self); return; }
        let covered = (0..3).all(|a| self.min[a] >= cut.min[a]
            && self.min[a]+self.side <= cut.min[a]+cut.side);
        if covered { return; }
        let side = if self.side == 144 { 72 } else { TICKS_PER_C1 };
        let n = self.side / side;
        for x in 0..n { for y in 0..n { for z in 0..n {
            Block {
                min: core::array::from_fn(|a| self.min[a]+[x,y,z][a]*side),
                side, material:self.material,
            }.without_r_cut(cut,out);
        }}}
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MiningTarget {
    pub parent: Block,
    pub cut: Block,
}
impl MiningTarget {
    fn collection_source(self) -> Block {
        let side = TICKS_PER_C1/2;
        Block {
            min: core::array::from_fn(|a| self.parent.min[a]
                +(self.cut.min[a]-self.parent.min[a])/side*side),
            side, material:self.parent.material,
        }
    }
}

/// Visual-only collection pieces: never part of collision or mining targets.
pub struct CollectedPiece {
    pub block: Block,
    pub source_center: [f32; 3],
    pub started: u64,
    pub duration: u64,
}
impl CollectedPiece {
    pub const EXPAND_MS: u64 = 500;
    pub const FLIGHT_MS: u64 = 700;
    pub fn progress(&self, now: u64) -> f32 {
        (now.saturating_sub(self.started) as f32 / self.duration as f32).min(1.)
    }
    /// Damped overshoot inspired by the reference's bottom-center physical
    /// curve. Normalize the endpoint so the flight starts without a jump.
    fn spring(t: f32) -> f32 {
        let response = |t: f32| 1.-libm::expf(-6.*t)
            *(libm::cosf(10.*t)+0.6*libm::sinf(10.*t));
        response(t.clamp(0.,1.))/response(1.)
    }
    /// Positive scale envelope inspired by Physical: 4, 5. Reflect the spring
    /// undershoot into small rebounds instead of inverting the cube geometry.
    fn shrink_spring(t: f32) -> f32 {
        let frequency = core::f32::consts::TAU*5.;
        let residual = |t: f32| libm::expf(-5.*t)
            *(libm::cosf(frequency*t)+(5./frequency)*libm::sinf(frequency*t));
        let end = residual(1.);
        ((residual(t.clamp(0.,1.))-end)/(1.-end)).abs()
    }
    /// Expanded world-space launch point, animated half-size, flight blend.
    pub fn animation(&self, now: u64) -> ([f32;3], f32, f32) {
        let elapsed = now.saturating_sub(self.started);
        let expansion = Self::spring(elapsed as f32 / Self::EXPAND_MS as f32);
        let (center, original_scale) = self.block.pose();
        let launch = core::array::from_fn(|a| self.source_center[a]
            +(center[a]-self.source_center[a])*(1.+2.*expansion));
        let t = (elapsed.saturating_sub(Self::EXPAND_MS) as f32
            / (self.duration-Self::EXPAND_MS) as f32).min(1.);
        let flight = t*t*(3.-2.*t);
        let peak_scale = (C1*0.5-C1*0.005)*0.25;
        let fade = ((flight-0.4)/0.6).clamp(0.,1.);
        let grown = original_scale+(peak_scale-original_scale)*(flight/0.4).min(1.);
        // Do not enter the hull shader's <0.001 marker path during rebounds.
        let scale = (grown*Self::shrink_spring(fade)).max(0.0011);
        (launch,scale,flight)
    }
    pub fn fade_flags(&self, now: u64) -> u32 {
        let (_,_,flight) = self.animation(now);
        let t = ((flight-0.4)/0.6).clamp(0.,1.);
        let alpha = ((1.-t*t*(3.-2.*t))*127.+0.5) as u32;
        // Whole-cube palette transparency; bit 8 selects continuous alpha.
        // Seven alpha bits occupy 3..7 and 10..11, leaving material bits 0..2.
        24576 | 512 | 4096 | 256 | ((alpha&31)<<3) | ((alpha&96)<<5) | self.block.material
    }
    /// Soft software RNG, seeded from immutable flight identity, never frame
    /// time. The same piece therefore keeps its curve while the camera moves.
    fn curve_random(&self) -> [f32;3] {
        let mut rng = self.started as u32 ^ (self.started >> 32) as u32 ^ 0x6d2b79f5;
        for v in self.block.min {
            rng = (rng ^ v as u32).wrapping_mul(0x9e3779b9).rotate_left(13);
        }
        rng ^= (self.block.side as u32).wrapping_mul(7919);
        if rng == 0 { rng = 0x6d2b79f5; }
        core::array::from_fn(|_| {
            // Same small xorshift family as the Rubik palette's soft RNG.
            rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5;
            (rng >> 8) as f32 / 16777215. * 2. - 1.
        })
    }
    pub fn flight_position(&self, from: [f32;3], to: [f32;3], blend: f32) -> [f32;3] {
        let t = blend.clamp(0.,1.);
        if t == 0. { return from; }
        if t == 1. { return to; }
        let delta: [f32;3] = core::array::from_fn(|a|to[a]-from[a]);
        let length = libm::sqrtf(delta.iter().map(|v|v*v).sum());
        if length < 1e-6 { return from; }
        let direction = delta.map(|v|v/length);
        let random = self.curve_random();
        let along: f32 = (0..3).map(|a|random[a]*direction[a]).sum();
        let bend: [f32;3] = core::array::from_fn(|a|random[a]-along*direction[a]);
        // Gentle quadratic arc, at most 8% of the trip (and 0.3 world units).
        // Its envelope vanishes at both ends; no snap into or out of flight.
        let strength = (length*0.08).min(0.3)*4.*t*(1.-t)/1.732051;
        core::array::from_fn(|a|from[a]+delta[a]*t+bend[a]*strength)
    }
}
#[derive(Default)]
pub struct Collection {
    pub pieces: Vec<CollectedPiece>,
    pub vanished: u64,
    opened: u64,
    arrival: u64,
}
impl Collection {
    // Six largest volleys fit beneath the minimum full-geometry budget,
    // including the Rubik companion and both counters.
    pub const MAX_PIECES: usize = 378;
    pub fn advance(&mut self, now: u64) {
        let before = self.pieces.len();
        self.pieces.retain(|p| p.progress(now) < 1.);
        self.vanished = self.vanished.saturating_add((before-self.pieces.len()) as u64);
    }
    pub fn expansion(&self, now: u64) -> f32 {
        if self.arrival == 0 { return 0.; }
        let opening = (now.saturating_sub(self.opened) as f32 / 120.).min(1.);
        let closing = (self.arrival.saturating_add(180).saturating_sub(now) as f32 / 180.).min(1.);
        let t = opening.min(closing);
        t*t*(3.-2.*t)
    }
    pub fn commit(&mut self, demo: &mut Demo, target: MiningTarget, now: u64) -> bool {
        self.advance(now);
        if !demo.blocks.contains(&target.parent) { return false; }
        // Validate through the same rules as normal mining before changing
        // either scene occupancy or the collection state.
        let mut isolated = Demo { blocks: alloc::vec![target.parent], tool: demo.tool };
        isolated.mine(target);
        if isolated.blocks == [target.parent] { return false; }
        if demo.tool == TOOL_6 {
            let flying = isolated.blocks.iter().filter(|b| b.side == target.cut.side).count();
            if self.pieces.len() + flying > Self::MAX_PIECES { return false; }
            if self.pieces.is_empty() { self.opened = now; }
            demo.blocks.retain(|b| *b != target.parent);
            let mut i = 0;
            for block in isolated.blocks {
                if block.side != target.cut.side {
                    demo.blocks.push(block);
                    continue;
                }
                let duration = CollectedPiece::EXPAND_MS + CollectedPiece::FLIGHT_MS + i as u64 * 3;
                self.arrival = self.arrival.max(now + duration);
                self.pieces.push(CollectedPiece { block, source_center: target.collection_source().pose().0, started: now, duration });
                i += 1;
            }
        } else { demo.mine(target); }
        true
    }
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
        let at = WHEEL_ORDER.iter().position(|&t| t == self.tool).unwrap_or(WHEEL_ORDER.len()-1);
        self.tool = WHEEL_ORDER[(at as i32 + wheel.signum()).rem_euclid(WHEEL_ORDER.len() as i32) as usize];
    }
    pub fn tool_id(&self) -> &'static str {
        ["1","2","3","4","6","3.5"].get(self.tool).copied().unwrap_or("0")
    }
    fn cut_for(&self, parent: Block) -> Option<i32> {
        if self.tool == TOOL_35 && parent.side == 72 { Some(48) } else { self.cut_side() }
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
        if self.tool == TOOL_35 { return matches!(side, 72 | 144); }
        let Some(selected) = self.tool_side() else { return false; };
        side == selected
            || (self.tool < 4 && self.cut_side() == Some(side))
            || (matches!(self.tool, 1 | 2 | 3 | TOOL_6) && side == selected / 2)
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
        let side = self.cut_for(parent)?;
        if parent.side == side {
            return Some(MiningTarget { parent, cut: parent });
        }
        let min = core::array::from_fn(|a| {
            let p = origin[a] + direction[a] * (distance + 0.0001) - parent.min[a] as f32;
            if self.tool == TOOL_35 && parent.side == 72 {
                // A 2-c1 cut fits either corner of a 3-c1 parent on each axis.
                return parent.min[a] + if p >= 36. { 24 } else { 0 };
            }
            if self.tool == TOOL_35 && parent.side == 144 {
                let cell = ((p / TICKS_PER_C1 as f32) as i32)
                    .clamp(0, (parent.side-side)/TICKS_PER_C1);
                return parent.min[a]+cell*TICKS_PER_C1;
            }
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
                    if self.tool == TOOL_35 {
                        block.without_r_cut(t.cut, &mut out);
                    } else if self.tool == TOOL_6 && block.side == TICKS_PER_C1 {
                        let source = t.collection_source();
                        for half in block.children() {
                            if half == source {
                                let isolated = Demo { blocks:alloc::vec![half], tool:TOOL_6 };
                                out.extend(isolated.preview_blocks(Some(MiningTarget {parent:half,cut:t.cut})));
                            } else { out.push(half); }
                        }
                    } else if block.side == TICKS_PER_C1 || self.tool == TOOL_6 {
                        // Tool 4 splits c1; tool 6 splits C1/2 into quarters.
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
        let Some(cut_side) = self.cut_for(target.parent) else { return; };
        let valid = if target.parent.side == cut_side {
            target.cut == target.parent
        } else {
            target.cut.side == cut_side
                && target.cut.material == target.parent.material
                && (0..3).all(|a| {
                    let offset = target.cut.min[a] - target.parent.min[a];
                    offset >= 0 && offset + target.cut.side <= target.parent.side
                        && offset % (if self.tool == TOOL_35 { TICKS_PER_C1 } else { target.cut.side }) == 0
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
    #[test]
    fn collection_counts_only_arrivals_and_removes_only_committed_parent() {
        for tool in [TOOL_6] { for material in 0..6 {
            let parent = Block { min: [0;3], side: 12, material };
            let other = Block { min: [96;3], ..parent };
            let mut demo = Demo { blocks:alloc::vec![parent,other], tool };
            let target = demo.target_details([UNIT,UNIT,-UNIT],[0.,0.,1.]).unwrap();
            let expected = demo.preview_blocks(Some(target));
            let mut collection = Collection::default();
            assert!(collection.commit(&mut demo,target,100));
            assert_eq!(demo.blocks,[other]);
            assert_eq!(collection.pieces.len(), 63);
            assert_eq!(collection.vanished,0);
            assert!(collection.pieces.iter().all(|p| expected.contains(&p.block)));
            assert!(!collection.commit(&mut demo,target,101));
            assert_eq!(collection.expansion(220),1.);
            collection.advance(1299);
            assert_eq!(collection.vanished,0);
            collection.advance(1300);
            assert_eq!(collection.vanished,1);
            collection.advance(1500);
            assert!(collection.pieces.is_empty());
            assert_eq!(collection.vanished, 63);
            collection.advance(1800);
            assert_eq!(collection.vanished, 63);
            assert_eq!(collection.expansion(1800),0.);
        }}
    }
    #[test]
    fn collection_alpha_fades_with_positive_spring_shrink_and_preserves_palette() {
        for material in 0..6 {
            let piece=CollectedPiece {
                block:Block { min:[0;3], side:3, material }, source_center:[0.05;3],
                started:100, duration:CollectedPiece::EXPAND_MS+CollectedPiece::FLIGHT_MS,
            };
            let mut last_alpha=127;
            let mut last_scale=0.;
            for now in 100..=1300 {
                let flags=piece.fade_flags(now);
                let alpha=((flags>>3)&31)|((flags>>5)&96);
                assert_eq!(flags&7,material);
                assert_eq!(flags&57856,25088); // whole-cube transparent route
                assert_eq!(flags&4352,4352); // continuous palette alpha
                assert!(alpha<=last_alpha);
                if now<=600 { assert_eq!(alpha,127); }
                let (_,scale,_)=piece.animation(now);
                assert!(scale.is_finite() && scale>=0.0011);
                last_alpha=alpha; last_scale=scale;
            }
            assert_eq!(last_alpha,0);
            assert!((last_scale-0.0011).abs()<1e-6);
        }
    }
    #[test]
    fn collection_soft_rng_curves_are_stable_distinct_bounded_and_meet_endpoints() {
        let mut mids=Vec::new();
        for x in 0..4 { for y in 0..4 { for z in 0..4 {
            let piece=CollectedPiece {
                block:Block {min:[x*3,y*3,z*3],side:3,material:0},
                source_center:[0.05;3],started:100,duration:1300,
            };
            let from=[-2.,-1.,-3.]; let to=[0.2,0.2,-1.];
            assert_eq!(piece.flight_position(from,to,0.),from);
            assert_eq!(piece.flight_position(from,to,1.),to);
            assert_eq!(piece.flight_position(from,from,0.5),from);
            let mid=piece.flight_position(from,to,0.5);
            assert_eq!(mid,piece.flight_position(from,to,0.5));
            assert!(!mids.contains(&mid));
            mids.push(mid);
            for step in 0..=100 {
                let t=step as f32/100.;
                let p=piece.flight_position(from,to,t);
                let offset: [f32;3]=core::array::from_fn(|a|p[a]-(from[a]+(to[a]-from[a])*t));
                let squared:f32=offset.iter().map(|v|v*v).sum();
                assert!(squared<=0.3*0.3+1e-6);
                assert!((0..3).map(|a|offset[a]*(to[a]-from[a])).sum::<f32>().abs()<1e-5);
                assert!(p.iter().all(|v|v.is_finite()));
            }
        }}}
    }
    #[test]
    fn tool6_c1_preview_keeps_seven_halves_and_only_collects_target_half() {
        for material in 0..6 { for x in 0..2 { for y in 0..2 { for z in 0..2 {
            let parent=Block {min:[-24;3],side:24,material};
            let half=Block {min:[-24+x*12,-24+y*12,-24+z*12],side:12,material};
            let target=MiningTarget {parent,cut:Block {min:half.min,side:3,material}};
            let mut demo=Demo {blocks:alloc::vec![parent],tool:TOOL_6};
            let preview=demo.preview_blocks(Some(target));
            assert_eq!(preview.len(),70);
            let retained:Vec<_>=preview.iter().copied().filter(|b|b.side==12).collect();
            assert_eq!(retained.len(),7);
            assert!(!retained.contains(&half));
            assert_eq!(preview.iter().filter(|b|b.side==3).count(),63);
            assert_eq!(demo.blocks,[parent]);
            let mut collection=Collection::default();
            assert!(collection.commit(&mut demo,target,100));
            assert_eq!(demo.blocks,retained);
            assert_eq!(collection.pieces.len(),63);
            assert!(collection.pieces.iter().all(|p| p.block.side==3 && p.block.material==material
                && p.source_center==half.pose().0 && preview.contains(&p.block)));
            collection.advance(2000);
            assert_eq!(collection.vanished,63);
            assert_eq!(demo.blocks,retained);
        }}}}
    }
    #[test]
    fn collection_capacity_and_other_tools_never_lose_scene_blocks() {
        let mut collection = Collection::default();
        let parent = Block { min: [0;3], side: 12, material:2 };
        for i in 0..7 {
            let mut demo = Demo { blocks:alloc::vec![parent], tool:TOOL_6 };
            let target = demo.target_details([UNIT,UNIT,-UNIT],[0.,0.,1.]).unwrap();
            assert_eq!(collection.commit(&mut demo,target,10),i<6);
            assert_eq!(demo.blocks.is_empty(),i<6);
        }
        assert_eq!(collection.pieces.len(),Collection::MAX_PIECES);
        collection.advance(2000);
        assert_eq!(collection.vanished,378);
        for tool in 0..4 {
            let parent = Block { side:TOOLS[tool], ..parent };
            let mut demo = Demo { blocks:alloc::vec![parent], tool };
            let target = demo.target_details([UNIT,UNIT,-UNIT],[0.,0.,1.]).unwrap();
            let expected = demo.preview_blocks(Some(target));
            assert!(collection.commit(&mut demo,target,2100));
            assert_eq!(demo.blocks,expected);
            assert!(collection.pieces.is_empty());
            assert_eq!(collection.vanished,378);
        }
    }
    #[test]
    fn collection_expands_then_grows_and_spring_shrinks_during_fade() {
        assert_eq!(CollectedPiece::spring(0.),0.);
        assert!((CollectedPiece::spring(1.)-1.).abs()<1e-6);
        assert!(CollectedPiece::spring(0.32)>1.1);
        for side in [3] {
            let piece = CollectedPiece {
                block:Block {min:[0;3],side,material:2},
                source_center:[0.05;3], started:100,
                duration:CollectedPiece::EXPAND_MS+CollectedPiece::FLIGHT_MS,
            };
            let (original, size)=piece.block.pose();
            let (start,start_size,flight)=piece.animation(100);
            for a in 0..3 { assert!((start[a]-original[a]).abs()<1e-6); }
            assert_eq!(start_size,size);
            assert_eq!(flight,0.);
            let (peak,_,flight)=piece.animation(260);
            assert_eq!(flight,0.);
            let (expanded,expanded_size,flight)=piece.animation(600);
            assert_eq!(flight,0.);
            assert_eq!(expanded_size,size);
            assert!((peak[0]-piece.source_center[0]).abs()>(expanded[0]-piece.source_center[0]).abs());
            for a in 0..3 {
                assert!((expanded[a]-(piece.source_center[a]+3.*(original[a]-piece.source_center[a]))).abs()<1e-6);
            }
            let (after,_,_)=piece.animation(601);
            assert_eq!(after,expanded);
            let (_,mid_size,blend)=piece.animation(950);
            assert!((blend-0.5).abs()<1e-6);
            let peak_scale=(C1*0.5-C1*0.005)*0.25;
            assert!(mid_size>=0.0011 && mid_size<=peak_scale);
            let (_,growing,_) = piece.animation(810);
            assert!(growing>size && growing<peak_scale);
            let actual_peak=(600..=1300).map(|now|piece.animation(now).1).fold(0f32,f32::max);
            assert!(actual_peak<=peak_scale && actual_peak>peak_scale*0.99);
            let (_,end_size,blend)=piece.animation(1300);
            assert_eq!(blend,1.);
            assert!((end_size-0.0011).abs()<1e-6);
        }
    }
    #[test]
    fn physical_shrink_has_rebounds_and_settles_without_negative_scale() {
        assert!((CollectedPiece::shrink_spring(0.)-1.).abs()<1e-6);
        assert_eq!(CollectedPiece::shrink_spring(1.),0.);
        let samples: Vec<_>=(0..=1000).map(|i|CollectedPiece::shrink_spring(i as f32/1000.)).collect();
        assert!(samples.iter().all(|s| s.is_finite() && *s>=0. && *s<=1.01));
        let rebounds=samples.windows(3).filter(|v|v[1]>v[0] && v[1]>v[2]).count();
        assert!(rebounds>=4);
        assert!(samples[900]<0.02);
    }
    fn gesture_target(x: i32) -> MiningTarget {
        let parent = Block { min: [x,0,0], side: 24, material: 0 };
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
        let expected = ["1","2","3","3.5","4","6","0"];
        for id in expected {
            d.cycle(1);
            assert_eq!(d.tool_id(),id);
        }
        for tool in WHEEL_ORDER.into_iter().filter(|&t| t != NO_TOOL) {
            d.cycle(1);
            assert_eq!(d.tool, tool);
        }
        d.cycle(1);
        assert_eq!(d.tool, NO_TOOL);
        for tool in WHEEL_ORDER.into_iter().filter(|&t| t != NO_TOOL).rev() {
            d.cycle(-1);
            assert_eq!(d.tool, tool);
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
                        preview.iter().map(|b| (b.side as i64).pow(3)).sum::<i64>()
                            + (target.cut.side as i64).pow(3),
                        (side as i64).pow(3)
                    );
                    assert!(preview.iter().all(|b| b.material == 5 && *b != target.cut));
                    let n = side / target.cut.side;
                    assert_eq!(n, [4,4,4,4,8,2][tool]);
                    assert_eq!(preview.len(), [63, 14, 14, 14, 70,7][tool]);
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
    fn small_split_tools_keep_uniform_children_and_enforce_target_sizes() {
        for material in 0..6 { for tool in [3,TOOL_6] {
            let size = if tool == 3 {24} else {12};
            let parent = Block { min: [-size;3], side: size, material };
            let mut d = Demo { blocks: alloc::vec![parent], tool };
            let cut_side = CUT_SIDES[tool];
            let n = size / cut_side;
            for axis in 0..3 { for u in 0..n { for v in 0..n {
                let mut origin = [-size as f32 + 0.5;3];
                origin[axis] = -size as f32-1.;
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
                d.tool = if tool == TOOL_6 { 3 } else { TOOL_6 };
                d.mine(target);
                assert_eq!(d.blocks, [parent]); // stale wheel target
                d.tool = tool;
            }}}
            for side in MINING_SIDES.into_iter().chain([MINING_BASE_SIDE]) {
                let block = Block { min: [0;3], side, material };
                d.blocks = alloc::vec![block];
                let hit = d.target_details([UNIT*0.5,UNIT*0.5,-UNIT], [0.,0.,1.]);
                assert_eq!(hit.is_some(), if tool == 3 {matches!(side,12|24|48)} else {matches!(side,12|24)});
                if tool == 3 && side == 12 {
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
    fn tool_35_cuts_r1_corners_and_r2_octants_without_collecting() {
        for side in [72,144] { for material in 0..6 {
            for axis in 0..3 { for sign in [-1.,1.] { for u in 0..2 { for v in 0..2 {
                let parent = Block { min:[-side;3], side, material };
                let mut d = Demo { blocks:alloc::vec![parent], tool:TOOL_35 };
                assert_eq!(d.tool_id(),"3.5");
                let mut origin = [-side as f32+1.;3];
                origin[axis] = if sign > 0. { -side as f32-1. } else { 1. };
                origin[(axis+1)%3] += (u*(side-2)) as f32;
                origin[(axis+2)%3] += (v*(side-2)) as f32;
                let mut direction = [0.;3]; direction[axis]=sign;
                let t = d.target_details(origin.map(|x|x*UNIT), direction).unwrap();
                let cut = if side == 72 {48} else {72};
                assert_eq!(t.cut.side,cut);
                let expected = d.preview_blocks(Some(t));
                assert_eq!(expected.len(),if side == 72 {19} else {7});
                assert!(expected.iter().all(|b| b.material==material && b.side==if side==72 {24} else {72}));
                assert_eq!(expected.iter().map(|b|b.side.pow(3)).sum::<i32>()+cut.pow(3),side.pow(3));
                let mut collection = Collection::default();
                assert!(collection.commit(&mut d,t,100));
                assert_eq!(d.blocks,expected);
                assert!(collection.pieces.is_empty());
                assert_eq!(collection.vanished,0);
            }}}}
        }}
        for side in MINING_SIDES.into_iter().chain([MINING_BASE_SIDE]).filter(|s| !matches!(s, 72|144)) {
            let parent=Block {min: [0;3],side,material:0};
            let mut d=Demo {blocks:alloc::vec![parent],tool:TOOL_35};
            assert!(d.target_details([UNIT,UNIT,-UNIT],[0.,0.,1.]).is_none());
            d.mine(MiningTarget {parent,cut:parent});
            assert_eq!(d.blocks,[parent]);
        }
    }
    #[test]
    fn tool_35_r2_snaps_each_c1_step_and_preview_matches_cut_occupancy() {
        let parent = Block { min: [-144;3],side: 144,material:4 };
        for axis in 0..3 { for sign in [-1.,1.] { for u in 0..4 { for v in 0..4 {
            let mut d = Demo { blocks:alloc::vec![parent],tool:TOOL_35 };
            let mut origin = [-143.5;3];
            origin[axis] = if sign>0. {-145.} else {1.};
            origin[(axis+1)%3] += (u*24) as f32;
            origin[(axis+2)%3] += (v*24) as f32;
            let mut direction=[0.;3]; direction[axis]=sign;
            let target=d.target_details(origin.map(|x|x*UNIT),direction).unwrap();
            let mut expected=parent.min;
            expected[axis] += if sign>0. {0} else {72};
            expected[(axis+1)%3] += u*24;
            expected[(axis+2)%3] += v*24;
            assert_eq!(target.cut.min,expected);
            assert_eq!(target.cut.side,72);
            let preview=d.preview_blocks(Some(target));
            let crossed=(if matches!(u,1|2) {2} else {1})*(if matches!(v,1|2) {2} else {1});
            assert_eq!(preview.iter().filter(|b|b.side==72).count(),8-crossed);
            assert_eq!(preview.len(),[0,7,33,0,85][crossed]);
            for x in 0..6 { for y in 0..6 { for z in 0..6 {
                let p=core::array::from_fn::<_,3,_>(|a|parent.min[a]+[x,y,z][a]*24);
                let cut=(0..3).all(|a|p[a]>=expected[a] && p[a]<expected[a]+72);
                let occupancy=preview.iter().filter(|b|(0..3).all(|a|p[a]>=b.min[a] && p[a]<b.min[a]+b.side)).count();
                assert_eq!(occupancy,usize::from(!cut));
            }}}
            assert_eq!(d.blocks,[parent]);
            d.mine(target);
            assert_eq!(d.blocks,preview);
        }}}}
    }
    #[test]
    fn fourth_tool_c2_preserves_seven_c1_chunks_and_seven_halves() {
        for material in 0..6 {
            let parent = Block { min: [-48;3], side: 48, material };
            for axis in 0..3 { for sign in [-1.,1.] { for u in 0..4 { for v in 0..4 {
                let mut d = Demo { blocks: alloc::vec![parent], tool: 3 };
                let mut origin = [-47.5;3];
                origin[axis] = if sign > 0. { -49. } else { 1. };
                origin[(axis+1)%3] += (u*12) as f32;
                origin[(axis+2)%3] += (v*12) as f32;
                let mut direction = [0.;3]; direction[axis] = sign;
                let target = d.target_details(origin.map(|x|x*UNIT), direction).unwrap();
                assert_eq!(target.cut.side, 12);
                let preview = d.preview_blocks(Some(target));
                assert_eq!(preview.len(), 14);
                assert_eq!(preview.iter().filter(|b| b.side == 24).count(), 7);
                assert_eq!(preview.iter().filter(|b| b.side == 12).count(), 7);
                assert!(preview.iter().all(|b| b.material == material));
                assert_eq!(preview.iter().map(|b| b.side.pow(3)).sum::<i32>(), 48i32.pow(3)-12i32.pow(3));
                assert_eq!(d.blocks, [parent]);
                d.mine(target);
                assert_eq!(d.blocks, preview);
            }}}}
        }
    }
    #[test]
    fn third_tool_removes_c1_and_rejects_larger_than_c3_in_all_colors() {
        for material in 0..6 {
            let parent = Block { min: [0;3], side: 96, material };
            let mut d = Demo { blocks: alloc::vec![parent], tool: 2 };
            let origin = [UNIT, UNIT, -UNIT];
            let direction = [0., 0., 1.];
            let target = d.target_details(origin, direction).unwrap();
            assert_eq!(target.cut.side, TICKS_PER_C1);
            let preview = d.preview_blocks(Some(target));
            assert_eq!(preview.len(), 14);
            assert_eq!(preview.iter().filter(|b| b.side == 48).count(), 7);
            assert_eq!(preview.iter().filter(|b| b.side == 24).count(), 7);
            assert_eq!(d.blocks, [parent]);
            d.mine(target);
            assert_eq!(d.blocks, preview);
            assert!(d.blocks.iter().all(|b| matches!(b.side, 24 | 48) && b.material == material));
            let child = d.target_details(origin, direction).unwrap();
            assert_eq!(child.cut, child.parent);
            d.mine(child);
            assert_eq!(d.blocks.len(), 13);
            let c2 = d.target_details(origin, direction).unwrap();
            assert_eq!(c2.parent.side, 48);
            assert_eq!(c2.cut.side, 24);
            let before_volume = d.blocks.iter().map(|b| b.side.pow(3)).sum::<i32>();
            d.mine(c2);
            assert_eq!(d.blocks.len(), 19);
            assert_eq!(before_volume - d.blocks.iter().map(|b| b.side.pow(3)).sum::<i32>(), 24i32.pow(3));
            d.blocks = alloc::vec![Block { min: [0;3], side: 48, material }];
            let c2 = d.target_details(origin, direction).unwrap();
            d.mine(c2);
            assert_eq!(d.blocks.len(), 7);
            assert!(d.blocks.iter().all(|b| b.side == 24 && b.material == material));
            d.blocks = alloc::vec![Block { min: [0;3], side: 24, material }];
            let child = d.target_details(origin, direction).unwrap();
            d.mine(child);
            assert!(d.blocks.is_empty());
            for side in [144, 192, 384, MINING_BASE_SIDE] {
                d.blocks = alloc::vec![Block { min: [0;3], side, material }];
                assert!(d.target_details(origin, direction).is_none());
            }
        }
    }
    #[test]
    fn second_tool_splits_c4_and_removes_c3_whole_in_every_color() {
        for material in 0..6 {
            let parent = Block { min: [0;3], side: 16 * TICKS_PER_C1, material };
            let mut d = Demo { blocks: alloc::vec![parent], tool: 1 };
            let origin = [UNIT, UNIT, -UNIT];
            let direction = [0., 0., 1.];
            let target = d.target_details(origin, direction).unwrap();
            assert_eq!(target.cut.side, 4 * TICKS_PER_C1);
            let preview = d.preview_blocks(Some(target));
            assert_eq!(preview.len(), 14);
            assert_eq!(preview.iter().filter(|b| b.side == 192).count(), 7);
            assert_eq!(preview.iter().filter(|b| b.side == 96).count(), 7);
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
            assert!(d.blocks.iter().all(|b| matches!(b.side, 96 | 192) && b.material == material));
            // Continue along the same ray into the next intact r3 chunk.
            let r3 = d.target_details(origin, direction).unwrap();
            assert_eq!(r3.parent.side, 192);
            assert_eq!(r3.cut.side, 96);
            let before = d.blocks.clone();
            d.mine(r3);
            assert_eq!(d.blocks.len(), before.len() + 6);
            assert_eq!(before.iter().map(|b| b.side.pow(3)).sum::<i32>()
                - d.blocks.iter().map(|b| b.side.pow(3)).sum::<i32>(), 96i32.pow(3));
            d.blocks = alloc::vec![Block { min: [0;3], side: 4 * TICKS_PER_C1, material }];
            let standalone = d.target_details(origin, direction).unwrap();
            assert_eq!(standalone.cut, standalone.parent);
            d.mine(standalone);
            assert!(d.blocks.is_empty());
        }
    }
    #[test]
    fn first_tool_removes_generated_and_standalone_c4_blocks_whole() {
        let parent = Block { min: [0;3], side: MINING_BASE_SIDE, material: 3 };
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

        d.blocks = alloc::vec![Block { min: [0;3], side: 16 * TICKS_PER_C1, material: 5 }];
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
            min: [0;3],
            side: 24,
            material: 0,
        };
        let back = Block {
            min: [0, 0, 48],
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
            min: [-1536, -1536, -1536],
            side: 1536,
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
                        expected[axis] += if sign > 0. { 0 } else { 1152 };
                        expected[(axis + 1) % 3] += u * 384;
                        expected[(axis + 2) % 3] += v * 384;
                        for fraction in [0.01, 0.25, 0.99] {
                            let mut origin = expected.map(|x| (x as f32 + fraction * 384.) * UNIT);
                            origin[axis] = if sign > 0. { -1600. * UNIT } else { 16. * UNIT };
                            let mut direction = [0.; 3];
                            direction[axis] = sign;
                            let target = d.target_details(origin, direction).unwrap();
                            assert_eq!(
                                target.cut,
                                Block {
                                    min: expected,
                                    side: 384,
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
    fn collection_uses_halves_and_preserves_display_world_dimensions() {
        for (side, c1_units) in MINING_SIDES.into_iter().zip([0.25,1./3.,0.5,1.,2.,3.,4.,6.,8.,16.]) {
            let block = Block {min:[0;3],side,material:0};
            let (center,scale)=block.pose();
            assert!((center[0]-c1_units*C1*0.5).abs()<1e-6);
            assert!((scale+C1*0.005-c1_units*C1*0.5).abs()<1e-6);
        }
        for tool in [TOOL_6] {
            assert_eq!(TOOLS[tool],TICKS_PER_C1);
            assert_eq!(CUT_SIDES[tool]*8,TICKS_PER_C1);
            let d=Demo {blocks:Vec::new(),tool};
            assert!(d.accepts(TICKS_PER_C1));
            assert!(d.accepts(TICKS_PER_C1/2));
            assert!(!d.accepts(CUT_SIDES[tool]));
        }
    }
    #[test]
    fn fine_tiers_have_exact_two_and_three_subdivisions() {
        let c1 = Block {
            min: [0;3],
            side: 24,
            material: 2,
        };
        assert_eq!(c1.children().len(), 8);
        assert_eq!(TICKS_PER_C1 / 3, 8);
        assert_eq!(TICKS_PER_C1 / 4, 6);
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
