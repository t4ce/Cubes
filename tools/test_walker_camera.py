#!/usr/bin/env python3
"""Exercise the production walker against the HTML reference and all world pages."""
from pathlib import Path
import json
import argparse
import subprocess
import tempfile

APP = Path(__file__).resolve().parents[1]
# Run the reference's actual math/controller, without loading its UI or scripts.
reference_js = r'''
const fs=require('fs');
const html=fs.readFileSync(process.argv[1],'utf8');
const math=html.slice(html.indexOf('class V3 {'),html.indexOf('\n',html.indexOf('function lookQuaternion')));
const controller=html.slice(html.indexOf('const SURFACE_SKIN='),html.indexOf('/* App / editor'));
const ray=html.slice(html.indexOf('function gridRay('),html.indexOf('\n',html.indexOf('function gridRay(')));
const fixtures=`
const clamp=(x,a,b)=>Math.max(a,Math.min(b,x));
const voxelKey=(...p)=>p.join(',');
const currentGeometry={voxels:new Map()};
for(let x=0;x<4;x++)for(let y=0;y<4;y++)for(let z=0;z<4;z++)currentGeometry.voxels.set(voxelKey(x,y,z),{x,y,z});
const CHUNK=64,data=()=>({spanChunks:1});
const exploreSettings={cameraAssist:100,edgePerch:true};
const explorer={foot:new V3(3.5,4.025,1.5),up:new V3(0,1,0),forward:new V3(1,0,0),pitch:0,view:new Q4(),turn:null,transitions:0};
resetView();
let rows=[];
for(const distance of [.2,.5,.6,-.3,.8,.4,.9,1.2,2.1,3.2,.6,-.4]){
 spiderStep(explorer.forward.clone().multiplyScalar(Math.sign(distance)),Math.abs(distance));
 rows.push([...explorer.foot.toArray(),...explorer.up.toArray(),...explorer.forward.toArray(),...explorer.view.toArray(),explorer.turn?.progress??-1]);
}
console.log(JSON.stringify(rows));`;
eval(math+'\n'+ray+'\n'+controller+'\n'+fixtures);
'''
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--reference', type=Path, default=APP/'Cube/WorldShowcase.html')
args = parser.parse_args()
rows = json.loads(subprocess.check_output(['node', '-e', reference_js, str(args.reference)], text=True))
source = '''#![allow(dead_code)]
extern crate alloc;
extern crate self as libm;
extern crate self as trueos_picasso;
pub fn sqrtf(x:f32)->f32 {x.sqrt()}
pub fn sinf(x:f32)->f32 {x.sin()}
pub fn cosf(x:f32)->f32 {x.cos()}
pub fn acosf(x:f32)->f32 {x.acos()}
pub fn expf(x:f32)->f32 {x.exp()}
pub fn floorf(x:f32)->f32 {x.floor()}
pub fn ceilf(x:f32)->f32 {x.ceil()}
'''
source += f'#[path="{APP.parent}/TRUEOS-Picasso/src/cam.rs"] pub mod cam;\n'
source += f'#[path="{APP}/src/CubesWalkerCam.rs"] mod walker_camera;\n'
source += f'#[path="{APP}/src/floor.rs"] mod floor;\n'
source += 'const REFERENCE_TRACES: &[[f32;14]] = &[\n'
for row in rows:
    source += '[' + ','.join(f'{float(x):.10f}' for x in row) + '],\n'
source += '];\nconst WORLD_PAGES: &[&[u8]] = &[\n'
for path in sorted((APP/'Cube/lvl27').glob('*.cubes')):
    source += f'include_bytes!("{path}"),\n'
source += '];\n'
with tempfile.TemporaryDirectory(prefix='cubes-walker-') as directory:
    root = Path(directory)
    (root/'tests.rs').write_text(source)
    subprocess.run(['rustc','--edition=2024','--test',str(root/'tests.rs'),'-o',str(root/'tests')],check=True)
    subprocess.run([str(root/'tests')],check=True)
