#!/usr/bin/env python3
"""Real redb, profile HTTP handlers/client, .cubes geometry and walker on the host."""
from pathlib import Path
import subprocess, tempfile, json
APP = Path(__file__).resolve().parents[1]
SERVER = APP.parent / "TRUEOS-Blueprints/apps/cubesrv"
with tempfile.TemporaryDirectory(prefix="cubes-plateau-") as directory:
    root = Path(directory)
    (root / "Cargo.toml").write_text(f'''[package]
name = "cubes-plateau-test"
version = "0.0.0"
edition = "2024"
[dependencies]
libm = "0.2"
serde = {{ version = "=1.0.228", features = ["derive"] }}
serde_json = "=1.0.150"
axum = {{ version = "=0.8.9", default-features = false, features = ["http1", "json", "tokio"] }}
reqwest = {{ version = "=0.13.3", default-features = false, features = ["json"] }}
tokio = {{ version = "=1.52.3", features = ["rt", "macros", "net", "sync", "time"] }}
trueos-redb = {{ path = "{APP.parent}/TRUEOS-Blueprints/crates/trueos-redb", features = ["std"] }}
[lib]
path = "lib.rs"
''')
    palette = b''.join(p.read_bytes()[16:20] for p in sorted((APP/'Cube/lvl27').glob('*.cubes'))[:6])
    assert palette == (APP.parent/'TRUEOS-Blueprints/crates/cubes-protocol/palette.rgba').read_bytes()
    source = '''#![allow(dead_code)]
extern crate alloc;
extern crate self as cubes_protocol;
extern crate self as trueos;
extern crate self as trueos_picasso;
pub use tokio;
pub mod worker {
    pub fn spawn(f: impl FnOnce() + Send + 'static) -> Result<(), ()> {
        std::thread::Builder::new().spawn(f).map(|_| ()).map_err(|_| ())
    }
}
pub mod runtime {
    pub fn current_thread_net() -> tokio::runtime::Builder {
        let mut b = tokio::runtime::Builder::new_current_thread(); b.enable_all(); b
    }
}
pub mod logl { pub mod level {pub const WARN:u8=1;} pub fn log(_:u8,_:core::fmt::Arguments) {} }
'''
    source += (APP / 'tools/plateau_fs_host.rs').read_text()
    for module, path in [('plateau', APP.parent/'TRUEOS-Blueprints/crates/cubes-protocol/src/lib.rs'), ('profiles', SERVER/'profiles.rs'),
                         ('protocol', SERVER/'protocol.rs'), ('plateau_client', APP/'src/plateau_client.rs'),
                         ('cam', APP.parent/'TRUEOS-Picasso/src/cam.rs'),
                         ('walker_camera', APP/'src/CubesWalkerCam.rs'), ('subcubes', APP/'src/SubCubes.rs'),
                         ('cubepathfind', APP/'src/cubepathfind.rs'),
                         ('cube_format', APP/'src/cube_format.rs'), ('orchard', APP/'src/orchard.rs'),
                         ('slideshow', APP/'src/slideshow.rs'), ('floor', APP/'src/floor.rs'), ('asset_brush', APP/'src/asset_brush.rs')]:
        source += f'#[path="{path}"] pub mod {module};\n'
    source += 'pub use plateau::gallery;\n'
    source += 'const WORLD_PAGES: &[&[u8]] = &[\n' + ''.join(f'include_bytes!("{p}"),\n' for p in sorted((APP/'Cube/lvl27').glob('*.cubes'))) + '];\n'
    editor = (APP/'Cube/WorldShowcase.html').read_text()
    start = editor.index('function moduleVoxels(')
    function = editor[start:editor.index('\nfunction ', start+1)]
    js = "const SLOT=8; const cellOrigin=()=>({x:0,y:-88,z:0}); const cellKey=()=>''; const platformProfile=()=>({size:16});\n" + function
    js += "\nconst cells=[]; moduleVoxels({}, {}, {kind:'manual',cell:[0,0,0]}, (x,y,z)=>cells.push([x,y,z])); process.stdout.write(JSON.stringify(cells));"
    fixture = subprocess.check_output(['node', '-e', js], text=True)
    source += f'const EDITOR_TERRACE: &str = {json.dumps(fixture)};\n'
    source += (APP / 'tools/plateau_regression.rs').read_text()
    (root / 'lib.rs').write_text(source)
    subprocess.run(['cargo', 'test', '--manifest-path', str(root/'Cargo.toml'),
                    '--target-dir', str(APP/'target/plateau-host'), '--', '--test-threads=1'], check=True)
