#!/usr/bin/env python3
"""Switch one pinned Buzz Desktop ELF while retaining the installed sidecars."""

from __future__ import annotations

import argparse
from collections.abc import Callable
from dataclasses import replace
import fcntl
import json
import os
from pathlib import Path
import re
import signal
import stat
import subprocess
import sys
import tempfile
from types import FrameType

import install_native as native


HEX40 = re.compile(r"[0-9a-f]{40}\Z")
HEX64 = re.compile(r"[0-9a-f]{64}\Z")
BACKUPS = native.HOME / ".local/state/rowvia-buzz-native-install/desktop-backups"
PIN_FIELDS = {
    "version",
    "desktop_source_commit",
    "desktop_sha256",
    "baseline_desktop_sha256",
    "buzz_sha256",
    "buzz_acp_sha256",
    "custom_acp_sha256",
}
V2_FIELDS = PIN_FIELDS | {
    "native_root", "baseline_desktop_source_commit", "sidecar_source_commit", "image_id",
    "helper_sha256", "launcher_sha256", "wrapper_sha256", "bridge_config_sha256",
}


def _pin(path: Path) -> dict:
    """Load an operator-reviewed, private exact-artifact pin."""

    native._safe_directory(path.parent, private=True)
    info = native._regular_owned(path, executable=False)
    native._require(stat.S_IMODE(info.st_mode) == 0o600, "pin must be mode 0600")
    data = json.loads(path.read_text(encoding="utf-8"))
    native._require(isinstance(data, dict) and type(data.get("version")) is int and data["version"] in (1, 2), "unsupported pin version")
    native._require(set(data) == (PIN_FIELDS if data["version"] == 1 else V2_FIELDS), "invalid pin fields")
    native._require(
        isinstance(data["desktop_source_commit"], str)
        and HEX40.fullmatch(data["desktop_source_commit"])
        and (data["version"] == 2 or data["desktop_source_commit"] != native.SOURCE_COMMIT),
        "invalid new Desktop commit pin",
    )
    for field in PIN_FIELDS - {"version", "desktop_source_commit"}:
        native._require(isinstance(data[field], str) and HEX64.fullmatch(data[field]), f"invalid {field}")
    if data["version"] == 2:
        for field in ("baseline_desktop_source_commit", "sidecar_source_commit"):
            native._require(isinstance(data[field], str) and HEX40.fullmatch(data[field]), f"invalid {field}")
        native._require(data["desktop_source_commit"] != data["baseline_desktop_source_commit"], "Desktop commit must change")
        native._require(data["image_id"] == native.IMAGE_ID, "build image pin mismatch")
        root = data["native_root"]
        native._require(isinstance(root, str) and str(Path(root)) == root and Path(root).is_absolute(), "invalid native root")
        native._safe_directory(Path(root), private=True)
        helpers = data["helper_sha256"]
        native._require(isinstance(helpers, dict) and set(helpers) == set(native.ROOT_HELPERS), "invalid helper pins")
        hashes = [data["launcher_sha256"], *helpers.values()]
        for field in ("wrapper_sha256", "bridge_config_sha256"):
            native._require(isinstance(data[field], list) and len(data[field]) == 2, f"invalid {field}")
            hashes.extend(data[field])
        native._require(all(isinstance(value, str) and HEX64.fullmatch(value) for value in hashes), "invalid binding hash")
    return data


def _layout(pin: dict, layout: native.Layout) -> native.Layout:
    """Resolve v2 replacement paths exclusively from the reviewed root."""

    if pin["version"] == 1:
        return layout
    root = Path(pin["native_root"])
    native._safe_directory(root, private=True)
    native._safe_directory(root / "usr/bin", private=True)
    native._require(layout.native_root is None or layout.native_root == root, "native root binding changed")
    targets = tuple(root / "usr/bin" / name for name in ("buzz-desktop", "buzz", "buzz-acp")) + (layout.targets[3],)
    return replace(layout, targets=targets, native_root=root, pin=pin)


