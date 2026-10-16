import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import test from "node:test";
import { ownerTestRequestSchema } from "./ownerTestHookProtocol.ts";

function request(operation, args) {
  return {
    schema: "rowvia.buzz.owner-test/v1",
    requestId: randomUUID(),
    operation,
    arguments: args,
    expected: {
      ownerPubkey: "a".repeat(64),
      architectPubkey: "b".repeat(64),
      relayUrl: "wss://test.example",
    },
  };
}
for (const operation of [
  "inspect_saved_connector",
  "prepare_saved_connector",
]) {
  test(`${operation} has no caller selectors or bodies`, () => {
    assert.equal(
      ownerTestRequestSchema.safeParse(request(operation, {})).success,
      true,
    );
    for (const args of [
      { targetPubkey: "a".repeat(64) },
      { selectionHandle: randomUUID() },
      { action: "connector.revoke" },
      { relayEvent: {} },
      { eval: "anything" },
    ]) {
      assert.equal(
        ownerTestRequestSchema.safeParse(request(operation, args)).success,
        false,
      );
    }
  });
}
for (const operation of [
  "approve_saved_connector",
  "inspect_saved_connector_operation",
]) {
  test(`${operation} accepts only a lowercase exact canonical fingerprint`, () => {
    assert.equal(
      ownerTestRequestSchema.safeParse(
        request(operation, { fingerprint: "a".repeat(64) }),
      ).success,
      true,
    );
    for (const args of [
      {},
      { fingerprint: "A".repeat(64) },
      { fingerprint: "a".repeat(63) },
      { fingerprint: "a".repeat(64), receiptHandle: randomUUID() },
      { fingerprint: "a".repeat(64), operations: ["gmail.message.send"] },
    ]) {
      assert.equal(
        ownerTestRequestSchema.safeParse(request(operation, args)).success,
        false,
      );
    }
  });
}
