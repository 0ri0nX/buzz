//! Opt-in delivery boundary for a single dedicated harness. No replay recovery.
use super::*;
use tokio::sync::watch;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Status {
    Unarmed,
    AwaitingEose,
    Live,
    Failed(String),
}

pub(super) struct Gate {
    channel: Uuid,
    subscription: String,
    binding: Option<(String, u64)>,
    deadline: tokio::time::Instant,
    status: watch::Sender<Status>,
}

impl Gate {
    pub(super) fn new(channel: Uuid) -> (Self, watch::Receiver<Status>) {
        let (status, receiver) = watch::channel(Status::Unarmed);
        (
            Self {
                channel,
                subscription: format!("startup-{}", Uuid::new_v4().simple()),
                binding: None,
                deadline: tokio::time::Instant::now() + STARTUP_TIMEOUT,
                status,
            },
            receiver,
        )
    }

    pub(super) fn subscription(&self) -> &str {
        &self.subscription
    }
    pub(super) fn channel(&self) -> Uuid {
        self.channel
    }

    pub(super) fn fail(&mut self, reason: &str) {
        if !self.failed() {
            warn!(
                reason,
                "startup-live-only connection terminated; explicit restart required"
            );
            self.status.send_replace(Status::Failed(reason.into()));
        }
    }

    fn failed(&self) -> bool {
        matches!(*self.status.borrow(), Status::Failed(_))
    }

    fn expired(&mut self) -> bool {
        if matches!(
            *self.status.borrow(),
            Status::Unarmed | Status::AwaitingEose
        ) && tokio::time::Instant::now() >= self.deadline
        {
            self.fail("selected channel REQ/EOSE deadline expired");
        }
        self.failed()
    }

    pub(super) fn allow_request(&mut self, channel: Uuid) -> bool {
        !self.expired() && channel == self.channel && *self.status.borrow() == Status::Unarmed
    }

    pub(super) fn request_sent(&mut self, channel: Uuid, subscription: &str, generation: u64) {
        if self.allow_request(channel) && subscription == self.subscription {
            self.binding = Some((subscription.into(), generation));
            self.status.send_replace(Status::AwaitingEose);
        } else {
            self.fail("invalid selected channel REQ witness");
        }
    }

    pub(super) fn eose(&mut self, subscription: &str, generation: u64) {
        if !self.expired()
            && *self.status.borrow() == Status::AwaitingEose
            && self.matches(subscription, generation)
        {
            self.status.send_replace(Status::Live);
        }
    }

    fn matches(&self, subscription: &str, generation: u64) -> bool {
        self.binding
            .as_ref()
            .is_some_and(|(bound, epoch)| bound == subscription && *epoch == generation)
    }

    pub(super) fn admit(
        &mut self,
        subscription: &str,
        generation: u64,
        channel: Option<Uuid>,
    ) -> bool {
        !self.expired()
            && *self.status.borrow() == Status::Live
            && channel == Some(self.channel)
            && self.matches(subscription, generation)
    }
}

pub(super) async fn wait_live(receiver: &mut watch::Receiver<Status>) -> Result<(), RelayError> {
    loop {
        if receiver.has_changed().is_err() {
            return Err(RelayError::ConnectionClosed);
        }
        match receiver.borrow_and_update().clone() {
            Status::Live => return Ok(()),
            Status::Failed(reason) => return Err(RelayError::UnexpectedMessage(reason)),
            _ => {}
        }
        // Independent bound also covers a stalled background send/task.
        timeout(STARTUP_TIMEOUT, receiver.changed())
            .await
            .map_err(|_| RelayError::Timeout)?
            .map_err(|_| RelayError::ConnectionClosed)?;
    }
}

