# PLAN

## Bounded existing-channel owner test message

### Goal / Scope

Extend only the opted-in temporary owner hook with `send_existing_test_message` and `read_existing_test_channel` for the source-confirmed Developer and Cerberus channel/recipient pairs. Reuse the normal native signer, scoped history reader, relay enforcement and request ledger. The explicit readback is needed because existing response readers cannot access these channels through the generic hook registry. No key export, generic invoke/sign API, permission bypass, live sends, process restart or deployment.

### Approach / Validation / Risks

- Pin both pairs in the strict TS and Rust contracts; use requestId as the nonce and existing ledger as the one-attempt boundary. Accept only root messages beginning `ROWVIA_E2E_`, at most 1024 UTF-8 bytes, with a future expiry no more than two minutes away.
- Precheck both members through the normal roster read, recheck owner/relay and expiry before the scoped native send. Failures after send begins remain unknown and never trigger a hook retry.
- Readback uses the same fixed pairs and membership checks, at most 20 kind-9 events with optional `since`, the existing scoped history reader and bounded response validation. It returns signed event fields for separate Core reply verification without registering channels or thread provenance.
- A concrete native-test failure is consistent with Root's separate owned socket probe, which observes inode reuse8/8. Make `SocketGuard` cleanup atomic and one-attempt across stop/serve/Drop; retain the original replacement test and add a deterministic regression. This does not establish the failed run's original inode values or make the first metadata-check/unlink atomic.
- Validate synthetic TS handler/schema tests, Rust validation/ledger tests, focused lint/typecheck/format under the capped runner. Native activation and actual relay acceptance require a separate executable/process and remain outside this source task; independent boundary review follows.

### Implementation Notes

- Implemented the strict TS/Rust operation and exact pairs. The handler checks both members through the existing roster wrapper, then uses the ordinary scoped native root-message path; existing channel access, thread provenance, hook flags and private socket transport are unchanged.
- Added `read_existing_test_channel` with the same pinned scope/pairs and membership checks, limit 1–20 and optional `since`. It shares the unchanged scoped kind-9 history/response checks with `read_channel`, preserving signed reply envelopes for Core verification without granting generic channel access.
- Added one-attempt atomic socket cleanup and a deterministic matching-metadata/Drop regression, preserving the original replacement test apart from required constructor initialization. Only new native tests moved into `owner_test_hook_message_tests.rs`, included inside the existing test module; final native files are below the size ceiling. The initial check/unlink race remains outside this narrow correction.
- RequestId deduplication is process-local. Callers must retain attempted IDs and never replace a nonce after an unknown outcome, including across restarts. Expiry uses Unix milliseconds and gates admission/dispatch, not the settlement of an already submitted event.
- Under the shared 5 GiB/zero-swap runner: new send/readback tests failed before implementation; author and Root repeat61/61 focused frontend tests with zero skips and full Desktop `tsc --noEmit`; focused Biome and both Rust formatting checks pass. The author's initial offline Cargo dependency miss is resolved only inside the existing isolated public-dependency phase. Root's first actual hook run compiles and passes19/20, exposing the old replacement-cleanup failure; final five-file packet compiles in1m42s and passes22/22 actual native tests, zero failed/ignored,3495 filtered. The pinned image, network-none/read-only source, synthetic HOME/TMPDIR, phase-only caches,5GiB/no-extra-swap and stopped/no-OOM container custody are verified. Fresh independent source review approves the final packet. This is not whole-suite/strict-Clippy, literal owner IPC, signatures or live relay acceptance.
- Earlier Desktop file-size ratchet fails only on untouched `managed_agents/owner_test_saved_developer.rs` (1507/1500) and `relayClientSession.ts` (1261/1200); this patch's native files1395/219 remain below1500. Default Cargo/frontend opt-ins, same-UID socket, fixed scopes and ordinary signing remain intact; no key export exists.
- No live hook calls, relay sends, key access, deployment, Desktop restart or protected-session interaction occurred. Release assets/native allowlist are compiled, so activation requires a new exact-revision build and Desktop-only replacement/restart. The existing protected Desktop generation is not restarted by this source unit; activation remains separately unqualified.
- Subsequent owner instruction explicitly removes restart protection for this implementation. Root starts the existing capped live/owner-hook builder from exact committed35ad4cb8, not dirty source. Fresh read-only reconnaissance confirms Desktop539064/start50842162 runs installedb7e08522 and all retained sidecars/bindings match the priorv2 pin. Use a new current-baseline pin, immediate Desktop-only backup and ordinary installer rechecks/rollback; preserve original Architekt/native unless necessary. Runtime tests use the existing app-internal signer and signature-verifying readback, never a copied owner key.
- Activation complete: capped build16m28s, canonical live/owner-hook provenance and host ldd-r pass; exact ELF SHA256dac14066bb899df2487e605061a16a5d478602b2be3e01231e65363f8ccfa297. Installer mocks, actual preflight and fresh deployment-data review pass. Desktop674570/invocation8ea619cb1a6b46dc84019d018e54ed5f runs that hash; immediate backup `backup-fujy4vsj` retains baselineb7e08522 and the exact new v2 pin. Pin is `/home/orionx/rowvia-owner-message-switch.1rsSS1zH/desktop-pin.json` (SHA4c764c1fadd5f618d4f7c468cc9ff389545951b654927d96df18719e31e84d9a). Existing installer rechecks retained sidecars/bindings across stop/start; original Architekt219155/invocation96f8325fe5354c378ab048c9846b3e41 and Native1661814 remain alive unchanged. Owned build container/scratch are absent.
- Actual bounded owner socket send/read works: two distinct Cerberus roots47504c30/e4d5ebd0 and3db61f8d/1dfebe21 each verify real event signatures and exact owner/recipient/channel/thread/content bindings through Core client8e5cdad. Attempting the first send again refuses before transport. Durable receipts remain in `/var/tmp/rowvia-owner-pilot-live.MB2DJDfe`; no uncertain resend, key export, connected-content write or human UI acceptance is claimed. Normal Developer capsule inspection still refuses core_expired; no new Developer turn or runtime restart is attempted. Its acceptance awaits the separately qualified ordinary renewal path.

