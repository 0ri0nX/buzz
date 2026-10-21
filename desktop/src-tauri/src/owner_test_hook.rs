//! Temporary, opt-in local bridge to the already authenticated main webview.
//! No key export, generic invocation or script execution surface.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{oneshot, Semaphore};
use tokio_util::sync::CancellationToken;

const SCHEMA: &str = "rowvia.buzz.owner-test/v1";
const OWNER: &str = "89af092923ad49e3b9916b0c9f28c6b9237d8fcab4461cb7ddd12afe354f0d68";
const ARCHITECT: &str = "05d1901b020205f04db1ee6c434d4cd69211af3dbc1d2a46e0e1e93f81f6dd20";
const MAX_REQUEST: usize = 64 * 1024;
const MAX_RESPONSE: usize = 256 * 1024;
const MAX_PENDING: usize = 8;
const MAX_CACHE: usize = 128;
const DEADLINE: Duration = Duration::from_secs(30);
const EMPTY_ID: &str = "00000000-0000-0000-0000-000000000000";
const TEST_MESSAGE_TARGETS: [(&str, &str); 3] = [
    (
        "31986c38-6676-4d6f-a4ba-ed833926d0d5",
        "29cc1bb0ac338520c263dcfe654764224018a1ff5e5d3fc633be73f71f0c8c78",
    ),
    (
        "fd0cc15d-c216-442f-bac2-837dd6fddbbd",
        "9d3972d809d53ce8a87003ca7f631009963d9414739188cea6209ced88365c87",
    ),
    (
        "31986c38-6676-4d6f-a4ba-ed833926d0d5",
        "01ab717b2ebeeef89c3abde0be5a55f6aa95fac66d89c96124083ad381857b4c",
    ),
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TestMessageArguments {
    channel_id: String,
    recipient_pubkey: String,
    content: String,
    expires_at: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TestReadArguments {
    channel_id: String,
    recipient_pubkey: String,
    limit: u8,
    since: Option<u64>,
}

fn validate_test_read(arguments: &serde_json::Map<String, Value>) -> Result<(), Code> {
    let args: TestReadArguments = serde_json::from_value(Value::Object(arguments.clone()))
        .map_err(|_| Code::InvalidRequest)?;
    if !TEST_MESSAGE_TARGETS.iter().any(|(channel, recipient)| {
        args.channel_id == *channel && args.recipient_pubkey == *recipient
    }) || !(1..=20).contains(&args.limit)
        || args
            .since
            .is_some_and(|since| since > 9_007_199_254_740_991)
        || arguments.get("since").is_some_and(Value::is_null)
    {
        return Err(Code::InvalidRequest);
    }
    Ok(())
}

fn validate_test_message(arguments: &serde_json::Map<String, Value>) -> Result<(), Code> {
    let args: TestMessageArguments = serde_json::from_value(Value::Object(arguments.clone()))
        .map_err(|_| Code::InvalidRequest)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| Code::InvalidRequest)?
        .as_millis();
    if !TEST_MESSAGE_TARGETS.iter().any(|(channel, recipient)| {
        args.channel_id == *channel && args.recipient_pubkey == *recipient
    }) || !args.content.starts_with("ROWVIA_E2E_")
        || args.content.len() > 1024
        || u128::from(args.expires_at) <= now
        || u128::from(args.expires_at) > now + 120_000
    {
        return Err(Code::InvalidRequest);
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Expected {
    owner_pubkey: String,
    architect_pubkey: String,
    relay_url: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Request {
    schema: String,
    request_id: String,
    operation: String,
    expected: Expected,
    arguments: serde_json::Map<String, Value>,
}

/// Closed response envelope shared with the owner test frontend.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Response {
    schema: String,
    request_id: String,
    status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Map<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<Code>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Ok,
    Error,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Code {
    InvalidRequest,
    DuplicateRequest,
    ScopeMismatch,
    NotReady,
    ChannelNotOwned,
    MembershipFailed,
    OperationFailed,
    CapacityExceeded,
    ResponseTooLarge,
    Timeout,
    Busy,
    CacheFull,
    InvalidResponse,
    HookDisabled,
    Shutdown,
}

impl Response {
    fn failure(id: &str, status: Status, code: Code) -> Self {
        Self {
            schema: SCHEMA.into(),
            request_id: id.into(),
            status,
            result: None,
            code: Some(code),
        }
    }

    fn valid(&self) -> bool {
        self.schema == SCHEMA
            && valid_id(&self.request_id)
            && match self.status {
                Status::Ok => self.result.is_some() && self.code.is_none(),
                Status::Error | Status::Unknown => self.result.is_none() && self.code.is_some(),
            }
            && serde_json::to_vec(self).is_ok_and(|bytes| bytes.len() <= MAX_RESPONSE)
    }
}

struct Entry {
    request: Request,
    response: Option<Response>,
    pending: Option<oneshot::Sender<Response>>,
    deadline: Option<tokio::time::Instant>,
    claude_membership_started: bool,
}

#[derive(Default)]
struct Ledger {
    entries: HashMap<String, Entry>,
}

enum Admission {
    Dispatch(oneshot::Receiver<Response>),
    Return(Response),
}

impl Ledger {
    fn admit(&mut self, request: &Request) -> Admission {
        if let Some(entry) = self.entries.get(&request.request_id) {
            return Admission::Return(if entry.request != *request {
                Response::failure(&request.request_id, Status::Error, Code::DuplicateRequest)
            } else {
                entry.response.clone().unwrap_or_else(|| {
                    Response::failure(&request.request_id, Status::Unknown, Code::DuplicateRequest)
                })
            });
        }
        if self.entries.len() >= MAX_CACHE {
            return Admission::Return(Response::failure(
                &request.request_id,
                Status::Error,
                Code::CacheFull,
            ));
        }
        if self
            .entries
            .values()
            .filter(|entry| entry.pending.is_some())
            .count()
            >= MAX_PENDING
        {
            return Admission::Return(Response::failure(
                &request.request_id,
                Status::Error,
                Code::Busy,
            ));
        }
        let (tx, rx) = oneshot::channel();
        self.entries.insert(
            request.request_id.clone(),
            Entry {
                request: request.clone(),
                response: None,
                pending: Some(tx),
                deadline: None,
                claude_membership_started: false,
            },
        );
        Admission::Dispatch(rx)
    }

    fn complete(&mut self, response: Response) -> Result<(), &'static str> {
        if !response.valid() {
            return Err("invalid_response");
        }
        let entry = self
            .entries
            .get_mut(&response.request_id)
            .ok_or("invalid_response")?;
        if entry.response.is_some() || entry.pending.is_none() {
            return Err("duplicate_request");
        }
        entry.response = Some(response.clone());
        if let Some(tx) = entry.pending.take() {
            // Retain the result even when the socket caller has disconnected.
            let _ = tx.send(response);
        }
        Ok(())
    }
}

struct Hook {
    ledger: Mutex<Ledger>,
    cancel: CancellationToken,
    socket: SocketGuard,
}

struct SocketGuard {
    path: PathBuf,
    device: u64,
    inode: u64,
    uid: u32,
    cleanup_attempted: AtomicBool,
}

impl SocketGuard {
    fn remove(&self) {
        // stop, serve teardown and Drop share one cleanup attempt, even when
        // it fails. A later path may have reused this socket's inode.
        if self.cleanup_attempted.swap(true, Ordering::Relaxed) {
            return;
        }
        // The first metadata check/unlink is still not atomic.
        // Never unlink a replacement created by another process.
        if fs::symlink_metadata(&self.path).is_ok_and(|m| {
            m.file_type().is_socket() && m.dev() == self.device && m.ino() == self.inode
        }) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        self.remove();
    }
}

fn valid_id(id: &str) -> bool {
    uuid::Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == id)
}

fn validate_request(bytes: &[u8]) -> Result<Request, Code> {
    if bytes.len() > MAX_REQUEST {
        return Err(Code::InvalidRequest);
    }
    let request: Request = serde_json::from_slice(bytes).map_err(|_| Code::InvalidRequest)?;
    if request.schema != SCHEMA
        || !valid_id(&request.request_id)
        || !matches!(
            request.operation.as_str(),
            "status"
                | "inspect_cerberus_proposal_ui"
                | "create_private_stream"
                | "add_architect"
                | "send_message"
                | "send_existing_test_message"
                | "read_existing_test_channel"
                | "read_channel"
                | "inspect_managed_agent_runtime"
                | "ensure_managed_agent_runtime"
                | "inspect_saved_developer"
                | "configure_saved_developer"
                | "start_saved_developer"
                | "create_stopped_claude_developer"
                | "attach_saved_claude_developer"
                | "prepare_saved_claude_setup"
                | "start_saved_claude_setup"
                | "stop_restore_saved_claude_normal"
                | "start_saved_claude_normal"
                | "inspect_saved_claude_initial_state"
                | "stop_cerberus_for_recovery"
                | "inspect_saved_connector"
                | "prepare_saved_connector"
                | "approve_saved_connector"
                | "inspect_saved_connector_operation"
        )
    {
        return Err(Code::InvalidRequest);
    }
    if request.operation == "inspect_cerberus_proposal_ui" && !request.arguments.is_empty() {
        return Err(Code::InvalidRequest);
    }
    if request.operation == "send_existing_test_message" {
        validate_test_message(&request.arguments)?;
    }
    if request.operation == "read_existing_test_channel" {
        validate_test_read(&request.arguments)?;
    }
    if request.operation == crate::commands::owner_test_claude_create::OPERATION {
        crate::commands::owner_test_claude_create::validate_arguments(&request.arguments)
            .map_err(|_| Code::InvalidRequest)?;
    }
    if request.operation == crate::commands::owner_test_cerberus_recovery::OPERATION {
        crate::commands::owner_test_cerberus_recovery::validate_arguments(&request.arguments)
            .map_err(|_| Code::InvalidRequest)?;
    }
    if crate::commands::owner_test_claude_lifecycle::is_operation(&request.operation) {
        crate::commands::owner_test_claude_lifecycle::validate_arguments(
            &request.operation,
            &request.arguments,
        )
        .map_err(|_| Code::InvalidRequest)?;
    }
    if crate::commands::owner_test_saved_connector::is_operation(&request.operation) {
        crate::commands::owner_test_saved_connector::validate_arguments(
            &request.operation,
            &request.arguments,
        )
        .map_err(|_| Code::InvalidRequest)?;
    }
    if crate::managed_agents::owner_test_saved_developer::is_operation(&request.operation) {
        crate::managed_agents::owner_test_saved_developer::validate_arguments(
            &request.operation,
            &request.arguments,
        )
        .map_err(|_| Code::InvalidRequest)?;
    }
    if request.expected.owner_pubkey != OWNER
        || request.expected.architect_pubkey != ARCHITECT
        || !matches!(
            request.expected.relay_url.as_str(),
            "https://buzz.rowvia.ai:8443"
                | "https://buzz.rowvia.ai:8443/"
                | "wss://buzz.rowvia.ai:8443"
                | "wss://buzz.rowvia.ai:8443/"
        )
    {
        return Err(Code::ScopeMismatch);
    }
    Ok(request)
}

fn configured_path(value: Option<OsString>) -> Result<Option<PathBuf>, &'static str> {
    let Some(value) = value else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    if !path.is_absolute() || path.as_os_str().len() > 100 || path.file_name().is_none() {
        return Err("invalid_socket_parent");
    }
    let parent = path.parent().ok_or("invalid_socket_parent")?;
    if fs::canonicalize(parent).map_err(|_| "invalid_socket_parent")? != parent {
        return Err("invalid_socket_parent");
    }
    let metadata = fs::symlink_metadata(parent).map_err(|_| "invalid_socket_parent")?;
    // A safely created anonymous file provides the effective process uid without
    // introducing an unsafe libc call or trusting an inherited UID variable.
    let uid = tempfile::tempfile()
        .and_then(|file| file.metadata())
        .map_err(|_| "invalid_socket_parent")?
        .uid();
    if !metadata.is_dir()
        || metadata.permissions().mode() & 0o7777 != 0o700
        || metadata.uid() != uid
    {
        return Err("invalid_socket_parent");
    }
    if fs::symlink_metadata(&path).is_ok() {
        return Err("socket_already_exists");
    }
    Ok(Some(path))
}

/// Install only when this module's Cargo feature and socket environment opt-in exist.
pub(crate) fn setup(app: &tauri::AppHandle) -> Result<(), &'static str> {
    let Some(path) = configured_path(std::env::var_os("BUZZ_ROWVIA_E2E_SOCKET"))? else {
        return Ok(());
    };
    let listener =
        std::os::unix::net::UnixListener::bind(&path).map_err(|_| "socket_bind_failed")?;
    let metadata = fs::symlink_metadata(&path).map_err(|_| "socket_bind_failed")?;
    let socket = SocketGuard {
        path,
        device: metadata.dev(),
        inode: metadata.ino(),
        uid: metadata.uid(),
        cleanup_attempted: AtomicBool::new(false),
    };
    fs::set_permissions(&socket.path, fs::Permissions::from_mode(0o600))
        .map_err(|_| "socket_bind_failed")?;
    listener
        .set_nonblocking(true)
        .map_err(|_| "socket_bind_failed")?;
    let hook = Arc::new(Hook {
        ledger: Mutex::new(Ledger::default()),
        cancel: CancellationToken::new(),
        socket,
    });
    app.manage(Arc::clone(&hook));
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Ok(listener) = UnixListener::from_std(listener) else {
            hook.socket.remove();
            return;
        };
        serve(listener, app, hook).await;
    });
    Ok(())
}

