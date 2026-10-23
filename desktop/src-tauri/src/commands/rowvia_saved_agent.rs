//! Quiet native preparation; deliberately has no Tauri command registration.

use super::{managed_agent_auth_tag, mint_managed_agent_identity};
use crate::{app_state::AppState, managed_agents::*};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};
use tauri::{AppHandle, Manager};

/// Trusted host selection, not an IPC/model request or Core registration approval.
#[derive(Clone)]
pub struct RowviaSavedAgentPreparation {
    pub operation_id: String,
    pub owner_pubkey: String,
    pub relay_url: String,
    pub source_instance: String,
    pub channel_id: String,
    pub name: String,
    pub system_prompt: String,
    pub recipe: SavedAgentRecipe,
    pub model: String,
    pub workspace: String,
    pub capsule_run: String,
}

/// Public metadata only; preparation establishes neither Core nor runtime readiness.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreparedRowviaSavedAgent {
    pub operation_id: String,
    pub pubkey: String,
    pub core_agent_name: String,
    pub saved_authorization_sha256: String,
    pub native_config_sha256: String,
    pub created_stopped: bool,
    pub bindings_ready: bool,
    pub runtime_ready: bool,
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn canonical_path(value: &str) -> bool {
    let path = Path::new(value);
    value.len() <= 4096
        && path.is_absolute()
        && !value.chars().any(char::is_control)
        && !path
            .components()
            .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
        && path.components().collect::<PathBuf>().to_str() == Some(value)
}

fn validate(input: &RowviaSavedAgentPreparation) -> Result<(), String> {
    let operation =
        uuid::Uuid::parse_str(&input.operation_id).map_err(|_| "invalid preparation")?;
    let channel = uuid::Uuid::parse_str(&input.channel_id).map_err(|_| "invalid preparation")?;
    if operation.get_version_num() != 7
        || operation.get_variant() != uuid::Variant::RFC4122
        || operation.to_string() != input.operation_id
        || channel.is_nil()
        || channel.get_variant() != uuid::Variant::RFC4122
        || channel.to_string() != input.channel_id
        || input.owner_pubkey.len() != 64
        || !input
            .owner_pubkey
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || input.name.trim() != input.name
        || input.source_instance.is_empty()
        || input.source_instance.len() > 64
        || !input
            .source_instance
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
        || !input.source_instance.as_bytes()[0].is_ascii_alphanumeric()
        || input.model.is_empty()
        || input.model.len() > 128
        || !input
            .model
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
        || !canonical_path(&input.workspace)
        || Path::new(&input.workspace)
            .file_name()
            .and_then(|s| s.to_str())
            != Some(input.operation_id.as_str())
        || !canonical_path(&input.capsule_run)
        || !input.capsule_run.ends_with("/run")
        || !Path::new(&input.capsule_run)
            .components()
            .any(|p| p.as_os_str() == input.operation_id.as_str())
    {
        return Err("invalid preparation".into());
    }
    validate_agent_definition_text(&input.name, &input.system_prompt)?;
    let relay = buzz_core_pkg::relay::normalize_relay_url(&input.relay_url)
        .map_err(|_| "invalid preparation relay")?;
    if relay != input.relay_url {
        return Err("invalid preparation relay".into());
    }
    Ok(())
}

fn check_scope(state: &AppState, input: &RowviaSavedAgentPreparation) -> Result<(), String> {
    if state
        .shutdown_started
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return Err("desktop shutdown has started".into());
    }
    let owner = state.signing_keys()?.public_key().to_hex();
    if owner != input.owner_pubkey {
        return Err("prepared owner changed".into());
    }
    crate::relay::bind_expected_relay_scope(
        Some(&input.relay_url),
        crate::relay::relay_ws_url_with_override(state),
    )?;
    Ok(())
}

