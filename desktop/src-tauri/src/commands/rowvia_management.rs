//! Owner-signed, bounded Rowvia management transport. The endpoint is fixed by
//! the native build; neither an agent nor an invoke argument supplies a URL.

use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use nostr::{Event, JsonUtil, Keys};
use reqwest::{Method, Url};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, State};

use super::rowvia_management_journal::{self as journal, Candidate, Entry, Scope};
use super::identity_archive::fetch_relay_self_at;
use crate::{
    app_state::AppState,
    managed_agents::{managed_agents_base_dir, retention::active_retention_scope},
    nostr_convert,
    relay::build_nip98_auth_header_for_keys,
    relay::{query_relay_at_with_keys, relay_http_base_url},
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_REQUEST_BYTES: usize = 64 * 1024;
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const API_PREFIX: &str = "v1/buzz/management";
const CANDIDATE_TTL_SECONDS: i64 = 120;
const OPERATIONS: [&str; 2] = ["gmail.message.read", "gmail.message.search"];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagementAction {
    #[serde(rename = "connector.grant")]
    ConnectorGrant,
    #[serde(rename = "connector.revoke")]
    ConnectorRevoke,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManagementProposalRequest {
    pub request_id: String,
    pub action: ManagementAction,
    pub target_agent_pubkey: String,
    pub binding_id: String,
    pub operations: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ManagementProposalResponse {
    pub proposal_id: String,
    pub operation_id: String,
    pub proposal_digest: String,
    /// Server-produced canonical JSON bytes, preserved without reserialization.
    pub proposal_bytes_b64: String,
}

#[derive(Clone, Debug, Deserialize)]
struct CandidateWire {
    agent_name: String,
    target_pubkey: String,
    binding_id: String,
    gmail_label: String,
    binding_status: String,
}

#[derive(Deserialize)]
struct CandidatesWire {
    candidates: Vec<CandidateWire>,
    truncated: bool,
}

#[derive(Serialize)]
pub struct CandidateView {
    selection_handle: String,
    agent_name: String,
    gmail_label: String,
    binding_status: String,
}

#[derive(Serialize)]
pub struct CandidatesView {
    candidates: Vec<CandidateView>,
    truncated: bool,
}

#[derive(Deserialize)]
pub struct CreateSelection {
    selection_handle: String,
    action: ManagementAction,
    relay_event_json: String,
}

// Typed decryption rejects duplicate and unknown keys before any JSON object
// can collapse them. CLI emits one frame per connector draft.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConnectorFrame {
    seq: u64,
    timestamp: String,
    kind: String,
    agent_index: Option<usize>,
    channel_id: Option<String>,
    session_id: Option<String>,
    turn_id: Option<String>,
    payload: ConnectorPayload,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConnectorPayload {
    #[serde(rename = "type")]
    request_type: String,
    version: u8,
    action: String,
    request_id: String,
    request: ConnectorHints,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConnectorHints {
    channel_id: String,
    target_name: String,
    gmail_labels: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct NativeReceipt {
    receipt_handle: String,
    #[serde(flatten)]
    proposal: ManagementProposalResponse,
}

#[derive(Serialize)]
pub struct PendingReceipts {
    receipts: Vec<NativeReceipt>,
    unresolved: Vec<UnresolvedProposal>,
}

fn pending_from_journal(path: &std::path::Path, scope: &Scope) -> Result<PendingReceipts, String> {
    let (receipts, unresolved) = journal::transact(path, scope, |stored| {
        let mut receipts = Vec::new();
        let mut unresolved = Vec::new();
        for entry in &stored.entries {
            if entry.refused || entry.approval_intent {
                continue;
            }
            if entry.receipt_body_b64.is_none() {
                let (_, request) = unresolved_request(entry)?;
                unresolved.push(UnresolvedProposal {
                    receipt_handle: entry.receipt_handle.clone(),
                    action: request.action,
                });
                continue;
            }
            match decode_entry_receipt(entry, scope) {
                Ok((proposal, _)) => receipts.push(NativeReceipt {
                    receipt_handle: entry.receipt_handle.clone(),
                    proposal,
                }),
                Err(error) if error == "Rowvia proposal has expired" => {}
                Err(error) => return Err(error),
            }
        }
        Ok(((receipts, unresolved), false))
    })?;
    Ok(PendingReceipts {
        receipts,
        unresolved,
    })
}

#[derive(Serialize)]
pub struct UnresolvedProposal {
    receipt_handle: String,
    action: ManagementAction,
}

#[derive(Serialize)]
pub struct OperationRecovery {
    receipt_handle: String,
    operation_id: String,
    approval_intent: bool,
    refused: bool,
    last_status: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ManagementApprovalRequest {
    pub proposal_id: String,
    pub operation_id: String,
    pub proposal_digest: String,
    pub proposal_bytes_b64: String,
    pub expires_at: String,
}

#[derive(Serialize)]
struct ApprovalWire<'a> {
    version: &'static str,
    proposal_id: &'a str,
    operation_id: &'a str,
    proposal_digest: &'a str,
    decision: &'static str,
    expires_at: &'a str,
}

struct ManagementClient {
    origin: Url,
    http: reqwest::Client,
    timeout: Duration,
}

impl ManagementClient {
    fn configured() -> Result<Self, String> {
        let origin = option_env!("BUZZ_DESKTOP_BUILD_ROWVIA_MANAGEMENT_ORIGIN")
            .ok_or("Rowvia management origin is not configured in this build")?;
        Self::new(origin, false, REQUEST_TIMEOUT)
    }

    fn new(origin: &str, allow_loopback_http: bool, timeout: Duration) -> Result<Self, String> {
        let url = Url::parse(origin).map_err(|_| "invalid Rowvia management origin")?;
        let allowed_scheme = url.scheme() == "https"
            || (allow_loopback_http
                && url.scheme() == "http"
                && matches!(url.host_str(), Some("127.0.0.1" | "localhost")));
        if !allowed_scheme
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("Rowvia management origin must be a bare HTTPS origin".to_string());
        }
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "could not create Rowvia management client")?;
        Ok(Self {
            origin: url,
            http,
            timeout,
        })
    }

    fn endpoint(&self, segments: &[&str]) -> Result<Url, String> {
        let mut url = self.origin.clone();
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| "invalid Rowvia management origin")?;
            path.pop_if_empty();
            path.extend(API_PREFIX.split('/'));
            path.extend(segments);
        }
        Ok(url)
    }

    async fn request_raw(
        &self,
        keys: &Keys,
        method: Method,
        segments: &[&str],
        body: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, String> {
        let body = body.unwrap_or_default();
        if body.len() > MAX_REQUEST_BYTES {
            return Err("Rowvia management request is too large".to_string());
        }
        let url = self.endpoint(segments)?;
        let auth = build_nip98_auth_header_for_keys(keys, &method, url.as_str(), &body)?;
        let mut request = self
            .http
            .request(method, url)
            .header(reqwest::header::AUTHORIZATION, auth)
            .header(reqwest::header::ACCEPT, "application/json");
        if !body.is_empty() {
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body);
        }
        tokio::time::timeout(self.timeout, async {
            let response = request
                .send()
                .await
                .map_err(|_| "Rowvia management request failed".to_string())?;
            if !response.status().is_success() {
                return Err(format!(
                    "Rowvia management HTTP {}",
                    response.status().as_u16()
                ));
            }
            if response
                .content_length()
                .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
            {
                return Err("Rowvia management response is too large".to_string());
            }
            let mut stream = response.bytes_stream();
            let mut bytes = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|_| "Rowvia management response failed")?;
                if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                    return Err("Rowvia management response is too large".to_string());
                }
                bytes.extend_from_slice(&chunk);
            }
            serde_json::from_slice::<serde_json::Value>(&bytes)
                .map_err(|_| "Rowvia management response is not valid JSON".to_string())?;
            Ok(bytes)
        })
        .await
        .map_err(|_| "Rowvia management request timed out".to_string())?
    }

    async fn request(
        &self,
        keys: &Keys,
        method: Method,
        segments: &[&str],
        body: Option<Vec<u8>>,
    ) -> Result<serde_json::Value, String> {
        let bytes = self.request_raw(keys, method, segments, body).await?;
        serde_json::from_slice(&bytes).map_err(|_| "invalid Rowvia response".to_string())
    }

    async fn candidates(&self, keys: &Keys) -> Result<CandidatesWire, String> {
        let bytes = self
            .request_raw(keys, Method::GET, &["candidates"], None)
            .await?;
        let result: CandidatesWire = serde_json::from_slice(&bytes)
            .map_err(|_| "invalid Rowvia candidates response".to_string())?;
        if result.candidates.len() > 64 {
            return Err("too many Rowvia candidates".to_string());
        }
        for candidate in &result.candidates {
            validate_pubkey(&candidate.target_pubkey)?;
            validate_id(&candidate.binding_id)?;
            if candidate.agent_name.is_empty()
                || candidate.agent_name.len() > 64
                || candidate.gmail_label.len() > 128
                || candidate.binding_status != "active"
            {
                return Err("invalid Rowvia candidate".to_string());
            }
        }
        Ok(result)
    }

    async fn create_proposal(
        &self,
        keys: &Keys,
        proposal: &ManagementProposalRequest,
    ) -> Result<ManagementProposalResponse, String> {
        validate_id(&proposal.request_id)?;
        validate_id(&proposal.binding_id)?;
        if proposal.target_agent_pubkey.len() != 64
            || !proposal
                .target_agent_pubkey
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("invalid target agent pubkey".to_string());
        }
        if proposal.operations.is_empty()
            || proposal.operations.len() > 64
            || proposal.operations.iter().any(|operation| {
                operation.is_empty() || operation.len() > 128 || !operation.is_ascii()
            })
            || proposal
                .operations
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
        {
            return Err("operations must be sorted, unique, and bounded".to_string());
        }
        let body = serde_json::to_vec(proposal).map_err(|_| "invalid proposal")?;
        let response = self
            .request(keys, Method::POST, &["proposals"], Some(body))
            .await?;
        let receipt: ManagementProposalResponse = serde_json::from_value(response)
            .map_err(|_| "invalid Rowvia proposal response".to_string())?;
        verify_canonical_proposal(
            &receipt.proposal_bytes_b64,
            &receipt.proposal_digest,
            &receipt.proposal_id,
            &receipt.operation_id,
        )?;
        Ok(receipt)
    }

    async fn approve(
        &self,
        keys: &Keys,
        approval: &ManagementApprovalRequest,
    ) -> Result<serde_json::Value, String> {
        let body = approval_body(approval, true)?;
        self.post_approval_body(keys, &approval.proposal_id, body).await
    }

    async fn post_approval_body(
        &self,
        keys: &Keys,
        proposal_id: &str,
        body: Vec<u8>,
    ) -> Result<serde_json::Value, String> {
        validate_id(proposal_id)?;
        self.request(
            keys,
            Method::POST,
            &["proposals", proposal_id, "approve"],
            Some(body),
        )
        .await
    }

    async fn operation(
        &self,
        keys: &Keys,
        operation_id: &str,
    ) -> Result<serde_json::Value, String> {
        validate_id(operation_id)?;
        self.request(keys, Method::GET, &["operations", operation_id], None)
            .await
    }
}