/// Cancel pending work and unlink only the socket this process created.
pub(crate) fn stop(app: &tauri::AppHandle) {
    if let Some(hook) = app.try_state::<Arc<Hook>>() {
        hook.cancel.cancel();
        if let Ok(mut ledger) = hook.ledger.lock() {
            let ids: Vec<_> = ledger
                .entries
                .iter()
                .filter(|(_, entry)| entry.pending.is_some())
                .map(|(id, _)| id.clone())
                .collect();
            for id in ids {
                let _ = ledger.complete(Response::failure(&id, Status::Unknown, Code::Shutdown));
            }
        }
        hook.socket.remove();
    }
}

/// Accept replies only from the main webview and an outstanding request ID.
#[tauri::command]
pub(crate) fn rowvia_owner_test_reply(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    response: Response,
) -> Result<(), &'static str> {
    if window.label() != "main" {
        return Err("invalid_response");
    }
    let hook = app.try_state::<Arc<Hook>>().ok_or("hook_disabled")?;
    if hook.cancel.is_cancelled() {
        return Err("shutdown");
    }
    let result = hook
        .ledger
        .lock()
        .map_err(|_| "operation_failed")?
        .complete(response);
    result
}

fn pending_connector_request(
    hook: &Hook,
    id: &str,
) -> Result<(Request, tokio::time::Instant), &'static str> {
    if hook.cancel.is_cancelled() {
        return Err("shutdown");
    }
    let ledger = hook.ledger.lock().map_err(|_| "operation_failed")?;
    let entry = ledger.entries.get(id).ok_or("not_ready")?;
    let deadline = entry.deadline.ok_or("not_ready")?;
    if entry.response.is_some()
        || entry.pending.is_none()
        || deadline <= tokio::time::Instant::now()
        || !crate::commands::owner_test_saved_connector::is_operation(&entry.request.operation)
    {
        return Err("not_ready");
    }
    Ok((entry.request.clone(), deadline))
}

