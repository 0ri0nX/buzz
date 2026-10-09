# Isolated Linux Buzz sidecar build

`build-linux-sidecars.sh` requires an explicit full `--revision`. Build
all six sidecars from the same reviewed committed application source as
Desktop. For the stable 0.5.27 port that source is
`765e7fd5314e731c4b4a73f230989d1f599e0c11`; a later tooling-only commit does
not require changing the source archived for either artifact set.

The builder retains the pinned local image
`sha256:b30f21ab265a10892f7a77273440f6eeee7996f0108cf7df2e622965b435d77c`.
It exports with `git archive`, uses locked Cargo dependencies, and never
changes the checkout, installs packages, builds an image, launches a binary,
or changes live services. Fetches of official sources named by the committed
Cargo lockfile are permitted; target-cache reuse does not imply download
caches are warm. No host Cargo download cache is mounted.

Run through the shared workstation cap, serially with the Desktop build:

```bash
/home/orionx/project/RowviaContext/repos/rowvia-context-shared-session/scripts/run-capped-check \
  --cwd /home/orionx/project/RowviaContext/repos/buzz-upgrade-v0.5.27 -- \
  bash integration/rowvia/native-management/build-linux-sidecars.sh \
  --revision 765e7fd5314e731c4b4a73f230989d1f599e0c11 \
  --cache-dir /home/orionx/rowvia-buzz-owner-build-cache \
  --output /home/orionx/rowvia-buzz-sidecars-v0.5.27-20261009
```

Add `--dry-run` to validate inputs without exporting or building. Output must
be absent and outside the source repository. Cache is optional; when supplied,
it uses the Desktop builder's managed-cache marker and the same nonblocking
`.lock`. Only `cargo-target` is mounted; it survives success or failure.
Unmarked, linked, foreign-owned, non-private or unrelated cache contents and
overlapping paths are refused. Never bypass either the shared cap or cache lock.

Docker receives 5 GiB RAM and 5 GiB RAM-plus-swap, which means zero swap,
two CPUs, one Cargo job, and 512 PIDs. The six-hour timeout and 25 GiB monitor
cover both disposable scratch and retained target cache. An initial over-budget
cache fails before Docker launch; measurement failure also fails closed.
There is no eviction or cleanup of retained cache.

Output contains `buzz`, `buzz-acp`, `buzz-agent`, `buzz-backend-kubernetes`,
`buzz-dev-mcp`, `git-credential-nostr`, and `provenance.json`: exact source
and archive hashes, pinned image, binary hashes, patch markers and host
`ldd -r` checks. CLI requires `draft-connector`; ACP requires
`buzz.trusted-turn-context/v1`. The four helpers also pass ELF and host loader
checks. Build records do not replace reviewed artifact
acceptance or demonstrate a live service contract.

Focused checks use synthetic containers and private temporary artifacts:

```bash
bash -n integration/rowvia/native-management/build-linux-sidecars.sh
bash integration/rowvia/native-management/test-build-linux-sidecars.sh
```
