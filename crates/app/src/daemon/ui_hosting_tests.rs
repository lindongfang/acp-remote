//! 前端静态面的组合根接线用例（`openspec/changes/sync-ui-hosting/tasks.md` 的 3.1–3.3）。
//!
//! 与 `sync_ingress_tests` 同一个理由：这些用例**经真实组合根**跑（`NetIngress::start`，与
//! `crate::daemon::run` 是同一个装配点），只调 `register_static` 会让「组合根忘了注册静态面」「注册顺序
//! 把协议端点顶掉」这一类回归逃掉。资源表本身（解析顺序、内容类型、安全响应头）由 `server` 侧的单测
//! 覆盖；这里钉的是**接线**：静态面真的挂上了、协议面没有被它改变、版本字段真的取自那份被托管的资源表。
//!
//! 对端复用 `sync_ingress_tests` 的极简客户端（`pub(super)`：同一个 crate 里只留一处 HTTP/WS 手写实现，
//! 不复制第二套长期维护）。
//!
//! 环境无关性：产物不入库，CI 的 Rust 作业没有 `clients/app/dist`，因此用例**不**断言「主检出一定有某
//! 个版本」——版本与占位入口两条语义按环境条件分支断言（见 [`the_reported_version_matches_the_build_kind`]），
//! 而「托管的资源来自嵌入表」这条对所有环境都成立。

use std::net::SocketAddr;
use std::sync::Arc;

use serde_json::Value;
use server::local_admin::{DaemonControl as _, LocalErrorCode};
use server::node_link::{WS_PATH as NODE_LINK_WS_PATH, WS_SUBPROTOCOL as NODE_LINK_WS_SUBPROTOCOL};
use server::sync::conn::{WS_PATH as SYNC_WS_PATH, WS_SUBPROTOCOL as SYNC_WS_SUBPROTOCOL};
use server::transport::local::InstanceId;
use server::transport::net::{EmbeddedUi, MissingFrontendBuild, UiAssetSource as _};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;

use super::sync_ingress_tests::{HOST, Harness, IO_TIMEOUT, RawResponse, WsClient, parse_response};
use super::{AppDaemonControl, DaemonError, NetworkError, ShutdownSignal};

/// 发一条 `GET` 并读全响应。
///
/// `Connection: close`：响应写完即关流，因此「读到 EOF」就是「响应读全」（与配对 POST 面的助手同款做法，
/// 静态面的响应体是定长的 `Bytes`，不带分块编码）。
async fn get(addr: SocketAddr, path: &str) -> RawResponse {
    let request = format!("GET {path} HTTP/1.1\r\nHost: {HOST}\r\nConnection: close\r\n\r\n");
    let mut stream = tokio::time::timeout(IO_TIMEOUT, TcpStream::connect(addr))
        .await
        .expect("连接 loopback 超时")
        .expect("连接 loopback");
    tokio::time::timeout(IO_TIMEOUT, stream.write_all(request.as_bytes()))
        .await
        .expect("写请求超时")
        .expect("写请求");
    let raw = tokio::time::timeout(IO_TIMEOUT, async {
        let mut raw = Vec::new();
        let _ = stream.read_to_end(&mut raw).await;
        raw
    })
    .await
    .expect("读响应超时");
    parse_response(&raw)
}

/// 3.1：根路径由**嵌入资源表**的入口文档接管（在静态面注册之前，这条路径是 404）。
///
/// 断言逐字比对响应体与 `EmbeddedUi::entry_document()`：这同时否证了「处理器从磁盘读」与「组合根注册的
/// 是另一份资源表」两种走样——浏览器拿到的字节就是本二进制里的那一份。
#[tokio::test]
async fn the_root_path_serves_the_embedded_entry_document() {
    let harness = Harness::start("ui-root").await;

    let response = get(harness.addr, "/").await;
    assert_eq!(
        response.status, 200,
        "根路径必须返回入口文档（注册静态面之前这里是 404）"
    );
    assert_eq!(
        response.header("content-type"),
        Some("text/html; charset=utf-8"),
        "入口文档必须按 HTML 下发"
    );
    let entry = EmbeddedUi::new().entry_document();
    assert_eq!(
        response.body.as_bytes(),
        entry.as_ref(),
        "响应体必须与嵌入资源表的入口文档逐字相同（托管的资源就是嵌入的那一份）"
    );

    harness.stop().await;
}

/// 3.1：静态面注册在协议端点**之后**，协议端点的行为因此一字不变。
///
/// 判据的**区分力**分布要写清楚：真正能抓住「静态面接管协议 path」的是**升级断言**——静态面只装在
/// `router.fallback` 上，一旦协议 path 不再被精确路由命中，升级就升不起来。后面那三条钉的是协议面拒绝
/// 响应的**形状**（明确 4xx、响应体不是入口文档、不带静态面的 CSP）；它们在「被接管」的形态下也会成立
/// （静态面对协议族保留前缀返回 404 且不加静态头），因此是形状断言，不是那条走样的判据。
#[tokio::test]
async fn registering_the_static_endpoint_leaves_the_protocol_endpoints_intact() {
    let harness = Harness::start("ui-protocol").await;

    // 两个协议族各升一次：`/sync/v1` 与 `/node-link/v1` 仍必须由各自的 subprotocol 命中精确路由。
    for (path, subprotocol, family) in [
        (SYNC_WS_PATH, SYNC_WS_SUBPROTOCOL, "Sync"),
        (NODE_LINK_WS_PATH, NODE_LINK_WS_SUBPROTOCOL, "Node Link"),
    ] {
        let upgraded = WsClient::try_upgrade(harness.addr, path, subprotocol, None).await;
        assert!(
            upgraded.is_ok(),
            "注册静态面之后 `{path}` 仍必须按 {family} 的 subprotocol 升级成功，实际：{:?}",
            upgraded.err()
        );
        drop(upgraded);
    }

    // 协议面的拒绝形状：非升级请求给出明确 4xx，**不是** 200 的入口文档，也不带静态面的响应头。
    for path in [SYNC_WS_PATH, NODE_LINK_WS_PATH] {
        let plain = get(harness.addr, path).await;
        assert!(
            (400..500).contains(&plain.status),
            "`{path}` 的非升级请求必须被协议面拒绝，实际状态：{}",
            plain.status
        );
        assert_ne!(
            plain.body.as_bytes(),
            EmbeddedUi::new().entry_document().as_ref(),
            "`{path}` 的响应体不是入口文档"
        );
        assert!(
            plain.header("content-security-policy").is_none(),
            "CSP 是静态面自己的响应头：`{path}` 上协议面的拒绝响应不该带它"
        );
    }

    harness.stop().await;
}

