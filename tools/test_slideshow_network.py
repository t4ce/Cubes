#!/usr/bin/env python3
"""Host-check the real client against the server packet encoders."""
from pathlib import Path
import subprocess
import tempfile
APP = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='cubes-network-') as directory:
    root = Path(directory)
    (root / 'Cargo.toml').write_text(f'''[package]
name = "cubes-network-test"
version = "0.0.0"
edition = "2024"
[dependencies]
trueos = {{ path = "{APP.parent}/TRUEOS-Blueprints/api", features = ["tokio-net-probe"] }}
[lib]
path = "lib.rs"
''')
    (root / 'lib.rs').write_text(f'''extern crate alloc;
#[path="{APP}/src/network.rs"] mod network;
// The packet tests do not call the Blueprint lifecycle/console ABI.
#[unsafe(no_mangle)] pub extern "C" fn trueos_cabi_write(_: u32, _: *const u8, _: usize) {{}}
#[unsafe(no_mangle)] pub extern "C" fn trueos_cabi_blueprint_shutdown(_: *const u8, _: usize) -> i32 {{ 0 }}
''')
    subprocess.run(['cargo', 'test', '--offline', '--manifest-path', str(root/'Cargo.toml'),
                    '--target-dir', str(APP/'target')], check=True)
