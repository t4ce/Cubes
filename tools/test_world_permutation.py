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
pub fn roundf(x:f32)->f32 {x.round()}
'''
source += f'#[path="{APP}/src/cube_format.rs"] mod cube_format;\n'
source += f'#[path="{APP}/src/SubCubes.rs"] mod subcubes;\n'
for module in ('rubik', 'grid', 'picking', 'orchard', 'environment', 'world_topology', 'world_portals', 'world_cube', 'transition', 'asset_brush'):
    source += f'#[path="{APP}/src/{module}.rs"] mod {module};\n'
source += 'const ASSETS: &[(&str, &[u8])] = &[\n'
for path in sorted((APP/'Cube/lvl27').glob('*.cubes')):
    source += f'("{path.name}",include_bytes!("{path}")),\n'
source += '];\n'
source += 'const BRUSH_ASSETS: &[(&str, &[u8])] = &[\n'
for path in sorted((APP/'Cube/Assets').glob('*.cubes')):
    source += f'("{path.name}",include_bytes!("{path}")),\n'
source += '];\n'
source += r'''
#[test]
fn all_catalog_assets_place_on_grid() {
    for &(name,bytes) in BRUSH_ASSETS {
        let asset=orchard::decode(name,bytes).unwrap();
        for axis in 0..3 { for sign in [-1.,1.] {
            let mut n=[0.;3];n[axis]=sign;
            let pieces=asset_brush::place(bytes,[0.;3],n);
            assert_eq!(pieces.len(),asset.cubes.len());
            for c in pieces {
                let half=roundf(c.scale*40.)*0.025;
                assert!(c.center[axis]*sign-half > -0.0001);
                for a in 0..3 {
                    let lo=(c.center[a]-half)*20.;
                    assert!((lo-roundf(lo)).abs()<0.001);
                }
            }
        } }
    }
}
#[test]
fn entry_uses_current_permutation_and_real_authored_floors_never_change() {
    let mut puzzle = rubik::Puzzle::new(0);
    assert!(puzzle.select(0,0));
    for now in [1000,2000,3000,4000] {puzzle.update(now);}
    for (world,&(name,bytes)) in ASSETS.iter().enumerate() {
        let mut asset = orchard::decode(name,bytes).unwrap();
        for cube in &mut asset.cubes {cube.center=orchard::world_from_demo(cube.center);}
        let scene = world_portals::World::new(world,&asset,bytes,&puzzle);
        let records = cube_format::cubes(bytes);
        for ((a,b),r) in asset.cubes.iter().zip(&scene.scene.cubes).zip(records) {
            assert_eq!(a.center,b.center); assert_eq!(a.scale,b.scale);
            if r.part == 0 || world == world_topology::VOID {assert_eq!(a.flags,b.flags);}
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
        if cell.iter().filter(|&&x|x!=0.).count()<1 {continue;}
        let axis = cell.iter().position(|&x|x!=0.).unwrap();
        let mut origin = cell.map(|x|x*grid::CUBE_COMPACT_SPACING);
        origin[axis] = cell[axis]*10.;
        let mut direction=[0.;3]; direction[axis]=-cell[axis];
        assert_eq!(picking::pick_poses(origin,direction,grid::CUBE_COMPACT_SPACING,
            grid::CUBE_GRID_SCALE,|i|p.pose(i,0.,1.)),Some(id));
    }
}
#[test]
fn every_exposed_sticker_click_keeps_its_world_and_leave_portal_after_turns() {
    for world in 0..world_topology::VOID {
        let id=world_topology::cubie(world);
        let solved=world_topology::solved_cell(world);
        let mut p=rubik::Puzzle::new(0);
        assert!(p.select(id,0));
        for now in [1000,2000,3000,4000] {p.update(now);}
        assert!(!p.locked(), "world {world} never completed its turns");
        let (cell,basis)=p.pose(id,0.,1.);
        for axis in 0..3 {
            if solved[axis]==0 {continue;}
            let outward=basis[axis].map(|x|x*solved[axis] as f32);
            let origin=core::array::from_fn(|a|cell[a]*grid::CUBE_COMPACT_SPACING+outward[a]*10.);
            let hit=picking::pick_face(origin,outward.map(|x|-x),grid::CUBE_COMPACT_SPACING,
                grid::CUBE_GRID_SCALE,|i|p.pose(i,0.,1.)).unwrap();
            assert_eq!(hit.cubie,id);
            assert_eq!(hit.face_axis,axis);
            let (destination,portal)=world_topology::entry(hit.cubie,hit.face_axis).unwrap();
            assert_eq!(destination,world);
            assert_eq!(world_topology::routes(world,&p)[portal],world_topology::Destination::Leave);
        }
    }
    assert_eq!(world_topology::entry(13,0),None);
}
'''
with tempfile.TemporaryDirectory(prefix='cubes-world-permutation-') as directory:
    root = Path(directory)
    (root/'tests.rs').write_text(source)
    subprocess.run(['rustc','--edition=2024','--test',str(root/'tests.rs'),'-o',str(root/'tests')],check=True)
    subprocess.run([str(root/'tests')],check=True)
