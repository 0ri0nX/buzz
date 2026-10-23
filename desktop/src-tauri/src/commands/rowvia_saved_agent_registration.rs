//! Invocation-local callback boundary for the fixed owner-host Node parent.
//! Preparation may retain once; resumed continuations only capture/revalidate.
//! Installation qualification remains separate from the callback protocol.

use super::rowvia_saved_agent::{
    capture_rowvia_saved_agent_scope, retain_rowvia_saved_agent_preparation_ack,
    revalidate_resumed_rowvia_saved_agent_preparation_ack,
    revalidate_rowvia_saved_agent_preparation_ack, ConfirmedRowviaSavedAgentPreparationAck,
    PreparedRowviaSavedAgent, RowviaSavedAgentPreparation, RowviaSavedAgentScope,
    RowviaSavedAgentScopeExpectation,
};
use crate::managed_agents::{PendingSavedAgentRegistration, SavedAgentPreparationAck};
use serde::{
    de::{MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use std::fmt;
use tauri::AppHandle;

const MAX_LINE: usize = 64 * 1024;
const MAX_TRAFFIC: usize = 1024 * 1024;
const MAX_ID: u16 = 128;

/// Local errors contain no native diagnostic, credential, path or wire payload.
/// The owner-Save parent must treat any post-native-save failure as an
/// uncertain overall result, including a readonly callback refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallbackFailure {
    Protocol,
    Unknown,
}

enum RequestKind {
    Capture,
    Retain(SavedAgentPreparationAck),
    Revalidate(SavedAgentPreparationAck),
}

struct ChildRequest {
    id: u16,
    kind: RequestKind,
}

impl<'de> Deserialize<'de> for ChildRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RequestVisitor;
        impl<'de> Visitor<'de> for RequestVisitor {
            type Value = ChildRequest;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a closed callback request object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                use serde::de::Error;
                let (mut version, mut id, mut kind, mut ack) = (None, None, None, None);
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "version" if version.is_none() => version = Some(map.next_value::<u8>()?),
                        "id" if id.is_none() => id = Some(map.next_value::<u16>()?),
                        "kind" if kind.is_none() => kind = Some(map.next_value::<String>()?),
                        // Reuse C1v2's map-only, exact nine-field ACK decoder.
                        "ack" if ack.is_none() => {
                            ack = Some(map.next_value::<SavedAgentPreparationAck>()?)
                        }
                        _ => return Err(M::Error::custom("invalid callback request")),
                    }
                }
                if version != Some(1) || !matches!(id, Some(1..=MAX_ID)) {
                    return Err(M::Error::custom("invalid callback request"));
                }
                let request = match (kind.as_deref(), ack) {
                    (Some("capture"), None) => RequestKind::Capture,
                    (Some("retain"), Some(ack)) => RequestKind::Retain(ack),
                    (Some("revalidate"), Some(ack)) => RequestKind::Revalidate(ack),
                    _ => return Err(M::Error::custom("invalid callback request")),
                };
                Ok(ChildRequest {
                    id: id.ok_or_else(|| M::Error::custom("invalid callback request"))?,
                    kind: request,
                })
            }
        }
        deserializer.deserialize_map(RequestVisitor)
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum ParentResponse {
    Scope {
        version: u8,
        id: u16,
        scope: RowviaSavedAgentScope,
    },
    Ack {
        version: u8,
        id: u16,
        ack: SavedAgentPreparationAck,
        confirmed: bool,
    },
    Refused {
        version: u8,
        id: u16,
    },
    Unknown {
        version: u8,
        id: u16,
    },
}

// Production is fixed to the complete C2 boundary. Only this module's tests can
// substitute a whole explicit fake boundary; no caller chooses individual hooks.
trait C2Boundary {
    fn capture(
        &mut self,
        expected: &RowviaSavedAgentScopeExpectation,
    ) -> Result<RowviaSavedAgentScope, ()>;
    fn retain(
        &mut self,
        expected: &RowviaSavedAgentScopeExpectation,
        ack: &SavedAgentPreparationAck,
    ) -> Result<ConfirmedRowviaSavedAgentPreparationAck, ()>;
    fn revalidate(
        &mut self,
        expected: &RowviaSavedAgentScopeExpectation,
        ack: &SavedAgentPreparationAck,
    ) -> Result<ConfirmedRowviaSavedAgentPreparationAck, ()>;
}

