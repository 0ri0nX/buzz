#!/usr/bin/env python3
"""Switch one pinned Buzz Desktop ELF while retaining the installed sidecars."""

from __future__ import annotations

import argparse
from collections.abc import Callable
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


def _pin(path: Path) -> dict[str, str | int]:
    """Load an operator-reviewed, private exact-artifact pin."""

    native._safe_directory(path.parent, private=True)
    info = native._regular_owned(path, executable=False)
    native._require(stat.S_IMODE(info.st_mode) == 0o600, "pin must be mode 0600")
    data = json.loads(path.read_text(encoding="utf-8"))
    native._require(isinstance(data, dict) and set(data) == PIN_FIELDS, "invalid pin fields")
    native._require(data["version"] == 1, "unsupported pin version")
    native._require(
        isinstance(data["desktop_source_commit"], str)
        and HEX40.fullmatch(data["desktop_source_commit"])
        and data["desktop_source_commit"] != native.SOURCE_COMMIT,
        "invalid new Desktop commit pin",
    )
    for field in PIN_FIELDS - {"version", "desktop_source_commit"}:
        native._require(isinstance(data[field], str) and HEX64.fullmatch(data[field]), f"invalid {field}")
    return data


def _sidecars_unchanged(pin: dict[str, str | int], sidecars: Path, layout: native.Layout) -> None:
    """Require old sidecar provenance and all three installed sidecar hashes."""

    native._safe_directory(sidecars, private=True)
    provenance = native._json(sidecars / "provenance.json")
    native._require(
        provenance.get("source_commit") == native.SOURCE_COMMIT
        and provenance.get("image_id") == native.IMAGE_ID,
        "sidecar source or image provenance mismatch",
    )
    artifacts = provenance.get("artifacts")
    native._require(isinstance(artifacts, dict) and set(artifacts) == {"buzz", "buzz-acp"}, "invalid sidecar provenance")
    checks = (
        ("buzz", "buzz_sha256", layout.targets[1], "draft-connector"),
        ("buzz-acp", "buzz_acp_sha256", layout.targets[2], "buzz.trusted-turn-context/v1"),
    )
    for name, field, target, marker in checks:
        record = artifacts[name]
        native._require(isinstance(record, dict), f"invalid {name} provenance")
        native._require(
            record.get("sha256") == pin[field]
            and record.get("patch_marker") == marker
            and record.get("host_ldd_r") == "pass",
            f"{name} provenance mismatch",
        )
        source = sidecars / name
        native._regular_owned(source)
        native._regular_owned(target)
        native._safe_directory(target.parent)
        native._require(native._sha256(source) == pin[field], f"{name} source hash mismatch")
        native._require(native._sha256(target) == pin[field], f"{name} installed hash changed")
    custom = layout.targets[3]
    native._regular_owned(custom)
    native._safe_directory(custom.parent)
    native._require(native._sha256(custom) == pin["custom_acp_sha256"], "custom ACP hash changed")


def preflight(
    desktop: Path, sidecars: Path, pin_path: Path, layout: native.Layout = native.Layout()
) -> dict[str, str | int]:
    """Validate committed-source provenance, live ELF, and unchanged host seams."""

    native._require(os.getuid() == 1000, "installer requires host UID 1000")
    pin = _pin(pin_path)
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


def _backup(target: Path, backup_root: Path) -> Path:
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
        json.dump({"version": 1, "target": str(target), "sha256": digest, "mode": 0o755}, output)
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
    native._require(set(data) == {"version", "target", "sha256", "mode"}, "invalid backup manifest")
    native._require(data["version"] == 1 and data["target"] == str(layout.targets[0]) and data["mode"] == 0o755, "backup target mismatch")
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
) -> Path:
    """Back up, stop, replace, start, and restore on any switch failure."""

    target = layout.targets[0]
    native._require(stat.S_IMODE(native._regular_owned(target).st_mode) == 0o755, "installed Desktop mode changed")
    native._safe_directory(target.parent)
    backup = _backup(target, backup_root)
    old_source, old_digest = _backup_source(backup, layout, backup_root)
    try:
        launch(layout, "stop")
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
    source = desktop / "buzz-desktop"
    return _switch(source, str(pin["desktop_sha256"]), layout, backup_root, launch)


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
    _sidecars_unchanged(pin, sidecars, layout)
    source, digest = _backup_source(backup, layout, backup_root)
    native._require(digest == pin["baseline_desktop_sha256"], "rollback backup differs from pinned baseline")
    native._regular_owned(layout.targets[0])
    native._safe_directory(layout.targets[0].parent)
    current = native._sha256(layout.targets[0])
    native._require(current in (pin["desktop_sha256"], pin["baseline_desktop_sha256"]), "current Desktop is not pinned")
    return _switch(source, digest, layout, backup_root, launch)


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