def _bindings(pin: dict, layout: native.Layout) -> None:
    """Verify exact retained launcher, CLI selection, wrappers and configs."""

    native._safe_directory(layout.launcher.parent)
    native._require(stat.S_IMODE(native._regular_owned(layout.launcher).st_mode) == 0o755, "launcher mode mismatch")
    native._require(native._sha256(layout.launcher) == pin["launcher_sha256"], "launcher hash changed")
    if layout.launcher == native.LAUNCHER:
        native._require(pin["launcher_sha256"] == native.LAUNCHER_SHA256, "launcher acceptance pin mismatch")
    native._safe_directory(layout.cli_link.parent)
    native._require(layout.cli_link.is_symlink() and os.readlink(layout.cli_link) == str(layout.targets[1]), "CLI link changed")
    for paths, field, mode in ((layout.wrappers, "wrapper_sha256", 0o700), (layout.bridge_configs, "bridge_config_sha256", 0o600)):
        for path, digest in zip(paths, pin[field], strict=True):
            native._safe_directory(path.parent)
            native._require(stat.S_IMODE(native._regular_owned(path, executable=mode == 0o700).st_mode) == mode, "binding mode changed")
            native._require(native._sha256(path) == digest, f"{field} changed")
    for config in layout.bridge_configs:
        native._require(native.tomllib.loads(config.read_text(encoding="utf-8")).get("stock_buzz", {}).get("executable") == str(layout.targets[3]), "bridge config custom ACP path changed")


def _retained(pin: dict, sidecars: Path, layout: native.Layout, *, installing: bool) -> None:
    """Recheck root, retained seams and permitted Desktop before switching."""

    layout = _layout(pin, layout)
    _sidecars_unchanged(pin, sidecars, layout)
    expected = {pin["baseline_desktop_sha256"]}
    if not installing:
        expected.add(pin["desktop_sha256"])
    native._require(native._sha256(layout.targets[0]) in expected, "installed Desktop changed before switch")


def _sidecars_unchanged(pin: dict, sidecars: Path, layout: native.Layout) -> None:
    """Require retained sidecar provenance, source hashes and installed hashes."""

    native._safe_directory(sidecars, private=True)
    provenance = native._json(sidecars / "provenance.json")
    native._require(
        provenance.get("source_commit") == pin.get("sidecar_source_commit", native.SOURCE_COMMIT)
        and provenance.get("image_id") == pin.get("image_id", native.IMAGE_ID),
        "sidecar source or image provenance mismatch",
    )
    artifacts = provenance.get("artifacts")
    v2 = pin["version"] == 2
    names = {"buzz", "buzz-acp"} | (set(native.ROOT_HELPERS) if v2 else set())
    native._require(isinstance(artifacts, dict) and set(artifacts) == names, "invalid sidecar provenance")
    checks = (
        ("buzz", "buzz_sha256", layout.targets[1], "draft-connector"),
        ("buzz-acp", "buzz_acp_sha256", layout.targets[2], "buzz.trusted-turn-context/v1"),
    )
    checks += tuple((name, name, layout.native_root / "usr/bin" / name, None) for name in native.ROOT_HELPERS) if v2 else ()
    for name, field, target, marker in checks:
        digest = pin["helper_sha256"][field] if name in native.ROOT_HELPERS else pin[field]
        record = artifacts[name]
        native._require(isinstance(record, dict), f"invalid {name} provenance")
        native._require(
            record.get("sha256") == digest
            and (marker is None or record.get("patch_marker") == marker)
            and record.get("host_ldd_r") == "pass",
            f"{name} provenance mismatch",
        )
        source = sidecars / name
        native._regular_owned(source)
        native._regular_owned(target)
        native._safe_directory(target.parent)
        native._require(native._sha256(source) == digest, f"{name} source hash mismatch")
        native._require(native._sha256(target) == digest, f"{name} installed hash changed")
        if v2:
            native._require(stat.S_IMODE(source.stat().st_mode) == 0o755 and stat.S_IMODE(target.stat().st_mode) == 0o755, "sidecar mode changed")
            if marker:
                native._require(native._contains(source, marker), "sidecar patch marker missing")
            native._loader(source)
            native._loader(target)
    custom = layout.targets[3]
    native._regular_owned(custom)
    native._safe_directory(custom.parent)
    native._require(native._sha256(custom) == pin["custom_acp_sha256"], "custom ACP hash changed")
    if v2:
        native._require(stat.S_IMODE(custom.stat().st_mode) == 0o700, "custom ACP mode changed")
        _bindings(pin, layout)


