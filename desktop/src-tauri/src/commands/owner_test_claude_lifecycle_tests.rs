use super::*;

fn record() -> ManagedAgentRecord {
    serde_json::from_value(
        serde_json::json!({"pubkey":core::TARGET,"name":"Rowvia Developer Claude",
        "relay_url":core::RELAY,"acp_command":"buzz-acp","agent_command":core::PYTHON,
        "agent_command_override":core::PYTHON,"agent_args":["-IS",core::RUN],"mcp_command":"",
        "turn_timeout_seconds":300,"start_on_app_launch":false,"created_at":"","updated_at":"",
        "private_key_nsec":"","auth_tag":"SYNTHETIC_AUTH_PRIVATE_CANARY",
        "env_vars":{"BUZZ_ACP_NO_BASE_PROMPT":"true"},"parallelism":1}),
    )
    .unwrap()
}

#[test]
fn claude_lifecycle_target_contract_is_exact_and_private_fields_are_untouched() {
    let original = record();
    assert!(target_contract(std::slice::from_ref(&original), false).is_ok());
    assert!(target_contract(&[original.clone(), original.clone()], false).is_err());
    for field in [
        "pubkey",
        "agent_command_override",
        "runtime",
        "start_on_app_launch",
        "env_vars",
        "acp_command",
        "prompt",
        "model",
        "provider",
        "session",
        "respond_to",
        "allowlist",
        "team",
    ] {
        let mut changed = original.clone();
        match field {
            "pubkey" => changed.pubkey = "f".repeat(64),
            "agent_command_override" => changed.agent_command_override = Some("/bin/sh".into()),
            "runtime" => changed.runtime = Some("claude".into()),
            "start_on_app_launch" => changed.start_on_app_launch = true,
            "acp_command" => changed.acp_command = "/bin/sh".into(),
            "prompt" => changed.system_prompt = Some("PRIVATE_CANARY".into()),
            "model" => changed.model = Some("PRIVATE_CANARY".into()),
            "provider" => changed.provider = Some("PRIVATE_CANARY".into()),
            "session" => changed.session_policy = crate::managed_agents::AcpSessionPolicy::Thread,
            "respond_to" => changed.respond_to = crate::managed_agents::RespondTo::Anyone,
            "allowlist" => changed.respond_to_allowlist = vec!["PRIVATE_CANARY".into()],
            "team" => changed.team_id = Some("PRIVATE_CANARY".into()),
            _ => {
                changed
                    .env_vars
                    .insert("BUZZ_AUTH_TAG".into(), "CANARY".into());
            }
        }
        assert!(target_contract(&[changed], false).is_err(), "{field}");
    }
    let mut setup = original.clone();
    let (command, args) = core::command(true);
    setup.agent_command_override = Some(command.into());
    setup.agent_args = args;
    assert!(target_contract(std::slice::from_ref(&setup), true).is_ok());
    assert!(target_contract(&[setup], false).is_err());
    let mut initial = original.clone();
    initial.env_vars.clear();
    initial.parallelism = crate::managed_agents::DEFAULT_AGENT_PARALLELISM;
    assert!(
        target_contract(std::slice::from_ref(&initial), false).is_err(),
        "initial must never start"
    );
    assert!(target_contract_with_initial(std::slice::from_ref(&initial), &[], false, true).is_ok());
    assert!(target_contract_with_initial(&[initial], &[], true, true).is_err());
    for change in ["missing", "wrong", "extra", "parallelism"] {
        let mut changed = original.clone();
        match change {
            "missing" => changed.env_vars.clear(),
            "wrong" => {
                changed
                    .env_vars
                    .insert("BUZZ_ACP_NO_BASE_PROMPT".into(), "false".into());
            }
            "extra" => {
                changed.env_vars.insert("OTHER".into(), "value".into());
            }
            _ => changed.parallelism = 2,
        }
        assert!(target_contract(&[changed], false).is_err(), "{change}");
    }
    assert_eq!(
        original.auth_tag.as_deref(),
        Some("SYNTHETIC_AUTH_PRIVATE_CANARY")
    );
    for op in [
        "prepare_saved_claude_setup",
        "start_saved_claude_setup",
        "stop_restore_saved_claude_normal",
        "start_saved_claude_normal",
    ] {
        let serialized = serde_json::to_string(&core::public_result(op)).unwrap();
        assert!(
            !serialized.contains("CANARY")
                && !serialized.contains("nsec")
                && !serialized.contains("auth")
        );
    }
}

