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
import re
import tarfile
import time
from itertools import chain
import stat
import subprocess
import sys
import tempfile
import tomllib
from dataclasses import dataclass, replace
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
LAUNCHER_SHA256 = "1284db2a314c6bbae0817f1ab9014618203b8fc1995a3e2ac8415ad1b514c762"
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
ROOT_HELPERS = ("buzz-agent", "buzz-backend-kubernetes", "buzz-dev-mcp", "git-credential-nostr")


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
    pin: dict | None = None
    native_root: Path | None = None
    baseline_native_root: Path | None = None


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


def _load_pin(path: Path, *, for_rollback: bool = False) -> Layout:
    """Load a reviewed upgrade pin without changing legacy installer consumers."""

    _safe_directory(path.parent, private=True)
    _require(stat.S_IMODE(_regular_owned(path, executable=False).st_mode) == 0o600, "pin must be mode 0600")
    pin = _json(path)
    expected = {"version", "source_commit", "image_id", "native_root", "baseline_native_root",
                "artifact_sha256", "baseline_sha256", "launcher_sha256", "wrapper_sha256",
                "bridge_config_sha256", "state_scope", "helper_sha256", "baseline_helper_sha256"}
    _require(set(pin) == expected and pin["version"] == 2, "unexpected upgrade pin fields")
    _require(isinstance(pin["source_commit"], str) and re.fullmatch("[0-9a-f]{40}", pin["source_commit"]) is not None, "invalid source commit pin")
    _require(pin["image_id"] == IMAGE_ID and pin["state_scope"] == "rowvia-desktop-cold-v1", "image or state scope mismatch")
    for key, count in (("artifact_sha256", 3), ("baseline_sha256", 4),
                       ("wrapper_sha256", 2), ("bridge_config_sha256", 2),
                       ("helper_sha256", 4), ("baseline_helper_sha256", 4)):
        values = pin[key]
        _require(isinstance(values, list) and len(values) == count
                 and all(isinstance(value, str) and re.fullmatch("[0-9a-f]{64}", value) for value in values), "invalid hash pin")
    _require(pin["launcher_sha256"] == LAUNCHER_SHA256, "unreviewed launcher")
    baseline = Path(pin["baseline_native_root"])
    _require(baseline == HOME / "rowvia-buzz-native-startup-live-20261006/native-root", "unexpected baseline native root")
    _safe_directory(baseline, private=True)
    root = Path(pin["native_root"])
    _require(root.is_absolute() and HOME in root.parents and ".." not in root.parts
             and root != baseline and not root.is_symlink()
             and (for_rollback or not root.exists()), "new native root must be absent and distinct")
    _safe_directory(root.parent, private=True)
    if root.exists():
        _safe_directory(root, private=True)
    targets = tuple(baseline / "usr/bin" / name for name in ("buzz-desktop", "buzz", "buzz-acp")) + (CUSTOM_ACP,)
    return Layout(targets=targets, pin=pin, native_root=root, baseline_native_root=baseline)


def _baseline(layout: Layout) -> Layout:
    """Select the recorded old root explicitly for recovery."""

    return replace(layout, native_root=layout.baseline_native_root) if layout.pin else layout


