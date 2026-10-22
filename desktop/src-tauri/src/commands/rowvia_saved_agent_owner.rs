//! Fixed Save and operation-only owner review/approval. Never starts a runtime.
use super::rowvia_saved_agent::{
    self as native, PreparedRowviaSavedAgent, RowviaSavedAgentScopeExpectation,
    VerifiedRowviaSavedAgentPreparation,
};
use crate::managed_agents::SavedAgentPreparationAck;
use chrono::{DateTime, Utc};
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::fmt;
use tauri::{AppHandle, Manager};

const UNAVAILABLE: &str = "saved_agent_owner_unavailable";
const UNKNOWN: &str = "saved_agent_authorization_unknown";
#[cfg(target_os = "linux")]
use super::rowvia_saved_agent_registration::prepare_caller::{
    self, production::ProductionOwnerBoundary,
};

#[path = "rowvia_saved_agent_owner_public.rs"]
mod public;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Failure {
    Unavailable,
    Unknown,
}
impl Failure {
    fn public(&self) -> String {
        match self {
            Self::Unavailable => UNAVAILABLE,
            Self::Unknown => UNKNOWN,
        }
        .into()
    }
}

fn operation(value: &str) -> Result<(), Failure> {
    let id = uuid::Uuid::parse_str(value).map_err(|_| Failure::Unavailable)?;
    if id.get_version_num() != 7
        || id.get_variant() != uuid::Variant::RFC4122
        || id.to_string() != value
    {
        return Err(Failure::Unavailable);
    }
    Ok(())
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && value.bytes().any(|b| b != b'0')
}

/// Exactly one owner-selected field; never an expectation, ACK, hash or origin.
pub struct SavedAgentOwnerInput {
    operation_id: String,
}
impl<'de> Deserialize<'de> for SavedAgentOwnerInput {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct InputVisitor;
        impl<'de> Visitor<'de> for InputVisitor {
            type Value = SavedAgentOwnerInput;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("operation-only object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut id = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key != "operationId" || id.is_some() {
                        return Err(de::Error::custom("invalid operation input"));
                    }
                    id = Some(map.next_value::<String>()?);
                }
                let operation_id =
                    id.ok_or_else(|| de::Error::custom("invalid operation input"))?;
                operation(&operation_id)
                    .map_err(|_| de::Error::custom("invalid operation input"))?;
                Ok(SavedAgentOwnerInput { operation_id })
            }
        }
        deserializer.deserialize_map(InputVisitor)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SavedAgentReview {
    version: u8,
    operation_id: String,
    agent_pubkey: String,
    owner_pubkey: String,
    relay_url: String,
    source_instance: String,
    name: String,
    system_prompt: String,
    channel_id: String,
    recipe_id: String,
    model: String,
    recipe_sha256: String,
    canonical21_sha256: String,
    canonical21: String,
    created_stopped: bool,
    bindings_ready: bool,
    runtime_ready: bool,
}

pub(super) struct Resumed {
    pub(super) expected: RowviaSavedAgentScopeExpectation,
    pub(super) prepared: PreparedRowviaSavedAgent,
    pub(super) ack: SavedAgentPreparationAck,
}
impl Resumed {
    fn from_verified(value: VerifiedRowviaSavedAgentPreparation) -> Self {
        let (expected, prepared, ack) = value.into_parts();
        Self {
            expected,
            prepared,
            ack,
        }
    }
    fn project(&self, requested: &str) -> Result<SavedAgentReview, Failure> {
        let e = &self.expected;
        let s = &e.selection;
        let p = &self.prepared;
        let a = &self.ack;
        let recipe = serde_json::to_value(s.recipe).map_err(|_| Failure::Unavailable)?;
        let recipe = recipe.as_str().ok_or(Failure::Unavailable)?;
        if s.operation_id != requested
            || p.operation_id != requested
            || a.operation_id != requested
            || a.version != 1
            || a.agent_pubkey != e.agent_pubkey
            || p.pubkey != e.agent_pubkey
            || a.native_config_sha256 != e.native_config_sha256
            || p.native_config_sha256 != e.native_config_sha256
            || a.saved_authorization_sha256 != e.saved_authorization_sha256
            || p.saved_authorization_sha256 != e.saved_authorization_sha256
            || p.core_agent_name != format!("saved-{}", requested.replace('-', ""))
            || !p.created_stopped
            || p.bindings_ready
            || p.runtime_ready
            || !["codex-local-v1", "codex-development-memory-git-read-v1"].contains(&recipe)
            || s.model != "gpt-6.1-sol"
            || ![
                &a.agent_pubkey,
                &a.native_config_sha256,
                &a.saved_authorization_sha256,
                &a.recipe_sha256,
                &a.request_sha256,
                &a.canonical21_sha256,
                &a.prepared_receipt_sha256,
                &s.owner_pubkey,
            ]
            .iter()
            .all(|v| hash(v))
        {
            return Err(Failure::Unavailable);
        }
        Ok(SavedAgentReview {
            version: 1,
            operation_id: requested.into(),
            agent_pubkey: e.agent_pubkey.clone(),
            owner_pubkey: s.owner_pubkey.clone(),
            relay_url: s.relay_url.clone(),
            source_instance: s.source_instance.clone(),
            name: s.name.clone(),
            system_prompt: s.system_prompt.clone(),
            channel_id: s.channel_id.clone(),
            recipe_id: recipe.into(),
            model: s.model.clone(),
            recipe_sha256: a.recipe_sha256.clone(),
            canonical21_sha256: a.canonical21_sha256.clone(),
            canonical21: String::new(), // populated only after trusted host verification
            created_stopped: true,
            bindings_ready: false,
            runtime_ready: false,
        })
    }
}

