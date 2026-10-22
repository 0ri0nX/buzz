// Pure wire/API boundary. No signing, provider IO or automatic approval.
export type SavedAgentReview = Readonly<{
  version: 1;
  operationId: string;
  agentPubkey: string;
  ownerPubkey: string;
  relayUrl: string;
  sourceInstance: string;
  name: string;
  systemPrompt: string;
  channelId: string;
  recipeId: "codex-local-v1" | "codex-development-memory-git-read-v1";
  model: "gpt-6.1-sol";
  recipeSha256: string;
  canonical21Sha256: string;
  canonical21: string;
  createdStopped: true;
  bindingsReady: false;
  runtimeReady: false;
}>;
export type SavedAgentAuthorization = Readonly<{
  version: 1;
  operationId: string;
  state: "authorized" | "expired" | "revoked";
  expiresAt: string;
}>;
export type SavedAgentRegistrationCompletion = Readonly<{
  version: 1;
  operationId: string;
  agentPubkey: string;
  canonical21: string;
  canonical21Sha256: string;
  state: "registered-stopped";
  bindingsReady: true;
  runtimeReady: false;
}>;
export type SavedAgentApproval =
  | Readonly<{ kind: "authorization"; authorization: SavedAgentAuthorization }>
  | Readonly<{
      kind: "registered-stopped";
      completion: SavedAgentRegistrationCompletion;
    }>;
export type SavedAgentPending = Readonly<{
  version: 1;
  operationId: string;
  agentPubkey: string;
  name: string;
  status: "pending";
}>;
export type SavedCodexInput = Readonly<{
  displayName: string;
  systemPrompt: string;
  channelId: string;
  recipe: "codex-development-memory-git-read-v1";
}>;
export type OwnerInvoke = (
  command: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;
const uuid7 =
  /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u;
const uuid =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u;
const sha = /^(?!0{64}$)[0-9a-f]{64}$/u;
const unavailable = () => new Error("saved_agent_owner_unavailable");
const unknown = () => new Error("saved_agent_authorization_unknown");
function exact(
  value: unknown,
  fields: readonly string[],
): Record<string, unknown> {
  if (
    value === null ||
    typeof value !== "object" ||
    Object.getPrototypeOf(value) !== Object.prototype ||
    Reflect.ownKeys(value).length !== fields.length ||
    !fields.every((field) => {
      const item = Object.getOwnPropertyDescriptor(value, field);
      return item?.enumerable === true && Object.hasOwn(item, "value");
    })
  )
    throw unavailable();
  return value as Record<string, unknown>;
}
function hasOnlyScalarPairs(value: string): boolean {
  for (let index = 0; index < value.length; index++) {
    const unit = value.charCodeAt(index);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = value.charCodeAt(index + 1);
      if (!(next >= 0xdc00 && next <= 0xdfff)) return false;
      index++;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) {
      return false;
    }
  }
  return true;
}
function text(value: unknown, limit: number): value is string {
  return (
    typeof value === "string" &&
    value.length <= limit &&
    hasOnlyScalarPairs(value) &&
    !value.includes("\0")
  );
}
function hash(value: unknown): value is string {
  return typeof value === "string" && sha.test(value);
}
function validOperation(id: string): void {
  if (typeof id !== "string" || !uuid7.test(id)) throw unavailable();
}
export function parseSavedAgentReview(
  raw: unknown,
  id: string,
): SavedAgentReview {
  validOperation(id);
  const v = exact(raw, [
    "version",
    "operationId",
    "agentPubkey",
    "ownerPubkey",
    "relayUrl",
    "sourceInstance",
    "name",
    "systemPrompt",
    "channelId",
    "recipeId",
    "model",
    "recipeSha256",
    "canonical21Sha256",
    "createdStopped",
    "bindingsReady",
    "runtimeReady",
    "canonical21",
  ]);
  if (
    v.version !== 1 ||
    v.operationId !== id ||
    !hash(v.agentPubkey) ||
    !hash(v.ownerPubkey) ||
    v.agentPubkey === v.ownerPubkey ||
    !hash(v.recipeSha256) ||
    !hash(v.canonical21Sha256) ||
    !text(v.name, 128) ||
    v.name.length === 0 ||
    v.name.trim() !== v.name ||
    !text(v.systemPrompt, 4096) ||
    typeof v.channelId !== "string" ||
    !uuid.test(v.channelId) ||
    typeof v.sourceInstance !== "string" ||
    !/^[a-z0-9][a-z0-9._-]{0,63}$/u.test(v.sourceInstance) ||
    !text(v.relayUrl, 4096) ||
    typeof v.recipeId !== "string" ||
    !["codex-local-v1", "codex-development-memory-git-read-v1"].includes(
      v.recipeId,
    ) ||
    v.model !== "gpt-6.1-sol" ||
    v.createdStopped !== true ||
    v.bindingsReady !== false ||
    v.runtimeReady !== false
  )
    throw unavailable();
  try {
    const relay = new URL(v.relayUrl);
    if (
      !["ws:", "wss:"].includes(relay.protocol) ||
      relay.username ||
      relay.password ||
      relay.search ||
      relay.hash ||
      !relay.hostname
    )
      throw unavailable();
  } catch {
    throw unavailable();
  }
  const result = Object.freeze({ ...v }) as SavedAgentReview;
  canonicalRegistration(result.canonical21, result);
  return result;
}