#[test]
fn claude_lifecycle_setup_pins_refuse_symlinks_modes_and_changed_bytes() {
    let dir = tempfile::tempdir().unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let exe = dir.path().join("rowvia-native-auth-setup");
    let profile = dir.path().join("profile.json");
    fs::write(&exe, b"inert fixture").unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(&profile, b"public fixture").unwrap();
    fs::set_permissions(&profile, fs::Permissions::from_mode(0o600)).unwrap();
    let uid = fs::metadata(dir.path()).unwrap().uid();
    let exe_hash = hex::encode(Sha256::digest(b"inert fixture"));
    let profile_hash = hex::encode(Sha256::digest(b"public fixture"));
    assert!(verify_setup_at(dir.path(), uid, &exe_hash, &profile_hash).is_ok());
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o4700)).unwrap();
    assert!(verify_setup_at(dir.path(), uid, &exe_hash, &profile_hash).is_err());
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(&profile, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(verify_setup_at(dir.path(), uid, &exe_hash, &profile_hash).is_err());
    fs::set_permissions(&profile, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(&profile, b"changed").unwrap();
    assert!(verify_setup_at(dir.path(), uid, &exe_hash, &profile_hash).is_err());
    fs::remove_file(&profile).unwrap();
    std::os::unix::fs::symlink(&exe, &profile).unwrap();
    assert!(verify_setup_at(dir.path(), uid, &exe_hash, &profile_hash).is_err());
}

#[test]
fn claude_lifecycle_partial_restore_never_claims_success_or_retries() {
    use std::cell::Cell;
    let stops = Cell::new(0);
    let restores = Cell::new(0);
    let stop = || {
        stops.set(stops.get() + 1);
        Ok(())
    };
    assert!(stop_then_restore(
        || Err("stop failed".into()),
        || {
            restores.set(1);
            Ok(())
        }
    )
    .is_err());
    assert_eq!(restores.get(), 0);
    assert!(stop_then_restore(stop, || {
        restores.set(restores.get() + 1);
        Err("restore failed".into())
    })
    .is_err());
    assert_eq!((stops.get(), restores.get()), (1, 1));
}

#[test]
fn claude_lifecycle_membership_rechecks_after_actual_admission_await() {
    let state = crate::app_state::build_app_state();
    let keys = nostr::Keys::generate();
    let event = nostr::EventBuilder::text_note("ROWVIA_E2E_ synthetic inert refusal")
        .sign_with_keys(&keys)
        .unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let mut state = state;
    state.http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(100))
        .build()
        .unwrap();
    let expired = std::sync::atomic::AtomicBool::new(false);
    let guard = || {
        if expired.load(Ordering::Acquire) {
            Err("invalid_request".into())
        } else {
            Ok(())
        }
    };
    let result =
        tauri::async_runtime::block_on(crate::relay::submit_signed_event_guarded_after_admission(
            &event,
            &state,
            &endpoint,
            &keys,
            &guard,
            async {
                expired.store(true, Ordering::Release);
            },
        ));
    assert_eq!(result.err().as_deref(), Some("invalid_request"));
    assert!(
        matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock),
        "expiry after admission must prevent even loopback connect"
    );
}

