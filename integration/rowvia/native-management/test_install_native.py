#!/usr/bin/env python3
"""Resource-light mock checks for the native installer; never touch live paths."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
import resource
import subprocess
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
        # The new full-upgrade lane keeps the legacy consumers above intact.
        installer.HOME = root
        installer.CUSTOM_ACP = layout.targets[3]
        baseline_root = root / "rowvia-buzz-native-startup-live-20261006/native-root"
        baseline_targets = tuple(baseline_root / "usr/bin" / name for name in ("buzz-desktop", "buzz", "buzz-acp")) + (layout.targets[3],)
        for old_target, target in zip(layout.targets, baseline_targets, strict=True):
            _put(target, old_target.read_bytes(), 0o700 if target == baseline_targets[3] else 0o755)
        for directory in (baseline_root.parent, baseline_root, baseline_root / "usr", baseline_root / "usr/bin"):
            directory.chmod(0o700)
        layout.cli_link.unlink()
        layout.cli_link.symlink_to(baseline_targets[1])
        native_root = root / "new-native-root"
        pin_path = root / "upgrade-pin.json"
        source_commit = "7" * 40
        desktop_meta = json.loads((desktop / "provenance.json").read_text())
        desktop_meta.update(source_commit=source_commit, owner_test_hook=True)
        _put(desktop / "provenance.json", json.dumps(desktop_meta).encode(), 0o600)
        sidecar_meta = json.loads((sidecars / "provenance.json").read_text())
        sidecar_meta["source_commit"] = source_commit
        for name in installer.ROOT_HELPERS:
            _put(sidecars / name, ("new helper " + name).encode())
            _put(baseline_root / "usr/bin" / name, ("old helper " + name).encode())
            sidecar_meta["artifacts"][name] = {"sha256": _hash(sidecars / name), "host_ldd_r": "pass", "patch_marker": None}
        _put(sidecars / "provenance.json", json.dumps(sidecar_meta).encode(), 0o600)
        sources = (desktop / "buzz-desktop", sidecars / "buzz", sidecars / "buzz-acp")
        pin = {
            "version": 2, "source_commit": source_commit, "image_id": installer.IMAGE_ID,
            "native_root": str(native_root), "baseline_native_root": str(baseline_root),
            "artifact_sha256": [_hash(path) for path in sources],
            "baseline_sha256": [_hash(path) for path in baseline_targets],
            "helper_sha256": [_hash(sidecars / name) for name in installer.ROOT_HELPERS],
            "baseline_helper_sha256": [_hash(baseline_root / "usr/bin" / name) for name in installer.ROOT_HELPERS],
            "launcher_sha256": installer.LAUNCHER_SHA256,
            "wrapper_sha256": [_hash(path) for path in layout.wrappers],
            "bridge_config_sha256": [_hash(path) for path in layout.bridge_configs],
            "state_scope": "rowvia-desktop-cold-v1",
        }
        _put(pin_path, json.dumps(pin).encode(), 0o600)
        pinned = installer._load_pin(pin_path)
        pinned = installer.replace(pinned, launcher=layout.launcher, cli_link=layout.cli_link,
                                   wrappers=layout.wrappers, bridge_configs=layout.bridge_configs,
                                   backup_root=root / "upgrade-custody/backups")
        # Closed-reader inspection is the external runtime boundary in this fixture.
        production_closed = installer._closed
        original_path = installer.Path
        original_run = installer.subprocess.run
        original_readlink = installer.os.readlink
        proc = root / "fake-proc"
        cgroup = root / "fake-cgroup"
        proc.mkdir()
        cgroup.mkdir()
        process_file = cgroup / "cgroup.procs"
        process_file.write_text("")
        properties = {"ActiveState": "inactive", "MainPID": "0", "ControlGroup": ""}
        def routed_path(value: str) -> Path:
            """Redirect only the production kernel metadata paths into fixtures."""

            if value == "/proc":
                return proc
            if value == "/sys/fs/cgroup/user.slice/user-1000.slice/user@1000.service/app.slice/buzz-desktop.service":
                return cgroup
            return original_path(value)

        def fake_show(arguments: list[str], **_kwargs: object) -> object:
            """Model the selected user-unit properties without launching commands."""

            name = next(value.split("=", 1)[1] for value in arguments if value.startswith("--property="))
            return subprocess.CompletedProcess(arguments, 0, stdout=properties[name], stderr="")

        installer.Path = routed_path
        installer.subprocess.run = fake_show
        opaque_process = proc / "124"
        opaque_process.mkdir()
        assert opaque_process.stat().st_uid == os.getuid()

        def opaque_readlink(path: object, **kwargs: object) -> str:
            """Model an unrelated same-UID host agent with an inaccessible exe."""

            if path == opaque_process / "exe":
                raise PermissionError("opaque unrelated host process")
            return original_readlink(path, **kwargs)

        installer.os.readlink = opaque_readlink
        try:
            production_closed(pinned)
            for property_name, bad_value in (("ActiveState", "active"), ("MainPID", "1")):
                previous = properties[property_name]
                properties[property_name] = bad_value
                try:
                    production_closed(pinned)
                    raise AssertionError("active native unit passed closure guard")
                except installer.InstallError:
                    pass
                properties[property_name] = previous
            process_file.write_text("123\n")
            try:
                production_closed(pinned)
                raise AssertionError("nonempty descendants passed closure guard")
            except installer.InstallError:
                pass
            process_file.write_text("")
            fake_process = proc / "123"
            fake_process.mkdir()
            executable = fake_process / "exe"
            for selected in (pinned.targets[3], baseline_root / "usr/bin" / installer.ROOT_HELPERS[0]):
                executable.symlink_to(selected)
                try:
                    production_closed(pinned)
                    raise AssertionError("selected native reader passed closure guard")
                except installer.InstallError:
                    pass
                executable.unlink()
            fake_process.rmdir()
            production_closed(pinned)
        finally:
            installer.Path = original_path
            installer.subprocess.run = original_run
            installer.os.readlink = original_readlink
        installer._closed = lambda _layout: None
        installer.preflight(desktop, sidecars, pinned)
        helper = sidecars / installer.ROOT_HELPERS[0]
        helper.rename(sidecars / "saved-helper")
        try:
            installer.preflight(desktop, sidecars, pinned)
            raise AssertionError("missing bundled helper passed preflight")
        except FileNotFoundError:
            pass
        (sidecars / "saved-helper").rename(helper)
        for key, value in (("source_commit", "1" * 40), ("owner_test_hook", False)):
            rejected = dict(desktop_meta)
            rejected[key] = value
            _put(desktop / "provenance.json", json.dumps(rejected).encode(), 0o600)
            try:
                installer.preflight(desktop, sidecars, pinned)
                raise AssertionError("unaccepted artifact passed preflight")
            except installer.InstallError:
                pass
        _put(desktop / "provenance.json", json.dumps(desktop_meta).encode(), 0o600)
        state = root / ".local/share/xyz.block.buzz.app/agents/managed-agents.json"
        secret_fallback = root / ".local/share/xyz.block.buzz.app/identity.key"
        _put(state, b"original durable state", 0o600)
        _put(secret_fallback, b"opaque fallback", 0o600)
        archive = root / ".buzz/archive/archive.db"
        for suffix in ("", "-wal", "-shm"):
            _put(Path(str(archive) + suffix), ("archive" + suffix).encode(), 0o664)
        archive.parent.chmod(0o775)
        teams = state.with_name("teams.json")
        _put(teams, b"owned primary-group state", 0o664)
        teams.parent.chmod(0o775)
        ignored = root / ".local/share/xyz.block.buzz.app/agents/logs/log.txt"
        _put(ignored, b"logs must not enter snapshot", 0o600)
        upgrade_actions = []
        def upgrade_launch(selected: object, action: str) -> None:
            """Record root selection and simulate startup rewriting durable state."""

            upgrade_actions.append((action, selected.native_root))
            if action == "start" and selected.native_root == native_root:
                _put(state, b"new schema", 0o600)
                _put(archive, b"rewritten archive", 0o664)

        upgrade_backup = installer.install(desktop, sidecars, pinned, upgrade_launch)
        assert upgrade_actions == [("stop", native_root), ("start", native_root)]
        assert os.readlink(pinned.cli_link) == str(native_root / "usr/bin/buzz")
        assert [path.read_bytes() for path in baseline_targets[:3]] == list(original[:3])
        state_manifest = installer._validate_state(upgrade_backup)
        assert str(ignored) not in state_manifest["roots"]
        installer.rollback(upgrade_backup, pinned, upgrade_launch)
        assert upgrade_actions[-1] == ("start", baseline_root)
        assert state.read_bytes() == b"original durable state"
        assert secret_fallback.read_bytes() == b"opaque fallback"
        assert teams.stat().st_mode & 0o777 == 0o664
        assert teams.parent.stat().st_mode & 0o777 == 0o775
        assert all((native_root / "usr/bin" / name).exists() for name in installer.ROOT_HELPERS)
        assert os.readlink(pinned.cli_link) == str(baseline_targets[1])
        assert all(Path(str(archive) + suffix).read_bytes() == ("archive" + suffix).encode() for suffix in ("", "-wal", "-shm"))
        # A failure after the first new launch must restore cold state and old selection.
        second_root = root / "second-native-root"
        second_pin = dict(pin, native_root=str(second_root))
        second = installer.replace(pinned, native_root=second_root, pin=second_pin)
        failed_actions = []
        def fail_new_start(selected: object, action: str) -> None:
            """Model a migration followed by a failed candidate GUI launch."""

            failed_actions.append((action, selected.native_root))
            if action == "start" and selected.native_root == second_root:
                _put(state, b"migration before failure", 0o600)
                _put(state.parent / "retention/new-scope.db", b"new retention state", 0o664)
                raise RuntimeError("candidate launch failed")

        try:
            installer.install(desktop, sidecars, second, fail_new_start)
            raise AssertionError("failed new-root launch was accepted")
        except RuntimeError:
            pass
        assert failed_actions[-1] == ("start", baseline_root)
        assert state.read_bytes() == b"original durable state"
        assert not (state.parent / "retention/new-scope.db").exists()
        assert os.readlink(pinned.cli_link) == str(baseline_targets[1])
        # CLI entrypoints require an explicit reviewed pin before any runtime work.
        result = subprocess.run(["python3", str(SCRIPT), "preflight", "--desktop", str(desktop),
                                 "--sidecars", str(sidecars)], capture_output=True, timeout=5)
        assert result.returncode == 2 and b"--pin" in result.stderr
    print("native installer legacy, explicit-pin, new-root and cold-state recovery checks passed")


if __name__ == "__main__":
    main()
