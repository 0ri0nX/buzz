# Isolated Linux Buzz Desktop build

`build-linux-desktop.sh` exports an exact Buzz commit with `git archive`, builds
the Rowvia management desktop in the existing local
`rowvia-buzz-desktop-dev:rust-1.95.0` image, then checks the resulting ELF on
the host. It does not install host packages, build a new Docker image, alter the
source checkout, or replace a running Desktop. The script checks that the tag
resolves to the pinned local image ID before any source export.

```bash
integration/rowvia/native-management/build-linux-desktop.sh \
  --dry-run --output /home/orionx/rowvia-buzz-compile-check

integration/rowvia/native-management/build-linux-desktop.sh \
  --output /home/orionx/rowvia-buzz-compile-check

integration/rowvia/native-management/build-linux-desktop.sh \
  --live --dry-run --output /home/orionx/rowvia-buzz-live

integration/rowvia/native-management/build-linux-desktop.sh \
  --live --output /home/orionx/rowvia-buzz-live
```

The default `compile-check` mode verifies compilation and host ABI only. It
produces a non-executable `buzz-desktop.compile-check` file. Its custom Tauri
identifier, `ai.rowvia.buzz.compile-check`, does **not** isolate runtime state:
Buzz still uses the production `buzz-desktop` keyring and `~/.buzz` nest. Never
launch or install this artifact.

The explicit `--live` mode is the only deployable build. It uses the source Tauri config's canonical
`xyz.block.buzz.app` identifier and `Buzz` product name; it fails if either
source value differs. Tauri derives its app data directory from that
identifier, so the live binary addresses the existing Buzz state when run.
Both modes still write only to the chosen absent output directory during the
build. Neither mode launches, installs, or replaces Desktop. `provenance.json`
records deployability, artifact filename, selected mode, identifier, and
product name alongside the source, image, binary hash, and host loader check.

The output directory must be absent and outside the Buzz source repository.
For a later upstream integration, pass a full commit SHA with `--revision` after
the native management change has been applied and committed there. The script
requires the source commit to contain the management command and its build-time
configuration. It verifies that the Cargo, Tauri, and desktop package versions
agree. The default revision is the current Buzz `HEAD` at invocation time; the
full SHA is printed and recorded in `provenance.json`.

The script reads only `ROWVIA_CONTEXT_BUZZ_OWNER_PUBKEY` and
`ROWVIA_CONTEXT_CERBERUS_PUBKEY` from
`/home/orionx/.local/state/rowvia-management-pilot-v75/public-identities.env`.
Both must be distinct, 64-character hex public keys. It parses the file as
data, rejects unknown fields, and does not print the keys. The fixed build
configuration is `orionx-hive-buzz-desktop` and
`https://buzz.rowvia.ai:8443`. These public values are passed as environment
variables to a transient container, never through image build arguments or
image layers.

The container runs as the invoking host UID/GID with a read-only root
filesystem and Docker logging disabled. Its writable home and tool caches live
under the disposable bind mount. It uses one Cargo job, two CPUs, 4 GiB RAM, a
5 GiB RAM plus swap limit, 512 PIDs, and a 512 MiB `/tmp` tmpfs. A host
monitor stops it if the disposable source/build tree grows beyond 25 GiB.
Hermit, pnpm, Cargo, and
Node caches are directed into that tree so the monitor includes them. If the
monitor cannot measure disk usage, it stops the build. The build has a six-hour
timeout. The temporary tree is deleted on exit; only the verified desktop ELF
and provenance file are retained. Sidecar placeholders satisfy Tauri's build
validation inside that disposable tree; this output is a standalone binary for
the native management MVP, not a full sidecar bundle.

Before publishing the binary, the script checks its ELF architecture, required
embedded public configuration and version string, SHA-256, and `ldd -r` on the host. The latter is
required because the build image has WebKitGTK 2.50.6 while the host has
2.50.3; version numbers alone cannot establish ABI compatibility. A passing
`ldd -r` establishes loader and symbol resolution, not a successful GUI launch.
The build neither launches the Desktop nor changes live Buzz configuration or
state. Keep the live binary stopped until coordinating a deliberate switch
from the installed Buzz Desktop; it shares the canonical app data directory.

Run the narrow script checks before a full build:

```bash
bash -n integration/rowvia/native-management/build-linux-desktop.sh
bash integration/rowvia/native-management/test-build-linux-desktop.sh
```
