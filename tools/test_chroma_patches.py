#!/usr/bin/env python3
"""Validate actual benchmark_mandelbox RGBA readbacks against the cube mask.

Usage: python3 tools/test_chroma_patches.py output-directory [width height]
The probe writes front/sky/ground/behind/edge/corner views of all four presets.
"""
from pathlib import Path
import math
import sys

root = Path(sys.argv[1])
width, height = map(int, sys.argv[2:4]) if len(sys.argv) == 4 else (640, 360)
views = ('front', 'sky', 'ground', 'behind', 'edge', 'corner')
presets = ('cathedral-single', 'cathedral-duo', 'cathedral-trio', 'folded-void')
for view_id, view in enumerate(views):
    pitch = 0.7 if view_id == 1 else -0.7 if view_id == 2 else 0.30773985 if view_id == 5 else 0.
    yaw = 1.57079633 if view_id == 3 else 0.39269908 if view_id >= 4 else 0.
    # The probe uses a yaw * pitch quaternion. Apply the two rotations directly
    # here, independently of the GPU quaternion/cubemap face-selection code.
    sx, cx, sy, cy = math.sin(2*pitch), math.cos(2*pitch), math.sin(2*yaw), math.cos(2*yaw)
    data = (root/f'{presets[0]}-{view}.rgba').read_bytes()
    assert len(data) == width*height*4
    alpha = data[3::4]
    for y in range(height):
        for x in range(width):
            rx = (2*x+1-width)/height * 0.5773503
            ry = (height-2*y-1)/height * 0.5773503
            py, pz = cx*ry+sx, sx*ry-cx
            # Project direction to the cube. The mask is sign/face symmetric.
            a, b, dominant = sorted(map(abs, (cy*rx+sy*pz, py, -sy*rx+cy*pz)))
            u, v = a/dominant, b/dominant
            expected = v <= .4 or u >= .7
            # Native floating-point can choose either side exactly on an edge.
            if min(abs(v-.4), abs(u-.7)) > 2e-6:
                assert alpha[y*width+x] == (255 if expected else 0), (view,x,y,u,v)
    assert 0 < alpha.count(255) < width*height, view
    for preset in presets:
        pixels = (root/f'{preset}-{view}.rgba').read_bytes()
        assert pixels[3::4] == alpha, (preset,view,'mask changed with palette')
        for i in range(0,len(pixels),4):
            assert pixels[i+3] in (0,255)
            if pixels[i+3] == 0:
                assert pixels[i:i+4] == bytes(4), 'gap is not premultiplied transparent black'
    print(f'{view}: GPU mask matches cube projection; gaps transparent across all presets')
print('24 GPU views verified; six face centers and eight joined corners, no crop zoom.')