#[test]
fn claude_lifecycle_real_normal_update_and_preflight_are_confined_and_never_spawn() {
    const CHILD: &str = "BUZZ_CLAUDE_LIFECYCLE_TEST_HOME";
    if let Some(home) = std::env::var_os(CHILD) {
        return confined_update(Path::new(&home));
    }
    let home = tempfile::tempdir().unwrap();
    let mut child=std::process::Command::new(std::env::current_exe().unwrap()).args([
        "--exact","commands::agents::owner_test_claude_lifecycle::tests::claude_lifecycle_real_normal_update_and_preflight_are_confined_and_never_spawn","--nocapture"])
        .env(CHILD,home.path()).env("HOME",home.path()).env("XDG_DATA_HOME",home.path())
        .env("APPDATA",home.path()).env("LOCALAPPDATA",home.path())
        .env_remove("BUZZ_PRIVATE_KEY").env_remove("BUZZ_AUTH_TAG").env_remove("BUZZ_RELAY_URL")
        .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("production update wrapper exceeded bounded fixture deadline (possible lock recursion)");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "confined fixture failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn confined_update(home: &Path) {
    use crate::managed_agents::{
        load_managed_agents, save_managed_agents, storage::NO_KEYCHAIN_FOR_TEST,
    };
    NO_KEYCHAIN_FOR_TEST.store(true, Ordering::SeqCst);
    crate::managed_agents::pin_nest_dir_for_test(home.join("nest"));
    let data = home.join("app-data");
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().identifier = data.to_str().unwrap().to_owned();
    let state = crate::app_state::build_app_state();
    *state.relay_url_override.lock().unwrap() = Some("ws://127.0.0.1:9".into());
    let app = tauri::test::mock_builder()
        .manage(state)
        .build(context)
        .unwrap();
    let state = app.state::<AppState>();
    let handle = app.handle().clone();
    let mut original = record();
    use nostr::ToBech32;
    original.private_key_nsec = nostr::Keys::generate().secret_key().to_bech32().unwrap();
    save_managed_agents(app.handle(), std::slice::from_ref(&original)).unwrap();
    // Stabilize ordinary startup/catalog materialization before testing edits.
    drop(load_managed_agents(app.handle()).unwrap());
    let store = data.join("agents/managed-agents.json");
    let before = fs::read(&store).unwrap();
    let target_bytes = || {
        let rows: Vec<Value> = serde_json::from_slice(&fs::read(&store).unwrap()).unwrap();
        rows.into_iter()
            .find(|r| r["pubkey"] == core::TARGET)
            .unwrap()
    };
    let before_target = target_bytes();
    let input = || serde_json::from_value(core::update(true)).unwrap();
    // The actual store lock is held until authorization drifts. Even ordinary
    // load/migration must not run after that wait with stale permission.
    for error in ["invalid_request", "scope_mismatch", "not_ready"] {
        let store_guard = state.managed_agents_store_lock.lock().unwrap();
        let worker_handle = handle.clone();
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            let worker_state = worker_handle.state::<AppState>();
            entered_tx.send(()).unwrap();
            super::super::super::agent_models::update_managed_agent_local_guarded_in(
                serde_json::from_value(core::update(true)).unwrap(),
                &worker_handle,
                &worker_state,
                (&|_, _| Ok(()), &|| Err(error.into())),
            )
            .map(drop)
        });
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        drop(store_guard);
        assert_eq!(worker.join().unwrap().err().as_deref(), Some(error));
        assert!(
            fs::read(&store).unwrap() == before,
            "store wait must fail before loader mutation"
        );
    }
    let guard =
        |rows: &[ManagedAgentRecord],
         runtimes: &HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>| {
            check_target(&handle, rows, runtimes, false, false)
        };
    // Failure at the final save boundary leaves even the real normal updater's
    // store byte-identical; there was no command/probe/install call.
    for error in ["invalid_request", "scope_mismatch", "not_ready"] {
        let checks = std::sync::atomic::AtomicUsize::new(0);
        let authorize = || {
            if checks.fetch_add(1, Ordering::SeqCst) == 0 {
                Ok(())
            } else {
                Err(error.into())
            }
        };
        let result = super::super::super::agent_models::update_managed_agent_local_guarded_in(
            input(),
            app.handle(),
            &state,
            (&guard, &authorize),
        );
        assert_eq!(result.err().as_deref(), Some(error));
        // The ordinary loader can reorder/materialize catalog definitions.
        // The guarded requested target edit must remain byte-value-identical.
        assert!(
            target_bytes() == before_target,
            "failed save must leave target unchanged"
        );
    }
    // Exercise the PRODUCTION async wrapper under the same outer transition
    // lock as execute, not just the synchronous save seam. Parent bounds hangs.
    let mut response = {
        let admission = crate::managed_agents::AdmissionSnapshot::capture(&state);
        let _transition = state.managed_agent_runtime_transition.lock().unwrap();
        tauri::async_runtime::block_on(
            super::super::super::agent_models::update_managed_agent_guarded_with_finish_in(
                input(),
                &handle,
                &state,
                (&guard, &|| Ok(())),
                Some(&admission),
                |saved, _| async move { Ok(saved.unreconciled_for_test()) },
            ),
        )
        .unwrap()
    };
    response.profile_sync_error = Some("nsec1_PRIVATE_RAW_ERROR_CANARY".into());
    assert_eq!(
        discard_update(response).err().as_deref(),
        Some("operation_failed")
    );
    let rows = load_managed_agents(app.handle()).unwrap();
    let saved = target_contract(&rows, true).unwrap();
    assert!(saved.auth_tag == original.auth_tag);
    assert!(saved.private_key_nsec == original.private_key_nsec);
    assert!(!saved.start_on_app_launch && saved.runtime_pid.is_none());
    assert!(state.managed_agent_processes.lock().unwrap().is_empty());
    // A drift during the ordinary awaited start preflight must refuse before
    // executable lookup or spawn. The real setup executable is not mounted.
    let drift = std::sync::atomic::AtomicBool::new(false);
    let guard =
        |rows: &[ManagedAgentRecord],
         runtimes: &HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>| {
            check_target(&handle, rows, runtimes, true, false)?;
            if drift.load(Ordering::Acquire) {
                Err("invalid_request".into())
            } else {
                Ok(())
            }
        };
    let result =
        tauri::async_runtime::block_on(super::super::start_local_agent_after_preflight_guarded(
            app.handle(),
            &state,
            core::TARGET,
            Some("ws://127.0.0.1:9"),
            None,
            None,
            (
                |_| async {
                    drift.store(true, Ordering::Release);
                    Ok(())
                },
                Some(&guard),
                None,
            ),
        ));
    assert_eq!(result.err().as_deref(), Some("invalid_request"));
    assert!(state.managed_agent_processes.lock().unwrap().is_empty());
    // Exercise the real normal Stop on one owned, inert process group, then
    // restore using the same normal local-save body. No ACP/model is launched.
    use std::os::unix::process::CommandExt;
    let child = std::process::Command::new("/usr/bin/sleep")
        .arg("30")
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .unwrap();
    let pid = child.id();
    let process = crate::managed_agents::ManagedAgentProcess {
        child,
        log_path: home.join("inert-stop.log"),
        spawn_config: crate::managed_agents::spawn_snapshot::prospective_spawn_config_snapshot(
            saved,
            &[],
            &[],
            core::RELAY,
            &Default::default(),
            false,
        ),
        setup_mode: false,
        adapter_availability: None,
        start_nonce: "owned-stop-fixture".into(),
    };
    state.managed_agent_processes.lock().unwrap().insert(
        ManagedAgentRuntimeKey::new(core::TARGET, core::RELAY).unwrap(),
        ManagedAgentPairRuntime::starting(process),
    );
    let inspected = inspect_initial_in(&handle, &|| Ok(())).unwrap();
    assert_eq!(inspected["noTrackedPairs"], false);
    assert_eq!(inspected["stoppedInitialMatches"], false);
    assert!(
        Path::new(&format!("/proc/{pid}")).exists(),
        "inspection must not stop/reap the owned child"
    );
    let mut wrong_pid = saved.clone();
    wrong_pid.runtime_pid = Some(pid + 1);
    assert!(check_target(
        &handle,
        &[wrong_pid],
        &state.managed_agent_processes.lock().unwrap(),
        true,
        true
    )
    .is_err());
    *state.relay_url_override.lock().unwrap() = Some(core::RELAY.into());
    let stopping =
        |rows: &[ManagedAgentRecord],
         runtimes: &HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>| {
            check_target(&handle, rows, runtimes, true, true)
        };
    let stopped = super::super::stop_managed_agent_guarded_in(
        core::TARGET,
        &handle,
        &state,
        (&stopping, &|| Ok(())),
    )
    .unwrap();
    drop(stopped);
    assert!(state.managed_agent_processes.lock().unwrap().is_empty());
    assert!(
        !Path::new(&format!("/proc/{pid}")).exists(),
        "owned stop child must be reaped"
    );
    let restoring =
        |rows: &[ManagedAgentRecord],
         runtimes: &HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>| {
            check_target(&handle, rows, runtimes, true, false)
        };
    let restore_input = || serde_json::from_value(core::update(false)).unwrap();
    let checks = std::sync::atomic::AtomicUsize::new(0);
    let authorize = || {
        if checks.fetch_add(1, Ordering::SeqCst) == 0 {
            Ok(())
        } else {
            Err("invalid_request".into())
        }
    };
    assert!(
        super::super::super::agent_models::update_managed_agent_local_guarded_in(
            restore_input(),
            &handle,
            &state,
            (&restoring, &authorize)
        )
        .is_err()
    );
    assert!(
        target_contract(&load_managed_agents(&handle).unwrap(), true).is_ok(),
        "before-save failure keeps stopped setup"
    );
    let admission = crate::managed_agents::AdmissionSnapshot::capture(&state);
    let after_save = {
        let _transition = state.managed_agent_runtime_transition.lock().unwrap();
        tauri::async_runtime::block_on(
            super::super::super::agent_models::update_managed_agent_guarded_with_finish_in(
                restore_input(),
                &handle,
                &state,
                (&restoring, &|| Ok(())),
                Some(&admission),
                |saved, _| async move {
                    drop(saved);
                    Err("post_save_followup_failed".into())
                },
            ),
        )
    };
    assert_eq!(
        after_save.err().as_deref(),
        Some("post_save_followup_failed")
    );
    let restored = load_managed_agents(&handle).unwrap();
    assert!(target_contract(&restored, false).is_ok());
    assert!(
        state.managed_agent_processes.lock().unwrap().is_empty(),
        "post-save failure stays stopped even if normal already persisted"
    );
    // A later known normal start is a distinct lifecycle from setup Stop.
    // Exercise the production restore guard at BOTH normal Stop and Update.
    let normal = target_contract(&restored, false).unwrap().clone();
    let child = std::process::Command::new("/usr/bin/sleep")
        .arg("30")
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .unwrap();
    let normal_pid = child.id();
    let key = ManagedAgentRuntimeKey::new(core::TARGET, core::RELAY).unwrap();
    state.managed_agent_processes.lock().unwrap().insert(
        key.clone(),
        ManagedAgentPairRuntime::starting(crate::managed_agents::ManagedAgentProcess {
            child,
            log_path: home.join("inert-normal-stop.log"),
            spawn_config: crate::managed_agents::spawn_snapshot::prospective_spawn_config_snapshot(
                &normal,
                &[],
                &[],
                core::RELAY,
                &Default::default(),
                false,
            ),
            setup_mode: false,
            adapter_availability: None,
            start_nonce: "owned-normal-stop-fixture".into(),
        }),
    );
    let stop_guard =
        |rows: &[ManagedAgentRecord],
         runtimes: &HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>| {
            check_restore_target(&handle, rows, runtimes, true)
        };
    for change in [
        "initial",
        "command",
        "relay",
        "backend",
        "duplicate",
        "scalar",
        "self",
        "version",
        "env",
        "parallel",
    ] {
        let mut changed = normal.clone();
        match change {
            "initial" => {
                changed.env_vars.clear();
                changed.parallelism = crate::managed_agents::DEFAULT_AGENT_PARALLELISM;
            }
            "command" => changed.agent_command_override = Some("/bin/sh".into()),
            "relay" => changed.relay_url = "ws://127.0.0.1:9".into(),
            "backend" => {
                changed.backend = BackendKind::Provider {
                    id: "PRIVATE_CANARY".into(),
                    config: Value::Null,
                }
            }
            "scalar" => changed.runtime_pid = Some(normal_pid + 1),
            "self" => changed.persona_id = Some(core::TARGET.into()),
            "version" => changed.persona_source_version = Some("PRIVATE_CANARY".into()),
            "env" => {
                changed
                    .env_vars
                    .insert("PRIVATE_CANARY".into(), "true".into());
            }
            "parallel" => changed.parallelism = 2,
            _ => {}
        }
        let rows = if change == "duplicate" {
            vec![changed.clone(), changed]
        } else {
            vec![changed]
        };
        save_managed_agents(&handle, &rows).unwrap();
        assert!(
            super::super::stop_managed_agent_guarded_in(
                core::TARGET,
                &handle,
                &state,
                (&stop_guard, &|| Ok(()))
            )
            .is_err(),
            "{change}"
        );
        assert!(state
            .managed_agent_processes
            .lock()
            .unwrap()
            .get_mut(&key)
            .unwrap()
            .child
            .try_wait()
            .unwrap()
            .is_none());
    }
    save_managed_agents(&handle, std::slice::from_ref(&normal)).unwrap();
    {
        let mut pairs = state.managed_agent_processes.lock().unwrap();
        let runtime = pairs.remove(&key).unwrap();
        let foreign = ManagedAgentRuntimeKey::new(core::TARGET, "ws://127.0.0.1:9").unwrap();
        pairs.insert(foreign.clone(), runtime);
        assert!(
            check_restore_target(&handle, std::slice::from_ref(&normal), &pairs, true).is_err()
        );
        let runtime = pairs.remove(&foreign).unwrap();
        pairs.insert(key.clone(), runtime);
        let child = std::process::Command::new("/usr/bin/sleep")
            .arg("30")
            .env_clear()
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .process_group(0)
            .spawn()
            .unwrap();
        pairs.insert(
            foreign.clone(),
            ManagedAgentPairRuntime::starting(crate::managed_agents::ManagedAgentProcess {
                child,
                log_path: home.join("inert-extra-pair.log"),
                spawn_config:
                    crate::managed_agents::spawn_snapshot::prospective_spawn_config_snapshot(
                        &normal,
                        &[],
                        &[],
                        core::RELAY,
                        &Default::default(),
                        false,
                    ),
                setup_mode: false,
                adapter_availability: None,
                start_nonce: "extra-owned-pair".into(),
            }),
        );
        let refused =
            check_restore_target(&handle, std::slice::from_ref(&normal), &pairs, true).is_err();
        let mut extra = pairs.remove(&foreign).unwrap();
        extra.child.kill().unwrap();
        extra.child.wait().unwrap();
        assert!(refused, "multiple target pairs must stay refused");
    }
    let receipt_dir = data.join("agents/agent-pids");
    fs::create_dir_all(&receipt_dir).unwrap();
    let foreign_receipt = receipt_dir.join(format!("{}-foreign.json", core::TARGET));
    fs::write(&foreign_receipt, b"PRIVATE_CANARY").unwrap();
    let refused = super::super::stop_managed_agent_guarded_in(
        core::TARGET,
        &handle,
        &state,
        (&stop_guard, &|| Ok(())),
    )
    .is_err();
    fs::remove_file(&foreign_receipt).unwrap();
    assert!(refused, "unexpected receipt must stay refused");
    // Real store/runtime waits may invalidate authorization before termination.
    for (runtime_wait, error) in [
        (false, "scope_mismatch"),
        (true, "invalid_request"),
        (false, "not_ready"),
    ] {
        let held_store = (!runtime_wait).then(|| state.managed_agents_store_lock.lock().unwrap());
        let held_runtime = runtime_wait.then(|| state.managed_agent_processes.lock().unwrap());
        let worker_handle = handle.clone();
        let denied = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_denied = denied.clone();
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
        let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            let worker_state = worker_handle.state::<AppState>();
            let authorize = || {
                if worker_denied.load(Ordering::Acquire) {
                    Err(error.into())
                } else {
                    Ok(())
                }
            };
            let guard = |rows:&[ManagedAgentRecord],runtimes:&HashMap<ManagedAgentRuntimeKey,ManagedAgentPairRuntime>| {
                check_restore_target(&worker_handle, rows, runtimes, true)?;
                authorize()
            };
            entered_tx.send(()).unwrap();
            done_tx
                .send(
                    super::super::stop_managed_agent_guarded_in(
                        core::TARGET,
                        &worker_handle,
                        &worker_state,
                        (&guard, &authorize),
                    )
                    .map(drop),
                )
                .unwrap();
        });
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(done_rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .is_err());
        denied.store(true, Ordering::Release);
        drop((held_store, held_runtime));
        assert_eq!(
            done_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap()
                .err()
                .as_deref(),
            Some(error)
        );
        worker.join().unwrap();
        assert!(state
            .managed_agent_processes
            .lock()
            .unwrap()
            .get_mut(&key)
            .unwrap()
            .child
            .try_wait()
            .unwrap()
            .is_none());
    }
    {
        let admission = crate::managed_agents::AdmissionSnapshot::capture(&state);
        let _transition = state.managed_agent_runtime_transition.lock().unwrap();
        let stopping =
            |rows: &[ManagedAgentRecord],
             runtimes: &HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>| {
                check_restore_target(&handle, rows, runtimes, true)
            };
        let stopped = super::super::stop_managed_agent_guarded_in(
            core::TARGET,
            &handle,
            &state,
            (&stopping, &|| Ok(())),
        );
        if stopped.is_err() {
            // Causal RED cleanup owns only this fixture's inert child.
            if let Some(mut pair) = state.managed_agent_processes.lock().unwrap().remove(&key) {
                let _ = pair.child.kill();
                let _ = pair.child.wait();
            }
        }
        drop(stopped.expect("configured NORMAL must pass the production Stop/restore guard"));
        assert!(!Path::new(&format!("/proc/{normal_pid}")).exists());
        let restoring =
            |rows: &[ManagedAgentRecord],
             runtimes: &HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>| {
                check_restore_target(&handle, rows, runtimes, false)
            };
        let response = tauri::async_runtime::block_on(
            super::super::super::agent_models::update_managed_agent_guarded_with_finish_in(
                restore_input(),
                &handle,
                &state,
                (&restoring, &|| Ok(())),
                Some(&admission),
                |saved, _| async move { Ok(saved.unreconciled_for_test()) },
            ),
        )
        .expect("stopped NORMAL must pass the same production restore guard");
        discard_update(response).unwrap();
    }
    let rows = load_managed_agents(&handle).unwrap();
    let saved_normal = target_contract(&rows, false).unwrap();
    assert_eq!(saved_normal.private_key_nsec, normal.private_key_nsec);
    assert_eq!(saved_normal.auth_tag, normal.auth_tag);
    assert!(state.managed_agent_processes.lock().unwrap().is_empty());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let attachment_args = serde_json::json!({"expiresAt":now+60_000})
        .as_object()
        .unwrap()
        .clone();
    let workspace = tauri::async_runtime::block_on(state.workspace_apply_lock.lock());
    assert_eq!(
        check_attachment(&handle, &attachment_args, &|| Ok(()))
            .err()
            .as_deref(),
        Some("not_ready"),
        "attachment must refuse a busy workspace transaction before normal loader"
    );
    drop(workspace);
    // Pending authority can disappear while waiting for the real transition.
    // Private scope DI avoids needing/exporting the production owner's key.
    let transition = state.managed_agent_runtime_transition.lock().unwrap();
    let pending = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let changed = pending.clone();
    let worker_handle = handle.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        ready_tx.send(()).unwrap();
        let result = check_attachment_in(&worker_handle, &|| Ok(()), &|| {
            if changed.load(Ordering::Acquire) {
                Ok(())
            } else {
                Err("not_ready".into())
            }
        });
        done_tx.send(result).unwrap();
    });
    ready_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert!(matches!(
        done_rx.recv_timeout(std::time::Duration::from_millis(100)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));
    pending.store(false, Ordering::Release);
    drop(transition);
    assert_eq!(
        done_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .err()
            .as_deref(),
        Some("not_ready")
    );
    worker.join().unwrap();
    // Actual scope key waits must not extend the request's wall-clock expiry.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let short_args = serde_json::json!({"expiresAt":now+500})
        .as_object()
        .unwrap()
        .clone();
    let held_keys = state.keys.lock().unwrap();
    let worker_handle = handle.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let worker_state = worker_handle.state::<AppState>();
        ready_tx.send(()).unwrap();
        done_tx
            .send(current_scope(&worker_state, ATTACH, &short_args))
            .unwrap();
    });
    ready_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert!(matches!(
        done_rx.recv_timeout(std::time::Duration::from_millis(600)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));
    drop(held_keys);
    assert_eq!(
        done_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .err()
            .as_deref(),
        Some("invalid_request")
    );
    worker.join().unwrap();
    // Ledger validation itself can wait too: the final scope/expiry check must
    // follow it, not precede it. No real identity or relay transport is used.
    let invalidated = std::sync::atomic::AtomicBool::new(false);
    let pending_checks = std::sync::atomic::AtomicUsize::new(0);
    let result = check_attachment_in(
        &handle,
        &|| {
            if invalidated.load(Ordering::Acquire) {
                Err("invalid_request".into())
            } else {
                Ok(())
            }
        },
        &|| {
            if pending_checks.fetch_add(1, Ordering::SeqCst) == 1 {
                invalidated.store(true, Ordering::Release);
            }
            Ok(())
        },
    );
    assert_eq!(result.err().as_deref(), Some("invalid_request"));
    // The reused normal membership body must authorize AFTER signing-key wait,
    // before even a loopback publication. This uses only fresh synthetic keys.
    *state.relay_url_override.lock().unwrap() = Some("ws://127.0.0.1:9".into());
    let held_keys = state.keys.lock().unwrap();
    let member_handle = handle.clone();
    let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
    let member = std::thread::spawn(move || {
        let member_state = member_handle.state::<AppState>();
        entered_tx.send(()).unwrap();
        tauri::async_runtime::block_on(crate::commands::add_channel_members_guarded(
            core::CHANNEL.into(),
            vec![core::TARGET.into()],
            Some("bot".into()),
            Some("ws://127.0.0.1:9".into()),
            None,
            &member_state,
            &|| Err("invalid_request".into()),
        ))
    });
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    drop(held_keys);
    assert_eq!(
        member.join().unwrap().err().as_deref(),
        Some("invalid_request")
    );
    assert!(
        !Path::new(core::SETUP_DIR).exists(),
        "fixture must never mount/access the real setup artifacts"
    );
}

