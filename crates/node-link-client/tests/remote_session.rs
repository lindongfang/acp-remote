//! 远程会话的 `SessionEndpoint` 路由：prompt/命令经 `resource.attach` 票证提交，调用方给出的
//! `requestId` 原样保留；连接丢失后进入 uncertain，**只**以 `command.status` 回查，不重发副作用命令。
//!
//! 网络承载是内存通道；客户端侧（握手、信封、票证缓存、pending 表、uncertain 表）都是真实实现。

mod support;

use std::sync::{Arc, Mutex};

use acp_core::model::{PromptContentBlock, PromptRequest, RequestId};
use acp_core::ports::SessionEndpoint;
use node_link_protocol::command::{CommandSubmit, TerminalStatus};
use node_link_protocol::common::{DecimalString, SessionMeta, SessionState, Timestamp};
use node_link_protocol::envelope::MessageType;
use node_link_protocol::resource::Attached;

use node_link_client::handshake::OwnerTrust;
use node_link_client::identity::NodeIdentity;
use node_link_client::{ClientConfig, ClientError, NodeLinkClient, TlsTrust};

use support::{
    MemTransport, OwnerIdentity, OwnerIo, RecordingDeliveries, RecordingSink, accept_handshake,
    decimal, selected_features, uuid_text,
};

use acp_core::model::{NodeId, PeerPublicKey};
use async_trait::async_trait;
use identity_auth::P1363Signature;

/// `command.status` 回查记录：`(查询 requestId, 目标 requestId)`。
type Queried = Arc<Mutex<Vec<(String, Option<String>)>>>;

const ACCESS_NODE: &str = "9f1c3a52-6a4e-4f0e-8d3c-5b2a1e0f7c12";
const OWNER_NODE: &str = "5c6d7e8f-9012-4a3b-8c4d-5e6f708192b5";
const SESSION: &str = "2bc8b944-2a4f-46a7-8c31-b2c40923f60a";
const ATTACHMENT: &str = "018f0000-0000-4000-8000-0000000000aa";
const VERSION: u64 = 3;

/// 固定 P-256 身份的 `NodeIdentity`。
struct FixedIdentity {
    node_id: NodeId,
    signing: p256::ecdsa::SigningKey,
}

impl FixedIdentity {
    fn new(seed: [u8; 32]) -> Self {
        Self {
            node_id: NodeId::new(ACCESS_NODE).expect("node id"),
            signing: p256::ecdsa::SigningKey::from_slice(&seed).expect("signing key"),
        }
    }
}

#[async_trait]
impl NodeIdentity for FixedIdentity {
    fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    async fn public_key(&self) -> Result<PeerPublicKey, ClientError> {
        let encoded = self.signing.verifying_key().to_sec1_point(false);
        PeerPublicKey::try_from_bytes(encoded.as_bytes())
            .map_err(|_| ClientError::Local("test key"))
    }

    async fn sign(&self, transcript: &[u8]) -> Result<P1363Signature, ClientError> {
        use p256::ecdsa::signature::Signer as _;
        let signature: p256::ecdsa::Signature = self.signing.sign(transcript);
        P1363Signature::try_from_bytes(signature.to_bytes().as_slice())
            .map_err(|_| ClientError::Local("test signature"))
    }
}

fn owner_trust(owner: &OwnerIdentity) -> OwnerTrust {
    OwnerTrust {
        node_id: owner.node_id.clone(),
        public_key: owner.public_key.clone(),
    }
}

fn ownership() -> OwnerIdentity {
    OwnerIdentity::new([0x51; 32], OWNER_NODE)
}

fn remote() -> acp_core::model::RemoteSessionRef {
    acp_core::model::RemoteSessionRef::new(
        NodeId::new(OWNER_NODE).expect("owner"),
        acp_core::model::SessionId::new(SESSION).expect("session"),
    )
}

/// `resource.attached`：票证与当前会话元数据。
fn attached() -> Attached {
    Attached {
        attachment_id: uuid_text(ATTACHMENT),
        attachment_generation: decimal(1),
        session_meta: SessionMeta {
            state: SessionState::Idle,
            version: DecimalString::parse(&VERSION.to_string()).expect("版本"),
        },
    }
}

/// 回填空 catalog（supervisor 在握手后总会先发 `catalog.subscribe`）。
async fn send_catalog_snapshot(io: &mut OwnerIo, connection: &node_link_protocol::common::Uuid) {
    let snapshot = node_link_protocol::catalog::CatalogSnapshot {
        revision: decimal(1),
        agents: node_link_protocol::catalog::AgentEntryList::new(Vec::new()).expect("agents"),
        directories: node_link_protocol::catalog::DirectoryEntryList::new(Vec::new())
            .expect("directories"),
        sessions: node_link_protocol::catalog::CatalogSessionList::new(Vec::new())
            .expect("sessions"),
    };
    io.send_envelope(MessageType::CatalogSnapshot, connection, &snapshot)
        .await;
}

/// 回填 `resource.attached`（票证与当前会话元数据）。
async fn reply_attached(io: &mut OwnerIo, connection: &node_link_protocol::common::Uuid) {
    io.send_envelope(MessageType::ResourceAttached, connection, &attached())
        .await;
}

/// 读一条 `command.submit`（所有命令共用同一个信封类型）。
async fn recv_submit(
    io: &mut OwnerIo,
    connection: &node_link_protocol::common::Uuid,
    inbound: &mut u64,
) -> CommandSubmit {
    let envelope = io
        .recv_envelope(connection, inbound)
        .await
        .expect("command.submit");
    assert_eq!(envelope.message_type(), MessageType::CommandSubmit);
    serde_json::from_str(envelope.body().get()).expect("command.submit body")
}

/// 接受一条命令（`command.accepted`；prompt 的接受语义不等待终态）。
async fn accept_command(
    io: &mut OwnerIo,
    connection: &node_link_protocol::common::Uuid,
    submit: &CommandSubmit,
) {
    let accepted = node_link_protocol::command::CommandAccepted {
        request_id: submit.request_id.clone(),
        command: submit.command,
        accepted_at: Timestamp::parse("2026-10-20T00:00:00.000Z").expect("时间戳"),
        result: acpr_wire::Nullable::null(),
    };
    io.send_envelope(MessageType::CommandAccepted, connection, &accepted)
        .await;
}

fn request(text: &str) -> RequestId {
    RequestId::new(text).expect("request id")
}

/// `terminal.status != completed` 必须携带 `error`（schema 的 `if/then`），否则客户端按 schema 失败关闭。
fn uncertain_error() -> node_link_protocol::common::PublicError {
    node_link_protocol::common::PublicError {
        code: node_link_protocol::error::ErrorCode::CommandUncertain,
        message: node_link_protocol::common::Text::parse("the command result is uncertain")
            .expect("message"),
        retryable: false,
        details: node_link_protocol::common::RawObject::empty(),
    }
}