struct NativeC2Boundary<'a, R: tauri::Runtime>(&'a AppHandle<R>);
impl<R: tauri::Runtime> C2Boundary for NativeC2Boundary<'_, R> {
    fn capture(
        &mut self,
        expected: &RowviaSavedAgentScopeExpectation,
    ) -> Result<RowviaSavedAgentScope, ()> {
        capture_rowvia_saved_agent_scope(self.0, expected).map_err(|_| ())
    }
    fn retain(
        &mut self,
        expected: &RowviaSavedAgentScopeExpectation,
        ack: &SavedAgentPreparationAck,
    ) -> Result<ConfirmedRowviaSavedAgentPreparationAck, ()> {
        retain_rowvia_saved_agent_preparation_ack(self.0, expected, ack).map_err(|_| ())
    }
    fn revalidate(
        &mut self,
        expected: &RowviaSavedAgentScopeExpectation,
        ack: &SavedAgentPreparationAck,
    ) -> Result<ConfirmedRowviaSavedAgentPreparationAck, ()> {
        revalidate_rowvia_saved_agent_preparation_ack(self.0, expected, ack).map_err(|_| ())
    }
}

// Resumed owner continuations may capture and revalidate existing evidence,
// but may never retain a replacement ACK or repair a legacy pending record.
struct ResumedNativeC2Boundary<'a, R: tauri::Runtime>(&'a AppHandle<R>);
impl<R: tauri::Runtime> C2Boundary for ResumedNativeC2Boundary<'_, R> {
    fn capture(
        &mut self,
        expected: &RowviaSavedAgentScopeExpectation,
    ) -> Result<RowviaSavedAgentScope, ()> {
        capture_rowvia_saved_agent_scope(self.0, expected).map_err(|_| ())
    }
    fn retain(
        &mut self,
        _: &RowviaSavedAgentScopeExpectation,
        _: &SavedAgentPreparationAck,
    ) -> Result<ConfirmedRowviaSavedAgentPreparationAck, ()> {
        Err(())
    }
    fn revalidate(
        &mut self,
        expected: &RowviaSavedAgentScopeExpectation,
        ack: &SavedAgentPreparationAck,
    ) -> Result<ConfirmedRowviaSavedAgentPreparationAck, ()> {
        revalidate_resumed_rowvia_saved_agent_preparation_ack(self.0, expected, ack).map_err(|_| ())
    }
}

/// Original trusted selection custody lasts only for this invocation. This type
/// is deliberately not Deserialize, Clone, a persistent store, or a recovery API.
pub(crate) struct NativeRegistrationCallbacks {
    expected: RowviaSavedAgentScopeExpectation,
    line: Vec<u8>,
    incoming: usize,
    outgoing: usize,
    next_id: u16,
    response_pending: bool,
    captured: bool,
    retained: Option<SavedAgentPreparationAck>,
    terminal: bool,
    input_finished: bool,
}

