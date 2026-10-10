import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import test from "node:test";

const ownerPubkey = "a".repeat(64);
const architectPubkey = "b".repeat(64);
const relayUrl = "ws://localhost:3000";
const channelId = "a1234567-1234-4234-8234-123456789abc";
let identity = ownerPubkey;
let relay = relayUrl;
let membersResult = { added: [architectPubkey], errors: [] };
const calls = [];
let events = [];
let onSign = () => {};
let onConnect = () => {};
let createdChannelId = channelId;
let sendCount = 0;
let onNativeSend = () => {};
const nativeOverrides = new Map();
const eventCallbacks = new Map();
const eventListeners = new Map();
let nextCallbackId = 0;
const ownerRequestEvent = "rowvia-owner-test-request";
globalThis.window = {
  setTimeout,
  clearTimeout,
  __TAURI_EVENT_PLUGIN_INTERNALS__: {
    unregisterListener: (event, eventId) => {
      assert.equal(eventListeners.get(eventId).event, event);
      eventCallbacks.delete(eventListeners.get(eventId).handler);
      eventListeners.delete(eventId);
    },
  },
  __TAURI_INTERNALS__: {
    transformCallback: (callback) => {
      const id = ++nextCallbackId;
      eventCallbacks.set(id, callback);
      return id;
    },
    invoke: async (command, args) => {
      calls.push({ command, args });
      if (nativeOverrides.has(command))
        return nativeOverrides.get(command)(args);
      if (command === "plugin:event|listen") {
        const id = eventListeners.size + 1;
        eventListeners.set(id, args);
        return id;
      }
      if (
        command === "plugin:event|unlisten" ||
        command === "rowvia_owner_test_reply" ||
        command === "rowvia_owner_test_phase"
      )
        return;
      if (command === "get_identity")
        return { pubkey: identity, display_name: "Owner" };
      if (command === "get_relay_ws_url") return relay;
      if (command === "create_channel")
        return {
          id: createdChannelId,
          name: args.name,
          channel_type: "stream",
          visibility: "private",
          member_count: 1,
        };
      if (command === "add_channel_members") return membersResult;
      if (command === "send_channel_message") {
        const result = {
          event_id: (++sendCount).toString(16).padStart(64, "0"),
          parent_event_id: args.parentEventId,
          root_event_id: args.rootEventId,
          depth: args.parentEventId
            ? args.parentEventId === args.rootEventId
              ? 1
              : 2
            : 0,
          created_at: 1,
        };
        onNativeSend(result);
        return result;
      }
      if (command === "sign_event") {
        onSign();
        return JSON.stringify({
          id: "c".repeat(64),
          pubkey: identity,
          created_at: 1,
          sig: "d".repeat(128),
          ...args,
        });
      }
      throw new Error(`Unexpected native command ${command}`);
    },
  },
};
const { relayClient } = await import("../shared/api/relayClient.ts");
const { RelayClient } = await import("../shared/api/relayClientSession.ts");
const { activateRateLimit, resetRateLimitGate } = await import(
  "../shared/api/relayRateLimitGate.ts"
);
const { createOwnerTestHandler, installOwnerTestHook } = await import(
  "./ownerTestHook.ts"
);
const { OWNER_TEST_SCHEMA, OWNER_TEST_ENSURE_TARGET_PUBKEYS } = await import(
  "./ownerTestHookProtocol.ts"
);
const { createOwnerTestPhaseReporter } = await import(
  "./ownerTestHookPhases.ts"
);

// Mock native invoke and the session/transport boundary; retain the stock
// native send wrapper, independent JS publisher, reads and mention handling.
relayClient.ensureConnected = async () => {
  onConnect();
  return 1;
};
relayClient.publishEvent = async (event) => {
  calls.push({ command: "publish", args: event });
  return event;
};
relayClient.fetchHistory = async (filter) => {
  calls.push({ command: "history", args: filter });
  return events;
};

function reset() {
  identity = ownerPubkey;
  relay = relayUrl;
  calls.length = 0;
  events = [];
  membersResult = { added: [architectPubkey], errors: [] };
  onSign = () => {};
  onConnect = () => {};
  onNativeSend = () => {};
  createdChannelId = channelId;
  sendCount = 0;
  nativeOverrides.clear();
  eventCallbacks.clear();
  eventListeners.clear();
  relayClient.relayUrl = relayUrl;
  relayClient.connectionStateEmitter.set("connected");
}
function request(operation, args = {}, overrides = {}) {
  return {
    schema: OWNER_TEST_SCHEMA,
    requestId: randomUUID(),
    operation,
    expected: { ownerPubkey, architectPubkey, relayUrl },
    arguments: args,
    ...overrides,
  };
}

// owner_test_hook.rs tests::request(): serde uses camelCase for Request and
// Expected. Keep both pinned public keys and the HTTPS relay of that fixture.
function nativeStatusRequest() {
  return JSON.parse(
    JSON.stringify({
      schema: "rowvia.buzz.owner-test/v1",
      requestId: randomUUID(),
      operation: "status",
      expected: {
        ownerPubkey:
          "89af092923ad49e3b9916b0c9f28c6b9237d8fcab4461cb7ddd12afe354f0d68",
        architectPubkey:
          "05d1901b020205f04db1ee6c434d4cd69211af3dbc1d2a46e0e1e93f81f6dd20",
        relayUrl: "https://buzz.rowvia.ai:8443",
      },
      arguments: {},
    }),
  );
}

function setNativeScope(payload) {
  identity = payload.expected.ownerPubkey;
  relay = "wss://buzz.rowvia.ai:8443";
  relayClient.relayUrl = relay;
}

function dispatchNativeRequest(payload) {
  const listeners = [...eventListeners.entries()].filter(
    ([, listener]) => listener.event === ownerRequestEvent,
  );
  assert.equal(listeners.length, 1);
  const [id, listener] = listeners[0];
  return eventCallbacks.get(listener.handler)({
    event: ownerRequestEvent,
    id,
    payload,
  });
}

function replies() {
  return calls.filter((call) => call.command === "rowvia_owner_test_reply");
}

function expectedStatusReply(payload) {
  return {
    schema: "rowvia.buzz.owner-test/v1",
    requestId: payload.requestId,
    status: "ok",
    result: {
      ownerPubkey: payload.expected.ownerPubkey,
      relayUrl: "wss://buzz.rowvia.ai:8443",
      ready: true,
    },
  };
}

