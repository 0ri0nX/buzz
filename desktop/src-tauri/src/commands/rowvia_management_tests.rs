use super::*;
use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::{header, Response, StatusCode},
    response::IntoResponse,
    Router,
};
use base64::engine::general_purpose::STANDARD as BASE64;
use nostr::{EventBuilder, JsonUtil, Kind};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

#[test]
fn forged_roster_signed_by_other_key_is_rejected() {
    let relay = Keys::generate();
    let attacker = Keys::generate();
    let trusted = EventBuilder::new(Kind::Custom(39002), "{}")
        .sign_with_keys(&relay)
        .unwrap();
    let forged = EventBuilder::new(Kind::Custom(39002), "{}")
        .sign_with_keys(&attacker)
        .unwrap();
    assert!(roster_signer_matches_relay(
        &trusted,
        &relay.public_key().to_hex()
    ));
    assert!(!roster_signer_matches_relay(
        &forged,
        &relay.public_key().to_hex()
    ));
}

#[test]
fn connector_frame_rejects_duplicate_known_and_unknown_authority_fields() {
    let valid = r#"{"seq":0,"timestamp":"2026-09-25T00:00:00Z","kind":"agent_management_request","agentIndex":null,"channelId":"00000000-0000-4000-8000-000000000000","sessionId":null,"turnId":null,"payload":{"type":"agent_management_request","version":1,"action":"connector.grant","requestId":"00000000-0000-4000-8000-000000000001","request":{"channelId":"00000000-0000-4000-8000-000000000000","targetName":"Target"}}}"#;
    assert!(serde_json::from_str::<ConnectorFrame>(valid).is_ok());
    let duplicate_action = valid.replace(
        "\"action\":\"connector.grant\",",
        "\"action\":\"connector.grant\",\"action\":\"connector.revoke\",",
    );
    assert!(serde_json::from_str::<ConnectorFrame>(&duplicate_action).is_err());
    let duplicate_target = valid.replace(
        "\"targetName\":\"Target\"",
        "\"targetName\":\"Target\",\"targetName\":\"Other\"",
    );
    assert!(serde_json::from_str::<ConnectorFrame>(&duplicate_target).is_err());
    let unknown = valid.replace(
        "\"targetName\":\"Target\"",
        "\"targetName\":\"Target\",\"proposal_bytes_b64\":\"forged\"",
    );
    assert!(serde_json::from_str::<ConnectorFrame>(&unknown).is_err());
}

#[test]
fn native_receipt_must_match_frozen_scope_and_create_body() {
    let scope = Scope {
        owner_pubkey: "a".repeat(64),
        relay_url: "wss://community.example".to_string(),
        origin: "https://rowvia.example/".to_string(),
        source_instance: "community".to_string(),
        cerberus_pubkey: "c".repeat(64),
    };
    let request = ManagementProposalRequest {
        request_id: "0199e8af-62c2-7000-8000-000000000001".to_string(),
        action: ManagementAction::ConnectorGrant,
        target_agent_pubkey: "b".repeat(64),
        binding_id: "0199e8af-62c2-7000-8000-000000000002".to_string(),
        operations: OPERATIONS.map(str::to_string).to_vec(),
    };
    let canonical = serde_json::json!({
        "version": 1,
        "proposal_id": request.request_id.clone(),
        "operation_id": "0199e8af-62c2-7000-8000-000000000003",
        "owner_pubkey": scope.owner_pubkey.clone(),
        "source_instance_id": scope.source_instance.clone(),
        "action": "connector.grant",
        "target_pubkey": request.target_agent_pubkey.clone(),
        "binding_id": request.binding_id.clone(),
        "operations": OPERATIONS,
        "expires_at": "2030-01-01T00:00:00Z"
    });
    let bytes = serde_json::to_vec(&canonical).unwrap();
    let mut receipt = ManagementProposalResponse {
        proposal_id: request.request_id.clone(),
        operation_id: "0199e8af-62c2-7000-8000-000000000003".to_string(),
        proposal_digest: hex::encode(Sha256::digest(&bytes)),
        proposal_bytes_b64: BASE64.encode(&bytes),
    };
    assert!(verify_receipt_against_request(&receipt, &request, &scope).is_ok());
    let mut other_scope = scope.clone();
    other_scope.owner_pubkey = "d".repeat(64);
    assert!(verify_receipt_against_request(&receipt, &request, &other_scope).is_err());
    receipt.operation_id = "0199e8af-62c2-7000-8000-000000000004".to_string();
    assert!(verify_receipt_against_request(&receipt, &request, &scope).is_err());
}

