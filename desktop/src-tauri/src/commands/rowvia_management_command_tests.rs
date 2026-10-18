//! Synthetic native-body components, not literal IPC, TLS, UI, or owner acceptance.
use super::*;
use axum::{body::to_bytes, extract::Request, extract::State as HttpState, Json, Router};
use nostr::{EventBuilder, Kind, Tag};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::sync::{
    atomic::{AtomicBool, AtomicU8, Ordering},
    Arc, Mutex,
};
use tauri::{test::MockRuntime, Manager};
use tokio::sync::Notify;

const CHANNEL: &str = "550e8400-e29b-41d4-a716-446655440000";
const BINDING: &str = "018fae01-1111-7111-8111-111111111111";
const OPERATION: &str = "018fae01-2222-7222-8222-222222222222";
const CHILD: &str = "BUZZ_ROWVIA_COMPONENT_CHILD";
const COMPLETE: &[u8] = b"native-body-complete\n";
const SCRATCH_PREFIX: &str = "buzz-rowvia-native-";

fn private_child_scratch() -> tempfile::TempDir {
    let mut builder = tempfile::Builder::new();
    builder.prefix(SCRATCH_PREFIX);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    builder.tempdir_in("/tmp").unwrap()
}

// Validate supplied pairs without printing values or consulting normal profiles.
fn closed_child_environment(
    pairs: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> Result<std::path::PathBuf, &'static str> {
    let mut child = false;
    let mut root = None;
    for (key, value) in pairs {
        if key == CHILD && value == "1" && !child {
            child = true;
        } else if key == "XDG_DATA_HOME" && root.is_none() {
            let path = std::path::PathBuf::from(value);
            if path.parent() != Some(std::path::Path::new("/tmp"))
                || !path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|name| name.strip_prefix(SCRATCH_PREFIX))
                    .is_some_and(|suffix| {
                        suffix.len() >= 6 && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric())
                    })
            {
                return Err("synthetic child scratch path refused");
            }
            root = Some(path);
        } else {
            return Err("synthetic child environment is not closed");
        }
    }
    if !child {
        return Err("synthetic child admission flag refused");
    }
    root.ok_or("synthetic child scratch path missing")
}

fn child_scratch(fresh: bool) -> std::path::PathBuf {
    let root =
        closed_child_environment(std::env::vars_os()).expect("closed synthetic child environment");
    verify_child_scratch(&root, fresh);
    root
}

fn verify_child_scratch(root: &std::path::Path, fresh: bool) {
    let metadata = std::fs::symlink_metadata(root).expect("synthetic child scratch metadata");
    assert!(metadata.is_dir() && !metadata.file_type().is_symlink());
    assert_eq!(std::fs::canonicalize(root).unwrap(), root);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
        assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
        let directory = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(root)
            .unwrap();
        let pinned = directory.metadata().unwrap();
        assert_eq!(
            (metadata.dev(), metadata.ino()),
            (pinned.dev(), pinned.ino())
        );
        assert_eq!(pinned.permissions().mode() & 0o777, 0o700);
        if fresh {
            assert!(
                std::fs::read_dir(root).unwrap().next().is_none(),
                "synthetic scratch is not freshly empty"
            );
        }
        // The kernel assigns a newly created anonymous file our effective file
        // ownership; no normal home, profile, environment value, or /proc read.
        let probe = tempfile::tempfile_in(root).unwrap();
        assert_eq!(
            pinned.uid(),
            probe.metadata().unwrap().uid(),
            "synthetic scratch owner mismatch"
        );
    }
    #[cfg(not(unix))]
    panic!("synthetic native scratch admission requires Unix no-follow checks");
}

fn retained_fixture_data(root: &std::path::Path) -> std::path::PathBuf {
    tempfile::tempdir_in(root).unwrap().keep()
}

struct Seen {
    path: String,
}

struct Server {
    owner: Keys,
    cerberus: Keys,
    relay: Keys,
    target: String,
    origin: String,
    seen: Mutex<Vec<Seen>>,
    journal: Mutex<Option<(std::path::PathBuf, Scope)>>,
    roster_mode: AtomicU8,
    expired: AtomicBool,
    defer: AtomicBool,
    entered: Notify,
    release: Notify,
}

