//! Temporary fixed recovery Stop. No launcher/configuration mutation surface.
use crate::{
    app_state::AppState,
    managed_agents::{
        BackendKind, ManagedAgentPairRuntime, ManagedAgentRecord, ManagedAgentRuntimeKey,
    },
};
use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    sync::{atomic::Ordering, Mutex},
};
use tauri::{AppHandle, Manager};

pub(crate) const OPERATION: &str = "stop_cerberus_for_recovery";
const INSPECT: &str = "inspect_cerberus_recovery_state";
pub(crate) fn is_operation(op: &str) -> bool {
    matches!(op, OPERATION | INSPECT)
}
const TARGET: &str = "9d3972d809d53ce8a87003ca7f631009963d9414739188cea6209ced88365c87";
const OWNER: &str = "89af092923ad49e3b9916b0c9f28c6b9237d8fcab4461cb7ddd12afe354f0d68";
const RELAY: &str = "wss://buzz.rowvia.ai:8443";

fn now() -> Result<u128, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_millis())
        .map_err(|_| "invalid_request".into())
}

fn validate_at(args: &Map<String, Value>, now: u128) -> Result<(), String> {
    let expiry = args
        .get("expiresAt")
        .and_then(Value::as_u64)
        .ok_or("invalid_request")?;
    if args.len() != 1 || u128::from(expiry) <= now || u128::from(expiry) > now + 120_000 {
        return Err("invalid_request".into());
    }
    Ok(())
}

pub(crate) fn validate_arguments(args: &Map<String, Value>) -> Result<(), String> {
    validate_at(args, now()?)
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
    validate_arguments(args)?; // key/relay/signing waits cannot extend expiry
    if owner != OWNER || relay != RELAY {
        return Err("scope_mismatch".into());
    }
    if !unlocked {
        return Err("not_ready".into());
    }
    Ok(())
}

// Lexical checks ONLY: never stat/realpath/read a referenced source/config file.
fn safe_path(value: &str, program: bool) -> bool {
    if value.len() > 1024
        || !value.starts_with('/')
        || value
            .split('/')
            .skip(1)
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
    {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    if [
        "nsec",
        "auth",
        "token",
        "credential",
        "secret",
        "private",
        "scanner",
        "key",
        ".env",
    ]
    .iter()
    .any(|word| lower.contains(word))
    {
        return false;
    }
    [
        "/home/orionx/project/RowviaContext",
        "/var/tmp/rowvia-management-pilot-v75-1000",
    ]
    .iter()
    .chain(if program {
        ["/usr/bin", "/bin", "/nix/store"].as_slice()
    } else {
        [].as_slice()
    })
    .any(|root| {
        value
            .strip_prefix(*root)
            .is_some_and(|tail| tail.starts_with('/'))
    })
}

fn public_program(value: &str) -> Option<&str> {
    (matches!(
        value,
        "buzz-acp" | "python3" | "python3.13" | "node" | "bash" | "sh"
    ) || safe_path(value, true))
    .then_some(value)
}

fn launcher_projection(outer: &str, agent: &str, args: &[String]) -> Value {
    let mut sources = Vec::new();
    let mut configs = Vec::new();
    for (index, arg) in args.iter().enumerate() {
        // Unknown long-flag values are not promoted to public source paths.
        if safe_path(arg, false)
            && [".py", ".mjs", ".sh"].iter().any(|ext| arg.ends_with(ext))
            && (index == 0 || !args[index - 1].starts_with("--"))
            && sources.len() < 8
            && !sources.contains(arg)
        {
            sources.push(arg.clone());
        }
        let config = if matches!(arg.as_str(), "--profile" | "--config") {
            args.get(index + 1).map(String::as_str)
        } else {
            arg.strip_prefix("--profile=")
                .or_else(|| arg.strip_prefix("--config="))
        };
        if let Some(path) = config.filter(|path| {
            safe_path(path, false) && (path.ends_with(".json") || path.ends_with(".toml"))
        }) {
            if configs.len() < 8 && !configs.iter().any(|p| p == path) {
                configs.push(path.to_owned());
            }
        }
    }
    json!({"outerProgram":public_program(outer),"agentProgram":public_program(agent),"sourcePaths":sources,"configPaths":configs})
}

fn check_target<'a>(
    rows: &'a [ManagedAgentRecord],
    runtimes: &HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>,
) -> Result<(&'a ManagedAgentRecord, u32), String> {
    let mut matches = rows
        .iter()
        .filter(|row| row.pubkey.eq_ignore_ascii_case(TARGET));
    let row = matches.next().ok_or("target_contract_mismatch")?;
    let pairs: Vec<_> = runtimes
        .iter()
        .filter(|(key, _)| key.pubkey.eq_ignore_ascii_case(TARGET))
        .collect();
    if matches.next().is_some()
        || row.pubkey != TARGET
        || row.backend != BackendKind::Local
        || row.relay_url != RELAY
        || pairs.len() != 1
    {
        return Err("runtime_ownership_unknown".into());
    }
    let (key, runtime) = pairs[0];
    let pid = runtime.child.id();
    if key.pubkey != TARGET
        || key.relay_url != RELAY
        || row.runtime_pid.is_some_and(|record_pid| record_pid != pid)
    {
        return Err("runtime_ownership_unknown".into());
    }
    Ok((row, pid))
}