fn config_digest(
    record: &ManagedAgentRecord,
    pending: &PendingSavedAgentRegistration,
) -> Result<String, String> {
    #[derive(Serialize)]
    struct Selection<'a> {
        operation_id: &'a str,
        owner_pubkey: &'a str,
        relay_url: &'a str,
        source_instance: &'a str,
        channel_id: &'a str,
        workspace: &'a str,
        capsule_run: &'a str,
        recipe: &'a SavedAgentRecipe,
        model: &'a str,
        saved_authorization_sha256: &'a str,
    }
    #[derive(Serialize)]
    struct Native<'a> {
        pubkey: &'a str,
        name: &'a str,
        acp_command: &'a str,
        agent_command: &'a str,
        agent_command_override: Option<&'a str>,
        agent_args: &'a [String],
        mcp_command: &'a str,
        persona_id: Option<&'a str>,
        team_id: Option<&'a str>,
        system_prompt: Option<&'a str>,
        provider: Option<&'a str>,
        model: Option<&'a str>,
        env_vars: &'a BTreeMap<String, String>,
        parallelism: u32,
        session_policy: &'a AcpSessionPolicy,
        respond_to: &'a RespondTo,
        respond_to_allowlist: &'a [String],
        start_on_app_launch: bool,
        auto_restart_on_config_change: bool,
        backend: &'a BackendKind,
        relay_url: &'a str,
        idle_timeout_seconds: Option<u64>,
        max_turn_duration_seconds: Option<u64>,
        effort_level: Option<&'a str>,
    }
    #[derive(Serialize)]
    struct Frozen<'a> {
        version: &'static str,
        selection: Selection<'a>,
        native: Native<'a>,
    }
    let frozen = Frozen {
        version: "rowvia.native-saved-agent-preparation/v1",
        selection: Selection {
            operation_id: &pending.operation_id,
            owner_pubkey: &pending.owner_pubkey,
            relay_url: &pending.relay_url,
            source_instance: &pending.source_instance,
            channel_id: &pending.channel_id,
            workspace: &pending.workspace,
            capsule_run: &pending.capsule_run,
            recipe: &pending.recipe,
            model: &pending.model,
            saved_authorization_sha256: &pending.saved_authorization_sha256,
        },
        native: Native {
            pubkey: &record.pubkey,
            name: &record.name,
            acp_command: &record.acp_command,
            agent_command: &record.agent_command,
            agent_command_override: record.agent_command_override.as_deref(),
            agent_args: &record.agent_args,
            mcp_command: &record.mcp_command,
            persona_id: record.persona_id.as_deref(),
            team_id: record.team_id.as_deref(),
            system_prompt: record.system_prompt.as_deref(),
            provider: record.provider.as_deref(),
            model: record.model.as_deref(),
            env_vars: &record.env_vars,
            parallelism: record.parallelism,
            session_policy: &record.session_policy,
            respond_to: &record.respond_to,
            respond_to_allowlist: &record.respond_to_allowlist,
            start_on_app_launch: record.start_on_app_launch,
            auto_restart_on_config_change: record.auto_restart_on_config_change,
            backend: &record.backend,
            relay_url: &record.relay_url,
            idle_timeout_seconds: record.idle_timeout_seconds,
            max_turn_duration_seconds: record.max_turn_duration_seconds,
            effort_level: record.effort_level.as_deref(),
        },
    };
    serde_json::to_vec(&frozen)
        .map(|bytes| digest(&bytes))
        .map_err(|_| "invalid native preparation".into())
}

fn project(record: &ManagedAgentRecord) -> Result<PreparedRowviaSavedAgent, String> {
    let pending = record
        .pending_saved_registration
        .as_ref()
        .ok_or("preparation missing")?;
    let auth = record
        .auth_tag
        .as_deref()
        .ok_or("saved authorization missing")?;
    if digest(auth.as_bytes()) != pending.saved_authorization_sha256
        || config_digest(record, pending)? != pending.native_config_sha256
    {
        return Err("native preparation changed".into());
    }
    Ok(PreparedRowviaSavedAgent {
        operation_id: pending.operation_id.clone(),
        pubkey: record.pubkey.clone(),
        core_agent_name: format!("saved-{}", pending.operation_id.replace('-', "")),
        saved_authorization_sha256: pending.saved_authorization_sha256.clone(),
        native_config_sha256: pending.native_config_sha256.clone(),
        created_stopped: true,
        bindings_ready: false,
        runtime_ready: false,
    })
}

