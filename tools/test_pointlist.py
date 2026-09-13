#!/usr/bin/env python3
"""Run actual Cubes background worker and native point packing against a lease mock.
No GPU/VM is launched; the SDK batch ABI and palette build function are real.
"""
from pathlib import Path
import json, subprocess, tempfile
ROOT=Path(__file__).resolve().parents[1]
source=(ROOT/'tools/pointlist_host.rs').read_text()
modules=''
for name,file in [('pointlist','pointlist'),('background','background'),('environment','environment'),('world_look','world_look'),('orchard','orchard'),('cube_format','cube_format'),('subcubes','SubCubes'),('asset_brush','asset_brush'),('marker_lod','marker_lod'),('modes','modes')]:
 modules+=f'#[path="{ROOT}/src/{file}.rs"] mod {name};\n'
source=source.replace('// MODULES',modules)
source+='\ninclude!(concat!(env!("OUT_DIR"),"/palette.rs"));\n'
palette=json.loads((ROOT/'Cube/subcubes-materials.json').read_text())
colors={m['id']:m['rgb'] for m in palette['materials']}
expected=[int.from_bytes(bytes([int(colors[id][c]*255+0.5) for c in ('r','g','b')]+[255]),'little')
          for id in ('red','orange','yellow','green','blue','violet')]
source+=f'const EXPECTED_PALETTE_RGBA: [u32;6] = {expected};\n'
build=(ROOT/'build.rs').read_text()
start=build.index('fn material_palette_registry()')
palette_build=build[start:build.index('\n}',start)+2]
with tempfile.TemporaryDirectory(prefix='cubes-points-host-') as tmp:
 p=Path(tmp)
 (p/'Cube').mkdir()
 # Reorder the input to prove the production build resolves stable material IDs.
 palette['materials'].reverse()
 (p/'Cube/subcubes-materials.json').write_text(json.dumps(palette))
 (p/'build.rs').write_text('use std::fs;\n'+palette_build+'\nfn main(){fs::write(std::path::Path::new(&std::env::var_os("OUT_DIR").unwrap()).join("palette.rs"),material_palette_registry()).unwrap();}\n')
 (p/'tests.rs').write_text(source)
 (p/'Cargo.toml').write_text(f'''[package]
name="cubes-points-host"
version="0.1.0"
edition="2024"
[lib]
path="tests.rs"
[profile.test]
opt-level=2
[build-dependencies]
serde_json="1"
[dependencies]
libm="0.2"
potato-stamps={{path="{ROOT.parent}/PotatoStamps"}}
trueos-sdk={{package="trueos",path="{ROOT.parent}/TRUEOS-Blueprints/api"}}
trueos-picasso={{path="{ROOT.parent}/TRUEOS-Picasso",default-features=false,features=["blueprint","test-std"]}}
''')
 subprocess.run(['cargo','test','--offline','--quiet','--manifest-path',str(p/'Cargo.toml'),'--target-dir',str(ROOT/'target/points-host'),'--','--test-threads=1'],check=True)
