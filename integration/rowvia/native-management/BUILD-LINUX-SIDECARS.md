# Isolated Linux Buzz sidecar build

`build-linux-sidecars.sh` builds the `buzz` CLI and `buzz-acp` from commit
`55f1d59a558ae16be687648179ff174bf6e55792`, the source selected for the
Rowvia native Desktop build. It accepts no other revision. Re-pinning a later
Desktop requires reviewing and changing the script's source commit and tests
together. The build uses the pinned local Desktop build image ID and an isolated
`git archive`; it does not modify the checkout, install packages, build an
image, launch either binary, or change a running service.

```bash
integration/rowvia/native-management/build-linux-sidecars.sh \
  --dry-run --output /home/orionx/rowvia-buzz-sidecars

integration/rowvia/native-management/build-linux-sidecars.sh \
  --output /home/orionx/rowvia-buzz-sidecars
```

The output path must be absent and outside the Buzz repository. A successful
run publishes only `buzz`, `buzz-acp`, and `provenance.json`. The JSON records
the source commit and archive SHA-256, image ID, binary SHA-256 values, the
required patch marker found in each ELF, and passing host `ldd -r` checks.
The `buzz` marker is `draft-connector`; the ACP marker is
`buzz.trusted-turn-context/v1`. Loader validation establishes symbol
resolution, not that a deployed service is configured correctly.

The container is limited to 4 GiB RAM, 5 GiB RAM plus swap, two CPUs, one
Cargo job, and 512 PIDs. A host watchdog stops the build if the disposable
source and cache tree exceeds 25 GiB or cannot be measured. A six-hour timeout
also applies. The tree is removed on success and failure, leaving no persistent
build cache or log files. The build uses the source's locked Cargo dependencies.

Run the focused checks before a full build:

```bash
bash -n integration/rowvia/native-management/build-linux-sidecars.sh \
  integration/rowvia/native-management/test-build-linux-sidecars.sh
bash integration/rowvia/native-management/test-build-linux-sidecars.sh
```
