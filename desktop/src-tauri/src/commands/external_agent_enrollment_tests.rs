use super::*;
use nostr::Timestamp;

fn entry(owner: &Keys, agent: &Keys, issued_at: u64) -> ExternalAgentEnrollment {
    ExternalAgentEnrollment {
        agent_pubkey: agent.public_key().to_hex(),
        owner_pubkey: owner.public_key().to_hex(),
        relay_url: "wss://relay.example".into(),
        issued_at,
    }
}

fn proof(signer: &Keys, challenge: &str, owner: &Keys, created_at: u64) -> Event {
    EventBuilder::new(Kind::TextNote, challenge)
        .tags([Tag::parse(["p", owner.public_key().to_hex().as_str()]).unwrap()])
        .custom_created_at(Timestamp::from(created_at))
        .sign_with_keys(signer)
        .unwrap()
}

#[test]
fn accepts_only_fresh_agent_signed_proof_for_the_named_owner() {
    let owner = Keys::generate();
    let agent = Keys::generate();
    let other = Keys::generate();
    let issued_at = 1_700_000_000;
    let challenge = "buzz:external-agent-enrollment:v1:test";
    let pending = entry(&owner, &agent, issued_at);
    assert!(verify_proof(
        &proof(&agent, challenge, &owner, issued_at),
        challenge,
        &pending,
        issued_at
    )
    .is_ok());
    assert!(
        verify_proof(
            &proof(&other, challenge, &owner, issued_at),
            challenge,
            &pending,
            issued_at
        )
        .is_err(),
        "forged agent identity"
    );
    assert!(
        verify_proof(
            &proof(&agent, challenge, &other, issued_at),
            challenge,
            &pending,
            issued_at
        )
        .is_err(),
        "wrong owner"
    );
    let duplicate_owner = EventBuilder::new(Kind::TextNote, challenge)
        .tags([
            Tag::parse(["p", owner.public_key().to_hex().as_str()]).unwrap(),
            Tag::parse(["p", owner.public_key().to_hex().as_str()]).unwrap(),
        ])
        .custom_created_at(Timestamp::from(issued_at))
        .sign_with_keys(&agent)
        .unwrap();
    assert!(
        verify_proof(&duplicate_owner, challenge, &pending, issued_at).is_err(),
        "duplicate owner tags"
    );
    assert!(
        verify_proof(
            &proof(&agent, "another challenge", &owner, issued_at),
            challenge,
            &pending,
            issued_at
        )
        .is_err(),
        "wrong challenge"
    );
    assert!(
        verify_proof(
            &proof(&agent, challenge, &owner, issued_at - 1),
            challenge,
            &pending,
            issued_at
        )
        .is_err(),
        "pre-challenge signature"
    );
    assert!(
        verify_proof(
            &proof(&agent, challenge, &owner, issued_at),
            challenge,
            &pending,
            issued_at + ENROLLMENT_TTL_SECONDS + 1
        )
        .is_err(),
        "expired challenge"
    );
    assert!(
        verify_proof(
            &proof(&agent, challenge, &owner, issued_at + 31),
            challenge,
            &pending,
            issued_at
        )
        .is_err(),
        "future proof"
    );
}

fn seed(state: &AppState, challenge: &str, owner: &Keys, agent: &Keys, issued_at: u64) {
    state
        .external_agent_enrollments
        .lock()
        .unwrap()
        .insert(challenge.to_string(), entry(owner, agent, issued_at));
}

fn input(name: &str, challenge: &str, proof: &Event) -> CompleteExternalAgentEnrollment {
    CompleteExternalAgentEnrollment {
        name: name.into(),
        challenge: challenge.into(),
        proof_event_json: proof.as_json(),
    }
}

