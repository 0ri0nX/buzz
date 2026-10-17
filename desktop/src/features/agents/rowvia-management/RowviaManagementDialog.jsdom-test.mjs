import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { after, afterEach, test } from "node:test";
import React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";

// Synthetic frontend integration only: no valid signatures, native commands,
// owner input, policy writes, or live relay connections are exercised here.
const calls = [];
// Media URL discovery also runs at import time; keep it synthetic throughout.
const mediaHandlers = [
  ["get_relay_http_url", () => "https://a.example.invalid"],
  ["get_media_proxy_port", () => 1],
];
const handlers = new Map(mediaHandlers);
const unexpectedCommands = [];
const previousInternals = globalThis.__TAURI_INTERNALS__;
const previousWindowInternals = window.__TAURI_INTERNALS__;
const previousMatchMedia = window.matchMedia;
window.matchMedia = (media) => ({
  media,
  matches: false,
  addEventListener() {},
  removeEventListener() {},
});
const internals = {
  invoke(command, args) {
    calls.push({ command, args });
    const handler = handlers.get(command);
    if (!handler) {
      unexpectedCommands.push(command);
      return Promise.reject(new Error(`Unmocked synthetic IPC: ${command}`));
    }
    return Promise.resolve(handler(args));
  },
};
globalThis.__TAURI_INTERNALS__ = internals;
window.__TAURI_INTERNALS__ = internals;

const { RowviaManagementDialog } = await import("./RowviaManagementDialog.tsx");
const { ThemeProvider } = await import(
  "../../../shared/theme/ThemeProvider.tsx"
);
const { CommunitiesProvider, useCommunities } = await import(
  "../../communities/useCommunities.tsx"
);
const { saveCommunities, saveActiveCommunityId } = await import(
  "../../communities/communityStorage.ts"
);
const {
  _testHandleRelayObserverEvent,
  _testRegisterKnownAgents,
  resetAgentObserverStore,
} = await import("../observerRelayStore.ts");

const ownerPubkey = "a".repeat(64);
const cerberusPubkey = "b".repeat(64);
const targetPubkey = "c".repeat(64);
const channelId = "550e8400-e29b-41d4-a716-446655440000";
const selectionHandle = "018fae01-1111-7111-8111-111111111111";
const otherSelectionHandle = "018fae01-2222-7222-8222-222222222222";
const receiptHandle = "018fae01-3333-7333-8333-333333333333";
const proposalId = "018fae01-4444-7444-8444-444444444444";
const operationId = "018fae01-5555-7555-8555-555555555555";
const bindingId = "018fae01-6666-7666-8666-666666666666";
const createCommand = "rowvia_create_management_proposal";
const approveCommand = "rowvia_approve_management_proposal";
const clients = [];

function proposalReceipt() {
  const proposal = {
    version: 1,
    tenant_id: "018fae01-7777-7777-8777-777777777777",
    proposal_id: proposalId,
    operation_id: operationId,
    owner_subject_id: "018fae01-8888-7888-8888-888888888888",
    owner_pubkey: ownerPubkey,
    action: "connector.grant",
    agent_name: "Synthetic-Mail-Helper",
    source_instance_id: "synthetic-instance",
    target_pubkey: targetPubkey,
    agent_identity_id: "018fae01-9999-7999-8999-999999999999",
    workload_principal_id: "018fae01-aaaa-7aaa-8aaa-aaaaaaaaaaaa",
    binding_id: bindingId,
    provider_subject: "synthetic-provider-subject",
    constrained_resource: "gmail-account:review-fixture@example.invalid",
    binding_generation: 1,
    credential_generation: 1,
    policy_generation: 1,
    access_policy_generation: 1,
    access_policy_state: "absent",
    manual_target_revision: 0,
    operations: ["gmail.message.read", "gmail.message.search"],
    expires_at: new Date(Date.now() + 300_000).toISOString(),
  };
  const bytes = Buffer.from(JSON.stringify(proposal));
  return {
    receipt_handle: receiptHandle,
    proposal_id: proposalId,
    operation_id: operationId,
    proposal_digest: createHash("sha256").update(bytes).digest("hex"),
    proposal_bytes_b64: bytes.toString("base64"),
  };
}

