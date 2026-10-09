# External agent enrollment seam

This fork adds a one-time Desktop operation for an agent whose signer and runtime
stay outside Buzz. It does not import, export, or store the external secret key,
create a managed-agent record, or start ACP. The only durable Buzz write is an
owner-signed kind 30177 announcement in the active workspace's retention queue.

## Operator flow

1. Generate and retain the agent key **on the external host**. Bring only its
   64-character public-key hex to Buzz Desktop.
2. Open **Agents → Authorize external agent**. Enter a display name and that
   public key; select **Create signing challenge**.
3. On the external host, sign a private Nostr kind 1 event with the agent key:
   `content` is the challenge verbatim; `tags` contains exactly one
   `["p", "<owner public key shown by Desktop>"]`; `created_at` is the current
   Unix second. Do not publish this proof event. Paste its complete signed JSON
   into Desktop before the five-minute deadline.
4. Select **Authorize public key**. Desktop verifies the agent signature,
   challenge, owner binding, and time; consumes the challenge; queues the
   owner-signed kind 30177; and returns a public NIP-OA `auth` tag. Copy the
   tag to the external host. A failed attempt consumes the challenge, so begin
   again with a new one.
5. On the external host, publish a kind 0 profile signed by the **agent** key
   with exactly this one `auth` tag. The tag is restricted to `kind=0`. The
   profile must be the latest kind 0 for this key. Optionally select an existing
   stream in the same Desktop dialog and add this exact public key with bot role.
   That direct membership action works before discovery finds the kind 0 profile;
   it does not start ACP. The 30177 announcement itself grants no membership or
   availability evidence.

If the response is lost after Desktop queued the 30177 announcement, submit
the same challenge and signed proof again. If those were also lost, start a new
challenge for the same public key and display name and sign a new proof. Desktop
verifies either recovery path and returns a new public auth tag without changing
the existing 30177 policy or spawning anything. A different display name is
rejected. A pending external announcement is re-signed at publication time if
it aged outside the relay's safe timestamp window during an outage or restart;
only verified external-enrollment rows take this path.

The Desktop's owner key never appears in this flow as a secret. A returned
`auth` tag is public authorization evidence, not a signing key. Do not try to
extract the Desktop's key or pass the external agent's secret to Desktop.

## Patch custody

The implementation touches `desktop/src-tauri/src/app_state.rs`,
`desktop/src-tauri/src/commands/{mod.rs,external_agent_enrollment.rs,
external_agent_enrollment_tests.rs}`, `desktop/src-tauri/src/lib.rs`,
`desktop/src-tauri/src/managed_agents/persona_events{.rs,/tests.rs}`,
`desktop/src/shared/api/tauri.ts`, and
`desktop/src/features/agents/ui/{AgentsView.tsx,ExternalAgentEnrollmentDialog.tsx}`.
Replay these focused edits after rebasing the Buzz fork, then run the Rust
enrollment tests and Desktop typecheck. Once this work has an authorized commit,
regenerate the native-management patch and its `manifest.json` hash, commit, and
result-tree fields against that committed fork revision. The current manifest
describes the earlier management patch and must not be changed to guessed hashes.