fn prompt() -> PromptRequest {
    let block = PromptContentBlock::from_json_text(r#"{"type":"text","text":"hello"}"#)
        .expect("文本内容块");
    PromptRequest::new(vec![block])
}

/// 等连接在线（握手与 catalog 订阅由 supervisor 自己完成）。
async fn wait_online(link: &node_link_client::manager::OwnerLink) {
    wait_until(|| link.is_online(), "the link to come online").await;
}

/// 轮询一个条件（上限 5 秒）；避免用固定 sleep 与对端任务竞速。
async fn wait_until(mut condition: impl FnMut() -> bool, what: &str) {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        if condition() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

/// prompt 经 `resource.attach` 票证提交，且调用方给出的 `requestId` 原样上行（两次同一 id 都以同一 id 上行）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn prompt_routes_through_the_attachment_ticket_and_keeps_the_caller_request_id() {
    let owner = ownership();
    let mem = Arc::new(MemTransport::default());
    let (client_connection, mut io) = OwnerIo::pair();
    mem.push(client_connection);
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_for_task = Arc::clone(&seen);

    let owner_task = tokio::spawn(async move {
        let session = accept_handshake(
            &mut io,
            &owner,
            &selected_features(&["node-link.command-status.v1"]),
        )
        .await;
        let mut inbound = session.inbound_next;
        let _ = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        let snapshot = node_link_protocol::catalog::CatalogSnapshot {
            revision: decimal(1),
            agents: node_link_protocol::catalog::AgentEntryList::new(Vec::new()).expect("agents"),
            directories: node_link_protocol::catalog::DirectoryEntryList::new(Vec::new())
                .expect("directories"),
            sessions: node_link_protocol::catalog::CatalogSessionList::new(Vec::new())
                .expect("sessions"),
        };
        io.send_envelope(
            MessageType::CatalogSnapshot,
            &session.connection_id,
            &snapshot,
        )
        .await;

        // 第一条 prompt：先 attach，再提交。
        let attach = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("resource.attach");
        assert_eq!(attach.message_type(), MessageType::ResourceAttach);
        io.send_envelope(
            MessageType::ResourceAttached,
            &session.connection_id,
            &attached(),
        )
        .await;
        let first = recv_submit(&mut io, &session.connection_id, &mut inbound).await;
        assert_eq!(
            first.command,
            node_link_protocol::command::CommandName::SessionPrompt,
            "prompt 必须以 session.prompt 上行（不另造同义命令）"
        );
        assert_eq!(
            first
                .session_ref
                .as_ref()
                .map(|remote| remote.session_id.as_str()),
            Some(SESSION),
            "命令必须带远程会话引用"
        );
        assert_eq!(
            first.attachment_id.as_ref().map(|id| id.as_str()),
            Some(ATTACHMENT),
            "命令必须带本次 attach 得到的票证（Owner 据此校验会话归属）"
        );
        seen_for_task
            .lock()
            .expect("seen")
            .push(first.request_id.as_str().to_owned());
        accept_command(&mut io, &session.connection_id, &first).await;

        // 第二条 prompt：重试语义要求复用同一个 requestId；票证从缓存取（不再次 attach）。
        let second = recv_submit(&mut io, &session.connection_id, &mut inbound).await;
        assert_eq!(
            second.command,
            node_link_protocol::command::CommandName::SessionPrompt
        );
        seen_for_task
            .lock()
            .expect("seen")
            .push(second.request_id.as_str().to_owned());
        accept_command(&mut io, &session.connection_id, &second).await;
    });

    let client = NodeLinkClient::new(
        mem.factory(),
        Arc::new(FixedIdentity::new([0x61; 32])),
        Arc::new(RecordingSink::default()),
        Arc::new(RecordingDeliveries::default()),
    );
    let link = client
        .open(ClientConfig::new(
            owner_trust(&ownership()),
            "wss://owner.test/node-link/v1",
            TlsTrust::PlaintextDev,
        ))
        .expect("open");
    // 等在线（握手与 catalog 订阅由 supervisor 自己完成）。
    wait_online(&link).await;

    let session = link.session(remote(), Arc::new(RecordingDeliveries::default()));
    let stable = request("11111111-2222-4333-8444-555555555555");
    let accepted = session
        .prompt_with_request(stable.clone(), prompt(), None)
        .await
        .expect("prompt 被接受");
    assert_eq!(
        accepted.turn.as_str().len(),
        36,
        "turn 标识必须由适配器分配（形状合法，权威仍在 core）"
    );
    // 同一次重试：**同一个** requestId（Access 不得改写调用方给出的稳定标识）。
    session
        .prompt_with_request(stable.clone(), prompt(), None)
        .await
        .expect("重试被接受");

    owner_task.await.expect("owner task");
    assert_eq!(
        *seen.lock().expect("seen"),
        vec![stable.as_str().to_owned(), stable.as_str().to_owned()],
        "上行 requestId 必须逐字等于调用方给出的稳定标识"
    );
    assert_eq!(
        session.reference(),
        acp_core::model::SessionReference::Remote(remote())
    );
    link.shutdown().await;
}

/// 连接在命令受理前丢失 → 调用方得到失败；重连后**只**发 `command.status`，绝不重发 `session.prompt`。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_lost_command_is_uncertain_and_only_queried_after_reconnect() {
    let mem = Arc::new(MemTransport::default());
    let (first_connection, mut io) = OwnerIo::pair();
    mem.push(first_connection);
    let owner_for_task = ownership();
    let stable = request("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee");
    let stable_for_task = stable.clone();
    let queried: Queried = Arc::new(Mutex::new(Vec::new()));
    let queried_for_task = Arc::clone(&queried);

    let owner_task = tokio::spawn(async move {
        // 第一条连接：attach 之后收下 prompt，不发受理就断开。
        let session = accept_handshake(
            &mut io,
            &owner_for_task,
            &selected_features(&["node-link.command-status.v1"]),
        )
        .await;
        let mut inbound = session.inbound_next;
        let _ = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        let attach = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("resource.attach");
        assert_eq!(attach.message_type(), MessageType::ResourceAttach);
        io.send_envelope(
            MessageType::ResourceAttached,
            &session.connection_id,
            &attached(),
        )
        .await;
        let lost = recv_submit(&mut io, &session.connection_id, &mut inbound).await;
        assert_eq!(lost.request_id.as_str(), stable_for_task.as_str());
        io.close().await;
    });

    let client = NodeLinkClient::new(
        mem.factory(),
        Arc::new(FixedIdentity::new([0x62; 32])),
        Arc::new(RecordingSink::default()),
        Arc::new(RecordingDeliveries::default()),
    );
    let link = client
        .open(ClientConfig::new(
            owner_trust(&ownership()),
            "wss://owner.test/node-link/v1",
            TlsTrust::PlaintextDev,
        ))
        .expect("open");
    wait_online(&link).await;

    let session = link.session(remote(), Arc::new(RecordingDeliveries::default()));
    let outcome = session
        .prompt_with_request(stable.clone(), prompt(), None)
        .await;
    assert!(
        outcome.is_err(),
        "未收到受理即断线时必须失败，不能假装已接受"
    );

    owner_task.await.expect("owner task");

    // 第二条连接：只允许 `command.status`（回查），且目标就是原 requestId。
    let (second_connection, mut io) = OwnerIo::pair();
    mem.push(second_connection);
    let owner_for_task = ownership();
    let stable_for_task = stable.clone();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let (sent_tx, sent_rx) = tokio::sync::oneshot::channel();
    let owner_task = tokio::spawn(async move {
        let session = accept_handshake(
            &mut io,
            &owner_for_task,
            &selected_features(&["node-link.command-status.v1"]),
        )
        .await;
        let mut inbound = session.inbound_next;
        // 重连后 supervisor 先重订阅 catalog，再回查 uncertain 命令。
        let _ = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        let submit = recv_submit(&mut io, &session.connection_id, &mut inbound).await;
        assert_eq!(
            submit.command,
            node_link_protocol::command::CommandName::CommandStatus,
            "uncertain 的命令只能回查，不得重发副作用命令"
        );
        let target = match &submit.payload {
            node_link_protocol::command::CommandPayload::CommandStatus(body) => {
                body.target_request_id.as_str().to_owned()
            }
            other => panic!("command.status 的 payload 形状不符：{other:?}"),
        };
        assert_eq!(target, stable_for_task.as_str(), "回查必须指向原 requestId");
        queried_for_task
            .lock()
            .expect("queried")
            .push((submit.request_id.as_str().to_owned(), Some(target.clone())));
        // 终态：uncertain（崩溃窗口无法确认）——Access 不得据此自动重做。
        // 回复的 `requestId` 是**目标命令**的 id（服务端 `terminal_body` 用持久记录的 request），
        // 不是这条查询自己的 id；用错会让客户端把回复当作孤儿丢弃。
        let terminal = node_link_protocol::command::CommandTerminal {
            request_id: uuid_text(target.as_str()),
            command: node_link_protocol::command::CommandName::SessionPrompt,
            terminal: node_link_protocol::command::Terminal {
                status: TerminalStatus::Uncertain,
                terminal_at: Timestamp::parse("2026-10-20T00:00:05.000Z").expect("时间戳"),
                terminal_event_id: acpr_wire::Nullable::value(uuid_text(
                    "018f0000-0000-4000-8000-0000000000ff",
                )),
                result: acpr_wire::Nullable::null(),
                error: acpr_wire::Nullable::value(uncertain_error()),
            },
        };
        io.send_envelope(
            MessageType::CommandTerminal,
            &session.connection_id,
            &terminal,
        )
        .await;
        // 保持连接存活到主线程完成断言：否则关闭 socket 会让 link 离线，掩盖「终态是否被关联」。
        sent_tx.send(()).expect("terminal sent");
        stop_rx.await.expect("stop");
    });

    sent_rx.await.expect("terminal sent");
    wait_until(
        || link.shared().uncertain().is_empty(),
        "the terminal reply to clear the uncertain command",
    )
    .await;
    assert_eq!(mem.opens(), 2, "断线后必须重连一次（且只一次）");
    assert!(link.is_online(), "终态回复不应关闭连接");
    assert!(
        link.shared().uncertain().is_empty(),
        "目标 requestId 的终态必须结清不确定项（不能按查询 id 丢弃回复）：{:?}",
        link.shared().uncertain()
    );
    {
        let queried = queried.lock().expect("queried");
        assert_eq!(queried.len(), 1, "uncertain 命令只回查一次");
        assert_eq!(queried[0].1.as_deref(), Some(stable.as_str()));
    }
    stop_tx.send(()).expect("stop owner");
    owner_task.await.expect("owner task");
    link.shutdown().await;
}

