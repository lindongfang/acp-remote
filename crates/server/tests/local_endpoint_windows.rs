//! Windows 本机的真实 Named Pipe 用例（`[PV5]` 的 WP3a 部分）。
//!
//! 覆盖 R17/R18（pipe 名规则、真实创建）与 R20/R21（对端凭据校验、同用户连接被接受），并对 WP2 移交的
//! 未核实项给出结论：**同名后续实例经 tokio `ServerOptions` 创建后可连接**（继承与否对安全无伤，
//! 首实例的 SDDL 已限制创建者）。
//!
//! 已知限制（如实记录，不能用这些用例替代）：本机只有一个可用的 OS 账号，「另一个用户的连接被拒绝」
//! 无法真实执行；本文件给出的是「同用户被接受 + SID 逐字相等 + 同名第二实例可用」的正面证据，以及
//! 「拿不到对端 SID 即拒绝」的失败关闭路径（由 WP2 用例 ⑧ 覆盖）。真实跨用户拒绝的结论记在报告里。

#![cfg(windows)]

use std::sync::Arc;

use server::local_admin::{AdminOutcome, AdminResponse, UnroutedAdminHandler};
use server::transport::local::{
    CHANNEL_LOCAL_ADMIN_BYTE, CloseReason, EndpointError, InstanceId, LocalConnectionHandlers,
    LocalEndpoint, LocalEndpointConfig, LoggingAuditHook, named_pipe_path, serve_connection,
};
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::ClientOptions;

fn config() -> LocalEndpointConfig {
    LocalEndpointConfig {
        data_dir: std::env::temp_dir(),
        instance_id: InstanceId::generate(),
        runtime_dir: None,
    }
}

fn handlers() -> Arc<LocalConnectionHandlers> {
    Arc::new(LocalConnectionHandlers::new(Arc::new(UnroutedAdminHandler)))
}

async fn bind(config: LocalEndpointConfig) -> LocalEndpoint {
    LocalEndpoint::bind(config, Arc::new(LoggingAuditHook))
        .await
        .expect("本机创建 Named Pipe endpoint 应成功")
}

/// 一帧：`u32be payload 长度 | channel | body`。
fn frame(body: &[u8]) -> Vec<u8> {
    let length = u32::try_from(body.len() + 1).expect("用例 payload 很小");
    let mut out = Vec::with_capacity(4 + body.len() + 1);
    out.extend_from_slice(&length.to_be_bytes());
    out.push(CHANNEL_LOCAL_ADMIN_BYTE);
    out.extend_from_slice(body);
    out
}

/// pipe 名必须逐字等于 §2.1 的规则（输入是当前用户 SID 字符串的 UTF-8 字节）。
#[tokio::test]
async fn pipe_name_matches_the_endpoint_rule_on_this_host() {
    let config = config();
    let endpoint = bind(config.clone()).await;
    let sid = windows_local_ipc::current_user_sid().expect("取当前用户 SID");
    assert!(sid.starts_with("S-1-"), "SID 应以 S-1- 开头，实际 {sid}");

    let digest = Sha256::digest(sid.as_bytes());
    let expected_hash: String = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(expected_hash.len(), 16);

    let expected = named_pipe_path(&sid, &config.instance_id);
    assert_eq!(endpoint.describe(), expected);
    assert_eq!(
        endpoint.describe(),
        format!(
            r"\\.\pipe\acp-remote-{expected_hash}-{}",
            config.instance_id.as_str()
        )
    );
    println!(
        "[PV5] pipe={} sid_hash={expected_hash} instance={}",
        endpoint.describe(),
        config.instance_id.as_str()
    );
}

/// 同用户客户端经真实 pipe 连接 → 通过两层访问控制 → 管理请求得到恰好一个响应。
#[tokio::test]
async fn same_user_client_is_accepted_and_admin_requests_are_answered() {
    let config = config();
    let mut endpoint = bind(config.clone()).await;
    let pipe_name = endpoint.describe();

    let mut client = ClientOptions::new()
        .open(&pipe_name)
        .expect("同用户客户端连接应成功");
    let stream = endpoint
        .accept()
        .await
        .expect("同用户连接必须通过 endpoint 权限与对端凭据校验");
    let server = tokio::spawn(serve_connection(stream, handlers()));

    let body = br#"{"v":1,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","method":"daemon.status","params":{}}"#;
    client
        .write_all(&frame(body))
        .await
        .expect("客户端写入请求");
    let mut header = [0u8; 4];
    client.read_exact(&mut header).await.expect("读取响应帧头");
    let length = usize::try_from(u32::from_be_bytes(header)).expect("32 位长度");
    let mut payload = vec![0u8; length];
    client.read_exact(&mut payload).await.expect("读取响应帧");
    assert_eq!(payload[0], CHANNEL_LOCAL_ADMIN_BYTE);
    let response = AdminResponse::decode(&payload[1..]).expect("响应可解码");
    assert_eq!(
        response.id().as_str(),
        "5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10"
    );
    assert!(matches!(
        response.outcome(),
        AdminOutcome::Failure { error } if error.code().as_str() == "local.unsupported"
    ));

    drop(client);
    assert_eq!(
        server.await.expect("服务端任务").expect("无连接级错误"),
        CloseReason::PeerEof
    );
}

/// 同名后续实例（tokio `ServerOptions`）：首实例经 wrapper 带 SDDL 创建，后续实例照旧可创建、可连接、
/// 可通过凭据校验——这就是 WP2 移交的「后续实例 DACL 继承与否」未核实项的结论（可接受）。
#[tokio::test]
async fn subsequent_pipe_instances_are_created_and_accept_same_user_clients() {
    let config = config();
    let mut endpoint = bind(config).await;
    let pipe_name = endpoint.describe();

    for round in 0..3 {
        let mut client = ClientOptions::new()
            .open(&pipe_name)
            .unwrap_or_else(|error| panic!("第 {round} 轮客户端连接失败：{error}"));
        let stream = endpoint
            .accept()
            .await
            .unwrap_or_else(|error| panic!("第 {round} 轮 accept 失败：{error}"));
        client
            .write_all(&frame(br#"{"v":1,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","method":"daemon.status","params":{}}"#))
            .await
            .expect("写入请求");
        let server = tokio::spawn(serve_connection(stream, handlers()));
        let mut byte = [0u8; 1];
        let read = client.read(&mut byte).await.expect("读取响应");
        assert!(read > 0, "第 {round} 轮必须得到响应");
        drop(client);
        let _ = server.await;
    }
    println!("[PV5] 同名后续 pipe 实例：连续 3 轮创建 + 同用户连接 + 凭据校验通过");
}

/// 同一 endpoint 起两次是明确错误（`FILE_FLAG_FIRST_PIPE_INSTANCE`）→ 失败关闭。
#[tokio::test]
async fn a_second_endpoint_on_the_same_name_fails_closed() {
    let config = config();
    let _first = bind(config.clone()).await;
    let second = LocalEndpoint::bind(config, Arc::new(LoggingAuditHook)).await;
    assert!(
        matches!(second, Err(EndpointError::AlreadyInUse)),
        "同名第二个首实例应返回 AlreadyInUse，实际 {:?}",
        second.err().map(|error| error.to_string())
    );
}
