//! 受控路径全链路集成测试（切片 5 的验收闭环，任务 2.22 / `[PV5]` 主体）。
//!
//! 驱动方式：脚本化 fake Access 客户端（`support::nodelink`）经**真实 loopback listener** 走完
//! 配对 claim → 本地确认（真实 `LocalAdminRouter`）→ status approved → 握手（含 `catalogRevision`
//! 验签路径）→ `catalog.snapshot`（未撤销 ∧ grants 相交 ∧ 在确认点名的 `exportIds` 清单内）→
//! `session.create` → `resource.attach`/`subscribe`
//! → `resource.event`/`ack` → `session.prompt` 的终态推送 → `export.revoke`/`node.revoke` 的撤销传播。
//! Owner 侧用真实 SQLite + 真实 `Authority` + 组合根自己的装配点（`app::daemon`）。
//!
//! 三条必须的端到端断言（来自检视遗留）：
//!
//! - **R61**：broker 提交失败时连接上**不出现**对应 `resource.event`（`FlakySessionStore` 注入）；
//! - **终态推送**：被接受的 mutation 终态经命令路由的终态观察循环推到提交方连接；
//! - **撤销端到端**：`export.revoke` 后受影响连接收到 `export.revoked` 且后续命令被拒；`node.revoke`
//!   后连接收到 `node.trust.revoked` 并以 4410 关闭。
//!
//! 已知取舍（登记，不修）：① 扇出队列满即丢（`EVENT_QUEUE_CAPACITY` 残余①）；② 「会话阻塞在
//! socket 写」路径；③ 预持久化失败可能留下无端点的会话行（残余⑤）。

mod support;

use serde_json::{Value, json};
use server::local_admin::Method;
use support::nodelink::{AccessKey, NodeLinkClient};
use support::owner::OwnerNode;

/// 本用例扮演的 Access Node 标识。
const ACCESS_NODE: &str = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";

/// TLS `direct` 轮次：自签证书 + rustls，走完同一批配对/握手步骤（配对 HTTP 也在 TLS 之上）。
#[test]
fn tls_direct_terminates_the_same_handshake() {
    support::block_on(async {
        let certificate = TestCertificate::generate("e2e-tls");
        let config = certificate.client_config();
        let owner =
            OwnerNode::start_tls("e2e-tls", &certificate.cert_path, &certificate.key_path).await;
        let access = AccessKey::new(ACCESS_NODE);

        // ① 配对开始 + claim（TLS POST）。
        let begin = owner
            .admin(
                Method::NodePairBegin,
                json!({
                    "mode": "owner",
                    "pairingUrl": null,
                }),
            )
            .await;
        let pairing_id = begin["pairingId"].as_str().expect("pairingId").to_owned();
        let ticket = support::nodelink::ticket_from_url(
            begin["pairingUrl"].as_str().expect("pairingUrl"),
            &pairing_id,
        );
        let client_nonce = support::nodelink::nonce_text(0x61);
        let claim = support::nodelink::claim_request(&ticket, &access, &client_nonce);
        let reply = support::nodelink::post_json_tls(
            owner.addr,
            server::node_link::CLAIM_PATH,
            &serde_json::to_vec(&claim).expect("claim 可序列化"),
            std::sync::Arc::clone(&config),
        )
        .await;
        assert_eq!(
            reply.status,
            201,
            "TLS 上的合法 claim 必须返回 201：{}",
            String::from_utf8_lossy(&reply.body)
        );
        let body = reply.json();
        support::nodelink::verify_owner_proof(&ticket, &body, &access, &client_nonce);

        // ② 本地确认 + status approved。
        owner
            .admin(Method::NodePairConfirm, json!({ "pairingId": pairing_id }))
            .await;
        let status = support::nodelink::status_request(
            &ticket,
            &access,
            body["pairingRequestId"].as_str().expect("pairingRequestId"),
            &support::nodelink::nonce_text(0x62),
        );
        let reply = support::nodelink::post_json_tls(
            owner.addr,
            server::node_link::STATUS_PATH,
            &serde_json::to_vec(&status).expect("status 可序列化"),
            std::sync::Arc::clone(&config),
        )
        .await;
        assert_eq!(reply.json()["status"], json!("approved"));

        // ③ WSS（TLS 之上）完成同一握手，并取回一份 catalog 快照。
        let mut client =
            NodeLinkClient::connect_tls(owner.addr, std::sync::Arc::clone(&config)).await;
        support::nodelink::handshake(&mut client, &access, &ticket, &client_nonce).await;
        client
            .send("catalog.subscribe", json!({ "knownRevision": null }))
            .await;
        let snapshot = client.expect("catalog.snapshot").await;
        assert_eq!(
            snapshot["body"]["agents"].as_array().expect("agents").len(),
            1,
            "catalog 快照必须携带配置声明的 Agent（握手与 catalog 都跑在 TLS 上）"
        );
        owner.stop().await;
    });
}

/// 自签证书（`rcgen`）与它的 PEM 路径。
struct TestCertificate {
    directory: std::path::PathBuf,
    cert_path: std::path::PathBuf,
    key_path: std::path::PathBuf,
    der: rustls_pki_types::CertificateDer<'static>,
}

impl TestCertificate {
    fn generate(label: &str) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "acpr-wp7-e2e-cert-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("创建临时目录");
        let certified = rcgen::generate_simple_self_signed(vec![
            support::nodelink::PUBLIC_HOST.to_owned(),
            "localhost".to_owned(),
        ])
        .expect("自签证书");
        let cert_path = directory.join("cert.pem");
        let key_path = directory.join("key.pem");
        std::fs::write(&cert_path, certified.cert.pem()).expect("写证书");
        std::fs::write(&key_path, certified.signing_key.serialize_pem()).expect("写私钥");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&cert_path, std::fs::Permissions::from_mode(0o600))
                .expect("chmod 0600");
            std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))
                .expect("chmod 0600");
        }
        Self {
            directory,
            cert_path,
            key_path,
            der: certified.cert.der().clone(),
        }
    }

    /// 只信任本用例现算的那张证书的 rustls 客户端配置（不做危险的「跳过校验」）。
    fn client_config(&self) -> std::sync::Arc<rustls::ClientConfig> {
        let mut roots = rustls::RootCertStore::empty();
        roots.add(self.der.clone()).expect("自签证书可加入信任根");
        let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("默认协议版本可用")
            .with_root_certificates(roots)
            .with_no_client_auth();
        std::sync::Arc::new(config)
    }
}

