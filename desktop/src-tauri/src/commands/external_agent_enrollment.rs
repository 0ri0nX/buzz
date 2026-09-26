//! One-time owner authorization for an agent whose signer stays outside Buzz.
//! No managed-agent record or ACP runtime is created by this module.

use nostr::{Event, EventBuilder, JsonUtil, Keys, Kind, PublicKey, Tag};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, State};

use crate::{
    app_state::{AppState, ExternalAgentEnrollment},
    managed_agents::{
        load_managed_agents,
        retention::{
            active_retention_scope, get_retained_event, open_retention_db, retain_event,
            RetainedEvent,
        },
        validate_managed_agent_definition_text,
    },
    relay::relay_ws_url_with_override,
};

const ENROLLMENT_TTL_SECONDS: u64 = 300;
const AGENT_KIND: u32 = 30177;
const MAX_PROOF_JSON_BYTES: usize = 8192;

#[derive(Serialize, Deserialize)]
pub(crate) struct ExternalAnnouncement {
    name: String,
    parallelism: u32,
    respond_to: String,
    external_enrollment: EnrollmentEvidence,
}

#[derive(Serialize, Deserialize)]
struct EnrollmentEvidence {
    version: u8,
    challenge_hash: String,
    proof_event_id: String,
    issued_at: u64,
    relay_url_hash: String,
}

