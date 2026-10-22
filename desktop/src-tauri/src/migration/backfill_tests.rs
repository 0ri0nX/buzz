use super::backfill_standalone_agents_in_dir;
use crate::managed_agents::spawn_snapshot::prospective_spawn_config_snapshot;
use crate::managed_agents::{AgentDefinition, ManagedAgentRecord};
use crate::migration::test_support::{read_agents_json, write_agents_json};
use std::path::Path;

fn standalone_agent_json(name: &str, pubkey: &str, prompt: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "pubkey": pubkey,
        "relay_url": "ws://localhost:3000",
        "acp_command": "buzz-acp",
        "agent_command": "goose",
        "agent_args": [],
        "mcp_command": "",
        "turn_timeout_seconds": 320,
        "parallelism": 4,
        "system_prompt": prompt,
        "model": "gpt-x",
        "provider": "openai",
        "respond_to": "anyone",
        "env_vars": { "API_KEY": "secret" },
        "start_on_app_launch": true,
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z",
        "last_started_at": null,
        "last_stopped_at": null,
        "last_exit_code": null,
        "last_error": null
    })
}

fn load_typed(dir: &Path) -> Vec<ManagedAgentRecord> {
    let content = std::fs::read_to_string(dir.join("agents").join("managed-agents.json")).unwrap();
    serde_json::from_str(&content).unwrap()
}

fn base(dir: &Path) -> std::path::PathBuf {
    dir.join("agents")
}

// The saved-owner preparation fixture has Unix absolute paths. Its production
// owner-host lane is Linux-only; ordinary backfill tests below stay portable.
#[cfg(unix)]
fn pending_agent() -> ManagedAgentRecord {
    crate::commands::pending_saved_agent_fixture(&"a".repeat(64))
}

// Historical B5 output, including its two instance changes. This synthetic
// fixture never mints an identity, grants authority, or loads a real store.
#[cfg(unix)]
fn historical_pending_backfill(mut instance: ManagedAgentRecord) -> Vec<ManagedAgentRecord> {
    let mut source = instance.clone();
    source.slug = Some(instance.pubkey.clone());
    source.definition_respond_to = Some(instance.respond_to.as_str().to_string());
    source.definition_respond_to_allowlist = instance.respond_to_allowlist.clone();
    source.definition_parallelism = Some(instance.parallelism);
    let view = source.to_definition_view().unwrap();
    let version = crate::managed_agents::persona_events::persona_content_hash(
        &crate::managed_agents::persona_events::persona_event_content(&view),
    );
    instance.persona_id = Some(instance.pubkey.clone());
    instance.persona_source_version = Some(version);
    vec![instance, view.into_agent_record()]
}

#[test]
#[cfg(unix)]
fn pending_saved_registration_is_not_backfilled_or_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let original = pending_agent();
    write_agents_json(dir.path(), &serde_json::json!([original]));
    let before = std::fs::read(base(dir.path()).join("managed-agents.json")).unwrap();
    assert_eq!(
        backfill_standalone_agents_in_dir(&base(dir.path())).unwrap(),
        0
    );
    assert_eq!(
        std::fs::read(base(dir.path()).join("managed-agents.json")).unwrap(),
        before,
    );
    assert!(!base(dir.path())
        .join("managed-agents.json.pre-backfill.bak")
        .exists());
}

