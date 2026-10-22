use super::super::rowvia_saved_agent::{
    RowviaSavedAgentNativeScope, RowviaSavedAgentOperationScope,
};
use super::*;
use crate::managed_agents::SavedAgentRecipe;
use serde_json::{json, Value};

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

fn exchange(
    holder: &mut NativeRegistrationCallbacks,
    boundary: &mut FakeC2,
    request: Vec<u8>,
) -> Value {
    let response = holder.receive_with(&request, boundary).unwrap().unwrap();
    assert_eq!(response.last(), Some(&b'\n'));
    holder.response_flushed().unwrap();
    serde_json::from_slice(&response).unwrap()
}

fn capture(holder: &mut NativeRegistrationCallbacks, boundary: &mut FakeC2, id: u16) {
    let response = exchange(holder, boundary, frame(id, "capture", None));
    assert_eq!(response["kind"], "scope");
}

#[test]
fn callback_original_context_and_closed_success_shapes_are_preserved() {
    let mut input = original();
    let mut output = prepared();
    let mut holder = NativeRegistrationCallbacks::from_prepared(&input, &output).unwrap();
    // Changing the caller's values later must not replace invocation custody.
    input.name = output.core_agent_name.clone();
    input.owner_pubkey = "e".repeat(64);
    output.pubkey = "e".repeat(64);
    let mut boundary = FakeC2::default();
    let scope = exchange(&mut holder, &mut boundary, frame(1, "capture", None));
    assert_eq!(scope.as_object().unwrap().len(), 4);
    assert_eq!(scope["version"], 1);
    assert_eq!(scope["id"], 1);
    assert_eq!(scope["kind"], "scope");
    assert_eq!(scope["scope"].as_object().unwrap().len(), 7);
    assert_eq!(scope["scope"]["operation"].as_object().unwrap().len(), 5);
    assert_eq!(
        scope["scope"]["operation"]["native"]
            .as_object()
            .unwrap()
            .len(),
        8
    );
    assert_eq!(
        scope["scope"]["operation"]["agentName"],
        prepared().core_agent_name
    );
    assert_ne!(scope["scope"]["operation"]["agentName"], original().name);
    // Existing Main does repeated captures before and after the one retain.
    capture(&mut holder, &mut boundary, 2);
    let ack = ack();
    assert_eq!(
        exchange(&mut holder, &mut boundary, frame(3, "retain", Some(&ack))),
        json!({ "version":1, "id":3, "kind":"ack", "ack":ack, "confirmed":true })
    );
    capture(&mut holder, &mut boundary, 4);
    assert_eq!(
        exchange(
            &mut holder,
            &mut boundary,
            frame(5, "revalidate", Some(&ack))
        ),
        json!({ "version":1, "id":5, "kind":"ack", "ack":ack, "confirmed":true })
    );
    assert_eq!(
        exchange(
            &mut holder,
            &mut boundary,
            frame(6, "revalidate", Some(&ack))
        )["confirmed"],
        true
    );
    for (_, expected) in &boundary.calls {
        let held = &expected.selection;
        let original = original();
        assert_eq!(held.name, original.name);
        assert_ne!(held.name, prepared().core_agent_name);
        assert_eq!(held.operation_id, original.operation_id);
        assert_eq!(held.owner_pubkey, original.owner_pubkey);
        assert_eq!(held.relay_url, original.relay_url);
        assert_eq!(held.source_instance, original.source_instance);
        assert_eq!(held.channel_id, original.channel_id);
        assert_eq!(held.system_prompt, original.system_prompt);
        assert_eq!(held.recipe, original.recipe);
        assert_eq!(held.model, original.model);
        assert_eq!(held.workspace, original.workspace);
        assert_eq!(held.capsule_run, original.capsule_run);
        assert_eq!(expected.agent_pubkey, prepared().pubkey);
        assert_eq!(
            expected.native_config_sha256,
            prepared().native_config_sha256
        );
        assert_eq!(
            expected.saved_authorization_sha256,
            prepared().saved_authorization_sha256
        );
    }
    assert_eq!(
        boundary
            .calls
            .iter()
            .map(|(kind, _)| *kind)
            .collect::<Vec<_>>(),
        [
            "capture",
            "capture",
            "retain",
            "capture",
            "revalidate",
            "revalidate"
        ]
    );
    holder.finish_input().unwrap();
    assert!(holder
        .receive_with(&frame(7, "capture", None), &mut boundary)
        .is_err());
    assert_eq!(boundary.calls.len(), 6);
}

