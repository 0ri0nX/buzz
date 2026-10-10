# Guarded Desktop-only enrollment update

`install_desktop_only.py` is a separate one-ELF switch for the external-agent
enrollment change. It leaves the installed `buzz` CLI, both ACP binaries, their
link, wrappers, bridge configs, profiles, and keys in place. The existing
`install_native.py` installer is unchanged. Version 1 below retains the original
cached-root behavior. Use version 2 for a Desktop-only change in a selected
current native root with all six existing sidecars retained.

## Version 2: current native-root Desktop-only switch

Select the root explicitly in an operator-reviewed private pin. The public CLI
link must already point exactly to that root's `usr/bin/buzz`; the installer
does not retarget it or infer the root from a cache. On this host the intended
current selection is
`/home/orionx/rowvia-buzz-native-v0.5.27-20261009/native-root-r2`.
Confirm that selection before constructing a pin; an owner-test root is a
different installation. All paths must be canonical, private and owner safe.

The exact v2 JSON fields are:

```json
{
  "version": 2,
  "native_root": "/home/orionx/rowvia-buzz-native-v0.5.27-20261009/native-root-r2",
  "baseline_desktop_source_commit": "CURRENT_DESKTOP_FULL_COMMIT_SHA",
  "desktop_source_commit": "CANDIDATE_DESKTOP_FULL_COMMIT_SHA",
  "sidecar_source_commit": "RETAINED_SIDECARS_FULL_COMMIT_SHA",
  "image_id": "sha256:b30f21ab265a10892f7a77273440f6eeee7996f0108cf7df2e622965b435d77c",
  "desktop_sha256": "CANDIDATE_DESKTOP_SHA256",
  "baseline_desktop_sha256": "CURRENT_DESKTOP_SHA256",
  "buzz_sha256": "RETAINED_BUZZ_SHA256",
  "buzz_acp_sha256": "RETAINED_BUZZ_ACP_SHA256",
  "custom_acp_sha256": "RETAINED_CUSTOM_ACP_SHA256",
  "helper_sha256": {
    "buzz-agent": "RETAINED_SHA256",
    "buzz-backend-kubernetes": "RETAINED_SHA256",
    "buzz-dev-mcp": "RETAINED_SHA256",
    "git-credential-nostr": "RETAINED_SHA256"
  },
  "launcher_sha256": "REVIEWED_LAUNCHER_SHA256",
  "wrapper_sha256": ["DESKTOP_PROXY_SHA256", "MAIL_READER_WRAPPER_SHA256"],
  "bridge_config_sha256": ["DESKTOP_BRIDGE_SHA256", "MAIL_READER_BRIDGE_SHA256"]
}
```

Replace placeholders with lowercase exact hashes/commits. The wrapper/config
arrays follow the existing Desktop then mail-reader order in `install_native.py`.
The baseline commit is an operator assertion; the installed baseline is checked
against its exact hash. The candidate commit must differ from it. The retained
source directory passed as `--sidecars` must contain all six named binaries and
an exact six-entry provenance manifest naming the retained commit and pinned
image. Every source and installed sidecar hash, mode and host loader must pass;
the CLI/ACP retain their known patch markers and loader provenance. The custom
ACP and existing wrapper/config paths remain the existing host bindings.

Build the candidate from a committed revision with `--live --owner-test-hook`.
Its provenance must include `owner_test_hook: true`, canonical Tauri/product
identity, deployable live mode and successful host loader proof. The candidate
must be an ELF containing the app and enrollment markers. This pin permits a
new Desktop commit while retaining the separately pinned sidecar commit.

Use the same `preflight`, `install`, and `rollback` commands shown below with
the v2 pin and current retained sidecar artifact directory. v2 resolves only
`native_root/usr/bin/buzz-desktop` as its replacement target. Launch actions
receive `--owner-hook --native-root=EXACT_SELECTED_ROOT`. Retained hashes,
CLI link and host bindings are rechecked before and after the stop. Only Desktop
is copied; relay, sidecars, wrappers, configs and opaque state are untouched.
Version 2 backup manifests bind the entire reviewed pin and exact Desktop target;
explicit rollback requires that same pin and a verified baseline backup.
Automatic recovery reuses the immediate backup after stop/start/copy failures.
Recovery rechecks retained bindings after restoring Desktop and before restarting;
if drift persists, it leaves Desktop stopped and reports the intact backup.

## Version 1: original cached-root switch

Do not use this path until the enrollment source is committed and independently
reviewed. `build-linux-desktop.sh` archives an exact commit; dirty source files
cannot enter its output. Build a **live** Desktop ELF from the new full commit
in an absent, private directory. The build records the commit, image ID, ELF
SHA-256, canonical Tauri identity, and host loader result in `provenance.json`.
Run the heavy build through the shared workstation cap:

