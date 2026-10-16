//! Closed target contract. Called by the native hook under its store/transition locks.
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

pub(super) const TARGET: &str = "29cc1bb0ac338520c263dcfe654764224018a1ff5e5d3fc633be73f71f0c8c78";
pub(super) const OWNER: &str = "89af092923ad49e3b9916b0c9f28c6b9237d8fcab4461cb7ddd12afe354f0d68";
pub(super) const RELAY: &str = "wss://buzz.rowvia.ai:8443";
pub(super) const CHANNEL: &str = "31986c38-6676-4d6f-a4ba-ed833926d0d5";
pub(super) const CAPSULE: &str =
    "/home/orionx/.local/state/rowvia-saved-developer-codex/buzz-worker";
pub(super) const PYTHON: &str = "/usr/bin/python3.13";
pub(super) const HARNESS_ID: &str = "rowvia-saved-developer-codex";

pub(crate) fn is_operation(operation: &str) -> bool {
    matches!(
        operation,
        "inspect_saved_developer" | "configure_saved_developer" | "start_saved_developer"
    )
}

pub(super) fn harness() -> Value {
    json!({"id": HARNESS_ID, "label": "Rowvia saved developer", "command": PYTHON,
        "args": ["-IS", format!("{CAPSULE}/run")], "env": {},
        "installInstructionsUrl": "", "installHint": "Requires the separately reviewed saved-worker capsule"})
}

pub(super) fn harness_bytes() -> Result<Vec<u8>, &'static str> {
    serde_json::to_vec_pretty(&harness()).map_err(|_| "operation_failed")
}

pub(super) struct Scope<'a> {
    pub owner: &'a str,
    pub relay: &'a str,
    pub locked: bool,
}

pub(super) struct Plan {
    pub index: usize,
    pub record: Value,
    pub fingerprint: String,
    pub configured: bool,
    pub write: bool,
    pub start: bool,
}

