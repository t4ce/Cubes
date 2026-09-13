//! Authored world colors and the environment's damped quaternion follower.

// Read the exported pure-world terrain colours. The exporter writes the first
// terrain material as palette entry zero in each CUBES v2 asset. This keeps
// runtime portal routing in sync without another hand-maintained RGB table.
const fn terrain_color(bytes: &[u8]) -> u32 {
    ((bytes[16] as u32) << 16) | ((bytes[17] as u32) << 8) | bytes[18] as u32
}
pub const THEMES: [(&str, u32); 6] = [
    (
        "sky",
        terrain_color(include_bytes!("../Cube/lvl27/world_01_sky.cubes")),
    ),
    (
        "underground",
        terrain_color(include_bytes!("../Cube/lvl27/world_02_underground.cubes")),
    ),
    (
        "black-hole",
        terrain_color(include_bytes!("../Cube/lvl27/world_03_black-hole.cubes")),
    ),
    (
        "white-hole",
        terrain_color(include_bytes!("../Cube/lvl27/world_04_white-hole.cubes")),
    ),
    (
        "island",
        terrain_color(include_bytes!("../Cube/lvl27/world_05_island.cubes")),
    ),
    (
        "city",
        terrain_color(include_bytes!("../Cube/lvl27/world_06_city.cubes")),
    ),
];
pub const VOID_COLOR: u32 = THEMES[2].1;

// The optional Mandelbox shader uses these legacy packed values as geometry
// identifiers (theme_shape.glsl), independently of the world material palette.
const MANDELBOX_THEMES: [(&str, u32); 6] = [
    ("sky", 0x63c7f2),
    ("underground", 0x7a4b30),
    ("black-hole", 0x25153d),
    ("white-hole", 0xf4e8a6),
    ("island", 0x4eaf68),
    ("city", 0xd76567),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub colors: [u32; 3],
    pub count: u32,
    /// Folded Core is reserved for void; every other world uses Box Cathedral.
    pub cathedral: bool,
}

impl Palette {
    /// Arithmetic average of the authored sRGB bytes, rounded to nearest.
    /// Repeated padding colors are excluded; pure and Void colors stay exact.
    pub fn average_rgb(&self) -> [u8; 3] {
        [16, 8, 0].map(|shift| {
            let sum: u32 = self.colors[..self.count as usize]
                .iter()
                .map(|color| (color >> shift) & 255)
                .sum();
            ((sum + self.count / 2) / self.count) as u8
        })
    }

    pub fn for_world(name: &str) -> Option<Self> {
        Self::from_themes(name, THEMES, VOID_COLOR)
    }

    pub fn for_mandelbox_world(name: &str) -> Option<Self> {
        Self::from_themes(name, MANDELBOX_THEMES, 0xd83cff)
    }

    fn from_themes(name: &str, themes: [(&str, u32); 6], void: u32) -> Option<Self> {
        let name = name.strip_suffix(".cubes").unwrap_or(name);
        if name == "world_27_void" {
            return Some(Self {
                colors: [void; 3],
                count: 1,
                cathedral: false,
            });
        }
        let mut colors = [0; 3];
        let mut count = 0;
        for (theme, color) in themes {
            if name.split('_').any(|word| word == theme) {
                if count == colors.len() {
                    return None;
                }
                colors[count] = color;
                count += 1;
            }
        }
        if count == 0 {
            return None;
        }
        for i in count..3 {
            colors[i] = colors[0];
        }
        Some(Self {
            colors,
            count: count as u32,
            cathedral: true,
        })
    }
}

pub struct RotationFollower {
    rotation: [f32; 4],
    velocity: [f32; 3],
}

impl RotationFollower {
    pub fn new(rotation: [f32; 4]) -> Self {
        Self {
            rotation: normalize(rotation),
            velocity: [0.0; 3],
        }
    }

