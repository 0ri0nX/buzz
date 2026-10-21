use super::*;
use std::{path::Path, sync::atomic::AtomicBool, time::Duration};

fn record() -> ManagedAgentRecord {
    serde_json::from_value(serde_json::json!({
        "pubkey":TARGET,"name":"Cerberus fixture","relay_url":RELAY,
        "acp_command":"buzz-acp","agent_command":"/usr/bin/python3.13",
        "agent_command_override":"/usr/bin/python3.13",
        "agent_args":["-IS","/home/orionx/project/RowviaContext/inert-launcher.py",
            "--config","/var/tmp/rowvia-management-pilot-v75-1000/public.toml",
            "--auth","SYNTHETIC_AUTH_PRIVATE_CANARY"],
        "mcp_command":"","turn_timeout_seconds":300,"start_on_app_launch":true,
        "private_key_nsec":"","auth_tag":"SYNTHETIC_AUTH_PRIVATE_CANARY",
        "env_vars":{"PRIVATE_TOKEN":"SYNTHETIC_ENV_PRIVATE_CANARY"},
        "created_at":"","updated_at":""
    }))
    .unwrap()
}

#[test]
fn cerberus_recovery_expiry_and_projection_are_closed() {
    let args = |value| serde_json::from_value(serde_json::json!({"expiresAt":value})).unwrap();
    assert!(validate_at(&args(1001), 1000).is_ok());
    assert!(validate_at(&Map::new(), 1000).is_err());
    for expiry in [0, 1000, 121001] {
        assert!(validate_at(&args(expiry), 1000).is_err());
    }
    let mut selectors = args(1001);
    selectors.insert("command".into(), Value::String("arbitrary".into()));
    assert!(validate_at(&selectors, 1000).is_err());
    for value in [
        serde_json::json!(true),
        serde_json::json!("1001"),
        Value::Null,
    ] {
        assert!(validate_at(
            &serde_json::from_value(serde_json::json!({"expiresAt":value})).unwrap(),
            1000
        )
        .is_err());
    }
    let public = "/home/orionx/project/RowviaContext";
    let args = vec![
        format!("{public}/launcher.py"),
        format!("{public}/not-a-config.json"),
        "--config".into(),
        format!("{public}/config.json"),
        format!("--profile={public}/profile.toml"),
        "--auth=nsec1_PRIVATE_CANARY".into(),
        format!("{public}/token_PRIVATE_CANARY.sh"),
        format!("{public}/../private.py"),
        format!("{public}//private.py"),
        "/home/orionx/.local/state/private/run.py".into(),
        "relative.py".into(),
        format!("{public}/$(secret).sh"),
    ];
    let launcher = launcher_projection("buzz-acp", "/usr/bin/python3.13", &args);
    assert_eq!(
        launcher,
        serde_json::json!({
            "outerProgram":"buzz-acp","agentProgram":"/usr/bin/python3.13",
            "sourcePaths":[format!("{public}/launcher.py")],
            "configPaths":[format!("{public}/config.json"),format!("{public}/profile.toml")]
        })
    );
    let serialized = serde_json::to_string(&public_result(42, launcher)).unwrap();
    for private in ["CANARY", "nsec", "--config", "env", "auth", "private_key"] {
        assert!(!serialized.contains(private));
    }
    let projection = launcher_projection("sh -c nsec1_CANARY", "unknown-secret", &[]);
    assert!(projection["outerProgram"].is_null() && projection["agentProgram"].is_null());
    let many: Vec<_> = (0..20).map(|n| format!("{public}/source{n}.mjs")).collect();
    assert_eq!(
        launcher_projection("node", "node", &many)["sourcePaths"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
    assert!(!safe_path(
        &format!("{public}/{}.py", "x".repeat(1024)),
        false
    ));
}

#[test]
fn cerberus_recovery_normal_stop_and_wait_guards_are_confined() {
    const CHILD: &str = "BUZZ_CERBERUS_RECOVERY_FIXTURE";
    if let Some(home) = std::env::var_os(CHILD) {
        return confined_stop(Path::new(&home));
    }
    let home = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","commands::agents::owner_test_cerberus_recovery::tests::cerberus_recovery_normal_stop_and_wait_guards_are_confined","--nocapture"])
        .env(CHILD,home.path()).env("HOME",home.path()).env("XDG_DATA_HOME",home.path())
        .env("APPDATA",home.path()).env("LOCALAPPDATA",home.path())
        .env_remove("BUZZ_PRIVATE_KEY").env_remove("BUZZ_AUTH_TAG").env_remove("BUZZ_RELAY_URL")
        .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while child.try_wait().unwrap().is_none() {
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("bounded production Stop fixture exceeded deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "confined fixture failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn confined_stop(home: &Path) {
    use crate::managed_agents::{
        load_managed_agents, save_managed_agents, storage::NO_KEYCHAIN_FOR_TEST,
    };
    use nostr::ToBech32;
    use std::{fs, os::unix::process::CommandExt};
    NO_KEYCHAIN_FOR_TEST.store(true, Ordering::SeqCst);
    crate::managed_agents::pin_nest_dir_for_test(home.join("nest"));
    let data = home.join("app-data");
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().identifier = data.to_str().unwrap().to_owned();
    let app = tauri::test::mock_builder()
        .manage(crate::app_state::build_app_state())
        .build(context)
        .unwrap();
    let handle = app.handle().clone();
    let state = handle.state::<AppState>();
    *state.relay_url_override.lock().unwrap() = Some(RELAY.into());
    let mut original = record();
    original.private_key_nsec = nostr::Keys::generate().secret_key().to_bech32().unwrap();
    save_managed_agents(&handle, std::slice::from_ref(&original)).unwrap();
    drop(load_managed_agents(&handle).unwrap());
    assert!(
        execute_in(&handle, &|| Ok(())).is_err(),
        "missing runtime must not become stopped proof"
    );
    let workspace = state.workspace_apply_lock.try_lock().unwrap();
    assert_eq!(
        execute_in(&handle, &|| Ok(())).err().as_deref(),
        Some("not_ready")
    );
    drop(workspace);
    // The production scope checker must reject expiry AFTER its actual key wait.
    let args = serde_json::json!({"expiresAt":now().unwrap()+200})
        .as_object()
        .unwrap()
        .clone();
    let held_keys = state.keys.lock().unwrap();
    let worker_handle = handle.clone();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        ready_tx.send(()).unwrap();
        done_tx
            .send(current_scope(&worker_handle.state::<AppState>(), &args))
            .unwrap();
    });
    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(done_rx.recv_timeout(Duration::from_millis(300)).is_err());
    drop(held_keys);
    assert_eq!(
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .err()
            .as_deref(),
        Some("invalid_request")
    );
    worker.join().unwrap();

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
    let runtime = crate::managed_agents::ManagedAgentProcess {
        child,
        log_path: home.join("inert-stop.log"),
        spawn_config: crate::managed_agents::spawn_snapshot::prospective_spawn_config_snapshot(
            &original,
            &[],
            &[],
            RELAY,
            &Default::default(),
            false,
        ),
        setup_mode: false,
        adapter_availability: None,
        start_nonce: "owned-recovery-fixture".into(),
    };
    let key = ManagedAgentRuntimeKey::new(TARGET, RELAY).unwrap();
    state
        .managed_agent_processes
        .lock()
        .unwrap()
        .insert(key.clone(), ManagedAgentPairRuntime::starting(runtime));
    assert!(check_target(
        &[original.clone(), original.clone()],
        &state.managed_agent_processes.lock().unwrap()
    )
    .is_err());
    let mut mismatch = original.clone();
    mismatch.runtime_pid = Some(pid + 1);
    assert!(check_target(&[mismatch], &state.managed_agent_processes.lock().unwrap()).is_err());
    let mut remote = original.clone();
    remote.backend = BackendKind::Provider {
        id: "inert".into(),
        config: Value::Null,
    };
    assert!(check_target(&[remote], &state.managed_agent_processes.lock().unwrap()).is_err());
    {
        let mut pairs = state.managed_agent_processes.lock().unwrap();
        let runtime = pairs.remove(&key).unwrap();
        let wrong = ManagedAgentRuntimeKey::new(TARGET, "ws://127.0.0.1:9").unwrap();
        pairs.insert(wrong.clone(), runtime);
        assert!(check_target(std::slice::from_ref(&original), &pairs).is_err());
        let runtime = pairs.remove(&wrong).unwrap();
        pairs.insert(key.clone(), runtime);
    }

    // Actual transition/store waits precede authorization, never termination.
    for (transition, error) in [
        (false, "scope_mismatch"),
        (true, "invalid_request"),
        (false, "not_ready"),
    ] {
        let held_transition =
            transition.then(|| state.managed_agent_runtime_transition.lock().unwrap());
        let held_store = (!transition).then(|| state.managed_agents_store_lock.lock().unwrap());
        let worker_handle = handle.clone();
        let denied = std::sync::Arc::new(AtomicBool::new(false));
        let worker_denied = denied.clone();
        let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
        let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            done_tx
                .send(execute_in(&worker_handle, &|| {
                    if worker_denied.load(Ordering::Acquire) {
                        Err(error.into())
                    } else {
                        Ok(())
                    }
                }))
                .unwrap();
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(done_rx.recv_timeout(Duration::from_millis(100)).is_err());
        denied.store(true, Ordering::Release);
        drop((held_transition, held_store));
        assert!(done_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .is_err());
        worker.join().unwrap();
        assert!(Path::new(&format!("/proc/{pid}")).exists());
    }
    // Optional current-config projection must not block an authorized normal
    // Stop of the exact tracked child when its saved harness is unavailable.
    let mut dangling = original.clone();
    dangling.agent_command_override = None;
    dangling.runtime = Some("rowvia-inert-missing-recovery-harness".into());
    save_managed_agents(&handle, std::slice::from_ref(&dangling)).unwrap();
    assert!(crate::managed_agents::resolve_effective_harness_descriptor(
        &dangling,
        &[],
        &Default::default()
    )
    .is_err());
    let result = execute_in(&handle, &|| Ok(()))
        .expect("unavailable optional launcher projection must not prevent normal Stop");
    assert_eq!(result["trackedChildPid"], pid);
    assert_eq!(result["trackedChildSettled"], true);
    assert_eq!(result["launcherBasis"], "current_saved_effective_config");
    assert_eq!(
        result["launcher"],
        serde_json::json!({
            "outerProgram":null,"agentProgram":null,"sourcePaths":[],"configPaths":[]
        })
    );
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    assert!(state.managed_agent_processes.lock().unwrap().is_empty());
    let mut saved = load_managed_agents(&handle)
        .unwrap()
        .into_iter()
        .find(|r| r.pubkey == TARGET)
        .unwrap();
    saved.runtime_pid = original.runtime_pid;
    saved.updated_at = original.updated_at.clone();
    saved.last_stopped_at = original.last_stopped_at.clone();
    saved.last_exit_code = original.last_exit_code;
    assert_eq!(
        serde_json::to_value(saved).unwrap(),
        serde_json::to_value(dangling).unwrap()
    );
    let bytes = serde_json::to_string(&result).unwrap();
    for private in ["CANARY", "nsec", "env", "auth", "private_key"] {
        assert!(!bytes.contains(private));
    }
    assert_eq!(result.len(), 7);
    assert!(fs::read_dir(&data).is_ok());

    // A pair that exits between initial guard and normal synchronization must
    // not silently become a successful missing-pair/legacy Stop.
    let mut exited = std::process::Command::new("/usr/bin/true")
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .unwrap();
    exited.wait().unwrap();
    let original = load_managed_agents(&handle)
        .unwrap()
        .into_iter()
        .find(|r| r.pubkey == TARGET)
        .unwrap();
    let runtime = crate::managed_agents::ManagedAgentProcess {
        child: exited,
        log_path: home.join("already-exited.log"),
        spawn_config: crate::managed_agents::spawn_snapshot::prospective_spawn_config_snapshot(
            &original,
            &[],
            &[],
            RELAY,
            &Default::default(),
            false,
        ),
        setup_mode: false,
        adapter_availability: None,
        start_nonce: "exited-recovery-fixture".into(),
    };
    state
        .managed_agent_processes
        .lock()
        .unwrap()
        .insert(key.clone(), ManagedAgentPairRuntime::starting(runtime));
    assert!(
        execute_in(&handle, &|| Ok(())).is_err(),
        "post-sync missing pair must fail closed"
    );
    assert!(state.managed_agent_processes.lock().unwrap().is_empty());

    // Failure only AFTER the actual normal Stop remains Unknown, not a false
    // no-effect promise or a response carrying the raw diagnostic.
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
    let runtime = crate::managed_agents::ManagedAgentProcess {
        child,
        log_path: home.join("post-stop-unknown.log"),
        spawn_config: crate::managed_agents::spawn_snapshot::prospective_spawn_config_snapshot(
            &original,
            &[],
            &[],
            RELAY,
            &Default::default(),
            false,
        ),
        setup_mode: false,
        adapter_availability: None,
        start_nonce: "post-stop-unknown".into(),
    };
    state
        .managed_agent_processes
        .lock()
        .unwrap()
        .insert(key, ManagedAgentPairRuntime::starting(runtime));
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let result = execute_in(&handle, &|| {
        if calls.fetch_add(1, Ordering::SeqCst) == 7 {
            Err("nsec1_PRIVATE_RAW_ERROR_CANARY".into())
        } else {
            Ok(())
        }
    });
    assert_eq!(result.err().as_deref(), Some("operation_failed"));
    assert_eq!(calls.load(Ordering::SeqCst), 8);
    assert!(!Path::new(&format!("/proc/{pid}")).exists());
    assert!(state.managed_agent_processes.lock().unwrap().is_empty());
}