/// Reuses the normal authenticated socket, buffered-frame and command handlers.
/// Every failure drops this socket; none can enter normal recovery/resubscribe.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run(
    mut ws: WsStream,
    initial_buffer: VecDeque<RelayMessage>,
    event_tx: mpsc::Sender<Option<BuzzEvent>>,
    observer_tx: mpsc::Sender<Event>,
    mut commands: mpsc::Receiver<RelayCommand>,
    keys: Keys,
    relay_url: String,
    pubkey: String,
    auth_tag: Option<nostr::Tag>,
    gate: Gate,
) {
    let mut state = BgState::new();
    let deadline = gate.deadline;
    state.startup_gate = Some(gate); // Closed BEFORE processing any auth-buffered EVENT/EOSE.
    let buffered_ok = process_handshake_buffer(
        &mut ws,
        initial_buffer,
        &event_tx,
        &observer_tx,
        &mut state,
        &keys,
        &relay_url,
        &pubkey,
        auth_tag.as_ref(),
    )
    .await;
    let mut ping_interval = tokio::time::interval(PING_INTERVAL);
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut pong_due = None;
    if buffered_ok {
        loop {
            if state.startup_gate.as_mut().is_some_and(Gate::expired) {
                break;
            }
            let waiting = state
                .startup_gate
                .as_ref()
                .is_some_and(|gate| *gate.status.borrow() != Status::Live);
            let ok = tokio::select! {
                _ = tokio::time::sleep_until(deadline), if waiting => false,
                _ = async {
                    match pong_due { Some(at) => tokio::time::sleep_until(at).await,
                        None => std::future::pending().await }
                } => false,
                _ = ping_interval.tick() => {
                    if ws_send_timeout(&mut ws, Message::Ping(Vec::new().into()), WS_SEND_TIMEOUT_SECS).await.is_err() { false }
                    else { pong_due = Some(tokio::time::Instant::now() + PONG_TIMEOUT); true }
                }
                raw = ws.next() => match raw {
                    Some(Ok(Message::Pong(_))) => { pong_due = None; true }
                    Some(Ok(message)) => handle_ws_message(message, &mut ws, &event_tx,
                        &observer_tx, &mut state, &keys, &relay_url, &pubkey, auth_tag.as_ref()).await,
                    _ => false,
                },
                command = commands.recv() => match command {
                    // No caller-driven reconnect, dynamic membership expansion,
                    // unsubscribe/restart cycle, or second channel REQ.
                    None | Some(RelayCommand::Shutdown | RelayCommand::Reconnect |
                        RelayCommand::SubscribeMembership | RelayCommand::Unsubscribe { .. }) => false,
                    Some(command) => execute_connected_command(&mut ws, &mut state, &pubkey, command).await,
                },
            };
            if !ok {
                break;
            }
        }
    }
    if let Some(gate) = state.startup_gate.as_mut() {
        gate.fail("startup-live-only socket stopped (disconnect, timeout, or command failure)");
    }
    // Dropping the sender wakes next_event even when the bounded buffer is full.
}

#[cfg(test)]
mod tests {
    use super::super::tests::{next_test_frame, test_ws_pair};
    use super::*;

    fn event(channel: Uuid, text: &str, timestamp: u64) -> Event {
        EventBuilder::new(Kind::Custom(9), text)
            .tags([Tag::parse(["h", &channel.to_string()]).unwrap()])
            .custom_created_at(nostr::Timestamp::from(timestamp))
            .sign_with_keys(&Keys::generate())
            .unwrap()
    }

    async fn frame(
        ws: &mut WsStream,
        state: &mut BgState,
        tx: &mpsc::Sender<Option<BuzzEvent>>,
        data: Value,
    ) -> bool {
        let (observer, _) = mpsc::channel(4);
        let keys = Keys::generate();
        handle_ws_message(
            Message::Text(data.to_string().into()),
            ws,
            tx,
            &observer,
            state,
            &keys,
            "ws://localhost",
            &keys.public_key().to_hex(),
            None,
        )
        .await
    }

