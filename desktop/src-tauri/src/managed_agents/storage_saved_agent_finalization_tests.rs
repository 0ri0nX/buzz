use super::*;
use crate::managed_agents::{
    PendingSavedAgentRegistration, SavedAgentPreparationAck, SavedAgentRecipe,
};
use std::path::Path;

const OPERATION: &str = "019a5432-1234-7abc-8def-0123456789ab";
const UNKNOWN: &str = "saved agent registration finalization unknown";

fn rows() -> Vec<ManagedAgentRecord> {
    let mut target: ManagedAgentRecord = serde_json::from_value(serde_json::json!({
        "pubkey":"b".repeat(64), "name":"synthetic target", "private_key_nsec":"unusable-synthetic-key",
        "auth_tag":"synthetic-auth", "relay_url":"ws://127.0.0.1:9", "acp_command":"synthetic",
        "agent_command":"synthetic", "agent_args":[], "mcp_command":"", "turn_timeout_seconds":60,
        "created_at":"synthetic", "updated_at":"synthetic", "start_on_app_launch":false,
        "auto_restart_on_config_change":false
    })).unwrap();
    let ack = SavedAgentPreparationAck {
        version: 1,
        operation_id: OPERATION.into(),
        agent_pubkey: target.pubkey.clone(),
        native_config_sha256: "1".repeat(64),
        saved_authorization_sha256: "2".repeat(64),
        recipe_sha256: "3".repeat(64),
        request_sha256: "4".repeat(64),
        canonical21_sha256: "5".repeat(64),
        prepared_receipt_sha256: "6".repeat(64),
    };
    target.pending_saved_registration = Some(PendingSavedAgentRegistration {
        operation_id: OPERATION.into(),
        owner_pubkey: "a".repeat(64),
        relay_url: target.relay_url.clone(),
        source_instance: "synthetic".into(),
        channel_id: "019a5432-1234-4abc-8def-0123456789ab".into(),
        recipe: SavedAgentRecipe::CodexLocalV1,
        model: "synthetic-model".into(),
        workspace: format!("/var/tmp/synthetic/{OPERATION}"),
        capsule_run: format!("/var/tmp/synthetic/{OPERATION}/run"),
        native_config_sha256: ack.native_config_sha256.clone(),
        saved_authorization_sha256: ack.saved_authorization_sha256.clone(),
        prepared_ack: Some(ack),
    });
    let mut z = target.clone();
    z.pubkey.clear();
    z.slug = Some("z".into());
    z.pending_saved_registration = None;
    let mut a = z.clone();
    a.slug = Some("a".into());
    let mut other = target.clone();
    other.pubkey = "d".repeat(64);
    other.pending_saved_registration = None;
    vec![z, target, a, other]
}

fn fixture(
    test: impl FnOnce(&AppHandle<tauri::test::MockRuntime>, &Path, Vec<ManagedAgentRecord>),
) {
    // No AppState/identity construction or HOME changes: only an isolated mock
    // app path and synthetic raw store are needed by this storage-only seam.
    let home = tempfile::Builder::new()
        .prefix("rowvia-cas-store-")
        .tempdir_in("/var/tmp")
        .unwrap();
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().identifier = home.path().to_str().unwrap().into();
    let app = tauri::test::mock_builder().build(context).unwrap();
    let directory = home.path().join("agents");
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("managed-agents.json");
    let records = rows();
    fs::write(&path, serde_json::to_vec_pretty(&records).unwrap()).unwrap();
    test(app.handle(), &path, records);
}

fn bytes(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap()
}

