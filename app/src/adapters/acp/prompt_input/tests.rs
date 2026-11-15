use super::*;
use crate::adapters::acp::{
    ExtensionFields, Incoming, Response, SessionId, Version,
    test_support::{RpcJson, TestClient},
};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf},
    time::{Instant, timeout_at},
};

#[test]
fn full_upload_binding_and_root_retention_share_storage() {
    let uploaded = b" \n[{\"type\":\"text\",\"text\":\"x\\u0061\",\"unknown\":9007199254740993,\"n\":1e+009}]\t".to_vec();
    let pointer = uploaded.as_ptr();
    let expected: [u8; 32] = Sha256::digest(&uploaded).into();
    let document = PromptDocument::parse(uploaded.clone()).unwrap();
    let moved = PromptDocument::parse(uploaded).unwrap();
    assert_eq!(moved.uploaded_bytes().as_ptr(), pointer);
    assert_eq!(moved.uploaded_bytes(), document.uploaded_bytes());
    let adapter = Arc::new(AcpNativeAdapter::default());
    let (body, digest) = adapter.validate(&moved).unwrap();
    assert_eq!(digest, expected);
    assert_ne!(
        digest,
        <[u8; 32]>::from(Sha256::digest(moved.prompt_fragment()))
    );
    assert_eq!(body.uploaded_bytes().as_ptr(), pointer);
    let root_pointer = body.prompt_fragment().as_ptr();
    let bound = ValidatedBody::new(body, digest);
    let retained = adapter.retain(&bound);
    assert_eq!(retained.as_ref().fragment().as_ptr(), root_pointer);
    assert_eq!(retained.as_ref().fragment(), moved.prompt_fragment());
    drop(moved);
    drop(bound);
    assert_eq!(retained.as_ref().fragment(), document.prompt_fragment());
    let changed = PromptDocument::parse(document.prompt_fragment().as_bytes().to_vec()).unwrap();
    assert_ne!(adapter.validate(&changed).unwrap().1, expected);
}

#[test]
fn complete_structure_rejects_late_damage_and_preserves_extensions() {
    let original = br#"[{"type":"future","payload":{"n":1e9999,"raw":"\u0061"}},{"type":"resource","resource":{"uri":"opaque","text":"x","blob":"y"}}]"#;
    let parsed = PromptDocument::parse(original.to_vec()).unwrap();
    assert_eq!(parsed.uploaded_bytes(), original);
    let duplicate_extension = br#"[{"type":"text","text":"x","unknown":{"a":1,"\u0061":2}}]"#;
    assert!(matches!(
        PromptDocument::parse(duplicate_extension.to_vec()),
        Err(AcpError::InvalidMessage)
    ));
    for ambiguous in [
        br#"[{"type":"future","payload":{"n":1,"n":2}}]"#.as_slice(),
        br#"[{"type":"future","payload":[{"a":1},{"a":2}]},{}]"#.as_slice(),
    ] {
        assert!(matches!(
            PromptDocument::parse(ambiguous.to_vec()),
            Err(AcpError::InvalidMessage)
        ));
    }
    let nested_unknown =
        br#"[{"type":"future","payload":{"a":1,"b":{"\u0061":2}},"extra":[{"c":3}]}"#;
    assert!(matches!(
        PromptDocument::parse(nested_unknown.to_vec()),
        Err(AcpError::InvalidMessage)
    ));
    for ambiguous in [
        br#"[{"type":"text","text":"x","text":"y"}]"#.as_slice(),
        br#"[{"type":"text","\u0074ype":"image","text":"x"}]"#,
    ] {
        assert!(matches!(
            PromptDocument::parse(ambiguous.to_vec()),
            Err(AcpError::InvalidMessage)
        ));
    }
    for invalid in [
        b"{}".as_slice(),
        br#"[{"type":"future"},{"type":"image","data":7,"mimeType":"image/x"}]"#,
        br#"[{"type":"resource","resource":{"uri":"opaque","text":null,"blob":7}}]"#,
        br#"[{"type":"text","text":"x"}] []"#,
    ] {
        assert!(matches!(
            PromptDocument::parse(invalid.to_vec()),
            Err(AcpError::InvalidMessage)
        ));
    }
    assert_eq!(
        AcpError::from(super::super::raw_json::RawJsonError::Allocation),
        AcpError::Allocation
    );
}

type Peer = (BufReader<ReadHalf<DuplexStream>>, WriteHalf<DuplexStream>);

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

async fn read(peer: &mut Peer) -> Vec<u8> {
    let mut line = Vec::new();
    let size = timeout_at(deadline(), peer.0.read_until(b'\n', &mut line))
        .await
        .unwrap()
        .unwrap();
    assert!(size > 0);
    line
}

async fn feed(peer: &mut Peer, value: &Value) {
    let mut bytes = serde_json::to_vec(value).unwrap();
    bytes.push(b'\n');
    timeout_at(deadline(), peer.1.write_all(&bytes))
        .await
        .unwrap()
        .unwrap();
}