    #[tokio::test]
    async fn startup_live_only_closed_before_buffer_and_exact_req_eose_before_any_effects() {
        let (mut ws, mut server) = test_ws_pair().await;
        let channel = Uuid::new_v4();
        let historical = event(channel, "historical", 1);
        let interleaved = event(channel, "interleaved live", unix_now_secs());
        let (gate, mut witness) = Gate::new(channel);
        let sub = gate.subscription().to_string();
        let mut state = BgState::new();
        state.startup_gate = Some(gate);
        let (tx, mut rx) = mpsc::channel(4);
        let (observer, _) = mpsc::channel(4);
        let keys = Keys::generate();
        let buffer = VecDeque::from([
            RelayMessage::Eose {
                subscription_id: sub.clone(),
            },
            RelayMessage::Event {
                subscription_id: sub.clone(),
                event: Box::new(historical.clone()),
            },
        ]);
        assert!(
            process_handshake_buffer(
                &mut ws,
                buffer,
                &tx,
                &observer,
                &mut state,
                &keys,
                "ws://localhost",
                &keys.public_key().to_hex(),
                None
            )
            .await
        );
        assert_eq!(*witness.borrow(), Status::Unarmed);
        assert!(state.last_seen.is_empty());
        assert!(!state.seen_ids.contains(&historical.id.to_hex()));
        assert!(rx.try_recv().is_err());

        assert!(
            execute_connected_command(
                &mut ws,
                &mut state,
                "agent",
                RelayCommand::Subscribe {
                    channel_id: channel,
                    filter: ChannelFilter {
                        kinds: None,
                        require_mention: false
                    },
                    replay_since: None,
                }
            )
            .await
        );
        let request = next_test_frame(&mut server).await;
        assert_eq!(request[0], "REQ");
        assert_eq!(request[1], sub);
        assert_eq!(request[2]["#h"], json!([channel.to_string()]));
        assert_eq!(*witness.borrow(), Status::AwaitingEose);
        assert!(timeout(Duration::from_millis(10), wait_live(&mut witness))
            .await
            .is_err());
        assert!(
            frame(
                &mut ws,
                &mut state,
                &tx,
                json!(["EOSE", MEMBERSHIP_NOTIF_SUB_ID])
            )
            .await
        );
        assert!(
            frame(
                &mut ws,
                &mut state,
                &tx,
                json!(["EOSE", channel_sub_id(channel)])
            )
            .await
        );
        assert!(
            frame(
                &mut ws,
                &mut state,
                &tx,
                json!(["EOSE", OBSERVER_CONTROL_SUB_ID])
            )
            .await
        );
        state.startup_gate.as_mut().unwrap().eose(&sub, 99);
        assert!(frame(&mut ws, &mut state, &tx, json!(["EVENT", sub, historical])).await);
        assert!(frame(&mut ws, &mut state, &tx, json!(["EVENT", sub, interleaved])).await);
        assert!(state.last_seen.is_empty());
        assert!(!state.seen_ids.contains(&interleaved.id.to_hex()));
        assert!(rx.try_recv().is_err()); // No ingress => no queue/seen reaction/ACP wake.

        assert!(frame(&mut ws, &mut state, &tx, json!(["EOSE", sub])).await);
        wait_live(&mut witness).await.unwrap();
        // EOSE is a delivery boundary, never a created_at freshness predicate.
        assert!(frame(&mut ws, &mut state, &tx, json!(["EVENT", sub, historical])).await);
        assert_eq!(rx.try_recv().unwrap().unwrap().event.id, historical.id);
        assert_eq!(state.last_seen.get(&channel), Some(&1));
        assert!(state.seen_ids.contains(&historical.id.to_hex()));
        // Wrong channel/sub/generation still cannot enter normal processing.
        assert!(
            frame(
                &mut ws,
                &mut state,
                &tx,
                json!(["EVENT", channel_sub_id(Uuid::new_v4()), interleaved])
            )
            .await
        );
        state.connection_generation += 1;
        assert!(frame(&mut ws, &mut state, &tx, json!(["EVENT", sub, interleaved])).await);
        assert!(rx.try_recv().is_err());
        assert!(!state.startup_gate.as_mut().unwrap().allow_request(channel));
    }

