# Fixed saved-developer owner hook (source only)

This adds three closed operations to the existing opt-in, same-UID Unix owner
test hook. It is not deployed, and does not prepare or qualify a runtime. Do not
restart Desktop for this source check: Desktop shutdown stops unrelated agents
and terminals. No hook calls, publication, Buzz CLI/HTTP requests, or live agent starts
are part of validation here.

The existing request envelope and fixed expected owner/Architekt/relay checks
remain required. These three operations run in native code, with an additional
check of the actual current unlocked owner and selected community under the
workspace, identity and runtime/store locks. The workspace transaction lock is
acquired first without waiting; an in-flight workspace apply returns `not_ready`
before other locks or saved-state IO. They do not pass through arbitrary webview
invocation or JavaScript execution.

| Operation | Exact arguments | Effect |
| --- | --- | --- |
| `inspect_saved_developer` | `{}` | Project only target status, fingerprint and prerequisite presence. |
| `configure_saved_developer` | `{}` | Register absent/byte-identical fixed harness and update only the existing target row. |
| `start_saved_developer` | `{"fingerprint":"<64 lowercase hex>"}` | Compare current configuration/capsule metadata and dispatch one lazy managed pair. |

The fixed public target is
`29cc1bb0ac338520c263dcfe654764224018a1ff5e5d3fc633be73f71f0c8c78`, owner
`89af092923ad49e3b9916b0c9f28c6b9237d8fcab4461cb7ddd12afe354f0d68`, relay
`wss://buzz.rowvia.ai:8443`, channel `31986c38-6676-4d6f-a4ba-ed833926d0d5`,
model `gpt-6.1-sol`. There are no caller-selected keys, commands, arguments,
environment maps or target channels. Existing Architekt and unrelated records
remain semantically intact, including unknown JSON fields.

The reserved future Custom harness ID is `rowvia-saved-developer-codex`, command
`/usr/bin/python3.13`, with arguments exactly `-IS` and
`/home/orionx/.local/state/rowvia-saved-developer-codex/buzz-worker/run`.
The capsule coordinate is explicitly reserved, not claimed prepared or reviewed.
Root must prepare it separately through the existing six-input local-worker
process after the Native Memory/restart proof. The bootstrap runtime's `--epoch`
is not an ACP entrypoint and is never used as this harness.

Configure pins the per-instance harness, model/provider, single worker,
owner-only response gate, fixed channel/mentions subscription, lazy pool,
enabled mention filtering and no-base-prompt
flag, and disables automatic start/restart for this target. Other saved fields
and environment entries are preserved. A linked persona is resolved only from
the one matching keyless definition in the same bounded store read; the shared
definition is never edited. Team/pack links (including the legacy
`persona_pack_path` alias) remain refused. Nonlocal, nonstandard outer ACP commands,
duplicate target/definition rows and any conflicting harness bytes/ID
are refused. Exact harness bytes are defined by the production helper's
`harness_bytes()`; semantically equivalent reserialization is a collision.

Like normal native spawning, relay authority comes from the current canonical
workspace, not the saved record's legacy relay string. Valid saved relay strings
(including empty/stale strings) are preserved verbatim, never normalized or
rewritten; malformed/non-string relay fields remain refused. Actual unlocked
owner, selected workspace/community relay and runtime admission gates are unchanged.

The helper loads the actual normal global agent configuration, propagating
load/parse failures without exposing contents. It uses the normal effective
configuration and harness resolvers with a clone of only the selected definition.
The explicit fixed-target policy `fixed-saved-developer-model-provider/v1` sets
that in-memory clone's model/provider to `gpt-6.1-sol`/`codex`; it is not inherited
fallback. Prompt, environment, runtime provenance and the saved original remain
intact. No shared definition/global/store write is made for this overlay. The
final resolved tuple, fixed command/arguments and lazy/mention controls are
checked again. Missing, ambiguous, malformed or wrong-effective context fails
closed. An absent global file uses the normal accessor's defaults for other
fields, never as an invented model/provider source.

Configure can reserve saved configuration before the worker exists. Start
requires the canonical same-UID 0700 worker directory and its regular single-link
`run` (0700), `profile.json`, `parent.toml`, `provenance.json` (0600), plus the
canonical executable interpreter. Only metadata is checked/fingerprinted here;
the existing worker retains full provenance/pin/config verification. No capsule
file is manufactured or repaired. The helper does not inspect authentication
files or the bootstrap capsule's private contents.

