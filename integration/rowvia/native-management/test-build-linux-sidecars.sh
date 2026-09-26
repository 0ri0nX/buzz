#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
build=$script_dir/build-linux-sidecars.sh
scratch=$(mktemp -d /home/orionx/rowvia-sidecars-test.XXXXXXXX)
trap 'rm -rf -- "$scratch"' EXIT

expect_failure() {
  if bash "$build" --output "$scratch/output" "$@" >"$scratch/stdout" 2>"$scratch/stderr"; then
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
    for flag in '--cpus=2' '--memory=4g' '--memory-swap=5g' '--pids-limit=512' '--read-only' '--log-driver=none' 'CARGO_BUILD_JOBS=1'; do
      [[ $args = *" $flag "* ]] || { echo "missing container flag: $flag" >&2; exit 80; }
    done
    [[ $args = *" --user $(id -u):$(id -g) "* ]] || { echo 'wrong container user' >&2; exit 81; }
    [[ $args = *'cargo build --release --locked --jobs 1 -p buzz-cli -p buzz-acp --bins'* ]] || { echo 'wrong cargo command' >&2; exit 82; }
    for argument in "$@"; do
      case $argument in
        type=bind,src=*,dst=/work/source)
          source_dir=${argument#type=bind,src=}
          source_dir=${source_dir%,dst=/work/source} ;;
      esac
    done
    [[ -n ${source_dir-} ]] || exit 83
    scratch_dir=${source_dir%/source}
    printf '%s\n' "$scratch_dir" > "$ROWVIA_TEST_SCRATCH_PATH"
    if [[ $ROWVIA_TEST_MODE = success || $ROWVIA_TEST_MODE = bad_marker || $ROWVIA_TEST_MODE = missing_binary ]]; then
      mkdir -p "$source_dir/target/release"
      for name in buzz buzz-acp; do
        [[ $ROWVIA_TEST_MODE = missing_binary && $name = buzz-acp ]] && continue
        binary=$source_dir/target/release/$name
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

bash "$build" --dry-run --output "$scratch/output" >"$scratch/stdout"
grep -Fq 'Source: 55f1d59a558ae16be687648179ff174bf6e55792' "$scratch/stdout"
grep -Fq 'Dry run: source export and Docker build skipped' "$scratch/stdout"
[[ ! -e $scratch/output && ! -e $ROWVIA_TEST_SCRATCH_PATH ]]
expect_failure --dry-run --revision 0000000000000000000000000000000000000000
grep -Fq 'revision differs from the pinned Desktop source commit' "$scratch/stderr"
ROWVIA_TEST_IMAGE_ID=sha256:wrong expect_failure --dry-run
grep -Fq 'pinned image ID' "$scratch/stderr"
[[ ! -e $ROWVIA_TEST_SCRATCH_PATH ]]

for mode in cleanup overcap bad_marker missing_binary success; do
  export ROWVIA_TEST_MODE=$mode
  if [[ $mode = success ]]; then
    bash "$build" --output "$scratch/output" >"$scratch/stdout"
    [[ -x $scratch/output/buzz && -x $scratch/output/buzz-acp ]]
    [[ -s $scratch/output/provenance.json ]]
    [[ $(find "$scratch/output" -maxdepth 1 -type f | wc -l) -eq 3 ]]
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
for name in ("buzz", "buzz-acp"):
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
    fi
  fi
  [[ -f $ROWVIA_TEST_SCRATCH_PATH ]] || { echo "mock build did not reach container" >&2; sed -n '1,30p' "$scratch/stderr" >&2; exit 1; }
  build_scratch=$(<"$ROWVIA_TEST_SCRATCH_PATH")
  [[ ! -e $build_scratch ]] || { echo "build scratch was not removed" >&2; exit 1; }
  rm -f -- "$ROWVIA_TEST_SCRATCH_PATH" "$ROWVIA_TEST_STOP" "$ROWVIA_TEST_DU_TRIGGER"
done

echo "sidecar build dry-run, pinning, bounded cleanup, output and provenance checks passed"
