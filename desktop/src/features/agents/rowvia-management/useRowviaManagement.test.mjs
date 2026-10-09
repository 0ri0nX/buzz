import assert from "node:assert/strict";
import { test } from "node:test";
import { refillRowviaRecoveryQueue } from "./useRowviaManagement.ts";

test("durable receipt queue refills after each finish so entries 21 through 32 are reachable", () => {
  const receipts = Array.from({ length: 32 }, (_, index) => ({
    receipt_handle: `receipt-${index + 1}`,
    proposal_id: `proposal-${index + 1}`,
    operation_id: `operation-${index + 1}`,
    proposal_digest: "a".repeat(64),
    proposal_bytes_b64: "e30=",
  }));
  const pending = { receipts, unresolved: [] };
  const dismissed = new Set();
  let items = [];
  const visited = [];
  for (let index = 0; index < receipts.length; index += 1) {
    const refill = refillRowviaRecoveryQueue(items, pending, [], dismissed);
    items = refill.items;
    assert.ok(items.length > 0);
    assert.ok(items.length <= 20);
    const current = items.shift();
    visited.push(current.receipt.receipt_handle);
    dismissed.add(current.receipt.receipt_handle);
  }
  assert.deepEqual(
    visited,
    receipts.map((receipt) => receipt.receipt_handle),
  );
  assert.equal(
    refillRowviaRecoveryQueue(items, pending, [], dismissed).items.length,
    0,
  );
});