#[test]
fn completion_recovers_public_tag_without_rewriting_announcement_or_spawning_acp() {
    let owner = Keys::generate();
    let agent = Keys::generate();
    let state = crate::app_state::build_app_state();
    let issued_at = 1_700_000_000;
    let challenge = "buzz:external-agent-enrollment:v1:first";
    seed(&state, challenge, &owner, &agent, issued_at);
    let signed_proof = proof(&agent, challenge, &owner, issued_at);
    let temp = tempfile::tempdir().unwrap();
    let conn = open_retention_db(&temp.path().join("retention.db")).unwrap();
    let result = complete_external_agent_enrollment_at(
        input("Remote Scout", challenge, &signed_proof),
        &state,
        &conn,
        &owner,
        "wss://relay.example",
        false,
        issued_at,
    )
    .unwrap();
    let tag = result.auth_tag;
    let tag_fields: Vec<String> = serde_json::from_str(&tag).unwrap();
    assert_eq!(tag_fields[0], "auth");
    assert_eq!(tag_fields[1], owner.public_key().to_hex());
    assert_eq!(tag_fields[2], "kind=0");
    let compat_agent = nostr::PublicKey::from_hex(&agent.public_key().to_hex()).unwrap();
    let verified_owner = buzz_sdk_pkg::nip_oa::verify_auth_tag(&tag, &compat_agent).unwrap();
    assert_eq!(verified_owner.to_hex(), owner.public_key().to_hex());
    let retained = get_retained_event(
        &conn,
        AGENT_KIND,
        &owner.public_key().to_hex(),
        &agent.public_key().to_hex(),
    )
    .unwrap()
    .unwrap();
    let announcement: Event = serde_json::from_str(&retained.raw_event).unwrap();
    announcement.verify().unwrap();
    assert_eq!(announcement.pubkey, owner.public_key());
    assert_eq!(announcement.kind, Kind::Custom(30177));
    assert!(verified_external_announcement(
        &retained,
        &announcement,
        &owner.public_key().to_hex(),
        "wss://relay.example"
    )
    .is_some());
    assert!(verified_external_announcement(
        &retained,
        &announcement,
        &owner.public_key().to_hex(),
        "wss://another-relay.example"
    )
    .is_none());
    assert!(verified_external_announcement(
        &retained,
        &announcement,
        &Keys::generate().public_key().to_hex(),
        "wss://relay.example"
    )
    .is_none());
    assert!(retained.pending_sync);
    assert!(
        !temp.path().join("managed-agents.json").exists(),
        "external enrollment must not write a managed ACP record"
    );
    let recovered = complete_external_agent_enrollment_at(
        input("Remote Scout", challenge, &signed_proof),
        &state,
        &conn,
        &owner,
        "wss://relay.example",
        false,
        issued_at + 1000,
    )
    .unwrap();
    assert_eq!(recovered.agent_pubkey, agent.public_key().to_hex());
    assert_eq!(
        get_retained_event(
            &conn,
            AGENT_KIND,
            &owner.public_key().to_hex(),
            &agent.public_key().to_hex()
        )
        .unwrap()
        .unwrap()
        .raw_event,
        retained.raw_event
    );

    // Recovery still works when the old proof was lost: a fresh challenge and
    // fresh proof by the same signer return the tag without changing policy.
    let next_challenge = "buzz:external-agent-enrollment:v1:second";
    seed(&state, next_challenge, &owner, &agent, issued_at + 1000);
    let next_proof = proof(&agent, next_challenge, &owner, issued_at + 1000);
    complete_external_agent_enrollment_at(
        input("Remote Scout", next_challenge, &next_proof),
        &state,
        &conn,
        &owner,
        "wss://relay.example",
        false,
        issued_at + 1000,
    )
    .unwrap();
    assert!(
        complete_external_agent_enrollment_at(
            input("Remote Scout", next_challenge, &next_proof),
            &state,
            &conn,
            &owner,
            "wss://relay.example",
            false,
            issued_at + 1000,
        )
        .is_err(),
        "fresh recovery proof is one-use"
    );
    assert_eq!(
        get_retained_event(
            &conn,
            AGENT_KIND,
            &owner.public_key().to_hex(),
            &agent.public_key().to_hex()
        )
        .unwrap()
        .unwrap()
        .raw_event,
        retained.raw_event
    );
    assert!(complete_external_agent_enrollment_at(
        input("Changed name", challenge, &signed_proof),
        &state,
        &conn,
        &owner,
        "wss://relay.example",
        false,
        issued_at + 1000,
    )
    .is_err());
    assert!(
        complete_external_agent_enrollment_at(
            input("Remote Scout", challenge, &signed_proof),
            &state,
            &conn,
            &owner,
            "wss://another-relay.example",
            false,
            issued_at + 1000,
        )
        .is_err(),
        "retained evidence is bound to its relay"
    );
}

