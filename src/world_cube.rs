//! Display-only first-person Rubik companion. Opening never changes a cubie.
use crate::grid;

pub const SEEDS: usize = 27 + 54;
pub const PERIOD_MS: u64 = 7_500;
#[derive(Default)]
pub struct Companion { visible: bool, held: bool, started: u64 }
impl Companion {
    pub fn key(&mut self, held: bool, in_world: bool, now: u64) {
        if held && !self.held && in_world {
            self.visible = !self.visible;
            self.started = now;
        }
        self.held = held;
    }
    pub fn visible(&self, in_world: bool) -> bool { in_world && self.visible }
    pub fn expansion(&self, now: u64) -> f32 {
        let t = now.saturating_sub(self.started) % PERIOD_MS;
        let p = if t < PERIOD_MS - 2_000 { 0.0 }
            else if t < PERIOD_MS - 1_000 { (t - (PERIOD_MS - 2_000)) as f32 / 1_000.0 }
            else { (PERIOD_MS - t) as f32 / 1_000.0 };
        p * p * (3.0 - 2.0 * p)
    }
}

pub struct Placement { center: [f32; 3], factor: f32, spacing: f32 }
impl Placement {
    pub fn new(width: u32, height: u32, tan_half_fov: f32, expansion: f32) -> Self {
        let h = height.max(1) as f32;
        let w = width.max(1) as f32;
        let edge = (w.min(h) * 0.30).min(160.0);
        let pad = (w.min(h) * 0.035).min(14.0);
        let depth = 0.4;
        // Bound the expanded cube by a sphere, including perspective growth
        // towards the eye. Keep its whole pulse inside the top-right inset.
        let bound = 1.732051 * (grid::CUBE_GRID_SPACING + grid::CUBE_GRID_SCALE);
        let half = edge / h * tan_half_fov;
        let x = (w - 2.0 * pad - edge) / h * tan_half_fov;
        let y = (h - 2.0 * pad - edge) / h * tan_half_fov;
        let factor = half * depth / (bound * (1.0 + half + x.abs().max(y.abs())));
        Self { center: [x * depth, y * depth, -depth], factor,
            spacing: grid::CUBE_COMPACT_SPACING + (grid::CUBE_GRID_SPACING - grid::CUBE_COMPACT_SPACING) * expansion }
    }
    pub fn pose(&self, cell: [f32; 3], basis: [[f32; 3]; 3]) -> ([f32; 3], [[f32; 3]; 3], f32) {
        let p = orient(cell);
        (core::array::from_fn(|a| self.center[a] + p[a] * self.spacing * self.factor),
         basis.map(orient), grid::CUBE_GRID_SCALE * self.factor)
    }
}
fn orient([x, y, z]: [f32; 3]) -> [f32; 3] {
    // Fixed isometric presentation, with the Key-2 -Y-up convention retained.
    let (y, z) = (-0.921061 * y + 0.389418 * z, -0.389418 * y - 0.921061 * z);
    [0.825336 * x + 0.564642 * z, y, -0.564642 * x + 0.825336 * z]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rising_edge_toggles_and_pulse_starts_compact_every_seven_and_half_seconds() {
        let mut c = Companion::default();
        c.key(true, true, 100);
        for t in [100, 200, 1000] { c.key(true, true, t); assert!(c.visible(true)); }
        assert_eq!(c.expansion(100), 0.0);
        assert_eq!(c.expansion(5600), 0.0);
        assert_eq!(c.expansion(6600), 1.0);
        assert_eq!(c.expansion(7600), 0.0);
        assert_eq!(c.expansion(14100), 1.0);
        assert!(!c.visible(false));
        c.key(false, true, 8000); c.key(true, true, 8001);
        assert!(!c.visible(true));
    }
    #[test]
    fn entire_expanded_model_fits_inset_and_stays_beyond_near_plane() {
        for (w,h) in [(784,441),(2560,1440),(441,784),(4000,300)] {
            let placement = Placement::new(w,h,0.5773503,1.0);
            for x in [-1.,1.] { for y in [-1.,1.] { for z in [-1.,1.] {
                let (p,b,s) = placement.pose([x,y,z],[[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]]);
                for i in [-1.,1.] { for j in [-1.,1.] { for k in [-1.,1.] {
                    let v: [f32;3] = core::array::from_fn(|a| p[a]+s*(b[0][a]*i+b[1][a]*j+b[2][a]*k));
                    let px = (v[0]/(-v[2]*0.5773503)*h as f32+w as f32)*0.5;
                    let py = (1.-v[1]/(-v[2]*0.5773503))*h as f32*0.5;
                    assert!(-v[2]>0.1 && px>0. && px<w as f32 && py>0. && py<h as f32);
                }}}
            }}}
        }
    }
}