/// Required trusted expectation, never deserialized from IPC or provider input.
#[derive(Clone)]
pub struct RowviaSavedAgentScopeExpectation {
    pub selection: RowviaSavedAgentPreparation,
    pub agent_pubkey: String,
    pub native_config_sha256: String,
    pub saved_authorization_sha256: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RowviaSavedAgentNativeScope {
    pub agent_pubkey: String,
    pub saved_authorization_sha256: String,
    pub native_config_sha256: String,
    pub channel_id: String,
    pub system_prompt: String,
    pub model: String,
    pub workspace: String,
    pub capsule_run: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RowviaSavedAgentOperationScope {
    pub version: u8,
    pub operation_id: String,
    pub agent_name: String,
    pub recipe_id: String,
    pub native: RowviaSavedAgentNativeScope,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RowviaSavedAgentScope {
    pub version: u8,
    pub operation: RowviaSavedAgentOperationScope,
    pub owner_pubkey: String,
    pub relay_origin: String,
    pub source_instance: String,
    pub stopped: bool,
    pub pending: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ConfirmedRowviaSavedAgentPreparationAck {
    pub ack: SavedAgentPreparationAck,
    pub confirmed: bool,
}

const SCOPE_REFUSED: &str = "saved agent scope refused";

/// Custody from an existing verified pending snapshot, never deserialized or
/// reconstructed from later caller values. Only this module can construct it.
pub(crate) struct VerifiedRowviaSavedAgentPreparation {
    expected: RowviaSavedAgentScopeExpectation,
    prepared: PreparedRowviaSavedAgent,
    ack: SavedAgentPreparationAck,
}

impl VerifiedRowviaSavedAgentPreparation {
    pub(super) fn matches_cohort(
        &self,
        expected: &RowviaSavedAgentScopeExpectation,
        prepared: &PreparedRowviaSavedAgent,
        ack: &SavedAgentPreparationAck,
    ) -> bool {
        self.prepared == *prepared
            && self.ack == *ack
            && self.expected.selection.operation_id == expected.selection.operation_id
            && self.expected.agent_pubkey == expected.agent_pubkey
            && self.expected.native_config_sha256 == expected.native_config_sha256
            && self.expected.saved_authorization_sha256 == expected.saved_authorization_sha256
        // native_config_sha256 commits EVERY original selection field, including
        // owner/relay/source/name/prompt/recipe/model and both configured paths.
    }
    pub(super) fn into_parts(
        self,
    ) -> (
        RowviaSavedAgentScopeExpectation,
        PreparedRowviaSavedAgent,
        SavedAgentPreparationAck,
    ) {
        (self.expected, self.prepared, self.ack)
    }
}

fn validate_resume_operation(operation_id: &str) -> Result<(), String> {
    let operation = uuid::Uuid::parse_str(operation_id).map_err(|_| SCOPE_REFUSED)?;
    if operation.get_version_num() != 7
        || operation.get_variant() != uuid::Variant::RFC4122
        || operation.to_string() != operation_id
    {
        return Err(SCOPE_REFUSED.into());
    }
    Ok(())
}

fn checked_resume_target<T>(
    records: &[ManagedAgentRecord],
    runtimes: &std::collections::HashMap<ManagedAgentRuntimeKey, T>,
    operation_id: &str,
) -> Result<VerifiedRowviaSavedAgentPreparation, String> {
    validate_resume_operation(operation_id)?;
    let mut matches = records.iter().filter(|record| {
        record
            .pending_saved_registration
            .as_ref()
            .is_some_and(|pending| pending.operation_id == operation_id)
    });
    let record = matches.next().ok_or(SCOPE_REFUSED)?;
    if matches.next().is_some() {
        return Err(SCOPE_REFUSED.into());
    }
    // Verify the persisted native config/auth hashes BEFORE deriving the
    // original selection. A legacy record or absent ACK is never adopted.
    let prepared = project(record).map_err(|_| SCOPE_REFUSED)?;
    let pending = record
        .pending_saved_registration
        .as_ref()
        .ok_or(SCOPE_REFUSED)?;
    let ack = pending.prepared_ack.as_ref().ok_or(SCOPE_REFUSED)?;
    let expected = RowviaSavedAgentScopeExpectation {
        selection: RowviaSavedAgentPreparation {
            operation_id: pending.operation_id.clone(),
            owner_pubkey: pending.owner_pubkey.clone(),
            relay_url: pending.relay_url.clone(),
            source_instance: pending.source_instance.clone(),
            channel_id: pending.channel_id.clone(),
            name: record.name.clone(),
            system_prompt: record.system_prompt.clone().ok_or(SCOPE_REFUSED)?,
            recipe: pending.recipe,
            model: pending.model.clone(),
            workspace: pending.workspace.clone(),
            capsule_run: pending.capsule_run.clone(),
        },
        agent_pubkey: prepared.pubkey.clone(),
        native_config_sha256: prepared.native_config_sha256.clone(),
        saved_authorization_sha256: prepared.saved_authorization_sha256.clone(),
    };
    checked_saved_agent_target(records, runtimes, &expected)?;
    ack.validate_pending_cohort(pending, &expected.agent_pubkey)
        .map_err(|_| SCOPE_REFUSED)?;
    Ok(VerifiedRowviaSavedAgentPreparation {
        expected,
        prepared,
        ack: ack.clone(),
    })
}

/// Pure admission for undoing the two standalone-backfill link fields. The
/// caller must separately prove the exact manufactured definition and unique
/// references. Existing retained digests and ACK remain the only authority.
pub(crate) fn saved_agent_backfill_restore_is_original(record: &ManagedAgentRecord) -> bool {
    if record.persona_id.is_some()
        || record.persona_source_version.is_some()
        || record.team_id.is_some()
        || record.persona_team_dir.is_some()
        || record.persona_name_in_team.is_some()
        || record.source_team.is_some()
        || record.source_team_persona_slug.is_some()
        || record.catalog_source.is_some()
        || record.team_catalog_source.is_some()
        || record.slug.is_some()
        || record.runtime.is_some()
        || record.display_name.is_some()
        || !record.name_pool.is_empty()
        || record.definition_respond_to.is_some()
        || !record.definition_respond_to_allowlist.is_empty()
        || record.definition_parallelism.is_some()
        || record.is_builtin
        || !record.is_active
        || record.shared
        || record.start_on_app_launch
        || record.auto_restart_on_config_change
        || record.backend != BackendKind::Local
        || record.backend_agent_id.is_some()
        || record.provider_policy_pending
        || record.provider_binary_path.is_some()
        || record.relay_mesh.is_some()
    {
        return false;
    }
    let Some(pending) = record.pending_saved_registration.as_ref() else {
        return false;
    };
    checked_resume_target(
        std::slice::from_ref(record),
        &std::collections::HashMap::<_, ()>::new(),
        &pending.operation_id,
    )
    .is_ok()
}

/// Read only recovery of existing C2 evidence. The operation UUIDv7 is the only
/// selector; no key hydration, signing, retention, repair or store creation.
pub(crate) fn resume_rowvia_saved_agent_preparation<R: tauri::Runtime>(
    app: &AppHandle<R>,
    operation_id: &str,
) -> Result<VerifiedRowviaSavedAgentPreparation, String> {
    validate_resume_operation(operation_id)?;
    let state = app.state::<AppState>();
    let _workspace = state
        .workspace_apply_lock
        .try_lock()
        .map_err(|_| SCOPE_REFUSED)?;
    let _identity = state.identity_mutation.lock().map_err(|_| SCOPE_REFUSED)?;
    let _transition = state
        .managed_agent_runtime_transition
        .lock()
        .map_err(|_| SCOPE_REFUSED)?;
    if state
        .reset_failed
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return Err(SCOPE_REFUSED.into());
    }
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|_| SCOPE_REFUSED)?;
    let runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|_| SCOPE_REFUSED)?;
    let snapshot = storage::load_agent_store_readonly(app)?;
    let verified = checked_resume_target(&snapshot.records, &runtimes, operation_id)?;
    check_scope(&state, &verified.expected.selection).map_err(|_| SCOPE_REFUSED)?;
    Ok(verified)
}

const FINALIZATION_REFUSED: &str = "saved agent registration finalization refused";
const FINALIZATION_UNKNOWN: &str = "saved agent registration finalization unknown";

/// The exact Main registration result, not a deserializable authority token.
/// The trusted fixed continuation verifies the ready binding receipt, Paseo
/// record and capsule inspection before constructing the private completion.
struct RowviaSavedAgentRegistrationResult {
    version: u8,
    operation_id: String,
    agent_pubkey: String,
    canonical21_sha256: String,
    paseo_agent_id: String,
    bindings_ready: bool,
    runtime_prepared: bool,
    runtime_ready: bool,
}

/// Consumed once. Neither IPC JSON, approval status nor caller booleans can
/// construct this proof; the release constructor consumes fixed-host custody.
pub(crate) struct VerifiedRowviaSavedAgentRegistrationCompletion {
    preparation: VerifiedRowviaSavedAgentPreparation,
    result: RowviaSavedAgentRegistrationResult,
}

fn validate_registration_completion(
    completion: &VerifiedRowviaSavedAgentRegistrationCompletion,
) -> Result<(), String> {
    let proof = &completion.preparation;
    let expected = &proof.expected;
    let result = &completion.result;
    validate(&expected.selection).map_err(|_| FINALIZATION_REFUSED)?;
    let paseo = uuid::Uuid::parse_str(&result.paseo_agent_id).map_err(|_| FINALIZATION_REFUSED)?;
    if paseo.is_nil()
        || paseo.get_variant() != uuid::Variant::RFC4122
        || !(1..=8).contains(&paseo.get_version_num())
        || paseo.to_string() != result.paseo_agent_id
        || result.version != 1
        || result.operation_id != expected.selection.operation_id
        || result.agent_pubkey != expected.agent_pubkey
        || result.canonical21_sha256 != proof.ack.canonical21_sha256
        || !canonical_hash(&result.canonical21_sha256)
        || !result.bindings_ready
        || !result.runtime_prepared
        || result.runtime_ready
        || proof.prepared.operation_id != expected.selection.operation_id
        || proof.prepared.pubkey != expected.agent_pubkey
        || proof.prepared.native_config_sha256 != expected.native_config_sha256
        || proof.prepared.saved_authorization_sha256 != expected.saved_authorization_sha256
        || !proof.prepared.created_stopped
        || proof.prepared.bindings_ready
        || proof.prepared.runtime_ready
    {
        return Err(FINALIZATION_REFUSED.into());
    }
    Ok(())
}

#[cfg(test)]
fn registration_completion_for_test(
    preparation: VerifiedRowviaSavedAgentPreparation,
    result: RowviaSavedAgentRegistrationResult,
) -> Result<VerifiedRowviaSavedAgentRegistrationCompletion, String> {
    let completion = VerifiedRowviaSavedAgentRegistrationCompletion {
        preparation,
        result,
    };
    validate_registration_completion(&completion)?;
    Ok(completion)
}

/// The sole release constructor consumes evidence from the fixed host pipe
/// owner after real local+family settlement and its final resumed C2 check.
/// Neither a JSON registration result nor an authorization can call this path.
#[cfg(target_os = "linux")]
pub(super) fn registration_completion_from_host(
    preparation: VerifiedRowviaSavedAgentPreparation,
    host: super::rowvia_saved_agent_registration::prepare_caller::production::RegisteredHostProof,
) -> Result<VerifiedRowviaSavedAgentRegistrationCompletion, String> {
    let (
        version,
        operation_id,
        agent_pubkey,
        canonical21_sha256,
        paseo_agent_id,
        bindings_ready,
        runtime_prepared,
        runtime_ready,
    ) = host
        .into_native_result(&preparation.expected, &preparation.ack)
        .map_err(|_| FINALIZATION_REFUSED)?;
    let completion = VerifiedRowviaSavedAgentRegistrationCompletion {
        preparation,
        result: RowviaSavedAgentRegistrationResult {
            version,
            operation_id,
            agent_pubkey,
            canonical21_sha256,
            paseo_agent_id,
            bindings_ready,
            runtime_prepared,
            runtime_ready,
        },
    };
    validate_registration_completion(&completion)?;
    Ok(completion)
}

fn checked_registration_completion_target<T>(
    records: &[ManagedAgentRecord],
    runtimes: &std::collections::HashMap<ManagedAgentRuntimeKey, T>,
    completion: &VerifiedRowviaSavedAgentRegistrationCompletion,
) -> Result<usize, String> {
    validate_registration_completion(completion)?;
    let proof = &completion.preparation;
    let target = checked_saved_agent_target(records, runtimes, &proof.expected)
        .map_err(|_| FINALIZATION_REFUSED)?;
    let record = &records[target];
    let pending = record
        .pending_saved_registration
        .as_ref()
        .ok_or(FINALIZATION_REFUSED)?;
    if record.start_on_app_launch
        || record.auto_restart_on_config_change
        || pending.prepared_ack.as_ref() != Some(&proof.ack)
        || project(record).map_err(|_| FINALIZATION_REFUSED)? != proof.prepared
    {
        return Err(FINALIZATION_REFUSED.into());
    }
    proof
        .ack
        .validate_pending_cohort(pending, &record.pubkey)
        .map_err(|_| FINALIZATION_REFUSED)?;
    Ok(target)
}

fn check_finalization_scope(
    state: &AppState,
    selection: &RowviaSavedAgentPreparation,
) -> Result<(), String> {
    if state
        .reset_failed
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return Err(FINALIZATION_REFUSED.into());
    }
    check_scope(state, selection).map_err(|_| FINALIZATION_REFUSED.into())
}