## Port Rowvia Buzz fork to desktop-v0.5.27

### Goal

Integrate the exact upstream `desktop-v0.5.27` tag (`7a2fb3cc2d401e91e67d9819741d9025278cbec0`) while preserving the existing Rowvia production/native owner hook, owner-signed management, external-agent enrollment, and trusted-turn-context behavior.

### Scope

- In: merge and resolve upstream conflicts in this isolated branch; keep necessary Rowvia Rust, Tauri, desktop UI, and test automation; adapt obsolete test-only assumptions where upstream changed an equivalent contract.
- Out: unrelated features, schema/API redesign, broad dependency upgrades, changes to primary Buzz/Core clones, installer/build-script repins (separately owned), live Desktop/relay/database/config changes, and deployment in this SOURCE task.

### Pre-change state

- Current fork HEAD `3afd3c397` carries the Rowvia customizations; the fetched official target tag is `desktop-v0.5.27`.
- The current native owner-hook launch is opt-in and Core-controlled. Stock Buzz already root-anchors human-facing replies; no UI thread patch is needed.
- Existing build/install runbooks pin older revisions and are reserved for a separate worker after the source port.

### Approach

1. Merge the exact upstream tag without committing. Resolve each conflict by retaining upstream release behavior unless it would erase a required Rowvia owner/management/enrollment/TTC boundary. Keep generic stock behavior separate from Rowvia opt-in paths.
2. Review the merged diff for lost Rowvia entry points and tests. Adjust only concrete release incompatibilities (including test-only hardcoded enum assumptions when upstream semantics are equivalent); do not replay old patches wholesale.
3. Run narrow, serialized, resource-capped offline checks against the affected Rust/Tauri/desktop areas and document untested areas honestly. Freeze source for fresh independent review and final build gate.

### Validation

- [ ] Exact tag merge and conflict-resolution review; no missing required Rowvia entry points or unintended release drift.
- [ ] Focused Rust/Tauri/desktop tests and lint/typecheck where warm offline tooling supports them, always under the Core 5 GiB/zero-swap capped runner.
- [ ] Fresh review and independent final SOURCE gate before any commit or deployment; separately validate native artifacts and live owner workflow after rollout.

### Risks / Open Questions

- Upstream may have changed the same ACP queue, CLI command enum, or Desktop launch/UI surfaces; preserve the actual boundary semantics, not obsolete line-level patches or enumeration counts.
- A compile/test pass does not prove native owner workflow or safe deployment. Build provenance, installed pair, and human/live behavior remain separate acceptance gates.

### Implementation Notes

