# Dedicated saved-developer delivery policy

Policy identifier: `eose-live-only-fail-closed-v1`.

This source adds `--startup-live-only` / `BUZZ_ACP_STARTUP_LIVE_ONLY=true`,
default off. Activation is intended only for a separately built and pinned ACP
executable assigned to the one saved-developer target. It does not select,
replace, install, or enable any artifact, including the retained/global sidecar.
Ordinary ACP subscriptions and worker recovery keep their existing behavior.
The dedicated hook must invoke the ordinary continuing ACP service. The isolated
`buzz-acp run --task` CLI accepts service options but does not run this service
gate; using this policy through that subcommand is unsupported.

The option requires `--lazy-pool`, exactly one explicit canonical lowercase,
hyphenated channel UUID in `--channels`, no initial message, and heartbeat
interval zero. Discovery and subscription rules must resolve exactly that
channel, or startup fails. The normal relay authentication, discovered membership
and channel metadata, signed-event verification, owner/sibling authorization,
and encrypted observer-control checks still apply. Dynamic membership notification
subscription is omitted because this mode cannot expand beyond its selected
channel; initial membership discovery and channel authorization are retained.
Auxiliary observer-control subscriptions remain distinct and cannot open the gate.

The gate is closed before buffered authentication frames are handled. It starts
Unarmed and binds one successfully written channel REQ to the selected UUID,
a fresh unpredictable subscription ID, and the authenticated connection generation.
Only that subscription's timely EOSE transitions AwaitingEOSE to Live. Premature,
wrong, stale, and duplicate EOSE cannot open or reopen it. The startup deadline is
30 seconds after the authenticated socket is handed to the background task;
initial connection/authentication retain their own bounded timeouts.

All selected-channel EVENT delivery before EOSE is conservatively suppressed,
including live frames interleaved with history. Suppression precedes signature
verification work, deduplication, replay-watermark/last-seen updates, the harness
queue, seen reactions, and any ACP/model wake. These events remain on the relay.
Online presence, listening lifecycle, and startup readiness wait for the actual
REQ/EOSE witness; enqueueing a Subscribe command is insufficient.
Each readiness boundary checks the current connection state again; it cannot
promise that the socket will remain connected after the check.

EOSE is a delivery boundary, **not proof of signed `created_at` freshness**.
A valid old-timestamp event delivered after EOSE follows normal processing.
Events arriving live before EOSE are intentionally missed by this process.
A replay floor of now is insufficient for this guarantee because normal REQ
construction subtracts five seconds; the gate does not rely on that timestamp.

Missing EOSE, CLOSED, disconnect/pong timeout, any failed WebSocket write, buffer overflow,
an attempted second channel REQ, or explicit reconnect terminates the gated socket.
There is no autonomous reconnect, resubscribe, targeted overflow recovery, or
second selected-channel REQ. Failure remains terminal after reaching Live.
The harness exits through its existing bounded worker/task shutdown cleanup.
An explicit restart starts a new closed gate; it does not replay suppressed history.

This policy also terminates the outer harness on a non-success worker result,
worker task panic, or failed lazy pool wake, before taking/requeueing the uncertain
batch or spawning a replacement worker. This includes timeout, transport/protocol
failure, cancellation, and indeterminate project context. Successful turns retain
normal pool operation. No relay record or external journal is deleted or reset.
The returned worker stays owned for awaited shutdown. A connection-state fence
also runs after asynchronous ingress authorization, before queue/seen/dispatch effects.
Failures retain bounded process diagnostics and a static `turn_error` observer
event for non-success results. They bypass the stock automatic retry/dead-letter
failure-notice path; delivery of an observer event before shutdown is not guaranteed.
This source does not claim a new human-visible failure notification contract.

Desktop's normal outer-ACP exit monitor records stopped/error and does not itself
crash-respawn that process (`managed_agents/runtime/lifecycle.rs`). App-launch and
configuration restart controls are separate. Stock inner-worker refill and failed
batch retries do exist; the opt-in early terminal paths prevent those error retries
for this target. This is a continuing service for future post-EOSE work, not a
finite one-task wrapper. No Native installation or readiness is proven by unit tests.

Environment canaries exercise the existing custom ACP worker spawn boundary:
`NOSTR_PRIVATE_KEY`, `GIT_CONFIG_PARAMETERS`, and the host-only
`BUZZ_ACP_LAUNCH_PREFIX` control are stripped there, including inherited canaries.
This is not a claim that the Bridge parent environment is clean: it intentionally
retains Native authorization. Core service/Bridge allowlists are separate boundaries
and are not changed here. No real secrets, real provider, or real relay are needed
for these synthetic tests.
