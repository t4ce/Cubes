//! Client-side c1 lattice, tier decomposition, and the Key7 mining fixture.
//! A cut removes its intersection with every block. Remaining cells are packed
//! deterministically, largest tier first, without merging distinct materials.
use alloc::vec::Vec;

fn floor(v: f32) -> i32 {
    let n = v as i32;
    n - i32::from(v < n as f32)
}

pub const C1: f32 = 0.2;
pub const SIDES: [i32; 7] = [1, 2, 3, 4, 6, 8, 12];
pub const NAMES: [&str; 7] = ["c1", "c2", "r1", "c3", "r2", "c4", "r3"];
pub const TOOLS: [i32; 4] = [1, 2, 4, 8];
pub fn walkable(side: i32) -> bool {
    matches!(side, 4 | 6 | 8 | 12)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    pub min: [i32; 3],
    pub side: i32,
    pub material: u32,
}
impl Block {
    pub fn pose(self) -> ([f32; 3], f32) {
        (
            self.min.map(|v| (v as f32 + self.side as f32 * 0.5) * C1),
            (self.side as f32 - 0.01) * C1 * 0.5,
        )
    }
    fn contains(self, p: [i32; 3]) -> bool {
        (0..3).all(|a| p[a] >= self.min[a] && p[a] < self.min[a] + self.side)
    }
}

pub fn subtract(block: Block, cut: Block, out: &mut Vec<Block>) {
    if (0..3)
        .any(|a| cut.min[a] >= block.min[a] + block.side || cut.min[a] + cut.side <= block.min[a])
    {
        out.push(block);
        return;
    }
    let n = block.side as usize;
    let index = |x: usize, y: usize, z: usize| (x * n + y) * n + z;
    let mut remaining = alloc::vec![true; n*n*n];
    for x in 0..n {
        for y in 0..n {
            for z in 0..n {
                remaining[index(x, y, z)] = !cut.contains([
                    block.min[0] + x as i32,
                    block.min[1] + y as i32,
                    block.min[2] + z as i32,
                ]);
            }
        }
    }
    for side in SIDES.into_iter().rev().filter(|&s| s <= block.side) {
        let s = side as usize;
        for x in 0..=n - s {
            for y in 0..=n - s {
                for z in 0..=n - s {
                    if !(x..x + s)
                        .all(|a| (y..y + s).all(|b| (z..z + s).all(|c| remaining[index(a, b, c)])))
                    {
                        continue;
                    }
                    for a in x..x + s {
                        for b in y..y + s {
                            for c in z..z + s {
                                remaining[index(a, b, c)] = false;
                            }
                        }
                    }
                    out.push(Block {
                        min: [
                            block.min[0] + x as i32,
                            block.min[1] + y as i32,
                            block.min[2] + z as i32,
                        ],
                        side,
                        material: block.material,
                    });
                }
            }
        }
    }
}

