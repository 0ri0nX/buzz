import assert from "node:assert/strict";
import { after, afterEach, test } from "node:test";
import {
  id as savedOperation,
  review as savedReview,
  canonical,
  digest,
} from "@/shared/api/rowviaSavedAgent.test-fixture.mjs";
import React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";

// Offline integration at the real observer/dialog/IPC seams. Synthetic decrypt
// output stands in for authenticated owner telemetry; no signature, native
// authorization, TLS, deployed UI, or actual agent startup is exercised.
const ownerPubkey = "a".repeat(64);
const sourcePubkey = "b".repeat(64);
const createdPubkey = "c".repeat(64);
const channelId = "550e8400-e29b-41d4-a716-446655440000";
const displayName = "Synthetic review helper";
const systemPrompt = "Review only the explicitly selected synthetic task.";
const calls = [];
const unexpected = [];
let networkCalls = 0;
let client;
let rawPersonas = [];
let createdAgent = null;
let spawnError = null;
let savedPending = [];
let savedPendingResults = [];
let savedOperationId = savedOperation;
let savedReviewDelta = {};
let savedReviewFailure = false;
let savedApproval = "authorization";
let holdApproval = null;
let savedSaveFailure = null;
let savedSaveResults = [];
let membershipResult = { added: [createdPubkey], errors: [] };
let membershipFailure = false;
let holdMembership = null;
let startFailure = false;
function ownerReview() {
  const canonical21 = canonical({
    operation_id: savedOperationId,
    agent_name: `saved-${savedOperationId.replaceAll("-", "")}`,
    agent_pubkey: createdPubkey,
    owner_pubkey: ownerPubkey,
  });
  return {
    ...savedReview(),
    operationId: savedOperationId,
    agentPubkey: createdPubkey,
    ownerPubkey,
    name: displayName,
    systemPrompt,
    channelId,
    canonical21,
    canonical21Sha256: digest(canonical21),
    ...savedReviewDelta,
  };
}
const runtime = {
  id: "codex",
  label: "Codex",
  availability: "available",
  command: "synthetic-codex-acp",
  defaultArgs: [],
  mcpCommand: "",
  avatarUrl: null,
};
const rawAgent = (pubkey = createdPubkey, status = "stopped") => ({
  pubkey,
  name: displayName,
  persona_id: "synthetic-persona",
  runtime: "codex",
  relay_url: "wss://a.example.invalid",
  acp_command: "buzz-acp",
  agent_command: runtime.command,
  agent_args: [],
  mcp_command: "",
  status,
  backend: { type: "local" },
  respond_to: "owner-only",
  respond_to_allowlist: [],
  parallelism: 1,
  env_vars: {},
});
const mutationCommands = new Set([
  "create_persona",
  "create_managed_agent",
  "add_channel_members",
  "start_managed_agent",
]);
const handlers = new Map([
  [
    "rowvia_list_saved_agent_pending",
    () =>
      savedPendingResults.length > 0
        ? savedPendingResults.shift()
        : savedPending,
  ],
  [
    "rowvia_save_codex_agent",
    () => {
      if (savedSaveResults.length > 0) return savedSaveResults.shift();
      if (savedSaveFailure) throw new Error(savedSaveFailure);
      savedPending = [
        ...savedPending,
        {
          version: 1,
          operationId: savedOperationId,
          agentPubkey: createdPubkey,
          name: displayName,
          status: "pending",
        },
      ];
      return { version: 1, operationId: savedOperationId };
    },
  ],
  [
    "rowvia_review_saved_agent_preparation",
    () => {
      if (savedReviewFailure) throw new Error("PRIVATE_REVIEW_FAILURE");
      return ownerReview();
    },
  ],
  [
    "rowvia_approve_saved_agent_registration",
    () => {
      if (holdApproval) return holdApproval;
      if (savedApproval === "unknown" || savedApproval === "failed-cas")
        throw new Error("PRIVATE_CANARY");
      if (savedApproval === "registered-stopped") {
        createdAgent = rawAgent();
        const review = ownerReview();
        return {
          kind: "registered-stopped",
          completion: {
            version: 1,
            operationId: savedOperationId,
            agentPubkey: createdPubkey,
            canonical21: review.canonical21,
            canonical21Sha256: review.canonical21Sha256,
            state: "registered-stopped",
            bindingsReady: true,
            runtimeReady: false,
          },
        };
      }
      return {
        kind: "authorization",
        authorization: {
          version: 1,
          operationId: savedOperationId,
          state:
            savedApproval === "authorization" ? "authorized" : savedApproval,
          expiresAt: "2030-01-01T00:00:00Z",
        },
      };
    },
  ],
  ["get_relay_http_url", () => "https://a.example.invalid"],
  ["get_media_proxy_port", () => 1],
  ["discover_acp_commands", () => []],
  [
    "get_global_agent_config",
    () => ({
      env_vars: {},
      preferred_runtime: "codex",
      model: null,
      provider: null,
    }),
  ],
  ["get_baked_build_env", () => []],
  ["get_baked_build_env_keys", () => []],
  ["get_runtime_file_config", () => ({})],
  ["rowvia_list_management_pending", () => ({ receipts: [], unresolved: [] })],
  ["rowvia_list_management_operations", () => []],
  [
    "discover_agent_models",
    () => ({
      agentName: "Synthetic Codex",
      agentVersion: "fixture",
      agentDefaultModel: null,
      selectedModel: null,
      supportsSwitching: false,
      models: [],
    }),
  ],
  ["list_personas", () => rawPersonas],
  [
    "list_managed_agents",
    () => [rawAgent(sourcePubkey), ...(createdAgent ? [createdAgent] : [])],
  ],
  ["reconcile_managed_agent_runtimes", () => []],
  [
    "create_persona",
    ({ input }) => {
      const persona = {
        id: "synthetic-persona",
        display_name: input.displayName,
        system_prompt: input.systemPrompt,
        avatar_url: input.avatarUrl ?? null,
        acp_command: input.acpCommand,
        runtime: input.runtime,
        model: input.model ?? null,
        provider: input.provider ?? null,
        env_vars: input.envVars,
        is_builtin: false,
      };
      rawPersonas = [persona];
      return persona;
    },
  ],
  [
    "create_managed_agent",
    () => {
      createdAgent = rawAgent();
      return {
        agent: createdAgent,
        private_key_nsec: "synthetic-not-a-key",
        profile_sync_error: null,
        spawn_error: spawnError,
      };
    },
  ],
  [
    "add_channel_members",
    () => {
      if (membershipFailure) throw new Error("PRIVATE_MEMBERSHIP_FAILURE");
      return holdMembership ?? membershipResult;
    },
  ],
  [
    "start_managed_agent",
    () => {
      if (startFailure) throw new Error("PRIVATE_START_FAILURE");
      createdAgent = rawAgent(createdPubkey, "running");
      return createdAgent;
    },
  ],
]);
const previous = {
  internals: globalThis.__TAURI_INTERNALS__,
  windowInternals: window.__TAURI_INTERNALS__,
  fetch: globalThis.fetch,
  windowFetch: window.fetch,
  WebSocket: globalThis.WebSocket,
  windowWebSocket: window.WebSocket,
  matchMedia: window.matchMedia,
  requestAnimationFrame: window.requestAnimationFrame,
  cancelAnimationFrame: window.cancelAnimationFrame,
};
const internals = {
  invoke(command, args) {
    calls.push({ command, args });
    const handler = handlers.get(command);
    if (!handler) {
      unexpected.push(command);
      return Promise.reject(new Error(`Unexpected synthetic IPC: ${command}`));
    }
    return Promise.resolve(handler(args));
  },
};
globalThis.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ = internals;
globalThis.fetch = window.fetch = async () => {
  networkCalls++;
  throw new Error("Network forbidden in synthetic dialog test");
};
globalThis.WebSocket = window.WebSocket = class {
  constructor() {
    networkCalls++;
    throw new Error("WebSocket forbidden in synthetic dialog test");
  }
};
window.matchMedia = (media) => ({
  media,
  matches: false,
  addEventListener() {},
  removeEventListener() {},
});
window.requestAnimationFrame = (callback) =>
  setTimeout(() => callback(performance.now()), 0);
