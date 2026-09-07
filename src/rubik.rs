//! Exact cubie identities and lattice orientations; animation never accumulates drift.
pub const PALETTE_FLAG: u32 = 256;
/// Start one quarter-turn every five seconds, leaving four seconds to view it at rest.
const CADENCE: u64 = 5_000;
const TURN_MS: u64 = 1_000;
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
    next: u64,
    rng: u32,
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
            next: now + CADENCE,
            rng: 0x6d2b79f5,
        }
    }
    fn commit(&mut self, turn: Turn) {
        for cubie in &mut self.cubies {
            if cubie.cell[turn.axis] == turn.layer {
                cubie.cell = rotate(cubie.cell, turn.axis, turn.direction);
                cubie.basis = cubie.basis.map(|v| rotate(v, turn.axis, turn.direction));
            }
        }
    }
    pub fn update(&mut self, now: u64) {
        if let Some(turn) = self.turn {
            if now.saturating_sub(turn.started) < TURN_MS {
                return;
            }
            self.commit(turn);
            self.turn = None;
        }
        if now >= self.next {
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 17;
            self.rng ^= self.rng << 5;
            self.turn = Some(Turn {
                axis: (self.rng % 3) as usize,
                layer: if self.rng & 8 == 0 { -1 } else { 1 },
                direction: if self.rng & 16 == 0 { -1 } else { 1 },
                started: now,
            });
            self.next = now + CADENCE; // no catch-up burst after a stall
        }
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
            p.update(n * CADENCE);
            p.update(n * CADENCE + TURN_MS);
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
    fn starts_ordered_then_turns_once_every_five_seconds_for_one_second() {
        let mut p = Puzzle::new(100);
        p.update(5_099);
        assert!(p.turn.is_none());
        p.update(5_100);
        assert!(p.turn.is_some());
        assert_eq!(p.angle(5_100), 0.0);
        p.update(6_099);
        assert!(p.turn.is_some());
        p.update(6_100);
        assert!(p.turn.is_none());
        p.update(10_099);
        assert!(p.turn.is_none());
        p.update(10_100);
        assert!(p.turn.is_some());
    }
}
