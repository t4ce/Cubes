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
            for name in ("build.rs", "Cargo.lock"):
                shutil.copy2(ROOT / name, app / name)
            manifest = (ROOT / "Cargo.toml").read_text()
            # Blueprint rewrites path dependencies to the real workspace too.
            for dependency in ("TRUEOS-Blueprints/api", "TRUEOS-Picasso"):
                manifest = manifest.replace(f'"../{dependency}"', f'"{ROOT.parent / dependency}"')
            (app / "Cargo.toml").write_text(manifest)
            self.assertFalse((app.parent / "TRUEOS").exists())
            command = ["cargo", "check", "--offline", "--quiet", "--manifest-path",
                       str(app / "Cargo.toml"), "--target-dir", str(ROOT / "target/overlay-check")]
            result = subprocess.run(command, cwd=app, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            with (app / "Cube/cube.glb").open("ab") as asset:
                asset.write(b"stale")
            result = subprocess.run(command, cwd=app, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("cube reference differs from baked HS/DS", result.stderr)


if __name__ == "__main__":
    unittest.main()
