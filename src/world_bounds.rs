//! Key-5 camera containment for the authored floor and its portal ramps.
//!
//! The 27 exports use the same 0.2-unit strict grid as the world builder.
//! Worlds 1..=26 have a 160-cell (32-unit) square floor and four 16-cell
//! (3.2-unit) by 32-cell (6.4-unit) ramps. World 27 is Void, which has no
//! horizontal ramps. Keep these walls separate from rendering: they are only
//! a two-dimensional first-person movement rule.

const CORE_HALF_EXTENT: f32 = 16.0;
const RAMP_HALF_WIDTH: f32 = 1.6;
const RAMP_LENGTH: f32 = 6.4;
/// A point-camera still needs a small inset so it never appears outside a
/// floor cube because of its visual gap.
const WALKER_INSET: f32 = 0.25;
/// Standing portals sit at about 22.2 units from the center. Stop before the
/// portal plane, rather than making its geometry an accidental walk-through.
const PORTAL_CLEARANCE: f32 = 0.6;

/// The Void export is the only Key-5 world with no side ramps or portals.
pub const fn has_horizontal_ramps(world_index: usize) -> bool {
    world_index < 26
}

fn allowed(x: f32, z: f32, ramps: bool) -> bool {
    let x = x.abs();
    let z = z.abs();
    let core_half = CORE_HALF_EXTENT - WALKER_INSET;
    if x <= core_half && z <= core_half {
        return true;
    }
    if !ramps {
        return false;
    }

    let ramp_half_width = RAMP_HALF_WIDTH - WALKER_INSET;
    let ramp_outer = CORE_HALF_EXTENT + RAMP_LENGTH - PORTAL_CLEARANCE;
    // North/south ramps, then east/west ramps. Their overlap with the core
    // makes entering and leaving a centered ramp continuous.
    (x <= ramp_half_width && z <= ramp_outer) || (z <= ramp_half_width && x <= ramp_outer)
}

/// Apply a walking delta while treating the floor outline as invisible walls.
///
/// Test each axis after the full candidate fails. This preserves natural wall
/// sliding, including when the player walks diagonally into a core corner or a
/// narrow ramp edge.
pub fn advance(position: [f32; 2], delta: [f32; 2], ramps: bool) -> [f32; 2] {
    let wanted = [position[0] + delta[0], position[1] + delta[1]];
    if allowed(wanted[0], wanted[1], ramps) {
        return wanted;
    }

    let mut slid = position;
    if allowed(wanted[0], slid[1], ramps) {
        slid[0] = wanted[0];
    }
    if allowed(slid[0], wanted[1], ramps) {
        slid[1] = wanted[1];
    }
    slid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_is_walkable_but_its_outer_edge_is_not() {
        assert_eq!(advance([0.0, 0.0], [15.7, -15.7], false), [15.7, -15.7]);
        assert_eq!(advance([15.7, 0.0], [1.0, 0.0], false), [15.7, 0.0]);
    }

    #[test]
    fn portal_worlds_allow_only_the_four_centered_ramps() {
        assert_eq!(advance([0.0, 15.7], [0.0, 5.0], true), [0.0, 20.7]);
        // This diagonal lies beyond the core but outside the north ramp.
        assert_eq!(advance([15.7, 15.7], [1.0, 1.0], true), [15.7, 15.7]);
        // The portal-clearance wall stops forward motion at the ramp end.
        assert_eq!(advance([0.0, 21.7], [0.0, 1.0], true), [0.0, 21.7]);
    }

    #[test]
    fn void_has_no_ramps() {
        assert!(!has_horizontal_ramps(26));
        assert_eq!(advance([0.0, 15.7], [0.0, 1.0], false), [0.0, 15.7]);
    }

    #[test]
    fn diagonal_contact_slides_along_the_wall() {
        // X is blocked; Z remains legal and therefore slides along the east edge.
        assert_eq!(advance([15.7, 0.0], [1.0, 1.0], false), [15.7, 1.0]);
    }
}
