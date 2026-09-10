//! Number-key routing. The world sequence belongs only to Key 5.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneMode {
    InteractiveGrid,
    StaticCube,
    Sphere,
    Orchard,
    World,
    MaterialShowcase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Selection {
    pub mode: SceneMode,
    pub page: Option<usize>,
}

#[derive(Default)]
pub struct ModeKeys {
    held: u8,
    next_world: usize,
}

impl ModeKeys {
    pub fn update(
        &mut self,
        held: u8,
        current: SceneMode,
        orchard: usize,
        orchards: usize,
        worlds: usize,
    ) -> Option<Selection> {
        let pressed = held & !self.held;
        self.held = held;
        let mode = if pressed & 1 != 0 {
            SceneMode::InteractiveGrid
        } else if pressed & 2 != 0 {
            SceneMode::StaticCube
        } else if pressed & 4 != 0 {
            SceneMode::Sphere
        } else if pressed & 8 != 0 && orchards > 0 {
            SceneMode::Orchard
        } else if pressed & 16 != 0 && worlds > 0 {
            SceneMode::World
        } else if pressed & 64 != 0 {
            SceneMode::MaterialShowcase
        } else {
            return None;
        };
        let page = match mode {
            SceneMode::World => {
                let page = self.next_world % worlds;
                self.next_world = (page + 1) % worlds;
                Some(page)
            }
            SceneMode::Orchard => Some(if current == mode {
                (orchard + 1) % orchards
            } else {
                orchard % orchards
            }),
            SceneMode::StaticCube => None,
            _ if current == mode => return None,
            _ => None,
        };
        Some(Selection { mode, page })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn key5_cycles_all_worlds_once_per_press_and_wraps() {
        let mut keys = ModeKeys::default();
        let mut mode = SceneMode::StaticCube;
        for n in 0..55 {
            let selection = keys.update(16, mode, 0, 1, 27).unwrap();
            assert_eq!(
                selection,
                Selection {
                    mode: SceneMode::World,
                    page: Some(n % 27)
                }
            );
            mode = selection.mode;
            assert_eq!(keys.update(16, mode, 0, 1, 27), None);
            assert_eq!(keys.update(0, mode, 0, 1, 27), None);
        }
    }
    #[test]
    fn other_keys_neither_advance_nor_reset_world_sequence() {
        let mut keys = ModeKeys::default();
        let mut mode = SceneMode::StaticCube;
        for n in 0..27 {
            let world = keys.update(16, mode, 0, 2, 27).unwrap();
            assert_eq!(world.page, Some(n));
            mode = world.mode;
            keys.update(0, mode, 0, 2, 27);
            for (key, expected) in [
                (1, SceneMode::InteractiveGrid),
                (2, SceneMode::StaticCube),
                (4, SceneMode::Sphere),
                (8, SceneMode::Orchard),
            ] {
                let selection = keys.update(key, mode, 0, 2, 27).unwrap();
                assert_eq!(selection.mode, expected);
                assert_eq!(
                    selection.page,
                    (expected == SceneMode::Orchard).then_some(0)
                );
                mode = expected;
                keys.update(0, mode, 0, 2, 27);
            }
        }
    }
    #[test]
    fn existing_demo_repeat_actions_and_empty_catalogs() {
        let mut keys = ModeKeys::default();
        for (held, mode, page) in [
            (1, SceneMode::InteractiveGrid, None),
            (2, SceneMode::StaticCube, Some(None)),
            (4, SceneMode::Sphere, None),
            (8, SceneMode::Orchard, Some(Some(1))),
        ] {
            assert_eq!(keys.update(held, mode, 0, 2, 27).map(|s| s.page), page);
            keys.update(0, mode, 0, 2, 27);
        }
        assert_eq!(keys.update(16, SceneMode::StaticCube, 0, 0, 0), None);
    }

    #[test]
    fn key7_selects_material_showcase_once_per_press() {
        let mut keys = ModeKeys::default();
        let selection = keys.update(64, SceneMode::StaticCube, 0, 0, 0).unwrap();
        assert_eq!(selection.mode, SceneMode::MaterialShowcase);
        assert_eq!(selection.page, None);
        assert_eq!(keys.update(64, selection.mode, 0, 0, 0), None);
        assert_eq!(keys.update(0, selection.mode, 0, 0, 0), None);
    }
}