async fn ready(selected: Version, sessions: bool) -> (TestClient, Peer) {
    let (a, b) = tokio::io::duplex(65536);
    let (ar, aw) = tokio::io::split(a);
    let (br, bw) = tokio::io::split(b);
    let mut client = TestClient::new(ar, aw);
    let mut peer = (BufReader::new(br), bw);
    let id = client.initialize_preferred(deadline()).await.unwrap();
    let init: Value = serde_json::from_slice(&read(&mut peer).await).unwrap();
    assert_eq!(init["params"]["protocolVersion"], 2);
    let result = match selected {
        Version::V1 => json!({"protocolVersion":1,"agentCapabilities":{}}),
        Version::V2 => {
            json!({"protocolVersion":2,"info":{"name":"prompt-dev","version":"fixed"},"capabilities":if sessions {json!({"session":{}})} else {json!({})}})
        }
    };
    feed(
        &mut peer,
        &json!({"jsonrpc":"2.0","id":id.json(),"result":result}),
    )
    .await;
    assert!(matches!(
        client.next(deadline()).await.unwrap(),
        Incoming::Response {
            response: Response::Initialized(_),
            ..
        }
    ));
    assert_eq!(client.strategy().unwrap().version(), selected);
    (client, peer)
}

fn root(bytes: &[u8]) -> OwnedRawValue {
    RawDocument::parse(bytes.to_vec()).unwrap().root_owned()
}

fn session() -> SessionId {
    SessionId::parse(root(br#""s""#).as_ref()).unwrap()
}

#[tokio::test]
async fn negotiated_prompt_preserves_early_errors_and_exact_wire() {
    for selected in [Version::V1, Version::V2] {
        let (client, mut peer) = ready(selected, true).await;
        let strategy = &client.strategy().unwrap().0;
        for (bytes, expected) in [
            (
                br#"[{"type":"image","data":false},{"type":"text"}]"#.as_slice(),
                AcpError::UnsupportedCapability,
            ),
            (
                br#"[{"type":"text","text":false},{"type":"image"}]"#,
                AcpError::InvalidMessage,
            ),
            (
                br#"[{"type":"future"},7]"#,
                if selected == Version::V1 {
                    AcpError::UnsupportedCapability
                } else {
                    AcpError::InvalidMessage
                },
            ),
        ] {
            assert_eq!(
                strategy
                    .prompt(session(), root(bytes), ExtensionFields::empty().unwrap())
                    .err(),
                Some(expected)
            );
        }
        let extra = ExtensionFields::from_raw(root(br#"{"prompt":[]}"#)).unwrap();
        assert_eq!(
            strategy.prompt(session(), root(b"{}"), extra).err(),
            Some(AcpError::UnencodableConfiguration)
        );
        let document = PromptDocument::parse(
            br#" [{"type":"text","text":"wire","unknown":{"n":1e9999}}] "#.to_vec(),
        )
        .unwrap();
        let adapter = Arc::new(AcpNativeAdapter::default());
        let (body, digest) = adapter.validate(&document).unwrap();
        let retained = adapter.retain(&ValidatedBody::new(body, digest));
        let request = strategy
            .prompt(session(), retained, ExtensionFields::empty().unwrap())
            .unwrap();
        let id = client.request(request, deadline()).await.unwrap();
        let wire = RawDocument::parse(read(&mut peer).await).unwrap();
        let params = wire
            .root()
            .object()
            .unwrap()
            .get("params")
            .expect("unique decoded field")
            .unwrap()
            .object()
            .unwrap();
        assert_eq!(
            params
                .get("prompt")
                .expect("unique decoded field")
                .unwrap()
                .fragment(),
            document.prompt_fragment()
        );
        let result = if selected == Version::V1 {
            json!({"stopReason":"end_turn"})
        } else {
            json!({"messageId":"m"})
        };
        feed(
            &mut peer,
            &json!({"jsonrpc":"2.0","id":id.json(),"result":result}),
        )
        .await;
        let mut client = client;
        assert!(matches!(
            client.next(deadline()).await.unwrap(),
            Incoming::Response { .. }
        ));
        client.shutdown(deadline()).await.unwrap();
        assert!(client.is_closed());
        assert!(read_eof(&mut peer).await);
    }
}

async fn read_eof(peer: &mut Peer) -> bool {
    let mut line = Vec::new();
    timeout_at(deadline(), peer.0.read_until(b'\n', &mut line))
        .await
        .unwrap()
        .unwrap()
        == 0
}

#[tokio::test]
async fn missing_sessions_precedes_reserved_extra_and_bad_root() {
    let (client, mut peer) = ready(Version::V2, false).await;
    let extra = ExtensionFields::from_raw(root(br#"{"sessionId":"reserved"}"#)).unwrap();
    assert_eq!(
        client
            .strategy()
            .unwrap()
            .0
            .prompt(session(), root(b"{}"), extra)
            .err(),
        Some(AcpError::UnsupportedCapability)
    );
    client.shutdown(deadline()).await.unwrap();
    assert!(client.is_closed());
    assert!(read_eof(&mut peer).await);
}