/// 第一轮连接：attach 后收下 prompt，不发任何回复就断开，让客户端把该命令登记为 uncertain。
async fn lose_the_prompt_on_the_first_connection(
    mem: &Arc<MemTransport>,
    stable: &RequestId,
) -> tokio::task::JoinHandle<()> {
    let (connection, mut io) = OwnerIo::pair();
    mem.push(connection);
    let owner = ownership();
    let stable = stable.clone();
    tokio::spawn(async move {
        let session = accept_handshake(
            &mut io,
            &owner,
            &selected_features(&["node-link.command-status.v1"]),
        )
        .await;
        let mut inbound = session.inbound_next;
        let _ = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        let attach = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("resource.attach");
        assert_eq!(attach.message_type(), MessageType::ResourceAttach);
        io.send_envelope(
            MessageType::ResourceAttached,
            &session.connection_id,
            &attached(),
        )
        .await;
        let lost = recv_submit(&mut io, &session.connection_id, &mut inbound).await;
        assert_eq!(lost.request_id.as_str(), stable.as_str());
        io.close().await;
    })
}

/// 无 `correlationId` 的请求级 `link.error`（旧 Owner 的 `resource.rate_limited` 形状）：唯一
/// 待处理请求必须被立即结清——调用方快速失败、不登记 uncertain、连接继续可用。修复前它会一直等到
/// `COMMAND_WAIT_TIMEOUT`（120 s），并把已明确拒绝的 mutation 标成 uncertain。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_uncorrelated_rate_limit_error_fails_the_only_pending_command_without_uncertain() {
    let mem = Arc::new(MemTransport::default());
    let (connection, mut io) = OwnerIo::pair();
    mem.push(connection);
    let owner = ownership();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let owner_task = tokio::spawn(async move {
        let session = accept_handshake(
            &mut io,
            &owner,
            &selected_features(&["node-link.command-status.v1"]),
        )
        .await;
        let mut inbound = session.inbound_next;
        let _ = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        send_catalog_snapshot(&mut io, &session.connection_id).await;
        let attach = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("resource.attach");
        assert_eq!(attach.message_type(), MessageType::ResourceAttach);
        reply_attached(&mut io, &session.connection_id).await;
        let rejected = recv_submit(&mut io, &session.connection_id, &mut inbound).await;
        assert_eq!(
            rejected.command,
            node_link_protocol::command::CommandName::SessionPrompt
        );
        // admission 拒绝：`correlationId` 为 null（协议允许「无关联请求」）。
        let body = node_link_protocol::error::Body::new(
            node_link_protocol::error::ErrorCode::ResourceRateLimited,
            "the command rate limit was exceeded",
            true,
        )
        .expect("error body");
        io.send_envelope(MessageType::LinkError, &session.connection_id, &body)
            .await;
        // 连接保持可用：下一条 prompt 仍然能被受理（没有孤立 pending 占位）。
        let next = recv_submit(&mut io, &session.connection_id, &mut inbound).await;
        accept_command(&mut io, &session.connection_id, &next).await;
        stop_rx.await.expect("stop");
    });

    let link = open_client(&mem, [0x65; 32]);
    wait_online(&link).await;
    let session = link.session(remote(), Arc::new(RecordingDeliveries::default()));
    let rejected = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        session.prompt_with_request(
            request("dddddddd-eeee-4fff-8000-111111111111"),
            prompt(),
            None,
        ),
    )
    .await
    .expect("无 correlationId 的 rate_limited 必须立即结清，而不是等 120 s");
    assert!(rejected.is_err(), "被限流的命令必须显式失败");
    assert!(
        link.shared().uncertain().is_empty(),
        "admission 拒绝明确未执行，不得登记 uncertain：{:?}",
        link.shared().uncertain()
    );
    assert!(link.is_online(), "请求级错误不得关闭连接");
    assert_eq!(mem.opens(), 1, "请求级错误不得触发重连");

    session
        .prompt_with_request(
            request("dddddddd-eeee-4fff-8000-222222222222"),
            prompt(),
            None,
        )
        .await
        .expect("限流结清后连接必须继续可用（pending 表没有泄漏）");
    stop_tx.send(()).expect("stop owner");
    owner_task.await.expect("owner task");
    link.shutdown().await;
}

