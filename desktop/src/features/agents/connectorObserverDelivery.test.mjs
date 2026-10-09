import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

import {
  _testHandleRelayObserverEvent,
  _testRegisterKnownAgents,
  resetAgentObserverStore,
  subscribeConnectorRequests,
} from "./observerRelayStore.ts";

const agentPubkey = "a".repeat(64);
const otherPubkey = "b".repeat(64);
const channelId = "7c07e659-3610-42f4-9a5e-1e9973c09da9";
const requestId = "550e8400-e29b-41d4-a716-446655440000";

function rawEvent(overrides = {}) {
  return {
    id: "c".repeat(64),
    pubkey: agentPubkey,
    created_at: 1,
    kind: 1,
    tags: [
      ["agent", agentPubkey],
      ["frame", "telemetry"],
    ],
    content: "encrypted",
    sig: "d".repeat(128),
    ...overrides,
  };
}

function observerEvent(overrides = {}) {
  return {
    seq: 1,
    timestamp: "2026-09-25T12:00:00Z",
    kind: "agent_management_request",
    agentIndex: null,
    channelId,
    sessionId: null,
    turnId: null,
    payload: {
      type: "agent_management_request",
      version: 1,
      action: "connector.grant",
      requestId,
      request: { channelId, targetName: "Mail helper" },
    },
    ...overrides,
  };
}

afterEach(resetAgentObserverStore);

test("live observer delivers parsed draft with the verified raw origin", async () => {
  _testRegisterKnownAgents("test", [agentPubkey]);
  const deliveries = [];
  subscribeConnectorRequests((draft, evidence) =>
    deliveries.push({ draft, evidence }),
  );
  const event = rawEvent();
  await _testHandleRelayObserverEvent(event, async () => observerEvent());
  assert.equal(deliveries.length, 1);
  assert.deepEqual(deliveries[0].draft, observerEvent().payload);
  assert.equal(deliveries[0].evidence.relayEvent, event);
  assert.equal(deliveries[0].evidence.agentPubkey, agentPubkey);
  assert.equal(deliveries[0].evidence.observerChannelId, channelId);
  // The native request journal owns deduplication, including after a restart.
  await _testHandleRelayObserverEvent(event, async () => observerEvent());
  assert.equal(deliveries.length, 2);
});

test("distinct signed proposals sharing the transcript key are both delivered", async () => {
  _testRegisterKnownAgents("test", [agentPubkey]);
  const deliveries = [];
  subscribeConnectorRequests((draft, evidence) =>
    deliveries.push({ draft, evidence }),
  );
  const first = rawEvent();
  const second = rawEvent({ id: "e".repeat(64) });
  await _testHandleRelayObserverEvent(first, async () => observerEvent());
  await _testHandleRelayObserverEvent(second, async () =>
    observerEvent({
      payload: {
        ...observerEvent().payload,
        requestId: "aa0e8400-e29b-41d4-a716-446655440000",
      },
    }),
  );
  assert.equal(deliveries.length, 2);
  assert.equal(deliveries[1].evidence.relayEvent, second);
});

test("forged sender, unknown agent and channel mismatch do not deliver", async () => {
  _testRegisterKnownAgents("test", [agentPubkey]);
  const deliveries = [];
  subscribeConnectorRequests((draft) => deliveries.push(draft));
  await _testHandleRelayObserverEvent(
    rawEvent({ pubkey: otherPubkey }),
    async () => observerEvent(),
  );
  await _testHandleRelayObserverEvent(
    rawEvent({
      tags: [
        ["agent", otherPubkey],
        ["frame", "telemetry"],
      ],
    }),
    async () => observerEvent(),
  );
  await _testHandleRelayObserverEvent(rawEvent(), async () =>
    observerEvent({ channelId: "another-channel" }),
  );
  assert.equal(deliveries.length, 0);
});