window.cancelAnimationFrame = clearTimeout;

const { AgentManagementDialogs } = await import("./AgentManagementDialogs.tsx");
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

function ScopedDialogs() {
  const { activeCommunity, switchCommunity } = useCommunities();
  // App.tsx uses this same community-keyed remount boundary for AppReady.
  return React.createElement(
    React.Fragment,
    null,
    React.createElement(
      "button",
      {
        type: "button",
        onClick: () => switchCommunity("synthetic-b"),
      },
      "Switch synthetic community",
    ),
    React.createElement(AgentManagementDialogs, { key: activeCommunity?.id }),
  );
}

function mountDialogs() {
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
  client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: Infinity, gcTime: 0 },
      mutations: { retry: false },
    },
  });
  for (const [key, value] of [
    [["identity"], { pubkey: ownerPubkey }],
    [["personas"], []],
    [["managed-agents"], [{ pubkey: sourcePubkey, status: "stopped" }]],
    [
      ["channels"],
      [
        {
          id: channelId,
          name: "synthetic-origin",
          isMember: true,
          memberPubkeys: [ownerPubkey, sourcePubkey],
        },
      ],
    ],
    [["acp-runtimes"], [runtime]],
    [["projects"], []],
    [["channel-templates"], []],
    [["teams"], []],
    [["acp-commands"], []],
    [["backend-providers"], []],
    [["baked-build-env"], []],
    [["baked-build-env-keys"], []],
    [["runtime-file-config", "codex"], {}],
    [
      ["globalAgentConfig"],
      { env_vars: {}, preferred_runtime: "codex", model: null, provider: null },
    ],
  ])
    client.setQueryData(key, value);
  _testRegisterKnownAgents("synthetic-create-dialog", [sourcePubkey]);
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
          React.createElement(ScopedDialogs),
        ),
      ),
    ),
  );
}

