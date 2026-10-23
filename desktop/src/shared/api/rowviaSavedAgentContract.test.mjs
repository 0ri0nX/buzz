import assert from "node:assert/strict";
import test from "node:test";
import {
  createSavedAgentOwnerApi,
  parseSavedAgentReview,
  parseSavedAgentAuthorization,
} from "./rowviaSavedAgentContract.ts";
import {
  id,
  review,
  authorization,
  canonical,
  digest,
  completion,
} from "./rowviaSavedAgent.test-fixture.mjs";
test("API import/construction is inert and review never authorizes or starts", async () => {
  const calls = [];
  const api = createSavedAgentOwnerApi(async (...args) => {
    calls.push(args);
    return review();
  });
  assert.deepEqual(calls, []);
  const result = await api.review(id);
  assert.deepEqual(calls, [
    ["rowvia_review_saved_agent_preparation", { input: { operationId: id } }],
  ]);
  assert.ok(Object.isFrozen(result));
  assert.equal(result.name, "Original Ω name");
  assert.equal(result.recipeId, "codex-development-memory-git-read-v1");
  assert.equal(result.runtimeReady, false);
});
test("explicit authorize sends only operation ID once, not hashes or paths", async () => {
  const calls = [];
  const api = createSavedAgentOwnerApi(async (...args) => {
    calls.push(args);
    return authorization();
  });
  assert.equal((await api.authorize(id)).state, "authorized");
  assert.deepEqual(calls, [
    ["rowvia_authorize_saved_agent", { input: { operationId: id } }],
  ]);
  assert.ok(Object.isFrozen(calls[0][1]) && Object.isFrozen(calls[0][1].input));
});
test("invalid operation refuses before any invoke", async () => {
  let calls = 0;
  const api = createSavedAgentOwnerApi(async () => {
    calls++;
    return authorization();
  });
  for (const value of [
    "bad",
    `${id}\n`,
    "00000000-0000-0000-0000-000000000000",
    id.replace("01940000", "0194A000"),
    null,
    {},
    [],
  ]) {
    await assert.rejects(
      () => api.review(value),
      /saved_agent_owner_unavailable/u,
    );
    await assert.rejects(
      () => api.authorize(value),
      /saved_agent_owner_unavailable/u,
    );
  }
  assert.equal(calls, 0);
});
test("review rejects unknown fields, accessors, prototype, surrogate and ready-state drift", async () => {
  let evaluated = false;
  const accessor = review();
  Object.defineProperty(accessor, "name", {
    enumerable: true,
    get() {
      evaluated = true;
      return "x";
    },
  });
  const variants = [
    { ...review(), extra: "PRIVATE_CANARY" },
    { ...review(), runtimeReady: true },
    { ...review(), bindingsReady: true },
    { ...review(), createdStopped: false },
    { ...review(), name: "\ud800" },
    { ...review(), systemPrompt: "x".repeat(4097) },
    { ...review(), canonical21Sha256: "0".repeat(64) },
    { ...review(), recipeId: "claude-local-v1" },
    { ...review(), relayUrl: "wss://user:secret@relay.example" },
    { ...review(), ownerPubkey: "a".repeat(64) },
    Object.assign(Object.create(null), review()),
    accessor,
    [],
    null,
  ];
  for (const value of variants) {
    const api = createSavedAgentOwnerApi(async () => value);
    await assert.rejects(
      () => api.review(id),
      (e) =>
        e.message === "saved_agent_owner_unavailable" &&
        !Object.hasOwn(e, "cause"),
    );
  }
  assert.equal(evaluated, false);
});
test("review response and invocation error diagnostics are fixed and never trigger approval", async () => {
  let count = 0;
  const api = createSavedAgentOwnerApi(async () => {
    count++;
    throw new Error("PRIVATE_CANARY");
  });
  await assert.rejects(
    () => api.review(id),
    (e) =>
      e.message === "saved_agent_owner_unavailable" &&
      !e.message.includes("CANARY"),
  );
  assert.equal(count, 1);
});
test("authorization malformed response is unknown with no automatic retry or readiness inference", async () => {
  for (const value of [
    { ...authorization(), operationId: "01940000-0000-7000-8000-000000000078" },
    { ...authorization(), state: "registered" },
    { ...authorization(), runtimeReady: true },
    { ...authorization(), expiresAt: "PRIVATE_CANARY" },
    [],
    { ...authorization(), expiresAt: "2030-02-30T00:00:00Z" },
  ]) {
    let count = 0;
    const api = createSavedAgentOwnerApi(async () => {
      count++;
      return value;
    });
    await assert.rejects(
      () => api.authorize(id),
      (e) => e.message === "saved_agent_authorization_unknown",
    );
    assert.equal(count, 1);
  }
});
test("authorization dispatch loss stays unknown; fixed native pre-dispatch refusal remains unavailable", async () => {
  for (const [raw, expected] of [
    ["PRIVATE_CANARY", "saved_agent_authorization_unknown"],
    ["saved_agent_owner_unavailable", "saved_agent_owner_unavailable"],
  ]) {
    let count = 0;
    const api = createSavedAgentOwnerApi(async () => {
      count++;
      throw new Error(raw);
    });
    await assert.rejects(
      () => api.authorize(id),
      (e) => e.message === expected && !Object.hasOwn(e, "cause"),
    );
    assert.equal(count, 1);
  }
});
test("known Core terminal states remain authorization states only", () => {
  for (const state of ["authorized", "revoked", "expired"]) {
    const result = parseSavedAgentAuthorization(
      { ...authorization(), state },
      id,
    );
    assert.equal(result.state, state);
    assert.ok(Object.isFrozen(result));
    assert.deepEqual(
      Object.keys(result).sort(),
      ["version", "operationId", "state", "expiresAt"].sort(),
    );
  }
});
test("different recipe and canonical digest remain distinct public review fields", () => {
  const result = parseSavedAgentReview(review(), id);
  assert.notEqual(result.recipeSha256, result.canonical21Sha256);
  assert.equal(result.systemPrompt, "Owner-reviewed prompt");
});