def _closed(layout: Layout) -> None:
    """Require the Desktop unit and selected native readers to be absent."""

    if not layout.pin:
        return
    deadline = time.monotonic() + 10
    values = {}
    for name in ("ActiveState", "MainPID", "ControlGroup"):
        result = subprocess.run(["systemctl", "--user", "show", "buzz-desktop.service",
                                 "--property=" + name, "--value", "--no-pager"],
                                capture_output=True, text=True, check=True, timeout=5)
        values[name] = result.stdout.strip()
    _require(values["ActiveState"] in ("inactive", "failed") and values["MainPID"] == "0", "Desktop is not stopped")
    group = values["ControlGroup"]
    expected_group = "/user.slice/user-1000.slice/user@1000.service/app.slice/buzz-desktop.service"
    _require(group in ("", expected_group), "unexpected Desktop cgroup")
    directory = Path("/sys/fs/cgroup" + expected_group)
    if directory.exists():
        for process_file in directory.rglob("cgroup.procs"):
            _require(time.monotonic() < deadline, "cgroup inspection exceeded deadline")
            _require(not process_file.read_text().strip(), "Desktop descendants remain")
    selected = {str(path) for path in layout.targets}
    for root in (layout.native_root, layout.baseline_native_root):
        if root:
            selected.update(str(root / "usr/bin" / name) for name in ("buzz-desktop", "buzz", "buzz-acp") + ROOT_HELPERS)
    for entry in Path("/proc").iterdir():
        _require(time.monotonic() < deadline, "native reader inspection exceeded deadline")
        if not entry.name.isdecimal():
            continue
        try:
            executable = os.readlink(entry / "exe").removesuffix(" (deleted)")
        except FileNotFoundError:
            continue
        except PermissionError:
            _require(entry.stat().st_uid != os.getuid(), "owned native reader inspection denied")
            continue
        _require(executable not in selected, "selected native reader remains active")


def _state_paths() -> list[Path]:
    """Enumerate only the approved durable Desktop state scopes."""

    app = HOME / ".local/share/xyz.block.buzz.app"
    paths = [app / "agents" / name for name in ("managed-agents.json", "teams.json", "personas.json", "global-agent-config.json")]
    paths += [app / name for name in ("agents/rowvia-management", "custom_harnesses",
                                     "identity.migrated", "identity.key", "localstorage",
                                     "tts-settings.json", "mesh-sharing.json")]
    paths += [HOME / ".buzz" / name for name in ("AGENTS.md", ".nest-agents-version")]
    paths += [HOME / ".config/xyz.block.buzz.app/.window-state.json"]
    databases = [app / name for name in ("observed-unread.db", "channel-head-cache.db")]
    databases += [HOME / ".buzz/archive/archive.db"]
    retention = app / "agents/retention"
    if retention.exists():
        _state_directory(retention)
        databases += sorted(retention.glob("*.db"))
        # A WAL can survive without its main database.
        databases += [Path(str(path)[:-4]) for path in retention.glob("*.db-wal")]
        databases += [Path(str(path)[:-4]) for path in retention.glob("*.db-shm")]
    paths += [Path(str(path) + suffix) for path in databases for suffix in ("", "-wal", "-shm")]
    return sorted(set(paths))


def _state_directory(path: Path) -> None:
    """Validate existing app directories without changing their primary-group modes."""

    _require(path == HOME or HOME in path.parents, "state directory is outside home")
    _require(path.resolve(strict=True) == path, "linked state directory")
    while True:
        info = path.lstat()
        _require(stat.S_ISDIR(info.st_mode) and info.st_uid == os.getuid()
                 and info.st_gid == os.getgid() and not info.st_mode & 0o7002, "unsafe state directory")
        if path == HOME:
            break
        path = path.parent


def _check_state_metadata() -> None:
    """Fail before downtime on unsafe state entries or an excessive snapshot budget."""

    count = 0
    total = 0
    for root in _state_paths():
        if not root.exists() and not root.is_symlink():
            continue
        _state_directory(root.parent)
        entries = chain((root,), root.rglob("*")) if root.is_dir() and not root.is_symlink() else (root,)
        for path in entries:
            info = path.lstat()
            count += 1
            total += info.st_size if stat.S_ISREG(info.st_mode) else 0
            _require((stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode))
                     and info.st_uid == os.getuid() and info.st_gid == os.getgid()
                     and not info.st_mode & 0o7002, "unsafe state metadata")
            _require(count <= 20000 and total <= 2 * 1024**3, "state snapshot exceeds bounded budget")