const mutations = () =>
  calls.filter(({ command }) => mutationCommands.has(command));

function assertNoMutations() {
  assert.deepEqual(
    mutations(),
    [],
    "draft review must not create, attach or start an agent",
  );
  assert.equal(
    calls.some(({ command }) => command === "reconcile_managed_agent_runtimes"),
    false,
    "draft review must not reconcile runtime pairs",
  );
}

async function deliverDraft(sequence = 1) {
  const before = mutations();
  const event = {
    id: sequence.toString(16).padStart(64, "0"),
    pubkey: sourcePubkey,
    created_at: 1,
    kind: 1,
    tags: [
      ["agent", sourcePubkey],
      ["frame", "telemetry"],
    ],
    content: "synthetic-ciphertext",
    sig: "e".repeat(128),
  };
  await act(async () => {
    await _testHandleRelayObserverEvent(event, async (received) => {
      assert.equal(received, event);
      return {
        seq: sequence,
        timestamp: "2026-10-20T12:00:00Z",
        kind: "agent_management_request",
        agentIndex: null,
        channelId,
        sessionId: null,
        turnId: null,
        payload: {
          type: "agent_management_request",
          action: "create",
          requestId: `synthetic-create-${sequence}`,
          request: { channelId, displayName, systemPrompt },
        },
      };
    });
  });
  assert.equal((await screen.findByLabelText("Agent name")).value, displayName);
  assert.ok(screen.getByDisplayValue(systemPrompt));
  const create = screen.getByRole("button", { name: "Create agent" });
  await waitFor(() => assert.equal(create.disabled, false));
  assert.deepEqual(
    mutations(),
    before,
    "draft delivery must not mutate agents",
  );
  return create;
}

