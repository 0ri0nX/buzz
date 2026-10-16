import { z } from "zod";
import type { ConnectorDraftRequest } from "@/features/agents/agentManagement";
import type { ConnectorRequestEvidence } from "@/features/agents/observerRelayStore";
import { normalizeRelayUrl } from "@/features/communities/relayProbe";
import {
  getRowviaManagementOperation,
  type RowviaCandidates,
  type RowviaNativeReceipt,
} from "@/shared/api/rowviaManagement";
import { invokeTauri } from "@/shared/api/tauri";
import type { OwnerTestRequest } from "./ownerTestHookProtocol";

export const SAVED_CONNECTOR_TARGET =
  "29cc1bb0ac338520c263dcfe654764224018a1ff5e5d3fc633be73f71f0c8c78";
export const SAVED_CONNECTOR_ACCOUNT = "jiri.hroza@gmail.com";
const OWNER =
  "89af092923ad49e3b9916b0c9f28c6b9237d8fcab4461cb7ddd12afe354f0d68";
const RELAY = "wss://buzz.rowvia.ai:8443";
const CHANNEL = "31986c38-6676-4d6f-a4ba-ed833926d0d5";
const digest = z.string().regex(/^[0-9a-f]{64}$/);
const base = {
  ownerPubkey: z.literal(OWNER),
  relayUrl: z.literal(RELAY),
  targetPubkey: z.literal(SAVED_CONNECTOR_TARGET),
  gmailLabel: z.literal(SAVED_CONNECTOR_ACCOUNT),
  cerberusPubkey: digest,
  deadlineUnixMs: z.number().int().positive(),
};
const receipt = {
  fingerprint: digest,
  receiptHandle: z.string().uuid(),
  proposalId: z.string().uuid(),
  operationId: z.string().uuid(),
  proposalDigest: digest,
  expiresAt: z.string().datetime({ offset: true }),
  agentName: z.string().min(1),
  bindingId: z.string().uuid(),
  approvalIntent: z.boolean(),
  sourceEventId: digest,
};
const contextSchema = z.discriminatedUnion("stage", [
  z.strictObject({
    ...base,
    stage: z.literal("selection"),
    selectionHandle: z.string().uuid(),
    agentName: z.string().min(1),
  }),
  z.strictObject({
    ...base,
    ...receipt,
    stage: z.enum(["review", "operation"]),
  }),
]);
type Context = z.infer<typeof contextSchema>;
export type SavedConnectorDialogState = {
  ownerPubkey: string | undefined;
  relayUrl: string | undefined;
  open: boolean;
  phase: string | undefined;
  itemId: string | undefined;
  request: ConnectorDraftRequest | null;
  evidence: ConnectorRequestEvidence | null;
  receipt: RowviaNativeReceipt | null;
  candidates: RowviaCandidates | null;
  selectionHandle: string | null;
  dialog: HTMLDivElement | null;
};
let registration: { read: () => SavedConnectorDialogState } | null = null;
// Remounts/new socket IDs must not click the same canonical approval twice.
// A Desktop restart is additionally fenced by the existing durable intent.
const consumed = new Set<string>();
const prepared = new Set<string>();

/** Passive committed refs only; no IPC, reads or actions until the opted-in hook. */
export function registerSavedConnectorOwnerTest(
  read: () => SavedConnectorDialogState,
) {
  const entry = { read };
  registration = entry;
  return () => {
    if (registration === entry) registration = null;
  };
}

export class SavedConnectorOwnerTestFailure extends Error {
  readonly code: "not_ready" | "scope_mismatch";
  constructor(code: "not_ready" | "scope_mismatch") {
    super(code);
    this.code = code;
  }
}
function refuse(): never {
  throw new SavedConnectorOwnerTestFailure("not_ready");
}
function current(context: Context, entry: NonNullable<typeof registration>) {
  if (registration !== entry || Date.now() >= context.deadlineUnixMs) refuse();
  const state = entry.read();
  if (
    state.ownerPubkey !== OWNER ||
    normalizeRelayUrl(state.relayUrl ?? "") !== RELAY
  )
    throw new SavedConnectorOwnerTestFailure("scope_mismatch");
  if (
    !state.open ||
    !state.dialog?.isConnected ||
    state.dialog.getAttribute("data-state") !== "open"
  )
    refuse();
  return state;
}
function button(state: SavedConnectorDialogState, name: string) {
  const buttons = Array.from(
    state.dialog?.querySelectorAll("button") ?? [],
  ).filter((item) => item.textContent?.trim() === name);
  if (
    buttons.length !== 1 ||
    buttons[0].disabled ||
    buttons[0].closest('[hidden], [aria-hidden="true"]')
  )
    refuse();
  for (
    let element: HTMLElement | null = buttons[0];
    element;
    element = element.parentElement
  ) {
    const style = window.getComputedStyle(element);
    if (style.display === "none" || style.visibility === "hidden") refuse();
  }
  return buttons[0];
}
function pending(
  state: SavedConnectorDialogState,
  context: Extract<Context, { stage: "selection" }>,
) {
  const request = state.request;
  const evidence = state.evidence;
  if (
    state.phase !== "needs-selection" ||
    !request ||
    !evidence ||
    request.action !== "connector.grant" ||
    request.request.channelId !== CHANNEL ||
    request.request.targetName !== context.agentName ||
    (request.request.gmailLabels !== undefined &&
      (request.request.gmailLabels.length !== 1 ||
        request.request.gmailLabels[0] !== SAVED_CONNECTOR_ACCOUNT)) ||
    evidence.agentPubkey !== context.cerberusPubkey ||
    evidence.relayEvent.pubkey !== context.cerberusPubkey ||
    evidence.observerChannelId !== CHANNEL ||
    !/^[0-9a-f]{64}$/.test(evidence.relayEvent.id)
  )
    refuse();
  const age = Date.now() / 1000 - evidence.relayEvent.created_at;
  if (
    !Number.isFinite(age) ||
    age < -30 ||
    age > 300 ||
    state.candidates?.truncated !== false
  )
    refuse();
  const candidates = state.candidates.candidates.filter(
    (candidate) =>
      candidate.selection_handle === context.selectionHandle &&
      candidate.agent_name === context.agentName &&
      candidate.gmail_label === SAVED_CONNECTOR_ACCOUNT &&
      candidate.binding_status === "active",
  );
  if (candidates.length !== 1) refuse();
  return evidence.relayEvent.id;
}
function review(
  state: SavedConnectorDialogState,
  context: Exclude<Context, { stage: "selection" }>,
) {
  if (
    context.stage !== "review" ||
    context.approvalIntent ||
    state.phase !== "review" ||
    !state.receipt ||
    state.receipt.receipt_handle !== context.receiptHandle ||
    state.receipt.proposal_id !== context.proposalId ||
    state.receipt.operation_id !== context.operationId ||
    state.receipt.proposal_digest !== context.proposalDigest ||
    Date.parse(context.expiresAt) <= Date.now()
  )
    refuse();
  return button(state, "Approve");
}