#[test]
#[cfg(unix)]
fn exact_historical_pending_backfill_restores_original_without_new_authority() {
    let dir = tempfile::tempdir().unwrap();
    let original = pending_agent();
    let rows = historical_pending_backfill(original.clone());
    let definition = rows[1].clone();
    write_agents_json(dir.path(), &serde_json::json!(rows));
    let before = std::fs::read(base(dir.path()).join("managed-agents.json")).unwrap();
    assert_eq!(
        backfill_standalone_agents_in_dir(&base(dir.path())).unwrap(),
        1
    );
    let restored = load_typed(dir.path());
    assert_eq!(
        serde_json::to_value(&restored).unwrap(),
        serde_json::json!([original, definition])
    );
    assert_eq!(
        std::fs::read(base(dir.path()).join("managed-agents.json.pre-backfill.bak")).unwrap(),
        before,
    );
    let first = std::fs::read(base(dir.path()).join("managed-agents.json")).unwrap();
    assert_eq!(
        backfill_standalone_agents_in_dir(&base(dir.path())).unwrap(),
        0
    );
    assert_eq!(
        std::fs::read(base(dir.path()).join("managed-agents.json")).unwrap(),
        first
    );
}

#[test]
#[cfg(unix)]
fn pending_backfill_repair_refuses_drift_ambiguity_and_foreign_links_without_writes() {
    for mutation in [
        "prompt",
        "acp",
        "command",
        "coordinated-command",
        "override",
        "args",
        "team",
        "source-team",
        "source-slug",
        "source-version",
        "foreign-link",
        "owner",
        "relay",
        "source",
        "channel",
        "model",
        "workspace",
        "capsule",
        "auth",
        "digest",
        "ack",
        "missing-ack",
        "definition-prompt",
        "definition-command",
        "definition-builtin",
        "definition-shared",
        "instance-shared",
        "definition-team",
        "definition-slug",
        "duplicate-definition",
        "shared-link",
        "duplicate-instance",
        "duplicate-operation",
        "live",
        "autostart",
        "auto-restart",
        "instance-model",
        "provider",
        "recipe",
        "definition-version",
        "definition-created",
        "definition-key",
        "runtime",
        "definition-quad",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let original = pending_agent();
        let mut rows = serde_json::json!(historical_pending_backfill(original.clone()));
        let pending = "pending_saved_registration";
        match mutation {
            "prompt" => rows[0]["system_prompt"] = "Changed prompt".into(),
            "acp" => rows[0]["acp_command"] = "changed-acp".into(),
            "command" => rows[0]["agent_command"] = "/changed/command".into(),
            "coordinated-command" => {
                let mut changed = original.clone();
                changed.acp_command = "changed-acp".into();
                changed.agent_command = "/changed/command".into();
                changed.agent_command_override = Some("/changed/command".into());
                rows = serde_json::json!(historical_pending_backfill(changed));
            }
            "override" => rows[0]["agent_command_override"] = "/changed/command".into(),
            "args" => rows[0]["agent_args"] = serde_json::json!(["changed"]),
            "team" => rows[0]["team_id"] = "foreign-team".into(),
            "source-team" => rows[0]["source_team"] = "foreign-team".into(),
            "source-slug" => rows[0]["source_team_persona_slug"] = "foreign-persona".into(),
            "source-version" => rows[0]["persona_source_version"] = "changed".into(),
            "foreign-link" => rows[0]["persona_id"] = "foreign-persona".into(),
            "owner" => rows[0][pending]["owner_pubkey"] = "c".repeat(64).into(),
            "relay" => rows[0][pending]["relay_url"] = "ws://127.0.0.1:10".into(),
            "source" => rows[0][pending]["source_instance"] = "foreign-native".into(),
            "channel" => {
                rows[0][pending]["channel_id"] = "019a5432-1234-4abc-8def-0123456789ac".into()
            }
            "model" => rows[0][pending]["model"] = "changed-model".into(),
            "workspace" => rows[0][pending]["workspace"] = "/changed/workspace".into(),
            "capsule" => rows[0][pending]["capsule_run"] = "/changed/run".into(),
            "auth" => rows[0]["auth_tag"] = "changed-synthetic-auth".into(),
            "digest" => rows[0][pending]["native_config_sha256"] = "5".repeat(64).into(),
            "ack" => rows[0][pending]["prepared_ack"]["nativeConfigSha256"] = "5".repeat(64).into(),
            "missing-ack" => rows[0][pending]["prepared_ack"] = serde_json::Value::Null,
            "definition-prompt" => rows[1]["system_prompt"] = "Changed prompt".into(),
            "definition-command" => rows[1]["acp_command"] = "changed-acp".into(),
            "definition-builtin" => rows[1]["is_builtin"] = true.into(),
            "definition-shared" => rows[1]["shared"] = true.into(),
            "instance-shared" => rows[0]["shared"] = true.into(),
            "definition-team" => rows[1]["source_team"] = "foreign-team".into(),
            "definition-slug" => rows[1]["slug"] = "foreign-persona".into(),
            "duplicate-definition" => {
                let duplicate = rows[1].clone();
                rows.as_array_mut().unwrap().push(duplicate);
            }
            "shared-link" => {
                let mut linked = serde_json::to_value(&original).unwrap();
                linked["pubkey"] = "c".repeat(64).into();
                linked["persona_id"] = original.pubkey.clone().into();
                linked[pending] = serde_json::Value::Null;
                rows.as_array_mut().unwrap().push(linked);
            }
            "duplicate-instance" | "duplicate-operation" => {
                let mut duplicate = rows[0].clone();
                if mutation == "duplicate-operation" {
                    duplicate["pubkey"] = "c".repeat(64).into();
                    duplicate["persona_id"] = "unrelated-definition".into();
                }
                rows.as_array_mut().unwrap().push(duplicate);
            }
            "live" => rows[0]["runtime_pid"] = 42.into(),
            "autostart" => rows[0]["start_on_app_launch"] = true.into(),
            "auto-restart" => rows[0]["auto_restart_on_config_change"] = true.into(),
            "instance-model" => rows[0]["model"] = "changed-model".into(),
            "provider" => rows[0]["provider"] = "changed-provider".into(),
            "recipe" => rows[0][pending]["recipe"] = "claude-local-v1".into(),
            "definition-version" => rows[1]["persona_source_version"] = "changed".into(),
            "definition-created" => rows[1]["created_at"] = "changed".into(),
            "definition-key" => rows[1]["private_key_nsec"] = "invalid-synthetic-key".into(),
            "runtime" => rows[0]["runtime"] = "changed-runtime".into(),
            "definition-quad" => rows[0]["definition_parallelism"] = 7.into(),
            _ => unreachable!(),
        }
        // Every negative row must reach the recovery classifier. Malformed
        // serde input would only prove parsing failed, not cohort refusal.
        let parsed: Vec<ManagedAgentRecord> = serde_json::from_value(rows.clone()).unwrap();
        if mutation == "coordinated-command" {
            assert_eq!(
                parsed[0].pending_saved_registration,
                original.pending_saved_registration
            );
            assert_eq!(parsed[0].auth_tag, original.auth_tag);
            assert_ne!(
                parsed[0].persona_source_version,
                historical_pending_backfill(original.clone())[0].persona_source_version,
            );
        }
        write_agents_json(dir.path(), &rows);
        let before = std::fs::read(base(dir.path()).join("managed-agents.json")).unwrap();
        assert_eq!(
            backfill_standalone_agents_in_dir(&base(dir.path())).unwrap(),
            0,
            "{mutation}"
        );
        assert_eq!(
            std::fs::read(base(dir.path()).join("managed-agents.json")).unwrap(),
            before,
            "{mutation}"
        );
        assert!(
            !base(dir.path())
                .join("managed-agents.json.pre-backfill.bak")
                .exists(),
            "{mutation}"
        );
    }
}

