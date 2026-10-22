use super::resume_tests::{fingerprint, fixture_record, OPERATION, RELAY};
use super::*;
use std::{collections::HashMap, fs, path::Path, sync::atomic::Ordering};

fn result_for(proof: &VerifiedRowviaSavedAgentPreparation) -> RowviaSavedAgentRegistrationResult {
    RowviaSavedAgentRegistrationResult {
        version: 1,
        operation_id: proof.prepared.operation_id.clone(),
        agent_pubkey: proof.prepared.pubkey.clone(),
        canonical21_sha256: proof.ack.canonical21_sha256.clone(),
        paseo_agent_id: "019a5432-1234-4abc-8def-0123456789ab".into(),
        bindings_ready: true,
        runtime_prepared: true,
        runtime_ready: false,
    }
}

fn completion(rows: &[ManagedAgentRecord]) -> VerifiedRowviaSavedAgentRegistrationCompletion {
    let proof = checked_resume_target(rows, &HashMap::<_, ()>::new(), OPERATION).unwrap();
    let result = result_for(&proof);
    registration_completion_for_test(proof, result).unwrap()
}

#[test]
fn native_finalize_completion_tuple_is_closed_and_exact() {
    let rows = vec![fixture_record(&"a".repeat(64))];
    assert!(validate_registration_completion(&completion(&rows)).is_ok());
    for mutation in 0..12 {
        let proof = checked_resume_target(&rows, &HashMap::<_, ()>::new(), OPERATION).unwrap();
        let mut result = result_for(&proof);
        match mutation {
            0 => result.version = 2,
            1 => result.operation_id = "019a5432-1234-7abc-8def-0123456789ac".into(),
            2 => result.agent_pubkey = "c".repeat(64),
            3 => result.canonical21_sha256 = "5".repeat(64),
            4 => result.bindings_ready = false,
            5 => result.runtime_prepared = false,
            6 => result.runtime_ready = true,
            7 => result.paseo_agent_id.clear(),
            8 => result.paseo_agent_id = uuid::Uuid::nil().to_string(),
            9 => result.paseo_agent_id = result.paseo_agent_id.to_uppercase(),
            10 => result.paseo_agent_id = "not-a-paseo-id".into(),
            _ => result.canonical21_sha256.clear(),
        }
        assert!(
            registration_completion_for_test(proof, result).is_err(),
            "mutation {mutation}"
        );
    }
    for mutation in 0..6 {
        let mut candidate = completion(&rows);
        match mutation {
            0 => candidate.preparation.prepared.created_stopped = false,
            1 => candidate.preparation.prepared.bindings_ready = true,
            2 => candidate.preparation.prepared.runtime_ready = true,
            3 => candidate.preparation.prepared.native_config_sha256 = "5".repeat(64),
            4 => candidate.preparation.prepared.saved_authorization_sha256 = "5".repeat(64),
            _ => candidate
                .preparation
                .expected
                .selection
                .capsule_run
                .push('x'),
        }
        assert!(checked_registration_completion_target(
            &rows,
            &HashMap::<_, ()>::new(),
            &candidate
        )
        .is_err());
    }
}