/// `correlationId` 指到被拒绝的那条命令（`resource.rate_limited` 的 in-flight/速率拒绝）时，只结清
/// 它：另一条已受理的 mutation 正常返回，被拒命令快速失败且不产生 uncertain。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_in_flight_limit_rejection_settles_only_the_rejected_command() {
    let mem = Arc::new(MemTransport::default());
    let (connection, mut io) = OwnerIo::pair();
    mem.push(connection);
    let owner = ownership();
    let (first_seen_tx, first_seen_rx) = tokio::sync::oneshot::channel();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let owner_task = tokio::spawn(async move {
        let session = accept_handshake(
            &mut io,
            &owner,
            &selected_features(&["node-link.command-status.v1"]),
        )
        .await;
        let mut inbound = session.inbound_next;
        let _ = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        send_catalog_snapshot(&mut io, &session.connection_id).await;

        // 第一条 prompt 申请票证；票证随后在共享缓存里复用，第二条 prompt 不再 attach。
        let attach = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("resource.attach");
        assert_eq!(attach.message_type(), MessageType::ResourceAttach);
        reply_attached(&mut io, &session.connection_id).await;
        let accepted = recv_submit(&mut io, &session.connection_id, &mut inbound).await;
        first_seen_tx.send(()).expect("first submit seen");

        let rejected = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("command.submit");
        assert_eq!(rejected.message_type(), MessageType::CommandSubmit);
        let _rejected_body: CommandSubmit =
            serde_json::from_str(rejected.body().get()).expect("command.submit body");

        // in-flight 上限拒绝：关联到被拒帧的 `messageId`（Owner 侧记录的逻辑）。
        let mut body = node_link_protocol::error::Body::new(
            node_link_protocol::error::ErrorCode::ResourceRateLimited,
            "the in-flight command limit was reached",
            true,
        )
        .expect("error body");
        body.correlation_id = acpr_wire::Nullable::value(rejected.message_id().clone());
        io.send_envelope(MessageType::LinkError, &session.connection_id, &body)
            .await;
        // 已受理的那条正常返回 accepted。
        accept_command(&mut io, &session.connection_id, &accepted).await;
        stop_rx.await.expect("stop");
    });

    let link = open_client(&mem, [0x66; 32]);
    wait_online(&link).await;
    let first_session = link.session(remote(), Arc::new(RecordingDeliveries::default()));
    let second_session = link.session(remote(), Arc::new(RecordingDeliveries::default()));
    let accepted = tokio::spawn(async move {
        first_session
            .prompt_with_request(
                request("eeeeeeee-ffff-4000-8111-111111111111"),
                prompt(),
                None,
            )
            .await
    });
    first_seen_rx.await.expect("first submit");
    let rejected = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        second_session.prompt_with_request(
            request("eeeeeeee-ffff-4000-8111-222222222222"),
            prompt(),
            None,
        ),
    )
    .await
    .expect("被关联的 rate_limited 必须立即结清");
    assert!(rejected.is_err(), "被 in-flight 上限拒绝的命令必须显式失败");
    let accepted = accepted.await.expect("first prompt task");
    assert!(accepted.is_ok(), "已受理的命令不得被无关的错误结清");
    assert!(
        link.shared().uncertain().is_empty(),
        "admission 拒绝不得登记 uncertain：{:?}",
        link.shared().uncertain()
    );
    assert!(link.is_online(), "请求级错误不得关闭连接");
    assert_eq!(mem.opens(), 1, "请求级错误不得触发重连");
    stop_tx.send(()).expect("stop owner");
    owner_task.await.expect("owner task");
    link.shutdown().await;
}

/// 打开一个到固定 Owner 的客户端（supervisor 由本函数启动）。
fn open_client(mem: &Arc<MemTransport>, seed: [u8; 32]) -> Arc<node_link_client::OwnerLink> {
    let client = NodeLinkClient::new(
        mem.factory(),
        Arc::new(FixedIdentity::new(seed)),
        Arc::new(RecordingSink::default()),
        Arc::new(RecordingDeliveries::default()),
    );
    client
        .open(ClientConfig::new(
            owner_trust(&ownership()),
            "wss://owner.test/node-link/v1",
            TlsTrust::PlaintextDev,
        ))
        .expect("open")
}

/// `command.accepted`（针对目标命令的回复）必须结清查询本身，但**保留**不确定项直到终态。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_status_query_accepted_reply_is_correlated_and_keeps_the_command_uncertain() {
    let mem = Arc::new(MemTransport::default());
    let stable = request("bbbbbbbb-cccc-4ddd-8eee-ffffffffffff");
    let first = lose_the_prompt_on_the_first_connection(&mem, &stable).await;
    let link = open_client(&mem, [0x63; 32]);
    wait_online(&link).await;

    let session = link.session(remote(), Arc::new(RecordingDeliveries::default()));
    assert!(
        session
            .prompt_with_request(stable.clone(), prompt(), None)
            .await
            .is_err(),
        "未收到受理即断线的命令必须失败"
    );
    first.await.expect("first owner task");

    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let (go_tx, go_rx) = tokio::sync::oneshot::channel();
    let (second_connection, mut io) = OwnerIo::pair();
    mem.push(second_connection);
    let owner = ownership();
    let stable_for_task = stable.clone();
    let owner_task = tokio::spawn(async move {
        let session = accept_handshake(
            &mut io,
            &owner,
            &selected_features(&["node-link.command-status.v1"]),
        )
        .await;
        let mut inbound = session.inbound_next;
        let _ = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        let submit = recv_submit(&mut io, &session.connection_id, &mut inbound).await;
        assert_eq!(
            submit.command,
            node_link_protocol::command::CommandName::CommandStatus
        );
        let target = match &submit.payload {
            node_link_protocol::command::CommandPayload::CommandStatus(body) => {
                body.target_request_id.clone()
            }
            other => panic!("command.status 的 payload 形状不符：{other:?}"),
        };
        assert_eq!(target.as_str(), stable_for_task.as_str());
        // 受理态回复使用**目标** requestId 与目标命令名（服务端 `send_accepted` 的口径）。
        let accepted = node_link_protocol::command::CommandAccepted {
            request_id: target.clone(),
            command: node_link_protocol::command::CommandName::SessionPrompt,
            accepted_at: Timestamp::parse("2026-10-20T00:00:01.000Z").expect("时间戳"),
            result: acpr_wire::Nullable::null(),
        };
        io.send_envelope(
            MessageType::CommandAccepted,
            &session.connection_id,
            &accepted,
        )
        .await;
        ready_tx.send(()).expect("ready");
        go_rx.await.expect("terminal go");

        let terminal = node_link_protocol::command::CommandTerminal {
            request_id: target,
            command: node_link_protocol::command::CommandName::SessionPrompt,
            terminal: node_link_protocol::command::Terminal {
                status: TerminalStatus::Uncertain,
                terminal_at: Timestamp::parse("2026-10-20T00:00:05.000Z").expect("时间戳"),
                terminal_event_id: acpr_wire::Nullable::null(),
                result: acpr_wire::Nullable::null(),
                error: acpr_wire::Nullable::value(uncertain_error()),
            },
        };
        io.send_envelope(
            MessageType::CommandTerminal,
            &session.connection_id,
            &terminal,
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    });

    ready_rx.await.expect("accepted reply sent");
    // 受理只是中间态：不确定项仍在（只有终态能证明结果）。
    assert_eq!(
        link.shared().uncertain().len(),
        1,
        "command.accepted 不得清除不确定项"
    );
    assert!(link.is_online(), "受理回复不应关闭连接");
    go_tx.send(()).expect("send terminal go");
    owner_task.await.expect("owner task");
    wait_until(
        || link.shared().uncertain().is_empty(),
        "the terminal reply to clear the uncertain command",
    )
    .await;
    assert!(
        link.shared().uncertain().is_empty(),
        "终态（即使 status=uncertain）必须结清查询"
    );
    assert_eq!(mem.opens(), 2, "不得因受理/终态回复重连");
    link.shutdown().await;
}

