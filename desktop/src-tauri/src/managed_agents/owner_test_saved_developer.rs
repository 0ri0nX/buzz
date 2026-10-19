//! Temporary fixed saved-developer handoff, reachable only through the opt-in hook.
//! No global store hydration, status synchronization, pending flush or catalog merge.
#[path = "owner_test_saved_developer_core.rs"]
mod core;
pub(crate) use core::{is_operation, validate_arguments};

use super::{custom_harnesses, ManagedAgentRecord, ManagedAgentRuntimeKey};
use crate::app_state::AppState;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    sync::Arc,
};
use tauri::{AppHandle, Manager};

const ACP_SOURCE_REPO: &str =
    "/home/orionx/project/RowviaContext/repos/buzz-cerberus-ui-diagnostic-20261015";
const ACP_SOURCE_COMMIT: &str = "d09d5977265385d59ad9619ee3304003b19d3822";

#[derive(Clone, Copy)]
struct AcpPin<'a> {
    activation_sha256: &'a str,
    executable_sha256: &'a str,
    executable_bytes: u64,
}

// Root-qualified exact committed release and activation bytes, not authority
// adopted from current files or the manifest's own provenance claims.
const ACP_PIN: Option<AcpPin<'static>> = Some(AcpPin {
    activation_sha256: "ab1b6178676ee0adc979139f840d8155575d00b6940e7867d5978edd0a737645",
    executable_sha256: "2c1d8308ca14d188d7be795b24164e9f6c6a7038c2a96db24c649625bc239e91",
    executable_bytes: 19_421_016,
});

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AcpActivation {
    schema: String,
    agent_pubkey: String,
    executable: AcpExecutable,
    provenance: AcpProvenance,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AcpExecutable {
    path: String,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AcpProvenance {
    source_repo: String,
    source_commit: String,
    package: String,
    profile: String,
    request_policy: String,
}

fn activation_metadata(metadata: &fs::Metadata) -> Value {
    json!([
        metadata.dev(),
        metadata.ino(),
        metadata.uid(),
        metadata.mode(),
        metadata.nlink(),
        metadata.len(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec()
    ])
}

fn activation_directory(path: &Path, uid: u32) -> Result<Value, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "acp_activation_unavailable")?;
    if !metadata.is_dir()
        || metadata.uid() != uid
        || metadata.mode() & 0o7777 != 0o700
        || fs::canonicalize(path).map_err(|_| "acp_activation_invalid")? != path
    {
        return Err("acp_activation_invalid".into());
    }
    Ok(activation_metadata(&metadata))
}

/// Open without following a leaf link or waiting for a FIFO peer, then fence
/// substitutions against the regular-file metadata captured before opening.
fn open_activation_file(path: &Path, stamp: &Value) -> Result<fs::File, String> {
    // A pathname can become a FIFO after lstat. Nonblocking open must precede
    // the descriptor metadata fence, otherwise that fence is never reached.
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "acp_activation_unavailable")?;
    if activation_metadata(&file.metadata().map_err(|_| "acp_activation_invalid")?) != *stamp {
        return Err("acp_activation_invalid".into());
    }
    Ok(file)
}

/// Hash a bounded regular, single-link file through one no-follow descriptor.
/// Recheck both that descriptor and its pathname after EOF. For the ELF retain
/// only its magic bytes, never the executable payload in memory or responses.
fn activation_file(
    path: &Path,
    uid: u32,
    mode: u32,
    cap: u64,
    collect: bool,
) -> Result<(Vec<u8>, String, Value), String> {
    let before = fs::symlink_metadata(path).map_err(|_| "acp_activation_unavailable")?;
    if !before.is_file()
        || before.uid() != uid
        || before.mode() & 0o7777 != mode
        || before.nlink() != 1
        || before.len() > cap
    {
        return Err("acp_activation_invalid".into());
    }
    let stamp = activation_metadata(&before);
    let mut file = open_activation_file(path, &stamp)?;
    let mut digest = Sha256::new();
    let mut bytes = Vec::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| "acp_activation_unavailable")?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or("acp_activation_invalid")?;
        if total > cap {
            return Err("acp_activation_invalid".into());
        }
        digest.update(&buffer[..count]);
        if collect {
            bytes.extend_from_slice(&buffer[..count]);
        } else if bytes.len() < 4 {
            bytes.extend_from_slice(&buffer[..count.min(4 - bytes.len())]);
        }
    }
    if total != before.len()
        || activation_metadata(&file.metadata().map_err(|_| "acp_activation_invalid")?) != stamp
        || activation_metadata(&fs::symlink_metadata(path).map_err(|_| "acp_activation_invalid")?)
            != stamp
    {
        return Err("acp_activation_invalid".into());
    }
    Ok((bytes, hex::encode(digest.finalize()), stamp))
}