#[cfg(test)]
#[derive(Clone, Copy)]
enum FinalizationPostCommitFault {
    Shutdown,
    ResetFailed,
    Owner,
    Relay,
}

#[cfg(test)]
thread_local! {
    static FINALIZATION_POST_COMMIT_FAULT: std::cell::Cell<Option<FinalizationPostCommitFault>> = const { std::cell::Cell::new(None) };
}

/// Consuming Native finalization: registered/stopped, never Start or automatic
/// retry. The fixed owner continuation supplies the private completion proof.
pub(crate) fn finalize_rowvia_saved_agent_registration<R: tauri::Runtime>(
    app: &AppHandle<R>,
    completion: VerifiedRowviaSavedAgentRegistrationCompletion,
) -> Result<(), String> {
    validate_registration_completion(&completion)?;
    let state = app.state::<AppState>();
    let _workspace = state
        .workspace_apply_lock
        .try_lock()
        .map_err(|_| FINALIZATION_REFUSED)?;
    let _identity = state
        .identity_mutation
        .lock()
        .map_err(|_| FINALIZATION_REFUSED)?;
    let _transition = state
        .managed_agent_runtime_transition
        .lock()
        .map_err(|_| FINALIZATION_REFUSED)?;
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|_| FINALIZATION_REFUSED)?;
    let runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|_| FINALIZATION_REFUSED)?;
    check_finalization_scope(&state, &completion.preparation.expected.selection)?;
    let snapshot = storage::load_agent_store_readonly(app).map_err(|_| FINALIZATION_REFUSED)?;
    let target = checked_registration_completion_target(&snapshot.records, &runtimes, &completion)?;
    let pending = snapshot.records[target]
        .pending_saved_registration
        .clone()
        .ok_or(FINALIZATION_REFUSED)?;
    let retained = storage::finalize_saved_agent_registration(app, snapshot, target, &pending)
        .map_err(|_| FINALIZATION_UNKNOWN)?;
    #[cfg(test)]
    if let Some(fault) = FINALIZATION_POST_COMMIT_FAULT.with(|slot| slot.take()) {
        match fault {
            FinalizationPostCommitFault::Shutdown => state
                .shutdown_started
                .store(true, std::sync::atomic::Ordering::Release),
            FinalizationPostCommitFault::ResetFailed => state
                .reset_failed
                .store(true, std::sync::atomic::Ordering::Release),
            FinalizationPostCommitFault::Owner => {
                *state.keys.lock().map_err(|_| FINALIZATION_UNKNOWN)? = nostr::Keys::generate()
            }
            FinalizationPostCommitFault::Relay => {
                *state
                    .relay_url_override
                    .lock()
                    .map_err(|_| FINALIZATION_UNKNOWN)? = Some("ws://127.0.0.1:10".into())
            }
        }
    }
    check_finalization_scope(&state, &completion.preparation.expected.selection)
        .map_err(|_| FINALIZATION_UNKNOWN)?;
    let record = retained.records.get(target).ok_or(FINALIZATION_UNKNOWN)?;
    if record.pubkey != completion.preparation.expected.agent_pubkey
        || record.pending_saved_registration.is_some()
        || record.runtime_pid.is_some()
        || record.start_on_app_launch
        || record.auto_restart_on_config_change
        || !managed_agent_runtime_keys(&runtimes, &record.pubkey).is_empty()
    {
        return Err(FINALIZATION_UNKNOWN.into());
    }
    Ok(())
}