/// One complete trusted boundary packet. No partial/default production fakes.
/// host_review must validate Main's exact retained recipe/request/canonical21/
/// receipt and return the regenerated existing ACK, not merely echo this ACK.
pub(super) struct HostReviewedPreparation {
    pub version: u8,
    pub operation_id: String,
    pub canonical21: String,
    pub canonical21_sha256: String,
    pub ack: SavedAgentPreparationAck,
}
impl<'de> Deserialize<'de> for HostReviewedPreparation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ReviewedVisitor;
        impl<'de> Visitor<'de> for ReviewedVisitor {
            type Value = HostReviewedPreparation;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("closed host review object")
            }
            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields {
                    version: u8,
                    operation_id: String,
                    canonical21: String,
                    canonical21_sha256: String,
                    ack: SavedAgentPreparationAck,
                }
                let f = Fields::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(HostReviewedPreparation {
                    version: f.version,
                    operation_id: f.operation_id,
                    canonical21: f.canonical21,
                    canonical21_sha256: f.canonical21_sha256,
                    ack: f.ack,
                })
            }
        }
        deserializer.deserialize_map(ReviewedVisitor)
    }
}

pub(super) trait OwnerBoundary: Send {
    fn preflight(&mut self) -> Result<(), Failure>;
    fn resume(&mut self, operation_id: &str) -> Result<Resumed, Failure>;
    fn host_review(
        &mut self,
        resumed: &Resumed,
    ) -> impl std::future::Future<Output = Result<HostReviewedPreparation, Failure>> + Send;
    fn revalidate(&mut self, resumed: &Resumed) -> Result<(), Failure>;
    // Explicit approval only: capture the current owner signer and POST exactly
    // these bytes using the fixed ManagementClient; never retry or expose keys.
    fn authorize(
        &mut self,
        resumed: &Resumed,
        body: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<Vec<u8>, Failure>> + Send;
    fn now(&self) -> DateTime<Utc>;
}
async fn reviewed<B: OwnerBoundary>(
    boundary: &mut B,
    id: &str,
) -> Result<(Resumed, SavedAgentReview), Failure> {
    operation(id)?;
    boundary.preflight()?;
    let resumed = boundary.resume(id)?;
    let mut view = resumed.project(id)?;
    let host = boundary
        .host_review(&resumed)
        .await
        .map_err(|_| Failure::Unavailable)?;
    if host.version != 1 || host.operation_id != id || host.ack != resumed.ack {
        return Err(Failure::Unavailable);
    }
    public::verify_canonical21(&host.canonical21, &host.canonical21_sha256, &resumed)?;
    view.canonical21 = host.canonical21;
    boundary.preflight()?;
    boundary.revalidate(&resumed)?;
    Ok((resumed, view))
}
pub(super) async fn review_with<B: OwnerBoundary>(
    boundary: &mut B,
    id: &str,
) -> Result<SavedAgentReview, Failure> {
    let (_, view) = reviewed(boundary, id).await?;
    Ok(view)
}

#[derive(Serialize)]
struct AuthorizationBody<'a> {
    version: u8,
    operation_id: &'a str,
    agent_pubkey: &'a str,
    harness_kind: &'static str,
    config_digest: &'a str,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SavedAgentAuthorization {
    pub(super) version: u8,
    pub(super) operation_id: String,
    pub(super) state: String,
    pub(super) expires_at: DateTime<Utc>,
}

