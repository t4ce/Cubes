// Begin the camera flight as soon as the third turn commits, then ease into
// the selected world through a short fade instead of pausing between stages.
pub const WAIT_MS: u64 = 0;
pub const FLIGHT_MS: u64 = 3500;
pub struct Flight {
    pub started: u64,
    pub rotation: [f32; 4],
    pub up: [f32; 3],
    pub points: [[f32; 3]; 4],
}
pub const FADE_MS: u64 = 700;
pub const REVEAL_MS: u64 = 900;
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
pub fn reveal_opacity(elapsed: u64) -> u8 {
    (255. * smooth(elapsed as f32 / REVEAL_MS as f32)) as u8
}
impl Flight {
    pub fn orientation_blend(&self, now: u64) -> f32 {
        smooth(now.saturating_sub(self.started) as f32 / 450.)
    }
    pub fn opacity(&self, now: u64) -> u8 {
        let elapsed = now
            .saturating_sub(self.started)
            .saturating_sub(FLIGHT_MS - FADE_MS);
        255 - reveal_opacity(elapsed * REVEAL_MS / FADE_MS)
    }
    pub fn position(&self, now: u64) -> [f32; 3] {
        let t = (now.saturating_sub(self.started) as f32 / FLIGHT_MS as f32).clamp(0., 1.);
        let t = t * t * (3. - 2. * t);
        let u = 1. - t;
        core::array::from_fn(|i| {
            u * u * u * self.points[0][i]
                + 3. * u * u * t * self.points[1][i]
                + 3. * u * t * t * self.points[2][i]
                + t * t * t * self.points[3][i]
        })
    }
    pub fn done(&self, now: u64) -> bool {
        now.saturating_sub(self.started) >= FLIGHT_MS
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flight_is_bounded_and_reaches_selected_center() {
        let f = Flight {
            started: 3000,
            rotation: [0., 0., 0., 1.],
            up: [0., -1., 0.],
            points: [[0., 0., -7.5], [2., -1., -5.], [1., 1., -2.], [1., 1., -1.]],
        };
        assert_eq!(f.position(3000), f.points[0]);
        assert_eq!(f.position(6500), f.points[3]);
        assert!(!f.done(6499));
        assert!(f.done(6500));
        for n in 3000..6500 {
            assert!(f.position(n).iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn orientation_handoff_and_fades_have_continuous_endpoints() {
        let f = Flight {
            started: 100,
            rotation: [0., 0., 0., 1.],
            up: [0., -1., 0.],
            points: [[0.; 3]; 4],
        };
        assert_eq!(f.orientation_blend(100), 0.);
        assert_eq!(f.orientation_blend(550), 1.);
        assert_eq!(f.opacity(100), 255);
        assert_eq!(f.opacity(100 + FLIGHT_MS - FADE_MS), 255);
        assert_eq!(f.opacity(100 + FLIGHT_MS), 0);
        assert_eq!(reveal_opacity(0), 0);
        assert_eq!(reveal_opacity(REVEAL_MS), 255);
        for t in 100..100 + FLIGHT_MS {
            assert!(f.opacity(t) >= f.opacity(t + 1));
        }
    }
    #[test]
    fn post_turn_pause_is_removed_and_flight_is_extended() {
        assert_eq!(WAIT_MS, 0);
        assert_eq!(FLIGHT_MS, 3_500);
    }
}