/// Private layout seam for inert file fixtures; production always supplies the
/// fixed root and compiled Root pin, never request-selected paths or metadata.
fn verify_activation(root: &Path, uid: u32, pin: AcpPin<'_>) -> Result<Value, String> {
    let parent = root.parent().ok_or("acp_activation_invalid")?;
    let parent_stamp = activation_directory(parent, uid)?;
    let root_stamp = activation_directory(root, uid)?;
    let (bytes, activation_hash, activation_stamp) =
        activation_file(&root.join("activation.json"), uid, 0o600, 16 * 1024, true)?;
    if activation_hash != pin.activation_sha256 {
        return Err("acp_activation_invalid".into());
    }
    let activation: AcpActivation =
        serde_json::from_slice(&bytes).map_err(|_| "acp_activation_invalid")?;
    if activation.schema != "io.rowvia.saved-developer-acp.activation/v1"
        || activation.agent_pubkey != core::TARGET
        || activation.executable.path != core::ACP_EXECUTABLE
        || activation.executable.sha256 != pin.executable_sha256
        || activation.provenance.source_repo != ACP_SOURCE_REPO
        || activation.provenance.source_commit != ACP_SOURCE_COMMIT
        || activation.provenance.package != "buzz-acp"
        || activation.provenance.profile != "release"
        || activation.provenance.request_policy != core::ACP_POLICY
    {
        return Err("acp_activation_invalid".into());
    }
    let (magic, executable_hash, executable_stamp) = activation_file(
        &root.join("buzz-acp"),
        uid,
        0o700,
        pin.executable_bytes,
        false,
    )?;
    if magic != b"\x7fELF"
        || executable_hash != pin.executable_sha256
        || executable_stamp[5] != pin.executable_bytes
        || activation_directory(root, uid)? != root_stamp
        || activation_directory(parent, uid)? != parent_stamp
    {
        return Err("acp_activation_invalid".into());
    }
    // Re-read the small manifest after hashing the ELF, fencing substitutions
    // between the two reads rather than returning a mixed generation.
    let (_, current_hash, current_stamp) =
        activation_file(&root.join("activation.json"), uid, 0o600, 16 * 1024, true)?;
    if current_hash != activation_hash
        || current_stamp != activation_stamp
        || activation_directory(root, uid)? != root_stamp
        || activation_directory(parent, uid)? != parent_stamp
        || activation_metadata(
            &fs::symlink_metadata(root.join("buzz-acp")).map_err(|_| "acp_activation_invalid")?,
        ) != executable_stamp
    {
        return Err("acp_activation_invalid".into());
    }
    Ok(json!([
        ACP_SOURCE_COMMIT,
        core::ACP_POLICY,
        activation_hash,
        executable_hash,
        parent_stamp,
        root_stamp,
        activation_stamp,
        executable_stamp
    ]))
}

fn pinned_activation(uid: u32) -> Result<Value, String> {
    verify_activation(
        Path::new(core::ACP_ROOT),
        uid,
        ACP_PIN.ok_or("acp_activation_unpinned")?,
    )
}

