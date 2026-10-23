use super::*;
use std::{collections::HashMap, fs, path::Path, sync::atomic::Ordering};

pub(crate) const OPERATION: &str = "019a5432-1234-7abc-8def-0123456789ab";
pub(crate) const RELAY: &str = "ws://127.0.0.1:9";

/// Pure synthetic persisted bytes: deliberately no usable agent private key,
/// no prepare/retain/signing call, and no key hydration prerequisite.
pub(crate) fn fixture_record(owner: &str) -> ManagedAgentRecord {
    let mut record: ManagedAgentRecord = serde_json::from_value(serde_json::json!({
        "pubkey": "b".repeat(64), "name": "Original reviewed display name",
        "private_key_nsec": "synthetic-invalid-key-never-hydrated",
        "auth_tag": "synthetic-existing-auth-tag", "relay_url": RELAY,
        "acp_command": "synthetic-acp", "agent_command": "/usr/bin/python3.13",
        "agent_command_override": "/usr/bin/python3.13",
        "agent_args": ["-IS", format!("/var/tmp/synthetic/{OPERATION}/run")],
        "mcp_command": "", "turn_timeout_seconds": 60,
        "system_prompt": "Original reviewed prompt", "start_on_app_launch": false,
        "auto_restart_on_config_change": false,
        "created_at": "synthetic", "updated_at": "synthetic"
    }))
    .unwrap();
    let mut pending = PendingSavedAgentRegistration {
        operation_id: OPERATION.into(),
        owner_pubkey: owner.into(),
        relay_url: RELAY.into(),
        source_instance: "synthetic-native".into(),
        channel_id: "019a5432-1234-4abc-8def-0123456789ab".into(),
        recipe: SavedAgentRecipe::CodexLocalV1,
        model: "synthetic-model".into(),
        workspace: format!("/var/tmp/synthetic/{OPERATION}"),
        capsule_run: format!("/var/tmp/synthetic/{OPERATION}/run"),
        saved_authorization_sha256: digest(record.auth_tag.as_ref().unwrap().as_bytes()),
        native_config_sha256: String::new(),
        prepared_ack: None,
    };
    pending.native_config_sha256 = config_digest(&record, &pending).unwrap();
    pending.prepared_ack = Some(SavedAgentPreparationAck {
        version: 1,
        operation_id: OPERATION.into(),
        agent_pubkey: record.pubkey.clone(),
        native_config_sha256: pending.native_config_sha256.clone(),
        saved_authorization_sha256: pending.saved_authorization_sha256.clone(),
        recipe_sha256: "1".repeat(64),
        request_sha256: "2".repeat(64),
        canonical21_sha256: "3".repeat(64),
        prepared_receipt_sha256: "4".repeat(64),
    });
    record.pending_saved_registration = Some(pending);
    record
}

