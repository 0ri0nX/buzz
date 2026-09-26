#!/usr/bin/env bash
# Build the Rowvia Buzz CLI and ACP sidecars from the Desktop's exact source commit.
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
repo=$(cd -- "$script_dir/../../.." && pwd -P)
revision=55f1d59a558ae16be687648179ff174bf6e55792
image_tag=rowvia-buzz-desktop-dev:rust-1.95.0
expected_image_id=sha256:b30f21ab265a10892f7a77273440f6eeee7996f0108cf7df2e622965b435d77c
max_kib=$((25 * 1024 * 1024))
output=
dry_run=0

die() { echo "error: $*" >&2; exit 1; }
usage() { echo "Usage: $0 --output ABSENT_DIRECTORY [--revision FULL_COMMIT] [--dry-run]"; }

while (($#)); do
  case $1 in
    --output|--revision)
      (($# >= 2)) || die "$1 requires a value"
      if [[ $1 = --output ]]; then output=$2; else
        [[ $2 = "$revision" ]] || die "revision differs from the pinned Desktop source commit"
      fi
      shift 2 ;;
    --dry-run) dry_run=1; shift ;;
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
[[ $(git -C "$repo" rev-parse "${revision}^{commit}") = "$revision" ]] || die "pinned revision is unavailable"
for marker in draft-connector; do
  git -C "$repo" grep -Fq "$marker" "$revision" -- crates/buzz-cli/src/lib.rs || die "revision lacks CLI $marker marker"
done
git -C "$repo" grep -Fq 'buzz.trusted-turn-context/v1' "$revision" -- crates/buzz-acp/src/trusted_turn_context.rs || die "revision lacks ACP trusted-turn-context marker"
for command in docker git tar python3 sha256sum ldd readelf strings timeout du; do
  command -v "$command" >/dev/null || die "missing command: $command"
done
image_id=$(docker image inspect "$image_tag" --format '{{.Id}}') || die "local build image is missing"
[[ $image_id = "$expected_image_id" ]] || die "local build image does not match the pinned image ID"
[[ $(docker image inspect "$image_id" --format '{{.Architecture}}/{{.Os}}') = amd64/linux ]] || die "build image must be linux/amd64"
[[ $(uname -m) = x86_64 && $(uname -s) = Linux ]] || die "host must be Linux x86_64"

echo "Source: $revision"
echo "Image: $image_id"
echo "Output: $output"
echo "Limits: 4 GiB RAM, 5 GiB RAM+swap, 2 CPUs, 1 Cargo job, 512 PIDs, 25 GiB scratch"
if ((dry_run)); then
  echo "Dry run: source export and Docker build skipped"
  exit 0
fi

scratch=$(mktemp -d /home/orionx/rowvia-buzz-sidecars.XXXXXXXX)
container=rowvia-buzz-sidecars-$(basename -- "$scratch" | tr -cd 'a-zA-Z0-9')
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
  if ! rm -rf -- "$scratch"; then
    echo "error: could not remove isolated build scratch: $scratch" >&2
    result=1
  fi
  exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -- "$scratch/source"
git -C "$repo" archive --format=tar "$revision" > "$scratch/source.tar"
source_archive_sha=$(sha256sum "$scratch/source.tar" | cut -d' ' -f1)
initial_kib=$(measure_scratch_kib) || die "cannot measure source archive size"
((initial_kib <= max_kib)) || die "source archive exceeds scratch limit"
tar -xf "$scratch/source.tar" -C "$scratch/source"
initial_kib=$(measure_scratch_kib) || die "cannot measure extracted source size"
((initial_kib <= max_kib)) || die "extracted source exceeds scratch limit"
mkdir -- "$scratch/source/.home"

timeout --signal=TERM --kill-after=30s 6h docker run --rm \
  --name "$container" --network bridge --cpus=2 --memory=4g --memory-swap=5g \
  --pids-limit=512 --cap-drop=ALL --security-opt=no-new-privileges \
  --user "$(id -u):$(id -g)" --read-only --log-driver=none \
  --tmpfs /tmp:rw,nosuid,nodev,size=512m,mode=1777 \
  --mount "type=bind,src=$scratch/source,dst=/work/source" \
  --workdir /work/source \
  --env CARGO_BUILD_JOBS=1 --env CARGO_INCREMENTAL=0 \
  --env HOME=/work/source/.home \
  --env CARGO_HOME=/work/source/.cargo-home \
  --env HERMIT_STATE_DIR=/work/source/.hermit-state \
  --env XDG_CACHE_HOME=/work/source/.cache \
  --env XDG_DATA_HOME=/work/source/.local/share \
  "$image_id" bash -euo pipefail -c '
    . ./bin/activate-hermit
    test "$(rustc --version)" = "rustc 1.95.0 (59807616e 2026-04-14)"
    test "$(rustc -vV | sed -n "s/^host: //p")" = x86_64-unknown-linux-gnu
    cargo build --release --locked --jobs 1 -p buzz-cli -p buzz-acp --bins
    test -s target/release/buzz
    test -s target/release/buzz-acp
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

for name in buzz buzz-acp; do
  binary=$scratch/source/target/release/$name
  [[ -s $binary ]] || die "build did not produce $name"
  [[ $(readelf -h "$binary" | sed -n 's/^[[:space:]]*Machine:[[:space:]]*//p') = 'Advanced Micro Devices X86-64' ]] || die "unexpected $name ELF architecture"
  [[ $(readelf -h "$binary" | sed -n 's/^[[:space:]]*Type:[[:space:]]*\([^ ]*\).*/\1/p') = @(EXEC|DYN) ]] || die "unexpected $name ELF type"
  if [[ $name = buzz ]]; then marker=draft-connector; else marker=buzz.trusted-turn-context/v1; fi
  strings -a "$binary" | grep -F -- "$marker" >/dev/null || die "$name lacks patch marker: $marker"
  ldd_report=$(ldd -r "$binary" 2>&1) || { echo "$ldd_report" >&2; die "host loader cannot resolve $name"; }
  if [[ $ldd_report = *'not found'* || $ldd_report = *'undefined symbol'* ]]; then
    echo "$ldd_report" >&2
    die "host libraries are incompatible with $name"
  fi
done

staged_output=$(mktemp -d "$output_parent/.rowvia-buzz-sidecars-output.XXXXXXXX")
for name in buzz buzz-acp; do
  install -m 755 "$scratch/source/target/release/$name" "$staged_output/$name"
done
python3 - "$staged_output/provenance.json" "$revision" "$source_archive_sha" "$image_id" \
  "$(sha256sum "$staged_output/buzz" | cut -d' ' -f1)" \
  "$(sha256sum "$staged_output/buzz-acp" | cut -d' ' -f1)" <<'PY'
import json
import sys

path, revision, archive_sha, image_id, cli_sha, acp_sha = sys.argv[1:]
with open(path, "w", encoding="utf-8") as output_file:
    json.dump({
        "source_commit": revision,
        "source_archive_sha256": archive_sha,
        "image_id": image_id,
        "artifacts": {
            "buzz": {"sha256": cli_sha, "patch_marker": "draft-connector", "host_ldd_r": "pass"},
            "buzz-acp": {"sha256": acp_sha, "patch_marker": "buzz.trusted-turn-context/v1", "host_ldd_r": "pass"},
        },
    }, output_file, indent=2)
    output_file.write("\n")
PY
[[ ! -e $output && ! -L $output ]] || die "output path appeared during build"
mv -- "$staged_output" "$output"
staged_output=
echo "Verified artifacts: $output/buzz and $output/buzz-acp"
echo "Host ldd -r: pass for both"