fn verify_auth(request: &Request, body: &[u8], server: &Server) {
    let auth = request.headers()["authorization"].to_str().unwrap();
    let bytes = BASE64.decode(auth.strip_prefix("Nostr ").unwrap()).unwrap();
    let event = Event::from_json(&bytes).unwrap();
    event.verify().unwrap();
    assert_eq!(event.pubkey, server.owner.public_key());
    assert_eq!(event.kind.as_u16(), 27235);
    let wire: Value = serde_json::from_slice(&bytes).unwrap();
    let tag = |name| {
        wire["tags"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tag| tag[0] == name)
            .unwrap()[1]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(
        tag("u"),
        format!("{}{}", server.origin, request.uri().path())
    );
    assert_eq!(tag("method"), request.method().as_str());
    assert_eq!(tag("payload"), hex::encode(Sha256::digest(body)));
    assert!(!tag("nonce").is_empty());
}

async fn respond(HttpState(server): HttpState<Arc<Server>>, request: Request) -> Json<Value> {
    let (parts, body) = request.into_parts();
    let bytes = to_bytes(body, MAX_REQUEST_BYTES + 1)
        .await
        .unwrap()
        .to_vec();
    let request = Request::from_parts(parts, axum::body::Body::empty());
    let path = request.uri().path();
    {
        let mut seen = server.seen.lock().unwrap();
        assert!(seen.len() < 64, "fixture request budget exceeded");
        seen.push(Seen {
            path: path.to_owned(),
        });
    }
    if path != "/" {
        verify_auth(&request, &bytes, &server);
    }
    match path {
        "/" => Json(json!({ "self": server.relay.public_key().to_hex() })),
        "/query" => {
            assert_eq!(
                serde_json::from_slice::<Value>(&bytes).unwrap(),
                json!([{ "kinds": [39002], "#d": [CHANNEL], "limit": 1 }])
            );
            let mode = server.roster_mode.load(Ordering::SeqCst);
            let mut tags = vec![
                Tag::parse(["d", CHANNEL]).unwrap(),
                Tag::parse(["p", &server.cerberus.public_key().to_hex(), "", "bot"]).unwrap(),
            ];
            if mode != 2 {
                tags.push(Tag::parse(["p", &server.owner.public_key().to_hex()]).unwrap());
            }
            let signer = if mode == 1 {
                &server.cerberus
            } else {
                &server.relay
            };
            let roster = EventBuilder::new(Kind::Custom(39002), "")
                .tags(tags)
                .sign_with_keys(signer)
                .unwrap();
            Json(json!([roster]))
        }
        "/v1/buzz/management/candidates" => Json(json!({
            "candidates": [{ "agent_name": "Synthetic-Target", "target_pubkey": server.target,
                "binding_id": BINDING, "gmail_label": "Synthetic fixture", "binding_status": "active" }],
            "truncated": false,
        })),
        "/v1/buzz/management/proposals" => {
            let request: ManagementProposalRequest = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(request.target_agent_pubkey, server.target);
            assert_eq!(request.binding_id, BINDING);
            assert_eq!(request.operations, OPERATIONS.map(str::to_owned));
            let (path, scope) = server.journal.lock().unwrap().clone().unwrap();
            journal::transact(&path, &scope, |stored| {
                assert_eq!(stored.entries.len(), 1);
                assert_eq!(
                    BASE64.decode(&stored.entries[0].request_body_b64).unwrap(),
                    bytes
                );
                assert!(!stored.entries[0].approval_intent);
                Ok(((), false))
            })
            .unwrap();
            if server.defer.load(Ordering::SeqCst) {
                server.entered.notify_one();
                tokio::time::timeout(Duration::from_secs(3), server.release.notified())
                    .await
                    .unwrap();
            }
            let expiry = Utc::now()
                + chrono::Duration::seconds(if server.expired.load(Ordering::SeqCst) {
                    -60
                } else {
                    300
                });
            let canonical = serde_json::to_vec(&json!({
                "version": 1, "proposal_id": request.request_id, "operation_id": OPERATION,
                "owner_pubkey": server.owner.public_key().to_hex(), "source_instance_id": "synthetic",
                "action": "connector.grant", "target_pubkey": request.target_agent_pubkey,
                "binding_id": request.binding_id, "operations": OPERATIONS, "expires_at": expiry.to_rfc3339(),
            })).unwrap();
            Json(
                json!({ "proposal_id": request.request_id, "operation_id": OPERATION,
                "proposal_digest": hex::encode(Sha256::digest(&canonical)),
                "proposal_bytes_b64": BASE64.encode(canonical) }),
            )
        }
        path if path.ends_with("/approve") => {
            let (path, scope) = server.journal.lock().unwrap().clone().unwrap();
            journal::transact(&path, &scope, |stored| {
                let entry = &stored.entries[0];
                assert!(
                    entry.approval_intent,
                    "intent must be durable before transport"
                );
                assert_eq!(
                    BASE64
                        .decode(entry.approval_body_b64.as_ref().unwrap())
                        .unwrap(),
                    bytes
                );
                let (receipt, _) = decode_entry_receipt(entry, &scope).unwrap();
                assert_eq!(
                    request.uri().path(),
                    format!(
                        "/v1/buzz/management/proposals/{}/approve",
                        receipt.proposal_id
                    )
                );
                let approval: Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(approval["proposal_digest"], receipt.proposal_digest);
                assert_eq!(approval["decision"], "approve");
                assert_eq!(approval["operation_id"], OPERATION);
                Ok(((), false))
            })
            .unwrap();
            Json(json!({ "operation_id": OPERATION, "state": "succeeded",
                "policy_applied": true, "runtime_ready": false }))
        }
        _ => panic!("unexpected fixture endpoint: {path}"),
    }
}

struct Fixture {
    app: tauri::App<MockRuntime>,
    client: ManagementClient,
    server: Arc<Server>,
    owner_hex: String,
    cerberus_hex: String,
    task: tokio::task::JoinHandle<()>,
    _data: std::path::PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
        // Aborting the listener does not join Axum connection tasks. Storage is
        // retained under child scratch until the parent observes process exit.
    }
}