impl NativeRegistrationCallbacks {
    fn from_resumed(
        original: &RowviaSavedAgentPreparation,
        prepared: &PreparedRowviaSavedAgent,
        ack: &SavedAgentPreparationAck,
    ) -> Result<Self, CallbackFailure> {
        let mut holder = Self::from_prepared(original, prepared)?;
        ack.validate_pending_cohort(
            &pending_projection(&holder.expected),
            &holder.expected.agent_pubkey,
        )
        .map_err(|_| CallbackFailure::Protocol)?;
        holder.retained = Some(ack.clone());
        Ok(holder)
    }
    /// Caller must supply the ORIGINAL validated owner selection passed to C2
    /// preparation, including its display name, plus that same preparation's
    /// result. The wire has no constructor or expectation-replacement operation.
    pub(crate) fn from_prepared(
        original: &RowviaSavedAgentPreparation,
        prepared: &PreparedRowviaSavedAgent,
    ) -> Result<Self, CallbackFailure> {
        if prepared.operation_id != original.operation_id
            || prepared.core_agent_name
                != format!("saved-{}", original.operation_id.replace('-', ""))
            || !prepared.created_stopped
            || prepared.bindings_ready
            || prepared.runtime_ready
        {
            return Err(CallbackFailure::Protocol);
        }
        let expected = RowviaSavedAgentScopeExpectation {
            selection: original.clone(),
            agent_pubkey: prepared.pubkey.clone(),
            native_config_sha256: prepared.native_config_sha256.clone(),
            saved_authorization_sha256: prepared.saved_authorization_sha256.clone(),
        };
        let operation =
            uuid::Uuid::parse_str(&original.operation_id).map_err(|_| CallbackFailure::Protocol)?;
        if operation.get_version_num() != 7
            || operation.get_variant() != uuid::Variant::RFC4122
            || operation.to_string() != original.operation_id
            || [
                &expected.agent_pubkey,
                &expected.native_config_sha256,
                &expected.saved_authorization_sha256,
            ]
            .into_iter()
            .any(|value| {
                value.len() != 64
                    || !value
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    || value.bytes().all(|b| b == b'0')
            })
        {
            return Err(CallbackFailure::Protocol);
        }
        Ok(Self {
            expected,
            line: Vec::new(),
            incoming: 0,
            outgoing: 0,
            next_id: 1,
            response_pending: false,
            captured: false,
            retained: None,
            terminal: false,
            input_finished: false,
        })
    }

    fn protocol_failure<T>(&mut self) -> Result<T, CallbackFailure> {
        self.terminal = true;
        self.input_finished = false;
        self.line.clear();
        Err(CallbackFailure::Protocol)
    }

    fn receive_with(
        &mut self,
        bytes: &[u8],
        boundary: &mut impl C2Boundary,
    ) -> Result<Option<Vec<u8>>, CallbackFailure> {
        if self.terminal || self.response_pending {
            return self.protocol_failure();
        }
        self.incoming = self.incoming.saturating_add(bytes.len());
        if self.incoming > MAX_TRAFFIC || self.line.len().saturating_add(bytes.len()) > MAX_LINE {
            return self.protocol_failure();
        }
        self.line.extend_from_slice(bytes);
        let Some(newline) = self.line.iter().position(|byte| *byte == b'\n') else {
            return Ok(None);
        };
        // A pipelined/trailing frame is refused before even the first callback.
        if newline + 1 != self.line.len() {
            return self.protocol_failure();
        }
        let request = std::str::from_utf8(&self.line[..newline])
            .ok()
            .and_then(|line| serde_json::from_str::<ChildRequest>(line).ok());
        self.line.clear();
        let Some(request) = request else {
            return self.protocol_failure();
        };
        if request.id != self.next_id {
            return self.protocol_failure();
        }
        match &request.kind {
            RequestKind::Capture => {}
            RequestKind::Retain(ack) | RequestKind::Revalidate(ack) => {
                if !self.captured
                    || ack
                        .validate_pending_cohort(
                            &pending_projection(&self.expected),
                            &self.expected.agent_pubkey,
                        )
                        .is_err()
                {
                    return self.protocol_failure();
                }
                match &request.kind {
                    RequestKind::Retain(_) if self.retained.is_some() => {
                        return self.protocol_failure()
                    }
                    RequestKind::Revalidate(_) if self.retained.as_ref() != Some(ack) => {
                        return self.protocol_failure()
                    }
                    _ => {}
                }
            }
        }
        self.next_id += 1;
        self.response_pending = true;
        let response = match request.kind {
            RequestKind::Capture => match boundary.capture(&self.expected) {
                Ok(scope) => {
                    self.captured = true;
                    ParentResponse::Scope {
                        version: 1,
                        id: request.id,
                        scope,
                    }
                }
                Err(()) => {
                    self.terminal = true;
                    ParentResponse::Refused {
                        version: 1,
                        id: request.id,
                    }
                }
            },
            RequestKind::Retain(ack) => match boundary.retain(&self.expected, &ack) {
                Ok(confirmation) if confirmation.confirmed && confirmation.ack == ack => {
                    self.retained = Some(ack.clone());
                    ParentResponse::Ack {
                        version: 1,
                        id: request.id,
                        ack,
                        confirmed: true,
                    }
                }
                // Do not parse native error strings to guess whether it committed.
                _ => {
                    self.terminal = true;
                    ParentResponse::Unknown {
                        version: 1,
                        id: request.id,
                    }
                }
            },
            RequestKind::Revalidate(ack) => match boundary.revalidate(&self.expected, &ack) {
                Ok(confirmation) if confirmation.confirmed && confirmation.ack == ack => {
                    ParentResponse::Ack {
                        version: 1,
                        id: request.id,
                        ack,
                        confirmed: true,
                    }
                }
                _ => {
                    self.terminal = true;
                    ParentResponse::Refused {
                        version: 1,
                        id: request.id,
                    }
                }
            },
        };
        let Ok(mut bytes) = serde_json::to_vec(&response) else {
            self.terminal = true;
            return Err(CallbackFailure::Unknown);
        };
        bytes.push(b'\n');
        self.outgoing = self.outgoing.saturating_add(bytes.len());
        if bytes.len() > MAX_LINE || self.outgoing > MAX_TRAFFIC {
            self.terminal = true;
            return Err(CallbackFailure::Unknown);
        }
        Ok(Some(bytes))
    }

