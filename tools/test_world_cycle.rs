//! Host integration check: real key routing + lazy cache + all bundled worlds.
#![allow(dead_code)]
extern crate alloc;

#[path = "../src/cube_format.rs"]
mod cube_format;
#[path = "../src/modes.rs"]
mod modes;
#[path = "../src/orchard.rs"]
mod orchard;
#[path = "../src/SubCubes.rs"]
mod subcubes;

#[test]
fn every_bundled_world_loads_in_order_and_revisits_are_cached() {
    use modes::{ModeKeys, SceneMode};
    let mut paths: Vec<_> = std::fs::read_dir("Cube/lvl27")
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "cubes"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 27);
    let sources: Vec<(&'static str, &'static [u8])> = paths
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            assert!(name.starts_with(&format!("world_{:02}_", i + 1)));
            let bytes = std::fs::read(path).unwrap();
            (
                &*Box::leak(name.into_boxed_str()),
                &*Box::leak(bytes.into_boxed_slice()),
            )
        })
        .collect();
    let sources = Box::leak(sources.into_boxed_slice());
    let mut pages = orchard::Pages::new(sources, false);
    let mut keys = ModeKeys::default();
    let mut current = SceneMode::StaticCube;
    let mut pointers = Vec::new();
    for step in 0..54 {
        let selection = keys.update(16, current, 0, 1, pages.len()).unwrap();
        assert_eq!(selection.mode, SceneMode::World);
        let index = selection.page.unwrap();
        assert_eq!(index, step % 27);
        assert_eq!(pages.load_world(index), Ok(step < 27));
        assert_eq!(pages[index].name, sources[index].0);
        let original = orchard::decode(sources[index].0, sources[index].1).unwrap();
        for (oriented, original) in pages[index].cubes.iter().zip(&original.cubes) {
            assert_eq!(oriented.center, orchard::world_from_demo(original.center));
            assert_eq!(oriented.scale, original.scale);
            assert_eq!(oriented.flags, original.flags);
        }
        assert_eq!(pages[index].radius, original.radius);
        assert!(!pages[index].cubes.is_empty());
        if step < 27 {
            pointers.push(pages[index].cubes.as_ptr());
        } else {
            assert_eq!(pages[index].cubes.as_ptr(), pointers[index]);
        }
        assert!(keys.update(16, SceneMode::World, 0, 1, 27).is_none());
        keys.update(0, SceneMode::World, 0, 1, 27);
        // World sequence must survive visiting every existing demo.
        current = SceneMode::World;
        for key in [1, 2, 4, 8] {
            current = keys.update(key, current, 0, 1, 27).unwrap().mode;
            keys.update(0, current, 0, 1, 27);
        }
    }
}

#[path = "../src/camera_entry.rs"]
mod camera_entry;

#[test]
fn world_room_puzzle_sequence_has_valid_puzzle_entry() {
    use modes::{ModeKeys, SceneMode};
    let mut keys = ModeKeys::default();
    let mut mode = SceneMode::StaticCube;
    for key in [16, 1, 2] {
        mode = keys.update(key, mode, 0, 1, 27).unwrap().mode;
        keys.update(0, mode, 0, 1, 27);
    }
    assert_eq!(mode, SceneMode::StaticCube);
    // Key 1's centered camera was the failing input to the Key 2 look-at.
    assert_eq!(camera_entry::puzzle_position([0.0; 3]), [0.0, 0.0, -7.5]);
}
