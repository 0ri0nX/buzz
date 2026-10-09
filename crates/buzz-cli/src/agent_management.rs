//! Owner-reviewed agent draft requests published through Buzz observer frames.

use buzz_core::observer::{encrypt_observer_payload, OBSERVER_FRAME_TELEMETRY};
use nostr::{Event, Keys, PublicKey};
use serde::Serialize;

use crate::error::CliError;

const AGENT_REQUEST_KIND: &str = "agent_management_request";
const PROJECT_CHANNEL_REQUEST_KIND: &str = "project_channel_request";
const MAX_NAME_CHARS: usize = 120;
const MAX_PROMPT_CHARS: usize = 20_000;
const MAX_GMAIL_LABEL_HINTS: usize = 4;
const MAX_GMAIL_LABEL_CHARS: usize = 120;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAgentDraft {
    pub channel_id: String,
    pub display_name: String,
    pub system_prompt: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAgentDraft {
    pub channel_id: String,
    pub agent_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub respond_to: Option<String>,
}

/// The only connector actions an agent may propose for owner review.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ConnectorDraftAction {
    Grant,
    Revoke,
}

impl ConnectorDraftAction {
    fn to_wire(self) -> &'static str {
        match self {
            Self::Grant => "connector.grant",
            Self::Revoke => "connector.revoke",
        }
    }
}

/// Human-readable selection hints; no authority or resolved IDs are accepted.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorDraft {
    pub channel_id: String,
    pub target_name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub gmail_labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectChannelDraft {
    pub home_channel_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub visibility: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagementRequest<T> {
    #[serde(rename = "type")]
    request_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<u8>,
    action: &'static str,
    request_id: String,
    request: T,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ObserverEvent<T> {
    seq: u64,
    timestamp: String,
    kind: &'static str,
    agent_index: Option<usize>,
    channel_id: Option<String>,
    session_id: Option<String>,
    turn_id: Option<String>,
    payload: ManagementRequest<T>,
}

#[derive(Debug)]
pub struct BuiltDraftRequest {
    pub event: Event,
    pub request_id: String,
    pub action: &'static str,
}

fn required(value: String, label: &str, max: usize) -> Result<String, CliError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(CliError::Usage(format!("{label} is required")));
    }
    if value.chars().count() > max {
        return Err(CliError::Usage(format!(
            "{label} is too long (max {max} characters)"
        )));
    }
    Ok(value.to_owned())
}

fn optional(value: Option<String>, label: &str) -> Result<Option<String>, CliError> {
    value.map(|value| required(value, label, 300)).transpose()
}

fn build<T: Serialize>(
    keys: &Keys,
    owner: &PublicKey,
    channel_id: String,
    request_kind: &'static str,
    version: Option<u8>,
    action: &'static str,
    request: T,
) -> Result<BuiltDraftRequest, CliError> {
    build_with_request_id(
        keys,
        owner,
        channel_id,
        (request_kind, version, action),
        request,
        None,
    )
}

fn build_with_request_id<T: Serialize>(
    keys: &Keys,
    owner: &PublicKey,
    channel_id: String,
    metadata: (&'static str, Option<u8>, &'static str),
    request: T,
    request_id: Option<String>,
) -> Result<BuiltDraftRequest, CliError> {
    let (request_kind, version, action) = metadata;
    let request_id = request_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let payload = ObserverEvent {
        seq: 0,
        timestamp: chrono::Utc::now().to_rfc3339(),
        kind: request_kind,
        agent_index: None,
        channel_id: Some(channel_id),
        session_id: None,
        turn_id: None,
        payload: ManagementRequest {
            request_type: request_kind,
            version,
            action,
            request_id: request_id.clone(),
            request,
        },
    };
    let encrypted = encrypt_observer_payload(keys, owner, &payload)
        .map_err(|error| CliError::Other(format!("could not encrypt draft request: {error}")))?;
    let event = buzz_sdk::build_agent_observer_frame(
        &owner.to_hex(),
        &keys.public_key().to_hex(),
        OBSERVER_FRAME_TELEMETRY,
        &encrypted,
    )
    .map_err(|error| CliError::Other(format!("could not build draft request: {error}")))?
    .sign_with_keys(keys)
    .map_err(|error| CliError::Other(format!("could not sign draft request: {error}")))?;
    Ok(BuiltDraftRequest {
        event,
        request_id,
        action,
    })
}