#[test]
fn expired_receipt_remains_verifiable_for_operation_recovery() {
    let bytes = br#"{"proposal_id":"proposal-1","operation_id":"operation-1","expires_at":"2020-01-01T00:00:00Z"}"#;
    let digest = hex::encode(Sha256::digest(bytes));
    let encoded = BASE64.encode(bytes);
    assert_eq!(
        verify_canonical_proposal_with_freshness(
            &encoded,
            &digest,
            "proposal-1",
            "operation-1",
            false,
        )
        .unwrap(),
        "2020-01-01T00:00:00Z"
    );
    assert!(verify_canonical_proposal(&encoded, &digest, "proposal-1", "operation-1").is_err());
    assert!(verify_canonical_proposal_with_freshness(
        &encoded,
        &"0".repeat(64),
        "proposal-1",
        "operation-1",
        false,
    )
    .is_err());
}

#[test]
fn bridge_request_replay_reuses_frozen_body_across_new_signed_delivery() {
    let mut request = ManagementProposalRequest {
        request_id: "0199e8af-62c2-7000-8000-000000000001".to_string(),
        action: ManagementAction::ConnectorGrant,
        target_agent_pubkey: "b".repeat(64),
        binding_id: "0199e8af-62c2-7000-8000-000000000002".to_string(),
        operations: OPERATIONS.map(str::to_string).to_vec(),
    };
    let body = serde_json::to_vec(&request).unwrap();
    let entry = Entry {
        created_at: Utc::now().timestamp(),
        logical_request_id: "draft-uuid".to_string(),
        draft_digest: "unchanged-draft".to_string(),
        source_event_id: "first-event-id".to_string(),
        receipt_handle: "receipt".to_string(),
        request_id: request.request_id.clone(),
        request_body_b64: BASE64.encode(&body),
        request_digest: hex::encode(Sha256::digest(&body)),
        receipt_body_b64: None,
        receipt_digest: None,
        operation_id: None,
        approval_intent: false,
        approval_body_b64: None,
        approval_body_digest: None,
        refused: false,
        last_status_b64: None,
        last_status_digest: None,
    };
    // The caller has a new signed event and tentative UUIDv7; neither changes
    // the persisted create bytes for the same scoped draft ID and intent.
    request.request_id = "0199e8af-62c2-7000-8000-000000000099".to_string();
    assert_eq!(
        frozen_replay_body(&entry, "unchanged-draft", &request).unwrap(),
        body
    );
    assert!(frozen_replay_body(&entry, "changed-draft", &request).is_err());
    request.binding_id = "0199e8af-62c2-7000-8000-000000000098".to_string();
    assert!(frozen_replay_body(&entry, "unchanged-draft", &request).is_err());
}

