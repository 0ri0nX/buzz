//! Temporary fixed-target native adapters; ordinary public APIs remain unchanged.
#[path = "owner_test_claude_lifecycle_core.rs"]
mod core;
use crate::{
    app_state::AppState,
    managed_agents::{
        BackendKind, ManagedAgentPairRuntime, ManagedAgentRecord, ManagedAgentRuntimeKey,
        UpdateManagedAgentRequest, UpdateManagedAgentResponse,
    },
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
    sync::atomic::Ordering,
};
use tauri::{AppHandle, Manager};

pub(crate) use core::is_operation;
pub(crate) const ATTACH: &str = "attach_saved_claude_developer";
pub(crate) const TARGET: &str = core::TARGET;
pub(crate) const CHANNEL: &str = core::CHANNEL;
pub(crate) const RELAY: &str = core::RELAY;
pub(crate) const OWNER: &str = core::OWNER;

pub(crate) fn validate_arguments(op: &str, args: &Map<String, Value>) -> Result<(), String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "invalid_request")?
        .as_millis();
    core::validate_at(op, args, now).map_err(str::to_owned)
}

fn current_scope(state: &AppState, op: &str, args: &Map<String, Value>) -> Result<(), String> {
    validate_arguments(op, args)?;
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
    // Relay/key/signing-key mutex waits cannot extend the authorization.
    validate_arguments(op, args)?;
    core::check_scope(&owner, &relay, unlocked).map_err(str::to_owned)
}

fn pinned_file(path: &Path, uid: u32, mode: u32, max: u64, hash: &str) -> Result<(), String> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| "setup_pin_invalid")?;
    let before = file.metadata().map_err(|_| "setup_pin_invalid")?;
    if !before.is_file()
        || before.uid() != uid
        || before.nlink() != 1
        || before.permissions().mode() & 0o7777 != mode
        || before.len() > max
    {
        return Err("setup_pin_invalid".into());
    }
    let mut bytes = Vec::new();
    (&file)
        .take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "setup_pin_invalid")?;
    if bytes.len() as u64 > max || hex::encode(Sha256::digest(&bytes)) != hash {
        return Err("setup_pin_invalid".into());
    }
    let after = file.metadata().map_err(|_| "setup_pin_invalid")?;
    let named = fs::symlink_metadata(path).map_err(|_| "setup_pin_invalid")?;
    let stamp = |m: &fs::Metadata| {
        (
            m.dev(),
            m.ino(),
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
            m.mode(),
            m.uid(),
            m.nlink(),
        )
    };
    if stamp(&before) != stamp(&after) || stamp(&after) != stamp(&named) {
        return Err("setup_pin_invalid".into());
    }
    Ok(())
}

fn verify_setup_at(dir: &Path, uid: u32, exe_hash: &str, profile_hash: &str) -> Result<(), String> {
    let before = fs::symlink_metadata(dir).map_err(|_| "setup_pin_invalid")?;
    if !before.is_dir()
        || before.uid() != uid
        || before.permissions().mode() & 0o7777 != 0o700
        || fs::canonicalize(dir).map_err(|_| "setup_pin_invalid")? != dir
    {
        return Err("setup_pin_invalid".into());
    }
    pinned_file(
        &dir.join("rowvia-native-auth-setup"),
        uid,
        0o700,
        32 * 1024 * 1024,
        exe_hash,
    )?;
    pinned_file(&dir.join("profile.json"), uid, 0o600, 4096, profile_hash)?;
    let after = fs::symlink_metadata(dir).map_err(|_| "setup_pin_invalid")?;
    if (before.dev(), before.ino(), before.mode(), before.uid())
        != (after.dev(), after.ino(), after.mode(), after.uid())
    {
        return Err("setup_pin_invalid".into());
    }
    Ok(())
}

fn verify_setup() -> Result<(), String> {
    verify_setup_at(
        Path::new(core::SETUP_DIR),
        1000,
        core::SETUP_SHA,
        core::PROFILE_SHA,
    )
}

