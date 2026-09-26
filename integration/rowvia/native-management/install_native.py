#!/usr/bin/env python3
"""Install the pinned Rowvia Buzz native binaries with a recoverable backup."""

from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
import signal
import shutil
import stat
import subprocess
import sys
import tempfile
import tomllib
from dataclasses import dataclass
from pathlib import Path
from types import FrameType
from typing import Callable


SOURCE_COMMIT = "55f1d59a558ae16be687648179ff174bf6e55792"
IMAGE_ID = "sha256:b30f21ab265a10892f7a77273440f6eeee7996f0108cf7df2e622965b435d77c"
HOME = Path("/home/orionx")
NATIVE_BIN = HOME / ".cache/buzz-native-0.5.23/usr/bin"
CUSTOM_ACP = HOME / ".local/state/rowvia-buzz-mvp/ttc-build-0dd1323d/artifacts/buzz-acp"
BACKUP_ROOT = HOME / ".local/state/rowvia-buzz-native-install/backups"
LAUNCHER = Path("/home/orionx/project/RowviaContext/scripts/buzz-desktop")
LAUNCHER_SHA256 = "e0a5e8aecbe6212197455a2e501fe638f797a1616b927947a4bc32fff85620d5"
CLI_LINK = HOME / ".local/bin/buzz"
WRAPPERS = (
    HOME / ".local/state/rowvia-buzz-mvp/run/acp-proxy-desktop.sh",
    HOME / ".local/state/rowvia-buzz-mvp/gmail-pilot/mail-reader/start-mail-reader.sh",
)
BRIDGE_CONFIGS = (
    HOME / ".local/state/rowvia-buzz-mvp/config/bridge.desktop-managed.toml",
    HOME / ".local/state/rowvia-buzz-mvp/gmail-pilot/mail-reader/bridge.toml",
)
TARGETS = (NATIVE_BIN / "buzz-desktop", NATIVE_BIN / "buzz", NATIVE_BIN / "buzz-acp", CUSTOM_ACP)


class InstallError(Exception):
    """A deployment invariant failed."""


@dataclass(frozen=True)
class Layout:
    """The four replacement targets and host integration points."""

    targets: tuple[Path, Path, Path, Path] = TARGETS
    backup_root: Path = BACKUP_ROOT
    launcher: Path = LAUNCHER
    cli_link: Path = CLI_LINK
    wrappers: tuple[Path, Path] = WRAPPERS
    bridge_configs: tuple[Path, Path] = BRIDGE_CONFIGS


def _require(condition: bool, message: str) -> None:
    """Reject an unmet deployment invariant."""

    if not condition:
        raise InstallError(message)


def _sha256(path: Path) -> str:
    """Hash a file without loading the executable into memory."""

    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _regular_owned(path: Path, *, executable: bool = True) -> os.stat_result:
    """Require a regular, private-owner file without following a symlink."""

    info = path.lstat()
    _require(stat.S_ISREG(info.st_mode), f"not a regular file: {path}")
    _require(info.st_uid == os.getuid(), f"wrong owner: {path}")
    _require(not info.st_mode & (stat.S_IWGRP | stat.S_IWOTH | stat.S_ISUID | stat.S_ISGID), f"unsafe mode: {path}")
    _require(not executable or bool(info.st_mode & stat.S_IXUSR), f"not owner-executable: {path}")
    _require(info.st_size > 0, f"empty file: {path}")
    return info


def _safe_directory(path: Path, *, private: bool = False) -> None:
    """Reject symlinks and writable or foreign-owned paths inside the host home."""

    _require(path == HOME or HOME in path.parents, f"directory must be below {HOME}: {path}")
    _require(path.resolve(strict=True) == path, f"directory path is not canonical: {path}")
    chain = []
    directory = path
    while True:
        chain.append(directory)
        if directory == HOME:
            break
        directory = directory.parent
    for directory in reversed(chain):
        info = directory.lstat()
        _require(stat.S_ISDIR(info.st_mode), f"directory is a symlink or not a directory: {directory}")
        _require(info.st_uid in (0, os.getuid()), f"foreign-owned directory: {directory}")
        _require(not info.st_mode & (stat.S_IWGRP | stat.S_IWOTH), f"writable directory ancestor: {directory}")
    if private:
        info = path.lstat()
        _require(info.st_uid == os.getuid() and stat.S_IMODE(info.st_mode) == 0o700, f"directory must be owned and mode 0700: {path}")


