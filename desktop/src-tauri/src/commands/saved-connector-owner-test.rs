//! Closed, opt-in projection for the real saved-developer Gmail review UI.
//! No proposal creation, approval, signer, transport or journal mutation here.

use super::*;
use serde_json::{json, Map, Value};
use tauri::Manager;

const OWNER: &str = "89af092923ad49e3b9916b0c9f28c6b9237d8fcab4461cb7ddd12afe354f0d68";
const RELAY: &str = "wss://buzz.rowvia.ai:8443";
const TARGET: &str = "29cc1bb0ac338520c263dcfe654764224018a1ff5e5d3fc633be73f71f0c8c78";
const ACCOUNT: &str = "jiri.hroza@gmail.com";

pub(crate) fn is_operation(operation: &str) -> bool {
    matches!(
        operation,
        "inspect_saved_connector"
            | "prepare_saved_connector"
            | "approve_saved_connector"
            | "inspect_saved_connector_operation"
    )
}

pub(crate) fn validate_arguments(operation: &str, args: &Map<String, Value>) -> Result<(), String> {
    match operation {
        "inspect_saved_connector" | "prepare_saved_connector" if args.is_empty() => Ok(()),
        "approve_saved_connector" | "inspect_saved_connector_operation"
            if args.len() == 1
                && args
                    .get("fingerprint")
                    .and_then(Value::as_str)
                    .is_some_and(|value| {
                        value.len() == 64
                            && value
                                .bytes()
                                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    }) =>
        {
            Ok(())
        }
        _ => Err("invalid_request".into()),
    }
}

fn receipt_projection(entry: &Entry, scope: &Scope, fresh: bool) -> Result<Option<Value>, String> {
    let (receipt, expiry) = decode_entry_receipt_with_freshness(entry, scope, fresh)?;
    let bytes = BASE64
        .decode(&receipt.proposal_bytes_b64)
        .map_err(|_| "not_ready")?;
    let body: Value = serde_json::from_slice(&bytes).map_err(|_| "not_ready")?;
    if body["target_pubkey"] != TARGET
        || body["constrained_resource"] != format!("gmail-account:{ACCOUNT}")
        || body["action"] != "connector.grant"
    {
        return Ok(None);
    }
    let binding_id = body["binding_id"].as_str().ok_or("not_ready")?;
    let agent_name = body["agent_name"].as_str().ok_or("not_ready")?;
    // Hash the immutable native scope and exact journal/receipt identities, not
    // reserialized canonical bytes. Approval intent/status deliberately do not
    // change this token, so a timed-out decision can still be inspected safely.
    let fingerprint = hex::encode(Sha256::digest(
        serde_json::to_vec(&json!([
            "rowvia.saved-connector/v1",
            scope,
            entry.receipt_handle,
            entry.request_digest,
            entry.receipt_digest,
            entry.source_event_id,
            receipt.proposal_id,
            receipt.operation_id,
            receipt.proposal_digest,
            expiry
        ]))
        .map_err(|_| "not_ready")?,
    ));
    Ok(Some(json!({
        "stage": if entry.approval_intent { "operation" } else { "review" },
        "fingerprint": fingerprint, "receiptHandle": entry.receipt_handle,
        "proposalId": receipt.proposal_id, "operationId": receipt.operation_id,
        "proposalDigest": receipt.proposal_digest, "expiresAt": expiry,
        "agentName": agent_name, "bindingId": binding_id,
        "approvalIntent": entry.approval_intent,
        // Journal provenance proves ordinary native admission; it does not
        // retain a channel and must not imply historical fixed-channel proof.
        "sourceEventId": entry.source_event_id,
    })))
}

fn project(
    stored: &journal::Journal,
    scope: &Scope,
    operation: &str,
    args: &Map<String, Value>,
    now: i64,
) -> Result<Map<String, Value>, String> {
    validate_arguments(operation, args)?;
    if scope.owner_pubkey != OWNER || scope.relay_url != RELAY {
        return Err("scope_mismatch".into());
    }
    let status = operation == "inspect_saved_connector_operation";
    let prepare = operation == "prepare_saved_connector";
    let wanted = args.get("fingerprint").and_then(Value::as_str);
    let mut matches = Vec::new();
    if !prepare {
        for entry in &stored.entries {
            if entry.refused
                || entry.receipt_body_b64.is_none()
                || (!status && entry.approval_intent)
            {
                continue;
            }
            // Unrelated expired receipts must not prevent a fresh target review.
            let value = match receipt_projection(entry, scope, !status) {
                Ok(Some(value)) => value,
                Ok(None) => continue,
                Err(error) if error == "Rowvia proposal has expired" => continue,
                Err(_) => return Err("not_ready".into()),
            };
            if wanted.is_none_or(|fingerprint| value["fingerprint"] == fingerprint) {
                matches.push(value);
            }
        }
    }
    let mut result = if matches.len() == 1 {
        let value = matches.pop().ok_or("not_ready")?;
        if status && value["approvalIntent"] != true {
            return Err("not_ready".into());
        }
        value.as_object().cloned().ok_or("not_ready")?
    } else if !matches.is_empty() || wanted.is_some() || status {
        return Err("not_ready".into());
    } else {
        if now < stored.candidates_at || now - stored.candidates_at > CANDIDATE_TTL_SECONDS {
            return Err("not_ready".into());
        }
        let candidates: Vec<_> = stored
            .candidates
            .iter()
            .filter(|candidate| {
                candidate.target_pubkey == TARGET
                    && candidate.gmail_label == ACCOUNT
                    && candidate.binding_status == "active"
            })
            .collect();
        if candidates.len() != 1 {
            return Err("not_ready".into());
        }
        let candidate = candidates[0];
        json!({"stage": "selection", "selectionHandle": candidate.selection_handle, "agentName": candidate.agent_name}).as_object().cloned().ok_or("not_ready")?
    };
    result.extend(json!({"ownerPubkey": OWNER, "relayUrl": RELAY, "targetPubkey": TARGET, "gmailLabel": ACCOUNT, "cerberusPubkey": scope.cerberus_pubkey}).as_object().cloned().ok_or("not_ready")?);
    Ok(result)
}

