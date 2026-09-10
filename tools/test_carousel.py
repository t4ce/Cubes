#!/usr/bin/env python3
"""Exercise the carousel on every real exported asset and generator group."""
from pathlib import Path
import json, subprocess, tempfile
ROOT=Path(__file__).resolve().parents[1]
assets=sorted((ROOT/'Cube/Assets').glob('*.cubes'))
groups=json.loads((ROOT/'Cube/asset-groups.json').read_text())['groups']
source='''#![allow(dead_code)]
extern crate alloc;
extern crate self as libm;
pub fn sqrtf(x:f32)->f32{x.sqrt()}
pub fn floorf(x:f32)->f32{x.floor()}
pub fn roundf(x:f32)->f32{x.round()}
'''
for name,file in [('orchard','orchard'),('subcubes','SubCubes'),('cube_format','cube_format'),('reveal','reveal'),('carousel','carousel')]:
 source+=f'#[path="{ROOT}/src/{file}.rs"] mod {name};\n'
source+='static ASSETS: &[(&str,&[u8])] = &[\n'+''.join(f'("{p.name}",include_bytes!("{p}")),\n' for p in assets)+'];\n'
source+='static GROUPS: &[(&str,&[usize])] = &[\n'+''.join('('+json.dumps(g['name'])+', &'+str([assets.index(ROOT/'Cube/Assets'/n) for n in g['assets']])+'),\n' for g in groups)+'];\n'
with tempfile.TemporaryDirectory(prefix='cubes-carousel-') as folder:
 p=Path(folder);(p/'tests.rs').write_text(source)
 subprocess.run(['rustc','--edition=2024','-O','--test',str(p/'tests.rs'),'-o',str(p/'tests')],check=True)
 subprocess.run([str(p/'tests'),'carousel::tests','--nocapture'],check=True)
