#!/usr/bin/env python3
"""Check Key4's anchor and seed encoding against the sibling kernel's decoder."""
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT.parent / "TRUEOS/tools"))
from test_clip_position3_uv_texture import constant, item
from test_retained_scene import source

tests = source()
tests += "mod grid {" + constant(str(ROOT / "src/grid.rs"), "CUBE_LOCAL_RADIUS") + "}\n"
tests += "mod flight_target {" + constant(str(ROOT / "src/flight_target.rs"), "FLAGS") + "}\n"
tests += '''mod orchard {
    #[derive(Clone, Copy)] pub struct Cube {pub center:[f32;3], pub scale:f32, pub flags:u32}
    pub const WORLD_ROTATION:[f32;4]=[1.,0.,0.,0.];
}\n'''
for name in ("carousel_anchor_seed", "flight_target_seed", "encode_seed"):
    tests += item(str(ROOT / "src/main.rs"), name)
tests += r'''
#[test]
fn carousel_submit_accepts_waiting_reveal_and_full_capacity_frames() {
    let draws = [RetainedDrawRange { first_index: 0, index_count: 44 }; 2];
    // Group 1 contains a placeholder during the initial delay, then visible
    // cubes. The hidden opaque anchor is appended last, as in the app.
    for transparent_count in [1usize, 32, 600, MAX_RETAINED_SCENE_INSTANCES - 1] {
        let anchor = carousel_anchor_seed([0., 0., -14.]);
        let mut bytes = vec![0; (transparent_count + 1) * 64];
        for slot in 0..transparent_count {
            encode_seed(RetainedTransformSeed {
                rotation: [0., 0., 0., 1.], draw_group: 1,
                flags: ((slot as u32) << 16) | 25088,
                ..anchor
            }, &mut bytes[slot * 64..(slot + 1) * 64]);
        }
        encode_seed(anchor, &mut bytes[transparent_count * 64..]);
        let seeds = decode_retained_scene_seeds(&bytes)
            .expect("Key4 must not fail frame-submit with -95");
        assert_eq!(picasso_retained_draw_templates(44, &seeds, &draws).unwrap(), [
            [44, 0, 0, 0, 1, 0], [44, 0, 0, 1, transparent_count as u32, 0],
        ]);
    }
}
#[test]
fn flight_indicator_remains_a_valid_transparent_group_before_and_after_landing() {
    let draws = [RetainedDrawRange {first_index:0, index_count:44}; 2];
    // Empty view, loaded world, full seed budget. One reserved transparent slot
    // survives appearing, retargeting, landing and taking off again.
    for opaque_count in [1, 42, MAX_RETAINED_SCENE_INSTANCES - 1] {
        for shown in [false, true, false, true] {
            let cube = shown.then_some(orchard::Cube {
                center:[1.,2.,-8.], scale:0.4, flags:flight_target::FLAGS | 5,
            });
            let target = flight_target_seed(cube, [0.,0.,2.]);
            assert_eq!(target.draw_group, 1);
            assert_eq!((target.flags >> 10) & 3, 3);
            assert_eq!(target.flags & 57856, 25088); // Whole-cube transparent path.
            assert_eq!(target.translation[2] < 0., shown);
            let mut bytes = vec![0; (opaque_count + 1) * 64];
            for row in 0..opaque_count {
                let mut opaque = carousel_anchor_seed([0.,0.,2.]);
                opaque.flags = (row as u32) << 16;
                encode_seed(opaque, &mut bytes[row*64..(row+1)*64]);
            }
            encode_seed(target, &mut bytes[opaque_count*64..]);
            let seeds = decode_retained_scene_seeds(&bytes).expect("valid flight frame");
            assert_eq!(picasso_retained_draw_templates(44, &seeds, &draws).unwrap(), [
                [44,0,0,0,opaque_count as u32,0], [44,0,0,opaque_count as u32,1,0],
            ]);
        }
    }
}
#[test]
fn full_path_and_target_share_one_valid_transparent_group_at_seed_capacity() {
    let draws = [RetainedDrawRange {first_index:0, index_count:44}; 2];
    for dashes in [0usize, 1, 512, 0] {
        let transparent = dashes + 1;
        let opaque = MAX_RETAINED_SCENE_INSTANCES - transparent;
        let mut bytes = vec![0; MAX_RETAINED_SCENE_INSTANCES * 64];
        for row in 0..opaque {
            let mut seed = carousel_anchor_seed([0.,0.,2.]);
            seed.flags = (row as u32) << 16;
            encode_seed(seed, &mut bytes[row*64..(row+1)*64]);
        }
        for slot in 0..transparent {
            let mut seed = flight_target_seed(Some(orchard::Cube {
                center:[0.,0.,-2.-slot as f32], scale:0.036, flags:flight_target::FLAGS | 2,
            }), [0.,0.,2.]);
            seed.flags |= (slot as u32) << 16;
            assert!(seed.scale[0] >= 0.001, "path must retain cube geometry");
            encode_seed(seed, &mut bytes[(opaque+slot)*64..(opaque+slot+1)*64]);
        }
        let seeds = decode_retained_scene_seeds(&bytes).unwrap();
        assert_eq!(picasso_retained_draw_templates(44, &seeds, &draws).unwrap(), [
            [44,0,0,0,opaque as u32,0], [44,0,0,opaque as u32,transparent as u32,0],
        ]);
    }
}
'''

with tempfile.TemporaryDirectory(prefix="cubes-carousel-submit-") as folder:
    path = Path(folder)
    (path / "tests.rs").write_text(tests)
    subprocess.run(["rustc", "--edition=2024", "-O", "--test", str(path / "tests.rs"),
                    "-o", str(path / "tests")], check=True)
    subprocess.run([str(path / "tests")], check=True)
