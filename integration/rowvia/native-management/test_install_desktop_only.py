#!/usr/bin/env python3
"""Exercise Desktop-only deployment guards with disposable synthetic files."""

from __future__ import annotations

from collections.abc import Callable
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile


SCRIPT = Path(__file__).with_name("install_desktop_only.py")
spec = importlib.util.spec_from_file_location("install_desktop_only", SCRIPT)
assert spec is not None and spec.loader is not None
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)
native = installer.native


def _put(path: Path, data: bytes, mode: int = 0o755) -> None:
    """Write a synthetic artifact with an explicit mode."""

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    path.chmod(mode)


def _hash(path: Path) -> str:
    """Hash one test artifact."""

    return hashlib.sha256(path.read_bytes()).hexdigest()


def _fixture(root: Path) -> tuple[Path, Path, Path, native.Layout, Path]:
    """Make a committed Desktop, old sidecars, and host integration paths."""

    desktop = root / "desktop"
    sidecars = root / "sidecars"
    pin_path = root / "pin" / "pin.json"
    backup_root = root / "desktop-backups"
    targets = tuple(root / "installed" / name for name in ("buzz-desktop", "buzz", "buzz-acp", "custom-acp"))
    launcher = root / "launcher"
    link = root / "bin" / "buzz"
    wrappers = (root / "wrapper-a", root / "wrapper-b")
    configs = (root / "config-a.toml", root / "config-b.toml")
    _put(targets[0], b"old xyz.block.buzz.app")
    _put(targets[1], b"old draft-connector")
    _put(targets[2], b"old buzz.trusted-turn-context/v1")
    _put(targets[3], b"custom buzz.trusted-turn-context/v1", 0o700)
    targets[0].parent.chmod(0o700)
    _put(desktop / "buzz-desktop", b"new xyz.block.buzz.app buzz:external-agent-enrollment:v1")
    _put(sidecars / "buzz", targets[1].read_bytes())
    _put(sidecars / "buzz-acp", targets[2].read_bytes())
    desktop.chmod(0o700)
    sidecars.chmod(0o700)
    _put(launcher, b"#!/bin/sh\n")
    for wrapper in wrappers:
        _put(wrapper, b"#!/bin/sh\n", 0o700)
    for config in configs:
        _put(config, f'[stock_buzz]\nexecutable = "{targets[3]}"\n'.encode(), 0o600)
    link.parent.mkdir(mode=0o700)
    link.symlink_to(targets[1])
    _put(
        desktop / "provenance.json",
        json.dumps({
            "source_commit": "a" * 40,
            "image_id": native.IMAGE_ID,
            "build_mode": "live",
            "deployable": True,
            "artifact_filename": "buzz-desktop",
            "tauri_identifier": "xyz.block.buzz.app",
            "product_name": "Buzz",
            "host_ldd_r": "pass",
            "binary_sha256": _hash(desktop / "buzz-desktop"),
        }).encode(),
        0o600,
    )
    _put(
        sidecars / "provenance.json",
        json.dumps({
            "source_commit": native.SOURCE_COMMIT,
            "image_id": native.IMAGE_ID,
            "artifacts": {
                "buzz": {"sha256": _hash(targets[1]), "patch_marker": "draft-connector", "host_ldd_r": "pass"},
                "buzz-acp": {"sha256": _hash(targets[2]), "patch_marker": "buzz.trusted-turn-context/v1", "host_ldd_r": "pass"},
            },
        }).encode(),
        0o600,
    )
    _put(
        pin_path,
        json.dumps({
            "version": 1,
            "desktop_source_commit": "a" * 40,
            "desktop_sha256": _hash(desktop / "buzz-desktop"),
            "baseline_desktop_sha256": _hash(targets[0]),
            "buzz_sha256": _hash(targets[1]),
            "buzz_acp_sha256": _hash(targets[2]),
            "custom_acp_sha256": _hash(targets[3]),
        }).encode(),
        0o600,
    )
    pin_path.parent.chmod(0o700)
    layout = native.Layout(targets, root / "unused-backups", launcher, link, wrappers, configs)
    return desktop, sidecars, pin_path, layout, backup_root


def _reject(action: Callable[[], object]) -> None:
    """Require a guard to reject an unsafe fixture."""

    try:
        action()
    except native.InstallError:
        return
    raise AssertionError("unsafe input was accepted")


