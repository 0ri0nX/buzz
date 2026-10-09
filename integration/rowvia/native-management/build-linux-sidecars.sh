#!/usr/bin/env bash
# Build the Rowvia Buzz CLI and ACP sidecars from the Desktop's exact source commit.
set -euo pipefail
umask 077

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
repo=$(cd -- "$script_dir/../../.." && pwd -P)
revision=
cache_dir=
cache_marker=rowvia-desktop-cargo-cache-v1
image_tag=rowvia-buzz-desktop-dev:rust-1.95.0
expected_image_id=sha256:b30f21ab265a10892f7a77273440f6eeee7996f0108cf7df2e622965b435d77c
max_kib=$((25 * 1024 * 1024))
output=
dry_run=0

die() { echo "error: $*" >&2; exit 1; }
usage() { echo "Usage: $0 --output ABSENT_DIRECTORY --revision FULL_COMMIT [--cache-dir PRIVATE_DIRECTORY] [--dry-run]"; }

while (($#)); do
  case $1 in
    --output|--revision|--cache-dir)
      (($# >= 2)) || die "$1 requires a value"
      case $1 in
        --output) output=$2 ;;
        --revision) revision=$2 ;;
        --cache-dir) [[ -z $cache_dir && -n $2 ]] || die "cache may be specified once"; cache_dir=$2 ;;
      esac
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
[[ $revision =~ ^[0-9a-f]{40}$ ]] || die "--revision requires a full lowercase commit SHA"
[[ $(git -C "$repo" rev-parse "${revision}^{commit}") = "$revision" ]] || die "pinned revision is unavailable"
if [[ -n $cache_dir ]]; then
  [[ $cache_dir = /* && $cache_dir != *','* && $cache_dir != *$'\n'* && ! -L $cache_dir ]] || die "unsafe cache path"
  [[ $(basename -- "$cache_dir") != . && $(basename -- "$cache_dir") != .. ]] || die "cache must name a directory"
  cache_parent=$(cd -- "$(dirname -- "$cache_dir")" && pwd -P) || die "cache parent is absent"
  cache_dir=$cache_parent/$(basename -- "$cache_dir")
  [[ $cache_dir != "$repo" && $cache_dir != "$repo"/* && $repo != "$cache_dir"/*
    && $cache_dir != "$output" && $cache_dir != "$output"/* && $output != "$cache_dir"/* ]] || die "cache overlaps source or output"
  if [[ -e $cache_dir ]]; then
    [[ -d $cache_dir && $(stat -c %u "$cache_dir") = "$(id -u)" && $(stat -c %a "$cache_dir") = 700 ]] || die "cache must be owned and mode 700"
    [[ -f $cache_dir/.rowvia-build-cache && ! -L $cache_dir/.rowvia-build-cache && $(<"$cache_dir/.rowvia-build-cache") = "$cache_marker" ]] || die "unmanaged cache"
    while IFS= read -r -d '' entry; do
      case ${entry##*/} in
        .rowvia-build-cache|.lock) [[ -f $entry && ! -L $entry ]] || die "unsafe cache metadata" ;;
        cargo-target) [[ -d $entry && ! -L $entry ]] || die "unsafe cargo-target" ;;
        *) die "unrelated cache contents" ;;
      esac
    done < <(find "$cache_dir" -mindepth 1 -maxdepth 1 -print0)
  fi
fi
for marker in draft-connector; do
  git -C "$repo" grep -Fq "$marker" "$revision" -- crates/buzz-cli/src/lib.rs || die "revision lacks CLI $marker marker"
done
git -C "$repo" grep -Fq 'buzz.trusted-turn-context/v1' "$revision" -- crates/buzz-acp/src/trusted_turn_context.rs || die "revision lacks ACP trusted-turn-context marker"
for command in docker git tar python3 sha256sum ldd readelf strings timeout du flock; do
  command -v "$command" >/dev/null || die "missing command: $command"
