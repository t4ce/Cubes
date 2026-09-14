#!/usr/bin/env python3
"""Host tests for the Key9 controls, budgets, mode routing and marker admission."""
from pathlib import Path
import subprocess,tempfile
ROOT=Path(__file__).resolve().parents[1]
# The kernel's retained-static path only admits LINE_LIST draws of <=128
# indices. Keep the startup floor on a fixed mesh contract across modes.
import re
floor_source=(ROOT/'src/floor.rs').read_text()
main_source=(ROOT/'src/main.rs').read_text()
static_vertices=int(re.search(r'pub const VERTICES: usize = (\d+);',floor_source)[1])
assert 0 < static_vertices <= 128 and static_vertices % 2 == 0
static_draw=main_source.split('static_draws: [',1)[1].split('..trueos::vgpu::IndexedBatchDrawV2::default()',1)[0]
assert 'index_count: floor::VERTICES as u32' in static_draw
assert 'topology: trueos::vgpu::PRIMITIVE_TOPOLOGY_LINE_LIST,' in static_draw
source='''#![allow(dead_code)]
extern crate alloc;
extern crate self as libm;
pub fn sqrtf(x:f32)->f32{x.sqrt()}
pub fn floorf(x:f32)->f32{x.floor()}
pub fn roundf(x:f32)->f32{x.round()}
pub fn tanf(x:f32)->f32{x.tan()}
pub fn sinf(x:f32)->f32{x.sin()}
pub fn cosf(x:f32)->f32{x.cos()}
pub fn expf(x:f32)->f32{x.exp()}
'''
for module,file in [('orchard','orchard'),('subcubes','SubCubes'),('cube_format','cube_format'),('asset_brush','asset_brush'),('marker_lod','marker_lod'),('modes','modes'),('render_limits','render_limits')]:
    source+=f'#[path="{ROOT}/src/{file}.rs"] mod {module};\n'
source+='''
#[test]
fn world_markers_obey_every_detail_step_even_when_platforms_request_solids() {
    let source:Vec<_>=(0..8192).map(|i|orchard::Cube{center:[i as f32*0.01,0.,-100.],scale:0.8,flags:orchard::CUSTOM_RGB555|31}).collect();
    let visible:Vec<_>=(0..source.len()).collect();
    let view=[1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.];
    let mut reducer=marker_lod::Reducer::new();
    for step in 0..16 {
        let cap=render_limits::value(768,3840,step);
        reducer.prepare_with_solids(&source,&visible,[0.;3],&view,2.414,480,cap,|_|1.,|_|true);
        assert_eq!(reducer.cubes.iter().filter(|c|c.scale>=0.001).count(),cap);
        assert!(reducer.cubes.len()<=8192);
    }
}
'''
with tempfile.TemporaryDirectory(prefix='cubes-limits-') as tmp:
    p=Path(tmp);(p/'tests.rs').write_text(source)
    subprocess.run(['rustc','--edition=2024','-O','--test',str(p/'tests.rs'),'-o',str(p/'tests')],check=True)
    subprocess.run([str(p/'tests')],check=True)
