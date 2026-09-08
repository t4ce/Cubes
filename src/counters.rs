//! Allocation-free quarter-second count samples, one line per completed second.
use core::fmt;
#[derive(Clone, Copy, Default)]
struct Sample {
    at: u64,
    mode: u8,
    expanded: usize,
    unexpanded: usize,
    frames: u32,
}
pub struct Report {
    start: u64,
    samples: [Sample; 4],
}
pub struct Sampler {
    start: u64,
    samples: [Sample; 4],
}
impl Sampler {
    pub fn new(now: u64) -> Self {
        Self {
            start: now,
            samples: [Sample::default(); 4],
        }
    }
    pub fn record(
        &mut self,
        now: u64,
        mode: u8,
        expanded: usize,
        unexpanded: usize,
    ) -> Option<Report> {
        let elapsed = now.saturating_sub(self.start);
        let report = if elapsed >= 1000 {
            let report = Report {
                start: self.start,
                samples: self.samples,
            };
            // No fabricated catch-up samples or burst of logs after a stall.
            self.start += elapsed / 1000 * 1000;
            self.samples = [Sample::default(); 4];
            Some(report)
        } else {
            None
        };
        let index = (now.saturating_sub(self.start) / 250).min(3) as usize;
        let frames = self.samples[index].frames.saturating_add(1);
        self.samples[index] = Sample {
            at: now - self.start,
            mode,
            expanded,
            unexpanded,
            frames,
        };
        report
    }
}
impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "counts t={}ms bins=250ms", self.start)?;
        for (i, s) in self.samples.iter().enumerate() {
            if s.frames == 0 {
                write!(f, " [{}:-]", i)?;
            } else {
                write!(
                    f,
                    " [{}:+{}ms M{} E{} U{} F{}]",
                    i, s.at, s.mode, s.expanded, s.unexpanded, s.frames
                )?;
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn emits_four_bins_once_per_second() {
        let mut s = Sampler::new(0);
        for t in [0, 100, 250, 500, 750, 999] {
            assert!(s.record(t, 2, 27, 0).is_none());
        }
        let r = s.record(1000, 4, 900, 0).unwrap();
        assert_eq!(r.samples.map(|s| s.frames), [2, 1, 1, 2]);
        assert_eq!(r.samples[3].at, 999);
        assert_eq!(s.samples[0].mode, 4);
        assert!(s.record(1100, 4, 900, 0).is_none());
    }
    #[test]
    fn stalls_leave_missing_bins_not_fake_datapoints() {
        let mut s = Sampler::new(0);
        s.record(20, 1, 3, 597);
        let r = s.record(4300, 1, 5, 595).unwrap();
        assert_eq!(r.samples.map(|s| s.frames), [1, 0, 0, 0]);
        assert_eq!(s.start, 4000);
        assert!(s.record(4301, 1, 5, 595).is_none());
    }
}