    pub fn advance(&mut self, target: [f32; 4], seconds: f32) -> [f32; 4] {
        let mut target = normalize(target);
        // q and -q denote the same orientation. Always pursue the short arc,
        // including camera yaw crossing +/-pi, instead of rolling a full turn.
        if dot(self.rotation, target) < 0.0 {
            target = target.map(|x| -x);
        }
        let mut remaining = seconds.clamp(0.0, 0.1);
        while remaining > 0.0 {
            let dt = remaining.min(1.0 / 120.0);
            remaining -= dt;
            let q = self.rotation;
            let mut error = multiply(target, [-q[0], -q[1], -q[2], q[3]]);
            if error[3] < 0.0 {
                error = error.map(|x| -x);
            }
            let length = libm::sqrtf(error[..3].iter().map(|x| x * x).sum());
            let angle = 2.0 * libm::atan2f(length, error[3].max(0.0));
            if angle < 0.0001 && self.velocity.iter().all(|x| x.abs() < 0.0004) {
                self.rotation = target;
                self.velocity = [0.0; 3];
                break;
            }
            // 10 rad/s, damping ratio 0.74: a little overshoot, then rest.
            let scale = if length > 1e-7 { angle / length } else { 2.0 };
            for (i, velocity) in self.velocity.iter_mut().enumerate() {
                *velocity += (100.0 * error[i] * scale - 14.8 * *velocity) * dt;
            }
            let speed = libm::sqrtf(self.velocity.iter().map(|x| x * x).sum());
            if speed > 1e-7 {
                let half_angle = speed * dt * 0.5;
                let scale = libm::sinf(half_angle) / speed;
                let delta = [
                    self.velocity[0] * scale,
                    self.velocity[1] * scale,
                    self.velocity[2] * scale,
                    libm::cosf(half_angle),
                ];
                self.rotation = normalize(multiply(delta, self.rotation));
            }
        }
        self.rotation
    }
}

fn dot(a: [f32; 4], b: [f32; 4]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn normalize(q: [f32; 4]) -> [f32; 4] {
    let length = libm::sqrtf(dot(q, q));
    if !length.is_finite() || length < 1e-7 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    q.map(|x| x / length)
}

fn multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn yaw(angle: f32) -> [f32; 4] {
        [0.0, (angle * 0.5).sin(), 0.0, (angle * 0.5).cos()]
    }
    #[test]
    fn follower_lags_overshoots_and_then_stops_requesting_changes() {
        let mut follower = RotationFollower::new(yaw(0.0));
        let target = yaw(0.5);
        let first = follower.advance(target, 1.0 / 60.0);
        assert!(first[1] > 0.0 && first[1] < target[1] * 0.1);
        let mut overshot = false;
        for _ in 0..180 {
            let q = follower.advance(target, 1.0 / 60.0);
            assert!((dot(q, q) - 1.0).abs() < 1e-5);
            overshot |= q[1] > target[1] + 0.0001;
        }
        assert!(overshot);
        let settled = follower.advance(target, 1.0 / 60.0);
        assert_eq!(settled, follower.advance(target, 1.0 / 60.0));
        assert!(dot(settled, target) > 0.99999);
    }
    #[test]
    fn yaw_wrap_and_quaternion_sign_do_not_make_full_turns() {
        let initial = yaw(core::f32::consts::PI - 0.01);
        let mut follower = RotationFollower::new(initial);
        let q = follower.advance(yaw(-core::f32::consts::PI + 0.01), 0.016);
        assert!(dot(q, initial).abs() > 0.9999);
        let mut follower = RotationFollower::new(initial);
        assert!(dot(follower.advance(initial.map(|x| -x), 0.016), initial) > 0.99999);
    }
    #[test]
    fn time_subdivision_keeps_following_consistent() {
        let mut a = RotationFollower::new(yaw(0.0));
        let mut b = RotationFollower::new(yaw(0.0));
        for _ in 0..30 {
            a.advance(yaw(0.9), 1.0 / 30.0);
        }
        for _ in 0..120 {
            b.advance(yaw(0.9), 1.0 / 120.0);
        }
        assert!(dot(a.rotation, b.rotation) > 0.99999);
    }
}