#[test]
fn claude_initial_inspection_real_creator_projection_and_waits_are_confined() {
    const CHILD: &str = "BUZZ_CLAUDE_INITIAL_INSPECT_FIXTURE";
    if let Some(home) = std::env::var_os(CHILD) {
        return confined_initial_inspection(Path::new(&home));
    }
    let home = tempfile::tempdir().unwrap();
    let mut child=std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","commands::agents::owner_test_claude_lifecycle::tests::claude_initial_inspection_real_creator_projection_and_waits_are_confined","--nocapture"])
        .env(CHILD,home.path()).env("HOME",home.path()).env("XDG_DATA_HOME",home.path())
        .env("APPDATA",home.path()).env("LOCALAPPDATA",home.path())
        .env_remove("BUZZ_PRIVATE_KEY").env_remove("BUZZ_AUTH_TAG").env_remove("BUZZ_RELAY_URL")
        .env_remove("BUZZ_NETWORK_TRACE").stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while child.try_wait().unwrap().is_none() {
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("bounded initial-inspection production wrapper fixture timed out");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "fixture failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn confined_initial_inspection(home: &Path) {
    use crate::managed_agents::{
        load_managed_agents, save_managed_agents, storage::NO_KEYCHAIN_FOR_TEST,
    };
    NO_KEYCHAIN_FOR_TEST.store(true, Ordering::SeqCst);
    crate::managed_agents::pin_nest_dir_for_test(home.join("nest"));
    let data = home.join("app-data");
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().identifier = data.to_str().unwrap().to_owned();
    let mut state = crate::app_state::build_app_state();
    *state.relay_url_override.lock().unwrap() = Some(core::RELAY.into());
    // Exact creator relay/tuple, but all HTTP is routed to owned loopback refusal.
    state.http_client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all("http://127.0.0.1:9").unwrap())
        .timeout(std::time::Duration::from_millis(100))
        .build()
        .unwrap();
    let app = tauri::test::mock_builder()
        .manage(state)
        .build(context)
        .unwrap();
    let handle = app.handle().clone();
    let state = handle.state::<AppState>();
    let empty = inspect_initial_in(&handle, &|| Ok(())).unwrap();
    assert_eq!(empty["recordState"], "missing");
    assert!(empty["checks"].is_null());
    let input=serde_json::from_value(serde_json::json!({"name":"Rowvia Developer Claude",
        "relayUrl":core::RELAY,"agentCommand":core::PYTHON,"personaId":null,"teamId":null,
        "agentArgs":["-IS",core::RUN],"envVars":{},"spawnAfterCreate":false,"startOnAppLaunch":false})).unwrap();
    let response = tauri::async_runtime::block_on(super::super::create_managed_agent_in(
        input,
        handle.clone(),
        &state,
    ))
    .unwrap();
    let created = response.agent.pubkey.clone();
    drop(response);
    let mut original = load_managed_agents(&handle)
        .unwrap()
        .into_iter()
        .find(|r| r.pubkey == created)
        .unwrap();
    // Only public-key substitution: exact real creator metadata, NOT signature proof.
    original.pubkey = core::TARGET.into();
    save_managed_agents(&handle, std::slice::from_ref(&original)).unwrap();
    let snapshot = inspect_initial_in(&handle, &|| Ok(())).unwrap();
    assert_eq!(
        snapshot.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "checks",
            "contractMatches",
            "noScalarPid",
            "noTargetReceipts",
            "noTrackedPairs",
            "pubkey",
            "receiptsReadable",
            "recordState",
            "stoppedInitialMatches"
        ]
    );
    assert_eq!(snapshot["recordState"], "unique");
    assert_eq!(
        snapshot["contractMatches"], true,
        "real creator closed projection: {snapshot:?}"
    );
    assert_eq!(snapshot["stoppedInitialMatches"], true);
    assert_eq!(snapshot["checks"]["initialSettingsMatch"], true);
    assert_eq!(snapshot["checks"]["configuredNormalSettingsMatch"], false);
    assert_eq!(
        snapshot["checks"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "autostartOff",
            "bindingPresent",
            "configuredNormalSettingsMatch",
            "creatorSnapshotMatches",
            "defaultTransport",
            "initialSettingsMatch",
            "localBackend",
            "nameMatches",
            "normalCommandMatches",
            "personaIsSelf",
            "relayMatches",
            "runtimeUnset",
            "selfDefinitionUnique",
            "sourceVersionMatches",
            "teamNone",
            "unlinked"
        ]
    );
    let base = crate::managed_agents::managed_agents_base_dir(&handle).unwrap();
    assert_eq!(
        crate::migration::backfill_standalone_agents_in_dir_for_test(&base).unwrap(),
        1
    );
    let backfilled = load_managed_agents(&handle)
        .unwrap()
        .into_iter()
        .find(|row| row.pubkey == core::TARGET)
        .unwrap();
    assert_eq!(backfilled.persona_id.as_deref(), Some(core::TARGET));
    let backfilled_projection = inspect_initial_in(&handle, &|| Ok(())).unwrap();
    assert_eq!(
        backfilled_projection["contractMatches"], true,
        "the actual normal creator→startup-backfill snapshot must retain its fixed contract"
    );
    assert_eq!(backfilled_projection["stoppedInitialMatches"], true);
    let definitions = crate::managed_agents::storage::load_agent_definitions(&handle).unwrap();
    let manufactured = definitions
        .iter()
        .find(|d| d.slug.as_deref() == Some(core::TARGET))
        .unwrap()
        .clone();
    for field in [
        "foreign",
        "persona_case",
        "missing",
        "duplicate",
        "duplicate_case",
        "prompt",
        "model",
        "provider",
        "runtime",
        "env",
        "transport",
        "session",
        "respond_to",
        "parallelism",
        "team",
        "stale_hash",
        "provenance",
    ] {
        let mut row = backfilled.clone();
        let mut definition = manufactured.clone();
        match field {
            "foreign" => row.persona_id = Some("foreign".into()),
            "persona_case" => row.persona_id = Some(core::TARGET.to_uppercase()),
            "prompt" => definition.system_prompt = Some("nsec1_PRIVATE_CANARY".into()),
            "model" => definition.model = Some("PRIVATE_CANARY".into()),
            "provider" => definition.provider = Some("PRIVATE_CANARY".into()),
            "runtime" => definition.runtime = Some("PRIVATE_CANARY".into()),
            "env" => {
                definition
                    .env_vars
                    .insert("PRIVATE_TOKEN".into(), "nsec1_PRIVATE_CANARY".into());
            }
            "transport" => definition.acp_command = "/bin/sh".into(),
            "session" => {
                definition.session_policy = crate::managed_agents::AcpSessionPolicy::Thread
            }
            "respond_to" => definition.definition_respond_to = Some("all".into()),
            "parallelism" => definition.definition_parallelism = Some(1),
            "team" => row.team_id = Some("PRIVATE_CANARY".into()),
            "provenance" => definition.source_team = Some("PRIVATE_CANARY".into()),
            "stale_hash" => row.persona_source_version = Some("0".repeat(64)),
            _ => {}
        }
        // Even a recomputed matching hash cannot bless an injected definition.
        if !matches!(field, "stale_hash" | "foreign" | "persona_case") {
            let view = definition.to_definition_view().unwrap();
            row.persona_source_version =
                Some(crate::managed_agents::persona_events::persona_content_hash(
                    &crate::managed_agents::persona_events::persona_event_content(&view),
                ));
        }
        let mut changed: Vec<_> = definitions
            .iter()
            .filter(|d| d.slug.as_deref() != Some(core::TARGET))
            .cloned()
            .collect();
        if field != "missing" {
            changed.push(definition.clone());
        }
        if matches!(field, "duplicate" | "duplicate_case") {
            if field == "duplicate_case" {
                definition.slug = Some(core::TARGET.to_uppercase());
            }
            changed.push(definition);
        }
        crate::managed_agents::storage::save_agent_definitions(&handle, &changed).unwrap();
        save_managed_agents(&handle, &[row]).unwrap();
        let result = inspect_initial_in(&handle, &|| Ok(())).unwrap();
        assert_eq!(result["contractMatches"], false, "{field}");
        assert_eq!(result["stoppedInitialMatches"], false, "{field}");
        assert!(
            !serde_json::to_string(&result).unwrap().contains("CANARY"),
            "{field}"
        );
    }
    crate::managed_agents::storage::save_agent_definitions(&handle, &definitions).unwrap();
    for configured in [false, true] {
        let mut row = backfilled.clone();
        if configured {
            row.env_vars = core::configured_env();
            row.parallelism = 1;
        }
        save_managed_agents(&handle, &[row]).unwrap();
        let result = inspect_initial_in(&handle, &|| Ok(())).unwrap();
        for flag in [
            "personaIsSelf",
            "teamNone",
            "selfDefinitionUnique",
            "creatorSnapshotMatches",
            "sourceVersionMatches",
        ] {
            assert_eq!(result["checks"][flag], true, "{flag}");
        }
        assert_eq!(result["checks"]["unlinked"], false);
        assert_eq!(result["contractMatches"], true);
        check_attachment_in(&handle, &|| Ok(()), &|| Ok(())).unwrap();
    }
    save_managed_agents(&handle, std::slice::from_ref(&backfilled)).unwrap();
    for to_setup in [true, false] {
        let guard =
            |rows: &[ManagedAgentRecord],
             runtimes: &HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>| {
                check_target_with_initial(&handle, rows, runtimes, !to_setup, false, to_setup)
            };
        super::super::super::agent_models::update_managed_agent_local_guarded_in(
            serde_json::from_value(core::update(to_setup)).unwrap(),
            &handle,
            &state,
            (&guard, &|| Ok(())),
        )
        .unwrap();
        let rows = load_managed_agents(&handle).unwrap();
        check_target(
            &handle,
            &rows,
            &state.managed_agent_processes.lock().unwrap(),
            to_setup,
            false,
        )
        .unwrap();
        assert_eq!(rows[0].persona_id.as_deref(), Some(core::TARGET));
        assert_eq!(
            rows[0].persona_source_version,
            backfilled.persona_source_version
        );
        assert!(!rows[0].start_on_app_launch && rows[0].runtime_pid.is_none());
    }
    save_managed_agents(&handle, std::slice::from_ref(&original)).unwrap();
    for field in [
        "name",
        "backend",
        "transport",
        "persona",
        "team",
        "runtime",
        "binding",
        "autostart",
        "legacy_command",
        "override",
        "args",
        "env",
        "parallelism",
        "relay",
        "pubkey",
    ] {
        let mut changed = original.clone();
        match field {
            "name" => changed.name = "PRIVATE_CANARY".into(),
            "backend" => {
                changed.backend = BackendKind::Provider {
                    id: "inert".into(),
                    config: Value::Null,
                }
            }
            "transport" => changed.acp_command = "/bin/sh".into(),
            "persona" => changed.persona_id = Some("inert".into()),
            "team" => changed.team_id = Some("inert".into()),
            "runtime" => changed.runtime = Some("inert".into()),
            "binding" => changed.auth_tag = None,
            "autostart" => changed.start_on_app_launch = true,
            "legacy_command" => changed.agent_command = "nsec1_PRIVATE_CANARY".into(),
            "override" => changed.agent_command_override = None,
            "args" => changed.agent_args = vec!["nsec1_PRIVATE_CANARY".into()],
            "env" => {
                changed
                    .env_vars
                    .insert("TOKEN".into(), "nsec1_PRIVATE_CANARY".into());
            }
            "parallelism" => {
                changed.parallelism = crate::managed_agents::DEFAULT_AGENT_PARALLELISM + 1
            }
            "relay" => changed.relay_url = "ws://127.0.0.1:9".into(),
            _ => changed.pubkey = "f".repeat(64),
        }
        let expected =
            target_contract_with_initial(std::slice::from_ref(&changed), &[], false, true).is_ok();
        assert!(
            !expected,
            "fixture mutation must actually fail the existing guard: {field}"
        );
        save_managed_agents(&handle, std::slice::from_ref(&changed)).unwrap();
        let result = inspect_initial_in(&handle, &|| Ok(())).unwrap();
        assert_eq!(result["contractMatches"], expected, "{field}");
        assert_eq!(result["stoppedInitialMatches"], false, "{field}");
        let public = serde_json::to_string(&result).unwrap();
        for private in [
            "CANARY",
            "nsec",
            core::PYTHON,
            core::RUN,
            "auth_tag",
            "private_key",
        ] {
            assert!(!public.contains(private), "{field}");
        }
    }
    let mut configured = original.clone();
    configured.env_vars = core::configured_env();
    configured.parallelism = 1;
    save_managed_agents(&handle, std::slice::from_ref(&configured)).unwrap();
    let result = inspect_initial_in(&handle, &|| Ok(())).unwrap();
    assert_eq!(result["contractMatches"], true);
    assert_eq!(result["checks"]["configuredNormalSettingsMatch"], true);
    save_managed_agents(&handle, &[original.clone(), original.clone()]).unwrap();
    let duplicate = inspect_initial_in(&handle, &|| Ok(())).unwrap();
    assert_eq!(duplicate["recordState"], "ambiguous");
    assert!(duplicate["checks"].is_null());
    save_managed_agents(&handle, std::slice::from_ref(&original)).unwrap();
    let receipt_dir = data.join("agents/agent-pids");
    fs::create_dir_all(&receipt_dir).unwrap();
    let receipt = receipt_dir.join(format!("{}.json", core::TARGET));
    fs::write(&receipt, b"PRIVATE_CANARY").unwrap();
    let result = inspect_initial_in(&handle, &|| Ok(())).unwrap();
    assert_eq!(result["noTargetReceipts"], false);
    assert_eq!(result["stoppedInitialMatches"], false);
    fs::remove_file(&receipt).unwrap();
    fs::remove_dir(&receipt_dir).unwrap();
    fs::write(&receipt_dir, b"owned non-directory fixture").unwrap();
    let result = inspect_initial_in(&handle, &|| Ok(())).unwrap();
    assert_eq!(result["receiptsReadable"], false);
    assert_eq!(result["noTargetReceipts"], false);
    fs::remove_file(&receipt_dir).unwrap();
    let workspace = state.workspace_apply_lock.try_lock().unwrap();
    assert_eq!(
        inspect_initial_in(&handle, &|| Ok(())).err().as_deref(),
        Some("not_ready")
    );
    drop(workspace);
    for transition in [false, true] {
        let held_transition =
            transition.then(|| state.managed_agent_runtime_transition.lock().unwrap());
        let held_store = (!transition).then(|| state.managed_agents_store_lock.lock().unwrap());
        let worker_handle = handle.clone();
        let denied = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_denied = denied.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            done_tx
                .send(inspect_initial_in(&worker_handle, &|| {
                    if worker_denied.load(Ordering::Acquire) {
                        Err("scope_mismatch".into())
                    } else {
                        Ok(())
                    }
                }))
                .unwrap();
        });
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(done_rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .is_err());
        denied.store(true, Ordering::Release);
        drop((held_transition, held_store));
        assert!(done_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap()
            .is_err());
        worker.join().unwrap();
    }
    for fail_at in 0..4 {
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let result = inspect_initial_in(&handle, &|| {
            if calls.fetch_add(1, Ordering::SeqCst) == fail_at {
                Err("invalid_request".into())
            } else {
                Ok(())
            }
        });
        assert_eq!(result.err().as_deref(), Some("invalid_request"));
    }
    assert!(
        state.managed_agent_processes.lock().unwrap().is_empty(),
        "normal creation/inspection must never spawn"
    );
}