test("installation registers the stock Tauri listener and replies to a native status envelope", async (t) => {
  reset();
  const payload = nativeStatusRequest();
  setNativeScope(payload);
  const dispose = await installOwnerTestHook();
  t.after(dispose);
  const registration = calls[0];
  assert.equal(registration.command, "plugin:event|listen");
  assert.deepEqual(registration.args, {
    event: ownerRequestEvent,
    target: { kind: "Any" },
    handler: registration.args.handler,
  });
  assert.equal(typeof registration.args.handler, "number");
  await dispatchNativeRequest(payload);
  assert.deepEqual(calls.slice(1), [
    { command: "get_identity", args: {} },
    { command: "get_relay_ws_url", args: {} },
    {
      command: "rowvia_owner_test_reply",
      args: { response: expectedStatusReply(payload) },
    },
  ]);
});

test("installed runtime inspection binds the stock list wrapper and projects only the exact pair", async (t) => {
  reset();
  const pubkey = "c".repeat(64); // Cerberus/Postman need not be Architect.
  const payload = request("inspect_managed_agent_runtime", {
    pubkey,
    relayUrl: "http://localhost:3000/",
  });
  nativeOverrides.set("list_managed_agent_runtimes", () => [
    { pubkey: architectPubkey, relayUrl, lifecycle: "failed", pid: null },
    {
      pubkey,
      relayUrl: "ws://localhost:9999",
      lifecycle: "stopped",
      pid: null,
    },
    {
      pubkey,
      relayUrl,
      lifecycle: "ready",
      pid: 123,
      localSetup: true,
      error: "private error with token",
      logPath: "/private/log",
      config: { privateKey: "secret" },
      requestedRelayUrl: "private descriptor",
    },
  ]);
  const dispose = await installOwnerTestHook();
  t.after(dispose);
  await dispatchNativeRequest(payload);
  assert.deepEqual(replies()[0].args.response, {
    schema: OWNER_TEST_SCHEMA,
    requestId: payload.requestId,
    status: "ok",
    result: { pubkey, relayUrl, lifecycle: "ready", pid: 123 },
  });
  assert.deepEqual(
    calls.filter((call) => call.command === "list_managed_agent_runtimes"),
    [{ command: "list_managed_agent_runtimes", args: {} }],
  );
  assert.equal(
    calls.some((call) => /^(start|stop|restart|reconcile)_/.test(call.command)),
    false,
  );
});

test("runtime inspection strictly rejects invalid keys, relays and extra arguments", async (t) => {
  reset();
  const handler = createOwnerTestHandler();
  t.after(handler.dispose);
  const args = { pubkey: architectPubkey, relayUrl };
  const invalid = [
    ...[
      "",
      "a".repeat(63),
      "a".repeat(65),
      "A".repeat(64),
      "g".repeat(64),
      null,
    ].map((pubkey) => ({ ...args, pubkey })),
    ...[
      "",
      "localhost:3000",
      "ftp://localhost",
      "ws://",
      `ws://localhost/${"x".repeat(2048)}`,
      null,
    ].map((relayUrl) => ({ ...args, relayUrl })),
    { relayUrl },
    { pubkey: architectPubkey },
    { ...args, privateKey: "secret" },
    { ...args, name: "Cerberus" },
    { ...args, limit: 1 },
  ];
  for (const value of invalid)
    assert.equal(
      (await handler(request("inspect_managed_agent_runtime", value))).code,
      "invalid_request",
    );
  assert.equal(calls.length, 0);
});

test("runtime inspection preserves owner, relay, unlock and connection gates before listing", async (t) => {
  for (const failure of [
    "owner",
    "relay",
    "target",
    "locked",
    "lost",
    "resetFailed",
    "disconnected",
  ]) {
    reset();
    const handler = createOwnerTestHandler();
    t.after(handler.dispose);
    let targetRelay = relayUrl;
    if (failure === "owner") identity = "f".repeat(64);
    if (failure === "relay") relay = "ws://localhost:9999";
    if (failure === "target") targetRelay = "ws://localhost:9999";
    if (["locked", "lost", "resetFailed"].includes(failure))
      nativeOverrides.set("get_identity", () => ({
        pubkey: ownerPubkey,
        [failure === "resetFailed" ? "reset_failed" : failure]: true,
      }));
    if (failure === "disconnected")
      relayClient.connectionStateEmitter.set("disconnected");
    const result = await handler(
      request("inspect_managed_agent_runtime", {
        pubkey: architectPubkey,
        relayUrl: targetRelay,
      }),
    );
    assert.equal(result.status, "error");
    assert.equal(
      result.code,
      ["owner", "relay", "target"].includes(failure)
        ? "scope_mismatch"
        : "not_ready",
    );
    assert.equal(
      calls.some((call) => call.command === "list_managed_agent_runtimes"),
      false,
    );
  }
});

test("runtime inspection returns not_tracked for missing exact pairs, never stopped proof", async (t) => {
  reset();
  const handler = createOwnerTestHandler();
  t.after(handler.dispose);
  nativeOverrides.set("list_managed_agent_runtimes", () => [
    {
      pubkey: architectPubkey,
      relayUrl: `${relayUrl}/`,
      lifecycle: "stopped",
      pid: null,
    },
    { pubkey: "c".repeat(64), relayUrl, lifecycle: "ready", pid: 12 },
  ]);
  const payload = request("inspect_managed_agent_runtime", {
    pubkey: architectPubkey,
    relayUrl,
  });
  const result = await handler(payload);
  assert.deepEqual(result.result, {
    pubkey: architectPubkey,
    relayUrl,
    lifecycle: "not_tracked",
    pid: null,
  });
  assert.equal(result.status, "ok");
  assert.equal((await handler(payload)).code, "duplicate_request");
  assert.equal(
    calls.filter((call) => call.command === "list_managed_agent_runtimes")
      .length,
    1,
  );
});

