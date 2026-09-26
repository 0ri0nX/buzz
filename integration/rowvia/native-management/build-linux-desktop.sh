#!/usr/bin/env bash
# Build the pinned Rowvia Buzz Desktop from an isolated Git archive.
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
repo=$(cd -- "$script_dir/../../.." && pwd -P)
default_identity_file=/home/orionx/.local/state/rowvia-management-pilot-v75/public-identities.env
image_tag=rowvia-buzz-desktop-dev:rust-1.95.0
expected_image_id=sha256:b30f21ab265a10892f7a77273440f6eeee7996f0108cf7df2e622965b435d77c
source_instance=orionx-hive-buzz-desktop
management_origin=https://buzz.rowvia.ai:8443
max_kib=$((25 * 1024 * 1024))
identity_file=$default_identity_file
revision=$(git -C "$repo" rev-parse HEAD)
output=
dry_run=0
build_mode=compile-check

usage() {
  echo "Usage: $0 --output ABSENT_DIRECTORY [--live] [--revision FULL_COMMIT] [--identity-file FILE] [--dry-run]"
}
die() { echo "error: $*" >&2; exit 1; }

while (($#)); do
  case $1 in
    --output|--revision|--identity-file)
      (($# >= 2)) || die "$1 requires a value"
      case $1 in
        --output) output=$2 ;;
        --revision) revision=$2 ;;
        --identity-file) identity_file=$2 ;;
      esac
      shift 2 ;;
    --dry-run) dry_run=1; shift ;;
    --live)
      [[ $build_mode = compile-check ]] || die "--live was specified more than once"
      build_mode=live
      shift ;;
    --help|-h) usage; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
done