def _state_member_allowed(path: Path) -> bool:
    """Constrain snapshot restoration to reviewed fixed paths and retention DBs."""

    app = HOME / ".local/share/xyz.block.buzz.app"
    directories = [app / name for name in ("agents/rowvia-management", "custom_harnesses", "localstorage")]
    if any(path == directory or directory in path.parents for directory in directories):
        return True
    retention = app / "agents/retention"
    if path.parent == retention and re.fullmatch(r"[^/]+\.db(?:-wal|-shm)?", path.name):
        return True
    return path in _state_paths()


def _snapshot_state(backup: Path, layout: Layout) -> None:
    """Keep opaque cold state with an absence manifest before the first launch."""

    _closed(layout)
    _check_state_metadata()
    roots = _state_paths()
    count = 0
    total = 0
    def check_member(info: tarfile.TarInfo) -> tarfile.TarInfo:
        """Reject links, foreign owners, unsafe modes, and excessive payloads."""

        nonlocal count, total
        _require(info.isfile() or info.isdir(), "unsupported state entry")
        _require(info.uid == os.getuid() and info.gid == os.getgid()
                 and not info.mode & 0o7002, "unsafe state owner or mode")
        count += 1
        total += info.size
        _require(count <= 20000 and total <= 2 * 1024**3, "state snapshot exceeds bounded budget")
        return info
    with tarfile.open(backup / "desktop-state.tar", "w") as archive:
        for path in roots:
            _require(_state_member_allowed(path), "unapproved state path")
            if path.exists() or path.is_symlink():
                _state_directory(path.parent)
                archive.add(path, arcname=str(path.relative_to(HOME)), filter=check_member)
    os.chmod(backup / "desktop-state.tar", 0o600)
    manifest = {"version": 1, "scope": "rowvia-desktop-cold-v1",
                "roots": [str(path) for path in roots],
                "present": [str(path) for path in roots if path.exists()],
                "sha256": _sha256(backup / "desktop-state.tar")}
    with (backup / "desktop-state.json").open("x", encoding="utf-8") as output:
        json.dump(manifest, output)
        output.flush()
        os.fchmod(output.fileno(), 0o600)
        os.fsync(output.fileno())
    with (backup / "desktop-state.tar").open("rb") as payload:
        os.fsync(payload.fileno())
    _fsync_directory(backup)


def _validate_state(backup: Path) -> dict:
    """Validate opaque snapshot custody and every archived restore pathname."""

    manifest = _json(backup / "desktop-state.json")
    _require(manifest.get("version") == 1 and manifest.get("scope") == "rowvia-desktop-cold-v1", "state manifest mismatch")
    _regular_owned(backup / "desktop-state.tar", executable=False)
    _require(_sha256(backup / "desktop-state.tar") == manifest.get("sha256"), "state snapshot hash mismatch")
    for name in ("roots", "present"):
        _require(isinstance(manifest.get(name), list) and all(isinstance(value, str) and Path(value).is_absolute()
                 and ".." not in Path(value).parts and _state_member_allowed(Path(value)) for value in manifest[name]), "state manifest path mismatch")
    _require(set(manifest["present"]) <= set(manifest["roots"]), "invalid state presence manifest")
    with tarfile.open(backup / "desktop-state.tar", "r") as archive:
        count = 0
        total = 0
        for member in archive:
            path = HOME / member.name
            count += 1
            total += member.size
            _require(count <= 20000 and total <= 2 * 1024**3 and not Path(member.name).is_absolute()
                     and ".." not in Path(member.name).parts and _state_member_allowed(path)
                     and (member.isfile() or member.isdir()) and member.uid == os.getuid()
                     and member.gid == os.getgid() and not member.mode & 0o7002, "unsafe archived state")
    return manifest