test("runtime inspection ambiguity, malformed response, scope drift and invoke failure remain Unknown without retry", async (t) => {
  for (const failure of [
    "duplicate",
    "lifecycle",
    "pid",
    "response",
    "throw",
    "owner",
    "relay",
    "disconnect",
  ]) {
    reset();
    const handler = createOwnerTestHandler();
    t.after(handler.dispose);
    nativeOverrides.set("list_managed_agent_runtimes", () => {
      const runtime = {
        pubkey: architectPubkey,
        relayUrl,
        lifecycle: "listening",
        pid: 123,
      };
      if (failure === "duplicate") return [runtime, runtime];
      if (failure === "lifecycle") runtime.lifecycle = "secret token";
      if (failure === "pid") runtime.pid = "secret pid";
      if (failure === "response") return { secret: "bad response" };
      if (failure === "throw") throw new Error("private native failure");
      if (failure === "owner") identity = "f".repeat(64);
      if (failure === "relay") relay = "ws://localhost:9999";
      if (failure === "disconnect")
        relayClient.connectionStateEmitter.set("disconnected");
      return [runtime];
    });
    const payload = request("inspect_managed_agent_runtime", {
      pubkey: architectPubkey,
      relayUrl,
    });
    assert.deepEqual(await handler(payload), {
      schema: OWNER_TEST_SCHEMA,
      requestId: payload.requestId,
      status: "unknown",
      code: ["owner", "relay", "disconnect"].includes(failure)
        ? "scope_mismatch"
        : "operation_failed",
    });
    const before = calls.length;
    assert.equal((await handler(payload)).code, "duplicate_request");
    assert.equal(calls.length, before);
    assert.equal(
      calls.filter((call) => call.command === "list_managed_agent_runtimes")
        .length,
      1,
    );
  }
});

test("installed runtime ensure calls the stock start wrapper once for each pilot and projects observed status", async () => {
  assert.deepEqual(OWNER_TEST_ENSURE_TARGET_PUBKEYS, [
    "9d3972d809d53ce8a87003ca7f631009963d9414739188cea6209ced88365c87",
    "fa17b69969fa097c40b105bfd85b383101b2e918827ce0f67958f7c7197da2dc",
  ]);
  for (const pubkey of OWNER_TEST_ENSURE_TARGET_PUBKEYS) {
    for (const lifecycle of ["starting", "listening", "ready", "failed"]) {
      reset();
      const payload = nativeStatusRequest();
      payload.operation = "ensure_managed_agent_runtime";
      payload.arguments = { pubkey, relayUrl: "https://buzz.rowvia.ai:8443/" };
      setNativeScope(payload);
      // The same returned PID/status may describe an already-running pair.
      const observed = {
        pubkey,
        relayUrl: relay,
        lifecycle,
        pid: lifecycle === "failed" ? null : 123,
      };
      nativeOverrides.set("start_managed_agent_runtime", () => ({
        ...observed,
        error: "private failure",
        logPath: "/private/log",
        config: { privateKey: "secret", model: "private" },
      }));
      const dispose = await installOwnerTestHook();
      try {
        await dispatchNativeRequest(payload);
        assert.deepEqual(replies()[0].args.response, {
          schema: OWNER_TEST_SCHEMA,
          requestId: payload.requestId,
          status: "ok",
          result: observed,
        });
        const beforeDuplicate = calls.length;
        await dispatchNativeRequest(payload);
        assert.equal(replies()[1].args.response.code, "duplicate_request");
        assert.equal(calls.length, beforeDuplicate + 1); // Only the duplicate reply.
        assert.deepEqual(
          calls.filter((call) => /managed_agent_runtime/.test(call.command)),
          [
            {
              command: "start_managed_agent_runtime",
              args: { pubkey, relayUrl: relay },
            },
          ],
        );
        assert.equal(
          calls.some((call) => call.command === "publish"),
          false,
        );
      } finally {
        dispose();
      }
    }
  }
});

test("runtime ensure rejects non-pilot keys, malformed relays, missing fields and setup arguments before IPC", async (t) => {
  reset();
  const handler = createOwnerTestHandler();
  t.after(handler.dispose);
  const args = { pubkey: OWNER_TEST_ENSURE_TARGET_PUBKEYS[0], relayUrl };
  for (const value of [
    ...[
      ownerPubkey,
      architectPubkey,
      "c".repeat(64),
      "",
      "A".repeat(64),
      null,
    ].map((pubkey) => ({ ...args, pubkey })),
    ...[
      "",
      "localhost:3000",
      "ftp://localhost",
      "ws://",
      null,
      `ws://localhost/${"x".repeat(2048)}`,
    ].map((relayUrl) => ({ ...args, relayUrl })),
    {},
    { pubkey: args.pubkey },
    { relayUrl },
    ...[
      "privateKey",
      "definition",
      "model",
      "path",
      "provider",
      "autoRestart",
    ].map((field) => ({ ...args, [field]: "secret" })),
  ]) {
    const result = await handler(
      request("ensure_managed_agent_runtime", value),
    );
    assert.equal(result.status, "error");
    assert.equal(result.code, "invalid_request");
  }
  assert.equal(calls.length, 0);
});

test("runtime ensure preserves owner, exact relay, unlock and connected gates before start", async (t) => {
  for (const failure of [
    "owner",
    "relay",
    "target",
    "locked",
    "lost",
    "resetFailed",
    "disconnected",
  ]) {
    reset();
    const handler = createOwnerTestHandler();
    t.after(handler.dispose);
    if (failure === "owner") identity = "f".repeat(64);
    if (failure === "relay") relay = "ws://localhost:9999";
    if (["locked", "lost", "resetFailed"].includes(failure))
      nativeOverrides.set("get_identity", () => ({
        pubkey: ownerPubkey,
        [failure === "resetFailed" ? "reset_failed" : failure]: true,
      }));
    if (failure === "disconnected")
      relayClient.connectionStateEmitter.set("disconnected");
    const result = await handler(
      request("ensure_managed_agent_runtime", {
        pubkey: OWNER_TEST_ENSURE_TARGET_PUBKEYS[0],
        relayUrl: failure === "target" ? "ws://localhost:9999" : relayUrl,
      }),
    );
    assert.equal(result.status, "error");
    assert.equal(
      result.code,
      ["owner", "relay", "target"].includes(failure)
        ? "scope_mismatch"
        : "not_ready",
    );
    assert.equal(
      calls.some((call) => /managed_agent_runtime/.test(call.command)),
      false,
    );
  }
});

