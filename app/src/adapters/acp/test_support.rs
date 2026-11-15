//! Developer-only migration adapter. Every transport action uses the real public client.
//! Value is used solely for old small fixture construction/assertions; never product IO.
use super::*;
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    time::{Instant, timeout_at},
};
pub(super) fn raw_value(v: &Value) -> raw_json::OwnedRawValue {
    raw_json::RawDocument::parse(serde_json::to_vec(v).unwrap())
        .unwrap()
        .root_owned()
}
pub(super) fn json_fields(raw: &RawMessage) -> Value {
    serde_json::from_slice(raw.bytes()).unwrap()
}
pub(super) fn parse_session(v: &Value) -> Result<SessionId, AcpError> {
    SessionId::parse(raw_value(v).as_ref())
}
pub(super) fn parse_tool(v: &Value) -> Result<ToolCallId, AcpError> {
    ToolCallId::parse(raw_value(v).as_ref())
}

pub(super) fn parse_option(v: &Value) -> Result<OptionId, AcpError> {
    OptionId::parse(raw_value(v).as_ref())
}
pub(super) fn parse_rpc(v: &Value) -> Result<RpcId, AcpError> {
    RpcId::parse(raw_value(v).as_ref())
}
pub(super) trait RpcJson {
    fn json(&self) -> Value;
}
impl RpcJson for RpcId {
    fn json(&self) -> Value {
        let mut v = Vec::new();
        self.write_json(&mut v).unwrap();
        serde_json::from_slice(&v).unwrap()
    }
}
pub(super) struct Allow;
impl WriteFence for Allow {
    fn check_and_poll(
        &self,
        _: &WriteIdentity,
        _: WriteStep,
        cx: &mut std::task::Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        let _ = io.poll(cx);
        Ok(())
    }
}
pub(super) fn setup(wire: Version, cwd: &str) -> SessionSetup {
    SessionSetup::try_from_snapshot(SetupSnapshot {
        wire,
        source: ConfigSourceStamp {
            reference: "fixture".into(),
            revision: 1,
        },
        cwd: Known::Known(cwd.into()),
        mcp_servers: Known::Known(Vec::new()),
        additional_directories: Known::Known(Vec::new()),
        extra: ExtensionFields::empty().unwrap(),
    })
    .unwrap()
}
#[derive(Clone, Debug)]
pub(super) struct TestStrategy(pub Strategy);
impl TestStrategy {
    pub fn version(&self) -> Version {
        self.0.version()
    }
    pub fn negotiated(&self) -> &Negotiated {
        self.0.negotiated()
    }
    pub fn new_session(&self, cwd: &str) -> Result<Request, AcpError> {
        self.0.new_session(&setup(self.version(), cwd))
    }
    pub fn resume(&self, s: SessionId, cwd: &str) -> Result<Request, AcpError> {
        self.0.resume(s, &setup(self.version(), cwd))
    }
    pub fn replay(&self, s: SessionId, cwd: &str) -> Result<Request, AcpError> {
        self.0.replay(s, &setup(self.version(), cwd))
    }
    pub fn prompt(&self, s: SessionId, v: &Value) -> Result<Request, AcpError> {
        self.0
            .prompt(s, raw_value(v), ExtensionFields::empty().unwrap())
    }
    pub fn close(&self, s: SessionId) -> Result<Request, AcpError> {
        self.0.close(s, ExtensionFields::empty().unwrap())
    }
    pub fn list_sessions(
        &self,
        cwd: Option<&str>,
        cursor: Option<&str>,
    ) -> Result<Request, AcpError> {
        self.0
            .list_sessions(cwd, cursor, ExtensionFields::empty().unwrap())
    }
    pub fn prompt_result(&self, v: &Value) -> Result<PromptResult, AcpError> {
        self.0.prompt_result(raw_value(v).as_ref())
    }

