//! Closed, temporary saved-Claude workflow. No key/tag enters this contract.
use serde_json::{json, Map, Value};

pub(super) const TARGET: &str = "01ab717b2ebeeef89c3abde0be5a55f6aa95fac66d89c96124083ad381857b4c";
pub(super) const INSPECT_INITIAL: &str = "inspect_saved_claude_initial_state";
pub(super) const CHANNEL: &str = "31986c38-6676-4d6f-a4ba-ed833926d0d5";
pub(super) const OWNER: &str = "89af092923ad49e3b9916b0c9f28c6b9237d8fcab4461cb7ddd12afe354f0d68";
pub(super) const RELAY: &str = "wss://buzz.rowvia.ai:8443";
pub(super) const PYTHON: &str = "/usr/bin/python3.13";
pub(super) const RUN: &str = "/home/orionx/.local/state/rowvia-saved-claude-normal/buzz-worker/run";
pub(super) const SETUP_DIR: &str = "/var/tmp/rowvia-native-auth-setup-claude-GUGVxJY4";
pub(super) const SETUP: &str =
    "/var/tmp/rowvia-native-auth-setup-claude-GUGVxJY4/rowvia-native-auth-setup";
pub(super) const PROFILE: &str = "/var/tmp/rowvia-native-auth-setup-claude-GUGVxJY4/profile.json";
pub(super) const SETUP_SHA: &str =
    "ea3aa4b6f17e16800e22de5998e8b1b8f738269e75ade4a880d62ec8ab95a394";
pub(super) const PROFILE_SHA: &str =
    "65848d77c42d1b474dc2a381afe65652d0813cf3787fc34a66485da55e2a8326";

pub(crate) fn is_operation(op: &str) -> bool {
    matches!(
        op,
        INSPECT_INITIAL
            | "attach_saved_claude_developer"
            | "prepare_saved_claude_setup"
            | "start_saved_claude_setup"
            | "stop_restore_saved_claude_normal"
            | "start_saved_claude_normal"
    )
}

pub(super) fn check_scope(owner: &str, relay: &str, unlocked: bool) -> Result<(), &'static str> {
    if owner != OWNER || relay != RELAY {
        return Err("scope_mismatch");
    }
    if !unlocked {
        return Err("not_ready");
    }
    Ok(())
}

pub(super) fn validate_at(
    op: &str,
    args: &Map<String, Value>,
    now: u128,
) -> Result<(), &'static str> {
    let expires = args
        .get("expiresAt")
        .and_then(Value::as_u64)
        .ok_or("invalid_request")?;
    if !is_operation(op)
        || args.len() != 1
        || u128::from(expires) <= now
        || u128::from(expires) > now + 120_000
    {
        return Err("invalid_request");
    }
    Ok(())
}

pub(super) fn command(setup: bool) -> (&'static str, Vec<String>) {
    if setup {
        (SETUP, vec!["--setup-profile".into(), PROFILE.into()])
    } else {
        (PYTHON, vec!["-IS".into(), RUN.into()])
    }
}

pub(super) fn update(setup: bool) -> Value {
    let (command, args) = command(setup);
    json!({"pubkey":TARGET,"agentCommand":command,"agentArgs":args,"envVars":configured_env(),"parallelism":1,"harnessOverride":true})
}

pub(super) fn configured_env() -> std::collections::BTreeMap<String, String> {
    std::collections::BTreeMap::from([("BUZZ_ACP_NO_BASE_PROMPT".into(), "true".into())])
}

pub(super) fn public_result(op: &str) -> Map<String, Value> {
    let mut result = Map::from_iter([("pubkey".into(), json!(TARGET))]);
    match op {
        "prepare_saved_claude_setup" => {
            result.insert("setupConfigured".into(), json!(true));
            result.insert("startOnAppLaunch".into(), json!(false));
        }
        "stop_restore_saved_claude_normal" => {
            result.insert("stopped".into(), json!(true));
            result.insert("normalConfigured".into(), json!(true));
            result.insert("startOnAppLaunch".into(), json!(false));
        }
        "start_saved_claude_setup" | "start_saved_claude_normal" => {
            result.insert("relayUrl".into(), json!(RELAY));
            result.insert("startDispatched".into(), json!(true));
            if op == "start_saved_claude_setup" {
                result.insert("setupOnly".into(), json!(true));
            }
        }
        _ => unreachable!("validated native operation"),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn claude_lifecycle_expiry_selectors_and_exact_setup_descriptor() {
        assert!(check_scope(OWNER, RELAY, true).is_ok());
        assert!(check_scope("foreign", RELAY, true).is_err());
        assert!(check_scope(OWNER, "wss://foreign.invalid", true).is_err());
        assert!(check_scope(OWNER, RELAY, false).is_err());
        for op in [
            "inspect_saved_claude_initial_state",
            "attach_saved_claude_developer",
            "prepare_saved_claude_setup",
            "start_saved_claude_setup",
            "stop_restore_saved_claude_normal",
            "start_saved_claude_normal",
        ] {
            let mut args = json!({"expiresAt":1100}).as_object().unwrap().clone();
            assert!(validate_at(op, &args, 1000).is_ok());
            assert!(validate_at(op, &args, 1100).is_err());
            args.insert("command".into(), json!("nsec1_CANARY"));
            assert!(validate_at(op, &args, 1000).is_err());
        }
        assert_eq!(
            command(true),
            (
                SETUP,
                vec!["--setup-profile".to_owned(), PROFILE.to_owned()]
            )
        );
        assert_eq!(
            command(false),
            (PYTHON, vec!["-IS".to_owned(), RUN.to_owned()])
        );
        for setup in [true, false] {
            let patch = update(setup);
            assert_eq!(patch.as_object().unwrap().len(), 6);
            assert_eq!(patch["envVars"], json!({"BUZZ_ACP_NO_BASE_PROMPT":"true"}));
            assert_eq!(patch["parallelism"], json!(1));
            assert!(patch.get("privateKeyNsec").is_none());
        }
    }
    #[test]
    fn claude_lifecycle_public_results_are_closed() {
        for (op, keys) in [
            (
                "prepare_saved_claude_setup",
                vec!["pubkey", "setupConfigured", "startOnAppLaunch"],
            ),
            (
                "stop_restore_saved_claude_normal",
                vec!["normalConfigured", "pubkey", "startOnAppLaunch", "stopped"],
            ),
            (
                "start_saved_claude_setup",
                vec!["pubkey", "relayUrl", "setupOnly", "startDispatched"],
            ),
            (
                "start_saved_claude_normal",
                vec!["pubkey", "relayUrl", "startDispatched"],
            ),
        ] {
            assert_eq!(
                public_result(op)
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                keys
            );
        }
    }
}