test("runtime ensure failure, wrong pair, invalid projection and post-start scope drift stay Unknown without retry", async (t) => {
  for (const failure of [
    "throw",
    "response",
    "pubkey",
    "resultRelay",
    "lifecycle",
    "pid",
    "owner",
    "relay",
    "disconnect",
    "locked",
  ]) {
    reset();
    const handler = createOwnerTestHandler();
    t.after(handler.dispose);
    const args = { pubkey: OWNER_TEST_ENSURE_TARGET_PUBKEYS[0], relayUrl };
    nativeOverrides.set("start_managed_agent_runtime", () => {
      const runtime = { ...args, lifecycle: "listening", pid: 123 };
      if (failure === "throw") throw new Error("private start failure");
      if (failure === "response") return null;
      if (failure === "pubkey")
        runtime.pubkey = OWNER_TEST_ENSURE_TARGET_PUBKEYS[1];
      if (failure === "resultRelay") runtime.relayUrl = `${relayUrl}/`;
      if (failure === "lifecycle") runtime.lifecycle = "secret value";
      if (failure === "pid") runtime.pid = -1;
      if (failure === "owner") identity = "f".repeat(64);
      if (failure === "relay") relay = "ws://localhost:9999";
      if (failure === "disconnect")
        relayClient.connectionStateEmitter.set("disconnected");
      if (failure === "locked")
        nativeOverrides.set("get_identity", () => ({
          pubkey: ownerPubkey,
          locked: true,
        }));
      return runtime;
    });
    const payload = request("ensure_managed_agent_runtime", args);
    assert.deepEqual(await handler(payload), {
      schema: OWNER_TEST_SCHEMA,
      requestId: payload.requestId,
      status: "unknown",
      code:
        failure === "locked"
          ? "not_ready"
          : ["owner", "relay", "disconnect"].includes(failure)
            ? "scope_mismatch"
            : "operation_failed",
    });
    const before = calls.length;
    assert.equal((await handler(payload)).code, "duplicate_request");
    assert.equal(calls.length, before);
    assert.deepEqual(
      calls.filter((call) => /managed_agent_runtime/.test(call.command)),
      [{ command: "start_managed_agent_runtime", args }],
    );
  }
});

test("stalled runtime ensure sends no success and does not start a duplicate request", async (t) => {
  reset();
  const handler = createOwnerTestHandler();
  t.after(handler.dispose);
  const args = { pubkey: OWNER_TEST_ENSURE_TARGET_PUBKEYS[0], relayUrl };
  const started = Promise.withResolvers();
  const completion = Promise.withResolvers();
  nativeOverrides.set("start_managed_agent_runtime", () => {
    started.resolve();
    return completion.promise;
  });
  const payload = request("ensure_managed_agent_runtime", args);
  let settled = false;
  const pending = handler(payload).finally(() => {
    settled = true;
  });
  await started.promise;
  const duplicate = handler(payload);
  try {
    await new Promise(setImmediate);
    assert.equal(settled, false);
    assert.equal(
      calls.filter((call) => call.command === "start_managed_agent_runtime")
        .length,
      1,
    );
  } finally {
    completion.reject(new Error("timeout after possible start"));
  }
  assert.equal((await pending).status, "unknown");
  assert.equal((await duplicate).code, "duplicate_request");
  assert.equal(
    calls.filter((call) => /managed_agent_runtime/.test(call.command)).length,
    1,
  );
});

test("installation propagates listener rejection and releases its relay subscription", async (t) => {
  reset();
  const failure = new Error("listener registration denied");
  nativeOverrides.set("plugin:event|listen", () => {
    throw failure;
  });
  const subscriptions = [];
  const subscribe = relayClient.subscribeToConnectionState.bind(relayClient);
  t.mock.method(relayClient, "subscribeToConnectionState", (listener) => {
    const dispose = t.mock.fn(subscribe(listener));
    subscriptions.push(dispose);
    return dispose;
  });
  t.after(() => {
    for (const dispose of subscriptions) dispose();
  });
  await assert.rejects(installOwnerTestHook(), (error) => error === failure);
  assert.equal(eventListeners.size, 0);
  assert.equal(replies().length, 0);
  assert.equal(subscriptions.length, 1);
  assert.equal(subscriptions[0].mock.callCount(), 1);
});

test("installed handler maps identity and relay IPC failures to correlated unknown replies", async () => {
  for (const command of ["get_identity", "get_relay_ws_url"]) {
    reset();
    const payload = nativeStatusRequest();
    setNativeScope(payload);
    nativeOverrides.set(command, () => {
      throw new Error(`offline ${command} failure`);
    });
    const dispose = await installOwnerTestHook();
    try {
      await dispatchNativeRequest(payload);
      assert.deepEqual(replies(), [
        {
          command: "rowvia_owner_test_reply",
          args: {
            response: {
              schema: payload.schema,
              requestId: payload.requestId,
              status: "unknown",
              code: "operation_failed",
            },
          },
        },
      ]);
      assert.equal(calls.filter((call) => call.command === command).length, 1);
    } finally {
      dispose();
    }
  }
});

test("installed status emits no reply while identity or relay IPC is stalled", async () => {
  for (const command of ["get_identity", "get_relay_ws_url"]) {
    reset();
    const payload = nativeStatusRequest();
    setNativeScope(payload);
    const started = Promise.withResolvers();
    const completion = Promise.withResolvers();
    nativeOverrides.set(command, () => {
      started.resolve();
      return completion.promise;
    });
    const dispose = await installOwnerTestHook();
    let settled = false;
    const pending = dispatchNativeRequest(payload).finally(() => {
      settled = true;
    });
    try {
      await started.promise;
      await new Promise(setImmediate);
      assert.equal(settled, false);
      assert.equal(replies().length, 0);
      assert.deepEqual(
        calls.slice(1).map((call) => call.command),
        command === "get_identity"
          ? ["get_identity"]
          : ["get_identity", "get_relay_ws_url"],
      );
    } finally {
      completion.resolve(
        command === "get_identity"
          ? { pubkey: identity, display_name: "Owner" }
          : relay,
      );
      await pending;
      dispose();
    }
    assert.deepEqual(replies()[0].args, {
      response: expectedStatusReply(payload),
    });
  }
});