afterEach(() => {
  cleanup();
  client?.clear();
  client = undefined;
  resetAgentObserverStore();
  localStorage.clear();
  rawPersonas = [];
  createdAgent = null;
  spawnError = null;
  savedPending = [];
  savedPendingResults = [];
  savedOperationId = savedOperation;
  savedReviewDelta = {};
  savedReviewFailure = false;
  savedApproval = "authorization";
  holdApproval = null;
  savedSaveFailure = null;
  savedSaveResults = [];
  membershipResult = { added: [createdPubkey], errors: [] };
  membershipFailure = false;
  holdMembership = null;
  startFailure = false;
  calls.length = 0;
  assert.deepEqual(
    unexpected.splice(0),
    [],
    "IPC must stay inside the explicit synthetic allowlist",
  );
  assert.equal(networkCalls, 0);
});
after(() => {
  globalThis.__TAURI_INTERNALS__ = previous.internals;
  window.__TAURI_INTERNALS__ = previous.windowInternals;
  globalThis.fetch = previous.fetch;
  window.fetch = previous.windowFetch;
  globalThis.WebSocket = previous.WebSocket;
  window.WebSocket = previous.windowWebSocket;
  window.matchMedia = previous.matchMedia;
  window.requestAnimationFrame = previous.requestAnimationFrame;
  window.cancelAnimationFrame = previous.cancelAnimationFrame;
});

async function openSavedPending() {
  savedPending = [
    {
      version: 1,
      operationId: savedOperation,
      agentPubkey: createdPubkey,
      name: displayName,
      status: "pending",
    },
  ];
  mountDialogs();
  fireEvent.click(
    await screen.findByRole("button", {
      name: `Review saved Codex: ${displayName}`,
    }),
  );
  await screen.findByRole("button", { name: "Approve registration" });
}

test("saved Codex reuses name and prompt form and sends only the fixed four fields", async () => {
  mountDialogs();
  await deliverDraft();
  fireEvent.change(screen.getByLabelText("Save type"), {
    target: { value: "saved-codex" },
  });
  assert.equal(screen.getByLabelText("Name").value, displayName);
  assert.equal(screen.getByLabelText("Agent instructions").value, systemPrompt);
  assert.equal(screen.queryByRole("button", { name: "Advanced" }), null);
  fireEvent.click(
    screen.getByRole("button", { name: "Save for registration review" }),
  );
  await screen.findByRole("button", { name: "Approve registration" });
  const save = calls.filter(
    ({ command }) => command === "rowvia_save_codex_agent",
  );
  assert.equal(save.length, 1);
  assert.deepEqual(save[0].args, {
    input: {
      displayName,
      systemPrompt,
      channelId,
      recipe: "codex-development-memory-git-read-v1",
    },
  });
  assertNoMutations();
  assert.equal(
    calls.filter(
      ({ command }) => command === "rowvia_approve_saved_agent_registration",
    ).length,
    0,
  );
});

