#!/usr/bin/env python3
"""Resource-light mock checks for the native installer; never touch live paths."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import resource
import tempfile
from pathlib import Path


SCRIPT = Path(__file__).with_name("install_native.py")
spec = importlib.util.spec_from_file_location("install_native", SCRIPT)
assert spec is not None and spec.loader is not None
installer = importlib.util.module_from_spec(spec)
import sys

sys.modules[spec.name] = installer
spec.loader.exec_module(installer)


def _put(path: Path, content: bytes, mode: int = 0o755) -> None:
    """Create one synthetic executable or provenance file."""

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(content)
    path.chmod(mode)


def _hash(path: Path) -> str:
    """Hash a synthetic artifact."""

    return hashlib.sha256(path.read_bytes()).hexdigest()


def _fixture(root: Path) -> tuple[Path, Path, object]:
    """Make isolated outputs and four distinct pre-install targets."""

    desktop = root / "desktop"
    sidecars = root / "sidecars"
    targets = tuple(root / "installed" / name for name in ("buzz-desktop", "buzz", "buzz-acp", "custom-acp"))
    custom = targets[3]
    wrappers = (root / "wrapper-a.sh", root / "wrapper-b.sh")
    configs = (root / "bridge-a.toml", root / "bridge-b.toml")
    launcher = root / "launcher"
    link = root / "link-buzz"
    for index, target in enumerate(targets):
        _put(target, f"old-{index}".encode(), 0o700 if index == 3 else 0o755)
    targets[0].parent.chmod(0o700)
    for wrapper in wrappers:
        _put(wrapper, b"#!/bin/sh\n", 0o700)
    for config in configs:
        _put(config, f'[stock_buzz]\nexecutable = "{custom}"\n'.encode(), 0o600)
    _put(launcher, b"#!/bin/sh\n")
    link.symlink_to(targets[1])
    sources = (desktop / "buzz-desktop", sidecars / "buzz", sidecars / "buzz-acp")
    for source, marker in zip(sources, ("xyz.block.buzz.app", "draft-connector", "buzz.trusted-turn-context/v1"), strict=True):
        _put(source, f"new {marker}".encode())
    desktop_meta = {
        "source_commit": installer.SOURCE_COMMIT,
        "image_id": installer.IMAGE_ID,
        "build_mode": "live",
        "deployable": True,
        "artifact_filename": "buzz-desktop",
        "tauri_identifier": "xyz.block.buzz.app",
        "product_name": "Buzz",
        "host_ldd_r": "pass",
        "binary_sha256": _hash(sources[0]),
    }
    sidecar_meta = {
        "source_commit": installer.SOURCE_COMMIT,
        "image_id": installer.IMAGE_ID,
        "artifacts": {
            "buzz": {"sha256": _hash(sources[1]), "patch_marker": "draft-connector", "host_ldd_r": "pass"},
            "buzz-acp": {"sha256": _hash(sources[2]), "patch_marker": "buzz.trusted-turn-context/v1", "host_ldd_r": "pass"},
        },
    }
    _put(desktop / "provenance.json", json.dumps(desktop_meta).encode(), 0o600)
    _put(sidecars / "provenance.json", json.dumps(sidecar_meta).encode(), 0o600)
    desktop.chmod(0o700)
    sidecars.chmod(0o700)
    layout = installer.Layout(targets, root / "backups", launcher, link, wrappers, configs)
    return desktop, sidecars, layout


def main() -> None:
    """Check refusal, successful switch, explicit restore, and failed-start recovery."""

    resource.setrlimit(resource.RLIMIT_AS, (256 * 1024 * 1024, 256 * 1024 * 1024))
    resource.setrlimit(resource.RLIMIT_CPU, (10, 10))
    resource.setrlimit(resource.RLIMIT_FSIZE, (16 * 1024 * 1024, 16 * 1024 * 1024))
    installer._loader = lambda path: None
    fsync_calls: list[int] = []

    def record_fsync(descriptor: int) -> None:
        """Record durability calls without exercising the test filesystem."""

        fsync_calls.append(descriptor)

    installer.os.fsync = record_fsync
    with tempfile.TemporaryDirectory(prefix="rowvia-installer-test-", dir=Path.home()) as directory:
        root = Path(directory)
        desktop, sidecars, layout = _fixture(root)
        original = tuple(target.read_bytes() for target in layout.targets)
        installer.preflight(desktop, sidecars, layout)
        meta_path = desktop / "provenance.json"
        meta = json.loads(meta_path.read_text())
        meta["build_mode"] = "compile-check"
        _put(meta_path, json.dumps(meta).encode(), 0o600)
        try:
            installer.preflight(desktop, sidecars, layout)
            raise AssertionError("compile-check artifact was accepted")
        except installer.InstallError:
            pass
        meta["build_mode"] = "live"
        _put(meta_path, json.dumps(meta).encode(), 0o600)
        _put(sidecars / "buzz", b"tampered draft-connector")
        try:
            installer.preflight(desktop, sidecars, layout)
            raise AssertionError("artifact hash mismatch was accepted")
        except installer.InstallError:
            pass
        _put(sidecars / "buzz", b"new draft-connector")
        actions: list[str] = []
        backup = installer.install(desktop, sidecars, layout, lambda _layout, action: actions.append(action))
        assert len(fsync_calls) >= 7
        assert actions == ["stop", "start"]
        assert all(target.read_bytes() != old for target, old in zip(layout.targets, original, strict=True))
        assert (backup / "manifest.json").exists()
        installer.rollback(backup, layout, lambda _layout, action: actions.append(action))
        assert tuple(target.read_bytes() for target in layout.targets) == original
        assert actions[-2:] == ["stop", "start"]
        (backup / "2").write_bytes(b"bad backup")
        before_actions = list(actions)
        try:
            installer.rollback(backup, layout, lambda _layout, action: actions.append(action))
            raise AssertionError("damaged backup was accepted")
        except installer.InstallError:
            pass
        assert actions == before_actions

        def fail_once(_layout: object, action: str) -> None:
            """Simulate a failed first restart after all replacements."""

            actions.append(action)
            if action == "start" and actions.count("start") == 3:
                raise RuntimeError("start failed")

        try:
            installer.install(desktop, sidecars, layout, fail_once)
            raise AssertionError("failed restart was accepted")
        except RuntimeError:
            pass
        assert tuple(target.read_bytes() for target in layout.targets) == original
        assert actions[-4:] == ["stop", "start", "stop", "start"]

        stop_actions: list[str] = []

        def fail_stop_once(_layout: object, action: str) -> None:
            """Simulate the launcher stopping its unit before returning failure."""

            stop_actions.append(action)
            if stop_actions == ["stop"]:
                raise RuntimeError("stop reported failure after stopping")

        try:
            installer.install(desktop, sidecars, layout, fail_stop_once)
            raise AssertionError("failed stop was accepted")
        except RuntimeError:
            pass
        assert stop_actions == ["stop", "stop", "start"]
        assert tuple(target.read_bytes() for target in layout.targets) == original
        assert os.readlink(layout.cli_link) == str(layout.targets[1])

        symlink = root / "linked-backups"
        symlink.symlink_to(layout.backup_root, target_is_directory=True)
        unsafe_layout = installer.Layout(layout.targets, symlink, layout.launcher, layout.cli_link, layout.wrappers, layout.bridge_configs)
        try:
            installer.install(desktop, sidecars, unsafe_layout, fail_stop_once)
            raise AssertionError("symlink backup root was accepted")
        except installer.InstallError:
            pass
    print("native installer mock checks passed")


if __name__ == "__main__":
    main()