impl Fixture {
    async fn new() -> Self {
        let root = child_scratch(false);
        // Keep immediately, including setup-panic paths before Fixture exists.
        let data = retained_fixture_data(&root);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let owner = Keys::generate();
        let cerberus = Keys::generate();
        let owner_hex = owner.public_key().to_hex();
        let cerberus_hex = cerberus.public_key().to_hex();
        let server = Arc::new(Server {
            owner: owner.clone(),
            cerberus,
            relay: Keys::generate(),
            target: Keys::generate().public_key().to_hex(),
            origin: origin.clone(),
            seen: Mutex::new(Vec::new()),
            journal: Mutex::new(None),
            roster_mode: AtomicU8::new(0),
            expired: AtomicBool::new(false),
            defer: AtomicBool::new(false),
            entered: Notify::new(),
            release: Notify::new(),
        });
        let router = Router::new().fallback(respond).with_state(server.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = data.to_str().unwrap().to_owned();
        // Admission above refuses ambient identity/configuration before this builder.
        // Persisted identity resolution is never called.
        let mut state = crate::app_state::build_app_state();
        *state.keys.lock().unwrap() = owner;
        *state.relay_url_override.lock().unwrap() = Some(origin.replacen("http://", "ws://", 1));
        state.http_client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        let app = tauri::test::mock_builder()
            .manage(state)
            .build(context)
            .unwrap();
        let mut client = ManagementClient::new(&origin, true, Duration::from_secs(3)).unwrap();
        client.http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let fixture = Self {
            app,
            client,
            server,
            owner_hex,
            cerberus_hex,
            task,
            _data: data,
        };
        let scope = capture_scope_in(
            fixture.app.handle(),
            &fixture.app.state::<AppState>(),
            &fixture.client,
            &fixture.pins(),
        )
        .unwrap()
        .0;
        let path = journal_path(fixture.app.handle(), &scope).unwrap();
        assert!(
            path.starts_with(&fixture._data),
            "journal escaped temporary app data"
        );
        *fixture.server.journal.lock().unwrap() = Some((path, scope));
        fixture
    }

    fn pins(&self) -> ManagementPins<'_> {
        ManagementPins {
            source_instance: Some("synthetic"),
            cerberus_pubkey: Some(&self.cerberus_hex),
            owner_pubkey: Some(&self.owner_hex),
        }
    }