#[path = "owner_test_hook_claude_membership.rs"]
pub(crate) mod claude_membership;
#[cfg(test)]
use claude_membership::pending_claude_attachment;

/// The main webview can resolve only an already admitted, still-live closed
/// connector request. No caller-selected target, receipt, action or body.
#[tauri::command]
pub(crate) async fn rowvia_owner_test_saved_connector_context(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    request_id: String,
) -> Result<serde_json::Map<String, Value>, &'static str> {
    if window.label() != "main" {
        return Err("not_ready");
    }
    let hook = app
        .try_state::<Arc<Hook>>()
        .ok_or("hook_disabled")?
        .inner()
        .clone();
    let (request, deadline) = pending_connector_request(&hook, &request_id)?;
    let native_app = app.clone();
    let mut task = tokio::task::spawn_blocking(move || {
        crate::commands::owner_test_saved_connector::execute(
            &native_app,
            &request.operation,
            &request.arguments,
        )
    });
    let result = tokio::select! {
        _ = hook.cancel.cancelled() => { task.abort(); return Err("shutdown"); },
        _ = tokio::time::sleep_until(deadline) => { task.abort(); return Err("not_ready"); },
        result = &mut task => result.map_err(|_| "operation_failed")?.map_err(|_| "not_ready")?,
    };
    pending_connector_request(&hook, &request_id)?;
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "not_ready")?
        .as_millis()
        .saturating_add(remaining.as_millis());
    let mut result = result;
    result.insert(
        "deadlineUnixMs".into(),
        Value::from(u64::try_from(expires).map_err(|_| "not_ready")?),
    );
    Ok(result)
}