done
image_id=$(docker image inspect "$image_tag" --format '{{.Id}}') || die "local build image is missing"
[[ $image_id = "$expected_image_id" ]] || die "local build image does not match the pinned image ID"
[[ $(docker image inspect "$image_id" --format '{{.Architecture}}/{{.Os}}') = amd64/linux ]] || die "build image must be linux/amd64"
[[ $(uname -m) = x86_64 && $(uname -s) = Linux ]] || die "host must be Linux x86_64"

echo "Source: $revision"
echo "Image: $image_id"
echo "Output: $output"
echo "Limits: 5 GiB RAM, zero swap, 2 CPUs, 1 Cargo job, 512 PIDs, 25 GiB scratch"
if ((dry_run)); then
  echo "Dry run: source export and Docker build skipped"
  exit 0
fi

if [[ -n $cache_dir ]]; then
  if [[ ! -e $cache_dir ]]; then
    mkdir -m 700 -- "$cache_dir"
    printf '%s\n' "$cache_marker" > "$cache_dir/.rowvia-build-cache"
  fi
  exec {cache_lock_fd}>> "$cache_dir/.lock"
  flock -n "$cache_lock_fd" || die "build cache is already in use"
  mkdir -p -- "$cache_dir/cargo-target"
fi

scratch=$(mktemp -d /home/orionx/rowvia-buzz-sidecars.XXXXXXXX)
container=rowvia-buzz-sidecars-$(basename -- "$scratch" | tr -cd 'a-zA-Z0-9')
staged_output=
monitor_pid=
container_pid=
watchdog_failure=$scratch/watchdog.failed
measure_scratch_kib() {
  local usage_line usage_kib path total_kib=0 attempt measured
  local paths=("$scratch")
  if [[ -n $cache_dir ]]; then paths+=("$cache_dir"); fi
  for path in "${paths[@]}"; do
    measured=0
    for attempt in 1 2 3; do
      if usage_line=$(du -sk "$path"); then measured=1; break; fi
      if ((attempt < 3)); then sleep 1; fi
    done
    ((measured)) || return 1
    [[ $usage_line = *$'\t'* ]] || return 1
    usage_kib=${usage_line%%$'\t'*}
    [[ $usage_kib =~ ^[0-9]+$ ]] || return 1
    total_kib=$((total_kib + usage_kib))
  done
  printf '%s\n' "$total_kib"
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
git -C "$repo" archive --format=tar "$revision" > "$scratch/source.tar"
source_archive_sha=$(sha256sum "$scratch/source.tar" | cut -d' ' -f1)
initial_kib=$(measure_scratch_kib) || die "cannot measure source archive size"
((initial_kib <= max_kib)) || die "source archive exceeds scratch limit"
tar -xf "$scratch/source.tar" -C "$scratch/source"
initial_kib=$(measure_scratch_kib) || die "cannot measure extracted source size"
((initial_kib <= max_kib)) || die "extracted source exceeds scratch limit"
mkdir -- "$scratch/source/target"
extra_build_args=()
if [[ -n $cache_dir ]]; then
  extra_build_args+=(--mount "type=bind,src=$cache_dir/cargo-target,dst=/work/source/target")
fi

timeout --signal=TERM --kill-after=30s 6h docker run --rm \
  --name "$container" --network bridge --cpus=2 --memory=5g --memory-swap=5g \
  --pids-limit=512 --cap-drop=ALL --security-opt=no-new-privileges \
  --user "$(id -u):$(id -g)" --read-only --log-driver=none \
  --tmpfs /tmp:rw,nosuid,nodev,size=512m,mode=1777 \
  --mount "type=bind,src=$scratch/source,dst=/work/source" \
  --workdir /work/source \
  --env CARGO_BUILD_JOBS=1 --env CARGO_INCREMENTAL=0 \
  --env CARGO_HOME=/work/source/.cargo-home \
  --env HERMIT_STATE_DIR=/work/source/.hermit-state \
  --env HERMIT_BIN_INSTALL_DIR=/work/source/.hermit-bin \
  --env XDG_CACHE_HOME=/work/source/.cache \
  --env XDG_DATA_HOME=/work/source/.local/share \
  "${extra_build_args[@]}" "$image_id" bash -euo pipefail -c '
    . ./bin/activate-hermit
    test "$(rustc --version)" = "rustc 1.95.0 (59807616e 2026-04-14)"
    test "$(rustc -vV | sed -n "s/^host: //p")" = x86_64-unknown-linux-gnu
    cargo build --release --locked --jobs 1 -p buzz-cli -p buzz-acp -p buzz-agent -p buzz-backend-kubernetes -p buzz-dev-mcp -p git-credential-nostr --bins
    for name in buzz buzz-acp buzz-agent buzz-backend-kubernetes buzz-dev-mcp git-credential-nostr; do test -s "target/release/$name"; done
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

for name in buzz buzz-acp buzz-agent buzz-backend-kubernetes buzz-dev-mcp git-credential-nostr; do
  binary=$scratch/source/target/release/$name
  if [[ -n $cache_dir ]]; then binary=$cache_dir/cargo-target/release/$name; fi
  [[ -s $binary ]] || die "build did not produce $name"
  [[ $(readelf -h "$binary" | sed -n 's/^[[:space:]]*Machine:[[:space:]]*//p') = 'Advanced Micro Devices X86-64' ]] || die "unexpected $name ELF architecture"
  [[ $(readelf -h "$binary" | sed -n 's/^[[:space:]]*Type:[[:space:]]*\([^ ]*\).*/\1/p') = @(EXEC|DYN) ]] || die "unexpected $name ELF type"
  marker=
  if [[ $name = buzz ]]; then marker=draft-connector
  elif [[ $name = buzz-acp ]]; then marker=buzz.trusted-turn-context/v1
  fi
  if [[ -n $marker ]]; then
    strings -a "$binary" | grep -F -- "$marker" >/dev/null || die "$name lacks patch marker: $marker"
  fi
  ldd_report=$(ldd -r "$binary" 2>&1) || { echo "$ldd_report" >&2; die "host loader cannot resolve $name"; }
  if [[ $ldd_report = *'not found'* || $ldd_report = *'undefined symbol'* ]]; then
    echo "$ldd_report" >&2
    die "host libraries are incompatible with $name"
  fi
done

staged_output=$(mktemp -d "$output_parent/.rowvia-buzz-sidecars-output.XXXXXXXX")
for name in buzz buzz-acp buzz-agent buzz-backend-kubernetes buzz-dev-mcp git-credential-nostr; do
  binary=$scratch/source/target/release/$name
  if [[ -n $cache_dir ]]; then binary=$cache_dir/cargo-target/release/$name; fi
  install -m 755 "$binary" "$staged_output/$name"
done
python3 - "$staged_output/provenance.json" "$revision" "$source_archive_sha" "$image_id" \
  "$(sha256sum "$staged_output/buzz" | cut -d' ' -f1)" \
  "$(sha256sum "$staged_output/buzz-acp" | cut -d' ' -f1)" <<'PY'
import hashlib
import json
from pathlib import Path
import sys

path, revision, archive_sha, image_id, cli_sha, acp_sha = sys.argv[1:]
with open(path, "w", encoding="utf-8") as output_file:
    json.dump({
        "source_commit": revision,
        "source_archive_sha256": archive_sha,
        "image_id": image_id,
        "artifacts": {
            name: {"sha256": hashlib.file_digest((Path(path).parent / name).open("rb"), "sha256").hexdigest(),
                   "patch_marker": {"buzz": "draft-connector", "buzz-acp": "buzz.trusted-turn-context/v1"}.get(name),
                   "host_ldd_r": "pass"}
            for name in ("buzz", "buzz-acp", "buzz-agent", "buzz-backend-kubernetes", "buzz-dev-mcp", "git-credential-nostr")
        },
    }, output_file, indent=2)
    output_file.write("\n")
PY
[[ ! -e $output && ! -L $output ]] || die "output path appeared during build"
mv -- "$staged_output" "$output"
staged_output=
echo "Verified artifacts: $output/buzz and $output/buzz-acp"
echo "Host ldd -r: pass for both"
