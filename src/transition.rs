// Begin the camera flight as soon as the third turn commits, then ease into
// the room over an additional second instead of pausing between the stages.
pub const WAIT_MS: u64 = 0;
pub const FLIGHT_MS: u64 = 3500;
pub struct Flight {
    pub started: u64,
    pub points: [[f32; 3]; 4],
}
impl Flight {
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
            points: [[0., 0., -7.5], [2., -1., -5.], [1., 1., -2.], [1., 1., -1.]],
        };
        assert_eq!(f.position(3000), f.points[0]);
        assert_eq!(f.position(5500), f.points[3]);
        assert!(!f.done(5499));
        assert!(f.done(5500));
        for n in 3000..5500 {
            assert!(f.position(n).iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn post_turn_pause_is_removed_and_flight_is_extended() {
        assert_eq!(WAIT_MS, 0);
        assert_eq!(FLIGHT_MS, 3_500);
    }
}