test("ES2022 scalar-pair validation rejects lone halves and preserves valid astral strings exactly", async () => {
  const valid = {
    ...review(),
    name: "Agent 🧭",
    systemPrompt: "Original 𐐀 prompt 🧭",
  };
  const checked = parseSavedAgentReview(valid, id);
  assert.equal(checked.name, valid.name);
  assert.equal(checked.systemPrompt, valid.systemPrompt);
  assert.equal(checked.canonical21Sha256, valid.canonical21Sha256);
  assert.deepEqual(
    Buffer.from(checked.systemPrompt),
    Buffer.from(valid.systemPrompt),
  );
  for (const malformed of [
    "\ud800",
    "\udfff",
    "prefix\ud800tail",
    "\ud800\ud800",
    "\udc00\ud800",
    "🧭\ud800",
    "🧭\udc00",
  ]) {
    for (const field of ["name", "systemPrompt"]) {
      const api = createSavedAgentOwnerApi(async () => ({
        ...review(),
        [field]: malformed,
      }));
      await assert.rejects(
        () => api.review(id),
        (e) => e.message === "saved_agent_owner_unavailable",
      );
    }
  }
});

test("review rejects altered, oversized, duplicate, reordered and wrong-cohort canonical21 before UI", async () => {
  for (const text of [
    canonical({ agent_pubkey: "9".repeat(64) }),
    canonical({ owner_pubkey: "9".repeat(64) }),
    canonical({ runtime_config_digest: "9".repeat(64) }),
    canonical({ operation_id: id.replace(/77$/u, "78") }),
    canonical({ credential_ttl_seconds: "86400" }),
    canonical({ extra: "secret" }),
    ` ${canonical()}`,
    canonical().replace(
      '"manifest_version":1',
      '"manifest_version":1,"manifest_version":1',
    ),
    canonical().replace('"manifest_version":1', '"manifest_version":1.0'),
    "x".repeat(4097),
  ]) {
    const api = createSavedAgentOwnerApi(async () => ({
      ...review(),
      canonical21: text,
      canonical21Sha256: digest(text),
    }));
    await assert.rejects(
      () => api.review(id),
      /saved_agent_owner_unavailable/u,
    );
  }
  const api = createSavedAgentOwnerApi(async () => ({
    ...review(),
    canonical21Sha256: "9".repeat(64),
  }));
  await assert.rejects(() => api.review(id), /saved_agent_owner_unavailable/u);
});

test("Save has exactly four owner fields and fixed recipe; approval sends operation only", async () => {
  const calls = [];
  const api = createSavedAgentOwnerApi(async (command, args) => {
    calls.push({ command, args });
    return command === "rowvia_save_codex_agent"
      ? { version: 1, operationId: id }
      : completion();
  });
  await api.save({
    displayName: "Codex",
    systemPrompt: "Prompt",
    channelId: review().channelId,
    recipe: "codex-development-memory-git-read-v1",
  });
  assert.deepEqual(calls[0], {
    command: "rowvia_save_codex_agent",
    args: {
      input: {
        displayName: "Codex",
        systemPrompt: "Prompt",
        channelId: review().channelId,
        recipe: "codex-development-memory-git-read-v1",
      },
    },
  });
  assert.equal((await api.approve(review())).kind, "registered-stopped");
  assert.deepEqual(calls[1], {
    command: "rowvia_approve_saved_agent_registration",
    args: { input: { operationId: id } },
  });
  for (const input of [
    {
      displayName: "x",
      systemPrompt: "p",
      channelId: review().channelId,
      recipe: "codex-local-v1",
    },
    {
      displayName: "x",
      systemPrompt: "p",
      channelId: review().channelId,
      recipe: "codex-development-memory-git-read-v1",
      command: "sh",
    },
  ]) {
    await assert.rejects(
      () => api.save(input),
      /saved_agent_owner_unavailable/u,
    );
  }
  assert.equal(calls.length, 2);
});

test("completion must match reviewed original bytes, identity and stopped state", async () => {
  for (const delta of [
    { agentPubkey: "9".repeat(64) },
    { canonical21: "{}" },
    { canonical21Sha256: "9".repeat(64) },
    { runtimeReady: true },
    { bindingsReady: false },
    { state: "authorized" },
    { extra: "PRIVATE_CANARY" },
  ]) {
    let calls = 0;
    const raw = completion();
    Object.assign(raw.completion, delta);
    const api = createSavedAgentOwnerApi(async () => {
      calls++;
      return raw;
    });
    await assert.rejects(
      () => api.approve(review()),
      /saved_agent_authorization_unknown/u,
    );
    assert.equal(calls, 1);
  }
  const api = createSavedAgentOwnerApi(async () => ({
    kind: "authorization",
    authorization: authorization(),
  }));
  assert.equal((await api.approve(review())).kind, "authorization");
});
