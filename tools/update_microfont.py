#!/usr/bin/env python3
"""Refresh the HTML's printable ASCII masks from a MicroFont source checkout.

Then run export_interface_style.cjs to refresh the native Rust table.
Normal app builds use the checked-in data and need no source checkout.
"""

import argparse
import hashlib
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--source', type=Path, default=ROOT.parent / 'TRUEOS/vendor/microfont')
parser.add_argument('--check', action='store_true')
args = parser.parse_args()
version = tomllib.loads((args.source / 'Cargo.toml').read_text())['package']['version']
records = {p.stem.lower(): p for p in (args.source / 'data/line-v1').rglob('u*.bin')}
masks = []
digest = hashlib.sha256()
for codepoint in range(32, 127):
    data = records[f'u{codepoint:04x}'].read_bytes()
    if len(data) < 20 or data[16] != 1 or data[18:20] != b'\0\0':
        raise ValueError(f'Missing or invalid bitmap for U+{codepoint:04X}')
    mask = int.from_bytes(data[:16], 'little')
    if mask >> 66:
        raise ValueError(f'Oversized bitmap for U+{codepoint:04X}')
    masks.append(mask)
    digest.update(codepoint.to_bytes(4, 'little') + data[:16])

path = ROOT / 'Cube/AssetShowcase.html'
original = path.read_text()
text = re.sub(r'MicroFont \d+\.\d+\.\d+ — printable ASCII',
              f'MicroFont {version} — printable ASCII', original)
text = re.sub(r"name:'MicroFont',version:'[^']+'", f"name:'MicroFont',version:'{version}'", text)
table = '// Full 66-bit canonical masks for Unicode U+0020–U+007E.\n'
table += f'// Bitmap SHA-256: {digest.hexdigest()}\n'
table += 'const GLYPHS=Object.freeze([\n'
table += ',\n'.join('  ' + ', '.join(f'0x{mask:x}n' for mask in masks[i:i+4])
                   for i in range(0, len(masks), 4))
table += '\n]);'
text, count = re.subn(r'(?:const z=\(bits,shift\)=>bits<<BigInt\(shift\);\n// Exactly[^\n]*\n|// Full 66-bit canonical masks[^\n]*\n// Bitmap SHA-256: [^\n]*\n)const GLYPHS=Object.freeze\(\[.*?\]\);',
                      lambda _: table, text, flags=re.S)
if count != 1:
    raise ValueError(f'Expected one glyph table, found {count}')
if args.check:
    if text != original:
        raise SystemExit('Stale MicroFont HTML data; run tools/update_microfont.py')
else:
    path.write_text(text)
print(f'MicroFont {version}: 95 canonical ASCII masks checked' if args.check else
      f'MicroFont {version}: updated 95 canonical ASCII masks')
