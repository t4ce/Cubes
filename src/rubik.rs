//! Exact cubie identities and lattice orientations; animation never accumulates drift.
pub const PALETTE_FLAG: u32 = 256;
/// Shade an entire key-1 room wall with its matching Rubik palette colour.
pub const ROOM_PALETTE_FLAG: u32 = 1 << 13;
/// Shade Key-3 seeds from their sphere position rather than the base material.
pub const SPHERE_GRADIENT_FLAG: u32 = 1 << 14;
/// Key 7 combines the otherwise-exclusive room and sphere bits. Bits 0..2
/// select one of six fixed metallic/roughness records in the cube shader.
pub const MATERIAL_SHOWCASE_FLAG: u32 = ROOM_PALETTE_FLAG | SPHERE_GRADIENT_FLAG;
const TURN_MS: u64 = 1_000;
pub const OPEN_MS: u64 = 1_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cubie {
    cell: [i8; 3],
    basis: [[i8; 3]; 3],
}
#[derive(Clone, Copy)]
struct Turn {
    axis: usize,
    layer: i8,
    direction: i8,
    started: u64,
}
pub struct Puzzle {
    cubies: [Cubie; 27],
    turn: Option<Turn>,
    selected: Option<usize>,
    opened: u64,
    completed: u8,
    previous_axis: Option<usize>,
    rng: u32,
    revision: u64,
}
fn rotate(mut v: [i8; 3], axis: usize, direction: i8) -> [i8; 3] {
    let a = (axis + 1) % 3;
    let b = (axis + 2) % 3;
    (v[a], v[b]) = (-direction * v[b], direction * v[a]);
    v
}
impl Puzzle {
    pub fn new(now: u64) -> Self {
        Self {
            cubies: core::array::from_fn(|i| Cubie {
                cell: [
                    (i % 3) as i8 - 1,
                    ((i / 3) % 3) as i8 - 1,
                    (i / 9) as i8 - 1,
                ],
                basis: [[1, 0, 0], [0, 1, 0], [0, 0, 1]],
            }),
            turn: None,
            selected: None,
            opened: now,
            completed: 0,
            previous_axis: None,
            rng: 0x6d2b79f5,
            revision: 0,
        }
    }
    fn commit(&mut self, turn: Turn) {
        for cubie in &mut self.cubies {
            if cubie.cell[turn.axis] == turn.layer {
                cubie.cell = rotate(cubie.cell, turn.axis, turn.direction);
                cubie.basis = cubie.basis.map(|v| rotate(v, turn.axis, turn.direction));
            }
        }
        self.revision = self.revision.wrapping_add(1);
    }
    /// Only completed lattice turns change world adjacency.
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn lattice_pose(&self, id: usize) -> ([i8; 3], [[i8; 3]; 3]) {
        (self.cubies[id].cell, self.cubies[id].basis)
    }
    pub fn identity_at(&self, cell: [i8; 3]) -> Option<usize> {
        self.cubies.iter().position(|cubie| cubie.cell == cell)
    }
    /// Re-enter the Key-2 demo without resetting its shared permutation. An
    /// in-progress action continues; a finished action can be selected again.
    pub fn reenter(&mut self, now: u64) {
        if !self.locked() {
            self.selected = None;
            self.opened = now;
            self.completed = 0;
            self.previous_axis = None;
        }
    }
    pub fn update(&mut self, now: u64) {
        let Some(id) = self.selected else {
            return;
        };
        if now < self.opened + OPEN_MS || self.completed == 3 {
            return;
        }
        if let Some(turn) = self.turn {
            if now.saturating_sub(turn.started) < TURN_MS {
                return;
            }
            self.commit(turn);
            self.turn = None;
            self.completed += 1;
            if self.completed == 3 {
                return;
            }
        }
        {
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 17;
            self.rng ^= self.rng << 5;
            let cell = self.cubies[id].cell;
            let axes: [usize; 3] = core::array::from_fn(|i| ((self.rng as usize % 3) + i) % 3);
            let axis = *axes
                .iter()
                .find(|&&a| cell[a] != 0 && self.previous_axis != Some(a))
                .or_else(|| axes.iter().find(|&&a| cell[a] != 0))
                .unwrap();
            self.previous_axis = Some(axis); // centers keep direction; other pieces change axes
            self.turn = Some(Turn {
                axis,
                layer: cell[axis],
                direction: if cell.iter().filter(|&&x| x != 0).count() == 1 {
                    1
                } else if self.rng & 16 == 0 {
                    -1
                } else {
                    1
                },
                started: now,
            });
        }
    }
    pub fn select(&mut self, id: usize, now: u64) -> bool {
        if self.selected.is_some()
            || id >= 27
            || self.cubies[id].cell.iter().filter(|&&x| x != 0).count() < 1
        {
            return false;
        }
        self.selected = Some(id);
        self.opened = now;
        self.rng ^= now as u32 ^ ((id as u32 + 1) * 7919);
        true
    }
    /// Cancel an automatic journey without altering committed lattice poses.
    pub fn cancel_travel(&mut self, now: u64) {
        self.turn = None;
        self.selected = None;
        self.completed = 0;
        self.opened = now;
        self.previous_axis = None;
    }
    /// One portal-travel turn moves both adjacent worlds in the same layer.
    pub fn travel_turn(&mut self, source: usize, destination: usize, now: u64) {
        let a = self.cubies[source].cell;
        let b = self.cubies[destination].cell;
        let axis = (0..3).find(|&axis| a[axis] == b[axis]).unwrap();
        self.selected = Some(source);
        self.opened = now;
        self.completed = 2;
        self.previous_axis = Some(axis);
        self.turn = Some(Turn {
            axis,
            layer: a[axis],
            direction: 1,
            started: now,
        });
    }
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }
    pub fn locked(&self) -> bool {
        self.selected.is_some() && self.completed < 3
    }
    pub fn expansion(&self, now: u64) -> f32 {
        if self.selected.is_none() {
            return 0.0;
        }
        let p = (now.saturating_sub(self.opened) as f32 / OPEN_MS as f32).clamp(0.0, 1.0);
        p * p * (3.0 - 2.0 * p)
    }
    pub fn angle(&self, now: u64) -> f32 {
        self.turn.map_or(0.0, |t| {
            let p = (now.saturating_sub(t.started) as f32 / TURN_MS as f32).clamp(0.0, 1.0);
            (p * p * (3.0 - 2.0 * p)) * core::f32::consts::FRAC_PI_2 * t.direction as f32
        })
    }
    /// Position in lattice units and orientation columns; caller supplies sin/cos.
    pub fn pose(&self, id: usize, sin: f32, cos: f32) -> ([f32; 3], [[f32; 3]; 3]) {
        let cubie = self.cubies[id];
        let transform = |v: [i8; 3]| {
            let mut v = v.map(|x| x as f32);
            if let Some(t) = self.turn {
                if cubie.cell[t.axis] == t.layer {
                    let a = (t.axis + 1) % 3;
                    let b = (t.axis + 2) % 3;
                    (v[a], v[b]) = (cos * v[a] - sin * v[b], sin * v[a] + cos * v[b]);
                }
            }
            v
        };
        (transform(cubie.cell), cubie.basis.map(transform))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn four_turns_restore_every_identity_and_orientation() {
        for axis in 0..3 {
            for layer in [-1, 1] {
                for direction in [-1, 1] {
                    let mut p = Puzzle::new(0);
                    let original = p.cubies;
                    let t = Turn {
                        axis,
                        layer,
                        direction,
                        started: 0,
                    };
                    p.commit(t);
                    assert_eq!(
                        p.cubies
                            .iter()
                            .zip(original)
                            .filter(|(a, b)| **a != *b)
                            .count(),
                        9
                    );
                    for _ in 0..3 {
                        p.commit(t);
                    }
                    assert_eq!(p.cubies, original);
                }
            }
        }
    }
    #[test]
    fn scramble_preserves_unique_cells_and_outward_stickers() {
        let mut p = Puzzle::new(0);
        for n in 1..200 {
            p.commit(Turn {
                axis: n as usize % 3,
                layer: if n % 2 == 0 { 1 } else { -1 },
                direction: 1,
                started: 0,
            });
            for i in 0..27 {
                for j in 0..i {
                    assert_ne!(p.cubies[i].cell, p.cubies[j].cell);
                }
                let original = Puzzle::new(0).cubies[i].cell;
                for axis in 0..3 {
                    if original[axis] != 0 {
                        let normal = p.cubies[i].basis[axis].map(|x| x * original[axis]);
                        assert_eq!(
                            normal
                                .iter()
                                .zip(p.cubies[i].cell)
                                .map(|(a, b)| a * b)
                                .sum::<i8>(),
                            1
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn selection_runs_exactly_three_consecutive_noninverse_turns() {
        let mut p = Puzzle::new(100);
        p.update(50_000);
        assert!(p.turn.is_none());
        assert!(!p.select(13, 50_000));

        assert!(p.select(0, 50_000));
        assert!(!p.select(2, 50_000));
        let mut previous = None;
        for i in 0..3 {
            p.update(51_000 + i * TURN_MS);
            let t = p.turn.unwrap();
            assert_ne!(Some(t.axis), previous);
            previous = Some(t.axis);
            assert_eq!(p.cubies[0].cell[t.axis], t.layer);
        }
        p.update(54_000);
        assert!(!p.locked());
        assert_eq!(p.completed, 3);
        let final_state = p.cubies;
        p.update(100_000);
        assert_eq!(p.cubies, final_state);
    }
}
