use super::*;
use crate::managed_agents::SavedAgentRecipe;
use native::RowviaSavedAgentPreparation;

const ID: &str = "01940000-0000-7000-8000-000000000077";
fn canonical21() -> String {
    let value = serde_json::json!({
        "agent_credential_id": "01940000-0000-7000-8000-000000000012",
        "agent_name": format!("saved-{}", ID.replace('-', "")), "agent_pubkey": "a".repeat(64),
        "agent_verifier_sha256": "3".repeat(64), "credential_ttl_seconds": 86400,
        "default_space_id": "01940000-0000-7000-8000-000000000005", "harness_kind": "custom-acp",
        "lifecycle_actions": ["agent.instance.start", "agent.instance.stop", "runtime.instance.register", "runtime.instance.stop"],
        "manifest_version": 1, "operation_id": ID, "operator_profile": "localprofile/v1",
        "owner_principal_id": "01940000-0000-7000-8000-000000000004", "owner_pubkey": "b".repeat(64),
        "owner_subject_id": "01940000-0000-7000-8000-000000000002", "retry_key_sha256": "4".repeat(64),
        "runtime_config_digest": "e".repeat(64), "source_instance": "fixture",
        "tenant_id": "01940000-0000-7000-8000-000000000001", "version": "rowvia.saved-local-registration/v1",
        "workload_credential_id": "01940000-0000-7000-8000-000000000013", "workload_verifier_sha256": "5".repeat(64)
    });
    serde_json::to_string(&value).unwrap()
}
fn canonical_hash(text: &str) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(text.as_bytes()))
}
fn cohort() -> Resumed {
    let expected = RowviaSavedAgentScopeExpectation {
        selection: RowviaSavedAgentPreparation {
            operation_id: ID.into(),
            owner_pubkey: "b".repeat(64),
            relay_url: "wss://relay.example".into(),
            source_instance: "fixture".into(),
            channel_id: "01940000-0000-7000-8000-000000000088".into(),
            name: "Original Ω name".into(),
            system_prompt: "Owner-reviewed prompt".into(),
            recipe: SavedAgentRecipe::CodexLocalV1,
            model: "gpt-6.1-sol".into(),
            workspace: format!("/owned/workspaces/{ID}"),
            capsule_run: format!("/owned/operations/{ID}/capsule/buzz-worker/run"),
        },
        agent_pubkey: "a".repeat(64),
        native_config_sha256: "c".repeat(64),
        saved_authorization_sha256: "d".repeat(64),
    };
    let prepared = PreparedRowviaSavedAgent {
        operation_id: ID.into(),
        pubkey: expected.agent_pubkey.clone(),
        core_agent_name: format!("saved-{}", ID.replace('-', "")),
        saved_authorization_sha256: expected.saved_authorization_sha256.clone(),
        native_config_sha256: expected.native_config_sha256.clone(),
        created_stopped: true,
        bindings_ready: false,
        runtime_ready: false,
    };
    let ack = SavedAgentPreparationAck {
        version: 1,
        operation_id: ID.into(),
        agent_pubkey: expected.agent_pubkey.clone(),
        native_config_sha256: expected.native_config_sha256.clone(),
        saved_authorization_sha256: expected.saved_authorization_sha256.clone(),
        recipe_sha256: "e".repeat(64),
        request_sha256: "f".repeat(64),
        canonical21_sha256: canonical_hash(&canonical21()),
        prepared_receipt_sha256: "2".repeat(64),
    };
    Resumed {
        expected,
        prepared,
        ack,
    }
}
struct Owned {
    events: Vec<&'static str>,
    body: Option<Vec<u8>>,
    response: Vec<u8>,
    failed: Option<&'static str>,
    host_drift: bool,
    native_drift: bool,
    sent: bool,
}
impl Owned {
    fn new() -> Self {
        Self { events: vec![], body: None,
            response: format!(r#"{{"version":1,"operation_id":"{ID}","state":"authorized","expires_at":"2030-01-01T00:00:00Z"}}"#).into_bytes(),
            failed: None, host_drift: false, native_drift: false, sent: false }
    }
    fn event(&mut self, name: &'static str) -> Result<(), Failure> {
        self.events.push(name);
        if self.failed == Some(name) {
            Err(Failure::Unavailable)
        } else {
            Ok(())
        }
    }
}
impl OwnerBoundary for Owned {
    fn preflight(&mut self) -> Result<(), Failure> {
        self.event("preflight")
    }
    fn resume(&mut self, _: &str) -> Result<Resumed, Failure> {
        self.event("resume")?;
        Ok(cohort())
    }
    async fn host_review(&mut self, _: &Resumed) -> Result<HostReviewedPreparation, Failure> {
        self.event("host_review")?;
        let mut ack = cohort().ack;
        if self.host_drift {
            ack.request_sha256 = "3".repeat(64);
        }
        let canonical21 = canonical21();
        Ok(HostReviewedPreparation {
            version: 1,
            operation_id: ID.into(),
            canonical21_sha256: canonical_hash(&canonical21),
            canonical21,
            ack,
        })
    }
    fn revalidate(&mut self, _: &Resumed) -> Result<(), Failure> {
        self.event("revalidate")?;
        if self.native_drift && self.sent {
            Err(Failure::Unavailable)
        } else {
            Ok(())
        }
    }
    async fn authorize(&mut self, _: &Resumed, body: Vec<u8>) -> Result<Vec<u8>, Failure> {
        self.sent = true;
        self.body = Some(body);
        self.event("authorize")?;
        Ok(self.response.clone())
    }
    fn now(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(1_767_225_600, 0).unwrap()
    }
}

#[test]
fn owner_input_is_one_map_field_canonical_uuid_only() {
    assert!(
        serde_json::from_str::<SavedAgentOwnerInput>(&format!(r#"{{"operationId":"{ID}"}}"#))
            .is_ok()
    );
    for raw in [
        format!(r#"{{"operationId":"{ID}","hash":"arbitrary"}}"#),
        format!(r#"{{"operationId":"{ID}","operationId":"{ID}"}}"#),
        format!(r#"["{ID}"]"#),
        r#"{"operationId":"00000000-0000-0000-0000-000000000000"}"#.into(),
        format!(
            r#"{{"operationId":"{}"}}"#,
            ID.replacen("01940000", "0194a000", 1).to_uppercase()
        ),
        "{}".into(),
        "null".into(),
    ] {
        assert!(
            serde_json::from_str::<SavedAgentOwnerInput>(&raw).is_err(),
            "{raw}"
        );
    }
}
#[test]
fn owner_installation_is_unconfigured_without_io_or_fallback() {
    #[cfg(target_os = "linux")]
    assert!(!prepare_caller::production::qualification_valid(
        &[],
        "",
        "/fixture/owner-entrypoint.mjs",
        "/fixture",
        "/fixture/artifacts"
    ));
}
#[tokio::test]
async fn owner_review_preserves_original_and_has_no_sign_or_mutation_boundary() {
    let mut b = Owned::new();
    let view = review_with(&mut b, ID).await.unwrap();
    assert_eq!(view.name, "Original Ω name");
    assert_eq!(view.system_prompt, "Owner-reviewed prompt");
    assert_eq!(view.canonical21_sha256, canonical_hash(&canonical21()));
    assert_eq!(view.canonical21, canonical21());
    assert_eq!(
        b.events,
        [
            "preflight",
            "resume",
            "host_review",
            "preflight",
            "revalidate"
        ]
    );
    assert!(!b.sent && b.body.is_none());
    assert!(view.created_stopped && !view.bindings_ready && !view.runtime_ready);
}
#[tokio::test]
async fn owner_authorization_body_is_exact_core_contract_and_canonical_digest() {
    let mut b = Owned::new();
    let result = approve_with(&mut b, ID).await.unwrap();
    assert_eq!(result.state, "authorized");
    let raw = b.body.unwrap();
    let expected = format!(
        r#"{{"version":1,"operation_id":"{ID}","agent_pubkey":"{}","harness_kind":"custom-acp","config_digest":"{}"}}"#,
        "a".repeat(64),
        canonical_hash(&canonical21())
    );
    assert_eq!(raw, expected.as_bytes());
    assert_ne!(
        serde_json::from_slice::<serde_json::Value>(&raw).unwrap()["config_digest"],
        "e".repeat(64)
    );
    assert_eq!(b.events.iter().filter(|e| **e == "authorize").count(), 1);
}

#[tokio::test]
async fn owner_post_authorization_preconditions_stay_unknown_without_retry_or_cas() {
    for failed_check in ["expiry", "resume", "cohort"] {
        let mut boundary = Owned::new();
        let authorization = approve_with(&mut boundary, ID).await.unwrap();
        assert_eq!(authorization.state, "authorized");
        let mut attempts = 0;
        let result: Result<(), Failure> = after_owner_authorization(|| {
            attempts += 1;
            // Whole-boundary synthetic failure before child register or CAS.
            // The explicit owner POST above has already succeeded once.
            boundary.events.push(failed_check);
            Err(Failure::Unavailable)
        });
        assert_eq!(result, Err(Failure::Unknown), "{failed_check}");
        assert_eq!(attempts, 1);
        assert_eq!(
            boundary
                .events
                .iter()
                .filter(|event| **event == "authorize")
                .count(),
            1
        );
        assert_eq!(boundary.events.last(), Some(&failed_check));
        assert!(!boundary
            .events
            .iter()
            .any(|event| ["register", "cas"].contains(event)));
    }
}
#[tokio::test]
async fn owner_invalid_operation_refuses_before_boundary() {
    let mut b = Owned::new();
    assert_eq!(approve_with(&mut b, "bad").await, Err(Failure::Unavailable));
    assert!(b.events.is_empty());
}
#[tokio::test]
async fn owner_unconfigured_preflight_refuses_before_snapshot_or_http() {
    let mut b = Owned::new();
    b.failed = Some("preflight");
    assert_eq!(approve_with(&mut b, ID).await, Err(Failure::Unavailable));
    assert_eq!(b.events, ["preflight"]);
}
#[tokio::test]
async fn owner_missing_durable_resume_never_contacts_host_or_signer() {
    let mut b = Owned::new();
    b.failed = Some("resume");
    assert_eq!(approve_with(&mut b, ID).await, Err(Failure::Unavailable));
    assert!(!b.sent);
    assert!(!b.events.contains(&"host_review"));
}
#[tokio::test]
async fn owner_host_drift_in_any_ack_field_refuses_before_signing() {
    let mut b = Owned::new();
    b.host_drift = true;
    assert_eq!(approve_with(&mut b, ID).await, Err(Failure::Unavailable));
    assert!(!b.sent);
}
#[tokio::test]
async fn owner_pre_dispatch_revalidation_refuses_without_signing() {
    let mut b = Owned::new();
    b.failed = Some("revalidate");
    assert_eq!(approve_with(&mut b, ID).await, Err(Failure::Unavailable));
    assert!(!b.sent);
}
#[tokio::test]
async fn owner_dispatch_failure_is_unknown_once_and_has_sanitized_error() {
    let mut b = Owned::new();
    b.failed = Some("authorize");
    let error = approve_with(&mut b, ID).await.unwrap_err();
    assert_eq!(error.public(), UNKNOWN);
    assert!(b.sent);
    assert_eq!(b.events.iter().filter(|e| **e == "authorize").count(), 1);
}
#[tokio::test]
async fn owner_post_dispatch_scope_drift_is_unknown_not_ready() {
    let mut b = Owned::new();
    b.native_drift = true;
    assert_eq!(approve_with(&mut b, ID).await, Err(Failure::Unknown));
    assert!(b.sent);
}
#[tokio::test]
async fn owner_http_result_rejects_duplicate_extra_sequence_and_wrong_cohort() {
    for raw in [
        "[]".into(), "{}".into(), "secret diagnostic".into(),
        r#"{"version":1,"operation_id":"01940000-0000-7000-8000-000000000078","state":"authorized","expires_at":"2030-01-01T00:00:00Z"}"#.into(),
        format!(r#"{{"version":1,"version":1,"operation_id":"{ID}","state":"authorized","expires_at":"2030-01-01T00:00:00Z"}}"#),
        format!(r#"{{"version":1,"operation_id":"{ID}","state":"authorized","expires_at":"2030-01-01T00:00:00Z","ready":true}}"#),
        format!(r#"{{"version":1,"operation_id":"{ID}","state":"ready","expires_at":"2030-01-01T00:00:00Z"}}"#),
        format!(r#"{{"version":1,"operation_id":"{ID}","state":"authorized","expires_at":"2020-01-01T00:00:00Z"}}"#),
        "x".repeat(2049),
    ] {
        let mut b = Owned::new(); b.response = raw.into_bytes();
        assert_eq!(approve_with(&mut b, ID).await, Err(Failure::Unknown));
        assert_eq!(b.events.iter().filter(|e| **e == "authorize").count(), 1);
    }
}
#[tokio::test]
async fn owner_known_expired_or_revoked_is_not_invented_provisioning_readiness() {
    for state in ["expired", "revoked"] {
        let mut b = Owned::new();
        b.response = format!(r#"{{"version":1,"operation_id":"{ID}","state":"{state}","expires_at":"2020-01-01T00:00:00Z"}}"#).into_bytes();
        let result = approve_with(&mut b, ID).await.unwrap();
        assert_eq!(result.state, state);
        let view = serde_json::to_value(result).unwrap();
        assert!(view.get("runtimeReady").is_none() && view.get("registered").is_none());
    }
}
#[test]
fn owner_native_projection_rejects_inconsistent_ack_and_readiness() {
    let mut c = cohort();
    c.ack.native_config_sha256 = "9".repeat(64);
    assert_eq!(c.project(ID), Err(Failure::Unavailable));
    let mut c = cohort();
    c.prepared.runtime_ready = true;
    assert_eq!(c.project(ID), Err(Failure::Unavailable));
}

#[test]
fn owner_public_canonical21_is_bounded_exact_typed_and_bound_to_original_cohort() {
    let original = canonical21();
    assert!(public::verify_canonical21(&original, &canonical_hash(&original), &cohort()).is_ok());
    for text in [
        format!(" {original}"),
        original.replace("\"manifest_version\":1", "\"manifest_version\":1.0"),
        original.replace(
            "\"manifest_version\":1",
            "\"manifest_version\":1,\"manifest_version\":1",
        ),
        original.replace(&"a".repeat(64), &"9".repeat(64)),
        original.replace(&"b".repeat(64), &"9".repeat(64)),
        original.replace(&"e".repeat(64), &"9".repeat(64)),
        "x".repeat(4097),
        "[]".into(),
    ] {
        let mut c = cohort();
        let hash = canonical_hash(&text);
        c.ack.canonical21_sha256 = hash.clone();
        assert_eq!(
            public::verify_canonical21(&text, &hash, &c),
            Err(Failure::Unavailable)
        );
    }
    assert_eq!(
        public::verify_canonical21(&original, &"9".repeat(64), &cohort()),
        Err(Failure::Unavailable)
    );
}

#[test]
fn saved_recipe_enum_preserves_legacy_bytes_and_refuses_unknown_variants() {
    for (recipe, text) in [
        (SavedAgentRecipe::CodexLocalV1, "codex-local-v1"),
        (SavedAgentRecipe::ClaudeLocalV1, "claude-local-v1"),
        (
            SavedAgentRecipe::CodexDevelopmentMemoryGitReadV1,
            "codex-development-memory-git-read-v1",
        ),
    ] {
        let wire = format!("\"{text}\"");
        assert_eq!(serde_json::to_string(&recipe).unwrap(), wire);
        assert_eq!(
            serde_json::from_str::<SavedAgentRecipe>(&wire).unwrap(),
            recipe
        );
    }
    assert!(serde_json::from_str::<SavedAgentRecipe>("\"codex-development-v2\"").is_err());
    assert_ne!(
        SavedAgentRecipe::CodexLocalV1,
        SavedAgentRecipe::CodexDevelopmentMemoryGitReadV1
    );
}

#[test]
fn saved_codex_request_is_closed_map_and_never_reinterprets_old_recipe() {
    let valid = r#"{"displayName":"Codex Ω","systemPrompt":"Original 🧭 prompt","channelId":"01940000-0000-7000-8000-000000000088","recipe":"codex-development-memory-git-read-v1"}"#;
    let selected: SavedAgentSaveInput = serde_json::from_str(valid).unwrap();
    assert_eq!(selected.display_name, "Codex Ω");
    assert_eq!(selected.system_prompt, "Original 🧭 prompt");
    for text in [
        valid.replace("codex-development-memory-git-read-v1", "codex-local-v1"),
        valid.replace("codex-development-memory-git-read-v1", "claude-local-v1"),
        valid.replace("codex-development-memory-git-read-v1", "unknown"),
        valid.replace("\"displayName\":", "\"command\":\"sh\",\"displayName\":"),
        valid.replace(
            "\"displayName\":",
            "\"displayName\":\"duplicate\",\"displayName\":",
        ),
        "[]".into(),
    ] {
        assert!(serde_json::from_str::<SavedAgentSaveInput>(&text).is_err());
    }
}
