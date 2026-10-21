//! Temporary native owner-flow test; never serializes the normal create response.
#[path = "owner_test_claude_create_core.rs"]
mod core;

use crate::{
    app_state::AppState,
    managed_agents::{
        custom_harnesses, CreateManagedAgentRequest, CreateManagedAgentResponse, ManagedAgentRecord,
    },
};
use serde_json::{Map, Value};
use std::{fs, io::Read, os::unix::fs::OpenOptionsExt, path::Path, sync::atomic::Ordering};
use tauri::{AppHandle, Manager};

pub(crate) use core::OPERATION;

pub(crate) fn validate_arguments(args: &Map<String, Value>) -> Result<(), String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "invalid_request")?
        .as_millis();
    core::validate_at(args, now).map_err(str::to_owned)
}

fn current_scope(state: &AppState, args: &Map<String, Value>) -> Result<(), String> {
    validate_arguments(args)?;
    let relay = state
        .relay_url_override
        .lock()
        .map_err(|_| "operation_failed")?
        .clone()
        .unwrap_or_else(crate::relay::relay_ws_url);
    let relay = buzz_core_pkg::relay::normalize_relay_url(&relay).map_err(|_| "scope_mismatch")?;
    let owner = state
        .keys
        .lock()
        .map_err(|_| "operation_failed")?
        .public_key()
        .to_hex();
    let unlocked = state.signing_keys().is_ok()
        && !state.reset_failed.load(Ordering::Acquire)
        && !state.shutdown_started.load(Ordering::Acquire);
    core::check_scope(&owner, &relay, unlocked).map_err(str::to_owned)
}

fn install_harness(
    dir: &Path,
    check_scope: &impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    let _persist = custom_harnesses::persist_mutex()
        .lock()
        .map_err(|_| "operation_failed")?;
    if custom_harnesses::lookup_loaded_harness_by_id(core::HARNESS_ID).is_some() {
        return Err("harness_collision".into());
    }
    match fs::read_dir(dir) {
        Ok(entries) => {
            for (index, entry) in entries.enumerate() {
                if index >= 256 {
                    return Err("harness_collision".into());
                }
                let entry = entry.map_err(|_| "operation_failed")?;
                let path = entry.path();
                if path
                    .file_name()
                    .is_some_and(|name| name == format!("{}.json", core::HARNESS_ID).as_str())
                {
                    return Err("harness_collision".into());
                }
                if path.extension().is_none_or(|ext| ext != "json") {
                    continue;
                }
                let metadata = fs::symlink_metadata(&path).map_err(|_| "harness_collision")?;
                if !metadata.is_file() || metadata.len() > 64 * 1024 {
                    return Err("harness_collision".into());
                }
                let file = fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
                    .open(path)
                    .map_err(|_| "harness_collision")?;
                if !file.metadata().map_err(|_| "harness_collision")?.is_file() {
                    return Err("harness_collision".into());
                }
                let mut contents = Vec::new();
                file.take(64 * 1024 + 1)
                    .read_to_end(&mut contents)
                    .map_err(|_| "harness_collision")?;
                if contents.len() > 64 * 1024 {
                    return Err("harness_collision".into());
                }
                let value: Value =
                    serde_json::from_slice(&contents).map_err(|_| "harness_collision")?;
                if value["id"] == core::HARNESS_ID {
                    return Err("harness_collision".into());
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("operation_failed".into()),
    }
    let definition = serde_json::from_value(core::harness()).map_err(|_| "operation_failed")?;
    custom_harnesses::validate_harness_definition_pub(&definition)
        .map_err(|_| "operation_failed")?;
    custom_harnesses::check_id_collision(&definition.id).map_err(|_| "harness_collision")?;
    // The lock wait and bounded catalog reads may outlive authorization. This
    // check is inside persist_mutex, immediately before the first catalog write.
    check_scope()?;
    fs::create_dir_all(dir).map_err(|_| "operation_failed")?;
    custom_harnesses::save_custom_harness_to_dir(dir, &definition, None)
        .map_err(|_| "operation_failed")?;
    // This is the ordinary static JSON/preset loader, with no discovery probes,
    // executable resolution, install or spawn path.
    custom_harnesses::warm_harness_registry_from_dir(Some(dir));
    Ok(())
}

fn prepare_create<R: tauri::Runtime>(
    app: &AppHandle<R>,
    rows: &[ManagedAgentRecord],
    check_scope: impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    check_scope()?;
    core::refuse_target(rows.iter().map(|r| {
        (
            r.name.as_str(),
            r.agent_command.as_str(),
            r.agent_command_override.as_deref(),
            r.runtime.as_deref(),
        )
    }))
    .map_err(str::to_owned)?;
    // Normal creation pins a raw executable and explicit arguments. Recognize
    // the unique fixed capsule reference even if somebody later renames it.
    if rows
        .iter()
        .any(|row| row.agent_args.iter().any(|arg| arg == core::RUN))
    {
        return Err("target_collision".into());
    }
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "operation_failed")?
        .join("custom_harnesses");
    install_harness(&dir, &check_scope)?;
    // Recheck after catalog I/O too: expiry or shutdown cannot become a stale
    // authorization for the immediately following saved-record write.
    check_scope()
}

fn public_projection(response: CreateManagedAgentResponse) -> Map<String, Value> {
    let pubkey = response.agent.pubkey.clone();
    let profile_sync_succeeded = response.profile_sync_error.is_none();
    // Drop nsec, auth-bearing summary and every raw error before constructing
    // the hook's serializable response. No full-response Value ever exists.
    drop(response);
    core::public_result(pubkey, profile_sync_succeeded)
}

/// Create only the compiled fixed target through normal managed-agent creation.
/// Must run in dispatch_native's blocking worker: the narrow block_on adaptation
/// keeps synchronous scope guards off the socket executor and never holds an
/// outer managed_agents_store_lock while normal creation takes its own locks.
pub(crate) fn execute<R: tauri::Runtime>(
    app: &AppHandle<R>,
    args: &Map<String, Value>,
) -> Result<Map<String, Value>, String> {
    let state = app.state::<AppState>();
    let _workspace = state
        .workspace_apply_lock
        .try_lock()
        .map_err(|_| "not_ready")?;
    let _identity = state
        .identity_mutation
        .lock()
        .map_err(|_| "operation_failed")?;
    let _transition = state
        .managed_agent_runtime_transition
        .lock()
        .map_err(|_| "operation_failed")?;
    current_scope(&state, args)?;
    let input: CreateManagedAgentRequest =
        serde_json::from_value(core::request()).map_err(|_| "operation_failed")?;
    let response = tauri::async_runtime::block_on(super::create_managed_agent_guarded_in(
        input,
        app.clone(),
        &state,
        |rows| prepare_create(app, rows, || current_scope(&state, args)),
    ))
    .map_err(|error| match error.as_str() {
        "scope_mismatch" => "scope_mismatch".to_owned(),
        "not_ready" => "not_ready".to_owned(),
        _ => "operation_failed".to_owned(),
    })?;
    Ok(public_projection(response))
}

#[cfg(test)]
#[path = "owner_test_claude_create_tests.rs"]
mod tests;
