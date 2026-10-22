import assert from "node:assert/strict";
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
  ["rowvia_list_saved_agent_pending", () => []],
  ["get_relay_http_url", () => "https://a.example.invalid"],
  ["get_media_proxy_port", () => 1],
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
  ["add_channel_members", () => ({ added: [createdPubkey], errors: [] })],
  [
    "start_managed_agent",
    () => {
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

async function deliverDraft() {
  const event = {
    id: "d".repeat(64),
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
        seq: 1,
        timestamp: "2026-10-20T12:00:00Z",
        kind: "agent_management_request",
        agentIndex: null,
        channelId,
        sessionId: null,
        turnId: null,
        payload: {
          type: "agent_management_request",
          action: "create",
          requestId: "synthetic-create-1",
          request: { channelId, displayName, systemPrompt },
        },
      };
    });
  });
  assert.equal((await screen.findByLabelText("Agent name")).value, displayName);
  assert.ok(screen.getByDisplayValue(systemPrompt));
  const create = screen.getByRole("button", { name: "Create agent" });
  await waitFor(() => assert.equal(create.disabled, false));
  assertNoMutations();
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

test("real creation dialog Cancel dismisses an observer draft with zero mutations", async () => {
  mountDialogs();
  await deliverDraft();
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  await waitFor(() => assert.ok(!screen.queryByRole("dialog")));
  assertNoMutations();
});

test("visible Create agent saves the persona, creates its instance, attaches and ensures running in order", async () => {
  mountDialogs();
  const create = await deliverDraft();
  fireEvent.click(create);
  await waitFor(() => assert.ok(!screen.queryByRole("dialog")));
  assert.deepEqual(
    mutations().map(({ command }) => command),
    [
      "create_persona",
      "create_managed_agent",
      "add_channel_members",
      "start_managed_agent",
    ],
  );
  const [persona, instance, attach, start] = mutations();
  assert.equal(persona.args.input.displayName, displayName);
  assert.equal(persona.args.input.systemPrompt, systemPrompt);
  assert.equal(persona.args.input.runtime, "codex");
  assert.equal(persona.args.input.acpCommand, "buzz-acp");
  assert.equal(instance.args.input.name, displayName);
  assert.equal(instance.args.input.personaId, "synthetic-persona");
  assert.equal(instance.args.input.systemPrompt, systemPrompt);
  assert.equal(instance.args.input.agentCommand, runtime.command);
  assert.equal(instance.args.input.spawnAfterCreate, true);
  assert.equal(instance.args.input.startOnAppLaunch, true);
  assert.deepEqual(instance.args.input.backend, { type: "local" });
  assert.deepEqual(attach.args, {
    channelId,
    pubkeys: [createdPubkey],
    role: "bot",
  });
  assert.deepEqual(start.args, {
    pubkey: createdPubkey,
    expectedRelayUrl: null,
    expectedSignerPubkey: null,
    replayFloorUnix: null,
  });
});

test("spawnError stays visible in the real dialog and prevents channel attachment or start", async () => {
  spawnError = "Synthetic runtime could not start";
  mountDialogs();
  fireEvent.click(await deliverDraft());
  assert.ok(await screen.findByText(spawnError));
  assert.ok(screen.getByRole("dialog"));
  assert.deepEqual(
    mutations().map(({ command }) => command),
    ["create_persona", "create_managed_agent"],
  );
});

for (const drift of ["owner leaves", "source agent leaves"]) {
  test(`${drift} after review opens blocks every creation mutation`, async () => {
    mountDialogs();
    const create = await deliverDraft();
    await act(async () => {
      client.setQueryData(
        ["channels"],
        [
          {
            id: channelId,
            name: "synthetic-origin",
            isMember: drift !== "owner leaves",
            memberPubkeys:
              drift === "source agent leaves"
                ? [ownerPubkey]
                : [ownerPubkey, sourcePubkey],
          },
        ],
      );
    });
    await waitFor(() => assert.equal(client.isFetching(), 0));
    fireEvent.click(create);
    assert.ok(
      await screen.findByText(
        "An agent can only manage agents from a channel you both belong to.",
      ),
    );
    assertNoMutations();
  });
}

test("community remount abandons the old creation draft before any mutation", async () => {
  const view = mountDialogs();
  await deliverDraft();
  fireEvent.click(view.getByText("Switch synthetic community"));
  await waitFor(() => assert.ok(!screen.queryByRole("dialog")));
  assert.equal(localStorage.getItem("buzz-active-community-id"), "synthetic-b");
  assert.equal(screen.queryByRole("button", { name: "Create agent" }), null);
  assertNoMutations();
});
