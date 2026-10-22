//! Fixed private owner-host composition. No UI/CLI/environment authority hooks.
//!
//! Deployment fills the literal source closure and workspace roots below only
//! after qualification. Missing pins/roots fail before Native Save. The concrete
//! supplier, pipe transport, resumed review/register and CAS path are present.
use super::super::super::{rowvia_saved_agent as native, rowvia_saved_agent_owner as owner};
use super::super::{NativeC2Boundary, ResumedNativeC2Boundary};
use super::*;
use crate::managed_agents::SavedAgentPreparationAck;
use serde_json::Value;
use std::{
    fs::OpenOptions,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};
use tauri::{AppHandle, Manager};

const RECIPE: &str = "codex-development-memory-git-read-v1";
const NODE: Pin = Pin {
    path: "/nix/store/2gd4h12irmnanhg33md2rpjygrsj5ypb-nodejs-slim-22.23.2/bin/node",
    sha256: "1c986b3913f4f67a25d7085750654fca847a28bf015913636294614b4443ff4f",
};
// Root deployment inputs, never ambient or caller supplied. A source pin table
// includes EVERY reached module/helper and the fixed shared-group successors.
// The closure seal commits the sorted `path SP sha256 LF` table after independent
// review of all reached spawn calls. An empty/unqualified table is unavailable.
#[path = "rowvia_saved_agent_owner_host_pins.rs"]
mod pins;
use pins::{SHARED_GROUP_CLOSURE_SHA256, SOURCE_PINS};

const ENTRYPOINT: &str = "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source/owner-entrypoint.mjs";
const WORKING_DIRECTORY: &str = "/var/tmp/rowvia-owner-host-adapter-v3.dpSKNt/source";
const ARTIFACT_ROOT: &str = "/var/tmp/rowvia-management-pilot-v75-1000";

pub(in crate::commands::agents) struct Pin {
    path: &'static str,
    sha256: &'static str,
}

fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && value.bytes().any(|b| b != b'0')
}
fn exact_path(value: &str) -> bool {
    let path = Path::new(value);
    path.is_absolute()
        && value.len() <= 4096
        && !value.chars().any(char::is_control)
        && !path.components().any(|p| {
            matches!(
                p,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
        && path.components().collect::<PathBuf>().to_str() == Some(value)
        && !path.starts_with("/proc")
        && !path.starts_with("/dev/fd")
}
pub(in crate::commands::agents) fn closure_qualified() -> bool {
    qualification_valid(
        SOURCE_PINS,
        SHARED_GROUP_CLOSURE_SHA256,
        ENTRYPOINT,
        WORKING_DIRECTORY,
        ARTIFACT_ROOT,
    )
}
pub(in crate::commands::agents) fn qualification_valid(
    pins: &[Pin],
    seal: &str,
    entrypoint: &str,
    working_directory: &str,
    artifact_root: &str,
) -> bool {
    if pins.is_empty()
        || !hash(seal)
        || ![entrypoint, working_directory, artifact_root]
            .into_iter()
            .all(exact_path)
        || !pins.iter().any(|pin| pin.path == entrypoint)
    {
        return false;
    }
    let mut previous = "";
    let mut commitment = Sha256::new();
    for pin in pins {
        if !exact_path(pin.path) || !hash(pin.sha256) || pin.path <= previous {
            return false;
        }
        previous = pin.path;
        commitment.update(pin.path.as_bytes());
        commitment.update(b" ");
        commitment.update(pin.sha256.as_bytes());
        commitment.update(b"\n");
    }
    hex::encode(commitment.finalize()) == seal
}
const MAX_PUBLIC_PIN_BYTES: u64 = 512 * 1024 * 1024;

fn checked_public_pin_size(offset: u64, additional: u64) -> Result<u64, ()> {
    offset
        .checked_add(additional)
        .filter(|size| *size <= MAX_PUBLIC_PIN_BYTES)
        .ok_or(())
}

fn held_digest(file: &File) -> Result<String, ()> {
    use std::os::unix::fs::FileExt;
    let mut digest = Sha256::new();
    let mut offset = 0u64;
    loop {
        let mut bytes = [0u8; 16384];
        let count = file.read_at(&mut bytes, offset).map_err(|_| ())?;
        if count == 0 {
            return Ok(hex::encode(digest.finalize()));
        }
        offset = checked_public_pin_size(offset, count as u64)?;
        digest.update(&bytes[..count]);
    }
}
fn held_pin(pin: &Pin, executable: bool) -> Result<File, ()> {
    if !exact_path(pin.path) || !hash(pin.sha256) {
        return Err(());
    }
    // Canonical named source only. No symlink or FD aliases can choose code.
    if std::fs::canonicalize(pin.path).map_err(|_| ())? != Path::new(pin.path) {
        return Err(());
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(pin.path)
        .map_err(|_| ())?;
    let stat = file.metadata().map_err(|_| ())?;
    let uid = rustix::process::geteuid().as_raw();
    if !stat.is_file()
        || checked_public_pin_size(0, stat.len()).is_err()
        || ![0, uid].contains(&stat.uid())
        || stat.mode() & 0o022 != 0
        || (executable && stat.mode() & 0o111 == 0)
        || held_digest(&file)? != pin.sha256
    {
        return Err(());
    }
    Ok(file)
}
fn unchanged(file: &File, pin: &Pin, executable: bool) -> bool {
    let check = || -> Result<(), ()> {
        let current = held_pin(pin, executable)?;
        let a = file.metadata().map_err(|_| ())?;
        let b = current.metadata().map_err(|_| ())?;
        if (
            a.dev(),
            a.ino(),
            a.len(),
            a.mtime(),
            a.mtime_nsec(),
            a.ctime(),
            a.ctime_nsec(),
        ) != (
            b.dev(),
            b.ino(),
            b.len(),
            b.mtime(),
            b.mtime_nsec(),
            b.ctime(),
            b.ctime_nsec(),
        ) || held_digest(file)? != pin.sha256
        {
            return Err(());
        }
        Ok(())
    };
    check().is_ok()
}
fn installation() -> Result<FixedInstallation, ()> {
    if !closure_qualified() {
        return Err(());
    }
    let node = held_pin(&NODE, true)?;
    let sources = SOURCE_PINS
        .iter()
        .map(|pin| held_pin(pin, false))
        .collect::<Result<Vec<_>, _>>()?;
    if !std::fs::metadata(WORKING_DIRECTORY)
        .map_err(|_| ())?
        .is_dir()
    {
        return Err(());
    }
    Ok(FixedInstallation {
        node,
        entrypoint: ENTRYPOINT.into(),
        working_directory: WORKING_DIRECTORY.into(),
        _held_source_custody: sources,
    })
}
fn revalidate(installation: &FixedInstallation) -> bool {
    closure_qualified()
        && installation.entrypoint == Path::new(ENTRYPOINT)
        && installation.working_directory == Path::new(WORKING_DIRECTORY)
        && installation._held_source_custody.len() == SOURCE_PINS.len()
        && unchanged(&installation.node, &NODE, true)
        && installation
            ._held_source_custody
            .iter()
            .zip(SOURCE_PINS)
            .all(|(file, pin)| unchanged(file, pin, false))
}
fn frame(value: &impl Serialize) -> Result<Vec<u8>, ()> {
    let mut frame = serde_json::to_vec(value).map_err(|_| ())?;
    frame.push(b'\n');
    checked_frame(&frame)?;
    Ok(frame)
}
fn map_only<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    struct MapOnly<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for MapOnly<T> {
        type Value = T;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("closed JSON object")
        }
        fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<T, M::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(MapOnly(std::marker::PhantomData))
}
fn decode_frame<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, ()> {
    let mut decoder = serde_json::Deserializer::from_slice(checked_frame(bytes)?);
    let value = map_only(&mut decoder).map_err(|_| ())?;
    decoder.end().map_err(|_| ())?;
    Ok(value)
}
fn current_scope<R: tauri::Runtime>(
    app: &AppHandle<R>,
) -> Result<(String, String, &'static str), ()> {
    let source = option_env!("BUZZ_DESKTOP_BUILD_ROWVIA_SOURCE_INSTANCE").ok_or(())?;
    let configured_owner = option_env!("BUZZ_DESKTOP_BUILD_ROWVIA_OWNER_PUBKEY").ok_or(())?;
    let configured_cerberus = option_env!("BUZZ_DESKTOP_BUILD_CERBERUS_PUBKEY").ok_or(())?;
    let management_origin = option_env!("BUZZ_DESKTOP_BUILD_ROWVIA_MANAGEMENT_ORIGIN").ok_or(())?;
    let origin = reqwest::Url::parse(management_origin).map_err(|_| ())?;
    if !hash(configured_owner)
        || !hash(configured_cerberus)
        || configured_owner == configured_cerberus
        || origin.scheme() != "https"
        || origin.host_str().is_none()
        || !origin.username().is_empty()
        || origin.password().is_some()
        || origin.path() != "/"
        || origin.query().is_some()
        || origin.fragment().is_some()
    {
        return Err(());
    }
    if source.is_empty()
        || source.len() > 64
        || !source
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
        || !source.as_bytes()[0].is_ascii_alphanumeric()
    {
        return Err(());
    }
    let state = app.state::<crate::app_state::AppState>();
    let _workspace = state.workspace_apply_lock.try_lock().map_err(|_| ())?;
    let _identity = state.identity_mutation.lock().map_err(|_| ())?;
    if state
        .reset_failed
        .load(std::sync::atomic::Ordering::Acquire)
        || state
            .shutdown_started
            .load(std::sync::atomic::Ordering::Acquire)
    {
        return Err(());
    }
    let owner = state.signing_keys().map_err(|_| ())?.public_key().to_hex();
    if owner != configured_owner {
        return Err(());
    }
    let relay = crate::relay::relay_ws_url_with_override(&state);
    let normalized = buzz_core_pkg::relay::normalize_relay_url(&relay).map_err(|_| ())?;
    if relay != normalized {
        return Err(());
    }
    Ok((owner, relay, source))
}
fn expected(
    original: &RowviaSavedAgentPreparation,
    prepared: &PreparedRowviaSavedAgent,
) -> Result<native::RowviaSavedAgentScopeExpectation, ()> {
    let holder = NativeRegistrationCallbacks::from_prepared(original, prepared).map_err(|_| ())?;
    Ok(holder.expected)
}
fn operation<R: tauri::Runtime>(
    app: &AppHandle<R>,
    original: &RowviaSavedAgentPreparation,
    prepared: &PreparedRowviaSavedAgent,
) -> Result<native::RowviaSavedAgentOperationScope, ()> {
    Ok(
        native::capture_rowvia_saved_agent_scope(app, &expected(original, prepared)?)
            .map_err(|_| ())?
            .operation,
    )
}

struct SaveContext<'a, R: tauri::Runtime> {
    app: &'a AppHandle<R>,
    boundary: NativeC2Boundary<'a, R>,
    family: Option<SettledProcessFamily>,
}
impl<'a, R: tauri::Runtime> WholeContext for SaveContext<'a, R> {
    type Boundary = NativeC2Boundary<'a, R>;
    fn preflight(&mut self, request: &OwnerSaveRequest) -> Result<ValidatedInvocation, ()> {
        if request.recipe != SavedAgentRecipe::CodexDevelopmentMemoryGitReadV1 {
            return Err(());
        }
        crate::managed_agents::validate_agent_definition_text(
            &request.display_name,
            &request.system_prompt,
        )
        .map_err(|_| ())?;
        let channel = uuid::Uuid::parse_str(&request.channel_id).map_err(|_| ())?;
        if channel.is_nil()
            || channel.get_variant() != uuid::Variant::RFC4122
            || channel.to_string() != request.channel_id
        {
            return Err(());
        }
        let fixed = installation()?;
        let (owner, relay, source) = current_scope(self.app)?;
        let startup = frame(
            &serde_json::json!({ "version": 1, "kind": "preflight", "input": {
            "displayName": request.display_name, "systemPrompt": request.system_prompt,
            "channelId": request.channel_id, "recipe": request.recipe } }),
        )?;
        let deadline = Instant::now() + CHILD_EXCHANGE_LIMIT;
        let mut child = OwnedNode::spawn(&fixed)?;
        let result = child.exchange_inner(&startup, deadline, None, &mut self.boundary);
        let settled = child.settle();
        let family = child.take_family_settlement().ok_or(())?;
        if !settled || family.leader != child.child.id() || !revalidate(&fixed) {
            return Err(());
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Preflight {
            version: u8,
            kind: String,
            supported: bool,
            recipe: String,
        }
        let result: Preflight = decode_frame(&result?)?;
        if result.version != 1
            || result.kind != "preflight"
            || !result.supported
            || result.recipe != RECIPE
        {
            return Err(());
        }
        let id = uuid::Uuid::now_v7().to_string();
        Ok(ValidatedInvocation {
            original: RowviaSavedAgentPreparation {
                operation_id: id.clone(),
                owner_pubkey: owner,
                relay_url: relay,
                source_instance: source.into(),
                channel_id: request.channel_id.clone(),
                name: request.display_name.clone(),
                system_prompt: request.system_prompt.clone(),
                recipe: request.recipe,
                model: "gpt-6.1-sol".into(),
                workspace: Path::new(ARTIFACT_ROOT)
                    .join("workspaces")
                    .join(&id)
                    .to_str()
                    .ok_or(())?
                    .into(),
                capsule_run: Path::new(ARTIFACT_ROOT)
                    .join("operations")
                    .join(&id)
                    .join("capsule/buzz-worker/run")
                    .to_str()
                    .ok_or(())?
                    .into(),
            },
            installation: fixed,
        })
    }
    fn save(
        &mut self,
        original: RowviaSavedAgentPreparation,
    ) -> Result<PreparedRowviaSavedAgent, ()> {
        native::prepare_rowvia_saved_agent(self.app, original).map_err(|_| ())
    }
    fn startup(
        &mut self,
        original: &RowviaSavedAgentPreparation,
        prepared: &PreparedRowviaSavedAgent,
    ) -> Result<Vec<u8>, ()> {
        frame(
            &serde_json::json!({ "version": 1, "kind": "prepare", "operation": operation(self.app, original, prepared)? }),
        )
    }
    fn boundary(&mut self) -> &mut Self::Boundary {
        &mut self.boundary
    }
    fn record_family_settlement(&mut self, settled: SettledProcessFamily) {
        self.family = Some(settled);
    }
    fn owned_descendants_settled(&mut self, leader: u32) -> bool {
        self.family
            .take()
            .is_some_and(|proof| proof.leader == leader)
    }
    fn revalidate_installation(&mut self, fixed: &FixedInstallation) -> bool {
        revalidate(fixed)
    }
}

pub(crate) fn save<R: tauri::Runtime>(
    app: &AppHandle<R>,
    request: &OwnerSaveRequest,
) -> Result<PreparedProposal, PrepareFailure> {
    run_with(
        &mut SaveContext {
            app,
            boundary: NativeC2Boundary(app),
            family: None,
        },
        request,
    )
}

pub(in crate::commands::agents) struct ProductionOwnerBoundary<'a, R: tauri::Runtime> {
    app: &'a AppHandle<R>,
    fixed: FixedInstallation,
}
impl<'a, R: tauri::Runtime> ProductionOwnerBoundary<'a, R> {
    pub(in crate::commands::agents) fn new(app: &'a AppHandle<R>) -> Result<Self, owner::Failure> {
        current_scope(app).map_err(|_| owner::Failure::Unavailable)?;
        Ok(Self {
            app,
            fixed: installation().map_err(|_| owner::Failure::Unavailable)?,
        })
    }
    fn invoke(
        &mut self,
        resumed: &owner::Resumed,
        kind: &str,
        authorization: Option<&owner::SavedAgentAuthorization>,
    ) -> Result<(Vec<u8>, SettledProcessFamily), ()> {
        let qualification_deadline = Instant::now() + QUALIFICATION_LIMIT;
        if !revalidate(&self.fixed) {
            return Err(());
        }
        owner::revalidate_native(self.app, resumed).map_err(|_| ())?;
        let s = &resumed.expected.selection;
        let op = operation(self.app, s, &resumed.prepared)?;
        let mut startup = serde_json::json!({ "version": 1, "kind": kind, "operation": op });
        if let Some(authorization) = authorization {
            startup["authorization"] = serde_json::json!({ "version": 1, "operationId": s.operation_id,
                "agentPubkey": resumed.expected.agent_pubkey, "ownerPubkey": s.owner_pubkey,
                "sourceInstance": s.source_instance, "canonical21Sha256": resumed.ack.canonical21_sha256,
                "result": authorization });
        }
        let startup = frame(&startup)?;
        remaining(qualification_deadline)?;
        if authorization.is_some_and(|value| value.expires_at <= chrono::Utc::now()) {
            return Err(());
        }
        let deadline = Instant::now() + INVOCATION_LIMIT;
        let mut holder =
            NativeRegistrationCallbacks::from_resumed(s, &resumed.prepared, &resumed.ack)
                .map_err(|_| ())?;
        let mut boundary = ResumedNativeC2Boundary(self.app);
        let mut child = OwnedNode::spawn(&self.fixed)?;
        let leader = child.child.id();
        let result = child.exchange(&startup, deadline, &mut holder, &mut boundary);
        let local = child.settle();
        let family = child.take_family_settlement().ok_or(())?;
        if !local || family.leader != leader || !revalidate(&self.fixed) {
            holder.delivery_failed();
            return Err(());
        }
        // Both settlement domains precede the final C2; no callback replay or
        // substitute ACK is permitted in a resumed continuation.
        let result = result?;
        remaining(deadline)?;
        holder
            .finalize_prepared_with(
                &s.operation_id,
                &resumed.expected.agent_pubkey,
                &resumed.ack.recipe_sha256,
                &resumed.ack.canonical21_sha256,
                &mut boundary,
            )
            .map_err(|_| ())?;
        remaining(deadline)?;
        Ok((result, family))
    }
    pub(in crate::commands::agents) fn register(
        &mut self,
        resumed: &owner::Resumed,
        authorization: &owner::SavedAgentAuthorization,
    ) -> Result<native::VerifiedRowviaSavedAgentRegistrationCompletion, owner::Failure> {
        if authorization.state != "authorized"
            || authorization.operation_id != resumed.expected.selection.operation_id
            || authorization.expires_at <= chrono::Utc::now()
        {
            return Err(owner::Failure::Unknown);
        }
        let preparation = native::resume_rowvia_saved_agent_preparation(
            self.app,
            &resumed.expected.selection.operation_id,
        )
        .map_err(|_| owner::Failure::Unknown)?;
        if !preparation.matches_cohort(&resumed.expected, &resumed.prepared, &resumed.ack) {
            return Err(owner::Failure::Unknown);
        }
        // The owner POST already crossed the effect boundary. Expiry, resume,
        // cohort, child, read-only and CAS failures all remain unknown.
        let (bytes, family) = self
            .invoke(resumed, "register", Some(authorization))
            .map_err(|_| owner::Failure::Unknown)?;
        let proof = RegisteredHostProof::checked(bytes, family, resumed)
            .map_err(|_| owner::Failure::Unknown)?;
        let qualification_deadline = Instant::now() + QUALIFICATION_LIMIT;
        if !revalidate(&self.fixed) || authorization.expires_at <= chrono::Utc::now() {
            return Err(owner::Failure::Unknown);
        }
        owner::revalidate_native(self.app, resumed).map_err(|_| owner::Failure::Unknown)?;
        remaining(qualification_deadline).map_err(|_| owner::Failure::Unknown)?;
        if authorization.expires_at <= chrono::Utc::now() {
            return Err(owner::Failure::Unknown);
        }
        native::registration_completion_from_host(preparation, proof)
            .map_err(|_| owner::Failure::Unknown)
    }
}
impl<R: tauri::Runtime> owner::OwnerBoundary for ProductionOwnerBoundary<'_, R> {
    fn preflight(&mut self) -> Result<(), owner::Failure> {
        current_scope(self.app).map_err(|_| owner::Failure::Unavailable)?;
        if revalidate(&self.fixed) {
            Ok(())
        } else {
            Err(owner::Failure::Unavailable)
        }
    }
    fn resume(&mut self, id: &str) -> Result<owner::Resumed, owner::Failure> {
        owner::resume_native(self.app, id)
    }
    async fn host_review(
        &mut self,
        resumed: &owner::Resumed,
    ) -> Result<owner::HostReviewedPreparation, owner::Failure> {
        let (bytes, _family) = self
            .invoke(resumed, "review", None)
            .map_err(|_| owner::Failure::Unavailable)?;
        decode_frame(&bytes).map_err(|_| owner::Failure::Unavailable)
    }
    fn revalidate(&mut self, resumed: &owner::Resumed) -> Result<(), owner::Failure> {
        owner::revalidate_native(self.app, resumed)
    }
    async fn authorize(
        &mut self,
        resumed: &owner::Resumed,
        body: Vec<u8>,
    ) -> Result<Vec<u8>, owner::Failure> {
        owner::authorize_native(self.app, resumed, body).await
    }
    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HostRegistration {
    version: u8,
    operation_id: String,
    agent_pubkey: String,
    canonical21_sha256: String,
    paseo_agent_id: String,
    bindings_ready: bool,
    runtime_prepared: bool,
    runtime_ready: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InspectionEvidence {
    version: u8,
    operation_id: String,
    agent_pubkey: String,
    canonical21_sha256: String,
    recipe_sha256: String,
    request_sha256: String,
    ack_sha256: String,
    binding_identity: String,
    paseo_agent_id: String,
    receipt_sha256: String,
    core_graph_sha256: String,
    profile_sha256: String,
    pins_sha256: String,
    memory_capsule_sha256: String,
    memory_launcher_sha256: String,
    worker_inspection_sha256: String,
    transcript_sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HostRegistered {
    #[serde(deserialize_with = "map_only")]
    registration: HostRegistration,
    #[serde(deserialize_with = "map_only")]
    evidence: InspectionEvidence,
}

fn canonical(value: &Value) -> Result<Vec<u8>, ()> {
    fn encode(value: &Value, out: &mut Vec<u8>) -> Result<(), ()> {
        match value {
            Value::Object(map) => {
                let mut keys = map.keys().collect::<Vec<_>>();
                keys.sort();
                out.push(b'{');
                for (index, key) in keys.into_iter().enumerate() {
                    if index != 0 {
                        out.push(b',');
                    }
                    out.extend(serde_json::to_vec(key).map_err(|_| ())?);
                    out.push(b':');
                    encode(&map[key], out)?;
                }
                out.push(b'}');
            }
            Value::Array(items) => {
                out.push(b'[');
                for (index, item) in items.iter().enumerate() {
                    if index != 0 {
                        out.push(b',');
                    }
                    encode(item, out)?;
                }
                out.push(b']');
            }
            Value::Number(number) if !number.is_i64() && !number.is_u64() => return Err(()),
            _ => out.extend(serde_json::to_vec(value).map_err(|_| ())?),
        }
        Ok(())
    }
    let mut out = Vec::new();
    encode(value, &mut out)?;
    Ok(out)
}
fn digest_json(value: &impl Serialize) -> Result<String, ()> {
    Ok(hex::encode(Sha256::digest(canonical(
        &serde_json::to_value(value).map_err(|_| ())?,
    )?)))
}
/// Not Deserialize/Clone. Construction requires the actual closed FD4 result,
/// Native's private family token, original B1 cohort and final-C2 invoke above.
pub(crate) struct RegisteredHostProof {
    result: HostRegistration,
    ack: SavedAgentPreparationAck,
    family: SettledProcessFamily,
    closure: &'static str,
}
type NativeRegistrationResult = (u8, String, String, String, String, bool, bool, bool);

impl RegisteredHostProof {
    fn checked(
        bytes: Vec<u8>,
        family: SettledProcessFamily,
        resumed: &owner::Resumed,
    ) -> Result<Self, ()> {
        let value: HostRegistered = decode_frame(&bytes)?;
        let r = &value.registration;
        let e = &value.evidence;
        let a = &resumed.ack;
        let id = uuid::Uuid::parse_str(&r.paseo_agent_id).map_err(|_| ())?;
        let mut transcript = serde_json::to_value(e).map_err(|_| ())?;
        transcript
            .as_object_mut()
            .ok_or(())?
            .remove("transcript_sha256");
        if family.leader == 0
            || r.version != 1
            || e.version != 1
            || !r.bindings_ready
            || !r.runtime_prepared
            || r.runtime_ready
            || id.is_nil()
            || id.get_variant() != uuid::Variant::RFC4122
            || id.to_string() != r.paseo_agent_id
            || r.operation_id != a.operation_id
            || r.agent_pubkey != a.agent_pubkey
            || r.canonical21_sha256 != a.canonical21_sha256
            || e.operation_id != r.operation_id
            || e.agent_pubkey != r.agent_pubkey
            || e.canonical21_sha256 != r.canonical21_sha256
            || e.recipe_sha256 != a.recipe_sha256
            || e.request_sha256 != a.request_sha256
            || e.ack_sha256 != digest_json(a)?
            || e.paseo_agent_id != r.paseo_agent_id
            || e.binding_identity.is_empty()
            || e.binding_identity.len() > 256
            || [
                &e.receipt_sha256,
                &e.core_graph_sha256,
                &e.profile_sha256,
                &e.pins_sha256,
                &e.memory_capsule_sha256,
                &e.memory_launcher_sha256,
                &e.worker_inspection_sha256,
                &e.transcript_sha256,
            ]
            .iter()
            .any(|v| !hash(v))
            || hex::encode(Sha256::digest(canonical(&transcript)?)) != e.transcript_sha256
        {
            return Err(());
        }
        Ok(Self {
            result: value.registration,
            ack: a.clone(),
            family,
            closure: SHARED_GROUP_CLOSURE_SHA256,
        })
    }
    pub(crate) fn into_native_result(
        self,
        expected: &native::RowviaSavedAgentScopeExpectation,
        ack: &SavedAgentPreparationAck,
    ) -> Result<NativeRegistrationResult, ()> {
        let r = self.result;
        if self.family.leader == 0
            || self.closure != SHARED_GROUP_CLOSURE_SHA256
            || !closure_qualified()
            || self.ack != *ack
            || r.operation_id != expected.selection.operation_id
            || r.agent_pubkey != expected.agent_pubkey
            || r.canonical21_sha256 != ack.canonical21_sha256
        {
            return Err(());
        }
        Ok((
            r.version,
            r.operation_id,
            r.agent_pubkey,
            r.canonical21_sha256,
            r.paseo_agent_id,
            r.bindings_ready,
            r.runtime_prepared,
            r.runtime_ready,
        ))
    }
}

#[cfg(test)]
#[path = "rowvia_saved_agent_owner_host_tests.rs"]
mod tests;
