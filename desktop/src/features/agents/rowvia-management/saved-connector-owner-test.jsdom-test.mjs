import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { afterEach, test } from "node:test";
import React, { act } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { CommunitiesProvider } from "@/features/communities/useCommunities";
import {
  saveActiveCommunityId,
  saveCommunities,
} from "@/features/communities/communityStorage";
import { ThemeProvider } from "@/shared/theme/ThemeProvider";
import { RowviaManagementDialog } from "./RowviaManagementDialog.tsx";
import { createOwnerTestHandler } from "@/testing/ownerTestHook";
import { relayClient } from "@/shared/api/relayClient";
import {
  _testRegisterKnownAgents,
  _testHandleRelayObserverEvent,
} from "@/features/agents/observerRelayStore";
import { KIND_AGENT_OBSERVER_FRAME } from "@/shared/constants/kinds";

// Synthetic IPC responses only: exercise the real dialog, verifier, hook and
// typed native API without signing a proposal or changing any live grant.
const ownerPubkey = "c".repeat(64);
const targetPubkey = "a".repeat(64);
const proposalId = "018fae01-1111-7111-8111-111111111111";
const operationId = "018fae01-2222-7222-8222-222222222222";
const bindingId = "018fae01-3333-7333-8333-333333333333";
const receiptHandle = "018fae01-8888-7888-8888-888888888888";
const calls = [];
let nativeReceipt;
let queryClient;
const fixedOwner =
  "89af092923ad49e3b9916b0c9f28c6b9237d8fcab4461cb7ddd12afe354f0d68";
const fixedTarget =
  "29cc1bb0ac338520c263dcfe654764224018a1ff5e5d3fc633be73f71f0c8c78";
const fixedRelay = "wss://buzz.rowvia.ai:8443";
const fixedChannel = "31986c38-6676-4d6f-a4ba-ed833926d0d5";
const selectionHandle = "018fae01-9999-7999-8999-999999999999";
let activeOwner = ownerPubkey;
let recoverReceipt = true;
let nativeContext = null;
let onContext = () => {};
let failApproval = false;
let handler;
const internals = {
  invoke(command, args) {
    calls.push({ command, args });
    switch (command) {
      case "get_identity":
        return Promise.resolve({ pubkey: activeOwner });
      case "get_relay_ws_url":
        return Promise.resolve(fixedRelay);
      case "rowvia_owner_test_saved_connector_context":
        onContext();
        return Promise.resolve(nativeContext);
      case "rowvia_get_management_candidates":
        return Promise.resolve({
          candidates: [
            {
              selection_handle: selectionHandle,
              agent_name: "saved-developer-test",
              gmail_label: "jiri.hroza@gmail.com",
              binding_status: "active",
            },
          ],
          truncated: false,
        });
      case "rowvia_create_management_proposal":
        return Promise.resolve(nativeReceipt);
      case "rowvia_get_management_operation":
        return Promise.resolve({
          operation_id: operationId,
          proposal_id: proposalId,
          state: "succeeded",
          action: "connector.grant",
          agent_name: "saved-developer-test",
          binding_id: bindingId,
          policy_applied: true,
          runtime_ready: false,
        });
      case "get_media_proxy_port":
        return Promise.resolve(54321);
      case "get_relay_http_url":
        return Promise.resolve("https://test.example");
      case "rowvia_list_management_pending":
        return Promise.resolve({
          receipts: recoverReceipt ? [nativeReceipt] : [],
          unresolved: [],
        });
      case "rowvia_list_management_operations":
        return Promise.resolve([]);
      case "rowvia_approve_management_proposal":
        if (failApproval)
          return Promise.reject(new Error("synthetic transport lost"));
        return Promise.resolve({
          operation_id: operationId,
          proposal_id: proposalId,
          state: "policy_applied",
          action: "connector.grant",
          agent_name: "saved-developer-test",
          binding_id: bindingId,
          policy_applied: true,
          runtime_ready: false,
        });
      default:
        return Promise.reject(new Error(`Unmocked Tauri command: ${command}`));
    }
  },
};
globalThis.__TAURI_INTERNALS__ = internals;
window.__TAURI_INTERNALS__ = internals;
window.matchMedia = () => ({
  matches: false,
  addEventListener() {},
  removeEventListener() {},
});

