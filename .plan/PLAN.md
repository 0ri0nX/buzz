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

### Native upgrade checkpoint — 2026-10-09

- Application source is committed765e7fd5314e731c4b4a73f230989d1f599e0c11; coherent build/install tooling is separately reviewed, tested, committed and pushed as5b62c727f1af2c0c8197adadfd0e36bbf8f5c466. Draft fork PR #9 is not human/live acceptance.
- Actual frozen pnpm install, TypeScript/Vite protected-feature matrix and live owner-hook Desktop0.5.27 build passed. Independent capped artifact custody/ELF/host loader/provenance checks accepted Desktop SHA2563f1e7e0d93d648581f154da984c8560760f9cd8dac47ef9f48d8f1faea51c6ea. The native Tauri build did not explicitly pass Cargo--locked; three archived lock hashes matched the selected source after resolution.
- Actual six-sidecar build is running from the same765 application source and pinned Rust1.95 image/cache, using one Cargo job, 5GiB RAM/zero swap, the shared serial cap and25GiB combined scratch/cache budget. Final coherent artifact acceptance and focused frontend units remain pending; no full upstream suite/Tauri unit execution is claimed.
- The approved Core launcher3704c2174fe1fe5e2af50cd80c369ae4794e906d is deployed byte-for-byte to the workspace launcher after a private verified preimage backup. No Desktop restart, native-root switch, relay/relay-DB/Paseo/keyring change or live acceptance has occurred at this checkpoint.

## Done