impl Drop for TestCertificate {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

// ---------------------------------------------------------------------------------------------
// AC1（tasks 4.1）：节点级事件的**组合根接线**在真实组合根 + 真实 SQLite + 真实 fake ACP 子进程上
// 落库，且会话标识为空、不被会话级投递路径误收。
//
// 为什么在这里（`node_link_e2e.rs`）：tasks 4.1 把 AC1 的证据面钉在本文件；本节补的正是本文件其余用例
// **覆盖不到**的一段——其余用例用脚本化后端（`support::owner::OwnerNode`），不启动真实 Agent 进程，
// 因此**永远不会产生** `agent.connected`；而节点级事件的唯一生产来源是真实 profile 进程的生命周期。
//
// 本节的装配点是 `app::compose::Composition` **本身**（不是测试自建的替身）：若 `compose.rs` 没有把
// `NodeEvents` 接到 `Broker::commit_node_event`（规划缺口「节点级事件的组合根接线无人认领」），
// `NodeEvents` 保持 unbound，`agent.connected` 会在 core 之前被丢弃，本节的落库断言必然失败。
//
// 本节只覆盖**节点级**事件（`agent.connected`/`agent.disconnected`）。AC1 的另外两项断言
// （`file.changed`（会话级）经 owned 路径落库并按 origin 序回放、会话标题在 `session_info_update`
// 后更新）由紧接本节的两条用例覆盖——它们用同一个组合根与同一个真实 fake ACP 子进程，只是把场景换成
// 会发类型化 Diff 与会话信息更新的 `chunked-updates`。本文件其余用例（脚本化后端）不产生这两类事件。
// ---------------------------------------------------------------------------------------------

/// fake ACP Agent 可执行文件：优先取 `ACPR_FAKE_ACP_AGENT`，否则按「测试可执行文件同 target 目录」
/// 推断（`<profile>/deps/<test>` → `<profile>/acpr-fake-acp-agent[.exe]`）。
///
/// AC1 的环境列明写「fake ACP Agent」。它由 `agent-host` 提供，`cargo test -p app` **不**会构建它，
/// 因此这里在缺失时**明确失败**（不静默跳过——跳过的测试不算通过）：先跑一次 PV1/PV2，或
/// `cargo build -p agent-host --bin acpr-fake-acp-agent`。
fn fake_acp_agent_binary() -> std::path::PathBuf {
    // 显式覆盖（CI/矩阵需要时可指定；取值必须是可执行的 fake ACP Agent）。
    if let Ok(explicit) = std::env::var("ACPR_FAKE_ACP_AGENT") {
        let path = std::path::PathBuf::from(explicit);
        assert!(
            path.is_file(),
            "ACPR_FAKE_ACP_AGENT 指向的文件不存在：{}",
            path.display()
        );
        return path;
    }
    let test_exe = std::env::current_exe().expect("测试可执行文件路径");
    let profile_dir = test_exe
        .parent()
        .and_then(std::path::Path::parent)
        .expect("测试可执行文件位于 <profile>/deps/");
    let name = if cfg!(windows) {
        "acpr-fake-acp-agent.exe"
    } else {
        "acpr-fake-acp-agent"
    };
    let path = profile_dir.join(name);
    assert!(
        path.is_file(),
        "AC1 需要 fake ACP Agent（与测试同 target 目录）：{} 不存在。\
         请先 `cargo build -p agent-host --bin acpr-fake-acp-agent`（或先跑 PV1/PV2 构建全工作区），\
         或用 ACPR_FAKE_ACP_AGENT 指定它的路径。",
        path.display()
    );
    path
}

/// 直接读**真实 SQLite** 的 `owned_event`（`(event_id, session_id 或 '<null>')`）。
///
/// AC1「可按事件库读回」最直接的证据：读的是落盘行，不经过任何 `Broker` 的内存状态。
async fn owned_event_rows(data_dir: &std::path::Path, event_type: &str) -> Vec<(String, String)> {
    let pool = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(data_dir.join("acp-remote.sqlite3"))
            .read_only(true)
            .busy_timeout(std::time::Duration::from_secs(10)),
    )
    .await
    .expect("打开真实 SQLite（只读）");
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT event_id, COALESCE(session_id, '<null>') FROM owned_event \
         WHERE event_type = ?1 ORDER BY global_sequence",
    )
    .bind(event_type)
    .fetch_all(&pool)
    .await
    .expect("查询 owned_event");
    pool.close().await;
    rows
}

/// 组合根的开发模式配置：显式进程内 keystore + 一条指向 fake ACP Agent 的 profile 种子
/// （`--scenario normal`）。
fn ac1_config(data_dir: &std::path::Path, agent_command: &str) -> app::Config {
    ac1_config_with_scenario(data_dir, agent_command, "normal")
}

/// 同上，但 profile 的 `--scenario` 由调用方给出：AC1 的 `file.changed` / 标题断言需要
/// `chunked-updates`（该场景在**同一次 prompt** 里发出带类型化 `diff` 的 `tool_call` 与带 `title` 的
/// `session_info_update`，是真实 Agent 路径上唯一能同时产生这两类事件的场景）。
fn ac1_config_with_scenario(
    data_dir: &std::path::Path,
    agent_command: &str,
    scenario: &str,
) -> app::Config {
    let command = agent_command.replace('\\', "/");
    let text = format!(
        "[daemon]\ndata_dir = {dir:?}\n[identity]\nkeystore = \"ephemeral\"\n\
         [dev_mode]\nenabled = true\n\
         [storage]\npersist_deltas = true\n\
         [agent]\ndefault = \"codex\"\n\
         codex = [\"{command}\", \"--scenario\", \"{scenario}\"]\n",
        dir = data_dir.display(),
    );
    app::Config::from_toml(&text).expect("AC1 配置合法")
}

/// 32 字节 base64url（无填充）的 payload 摘要（`session.create` 的幂等指纹位）。
fn ac1_digest(label: &str) -> acp_core::model::Digest {
    use base64::Engine as _;
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(label.as_bytes());
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize());
    acp_core::model::Digest::new(&encoded).expect("32 字节摘要")
}

