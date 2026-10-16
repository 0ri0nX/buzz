//! Temporary fixed saved-developer handoff, reachable only through the opt-in hook.
//! No global store hydration, status synchronization, pending flush or catalog merge.
#[path = "owner_test_saved_developer_core.rs"]
mod core;
pub(crate) use core::{is_operation, validate_arguments};

use super::{custom_harnesses, ManagedAgentRecord, ManagedAgentRuntimeKey};
use crate::app_state::AppState;
use serde_json::{json, Map, Value};
use std::{fs, io::Read, os::unix::fs::MetadataExt, path::Path, sync::Arc};
use tauri::{AppHandle, Manager};

fn target_runtime_occupied(
    receipt_dir: &Path,
    runtimes: &std::collections::HashMap<ManagedAgentRuntimeKey, super::ManagedAgentPairRuntime>,
) -> Result<bool, String> {
    let receipt_present = match fs::read_dir(receipt_dir) {
        Ok(entries) => {
            let mut found = false;
            for (index, entry) in entries.enumerate() {
                if index == 4096 {
                    return Err("runtime_ownership_unknown".into());
                }
                let entry = entry.map_err(|_| "runtime_ownership_unknown")?;
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(core::TARGET)
                {
                    found = true;
                }
            }
            found
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => return Err("runtime_ownership_unknown".into()),
    };
    Ok(receipt_present || runtimes.keys().any(|k| k.pubkey == core::TARGET))
}

fn retain_started_runtime<R: tauri::Runtime>(
    app: &AppHandle<R>,
    key: ManagedAgentRuntimeKey,
    process: super::ManagedAgentProcess,
    runtimes: &mut std::collections::HashMap<
        ManagedAgentRuntimeKey,
        super::ManagedAgentPairRuntime,
    >,
) -> Result<(), String> {
    let receipt = super::ManagedAgentRuntimeReceipt {
        key: key.clone(),
        pid: process.child.id(),
        desktop_instance_id: super::current_instance_id(app),
        started_at: crate::util::now_iso(),
    };
    // Keep the child discoverable even if the durable receipt cannot be written.
    runtimes.insert(key, super::ManagedAgentPairRuntime::starting(process));
    super::write_agent_runtime_receipt(app, &receipt)
}

fn bounded_read(path: &Path, cap: u64) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "input_unavailable")?;
    if !metadata.is_file() || metadata.len() > cap {
        return Err("input_invalid".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| "input_unavailable")?
        .take(cap + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "input_unavailable")?;
    if bytes.len() as u64 > cap {
        return Err("input_invalid".into());
    }
    Ok(bytes)
}

// Metadata only: never reads or manufactures private worker inputs. The existing
// pinned worker performs full provenance/pin/config verification when actually run.
fn capsule_stamp(uid: u32) -> Option<Value> {
    let root = Path::new(core::CAPSULE);
    if fs::canonicalize(root).ok()?.as_path() != root {
        return None;
    }
    let mut stamps = Vec::new();
    for (name, mode, directory) in [
        ("", 0o700, true),
        ("run", 0o700, false),
        ("profile.json", 0o600, false),
        ("parent.toml", 0o600, false),
        ("provenance.json", 0o600, false),
    ] {
        let m = fs::symlink_metadata(root.join(name)).ok()?;
        if m.uid() != uid
            || m.mode() & 0o7777 != mode
            || m.is_dir() != directory
            || (!directory && (!m.is_file() || m.nlink() != 1 || m.len() > 1024 * 1024))
        {
            return None;
        }
        stamps.push(json!([
            name,
            m.dev(),
            m.ino(),
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec()
        ]));
    }
    let python = Path::new(core::PYTHON);
    if fs::canonicalize(python).ok()?.as_path() != python {
        return None;
    }
    let m = fs::symlink_metadata(python).ok()?;
    if !m.is_file() || m.mode() & 0o022 != 0 || m.mode() & 0o111 == 0 {
        return None;
    }
    stamps.push(json!([
        core::PYTHON,
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec()
    ]));
    Some(json!(stamps))
}

