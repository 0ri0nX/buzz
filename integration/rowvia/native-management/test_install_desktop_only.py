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


def _v2_fixture(root: Path) -> tuple[Path, Path, Path, native.Layout, Path]:
    """Extend the legacy fixture with a private selected root and six sidecars."""

    root.mkdir(mode=0o700)
    desktop, sidecars, pin_path, layout, backups = _fixture(root)
    selected = root / "native-root-r2"
    targets = tuple(selected / "usr/bin" / name for name in ("buzz-desktop", "buzz", "buzz-acp")) + (layout.targets[3],)
    for source, target in zip(layout.targets[:3], targets[:3], strict=True):
        _put(target, source.read_bytes())
    for directory in (selected, selected / "usr", selected / "usr/bin"):
        directory.chmod(0o700)
    layout.cli_link.unlink()
    layout.cli_link.symlink_to(targets[1])
    _put(desktop / "buzz-desktop", b"\x7fELF new xyz.block.buzz.app buzz:external-agent-enrollment:v1")
    provenance_path = desktop / "provenance.json"
    provenance = json.loads(provenance_path.read_text())
    provenance.update(owner_test_hook=True, binary_sha256=_hash(desktop / "buzz-desktop"))
    _put(provenance_path, json.dumps(provenance).encode(), 0o600)
    provenance_path = sidecars / "provenance.json"
    provenance = json.loads(provenance_path.read_text())
    provenance["source_commit"] = "b" * 40
    helpers = {}
    for name in native.ROOT_HELPERS:
        _put(sidecars / name, b"\x7fELF " + name.encode())
        _put(selected / "usr/bin" / name, (sidecars / name).read_bytes())
        helpers[name] = _hash(sidecars / name)
        provenance["artifacts"][name] = {"sha256": helpers[name], "host_ldd_r": "pass"}
    _put(provenance_path, json.dumps(provenance).encode(), 0o600)
    pin = json.loads(pin_path.read_text())
    pin.update(version=2, native_root=str(selected), baseline_desktop_source_commit="b" * 40,
               sidecar_source_commit="b" * 40, image_id=native.IMAGE_ID,
               desktop_sha256=_hash(desktop / "buzz-desktop"), helper_sha256=helpers,
               launcher_sha256=_hash(layout.launcher),
               wrapper_sha256=[_hash(path) for path in layout.wrappers],
               bridge_config_sha256=[_hash(path) for path in layout.bridge_configs])
    _put(pin_path, json.dumps(pin).encode(), 0o600)
    return desktop, sidecars, pin_path, layout, backups