fn bind_activation(context_digest: &str, activation: &Value) -> Result<String, String> {
    let bytes =
        serde_json::to_vec(&json!([context_digest, activation])).map_err(|_| "operation_failed")?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn require_no_initial_message(present: bool) -> Result<(), String> {
    if present {
        return Err("target_effective_config_mismatch".into());
    }
    Ok(())
}

struct CapturedContext {
    personas: Vec<super::AgentDefinition>,
    global: super::GlobalAgentConfig,
    digest: String,
}

/// The normal spawn resolvers, without catalog hydration or fallback on errors.
/// All configuration stays internal: only its digest enters the owner token.
fn capture_context(
    rows: &[Value],
    record: &ManagedAgentRecord,
    global: super::GlobalAgentConfig,
) -> Result<CapturedContext, String> {
    if record.pubkey != core::TARGET
        || record.runtime.as_deref() != Some(core::HARNESS_ID)
        || record.acp_command != core::ACP_EXECUTABLE
    {
        return Err("target_contract_mismatch".into());
    }
    let raw = serde_json::to_value(record).map_err(|_| "target_contract_mismatch")?;
    let mut original_definition = core::linked_definition(rows, &raw)?.cloned();
    // Signing material is not configuration authority or part of this token.
    if let Some(object) = original_definition.as_mut().and_then(Value::as_object_mut) {
        object.remove("private_key_nsec");
        object.remove("auth_tag");
    }
    let personas = core::linked_definition(rows, &raw)?
        .map(|row| {
            let mut definition = serde_json::from_value::<ManagedAgentRecord>(row.clone())
                .map_err(|_| "target_contract_mismatch")?
                .to_definition_view()
                .ok_or("target_contract_mismatch")?;
            // Explicit owner-approved target-only policy, not inherited defaults.
            // The shared saved definition and its prompt/env/runtime stay intact.
            definition.model = Some("gpt-6.1-sol".into());
            definition.provider = Some("codex".into());
            Ok::<_, &'static str>(definition)
        })
        .transpose()?
        .into_iter()
        .collect::<Vec<_>>();
    let effective = super::effective_config::resolve_effective_config(record, &personas, &global)
        .require_resolved()
        .map_err(|_| "target_effective_config_mismatch")?;
    if effective.model.value.as_deref() != Some("gpt-6.1-sol")
        || effective.provider.value.as_deref() != Some("codex")
    {
        return Err("target_effective_config_mismatch".into());
    }
    let descriptor = super::resolve_effective_harness_descriptor(record, &personas, &global)
        .map_err(|_| "target_effective_config_mismatch")?;
    if descriptor.command != core::PYTHON
        || descriptor.args != vec!["-IS".to_owned(), format!("{}/run", core::CAPSULE)]
        || descriptor.env.get("BUZZ_ACP_LAZY_POOL").map(String::as_str) != Some("true")
        || descriptor
            .env
            .get("BUZZ_ACP_NO_MENTION_FILTER")
            .map(String::as_str)
            != Some("false")
        || descriptor
            .env
            .get("BUZZ_ACP_STARTUP_LIVE_ONLY")
            .map(String::as_str)
            != Some("true")
        || descriptor
            .env
            .get("BUZZ_ACP_HEARTBEAT_INTERVAL")
            .map(String::as_str)
            != Some("0")
        || descriptor.env.get("BUZZ_ACP_CHANNELS").map(String::as_str) != Some(core::CHANNEL)
    {
        return Err("target_effective_config_mismatch".into());
    }
    require_no_initial_message(descriptor.env.contains_key("BUZZ_ACP_INITIAL_MESSAGE"))?;
    let bytes = serde_json::to_vec(&json!([
        "fixed-saved-developer-model-provider/v1",
        original_definition,
        personas,
        global,
        effective.model.value,
        effective.provider.value,
        effective.system_prompt.value,
        descriptor.command,
        descriptor.args,
        descriptor.env
    ]))
    .map_err(|_| "operation_failed")?;
    Ok(CapturedContext {
        personas,
        global,
        digest: hex::encode(Sha256::digest(bytes)),
    })
}

fn load_context<R: tauri::Runtime>(
    app: &AppHandle<R>,
    rows: &[Value],
    record: &ManagedAgentRecord,
) -> Result<CapturedContext, String> {
    require_no_initial_message(std::env::var_os("BUZZ_ACP_INITIAL_MESSAGE").is_some())?;
    let global = super::load_global_agent_config(app).map_err(|_| "target_context_unavailable")?;
    capture_context(rows, record, global)
}

fn recheck_context<R: tauri::Runtime>(
    app: &AppHandle<R>,
    rows: &[Value],
    record: &ManagedAgentRecord,
    expected: &str,
) -> Result<CapturedContext, String> {
    let context = load_context(app, rows, record)?;
    if context.digest != expected {
        return Err("fingerprint_mismatch".into());
    }
    Ok(context)
}

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
    let activation = pinned_activation(uid)?;
    let stamp = capsule_stamp(uid);
    // Derive the prospective fixed record first, then bind normal effective
    // persona/global resolution before admitting any write or start.
    let prospective = core::plan(
        "inspect_saved_developer",
        &Map::new(),
        core::Scope {
            owner: &owner,
            relay: &relay,
            locked,
            context_digest: "",
        },
        &rows,
        occupied,
        existing.as_deref(),
        stamp.as_ref(),
    )?;
    let mut record: ManagedAgentRecord =
        serde_json::from_value(prospective.record).map_err(|_| "target_contract_mismatch")?;
    let context = load_context(app, &rows, &record)?;
    let context_digest = bind_activation(&context.digest, &activation)?;
    let plan = core::plan(
        operation,
        args,
        core::Scope {
            owner: &owner,
            relay: &relay,
            locked,
            context_digest: &context_digest,
        },
        &rows,
        occupied,
        existing.as_deref(),
        stamp.as_ref(),
    )?;
    // Validate the target's real persisted schema before any write; unrelated
    // rows remain raw JSON and never enter generic key hydration/normalization.
    let result = json!({"pubkey":core::TARGET,"relayUrl":core::RELAY,"channelId":core::CHANNEL,
        "fingerprint":plan.fingerprint,"configured":plan.configured || plan.write,
        "prerequisitesPresent":stamp.is_some(),"runtimeReady":false,"runtimeOccupied":occupied || record.runtime_pid.is_some() || plan.start,
        "startDispatched":plan.start});
    if plan.write {
        if pinned_activation(uid)? != activation {
            return Err("fingerprint_mismatch".into());
        }
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
        // Global setters have no shared mutex. Re-read before start and refuse
        // drift, then use these exact captured values through the spawn boundary.
        // This is captured-value consistency, not serialization of future edits.
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
        let current_context = recheck_context(app, &rows, &record, &context.digest)?;
        if pinned_activation(uid)? != activation || capsule_stamp(uid) != stamp {
            return Err("fingerprint_mismatch".into());
        }
        let process = super::runtime::spawn_agent_child_with_context(
            app,
            &record,
            core::RELAY,
            &admitted,
            true,
            Some(core::OWNER),
            None,
            &current_context.personas,
            &[],
            &current_context.global,
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
    use std::os::unix::fs::{symlink, PermissionsExt};

    struct ActivationFixture {
        parent: tempfile::TempDir,
        root: std::path::PathBuf,
        executable_sha256: String,
        activation_sha256: String,
        executable_bytes: u64,
        uid: u32,
    }

    impl ActivationFixture {
        fn pin(&self) -> AcpPin<'_> {
            AcpPin {
                activation_sha256: &self.activation_sha256,
                executable_sha256: &self.executable_sha256,
                executable_bytes: self.executable_bytes,
            }
        }

        fn write_activation(&mut self, value: &Value) {
            let bytes = serde_json::to_vec(value).unwrap();
            fs::write(self.root.join("activation.json"), &bytes).unwrap();
            fs::set_permissions(
                self.root.join("activation.json"),
                fs::Permissions::from_mode(0o600),
            )
            .unwrap();
            self.activation_sha256 = hex::encode(Sha256::digest(bytes));
        }
    }

    fn activation_value(executable_sha256: &str) -> Value {
        json!({"schema":"io.rowvia.saved-developer-acp.activation/v1", "agentPubkey":core::TARGET,
            "executable":{"path":core::ACP_EXECUTABLE,"sha256":executable_sha256},
            "provenance":{"sourceRepo":ACP_SOURCE_REPO,"sourceCommit":ACP_SOURCE_COMMIT,
                "package":"buzz-acp","profile":"release","requestPolicy":core::ACP_POLICY}})
    }

    fn activation_fixture() -> ActivationFixture {
        let parent = tempfile::tempdir().unwrap();
        fs::set_permissions(parent.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let root = parent.path().join("developer-acp");
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        // Inert bytes: the validator never executes even a fixture ELF.
        let payload = b"\x7fELFinert-activation-fixture";
        fs::write(root.join("buzz-acp"), payload).unwrap();
        fs::set_permissions(root.join("buzz-acp"), fs::Permissions::from_mode(0o700)).unwrap();
        let mut fixture = ActivationFixture {
            uid: fs::metadata(parent.path()).unwrap().uid(),
            parent,
            root,
            executable_sha256: hex::encode(Sha256::digest(payload)),
            activation_sha256: String::new(),
            executable_bytes: payload.len() as u64,
        };
        let value = activation_value(&fixture.executable_sha256);
        fixture.write_activation(&value);
        assert!(verify_activation(&fixture.root, fixture.uid, fixture.pin()).is_ok());
        fixture
    }

    #[test]
    fn actual_activation_open_is_nonblocking_and_refuses_a_deterministic_fifo_swap() {
        use std::os::fd::AsRawFd;
        let fixture = activation_fixture();
        let path = fixture.root.join("activation.json");
        let stamp = activation_metadata(&fs::symlink_metadata(&path).unwrap());
        let file = open_activation_file(&path, &stamp).unwrap();
        let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFL) };
        assert!(flags >= 0);
        // This assertion fails before the FIFO attempt if O_NONBLOCK is removed,
        // so the negative qualification cannot hang waiting for a FIFO writer.
        assert_ne!(flags & libc::O_NONBLOCK, 0);
        drop(file);
        fs::rename(&path, fixture.root.join("original-activation.json")).unwrap();
        let fifo_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_path.as_ptr(), 0o600) }, 0);
        assert_eq!(
            open_activation_file(&path, &stamp).err().as_deref(),
            Some("acp_activation_invalid")
        );
    }

    #[test]
    fn actual_activation_validator_binds_exact_pins_and_preserves_inert_artifacts() {
        let fixture = activation_fixture();
        let before = fs::read(fixture.root.join("buzz-acp")).unwrap();
        let stamp = verify_activation(&fixture.root, fixture.uid, fixture.pin()).unwrap();
        assert_eq!(
            verify_activation(&fixture.root, fixture.uid, fixture.pin()).unwrap(),
            stamp
        );
        let digest = bind_activation("captured-context", &stamp).unwrap();
        assert_ne!(bind_activation("changed-context", &stamp).unwrap(), digest);
        assert_ne!(
            bind_activation("captured-context", &json!(["changed-artifact"])).unwrap(),
            digest
        );
        assert_eq!(fs::read(fixture.root.join("buzz-acp")).unwrap(), before);
        let wrong_pin = AcpPin {
            activation_sha256: "not-the-root-pin",
            ..fixture.pin()
        };
        assert!(verify_activation(&fixture.root, fixture.uid, wrong_pin).is_err());
        let bytes = fs::read(fixture.root.join("activation.json")).unwrap();
        let replacement = fixture.root.join("replacement.json");
        fs::write(&replacement, bytes).unwrap();
        fs::set_permissions(&replacement, fs::Permissions::from_mode(0o600)).unwrap();
        fs::rename(replacement, fixture.root.join("activation.json")).unwrap();
        let replaced = verify_activation(&fixture.root, fixture.uid, fixture.pin()).unwrap();
        assert_ne!(stamp, replaced);
        assert_ne!(
            bind_activation("captured-context", &replaced).unwrap(),
            digest
        );
        fs::write(
            fixture.root.join("buzz-acp"),
            b"\x7fELFsubstituted-artifact",
        )
        .unwrap();
        assert!(verify_activation(&fixture.root, fixture.uid, fixture.pin()).is_err());
    }

    #[test]
    fn actual_activation_closed_schema_never_adopts_self_reported_provenance() {
        let mut fixture = activation_fixture();
        for pointer in [
            "/schema",
            "/agentPubkey",
            "/executable/path",
            "/executable/sha256",
            "/provenance/sourceRepo",
            "/provenance/sourceCommit",
            "/provenance/package",
            "/provenance/profile",
            "/provenance/requestPolicy",
        ] {
            let mut value = activation_value(&fixture.executable_sha256);
            *value.pointer_mut(pointer).unwrap() = json!("foreign-unapproved-value");
            // A matching outer digest alone must not authorize different fields.
            fixture.write_activation(&value);
            assert_eq!(
                verify_activation(&fixture.root, fixture.uid, fixture.pin())
                    .err()
                    .as_deref(),
                Some("acp_activation_invalid")
            );
        }
        for pointer in ["", "/executable", "/provenance"] {
            let mut value = activation_value(&fixture.executable_sha256);
            value
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unknown".into(), json!(true));
            fixture.write_activation(&value);
            assert!(verify_activation(&fixture.root, fixture.uid, fixture.pin()).is_err());
        }
        let value = activation_value(&fixture.executable_sha256);
        fixture.write_activation(&value);
        fs::write(fixture.root.join("activation.json"), b"invalid-json").unwrap();
        fixture.activation_sha256 = hex::encode(Sha256::digest(b"invalid-json"));
        assert!(verify_activation(&fixture.root, fixture.uid, fixture.pin()).is_err());
        let duplicate = format!(
            "{{\"schema\":\"io.rowvia.saved-developer-acp.activation/v1\",{}",
            &serde_json::to_string(&value).unwrap()[1..]
        );
        fs::write(fixture.root.join("activation.json"), duplicate.as_bytes()).unwrap();
        fixture.activation_sha256 = hex::encode(Sha256::digest(duplicate.as_bytes()));
        assert!(verify_activation(&fixture.root, fixture.uid, fixture.pin()).is_err());
    }

    #[test]
    fn actual_activation_refuses_missing_links_modes_owner_and_size_changes() {
        for name in ["activation.json", "buzz-acp"] {
            let fixture = activation_fixture();
            fs::remove_file(fixture.root.join(name)).unwrap();
            assert!(verify_activation(&fixture.root, fixture.uid, fixture.pin()).is_err());
            let fixture = activation_fixture();
            fs::hard_link(fixture.root.join(name), fixture.root.join("extra-hardlink")).unwrap();
            assert!(verify_activation(&fixture.root, fixture.uid, fixture.pin()).is_err());
            let fixture = activation_fixture();
            fs::rename(fixture.root.join(name), fixture.root.join("original")).unwrap();
            symlink(fixture.root.join("original"), fixture.root.join(name)).unwrap();
            assert!(verify_activation(&fixture.root, fixture.uid, fixture.pin()).is_err());
        }
        for (name, mode) in [("activation.json", 0o644), ("buzz-acp", 0o755), ("", 0o755)] {
            let fixture = activation_fixture();
            fs::set_permissions(fixture.root.join(name), fs::Permissions::from_mode(mode)).unwrap();
            assert!(verify_activation(&fixture.root, fixture.uid, fixture.pin()).is_err());
        }
        let fixture = activation_fixture();
        assert!(
            verify_activation(&fixture.root, fixture.uid.wrapping_add(1), fixture.pin()).is_err()
        );
        assert!(activation_file(
            &fixture.root.join("buzz-acp"),
            fixture.uid.wrapping_add(1),
            0o700,
            fixture.executable_bytes,
            false
        )
        .is_err());
        let wrong_size = AcpPin {
            executable_bytes: fixture.executable_bytes - 1,
            ..fixture.pin()
        };
        assert!(verify_activation(&fixture.root, fixture.uid, wrong_size).is_err());
        let linked_root = fixture.parent.path().join("linked-root");
        symlink(&fixture.root, &linked_root).unwrap();
        assert!(verify_activation(&linked_root, fixture.uid, fixture.pin()).is_err());
        let fixture = activation_fixture();
        fs::set_permissions(fixture.parent.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(verify_activation(&fixture.root, fixture.uid, fixture.pin()).is_err());
        let fixture = activation_fixture();
        fs::write(
            fixture.root.join("activation.json"),
            vec![b'x'; 16 * 1024 + 1],
        )
        .unwrap();
        assert!(verify_activation(&fixture.root, fixture.uid, fixture.pin()).is_err());
    }

    #[test]
    fn ordinary_service_environment_refuses_initial_message_in_any_layer() {
        assert!(require_no_initial_message(false).is_ok());
        assert!(require_no_initial_message(true).is_err());
        let fixture = super::super::admission_test_support::app_with_keyless_agent();
        let rows = linked_fixture_rows(
            super::super::load_managed_agents(fixture.app.handle())
                .unwrap()
                .remove(0),
        );
        let record = prospective_record(&rows);
        for value in ["", "private-initial-message"] {
            let mut global = valid_global();
            global
                .env_vars
                .insert("BUZZ_ACP_INITIAL_MESSAGE".into(), value.into());
            assert_eq!(
                capture_context(&rows, &record, global).err().as_deref(),
                Some("target_effective_config_mismatch")
            );
            let mut changed = rows.clone();
            changed[1]["env_vars"]["BUZZ_ACP_INITIAL_MESSAGE"] = json!(value);
            assert!(capture_context(&changed, &record, valid_global()).is_err());
        }
        let mut task = record.clone();
        task.acp_command = format!("{} run --task unsupported", core::ACP_EXECUTABLE);
        assert!(capture_context(&rows, &task, valid_global()).is_err());
        for (key, value) in [
            ("BUZZ_ACP_STARTUP_LIVE_ONLY", "false"),
            ("BUZZ_ACP_HEARTBEAT_INTERVAL", "1"),
        ] {
            let mut invalid = record.clone();
            invalid.env_vars.insert(key.into(), value.into());
            assert!(capture_context(&rows, &invalid, valid_global()).is_err());
        }
    }

    fn linked_fixture_rows(mut record: ManagedAgentRecord) -> Vec<Value> {
        record.pubkey = core::TARGET.into();
        record.relay_url = String::new();
        record.persona_id = Some("saved-role".into());
        record.acp_command = "buzz-acp".into();
        let mut definition = record.clone();
        definition.pubkey.clear();
        definition.persona_id = None;
        definition.slug = Some("saved-role".into());
        definition.model = None;
        definition.provider = None;
        definition.system_prompt = Some("private-definition-prompt".into());
        definition.env_vars = std::collections::BTreeMap::from([
            ("PRIVATE_PERSONA".into(), "private-persona-env".into()),
            ("BUZZ_ACP_LAZY_POOL".into(), "false".into()),
        ]);
        vec![
            serde_json::to_value(record).unwrap(),
            serde_json::to_value(definition).unwrap(),
        ]
    }

    fn prospective_record(rows: &[Value]) -> ManagedAgentRecord {
        let plan = core::plan(
            "inspect_saved_developer",
            &Map::new(),
            core::Scope {
                owner: core::OWNER,
                relay: core::RELAY,
                locked: false,
                context_digest: "",
            },
            rows,
            false,
            None,
            None,
        )
        .unwrap();
        serde_json::from_value(plan.record).unwrap()
    }

    fn valid_global() -> super::super::GlobalAgentConfig {
        super::super::GlobalAgentConfig {
            model: Some("gpt-6.1-sol".into()),
            provider: Some("codex".into()),
            env_vars: std::collections::BTreeMap::from([(
                "PRIVATE_GLOBAL".into(),
                "private-global-env".into(),
            )]),
            ..Default::default()
        }
    }

    #[test]
    fn actual_resolvers_bind_target_only_overlay_original_definition_and_global_env() {
        let fixture = super::super::admission_test_support::app_with_keyless_agent();
        let rows = linked_fixture_rows(
            super::super::load_managed_agents(fixture.app.handle())
                .unwrap()
                .remove(0),
        );
        let preserved = rows.clone();
        let record = prospective_record(&rows);
        let context = capture_context(&rows, &record, valid_global()).unwrap();
        assert_eq!(context.personas.len(), 1);
        assert_eq!(context.personas[0].model.as_deref(), Some("gpt-6.1-sol"));
        assert_eq!(context.personas[0].provider.as_deref(), Some("codex"));
        assert_eq!(rows[1]["model"], Value::Null);
        assert_eq!(rows[1]["provider"], Value::Null);
        assert_eq!(record.persona_id.as_deref(), Some("saved-role"));
        assert_eq!(record.relay_url, "");
        let effective = super::super::effective_config::resolve_effective_config(
            &record,
            &context.personas,
            &context.global,
        )
        .require_resolved()
        .unwrap();
        assert_eq!(effective.model.value.as_deref(), Some("gpt-6.1-sol"));
        assert_eq!(effective.provider.value.as_deref(), Some("codex"));
        assert_eq!(
            effective.system_prompt.value.as_deref(),
            Some("private-definition-prompt")
        );
        let descriptor = super::super::resolve_effective_harness_descriptor(
            &record,
            &context.personas,
            &context.global,
        )
        .unwrap();
        for (key, value) in [
            ("PRIVATE_GLOBAL", "private-global-env"),
            ("PRIVATE_PERSONA", "private-persona-env"),
            ("BUZZ_ACP_LAZY_POOL", "true"),
        ] {
            assert_eq!(descriptor.env.get(key).map(String::as_str), Some(value));
        }
        let mut changed = rows.clone();
        changed[1]["system_prompt"] = json!("changed-private-prompt");
        assert_ne!(
            capture_context(&changed, &record, valid_global())
                .unwrap()
                .digest,
            context.digest
        );
        let mut global = valid_global();
        global
            .env_vars
            .insert("PRIVATE_GLOBAL".into(), "changed-private-env".into());
        assert_ne!(
            capture_context(&rows, &record, global).unwrap().digest,
            context.digest
        );
        assert_eq!(rows, preserved);
        assert!(!context.digest.contains("private"));
    }

    #[test]
    fn actual_context_overlay_is_fixed_target_only_and_preserves_original_authority() {
        let fixture = super::super::admission_test_support::app_with_keyless_agent();
        let rows = linked_fixture_rows(
            super::super::load_managed_agents(fixture.app.handle())
                .unwrap()
                .remove(0),
        );
        let record = prospective_record(&rows);
        for field in ["model", "provider"] {
            let mut wrong = valid_global();
            if field == "model" {
                wrong.model = Some("private-wrong-model".into());
            } else {
                wrong.provider = Some("private-wrong-provider".into());
            }
            assert!(capture_context(&rows, &record, wrong).is_ok());
            let mut changed = rows.clone();
            changed[1][field] = json!("private-wrong-definition-value");
            assert_ne!(
                capture_context(&changed, &record, valid_global())
                    .unwrap()
                    .digest,
                capture_context(&rows, &record, valid_global())
                    .unwrap()
                    .digest
            );
        }
        let mut foreign = record.clone();
        foreign.pubkey = "other-target".into();
        assert_eq!(
            capture_context(&rows, &foreign, valid_global())
                .err()
                .as_deref(),
            Some("target_contract_mismatch")
        );
        let mut missing_harness = record.clone();
        missing_harness.runtime = None;
        assert_eq!(
            capture_context(&rows, &missing_harness, valid_global())
                .err()
                .as_deref(),
            Some("target_contract_mismatch")
        );
        let mut wrong_command = record.clone();
        wrong_command.agent_command_override = Some("old-definition-command".into());
        assert_eq!(
            capture_context(&rows, &wrong_command, valid_global())
                .err()
                .as_deref(),
            Some("target_effective_config_mismatch")
        );
        let mut old_runtime = rows.clone();
        old_runtime[1]["runtime"] = json!("missing-old-definition-runtime");
        old_runtime[1]["acp_command"] = json!("old-definition-command");
        let context = capture_context(&old_runtime, &record, valid_global()).unwrap();
        let descriptor = super::super::resolve_effective_harness_descriptor(
            &record,
            &context.personas,
            &context.global,
        )
        .unwrap();
        assert_eq!(descriptor.command, core::PYTHON);
        assert_eq!(
            descriptor.args,
            vec!["-IS".to_owned(), format!("{}/run", core::CAPSULE)]
        );
        assert_eq!(
            capture_context(&rows[..1], &record, valid_global())
                .err()
                .as_deref(),
            Some("target_definition_missing")
        );
        let mut duplicate = rows.clone();
        duplicate.push(rows[1].clone());
        assert_eq!(
            capture_context(&duplicate, &record, valid_global())
                .err()
                .as_deref(),
            Some("target_definition_ambiguous")
        );
    }

    #[test]
    fn actual_global_accessor_allows_missing_and_refuses_invalid_unreadable_or_drift() {
        let fixture = super::super::admission_test_support::app_with_keyless_agent();
        let app = fixture.app.handle();
        let rows = linked_fixture_rows(super::super::load_managed_agents(app).unwrap().remove(0));
        let record = prospective_record(&rows);
        let path = app
            .path()
            .app_data_dir()
            .unwrap()
            .join("agents/global-agent-config.json");
        assert!(!path.exists());
        assert!(load_context(app, &rows, &record).is_ok());
        fs::write(&path, serde_json::to_vec(&valid_global()).unwrap()).unwrap();
        let context = load_context(app, &rows, &record).unwrap();
        let current = recheck_context(app, &rows, &record, &context.digest).unwrap();
        assert_eq!(current.global, context.global);
        assert_eq!(
            serde_json::to_value(current.personas).unwrap(),
            serde_json::to_value(context.personas).unwrap()
        );
        let mut drift = valid_global();
        drift
            .env_vars
            .insert("PRIVATE_GLOBAL".into(), "changed-private-global".into());
        fs::write(&path, serde_json::to_vec(&drift).unwrap()).unwrap();
        assert_eq!(
            recheck_context(app, &rows, &record, &context.digest)
                .err()
                .as_deref(),
            Some("fingerprint_mismatch")
        );
        fs::write(&path, "private-invalid-global-json").unwrap();
        assert_eq!(
            load_context(app, &rows, &record).err().as_deref(),
            Some("target_context_unavailable")
        );
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert_eq!(
            load_context(app, &rows, &record).err().as_deref(),
            Some("target_context_unavailable")
        );
        assert!(app
            .state::<AppState>()
            .managed_agent_processes
            .lock()
            .unwrap()
            .is_empty());
    }

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
                context_digest: "resolved-fixture-context",
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
