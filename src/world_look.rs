//! Level first-person look angles for Key 5.
//!
//! Key-5 worlds have a fixed +Y-up terrain plane. Keep yaw and pitch as
//! explicit world-space angles so looking horizontally can never roll the
//! camera or the horizon.

/// The demo-to-world half-turn maps the authored initial view to +Z.
pub const INITIAL_YAW: f32 = core::f32::consts::PI;

pub const MAX_PITCH: f32 = core::f32::consts::FRAC_PI_2 - 0.05;

#[cfg(not(test))]
mod math {
    pub fn sin(angle: f32) -> f32 {
        libm::sinf(angle)
    }

    pub fn cos(angle: f32) -> f32 {
        libm::cosf(angle)
    }
}

// Keep the small pure-logic test runnable directly with rustc, without
// needing the Blueprint's Cargo dependency graph.
#[cfg(test)]
mod math {
    pub fn sin(angle: f32) -> f32 {
        angle.sin()
    }

    pub fn cos(angle: f32) -> f32 {
        angle.cos()
    }
}

pub fn update(yaw: &mut f32, pitch: &mut f32, delta_x: f32, delta_y: f32, sensitivity: f32) {
    *yaw = (*yaw + delta_x * sensitivity) % core::f32::consts::TAU;
    *pitch = (*pitch - delta_y * sensitivity).clamp(-MAX_PITCH, MAX_PITCH);
}

/// Unit forward direction for an unrolled, +Y-up first-person camera.
pub fn direction(yaw: f32, pitch: f32) -> [f32; 3] {
    let horizontal = math::cos(pitch);
    [
        math::sin(yaw) * horizontal,
        math::sin(pitch),
        -math::cos(yaw) * horizontal,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_angles_face_negative_z() {
        assert_eq!(direction(0.0, 0.0), [0.0, 0.0, -1.0]);
    }

    #[test]
    fn first_mouse_event_preserves_the_positive_z_entry_heading() {
        let mut yaw = INITIAL_YAW;
        let mut pitch = 0.0;
        assert!(direction(yaw, pitch)[2] > 0.99999);
        update(&mut yaw, &mut pitch, 1.0, -1.0, 0.002);
        let next = direction(yaw, pitch);
        assert!(next[2] > 0.9999 && next[0].abs() < 0.003 && next[1].abs() < 0.003);
    }

    #[test]
    fn horizontal_mouse_motion_changes_yaw_not_horizon_tilt() {
        let mut yaw = 0.0;
        let mut pitch = 0.0;
        update(&mut yaw, &mut pitch, 100.0, 0.0, 0.002);
        assert_eq!(pitch, 0.0);
        assert!(direction(yaw, pitch)[0] > 0.0);
        assert_eq!(direction(yaw, pitch)[1], 0.0);
    }

    #[test]
    fn pitch_is_clamped_before_forward_becomes_parallel_to_world_up() {
        let mut yaw = 0.0;
        let mut pitch = 0.0;
        update(&mut yaw, &mut pitch, 0.0, -10_000.0, 0.002);
        assert_eq!(pitch, MAX_PITCH);
        assert!(math::cos(pitch) > 0.0);
    }
}