fn check_phase_scope(
    window_label: &str,
    hook_enabled: bool,
    shutdown: bool,
) -> Result<(), &'static str> {
    if window_label != "main" {
        return Err("invalid_response");
    }
    if !hook_enabled {
        return Err("hook_disabled");
    }
    if shutdown {
        return Err("shutdown");
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(crate) enum NativePhase {
    SetupEntered,
    MigrationsReady,
    IdentityReady,
    NestReady,
    SetupCompleted,
    PageLoadStarted,
    PageLoadFinished,
}

fn checked_native_phase(
    window_label: &str,
    hook_enabled: bool,
    shutdown: bool,
    phase: NativePhase,
) -> Result<&'static str, &'static str> {
    check_phase_scope(window_label, hook_enabled, shutdown)?;
    Ok(match phase {
        NativePhase::SetupEntered => "native_setup_entered",
        NativePhase::MigrationsReady => "native_migrations_ready",
        NativePhase::IdentityReady => "native_identity_ready",
        NativePhase::NestReady => "native_nest_ready",
        NativePhase::SetupCompleted => "native_setup_completed",
        NativePhase::PageLoadStarted => "native_page_load_started",
        NativePhase::PageLoadFinished => "native_page_load_finished",
    })
}

/// Native call-boundary observations, not readiness or frontend IPC authority.
pub(crate) fn report_native_phase(app: &tauri::AppHandle, window_label: &str, phase: NativePhase) {
    let hook = app.try_state::<Arc<Hook>>();
    if let Ok(phase) = checked_native_phase(
        window_label,
        hook.is_some(),
        hook.as_ref().is_some_and(|hook| hook.cancel.is_cancelled()),
        phase,
    ) {
        eprintln!("rowvia-owner-test-phase: {phase}");
    }
}

fn checked_phase(
    window_label: &str,
    hook_enabled: bool,
    shutdown: bool,
    phase: &str,
) -> Result<&'static str, &'static str> {
    check_phase_scope(window_label, hook_enabled, shutdown)?;
    // Return literals, never an untrusted input or a serde variant error.
    match phase {
        "bootstrap_started" => Ok("bootstrap_started"),
        "bootstrap_completed" => Ok("bootstrap_completed"),
        "bootstrap_failed" => Ok("bootstrap_failed"),
        "registration_started" => Ok("registration_started"),
        "registration_ready" => Ok("registration_ready"),
        "registration_failed" => Ok("registration_failed"),
        "request_received" => Ok("request_received"),
        "identity_started" => Ok("identity_started"),
        "identity_ready" => Ok("identity_ready"),
        "identity_error" => Ok("identity_error"),
        "relay_started" => Ok("relay_started"),
        "relay_ready" => Ok("relay_ready"),
        "relay_error" => Ok("relay_error"),
        "reply_started" => Ok("reply_started"),
        "reply_accepted" => Ok("reply_accepted"),
        "reply_failed" => Ok("reply_failed"),
        _ => Err("invalid_phase"),
    }
}

/// Write only closed phase markers for the opted-in main webview hook.
#[tauri::command]
pub(crate) async fn rowvia_owner_test_phase(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    phase: String,
) -> Result<(), &'static str> {
    let hook = app.try_state::<Arc<Hook>>();
    let phase = checked_phase(
        window.label(),
        hook.is_some(),
        hook.as_ref().is_some_and(|hook| hook.cancel.is_cancelled()),
        &phase,
    )?;
    // No request payload, arbitrary error text, or identity/ledger mutex access.
    eprintln!("rowvia-owner-test-phase: {phase}");
    Ok(())
}

async fn serve(listener: UnixListener, app: tauri::AppHandle, hook: Arc<Hook>) {
    let permits = Arc::new(Semaphore::new(MAX_PENDING));
    let mut tasks = tokio::task::JoinSet::new();
    loop {
        let permit = tokio::select! {
            _ = hook.cancel.cancelled() => break,
            result = Arc::clone(&permits).acquire_owned() => match result { Ok(p) => p, Err(_) => break },
        };
        let stream = tokio::select! {
            _ = hook.cancel.cancelled() => break,
            result = listener.accept() => match result { Ok((stream, _)) => stream, Err(_) => break },
        };
        let app = app.clone();
        let hook = Arc::clone(&hook);
        if !stream
            .peer_cred()
            .is_ok_and(|credentials| credentials.uid() == hook.socket.uid)
        {
            continue;
        }
        tasks.spawn(async move {
            let _permit = permit;
            handle(stream, app, hook).await;
        });
        while tasks.try_join_next().is_some() {}
    }
    tasks.abort_all();
    hook.socket.remove();
}

async fn read_request(stream: &mut UnixStream) -> Result<Vec<u8>, Code> {
    let mut bytes = Vec::new();
    // One frame per connection; never allocate beyond the frame cap.
    loop {
        let byte = stream.read_u8().await.map_err(|_| Code::InvalidRequest)?;
        if byte == b'\n' {
            return Ok(bytes);
        }
        if bytes.len() == MAX_REQUEST {
            return Err(Code::InvalidRequest);
        }
        bytes.push(byte);
    }
}

async fn handle(mut stream: UnixStream, app: tauri::AppHandle, hook: Arc<Hook>) {
    let deadline = tokio::time::Instant::now() + DEADLINE;
    let response = match tokio::time::timeout_at(deadline, read_request(&mut stream)).await {
        Ok(Ok(bytes)) => {
            let id = serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|value| {
                    value
                        .get("requestId")
                        .and_then(Value::as_str)
                        .filter(|id| valid_id(id))
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| EMPTY_ID.into());
            match validate_request(&bytes) {
                Ok(request) => dispatch(&app, &hook, request, deadline).await,
                Err(code) => Response::failure(&id, Status::Error, code),
            }
        }
        Ok(Err(code)) => Response::failure(EMPTY_ID, Status::Error, code),
        Err(_) => Response::failure(EMPTY_ID, Status::Error, Code::Timeout),
    };
    if let Ok(mut bytes) = serde_json::to_vec(&response) {
        if bytes.len() <= MAX_RESPONSE {
            bytes.push(b'\n');
            let _ = tokio::time::timeout(Duration::from_secs(2), stream.write_all(&bytes)).await;
        }
    }
}