#[test]
#[cfg(unix)]
fn pending_backfill_repair_preserves_unrelated_records_and_pristine_backup() {
    let dir = tempfile::tempdir().unwrap();
    let original = pending_agent();
    let mut rows = historical_pending_backfill(original.clone());
    let definition = rows[1].clone();
    let mut unrelated = original.clone();
    unrelated.pubkey = "c".repeat(64);
    unrelated.persona_id = Some("unrelated-definition".into());
    unrelated.pending_saved_registration = None;
    rows.insert(0, unrelated.clone());
    write_agents_json(dir.path(), &serde_json::json!(rows));
    std::fs::write(
        base(dir.path()).join("managed-agents.json.pre-backfill.bak"),
        b"earlier pristine backup",
    )
    .unwrap();
    assert_eq!(
        backfill_standalone_agents_in_dir(&base(dir.path())).unwrap(),
        1
    );
    assert_eq!(
        load_typed(dir.path()),
        vec![unrelated, original, definition]
    );
    assert_eq!(
        std::fs::read(base(dir.path()).join("managed-agents.json.pre-backfill.bak")).unwrap(),
        b"earlier pristine backup"
    );
}

#[test]
fn backfill_links_standalone_agent_to_manufactured_definition() {
    let dir = tempfile::tempdir().unwrap();
    let pubkey = "a".repeat(64);
    write_agents_json(
        dir.path(),
        &serde_json::json!([standalone_agent_json(
            "Solo",
            &pubkey,
            Some("You are Solo.")
        )]),
    );

    let backfilled = backfill_standalone_agents_in_dir(&base(dir.path())).unwrap();
    assert_eq!(backfilled, 1);

    let records = load_typed(dir.path());
    assert_eq!(records.len(), 2, "instance + manufactured definition");

    let instance = records.iter().find(|r| !r.pubkey.is_empty()).unwrap();
    assert_eq!(instance.persona_id.as_deref(), Some(pubkey.as_str()));
    assert!(instance.persona_source_version.is_some());

    let definition = records.iter().find(|r| r.pubkey.is_empty()).unwrap();
    assert_eq!(definition.slug.as_deref(), Some(pubkey.as_str()));
    assert_eq!(definition.system_prompt.as_deref(), Some("You are Solo."));
    assert_eq!(definition.model.as_deref(), Some("gpt-x"));
    // Env COPIED (B5 pin): later instances inherit a working config.
    assert_eq!(
        definition.env_vars.get("API_KEY").map(String::as_str),
        Some("secret")
    );
    // Instance quad copied up as the definition's defaults.
    assert_eq!(definition.definition_respond_to.as_deref(), Some("anyone"));
    assert_eq!(definition.definition_parallelism, Some(4));

    // The recorded version matches the definition's actual content hash —
    // the drift badge starts clean.
    let view = definition.to_definition_view().unwrap();
    let expected = crate::managed_agents::persona_events::persona_content_hash(
        &crate::managed_agents::persona_events::persona_event_content(&view),
    );
    assert_eq!(
        instance.persona_source_version.as_deref(),
        Some(expected.as_str())
    );
}

