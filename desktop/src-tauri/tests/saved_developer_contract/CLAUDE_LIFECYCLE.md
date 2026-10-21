# Temporary fixed saved-Claude owner test

Development feature `rowvia-owner-test-hook` only. Target is the already-created
`01ab717b2ebeeef89c3abde0be5a55f6aa95fac66d89c96124083ad381857b4c`,
on developer channel `31986c38-6676-4d6f-a4ba-ed833926d0d5`. Creation is not repeated.
The existing Codex/diagnostic pairs and generic runtime-ensure allowlist remain unchanged.

All five requests accept exactly `{ "expiresAt": <Unix milliseconds> }`, valid
for at most two minutes. The existing pinned owner/architect/relay and same-UID
socket admission still apply. No caller-selected command/path/provider/env/key.

1. `attach_saved_claude_developer`: the existing normal attach helper, with
   `ensureRunning:false`. A main-webview-only temporary native invoke accepts
   only an already-admitted request ID. Its existing ledger entry reserves the
   membership publication once; the normal native publication body checks scope,
   target and expiry after key/store/runtime waits and immediately before publish.
2. `prepare_saved_claude_setup`: while stopped with no conflicting target pair,
   use the normal update save/follow-up body to select the fixed setup executable
   `/var/tmp/rowvia-native-auth-setup-claude-GUGVxJY4/rowvia-native-auth-setup`
   with exactly `--setup-profile` and the sibling `profile.json`.
   Fixed saved env is only `BUZZ_ACP_NO_BASE_PROMPT=true`; parallelism is 1.
   Executable SHA256 is `ea3aa4b6f17e16800e22de5998e8b1b8f738269e75ade4a880d62ec8ab95a394`;
   profile SHA256 is `65848d77c42d1b474dc2a381afe65652d0813cf3787fc34a66485da55e2a8326`.
   Parent/executable/profile must be canonical, UID1000, modes0700/0700/0600;
   files must be no-follow regular single-link and bounded, unchanged across read.
3. `start_saved_claude_setup`: ordinary local saved-agent start/preflight/spawn,
   with expected owner/relay and exact saved setup contract rechecked under the
   final transition/store/runtime locks. Relay-mesh preflight is refused here.
   Native passes its saved authorization internally; nothing exports it.
4. Send one authorized `send_existing_test_message` to the fixed Claude pair
   promptly. Start only initializes ACP; the first accepted channel work reaches
   `session/new`. The existing helper validates the authorization at startup and
   publishes the public digest receipt only after valid ACP2 `session/new` with
   absolute cwd and empty MCP list. Its watchdog is 90 seconds; expected prompt
   failure is `rowvia_setup_required`, not a model success/readiness signal.
5. Root requires complete new receipt validation and child/pipe settlement.
   `stop_restore_saved_claude_normal` uses normal Stop then guarded normal Update.
   Stop waits/reaps its owned outer child; it does not independently certify the
   inner setup protocol/pipe settlement. Restore requires exact persisted setup
   configuration, stopped, no conflicting target pairs/receipts. A before-save
   restore failure leaves stopped setup configuration. A post-save ancillary
   failure can leave stopped NORMAL instead: the outcome remains Unknown, no
   automatic retry or assertion that every failure leaves SETUP is made.
6. Root binds this NEW receipt digest, compiles/prepares normal worker inputs,
   and only then requests `start_saved_claude_normal`. It restores/starts exactly
   `/usr/bin/python3.13 -IS /home/orionx/.local/state/rowvia-saved-claude-normal/buzz-worker/run`.

The already-created record starts unconfigured (`env{}`, original default
parallelism). Only stopped attachment/preparation accepts that initial state.
Preparation writes configured SETUP; restoration writes configured NORMAL.
Both require exact saved env `{"BUZZ_ACP_NO_BASE_PROMPT":"true"}` and
`parallelism:1`; NORMAL Start refuses initial env{}. Native reserves
`BUZZ_ACP_AGENTS` and derives it from the parallelism column, so no env override
of that key is introduced. Descriptor user env is applied last to outer ACP;
NO_BASE_PROMPT is a supported non-reserved behavior key, not an ambient premise.
Both states retain legacy Python command, explicit override, `runtime:null`, no
team, default outer `buzz-acp` transport, and `startOnAppLaunch:false`.
Persona linkage is either absent with no source version, or the exact verified
startup-manufactured self-definition below. No unlink/recreate is performed.
Custom outer transports are refused rather than overwritten.
No custom catalog registration/discovery/install is needed for setup. Raw command
resolution has no built-in MCP command; ordinary Native explicitly supplies empty
`BUZZ_ACP_MCP_COMMAND`, so staged normal Claude MCP is not handed to setup.

