#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
build=$script_dir/build-linux-sidecars.sh
revision=55f1d59a558ae16be687648179ff174bf6e55792
scratch=$(mktemp -d /home/orionx/rowvia-sidecars-test.XXXXXXXX)
trap 'rm -rf -- "$scratch"' EXIT

expect_failure() {
  if bash "$build" --revision "$revision" --output "$scratch/output" "$@" >"$scratch/stdout" 2>"$scratch/stderr"; then
    echo "expected build failure" >&2
    exit 1
  fi
  [[ ! -e $scratch/output ]] || { echo "failed build created output" >&2; exit 1; }
}

mkdir -- "$scratch/mockbin"
cat >"$scratch/mockbin/docker" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
case $1 in
  image)
    if [[ ${5-} = *Architecture* ]]; then echo amd64/linux
    else echo "${ROWVIA_TEST_IMAGE_ID:-sha256:b30f21ab265a10892f7a77273440f6eeee7996f0108cf7df2e622965b435d77c}"
    fi ;;
  run)
    args=" $* "
    for flag in '--cpus=2' '--memory=5g' '--memory-swap=5g' '--pids-limit=512' '--read-only' '--log-driver=none' 'CARGO_BUILD_JOBS=1'; do
      [[ $args = *" $flag "* ]] || { echo "missing container flag: $flag" >&2; exit 80; }
    done
    [[ $args = *" --user $(id -u):$(id -g) "* ]] || { echo 'wrong container user' >&2; exit 81; }
    [[ $args != *' HOME='* ]] || { echo 'container HOME override' >&2; exit 85; }
    [[ $args = *' HERMIT_BIN_INSTALL_DIR=/work/source/.hermit-bin '* ]] || { echo 'missing Hermit bin path' >&2; exit 86; }
    [[ $args = *'cargo build --release --locked --jobs 1 -p buzz-cli -p buzz-acp -p buzz-agent -p buzz-backend-kubernetes -p buzz-dev-mcp -p git-credential-nostr --bins'* ]] || { echo 'wrong cargo command' >&2; exit 82; }
    for argument in "$@"; do
      case $argument in
        type=bind,src=*,dst=/work/source)
          source_dir=${argument#type=bind,src=}
          source_dir=${source_dir%,dst=/work/source} ;;
        type=bind,src=*,dst=/work/source/target)
          target_dir=${argument#type=bind,src=}
          target_dir=${target_dir%,dst=/work/source/target} ;;
      esac
    done
    [[ -n ${source_dir-} ]] || exit 83
    scratch_dir=${source_dir%/source}
    printf '%s\n' "$scratch_dir" > "$ROWVIA_TEST_SCRATCH_PATH"
    if [[ $ROWVIA_TEST_MODE = success || $ROWVIA_TEST_MODE = bad_marker || $ROWVIA_TEST_MODE = missing_binary || $ROWVIA_TEST_MODE = missing_helper ]]; then
      target_dir=${target_dir:-$source_dir/target}
      mkdir -p "$target_dir/release"
      for name in buzz buzz-acp buzz-agent buzz-backend-kubernetes buzz-dev-mcp git-credential-nostr; do
        [[ $ROWVIA_TEST_MODE = missing_binary && $name = buzz-acp ]] && continue
        [[ $ROWVIA_TEST_MODE = missing_helper && $name = buzz-agent ]] && continue
        binary=$target_dir/release/$name
        cp /usr/bin/true "$binary"
        if [[ $name = buzz ]]; then marker=draft-connector
        else marker=buzz.trusted-turn-context/v1
        fi
        if [[ $ROWVIA_TEST_MODE = bad_marker && $name = buzz-acp ]]; then marker=wrong-marker; fi
        printf '%s\n' "$marker" >> "$binary"
      done
      exit 0
    fi
    if [[ $ROWVIA_TEST_MODE = cleanup ]]; then exit 42; fi
    : > "$ROWVIA_TEST_DU_TRIGGER"
    while [[ ! -e $ROWVIA_TEST_STOP ]]; do /usr/bin/sleep 0.1; done
    exit 43 ;;
  stop|kill) : > "$ROWVIA_TEST_STOP" ;;
  *) exit 84 ;;
esac
SH
cat >"$scratch/mockbin/sleep" <<'SH'
#!/usr/bin/env bash
exec /usr/bin/sleep 0.1
SH
cat >"$scratch/mockbin/du" <<'SH'
#!/usr/bin/env bash
if [[ ${ROWVIA_TEST_MODE-} = overcap && -e ${ROWVIA_TEST_DU_TRIGGER-} ]]; then
  printf '26214401\t%s\n' "${*: -1}"
else
  exec /usr/bin/du "$@"