fn canonical_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && !value.bytes().all(|b| b == b'0')
}

/// Shared exact-target admission over the raw snapshot, including all relay map
/// keys. Generic marker values let tests exercise the stopped proof without children.
fn checked_saved_agent_target<T>(
    records: &[ManagedAgentRecord],
    runtimes: &std::collections::HashMap<ManagedAgentRuntimeKey, T>,
    expected: &RowviaSavedAgentScopeExpectation,
) -> Result<usize, String> {
    let input = &expected.selection;
    validate(input).map_err(|_| SCOPE_REFUSED)?;
    if !canonical_hash(&expected.agent_pubkey)
        || !canonical_hash(&expected.native_config_sha256)
        || !canonical_hash(&expected.saved_authorization_sha256)
    {
        return Err(SCOPE_REFUSED.into());
    }
    let mut targets = records
        .iter()
        .enumerate()
        .filter(|(_, record)| record.pubkey.eq_ignore_ascii_case(&expected.agent_pubkey));
    let (index, record) = targets.next().ok_or(SCOPE_REFUSED)?;
    if targets.next().is_some()
        || record.pubkey != expected.agent_pubkey
        || record.runtime_pid.is_some()
        || !managed_agent_runtime_keys(runtimes, &record.pubkey).is_empty()
    {
        return Err(SCOPE_REFUSED.into());
    }
    let pending = record
        .pending_saved_registration
        .as_ref()
        .ok_or(SCOPE_REFUSED)?;
    if records.iter().enumerate().any(|(other, record)| {
        other != index
            && record
                .pending_saved_registration
                .as_ref()
                .is_some_and(|prior| {
                    prior.operation_id == input.operation_id
                        || prior.capsule_run == input.capsule_run
                })
    }) || pending.operation_id != input.operation_id
        || pending.owner_pubkey != input.owner_pubkey
        || pending.relay_url != input.relay_url
        || record.relay_url != input.relay_url
        || pending.source_instance != input.source_instance
        || pending.channel_id != input.channel_id
        || record.name != input.name
        || record.system_prompt.as_deref() != Some(input.system_prompt.as_str())
        || pending.recipe != input.recipe
        || pending.model != input.model
        || pending.workspace != input.workspace
        || pending.capsule_run != input.capsule_run
    {
        return Err(SCOPE_REFUSED.into());
    }
    let public = project(record).map_err(|_| SCOPE_REFUSED)?;
    if public.native_config_sha256 != expected.native_config_sha256
        || public.saved_authorization_sha256 != expected.saved_authorization_sha256
    {
        return Err(SCOPE_REFUSED.into());
    }
    Ok(index)
}