def _ensure_private_directory(path: Path) -> None:
    """Create one private directory after validating its existing parent."""

    _safe_directory(path.parent)
    try:
        path.mkdir(mode=0o700)
    except FileExistsError:
        pass
    _safe_directory(path, private=True)


def _fsync_directory(path: Path) -> None:
    """Persist newly created backup directory entries."""

    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def _json(path: Path) -> dict:
    """Load a provenance or backup manifest object."""

    _regular_owned(path, executable=False)
    data = json.loads(path.read_text(encoding="utf-8"))
    _require(isinstance(data, dict), f"invalid JSON object: {path}")
    return data


def _loader(path: Path) -> None:
    """Check host loader and symbol resolution."""

    result = subprocess.run(["ldd", "-r", str(path)], capture_output=True, text=True, timeout=30, check=False)
    _require(result.returncode == 0 and "not found" not in result.stdout + result.stderr and "undefined symbol" not in result.stdout + result.stderr, f"host ldd -r failed: {path}")


def _contains(path: Path, marker: str) -> bool:
    """Find a required build marker in an ELF."""

    result = subprocess.run(["strings", "-a", str(path)], capture_output=True, text=True, timeout=30, check=True)
    return marker in result.stdout


def preflight(desktop: Path, sidecars: Path, layout: Layout = Layout()) -> tuple[Path, Path, Path]:
    """Validate exact artifacts, targets, symlink, wrappers, and host ABI."""

    _require(os.getuid() == 1000, "installer requires host UID 1000")
    _safe_directory(desktop, private=True)
    _safe_directory(sidecars, private=True)
    desktop_meta = _json(desktop / "provenance.json")
    sidecar_meta = _json(sidecars / "provenance.json")
    for data in (desktop_meta, sidecar_meta):
        _require(data.get("source_commit") == SOURCE_COMMIT, "source commit mismatch")
        _require(data.get("image_id") == IMAGE_ID, "build image mismatch")
    _require(desktop_meta.get("build_mode") == "live" and desktop_meta.get("deployable") is True, "desktop is not a deployable live build")
    _require(desktop_meta.get("artifact_filename") == "buzz-desktop", "unexpected desktop artifact name")
    _require(desktop_meta.get("tauri_identifier") == "xyz.block.buzz.app", "Tauri app identity mismatch")
    _require(desktop_meta.get("product_name") == "Buzz", "desktop product identity mismatch")
    _require(desktop_meta.get("host_ldd_r") == "pass", "desktop build lacks host loader validation")
    artifacts = sidecar_meta.get("artifacts")
    _require(isinstance(artifacts, dict) and set(artifacts) == {"buzz", "buzz-acp"}, "unexpected sidecar manifest")
    _require(all(isinstance(artifacts[name], dict) for name in ("buzz", "buzz-acp")), "invalid sidecar artifact metadata")
    sources = (desktop / "buzz-desktop", sidecars / "buzz", sidecars / "buzz-acp")
    hashes = (desktop_meta.get("binary_sha256"), artifacts["buzz"].get("sha256"), artifacts["buzz-acp"].get("sha256"))
    markers = ("xyz.block.buzz.app", "draft-connector", "buzz.trusted-turn-context/v1")
    for index, source in enumerate(sources):
        source_info = _regular_owned(source)
        _require(stat.S_IMODE(source_info.st_mode) == 0o755, f"unexpected artifact mode: {source}")
        _require(_sha256(source) == hashes[index], f"artifact SHA-256 mismatch: {source}")
        _require(_contains(source, markers[index]), f"artifact patch marker missing: {source}")
        _loader(source)
    for name, marker in (("buzz", markers[1]), ("buzz-acp", markers[2])):
        _require(artifacts[name].get("patch_marker") == marker and artifacts[name].get("host_ldd_r") == "pass", f"sidecar provenance incomplete: {name}")
    for index, target in enumerate(layout.targets):
        target_info = _regular_owned(target)
        _require(stat.S_IMODE(target_info.st_mode) == (0o700 if index == 3 else 0o755), f"unexpected target mode: {target}")
        _safe_directory(target.parent)
    _require(stat.S_IMODE(_regular_owned(layout.launcher).st_mode) == 0o755, "unexpected launcher mode")
    if layout.launcher == LAUNCHER:
        _require(_sha256(layout.launcher) == LAUNCHER_SHA256, "launcher SHA-256 mismatch")
    _safe_directory(layout.cli_link.parent)
    _require(layout.cli_link.is_symlink() and os.readlink(layout.cli_link) == str(layout.targets[1]), "CLI link does not follow cached buzz")
    for wrapper in layout.wrappers:
        _safe_directory(wrapper.parent)
        _require(stat.S_IMODE(_regular_owned(wrapper).st_mode) == 0o700, f"unexpected wrapper mode: {wrapper}")
    for config in layout.bridge_configs:
        _safe_directory(config.parent)
        _require(stat.S_IMODE(_regular_owned(config, executable=False).st_mode) == 0o600, f"unexpected bridge config mode: {config}")
        with config.open("rb") as input_file:
            settings = tomllib.load(input_file)
        _require(settings.get("stock_buzz", {}).get("executable") == str(layout.targets[3]), f"custom ACP config points elsewhere: {config}")
    return sources