    fn event(&self, signer: &Keys) -> String {
        let frame = json!({ "seq": 0, "timestamp": Utc::now().to_rfc3339(),
            "kind": "agent_management_request", "agentIndex": null, "channelId": CHANNEL,
            "sessionId": null, "turnId": null, "payload": {
                "type": "agent_management_request", "version": 1, "action": "connector.grant",
                "requestId": "550e8400-e29b-41d4-a716-446655440001", "request": {
                    "channelId": CHANNEL, "targetName": "Synthetic-Target", "gmailLabels": ["Synthetic fixture"] } } });
        let content = buzz_core_pkg::observer::encrypt_observer_payload(
            signer,
            &self.server.owner.public_key(),
            &frame,
        )
        .unwrap();
        EventBuilder::new(
            Kind::Custom(buzz_core_pkg::kind::KIND_AGENT_OBSERVER_FRAME as u16),
            content,
        )
        .tags([
            Tag::parse(["p", &self.owner_hex]).unwrap(),
            Tag::parse(["agent", &self.cerberus_hex]).unwrap(),
            Tag::parse(["frame", "telemetry"]).unwrap(),
        ])
        .sign_with_keys(signer)
        .unwrap()
        .as_json()
    }

    async fn selection(&self) -> CreateSelection {
        let candidates = rowvia_get_management_candidates_in(
            self.app.handle().clone(),
            &self.app.state::<AppState>(),
            &self.client,
            &self.pins(),
        )
        .await
        .unwrap();
        assert_eq!(candidates.candidates.len(), 1);
        CreateSelection {
            selection_handle: candidates.candidates[0].selection_handle.clone(),
            action: ManagementAction::ConnectorGrant,
            relay_event_json: self.event(&self.server.cerberus),
        }
    }

    async fn create(&self, selection: CreateSelection) -> Result<NativeReceipt, String> {
        rowvia_create_management_proposal_in(
            self.app.handle().clone(),
            &self.app.state::<AppState>(),
            selection,
            &self.client,
            &self.pins(),
        )
        .await
    }

    async fn approve(&self, handle: String) -> Result<Value, String> {
        rowvia_approve_management_proposal_in(
            self.app.handle().clone(),
            &self.app.state::<AppState>(),
            handle,
            &self.client,
            &self.pins(),
        )
        .await
    }

    fn entries(&self) -> Vec<Entry> {
        let (path, scope) = self.server.journal.lock().unwrap().clone().unwrap();
        journal::transact(&path, &scope, |stored| Ok((stored.entries.clone(), false))).unwrap()
    }

    fn count(&self, suffix: &str) -> usize {
        self.server
            .seen
            .lock()
            .unwrap()
            .iter()
            .filter(|seen| seen.path.ends_with(suffix))
            .count()
    }
}

