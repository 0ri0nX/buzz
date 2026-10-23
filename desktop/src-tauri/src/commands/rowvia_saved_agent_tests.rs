use super::*;
use std::{fs, path::Path, sync::atomic::Ordering};

const RELAY: &str = "ws://127.0.0.1:9";

#[test]
fn quiet_preparation_real_save_is_stopped_private_and_scope_bound() {
    const CHILD: &str = "BUZZ_ROWVIA_PREPARATION_TEST_HOME";
    if let Some(home) = std::env::var_os(CHILD) {
        return confined_preparation(Path::new(&home));
    }
    let home = tempfile::Builder::new()
        .prefix("rowvia-b002a-native-")
        .tempdir_in(std::env::temp_dir())
        .unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "commands::agents::rowvia_saved_agent::tests::quiet_preparation_real_save_is_stopped_private_and_scope_bound", "--nocapture"])
        .env(CHILD, home.path()).env("HOME", home.path()).env("XDG_DATA_HOME", home.path())
        .env("APPDATA", home.path()).env("LOCALAPPDATA", home.path())
        .env_remove("BUZZ_PRIVATE_KEY").env_remove("BUZZ_AUTH_TAG")
        .env_remove("BUZZ_RELAY_URL").env_remove("BUZZ_NETWORK_TRACE")
        .output().unwrap();
    assert!(
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "confined synthetic preparation failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn selection(home: &Path, owner: String) -> RowviaSavedAgentPreparation {
    let operation_id = uuid::Uuid::now_v7().to_string();
    RowviaSavedAgentPreparation {
        operation_id: operation_id.clone(),
        owner_pubkey: owner,
        relay_url: RELAY.into(),
        source_instance: "synthetic-native".into(),
        channel_id: uuid::Uuid::new_v4().to_string(),
        name: "Prepared synthetic agent".into(),
        system_prompt: "Synthetic instructions.".into(),
        recipe: SavedAgentRecipe::CodexLocalV1,
        model: "synthetic-model".into(),
        workspace: home
            .join("workspaces")
            .join(&operation_id)
            .to_str()
            .unwrap()
            .into(),
        capsule_run: home
            .join("capsules")
            .join(operation_id)
            .join("buzz-worker/run")
            .to_str()
            .unwrap()
            .into(),
    }
}