#[test]
fn backfilled_definition_carries_prompt_present_even_if_empty() {
    // LOAD-BEARING (B5 gates): old readers hard-fail on an absent prompt. A
    // prompt-less backfilled definition would leave a wiped old device with
    // no heal source, permanently. `AgentDefinition.system_prompt` is a plain
    // String and the outbound projection wraps it in `Some` unconditionally
    // — this row pins that chain against refactors.
    let dir = tempfile::tempdir().unwrap();
    let pubkey = "b".repeat(64);
    write_agents_json(
        dir.path(),
        &serde_json::json!([standalone_agent_json("NoPrompt", &pubkey, None)]),
    );

    backfill_standalone_agents_in_dir(&base(dir.path())).unwrap();

    let records = load_typed(dir.path());
    let definition = records.iter().find(|r| r.pubkey.is_empty()).unwrap();
    let view: AgentDefinition = definition.to_definition_view().unwrap();
    assert_eq!(view.system_prompt, "", "empty, not absent");
    let content = crate::managed_agents::persona_events::persona_event_content(&view);
    assert_eq!(
        content.system_prompt.as_deref(),
        Some(""),
        "wire projection must carry Some(\"\") — the old-reader heal source"
    );
}

#[test]
fn backfill_of_promptless_record_keeps_spawn_snapshot_stable() {
    // B5 drift row 2: pre-backfill the record snapshots prompt None; post-backfill
    // the prospective re-snapshot pulls Some("") from the manufactured
    // definition. The spawn layer treats an empty prompt as no prompt (env
    // absent either way), so the snapshot must not move — otherwise every
    // prompt-less standalone agent lights the restart badge on upgrade.
    let dir = tempfile::tempdir().unwrap();
    let pubkey = "c".repeat(64);
    write_agents_json(
        dir.path(),
        &serde_json::json!([standalone_agent_json("NoPrompt", &pubkey, None)]),
    );

    let pre_records = load_typed(dir.path());
    let pre_instance = pre_records.iter().find(|r| !r.pubkey.is_empty()).unwrap();
    let before = prospective_spawn_config_snapshot(
        pre_instance,
        &[],
        &[],
        "wss://ws.example",
        &Default::default(),
        false,
    );

    backfill_standalone_agents_in_dir(&base(dir.path())).unwrap();

    let post_records = load_typed(dir.path());
    let post_instance = post_records.iter().find(|r| !r.pubkey.is_empty()).unwrap();
    let personas: Vec<AgentDefinition> = post_records
        .iter()
        .filter_map(|r| r.to_definition_view())
        .collect();
    let after = prospective_spawn_config_snapshot(
        post_instance,
        &personas,
        &[],
        "wss://ws.example",
        &Default::default(),
        false,
    );

    assert_eq!(
        before.canonical(),
        after.canonical(),
        "backfill must not flip the restart badge for prompt-less agents"
    );
}

