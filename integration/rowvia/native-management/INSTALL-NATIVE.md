# Rowvia Buzz coherent native upgrade and rollback

This lane updates Desktop, CLI, both selected ACP copies and all four bundled
helpers from one reviewed committed source. Relay, relay database,
TLS/community configuration, mobile
protocol and Paseo remain outside this lane. `install_desktop_only.py` retains
its separate legacy contract; it is insufficient for this coherent upgrade.

Current baseline is the explicit Oct6 native root:
`/home/orionx/rowvia-buzz-native-startup-live-20261006/native-root`.
Its seven `usr/bin` ELFs are backed up but remain
in place during the upgrade. The fourth baseline target is
`/home/orionx/.local/state/rowvia-buzz-mvp/ttc-build-0dd1323d/artifacts/buzz-acp`,
selected by the existing Desktop-managed and MailReader bridge configs.
The CLI symlink currently selects Oct6's `usr/bin/buzz`.

Build Desktop with the unchanged `build-linux-desktop.sh`,
`--live --owner-test-hook --revision FULL_COMMIT --cache-dir PRIVATE_CACHE`;
build sidecars with the same exact source and image (see BUILD-LINUX-SIDECARS).
The unchanged Desktop builder invokes Tauri without an explicit Cargo
`--locked`; check the archived root/Desktop Cargo lockfiles and pnpm lockfile
against the selected commit after dependency resolution. The sidecar builder
does invoke Cargo with `--locked`. Record actual lock checks with build evidence.
Compile-check, unaccepted hashes, mismatched provenance, or a build without the
owner hook are refused. Both outputs and the new root's existing parent must
be private UID-1000-owned directories under `/home/orionx`. The new native root
must be absent and distinct from baseline.

Create a separately reviewed, uncommitted JSON pin in a mode-0700 directory;
the pin itself must be a regular mode-0600 file. Its complete schema is:

```json
{
  "version": 2,
  "source_commit": "REVIEWED_FULL_COMMIT_SHA",
  "image_id": "sha256:b30f21ab265a10892f7a77273440f6eeee7996f0108cf7df2e622965b435d77c",
  "native_root": "/home/orionx/PRIVATE_UPGRADE_DIRECTORY/native-root",
  "baseline_native_root": "/home/orionx/rowvia-buzz-native-startup-live-20261006/native-root",
  "artifact_sha256": ["NEW_DESKTOP_SHA", "NEW_CLI_SHA", "NEW_ACP_SHA"],
  "baseline_sha256": ["OLD_DESKTOP_SHA", "OLD_CLI_SHA", "OLD_ROOT_ACP_SHA", "OLD_CUSTOM_ACP_SHA"],
  "helper_sha256": ["NEW_AGENT_SHA", "NEW_KUBERNETES_SHA", "NEW_DEV_MCP_SHA", "NEW_GIT_CREDENTIAL_SHA"],
  "baseline_helper_sha256": ["OLD_AGENT_SHA", "OLD_KUBERNETES_SHA", "OLD_DEV_MCP_SHA", "OLD_GIT_CREDENTIAL_SHA"],
  "launcher_sha256": "1284db2a314c6bbae0817f1ab9014618203b8fc1995a3e2ac8415ad1b514c762",
  "wrapper_sha256": ["DESKTOP_WRAPPER_SHA", "MAILREADER_WRAPPER_SHA"],
  "bridge_config_sha256": ["DESKTOP_BRIDGE_CONFIG_SHA", "MAILREADER_BRIDGE_CONFIG_SHA"],
  "state_scope": "rowvia-desktop-cold-v1"
}
```

Replace placeholders only from reviewed source/build and actual selected-target
metadata. Arrays have the displayed fixed order. Provenance is a build record,
not cryptographic source attestation. No legacy 55f1d59 source or default
`.cache/buzz-native-0.5.23` Desktop path is inferred by this upgrade CLI.

```bash
python3 integration/rowvia/native-management/install_native.py preflight \
  --desktop /absolute/live-output --sidecars /absolute/sidecar-output --pin /absolute/pin.json

# Only in the authorized deployment window:
python3 integration/rowvia/native-management/install_native.py install \
  --desktop /absolute/live-output --sidecars /absolute/sidecar-output --pin /absolute/pin.json
```

Preflight verifies accepted hashes, live Tauri identity, owner hook, patch
markers, host loader, baseline binaries, CLI selection, wrappers, selected ACP
configs and the pinned launcher. It creates only the shared private installer
lock. Install stages an isolated root, backs up all eight selected originals with a durable
manifest, then stops Desktop through the already-opened launcher.
Before copying state or switching custom ACP it requires the Desktop unit
inactive/MainPID zero, its cgroup and descendants empty, and no process still
executing the selected Desktop or ACP paths. A reader outside the stopped unit
is a blocker; this installer does not kill unrelated processes.

Cold snapshot scope is fixed: app data under
`~/.local/share/xyz.block.buzz.app` includes agents' durable JSON,
`agents/rowvia-management`, retention DBs with WAL/SHM, `custom_harnesses`,
optional fallback identity files, `localstorage`, TTS/mesh settings, and optional
unread/channel-head SQLite triplets. Separately it includes only
`~/.buzz/archive/archive.db` with WAL/SHM, optional `~/.buzz/AGENTS.md` and
`.nest-agents-version`, and the optional app window-state file.
The launcher can move window state to its own timestamped backup during start/restart;
retain that existing backup as well. No whole profile, agent logs/PIDs,
WebKit caches, models, Codex state, keyring backend or relay DB is copied.
Metadata preflight rejects links, foreign owners/groups, world-writable or
special entries before downtime. Existing owned primary-group modes, including
0664/0775 app state, are preserved inside private snapshot custody.
The snapshot is bounded to 20,000 entries/2 GiB.
Fallback identity bytes and all snapshot content remain opaque and private.

The new root contains Desktop, CLI, ACP, `buzz-agent`,
`buzz-backend-kubernetes`, `buzz-dev-mcp`, and `git-credential-nostr` from the
same accepted source; missing helpers are refused.
Install replaces custom ACP, atomically selects the CLI in the new root, and
starts with explicit `--owner-hook --native-root=ABSOLUTE_NEW_ROOT`.
The canonical Tauri identity and existing shared SecretService/keyring identity
are preserved. The installer neither copies nor restores shared keyring files;
a cold Desktop snapshot is not a SecretService snapshot.

On failure, recovery stops the candidate, proves native readers closed, restores
the verified originals and cold Desktop state, restores the old CLI selection,
then starts with the explicitly recorded old native root and owner hook.
Old-root bundled helpers are rewritten only if their current hash differs from
their verified backup; normally that entire original root stays intact.
Displaced state and both binary roots remain available for inspection.
A hard kill/power loss can require explicit recovery. Use the same pin and the
reported backup:

```bash
python3 integration/rowvia/native-management/install_native.py rollback \
  --backup /home/orionx/.local/state/rowvia-buzz-native-install/backups/backup-EXACT \
  --pin /absolute/pin.json
```

Rollback verifies backup and snapshot custody before stopping, restores only
closed Desktop state, and never silently selects an old default cache.
Observe GUI/identity continuity, owner management/enrollment, TTC, both ACP
selections, and real signed final delivery after root's authorized restart.
Synthetic checks do not establish that live acceptance:

```bash
python3 integration/rowvia/native-management/test_install_native.py
python3 integration/rowvia/native-management/test_install_desktop_only.py
```