async fn exercise_native_bodies() {
    let fixture = Fixture::new().await;
    let selection = fixture.selection().await;
    let selection_handle = selection.selection_handle.clone();
    let source_id = Event::from_json(&selection.relay_event_json)
        .unwrap()
        .id
        .to_hex();
    let receipt = fixture.create(selection).await.unwrap();
    assert_eq!(fixture.count("/approve"), 0);
    let entry = fixture.entries().remove(0);
    assert!(!entry.approval_intent);
    assert_eq!(entry.receipt_handle, receipt.receipt_handle);
    assert_eq!(entry.source_event_id, source_id);
    let encoded: Value =
        serde_json::from_slice(&BASE64.decode(entry.receipt_body_b64.unwrap()).unwrap()).unwrap();
    assert_eq!(
        encoded["proposal_bytes_b64"],
        receipt.proposal.proposal_bytes_b64
    );
    assert_eq!(encoded["proposal_digest"], receipt.proposal.proposal_digest);
    let replay = fixture.create(fixture.selection().await).await.unwrap();
    assert_eq!(replay.receipt_handle, receipt.receipt_handle);
    assert_eq!(
        replay.proposal.proposal_bytes_b64,
        receipt.proposal.proposal_bytes_b64
    );
    assert_eq!(
        fixture.count("/proposals"),
        1,
        "same logical draft must reuse its receipt"
    );
    assert!(fixture
        .approve(selection_handle)
        .await
        .unwrap_err()
        .contains("unknown Rowvia receipt"));
    assert!(fixture
        .approve(receipt.proposal.proposal_id.clone())
        .await
        .unwrap_err()
        .contains("unknown Rowvia receipt"));
    assert_eq!(fixture.count("/approve"), 0);
    let operation = fixture
        .approve(receipt.receipt_handle.clone())
        .await
        .unwrap();
    assert_eq!(operation["policy_applied"], true);
    assert_eq!(operation["runtime_ready"], false);
    assert!(fixture.entries()[0].approval_intent);
    assert!(fixture
        .approve(receipt.receipt_handle)
        .await
        .unwrap_err()
        .contains("already requested"));
    assert_eq!(fixture.count("/approve"), 1);
    println!("native body: canonical receipt, handle-only explicit approval, signed transport and durable intent passed");

    for mode in 0..=3 {
        let fixture = Fixture::new().await;
        let mut selection = fixture.selection().await;
        if mode == 0 {
            selection.relay_event_json = fixture.event(&Keys::generate());
        }
        if mode == 1 {
            *fixture.app.state::<AppState>().keys.lock().unwrap() = Keys::generate();
        }
        if mode >= 2 {
            fixture.server.roster_mode.store(mode - 1, Ordering::SeqCst);
        }
        assert!(
            fixture.create(selection).await.is_err(),
            "admission negative {mode}"
        );
        assert_eq!(
            fixture.count("/candidates"),
            1,
            "no management refresh before admission"
        );
        assert_eq!(fixture.count("/proposals"), 0);
        assert_eq!(fixture.count("/approve"), 0);
        if mode <= 1 {
            assert_eq!(
                fixture.count("/query"),
                0,
                "signer/owner refusal must precede relay transport"
            );
        }
        assert!(fixture.entries().is_empty());
    }
    println!(
        "native body: wrong signer, active owner, roster signer and roster membership refused"
    );

    let fixture = Fixture::new().await;
    {
        let state = fixture.app.state::<AppState>();
        let mut pins = ManagementPins {
            source_instance: None,
            cerberus_pubkey: None,
            owner_pubkey: None,
        };
        state.identity_lost.store(true, Ordering::SeqCst);
        assert!(
            capture_scope_in(fixture.app.handle(), &state, &fixture.client, &pins)
                .err()
                .unwrap()
                .contains("recovery mode")
        );
        state.identity_lost.store(false, Ordering::SeqCst);
        assert_eq!(
            capture_scope_in(fixture.app.handle(), &state, &fixture.client, &pins)
                .err()
                .as_deref(),
            Some("Rowvia source instance is not configured in this build")
        );
        pins.source_instance = Some("synthetic");
        assert_eq!(
            capture_scope_in(fixture.app.handle(), &state, &fixture.client, &pins)
                .err()
                .as_deref(),
            Some("Cerberus identity is not configured in this build")
        );
        pins.cerberus_pubkey = Some(&fixture.cerberus_hex);
        assert_eq!(
            capture_scope_in(fixture.app.handle(), &state, &fixture.client, &pins)
                .err()
                .as_deref(),
            Some("Rowvia owner identity is not configured in this build")
        );
        assert_eq!(fixture.count("/candidates"), 0);
        println!(
            "native body: signing recovery and build-pin failure order preserved without transport"
        );
    }

    let fixture = Fixture::new().await;
    let selection = fixture.selection().await;
    fixture.server.expired.store(true, Ordering::SeqCst);
    assert_eq!(
        fixture.create(selection).await.err().as_deref(),
        Some("Rowvia proposal has expired")
    );
    assert!(fixture.entries()[0].receipt_body_b64.is_none());
    assert!(!fixture.entries()[0].approval_intent);
    assert_eq!(fixture.count("/approve"), 0);
    println!("native body: expired receipt refused before receipt save or approval");

    let fixture = Fixture::new().await;
    let selection = fixture.selection().await;
    fixture.server.defer.store(true, Ordering::SeqCst);
    let (result, ()) = tokio::join!(fixture.create(selection), async {
        tokio::time::timeout(Duration::from_secs(3), fixture.server.entered.notified())
            .await
            .unwrap();
        *fixture
            .app
            .state::<AppState>()
            .relay_url_override
            .lock()
            .unwrap() = Some("ws://127.0.0.1:1".into());
        fixture.server.release.notify_one();
    });
    assert_eq!(
        result.err().as_deref(),
        Some("Rowvia owner or community changed")
    );
    assert_eq!(
        fixture.count("/candidates"),
        2,
        "scope refusal must precede binding refresh after response"
    );
    assert!(fixture.entries()[0].receipt_body_b64.is_none());
    assert!(!fixture.entries()[0].approval_intent);
    assert_eq!(fixture.count("/approve"), 0);
    println!("native body: scope change during response refused without storing stale receipt");

    let fixture = Fixture::new().await;
    let selection = fixture.selection().await;
    let receipt = fixture.create(selection).await.unwrap();
    let (path, scope) = fixture.server.journal.lock().unwrap().clone().unwrap();
    journal::transact(&path, &scope, |stored| {
        stored.entries[0].receipt_digest = Some("0".repeat(64));
        Ok(((), true))
    })
    .unwrap();
    assert_eq!(
        fixture
            .approve(receipt.receipt_handle)
            .await
            .err()
            .as_deref(),
        Some("Rowvia journal receipt digest mismatch")
    );
    assert!(!fixture.entries()[0].approval_intent);
    assert_eq!(fixture.count("/approve"), 0);
    println!("native body: tampered journal receipt refused before intent or transport");
}

