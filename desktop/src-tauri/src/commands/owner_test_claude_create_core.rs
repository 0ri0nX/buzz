//! Temporary closed contract for creating one stopped normal Claude developer.
use serde_json::{json, Map, Value};

pub(crate) const OPERATION: &str = "create_stopped_claude_developer";
pub(super) const NAME: &str = "Rowvia Developer Claude";
pub(super) const OWNER: &str = "89af092923ad49e3b9916b0c9f28c6b9237d8fcab4461cb7ddd12afe354f0d68";
pub(super) const RELAY: &str = "wss://buzz.rowvia.ai:8443";
pub(super) const HARNESS_ID: &str = "rowvia-saved-claude-normal";
pub(super) const PYTHON: &str = "/usr/bin/python3.13";
pub(super) const RUN: &str = "/home/orionx/.local/state/rowvia-saved-claude-normal/buzz-worker/run";

pub(crate) fn validate_at(args: &Map<String, Value>, now: u128) -> Result<(), &'static str> {
    let expiry = args
        .get("expiresAt")
        .and_then(Value::as_u64)
        .ok_or("invalid_request")?;
    if args.len() != 1 || u128::from(expiry) <= now || u128::from(expiry) > now + 120_000 {
        return Err("invalid_request");
    }
    Ok(())
}

pub(super) fn check_scope(owner: &str, relay: &str, unlocked: bool) -> Result<(), &'static str> {
    if !unlocked {
        return Err("not_ready");
    }
    if owner != OWNER || relay != RELAY {
        return Err("scope_mismatch");
    }
    Ok(())
}

pub(super) fn refuse_target<'a>(
    rows: impl IntoIterator<Item = (&'a str, &'a str, Option<&'a str>, Option<&'a str>)>,
) -> Result<(), &'static str> {
    if rows
        .into_iter()
        .any(|(name, command, override_command, runtime)| {
            name.trim() == NAME
                || command == HARNESS_ID
                || override_command == Some(HARNESS_ID)
                || runtime == Some(HARNESS_ID)
        })
    {
        return Err("target_collision");
    }
    Ok(())
}

pub(super) fn request() -> Value {
    json!({"name": NAME, "relayUrl": RELAY, "agentCommand": PYTHON,
        "personaId": null, "teamId": null, "agentArgs": ["-IS", RUN], "envVars": {},
        "spawnAfterCreate": false, "startOnAppLaunch": false})
}

pub(super) fn harness() -> Value {
    json!({"id": HARNESS_ID, "label": NAME, "command": PYTHON,
        "args": ["-IS", RUN], "env": {}, "installInstructionsUrl": "", "installHint": ""})
}

pub(super) fn public_result(pubkey: String, profile_sync_succeeded: bool) -> Map<String, Value> {
    Map::from_iter([
        ("pubkey".into(), Value::String(pubkey)),
        ("name".into(), Value::String(NAME.into())),
        ("createdStopped".into(), Value::Bool(true)),
        ("startOnAppLaunch".into(), Value::Bool(false)),
        (
            "profileSyncSucceeded".into(),
            Value::Bool(profile_sync_succeeded),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expiry_and_selectors_are_closed() {
        let now = 10_000;
        for (expiry, valid) in [
            (10_000, false),
            (10_001, true),
            (130_000, true),
            (130_001, false),
        ] {
            let args = json!({"expiresAt": expiry}).as_object().unwrap().clone();
            assert_eq!(validate_at(&args, now).is_ok(), valid);
        }
        for args in [
            json!({}),
            json!({"expiresAt": null}),
            json!({"expiresAt": 10_001.5}),
            json!({"expiresAt": "10001"}),
            json!({"expiresAt": 10_001,"command":"secret"}),
        ] {
            assert!(validate_at(args.as_object().unwrap(), now).is_err());
        }
        let args = json!({"expiresAt": 10_001}).as_object().unwrap().clone();
        assert!(validate_at(&args, 10_000).is_ok());
        assert!(
            validate_at(&args, 10_001).is_err(),
            "must expire again at mutation boundary"
        );
    }

    #[test]
    fn actual_scope_and_lock_drift_fail_closed() {
        assert!(check_scope(OWNER, RELAY, true).is_ok());
        for (owner, relay, unlocked) in [
            ("foreign", RELAY, true),
            (OWNER, "wss://foreign", true),
            (OWNER, RELAY, false),
        ] {
            assert!(check_scope(owner, relay, unlocked).is_err());
        }
    }

    #[test]
    fn fixed_target_refuses_name_harness_and_ambiguous_restarted_rows() {
        assert!(refuse_target([("Other", "goose", None, None)]).is_ok());
        for row in [
            (NAME, "goose", None, None),
            (" Rowvia Developer Claude ", "goose", None, None),
            ("Renamed", HARNESS_ID, None, None),
            ("Renamed", "goose", Some(HARNESS_ID), None),
            ("Renamed", "goose", None, Some(HARNESS_ID)),
        ] {
            assert!(refuse_target([row]).is_err());
            assert!(refuse_target([row, row]).is_err());
        }
        // A caller restart loses its request ledger; persisted target facts still refuse it.
        let disk = json!([[NAME, HARNESS_ID, null, null]]).to_string();
        let rows: Vec<(String, String, Option<String>, Option<String>)> =
            serde_json::from_str(&disk).unwrap();
        assert!(refuse_target(rows.iter().map(|r| (
            r.0.as_str(),
            r.1.as_str(),
            r.2.as_deref(),
            r.3.as_deref()
        )))
        .is_err());
    }

    #[test]
    fn fixed_create_has_no_start_or_provider_configuration_and_exact_harness() {
        let input = request();
        assert_eq!(input["name"], NAME);
        assert_eq!(input["agentCommand"], PYTHON);
        assert_eq!(input["agentArgs"], json!(["-IS", RUN]));
        assert_eq!(input["spawnAfterCreate"], false);
        assert_eq!(input["startOnAppLaunch"], false);
        assert_eq!(input["personaId"], Value::Null);
        assert!(input.get("provider").is_none());
        assert!(input.get("model").is_none());
        assert_eq!(harness()["command"], PYTHON);
        assert_eq!(harness()["args"], json!(["-IS", RUN]));
        assert_eq!(harness()["env"], json!({}));
    }

    #[test]
    fn public_result_has_exact_keys_and_claims_only_stopped_creation() {
        let result = public_result("a".repeat(64), false);
        let mut keys = result.keys().map(String::as_str).collect::<Vec<_>>();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "createdStopped",
                "name",
                "profileSyncSucceeded",
                "pubkey",
                "startOnAppLaunch"
            ]
        );
        assert_eq!(result["createdStopped"], true);
        assert_eq!(result["profileSyncSucceeded"], false);
    }
}
