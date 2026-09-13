//! Key7 uses an exact sixth-c1 lattice and block-local subdivision.
use alloc::vec::Vec;

pub const C1: f32 = 0.2;
/// Existing world/VFX contract, in c1 units.
pub const SIDES: [i32; 7] = [1, 2, 3, 4, 6, 8, 12];
pub const TICKS_PER_C1: i32 = 6;
pub const UNIT: f32 = C1 / TICKS_PER_C1 as f32;
/// Display tiers, in sixth-c1 ticks; c4 remains 16 c1.
pub const MINING_SIDES: [i32; 9] = [1, 3, 6, 12, 18, 24, 36, 72, 96];
pub const MINING_NAMES: [&str; 9] = ["R1/2", "C1/2", "c1", "c2", "r1", "c3", "r2", "r3", "c4"];
/// First mining pass: split the largest blocks or remove whole c4 children.
pub const TOOLS: [i32; 1] = [384];
pub const TOOL_NAMES: [&str; 1] = ["64 c1 -> c4 (16 c1)"];
pub const NO_TOOL: usize = TOOLS.len();
pub const MINING_BASE_SIDE: i32 = 64 * TICKS_PER_C1;

pub fn walkable(side: i32) -> bool {
    matches!(side, 4 | 6 | 8 | 12 | 16 | 64)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    /// Coordinates and side length in sixth-c1 ticks.
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
        let side = if self.side == MINING_BASE_SIDE {
            Some(16 * TICKS_PER_C1)
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MiningTarget {
    pub parent: Block,
    pub cut: Block,
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
                    min: [(color * 18 - 51) * 6, 0, (row as i32 * 18 - 75) * 6],
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
    pub fn tool_name(&self) -> &'static str {
        TOOL_NAMES.get(self.tool).copied().unwrap_or("none")
    }

    pub fn target_details(&self, origin: [f32; 3], direction: [f32; 3]) -> Option<MiningTarget> {
        let selected = self.tool_side()?;
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
        if parent.side == selected / 4 {
            return Some(MiningTarget { parent, cut: parent });
        }
        if parent.side != selected {
            return None;
        }
        let side = parent.children()[0].side;
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
                    out.extend(block.children().into_iter().filter(|b| *b != t.cut));
                }
            } else {
                out.push(block);
            }
        }
        out
    }
    pub fn mine(&mut self, target: MiningTarget) {
        // Reject stale previews or targets from a different wheel selection.
        let Some(selected) = self.tool_side() else { return; };
        let valid = if target.parent.side == selected / 4 {
            target.cut == target.parent
        } else {
            target.parent.side == selected && target.parent.children().contains(&target.cut)
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
                    assert_eq!(n, 4);
                    assert_eq!(preview.len(), (n * n * n - 1) as usize);
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
            side: 6,
            material: 0,
        };
        let back = Block {
            min: [0, 0, 12],
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
            min: [-384, -384, -384],
            side: 384,
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
                        expected[axis] += if sign > 0. { 0 } else { 288 };
                        expected[(axis + 1) % 3] += u * 96;
                        expected[(axis + 2) % 3] += v * 96;
                        for fraction in [0.01, 0.25, 0.99] {
                            let mut origin = expected.map(|x| (x as f32 + fraction * 96.) * UNIT);
                            origin[axis] = if sign > 0. { -400. * UNIT } else { 16. * UNIT };
                            let mut direction = [0.; 3];
                            direction[axis] = sign;
                            let target = d.target_details(origin, direction).unwrap();
                            assert_eq!(
                                target.cut,
                                Block {
                                    min: expected,
                                    side: 96,
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
            side: 6,
            material: 2,
        };
        assert_eq!(c1.children().len(), 8);
        assert_eq!(c1.children()[0].children().len(), 27);
        let d = Demo::new();
        assert_eq!(d.blocks.len(), 66);
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
