#!/usr/bin/env python3
"""Build and seal a deterministic RestOS web release bundle."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile


FRONTEND_ROOT = Path(__file__).resolve().parents[1]
APP_NAME = "resty_form_alpha"
RELEASE_PLACEHOLDER = "__RESTOS_RELEASE_ID__"
RELEASE_ARTIFACT_SUFFIXES = {".html", ".js", ".wasm", ".css"}
FINGERPRINTED_ASSET = re.compile(r".+-dxh[0-9a-f]+\.(?:js|wasm|css)$")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def release_artifacts(bundle_dir: Path) -> list[Path]:
    artifacts = sorted(
        path
        for path in bundle_dir.rglob("*")
        if path.is_file()
        and path.name not in {"sw.js", "release-manifest.json"}
        and path.suffix in RELEASE_ARTIFACT_SUFFIXES
    )
    suffixes = {path.suffix for path in artifacts}
    required = {".html", ".js", ".wasm", ".css"}
    if not required.issubset(suffixes):
        missing = ", ".join(sorted(required - suffixes))
        raise RuntimeError(f"release bundle is incomplete: {missing}")
    if not any(path.name == "index.html" for path in artifacts):
        raise RuntimeError("release bundle has no index.html")
    return artifacts


def compute_release_id(bundle_dir: Path, artifacts: list[Path]) -> str:
    digest = hashlib.sha256()
    for path in artifacts:
        relative = path.relative_to(bundle_dir).as_posix().encode("utf-8")
        digest.update(relative)
        digest.update(b"\0")
        digest.update(bytes.fromhex(sha256(path)))
        digest.update(b"\0")
    return digest.hexdigest()


def atomic_write(path: Path, content: bytes) -> None:
    descriptor, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(content)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(temporary_name, 0o644)
        os.replace(temporary_name, path)
    finally:
        if os.path.exists(temporary_name):
            os.unlink(temporary_name)


def retain_previous_fingerprinted_assets(bundle_dir: Path, previous_dir: Path) -> list[str]:
    previous_assets = previous_dir / "assets"
    current_assets = bundle_dir / "assets"
    if not previous_assets.is_dir():
        raise RuntimeError("previous release has no assets directory")
    current_assets.mkdir(parents=True, exist_ok=True)
    retained: list[str] = []
    for source in sorted(previous_assets.iterdir()):
        if not source.is_file() or not FINGERPRINTED_ASSET.fullmatch(source.name):
            continue
        destination = current_assets / source.name
        if destination.exists():
            if sha256(destination) != sha256(source):
                raise RuntimeError("fingerprinted asset collision")
            continue
        shutil.copyfile(source, destination)
        os.chmod(destination, 0o644)
        retained.append(destination.relative_to(bundle_dir).as_posix())
    return retained


def seal_bundle(bundle_dir: Path, previous_release: Path | None = None) -> dict[str, object]:
    bundle_dir = bundle_dir.resolve()
    service_worker = bundle_dir / "sw.js"
    if not service_worker.is_file():
        raise RuntimeError("release bundle has no sw.js")
    template = service_worker.read_text(encoding="utf-8")
    if template.count(RELEASE_PLACEHOLDER) != 1:
        raise RuntimeError("service worker release placeholder must occur exactly once")

    artifacts = release_artifacts(bundle_dir)
    release_id = compute_release_id(bundle_dir, artifacts)
    rendered_worker = template.replace(RELEASE_PLACEHOLDER, release_id)
    if RELEASE_PLACEHOLDER in rendered_worker:
        raise RuntimeError("service worker release placeholder was not replaced")
    atomic_write(service_worker, rendered_worker.encode("utf-8"))

    retained = (
        retain_previous_fingerprinted_assets(bundle_dir, previous_release.resolve())
        if previous_release is not None
        else []
    )
    artifact_manifest = {
        path.relative_to(bundle_dir).as_posix(): sha256(path) for path in artifacts
    }
    manifest: dict[str, object] = {
        "schema_version": 1,
        "release_id": release_id,
        "cache_names": [
            f"restos-shell-{release_id}",
            f"restos-static-{release_id}",
        ],
        "artifacts": artifact_manifest,
        "service_worker_sha256": sha256(service_worker),
        "retained_legacy_assets": retained,
        "required_headers": {
            "/sw.js": {
                "Cache-Control": "no-cache, no-store, must-revalidate",
                "Service-Worker-Allowed": "/",
            },
            "/ and /index.html": {"Cache-Control": "no-cache, must-revalidate"},
            "/assets/*-dxh<hash>.(js|wasm|css)": {
                "Cache-Control": "public, max-age=31536000, immutable"
            },
        },
    }
    atomic_write(
        bundle_dir / "release-manifest.json",
        (json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode(
            "utf-8"
        ),
    )
    return manifest


def default_bundle_dir() -> Path:
    cargo_target = Path(os.environ.get("CARGO_TARGET_DIR", FRONTEND_ROOT / "target"))
    return cargo_target / "dx" / APP_NAME / "release" / "web" / "public"


def locate_bundle(preferred: Path) -> Path:
    if (preferred / "index.html").is_file() and (preferred / "sw.js").is_file():
        return preferred
    search_roots = {preferred.parent, FRONTEND_ROOT / "target"}
    candidates = sorted(
        path
        for root in search_roots
        if root.exists()
        for path in root.rglob("public")
        if (path / "index.html").is_file() and (path / "sw.js").is_file()
    )
    if len(candidates) != 1:
        raise RuntimeError(f"expected one Dioxus release bundle, found {len(candidates)}")
    return candidates[0]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bundle-dir", type=Path, default=default_bundle_dir())
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--dx-bin", default=os.environ.get("DX_BIN", "dx"))
    parser.add_argument("--retain-assets-from", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if not args.skip_build:
        subprocess.run(
            [args.dx_bin, "build", "--release", "--platform", "web"],
            cwd=FRONTEND_ROOT,
            check=True,
        )
    bundle_dir = locate_bundle(args.bundle_dir.resolve())
    manifest = seal_bundle(bundle_dir, args.retain_assets_from)
    print(f"bundle_dir={bundle_dir}")
    print(f"release_id={manifest['release_id']}")
    print(f"artifacts={len(manifest['artifacts'])}")
    print(f"retained_legacy_assets={len(manifest['retained_legacy_assets'])}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