fn approval_body(
    approval: &ManagementApprovalRequest,
    require_fresh: bool,
) -> Result<Vec<u8>, String> {
        validate_id(&approval.proposal_id)?;
        validate_id(&approval.operation_id)?;
        let canonical_expiry = verify_canonical_proposal_with_freshness(
            &approval.proposal_bytes_b64,
            &approval.proposal_digest,
            &approval.proposal_id,
            &approval.operation_id,
            require_fresh,
        )?;
        if approval.expires_at != canonical_expiry {
            return Err("Rowvia proposal expiry mismatch".to_string());
        }
        let wire = ApprovalWire {
            version: "rowvia.buzz-management-approval/v1",
            proposal_id: &approval.proposal_id,
            operation_id: &approval.operation_id,
            proposal_digest: &approval.proposal_digest,
            decision: "approve",
            expires_at: &canonical_expiry,
        };
        serde_json::to_vec(&wire).map_err(|_| "invalid Rowvia approval".to_string())
}

fn verify_canonical_proposal(
    encoded: &str,
    digest: &str,
    proposal_id: &str,
    operation_id: &str,
) -> Result<String, String> {
    verify_canonical_proposal_with_freshness(encoded, digest, proposal_id, operation_id, true)
}

fn verify_canonical_proposal_with_freshness(
    encoded: &str,
    digest: &str,
    proposal_id: &str,
    operation_id: &str,
    require_fresh: bool,
) -> Result<String, String> {
    validate_id(proposal_id)?;
    validate_id(operation_id)?;
    if encoded.len() > MAX_RESPONSE_BYTES * 2 || digest.len() != 64 {
        return Err("invalid canonical Rowvia proposal".to_string());
    }
    let bytes = BASE64
        .decode(encoded)
        .map_err(|_| "invalid canonical Rowvia proposal")?;
    if bytes.len() > MAX_RESPONSE_BYTES || hex::encode(Sha256::digest(&bytes)) != digest {
        return Err("Rowvia proposal digest mismatch".to_string());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "invalid canonical Rowvia proposal")?;
    if value.get("proposal_id").and_then(|id| id.as_str()) != Some(proposal_id)
        || value.get("operation_id").and_then(|id| id.as_str()) != Some(operation_id)
    {
        return Err("Rowvia proposal identity mismatch".to_string());
    }
    let expires_at = value
        .get("expires_at")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty() && value.len() <= 64)
        .ok_or("invalid canonical Rowvia proposal expiry")?;
    let expiry = DateTime::parse_from_rfc3339(expires_at)
        .map_err(|_| "invalid canonical Rowvia proposal expiry")?;
    if require_fresh && expiry <= Utc::now() {
        return Err("Rowvia proposal has expired".to_string());
    }
    Ok(expires_at.to_string())
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err("invalid Rowvia management identifier".to_string());
    }
    Ok(())
}