/// 可恢复的 `link.error(command.not_found)` 只结清被 `correlationId` 指到的恢复查询，不关闭连接。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_command_not_found_link_error_clears_the_uncertain_command_without_reconnecting() {
    let mem = Arc::new(MemTransport::default());
    let stable = request("cccccccc-dddd-4eee-8fff-000000000000");
    let first = lose_the_prompt_on_the_first_connection(&mem, &stable).await;
    let link = open_client(&mem, [0x64; 32]);
    wait_online(&link).await;
    let session = link.session(remote(), Arc::new(RecordingDeliveries::default()));
    assert!(
        session
            .prompt_with_request(stable.clone(), prompt(), None)
            .await
            .is_err()
    );
    first.await.expect("first owner task");

    let stable_for_task = stable.clone();
    let (second_connection, mut io) = OwnerIo::pair();
    mem.push(second_connection);
    let owner = ownership();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let (sent_tx, sent_rx) = tokio::sync::oneshot::channel();
    let owner_task = tokio::spawn(async move {
        let session = accept_handshake(
            &mut io,
            &owner,
            &selected_features(&["node-link.command-status.v1"]),
        )
        .await;
        let mut inbound = session.inbound_next;
        let _ = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        let envelope = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("command.status");
        let submit: CommandSubmit =
            serde_json::from_str(envelope.body().get()).expect("command.submit body");
        assert_eq!(
            submit.command,
            node_link_protocol::command::CommandName::CommandStatus
        );
        let target = match &submit.payload {
            node_link_protocol::command::CommandPayload::CommandStatus(body) => {
                body.target_request_id.as_str().to_owned()
            }
            other => panic!("command.status 的 payload 形状不符：{other:?}"),
        };
        assert_eq!(target, stable_for_task.as_str());
        // 服务端 `on_status` 的 unknown-request 分支：`link.error(command.not_found)`，
        // `correlationId` 是**查询信封**的 messageId（不是目标 requestId），连接保持可用。
        let mut body = node_link_protocol::error::Body::new(
            node_link_protocol::error::ErrorCode::CommandNotFound,
            "the request is not known to this owner",
            false,
        )
        .expect("error body");
        body.correlation_id = acpr_wire::Nullable::value(envelope.message_id().clone());
        io.send_envelope(MessageType::LinkError, &session.connection_id, &body)
            .await;
        // 保持连接存活到主线程完成断言：否则任务结束关闭 socket 会让 link 离线，掩盖「错误是否致命」。
        sent_tx.send(()).expect("error sent");
        stop_rx.await.expect("stop");
    });

    sent_rx.await.expect("error sent");
    wait_until(
        || link.shared().uncertain().is_empty(),
        "command.not_found to clear the uncertain command",
    )
    .await;
    assert!(link.is_online(), "可恢复的 link.error 不得关闭连接");
    assert!(
        link.shared().uncertain().is_empty(),
        "not_found 结清对应不确定项：{:?}",
        link.shared().uncertain()
    );
    assert_eq!(mem.opens(), 2, "可恢复错误不得触发重连");
    stop_tx.send(()).expect("stop owner");
    owner_task.await.expect("owner task");
    link.shutdown().await;
}

/// 公开的 `RemoteSession::status` 也必须按目标 `requestId` 关联：Owner 回 `command.accepted`
/// （未终结的命令）时调用方立即拿到 `Accepted`，而不是等 `COMMAND_WAIT_TIMEOUT`。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_public_status_query_resolves_on_command_accepted() {
    let mem = Arc::new(MemTransport::default());
    let (connection, mut io) = OwnerIo::pair();
    mem.push(connection);
    let owner = ownership();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let owner_task = tokio::spawn(async move {
        let session = accept_handshake(
            &mut io,
            &owner,
            &selected_features(&["node-link.command-status.v1"]),
        )
        .await;
        let mut inbound = session.inbound_next;
        let _ = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        let submit = recv_submit(&mut io, &session.connection_id, &mut inbound).await;
        assert_eq!(
            submit.command,
            node_link_protocol::command::CommandName::CommandStatus
        );
        let target = match &submit.payload {
            node_link_protocol::command::CommandPayload::CommandStatus(body) => {
                body.target_request_id.clone()
            }
            other => panic!("command.status 的 payload 形状不符：{other:?}"),
        };
        let accepted = node_link_protocol::command::CommandAccepted {
            request_id: target,
            command: node_link_protocol::command::CommandName::SessionPrompt,
            accepted_at: Timestamp::parse("2026-10-20T00:00:01.000Z").expect("时间戳"),
            result: acpr_wire::Nullable::null(),
        };
        io.send_envelope(
            MessageType::CommandAccepted,
            &session.connection_id,
            &accepted,
        )
        .await;
        stop_rx.await.expect("stop");
    });

    let link = open_client(&mem, [0x65; 32]);
    wait_online(&link).await;
    let session = link.session(remote(), Arc::new(RecordingDeliveries::default()));
    let target = request("dddddddd-eeee-4fff-8000-111111111111");
    let started = std::time::Instant::now();
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        session.status(
            request("eeeeeeee-ffff-4000-8111-222222222222"),
            target.clone(),
        ),
    )
    .await
    .expect("status must resolve on command.accepted, not time out")
    .expect("the owner accepted the query");
    assert!(
        matches!(
            outcome,
            node_link_client::link::CommandOutcome::Accepted { .. }
        ),
        "got {outcome:?}"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    stop_tx.send(()).expect("stop owner");
    owner_task.await.expect("owner task");
    link.shutdown().await;
}

/// `resource.attach` 的请求级拒绝带 `correlationId`（Owner 的 `scope_denied`/`not_found`/旧 epoch
/// 形状是 `protocol.sequence_invalid`）：必须立即结清 attach，而不是等满 120 s，也不得把连接当作
/// 不可用（那会触发 supervisor 重连并重试同一个会话）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_correlated_attach_rejection_settles_without_closing_the_connection() {
    let mem = Arc::new(MemTransport::default());
    let (connection, mut io) = OwnerIo::pair();
    mem.push(connection);
    let owner = ownership();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let owner_task = tokio::spawn(async move {
        let session = accept_handshake(
            &mut io,
            &owner,
            &selected_features(&["node-link.command-status.v1"]),
        )
        .await;
        let mut inbound = session.inbound_next;
        let _ = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        send_catalog_snapshot(&mut io, &session.connection_id).await;
        let attach = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("resource.attach");
        assert_eq!(attach.message_type(), MessageType::ResourceAttach);
        // 服务端 `protocol_error`/`attach_fault` 的拒绝形状：请求级 + 不可重试 + 关联请求信封。
        let mut body = node_link_protocol::error::Body::new(
            node_link_protocol::error::ErrorCode::ProtocolSequenceInvalid,
            "the session is not visible to this access node",
            false,
        )
        .expect("error body");
        body.correlation_id = acpr_wire::Nullable::value(attach.message_id().clone());
        io.send_envelope(MessageType::LinkError, &session.connection_id, &body)
            .await;
        stop_rx.await.expect("stop");
    });

    let link = open_client(&mem, [0x67; 32]);
    wait_online(&link).await;
    let wire = node_link_protocol::common::RemoteSessionRef {
        owner_node_id: uuid_text(OWNER_NODE),
        session_id: uuid_text(SESSION),
    };
    let started = std::time::Instant::now();
    let rejected = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        link.shared().attach(wire.clone()),
    )
    .await
    .expect("带 correlationId 的 attach 拒绝必须立即结清，而不是等 120 s");
    assert!(rejected.is_err(), "被拒绝的 attach 必须显式失败");
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert!(link.is_online(), "请求级拒绝不得关闭连接");
    assert_eq!(mem.opens(), 1, "请求级拒绝不得触发重连");
    assert!(
        link.shared().subscription_rejected(&wire),
        "不可重试的 attach 拒绝必须把该会话标记为不可恢复，避免重连时重复 attach"
    );
    stop_tx.send(()).expect("stop owner");
    owner_task.await.expect("owner task");
    link.shutdown().await;
}