#[derive(Default)]
struct ChildEvidence {
    exited: bool,
    complete: bool,
    uncertain: bool,
}

impl ChildEvidence {
    fn observe_wait(&mut self, result: Result<bool, ()>) {
        match result {
            Ok(true) => self.exited = true,
            Ok(false) => {}
            Err(()) => self.uncertain = true,
        }
    }

    fn observe_kill(&mut self, result: Result<(), ()>) {
        if result.is_err() {
            self.uncertain = true;
        }
    }

    fn may_delete(&self) -> bool {
        self.exited && self.complete && !self.uncertain
    }
}

struct ChildRun {
    child: Option<std::process::Child>,
    data: Option<tempfile::TempDir>,
    evidence: ChildEvidence,
}

impl ChildRun {
    fn poll(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        let result = self.child.as_mut().unwrap().try_wait();
        self.evidence.observe_wait(
            result
                .as_ref()
                .map(|status| status.is_some())
                .map_err(|_| ()),
        );
        if matches!(result, Ok(Some(_))) {
            drop(self.child.take());
        }
        result
    }
}

impl Drop for ChildRun {
    fn drop(&mut self) {
        if self.child.is_some() || !self.evidence.may_delete() {
            if let Some(data) = self.data.take() {
                eprintln!(
                    "retained synthetic child scratch: {}",
                    data.keep().display()
                );
            }
        }
        if let Some(child) = self.child.as_mut() {
            // A wait error may mean custody was lost: do not signal a stale PID.
            if !self.evidence.uncertain {
                self.evidence.observe_kill(child.kill().map_err(|_| ()));
                let deadline = std::time::Instant::now() + Duration::from_secs(3);
                loop {
                    let result = child.try_wait();
                    self.evidence.observe_wait(
                        result
                            .as_ref()
                            .map(|status| status.is_some())
                            .map_err(|_| ()),
                    );
                    if !matches!(result, Ok(None)) || std::time::Instant::now() >= deadline {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }
}

fn exact_completion_marker(root: &std::path::Path) -> std::io::Result<bool> {
    let marker = root.join("native-body.complete");
    let metadata = std::fs::symlink_metadata(&marker)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() != COMPLETE.len() as u64
    {
        return Ok(false);
    }
    let mut bytes = Vec::new();
    std::fs::File::open(marker)?
        .take(64)
        .read_to_end(&mut bytes)?;
    Ok(bytes == COMPLETE)
}

#[test]
fn native_synthetic_environment_is_closed_before_construction() {
    let clean = || {
        vec![
            (CHILD.into(), "1".into()),
            (
                "XDG_DATA_HOME".into(),
                "/tmp/buzz-rowvia-native-synthetic".into(),
            ),
        ]
    };
    assert!(closed_child_environment(clean()).is_ok());
    for key in [
        "BUZZ_PRIVATE_KEY",
        "HOME",
        "XDG_CONFIG_HOME",
        "BUZZ_RELAY_URL",
        "HTTP_PROXY",
        "DBUS_SESSION_BUS_ADDRESS",
        "LD_PRELOAD",
        "UNKNOWN",
    ] {
        let mut pairs = clean();
        pairs.push((key.into(), "synthetic-canary-not-an-ambient-value".into()));
        assert!(closed_child_environment(pairs).is_err());
    }
    for flag in ["", "true", "0"] {
        let mut pairs = clean();
        pairs[0].1 = flag.into();
        assert!(closed_child_environment(pairs).is_err());
    }
    for root in [
        "/tmp",
        "/tmp/buzz-rowvia-native-x",
        "/home/synthetic-not-a-profile",
        "/tmp/buzz-rowvia-native-synthetic/../outside",
    ] {
        let mut pairs = clean();
        pairs[1].1 = root.into();
        assert!(closed_child_environment(pairs).is_err());
    }
    assert!(closed_child_environment(clean().into_iter().skip(1)).is_err());
    assert!(closed_child_environment(clean().into_iter().take(1)).is_err());
    for paths in [
        "",
        "/tmp/synthetic",
        "relative",
        "/nix/store",
        "/nix/store/synthetic-public-lib/lib",
    ] {
        let mut pairs = clean();
        pairs.push(("LD_LIBRARY_PATH".into(), paths.into()));
        assert!(closed_child_environment(pairs).is_err());
    }
}

#[test]
fn native_synthetic_uncertain_custody_retains_scratch() {
    let outer = tempfile::tempdir_in("/tmp").unwrap();
    let fixture_data = retained_fixture_data(outer.path());
    assert!(
        fixture_data.is_dir(),
        "fixture storage was deleted before process settlement"
    );
    for failure in ["wait", "kill", "post-kill-wait", "no-exit", "no-marker"] {
        let data = tempfile::tempdir_in(outer.path()).unwrap();
        let path = data.path().to_owned();
        let mut run = ChildRun {
            child: None,
            data: Some(data),
            evidence: ChildEvidence::default(),
        };
        match failure {
            "wait" => run.evidence.observe_wait(Err(())),
            "kill" => run.evidence.observe_kill(Err(())),
            "post-kill-wait" => {
                run.evidence.observe_kill(Ok(()));
                run.evidence.observe_wait(Err(()));
            }
            _ => {}
        }
        if failure != "no-exit" {
            run.evidence.observe_wait(Ok(true));
        }
        run.evidence.complete = failure != "no-marker";
        assert!(!run.evidence.may_delete());
        drop(run);
        assert!(
            path.is_dir(),
            "uncertainty or missing proof deleted scratch"
        );
    }
    let data = tempfile::tempdir_in(outer.path()).unwrap();
    let path = data.path().to_owned();
    let mut run = ChildRun {
        child: None,
        data: Some(data),
        evidence: ChildEvidence::default(),
    };
    run.evidence.observe_wait(Ok(true));
    run.evidence.complete = true;
    assert!(run.evidence.may_delete());
    drop(run);
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn native_synthetic_scratch_creation_is_private_fresh_and_distinct() {
    let first = private_child_scratch();
    let second = private_child_scratch();
    assert_ne!(first.path(), second.path());
    for root in [&first, &second] {
        verify_child_scratch(root.path(), true);
        assert_eq!(
            closed_child_environment([
                (CHILD.into(), "1".into()),
                ("XDG_DATA_HOME".into(), root.path().as_os_str().to_owned()),
            ])
            .unwrap(),
            root.path()
        );
    }
}

#[cfg(unix)]
#[test]
fn native_synthetic_scratch_must_be_fresh_private_and_not_a_symlink() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let root = private_child_scratch();
    verify_child_scratch(root.path(), true);
    std::fs::write(root.path().join("synthetic-existing-file"), b"synthetic").unwrap();
    assert!(std::panic::catch_unwind(|| verify_child_scratch(root.path(), true)).is_err());
    verify_child_scratch(root.path(), false);
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(std::panic::catch_unwind(|| verify_child_scratch(root.path(), false)).is_err());
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let links = tempfile::tempdir_in("/tmp").unwrap();
    let link = links.path().join("synthetic-link");
    symlink(root.path(), &link).unwrap();
    assert!(std::panic::catch_unwind(|| verify_child_scratch(&link, false)).is_err());
}

#[test]
fn native_synthetic_completion_marker_is_not_empty_green() {
    let root = tempfile::tempdir_in("/tmp").unwrap();
    let marker = root.path().join("native-body.complete");
    assert!(exact_completion_marker(root.path()).is_err());
    for bytes in [
        b"".as_slice(),
        b"native-body-complete",
        b"wrong-body-complete\n",
        COMPLETE,
    ] {
        std::fs::write(&marker, bytes).unwrap();
        assert_eq!(
            exact_completion_marker(root.path()).unwrap(),
            bytes == COMPLETE
        );
    }
    #[cfg(unix)]
    {
        std::fs::remove_file(&marker).unwrap();
        let target = root.path().join("synthetic-marker-target");
        std::fs::write(&target, COMPLETE).unwrap();
        std::os::unix::fs::symlink(target, marker).unwrap();
        assert!(!exact_completion_marker(root.path()).unwrap());
    }
}

#[test]
fn native_command_bodies_synthetic() {
    if std::env::var_os(CHILD).is_some() {
        let root = child_scratch(true);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(exercise_native_bodies());
        // Drop joins the current-thread runtime's shutdown before publishing completion.
        // The parent additionally requires real process exit before deleting scratch.
        drop(runtime);
        let marker = root.join("native-body.complete");
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(marker)
            .unwrap()
            .write_all(COMPLETE)
            .unwrap();
        return;
    }
    // No inherited provider/identity/proxy settings reach AppState construction.
    let mut run = ChildRun {
        child: None,
        data: Some(private_child_scratch()),
        evidence: ChildEvidence::default(),
    };
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .env_clear()
        .env("XDG_DATA_HOME", run.data.as_ref().unwrap().path())
        .env(CHILD, "1")
        .args([
            "--exact",
            "commands::rowvia_management::command_tests::native_command_bodies_synthetic",
            "--nocapture",
        ]);
    // Verify the exact launch packet too; no ambient loader/socket/proxy exceptions.
    closed_child_environment(
        command
            .get_envs()
            .map(|(key, value)| (key.to_owned(), value.unwrap().to_owned())),
    )
    .expect("closed synthetic launcher environment");
    run.child = Some(command.spawn().unwrap());
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        match run.poll() {
            Ok(Some(status)) => {
                assert!(status.success(), "synthetic child failed");
                assert!(
                    exact_completion_marker(run.data.as_ref().unwrap().path()).unwrap_or(false),
                    "child must execute every native-body assertion"
                );
                run.evidence.complete = true;
                return;
            }
            Ok(None) => {}
            Err(_) => panic!("synthetic child wait failed; retaining custody evidence"),
        }
        if std::time::Instant::now() >= deadline {
            // Drop retains storage first, then attempts bounded owned-child settlement.
            panic!("synthetic child exceeded its finite deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
