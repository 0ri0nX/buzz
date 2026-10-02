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
globalThis.window = {
  setTimeout,
  clearTimeout,
  __TAURI_INTERNALS__: {
    invoke: async (command, args) => {
      calls.push({ command, args });
      if (command === "get_identity")
        return { pubkey: identity, display_name: "Owner" };
      if (command === "get_relay_ws_url") return relay;
      if (command === "create_channel")
        return {
          id: channelId,
          name: args.name,
          channel_type: "stream",
          visibility: "private",
          member_count: 1,
        };
      if (command === "add_channel_members") return membersResult;
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
const { createOwnerTestHandler } = await import("./ownerTestHook.ts");
const { OWNER_TEST_SCHEMA } = await import("./ownerTestHookProtocol.ts");

// Mock the session/transport boundary; retain the production sendMessage,
// fetchEvents, mention normalization and native API wrappers.
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
async function create(handler) {
  const result = await handler(
    request("create_private_stream", { name: "rowvia-e2e-flow" }),
  );
  assert.equal(result.status, "ok");
  return result.result.channelId;
}

test("create/add/send/read binds production channel wrappers and ordinary kind9 h/p message path", async (t) => {
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
  const signed = calls.find((call) => call.command === "sign_event").args;
  assert.deepEqual(signed, {
    kind: 9,
    content: "ROWVIA_E2E_ hello",
    tags: [
      ["h", id],
      ["p", architectPubkey],
    ],
  });
  const createArgs = calls.find(
    (call) => call.command === "create_channel",
  ).args;
  assert.equal(createArgs.expectedSignerPubkey, ownerPubkey);
  assert.equal(createArgs.expectedRelayUrl, relayUrl);
  const addArgs = calls.find(
    (call) => call.command === "add_channel_members",
  ).args;
  assert.deepEqual(addArgs.pubkeys, [architectPubkey]);
  events = [{ ...signed, id: sent.result.eventId }];
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
      calls.some((call) => call.command === "sign_event"),
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
