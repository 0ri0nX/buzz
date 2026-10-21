use super::*;

#[test]
fn fixed_claude_real_create_confines_stops_redacts_and_refuses_repeats() {
    const CHILD: &str = "BUZZ_FIXED_CLAUDE_TEST_HOME";
    if let Some(home) = std::env::var_os(CHILD) {
        return confined_create(std::path::Path::new(&home));
    }
    let home = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "commands::agents::owner_test_claude_create::tests::fixed_claude_real_create_confines_stops_redacts_and_refuses_repeats", "--nocapture"])
        .env(CHILD, home.path()).env("HOME", home.path())
        .env("XDG_DATA_HOME", home.path()).env("APPDATA", home.path())
        .env("LOCALAPPDATA", home.path())
        .env_remove("BUZZ_PRIVATE_KEY").env_remove("BUZZ_AUTH_TAG")
        .env_remove("BUZZ_RELAY_URL").env_remove("BUZZ_NETWORK_TRACE")
        .output().unwrap();
    assert!(
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "fixture failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn confined_create(home: &Path) {
    use crate::managed_agents::{load_managed_agents, storage::NO_KEYCHAIN_FOR_TEST};
    NO_KEYCHAIN_FOR_TEST.store(true, Ordering::SeqCst);
    crate::managed_agents::pin_nest_dir_for_test(home.join("nest"));
    let data = home.join("app-data");
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().identifier = data.to_str().unwrap().to_owned();
    let state = crate::app_state::build_app_state();
    const REFUSED_RELAY: &str = "ws://127.0.0.1:9";
    *state.relay_url_override.lock().unwrap() = Some(REFUSED_RELAY.into());
    let app = tauri::test::mock_builder()
        .manage(state)
        .build(context)
        .unwrap();
    let state = app.state::<AppState>();
    let input = || {
        let mut input: CreateManagedAgentRequest = serde_json::from_value(core::request()).unwrap();
        // Only this private fixture redirects normal profile publication to an
        // owned refusal endpoint; production uses the compiled owner/relay guard.
        input.relay_url = Some(REFUSED_RELAY.into());
        input
    };
    for error in ["scope_mismatch", "not_ready", "invalid_request"] {
        let result = tauri::async_runtime::block_on(super::super::create_managed_agent_guarded_in(
            input(),
            app.handle().clone(),
            &state,
            |rows| prepare_create(app.handle(), rows, || Err(error.into())),
        ));
        assert_eq!(result.err().as_deref(), Some(error));
        assert!(load_managed_agents(app.handle()).unwrap().is_empty());
        assert!(!data.join("custom_harnesses").exists());
    }
    // A request can lose its scope or expire while waiting for the catalog
    // lock. Neither a saved row nor even the inert harness prefix is then
    // authorized. Hold the real persist lock until the first scope check ran.
    for error in ["scope_mismatch", "not_ready", "invalid_request"] {
        let persist = custom_harnesses::persist_mutex().lock().unwrap();
        let drifted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let changed = drifted.clone();
        let handle = app.handle().clone();
        let (checked_tx, checked_rx) = std::sync::mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            let state = handle.state::<AppState>();
            let mut input: CreateManagedAgentRequest =
                serde_json::from_value(core::request()).unwrap();
            input.relay_url = Some(REFUSED_RELAY.into());
            tauri::async_runtime::block_on(super::super::create_managed_agent_guarded_in(
                input,
                handle.clone(),
                &state,
                |rows| {
                    prepare_create(&handle, rows, || {
                        if changed.load(Ordering::Acquire) {
                            Err(error.into())
                        } else {
                            let _ = checked_tx.try_send(());
                            Ok(())
                        }
                    })
                },
            ))
        });
        let checked = checked_rx.recv_timeout(std::time::Duration::from_secs(5));
        drifted.store(true, Ordering::Release);
        drop(persist);
        let result = worker.join().unwrap();
        assert!(
            checked.is_ok(),
            "real create must reach scope before catalog lock"
        );
        assert_eq!(result.err().as_deref(), Some(error));
        assert!(
            !data.join("custom_harnesses").exists(),
            "catalog mutation after scope/expiry drift while waiting for persist lock"
        );
        assert!(load_managed_agents(app.handle()).unwrap().is_empty());
    }
    fs::create_dir_all(data.join("custom_harnesses")).unwrap();
    for filename in [format!("{}.json", core::HARNESS_ID), "shadow.json".into()] {
        let path = data.join("custom_harnesses").join(filename);
        let original = serde_json::to_vec(&core::harness()).unwrap();
        fs::write(&path, &original).unwrap();
        assert_eq!(
            install_harness(path.parent().unwrap(), &|| Ok(()))
                .err()
                .as_deref(),
            Some("harness_collision")
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            original,
            "never overwrite/adopt a matching file or shadow ID"
        );
        fs::remove_file(path).unwrap();
    }
    // A scope/lock/expiry change during catalog I/O must still prevent the
    // saved-record commit. The inert fixture scope changes on its third read,
    // after the under-persist pre-write check and catalog registration.
    for error in ["scope_mismatch", "not_ready", "invalid_request"] {
        let checks = std::sync::atomic::AtomicUsize::new(0);
        let result = tauri::async_runtime::block_on(super::super::create_managed_agent_guarded_in(
            input(),
            app.handle().clone(),
            &state,
            |rows| {
                prepare_create(app.handle(), rows, || {
                    if checks.fetch_add(1, Ordering::SeqCst) < 2 {
                        Ok(())
                    } else {
                        Err(error.into())
                    }
                })
            },
        ));
        assert_eq!(result.err().as_deref(), Some(error));
        assert!(load_managed_agents(app.handle()).unwrap().is_empty());
        custom_harnesses::delete_and_warm(&data.join("custom_harnesses"), core::HARNESS_ID)
            .unwrap();
    }
    // A catalog executable records *any* invocation, including --version or
    // auth/install probes. Warming during stopped creation must only read JSON.
    use std::os::unix::fs::PermissionsExt;
    let sentinel = home.join("catalog-invocation-sentinel");
    let invoked = home.join("catalog-command-was-invoked");
    fs::write(
        &sentinel,
        format!("#!/bin/sh\n: > \"{}\"\nexit 0\n", invoked.display()),
    )
    .unwrap();
    fs::set_permissions(&sentinel, fs::Permissions::from_mode(0o700)).unwrap();
    fs::create_dir_all(data.join("custom_harnesses")).unwrap();
    fs::write(
        data.join("custom_harnesses/sentinel.json"),
        serde_json::to_vec(&serde_json::json!({
            "id":"owner-test-no-probe", "label":"Inert fixture", "command":sentinel,
            "args":[],"env":{},"installInstructionsUrl":"","installHint":""
        }))
        .unwrap(),
    )
    .unwrap();
    // Competing real creates share the production final store lock. Exactly
    // one persists; the loser must see the target before installing a harness.
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let tasks: Vec<_> = (0..2)
        .map(|_| {
            let app = app.handle().clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let state = app.state::<AppState>();
                let mut input: CreateManagedAgentRequest =
                    serde_json::from_value(core::request()).unwrap();
                input.relay_url = Some(REFUSED_RELAY.into());
                barrier.wait();
                tauri::async_runtime::block_on(super::super::create_managed_agent_guarded_in(
                    input,
                    app.clone(),
                    &state,
                    |rows| {
                        prepare_create(&app, rows, || {
                            core::check_scope(core::OWNER, core::RELAY, true).map_err(str::to_owned)
                        })
                    },
                ))
            })
        })
        .collect();
    let results: Vec<_> = tasks.into_iter().map(|task| task.join().unwrap()).collect();
    assert_eq!(
        results
            .iter()
            .filter(|r| r.as_ref().err().is_some_and(|e| e == "target_collision"))
            .count(),
        1
    );
    let mut response = results.into_iter().find_map(Result::ok).unwrap();
    assert!(response.spawn_error.is_none());
    assert_eq!(response.agent.status, "stopped");
    assert!(!response.agent.start_on_app_launch);
    assert!(state.managed_agent_processes.lock().unwrap().is_empty());
    assert!(
        !invoked.exists(),
        "catalog warm must never execute/probe/install a harness"
    );
    assert!(custom_harnesses::lookup_loaded_harness_by_id("owner-test-no-probe").is_some());
    let records = load_managed_agents(app.handle()).unwrap();
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record.name, core::NAME);
    assert_eq!(record.agent_command, core::PYTHON);
    assert_eq!(record.agent_command_override.as_deref(), Some(core::PYTHON));
    assert!(
        record.runtime.is_none(),
        "normal create pins executable/args without a runtime-ID field"
    );
    let descriptor = crate::managed_agents::resolve_effective_harness_descriptor(
        record,
        &[],
        &Default::default(),
    )
    .unwrap();
    assert_eq!(descriptor.command, core::PYTHON);
    assert_eq!(descriptor.args, ["-IS", core::RUN]);
    assert!(record.persona_id.is_none());
    assert!(record.provider.is_none() && record.model.is_none());
    assert!(!record.start_on_app_launch);
    assert!(record.last_started_at.is_none() && record.runtime_pid.is_none());
    let mut renamed = record.clone();
    renamed.name = "Renamed existing Claude".into();
    assert_eq!(
        prepare_create(app.handle(), &[renamed], || Ok(()))
            .err()
            .as_deref(),
        Some("target_collision")
    );
    let harness = custom_harnesses::lookup_loaded_harness_by_id(core::HARNESS_ID).unwrap();
    assert_eq!(
        serde_json::to_value(harness.as_ref()).unwrap(),
        core::harness()
    );
    assert_eq!(harness.command, core::PYTHON);
    assert_eq!(harness.args, ["-IS", core::RUN]);

    const CANARY: &str = "nsec1_PRIVATE_KEY_RAW_ERROR_AUTH_CANARY";
    response.private_key_nsec = CANARY.into();
    response.profile_sync_error = Some(CANARY.into());
    response.spawn_error = Some(CANARY.into());
    response.agent.last_error = Some(CANARY.into());
    response
        .agent
        .env_vars
        .insert("SECRET".into(), CANARY.into());
    let public = public_projection(response);
    let encoded = serde_json::to_string(&public).unwrap();
    assert!(
        !encoded.contains(CANARY)
            && !encoded.contains("nsec")
            && !encoded.contains("error")
            && !encoded.contains("SECRET")
    );
    let mut keys = public.keys().map(String::as_str).collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "createdStopped",
            "name",
            "profileSyncSucceeded",
            "pubkey",
            "startOnAppLaunch"
        ]
    );

    let harness_path = data
        .join("custom_harnesses")
        .join(format!("{}.json", core::HARNESS_ID));
    let before = fs::read(&harness_path).unwrap();
    let result = tauri::async_runtime::block_on(super::super::create_managed_agent_guarded_in(
        input(),
        app.handle().clone(),
        &state,
        |rows| prepare_create(app.handle(), rows, || Ok(())),
    ));
    assert_eq!(result.err().as_deref(), Some("target_collision"));
    assert_eq!(load_managed_agents(app.handle()).unwrap().len(), 1);
    assert_eq!(fs::read(&harness_path).unwrap(), before);
    // No in-memory ledger is involved in the second call: disk owns refusal.
    assert!(install_harness(harness_path.parent().unwrap(), &|| Ok(())).is_err());

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let args = serde_json::json!({"expiresAt": now + 60_000})
        .as_object()
        .unwrap()
        .clone();
    assert_eq!(execute(app.handle(), &args).unwrap_err(), "scope_mismatch");
    state.keyring_locked.store(true, Ordering::Release);
    assert_eq!(execute(app.handle(), &args).unwrap_err(), "not_ready");
}