/// 建一条本机 owned 会话并**落账**（§6 第 20 条的三笔时序：受理 → 执行 → 落账），返回新会话的标识。
///
/// 本文件的用例都要求这条会话**真实存在**（`session.prompt` 要有会话行、`owned_session` 的列要可读），
/// 而受理提交只落幂等记录——会话行与终态在**落账提交**里同批落盘，因此 helper 在 `create_session`
/// 之后必须再调一次 `settle_session_create`（与生产适配层的做法同形）。终态结果的原文只是形状占位：
/// 本文件对会话的断言都不读它。
async fn ac1_create_and_settle(
    composition: &app::compose::Composition,
    fingerprint: acp_core::model::Digest,
    request: acp_core::model::CreateSessionRequest,
) -> acp_core::model::SessionId {
    use acp_core::model::{Actor, CommandResult, CommandStatus};

    let request_id = composition.ids().request_id();
    let prepared = match composition
        .use_cases()
        .create_session(
            &Actor::LocalCli,
            request_id.clone(),
            fingerprint,
            request,
            None,
        )
        .await
        .expect("session.create（会惰性拉起真实 fake ACP 子进程）")
    {
        acp_core::ports::SessionCreateAdmission::Prepared(prepared) => prepared,
        other => panic!("首次受理必须返回 Prepared，得到 {other:?}"),
    };
    let result =
        CommandResult::from_json_text(r#"{"sessionId":"projected"}"#).expect("result object");
    assert!(
        composition
            .use_cases()
            .settle_session_create(
                &Actor::LocalCli,
                &request_id,
                CommandStatus::Completed,
                Some(result),
                None,
            )
            .await
            .expect("落账"),
        "落账必须真的写入"
    );
    prepared.session
}

/// R30–R35（`specs/core-derived-events/spec.md` §「Agent 连接状态是节点级生命周期」）+ AC1：
/// profile 进程**建立**（真实 ACP 子进程、经组合根自己的装配）时产生一次 `agent.connected`，它经
/// `compose.rs` 的接线 → `Broker::commit_node_event` **落库**，`owned_event.session_id` 为空。
///
/// 同时断言**不被会话级投递路径误收**：组合根实际安装的 `forked_publisher` → `NodeLinkPublisher`
/// （`server::node_link::resource`）是「按会话归属投递」的入站路径，它必须丢弃会话标识为空的事件；
/// 因此扇出队列里**不出现**任何节点级事件（其余会话级事件仍会出现，见下方的正向对照）。
#[test]
fn ac1_the_composition_root_persists_node_level_events_without_session_identity() {
    use acp_core::model::{
        Actor, AgentId, AgentRef, CreateSessionRequest, Digest, ResolvedWorkspace, ResourceOrigin,
    };

    support::block_on(async {
        let root = support::TempRoot::new("ac1-node");
        let data_dir = root.join("data");
        support::create_owner_only_dir(&data_dir);
        let fake = fake_acp_agent_binary();

        // 组合根**实际安装**的发布分叉：扇出接收端留在这里，用来断言节点级事件不进入会话级投递路径。
        let (publisher, mut fanout) = app::compose::forked_publisher();
        let composition = app::compose::Composition::assemble(
            ac1_config(&data_dir, &fake.display().to_string()),
            publisher,
        )
        .await
        .expect("组合根装配");

        // 前置：接线已落地（这正是规划缺口的核心——接线缺失时这里是 false）。
        assert!(
            composition.host().node_events_bound(),
            "组合根必须已接线 NodeEvents（否则节点级事件在 core 之前被丢弃）"
        );

        let agent =
            AgentRef::try_new(AgentId::new("codex").expect("agentId"), "Codex").expect("agent ref");
        let path = data_dir.display().to_string();
        let workspace =
            ResolvedWorkspace::try_new(acp_core::model::derive_directory_id(&path), path)
                .expect("workspace");
        let request =
            CreateSessionRequest::new(agent, Some(workspace), None, ResourceOrigin::Local);
        let request_id = composition.ids().request_id();
        let fingerprint: Digest = ac1_digest("ac1-session-create");
        // 本用例只观察**节点级**事件，因此这里**刻意不结算**本次创建（受理的窗口期一直开着，没有会话
        // 行、也没有终态）：结算会写下 `command.completed`，那是一条会话级投递，会污染下面
        // 「会话级扇出队列应恰为空」的断言。窗口期里到达的事件留在队列里不落盘，正是本用例要的现场。
        let session = match composition
            .use_cases()
            .create_session(&Actor::LocalCli, request_id, fingerprint, request, None)
            .await
            .expect("session.create（会拉起真实 fake ACP 子进程）")
        {
            acp_core::ports::SessionCreateAdmission::Prepared(prepared) => prepared.session,
            other => panic!("首次受理必须返回 Prepared，得到 {other:?}"),
        };
        assert_eq!(
            session.as_str().len(),
            36,
            "会话 id 必须是规范 uuid 文本：{session}"
        );

        // 落库是组合根 runtime 上的异步提交任务，轮询等待（上限 15 s）。
        let pool = sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(data_dir.join("acp-remote.sqlite3"))
                .read_only(true)
                .busy_timeout(std::time::Duration::from_secs(10)),
        )
        .await
        .expect("打开真实 SQLite（只读）");

        // §7.3 的四组 CHECK 成对成立：节点级事件的 `session_id` 为空时，`session_sequence`/
        // `origin_epoch`/`origin_sequence` 必须**同时**为空（否则落盘行违反表级 CHECK）。一并对拍。
        type NodeRow = (
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        );
        let mut rows: Vec<NodeRow> = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while std::time::Instant::now() < deadline {
            rows = sqlx::query_as::<_, NodeRow>(
                "SELECT event_id, session_id, session_sequence, origin_epoch, origin_sequence \
                 FROM owned_event WHERE event_type = 'agent.connected' ORDER BY global_sequence",
            )
            .fetch_all(&pool)
            .await
            .expect("查询 owned_event");
            if !rows.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        assert_eq!(
            rows.len(),
            1,
            "R30：进程建立必须**恰好一次**落库 agent.connected（接线缺失时会漏掉）；实际 {} 行",
            rows.len()
        );
        let (event_id, session_id, session_sequence, origin_epoch, origin_sequence) = &rows[0];
        assert!(
            session_id.is_none(),
            "R30/R8：节点级事件的 session_id 必须为空（事件 {event_id}）；实际 {session_id:?}"
        );
        assert!(
            session_sequence.is_none(),
            "会话标识为空时 session_sequence 必须同时为空（§7.3 成对 CHECK）；实际 {session_sequence:?}"
        );
        assert!(
            origin_epoch.is_none() && origin_sequence.is_none(),
            "节点级事件不得有会话级 origin 游标（§7.3）；实际 {origin_epoch:?}/{origin_sequence:?}"
        );

        // 「不被会话级投递路径误收」：扇出队列（组合根实际安装的 `NodeLinkPublisher`）不得出现节点级事件。
        // 给提交/发布一点余量后把队列看空，收集可见的事件类型。
        let mut seen: Vec<String> = Vec::new();
        let drain_deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        while std::time::Instant::now() < drain_deadline {
            match fanout.try_recv() {
                Ok(event) => seen.push(event.event_type.as_str().to_owned()),
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
            }
        }
        // 丢弃点在 `crates/server/src/node_link/resource.rs` 的 `NodeLinkPublisher::publish`：
        // `if event.session.is_none() { return; }`——节点级事件的会话标识为空，因此**不得**进入扇出。
        // 本场景（`normal`）不产生会话级事件，队列应恰为空；若非空也不得含任何 `agent.*`。
        assert!(
            !seen.iter().any(|kind| kind.starts_with("agent.")),
            "会话级投递路径不得收到节点级事件（agent.*）；实际收到 {seen:?}"
        );
        assert!(
            seen.is_empty(),
            "会话级投递路径只承载会话级事件，本场景应为空；实际 {seen:?}"
        );

        pool.close().await;

        // 关闭序列（与 `crates/app/src/daemon.rs::close` 同序）：先停 Agent（此处产生本进程实例唯一的一次
        // `agent.disconnected`，其提交任务由组合根的 sink spawn），**再** `Composition::close`——后者会先
        // 排空这些提交任务，然后才关存储。若不先 `shutdown_all`，退出路由任务会持有 `AgentRuntime`（进而
        // 持有 `NodeEvents` → `Broker` → 存储），`close` 会以上报 `StoreStillShared` 的方式**如实失败**
        // 而不是静默跳过检查点。
        composition.host().shutdown_all().await;
        composition.close().await.expect("组合根可关闭");

        // R32：断开事件同样落库且会话标识为空（`shutdown_all` 结束进程时产生）。
        let disconnected = owned_event_rows(&data_dir, "agent.disconnected").await;
        assert_eq!(
            disconnected.len(),
            1,
            "R32：进程退出必须**恰好一次**落库 agent.disconnected；实际 {disconnected:?}"
        );
        assert_eq!(
            disconnected[0].1, "<null>",
            "R32：断开事件的 session_id 必须为空"
        );
    });
}

// ---------------------------------------------------------------------------------------------
// AC1（tasks 4.1）断言 A：会话级 `file.changed` 经 **owned 路径**（`session_id` 非空）落库，并按
// **origin 序**回放（R5–R7 / `design.md` D4/D5）。
//
// 与上一节的分工：上一节证明**节点级**事件落库且会话标识为空；本节证明**会话级**事件落库时带会话标识，
// 且按 origin 序回放（不是 UUID 序，也不是「派生后追加在批尾」）。驱动面同上：真实组合根
// `app::compose::Composition` + 真实 SQLite + 真实 fake ACP 子进程。
//
// `chunked-updates` 的一次 prompt 发出：两条 `agent_message_chunk`、一条 `agent_thought_chunk`、一条带
// **类型化 Diff**（`path: "src/main.rs"`、`oldText: "let a = 1;"`、`newText: "let a = 2;"`）的
// `tool_call`、一条 `tool_call_update`（completed，带普通内容块）、plan/commands/mode/config/
// session_info/usage/未来判别子各一条，最后 `stopReason: end_turn`。核心的派生点在组装
// `tool.call.started` 的**同一次提交**内（紧随其前），因此 `file.changed` 落在该簇中间，而不是序列末尾。
// ---------------------------------------------------------------------------------------------

/// 只读连接（每个 helper 自开自闭）。测试进程持有的组合根仍在写这个库，WAL 允许并发读。
async fn ac1_ro_pool(data_dir: &std::path::Path) -> sqlx::SqlitePool {
    sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(data_dir.join("acp-remote.sqlite3"))
            .read_only(true)
            .busy_timeout(std::time::Duration::from_secs(10)),
    )
    .await
    .expect("打开真实 SQLite（只读）")
}