#[test]
fn callback_codec_rejects_closed_shape_utf8_duplicates_and_ack_sequences_before_dispatch() {
    let valid_ack = serde_json::to_string(&ack()).unwrap();
    let ack_value = json!(ack());
    let sequence = json!([
        ack_value["version"],
        ack_value["operationId"],
        ack_value["agentPubkey"],
        ack_value["nativeConfigSha256"],
        ack_value["savedAuthorizationSha256"],
        ack_value["recipeSha256"],
        ack_value["requestSha256"],
        ack_value["canonical21Sha256"],
        ack_value["preparedReceiptSha256"]
    ]);
    let cases = vec![
        "[]\n".into(), "null\n".into(), "{}\n".into(),
        "{\"version\":1,\"id\":1,\"kind\":\"capture\",\"ownerPubkey\":\"child\"}\n".into(),
        "{\"version\":1,\"id\":1,\"kind\":\"capture\",\"expectation\":{}}\n".into(),
        "{\"version\":1,\"id\":1,\"kind\":\"capture\",\"path\":\"/var/tmp/child\"}\n".into(),
        "{\"version\":1,\"id\":1,\"kind\":\"capture\",\"relay\":\"child\"}\n".into(),
        "{\"version\":1,\"id\":1,\"kind\":\"capture\",\"operation\":{}}\n".into(),
        "{\"version\":1,\"id\":1,\"kind\":\"retain\"}\n".into(),
        "{\"version\":1,\"id\":1,\"kind\":\"capture\",\"ack\":null}\n".into(),
        "{\"version\":1,\"id\":1,\"id\":1,\"kind\":\"capture\"}\n".into(),
        "{\"version\":1,\"version\":1,\"id\":1,\"kind\":\"capture\"}\n".into(),
        "{\"version\":1,\"id\":1,\"kind\":\"capture\",\"kind\":\"capture\"}\n".into(),
        "{\"version\":1,\"id\":1,\"kind\":\"approve\"}\n".into(),
        "{\"version\":\"1\",\"id\":1,\"kind\":\"capture\"}\n".into(),
        "{\"version\":2,\"id\":1,\"kind\":\"capture\"}\n".into(),
        "{\"version\":1,\"id\":1.0,\"kind\":\"capture\"}\n".into(),
        "{\"version\":1,\"id\":\"1\",\"kind\":\"capture\"}\n".into(),
        "{\"version\":1,\"id\":0,\"kind\":\"capture\"}\n".into(),
        "{\"version\":1,\"id\":-1,\"kind\":\"capture\"}\n".into(),
        "{\"version\":1,\"id\":129,\"kind\":\"capture\"}\n".into(),
        "{\"version\":1,\"id\":1,\"kind\":\"capture\"} {}\n".into(),
        format!("{{\"version\":1,\"id\":1,\"kind\":\"retain\",\"ack\":{sequence}}}\n"),
        format!("{{\"version\":1,\"id\":1,\"kind\":\"capture\",\"ack\":{valid_ack}}}\n"),
        format!("{{\"version\":1,\"id\":1,\"kind\":\"retain\",\"ack\":{valid_ack},\"ack\":{valid_ack}}}\n"),
        format!("{{\"version\":1,\"id\":1,\"kind\":\"retain\",\"ack\":{{\"version\":1,{}}}\n", &valid_ack[1..]),
    ];
    for case in cases {
        assert!(serde_json::from_str::<ChildRequest>(case.trim_end()).is_err());
        let mut holder = new_holder();
        let mut boundary = FakeC2::default();
        assert_eq!(
            holder.receive_with(case.as_bytes(), &mut boundary),
            Err(CallbackFailure::Protocol)
        );
        assert!(boundary.calls.is_empty());
        assert!(holder
            .receive_with(&frame(1, "capture", None), &mut boundary)
            .is_err());
        assert!(boundary.calls.is_empty());
    }
    let mut holder = new_holder();
    let mut boundary = FakeC2::default();
    let mut malformed = frame(1, "capture", None);
    malformed.insert(1, 0xff);
    assert_eq!(
        holder.receive_with(&malformed, &mut boundary),
        Err(CallbackFailure::Protocol)
    );
    assert!(boundary.calls.is_empty());
}

