#!/usr/bin/env python3
"""Check the real 27-world theme mapping and authenticated shader package."""
from pathlib import Path
import re
import subprocess
import sys
import tempfile

APP=Path(__file__).resolve().parents[1]
OS=APP.parent/'TRUEOS'
sys.path.insert(0,str(OS/'tools'))
from test_clip_position3_uv_texture import item

source=(APP/'src/background.rs').read_text()
constant=re.search(r'const THEMES:[^=]*=.*?;',source,re.S).group(0)
program=constant+'\n'+item(str(APP/'src/background.rs'),'theme_mask')+'\n'
worlds=sorted((APP/'Cube/lvl27').glob('*.cubes'))
expected=[1,2,4,8,16,32,5,9,17,33,6,10,18,34,20,36,24,40,21,37,25,41,22,38,26,42,0]
assert len(worlds)==len(expected)==27
program+='#[test] fn authored_roster_matches_the_six_theme_bits() {\n'
for world,mask in zip(worlds,expected): program+=f'assert_eq!(theme_mask("{world.name}"),{mask});\n'
program+='}\n'
program+=item(str(APP/'src/background.rs'),'render_command')+'\n'
program+='mod libm { pub fn sinf(x:f32)->f32 {x.sin()} pub fn cosf(x:f32)->f32 {x.cos()} }\n'
program+=r'''
#[test] fn sky_culling_tracks_pitch_and_vertical_fov() {
    let command=|pitch,fov|render_command(true,"sky.cubes",0.0,pitch,fov,(784,441));
    assert_eq!(command(0.0,0.577)[0],1); // half sky
    assert_eq!(command(-0.7,0.577)[0],0); // entirely ground
    assert_eq!(command(-0.7,1.0)[0],1); // wider FOV still sees sky
    assert_eq!(command(1.4,0.577)[0],1);
}
#[test] fn hidden_sky_retains_pixels_across_camera_and_theme_changes() {
    let ground=render_command(true,"sky.cubes",0.0,-1.0,0.577,(784,441));
    assert_eq!(ground,render_command(true,"city.cubes",2.0,-1.4,0.577,(784,441)));
    assert_eq!(ground,render_command(false,"island.cubes",2.0,1.4,0.577,(784,441)));
    assert_ne!(ground,render_command(false,"sky.cubes",0.0,0.0,0.577,(1920,1080)));
}
'''
program+='#[test] fn absent_theme_is_not_substring_matched() { assert_eq!(theme_mask("cityscape.cubes"),0); }\n'
with tempfile.TemporaryDirectory(prefix='cubes-background-') as directory:
    root=Path(directory);(root/'tests.rs').write_text(program)
    subprocess.run(['rustc','--edition=2024','--test',str(root/'tests.rs'),'-o',str(root/'tests')],check=True)
    subprocess.run([str(root/'tests')],check=True)
# Palette provenance remains tied to the authored world builder.
html=(APP/'Cube/cube_tree_builder_world_ramps.html').read_text()
glsl=(APP/'Cube/mandelbox/input.glsl').read_text()
for color in ['63C7F2','7A4B30','25153D','F4E8A6','4EAF68','D76567']:
    assert '#'+color in html
    rgb=','.join(str(int(color[i:i+2],16))+'.0' for i in (0,2,4))
    assert 'vec3('+rgb+')/255.0' in glsl
subprocess.run([sys.executable,str(APP/'tools/bake_mandelbox.py'),'--check'],check=True)
print('27 worlds and six authored palette colors verified')
