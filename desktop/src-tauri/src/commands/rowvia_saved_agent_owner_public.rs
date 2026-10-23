//! Closed public registration bytes verified before reaching an owner view.
use super::{hash, Failure, Resumed};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Lexicographic Core canonical21 shape; typed decoding rejects duplicate keys.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct CanonicalRegistration {
    agent_credential_id: String,
    agent_name: String,
    agent_pubkey: String,
    agent_verifier_sha256: String,
    credential_ttl_seconds: u32,
    default_space_id: String,
    harness_kind: String,
    lifecycle_actions: [String; 4],
    manifest_version: u8,
    operation_id: String,
    operator_profile: String,
    owner_principal_id: String,
    owner_pubkey: String,
    owner_subject_id: String,
    retry_key_sha256: String,
    runtime_config_digest: String,
    source_instance: String,
    tenant_id: String,
    version: String,
    workload_credential_id: String,
    workload_verifier_sha256: String,
}

/// Public JSON is metadata only. Never accept arbitrary child output here.
pub(super) fn verify_canonical21(
    text: &str,
    digest: &str,
    resumed: &Resumed,
) -> Result<(), Failure> {
    if text.is_empty()
        || text.len() > 4096
        || !hash(digest)
        || hex::encode(Sha256::digest(text.as_bytes())) != digest
        || digest != resumed.ack.canonical21_sha256
    {
        return Err(Failure::Unavailable);
    }
    let c: CanonicalRegistration = serde_json::from_str(text).map_err(|_| Failure::Unavailable)?;
    let s = &resumed.expected.selection;
    let canonical = serde_json::to_string(&c).map_err(|_| Failure::Unavailable)?;
    if canonical != text
        || c.version != "rowvia.saved-local-registration/v1"
        || c.operator_profile != "localprofile/v1"
        || c.tenant_id != "01940000-0000-7000-8000-000000000001"
        || c.owner_subject_id != "01940000-0000-7000-8000-000000000002"
        || c.owner_principal_id != "01940000-0000-7000-8000-000000000004"
        || c.default_space_id != "01940000-0000-7000-8000-000000000005"
        || c.credential_ttl_seconds != 86400 || c.manifest_version != 1
        || c.lifecycle_actions != ["agent.instance.start", "agent.instance.stop", "runtime.instance.register", "runtime.instance.stop"]
        || super::operation(&c.agent_credential_id).is_err()
        || super::operation(&c.workload_credential_id).is_err()
        || c.agent_credential_id == c.workload_credential_id
        || ![&c.agent_verifier_sha256, &c.workload_verifier_sha256, &c.retry_key_sha256, &c.runtime_config_digest].iter().all(|v| hash(v))
        || c.operation_id != s.operation_id
        || c.agent_name != resumed.prepared.core_agent_name
        || c.agent_pubkey != resumed.expected.agent_pubkey
        || c.owner_pubkey != s.owner_pubkey || c.agent_pubkey == c.owner_pubkey
        || c.source_instance != s.source_instance || c.harness_kind != "custom-acp"
        // Recipe commitment differs from authorization's whole canonical21 SHA.
        || c.runtime_config_digest != resumed.ack.recipe_sha256
    {
        return Err(Failure::Unavailable);
    }
    Ok(())
}
