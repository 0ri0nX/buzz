import { z } from "zod";
import type { RelayEvent } from "@/shared/api/types";
import { invokeTauri } from "./tauri";

export type RowviaManagementAction = "connector.grant" | "connector.revoke";

const candidateSchema = z.strictObject({
  selection_handle: z.string().uuid(),
  agent_name: z.string().min(1),
  gmail_label: z.string().min(1),
  binding_status: z.string().min(1),
});
const candidatesSchema = z.strictObject({
  candidates: z.array(candidateSchema).max(200),
  truncated: z.boolean(),
});
const receiptSchema = z.strictObject({
  receipt_handle: z.string().uuid(),
  proposal_id: z.string().uuid(),
  operation_id: z.string().uuid(),
  proposal_digest: z.string().regex(/^[0-9a-f]{64}$/),
  proposal_bytes_b64: z.base64().min(1).max(524_288),
});
const operationSchema = z.object({
  operation_id: z.string().uuid(),
  proposal_id: z.string().uuid(),
  state: z.string().min(1),
  action: z.enum(["connector.grant", "connector.revoke"]),
  agent_name: z.string().min(1),
  binding_id: z.string().uuid(),
  policy_applied: z.boolean(),
  runtime_ready: z.boolean(),
  error_code: z.string().optional(),
});

export type RowviaCandidate = z.infer<typeof candidateSchema>;
export type RowviaCandidates = z.infer<typeof candidatesSchema>;
export type RowviaNativeReceipt = z.infer<typeof receiptSchema>;
export type RowviaOperation = z.infer<typeof operationSchema>;

const unresolvedSchema = z.strictObject({
  receipt_handle: z.string().uuid(),
  action: z.enum(["connector.grant", "connector.revoke"]),
});
export type RowviaUnresolvedProposal = z.infer<typeof unresolvedSchema>;

/** Native owns candidate selection, signed event admission, and proposal provenance. */
export async function getRowviaManagementCandidates(): Promise<RowviaCandidates> {
  return candidatesSchema.parse(
    await invokeTauri<unknown>("rowvia_get_management_candidates"),
  );
}

export async function createRowviaManagementProposal(input: {
  selectionHandle: string;
  action: RowviaManagementAction;
  relayEvent: RelayEvent;
}): Promise<RowviaNativeReceipt> {
  return receiptSchema.parse(
    await invokeTauri<unknown>("rowvia_create_management_proposal", {
      selection: {
        selection_handle: input.selectionHandle,
        action: input.action,
        relay_event_json: JSON.stringify(input.relayEvent),
      },
    }),
  );
}

/** The only approval input is the handle to a native-journaled receipt. */
export async function approveRowviaManagementProposal(
  receiptHandle: string,
): Promise<RowviaOperation> {
  return operationSchema.parse(
    await invokeTauri<unknown>("rowvia_approve_management_proposal", {
      receiptHandle,
    }),
  );
}

/** Explicit owner retry of the native journal's exact approval bytes. */
export async function retryRowviaManagementApproval(
  receiptHandle: string,
): Promise<RowviaOperation> {
  return operationSchema.parse(
    await invokeTauri<unknown>("rowvia_retry_management_approval", {
      receiptHandle,
    }),
  );
}

export async function getRowviaManagementOperation(
  operationId: string,
): Promise<RowviaOperation> {
  return operationSchema.parse(
    await invokeTauri<unknown>("rowvia_get_management_operation", {
      operationId,
    }),
  );
}

const pendingSchema = z.strictObject({
  receipts: z.array(receiptSchema).max(200),
  unresolved: z.array(unresolvedSchema).max(200),
});

export async function listRowviaManagementRecovery(): Promise<
  z.infer<typeof pendingSchema>
> {
  return pendingSchema.parse(
    await invokeTauri<unknown>("rowvia_list_management_pending"),
  );
}

export async function listRowviaManagementPending(): Promise<
  RowviaNativeReceipt[]
> {
  return (await listRowviaManagementRecovery()).receipts;
}

/** Retries only the exact native-journaled request; no request body crosses IPC. */
export async function retryRowviaManagementProposal(
  receiptHandle: string,
): Promise<RowviaNativeReceipt> {
  return receiptSchema.parse(
    await invokeTauri<unknown>("rowvia_retry_management_proposal", {
      receiptHandle,
    }),
  );
}

const recoverySchema = z.strictObject({
  receipt_handle: z.string().uuid(),
  operation_id: z.string().uuid(),
  approval_intent: z.boolean(),
  refused: z.boolean(),
  last_status: operationSchema.nullable(),
});

export type RowviaOperationRecovery = z.infer<typeof recoverySchema>;

export async function listRowviaManagementOperations(): Promise<
  RowviaOperationRecovery[]
> {
  return z
    .array(recoverySchema)
    .max(200)
    .parse(await invokeTauri<unknown>("rowvia_list_management_operations"));
}

/** Keep proposal recovery available when operation status cannot be read. */
export async function loadRowviaManagementRecoveryState(): Promise<{
  pending: z.infer<typeof pendingSchema>;
  operations: RowviaOperationRecovery[];
  errors: unknown[];
}> {
  const [pendingResult, operationsResult] = await Promise.allSettled([
    listRowviaManagementRecovery(),
    listRowviaManagementOperations(),
  ]);
  return {
    pending:
      pendingResult.status === "fulfilled"
        ? pendingResult.value
        : { receipts: [], unresolved: [] },
    operations:
      operationsResult.status === "fulfilled" ? operationsResult.value : [],
    errors: [
      pendingResult.status === "rejected" ? pendingResult.reason : null,
      operationsResult.status === "rejected" ? operationsResult.reason : null,
    ].filter((error) => error !== null),
  };
}

/** Refusal only changes the scoped native journal; no server rejection exists. */
export async function rejectRowviaManagementLocal(
  receiptHandle: string,
): Promise<void> {
  await invokeTauri<unknown>("rowvia_reject_management_local", {
    receiptHandle,
  });
}
