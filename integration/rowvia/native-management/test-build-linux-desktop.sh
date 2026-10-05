#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
build=$script_dir/build-linux-desktop.sh
scratch=$(mktemp -d /home/orionx/rowvia-build-test.XXXXXXXX)
trap 'rm -rf -- "$scratch"' EXIT

expect_failure() {
  if bash "$build" --dry-run --output "$scratch/output" --identity-file "$scratch/identities.env" "$@" >"$scratch/stdout" 2>"$scratch/stderr"; then
    echo "expected dry-run failure" >&2
    exit 1
  fi
  [[ ! -e $scratch/output ]] || { echo "dry-run created output" >&2; exit 1; }
}

printf 'ROWVIA_CONTEXT_BUZZ_OWNER_PUBKEY=%064d\nROWVIA_CONTEXT_CERBERUS_PUBKEY=%064d\n' 0 1 >"$scratch/identities.env"
bash "$build" --dry-run --output "$scratch/output" --identity-file "$scratch/identities.env" >"$scratch/stdout"
grep -Fq 'Mode: compile-check (ai.rowvia.buzz.compile-check; Rowvia Buzz Compile Check)' "$scratch/stdout"
grep -Fq 'Compile-check only: artifact is non-executable and has no runtime state isolation' "$scratch/stdout"
grep -Fq 'Limits: 5 GiB RAM, zero swap, 2 CPUs, 1 build job, 512 PIDs, 25 GiB scratch' "$scratch/stdout"
bash "$build" --live --dry-run --output "$scratch/output" --identity-file "$scratch/identities.env" >"$scratch/stdout"
grep -Fq 'Mode: live (xyz.block.buzz.app; Buzz)' "$scratch/stdout"
[[ ! -e $scratch/output ]] || { echo "dry-run created output" >&2; exit 1; }
expect_failure --live --live
grep -Fq -- '--live was specified more than once' "$scratch/stderr"
expect_failure --owner-test-hook --owner-test-hook
expect_failure --cache-dir relative
expect_failure --cache-dir "$scratch/cache,invalid"
expect_failure --cache-dir "$scratch/.."
expect_failure --cache-dir "$scratch/cache" --cache-dir "$scratch/cache"
bash "$build" --owner-test-hook --cache-dir "$scratch/cache" --dry-run --output "$scratch/output" --identity-file "$scratch/identities.env" >"$scratch/stdout"
grep -Fq 'Owner test hook: enabled (--features rowvia-owner-test-hook; VITE_ROWVIA_OWNER_TEST_HOOK=1)' "$scratch/stdout"
[[ ! -e $scratch/cache ]] || { echo "dry-run created cache" >&2; exit 1; }
mkdir -m 700 -- "$scratch/unmanaged"
expect_failure --cache-dir "$scratch/unmanaged"
ln -s "$scratch/unmanaged" "$scratch/symlink-cache"
expect_failure --cache-dir "$scratch/symlink-cache"
expect_failure --cache-dir "$scratch/output"
expect_failure --cache-dir "$script_dir/build-cache"

printf 'ROWVIA_CONTEXT_BUZZ_OWNER_PUBKEY=bad\nROWVIA_CONTEXT_CERBERUS_PUBKEY=%064d\n' 1 >"$scratch/identities.env"
expect_failure

printf 'ROWVIA_CONTEXT_BUZZ_OWNER_PUBKEY=%064d\nROWVIA_CONTEXT_CERBERUS_PUBKEY=%064d\n' 1 1 >"$scratch/identities.env"
expect_failure