/// Validate scope, fixed target, collisions and the compare-before-start token.
/// The entire returned record is derived from the existing row, never caller input.
pub(crate) fn validate_arguments(
    operation: &str,
    args: &Map<String, Value>,
) -> Result<(), &'static str> {
    if !is_operation(operation)
        || (operation != "start_saved_developer" && !args.is_empty())
        || (operation == "start_saved_developer"
            && (args.len() != 1
                || !args
                    .get("fingerprint")
                    .and_then(Value::as_str)
                    .is_some_and(|s| {
                        s.len() == 64
                            && s.bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    })))
    {
        return Err("invalid_request");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn plan(
    operation: &str,
    args: &Map<String, Value>,
    scope: Scope<'_>,
    rows: &[Value],
    occupied: bool,
    existing_harness: Option<&[u8]>,
    capsule_stamp: Option<&Value>,
) -> Result<Plan, &'static str> {
    validate_arguments(operation, args)?;
    if scope.owner != OWNER || scope.relay != RELAY {
        return Err("scope_mismatch");
    }
    if scope.locked {
        return Err("not_ready");
    }
    let mut matches = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r["pubkey"] == TARGET);
    let (index, original) = matches.next().ok_or("target_missing")?;
    if matches.next().is_some() {
        return Err("target_ambiguous");
    }
    if original.get("persona_id").is_some_and(|v| !v.is_null())
        || original.get("team_id").is_some_and(|v| !v.is_null())
        || original
            .get("persona_team_dir")
            .is_some_and(|v| !v.is_null())
        || original
            .get("persona_pack_path")
            .is_some_and(|v| !v.is_null())
        || original["backend"]["type"] != "local"
        || original["acp_command"] != "buzz-acp"
        || !matches!(
            original["relay_url"].as_str(),
            Some("wss://buzz.rowvia.ai:8443" | "https://buzz.rowvia.ai:8443")
        )
    {
        return Err("target_contract_mismatch");
    }
    let expected_harness = harness_bytes()?;
    if existing_harness.is_some_and(|bytes| bytes != expected_harness) {
        return Err("harness_collision");
    }
    let mut record = original.clone();
    let object = record.as_object_mut().ok_or("target_contract_mismatch")?;
    for (key, value) in [
        ("agent_command", json!(PYTHON)),
        ("agent_command_override", json!(PYTHON)),
        ("runtime", json!(HARNESS_ID)),
        ("agent_args", json!(["-IS", format!("{CAPSULE}/run")])),
        ("model", json!("gpt-6.1-sol")),
        ("provider", json!("codex")),
        ("parallelism", json!(1)),
        ("respond_to", json!("owner-only")),
        ("relay_url", json!(RELAY)),
        ("start_on_app_launch", json!(false)),
        ("auto_restart_on_config_change", json!(false)),
    ] {
        object.insert(key.into(), value);
    }
    let env = object
        .entry("env_vars")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("target_contract_mismatch")?;
    env.insert("BUZZ_ACP_NO_BASE_PROMPT".into(), json!("true"));
    env.insert("BUZZ_ACP_CHANNELS".into(), json!(CHANNEL));
    env.insert("BUZZ_ACP_MODEL".into(), json!("gpt-6.1-sol"));
    env.insert("BUZZ_ACP_SUBSCRIBE".into(), json!("mentions"));
    env.insert("BUZZ_ACP_LAZY_POOL".into(), json!("true"));
    env.insert("BUZZ_ACP_NO_MENTION_FILTER".into(), json!("false"));
    let configured = &record == original && existing_harness.is_some();
    // Never return saved fields. The token covers configuration (including env)
    // without exposing raw values; private signing material is excluded.
    let mut public_record = record.clone();
    if let Some(object) = public_record.as_object_mut() {
        object.remove("private_key_nsec");
        object.remove("auth_tag");
    }
    let bytes = serde_json::to_vec(&json!([public_record, harness(), capsule_stamp]))
        .map_err(|_| "operation_failed")?;
    let fingerprint = hex::encode(Sha256::digest(bytes));
    let start = operation == "start_saved_developer";
    if operation != "inspect_saved_developer"
        && (occupied || original.get("runtime_pid").is_some_and(|v| !v.is_null()))
    {
        return Err("target_runtime_occupied");
    }
    if start && (!configured || args["fingerprint"] != fingerprint) {
        return Err("fingerprint_mismatch");
    }
    if start && capsule_stamp.is_none() {
        return Err("capsule_unprepared");
    }
    Ok(Plan {
        index,
        record,
        fingerprint,
        configured,
        write: operation == "configure_saved_developer" && !configured,
        start,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row() -> Value {
        json!({"pubkey": TARGET, "backend":{"type":"local"}, "acp_command":"buzz-acp", "relay_url": RELAY,
        "agent_command":"old", "env_vars":{"PRIVATE":"do-not-export", "BUZZ_ACP_LAZY_POOL":"false", "BUZZ_ACP_NO_MENTION_FILTER":"true"}, "name":"saved", "private_key_nsec":"do-not-hash"})
    }
    fn scope() -> Scope<'static> {
        Scope {
            owner: OWNER,
            relay: RELAY,
            locked: false,
        }
    }
    #[test]
    fn actual_plan_preserves_unrelated_rows_and_user_fields() {
        let unrelated =
            json!({"pubkey":"architect", "unknown":{"future":true}, "private_key_nsec":"opaque"});
        let mut rows = vec![unrelated.clone(), row()];
        let p = plan(
            "configure_saved_developer",
            &Map::new(),
            scope(),
            &rows,
            false,
            None,
            None,
        )
        .unwrap();
        assert!(p.write);
        assert!(!p.start);
        assert_eq!(p.index, 1);
        assert_eq!(p.record["env_vars"]["PRIVATE"], "do-not-export");
        assert_eq!(p.record["private_key_nsec"], "do-not-hash");
        rows[p.index] = p.record;
        assert_eq!(rows[0], unrelated);
        let h = harness_bytes().unwrap();
        let inspected = plan(
            "inspect_saved_developer",
            &Map::new(),
            scope(),
            &rows,
            false,
            Some(&h),
            Some(&json!([1])),
        )
        .unwrap();
        assert!(inspected.configured);
        let args = json!({"fingerprint":inspected.fingerprint})
            .as_object()
            .unwrap()
            .clone();
        assert!(
            plan(
                "start_saved_developer",
                &args,
                scope(),
                &rows,
                false,
                Some(&h),
                Some(&json!([1]))
            )
            .unwrap()
            .start
        );
        rows[1]["name"] = json!("changed");
        assert_eq!(
            plan(
                "start_saved_developer",
                &args,
                scope(),
                &rows,
                false,
                Some(&h),
                Some(&json!([1]))
            )
            .err(),
            Some("fingerprint_mismatch")
        );
    }
    #[test]
    fn guards_are_bound_to_production_plan() {
        let rows = vec![row()];
        for (scope, reason) in [
            (
                Scope {
                    locked: true,
                    ..scope()
                },
                "not_ready",
            ),
            (
                Scope {
                    owner: "other",
                    ..scope()
                },
                "scope_mismatch",
            ),
            (
                Scope {
                    relay: "wss://other",
                    ..scope()
                },
                "scope_mismatch",
            ),
        ] {
            assert_eq!(
                plan(
                    "configure_saved_developer",
                    &Map::new(),
                    scope,
                    &rows,
                    false,
                    None,
                    None
                )
                .err(),
                Some(reason)
            );
        }
        assert_eq!(
            plan(
                "configure_saved_developer",
                &Map::new(),
                scope(),
                &rows,
                true,
                None,
                None
            )
            .err(),
            Some("target_runtime_occupied")
        );
        assert_eq!(
            plan(
                "configure_saved_developer",
                &Map::new(),
                scope(),
                &rows,
                false,
                Some(b"collision"),
                None
            )
            .err(),
            Some("harness_collision")
        );
        assert_eq!(
            plan(
                "configure_saved_developer",
                &Map::new(),
                scope(),
                &[],
                false,
                None,
                None
            )
            .err(),
            Some("target_missing")
        );
        for field in ["pubkey", "command", "env", "eval"] {
            let args = json!({field:"forbidden"}).as_object().unwrap().clone();
            assert_eq!(
                plan(
                    "configure_saved_developer",
                    &args,
                    scope(),
                    &rows,
                    false,
                    None,
                    None
                )
                .err(),
                Some("invalid_request")
            );
        }
        let p = plan(
            "configure_saved_developer",
            &Map::new(),
            scope(),
            &rows,
            false,
            None,
            None,
        )
        .unwrap();
        let h = harness_bytes().unwrap();
        let rows = vec![p.record];
        let p = plan(
            "inspect_saved_developer",
            &Map::new(),
            scope(),
            &rows,
            false,
            Some(&h),
            None,
        )
        .unwrap();
        let args = json!({"fingerprint":p.fingerprint})
            .as_object()
            .unwrap()
            .clone();
        assert_eq!(
            plan(
                "start_saved_developer",
                &args,
                scope(),
                &rows,
                false,
                Some(&h),
                None
            )
            .err(),
            Some("capsule_unprepared")
        );
        assert_eq!(
            plan(
                "start_saved_developer",
                &args,
                scope(),
                &rows,
                true,
                Some(&h),
                None
            )
            .err(),
            Some("target_runtime_occupied")
        );
    }

    #[test]
    fn fixed_launch_rejects_other_targets_links_duplicates_and_process_metadata() {
        for (field, value) in [
            ("persona_id", json!("shared-definition")),
            ("team_id", json!("shared-team")),
            ("persona_team_dir", json!("/other/team")),
            ("persona_pack_path", json!("/legacy/team")),
            ("backend", json!({"type":"provider"})),
            ("acp_command", json!("arbitrary-executable")),
            ("relay_url", json!("wss://other")),
        ] {
            let mut value_row = row();
            value_row[field] = value;
            assert_eq!(
                plan(
                    "configure_saved_developer",
                    &Map::new(),
                    scope(),
                    &[value_row],
                    false,
                    None,
                    None
                )
                .err(),
                Some("target_contract_mismatch")
            );
        }
        let mut other = row();
        other["pubkey"] = json!("05d1901b020205f04db1ee6c434d4cd69211af3dbc1d2a46e0e1e93f81f6dd20");
        assert_eq!(
            plan(
                "configure_saved_developer",
                &Map::new(),
                scope(),
                &[other],
                false,
                None,
                None
            )
            .err(),
            Some("target_missing")
        );
        assert_eq!(
            plan(
                "configure_saved_developer",
                &Map::new(),
                scope(),
                &[row(), row()],
                false,
                None,
                None
            )
            .err(),
            Some("target_ambiguous")
        );
        let mut legacy = row();
        legacy["runtime_pid"] = json!(2345);
        assert_eq!(
            plan(
                "configure_saved_developer",
                &Map::new(),
                scope(),
                &[legacy],
                false,
                None,
                None
            )
            .err(),
            Some("target_runtime_occupied")
        );
    }

    #[test]
    fn compare_token_covers_capsule_user_config_and_exact_harness() {
        let p = plan(
            "configure_saved_developer",
            &Map::new(),
            scope(),
            &[row()],
            false,
            None,
            Some(&json!([1])),
        )
        .unwrap();
        let h = harness_bytes().unwrap();
        let rows = vec![p.record];
        let args = json!({"fingerprint":p.fingerprint})
            .as_object()
            .unwrap()
            .clone();
        assert!(
            !plan(
                "configure_saved_developer",
                &Map::new(),
                scope(),
                &rows,
                false,
                Some(&h),
                Some(&json!([1]))
            )
            .unwrap()
            .write
        );
        assert!(plan(
            "inspect_saved_developer",
            &Map::new(),
            scope(),
            &rows,
            true,
            Some(&h),
            Some(&json!([1]))
        )
        .is_ok());
        assert_eq!(
            plan(
                "start_saved_developer",
                &args,
                scope(),
                &rows,
                false,
                Some(&h),
                Some(&json!([2]))
            )
            .err(),
            Some("fingerprint_mismatch")
        );
        let mut changed = rows.clone();
        changed[0]["env_vars"]["PRIVATE"] = json!("changed");
        assert_eq!(
            plan(
                "start_saved_developer",
                &args,
                scope(),
                &changed,
                false,
                Some(&h),
                Some(&json!([1]))
            )
            .err(),
            Some("fingerprint_mismatch")
        );
        let mut changed_h = h.clone();
        changed_h.push(b'\n');
        assert_eq!(
            plan(
                "start_saved_developer",
                &args,
                scope(),
                &rows,
                false,
                Some(&changed_h),
                Some(&json!([1]))
            )
            .err(),
            Some("harness_collision")
        );
        for invalid in [
            json!({}),
            json!({"fingerprint":"bad"}),
            json!({"fingerprint":"a".repeat(64),"command":"ignored"}),
        ] {
            assert_eq!(
                validate_arguments("start_saved_developer", invalid.as_object().unwrap()),
                Err("invalid_request")
            );
        }
        assert_eq!(harness()["command"], PYTHON);
        assert_eq!(harness()["args"], json!(["-IS", format!("{CAPSULE}/run")]));
        assert_eq!(rows[0]["env_vars"]["BUZZ_ACP_NO_BASE_PROMPT"], "true");
        assert_eq!(rows[0]["env_vars"]["BUZZ_ACP_CHANNELS"], CHANNEL);
        assert_eq!(rows[0]["env_vars"]["BUZZ_ACP_MODEL"], "gpt-6.1-sol");
        assert_eq!(rows[0]["env_vars"]["BUZZ_ACP_LAZY_POOL"], "true");
        assert_eq!(rows[0]["env_vars"]["BUZZ_ACP_NO_MENTION_FILTER"], "false");
        assert_eq!(rows[0]["parallelism"], 1);
        assert_eq!(rows[0]["start_on_app_launch"], false);
    }

    #[test]
    fn configured_start_rechecks_owner_relay_saved_pid_and_tracked_ownership() {
        let configured = plan(
            "configure_saved_developer",
            &Map::new(),
            scope(),
            &[row()],
            false,
            None,
            None,
        )
        .unwrap()
        .record;
        let h = harness_bytes().unwrap();
        let stamp = json!(["capsule-fixture"]);
        let inspected = plan(
            "inspect_saved_developer",
            &Map::new(),
            scope(),
            &[configured.clone()],
            false,
            Some(&h),
            Some(&stamp),
        )
        .unwrap();
        let args = json!({"fingerprint":inspected.fingerprint})
            .as_object()
            .unwrap()
            .clone();
        for operation in [
            "inspect_saved_developer",
            "configure_saved_developer",
            "start_saved_developer",
        ] {
            let empty = Map::new();
            let arguments = if operation == "start_saved_developer" {
                &args
            } else {
                &empty
            };
            for drift in [
                Scope {
                    owner: "changed-owner",
                    ..scope()
                },
                Scope {
                    relay: "wss://changed",
                    ..scope()
                },
                Scope {
                    locked: true,
                    ..scope()
                },
            ] {
                let reason = if drift.locked {
                    "not_ready"
                } else {
                    "scope_mismatch"
                };
                assert_eq!(
                    plan(
                        operation,
                        arguments,
                        drift,
                        &[configured.clone()],
                        false,
                        Some(&h),
                        Some(&stamp)
                    )
                    .err(),
                    Some(reason)
                );
            }
            for (tracked, saved_pid) in [(true, false), (false, true), (true, true)] {
                let mut saved = configured.clone();
                if saved_pid {
                    saved["runtime_pid"] = json!(2345);
                }
                let result = plan(
                    operation,
                    arguments,
                    scope(),
                    &[saved],
                    tracked,
                    Some(&h),
                    Some(&stamp),
                );
                if operation == "inspect_saved_developer" {
                    assert!(result.is_ok());
                } else {
                    assert_eq!(result.err(), Some("target_runtime_occupied"));
                }
            }
        }
        assert!(
            plan(
                "start_saved_developer",
                &args,
                scope(),
                &[configured.clone()],
                false,
                Some(&h),
                Some(&stamp)
            )
            .unwrap()
            .start
        );
        let mut pending_edit = configured.clone();
        pending_edit["system_prompt"] = json!("new saved prompt pending next start");
        assert_eq!(
            plan(
                "start_saved_developer",
                &args,
                scope(),
                &[pending_edit],
                false,
                Some(&h),
                Some(&stamp)
            )
            .err(),
            Some("fingerprint_mismatch")
        );
        let mut pinned_edit = configured.clone();
        pinned_edit["model"] = json!("different-model");
        assert_eq!(
            plan(
                "start_saved_developer",
                &args,
                scope(),
                &[pinned_edit],
                false,
                Some(&h),
                Some(&stamp)
            )
            .err(),
            Some("fingerprint_mismatch")
        );
        assert_eq!(
            plan(
                "start_saved_developer",
                &args,
                scope(),
                &[configured],
                false,
                Some(&h),
                None
            )
            .err(),
            Some("fingerprint_mismatch")
        );
    }
}
