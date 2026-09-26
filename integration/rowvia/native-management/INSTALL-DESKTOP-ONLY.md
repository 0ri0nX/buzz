# Guarded Desktop-only enrollment update

`install_desktop_only.py` is a separate one-ELF switch for the external-agent
enrollment change. It leaves the installed `buzz` CLI, both ACP binaries, their
link, wrappers, bridge configs, profiles, and keys in place. The existing
`install_native.py` four-binary pin and installer are unchanged.

Do not use this path until the enrollment source is committed and independently
reviewed. `build-linux-desktop.sh` archives an exact commit; dirty source files
cannot enter its output. Build a **live** Desktop ELF from the new full commit
in an absent, private directory. The build records the commit, image ID, ELF
SHA-256, canonical Tauri identity, and host loader result in `provenance.json`.
Run the heavy build through the shared workstation cap:

```bash
/home/orionx/project/RowviaContext/repos/rowvia-context/scripts/run-capped-check \
  --cwd /home/orionx/project/RowviaContext/repos/buzz -- \
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
python3 integration/rowvia/native-management/test_install_desktop_only.py
python3 -m py_compile integration/rowvia/native-management/install_desktop_only.py
```

A passing preflight and mock test do not prove a live GUI, owner challenge,
relay publication, external kind-0 profile, or Desktop/Mobile conversation.
Observe those behaviors after an authorized restart before accepting the lane.