fn receipt_present<R: tauri::Runtime>(
    app: &AppHandle<R>,
    except: Option<&str>,
) -> Result<bool, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "operation_failed")?
        .join("agents/agent-pids");
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err("runtime_ownership_unknown".into()),
    };
    for (index, entry) in entries.enumerate() {
        if index >= 4096 {
            return Err("runtime_ownership_unknown".into());
        }
        let name = entry.map_err(|_| "runtime_ownership_unknown")?.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(core::TARGET) && except != Some(name.as_ref()) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn target_contract(
    rows: &[ManagedAgentRecord],
    setup: bool,
) -> Result<&ManagedAgentRecord, String> {
    target_contract_with_initial(rows, setup, false)
}

fn target_contract_with_initial(
    rows: &[ManagedAgentRecord],
    setup: bool,
    allow_initial: bool,
) -> Result<&ManagedAgentRecord, String> {
    let mut matches = rows.iter().filter(|r| r.pubkey == core::TARGET);
    let record = matches.next().ok_or("target_contract_mismatch")?;
    let (command, args) = core::command(setup);
    if matches.next().is_some()
        || record.name != "Rowvia Developer Claude"
        || record.backend != BackendKind::Local
        || record.acp_command != crate::managed_agents::DEFAULT_ACP_COMMAND
        || record.persona_id.is_some()
        || record.team_id.is_some()
        || record.runtime.is_some()
        || record.auth_tag.as_ref().is_none_or(|tag| tag.is_empty())
        || record.start_on_app_launch
        || record.agent_command != core::PYTHON
        || record.agent_command_override.as_deref() != Some(command)
        || record.agent_args != args
        || !((record.env_vars == core::configured_env() && record.parallelism == 1)
            || (allow_initial
                && !setup
                && record.env_vars.is_empty()
                && record.parallelism == crate::managed_agents::DEFAULT_AGENT_PARALLELISM))
        || record.relay_url != core::RELAY
    {
        return Err("target_contract_mismatch".into());
    }
    Ok(record)
}

fn check_target<R: tauri::Runtime>(
    app: &AppHandle<R>,
    rows: &[ManagedAgentRecord],
    runtimes: &HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>,
    setup: bool,
    stopping: bool,
) -> Result<(), String> {
    check_target_with_initial(app, rows, runtimes, setup, stopping, false)
}

fn check_target_with_initial<R: tauri::Runtime>(
    app: &AppHandle<R>,
    rows: &[ManagedAgentRecord],
    runtimes: &HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>,
    setup: bool,
    stopping: bool,
    allow_initial: bool,
) -> Result<(), String> {
    let record = if allow_initial {
        target_contract_with_initial(rows, setup, true)?
    } else {
        target_contract(rows, setup)?
    };
    let pairs: Vec<_> = runtimes
        .keys()
        .filter(|k| k.pubkey == core::TARGET)
        .collect();
    if stopping {
        let expected = format!(
            "{}.json",
            ManagedAgentRuntimeKey::new(core::TARGET, core::RELAY)?.runtime_id()
        );
        if pairs.len() > 1
            || pairs.iter().any(|k| k.relay_url != core::RELAY)
            || record.runtime_pid.is_some_and(|pid| {
                pairs
                    .first()
                    .and_then(|key| runtimes.get(*key))
                    .is_none_or(|runtime| runtime.child.id() != pid)
            })
            || receipt_present(
                app,
                if pairs.is_empty() {
                    None
                } else {
                    Some(&expected)
                },
            )?
            || (pairs.is_empty() && record.runtime_pid.is_some())
        {
            return Err("runtime_ownership_unknown".into());
        }
    } else if !pairs.is_empty() || record.runtime_pid.is_some() || receipt_present(app, None)? {
        return Err("runtime_occupied".into());
    }
    Ok(())
}

pub(crate) fn check_attachment<R: tauri::Runtime>(
    app: &AppHandle<R>,
    args: &Map<String, Value>,
    still_pending: &dyn Fn() -> Result<(), String>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    check_attachment_in(app, &|| current_scope(&state, ATTACH, args), still_pending)
}

fn check_attachment_in<R: tauri::Runtime>(
    app: &AppHandle<R>,
    scope: &dyn Fn() -> Result<(), String>,
    still_pending: &dyn Fn() -> Result<(), String>,
) -> Result<(), String> {
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
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|_| "operation_failed")?;
    scope()?;
    still_pending()?;
    scope()?;
    let rows = crate::managed_agents::load_managed_agents(app).map_err(|_| "operation_failed")?;
    let runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|_| "operation_failed")?;
    check_target_with_initial(app, &rows, &runtimes, false, false, true)?;
    scope()?;
    still_pending()?;
    scope()
}

