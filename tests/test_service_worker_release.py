from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "build_release.py"
SPEC = importlib.util.spec_from_file_location("restos_release_builder", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
BUILDER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUILDER)


class ReleaseBuilderTests(unittest.TestCase):
    def make_bundle(self, root: Path, marker: str = "a") -> Path:
        bundle = root / "public"
        assets = bundle / "assets"
        assets.mkdir(parents=True)
        (bundle / "index.html").write_text(
            f'<script src="/assets/app-dxh{marker * 8}.js"></script>', encoding="utf-8"
        )
        (assets / f"app-dxh{marker * 8}.js").write_text(
            f"console.log('{marker}')", encoding="utf-8"
        )
        (assets / f"app_bg-dxh{marker * 8}.wasm").write_bytes(
            b"\x00asm" + marker.encode("ascii")
        )
        (assets / f"main-dxh{marker * 8}.css").write_text(
            f":root{{--release:{marker}}}", encoding="utf-8"
        )
        (bundle / "sw.js").write_text(
            f'const APP_VERSION = "{BUILDER.RELEASE_PLACEHOLDER}";\n', encoding="utf-8"
        )
        return bundle

    def test_release_id_is_deterministic_and_placeholder_is_removed(self):
        with tempfile.TemporaryDirectory() as directory:
            first = self.make_bundle(Path(directory) / "first")
            second = self.make_bundle(Path(directory) / "second")
            first_manifest = BUILDER.seal_bundle(first)
            second_manifest = BUILDER.seal_bundle(second)
            self.assertEqual(first_manifest["release_id"], second_manifest["release_id"])
            self.assertEqual(len(first_manifest["release_id"]), 64)
            self.assertNotIn(
                BUILDER.RELEASE_PLACEHOLDER,
                (first / "sw.js").read_text(encoding="utf-8"),
            )
            saved = json.loads((first / "release-manifest.json").read_text(encoding="utf-8"))
            self.assertEqual(saved["release_id"], first_manifest["release_id"])
            self.assertEqual(len(saved["artifacts"]), 4)

    def test_changed_bundle_changes_release_id(self):
        with tempfile.TemporaryDirectory() as directory:
            first = self.make_bundle(Path(directory) / "first", "a")
            second = self.make_bundle(Path(directory) / "second", "b")
            self.assertNotEqual(
                BUILDER.seal_bundle(first)["release_id"],
                BUILDER.seal_bundle(second)["release_id"],
            )

    def test_missing_or_duplicate_placeholder_fails_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            bundle = self.make_bundle(Path(directory))
            (bundle / "sw.js").write_text("const APP_VERSION='manual';", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "exactly once"):
                BUILDER.seal_bundle(bundle)

    def test_previous_fingerprinted_assets_are_retained_without_collision(self):
        with tempfile.TemporaryDirectory() as directory:
            previous = self.make_bundle(Path(directory) / "previous", "a")
            current = self.make_bundle(Path(directory) / "current", "b")
            manifest = BUILDER.seal_bundle(current, previous)
            retained = manifest["retained_legacy_assets"]
            self.assertEqual(len(retained), 3)
            for relative in retained:
                self.assertTrue((current / relative).is_file())


if __name__ == "__main__":
    unittest.main()
