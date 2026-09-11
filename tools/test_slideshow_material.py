#!/usr/bin/env python3
"""Validate cell alignment and tangent-space data in the shared PBR maps."""
import unittest
from PIL import Image
from prepare_slideshow_material import OUTPUT, CELLS, TEXELS_PER_CELL, tile

class MaterialTests(unittest.TestCase):
    def test_bevel_normals_and_occlusion_match_each_cell(self):
        normal, ao = tile()
        n = lambda x,y: tuple(v/255*2-1 for v in normal.getpixel((x,y)))
        self.assertLess(n(0,1)[0], -.6)
        self.assertGreater(n(3,1)[0], .6)
        self.assertLess(n(1,0)[1], -.6)
        self.assertGreater(n(1,3)[1], .6)
        self.assertGreater(n(1,1)[2], .99)
        for y in range(4):
            for x in range(4):
                self.assertAlmostEqual(sum(v*v for v in n(x,y)), 1, delta=.015)
        self.assertLess(ao.getpixel((0,0))[0], ao.getpixel((0,1))[0])
        self.assertLess(ao.getpixel((0,1))[0], ao.getpixel((1,1))[0])

    def test_embedded_maps_are_periodic_for_all_512_cells(self):
        for name, expected in zip(('normal', 'occlusion'), tile()):
            with Image.open(OUTPUT/f'{name}.png') as image:
                self.assertEqual(image.size, (CELLS*TEXELS_PER_CELL,)*2)
                for cx,cy in ((0,0),(511,511),(200,300)):
                    x,y=cx*TEXELS_PER_CELL,cy*TEXELS_PER_CELL
                    self.assertEqual(image.crop((x,y,x+4,y+4)).tobytes(), expected.tobytes())
                # Two decoded static maps use 32 MiB; source PNGs stay compact.
                self.assertEqual(image.width*image.height*4, 16*1024*1024)
                self.assertLess((OUTPUT/f'{name}.png').stat().st_size, 100_000)

if __name__ == '__main__': unittest.main()
