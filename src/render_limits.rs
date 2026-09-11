//! Session-local rendering budgets and the Key9 cube sliders.
use crate::orchard::{CUSTOM_RGB555, Cube};
use alloc::{format, vec::Vec};
pub const STEPS: usize = 16;
pub const FULL_MIN: usize = 768;
pub const FULL_MAX: usize = 3840;
pub const SEED_MIN: usize = 2048;
pub const SEED_MAX: usize = 8192;
pub const LEFT: f32 = -6.;
pub const RIGHT: f32 = 6.;
pub const ROW_Y: [f32; 2] = [-1.5, 1.5];

pub struct Limits {
    steps: [usize; 2],
    drag: Option<usize>,
    drag_offset: f32,
    pub cubes: Vec<Cube>,
}
impl Limits {
    pub fn new() -> Self {
        let mut out = Self {
            steps: [STEPS - 1; 2],
            drag: None,
            drag_offset: 0.,
            cubes: Vec::new(),
        };
        out.rebuild();
        out
    }
    pub fn full(&self) -> usize {
        value(FULL_MIN, FULL_MAX, self.steps[0])
    }
    pub fn seeds(&self) -> usize {
        value(SEED_MIN, SEED_MAX, self.steps[1])
    }
    /// Reserve overlays/face seeds before admitting scene cubes. This is shared
    /// by visibility and geometry selection, so neither can overrun the frame.
    pub fn scene_budget(&self, reserved_full: usize, reserved_seeds: usize) -> (usize, usize) {
        let seeds = self.seeds().saturating_sub(reserved_seeds);
        (self.full().saturating_sub(reserved_full).min(seeds), seeds)
    }
    pub fn cancel_drag(&mut self) {
        self.drag = None;
    }
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }
    /// Ray intersection with the slider plane. An active drag clamps beyond the
    /// strip ends; moving between rows never captures the other slider.
    pub fn pointer(
        &mut self,
        origin: [f32; 3],
        direction: [f32; 3],
        pressed: bool,
        down: bool,
    ) -> bool {
        if !down {
            self.cancel_drag();
            return false;
        }
        if !origin.iter().chain(direction.iter()).all(|v| v.is_finite())
            || direction[2].abs() < 1e-6
        {
            return false;
        }
        let t = -origin[2] / direction[2];
        if t < 0. {
            return false;
        }
        let x = origin[0] + direction[0] * t;
        let y = origin[1] + direction[1] * t;
        if pressed {
            // The marker is in front of the strip. Capture it at its own depth
            // and preserve the grab offset, avoiding a jump on perspective clicks.
            let marker_t = (-0.65 - origin[2]) / direction[2];
            let mx = origin[0] + direction[0] * marker_t;
            let my = origin[1] + direction[1] * marker_t;
            self.drag = (0..2).find(|&row| {
                (mx - step_x(self.steps[row])).abs() <= 0.35 && (my - ROW_Y[row]).abs() <= 0.35
            });
            self.drag_offset = self.drag.map_or(0., |row| x - step_x(self.steps[row]));
            if self.drag.is_none() && x >= LEFT - 0.5 && x <= RIGHT + 0.5 {
                self.drag = ROW_Y.iter().position(|row| (y - row).abs() <= 0.5);
            }
        }
        let Some(row) = self.drag else {
            return false;
        };
        let step = libm::roundf(
            ((x - self.drag_offset - LEFT) / (RIGHT - LEFT)).clamp(0., 1.) * (STEPS - 1) as f32,
        ) as usize;
        if self.steps[row] == step {
            return false;
        }
        self.steps[row] = step;
        self.rebuild();
        true
    }
    fn rebuild(&mut self) {
        self.cubes.clear();
        for row in 0..2 {
            let color = if row == 0 {
                rgb(8, 26, 31)
            } else {
                rgb(31, 20, 5)
            };
            for step in 0..STEPS {
                self.cubes.push(Cube {
                    center: [step_x(step), ROW_Y[row], 0.],
                    scale: 0.19,
                    flags: if step <= self.steps[row] {
                        color
                    } else {
                        rgb(7, 9, 11)
                    },
                });
            }
            self.cubes.push(Cube {
                center: [step_x(self.steps[row]), ROW_Y[row], -0.65],
                scale: 0.3,
                flags: rgb(31, 31, 31),
            });
            let (name, number, low, high) = if row == 0 {
                ("FULL", self.full(), FULL_MIN, FULL_MAX)
            } else {
                ("SEEDS", self.seeds(), SEED_MIN, SEED_MAX)
            };
            text(
                &mut self.cubes,
                &format!("{name} {number}"),
                [LEFT, ROW_Y[row] - 1., 0.],
                0.1,
                color,
            );
            text(
                &mut self.cubes,
                &format!("{low}"),
                [LEFT, ROW_Y[row] + 0.65, 0.],
                0.065,
                rgb(17, 20, 24),
            );
            text(
                &mut self.cubes,
                &format!("{high}"),
                [RIGHT - 1.5, ROW_Y[row] + 0.65, 0.],
                0.065,
                rgb(17, 20, 24),
            );
        }
    }
}
pub fn value(low: usize, high: usize, step: usize) -> usize {
    low + ((high - low) * step.min(STEPS - 1) + (STEPS - 1) / 2) / (STEPS - 1)
}
pub fn step_x(step: usize) -> f32 {
    LEFT + (RIGHT - LEFT) * step as f32 / (STEPS - 1) as f32
}
pub fn camera_distance(width: u32, height: u32, yfov: f32) -> f32 {
    let tan = libm::tanf(yfov * 0.5);
    (7. / (width.max(1) as f32 / height.max(1) as f32 * tan)).max(3.2 / tan) + 1.
}
fn rgb(r: u32, g: u32, b: u32) -> u32 {
    CUSTOM_RGB555 | r | (g << 5) | (b << 10)
}
fn text(cubes: &mut Vec<Cube>, text: &str, origin: [f32; 3], pitch: f32, flags: u32) {
    for (i, c) in text.bytes().enumerate() {
        let rows = match c {
            b'0' => [7, 5, 5, 5, 7],
            b'1' => [2, 6, 2, 2, 7],
            b'2' => [7, 1, 7, 4, 7],
            b'3' => [7, 1, 7, 1, 7],
            b'4' => [5, 5, 7, 1, 1],
            b'5' => [7, 4, 7, 1, 7],
            b'6' => [7, 4, 7, 5, 7],
            b'7' => [7, 1, 1, 1, 1],
            b'8' => [7, 5, 7, 5, 7],
            b'9' => [7, 5, 7, 1, 7],
            b'F' => [7, 4, 6, 4, 4],
            b'U' => [5, 5, 5, 5, 7],
            b'L' => [4, 4, 4, 4, 7],
            b'S' => [7, 4, 7, 1, 7],
            b'E' => [7, 4, 6, 4, 7],
            b'D' => [6, 5, 5, 5, 6],
            _ => [0; 5],
        };
        for (y, bits) in rows.into_iter().enumerate() {
            for x in 0..3 {
                if bits & (1 << (2 - x)) != 0 {
                    cubes.push(Cube {
                        center: [
                            origin[0] + (i * 4 + x) as f32 * pitch,
                            origin[1] + y as f32 * pitch,
                            origin[2],
                        ],
                        scale: pitch * 0.42,
                        flags,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn set(limits: &mut Limits, row: usize, step: usize) {
        // Target the strip's back plane, away from the protruding marker.
        limits.pointer(
            [step_x(step), ROW_Y[row] + 0.4, -10.],
            [0., 0., 1.],
            true,
            true,
        );
        limits.cancel_drag();
    }
    #[test]
    fn all_steps_endpoints_and_ui_fit_the_minimum_budget() {
        let mut l = Limits::new();
        assert_eq!((l.full(), l.seeds()), (FULL_MAX, SEED_MAX));
        for row in 0..2 {
            let mut previous = 0;
            for step in 0..STEPS {
                set(&mut l, row, step);
                let current = if row == 0 { l.full() } else { l.seeds() };
                assert!(current > previous);
                previous = current;
                assert!(l.cubes.len() < FULL_MIN && l.cubes.len() < SEED_MIN);
                assert!(
                    l.cubes
                        .iter()
                        .all(|c| c.scale >= 0.001 && c.center.iter().all(|v| v.is_finite()))
                );
            }
        }
        set(&mut l, 0, 0);
        set(&mut l, 1, 0);
        assert_eq!((l.full(), l.seeds()), (FULL_MIN, SEED_MIN));
    }
    #[test]
    fn drag_locks_row_clamps_ends_and_release_or_focus_loss_stops_changes() {
        let mut l = Limits::new();
        assert!(!l.pointer([0., 0., -10.], [0., 0., 1.], true, true));
        assert!(!l.dragging());
        assert!(l.pointer([0., ROW_Y[0], -10.], [0., 0., 1.], true, true));
        l.pointer([-100., ROW_Y[1], -10.], [0., 0., 1.], false, true);
        assert_eq!((l.full(), l.seeds()), (FULL_MIN, SEED_MAX));
        l.pointer([100., ROW_Y[1], -10.], [0., 0., 1.], false, true);
        assert_eq!(l.full(), FULL_MAX);
        l.pointer([0., 0., -10.], [0., 0., 1.], false, false);
        l.pointer([LEFT, ROW_Y[0], -10.], [0., 0., 1.], false, true);
        assert_eq!(l.full(), FULL_MAX);
        l.cancel_drag();
        assert!(!l.dragging());
        assert!(!l.pointer([0.; 3], [f32::NAN, 0., 1.], true, true));
        assert!(!l.pointer([0.; 3], [1., 0., 0.], true, true));
    }
    #[test]
    fn grabbing_marker_at_perspective_depth_does_not_jump() {
        let mut l = Limits::new();
        set(&mut l, 0, 13);
        let before = l.full();
        let eye = [0., 0., -7.];
        let target = [step_x(13), ROW_Y[0], -0.65];
        let direction = core::array::from_fn(|a| target[a] - eye[a]);
        assert!(!l.pointer(eye, direction, true, true));
        assert!(l.dragging());
        assert_eq!(l.full(), before);
    }
    #[test]
    fn every_budget_pair_reserves_overlays_and_never_overruns() {
        let mut l = Limits::new();
        for full in 0..STEPS {
            for seeds in 0..STEPS {
                set(&mut l, 0, full);
                set(&mut l, 1, seeds);
                for (reserved_full, reserved_seeds) in
                    [(0, 0), (384, 384 + 768), (384 + 27, 384 + 768 + 81), (1, 1)]
                {
                    let (detail, count) = l.scene_budget(reserved_full, reserved_seeds);
                    assert!(count + reserved_seeds <= l.seeds());
                    assert!(detail + reserved_full <= l.full());
                    assert!(detail <= count);
                }
            }
        }
        assert_eq!(l.scene_budget(10000, 10000), (0, 0));
    }
    #[test]
    fn static_camera_fits_both_sliders_after_resize() {
        let fov = core::f32::consts::FRAC_PI_4;
        for (width, height) in [(784, 441), (400, 2000), (3000, 400), (1, 1)] {
            let distance = camera_distance(width, height, fov);
            let vertical = (distance - 0.95) * libm::tanf(fov * 0.5);
            let horizontal = vertical * width as f32 / height as f32;
            assert!(vertical > 3. && horizontal > 6.35);
        }
    }
}
