//! B5 (unified agent model): one-time backfill of standalone agents into
//! definition-linked records. Every non-pending keyed record with `persona_id: None`
//! gets a key-less definition manufactured from its own settings, retiring
//! the standalone-create pattern (Wes's Option 2: backfill, not grandfather).

use std::path::Path;

use crate::managed_agents::{
    persona_events::{persona_content_hash, persona_event_content},
    ManagedAgentRecord,
};

fn standalone_definition(record: &ManagedAgentRecord) -> Option<(ManagedAgentRecord, String)> {
    let mut source = record.clone();
    source.slug = Some(record.pubkey.clone());
    source.definition_respond_to = Some(record.respond_to.as_str().to_string());
    source.definition_respond_to_allowlist = record.respond_to_allowlist.clone();
    source.definition_parallelism = Some(record.parallelism);
    let view = source.to_definition_view()?;
    let version = persona_content_hash(&persona_event_content(&view));
    Some((view.into_agent_record(), version))
}

fn original_pending_backfill(
    records: &[ManagedAgentRecord],
    index: usize,
) -> Option<ManagedAgentRecord> {
    let record = records.get(index)?;
    let pending = record.pending_saved_registration.as_ref()?;
    if record.persona_id.as_deref() != Some(record.pubkey.as_str())
        || records
            .iter()
            .filter(|item| item.pubkey.eq_ignore_ascii_case(&record.pubkey))
            .count()
            != 1
        || records
            .iter()
            .filter(|item| item.persona_id.as_deref() == Some(record.pubkey.as_str()))
            .count()
            != 1
        || records
            .iter()
            .filter(|item| {
                item.pending_saved_registration
                    .as_ref()
                    .is_some_and(|other| {
                        other.operation_id == pending.operation_id
                            || other.capsule_run == pending.capsule_run
                    })
            })
            .count()
            != 1
    {
        return None;
    }
    let mut definitions = records
        .iter()
        .filter(|item| item.slug.as_deref() == Some(record.pubkey.as_str()));
    let definition = definitions.next()?;
    if definitions.next().is_some() || !definition.pubkey.is_empty() || definition.shared {
        return None;
    }
    // Restore only what this migration wrote. Never adopt a fresh digest or
    // modify the held ACK, owner scope, identity, instructions or commands.
    let mut original = record.clone();
    original.persona_id = None;
    original.persona_source_version = None;
    if !crate::commands::saved_agent_backfill_restore_is_original(&original) {
        return None;
    }
    let (expected_definition, version) = standalone_definition(&original)?;
    if record.persona_source_version.as_ref() != Some(&version)
        || *definition != expected_definition
    {
        return None;
    }
    Some(original)
}

fn restore_pending_backfills(records: &mut [ManagedAgentRecord]) -> usize {
    let mut restored = 0;
    for index in 0..records.len() {
        if records[index].pending_saved_registration.is_none()
            || records[index].persona_id.is_none()
        {
            continue;
        }
        if let Some(original) = original_pending_backfill(records, index) {
            records[index] = original;
            restored += 1;
        } else {
            eprintln!("buzz-desktop: standalone-backfill: pending registration repair refused; original preparation or exact definition is unavailable");
        }
    }
    // The derived definition can have authoritative catalog or team references
    // outside this snapshot. Preserve it even after restoring the pending link.
    restored
}

/// Manufacture definitions for standalone agents (B5 backfill).
///
/// For each non-pending keyed record with `persona_id: None`, append a key-less
/// definition record snapshotting the agent's own config and link the agent
/// to it. Safety rails (pinned in the B5 review gates):
/// - **Idempotent**: linked records are skipped, so a second run is a no-op.
/// - **`.bak` create-if-absent**: the pre-migration backup is taken once and
///   never clobbered — a partial first run must not replace the pristine
///   backup with a half-migrated snapshot on re-run.
/// - **Fail loudly per record**: a record that cannot be backfilled (slug
///   collision) is logged and skipped; the rest proceed.
/// - **No behavior change**: the definition snapshots the record's own
///   values (prompt present-even-if-empty via `to_definition_view`'s
///   `unwrap_or_default`, env COPIED so later instances inherit a working
///   config, quad copied to the definition defaults) and the record gains
///   `persona_source_version` = the new definition's content hash, so
///   neither the spawn-config snapshot nor the drift badge moves.
///
/// The manufactured definition's slug is the agent's pubkey: 64-hex passes
/// the NIP-AP slug grammar on both relay and desktop ends, and agent pubkeys
/// are unique, so the coordinate is collision-free by construction.
pub fn backfill_standalone_agents(app: &tauri::AppHandle) {
    let Ok(base_dir) = crate::managed_agents::managed_agents_base_dir(app) else {
        return;
    };
    match backfill_standalone_agents_in_dir(&base_dir) {
        Ok(0) => {}
        Ok(changed) => {
            eprintln!("buzz-desktop: standalone-backfill: {changed} agent records reconciled");
        }
        Err(e) => eprintln!("buzz-desktop: standalone-backfill: {e}"),
    }
}

