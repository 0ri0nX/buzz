use super::super::tests::node_for_test;
use super::*;
use crate::commands::agents::rowvia_saved_agent::{
    ConfirmedRowviaSavedAgentPreparationAck, RowviaSavedAgentNativeScope,
    RowviaSavedAgentOperationScope, RowviaSavedAgentScope, RowviaSavedAgentScopeExpectation,
};
use serde_json::json;

fn cohort() -> owner::Resumed {
    let original = RowviaSavedAgentPreparation {
        operation_id: "01940000-0000-7000-8000-000000000077".into(),
        owner_pubkey: "a".repeat(64),
        relay_url: "wss://fixture.invalid".into(),
        source_instance: "owned-fixture".into(),
        channel_id: "01940000-0000-7000-8000-000000000088".into(),
        name: "Owned fixture".into(),
        system_prompt: "Fixture instructions".into(),
        recipe: SavedAgentRecipe::CodexDevelopmentMemoryGitReadV1,
        model: "gpt-6.1-sol".into(),
        workspace: "/fixture/workspaces/01940000-0000-7000-8000-000000000077".into(),
        capsule_run:
            "/fixture/operations/01940000-0000-7000-8000-000000000077/capsule/buzz-worker/run"
                .into(),
    };
    let prepared = PreparedRowviaSavedAgent {
        operation_id: original.operation_id.clone(),
        pubkey: "b".repeat(64),
        core_agent_name: format!("saved-{}", original.operation_id.replace('-', "")),
        saved_authorization_sha256: "c".repeat(64),
        native_config_sha256: "d".repeat(64),
        created_stopped: true,
        bindings_ready: false,
        runtime_ready: false,
    };
    let expected = expected(&original, &prepared).unwrap();
    let ack = SavedAgentPreparationAck {
        version: 1,
        operation_id: original.operation_id,
        agent_pubkey: prepared.pubkey.clone(),
        native_config_sha256: prepared.native_config_sha256.clone(),
        saved_authorization_sha256: prepared.saved_authorization_sha256.clone(),
        recipe_sha256: "1".repeat(64),
        request_sha256: "2".repeat(64),
        canonical21_sha256: "3".repeat(64),
        prepared_receipt_sha256: "4".repeat(64),
    };
    owner::Resumed {
        expected,
        prepared,
        ack,
    }
}

#[derive(Default)]
struct Boundary {
    captures: usize,
    revalidations: usize,
    retains: usize,
}
impl C2Boundary for Boundary {
    fn capture(
        &mut self,
        expected: &RowviaSavedAgentScopeExpectation,
    ) -> Result<RowviaSavedAgentScope, ()> {
        self.captures += 1;
        let s = &expected.selection;
        Ok(RowviaSavedAgentScope {
            version: 1,
            owner_pubkey: s.owner_pubkey.clone(),
            relay_origin: s.relay_url.clone(),
            source_instance: s.source_instance.clone(),
            stopped: true,
            pending: true,
            operation: RowviaSavedAgentOperationScope {
                version: 1,
                operation_id: s.operation_id.clone(),
                agent_name: format!("saved-{}", s.operation_id.replace('-', "")),
                recipe_id: RECIPE.into(),
                native: RowviaSavedAgentNativeScope {
                    agent_pubkey: expected.agent_pubkey.clone(),
                    saved_authorization_sha256: expected.saved_authorization_sha256.clone(),
                    native_config_sha256: expected.native_config_sha256.clone(),
                    channel_id: s.channel_id.clone(),
                    system_prompt: s.system_prompt.clone(),
                    model: s.model.clone(),
                    workspace: s.workspace.clone(),
                    capsule_run: s.capsule_run.clone(),
                },
            },
        })
    }
    fn retain(
        &mut self,
        _: &RowviaSavedAgentScopeExpectation,
        _: &SavedAgentPreparationAck,
    ) -> Result<ConfirmedRowviaSavedAgentPreparationAck, ()> {
        self.retains += 1;
        Err(())
    }
    fn revalidate(
        &mut self,
        _: &RowviaSavedAgentScopeExpectation,
        ack: &SavedAgentPreparationAck,
    ) -> Result<ConfirmedRowviaSavedAgentPreparationAck, ()> {
        self.revalidations += 1;
        Ok(ConfirmedRowviaSavedAgentPreparationAck {
            ack: ack.clone(),
            confirmed: true,
        })
    }
}
fn fixture() -> FixedInstallation {
    FixedInstallation {
        node: node_for_test().unwrap(),
        entrypoint: Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/commands/rowvia_saved_agent_owner_host_fixture.mjs"),
        working_directory: "/".into(),
        _held_source_custody: vec![],
    }
}
fn holder(resumed: &owner::Resumed) -> NativeRegistrationCallbacks {
    NativeRegistrationCallbacks::from_resumed(
        &resumed.expected.selection,
        &resumed.prepared,
        &resumed.ack,
    )
    .unwrap()
}

