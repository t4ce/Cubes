//! Short, rate-limited Key4 pop-ins, keyed by authored cube ID.
extern crate alloc;
use alloc::vec::Vec;

pub const DELAY_MS: u64 = 120;
pub const STARTS_PER_SECOND: u64 = 1600;
pub const MAX_STARTS_PER_FRAME: u64 = 96;
pub const REARM_MS: u64 = 240;

#[derive(Clone, Copy, Default)]
enum Phase {
    #[default]
    Hidden,
    Waiting(u64),
    Full,
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
            seed.phase = Phase::Full;
        }
        true
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
            for now in (0..4000).step_by(step) {
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
        assert_eq!(frame(&mut reveal, DELAY_MS, 900, &ids), ids[..96]);
        // Existing cubes remain shown; at most 96 additional cubes start.
        assert_eq!(frame(&mut reveal, 60_000, 900, &ids), ids[..192]);
    }

    #[test]
    fn brief_occlusion_preserves_admission_but_long_absence_rearms_the_delay() {
        let mut reveal = Reveal::new();
        frame(&mut reveal, 0, 1, &[0]);
        frame(&mut reveal, 120, 1, &[0]);
        frame(&mut reveal, 180, 1, &[]);
        assert_eq!(frame(&mut reveal, 220, 1, &[0]), [0]);
        frame(&mut reveal, 300, 1, &[]);
        assert!(frame(&mut reveal, 540, 1, &[0]).is_empty());
        assert_eq!(frame(&mut reveal, 660, 1, &[0]), [0]);
    }

    #[test]
    fn fleeting_exposures_reset_and_reordered_compaction_keeps_authored_identity() {
        let mut reveal = Reveal::new();
        frame(&mut reveal, 0, 3, &[2]);
        frame(&mut reveal, 60, 3, &[]);
        assert!(frame(&mut reveal, 120, 3, &[2]).is_empty());
        assert!(frame(&mut reveal, 180, 3, &[1, 2]).is_empty());
        assert_eq!(frame(&mut reveal, 240, 3, &[2, 1]), [2]);
        assert_eq!(frame(&mut reveal, 300, 3, &[1, 2]), [1, 2]);
    }

    #[test]
    fn page_reset_clears_equal_size_assets_and_empty_scenes() {
        let mut reveal = Reveal::new();
        frame(&mut reveal, 0, 1, &[0]);
        frame(&mut reveal, 120, 1, &[0]);
        reveal.reset();
        assert!(frame(&mut reveal, 200, 1, &[0]).is_empty());
        assert!(frame(&mut reveal, 300, 0, &[]).is_empty());
        assert!(frame(&mut reveal, 400, 1, &[0]).is_empty());
    }
}
