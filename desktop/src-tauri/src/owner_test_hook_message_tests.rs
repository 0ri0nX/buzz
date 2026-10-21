// Included inside owner_test_hook::tests to share the production validator and ledger fixtures.

#[test]
fn fixed_claude_create_contract_is_native_closed_expiring_and_owner_scoped() {
    let mut r = request();
    r.operation = "create_stopped_claude_developer".into();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    r.arguments = serde_json::json!({"expiresAt": now + 60_000})
        .as_object()
        .unwrap()
        .clone();
    assert!(validate_request(&serde_json::to_vec(&r).unwrap()).is_ok());
    for (field, value) in [
        ("expiresAt", serde_json::json!(now)),
        ("expiresAt", serde_json::json!(now + 121_000)),
        ("expiresAt", serde_json::json!("later")),
        ("name", serde_json::json!("Other")),
        ("command", serde_json::json!("/bin/sh")),
        ("provider", serde_json::json!("other")),
        ("model", serde_json::json!("other")),
        ("path", serde_json::json!("/tmp/other")),
        ("recipientPubkey", serde_json::json!("f".repeat(64))),
        ("spawnAfterCreate", serde_json::json!(true)),
        ("startOnAppLaunch", serde_json::json!(true)),
    ] {
        let mut invalid = r.clone();
        invalid.arguments.insert(field.into(), value);
        assert!(
            matches!(
                validate_request(&serde_json::to_vec(&invalid).unwrap()),
                Err(Code::InvalidRequest)
            ),
            "{field}"
        );
    }
    let mut missing = r.clone();
    missing.arguments.clear();
    assert!(matches!(
        validate_request(&serde_json::to_vec(&missing).unwrap()),
        Err(Code::InvalidRequest)
    ));
    for field in ["ownerPubkey", "architectPubkey", "relayUrl"] {
        let mut invalid = serde_json::to_value(&r).unwrap();
        invalid["expected"][field] = Value::String("foreign".into());
        assert!(matches!(
            validate_request(&serde_json::to_vec(&invalid).unwrap()),
            Err(Code::ScopeMismatch)
        ));
    }
}

#[tokio::test]
async fn fixed_claude_native_error_never_serializes_raw_error_or_secret_canary() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("redaction.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let metadata = fs::symlink_metadata(&path).unwrap();
    let hook = Hook {
        ledger: Mutex::new(Ledger::default()),
        cancel: CancellationToken::new(),
        socket: SocketGuard {
            path,
            device: metadata.dev(),
            inode: metadata.ino(),
            uid: metadata.uid(),
            cleanup_attempted: AtomicBool::new(false),
        },
    };
    let mut r = request();
    r.operation = "create_stopped_claude_developer".into();
    assert!(matches!(
        hook.ledger.lock().unwrap().admit(&r),
        Admission::Dispatch(_)
    ));
    let response = dispatch_native(
        &hook,
        &r,
        tokio::time::Instant::now() + Duration::from_secs(5),
        || Err("nsec1_PRIVATE_AGENT_KEY_AUTH_RAW_ERROR_CANARY".into()),
    )
    .await;
    let encoded = serde_json::to_string(&response).unwrap();
    assert_eq!(response.status, Status::Unknown);
    assert!(matches!(response.code, Some(Code::OperationFailed)));
    assert!(!encoded.contains("nsec") && !encoded.contains("CANARY"));
    assert!(response.result.is_none());
}

#[test]
fn socket_cleanup_never_retries_even_when_replacement_metadata_matches() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("one-attempt.sock");
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let metadata = fs::symlink_metadata(&path).unwrap();
    let mut guard = SocketGuard {
        path: path.clone(),
        device: metadata.dev(),
        inode: metadata.ino(),
        uid: metadata.uid(),
        cleanup_attempted: AtomicBool::new(false),
    };
    guard.remove();
    assert!(!path.exists());
    drop(listener);
    let replacement = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let metadata = fs::symlink_metadata(&path).unwrap();
    // Deterministically model inode reuse without depending on allocator timing.
    guard.device = metadata.dev();
    guard.inode = metadata.ino();
    guard.remove();
    assert!(
        path.exists(),
        "a second cleanup must not unlink a matching replacement"
    );
    drop(guard);
    assert!(
        path.exists(),
        "Drop must not retry the consumed cleanup attempt"
    );
    drop(replacement);
}

#[test]
fn existing_test_readback_contract_pins_pairs_scope_and_bounded_closed_arguments() {
    for target in TEST_MESSAGE_TARGETS {
        let mut r = request();
        r.operation = "read_existing_test_channel".into();
        r.arguments = serde_json::json!({"channelId":target.0, "recipientPubkey":target.1, "limit":20, "since":1}).as_object().unwrap().clone();
        assert!(validate_request(&serde_json::to_vec(&r).unwrap()).is_ok());
        let mut without_since = r.clone();
        without_since.arguments.remove("since");
        assert!(validate_request(&serde_json::to_vec(&without_since).unwrap()).is_ok());
        for (field, value) in [
            (
                "channelId",
                serde_json::json!("a1234567-1234-4234-8234-123456789abc"),
            ),
            ("recipientPubkey", serde_json::json!("f".repeat(64))),
            ("limit", serde_json::json!(0)),
            ("limit", serde_json::json!(21)),
            ("limit", serde_json::json!(1.5)),
            ("since", serde_json::json!(-1)),
            ("since", serde_json::json!("now")),
            ("rootEventId", serde_json::json!("a".repeat(64))),
            ("parentEventId", serde_json::json!("a".repeat(64))),
            ("content", serde_json::json!("ROWVIA_E2E_")),
        ] {
            let mut invalid = r.clone();
            invalid.arguments.insert(field.into(), value);
            assert!(
                matches!(
                    validate_request(&serde_json::to_vec(&invalid).unwrap()),
                    Err(Code::InvalidRequest)
                ),
                "{field}"
            );
        }
        for field in ["channelId", "recipientPubkey", "limit"] {
            let mut invalid = r.clone();
            invalid.arguments.remove(field);
            assert!(matches!(
                validate_request(&serde_json::to_vec(&invalid).unwrap()),
                Err(Code::InvalidRequest)
            ));
        }
        for field in ["ownerPubkey", "architectPubkey", "relayUrl"] {
            let mut invalid = serde_json::to_value(&r).unwrap();
            invalid["expected"][field] = Value::String("foreign".into());
            assert!(matches!(
                validate_request(&serde_json::to_vec(&invalid).unwrap()),
                Err(Code::ScopeMismatch)
            ));
        }
    }
}

