//! World identities are cubie sticker sets; connections follow exact lattice poses.
use crate::rubik::Puzzle;

// The authored roster: opposing pairs are Sky/Underground (+/-X),
// Black/White hole (+/-Y), Island/City (+/-Z). Index order is not lattice order.
pub const THEME_MASKS: [u8; 27] = [
    1, 2, 4, 8, 16, 32, 5, 9, 17, 33, 6, 10, 18, 34, 20, 36, 24, 40, 21, 37, 25, 41, 22, 38, 26,
    42, 0,
];
pub const VOID: usize = 26;
pub const FACES: usize = 7; // north, east, south, west, bottom, top, center

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    World(usize),
    Leave,
    None,
}
pub type Routes = [Destination; FACES];

pub fn solved_cell(world: usize) -> [i8; 3] {
    let mask = THEME_MASKS[world];
    core::array::from_fn(|axis| {
        if mask & (1 << (axis * 2)) != 0 {
            1
        } else if mask & (2 << (axis * 2)) != 0 {
            -1
        } else {
            0
        }
    })
}
pub fn cubie(world: usize) -> usize {
    let [x, y, z] = solved_cell(world).map(|x| (x + 1) as usize);
    x + y * 3 + z * 9
}
/// Local sticker axis maps to the corresponding authored generic Leave slot.
/// Cubie identity (not its post-turn lattice location) determines the world.
pub fn entry(cubie_id: usize, face_axis: usize) -> Option<(usize, usize)> {
    let world = (0..VOID).find(|&w| cubie(w) == cubie_id)?;
    let cell = solved_cell(world);
    let rank = (0..3)
        .filter(|&a| cell[a] != 0)
        .position(|a| a == face_axis)?;
    let portal = authored_routes(world)
        .iter()
        .enumerate()
        .filter(|(_, d)| **d == Destination::Leave)
        .nth(rank)?
        .0;
    Some((world, portal))
}

fn world_for_mask(mask: u8) -> usize {
    THEME_MASKS
        .iter()
        .position(|&m| m == mask)
        .expect("closed world roster")
}

/// Keep the authored room faces, including its Leave slots and Void markers.
pub fn authored_routes(world: usize) -> Routes {
    use Destination::{Leave, None, World};
    let mask = THEME_MASKS[world];
    let themes: [u8; 6] = core::array::from_fn(|i| 1 << i);
    let mut active = themes.into_iter().filter(|bit| mask & bit != 0);
    match mask.count_ones() {
        0 => [
            World(0),
            World(2),
            World(1),
            World(3),
            World(5),
            World(4),
            Leave,
        ],
        1 => {
            let pair = (0..3).find(|pair| mask & (3 << (pair * 2)) != 0).unwrap();
            let mut other = themes
                .into_iter()
                .filter(|bit| bit & (3 << (pair * 2)) == 0);
            let mut routes = [None; FACES];
            for route in &mut routes[..4] {
                *route = World(world_for_mask(mask | other.next().unwrap()));
            }
            routes[4] = World(VOID);
            routes[5] = Leave;
            routes
        }
        2 => {
            let a = active.next().unwrap();
            let b = active.next().unwrap();
            let pair = (0..3).find(|pair| mask & (3 << (pair * 2)) == 0).unwrap();
            [
                World(world_for_mask(a)),
                World(world_for_mask(mask | (1 << (pair * 2)))),
                World(world_for_mask(b)),
                World(world_for_mask(mask | (2 << (pair * 2)))),
                Leave,
                Leave,
                None,
            ]
        }
        3 => {
            let a = active.next().unwrap();
            let b = active.next().unwrap();
            let c = active.next().unwrap();
            [
                World(world_for_mask(a | b)),
                World(world_for_mask(a | c)),
                World(world_for_mask(b | c)),
                Leave,
                Leave,
                Leave,
                None,
            ]
        }
        _ => unreachable!(),
    }
}

pub fn routes(world: usize, puzzle: &Puzzle) -> Routes {
    let authored = authored_routes(world);
    if world == VOID {
        return authored;
    }
    let origin = solved_cell(world);
    let (cell, basis) = puzzle.lattice_pose(cubie(world));
    authored.map(|destination| {
        let Destination::World(target) = destination else {
            return destination;
        };
        let neighbor = solved_cell(target);
        let direction: [i8; 3] = core::array::from_fn(|axis| neighbor[axis] - origin[axis]);
        let adjacent = core::array::from_fn(|row| {
            cell[row]
                + (0..3)
                    .map(|column| basis[column][row] * direction[column])
                    .sum::<i8>()
        });
        let identity = puzzle
            .identity_at(adjacent)
            .expect("inward cubie face must have a neighbor");
        Destination::World(
            (0..27)
                .find(|&world| cubie(world) == identity)
                .expect("every cubie has a world"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roster_is_a_bijection_and_solved_topology_matches_the_exports() {
        let puzzle = Puzzle::new(0);
        let mut ids: Vec<_> = (0..27).map(cubie).collect();
        ids.sort();
        assert_eq!(ids, (0..27).collect::<Vec<_>>());
        assert_eq!(cubie(VOID), 13);
        for world in 0..27 {
            assert_eq!(routes(world, &puzzle), authored_routes(world));
        }
    }
    #[test]
    fn committed_turns_change_neighbors_but_preserve_exits_void_and_reciprocity() {
        let mut puzzle = Puzzle::new(0);
        let initial: Vec<_> = (0..27).map(|w| routes(w, &puzzle)).collect();
        assert!(puzzle.select(0, 0));
        puzzle.update(1000); // animation started; no committed permutation yet
        assert_eq!(puzzle.revision(), 0);
        for world in 0..27 {
            assert_eq!(routes(world, &puzzle), initial[world]);
        }
        let mut changed = false;
        for now in [2000, 3000, 4000] {
            puzzle.update(now);
            for world in 0..27 {
                let current = routes(world, &puzzle);
                changed |= current != initial[world];
                for (face, destination) in current.into_iter().enumerate() {
                    if initial[world][face] == Destination::Leave {
                        assert_eq!(destination, Destination::Leave);
                    }
                    if let Destination::World(target) = destination {
                        assert_ne!(world, target);
                        assert!(routes(target, &puzzle).contains(&Destination::World(world)));
                        if world == VOID {
                            assert!(target < 6);
                        }
                        if target == VOID {
                            assert!(world < 6);
                        }
                    }
                }
            }
        }
        assert!(changed);
        assert_eq!(routes(VOID, &puzzle), initial[VOID]);
        let revision = puzzle.revision();
        puzzle.reenter(5000);
        assert_eq!(puzzle.revision(), revision);
        assert!(puzzle.selected().is_none());
    }
}