afterEach(() => {
  handler?.dispose();
  handler = undefined;
  cleanup();
  queryClient?.clear();
  localStorage.clear();
  calls.length = 0;
  nativeContext = null;
  onContext = () => {};
  failApproval = false;
});

function receipt(overrides = {}, envelopeOverrides = {}) {
  const proposal = {
    version: 1,
    tenant_id: "018fae01-4444-7444-8444-444444444444",
    proposal_id: proposalId,
    operation_id: operationId,
    owner_subject_id: "018fae01-5555-7555-8555-555555555555",
    owner_pubkey: ownerPubkey,
    action: "connector.grant",
    agent_name: "saved-developer-test",
    source_instance_id: "cerberus-test",
    target_pubkey: targetPubkey,
    agent_identity_id: "018fae01-6666-7666-8666-666666666666",
    workload_principal_id: "018fae01-7777-7777-8777-777777777777",
    binding_id: bindingId,
    provider_subject: "synthetic-google-subject",
    constrained_resource: "gmail-account:synthetic-owner@example.com",
    binding_generation: 1,
    credential_generation: 1,
    policy_generation: 1,
    access_policy_generation: 1,
    access_policy_state: "absent",
    manual_target_revision: 0,
    operations: ["gmail.message.read", "gmail.message.search"],
    expires_at: new Date(Date.now() + 60_000).toISOString(),
    ...overrides,
  };
  const bytes = Buffer.from(JSON.stringify(proposal), "utf8");
  return {
    receipt_handle: receiptHandle,
    proposal_id: proposalId,
    operation_id: operationId,
    proposal_digest: createHash("sha256").update(bytes).digest("hex"),
    proposal_bytes_b64: bytes.toString("base64"),
    ...envelopeOverrides,
  };
}

function show(input, { owner = ownerPubkey, pending = true } = {}) {
  nativeReceipt = input;
  activeOwner = owner;
  recoverReceipt = pending;
  saveCommunities([
    { id: "synthetic-community", name: "Test", relayUrl: fixedRelay },
  ]);
  saveActiveCommunityId("synthetic-community");
  localStorage.setItem("buzz-follow-system", "false");
  queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: Infinity } },
  });
  queryClient.setQueryData(["identity"], { pubkey: owner });
  return render(
    React.createElement(
      QueryClientProvider,
      { client: queryClient },
      React.createElement(
        CommunitiesProvider,
        null,
        React.createElement(
          ThemeProvider,
          null,
          React.createElement(RowviaManagementDialog),
        ),
      ),
    ),
  );
}

function hookRequest(operation, args = {}) {
  return {
    schema: "rowvia.buzz.owner-test/v1",
    requestId: crypto.randomUUID(),
    operation,
    arguments: args,
    expected: {
      ownerPubkey: fixedOwner,
      architectPubkey:
        "05d1901b020205f04db1ee6c434d4cd69211af3dbc1d2a46e0e1e93f81f6dd20",
      relayUrl: fixedRelay,
    },
  };
}
function fixedReceipt(overrides = {}) {
  return receipt({
    owner_pubkey: fixedOwner,
    target_pubkey: fixedTarget,
    constrained_resource: "gmail-account:jiri.hroza@gmail.com",
    ...overrides,
  });
}
function context(input, fingerprint = "1".repeat(64)) {
  const body = JSON.parse(
    Buffer.from(input.proposal_bytes_b64, "base64").toString(),
  );
  return {
    ownerPubkey: fixedOwner,
    relayUrl: fixedRelay,
    targetPubkey: fixedTarget,
    gmailLabel: "jiri.hroza@gmail.com",
    cerberusPubkey: "d".repeat(64),
    deadlineUnixMs: Date.now() + 10_000,
    stage: "review",
    fingerprint,
    receiptHandle: input.receipt_handle,
    proposalId: input.proposal_id,
    operationId: input.operation_id,
    proposalDigest: input.proposal_digest,
    expiresAt: body.expires_at,
    agentName: body.agent_name,
    bindingId: body.binding_id,
    approvalIntent: false,
    sourceEventId: "e".repeat(64),
  };
}
function ownerHook() {
  relayClient.connectionStateEmitter.set("connected");
  handler = createOwnerTestHandler(() => {});
  return handler;
}
async function actRequest(run, request) {
  let response;
  await act(async () => {
    response = await run(request);
  });
  return response;
}