test("installed reply rejection propagates and fences further requests without retry", async () => {
  reset();
  const payload = nativeStatusRequest();
  setNativeScope(payload);
  nativeOverrides.set("rowvia_owner_test_reply", () => {
    throw new Error("offline reply denied");
  });
  const dispose = await installOwnerTestHook();
  try {
    await assert.rejects(
      dispatchNativeRequest(payload),
      /offline reply denied/,
    );
    assert.deepEqual(replies()[0].args, {
      response: expectedStatusReply(payload),
    });
    assert.equal(replies().length, 1);
    nativeOverrides.delete("rowvia_owner_test_reply");
    const next = nativeStatusRequest();
    await dispatchNativeRequest(next);
    assert.deepEqual(replies()[1].args, {
      response: {
        schema: next.schema,
        requestId: next.requestId,
        status: "error",
        code: "scope_mismatch",
      },
    });
    assert.equal(
      calls.filter((call) => call.command === "get_identity").length,
      1,
    );
  } finally {
    dispose();
  }
});

test("installed hook disposal removes the Tauri listener and relay subscription", async (t) => {
  reset();
  const subscribe = relayClient.subscribeToConnectionState.bind(relayClient);
  const subscriptions = [];
  t.mock.method(relayClient, "subscribeToConnectionState", (listener) => {
    const dispose = t.mock.fn(subscribe(listener));
    subscriptions.push(dispose);
    return dispose;
  });
  const dispose = await installOwnerTestHook();
  t.after(() => {
    for (const unsubscribe of subscriptions) unsubscribe();
  });
  const [eventId] = eventListeners.keys();
  dispose();
  await new Promise(setImmediate);
  assert.equal(eventListeners.size, 0);
  assert.equal(eventCallbacks.size, 0);
  assert.equal(subscriptions[0].mock.callCount(), 1);
  assert.deepEqual(calls.at(-1), {
    command: "plugin:event|unlisten",
    args: { event: ownerRequestEvent, eventId },
  });
});

test("enabled installation markers follow the actual status IPC path", async () => {
  reset();
  const payload = nativeStatusRequest();
  setNativeScope(payload);
  const dispose = await installOwnerTestHook(
    createOwnerTestPhaseReporter(true),
  );
  try {
    await dispatchNativeRequest(payload);
    assert.deepEqual(
      calls.map((call) =>
        call.command === "rowvia_owner_test_phase"
          ? call.args.phase
          : call.command,
      ),
      [
        "registration_started",
        "plugin:event|listen",
        "registration_ready",
        "request_received",
        "identity_started",
        "get_identity",
        "identity_ready",
        "relay_started",
        "get_relay_ws_url",
        "relay_ready",
        "reply_started",
        "rowvia_owner_test_reply",
        "reply_accepted",
      ],
    );
    assert.deepEqual(replies()[0].args, {
      response: expectedStatusReply(payload),
    });
    for (const call of calls.filter(
      (call) => call.command === "rowvia_owner_test_phase",
    ))
      assert.deepEqual(Object.keys(call.args), ["phase"]);
  } finally {
    dispose();
  }
});

test("phase reporting cannot replace registration, identity, relay or reply failures", async () => {
  for (const command of [
    "plugin:event|listen",
    "get_identity",
    "get_relay_ws_url",
    "rowvia_owner_test_reply",
  ]) {
    reset();
    const payload = nativeStatusRequest();
    setNativeScope(payload);
    const failure = new Error("original business failure");
    nativeOverrides.set(command, () => {
      throw failure;
    });
    nativeOverrides.set("rowvia_owner_test_phase", () => {
      throw new Error("private diagnostic failure");
    });
    const report = createOwnerTestPhaseReporter(true);
    if (command === "plugin:event|listen") {
      await assert.rejects(
        installOwnerTestHook(report),
        (error) => error === failure,
      );
    } else {
      const dispose = await installOwnerTestHook(report);
      try {
        if (command === "rowvia_owner_test_reply")
          await assert.rejects(
            dispatchNativeRequest(payload),
            /original business failure/,
          );
        else {
          await dispatchNativeRequest(payload);
          assert.deepEqual(replies()[0].args.response, {
            schema: payload.schema,
            requestId: payload.requestId,
            status: "unknown",
            code: "operation_failed",
          });
        }
      } finally {
        dispose();
      }
    }
    const phases = calls
      .filter((call) => call.command === "rowvia_owner_test_phase")
      .map((call) => call.args.phase);
    const expectedFailure = {
      "plugin:event|listen": "registration_failed",
      get_identity: "identity_error",
      get_relay_ws_url: "relay_error",
      rowvia_owner_test_reply: "reply_failed",
    }[command];
    assert.ok(phases.includes(expectedFailure));
    assert.equal(calls.filter((call) => call.command === command).length, 1);
  }
});
async function create(handler) {
  const result = await handler(
    request("create_private_stream", { name: "rowvia-e2e-flow" }),
  );
  assert.equal(result.status, "ok");
  return result.result.channelId;
}

function nativeSendArgs(id, content, parentEventId = null, rootEventId = null) {
  return {
    channelId: id,
    content,
    parentEventId,
    rootEventId,
    mediaTags: null,
    emojiTags: null,
    mentionTags: null,
    linkPreviewTags: undefined,
    sentFromThreadTag: null,
    mentionPubkeys: [architectPubkey],
    kind: null,
    expectedRelayUrl: relayUrl,
    expectedSignerPubkey: ownerPubkey,
  };
}

async function prepare(handler) {
  const id = await create(handler);
  assert.equal(
    (await handler(request("add_architect", { channelId: id }))).status,
    "ok",
  );
  return id;
}

async function send(handler, id, references = {}) {
  return handler(
    request("send_message", {
      channelId: id,
      content: "ROWVIA_E2E_ thread",
      ...references,
    }),
  );
}

test("native root, direct and nested sends preserve exact parent/root and closed tag inputs", async (t) => {
  reset();
  const handler = createOwnerTestHandler();
  t.after(handler.dispose);
  const id = await prepare(handler);
  const root = await send(handler, id);
  assert.equal(root.status, "ok");
  const rootId = root.result.eventId;
  const direct = await send(handler, id, { parentEventId: rootId });
  assert.equal(direct.status, "ok");
  const directId = direct.result.eventId;
  const nested = await send(handler, id, {
    parentEventId: directId,
    rootEventId: rootId,
  });
  assert.equal(nested.status, "ok");
  const deeper = await send(handler, id, {
    parentEventId: nested.result.eventId,
  });
  assert.equal(deeper.status, "ok");
  assert.deepEqual(
    calls
      .filter((call) => call.command === "send_channel_message")
      .map((call) => call.args),
    [
      nativeSendArgs(id, "ROWVIA_E2E_ thread"),
      nativeSendArgs(id, "ROWVIA_E2E_ thread", rootId, rootId),
      nativeSendArgs(id, "ROWVIA_E2E_ thread", directId, rootId),
      nativeSendArgs(id, "ROWVIA_E2E_ thread", nested.result.eventId, rootId),
    ],
  );
  assert.equal(
    calls.some((call) =>
      ["sign_event", "publish", "history"].includes(call.command),
    ),
    false,
  );
});