Inspect/configure return a compare token. After capsule preparation, inspect
again before start because capsule metadata participates in that token. Changes
to target configuration, the original definition/global configuration, named
fixed-target override policy, resolved
prompt/environment/harness or capsule metadata invalidate it. Before start the
helper reloads global context and refuses digest drift, then passes those same
captured personas/global values to the existing native spawn boundary. Global
setters have no shared transaction lock: this guarantees captured-value
consistency, not serialization against future configuration edits. Signing
material/auth tags are not returned. `prerequisitesPresent` is metadata presence,
not authority or provenance acceptance. `runtimeReady` is always false;
`startDispatched` confirms only a managed lazy harness spawn, not ACP/provider,
Memory or reply readiness.

This persona/relay compatibility change does not establish a no-history-replay
startup barrier. Actual start remains a separate approval gate; no persisted
queue/cursor is cleared or reset by this helper.

Any tracked target pair, saved PID or target receipt causes configure/start
refusal. There is no reaping, adoption, termination, global status synchronization,
pending profile flush or unrelated key migration. A stale target receipt requires
separate explicit reconciliation. Inspect does not clean it up. A child is
registered immediately after spawn, including on receipt-write failure; an
unknown response must be reconciled rather than retried. Existing generic
managed operations retain their behavior.

Native dispatch uses the original socket deadline and shutdown cancellation.
Timeout/cancellation caches an unknown outcome and releases the pending slot.
Queued blocking work is aborted; an already-running operation can still finish
and retain a child, so late completion never turns that cached outcome into a
confirmed success or permits redispatch of the same request ID.

## Credential-free validation

All checks must use the existing shared capped runner with the clean environment:

```text
/usr/bin/env -i HOME=/home/orionx PATH=/usr/bin:/bin XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus /bin/bash /home/orionx/project/RowviaContext/repos/rowvia-context-postman-home/scripts/run-capped-check -- COMMAND
```

The focused crate compiles the exact production contract file; all five tests
pass, exercising its scope/locked-owner/target/collision/process/fingerprint
gates with fake rows and capsule metadata. Run `cargo test --offline --locked --manifest-path
desktop/src-tauri/tests/saved_developer_contract/Cargo.toml` with matching pinned
`RUSTC` and `RUSTDOC`. The full native gate is `cargo check --offline --locked
--manifest-path desktop/src-tauri/Cargo.toml --features rowvia-owner-test-hook
--lib`, followed by the hook and managed-agent unit tests when dependencies are
available. Core tests do not establish that the native adapter compiles or
behaves correctly in Tauri.
The initial host check fetched exact locked Rust dependencies, then stopped at
missing `glib-2.0.pc`. Native compilation subsequently passed using the existing
`integration/buzz-desktop-dev` Docker environment from the Rowvia repository,
image `sha256:b30f21ab265a10892f7a77273440f6eeee7996f0108cf7df2e622965b435d77c`
and the repository's pinned Rust 1.95.0. The check used the owner-hook feature,
`--locked --offline --lib -j 1`, read-only source with a disposable Tauri copy,
existing caches, 260-second check epochs (600 seconds for the final test epoch)
and verified 5 GiB/zero-swap container
limits inside the shared capped runner. Each epoch verifies removal of only its
exact owned container. Existing application lockfiles and cache permissions are
unchanged. The compile emitted two existing dead-code warnings in
`rowvia_management.rs`.

The focused native command `cargo test --locked --offline --manifest-path
desktop/src-tauri/Cargo.toml --features rowvia-owner-test-hook --lib -j 1
owner_test_ -- --test-threads=1` passed all 28 selected tests (3,467 others
filtered out). The six helper tests and native dispatch timeout/cancellation
regression bind the actual
identity gate, captured spawn context, bounded receipt/runtime ownership scan,
and retain-before-receipt failure path; fixture processes use an already-reaped
`/usr/bin/true`, never an ACP harness. Opaque store/team/global/pending-profile
fixtures check that the captured-context refusal leaves their bytes intact.
Additional regressions exercise real workspace key replacement under the
workspace lock and the resolved descriptor applied at the production command
environment boundary with hostile saved lazy/mention overrides.
Removing the production legacy-alias guard made the focused target-contract
test fail as expected; the guard was immediately restored and all five tests
passed again. This mutation observation covers that core guard only. The
native timeout/cancellation fixture uses bounded wall-clock waits and a worker
receive timeout; all selected native tests passed after replacing its initial
paused-clock coordination, which had stalled.
No live workflow acceptance is inferred from these source tests.
