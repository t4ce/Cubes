#!/usr/bin/env python3
"""Host regression for the Key-5 world-01 joined surface builder."""
from pathlib import Path
import subprocess
import tempfile

APP = Path(__file__).resolve().parents[1]

with tempfile.TemporaryDirectory(prefix="cubes-joined-world-") as directory:
    root = Path(directory)
    source = f'''#![allow(dead_code)]
#[path="{APP / 'tools/build_joined_world.rs'}"] mod build_joined_world;
'''
    test_source = root / "tests.rs"
    test_source.write_text(source)
    binary = root / "tests"
    subprocess.run(
        ["rustc", "--edition=2024", "-O", "--test", str(test_source), "-o", str(binary)],
        check=True,
    )
    subprocess.run([str(binary), "--test-threads=1"], check=True)