fn validate_pubkey(pubkey: &str) -> Result<(), String> {
    if pubkey.len() != 64 || !pubkey.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid Rowvia agent pubkey".to_string());
    }
    Ok(())
}

fn configured_cerberus() -> Result<String, String> {
    let pin = option_env!("BUZZ_DESKTOP_BUILD_CERBERUS_PUBKEY")
        .ok_or("Cerberus identity is not configured in this build")?;
    validate_pubkey(pin)?;
    Ok(pin.to_ascii_lowercase())
}

fn capture_scope(
    app: &AppHandle,
    state: &AppState,
    client: &ManagementClient,
) -> Result<(Scope, Keys), String> {
    let active = active_retention_scope(app, state)?;
    let scope = Scope {
        owner_pubkey: active.owner_keys.public_key().to_hex(),
        relay_url: buzz_core_pkg::relay::normalize_relay_url(&active.relay_url)
            .map_err(|error| error.to_string())?,
        origin: client.origin.as_str().to_string(),
        source_instance: option_env!("BUZZ_DESKTOP_BUILD_ROWVIA_SOURCE_INSTANCE")
            .ok_or("Rowvia source instance is not configured in this build")?
            .to_string(),
        cerberus_pubkey: configured_cerberus()?,
    };
    let expected_owner = option_env!("BUZZ_DESKTOP_BUILD_ROWVIA_OWNER_PUBKEY")
        .ok_or("Rowvia owner identity is not configured in this build")?;
    validate_pubkey(expected_owner)?;
    if scope.owner_pubkey != expected_owner.to_ascii_lowercase() {
        return Err("active identity is not the configured Rowvia owner".to_string());
    }
    Ok((scope, active.owner_keys))
}

fn assert_scope(
    app: &AppHandle,
    state: &AppState,
    client: &ManagementClient,
    expected: &Scope,
) -> Result<(), String> {
    let (current, _) = capture_scope(app, state, client)?;
    if &current == expected {
        Ok(())
    } else {
        Err("Rowvia owner or community changed".to_string())
    }
}

fn journal_path(app: &AppHandle, scope: &Scope) -> Result<std::path::PathBuf, String> {
    journal::path(&managed_agents_base_dir(app)?, scope)
}

fn verify_receipt_against_request(
    receipt: &ManagementProposalResponse,
    request: &ManagementProposalRequest,
    scope: &Scope,
) -> Result<String, String> {
    verify_receipt_against_request_with_freshness(receipt, request, scope, true)
}

fn verify_receipt_against_request_with_freshness(
    receipt: &ManagementProposalResponse,
    request: &ManagementProposalRequest,
    scope: &Scope,
    require_fresh: bool,
) -> Result<String, String> {
    let expiry = verify_canonical_proposal_with_freshness(
        &receipt.proposal_bytes_b64,
        &receipt.proposal_digest,
        &receipt.proposal_id,
        &receipt.operation_id,
        require_fresh,
    )?;
    let bytes = BASE64
        .decode(&receipt.proposal_bytes_b64)
        .map_err(|_| "invalid canonical Rowvia proposal")?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "invalid canonical Rowvia proposal")?;
    let expected_action =
        serde_json::to_value(&request.action).map_err(|_| "invalid Rowvia action")?;
    if value["version"] != 1
        || value["proposal_id"] != request.request_id
        || value["owner_pubkey"] != scope.owner_pubkey
        || value["action"] != expected_action
        || value["target_pubkey"] != request.target_agent_pubkey
        || value["binding_id"] != request.binding_id
        || value["source_instance_id"] != scope.source_instance
        || value["operations"] != serde_json::json!(OPERATIONS)
    {
        return Err("Rowvia receipt does not match the frozen request".to_string());
    }
    Ok(expiry)
}