export const canonical21Fields = [
  "agent_credential_id",
  "agent_name",
  "agent_pubkey",
  "agent_verifier_sha256",
  "credential_ttl_seconds",
  "default_space_id",
  "harness_kind",
  "lifecycle_actions",
  "manifest_version",
  "operation_id",
  "operator_profile",
  "owner_principal_id",
  "owner_pubkey",
  "owner_subject_id",
  "retry_key_sha256",
  "runtime_config_digest",
  "source_instance",
  "tenant_id",
  "version",
  "workload_credential_id",
  "workload_verifier_sha256",
] as const;

/** Read-only rendering data; the original text is the sole byte commitment. */
export function canonicalRegistration(
  raw: unknown,
  review: Pick<
    SavedAgentReview,
    | "operationId"
    | "agentPubkey"
    | "ownerPubkey"
    | "sourceInstance"
    | "recipeSha256"
  >,
): Readonly<Record<string, unknown>> {
  if (!text(raw, 4096) || new TextEncoder().encode(raw).length > 4096)
    throw unavailable();
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    throw unavailable();
  }
  const c = exact(parsed, canonical21Fields);
  // Exact serialization also rejects duplicate keys, whitespace, escapes,
  // alternate field ordering and number spellings. This is validation only;
  // the UI never reconstructs bytes for signing.
  if (
    JSON.stringify(c) !== raw ||
    Object.keys(c).join() !== canonical21Fields.join() ||
    c.version !== "rowvia.saved-local-registration/v1" ||
    c.operator_profile !== "localprofile/v1" ||
    c.tenant_id !== "01940000-0000-7000-8000-000000000001" ||
    c.owner_subject_id !== "01940000-0000-7000-8000-000000000002" ||
    c.owner_principal_id !== "01940000-0000-7000-8000-000000000004" ||
    c.default_space_id !== "01940000-0000-7000-8000-000000000005" ||
    c.credential_ttl_seconds !== 86400 ||
    c.manifest_version !== 1 ||
    JSON.stringify(c.lifecycle_actions) !==
      '["agent.instance.start","agent.instance.stop","runtime.instance.register","runtime.instance.stop"]' ||
    typeof c.agent_credential_id !== "string" ||
    !uuid7.test(c.agent_credential_id) ||
    typeof c.workload_credential_id !== "string" ||
    !uuid7.test(c.workload_credential_id) ||
    c.agent_credential_id === c.workload_credential_id ||
    ![
      c.agent_verifier_sha256,
      c.workload_verifier_sha256,
      c.retry_key_sha256,
      c.runtime_config_digest,
    ].every(hash) ||
    c.operation_id !== review.operationId ||
    c.agent_pubkey !== review.agentPubkey ||
    c.owner_pubkey !== review.ownerPubkey ||
    c.source_instance !== review.sourceInstance ||
    c.runtime_config_digest !== review.recipeSha256 ||
    c.harness_kind !== "custom-acp" ||
    c.agent_name !== `saved-${review.operationId.replaceAll("-", "")}`
  )
    throw unavailable();
  return Object.freeze(c);
}

