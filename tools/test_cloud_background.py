#!/usr/bin/env python3
"""Verify Cubes embeds the authenticated, unchanged Protean Clouds package."""
from pathlib import Path
import hashlib
import re
import struct
APP = Path(__file__).resolve().parents[1]
OS = APP.parent / 'TRUEOS'
data = (APP/'Cube/protean_clouds/protean_clouds.stpkg').read_bytes()
source = (APP.parent/'TRUEOS-Blueprints/apps/shadertoy/assets/protean_clouds.stpkg').read_bytes()
assert data == source, 'Cubes cloud package differs from the catalog'
assert data[:8] == b'STPKG01\0'
lengths = struct.unpack_from('<6I', data, 8)
assert len(data) == 32 + sum(lengths)
trust = (OS/'src/intel/gpgpu/artifacts/shadertoy_packages.rs').read_text()
entry = re.search(r'SHADERTOY_PROTEAN_CLOUDS_PACKAGE:.*?sha256: \[([^]]+)\]', trust, re.S).group(1)
expected = bytes(int(x.strip(), 16) for x in entry.split(',') if x.strip())
assert hashlib.sha256(data).digest() == expected, 'cloud package does not match kernel trust'
print('Protean source package, six payload lengths, and kernel trust agree')