fn stop_then_restore(
    stop: impl FnOnce() -> Result<(), String>,
    restore: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    stop()?;
    restore()
}

fn discard_update(response: UpdateManagedAgentResponse) -> Result<(), String> {
    let synced = response.profile_sync_error.is_none();
    drop(response);
    if synced {
        Ok(())
    } else {
        Err("operation_failed".into())
    }
}

fn inspect_initial_in<R: tauri::Runtime>(
    app: &AppHandle<R>,
    authorize: &(dyn Fn() -> Result<(), String> + Sync),
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
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|_| "operation_failed")?;
    authorize()?;
    // Normal loading may hydrate/migrate internal keys. Not a zero-write claim.
    let rows = crate::managed_agents::load_managed_agents(app).map_err(|_| "operation_failed")?;
    authorize()?;
    let runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|_| "operation_failed")?;
    authorize()?;
    let matching: Vec<_> = rows
        .iter()
        .filter(|row| row.pubkey == core::TARGET)
        .collect();
    let record_state = match matching.len() {
        0 => "missing",
        1 => "unique",
        _ => "ambiguous",
    };
    let row = match matching.as_slice() {
        [row] => Some(*row),
        _ => None,
    };
    // These actual guards remain authoritative, not the diagnostic flags below.
    let contract = target_contract_with_initial(&rows, false, true).is_ok();
    let stopped = check_target_with_initial(app, &rows, &runtimes, false, false, true).is_ok();
    let no_pairs = !runtimes.keys().any(|key| key.pubkey == core::TARGET);
    let no_scalar = row.is_some_and(|row| row.runtime_pid.is_none());
    let receipts = receipt_present(app, None);
    let receipts_readable = receipts.is_ok();
    let no_receipts = matches!(receipts, Ok(false));
    let checks = row.map(|row| {
        [
            row.name == "Rowvia Developer Claude",
            row.backend == BackendKind::Local,
            row.acp_command == crate::managed_agents::DEFAULT_ACP_COMMAND,
            row.persona_id.is_none() && row.team_id.is_none(),
            row.runtime.is_none(),
            row.auth_tag.as_ref().is_some_and(|tag| !tag.is_empty()),
            !row.start_on_app_launch,
            row.agent_command == core::PYTHON
                && row.agent_command_override.as_deref() == Some(core::PYTHON)
                && row.agent_args == core::command(false).1,
            row.env_vars.is_empty()
                && row.parallelism == crate::managed_agents::DEFAULT_AGENT_PARALLELISM,
            row.env_vars == core::configured_env() && row.parallelism == 1,
            row.relay_url == core::RELAY,
        ]
    });
    drop(matching);
    drop(runtimes);
    drop(rows);
    authorize()?;
    Ok(
        serde_json::json!({"pubkey":core::TARGET,"recordState":record_state,
        "contractMatches":contract,"stoppedInitialMatches":stopped,"noTrackedPairs":no_pairs,
        "noScalarPid":no_scalar,"receiptsReadable":receipts_readable,"noTargetReceipts":no_receipts,
        "checks":checks.map(|c|serde_json::json!({"nameMatches":c[0],"localBackend":c[1],
            "defaultTransport":c[2],"unlinked":c[3],"runtimeUnset":c[4],"bindingPresent":c[5],
            "autostartOff":c[6],"normalCommandMatches":c[7],"initialSettingsMatch":c[8],
            "configuredNormalSettingsMatch":c[9],"relayMatches":c[10]}))})
        .as_object()
        .unwrap()
        .clone(),
    )
}