fn decode_entry_receipt(
    entry: &Entry,
    scope: &Scope,
) -> Result<(ManagementProposalResponse, String), String> {
    decode_entry_receipt_with_freshness(entry, scope, true)
}

fn decode_entry_receipt_with_freshness(
    entry: &Entry,
    scope: &Scope,
    require_fresh: bool,
) -> Result<(ManagementProposalResponse, String), String> {
    let encoded = entry
        .receipt_body_b64
        .as_ref()
        .ok_or("Rowvia receipt is pending")?;
    let bytes = BASE64
        .decode(encoded)
        .map_err(|_| "invalid Rowvia journal receipt")?;
    if hex::encode(Sha256::digest(&bytes)) != entry.receipt_digest.as_deref().unwrap_or_default() {
        return Err("Rowvia journal receipt digest mismatch".to_string());
    }
    let receipt: ManagementProposalResponse =
        serde_json::from_slice(&bytes).map_err(|_| "invalid Rowvia journal receipt")?;
    let body = BASE64
        .decode(&entry.request_body_b64)
        .map_err(|_| "invalid Rowvia journal request")?;
    if hex::encode(Sha256::digest(&body)) != entry.request_digest {
        return Err("Rowvia journal request digest mismatch".to_string());
    }
    let request: ManagementProposalRequest =
        serde_json::from_slice(&body).map_err(|_| "invalid Rowvia journal request")?;
    let expiry =
        verify_receipt_against_request_with_freshness(&receipt, &request, scope, require_fresh)?;
    if entry.operation_id.as_deref() != Some(&receipt.operation_id) {
        return Err("Rowvia journal operation mismatch".to_string());
    }
    Ok((receipt, expiry))
}

fn frozen_approval_body(
    entry: &Entry,
    scope: &Scope,
) -> Result<(ManagementProposalResponse, Vec<u8>), String> {
    if !entry.approval_intent || entry.refused {
        return Err("Rowvia approval was not requested".to_string());
    }
    let (receipt, expires_at) = decode_entry_receipt_with_freshness(entry, scope, false)?;
    let (Some(encoded), Some(digest)) =
        (&entry.approval_body_b64, &entry.approval_body_digest)
    else {
        return Err("Rowvia approval recovery body is unavailable".to_string());
    };
    let bytes = BASE64
        .decode(encoded)
        .map_err(|_| "invalid Rowvia journal approval")?;
    if bytes.len() > MAX_REQUEST_BYTES || hex::encode(Sha256::digest(&bytes)) != *digest {
        return Err("Rowvia journal approval digest mismatch".to_string());
    }
    let expected = approval_body(
        &ManagementApprovalRequest {
            proposal_id: receipt.proposal_id.clone(),
            operation_id: receipt.operation_id.clone(),
            proposal_digest: receipt.proposal_digest.clone(),
            proposal_bytes_b64: receipt.proposal_bytes_b64.clone(),
            expires_at,
        },
        false,
    )?;
    if bytes != expected {
        return Err("Rowvia journal approval does not match receipt".to_string());
    }
    Ok((receipt, bytes))
}

fn frozen_replay_body(
    entry: &Entry,
    draft_digest: &str,
    proposed: &ManagementProposalRequest,
) -> Result<Vec<u8>, String> {
    if entry.draft_digest != draft_digest {
        return Err("Rowvia draft request ID conflicts with changed intent".to_string());
    }
    let frozen = BASE64
        .decode(&entry.request_body_b64)
        .map_err(|_| "invalid Rowvia journal request")?;
    if hex::encode(Sha256::digest(&frozen)) != entry.request_digest {
        return Err("Rowvia journal request digest mismatch".to_string());
    }
    let prior: ManagementProposalRequest =
        serde_json::from_slice(&frozen).map_err(|_| "invalid Rowvia journal request")?;
    if prior.target_agent_pubkey != proposed.target_agent_pubkey
        || prior.binding_id != proposed.binding_id
        || serde_json::to_value(&prior.action).ok() != serde_json::to_value(&proposed.action).ok()
        || prior.operations != proposed.operations
    {
        return Err("Rowvia logical request conflicts with a frozen request".to_string());
    }
    if entry.refused || entry.approval_intent {
        return Err("Rowvia request has already been decided locally".to_string());
    }
    Ok(frozen)
}

fn unresolved_request(entry: &Entry) -> Result<(Vec<u8>, ManagementProposalRequest), String> {
    if entry.receipt_body_b64.is_some() || entry.refused || entry.approval_intent {
        return Err("Rowvia request is no longer unresolved".to_string());
    }
    let body = BASE64
        .decode(&entry.request_body_b64)
        .map_err(|_| "invalid Rowvia journal request")?;
    if body.len() > MAX_REQUEST_BYTES || hex::encode(Sha256::digest(&body)) != entry.request_digest
    {
        return Err("Rowvia journal request digest mismatch".to_string());
    }
    let request: ManagementProposalRequest =
        serde_json::from_slice(&body).map_err(|_| "invalid Rowvia journal request")?;
    if request.request_id != entry.request_id
        || request.operations != OPERATIONS.map(str::to_string).to_vec()
    {
        return Err("Rowvia journal request identity mismatch".to_string());
    }
    validate_id(&request.request_id)?;
    validate_id(&request.binding_id)?;
    validate_pubkey(&request.target_agent_pubkey)?;
    Ok((body, request))
}