/// Core backfill logic, decoupled from the Tauri `AppHandle` for testing.
/// Returns the number of instances backfilled or restored (0 = nothing to do).
fn backfill_standalone_agents_in_dir(base_dir: &Path) -> Result<usize, String> {
    let agents_path = base_dir.join("managed-agents.json");
    if !agents_path.exists() {
        return Ok(0);
    }
    let content = std::fs::read_to_string(&agents_path)
        .map_err(|e| format!("failed to read managed-agents.json: {e}"))?;
    let mut all: Vec<ManagedAgentRecord> = serde_json::from_str(&content)
        .map_err(|e| format!("failed to parse managed-agents.json: {e}"))?;

    let restored = restore_pending_backfills(&mut all);
    let needs_backfill = |record: &ManagedAgentRecord| {
        !record.pubkey.is_empty()
            && record.persona_id.is_none()
            && record.pending_saved_registration.is_none()
    };
    if restored == 0 && !all.iter().any(needs_backfill) {
        return Ok(0);
    }

    // Pre-migration backup, taken ONCE: a re-run after a partial failure must
    // not overwrite the pristine backup with a half-migrated snapshot. Owner-only
    // from the initial open, and sited next to the resolved store — see
    // `create_restricted_backup_once` and `resolved_backup_path`.
    let bak_path =
        crate::util::resolved_backup_path(&agents_path, "managed-agents.json.pre-backfill.bak");
    crate::util::create_restricted_backup_once(&bak_path, content.as_bytes())
        .map_err(|e| format!("failed to write pre-backfill backup: {e}"))?;

    let existing_slugs: std::collections::HashSet<String> =
        all.iter().filter_map(|r| r.slug.clone()).collect();

    let mut manufactured: Vec<ManagedAgentRecord> = Vec::new();
    let mut backfilled = 0usize;
    for record in all.iter_mut().filter(|r| needs_backfill(r)) {
        // Pubkeys are unique so this cannot fire against another manufactured
        // definition — only against a pre-existing definition improbably
        // slugged as this agent's pubkey. Fail loudly, skip, continue: the
        // agent keeps working persona-less (`persona_id: None`), and the
        // backfill retries it on every boot. Recovery path: delete or re-slug
        // the colliding definition, then relaunch.
        if existing_slugs.contains(&record.pubkey) {
            eprintln!(
                "buzz-desktop: standalone-backfill: slug collision for agent {} — skipped; \
                 delete or re-slug the colliding definition to let the next launch backfill it",
                record.pubkey
            );
            continue;
        }

        // Snapshot the record's own config as a definition. Via the same
        // fold path every definition takes: a temporary persona view of the
        // record (prompt unwrap_or_default = present-even-if-empty — the
        // heal source old devices hard-require) folded into a key-less
        // definition record. Quad + env come along so future instances
        // minted from this definition inherit a working config.
        let Some((definition, source_version)) = standalone_definition(record) else {
            eprintln!(
                "buzz-desktop: standalone-backfill: agent {} produced no persona view — skipped",
                record.pubkey
            );
            continue;
        };

        // Link using the exact manufactured definition's content hash.
        record.persona_id = Some(record.pubkey.clone());
        record.persona_source_version = Some(source_version);
        manufactured.push(definition);
        backfilled += 1;
    }

    if backfilled == 0 && restored == 0 {
        return Ok(0);
    }
    all.extend(manufactured);
    let payload = serde_json::to_vec_pretty(&all)
        .map_err(|e| format!("failed to serialize unified store: {e}"))?;
    crate::managed_agents::atomic_write_json_restricted(&agents_path, &payload)?;
    Ok(backfilled + restored)
}

#[cfg(test)]
#[path = "backfill_tests.rs"]
mod tests;