#[test]
fn native_finalize_store_compare_and_swap_preserves_full_record_order() {
    fixture(|app, path, records| {
        let pending = records[1].pending_saved_registration.clone().unwrap();
        let snapshot = load_agent_store_readonly(app).unwrap();
        let actual = finalize_saved_agent_registration(app, snapshot, 1, &pending).unwrap();
        let mut expected = records.clone();
        expected[1].pending_saved_registration = None;
        assert_eq!(actual.records, expected);
        assert_eq!(load_agent_store_readonly(app).unwrap().records, expected);
        assert_eq!(actual.records[0].slug.as_deref(), Some("z"));
        assert_eq!(actual.records[2].slug.as_deref(), Some("a"));
        let before = bytes(path);
        assert!(finalize_saved_agent_registration(app, actual, 1, &pending).is_err());
        assert_eq!(bytes(path), before);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    });
}

#[test]
fn native_finalize_store_refuses_full_snapshot_and_configured_path_drift() {
    fixture(|app, path, records| {
        let pending = records[1].pending_saved_registration.clone().unwrap();
        for mutation in 0..9 {
            fs::write(path, serde_json::to_vec_pretty(&records).unwrap()).unwrap();
            let mut snapshot = load_agent_store_readonly(app).unwrap();
            let mut changed = records.clone();
            match mutation {
                0 => changed[3].name.push('x'),
                1 => changed[3].updated_at.push('x'),
                2 => changed[3].private_key_nsec.push('x'),
                3 => changed.swap(0, 2),
                4 => {
                    changed.pop();
                }
                5 => changed.push(changed[3].clone()),
                6 => snapshot.path = path.with_file_name("other.json"),
                7 => snapshot.resolved = path.with_file_name("other-target.json"),
                _ => changed[1].pending_saved_registration = None,
            }
            fs::write(path, serde_json::to_vec_pretty(&changed).unwrap()).unwrap();
            let before = bytes(path);
            assert!(
                matches!(finalize_saved_agent_registration(app,snapshot,1,&pending),Err(error) if error==UNKNOWN),
                "mutation {mutation}"
            );
            assert_eq!(bytes(path), before);
        }
        for mutation in 0..6 {
            let mut changed = records.clone();
            match mutation {
                0 => changed[1].shared = true,
                1 => changed[1].runtime_pid = Some(42),
                2 => changed[1].start_on_app_launch = true,
                3 => changed[1].auto_restart_on_config_change = true,
                4 => {
                    changed[1]
                        .pending_saved_registration
                        .as_mut()
                        .unwrap()
                        .prepared_ack = None
                }
                _ => changed[1]
                    .pending_saved_registration
                    .as_mut()
                    .unwrap()
                    .prepared_ack
                    .as_mut()
                    .unwrap()
                    .request_sha256
                    .clear(),
            }
            // shared is a legacy deserializable but non-serializable field.
            let mut encoded = serde_json::to_value(&changed).unwrap();
            if mutation == 0 {
                encoded[1]["shared"] = serde_json::json!(true);
            }
            fs::write(path, serde_json::to_vec_pretty(&encoded).unwrap()).unwrap();
            let snapshot = load_agent_store_readonly(app).unwrap();
            let expected_pending = snapshot.records[1]
                .pending_saved_registration
                .clone()
                .unwrap();
            let before = bytes(path);
            assert!(
                finalize_saved_agent_registration(app, snapshot, 1, &expected_pending).is_err()
            );
            assert_eq!(bytes(path), before);
        }
    });
}

