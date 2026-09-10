"""Host/source tests for the sun angle; not a native shader execution proof."""
import hashlib
import math
from pathlib import Path
import re
import tempfile
import unittest
from unittest.mock import patch

import bake_patch_cube


# Shader generation needs geometry, not the native compiler or sibling repos.
# Keep this fixture separate from the approved-GLB tests in test_bake_patch_cube.
TRIANGLE = tuple((position, (0.0, 0.0, 1.0)) for position in
                 ((0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0)))
LOCAL_SUN = (0.35, 0.80, 0.45)  # toward the sun: right, up, backward


def normalize(vector):
    length = math.sqrt(sum(x*x for x in vector))
    return tuple(x/length for x in vector)


def rotate(vector, axis, angle):
    """Rodrigues rotation; the test axes are unit vectors."""
    c, s = math.cos(angle), math.sin(angle)
    dot = sum(a*b for a, b in zip(axis, vector))
    cross = (axis[1]*vector[2] - axis[2]*vector[1],
             axis[2]*vector[0] - axis[0]*vector[2],
             axis[0]*vector[1] - axis[1]*vector[0])
    return tuple(c*x + s*y + (1-c)*dot*z
                 for x, y, z in zip(vector, cross, axis))


class CameraSunTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            with patch.object(bake_patch_cube, "geometry",
                              return_value=(b"source-test-fixture", [TRIANGLE])):
                bake_patch_cube.write_sources(Path("unused.glb"), out)
            cls.ds = (out / "cube.tese").read_text()
            cls.ps = (out / "cube.frag").read_text()

    def emitted_sun(self, view_columns):
        # Evaluate the actual emitted basis indices and coefficients, so a
        # transposed matrix or incorrect sign in the GLSL fails these tests.
        axes = {}
        for name, expression in re.findall(r"vec3 (camera\w+) = vec3\(([^;]+)\);", self.ds):
            indices = re.findall(r"camera\.view\[(\d)\]\[(\d)\]", expression)
            self.assertEqual(len(indices), 3)
            axes[name] = tuple(view_columns[int(c)][int(r)] for c, r in indices)
        expression = re.search(r"surfaceLight = vec4\(([^;]+)\);", self.ds)[1]
        terms = re.findall(r"(camera\w+)\*([0-9.]+)", expression)
        self.assertEqual(len(terms), 3)
        return normalize(tuple(sum(axes[name][i]*float(weight) for name, weight in terms)
                               for i in range(3)))

    def test_sun_stays_above_behind_and_right_under_camera_rotation(self):
        for axis in ((1, 0, 0), (0, 1, 0), (0, 0, 1), normalize((1, 2, 3))):
            for angle in (0, math.pi/2, -math.pi/2, math.pi, 0.71):
                with self.subTest(axis=axis, angle=angle):
                    basis = [rotate(v, axis, angle) for v in
                             ((1, 0, 0), (0, 1, 0), (0, 0, 1))]
                    # Camera::retained stores the inverse rotation column-major.
                    columns = [tuple(basis[row][column] for row in range(3)) + (0,)
                               for column in range(3)] + [(0, 0, 0, 1)]
                    actual = self.emitted_sun(columns)
                    expected = rotate(normalize(LOCAL_SUN), axis, angle)
                    for a, b in zip(actual, expected):
                        self.assertAlmostEqual(a, b, places=12)
                    for camera_axis, expected_component in zip(basis, normalize(LOCAL_SUN)):
                        self.assertAlmostEqual(sum(a*b for a, b in zip(actual, camera_axis)),
                                               expected_component, places=12)

    def test_translation_does_not_turn_sun_into_a_point_light(self):
        columns = [(1, 0, 0, 0), (0, 1, 0, 0), (0, 0, 1, 0), (0, 0, 0, 1)]
        before = self.emitted_sun(columns)
        columns[3] = (1234, -5678, 9012, 1)
        self.assertEqual(before, self.emitted_sun(columns))
        expression = re.search(r"surfaceLight = vec4\(([^;]+)\);", self.ds)[1]
        self.assertNotIn("p.xyz", expression)
        self.assertNotIn("position_near", expression)

    def test_stage_interface_and_native_export_expectations_match(self):
        for source, direction in ((self.ds, "out"), (self.ps, "in")):
            self.assertIn(f"layout(location=3) {direction} vec4 surfaceLight;", source)
            self.assertEqual(len(re.findall(rf"layout\(location=\d\) {direction} vec4 surface", source)), 4)
        self.assertIn("vec3 light = normalize(surfaceLight.xyz);", self.ps)
        self.assertNotIn("readonly buffer", self.ps)
        self.assertNotIn("sampler", self.ps)
        exporter = (bake_patch_cube.ROOT / "tools/export_patch_driver.py").read_text()
        self.assertIn("num_varying_inputs=4", exporter)
        self.assertIn("num_varying_inputs: 4", exporter)
        self.assertIn("urb_read_length=15, vue_slots=6", exporter)

    def test_pixel_lighting_math_is_unchanged_except_for_direction(self):
        # Canonical PS body from Cubes 155206c, with comments/whitespace removed.
        # Replace only the new direction input before checking that every
        # ambient, diffuse, material, specular and alpha expression is intact.
        body = self.ps[self.ps.index("void main() {"):]
        body = body.replace("normalize(surfaceLight.xyz)", "normalize(vec3(0.35,0.80,0.45))")
        body = re.sub(r"//[^\n]*", "", body)
        body = re.sub(r"\s+", "", body)
        self.assertEqual(hashlib.sha256(body.encode()).hexdigest(),
                         "473312acd2fca035eb1118d75628a0b7d49cfa9ae6495c7c1e45ed7e0c261401")


if __name__ == "__main__":
    unittest.main()
