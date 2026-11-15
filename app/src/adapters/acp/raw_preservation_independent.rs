//! Independent tester asset. Payload JSON is never converted through `serde_json::Value`.
#![allow(
    clippy::cast_possible_truncation,
    clippy::many_single_char_names,
    clippy::option_if_let_else,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
macro_rules! tassert {($($arg:tt)*) => {{ $crate::adapters::acp::raw_preservation_independent::tick(); assert!($($arg)*); }}}
macro_rules! tassert_eq {($($arg:tt)*) => {{ $crate::adapters::acp::raw_preservation_independent::tick(); assert_eq!($($arg)*); }}}

use super::raw_json::{RawDocument, RawJsonError, RawKind, RawValueRef};
use super::*;
use futures_util::FutureExt;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{panic::AssertUnwindSafe, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf},
    time::{Instant, timeout},
};

thread_local! { static ASSERTIONS: std::cell::Cell<u64> = const {std::cell::Cell::new(0)}; }
pub fn tick() {
    ASSERTIONS.with(|n| n.set(n.get() + 1));
}
fn assertion_count() -> u64 {
    ASSERTIONS.with(std::cell::Cell::get)
}
fn add_assertions(v: u64) {
    ASSERTIONS.with(|n| n.set(n.get() + v));
}
pub fn vectors(group: &str) -> Vec<Value> {
    let text = match group {
        "core" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/raw_preservation_independent/core.json"
        )),
        "projection" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/raw_preservation_independent/projection.json"
        )),
        "tool" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/raw_preservation_independent/tool.json"
        )),
        "observation" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/raw_preservation_independent/observation.json"
        )),
        "management" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/raw_preservation_independent/management.json"
        )),
        "profile" => include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/raw_preservation_independent/profile.json"
        )),
        _ => panic!("unknown fixed group"),
    };
    serde_json::from_str(text).expect("descriptor JSON only")
}
pub fn id(v: &Value) -> &str {
    v["id"].as_str().unwrap()
}
pub fn version(v: &Value) -> Version {
    if v["version"].as_u64() == Some(1) {
        Version::V1
    } else {
        Version::V2
    }
}
fn select_offer(v: &Value) -> Result<Version, &'static str> {
    match v.get("offer") {
        Some(offer) => match offer.as_u64() {
            Some(1) => Ok(Version::V1),
            Some(2) => Ok(Version::V2),
            _ => Err("select offer descriptor must be integer 1 or 2"),
        },
        None => Ok(version(v)),
    }
}
pub fn expected(v: &Value) -> &str {
    if let Some(s) = v["expected"].as_str() {
        s
    } else if let Some(s) = v["expected"]["result"].as_str() {
        s
    } else {
        v["expected"]["error"].as_str().unwrap_or("")
    }
}
pub fn input(v: &Value) -> Vec<u8> {
    if let Some(s) = v["inputUtf8"].as_str() {
        return s.as_bytes().to_vec();
    }
    if let Some(s) = v["inputHex"].as_str() {
        return hex::decode(s).unwrap();
    }
    let g = &v["generator"];
    let mut out = Vec::new();
    if g["repeatPrefix"].is_number() {
        for _ in 0..g["repeatPrefix"].as_u64().unwrap() {
            out.extend_from_slice(g["prefix"].as_str().unwrap().as_bytes());
        }
        out.extend_from_slice(g["leaf"].as_str().unwrap().as_bytes());
        for _ in 0..g["repeatSuffix"].as_u64().unwrap() {
            out.extend_from_slice(g["suffix"].as_str().unwrap().as_bytes());
        }
    } else {
        out.extend_from_slice(g["prefix"].as_str().unwrap().as_bytes());
        out.extend(std::iter::repeat_n(
            g["repeatByte"].as_u64().unwrap() as u8,
            g["count"].as_u64().unwrap() as usize,
        ));
        out.extend_from_slice(g["suffix"].as_str().unwrap().as_bytes());
        out.extend_from_slice(&hex::decode(g["appendHex"].as_str().unwrap()).unwrap());
    }
    out
}
pub fn mark(v: &Value, phase: &str) {
    if phase == "start" {
        ASSERTIONS.with(|n| n.set(0));
    }
    println!(
        "RAW_VECTOR {}",
        serde_json::json!({"id":id(v),"phase":phase,"assertions":assertion_count(),"inputBytes":input(v).len(),"actual":if phase=="pass"{"all typed assertions matched; raw comparisons completed"}else{"not completed"},"inputSha256":hex::encode(Sha256::digest(input(v))),"expected":v["expected"]})
    );
}
pub fn raw(v: &Value) -> RawDocument {
    RawDocument::parse(input(v)).unwrap()
}
pub fn field<'a>(v: RawValueRef<'a>, k: &str) -> RawValueRef<'a> {
    v.object().unwrap().get(k).unwrap().unwrap()
}
pub fn baseline_event(s: &Strategy, state: bool) -> SessionEvent {
    let b = if state {
        r#"{"sessionId":"s","update":{"sessionUpdate":"state_update","state":"idle"}}"#
    } else {
        r#"{"sessionId":"s","update":{"sessionUpdate":"tool_call_update","toolCallId":"t"}}"#
    };
    let d = RawDocument::parse(b.as_bytes().to_vec()).unwrap();
    s.update(d.root()).unwrap()
}
pub fn setup(w: Version, kind: &str) -> SessionSetup {
    let servers = match kind {
        "http" | "MCP-PARENT" => vec![
            McpServer::http(
                w,
                Arc::from("m"),
                Arc::from("https://fixture.invalid"),
                vec![],
                ExtensionFields::empty().unwrap(),
            )
            .unwrap(),
        ],
        "sse" => vec![
            McpServer::sse_v1(
                Arc::from("m"),
                Arc::from("https://fixture.invalid"),
                vec![],
                ExtensionFields::empty().unwrap(),
            )
            .unwrap(),
        ],
        "stdio" => vec![
            McpServer::stdio(
                w,
                Arc::from("m"),
                Arc::from("fixture"),
                vec![],
                vec![],
                ExtensionFields::empty().unwrap(),
            )
            .unwrap(),
        ],
        _ => vec![],
    };
    SessionSetup::try_from_snapshot(SetupSnapshot {
        wire: w,
        source: ConfigSourceStamp {
            reference: Arc::from("independent-fixture"),
            revision: 1,
        },
        cwd: Known::Known(Arc::from("/w")),
        mcp_servers: Known::Known(servers),
        additional_directories: Known::Known(if kind == "additionalDirectories" {
            vec![Arc::from("/extra")]
        } else {
            vec![]
        }),
        extra: ExtensionFields::empty().unwrap(),
    })
    .unwrap()
}
fn init_result(w: Version, caps: &str) -> String {
    let mut p = if w == Version::V1 {
        String::from(
            r#"{"protocolVersion":1,"agentInfo":{"name":"fixture","version":"1"},"agentCapabilities":{"loadSession":true,"sessionCapabilities":{"list":{},"resume":{},"close":{},"additionalDirectories":{}},"mcpCapabilities":{"http":true,"sse":true},"promptCapabilities":{"image":true,"audio":true,"embeddedContext":true}}}"#,
        )
    } else {
        String::from(
            r#"{"protocolVersion":2,"info":{"name":"fixture","version":"1"},"capabilities":{"session":{"prompt":{"image":{},"audio":{},"embeddedContext":{}},"mcp":{"stdio":{},"http":{}},"additionalDirectories":{}}}}"#,
        )
    };
    match caps {
        "no-image" => {
            p = p
                .replace(r#""image":true"#, r#""image":false"#)
                .replace(r#""image":{},"#, "");
        }
        "no-embedded" => {
            p = p
                .replace(r#""embeddedContext":true"#, r#""embeddedContext":false"#)
                .replace(r#""embeddedContext":{}"#, r#""embeddedContext":false"#);
        }
        "no-session" => {
            p = String::from(
                r#"{"protocolVersion":2,"info":{"name":"fixture","version":"1"},"capabilities":{}}"#,
            );
        }
        _ => {}
    }
    p
}
struct LocalFence;
impl WriteFence for LocalFence {
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
pub struct Fixture {
    pub connection: Connection,
    pub events: EventReceiver,
    owner: IoOwner,
    reader: BufReader<ReadHalf<DuplexStream>>,
    writer: WriteHalf<DuplexStream>,
}
impl Fixture {
    pub fn new() -> Self {
        let (a, b) = tokio::io::duplex(128 * 1024);
        let (ar, aw) = tokio::io::split(a);
        let (br, bw) = tokio::io::split(b);
        let (connection, events, owner) = Connection::spawn(ar, aw).unwrap();
        Self {
            connection,
            events,
            owner,
            reader: BufReader::new(br),
            writer: bw,
        }
    }
    pub fn strategy(&self, w: Version, caps: &str) -> Strategy {
        let s = format!(
            r#"{{"jsonrpc":"2.0","id":1,"result":{}}}"#,
            init_result(w, caps)
        );
        let raw = RawMessage::parse(s.into_bytes()).unwrap();
        let doc = RawDocument::parse(raw.bytes().to_vec()).unwrap();
        Strategy::select(w, self.connection.scope(), raw, field(doc.root(), "result")).unwrap()
    }
    async fn read_frame(&mut self) -> Vec<u8> {
        let mut b = Vec::new();
        let n = timeout(
            Duration::from_secs(10),
            self.reader.read_until(b'\n', &mut b),
        )
        .await
        .unwrap()
        .unwrap();
        tassert!(n > 0 && b.last() == Some(&b'\n'));
        b
    }
    async fn feed(&mut self, mut b: Vec<u8>) {
        b.push(b'\n');
        timeout(Duration::from_secs(10), self.writer.write_all(&b))
            .await
            .unwrap()
            .unwrap();
    }
    async fn send(&mut self, r: Reservation) -> RpcId {
        let id = r.id().unwrap().clone();
        let frame = r.encode().unwrap();
        let ticket = self
            .connection
            .enqueue(
                r,
                frame,
                Arc::new(LocalFence),
                Instant::now() + Duration::from_secs(10),
            )
            .unwrap();
        self.read_frame().await;
        tassert_eq!(ticket.wait().await, WriteReport::Written);
        id
    }
    async fn initialize(&mut self, w: Version) {
        let r = self
            .connection
            .reserve_initialize(w, Instant::now() + Duration::from_secs(10))
            .unwrap();
        let id = self.send(r).await;
        let mut ib = Vec::new();
        id.write_json(&mut ib).unwrap();
        let response = format!(
            r#"{{"jsonrpc":"2.0","id":{},"result":{}}}"#,
            std::str::from_utf8(&ib).unwrap(),
            init_result(w, "all")
        );
        self.feed(response.into_bytes()).await;
        let e = timeout(Duration::from_secs(10), self.events.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        tassert!(matches!(e.incoming(), Incoming::Response { .. }));
        tassert_eq!(self.connection.strategy().unwrap().version(), w);
    }
    pub async fn close(&self) {
        let r = self
            .owner
            .shutdown(Instant::now() + Duration::from_secs(5))
            .await;
        tassert!(r.joined, "owned fixture IO was not joined");
    }
}
pub async fn check_sync(v: &Value, run: impl FnOnce(&Fixture) + std::panic::UnwindSafe) {
    mark(v, "start");
    let f = Fixture::new();
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| run(&f)));
    f.close().await;
    if let Err(e) = result {
        std::panic::resume_unwind(e)
    }
    mark(v, "pass");
}
fn acp<T>(r: Result<T, AcpError>, e: &str) -> Option<T> {
    let error = if e.contains("UnencodableConfiguration") {
        Some(AcpError::UnencodableConfiguration)
    } else if e.contains("IncompatibleVersion") {
        Some(AcpError::IncompatibleVersion)
    } else if e.contains("UnsupportedCapability") {
        Some(AcpError::UnsupportedCapability)
    } else if e.contains("InvalidMessage") {
        Some(AcpError::InvalidMessage)
    } else {
        None
    };
    if let Some(want) = error {
        tassert!(matches!(r,Err(got)if got==want), "typed ACP error mismatch");
        None
    } else {
        Some(r.unwrap())
    }
}
fn assert_selected_capabilities(w: Version, e: &str, row: &str, c: &Capabilities) {
    if e.contains("V1 sessions=true") {
        tassert_eq!(c.sessions, w == Version::V1);
    } else if e.contains("sessions=true") {
        tassert!(c.sessions);
    }
    if e.contains("sessions=false") {
        tassert!(!c.sessions);
    }
    if e.contains("default false")
        || e.contains("image/audio/embedded/list/resume/close/standard_replay=false")
    {
        tassert!(
            !c.image
                && !c.audio
                && !c.embedded_context
                && !c.list
                && !c.resume
                && !c.close
                && !c.standard_replay
        );
    }
    if e.contains("list/resume/close=false") || e.contains("lifecycle=false") {
        tassert!(!c.list && !c.resume && !c.close);
    }
    if e.contains("list/resume/close/standard_replay=true") {
        tassert!(c.list && c.resume && c.close && c.standard_replay);
    }
    if e.contains("media=false") {
        tassert!(!c.image && !c.audio && !c.embedded_context);
    }
    for (n, b) in [
        ("list", c.list),
        ("resume", c.resume),
        ("close", c.close),
        ("standard_replay", c.standard_replay),
        ("image", c.image),
        ("audio", c.audio),
        ("embeddedContext", c.embedded_context),
    ] {
        if e.contains(&format!("{n}=true")) {
            tassert!(b);
        }
        if e.contains(&format!("{n}=false")) {
            tassert!(!b);
        }
        if e == "Ok(capability=false)" && row.ends_with(n) {
            tassert!(!b);
        }
    }
}
fn path_case(v: &Value, f: &Fixture) {
    let d = raw(v);
    let w = version(v);
    let e = expected(v);
    let op = v["operation"].as_str().unwrap();
    let s = f.strategy(w, "all");
    match op {
        "select" => {
            let m = RawMessage::parse(input(v)).unwrap();
            if let Some(got) = acp(
                Strategy::select(
                    select_offer(v).expect("valid select offer descriptor"),
                    f.connection.scope(),
                    m,
                    field(d.root(), "result"),
                ),
                e,
            ) {
                tassert_eq!(got.version(), w);
                let c = &got.negotiated().capabilities;
                assert_selected_capabilities(w, e, v["row"].as_str().unwrap(), c);
            }
        }
        "session_listing" => {
            if let Some(x) = acp(s.session_listing(d.root()), e) {
                tassert_eq!(x.raw().fragment(), d.root().fragment());
                if e.contains("empty") {
                    tassert!(x.sessions().is_empty());
                }
                if e.contains("next_cursor=Some") {
                    tassert_eq!(x.next_cursor(), Some("next"));
                }
                if e == "Ok(None)" {
                    tassert_eq!(x.next_cursor(), None);
                }
                if e.contains("Directories")
                    || ["Ok(Omitted)", "Ok(Null)", "Ok(Uninterpreted raw)"].contains(&e)
                {
                    let dirs = x.sessions()[0].additional_directories();
                    match e {
                        "Ok(Omitted)" => tassert!(matches!(dirs, ReportedDirectories::Omitted)),
                        "Ok(Null)" => tassert!(matches!(dirs, ReportedDirectories::Null)),
                        "Ok(Uninterpreted raw)" => {
                            tassert!(matches!(dirs, ReportedDirectories::Uninterpreted(_)));
                        }
                        _ => tassert_eq!(
                            dirs.known()
                                .unwrap()
                                .iter()
                                .map(std::convert::AsRef::as_ref)
                                .collect::<Vec<_>>(),
                            ["/a"]
                        ),
                    }
                }
            }
        }
        "prompt_result" => {
            if let Some(x) = acp(s.prompt_result(d.root()), e) {
                match x {
                    PromptResult::Completed { stop_reason } => {
                        tassert_eq!(w, Version::V1);
                        tassert_eq!(&*stop_reason, "end_turn");
                    }
                    PromptResult::Inserted { message } => {
                        tassert_eq!(w, Version::V2);
                        tassert_eq!(message.as_str(), "m");
                    }
                }
            }
        }
        "update" => {
            if let Some(x) = acp(s.update(d.root()), e) {
                if e.contains("State{") {
                    tassert!(
                        matches!(x,SessionEvent::State{state,stop_reason,..}if &*state=="idle" && if e.contains("cancelled"){stop_reason.as_deref()==Some("cancelled")}else{stop_reason.is_none()})
                    );
                } else if e == "Ok(None)" {
                    match x {
                        SessionEvent::Message { message, .. } => tassert!(message.is_none()),
                        SessionEvent::State { stop_reason, .. } => tassert!(stop_reason.is_none()),
                        _ => panic!("unexpected optional event kind"),
                    }
                }
            }
        }
        "tool_name_patch" => {
            if let Some(x) = acp(s.tool_name_patch(d.root()), e) {
                match e {
                    "Ok(Unchanged)" => tassert!(matches!(x, ToolNamePatch::Unchanged)),
                    "Ok(Clear)" => tassert!(matches!(x, ToolNamePatch::Clear)),
                    _ => tassert!(matches!(x,ToolNamePatch::Set(n)if &*n=="new")),
                }
            }
        }
        "prompt_document" => {
            if let Some(x) = acp(PromptDocument::parse(input(v)), e) {
                tassert_eq!(x.uploaded_bytes(), input(v));
                tassert_eq!(x.prompt_fragment(), d.root().fragment());
            }
        }
        "permission" => {
            let wire = [
                br#"{"jsonrpc":"2.0","id":7,"method":"session/request_permission","params":"#
                    .as_slice(),
                d.root().fragment().as_bytes(),
                b"}",
            ]
            .concat();
            let m = RawMessage::parse(wire.clone()).unwrap();
            if let Some(x) = acp(s.permission(RpcId::Integer(7), m, d.root()), e) {
                tassert_eq!(x.raw().bytes(), wire);
                if e.contains("supported_subject=false") {
                    tassert!(!x.supported_subject());
                    tassert!(x.tool().is_none());
                }
                if e.contains("supported=true") {
                    tassert!(x.supported_subject());
                    tassert!(x.tool().is_none());
                }
                if e == "Ok(None)" {
                    tassert!(x.tool().is_none());
                }
            }
        }
        "mcp_from_raw" => {
            let r = McpServer::from_raw(w, d.root_owned());
            if e.contains("SetupError::InvalidShape") {
                tassert!(matches!(r, Err(SetupError::InvalidShape)));
            } else if e.contains("SetupError::UnsupportedEncoding") {
                tassert!(matches!(r, Err(SetupError::UnsupportedEncoding)));
            } else {
                let x = r.unwrap();
                tassert_eq!(x.as_raw().fragment(), d.root().fragment());
                if e.contains("stdio default") {
                    tassert!(matches!(x.transport(), McpTransport::Stdio));
                }
            }
        }
        "extension_from_raw" => {
            let r = ExtensionFields::from_raw(d.root_owned());
            if e.contains("InvalidShape") {
                tassert!(matches!(r, Err(SetupError::InvalidShape)));
            } else {
                tassert_eq!(r.unwrap().as_raw().fragment(), d.root().fragment());
            }
        }
        "new_session_with_setup" => {
            let m = RawMessage::parse(input(v)).unwrap();
            let r = Strategy::select(w, f.connection.scope(), m, field(d.root(), "result"))
                .and_then(|selected| {
                    let row = v["row"].as_str().unwrap();
                    let k = if row.ends_with("MCP-PARENT") {
                        "MCP-PARENT"
                    } else {
                        row.rsplit('-').next().unwrap()
                    };
                    selected.new_session(&setup(w, k))
                });
            if let Some(x) = acp(r, e) {
                tassert_eq!(x.setup_stamp().unwrap().revision, 1);
                tassert!(matches!(x.kind(), RequestKind::NewSession));
            }
        }
        _ => panic!("unhandled path operation"),
    }
    tassert_eq!(d.bytes(), input(v), "original bytes after semantic outcome");
}

fn compact_oracle(s: &str) -> String {
    let mut out = String::new();
    let (mut quoted, mut escaped) = (false, false);
    for c in s.chars() {
        if quoted {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                quoted = false;
            }
        } else if c == '"' {
            quoted = true;
            out.push(c);
        } else if !matches!(c, ' ' | '\r' | '\n' | '\t') {
            out.push(c);
        }
    }
    out
}
fn raw_case(v: &Value) {
    let b = input(v);
    let e = &v["expected"];
    if let Some(want) = e["error"].as_str() {
        let got = RawDocument::parse(b).unwrap_err();
        tassert_eq!(
            got,
            match want {
                "InvalidJson" => RawJsonError::InvalidJson,
                "InvalidUnicode" => RawJsonError::InvalidUnicode,
                _ => panic!("unknown raw error"),
            }
        );
        return;
    }
    let d = RawDocument::parse(b.clone()).unwrap();
    tassert_eq!(d.bytes(), b);
    match id(v) {
        "RAW-OPAQUE-001" => {
            let root = d.root().object().unwrap();
            tassert_eq!(d.root().fragment(), e["rootFragment"].as_str().unwrap());
            tassert_eq!(
                root.iter().map(|(k, _)| k.to_owned()).collect::<Vec<_>>(),
                e["memberKeys"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_str().unwrap().to_owned())
                    .collect::<Vec<_>>()
            );
            let f = field(d.root(), "future");
            let o = f.object().unwrap();
            let pairs = o
                .iter()
                .map(|(k, v)| (k.to_owned(), v.fragment().as_bytes().to_vec()))
                .collect::<Vec<_>>();
            tassert_eq!(
                pairs.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
                e["futureKeys"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_str().unwrap())
                    .collect::<Vec<_>>()
            );
            tassert_eq!(
                o.iter()
                    .rev()
                    .map(|(k, v)| (k.to_owned(), v.fragment().as_bytes().to_vec()))
                    .collect::<Vec<_>>(),
                pairs.iter().rev().cloned().collect::<Vec<_>>()
            );
            tassert_eq!(
                pairs[..3]
                    .iter()
                    .map(|(_, b)| b.as_slice())
                    .collect::<Vec<_>>(),
                [b"1".as_slice(), b"2", b"3"]
            );
            tassert_eq!(
                o.iter()
                    .filter(|(_, v)| v.kind() == RawKind::Number)
                    .skip(3)
                    .take(5)
                    .map(|(_, v)| v.fragment())
                    .collect::<Vec<_>>(),
                e["rawNumbers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_str().unwrap())
                    .collect::<Vec<_>>()
            );
            let owned = f.to_owned();
            let copy = d.clone();
            let root_owned = d.root_owned();
            let fragment = owned.as_ref().fragment().to_owned();
            tassert_eq!(
                root_owned.as_ref().compact().unwrap(),
                compact_oracle(d.root().fragment())
            );
            drop(d);
            drop(copy);
            tassert_eq!(owned.as_ref().fragment(), fragment);
            tassert_eq!(
                root_owned.as_ref().compact().unwrap(),
                compact_oracle(root_owned.as_ref().fragment())
            );
            return;
        }
        "RAW-OPAQUE-002" | "RAW-OPAQUE-ASTRAL-KEY" => {
            let key = if id(v).ends_with("002") { "a" } else { "🚀" };
            let o = d.root().object().unwrap();
            tassert_eq!(o.len(), 4);
            tassert!(matches!(o.get(key), Err(RawJsonError::DuplicateField)));
            tassert!(o.get("absent").unwrap().is_none());
            tassert_eq!(o.get("é").unwrap().unwrap().integer(), Some(3));
            tassert_eq!(o.get("e\u{301}").unwrap().unwrap().integer(), Some(4));
            let expected_keys = [key, key, "é", "e\u{301}"];
            tassert_eq!(o.iter().map(|(k, _)| k).collect::<Vec<_>>(), expected_keys);
            let ordered = OrderedRawObject::from_raw(d.root_owned()).unwrap();
            tassert!(matches!(ordered.get(key), Err(AcpError::InvalidMessage)));
            tassert!(ordered.get("absent").unwrap().is_none());
            tassert_eq!(ordered.get("é").unwrap().unwrap().integer(), Some(3));
            tassert_eq!(ordered.get("e\u{301}").unwrap().unwrap().integer(), Some(4));
            tassert_eq!(
                ordered.iter().map(|(k, _)| k).collect::<Vec<_>>(),
                expected_keys
            );
        }
        "RAW-DEEP-20000" => {
            let owned = d.root_owned();
            let clone = d.clone();
            drop(d);
            drop(clone);
            let mut leaf = owned.as_ref();
            for _ in 0..20000 {
                let a = leaf.array().unwrap();
                tassert_eq!(a.len(), 1);
                leaf = a.get(0).unwrap();
            }
            let o = leaf.object().unwrap();
            tassert_eq!(
                o.iter().map(|(k, v)| (k, v.fragment())).collect::<Vec<_>>(),
                [("a", "1"), ("a", "2")]
            );
            tassert!(matches!(o.get("a"), Err(RawJsonError::DuplicateField)));
            drop(owned);
            return;
        }
        x if x.starts_with("RAW-INT-") => {
            let n = d.root();
            tassert_eq!(n.fragment(), e["numberFragment"].as_str().unwrap());
            let signed = if e["integer"].is_null() {
                None
            } else if let Some(s) = e["integer"].as_str() {
                Some(s.parse::<i64>().unwrap())
            } else {
                e["integer"].as_i64()
            };
            let unsigned = if e["unsigned"].is_null() {
                None
            } else if let Some(s) = e["unsigned"].as_str() {
                Some(s.parse::<u64>().unwrap())
            } else {
                e["unsigned"].as_u64()
            };
            tassert_eq!(n.integer(), signed);
            tassert_eq!(n.unsigned(), unsigned);
            tassert_eq!(n.string().unwrap_err(), RawJsonError::WrongType);
        }
        _ => panic!("unhandled raw success vector"),
    }
    tassert_eq!(d.bytes(), b);
}
fn pairs(m: &RawMetadata) -> String {
    if m.is_null() {
        return "Null".into();
    }
    let o = m.as_object().unwrap();
    if o.is_empty() {
        return "Object(empty)".into();
    }
    format!(
        "[{}]",
        o.iter()
            .map(|(k, v)| format!("({k},{})", v.fragment()))
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn meta_case(v: &Value, f: &Fixture) {
    let current_bytes = v["currentUtf8"].as_str().unwrap().as_bytes().to_vec();
    let cd = RawDocument::parse(current_bytes.clone()).unwrap();
    let current = RawMetadata::from_raw(cd.root_owned()).unwrap();
    let before = pairs(&current);
    let d = raw(v);
    let s = f.strategy(version(v), "all");
    let e = expected(v);
    let r = s.tool_metadata_patch(&current, d.root());
    if let Some(got) = acp(r, e) {
        if e == "same current clone" {
            tassert_eq!(pairs(&got), before);
        } else {
            tassert_eq!(pairs(&got), e);
        }
    }
    tassert_eq!(pairs(&current), before);
    tassert_eq!(cd.bytes(), current_bytes);
    tassert_eq!(d.bytes(), input(v));
}
fn prompt_case(v: &Value, f: &Fixture) {
    let b = input(v);
    let e = expected(v);
    let r = PromptDocument::parse(b.clone());
    if id(v) == "RAW-PROMPT-BYTES" {
        use crate::application::{
            queue_owner::RetainPrompt,
            request_registry::{ValidatedBody, ValidatedBodyPort},
        };
        let p = r.unwrap();
        let d = raw(v);
        tassert_eq!(p.uploaded_bytes(), b);
        tassert_eq!(p.prompt_fragment(), d.root().fragment());
        let adapter = Arc::<native_dispatch::AcpNativeAdapter>::default();
        let (body, digest) = adapter.validate(&p).unwrap();
        let own_digest: [u8; 32] = Sha256::digest(&b).into();
        tassert_eq!(digest, own_digest);
        let validated = ValidatedBody::new(body, digest);
        let retained = adapter.retain(&validated);
        drop(p);
        drop(validated);
        tassert_eq!(retained.as_ref().fragment(), d.root().fragment());
        for w in [Version::V1, Version::V2] {
            let s = f.strategy(w, "all");
            let result = s.prompt(
                SessionId::parse(RawDocument::parse(br#""s""#.to_vec()).unwrap().root()).unwrap(),
                retained.clone(),
                ExtensionFields::empty().unwrap(),
            );
            if w == Version::V1 {
                tassert!(matches!(result, Err(AcpError::UnsupportedCapability)));
            } else {
                let req = result.unwrap();
                tassert_eq!(
                    field(req.params(), "prompt").fragment(),
                    compact_oracle(d.root().fragment())
                );
            }
        }
    } else if e.contains("InvalidMessage") || e.contains("error") {
        tassert!(matches!(r, Err(AcpError::InvalidMessage)));
    } else {
        let p = r.unwrap();
        tassert_eq!(p.uploaded_bytes(), b);
        tassert_eq!(p.prompt_fragment(), raw(v).root().fragment());
    }
}
fn redaction_case(v: &Value) {
    let d = raw(v);
    let root = d.root();
    let object = root.object().unwrap();
    tassert!(matches!(
        object.get("SECRET_KEY"),
        Err(RawJsonError::DuplicateField)
    ));
    let array = object.iter().next().unwrap().1.array().unwrap();
    let owned = d.root_owned();
    let texts = vec![
        format!("{d:?}"),
        format!("{root:?}"),
        format!("{object:?}"),
        format!("{array:?}"),
        format!("{owned:?}"),
    ];
    for t in texts {
        for canary in ["SECRET_KEY", "SECRET_VALUE", "123456789"] {
            tassert!(!t.contains(canary));
        }
    }
    for (e, m) in [
        (RawJsonError::InvalidJson, "JSON is invalid"),
        (
            RawJsonError::DuplicateField,
            "JSON contains a duplicate object field",
        ),
        (
            RawJsonError::InvalidUnicode,
            "JSON contains invalid Unicode",
        ),
        (RawJsonError::WrongType, "JSON value has a different type"),
        (RawJsonError::Allocation, "JSON allocation failed"),
    ] {
        tassert_eq!(e.to_string(), m);
        tassert_eq!(
            AcpError::from(e),
            if e == RawJsonError::Allocation {
                AcpError::Allocation
            } else {
                AcpError::InvalidMessage
            }
        );
        for t in [format!("{e:?}"), e.to_string()] {
            for c in ["SECRET_KEY", "SECRET_VALUE", "123456789"] {
                tassert!(!t.contains(c));
            }
        }
    }
}
async fn wire_case(v: &Value) {
    mark(v, "start");
    let mut f = Fixture::new();
    let outcome = AssertUnwindSafe(async {
        let w = version(v);
        f.initialize(w).await;
        let mut b = input(v);
        let d = RawDocument::parse(b.clone()).unwrap();
        tassert_eq!(d.bytes(), b);
        if id(v).starts_with("RAW-WIRE-") {
            let req = f
                .connection
                .strategy()
                .unwrap()
                .new_session(&setup(w, ""))
                .unwrap();
            let r = f
                .connection
                .reserve_request(
                    req,
                    ResponseDeadline::At(Instant::now() + Duration::from_secs(10)),
                )
                .unwrap();
            let assigned = f.send(r).await;
            let mut ib = Vec::new();
            assigned.write_json(&mut ib).unwrap();
            let s = std::str::from_utf8(&b).unwrap();
            tassert_eq!(s.matches("\"id\":9").count(), 1);
            b = s
                .replacen(
                    "\"id\":9",
                    &format!("\"id\":{}", std::str::from_utf8(&ib).unwrap()),
                    1,
                )
                .into_bytes();
        }
        let injected = RawDocument::parse(b.clone()).unwrap();
        tassert_eq!(injected.bytes(), b);
        println!("RAW_WIRE {}", serde_json::json!({"id":id(v),"descriptorSha256":hex::encode(Sha256::digest(input(v))),"deliveredSha256":hex::encode(Sha256::digest(&b)),"deliveredBytes":b.len(),"ownedRequestIdBound":id(v).starts_with("RAW-WIRE-")}));
        f.feed(b.clone()).await;
        let got = timeout(Duration::from_secs(10), f.events.next())
            .await
            .unwrap();
        let want = if id(v) == "RAW-ORDER-RESPONSE-UNMATCHED" {
            AcpError::UnmatchedResponse
        } else {
            AcpError::InvalidMessage
        };
        tassert!(
            matches!(got,Err(e)if e==want),
            "wire rejected with expected first error"
        );
        if let Some(rejected) = f.connection.take_rejected_message() {
            tassert_eq!(rejected.bytes(), b);
        }
        tassert_eq!(injected.bytes(), b);
        tassert_eq!(d.bytes(), input(v));
        tassert!(f.connection.is_closed());
        let mut extra = Vec::new();
        let _ = timeout(
            Duration::from_millis(100),
            f.reader.read_until(b'\n', &mut extra),
        )
        .await;
        tassert!(extra.is_empty(), "invalid frame emitted no outgoing action");
    })
    .catch_unwind()
    .await;
    f.close().await;
    if let Err(e) = outcome {
        std::panic::resume_unwind(e)
    }
    mark(v, "pass");
}
fn small_stack_child(v: &Value) {
    use std::process::{Command, Stdio};
    let exact =
        "adapters::acp::raw_preservation_independent::raw_preservation_independent_deep_child";
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", exact, "--nocapture", "--test-threads=1"])
        .env("TESTER_RAW_CHILD_ID", id(v))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    println!(
        "RAW_CHILD {}",
        serde_json::json!({"id":id(v),"pid":child.id(),"phase":"start"})
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(s) = child.try_wait().unwrap() {
            let output = child.wait_with_output().unwrap();
            let text = String::from_utf8(output.stdout).unwrap();
            for line in text.lines() {
                if let Some(n) = line.split("RAW_CHILD_ASSERTIONS ").nth(1) {
                    add_assertions(n.trim().parse().unwrap());
                }
            }
            tassert!(s.success(), "owned deep child failed");
            println!(
                "RAW_CHILD {}",
                serde_json::json!({"id":id(v),"phase":"joined","status":s.code()})
            );
            break;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("owned deep child exceeded 20s")
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[test]
fn raw_preservation_independent_deep_child() {
    let Ok(want) = std::env::var("TESTER_RAW_CHILD_ID") else {
        return;
    };
    tassert!(matches!(
        want.as_str(),
        "RAW-DEEP-20000" | "RAW-DEEP-LATE-UNICODE"
    ));
    let v = vectors("core").into_iter().find(|v| id(v) == want).unwrap();
    let count = std::thread::Builder::new()
        .name("tester-small-stack".into())
        .stack_size(256 * 1024)
        .spawn(move || {
            raw_case(&v);
            assertion_count()
        })
        .unwrap()
        .join()
        .unwrap();
    println!("RAW_CHILD_ASSERTIONS {count}");
}
#[tokio::test(flavor = "current_thread")]
async fn raw_preservation_independent_core() {
    let vv = vectors("core");
    tassert_eq!(vv.len(), 1238);
    for v in vv {
        if v["operation"] == "wire" {
            wire_case(&v).await;
            continue;
        }
        if id(&v).starts_with("RAW-DEEP-") {
            mark(&v, "start");
            small_stack_child(&v);
            mark(&v, "pass");
            continue;
        }
        check_sync(&v, |f| match v["operation"].as_str().unwrap() {
            "raw_document" => raw_case(&v),
            "metadata_patch" => meta_case(&v, f),
            "errors_and_debug" => redaction_case(&v),
            "strategy_prompt" => {
                let d = raw(&v);
                let cap = v["capabilities"].as_str().unwrap();
                let s = f.strategy(version(&v), cap);
                let extra = if cap == "all-plus-reserved-extra" {
                    ExtensionFields::from_raw(
                        RawDocument::parse(br#"{"prompt":0}"#.to_vec())
                            .unwrap()
                            .root_owned(),
                    )
                    .unwrap()
                } else {
                    ExtensionFields::empty().unwrap()
                };
                let sid = RawDocument::parse(br#""s""#.to_vec()).unwrap();
                acp(
                    s.prompt(SessionId::parse(sid.root()).unwrap(), d.root_owned(), extra),
                    expected(&v),
                );
                tassert_eq!(d.bytes(), input(&v));
            }
            "prompt_document" if !id(&v).starts_with("RAW-PATH-") => prompt_case(&v, f),
            "extension_from_raw" if id(&v) == "RAW-EXTENSION-OPAQUE-DUP" => {
                let d = raw(&v);
                let ext = ExtensionFields::from_raw(d.root_owned()).unwrap();
                tassert_eq!(ext.as_raw().fragment(), d.root().fragment());
                tassert_eq!(
                    ext.as_raw()
                        .object()
                        .unwrap()
                        .iter()
                        .map(|(k, _)| k)
                        .collect::<Vec<_>>(),
                    ["future", "same", "same"]
                );
                tassert_eq!(d.bytes(), input(&v));
            }
            "session_listing" if id(&v).starts_with("RAW-BOUNDARY-") => {
                let d = raw(&v);
                let x = f
                    .strategy(version(&v), "all")
                    .session_listing(d.root())
                    .unwrap();
                tassert!(x.sessions().is_empty());
                tassert_eq!(d.bytes(), input(&v));
            }
            "permission" if id(&v).starts_with("RAW-BOUNDARY-") => {
                let mut p = v.clone();
                p["expected"] = Value::String(
                    if version(&v) == Version::V1 {
                        "Ok"
                    } else {
                        "InvalidMessage"
                    }
                    .into(),
                );
                path_case(&p, f);
            }
            _ => path_case(&v, f),
        })
        .await;
    }
}

// Oracle-only injected results. These do not invoke or validate product behavior.
#[test]
fn raw_preservation_oracle_version_self_check() {
    let vv = vectors("core");
    let lookup = |wanted: &str| vv.iter().find(|v| id(v) == wanted).unwrap();
    let check = |v: &Value, c: &Capabilities| {
        assert_selected_capabilities(version(v), expected(v), v["row"].as_str().unwrap(), c);
    };
    let defaults = |w| Capabilities {
        sessions: w == Version::V1,
        ..Capabilities::default()
    };
    let session = Capabilities {
        sessions: true,
        list: true,
        resume: true,
        close: true,
        standard_replay: true,
        ..Capabilities::default()
    };
    for wanted in [
        "RAW-PATH-V1-INIT-CAPS-MISSING",
        "RAW-PATH-V2-INIT-CAPS-MISSING",
        "RAW-PATH-V2-INIT-CAPS-VALID",
        "RAW-PATH-V2-INIT-SESSION-MISSING",
        "RAW-PATH-V2-INIT-SESSION-NULL",
        "RAW-PATH-V2-INIT-SESSION-VALID",
    ] {
        let v = lookup(wanted);
        let c = if wanted == "RAW-PATH-V2-INIT-SESSION-VALID" {
            session
        } else {
            defaults(version(v))
        };
        check(v, &c);
        println!("ORACLE_SELF_CHECK positive {wanted}");
    }
    let mut rejected = 0;
    for wanted in [
        "RAW-PATH-V2-INIT-CAPS-MISSING",
        "RAW-PATH-V1-INIT-CAPS-MISSING",
        "RAW-PATH-V2-INIT-SESSION-VALID",
    ] {
        let v = lookup(wanted);
        let mut c = if wanted == "RAW-PATH-V2-INIT-SESSION-VALID" {
            session
        } else {
            defaults(version(v))
        };
        c.sessions = !c.sessions;
        assert!(
            std::panic::catch_unwind(AssertUnwindSafe(|| check(v, &c))).is_err(),
            "oracle accepted wrong sessions for {wanted}"
        );
        rejected += 1;
        println!("ORACLE_SELF_CHECK rejected {wanted} sessions");
    }
    for wanted in [
        "RAW-PATH-V1-INIT-CAPS-MISSING",
        "RAW-PATH-V2-INIT-CAPS-MISSING",
    ] {
        let v = lookup(wanted);
        for field in [
            "image",
            "audio",
            "embedded_context",
            "list",
            "resume",
            "close",
            "standard_replay",
        ] {
            let mut c = defaults(version(v));
            match field {
                "image" => c.image = true,
                "audio" => c.audio = true,
                "embedded_context" => c.embedded_context = true,
                "list" => c.list = true,
                "resume" => c.resume = true,
                "close" => c.close = true,
                "standard_replay" => c.standard_replay = true,
                _ => unreachable!(),
            }
            assert!(
                std::panic::catch_unwind(AssertUnwindSafe(|| check(v, &c))).is_err(),
                "oracle accepted wrong default {field} for {wanted}"
            );
            rejected += 1;
            println!("ORACLE_SELF_CHECK rejected {wanted} {field}");
        }
    }
    assert_eq!(rejected, 17);
    println!("ORACLE_SELF_CHECK complete positive=6 rejected=17");
}

#[tokio::test(flavor = "current_thread")]
async fn raw_preservation_oracle_offer_self_check() {
    for (descriptor, want) in [
        (serde_json::json!({"offer": 1}), Version::V1),
        (serde_json::json!({"offer": 2}), Version::V2),
        (serde_json::json!({"version": 1}), Version::V1),
        (serde_json::json!({"version": 2}), Version::V2),
        (serde_json::json!({"offer": 2, "version": 1}), Version::V2),
        (serde_json::json!({"offer": 1, "version": 2}), Version::V1),
    ] {
        tassert_eq!(select_offer(&descriptor), Ok(want));
        println!("OFFER_SELF_CHECK parameter accepted");
    }
    for bad in [
        serde_json::json!(null),
        serde_json::json!(0),
        serde_json::json!(3),
        serde_json::json!(true),
        serde_json::json!("1"),
        serde_json::json!(1.0),
        serde_json::json!({}),
        serde_json::json!([]),
    ] {
        tassert!(select_offer(&serde_json::json!({"offer": bad, "version": 2})).is_err());
        println!("OFFER_SELF_CHECK parameter rejected");
    }
    let vv = vectors("core");
    let lookup = |wanted: &str| vv.iter().find(|v| id(v) == wanted).unwrap();
    let f = Fixture::new();
    let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let original = lookup("RAW-ORDER-SELECT-NO-V2-FROM-V1");
        path_case(original, &f);
        println!("OFFER_SELF_CHECK product offer1-selected2 incompatible");
        let mut higher = original.clone();
        higher["offer"] = serde_json::json!(2);
        higher["expected"] = serde_json::json!("InvalidMessage");
        path_case(&higher, &f);
        println!("OFFER_SELF_CHECK product offer2-selected2 invalid-info");
        path_case(lookup("RAW-ORDER-SELECT-INCOMPATIBLE"), &f);
        println!("OFFER_SELF_CHECK product offer2-selected3 incompatible");
        path_case(lookup("RAW-ORDER-SELECT-PROTOCOL-DUP"), &f);
        println!("OFFER_SELF_CHECK product duplicate-protocol invalid-message");
        let fallback = serde_json::json!({"version": 1, "offer": 2, "operation": "select",
            "row": "oracle-offer-fallback", "inputUtf8": "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{}}}",
            "expected": "Ok(sessions=true; image/audio/embedded/list/resume/close/standard_replay=false)"});
        path_case(&fallback, &f);
        println!("OFFER_SELF_CHECK product offer2-selected1 accepted");
    }));
    f.close().await;
    if let Err(e) = outcome {
        std::panic::resume_unwind(e);
    }
    println!("OFFER_SELF_CHECK complete accepted=6 rejected=8 product=5");
}
