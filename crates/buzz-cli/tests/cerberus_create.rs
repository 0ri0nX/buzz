//! Disposable MCP → pinned real CLI → encrypted fake observer integration.
#![cfg(unix)]

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
    routing::get,
    Router,
};
use buzz_core::observer::decrypt_observer_payload;
use nostr::{Event, Keys};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Stdio};
use tempfile::TempDir;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    sync::mpsc,
    time::{timeout, Duration},
};

const CHANNEL: &str = "22222222-2222-4222-8222-222222222222";

async fn observer(
    ws: WebSocketUpgrade,
    State(events): State<mpsc::UnboundedSender<Event>>,
) -> Response {
    ws.on_upgrade(move |socket| async move { observe(socket, events).await })
}

async fn observe(mut socket: WebSocket, events: mpsc::UnboundedSender<Event>) {
    socket
        .send(Message::Text(
            json!(["AUTH", "disposable-cerberus-challenge"])
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    while let Some(Ok(Message::Text(text))) = socket.recv().await {
        let value: Value = serde_json::from_str(&text).unwrap();
        let event: Event = serde_json::from_value(value[1].clone()).unwrap();
        event.verify().unwrap();
        match value[0].as_str().unwrap() {
            "AUTH" => assert_eq!(event.kind.as_u16(), 22242),
            "EVENT" => {
                events.send(event.clone()).unwrap();
            }
            _ => panic!("unexpected fake relay operation"),
        }
        socket
            .send(Message::Text(
                json!(["OK", event.id.to_hex(), true, "fixture accepted"])
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
    }
}

struct Fixture {
    directory: TempDir,
    bridge: PathBuf,
    config: PathBuf,
    log: PathBuf,
    relay: String,
    agent: Keys,
    owner: Keys,
    authorization: String,
}

impl Fixture {
    fn new(relay: String) -> Self {
        let directory = tempfile::tempdir().unwrap();
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let bridge = PathBuf::from(
            std::env::var_os("ROWVIA_TEST_BRIDGE").expect("select built Bridge explicitly"),
        );
        let source = PathBuf::from(
            std::env::var_os("ROWVIA_TEST_BRIDGE_REPO").expect("select Bridge source explicitly"),
        );
        let agent = Keys::parse("0000000000000000000000000000000000000000000000000000000000000003")
            .unwrap();
        let owner = Keys::parse("0101010101010101010101010101010101010101010101010101010101010101")
            .unwrap();
        let message = nostr::secp256k1::Message::from_digest(
            Sha256::digest(format!("nostr:agent-auth:{}:", agent.public_key()).as_bytes()).into(),
        );
        let authorization = json!([
            "auth",
            owner.public_key().to_hex(),
            "",
            owner.sign_schnorr(&message).to_string()
        ])
        .to_string();
        let log = directory.path().join("tool-result.json");
        let adapter = directory.path().join("fake-adapter.py");
        fs::write(&adapter, ADAPTER).unwrap();
        fs::set_permissions(&adapter, fs::Permissions::from_mode(0o700)).unwrap();
        let workspace = directory.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        fs::set_permissions(&workspace, fs::Permissions::from_mode(0o700)).unwrap();
        let cli = PathBuf::from(env!("CARGO_BIN_EXE_buzz"));
        let mut config =
            fs::read_to_string(source.join("config.cerberus-agent-draft.example.toml")).unwrap();
        config = config
            .replace("/opt/buzz/bin/buzz-acp", "/usr/bin/false")
            .replace("/opt/agents/bin/codex-acp", adapter.to_str().unwrap())
            .replace("/opt/buzz/bin/buzz", cli.to_str().unwrap())
            .replace(
                "/home/example/cerberus-workspace",
                workspace.to_str().unwrap(),
            )
            .replace(
                "/run/user/1000/rowvia-buzz-cerberus-mcp.sock",
                directory.path().join("mcp.sock").to_str().unwrap(),
            )
            .replace("wss://relay.example.test", &relay)
            .replace("12345678-1234-1234-1234-123456789abc", CHANNEL)
            .replace(
                "89af092923ad49e3b9916b0c9f28c6b9237d8fcab4461cb7ddd12afe354f0d68",
                &owner.public_key().to_hex(),
            )
            .replace(
                "3617ebd52d98c2f4c1804add86fe2d22ec1105264026e8e82e9f7bcf0dc73e14",
                &agent.public_key().to_hex(),
            );
        config = config.replace(
            "[downstream_acp]\n",
            &format!(
                "[downstream_acp]\nargs = [{}]\n",
                json!(log.to_str().unwrap())
            ),
        );
        let cli_hash = hex::encode(Sha256::digest(fs::read(cli).unwrap()));
        let auth_hash = hex::encode(Sha256::digest(authorization.as_bytes()));
        config = config.replace("buzz_executable_sha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"", &format!("buzz_executable_sha256 = \"{cli_hash}\""))
            .replace("authorization_sha256 = \"0000000000000000000000000000000000000000000000000000000000000000\"", &format!("authorization_sha256 = \"{auth_hash}\""));
        let config_path = directory.path().join("bridge.toml");
        fs::write(&config_path, config).unwrap();
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
        Self {
            directory,
            bridge,
            config: config_path,
            log,
            relay,
            agent,
            owner,
            authorization,
        }
    }

    async fn run(
        &self,
        args: Value,
        author: &str,
        signer: &str,
        ambient_relay: Option<&str>,
    ) -> Result<Value, String> {
        if self.log.exists() {
            fs::remove_file(&self.log).unwrap();
        }
        let mut command = tokio::process::Command::new(&self.bridge);
        command
            .args(["acp-proxy", "--config"])
            .arg(&self.config)
            .env_clear()
            .env("HOME", self.directory.path())
            .env("PATH", "/usr/bin:/bin")
            .env("LANG", "C.UTF-8")
            .env("BUZZ_PRIVATE_KEY", signer)
            .env("BUZZ_AUTH_TAG", &self.authorization)
            .env("ROWVIA_OWNER_BEARER", "synthetic-owner-secret-canary")
            .env("OPENAI_API_KEY", "synthetic-provider-canary")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(relay) = ambient_relay {
            command.env("BUZZ_RELAY_URL", relay);
        }
        let mut child = command.spawn().unwrap();
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let envelope = json!({
            "version":"buzz.trusted-turn-context/v1", "source":"message", "relayOrigin":self.relay,
            "agent":{"pubkey":self.agent.public_key().to_hex(), "processInstanceId":"11111111-1111-4111-8111-111111111111",
                "owner":{"pubkey":self.owner.public_key().to_hex(),"provenance":"nip-oa"}},
            "sessionId":"cerberus", "channelId":CHANNEL, "channelType":"stream", "executionScope":{"type":"conversation"},
            "turnId":"33333333-3333-4333-8333-333333333333", "generation":1,
            "events":[{"eventId":"d".repeat(64),"authorPubkey":author,"actorPubkey":author,"threadRootEventId":"c".repeat(64)}],
            "currentEventId":"d".repeat(64)
        });
        let requests = [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientCapabilities":{"_meta":{"buzz":{"trustedTurnContext":{"versions":["buzz.trusted-turn-context/v1"]}}}}}}),
            json!({"jsonrpc":"2.0","id":2,"method":"session/new","params":{"mcpServers":[]}}),
            json!({"jsonrpc":"2.0","id":3,"method":"session/prompt","params":{"sessionId":"cerberus","prompt":[{"type":"text","text":args.to_string()}],"_meta":{"buzz":{"trustedTurnContext":envelope}}}}),
        ];
        let result = timeout(Duration::from_secs(20), async {
            for request in requests {
                input
                    .write_all(format!("{request}\n").as_bytes())
                    .await
                    .map_err(|_| "proxy stdin closed".to_owned())?;
                input
                    .flush()
                    .await
                    .map_err(|_| "proxy stdin closed".to_owned())?;
                let mut line = String::new();
                if output
                    .read_line(&mut line)
                    .await
                    .map_err(|_| "proxy stdout closed".to_owned())?
                    == 0
                {
                    return Err("proxy exited before completion".to_owned());
                }
            }
            fs::read_to_string(&self.log)
                .map_err(|_| "no tool result".to_owned())
                .and_then(|text| {
                    serde_json::from_str(&text).map_err(|_| "invalid tool result".to_owned())
                })
        })
        .await
        .unwrap_or_else(|_| Err("fixture timeout".to_owned()));
        child.start_kill().ok();
        timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap();
        if let Err(reason) = &result {
            let mut diagnostic = Vec::new();
            if let Some(stderr) = child.stderr.take() {
                timeout(
                    Duration::from_secs(3),
                    stderr.take(8192).read_to_end(&mut diagnostic),
                )
                .await
                .unwrap()
                .unwrap();
            }
            return Err(format!(
                "{reason}; fixture proxy diagnostic: {}",
                String::from_utf8_lossy(&diagnostic)
            ));
        }
        result
    }
}

const ADAPTER: &str = r#"#!/usr/bin/python3 -I
import json, os, subprocess, sys
assert set(os.environ).issubset({'HOME','PATH','LANG','INITIAL_AGENT_MODE','LC_CTYPE'})
assert 'BUZZ_PRIVATE_KEY' not in os.environ and 'BUZZ_AUTH_TAG' not in os.environ
def send(stream, value):
    stream.write((json.dumps(value) + '\n').encode()); stream.flush()
def receive(stream):
    return json.loads(stream.readline())
gateway = None
for raw in sys.stdin.buffer:
    request = json.loads(raw)
    if request.get('method') == 'initialize':
        send(sys.stdout.buffer, {'jsonrpc':'2.0','id':request['id'],'result':{'protocolVersion':2,'agentCapabilities':{}}})
    elif request.get('method') == 'session/new':
        descriptor = request['params']['mcpServers'][0]
        assert descriptor['env'] == []
        send(sys.stdout.buffer, {'jsonrpc':'2.0','id':request['id'],'result':{'sessionId':'cerberus','modes':{'currentModeId':'agent','availableModes':[{'id':'agent'}]},'configOptions':[]}})
        gateway = subprocess.Popen([descriptor['command'], *descriptor['args']], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env={})
        send(gateway.stdin, {'jsonrpc':'2.0','id':1,'method':'initialize','params':{'protocolVersion':'2025-06-18'}}); receive(gateway.stdout)
        send(gateway.stdin, {'jsonrpc':'2.0','method':'notifications/initialized','params':{}})
        send(gateway.stdin, {'jsonrpc':'2.0','id':2,'method':'tools/list','params':{}})
        tools = receive(gateway.stdout)['result']['tools']
        assert [tool['name'] for tool in tools] == ['rowvia_buzz_publish_reply','rowvia_buzz_propose_connector','rowvia_buzz_propose_agent']
        schema = tools[2]['inputSchema']
        assert schema['additionalProperties'] is False and set(schema['properties']) == {'display_name','system_prompt'}
    elif request.get('method') == 'session/prompt':
        arguments = json.loads(request['params']['prompt'][0]['text'])
        send(gateway.stdin, {'jsonrpc':'2.0','id':3,'method':'tools/call','params':{'name':'rowvia_buzz_propose_agent','arguments':arguments}})
        result = receive(gateway.stdout)
        gateway.stdin.close(); gateway.wait(timeout=5)
        with open(sys.argv[1], 'w', encoding='utf-8') as log: json.dump(result, log)
        send(sys.stdout.buffer, {'jsonrpc':'2.0','id':request['id'],'result':{'stopReason':'end_turn'}})
"#;

#[tokio::test]
#[ignore = "select source-only Bridge and its repository explicitly; disposable local relay only"]
async fn cerberus_mcp_real_cli_encrypted_observer_replay_restart_and_denials() {
    let (events, mut receiver) = mpsc::unbounded_channel();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay = format!("ws://{}", listener.local_addr().unwrap());
    let app = Router::new().route("/", get(observer)).with_state(events);
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let fixture = Fixture::new(relay);
    let signer = fixture.agent.secret_key().to_secret_hex();
    let owner = fixture.owner.public_key().to_hex();
    let arguments = json!({"display_name":"Scout", "system_prompt":"Find sources.\nCite them."});
    let mut drafts = HashMap::new();
    let mut frozen = None;
    for _ in 0..2 {
        let result = fixture
            .run(arguments.clone(), &owner, &signer, None)
            .await
            .unwrap();
        assert_eq!(result["result"]["isError"], false, "{result}");
        assert_eq!(result["result"]["structuredContent"]["saved"], false);
        let event = timeout(Duration::from_secs(2), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.pubkey, fixture.agent.public_key());
        assert_eq!(event.kind.as_u16(), 24200);
        assert!(!event.content.contains("Scout"));
        let payload: Value = decrypt_observer_payload(&fixture.owner, &event).unwrap();
        assert_eq!(payload["payload"]["action"], "create");
        assert_eq!(
            payload["payload"]["request"],
            json!({"channelId":CHANNEL,"displayName":"Scout","systemPrompt":"Find sources.\nCite them."})
        );
        let request_id = payload["payload"]["requestId"].as_str().unwrap().to_owned();
        assert_eq!(
            result["result"]["structuredContent"]["request_id"],
            request_id
        );
        if let Some(previous) = &frozen {
            assert_eq!(previous, &request_id);
        }
        frozen = Some(request_id.clone());
        drafts.insert(request_id, payload["payload"]["request"].clone());
    }
    assert_eq!(
        drafts.len(),
        1,
        "fake owner observer sees one draft after two process starts"
    );
    let conflict = fixture
        .run(
            json!({"display_name":"Changed","system_prompt":"Different"}),
            &owner,
            &signer,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        conflict["result"]["structuredContent"]["status"],
        "conflict"
    );
    assert_eq!(
        conflict["result"]["structuredContent"]["request_id"],
        frozen.unwrap()
    );
    let foreign = Keys::generate();
    let unauthorized = fixture
        .run(
            arguments.clone(),
            &foreign.public_key().to_hex(),
            &signer,
            None,
        )
        .await
        .unwrap();
    assert_eq!(unauthorized["result"]["isError"], true);
    assert!(fixture
        .run(
            arguments.clone(),
            &owner,
            &foreign.secret_key().to_secret_hex(),
            None
        )
        .await
        .is_err());
    assert!(fixture
        .run(arguments.clone(), &owner, &signer, Some("ws://127.0.0.1:1"))
        .await
        .is_err());
    assert!(fixture
        .run(
            json!({"display_name":"Scout","system_prompt":"Find","runtime":"forbidden"}),
            &owner,
            &signer,
            None
        )
        .await
        .is_err());
    assert!(
        timeout(Duration::from_millis(100), receiver.recv())
            .await
            .is_err(),
        "denied calls must not publish observer events"
    );
    server.abort();
}