fn captured_scope(record: &ManagedAgentRecord) -> Result<RowviaSavedAgentScope, String> {
    let public = project(record).map_err(|_| SCOPE_REFUSED)?;
    let pending = record
        .pending_saved_registration
        .as_ref()
        .ok_or(SCOPE_REFUSED)?;
    Ok(RowviaSavedAgentScope {
        version: 1,
        operation: RowviaSavedAgentOperationScope {
            version: 1,
            operation_id: pending.operation_id.clone(),
            agent_name: public.core_agent_name,
            recipe_id: match pending.recipe {
                SavedAgentRecipe::CodexLocalV1 => "codex-local-v1",
                SavedAgentRecipe::CodexDevelopmentMemoryGitReadV1 => {
                    "codex-development-memory-git-read-v1"
                }
                SavedAgentRecipe::ClaudeLocalV1 => "claude-local-v1",
            }
            .into(),
            native: RowviaSavedAgentNativeScope {
                agent_pubkey: record.pubkey.clone(),
                saved_authorization_sha256: public.saved_authorization_sha256,
                native_config_sha256: public.native_config_sha256,
                channel_id: pending.channel_id.clone(),
                system_prompt: record.system_prompt.clone().ok_or(SCOPE_REFUSED)?,
                model: pending.model.clone(),
                workspace: pending.workspace.clone(),
                capsule_run: pending.capsule_run.clone(),
            },
        },
        owner_pubkey: pending.owner_pubkey.clone(),
        relay_origin: pending.relay_url.clone(),
        source_instance: pending.source_instance.clone(),
        stopped: true,
        pending: true,
    })
}