pub(crate) fn execute(
    app: &AppHandle,
    operation: &str,
    args: &Map<String, Value>,
) -> Result<Map<String, Value>, String> {
    let state = app.state::<AppState>();
    // Workspace apply replaces keys without identity_mutation. Never wait for
    // it while holding another lock, nor retain a mutex across the UI/HTTP path.
    let _workspace = state
        .workspace_apply_lock
        .try_lock()
        .map_err(|_| "not_ready")?;
    let _identity = state
        .identity_mutation
        .try_lock()
        .map_err(|_| "not_ready")?;
    if state
        .reset_failed
        .load(std::sync::atomic::Ordering::Acquire)
        || state
            .shutdown_started
            .load(std::sync::atomic::Ordering::Acquire)
    {
        return Err("not_ready".into());
    }
    let client = ManagementClient::configured().map_err(|_| "not_ready")?;
    let (scope, _) = capture_scope(app, &state, &client).map_err(|_| "not_ready")?;
    if scope.owner_pubkey != OWNER || scope.relay_url != RELAY {
        return Err("scope_mismatch".into());
    }
    let path = journal_path(app, &scope).map_err(|_| "not_ready")?;
    let result = journal::transact(&path, &scope, |stored| {
        Ok((
            project(stored, &scope, operation, args, Utc::now().timestamp())?,
            false,
        ))
    })?;
    assert_scope(app, &state, &client, &scope).map_err(|_| "scope_mismatch")?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> Scope {
        Scope {
            owner_pubkey: OWNER.into(),
            relay_url: RELAY.into(),
            origin: "https://management.example/".into(),
            source_instance: "test-source".into(),
            cerberus_pubkey: "a".repeat(64),
        }
    }
    fn candidate() -> Candidate {
        Candidate {
            selection_handle: uuid::Uuid::new_v4().to_string(),
            agent_name: "saved-developer".into(),
            target_pubkey: TARGET.into(),
            binding_id: uuid::Uuid::new_v4().to_string(),
            gmail_label: ACCOUNT.into(),
            binding_status: "active".into(),
        }
    }

    fn entry(expiry: &str) -> Entry {
        let request = ManagementProposalRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            action: ManagementAction::ConnectorGrant,
            target_agent_pubkey: TARGET.into(),
            binding_id: uuid::Uuid::new_v4().to_string(),
            operations: OPERATIONS.map(str::to_string).to_vec(),
        };
        let operation_id = uuid::Uuid::new_v4().to_string();
        let canonical = serde_json::to_vec(&json!({ "version": 1, "proposal_id": request.request_id, "operation_id": operation_id, "expires_at": expiry, "owner_pubkey": OWNER, "target_pubkey": TARGET, "action": "connector.grant", "source_instance_id": scope().source_instance, "operations": OPERATIONS, "binding_id": request.binding_id, "agent_name": "saved-developer", "constrained_resource": format!("gmail-account:{ACCOUNT}") })).unwrap();
        let receipt = ManagementProposalResponse {
            proposal_id: request.request_id.clone(),
            operation_id: operation_id.clone(),
            proposal_digest: hex::encode(Sha256::digest(&canonical)),
            proposal_bytes_b64: BASE64.encode(&canonical),
        };
        let request_bytes = serde_json::to_vec(&request).unwrap();
        let receipt_bytes = serde_json::to_vec(&receipt).unwrap();
        Entry {
            created_at: Utc::now().timestamp(),
            logical_request_id: uuid::Uuid::new_v4().to_string(),
            draft_digest: "d".repeat(64),
            source_event_id: "e".repeat(64),
            receipt_handle: uuid::Uuid::new_v4().to_string(),
            request_id: request.request_id,
            request_body_b64: BASE64.encode(&request_bytes),
            request_digest: hex::encode(Sha256::digest(&request_bytes)),
            receipt_body_b64: Some(BASE64.encode(&receipt_bytes)),
            receipt_digest: Some(hex::encode(Sha256::digest(&receipt_bytes))),
            operation_id: Some(operation_id),
            approval_intent: false,
            approval_body_b64: None,
            approval_body_digest: None,
            refused: false,
            last_status_b64: None,
            last_status_digest: None,
        }
    }

    #[test]
    fn owner_test_saved_connector_uses_exact_production_receipt_decode_and_stable_recovery_token() {
        let mut stored = journal::Journal {
            entries: vec![entry(
                &(Utc::now() + chrono::Duration::minutes(2)).to_rfc3339(),
            )],
            ..Default::default()
        };
        let inspected = project(
            &stored,
            &scope(),
            "inspect_saved_connector",
            &Map::new(),
            100,
        )
        .unwrap();
        let args = json!({"fingerprint": inspected["fingerprint"]})
            .as_object()
            .unwrap()
            .clone();
        assert_eq!(
            project(&stored, &scope(), "approve_saved_connector", &args, 100).unwrap()
                ["receiptHandle"],
            stored.entries[0].receipt_handle
        );
        let bytes_before = serde_json::to_vec(&stored).unwrap();
        stored.entries[0].approval_intent = true;
        assert!(project(&stored, &scope(), "approve_saved_connector", &args, 100).is_err());
        assert_eq!(
            project(
                &stored,
                &scope(),
                "inspect_saved_connector_operation",
                &args,
                100
            )
            .unwrap()["fingerprint"],
            inspected["fingerprint"]
        );
        stored.entries[0].approval_intent = false;
        assert_eq!(
            serde_json::to_vec(&stored).unwrap(),
            bytes_before,
            "projection cannot rewrite journal rows"
        );
        let original = stored.entries[0].request_digest.clone();
        stored.entries[0].request_digest = "0".repeat(64);
        assert!(project(&stored, &scope(), "approve_saved_connector", &args, 100).is_err());
        stored.entries[0].request_digest = original;
        stored.entries.push(stored.entries[0].clone());
        assert!(project(&stored, &scope(), "approve_saved_connector", &args, 100).is_err());
        stored.entries = vec![entry("2000-01-01T00:00:00Z")];
        let expired = receipt_projection(&stored.entries[0], &scope(), false)
            .unwrap()
            .unwrap();
        let expired_args = json!({"fingerprint": expired["fingerprint"]})
            .as_object()
            .unwrap()
            .clone();
        assert!(project(
            &stored,
            &scope(),
            "approve_saved_connector",
            &expired_args,
            100
        )
        .is_err());
        stored.entries[0].approval_intent = true;
        assert!(project(
            &stored,
            &scope(),
            "inspect_saved_connector_operation",
            &expired_args,
            100
        )
        .is_ok());
        stored.entries[0].refused = true;
        assert!(project(
            &stored,
            &scope(),
            "inspect_saved_connector_operation",
            &expired_args,
            100
        )
        .is_err());
    }

    #[test]
    fn owner_test_saved_connector_protocol_and_native_candidate_are_closed() {
        for operation in ["inspect_saved_connector", "prepare_saved_connector"] {
            assert!(validate_arguments(operation, &Map::new()).is_ok());
            assert!(
                validate_arguments(operation, json!({"target": TARGET}).as_object().unwrap())
                    .is_err()
            );
        }
        for operation in [
            "approve_saved_connector",
            "inspect_saved_connector_operation",
        ] {
            assert!(validate_arguments(
                operation,
                json!({"fingerprint": "a".repeat(64)}).as_object().unwrap()
            )
            .is_ok());
            assert!(validate_arguments(
                operation,
                json!({"fingerprint": "A".repeat(64)}).as_object().unwrap()
            )
            .is_err());
        }
        let mut stored = journal::Journal {
            candidates_at: 100,
            candidates: vec![candidate()],
            ..Default::default()
        };
        assert_eq!(
            project(
                &stored,
                &scope(),
                "prepare_saved_connector",
                &Map::new(),
                101
            )
            .unwrap()["stage"],
            "selection"
        );
        assert!(project(
            &stored,
            &scope(),
            "prepare_saved_connector",
            &Map::new(),
            221
        )
        .is_err());
        stored.candidates.push(candidate());
        assert!(project(
            &stored,
            &scope(),
            "prepare_saved_connector",
            &Map::new(),
            101
        )
        .is_err());
        stored.candidates.truncate(1);
        stored.candidates[0].target_pubkey = "b".repeat(64);
        assert!(project(
            &stored,
            &scope(),
            "prepare_saved_connector",
            &Map::new(),
            101
        )
        .is_err());
        stored.candidates[0] = candidate();
        stored.candidates[0].gmail_label = "other@gmail.com".into();
        assert!(project(
            &stored,
            &scope(),
            "prepare_saved_connector",
            &Map::new(),
            101
        )
        .is_err());
        let mut foreign = scope();
        foreign.owner_pubkey = "b".repeat(64);
        assert!(project(
            &stored,
            &foreign,
            "prepare_saved_connector",
            &Map::new(),
            101
        )
        .is_err());
    }
}