#[test]
fn callback_order_replays_and_exact_ack_are_fenced_before_dispatch() {
    for request in [
        frame(2, "capture", None),
        frame(1, "retain", Some(&ack())),
        frame(1, "revalidate", Some(&ack())),
    ] {
        let mut holder = new_holder();
        let mut boundary = FakeC2::default();
        assert!(holder.receive_with(&request, &mut boundary).is_err());
        assert!(boundary.calls.is_empty());
    }
    for mutation in 0..9 {
        let mut holder = new_holder();
        let mut boundary = FakeC2::default();
        capture(&mut holder, &mut boundary, 1);
        let ack = ack();
        exchange(&mut holder, &mut boundary, frame(2, "retain", Some(&ack)));
        let mut changed = json!(ack);
        let field = [
            "version",
            "operationId",
            "agentPubkey",
            "nativeConfigSha256",
            "savedAuthorizationSha256",
            "recipeSha256",
            "requestSha256",
            "canonical21Sha256",
            "preparedReceiptSha256",
        ][mutation];
        changed[field] = if field == "version" {
            json!(2)
        } else if field == "operationId" {
            json!("019a5432-1234-7abc-8def-0123456789ac")
        } else {
            json!("e".repeat(64))
        };
        let changed: SavedAgentPreparationAck = serde_json::from_value(changed).unwrap();
        assert!(holder
            .receive_with(&frame(3, "revalidate", Some(&changed)), &mut boundary)
            .is_err());
        assert_eq!(boundary.calls.len(), 2, "changed {field} reached boundary");
    }
    for request in [
        frame(1, "capture", None),
        frame(2, "retain", Some(&ack())),
        frame(3, "retain", Some(&ack())),
        frame(4, "capture", None),
    ] {
        let mut holder = new_holder();
        let mut boundary = FakeC2::default();
        capture(&mut holder, &mut boundary, 1);
        exchange(&mut holder, &mut boundary, frame(2, "retain", Some(&ack())));
        assert!(holder.receive_with(&request, &mut boundary).is_err());
        assert_eq!(boundary.calls.len(), 2);
    }
}

#[test]
fn callback_inflight_partial_eof_and_delivery_failure_never_retry() {
    let mut holder = new_holder();
    let mut boundary = FakeC2::default();
    let request = frame(1, "capture", None);
    let split = request.len() / 2;
    assert_eq!(
        holder.receive_with(&request[..split], &mut boundary),
        Ok(None)
    );
    assert!(boundary.calls.is_empty());
    assert!(holder
        .receive_with(&request[split..], &mut boundary)
        .unwrap()
        .is_some());
    assert_eq!(boundary.calls.len(), 1);
    assert!(holder
        .receive_with(&frame(2, "capture", None), &mut boundary)
        .is_err());
    assert_eq!(boundary.calls.len(), 1);
    holder.response_flushed().unwrap();
    assert!(holder
        .receive_with(&frame(2, "capture", None), &mut boundary)
        .is_err());
    assert_eq!(boundary.calls.len(), 1);
    let mut holder = new_holder();
    let mut boundary = FakeC2::default();
    assert_eq!(
        holder.receive_with(&request[..split], &mut boundary),
        Ok(None)
    );
    assert_eq!(holder.finish_input(), Err(CallbackFailure::Protocol));
    assert!(boundary.calls.is_empty());
    let mut holder = new_holder();
    let mut boundary = FakeC2::default();
    let mut pipelined = request.clone();
    pipelined.extend(frame(2, "capture", None));
    assert!(holder.receive_with(&pipelined, &mut boundary).is_err());
    assert!(boundary.calls.is_empty());
    let mut holder = new_holder();
    let mut boundary = FakeC2::default();
    capture(&mut holder, &mut boundary, 1);
    assert!(holder
        .receive_with(&frame(2, "retain", Some(&ack())), &mut boundary)
        .unwrap()
        .is_some());
    assert_eq!(holder.delivery_failed(), CallbackFailure::Unknown);
    assert!(holder.response_flushed().is_err());
    assert!(holder
        .receive_with(&frame(2, "retain", Some(&ack())), &mut boundary)
        .is_err());
    assert_eq!(boundary.calls.len(), 2);
}