/// Public Save selection only. Paths/model/pins belong to the fixed supplier.
pub struct SavedAgentSaveInput {
    display_name: String,
    system_prompt: String,
    channel_id: String,
    recipe: crate::managed_agents::SavedAgentRecipe,
}
impl<'de> Deserialize<'de> for SavedAgentSaveInput {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct SaveVisitor;
        impl<'de> Visitor<'de> for SaveVisitor {
            type Value = SavedAgentSaveInput;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("closed saved Codex selection")
            }
            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase", deny_unknown_fields)]
                struct Selection {
                    display_name: String,
                    system_prompt: String,
                    channel_id: String,
                    recipe: crate::managed_agents::SavedAgentRecipe,
                }
                let value = Selection::deserialize(de::value::MapAccessDeserializer::new(map))?;
                let channel =
                    uuid::Uuid::parse_str(&value.channel_id).map_err(de::Error::custom)?;
                if value.display_name.is_empty()
                    || value.display_name.len() > 128
                    || value.display_name.trim() != value.display_name
                    || value.display_name.chars().any(char::is_control)
                    || value.system_prompt.len() > 4096
                    || value.system_prompt.contains('\0')
                    || channel.is_nil()
                    || channel.get_variant() != uuid::Variant::RFC4122
                    || channel.to_string() != value.channel_id
                    || value.recipe
                        != crate::managed_agents::SavedAgentRecipe::CodexDevelopmentMemoryGitReadV1
                {
                    return Err(de::Error::custom("invalid saved Codex selection"));
                }
                // This selection cannot supply paths/model/authority or become
                // a generic persona form.
                Ok(SavedAgentSaveInput {
                    display_name: value.display_name,
                    system_prompt: value.system_prompt,
                    channel_id: value.channel_id,
                    recipe: value.recipe,
                })
            }
        }
        deserializer.deserialize_map(SaveVisitor)
    }
}

/// Operation selector returned after the fixed C2 Save/preparation completes.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedAgentSaveResult {
    version: u8,
    operation_id: String,
}

/// Public completion, intentionally neither Deserialize nor externally
/// constructible. A success-shaped child response cannot create this value.
/// Constructed only AFTER trusted host final-C2 and the consuming Native CAS.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedAgentRegistrationCompletion {
    version: u8,
    operation_id: String,
    agent_pubkey: String,
    canonical21: String,
    canonical21_sha256: String,
    state: &'static str,
    bindings_ready: bool,
    runtime_ready: bool,
}

/// Authorization alone never grants completed registration or a Start action.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SavedAgentApproval {
    Authorization {
        authorization: SavedAgentAuthorization,
    },
    RegisteredStopped {
        completion: SavedAgentRegistrationCompletion,
    },
}
impl<'de> Deserialize<'de> for SavedAgentAuthorization {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ResultVisitor;
        impl<'de> Visitor<'de> for ResultVisitor {
            type Value = SavedAgentAuthorization;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("saved authorization object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let (mut version, mut operation, mut state, mut expiry) = (None, None, None, None);
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "version" if version.is_none() => version = Some(map.next_value::<u8>()?),
                        "operation_id" if operation.is_none() => {
                            operation = Some(map.next_value::<String>()?)
                        }
                        "state" if state.is_none() => state = Some(map.next_value::<String>()?),
                        "expires_at" if expiry.is_none() => {
                            expiry = Some(map.next_value::<DateTime<Utc>>()?)
                        }
                        _ => return Err(de::Error::custom("invalid saved authorization")),
                    }
                }
                Ok(SavedAgentAuthorization {
                    version: version
                        .ok_or_else(|| de::Error::custom("invalid saved authorization"))?,
                    operation_id: operation
                        .ok_or_else(|| de::Error::custom("invalid saved authorization"))?,
                    state: state.ok_or_else(|| de::Error::custom("invalid saved authorization"))?,
                    expires_at: expiry
                        .ok_or_else(|| de::Error::custom("invalid saved authorization"))?,
                })
            }
        }
        deserializer.deserialize_map(ResultVisitor)
    }
}
pub(super) async fn approve_with<B: OwnerBoundary>(
    boundary: &mut B,
    id: &str,
) -> Result<SavedAgentAuthorization, Failure> {
    // A separate explicit command runs the full durable review again. A prior
    // UI review object/hash is not accepted as input or signing authority.
    let (resumed, view) = reviewed(boundary, id).await?;
    authorize_reviewed(boundary, &resumed, &view).await
}