def preflight(
    desktop: Path, sidecars: Path, pin_path: Path, layout: native.Layout = native.Layout()
) -> dict:
    """Validate committed-source provenance, live ELF, and unchanged host seams."""

    native._require(os.getuid() == 1000, "installer requires host UID 1000")
    pin = _pin(pin_path)
    layout = _layout(pin, layout)
    native._safe_directory(desktop, private=True)
    provenance = native._json(desktop / "provenance.json")
    native._require(provenance.get("source_commit") == pin["desktop_source_commit"], "Desktop source commit mismatch")
    native._require(provenance.get("image_id") == native.IMAGE_ID, "Desktop build image mismatch")
    native._require(
        provenance.get("build_mode") == "live"
        and provenance.get("deployable") is True
        and provenance.get("artifact_filename") == "buzz-desktop"
        and provenance.get("tauri_identifier") == "xyz.block.buzz.app"
        and provenance.get("product_name") == "Buzz"
        and provenance.get("host_ldd_r") == "pass",
        "Desktop provenance is not a canonical live build",
    )
    native._require(provenance.get("binary_sha256") == pin["desktop_sha256"], "Desktop provenance hash mismatch")
    source = desktop / "buzz-desktop"
    native._require(stat.S_IMODE(native._regular_owned(source).st_mode) == 0o755, "Desktop source mode mismatch")
    native._require(native._sha256(source) == pin["desktop_sha256"], "Desktop ELF hash mismatch")
    if pin["version"] == 2:
        native._require(provenance.get("owner_test_hook") is True, "Desktop owner hook missing")
        with source.open("rb") as payload:
            native._require(payload.read(4) == b"\x7fELF", "Desktop is not ELF")
    native._require(native._contains(source, "xyz.block.buzz.app"), "Desktop app marker missing")
    native._require(native._contains(source, "buzz:external-agent-enrollment:v1"), "enrollment marker missing")
    native._loader(source)
    target = layout.targets[0]
    native._require(stat.S_IMODE(native._regular_owned(target).st_mode) == 0o755, "installed Desktop mode mismatch")
    native._safe_directory(target.parent)
    native._require(native._sha256(target) == pin["baseline_desktop_sha256"], "installed Desktop differs from pinned baseline")
    native._require(stat.S_IMODE(native._regular_owned(layout.launcher).st_mode) == 0o755, "launcher mode mismatch")
    if layout.launcher == native.LAUNCHER:
        native._require(native._sha256(layout.launcher) == native.LAUNCHER_SHA256, "launcher hash mismatch")
    native._safe_directory(layout.cli_link.parent)
    native._require(layout.cli_link.is_symlink() and os.readlink(layout.cli_link) == str(layout.targets[1]), "CLI link changed")
    for wrapper in layout.wrappers:
        native._safe_directory(wrapper.parent)
        native._require(stat.S_IMODE(native._regular_owned(wrapper).st_mode) == 0o700, "wrapper mode changed")
    for config in layout.bridge_configs:
        native._safe_directory(config.parent)
        native._require(stat.S_IMODE(native._regular_owned(config, executable=False).st_mode) == 0o600, "bridge config mode changed")
        native._require(
            native.tomllib.loads(config.read_text(encoding="utf-8")).get("stock_buzz", {}).get("executable")
            == str(layout.targets[3]),
            "bridge config custom ACP path changed",
        )
    _sidecars_unchanged(pin, sidecars, layout)
    return pin


