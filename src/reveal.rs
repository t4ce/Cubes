//! Rate-limited Key4/Key5 reveals, keyed by authored cube ID.
extern crate alloc;
use alloc::vec::Vec;

pub const DELAY_MS: u64 = 333;
pub const STARTS_PER_SECOND: u64 = 1600;
pub const MAX_STARTS_PER_FRAME: u64 = 32;
pub const REARM_MS: u64 = 240;
/// Key5 placement experiment. False restores full-size pop-in after admission.
pub const PLACED_BOUNCE_UNIFORM_GROWTH: bool = true;
pub const GROWTH_MS: u64 = 700;

/// Uniform acceleration / constant speed / deceleration, then two shrinking
/// bounce dips. No overshoot beyond the authored bounds and no transcendental math.
pub(crate) fn bounce_uniform(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    if t < 0.65 {
        let u = t / 0.65;
        if u < 0.25 {
            (8. / 3.) * u * u
        } else if u < 0.75 {
            (4. / 3.) * (u - 0.125)
        } else {
            1. - (8. / 3.) * (1. - u) * (1. - u)
        }
    } else {
        let (u, depth) = if t < 0.86 {
            ((t - 0.65) / 0.21, 0.16)
        } else {
            ((t - 0.86) / 0.14, 0.04)
        };
        1. - 4. * depth * u * (1. - u)
    }
}

#[derive(Clone, Copy, Default)]
enum Phase {
    #[default]
    Hidden,
    Waiting(u64),
    Full(u64),
}

#[derive(Clone, Copy, Default)]
struct Seed {
    phase: Phase,
    absent_since: Option<u64>,
    seen: bool,
}

pub struct Reveal {
    seeds: Vec<Seed>,
    now: u64,
    previous_frame: Option<u64>,
    // Thousandths of one start; retained fractions make the rate FPS-independent
    // until the per-frame burst cap becomes the limiting factor.
    credit: u64,
}

impl Reveal {
    pub fn new() -> Self {
        Self {
            seeds: Vec::new(),
            now: 0,
            previous_frame: None,
            credit: 0,
        }
    }

    /// Reset on every Key4 entry or asset-page change, including equal-size pages.
    pub fn reset(&mut self) {
        self.seeds.clear();
        self.previous_frame = None;
        self.credit = 0;
    }

    /// Adding placed cubes preserves the reveal state of existing instances.
    pub fn append(&mut self, source: usize) {
        self.seeds.resize(source, Seed::default());
    }

    pub fn reset_range(&mut self, range: core::ops::Range<usize>) {
        for seed in &mut self.seeds[range] {
            *seed = Seed::default();
        }
    }
    pub fn linear_scale(&self, id: usize) -> f32 {
        match self.seeds[id].phase {
            Phase::Full(start) => {
                (self.now.saturating_sub(start) as f32 / GROWTH_MS as f32).min(1.)
            }
            _ => 0.,
        }
    }
    pub fn begin_frame(&mut self, now: u64, source: usize) {
        if self.seeds.len() != source {
            self.reset();
            self.seeds.resize(source, Seed::default());
        }
        let elapsed = now.saturating_sub(self.previous_frame.unwrap_or(now));
        self.credit = self
            .credit
            .saturating_add(elapsed.saturating_mul(STARTS_PER_SECOND))
            .min(MAX_STARTS_PER_FRAME * 1000);
        self.previous_frame = Some(now);
        self.now = now;
        for seed in &mut self.seeds {
            seed.seen = false;
        }
    }

    /// Called once per eligible seed in the culler's nearest-first order.
    /// False means no submitted seed and therefore no occlusion coverage.
    pub fn admit(&mut self, id: usize) -> bool {
        let seed = &mut self.seeds[id];
        seed.seen = true;
        if seed
            .absent_since
            .is_some_and(|since| self.now.saturating_sub(since) >= REARM_MS)
        {
            seed.phase = Phase::Hidden;
        }
        seed.absent_since = None;
        if matches!(seed.phase, Phase::Hidden) {
            seed.phase = Phase::Waiting(self.now);
        }
        if let Phase::Waiting(since) = seed.phase {
            if self.now.saturating_sub(since) < DELAY_MS || self.credit < 1000 {
                return false;
            }
            self.credit -= 1000;
            seed.phase = Phase::Full(self.now);
        }
        true
    }

    /// Render-only scale; physics and authored geometry always retain full size.
    pub fn growth_scale(&self, id: usize) -> f32 {
        if !PLACED_BOUNCE_UNIFORM_GROWTH {
            return 1.;
        }
        match self.seeds[id].phase {
            Phase::Full(start) => {
                bounce_uniform(self.now.saturating_sub(start) as f32 / GROWTH_MS as f32)
            }
            _ => 0.,
        }
    }

    pub fn settled(&self, id: usize) -> bool {
        !PLACED_BOUNCE_UNIFORM_GROWTH
            || matches!(self.seeds[id].phase,
            Phase::Full(start) if self.now.saturating_sub(start) >= GROWTH_MS)
    }