def _restore_state(backup: Path, layout: Layout) -> None:
    """Restore only closed Desktop state, retaining displaced state for inspection."""

    _closed(layout)
    manifest = _validate_state(backup)
    displaced = Path(tempfile.mkdtemp(prefix="displaced-state-", dir=backup))
    restore = Path(tempfile.mkdtemp(prefix="restored-state-", dir=backup))
    with tarfile.open(backup / "desktop-state.tar", "r") as archive:
        # All entries were validated, including prohibition of links.
        archive.extractall(restore, filter=lambda member, _directory: member)
    roots = sorted(set(manifest["roots"]) | {str(path) for path in _state_paths()})
    for index, value in enumerate(roots):
        path = Path(value)
        _require(_state_member_allowed(path), "unapproved restore scope")
        if path.exists() or path.is_symlink():
            _state_directory(path.parent)
            _require(not path.is_symlink(), "state path changed to a symlink")
            os.replace(path, displaced / str(index))
        saved = restore / path.relative_to(HOME)
        if value in manifest["present"]:
            _require(saved.exists(), "state payload is missing")
            _require(path.parent.exists(), "state parent is absent")
            _state_directory(path.parent)
            os.replace(saved, path)
            _fsync_directory(path.parent)
    _fsync_directory(displaced)
    _fsync_directory(backup)


def _select_cli(layout: Layout, target: Path) -> None:
    """Atomically select the coherent CLI without overwriting unrelated links."""

    expected = {str(layout.targets[1]), str(layout.native_root / "usr/bin/buzz")}
    _require(layout.cli_link.is_symlink() and os.readlink(layout.cli_link) in expected, "CLI selection changed")
    temporary = layout.cli_link.with_name(".rowvia-buzz-cli-" + str(os.getpid()))
    _require(not temporary.exists() and not temporary.is_symlink(), "CLI staging path exists")
    temporary.symlink_to(target)
    try:
        os.replace(temporary, layout.cli_link)
        _fsync_directory(layout.cli_link.parent)
    finally:
        if temporary.is_symlink():
            temporary.unlink()


def _stage_root(sources: tuple[Path, ...], layout: Layout) -> None:
    """Publish an isolated native root while leaving all baseline files intact."""

    root = layout.native_root
    _require(root is not None and not root.exists() and not root.is_symlink(), "new native root appeared")
    _ensure_private_directory(root)
    _ensure_private_directory(root / "usr")
    _ensure_private_directory(root / "usr/bin")
    for source, expected in zip(sources, layout.pin["artifact_sha256"] + layout.pin["helper_sha256"], strict=True):
        _require(_sha256(source) == expected, "artifact changed before staging")
        target = root / "usr/bin" / source.name
        _atomic_copy(source, target, 0o755)
        _require(_sha256(target) == expected, "staged artifact hash mismatch")