#[tokio::test]
async fn unresolved_proposal_survives_restart_and_retries_one_logical_operation() {
    let (client, fixture) = fixture(5, Duration::from_secs(2)).await;
    let keys = Keys::generate();
    let scope = Scope {
        owner_pubkey: "a".repeat(64),
        relay_url: "wss://community.example".to_string(),
        origin: client.origin.as_str().to_string(),
        source_instance: "community".to_string(),
        cerberus_pubkey: "c".repeat(64),
    };
    let request = ManagementProposalRequest {
        request_id: "0199e8af-62c2-7000-8000-000000000001".to_string(),
        action: ManagementAction::ConnectorGrant,
        target_agent_pubkey: "b".repeat(64),
        binding_id: "0199e8af-62c2-7000-8000-000000000002".to_string(),
        operations: OPERATIONS.map(str::to_string).to_vec(),
    };
    let frozen = serde_json::to_vec(&request).unwrap();
    let base = tempfile::tempdir().unwrap();
    let path = journal::path(base.path(), &scope).unwrap();
    let handle = "0199e8af-62c2-7000-8000-000000000004";
    journal::transact(&path, &scope, |stored| {
        stored.entries.push(Entry {
            created_at: Utc::now().timestamp(),
            logical_request_id: "draft-1".to_string(),
            draft_digest: "draft-digest".to_string(),
            source_event_id: "event-1".to_string(),
            receipt_handle: handle.to_string(),
            request_id: request.request_id.clone(),
            request_body_b64: BASE64.encode(&frozen),
            request_digest: hex::encode(Sha256::digest(&frozen)),
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
    assert!(submit_frozen_proposal(
        &client,
        &keys,
        &path,
        &scope,
        handle,
        frozen.clone(),
        || Ok(())
    )
    .await
    .is_err());

    // The next read uses the durable file, as a new process would after restart.
    let pending = pending_from_journal(&path, &scope).unwrap();
    assert_eq!(pending.receipts.len(), 0);
    assert_eq!(pending.unresolved.len(), 1);
    assert_eq!(pending.unresolved[0].receipt_handle, handle);
    let visible = serde_json::to_string(&pending).unwrap();
    assert!(!visible.contains(&request.request_id));
    assert!(!visible.contains(&BASE64.encode(&frozen)));
    let recovered = journal::transact(&path, &scope, |stored| {
        Ok((unresolved_request(&stored.entries[0])?, false))
    })
    .unwrap();
    assert_eq!(recovered.0, frozen);
    let receipt = submit_frozen_proposal(
        &client,
        &keys,
        &path,
        &scope,
        handle,
        recovered.0,
        || Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(receipt.proposal.proposal_id, request.request_id);
    let pending = pending_from_journal(&path, &scope).unwrap();
    assert!(pending.unresolved.is_empty());
    assert_eq!(pending.receipts.len(), 1);
    let seen = fixture.seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].body, seen[1].body);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&seen[0].body).unwrap()["request_id"],
        request.request_id
    );
    for attempt in seen.iter() {
        assert_signed(attempt, &keys);
    }
    assert_eq!(fixture.operations.lock().unwrap().len(), 1);
}

#[derive(Clone)]
struct Seen {
    path: String,
    method: String,
    body: Vec<u8>,
    auth: String,
}

#[derive(Default)]
struct Fixture {
    seen: Mutex<Vec<Seen>>,
    operations: Mutex<std::collections::HashSet<String>>,
    mode: std::sync::atomic::AtomicU8,
}

async fn serve(State(fixture): State<Arc<Fixture>>, request: Request) -> impl IntoResponse {
    let path = request.uri().path().to_string();
    let method = request.method().to_string();
    let auth = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = to_bytes(request.into_body(), MAX_REQUEST_BYTES + 1)
        .await
        .unwrap()
        .to_vec();
    if fixture.mode.load(std::sync::atomic::Ordering::Relaxed) == 5 && method == "GET" {
        return Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "candidates": [{
                        "agent_name": "Target",
                        "target_pubkey": "b".repeat(64),
                        "binding_id": "0199e8af-62c2-7000-8000-000000000002",
                        "gmail_label": "Primary",
                        "binding_status": "active"
                    }],
                    "truncated": false
                })
                .to_string(),
            ))
            .unwrap();
    }
    let attempt = {
        let mut seen = fixture.seen.lock().unwrap();
        seen.push(Seen {
            path: path.clone(),
            method: method.clone(),
            body: body.clone(),
            auth,
        });
        seen.len()
    };
    let mode = fixture.mode.load(std::sync::atomic::Ordering::Relaxed);
    if (mode == 6 || mode == 7) && method == "GET" && path.ends_with("/operations/operation-1") {
        let state = if mode == 6 { "proposed" } else { "accepted" };
        return Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({"operation_id": "operation-1", "state": state}).to_string(),
            ))
            .unwrap();
    }
    if (mode == 6 || mode == 8) && method == "POST" && path.ends_with("/approve") {
        if mode == 8 {
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        return Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({"operation_id": "operation-1", "state": "accepted"})
                    .to_string(),
            ))
            .unwrap();
    }
    match mode {
        5 => {
            let request: ManagementProposalRequest = serde_json::from_slice(&body).unwrap();
            fixture
                .operations
                .lock()
                .unwrap()
                .insert(request.request_id.clone());
            if attempt == 1 {
                // The server records the operation, but the receipt bytes are lost.
                return Response::builder().body(Body::empty()).unwrap();
            }
            let canonical = serde_json::json!({
                "version": 1,
                "proposal_id": request.request_id.clone(),
                "operation_id": "0199e8af-62c2-7000-8000-000000000003",
                "owner_pubkey": "a".repeat(64),
                "source_instance_id": "community",
                "action": "connector.grant",
                "target_pubkey": request.target_agent_pubkey,
                "binding_id": request.binding_id,
                "operations": OPERATIONS,
                "expires_at": "2030-01-01T00:00:00Z",
            });
            let canonical = serde_json::to_vec(&canonical).unwrap();
            let receipt = serde_json::json!({
                "proposal_id": request.request_id,
                "operation_id": "0199e8af-62c2-7000-8000-000000000003",
                "proposal_digest": hex::encode(Sha256::digest(&canonical)),
                "proposal_bytes_b64": BASE64.encode(&canonical),
            });
            Response::builder()
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(receipt.to_string()))
                .unwrap()
        }
        1 => Response::builder()
            .status(StatusCode::FOUND)
            .header(header::LOCATION, "https://different.example/steal")
            .body(Body::empty())
            .unwrap(),
        2 => {
            tokio::time::sleep(Duration::from_millis(150)).await;
            Response::new(Body::from("{}"))
        }
        3 => Response::new(Body::from(vec![b'x'; MAX_RESPONSE_BYTES + 1])),
        _ => {
            let canonical = if fixture.mode.load(std::sync::atomic::Ordering::Relaxed) == 4 {
                br#"{"proposal_id":"proposal-1","operation_id":"operation-1","expires_at":"2020-01-01T00:00:00Z","server":true}"#.as_slice()
            } else {
                br#"{"proposal_id":"proposal-1","operation_id":"operation-1","expires_at":"2030-01-01T00:00:00Z","server":true}"#.as_slice()
            };
            let receipt = serde_json::json!({
                "proposal_id": "proposal-1",
                "operation_id": "operation-1",
                "proposal_digest": hex::encode(Sha256::digest(canonical)),
                "proposal_bytes_b64": BASE64.encode(canonical),
            });
            Response::builder()
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(receipt.to_string()))
                .unwrap()
        }
    }
}

