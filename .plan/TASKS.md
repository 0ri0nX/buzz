# TASKS

## Active

- [-] BUZZ-527-LIVE — Installed Desktop passes native owner/relay readiness and keyring metadata checks; interactive GUI/TTC/owner-operation acceptance still awaits human observation. Draft PR remains draft.

## After MVP

- [ ] BUZZ-527-OPAQUE-READERS — Assess stronger outside-cgroup reader fencing only if the threat model requires it. Current upgrade uses stopped-unit/cgroup and readable selected-executable checks on a trusted host; unrelated opaque same-UID agents do not block recovery.

## Done

- [x] BUZZ-527-BUILD — Same-source Desktop and all six sidecars passed independent artifact hashes, custody, ELF, host-loader and provenance checks. Author and independent verifier each passed 93 focused frontend tests; TypeScript/Vite and focused Rust checks passed. No full upstream/Tauri unit-suite claim.
- [x] BUZZ-527-DEPLOY — Corrected installer installed the coherent seven-ELF native-root-r2; actual Desktop/ACP hashes, CLI selection and cold-state backup independently verified. Owner/relay ready, keyring unlocked; relay/relay database/Paseo unchanged. First global process-inspection false refusal was corrected, reviewed and regression-tested before retry.
- [x] BUZZ-527-SOURCE — Merge official desktop-v0.5.27 and port required Rowvia ACP, management, enrollment, native owner hook, and focused tests; independent source review and frozen-scope gate passed. Build/deployment acceptance is recorded separately above; human live acceptance remains open.