/// `resource.subscribe` 的关联拒绝不再静默：会话进入 `subscription_error`，后续订阅立即失败，
/// 连接保持可用（`protocol.sequence_invalid` 不是连接致命错误）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_correlated_subscribe_rejection_is_reported_and_keeps_the_connection() {
    let mem = Arc::new(MemTransport::default());
    let (connection, mut io) = OwnerIo::pair();
    mem.push(connection);
    let owner = ownership();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let owner_task = tokio::spawn(async move {
        let session = accept_handshake(
            &mut io,
            &owner,
            &selected_features(&["node-link.command-status.v1"]),
        )
        .await;
        let mut inbound = session.inbound_next;
        let _ = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        send_catalog_snapshot(&mut io, &session.connection_id).await;
        let attach = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("resource.attach");
        assert_eq!(attach.message_type(), MessageType::ResourceAttach);
        reply_attached(&mut io, &session.connection_id).await;
        let subscribe = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("resource.subscribe");
        assert_eq!(subscribe.message_type(), MessageType::ResourceSubscribe);
        let mut body = node_link_protocol::error::Body::new(
            node_link_protocol::error::ErrorCode::ProtocolSequenceInvalid,
            "the cursor is not usable for this session on this connection",
            false,
        )
        .expect("error body");
        body.correlation_id = acpr_wire::Nullable::value(subscribe.message_id().clone());
        io.send_envelope(MessageType::LinkError, &session.connection_id, &body)
            .await;
        stop_rx.await.expect("stop");
    });

    let link = open_client(&mem, [0x68; 32]);
    wait_online(&link).await;
    let wire = node_link_protocol::common::RemoteSessionRef {
        owner_node_id: uuid_text(OWNER_NODE),
        session_id: uuid_text(SESSION),
    };
    let ticket = link
        .shared()
        .attach(wire.clone())
        .await
        .expect("attachment ticket");
    link.shared()
        .subscribe(&ticket, None)
        .await
        .expect("the subscribe frame is handed to the transport");
    wait_until(
        || link.shared().subscription_error(&wire).is_some(),
        "the subscribe rejection to be reported",
    )
    .await;
    let error = link
        .shared()
        .subscription_error(&wire)
        .expect("rejection reason");
    assert!(
        format!("{error}").contains("nodelink.protocol.sequence_invalid"),
        "the rejection must carry the owner's code, got {error}"
    );
    assert!(link.is_online(), "请求级拒绝不得关闭连接");
    assert_eq!(mem.opens(), 1, "请求级拒绝不得触发重连");
    assert!(
        link.shared().subscribe(&ticket, None).await.is_err(),
        "a rejected session must fail fast instead of queueing another doomed subscribe"
    );
    stop_tx.send(()).expect("stop owner");
    owner_task.await.expect("owner task");
    link.shutdown().await;
}

/// `resource.ack` 的关联拒绝（旧 epoch/回退 cursor 的 `protocol.sequence_invalid`）同样被结清：
/// 会话进入 `subscription_error`，连接保持可用，恢复计划不再重试该持久化订阅。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_correlated_ack_rejection_settles_and_marks_the_subscription() {
    let mem = Arc::new(MemTransport::default());
    let (connection, mut io) = OwnerIo::pair();
    mem.push(connection);
    let owner = ownership();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let owner_task = tokio::spawn(async move {
        let session = accept_handshake(
            &mut io,
            &owner,
            &selected_features(&["node-link.command-status.v1"]),
        )
        .await;
        let mut inbound = session.inbound_next;
        let _ = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        send_catalog_snapshot(&mut io, &session.connection_id).await;
        let attach = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("resource.attach");
        assert_eq!(attach.message_type(), MessageType::ResourceAttach);
        reply_attached(&mut io, &session.connection_id).await;
        let ack = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("resource.ack");
        assert_eq!(ack.message_type(), MessageType::ResourceAck);
        // Owner 的 `on_ack` 拒绝形状：epoch 不符/回退一律 `sequence_invalid`，请求级且不可重试。
        let mut body = node_link_protocol::error::Body::new(
            node_link_protocol::error::ErrorCode::ProtocolSequenceInvalid,
            "the cursor is not usable for this session on this connection",
            false,
        )
        .expect("error body");
        body.correlation_id = acpr_wire::Nullable::value(ack.message_id().clone());
        io.send_envelope(MessageType::LinkError, &session.connection_id, &body)
            .await;
        stop_rx.await.expect("stop");
    });

    let link = open_client(&mem, [0x69; 32]);
    wait_online(&link).await;
    let wire = node_link_protocol::common::RemoteSessionRef {
        owner_node_id: uuid_text(OWNER_NODE),
        session_id: uuid_text(SESSION),
    };
    let _ticket = link
        .shared()
        .attach(wire.clone())
        .await
        .expect("attachment ticket");
    link.shared()
        .ack(
            wire.clone(),
            node_link_protocol::common::OriginCursor {
                origin_epoch: uuid_text("018f0000-0000-4000-8000-0000000000ee"),
                origin_sequence: decimal(3),
            },
        )
        .await
        .expect("the ack frame is handed to the transport");
    wait_until(
        || link.shared().subscription_error(&wire).is_some(),
        "the ack rejection to be reported",
    )
    .await;
    assert!(link.is_online(), "请求级拒绝不得关闭连接");
    assert_eq!(mem.opens(), 1, "请求级拒绝不得触发重连");
    stop_tx.send(()).expect("stop owner");
    owner_task.await.expect("owner task");
    link.shutdown().await;
}

/// 连续 origin 序号的 `resource.event` wire body（`payloadDigest` 只需是 32 字节 base64url）。
fn event_body(sequence: u64, event_id: &str) -> serde_json::Value {
    serde_json::json!({
        "originEventId": event_id,
        "originEpoch": "018f0000-0000-4000-8000-0000000000ee",
        "originSequence": sequence.to_string(),
        "sessionRef": {
            "ownerNodeId": OWNER_NODE,
            "sessionId": SESSION,
        },
        "eventType": "session.updated",
        "payloadDigest": support::base64url(&[0x2a; 32]),
        "createdAt": "2026-10-20T00:00:00.000Z",
        "payload": {"view": {}},
    })
}