function commandCalls(command) {
  return calls.filter((call) => call.command === command);
}

function ScopeSwitch() {
  const { switchCommunity } = useCommunities();
  return React.createElement(
    "button",
    { onClick: () => switchCommunity("synthetic-b"), type: "button" },
    "Switch synthetic community",
  );
}

function mountDialog() {
  localStorage.clear();
  localStorage.setItem("buzz-follow-system", "false");
  saveCommunities([
    {
      id: "synthetic-a",
      name: "Synthetic A",
      relayUrl: "wss://a.example.invalid",
    },
    {
      id: "synthetic-b",
      name: "Synthetic B",
      relayUrl: "wss://b.example.invalid",
    },
  ]);
  saveActiveCommunityId("synthetic-a");
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  clients.push(client);
  client.setQueryData(["identity"], {
    pubkey: ownerPubkey,
    displayName: "Synthetic owner",
  });
  handlers.set("rowvia_list_management_pending", () => ({
    receipts: [],
    unresolved: [],
  }));
  handlers.set("rowvia_list_management_operations", () => []);
  handlers.set("rowvia_get_management_candidates", () => ({
    candidates: [
      {
        selection_handle: otherSelectionHandle,
        agent_name: "Synthetic-Mail-Helper",
        gmail_label: "Other fixture",
        binding_status: "active",
      },
      {
        selection_handle: selectionHandle,
        agent_name: "Synthetic-Mail-Helper",
        gmail_label: "Selected fixture",
        binding_status: "active",
      },
    ],
    truncated: false,
  }));
  _testRegisterKnownAgents("synthetic-dialog", [cerberusPubkey]);
  return render(
    React.createElement(
      QueryClientProvider,
      { client },
      React.createElement(
        ThemeProvider,
        null,
        React.createElement(
          CommunitiesProvider,
          null,
          React.createElement(ScopeSwitch),
          React.createElement(RowviaManagementDialog),
        ),
      ),
    ),
  );
}

async function deliverDraft(prepare = true) {
  const event = {
    id: "d".repeat(64),
    pubkey: cerberusPubkey,
    created_at: 1,
    kind: 1,
    tags: [
      ["agent", cerberusPubkey],
      ["frame", "telemetry"],
    ],
    content: "synthetic-ciphertext-not-encrypted",
    sig: "e".repeat(128),
  };
  await act(async () => {
    await _testHandleRelayObserverEvent(event, async (received) => {
      assert.equal(received, event);
      return {
        seq: 1,
        timestamp: "2026-10-17T12:00:00Z",
        kind: "agent_management_request",
        agentIndex: null,
        channelId,
        sessionId: null,
        turnId: null,
        payload: {
          type: "agent_management_request",
          version: 1,
          action: "connector.grant",
          requestId: "550e8400-e29b-41d4-a716-446655440001",
          request: {
            channelId,
            targetName: "Untrusted name hint",
            gmailLabels: ["Hint only"],
          },
        },
      };
    });
  });
  const select = await screen.findByRole("combobox", {
    name: "Agent and Gmail connection",
  });
  await screen.findByRole("option", { name: /Selected fixture/ });
  if (!prepare) return event;
  assert.equal(commandCalls(createCommand).length, 0);
  assert.equal(commandCalls(approveCommand).length, 0);
  fireEvent.change(select, { target: { value: selectionHandle } });
  fireEvent.click(screen.getByRole("button", { name: "Review proposal" }));
  await waitFor(() => assert.equal(commandCalls(createCommand).length, 1));
  assert.deepEqual(commandCalls(createCommand), [
    {
      command: createCommand,
      args: {
        selection: {
          selection_handle: selectionHandle,
          action: "connector.grant",
          relay_event_json: JSON.stringify(event),
        },
      },
    },
  ]);
  return event;
}

