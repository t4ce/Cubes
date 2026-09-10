#!/usr/bin/env python3
"""Test world marker reduction and report its warm-buffer CPU cost (not GPU FPS)."""
from pathlib import Path
import subprocess
import tempfile

APP = Path(__file__).resolve().parents[1]
source = '''#![allow(dead_code)]
extern crate alloc;
extern crate self as libm;
pub fn sqrtf(x:f32)->f32{x.sqrt()}
pub fn floorf(x:f32)->f32{x.floor()}
pub fn roundf(x:f32)->f32{x.round()}
'''
for module, file in [('orchard','orchard'), ('asset_brush','asset_brush'),
                     ('subcubes','SubCubes'), ('cube_format','cube_format'), ('marker_lod','marker_lod')]:
    source += f'#[path="{APP}/src/{file}.rs"] mod {module};\n'
source += r'''
#[test]
#[ignore]
fn benchmark_marker_lod() {
    use std::{hint::black_box, time::Instant};
    let cubes: Vec<_> = (0..8192).map(|i| orchard::Cube {
        center: [(i % 128) as f32 * 1.6 - 102.4, (i / 128) as f32 * 1.6 - 51.2, 0.],
        scale: 0.1, flags: orchard::CUSTOM_RGB555 | (i as u32 & 0x7fff),
    }).collect();
    let ids: Vec<_> = (0..cubes.len()).collect();
    let view = [1.,0.,0.,0.,0.,1.,0.,0.,0.,0.,1.,0.,0.,0.,-710.,1.];
    let eye = [0.,0.,710.];
    let mut reducer = marker_lod::Reducer::new();
    reducer.prepare(&cubes, &ids, eye, &view, 2.414, 441);
    let count = reducer.cubes.len();
    let iterations = 1000;
    let start = Instant::now();
    for _ in 0..iterations {
        for (rank, &id) in black_box(&ids).iter().enumerate() {
            let cube = cubes[id];
            let distance = asset_brush::lod_distance_squared(cube.center, eye, &view);
            let scale = if asset_brush::detailed(rank, cube, distance, 2.414, 441) { cube.scale }
                else { asset_brush::marker_scale(cube.scale, 710., 2.414, 441) };
            black_box((cube.center, scale, cube.flags));
        }
    }
    let baseline = start.elapsed().as_secs_f64() * 1e6 / iterations as f64;
    let start = Instant::now();
    for _ in 0..iterations {
        reducer.prepare(black_box(&cubes), black_box(&ids), black_box(eye), &view, 2.414, 441);
        black_box(&reducer.cubes);
    }
    let reduced = start.elapsed().as_secs_f64() * 1e6 / iterations as f64;
    println!("8192 dots at one world diagonal ahead -> {count} submitted dots; marker preparation baseline={baseline:.1}us grouped={reduced:.1}us/frame (CPU only, 1000 warm frames)");
    assert!(count <= 8192 / 4 + 128, "double forward reach keeps this fixture near the 4:1 band");
}
'''
with tempfile.TemporaryDirectory(prefix='cubes-marker-lod-') as temporary:
    root = Path(temporary)
    (root / 'tests.rs').write_text(source)
    subprocess.run(['rustc','--edition=2024','-O','--test',str(root/'tests.rs'),'-o',str(root/'tests')],check=True)
    subprocess.run([str(root/'tests'),'marker_lod::tests'],check=True)
    subprocess.run([str(root/'tests'),'benchmark_marker_lod','--ignored','--nocapture'],check=True)