/// 链路离线时 `subscribe` 必须立即失败：离线期间发件箱无人消费，无超时的等待会让调用方永久挂起。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn subscribing_while_the_link_is_offline_fails_instead_of_hanging() {
    let mem = Arc::new(MemTransport::default());
    let (connection, mut io) = OwnerIo::pair();
    mem.push(connection);
    let owner = ownership();
    let owner_task = tokio::spawn(async move {
        let session = accept_handshake(
            &mut io,
            &owner,
            &selected_features(&["node-link.command-status.v1"]),
        )
        .await;
        let mut inbound = session.inbound_next;
        let _ = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        send_catalog_snapshot(&mut io, &session.connection_id).await;
        let attach = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("resource.attach");
        assert_eq!(attach.message_type(), MessageType::ResourceAttach);
        reply_attached(&mut io, &session.connection_id).await;
        // 断开连接：supervisor 进入重连，但脚本化传输里没有第二条连接，因此保持离线。
        io.close().await;
    });

    let link = open_client(&mem, [0x6c; 32]);
    wait_online(&link).await;
    let wire = node_link_protocol::common::RemoteSessionRef {
        owner_node_id: uuid_text(OWNER_NODE),
        session_id: uuid_text(SESSION),
    };
    let ticket = link
        .shared()
        .attach(wire)
        .await
        .expect("attachment ticket before the owner disconnects");
    owner_task.await.expect("owner task");
    wait_until(|| !link.is_online(), "the link to go offline").await;

    let started = std::time::Instant::now();
    let error = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        link.shared().subscribe(&ticket, None),
    )
    .await
    .expect("离线订阅必须立即失败，而不是无超时等待")
    .expect_err("离线订阅不得成功");
    assert!(
        matches!(error, ClientError::OwnerOffline),
        "离线订阅必须是明确的不可用错误，got {error:?}"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    link.shutdown().await;
}

/// 可重试的 `resource.subscribe` 拒绝（`internal.unavailable`）必须：可观测、按**同一 cursor**
/// 自动重发、在收到该会话的入站帧之前不当作已建立；重发成功后事件连续到达。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retryable_subscribe_rejection_is_observable_and_resubscribed_from_the_same_cursor() {
    let mem = Arc::new(MemTransport::default());
    let (connection, mut io) = OwnerIo::pair();
    mem.push(connection);
    let owner = ownership();
    let trust = owner_trust(&owner);
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let owner_task = tokio::spawn(async move {
        let session = accept_handshake(
            &mut io,
            &owner,
            &selected_features(&["node-link.command-status.v1"]),
        )
        .await;
        let mut inbound = session.inbound_next;
        let _ = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        send_catalog_snapshot(&mut io, &session.connection_id).await;
        let attach = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("resource.attach");
        assert_eq!(attach.message_type(), MessageType::ResourceAttach);
        reply_attached(&mut io, &session.connection_id).await;

        let first = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("resource.subscribe");
        assert_eq!(first.message_type(), MessageType::ResourceSubscribe);
        let first_body: node_link_protocol::resource::Subscribe =
            serde_json::from_str(first.body().get()).expect("subscribe body");
        // Owner 的内部故障形状：请求级 + 可重试 + 关联请求信封。
        let mut body = node_link_protocol::error::Body::new(
            node_link_protocol::error::ErrorCode::InternalUnavailable,
            "the owner node cannot serve the resource request",
            true,
        )
        .expect("error body");
        body.correlation_id = acpr_wire::Nullable::value(first.message_id().clone());
        io.send_envelope(MessageType::LinkError, &session.connection_id, &body)
            .await;

        let second = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("the retried resource.subscribe");
        assert_eq!(second.message_type(), MessageType::ResourceSubscribe);
        let second_body: node_link_protocol::resource::Subscribe =
            serde_json::from_str(second.body().get()).expect("subscribe body");
        assert_eq!(
            second_body.cursor, first_body.cursor,
            "重试必须从同一 cursor 续读，不能跳过未补齐的位置"
        );
        assert_eq!(
            second_body.attachment_id.as_str(),
            first_body.attachment_id.as_str(),
            "重试复用同一条 attachment（没有 stale 拒绝）"
        );

        // 补齐缺口：cursor=7 之后的事件是 8、9。
        for (sequence, event_id) in [
            (8u64, "018f0000-0000-4000-8000-000000000108"),
            (9u64, "018f0000-0000-4000-8000-000000000109"),
        ] {
            let event = event_body(sequence, event_id);
            io.send_envelope(MessageType::ResourceEvent, &session.connection_id, &event)
                .await;
        }
        stop_rx.await.expect("stop");
    });

    let sink: Arc<RecordingSink> = Arc::new(RecordingSink::default());
    let client = NodeLinkClient::new(
        mem.factory(),
        Arc::new(FixedIdentity::new([0x6b; 32])),
        sink.clone(),
        Arc::new(RecordingDeliveries::default()),
    );
    let link = client
        .open(ClientConfig::new(
            trust,
            "wss://owner.test/node-link/v1",
            TlsTrust::PlaintextDev,
        ))
        .expect("open");
    wait_online(&link).await;
    let wire = node_link_protocol::common::RemoteSessionRef {
        owner_node_id: uuid_text(OWNER_NODE),
        session_id: uuid_text(SESSION),
    };
    let ticket = link
        .shared()
        .attach(wire.clone())
        .await
        .expect("attachment ticket");
    let cursor = node_link_protocol::common::OriginCursor {
        origin_epoch: uuid_text("018f0000-0000-4000-8000-0000000000ee"),
        origin_sequence: decimal(7),
    };
    link.shared()
        .subscribe(&ticket, Some(cursor))
        .await
        .expect("the subscribe frame is handed to the transport");

    wait_until(
        || link.shared().subscription_error(&wire).is_some(),
        "the retryable rejection to be reported",
    )
    .await;
    let error = link
        .shared()
        .subscription_error(&wire)
        .expect("rejection reason");
    assert!(
        format!("{error}").contains("nodelink.internal.unavailable"),
        "the fault must carry the owner's code, got {error}"
    );
    assert!(
        !link.shared().subscription_rejected(&wire),
        "a retryable rejection is not a permanent conclusion"
    );

    wait_until(
        || sink.events.lock().expect("events").len() == 2,
        "both events after the resubscribe",
    )
    .await;
    let ids: Vec<String> = sink
        .events
        .lock()
        .expect("events")
        .iter()
        .map(|(_, id)| id.clone())
        .collect();
    assert_eq!(
        ids,
        vec![
            "018f0000-0000-4000-8000-000000000108".to_owned(),
            "018f0000-0000-4000-8000-000000000109".to_owned(),
        ],
        "补齐后事件必须连续、保序到达"
    );
    // 实时事件只证明扇出在工作，不代表被拒绝的重放已经补齐；故障必须保持可观测，
    // 直到显式重新订阅或重连恢复（不能在退避窗口内被一条事件清掉）。
    assert!(
        link.shared().subscription_error(&wire).is_some(),
        "实时事件不得清除待补齐 gap 的故障记录"
    );
    assert!(link.is_online(), "可重试拒绝不得关闭连接");
    assert_eq!(mem.opens(), 1, "可重试拒绝不得触发重连");
    stop_tx.send(()).expect("stop owner");
    owner_task.await.expect("owner task");
    link.shutdown().await;
}