fn confined_preparation(home: &Path) {
    storage::NO_KEYCHAIN_FOR_TEST.store(true, Ordering::SeqCst);
    pin_nest_dir_for_test(home.join("nest"));
    let app_data = home.join("app-data");
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().identifier = app_data.to_str().unwrap().into();
    let state = crate::app_state::build_app_state();
    *state.relay_url_override.lock().unwrap() = Some(RELAY.into());
    let app = tauri::test::mock_builder()
        .manage(state)
        .build(context)
        .unwrap();
    assert_eq!(app.path().app_data_dir().unwrap(), app_data);
    let state = app.state::<AppState>();
    let input = selection(home, state.signing_keys().unwrap().public_key().to_hex());

    let mut wrong = input.clone();
    wrong.channel_id = "01940000-0000-7000-f000-000000000011".into();
    assert!(prepare_rowvia_saved_agent(app.handle(), wrong).is_err());

    let mut wrong = input.clone();
    wrong.owner_pubkey = "a".repeat(64);
    assert!(prepare_rowvia_saved_agent(app.handle(), wrong).is_err());
    let mut wrong = input.clone();
    wrong.relay_url = "ws://127.0.0.1:10".into();
    assert!(prepare_rowvia_saved_agent(app.handle(), wrong).is_err());
    state.keyring_locked.store(true, Ordering::SeqCst);
    assert!(prepare_rowvia_saved_agent(app.handle(), input.clone()).is_err());
    state.keyring_locked.store(false, Ordering::SeqCst);
    assert!(!app_data.join("agents").exists());

    let public = prepare_rowvia_saved_agent(app.handle(), input.clone()).unwrap();
    let rows = storage::load_unhydrated_managed_agents(app.handle()).unwrap();
    assert_eq!(rows.len(), 1);
    let record = &rows[0];
    let pending = record.pending_saved_registration.as_ref().unwrap();
    assert_eq!(
        public.saved_authorization_sha256,
        digest(record.auth_tag.as_deref().unwrap().as_bytes())
    );
    assert_eq!(public.pubkey, record.pubkey);
    assert!(public.created_stopped && !public.bindings_ready && !public.runtime_ready);
    assert!(record.persona_id.is_none() && record.team_id.is_none());
    assert!(!record.start_on_app_launch && !record.auto_restart_on_config_change);
    assert!(record.runtime_pid.is_none() && record.last_started_at.is_none());
    assert_eq!(record.parallelism, 1);
    assert_eq!(record.backend, BackendKind::Local);
    assert_eq!(
        record.env_vars,
        BTreeMap::from([("BUZZ_ACP_NO_BASE_PROMPT".into(), "true".into())])
    );
    assert_eq!(
        record.agent_args,
        vec!["-IS".to_owned(), input.capsule_run.clone()]
    );
    assert_eq!(record.respond_to, RespondTo::OwnerOnly);
    assert!(record.model.is_none() && record.provider.is_none());
    let serialized = serde_json::to_string(&public).unwrap();
    assert!(!record.private_key_nsec.is_empty());
    assert!(!serialized.contains(&record.private_key_nsec));
    assert!(!serialized.contains(record.auth_tag.as_deref().unwrap()));
    assert!(!serialized.contains("privateKey") && !serialized.contains("authTag"));
    assert!(
        !home.join("nest").exists()
            && !home.join("workspaces").exists()
            && !home.join("capsules").exists()
    );
    assert_eq!(fs::read_dir(app_data.join("agents")).unwrap().count(), 1);
    let store = storage::managed_agents_store_path(app.handle()).unwrap();
    let saved = fs::read(&store).unwrap();
    assert!(prepare_rowvia_saved_agent(app.handle(), input.clone()).is_err());
    let duplicate_name = selection(home, input.owner_pubkey.clone());
    assert_ne!(duplicate_name.operation_id, input.operation_id);
    validate(&duplicate_name).unwrap();
    assert!(prepare_rowvia_saved_agent(app.handle(), duplicate_name).is_err());
    assert_eq!(fs::read(&store).unwrap(), saved);

    // Background reconciliation and unrelated Nest regeneration must not turn
    // a quiet local save into an owner-signed identity or shared roster entry.
    assert!(crate::managed_agents::agent_events::build_agent_event(record).is_err());
    let conn =
        crate::managed_agents::retention::open_retention_db(&home.join("synthetic-retention.db"))
            .unwrap();
    let owner_keys = state.signing_keys().unwrap();
    assert!(
        !crate::managed_agents::reconcile::retain_agent_record(&conn, &owner_keys, record).unwrap()
    );
    assert!(crate::managed_agents::retention::get_pending_sync(&conn)
        .unwrap()
        .is_empty());
    let roster =
        crate::managed_agents::render_dynamic_section(&[], &rows, &Default::default(), RELAY);
    assert!(!roster.contains(&record.name) && !roster.contains(&record.pubkey));
    let profile = super::super::profile::profile_reconcile_data(record, &[]);
    let result = tauri::async_runtime::block_on(super::super::profile::reconcile_agent_profile(
        &state,
        app.handle(),
        &record.pubkey,
        &profile,
    ));
    assert!(result
        .err()
        .is_some_and(|error| error.contains("registration is pending")));

    let result = tauri::async_runtime::block_on(super::super::start_local_agent_with_preflight(
        app.handle(),
        &state,
        &record.pubkey,
        false,
        None,
        None,
        None,
    ));
    assert!(result
        .err()
        .is_some_and(|error| error.contains("registration is pending")));
    assert_eq!(fs::read(&store).unwrap(), saved);
    assert_eq!(fs::read_dir(app_data.join("agents")).unwrap().count(), 1);
    assert!(!home.join("nest").exists());

    // The actual shared spawn boundary receives a valid synthetic key. If its
    // pending refusal is removed, only /usr/bin/true can run; always reap it.
    let admission = AdmissionSnapshot::capture(&state);
    let transition = state.managed_agent_runtime_transition.lock().unwrap();
    let admitted = transition.admit(&admission, RELAY).unwrap();
    let mut inert = record.clone();
    inert.acp_command = "/usr/bin/true".into();
    inert.agent_command = "/usr/bin/true".into();
    inert.agent_command_override = Some("/usr/bin/true".into());
    inert.agent_args.clear();
    let result = spawn_agent_child(
        app.handle(),
        &inert,
        RELAY,
        &admitted,
        false,
        Some(&input.owner_pubkey),
        None,
    );
    let error = match result {
        Err(error) => error,
        Ok(mut process) => {
            let _ = process.child.wait();
            panic!("pending preparation reached process spawn");
        }
    };
    assert!(error.contains("registration is pending"));
    drop(transition);
    assert_eq!(fs::read_dir(app_data.join("agents")).unwrap().count(), 1);
    assert_eq!(fs::read(&store).unwrap(), saved);

    for mutation in 0..6 {
        let mut changed = record.clone();
        match mutation {
            0 => changed.acp_command = "/usr/bin/false".into(),
            1 => changed.auth_tag.as_mut().unwrap().push(' '),
            2 => changed.system_prompt = Some("Changed after prepare".into()),
            3 => changed.pending_saved_registration.as_mut().unwrap().model = "changed".into(),
            4 => changed
                .pending_saved_registration
                .as_mut()
                .unwrap()
                .workspace
                .push_str("-changed"),
            _ => changed.persona_id = Some("borrowed-persona".into()),
        }
        assert!(project(&changed).is_err());
    }
    assert_eq!(pending.operation_id, input.operation_id);

    // These are the same production bodies unconditionally called by the
    // ordinary Tauri wrappers, not a test of the refusal helper in isolation.
    let update: crate::managed_agents::UpdateManagedAgentRequest = serde_json::from_value(
        serde_json::json!({"pubkey": public.pubkey.clone(), "name": "Unapproved rename"}),
    )
    .unwrap();
    require_pending_refusal(tauri::async_runtime::block_on(
        crate::commands::agent_models::update_managed_agent(
            update,
            app.handle().clone(),
            app.state::<AppState>(),
        ),
    ));
    assert_eq!(fs::read(&store).unwrap(), saved);
    require_pending_refusal(tauri::async_runtime::block_on(
        crate::commands::agent_settings::set_managed_agent_start_on_app_launch(
            public.pubkey.clone(),
            true,
            app.handle().clone(),
        ),
    ));
    require_pending_refusal(tauri::async_runtime::block_on(
        crate::commands::agent_settings::set_managed_agent_auto_restart(
            public.pubkey.clone(),
            true,
            app.handle().clone(),
        ),
    ));
    assert_eq!(fs::read(&store).unwrap(), saved);

    // Only this fresh synthetic probe fixture selects a fake ACP executable;
    // the genuine preparation bytes are restored before further assertions.
    let marker = home.join("unapproved-model-probe");
    let fake_acp = home.join("synthetic-acp-model-probe");
    fs::write(
        &fake_acp,
        format!(
            "#!/bin/sh\nprintf '%s' 'owned-synthetic-probe' > {:?}\nprintf '%s\\n' '{{}}'\n",
            marker.to_str().unwrap()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&fake_acp, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut probe_record = (*record).clone();
    probe_record.acp_command = fake_acp.to_str().unwrap().into();
    // Explicit synthetic selection prevents ambient provider inference.
    probe_record.provider = Some("synthetic-pending-canary".into());
    fs::write(&store, serde_json::to_vec_pretty(&[probe_record]).unwrap()).unwrap();
    let probe_saved = fs::read(&store).unwrap();
    let models = tauri::async_runtime::block_on(crate::commands::agent_models::get_agent_models(
        public.pubkey.clone(),
        app.handle().clone(),
        app.state::<AppState>(),
    ));
    assert!(!marker.exists(), "pending model command was executed");
    require_pending_refusal(models);
    assert_eq!(fs::read(&store).unwrap(), probe_saved);
    fs::write(&store, &saved).unwrap();
    assert_eq!(fs::read(&store).unwrap(), saved);

    for force in [None, Some(true)] {
        require_pending_refusal(tauri::async_runtime::block_on(
            super::super::delete_managed_agent(public.pubkey.clone(), force, app.handle().clone()),
        ));
        assert_eq!(fs::read(&store).unwrap(), saved);
        assert_eq!(fs::read_dir(app_data.join("agents")).unwrap().count(), 1);
        assert!(!home.join("nest").exists());
        assert!(crate::managed_agents::retention::get_pending_sync(&conn)
            .unwrap()
            .is_empty());
    }
}

fn require_pending_refusal<T>(result: Result<T, String>) {
    assert!(result
        .err()
        .is_some_and(|error| error.contains("registration is pending")));
}

#[test]
fn preparation_ack_capture_retain_revalidate_is_confined_and_durable() {
    const CHILD: &str = "BUZZ_ROWVIA_C2_TEST_HOME";
    if let Some(home) = std::env::var_os(CHILD) {
        return confined_preparation_ack(Path::new(&home));
    }
    let home = tempfile::Builder::new()
        .prefix("rowvia-b002c2-native-")
        .tempdir_in("/var/tmp")
        .unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "commands::agents::rowvia_saved_agent::tests::preparation_ack_capture_retain_revalidate_is_confined_and_durable", "--nocapture"])
        .env(CHILD, home.path()).env("HOME", home.path()).env("XDG_DATA_HOME", home.path())
        .env("APPDATA", home.path()).env("LOCALAPPDATA", home.path())
        .env_remove("BUZZ_PRIVATE_KEY").env_remove("BUZZ_AUTH_TAG")
        .env_remove("BUZZ_RELAY_URL").env_remove("BUZZ_NETWORK_TRACE")
        .output().unwrap();
    assert!(
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "confined C2 synthetic test failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn store_fingerprint(path: &Path) -> (Vec<u8>, std::time::SystemTime, u64) {
    let metadata = fs::metadata(path).unwrap();
    #[cfg(unix)]
    let inode = {
        use std::os::unix::fs::MetadataExt;
        metadata.ino()
    };
    #[cfg(not(unix))]
    let inode = metadata.len();
    (fs::read(path).unwrap(), metadata.modified().unwrap(), inode)
}

fn synthetic_ack(expected: &RowviaSavedAgentScopeExpectation) -> SavedAgentPreparationAck {
    SavedAgentPreparationAck {
        version: 1,
        operation_id: expected.selection.operation_id.clone(),
        agent_pubkey: expected.agent_pubkey.clone(),
        native_config_sha256: expected.native_config_sha256.clone(),
        saved_authorization_sha256: expected.saved_authorization_sha256.clone(),
        recipe_sha256: "1".repeat(64),
        request_sha256: "2".repeat(64),
        canonical21_sha256: "3".repeat(64),
        prepared_receipt_sha256: "4".repeat(64),
    }
}

fn confined_preparation_ack(home: &Path) {
    storage::NO_KEYCHAIN_FOR_TEST.store(true, Ordering::SeqCst);
    pin_nest_dir_for_test(home.join("nest"));
    let app_data = home.join("app-data");
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().identifier = app_data.to_str().unwrap().into();
    let state = crate::app_state::build_app_state();
    *state.relay_url_override.lock().unwrap() = Some(RELAY.into());
    let app = tauri::test::mock_builder()
        .manage(state)
        .build(context)
        .unwrap();
    assert_eq!(app.path().app_data_dir().unwrap(), app_data);
    let state = app.state::<AppState>();
    let input = selection(home, state.signing_keys().unwrap().public_key().to_hex());
    let mut expected = RowviaSavedAgentScopeExpectation {
        selection: input.clone(),
        agent_pubkey: "a".repeat(64),
        native_config_sha256: "b".repeat(64),
        saved_authorization_sha256: "c".repeat(64),
    };
    let ack = synthetic_ack(&expected);
    assert!(capture_rowvia_saved_agent_scope(app.handle(), &expected).is_err());
    assert!(retain_rowvia_saved_agent_preparation_ack(app.handle(), &expected, &ack).is_err());
    assert!(revalidate_rowvia_saved_agent_preparation_ack(app.handle(), &expected, &ack).is_err());
    assert!(
        !app_data.exists(),
        "readonly missing store created a directory"
    );
    // A configured path blocked by a regular file must not trigger mkdir/backup.
    fs::write(&app_data, b"synthetic path blocker").unwrap();
    assert!(capture_rowvia_saved_agent_scope(app.handle(), &expected).is_err());
    assert_eq!(fs::read(&app_data).unwrap(), b"synthetic path blocker");
    fs::remove_file(&app_data).unwrap();
    fs::create_dir_all(app_data.join("agents")).unwrap();
    let store = app_data.join("agents/managed-agents.json");
    fs::write(&store, b"malformed synthetic JSON").unwrap();
    let malformed = store_fingerprint(&store);
    assert!(capture_rowvia_saved_agent_scope(app.handle(), &expected).is_err());
    assert!(retain_rowvia_saved_agent_preparation_ack(app.handle(), &expected, &ack).is_err());
    assert_eq!(store_fingerprint(&store), malformed);
    assert_eq!(fs::read_dir(app_data.join("agents")).unwrap().count(), 1);
    // The ordinary loader's existing evidence backup remains a separate,
    // intentional side effect; the readonly path above never called it.
    assert!(storage::load_unhydrated_managed_agents(app.handle()).is_err());
    let invalid = store.with_extension("json.invalid");
    assert_eq!(fs::read(&invalid).unwrap(), malformed.0);
    fs::remove_file(invalid).unwrap();
    fs::remove_file(&store).unwrap();
    let public = prepare_rowvia_saved_agent(app.handle(), input.clone()).unwrap();
    expected.agent_pubkey = public.pubkey;
    expected.native_config_sha256 = public.native_config_sha256;
    expected.saved_authorization_sha256 = public.saved_authorization_sha256;
    let record = storage::load_agent_store_readonly(app.handle())
        .unwrap()
        .records
        .remove(0);
    let mut definition = record.clone();
    definition.pubkey.clear();
    definition.name = "Synthetic definition".into();
    definition.slug = Some("synthetic-definition".into());
    definition.pending_saved_registration = None;
    definition.private_key_nsec = "synthetic-inline-definition-placeholder".into();
    let mut unrelated = record.clone();
    unrelated.pubkey = "d".repeat(64);
    unrelated.name = "Synthetic unrelated instance".into();
    unrelated.pending_saved_registration = None;
    unrelated.private_key_nsec = "synthetic-inline-instance-placeholder".into();
    let original = vec![definition.clone(), record.clone(), unrelated.clone()];
    fs::write(&store, serde_json::to_vec_pretty(&original).unwrap()).unwrap();
    let initial = store_fingerprint(&store);
    let ack = synthetic_ack(&expected);
    let capture = capture_rowvia_saved_agent_scope(app.handle(), &expected).unwrap();
    assert_eq!(store_fingerprint(&store), initial);
    let json = serde_json::to_value(&capture).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "version": 1, "ownerPubkey": input.owner_pubkey, "relayOrigin": input.relay_url,
            "sourceInstance": input.source_instance, "stopped": true, "pending": true,
            "operation": { "version": 1, "operationId": input.operation_id,
                "agentName": format!("saved-{}", input.operation_id.replace('-', "")),
                "recipeId": "codex-local-v1", "native": {
                    "agentPubkey": expected.agent_pubkey,
                    "savedAuthorizationSha256": expected.saved_authorization_sha256,
                    "nativeConfigSha256": expected.native_config_sha256,
                    "channelId": input.channel_id, "systemPrompt": input.system_prompt,
                    "model": input.model, "workspace": input.workspace, "capsuleRun": input.capsule_run
                }
            }
        })
    );
    assert!(revalidate_rowvia_saved_agent_preparation_ack(app.handle(), &expected, &ack).is_err());
    assert_eq!(store_fingerprint(&store), initial);

    // All trusted selection fields and digest/pubkey expectations are required.
    for mutation in 0..15 {
        let mut changed = expected.clone();
        match mutation {
            0 => changed.selection.operation_id = uuid::Uuid::now_v7().to_string(),
            1 => changed.selection.owner_pubkey = "e".repeat(64),
            2 => changed.selection.relay_url = "ws://127.0.0.1:10".into(),
            3 => changed.selection.source_instance.push_str("-changed"),
            4 => changed.selection.channel_id = uuid::Uuid::new_v4().to_string(),
            5 => changed.selection.name.push_str(" changed"),
            6 => changed.selection.system_prompt.push_str(" changed"),
            7 => changed.selection.recipe = SavedAgentRecipe::ClaudeLocalV1,
            8 => changed.selection.model.push_str("-changed"),
            9 => changed.selection.workspace.push_str("-changed"),
            10 => changed.selection.capsule_run.push_str("/changed"),
            11 => changed.agent_pubkey = "e".repeat(64),
            12 => changed.native_config_sha256 = "e".repeat(64),
            13 => changed.saved_authorization_sha256 = "e".repeat(64),
            _ => changed.agent_pubkey = changed.agent_pubkey.to_ascii_uppercase(),
        }
        assert!(
            capture_rowvia_saved_agent_scope(app.handle(), &changed).is_err(),
            "selection {mutation}"
        );
        assert!(retain_rowvia_saved_agent_preparation_ack(app.handle(), &changed, &ack).is_err());
        assert_eq!(store_fingerprint(&store), initial);
    }
    for mutation in 0..8 {
        let mut rows = original.clone();
        match mutation {
            0 => rows[1].runtime_pid = Some(4_000_001),
            1 => rows.push(record.clone()),
            2 => {
                let mut duplicate = record.clone();
                duplicate.pubkey.make_ascii_uppercase();
                rows.push(duplicate);
            }
            3 => {
                let mut duplicate = record.clone();
                duplicate.pubkey = "e".repeat(64);
                rows.push(duplicate);
            }
            4 => {
                let mut duplicate = record.clone();
                duplicate.pubkey = "e".repeat(64);
                duplicate
                    .pending_saved_registration
                    .as_mut()
                    .unwrap()
                    .operation_id = uuid::Uuid::now_v7().to_string();
                rows.push(duplicate);
            }
            5 => rows[1].pending_saved_registration = None,
            6 => rows[1].auth_tag.as_mut().unwrap().push(' '),
            _ => rows.remove(1).pubkey.clear(),
        }
        fs::write(&store, serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
        let before = store_fingerprint(&store);
        assert!(
            capture_rowvia_saved_agent_scope(app.handle(), &expected).is_err(),
            "record {mutation}"
        );
        assert!(retain_rowvia_saved_agent_preparation_ack(app.handle(), &expected, &ack).is_err());
        assert_eq!(store_fingerprint(&store), before);
    }
    fs::write(&store, &initial.0).unwrap();
    let before = store_fingerprint(&store);
    // Marker values, no process or current-relay filtering. Uppercase key and
    // a different relay must still fail the same shared production checker.
    let mut marker_map = std::collections::HashMap::new();
    marker_map.insert(
        ManagedAgentRuntimeKey {
            pubkey: expected.agent_pubkey.to_ascii_uppercase(),
            relay_url: "ws://127.0.0.1:10".into(),
        },
        (),
    );
    assert!(checked_saved_agent_target(&original, &marker_map, &expected).is_err());
    assert!(checked_saved_agent_target(
        &original,
        &std::collections::HashMap::<_, ()>::new(),
        &expected
    )
    .is_ok());
    state.shutdown_started.store(true, Ordering::SeqCst);
    assert!(capture_rowvia_saved_agent_scope(app.handle(), &expected).is_err());
    assert!(retain_rowvia_saved_agent_preparation_ack(app.handle(), &expected, &ack).is_err());
    state.shutdown_started.store(false, Ordering::SeqCst);
    state.keyring_locked.store(true, Ordering::SeqCst);
    assert!(capture_rowvia_saved_agent_scope(app.handle(), &expected).is_err());
    state.keyring_locked.store(false, Ordering::SeqCst);
    *state.relay_url_override.lock().unwrap() = Some("ws://127.0.0.1:10".into());
    assert!(capture_rowvia_saved_agent_scope(app.handle(), &expected).is_err());
    *state.relay_url_override.lock().unwrap() = Some(RELAY.into());
    {
        let _workspace = state.workspace_apply_lock.try_lock().unwrap();
        assert!(capture_rowvia_saved_agent_scope(app.handle(), &expected).is_err());
    }
    assert_eq!(store_fingerprint(&store), before);

    let confirmation =
        retain_rowvia_saved_agent_preparation_ack(app.handle(), &expected, &ack).unwrap();
    assert_eq!(
        serde_json::to_value(&confirmation).unwrap(),
        serde_json::json!({"ack": ack, "confirmed": true})
    );
    let rows = storage::load_agent_store_readonly(app.handle())
        .unwrap()
        .records;
    assert_eq!(rows[0], definition);
    assert_eq!(rows[2], unrelated);
    let mut retained_record = record.clone();
    retained_record
        .pending_saved_registration
        .as_mut()
        .unwrap()
        .prepared_ack = Some(ack.clone());
    assert_eq!(rows[1], retained_record);
    assert_eq!(
        config_digest(
            &rows[1],
            rows[1].pending_saved_registration.as_ref().unwrap()
        )
        .unwrap(),
        expected.native_config_sha256
    );
    assert_eq!(rows[1].updated_at, record.updated_at);
    assert!(storage::pending_saved_registration_refusal(&rows[1]).is_some());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&store).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let retained = store_fingerprint(&store);
    assert_eq!(
        retain_rowvia_saved_agent_preparation_ack(app.handle(), &expected, &ack).unwrap(),
        confirmation
    );
    assert_eq!(
        revalidate_rowvia_saved_agent_preparation_ack(app.handle(), &expected, &ack).unwrap(),
        confirmation
    );
    assert_eq!(
        capture_rowvia_saved_agent_scope(app.handle(), &expected).unwrap(),
        capture
    );
    assert_eq!(
        store_fingerprint(&store),
        retained,
        "identical ACK replay rewrote the store"
    );
    for mutation in 0..9 {
        let mut changed = ack.clone();
        match mutation {
            0 => changed.version = 2,
            1 => changed.operation_id = uuid::Uuid::now_v7().to_string(),
            2 => changed.agent_pubkey = "e".repeat(64),
            3 => changed.native_config_sha256 = "e".repeat(64),
            4 => changed.saved_authorization_sha256 = "e".repeat(64),
            5 => changed.recipe_sha256 = "e".repeat(64),
            6 => changed.request_sha256 = "e".repeat(64),
            7 => changed.canonical21_sha256 = "e".repeat(64),
            _ => changed.prepared_receipt_sha256 = "e".repeat(64),
        }
        assert!(
            retain_rowvia_saved_agent_preparation_ack(app.handle(), &expected, &changed).is_err(),
            "ACK field {mutation}"
        );
        assert!(
            revalidate_rowvia_saved_agent_preparation_ack(app.handle(), &expected, &changed)
                .is_err()
        );
        assert_eq!(store_fingerprint(&store), retained);
    }
    for fault in [
        storage::PreparationAckWriteFault::BeforeCommit,
        storage::PreparationAckWriteFault::AfterCommit,
        storage::PreparationAckWriteFault::Readback,
    ] {
        fs::write(&store, &initial.0).unwrap();
        let before = store_fingerprint(&store);
        storage::PREPARATION_ACK_WRITE_FAULT.with(|slot| slot.set(Some(fault)));
        assert_eq!(
            retain_rowvia_saved_agent_preparation_ack(app.handle(), &expected, &ack).unwrap_err(),
            "saved agent preparation retention unknown"
        );
        if fault == storage::PreparationAckWriteFault::BeforeCommit {
            assert_eq!(store_fingerprint(&store), before);
            assert!(
                revalidate_rowvia_saved_agent_preparation_ack(app.handle(), &expected, &ack)
                    .is_err()
            );
        } else {
            let committed = store_fingerprint(&store);
            assert_eq!(
                revalidate_rowvia_saved_agent_preparation_ack(app.handle(), &expected, &ack)
                    .unwrap(),
                confirmation
            );
            assert_eq!(store_fingerprint(&store), committed);
        }
    }
    #[cfg(unix)]
    {
        let actual = app_data.join("synthetic-store-target.json");
        fs::rename(&store, &actual).unwrap();
        std::os::unix::fs::symlink(&actual, &store).unwrap();
        fs::write(&actual, &initial.0).unwrap();
        assert_eq!(
            capture_rowvia_saved_agent_scope(app.handle(), &expected).unwrap(),
            capture
        );
        assert_eq!(
            retain_rowvia_saved_agent_preparation_ack(app.handle(), &expected, &ack).unwrap(),
            confirmation
        );
        assert!(fs::symlink_metadata(&store)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read_link(&store).unwrap(), actual);
        let before = store_fingerprint(&actual);
        assert_eq!(
            revalidate_rowvia_saved_agent_preparation_ack(app.handle(), &expected, &ack).unwrap(),
            confirmation
        );
        assert_eq!(store_fingerprint(&actual), before);
        let snapshot = storage::load_agent_store_readonly(app.handle()).unwrap();
        let other = app_data.join("synthetic-drift-target.json");
        fs::write(&other, &initial.0).unwrap();
        let other_before = store_fingerprint(&other);
        fs::remove_file(&store).unwrap();
        std::os::unix::fs::symlink(&other, &store).unwrap();
        assert!(
            storage::retain_saved_agent_preparation_ack(app.handle(), snapshot, 1, &ack).is_err()
        );
        assert_eq!(store_fingerprint(&actual), before);
        assert_eq!(store_fingerprint(&other), other_before);
        fs::remove_file(&store).unwrap();
        std::os::unix::fs::symlink(&actual, &store).unwrap();
    }
    // Poison admission locks last; all refusals must leave the same saved bytes.
    let before = store_fingerprint(&store);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = state.managed_agent_processes.lock().unwrap();
        panic!("synthetic process-map poison");
    }));
    assert!(capture_rowvia_saved_agent_scope(app.handle(), &expected).is_err());
    assert!(retain_rowvia_saved_agent_preparation_ack(app.handle(), &expected, &ack).is_err());
    assert!(revalidate_rowvia_saved_agent_preparation_ack(app.handle(), &expected, &ack).is_err());
    assert_eq!(store_fingerprint(&store), before);
    assert_eq!(fs::read_dir(app_data.join("agents")).unwrap().count(), 1);
    assert!(
        !home.join("nest").exists()
            && !home.join("workspaces").exists()
            && !home.join("capsules").exists()
    );
}