pub fn build_create(
    keys: &Keys,
    owner: &PublicKey,
    draft: CreateAgentDraft,
) -> Result<BuiltDraftRequest, CliError> {
    let channel_id = required(draft.channel_id, "channel", 128)?;
    uuid::Uuid::parse_str(&channel_id)
        .map_err(|_| CliError::Usage(format!("invalid channel UUID: {channel_id}")))?;
    let request = CreateAgentDraft {
        channel_id: channel_id.clone(),
        display_name: required(draft.display_name, "display name", MAX_NAME_CHARS)?,
        system_prompt: required(draft.system_prompt, "system prompt", MAX_PROMPT_CHARS)?,
    };
    build(
        keys,
        owner,
        channel_id,
        AGENT_REQUEST_KIND,
        None,
        "create",
        request,
    )
}

pub fn build_update(
    keys: &Keys,
    owner: &PublicKey,
    draft: UpdateAgentDraft,
) -> Result<BuiltDraftRequest, CliError> {
    let channel_id = required(draft.channel_id, "channel", 128)?;
    uuid::Uuid::parse_str(&channel_id)
        .map_err(|_| CliError::Usage(format!("invalid channel UUID: {channel_id}")))?;
    let respond_to = optional(draft.respond_to, "respond-to")?;
    if respond_to
        .as_deref()
        .is_some_and(|value| value != "owner-only" && value != "anyone")
    {
        return Err(CliError::Usage(
            "respond-to must be owner-only or anyone".into(),
        ));
    }
    let request = UpdateAgentDraft {
        channel_id: channel_id.clone(),
        agent_name: required(draft.agent_name, "agent name", MAX_NAME_CHARS)?,
        display_name: optional(draft.display_name, "display name")?,
        system_prompt: draft
            .system_prompt
            .map(|value| required(value, "system prompt", MAX_PROMPT_CHARS))
            .transpose()?,
        runtime: optional(draft.runtime, "runtime")?,
        provider: optional(draft.provider, "provider")?,
        model: optional(draft.model, "model")?,
        respond_to,
    };
    if request.display_name.is_none()
        && request.system_prompt.is_none()
        && request.runtime.is_none()
        && request.provider.is_none()
        && request.model.is_none()
        && request.respond_to.is_none()
    {
        return Err(CliError::Usage(
            "include at least one field to update".into(),
        ));
    }
    build(
        keys,
        owner,
        channel_id,
        AGENT_REQUEST_KIND,
        None,
        "update",
        request,
    )
}

/// Build an agent-signed, owner-encrypted connector proposal, never an approval.
pub fn build_connector(
    keys: &Keys,
    owner: &PublicKey,
    action: ConnectorDraftAction,
    draft: ConnectorDraft,
) -> Result<BuiltDraftRequest, CliError> {
    build_connector_with_request_id(keys, owner, action, draft, None)
}

/// Reuse a trusted, persisted observer UUIDv4 for exact draft delivery retries.
/// This ID is not a Rowvia management request ID and conveys no owner authority.
pub fn build_connector_with_request_id(
    keys: &Keys,
    owner: &PublicKey,
    action: ConnectorDraftAction,
    draft: ConnectorDraft,
    request_id: Option<String>,
) -> Result<BuiltDraftRequest, CliError> {
    let request_id = request_id
        .map(|value| {
            let parsed = uuid::Uuid::parse_str(&value)
                .map_err(|_| CliError::Usage("request ID must be a UUIDv4".into()))?;
            if parsed.get_version_num() != 4 || parsed.get_variant() != uuid::Variant::RFC4122 {
                return Err(CliError::Usage("request ID must be a UUIDv4".into()));
            }
            Ok(parsed.to_string())
        })
        .transpose()?;
    let supplied_channel_id = required(draft.channel_id, "channel", 128)?;
    let channel_id = uuid::Uuid::parse_str(&supplied_channel_id)
        .map_err(|_| CliError::Usage(format!("invalid channel UUID: {supplied_channel_id}")))?
        .to_string();
    let target_name = selection_hint(draft.target_name, "target name", MAX_NAME_CHARS)?;
    if draft.gmail_labels.len() > MAX_GMAIL_LABEL_HINTS {
        return Err(CliError::Usage(format!(
            "too many Gmail label hints (max {MAX_GMAIL_LABEL_HINTS})"
        )));
    }
    let gmail_labels = draft
        .gmail_labels
        .into_iter()
        .map(|label| selection_hint(label, "Gmail label hint", MAX_GMAIL_LABEL_CHARS))
        .collect::<Result<Vec<_>, _>>()?;
    build_with_request_id(
        keys,
        owner,
        channel_id.clone(),
        (AGENT_REQUEST_KIND, Some(1), action.to_wire()),
        ConnectorDraft {
            channel_id,
            target_name,
            gmail_labels,
        },
        request_id,
    )
}