/// 某会话的全部落盘事件类型（按 **origin 序**）。节点级事件（`session_id` 为空）不在其中。
async fn ac1_session_event_types(
    data_dir: &std::path::Path,
    session: &acp_core::model::SessionId,
) -> Vec<String> {
    let pool = ac1_ro_pool(data_dir).await;
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT event_type FROM owned_event WHERE session_id = ?1 ORDER BY origin_sequence ASC",
    )
    .bind(session.as_str())
    .fetch_all(&pool)
    .await
    .expect("查询 owned_event");
    pool.close().await;
    rows
}

/// 组合根 + 真实 fake ACP 子进程驱动**一次** owned 会话的一轮 prompt，返回会话 id。
///
/// `submit_command` 只把 turn 排进队列并派发；后端事件异步写进 sink 的缓冲，必须由持有 runtime 的组合根
/// 周期调 `Broker::pump` 才落盘（与 `crates/app/src/daemon.rs` 的合并窗口是同一件事）。因此这里轮询
/// `pump` 直到该轮的 `turn.completed` 落库——它的落库保证同一轮更早的事件（含 `file.changed` 与
/// `session.info.changed`）已经在更早的批次中提交。
async fn ac1_prompt_once(
    composition: &app::compose::Composition,
    data_dir: &std::path::Path,
    label: &str,
) -> acp_core::model::SessionId {
    use acp_core::model::{
        Actor, AgentId, AgentRef, ClientCommand, CommandKind, CommandPayload, CreateSessionRequest,
        PromptContentBlock, ResolvedWorkspace, ResourceOrigin, ViewJson,
    };

    let agent =
        AgentRef::try_new(AgentId::new("codex").expect("agentId"), "Codex").expect("agent ref");
    let path = data_dir.display().to_string();
    let workspace = ResolvedWorkspace::try_new(acp_core::model::derive_directory_id(&path), path)
        .expect("workspace");
    let request = CreateSessionRequest::new(agent, Some(workspace), None, ResourceOrigin::Local);
    let session = ac1_create_and_settle(
        composition,
        ac1_digest(&format!("ac1-session-{label}")),
        request,
    )
    .await;

    let prompt = ClientCommand {
        actor: Actor::LocalCli,
        request: composition.ids().request_id(),
        command: "session.prompt".to_owned(),
        kind: CommandKind::Mutation,
        session: Some(session.clone()),
        expected_version: None,
        request_fingerprint: ac1_digest(&format!("ac1-prompt-{label}")),
        payload: CommandPayload::Prompt {
            content: vec![PromptContentBlock::new(
                ViewJson::new(r#"{"type":"text","text":"请编辑 src/main.rs"}"#).expect("view"),
            )],
        },
    };
    let receipt = composition
        .use_cases()
        .submit_command(&Actor::LocalCli, prompt)
        .await
        .expect("session.prompt 提交");
    assert!(
        matches!(
            receipt,
            acp_core::model::CommandReceipt::Accepted { turn: Some(_), .. }
        ),
        "prompt 必须被接受且带 turn：{receipt:?}"
    );

    // 驱动合并窗口直到本轮的终态事件落库（上限 20 s）。
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        composition
            .broker()
            .pump(&session)
            .await
            .expect("驱动合并窗口");
        let types = ac1_session_event_types(data_dir, &session).await;
        if types.iter().any(|kind| kind == "turn.completed") {
            return session;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "超时：turn.completed 未落库（{label}）；实际事件 {types:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// 组合根的开发模式配置（含 `storage.persist_deltas = true`）+ 指向 fake ACP Agent 的指定场景
/// profile 种子（两条新断言共用）。
///
/// `persist_deltas = true`：默认 `false` 会让 turn 终态后压缩增量（把 delta 行标记 `compacted_into`，
/// 由一条 `turn.delta_compacted` 汇总替代），而压缩行不进 origin 切片（`compacted_into IS NULL`）——
/// 那会让本节的「origin 序恰为 `1..=N`」断言被压缩断层破坏。开 `persist_deltas` 后每轮的 origin 序列
/// 连续，这正是「按 origin 序回放」要观察的对象。
async fn ac1_composition_with_scenario(
    data_dir: &std::path::Path,
    scenario: &str,
) -> (
    app::compose::Composition,
    tokio::sync::mpsc::Receiver<acp_core::model::CommittedEvent>,
) {
    let fake = fake_acp_agent_binary();
    let (publisher, fanout) = app::compose::forked_publisher();
    let composition = app::compose::Composition::assemble(
        ac1_config_with_scenario(data_dir, &fake.display().to_string(), scenario),
        publisher,
    )
    .await
    .expect("组合根装配");
    // 扇出接收端交回调用方持有：消费者缺席时队列满会丢弃（`EVENT_QUEUE_CAPACITY`），本节只断言落库，
    // 但保有接收端才是组合根在生产里的形态（`daemon` 把接收端交给 `ResourceRoute`）。
    (composition, fanout)
}

/// R5–R7：会话级 `file.changed` 经 owned 路径落库（`session_id` 非空），且**按 origin 序回放**。
///
/// 两个会话各驱动一次 prompt，因此库里恰有**两条** `file.changed`；会话 A 上另断言完整的 origin 序与
/// 游标续读（否则「顺序」无从验证）。可证伪点：把 `Broker::commit_chunk` 的派生块或存储的
/// `origin_sequence` 分配撤掉，本节的前置（`file.changed` 出现 + 序列恰为 `1..=N`）必然失败。
#[test]
fn ac1_the_composition_root_persists_file_changes_on_the_owned_path_in_origin_order() {
    use acp_core::model::OriginCursor;
    use acp_core::ports::ReplayLimit;

    support::block_on(async {
        let root = support::TempRoot::new("ac1-file");
        let data_dir = root.join("data");
        support::create_owner_only_dir(&data_dir);

        let (composition, _fanout) =
            ac1_composition_with_scenario(&data_dir, "chunked-updates").await;
        let first = ac1_prompt_once(&composition, &data_dir, "file-a").await;
        let second = ac1_prompt_once(&composition, &data_dir, "file-b").await;

        // ① 「落库」：库中恰有两条 `file.changed`，分属两个会话且 `session_id` **非空**——这正是它与会话
        //    标识为空、不走会话级投递路径的节点级事件的本质区别。
        let pool = ac1_ro_pool(&data_dir).await;
        type FileChangedRow = (Option<String>, Option<i64>, Option<i64>, String);
        let rows: Vec<FileChangedRow> = sqlx::query_as(
            "SELECT session_id, session_sequence, origin_sequence, payload_json FROM owned_event \
             WHERE event_type = 'file.changed' ORDER BY global_sequence",
        )
        .fetch_all(&pool)
        .await
        .expect("查询 file.changed");
        assert_eq!(
            rows.len(),
            2,
            "两个会话各派生恰好一条 file.changed；实际 {} 行",
            rows.len()
        );
        let mut owners: Vec<&str> = rows
            .iter()
            .map(|(session, _, _, _)| session.as_deref().expect("owned 路径：session_id 非空"))
            .collect();
        owners.sort_unstable();
        let mut expected = vec![first.as_str(), second.as_str()];
        expected.sort_unstable();
        assert_eq!(
            owners, expected,
            "两条 file.changed 必须分别属于两个会话（owned 路径）"
        );

        // 会话 A 的那条：会话标识与两套会话级游标都在（node 级事件三者皆空）。
        let a_row = rows
            .iter()
            .find(|(session, _, _, _)| session.as_deref() == Some(first.as_str()))
            .expect("会话 A 的 file.changed");
        assert!(
            a_row.1.is_some() && a_row.2.is_some(),
            "会话级 file.changed 必须带 session_sequence 与 origin_sequence：{a_row:?}"
        );
        // 视图字段（R5/R6/R7）：类型化 Diff 派生、行级 Myers 结果、工作区内相对路径。
        let view: Value = serde_json::from_str(&a_row.3).expect("file.changed view 是 JSON");
        assert_eq!(view["kind"], json!("modified"), "{view}");
        assert_eq!(view["displayPath"], json!("src/main.rs"), "{view}");
        assert_eq!(view["addedLines"], json!("1"), "{view}");
        assert_eq!(view["deletedLines"], json!("1"), "{view}");
        assert!(
            view.get("outsideWorkspace").is_none(),
            "工作区内的路径不得带 outsideWorkspace：{view}"
        );

        // ② 「按 origin 序回放」：经真实读视图的 origin 切片（`ORDER BY origin_sequence`）取回事件。
        let read = composition.broker().read_view().await.expect("读视图");
        let slice = read
            .node_link_slice(&first, None, ReplayLimit::new(200))
            .await
            .expect("会话 A 的 origin 切片");
        let ordered: Vec<(String, u64)> = slice
            .events
            .iter()
            .map(|record| {
                (
                    record.event.event_type.as_str().to_owned(),
                    record
                        .event
                        .origin_sequence
                        .expect("会话事件必有 origin 序列")
                        .get(),
                )
            })
            .collect();
        let seqs: Vec<u64> = ordered.iter().map(|(_, seq)| *seq).collect();
        assert!(seqs.len() >= 4, "混合序列不足以验证顺序：{ordered:?}");
        assert_eq!(
            seqs,
            (1..=seqs.len() as u64).collect::<Vec<_>>(),
            "回放必须按 origin 序：序列恰为 1..=N 且严格递增；实际 {seqs:?}"
        );

        let position = ordered
            .iter()
            .position(|(kind, _)| kind == "file.changed")
            .expect("会话 A 的 origin 序列必须含 file.changed");
        let change_seq = ordered[position].1;
        assert!(
            position > 0 && position + 1 < ordered.len(),
            "file.changed 必须落在序列内部（不是派生后追加在批尾）：{ordered:?}"
        );
        assert!(
            [position - 1, position + 1]
                .iter()
                .any(|index| ordered[*index].0.starts_with("tool.call.")),
            "file.changed 必须紧邻产生它的工具调用事件（即时派生）：{ordered:?}"
        );

        // ③ 游标续读按 origin 定位到 file.changed 本身（`origin_sequence > after`）。
        let before = read
            .node_link_slice(
                &first,
                Some(OriginCursor {
                    origin_epoch: slice.head.origin_epoch.clone(),
                    origin_sequence: acp_core::model::Sequence::new(change_seq - 1)
                        .expect("sequence"),
                }),
                ReplayLimit::new(200),
            )
            .await
            .expect("增量回放（file.changed 之前一条）");
        assert_eq!(
            before
                .events
                .first()
                .map(|record| record.event.event_type.as_str()),
            Some("file.changed"),
            "after = file.changed 的前一条时，回放的第一条必须就是它（按 origin 序定位）"
        );
        let after = read
            .node_link_slice(
                &first,
                Some(OriginCursor {
                    origin_epoch: slice.head.origin_epoch.clone(),
                    origin_sequence: acp_core::model::Sequence::new(change_seq).expect("sequence"),
                }),
                ReplayLimit::new(200),
            )
            .await
            .expect("增量回放（file.changed 之后）");
        assert!(
            after
                .events
                .iter()
                .all(|record| record.event.event_type.as_str() != "file.changed"),
            "file.changed 之后的重放不得再出现它"
        );
        assert_eq!(
            after.events.len(),
            ordered.len() - change_seq as usize,
            "游标之后的条数必须是 N - seq"
        );
        pool.close().await;

        composition.host().shutdown_all().await;
        composition.close().await.expect("组合根可关闭");
    });
}

// ---------------------------------------------------------------------------------------------
// AC1（tasks 4.1）断言 B：会话标题在 `session_info_update` 后更新（R9 / `design.md` D7）。
//
// 三件事：① 通知把标题**单向**写入会话（创建时为空 → 通知后为通知取值，会话摘要随之反映）；
// ② `session.info.changed` 事件落库且带会话标识；③ 会话的**权威**更新时间取 Daemon 持久化时间，
// 不是 Agent 自报的 `updatedAt`。
//
// 未覆盖的三态之一：fake ACP Agent 只在 `chunked-updates` 里发 `session_info_update`，且恒带非空
// `title`，因此「通知显式置空」与「只带 updatedAt」两条路径无法经真实子进程驱动；它们由 WP3 的
// `crates/storage-sqlite/tests/title_write.rs` 与 `crates/core/src/broker.rs` 的 R9 行为测试覆盖
// （本任务边界不允许改 `crates/agent-host/**` 去加场景）。
// ---------------------------------------------------------------------------------------------

/// R9：会话标题在 `session_info_update` 后更新，`updatedAt` 取 Daemon 持久化时间。
#[test]
fn ac1_the_composition_root_updates_the_session_title_from_the_agent_notification() {
    use acp_core::model::Actor;
    use acp_core::ports::SessionQuery;

    support::block_on(async {
        let root = support::TempRoot::new("ac1-title");
        let data_dir = root.join("data");
        support::create_owner_only_dir(&data_dir);

        let (composition, _fanout) =
            ac1_composition_with_scenario(&data_dir, "chunked-updates").await;

        // 驱动一次 prompt（fake 会在其中发出带 `title: "会话标题"` 的 `session_info_update`）。
        let session = ac1_prompt_once(&composition, &data_dir, "title").await;

        let pool = ac1_ro_pool(&data_dir).await;
        let (title, version, created_at, updated_at): (Option<String>, i64, String, String) =
            sqlx::query_as(
                "SELECT title, version, created_at, updated_at FROM owned_session WHERE session_id = ?1",
            )
            .bind(session.as_str())
            .fetch_one(&pool)
            .await
            .expect("查询 owned_session");

        // ① 标题已被通知写入（创建时为空——由 `create_session` 恒写 `title: None` 决定）。
        assert_eq!(
            title.as_deref(),
            Some("会话标题"),
            "R9：标题必须来自 Agent 的 session_info_update"
        );
        assert!(
            version > 1,
            "标题写入走通用状态变更路径，会话版本必须递增：version={version}"
        );

        // ② 投影出 `session.info.changed` 事件，且它带会话标识（会话级事件）。
        let events: Vec<(Option<String>, String)> = sqlx::query_as(
            "SELECT session_id, payload_json FROM owned_event \
             WHERE session_id = ?1 AND event_type = 'session.info.changed' \
             ORDER BY origin_sequence",
        )
        .bind(session.as_str())
        .fetch_all(&pool)
        .await
        .expect("查询 session.info.changed");
        assert_eq!(
            events.len(),
            1,
            "R9：一次通知投影恰好一条 session.info.changed；实际 {} 行",
            events.len()
        );
        assert!(
            events[0].0.as_deref() == Some(session.as_str()),
            "session.info.changed 是会话级事件，必须带 session_id"
        );
        let event_view: Value =
            serde_json::from_str(&events[0].1).expect("session.info.changed view 是 JSON");
        assert_eq!(event_view["title"], json!("会话标题"), "{event_view}");
        // 事件视图**转发** Agent 自报的 `updatedAt`（合同字段），与会话的权威更新时间是两个来源。
        assert_eq!(
            event_view["updatedAt"],
            json!("2026-09-24T10:00:00.000Z"),
            "{event_view}"
        );

        // ③ 会话的权威更新时间取 Daemon 持久化时间，**不是** Agent 自报值；且单调不早于创建时间。
        assert_ne!(
            updated_at, "2026-09-24T10:00:00.000Z",
            "R9/D7：会话更新时间不得采用 Agent 自报的 updatedAt"
        );
        assert!(
            updated_at.as_str() >= created_at.as_str(),
            "更新时间必须不早于创建时间：{created_at} → {updated_at}"
        );
        pool.close().await;

        // ④ 会话摘要随之反映该标题（目录页的读面）。
        let summaries = composition
            .use_cases()
            .list_sessions(
                &Actor::LocalCli,
                SessionQuery {
                    only: Some(vec![session.clone()]),
                    states: Vec::new(),
                    limit: None,
                },
            )
            .await
            .expect("session.list");
        assert_eq!(summaries.len(), 1, "只应取回本次会话");
        assert_eq!(
            summaries[0].title(),
            Some("会话标题"),
            "会话摘要必须反映被更新的标题"
        );

        composition.host().shutdown_all().await;
        composition.close().await.expect("组合根可关闭");
    });
}

// ---------------------------------------------------------------------------------------------
// tasks 3.2（WP4）：「出站 ACP 路径参数的形式」（`openspec/changes/acp-cwd-path-form/specs/
// local-agent-host/spec.md` 的 ADDED Requirement）在 app 层的证伪断言——同一次 `session.create`
// 的**两侧**取值同时被观察：持久化侧不改写、线缆侧剥掉 Windows verbatim 前缀。
//
// 覆盖该要求的三个 Scenario：①「带 verbatim 前缀的目录在出站时被剥除」（本用例即此形态）、
// ②「持久化取值与解析结果不被改写」（持久化侧断言）、③「非 Windows 平台保持原值」（非 Windows
// 分支下出站取值与持久化取值逐字节相同）。
//
// 为什么必须是这条形状：`outbound_cwd`（`crates/agent-host/src/host.rs`）只作用于**发往 Agent 的
// 参数**，不得回写 core 的解析结果或持久化列。两侧断言必须落在**同一个** `create_session` 上，
// 否则「没剥前缀」与「把剥过的值写回持久化」两种相反的实现错误都能各自被另一半掩盖。因此：
//
// - 持久化侧读**真实 SQLite** 的 `owned_session.workspace_cwd`（core 在 `factory.create` 成功后
//   紧接着写入的取值，来自 `ResolvedWorkspace::canonical_path()`）；
// - 线缆侧读假 ACP 子进程经 `--dump-request-params` 写下的**真实入站报文**里的
//   `session/new` → `params.cwd`（口径同 `crates/agent-host/tests/resume.rs`：只有真实子进程
//   收到的字节才算线级证据）。
//
// 平台分工（CI 在 Linux 上跑 `cargo test -p app`，因此断言不能整条 `#[cfg(windows)]` 钡掉）：
// Windows 上 `canonicalize` 给出 `\\?\` 前缀，出站取值必然与持久化取值**不同**；其它平台上
// `strip_verbatim` 逐字节直通，三者必须相同。两种情况都由断言固定，不靠「不抛异常」兜底。
// ---------------------------------------------------------------------------------------------

/// [`ac1_config_with_scenario`] 的同形变体：场景固定为 `normal`，另在 profile 的 `args` 里加
/// `--dump-request-params <path>`，让假 Agent 把**实际收到**的每条入站报文追加一行
/// `{"method":…,"params":…}`（`acpr-fake-acp-agent.rs` 的同名选项）。
///
/// 路径文本按本文件既有口径处理（正斜杠 → TOML 基本字符串，与 `command` 一致），避免 Windows
/// 反斜杠的转义问题；调用方给的 `dump_path` 同样先走 `TempRoot::toml`。
fn ac1_config_with_request_dump(
    data_dir: &std::path::Path,
    agent_command: &str,
    dump_path: &str,
) -> app::Config {
    let command = agent_command.replace('\\', "/");
    let dump = dump_path.replace('\\', "/");
    let text = format!(
        "[daemon]\ndata_dir = {dir:?}\n[identity]\nkeystore = \"ephemeral\"\n\
         [dev_mode]\nenabled = true\n\
         [agent]\ndefault = \"codex\"\n\
         codex = [\"{command}\", \"--scenario\", \"normal\", \"--dump-request-params\", {dump:?}]\n",
        dir = data_dir.display(),
    );
    app::Config::from_toml(&text).expect("cwd 用例配置合法")
}

/// `--dump-request-params` 的记录行（每行一条 `{"method":…,"params":…}`，由真实子进程追加写出）。
///
/// 文件尚未创建（子进程还没收到任何报文）时返回空表——调用方一律轮询到出现目标方法再断言，因此
/// 「读空」不是通过、而是「还没到」。
fn ac1_dumped_request_rows(path: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .unwrap_or_else(|error| panic!("dump 行不是 JSON：{line}（{error}）"))
        })
        .collect()
}

/// tasks 3.2（WP4）：`session/new` 的出站 `cwd` 剥掉 Windows verbatim 前缀，而会话行的
/// `workspace_cwd` 仍是创建时交给 core 的 canonical 原文。
///
/// 可证伪点：
/// - 实现漏掉剥离（`session_new_params` 直发 `canonical_path()`）⇒ Windows 上线缆侧断言红
///   （出站取值带 `\\?\`，与持久化取值相同）；
/// - 实现把剥离**回写**持久化（写库用 `outbound_cwd` 的结果）⇒ 持久化侧断言红；
/// - 实现多剥/少剥（例如按字面量截前 4 字节而非按 `strip_verbatim` 规则）⇒ 两侧必有一条红；
/// - 一次 `session.create` 发出**多次** `session/new`（重试语义变化）⇒ 条数断言红。
#[test]
fn ac1_the_agent_receives_a_stripped_cwd_while_the_persisted_cwd_stays_canonical() {
    use acp_core::model::{
        AgentId, AgentRef, CreateSessionRequest, ResolvedWorkspace, ResourceOrigin,
    };

    support::block_on(async {
        let root = support::TempRoot::new("ac1-cwd");
        let data_dir = root.join("data");
        support::create_owner_only_dir(&data_dir);
        let fake = fake_acp_agent_binary();

        // 真实存在、可 `canonicalize` 的工作目录：core 的 workspace 解析与本用例的前置都要求它存在，
        // 且不是 `data_dir`（否则「两侧取值不同」的对照会被数据目录自己的形态搅浑）。
        let workspace_dir = root.join("workspace");
        support::create_owner_only_dir(&workspace_dir);
        let canonical = std::fs::canonicalize(&workspace_dir)
            .expect("工作目录可规范化")
            .to_str()
            .expect("路径是 UTF-8")
            .to_owned();

        // 前置（不成立即失败，绝不退化成恒真）：Windows 上 `canonicalize` 必须给出 verbatim 前缀，
        // 否则本用例根本证伪不了「剥前缀」这一条。
        #[cfg(windows)]
        assert!(
            canonical.starts_with(r"\\?\"),
            "前提：Windows 上 canonicalize 必须给出 \\\\?\\ 前缀（本用例的转换才有对象）；实际 {canonical}"
        );

        // 期望的出站取值：规则只由 core 的唯一实现给出（`acp_core::strip_verbatim`，WP1 已公开）；
        // 本用例不复制前缀判断。
        let expected_wire = acp_core::strip_verbatim(std::path::PathBuf::from(&canonical))
            .to_str()
            .expect("剥前缀后仍是 UTF-8")
            .to_owned();
        #[cfg(windows)]
        assert_ne!(
            expected_wire, canonical,
            "前提：Windows 上剥掉 \\\\?\\ 前缀必须真的改变取值（否则两侧断言同真而毫无信息）"
        );
        #[cfg(not(windows))]
        assert_eq!(
            expected_wire, canonical,
            "非 Windows 上 strip_verbatim 是逐字节直通，出站取值与持久化取值必须相同"
        );

        let dump_path = root.join("session-new-params.jsonl");
        let dump_arg = root.toml("session-new-params.jsonl");
        let (publisher, _fanout) = app::compose::forked_publisher();
        let composition = app::compose::Composition::assemble(
            ac1_config_with_request_dump(&data_dir, &fake.display().to_string(), &dump_arg),
            publisher,
        )
        .await
        .expect("组合根装配");
        // `ResolvedWorkspace` 直接给 canonical 原文：core 不重新解析，落盘与交给后端的是同一个取值。
        let agent =
            AgentRef::try_new(AgentId::new("codex").expect("agentId"), "Codex").expect("agent ref");
        let workspace = ResolvedWorkspace::try_new(
            acp_core::model::derive_directory_id(&canonical),
            canonical.clone(),
        )
        .expect("workspace");
        let request =
            CreateSessionRequest::new(agent, Some(workspace), None, ResourceOrigin::Local);
        let session =
            ac1_create_and_settle(&composition, ac1_digest("ac1-cwd-session-create"), request)
                .await;

        // ① 持久化侧：真实 SQLite 的 `owned_session.workspace_cwd`。受理提交只落幂等记录，会话行
        //    （含这一列）与终态在**同一次落账提交**里落盘；落账由组合根 runtime 上的异步提交任务完成，
        //    因此按 15 s 上限轮询到非空再断言。
        let pool = ac1_ro_pool(&data_dir).await;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let persisted: String = loop {
            let row: Option<Option<String>> = sqlx::query_scalar::<_, Option<String>>(
                "SELECT workspace_cwd FROM owned_session WHERE session_id = ?1",
            )
            .bind(session.as_str())
            .fetch_optional(&pool)
            .await
            .expect("查询 owned_session");
            if let Some(Some(cwd)) = row {
                break cwd;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "超时：会话 {session} 的 owned_session.workspace_cwd 未落盘（落账提交缺失）"
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        };
        assert_eq!(
            persisted, canonical,
            "持久化取值必须逐字节等于创建时交给 core 的 canonical 路径：\
             出站的剥前缀只作用于发往 Agent 的参数，不得回写 `owned_session.workspace_cwd`"
        );
        pool.close().await;

        // ② 线缆侧：假 Agent **实际收到**的 `session/new`（子进程写盘是异步的，同样轮询到就绪）。
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let rows: Vec<Value> = loop {
            let rows = ac1_dumped_request_rows(&dump_path);
            if rows.iter().any(|row| row["method"] == json!("session/new")) {
                break rows;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "超时：假 Agent 未收到 session/new；dump 现有 {} 行：{rows:?}",
                rows.len()
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        };
        let session_new: Vec<&Value> = rows
            .iter()
            .filter(|row| row["method"] == json!("session/new"))
            .collect();
        assert_eq!(
            session_new.len(),
            1,
            "一次 session.create 必须恰好发出一次 session/new（多于一次=重试语义变了，少于一次=上面已排除）；\
             dump 实际 {} 行",
            session_new.len()
        );
        let params = &session_new[0]["params"];
        let wire_cwd = params
            .get("cwd")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("session/new 的 params.cwd 缺失或不是字符串：{params}"));
        assert_eq!(
            wire_cwd, expected_wire,
            "发往 Agent 的 cwd 必须是剥掉 Windows verbatim 前缀（`\\\\?\\`）后的形式：\
             带前缀时 Agent 可能把它当路径片段（`?` 是 Windows 非法文件名字符）"
        );
        // Windows 上额外钉死「转换真的发生了」：出站取值与持久化取值必须是两个不同的字节串。
        #[cfg(windows)]
        assert_ne!(
            wire_cwd,
            canonical.as_str(),
            "Windows 上线缆侧取值不得与持久化取值相同（相同即意味着剥离没做）"
        );

        composition.host().shutdown_all().await;
        composition.close().await.expect("组合根可关闭");
    });
}