test("newly saved review reopens after Cancel despite a late initial pending inventory", async () => {
  let settleInitial;
  savedPendingResults = [
    new Promise((resolve) => {
      settleInitial = resolve;
    }),
  ];
  mountDialogs();
  await deliverDraft();
  fireEvent.change(screen.getByLabelText("Save type"), {
    target: { value: "saved-codex" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Save for registration review" }),
  );
  await screen.findByRole("button", { name: "Approve registration" });
  await act(async () => settleInitial([]));
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  fireEvent.click(
    await screen.findByRole("button", {
      name: `Review saved Codex: ${displayName}`,
    }),
  );
  await screen.findByRole("button", { name: "Approve registration" });
  assert.equal(
    calls.filter(({ command }) => command === "rowvia_save_codex_agent").length,
    1,
  );
  assert.equal(
    calls.filter(
      ({ command }) => command === "rowvia_review_saved_agent_preparation",
    ).length,
    2,
  );
  assert.equal(
    calls.filter(
      ({ command }) => command === "rowvia_approve_saved_agent_registration",
    ).length,
    0,
  );
  assertNoMutations();
});

test("known Save retains Native pending recovery when its first review fails", async () => {
  savedReviewFailure = true;
  mountDialogs();
  await deliverDraft();
  fireEvent.change(screen.getByLabelText("Save type"), {
    target: { value: "saved-codex" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Save for registration review" }),
  );
  await screen.findByText("Saved Codex registration is unavailable.");
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  savedReviewFailure = false;
  fireEvent.click(
    await screen.findByRole("button", {
      name: `Review saved Codex: ${displayName}`,
    }),
  );
  await screen.findByRole("button", { name: "Approve registration" });
  assert.equal(
    calls.filter(({ command }) => command === "rowvia_save_codex_agent").length,
    1,
  );
  assert.equal(
    calls.filter(
      ({ command }) => command === "rowvia_approve_saved_agent_registration",
    ).length,
    0,
  );
  assertNoMutations();
});

test("completed saved registration allows a distinct Save without remounting", async () => {
  savedApproval = "registered-stopped";
  mountDialogs();
  await deliverDraft();
  fireEvent.change(screen.getByLabelText("Save type"), {
    target: { value: "saved-codex" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Save for registration review" }),
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "Approve registration" }),
  );
  await screen.findByText("Registered · Stopped");
  fireEvent.click(screen.getByRole("button", { name: "Start" }));
  await screen.findByText("Start request completed.");
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  savedPending = [];
  savedOperationId = "01940000-0000-7000-8000-000000000078";
  await deliverDraft(2);
  fireEvent.change(screen.getByLabelText("Save type"), {
    target: { value: "saved-codex" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Save for registration review" }),
  );
  await screen.findByRole("button", { name: "Approve registration" });
  assert.ok(screen.getAllByText(savedOperationId).length > 0);
  assert.equal(
    calls.filter(({ command }) => command === "rowvia_save_codex_agent").length,
    2,
  );
  assert.equal(
    calls.filter(
      ({ command }) => command === "rowvia_approve_saved_agent_registration",
    ).length,
    1,
  );
  assert.deepEqual(
    mutations().map(({ command }) => command),
    ["add_channel_members", "start_managed_agent"],
  );
});

test("unknown approval remains nonretryable after Cancel and review reopening", async () => {
  savedApproval = "unknown";
  mountDialogs();
  await deliverDraft();
  fireEvent.change(screen.getByLabelText("Save type"), {
    target: { value: "saved-codex" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Save for registration review" }),
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "Approve registration" }),
  );
  await screen.findByText(/Outcome unknown/u);
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  fireEvent.click(
    await screen.findByRole("button", {
      name: `Review saved Codex: ${displayName}`,
    }),
  );
  await screen.findByText(/Outcome unknown/u);
  assert.equal(
    screen.queryByRole("button", { name: "Approve registration" }),
    null,
  );
  assert.equal(screen.queryByRole("button", { name: "Start" }), null);
  assert.equal(
    calls.filter(({ command }) => command === "rowvia_save_codex_agent").length,
    1,
  );
  assert.equal(
    calls.filter(
      ({ command }) => command === "rowvia_approve_saved_agent_registration",
    ).length,
    1,
  );
  assertNoMutations();
});

test("completing another pending operation cannot release an unknown Save", async () => {
  savedPending = [
    {
      version: 1,
      operationId: savedOperation,
      agentPubkey: createdPubkey,
      name: displayName,
      status: "pending",
    },
  ];
  savedSaveFailure = "PRIVATE_UNKNOWN_SAVE";
  savedApproval = "registered-stopped";
  mountDialogs();
  await deliverDraft();
  fireEvent.change(screen.getByLabelText("Save type"), {
    target: { value: "saved-codex" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Save for registration review" }),
  );
  await screen.findByText(/Outcome unknown/u);
  fireEvent.click(
    within(
      screen.getByRole("dialog", { name: "Saved Codex registration" }),
    ).getByRole("button", { name: "Cancel" }),
  );
  savedSaveFailure = null;
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  fireEvent.click(
    await screen.findByRole("button", {
      name: `Review saved Codex: ${displayName}`,
    }),
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "Approve registration" }),
  );
  await screen.findByText("Registered · Stopped");
  fireEvent.click(
    within(
      screen.getByRole("dialog", { name: "Saved Codex registration" }),
    ).getByRole("button", { name: "Cancel" }),
  );
  await deliverDraft(2);
  fireEvent.change(screen.getByLabelText("Save type"), {
    target: { value: "saved-codex" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Save for registration review" }),
  );
  await act(async () => {});
  assert.equal(
    calls.filter(({ command }) => command === "rowvia_save_codex_agent").length,
    1,
  );
  assert.equal(
    calls.filter(
      ({ command }) => command === "rowvia_approve_saved_agent_registration",
    ).length,
    1,
  );
  assertNoMutations();
});

test("review shows every exact public field and original JSON before approval; Cancel is inert", async () => {
  await openSavedPending();
  const review = ownerReview();
  assert.ok(screen.getByText(review.canonical21));
  for (const [field, value] of Object.entries(JSON.parse(review.canonical21))) {
    assert.ok(screen.getByText(field));
    assert.ok(
      screen.getAllByText(
        Array.isArray(value) ? value.join(", ") : String(value),
      ).length > 0,
    );
  }
  assert.equal(
    calls.filter(
      ({ command }) => command === "rowvia_approve_saved_agent_registration",
    ).length,
    0,
  );
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  await waitFor(() => assert.ok(!screen.queryByRole("dialog")));
  assertNoMutations();
  assert.equal(
    calls.filter(
      ({ command }) =>
        command === "rowvia_save_codex_agent" ||
        command === "rowvia_approve_saved_agent_registration",
    ).length,
    0,
  );
});

for (const state of [
  "authorization",
  "expired",
  "revoked",
  "unknown",
  "failed-cas",
]) {
  test(`${state} cannot enable Start or retry approval`, async () => {
    savedApproval = state;
    await openSavedPending();
    const button = screen.getByRole("button", { name: "Approve registration" });
    fireEvent.click(button);
    fireEvent.click(button);
    await waitFor(() =>
      assert.equal(
        screen.queryByRole("button", { name: "Approve registration" }),
        null,
      ),
    );
    await waitFor(() =>
      assert.match(
        screen.getByRole("status").textContent,
        state === "authorization"
          ? /Authorized/u
          : state === "unknown" || state === "failed-cas"
            ? /Outcome unknown/u
            : new RegExp(state, "u"),
      ),
    );
    assert.equal(screen.queryByRole("button", { name: "Start" }), null);
    assert.equal(
      calls.filter(
        ({ command }) => command === "rowvia_approve_saved_agent_registration",
      ).length,
      1,
    );
    assertNoMutations();
  });
}

test("completed registration attaches the reviewed channel only on explicit Start before starting exact identity", async () => {
  let settleMembership;
  holdMembership = new Promise((resolve) => {
    settleMembership = resolve;
  });
  savedApproval = "registered-stopped";
  await openSavedPending();
  const before = calls.filter(
    ({ command }) => command === "list_managed_agents",
  ).length;
  fireEvent.click(screen.getByRole("button", { name: "Approve registration" }));
  assert.ok(await screen.findByText("Registered · Stopped"));
  await waitFor(() =>
    assert.ok(
      calls.filter(({ command }) => command === "list_managed_agents").length >
        before,
    ),
  );
  assertNoMutations();
  fireEvent.click(screen.getByRole("button", { name: "Start" }));
  await waitFor(() =>
    assert.equal(
      calls.filter(({ command }) => command === "add_channel_members").length,
      1,
    ),
  );
  assert.deepEqual(
    mutations().map(({ command }) => command),
    ["add_channel_members"],
  );
  assert.deepEqual(
    calls.find(({ command }) => command === "add_channel_members").args,
    {
      channelId,
      pubkeys: [createdPubkey],
      role: "bot",
      expectedRelayUrl: "wss://a.example.invalid",
      expectedSignerPubkey: ownerPubkey,
    },
  );
  await act(async () => settleMembership(membershipResult));
  await waitFor(() =>
    assert.equal(
      calls.filter(({ command }) => command === "start_managed_agent").length,
      1,
    ),
  );
  assert.deepEqual(
    calls.find(({ command }) => command === "start_managed_agent").args,
    {
      pubkey: createdPubkey,
      expectedRelayUrl: "wss://a.example.invalid",
      expectedSignerPubkey: ownerPubkey,
      replayFloorUnix: null,
    },
  );
  assert.deepEqual(
    mutations().map(({ command }) => command),
    ["add_channel_members", "start_managed_agent"],
  );
});

for (const failure of ["target-error", "missing-target", "rejected"]) {
  test(`attachment ${failure} keeps registration stopped and retry never reapproves`, async () => {
    savedApproval = "registered-stopped";
    if (failure === "rejected") membershipFailure = true;
    else
      membershipResult = {
        added: failure === "target-error" ? [createdPubkey] : [],
        errors:
          failure === "target-error"
            ? [{ pubkey: createdPubkey, error: "PRIVATE_MEMBERSHIP_FAILURE" }]
            : [],
      };
    await openSavedPending();
    fireEvent.click(
      screen.getByRole("button", { name: "Approve registration" }),
    );
    await screen.findByText("Registered · Stopped");
    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    assert.match((await screen.findByRole("alert")).textContent, /channel/u);
    assert.ok(screen.getByText("Registered · Stopped"));
    assert.equal(
      screen.queryByRole("button", { name: "Approve registration" }),
      null,
    );
    assert.equal(
      calls.filter(({ command }) => command === "start_managed_agent").length,
      0,
    );
    assert.equal(screen.queryByText(/PRIVATE_MEMBERSHIP_FAILURE/u), null);
    membershipFailure = false;
    membershipResult = { added: [createdPubkey], errors: [] };
    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    await screen.findByText("Start request completed.");
    assert.equal(screen.queryByRole("alert"), null);
    assert.equal(
      calls.filter(({ command }) => command === "add_channel_members").length,
      2,
    );
    assert.equal(
      calls.filter(({ command }) => command === "start_managed_agent").length,
      1,
    );
    assert.equal(
      calls.filter(
        ({ command }) => command === "rowvia_approve_saved_agent_registration",
      ).length,
      1,
    );
  });
}

for (const changedScope of ["owner", "community", "dismiss"]) {
  test(`${changedScope} scope change during attachment prevents runtime Start`, async () => {
    let settleMembership;
    holdMembership = new Promise((resolve) => {
      settleMembership = resolve;
    });
    savedApproval = "registered-stopped";
    await openSavedPending();
    fireEvent.click(
      screen.getByRole("button", { name: "Approve registration" }),
    );
    await screen.findByText("Registered · Stopped");
    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    await waitFor(() =>
      assert.equal(
        calls.filter(({ command }) => command === "add_channel_members").length,
        1,
      ),
    );
    if (changedScope === "owner")
      await act(async () =>
        client.setQueryData(["identity"], { pubkey: "9".repeat(64) }),
      );
    else if (changedScope === "community")
      fireEvent.click(screen.getByText("Switch synthetic community"));
    else fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => assert.ok(!screen.queryByRole("dialog")));
    await act(async () => settleMembership(membershipResult));
    assert.equal(
      calls.filter(({ command }) => command === "start_managed_agent").length,
      0,
    );
    assert.equal(screen.queryByText("Start request completed."), null);
  });
}

test("actual Start rejection after attachment remains unknown without retry", async () => {
  savedApproval = "registered-stopped";
  startFailure = true;
  await openSavedPending();
  fireEvent.click(screen.getByRole("button", { name: "Approve registration" }));
  await screen.findByText("Registered · Stopped");
  fireEvent.click(screen.getByRole("button", { name: "Start" }));
  await screen.findByText(/Outcome unknown/u);
  assert.equal(screen.queryByRole("alert"), null);
  assert.equal(screen.queryByRole("button", { name: "Start" }), null);
  assert.equal(
    screen.queryByRole("button", { name: "Approve registration" }),
    null,
  );
  assert.equal(screen.queryByText(/PRIVATE_START_FAILURE/u), null);
  assert.deepEqual(
    mutations().map(({ command }) => command),
    ["add_channel_members", "start_managed_agent"],
  );
});

for (const delta of [
  { canonical21: "{}" },
  { canonical21: "x".repeat(4097) },
  { canonical21Sha256: "9".repeat(64) },
  { canonical21: canonical({ agent_pubkey: "9".repeat(64) }) },
  { ownerPubkey: "9".repeat(64) },
  { recipeId: "codex-local-v1" },
]) {
  test(`malformed or mismatched review refuses before owner action: ${Object.keys(delta)}`, async () => {
    savedReviewDelta = delta;
    savedPending = [
      {
        version: 1,
        operationId: savedOperation,
        agentPubkey: createdPubkey,
        name: displayName,
        status: "pending",
      },
    ];
    mountDialogs();
    fireEvent.click(
      await screen.findByRole("button", {
        name: `Review saved Codex: ${displayName}`,
      }),
    );
    assert.ok(
      await screen.findByText("Saved Codex registration is unavailable."),
    );
    assert.equal(
      screen.queryByRole("button", { name: "Approve registration" }),
      null,
    );
    assertNoMutations();
  });
}

test("scope remount discards an in-flight approval result without cache refresh or Start", async () => {
  let settle;
  holdApproval = new Promise((resolve) => {
    settle = resolve;
  });
  await openSavedPending();
  fireEvent.click(screen.getByRole("button", { name: "Approve registration" }));
  await screen.findByText("Authorizing registration…");
  fireEvent.click(screen.getByText("Switch synthetic community"));
  await waitFor(() => assert.ok(!screen.queryByRole("dialog")));
  const review = ownerReview();
  await act(async () => {
    settle({
      kind: "registered-stopped",
      completion: {
        version: 1,
        operationId: savedOperation,
        agentPubkey: createdPubkey,
        canonical21: review.canonical21,
        canonical21Sha256: review.canonical21Sha256,
        state: "registered-stopped",
        bindingsReady: true,
        runtimeReady: false,
      },
    });
  });
  assert.equal(screen.queryByText("Registered · Stopped"), null);
  assert.equal(screen.queryByRole("button", { name: "Start" }), null);
  assertNoMutations();
});

test("owner change abandons the reviewed operation before approval", async () => {
  await openSavedPending();
  await act(async () => {
    client.setQueryData(["identity"], { pubkey: "9".repeat(64) });
  });
  await waitFor(() => assert.ok(!screen.queryByRole("dialog")));
  assert.equal(
    calls.filter(
      ({ command }) => command === "rowvia_approve_saved_agent_registration",
    ).length,
    0,
  );
  assertNoMutations();
});

test("unavailable fixed Save never falls back to generic creation", async () => {
  savedSaveFailure = "saved_agent_owner_unavailable";
  mountDialogs();
  await deliverDraft();
  fireEvent.change(screen.getByLabelText("Save type"), {
    target: { value: "saved-codex" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Save for registration review" }),
  );
  assert.ok(
    await screen.findByText("Saved Codex registration is unavailable."),
  );
  assert.equal(
    calls.filter(({ command }) => command === "rowvia_save_codex_agent").length,
    1,
  );
  assertNoMutations();
});

test("old owner unavailable cannot clear a newer owner unknown Save latch", async () => {
  let rejectOld;
  let rejectNew;
  savedSaveResults = [
    new Promise((_, reject) => {
      rejectOld = reject;
    }),
    new Promise((_, reject) => {
      rejectNew = reject;
    }),
  ];
  mountDialogs();
  await deliverDraft();
  fireEvent.change(screen.getByLabelText("Save type"), {
    target: { value: "saved-codex" },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Save for registration review" }),
  );
  await screen.findByText("Loading registration review…");
  await act(async () => {
    client.setQueryData(["identity"], { pubkey: "9".repeat(64) });
  });
  await waitFor(() =>
    assert.ok(!screen.queryByText("Loading registration review…")),
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Save for registration review" }),
  );
  await screen.findByText("Loading registration review…");
  await act(async () => {
    rejectOld(new Error("saved_agent_owner_unavailable"));
  });
  assert.ok(screen.getByText("Loading registration review…"));
  await act(async () => {
    rejectNew(new Error("PRIVATE_NEW_OWNER_UNKNOWN"));
  });
  await screen.findByText(/Outcome unknown/u);
  const dialog = screen.getByRole("dialog", {
    name: "Saved Codex registration",
  });
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  await waitFor(() => assert.ok(!screen.queryByText(/Outcome unknown/u)));
  fireEvent.click(
    screen.getByRole("button", { name: "Save for registration review" }),
  );
  await act(async () => {});
  assert.equal(
    calls.filter(({ command }) => command === "rowvia_save_codex_agent").length,
    2,
  );
  assert.equal(
    calls.filter(
      ({ command }) =>
        command === "rowvia_review_saved_agent_preparation" ||
        command === "rowvia_approve_saved_agent_registration",
    ).length,
    0,
  );
  assertNoMutations();
});