test("malformed thread IDs, root without parent and unrestricted arguments fail schema before native calls", async (t) => {
  reset();
  const handler = createOwnerTestHandler();
  t.after(handler.dispose);
  const malformed = [
    "a".repeat(63),
    "a".repeat(65),
    "A".repeat(64),
    "g".repeat(64),
    "",
    null,
  ];
  for (const key of ["parentEventId", "rootEventId"]) {
    for (const value of malformed) {
      assert.equal(
        (
          await send(handler, channelId, {
            parentEventId: "c".repeat(64),
            [key]: value,
          })
        ).code,
        "invalid_request",
      );
    }
  }
  for (const extra of [
    { rootEventId: "c".repeat(64) },
    { tags: [["e", "c".repeat(64), "", "reply"]] },
    { mentionPubkeys: [ownerPubkey] },
    { kind: 40002 },
    { expectedSignerPubkey: ownerPubkey },
    { privateKey: "c".repeat(64) },
  ])
    assert.equal(
      (await send(handler, channelId, extra)).code,
      "invalid_request",
    );
  assert.equal(calls.length, 0);
});

test("unknown, cross-channel, cross-handler and inconsistent thread roots fail before native send", async (t) => {
  reset();
  const handler = createOwnerTestHandler();
  t.after(handler.dispose);
  const id = await prepare(handler);
  const rootId = (await send(handler, id)).result.eventId;
  const otherRootId = (await send(handler, id)).result.eventId;
  const directId = (await send(handler, id, { parentEventId: rootId })).result
    .eventId;
  createdChannelId = randomUUID();
  const otherChannel = await prepare(handler);
  events = [{ id: "f".repeat(64), kind: 9, tags: [["h", id]] }];
  assert.equal(
    (await handler(request("read_channel", { channelId: id, limit: 1 })))
      .status,
    "ok",
  );
  const before = calls.filter(
    (call) => call.command === "send_channel_message",
  ).length;
  for (const [target, references] of [
    [id, { parentEventId: "f".repeat(64) }],
    [otherChannel, { parentEventId: rootId, rootEventId: rootId }],
    [id, { parentEventId: directId, rootEventId: otherRootId }],
    [id, { parentEventId: directId, rootEventId: directId }],
    [id, { parentEventId: rootId, rootEventId: "f".repeat(64) }],
  ])
    assert.equal(
      (await send(handler, target, references)).code,
      "invalid_request",
    );
  const separate = createOwnerTestHandler();
  t.after(separate.dispose);
  createdChannelId = id;
  await prepare(separate);
  assert.equal(
    (await send(separate, id, { parentEventId: rootId })).code,
    "invalid_request",
  );
  assert.equal(
    calls.filter((call) => call.command === "send_channel_message").length,
    before,
  );
});

test("native failures and invalid responses never establish targets or retry sends", async (t) => {
  for (const failure of ["throw", "id", "parent", "root"]) {
    reset();
    const handler = createOwnerTestHandler();
    t.after(handler.dispose);
    const id = await prepare(handler);
    onNativeSend = (result) => {
      if (failure === "throw") throw new Error("uncertain transport");
      if (failure === "id") result.event_id = "C".repeat(64);
      if (failure === "parent") result.parent_event_id = "f".repeat(64);
      if (failure === "root") result.root_event_id = "f".repeat(64);
    };
    const item = request("send_message", {
      channelId: id,
      content: "ROWVIA_E2E_ unknown",
    });
    const result = await handler(item);
    assert.equal(result.status, "unknown");
    assert.equal(result.code, "operation_failed");
    assert.equal((await handler(item)).code, "duplicate_request");
    onNativeSend = () => {};
    assert.equal(
      (await send(handler, id, { parentEventId: "1".padStart(64, "0") })).code,
      "invalid_request",
    );
    assert.equal(
      calls.filter((call) => call.command === "send_channel_message").length,
      1,
    );
  }
});

test("duplicate native IDs fail closed without replacing earlier channel provenance", async (t) => {
  reset();
  const handler = createOwnerTestHandler();
  t.after(handler.dispose);
  const id = await prepare(handler);
  const rootId = (await send(handler, id)).result.eventId;
  createdChannelId = randomUUID();
  const otherChannel = await prepare(handler);
  onNativeSend = (result) => {
    result.event_id = rootId;
  };
  const duplicate = await send(handler, otherChannel);
  assert.equal(duplicate.status, "unknown");
  assert.equal(duplicate.code, "operation_failed");
  onNativeSend = () => {};
  assert.equal(
    (await send(handler, otherChannel, { parentEventId: rootId })).code,
    "invalid_request",
  );
  assert.equal(
    (await send(handler, id, { parentEventId: rootId })).status,
    "ok",
  );
  assert.equal(
    calls.filter((call) => call.command === "send_channel_message").length,
    3,
  );
});

test("scope drift rejects thread sends before invoke and fences completion provenance", async (t) => {
  for (const phase of ["before", "during"]) {
    for (const field of ["owner", "relay"]) {
      reset();
      const handler = createOwnerTestHandler();
      t.after(handler.dispose);
      const id = await prepare(handler);
      const rootId = (await send(handler, id)).result.eventId;
      const drift = () => {
        if (field === "owner") identity = "f".repeat(64);
        else relay = "ws://localhost:9999";
      };
      if (phase === "before") drift();
      else onNativeSend = drift;
      const result = await send(handler, id, { parentEventId: rootId });
      assert.equal(result.status, phase === "before" ? "error" : "unknown");
      assert.equal(result.code, "scope_mismatch");
      assert.equal(
        calls.filter((call) => call.command === "send_channel_message").length,
        phase === "before" ? 1 : 2,
      );
      identity = ownerPubkey;
      relay = relayUrl;
      assert.equal(
        (await send(handler, id, { parentEventId: rootId })).code,
        "scope_mismatch",
      );
    }
  }
});