afterEach(() => {
  cleanup();
  for (const client of clients.splice(0)) client.clear();
  resetAgentObserverStore();
  localStorage.clear();
  calls.length = 0;
  handlers.clear();
  for (const [command, handler] of mediaHandlers)
    handlers.set(command, handler);
  const unexpected = unexpectedCommands.splice(0);
  assert.deepEqual(
    unexpected,
    [],
    "all IPC must stay inside the synthetic allowlist",
  );
});
after(() => {
  globalThis.__TAURI_INTERNALS__ = previousInternals;
  window.__TAURI_INTERNALS__ = previousWindowInternals;
  window.matchMedia = previousMatchMedia;
});

test("synthetic observer → real dialog/hook → explicit approval preserves IPC authority boundaries", {
  timeout: 15_000,
}, async () => {
  const receipt = proposalReceipt();
  handlers.set(createCommand, () => receipt);
  handlers.set(approveCommand, () => ({
    operation_id: operationId,
    proposal_id: proposalId,
    state: "succeeded",
    action: "connector.grant",
    agent_name: "Synthetic-Mail-Helper",
    binding_id: bindingId,
    policy_applied: true,
    runtime_ready: false,
  }));
  mountDialog();
  await deliverDraft();
  await waitFor(() =>
    assert.equal(
      screen.getByRole("button", { name: "Approve" }).disabled,
      false,
    ),
  );
  assert.ok(screen.getByText("Grant access"));
  assert.ok(screen.getByText(`Identity: ${targetPubkey}`));
  assert.ok(screen.getByText("review-fixture@example.invalid"));
  assert.ok(screen.getByText(bindingId));
  assert.ok(screen.getByText("Read and search Gmail messages"));
  assert.equal(
    commandCalls(approveCommand).length,
    0,
    "review must never auto-approve",
  );
  fireEvent.click(screen.getByRole("button", { name: "Approve" }));
  await screen.findByText("Access granted, runtime updating.");
  assert.equal(screen.queryByText("Access granted and runtime ready."), null);
  assert.deepEqual(commandCalls(approveCommand), [
    {
      command: approveCommand,
      args: { receiptHandle: receipt.receipt_handle },
    },
  ]);
});

test("scope change during synthetic deferred creation cannot expose or approve the stale receipt", {
  timeout: 15_000,
}, async () => {
  let resolveCreate;
  handlers.set(
    createCommand,
    () =>
      new Promise((resolve) => {
        resolveCreate = resolve;
      }),
  );
  const view = mountDialog();
  await deliverDraft();
  assert.ok(screen.getByRole("button", { name: "Preparing…" }).disabled);
  // The switch is outside the modal, as a community change originates elsewhere
  // in the app. Use the rendered element despite Radix hiding that background.
  fireEvent.click(view.getByText("Switch synthetic community"));
  await waitFor(() => assert.equal(screen.queryByRole("dialog"), null));
  assert.equal(localStorage.getItem("buzz-active-community-id"), "synthetic-b");
  // Reuse the same fixture event/request ID in the new scope. A late update
  // cannot accidentally attach the old receipt to a new item with the same ID.
  await deliverDraft(false);
  await act(async () => {
    resolveCreate(proposalReceipt());
  });
  assert.ok(
    screen.getByRole("combobox", { name: "Agent and Gmail connection" }),
  );
  assert.ok(screen.getByRole("button", { name: "Review proposal" }).disabled);
  assert.equal(
    screen.queryByRole("button", { name: "Approve", hidden: true }),
    null,
  );
  assert.equal(screen.queryByText(`Identity: ${targetPubkey}`), null);
  assert.equal(screen.queryByText("review-fixture@example.invalid"), null);
  assert.equal(commandCalls(createCommand).length, 1);
  assert.equal(commandCalls(approveCommand).length, 0);
});
