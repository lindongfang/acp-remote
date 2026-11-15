use super::bridge::*;
use acp_remote::{
    adapters::acp::{
        AcpError, Version,
        tool_projection::{ProjectionStep, ToolProjectionRead},
    },
    domain::{EntityId, native::*, views::*},
};
use serde_json::{Value, json};
use tokio::{
    io::{
        AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf,
    },
    time::{Duration, Instant, timeout},
};

pub type Client = TestClient;
pub struct Peer {
    reader: BufReader<ReadHalf<DuplexStream>>,
    writer: WriteHalf<DuplexStream>,
}
pub fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}
pub async fn wire(peer: &mut Peer) -> Value {
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
pub async fn next_raw(
    client: &mut Client,
    peer: &mut Peer,
    raw: &[u8],
) -> Result<acp_remote::adapters::acp::Received, AcpError> {
    let (write, incoming) = tokio::join!(
        async {
            peer.writer.write_all(raw).await?;
            peer.writer.write_all(b"\n").await
        },
        client.next(deadline())
    );
    write.unwrap();
    incoming
}
pub async fn ready(selected: Version) -> (Client, Peer) {
    let (c, p) = tokio::io::duplex(64 * 1024);
    let (r, w) = tokio::io::split(c);
    let (pr, pw) = tokio::io::split(p);
    let mut client = TestClient::new(r, w);
    let mut peer = Peer {
        reader: BufReader::new(pr),
        writer: pw,
    };
    let id = client.initialize_preferred(deadline()).await.unwrap();
    assert_eq!(wire(&mut peer).await["params"]["protocolVersion"], 2);
    let result = if selected == Version::V1 {
        json!({"protocolVersion":1,"agentInfo":{"name":"independent","version":"synthetic"},"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"resume":{}}}})
    } else {
        json!({"protocolVersion":2,"info":{"name":"independent","version":"synthetic"},"capabilities":{"session":{}}})
    };
    next_raw(
        &mut client,
        &mut peer,
        &serde_json::to_vec(&json!({"jsonrpc":"2.0","id":id.json(),"result":result})).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(client.strategy().unwrap().version(), selected);
    (client, peer)
}
pub const fn epoch(n: u8) -> ResourceEpoch {
    ResourceEpoch {
        runtime: RuntimeEpoch(EntityId::from_bytes([n; 16])),
        connection: HarnessConnectionEpoch(EntityId::from_bytes([n; 16])),
        load: LoadEpoch(EntityId::from_bytes([n; 16])),
    }
}
pub fn target() -> NativeSessionKey {
    NativeSessionKey::from_config(Some(EntityId::from_bytes([1; 16])), "A").unwrap()
}
pub const fn key(id: &str) -> SourceKey<'_> {
    SourceKey {
        namespace: KeyNamespace::Tool,
        id,
    }
}
pub fn frame(update: Value) -> Vec<u8> {
    let mut envelope = json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"A"},"_meta":{"source":"synthetic"}});
    envelope["params"]
        .as_object_mut()
        .unwrap()
        .insert("update".to_owned(), update);
    serde_json::to_vec(&envelope).unwrap()
}
pub struct Rig {
    pub windows: ViewWindows,
    pub h: ViewHandle,
    pub read: ToolProjectionRead,
    pub client: Client,
    pub peer: Peer,
}
impl Rig {
    pub async fn new(version: Version, bytes: usize, items: usize) -> Self {
        let (client, peer) = ready(version).await;
        let mut windows = ViewWindows::new(ViewLimits {
            target_bytes: bytes,
            target_items: items,
            ..ViewLimits::default()
        })
        .unwrap();
        let h = windows.begin(target(), epoch(1)).unwrap();
        let read = ToolProjectionRead::begin(&mut windows, &h, client.strategy().unwrap()).unwrap();
        Self {
            windows,
            h,
            read,
            client,
            peer,
        }
    }
    pub async fn incoming(&mut self, bytes: &[u8]) -> acp_remote::adapters::acp::Received {
        next_raw(&mut self.client, &mut self.peer, bytes)
            .await
            .unwrap()
    }
    pub async fn apply(&mut self, update: Value) -> ProjectionStep {
        self.apply_bytes(&frame(update)).await
    }
    pub async fn apply_bytes(&mut self, bytes: &[u8]) -> ProjectionStep {
        let input = self.incoming(bytes).await;
        self.read.apply(&mut self.windows, &input).unwrap()
    }
    pub fn anchor(&mut self, id: &str) -> (u64, Vec<u8>) {
        let lease = self.windows.item_lease(&self.h, key(id)).unwrap();
        (lease.item().position, lease.item().bytes.to_vec())
    }
    pub fn usage(&self) -> ViewUsage {
        self.windows.target_usage(&target()).unwrap()
    }
    pub async fn no_wire(&mut self) {
        let mut byte = [0];
        assert!(
            timeout(Duration::from_millis(10), self.peer.reader.read(&mut byte))
                .await
                .is_err()
        );
    }
}
pub fn u64_at(bytes: &[u8], n: usize) -> u64 {
    u64::from_le_bytes(bytes[n..n + 8].try_into().unwrap())
}
pub fn put(bytes: &mut [u8], n: usize, value: u64) {
    bytes[n..n + 8].copy_from_slice(&value.to_le_bytes());
}