def _atomic_copy(source: Path, target: Path, mode: int) -> None:
    """Publish one executable atomically in its original directory."""

    descriptor, temporary = tempfile.mkstemp(prefix=".rowvia-install-", dir=target.parent)
    try:
        with os.fdopen(descriptor, "wb") as output, source.open("rb") as input_file:
            shutil.copyfileobj(input_file, output, 1024 * 1024)
            output.flush()
            os.fchmod(output.fileno(), mode)
            os.fsync(output.fileno())
        os.replace(temporary, target)
        directory = os.open(target.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def _backup(layout: Layout) -> Path:
    """Keep all original executables with a SHA manifest before stopping Buzz."""

    _ensure_private_directory(layout.backup_root.parent)
    _ensure_private_directory(layout.backup_root)
    backup = Path(tempfile.mkdtemp(prefix="backup-", dir=layout.backup_root))
    entries = []
    for index, target in enumerate(layout.targets):
        info = _regular_owned(target)
        copy = backup / str(index)
        shutil.copy2(target, copy, follow_symlinks=False)
        _require(_sha256(copy) == _sha256(target), f"backup copy mismatch: {target}")
        with copy.open("rb") as payload:
            os.fsync(payload.fileno())
        entries.append({"target": str(target), "file": str(index), "sha256": _sha256(copy), "mode": stat.S_IMODE(info.st_mode)})
    manifest = backup / "manifest.json"
    with manifest.open("x", encoding="utf-8") as output:
        json.dump({"version": 1, "entries": entries}, output, indent=2)
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())
    os.chmod(manifest, 0o600)
    with manifest.open("rb") as saved_manifest:
        os.fsync(saved_manifest.fileno())
    _fsync_directory(backup)
    _fsync_directory(layout.backup_root)
    _fsync_directory(layout.backup_root.parent)
    _fsync_directory(layout.backup_root.parent.parent)
    return backup


def _restore(backup: Path, layout: Layout) -> None:
    """Validate a complete backup before replacing any target."""

    _restore_validate(backup, layout)
    entries = _json(backup / "manifest.json").get("entries")
    for index, entry in enumerate(entries):
        _atomic_copy(backup / str(index), layout.targets[index], entry["mode"])