- Merged the exact official `desktop-v0.5.27` tag without committing. Resolved seven content conflicts in ACP documentation/queue/runtime integration, CLI test expectations, scoped channel readback, and relay publish cancellation; the merge has no unresolved paths.
- Preserved the Rowvia trusted source facts alongside upstream edit routing through admitted listener events, native steering, batching, and both requeue paths. The upstream isolated task path cannot claim a Buzz source: its separate ACP initialization does not offer trusted turn context or accept an unsolicited capability response.
- Kept upstream command/API additions while retaining owner-signed management, external enrollment, and opt-in native owner-hook paths. The CLI count expectation reflects the actual merged command set, not a pinned pre-release enumeration.
- Author checks under the shared 5 GiB/zero-swap runner: offline locked `buzz-acp` library check passed; focused ACP trusted ingress (3), isolated task prompt (1), edit/requeue (1), and `buzz-cli` command count (1) tests passed. Rust formatting and staged/unstaged diff whitespace checks passed. Exact Cargo git dependencies were fetched under the cap once before offline tests.
- Desktop Node dependencies are absent in this isolated worktree, so frontend tests/typecheck were not claimed by this author checkpoint. Independent source review, final gate, native build/install, and live UI/owner observation remain separate.
- Fresh Sol source review approved the frozen 13-file packet, including the isolated-task correction. Luna independently verified the hashes, absence of unmerged paths, scoped whitespace checks, and capped Rust formatting. This completes the source checkpoint only; frontend/Tauri execution, artifacts, deployment, and human workflow confirmation are not inferred.

### Pre-deployment checkpoint — 2026-10-09 (historical, superseded below)

- Application source is committed765e7fd5314e731c4b4a73f230989d1f599e0c11; coherent build/install tooling is separately reviewed, tested, committed and pushed as5b62c727f1af2c0c8197adadfd0e36bbf8f5c466. Draft fork PR #9 is not human/live acceptance.
- Actual frozen pnpm install, TypeScript/Vite protected-feature matrix and live owner-hook Desktop0.5.27 build passed. Independent capped artifact custody/ELF/host loader/provenance checks accepted Desktop SHA2563f1e7e0d93d648581f154da984c8560760f9cd8dac47ef9f48d8f1faea51c6ea. The native Tauri build did not explicitly pass Cargo--locked; three archived lock hashes matched the selected source after resolution.
- Actual six-sidecar build is running from the same765 application source and pinned Rust1.95 image/cache, using one Cargo job, 5GiB RAM/zero swap, the shared serial cap and25GiB combined scratch/cache budget. Final coherent artifact acceptance and focused frontend units remain pending; no full upstream suite/Tauri unit execution is claimed.
- The approved Core launcher3704c2174fe1fe5e2af50cd80c369ae4794e906d is deployed byte-for-byte to the workspace launcher after a private verified preimage backup. No Desktop restart, native-root switch, relay/relay-DB/Paseo/keyring change or live acceptance has occurred at this checkpoint.

### Deployment checkpoint — 2026-10-09

- Desktop and all six sidecars from application source `765e7fd5314e731c4b4a73f230989d1f599e0c11` passed independent hash/custody/ELF/host-loader/provenance checks. Author and independent verifier each passed 93 focused frontend tests. Full upstream and Tauri unit suites were not run.
- First install stopped before cold snapshot or ACP/CLI selection changes because the process guard rejected unrelated inaccessible same-UID host agents. The verified old Desktop was restored with owner/relay ready. Narrow correction `3fd6552e6bbcea5fa43f62389362c4bc5f4a607d` was independently reviewed, regression-tested and pushed; stopped-unit/cgroup and readable selected-reader checks remain. Opaque outside-unit readers are an explicit trusted-host limitation, not an MVP blocker.
- Reviewed retry pin `/home/orionx/rowvia-buzz-native-v0.5.27-20261009/pin-r2.json` selected the previously absent `native-root-r2`. Actual corrected install succeeded; Desktop PID 539732 hashes to the accepted artifact, all seven installed binaries/custom ACP/CLI selection and cold backup were independently verified. Two Desktop-descendant ACP processes match the accepted ACP binary.
- Root post-install native status returned `validated-ready`; keyring metadata is unlocked. Actual rollback custody is `/home/orionx/.local/state/rowvia-buzz-native-install/backups/backup-ig8j37i9`, validated without restoring or exposing payloads. Old root is retained, shared keyring is untouched; relay/relay database/Paseo were not changed.
- BUILD and DEPLOY are Done. Interactive GUI/TTC/owner-operation observation remains separate and open; PR #9 remains draft pending human confirmation.

## Done