#[test]
fn completion_rejects_forged_proof_owner_drift_managed_collision_and_large_input() {
    let owner = Keys::generate();
    let agent = Keys::generate();
    let stranger = Keys::generate();
    let state = crate::app_state::build_app_state();
    let issued_at = 1_700_000_000;
    let temp = tempfile::tempdir().unwrap();
    let conn = open_retention_db(&temp.path().join("retention.db")).unwrap();
    let challenge = "buzz:external-agent-enrollment:v1:guard";
    let signed = proof(&agent, challenge, &owner, issued_at);

    seed(&state, challenge, &owner, &agent, issued_at);
    assert!(
        complete_external_agent_enrollment_at(
            input("Remote", challenge, &signed),
            &state,
            &conn,
            &stranger,
            "wss://relay.example",
            false,
            issued_at
        )
        .is_err(),
        "owner drift"
    );
    seed(&state, challenge, &owner, &agent, issued_at);
    assert!(
        complete_external_agent_enrollment_at(
            input("Remote", challenge, &signed),
            &state,
            &conn,
            &owner,
            "wss://other.example",
            false,
            issued_at
        )
        .is_err(),
        "relay drift"
    );
    seed(&state, challenge, &owner, &agent, issued_at);
    assert!(
        complete_external_agent_enrollment_at(
            input("Remote", challenge, &signed),
            &state,
            &conn,
            &owner,
            "wss://relay.example",
            true,
            issued_at
        )
        .is_err(),
        "managed key collision"
    );
    seed(&state, challenge, &owner, &agent, issued_at);
    let forged = proof(&stranger, challenge, &owner, issued_at);
    assert!(
        complete_external_agent_enrollment_at(
            input("Remote", challenge, &forged),
            &state,
            &conn,
            &owner,
            "wss://relay.example",
            false,
            issued_at
        )
        .is_err(),
        "wrong signer"
    );
    assert!(
        complete_external_agent_enrollment_at(
            input("Remote", challenge, &signed),
            &state,
            &conn,
            &owner,
            "wss://relay.example",
            false,
            issued_at,
        )
        .is_err(),
        "failed proof consumed the challenge"
    );
    seed(&state, challenge, &owner, &agent, issued_at);
    let mut tampered: serde_json::Value = serde_json::from_str(&signed.as_json()).unwrap();
    tampered["id"] = serde_json::json!("00".repeat(32));
    let mut bad = input("Remote", challenge, &signed);
    bad.proof_event_json = tampered.to_string();
    assert!(
        complete_external_agent_enrollment_at(
            bad,
            &state,
            &conn,
            &owner,
            "wss://relay.example",
            false,
            issued_at
        )
        .is_err(),
        "tampered id"
    );
    seed(&state, challenge, &owner, &agent, issued_at);
    let mut tampered_sig: serde_json::Value = serde_json::from_str(&signed.as_json()).unwrap();
    tampered_sig["sig"] = serde_json::json!("00".repeat(64));
    let mut bad_sig = input("Remote", challenge, &signed);
    bad_sig.proof_event_json = tampered_sig.to_string();
    assert!(
        complete_external_agent_enrollment_at(
            bad_sig,
            &state,
            &conn,
            &owner,
            "wss://relay.example",
            false,
            issued_at
        )
        .is_err(),
        "tampered signature"
    );
    seed(&state, challenge, &owner, &agent, issued_at);
    let mut oversized = input("Remote", challenge, &signed);
    oversized.proof_event_json = "x".repeat(MAX_PROOF_JSON_BYTES + 1);
    assert!(
        complete_external_agent_enrollment_at(
            oversized,
            &state,
            &conn,
            &owner,
            "wss://relay.example",
            false,
            issued_at
        )
        .is_err(),
        "proof input bound"
    );
    assert!(get_retained_event(
        &conn,
        AGENT_KIND,
        &owner.public_key().to_hex(),
        &agent.public_key().to_hex()
    )
    .unwrap()
    .is_none());
}
