#![allow(dead_code)] // Shared independent fixtures serve separately selected targets.
use acp_remote::adapters::acp::{
    raw_json::{OwnedRawValue, RawDocument},
    *,
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io,
    pin::Pin,
    sync::{Arc, Mutex, Weak},
    task::{Context, Poll, Waker},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::Notify,
    time::{Instant, timeout},
};

pub fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(20)
}
pub fn owned(bytes: impl AsRef<[u8]>) -> OwnedRawValue {
    RawDocument::parse(bytes.as_ref().to_vec())
        .unwrap()
        .root_owned()
}
pub fn raw_value(value: &Value) -> OwnedRawValue {
    owned(serde_json::to_vec(value).unwrap())
}
pub fn sid(s: &str) -> SessionId {
    SessionId::parse(raw_value(&json!(s)).as_ref()).unwrap()
}
pub fn oid(s: &str) -> OptionId {
    OptionId::parse(raw_value(&json!(s)).as_ref()).unwrap()
}
pub fn rpc_value(id: &RpcId) -> Value {
    let mut bytes = Vec::new();
    id.write_json(&mut bytes).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
pub fn setup(version: Version, cwd: &str) -> SessionSetup {
    SessionSetup::try_from_snapshot(SetupSnapshot {
        wire: version,
        source: ConfigSourceStamp {
            reference: Arc::from("independent-synthetic"),
            revision: 7,
        },
        cwd: Known::Known(Arc::from(cwd)),
        mcp_servers: Known::Known(vec![]),
        additional_directories: Known::Known(vec![]),
        extra: ExtensionFields::empty().unwrap(),
    })
    .unwrap()
}

pub struct Fence;
impl WriteFence for Fence {
    fn check_and_poll(
        &self,
        _: &WriteIdentity,
        _: WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        let _actual = io.poll(cx);
        Ok(())
    }
}
pub fn send(client: &Connection, reservation: Reservation) -> WriteTicket {
    let frame = reservation.encode().unwrap();
    client
        .enqueue(reservation, frame, Arc::new(Fence), deadline())
        .unwrap()
}
pub async fn finish(ticket: &WriteTicket) {
    assert!(matches!(
        timeout(Duration::from_secs(20), ticket.wait())
            .await
            .unwrap(),
        WriteReport::Written
    ));
}

#[derive(Default)]
struct ReadState {
    chunks: VecDeque<Vec<u8>>,
    offset: usize,
    eof: bool,
    fail: bool,
    waker: Option<Waker>,
    polls: usize,
    ready: usize,
    open_writer_at_lf: Option<WriteControl>,
    open_writer_at_terminal: Option<WriteControl>,
    terminal_writer_poll_snapshot: Option<usize>,
}
#[derive(Clone)]
pub struct ReadControl {
    state: Arc<Mutex<ReadState>>,
    pub ready: Arc<Notify>,
}
pub struct ScriptRead {
    control: ReadControl,
    witness: Arc<()>,
}
impl ReadControl {
    pub fn new() -> (Self, ScriptRead, Weak<()>) {
        let control = Self {
            state: Arc::new(Mutex::new(ReadState::default())),
            ready: Arc::new(Notify::new()),
        };
        let witness = Arc::new(());
        let weak = Arc::downgrade(&witness);
        (control.clone(), ScriptRead { control, witness }, weak)
    }
    pub fn push(&self, bytes: Vec<u8>) {
        let wake = {
            let mut s = self.state.lock().unwrap();
            s.chunks.push_back(bytes);
            s.waker.take()
        };
        if let Some(w) = wake {
            w.wake();
        }
    }
    pub fn frame(&self, bytes: impl AsRef<[u8]>) {
        let mut frame = bytes.as_ref().to_vec();
        frame.push(b'\n');
        self.push(frame);
    }
    pub fn eof(&self) {
        let w = {
            let mut s = self.state.lock().unwrap();
            s.eof = true;
            s.waker.take()
        };
        if let Some(w) = w {
            w.wake();
        }
    }
    pub fn fail(&self) {
        let w = {
            let mut s = self.state.lock().unwrap();
            s.fail = true;
            s.waker.take()
        };
        if let Some(w) = w {
            w.wake();
        }
    }
    pub fn terminal_and_open_writer(&self, writer: WriteControl, fail: bool) {
        let wake = {
            let mut s = self.state.lock().unwrap();
            s.open_writer_at_terminal = Some(writer);
            s.fail = fail;
            s.eof = !fail;
            s.waker.take()
        };
        if let Some(w) = wake {
            w.wake();
        }
    }
    pub fn terminal_writer_polls(&self) -> Option<usize> {
        self.state.lock().unwrap().terminal_writer_poll_snapshot
    }
    pub fn polls(&self) -> usize {
        self.state.lock().unwrap().polls
    }
    pub fn batches(&self) -> usize {
        self.state.lock().unwrap().ready
    }
    pub fn open_writer_at_next_lf(&self, writer: WriteControl) {
        self.state.lock().unwrap().open_writer_at_lf = Some(writer);
    }
    pub async fn wait_batches(&self, n: usize) {
        timeout(Duration::from_secs(20), async {
            loop {
                let notified = self.ready.notified();
                if self.batches() >= n {
                    return;
                }
                notified.await;
            }
        })
        .await
        .unwrap();
    }
}
impl AsyncRead for ScriptRead {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let mut s = this.control.state.lock().unwrap();
        s.polls += 1;
        if s.fail {
            let open = s.open_writer_at_terminal.take();
            s.terminal_writer_poll_snapshot = open.as_ref().map(WriteControl::polls);
            drop(s);
            if let Some(w) = open {
                w.mode(WriteMode::Open);
            }
            return Poll::Ready(Err(io::Error::other("independent-read-failure")));
        }
        if let Some(chunk) = s.chunks.front() {
            let n = (chunk.len() - s.offset).min(buf.remaining());
            let has_lf = chunk[s.offset..s.offset + n].contains(&b'\n');
            buf.put_slice(&chunk[s.offset..s.offset + n]);
            s.offset += n;
            if s.offset == s.chunks.front().unwrap().len() {
                s.chunks.pop_front();
                s.offset = 0;
            }
            s.ready += 1;
            let open = if has_lf {
                s.open_writer_at_lf.take()
            } else {
                None
            };
            drop(s);
            if let Some(w) = open {
                w.mode(WriteMode::Open);
            }
            this.control.ready.notify_waiters();
            return Poll::Ready(Ok(()));
        }
        if s.eof {
            let open = s.open_writer_at_terminal.take();
            s.terminal_writer_poll_snapshot = open.as_ref().map(WriteControl::polls);
            drop(s);
            if let Some(w) = open {
                w.mode(WriteMode::Open);
            }
            return Poll::Ready(Ok(()));
        }
        s.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

#[derive(Clone, Copy, Debug)]
pub enum WriteMode {
    Open,
    Pending,
    Limit(usize),
    Fail,
}
struct WriteState {
    bytes: Vec<u8>,
    mode: WriteMode,
    flush: WriteMode,
    waker: Option<Waker>,
    polls: usize,
    flushes: usize,
    shutdowns: usize,
}
#[derive(Clone)]
pub struct WriteControl {
    state: Arc<Mutex<WriteState>>,
    pub changed: Arc<Notify>,
}
pub struct ScriptWrite {
    control: WriteControl,
    witness: Arc<()>,
}
impl WriteControl {
    pub fn new() -> (Self, ScriptWrite, Weak<()>) {
        let control = Self {
            state: Arc::new(Mutex::new(WriteState {
                bytes: vec![],
                mode: WriteMode::Open,
                flush: WriteMode::Open,
                waker: None,
                polls: 0,
                flushes: 0,
                shutdowns: 0,
            })),
            changed: Arc::new(Notify::new()),
        };
        let witness = Arc::new(());
        let weak = Arc::downgrade(&witness);
        (control.clone(), ScriptWrite { control, witness }, weak)
    }
    pub fn mode(&self, mode: WriteMode) {
        let w = {
            let mut s = self.state.lock().unwrap();
            s.mode = mode;
            s.waker.take()
        };
        if let Some(w) = w {
            w.wake();
        }
    }
    pub fn flush(&self, mode: WriteMode) {
        let w = {
            let mut s = self.state.lock().unwrap();
            s.flush = mode;
            s.waker.take()
        };
        if let Some(w) = w {
            w.wake();
        }
    }
    pub fn bytes(&self) -> Vec<u8> {
        self.state.lock().unwrap().bytes.clone()
    }
    pub fn clear(&self) {
        self.state.lock().unwrap().bytes.clear();
    }
    pub fn polls(&self) -> usize {
        self.state.lock().unwrap().polls
    }
    pub fn flushes(&self) -> usize {
        self.state.lock().unwrap().flushes
    }
    pub fn shutdowns(&self) -> usize {
        self.state.lock().unwrap().shutdowns
    }
    pub async fn wait_for(&self, predicate: impl Fn(&Self) -> bool + Send + Sync) {
        timeout(Duration::from_secs(20), async {
            loop {
                let n = self.changed.notified();
                if predicate(self) {
                    return;
                }
                n.await;
            }
        })
        .await
        .unwrap();
    }
}
impl AsyncWrite for ScriptWrite {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let mut s = this.control.state.lock().unwrap();
        s.polls += 1;
        let result = match s.mode {
            WriteMode::Open => {
                s.bytes.extend_from_slice(bytes);
                Poll::Ready(Ok(bytes.len()))
            }
            WriteMode::Limit(budget) => {
                let n = budget.min(bytes.len());
                s.bytes.extend_from_slice(&bytes[..n]);
                s.mode = if n == budget {
                    WriteMode::Pending
                } else {
                    WriteMode::Limit(budget - n)
                };
                Poll::Ready(Ok(n))
            }
            WriteMode::Pending => {
                s.waker = Some(cx.waker().clone());
                Poll::Pending
            }
            WriteMode::Fail => Poll::Ready(Err(io::Error::other("independent-write-failure"))),
        };
        drop(s);
        this.control.changed.notify_waiters();
        result
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let mut s = this.control.state.lock().unwrap();
        s.flushes += 1;
        let result = match s.flush {
            WriteMode::Pending => {
                s.waker = Some(cx.waker().clone());
                Poll::Pending
            }
            WriteMode::Fail => Poll::Ready(Err(io::Error::other("independent-flush-failure"))),
            _ => Poll::Ready(Ok(())),
        };
        drop(s);
        this.control.changed.notify_waiters();
        result
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.control.state.lock().unwrap().shutdowns += 1;
        Poll::Ready(Ok(()))
    }
}

pub struct Rig {
    pub client: Connection,
    pub receiver: EventReceiver,
    pub owner: IoOwner,
    pub read: ReadControl,
    pub write: WriteControl,
    pub read_weak: Weak<()>,
    pub write_weak: Weak<()>,
}
impl Rig {
    pub fn spawn() -> Self {
        let (read, r, read_weak) = ReadControl::new();
        let (write, w, write_weak) = WriteControl::new();
        let (client, receiver, owner) = Connection::spawn(r, w).unwrap();
        Self {
            client,
            receiver,
            owner,
            read,
            write,
            read_weak,
            write_weak,
        }
    }
    pub async fn ready(version: Version) -> Self {
        let mut r = Self::spawn();
        let res = r.client.reserve_initialize(version, deadline()).unwrap();
        let id = res.id().unwrap().clone();
        let ticket = send(&r.client, res);
        finish(&ticket).await;
        r.respond(&id, handshake(version)).await;
        assert_eq!(r.client.strategy().unwrap().version(), version);
        r.write.clear();
        r
    }
    pub async fn next(&mut self) -> Result<Option<Received>, AcpError> {
        timeout(Duration::from_secs(20), self.receiver.next())
            .await
            .unwrap()
    }
    pub async fn feed(&mut self, bytes: impl AsRef<[u8]>) -> Received {
        self.read.frame(bytes);
        self.next().await.unwrap().unwrap()
    }
    pub async fn respond(&mut self, id: &RpcId, result: Value) -> Received {
        self.feed(
            serde_json::to_vec(&json!({"jsonrpc":"2.0","id":rpc_value(id),"result":result}))
                .unwrap(),
        )
        .await
    }
    pub fn prompt(&self) -> Request {
        self.client
            .strategy()
            .unwrap()
            .prompt(
                sid("A"),
                owned(br#"[{"type":"text","text":"independent"}]"#),
                ExtensionFields::empty().unwrap(),
            )
            .unwrap()
    }
    pub fn reserve_prompt(&self) -> Reservation {
        self.client
            .reserve_request(self.prompt(), ResponseDeadline::Unbounded)
            .unwrap()
    }
    pub async fn close(&self) {
        assert!(self.owner.shutdown(deadline()).await.joined);
        assert!(self.read_weak.upgrade().is_none());
        assert!(self.write_weak.upgrade().is_none());
    }
}
pub fn handshake(version: Version) -> Value {
    match version {
        Version::V1 => {
            json!({"protocolVersion":1,"agentInfo":{"name":"independent","version":"synthetic"},"agentCapabilities":{"loadSession":true,"promptCapabilities":{"image":true,"audio":true,"embeddedContext":true},"mcpCapabilities":{"http":true,"sse":true},"sessionCapabilities":{"resume":{},"list":{},"close":{},"additionalDirectories":{}}}})
        }
        Version::V2 => {
            json!({"protocolVersion":2,"info":{"name":"independent","version":"synthetic"},"capabilities":{"session":{"additionalDirectories":{},"mcp":{"stdio":{},"http":{}},"list":{},"close":{},"prompt":{"image":{},"audio":{},"embeddedContext":{}}}}})
        }
    }
}
pub fn notification(session: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":session,"update":{"sessionUpdate":"future_display","opaque":1}}})).unwrap()
}
pub fn permission(version: Version, id: &RpcId) -> Vec<u8> {
    let params = if version == Version::V1 {
        json!({"sessionId":"A","toolCall":{"toolCallId":"t","title":"independent"},"options":[{"optionId":"allow","name":"Synthetic choice","kind":"future_kind"}]})
    } else {
        json!({"sessionId":"A","title":"independent","subject":{"type":"_synthetic_future","opaque":true},"options":[{"optionId":"allow","name":"Synthetic choice","kind":"future_kind"}]})
    };
    serde_json::to_vec(&json!({"jsonrpc":"2.0","id":rpc_value(id),"method":"session/request_permission","params":params})).unwrap()
}

pub fn lf_count(bytes: impl AsRef<[u8]>) -> usize {
    bytes.as_ref().split(|&b| b == b'\n').count() - 1
}