pub struct Demo {
    pub blocks: Vec<Block>,
    pub tool: usize,
}
impl Demo {
    pub fn new() -> Self {
        let mut blocks = Vec::new();
        for (row, side) in SIDES.into_iter().enumerate() {
            for color in 0..6 {
                blocks.push(Block {
                    min: [color * 18 - 51, 0, row as i32 * 18 - 57],
                    side,
                    material: color as u32,
                });
            }
        }
        Self { blocks, tool: 0 }
    }
    pub fn cycle(&mut self, wheel: i32) {
        self.tool = (self.tool as i32 + wheel.signum()).rem_euclid(4) as usize;
    }
    /// Ray hits ideal faces. Tangential coordinates always advance by c1,
    /// independent of the tool side; the tool extends inward from that face.
    pub fn target(&self, origin: [f32; 3], direction: [f32; 3]) -> Option<Block> {
        let origin = origin.map(|x| x / C1);
        let mut best = f32::INFINITY;
        let mut result = None;
        for block in &self.blocks {
            let mut near = f32::NEG_INFINITY;
            let mut far = f32::INFINITY;
            let mut axis = 0;
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
                    if t0.min(t1) > near {
                        near = t0.min(t1);
                        axis = a;
                    }
                    far = far.min(t0.max(t1));
                }
            }
            if near < 0. || near > far || near >= best {
                continue;
            }
            best = near;
            let side = TOOLS[self.tool];
            let mut min = core::array::from_fn(|a| {
                floor(origin[a] + direction[a] * near - direction[a] * 0.0001)
            });
            for a in 0..3 {
                if a != axis {
                    min[a] = min[a].clamp(block.min[a], block.min[a] + block.side - 1);
                }
            }
            min[axis] = if direction[axis] > 0. {
                block.min[axis]
            } else {
                block.min[axis] + block.side - side
            };
            result = Some(Block {
                min,
                side,
                material: 0,
            });
        }
        result
    }
    pub fn mine(&mut self, cut: Block) {
        let mut next = Vec::new();
        for &block in &self.blocks {
            subtract(block, cut, &mut next);
        }
        self.blocks = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_surface_cell_and_tool_preserves_exact_volume_and_material() {
        for side in SIDES {
            for tool in TOOLS {
                for axis in 0..3 {
                    for sign in [false, true] {
                        for u in 0..side {
                            for v in 0..side {
                                let block = Block {
                                    min: [0; 3],
                                    side,
                                    material: 5,
                                };
                                let mut min = [0; 3];
                                min[axis] = if sign { side - tool } else { 0 };
                                min[(axis + 1) % 3] = u;
                                min[(axis + 2) % 3] = v;
                                let cut = Block {
                                    min,
                                    side: tool,
                                    material: 0,
                                };
                                let mut out = Vec::new();
                                subtract(block, cut, &mut out);
                                for b in &out {
                                    assert!(SIDES.contains(&b.side));
                                    assert_eq!(b.material, 5);
                                }
                                for x in 0..side {
                                    for y in 0..side {
                                        for z in 0..side {
                                            let p = [x, y, z];
                                            assert_eq!(
                                                out.iter().filter(|b| b.contains(p)).count(),
                                                usize::from(!cut.contains(p))
                                            );
                                        }
                                    }
                                }
                                assert!(out.iter().all(|b| {
                                    (0..3).all(|a| b.min[a] >= 0 && b.min[a] + b.side <= side)
                                }));
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn fixture_has_every_color_at_every_size_and_small_tiers_are_ghosts() {
        let d = Demo::new();
        assert_eq!(d.blocks.len(), 42);
        for side in SIDES {
            for material in 0..6 {
                assert!(
                    d.blocks
                        .iter()
                        .any(|b| b.side == side && b.material == material)
                );
            }
        }
        assert_eq!(
            SIDES.map(walkable),
            [false, false, false, true, true, true, true]
        );
        let mut out = Vec::new();
        subtract(
            Block {
                min: [0; 3],
                side: 8,
                material: 0,
            },
            Block {
                min: [0; 3],
                side: 1,
                material: 0,
            },
            &mut out,
        );
        assert!(out.iter().any(|b| b.side >= 4));
    }
    #[test]
    fn ray_tools_use_c1_steps_on_every_face_and_cycle_four_sizes() {
        let mut d = Demo {
            blocks: alloc::vec![Block {
                min: [0; 3],
                side: 12,
                material: 3
            }],
            tool: 0,
        };
        for tool in 0..4 {
            d.tool = tool;
            for axis in 0..3 {
                for sign in [-1f32, 1.] {
                    let mut direction = [0.; 3];
                    direction[axis] = -sign;
                    for cell in 0..12 {
                        let mut origin = [1.1 * C1; 3];
                        origin[axis] = if sign > 0. { 20. * C1 } else { -8. * C1 };
                        origin[(axis + 1) % 3] = (cell as f32 + 0.5) * C1;
                        let cut = d.target(origin, direction).unwrap();
                        assert_eq!(cut.side, TOOLS[tool]);
                        assert_eq!(cut.min[(axis + 1) % 3], cell);
                        assert_eq!(cut.min[axis], if sign > 0. { 12 - TOOLS[tool] } else { 0 });
                    }
                }
            }
        }
        d.tool = 0;
        for expected in [1, 2, 3, 0] {
            d.cycle(1);
            assert_eq!(d.tool, expected);
        }
        d.cycle(-1);
        assert_eq!(d.tool, 3);
        assert!(d.target([-1.; 3], [-1., 0., 0.]).is_none());
    }
    #[test]
    fn repeated_cuts_never_restore_mined_cells_or_change_other_blocks() {
        let mut d = Demo {
            blocks: alloc::vec![
                Block {
                    min: [0; 3],
                    side: 8,
                    material: 2
                },
                Block {
                    min: [20; 3],
                    side: 12,
                    material: 5
                }
            ],
            tool: 0,
        };
        let untouched = d.blocks[1];
        for min in [[0, 0, 0], [1, 0, 0], [2, 0, 0], [0, 0, 0]] {
            d.mine(Block {
                min,
                side: 1,
                material: 0,
            });
        }
        assert_eq!(
            d.blocks.iter().map(|b| b.side.pow(3)).sum::<i32>(),
            8i32.pow(3) + 12i32.pow(3) - 3
        );
        assert!(d.blocks.contains(&untouched));
        for x in 0..3 {
            assert!(!d.blocks.iter().any(|b| b.contains([x, 0, 0])));
        }
    }
}