async fn dispatch(
    app: &tauri::AppHandle,
    hook: &Hook,
    request: Request,
    deadline: tokio::time::Instant,
) -> Response {
    if hook.cancel.is_cancelled() {
        return Response::failure(&request.request_id, Status::Error, Code::Shutdown);
    }
    let admission = match hook.ledger.lock() {
        Ok(mut ledger) => ledger.admit(&request),
        Err(_) => {
            return Response::failure(&request.request_id, Status::Error, Code::OperationFailed)
        }
    };
    let rx = match admission {
        Admission::Dispatch(rx) => rx,
        Admission::Return(response) => return response,
    };
    if request.operation == crate::commands::owner_test_cerberus_recovery::OPERATION {
        let arguments = request.arguments.clone();
        let native_app = app.clone();
        let cancel = hook.cancel.clone();
        return dispatch_native(hook, &request, deadline, move || {
            crate::commands::owner_test_cerberus_recovery::execute(&native_app, &arguments, &|| {
                if cancel.is_cancelled() || tokio::time::Instant::now() >= deadline {
                    Err("not_ready".into())
                } else {
                    Ok(())
                }
            })
        })
        .await;
    }
    if request.operation == crate::commands::owner_test_claude_create::OPERATION {
        let arguments = request.arguments.clone();
        let native_app = app.clone();
        return dispatch_native(hook, &request, deadline, move || {
            crate::commands::owner_test_claude_create::execute(&native_app, &arguments)
        })
        .await;
    }
    if crate::commands::owner_test_claude_lifecycle::is_operation(&request.operation)
        && request.operation != crate::commands::owner_test_claude_lifecycle::ATTACH
    {
        let operation = request.operation.clone();
        let arguments = request.arguments.clone();
        let native_app = app.clone();
        return dispatch_native(hook, &request, deadline, move || {
            crate::commands::owner_test_claude_lifecycle::execute(
                &native_app,
                &operation,
                &arguments,
            )
        })
        .await;
    }
    if request.operation == crate::commands::owner_test_claude_lifecycle::ATTACH {
        if let Ok(mut ledger) = hook.ledger.lock() {
            if let Some(entry) = ledger.entries.get_mut(&request.request_id) {
                entry.deadline = Some(deadline);
            }
        }
    }
    if crate::commands::owner_test_saved_connector::is_operation(&request.operation) {
        if let Ok(mut ledger) = hook.ledger.lock() {
            if let Some(entry) = ledger.entries.get_mut(&request.request_id) {
                entry.deadline = Some(deadline);
            }
        }
    }
    if crate::managed_agents::owner_test_saved_developer::is_operation(&request.operation) {
        let operation = request.operation.clone();
        let arguments = request.arguments.clone();
        let native_app = app.clone();
        // Keep blocking filesystem/process work off the hook's socket executor.
        // The native helper checks actual current unlocked owner/community; the
        // request's expected values alone never authorize a saved-agent change.
        return dispatch_native(hook, &request, deadline, move || {
            crate::managed_agents::owner_test_saved_developer::execute(
                &native_app,
                &operation,
                &arguments,
            )
        })
        .await;
    }
    let emitted = app
        .get_webview_window("main")
        .is_some_and(|window| window.emit("rowvia-owner-test-request", &request).is_ok());
    if !emitted {
        let response = Response::failure(&request.request_id, Status::Unknown, Code::NotReady);
        if let Ok(mut ledger) = hook.ledger.lock() {
            let _ = ledger.complete(response.clone());
        }
        return response;
    }
    wait_for_reply(hook, &request.request_id, rx, deadline).await
}

async fn dispatch_native(
    hook: &Hook,
    request: &Request,
    deadline: tokio::time::Instant,
    native: impl FnOnce() -> Result<serde_json::Map<String, Value>, String> + Send + 'static,
) -> Response {
    if hook.cancel.is_cancelled() || deadline <= tokio::time::Instant::now() {
        let response = Response::failure(
            &request.request_id,
            Status::Unknown,
            if hook.cancel.is_cancelled() {
                Code::Shutdown
            } else {
                Code::Timeout
            },
        );
        if let Ok(mut ledger) = hook.ledger.lock() {
            let _ = ledger.complete(response.clone());
        }
        return response;
    }
    let mut task = tokio::task::spawn_blocking(native);
    let response = tokio::select! {
        biased;
        _ = hook.cancel.cancelled() => {
            task.abort();
            Response::failure(&request.request_id, Status::Unknown, Code::Shutdown)
        },
        _ = tokio::time::sleep_until(deadline) => {
            // Abort queued work. An already-running operation may finish; its
            // child remains tracked and this unknown outcome stays cached.
            task.abort();
            Response::failure(&request.request_id, Status::Unknown, Code::Timeout)
        },
        result = &mut task => match result {
            Ok(Ok(result)) => Response {
                schema: SCHEMA.into(),
                request_id: request.request_id.clone(),
                status: Status::Ok,
                result: Some(result),
                code: None,
            },
            Ok(Err(error)) if error == "scope_mismatch" => Response::failure(&request.request_id, Status::Error, Code::ScopeMismatch),
            Ok(Err(error)) if error == "not_ready" => Response::failure(&request.request_id, Status::Error, Code::NotReady),
            _ => Response::failure(&request.request_id, if request.operation == "inspect_saved_developer" { Status::Error } else { Status::Unknown }, Code::OperationFailed),
        },
    };
    if let Ok(mut ledger) = hook.ledger.lock() {
        let _ = ledger.complete(response.clone());
    }
    response
}