/// 可重试拒绝的重发有界：预算耗尽后本连接不再重发（不会形成请求风暴），错误保持可观测，
/// 且该会话**不**被标记成永久拒绝（下一次重连的恢复计划仍会尝试）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retryable_subscribe_rejections_stop_after_the_bounded_budget() {
    let mem = Arc::new(MemTransport::default());
    let (connection, mut io) = OwnerIo::pair();
    mem.push(connection);
    let owner = ownership();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let owner_task = tokio::spawn(async move {
        let session = accept_handshake(
            &mut io,
            &owner,
            &selected_features(&["node-link.command-status.v1"]),
        )
        .await;
        let mut inbound = session.inbound_next;
        let _ = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        send_catalog_snapshot(&mut io, &session.connection_id).await;
        let attach = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("resource.attach");
        assert_eq!(attach.message_type(), MessageType::ResourceAttach);
        reply_attached(&mut io, &session.connection_id).await;

        // 初始订阅 + 4 次有界重发；之后必须安静下来。
        let mut seen = 0usize;
        while let Ok(Some(envelope)) = tokio::time::timeout(
            std::time::Duration::from_millis(1_500),
            io.recv_envelope(&session.connection_id, &mut inbound),
        )
        .await
        {
            assert_eq!(envelope.message_type(), MessageType::ResourceSubscribe);
            seen += 1;
            assert!(
                seen <= 5,
                "a retryable rejection must not turn into a request storm"
            );
            let mut body = node_link_protocol::error::Body::new(
                node_link_protocol::error::ErrorCode::InternalUnavailable,
                "the owner node cannot serve the resource request",
                true,
            )
            .expect("error body");
            body.correlation_id = acpr_wire::Nullable::value(envelope.message_id().clone());
            io.send_envelope(MessageType::LinkError, &session.connection_id, &body)
                .await;
        }
        assert_eq!(seen, 5, "初始订阅 + 4 次有界重发后不再重发");
        stop_rx.await.expect("stop");
    });

    let link = open_client(&mem, [0x6d; 32]);
    wait_online(&link).await;
    let wire = node_link_protocol::common::RemoteSessionRef {
        owner_node_id: uuid_text(OWNER_NODE),
        session_id: uuid_text(SESSION),
    };
    let ticket = link
        .shared()
        .attach(wire.clone())
        .await
        .expect("attachment ticket");
    link.shared()
        .subscribe(&ticket, None)
        .await
        .expect("the subscribe frame is handed to the transport");
    wait_until(
        || {
            link.shared()
                .subscription_fault(&wire)
                .is_some_and(|fault| !fault.retrying)
        },
        "the retry budget to be exhausted",
    )
    .await;
    let fault = link.shared().subscription_fault(&wire).expect("fault");
    assert_eq!(fault.attempts, 5, "4 次重发后预算耗尽");
    assert!(
        !link.shared().subscription_rejected(&wire),
        "预算耗尽不是永久拒绝：下一次重连的恢复计划仍会尝试"
    );
    assert!(link.shared().subscription_error(&wire).is_some());
    assert!(link.is_online(), "可重试拒绝不得关闭连接");
    assert_eq!(mem.opens(), 1, "可重试拒绝不得触发重连");
    stop_tx.send(()).expect("stop owner");
    owner_task.await.expect("owner task");
    link.shutdown().await;
}

/// 可重试的 `attach_generation_stale` 订阅拒绝：过期票证被丢弃，客户端先重新 attach，再用**同一
/// cursor** 订阅；新代际上的事件正常到达（缺口补齐，而不是停在一个必然被拒的票证上）。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stale_generation_subscribe_rejection_re_attaches_before_resubscribing() {
    let mem = Arc::new(MemTransport::default());
    let (connection, mut io) = OwnerIo::pair();
    mem.push(connection);
    let owner = ownership();
    let trust = owner_trust(&owner);
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let owner_task = tokio::spawn(async move {
        let session = accept_handshake(
            &mut io,
            &owner,
            &selected_features(&["node-link.command-status.v1"]),
        )
        .await;
        let mut inbound = session.inbound_next;
        let _ = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("catalog.subscribe");
        send_catalog_snapshot(&mut io, &session.connection_id).await;
        let attach = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("resource.attach");
        assert_eq!(attach.message_type(), MessageType::ResourceAttach);
        reply_attached(&mut io, &session.connection_id).await;

        let first = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("resource.subscribe");
        let first_body: node_link_protocol::resource::Subscribe =
            serde_json::from_str(first.body().get()).expect("subscribe body");
        // Owner 的 stale 代际形状：请求级 + 可重试。
        let mut body = node_link_protocol::error::Body::new(
            node_link_protocol::error::ErrorCode::ResourceAttachGenerationStale,
            "the attachment is not the current one; attach again before subscribing",
            true,
        )
        .expect("error body");
        body.correlation_id = acpr_wire::Nullable::value(first.message_id().clone());
        io.send_envelope(MessageType::LinkError, &session.connection_id, &body)
            .await;

        // 过期票证被丢弃后，客户端必须先重新 attach（而不是复用必然被拒的代际）。
        let second_attach = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("the retried resource.attach");
        assert_eq!(second_attach.message_type(), MessageType::ResourceAttach);
        let mut refreshed = attached();
        refreshed.attachment_generation = decimal(2);
        io.send_envelope(
            MessageType::ResourceAttached,
            &session.connection_id,
            &refreshed,
        )
        .await;

        let second = io
            .recv_envelope(&session.connection_id, &mut inbound)
            .await
            .expect("the resubscribe on the refreshed attachment");
        assert_eq!(second.message_type(), MessageType::ResourceSubscribe);
        let second_body: node_link_protocol::resource::Subscribe =
            serde_json::from_str(second.body().get()).expect("subscribe body");
        assert_eq!(
            second_body.cursor, first_body.cursor,
            "重新 attach 后必须从同一 cursor 续读"
        );
        assert_eq!(
            second_body.attachment_generation.as_str(),
            "2",
            "订阅必须使用新代际的票证"
        );
        let event = event_body(8, "018f0000-0000-4000-8000-000000000108");
        io.send_envelope(MessageType::ResourceEvent, &session.connection_id, &event)
            .await;
        stop_rx.await.expect("stop");
    });

    let sink: Arc<RecordingSink> = Arc::new(RecordingSink::default());
    let client = NodeLinkClient::new(
        mem.factory(),
        Arc::new(FixedIdentity::new([0x6e; 32])),
        sink.clone(),
        Arc::new(RecordingDeliveries::default()),
    );
    let link = client
        .open(ClientConfig::new(
            trust,
            "wss://owner.test/node-link/v1",
            TlsTrust::PlaintextDev,
        ))
        .expect("open");
    wait_online(&link).await;
    let wire = node_link_protocol::common::RemoteSessionRef {
        owner_node_id: uuid_text(OWNER_NODE),
        session_id: uuid_text(SESSION),
    };
    let ticket = link
        .shared()
        .attach(wire.clone())
        .await
        .expect("attachment ticket");
    let cursor = node_link_protocol::common::OriginCursor {
        origin_epoch: uuid_text("018f0000-0000-4000-8000-0000000000ee"),
        origin_sequence: decimal(7),
    };
    link.shared()
        .subscribe(&ticket, Some(cursor))
        .await
        .expect("the subscribe frame is handed to the transport");
    wait_until(
        || link.shared().subscription_error(&wire).is_some(),
        "the stale rejection to be reported",
    )
    .await;
    wait_until(
        || sink.events.lock().expect("events").len() == 1,
        "the event after the refreshed subscription",
    )
    .await;
    let refreshed = link
        .shared()
        .attachment(&wire)
        .expect("the refreshed attachment ticket");
    assert_eq!(
        refreshed.generation.as_str(),
        "2",
        "过期票证必须被替换，而不是继续被复用"
    );
    assert!(link.is_online(), "可重试拒绝不得关闭连接");
    assert_eq!(mem.opens(), 1, "可重试拒绝不得触发重连");
    stop_tx.send(()).expect("stop owner");
    owner_task.await.expect("owner task");
    link.shutdown().await;
}
