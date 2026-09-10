#!/usr/bin/env python3
"""Check the native beveled Palette Grid source/package/contract chain."""
from pathlib import Path
import hashlib
import re
import struct
import sys
APP = Path(__file__).resolve().parents[1]
OS = APP.parent/'TRUEOS'
assets = APP.parent/'TRUEOS-Blueprints/apps/shadertoy/assets'
sys.path.insert(0, str(OS/'tools/shadertoy-cpp-offline'))
from adapter import adapt
source = (assets/'palette_grid/input.glsl').read_text()
assert (APP/'Cube/palette_grid/input.glsl').read_text() == source
assert adapt(source, 'shadertoy_palette_grid') == (assets/'palette_grid/kernel.clcpp').read_text()
assert 'max(max(face.x, face.y), (face.x + face.y) / 1.8)' in source
assert 'sin(bevelRadius * 7.0 + iTime * 0.5)' in source
assert 'length(gridUV)' not in source
package = (APP/'Cube/palette_grid/palette_grid.stpkg').read_bytes()
assert package == (assets/'palette_grid.stpkg').read_bytes()
assert package[:8] == b'STPKG01\0'
lengths = struct.unpack_from('<6I', package, 8)
assert len(package) == 32 + sum(lengths)
trust = (OS/'src/intel/gpgpu/artifacts/shadertoy_packages.rs').read_text()
entry = re.search(r'SHADERTOY_PALETTE_GRID_PACKAGE:.*?sha256: \[([^]]+)\]', trust, re.S).group(1)
expected = bytes(int(x.strip(), 16) for x in entry.split(',') if x.strip())
assert hashlib.sha256(package).digest() == expected
print('Native beveled Palette Grid: source, generated kernel, package and trust agree')