#[derive(Clone, Copy)]
enum ScopeOperation<'a> {
    Capture,
    Retain(&'a SavedAgentPreparationAck),
    Revalidate(&'a SavedAgentPreparationAck),
    RevalidateRetained(&'a SavedAgentPreparationAck),
}

enum ScopeConfirmation {
    Capture(RowviaSavedAgentScope),
    Ack(ConfirmedRowviaSavedAgentPreparationAck),
}

fn checked_saved_agent_scope<R: tauri::Runtime>(
    app: &AppHandle<R>,
    expected: &RowviaSavedAgentScopeExpectation,
    operation: ScopeOperation<'_>,
) -> Result<ScopeConfirmation, String> {
    validate(&expected.selection).map_err(|_| SCOPE_REFUSED)?;
    let state = app.state::<AppState>();
    let _workspace = state
        .workspace_apply_lock
        .try_lock()
        .map_err(|_| SCOPE_REFUSED)?;
    let _identity = state.identity_mutation.lock().map_err(|_| SCOPE_REFUSED)?;
    let _transition = state
        .managed_agent_runtime_transition
        .lock()
        .map_err(|_| SCOPE_REFUSED)?;
    if matches!(operation, ScopeOperation::RevalidateRetained(_))
        && state
            .reset_failed
            .load(std::sync::atomic::Ordering::Acquire)
    {
        return Err(SCOPE_REFUSED.into());
    }
    check_scope(&state, &expected.selection).map_err(|_| SCOPE_REFUSED)?;
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|_| SCOPE_REFUSED)?;
    let runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|_| SCOPE_REFUSED)?;
    let snapshot = storage::load_agent_store_readonly(app)?;
    let target = checked_saved_agent_target(&snapshot.records, &runtimes, expected)?;
    let ack = match operation {
        ScopeOperation::Capture => {
            let scope = captured_scope(&snapshot.records[target])?;
            check_scope(&state, &expected.selection).map_err(|_| SCOPE_REFUSED)?;
            return Ok(ScopeConfirmation::Capture(scope));
        }
        ScopeOperation::Retain(ack)
        | ScopeOperation::Revalidate(ack)
        | ScopeOperation::RevalidateRetained(ack) => ack,
    };
    let pending = snapshot.records[target]
        .pending_saved_registration
        .as_ref()
        .ok_or(SCOPE_REFUSED)?;
    ack.validate_pending_cohort(pending, &expected.agent_pubkey)
        .map_err(|_| SCOPE_REFUSED)?;
    if pending
        .prepared_ack
        .as_ref()
        .is_some_and(|prior| prior != ack)
    {
        return Err(SCOPE_REFUSED.into());
    }
    let snapshot = match operation {
        ScopeOperation::Retain(_) => {
            storage::retain_saved_agent_preparation_ack(app, snapshot, target, ack)?
        }
        ScopeOperation::Revalidate(_) | ScopeOperation::RevalidateRetained(_) => snapshot,
        ScopeOperation::Capture => unreachable!(),
    };
    let unconfirmed = match operation {
        ScopeOperation::Retain(_) => "saved agent preparation retention unknown",
        _ => SCOPE_REFUSED,
    };
    let target = checked_saved_agent_target(&snapshot.records, &runtimes, expected)
        .map_err(|_| unconfirmed)?;
    if snapshot.records[target]
        .pending_saved_registration
        .as_ref()
        .and_then(|pending| pending.prepared_ack.as_ref())
        != Some(ack)
    {
        return Err(unconfirmed.into());
    }
    check_scope(&state, &expected.selection).map_err(|_| unconfirmed)?;
    Ok(ScopeConfirmation::Ack(
        ConfirmedRowviaSavedAgentPreparationAck {
            ack: ack.clone(),
            confirmed: true,
        },
    ))
}

/// Read only; no IPC registration, authority grant, or runtime operation.
pub fn capture_rowvia_saved_agent_scope<R: tauri::Runtime>(
    app: &AppHandle<R>,
    expected: &RowviaSavedAgentScopeExpectation,
) -> Result<RowviaSavedAgentScope, String> {
    match checked_saved_agent_scope(app, expected, ScopeOperation::Capture)? {
        ScopeConfirmation::Capture(scope) => Ok(scope),
        ScopeConfirmation::Ack(_) => Err(SCOPE_REFUSED.into()),
    }
}

pub(super) fn revalidate_resumed_rowvia_saved_agent_preparation_ack<R: tauri::Runtime>(
    app: &AppHandle<R>,
    expected: &RowviaSavedAgentScopeExpectation,
    ack: &SavedAgentPreparationAck,
) -> Result<ConfirmedRowviaSavedAgentPreparationAck, String> {
    match checked_saved_agent_scope(app, expected, ScopeOperation::RevalidateRetained(ack))? {
        ScopeConfirmation::Ack(confirmation) => Ok(confirmation),
        ScopeConfirmation::Capture(_) => Err(SCOPE_REFUSED.into()),
    }
}

/// Retain public historical preparation evidence only, keeping pending in place.
pub fn retain_rowvia_saved_agent_preparation_ack<R: tauri::Runtime>(
    app: &AppHandle<R>,
    expected: &RowviaSavedAgentScopeExpectation,
    ack: &SavedAgentPreparationAck,
) -> Result<ConfirmedRowviaSavedAgentPreparationAck, String> {
    match checked_saved_agent_scope(app, expected, ScopeOperation::Retain(ack))? {
        ScopeConfirmation::Ack(confirmation) => Ok(confirmation),
        ScopeConfirmation::Capture(_) => Err(SCOPE_REFUSED.into()),
    }
}

/// Explicit readonly reconciliation; missing/different evidence is never repaired.
pub fn revalidate_rowvia_saved_agent_preparation_ack<R: tauri::Runtime>(
    app: &AppHandle<R>,
    expected: &RowviaSavedAgentScopeExpectation,
    ack: &SavedAgentPreparationAck,
) -> Result<ConfirmedRowviaSavedAgentPreparationAck, String> {
    match checked_saved_agent_scope(app, expected, ScopeOperation::Revalidate(ack))? {
        ScopeConfirmation::Ack(confirmation) => Ok(confirmation),
        ScopeConfirmation::Capture(_) => Err(SCOPE_REFUSED.into()),
    }
}