def preflight(desktop: Path, sidecars: Path, layout: Layout = Layout()) -> tuple[Path, ...]:
    """Validate exact artifacts, targets, symlink, wrappers, and host ABI."""

    _require(os.getuid() == 1000, "installer requires host UID 1000")
    _safe_directory(desktop, private=True)
    _safe_directory(sidecars, private=True)
    desktop_meta = _json(desktop / "provenance.json")
    sidecar_meta = _json(sidecars / "provenance.json")
    for data in (desktop_meta, sidecar_meta):
        _require(data.get("source_commit") == (layout.pin["source_commit"] if layout.pin else SOURCE_COMMIT), "source commit mismatch")
        _require(data.get("image_id") == IMAGE_ID, "build image mismatch")
    _require(desktop_meta.get("build_mode") == "live" and desktop_meta.get("deployable") is True, "desktop is not a deployable live build")
    _require(desktop_meta.get("artifact_filename") == "buzz-desktop", "unexpected desktop artifact name")
    _require(desktop_meta.get("tauri_identifier") == "xyz.block.buzz.app", "Tauri app identity mismatch")
    _require(desktop_meta.get("product_name") == "Buzz", "desktop product identity mismatch")
    _require(desktop_meta.get("host_ldd_r") == "pass", "desktop build lacks host loader validation")
    artifacts = sidecar_meta.get("artifacts")
    names = ("buzz", "buzz-acp") + (ROOT_HELPERS if layout.pin else ())
    _require(isinstance(artifacts, dict) and set(artifacts) == set(names), "unexpected sidecar manifest")
    _require(all(isinstance(artifacts[name], dict) for name in names), "invalid sidecar artifact metadata")
    sources = (desktop / "buzz-desktop", sidecars / "buzz", sidecars / "buzz-acp")
    hashes = (desktop_meta.get("binary_sha256"), artifacts["buzz"].get("sha256"), artifacts["buzz-acp"].get("sha256"))
    markers = ("xyz.block.buzz.app", "draft-connector", "buzz.trusted-turn-context/v1")
    if layout.pin:
        _require(list(hashes) == layout.pin["artifact_sha256"] and desktop_meta.get("owner_test_hook") is True, "artifact acceptance pin mismatch")
        _require([artifacts[name].get("sha256") for name in ROOT_HELPERS] == layout.pin["helper_sha256"], "helper acceptance pin mismatch")
        sources += tuple(sidecars / name for name in ROOT_HELPERS)
        hashes += tuple(artifacts[name]["sha256"] for name in ROOT_HELPERS)
        markers += (None,) * len(ROOT_HELPERS)
    for index, source in enumerate(sources):
        source_info = _regular_owned(source)
        _require(stat.S_IMODE(source_info.st_mode) == 0o755, f"unexpected artifact mode: {source}")
        _require(_sha256(source) == hashes[index], f"artifact SHA-256 mismatch: {source}")
        if markers[index]:
            _require(_contains(source, markers[index]), f"artifact patch marker missing: {source}")
        _loader(source)
        if layout.pin and index >= 3:
            _require(artifacts[source.name].get("host_ldd_r") == "pass", "helper loader provenance missing")
    for name, marker in (("buzz", markers[1]), ("buzz-acp", markers[2])):
        _require(artifacts[name].get("patch_marker") == marker and artifacts[name].get("host_ldd_r") == "pass", f"sidecar provenance incomplete: {name}")
    for index, target in enumerate(layout.targets):
        target_info = _regular_owned(target)
        _require(stat.S_IMODE(target_info.st_mode) == (0o700 if index == 3 else 0o755), f"unexpected target mode: {target}")
        _safe_directory(target.parent)
        if layout.pin:
            _require(_sha256(target) == layout.pin["baseline_sha256"][index], "baseline binary changed")
    _require(stat.S_IMODE(_regular_owned(layout.launcher).st_mode) == 0o755, "unexpected launcher mode")
    if layout.launcher == LAUNCHER:
        _require(_sha256(layout.launcher) == LAUNCHER_SHA256, "launcher SHA-256 mismatch")
    _safe_directory(layout.cli_link.parent)
    _require(layout.cli_link.is_symlink() and os.readlink(layout.cli_link) == str(layout.targets[1]), "CLI link does not follow cached buzz")
    for index, wrapper in enumerate(layout.wrappers):
        _safe_directory(wrapper.parent)
        if layout.pin:
            _require(_sha256(wrapper) == layout.pin["wrapper_sha256"][index], "wrapper changed")
        _require(stat.S_IMODE(_regular_owned(wrapper).st_mode) == 0o700, f"unexpected wrapper mode: {wrapper}")
    for index, config in enumerate(layout.bridge_configs):
        _safe_directory(config.parent)
        if layout.pin:
            _require(_sha256(config) == layout.pin["bridge_config_sha256"][index], "bridge config changed")
        _require(stat.S_IMODE(_regular_owned(config, executable=False).st_mode) == 0o600, f"unexpected bridge config mode: {config}")
        with config.open("rb") as input_file:
            settings = tomllib.load(input_file)
        _require(settings.get("stock_buzz", {}).get("executable") == str(layout.targets[3]), f"custom ACP config points elsewhere: {config}")
    if layout.pin:
        for name, expected in zip(ROOT_HELPERS, layout.pin["baseline_helper_sha256"], strict=True):
            path = layout.baseline_native_root / "usr/bin" / name
            _require(stat.S_IMODE(_regular_owned(path).st_mode) == 0o755 and _sha256(path) == expected, "baseline helper changed")
        _check_state_metadata()
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
        if layout.pin:
            _require(_sha256(target) == layout.pin["baseline_sha256"][index], "baseline changed before backup")
        copy = backup / str(index)
        shutil.copy2(target, copy, follow_symlinks=False)
        _require(_sha256(copy) == _sha256(target), f"backup copy mismatch: {target}")
        if layout.pin:
            _require(_sha256(copy) == layout.pin["baseline_sha256"][index], "baseline changed during backup")
        with copy.open("rb") as payload:
            os.fsync(payload.fileno())
        entries.append({"target": str(target), "file": str(index), "sha256": _sha256(copy), "mode": stat.S_IMODE(info.st_mode)})
    helpers = []
    if layout.pin:
        for index, name in enumerate(ROOT_HELPERS):
            target = layout.baseline_native_root / "usr/bin" / name
            _regular_owned(target)
            copy = backup / ("helper-" + str(index))
            shutil.copy2(target, copy, follow_symlinks=False)
            _require(_sha256(copy) == layout.pin["baseline_helper_sha256"][index], "helper backup mismatch")
            with copy.open("rb") as payload:
                os.fsync(payload.fileno())
            helpers.append({"target": str(target), "file": copy.name, "sha256": _sha256(copy)})
    manifest = backup / "manifest.json"
    with manifest.open("x", encoding="utf-8") as output:
        json.dump({"version": 1, "entries": entries, "helpers": helpers, "upgrade_pin": layout.pin}, output, indent=2)
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
    if layout.pin:
        for entry in _json(backup / "manifest.json")["helpers"]:
            target = Path(entry["target"])
            if not target.exists() or _sha256(target) != entry["sha256"]:
                _atomic_copy(backup / entry["file"], target, 0o755)


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
        arguments = ["/usr/bin/bash", f"/proc/self/fd/{descriptor}", action]
        if layout.native_root:
            arguments += ["--owner-hook", "--native-root=" + str(layout.native_root)]
        subprocess.run(arguments, pass_fds=(descriptor,), check=True, timeout=150)
    finally:
        os.close(descriptor)