#[test]
fn owner_host_public_pin_size_is_bounded_for_metadata_and_stream() {
    assert_eq!(MAX_PUBLIC_PIN_BYTES, 536_870_912);
    assert_eq!(checked_public_pin_size(0, 296_865_480), Ok(296_865_480));
    assert_eq!(
        checked_public_pin_size(0, MAX_PUBLIC_PIN_BYTES),
        Ok(MAX_PUBLIC_PIN_BYTES)
    );
    assert!(checked_public_pin_size(0, MAX_PUBLIC_PIN_BYTES + 1).is_err());
    assert_eq!(
        checked_public_pin_size(MAX_PUBLIC_PIN_BYTES - 16_384, 16_384),
        Ok(MAX_PUBLIC_PIN_BYTES)
    );
    assert!(checked_public_pin_size(MAX_PUBLIC_PIN_BYTES - 16_384, 16_385).is_err());
    assert!(checked_public_pin_size(u64::MAX, 1).is_err());
    assert!(checked_public_pin_size(1, u64::MAX).is_err());
}

#[test]
fn owner_host_missing_qualification_precedes_save() {
    let pins = [Pin {
        path: "/fixture/owner-entrypoint.mjs",
        sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    }];
    let seal = hex::encode(Sha256::digest(format!(
        "{} {}\n",
        pins[0].path, pins[0].sha256
    )));
    let qualifies = |pins: &[Pin], seal: &str, entrypoint: &str, directory: &str, root: &str| {
        qualification_valid(pins, seal, entrypoint, directory, root)
    };
    assert!(qualifies(
        &pins,
        &seal,
        pins[0].path,
        "/fixture",
        "/fixture/artifacts"
    ));
    // Explicit missing fixtures, independent of legitimate production fills.
    // Production installation() checks this same pure gate before opening any
    // executable/source or allowing the subsequent Native Save.
    assert!(!qualifies(
        &[],
        &seal,
        pins[0].path,
        "/fixture",
        "/fixture/artifacts"
    ));
    assert!(!qualifies(
        &pins,
        "",
        pins[0].path,
        "/fixture",
        "/fixture/artifacts"
    ));
    assert!(!qualifies(
        &pins,
        &seal,
        "",
        "/fixture",
        "/fixture/artifacts"
    ));
    assert!(!qualifies(
        &pins,
        &seal,
        pins[0].path,
        "",
        "/fixture/artifacts"
    ));
    assert!(!qualifies(&pins, &seal, pins[0].path, "/fixture", ""));
    assert!(!qualifies(
        &pins,
        &"b".repeat(64),
        pins[0].path,
        "/fixture",
        "/fixture/artifacts"
    ));
}

#[test]
fn owner_host_resumed_holder_rejects_ack_cohort_and_retain() {
    let mut resumed = cohort();
    resumed.ack.agent_pubkey = "f".repeat(64);
    assert!(NativeRegistrationCallbacks::from_resumed(
        &resumed.expected.selection,
        &resumed.prepared,
        &resumed.ack
    )
    .is_err());
    let resumed = cohort();
    let mut holder = holder(&resumed);
    let mut boundary = Boundary::default();
    holder
        .receive_with(
            &frame(&json!({"version":1,"id":1,"kind":"capture"})).unwrap(),
            &mut boundary,
        )
        .unwrap();
    holder.response_flushed().unwrap();
    assert!(holder
        .receive_with(
            &frame(&json!({"version":1,"id":2,"kind":"retain","ack":resumed.ack})).unwrap(),
            &mut boundary
        )
        .is_err());
    assert_eq!(boundary.retains, 0);
}