/** Only the real mounted dialog and a native-admitted live hook request act. */
export async function executeSavedConnectorOwnerTest(
  request: OwnerTestRequest,
  markMutation: () => void,
  signal: AbortSignal,
): Promise<object> {
  const entry = registration;
  if (
    !entry ||
    signal.aborted ||
    request.expected.ownerPubkey !== OWNER ||
    request.expected.relayUrl !== RELAY
  )
    refuse();
  let raw: unknown;
  try {
    raw = await invokeTauri("rowvia_owner_test_saved_connector_context", {
      requestId: request.requestId,
    });
  } catch {
    return refuse();
  }
  const parsed = contextSchema.safeParse(raw);
  if (!parsed.success) refuse();
  const context = parsed.data;
  const read = () => {
    if (signal.aborted) refuse();
    return current(context, entry);
  };
  const state = read();
  if (request.operation === "inspect_saved_connector") {
    if (context.stage === "selection") {
      pending(state, context);
      return {
        stage: "selection",
        targetPubkey: SAVED_CONNECTOR_TARGET,
        gmailLabel: SAVED_CONNECTOR_ACCOUNT,
        freshChannelBound: true,
      };
    }
    review(state, context);
    return {
      stage: "review",
      targetPubkey: SAVED_CONNECTOR_TARGET,
      gmailLabel: SAVED_CONNECTOR_ACCOUNT,
      fingerprint: context.fingerprint,
      proposalId: context.proposalId,
      operationId: context.operationId,
      proposalDigest: context.proposalDigest,
      expiresAt: context.expiresAt,
      channelProvenance: "native_journal_no_historical_channel",
    };
  }
  if (request.operation === "prepare_saved_connector") {
    if (context.stage !== "selection") refuse();
    const eventId = pending(state, context);
    const itemId = state.itemId;
    const select = state.dialog?.querySelector("select");
    if (!select || select.disabled) refuse();
    if (prepared.has(eventId) || prepared.size >= 128) refuse();
    prepared.add(eventId);
    markMutation();
    // React must commit the selected handle before its current prepare closure
    // can be clicked. Never retain a pre-selection callback or button reference.
    select.value = context.selectionHandle;
    select.dispatchEvent(new Event("change", { bubbles: true }));
    for (;;) {
      const latest = read();
      if (latest.itemId !== itemId || pending(latest, context) !== eventId)
        refuse();
      if (latest.selectionHandle === context.selectionHandle) {
        button(latest, "Review proposal").click();
        return { prepareClickDispatched: true, runtimeReady: false };
      }
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
  }
  if (
    context.stage === "selection" ||
    !("fingerprint" in request.arguments) ||
    request.arguments.fingerprint !== context.fingerprint
  )
    refuse();
  if (request.operation === "approve_saved_connector") {
    const approve = review(state, context);
    if (consumed.has(context.fingerprint) || consumed.size >= 128) refuse();
    consumed.add(context.fingerprint);
    markMutation();
    // No await between the final committed-state check and the actual button.
    // Existing management.approve captures the full receipt before its await.
    approve.click();
    return {
      approvalClickDispatched: true,
      operationId: context.operationId,
      runtimeReady: false,
    };
  }
  if (
    request.operation !== "inspect_saved_connector_operation" ||
    context.stage !== "operation" ||
    !context.approvalIntent
  )
    refuse();
  markMutation();
  const operation = await getRowviaManagementOperation(context.operationId);
  read();
  if (
    operation.operation_id !== context.operationId ||
    operation.proposal_id !== context.proposalId ||
    operation.binding_id !== context.bindingId ||
    operation.action !== "connector.grant" ||
    operation.agent_name !== context.agentName
  )
    refuse();
  return {
    operationId: operation.operation_id,
    proposalId: operation.proposal_id,
    state: operation.state,
    policyApplied: operation.policy_applied,
    runtimeReady: operation.runtime_ready,
  };
}
