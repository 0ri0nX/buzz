import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

const calls = [];
const handlers = new Map();
const internals = {
  invoke: (command, args) => {
    calls.push({ command, args });
    const handler = handlers.get(command);
    return handler
      ? Promise.resolve(handler(args))
      : Promise.reject(new Error(`unmocked command: ${command}`));
  },
};
globalThis.window = { __TAURI_INTERNALS__: internals };
globalThis.__TAURI_INTERNALS__ = internals;

const {
  approveRowviaManagementProposal,
  createRowviaManagementProposal,
  getRowviaManagementCandidates,
  loadRowviaManagementRecoveryState,
  listRowviaManagementRecovery,
  listRowviaManagementPending,
  listRowviaManagementOperations,
  rejectRowviaManagementLocal,
  retryRowviaManagementApproval,
  retryRowviaManagementProposal,
} = await import("./rowviaManagement.ts");

const receipt = {
  receipt_handle: "018fae01-1111-7111-8111-111111111111",
  proposal_id: "018fae01-2222-7222-8222-222222222222",
  operation_id: "018fae01-3333-7333-8333-333333333333",
  proposal_digest: "a".repeat(64),
  proposal_bytes_b64: "e30=",
};

afterEach(() => {
  calls.length = 0;
  handlers.clear();
});

test("native creation receives only a selection and signed relay evidence", async () => {
  handlers.set("rowvia_create_management_proposal", () => receipt);
  const relayEvent = {
    id: "b".repeat(64),
    pubkey: "c".repeat(64),
    created_at: 1,
    kind: 1,
    tags: [],
    content: "encrypted",
    sig: "d".repeat(128),
  };
  assert.deepEqual(
    await createRowviaManagementProposal({
      selectionHandle: receipt.receipt_handle,
      action: "connector.grant",
      relayEvent,
    }),
    receipt,
  );
  assert.deepEqual(calls, [
    {
      command: "rowvia_create_management_proposal",
      args: {
        selection: {
          selection_handle: receipt.receipt_handle,
          action: "connector.grant",
          relay_event_json: JSON.stringify(relayEvent),
        },
      },
    },
  ]);
});

test("approval sends only the journal receipt handle", async () => {
  handlers.set("rowvia_approve_management_proposal", () => ({
    operation_id: receipt.operation_id,
    proposal_id: receipt.proposal_id,
    state: "succeeded",
    action: "connector.grant",
    agent_name: "Mail-helper",
    binding_id: "018fae01-4444-7444-8444-444444444444",
    policy_applied: true,
    runtime_ready: false,
  }));
  await approveRowviaManagementProposal(receipt.receipt_handle);
  assert.deepEqual(calls, [
    {
      command: "rowvia_approve_management_proposal",
      args: { receiptHandle: receipt.receipt_handle },
    },
  ]);
});

test("approval retry sends only the native journal handle", async () => {
  handlers.set("rowvia_retry_management_approval", () => ({
    operation_id: receipt.operation_id,
    proposal_id: receipt.proposal_id,
    state: "accepted",
    action: "connector.grant",
    agent_name: "Mail-helper",
    binding_id: "018fae01-4444-7444-8444-444444444444",
    policy_applied: false,
    runtime_ready: false,
  }));
  await retryRowviaManagementApproval(receipt.receipt_handle);
  assert.deepEqual(calls, [
    {
      command: "rowvia_retry_management_approval",
      args: { receiptHandle: receipt.receipt_handle },
    },
  ]);
});

test("candidate and recovery responses reject unexpected authority fields", async () => {
  handlers.set("rowvia_get_management_candidates", () => ({
    candidates: [
      {
        selection_handle: receipt.receipt_handle,
        agent_name: "Mail-helper",
        gmail_label: "Primary",
        binding_status: "active",
        binding_id: "copied-secret-coordinate",
      },
    ],
    truncated: false,
  }));
  await assert.rejects(getRowviaManagementCandidates());
  handlers.set("rowvia_list_management_pending", () => ({
    receipts: [{ ...receipt, approval_intent: true }],
    unresolved: [],
  }));
  await assert.rejects(listRowviaManagementPending());
});

test("unresolved recovery exposes metadata and retries by handle only", async () => {
  handlers.set("rowvia_list_management_pending", () => ({
    receipts: [],
    unresolved: [
      { receipt_handle: receipt.receipt_handle, action: "connector.grant" },
    ],
  }));
  handlers.set("rowvia_retry_management_proposal", () => receipt);
  const pending = await listRowviaManagementRecovery();
  assert.deepEqual(pending.unresolved, [
    { receipt_handle: receipt.receipt_handle, action: "connector.grant" },
  ]);
  assert.deepEqual(
    await retryRowviaManagementProposal(receipt.receipt_handle),
    receipt,
  );
  assert.deepEqual(calls[1], {
    command: "rowvia_retry_management_proposal",
    args: { receiptHandle: receipt.receipt_handle },
  });
});

test("operation recovery failure does not hide an unresolved proposal", async () => {
  handlers.set("rowvia_list_management_pending", () => ({
    receipts: [],
    unresolved: [
      { receipt_handle: receipt.receipt_handle, action: "connector.grant" },
    ],
  }));
  handlers.set("rowvia_list_management_operations", () => {
    throw new Error("operation status unavailable");
  });
  const recovered = await loadRowviaManagementRecoveryState();
  assert.equal(
    recovered.pending.unresolved[0].receipt_handle,
    receipt.receipt_handle,
  );
  assert.deepEqual(recovered.operations, []);
  assert.match(String(recovered.errors[0]), /operation status unavailable/);
});

test("local rejection names only the native receipt", async () => {
  handlers.set("rowvia_reject_management_local", () => null);
  await rejectRowviaManagementLocal(receipt.receipt_handle);
  assert.deepEqual(calls, [
    {
      command: "rowvia_reject_management_local",
      args: { receiptHandle: receipt.receipt_handle },
    },
  ]);
});

test("operation recovery carries status handles without approval inputs", async () => {
  handlers.set("rowvia_list_management_operations", () => [
    {
      receipt_handle: receipt.receipt_handle,
      operation_id: receipt.operation_id,
      approval_intent: true,
      refused: false,
      last_status: null,
    },
  ]);
  const recovered = await listRowviaManagementOperations();
  assert.equal(recovered[0].approval_intent, true);
  assert.deepEqual(calls, [
    { command: "rowvia_list_management_operations", args: {} },
  ]);
});
