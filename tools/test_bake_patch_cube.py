"""CPU/source-contract tests. These do not prove shader execution or rendering."""
import hashlib
from pathlib import Path
import re
import struct
import tempfile
import unittest

from bake_patch_cube import ROOT, geometry, replace, write_sources


class PatchCubeTests(unittest.TestCase):
    def test_picking_planes_match_reference_bevel(self):
        from itertools import product
        _, triangles = geometry(ROOT / "Cube/cube.glb")
        points = [v[0] for t in triangles for v in t]
        for normal in product((-1, 0, 1), repeat=3):
            count = sum(abs(x) for x in normal)
            if not count:
                continue
            bound = (0, 1.0, 1.8, 2.6)[count]
            support = max(sum(a*b for a,b in zip(normal,p)) for p in points)
            self.assertAlmostEqual(support, bound, places=5)
    def test_approved_reference(self):
        raw, triangles = geometry(ROOT / "Cube/cube.glb")
        self.assertEqual(hashlib.sha256(raw).hexdigest(),
                         "1166ee9b78e3e502466cc0737c4e0727d4eefbce3336b43284f6bd342f9ed6d2")
        self.assertEqual(len(triangles), 44)
        self.assertEqual(len({v[0] for t in triangles for v in t}), 24)
        self.assertEqual(len({v[1] for t in triangles for v in t}), 26)

    def test_seed_and_canonical_geometry_mapping(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            manifest = write_sources(ROOT / "Cube/cube.glb", out)
            _, triangles = geometry(ROOT / "Cube/cube.glb")
            self.assertEqual((out / "seed.f32le").read_bytes(), bytes(12))
            self.assertEqual((out / "patches.u32le").read_bytes(), bytes(44 * 4))
            hs = (out / "cube.tesc").read_text()
            position_cases = re.findall(r"case (\d+): p=uvec3\(([^)]+)\); break;", hs)
            corner_position_cases = re.findall(r"case (\d+): return (\d+);", hs)
            normal_cases = re.findall(r"case (\d+): n=uvec3\(([^)]+)\); break;", hs)
            self.assertEqual(len(position_cases), 24)
            self.assertEqual(len(corner_position_cases), 132)
            self.assertEqual(len(normal_cases), 132)
            source_vertices = [v for triangle in triangles for v in triangle]
            canonical_positions = [None] * len(position_cases)
            for i, (number, p) in enumerate(position_cases):
                self.assertEqual(int(number), i)
                bits = [int(word[:-1], 16) for word in p.split(",")]
                canonical_positions[i] = struct.unpack("<3f", struct.pack("<3I", *bits))
            for i, (number, position_id) in enumerate(corner_position_cases):
                self.assertEqual(int(number), i)
                self.assertEqual(canonical_positions[int(position_id)], source_vertices[i][0])
            for i, (number, n) in enumerate(normal_cases):
                self.assertEqual(int(number), i)
                bits = [int(word[:-1], 16) for word in n.split(",")]
                self.assertEqual(struct.pack("<3I", *bits), struct.pack("<3f", *source_vertices[i][1]))
            self.assertEqual(manifest["canonical_position_count"], 24)
            self.assertEqual(manifest["triangle_corner_count"], 132)
            self.assertIn("triangleCornerToPositionID(gl_PrimitiveID, gl_InvocationID)", hs)
            self.assertIn("vec3 p = cubePosition(positionID);", hs)
            self.assertIn("vec3 n = triangleCornerNormal(gl_PrimitiveID, gl_InvocationID);", hs)
            self.assertIn("layout(vertices=3) out", hs)
            self.assertIn("gl_PrimitiveID < 2", hs)
            self.assertIn("vec4(offset, scale * 1000.0, -instanceID[0])", hs)
            vs = (out / "cube.vert").read_text()
            ds = (out / "cube.tese").read_text()
            ps = (out / "cube.frag").read_text()
            self.assertIn("uint id = compacted.ids[gl_InstanceIndex]", vs)
            self.assertIn("uint base = id * 13u", vs)
            self.assertIn("clip.w > 0.0", vs)
            self.assertEqual(ds.count("alpha=0.35"), 1)
            self.assertIn("bool transparentPass", ds)
            self.assertIn("sticker != int((flags >> 10u) & 7u)", ds)
            self.assertIn("(flags & 8192u) != 0u && id < 600u", ds)
            self.assertIn("(flags & 16384u) != 0u", ds)
            self.assertIn("normalize(model[3].xyz)", ds)
            self.assertIn("uint wall = id / 100u", ds)
            self.assertEqual(ds.count("wall =="), 6)
            self.assertIn("float alpha = 1.0", ds)
            self.assertIn("layout(location=0) out vec4 shadedColor", ds)
            self.assertIn("color = shadedColor", ps)
            self.assertFalse(manifest["runtime_integrated"])
            self.assertFalse(manifest["host_render_verified"])
            self.assertFalse(manifest["baremetal_verified"])

    def test_exactly_54_square_stickers_excluding_bevels(self):
        _, triangles = geometry(ROOT / "Cube/cube.glb")
        per_face = {}
        for triangle in triangles:
            n = triangle[0][1]
            self.assertTrue(all(v[1] == n for v in triangle))
            if sum(abs(x) > 0.9999 for x in n) == 1:
                per_face[n] = per_face.get(n, 0) + 1
        self.assertEqual(len(per_face), 6)
        self.assertEqual(list(per_face.values()), [2] * 6)
        self.assertEqual(sum(per_face.values()) * 9, 108) # 54 faces, two triangles each

    def test_capture_template_drift_fails_closed(self):
        for text in ("missing", "twice twice"):
            with self.assertRaises(ValueError):
                replace(text, "twice", "replacement")

    def test_truncated_glb(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "bad.glb"
            path.write_bytes(b"glTF")
            with self.assertRaisesRegex(ValueError, "truncated GLB header"):
                geometry(path)


if __name__ == "__main__":
    unittest.main()