async fn fixture(mode: u8, timeout: Duration) -> (ManagementClient, Arc<Fixture>) {
    let fixture = Arc::new(Fixture::default());
    fixture
        .mode
        .store(mode, std::sync::atomic::Ordering::Relaxed);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new().fallback(serve).with_state(fixture.clone());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (
        ManagementClient::new(&origin, true, timeout).unwrap(),
        fixture,
    )
}

fn proposal() -> ManagementProposalRequest {
    ManagementProposalRequest {
        request_id: "stable-request-1".to_string(),
        action: ManagementAction::ConnectorGrant,
        target_agent_pubkey: "b".repeat(64),
        binding_id: "binding-1".to_string(),
        operations: vec!["read".to_string(), "write".to_string()],
    }
}

fn assert_signed(seen: &Seen, keys: &Keys) {
    let encoded = seen.auth.strip_prefix("Nostr ").unwrap();
    let bytes = BASE64.decode(encoded).unwrap();
    let event = nostr::Event::from_json(&bytes).unwrap();
    event.verify().unwrap();
    assert_eq!(event.pubkey, keys.public_key());
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let tags = value["tags"].as_array().unwrap();
    let tag = |key: &str| {
        tags.iter().find(|tag| tag[0] == key).unwrap()[1]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(
        tag("u"),
        format!("http://127.0.0.1:{}{}", seen_port(&tag("u")), seen.path)
    );
    assert_eq!(tag("method"), seen.method);
    assert_eq!(tag("payload"), hex::encode(Sha256::digest(&seen.body)));
    assert!(!tag("nonce").is_empty());
    assert_eq!(value["kind"], 27235);
}

fn journaled_approval(base: &std::path::Path) -> (Scope, std::path::PathBuf, Vec<u8>) {
    let scope = Scope {
        owner_pubkey: "a".repeat(64),
        relay_url: "wss://community.example".into(),
        origin: "https://rowvia.example/".into(),
        source_instance: "community".into(),
        cerberus_pubkey: "c".repeat(64),
    };
    let path = journal::path(base, &scope).unwrap();
    let request = ManagementProposalRequest {
        request_id: "proposal-1".into(),
        action: ManagementAction::ConnectorGrant,
        target_agent_pubkey: "b".repeat(64),
        binding_id: "binding-1".into(),
        operations: OPERATIONS.map(str::to_string).to_vec(),
    };
    let request_body = serde_json::to_vec(&request).unwrap();
    let canonical = serde_json::to_vec(&serde_json::json!({
        "version": 1,
        "proposal_id": request.request_id,
        "operation_id": "operation-1",
        "owner_pubkey": scope.owner_pubkey,
        "source_instance_id": scope.source_instance,
        "action": "connector.grant",
        "target_pubkey": request.target_agent_pubkey,
        "binding_id": request.binding_id,
        "operations": OPERATIONS,
        "expires_at": "2030-01-01T00:00:00Z"
    }))
    .unwrap();
    let receipt = ManagementProposalResponse {
        proposal_id: "proposal-1".into(),
        operation_id: "operation-1".into(),
        proposal_digest: hex::encode(Sha256::digest(&canonical)),
        proposal_bytes_b64: BASE64.encode(&canonical),
    };
    let receipt_body = serde_json::to_vec(&receipt).unwrap();
    let body = approval_body(
        &ManagementApprovalRequest {
            proposal_id: receipt.proposal_id.clone(),
            operation_id: receipt.operation_id.clone(),
            proposal_digest: receipt.proposal_digest.clone(),
            proposal_bytes_b64: receipt.proposal_bytes_b64.clone(),
            expires_at: "2030-01-01T00:00:00Z".into(),
        },
        true,
    )
    .unwrap();
    journal::transact(&path, &scope, |stored| {
        stored.entries.push(Entry {
            created_at: Utc::now().timestamp(),
            logical_request_id: "draft-1".into(),
            draft_digest: "draft-digest".into(),
            source_event_id: "source-event".into(),
            receipt_handle: "receipt-1".into(),
            request_id: "proposal-1".into(),
            request_body_b64: BASE64.encode(&request_body),
            request_digest: hex::encode(Sha256::digest(&request_body)),
            receipt_body_b64: Some(BASE64.encode(&receipt_body)),
            receipt_digest: Some(hex::encode(Sha256::digest(&receipt_body))),
            operation_id: Some("operation-1".into()),
            approval_intent: true,
            approval_body_b64: Some(BASE64.encode(&body)),
            approval_body_digest: Some(hex::encode(Sha256::digest(&body))),
            refused: false,
            last_status_b64: None,
            last_status_digest: None,
        });
        Ok(((), true))
    })
    .unwrap();
    (scope, path, body)
}

#[tokio::test]
async fn approval_timeout_recovers_exact_body_with_fresh_proof_after_status_check() {
    let base = tempfile::tempdir().unwrap();
    let (scope, path, body) = journaled_approval(base.path());
    let (client, fixture) = fixture(8, Duration::from_millis(40)).await;
    let keys = Keys::generate();
    // The durable intent exists before any send. A lost response leaves it
    // intact for the next process to reconcile by operation ID.
    let recovered = journal::transact(&path, &scope, |stored| {
        Ok((frozen_approval_body(&stored.entries[0], &scope)?, false))
    })
    .unwrap();
    assert_eq!(recovered.1, body);
    assert!(client
        .post_approval_body(&keys, "proposal-1", body.clone())
        .await
        .is_err());
    fixture
        .mode
        .store(6, std::sync::atomic::Ordering::Relaxed);
    let result = retry_frozen_approval(&client, &keys, &path, &scope, "receipt-1", || Ok(()))
        .await
        .unwrap();
    assert_eq!(result["state"], "accepted");
    let seen = fixture.seen.lock().unwrap();
    assert_eq!(seen.len(), 3);
    assert_eq!(seen[0].body, body);
    assert_eq!(seen[2].body, body);
    assert_eq!(seen[1].method, "GET");
    assert_signed(&seen[0], &keys);
    assert_signed(&seen[2], &keys);
    assert_ne!(seen[0].auth, seen[2].auth);
}

#[tokio::test]
async fn approval_intent_survives_crash_before_first_post() {
    let base = tempfile::tempdir().unwrap();
    let (scope, path, body) = journaled_approval(base.path());
    let (client, fixture) = fixture(6, Duration::from_secs(2)).await;
    let keys = Keys::generate();
    assert!(fixture.seen.lock().unwrap().is_empty());
    let result = retry_frozen_approval(&client, &keys, &path, &scope, "receipt-1", || Ok(()))
        .await
        .unwrap();
    assert_eq!(result["state"], "accepted");
    let seen = fixture.seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].method, "GET");
    assert_eq!(seen[1].method, "POST");
    assert_eq!(seen[1].body, body);
    assert_signed(&seen[1], &keys);
}