    pub fn tool_name_patch(&self, v: &Value) -> Result<ToolNamePatch, AcpError> {
        self.0.tool_name_patch(raw_value(v).as_ref())
    }
    pub fn tool_metadata_patch(&self, current: &Value, update: &Value) -> Result<Value, AcpError> {
        let current = RawMetadata::from_raw(raw_value(current))?;
        let result = self
            .0
            .tool_metadata_patch(&current, raw_value(update).as_ref())?;
        Ok(serde_json::from_str(result.to_raw()?.as_ref().fragment()).unwrap())
    }
}
pub(super) struct TestClient {
    pub connection: Connection,
    pub receiver: EventReceiver,
    pub owner: IoOwner,
    selected: Option<TestStrategy>,
    unsupported: HashMap<RpcId, UnsupportedRequest>,
}
impl TestClient {
    pub fn new<R, W>(r: R, w: W) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (connection, receiver, owner) = Connection::spawn(r, w).unwrap();
        Self {
            connection,
            receiver,
            owner,
            selected: None,
            unsupported: HashMap::new(),
        }
    }
    pub fn strategy(&self) -> Option<&TestStrategy> {
        self.selected.as_ref()
    }
    async fn send(&self, r: Reservation, deadline: Instant) -> Result<(), AcpError> {
        let frame = r.encode()?;
        let ticket = self
            .connection
            .enqueue(r, frame, Arc::new(Allow), deadline)
            .map_err(|e| e.error())?;
        match ticket.wait().await {
            WriteReport::Written => Ok(()),
            WriteReport::Zero {
                reason: ZeroReason::Deadline,
            }
            | WriteReport::Unknown {
                cause: WriteFailure::Deadline,
                ..
            } => Err(AcpError::Deadline),
            _ => Err(AcpError::DeliveryUnknown),
        }
    }
    pub async fn initialize(&self, v: Version, d: Instant) -> Result<RpcId, AcpError> {
        let r = self.connection.reserve_initialize(v, d)?;
        let id = r.id().unwrap().clone();
        self.send(r, d).await?;
        Ok(id)
    }
    pub async fn initialize_preferred(&self, d: Instant) -> Result<RpcId, AcpError> {
        self.initialize(Version::preferred(), d).await
    }
    pub async fn request(&self, r: Request, d: Instant) -> Result<RpcId, AcpError> {
        let r = self
            .connection
            .reserve_request(r, ResponseDeadline::At(d))?;
        let id = r.id().unwrap().clone();
        self.send(r, d).await?;
        Ok(id)
    }
    pub async fn cancel(&self, s: &SessionId, d: Instant) -> Result<(), AcpError> {
        let r = self
            .connection
            .reserve_cancel(s.clone(), ExtensionFields::empty().unwrap())?;
        self.send(r, d).await
    }
    pub async fn permission_response(
        &self,
        r: &PermissionRequest,
        id: Option<&OptionId>,
        d: Instant,
    ) -> Result<(), AcpError> {
        let selection = id.map_or(PermissionSelection::Cancelled, |id| {
            PermissionSelection::Selected(id.clone())
        });
        let r = self.connection.reserve_permission_response(r, selection)?;
        self.send(r, d).await
    }
    pub async fn reject_unsupported(&self, id: &RpcId, d: Instant) -> Result<(), AcpError> {
        let r = self.connection.reserve_unsupported(
            self.unsupported
                .get(id)
                .ok_or(AcpError::InvalidRequestScope)?,
        )?;
        self.send(r, d).await
    }
    pub async fn next_received(&mut self, d: Instant) -> Result<Received, AcpError> {
        let r = timeout_at(d, self.receiver.next())
            .await
            .map_err(|_| AcpError::Deadline)??
            .ok_or(AcpError::Closed)?;
        match r.incoming() {
            Incoming::Response {
                response: Response::Initialized(s),
                ..
            } => self.selected = Some(TestStrategy(s.clone())),
            Incoming::UnsupportedRequest(u) => {
                self.unsupported.insert(u.id().clone(), u.clone());
            }
            _ => {}
        }
        Ok(r)
    }
    pub async fn next(&mut self, d: Instant) -> Result<Incoming, AcpError> {
        Ok(self.next_received(d).await?.incoming().clone())
    }
    pub async fn shutdown(&self, d: Instant) -> Result<(), AcpError> {
        if self.owner.shutdown(d).await.joined {
            Ok(())
        } else {
            Err(AcpError::Deadline)
        }
    }
    pub fn is_closed(&self) -> bool {
        self.connection.is_closed()
    }
    pub fn pending_count(&self) -> usize {
        self.connection.pending_count()
    }
    pub fn take_rejected_message(&self) -> Option<RawMessage> {
        self.connection.take_rejected_message()
    }
}
// Pure legacy projection units use a Strategy from an actual completed handshake, on a
// separate owned runtime. They call private data algorithms and make no live-source claim.
pub(super) fn unit_strategy(wire: Version) -> Strategy {
    std::thread::spawn(move||{tokio::runtime::Runtime::new().unwrap().block_on(async move{
  use tokio::io::{AsyncBufReadExt,AsyncWriteExt,BufReader};
  let(a,b)=tokio::io::duplex(65536);let(ar,aw)=tokio::io::split(a);let(br,mut bw)=tokio::io::split(b);let mut client=TestClient::new(ar,aw);
  let id=client.initialize(Version::V2,Instant::now()+std::time::Duration::from_secs(5)).await.unwrap();
  let mut reader=BufReader::new(br);let mut line=String::new();reader.read_line(&mut line).await.unwrap();
  let result=if wire==Version::V1{json!({"protocolVersion":1,"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"resume":{}}}})}else{json!({"protocolVersion":2,"info":{"name":"fixture","version":"fixed"},"capabilities":{"session":{}}})};
  let mut bytes=serde_json::to_vec(&json!({"jsonrpc":"2.0","id":id.json(),"result":result})).unwrap();bytes.push(b'\n');bw.write_all(&bytes).await.unwrap();client.next(Instant::now()+std::time::Duration::from_secs(5)).await.unwrap();let selected=client.strategy().unwrap().0.clone();client.shutdown(Instant::now()+std::time::Duration::from_secs(5)).await.unwrap();selected
 })}).join().unwrap()
}
impl std::ops::Deref for TestStrategy {
    type Target = Strategy;
    fn deref(&self) -> &Strategy {
        &self.0
    }
}