#[test]
fn owner_host_resumed_zero_callback_eof_cannot_finalize() {
    let resumed = cohort();
    let mut boundary = Boundary::default();
    let mut holder = holder(&resumed);
    let mut child = OwnedNode::spawn(&fixture()).unwrap();
    // The inert fixture emits a valid result and closes all streams without
    // Capture. Even real exit+EOF+settlement cannot replace this callback fence.
    let startup = frame(&json!({"version":1,"kind":"preflight"})).unwrap();
    child
        .exchange(
            &startup,
            Instant::now() + Duration::from_secs(3),
            &mut holder,
            &mut boundary,
        )
        .unwrap();
    assert!(child.settle());
    assert!(child.take_family_settlement().is_some());
    assert!(holder
        .finalize_prepared_with(
            &resumed.ack.operation_id,
            &resumed.ack.agent_pubkey,
            &resumed.ack.recipe_sha256,
            &resumed.ack.canonical21_sha256,
            &mut boundary
        )
        .is_err());
    assert_eq!(
        (boundary.captures, boundary.retains, boundary.revalidations),
        (0, 0, 0)
    );
}

#[test]
fn owner_host_real_resumed_pipe_settles_family_before_final_c2() {
    let resumed = cohort();
    let mut boundary = Boundary::default();
    let mut holder = holder(&resumed);
    let mut child = OwnedNode::spawn(&fixture()).unwrap();
    let leader = child.child.id();
    let startup =
        frame(&json!({"version":1,"kind":"review","fixture":"descendant","ack":resumed.ack}))
            .unwrap();
    let result = child
        .exchange(
            &startup,
            Instant::now() + Duration::from_secs(3),
            &mut holder,
            &mut boundary,
        )
        .unwrap();
    assert_eq!(boundary.revalidations, 1);
    let value: Value = serde_json::from_slice(checked_frame(&result).unwrap()).unwrap();
    assert_eq!(value["leader"], leader);
    assert!(value["descendant"].as_u64().is_some());
    // Pipe EOF and observed leader exit do not create a family proof.
    assert!(child.family_settlement.is_none());
    assert!(group_has_live_members(leader).unwrap());
    assert!(child.settle());
    let family = child.take_family_settlement().unwrap();
    assert_eq!(family.leader, leader);
    assert!(child.take_family_settlement().is_none());
    holder
        .finalize_prepared_with(
            &resumed.ack.operation_id,
            &resumed.ack.agent_pubkey,
            &resumed.ack.recipe_sha256,
            &resumed.ack.canonical21_sha256,
            &mut boundary,
        )
        .unwrap();
    assert_eq!(boundary.revalidations, 2);
    assert_eq!(boundary.retains, 0);
}

#[test]
fn owner_host_preflight_pipe_refuses_callbacks_without_native_save() {
    let resumed = cohort();
    let mut boundary = Boundary::default();
    let mut child = OwnedNode::spawn(&fixture()).unwrap();
    let startup = frame(&json!({"version":1,"kind":"review","ack":resumed.ack})).unwrap();
    assert!(child
        .exchange_inner(
            &startup,
            Instant::now() + Duration::from_secs(3),
            None,
            &mut boundary
        )
        .is_err());
    assert!(child.settle());
    assert!(child.take_family_settlement().is_some());
    assert_eq!(
        (boundary.captures, boundary.retains, boundary.revalidations),
        (0, 0, 0)
    );
}