fi
SH
chmod +x "$scratch/mockbin/docker" "$scratch/mockbin/sleep" "$scratch/mockbin/du"
export PATH=$scratch/mockbin:$PATH
export ROWVIA_TEST_SCRATCH_PATH=$scratch/build-scratch-path
export ROWVIA_TEST_STOP=$scratch/stop
export ROWVIA_TEST_DU_TRIGGER=$scratch/overcap-trigger

bash "$build" --revision "$revision" --dry-run --output "$scratch/output" >"$scratch/stdout"
grep -Fq 'Source: 55f1d59a558ae16be687648179ff174bf6e55792' "$scratch/stdout"
grep -Fq 'Dry run: source export and Docker build skipped' "$scratch/stdout"
[[ ! -e $scratch/output && ! -e $ROWVIA_TEST_SCRATCH_PATH ]]
expect_failure --dry-run --revision 0000000000000000000000000000000000000000
grep -Fq 'pinned revision is unavailable' "$scratch/stderr"
ROWVIA_TEST_IMAGE_ID=sha256:wrong expect_failure --dry-run
grep -Fq 'pinned image ID' "$scratch/stderr"
[[ ! -e $ROWVIA_TEST_SCRATCH_PATH ]]

for mode in cleanup overcap bad_marker missing_binary missing_helper success; do
  export ROWVIA_TEST_MODE=$mode
  if [[ $mode = success ]]; then
    bash "$build" --revision "$revision" --output "$scratch/output" >"$scratch/stdout"
    [[ -x $scratch/output/buzz && -x $scratch/output/buzz-acp ]]
    [[ -s $scratch/output/provenance.json ]]
    [[ $(find "$scratch/output" -maxdepth 1 -type f | wc -l) -eq 7 ]]
    python3 - "$scratch/output" <<'PY'
import hashlib
import json
from pathlib import Path
import sys

output = Path(sys.argv[1])
provenance = json.loads((output / "provenance.json").read_text())
assert provenance["source_commit"] == "55f1d59a558ae16be687648179ff174bf6e55792"
assert len(provenance["source_archive_sha256"]) == 64
assert provenance["image_id"] == "sha256:b30f21ab265a10892f7a77273440f6eeee7996f0108cf7df2e622965b435d77c"
for name in ("buzz", "buzz-acp", "buzz-agent", "buzz-backend-kubernetes", "buzz-dev-mcp", "git-credential-nostr"):
    assert provenance["artifacts"][name]["sha256"] == hashlib.sha256((output / name).read_bytes()).hexdigest()
    assert provenance["artifacts"][name]["host_ldd_r"] == "pass"
PY
  else
    expect_failure
    if [[ $mode = overcap ]]; then
      grep -Fq 'scratch exceeded 25 GiB' "$scratch/stderr"
    elif [[ $mode = bad_marker ]]; then
      grep -Fq 'buzz-acp lacks patch marker' "$scratch/stderr"
    elif [[ $mode = missing_binary ]]; then
      grep -Fq 'build did not produce buzz-acp' "$scratch/stderr"
    elif [[ $mode = missing_helper ]]; then
      grep -Fq 'build did not produce buzz-agent' "$scratch/stderr"
    fi
  fi
  [[ -f $ROWVIA_TEST_SCRATCH_PATH ]] || { echo "mock build did not reach container" >&2; sed -n '1,30p' "$scratch/stderr" >&2; exit 1; }
  build_scratch=$(<"$ROWVIA_TEST_SCRATCH_PATH")
  [[ ! -e $build_scratch ]] || { echo "build scratch was not removed" >&2; exit 1; }
  rm -f -- "$ROWVIA_TEST_SCRATCH_PATH" "$ROWVIA_TEST_STOP" "$ROWVIA_TEST_DU_TRIGGER"
done

mv -- "$scratch/output" "$scratch/success-output"
cache=$scratch/cache
mkdir -m 700 "$cache"
printf '%s\n' rowvia-desktop-cargo-cache-v1 > "$cache/.rowvia-build-cache"
mkdir "$cache/cargo-target"
export ROWVIA_TEST_MODE=success
bash "$build" --revision "$revision" --cache-dir "$cache" --output "$scratch/cached-output" >"$scratch/stdout"
[[ -x $scratch/cached-output/buzz && -x $cache/cargo-target/release/buzz-acp ]]
exec {test_lock}>> "$cache/.lock"
flock -n "$test_lock"
expect_failure --cache-dir "$cache"
grep -Fq 'build cache is already in use' "$scratch/stderr"
flock -u "$test_lock"
printf '%s\n' unrelated > "$cache/unrelated"
expect_failure --cache-dir "$cache" --dry-run
grep -Fq 'unrelated cache contents' "$scratch/stderr"

echo "sidecar build dry-run, pinning, bounded cleanup, output and provenance checks passed"