async fn authorize_reviewed<B: OwnerBoundary>(
    boundary: &mut B,
    resumed: &Resumed,
    view: &SavedAgentReview,
) -> Result<SavedAgentAuthorization, Failure> {
    let id = resumed.expected.selection.operation_id.as_str();
    let body = serde_json::to_vec(&AuthorizationBody {
        version: 1,
        operation_id: id,
        agent_pubkey: &view.agent_pubkey,
        harness_kind: "custom-acp",
        config_digest: &view.canonical21_sha256,
    })
    .map_err(|_| Failure::Unavailable)?;
    boundary.preflight()?;
    boundary.revalidate(resumed)?;
    // Every failure after handing bytes to the signing/HTTP boundary is unknown.
    // No raw transport/JSON diagnostics, status inference or retry escapes.
    let bytes = boundary
        .authorize(resumed, body)
        .await
        .map_err(|_| Failure::Unknown)?;
    if bytes.len() > 2048 {
        return Err(Failure::Unknown);
    }
    let result: SavedAgentAuthorization =
        serde_json::from_slice(&bytes).map_err(|_| Failure::Unknown)?;
    if result.version != 1
        || result.operation_id != id
        || !["authorized", "expired", "revoked"].contains(&result.state.as_str())
        || (result.state == "authorized" && result.expires_at <= boundary.now())
    {
        return Err(Failure::Unknown);
    }
    boundary.preflight().map_err(|_| Failure::Unknown)?;
    boundary.revalidate(resumed).map_err(|_| Failure::Unknown)?;
    Ok(result)
}

// The owner POST may already be durable. Every subsequent continuation error,
// including a read-only expiry/resume/cohort failure before child dispatch,
// remains unknown. FnOnce excludes a retry in this effect boundary.
fn after_owner_authorization<T>(
    continuation: impl FnOnce() -> Result<T, Failure>,
) -> Result<T, Failure> {
    continuation().map_err(|_| Failure::Unknown)
}

// Concrete existing Native adapters: no snapshot/key hydration or new store.
pub(super) fn resume_native<R: tauri::Runtime>(
    app: &AppHandle<R>,
    id: &str,
) -> Result<Resumed, Failure> {
    native::resume_rowvia_saved_agent_preparation(app, id)
        .map(Resumed::from_verified)
        .map_err(|_| Failure::Unavailable)
}
pub(super) fn revalidate_native<R: tauri::Runtime>(
    app: &AppHandle<R>,
    resumed: &Resumed,
) -> Result<(), Failure> {
    let checked = native::revalidate_resumed_rowvia_saved_agent_preparation_ack(
        app,
        &resumed.expected,
        &resumed.ack,
    )
    .map_err(|_| Failure::Unavailable)?;
    if !checked.confirmed || checked.ack != resumed.ack {
        return Err(Failure::Unavailable);
    }
    Ok(())
}

/// Real signer/transport adapter for the explicit approval continuation only.
/// Not called by review or externally exported. The fixed installation must
/// qualify first. No key hydration, journal or secret output.
pub(super) async fn authorize_native<R: tauri::Runtime>(
    app: &AppHandle<R>,
    resumed: &Resumed,
    body: Vec<u8>,
) -> Result<Vec<u8>, Failure> {
    let id = &resumed.expected.selection.operation_id;
    let view = resumed.project(id)?;
    let exact_body = serde_json::to_vec(&AuthorizationBody {
        version: 1,
        operation_id: id,
        agent_pubkey: &view.agent_pubkey,
        harness_kind: "custom-acp",
        config_digest: &view.canonical21_sha256,
    })
    .map_err(|_| Failure::Unavailable)?;
    if body != exact_body {
        return Err(Failure::Unavailable);
    }
    revalidate_native(app, resumed)?;
    let state = app.state::<crate::app_state::AppState>();
    let keys = {
        let _workspace = state
            .workspace_apply_lock
            .try_lock()
            .map_err(|_| Failure::Unavailable)?;
        let _identity = state
            .identity_mutation
            .lock()
            .map_err(|_| Failure::Unavailable)?;
        if state
            .reset_failed
            .load(std::sync::atomic::Ordering::Acquire)
            || state
                .shutdown_started
                .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(Failure::Unavailable);
        }
        let keys = state
            .signing_keys()
            .map_err(|_| Failure::Unavailable)?
            .clone();
        if keys.public_key().to_hex() != view.owner_pubkey {
            return Err(Failure::Unavailable);
        }
        crate::relay::bind_expected_relay_scope(
            Some(&resumed.expected.selection.relay_url),
            crate::relay::relay_ws_url_with_override(&state),
        )
        .map_err(|_| Failure::Unavailable)?;
        keys
    }; // Never hold identity/workspace/store locks over network IO.
    revalidate_native(app, resumed)?;
    let bytes = crate::commands::rowvia_management::post_saved_agent_authorization(
        &keys,
        &view.owner_pubkey,
        &view.source_instance,
        body,
    )
    .await
    .map_err(|_| Failure::Unknown)?;
    revalidate_native(app, resumed).map_err(|_| Failure::Unknown)?;
    Ok(bytes)
}
#[tauri::command]
pub async fn rowvia_review_saved_agent_preparation(
    app: AppHandle,
    input: SavedAgentOwnerInput,
) -> Result<SavedAgentReview, String> {
    operation(&input.operation_id).map_err(|e| e.public())?;
    #[cfg(target_os = "linux")]
    {
        return review_with(
            &mut ProductionOwnerBoundary::new(&app).map_err(|e| e.public())?,
            &input.operation_id,
        )
        .await
        .map_err(|e| e.public());
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = app;
        Err(Failure::Unavailable.public())
    }
}
#[tauri::command]
pub async fn rowvia_authorize_saved_agent(
    app: AppHandle,
    input: SavedAgentOwnerInput,
) -> Result<SavedAgentAuthorization, String> {
    operation(&input.operation_id).map_err(|e| e.public())?;
    #[cfg(target_os = "linux")]
    {
        return approve_with(
            &mut ProductionOwnerBoundary::new(&app).map_err(|e| e.public())?,
            &input.operation_id,
        )
        .await
        .map_err(|e| e.public());
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = app;
        Err(Failure::Unavailable.public())
    }
}