# The fixture must contain literal shell syntax to prove it is never executed.
# shellcheck disable=SC2016
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
        ROWVIA_BUILD_MODE=*) container_build_mode=${argument#ROWVIA_BUILD_MODE=} ;;
        ROWVIA_OWNER_TEST_HOOK=1) hook_enabled=1 ;;
        VITE_ROWVIA_OWNER_TEST_HOOK=1) vite_hook_enabled=1 ;;
        type=bind,src=*,dst=/work/source/desktop/src-tauri/target)
          cache_target=${argument#type=bind,src=}
          cache_target=${cache_target%,dst=/work/source/desktop/src-tauri/target}
          ;;
        type=bind,src=*,dst=/work/source)
          source_dir=${argument#type=bind,src=}
          source_dir=${source_dir%,dst=/work/source}
          ;;
      esac
    done
    [[ -n ${source_dir-} ]] || exit 83
    [[ ${container_build_mode-} = "$ROWVIA_TEST_EXPECTED_BUILD_MODE" ]] || exit 85
    [[ ${hook_enabled:-0} = "${ROWVIA_TEST_EXPECTED_HOOK:-0}" && ${vite_hook_enabled:-0} = "${ROWVIA_TEST_EXPECTED_HOOK:-0}" ]] || exit 86
    [[ " $* " = *' --cpus=2 --memory=5g --memory-swap=5g '* && " $* " = *' --pids-limit=512 '* && " $* " = *' --env CARGO_BUILD_JOBS=1 '* ]] || exit 87
    scratch_dir=${source_dir%/source}
    printf '%s\n' "$scratch_dir" > "$ROWVIA_TEST_SCRATCH_PATH"
    if [[ -n ${cache_target-} ]]; then
      if [[ $ROWVIA_TEST_MODE = mount_cleanup ]]; then
        mkdir -- "$source_dir/desktop/src-tauri/target"
        exit 42
      else
        ln -s "$cache_target" "$source_dir/desktop/src-tauri/target"
      fi
    fi
    if [[ $ROWVIA_TEST_MODE = success || $ROWVIA_TEST_MODE = bad_identifier || $ROWVIA_TEST_MODE = transient_du ]]; then
      binary=$source_dir/desktop/src-tauri/target/release/buzz-desktop
      mkdir -p -- "$(dirname -- "$binary")"
      cp /usr/bin/true "$binary"
      if [[ $container_build_mode = live ]]; then
        embedded_identifier=xyz.block.buzz.app
      else
        embedded_identifier=ai.rowvia.buzz.compile-check
      fi
      if [[ $ROWVIA_TEST_MODE = bad_identifier ]]; then embedded_identifier=wrong.identifier; fi
      printf '%s\n' "$BUZZ_BUILD_ROWVIA_MANAGEMENT_ORIGIN" "$BUZZ_BUILD_ROWVIA_SOURCE_INSTANCE" \
        "$BUZZ_BUILD_ROWVIA_OWNER_PUBKEY" "$BUZZ_BUILD_CERBERUS_PUBKEY" \
        "$ROWVIA_TEST_VERSION" "$embedded_identifier" >> "$binary"
      # Execute the actual Tauri dispatch from the container command with pnpm
      # mocked; a changed feature/env argument must fail this production seam.
      pnpm() {
        [[ $1 = -C && $2 = desktop && $3 = tauri && $4 = build && $5 = --no-bundle ]] || return 88
        if [[ ${ROWVIA_TEST_EXPECTED_HOOK:-0} = 1 ]]; then
          [[ ${VITE_ROWVIA_OWNER_TEST_HOOK:-} = 1 && ${*: -2} = '--features rowvia-owner-test-hook' ]] || return 89
        else
          [[ ! -v VITE_ROWVIA_OWNER_TEST_HOOK && " $* " != *' --features '* ]] || return 90
        fi
      }
      unset ROWVIA_OWNER_TEST_HOOK VITE_ROWVIA_OWNER_TEST_HOOK
      if [[ ${hook_enabled:-0} = 1 ]]; then
        export ROWVIA_OWNER_TEST_HOOK=1 VITE_ROWVIA_OWNER_TEST_HOOK=1
      fi
      export ROWVIA_BUILD_MODE=$container_build_mode
      inner_script=${*: -1}
      dispatch='feature_args=()'${inner_script#*'    feature_args=()'}
      cd -- "$source_dir"
      eval "$dispatch"
      exit 0
    fi
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
if [[ ${ROWVIA_TEST_MODE-} = transient_du || ${ROWVIA_TEST_MODE-} = permanent_du ]]; then
  count=0
  if [[ -f $ROWVIA_TEST_DU_COUNT ]]; then count=$(<"$ROWVIA_TEST_DU_COUNT"); fi
  count=$((count + 1))
  printf '%s\n' "$count" > "$ROWVIA_TEST_DU_COUNT"
  if [[ $count = 1 || $ROWVIA_TEST_MODE = permanent_du ]]; then
    echo 'du: cannot access pnpm-lock.yaml.temporary: No such file or directory' >&2
    exit 1
  fi
fi
if [[ ${ROWVIA_TEST_MODE-} = cache_overcap && -e ${ROWVIA_TEST_DU_TRIGGER-} ]]; then
  # Both paths are individually below the cap; their sum must stop the build.
  printf '13631488\t%s\n' "${*: -1}"
elif [[ ${ROWVIA_TEST_MODE-} = overcap && -e ${ROWVIA_TEST_DU_TRIGGER-} ]]; then
  printf '26214401\t%s\n' "${*: -1}"
else
  exec /usr/bin/du "$@"
fi
SH
cat >"$scratch/mockbin/stat" <<'SH'
#!/usr/bin/env bash
if [[ ${ROWVIA_TEST_MODE-} = mount_cleanup && ${1-} = -c && ${2-} = %u && ${3-} = */source/desktop/src-tauri/target ]]; then
  echo 0
else
  exec /usr/bin/stat "$@"
fi
SH
cat >"$scratch/mockbin/chmod" <<'SH'
#!/usr/bin/env bash
if [[ ${ROWVIA_TEST_MODE-} = mount_cleanup ]]; then
  for path in "$@"; do
    if [[ $path = */source/desktop/src-tauri/target && -d $path ]]; then
      echo 'simulated root-owned mountpoint cannot be chmodded' >&2
      exit 1
    fi
  done
fi
exec /usr/bin/chmod "$@"
SH
chmod +x "$scratch/mockbin/docker" "$scratch/mockbin/sleep" "$scratch/mockbin/du" "$scratch/mockbin/stat" "$scratch/mockbin/chmod"
export ROWVIA_TEST_SCRATCH_PATH=$scratch/build-scratch-path
export ROWVIA_TEST_STOP=$scratch/stop
export ROWVIA_TEST_DU_TRIGGER=$scratch/overcap-trigger
export ROWVIA_TEST_DU_COUNT=$scratch/du-attempts
export ROWVIA_TEST_VERSION=0.5.23
export PATH=$scratch/mockbin:$PATH

ROWVIA_TEST_IMAGE_ID=sha256:wrong expect_failure
[[ ! -e $ROWVIA_TEST_SCRATCH_PATH ]] || { echo "wrong image created scratch" >&2; exit 1; }

for mode in cleanup watchdog overcap; do
  export ROWVIA_TEST_MODE=$mode
  export ROWVIA_TEST_EXPECTED_BUILD_MODE=compile-check
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

for build_mode in compile-check live; do
  export ROWVIA_TEST_MODE=success
  export ROWVIA_TEST_EXPECTED_BUILD_MODE=$build_mode
  build_flag=()
  if [[ $build_mode = live ]]; then build_flag=(--live); fi
  bash "$build" --output "$scratch/output" --identity-file "$scratch/identities.env" "${build_flag[@]}" >"$scratch/stdout"
  expected_artifact=buzz-desktop.compile-check
  expected_deployable=false
  expected_identifier=ai.rowvia.buzz.compile-check
  expected_product='Rowvia Buzz Compile Check'
  if [[ $build_mode = live ]]; then
    expected_artifact=buzz-desktop
    expected_deployable=true
    expected_identifier=xyz.block.buzz.app
    expected_product=Buzz
  fi
  [[ -s $scratch/output/$expected_artifact && -s $scratch/output/provenance.json ]]
  if [[ $build_mode = live ]]; then
    [[ -x $scratch/output/$expected_artifact ]]
  else
    [[ ! -x $scratch/output/$expected_artifact ]]
  fi
  grep -Fq "\"build_mode\": \"$build_mode\"" "$scratch/output/provenance.json"
  grep -Fq "\"deployable\": $expected_deployable" "$scratch/output/provenance.json"
  grep -Fq "\"artifact_filename\": \"$expected_artifact\"" "$scratch/output/provenance.json"
  grep -Fq "\"tauri_identifier\": \"$expected_identifier\"" "$scratch/output/provenance.json"
  grep -Fq "\"product_name\": \"$expected_product\"" "$scratch/output/provenance.json"
  build_scratch=$(<"$ROWVIA_TEST_SCRATCH_PATH")
  [[ ! -e $build_scratch ]] || { echo "successful build left scratch" >&2; exit 1; }
  rm -r -- "$scratch/output"
  rm -f -- "$ROWVIA_TEST_SCRATCH_PATH" "$ROWVIA_TEST_STOP"
done

export ROWVIA_TEST_MODE=bad_identifier
export ROWVIA_TEST_EXPECTED_BUILD_MODE=live
if bash "$build" --live --output "$scratch/output" --identity-file "$scratch/identities.env" >"$scratch/stdout" 2>"$scratch/stderr"; then
  echo "wrong embedded live identifier unexpectedly passed" >&2
  exit 1
fi
grep -Fq 'Tauri identifier is missing from binary' "$scratch/stderr"
[[ ! -e $scratch/output ]]

export ROWVIA_TEST_EXPECTED_BUILD_MODE=live
for hook in 0 1; do
  export ROWVIA_TEST_MODE=success ROWVIA_TEST_EXPECTED_HOOK=$hook
  hook_flags=()
  if [[ $hook = 1 ]]; then hook_flags=(--owner-test-hook); fi
  # Inherited flags must not opt the default build in.
  ROWVIA_OWNER_TEST_HOOK=1 VITE_ROWVIA_OWNER_TEST_HOOK=1 bash "$build" --live --cache-dir "$scratch/cache" --output "$scratch/output" --identity-file "$scratch/identities.env" "${hook_flags[@]}" >"$scratch/stdout"
  [[ -s $scratch/cache/cargo-target/release/buzz-desktop && $(stat -c %a "$scratch/cache") = 700 ]]
  expected_hook=false
  if [[ $hook = 1 ]]; then expected_hook=true; fi
  grep -Fq "\"owner_test_hook\": $expected_hook" "$scratch/output/provenance.json"
  build_scratch=$(<"$ROWVIA_TEST_SCRATCH_PATH")
  [[ ! -e $build_scratch ]]
  rm -r -- "$scratch/output"
  rm -f -- "$ROWVIA_TEST_SCRATCH_PATH" "$ROWVIA_TEST_STOP"
done
export ROWVIA_TEST_EXPECTED_HOOK=0
: > "$scratch/cache/unrelated"
expect_failure --cache-dir "$scratch/cache"
grep -Fq 'cache contains unrelated data' "$scratch/stderr"
rm -- "$scratch/cache/unrelated"
chmod 755 "$scratch/cache"
expect_failure --cache-dir "$scratch/cache"
grep -Fq 'owner-only directory' "$scratch/stderr"
chmod 700 "$scratch/cache"
exec {test_lock_fd}>> "$scratch/cache/.lock"
flock -n "$test_lock_fd"
if bash "$build" --cache-dir "$scratch/cache" --output "$scratch/output" --identity-file "$scratch/identities.env" >"$scratch/stdout" 2>"$scratch/stderr"; then
  echo "locked cache unexpectedly accepted" >&2; exit 1
fi
grep -Fq 'build cache is already in use' "$scratch/stderr"
flock -u "$test_lock_fd"
for mode in cleanup cache_overcap mount_cleanup; do
  export ROWVIA_TEST_MODE=$mode ROWVIA_TEST_EXPECTED_BUILD_MODE=compile-check
  if bash "$build" --cache-dir "$scratch/cache" --output "$scratch/output" --identity-file "$scratch/identities.env" >"$scratch/stdout" 2>"$scratch/stderr"; then
    echo "mock cached $mode build unexpectedly succeeded" >&2; exit 1
  fi
  [[ -s $scratch/cache/cargo-target/release/buzz-desktop ]]
  build_scratch=$(<"$ROWVIA_TEST_SCRATCH_PATH")
  [[ ! -e $build_scratch && ! -e $scratch/output ]]
  if [[ $mode = cache_overcap ]]; then grep -Fq 'scratch exceeded 25 GiB' "$scratch/stderr"; fi
  rm -f -- "$ROWVIA_TEST_SCRATCH_PATH" "$ROWVIA_TEST_STOP" "$ROWVIA_TEST_DU_TRIGGER"
done

export ROWVIA_TEST_MODE=transient_du ROWVIA_TEST_EXPECTED_BUILD_MODE=compile-check
bash "$build" --output "$scratch/output" --identity-file "$scratch/identities.env" >"$scratch/stdout" 2>"$scratch/stderr"
[[ -s $scratch/output/buzz-desktop.compile-check && $(<"$ROWVIA_TEST_DU_COUNT") -ge 3 ]]
grep -Fq 'pnpm-lock.yaml.temporary: No such file or directory' "$scratch/stderr"
rm -r -- "$scratch/output"
rm -f -- "$ROWVIA_TEST_SCRATCH_PATH" "$ROWVIA_TEST_STOP" "$ROWVIA_TEST_DU_COUNT"
export ROWVIA_TEST_MODE=permanent_du
if bash "$build" --output "$scratch/output" --identity-file "$scratch/identities.env" >"$scratch/stdout" 2>"$scratch/stderr"; then
  echo "permanent du failure unexpectedly accepted" >&2; exit 1
fi
[[ $(<"$ROWVIA_TEST_DU_COUNT") = 3 && ! -e $scratch/output && ! -e $ROWVIA_TEST_SCRATCH_PATH ]]
grep -Fq 'cannot measure source archive size' "$scratch/stderr"

echo "build mode, opt-in hooks, cache retention/locking, provenance, fail-closed inputs, combined disk cap, bounded du retries, watchdog, and mountpoint cleanup checks passed"
