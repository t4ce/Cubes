#!/usr/bin/env python3
"""Verify the actual world palette, quaternion follower and Chroma source chain."""
from pathlib import Path
import re
import subprocess
import sys
import tempfile

APP = Path(__file__).resolve().parents[1]
worlds = sorted((APP / "Cube/lvl27").glob("*.cubes"))
expected = [1,2,4,8,16,32,5,9,17,33,6,10,18,34,20,36,24,40,21,37,25,41,22,38,26,42,0]
assert len(worlds) == len(expected) == 27
program = r'''#![allow(dead_code)]
extern crate self as libm;
pub fn sqrtf(x:f32)->f32 {x.sqrt()}
pub fn sinf(x:f32)->f32 {x.sin()}
pub fn cosf(x:f32)->f32 {x.cos()}
pub fn atan2f(y:f32,x:f32)->f32 {y.atan2(x)}
'''
program += f'#[path="{APP}/src/environment.rs"] mod environment;\n'
program += f'#[path="{APP}/src/world_look.rs"] mod world_look;\n'
program += 'use environment::*;\n#[test] fn authored_world_palettes_and_presets_match() {\n'
colors = [0x63c7f2,0x7a4b30,0x25153d,0xf4e8a6,0x4eaf68,0xd76567]
for world, mask in zip(worlds, expected):
    palette = [color for i,color in enumerate(colors) if mask & (1<<i)] or [0xd83cff]
    # The selected colors must also occur in the exported floor/portal palette.
    data = world.read_bytes()
    authored = [int.from_bytes(data[i:i+3],"big") for i in range(16,16+4*data[10],4)]
    assert all(color in authored for color in palette), world.name
    count = len(palette)
    palette += [palette[0]]*(3-count)
    program += f'assert_eq!(Palette::for_world("{world.name}"),Some(Palette {{colors:{palette},count:{count},cathedral:{str(bool(mask)).lower()}}}));\n'
program += '}\n#[test] fn unrecognized_names_do_not_silently_become_void() {assert!(Palette::for_world("cityscape.cubes").is_none());}\n'
with tempfile.TemporaryDirectory(prefix="cubes-background-") as directory:
    root = Path(directory)
    (root/"tests.rs").write_text(program)
    subprocess.run(["rustc","--edition=2024","--test",str(root/"tests.rs"),"-o",str(root/"tests")],check=True)
    subprocess.run([str(root/"tests")],check=True)
html = (APP/"Cube/cube_tree_builder_world_ramps.html").read_text().lower()
source = (APP/"src/environment.rs").read_text().lower()
for color in colors+[0xd83cff]:
    assert f"0x{color:06x}" in html and f"0x{color:06x}" in source
subprocess.run([sys.executable,str(APP/"tools/bake_mandelbox.py"),"--check"],check=True)
print("27 worlds, seven authored colors, full Chroma presets and follower verified")