/// Fixed saved-Codex entry; missing qualification refuses before Native Save.
#[tauri::command]
pub async fn rowvia_save_codex_agent(
    app: AppHandle,
    input: SavedAgentSaveInput,
) -> Result<SavedAgentSaveResult, String> {
    #[cfg(target_os = "linux")]
    {
        let request = prepare_caller::OwnerSaveRequest::from_selection(
            input.display_name,
            input.system_prompt,
            input.channel_id,
            input.recipe,
        );
        let result = prepare_caller::save_rowvia_saved_agent_preparation(&app, request).map_err(
            |failure| match failure {
                prepare_caller::PrepareFailure::Unavailable => Failure::Unavailable.public(),
                prepare_caller::PrepareFailure::Unknown => Failure::Unknown.public(),
            },
        )?;
        Ok(SavedAgentSaveResult {
            version: 1,
            operation_id: result.operation_id().into(),
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app, input);
        Err(Failure::Unavailable.public())
    }
}

/// Explicit single approval continuation through the qualified fixed host and
/// private CAS completion. Missing deployment pins remain unavailable.
#[tauri::command]
pub async fn rowvia_approve_saved_agent_registration(
    app: AppHandle,
    input: SavedAgentOwnerInput,
) -> Result<SavedAgentApproval, String> {
    operation(&input.operation_id).map_err(|e| e.public())?;
    #[cfg(target_os = "linux")]
    {
        let mut boundary = ProductionOwnerBoundary::new(&app).map_err(|e| e.public())?;
        let (resumed, view) = reviewed(&mut boundary, &input.operation_id)
            .await
            .map_err(|e| e.public())?;
        // Exactly one explicit owner POST. The register child's private wrapper
        // consumes this result; it cannot invoke a second signer or UI callback.
        let authorization = authorize_reviewed(&mut boundary, &resumed, &view)
            .await
            .map_err(|e| e.public())?;
        if authorization.state != "authorized" {
            return Ok(SavedAgentApproval::Authorization { authorization });
        }
        after_owner_authorization(|| {
            let completion = boundary.register(&resumed, &authorization)?;
            native::finalize_rowvia_saved_agent_registration(&app, completion)
                .map_err(|_| Failure::Unknown)
        })
        .map_err(|e| e.public())?;
        Ok(SavedAgentApproval::RegisteredStopped {
            completion: SavedAgentRegistrationCompletion {
                version: 1,
                operation_id: input.operation_id,
                agent_pubkey: view.agent_pubkey,
                canonical21: view.canonical21,
                canonical21_sha256: view.canonical21_sha256,
                state: "registered-stopped",
                bindings_ready: true,
                runtime_ready: false,
            },
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = app;
        Err(Failure::Unavailable.public())
    }
}

#[cfg(test)]
#[path = "rowvia_saved_agent_owner_tests.rs"]
mod tests;
