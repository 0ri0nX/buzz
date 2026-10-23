use super::super::super::rowvia_saved_agent::{
    RowviaSavedAgentNativeScope, RowviaSavedAgentOperationScope,
};
use super::super::*;
use super::*;
use serde_json::{json, Value};
use std::{ffi::OsStr, os::unix::fs::PermissionsExt};

#[path = "rowvia_saved_agent_prepare_deadline_tests.rs"]
mod deadline_tests;

fn node_from_command(command: &OsStr) -> io::Result<File> {
    // PATH may select a Hermit launcher. Hold Node itself for the fixture's
    // environment-cleared FD spawn, rather than holding that launcher.
    let output = std::process::Command::new(command)
        .args(["--print", "process.execPath"])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other("Node test executable lookup failed"));
    }
    let executable = String::from_utf8(output.stdout)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let path = PathBuf::from(executable.trim_end_matches(['\r', '\n']));
    if !path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Node test executable path must be absolute",
        ));
    }
    File::open(path.canonicalize()?)
}

pub(super) fn node_for_test() -> io::Result<File> {
    node_from_command(OsStr::new("node"))
}

#[test]
fn native_prepare_test_node_lookup_rejects_missing_executable_and_resolves_launcher() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing");
    assert_eq!(
        node_from_command(missing.as_os_str()).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
    let launcher = root.path().join("node-launcher");
    std::fs::write(&launcher, b"#!/bin/sh\nexec node \"$@\"\n").unwrap();
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
    let direct = node_for_test().unwrap();
    let held = node_from_command(launcher.as_os_str()).unwrap();
    let resolved = std::fs::read_link(format!("/proc/self/fd/{}", held.as_raw_fd())).unwrap();
    assert_eq!(
        resolved,
        std::fs::read_link(format!("/proc/self/fd/{}", direct.as_raw_fd())).unwrap()
    );
    assert_ne!(resolved, launcher.canonicalize().unwrap());
    std::fs::write(&launcher, b"#!/bin/sh\nexit 1\n").unwrap();
    assert!(node_from_command(launcher.as_os_str()).is_err());
}

fn original() -> RowviaSavedAgentPreparation {
    RowviaSavedAgentPreparation {
        operation_id: "019a5432-1234-7abc-8def-0123456789ab".into(),
        owner_pubkey: "a".repeat(64),
        relay_url: "wss://relay.invalid".into(),
        source_instance: "synthetic-native".into(),
        channel_id: "019a5432-1234-4abc-8def-0123456789ab".into(),
        name: "Owner reviewed display name".into(),
        system_prompt: "Synthetic instructions".into(),
        recipe: SavedAgentRecipe::CodexLocalV1,
        model: "synthetic-model".into(),
        workspace: "/var/tmp/synthetic/019a5432-1234-7abc-8def-0123456789ab".into(),
        capsule_run: "/var/tmp/synthetic/019a5432-1234-7abc-8def-0123456789ab/run".into(),
    }
}

fn prepared() -> PreparedRowviaSavedAgent {
    let input = original();
    PreparedRowviaSavedAgent {
        operation_id: input.operation_id.clone(),
        pubkey: "b".repeat(64),
        core_agent_name: format!("saved-{}", input.operation_id.replace('-', "")),
        saved_authorization_sha256: "c".repeat(64),
        native_config_sha256: "d".repeat(64),
        created_stopped: true,
        bindings_ready: false,
        runtime_ready: false,
    }
}

fn new_holder() -> NativeRegistrationCallbacks {
    NativeRegistrationCallbacks::from_prepared(&original(), &prepared()).unwrap()
}

fn ack() -> SavedAgentPreparationAck {
    SavedAgentPreparationAck {
        version: 1,
        operation_id: original().operation_id,
        agent_pubkey: prepared().pubkey,
        native_config_sha256: prepared().native_config_sha256,
        saved_authorization_sha256: prepared().saved_authorization_sha256,
        recipe_sha256: "1".repeat(64),
        request_sha256: "2".repeat(64),
        canonical21_sha256: "3".repeat(64),
        prepared_receipt_sha256: "4".repeat(64),
    }
}

fn frame(id: u16, kind: &str, ack: Option<&SavedAgentPreparationAck>) -> Vec<u8> {
    let mut value = json!({ "version": 1, "id": id, "kind": kind });
    if let Some(ack) = ack {
        value["ack"] = json!(ack);
    }
    let mut bytes = serde_json::to_vec(&value).unwrap();
    bytes.push(b'\n');
    bytes
}