def _v2_checks(root: Path) -> None:
    """Bind v2 success, launch, refusal, recovery and rollback to real seams."""

    desktop, sidecars, pin_path, layout, backups = _v2_fixture(root)
    pin = installer._pin(pin_path)
    resolved = installer._layout(pin, layout)
    target = resolved.targets[0]
    baseline = target.read_bytes()
    retained = (*resolved.targets[1:], *(resolved.native_root / "usr/bin" / name for name in native.ROOT_HELPERS),
                layout.launcher, *layout.wrappers, *layout.bridge_configs)
    originals = {path: path.read_bytes() for path in retained}
    actions = []

    def launch(actual: native.Layout, action: str) -> None:
        """Require the selected root for every synthetic lifecycle action."""

        assert actual.native_root == resolved.native_root
        assert actual.targets == resolved.targets
        actions.append(action)

    installer.preflight(desktop, sidecars, pin_path, layout)
    backup = installer.install(desktop, sidecars, pin_path, layout, launch, backups)
    assert target.read_bytes() == (desktop / "buzz-desktop").read_bytes()
    assert layout.targets[0].read_bytes() == baseline
    manifest = json.loads((backup / "manifest.json").read_text())
    assert manifest["version"] == 2 and manifest["pin"] == pin
    installer.rollback(backup, sidecars, pin_path, layout, launch, backups)
    assert target.read_bytes() == baseline and actions == ["stop", "start", "stop", "start"]
    assert {path: path.read_bytes() for path in retained} == originals

    for failure in ("stop", "start"):
        actions.clear()

        def fail(actual: native.Layout, action: str) -> None:
            """Fail one initial action while allowing automatic recovery."""

            launch(actual, action)
            if action == failure and len(actions) == (1 if failure == "stop" else 2):
                raise RuntimeError("synthetic lifecycle failure")

        try:
            installer.install(desktop, sidecars, pin_path, layout, fail, backups)
            raise AssertionError("lifecycle failure accepted")
        except RuntimeError:
            pass
        assert target.read_bytes() == baseline
        assert actions == (["stop", "stop", "start"] if failure == "stop" else ["stop", "start", "stop", "start"])
        assert {path: path.read_bytes() for path in retained} == originals

    # Invoke the actual launcher helper, replacing only process execution.
    run = native.subprocess.run
    calls = []
    native.subprocess.run = lambda arguments, **options: calls.append((arguments, options))
    try:
        native._launch(resolved, "start")
    finally:
        native.subprocess.run = run
    assert calls[0][0][2:] == ["start", "--owner-hook", "--native-root=" + str(resolved.native_root)]
    assert calls[0][1]["check"] is True

    def refused() -> None:
        """Prove pre-stop rejection through the installer entry point."""

        actions.clear()
        _reject(lambda: installer.install(desktop, sidecars, pin_path, layout, launch, backups))
        assert not actions and target.read_bytes() == baseline

    for path in retained:
        old = path.read_bytes()
        path.write_bytes(old + b"\n#drift")
        refused()
        path.write_bytes(old)
    for name in ("buzz", "buzz-acp", *native.ROOT_HELPERS):
        path = sidecars / name
        old = path.read_bytes()
        path.write_bytes(old + b"drift")
        refused()
        path.write_bytes(old)
    layout.cli_link.unlink()
    layout.cli_link.symlink_to(layout.targets[1])
    refused()
    layout.cli_link.unlink()
    layout.cli_link.symlink_to(resolved.targets[1])
    resolved.native_root.chmod(0o755)
    refused()
    resolved.native_root.chmod(0o700)
    provenance_path = desktop / "provenance.json"
    original = provenance_path.read_bytes()
    for field, value in (("owner_test_hook", False), ("build_mode", "compile-check"), ("deployable", False),
                         ("source_commit", "c" * 40), ("image_id", "sha256:" + "c" * 64), ("host_ldd_r", "fail")):
        data = json.loads(original)
        data[field] = value
        _put(provenance_path, json.dumps(data).encode(), 0o600)
        refused()
    provenance_path.write_bytes(original)
    provenance_path = sidecars / "provenance.json"
    original = provenance_path.read_bytes()
    for field, value in (("source_commit", "c" * 40), ("image_id", "sha256:" + "c" * 64)):
        data = json.loads(original)
        data[field] = value
        _put(provenance_path, json.dumps(data).encode(), 0o600)
        refused()
    for name in ("buzz", "buzz-acp", *native.ROOT_HELPERS):
        for field in ("sha256", "host_ldd_r"):
            data = json.loads(original)
            data["artifacts"][name][field] = "bad"
            _put(provenance_path, json.dumps(data).encode(), 0o600)
            refused()
    for name in native.ROOT_HELPERS:
        data = json.loads(original)
        del data["artifacts"][name]
        _put(provenance_path, json.dumps(data).encode(), 0o600)
        refused()
    for name in ("buzz", "buzz-acp"):
        data = json.loads(original)
        data["artifacts"][name]["patch_marker"] = "wrong-marker"
        _put(provenance_path, json.dumps(data).encode(), 0o600)
        refused()
    provenance_path.write_bytes(original)

    loader = native._loader
    def fail_loader(_path: Path) -> None:
        """Reject failed host resolution through the production preflight."""

        raise native.InstallError("synthetic loader failure")

    native._loader = fail_loader
    try:
        refused()
    finally:
        native._loader = loader

    # A correctly rehashed non-ELF candidate must still be rejected.
    candidate = desktop / "buzz-desktop"
    original_candidate = candidate.read_bytes()
    for payload in (original_candidate[4:], b"\x7fELF xyz.block.buzz.app", b"\x7fELF buzz:external-agent-enrollment:v1"):
        candidate.write_bytes(payload)
        changed_pin = dict(pin, desktop_sha256=_hash(candidate))
        _put(pin_path, json.dumps(changed_pin).encode(), 0o600)
        data = json.loads((desktop / "provenance.json").read_text())
        data["binary_sha256"] = changed_pin["desktop_sha256"]
        _put(desktop / "provenance.json", json.dumps(data).encode(), 0o600)
        refused()
    candidate.write_bytes(original_candidate)
    data["binary_sha256"] = pin["desktop_sha256"]
    _put(desktop / "provenance.json", json.dumps(data).encode(), 0o600)
    _put(pin_path, json.dumps(pin).encode(), 0o600)
    changed_pin = dict(pin, desktop_source_commit="c" * 40)
    _put(pin_path, json.dumps(changed_pin).encode(), 0o600)
    _reject(lambda: installer.rollback(backup, sidecars, pin_path, layout, launch, backups))
    _put(pin_path, json.dumps(pin).encode(), 0o600)

    actions.clear()

    def drift_after_stop(actual: native.Layout, action: str) -> None:
        """Recheck retained bindings after stopping and restore only Desktop."""

        launch(actual, action)
        if actions == ["stop"]:
            layout.wrappers[0].write_bytes(b"changed after stop")

    _reject(lambda: installer.install(desktop, sidecars, pin_path, layout, drift_after_stop, backups))
    assert target.read_bytes() == baseline and actions == ["stop", "stop"]
    assert layout.wrappers[0].read_bytes() == b"changed after stop"
    layout.wrappers[0].write_bytes(originals[layout.wrappers[0]])
    assert {path: path.read_bytes() for path in retained} == originals


