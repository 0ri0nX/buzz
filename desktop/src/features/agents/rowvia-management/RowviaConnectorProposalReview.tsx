import { useEffect, useState } from "react";
import { z } from "zod";
import { Button } from "@/shared/ui/button";

// Native response fields stay snake_case. Only the decoded, digest-verified
// expires_at is added when calling the native approval command.
const envelopeSchema = z.strictObject({
  proposal_id: z.string().uuid(),
  operation_id: z.string().uuid(),
  proposal_digest: z.string().regex(/^[0-9a-f]{64}$/),
  proposal_bytes_b64: z.base64().min(1).max(524_288),
});
const uuid = z.string().uuid();
const pubkey = z.string().regex(/^[0-9a-f]{64}$/);
const proposalSchema = z.strictObject({
  version: z.literal(1),
  tenant_id: uuid,
  proposal_id: uuid,
  operation_id: uuid,
  owner_subject_id: uuid,
  owner_pubkey: pubkey,
  action: z.enum(["connector.grant", "connector.revoke"]),
  agent_name: z.string().regex(/^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/),
  source_instance_id: z.string().regex(/^[a-z0-9][a-z0-9._-]{0,63}$/),
  target_pubkey: pubkey,
  agent_identity_id: uuid,
  workload_principal_id: uuid,
  binding_id: uuid,
  provider_subject: z.string().min(1),
  constrained_resource: z.string().regex(/^gmail-account:[^\s@]+@[^\s@]+$/),
  binding_generation: z.number().int().nonnegative(),
  credential_generation: z.number().int().nonnegative(),
  policy_generation: z.number().int().nonnegative(),
  access_policy_generation: z.number().int().nonnegative(),
  access_policy_state: z.string().min(1),
  manual_target_revision: z.number().int().nonnegative(),
  operations: z.tuple([
    z.literal("gmail.message.read"),
    z.literal("gmail.message.search"),
  ]),
  expires_at: z.string().datetime({ offset: true }),
});

export type RowviaConnectorProposalEnvelope = Readonly<
  z.infer<typeof envelopeSchema>
>;
export type RowviaConnectorDecision = Readonly<
  RowviaConnectorProposalEnvelope & { expires_at: string }
>;
export type RowviaConnectorProposal = Readonly<z.infer<typeof proposalSchema>>;
export type RowviaProposalStatus = "pending" | "loading" | "stale" | "error";

export interface RowviaConnectorProposalReviewProps {
  /** Opaque native response. No chat or model content belongs here. */
  proposal: unknown;
  status: RowviaProposalStatus;
  onApprove: (decision: RowviaConnectorDecision) => void;
  onReject: (proposal: RowviaConnectorProposalEnvelope) => void;
}

async function verifyProposal(
  envelope: RowviaConnectorProposalEnvelope,
): Promise<RowviaConnectorProposal | null> {
  try {
    const binary = atob(envelope.proposal_bytes_b64);
    const bytes = Uint8Array.from(binary, (character) =>
      character.charCodeAt(0),
    );
    const digest = await crypto.subtle.digest("SHA-256", bytes);
    const actualDigest = Array.from(new Uint8Array(digest), (byte) =>
      byte.toString(16).padStart(2, "0"),
    ).join("");
    if (actualDigest !== envelope.proposal_digest) return null;
    const json: unknown = JSON.parse(
      new TextDecoder("utf-8", { fatal: true }).decode(bytes),
    );
    const parsed = proposalSchema.safeParse(json);
    if (
      !parsed.success ||
      parsed.data.proposal_id !== envelope.proposal_id ||
      parsed.data.operation_id !== envelope.operation_id ||
      !Number.isFinite(Date.parse(parsed.data.expires_at))
    )
      return null;
    return parsed.data;
  } catch {
    return null;
  }
}

type Verification = {
  envelope: RowviaConnectorProposalEnvelope;
  proposal: RowviaConnectorProposal | null;
};