fn selection_hint(value: String, label: &str, max: usize) -> Result<String, CliError> {
    let value = required(value, label, max)?;
    if value.chars().any(char::is_control)
        || value.contains("://")
        || value.starts_with("www.")
        || value.contains('@')
        || uuid::Uuid::parse_str(&value).is_ok()
        || (value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err(CliError::Usage(format!("{label} must be a plain name")));
    }
    Ok(value)
}

pub fn build_project_channel(
    keys: &Keys,
    owner: &PublicKey,
    draft: CreateProjectChannelDraft,
) -> Result<BuiltDraftRequest, CliError> {
    let home_channel_id = required(draft.home_channel_id, "home channel", 128)?;
    uuid::Uuid::parse_str(&home_channel_id)
        .map_err(|_| CliError::Usage(format!("invalid channel UUID: {home_channel_id}")))?;
    let visibility = required(draft.visibility, "visibility", 16)?;
    if visibility != "open" && visibility != "private" {
        return Err(CliError::Usage("visibility must be open or private".into()));
    }
    if draft.ttl_seconds == Some(0) {
        return Err(CliError::Usage("ttl must be greater than zero".into()));
    }
    let request = CreateProjectChannelDraft {
        home_channel_id: home_channel_id.clone(),
        name: required(draft.name, "name", MAX_NAME_CHARS)?,
        description: draft
            .description
            .map(|value| required(value, "description", 2_048))
            .transpose()?,
        visibility,
        ttl_seconds: draft.ttl_seconds,
        template_name: optional(draft.template_name, "template")?,
    };
    build(
        keys,
        owner,
        home_channel_id,
        PROJECT_CHANNEL_REQUEST_KIND,
        None,
        "create",
        request,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use buzz_core::observer::{decrypt_observer_payload, OBSERVER_AGENT_TAG, OBSERVER_FRAME_TAG};

    const CHANNEL: &str = "7c07e659-3610-42f4-9a5e-1e9973c09da9";

    #[test]
    fn create_is_owner_encrypted_and_matches_desktop_contract() {
        let agent = Keys::generate();
        let owner = Keys::generate();
        let built = build_create(
            &agent,
            &owner.public_key(),
            CreateAgentDraft {
                channel_id: CHANNEL.into(),
                display_name: "Research helper".into(),
                system_prompt: "Find sources.".into(),
            },
        )
        .unwrap();

        assert_eq!(built.event.kind.as_u16(), 24_200);
        let tags: Vec<Vec<String>> = built
            .event
            .tags
            .iter()
            .map(|tag| tag.as_slice().to_vec())
            .collect();
        assert!(tags
            .iter()
            .any(|tag| tag == &["p", &owner.public_key().to_hex()]));
        assert!(tags
            .iter()
            .any(|tag| tag == &[OBSERVER_AGENT_TAG, &agent.public_key().to_hex()]));
        assert!(tags
            .iter()
            .any(|tag| tag == &[OBSERVER_FRAME_TAG, OBSERVER_FRAME_TELEMETRY]));
        assert!(!tags
            .iter()
            .any(|tag| tag.first().map(String::as_str) == Some("h")));

        let payload: serde_json::Value = decrypt_observer_payload(&owner, &built.event).unwrap();
        assert_eq!(payload["kind"], AGENT_REQUEST_KIND);
        assert_eq!(payload["channelId"], CHANNEL);
        assert_eq!(payload["payload"]["type"], AGENT_REQUEST_KIND);
        assert_eq!(payload["payload"]["action"], "create");
        assert_eq!(
            payload["payload"]["request"]["displayName"],
            "Research helper"
        );
        assert!(payload["payload"]["request"].get("runtime").is_none());
        assert!(payload["payload"]["request"].get("respondTo").is_none());
    }

    #[test]
    fn update_requires_a_change() {
        let error = build_update(
            &Keys::generate(),
            &Keys::generate().public_key(),
            UpdateAgentDraft {
                channel_id: CHANNEL.into(),
                agent_name: "Scout".into(),
                display_name: None,
                system_prompt: None,
                runtime: None,
                provider: None,
                model: None,
                respond_to: None,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("at least one field"));
    }

    #[test]
    fn create_rejects_invalid_channel() {
        let error = build_create(
            &Keys::generate(),
            &Keys::generate().public_key(),
            CreateAgentDraft {
                channel_id: "general".into(),
                display_name: "Scout".into(),
                system_prompt: "Help".into(),
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("invalid channel UUID"));
    }

    #[test]
    fn project_channel_request_is_owner_encrypted() {
        let agent = Keys::generate();
        let owner = Keys::generate();
        let built = build_project_channel(
            &agent,
            &owner.public_key(),
            CreateProjectChannelDraft {
                home_channel_id: CHANNEL.into(),
                name: "release-planning".into(),
                description: Some("Coordinate the next release.".into()),
                visibility: "open".into(),
                ttl_seconds: None,
                template_name: Some("Release team".into()),
            },
        )
        .unwrap();

        let payload: serde_json::Value = decrypt_observer_payload(&owner, &built.event).unwrap();
        assert_eq!(payload["kind"], PROJECT_CHANNEL_REQUEST_KIND);
        assert_eq!(payload["channelId"], CHANNEL);
        assert_eq!(payload["payload"]["type"], PROJECT_CHANNEL_REQUEST_KIND);
        assert_eq!(payload["payload"]["action"], "create");
        assert_eq!(payload["payload"]["request"]["homeChannelId"], CHANNEL);
        assert_eq!(
            payload["payload"]["request"]["templateName"],
            "Release team"
        );
    }

    #[test]
    fn connector_draft_has_exact_versioned_hint_only_schema() {
        let agent = Keys::generate();
        let owner = Keys::generate();
        let draft = || ConnectorDraft {
            channel_id: CHANNEL.into(),
            target_name: "Research helper".into(),
            gmail_labels: vec!["Work/Reports".into()],
        };
        for (action, expected) in [
            (ConnectorDraftAction::Grant, "connector.grant"),
            (ConnectorDraftAction::Revoke, "connector.revoke"),
        ] {
            let built = build_connector(&agent, &owner.public_key(), action, draft()).unwrap();
            assert_eq!(built.event.pubkey, agent.public_key());
            assert_eq!(built.event.kind.as_u16(), 24_200);
            assert!(built.event.verify().is_ok());
            let payload: serde_json::Value =
                decrypt_observer_payload(&owner, &built.event).unwrap();
            let request_id = payload["payload"]["requestId"].as_str().unwrap();
            assert_eq!(
                uuid::Uuid::parse_str(request_id).unwrap().get_version_num(),
                4
            );
            assert_eq!(
                payload["payload"],
                serde_json::json!({
                    "type": AGENT_REQUEST_KIND,
                    "version": 1,
                    "action": expected,
                    "requestId": request_id,
                    "request": {
                        "channelId": CHANNEL,
                        "targetName": "Research helper",
                        "gmailLabels": ["Work/Reports"]
                    }
                })
            );
            assert_eq!(payload["channelId"], CHANNEL);
            assert!(payload["sessionId"].is_null());
            assert!(payload["turnId"].is_null());
            assert_ne!(
                built.event.content,
                serde_json::to_string(&payload).unwrap()
            );
        }
    }

    #[test]
    fn connector_draft_omits_empty_label_hints_and_generates_fresh_ids() {
        let agent = Keys::generate();
        let owner = Keys::generate();
        let draft = || ConnectorDraft {
            channel_id: CHANNEL.into(),
            target_name: "Research helper".into(),
            gmail_labels: Vec::new(),
        };
        let first = build_connector(
            &agent,
            &owner.public_key(),
            ConnectorDraftAction::Grant,
            draft(),
        )
        .unwrap();
        let second = build_connector(
            &agent,
            &owner.public_key(),
            ConnectorDraftAction::Grant,
            draft(),
        )
        .unwrap();
        assert_ne!(first.request_id, second.request_id);
        let payload: serde_json::Value = decrypt_observer_payload(&owner, &first.event).unwrap();
        assert!(payload["payload"]["request"].get("gmailLabels").is_none());
    }

    #[test]
    fn connector_draft_canonicalizes_accepted_channel_uuid_forms() {
        let agent = Keys::generate();
        let owner = Keys::generate();
        for supplied in [
            "7c07e659361042f49a5e1e9973c09da9",
            "7C07E659-3610-42F4-9A5E-1E9973C09DA9",
            "urn:uuid:7c07e659-3610-42f4-9a5e-1e9973c09da9",
        ] {
            let built = build_connector(
                &agent,
                &owner.public_key(),
                ConnectorDraftAction::Grant,
                ConnectorDraft {
                    channel_id: supplied.into(),
                    target_name: "Research helper".into(),
                    gmail_labels: Vec::new(),
                },
            )
            .unwrap();
            let payload: serde_json::Value =
                decrypt_observer_payload(&owner, &built.event).unwrap();
            assert_eq!(payload["channelId"], CHANNEL);
            assert_eq!(payload["payload"]["request"]["channelId"], CHANNEL);
        }
    }

    #[test]
    fn connector_draft_retry_id_is_exact_and_uuidv4_only() {
        let agent = Keys::generate();
        let owner = Keys::generate();
        let draft = || ConnectorDraft {
            channel_id: CHANNEL.into(),
            target_name: "Research helper".into(),
            gmail_labels: Vec::new(),
        };
        let supplied = "550E8400E29B41D4A716446655440000";
        let expected = "550e8400-e29b-41d4-a716-446655440000";
        for _ in 0..2 {
            let built = build_connector_with_request_id(
                &agent,
                &owner.public_key(),
                ConnectorDraftAction::Grant,
                draft(),
                Some(supplied.into()),
            )
            .unwrap();
            let payload: serde_json::Value =
                decrypt_observer_payload(&owner, &built.event).unwrap();
            assert_eq!(built.request_id, expected);
            assert_eq!(payload["payload"]["requestId"], expected);
        }
        for invalid in [
            "not-a-uuid",
            "7c07e659-3610-72f4-9a5e-1e9973c09da9",
            "550e8400-e29b-41d4-0716-446655440000",
            "7c07e659-3610-42f4-9a5e-1e9973c09da9-extra",
        ] {
            assert!(build_connector_with_request_id(
                &agent,
                &owner.public_key(),
                ConnectorDraftAction::Grant,
                draft(),
                Some(invalid.into()),
            )
            .is_err());
        }
    }

    #[test]
    fn connector_draft_rejects_non_hint_values_and_bounds() {
        let agent = Keys::generate();
        let owner = Keys::generate();
        let make = |target_name: String, gmail_labels: Vec<String>| ConnectorDraft {
            channel_id: CHANNEL.into(),
            target_name,
            gmail_labels,
        };
        for target in [
            "".to_owned(),
            "x".repeat(MAX_NAME_CHARS + 1),
            "https://example.test".to_owned(),
            "someone@example.test".to_owned(),
            CHANNEL.to_owned(),
            "a".repeat(64),
            "name\nsecret".to_owned(),
        ] {
            assert!(build_connector(
                &agent,
                &owner.public_key(),
                ConnectorDraftAction::Grant,
                make(target, Vec::new())
            )
            .is_err());
        }
        for labels in [
            vec!["Inbox".into(); MAX_GMAIL_LABEL_HINTS + 1],
            vec!["".into()],
            vec!["x".repeat(MAX_GMAIL_LABEL_CHARS + 1)],
            vec!["https://example.test".into()],
            vec!["token@example.test".into()],
        ] {
            assert!(build_connector(
                &agent,
                &owner.public_key(),
                ConnectorDraftAction::Revoke,
                make("Research helper".into(), labels)
            )
            .is_err());
        }
        assert!(build_connector(
            &agent,
            &owner.public_key(),
            ConnectorDraftAction::Grant,
            ConnectorDraft {
                channel_id: "wrong".into(),
                ..make("Research helper".into(), Vec::new())
            }
        )
        .is_err());
    }
}