fn public_result(pid: u32, launcher: Value) -> Map<String, Value> {
    json!({"pubkey":TARGET,"relayUrl":RELAY,"stopCompleted":true,"trackedChildPid":pid,"trackedChildSettled":true,"launcherBasis":"current_saved_effective_config","launcher":launcher}).as_object().unwrap().clone()
}

pub(crate) fn execute(
    app: &AppHandle,
    operation: &str,
    args: &Map<String, Value>,
    still_live: &(dyn Fn() -> Result<(), String> + Sync),
) -> Result<Map<String, Value>, String> {
    let state = app.state::<AppState>();
    let authorize = || {
        still_live()?;
        current_scope(&state, args)?;
        still_live()
    };
    match operation {
        INSPECT => inspect_in(app, &authorize),
        OPERATION => execute_in(app, &authorize),
        _ => Err("invalid_request".into()),
    }
}

fn inspect_in<R: tauri::Runtime>(
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
    // Normal loading can hydrate/migrate metadata; never list/reap/start/stop.
    let rows = crate::managed_agents::load_managed_agents(app).map_err(|_| "operation_failed")?;
    authorize()?;
    let runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|_| "operation_failed")?;
    authorize()?;
    let matches: Vec<_> = rows
        .iter()
        .filter(|r| r.pubkey.eq_ignore_ascii_case(TARGET))
        .collect();
    let pairs: Vec<_> = runtimes
        .iter()
        .filter(|(k, _)| k.pubkey.eq_ignore_ascii_case(TARGET))
        .collect();
    let cardinality = |len| match len {
        0 => "missing",
        1 => "unique",
        _ => "ambiguous",
    };
    let row = match matches.as_slice() {
        [row] => Some(*row),
        _ => None,
    };
    let pair = match pairs.as_slice() {
        [pair] => Some(*pair),
        _ => None,
    };
    let record_state = cardinality(matches.len());
    let pair_state = cardinality(pairs.len());
    let record_exact = row.is_some_and(|r| r.pubkey == TARGET);
    let local = row.is_some_and(|r| r.backend == BackendKind::Local);
    let relay = row.map_or("unavailable", |r| match r.relay_url.as_str() {
        "" => "empty",
        RELAY => "fixed",
        _ => "other",
    });
    let pair_scope = pair.is_some_and(|(key, _)| key.pubkey == TARGET && key.relay_url == RELAY);
    let scalar = row.map_or("unavailable", |r| match r.runtime_pid {
        None => "absent",
        Some(pid) if pair.is_some_and(|(_, runtime)| runtime.child.id() == pid) => "matches",
        Some(_) => "other",
    });
    let preflight = check_target(&rows, &runtimes).is_ok();
    drop(pairs);
    drop(matches);
    drop(runtimes);
    drop(rows);
    authorize()?;
    Ok(json!({"pubkey":TARGET,"recordState":record_state,"recordKeyExact":record_exact,
        "localBackend":local,"recordRelay":relay,"pairState":pair_state,"pairScopeMatches":pair_scope,
        "scalarPidState":scalar,"stopPreflightMatches":preflight}).as_object().unwrap().clone())
}

fn execute_in<R: tauri::Runtime>(
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
    authorize()?;
    let captured = Mutex::new(None);
    let guard =
        |rows: &[ManagedAgentRecord],
         runtimes: &HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>| {
            let (row, pid) = check_target(rows, runtimes)?;
            authorize()?;
            // Optional saved views cannot merge/retire builtins or block Stop.
            // Read/resolution errors and layered env never reach the result.
            let launcher = (|| {
                let personas = crate::managed_agents::storage::load_agent_definitions(app)
                    .ok()?
                    .into_iter()
                    .filter_map(|definition| definition.to_definition_view())
                    .collect::<Vec<_>>();
                let global = crate::managed_agents::load_global_agent_config(app).ok()?;
                let descriptor = crate::managed_agents::resolve_effective_harness_descriptor(
                    row, &personas, &global,
                )
                .ok()?;
                Some(launcher_projection(
                    &row.acp_command,
                    &descriptor.command,
                    &descriptor.args,
                ))
            })()
            .unwrap_or_else(|| launcher_projection("", "", &[]));
            authorize()?;
            *captured.lock().map_err(|_| "operation_failed")? = Some((pid, launcher));
            Ok(())
        };
    let summary = super::stop_managed_agent_guarded_in(TARGET, app, &state, (&guard, authorize))
        .map_err(|_| "operation_failed")?;
    let settled = summary.pubkey == TARGET && summary.status == "stopped" && summary.pid.is_none();
    drop(summary); // never serialize normal summaries or errors
    if !settled {
        return Err("operation_failed".into());
    }
    authorize().map_err(|_| "operation_failed")?; // post-Stop failures remain Unknown
    let (pid, launcher) = captured
        .into_inner()
        .map_err(|_| "operation_failed")?
        .ok_or("operation_failed")?;
    Ok(public_result(pid, launcher))
}

#[cfg(test)]
#[path = "owner_test_cerberus_recovery_tests.rs"]
mod tests;