/// Execute under the same production locks/admission and spawn primitive as pairs.
pub(crate) fn execute<R: tauri::Runtime>(
    app: &AppHandle<R>,
    operation: &str,
    args: &Map<String, Value>,
) -> Result<Map<String, Value>, String> {
    validate_arguments(operation, args)?;
    let state = app.state::<AppState>();
    // apply_workspace replaces relay and keys without identity_mutation. Take
    // its outer transaction lock first, before any lock that restore may need.
    // Refuse a busy apply instead of waiting behind it with another lock held.
    let _workspace = state
        .workspace_apply_lock
        .try_lock()
        .map_err(|_| "not_ready")?;
    let admission = super::AdmissionSnapshot::capture(&state);
    let _identity = state
        .identity_mutation
        .lock()
        .map_err(|_| "operation_failed")?;
    let transition = state
        .managed_agent_runtime_transition
        .lock()
        .map_err(|_| "operation_failed")?;
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|_| "operation_failed")?;
    let relay_override = state
        .relay_url_override
        .lock()
        .map_err(|_| "operation_failed")?;
    let selected_relay = relay_override
        .clone()
        .unwrap_or_else(crate::relay::relay_ws_url);
    let relay =
        buzz_core_pkg::relay::normalize_relay_url(&selected_relay).map_err(|_| "scope_mismatch")?;
    let owner = state
        .keys
        .lock()
        .map_err(|_| "operation_failed")?
        .public_key()
        .to_hex();
    let locked = state.signing_keys().is_err()
        || state
            .reset_failed
            .load(std::sync::atomic::Ordering::Acquire)
        || state
            .shutdown_started
            .load(std::sync::atomic::Ordering::Acquire);
    // Check actual selected identity/community before touching any saved state.
    if locked {
        return Err("not_ready".into());
    }
    if owner != core::OWNER || relay != core::RELAY {
        return Err("scope_mismatch".into());
    }
    let data = app.path().app_data_dir().map_err(|_| "operation_failed")?;
    let store_path = data.join("agents/managed-agents.json");
    let mut rows: Vec<Value> = serde_json::from_slice(&bounded_read(&store_path, 4 * 1024 * 1024)?)
        .map_err(|_| "store_invalid")?;
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|_| "operation_failed")?;
    // Any tracked target pair or target receipt/PID is an ownership ambiguity.
    // Do not try_wait, adopt, terminate, clean caches, or reap any pair here.
    let receipt_dir = data.join("agents/agent-pids");
    let occupied = target_runtime_occupied(&receipt_dir, &runtimes)?;
    let _persist = custom_harnesses::persist_mutex()
        .lock()
        .map_err(|_| "operation_failed")?;
    let harness_dir = data.join("custom_harnesses");
    let harness_path = harness_dir.join(format!("{}.json", core::HARNESS_ID));
    let expected = core::harness_bytes()?;
    {
        let registry = custom_harnesses::loaded_harness_registry()
            .read()
            .map_err(|_| "operation_failed")?;
        for definition in registry.iter().filter(|d| d.id == core::HARNESS_ID) {
            if serde_json::to_value(definition.as_ref()).map_err(|_| "operation_failed")?
                != core::harness()
            {
                return Err("harness_collision".into());
            }
        }
    }
    let mut existing = None;
    // IDs are defined by JSON, not filename: refuse duplicate/shadow files too.
    match fs::read_dir(&harness_dir) {
        Ok(entries) => {
            for (index, entry) in entries.enumerate() {
                if index == 256 {
                    return Err("harness_collision".into());
                }
                let path = entry.map_err(|_| "harness_collision")?.path();
                if path.extension().is_some_and(|e| e == "json") {
                    let bytes = bounded_read(&path, 64 * 1024)?;
                    let value: Value =
                        serde_json::from_slice(&bytes).map_err(|_| "harness_collision")?;
                    if path == harness_path || value["id"] == core::HARNESS_ID {
                        if path != harness_path || existing.is_some() {
                            return Err("harness_collision".into());
                        }
                        existing = Some(bytes);
                    }
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(_) => return Err("harness_collision".into()),
    }
    let uid = tempfile::tempfile()
        .and_then(|f| f.metadata())
        .map_err(|_| "operation_failed")?
        .uid();
    let stamp = capsule_stamp(uid);
    let plan = core::plan(
        operation,
        args,
        core::Scope {
            owner: &owner,
            relay: &relay,
            locked,
        },
        &rows,
        occupied,
        existing.as_deref(),
        stamp.as_ref(),
    )?;
    // Validate the target's real persisted schema before any write; unrelated
    // rows remain raw JSON and never enter generic key hydration/normalization.
    let mut record: ManagedAgentRecord =
        serde_json::from_value(plan.record.clone()).map_err(|_| "target_contract_mismatch")?;
    let result = json!({"pubkey":core::TARGET,"relayUrl":core::RELAY,"channelId":core::CHANNEL,
        "fingerprint":plan.fingerprint,"configured":plan.configured || plan.write,
        "prerequisitesPresent":stamp.is_some(),"runtimeReady":false,"runtimeOccupied":occupied || record.runtime_pid.is_some() || plan.start,
        "startDispatched":plan.start});
    if plan.write {
        // Unused identical harness is a safe prefix if the target store save fails.
        if existing.is_none() {
            fs::create_dir_all(&harness_dir).map_err(|_| "operation_failed")?;
            let mut file =
                tempfile::NamedTempFile::new_in(&harness_dir).map_err(|_| "operation_failed")?;
            use std::io::Write;
            file.write_all(&expected).map_err(|_| "operation_failed")?;
            file.persist_noclobber(&harness_path)
                .map_err(|_| "harness_collision")?;
        }
        rows[plan.index] = plan.record.clone();
        super::storage::atomic_write_json_restricted(
            &store_path,
            &serde_json::to_vec_pretty(&rows).map_err(|_| "operation_failed")?,
        )?;
    }
    if plan.start {
        let admitted = transition.admit(&admission, core::RELAY)?;
        super::storage::owner_test_target_key(&mut record)?;
        let definition: custom_harnesses::HarnessDefinition =
            serde_json::from_value(core::harness()).map_err(|_| "operation_failed")?;
        // Add only this definition; a full warm would replace unrelated registry state.
        {
            let mut registry = custom_harnesses::loaded_harness_registry()
                .write()
                .map_err(|_| "operation_failed")?;
            for d in registry.iter().filter(|d| d.id == core::HARNESS_ID) {
                if serde_json::to_value(d.as_ref()).map_err(|_| "operation_failed")?
                    != core::harness()
                {
                    return Err("harness_collision".into());
                }
            }
            if !registry.iter().any(|d| d.id == core::HARNESS_ID) {
                registry.push(Arc::new(definition));
            }
        }
        let key = ManagedAgentRuntimeKey::new(core::TARGET, core::RELAY)?;
        let process = super::runtime::spawn_agent_child_with_context(
            app,
            &record,
            core::RELAY,
            &admitted,
            true,
            Some(core::OWNER),
            None,
            &[],
            &[],
            &super::GlobalAgentConfig::default(),
        )?;
        // Retain ownership immediately, even if receipt persistence fails. Never
        // orphan a child or claim confirmed failure after a possible spawn.
        retain_started_runtime(app, key, process, &mut runtimes)?;
    }
    result
        .as_object()
        .cloned()
        .ok_or_else(|| "operation_failed".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exited_process(record: &ManagedAgentRecord) -> super::super::ManagedAgentProcess {
        // An inert, already-reaped fixture child; never launches an ACP harness.
        let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
        child.wait().unwrap();
        super::super::ManagedAgentProcess {
            child,
            log_path: Default::default(),
            spawn_config: super::super::spawn_snapshot::prospective_spawn_config_snapshot(
                record,
                &[],
                &[],
                core::RELAY,
                &Default::default(),
                false,
            ),
            setup_mode: false,
            adapter_availability: None,
            start_nonce: "inert-fixture".into(),
        }
    }

    #[test]
    fn actual_ownership_scan_refuses_target_receipts_and_any_tracked_target_pair() {
        let fixture = super::super::admission_test_support::app_with_keyless_agent();
        let record = super::super::load_managed_agents(fixture.app.handle())
            .unwrap()
            .remove(0);
        let receipts = tempfile::tempdir().unwrap();
        let mut runtimes = std::collections::HashMap::new();
        assert!(!target_runtime_occupied(receipts.path(), &runtimes).unwrap());
        let unrelated = receipts.path().join("unrelated.json");
        fs::write(&unrelated, "opaque-unrelated-receipt").unwrap();
        assert!(!target_runtime_occupied(receipts.path(), &runtimes).unwrap());
        let receipt = receipts
            .path()
            .join(format!("{}__stale.json", core::TARGET));
        fs::write(&receipt, "opaque-target-receipt").unwrap();
        assert!(target_runtime_occupied(receipts.path(), &runtimes).unwrap());
        assert_eq!(fs::read(&receipt).unwrap(), b"opaque-target-receipt");
        assert_eq!(fs::read(&unrelated).unwrap(), b"opaque-unrelated-receipt");
        fs::remove_file(receipt).unwrap();
        let other = ManagedAgentRuntimeKey::new(&record.pubkey, core::RELAY).unwrap();
        runtimes.insert(
            other,
            super::super::ManagedAgentPairRuntime::starting(exited_process(&record)),
        );
        assert!(!target_runtime_occupied(receipts.path(), &runtimes).unwrap());
        let target = ManagedAgentRuntimeKey::new(core::TARGET, "wss://other.example").unwrap();
        runtimes.insert(
            target.clone(),
            super::super::ManagedAgentPairRuntime::starting(exited_process(&record)),
        );
        assert!(target_runtime_occupied(receipts.path(), &runtimes).unwrap());
        // The scan must not reap even an exited target or inspect receipt contents.
        assert!(runtimes.contains_key(&target));
        assert_eq!(runtimes.len(), 2);
        assert_eq!(
            target_runtime_occupied(&unrelated, &runtimes).unwrap_err(),
            "runtime_ownership_unknown"
        );
    }

    #[test]
    fn actual_receipt_failure_keeps_spawned_child_tracked_for_reconciliation() {
        let fixture = super::super::admission_test_support::app_with_keyless_agent();
        let app = fixture.app.handle();
        let record = super::super::load_managed_agents(app).unwrap().remove(0);
        let receipts = app.path().app_data_dir().unwrap().join("agents/agent-pids");
        // A file at the directory coordinate deterministically rejects receipt IO.
        fs::write(&receipts, "receipt-failure-fixture").unwrap();
        let process = exited_process(&record);
        let pid = process.child.id();
        let key = ManagedAgentRuntimeKey::new(core::TARGET, core::RELAY).unwrap();
        let mut runtimes = std::collections::HashMap::new();
        assert!(retain_started_runtime(app, key.clone(), process, &mut runtimes).is_err());
        let retained = runtimes.get(&key).unwrap();
        assert_eq!(retained.process.child.id(), pid);
        assert_eq!(
            retained.lifecycle,
            super::super::ManagedAgentRuntimeLifecycle::Starting
        );
        assert_eq!(fs::read(&receipts).unwrap(), b"receipt-failure-fixture");
    }

    #[test]
    fn actual_descriptor_and_command_env_keep_fixed_lazy_mentions_policy() {
        let _registry = custom_harnesses::registry_test_lock();
        let fixture = super::super::admission_test_support::app_with_keyless_agent();
        let mut raw = serde_json::to_value(
            super::super::load_managed_agents(fixture.app.handle())
                .unwrap()
                .remove(0),
        )
        .unwrap();
        raw["pubkey"] = json!(core::TARGET);
        raw["relay_url"] = json!(core::RELAY);
        raw["acp_command"] = json!("buzz-acp");
        raw["env_vars"] = json!({"BUZZ_ACP_LAZY_POOL":"false", "BUZZ_ACP_NO_MENTION_FILTER":"true", "UNRELATED":"retained"});
        let plan = core::plan(
            "configure_saved_developer",
            &Map::new(),
            core::Scope {
                owner: core::OWNER,
                relay: core::RELAY,
                locked: false,
            },
            &[raw],
            false,
            None,
            None,
        )
        .unwrap();
        let record: ManagedAgentRecord = serde_json::from_value(plan.record).unwrap();
        let previous = custom_harnesses::loaded_harness_registry()
            .read()
            .unwrap()
            .clone();
        custom_harnesses::loaded_harness_registry()
            .write()
            .unwrap()
            .push(Arc::new(serde_json::from_value(core::harness()).unwrap()));
        let descriptor =
            super::super::resolve_effective_harness_descriptor(&record, &[], &Default::default());
        *custom_harnesses::loaded_harness_registry().write().unwrap() = previous;
        let descriptor = descriptor.unwrap();
        let mut command = std::process::Command::new("/usr/bin/true");
        command
            .env("BUZZ_ACP_LAZY_POOL", "false")
            .env("BUZZ_ACP_NO_MENTION_FILTER", "true");
        super::super::runtime::apply_harness_descriptor_env(&mut command, &descriptor);
        let env: std::collections::BTreeMap<_, _> = command
            .get_envs()
            .filter_map(|(key, value)| value.map(|value| (key, value)))
            .collect();
        for (key, value) in [
            ("BUZZ_ACP_LAZY_POOL", "true"),
            ("BUZZ_ACP_NO_MENTION_FILTER", "false"),
            ("BUZZ_ACP_CHANNELS", core::CHANNEL),
            ("BUZZ_ACP_SUBSCRIBE", "mentions"),
            ("BUZZ_ACP_NO_BASE_PROMPT", "true"),
            ("UNRELATED", "retained"),
        ] {
            assert_eq!(
                env.get(std::ffi::OsStr::new(key)).copied(),
                Some(std::ffi::OsStr::new(value))
            );
        }
    }

    #[test]
    fn actual_native_entry_refuses_locked_or_foreign_current_identity_before_store_io() {
        let fixture = super::super::admission_test_support::app_with_keyless_agent();
        let app = fixture.app.handle();
        let state = app.state::<AppState>();
        // Remove only this fixture's store: a missing early identity gate now
        // fails on input_unavailable instead of accidentally passing via plan().
        let store = app
            .path()
            .app_data_dir()
            .unwrap()
            .join("agents/managed-agents.json");
        fs::remove_file(store).unwrap();
        state
            .keyring_locked
            .store(true, std::sync::atomic::Ordering::Release);
        assert_eq!(
            execute(app, "configure_saved_developer", &Map::new())
                .err()
                .as_deref(),
            Some("not_ready")
        );
        state
            .keyring_locked
            .store(false, std::sync::atomic::Ordering::Release);
        assert_eq!(
            execute(app, "configure_saved_developer", &Map::new())
                .err()
                .as_deref(),
            Some("scope_mismatch")
        );
        assert!(state.managed_agent_processes.lock().unwrap().is_empty());
    }

    #[test]
    fn actual_workspace_transaction_blocks_hook_across_live_key_replacement() {
        let fixture = super::super::admission_test_support::app_with_keyless_agent();
        let app = fixture.app.handle();
        let state = app.state::<AppState>();
        let workspace = state.workspace_apply_lock.try_lock().unwrap();
        let before = state.keys.lock().unwrap().public_key();
        // The same real keys field apply_workspace replaces under this lock.
        *state.keys.lock().unwrap() = nostr::Keys::generate();
        *state.relay_url_override.lock().unwrap() = Some(core::RELAY.into());
        assert_ne!(state.keys.lock().unwrap().public_key(), before);
        assert_eq!(
            execute(app, "configure_saved_developer", &Map::new())
                .err()
                .as_deref(),
            Some("not_ready")
        );
        drop(workspace);
        assert_eq!(
            execute(app, "configure_saved_developer", &Map::new())
                .err()
                .as_deref(),
            Some("scope_mismatch")
        );
        assert!(state.managed_agent_processes.lock().unwrap().is_empty());
    }

    #[test]
    fn generic_and_target_spawn_seams_share_real_config_refusal_without_spawning() {
        let fixture = super::super::admission_test_support::app_with_keyless_agent();
        let app = fixture.app.handle();
        let mut record = super::super::load_managed_agents(app).unwrap().remove(0);
        record.private_key_nsec = "inert-nonempty-test-key".into();
        record.persona_id = Some("__missing_test_definition__".into());
        let state = app.state::<AppState>();
        let transition = state.managed_agent_runtime_transition.lock().unwrap();
        let snapshot = transition.snapshot();
        let admitted = transition
            .admit(&snapshot, super::super::admission_test_support::RELAY)
            .unwrap();
        let generic = super::super::runtime::spawn_agent_child(
            app,
            &record,
            super::super::admission_test_support::RELAY,
            &admitted,
            true,
            None,
            None,
        )
        .err()
        .unwrap();
        let agents = app.path().app_data_dir().unwrap().join("agents");
        let preserved: Vec<_> = [
            "managed-agents.json",
            "teams.json",
            "global-agent-config.json",
            "profile-reconcile-pending.json",
        ]
        .into_iter()
        .map(|name| {
            let path = agents.join(name);
            let bytes = format!("opaque pending configuration fixture: {name}").into_bytes();
            fs::write(&path, &bytes).unwrap();
            (path, bytes)
        })
        .collect();
        let narrow = super::super::runtime::spawn_agent_child_with_context(
            app,
            &record,
            super::super::admission_test_support::RELAY,
            &admitted,
            true,
            None,
            None,
            &[],
            &[],
            &super::super::GlobalAgentConfig::default(),
        )
        .err()
        .unwrap();
        assert_eq!(
            generic,
            super::super::effective_config::ORPHANED_INSTANCE_ERROR
        );
        assert_eq!(narrow, generic);
        for (path, bytes) in preserved {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
        assert!(state.managed_agent_processes.lock().unwrap().is_empty());
    }
}
