# PLAN

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

## Saved Codex owner registration — first functional slice

### Goal

Join Cerberus proposal, genuine Buzz owner approval, saved registration and
ordinary managed Codex Start/reply. Paseo retains provider/session lifecycle;
Rowvia supplies Memory, connectors, credentials and grants. Claude is deferred.

### Scope

- Port the existing reviewed Native/UI contract, not a second orchestrator or
  provider sandbox. Reuse the current fixed installation once; do not add a
  parallel manifest/evidence framework.
- Keep canonical owner review, consume-once registration, fresh read-only graph
  checks, uncertain-outcome refusal and stopped completion before separate Start.
- Preserve unrelated dirty owner-hook and diagnostic work. Original Architekt
  remains running; component tests do not prove a new agent's live acceptance.

### Approach

1. Promote the existing bounded FD/Native and owner-review UI seams.
2. Independently review critical boundaries, run focused actual-checkout tests,
   then build/install the committed Desktop through the existing tooling.
3. Exercise the genuine first owner flow before extending the same agent to
   progress/deduplication, development, Memory, GitLab268/revocation and restart.

### Implementation Notes

- Main register-once/read-only-inspect and development workspace recipe are
  committed separately. The actual held SDK can read stock Paseo0.11.1 status
  and zero agents; this is not provider execution or owner-flow acceptance.
- Promoted UI passes49 controls, TypeScript/Biome and fresh independent review;
  five regressions cover Cancel/recovery, late inventory and operation-bound
  guard release. Actual Native/Desktop interaction is still unverified.
- Fixed supplier literals and203 public source pins are filled and pass fresh
  physical admission review; no credentials or private auth files enter this
  pin table. A separate77-path Git-index audit found no dependency on excluded
  local diagnostic changes; this is source closure, not archive compilation.
- Replaced16 newly introduced raw-FFI unsafe sites with already locked safe
  nix/rustix APIs, preserving child FD0–4, nonreaped leader custody, group
  settlement and post-dispatch no-replay. New child-only Node bootstrap preserves
  fixed cwd/argv without changing the Desktop process working directory.
- Rust source formatting/syntax and independent safe-transport review pass. The
  first84-case gate reached its20-minute bound during dependency compilation;
  no tests ran, no OOM, owned container removed, source exact and cache retained.
  Compile-time constants are now split into a private819-line file, leaving
  the754-line parent. Equivalent existing tempfile/env fixtures satisfy the
  oversized-file ratchets; fresh source review passed. The bounded cached
  continuation reached Native compilation and failed on two Wry-only helper
  signatures and one unused reexport. No selected test ran; no OOM, exact
  source/cleanup checks passed and the dependency cache is retained. Fix only
  those signatures/export, independently review, then continue the86-case gate.
  That minimal fix passed fresh review and actual release libtest compilation
  in10m32s. All18 owner tests pass; the next host group passes four and fails
  four because its fixture opens /usr/bin/node, absent from the image. Remaining
  groups did not run; source exact, no OOM, owned container removed. Seven new
  visibility/dead-code warnings remain. Fresh review recommends
  narrowing visibility and removing uncalled alternate helpers, keeping all
  reachable retained-ACK/final-C2/CAS checks. No warning suppression or authority
  widening is admitted. These four-file edits are reviewed and staged. A finite
  probe passes with existing Node22 plus29 exact read-only public library files;
  no image core library or production pin changes. The reviewed next gate
  compiles once, selects the exact Cargo-reported binary and runs the unchanged
  86 controls under the same20-minute cap.
- Saved Start omitted normal channel membership: six regressions reproduce it.
  A narrow repair attaches the exact reviewed key/channel through the existing
  owner/relay-scoped API before explicit Start, preserving stopped state on
  attachment failure. All49 UI/API controls pass; fresh review independently
  reran eight membership/Start cases without failure or skip.
- First-slice installation channel now matches existing Cerberus channelfd0;
  no operation/ACK was created under the superseded319 selection. All203 pins,
  Node and the updated seal passed fresh physical admission review. The overall
  size gate still reports an unrelated committed relayClientSession.ts violation;
  this change introduces no Native size violation. No Native build, installation, genuine owner
  approval, managed Codex model call or Buzz reply is claimed by this checkpoint.
- Final current-source Native gate passes: release libtest compilation14m36s,
  zero warnings/errors, all86 selected cases pass with zero failures/ignored
  cases and exact84+2 inventory. Actual pipe/bootstrap, descendant settlement,
  retained ACK, owner approval and CAS controls run. Corrected one new JSON
  fixture to include the existing required agent_args:[] field; no serde/default
  weakening. Test-only Node22 uses30 exact read-only public files; compile-once
  execution and multiline-output counting pass fresh control review. Exit0,
  no OOM, source66 unchanged and owned container removed. This is actual source
  qualification, not a deployable Desktop or genuine human workflow result.

## Done
