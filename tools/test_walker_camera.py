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
source += f'#[path="{APP}/src/asset_brush.rs"] mod asset_brush;\n'
source += f'#[path="{APP}/src/cube_format.rs"] mod cube_format;\n'
source += f'#[path="{APP}/src/SubCubes.rs"] mod subcubes;\n'
source += f'#[path="{APP.parent}/TRUEOS-Picasso/src/cam.rs"] pub mod cam;\n'
source += f'#[path="{APP}/src/CubesWalkerCam.rs"] mod walker_camera;\n'
source += f'#[path="{APP}/src/floor.rs"] mod floor;\n'
source += 'const WORLD_PAGES: &[&[u8]] = &[\n'
for path in sorted((APP/'Cube/lvl27').glob('*.cubes')):
    source += f'include_bytes!("{path}"),\n'
source += '];\n'
source += r'''
#[test]
fn distant_world_cubes_reach_detail_or_marker_selection() {
    let camera = walker_camera::CubesWalkerCam::from_world(WORLD_PAGES[0], false);
    let far = camera.far_plane();
    assert!(far > 790. && far < 810.);
    let height = 441;
    let focal = 1. / (walker_camera::FOV * 0.5).tan();
    // Standard retained perspective: forward -Z, depth range 0..1.
    let projection = |far: f32| {
        let near = walker_camera::NEAR;
        [focal / (784. / height as f32), 0., 0., 0.,
         0., focal, 0., 0.,
         0., 0., far / (near - far), -1.,
         0., 0., far * near / (near - far), 0.]
    };
    // c2 becomes a marker at this distance; c4 remains a detailed cube.
    for (side, depth, expected_detail) in [(2., 300., false), (8., 350., true)] {
        let cube = orchard::Cube {
            center: [0., 0., -depth],
            scale: (side - 0.014) * subcubes::C1 * 0.5,
            flags: orchard::CUSTOM_RGB555,
        };
        let asset = orchard::Asset { name: "distant", cubes: vec![cube], radius: depth };
        let mut scratch = orchard::VisibilityScratch::new();
        for (limit, expected_visible) in [(100., 0), (far, 1)] {
            let mut selected = None;
            let (ids, stats) = orchard::visible_with_lod(
                &mut scratch, &asset, [0.; 3], &projection(limit), 8192,
                |_| true,
                |id, rank| {
                    let view = [1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.];
                    let distance = asset_brush::lod_distance_squared(asset.cubes[id].center,[0.;3],&view);
                    let detail = asset_brush::detailed(rank, asset.cubes[id], distance, focal, height);
                    selected = Some(detail);
                    detail
                },
            );
            assert_eq!(stats.frustum, expected_visible);
            assert_eq!(ids.len(), expected_visible);
            assert_eq!(selected, (expected_visible != 0).then_some(expected_detail));
        }
        if !expected_detail {
            let scale = asset_brush::marker_scale(cube.scale, depth, focal, height);
            assert!(scale > 0. && scale < 0.001);
            let pixels = scale * 1000. * height as f32 * focal / depth;
            assert!((0.999..=9.001).contains(&pixels));
        }
    }
}
'''
with tempfile.TemporaryDirectory(prefix='cubes-walker-') as directory:
    root = Path(directory)
    (root/'tests.rs').write_text(source)
    subprocess.run(['rustc','--edition=2024','--test',str(root/'tests.rs'),'-o',str(root/'tests')],check=True)
    subprocess.run([str(root/'tests')],check=True)