#[test]
fn backfill_of_prompted_record_keeps_spawn_snapshot_stable() {
    // The general no-behavior-change rail: a standalone agent WITH config
    // must also snapshot identically across backfill (the definition snapshots
    // the record's own values, so the re-snapshot writes back what is
    // already there).
    let dir = tempfile::tempdir().unwrap();
    let pubkey = "d".repeat(64);
    write_agents_json(
        dir.path(),
        &serde_json::json!([standalone_agent_json(
            "Solo",
            &pubkey,
            Some("You are Solo.")
        )]),
    );

    let pre_records = load_typed(dir.path());
    let pre_instance = pre_records.iter().find(|r| !r.pubkey.is_empty()).unwrap();
    let before = prospective_spawn_config_snapshot(
        pre_instance,
        &[],
        &[],
        "wss://ws.example",
        &Default::default(),
        false,
    );

    backfill_standalone_agents_in_dir(&base(dir.path())).unwrap();

    let post_records = load_typed(dir.path());
    let post_instance = post_records.iter().find(|r| !r.pubkey.is_empty()).unwrap();
    let personas: Vec<AgentDefinition> = post_records
        .iter()
        .filter_map(|r| r.to_definition_view())
        .collect();
    let after = prospective_spawn_config_snapshot(
        post_instance,
        &personas,
        &[],
        "wss://ws.example",
        &Default::default(),
        false,
    );

    assert_eq!(before.canonical(), after.canonical());
}

#[test]
fn second_run_is_a_no_op_and_preserves_pristine_backup() {
    // B5 gates: double-run idempotence + create-if-absent .bak. Run 1
    // migrates; run 2 must change nothing and must NOT clobber the pristine
    // pre-migration backup with a half-migrated snapshot.
    let dir = tempfile::tempdir().unwrap();
    let pubkey = "e".repeat(64);
    write_agents_json(
        dir.path(),
        &serde_json::json!([standalone_agent_json("Solo", &pubkey, Some("P"))]),
    );
    let pristine = std::fs::read_to_string(base(dir.path()).join("managed-agents.json")).unwrap();

    assert_eq!(
        backfill_standalone_agents_in_dir(&base(dir.path())).unwrap(),
        1
    );
    let after_first =
        std::fs::read_to_string(base(dir.path()).join("managed-agents.json")).unwrap();

    assert_eq!(
        backfill_standalone_agents_in_dir(&base(dir.path())).unwrap(),
        0,
        "second run is a no-op"
    );
    let after_second =
        std::fs::read_to_string(base(dir.path()).join("managed-agents.json")).unwrap();
    assert_eq!(after_first, after_second, "store untouched by re-run");

    let bak =
        std::fs::read_to_string(base(dir.path()).join("managed-agents.json.pre-backfill.bak"))
            .unwrap();
    assert_eq!(
        bak, pristine,
        "backup is the PRE-migration state, never clobbered"
    );
}

