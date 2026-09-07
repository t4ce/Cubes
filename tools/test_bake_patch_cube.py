"""CPU/source-contract tests. These do not prove shader execution or rendering."""
import hashlib
from pathlib import Path
import re
import struct
import tempfile
import unittest

from bake_patch_cube import ROOT, geometry, replace, write_sources


class PatchCubeTests(unittest.TestCase):
    def test_approved_reference(self):
        raw, triangles = geometry(ROOT / "Cube/cube.glb")
        self.assertEqual(hashlib.sha256(raw).hexdigest(),
                         "1166ee9b78e3e502466cc0737c4e0727d4eefbce3336b43284f6bd342f9ed6d2")
        self.assertEqual(len(triangles), 44)
        self.assertEqual(len({v[0] for t in triangles for v in t}), 24)
        self.assertEqual(len({v[1] for t in triangles for v in t}), 26)

    def test_seed_and_every_generated_corner(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            manifest = write_sources(ROOT / "Cube/cube.glb", out)
            _, triangles = geometry(ROOT / "Cube/cube.glb")
            self.assertEqual((out / "seed.f32le").read_bytes(), bytes(12))
            self.assertEqual((out / "patches.u32le").read_bytes(), bytes(44 * 4))
            hs = (out / "cube.tesc").read_text()
            matches = re.findall(r"case (\d+): p=uvec3\(([^)]+)\); n=uvec3\(([^)]+)\); break;", hs)
            self.assertEqual(len(matches), 132)
            source_vertices = [v for triangle in triangles for v in triangle]
            for i, (number, p, n) in enumerate(matches):
                self.assertEqual(int(number), i)
                for encoded, reference in zip((p, n), source_vertices[i]):
                    bits = [int(word[:-1], 16) for word in encoded.split(",")]
                    self.assertEqual(struct.pack("<3I", *bits), struct.pack("<3f", *reference))
            self.assertIn("gl_PrimitiveID * 3 + gl_InvocationID", hs)
            self.assertIn("layout(vertices=3) out", hs)
            self.assertFalse(manifest["runtime_integrated"])
            self.assertFalse(manifest["host_render_verified"])
            self.assertFalse(manifest["baremetal_verified"])

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