    /// Called only by the future trusted pipe owner after the entire returned
    /// frame has been written AND flushed. It does not reset a terminal refusal.
    pub(crate) fn response_flushed(&mut self) -> Result<(), CallbackFailure> {
        if !self.response_pending {
            return self.protocol_failure();
        }
        self.response_pending = false;
        Ok(())
    }

    /// A lost/partial response can conceal a durable retain. Never replay it.
    pub(crate) fn delivery_failed(&mut self) -> CallbackFailure {
        self.terminal = true;
        self.input_finished = false;
        self.response_pending = false;
        self.line.clear();
        CallbackFailure::Unknown
    }

    pub(crate) fn finish_input(&mut self) -> Result<(), CallbackFailure> {
        if self.terminal || self.response_pending || !self.line.is_empty() {
            return self.protocol_failure();
        }
        self.terminal = true;
        self.input_finished = true;
        Ok(())
    }

    // One-shot final custody gate, not a new callback verb or retention path.
    fn finalize_prepared_with(
        &mut self,
        operation_id: &str,
        agent_pubkey: &str,
        recipe_sha256: &str,
        canonical21_sha256: &str,
        boundary: &mut impl C2Boundary,
    ) -> Result<(), CallbackFailure> {
        if !self.input_finished || self.response_pending || !self.captured {
            return self.protocol_failure();
        }
        let Some(ack) = self.retained.as_ref() else {
            return self.protocol_failure();
        };
        if ack.operation_id != operation_id
            || ack.agent_pubkey != agent_pubkey
            || ack.recipe_sha256 != recipe_sha256
            || ack.canonical21_sha256 != canonical21_sha256
        {
            return self.protocol_failure();
        }
        match boundary.revalidate(&self.expected, ack) {
            Ok(confirmation) if confirmation.confirmed && confirmation.ack == *ack => {
                self.input_finished = false;
                Ok(())
            }
            _ => self.protocol_failure(),
        }
    }
}

#[cfg(target_os = "linux")]
#[path = "rowvia_saved_agent_prepare_caller.rs"]
pub(crate) mod prepare_caller;

fn pending_projection(
    expected: &RowviaSavedAgentScopeExpectation,
) -> PendingSavedAgentRegistration {
    let input = &expected.selection;
    PendingSavedAgentRegistration {
        operation_id: input.operation_id.clone(),
        owner_pubkey: input.owner_pubkey.clone(),
        relay_url: input.relay_url.clone(),
        source_instance: input.source_instance.clone(),
        channel_id: input.channel_id.clone(),
        recipe: input.recipe,
        model: input.model.clone(),
        workspace: input.workspace.clone(),
        capsule_run: input.capsule_run.clone(),
        saved_authorization_sha256: expected.saved_authorization_sha256.clone(),
        native_config_sha256: expected.native_config_sha256.clone(),
        prepared_ack: None,
    }
}

#[cfg(test)]
#[path = "rowvia_saved_agent_registration_tests.rs"]
mod tests;