test("closed owner hook inspects real verified dialog, clicks Approve once and GETs only its native operation", async () => {
  const input = fixedReceipt();
  nativeContext = context(input);
  show(input, { owner: fixedOwner });
  await waitFor(() =>
    assert.equal(
      screen.getByRole("button", { name: "Approve" }).disabled,
      false,
    ),
  );
  const run = ownerHook();
  const inspected = await run(hookRequest("inspect_saved_connector"));
  assert.equal(inspected.status, "ok");
  assert.equal(inspected.result.fingerprint, nativeContext.fingerprint);
  assert.equal(
    inspected.result.channelProvenance,
    "native_journal_no_historical_channel",
  );
  assert.equal("receiptHandle" in inspected.result, false);
  const approval = hookRequest("approve_saved_connector", {
    fingerprint: nativeContext.fingerprint,
  });
  const result = await actRequest(run, approval);
  assert.equal(result.status, "ok");
  assert.equal(result.result.approvalClickDispatched, true);
  await waitFor(() =>
    assert.ok(screen.getByText("Access granted, runtime updating.")),
  );
  assert.equal((await run(approval)).code, "duplicate_request");
  assert.equal(
    (await run(hookRequest("approve_saved_connector", approval.arguments)))
      .code,
    "not_ready",
  );
  assert.deepEqual(
    calls.filter(
      ({ command }) => command === "rowvia_approve_management_proposal",
    ),
    [
      {
        command: "rowvia_approve_management_proposal",
        args: { receiptHandle: input.receipt_handle },
      },
    ],
  );
  nativeContext = {
    ...nativeContext,
    stage: "operation",
    approvalIntent: true,
  };
  const status = await run(
    hookRequest("inspect_saved_connector_operation", approval.arguments),
  );
  assert.equal(status.status, "ok");
  assert.equal(status.result.policyApplied, true);
  assert.equal(status.result.runtimeReady, false);
  assert.deepEqual(
    calls.filter(
      ({ command }) => command === "rowvia_get_management_operation",
    ),
    [{ command: "rowvia_get_management_operation", args: { operationId } }],
  );
  assert.equal(
    calls.some(({ command }) => command === "rowvia_retry_management_approval"),
    false,
  );
  assert.ok(
    calls
      .filter(
        ({ command }) =>
          command === "rowvia_owner_test_saved_connector_context",
      )
      .every(({ args }) => Object.keys(args).join() === "requestId"),
  );
});

async function showPending(eventDigit = "f") {
  const input = fixedReceipt();
  nativeContext = {
    ownerPubkey: fixedOwner,
    relayUrl: fixedRelay,
    targetPubkey: fixedTarget,
    gmailLabel: "jiri.hroza@gmail.com",
    cerberusPubkey: "d".repeat(64),
    deadlineUnixMs: Date.now() + 10_000,
    stage: "selection",
    selectionHandle,
    agentName: "saved-developer-test",
  };
  const view = show(input, { owner: fixedOwner, pending: false });
  await waitFor(() =>
    assert.ok(
      calls.some(({ command }) => command === "rowvia_list_management_pending"),
    ),
  );
  _testRegisterKnownAgents("saved-connector-ui", [
    nativeContext.cerberusPubkey,
  ]);
  const event = {
    id: eventDigit.repeat(64),
    pubkey: nativeContext.cerberusPubkey,
    created_at: Math.floor(Date.now() / 1000),
    kind: KIND_AGENT_OBSERVER_FRAME,
    tags: [
      ["agent", nativeContext.cerberusPubkey],
      ["frame", "telemetry"],
    ],
    content: "synthetic encrypted frame",
    sig: "0".repeat(128),
  };
  const payload = {
    type: "agent_management_request",
    version: 1,
    action: "connector.grant",
    requestId: crypto.randomUUID(),
    request: {
      channelId: fixedChannel,
      targetName: "saved-developer-test",
    },
  };
  await act(async () =>
    _testHandleRelayObserverEvent(event, async () => ({
      seq: 0,
      timestamp: new Date().toISOString(),
      kind: "agent_management_request",
      channelId: fixedChannel,
      sessionId: null,
      turnId: null,
      payload,
    })),
  );
  await waitFor(() => assert.ok(screen.getByRole("combobox")));
  return { view, event };
}