def _backup(target: Path, backup_root: Path, pin: dict | None = None) -> Path:
    """Durably preserve only the running Desktop before a switch."""

    native._ensure_private_directory(backup_root)
    backup = Path(tempfile.mkdtemp(prefix="backup-", dir=backup_root))
    copy = backup / "buzz-desktop"
    native.shutil.copy2(target, copy, follow_symlinks=False)
    digest = native._sha256(copy)
    native._require(digest == native._sha256(target), "Desktop backup copy mismatch")
    with copy.open("rb") as payload:
        os.fsync(payload.fileno())
    manifest = backup / "manifest.json"
    with manifest.open("x", encoding="utf-8") as output:
        data = {"version": 1, "target": str(target), "sha256": digest, "mode": 0o755}
        if pin and pin["version"] == 2:
            data.update(version=2, pin=pin)
        json.dump(data, output)
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())
    os.chmod(manifest, 0o600)
    with manifest.open("rb") as saved_manifest:
        os.fsync(saved_manifest.fileno())
    native._fsync_directory(backup)
    native._fsync_directory(backup_root)
    native._fsync_directory(backup_root.parent)
    return backup


def _backup_source(backup: Path, layout: native.Layout, backup_root: Path) -> tuple[Path, str]:
    """Validate an exact backup before stopping Desktop."""

    native._safe_directory(backup_root, private=True)
    native._require(backup.parent == backup_root and not backup.is_symlink(), "backup must be a direct child")
    native._safe_directory(backup, private=True)
    data = native._json(backup / "manifest.json")
    v2 = layout.pin is not None and layout.pin["version"] == 2
    fields = {"version", "target", "sha256", "mode"} | ({"pin"} if v2 else set())
    native._require(set(data) == fields, "invalid backup manifest")
    native._require(data["version"] == (2 if v2 else 1) and data["target"] == str(layout.targets[0]) and data["mode"] == 0o755, "backup target mismatch")
    if v2:
        native._require(data["pin"] == layout.pin, "backup pin binding changed")
    digest = data["sha256"]
    native._require(isinstance(digest, str) and HEX64.fullmatch(digest), "invalid backup hash")
    source = backup / "buzz-desktop"
    native._require(stat.S_IMODE(native._regular_owned(source).st_mode) == 0o755, "backup mode mismatch")
    native._require(native._sha256(source) == digest, "backup hash mismatch")
    return source, digest


def _switch(
    source: Path,
    digest: str,
    layout: native.Layout,
    backup_root: Path,
    launch: Callable[[native.Layout, str], None],
    retained: Callable[[], None] | None = None,
) -> Path:
    """Back up, stop, replace, start, and restore on any switch failure."""

    target = layout.targets[0]
    native._require(stat.S_IMODE(native._regular_owned(target).st_mode) == 0o755, "installed Desktop mode changed")
    native._safe_directory(target.parent)
    if retained:
        retained()
    backup = _backup(target, backup_root, layout.pin)
    old_source, old_digest = _backup_source(backup, layout, backup_root)
    try:
        launch(layout, "stop")
        if retained:
            retained()
        native._require(native._sha256(source) == digest, "Desktop source changed after preflight")
        native._atomic_copy(source, target, 0o755)
        native._require(native._sha256(target) == digest, "installed Desktop hash mismatch")
        launch(layout, "start")
    except BaseException:
        try:
            launch(layout, "stop")
            native._require(native._sha256(old_source) == old_digest, "recovery backup hash mismatch")
            native._atomic_copy(old_source, target, 0o755)
            native._require(native._sha256(target) == old_digest, "recovery Desktop hash mismatch")
            if retained:
                retained()
            launch(layout, "start")
        except Exception as error:
            raise native.InstallError(f"automatic recovery failed; intact backup: {backup}: {error}") from error
        raise
    return backup