#[tokio::test]
async fn approval_recovery_does_not_repost_when_server_already_accepted() {
    let base = tempfile::tempdir().unwrap();
    let (scope, path, _) = journaled_approval(base.path());
    let (client, fixture) = fixture(7, Duration::from_secs(2)).await;
    let result = retry_frozen_approval(
        &client,
        &Keys::generate(),
        &path,
        &scope,
        "receipt-1",
        || Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(result["state"], "accepted");
    let seen = fixture.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].method, "GET");
}

fn seen_port(url: &str) -> u16 {
    Url::parse(url).unwrap().port().unwrap()
}

#[tokio::test]
async fn proposal_retry_keeps_request_id_and_signs_exact_body() {
    let (client, fixture) = fixture(0, Duration::from_secs(2)).await;
    let keys = Keys::generate();
    let first = client.create_proposal(&keys, &proposal()).await.unwrap();
    let second = client.create_proposal(&keys, &proposal()).await.unwrap();
    assert_eq!(
        BASE64.decode(&first.proposal_bytes_b64).unwrap(),
        br#"{"proposal_id":"proposal-1","operation_id":"operation-1","expires_at":"2030-01-01T00:00:00Z","server":true}"#
    );
    assert_eq!(second.proposal_digest, first.proposal_digest);
    let seen = fixture.seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].body, seen[1].body);
    assert_eq!(seen[0].path, "/v1/buzz/management/proposals");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&seen[0].body).unwrap()["request_id"],
        "stable-request-1"
    );
    for request in seen.iter() {
        assert_signed(request, &keys);
    }
}