#[test]
fn native_finalize_checked_target_rejects_pending_config_auth_scope_duplicates_and_live_state() {
    let original = fixture_record(&"a".repeat(64));
    let proof = completion(std::slice::from_ref(&original));
    let empty = HashMap::<_, ()>::new();
    assert_eq!(
        checked_registration_completion_target(std::slice::from_ref(&original), &empty, &proof),
        Ok(0)
    );
    for mutation in 0..30 {
        let mut changed = original.clone();
        match mutation {
            0 => changed.name.push('!'),
            1 => changed.system_prompt = Some("changed".into()),
            2 => changed.auth_tag = Some("changed".into()),
            3 => changed.agent_args.push("changed".into()),
            4 => changed.runtime_pid = Some(42),
            5 => changed.start_on_app_launch = true,
            6 => changed.auto_restart_on_config_change = true,
            7 => changed.pending_saved_registration = None,
            8 => changed.pubkey = changed.pubkey.to_uppercase(),
            9 => changed.relay_url = "ws://127.0.0.1:10".into(),
            n => {
                let p = changed.pending_saved_registration.as_mut().unwrap();
                match n {
                    10 => p.prepared_ack = None,
                    11 => p.operation_id.push('x'),
                    12 => p.owner_pubkey = "c".repeat(64),
                    13 => p.workspace.push('x'),
                    14 => p.capsule_run.push('x'),
                    15 => p.model.push('x'),
                    16 => p.source_instance.push('x'),
                    17 => p.channel_id.push('x'),
                    18 => p.native_config_sha256 = "5".repeat(64),
                    19 => p.saved_authorization_sha256 = "5".repeat(64),
                    20 => p.relay_url = "ws://127.0.0.1:10".into(),
                    n => {
                        let ack = p.prepared_ack.as_mut().unwrap();
                        match n {
                            21 => ack.version = 2,
                            22 => ack.operation_id.push('x'),
                            23 => ack.agent_pubkey = "c".repeat(64),
                            24 => ack.native_config_sha256 = "5".repeat(64),
                            25 => ack.saved_authorization_sha256 = "5".repeat(64),
                            26 => ack.recipe_sha256 = "5".repeat(64),
                            27 => ack.request_sha256 = "5".repeat(64),
                            28 => ack.canonical21_sha256 = "5".repeat(64),
                            _ => ack.prepared_receipt_sha256 = "5".repeat(64),
                        }
                    }
                }
            }
        }
        assert!(
            checked_registration_completion_target(&[changed], &empty, &proof).is_err(),
            "mutation {mutation}"
        );
    }
    for mutation in 0..3 {
        let mut duplicate = original.clone();
        if mutation != 0 {
            duplicate.pubkey = "c".repeat(64);
        }
        if mutation == 2 {
            duplicate
                .pending_saved_registration
                .as_mut()
                .unwrap()
                .operation_id = "019a5432-1234-7abc-8def-0123456789ac".into();
        }
        assert!(checked_registration_completion_target(
            &[original.clone(), duplicate],
            &empty,
            &proof
        )
        .is_err());
    }
    let mut live = HashMap::new();
    live.insert(
        ManagedAgentRuntimeKey {
            relay_url: "ws://127.0.0.1:10".into(),
            pubkey: original.pubkey.to_uppercase(),
        },
        (),
    );
    assert!(checked_registration_completion_target(&[original], &live, &proof).is_err());
}

