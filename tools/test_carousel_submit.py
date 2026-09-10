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
for name in ("carousel_anchor_seed", "encode_seed"):
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
'''

with tempfile.TemporaryDirectory(prefix="cubes-carousel-submit-") as folder:
    path = Path(folder)
    (path / "tests.rs").write_text(tests)
    subprocess.run(["rustc", "--edition=2024", "-O", "--test", str(path / "tests.rs"),
                    "-o", str(path / "tests")], check=True)
    subprocess.run([str(path / "tests")], check=True)