def _interrupt(_signum: int, _frame: FrameType | None) -> None:
    """Turn termination into an exception so the installer can restore."""

    raise KeyboardInterrupt


def install(desktop: Path, sidecars: Path, layout: Layout = Layout(), launch: Callable[[Layout, str], None] = _launch) -> Path:
    """Back up, stop, replace, restart, and recover on any switch failure."""

    sources = preflight(desktop, sidecars, layout)
    source_hashes = tuple(_sha256(source) for source in sources)
    if layout.pin:
        _stage_root(sources, layout)
    backup = _backup(layout)
    switch_started = False
    try:
        switch_started = True
        launch(layout, "stop")
        if layout.pin:
            _closed(layout)
            _snapshot_state(backup, layout)
        switch_sources = sources
        switch_hashes = source_hashes
        destinations = layout.targets
        if layout.pin:
            destinations = tuple(layout.native_root / "usr/bin" / source.name for source in sources) + (layout.targets[3],)
        for source, expected_hash, target in zip((*switch_sources, switch_sources[2]), (*switch_hashes, switch_hashes[2]), destinations, strict=True):
            _require(_sha256(source) == expected_hash, f"source changed after preflight: {source}")
            _atomic_copy(source, target, stat.S_IMODE(target.stat().st_mode))
            _require(_sha256(target) == expected_hash, f"installed SHA-256 mismatch: {target}")
        if layout.pin:
            _select_cli(layout, layout.native_root / "usr/bin/buzz")
        launch(layout, "start")
    except BaseException:
        if switch_started:
            try:
                launch(layout, "stop")
                _closed(layout)
                _restore(backup, layout)
                if layout.pin:
                    if (backup / "desktop-state.json").exists():
                        _restore_state(backup, layout)
                    _select_cli(layout, layout.targets[1])
                launch(_baseline(layout), "start")
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
    if layout.pin:
        _validate_state(backup)
    launch(layout, "stop")
    _closed(layout)
    _restore(backup, layout)
    if layout.pin:
        _restore_state(backup, layout)
        _select_cli(layout, layout.targets[1])
    launch(_baseline(layout), "start")


