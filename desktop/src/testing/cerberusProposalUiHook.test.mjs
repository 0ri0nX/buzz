import assert from "node:assert/strict";
import { register } from "node:module";
import { after, test } from "node:test";
import { JSDOM } from "jsdom";

// Supply Vite's compile-time opt-in to the unchanged production hook in Node.
// All imports, hook decisions and cache reads otherwise use their real modules.
register(
  `data:text/javascript,${encodeURIComponent(`
  export async function load(url, context, next) {
    const result = await next(url, context);
    if (!url.endsWith('/features/agents/useAgentManagement.ts')) return result;
    return { ...result, source: result.source.toString().replace(
      'import.meta.env?.VITE_ROWVIA_OWNER_TEST_HOOK', '"1"') };
  }
`)}`,
  import.meta.url,
);

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "https://fixture.test",
  pretendToBeVisual: true,
});
Object.assign(globalThis, {
  window: dom.window,
  document: dom.window.document,
  localStorage: dom.window.localStorage,
  IS_REACT_ACT_ENVIRONMENT: true,
});
let nativeCalls = 0;
let networkCalls = 0;
globalThis.fetch = async () => {
  networkCalls++;
  throw new Error("unexpected network call");
};
globalThis.WebSocket = class {
  constructor() {
    networkCalls++;
    throw new Error("unexpected socket");
  }
};
window.__TAURI_INTERNALS__ = {
  invoke: async () => {
    nativeCalls++;
    throw new Error("unexpected native call");
  },
};
const { createElement } = await import("react");
const { QueryClient, QueryClientProvider } = await import(
  "@tanstack/react-query"
);
const { renderHook, act, cleanup } = await import("@testing-library/react");
const { CommunitiesProvider } = await import(
  "../features/communities/useCommunities.tsx"
);
const { useAgentManagement } = await import(
  "../features/agents/useAgentManagement.ts"
);
const { relayClient } = await import("../shared/api/relayClient.ts");
const {
  inspectCerberusProposalUi,
  resetCerberusProposalUiDiagnostic,
  CERBERUS_PROPOSAL_AGENT,
  CERBERUS_PROPOSAL_CHANNEL,
} = await import("./cerberusProposalUiDiagnostic.ts");
after(() => {
  cleanup();
  dom.window.close();
});

test("mounted production management hook reads existing cache only and unregisters on unmount", async () => {
  const expected = {
    ownerPubkey: "a".repeat(64),
    relayUrl: "wss://fixture.test",
  };
  relayClient.relayUrl = expected.relayUrl;
  localStorage.setItem(
    "buzz-communities",
    JSON.stringify([
      { id: "fixture", name: "Fixture", relayUrl: expected.relayUrl },
    ]),
  );
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: Infinity, gcTime: Infinity },
    },
  });
  for (const [key, value] of [
    ["identity", { pubkey: expected.ownerPubkey }],
    ["personas", []],
    ["managed-agents", []],
    ["acp-runtimes", []],
    [
      "relay-agents",
      [{ pubkey: CERBERUS_PROPOSAL_AGENT, ownerPubkey: expected.ownerPubkey }],
    ],
    [
      "channels",
      [
        {
          id: CERBERUS_PROPOSAL_CHANNEL,
          isMember: true,
          memberPubkeys: [CERBERUS_PROPOSAL_AGENT],
        },
      ],
    ],
  ]) {
    client.setQueryData([key], value);
  }
  const wrapper = ({ children }) =>
    createElement(
      QueryClientProvider,
      { client },
      createElement(CommunitiesProvider, null, children),
    );
  const mounted = renderHook(useAgentManagement, { wrapper });
  try {
    await act(async () => {});
    const beforeNativeCalls = nativeCalls;
    assert.equal(inspectCerberusProposalUi(expected).hookMounted, true);
    assert.equal(inspectCerberusProposalUi(expected).relayOwnedCached, true);
    assert.equal(
      inspectCerberusProposalUi(expected).locallyManagedCached,
      false,
    );
    assert.equal(inspectCerberusProposalUi(expected).ownerMemberCached, true);
    assert.equal(inspectCerberusProposalUi(expected).agentMemberCached, true);
    assert.equal(mounted.result.current.request, null);
    const before = client
      .getQueryCache()
      .getAll()
      .map((query) => query.state);
    for (let index = 0; index < 5; index++) inspectCerberusProposalUi(expected);
    assert.deepEqual(
      client
        .getQueryCache()
        .getAll()
        .map((query) => query.state),
      before,
    );
    assert.equal(nativeCalls, beforeNativeCalls);
    assert.equal(networkCalls, 0);
    assert.equal(client.isFetching(), 0);
    assert.equal(client.isMutating(), 0);
    await act(async () => {
      client.setQueryData(["identity"], { pubkey: "b".repeat(64) });
    });
    assert.equal(inspectCerberusProposalUi(expected), null);
    resetCerberusProposalUiDiagnostic();
    mounted.rerender();
    assert.equal(inspectCerberusProposalUi(expected), null);
    mounted.unmount();
    client.setQueryData(["identity"], { pubkey: expected.ownerPubkey });
    // A separate module instance receives no Vite opt-in substitution.
    const { useAgentManagement: disabledHook } = await import(
      "../features/agents/useAgentManagement.ts?without-owner-test-opt-in"
    );
    const disabled = renderHook(disabledHook, { wrapper });
    try {
      await act(async () => {});
      assert.equal(inspectCerberusProposalUi(expected), null);
    } finally {
      disabled.unmount();
    }
  } finally {
    mounted.unmount();
    client.clear();
  }
  assert.equal(inspectCerberusProposalUi(expected), null);
});