fn confined(name: &str, test: fn(&Path)) {
    const CHILD: &str = "BUZZ_ROWVIA_FINALIZATION_TEST_HOME";
    if let Some(home) = std::env::var_os(CHILD) {
        return test(Path::new(&home));
    }
    let home = tempfile::Builder::new()
        .prefix("rowvia-cas-")
        .tempdir_in("/var/tmp")
        .unwrap();
    let selector = format!("commands::agents::rowvia_saved_agent::finalization_tests::{name}");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &selector, "--nocapture"])
        .env(CHILD, home.path())
        .env("HOME", home.path())
        .env("XDG_DATA_HOME", home.path())
        .env("APPDATA", home.path())
        .env("LOCALAPPDATA", home.path())
        .env_remove("BUZZ_PRIVATE_KEY")
        .env_remove("BUZZ_AUTH_TAG")
        .env_remove("BUZZ_RELAY_URL")
        .env_remove("BUZZ_NETWORK_TRACE")
        .output()
        .unwrap();
    assert!(
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn with_app(
    home: &Path,
    test: impl FnOnce(&tauri::AppHandle<tauri::test::MockRuntime>, &Path, Vec<ManagedAgentRecord>),
) {
    storage::NO_KEYCHAIN_FOR_TEST.store(true, Ordering::SeqCst);
    pin_nest_dir_for_test(home.join("nest"));
    let app_data = home.join("app-data");
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().identifier = app_data.to_str().unwrap().into();
    let state = crate::app_state::build_app_state();
    *state.relay_url_override.lock().unwrap() = Some(RELAY.into());
    let owner = state.keys.lock().unwrap().public_key().to_hex();
    let app = tauri::test::mock_builder()
        .manage(state)
        .build(context)
        .unwrap();
    fs::create_dir_all(app_data.join("agents")).unwrap();
    let store = app_data.join("agents/managed-agents.json");
    let record = fixture_record(&owner);
    let mut other = record.clone();
    other.pubkey = "d".repeat(64);
    other.pending_saved_registration = None;
    other.name = "unrelated preserved".into();
    let rows = vec![other, record];
    fs::write(&store, serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
    test(app.handle(), &store, rows);
    assert!(!home.join("nest").exists());
}

#[test]
fn native_finalize_locked_cas_clears_only_pending_and_never_starts() {
    confined(
        "native_finalize_locked_cas_clears_only_pending_and_never_starts",
        |home| {
            with_app(home, |app, store, rows| {
                let proof = completion(&rows);
                let duplicate_proof = completion(&rows);
                let mut expected = rows.clone();
                expected[1].pending_saved_registration = None;
                finalize_rowvia_saved_agent_registration(app, proof).unwrap();
                assert_eq!(
                    storage::load_agent_store_readonly(app).unwrap().records,
                    expected
                );
                assert!(app
                    .state::<AppState>()
                    .managed_agent_processes
                    .lock()
                    .unwrap()
                    .is_empty());
                let before = fingerprint(store);
                assert_eq!(
                    finalize_rowvia_saved_agent_registration(app, duplicate_proof),
                    Err(FINALIZATION_REFUSED.into())
                );
                assert_eq!(fingerprint(store), before);
            })
        },
    );
}

#[test]
fn native_finalize_current_scope_refusals_preserve_store() {
    confined(
        "native_finalize_current_scope_refusals_preserve_store",
        |home| {
            with_app(home, |app, store, rows| {
                let state = app.state::<AppState>();
                let before = fingerprint(store);
                for flag in [
                    &state.identity_lost,
                    &state.keyring_locked,
                    &state.reset_failed,
                    &state.shutdown_started,
                ] {
                    flag.store(true, Ordering::SeqCst);
                    assert_eq!(
                        finalize_rowvia_saved_agent_registration(app, completion(&rows)),
                        Err(FINALIZATION_REFUSED.into())
                    );
                    flag.store(false, Ordering::SeqCst);
                    assert_eq!(fingerprint(store), before);
                }
                *state.relay_url_override.lock().unwrap() = Some("ws://127.0.0.1:10".into());
                assert!(finalize_rowvia_saved_agent_registration(app, completion(&rows)).is_err());
                *state.relay_url_override.lock().unwrap() = Some(RELAY.into());
                let old = state.keys.lock().unwrap().clone();
                *state.keys.lock().unwrap() = nostr::Keys::generate();
                assert!(finalize_rowvia_saved_agent_registration(app, completion(&rows)).is_err());
                *state.keys.lock().unwrap() = old;
                {
                    let _lock = state.workspace_apply_lock.try_lock().unwrap();
                    assert!(
                        finalize_rowvia_saved_agent_registration(app, completion(&rows)).is_err()
                    );
                }
                assert_eq!(fingerprint(store), before);
                for mutation in 0..4 {
                    let proof = completion(&rows);
                    let mut changed = rows.clone();
                    match mutation {
                        0 => changed[1].name.push('x'),
                        1 => changed[1].auth_tag = Some("changed".into()),
                        2 => {
                            changed[1]
                                .pending_saved_registration
                                .as_mut()
                                .unwrap()
                                .prepared_ack
                                .as_mut()
                                .unwrap()
                                .request_sha256 = "5".repeat(64)
                        }
                        _ => changed[1].pending_saved_registration = None,
                    }
                    fs::write(store, serde_json::to_vec_pretty(&changed).unwrap()).unwrap();
                    let changed_before = fingerprint(store);
                    assert_eq!(
                        finalize_rowvia_saved_agent_registration(app, proof),
                        Err(FINALIZATION_REFUSED.into())
                    );
                    assert_eq!(fingerprint(store), changed_before);
                }
                fs::write(store, b"malformed synthetic JSON").unwrap();
                let invalid_before = fingerprint(store);
                assert_eq!(
                    finalize_rowvia_saved_agent_registration(app, completion(&rows)),
                    Err(FINALIZATION_REFUSED.into())
                );
                assert_eq!(fingerprint(store), invalid_before);
                fs::remove_file(store).unwrap();
                assert_eq!(
                    finalize_rowvia_saved_agent_registration(app, completion(&rows)),
                    Err(FINALIZATION_REFUSED.into())
                );
                assert!(!store.exists());
                assert_eq!(fs::read_dir(store.parent().unwrap()).unwrap().count(), 0);
            })
        },
    );
}

#[test]
fn native_finalize_faults_leave_fixed_unknown_without_rollback_or_retry() {
    confined(
        "native_finalize_faults_leave_fixed_unknown_without_rollback_or_retry",
        |home| {
            with_app(home, |app, store, rows| {
                for fault in [
                    storage::PreparationAckWriteFault::BeforeCommit,
                    storage::PreparationAckWriteFault::AfterCommit,
                    storage::PreparationAckWriteFault::Readback,
                ] {
                    fs::write(store, serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
                    let before = fingerprint(store);
                    storage::FINALIZATION_WRITE_FAULT.with(|slot| slot.set(Some(fault)));
                    assert_eq!(
                        finalize_rowvia_saved_agent_registration(app, completion(&rows)),
                        Err(FINALIZATION_UNKNOWN.into())
                    );
                    let retained = storage::load_agent_store_readonly(app).unwrap().records;
                    if fault == storage::PreparationAckWriteFault::BeforeCommit {
                        assert_eq!(fingerprint(store), before);
                        assert_eq!(retained, rows);
                    } else {
                        let mut expected = rows.clone();
                        expected[1].pending_saved_registration = None;
                        assert_eq!(retained, expected);
                    }
                    assert!(app
                        .state::<AppState>()
                        .managed_agent_processes
                        .lock()
                        .unwrap()
                        .is_empty());
                }
            })
        },
    );
}

#[test]
fn native_finalize_post_commit_scope_failure_is_unknown() {
    confined(
        "native_finalize_post_commit_scope_failure_is_unknown",
        |home| {
            with_app(home, |app, store, rows| {
                let state = app.state::<AppState>();
                let original_owner = state.keys.lock().unwrap().clone();
                let mut expected = rows.clone();
                expected[1].pending_saved_registration = None;
                for fault in [
                    FinalizationPostCommitFault::Shutdown,
                    FinalizationPostCommitFault::ResetFailed,
                    FinalizationPostCommitFault::Owner,
                    FinalizationPostCommitFault::Relay,
                ] {
                    fs::write(store, serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
                    FINALIZATION_POST_COMMIT_FAULT.with(|slot| slot.set(Some(fault)));
                    assert_eq!(
                        finalize_rowvia_saved_agent_registration(app, completion(&rows)),
                        Err(FINALIZATION_UNKNOWN.into())
                    );
                    assert_eq!(
                        storage::load_agent_store_readonly(app).unwrap().records,
                        expected
                    );
                    let before = fingerprint(store);
                    state.shutdown_started.store(false, Ordering::SeqCst);
                    state.reset_failed.store(false, Ordering::SeqCst);
                    *state.keys.lock().unwrap() = original_owner.clone();
                    *state.relay_url_override.lock().unwrap() = Some(RELAY.into());
                    assert_eq!(fingerprint(store), before);
                }
                assert!(app
                    .state::<AppState>()
                    .managed_agent_processes
                    .lock()
                    .unwrap()
                    .is_empty());
            })
        },
    );
}
