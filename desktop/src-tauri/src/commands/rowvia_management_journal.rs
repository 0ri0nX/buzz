//! Small, owner and community scoped durable management journal.
//! Writes are atomic and completed before their corresponding network request.

use std::{
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_ENTRIES: usize = 32;
const MAX_CANDIDATES: usize = 64;
const MAX_RETIRED: usize = 128;
// A signed observer frame may be admitted for up to five minutes. Retire only
// after that window so a replay cannot acquire a fresh management request ID.
const RETIRE_AFTER_SECONDS: i64 = 360;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct Scope {
    pub owner_pubkey: String,
    pub relay_url: String,
    pub origin: String,
    pub source_instance: String,
    pub cerberus_pubkey: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Candidate {
    pub selection_handle: String,
    pub agent_name: String,
    pub target_pubkey: String,
    pub binding_id: String,
    pub gmail_label: String,
    pub binding_status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Entry {
    #[serde(default)]
    pub created_at: i64,
    pub logical_request_id: String,
    pub draft_digest: String,
    pub source_event_id: String,
    pub receipt_handle: String,
    pub request_id: String,
    pub request_body_b64: String,
    pub request_digest: String,
    pub receipt_body_b64: Option<String>,
    pub receipt_digest: Option<String>,
    pub operation_id: Option<String>,
    pub approval_intent: bool,
    #[serde(default)]
    pub approval_body_b64: Option<String>,
    #[serde(default)]
    pub approval_body_digest: Option<String>,
    #[serde(default)]
    pub refused: bool,
    #[serde(default)]
    pub last_status_b64: Option<String>,
    #[serde(default)]
    pub last_status_digest: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct RetiredEntry {
    pub logical_request_id: String,
    pub draft_digest: String,
    pub source_event_id: String,
    pub request_id: String,
    pub operation_id: Option<String>,
    pub outcome: String,
    pub retired_at: i64,
}

fn safely_terminal(entry: &Entry) -> Option<&'static str> {
    if entry.created_at <= 0 || Utc::now().timestamp() - entry.created_at < RETIRE_AFTER_SECONDS {
        return None;
    }
    if entry.refused && !entry.approval_intent {
        return Some("refused");
    }
    if !entry.approval_intent {
        return expired_unapproved_receipt(entry);
    }
    let (Some(encoded), Some(digest), Some(operation_id)) = (
        &entry.last_status_b64,
        &entry.last_status_digest,
        &entry.operation_id,
    ) else {
        return None;
    };
    let Ok(bytes) = BASE64.decode(encoded) else {
        return None;
    };
    if hex::encode(Sha256::digest(&bytes)) != *digest {
        return None;
    }
    let Ok(status) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return None;
    };
    if status["operation_id"] != *operation_id {
        return None;
    }
    match status["state"].as_str() {
        Some("failed") => Some("failed"),
        Some("expired") => Some("expired"),
        Some("succeeded") if status["policy_applied"] == true => Some("policy_applied"),
        _ if status["policy_applied"] == true => Some("policy_applied"),
        _ => None,
    }
}

fn expired_unapproved_receipt(entry: &Entry) -> Option<&'static str> {
    let (Some(encoded), Some(digest)) = (&entry.receipt_body_b64, &entry.receipt_digest) else {
        return None;
    };
    let bytes = BASE64.decode(encoded).ok()?;
    if hex::encode(Sha256::digest(&bytes)) != *digest {
        return None;
    }
    let receipt: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    if receipt["operation_id"] != entry.operation_id.as_deref()? {
        return None;
    }
    let proposal_bytes = BASE64
        .decode(receipt["proposal_bytes_b64"].as_str()?)
        .ok()?;
    if hex::encode(Sha256::digest(&proposal_bytes)) != receipt["proposal_digest"] {
        return None;
    }
    let proposal: serde_json::Value = serde_json::from_slice(&proposal_bytes).ok()?;
    if proposal["operation_id"] != entry.operation_id.as_deref()?
        || proposal["proposal_id"] != entry.request_id
    {
        return None;
    }
    let expiry = chrono::DateTime::parse_from_rfc3339(proposal["expires_at"].as_str()?).ok()?;
    (expiry <= Utc::now()).then_some("expired_unapproved")
}

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Journal {
    pub scope: Option<Scope>,
    pub candidates: Vec<Candidate>,
    pub candidates_at: i64,
    pub entries: Vec<Entry>,
    #[serde(default)]
    pub retired: Vec<RetiredEntry>,
}

fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub(super) fn path(base: &Path, scope: &Scope) -> Result<PathBuf, String> {
    let bytes = serde_json::to_vec(scope).map_err(|_| "invalid Rowvia journal scope")?;
    let directory = base.join("rowvia-management");
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("create Rowvia journal directory: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("protect Rowvia journal directory: {error}"))?;
    }
    Ok(directory.join(format!("{}.json", hex::encode(Sha256::digest(bytes)))))
}

pub(super) fn transact<T>(
    path: &Path,
    scope: &Scope,
    mutation: impl FnOnce(&mut Journal) -> Result<(T, bool), String>,
) -> Result<T, String> {
    let _guard = lock().lock().map_err(|_| "Rowvia journal lock poisoned")?;
    let mut journal = match std::fs::metadata(path) {
        Ok(metadata) if metadata.len() <= MAX_FILE_BYTES => {
            let bytes =
                std::fs::read(path).map_err(|error| format!("read Rowvia journal: {error}"))?;
            serde_json::from_slice::<Journal>(&bytes)
                .map_err(|_| "invalid Rowvia journal".to_string())?
        }
        Ok(_) => return Err("Rowvia journal is too large".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Journal::default(),
        Err(error) => return Err(format!("inspect Rowvia journal: {error}")),
    };
    if journal.scope.as_ref().is_some_and(|stored| stored != scope) {
        return Err("Rowvia journal scope mismatch".to_string());
    }
    journal.scope = Some(scope.clone());
    let (result, dirty) = mutation(&mut journal)?;
    if dirty && journal.entries.len() > MAX_ENTRIES {
        let mut excess = journal.entries.len() - MAX_ENTRIES;
        let mut kept = Vec::with_capacity(MAX_ENTRIES);
        for entry in std::mem::take(&mut journal.entries) {
            if excess > 0 {
                if let Some(outcome) = safely_terminal(&entry) {
                    excess -= 1;
                    journal.retired.push(RetiredEntry {
                        logical_request_id: entry.logical_request_id,
                        draft_digest: entry.draft_digest,
                        source_event_id: entry.source_event_id,
                        request_id: entry.request_id,
                        operation_id: entry.operation_id,
                        outcome: outcome.to_string(),
                        retired_at: Utc::now().timestamp(),
                    });
                    continue;
                }
            }
            kept.push(entry);
        }
        journal.entries = kept;
        if journal.retired.len() > MAX_RETIRED {
            journal.retired.drain(..journal.retired.len() - MAX_RETIRED);
        }
    }
    if journal.entries.len() > MAX_ENTRIES || journal.candidates.len() > MAX_CANDIDATES {
        return Err("Rowvia journal capacity reached".to_string());
    }
    if dirty {
        let bytes = serde_json::to_vec(&journal).map_err(|_| "encode Rowvia journal")?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err("Rowvia journal is too large".to_string());
        }
        crate::managed_agents::storage::atomic_write_json_restricted(path, &bytes)?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(index: usize, created_at: i64, refused: bool, applied: bool) -> Entry {
        let operation_id = format!("operation-{index}");
        let status = serde_json::to_vec(&serde_json::json!({
            "operation_id": operation_id,
            "policy_applied": applied,
        }))
        .unwrap();
        Entry {
            created_at,
            logical_request_id: format!("logical-{index}"),
            draft_digest: "draft".into(),
            source_event_id: format!("event-{index}"),
            receipt_handle: format!("receipt-{index}"),
            request_id: format!("request-{index}"),
            request_body_b64: "e30=".into(),
            request_digest: hex::encode(Sha256::digest(b"{}")),
            receipt_body_b64: None,
            receipt_digest: None,
            operation_id: applied.then_some(operation_id),
            approval_intent: applied,
            approval_body_b64: None,
            approval_body_digest: None,
            refused,
            last_status_b64: applied.then(|| BASE64.encode(&status)),
            last_status_digest: applied.then(|| hex::encode(Sha256::digest(&status))),
        }
    }

    fn failed_or_expired(index: usize, created_at: i64, state: &str) -> Entry {
        let mut result = entry(index, created_at, false, false);
        let operation_id = format!("operation-{index}");
        let status = serde_json::to_vec(&serde_json::json!({
            "operation_id": operation_id,
            "state": state,
            "policy_applied": false,
        }))
        .unwrap();
        result.operation_id = Some(operation_id);
        result.approval_intent = true;
        result.last_status_b64 = Some(BASE64.encode(&status));
        result.last_status_digest = Some(hex::encode(Sha256::digest(&status)));
        result
    }

    fn scope(relay: &str) -> Scope {
        Scope {
            owner_pubkey: "a".repeat(64),
            relay_url: relay.to_string(),
            origin: "https://rowvia.example/".to_string(),
            source_instance: "community".to_string(),
            cerberus_pubkey: "b".repeat(64),
        }
    }

    #[test]
    fn journal_survives_restart_and_separates_communities() {
        let base = tempfile::tempdir().unwrap();
        let community_a = scope("wss://a.example");
        let community_b = scope("wss://b.example");
        let a = path(base.path(), &community_a).unwrap();
        let b = path(base.path(), &community_b).unwrap();
        assert_ne!(a, b);
        transact(&a, &community_a, |stored| {
            stored.entries.push(Entry {
                created_at: Utc::now().timestamp(),
                logical_request_id: "event-1".to_string(),
                draft_digest: "draft-digest".to_string(),
                source_event_id: "event-id".to_string(),
                receipt_handle: "receipt-1".to_string(),
                request_id: "request-1".to_string(),
                request_body_b64: "e30=".to_string(),
                request_digest: hex::encode(Sha256::digest(b"{}")),
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
            Ok(((), true))
        })
        .unwrap();
        let found = transact(&a, &community_a, |stored| {
            Ok((stored.entries[0].request_id.clone(), false))
        })
        .unwrap();
        assert_eq!(found, "request-1");
        let empty = transact(&b, &community_b, |stored| Ok((stored.entries.len(), false))).unwrap();
        assert_eq!(empty, 0);
        assert!(transact(&a, &community_b, |_| Ok(((), false))).is_err());
    }

    #[test]
    fn journal_capacity_failure_does_not_replace_prior_snapshot() {
        let base = tempfile::tempdir().unwrap();
        let scope = scope("wss://a.example");
        let file = path(base.path(), &scope).unwrap();
        transact(&file, &scope, |stored| {
            stored.candidates_at = 123;
            Ok(((), true))
        })
        .unwrap();
        assert!(transact(&file, &scope, |stored| {
            stored.candidates = (0..=MAX_CANDIDATES)
                .map(|i| Candidate {
                    selection_handle: i.to_string(),
                    agent_name: "agent".to_string(),
                    target_pubkey: "c".repeat(64),
                    binding_id: "binding".to_string(),
                    gmail_label: "mail".to_string(),
                    binding_status: "active".to_string(),
                })
                .collect();
            Ok(((), true))
        })
        .is_err());
        let count = transact(&file, &scope, |stored| {
            Ok(((stored.candidates_at, stored.candidates.len()), false))
        })
        .unwrap();
        assert_eq!(count, (123, 0));
    }

    #[test]
    fn capacity_retires_only_old_terminal_entries_and_keeps_unresolved_intents() {
        let base = tempfile::tempdir().unwrap();
        let scope = scope("wss://a.example");
        let file = path(base.path(), &scope).unwrap();
        let old = Utc::now().timestamp() - RETIRE_AFTER_SECONDS - 1;
        transact(&file, &scope, |stored| {
            stored.entries.push(entry(0, old, true, false));
            stored.entries.push(entry(1, old, false, true));
            for index in 2..MAX_ENTRIES {
                stored.entries.push(entry(index, old, false, false));
            }
            Ok(((), true))
        })
        .unwrap();
        for index in MAX_ENTRIES..MAX_ENTRIES + 2 {
            transact(&file, &scope, |stored| {
                stored.entries.push(entry(index, Utc::now().timestamp(), false, false));
                Ok(((), true))
            })
            .unwrap();
        }
        let ids = transact(&file, &scope, |stored| {
            Ok((
                stored
                    .entries
                    .iter()
                    .map(|entry| entry.logical_request_id.clone())
                    .collect::<Vec<_>>(),
                false,
            ))
        })
        .unwrap();
        assert_eq!(ids.len(), MAX_ENTRIES);
        assert!(!ids.contains(&"logical-0".to_string()));
        assert!(!ids.contains(&"logical-1".to_string()));
        assert!(ids.contains(&"logical-2".to_string()));
        assert!(ids.contains(&format!("logical-{}", MAX_ENTRIES + 1)));
        assert!(transact(&file, &scope, |stored| {
            stored.entries.push(entry(MAX_ENTRIES + 2, Utc::now().timestamp(), false, false));
            Ok(((), true))
        })
        .is_err());
        let count = transact(&file, &scope, |stored| Ok((stored.entries.len(), false))).unwrap();
        assert_eq!(count, MAX_ENTRIES);
    }

    #[test]
    fn failed_and_expired_operations_do_not_permanently_fill_the_journal() {
        let base = tempfile::tempdir().unwrap();
        let scope = scope("wss://a.example");
        let file = path(base.path(), &scope).unwrap();
        let old = Utc::now().timestamp() - RETIRE_AFTER_SECONDS - 1;
        transact(&file, &scope, |stored| {
            for index in 0..MAX_ENTRIES {
                let state = if index % 2 == 0 { "failed" } else { "expired" };
                stored.entries.push(failed_or_expired(index, old, state));
            }
            Ok(((), true))
        })
        .unwrap();
        transact(&file, &scope, |stored| {
            stored.entries.push(entry(MAX_ENTRIES, Utc::now().timestamp(), false, false));
            Ok(((), true))
        })
        .unwrap();
        let (active, retired) = transact(&file, &scope, |stored| {
            Ok(((stored.entries.len(), stored.retired.len()), false))
        })
        .unwrap();
        assert_eq!(active, MAX_ENTRIES);
        assert_eq!(retired, 1);
        let outcome = transact(&file, &scope, |stored| {
            Ok((stored.retired[0].outcome.clone(), false))
        })
        .unwrap();
        assert_eq!(outcome, "failed");
    }

    #[test]
    fn expired_unapproved_receipt_can_retire_but_unresolved_request_cannot() {
        let old = Utc::now().timestamp() - RETIRE_AFTER_SECONDS - 1;
        let mut expired = entry(0, old, false, false);
        let canonical = br#"{"proposal_id":"request-0","operation_id":"operation-0","expires_at":"2020-01-01T00:00:00Z"}"#;
        let receipt = serde_json::to_vec(&serde_json::json!({
            "operation_id": "operation-0",
            "proposal_digest": hex::encode(Sha256::digest(canonical)),
            "proposal_bytes_b64": BASE64.encode(canonical),
        }))
        .unwrap();
        expired.operation_id = Some("operation-0".into());
        expired.receipt_body_b64 = Some(BASE64.encode(&receipt));
        expired.receipt_digest = Some(hex::encode(Sha256::digest(&receipt)));
        assert_eq!(safely_terminal(&expired), Some("expired_unapproved"));
        expired.receipt_body_b64 = None;
        assert_eq!(safely_terminal(&expired), None);
    }
}