#[test]
fn callback_native_errors_have_closed_terminal_results_without_diagnostics() {
    for (failure, wrong_confirmation) in [
        ("capture", false),
        ("retain", false),
        ("revalidate", false),
        ("retain", true),
        ("revalidate", true),
    ] {
        let mut holder = new_holder();
        let mut boundary = FakeC2::default();
        let (id, request) = if failure == "capture" {
            (1, frame(1, "capture", None))
        } else {
            capture(&mut holder, &mut boundary, 1);
            if failure == "retain" {
                (2, frame(2, "retain", Some(&ack())))
            } else {
                exchange(&mut holder, &mut boundary, frame(2, "retain", Some(&ack())));
                (3, frame(3, "revalidate", Some(&ack())))
            }
        };
        boundary.fail = if wrong_confirmation {
            None
        } else {
            Some(failure)
        };
        boundary.wrong_confirmation = wrong_confirmation;
        let before = boundary.calls.len();
        assert_eq!(
            exchange(&mut holder, &mut boundary, request),
            json!({"version":1,"id":id,"kind":if failure=="retain" {"unknown"} else {"refused"}})
        );
        assert_eq!(boundary.calls.len(), before + 1);
        assert!(holder
            .receive_with(&frame(id + 1, "capture", None), &mut boundary)
            .is_err());
        assert_eq!(boundary.calls.len(), before + 1);
    }
}

#[test]
fn callback_line_cumulative_id_and_response_bounds_are_fixed() {
    let mut holder = new_holder();
    let mut boundary = FakeC2::default();
    assert!(holder
        .receive_with(&vec![b' '; MAX_LINE + 1], &mut boundary)
        .is_err());
    assert!(boundary.calls.is_empty());
    let mut holder = new_holder();
    let mut boundary = FakeC2::default();
    for id in 1..=16 {
        let mut request = frame(id, "capture", None);
        request.pop();
        request.resize(MAX_LINE - 1, b' ');
        request.push(b'\n');
        exchange(&mut holder, &mut boundary, request);
    }
    assert!(holder
        .receive_with(&frame(17, "capture", None), &mut boundary)
        .is_err());
    assert_eq!(boundary.calls.len(), 16);
    let mut holder = new_holder();
    let mut boundary = FakeC2::default();
    for id in 1..=MAX_ID {
        capture(&mut holder, &mut boundary, id);
    }
    assert!(holder
        .receive_with(&frame(129, "capture", None), &mut boundary)
        .is_err());
    assert_eq!(boundary.calls.len(), 128);
    let mut holder = new_holder();
    let mut boundary = FakeC2 {
        extra_prompt: MAX_LINE,
        ..Default::default()
    };
    assert_eq!(
        holder.receive_with(&frame(1, "capture", None), &mut boundary),
        Err(CallbackFailure::Unknown)
    );
    assert_eq!(boundary.calls.len(), 1);
    let mut holder = new_holder();
    let mut boundary = FakeC2 {
        extra_prompt: 32 * 1024,
        ..Default::default()
    };
    let mut sent = 0;
    for id in 1..=MAX_ID {
        match holder.receive_with(&frame(id, "capture", None), &mut boundary) {
            Ok(Some(bytes)) => {
                assert!(bytes.len() <= MAX_LINE);
                sent += bytes.len();
                assert!(sent <= MAX_TRAFFIC);
                holder.response_flushed().unwrap();
            }
            Err(CallbackFailure::Unknown) => {
                assert!(sent > MAX_TRAFFIC - MAX_LINE);
                return;
            }
            _ => panic!("unexpected callback bound result"),
        }
    }
    panic!("cumulative outgoing bound was not enforced");
}

#[test]
fn callback_constructor_requires_consistent_native_preparation_result() {
    for mutation in 0..7 {
        let mut prepared = prepared();
        match mutation {
            0 => prepared.operation_id.push('x'),
            1 => prepared.core_agent_name = original().name,
            2 => prepared.created_stopped = false,
            3 => prepared.bindings_ready = true,
            4 => prepared.runtime_ready = true,
            5 => prepared.pubkey = "0".repeat(64),
            _ => prepared.native_config_sha256 = "D".repeat(64),
        }
        assert!(NativeRegistrationCallbacks::from_prepared(&original(), &prepared).is_err());
    }
}