def _launch(layout: Layout, action: str) -> None:
    """Coordinate the transient Desktop unit through its host launcher."""

    descriptor = os.open(layout.launcher, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        info = os.fstat(descriptor)
        _require(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid() and stat.S_IMODE(info.st_mode) == 0o755, "launcher changed before execution")
        if layout.launcher == LAUNCHER:
            with os.fdopen(os.dup(descriptor), "rb") as source:
                digest = hashlib.file_digest(source, "sha256").hexdigest()
            _require(digest == LAUNCHER_SHA256, "launcher changed before execution")
            os.lseek(descriptor, 0, os.SEEK_SET)
        subprocess.run(["/usr/bin/bash", f"/proc/self/fd/{descriptor}", action], pass_fds=(descriptor,), check=True, timeout=60)
    finally:
        os.close(descriptor)


def _interrupt(_signum: int, _frame: FrameType | None) -> None:
    """Turn termination into an exception so the installer can restore."""

    raise KeyboardInterrupt


def install(desktop: Path, sidecars: Path, layout: Layout = Layout(), launch: Callable[[Layout, str], None] = _launch) -> Path:
    """Back up, stop, replace, restart, and recover on any switch failure."""

    sources = preflight(desktop, sidecars, layout)
    source_hashes = tuple(_sha256(source) for source in sources)
    backup = _backup(layout)
    switch_started = False
    try:
        switch_started = True
        launch(layout, "stop")
        for source, expected_hash, target in zip((*sources, sources[2]), (*source_hashes, source_hashes[2]), layout.targets, strict=True):
            _require(_sha256(source) == expected_hash, f"source changed after preflight: {source}")
            _atomic_copy(source, target, stat.S_IMODE(target.stat().st_mode))
            _require(_sha256(target) == expected_hash, f"installed SHA-256 mismatch: {target}")
        launch(layout, "start")
    except BaseException:
        if switch_started:
            try:
                launch(layout, "stop")
                _restore(backup, layout)
                launch(layout, "start")
            except Exception as recovery_error:
                raise InstallError(f"automatic rollback failed; intact backup: {backup}: {recovery_error}") from recovery_error
        raise
    return backup


def rollback(backup: Path, layout: Layout = Layout(), launch: Callable[[Layout, str], None] = _launch) -> None:
    """Restore a selected backup, keeping it available for another attempt."""

    _require(os.getuid() == 1000, "installer requires host UID 1000")
    _require(backup.is_dir(), "backup does not exist")
    # Validate before stopping the running service.
    _restore_validate(backup, layout)
    launch(layout, "stop")
    _restore(backup, layout)
    launch(layout, "start")


def _restore_validate(backup: Path, layout: Layout) -> None:
    """Validate a backup without changing live files."""

    _safe_directory(layout.backup_root, private=True)
    _require(backup.parent == layout.backup_root and not backup.is_symlink(), "backup must be a direct child of backup root")
    _require(backup.lstat().st_uid == os.getuid() and stat.S_IMODE(backup.lstat().st_mode) == 0o700, "unsafe backup ownership or mode")
    entries = _json(backup / "manifest.json").get("entries")
    _require(isinstance(entries, list) and len(entries) == 4, "incomplete backup manifest")
    for index, entry in enumerate(entries):
        _require(isinstance(entry, dict) and entry.get("target") == str(layout.targets[index]) and entry.get("file") == str(index), "backup target mismatch")
        source = backup / str(index)
        _regular_owned(source)
        _require(_sha256(source) == entry.get("sha256"), f"backup SHA-256 mismatch: {source}")
        _require(entry.get("mode") in (0o700, 0o755), "unsupported backup executable mode")


def main() -> int:
    """Expose preflight, install, and explicit rollback with fixed live paths."""

    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("preflight", "install"):
        command = commands.add_parser(name)
        command.add_argument("--desktop", type=Path, required=True)
        command.add_argument("--sidecars", type=Path, required=True)
    commands.add_parser("rollback").add_argument("--backup", type=Path, required=True)
    args = parser.parse_args()
    signal.signal(signal.SIGTERM, _interrupt)
    try:
        lock_path = HOME / ".local/state/.rowvia-buzz-native-install.lock"
        _require(os.getuid() == 1000, "installer requires host UID 1000")
        _safe_directory(lock_path.parent, private=True)
        descriptor = os.open(lock_path, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
        with os.fdopen(descriptor, "r+") as lock:
            lock_info = os.fstat(lock.fileno())
            _require(stat.S_ISREG(lock_info.st_mode) and lock_info.st_uid == os.getuid() and stat.S_IMODE(lock_info.st_mode) == 0o600, "unsafe lock file")
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            if args.command == "rollback":
                rollback(args.backup)
                print("rollback complete")
            else:
                if args.command == "install":
                    backup = install(args.desktop, args.sidecars)
                    print(f"installed; rollback backup: {backup}")
                else:
                    preflight(args.desktop, args.sidecars)
                    print("preflight passed; no live files changed")
    except (InstallError, OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print("interrupted; automatic recovery was attempted if switching had begun", file=sys.stderr)
        return 130
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