export function RowviaConnectorProposalReview({
  proposal: input,
  status,
  onApprove,
  onReject,
}: RowviaConnectorProposalReviewProps) {
  const parsed = envelopeSchema.safeParse(input);
  const envelope = parsed.success ? parsed.data : null;
  const [verification, setVerification] = useState<Verification | null>(null);
  const [nowMs, setNowMs] = useState(() => Date.now());
  const encoded = envelope?.proposal_bytes_b64;
  const expectedDigest = envelope?.proposal_digest;
  const proposalId = envelope?.proposal_id;
  const operationId = envelope?.operation_id;

  useEffect(() => {
    if (!encoded || !expectedDigest || !proposalId || !operationId) return;
    let active = true;
    const candidate = {
      proposal_id: proposalId,
      operation_id: operationId,
      proposal_bytes_b64: encoded,
      proposal_digest: expectedDigest,
    };
    void verifyProposal(candidate).then((proposal) => {
      if (active) setVerification({ envelope: candidate, proposal });
    });
    return () => {
      active = false;
    };
  }, [encoded, expectedDigest, proposalId, operationId]);

  const matches =
    envelope !== null &&
    verification !== null &&
    verification.envelope.proposal_id === proposalId &&
    verification.envelope.operation_id === operationId &&
    verification.envelope.proposal_bytes_b64 === encoded &&
    verification.envelope.proposal_digest === expectedDigest;
  const proposal = matches ? verification.proposal : null;
  const expiresAtMs = proposal ? Date.parse(proposal.expires_at) : null;
  useEffect(() => {
    setNowMs(Date.now());
    if (expiresAtMs === null || expiresAtMs <= Date.now()) return;
    const timeout = window.setTimeout(
      () => setNowMs(Date.now()),
      Math.min(expiresAtMs - Date.now(), 2_147_483_647),
    );
    return () => window.clearTimeout(timeout);
  }, [expiresAtMs]);

  const expired = expiresAtMs !== null && expiresAtMs <= nowMs;
  const canDecide = status === "pending" && proposal !== null && !expired;
  const statusMessage =
    !envelope || (matches && !proposal)
      ? "This proposal is unavailable or invalid. Refresh it before reviewing."
      : !matches
        ? "Verifying proposal…"
        : expired
          ? "This proposal has expired. Request a new proposal."
          : status === "loading"
            ? "Submitting your decision…"
            : status === "stale"
              ? "This proposal is stale. Refresh it before reviewing."
              : status === "error"
                ? "The proposal could not be reviewed. Try again."
                : "Pending your decision.";

  return (
    <section
      aria-labelledby="rowvia-proposal-title"
      className="rounded-xl border border-border bg-card p-4 text-card-foreground"
    >
      <h2 className="text-base font-semibold" id="rowvia-proposal-title">
        Rowvia Gmail access request
      </h2>
      <p
        aria-live="polite"
        className="mt-1 text-sm text-muted-foreground"
        role="status"
      >
        {statusMessage}
      </p>
      {proposal && (
        <dl className="mt-4 grid gap-3 text-sm sm:grid-cols-[max-content_1fr] sm:gap-x-4">
          <dt className="font-medium">Decision</dt>
          <dd>
            {proposal.action === "connector.grant"
              ? "Grant access"
              : "Revoke access"}
          </dd>
          <dt className="font-medium">Agent</dt>
          <dd>
            {proposal.agent_name}
            <span className="block break-all font-mono text-xs text-muted-foreground">
              Identity: {proposal.target_pubkey}
            </span>
          </dd>
          <dt className="font-medium">Agent instance</dt>
          <dd className="break-all font-mono">{proposal.source_instance_id}</dd>
          <dt className="font-medium">Approving owner</dt>
          <dd className="break-all font-mono">{proposal.owner_pubkey}</dd>
          <dt className="font-medium">Verified Gmail account</dt>
          <dd className="break-all">
            {proposal.constrained_resource.slice("gmail-account:".length)}
          </dd>
          <dt className="font-medium">Provider subject</dt>
          <dd className="break-all font-mono">{proposal.provider_subject}</dd>
          <dt className="font-medium">Binding ID</dt>
          <dd className="break-all font-mono">{proposal.binding_id}</dd>
          <dt className="font-medium">Current access policy</dt>
          <dd>{proposal.access_policy_state}</dd>
          <dt className="font-medium">Manual target revision</dt>
          <dd>{proposal.manual_target_revision}</dd>
          <dt className="font-medium">Operations</dt>
          <dd>Read and search Gmail messages</dd>
          <dt className="font-medium">Expires</dt>
          <dd>{new Date(proposal.expires_at).toLocaleString()}</dd>
        </dl>
      )}
      <div className="mt-5 flex flex-wrap justify-end gap-2">
        <Button
          disabled={!canDecide}
          onClick={() => {
            if (
              canDecide &&
              envelope &&
              proposal &&
              Date.now() < Date.parse(proposal.expires_at)
            )
              onReject(envelope);
          }}
          type="button"
          variant="outline"
        >
          Reject
        </Button>
        <Button
          disabled={!canDecide}
          onClick={() => {
            if (
              canDecide &&
              envelope &&
              proposal &&
              Date.now() < Date.parse(proposal.expires_at)
            )
              onApprove({ ...envelope, expires_at: proposal.expires_at });
          }}
          type="button"
        >
          Approve
        </Button>
      </div>
    </section>
  );
}