    pub fn end_frame(&mut self) {
        for seed in &mut self.seeds {
            if !seed.seen {
                if matches!(seed.phase, Phase::Waiting(_)) {
                    // A fleeting exposure must not accumulate admission time.
                    seed.phase = Phase::Hidden;
                }
                seed.absent_since.get_or_insert(self.now);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(reveal: &mut Reveal, now: u64, source: usize, ids: &[usize]) -> Vec<usize> {
        reveal.begin_frame(now, source);
        let drawn = ids.iter().copied().filter(|&id| reveal.admit(id)).collect();
        reveal.end_frame();
        drawn
    }

    #[test]
    fn pop_in_waits_for_the_delay_then_stays_fully_admitted() {
        let mut reveal = Reveal::new();
        assert!(frame(&mut reveal, 0, 1, &[0]).is_empty());
        assert!(frame(&mut reveal, DELAY_MS - 1, 1, &[0]).is_empty());
        assert_eq!(frame(&mut reveal, DELAY_MS, 1, &[0]), [0]);
        assert_eq!(frame(&mut reveal, DELAY_MS + 1, 1, &[0]), [0]);
        assert_eq!(frame(&mut reveal, 60_000, 1, &[0]), [0]);
    }

    #[test]
    fn rate_and_burst_limits_hold_at_different_frame_rates() {
        for step in [7, 16, 67, 100, 333] {
            let mut reveal = Reveal::new();
            let ids: Vec<_> = (0..900).collect();
            let mut last = 0;
            for now in (0..12000).step_by(step) {
                let drawn = frame(&mut reveal, now, 900, &ids).len();
                assert!(drawn - last <= MAX_STARTS_PER_FRAME as usize);
                assert!(drawn as u64 * 1000 <= now * STARTS_PER_SECOND);
                last = drawn;
            }
            assert_eq!(last, 900);
        }
    }

    #[test]
    fn nearest_eligible_seeds_start_first_and_stalls_cannot_burst_the_whole_scene() {
        let mut reveal = Reveal::new();
        let ids: Vec<_> = (0..900).rev().collect();
        frame(&mut reveal, 0, 900, &ids);
        assert_eq!(frame(&mut reveal, DELAY_MS, 900, &ids), ids[..32]);
        // Existing cubes remain shown; at most 32 additional cubes start.
        assert_eq!(frame(&mut reveal, 60_000, 900, &ids), ids[..64]);
    }

    #[test]
    fn brief_occlusion_preserves_admission_but_long_absence_rearms_the_delay() {
        let mut reveal = Reveal::new();
        frame(&mut reveal, 0, 1, &[0]);
        frame(&mut reveal, 333, 1, &[0]);
        frame(&mut reveal, 380, 1, &[]);
        assert_eq!(frame(&mut reveal, 420, 1, &[0]), [0]);
        frame(&mut reveal, 460, 1, &[]);
        assert!(frame(&mut reveal, 700, 1, &[0]).is_empty());
        assert_eq!(frame(&mut reveal, 1033, 1, &[0]), [0]);
    }

    #[test]
    fn fleeting_exposures_reset_and_reordered_compaction_keeps_authored_identity() {
        let mut reveal = Reveal::new();
        frame(&mut reveal, 0, 3, &[2]);
        frame(&mut reveal, 60, 3, &[]);
        assert!(frame(&mut reveal, 120, 3, &[2]).is_empty());
        assert!(frame(&mut reveal, 180, 3, &[1, 2]).is_empty());
        assert_eq!(frame(&mut reveal, 453, 3, &[2, 1]), [2]);
        assert_eq!(frame(&mut reveal, 513, 3, &[1, 2]), [1, 2]);
    }

    #[test]
    fn appending_placement_does_not_restart_existing_reveal() {
        let mut reveal = Reveal::new();
        frame(&mut reveal, 0, 1, &[0]);
        assert_eq!(frame(&mut reveal, 333, 1, &[0]), [0]);
        reveal.append(2);
        assert_eq!(frame(&mut reveal, 353, 2, &[0, 1]), [0]);
        assert_eq!(frame(&mut reveal, 686, 2, &[0, 1]), [0, 1]);
    }
    #[test]
    fn page_reset_clears_equal_size_assets_and_empty_scenes() {
        let mut reveal = Reveal::new();
        frame(&mut reveal, 0, 1, &[0]);
        frame(&mut reveal, DELAY_MS, 1, &[0]);
        reveal.reset();
        assert!(frame(&mut reveal, 200, 1, &[0]).is_empty());
        assert!(frame(&mut reveal, 300, 0, &[]).is_empty());
        assert!(frame(&mut reveal, 400, 1, &[0]).is_empty());
    }
    #[test]
    fn growth_is_bounded_bounces_and_settles_at_exact_authored_size() {
        assert_eq!(bounce_uniform(0.), 0.);
        assert!(bounce_uniform(0.3) < bounce_uniform(0.5));
        assert!(bounce_uniform(0.755) < bounce_uniform(0.65));
        assert!(bounce_uniform(0.93) > bounce_uniform(0.755));
        for i in 0..=1000 {
            assert!((0. ..=1.).contains(&bounce_uniform(i as f32 / 1000.)));
        }
        let mut reveal = Reveal::new();
        frame(&mut reveal, 0, 1, &[0]);
        frame(&mut reveal, DELAY_MS, 1, &[0]);
        if !PLACED_BOUNCE_UNIFORM_GROWTH {
            assert_eq!(reveal.growth_scale(0), 1.);
            assert!(reveal.settled(0));
            return;
        }
        assert_eq!(reveal.growth_scale(0), 0.);
        assert!(!reveal.settled(0));
        frame(&mut reveal, DELAY_MS + GROWTH_MS / 2, 1, &[0]);
        assert!((0. ..1.).contains(&reveal.growth_scale(0)));
        assert!(!reveal.settled(0));
        frame(&mut reveal, DELAY_MS + GROWTH_MS, 1, &[0]);
        assert_eq!(reveal.growth_scale(0), 1.);
        assert!(reveal.settled(0));
    }
}
