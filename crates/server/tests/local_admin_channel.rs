//! 本地通道的 framing、channel 绑定、未完成请求上限与 facade 缺席期行为（`[PV3]` 的 WP3a 部分）。
//!
//! 覆盖 `specs/local-admin-channel` 的 R23–R32：合法帧处理、空帧、超长帧、未知 channel、channel 混用、
//! attachment 生命周期（注册表本身在 `transport::local::attachment` 的单元测试里）、第 33 条未完成请求
//! 关闭连接、`0x02` 即连即关。
//!
//! 用 `tokio::io::duplex` 驱动传输无关的部分；真实 endpoint 的用例在 `local_endpoint_windows.rs` /
//! `local_endpoint_unix.rs`。所有「关闭连接」的断言都同时检查**没有写出任何错误帧**（§3 末条）。

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use server::local_admin::{
    AdminOutcome, AdminResponse, JsonObject, LocalAdminHandler, UnroutedAdminHandler,
};
use server::transport::local::{
    CHANNEL_ACP_STREAM_BYTE, CHANNEL_LOCAL_ADMIN_BYTE, CloseReason, LocalConnectionHandlers,
    MAX_FRAME_PAYLOAD_BYTES, TransportError, serve_connection,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

/// 每个用例一条独立的请求 id（canonical 小写 UUID）。
fn request_id(index: u32) -> String {
    format!("5b1f0c2e-8a4d-4b6f-9c31-{index:012}")
}

/// 一条合法请求信封的 payload（不含 channel 字节）。
fn request_payload(index: u32) -> Vec<u8> {
    format!(
        r#"{{"v":1,"id":"{}","method":"daemon.status","params":{{}}}}"#,
        request_id(index)
    )
    .into_bytes()
}

/// 一条帧：`u32be payload 长度 | channel | body`。
fn frame(channel: u8, body: &[u8]) -> Vec<u8> {
    let length = u32::try_from(body.len() + 1).expect("用例里的 payload 不会超过 4 GiB");
    let mut out = Vec::with_capacity(4 + body.len() + 1);
    out.extend_from_slice(&length.to_be_bytes());
    out.push(channel);
    out.extend_from_slice(body);
    out
}

/// 读一帧；对端关闭时返回 `None`。
async fn read_frame<R>(reader: &mut R) -> Option<(u8, Vec<u8>)>
where
    R: AsyncRead + Unpin,
{
    let mut header = [0u8; 4];
    if reader.read_exact(&mut header).await.is_err() {
        return None;
    }
    let length = usize::try_from(u32::from_be_bytes(header)).expect("32 位长度");
    let mut payload = vec![0u8; length];
    reader.read_exact(&mut payload).await.expect("完整帧");
    let channel = payload.first().copied().expect("frame 长度已校验非 0");
    Some((channel, payload[1..].to_vec()))
}

/// 断言连接已关闭且**没有**任何 frame 被写出（§3：framing 错误不返回错误帧）。
async fn assert_closed_without_frame<R>(reader: &mut R)
where
    R: AsyncRead + Unpin,
{
    assert_eq!(read_frame(reader).await, None, "不应写出任何 frame");
}

/// 取服务端的关闭原因（连接级错误视为失败）。
async fn close_reason(handle: JoinHandle<Result<CloseReason, TransportError>>) -> CloseReason {
    match handle.await {
        Ok(Ok(reason)) => reason,
        Ok(Err(error)) => panic!("连接级错误：{error}"),
        Err(error) => panic!("服务端任务异常：{error}"),
    }
}

/// 只处理管理请求的装配。
fn handlers(admin: Arc<dyn LocalAdminHandler>) -> Arc<LocalConnectionHandlers> {
    Arc::new(LocalConnectionHandlers::new(admin))
}

#[tokio::test]
async fn legal_admin_frames_are_answered_and_keep_the_connection_usable() {
    let (mut client, server_stream) = tokio::io::duplex(64 * 1024);
    let handle = tokio::spawn(serve_connection(
        server_stream,
        handlers(Arc::new(UnroutedAdminHandler)),
    ));

    client
        .write_all(&frame(CHANNEL_LOCAL_ADMIN_BYTE, &request_payload(1)))
        .await
        .expect("写入第一帧");
    let (channel, body) = read_frame(&mut client).await.expect("第一个响应");
    assert_eq!(channel, CHANNEL_LOCAL_ADMIN_BYTE);
    let response = AdminResponse::decode(&body).expect("响应可解码");
    assert_eq!(response.id().as_str(), request_id(1));
    assert!(matches!(
        response.outcome(),
        AdminOutcome::Failure { error } if error.code().as_str() == "local.unsupported"
    ));

    // 同一连接上继续承载请求（管理连接是多次请求/响应的会话）。
    client
        .write_all(&frame(CHANNEL_LOCAL_ADMIN_BYTE, &request_payload(2)))
        .await
        .expect("写入第二帧");
    let (_, body) = read_frame(&mut client).await.expect("第二个响应");
    let response = AdminResponse::decode(&body).expect("响应可解码");
    assert_eq!(response.id().as_str(), request_id(2));

    drop(client);
    assert_eq!(close_reason(handle).await, CloseReason::PeerEof);
}

#[tokio::test]
async fn empty_frame_closes_the_connection_without_an_error_frame() {
    let (mut client, server_stream) = tokio::io::duplex(1024);
    let handle = tokio::spawn(serve_connection(
        server_stream,
        handlers(Arc::new(UnroutedAdminHandler)),
    ));

    client.write_all(&0u32.to_be_bytes()).await.expect("空帧头");
    assert_closed_without_frame(&mut client).await;
    assert_eq!(close_reason(handle).await, CloseReason::EmptyFrame);
}

#[tokio::test]
async fn oversized_frame_closes_the_connection_without_reading_the_payload() {
    let (mut client, server_stream) = tokio::io::duplex(1024);
    let handle = tokio::spawn(serve_connection(
        server_stream,
        handlers(Arc::new(UnroutedAdminHandler)),
    ));

    let oversized = u32::try_from(MAX_FRAME_PAYLOAD_BYTES).expect("1 MiB 在 u32 内") + 1;
    client
        .write_all(&oversized.to_be_bytes())
        .await
        .expect("超长帧头");
    assert_closed_without_frame(&mut client).await;
    assert_eq!(
        close_reason(handle).await,
        CloseReason::FrameTooLarge { length: oversized }
    );
}

#[tokio::test]
async fn frame_at_the_exact_limit_is_accepted_by_framing() {
    let (mut client, server_stream) = tokio::io::duplex(64 * 1024);
    let handle = tokio::spawn(serve_connection(
        server_stream,
        handlers(Arc::new(UnroutedAdminHandler)),
    ));

    // payload 恰好 1 MiB：§3 的上限合法取等号，因此这是「信封非法」而不是「framing 错误」。
    let body = vec![b' '; MAX_FRAME_PAYLOAD_BYTES - 1];
    client
        .write_all(&frame(CHANNEL_LOCAL_ADMIN_BYTE, &body))
        .await
        .expect("写入上限帧");
    let (channel, response) = read_frame(&mut client).await.expect("上限帧被读取并回答");
    assert_eq!(channel, CHANNEL_LOCAL_ADMIN_BYTE);
    let response = AdminResponse::decode(&response).expect("响应可解码");
    assert!(matches!(
        response.outcome(),
        AdminOutcome::Failure { error } if error.code().as_str() == "local.invalid_request"
    ));

    drop(client);
    assert_eq!(close_reason(handle).await, CloseReason::PeerEof);
}

#[tokio::test]
async fn unknown_channel_closes_the_connection_without_an_error_frame() {
    let (mut client, server_stream) = tokio::io::duplex(1024);
    let handle = tokio::spawn(serve_connection(
        server_stream,
        handlers(Arc::new(UnroutedAdminHandler)),
    ));

    client
        .write_all(&frame(0x03, b"{}"))
        .await
        .expect("未知 channel");
    assert_closed_without_frame(&mut client).await;
    assert_eq!(
        close_reason(handle).await,
        CloseReason::UnknownChannel { byte: 0x03 }
    );
}

#[tokio::test]
async fn channel_mismatch_closes_the_connection_after_the_answer() {
    let (mut client, server_stream) = tokio::io::duplex(1024);
    let handle = tokio::spawn(serve_connection(
        server_stream,
        handlers(Arc::new(UnroutedAdminHandler)),
    ));

    client
        .write_all(&frame(CHANNEL_LOCAL_ADMIN_BYTE, &request_payload(1)))
        .await
        .expect("首帧定为管理连接");
    let (_, body) = read_frame(&mut client).await.expect("首帧的响应");
    let response = AdminResponse::decode(&body).expect("响应可解码");
    assert_eq!(response.id().as_str(), request_id(1));

    client
        .write_all(&frame(CHANNEL_ACP_STREAM_BYTE, b"{}"))
        .await
        .expect("混用 channel");
    assert_closed_without_frame(&mut client).await;
    assert_eq!(
        close_reason(handle).await,
        CloseReason::ChannelMismatch {
            expected: server::transport::local::ChannelKind::LocalAdmin,
            found: server::transport::local::ChannelKind::AcpStream,
        }
    );
}

#[tokio::test]
async fn acp_stream_connection_is_closed_immediately_while_facade_is_absent() {
    let (mut client, server_stream) = tokio::io::duplex(1024);
    let handle = tokio::spawn(serve_connection(
        server_stream,
        handlers(Arc::new(UnroutedAdminHandler)),
    ));

    // 合法的一帧 0x02（framing 校验通过）：facade 未装配 → 立即关闭、不转发字节、不返回错误帧。
    client
        .write_all(&frame(CHANNEL_ACP_STREAM_BYTE, b"{\"jsonrpc\":\"2.0\"}"))
        .await
        .expect("写入 0x02 帧");
    assert_closed_without_frame(&mut client).await;
    assert_eq!(close_reason(handle).await, CloseReason::FacadeUnavailable);

    // 该路径不分配也不消耗 attachment：`FacadeAttachmentRegistry` 只在切片 6 的 facade 分发点被调用，
    // 本切片的 `0x02` 分支在上面的 `break` 之前不会触及它（结构性证据，见模块文档）。
}

#[tokio::test]
async fn envelope_version_two_closes_the_connection_without_an_error_frame() {
    let (mut client, server_stream) = tokio::io::duplex(1024);
    let handle = tokio::spawn(serve_connection(
        server_stream,
        handlers(Arc::new(UnroutedAdminHandler)),
    ));

    let body = format!(
        r#"{{"v":2,"id":"{}","method":"daemon.status","params":{{}}}}"#,
        request_id(1)
    );
    client
        .write_all(&frame(CHANNEL_LOCAL_ADMIN_BYTE, body.as_bytes()))
        .await
        .expect("写入 v=2 信封");
    assert_closed_without_frame(&mut client).await;
    assert_eq!(
        close_reason(handle).await,
        CloseReason::ChannelVersionUnsupported
    );
}

#[tokio::test]
async fn rejected_requests_keep_the_connection_usable() {
    let (mut client, server_stream) = tokio::io::duplex(4096);
    let handle = tokio::spawn(serve_connection(
        server_stream,
        handlers(Arc::new(UnroutedAdminHandler)),
    ));

    // v1 方法集内但未实现 → local.unsupported；语法合法但不在方法集里 → local.unsupported；
    // params 不是 object / 方法名命名非法 → local.invalid_request。全部都必须保持连接可用（R35）。
    let cases = [
        (
            r#"{"v":1,"id":"ID","method":"daemon.status","params":{}}"#,
            "local.unsupported",
        ),
        (
            r#"{"v":1,"id":"ID","method":"daemon.doctor","params":{}}"#,
            "local.unsupported",
        ),
        (
            r#"{"v":1,"id":"ID","method":"daemon.status","params":null}"#,
            "local.invalid_request",
        ),
        (
            r#"{"v":1,"id":"ID","method":"Daemon.status","params":{}}"#,
            "local.invalid_request",
        ),
        // 连字符名不匹配 §4 的方法名语法（命名非法 → invalid_request），因此 §5.7 的
        // `node.rotate-key.begin` 在本实现下也走这一条而不是 local.unsupported（见交付报告）。
        (
            r#"{"v":1,"id":"ID","method":"node.rotate-key.begin","params":{}}"#,
            "local.invalid_request",
        ),
    ];
    for (index, (template, expected_code)) in cases.iter().enumerate() {
        let index = u32::try_from(index).expect("用例数量很小");
        let body = template.replace("ID", &request_id(index));
        client
            .write_all(&frame(CHANNEL_LOCAL_ADMIN_BYTE, body.as_bytes()))
            .await
            .expect("写入请求");
        let (_, body) = read_frame(&mut client).await.expect("每个请求恰好一个响应");
        let response = AdminResponse::decode(&body).expect("响应可解码");
        match response.outcome() {
            AdminOutcome::Failure { error } => assert_eq!(error.code().as_str(), *expected_code),
            AdminOutcome::Success { .. } => panic!("{template} 应失败"),
        }
    }

    // 连接仍然可用：再发一条合法请求依然得到响应。
    client
        .write_all(&frame(CHANNEL_LOCAL_ADMIN_BYTE, &request_payload(9)))
        .await
        .expect("连接应仍然可用");
    let (_, body) = read_frame(&mut client).await.expect("连接可用");
    assert_eq!(
        AdminResponse::decode(&body)
            .expect("响应可解码")
            .id()
            .as_str(),
        request_id(9)
    );

    drop(client);
    assert_eq!(close_reason(handle).await, CloseReason::PeerEof);
}

/// 方法层参数校验失败时的处理器（WP3b 的形状：决定落在方法实现里）。
struct InvalidParamsHandler;

#[async_trait::async_trait]
impl LocalAdminHandler for InvalidParamsHandler {
    async fn handle(&self, request: server::local_admin::AdminRequest) -> AdminResponse {
        AdminResponse::failure(
            request.id().clone(),
            server::local_admin::AdminError::new(
                server::local_admin::LocalErrorCode::InvalidParams,
                "missing field rootPath",
            ),
        )
    }
}

/// `local.invalid_params` 在通道上的完整路径（R35 的后半）：方法层返回该错误码时，
/// 响应信封与连接可用性都不受影响。方法级的参数形状校验属 WP3b（本切片没有方法路由）。
#[tokio::test]
async fn invalid_params_from_the_method_layer_travels_as_a_normal_response() {
    let (mut client, server_stream) = tokio::io::duplex(4096);
    let handle = tokio::spawn(serve_connection(
        server_stream,
        handlers(Arc::new(InvalidParamsHandler)),
    ));

    client
        .write_all(&frame(CHANNEL_LOCAL_ADMIN_BYTE, &request_payload(1)))
        .await
        .expect("写入请求");
    let (_, body) = read_frame(&mut client).await.expect("响应");
    let response = AdminResponse::decode(&body).expect("响应可解码");
    assert!(matches!(
        response.outcome(),
        AdminOutcome::Failure { error }
            if error.code() == server::local_admin::LocalErrorCode::InvalidParams
                && error.message() == "missing field rootPath"
    ));

    // 连接保持可用（参数错误是方法级错误，不是协议滥用）。
    client
        .write_all(&frame(CHANNEL_LOCAL_ADMIN_BYTE, &request_payload(2)))
        .await
        .expect("连接应仍然可用");
    let (_, body) = read_frame(&mut client).await.expect("第二个响应");
    assert_eq!(
        AdminResponse::decode(&body)
            .expect("响应可解码")
            .id()
            .as_str(),
        request_id(2)
    );

    drop(client);
    assert_eq!(close_reason(handle).await, CloseReason::PeerEof);
}

/// §4 规则 6：`error.message` 简短英文，不含 secret、堆栈或完整敏感路径；`result` 永不回显凭据值。
/// 这里用「params 里带凭据形状的值」做回归护栏：错误响应不得把请求内容回显出去。
#[tokio::test]
async fn error_responses_do_not_echo_request_params() {
    let (mut client, server_stream) = tokio::io::duplex(4096);
    let handle = tokio::spawn(serve_connection(
        server_stream,
        handlers(Arc::new(UnroutedAdminHandler)),
    ));

    let body = format!(
        r#"{{"v":1,"id":"{}","method":"provider.configure","params":{{"values":{{"apiKey":"s3cr3t-value-9f2"}}}}}}"#,
        request_id(1)
    );
    client
        .write_all(&frame(CHANNEL_LOCAL_ADMIN_BYTE, body.as_bytes()))
        .await
        .expect("写入请求");
    let (_, body) = read_frame(&mut client).await.expect("响应");
    let text = String::from_utf8(body.clone()).expect("响应是 UTF-8");
    assert!(
        !text.contains("s3cr3t-value-9f2"),
        "响应不得回显凭据值：{text}"
    );
    assert!(!text.contains("apiKey"), "错误响应不得回显 params：{text}");
    let response = AdminResponse::decode(&body).expect("响应可解码");
    match response.outcome() {
        AdminOutcome::Failure { error } => {
            let message = error.message();
            assert!(!message.is_empty());
            assert!(message.chars().count() <= 512);
            assert!(
                message.is_ascii(),
                "error.message 应是简短英文描述，实际 {message}"
            );
        }
        AdminOutcome::Success { .. } => panic!("该请求应失败"),
    }

    drop(client);
    assert_eq!(close_reason(handle).await, CloseReason::PeerEof);
}

/// 阻塞在信号量上的处理器：让未完成请求可以真的堆到上限。
struct GatedHandler {
    permits: Arc<Semaphore>,
    started: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl LocalAdminHandler for GatedHandler {
    async fn handle(&self, request: server::local_admin::AdminRequest) -> AdminResponse {
        self.started.fetch_add(1, Ordering::SeqCst);
        let permit = self.permits.acquire().await.expect("测试里不会关闭信号量");
        permit.forget();
        AdminResponse::success(request.id().clone(), JsonObject::new())
    }
}

/// 等到处理器真的开始处理 `expected` 条请求（最多等 1 秒）。
async fn wait_for_started(started: &AtomicUsize, expected: usize) {
    for _ in 0..1000 {
        if started.load(Ordering::SeqCst) >= expected {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
    panic!(
        "处理器未在 1 秒内处理 {expected} 条请求（实际 {}）",
        started.load(Ordering::SeqCst)
    );
}

async fn write_requests<W>(client: &mut W, first: u32, count: u32)
where
    W: AsyncWrite + Unpin,
{
    for index in first..first + count {
        client
            .write_all(&frame(CHANNEL_LOCAL_ADMIN_BYTE, &request_payload(index)))
            .await
            .expect("写入请求");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn thirty_two_in_flight_requests_are_allowed_and_the_limit_recovers() {
    let (mut client, server_stream) = tokio::io::duplex(64 * 1024);
    let permits = Arc::new(Semaphore::new(0));
    let started = Arc::new(AtomicUsize::new(0));
    let handle = tokio::spawn(serve_connection(
        server_stream,
        handlers(Arc::new(GatedHandler {
            permits: Arc::clone(&permits),
            started: Arc::clone(&started),
        })),
    ));

    write_requests(&mut client, 0, 32).await;
    wait_for_started(&started, 32).await;

    // 放行 32 条：每条恰好一个响应，且 32 个 id 一个不少、互不重复。
    permits.add_permits(32);
    let mut answered = std::collections::HashSet::new();
    for _ in 0..32 {
        let (_, body) = read_frame(&mut client).await.expect("32 条响应都应写出");
        let response = AdminResponse::decode(&body).expect("响应可解码");
        assert!(matches!(response.outcome(), AdminOutcome::Success { .. }));
        answered.insert(response.id().as_str().to_string());
    }
    let expected: std::collections::HashSet<String> = (0..32).map(request_id).collect();
    assert_eq!(
        answered, expected,
        "32 条未完成请求各自恰好一个响应（§4 规则 5）"
    );

    // 计数已回落：连接仍然可用（未完成请求上限是「同时」的上限，不是累计上限）。
    client
        .write_all(&frame(CHANNEL_LOCAL_ADMIN_BYTE, &request_payload(99)))
        .await
        .expect("连接应仍然可用");
    wait_for_started(&started, 33).await;
    permits.add_permits(1);
    let (_, body) = read_frame(&mut client).await.expect("第 33 条也要有响应");
    assert_eq!(
        AdminResponse::decode(&body)
            .expect("响应可解码")
            .id()
            .as_str(),
        request_id(99)
    );

    drop(client);
    assert_eq!(close_reason(handle).await, CloseReason::PeerEof);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_thirty_third_in_flight_request_closes_the_connection() {
    let (mut client, server_stream) = tokio::io::duplex(64 * 1024);
    let permits = Arc::new(Semaphore::new(0));
    let started = Arc::new(AtomicUsize::new(0));
    let handle = tokio::spawn(serve_connection(
        server_stream,
        handlers(Arc::new(GatedHandler {
            permits: Arc::clone(&permits),
            started: Arc::clone(&started),
        })),
    ));

    write_requests(&mut client, 0, 33).await;
    // 第 33 条触发协议滥用：关闭连接，且不写出任何错误帧（已完成的响应不受影响——本用例里没有已完成的）。
    assert_closed_without_frame(&mut client).await;
    assert_eq!(
        close_reason(handle).await,
        CloseReason::TooManyInFlightRequests
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn duplicate_in_flight_request_id_is_rejected_without_closing_the_connection() {
    let (mut client, server_stream) = tokio::io::duplex(64 * 1024);
    let permits = Arc::new(Semaphore::new(0));
    let started = Arc::new(AtomicUsize::new(0));
    let handle = tokio::spawn(serve_connection(
        server_stream,
        handlers(Arc::new(GatedHandler {
            permits: Arc::clone(&permits),
            started: Arc::clone(&started),
        })),
    ));

    write_requests(&mut client, 0, 1).await;
    wait_for_started(&started, 1).await;
    // 同一条未完成请求的 id 再来一次：重复 id 立即被拒，不占用新的未完成槽位。
    write_requests(&mut client, 0, 1).await;
    let (_, body) = read_frame(&mut client).await.expect("重复 id 的响应");
    let response = AdminResponse::decode(&body).expect("响应可解码");
    assert_eq!(response.id().as_str(), request_id(0));
    assert!(matches!(
        response.outcome(),
        AdminOutcome::Failure { error } if error.code().as_str() == "local.invalid_request"
    ));

    // 首条请求仍在未完成集合里，放行后得到成功响应；连接此后仍可用。
    permits.add_permits(1);
    let (_, first) = read_frame(&mut client).await.expect("首条的响应");
    assert!(matches!(
        AdminResponse::decode(&first).expect("响应可解码").outcome(),
        AdminOutcome::Success { .. }
    ));

    client
        .write_all(&frame(CHANNEL_LOCAL_ADMIN_BYTE, &request_payload(7)))
        .await
        .expect("连接应仍然可用");
    wait_for_started(&started, 2).await;
    permits.add_permits(1);
    let (_, body) = read_frame(&mut client).await.expect("新请求的响应");
    assert_eq!(
        AdminResponse::decode(&body)
            .expect("响应可解码")
            .id()
            .as_str(),
        request_id(7)
    );

    drop(client);
    assert_eq!(close_reason(handle).await, CloseReason::PeerEof);
}