/// 3.2：`daemon.status` 直接回答前端构建版本，且取值取自**被托管的那份**嵌入资源表。
///
/// 「不新增方法」不在这里断言：方法集是机器合同，真正会因新增方法变红的是
/// `crates/server/tests/local_admin_schema_drift.rs`（`Method::ALL` ↔ schema 的 `methodName` enum）与
/// `crates/server/src/local_admin/method.rs` 的 `Method::ALL.len()` 计数断言；`npm run check:commands`
/// 只覆盖文档 §5.1 与词表（`localCapabilities`）之间的一致性（本变更只往既有 `result` 里加字段，
/// 见 `design.md` D5）。
#[tokio::test]
async fn daemon_status_reports_the_version_of_the_hosted_embedded_table() {
    let harness = Harness::start("ui-version").await;

    // 与 `crate::daemon::run` 同一个构造点：状态里的 `ui_build_version` 不是用例塞进去的，而是装配结果。
    let control = AppDaemonControl::new(
        harness.composition(),
        &InstanceId::generate(),
        harness.ingress(),
        Arc::new(ShutdownSignal::new()),
    );
    let status = control.status().await;

    assert_eq!(
        status.ui_build_version,
        EmbeddedUi::new().build_version(),
        "版本必须来自嵌入资源表（同源：`NetIngress::start` 从注册为静态面的那份表读）"
    );
    // 键**存在**必须单独断言：`serde_json` 对缺失键的索引返回 `Null`，而版本缺失（占位入口，CI 所处
    // 形态）时期望值同样是 `Null`，只看下面那条 `assert_eq!` 会在「键名写错/丢了字段」时恒过。
    assert!(
        status.to_json().contains_key("uiBuildVersion"),
        "`uiBuildVersion` 必须是响应里的一个键（camelCase，见 §5.2）"
    );
    // 字段名与缺失语义（`null`）是 `daemon.status` 的合同部分：`camelCase`，缺失即 `null`，不编造版本。
    assert_eq!(
        status.to_json()["uiBuildVersion"],
        match &status.ui_build_version {
            Some(version) => Value::from(version.as_str()),
            None => Value::Null,
        },
        "`uiBuildVersion` 的取值必须与字段一致，且 `None` 序列化为 `null`"
    );

    harness.stop().await;
}

/// 3.3：版本语义按环境条件分开断言——占位入口 **不报版本**，真实产物 **必须报出非空版本**。
///
/// 不无条件断言其中一条：产物不入库，开发机/CI 通常面对占位入口，而带 `clients/app/dist` 的检出面对真实
/// 产物。两条分支合起来正是「版本缺失可被检查出来、有产物时必须能回答版本」这条规格。
#[test]
fn the_reported_version_matches_the_build_kind() {
    let ui = EmbeddedUi::new();
    if ui.is_placeholder() {
        assert_eq!(
            ui.build_version(),
            None,
            "占位入口必须给出缺失语义（没有版本文件，MUST NOT 编造一个版本）"
        );
    } else {
        let version = ui
            .build_version()
            .expect("真实产物必须带可解析的 `build-version.json`（缺失即构建钩子缺陷）");
        assert!(
            !version.trim().is_empty(),
            "真实产物的版本不得为空白：{version:?}"
        );
        // 同源：版本文件与入口文档出自同一份资源表（同一份嵌入资源 → 同一次构建）。
        assert!(
            !ui.entry_document().is_empty(),
            "同一张资源表里的入口文档必须可读（版本与资源同源）"
        );
    }
}

/// 发布闸门的错误面向运维：必须能看到下一步（先在 `clients/app` 构建前端），并落在既有错误码上。
///
/// 闸门本身（`check_release_build`）与它的两个输入由 `server` 侧单测覆盖；这里锁的是**组合根这一侧的
/// 出口**：`daemon start` 失败时打印的那句话不能退化成一个内部错误码。
#[test]
fn the_release_gate_error_points_at_the_frontend_build_step() {
    let error = DaemonError::Network(NetworkError::MissingFrontendBuild(MissingFrontendBuild));
    let message = error.message();
    assert!(
        message.contains("npm run export:web"),
        "错误消息必须指出下一步，实际：{message}"
    );
    assert_eq!(
        error.code(),
        LocalErrorCode::Unavailable.as_str(),
        "发布闸门失败与其它启动失败同类：非零退出 + 既有本地错误码"
    );
}
