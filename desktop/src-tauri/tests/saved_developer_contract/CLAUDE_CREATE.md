# Temporary stopped Claude creation

The opt-in `rowvia-owner-test-hook` adds native operation
`create_stopped_claude_developer`. Its only argument is `expiresAt`, an integer
Unix timestamp in milliseconds strictly after now and at most 120 seconds away.
The existing same-UID socket, fixed expected owner/Architect/relay envelope,
unlocked identity and actual selected community guards apply. The native path
checks actual scope and expiry again under normal creation's final store lock,
then under the acquired catalog lock after reads and immediately before its first
write, and again after registration before committing the saved record.

It creates one normal local saved agent named `Rowvia Developer Claude`, without
a linked persona or custom provider/model configuration. Its fixed custom ACP
harness catalog entry is `rowvia-saved-claude-normal`, running `/usr/bin/python3.13` with exactly
`-IS /home/orionx/.local/state/rowvia-saved-claude-normal/buzz-worker/run`.
Normal creation saves a raw executable pin and these explicit arguments, with
`runtime: null`: this API has no runtime-ID field. The matching registered harness
is catalog metadata, not a structural link on the saved record; the exact effective
descriptor and argv are authoritative. Both immediate spawning and app-launch
autostart are false. Harness registration
warms the ordinary static catalog without command discovery, auth/install probes
or execution. The separately reviewed capsule must exist before any later start.

Existing matching names, references to this harness, or the unique fixed capsule
argument refuse creation under the
store lock. Existing fixed harness files/IDs also refuse registration; nothing
is adopted or overwritten. A failed creation can leave a registered harness, and
normal creation retains its ordinary saved policy, Nest and profile-sync effects.
The owner hook adds no durable receipt or retry mechanism. The operator must
reserve a durable one-shot intent before requesting creation and must treat an
unknown/timed-out outcome as consumed, including across app restarts.

Successful results contain exactly `pubkey`, `name`, `createdStopped`,
`startOnAppLaunch` and `profileSyncSucceeded`. Native code drops the normal
secret-bearing create response before building this projection; no nsec, auth
tag, environment, raw error or full agent summary reaches the hook serializer.
Success means stopped creation only: it does not attest channel membership,
capsule qualification, authentication, readiness or a running agent. Enrollment,
configuration and start are separate operations.