async function verifyDigest(text: string, expected: string): Promise<void> {
  const digest = await globalThis.crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(text),
  );
  if (
    Array.from(new Uint8Array(digest), (value) =>
      value.toString(16).padStart(2, "0"),
    ).join("") !== expected
  )
    throw unavailable();
}

export function parseSavedAgentPending(
  raw: unknown,
): readonly SavedAgentPending[] {
  if (!Array.isArray(raw) || raw.length > 64) throw unavailable();
  const seen = new Set<string>();
  return Object.freeze(
    raw.map((item) => {
      const v = exact(item, [
        "version",
        "operationId",
        "agentPubkey",
        "name",
        "status",
      ]);
      if (
        v.version !== 1 ||
        typeof v.operationId !== "string" ||
        !uuid7.test(v.operationId) ||
        !hash(v.agentPubkey) ||
        !text(v.name, 128) ||
        !v.name ||
        v.name.trim() !== v.name ||
        v.status !== "pending" ||
        seen.has(v.operationId) ||
        seen.has(v.agentPubkey)
      )
        throw unavailable();
      seen.add(v.operationId);
      seen.add(v.agentPubkey);
      return Object.freeze({ ...v }) as SavedAgentPending;
    }),
  );
}

async function parseApproval(
  raw: unknown,
  review: SavedAgentReview,
): Promise<SavedAgentApproval> {
  if (
    raw === null ||
    typeof raw !== "object" ||
    Object.getPrototypeOf(raw) !== Object.prototype
  )
    throw unknown();
  const kind = Object.getOwnPropertyDescriptor(raw, "kind");
  if (!kind || !Object.hasOwn(kind, "value") || !kind.enumerable)
    throw unknown();
  if (kind.value === "authorization") {
    const value = exact(raw, ["kind", "authorization"]);
    return Object.freeze({
      kind: "authorization",
      authorization: parseSavedAgentAuthorization(
        value.authorization,
        review.operationId,
      ),
    });
  }
  const value = exact(raw, ["kind", "completion"]);
  const c = exact(value.completion, [
    "version",
    "operationId",
    "agentPubkey",
    "canonical21",
    "canonical21Sha256",
    "state",
    "bindingsReady",
    "runtimeReady",
  ]);
  if (
    value.kind !== "registered-stopped" ||
    c.version !== 1 ||
    c.operationId !== review.operationId ||
    c.agentPubkey !== review.agentPubkey ||
    c.canonical21 !== review.canonical21 ||
    c.canonical21Sha256 !== review.canonical21Sha256 ||
    c.state !== "registered-stopped" ||
    c.bindingsReady !== true ||
    c.runtimeReady !== false
  )
    throw unknown();
  await verifyDigest(review.canonical21, review.canonical21Sha256);
  return Object.freeze({
    kind: "registered-stopped",
    completion: Object.freeze({ ...c }) as SavedAgentRegistrationCompletion,
  });
}
export function parseSavedAgentAuthorization(
  raw: unknown,
  id: string,
): SavedAgentAuthorization {
  validOperation(id);
  const v = exact(raw, ["version", "operationId", "state", "expiresAt"]);
  if (
    v.version !== 1 ||
    v.operationId !== id ||
    typeof v.state !== "string" ||
    !["authorized", "expired", "revoked"].includes(v.state) ||
    !text(v.expiresAt, 40) ||
    !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?Z$/u.test(
      v.expiresAt,
    ) ||
    !Number.isFinite(Date.parse(v.expiresAt))
  )
    throw unknown();
  if (
    new Date(v.expiresAt).toISOString().slice(0, 19) !==
    v.expiresAt.slice(0, 19)
  )
    throw unknown();
  // Authorization state is NOT registration or runtime readiness.
  return Object.freeze({ ...v }) as SavedAgentAuthorization;
}
export function createSavedAgentOwnerApi(invoke: OwnerInvoke) {
  if (typeof invoke !== "function") throw unavailable();
  return Object.freeze({
    async review(operationId: string): Promise<SavedAgentReview> {
      validOperation(operationId);
      try {
        const view = parseSavedAgentReview(
          await invoke(
            "rowvia_review_saved_agent_preparation",
            Object.freeze({ input: Object.freeze({ operationId }) }),
          ),
          operationId,
        );
        await verifyDigest(view.canonical21, view.canonical21Sha256);
        return view;
      } catch {
        throw unavailable();
      }
    },
    async pending(): Promise<readonly SavedAgentPending[]> {
      try {
        return parseSavedAgentPending(
          await invoke("rowvia_list_saved_agent_pending"),
        );
      } catch {
        throw unavailable();
      }
    },
    async save(
      input: SavedCodexInput,
    ): Promise<Readonly<{ operationId: string }>> {
      const v = exact(input, [
        "displayName",
        "systemPrompt",
        "channelId",
        "recipe",
      ]);
      if (
        !text(v.displayName, 128) ||
        !v.displayName ||
        v.displayName.trim() !== v.displayName ||
        !text(v.systemPrompt, 4096) ||
        typeof v.channelId !== "string" ||
        !uuid.test(v.channelId) ||
        v.recipe !== "codex-development-memory-git-read-v1"
      )
        throw unavailable();
      let raw: unknown;
      try {
        raw = await invoke("rowvia_save_codex_agent", {
          input: Object.freeze({ ...v }),
        });
      } catch (error: unknown) {
        if (
          error instanceof Error &&
          error.message === "saved_agent_owner_unavailable"
        )
          throw unavailable();
        throw unknown();
      }
      try {
        const result = exact(raw, ["version", "operationId"]);
        if (result.version !== 1 || typeof result.operationId !== "string")
          throw unknown();
        validOperation(result.operationId);
        return Object.freeze({ operationId: result.operationId });
      } catch {
        throw unknown();
      }
    },
    async approve(review: SavedAgentReview): Promise<SavedAgentApproval> {
      validOperation(review.operationId);
      let raw: unknown;
      try {
        raw = await invoke("rowvia_approve_saved_agent_registration", {
          input: Object.freeze({ operationId: review.operationId }),
        });
      } catch (error: unknown) {
        if (
          error instanceof Error &&
          error.message === "saved_agent_owner_unavailable"
        )
          throw unavailable();
        throw unknown();
      }
      try {
        return await parseApproval(raw, review);
      } catch {
        throw unknown();
      }
    },
    // Call only from an explicit owner action. No review success effect, timer,
    // lifecycle hook or automatic retry calls this method.
    async authorize(operationId: string): Promise<SavedAgentAuthorization> {
      validOperation(operationId);
      let result: unknown;
      try {
        result = await invoke(
          "rowvia_authorize_saved_agent",
          Object.freeze({ input: Object.freeze({ operationId }) }),
        );
      } catch (error: unknown) {
        // Native's pre-dispatch fixed refusal is safe to retain. Everything
        // else may have crossed the signing/HTTP boundary: unknown, no retry.
        if (
          error instanceof Error &&
          error.message === "saved_agent_owner_unavailable"
        )
          throw unavailable();
        throw unknown();
      }
      try {
        return parseSavedAgentAuthorization(result, operationId);
      } catch {
        throw unknown();
      }
    },
  });
}