test("create/add/send/read binds stock native wrappers with pinned kind9 recipients", async (t) => {
  reset();
  const handler = createOwnerTestHandler();
  t.after(handler.dispose);
  assert.equal((await handler(request("status"))).result.ready, true);
  const id = await create(handler);
  assert.deepEqual(
    (await handler(request("add_architect", { channelId: id }))).result,
    { added: true },
  );
  const sent = await handler(
    request("send_message", { channelId: id, content: " ROWVIA_E2E_ hello " }),
  );
  assert.equal(sent.status, "ok");
  assert.deepEqual(sent.result, { eventId: "1".padStart(64, "0") });
  assert.deepEqual(
    calls.find((call) => call.command === "send_channel_message").args,
    nativeSendArgs(id, " ROWVIA_E2E_ hello "),
  );
  assert.equal(
    calls.some((call) => ["sign_event", "publish"].includes(call.command)),
    false,
  );
  const createArgs = calls.find(
    (call) => call.command === "create_channel",
  ).args;
  assert.equal(createArgs.expectedSignerPubkey, ownerPubkey);
  assert.equal(createArgs.expectedRelayUrl, relayUrl);
  const addArgs = calls.find(
    (call) => call.command === "add_channel_members",
  ).args;
  assert.deepEqual(addArgs.pubkeys, [architectPubkey]);
  events = [
    {
      kind: 9,
      content: "ROWVIA_E2E_ hello",
      tags: [
        ["h", id],
        ["p", architectPubkey],
      ],
      id: sent.result.eventId,
    },
  ];
  assert.deepEqual(
    (
      await handler(
        request("read_channel", { channelId: id, limit: 5, since: 1 }),
      )
    ).result.events,
    events,
  );
  assert.deepEqual(calls.find((call) => call.command === "history").args, {
    kinds: [9],
    "#h": [id],
    limit: 5,
    since: 1,
  });
});

test("invalid operation, id, extra keys, oversize and unmarked messages never reach mutation APIs", async (t) => {
  reset();
  const handler = createOwnerTestHandler();
  t.after(handler.dispose);
  const invalid = [
    request("eval"),
    request("status", {}, { requestId: "invalid" }),
    request("status", { extra: true }),
    request("create_private_stream", { name: "general" }),
    request("send_message", {
      channelId,
      content: `ROWVIA_E2E_${"x".repeat(4096)}`,
    }),
    request("send_message", { channelId, content: "unmarked" }),
    request("read_channel", { channelId, limit: 51 }),
  ];
  for (const item of invalid)
    assert.equal((await handler(item)).code, "invalid_request");
  assert.equal(calls.length, 0);
});

test("duplicate request executes only once; unrelated channel denied", async (t) => {
  reset();
  const handler = createOwnerTestHandler();
  t.after(handler.dispose);
  const item = request("create_private_stream", { name: "rowvia-e2e-dupe" });
  const replies = await Promise.all([handler(item), handler(item)]);
  assert.equal(replies[1].code, "duplicate_request");
  assert.equal(
    calls.filter((call) => call.command === "create_channel").length,
    1,
  );
  assert.equal(
    (
      await handler(
        request("read_channel", { channelId: randomUUID(), limit: 5 }),
      )
    ).code,
    "channel_not_owned",
  );
});

test("wrong owner or relay fences subsequent writes even after scope returns", async (t) => {
  for (const field of ["owner", "relay"]) {
    reset();
    const handler = createOwnerTestHandler();
    t.after(handler.dispose);
    await create(handler);
    if (field === "owner") identity = "f".repeat(64);
    else relay = "ws://localhost:9999";
    assert.equal((await handler(request("status"))).code, "scope_mismatch");
    identity = ownerPubkey;
    relay = relayUrl;
    assert.equal(
      (await handler(request("add_architect", { channelId }))).code,
      "scope_mismatch",
    );
    assert.equal(
      calls.some((call) => call.command === "add_channel_members"),
      false,
    );
  }
});

test("status can retry initial scope mismatch before the app finishes initializing", async (t) => {
  reset();
  const handler = createOwnerTestHandler();
  t.after(handler.dispose);
  relay = "ws://localhost:9999";
  assert.equal((await handler(request("status"))).code, "scope_mismatch");
  relay = relayUrl;
  assert.equal((await handler(request("status"))).status, "ok");
});

test("canonical HTTPS scope and trailing slash match the same WSS relay", async (t) => {
  reset();
  const handler = createOwnerTestHandler();
  t.after(handler.dispose);
  relay = "wss://buzz.rowvia.ai:8443";
  relayClient.relayUrl = relay;
  const expected = {
    ownerPubkey,
    architectPubkey,
    relayUrl: "https://buzz.rowvia.ai:8443/",
  };
  assert.equal(
    (await handler(request("status", {}, { expected }))).status,
    "ok",
  );
  const created = await handler(
    request(
      "create_private_stream",
      { name: "rowvia-e2e-https" },
      { expected },
    ),
  );
  assert.equal(created.status, "ok");
  assert.equal(
    calls.find((call) => call.command === "create_channel").args
      .expectedRelayUrl,
    relay,
  );
  assert.equal(
    (await handler(request("add_architect", { channelId }, { expected })))
      .status,
    "ok",
  );
  assert.equal(
    (
      await handler(
        request(
          "send_message",
          { channelId, content: "ROWVIA_E2E_ https" },
          { expected },
        ),
      )
    ).status,
    "ok",
  );
});

test("membership errors and missing added confirmation prevent sending", async (t) => {
  for (const result of [
    { added: [], errors: [] },
    {
      added: [architectPubkey],
      errors: [{ pubkey: architectPubkey, error: "denied" }],
    },
  ]) {
    reset();
    const handler = createOwnerTestHandler();
    t.after(handler.dispose);
    await create(handler);
    membersResult = result;
    assert.equal(
      (await handler(request("add_architect", { channelId }))).code,
      "membership_failed",
    );
    assert.equal(
      (
        await handler(
          request("send_message", {
            channelId,
            content: "ROWVIA_E2E_ blocked",
          }),
        )
      ).code,
      "membership_failed",
    );
    assert.equal(
      calls.some((call) => call.command === "send_channel_message"),
      false,
    );
  }
});

