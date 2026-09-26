# Rowvia Buzz native install and rollback

`install_native.py` switches four host binaries to the pinned Rowvia build. It is
specific to UID 1000 on this host. Its fixed targets are:

- `/home/orionx/.cache/buzz-native-0.5.23/usr/bin/buzz-desktop`
- `/home/orionx/.cache/buzz-native-0.5.23/usr/bin/buzz`
- `/home/orionx/.cache/buzz-native-0.5.23/usr/bin/buzz-acp`
- `/home/orionx/.local/state/rowvia-buzz-mvp/ttc-build-0dd1323d/artifacts/buzz-acp`

The last target is selected by the Desktop-managed and MailReader bridge TOML
configs used by `acp-proxy-desktop.sh` and `start-mail-reader.sh`. The cached
ACP is a separate target. The installer checks both configs, both wrappers,
and the CLI symlink before switching. It does not edit them or any Buzz
profile, keyring, or management identity file.
The project directory containing `scripts/buzz-desktop` is group writable on
this host. The installer pins that launcher's SHA-256 and executes an already
opened file descriptor, so a path replacement after validation cannot change
which launcher runs. Any deliberate launcher edit requires reviewing and
updating the pinned digest in the installer.

Create a Desktop output using `build-linux-desktop.sh --live` and a sidecar
output using `build-linux-sidecars.sh`. Both outputs must come from commit
`55f1d59a558ae16be687648179ff174bf6e55792` and image
`sha256:b30f21ab265a10892f7a77273440f6eeee7996f0108cf7df2e622965b435d77c`.
The Desktop provenance must say `build_mode: live`, `deployable: true`, and
`tauri_identifier: xyz.block.buzz.app`. Compile-check outputs are refused.
Keep both output directories below `/home/orionx`, owned by UID 1000 with mode
`0700`. All path ancestors from that home onward must be real directories,
owned by UID 1000 and not writable by group or others. The installer trusts
those operator-controlled outputs and their provenance; their embedded commit
and image fields are build records, not cryptographic source attestation.

From the Buzz repository, first run the artifact and target check. It does not
change live binaries or services, but it creates a private installer lock file:

```bash
python3 integration/rowvia/native-management/install_native.py preflight \
  --desktop /absolute/path/to/live-desktop-output \
  --sidecars /absolute/path/to/sidecar-output
```

After reviewing the output and arranging a downtime window, run:

```bash
python3 integration/rowvia/native-management/install_native.py install \
  --desktop /absolute/path/to/live-desktop-output \
  --sidecars /absolute/path/to/sidecar-output
```

The command revalidates provenance, SHA-256, embedded patch markers, owner,
mode, paths, and `ldd -r`. It writes a private backup and SHA manifest below
`/home/orionx/.local/state/rowvia-buzz-native-install/backups/` and syncs the
backup payloads, manifest, and containing directories to disk. It then stops
the transient Desktop unit through `scripts/buzz-desktop stop`, replaces each
binary atomically, verifies installed hashes, and calls
`scripts/buzz-desktop start`. The launcher verifies the unit and renderer.
If switching or restart fails, the command stops the unit, restores all four
original binaries, and starts the original Desktop. It prints the backup path
on success. A process kill or host power loss can interrupt automatic recovery;
the backup is retained for an explicit rollback:

```bash
python3 integration/rowvia/native-management/install_native.py rollback \
  --backup /home/orionx/.local/state/rowvia-buzz-native-install/backups/backup-EXACT-NAME
```

Rollback validates the full backup manifest and all four hashes before
stopping the service. Backups are intentionally retained; the installer never
deletes prior binaries, backups, app data, or keyring entries. Do not manually
alter the backup or its manifest. The launcher itself backs up window state and
may clear an unowned stale socket during `start`; this is existing launcher
behavior, not an installer file operation.

The default verification is synthetic and does not address live files:

```bash
python3 integration/rowvia/native-management/test_install_native.py
python3 -m py_compile integration/rowvia/native-management/install_native.py
```

The mock test checks live versus compile-check refusal, successful replacement,
explicit rollback, and automatic rollback after a failed restart. A real
Desktop start, keyring continuity, and the two custom wrapper processes require
observation during the authorized deployment window.
