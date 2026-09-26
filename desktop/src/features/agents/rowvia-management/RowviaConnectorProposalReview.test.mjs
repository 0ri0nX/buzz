import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { after, afterEach, test } from "node:test";
import React from "react";
import { JSDOM } from "jsdom";
import { RowviaConnectorProposalReview } from "./RowviaConnectorProposalReview.tsx";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});
Object.assign(globalThis, {
  document: dom.window.document,
  HTMLElement: dom.window.HTMLElement,
  IS_REACT_ACT_ENVIRONMENT: true,
  window: dom.window,
});
const { fireEvent, render, screen, cleanup, waitFor } = await import(
  "@testing-library/react"
);
afterEach(cleanup);
after(() => dom.window.close());

const id = "018fae01-1111-7111-8111-111111111111";
const operationId = "018fae01-2222-7222-8222-222222222222";
const bindingId = "018fae01-3333-7333-8333-333333333333";
const pubkeyA = "a".repeat(64);
const pubkeyB = "b".repeat(64);
function fields(overrides = {}) {
  return {
    version: 1,
    tenant_id: "018fae01-4444-7444-8444-444444444444",
    proposal_id: id,
    operation_id: operationId,
    owner_subject_id: "018fae01-5555-7555-8555-555555555555",
    owner_pubkey: "c".repeat(64),
    action: "connector.grant",
    agent_name: "Cerberus",
    source_instance_id: "cerberus-1",
    target_pubkey: pubkeyA,
    agent_identity_id: "018fae01-6666-7666-8666-666666666666",
    workload_principal_id: "018fae01-7777-7777-8777-777777777777",
    binding_id: bindingId,
    provider_subject: "google-subject-1",
    constrained_resource: "gmail-account:owner@gmail.com",
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
}
function envelope(proposal = fields(), overrides = {}) {
  const bytes = Buffer.from(JSON.stringify(proposal), "utf8");
  return {
    proposal_id: proposal.proposal_id,
    operation_id: proposal.operation_id,
    proposal_bytes_b64: bytes.toString("base64"),
    proposal_digest: createHash("sha256").update(bytes).digest("hex"),
    ...overrides,
  };
}
function show(value = envelope(), status = "pending") {
  const approved = [];
  const rejected = [];
  const view = render(
    React.createElement(RowviaConnectorProposalReview, {
      proposal: value,
      status,
      onApprove: (item) => approved.push(item),
      onReject: (item) => rejected.push(item),
    }),
  );
  return { approved, rejected, ...view };
}
async function ready() {
  await waitFor(() =>
    assert.equal(
      screen.getByRole("button", { name: "Approve" }).disabled,
      false,
    ),
  );
}
async function invalid() {
  await waitFor(() => assert.ok(screen.getByText(/unavailable or invalid/)));
  assert.equal(screen.getByRole("button", { name: "Approve" }).disabled, true);
  assert.equal(screen.getByRole("button", { name: "Reject" }).disabled, true);
}

test("verified canonical fields render and approval adds only verified expiry to native request", async () => {
  const proposal = fields();
  const input = envelope(proposal);
  const { approved } = show(input);
  assert.equal(screen.getByRole("button", { name: "Approve" }).disabled, true);
  await ready();
  assert.ok(screen.getByText("Grant access"));
  assert.ok(screen.getByText(`Identity: ${pubkeyA}`));
  assert.ok(screen.getByText("owner@gmail.com"));
  assert.ok(screen.getByText(bindingId));
  assert.ok(screen.getByText("absent"));
  assert.ok(screen.getByText("0"));
  assert.ok(screen.getByText("Read and search Gmail messages"));
  const approve = screen.getByRole("button", { name: "Approve" });
  approve.focus();
  assert.equal(dom.window.document.activeElement, approve);
  fireEvent.click(approve);
  assert.deepEqual(approved, [{ ...input, expires_at: proposal.expires_at }]);
});

test("identical agent names retain different signed identities", async () => {
  show();
  await ready();
  cleanup();
  const second = envelope(fields({ target_pubkey: pubkeyB }));
  const { approved } = show(second);
  await ready();
  assert.ok(screen.getByText(`Identity: ${pubkeyB}`));
  fireEvent.click(screen.getByRole("button", { name: "Approve" }));
  assert.equal(approved[0].proposal_digest, second.proposal_digest);
});

test("Gmail account with s is accepted from verified resource bytes", async () => {
  show(
    envelope(fields({ constrained_resource: "gmail-account:susan@gmail.com" })),
  );
  await ready();
  assert.ok(screen.getByText("susan@gmail.com"));
});

for (const [caseName, input] of [
  ["digest mismatch", envelope(fields(), { proposal_digest: "f".repeat(64) })],
  ["proposal ID mismatch", envelope(fields(), { proposal_id: bindingId })],
  ["operation ID mismatch", envelope(fields(), { operation_id: bindingId })],
  ["missing resource", envelope(fields({ constrained_resource: undefined }))],
  [
    "secret field",
    envelope(
      fields({
        constrained_resource: "gmail-account:other@gmail.com",
        token: "SECRET_SIGNED",
      }),
    ),
  ],
  [
    "envelope authority",
    envelope(fields(), { account_email: "SPOOFED@gmail.com" }),
  ],
  [
    "unsupported operation",
    envelope(
      fields({ operations: ["gmail.message.read", "gmail.message.send"] }),
    ),
  ],
  ["invalid expiry", envelope(fields({ expires_at: "invalid" }))],
  [
    "missing policy state",
    envelope(fields({ access_policy_state: undefined })),
  ],
  ["missing revision", envelope(fields({ manual_target_revision: undefined }))],
  ["negative revision", envelope(fields({ manual_target_revision: -1 }))],
  [
    "resource whitespace",
    envelope(
      fields({ constrained_resource: "gmail-account:susan @gmail.com" }),
    ),
  ],
  [
    "resource trailing whitespace",
    envelope(
      fields({ constrained_resource: "gmail-account:susan@gmail.com " }),
    ),
  ],
]) {
  test(`invalid proposal ${caseName} fails closed`, async () => {
    const { approved, rejected } = show(input);
    await invalid();
    assert.doesNotMatch(
      dom.window.document.body.textContent,
      /SECRET_|SPOOFED/,
    );
    fireEvent.click(screen.getByRole("button", { name: "Approve" }));
    assert.equal(approved.length + rejected.length, 0);
  });
}

for (const [caseName, input, status] of [
  ["stale status", envelope(), "stale"],
  ["loading status", envelope(), "loading"],
  ["error status", envelope(), "error"],
  [
    "expired bytes",
    envelope(fields({ expires_at: new Date(Date.now() - 1).toISOString() })),
    "pending",
  ],
]) {
  test(`${caseName} cannot decide`, async () => {
    const { approved, rejected } = show(input, status);
    await waitFor(() => assert.ok(!screen.queryByText("Verifying proposal…")));
    assert.equal(
      screen.getByRole("button", { name: "Approve" }).disabled,
      true,
    );
    assert.equal(screen.getByRole("button", { name: "Reject" }).disabled, true);
    assert.equal(approved.length + rejected.length, 0);
  });
}

test("rejection returns exact verified envelope for revoke", async () => {
  const input = envelope(fields({ action: "connector.revoke" }));
  const { rejected } = show(input);
  await ready();
  assert.ok(screen.getByText("Revoke access"));
  fireEvent.click(screen.getByRole("button", { name: "Reject" }));
  assert.deepEqual(rejected, [input]);
});
