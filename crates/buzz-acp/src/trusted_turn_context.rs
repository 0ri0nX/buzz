//! Capability-negotiated, host-attested context for one ACP input request.
//!
//! This module carries only facts already known to `buzz-acp`. It deliberately
//! contains no authorization policy, credentials, or product-specific handles.

use serde::Serialize;
use url::{Host, Url};
use uuid::Uuid;

pub(crate) const VERSION: &str = "buzz.trusted-turn-context/v1";
pub(crate) const MAX_EVENTS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ChannelType {
    Stream,
    Dm,
}

impl ChannelType {
    pub(crate) fn from_raw(raw: &str) -> Result<Self, TrustedContextError> {
        match raw {
            "stream" | "private" => Ok(Self::Stream),
            "dm" => Ok(Self::Dm),
            _ => Err(TrustedContextError::InvalidChannelType),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EventFacts {
    pub(crate) event_id: String,
    pub(crate) author_pubkey: String,
    pub(crate) actor_pubkey: String,
    pub(crate) channel_type: ChannelType,
    pub(crate) thread_root_event_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum OwnerProvenance {
    NipOa,
    Configured,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Owner {
    pub(crate) pubkey: String,
    pub(crate) provenance: OwnerProvenance,
}

#[derive(Debug, Clone)]
pub(crate) struct ProcessFacts {
    pub(crate) relay_origin: String,
    pub(crate) agent_pubkey: String,
    pub(crate) process_instance_id: Uuid,
    pub(crate) owner: Option<Owner>,
}

#[derive(Debug, Clone)]
pub(crate) struct TurnFacts {
    pub(crate) process: ProcessFacts,
    pub(crate) channel_id: Option<Uuid>,
    pub(crate) channel_type: Option<ChannelType>,
    pub(crate) execution_scope: Option<ExecutionScope>,
    pub(crate) turn_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum ExecutionScope {
    Conversation,
    Thread {
        #[serde(rename = "rootEventId")]
        root_event_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Source {
    Message,
    Steer,
    Heartbeat,
    Bootstrap,
    Resume,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum TrustedContextError {
    #[error("trusted turn context generation overflow")]
    GenerationOverflow,
    #[error("trusted turn context has an invalid channel type")]
    InvalidChannelType,
    #[error("trusted turn context has invalid channel facts")]
    InvalidChannelFacts,
    #[error("trusted turn context has invalid event facts")]
    InvalidEventFacts,
    #[error("trusted turn context event batch is empty or exceeds its bound")]
    InvalidEventCount,
    #[error("trusted turn context relay URL is not an allowed canonical origin")]
    InvalidRelayOrigin,
    #[error("trusted turn context session id exceeds its bound")]
    InvalidSessionId,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WireOwner<'a> {
    pubkey: &'a str,
    provenance: OwnerProvenance,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WireAgent<'a> {
    pubkey: &'a str,
    process_instance_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<WireOwner<'a>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WireEvent<'a> {
    event_id: &'a str,
    author_pubkey: &'a str,
    actor_pubkey: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_root_event_id: Option<&'a str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<'a> {
    version: &'static str,
    source: Source,
    relay_origin: &'a str,
    agent: WireAgent<'a>,
    session_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    channel_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    channel_type: Option<ChannelType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    execution_scope: Option<&'a ExecutionScope>,
    turn_id: Uuid,
    generation: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    events: Option<Vec<WireEvent<'a>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_event_id: Option<&'a str>,
}

pub(crate) fn accepts_v1(result: &serde_json::Value) -> bool {
    result
        .pointer("/agentCapabilities/_meta/buzz")
        .and_then(serde_json::Value::as_object)
        .is_some_and(|buzz| {
            buzz.len() == 1
                && buzz
                    .get("trustedTurnContext")
                    .and_then(serde_json::Value::as_object)
                    .is_some_and(|context| {
                        context.len() == 1
                            && context.get("version").and_then(serde_json::Value::as_str)
                                == Some(VERSION)
                    })
        })
}

pub(crate) fn canonical_relay_origin(input: &str) -> Result<String, TrustedContextError> {
    if input.is_empty() || input.len() > 2048 {
        return Err(TrustedContextError::InvalidRelayOrigin);
    }
    let url = Url::parse(input).map_err(|_| TrustedContextError::InvalidRelayOrigin)?;
    if url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(TrustedContextError::InvalidRelayOrigin);
    }
    let authority = input
        .find("://")
        .and_then(|separator| input.get(separator + 3..))
        .ok_or(TrustedContextError::InvalidRelayOrigin)?;
    let raw_host = if authority.starts_with('[') {
        None
    } else {
        Some(
            authority
                .rsplit_once(':')
                .map_or(authority, |(host, _port)| host),
        )
    };
    if raw_host.is_some_and(|host| {
        host.bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
            && host.parse::<std::net::Ipv4Addr>().is_err()
    }) {
        return Err(TrustedContextError::InvalidRelayOrigin);
    }
    let scheme = url.scheme();
    let host = url.host().ok_or(TrustedContextError::InvalidRelayOrigin)?;
    let loopback = match host {
        Host::Domain(name) => name == "localhost",
        Host::Ipv4(ip) => ip.is_loopback(),
        Host::Ipv6(ip) => ip.is_loopback(),
    };
    if scheme != "wss" && !(scheme == "ws" && loopback) {
        return Err(TrustedContextError::InvalidRelayOrigin);
    }
    if let Host::Domain(name) = host {
        let valid_dns_name = name == "localhost"
            || (name.len() <= 253
                && !name.split('.').all(|label| {
                    !label.is_empty() && label.bytes().all(|byte| byte.is_ascii_digit())
                })
                && name.split('.').all(|label| {
                    label.len() <= 63
                        && label
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                        && label
                            .as_bytes()
                            .first()
                            .is_some_and(|byte| byte.is_ascii_alphanumeric())
                        && label
                            .as_bytes()
                            .last()
                            .is_some_and(|byte| byte.is_ascii_alphanumeric())
                }));
        if !valid_dns_name {
            return Err(TrustedContextError::InvalidRelayOrigin);
        }
    }
    let explicit_port = if authority.starts_with('[') {
        authority
            .find(']')
            .and_then(|end| authority.get(end + 1..))
            .and_then(|rest| rest.strip_prefix(':'))
    } else {
        authority.rsplit_once(':').map(|(_, port)| port)
    };
    if matches!(
        (scheme, explicit_port),
        ("wss", Some("443")) | ("ws", Some("80"))
    ) {
        return Err(TrustedContextError::InvalidRelayOrigin);
    }
    let mut origin = format!("{scheme}://");
    match host {
        Host::Ipv6(ip) => origin.push_str(&format!("[{ip}]")),
        _ => origin.push_str(&host.to_string().to_ascii_lowercase()),
    }
    if let Some(port) = url.port() {
        origin.push_str(&format!(":{port}"));
    }
    Ok(origin)
}

fn is_hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn is_contract_uuid(value: Uuid) -> bool {
    let value = value.hyphenated().to_string();
    matches!(value.as_bytes()[14], b'1'..=b'8')
        && matches!(value.as_bytes()[19], b'8' | b'9' | b'a' | b'b')
}

pub(crate) fn build_envelope(
    turn: &TurnFacts,
    session_id: &str,
    source: Source,
    generation: u32,
    events: &[EventFacts],
) -> Result<serde_json::Value, TrustedContextError> {
    if canonical_relay_origin(&turn.process.relay_origin).as_deref()
        != Ok(turn.process.relay_origin.as_str())
    {
        return Err(TrustedContextError::InvalidRelayOrigin);
    }
    if session_id.is_empty()
        || session_id.len() > 1024
        || session_id.bytes().any(|byte| byte < 0x20)
    {
        return Err(TrustedContextError::InvalidSessionId);
    }
    if generation == 0 {
        return Err(TrustedContextError::GenerationOverflow);
    }
    let carries_events = matches!(source, Source::Message | Source::Steer);
    if carries_events == events.is_empty() || events.len() > MAX_EVENTS {
        return Err(TrustedContextError::InvalidEventCount);
    }
    if source == Source::Steer && events.len() != 1 {
        return Err(TrustedContextError::InvalidEventCount);
    }
    if !is_contract_uuid(turn.turn_id)
        || !is_contract_uuid(turn.process.process_instance_id)
        || turn.channel_id.is_some_and(|id| !is_contract_uuid(id))
    {
        return Err(TrustedContextError::InvalidChannelFacts);
    }
    let channel_complete =
        turn.channel_id.is_some() && turn.channel_type.is_some() && turn.execution_scope.is_some();
    let any_channel_fact =
        turn.channel_id.is_some() || turn.channel_type.is_some() || turn.execution_scope.is_some();
    if !channel_complete && (source != Source::Heartbeat || any_channel_fact) {
        return Err(TrustedContextError::InvalidChannelFacts);
    }
    if source == Source::Heartbeat && channel_complete {
        return Err(TrustedContextError::InvalidChannelFacts);
    }
    if matches!(turn.channel_type, Some(ChannelType::Dm))
        && !matches!(turn.execution_scope, Some(ExecutionScope::Conversation))
    {
        return Err(TrustedContextError::InvalidChannelFacts);
    }
    if let Some(ExecutionScope::Thread { root_event_id }) = &turn.execution_scope {
        if !is_hex64(root_event_id) {
            return Err(TrustedContextError::InvalidChannelFacts);
        }
    }
    let channel_type = turn.channel_type;
    let mut seen = std::collections::HashSet::with_capacity(events.len());
    for event in events {
        if !is_hex64(&event.event_id)
            || !is_hex64(&event.author_pubkey)
            || !is_hex64(&event.actor_pubkey)
            || !seen.insert(&event.event_id)
            || Some(event.channel_type) != channel_type
            || match channel_type {
                Some(ChannelType::Stream) => event
                    .thread_root_event_id
                    .as_deref()
                    .is_none_or(|root| !is_hex64(root)),
                Some(ChannelType::Dm) => event.thread_root_event_id.is_some(),
                None => carries_events,
            }
        {
            return Err(TrustedContextError::InvalidEventFacts);
        }
        if let Some(ExecutionScope::Thread { root_event_id }) = turn.execution_scope.as_ref() {
            if event.thread_root_event_id.as_deref() != Some(root_event_id) {
                return Err(TrustedContextError::InvalidEventFacts);
            }
        }
    }
    if !is_hex64(&turn.process.agent_pubkey)
        || turn
            .process
            .owner
            .as_ref()
            .is_some_and(|o| !is_hex64(&o.pubkey))
    {
        return Err(TrustedContextError::InvalidEventFacts);
    }
    let wire_events: Vec<_> = events
        .iter()
        .map(|event| WireEvent {
            event_id: &event.event_id,
            author_pubkey: &event.author_pubkey,
            actor_pubkey: &event.actor_pubkey,
            thread_root_event_id: event.thread_root_event_id.as_deref(),
        })
        .collect();
    let current_event_id = events.last().map(|event| event.event_id.as_str());
    serde_json::to_value(Envelope {
        version: VERSION,
        source,
        relay_origin: &turn.process.relay_origin,
        agent: WireAgent {
            pubkey: &turn.process.agent_pubkey,
            process_instance_id: turn.process.process_instance_id,
            owner: turn.process.owner.as_ref().map(|owner| WireOwner {
                pubkey: &owner.pubkey,
                provenance: owner.provenance,
            }),
        },
        session_id,
        channel_id: turn.channel_id,
        channel_type: turn.channel_type,
        execution_scope: turn.execution_scope.as_ref(),
        turn_id: turn.turn_id,
        generation,
        events: carries_events.then_some(wire_events),
        current_event_id,
    })
    .map_err(|_| TrustedContextError::InvalidEventFacts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(owner: Option<Owner>) -> ProcessFacts {
        ProcessFacts {
            relay_origin: "wss://relay.example".into(),
            agent_pubkey: "a".repeat(64),
            process_instance_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            owner,
        }
    }

    fn stream_turn(scope: ExecutionScope) -> TurnFacts {
        TurnFacts {
            process: process(None),
            channel_id: Some(Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap()),
            channel_type: Some(ChannelType::Stream),
            execution_scope: Some(scope),
            turn_id: Uuid::parse_str("55555555-5555-4555-8555-555555555555").unwrap(),
        }
    }

    fn stream_event(id: char, root: char) -> EventFacts {
        EventFacts {
            event_id: id.to_string().repeat(64),
            author_pubkey: "e".repeat(64),
            actor_pubkey: "f".repeat(64),
            channel_type: ChannelType::Stream,
            thread_root_event_id: Some(root.to_string().repeat(64)),
        }
    }

    #[test]
    fn exact_negotiation_only() {
        let exact = serde_json::json!({"agentCapabilities":{"_meta":{"buzz":{"trustedTurnContext":{"version":VERSION}}}}});
        assert!(accepts_v1(&exact));
        assert!(!accepts_v1(&serde_json::json!({"agentCapabilities":{}})));
        assert!(!accepts_v1(
            &serde_json::json!({"agentCapabilities":{"_meta":{"buzz":{"trustedTurnContext":{"version":VERSION,"extra":true}}}}})
        ));
        assert!(!accepts_v1(
            &serde_json::json!({"agentCapabilities":{"_meta":{"buzz":{"trustedTurnContext":{"version":VERSION},"sibling":true}}}})
        ));
        assert!(!accepts_v1(
            &serde_json::json!({"agentCapabilities":{"_meta":{"buzz":"collision"}}})
        ));
        assert!(!accepts_v1(
            &serde_json::json!({"agentCapabilities":{"_meta":{"buzz":{"trustedTurnContext":"collision"}}}})
        ));
        assert!(!accepts_v1(
            &serde_json::json!({"agentCapabilities":{"_meta":{"buzz":{"trustedTurnContext":{"version":"buzz.trusted-turn-context/v2"}}}}})
        ));
    }

    #[test]
    fn raw_channel_classes_normalize_to_the_closed_semantic_pair() {
        assert_eq!(ChannelType::from_raw("stream"), Ok(ChannelType::Stream));
        assert_eq!(ChannelType::from_raw("private"), Ok(ChannelType::Stream));
        assert_eq!(ChannelType::from_raw("dm"), Ok(ChannelType::Dm));
        for rejected in ["", "group", "STREAM", "direct"] {
            assert_eq!(
                ChannelType::from_raw(rejected),
                Err(TrustedContextError::InvalidChannelType)
            );
        }
    }

    #[test]
    fn canonical_relay_policy_is_closed() {
        assert_eq!(
            canonical_relay_origin("WSS://Relay.Example/").unwrap(),
            "wss://relay.example"
        );
        assert_eq!(
            canonical_relay_origin("ws://127.0.0.1:7447").unwrap(),
            "ws://127.0.0.1:7447"
        );
        assert_eq!(
            canonical_relay_origin("ws://[::1]:7447").unwrap(),
            "ws://[::1]:7447"
        );
        for rejected in [
            "ws://relay.example",
            "wss://relay.example:443",
            "ws://localhost:80",
            "wss://user@relay.example",
            "wss://relay.example/path",
            "wss://relay.example?query",
            "wss://-relay..example",
            "wss://123.456",
        ] {
            assert_eq!(
                canonical_relay_origin(rejected),
                Err(TrustedContextError::InvalidRelayOrigin),
                "{rejected}"
            );
        }
    }

    #[test]
    fn stream_batch_preserves_order_and_newest_current() {
        let turn = stream_turn(ExecutionScope::Conversation);
        let events = vec![stream_event('1', '3'), stream_event('2', '4')];
        let envelope = build_envelope(&turn, "session", Source::Message, 1, &events).unwrap();
        assert_eq!(envelope["events"][0]["eventId"], "1".repeat(64));
        assert_eq!(envelope["events"][1]["threadRootEventId"], "4".repeat(64));
        assert_eq!(envelope["currentEventId"], "2".repeat(64));
    }

    #[test]
    fn thread_scope_rejects_mixed_roots_before_serialization() {
        let root = "3".repeat(64);
        let turn = stream_turn(ExecutionScope::Thread {
            root_event_id: root,
        });
        let error = build_envelope(
            &turn,
            "session",
            Source::Message,
            1,
            &[stream_event('1', '3'), stream_event('2', '4')],
        );
        assert_eq!(error, Err(TrustedContextError::InvalidEventFacts));
    }

    #[test]
    fn dm_has_no_thread_and_configured_owner_is_truthful() {
        let turn = TurnFacts {
            process: process(Some(Owner {
                pubkey: "b".repeat(64),
                provenance: OwnerProvenance::Configured,
            })),
            channel_id: Some(Uuid::parse_str("66666666-6666-4666-8666-666666666666").unwrap()),
            channel_type: Some(ChannelType::Dm),
            execution_scope: Some(ExecutionScope::Conversation),
            turn_id: Uuid::parse_str("77777777-7777-4777-8777-777777777777").unwrap(),
        };
        let event = EventFacts {
            event_id: "8".repeat(64),
            author_pubkey: "9".repeat(64),
            actor_pubkey: "9".repeat(64),
            channel_type: ChannelType::Dm,
            thread_root_event_id: None,
        };
        let envelope = build_envelope(&turn, "session", Source::Message, 1, &[event]).unwrap();
        assert_eq!(envelope["channelType"], "dm");
        assert_eq!(envelope["agent"]["owner"]["provenance"], "configured");
        assert!(envelope["events"][0].get("threadRootEventId").is_none());
    }

    #[test]
    fn authority_free_sources_omit_current_fields() {
        let channel_turn = stream_turn(ExecutionScope::Conversation);
        for source in [Source::Bootstrap, Source::Resume] {
            let value = build_envelope(&channel_turn, "session", source, 1, &[]).unwrap();
            assert!(value.get("events").is_none());
            assert!(value.get("currentEventId").is_none());
        }
        let heartbeat = TurnFacts {
            process: process(None),
            channel_id: None,
            channel_type: None,
            execution_scope: None,
            turn_id: Uuid::new_v4(),
        };
        let value = build_envelope(&heartbeat, "session", Source::Heartbeat, 1, &[]).unwrap();
        assert!(value.get("channelId").is_none());
    }

    #[test]
    fn invalid_or_oversize_inputs_fail() {
        let turn = stream_turn(ExecutionScope::Conversation);
        assert_eq!(
            build_envelope(
                &turn,
                "session",
                Source::Message,
                0,
                &[stream_event('1', '3')]
            ),
            Err(TrustedContextError::GenerationOverflow)
        );
        assert_eq!(
            build_envelope(
                &turn,
                &"s".repeat(1025),
                Source::Message,
                1,
                &[stream_event('1', '3')]
            ),
            Err(TrustedContextError::InvalidSessionId)
        );
        assert_eq!(
            build_envelope(
                &turn,
                "session\nsmuggle",
                Source::Message,
                1,
                &[stream_event('1', '3')]
            ),
            Err(TrustedContextError::InvalidSessionId)
        );
        assert_eq!(
            build_envelope(
                &turn,
                "session",
                Source::Message,
                1,
                &vec![stream_event('1', '3'); 65]
            ),
            Err(TrustedContextError::InvalidEventCount)
        );
        assert_eq!(
            build_envelope(
                &turn,
                "session",
                Source::Message,
                1,
                &[stream_event('1', '3'), stream_event('1', '3')]
            ),
            Err(TrustedContextError::InvalidEventFacts)
        );
    }

    #[test]
    fn constructor_rejects_closed_cross_field_negative_cases() {
        let stream = stream_turn(ExecutionScope::Conversation);
        let mut missing_root = stream_event('1', '3');
        missing_root.thread_root_event_id = None;
        assert_eq!(
            build_envelope(&stream, "session", Source::Message, 1, &[missing_root]),
            Err(TrustedContextError::InvalidEventFacts)
        );

        let mut drifted_class = stream_event('1', '3');
        drifted_class.channel_type = ChannelType::Dm;
        assert_eq!(
            build_envelope(&stream, "session", Source::Message, 1, &[drifted_class]),
            Err(TrustedContextError::InvalidEventFacts)
        );

        let mut dm = stream.clone();
        dm.channel_type = Some(ChannelType::Dm);
        let dm_with_root = stream_event('1', '3');
        assert_eq!(
            build_envelope(&dm, "session", Source::Message, 1, &[dm_with_root]),
            Err(TrustedContextError::InvalidEventFacts)
        );
        dm.execution_scope = Some(ExecutionScope::Thread {
            root_event_id: "3".repeat(64),
        });
        let dm_event = EventFacts {
            event_id: "1".repeat(64),
            author_pubkey: "e".repeat(64),
            actor_pubkey: "e".repeat(64),
            channel_type: ChannelType::Dm,
            thread_root_event_id: None,
        };
        assert_eq!(
            build_envelope(&dm, "session", Source::Message, 1, &[dm_event]),
            Err(TrustedContextError::InvalidChannelFacts)
        );

        assert_eq!(
            build_envelope(
                &stream,
                "session",
                Source::Steer,
                1,
                &[stream_event('1', '3'), stream_event('2', '4')]
            ),
            Err(TrustedContextError::InvalidEventCount)
        );
        assert_eq!(
            build_envelope(&stream, "session", Source::Heartbeat, 1, &[]),
            Err(TrustedContextError::InvalidChannelFacts)
        );

        let no_channel = TurnFacts {
            process: process(None),
            channel_id: None,
            channel_type: None,
            execution_scope: None,
            turn_id: Uuid::new_v4(),
        };
        for source in [Source::Bootstrap, Source::Resume] {
            assert_eq!(
                build_envelope(&no_channel, "session", source, 1, &[]),
                Err(TrustedContextError::InvalidChannelFacts)
            );
        }

        let mut invalid_uuid = stream.clone();
        invalid_uuid.turn_id = Uuid::nil();
        assert_eq!(
            build_envelope(
                &invalid_uuid,
                "session",
                Source::Message,
                1,
                &[stream_event('1', '3')]
            ),
            Err(TrustedContextError::InvalidChannelFacts)
        );

        let mut noncanonical_relay = stream;
        noncanonical_relay.process.relay_origin = "WSS://Relay.Example/".into();
        assert_eq!(
            build_envelope(
                &noncanonical_relay,
                "session",
                Source::Message,
                1,
                &[stream_event('1', '3')]
            ),
            Err(TrustedContextError::InvalidRelayOrigin)
        );
    }
}