async fn submit_frozen_proposal(
    client: &ManagementClient,
    keys: &Keys,
    path: &std::path::Path,
    scope: &Scope,
    receipt_handle: &str,
    frozen_body: Vec<u8>,
    check_scope: impl Fn() -> Result<(), String>,
) -> Result<NativeReceipt, String> {
    check_scope()?;
    let response_bytes = client
        .request_raw(
            keys,
            Method::POST,
            &["proposals"],
            Some(frozen_body.clone()),
        )
        .await?;
    check_scope()?;
    let proposal: ManagementProposalResponse =
        serde_json::from_slice(&response_bytes).map_err(|_| "invalid Rowvia proposal response")?;
    let request: ManagementProposalRequest =
        serde_json::from_slice(&frozen_body).map_err(|_| "invalid Rowvia journal request")?;
    verify_receipt_against_request(&proposal, &request, scope)?;
    let fresh = client.candidates(keys).await?;
    check_scope()?;
    if !fresh.candidates.iter().any(|candidate| {
        candidate.target_pubkey == request.target_agent_pubkey
            && candidate.binding_id == request.binding_id
            && candidate.binding_status == "active"
    }) {
        return Err("Rowvia agent binding changed before receipt save".to_string());
    }
    journal::transact(path, scope, |stored| {
        let entry = stored
            .entries
            .iter_mut()
            .find(|entry| entry.receipt_handle == receipt_handle)
            .ok_or("unknown Rowvia receipt")?;
        let (current_body, _) = unresolved_request(entry)?;
        if current_body != frozen_body {
            return Err("Rowvia journal request changed during retry".to_string());
        }
        entry.receipt_body_b64 = Some(BASE64.encode(&response_bytes));
        entry.receipt_digest = Some(hex::encode(Sha256::digest(&response_bytes)));
        entry.operation_id = Some(proposal.operation_id.clone());
        Ok(((), true))
    })?;
    Ok(NativeReceipt {
        receipt_handle: receipt_handle.to_string(),
        proposal,
    })
}

async fn admit_connector_event(
    app: &AppHandle,
    state: &AppState,
    client: &ManagementClient,
    scope: &Scope,
    keys: &Keys,
    event_json: &str,
    action: &ManagementAction,
    selected: &Candidate,
) -> Result<(String, String, String, bool), String> {
    if event_json.len() > 96 * 1024 {
        return Err("Cerberus event is too large".to_string());
    }
    let event = Event::from_json(event_json).map_err(|_| "invalid Cerberus event")?;
    if !event.verify_id()
        || !event.verify_signature()
        || event.kind.as_u16() != buzz_core_pkg::kind::KIND_AGENT_OBSERVER_FRAME as u16
        || event.pubkey.to_hex() != scope.cerberus_pubkey
    {
        return Err("Cerberus event signature or signer mismatch".to_string());
    }
    let age = Utc::now().timestamp() - event.created_at.as_secs() as i64;
    let stale = !(-30..=300).contains(&age);
    let wire: serde_json::Value =
        serde_json::from_str(&event.as_json()).map_err(|_| "invalid Cerberus event")?;
    let tags = wire["tags"]
        .as_array()
        .ok_or("invalid Cerberus event tags")?;
    let has_tag = |name: &str, value: &str| {
        tags.iter().any(|tag| {
            tag.as_array()
                .is_some_and(|items| items.len() >= 2 && items[0] == name && items[1] == value)
        })
    };
    if !has_tag("p", &scope.owner_pubkey)
        || !has_tag(
            buzz_core_pkg::observer::OBSERVER_AGENT_TAG,
            &scope.cerberus_pubkey,
        )
        || !has_tag(
            buzz_core_pkg::observer::OBSERVER_FRAME_TAG,
            buzz_core_pkg::observer::OBSERVER_FRAME_TELEMETRY,
        )
    {
        return Err("Cerberus event routing mismatch".to_string());
    }
    let frame: ConnectorFrame = buzz_core_pkg::observer::decrypt_observer_payload(keys, &event)
        .map_err(|_| "cannot decrypt Cerberus event")?;
    let payload = &frame.payload;
    let request = &payload.request;
    let action_value = match action {
        ManagementAction::ConnectorGrant => "connector.grant",
        ManagementAction::ConnectorRevoke => "connector.revoke",
    };
    let channel = request.channel_id.as_str();
    uuid::Uuid::parse_str(channel).map_err(|_| "invalid Cerberus channel")?;
    let request_id = payload.request_id.as_str();
    if uuid::Uuid::parse_str(request_id).is_err()
        || payload.request_type != "agent_management_request"
        || payload.version != 1
        || payload.action != action_value
        || frame.kind != "agent_management_request"
        || frame.channel_id.as_deref() != Some(channel)
        || request.target_name != selected.agent_name
        || frame.seq != 0
        || frame.agent_index.is_some()
        || frame.timestamp.len() > 64
        || frame
            .session_id
            .as_ref()
            .is_some_and(|value| value.len() > 128)
        || frame
            .turn_id
            .as_ref()
            .is_some_and(|value| value.len() > 128)
    {
        return Err("Cerberus connector draft mismatch".to_string());
    }
    if request.gmail_labels.as_ref().is_some_and(|labels| {
        labels.len() > 4 || !labels.iter().any(|label| label == &selected.gmail_label)
    }) {
        return Err("Cerberus Gmail label hint does not match selection".to_string());
    }
    assert_scope(app, state, client, scope)?;
    let events = query_relay_at_with_keys(
        state,
        &relay_http_base_url(&scope.relay_url),
        &[serde_json::json!({
            "kinds": [39002], "#d": [channel], "limit": 1
        })],
        keys,
        None,
    )
    .await?;
    assert_scope(app, state, client, scope)?;
    let roster = events
        .first()
        .ok_or("Cerberus channel roster unavailable")?;
    let relay_self = tokio::time::timeout(
        REQUEST_TIMEOUT,
        fetch_relay_self_at(state, &scope.relay_url),
    )
    .await
    .map_err(|_| "relay identity lookup timed out".to_string())??
    .ok_or("relay does not advertise a NIP-11 self key")?;
    assert_scope(app, state, client, scope)?;
    let roster_wire: serde_json::Value =
        serde_json::from_str(&roster.as_json()).map_err(|_| "invalid Cerberus channel roster")?;
    let correct_channel = roster_wire["tags"].as_array().is_some_and(|tags| {
        tags.iter().any(|tag| {
            tag.as_array()
                .is_some_and(|items| items.len() >= 2 && items[0] == "d" && items[1] == channel)
        })
    });
    if !roster_signer_matches_relay(roster, &relay_self)
        || !correct_channel
    {
        return Err("invalid Cerberus channel roster".to_string());
    }
    let members = nostr_convert::channel_members_from_event(roster)?;
    if !members
        .members
        .iter()
        .any(|member| member.pubkey == scope.owner_pubkey)
        || !members
            .members
            .iter()
            .any(|member| member.pubkey == scope.cerberus_pubkey)
    {
        return Err("owner and Cerberus are not current channel members".to_string());
    }
    let draft_bytes = serde_json::to_vec(payload).map_err(|_| "invalid Cerberus draft")?;
    Ok((
        request_id.to_string(),
        hex::encode(Sha256::digest(draft_bytes)),
        event.id.to_hex(),
        stale,
    ))
}