def main() -> None:
    """Prove Desktop-only switch, recovery, rollback, and pre-stop refusals."""

    native._loader = lambda _path: None
    native.os.fsync = lambda _descriptor: None
    with tempfile.TemporaryDirectory(prefix="rowvia-desktop-only-test-", dir=Path.home()) as temporary:
        root = Path(temporary)
        desktop, sidecars, pin_path, layout, backup_root = _fixture(root)
        target = layout.targets[0]
        old_desktop = target.read_bytes()
        old_sidecars = tuple(path.read_bytes() for path in layout.targets[1:])
        installer.preflight(desktop, sidecars, pin_path, layout)

        actions: list[str] = []
        launch = lambda _layout, action: actions.append(action)
        backup = installer.install(desktop, sidecars, pin_path, layout, launch, backup_root)
        assert actions == ["stop", "start"]
        assert target.read_bytes() == (desktop / "buzz-desktop").read_bytes()
        assert tuple(path.read_bytes() for path in layout.targets[1:]) == old_sidecars
        assert set(path.name for path in backup.iterdir()) == {"buzz-desktop", "manifest.json"}

        inverse = installer.rollback(backup, sidecars, pin_path, layout, launch, backup_root)
        assert target.read_bytes() == old_desktop
        assert actions[-2:] == ["stop", "start"]
        assert inverse != backup

        actions.clear()
        def fail_start(_layout: native.Layout, action: str) -> None:
            """Fail the first new Desktop start after its file is replaced."""

            actions.append(action)
            if actions == ["stop", "start"]:
                raise RuntimeError("start failed")

        try:
            installer.install(desktop, sidecars, pin_path, layout, fail_start, backup_root)
            raise AssertionError("failed start was accepted")
        except RuntimeError:
            pass
        assert actions == ["stop", "start", "stop", "start"]
        assert target.read_bytes() == old_desktop

        actions.clear()
        def fail_stop(_layout: native.Layout, action: str) -> None:
            """Simulate a stop that terminated Desktop before reporting failure."""

            actions.append(action)
            if actions == ["stop"]:
                raise RuntimeError("stop failed")

        try:
            installer.install(desktop, sidecars, pin_path, layout, fail_stop, backup_root)
            raise AssertionError("failed stop was accepted")
        except RuntimeError:
            pass
        assert actions == ["stop", "stop", "start"]
        assert target.read_bytes() == old_desktop

        pin = json.loads(pin_path.read_text())
        pin["desktop_source_commit"] = "b" * 40
        _put(pin_path, json.dumps(pin).encode(), 0o600)
        _reject(lambda: installer.install(desktop, sidecars, pin_path, layout, launch, backup_root))
        assert actions == ["stop", "stop", "start"]
        pin["desktop_source_commit"] = "a" * 40
        _put(pin_path, json.dumps(pin).encode(), 0o600)

        provenance_path = desktop / "provenance.json"
        provenance = json.loads(provenance_path.read_text())
        provenance["build_mode"] = "compile-check"
        _put(provenance_path, json.dumps(provenance).encode(), 0o600)
        _reject(lambda: installer.install(desktop, sidecars, pin_path, layout, launch, backup_root))
        provenance["build_mode"] = "live"
        _put(provenance_path, json.dumps(provenance).encode(), 0o600)

        new_desktop = (desktop / "buzz-desktop").read_bytes()
        _put(desktop / "buzz-desktop", b"tampered xyz.block.buzz.app buzz:external-agent-enrollment:v1")
        _reject(lambda: installer.install(desktop, sidecars, pin_path, layout, launch, backup_root))
        _put(desktop / "buzz-desktop", new_desktop)

        _put(layout.targets[1], b"tampered draft-connector")
        _reject(lambda: installer.install(desktop, sidecars, pin_path, layout, launch, backup_root))
        assert target.read_bytes() == old_desktop
        _put(layout.targets[1], old_sidecars[0])

        actions.clear()
        def change_source_after_stop(_layout: native.Layout, action: str) -> None:
            """Detect a changed build artifact before publishing its bytes."""

            actions.append(action)
            if actions == ["stop"]:
                _put(desktop / "buzz-desktop", b"changed after preflight")

        _reject(lambda: installer.install(desktop, sidecars, pin_path, layout, change_source_after_stop, backup_root))
        assert actions == ["stop", "stop", "start"]
        assert target.read_bytes() == old_desktop
        _put(desktop / "buzz-desktop", new_desktop)

        _put(backup / "buzz-desktop", b"tampered backup")
        _reject(lambda: installer.rollback(backup, sidecars, pin_path, layout, launch, backup_root))
        assert target.read_bytes() == old_desktop
        assert tuple(path.read_bytes() for path in layout.targets[1:]) == old_sidecars
    print("Desktop-only installer mock checks passed")


if __name__ == "__main__":
    main()
