import { createHash } from "node:crypto";
export const id = "01940000-0000-7000-8000-000000000077";
export const ownerPubkey = "b".repeat(64);
export const agentPubkey = "a".repeat(64);
export const relayUrl = "wss://a.example.invalid";
export function canonical(overrides = {}) {
  return JSON.stringify({
    agent_credential_id: "01940000-0000-7000-8000-000000000012",
    agent_name: `saved-${id.replaceAll("-", "")}`,
    agent_pubkey: agentPubkey,
    agent_verifier_sha256: "3".repeat(64),
    credential_ttl_seconds: 86400,
    default_space_id: "01940000-0000-7000-8000-000000000005",
    harness_kind: "custom-acp",
    lifecycle_actions: [
      "agent.instance.start",
      "agent.instance.stop",
      "runtime.instance.register",
      "runtime.instance.stop",
    ],
    manifest_version: 1,
    operation_id: id,
    operator_profile: "localprofile/v1",
    owner_principal_id: "01940000-0000-7000-8000-000000000004",
    owner_pubkey: ownerPubkey,
    owner_subject_id: "01940000-0000-7000-8000-000000000002",
    retry_key_sha256: "4".repeat(64),
    runtime_config_digest: "e".repeat(64),
    source_instance: "fixture",
    tenant_id: "01940000-0000-7000-8000-000000000001",
    version: "rowvia.saved-local-registration/v1",
    workload_credential_id: "01940000-0000-7000-8000-000000000013",
    workload_verifier_sha256: "5".repeat(64),
    ...overrides,
  });
}
export const digest = (text) => createHash("sha256").update(text).digest("hex");
export function review() {
  const canonical21 = canonical();
  return {
    version: 1,
    operationId: id,
    agentPubkey,
    ownerPubkey,
    relayUrl,
    sourceInstance: "fixture",
    name: "Original Ω name",
    systemPrompt: "Owner-reviewed prompt",
    channelId: "01940000-0000-7000-8000-000000000088",
    recipeId: "codex-development-memory-git-read-v1",
    model: "gpt-6.1-sol",
    recipeSha256: "e".repeat(64),
    canonical21,
    canonical21Sha256: digest(canonical21),
    createdStopped: true,
    bindingsReady: false,
    runtimeReady: false,
  };
}
export const authorization = (state = "authorized") => ({
  version: 1,
  operationId: id,
  state,
  expiresAt: "2030-01-01T00:00:00Z",
});
export const completion = () => ({
  kind: "registered-stopped",
  completion: {
    version: 1,
    operationId: id,
    agentPubkey,
    canonical21: canonical(),
    canonical21Sha256: digest(canonical()),
    state: "registered-stopped",
    bindingsReady: true,
    runtimeReady: false,
  },
});