test("oversized or cross-channel read responses are rejected", async (t) => {
  reset();
  const handler = createOwnerTestHandler();
  t.after(handler.dispose);
  await create(handler);
  events = [{ kind: 9, content: "x".repeat(17000), tags: [["h", channelId]] }];
  assert.equal(
    (await handler(request("read_channel", { channelId, limit: 1 }))).code,
    "response_too_large",
  );
  events = [{ kind: 9, content: "bad", tags: [["h", "other"]] }];
  assert.equal(
    (await handler(request("read_channel", { channelId, limit: 1 }))).code,
    "operation_failed",
  );
});

test("ordinary send scope fence blocks switches during connect or sign before publishing", async () => {
  for (const phase of ["connect", "sign"]) {
    reset();
    const switchSession = () => {
      relayClient.sessionEpoch++;
      relayClient.relayUrl = "ws://localhost:9999";
    };
    if (phase === "connect") onConnect = switchSession;
    else onSign = switchSession;
    await assert.rejects(
      relayClient.sendMessage(
        channelId,
        "ROWVIA_E2E_ fence",
        [architectPubkey],
        [],
        { ownerPubkey, relayUrl },
      ),
      /scope changed/,
    );
    assert.equal(
      calls.some((call) => call.command === "publish"),
      false,
    );
  }
});

test("disconnect invalidates lifetime channel access", async (t) => {
  reset();
  const handler = createOwnerTestHandler();
  t.after(handler.dispose);
  await create(handler);
  relayClient.connectionStateEmitter.set("idle");
  relayClient.connectionStateEmitter.set("connected");
  assert.equal(
    (await handler(request("read_channel", { channelId, limit: 1 }))).code,
    "scope_mismatch",
  );
});

test("request ledger never evicts identities and rejects when full", async (t) => {
  reset();
  const handler = createOwnerTestHandler();
  t.after(handler.dispose);
  const first = request("status");
  assert.equal((await handler(first)).status, "ok");
  for (let index = 1; index < 128; index++) {
    assert.equal((await handler(request("status"))).status, "ok");
  }
  assert.equal((await handler(request("status"))).code, "capacity_exceeded");
  assert.equal((await handler(first)).code, "duplicate_request");
});

test("architect scope drift and signer drift fail closed", async (t) => {
  reset();
  const handler = createOwnerTestHandler();
  t.after(handler.dispose);
  await create(handler);
  assert.equal(
    (
      await handler(
        request(
          "status",
          {},
          {
            expected: {
              ownerPubkey,
              relayUrl,
              architectPubkey: "e".repeat(64),
            },
          },
        ),
      )
    ).code,
    "scope_mismatch",
  );
  onSign = () => {
    identity = "e".repeat(64);
  };
  await assert.rejects(
    relayClient.sendMessage(
      channelId,
      "ROWVIA_E2E_ signer",
      [architectPubkey],
      [],
      { ownerPubkey, relayUrl },
    ),
    /signer scope changed/,
  );
  assert.equal(
    calls.some((call) => call.command === "publish"),
    false,
  );
});

test("scoped read uses the production history path and fences a community switch during its gate", async () => {
  reset();
  resetRateLimitGate();
  const client = new RelayClient();
  client.relayUrl = relayUrl;
  client.ensureConnected = async () => 1;
  const frames = [];
  client.sendRawForGeneration = async (frame) => {
    frames.push(frame);
    const subscription = client.subscriptions.get(frame[1]);
    clearTimeout(subscription.timeout);
    client.subscriptions.delete(frame[1]);
    subscription.resolve([]);
  };
  const filter = { kinds: [9], "#h": [channelId], limit: 1 };
  assert.deepEqual(
    await client.fetchEvents(filter, { ownerPubkey, relayUrl }),
    [],
  );
  assert.deepEqual(frames[0][2], filter);
  frames.length = 0;
  activateRateLimit(1);
  const pending = client.fetchEvents(filter, { ownerPubkey, relayUrl });
  await new Promise(setImmediate);
  client.sessionEpoch++;
  client.relayUrl = "ws://localhost:9999";
  resetRateLimitGate();
  await assert.rejects(pending, /History scope changed/);
  assert.equal(frames.length, 0);
  assert.equal(client.subscriptions.size, 0);
});

test("scoped ordinary publisher never sends EVENT after relay drift during rate gate", async () => {
  reset();
  resetRateLimitGate();
  const client = new RelayClient();
  client.relayUrl = relayUrl;
  client.ensureConnected = async () => 1;
  const frames = [];
  client.sendRawForGeneration = async (frame) => {
    frames.push(frame);
  };
  activateRateLimit(1);
  const pending = client.sendMessage(
    channelId,
    "ROWVIA_E2E_ gated",
    [architectPubkey],
    [],
    { ownerPubkey, relayUrl },
  );
  await new Promise(setImmediate);
  assert.equal(
    calls.some((call) => call.command === "sign_event"),
    true,
  );
  // Keep epoch unchanged: the optional carried scope guard, not only the
  // publisher's pre-existing ownership check, must catch this drift.
  client.relayUrl = "ws://localhost:9999";
  resetRateLimitGate();
  await assert.rejects(pending, /Message scope changed/);
  assert.equal(frames.length, 0);
  assert.equal(client.pendingEvents.size, 0);
});

test("scoped ordinary publisher refuses retry through a replacement relay", async () => {
  reset();
  resetRateLimitGate();
  const client = new RelayClient();
  client.relayUrl = relayUrl;
  let connects = 0;
  client.ensureConnected = async () => {
    if (connects++ > 0) {
      client.relayUrl = "ws://localhost:9999";
      client.connectionGeneration++;
    }
    return client.connectionGeneration;
  };
  client.recoverFromSocketFailure = (error) => error;
  const frames = [];
  client.sendRawForGeneration = async (frame) => {
    frames.push(frame);
    throw new Error("transport failed");
  };
  await assert.rejects(
    client.sendMessage(channelId, "ROWVIA_E2E_ retry", [architectPubkey], [], {
      ownerPubkey,
      relayUrl,
    }),
    /Message scope changed/,
  );
  assert.equal(frames.length, 1);
  assert.equal(client.pendingEvents.size, 0);
});