fn roster_signer_matches_relay(roster: &Event, relay_self: &str) -> bool {
    roster.verify_id()
        && roster.verify_signature()
        && roster.kind.as_u16() == 39002
        && roster.pubkey.to_hex().eq_ignore_ascii_case(relay_self)
}

#[tauri::command]
pub async fn rowvia_get_management_candidates(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CandidatesView, String> {
    let client = ManagementClient::configured()?;
    let (scope, keys) = capture_scope(&app, &state, &client)?;
    let response = client.candidates(&keys).await?;
    assert_scope(&app, &state, &client, &scope)?;
    let mut candidates = Vec::with_capacity(response.candidates.len());
    for candidate in response.candidates {
        if candidate
            .target_pubkey
            .eq_ignore_ascii_case(&scope.cerberus_pubkey)
        {
            continue;
        }
        candidates.push(Candidate {
            selection_handle: uuid::Uuid::new_v4().to_string(),
            agent_name: candidate.agent_name,
            target_pubkey: candidate.target_pubkey,
            binding_id: candidate.binding_id,
            gmail_label: candidate.gmail_label,
            binding_status: candidate.binding_status,
        });
    }
    let path = journal_path(&app, &scope)?;
    journal::transact(&path, &scope, |stored| {
        stored.candidates = candidates.clone();
        stored.candidates_at = Utc::now().timestamp();
        Ok(((), true))
    })?;
    Ok(CandidatesView {
        candidates: candidates
            .into_iter()
            .map(|candidate| CandidateView {
                selection_handle: candidate.selection_handle,
                agent_name: candidate.agent_name,
                gmail_label: candidate.gmail_label,
                binding_status: candidate.binding_status,
            })
            .collect(),
        truncated: response.truncated,
    })
}

#[tauri::command]
pub async fn rowvia_create_management_proposal(
    app: AppHandle,
    state: State<'_, AppState>,
    selection: CreateSelection,
) -> Result<NativeReceipt, String> {
    let client = ManagementClient::configured()?;
    let (scope, keys) = capture_scope(&app, &state, &client)?;
    validate_id(&selection.selection_handle)?;
    let path = journal_path(&app, &scope)?;
    let selected = journal::transact(&path, &scope, |stored| {
        if Utc::now().timestamp() - stored.candidates_at > CANDIDATE_TTL_SECONDS {
            return Err("Rowvia candidate selection is stale".to_string());
        }
        let candidate = stored
            .candidates
            .iter()
            .find(|candidate| candidate.selection_handle == selection.selection_handle)
            .ok_or("unknown Rowvia selection handle")?;
        Ok((candidate.clone(), false))
    })?;
    if selected
        .target_pubkey
        .eq_ignore_ascii_case(&scope.cerberus_pubkey)
    {
        return Err("Cerberus cannot be a management target".to_string());
    }
    let (logical_request_id, draft_digest, source_event_id, stale_event) = admit_connector_event(
        &app,
        &state,
        &client,
        &scope,
        &keys,
        &selection.relay_event_json,
        &selection.action,
        &selected,
    )
    .await?;
    let fresh = client.candidates(&keys).await?;
    assert_scope(&app, &state, &client, &scope)?;
    if !fresh.candidates.iter().any(|candidate| {
        candidate.target_pubkey == selected.target_pubkey
            && candidate.binding_id == selected.binding_id
            && candidate.agent_name == selected.agent_name
            && candidate.gmail_label == selected.gmail_label
            && candidate.binding_status == "active"
    }) {
        return Err("Rowvia candidate selection changed; refresh discovery".to_string());
    }
    let receipt_handle = uuid::Uuid::new_v4().to_string();
    let request = ManagementProposalRequest {
        request_id: uuid::Uuid::now_v7().to_string(),
        action: selection.action,
        target_agent_pubkey: selected.target_pubkey,
        binding_id: selected.binding_id,
        operations: OPERATIONS.map(str::to_string).to_vec(),
    };
    let body = serde_json::to_vec(&request).map_err(|_| "invalid Rowvia request")?;
    let digest = hex::encode(Sha256::digest(&body));
    let (stored_handle, frozen_body, existing_receipt) =
        journal::transact(&path, &scope, |stored| {
            if stored
                .retired
                .iter()
                .any(|entry| entry.logical_request_id == logical_request_id)
            {
                return Err("Rowvia draft request has already been retired".to_string());
            }
            if let Some(existing) = stored
                .entries
                .iter()
                .find(|entry| entry.logical_request_id == logical_request_id)
            {
                let frozen = frozen_replay_body(existing, &draft_digest, &request)?;
                return Ok((
                    (
                        existing.receipt_handle.clone(),
                        frozen,
                        existing.receipt_body_b64.clone(),
                    ),
                    false,
                ));
            }
            if stale_event {
                return Err("Cerberus connector event is stale".to_string());
            }
            stored.entries.push(Entry {
                created_at: Utc::now().timestamp(),
                logical_request_id,
                draft_digest,
                source_event_id,
                receipt_handle: receipt_handle.clone(),
                request_id: request.request_id.clone(),
                request_body_b64: BASE64.encode(&body),
                request_digest: digest,
                receipt_body_b64: None,
                receipt_digest: None,
                operation_id: None,
                approval_intent: false,
                approval_body_b64: None,
                approval_body_digest: None,
                refused: false,
                last_status_b64: None,
                last_status_digest: None,
            });
            Ok(((receipt_handle.clone(), body.clone(), None), true))
        })?;
    if existing_receipt.is_some() {
        let (proposal, _) = journal::transact(&path, &scope, |stored| {
            let entry = stored
                .entries
                .iter()
                .find(|entry| entry.receipt_handle == stored_handle)
                .ok_or("unknown Rowvia receipt")?;
            Ok((decode_entry_receipt(entry, &scope)?, false))
        })?;
        return Ok(NativeReceipt {
            receipt_handle: stored_handle,
            proposal,
        });
    }
    submit_frozen_proposal(
        &client,
        &keys,
        &path,
        &scope,
        &stored_handle,
        frozen_body,
        || assert_scope(&app, &state, &client, &scope),
    )
    .await
}

/// Retries one unresolved proposal with its journaled request ID and exact body.
#[tauri::command]
pub async fn rowvia_retry_management_proposal(
    app: AppHandle,
    state: State<'_, AppState>,
    receipt_handle: String,
) -> Result<NativeReceipt, String> {
    let client = ManagementClient::configured()?;
    let (scope, keys) = capture_scope(&app, &state, &client)?;
    validate_id(&receipt_handle)?;
    let path = journal_path(&app, &scope)?;
    let (body, request) = journal::transact(&path, &scope, |stored| {
        let entry = stored
            .entries
            .iter()
            .find(|entry| entry.receipt_handle == receipt_handle)
            .ok_or("unknown Rowvia receipt handle")?;
        Ok((unresolved_request(entry)?, false))
    })?;
    let fresh = client.candidates(&keys).await?;
    assert_scope(&app, &state, &client, &scope)?;
    if !fresh.candidates.iter().any(|candidate| {
        candidate.target_pubkey == request.target_agent_pubkey
            && candidate.binding_id == request.binding_id
            && candidate.binding_status == "active"
    }) {
        return Err("Rowvia agent binding changed; retry is unavailable".to_string());
    }
    submit_frozen_proposal(&client, &keys, &path, &scope, &receipt_handle, body, || {
        assert_scope(&app, &state, &client, &scope)
    })
    .await
}

#[tauri::command]
pub async fn rowvia_approve_management_proposal(
    app: AppHandle,
    state: State<'_, AppState>,
    receipt_handle: String,
) -> Result<serde_json::Value, String> {
    let client = ManagementClient::configured()?;
    let (scope, keys) = capture_scope(&app, &state, &client)?;
    validate_id(&receipt_handle)?;
    let path = journal_path(&app, &scope)?;
    assert_scope(&app, &state, &client, &scope)?;
    let (receipt, body) = journal::transact(&path, &scope, |stored| {
        let entry = stored
            .entries
            .iter_mut()
            .find(|entry| entry.receipt_handle == receipt_handle)
            .ok_or("unknown Rowvia receipt handle")?;
        if entry.refused {
            return Err("Rowvia proposal was refused locally".to_string());
        }
        if entry.approval_intent {
            return Err("Rowvia approval was already requested; check operation status".to_string());
        }
        let (receipt, expires_at) = decode_entry_receipt(entry, &scope)?;
        let body = approval_body(
            &ManagementApprovalRequest {
                proposal_id: receipt.proposal_id.clone(),
                operation_id: receipt.operation_id.clone(),
                proposal_digest: receipt.proposal_digest.clone(),
                proposal_bytes_b64: receipt.proposal_bytes_b64.clone(),
                expires_at,
            },
            true,
        )?;
        entry.approval_intent = true;
        entry.approval_body_b64 = Some(BASE64.encode(&body));
        entry.approval_body_digest = Some(hex::encode(Sha256::digest(&body)));
        Ok(((receipt, body), true))
    })?;
    assert_scope(&app, &state, &client, &scope)?;
    let result = client
        .post_approval_body(&keys, &receipt.proposal_id, body)
        .await?;
    assert_scope(&app, &state, &client, &scope)?;
    Ok(result)
}

/// Reconcile first, then explicitly retry the exact saved approval with a
/// newly signed NIP-98 proof only if Rowvia still reports it as proposed.
#[tauri::command]
pub async fn rowvia_retry_management_approval(
    app: AppHandle,
    state: State<'_, AppState>,
    receipt_handle: String,
) -> Result<serde_json::Value, String> {
    let client = ManagementClient::configured()?;
    let (scope, keys) = capture_scope(&app, &state, &client)?;
    validate_id(&receipt_handle)?;
    let path = journal_path(&app, &scope)?;
    retry_frozen_approval(&client, &keys, &path, &scope, &receipt_handle, || {
        assert_scope(&app, &state, &client, &scope)
    })
    .await
}

async fn retry_frozen_approval(
    client: &ManagementClient,
    keys: &Keys,
    path: &std::path::Path,
    scope: &Scope,
    receipt_handle: &str,
    check_scope: impl Fn() -> Result<(), String>,
) -> Result<serde_json::Value, String> {
    let (receipt, body) = journal::transact(&path, &scope, |stored| {
        let entry = stored
            .entries
            .iter()
            .find(|entry| entry.receipt_handle == receipt_handle)
            .ok_or("unknown Rowvia receipt handle")?;
        Ok((frozen_approval_body(entry, &scope)?, false))
    })?;
    let status = fetch_and_store_operation(client, keys, path, scope, &receipt.operation_id, &check_scope)
        .await?;
    match status["state"].as_str() {
        Some("proposed") => {}
        Some("accepted" | "running" | "succeeded" | "failed" | "expired") => {
            return Ok(status);
        }
        _ => return Err("invalid Rowvia operation state".to_string()),
    }
    // A stale proposal cannot be newly approved; keep the intent for status
    // reconciliation and require a new proposal for a different decision.
    let (_, expires_at) = journal::transact(&path, &scope, |stored| {
        let entry = stored
            .entries
            .iter()
            .find(|entry| entry.receipt_handle == receipt_handle)
            .ok_or("unknown Rowvia receipt handle")?;
        Ok((decode_entry_receipt_with_freshness(entry, &scope, false)?, false))
    })?;
    if DateTime::parse_from_rfc3339(&expires_at)
        .map_err(|_| "invalid Rowvia proposal expiry")?
        <= Utc::now()
    {
        return Err("Rowvia proposal has expired".to_string());
    }
    check_scope()?;
    let result = client
        .post_approval_body(keys, &receipt.proposal_id, body)
        .await?;
    check_scope()?;
    Ok(result)
}

#[tauri::command]
pub fn rowvia_list_management_pending(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<PendingReceipts, String> {
    let client = ManagementClient::configured()?;
    let (scope, _) = capture_scope(&app, &state, &client)?;
    let path = journal_path(&app, &scope)?;
    let pending = pending_from_journal(&path, &scope)?;
    assert_scope(&app, &state, &client, &scope)?;
    Ok(pending)
}

#[tauri::command]
pub fn rowvia_list_management_operations(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<OperationRecovery>, String> {
    let client = ManagementClient::configured()?;
    let (scope, _) = capture_scope(&app, &state, &client)?;
    let path = journal_path(&app, &scope)?;
    let operations = journal::transact(&path, &scope, |stored| {
        let mut operations = Vec::new();
        for entry in &stored.entries {
            let Some(operation_id) = &entry.operation_id else {
                continue;
            };
            validate_id(operation_id)?;
            let last_status = match (&entry.last_status_b64, &entry.last_status_digest) {
                (None, None) => None,
                (Some(encoded), Some(digest)) => {
                    let bytes = BASE64
                        .decode(encoded)
                        .map_err(|_| "invalid Rowvia journal status")?;
                    if hex::encode(Sha256::digest(&bytes)) != *digest {
                        return Err("Rowvia journal status digest mismatch".to_string());
                    }
                    let status: serde_json::Value = serde_json::from_slice(&bytes)
                        .map_err(|_| "invalid Rowvia journal status")?;
                    if status["operation_id"] != *operation_id {
                        return Err("Rowvia journal status identity mismatch".to_string());
                    }
                    Some(status)
                }
                _ => return Err("incomplete Rowvia journal status".to_string()),
            };
            operations.push(OperationRecovery {
                receipt_handle: entry.receipt_handle.clone(),
                operation_id: operation_id.clone(),
                approval_intent: entry.approval_intent,
                refused: entry.refused,
                last_status,
            });
        }
        Ok((operations, false))
    })?;
    assert_scope(&app, &state, &client, &scope)?;
    Ok(operations)
}

#[tauri::command]
pub fn rowvia_reject_management_local(
    app: AppHandle,
    state: State<'_, AppState>,
    receipt_handle: String,
) -> Result<(), String> {
    let client = ManagementClient::configured()?;
    let (scope, _) = capture_scope(&app, &state, &client)?;
    validate_id(&receipt_handle)?;
    let path = journal_path(&app, &scope)?;
    journal::transact(&path, &scope, |stored| {
        let entry = stored
            .entries
            .iter_mut()
            .find(|entry| entry.receipt_handle == receipt_handle)
            .ok_or("unknown Rowvia receipt handle")?;
        if entry.approval_intent {
            return Err("Rowvia approval has already been requested".to_string());
        }
        entry.refused = true;
        Ok(((), true))
    })?;
    assert_scope(&app, &state, &client, &scope)
}

#[tauri::command]
pub async fn rowvia_get_management_operation(
    app: AppHandle,
    state: State<'_, AppState>,
    operation_id: String,
) -> Result<serde_json::Value, String> {
    let client = ManagementClient::configured()?;
    let (scope, keys) = capture_scope(&app, &state, &client)?;
    validate_id(&operation_id)?;
    let path = journal_path(&app, &scope)?;
    fetch_and_store_operation(&client, &keys, &path, &scope, &operation_id, || {
        assert_scope(&app, &state, &client, &scope)
    })
    .await
}

async fn fetch_and_store_operation(
    client: &ManagementClient,
    keys: &Keys,
    path: &std::path::Path,
    scope: &Scope,
    operation_id: &str,
    check_scope: impl Fn() -> Result<(), String>,
) -> Result<serde_json::Value, String> {
    journal::transact(path, scope, |stored| {
        let entry = stored
            .entries
            .iter()
            .find(|entry| entry.operation_id.as_deref() == Some(operation_id))
            .ok_or("unknown Rowvia operation")?;
        // A prior approval may remain unconfirmed after the proposal expires.
        // Expiry prevents a new decision, but must not hide operation status.
        decode_entry_receipt_with_freshness(entry, scope, false)?;
        Ok(((), false))
    })?;
    let bytes = client
        .request_raw(keys, Method::GET, &["operations", operation_id], None)
        .await?;
    check_scope()?;
    let status: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "invalid Rowvia operation status")?;
    if status["operation_id"] != operation_id {
        return Err("Rowvia operation status identity mismatch".to_string());
    }
    journal::transact(path, scope, |stored| {
        let entry = stored
            .entries
            .iter_mut()
            .find(|entry| entry.operation_id.as_deref() == Some(operation_id))
            .ok_or("unknown Rowvia operation")?;
        entry.last_status_b64 = Some(BASE64.encode(&bytes));
        entry.last_status_digest = Some(hex::encode(Sha256::digest(&bytes)));
        Ok(((), true))
    })?;
    Ok(status)
}

#[cfg(test)]
#[path = "rowvia_management_tests.rs"]
mod tests;
