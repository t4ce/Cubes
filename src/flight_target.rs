//! One render-only landing cube. It never participates in collision or placement.
use crate::{
    orchard::{CUSTOM_RGB555, Cube},
    walker_camera::LandingTarget,
};

pub const APPEAR_MS: u64 = 700;
// Existing whole-cube transparent material encoding; class 3 is 35% alpha.
pub const FLAGS: u32 = 24576 | 512 | 4096 | (3 << 10);

/// Damped spring: four oscillations, five exponential decay constants.
/// Starts at rest, overshoots, and settles exactly at the end of the appearance.
fn physical(t: f32) -> f32 {
    if t <= 0. {
        return 0.;
    }
    if t >= 1. {
        return 1.;
    }
    let frequency = 4. * core::f32::consts::TAU;
    let spring = 1.
        - libm::expf(-5. * t)
            * (libm::cosf(frequency * t) + 5. / frequency * libm::sinf(frequency * t));
    // Remove the tiny terminal residual without changing the starting velocity.
    spring + libm::expf(-5.) * t * t * (3. - 2. * t)
}

#[derive(Default)]
pub struct Indicator {
    target: Option<LandingTarget>,
    started: u64,
    material: Option<u32>,
}
impl Indicator {
    pub fn flags(&self) -> Option<u32> { self.material.map(|m| FLAGS | m) }
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn update(
        &mut self,
        target: Option<LandingTarget>,
        now: u64,
        cubes: &[Cube],
        palette: &[u32; 6],
    ) -> Option<Cube> {
        if target != self.target {
            self.started = now;
            self.target = target;
            self.material = target.and_then(|t| {
                // Use authored geometry, independent of culling/marker LOD.
                cubes
                    .iter()
                    .find(|c| {
                        c.scale > 0.
                            && (0..3).all(|a| (c.center[a] - t.center[a]).abs() <= c.scale + 0.002)
                    })
                    .map(|c| material(c.flags, palette))
            });
        }
        let target = self.target?;
        let material = self.material?;
        let scale = target.scale
            * 0.5
            * physical(now.saturating_sub(self.started) as f32 / APPEAR_MS as f32);
        // Sub-millimetre seeds mean marker dots to the shader, not tiny cubes.
        if scale < 0.001 {
            return None;
        }
        Some(Cube {
            // Grow out of the face, keeping the whole indicator outside the
            // opaque target even during overshoot. One cube, on any of six faces.
            center: core::array::from_fn(|a| {
                target.center[a] + target.normal[a] * (target.scale + scale + 0.002)
            }),
            scale,
            flags: FLAGS | material,
        })
    }
}

fn material(flags: u32, palette: &[u32; 6]) -> u32 {
    if flags & CUSTOM_RGB555 == 0 {
        return (flags & 7).min(5);
    }
    // World RGB555 shades map back to the six shared imported theme materials.
    // Placed assets use their closest theme; explicit palette IDs stay exact.
    let rgb = [0, 5, 10].map(|shift| ((flags >> shift) & 31) as i32);
    palette
        .iter()
        .enumerate()
        .min_by_key(|(_, color)| {
            (0..3)
                .map(|a| {
                    let channel = ((*color >> (a * 8)) & 255) as i32;
                    let delta = rgb[a] - (channel * 31 + 127) / 255;
                    delta * delta
                })
                .sum::<i32>()
        })
        .unwrap()
        .0 as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    const PALETTE: [u32; 6] = [
        0xff0000ff, 0xff0080ff, 0xff00ffff, 0xff00ff00, 0xffff0000, 0xffff00ff,
    ];
    const TARGET: LandingTarget = LandingTarget {
        center: [2., 3., -4.],
        scale: 0.8,
        normal: [0., 1., 0.],
    };
    fn source(flags: u32) -> [Cube; 1] {
        [Cube {
            center: TARGET.center,
            scale: TARGET.scale,
            flags,
        }]
    }
    #[test]
    fn spring_starts_at_rest_overshoots_and_settles() {
        assert_eq!(physical(0.), 0.);
        assert!(physical(0.0001) < 0.00001);
        let samples: alloc::vec::Vec<_> = (0..=1000).map(|i| physical(i as f32 / 1000.)).collect();
        assert!(samples.iter().all(|s| s.is_finite() && *s >= 0.));
        assert!(samples.iter().copied().fold(0., f32::max) > 1.4);
        assert!((samples[250] - 1.).abs() > (samples[750] - 1.).abs());
        assert!((physical(0.9999) - 1.).abs() < 0.0001);
        assert_eq!(physical(1.), 1.);
        assert_eq!(physical(8.), 1.);
    }
    #[test]
    fn one_half_size_cube_grows_outside_each_face_and_keeps_its_theme() {
        for axis in 0..3 {
            for sign in [-1., 1.] {
                for theme in 0..6 {
                    let mut normal = [0.; 3];
                    normal[axis] = sign;
                    let target = LandingTarget { normal, ..TARGET };
                    let mut indicator = Indicator::default();
                    let cubes = source(24576 | theme);
                    assert!(
                        indicator
                            .update(Some(target), 0, &cubes, &PALETTE)
                            .is_none()
                    );
                    for now in [100, 200, APPEAR_MS, 9999] {
                        let cube = indicator
                            .update(Some(target), now, &cubes, &PALETTE)
                            .unwrap();
                        assert_eq!(cube.flags, FLAGS | theme);
                        assert_eq!((cube.flags >> 10) & 3, 3);
                        assert!(
                            (cube.center[axis] - target.center[axis]) * sign - cube.scale
                                > target.scale
                        );
                        for a in 0..3 {
                            if a != axis {
                                assert_eq!(cube.center[a], target.center[a]);
                            }
                        }
                        if now >= APPEAR_MS {
                            assert_eq!(cube.scale, target.scale * 0.5);
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn lost_target_landing_reentry_and_retarget_restart_without_a_trail() {
        let mut indicator = Indicator::default();
        let cubes = source(24576 | 4);
        indicator.update(Some(TARGET), 100, &cubes, &PALETTE);
        assert!(
            indicator
                .update(Some(TARGET), 500, &cubes, &PALETTE)
                .is_some()
        );
        assert!(indicator.update(None, 501, &cubes, &PALETTE).is_none());
        assert!(
            indicator
                .update(Some(TARGET), 900, &cubes, &PALETTE)
                .is_none()
        );
        assert!(
            indicator
                .update(Some(TARGET), 1000, &cubes, &PALETTE)
                .is_some()
        );
        let other_face = LandingTarget {
            normal: [1., 0., 0.],
            ..TARGET
        };
        assert!(
            indicator
                .update(Some(other_face), 1010, &cubes, &PALETTE)
                .is_none()
        );
        indicator.clear();
        assert!(
            indicator
                .update(Some(other_face), 2000, &cubes, &PALETTE)
                .is_none()
        );
    }
    #[test]
    fn every_world_palette_color_maps_to_its_matching_material() {
        for (id, color) in PALETTE.iter().enumerate() {
            let rgb = (0..3)
                .map(|a| (((color >> (a * 8)) & 255) * 31 + 127) / 255 << (a * 5))
                .sum::<u32>();
            assert_eq!(material(CUSTOM_RGB555 | rgb, &PALETTE), id as u32);
        }
    }
}