fn test_message_request(target: (&str, &str)) -> Request {
    let mut r = request();
    r.operation = "send_existing_test_message".into();
    r.arguments = serde_json::json!({
        "channelId": target.0,
        "recipientPubkey": target.1,
        "content": "ROWVIA_E2E_ synthetic",
        "expiresAt": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64 + 60_000,
    }).as_object().unwrap().clone();
    r
}

#[test]
fn existing_test_message_contract_denies_wrong_pairs_scope_threads_and_expiry() {
    for target in TEST_MESSAGE_TARGETS {
        let r = test_message_request(target);
        assert!(validate_request(&serde_json::to_vec(&r).unwrap()).is_ok());
        for (field, value) in [
            (
                "channelId",
                serde_json::json!("a1234567-1234-4234-8234-123456789abc"),
            ),
            ("recipientPubkey", serde_json::json!("f".repeat(64))),
            ("content", serde_json::json!("prefix ROWVIA_E2E_")),
            (
                "content",
                serde_json::json!(format!("ROWVIA_E2E_{}", "😀".repeat(300))),
            ),
            ("expiresAt", serde_json::json!(0)),
            ("expiresAt", serde_json::json!(u64::MAX)),
            ("expiresAt", serde_json::json!(1.5)),
            ("expiresAt", serde_json::json!("tomorrow")),
            ("parentEventId", serde_json::json!("a".repeat(64))),
            ("rootEventId", serde_json::json!("a".repeat(64))),
            ("mentionPubkeys", serde_json::json!([OWNER])),
            ("kind", serde_json::json!(40002)),
        ] {
            let mut invalid = r.clone();
            invalid.arguments.insert(field.into(), value);
            assert!(
                matches!(
                    validate_request(&serde_json::to_vec(&invalid).unwrap()),
                    Err(Code::InvalidRequest)
                ),
                "{field}"
            );
        }
        let mut cross_pair = r.clone();
        cross_pair.arguments.insert(
            "recipientPubkey".into(),
            Value::String(
                TEST_MESSAGE_TARGETS
                    .iter()
                    .find(|pair| **pair != target)
                    .unwrap()
                    .1
                    .into(),
            ),
        );
        assert!(matches!(
            validate_request(&serde_json::to_vec(&cross_pair).unwrap()),
            Err(Code::InvalidRequest)
        ));
        let mut value = serde_json::to_value(&r).unwrap();
        for field in ["channelId", "recipientPubkey", "content", "expiresAt"] {
            let mut invalid = value.clone();
            invalid["arguments"].as_object_mut().unwrap().remove(field);
            assert!(matches!(
                validate_request(&serde_json::to_vec(&invalid).unwrap()),
                Err(Code::InvalidRequest)
            ));
        }
        for field in ["ownerPubkey", "architectPubkey", "relayUrl"] {
            let mut invalid = value.clone();
            invalid["expected"][field] = Value::String("foreign".into());
            assert!(matches!(
                validate_request(&serde_json::to_vec(&invalid).unwrap()),
                Err(Code::ScopeMismatch)
            ));
        }
        value["operation"] = Value::String("send_existing_test_message_extra".into());
        assert!(matches!(
            validate_request(&serde_json::to_vec(&value).unwrap()),
            Err(Code::InvalidRequest)
        ));
    }
}

#[test]
fn existing_test_message_ledger_retains_nonce_after_unknown_and_rejects_altered_retry() {
    let r = test_message_request(TEST_MESSAGE_TARGETS[0]);
    let mut ledger = Ledger::default();
    assert!(matches!(ledger.admit(&r), Admission::Dispatch(_)));
    assert!(matches!(
        ledger.admit(&r),
        Admission::Return(Response {
            status: Status::Unknown,
            ..
        })
    ));
    ledger
        .complete(Response::failure(
            &r.request_id,
            Status::Unknown,
            Code::Timeout,
        ))
        .unwrap();
    assert!(matches!(
        ledger.admit(&r),
        Admission::Return(Response {
            status: Status::Unknown,
            code: Some(Code::Timeout),
            ..
        })
    ));
    let mut altered = r.clone();
    altered.arguments.insert(
        "content".into(),
        Value::String("ROWVIA_E2E_ altered retry".into()),
    );
    assert!(matches!(
        ledger.admit(&altered),
        Admission::Return(Response {
            status: Status::Error,
            code: Some(Code::DuplicateRequest),
            ..
        })
    ));
}
