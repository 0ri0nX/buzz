# TASKS

## Active

- [!] BUZZ-OWNER-TEST-ACTIVATE — Build the qualified exact revision and activate only through the existing Desktop-only path; current protected Desktop generation cannot be restarted in this source unit. Actual owner send/readback and agent reply acceptance remain unverified.

- [-] BUZZ-527-LIVE — Installed Desktop passes native owner/relay readiness and keyring metadata checks; interactive GUI/TTC/owner-operation acceptance still awaits human observation. Draft PR remains draft.

## After MVP

- [ ] BUZZ-OWNER-TEST-RETIRE — Retire temporary owner-message/readback authority after autonomous pilot acceptance; never enable it in ordinary product builds.

- [ ] BUZZ-527-OPAQUE-READERS — Assess stronger outside-cgroup reader fencing only if the threat model requires it. Current upgrade uses stopped-unit/cgroup and readable selected-executable checks on a trusted host; unrelated opaque same-UID agents do not block recovery.

## Done

- [x] BUZZ-OWNER-TEST-READBACK — Added bounded scoped reply readback for the fixed pilot pairs and one-attempt socket cleanup after a real test failure plus separate inode-reuse probe. Author/Root frontend61/61, full tsc, focused Biome/Rust formatting and fresh source review pass; final actual native22/22 passes. No runtime activation.

- [x] BUZZ-OWNER-TEST-MESSAGE — Added the closed, expiring owner test send for two pinned pilot pairs; initial57 frontend controls and final61/61 plus actual native22/22 pass. Scope/nonce/unknown-outcome bounds remain; no runtime activation.

- [x] BUZZ-527-BUILD — Same-source Desktop and all six sidecars passed independent artifact hashes, custody, ELF, host-loader and provenance checks. Author and independent verifier each passed 93 focused frontend tests; TypeScript/Vite and focused Rust checks passed. No full upstream/Tauri unit-suite claim.
- [x] BUZZ-527-DEPLOY — Corrected installer installed the coherent seven-ELF native-root-r2; actual Desktop/ACP hashes, CLI selection and cold-state backup independently verified. Owner/relay ready, keyring unlocked; relay/relay database/Paseo unchanged. First global process-inspection false refusal was corrected, reviewed and regression-tested before retry.
- [x] BUZZ-527-SOURCE — Merge official desktop-v0.5.27 and port required Rowvia ACP, management, enrollment, native owner hook, and focused tests; independent source review and frozen-scope gate passed. Build/deployment acceptance is recorded separately above; human live acceptance remains open.