def _restore_validate(backup: Path, layout: Layout) -> None:
    """Validate a backup without changing live files."""

    _safe_directory(layout.backup_root, private=True)
    _require(backup.parent == layout.backup_root and not backup.is_symlink(), "backup must be a direct child of backup root")
    _require(backup.lstat().st_uid == os.getuid() and stat.S_IMODE(backup.lstat().st_mode) == 0o700, "unsafe backup ownership or mode")
    manifest = _json(backup / "manifest.json")
    _require(manifest.get("upgrade_pin") == layout.pin, "backup upgrade pin mismatch")
    if layout.pin and (backup / "desktop-state.json").exists():
        _validate_state(backup)
    if layout.pin:
        helpers = manifest.get("helpers")
        _require(isinstance(helpers, list) and len(helpers) == len(ROOT_HELPERS), "incomplete helper backup")
        for index, name in enumerate(ROOT_HELPERS):
            entry = helpers[index]
            _require(entry == {"target": str(layout.baseline_native_root / "usr/bin" / name),
                               "file": "helper-" + str(index), "sha256": layout.pin["baseline_helper_sha256"][index]}, "helper backup manifest mismatch")
            _regular_owned(backup / entry["file"])
            _require(_sha256(backup / entry["file"]) == entry["sha256"], "helper backup changed")
    entries = manifest.get("entries")
    _require(isinstance(entries, list) and len(entries) == 4, "incomplete backup manifest")
    for index, entry in enumerate(entries):
        _safe_directory(layout.targets[index].parent)
        _require(not layout.targets[index].is_symlink(), "restore target changed to a symlink")
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
    for command in commands.choices.values():
        command.add_argument("--pin", type=Path, required=True)
    args = parser.parse_args()
    signal.signal(signal.SIGTERM, _interrupt)
    try:
        layout = _load_pin(args.pin, for_rollback=args.command == "rollback")
        lock_path = HOME / ".local/state/.rowvia-buzz-native-install.lock"
        _require(os.getuid() == 1000, "installer requires host UID 1000")
        _safe_directory(lock_path.parent, private=True)
        descriptor = os.open(lock_path, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
        with os.fdopen(descriptor, "r+") as lock:
            lock_info = os.fstat(lock.fileno())
            _require(stat.S_ISREG(lock_info.st_mode) and lock_info.st_uid == os.getuid() and stat.S_IMODE(lock_info.st_mode) == 0o600, "unsafe lock file")
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            if args.command == "rollback":
                rollback(args.backup, layout)
                print("rollback complete")
            else:
                if args.command == "install":
                    backup = install(args.desktop, args.sidecars, layout)
                    print(f"installed; rollback backup: {backup}")
                else:
                    preflight(args.desktop, args.sidecars, layout)
                    print("preflight passed; no live files changed")
    except (InstallError, OSError, ValueError, KeyError, tarfile.TarError, subprocess.SubprocessError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print("interrupted; automatic recovery was attempted if switching had begun", file=sys.stderr)
        return 130
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
