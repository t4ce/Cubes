#!/usr/bin/env python3
"""Exercise the Cubes-owned walker, tier collision, and bundled geometry pages."""
from pathlib import Path
import subprocess
import tempfile

APP = Path(__file__).resolve().parents[1]
source = '''#![allow(dead_code)]
extern crate alloc;
extern crate self as libm;
extern crate self as trueos_picasso;
pub fn sqrtf(x:f32)->f32 {x.sqrt()}
pub fn sinf(x:f32)->f32 {x.sin()}
pub fn asinf(x:f32)->f32 {x.asin()}
pub fn cosf(x:f32)->f32 {x.cos()}
pub fn acosf(x:f32)->f32 {x.acos()}
pub fn expf(x:f32)->f32 {x.exp()}
pub fn floorf(x:f32)->f32 {x.floor()}
pub fn roundf(x:f32)->f32 {x.round()}
pub fn ceilf(x:f32)->f32 {x.ceil()}
'''
source += f'#[path="{APP}/src/orchard.rs"] mod orchard;\n'
source += f'#[path="{APP}/src/cube_format.rs"] mod cube_format;\n'
source += f'#[path="{APP}/src/SubCubes.rs"] mod subcubes;\n'
source += f'#[path="{APP.parent}/TRUEOS-Picasso/src/cam.rs"] pub mod cam;\n'
source += f'#[path="{APP}/src/CubesWalkerCam.rs"] mod walker_camera;\n'
source += f'#[path="{APP}/src/floor.rs"] mod floor;\n'
source += 'const WORLD_PAGES: &[&[u8]] = &[\n'
for path in sorted((APP/'Cube/lvl27').glob('*.cubes')):
    source += f'include_bytes!("{path}"),\n'
source += '];\n'
with tempfile.TemporaryDirectory(prefix='cubes-walker-') as directory:
    root = Path(directory)
    (root/'tests.rs').write_text(source)
    subprocess.run(['rustc','--edition=2024','--test',str(root/'tests.rs'),'-o',str(root/'tests')],check=True)
    subprocess.run([str(root/'tests')],check=True)