def install(
    desktop: Path,
    sidecars: Path,
    pin_path: Path,
    layout: native.Layout = native.Layout(),
    launch: Callable[[native.Layout, str], None] = native._launch,
    backup_root: Path = BACKUPS,
) -> Path:
    """Install a reviewed Desktop artifact without replacing any sidecar."""

    pin = preflight(desktop, sidecars, pin_path, layout)
    layout = _layout(pin, layout)
    source = desktop / "buzz-desktop"
    retained = (lambda: _retained(pin, sidecars, layout, installing=True)) if pin["version"] == 2 else None
    return _switch(source, str(pin["desktop_sha256"]), layout, backup_root, launch, retained)


def rollback(
    backup: Path,
    sidecars: Path,
    pin_path: Path,
    layout: native.Layout = native.Layout(),
    launch: Callable[[native.Layout, str], None] = native._launch,
    backup_root: Path = BACKUPS,
) -> Path:
    """Restore a selected Desktop backup while preserving the current ELF."""

    native._require(os.getuid() == 1000, "installer requires host UID 1000")
    pin = _pin(pin_path)
    layout = _layout(pin, layout)
    _sidecars_unchanged(pin, sidecars, layout)
    source, digest = _backup_source(backup, layout, backup_root)
    native._require(digest == pin["baseline_desktop_sha256"], "rollback backup differs from pinned baseline")
    native._regular_owned(layout.targets[0])
    native._safe_directory(layout.targets[0].parent)
    current = native._sha256(layout.targets[0])
    native._require(current in (pin["desktop_sha256"], pin["baseline_desktop_sha256"]), "current Desktop is not pinned")
    retained = (lambda: _retained(pin, sidecars, layout, installing=False)) if pin["version"] == 2 else None
    return _switch(source, digest, layout, backup_root, launch, retained)


def _interrupt(_signum: int, _frame: FrameType | None) -> None:
    """Trigger recovery after a termination request."""

    raise KeyboardInterrupt


def main() -> int:
    """Expose the guarded preflight, install, and explicit rollback commands."""

    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("preflight", "install"):
        command = commands.add_parser(name)
        command.add_argument("--desktop", type=Path, required=True)
        command.add_argument("--sidecars", type=Path, required=True)
        command.add_argument("--pin", type=Path, required=True)
    command = commands.add_parser("rollback")
    command.add_argument("--backup", type=Path, required=True)
    command.add_argument("--sidecars", type=Path, required=True)
    command.add_argument("--pin", type=Path, required=True)
    args = parser.parse_args()
    signal.signal(signal.SIGTERM, _interrupt)
    try:
        native._require(os.getuid() == 1000, "installer requires host UID 1000")
        lock_path = native.HOME / ".local/state/.rowvia-buzz-native-install.lock"
        native._safe_directory(lock_path.parent, private=True)
        descriptor = os.open(lock_path, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
        with os.fdopen(descriptor, "r+") as lock:
            info = os.fstat(lock.fileno())
            native._require(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid() and stat.S_IMODE(info.st_mode) == 0o600, "unsafe lock file")
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            if args.command == "rollback":
                inverse = rollback(args.backup, args.sidecars, args.pin)
                print(f"rollback complete; inverse backup: {inverse}")
            elif args.command == "install":
                backup = install(args.desktop, args.sidecars, args.pin)
                print(f"installed Desktop only; rollback backup: {backup}")
            else:
                preflight(args.desktop, args.sidecars, args.pin)
                print("Desktop-only preflight passed; no live binaries changed")
    except (native.InstallError, OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print("interrupted; recovery attempted if switching had begun", file=sys.stderr)
        return 130
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
