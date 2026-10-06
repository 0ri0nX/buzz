//! Temporary, opt-in local bridge to the already authenticated main webview.
//! No signing, identity storage, generic invocation or script execution surface.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::PathBuf;
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
}

impl SocketGuard {
    fn remove(&self) {
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
            "status" | "create_private_stream" | "add_architect" | "send_message" | "read_channel"
        )
    {
        return Err(Code::InvalidRequest);
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

fn checked_phase(
    window_label: &str,
    hook_enabled: bool,
    shutdown: bool,
    phase: &str,
) -> Result<&'static str, &'static str> {
    if window_label != "main" {
        return Err("invalid_response");
    }
    if !hook_enabled {
        return Err("hook_disabled");
    }
    if shutdown {
        return Err("shutdown");
    }
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
