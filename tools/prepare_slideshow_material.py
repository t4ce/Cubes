#!/usr/bin/env python3
"""Generate shared linear-data bevel maps for the existing Picasso PBR shader."""
from pathlib import Path
from PIL import Image
import math

CELLS = 512
TEXELS_PER_CELL = 4
OUTPUT = Path(__file__).resolve().parents[1] / 'assets/slideshow'

def tile():
    normals = Image.new('RGB', (TEXELS_PER_CELL, TEXELS_PER_CELL))
    occlusion = Image.new('RGB', normals.size)
    for y in range(TEXELS_PER_CELL):
        for x in range(TEXELS_PER_CELL):
            nx = -1 if x == 0 else 1 if x == TEXELS_PER_CELL-1 else 0
            ny = -1 if y == 0 else 1 if y == TEXELS_PER_CELL-1 else 0
            length = math.sqrt(nx*nx + ny*ny + 1)
            # Tangent +V points down the image (handedness -1 on a +Z plane).
            normals.putpixel((x,y), tuple(round((v/length*.5+.5)*255) for v in (nx,ny,1)))
            ao = 100 if nx and ny else 165 if nx or ny else 255
            occlusion.putpixel((x,y), (ao,ao,ao))
    return normals, occlusion

def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    for name, small in zip(('normal', 'occlusion'), tile()):
        # Repeat identical data over the image UV range; the existing shader
        # uses the same UV set for all glTF material slots.
        row = Image.new('RGB', (CELLS*TEXELS_PER_CELL, TEXELS_PER_CELL))
        for x in range(CELLS): row.paste(small, (x*TEXELS_PER_CELL,0))
        atlas = Image.new('RGB', (CELLS*TEXELS_PER_CELL,)*2)
        for y in range(CELLS): atlas.paste(row, (0,y*TEXELS_PER_CELL))
        atlas.save(OUTPUT / f'{name}.png', optimize=True)

if __name__ == '__main__': main()