pub(crate) fn execute(
    app: &AppHandle,
    op: &str,
    args: &Map<String, Value>,
) -> Result<Map<String, Value>, String> {
    validate_arguments(op, args)?;
    if op == ATTACH {
        return Err("invalid_request".into());
    }
    let state = app.state::<AppState>();
    if op == core::INSPECT_INITIAL {
        return inspect_initial_in(app, &|| current_scope(&state, op, args));
    }
    // Never reacquire these outer locks in the normal helpers. Start owns its
    // own transition; stopped updates and stop/restore hold it here instead.
    let _workspace = state
        .workspace_apply_lock
        .try_lock()
        .map_err(|_| "not_ready")?;
    let _identity = state
        .identity_mutation
        .lock()
        .map_err(|_| "operation_failed")?;
    let authorize = || current_scope(&state, op, args);
    authorize()?;
    let setup = op != "start_saved_claude_normal";
    let check =
        |rows: &[ManagedAgentRecord],
         runtimes: &HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>| {
            check_target(app, rows, runtimes, setup, false)?;
            if setup {
                verify_setup()?;
            }
            authorize()
        };
    let result = match op {
        "start_saved_claude_setup" | "start_saved_claude_normal" => {
            tauri::async_runtime::block_on(super::start_local_agent_after_preflight_guarded(
                app,
                &state,
                core::TARGET,
                Some(core::RELAY),
                Some(core::OWNER),
                None,
                (
                    |model: Option<String>| async move {
                        if model.is_some() {
                            return Err("target_contract_mismatch".into());
                        }
                        super::ensure_relay_mesh_for_record(app, model.as_deref(), false).await
                    },
                    Some(&check),
                    Some(&authorize),
                ),
            ))
            .map(drop)
        }
        "prepare_saved_claude_setup" | "stop_restore_saved_claude_normal" => {
            let admission = crate::managed_agents::AdmissionSnapshot::capture(&state);
            let _transition = state
                .managed_agent_runtime_transition
                .lock()
                .map_err(|_| "operation_failed")?;
            let update = |to_setup: bool| {
                let input: UpdateManagedAgentRequest =
                    serde_json::from_value(core::update(to_setup))
                        .map_err(|_| "operation_failed")?;
                let guard = |rows: &[ManagedAgentRecord],
                             runtimes: &HashMap<
                    ManagedAgentRuntimeKey,
                    ManagedAgentPairRuntime,
                >| {
                    check_target_with_initial(app, rows, runtimes, !to_setup, false, to_setup)?;
                    if to_setup {
                        verify_setup()?;
                    }
                    authorize()
                };
                let before_save = || {
                    if to_setup {
                        verify_setup()?;
                    }
                    authorize()
                };
                tauri::async_runtime::block_on(
                    super::super::agent_models::update_managed_agent_guarded_in(
                        input,
                        app,
                        &state,
                        (&guard, &before_save),
                        Some(&admission),
                    ),
                )
                .and_then(discard_update)
            };
            if op == "prepare_saved_claude_setup" {
                update(true)
            } else {
                stop_then_restore(
                    || {
                        let guard = |rows: &[ManagedAgentRecord],
                                     runtimes: &HashMap<
                            ManagedAgentRuntimeKey,
                            ManagedAgentPairRuntime,
                        >| {
                            check_target(app, rows, runtimes, true, true)?;
                            authorize()
                        };
                        super::stop_managed_agent_guarded_in(
                            core::TARGET,
                            app,
                            &state,
                            (&guard, &authorize),
                        )
                        .map(drop)
                    },
                    || update(false),
                )
            }
        }
        _ => Err("invalid_request".into()),
    };
    // Normal summaries/errors may contain sensitive runtime data. Never turn
    // either into a Value or reflect their text into the owner-hook result.
    result.map_err(|e| {
        match e.as_str() {
            "scope_mismatch" => "scope_mismatch",
            "not_ready" => "not_ready",
            _ => "operation_failed",
        }
        .to_owned()
    })?;
    Ok(core::public_result(op))
}

#[cfg(test)]
#[path = "owner_test_claude_lifecycle_tests.rs"]
mod tests;
