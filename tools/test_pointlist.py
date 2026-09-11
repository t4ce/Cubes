#!/usr/bin/env python3
"""Run actual Cubes background worker and native point packing against a lease mock.
No GPU/VM is launched; the SDK batch ABI and PotatoStamps geometry are real.
"""
from pathlib import Path
import subprocess, tempfile
ROOT=Path(__file__).resolve().parents[1]
source=(ROOT/'tools/pointlist_host.rs').read_text()
modules=''
for name,file in [('pointlist','pointlist'),('background','background'),('environment','environment'),('world_look','world_look'),('orchard','orchard'),('cube_format','cube_format'),('subcubes','SubCubes'),('asset_brush','asset_brush'),('marker_lod','marker_lod'),('modes','modes')]:
 modules+=f'#[path="{ROOT}/src/{file}.rs"] mod {name};\n'
source=source.replace('// MODULES',modules)
with tempfile.TemporaryDirectory(prefix='cubes-points-host-') as tmp:
 p=Path(tmp)
 (p/'tests.rs').write_text(source)
 (p/'Cargo.toml').write_text(f'''[package]
name="cubes-points-host"
version="0.1.0"
edition="2024"
[lib]
path="tests.rs"
[profile.test]
opt-level=2
[dependencies]
libm="0.2"
potato-stamps={{path="{ROOT.parent}/PotatoStamps"}}
trueos-sdk={{package="trueos",path="{ROOT.parent}/TRUEOS-Blueprints/api"}}
trueos-picasso={{path="{ROOT.parent}/TRUEOS-Picasso",default-features=false,features=["blueprint","test-std"]}}
''')
 subprocess.run(['cargo','test','--offline','--quiet','--manifest-path',str(p/'Cargo.toml'),'--target-dir',str(ROOT/'target/points-host'),'--','--test-threads=1'],check=True)