#[test]
fn definitions_and_linked_records_are_untouched() {
    // Already-linked instances and existing definitions pass through
    // byte-identical; a store with nothing to backfill takes no backup.
    let dir = tempfile::tempdir().unwrap();
    let pubkey = "f".repeat(64);
    let mut linked = standalone_agent_json("Linked", &pubkey, Some("P"));
    linked["persona_id"] = serde_json::json!("some-definition");
    write_agents_json(dir.path(), &serde_json::json!([linked]));

    assert_eq!(
        backfill_standalone_agents_in_dir(&base(dir.path())).unwrap(),
        0
    );
    assert!(
        !base(dir.path())
            .join("managed-agents.json.pre-backfill.bak")
            .exists(),
        "no work, no backup"
    );
    let records = read_agents_json(dir.path());
    assert_eq!(records.len(), 1, "nothing manufactured");
}

#[test]
fn slug_collision_fails_loudly_per_record_and_continues() {
    // A pre-existing definition improbably slugged as an agent's pubkey:
    // that record is skipped (logged), the rest proceed.
    let dir = tempfile::tempdir().unwrap();
    let colliding = "1".repeat(64);
    let clean = "2".repeat(64);
    let mut definition = standalone_agent_json("Def", "", Some("P"));
    definition["slug"] = serde_json::json!(colliding.clone());
    definition["pubkey"] = serde_json::json!("");
    write_agents_json(
        dir.path(),
        &serde_json::json!([
            definition,
            standalone_agent_json("Collides", &colliding, Some("P")),
            standalone_agent_json("Clean", &clean, Some("P")),
        ]),
    );

    assert_eq!(
        backfill_standalone_agents_in_dir(&base(dir.path())).unwrap(),
        1,
        "collision skipped, clean record backfilled"
    );
    let records = load_typed(dir.path());
    let collided = records.iter().find(|r| r.pubkey == colliding).unwrap();
    assert_eq!(collided.persona_id, None, "collided record left standalone");
    let clean_rec = records.iter().find(|r| r.pubkey == clean).unwrap();
    assert_eq!(clean_rec.persona_id.as_deref(), Some(clean.as_str()));
}

#[cfg(unix)]
#[test]
fn backfill_creates_the_backup_owner_only() {
    // Same contract as the live store writer: this backup is a verbatim copy of
    // a file that carries plaintext agent nsecs when the keyring is unreachable,
    // so it is owner-only from the initial open rather than via a post-write
    // chmod that would leave a umask window.
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let pubkey = "e".repeat(64);
    let mut record = standalone_agent_json("Solo", &pubkey, Some("P"));
    record["private_key_nsec"] = serde_json::json!("nsec1exampleplaintextkey");
    write_agents_json(dir.path(), &serde_json::json!([record]));

    assert_eq!(
        backfill_standalone_agents_in_dir(&base(dir.path())).unwrap(),
        1
    );

    let bak = base(dir.path()).join("managed-agents.json.pre-backfill.bak");
    let mode = std::fs::metadata(&bak).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "backup of an inline nsec must be owner-only");
    assert!(
        std::fs::read_to_string(&bak)
            .unwrap()
            .contains("nsec1exampleplaintextkey"),
        "the fixture really did carry an inline key"
    );
}
