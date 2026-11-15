//! Independent P0 public Connection entry tests; no Node, projection or WSS claims.
#[path = "acp_runtime_acceptance/support/legacy.rs"]
mod bridge;
use acp_remote::adapters::acp::{
    AcpError, Incoming, MessageId, SessionEvent, SessionId, UpdateOrigin, Version,
};
use bridge::*;
use serde_json::{Value, json};
use tokio::{
    io::{
        AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf,
    },
    time::{Duration, Instant, timeout},
};
const TAGS: [&str; 3] = [
    "user_message_chunk",
    "agent_message_chunk",
    "agent_thought_chunk",
];
type Client = TestClient;
struct Peer {
    reader: BufReader<ReadHalf<DuplexStream>>,
    writer: WriteHalf<DuplexStream>,
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
async fn wire(peer: &mut Peer) -> Value {
    let mut line = String::new();
    assert!(
        timeout(Duration::from_secs(2), peer.reader.read_line(&mut line))
            .await
            .unwrap()
            .unwrap()
            > 0
    );
    serde_json::from_str(&line).unwrap()
}
async fn next_raw(client: &mut Client, peer: &mut Peer, raw: &[u8]) -> Result<Incoming, AcpError> {
    let (written, event) = tokio::join!(
        async {
            peer.writer.write_all(raw).await?;
            peer.writer.write_all(b"\n").await
        },
        client.next(deadline())
    );
    written.unwrap();
    event.map(|received| received.incoming().clone())
}
async fn ready(offer: Version, selected: Version) -> (Client, Peer) {
    let (client_io, peer_io) = tokio::io::duplex(64 * 1024);
    let (reader, writer) = tokio::io::split(client_io);
    let (peer_reader, peer_writer) = tokio::io::split(peer_io);
    let mut client = TestClient::new(reader, writer);
    let mut peer = Peer {
        reader: BufReader::new(peer_reader),
        writer: peer_writer,
    };
    let request = if offer == Version::preferred() {
        client.initialize_preferred(deadline()).await.unwrap()
    } else {
        client.initialize(offer, deadline()).await.unwrap()
    };
    let offered = wire(&mut peer).await;
    assert_eq!(offered["method"], "initialize");
    assert_eq!(offered["params"]["protocolVersion"], offer.number());
    let result = match selected {
        Version::V1 => {
            json!({"protocolVersion":1,"agentInfo":{"name":"independent-p0","version":"synthetic"},"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"resume":{}}}})
        }
        Version::V2 => {
            json!({"protocolVersion":2,"info":{"name":"independent-p0","version":"synthetic"},"capabilities":{"session":{}}})
        }
    };
    let response = json!({"jsonrpc":"2.0","id":request.json(),"result":result});
    next_raw(
        &mut client,
        &mut peer,
        &serde_json::to_vec(&response).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(client.strategy().unwrap().version(), selected);
    assert_eq!(client.strategy().unwrap().negotiated().offered, offer);
    (client, peer)
}
fn frame(tag: &str, message: Option<&Value>, sequence: u32, target: &str) -> Value {
    let mut frame = json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":target,"update":{"sessionUpdate":tag,"content":{"type":"text","text":"synthetic chunk"},"_meta":{"sequence":sequence},"extension":{"null":null,"ordered":[sequence,0]}}}});
    if let Some(id) = message {
        frame["params"]["update"]["messageId"] = id.clone();
    }
    frame
}
fn assert_update(
    event: &Incoming,
    raw_expected: &[u8],
    tag: &str,
    message: Option<&str>,
    target: &str,
    origin_expected: UpdateOrigin,
) {
    let Incoming::Update { event, origin, raw } = event else {
        panic!("expected update")
    };
    let SessionEvent::Message {
        session,
        message: actual,
        kind,
    } = event
    else {
        panic!("expected message")
    };
    assert_eq!(session.as_str(), target);
    assert_eq!(kind.as_ref(), tag);
    assert_eq!(actual.as_ref().map(MessageId::as_str), message);
    assert_eq!(*origin, origin_expected);
    assert_eq!(raw.bytes(), raw_expected);
    let expected: Value = serde_json::from_slice(raw_expected).unwrap();
    assert_eq!(
        raw.fields()
            .unwrap()
            .get("params")
            .unwrap()
            .unwrap()
            .compact()
            .unwrap(),
        serde_json::to_string(expected.get("params").unwrap()).unwrap()
    );
}
async fn accepted(
    client: &mut Client,
    peer: &mut Peer,
    tag: &str,
    id: Option<&Value>,
    expected: Option<&str>,
    sequence: u32,
) {
    let raw = serde_json::to_vec(&frame(tag, id, sequence, "A")).unwrap();
    let event = next_raw(client, peer, &raw).await.unwrap();
    assert_update(&event, &raw, tag, expected, "A", UpdateOrigin::Live);
    assert!(!client.is_closed());
}
async fn rejected(version: Version, tag: &str, id: &Value) {
    let (mut client, mut peer) = ready(version, version).await;
    let raw = serde_json::to_vec(&frame(tag, Some(id), 0, "A")).unwrap();
    assert!(matches!(
        next_raw(&mut client, &mut peer, &raw).await,
        Err(AcpError::InvalidMessage)
    ));
    assert!(client.is_closed());
    assert_eq!(client.take_rejected_message().unwrap().bytes(), raw);
}
async fn no_wire(peer: &mut Peer) {
    let mut byte = [0];
    assert!(
        timeout(Duration::from_millis(15), peer.reader.read(&mut byte))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn p0_a_v1_three_chunk_tags_omitted_null_string_empty_and_continue() {
    let (mut client, mut peer) = ready(Version::V1, Version::V1).await;
    for tag in TAGS {
        accepted(&mut client, &mut peer, tag, None, None, 1).await;
        accepted(&mut client, &mut peer, tag, Some(&Value::Null), None, 2).await;
        accepted(
            &mut client,
            &mut peer,
            tag,
            Some(&json!("native-id")),
            Some("native-id"),
            3,
        )
        .await;
        accepted(&mut client, &mut peer, tag, Some(&json!("")), Some(""), 4).await;
    }
    assert_eq!(client.pending_count(), 0);
    no_wire(&mut peer).await;
}

#[tokio::test]
async fn p0_b_v1_id_limit_is_4096_utf8_bytes_for_each_tag() {
    for tag in TAGS {
        for text in ["x".repeat(4096), "é".repeat(2048)] {
            let (mut client, mut peer) = ready(Version::V1, Version::V1).await;
            accepted(
                &mut client,
                &mut peer,
                tag,
                Some(&json!(text)),
                Some(&text),
                0,
            )
            .await;
        }
        for text in ["x".repeat(4097), "é".repeat(2048) + "x"] {
            let (mut client, mut peer) = ready(Version::V1, Version::V1).await;
            accepted(
                &mut client,
                &mut peer,
                tag,
                Some(&json!(text)),
                Some(&text),
                0,
            )
            .await;
        }
    }
}

#[tokio::test]
async fn p0_c_v1_wrong_shapes_do_not_enter_null_branch() {
    for tag in TAGS {
        for wrong in [
            json!(7),
            json!(1.5),
            json!(false),
            json!([]),
            json!({"id":"x"}),
        ] {
            rejected(Version::V1, tag, &wrong).await;
        }
    }
}

#[tokio::test]
async fn p0_d_offer_v2_selected_v1_uses_selected_rules_once() {
    let (mut client, mut peer) = ready(Version::V2, Version::V1).await;
    for tag in TAGS {
        accepted(&mut client, &mut peer, tag, Some(&Value::Null), None, 0).await;
        accepted(
            &mut client,
            &mut peer,
            tag,
            Some(&json!("after-null")),
            Some("after-null"),
            1,
        )
        .await;
    }
    no_wire(&mut peer).await;
}

#[tokio::test]
async fn p0_e_raw_order_origin_live_replay_resume_and_after_response() {
    for mode in 0..3 {
        let (mut client, mut peer) = ready(Version::V1, Version::V1).await;
        let pending = if mode == 0 {
            None
        } else {
            let strategy = client.strategy().unwrap();
            let target = SessionId::parse(raw_value(&json!("A")).as_ref()).unwrap();
            let request = if mode == 1 {
                strategy.legacy_replay(target, "C:/synthetic").unwrap()
            } else {
                strategy.legacy_resume(target, "C:/synthetic").unwrap()
            };
            let rpc = client.request(request, deadline()).await.unwrap();
            assert_eq!(
                wire(&mut peer).await["method"],
                if mode == 1 {
                    "session/load"
                } else {
                    "session/resume"
                }
            );
            Some(rpc)
        };
        let origin = match mode {
            1 => UpdateOrigin::DuringReplay,
            2 => UpdateOrigin::DuringResume,
            _ => UpdateOrigin::Live,
        };
        for (sequence, tag) in (0u32..).zip(TAGS) {
            let value = frame(tag, Some(&Value::Null), sequence, "A");
            let raw = format!("  {}  ", serde_json::to_string(&value).unwrap()).into_bytes();
            let event = next_raw(&mut client, &mut peer, &raw).await.unwrap();
            assert_update(&event, &raw, tag, None, "A", origin);
            let other = serde_json::to_vec(&frame(tag, Some(&Value::Null), sequence, "B")).unwrap();
            let event = next_raw(&mut client, &mut peer, &other).await.unwrap();
            assert_update(&event, &other, tag, None, "B", UpdateOrigin::Live);
        }
        if let Some(rpc) = pending {
            let raw =
                serde_json::to_vec(&json!({"jsonrpc":"2.0","id":rpc.json(),"result":{}})).unwrap();
            next_raw(&mut client, &mut peer, &raw).await.unwrap();
        }
        accepted(
            &mut client,
            &mut peer,
            TAGS[0],
            Some(&json!("last")),
            Some("last"),
            9,
        )
        .await;
        assert_eq!(client.pending_count(), 0);
    }
}

#[tokio::test]
async fn p0_f_v2_current_coarse_projection_behavior_is_unchanged() {
    for tag in TAGS {
        let (mut client, mut peer) = ready(Version::V2, Version::V2).await;
        accepted(&mut client, &mut peer, tag, None, None, 0).await;
        for text in [String::new(), "v2-id".to_owned(), "é".repeat(2048)] {
            accepted(
                &mut client,
                &mut peer,
                tag,
                Some(&json!(text)),
                Some(&text),
                1,
            )
            .await;
        }
        let text = "x".repeat(4097);
        accepted(
            &mut client,
            &mut peer,
            tag,
            Some(&json!(text)),
            Some(&text),
            2,
        )
        .await;
        for wrong in [
            Value::Null,
            json!(7),
            json!(1.5),
            json!(true),
            json!([]),
            json!({}),
        ] {
            rejected(Version::V2, tag, &wrong).await;
        }
    }
}
