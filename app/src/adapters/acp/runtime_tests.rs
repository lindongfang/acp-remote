//! Actual public IO and lifecycle regressions; not real Harness evidence.
use super::test_support::*;
use super::*;
use std::{
    io,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    time::{Instant, timeout},
};
struct ErrWriter;
impl AsyncWrite for ErrWriter {
    fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, _: &[u8]) -> Poll<io::Result<usize>> {
        Poll::Ready(Err(io::Error::other("fixture")))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
struct PendingWriter {
    entered: Arc<tokio::sync::Notify>,
}
impl AsyncWrite for PendingWriter {
    fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, _: &[u8]) -> Poll<io::Result<usize>> {
        self.entered.notify_one();
        Poll::Pending
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_deadline_finishes_actual_pending_io_before_response_deadline() {
    let (transport, _peer) = tokio::io::duplex(1);
    let (reader, _unused_writer) = tokio::io::split(transport);
    let entered = Arc::new(tokio::sync::Notify::new());
    let writer = PendingWriter {
        entered: Arc::clone(&entered),
    };
    let (connection, _events, owner) = Connection::spawn(reader, writer).unwrap();
    let reservation = connection
        .reserve_preferred_initialize(Instant::now() + Duration::from_secs(30))
        .unwrap();
    let frame = reservation.encode().unwrap();
    let ticket = connection
        .enqueue(
            reservation,
            frame,
            Arc::new(Allow),
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap();
    timeout(Duration::from_secs(2), entered.notified())
        .await
        .expect("actual writer was polled");
    assert!(matches!(
        ticket.progress(),
        WriteProgress::Writing { accepted: 0, .. }
            | WriteProgress::Finished(WriteReport::Zero {
                reason: ZeroReason::Deadline
            })
    ));
    assert_eq!(
        timeout(Duration::from_secs(3), ticket.wait())
            .await
            .expect("write deadline must finish before the response deadline"),
        WriteReport::Zero {
            reason: ZeroReason::Deadline
        }
    );
    assert!(!connection.is_closed());
    assert_eq!(connection.pending_count(), 0);
    assert!(
        owner
            .shutdown(Instant::now() + Duration::from_secs(2))
            .await
            .joined
    );
}
struct DropReceiver {
    receiver: Mutex<Option<EventReceiver>>,
    flush: bool,
}
impl WriteFence for DropReceiver {
    fn check_and_poll(
        &self,
        _: &WriteIdentity,
        step: WriteStep,
        cx: &mut Context<'_>,
        io: &mut dyn PollIo,
    ) -> Result<(), FenceRejection> {
        let out = io.poll(cx);
        if (self.flush && matches!(step, WriteStep::Flush { .. }))
            || (!self.flush && matches!(out, Poll::Ready(Err(_))))
        {
            drop(self.receiver.lock().unwrap().take());
        }
        Ok(())
    }
}
fn future_send<T: Send>(_: T) {}
#[tokio::test]
async fn actual_err_zero_then_synchronous_stop_preserves_unknown() {
    let (reader, _peer) = tokio::io::duplex(8192);
    let (c, receiver, owner) = Connection::spawn(reader, ErrWriter).unwrap();
    let r = c
        .reserve_preferred_initialize(Instant::now() + Duration::from_secs(5))
        .unwrap();
    let f = r.encode().unwrap();
    let t = c
        .enqueue(
            r,
            f,
            Arc::new(DropReceiver {
                receiver: Mutex::new(Some(receiver)),
                flush: false,
            }),
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
    assert_eq!(
        timeout(Duration::from_secs(5), t.wait()).await.unwrap(),
        WriteReport::Unknown {
            accepted: 0,
            cause: WriteFailure::Io
        }
    );
    let report = owner
        .shutdown(Instant::now() + Duration::from_secs(5))
        .await;
    assert!(report.joined);
    assert_eq!(report.error, Some(AcpError::Io));
}
#[tokio::test]
async fn actual_flush_then_synchronous_stop_preserves_written() {
    let (reader, _peer) = tokio::io::duplex(8192);
    let (c, receiver, owner) = Connection::spawn(reader, tokio::io::sink()).unwrap();
    let r = c
        .reserve_preferred_initialize(Instant::now() + Duration::from_secs(5))
        .unwrap();
    let f = r.encode().unwrap();
    let t = c
        .enqueue(
            r,
            f,
            Arc::new(DropReceiver {
                receiver: Mutex::new(Some(receiver)),
                flush: true,
            }),
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
    assert_eq!(
        timeout(Duration::from_secs(5), t.wait()).await.unwrap(),
        WriteReport::Written
    );
    assert!(c.is_closed());
    assert!(
        owner
            .shutdown(Instant::now() + Duration::from_secs(5))
            .await
            .joined
    );
}
async fn ready() -> (Connection, EventReceiver, IoOwner, tokio::io::DuplexStream) {
    let (transport, mut peer) = tokio::io::duplex(65536);
    let (reader, writer) = tokio::io::split(transport);
    let (connection, mut receiver, owner) = Connection::spawn(reader, writer).unwrap();
    let reservation = connection
        .reserve_preferred_initialize(Instant::now() + Duration::from_secs(5))
        .unwrap();
    let frame = reservation.encode().unwrap();
    let ticket = connection
        .enqueue(
            reservation,
            frame,
            Arc::new(Allow),
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
    assert_eq!(ticket.wait().await, WriteReport::Written);
    let mut reader = BufReader::new(&mut peer);
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    let request: serde_json::Value = serde_json::from_str(&line).unwrap();
    drop(reader);
    let mut result=serde_json::to_vec(&serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":{"protocolVersion":2,"info":{"name":"fixture","version":"fixed"},"capabilities":{"session":{}}}})).unwrap();
    result.push(b'\n');
    peer.write_all(&result).await.unwrap();
    assert!(matches!(
        receiver.next().await.unwrap().unwrap().incoming(),
        Incoming::Response {
            response: Response::Initialized(_),
            ..
        }
    ));
    (connection, receiver, owner, peer)
}
#[tokio::test]
async fn reservations_release_and_timer_progresses_without_consumer() {
    let (c, _receiver, owner, _peer) = ready().await;
    let s = c.strategy().unwrap();
    for _ in 0..128 {
        let r = c
            .reserve_request(
                s.new_session(&setup(Version::V2, "C:/fixture")).unwrap(),
                ResponseDeadline::Unbounded,
            )
            .unwrap();
        let p = r.probe();
        drop(r);
        assert_eq!(
            p.progress(),
            WriteProgress::Finished(WriteReport::Zero {
                reason: ZeroReason::Abandoned
            })
        );
        assert_eq!(c.pending_count(), 0);
    }
    let r = c
        .reserve_request(
            s.new_session(&setup(Version::V2, "C:/fixture")).unwrap(),
            ResponseDeadline::At(Instant::now() + Duration::from_millis(20)),
        )
        .unwrap();
    let p = r.probe();
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(
        p.progress(),
        WriteProgress::Finished(WriteReport::Zero {
            reason: ZeroReason::Deadline
        })
    );
    assert_eq!(c.pending_count(), 0);
    drop(r);
    future_send(owner.shutdown(Instant::now() + Duration::from_secs(5)));
    assert!(
        owner
            .shutdown(Instant::now() + Duration::from_secs(5))
            .await
            .joined
    );
}
#[tokio::test]
async fn private_projection_requires_real_received_scope() {
    use crate::domain::{
        EntityId,
        native::{
            HarnessConnectionEpoch, LoadEpoch, NativeSessionKey, ResourceEpoch, RuntimeEpoch,
        },
        views::{ViewLimits, ViewWindows},
    };
    let (a, mut ra, oa, mut pa) = ready().await;
    let (_b, mut rb, ob, mut pb) = ready().await;
    let target = NativeSessionKey::from_config(Some(EntityId::from_bytes([1; 16])), "s").unwrap();
    let epoch = ResourceEpoch {
        runtime: RuntimeEpoch(EntityId::from_bytes([1; 16])),
        connection: HarnessConnectionEpoch(EntityId::from_bytes([2; 16])),
        load: LoadEpoch(EntityId::from_bytes([3; 16])),
    };
    let mut windows = ViewWindows::new(ViewLimits::default()).unwrap();
    let handle = windows.begin(target, epoch).unwrap();
    let mut read =
        view_observations::ObservationRead::begin(&mut windows, &handle, &a.strategy().unwrap())
            .unwrap();
    let wire=b"{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"s\",\"update\":{\"sessionUpdate\":\"future\"}}}\n";
    pa.write_all(wire).await.unwrap();
    pb.write_all(wire).await.unwrap();
    let local = ra.next().await.unwrap().unwrap();
    let foreign = rb.next().await.unwrap().unwrap();
    assert!(matches!(
        read.observe(&mut windows, &local).unwrap(),
        view_observations::ObservationOutcome::Recorded { .. }
    ));
    assert_eq!(
        read.observe(&mut windows, &foreign).unwrap_err(),
        view_observations::ObservationError::InvalidObservation
    );
    assert!(
        oa.shutdown(Instant::now() + Duration::from_secs(5))
            .await
            .joined
    );
    assert!(
        ob.shutdown(Instant::now() + Duration::from_secs(5))
            .await
            .joined
    );
}
#[tokio::test]
async fn written_response_deadline_stops_idle_writer_without_shutdown_rescue() {
    let (c, mut receiver, owner, _peer) = ready().await;
    let r = c
        .reserve_request(
            c.strategy()
                .unwrap()
                .new_session(&setup(Version::V2, "C:/fixture"))
                .unwrap(),
            ResponseDeadline::At(Instant::now() + Duration::from_millis(40)),
        )
        .unwrap();
    let frame = r.encode().unwrap();
    let t = c
        .enqueue(
            r,
            frame,
            Arc::new(Allow),
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
    assert_eq!(t.wait().await, WriteReport::Written);
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(
        timeout(Duration::from_secs(2), receiver.next())
            .await
            .unwrap()
            .unwrap_err(),
        AcpError::Deadline
    );
    assert_eq!(t.progress(), WriteProgress::Finished(WriteReport::Written));
    assert!(
        owner
            .shutdown(Instant::now() + Duration::from_secs(2))
            .await
            .joined
    );
}
#[tokio::test]
async fn immediate_reader_eof_has_registered_tasks_and_terminal() {
    let (_c, mut receiver, owner) =
        Connection::spawn(tokio::io::empty(), tokio::io::sink()).unwrap();
    assert_eq!(
        timeout(Duration::from_secs(2), receiver.next())
            .await
            .unwrap()
            .unwrap_err(),
        AcpError::DeliveryUnknown
    );
    assert!(
        owner
            .shutdown(Instant::now() + Duration::from_secs(2))
            .await
            .joined
    );
}
#[cfg(windows)]
#[tokio::test]
async fn owned_protocol_child_real_pipes_handle_nested_permission_and_close() {
    use std::process::Stdio;
    let script = r#"
while($null -ne ($line=[Console]::ReadLine())) {
 $r=$line|ConvertFrom-Json
 if($r.method -eq 'initialize') {$result=@{protocolVersion=2;info=@{name='fixture';version='fixed'};capabilities=@{session=@{}}}}
 elseif($r.method -eq 'session/new') {$result=@{sessionId='s'}}
 elseif($r.method -eq 'session/prompt') {
  [Console]::WriteLine('{"jsonrpc":"2.0","id":"approval","method":"session/request_permission","params":{"sessionId":"s","title":"fixture","options":[{"optionId":"allow","kind":"allow_once","name":"Allow"}]}}')
  $reply=[Console]::ReadLine()|ConvertFrom-Json
  if($reply.id -ne 'approval') {exit 31}
  $result=@{messageId='m'}
 }
 elseif($r.method -eq 'session/close') {$result=@{}}
 elseif($r.method -eq 'session/cancel') {continue}
 else {exit 32}
 [Console]::WriteLine((@{jsonrpc='2.0';id=$r.id;result=$result}|ConvertTo-Json -Compress -Depth 12))
}
"#;
    let mut command = tokio::process::Command::new("powershell");
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .creation_flags(0x0800_0000);
    let mut child = command.spawn().unwrap();
    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut client = TestClient::new(stdout, stdin);
    let d = || Instant::now() + Duration::from_secs(60);
    client.initialize_preferred(d()).await.unwrap();
    assert!(matches!(
        client.next(d()).await.unwrap(),
        Incoming::Response {
            response: Response::Initialized(_),
            ..
        }
    ));
    let r = client
        .strategy()
        .unwrap()
        .new_session("C:/fixture")
        .unwrap();
    client.request(r, d()).await.unwrap();
    assert!(matches!(
        client.next(d()).await.unwrap(),
        Incoming::Response {
            response: Response::NewSession(_),
            ..
        }
    ));
    let s = parse_session(&serde_json::json!("s")).unwrap();
    let r = client
        .strategy()
        .unwrap()
        .prompt(s.clone(), &serde_json::json!([]))
        .unwrap();
    client.request(r, d()).await.unwrap();
    let Incoming::Permission(p) = client.next(d()).await.unwrap() else {
        panic!("permission")
    };
    client
        .permission_response(&p, Some(p.options()[0].id()), d())
        .await
        .unwrap();
    assert!(matches!(
        client.next(d()).await.unwrap(),
        Incoming::Response {
            response: Response::Prompt(PromptResult::Inserted { .. }),
            ..
        }
    ));
    client.cancel(&s, d()).await.unwrap();
    let r = client.strategy().unwrap().close(s).unwrap();
    client.request(r, d()).await.unwrap();
    assert!(matches!(
        client.next(d()).await.unwrap(),
        Incoming::Response {
            response: Response::Closed { .. },
            ..
        }
    ));
    client.shutdown(d()).await.unwrap();
    assert!(
        timeout(Duration::from_secs(60), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
}
#[test]
fn tool_name_patch_debug_redacts_the_actual_untrusted_value() {
    let strategy = unit_strategy(Version::V2);
    let sentinel = "private-tool-name-sentinel";
    let value = raw_value(&serde_json::json!({"name":sentinel}));
    let patch = strategy.tool_name_patch(value.as_ref()).unwrap();
    assert!(matches!(patch, ToolNamePatch::Set(_)));
    assert!(!format!("{patch:?}").contains(sentinel));
}
#[test]
fn runtime_shutdown_then_new_runtime_close_joins_owned_io_resources() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct R(Arc<AtomicUsize>);
    impl tokio::io::AsyncRead for R {
        fn poll_read(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            _: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Pending
        }
    }
    impl Drop for R {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    struct W(Arc<AtomicUsize>);
    impl AsyncWrite for W {
        fn poll_write(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            _: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Pending
        }
        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Pending
        }
        fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Pending
        }
    }
    impl Drop for W {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    for _ in 0..8 {
        let drops = Arc::new(AtomicUsize::new(0));
        let a = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (c, mut receiver, owner) = {
            let _enter = a.enter();
            Connection::spawn(R(drops.clone()), W(drops.clone())).unwrap()
        };
        a.shutdown_background();
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async move {
                let report = owner
                    .shutdown(Instant::now() + Duration::from_secs(5))
                    .await;
                assert!(report.joined);
                assert_eq!(drops.load(Ordering::SeqCst), 2);
                assert!(receiver.next().await.is_err());
                assert!(receiver.next().await.unwrap().is_none());
                drop(c);
            });
    }
}
#[tokio::test]
async fn embedded_resources_validate_schema_anyof_without_losing_raw() {
    let mut misaccepted = Vec::new();
    for wire in [Version::V1, Version::V2] {
        let (a, b) = tokio::io::duplex(65536);
        let (ar, aw) = tokio::io::split(a);
        let (br, mut bw) = tokio::io::split(b);
        let mut peer = BufReader::new(br);
        let mut client = TestClient::new(ar, aw);
        let d = || Instant::now() + Duration::from_secs(5);
        let id = client.initialize(wire, d()).await.unwrap();
        let mut line = String::new();
        peer.read_line(&mut line).await.unwrap();
        let result = if wire == Version::V1 {
            serde_json::json!({"protocolVersion":1,"agentCapabilities":{"promptCapabilities":{"embeddedContext":true}}})
        } else {
            serde_json::json!({"protocolVersion":2,"info":{"name":"fixture","version":"fixed"},"capabilities":{"session":{"prompt":{"embeddedContext":{}}}}})
        };
        let mut response = serde_json::to_vec(
            &serde_json::json!({"jsonrpc":"2.0","id":id.json(),"result":result}),
        )
        .unwrap();
        response.push(b'\n');
        bw.write_all(&response).await.unwrap();
        client.next(d()).await.unwrap();
        let strategy = client.strategy().unwrap().0.clone();
        let session = parse_session(&serde_json::json!("s")).unwrap();
        for (index, resource) in [
            serde_json::json!({}),
            serde_json::json!({"text":"x"}),
            serde_json::json!({"uri":7,"text":"x"}),
            serde_json::json!({"uri":"urn:x"}),
            serde_json::json!({"uri":"urn:x","text":7,"blob":false}),
        ]
        .into_iter()
        .enumerate()
        {
            let prompt = raw_value(&serde_json::json!([{"type":"resource","resource":resource}]));
            if strategy
                .prompt(session.clone(), prompt, ExtensionFields::empty().unwrap())
                .err()
                != Some(AcpError::InvalidMessage)
            {
                misaccepted.push((wire, index));
            }
            assert_eq!(client.pending_count(), 0);
        }
        for resource in [
            serde_json::json!({"uri":"urn:x","text":""}),
            serde_json::json!({"uri":"urn:x","blob":""}),
            serde_json::json!({"uri":"urn:x","text":"x","blob":"y"}),
            serde_json::json!({"uri":"urn:x","text":7,"blob":"y"}),
            serde_json::json!({"uri":"urn:x","text":"x","blob":false}),
        ] {
            let prompt = raw_value(&serde_json::json!([{"type":"resource","resource":resource}]));
            assert!(
                strategy
                    .prompt(session.clone(), prompt, ExtensionFields::empty().unwrap())
                    .is_ok()
            );
        }
        let bytes=br#"[{"type":"resource","resource":{"uri":"urn:x","text":"","extension":{"n":1.2300e+7,"huge":1e99999,"escape":"\u0041"}}}]"#;
        let prompt = raw_json::RawDocument::parse(bytes.to_vec())
            .unwrap()
            .root_owned();
        let request = strategy
            .prompt(session, prompt, ExtensionFields::empty().unwrap())
            .unwrap();
        let reservation = client
            .connection
            .reserve_request(request, ResponseDeadline::Unbounded)
            .unwrap();
        let encoded = reservation.encode().unwrap();
        let text = std::str::from_utf8(encoded.bytes()).unwrap();
        assert!(text.contains(r#""extension":{"n":1.2300e+7,"huge":1e99999,"escape":"\u0041"}"#));
        drop(reservation);
        assert_eq!(client.pending_count(), 0);
        client.shutdown(d()).await.unwrap();
    }
    assert!(
        misaccepted.is_empty(),
        "invalid resource branches accepted: {misaccepted:?}"
    );
}
#[tokio::test]
async fn remote_error_code_remains_readable_but_is_redacted_from_debug() {
    let (c, mut receiver, owner, mut peer) = ready().await;
    let request = c
        .strategy()
        .unwrap()
        .new_session(&setup(Version::V2, "C:/fixture"))
        .unwrap();
    let r = c
        .reserve_request(request, ResponseDeadline::Unbounded)
        .unwrap();
    let id = r.id().unwrap().clone();
    let frame = r.encode().unwrap();
    let ticket = c
        .enqueue(
            r,
            frame,
            Arc::new(Allow),
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
    assert_eq!(ticket.wait().await, WriteReport::Written);
    let mut reader = BufReader::new(&mut peer);
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    drop(reader);
    let code = 91_234_567_890_123i64;
    let mut wire=serde_json::to_vec(&serde_json::json!({"jsonrpc":"2.0","id":id.json(),"error":{"code":code,"message":"private diagnostic"}})).unwrap();
    wire.push(b'\n');
    peer.write_all(&wire).await.unwrap();
    let event = receiver.next().await.unwrap().unwrap();
    let Incoming::Response { response, .. } = event.incoming() else {
        panic!("response")
    };
    assert!(matches!(response,Response::RemoteError{code:value} if *value==code));
    assert!(!format!("{response:?}").contains(&code.to_string()));
    assert!(!format!("{:?}", event.incoming()).contains(&code.to_string()));
    assert!(
        owner
            .shutdown(Instant::now() + Duration::from_secs(5))
            .await
            .joined
    );
}
#[tokio::test]
async fn closing_drain_keeps_all_rejected_objects_and_later_legal_frame() {
    let (c, mut receiver, owner, mut peer) = ready().await;
    let first = br#" {"jsonrpc":"2.0","id":999,"result":{},"marker":"first"} "#;
    let second = br#"{"jsonrpc":"2.0","method":7,"marker":"second"}"#;
    let tail = br#"{"jsonrpc":"2.0","method":"after-error","params":{"marker":"tail"}}"#;
    let mut batch = Vec::new();
    for frame in [
        first.as_slice(),
        second.as_slice(),
        b"{not-json".as_slice(),
        tail.as_slice(),
    ] {
        batch.extend_from_slice(frame);
        batch.push(b'\n');
    }
    peer.write_all(&batch).await.unwrap();
    let event = timeout(Duration::from_secs(5), receiver.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(matches!(
        event.incoming(),
        Incoming::UnknownNotification { .. }
    ));
    assert_eq!(event.raw().bytes(), tail);
    assert_eq!(
        timeout(Duration::from_secs(5), receiver.next())
            .await
            .unwrap()
            .unwrap_err(),
        AcpError::UnmatchedResponse
    );
    assert!(receiver.next().await.unwrap().is_none());
    assert_eq!(c.take_rejected_message().unwrap().bytes(), first);
    assert_eq!(
        c.take_rejected_message()
            .expect("second rejected object retained")
            .bytes(),
        second
    );
    assert!(c.take_rejected_message().is_none());
    assert!(
        owner
            .shutdown(Instant::now() + Duration::from_secs(5))
            .await
            .joined
    );
}
