# Temporary fixed Cerberus recovery Stop

Development feature `rowvia-owner-test-hook` only. Operation
`stop_cerberus_for_recovery` accepts exactly `{ "expiresAt": <Unix milliseconds> }`,
valid for at most two minutes. Existing same-UID, owner/architect/relay, unlock and
nonce-ledger admission apply. Target is only
`9d3972d809d53ce8a87003ca7f631009963d9414739188cea6209ced88365c87`
on `wss://buzz.rowvia.ai:8443`. No caller selects a process, command, path or config.

The fixed adapter holds workspace→identity→transition and calls the existing
guarded normal Stop (store→runtime). It requires one unique local saved target,
one exact tracked pair and no scalar-PID mismatch. The guard repeats after normal
runtime synchronization, so an exited/missing pair cannot become legacy cleanup
or a false stopped proof. Scope, expiry and hook deadline/cancellation are checked
after waits and before termination. Configuration fields, rights and autostart
are not rewritten; ordinary Stop retains its normal status/storage bookkeeping.
Its existing post-Stop summary may merge builtin definitions; the projection
adds no such pre-Stop materialization.

Best-effort normal descriptor resolution over read-only saved definition views
(no pre-Stop builtin merge/retirement) provides a **current saved configuration**
pointer, not attestation of the old process's argv. If saved definition/config
reads or effective harness resolution fail, the optional launcher projection is
`{outerProgram:null,agentProgram:null,sourcePaths:[],configPaths:[]}` and the
authorized guarded normal Stop continues. Authority, ownership, scope, expiry,
lock and post-synchronization rechecks still apply. Layered environment,
resolution errors and the full normal summary/errors are dropped, never
serialized. Success has only:

```text
pubkey, relayUrl, stopCompleted:true, trackedChildPid, trackedChildSettled:true,
launcherBasis:"current_saved_effective_config",
launcher:{outerProgram:string|null,agentProgram:string|null,
          sourcePaths:string[],configPaths:string[]}
```

Programs are limited to public executable roots or fixed ordinary interpreter/ACP
names. Source paths are bare absolute lexical-canonical `.py/.mjs/.sh` arguments;
unknown long-flag values are withheld. Config paths must be exact `--profile` or
`--config` values (separate or `=` form) ending `.json/.toml`. Only workspace and
`/var/tmp/rowvia-management-pilot-v75-1000` roots are admitted. Arrays have at most
eight entries, paths at most1024 bytes. Controls, dot segments, URLs, shell strings
and auth/key/token/credential/private/scanner/.env-looking paths are withheld.
Referenced paths are never statted, resolved through symlinks or read. Null/empty
projection fields mean unavailable or withheld, not absence of a launcher/route.

Success means the normal tracked-child termination/wait and bookkeeping returned
success. It does **not** prove complete descendant/process-generation drainage,
credential revocation, recovery publication, readiness or safe restart. Root must
independently verify generation/descendant settlement and consumer routing.
Failure may occur before termination or after termination during normal save or
summary. Such uncertain outcomes remain consumed Unknown; never blindly retry.
The operation does not start, configure, adopt, recover or publish anything.

`inspect_cerberus_recovery_state` is a separate fixed expiresAt-only semantic
inspection using the same owner/relay/deadline guards and canonical locks. It
does not call Stop or waive its guards. Closed result fields are only:

```text
pubkey, recordState:missing|unique|ambiguous, recordKeyExact:bool,
localBackend:bool, recordRelay:empty|fixed|other|unavailable,
pairState:missing|unique|ambiguous, pairScopeMatches:bool,
scalarPidState:absent|matches|other|unavailable, stopPreflightMatches:bool
```

The final boolean calls the actual strict Stop preflight. An empty saved relay
remains reported as `empty`; the fixed adapter accepts empty OR exact fixed saved
relay only when the enclosing fixed owner/active-workspace scope, unique exact
fixed-relay tracked pair, and scalar-PID consistency all pass. Normal Stop resolves
the active workspace relay and ignores legacy stored pins, but this adapter stays
stricter: every nonempty foreign value (including whitespace or trailing-slash
variants) is refused, not normalized. No stored relay is rewritten. The guard is
still repeated after runtime sync and authorization/expiry waits, before normal
Stop. This correction does not reopen or retry any consumed Unknown. No raw row,
PID, environment, paths, summary, errors or credentials are returned. Normal
loading may hydrate/migrate internal metadata, so this is not a zero-write claim;
inspection itself never reaps, stops, starts or publishes. The observations are
not settlement/readiness/recovery authorization and do not replay an Unknown.
Main's existing runtime-inspection caller offers fixed `--cerberus-recovery-state`
and `--claude-initial-state` modes with the same closed validation, not new tools
or caller-selected targets. Existing generic inspect/ensure modes are unchanged.

Tests use disposable app data and owned inert process groups, never actual Desktop,
provider, IPC, credential state or projected files. Native checks use Main's shared
5GiB/swap0 capped runner and the pinned offline, unprivileged Docker recipe.