#[tokio::test]
async fn approval_sends_only_on_native_call_and_uses_fixed_decision() {
    let (client, fixture) = fixture(0, Duration::from_secs(2)).await;
    let keys = Keys::generate();
    let approval = ManagementApprovalRequest {
        proposal_id: "proposal-1".to_string(),
        operation_id: "operation-1".to_string(),
        proposal_digest: hex::encode(Sha256::digest(
            br#"{"proposal_id":"proposal-1","operation_id":"operation-1","expires_at":"2030-01-01T00:00:00Z","server":true}"#,
        )),
        proposal_bytes_b64: BASE64
            .encode(br#"{"proposal_id":"proposal-1","operation_id":"operation-1","expires_at":"2030-01-01T00:00:00Z","server":true}"#),
        expires_at: "2030-01-01T00:00:00Z".to_string(),
    };
    assert!(fixture.seen.lock().unwrap().is_empty());
    client.approve(&keys, &approval).await.unwrap();
    let seen = fixture.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        seen[0].path,
        "/v1/buzz/management/proposals/proposal-1/approve"
    );
    let body: serde_json::Value = serde_json::from_slice(&seen[0].body).unwrap();
    assert_eq!(body["version"], "rowvia.buzz-management-approval/v1");
    assert_eq!(body["decision"], "approve");
    assert_eq!(body["proposal_digest"], approval.proposal_digest);
    assert_signed(&seen[0], &keys);
}