[[ -n $output && $output = /* ]] || die "--output must be an absolute directory path"
[[ ! -e $output && ! -L $output ]] || die "output path already exists"
output_parent=$(cd -- "$(dirname -- "$output")" && pwd -P) || die "output parent does not exist"
[[ -w $output_parent ]] || die "output parent is not writable"
output=$output_parent/$(basename -- "$output")
[[ $output != "$repo"/* ]] || die "output must be outside the source repository"
[[ $revision =~ ^[0-9a-f]{40}$ ]] || die "revision must be a full lowercase commit SHA"
[[ $(git -C "$repo" rev-parse "${revision}^{commit}") = "$revision" ]] || die "revision is not a commit"
git -C "$repo" cat-file -e "$revision:desktop/src-tauri/src/commands/rowvia_management.rs" || die "revision lacks Rowvia management"
git -C "$repo" cat-file -e "$revision:desktop/src-tauri/build.rs" || die "revision lacks build-time configuration"
for marker in BUZZ_BUILD_ROWVIA_MANAGEMENT_ORIGIN BUZZ_BUILD_ROWVIA_SOURCE_INSTANCE BUZZ_BUILD_ROWVIA_OWNER_PUBKEY BUZZ_BUILD_CERBERUS_PUBKEY; do
  git -C "$repo" grep -Fq "$marker" "$revision" -- desktop/src-tauri/build.rs || die "revision lacks $marker build configuration"
done
[[ -r $identity_file && -f $identity_file ]] || die "public identity file is unreadable"

# Parse data as data. Sourcing this file would execute arbitrary shell code.
owner_key=
cerberus_key=
while IFS= read -r line || [[ -n $line ]]; do
  [[ $line =~ ^[[:space:]]*(#|$) ]] && continue
  [[ $line =~ ^([A-Z_]+)=(.*)$ ]] || die "invalid public identity file syntax"
  case ${BASH_REMATCH[1]} in
    ROWVIA_CONTEXT_BUZZ_OWNER_PUBKEY)
      [[ -z $owner_key ]] || die "duplicate owner public key"
      owner_key=${BASH_REMATCH[2],,} ;;
    ROWVIA_CONTEXT_CERBERUS_PUBKEY)
      [[ -z $cerberus_key ]] || die "duplicate Cerberus public key"
      cerberus_key=${BASH_REMATCH[2],,} ;;
    PILOT_TEST_AGENT_PUBKEY) : ;;
    *) die "unexpected identity field" ;;
  esac
done < "$identity_file"
[[ $owner_key =~ ^[0-9a-f]{64}$ ]] || die "owner public key must be 64 hex characters"
[[ $cerberus_key =~ ^[0-9a-f]{64}$ ]] || die "Cerberus public key must be 64 hex characters"
[[ $owner_key != "$cerberus_key" ]] || die "owner and Cerberus public keys must differ"

for command in docker git tar python3 sha256sum ldd readelf strings timeout du; do
  command -v "$command" >/dev/null || die "missing command: $command"
done
image_id=$(docker image inspect "$image_tag" --format '{{.Id}}') || die "local build image is missing"
[[ $image_id = "$expected_image_id" ]] || die "local build image does not match the pinned image ID"
[[ $(docker image inspect "$image_id" --format '{{.Architecture}}/{{.Os}}') = amd64/linux ]] || die "build image must be linux/amd64"
[[ $(uname -m) = x86_64 && $(uname -s) = Linux ]] || die "host must be Linux x86_64"

version=$(git -C "$repo" show "$revision:desktop/src-tauri/Cargo.toml" | sed -n '/^\[package\]/,/^\[/{s/^version = "\([^"]*\)"/\1/p;}' | head -1)
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "could not read desktop Cargo package version"
tauri_version=$(git -C "$repo" show "$revision:desktop/src-tauri/tauri.conf.json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["version"])')
package_version=$(git -C "$repo" show "$revision:desktop/package.json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["version"])')
[[ $version = "$tauri_version" && $version = "$package_version" ]] || die "desktop version manifests disagree"
if [[ $build_mode = live ]]; then
  app_identifier=$(git -C "$repo" show "$revision:desktop/src-tauri/tauri.conf.json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["identifier"])')
  product_name=$(git -C "$repo" show "$revision:desktop/src-tauri/tauri.conf.json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["productName"])')
  [[ $app_identifier = xyz.block.buzz.app && $product_name = Buzz ]] || die "live build requires the canonical Buzz identifier and product name"
else
  app_identifier=ai.rowvia.buzz.compile-check
  product_name='Rowvia Buzz Compile Check'
fi

echo "Source: $revision (desktop $version)"
echo "Image: $image_id"
echo "Mode: $build_mode ($app_identifier; $product_name)"
if [[ $build_mode = compile-check ]]; then
  echo "Compile-check only: artifact is non-executable and has no runtime state isolation"
fi
echo "Output: $output"
echo "Identity fields: owner and Cerberus public keys validated"
echo "Limits: 4 GiB RAM, 5 GiB RAM+swap, 2 CPUs, 1 build job, 512 PIDs, 25 GiB scratch"
if ((dry_run)); then
  echo "Dry run: source export and Docker build skipped"
  exit 0
fi

scratch=$(mktemp -d /home/orionx/rowvia-buzz-build.XXXXXXXX)
container=rowvia-buzz-build-$(basename -- "$scratch" | tr -cd 'a-zA-Z0-9')
staged_output=
monitor_pid=
container_pid=
watchdog_failure=$scratch/watchdog.failed
measure_scratch_kib() {
  local usage_line usage_kib
  usage_line=$(du -sk "$scratch") || return 1
  [[ $usage_line = *$'\t'* ]] || return 1
  usage_kib=${usage_line%%$'\t'*}
  [[ $usage_kib =~ ^[0-9]+$ ]] || return 1
  printf '%s\n' "$usage_kib"
}
cleanup() {
  result=$?
  trap - EXIT
  if [[ -n $monitor_pid ]]; then
    kill "$monitor_pid" 2>/dev/null || true
    wait "$monitor_pid" 2>/dev/null || true
  fi
  timeout 15s docker stop --time 10 "$container" >/dev/null 2>&1 ||
    timeout 10s docker kill "$container" >/dev/null 2>&1 || true
  if [[ -n $container_pid ]]; then
    kill "$container_pid" 2>/dev/null || true
    wait "$container_pid" 2>/dev/null || true
  fi
  if [[ -n $staged_output ]] && ! rm -rf -- "$staged_output"; then result=1; fi
  # Hermit installs read-only package directories; restore owner traversal before
  # deleting this exact disposable build tree. `find` does not follow symlinks.
  if ! find "$scratch" -type d -exec chmod u+rwx -- {} + ||
     ! find "$scratch" -depth -delete; then
    echo "error: could not remove isolated build scratch: $scratch" >&2
    result=1
  fi
  exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -- "$scratch/source"
git -C "$repo" archive --format=tar "$revision" | tar -xf - -C "$scratch/source"
initial_kib=$(measure_scratch_kib) || die "cannot measure source archive size"
((initial_kib <= max_kib)) || die "source archive exceeds scratch limit"
mkdir -- "$scratch/source/.home"

# The only accepted build inputs are the fixed values and two public identities.
export BUZZ_BUILD_ROWVIA_MANAGEMENT_ORIGIN=$management_origin
export BUZZ_BUILD_ROWVIA_SOURCE_INSTANCE=$source_instance
export BUZZ_BUILD_ROWVIA_OWNER_PUBKEY=$owner_key
export BUZZ_BUILD_CERBERUS_PUBKEY=$cerberus_key
unset owner_key cerberus_key

timeout --signal=TERM --kill-after=30s 6h docker run --rm \
  --name "$container" --network bridge --cpus=2 --memory=4g --memory-swap=5g \
  --pids-limit=512 --cap-drop=ALL --security-opt=no-new-privileges \
  --user "$(id -u):$(id -g)" --read-only --log-driver=none \
  --tmpfs /tmp:rw,nosuid,nodev,size=512m,mode=1777 \
  --mount "type=bind,src=$scratch/source,dst=/work/source" \
  --workdir /work/source \
  --env BUZZ_BUILD_ROWVIA_MANAGEMENT_ORIGIN \
  --env BUZZ_BUILD_ROWVIA_SOURCE_INSTANCE \
  --env BUZZ_BUILD_ROWVIA_OWNER_PUBKEY \
  --env BUZZ_BUILD_CERBERUS_PUBKEY \
  --env ROWVIA_BUILD_MODE="$build_mode" \
  --env CARGO_BUILD_JOBS=1 --env CARGO_INCREMENTAL=0 \
  --env HOME=/work/source/.home \
  --env CARGO_HOME=/work/source/.cargo-home \
  --env HERMIT_STATE_DIR=/work/source/.hermit-state \
  --env XDG_CACHE_HOME=/work/source/.cache \
  --env XDG_DATA_HOME=/work/source/.local/share \
  --env NPM_CONFIG_CACHE=/work/source/.npm-cache \
  "$image_id" bash -euo pipefail -c '
    . ./bin/activate-hermit
    test "$(rustc --version)" = "rustc 1.95.0 (59807616e 2026-04-14)"
    pnpm install --frozen-lockfile --store-dir /work/source/.pnpm-store
    target=$(rustc -vV | sed -n "s/^host: //p")
    test "$target" = x86_64-unknown-linux-gnu
    mkdir -p desktop/src-tauri/binaries
    for name in buzz-acp buzz-agent buzz-backend-kubernetes buzz-dev-mcp git-credential-nostr buzz; do
      : > "desktop/src-tauri/binaries/$name-$target"
    done
    if [[ $ROWVIA_BUILD_MODE = live ]]; then
      pnpm -C desktop tauri build --no-bundle
    elif [[ $ROWVIA_BUILD_MODE = compile-check ]]; then
      pnpm -C desktop tauri build --no-bundle --config "{\"identifier\":\"ai.rowvia.buzz.compile-check\",\"productName\":\"Rowvia Buzz Compile Check\"}"
    else
      echo "invalid Rowvia build mode" >&2
      exit 1
    fi
    test -s desktop/src-tauri/target/release/buzz-desktop
  ' &
container_pid=$!

(
  while sleep 30; do
    if ! used_kib=$(measure_scratch_kib); then
      reason="cannot measure scratch usage"
    elif ((used_kib > max_kib)); then
      reason="scratch exceeded 25 GiB"
    else
      continue
    fi
    echo "error: $reason; stopping build" >&2
    printf '%s\n' "$reason" > "$watchdog_failure" || true
    timeout 15s docker stop --time 10 "$container" >/dev/null 2>&1 ||
      timeout 10s docker kill "$container" >/dev/null 2>&1 || true
    kill "$container_pid" 2>/dev/null || true
    exit 1
  done
) &
monitor_pid=$!

if ! wait "$container_pid"; then die "container build failed"; fi
container_pid=
if ! kill -0 "$monitor_pid" 2>/dev/null; then
  wait "$monitor_pid" || die "build watchdog exited unexpectedly"
else
  kill "$monitor_pid" 2>/dev/null || true
  wait "$monitor_pid" 2>/dev/null || true
fi
monitor_pid=
[[ ! -e $watchdog_failure ]] || die "build watchdog stopped the container"
final_kib=$(measure_scratch_kib) || die "cannot measure final scratch usage"
((final_kib <= max_kib)) || die "final scratch exceeds 25 GiB"

binary=$scratch/source/desktop/src-tauri/target/release/buzz-desktop
[[ -s $binary ]] || die "build did not produce a desktop binary"
[[ $(readelf -h "$binary" | sed -n 's/^[[:space:]]*Machine:[[:space:]]*//p') = 'Advanced Micro Devices X86-64' ]] || die "unexpected ELF architecture"
for embedded in "$management_origin" "$source_instance" "$BUZZ_BUILD_ROWVIA_OWNER_PUBKEY" "$BUZZ_BUILD_CERBERUS_PUBKEY"; do
  strings -a "$binary" | grep -F -- "$embedded" >/dev/null || die "required build configuration is missing from binary"
done
# Cargo, Tauri and package manifests were required to agree before compilation.
# Optimized Tauri ELF output does not necessarily retain a literal version string.
strings -a "$binary" | grep -F -- "$app_identifier" >/dev/null || die "Tauri identifier is missing from binary"
unset BUZZ_BUILD_ROWVIA_OWNER_PUBKEY BUZZ_BUILD_CERBERUS_PUBKEY
ldd_report=$(ldd -r "$binary" 2>&1) || { echo "$ldd_report" >&2; die "host loader cannot resolve desktop binary"; }
if [[ $ldd_report = *'not found'* || $ldd_report = *'undefined symbol'* ]]; then
  echo "$ldd_report" >&2
  die "host desktop libraries are incompatible"
fi

staged_output=$(mktemp -d "$output_parent/.rowvia-buzz-output.XXXXXXXX")
if [[ $build_mode = live ]]; then
  artifact_name=buzz-desktop
  artifact_mode=755
else
  artifact_name=buzz-desktop.compile-check
  artifact_mode=644
fi
install -m "$artifact_mode" "$binary" "$staged_output/$artifact_name"
binary_sha=$(sha256sum "$staged_output/$artifact_name" | cut -d' ' -f1)
python3 - "$staged_output/provenance.json" "$revision" "$version" "$image_id" "$binary_sha" "$build_mode" "$app_identifier" "$product_name" "$artifact_name" <<'PY'
import json
import sys

path, revision, version, image_id, binary_sha, build_mode, identifier, product_name, artifact_name = sys.argv[1:]
with open(path, "w", encoding="utf-8") as output_file:
    json.dump(
        {
            "source_commit": revision,
            "desktop_version": version,
            "image_id": image_id,
            "binary_sha256": binary_sha,
            "build_mode": build_mode,
            "deployable": build_mode == "live",
            "artifact_filename": artifact_name,
            "tauri_identifier": identifier,
            "product_name": product_name,
            "source_instance": "orionx-hive-buzz-desktop",
            "management_origin": "https://buzz.rowvia.ai:8443",
            "host_ldd_r": "pass",
            "bundle": "none",
            "sidecars": "placeholders in disposable build tree only",
        },
        output_file,
        indent=2,
    )
    output_file.write("\n")
PY
[[ ! -e $output && ! -L $output ]] || die "output path appeared during build"
mv -- "$staged_output" "$output"
staged_output=
echo "Verified artifact: $output/$artifact_name"
echo "SHA-256: $binary_sha"
echo "Host ldd -r: pass"