test("closed hook prepares via real committed selection and Review proposal without approving", async () => {
  const { event } = await showPending();
  const run = ownerHook();
  // Allow the real hook to observe the committed selection between its awaits.
  const signer = nativeContext.cerberusPubkey;
  nativeContext = { ...nativeContext, cerberusPubkey: "a".repeat(64) };
  assert.equal(
    (await run(hookRequest("prepare_saved_connector"))).code,
    "not_ready",
  );
  assert.equal(
    calls.some(
      ({ command }) => command === "rowvia_create_management_proposal",
    ),
    false,
  );
  nativeContext = { ...nativeContext, cerberusPubkey: signer };
  let preparation;
  await act(async () => {
    preparation = run(hookRequest("prepare_saved_connector"));
  });
  const result = await preparation;
  assert.equal(result.status, "ok");
  assert.equal(result.result.prepareClickDispatched, true);
  await waitFor(() =>
    assert.equal(
      screen.getByRole("button", { name: "Approve" }).disabled,
      false,
    ),
  );
  const creates = calls.filter(
    ({ command }) => command === "rowvia_create_management_proposal",
  );
  assert.equal(creates.length, 1);
  assert.equal(creates[0].args.selection.selection_handle, selectionHandle);
  assert.equal(creates[0].args.selection.action, "connector.grant");
  assert.deepEqual(
    JSON.parse(creates[0].args.selection.relay_event_json),
    event,
  );
  assert.equal(
    calls.some(
      ({ command }) => command === "rowvia_approve_management_proposal",
    ),
    false,
  );
});

test("unmount during real selection returns Unknown and cannot later prepare or approve", async () => {
  const { view } = await showPending("8");
  const run = ownerHook();
  const request = hookRequest("prepare_saved_connector");
  screen
    .getByRole("combobox")
    .addEventListener("change", () => view.unmount(), { once: true });
  const result = await actRequest(run, request);
  assert.equal(result.status, "unknown");
  assert.equal((await run(request)).code, "duplicate_request");
  assert.equal(
    calls.some(
      ({ command }) =>
        command === "rowvia_create_management_proposal" ||
        command === "rowvia_approve_management_proposal",
    ),
    false,
  );
});

test("remount and a replacement hook lifetime cannot click a consumed canonical fingerprint", async () => {
  const input = fixedReceipt();
  nativeContext = context(input, "6".repeat(64));
  const first = show(input, { owner: fixedOwner });
  await waitFor(() =>
    assert.equal(
      screen.getByRole("button", { name: "Approve" }).disabled,
      false,
    ),
  );
  const args = { fingerprint: nativeContext.fingerprint };
  await actRequest(ownerHook(), hookRequest("approve_saved_connector", args));
  first.unmount();
  handler.dispose();
  show(input, { owner: fixedOwner });
  await waitFor(() =>
    assert.equal(
      screen.getByRole("button", { name: "Approve" }).disabled,
      false,
    ),
  );
  const result = await ownerHook()(
    hookRequest("approve_saved_connector", args),
  );
  assert.equal(result.code, "not_ready");
  assert.equal(
    calls.filter(
      ({ command }) => command === "rowvia_approve_management_proposal",
    ).length,
    1,
  );
});

test("closed hook cannot turn an invalid canonical producer receipt into an approval", async () => {
  const input = fixedReceipt({ account_email: "untrusted@example.com" });
  nativeContext = context(input, "7".repeat(64));
  show(input, { owner: fixedOwner });
  await waitFor(() => assert.ok(screen.getByText(/unavailable or invalid/)));
  const result = await ownerHook()(
    hookRequest("approve_saved_connector", {
      fingerprint: nativeContext.fingerprint,
    }),
  );
  assert.equal(result.code, "not_ready");
  assert.equal(
    calls.some(
      ({ command }) => command === "rowvia_approve_management_proposal",
    ),
    false,
  );
});