fn challenge_hash(challenge: &str) -> String {
    hex::encode(Sha256::digest(challenge.as_bytes()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAgentChallenge {
    challenge: String,
    owner_pubkey: String,
    relay_url: String,
    expires_at: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteExternalAgentEnrollment {
    name: String,
    challenge: String,
    proof_event_json: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAgentAuthorization {
    agent_pubkey: String,
    owner_pubkey: String,
    relay_url: String,
    auth_tag: String,
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[tauri::command]
pub fn prepare_external_agent_enrollment(
    agent_pubkey: String,
    state: State<'_, AppState>,
) -> Result<ExternalAgentChallenge, String> {
    let agent = PublicKey::from_hex(agent_pubkey.trim())
        .map_err(|_| "agent public key must be a valid 64-character hex key".to_string())?;
    let owner = state.signing_keys()?.public_key().to_hex();
    if agent.to_hex() == owner {
        return Err("agent and owner must have different public keys".into());
    }
    let relay_url = relay_ws_url_with_override(&state);
    let issued_at = now_unix();
    // The random key is discarded; only its public bytes serve as a nonce.
    let nonce = Keys::generate().public_key().to_hex();
    let challenge = format!(
        "buzz:external-agent-enrollment:v1:{owner}:{}:{nonce}",
        agent.to_hex()
    );
    let mut pending = state
        .external_agent_enrollments
        .lock()
        .map_err(|e| e.to_string())?;
    pending.retain(|_, entry| issued_at.saturating_sub(entry.issued_at) <= ENROLLMENT_TTL_SECONDS);
    if pending.len() >= 32 {
        return Err("too many pending agent enrollments".into());
    }
    pending.insert(
        challenge.clone(),
        ExternalAgentEnrollment {
            agent_pubkey: agent.to_hex(),
            owner_pubkey: owner.clone(),
            relay_url: relay_url.clone(),
            issued_at,
        },
    );
    Ok(ExternalAgentChallenge {
        challenge,
        owner_pubkey: owner,
        relay_url,
        expires_at: issued_at + ENROLLMENT_TTL_SECONDS,
    })
}

fn verify_proof(
    proof: &Event,
    challenge: &str,
    entry: &ExternalAgentEnrollment,
    now: u64,
) -> Result<(), String> {
    proof
        .verify()
        .map_err(|_| "invalid agent proof signature".to_string())?;
    if proof.kind != Kind::TextNote
        || proof.pubkey.to_hex() != entry.agent_pubkey
        || proof.content != challenge
    {
        return Err("agent proof does not match the enrollment challenge".into());
    }
    let owner_tags: Vec<_> = proof
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().map(String::as_str) == Some("p"))
        .collect();
    if owner_tags.len() != 1
        || owner_tags[0].as_slice().get(1).map(String::as_str) != Some(entry.owner_pubkey.as_str())
    {
        return Err("agent proof must address exactly the current owner".into());
    }
    let created_at = proof.created_at.as_secs();
    if now > entry.issued_at + ENROLLMENT_TTL_SECONDS
        || created_at < entry.issued_at
        || created_at > entry.issued_at + ENROLLMENT_TTL_SECONDS
        || created_at > now.saturating_add(30)
    {
        return Err("agent proof is stale or outside the challenge lifetime".into());
    }
    Ok(())
}

fn mint_auth_tag(owner_keys: &Keys, agent_pubkey: &str) -> Result<String, String> {
    // Bridge the desktop's nostr version to buzz-sdk's NIP-OA implementation.
    let compat_owner = nostr::Keys::parse(&owner_keys.secret_key().to_secret_hex())
        .map_err(|e| format!("failed to bridge owner signer: {e}"))?;
    let compat_agent =
        nostr::PublicKey::from_hex(agent_pubkey).map_err(|e| format!("invalid agent key: {e}"))?;
    buzz_sdk_pkg::nip_oa::compute_auth_tag(&compat_owner, &compat_agent, "kind=0")
        .map_err(|e| format!("failed to authorize agent profile: {e}"))
}

/// Only owner-signed rows with the external enrollment marker can be recovered
/// or refreshed at flush time. Managed-agent 30177 rows never carry this marker.
pub(crate) fn verified_external_announcement(
    row: &RetainedEvent,
    event: &Event,
    owner_pubkey: &str,
    relay_url: &str,
) -> Option<ExternalAnnouncement> {
    if row.kind != AGENT_KIND
        || row.pubkey != owner_pubkey
        || event.kind != Kind::Custom(AGENT_KIND as u16)
        || event.pubkey.to_hex() != owner_pubkey
        || event.content != row.content
        || event.created_at.as_secs() as i64 != row.created_at
        || event.tags.len() != 1
        || event.tags.iter().next()?.as_slice().len() != 2
        || event.tags.iter().next()?.as_slice()[0] != "d"
        || event.tags.iter().next()?.as_slice()[1] != row.d_tag.as_str()
        || event.verify().is_err()
    {
        return None;
    }
    let content: ExternalAnnouncement = serde_json::from_str(&row.content).ok()?;
    (content.external_enrollment.version == 1
        && content.parallelism == 1
        && content.respond_to == "owner-only"
        && content.external_enrollment.relay_url_hash == challenge_hash(relay_url)
        && content.external_enrollment.challenge_hash.len() == 64
        && content.external_enrollment.proof_event_id.len() == 64)
        .then_some(content)
}

fn authorize_public_agent(
    conn: &rusqlite::Connection,
    owner_keys: &Keys,
    agent_pubkey: &str,
    name: &str,
    challenge: &str,
    proof: &Event,
    issued_at: u64,
    relay_url: &str,
) -> Result<String, String> {
    let owner_pubkey = owner_keys.public_key().to_hex();
    if get_retained_event(conn, AGENT_KIND, &owner_pubkey, agent_pubkey)?.is_some() {
        return Err("this agent is already announced in this workspace".into());
    }

    let auth_tag = mint_auth_tag(owner_keys, agent_pubkey)?;
    let content = serde_json::to_string(&ExternalAnnouncement {
        name: name.to_string(),
        parallelism: 1,
        respond_to: "owner-only".to_string(),
        external_enrollment: EnrollmentEvidence {
            version: 1,
            challenge_hash: challenge_hash(challenge),
            proof_event_id: proof.id.to_hex(),
            issued_at,
            relay_url_hash: challenge_hash(relay_url),
        },
    })
    .map_err(|e| format!("failed to serialize agent announcement: {e}"))?;
    let event = EventBuilder::new(Kind::Custom(AGENT_KIND as u16), content)
        .tags([Tag::parse(["d", agent_pubkey]).map_err(|e| e.to_string())?])
        .sign_with_keys(owner_keys)
        .map_err(|e| format!("failed to sign agent announcement: {e}"))?;
    retain_event(
        conn,
        &RetainedEvent {
            kind: AGENT_KIND,
            pubkey: owner_pubkey,
            d_tag: agent_pubkey.to_string(),
            content: event.content.clone(),
            created_at: event.created_at.as_secs() as i64,
            raw_event: event.as_json(),
            pending_sync: true,
        },
    )?;
    Ok(auth_tag)
}

fn complete_external_agent_enrollment_at(
    input: CompleteExternalAgentEnrollment,
    state: &AppState,
    conn: &rusqlite::Connection,
    owner_keys: &Keys,
    relay_url: &str,
    managed_exists: bool,
    now: u64,
) -> Result<ExternalAgentAuthorization, String> {
    let name = input.name.trim();
    validate_managed_agent_definition_text(name, None, None)?;
    if input.challenge.len() > 256 || input.proof_event_json.len() > MAX_PROOF_JSON_BYTES {
        return Err("enrollment challenge or proof is too large".into());
    }
    let proof: Event = serde_json::from_str(&input.proof_event_json)
        .map_err(|_| "proof must be a signed Nostr event JSON object".to_string())?;
    let agent_pubkey = proof.pubkey.to_hex();
    let owner_pubkey = owner_keys.public_key().to_hex();
    if managed_exists {
        return Err("this public key already belongs to a local managed agent".into());
    }
    let existing = get_retained_event(conn, AGENT_KIND, &owner_pubkey, &agent_pubkey)?;
    let auth_tag = if let Some(existing) = existing {
        let event = Event::from_json(&existing.raw_event)
            .map_err(|_| "existing agent announcement is invalid".to_string())?;
        let content =
            verified_external_announcement(&existing, &event, &owner_pubkey, relay_url)
                .ok_or_else(|| "this key already has a different owner announcement".to_string())?;
        if content.name != name {
            return Err("existing agent name differs; enrollment cannot change policy".into());
        }
        if content.external_enrollment.relay_url_hash != challenge_hash(relay_url) {
            return Err("workspace changed since enrollment".into());
        }
        if content.external_enrollment.challenge_hash == challenge_hash(&input.challenge)
            && content.external_enrollment.proof_event_id == proof.id.to_hex()
        {
            // Exact retry, including after restart: the durable owner-signed
            // row proves this proof passed the original fresh challenge gate.
            let original = ExternalAgentEnrollment {
                agent_pubkey: agent_pubkey.clone(),
                owner_pubkey: owner_pubkey.clone(),
                relay_url: relay_url.to_string(),
                issued_at: content.external_enrollment.issued_at,
            };
            verify_proof(
                &proof,
                &input.challenge,
                &original,
                proof.created_at.as_secs(),
            )?;
        } else {
            // If the original challenge/proof was lost, a fresh proof by the
            // same agent key can recover the public tag without rewriting 30177.
            let pending = state
                .external_agent_enrollments
                .lock()
                .map_err(|e| e.to_string())?
                .remove(&input.challenge)
                .ok_or_else(|| "unknown or already-used enrollment challenge".to_string())?;
            if pending.owner_pubkey != owner_pubkey || pending.relay_url != relay_url {
                return Err("owner or workspace changed since challenge creation".into());
            }
            verify_proof(&proof, &input.challenge, &pending, now)?;
        }
        mint_auth_tag(owner_keys, &agent_pubkey)?
    } else {
        // First enrollment consumes its challenge before proof verification.
        let pending = state
            .external_agent_enrollments
            .lock()
            .map_err(|e| e.to_string())?
            .remove(&input.challenge)
            .ok_or_else(|| "unknown or already-used enrollment challenge".to_string())?;
        if pending.owner_pubkey != owner_pubkey || pending.relay_url != relay_url {
            return Err("owner or workspace changed since challenge creation".into());
        }
        verify_proof(&proof, &input.challenge, &pending, now)?;
        authorize_public_agent(
            conn,
            owner_keys,
            &agent_pubkey,
            name,
            &input.challenge,
            &proof,
            pending.issued_at,
            relay_url,
        )?
    };
    Ok(ExternalAgentAuthorization {
        agent_pubkey,
        owner_pubkey,
        relay_url: relay_url.to_string(),
        auth_tag,
    })
}

#[tauri::command]
pub fn complete_external_agent_enrollment(
    input: CompleteExternalAgentEnrollment,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ExternalAgentAuthorization, String> {
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let scope = active_retention_scope(&app, &state)?;
    let proof: Event = if input.proof_event_json.len() <= MAX_PROOF_JSON_BYTES {
        serde_json::from_str(&input.proof_event_json)
            .map_err(|_| "proof must be a signed Nostr event JSON object".to_string())?
    } else {
        return Err("enrollment proof is too large".into());
    };
    let managed_exists = load_managed_agents(&app)?
        .iter()
        .any(|agent| agent.pubkey.eq_ignore_ascii_case(&proof.pubkey.to_hex()));
    let conn = open_retention_db(&scope.db_path)?;
    complete_external_agent_enrollment_at(
        input,
        &state,
        &conn,
        &scope.owner_keys,
        &scope.relay_url,
        managed_exists,
        now_unix(),
    )
}

#[cfg(test)]
#[path = "external_agent_enrollment_tests.rs"]
mod tests;