    #[tokio::test]
    async fn startup_live_only_timeout_closed_and_overflow_are_terminal() {
        let (mut ws, _server) = test_ws_pair().await;
        let channel = Uuid::new_v4();
        let (tx, mut rx) = mpsc::channel(1);
        for cause in ["timeout", "closed", "overflow"] {
            let (mut gate, mut witness) = Gate::new(channel);
            let sub = gate.subscription().to_string();
            gate.request_sent(channel, &sub, 0);
            let mut state = BgState::new();
            if cause == "timeout" {
                gate.deadline = tokio::time::Instant::now();
            } else {
                gate.eose(&sub, 0);
            }
            state.startup_gate = Some(gate);
            match cause {
                "timeout" => assert!(frame(&mut ws, &mut state, &tx, json!(["EOSE", sub])).await),
                "closed" => assert!(
                    !frame(
                        &mut ws,
                        &mut state,
                        &tx,
                        json!(["CLOSED", sub, "rate-limited: retry in 1s"])
                    )
                    .await
                ),
                _ => {
                    let first = event(channel, "first", 1);
                    let second = event(channel, "overflow", 2);
                    assert!(frame(&mut ws, &mut state, &tx, json!(["EVENT", sub, first])).await);
                    assert!(!frame(&mut ws, &mut state, &tx, json!(["EVENT", sub, second])).await);
                    assert!(state.channel_dropped_since.is_empty());
                    let _ = rx.try_recv();
                }
            }
            state.startup_gate.as_mut().unwrap().eose(&sub, 0);
            assert!(wait_live(&mut witness).await.is_err());
            assert!(!state.startup_gate.as_mut().unwrap().allow_request(channel));
            assert!(state.rate_limited_pending.is_empty());
            assert!(state.resubscribe_retry.is_empty());
        }
    }