#[derive(Default)]
struct FakeC2 {
    calls: Vec<(&'static str, RowviaSavedAgentScopeExpectation)>,
    fail: Option<&'static str>,
    wrong_confirmation: bool,
    extra_prompt: usize,
}

impl C2Boundary for FakeC2 {
    fn capture(
        &mut self,
        expected: &RowviaSavedAgentScopeExpectation,
    ) -> Result<RowviaSavedAgentScope, ()> {
        self.calls.push(("capture", expected.clone()));
        if self.fail == Some("capture") {
            return Err(());
        }
        let input = &expected.selection;
        Ok(RowviaSavedAgentScope {
            version: 1,
            owner_pubkey: input.owner_pubkey.clone(),
            relay_origin: input.relay_url.clone(),
            source_instance: input.source_instance.clone(),
            stopped: true,
            pending: true,
            operation: RowviaSavedAgentOperationScope {
                version: 1,
                operation_id: input.operation_id.clone(),
                agent_name: format!("saved-{}", input.operation_id.replace('-', "")),
                recipe_id: "codex-local-v1".into(),
                native: RowviaSavedAgentNativeScope {
                    agent_pubkey: expected.agent_pubkey.clone(),
                    saved_authorization_sha256: expected.saved_authorization_sha256.clone(),
                    native_config_sha256: expected.native_config_sha256.clone(),
                    channel_id: input.channel_id.clone(),
                    system_prompt: format!(
                        "{}{}",
                        input.system_prompt,
                        "x".repeat(self.extra_prompt)
                    ),
                    model: input.model.clone(),
                    workspace: input.workspace.clone(),
                    capsule_run: input.capsule_run.clone(),
                },
            },
        })
    }
    fn retain(
        &mut self,
        expected: &RowviaSavedAgentScopeExpectation,
        ack: &SavedAgentPreparationAck,
    ) -> Result<ConfirmedRowviaSavedAgentPreparationAck, ()> {
        self.calls.push(("retain", expected.clone()));
        if self.fail == Some("retain") {
            return Err(());
        }
        Ok(ConfirmedRowviaSavedAgentPreparationAck {
            ack: ack.clone(),
            confirmed: !self.wrong_confirmation,
        })
    }
    fn revalidate(
        &mut self,
        expected: &RowviaSavedAgentScopeExpectation,
        ack: &SavedAgentPreparationAck,
    ) -> Result<ConfirmedRowviaSavedAgentPreparationAck, ()> {
        self.calls.push(("revalidate", expected.clone()));
        if self.fail == Some("revalidate") {
            return Err(());
        }
        Ok(ConfirmedRowviaSavedAgentPreparationAck {
            ack: ack.clone(),
            confirmed: !self.wrong_confirmation,
        })
    }
}

struct FakeContext {
    mode: &'static str,
    available: bool,
    saves: usize,
    original_seen: Option<RowviaSavedAgentPreparation>,
    boundary: FakeC2,
    descendants_settled: bool,
    installation_unchanged: bool,
}

impl FakeContext {
    fn new(mode: &'static str) -> Self {
        Self {
            mode,
            available: true,
            saves: 0,
            original_seen: None,
            boundary: FakeC2::default(),
            descendants_settled: true,
            installation_unchanged: true,
        }
    }
}

impl WholeContext for FakeContext {
    type Boundary = FakeC2;
    fn preflight(&mut self, request: &OwnerSaveRequest) -> Result<ValidatedInvocation, ()> {
        if !self.available {
            return Err(());
        }
        let mut selection = original();
        // Complete inert context: trusted synthetic owner/relay/source/paths and
        // UUID come from this context, never fields supplied by the fake UI.
        selection.name = request.display_name.clone();
        selection.system_prompt = request.system_prompt.clone();
        if request.channel_id != selection.channel_id || request.recipe != selection.recipe {
            return Err(());
        }
        Ok(ValidatedInvocation {
            original: selection,
            installation: FixedInstallation {
                node: node_for_test().map_err(|_| ())?,
                entrypoint: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("src/commands/rowvia_saved_agent_prepare_fixture.mjs"),
                working_directory: PathBuf::from("/"),
                _held_source_custody: Vec::new(),
            },
        })
    }
    fn save(
        &mut self,
        original: RowviaSavedAgentPreparation,
    ) -> Result<PreparedRowviaSavedAgent, ()> {
        self.saves += 1;
        self.original_seen = Some(original);
        Ok(prepared())
    }
    fn startup(
        &mut self,
        original: &RowviaSavedAgentPreparation,
        prepared: &PreparedRowviaSavedAgent,
    ) -> Result<Vec<u8>, ()> {
        assert_eq!(original.name, "Owner reviewed display name");
        assert_ne!(original.name, prepared.core_agent_name);
        let mut evidence = ack();
        evidence.canonical21_sha256 =
            hex::encode(Sha256::digest(b"synthetic public canonical cohort"));
        let mut frame = serde_json::to_vec(&json!({ "version": 1, "host": { "mode": self.mode },
            "operation": { "operationId": original.operation_id }, "execution": { "ack": evidence } })).unwrap();
        frame.push(b'\n');
        Ok(frame)
    }
    fn boundary(&mut self) -> &mut FakeC2 {
        &mut self.boundary
    }
    fn owned_descendants_settled(&mut self, _leader: u32) -> bool {
        self.descendants_settled
    }
    fn revalidate_installation(&mut self, _installation: &FixedInstallation) -> bool {
        self.installation_unchanged
    }
}

fn request() -> OwnerSaveRequest {
    OwnerSaveRequest {
        display_name: original().name,
        system_prompt: String::new(),
        channel_id: original().channel_id,
        recipe: original().recipe,
    }
}

#[test]
fn native_prepare_unavailable_refuses_before_any_save_or_launch() {
    // Missing fixed closure rows refuse before a supplier can create a Native
    // context or launch. Synthetic whole-context tests remain separate.
    assert!(!production::qualification_valid(
        &[],
        "",
        "/fixture/owner-entrypoint.mjs",
        "/fixture",
        "/fixture/artifacts"
    ));
    let mut context = FakeContext::new("success");
    context.available = false;
    assert_eq!(
        run_with(&mut context, &request()),
        Err(PrepareFailure::Unavailable)
    );
    assert_eq!(context.saves, 0);
    assert!(context.boundary.calls.is_empty());
}

#[test]
fn native_prepare_real_owned_pipes_keep_original_and_final_readonly_gate() {
    let mut context = FakeContext::new("success");
    let result = run_with(&mut context, &request()).unwrap();
    assert!(result.preparation_retained);
    assert!(!result.bindings_ready);
    assert!(!result.runtime_ready);
    let original = context.original_seen.as_ref().unwrap();
    assert_eq!(original.name, "Owner reviewed display name");
    assert_eq!(original.system_prompt, "");
    assert_eq!(context.saves, 1);
    assert_eq!(
        context
            .boundary
            .calls
            .iter()
            .map(|call| call.0)
            .collect::<Vec<_>>(),
        ["capture", "retain", "revalidate", "revalidate"]
    );
    for (_, expected) in &context.boundary.calls {
        assert_eq!(expected.selection.name, original.name);
        assert_eq!(expected.selection.system_prompt, "");
    }
}

#[test]
fn native_prepare_missing_ack_changed_cohort_exit_and_delivery_never_retry() {
    for mode in [
        "no-ack",
        "changed-cohort",
        "exit-failure",
        "lost-response",
        "trailing-result",
        "duplicate-result",
        "sequence-result",
        "stderr-flood",
    ] {
        let mut context = FakeContext::new(mode);
        assert_eq!(
            run_with(&mut context, &request()),
            Err(PrepareFailure::Unknown),
            "{mode}"
        );
        assert_eq!(context.saves, 1, "{mode}");
        assert!(
            context
                .boundary
                .calls
                .iter()
                .filter(|call| call.0 == "retain")
                .count()
                <= 1,
            "{mode}"
        );
        assert!(
            context
                .boundary
                .calls
                .iter()
                .filter(|call| call.0 == "revalidate")
                .count()
                <= 1,
            "{mode}"
        );
    }
}

#[test]
fn native_prepare_unsettled_descendants_or_installation_drift_withhold_result() {
    for descendants in [false, true] {
        let mut context = FakeContext::new("success");
        context.descendants_settled = descendants;
        context.installation_unchanged = !descendants;
        assert_eq!(
            run_with(&mut context, &request()),
            Err(PrepareFailure::Unknown)
        );
        assert_eq!(
            context
                .boundary
                .calls
                .iter()
                .filter(|call| call.0 == "revalidate")
                .count(),
            1
        );
    }
}

#[test]
fn native_prepare_monotonic_deadline_kills_and_drains_owned_stall() {
    let mut context = FakeContext::new("stall");
    let invocation = context.preflight(&request()).unwrap();
    let mut holder =
        NativeRegistrationCallbacks::from_prepared(&invocation.original, &prepared()).unwrap();
    let startup = context.startup(&invocation.original, &prepared()).unwrap();
    let mut child = OwnedNode::spawn(&invocation.installation).unwrap();
    assert!(child
        .exchange(
            &startup,
            Instant::now() + Duration::from_millis(150),
            &mut holder,
            &mut context.boundary
        )
        .is_err());
    assert!(child.settle());
    assert!(child.child.try_wait().unwrap().is_some());
    assert_eq!(holder.finish_input(), Err(CallbackFailure::Protocol));
}

#[test]
fn native_prepare_holder_eof_alone_or_delivery_failure_cannot_finalize() {
    let evidence = ack();
    for finished in [false, true] {
        let mut holder =
            NativeRegistrationCallbacks::from_prepared(&original(), &prepared()).unwrap();
        if finished {
            holder.finish_input().unwrap();
        }
        assert!(holder
            .finalize_prepared_with(
                &evidence.operation_id,
                &evidence.agent_pubkey,
                &evidence.recipe_sha256,
                &evidence.canonical21_sha256,
                &mut FakeC2::default()
            )
            .is_err());
    }
    let mut holder = NativeRegistrationCallbacks::from_prepared(&original(), &prepared()).unwrap();
    let mut boundary = FakeC2::default();
    for (id, kind, value) in [(1, "capture", None), (2, "retain", Some(&evidence))] {
        assert!(holder
            .receive_with(&frame(id, kind, value), &mut boundary)
            .unwrap()
            .is_some());
        holder.response_flushed().unwrap();
    }
    holder.finish_input().unwrap();
    holder.delivery_failed();
    assert!(holder
        .finalize_prepared_with(
            &evidence.operation_id,
            &evidence.agent_pubkey,
            &evidence.recipe_sha256,
            &evidence.canonical21_sha256,
            &mut boundary
        )
        .is_err());
    assert_eq!(
        boundary
            .calls
            .iter()
            .filter(|call| call.0 == "revalidate")
            .count(),
        0
    );
    let mut holder = NativeRegistrationCallbacks::from_prepared(&original(), &prepared()).unwrap();
    let mut boundary = FakeC2::default();
    for (id, kind, value) in [(1, "capture", None), (2, "retain", Some(&evidence))] {
        holder
            .receive_with(&frame(id, kind, value), &mut boundary)
            .unwrap();
        holder.response_flushed().unwrap();
    }
    holder.finish_input().unwrap();
    assert!(holder
        .finalize_prepared_with(
            &evidence.operation_id,
            &evidence.agent_pubkey,
            &"f".repeat(64),
            &evidence.canonical21_sha256,
            &mut boundary
        )
        .is_err());
    // Correcting a rejected result is not a repair/retry path.
    assert!(holder
        .finalize_prepared_with(
            &evidence.operation_id,
            &evidence.agent_pubkey,
            &evidence.recipe_sha256,
            &evidence.canonical21_sha256,
            &mut boundary
        )
        .is_err());
    assert_eq!(
        boundary
            .calls
            .iter()
            .filter(|call| call.0 == "revalidate")
            .count(),
        0
    );
}

#[test]
fn native_prepare_result_parser_is_closed_and_utf8_bounded() {
    assert!(proposal(b"{}\n{}").is_err());
    assert!(proposal(b"{\"version\":1,\"version\":1}\n").is_err());
    assert!(proposal(&[0xff, b'\n']).is_err());
    assert!(proposal(&vec![b'x'; FRAME_LIMIT + 1]).is_err());
    let wire = json!({ "displayName": "x", "systemPrompt": "", "channelId": original().channel_id,
        "recipe": "codex-local-v1", "ownerPubkey": "a".repeat(64) });
    assert!(serde_json::from_value::<OwnerSaveRequest>(wire).is_err());
}

fn valid_result() -> PreparedProposal {
    PreparedProposal {
        version: 1,
        operation_id: original().operation_id,
        agent_pubkey: prepared().pubkey,
        recipe_sha256: ack().recipe_sha256,
        canonical21: "synthetic public canonical cohort".into(),
        canonical21_sha256: hex::encode(Sha256::digest(b"synthetic public canonical cohort")),
        preparation_retained: true,
        bindings_ready: false,
        runtime_ready: false,
    }
}

fn json_frame(value: &Value) -> Vec<u8> {
    let mut frame = serde_json::to_vec(value).unwrap();
    frame.push(b'\n');
    frame
}

#[test]
fn native_prepare_full_valid_sequences_duplicates_and_objects_are_distinguished() {
    let result = valid_result();
    let object = serde_json::to_value(&result).unwrap();
    assert_eq!(proposal(&json_frame(&object)).unwrap(), result);
    let mut evidence = ack();
    evidence.canonical21_sha256 = result.canonical21_sha256.clone();
    let holder = new_holder();
    evidence
        .validate_pending_cohort(&pending_projection(&holder.expected), &prepared().pubkey)
        .unwrap();
    assert_eq!(evidence.recipe_sha256, result.recipe_sha256);
    // All nine sequence values are otherwise valid, including the exact hash
    // and ACK cohort; only the forbidden top-level sequence differs.
    let sequence = json!([
        result.version,
        result.operation_id,
        result.agent_pubkey,
        result.recipe_sha256,
        result.canonical21,
        result.canonical21_sha256,
        result.preparation_retained,
        result.bindings_ready,
        result.runtime_ready
    ]);
    assert_eq!(sequence.as_array().unwrap().len(), 9);
    assert!(proposal(&json_frame(&sequence)).is_err());
    let duplicated = String::from_utf8(json_frame(&object))
        .unwrap()
        .replace("\"version\":1", "\"version\":1,\"version\":1");
    assert!(proposal(duplicated.as_bytes()).is_err());

    let owner_object = json!({ "displayName": "Owner reviewed display name", "systemPrompt": "",
        "channelId": original().channel_id, "recipe": "codex-local-v1" });
    let owner = serde_json::from_value::<OwnerSaveRequest>(owner_object.clone()).unwrap();
    assert_eq!(owner.display_name, request().display_name);
    let owner_sequence = json!([
        owner.display_name,
        owner.system_prompt,
        owner.channel_id,
        owner.recipe
    ]);
    assert_eq!(owner_sequence.as_array().unwrap().len(), 4);
    assert!(serde_json::from_slice::<OwnerSaveRequest>(
        &serde_json::to_vec(&owner_sequence).unwrap()
    )
    .is_err());
    let duplicated_owner = serde_json::to_string(&owner_object).unwrap().replace(
        "\"systemPrompt\":\"\"",
        "\"systemPrompt\":\"\",\"systemPrompt\":\"\"",
    );
    assert!(serde_json::from_str::<OwnerSaveRequest>(&duplicated_owner).is_err());
}

#[test]
fn native_prepare_aggregate_budget_counts_both_fixed_descriptor_pairs() {
    fn padded(mut frame: Vec<u8>) -> Vec<u8> {
        assert_eq!(frame.pop(), Some(b'\n'));
        frame.resize(FRAME_LIMIT - 1, b' ');
        frame.push(b'\n');
        assert!(checked_frame(&frame).is_ok());
        frame
    }
    let startup = padded(json_frame(
        &json!({ "version": 1, "host": {}, "operation": {}, "execution": {} }),
    ));
    let result = padded(json_frame(&serde_json::to_value(valid_result()).unwrap()));
    assert!(proposal(&result).is_ok());
    let mut budget = TrafficBudget::default();
    budget.outbound(startup.len()).unwrap(); // FD3
    for id in 1..=15 {
        let request = padded(frame(id, "capture", None));
        budget.inbound(request.len()).unwrap(); // FD1
        budget.reserve_response().unwrap();
        budget.outbound(FRAME_LIMIT).unwrap(); // FD0, before queue/write
    }
    budget.inbound(result.len()).unwrap(); // FD4
    assert_eq!(budget.outbound, TRAFFIC_LIMIT);
    assert_eq!(budget.inbound, TRAFFIC_LIMIT);
    assert!(budget.outbound(1).is_err());
    assert!(budget.inbound(1).is_err());
    assert_eq!(budget.outbound, TRAFFIC_LIMIT);
    assert_eq!(budget.inbound, TRAFFIC_LIMIT);
}

#[test]
fn native_prepare_insufficient_response_or_input_budget_prevents_retain_dispatch() {
    for outbound_exhausted in [true, false] {
        let mut holder = new_holder();
        let mut boundary = FakeC2::default();
        let mut budget = TrafficBudget::default();
        receive_callback(
            &mut holder,
            &frame(1, "capture", None),
            &mut boundary,
            &mut budget,
        )
        .unwrap();
        holder.response_flushed().unwrap();
        if outbound_exhausted {
            budget.outbound = TRAFFIC_LIMIT - FRAME_LIMIT + 1;
        } else {
            budget.inbound = TRAFFIC_LIMIT;
        }
        assert!(receive_callback(
            &mut holder,
            &frame(2, "retain", Some(&ack())),
            &mut boundary,
            &mut budget
        )
        .is_err());
        assert_eq!(
            boundary
                .calls
                .iter()
                .filter(|call| call.0 == "retain")
                .count(),
            0
        );
        assert!(holder.retained.is_none());
        holder.delivery_failed();
        assert!(holder.finish_input().is_err());
    }
}

#[test]
fn native_prepare_wnowait_reserves_leader_until_teardown_and_reaped_custody_refuses() {
    for reap_before_settle in [false, true] {
        let mut context = FakeContext::new("success");
        let invocation = context.preflight(&request()).unwrap();
        let startup = context.startup(&invocation.original, &prepared()).unwrap();
        let mut holder = new_holder();
        let mut child = OwnedNode::spawn(&invocation.installation).unwrap();
        child
            .exchange(
                &startup,
                Instant::now() + Duration::from_secs(3),
                &mut holder,
                &mut context.boundary,
            )
            .unwrap();
        assert_eq!(child.observe_exit().unwrap(), Some(true));
        assert!(child.owns_unreaped_group());
        assert!(!child.leader_reaped);
        if reap_before_settle {
            // Deliberately simulate ownership loss. A numeric PID/PGID alone
            // must never authorize another broad signal, even for test children.
            assert!(child.child.try_wait().unwrap().unwrap().success());
            assert!(!child.owns_unreaped_group());
            assert!(!child.settle());
        } else {
            assert!(child.settle());
            assert!(child.leader_reaped);
            assert!(!child.owns_unreaped_group());
        }
    }
}

#[test]
fn native_prepare_high_descriptor_copies_exclude_fixed_remap_targets() {
    let (reader, writer) = pipe().unwrap();
    let reader = high_fd(reader).unwrap();
    let writer = high_fd(writer).unwrap();
    let executable = high_fd(node_for_test().unwrap()).unwrap();
    let source = high_fd(
        File::open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("src/commands/rowvia_saved_agent_prepare_fixture.mjs"),
        )
        .unwrap(),
    )
    .unwrap();
    let descriptors = [
        reader.as_raw_fd(),
        writer.as_raw_fd(),
        executable.as_raw_fd(),
        source.as_raw_fd(),
    ];
    for (index, descriptor) in descriptors.into_iter().enumerate() {
        assert!(descriptor >= 10);
        assert!(!descriptors[..index].contains(&descriptor));
        assert_ne!(
            rustix::io::fcntl_getfd([&reader, &writer, &executable, &source][index]).unwrap()
                & rustix::io::FdFlags::CLOEXEC,
            rustix::io::FdFlags::empty()
        );
    }
}

#[test]
fn native_prepare_safe_spawn_preserves_named_entry_cwd_and_fd_protocol() {
    let mut context = FakeContext::new("bootstrap-contract");
    let mut invocation = context.preflight(&request()).unwrap();
    invocation.installation.working_directory = invocation
        .installation
        .entrypoint
        .parent()
        .unwrap()
        .to_path_buf();
    let parent_directory = std::env::current_dir().unwrap();
    assert_ne!(parent_directory, invocation.installation.working_directory);
    let mut holder =
        NativeRegistrationCallbacks::from_prepared(&invocation.original, &prepared()).unwrap();
    let startup = context.startup(&invocation.original, &prepared()).unwrap();
    let mut child = OwnedNode::spawn(&invocation.installation).unwrap();
    let result = child.exchange(
        &startup,
        Instant::now() + Duration::from_secs(3),
        &mut holder,
        &mut context.boundary,
    );
    assert!(child.settle());
    assert!(child.take_family_settlement().is_some());
    assert!(proposal(&result.unwrap()).is_ok());
    assert_eq!(context.boundary.calls.len(), 3);
    assert_eq!(std::env::current_dir().unwrap(), parent_directory);
}