#[tokio::test]
async fn approval_rejects_tampered_digest_or_identity_without_sending() {
    let (client, fixture) = fixture(0, Duration::from_secs(2)).await;
    let keys = Keys::generate();
    let mut approval = ManagementApprovalRequest {
        proposal_id: "proposal-1".to_string(),
        operation_id: "operation-1".to_string(),
        proposal_digest: "0".repeat(64),
        proposal_bytes_b64: BASE64
            .encode(br#"{"proposal_id":"proposal-1","operation_id":"operation-1","expires_at":"2030-01-01T00:00:00Z"}"#),
        expires_at: "2030-01-01T00:00:00Z".to_string(),
    };
    assert!(client.approve(&keys, &approval).await.is_err());
    approval.proposal_digest = hex::encode(Sha256::digest(
        BASE64.decode(&approval.proposal_bytes_b64).unwrap(),
    ));
    approval.operation_id = "different-operation".to_string();
    assert!(client.approve(&keys, &approval).await.is_err());
    assert!(fixture.seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn approval_rejects_changed_or_expired_canonical_expiry_without_sending() {
    let (client, fixture) = fixture(0, Duration::from_secs(2)).await;
    let keys = Keys::generate();
    let canonical = br#"{"proposal_id":"proposal-1","operation_id":"operation-1","expires_at":"2030-01-01T00:00:00Z"}"#;
    let mut approval = ManagementApprovalRequest {
        proposal_id: "proposal-1".to_string(),
        operation_id: "operation-1".to_string(),
        proposal_digest: hex::encode(Sha256::digest(canonical)),
        proposal_bytes_b64: BASE64.encode(canonical),
        expires_at: "2030-01-02T00:00:00Z".to_string(),
    };
    assert_eq!(
        client.approve(&keys, &approval).await.unwrap_err(),
        "Rowvia proposal expiry mismatch"
    );
    let expired = br#"{"proposal_id":"proposal-1","operation_id":"operation-1","expires_at":"2020-01-01T00:00:00Z"}"#;
    approval.proposal_bytes_b64 = BASE64.encode(expired);
    approval.proposal_digest = hex::encode(Sha256::digest(expired));
    approval.expires_at = "2020-01-01T00:00:00Z".to_string();
    assert_eq!(
        client.approve(&keys, &approval).await.unwrap_err(),
        "Rowvia proposal has expired"
    );
    assert!(fixture.seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn expired_proposal_response_is_rejected() {
    let (client, fixture) = fixture(4, Duration::from_secs(2)).await;
    assert_eq!(
        client
            .create_proposal(&Keys::generate(), &proposal())
            .await
            .unwrap_err(),
        "Rowvia proposal has expired"
    );
    assert_eq!(fixture.seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn origin_and_redirect_are_refused() {
    for origin in [
        "http://rowvia.example",
        "https://rowvia.example/path",
        "https://rowvia.example?x=1",
        "https://user:pass@rowvia.example",
        "https://rowvia.example#fragment",
    ] {
        assert!(ManagementClient::new(origin, false, Duration::from_secs(1)).is_err());
    }
    let (client, fixture) = fixture(1, Duration::from_secs(2)).await;
    let result = client.operation(&Keys::generate(), "operation-1").await;
    assert_eq!(result.unwrap_err(), "Rowvia management HTTP 302");
    assert_eq!(fixture.seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn timeout_and_oversized_response_are_bounded() {
    let (slow, _) = fixture(2, Duration::from_millis(20)).await;
    assert_eq!(
        slow.operation(&Keys::generate(), "operation-1")
            .await
            .unwrap_err(),
        "Rowvia management request timed out"
    );
    let (large, _) = fixture(3, Duration::from_secs(2)).await;
    assert_eq!(
        large
            .operation(&Keys::generate(), "operation-1")
            .await
            .unwrap_err(),
        "Rowvia management response is too large"
    );
}
