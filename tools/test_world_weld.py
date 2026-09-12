#!/usr/bin/env python3
"""Host adjacency/geometry checks and CPU timing; no GPU/FPS claim."""
from pathlib import Path
import subprocess
import tempfile
import unittest
import struct
import re
from bake_patch_cube import ROOT, PALETTE, load_palette, weld_colors, write_sources, geometry


class WorldWeldTests(unittest.TestCase):
    def test_shader_face_winding_mask_and_instance_transport(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            write_sources(ROOT / 'Cube/cube.glb', out)
            hs = (out/'cube.tesc').read_text()
            vs = (out/'cube.vert').read_text()
            ds = (out/'cube.tese').read_text()
            _, triangles = geometry(ROOT / 'Cube/cube.glb')
            faces = re.findall(r'if \(n\.([xyz]) ([<>]) (-?0\.9999)\) face = (\d);',hs)
            self.assertEqual(len(faces),6)
            self.assertIn('p = sign(p);',hs)
            self.assertIn('face >= 0 && (mask & (1u << uint(max(face, 0)))) == 0u ? 1.0 : 0.0',hs)
            self.assertIn('floor(abs(instanceID[0]))',hs)
            per_face = [0]*6
            for triangle in triangles:
                normal = triangle[0][1]
                face = next((int(f) for a,op,t,f in faces
                    if (normal['xyz'.index(a)] > float(t) if op == '>' else normal['xyz'.index(a)] < float(t))),None)
                if face is None:
                    continue
                per_face[face] += 1
                points = [tuple(1 if x>0 else -1 for x in p) for p,n in triangle]
                u,v = [tuple(points[k][i]-points[0][i] for i in range(3)) for k in (1,2)]
                cross = (u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0])
                self.assertGreater(sum(a*b for a,b in zip(cross,normal)),0)
                axis = face//2
                self.assertTrue(all(p[axis] == (1 if face%2==0 else -1) for p in points))
            self.assertEqual(per_face,[2]*6)
            self.assertIn('float((flags >> 4u) & 63u) / 128.0',vs)
            self.assertIn('if ((flags & 63488u) == 6144u) transparentPass = false;',ds)
            for row in (0,1,1023,4095,8191):
                for mask in range(1,64):
                    encoded = struct.unpack('<f',struct.pack('<f',-(row+1)-mask/128))[0]
                    self.assertEqual(int(-encoded)-1,row)
                    self.assertEqual(round((-encoded-int(-encoded))*128),mask)
                    for color in range(16):
                        flags = (row<<16) | 0x1800 | (mask<<4) | color
                        self.assertEqual(flags>>16,row)
                        self.assertEqual(flags & 0xf800,0x1800)
            # Full row of n cubes: 4n+2 exterior quads, no internal faces.
            for n in (2,3,20):
                masks = [1 if i==0 else 2 if i==n-1 else 3 for i in range(n)]
                triangles = sum(2*(6-m.bit_count()) for m in masks)
                self.assertEqual(triangles,8*n+4)

    def test_weld_class_is_disjoint_from_all_existing_cube_encodings(self):
        # Check the predicates actually emitted to both stages: a CPU-only
        # encoder test misses a shader interpreting Rubik faces 4/5 as welds.
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            write_sources(ROOT / 'Cube/cube.glb', out)
            vs, ds = [(out / f'cube.{stage}').read_text() for stage in ('vert', 'tese')]
            predicates = re.findall(r'\(flags & (63488)u\) == (\d+)u', vs + ds)
            self.assertEqual(len(predicates), 3)  # VS geometry, DS color and opacity
            self.assertEqual(len(set(predicates)), 1)
            field_mask, tag = map(int, predicates[0])
            cpu = (ROOT / 'src/world_weld.rs').read_text()
            self.assertEqual(tag, int(re.search(r'pub const CLASS: u32 = (0x[0-9a-f]+);', cpu)[1], 16))
            rubik = (ROOT / 'src/rubik.rs').read_text()
            flag = lambda name: int(re.search(rf'pub const {name}: u32 = (\d+);', rubik)[1])
            legacy = {0, 8192, 16384, 512}
            for cubie in range(27):
                for all_faces in (0, flag('ALL_FACES_FLAG')):
                    body = flag('PALETTE_FLAG') | all_faces | cubie
                    legacy.add(body)
                    for face in range(6):
                        transparent_face = body | 512 | (face << 10)
                        self.assertNotEqual(transparent_face & field_mask, tag,
                            f'Rubik cubie={cubie}, face={face}, all_faces={bool(all_faces)} enters weld path')
                        legacy.add(transparent_face)
            legacy.update(0x8000 | rgb for rgb in range(32768))
            legacy.update(24576 | material for material in range(6))
            for opacity in range(4):
                legacy.update(25088 | (opacity << 10) | color for color in range(512))
                legacy.update(25088 | 4096 | (opacity << 10) | material for material in range(6))
            for flags in legacy:
                self.assertNotEqual(flags & field_mask, tag, hex(flags))
            for material in (0, 0x0400):
                for mask in range(64):
                    for color in range(16):
                        flags = tag | material | (mask << 4) | color
                        self.assertNotIn(flags, legacy)
                        self.assertEqual(flags & field_mask, tag)
                        self.assertEqual((flags >> 4) & 63, mask)
                        self.assertEqual(flags & 15, color)
                        self.assertEqual(flags & 0x0400, material)

    def test_rust_and_authored_worlds(self):
        source = '''#![allow(dead_code)]
extern crate alloc;
extern crate self as libm;
pub fn sqrtf(x:f32)->f32{x.sqrt()}
pub fn floorf(x:f32)->f32{x.floor()}
pub fn roundf(x:f32)->f32{x.round()}
'''
        for module,file in [('orchard','orchard'),('asset_brush','asset_brush'),('subcubes','SubCubes'),('cube_format','cube_format'),('world_weld','world_weld')]:
            source += f'#[path="{ROOT}/src/{file}.rs"] mod {module};\n'
        colors = weld_colors(load_palette(PALETTE)[1])
        source += f'const COLORS: &[u32] = &{colors};\n'
        paths = sorted((ROOT/'Cube/lvl27').glob('*.cubes'))
        source += 'const WORLDS: &[(&str,&[u8])] = &[\n' + ''.join(f'("{p.name}",include_bytes!("{p}")),\n' for p in paths) + '];\n'
        source += r'''
#[test]
fn authored_worlds_and_warm_cpu_cost() {
    use std::{hint::black_box,time::Instant};
    let mut total = 0;
    for &(name,bytes) in WORLDS {
        let asset = orchard::decode(name,bytes).unwrap();
        let solids: Vec<_> = (0..asset.cubes.len()).map(|i|(i,i)).collect();
        let mut cubes = asset.cubes.clone();
        let mut welder = world_weld::Welder::default(); welder.enabled = true;
        welder.prepare(&mut cubes,&solids,COLORS,|_|true);
        assert!(welder.joined > 0, "no welds in {name}");
        total += welder.joined;
        println!("{name}: source={} joined={} internal_faces={} triangles={} -> {} (all authored cubes, before view/LOD)",
            cubes.len(),welder.joined,welder.removed_faces,cubes.len()*44,cubes.len()*44-welder.triangles_saved());
        // Reversing submission order preserves geometry savings.
        let saved = welder.triangles_saved();
        cubes.clone_from(&asset.cubes); cubes.reverse();
        welder.prepare(&mut cubes,&solids,COLORS,|_|true);
        assert_eq!(saved,welder.triangles_saved());
    }
    assert!(total > 1000);
    let cubes: Vec<_> = (0..8192).map(|i|orchard::Cube {
        center: [(i%128) as f32*0.2,(i/128) as f32*0.2,0.],
        scale: 0.0986,flags: orchard::CUSTOM_RGB555|COLORS[0],
    }).collect();
    let solids: Vec<_> = (0..cubes.len()).map(|i|(i,i)).collect();
    let mut output = cubes.clone();
    let mut welder = world_weld::Welder::default(); welder.enabled=true;
    welder.prepare(&mut output,&solids,COLORS,|_|true);
    let start = Instant::now();
    for _ in 0..100 {
        output.clone_from(&cubes);
        welder.prepare(black_box(&mut output),black_box(&solids),COLORS,|_|true);
        black_box(&output);
    }
    println!("8192-cube weld preparation: {:.1} us/frame (100 warm CPU iterations)",start.elapsed().as_secs_f64()*1e4);
}
'''
        with tempfile.TemporaryDirectory() as tmp:
            out=Path(tmp); (out/'tests.rs').write_text(source)
            subprocess.run(['rustc','--edition=2024','-O','--test',str(out/'tests.rs'),'-o',str(out/'tests')],check=True)
            subprocess.run([str(out/'tests'),'world_weld::tests'],check=True)
            subprocess.run([str(out/'tests'),'authored_worlds_and_warm_cpu_cost','--nocapture'],check=True)

if __name__ == '__main__':
    unittest.main()
