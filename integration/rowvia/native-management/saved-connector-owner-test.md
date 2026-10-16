# Fixed saved-developer Gmail owner test (source only)

This extends only the existing opt-in local owner hook. It does not enable the
hook in an ordinary build, manufacture signed evidence, create a journal entry
directly, change production authorization, or start/restart Desktop or agents.
The passive dialog registration performs no IPC or business action. Actual
actions require the native owner-hook feature, configured same-UID socket,
main webview, live native request ledger, actual unlocked owner, and selected
community. No real hook calls, Gmail reads, grants or deployments are part of
source validation.

The fixed target is `29cc1bb0ac338520c263dcfe654764224018a1ff5e5d3fc633be73f71f0c8c78`,
owner `89af092923ad49e3b9916b0c9f28c6b9237d8fcab4461cb7ddd12afe354f0d68`,
relay `wss://buzz.rowvia.ai:8443`, and Gmail account `jiri.hroza@gmail.com`.
Only `connector.grant` with exactly `gmail.message.read` and
`gmail.message.search` is supported. No Git resource or arbitrary action,
account, identity, target UUID, command, event body, selector or signer is accepted.

The unchanged outer envelope is `rowvia.buzz.owner-test/v1` with a fresh UUID
`requestId`, the existing fixed owner/Architekt/relay `expected` values, and:

| Operation | Exact arguments | Meaning |
| --- | --- | --- |
| `inspect_saved_connector` | `{}` | Inspect a genuinely mounted eligible selection or verified review. |
| `prepare_saved_connector` | `{}` | Select the one native-eligible fixed connection and click the real **Review proposal**, never Approve. |
| `approve_saved_connector` | `{"fingerprint":"<64 lowercase hex>"}` | Click the real enabled **Approve** once for the exact canonical receipt. |
| `inspect_saved_connector_operation` | `{"fingerprint":"<same receipt fingerprint>"}` | GET only that native-journal operation's status; never retry approval. |

The main-webview-only native context command accepts only `requestId`. Native
code obtains operation/arguments/deadline from an admitted pending ledger entry;
expired, completed, foreign, missing, cancelled or nonconnector requests cannot
use it. The context is resolved off the socket executor and rechecked before
return. Workspace apply is excluded before the identity lock, actual signing
keys and canonical relay are checked, and normal management build pins and
receipt verification are reused. Locks are released before UI/network work.
Existing management paths retain their own scope checks and immutable captured
keys; this helper does not replace or bypass them.

Prepare requires a real current pending Cerberus request in the mounted dialog,
fresh source event metadata, build-pinned signer identity, exact channel
`31986c38-6676-4d6f-a4ba-ed833926d0d5`, and one fresh active native candidate for
the fixed target/account. Names alone are never selection authority. The optional
`gmailLabels` hint should be omitted: the existing frontend hint parser rejects
email-like hints, and is intentionally unchanged. A supplied nonmatching hint
is refused. Selecting waits for the next committed React state before clicking
the real enabled **Review proposal**. The normal native create operation then
verifies the genuine event signature, routing, decryption, frame, expiry and
authoritative relay roster before its ordinary frozen-request/journal/HTTPS
flow. A prepare-click receipt is not proof that a proposal was accepted.

Inspection/approval require a fresh canonical receipt from the native journal
and the same handle, IDs and digest in the real, digest-verified, enabled review.
The strict four-field UI envelope remains unchanged; the opaque journal handle
stays in the existing `management.approve` closure, which captures the full
receipt before awaiting native IPC. The token hashes the native scope and exact
journal/receipt identities and raw-byte digests. Status and approval intent do
not change it, so status remains available after an unknown decision or expiry.
Recovered receipts prove prior normal native admission, not historical fixed
channel provenance: that channel is not stored in the journal. Responses state
this limitation explicitly rather than inventing a channel.

Committed dialog refs, scope and native deadline are checked after each await;
unmount/remount, item replacement, disabled/hidden controls, identity/community
drift and expiry fail closed. No callback or button from before selection is
retained. A source-event ID is consumed before prepare selection, and a receipt
fingerprint before approval click, across remounts and new socket request IDs.
The existing native ledger caches Unknown outcomes and refuses redispatch of
the same ID. An already-started action may finish after a timeout; a late reply
cannot convert cached Unknown to success. There is no automatic approval retry.
The durable native approval intent protects a later Desktop process as well.
`approvalClickDispatched` acknowledges only the click, never applied policy.
Status reports `policyApplied` and `runtimeReady` separately; false is not
promoted to readiness. The existing UI retry button is never used because it
may POST an approval again.

## Input dependencies and later deployment gate

Live acceptance needs the actual existing active binding whose native label is
the fixed account, a genuine current Cerberus observer request to the fixed
channel, owner/Cerberus authoritative membership, a current unlocked owner and
selected community, and the canonical proposal from the configured Rowvia
service. No missing UUID or signer is guessed; no synthetic test fixture may
be injected into live state. Zero/multiple matches stop the flow. Synthetic
IPC/decryption fixtures validate source boundaries, not live acceptance.

Before any deployment, independently review the frozen source and validations.
The ordinary `build-linux-desktop.sh --live --owner-test-hook --revision FULL_SHA`
archives a committed exact revision: current dirty source cannot enter it, and
Git authorization is a separate prerequisite. Use the proven pinned image and
shared capped runner, retain caches/sidecars and explicit memory/swap limits,
then inspect live provenance, ELF/hash and host loader results without launch.
Use the existing guarded Desktop-only v2 installer with an exact reviewed pin
and immediate verified ELF backup; no sidecar/config/credential replacement.
Preflight/install/rollback are later authorized actions, not commands run here.

Desktop shutdown explicitly terminates tracked local agent groups, including
Architekt, and terminal sessions. **Continuous PID preservation is impossible
with an ordinary restart.** Coordinate an idle/quiescent Architekt boundary,
record its public session/history coordinates and exact current installation,
and arrange an approved opaque state backup/recovery plan before stopping.
Preserve history, session identity and configuration; never clear/reset them.
Only then perform one deliberate guarded Desktop switch, revalidate retained
bindings/unlocked owner/relay and resume the same Architekt history/session
through its existing supported workflow. If those prerequisites cannot be
established, do not restart. On failure use the verified immediate backup and
the same guarded rollback; do not broaden cleanup or replay unknown actions.

All source checks use the existing clean-environment `run-capped-check` shared
5 GiB/zero-swap lock. Focused frontend tests exercise the actual mounted dialog,
hook, strict schemas and typed IPC boundary; native tests bind the production
journal/receipt decoder and native ledger deadline/Unknown behavior. Neither
these tests nor a successful compile proves a live grant, real UI deployment,
Gmail access, runtime readiness or preserved Architekt recovery.

Author validation: full desktop `tsc --noEmit` and scoped Biome passed. The
six focused frontend files passed 90 tests, including 15 full-dialog cases,
strict protocol and existing hook/API/verifier regressions. The owner-feature
native test binary compiled with pinned Rust 1.95.0 in the existing image
`sha256:b30f21ab265a10892f7a77273440f6eeee7996f0108cf7df2e622965b435d77c`;
all 31 `owner_test_` tests passed (3,467 others filtered out), including the
previous 28 and the new production decoder/candidate and live-ledger gates.
The single bounded epoch took 2m30s to compile and 1.75s to run; its exact owned
container was verified absent after completion. Existing caches, permissions,
lockfiles, saved-developer helpers and strict review schema remain unchanged.
Removing the production consumed-fingerprint guard made the real remount/replay
test fail as expected; the guard was immediately restored and the focused suite
passed again. This mutation observation qualifies that guard only.