pub(crate) fn fingerprint(path: &Path) -> (Vec<u8>, std::time::SystemTime, u64) {
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

pub(crate) fn tree_fingerprint(
    root: &Path,
) -> BTreeMap<PathBuf, (bool, Vec<u8>, std::time::SystemTime, u64)> {
    fn visit(
        root: &Path,
        path: &Path,
        out: &mut BTreeMap<PathBuf, (bool, Vec<u8>, std::time::SystemTime, u64)>,
    ) {
        let metadata = fs::symlink_metadata(path).unwrap();
        assert!(
            !metadata.file_type().is_symlink(),
            "synthetic home unexpectedly contains symlink"
        );
        #[cfg(unix)]
        let inode = {
            use std::os::unix::fs::MetadataExt;
            metadata.ino()
        };
        #[cfg(not(unix))]
        let inode = metadata.len();
        out.insert(
            path.strip_prefix(root).unwrap().to_path_buf(),
            (
                metadata.is_dir(),
                if metadata.is_dir() {
                    Vec::new()
                } else {
                    fs::read(path).unwrap()
                },
                metadata.modified().unwrap(),
                inode,
            ),
        );
        if metadata.is_dir() {
            for child in fs::read_dir(path).unwrap() {
                visit(root, &child.unwrap().path(), out);
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

#[test]
fn native_resume_snapshot_preserves_original_selection_and_exact_historical_ack() {
    let record = fixture_record(&"a".repeat(64));
    let bytes = serde_json::to_vec(&record).unwrap();
    for _ in 0..2 {
        // A second deserialization has no invocation-local prepare history.
        let mut legacy = record.clone();
        legacy.pubkey = "c".repeat(64);
        legacy.name = "Unrelated legacy record".into();
        legacy.pending_saved_registration = None;
        let rows: Vec<ManagedAgentRecord> = vec![serde_json::from_slice(&bytes).unwrap(), legacy];
        let resumed = checked_resume_target(&rows, &HashMap::<_, ()>::new(), OPERATION).unwrap();
        let (expected, public, ack) = resumed.into_parts();
        assert_eq!(expected.selection.name, record.name);
        assert_eq!(Some(expected.selection.system_prompt), record.system_prompt);
        assert_ne!(expected.selection.name, public.core_agent_name);
        assert_eq!(public, project(&record).unwrap());
        assert_eq!(
            Some(ack),
            record
                .pending_saved_registration
                .as_ref()
                .unwrap()
                .prepared_ack
        );
        assert_eq!(serde_json::to_vec(&rows[0]).unwrap(), bytes);
    }
}

#[test]
fn native_resume_snapshot_refuses_missing_legacy_duplicate_and_live_cohorts() {
    let record = fixture_record(&"a".repeat(64));
    let empty = HashMap::<_, ()>::new();
    for mutation in 0..11 {
        let mut rows = vec![record.clone()];
        match mutation {
            0 => rows.clear(),
            1 => rows[0].pending_saved_registration = None,
            2 => {
                rows[0]
                    .pending_saved_registration
                    .as_mut()
                    .unwrap()
                    .prepared_ack = None
            }
            3 => rows.push(record.clone()),
            4 => {
                let mut duplicate = record.clone();
                duplicate.pubkey.make_ascii_uppercase();
                duplicate.pending_saved_registration = None;
                rows.push(duplicate);
            }
            5 => {
                let mut duplicate = record.clone();
                duplicate.pubkey = "c".repeat(64);
                rows.push(duplicate);
            }
            6 => {
                let mut duplicate = record.clone();
                duplicate.pubkey = "c".repeat(64);
                duplicate
                    .pending_saved_registration
                    .as_mut()
                    .unwrap()
                    .operation_id = "019a5432-1234-7abc-8def-0123456789ac".into();
                rows.push(duplicate);
            }
            7 => rows[0].runtime_pid = Some(4_000_001),
            8 => rows[0].pubkey.make_ascii_uppercase(),
            9 => rows[0].auth_tag = None,
            _ => {
                let mut duplicate = record.clone();
                duplicate.pending_saved_registration = None;
                rows.push(duplicate);
            }
        }
        let before = serde_json::to_vec(&rows).unwrap();
        assert!(
            checked_resume_target(&rows, &empty, OPERATION).is_err(),
            "mutation {mutation}"
        );
        assert_eq!(serde_json::to_vec(&rows).unwrap(), before);
    }
    let runtimes = HashMap::from([(
        ManagedAgentRuntimeKey {
            pubkey: record.pubkey.to_ascii_uppercase(),
            relay_url: "ws://127.0.0.1:10".into(),
        },
        (),
    )]);
    assert!(checked_resume_target(&[record], &runtimes, OPERATION).is_err());
    // Direct admission assertions cannot pass merely because a target is absent.
    assert!(validate_resume_operation(OPERATION).is_ok());
    for operation in [
        "",
        "not-a-uuid",
        "019A5432-1234-7ABC-8DEF-0123456789AB",
        "019a5432-1234-4abc-8def-0123456789ab",
        "019a5432-1234-7abc-cdef-0123456789ab",
    ] {
        assert!(
            validate_resume_operation(operation).is_err(),
            "operation {operation}"
        );
    }
}

#[test]
fn native_resume_snapshot_refuses_digest_auth_scope_and_ack_cohort_drift() {
    let record = fixture_record(&"a".repeat(64));
    for mutation in 0..19 {
        let mut changed = record.clone();
        match mutation {
            0 => changed.name.push_str(" changed"),
            1 => changed.system_prompt.as_mut().unwrap().push_str(" changed"),
            2 => changed.auth_tag.as_mut().unwrap().push('x'),
            3 => changed.agent_args.push("changed".into()),
            4 => {
                changed.env_vars.insert("CHANGED".into(), "1".into());
            }
            5 => changed
                .pending_saved_registration
                .as_mut()
                .unwrap()
                .source_instance
                .push_str("-changed"),
            6 => {
                changed
                    .pending_saved_registration
                    .as_mut()
                    .unwrap()
                    .owner_pubkey = "c".repeat(64)
            }
            7 => {
                changed
                    .pending_saved_registration
                    .as_mut()
                    .unwrap()
                    .relay_url = "ws://127.0.0.1:10".into()
            }
            8 => {
                changed
                    .pending_saved_registration
                    .as_mut()
                    .unwrap()
                    .channel_id = "019a5432-1234-4abc-8def-0123456789ac".into()
            }
            9 => changed
                .pending_saved_registration
                .as_mut()
                .unwrap()
                .capsule_run
                .push_str("/changed"),
            10 => {
                changed
                    .pending_saved_registration
                    .as_mut()
                    .unwrap()
                    .prepared_ack
                    .as_mut()
                    .unwrap()
                    .version = 2
            }
            11 => {
                changed
                    .pending_saved_registration
                    .as_mut()
                    .unwrap()
                    .prepared_ack
                    .as_mut()
                    .unwrap()
                    .operation_id = "019a5432-1234-7abc-8def-0123456789ac".into()
            }
            12 => {
                changed
                    .pending_saved_registration
                    .as_mut()
                    .unwrap()
                    .prepared_ack
                    .as_mut()
                    .unwrap()
                    .agent_pubkey = "c".repeat(64)
            }
            13 => {
                changed
                    .pending_saved_registration
                    .as_mut()
                    .unwrap()
                    .prepared_ack
                    .as_mut()
                    .unwrap()
                    .native_config_sha256 = "c".repeat(64)
            }
            14 => {
                changed
                    .pending_saved_registration
                    .as_mut()
                    .unwrap()
                    .prepared_ack
                    .as_mut()
                    .unwrap()
                    .saved_authorization_sha256 = "c".repeat(64)
            }
            15 => {
                changed
                    .pending_saved_registration
                    .as_mut()
                    .unwrap()
                    .prepared_ack
                    .as_mut()
                    .unwrap()
                    .recipe_sha256 = "0".repeat(64)
            }
            16 => {
                changed
                    .pending_saved_registration
                    .as_mut()
                    .unwrap()
                    .prepared_ack
                    .as_mut()
                    .unwrap()
                    .request_sha256 = "C".repeat(64)
            }
            17 => changed
                .pending_saved_registration
                .as_mut()
                .unwrap()
                .prepared_ack
                .as_mut()
                .unwrap()
                .canonical21_sha256
                .clear(),
            _ => changed
                .pending_saved_registration
                .as_mut()
                .unwrap()
                .prepared_ack
                .as_mut()
                .unwrap()
                .prepared_receipt_sha256
                .clear(),
        }
        let before = serde_json::to_vec(&changed).unwrap();
        assert!(
            checked_resume_target(&[changed.clone()], &HashMap::<_, ()>::new(), OPERATION).is_err(),
            "mutation {mutation}"
        );
        assert_eq!(serde_json::to_vec(&changed).unwrap(), before);
    }
}

#[test]
fn native_resume_readonly_store_restart_and_public_identity_refusals() {
    const CHILD: &str = "BUZZ_ROWVIA_RESUME_TEST_HOME";
    if let Some(home) = std::env::var_os(CHILD) {
        return confined_resume(Path::new(&home));
    }
    let home = tempfile::Builder::new()
        .prefix("rowvia-c3-resume-")
        .tempdir_in("/var/tmp")
        .unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "commands::agents::rowvia_saved_agent::resume_tests::native_resume_readonly_store_restart_and_public_identity_refusals", "--nocapture"])
        .env(CHILD, home.path()).env("HOME", home.path()).env("XDG_DATA_HOME", home.path())
        .env("APPDATA", home.path()).env("LOCALAPPDATA", home.path())
        .env_remove("BUZZ_PRIVATE_KEY").env_remove("BUZZ_AUTH_TAG")
        .env_remove("BUZZ_RELAY_URL").env_remove("BUZZ_NETWORK_TRACE").output().unwrap();
    assert!(
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "confined resume failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn confined_resume(home: &Path) {
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
    let state = app.state::<AppState>();
    let owner = state.keys.lock().unwrap().public_key().to_hex();
    assert!(resume_rowvia_saved_agent_preparation(app.handle(), OPERATION).is_err());
    assert!(!app_data.exists(), "missing store created a directory");
    fs::create_dir_all(app_data.join("agents")).unwrap();
    let store = app_data.join("agents/managed-agents.json");
    fs::write(&store, b"malformed synthetic JSON").unwrap();
    let before = fingerprint(&store);
    assert!(resume_rowvia_saved_agent_preparation(app.handle(), OPERATION).is_err());
    assert_eq!(fingerprint(&store), before);
    assert_eq!(fs::read_dir(app_data.join("agents")).unwrap().count(), 1);
    let record = fixture_record(&owner);
    fs::write(
        &store,
        serde_json::to_vec_pretty(std::slice::from_ref(&record)).unwrap(),
    )
    .unwrap();
    let before = fingerprint(&store);
    let key_canary = home.join("keyring-canary");
    let retention_canary = home.join("retention-canary");
    fs::write(&key_canary, b"synthetic keyring unchanged").unwrap();
    fs::write(&retention_canary, b"synthetic retention unchanged").unwrap();
    let key_before = fingerprint(&key_canary);
    let retention_before = fingerprint(&retention_canary);
    let tree_before = tree_fingerprint(home);
    for _ in 0..2 {
        let (expected, public, ack) =
            resume_rowvia_saved_agent_preparation(app.handle(), OPERATION)
                .unwrap()
                .into_parts();
        assert_eq!(expected.selection.name, record.name);
        assert_eq!(public, project(&record).unwrap());
        assert_eq!(
            Some(ack),
            record
                .pending_saved_registration
                .as_ref()
                .unwrap()
                .prepared_ack
        );
        assert_eq!(fingerprint(&store), before);
    }
    for flag in [
        &state.identity_lost,
        &state.keyring_locked,
        &state.reset_failed,
        &state.shutdown_started,
    ] {
        flag.store(true, Ordering::SeqCst);
        assert!(resume_rowvia_saved_agent_preparation(app.handle(), OPERATION).is_err());
        flag.store(false, Ordering::SeqCst);
    }
    *state.relay_url_override.lock().unwrap() = Some("ws://127.0.0.1:10".into());
    assert!(resume_rowvia_saved_agent_preparation(app.handle(), OPERATION).is_err());
    *state.relay_url_override.lock().unwrap() = Some(RELAY.into());
    {
        let _workspace = state.workspace_apply_lock.try_lock().unwrap();
        assert!(resume_rowvia_saved_agent_preparation(app.handle(), OPERATION).is_err());
    }
    assert_eq!(fingerprint(&store), before);
    assert_eq!(fingerprint(&key_canary), key_before);
    assert_eq!(fingerprint(&retention_canary), retention_before);
    assert_eq!(tree_fingerprint(home), tree_before);
    assert_eq!(state.keys.lock().unwrap().public_key().to_hex(), owner);
    assert_eq!(fs::read_dir(app_data.join("agents")).unwrap().count(), 1);
    assert!(!home.join("nest").exists());
}
