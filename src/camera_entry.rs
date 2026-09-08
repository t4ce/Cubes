//! Safe entry to the puzzle's origin-centered, -Y-up orbit camera.
pub fn puzzle_position(position: [f32; 3]) -> [f32; 3] {
    let horizontal2 = position[0] * position[0] + position[2] * position[2];
    let radius2 = horizontal2 + position[1] * position[1];
    // Room/sphere cameras sit at the target itself. A vertical position also
    // makes look-at's forward cross world-up zero. Both need a valid orbit
    // position before constructing the camera or its previous-frame matrix.
    if !radius2.is_finite() || horizontal2 <= 1e-8 {
        [0.0, 0.0, -7.5]
    } else {
        position
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_valid_look_at(position: [f32; 3]) {
        let length = position.iter().map(|x| x * x).sum::<f32>().sqrt();
        let forward = position.map(|x| -x / length);
        let right = [forward[2], 0.0, -forward[0]];
        let right_len = right.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(forward.iter().all(|x| x.is_finite()));
        assert!(right_len.is_finite() && right_len > 0.0);
    }

    #[test]
    fn room_or_sphere_to_puzzle_has_a_valid_camera_before_first_render() {
        assert_valid_look_at(puzzle_position([0.0; 3]));
        assert_eq!(puzzle_position([0.0; 3]), [0.0, 0.0, -7.5]);
    }

    #[test]
    fn pole_and_invalid_positions_cannot_poison_camera_history() {
        for position in [
            [0.0, 7.5, 0.0],
            [0.0, -7.5, 0.0],
            [f32::NAN, 0.0, 0.0],
            [f32::INFINITY, 0.0, 0.0],
        ] {
            assert_valid_look_at(puzzle_position(position));
        }
    }

    #[test]
    fn ordinary_orbit_position_is_preserved() {
        let position = [3.0, -2.0, 8.0];
        assert_eq!(puzzle_position(position), position);
        assert_valid_look_at(puzzle_position(position));
    }
}