/// Save one stopped identity in a trusted native host; never publishes or starts.
/// The caller supplies a reviewed host selection. No IPC entrypoint exists yet.
pub fn prepare_rowvia_saved_agent<R: tauri::Runtime>(
    app: &AppHandle<R>,
    input: RowviaSavedAgentPreparation,
) -> Result<PreparedRowviaSavedAgent, String> {
    validate(&input)?;
    let state = app.state::<AppState>();
    let _workspace = state
        .workspace_apply_lock
        .try_lock()
        .map_err(|_| "workspace changing")?;
    let _identity = state
        .identity_mutation
        .lock()
        .map_err(|_| "identity unavailable")?;
    let _transition = state
        .managed_agent_runtime_transition
        .lock()
        .map_err(|_| "runtime transition unavailable")?;
    check_scope(&state, &input)?;
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|_| "agent store unavailable")?;
    let records = storage::load_unhydrated_managed_agents(app)
        .map_err(|_| "saved agent preparation unavailable")?;
    if records.iter().any(|record| {
        record.name == input.name
            || record
                .pending_saved_registration
                .as_ref()
                .is_some_and(|prior| {
                    prior.operation_id == input.operation_id
                        || prior.capsule_run == input.capsule_run
                })
    }) {
        return Err("prepared agent already exists".into());
    }
    let identity = mint_managed_agent_identity().map_err(|_| "saved agent identity unavailable")?;
    let auth_tag = managed_agent_auth_tag(&state, &identity.keys)
        .map_err(|_| "saved agent authorization unavailable")?;
    let mut pending = PendingSavedAgentRegistration {
        operation_id: input.operation_id,
        owner_pubkey: input.owner_pubkey,
        relay_url: input.relay_url.clone(),
        source_instance: input.source_instance,
        channel_id: input.channel_id,
        recipe: input.recipe,
        model: input.model,
        workspace: input.workspace,
        capsule_run: input.capsule_run.clone(),
        saved_authorization_sha256: digest(auth_tag.as_bytes()),
        native_config_sha256: String::new(),
        prepared_ack: None,
    };
    let mut record = ManagedAgentRecord {
        pubkey: identity.pubkey,
        name: input.name,
        description: None,
        persona_id: None,
        team_id: None,
        private_key_nsec: identity.private_key_nsec,
        auth_tag: Some(auth_tag),
        pending_saved_registration: None,
        relay_url: input.relay_url,
        avatar_url: None,
        acp_command: DEFAULT_ACP_COMMAND.into(),
        agent_command: "/usr/bin/python3.13".into(),
        agent_command_override: Some("/usr/bin/python3.13".into()),
        agent_args: vec!["-IS".into(), input.capsule_run],
        mcp_command: String::new(),
        turn_timeout_seconds: DEFAULT_AGENT_TURN_TIMEOUT_SECONDS,
        idle_timeout_seconds: None,
        max_turn_duration_seconds: None,
        parallelism: 1,
        session_policy: Default::default(),
        system_prompt: Some(input.system_prompt),
        model: None,
        provider: None,
        persona_source_version: None,
        start_on_app_launch: false,
        auto_restart_on_config_change: false,
        runtime_pid: None,
        backend: BackendKind::Local,
        backend_agent_id: None,
        provider_policy_pending: false,
        provider_binary_path: None,
        persona_team_dir: None,
        persona_name_in_team: None,
        env_vars: BTreeMap::from([("BUZZ_ACP_NO_BASE_PROMPT".into(), "true".into())]),
        created_at: crate::util::now_iso(),
        updated_at: crate::util::now_iso(),
        last_started_at: None,
        last_stopped_at: None,
        last_exit_code: None,
        last_error: None,
        last_error_code: None,
        respond_to: RespondTo::default(),
        respond_to_allowlist: Vec::new(),
        display_name: None,
        slug: None,
        runtime: None,
        name_pool: Vec::new(),
        is_builtin: false,
        is_active: true,
        shared: false,
        source_team: None,
        source_team_persona_slug: None,
        catalog_source: None,
        team_catalog_source: None,
        definition_respond_to: None,
        definition_respond_to_allowlist: Vec::new(),
        definition_parallelism: None,
        relay_mesh: None,
        effort_level: None,
    };
    pending.native_config_sha256 = config_digest(&record, &pending)?;
    record.pending_saved_registration = Some(pending);
    let public = project(&record)?;
    // Locks prevent owner/relay transitions until the same record is persisted.
    storage::append_prepared_managed_agent(app, record)
        .map_err(|_| "saved agent preparation save unconfirmed")?;
    Ok(public)
}

#[cfg(test)]
#[path = "rowvia_saved_agent_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "rowvia_saved_agent_resume_tests.rs"]
pub(super) mod resume_tests;

#[cfg(test)]
#[path = "rowvia_saved_agent_finalization_tests.rs"]
mod finalization_tests;