    #[tokio::test]
    async fn startup_live_only_second_req_and_disconnect_do_not_retry() {
        for second_req in [false, true] {
            let (ws, mut server) = test_ws_pair().await;
            let channel = Uuid::new_v4();
            let (gate, mut witness) = Gate::new(channel);
            let sub = gate.subscription().to_string();
            let (tx, mut rx) = mpsc::channel(4);
            let (observer, _) = mpsc::channel(4);
            let (commands, command_rx) = mpsc::channel(4);
            let task = tokio::spawn(run(
                ws,
                VecDeque::new(),
                tx,
                observer,
                command_rx,
                Keys::generate(),
                "ws://127.0.0.1:1".into(),
                "agent".into(),
                None,
                gate,
            ));
            commands
                .send(RelayCommand::Subscribe {
                    channel_id: channel,
                    filter: ChannelFilter {
                        kinds: None,
                        require_mention: false,
                    },
                    replay_since: None,
                })
                .await
                .unwrap();
            loop {
                let message = server.next().await.unwrap().unwrap();
                if let Message::Ping(data) = message {
                    server.send(Message::Pong(data)).await.unwrap();
                    continue;
                }
                let value: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
                assert_eq!(value[0], "REQ");
                break;
            }
            server
                .send(Message::Text(json!(["EOSE", sub]).to_string().into()))
                .await
                .unwrap();
            wait_live(&mut witness).await.unwrap();
            if second_req {
                commands
                    .send(RelayCommand::Subscribe {
                        channel_id: channel,
                        filter: ChannelFilter {
                            kinds: None,
                            require_mention: false,
                        },
                        replay_since: None,
                    })
                    .await
                    .unwrap();
            } else {
                server.close(None).await.unwrap();
            }
            timeout(Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap();
            assert!(wait_live(&mut witness).await.is_err());
            assert!(commands.send(RelayCommand::Reconnect).await.is_err());
            assert!(rx.recv().await.is_none());
        }
    }

    #[tokio::test]
    async fn startup_live_only_missing_eose_and_req_send_failure_never_ready() {
        let (ws, _server) = test_ws_pair().await;
        let channel = Uuid::new_v4();
        let (mut gate, mut witness) = Gate::new(channel);
        gate.deadline = tokio::time::Instant::now() + Duration::from_millis(50);
        let (tx, _) = mpsc::channel(4);
        let (observer, _) = mpsc::channel(4);
        let (commands, command_rx) = mpsc::channel(4);
        let task = tokio::spawn(run(
            ws,
            VecDeque::new(),
            tx,
            observer,
            command_rx,
            Keys::generate(),
            "ws://127.0.0.1:1".into(),
            "agent".into(),
            None,
            gate,
        ));
        commands
            .send(RelayCommand::Subscribe {
                channel_id: channel,
                filter: ChannelFilter {
                    kinds: None,
                    require_mention: false,
                },
                replay_since: None,
            })
            .await
            .unwrap();
        assert!(wait_live(&mut witness).await.is_err());
        timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        assert!(commands.send(RelayCommand::Reconnect).await.is_err());

        let (mut ws, _server) = test_ws_pair().await;
        ws.close(None).await.unwrap();
        let (gate, mut witness) = Gate::new(channel);
        let mut state = BgState::new();
        state.startup_gate = Some(gate);
        assert!(
            !execute_connected_command(
                &mut ws,
                &mut state,
                "agent",
                RelayCommand::Subscribe {
                    channel_id: channel,
                    filter: ChannelFilter {
                        kinds: None,
                        require_mention: false
                    },
                    replay_since: None,
                }
            )
            .await
        );
        assert!(wait_live(&mut witness).await.is_err());
        assert!(state.active_subscriptions.is_empty());
        assert!(state.resubscribe_retry.is_empty());

        for kind in [9, KIND_AGENT_OBSERVER_FRAME] {
            let (mut gate, mut witness) = Gate::new(channel);
            let subscription = gate.subscription().to_string();
            gate.request_sent(channel, &subscription, 0);
            gate.eose(&subscription, 0);
            let mut state = BgState::new();
            state.startup_gate = Some(gate);
            let event = EventBuilder::new(Kind::Custom(kind as u16), "synthetic publish")
                .sign_with_keys(&Keys::generate())
                .unwrap();
            assert!(
                !execute_connected_command(
                    &mut ws,
                    &mut state,
                    "agent",
                    RelayCommand::PublishEvent {
                        event: Box::new(event.clone())
                    }
                )
                .await
            );
            assert!(wait_live(&mut witness).await.is_err());
            assert!(state.gated_observer_pending.is_empty());
            assert!(state.observer_in_flight.is_empty());
            let mut ordinary = BgState::new();
            assert!(
                execute_connected_command(
                    &mut ws,
                    &mut ordinary,
                    "agent",
                    RelayCommand::PublishEvent {
                        event: Box::new(event)
                    }
                )
                .await
            );
            assert_eq!(
                ordinary.gated_observer_pending.len(),
                usize::from(kind == KIND_AGENT_OBSERVER_FRAME)
            );
        }
    }

    #[tokio::test]
    async fn startup_live_only_readiness_and_buffered_failure_use_harness_seam() {
        let channel = Uuid::new_v4();
        let (mut gate, witness) = Gate::new(channel);
        let (tx, rx) = mpsc::channel(4);
        let (commands, _) = mpsc::channel(4);
        let mut relay = HarnessRelay {
            event_rx: rx,
            observer_control_rx: None,
            cmd_tx: commands,
            http: reqwest::Client::new(),
            relay_url: "ws://127.0.0.1:1".into(),
            keys: Keys::generate(),
            auth_tag: None,
            bg_handle: None,
            startup_gate: Some(witness),
        };
        assert!(
            timeout(Duration::from_millis(10), relay.wait_startup_live())
                .await
                .is_err()
        );
        let subscription = gate.subscription().to_string();
        gate.request_sent(channel, &subscription, 0);
        assert!(
            timeout(Duration::from_millis(10), relay.wait_startup_live())
                .await
                .is_err()
        );
        gate.eose(&subscription, 0);
        relay.wait_startup_live().await.unwrap();
        tx.send(Some(BuzzEvent {
            channel_id: channel,
            connection_generation: 0,
            event: event(channel, "buffered before terminal failure", 1),
        }))
        .await
        .unwrap();
        // A read-only author/metadata response may resolve after the socket
        // failed. The production fence must suppress its whole synchronous
        // queue/seen/steer/dispatch callback after that await.
        let (author_response_tx, author_response_rx) = tokio::sync::oneshot::channel();
        let mut effects_ran = false;
        let delayed_authorization = async {
            author_response_rx.await.unwrap();
            relay.with_live_ingress(|| {
                effects_ran = true;
            })
        };
        let disconnect_during_authorization = async {
            tokio::task::yield_now().await;
            gate.fail("synthetic disconnect during authorization");
            let _ = author_response_tx.send(());
        };
        let (admission, ()) = tokio::join!(delayed_authorization, disconnect_during_authorization);
        assert!(admission.is_err());
        assert!(!effects_ran);
        assert!(relay.check_startup_live().is_err());
        relay.startup_failure().await;
        assert!(
            relay.next_event().await.is_none(),
            "terminal socket cannot leak its buffered work"
        );
        assert!(relay.wait_startup_live().await.is_err());

        // Even an unexpected background-task drop after EOSE cannot leave a
        // stale Live witness accepted as readiness.
        let (mut dropped_gate, dropped_witness) = Gate::new(channel);
        let subscription = dropped_gate.subscription().to_string();
        dropped_gate.request_sent(channel, &subscription, 0);
        dropped_gate.eose(&subscription, 0);
        drop(dropped_gate);
        relay.startup_gate = Some(dropped_witness);
        assert!(relay.check_startup_live().is_err());
        assert!(relay.wait_startup_live().await.is_err());

        // The default-off seam remains immediately ready and forwards normally.
        relay.startup_gate = None;
        relay.wait_startup_live().await.unwrap();
        relay
            .with_live_ingress(|| {
                effects_ran = true;
            })
            .unwrap();
        assert!(effects_ran);
        assert!(timeout(Duration::from_millis(10), relay.startup_failure())
            .await
            .is_err());
        let normal = event(channel, "default off", 1);
        tx.send(Some(BuzzEvent {
            channel_id: channel,
            connection_generation: 0,
            event: normal.clone(),
        }))
        .await
        .unwrap();
        assert_eq!(relay.next_event().await.unwrap().event.id, normal.id);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn startup_live_only_custom_acp_canaries_do_not_reach_worker() {
        const CHILD: &str = "BUZZ_TEST_LIVE_ONLY_CANARY_CHILD";
        if std::env::var_os(CHILD).is_none() {
            // A separate test process injects inherited canaries without
            // mutating the shared test runner's process environment.
            let output = tokio::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "relay::startup_live_only::tests::startup_live_only_custom_acp_canaries_do_not_reach_worker", "--nocapture"])
                .env(CHILD, "1")
                .env("NOSTR_PRIVATE_KEY", "synthetic-inherited-secret")
                .env("GIT_CONFIG_PARAMETERS", "synthetic-inherited-git")
                .env("BUZZ_ACP_LAUNCH_PREFIX", "[\"/usr/bin/env\"]")
                .output().await.unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        assert_eq!(
            std::env::var("NOSTR_PRIVATE_KEY").unwrap(),
            "synthetic-inherited-secret"
        );
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("worker-env");
        let ready = directory.path().join("worker-env-ready");
        let mut worker = crate::acp::AcpClient::spawn(
            "/bin/sh",
            &[
                "-c".into(),
                "env > \"$CANARY_OUTPUT\"; touch \"$CANARY_READY\"; exec sleep 30".into(),
            ],
            &[
                ("CANARY_OUTPUT".into(), output.to_string_lossy().into()),
                ("CANARY_READY".into(), ready.to_string_lossy().into()),
                ("NOSTR_PRIVATE_KEY".into(), "synthetic-secret-canary".into()),
                (
                    "GIT_CONFIG_PARAMETERS".into(),
                    "synthetic-git-canary".into(),
                ),
                (
                    "BUZZ_ACP_LAUNCH_PREFIX".into(),
                    "synthetic-control-canary".into(),
                ),
            ],
            false,
        )
        .await
        .unwrap();
        timeout(Duration::from_secs(2), async {
            while !ready.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let captured = std::fs::read_to_string(output).unwrap();
        for name in [
            "NOSTR_PRIVATE_KEY=",
            "GIT_CONFIG_PARAMETERS=",
            "BUZZ_ACP_LAUNCH_PREFIX=",
        ] {
            assert!(
                !captured.lines().any(|line| line.starts_with(name)),
                "leaked {name}"
            );
        }
        worker.shutdown().await;
    }
}