#[test]
fn owner_host_lost_leader_custody_withholds_family_and_final_c2() {
    let resumed = cohort();
    let mut boundary = Boundary::default();
    let mut holder = holder(&resumed);
    let mut child = OwnedNode::spawn(&fixture()).unwrap();
    let startup = frame(&json!({"version":1,"kind":"review","ack":resumed.ack})).unwrap();
    child
        .exchange(
            &startup,
            Instant::now() + Duration::from_secs(3),
            &mut holder,
            &mut boundary,
        )
        .unwrap();
    // Deliberate ownership loss in the inert fixture. No group probe/signal may
    // follow this reap, and EOF cannot mint a replacement family token.
    assert!(child.child.try_wait().unwrap().is_some());
    child.leader_reaped = true;
    assert!(!child.settle());
    assert!(child.take_family_settlement().is_none());
    holder.delivery_failed();
    assert!(holder
        .finalize_prepared_with(
            &resumed.ack.operation_id,
            &resumed.ack.agent_pubkey,
            &resumed.ack.recipe_sha256,
            &resumed.ack.canonical21_sha256,
            &mut boundary
        )
        .is_err());
    assert_eq!(boundary.revalidations, 1);
}

fn registered(resumed: &owner::Resumed) -> Value {
    let a = &resumed.ack;
    let mut evidence = json!({"version":1,"operation_id":a.operation_id,"agent_pubkey":a.agent_pubkey,
        "canonical21_sha256":a.canonical21_sha256,"recipe_sha256":a.recipe_sha256,"request_sha256":a.request_sha256,
        "ack_sha256":digest_json(a).unwrap(),"binding_identity":"owned-fixture-binding",
        "paseo_agent_id":"01940000-0000-7000-8000-000000000099","receipt_sha256":"5".repeat(64),
        "core_graph_sha256":"6".repeat(64),"profile_sha256":"7".repeat(64),"pins_sha256":"8".repeat(64),
        "memory_capsule_sha256":"9".repeat(64),"memory_launcher_sha256":"a".repeat(64),"worker_inspection_sha256":"b".repeat(64)});
    evidence["transcript_sha256"] =
        Value::String(hex::encode(Sha256::digest(canonical(&evidence).unwrap())));
    json!({"registration":{"version":1,"operation_id":a.operation_id,"agent_pubkey":a.agent_pubkey,
        "canonical21_sha256":a.canonical21_sha256,"paseo_agent_id":"01940000-0000-7000-8000-000000000099",
        "bindings_ready":true,"runtime_prepared":true,"runtime_ready":false},"evidence":evidence})
}

#[test]
fn owner_host_registration_transcript_binds_original_cohort_and_is_closed() {
    let resumed = cohort();
    let good = registered(&resumed);
    assert!(RegisteredHostProof::checked(
        frame(&good).unwrap(),
        SettledProcessFamily { leader: 1 },
        &resumed
    )
    .is_ok());
    for path in [
        "operation_id",
        "agent_pubkey",
        "canonical21_sha256",
        "recipe_sha256",
        "request_sha256",
        "ack_sha256",
        "core_graph_sha256",
    ] {
        let mut bad = good.clone();
        bad["evidence"][path] = Value::String("f".repeat(64));
        assert!(
            RegisteredHostProof::checked(
                frame(&bad).unwrap(),
                SettledProcessFamily { leader: 1 },
                &resumed
            )
            .is_err(),
            "{path}"
        );
    }
    let mut extra = good.clone();
    extra["evidence"]["settled"] = json!(true);
    assert!(decode_frame::<HostRegistered>(&frame(&extra).unwrap()).is_err());
    let mut sequence = good.clone();
    sequence["registration"] = json!([
        1,
        resumed.ack.operation_id,
        resumed.ack.agent_pubkey,
        resumed.ack.canonical21_sha256,
        "01940000-0000-7000-8000-000000000099",
        true,
        true,
        false
    ]);
    assert!(decode_frame::<HostRegistered>(&frame(&sequence).unwrap()).is_err());
    let duplicate = String::from_utf8(frame(&good).unwrap())
        .unwrap()
        .replace("\"version\":1", "\"version\":1,\"version\":1");
    assert!(decode_frame::<HostRegistered>(duplicate.as_bytes()).is_err());
    assert!(decode_frame::<HostRegistered>(b"[]\n").is_err());
}
