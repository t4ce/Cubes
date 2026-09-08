//! Allocation-free quarter-second count samples, one line per completed second.
use core::fmt;
#[derive(Clone, Copy)]
pub struct Visibility {
    pub frustum: usize,
    pub occluded: usize,
    pub patches_per_cube: usize,
}
#[derive(Clone, Copy, Default)]
struct Sample {
    at: u64,
    mode: u8,
    expanded: usize,
    unexpanded: usize,
    visibility: Option<Visibility>,
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
        visibility: Option<Visibility>,
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
            visibility,
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
            } else if let Some(v) = s.visibility {
                write!(
                    f,
                    " [{}:+{}ms M{} S{} F{} O{} V{} P{} X{} frames{}]",
                    i,
                    s.at,
                    s.mode,
                    s.expanded + s.unexpanded,
                    v.frustum,
                    v.occluded,
                    s.expanded,
                    s.expanded * v.patches_per_cube,
                    s.unexpanded * v.patches_per_cube,
                    s.frames
                )?;
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
            assert!(s.record(t, 2, 27, 0, None).is_none());
        }
        let r = s.record(1000, 4, 900, 0, None).unwrap();
        assert_eq!(r.samples.map(|s| s.frames), [2, 1, 1, 2]);
        assert_eq!(r.samples[3].at, 999);
        assert_eq!(s.samples[0].mode, 4);
        assert!(s.record(1100, 4, 900, 0, None).is_none());
    }
    #[test]
    fn stalls_leave_missing_bins_not_fake_datapoints() {
        let mut s = Sampler::new(0);
        s.record(20, 1, 3, 597, None);
        let r = s.record(4300, 1, 5, 595, None).unwrap();
        assert_eq!(r.samples.map(|s| s.frames), [1, 0, 0, 0]);
        assert_eq!(s.start, 4000);
        assert!(s.record(4301, 1, 5, 595, None).is_none());
    }

    #[test]
    fn orchard_reports_full_source_and_avoided_patch_submissions() {
        let mut s = Sampler::new(0);
        s.record(
            0,
            4,
            385,
            515,
            Some(Visibility {
                frustum: 876,
                occluded: 491,
                patches_per_cube: 44,
            }),
        );
        s.record(
            250,
            4,
            0,
            900,
            Some(Visibility {
                frustum: 0,
                occluded: 0,
                patches_per_cube: 44,
            }),
        );
        let report = s.record(1000, 2, 27, 0, None).unwrap().to_string();
        assert!(report.contains("M4 S900 F876 O491 V385 P16940 X22660 frames1"));
        // The required empty-view placeholder is never a logical cube.
        assert!(report.contains("M4 S900 F0 O0 V0 P0 X39600 frames1"));
        assert!(
            s.record(2000, 2, 27, 0, None)
                .unwrap()
                .to_string()
                .contains("M2 E27 U0 F1")
        );
    }
}