```bash
/home/orionx/project/RowviaContext/repos/rowvia-context-shared-session/scripts/run-capped-check \
  --cwd /home/orionx/project/RowviaContext/repos/buzz-upgrade-v0.5.27 -- \
  bash integration/rowvia/native-management/build-linux-desktop.sh \
  --live --revision NEW_FULL_COMMIT_SHA \
  --output /home/orionx/rowvia-buzz-enrollment-live-NEW_SHORT_SHA
```

After inspecting the build provenance and installed hashes, create a reviewed
JSON pin in a UID-1000-owned, mode-0700 directory below `/home/orionx`, with
the file itself mode 0600. Do not commit it. Its exact fields are:

```json
{
  "version": 1,
  "desktop_source_commit": "NEW_FULL_COMMIT_SHA",
  "desktop_sha256": "SHA256_OF_NEW_BUILD_OUTPUT_BUZZ_DESKTOP",
  "baseline_desktop_sha256": "SHA256_OF_CURRENT_INSTALLED_BUZZ_DESKTOP",
  "buzz_sha256": "SHA256_OF_CURRENT_INSTALLED_BUZZ",
  "buzz_acp_sha256": "SHA256_OF_CURRENT_INSTALLED_BUZZ_ACP",
  "custom_acp_sha256": "SHA256_OF_CURRENT_BRIDGE_BUZZ_ACP"
}
```

Replace every placeholder with its real lowercase hex value. The existing
sidecar provenance directory on this host is
`/home/orionx/rowvia-buzz-mvp-sidecars-be11548b7`; its provenance must still
name old source `55f1d59a558ae16be687648179ff174bf6e55792`, the pinned
Docker image, and the same CLI/ACP hashes as the installed files. The
installer checks all those conditions, the custom ACP hash, exact source and
ELF pin, canonical live-build metadata, owner/modes/path safety, embedded
enrollment marker, and host `ldd -r`. It checks the launcher hash and bridge
paths too. The pin is an operator-reviewed statement about the build output;
provenance fields alone are not cryptographic source attestation. Same-UID
processes and the external agent runtime are not isolated by this installer.

With `PIN` set to that absolute reviewed JSON path and `OUTPUT` set to the
absolute live build directory, check first:

```bash
python3 integration/rowvia/native-management/install_desktop_only.py preflight \
  --desktop "$OUTPUT" \
  --sidecars /home/orionx/rowvia-buzz-mvp-sidecars-be11548b7 \
  --pin "$PIN"
```

`preflight` creates only the shared private installer lock. After explicit
deployment authorization, the `install` subcommand takes the same arguments.
It backs up only the current Desktop ELF with a SHA-256 manifest under
`/home/orionx/.local/state/rowvia-buzz-native-install/desktop-backups/`,
stops Buzz through the pinned launcher, atomically replaces Desktop, verifies
its hash, and restarts Buzz. A stop, copy, or start failure triggers a second
stop, verified Desktop restoration, and restart. No sidecar is copied.

The successful install prints an exact `backup-...` directory. To restore that
immediate pre-install Desktop after validating the backup and unchanged
sidecars, run the `rollback` subcommand with `--backup` set to that directory,
plus the same `--sidecars` and `--pin`. Rollback makes an inverse Desktop backup
before switching; it does not delete either backup. A power loss or hard kill
can interrupt automatic recovery, so retain the backup and use the explicit
rollback command after checking service state. The older
`backups/backup-c5rfxajw` belongs to a different four-binary switch and is not
the rollback target for this update.

Focused synthetic checks, which do not touch live paths or services:

```bash
/home/orionx/project/RowviaContext/repos/rowvia-context-shared-session/scripts/run-capped-check \
  --cwd /home/orionx/project/RowviaContext/repos/buzz-upgrade-v0.5.27 -- \
  python3 integration/rowvia/native-management/test_install_desktop_only.py
/home/orionx/project/RowviaContext/repos/rowvia-context-shared-session/scripts/run-capped-check \
  --cwd /home/orionx/project/RowviaContext/repos/buzz-upgrade-v0.5.27 -- \
  python3 -m py_compile integration/rowvia/native-management/install_desktop_only.py \
  integration/rowvia/native-management/test_install_desktop_only.py
```

A passing preflight and mock test do not prove a live GUI, owner challenge,
relay publication, external kind-0 profile, or Desktop/Mobile conversation.
Observe those behaviors after an authorized restart before accepting the lane.
