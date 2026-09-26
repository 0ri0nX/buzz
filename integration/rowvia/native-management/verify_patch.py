#!/usr/bin/env python3
"""Verify a pinned Rowvia Buzz patch in a disposable checkout."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile


CUSTODY = Path(__file__).resolve().parent
REPO = CUSTODY.parents[2]
ARTIFACTS = {
    "management": (CUSTODY, "buzz.rowvia-native-management/v1"),
    "enrollment": (
        CUSTODY / "external-agent-enrollment",
        "buzz.rowvia-external-agent-enrollment/v1",
    ),
}
ENROLLMENT_PATHS = {
    "desktop/src-tauri/src/app_state.rs",
    "desktop/src-tauri/src/commands/external_agent_enrollment.rs",
    "desktop/src-tauri/src/commands/external_agent_enrollment_tests.rs",
    "desktop/src-tauri/src/commands/mod.rs",
    "desktop/src-tauri/src/lib.rs",
    "desktop/src-tauri/src/managed_agents/persona_events.rs",
    "desktop/src-tauri/src/managed_agents/persona_events/tests.rs",
    "desktop/src/features/agents/ui/AgentsView.tsx",
    "desktop/src/features/agents/ui/ExternalAgentEnrollmentDialog.tsx",
    "desktop/src/shared/api/tauri.ts",
    "integration/rowvia/native-management/EXTERNAL-AGENT-ENROLLMENT.md",
}
MANIFEST_KEYS = {
    "contract_version",
    "patch_commit",
    "patch_filename",
    "patch_sha256",
    "result_tree",
    "upstream_base",
}
HEX40 = re.compile(r"[0-9a-f]{40}\Z")
HEX64 = re.compile(r"[0-9a-f]{64}\Z")


def git(*args: str, cwd: Path, env: dict[str, str] | None = None) -> bytes:
    """Run Git without reading input from the terminal."""

    return subprocess.run(
        ("git", *args),
        cwd=cwd,
        env=env,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=True,
    ).stdout


def verify(artifact: str, candidates: list[str]) -> None:
    """Check artifact bytes, source identity, and replayed commit identity."""

    directory, contract = ARTIFACTS[artifact]
    manifest = json.loads((directory / "manifest.json").read_text(encoding="utf-8"))
    if not isinstance(manifest, dict) or set(manifest) != MANIFEST_KEYS:
        raise ValueError("unexpected manifest fields")
    if manifest["contract_version"] != contract:
        raise ValueError("unexpected custody contract")
    for field in ("patch_commit", "result_tree", "upstream_base"):
        if not isinstance(manifest[field], str) or not HEX40.fullmatch(manifest[field]):
            raise ValueError(f"invalid {field}")
    if not isinstance(manifest["patch_sha256"], str) or not HEX64.fullmatch(
        manifest["patch_sha256"]
    ):
        raise ValueError("invalid patch_sha256")
    filename = manifest["patch_filename"]
    if not isinstance(filename, str) or Path(filename).name != filename or not filename.endswith(".patch"):
        raise ValueError("invalid patch_filename")
    patch = directory / filename
    patch_bytes = patch.read_bytes()
    if hashlib.sha256(patch_bytes).hexdigest() != manifest["patch_sha256"]:
        raise ValueError("patch SHA-256 mismatch")
    commit = manifest["patch_commit"]
    base = manifest["upstream_base"]
    if git("rev-parse", f"{commit}^", cwd=REPO).decode().strip() != base:
        raise ValueError("source commit has a different parent")
    if git("rev-parse", f"{commit}^{{tree}}", cwd=REPO).decode().strip() != manifest[
        "result_tree"
    ]:
        raise ValueError("source commit has a different tree")
    if artifact == "enrollment":
        paths = set(
            git("diff-tree", "--no-commit-id", "--name-only", "-r", commit, cwd=REPO)
            .decode()
            .splitlines()
        )
        if paths != ENROLLMENT_PATHS:
            raise ValueError("enrollment commit touches unexpected paths")
    expected = git("format-patch", "--no-signature", "--stdout", "-1", commit, cwd=REPO)
    if patch_bytes != expected:
        raise ValueError("patch differs from the source commit's format-patch")

    identity = git("show", "-s", "--format=%cn%x00%ce%x00%cI", commit, cwd=REPO)
    name, email, date = identity.decode().strip().split("\0")
    environment = os.environ.copy()
    environment.update(
        GIT_COMMITTER_NAME=name,
        GIT_COMMITTER_EMAIL=email,
        GIT_COMMITTER_DATE=date,
    )
    with tempfile.TemporaryDirectory(prefix="rowvia-native-patch-") as temporary:
        checkout = Path(temporary) / "buzz"
        git("clone", "--shared", "--no-checkout", "--quiet", str(REPO), str(checkout), cwd=REPO)
        git("checkout", "--detach", "--quiet", base, cwd=checkout)
        git("am", "--quiet", str(patch), cwd=checkout, env=environment)
        replayed = git("rev-parse", "HEAD", cwd=checkout).decode().strip()
        if replayed != commit:
            raise ValueError(f"replayed commit mismatch: {replayed} != {commit}")
        if git("rev-parse", "HEAD^{tree}", cwd=checkout).decode().strip() != manifest[
            "result_tree"
        ]:
            raise ValueError("replayed tree mismatch")
    print(f"verified {artifact} {commit} from {base} (SHA-256 and exact replay)")

    failed = False
    for candidate in candidates:
        with tempfile.TemporaryDirectory(prefix="rowvia-native-candidate-") as temporary:
            checkout = Path(temporary) / "buzz"
            git("clone", "--shared", "--no-checkout", "--quiet", str(REPO), str(checkout), cwd=REPO)
            revision = git("rev-parse", f"{candidate}^{{commit}}", cwd=checkout).decode().strip()
            git("checkout", "--detach", "--quiet", revision, cwd=checkout)
            result = subprocess.run(
                ("git", "apply", "--check", str(patch)),
                cwd=checkout,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
                text=True,
            )
            outcome = "PASS" if result.returncode == 0 else "FAIL"
            print(f"{candidate} {revision}: git apply --check {outcome}")
            if result.returncode:
                failed = True
                print(result.stderr.strip()[:4000])
            else:
                git("apply", str(patch), cwd=checkout)
                git("diff", "--check", cwd=checkout)
                print(f"{candidate}: git apply and git diff --check PASS")
    if failed:
        raise SystemExit(1)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact", choices=ARTIFACTS, default="management")
    parser.add_argument("--candidate", action="append", default=[], help="local tag or commit to check")
    arguments = parser.parse_args()
    verify(arguments.artifact, arguments.candidate)
