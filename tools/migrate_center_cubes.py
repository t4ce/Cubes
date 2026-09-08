"""Explicit one-time migration of old static center records, never a runtime fallback.

Writes a distinct output; preserves the original. Refuses lossy grid conversion.
"""
import argparse
from pathlib import Path
import struct

def convert(data):
    assert data[:6] == b'CUBE\x01\x00' and data[7] == 0 and data[11] == 8
    count = struct.unpack_from('<H', data, 8)[0]
    start = 16 + data[10]*4
    assert len(data) == start + count*8
    bias = struct.unpack_from('b', data, 6)[0]
    assert -99 <= bias <= -1
    out = bytearray(data)
    out[6], out[7], out[11] = -bias, 8, 4
    occupied = set()
    for offset in range(start,len(data),8):
        x,y,z,size,color,bone,part,flags = struct.unpack_from('<bbbBBBBB',data,offset)
        assert 1 <= size <= 4 and bone == 255 and flags == 0
        assert all((v-size)%2 == 0 for v in (x,y,z)), 'non-grid center'
        origin = [(v-size)//2 for v in (x,y,z)]
        assert all(-128 <= v <= 127 for v in origin)
        cells = {(origin[0]+a,origin[1]+b,origin[2]+c) for a in range(size) for b in range(size) for c in range(size)}
        assert not occupied.intersection(cells), 'overlapping source cubes'
        occupied.update(cells)
        struct.pack_into('<bbbBBBBB',out,offset,*origin,size,color,part,0,0)
    return out

if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source',type=Path)
    parser.add_argument('output',type=Path)
    args=parser.parse_args()
    result=convert(args.source.read_bytes())
    with args.output.open('xb') as output:
        output.write(result)
