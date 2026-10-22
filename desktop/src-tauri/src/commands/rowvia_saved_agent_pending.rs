//! Read-only public pending inventory; never exports a private pending record.
use crate::{
    app_state::AppState,
    managed_agents::{storage, ManagedAgentRecord, SavedAgentRecipe},
};
use serde::Serialize;
use tauri::{AppHandle, Manager};

const UNAVAILABLE: &str = "saved_agent_owner_unavailable";

/// Minimal recovery selector, scoped by Native to current owner and community.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SavedAgentPendingSummary {
    version: u8,
    operation_id: String,
    agent_pubkey: String,
    name: String,
    status: &'static str,
}

fn project(
    records: &[ManagedAgentRecord],
    owner: &str,
    relay: &str,
) -> Result<Vec<SavedAgentPendingSummary>, String> {
    let mut result = Vec::new();
    let mut operations = std::collections::BTreeSet::new();
    let mut pubkeys = std::collections::BTreeSet::new();
    for record in records {
        let Some(pending) = &record.pending_saved_registration else {
            continue;
        };
        if pending.owner_pubkey != owner
            || pending.relay_url != relay
            || record.relay_url != relay
            || pending.recipe != SavedAgentRecipe::CodexDevelopmentMemoryGitReadV1
        {
            continue;
        }
        let operation = uuid::Uuid::parse_str(&pending.operation_id).map_err(|_| UNAVAILABLE)?;
        if operation.get_version_num() != 7
            || operation.get_variant() != uuid::Variant::RFC4122
            || operation.to_string() != pending.operation_id
            || record.pubkey.len() != 64
            || !record
                .pubkey
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || record.pubkey.bytes().all(|b| b == b'0')
            || record.pubkey == owner
            || record.name.is_empty()
            || record.name.len() > 128
            || record.name.trim() != record.name
            || record.name.chars().any(char::is_control)
            || record.runtime_pid.is_some()
            || !operations.insert(pending.operation_id.as_str())
            || !pubkeys.insert(record.pubkey.as_str())
            || result.len() >= 64
        {
            return Err(UNAVAILABLE.into());
        }
        result.push(SavedAgentPendingSummary {
            version: 1,
            operation_id: pending.operation_id.clone(),
            agent_pubkey: record.pubkey.clone(),
            name: record.name.clone(),
            status: "pending",
        });
    }
    Ok(result)
}

/// Lists public selectors only. No migration, credential hydration or writes.
#[tauri::command]
pub fn rowvia_list_saved_agent_pending(
    app: AppHandle,
) -> Result<Vec<SavedAgentPendingSummary>, String> {
    let state = app.state::<AppState>();
    let _workspace = state
        .workspace_apply_lock
        .try_lock()
        .map_err(|_| UNAVAILABLE)?;
    let _identity = state.identity_mutation.lock().map_err(|_| UNAVAILABLE)?;
    let _transition = state
        .managed_agent_runtime_transition
        .lock()
        .map_err(|_| UNAVAILABLE)?;
    if state
        .shutdown_started
        .load(std::sync::atomic::Ordering::Acquire)
        || state
            .reset_failed
            .load(std::sync::atomic::Ordering::Acquire)
    {
        return Err(UNAVAILABLE.into());
    }
    let owner = state
        .signing_keys()
        .map_err(|_| UNAVAILABLE)?
        .public_key()
        .to_hex();
    let relay = crate::relay::relay_ws_url_with_override(&state);
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|_| UNAVAILABLE)?;
    let runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|_| UNAVAILABLE)?;
    let snapshot = storage::load_agent_store_readonly(&app).map_err(|_| UNAVAILABLE)?;
    let result = project(&snapshot.records, &owner, &relay)?;
    if result.iter().any(|item| {
        !crate::managed_agents::managed_agent_runtime_keys(&runtimes, &item.agent_pubkey).is_empty()
    }) {
        return Err(UNAVAILABLE.into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record() -> ManagedAgentRecord {
        serde_json::from_value(serde_json::json!({
            "pubkey": "a".repeat(64), "name": "Saved Codex", "relay_url": "wss://relay.example",
            "acp_command": "buzz-acp", "agent_command": "python3", "agent_args": [], "mcp_command": "",
            "turn_timeout_seconds": 0, "created_at": "2026-10-22T00:00:00Z", "updated_at": "2026-10-22T00:00:00Z",
            "private_key_nsec": "PRIVATE_KEY_CANARY", "auth_tag": "PRIVATE_AUTH_CANARY",
            "pending_saved_registration": {
                "operation_id": "01940000-0000-7000-8000-000000000077", "owner_pubkey": "b".repeat(64),
                "relay_url": "wss://relay.example", "source_instance": "fixture",
                "channel_id": "01940000-0000-7000-8000-000000000088", "model": "gpt-6.1-sol",
                "workspace": "/PRIVATE_PATH_CANARY/workspace", "capsule_run": "/PRIVATE_PATH_CANARY/run",
                "saved_authorization_sha256": "c".repeat(64), "native_config_sha256": "d".repeat(64),
                "recipe": "codex-development-memory-git-read-v1"
            }
        })).unwrap()
    }
    #[test]
    fn pending_inventory_is_scoped_bounded_and_exports_only_five_public_fields() {
        let record = record();
        let owner = "b".repeat(64);
        let result = project(&[record.clone()], &owner, "wss://relay.example").unwrap();
        assert_eq!(result.len(), 1);
        let json = serde_json::to_value(&result[0]).unwrap();
        assert_eq!(json.as_object().unwrap().len(), 5);
        let text = serde_json::to_string(&result).unwrap();
        assert!(!text.contains("PRIVATE") && !text.contains("workspace") && !text.contains("auth"));
        assert!(
            project(&[record.clone()], &"9".repeat(64), "wss://relay.example")
                .unwrap()
                .is_empty()
        );
        assert!(project(&[record.clone()], &owner, "wss://other.example")
            .unwrap()
            .is_empty());
        assert!(project(
            &[record.clone(), record.clone()],
            &owner,
            "wss://relay.example"
        )
        .is_err());
        let mut old = record.clone();
        old.pending_saved_registration.as_mut().unwrap().recipe = SavedAgentRecipe::CodexLocalV1;
        assert!(project(&[old], &owner, "wss://relay.example")
            .unwrap()
            .is_empty());
        let mut running = record;
        running.runtime_pid = Some(100);
        assert!(project(&[running], &owner, "wss://relay.example").is_err());
    }
}