async fn wait_for_reply(
    hook: &Hook,
    id: &str,
    rx: oneshot::Receiver<Response>,
    deadline: tokio::time::Instant,
) -> Response {
    let response = tokio::select! {
        _ = hook.cancel.cancelled() => Response::failure(id, Status::Unknown, Code::Shutdown),
        result = tokio::time::timeout_at(deadline, rx) => match result {
            Ok(Ok(response)) => return response,
            _ => Response::failure(id, Status::Unknown, Code::Timeout),
        },
    };
    if let Ok(mut ledger) = hook.ledger.lock() {
        let _ = ledger.complete(response.clone());
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_phases_require_active_main_scope_and_remain_native_only() {
        for (phase, marker) in [
            (NativePhase::SetupEntered, "native_setup_entered"),
            (NativePhase::MigrationsReady, "native_migrations_ready"),
            (NativePhase::IdentityReady, "native_identity_ready"),
            (NativePhase::NestReady, "native_nest_ready"),
            (NativePhase::SetupCompleted, "native_setup_completed"),
            (NativePhase::PageLoadStarted, "native_page_load_started"),
            (NativePhase::PageLoadFinished, "native_page_load_finished"),
        ] {
            assert_eq!(checked_native_phase("main", true, false, phase), Ok(marker));
            assert_eq!(
                checked_native_phase("other", true, false, phase),
                Err("invalid_response")
            );
            assert_eq!(
                checked_native_phase("main", false, false, phase),
                Err("hook_disabled")
            );
            assert_eq!(
                checked_native_phase("main", true, true, phase),
                Err("shutdown")
            );
            assert_eq!(
                checked_phase("main", true, false, marker),
                Err("invalid_phase")
            );
        }
    }

    #[test]
    fn phases_require_active_main_scope_and_never_echo_invalid_input() {
        for phase in [
            "bootstrap_started",
            "bootstrap_completed",
            "bootstrap_failed",
            "registration_started",
            "registration_ready",
            "registration_failed",
            "request_received",
            "identity_started",
            "identity_ready",
            "identity_error",
            "relay_started",
            "relay_ready",
            "relay_error",
            "reply_started",
            "reply_accepted",
            "reply_failed",
        ] {
            assert_eq!(checked_phase("main", true, false, phase), Ok(phase));
            assert_eq!(
                checked_phase("other", true, false, phase),
                Err("invalid_response")
            );
            assert_eq!(
                checked_phase("main", false, false, phase),
                Err("hook_disabled")
            );
            assert_eq!(checked_phase("main", true, true, phase), Err("shutdown"));
        }
        for rejected in [
            "",
            "secret_value",
            "reply_accepted\nsecret_value",
            "IDENTITY_READY",
        ] {
            assert_eq!(
                checked_phase("main", true, false, rejected),
                Err("invalid_phase")
            );
        }
    }

    fn request() -> Request {
        Request {
            schema: SCHEMA.into(),
            request_id: uuid::Uuid::new_v4().to_string(),
            operation: "status".into(),
            expected: Expected {
                owner_pubkey: OWNER.into(),
                architect_pubkey: ARCHITECT.into(),
                relay_url: "https://buzz.rowvia.ai:8443".into(),
            },
            arguments: serde_json::Map::new(),
        }
    }

    #[test]
    fn missing_opt_in_and_private_parent() {
        assert_eq!(configured_path(None), Ok(None));
        let dir = tempfile::tempdir().unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = fs::canonicalize(dir.path()).unwrap().join("owner.sock");
        assert_eq!(
            configured_path(Some(path.clone().into_os_string())),
            Ok(Some(path.clone()))
        );
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(configured_path(Some(path.into_os_string())).is_err());
    }

    include!("owner_test_hook_message_tests.rs");

    #[test]
    fn cerberus_recovery_stop_is_native_closed_and_expiring() {
        let mut r = request();
        r.operation = "stop_cerberus_for_recovery".into();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        r.arguments =
            serde_json::from_value(serde_json::json!({"expiresAt":now + 60_000})).unwrap();
        assert!(validate_request(&serde_json::to_vec(&r).unwrap()).is_ok());
        r.arguments
            .insert("pubkey".into(), Value::String("arbitrary".into()));
        assert!(validate_request(&serde_json::to_vec(&r).unwrap()).is_err());
        r.arguments = serde_json::from_value(serde_json::json!({"expiresAt":now - 1})).unwrap();
        assert!(validate_request(&serde_json::to_vec(&r).unwrap()).is_err());
    }

    #[test]
    fn rejects_oversize_operation_and_scope() {
        assert!(validate_request(&vec![b' '; MAX_REQUEST + 1]).is_err());
        let mut r = request();
        assert!(validate_request(&serde_json::to_vec(&r).unwrap()).is_ok());
        r.operation = "export_keys".into();
        assert!(validate_request(&serde_json::to_vec(&r).unwrap()).is_err());
        r.operation = "status".into();
        r.expected.relay_url = "https://foreign.example".into();
        assert!(validate_request(&serde_json::to_vec(&r).unwrap()).is_err());
    }

    #[test]
    fn saved_developer_dispatch_has_closed_arguments_and_existing_scope_guards() {
        for operation in [
            "inspect_saved_developer",
            "configure_saved_developer",
            "start_saved_developer",
        ] {
            let mut r = request();
            r.operation = operation.into();
            r.arguments = if operation == "start_saved_developer" {
                serde_json::json!({"fingerprint":"a".repeat(64)})
                    .as_object()
                    .unwrap()
                    .clone()
            } else {
                serde_json::Map::new()
            };
            assert!(validate_request(&serde_json::to_vec(&r).unwrap()).is_ok());
            for field in ["pubkey", "command", "env", "eval"] {
                let mut invalid = r.clone();
                invalid
                    .arguments
                    .insert(field.into(), Value::String("forbidden".into()));
                assert!(matches!(
                    validate_request(&serde_json::to_vec(&invalid).unwrap()),
                    Err(Code::InvalidRequest)
                ));
            }
            r.expected.owner_pubkey = "other".into();
            assert!(matches!(
                validate_request(&serde_json::to_vec(&r).unwrap()),
                Err(Code::ScopeMismatch)
            ));
        }
    }

    #[test]
    fn proposal_ui_inspection_is_zero_argument_and_preserves_all_scope_guards() {
        let mut value = serde_json::to_value(request()).unwrap();
        value["operation"] = Value::String("inspect_cerberus_proposal_ui".into());
        value["arguments"] = serde_json::json!({});
        assert!(validate_request(&serde_json::to_vec(&value).unwrap()).is_ok());
        for field in ["ownerPubkey", "architectPubkey", "relayUrl"] {
            let mut invalid = value.clone();
            invalid["expected"][field] = Value::String("foreign".into());
            assert!(matches!(
                validate_request(&serde_json::to_vec(&invalid).unwrap()),
                Err(Code::ScopeMismatch)
            ));
        }
        for argument in ["channelId", "pubkey", "selector", "eval"] {
            let mut invalid = value.clone();
            invalid["arguments"][argument] = Value::String("foreign".into());
            assert!(matches!(
                validate_request(&serde_json::to_vec(&invalid).unwrap()),
                Err(Code::InvalidRequest)
            ));
        }
        value["operation"] = Value::String("inspect_cerberus_proposal_ui_extra".into());
        assert!(matches!(
            validate_request(&serde_json::to_vec(&value).unwrap()),
            Err(Code::InvalidRequest)
        ));
    }

    #[test]
    fn runtime_inspection_passes_native_validator_without_opening_other_operations() {
        let mut r = request();
        r.operation = "inspect_managed_agent_runtime".into();
        r.arguments = serde_json::json!({
            "pubkey": "c".repeat(64),
            "relayUrl": "wss://buzz.rowvia.ai:8443"
        })
        .as_object()
        .unwrap()
        .clone();
        let validated = validate_request(&serde_json::to_vec(&r).unwrap()).unwrap();
        assert_eq!(validated, r);
        for operation in [
            "eval",
            "export_keys",
            "list_managed_agent_runtimes",
            "start_managed_agent_runtime",
            "stop_managed_agent_runtime",
            "restart_managed_agent_runtime",
            "inspect_managed_agent_runtime_extra",
        ] {
            r.operation = operation.into();
            assert!(matches!(
                validate_request(&serde_json::to_vec(&r).unwrap()),
                Err(Code::InvalidRequest)
            ));
        }
    }

    #[test]
    fn runtime_inspection_preserves_native_envelope_size_schema_and_scope_guards() {
        let mut value = serde_json::to_value(request()).unwrap();
        value["operation"] = Value::String("inspect_managed_agent_runtime".into());
        for (field, replacement) in [
            ("schema", Value::String("other/v1".into())),
            ("requestId", Value::String("invalid".into())),
            ("extra", Value::Bool(true)),
        ] {
            let mut invalid = value.clone();
            invalid[field] = replacement;
            let result = validate_request(&serde_json::to_vec(&invalid).unwrap());
            assert!(matches!(result, Err(Code::InvalidRequest)));
        }
        for field in ["ownerPubkey", "architectPubkey", "relayUrl"] {
            let mut invalid = value.clone();
            invalid["expected"][field] = Value::String("foreign".into());
            assert!(matches!(
                validate_request(&serde_json::to_vec(&invalid).unwrap()),
                Err(Code::ScopeMismatch)
            ));
        }
        value["arguments"]["padding"] = Value::String("x".repeat(MAX_REQUEST));
        assert!(matches!(
            validate_request(&serde_json::to_vec(&value).unwrap()),
            Err(Code::InvalidRequest)
        ));
    }

    #[test]
    fn runtime_ensure_passes_native_validator_without_opening_direct_lifecycle_operations() {
        let mut r = request();
        r.operation = "ensure_managed_agent_runtime".into();
        r.arguments = serde_json::json!({
            "pubkey": "9d3972d809d53ce8a87003ca7f631009963d9414739188cea6209ced88365c87",
            "relayUrl": "wss://buzz.rowvia.ai:8443"
        })
        .as_object()
        .unwrap()
        .clone();
        assert_eq!(
            validate_request(&serde_json::to_vec(&r).unwrap()).unwrap(),
            r
        );
        for operation in [
            "start_managed_agent_runtime",
            "stop_managed_agent_runtime",
            "restart_managed_agent_runtime",
            "ensure_managed_agent_runtime_extra",
        ] {
            r.operation = operation.into();
            assert!(matches!(
                validate_request(&serde_json::to_vec(&r).unwrap()),
                Err(Code::InvalidRequest)
            ));
        }
    }

    #[test]
    fn replies_have_only_closed_codes_and_bounded_results() {
        let r = request();
        let invalid_code = serde_json::json!({ "schema": SCHEMA, "requestId": r.request_id, "status": "error", "code": "arbitrary_exception" });
        assert!(serde_json::from_value::<Response>(invalid_code).is_err());
        let mut result = serde_json::Map::new();
        result.insert("content".into(), Value::String("x".repeat(MAX_RESPONSE)));
        assert!(!Response {
            schema: SCHEMA.into(),
            request_id: r.request_id,
            status: Status::Ok,
            result: Some(result),
            code: None
        }
        .valid());
    }

    #[test]
    fn pending_requests_are_capped() {
        let mut ledger = Ledger::default();
        for _ in 0..MAX_PENDING {
            assert!(matches!(ledger.admit(&request()), Admission::Dispatch(_)));
        }
        assert!(matches!(ledger.admit(&request()), Admission::Return(_)));
        assert_eq!(ledger.entries.len(), MAX_PENDING);
    }

    #[test]
    fn correlates_and_never_dispatches_duplicates_or_timed_out_work() {
        let r = request();
        let mut ledger = Ledger::default();
        assert!(matches!(ledger.admit(&r), Admission::Dispatch(_)));
        assert!(matches!(ledger.admit(&r), Admission::Return(_)));
        assert!(ledger
            .complete(Response::failure(
                &uuid::Uuid::new_v4().to_string(),
                Status::Unknown,
                Code::Timeout
            ))
            .is_err());
        let timed_out = Response::failure(&r.request_id, Status::Unknown, Code::Timeout);
        assert!(ledger.complete(timed_out.clone()).is_ok());
        assert!(ledger.complete(timed_out).is_err());
        let Admission::Return(repeated) = ledger.admit(&r) else {
            panic!("duplicate was dispatched")
        };
        assert_eq!(repeated.status, Status::Unknown);
        let mut changed = r;
        changed.operation = "send_message".into();
        assert!(matches!(ledger.admit(&changed), Admission::Return(_)));
    }

    #[test]
    fn capacity_does_not_evict_previous_ids() {
        let mut ledger = Ledger::default();
        let first = request();
        for r in std::iter::once(first.clone()).chain((1..MAX_CACHE).map(|_| request())) {
            assert!(matches!(ledger.admit(&r), Admission::Dispatch(_)));
            assert!(ledger
                .complete(Response::failure(
                    &r.request_id,
                    Status::Unknown,
                    Code::Timeout
                ))
                .is_ok());
        }
        assert!(matches!(ledger.admit(&request()), Admission::Return(_)));
        assert!(matches!(ledger.admit(&first), Admission::Return(_)));
        assert_eq!(ledger.entries.len(), MAX_CACHE);
    }

    #[tokio::test]
    async fn timeout_is_cached_and_refuses_late_reply() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("owner.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let metadata = fs::symlink_metadata(&path).unwrap();
        let hook = Hook {
            ledger: Mutex::new(Ledger::default()),
            cancel: CancellationToken::new(),
            socket: SocketGuard {
                path,
                device: metadata.dev(),
                inode: metadata.ino(),
                uid: metadata.uid(),
                cleanup_attempted: AtomicBool::new(false),
            },
        };
        let r = request();
        let Admission::Dispatch(rx) = hook.ledger.lock().unwrap().admit(&r) else {
            panic!("expected admission")
        };
        let response = wait_for_reply(&hook, &r.request_id, rx, tokio::time::Instant::now()).await;
        assert_eq!(response.status, Status::Unknown);
        let mut ledger = hook.ledger.lock().unwrap();
        assert!(ledger
            .complete(Response {
                schema: SCHEMA.into(),
                request_id: r.request_id.clone(),
                status: Status::Ok,
                result: Some(serde_json::Map::new()),
                code: None
            })
            .is_err());
        assert!(matches!(ledger.admit(&r), Admission::Return(_)));
    }

    #[tokio::test]
    async fn owner_test_connector_context_requires_native_live_ledger_and_cannot_reopen_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("connector-owner.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let metadata = fs::symlink_metadata(&path).unwrap();
        let hook = Hook {
            ledger: Mutex::new(Ledger::default()),
            cancel: CancellationToken::new(),
            socket: SocketGuard {
                path,
                device: metadata.dev(),
                inode: metadata.ino(),
                uid: metadata.uid(),
                cleanup_attempted: AtomicBool::new(false),
            },
        };
        let mut r = request();
        r.operation = "prepare_saved_connector".into();
        r.arguments.clear();
        assert!(validate_request(&serde_json::to_vec(&r).unwrap()).is_ok());
        assert!(pending_connector_request(&hook, &r.request_id).is_err());
        let Admission::Dispatch(rx) = hook.ledger.lock().unwrap().admit(&r) else {
            panic!("admission");
        };
        assert!(pending_connector_request(&hook, &r.request_id).is_err());
        hook.ledger
            .lock()
            .unwrap()
            .entries
            .get_mut(&r.request_id)
            .unwrap()
            .deadline = Some(tokio::time::Instant::now() + Duration::from_secs(10));
        assert!(pending_connector_request(&hook, &r.request_id).is_ok());
        let response = wait_for_reply(&hook, &r.request_id, rx, tokio::time::Instant::now()).await;
        assert_eq!(response.status, Status::Unknown);
        assert!(pending_connector_request(&hook, &r.request_id).is_err());
        assert!(matches!(
            hook.ledger.lock().unwrap().admit(&r),
            Admission::Return(_)
        ));
        hook.cancel.cancel();
        assert!(matches!(
            pending_connector_request(&hook, &r.request_id),
            Err("shutdown")
        ));
        r.arguments
            .insert("action".into(), serde_json::json!("connector.revoke"));
        assert!(matches!(
            validate_request(&serde_json::to_vec(&r).unwrap()),
            Err(Code::InvalidRequest)
        ));
    }

    #[tokio::test]
    async fn native_dispatch_bounds_wait_caches_unknown_and_refuses_late_completion() {
        for cancel in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("native-owner.sock");
            let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
            let metadata = fs::symlink_metadata(&path).unwrap();
            let hook = Arc::new(Hook {
                ledger: Mutex::new(Ledger::default()),
                cancel: CancellationToken::new(),
                socket: SocketGuard {
                    path,
                    device: metadata.dev(),
                    inode: metadata.ino(),
                    uid: metadata.uid(),
                    cleanup_attempted: AtomicBool::new(false),
                },
            });
            let mut r = request();
            r.operation = "start_saved_developer".into();
            let Admission::Dispatch(_rx) = hook.ledger.lock().unwrap().admit(&r) else {
                panic!("expected native admission")
            };
            let (started_tx, started_rx) = oneshot::channel();
            let (finished_tx, finished_rx) = oneshot::channel();
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let native_hook = Arc::clone(&hook);
            let native_request = r.clone();
            let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
            let mut task = tokio::spawn(async move {
                dispatch_native(&native_hook, &native_request, deadline, move || {
                    let _ = started_tx.send(());
                    let _ = release_rx.recv_timeout(Duration::from_secs(5));
                    let _ = finished_tx.send(());
                    Ok(serde_json::Map::new())
                })
                .await
            });
            // A live blocking worker inhibits Tokio's paused-time auto-advance;
            // use finite wall-clock waits so even a deleted guard cannot hang.
            started_rx.await.unwrap();
            if cancel {
                hook.cancel.cancel();
            }
            let bounded = tokio::time::timeout(Duration::from_secs(3), &mut task).await;
            // Always release the inert worker before assertions, even if a
            // deleted timeout/cancel guard makes the bounded wait fail.
            let _ = release_tx.send(());
            tokio::time::timeout(Duration::from_secs(2), finished_rx)
                .await
                .unwrap()
                .unwrap();
            if bounded.is_err() {
                task.abort();
            }
            let response = bounded
                .expect("native dispatch must release its slot")
                .unwrap();
            assert_eq!(response.status, Status::Unknown);
            if cancel {
                assert!(matches!(response.code, Some(Code::Shutdown)));
            } else {
                assert!(matches!(response.code, Some(Code::Timeout)));
            }
            let mut ledger = hook.ledger.lock().unwrap();
            assert!(ledger
                .complete(Response {
                    schema: SCHEMA.into(),
                    request_id: r.request_id.clone(),
                    status: Status::Ok,
                    result: Some(serde_json::Map::new()),
                    code: None
                })
                .is_err());
            let Admission::Return(repeated) = ledger.admit(&r) else {
                panic!("unknown native start was dispatched twice")
            };
            assert_eq!(repeated.status, Status::Unknown);
            assert!(matches!(ledger.admit(&request()), Admission::Dispatch(_)));
        }
    }

    #[tokio::test]
    async fn socket_frame_is_bounded() {
        let (mut input, mut output) = UnixStream::pair().unwrap();
        let writer =
            tokio::spawn(async move { output.write_all(&vec![b'x'; MAX_REQUEST + 1]).await });
        assert!(read_request(&mut input).await.is_err());
        writer.await.unwrap().unwrap();
    }

    #[test]
    fn refuses_existing_socket_and_replacement_cleanup() {
        let dir = tempfile::tempdir().unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = fs::canonicalize(dir.path()).unwrap().join("owner.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let metadata = fs::symlink_metadata(&path).unwrap();
        let guard = SocketGuard {
            path: path.clone(),
            device: metadata.dev(),
            inode: metadata.ino(),
            uid: metadata.uid(),
            cleanup_attempted: AtomicBool::new(false),
        };
        assert!(configured_path(Some(path.clone().into_os_string())).is_err());
        guard.remove();
        assert!(!path.exists());
        drop(listener);
        let replacement = std::os::unix::net::UnixListener::bind(&path).unwrap();
        guard.remove();
        assert!(path.exists());
        drop(replacement);
    }
}
