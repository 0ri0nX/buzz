#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
build=$script_dir/build-linux-desktop.sh
scratch=$(mktemp -d /home/orionx/rowvia-build-test.XXXXXXXX)
trap 'rm -rf -- "$scratch"' EXIT

expect_failure() {
  if bash "$build" --dry-run --output "$scratch/output" --identity-file "$scratch/identities.env" >"$scratch/stdout" 2>"$scratch/stderr"; then
    echo "expected dry-run failure" >&2
    exit 1
  fi
  [[ ! -e $scratch/output ]] || { echo "dry-run created output" >&2; exit 1; }
}

printf 'ROWVIA_CONTEXT_BUZZ_OWNER_PUBKEY=%064d\nROWVIA_CONTEXT_CERBERUS_PUBKEY=%064d\n' 0 1 >"$scratch/identities.env"
bash "$build" --dry-run --output "$scratch/output" --identity-file "$scratch/identities.env" >"$scratch/stdout"
[[ ! -e $scratch/output ]] || { echo "dry-run created output" >&2; exit 1; }

printf 'ROWVIA_CONTEXT_BUZZ_OWNER_PUBKEY=bad\nROWVIA_CONTEXT_CERBERUS_PUBKEY=%064d\n' 1 >"$scratch/identities.env"
expect_failure

printf 'ROWVIA_CONTEXT_BUZZ_OWNER_PUBKEY=%064d\nROWVIA_CONTEXT_CERBERUS_PUBKEY=%064d\n' 1 1 >"$scratch/identities.env"
expect_failure

printf 'ROWVIA_CONTEXT_BUZZ_OWNER_PUBKEY=$(id)\nROWVIA_CONTEXT_CERBERUS_PUBKEY=%064d\n' 1 >"$scratch/identities.env"
expect_failure

printf 'ROWVIA_CONTEXT_BUZZ_OWNER_PUBKEY=%064d\nROWVIA_CONTEXT_CERBERUS_PUBKEY=%064d\n' 0 1 >"$scratch/identities.env"
mkdir -- "$scratch/mockbin"
cat >"$scratch/mockbin/docker" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
case $1 in
  image)
    if [[ ${5-} = *Architecture* ]]; then
      echo amd64/linux
    else
      echo "${ROWVIA_TEST_IMAGE_ID:-sha256:b30f21ab265a10892f7a77273440f6eeee7996f0108cf7df2e622965b435d77c}"
    fi ;;
  run)
    [[ " $* " = *' --read-only '* && " $* " = *' --log-driver=none '* ]] || exit 80
    [[ " $* " = *" --user $(id -u):$(id -g) "* ]] || exit 81
    [[ " $* " = *' --env HOME=/work/source/.home '* ]] || exit 82
    for argument in "$@"; do
      case $argument in
        type=bind,src=*,dst=/work/source)
          source_dir=${argument#type=bind,src=}
          source_dir=${source_dir%,dst=/work/source}
          break ;;
      esac
    done
    [[ -n ${source_dir-} ]] || exit 83
    scratch_dir=${source_dir%/source}
    printf '%s\n' "$scratch_dir" > "$ROWVIA_TEST_SCRATCH_PATH"
    mkdir -- "$source_dir/nested"
    : > "$source_dir/nested/file"
    if [[ $ROWVIA_TEST_MODE = cleanup ]]; then exit 42; fi
    if [[ $ROWVIA_TEST_MODE = watchdog ]]; then
      chmod 000 "$source_dir/nested"
    else
      : > "$ROWVIA_TEST_DU_TRIGGER"
    fi
    while [[ ! -e $ROWVIA_TEST_STOP ]]; do /usr/bin/sleep 0.1; done
    exit 43 ;;
  stop|kill)
    if [[ -f $ROWVIA_TEST_SCRATCH_PATH ]]; then
      scratch_dir=$(<"$ROWVIA_TEST_SCRATCH_PATH")
      chmod 700 "$scratch_dir/source/nested" 2>/dev/null || true
    fi
    : > "$ROWVIA_TEST_STOP" ;;
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
export ROWVIA_TEST_SCRATCH_PATH=$scratch/build-scratch-path
export ROWVIA_TEST_STOP=$scratch/stop
export ROWVIA_TEST_DU_TRIGGER=$scratch/overcap-trigger
export PATH=$scratch/mockbin:$PATH

ROWVIA_TEST_IMAGE_ID=sha256:wrong expect_failure
[[ ! -e $ROWVIA_TEST_SCRATCH_PATH ]] || { echo "wrong image created scratch" >&2; exit 1; }

for mode in cleanup watchdog overcap; do
  export ROWVIA_TEST_MODE=$mode
  if bash "$build" --output "$scratch/output" --identity-file "$scratch/identities.env" >"$scratch/stdout" 2>"$scratch/stderr"; then
    echo "mock $mode build unexpectedly succeeded" >&2
    exit 1
  fi
  [[ -f $ROWVIA_TEST_SCRATCH_PATH ]] || { echo "mock build did not reach container" >&2; exit 1; }
  build_scratch=$(<"$ROWVIA_TEST_SCRATCH_PATH")
  [[ ! -e $build_scratch ]] || { echo "build scratch was not removed" >&2; exit 1; }
  [[ ! -e $scratch/output ]] || { echo "failed build produced output" >&2; exit 1; }
  if [[ $mode = watchdog ]]; then
    grep -Fq 'cannot measure scratch usage' "$scratch/stderr" || {
      echo "watchdog did not fail closed on inaccessible directory" >&2
      exit 1
    }
  elif [[ $mode = overcap ]]; then
    grep -Fq 'scratch exceeded 25 GiB' "$scratch/stderr" || {
      echo "watchdog did not enforce the scratch cap" >&2
      exit 1
    }
  fi
  rm -f -- "$ROWVIA_TEST_SCRATCH_PATH" "$ROWVIA_TEST_STOP" "$ROWVIA_TEST_DU_TRIGGER"
done

echo "build script dry-run, fail-closed inputs, disk cap, watchdog, and cleanup checks passed"
