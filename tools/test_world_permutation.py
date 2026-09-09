#!/usr/bin/env python3
"""Run the real lattice, portal animation, picking and companion on host assets."""
from pathlib import Path
import subprocess
import tempfile

APP = Path(__file__).resolve().parents[1]
source = '''#![allow(dead_code)]
extern crate alloc;
extern crate self as libm;
pub fn sqrtf(x:f32)->f32 {x.sqrt()}
pub fn sinf(x:f32)->f32 {x.sin()}
pub fn cosf(x:f32)->f32 {x.cos()}
pub fn atan2f(y:f32,x:f32)->f32 {y.atan2(x)}
pub fn floorf(x:f32)->f32 {x.floor()}
'''
for module in ('rubik', 'grid', 'picking', 'orchard', 'environment', 'world_topology', 'world_portals', 'world_cube'):
    source += f'#[path="{APP}/src/{module}.rs"] mod {module};\n'
source += 'const ASSETS: &[(&str, &[u8])] = &[\n'
for path in sorted((APP/'Cube/lvl27').glob('*.cubes')):
    source += f'("{path.name}",include_bytes!("{path}")),\n'
source += '];\n'
source += r'''
#[test]
fn entry_uses_current_permutation_and_real_authored_floors_never_change() {
    let mut puzzle = rubik::Puzzle::new(0);
    assert!(puzzle.select(0,0));
    for now in [1000,2000,3000,4000] {puzzle.update(now);}
    for (world,&(name,bytes)) in ASSETS.iter().enumerate() {
        let mut asset = orchard::decode(name,bytes).unwrap();
        for cube in &mut asset.cubes {cube.center=orchard::world_from_demo(cube.center);}
        let scene = world_portals::World::new(world,&asset,bytes,&puzzle);
        let records = bytes[16+bytes[10] as usize*4..].chunks_exact(8);
        for ((a,b),r) in asset.cubes.iter().zip(&scene.scene.cubes).zip(records) {
            assert_eq!(a.center,b.center); assert_eq!(a.scale,b.scale);
            if r[5] == 0 || world == world_topology::VOID {assert_eq!(a.flags,b.flags);}
        }
    }
}
#[test]
fn picking_tracks_permuted_identities_from_the_actual_visible_side() {
    let mut p = rubik::Puzzle::new(0);
    assert!(p.select(0,0));
    for now in [1000,2000,3000,4000] {p.update(now);}
    for id in 0..27 {
        let (cell,_) = p.pose(id,0.,1.);
        if cell.iter().filter(|&&x|x!=0.).count()<2 {continue;}
        let axis = cell.iter().position(|&x|x!=0.).unwrap();
        let mut origin = cell.map(|x|x*grid::CUBE_COMPACT_SPACING);
        origin[axis] = cell[axis]*10.;
        let mut direction=[0.;3]; direction[axis]=-cell[axis];
        assert_eq!(picking::pick_poses(origin,direction,grid::CUBE_COMPACT_SPACING,
            grid::CUBE_GRID_SCALE,|i|p.pose(i,0.,1.)),Some(id));
    }
}
'''
with tempfile.TemporaryDirectory(prefix='cubes-world-permutation-') as directory:
    root = Path(directory)
    (root/'tests.rs').write_text(source)
    subprocess.run(['rustc','--edition=2024','--test',str(root/'tests.rs'),'-o',str(root/'tests')],check=True)
    subprocess.run([str(root/'tests')],check=True)
