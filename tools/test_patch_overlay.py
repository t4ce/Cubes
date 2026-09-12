"""Build the app in isolation; no sibling TRUEOS checkout may be required."""
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class OverlayBuildTests(unittest.TestCase):
    def test_isolated_overlay_and_stale_asset_rejection(self):
        with tempfile.TemporaryDirectory(prefix="cubes-overlay-") as tmp:
            app = Path(tmp) / "source-overlay-app"
            app.mkdir()
            for directory in ("src", "Cube"):
                shutil.copytree(ROOT / directory, app / directory)
            (app / "tools").mkdir()
            shutil.copy2(ROOT / "tools/build_interface.rs", app / "tools/build_interface.rs")
            for name in ("build.rs", "Cargo.lock"):
                shutil.copy2(ROOT / name, app / name)
            manifest = (ROOT / "Cargo.toml").read_text()
            # Blueprint rewrites path dependencies to the real workspace too.
            for dependency in (
                "TRUEOS-Blueprints/api",
                "TRUEOS-Blueprints/crates/cubes-protocol",
                "TRUEOS-Picasso",
                "PotatoStamps",
            ):
                manifest = manifest.replace(f'"../{dependency}"', f'"{ROOT.parent / dependency}"')
            (app / "Cargo.toml").write_text(manifest)
            self.assertFalse((app.parent / "TRUEOS").exists())
            command = ["cargo", "check", "--offline", "--quiet", "--manifest-path",
                       str(app / "Cargo.toml"), "--target-dir", str(Path(tmp) / "target")]
            result = subprocess.run(command, cwd=app, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            palette = app / "Cube/subcubes-materials.json"
            original = palette.read_bytes()
            palette.write_bytes(original + b"\n")
            result = subprocess.run(command, cwd=app, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("material palette differs from baked HS/DS", result.stderr)
            palette.write_bytes(original)
            with (app / "Cube/cube.glb").open("ab") as asset:
                asset.write(b"stale")
            fresh_command = command[:-1] + [str(Path(tmp) / "stale-cube-target")]
            result = subprocess.run(fresh_command, cwd=app, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("run tools/prepare_image_cube.py", result.stderr)


if __name__ == "__main__":
    unittest.main()