Ordinary public APIs remain unchanged/no-op-guard delegates. Internal guards
authorize before normal loader materialization, after runtime-lock waits, after
awaited start preflight, and before save/publication. Normal update still has its
normal catalog/retention/Nest side effects; it is not a raw JSON rewrite. Outer
lock order is workspace apply→identity→transition→store→runtime. Update admission
is captured BEFORE the outer transition and passed into the normal wrapper,
never recursively recaptured. Start owns its
own transition rather than recursively reacquiring an outer one. Stop bookkeeping
after an authorized termination remains normal cleanup even if a deadline passes.
Membership rechecks after the normal rate-admission await and immediately before
HTTP send; expiry/deadline/shutdown/scope drift there refuses publication.
The synchronous attachment checker holds the canonical scope/transition locks
through loader verification and post-wait ledger/expiry rechecks, not through
the HTTP transport lifetime. This is admission consistency, not network-wide
transactional serialization.

Success responses are closed public booleans/identity metadata only. Start means
dispatched, not ready. Unknown outcomes stay consumed; Root must not blindly retry.
Existing readback remains bounded channel history (mixed authors), not target-only.
The helper's existing source was not modified or newly attested by this change.

`inspect_saved_claude_initial_state` is a separate native-only, expiresAt-only
semantic inspection. It returns exactly fixed `pubkey`, `recordState`
(`missing|unique|ambiguous`), `contractMatches`, `stoppedInitialMatches`,
`noTrackedPairs`, `noScalarPid`, `receiptsReadable`, `noTargetReceipts`, and
`checks`. Checks are null unless the fixed record is unique; otherwise only
sixteen closed predicate booleans identify name/backend/transport/link/runtime,
opaque binding presence, autostart, normal argv, initial/configured settings,
and relay matches, plus `personaIsSelf`, `teamNone`, `selfDefinitionUnique`,
`creatorSnapshotMatches`, and `sourceVersionMatches`. No record/summary,
environment value, command/path, key,
authorization value/digest or raw error is returned. The two authoritative
results call the existing target contract and stopped-attachment guard, not
an approximation built from diagnostic flags. Those guards accept initial OR
configured-normal state; neither result means membership or runtime readiness.

Normal startup backfill manufactures a keyless definition under the instance's
public-key slug and pins its normal persona content hash. The shared guard allows
that exact self-link only: no case-insensitive definition collision, no foreign
persona/team/provenance, and the static original creator snapshot must match.
That snapshot has fixed name/normal command-derived avatar, stock ACP, empty
prompt/env/name pool, no provider/model/runtime, owner-only/empty allowlist,
definition parallelism10 and channel session policy. Configured instance
parallelism1 does not rewrite this original definition snapshot. Hash consistency
alone cannot authorize a changed definition: prompt/env/provider/behavior edits
are refused even with a recomputed matching hash. Instance prompt/model/provider,
session/access defaults must also remain fixed. No definition bytes/hash are
returned. All lifecycle guards and inspection share this same predicate.

Inspection uses the normal `load_managed_agents` helper, which may hydrate or
migrate internal key metadata. Ordinary Native list flows additionally perform
reaping/builtin bookkeeping that can persist metadata: this is not a zero-write
claim. Inspection itself does not call list/summary, reap, start, stop, edit the
requested configuration, change membership or publish relay content. Raw rows
are dropped before constructing the closed DTO. Existing independent channel
readback supplies membership facts; no new membership/HTTP API is introduced.
The confined real-creator fixture substitutes ONLY the generated public key,
then runs the actual normal startup backfill through a cfg(test)-only directory
seam. It exercises metadata predicates, not signature/identity proof. The normal
migration algorithm and production API are unchanged.

Validation uses disposable isolated app-data fixtures, inert process groups and
no actual Desktop/provider/IPC/auth state. Focused checks:

```text
cargo test --locked --offline --features rowvia-owner-test-hook --lib claude_lifecycle
cargo test --locked --offline --features rowvia-owner-test-hook --lib owner_test_hook::tests
cargo test --locked --offline --features rowvia-owner-test-hook --lib commands::agent_models::update::tests
node --import ./test-loader.mjs --experimental-strip-types --test src/testing/ownerTestHook.test.mjs src/features/agents/channelAgents.accessPolicy.test.mjs
```

All heavy commands require Main's shared `scripts/run-capped-check` 5GiB/swap0
lease. Native uses the existing exact pinned Rust1.95 Docker image/caches, no
network/capabilities/auth mounts, readonly public source and isolated native copy.