for (const [name, change] of [
  ["wrong fingerprint", (value) => ({ ...value, fingerprint: "9".repeat(64) })],
  ["expired native deadline", (value) => ({ ...value, deadlineUnixMs: 1 })],
  [
    "foreign native owner",
    (value) => ({ ...value, ownerPubkey: "a".repeat(64) }),
  ],
  [
    "foreign native target",
    (value) => ({ ...value, targetPubkey: "a".repeat(64) }),
  ],
]) {
  test(`closed hook refuses ${name} before any approval`, async () => {
    const input = fixedReceipt();
    nativeContext = change(context(input, "2".repeat(64)));
    show(input, { owner: fixedOwner });
    await waitFor(() =>
      assert.equal(
        screen.getByRole("button", { name: "Approve" }).disabled,
        false,
      ),
    );
    const result = await ownerHook()(
      hookRequest("approve_saved_connector", { fingerprint: "2".repeat(64) }),
    );
    assert.equal(result.status, "error");
    assert.equal(
      calls.some(
        ({ command }) => command === "rowvia_approve_management_proposal",
      ),
      false,
    );
  });
}

test("lost native approval remains unconfirmed and new request IDs cannot click it again", async () => {
  const input = fixedReceipt();
  nativeContext = context(input, "3".repeat(64));
  failApproval = true;
  show(input, { owner: fixedOwner });
  await waitFor(() =>
    assert.equal(
      screen.getByRole("button", { name: "Approve" }).disabled,
      false,
    ),
  );
  const run = ownerHook();
  const args = { fingerprint: nativeContext.fingerprint };
  assert.equal(
    (await actRequest(run, hookRequest("approve_saved_connector", args))).result
      .approvalClickDispatched,
    true,
  );
  await waitFor(() => assert.ok(screen.getByText(/Decision unconfirmed/)));
  assert.equal(
    (await run(hookRequest("approve_saved_connector", args))).status,
    "error",
  );
  assert.equal(
    calls.filter(
      ({ command }) => command === "rowvia_approve_management_proposal",
    ).length,
    1,
  );
  assert.equal(
    calls.some(({ command }) => command === "rowvia_retry_management_approval"),
    false,
  );
});

test("full native receipt renders verified review and approves its original handle", async () => {
  const input = receipt();
  show(input);
  await waitFor(() => {
    assert.equal(
      screen.getByRole("button", { name: "Approve" }).disabled,
      false,
    );
  });
  assert.ok(
    screen.getByRole("dialog", { name: "Cerberus Gmail access request" }),
  );
  assert.ok(screen.getByText(`Identity: ${targetPubkey}`));
  assert.ok(screen.getByText("synthetic-owner@example.com"));
  assert.ok(screen.getByText("Read and search Gmail messages"));
  assert.deepEqual(
    calls
      .filter(({ command }) => command.startsWith("rowvia_"))
      .map(({ command }) => command),
    ["rowvia_list_management_pending", "rowvia_list_management_operations"],
    "rendering a verified review must not approve anything",
  );
  fireEvent.click(screen.getByRole("button", { name: "Approve" }));
  await waitFor(() => {
    assert.ok(screen.getByText("Access granted, runtime updating."));
  });
  assert.deepEqual(
    calls.filter(
      ({ command }) => command === "rowvia_approve_management_proposal",
    ),
    [
      {
        command: "rowvia_approve_management_proposal",
        args: { receiptHandle: input.receipt_handle },
      },
    ],
  );
  assert.equal(screen.queryByText("Access granted and runtime ready."), null);
});

for (const [name, input] of [
  [
    "unknown canonical authority field",
    receipt({ account_email: "spoof@example.com" }),
  ],
  [
    "unsupported canonical operation",
    receipt({ operations: ["gmail.message.send"] }),
  ],
  ["digest mismatch", receipt({}, { proposal_digest: "f".repeat(64) })],
  [
    "expired canonical proposal",
    receipt({ expires_at: "2000-01-01T00:00:00Z" }),
  ],
]) {
  test(`full native receipt with ${name} still refuses approval`, async () => {
    show(input);
    await waitFor(() => {
      const approve = screen.getByRole("button", { name: "Approve" });
      assert.equal(approve.disabled, true);
      assert.ok(screen.getByText(/unavailable or invalid|expired/i));
    });
    fireEvent.click(screen.getByRole("button", { name: "Approve" }));
    assert.equal(
      calls.some(
        ({ command }) => command === "rowvia_approve_management_proposal",
      ),
      false,
    );
  });
}