#[test]
fn native_finalize_store_symlink_target_drift_refuses_and_success_preserves_link() {
    #[cfg(unix)]
    fixture(|app, path, records| {
        use std::os::unix::{fs::symlink, fs::MetadataExt};
        let target = path.with_file_name("owned-target.json");
        let replacement = path.with_file_name("replacement.json");
        fs::rename(path, &target).unwrap();
        symlink(&target, path).unwrap();
        fs::write(&replacement, bytes(&target)).unwrap();
        let snapshot = load_agent_store_readonly(app).unwrap();
        let pending = records[1].pending_saved_registration.as_ref().unwrap();
        fs::remove_file(path).unwrap();
        symlink(&replacement, path).unwrap();
        let original_before = bytes(&target);
        let replacement_before = bytes(&replacement);
        assert!(finalize_saved_agent_registration(app, snapshot, 1, pending).is_err());
        assert_eq!(bytes(&target), original_before);
        assert_eq!(bytes(&replacement), replacement_before);
        let inode = fs::symlink_metadata(path).unwrap().ino();
        let snapshot = load_agent_store_readonly(app).unwrap();
        finalize_saved_agent_registration(app, snapshot, 1, pending).unwrap();
        assert!(fs::symlink_metadata(path).unwrap().file_type().is_symlink());
        assert_eq!(fs::symlink_metadata(path).unwrap().ino(), inode);
        assert_eq!(fs::read_link(path).unwrap(), replacement);
        assert_eq!(bytes(&target), original_before);
    });
    #[cfg(unix)]
    fixture(|app, path, records| {
        use std::os::unix::{fs::symlink, fs::MetadataExt};
        let target = path.with_file_name("pinned-target.json");
        let replacement = path.with_file_name("unverified-replacement.json");
        fs::rename(path, &target).unwrap();
        symlink(&target, path).unwrap();
        fs::write(&replacement, bytes(&target)).unwrap();
        let snapshot = load_agent_store_readonly(app).unwrap();
        let pending = records[1].pending_saved_registration.as_ref().unwrap();
        let replacement_before = (
            bytes(&replacement),
            fs::metadata(&replacement).unwrap().modified().unwrap(),
            fs::metadata(&replacement).unwrap().ino(),
        );
        // Causal mutation AFTER the helper's full snapshot/path comparison,
        // immediately before its existing atomic writer. With snapshot.path,
        // this control overwrites replacement and fails these assertions.
        FINALIZATION_CONFIGURED_ALIAS_RETARGET
            .with(|slot| *slot.borrow_mut() = Some(replacement.clone()));
        assert!(
            matches!(finalize_saved_agent_registration(app, snapshot, 1, pending), Err(error) if error == UNKNOWN)
        );
        assert_eq!(fs::read_link(path).unwrap(), replacement);
        assert_eq!(
            (
                bytes(&replacement),
                fs::metadata(&replacement).unwrap().modified().unwrap(),
                fs::metadata(&replacement).unwrap().ino()
            ),
            replacement_before
        );
        let mut expected = records;
        expected[1].pending_saved_registration = None;
        let committed: Vec<ManagedAgentRecord> = serde_json::from_slice(&bytes(&target)).unwrap();
        assert_eq!(committed, expected);
        assert!(load_agent_store_readonly(app).unwrap().records[1]
            .pending_saved_registration
            .is_some());
        // No automatic retry repairs the alias or clears the replacement.
        assert!(FINALIZATION_CONFIGURED_ALIAS_RETARGET.with(|slot| slot.borrow().is_none()));
    });
}

#[test]
fn native_finalize_store_faults_report_unknown_preserving_commit_outcome() {
    fixture(|app, path, records| {
        for fault in [
            PreparationAckWriteFault::BeforeCommit,
            PreparationAckWriteFault::AfterCommit,
            PreparationAckWriteFault::Readback,
        ] {
            fs::write(path, serde_json::to_vec_pretty(&records).unwrap()).unwrap();
            let before = bytes(path);
            let snapshot = load_agent_store_readonly(app).unwrap();
            FINALIZATION_WRITE_FAULT.with(|slot| slot.set(Some(fault)));
            assert!(
                matches!(finalize_saved_agent_registration(app,snapshot,1,records[1].pending_saved_registration.as_ref().unwrap()),Err(error) if error==UNKNOWN)
            );
            if fault == PreparationAckWriteFault::BeforeCommit {
                assert_eq!(bytes(path), before);
            } else {
                let mut expected = records.clone();
                expected[1].pending_saved_registration = None;
                assert_eq!(load_agent_store_readonly(app).unwrap().records, expected);
            }
        }
    });
}