def _launcher_parent_checks(root: Path) -> None:
    """Preserve the fixed default launcher contract with writable workspace parents."""

    desktop, sidecars, pin_path, layout, backups = _v2_fixture(root)
    workspace = root / "workspace"
    launcher = workspace / "launcher"
    payload = layout.launcher.read_bytes()
    _put(launcher, payload)
    workspace.chmod(0o775)
    layout = installer.replace(layout, launcher=launcher)
    actions = []
    launch = lambda _layout, action: actions.append(action)
    pin = installer._pin(pin_path)

    def refused() -> None:
        """Reject a launcher before executing lifecycle actions."""

        actions.clear()
        _reject(lambda: installer.install(desktop, sidecars, pin_path, layout, launch, backups))
        assert not actions

    refused()  # Custom launchers still require safe workspace ancestors.
    default, accepted_hash = native.LAUNCHER, native.LAUNCHER_SHA256
    native.LAUNCHER, native.LAUNCHER_SHA256 = launcher, _hash(launcher)
    try:
        installer.preflight(desktop, sidecars, pin_path, layout)
        backup = installer.install(desktop, sidecars, pin_path, layout, launch, backups)
        installer.rollback(backup, sidecars, pin_path, layout, launch, backups)
        assert actions == ["stop", "start", "stop", "start"]
        launcher.write_bytes(payload + b"changed")
        changed = dict(pin, launcher_sha256=_hash(launcher))
        _put(pin_path, json.dumps(changed).encode(), 0o600)
        refused()
        launcher.write_bytes(payload)
        _put(pin_path, json.dumps(pin).encode(), 0o600)
        for mode in (0o775, 0o700):
            launcher.chmod(mode)
            refused()
        launcher.chmod(0o755)
        launcher.unlink()
        launcher.symlink_to(root / "launcher")
        refused()
    finally:
        native.LAUNCHER, native.LAUNCHER_SHA256 = default, accepted_hash


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
        _v2_checks(root / "v2")
        _launcher_parent_checks(root / "launcher-parents")
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
