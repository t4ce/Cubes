#!/usr/bin/env python3
"""Exercise the Cubes-owned walker, tier collision, and bundled geometry pages."""
from pathlib import Path
import json
import subprocess
import tempfile

APP = Path(__file__).resolve().parents[1]
source = '''#![allow(dead_code)]
extern crate alloc;
extern crate self as libm;
extern crate self as cubes_protocol;
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
source += f'#[path="{APP}/src/slideshow.rs"] mod slideshow;\n'
source += f'#[path="{APP.parent}/TRUEOS-Blueprints/crates/cubes-protocol/src/gallery.rs"] pub mod gallery;\n'
source += f'#[path="{APP.parent}/TRUEOS-Blueprints/crates/cubes-protocol/src/holy.rs"] pub mod holy;\n'
source += f'#[path="{APP}/src/orchard.rs"] mod orchard;\n'
source += f'#[path="{APP}/src/asset_brush.rs"] mod asset_brush;\n'
source += f'#[path="{APP}/src/cube_format.rs"] mod cube_format;\n'
source += f'#[path="{APP}/src/SubCubes.rs"] mod subcubes;\n'
source += f'#[path="{APP.parent}/TRUEOS-Picasso/src/cam.rs"] pub mod cam;\n'
source += f'#[path="{APP}/src/CubesWalkerCam.rs"] mod walker_camera;\n'
source += f'#[path="{APP}/src/cubepathfind.rs"] mod cubepathfind;\n'
source += f'#[path="{APP}/src/flight_target.rs"] mod flight_target;\n'
source += f'#[path="{APP}/src/floor.rs"] mod floor;\n'
source += 'const WORLD_PAGES: &[&[u8]] = &[\n'
for path in sorted((APP/'Cube/lvl27').glob('*.cubes')):
    source += f'include_bytes!("{path}"),\n'
source += '];\n'
materials = json.loads((APP/'Cube/subcubes-materials.json').read_text())['materials']
palette = []
for name in ['red', 'orange', 'yellow', 'green', 'blue', 'violet']:
    rgb = next(m['rgb'] for m in materials if m['id'] == name)
    palette.append(0xff000000 | sum(int(rgb[axis]*255+0.5) << (8*i) for i, axis in enumerate('rgb')))
source += f'const MATERIAL_PALETTE: [u32; 6] = {palette};\n'
source += r'''
#[test]
fn authored_world_colors_select_the_six_imported_theme_materials() {
    for (world, expected) in [4, 1, 5, 2, 3, 0].into_iter().enumerate() {
        let bytes = WORLD_PAGES[world];
        let rgb = (0..3).map(|a| ((bytes[16+a] as u32*31+127)/255) << (a*5)).sum::<u32>();
        let cubes = [orchard::Cube {center:[0.;3],scale:1.,flags:orchard::CUSTOM_RGB555 | rgb}];
        let target = walker_camera::LandingTarget {center:[0.;3],scale:1.,normal:[0.,1.,0.]};
        let mut indicator = flight_target::Indicator::default();
        indicator.update(Some(target), 0, &cubes, &MATERIAL_PALETTE);
        let shown = indicator.update(Some(target), 700, &cubes, &MATERIAL_PALETTE).unwrap();
        assert_eq!(shown.flags & 7, expected);
    }
}
#[test]
fn distant_world_cubes_reach_detail_or_marker_selection() {
    let camera = walker_camera::CubesWalkerCam::from_world(WORLD_PAGES[0], false);
    let far = camera.far_plane();
    assert!(far > 705. && far < 715.);
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
