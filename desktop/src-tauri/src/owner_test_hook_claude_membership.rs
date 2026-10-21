//! Temporary fixed membership adapter; shares the canonical owner-hook ledger.
use super::*;

pub(super) fn pending_claude_attachment(
    hook: &Hook,
    id: &str,
    reserve: bool,
) -> Result<Request, &'static str> {
    if hook.cancel.is_cancelled() {
        return Err("shutdown");
    }
    let mut ledger = hook.ledger.lock().map_err(|_| "operation_failed")?;
    let entry = ledger.entries.get_mut(id).ok_or("not_ready")?;
    if entry.request.operation != crate::commands::owner_test_claude_lifecycle::ATTACH
        || entry.response.is_some()
        || entry.pending.is_none()
        || entry
            .deadline
            .is_none_or(|deadline| deadline <= tokio::time::Instant::now())
        || (reserve && entry.claude_membership_started)
    {
        return Err("not_ready");
    }
    crate::commands::owner_test_claude_lifecycle::validate_arguments(
        &entry.request.operation,
        &entry.request.arguments,
    )
    .map_err(|_| "not_ready")?;
    if reserve {
        entry.claude_membership_started = true;
    }
    Ok(entry.request.clone())
}

/// One admitted fixed attachment only. No caller-selected target/scope/body.
#[tauri::command]
pub(crate) async fn rowvia_owner_test_claude_membership(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    request_id: String,
) -> Result<Value, &'static str> {
    if window.label() != "main" {
        return Err("not_ready");
    }
    let hook = app
        .try_state::<Arc<Hook>>()
        .ok_or("hook_disabled")?
        .inner()
        .clone();
    let request = pending_claude_attachment(&hook, &request_id, true)?;
    let state = app.state::<crate::app_state::AppState>();
    let pending = || {
        pending_claude_attachment(&hook, &request_id, false)
            .map(drop)
            .map_err(str::to_owned)
    };
    let guard = || {
        pending()?;
        // The checker rechecks this ledger callback AFTER all blocking native
        // scope/record waits, while scope transaction locks are still held.
        crate::commands::owner_test_claude_lifecycle::check_attachment(
            &app,
            &request.arguments,
            &pending,
        )
    };
    crate::commands::add_channel_members_guarded(
        crate::commands::owner_test_claude_lifecycle::CHANNEL.into(),
        vec![crate::commands::owner_test_claude_lifecycle::TARGET.into()],
        Some("bot".into()),
        Some(crate::commands::owner_test_claude_lifecycle::RELAY.into()),
        Some(crate::commands::owner_test_claude_lifecycle::OWNER.into()),
        &state,
        &guard,
    )
    .await
    .map_err(|_| "operation_failed")
    .and_then(|result| {
        // Drop normal raw relay errors; only this closed fixed-target result
        // may cross the temporary command boundary.
        let target = crate::commands::owner_test_claude_lifecycle::TARGET;
        let failed = result
            .get("errors")
            .and_then(Value::as_array)
            .is_none_or(|e| !e.is_empty());
        let added = result
            .get("added")
            .and_then(Value::as_array)
            .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(target)));
        if failed {
            Err("operation_failed")
        } else {
            Ok(serde_json::json!({"added":if added {vec![target]} else {vec![]},"errors":[]}))
        }
    })
}
