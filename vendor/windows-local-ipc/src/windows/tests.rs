//! Windows 本机用例（[PV5] 的 WP2 部分）：真实创建 Named Pipe、连接、查询对端 SID 与失败路径。
//!
//! 用例只用公开 API 加同 crate 的私有辅助（`create_pipe_server_for_sid` / `owner_only_sddl`），
//! 目的是让「创建时用的 SDDL 到底写了什么」和「失败路径留下什么」都可被断言，而不只是「看起来能跑」。
//!
//! 已知限制（如实记录，不能靠这些用例替代）：本机只有一个可用的 OS 账号，**跨用户连接被拒绝**这一条
//! 无法真实执行（需要第二个用户账号或另一台机器的会话）。本文件能做的是给出静态证据——生成的 SDDL 只
//! 含当前用户的 SID、不含任何组/Everyone 主体，并且 DACL 是 `P`（不继承）——加上「对端凭据查询失败即
//! 返回 `Err`」的失败关闭用例。真实跨用户拒绝属 [PV5] 在 WP3 的验收范围。

use std::io::ErrorKind;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{ClientOptions, ServerOptions};

use super::{
    client_user_sid, create_pipe_server, create_pipe_server_for_sid, current_user_sid,
    owner_only_sddl,
};

/// Named Pipe 命名空间是本机共享资源：每个用例取一个独占名字（进程 id + 递增序号 + 计时），
/// 这样同机其它进程、以及同一进程内并行的用例之间都不会互相撞名。
fn unique_pipe_name(label: &str) -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    format!(
        r"\\.\pipe\acp-remote-winlocalipc-{label}-{}-{seq}-{nanos}",
        std::process::id()
    )
}

/// 当前用户的 SID，取不到就直接失败（后续断言都以它为前提）。
fn sid_of_current_user() -> String {
    let sid = current_user_sid().expect("取当前进程用户 SID 应成功");
    assert!(sid.starts_with("S-1-"), "SID 应以 S-1- 开头，实际 {sid}");
    sid
}

/// 用例 ①：以 SDDL 创建的 pipe，当前用户进程能真实连接并完成一次字节往返。
///
/// 字节往返是必要的：只创建句柄不足以证明句柄可重叠（`FILE_FLAG_OVERLAPPED`）且被正确注册进 IOCP，
/// 而 tokio 的异步读写正是靠这两点。
#[tokio::test]
async fn owner_only_pipe_accepts_current_user_client() {
    let name = unique_pipe_name("accept");
    let server = create_pipe_server(&name).expect("以当前用户 SDDL 创建 pipe 应成功");
    let mut client = ClientOptions::new()
        .open(&name)
        .expect("同用户客户端连接应成功");
    server.connect().await.expect("服务端应完成连接");

    client.write_all(b"ping").await.expect("客户端写入应成功");
    let mut received = [0u8; 4];
    let mut server = server;
    server
        .read_exact(&mut received)
        .await
        .expect("服务端读取应成功");
    assert_eq!(&received, b"ping");
    println!("[PV5] pipe={name}：同用户连接 + 4 字节往返 OK");
}

/// 用例 ②：`client_user_sid` 的返回值等于 `current_user_sid()`（两层访问控制的输入一致）。
#[tokio::test]
async fn client_user_sid_matches_current_user_sid() {
    let name = unique_pipe_name("sid");
    let server = create_pipe_server(&name).expect("创建 pipe 应成功");
    let client = ClientOptions::new().open(&name).expect("客户端连接应成功");
    server.connect().await.expect("服务端应完成连接");

    let server_side = sid_of_current_user();
    let client_side = client_user_sid(&server).expect("取对端用户 SID 应成功");
    println!("[PV5] current_user_sid={server_side} client_user_sid={client_side}");
    assert_eq!(server_side, client_side, "同一用户的 SID 必须逐字相等");

    drop(client);
}

/// 用例 ③：生成 SDDL 的字符串断言（跨用户拒绝的**静态**证据）。
///
/// 它与真实跨用户拒绝测试的关系：这里证明交给 `CreateNamedPipeW` 的 DACL 只授权当前用户、只有一条 ACE、
/// 且不继承；本机没有第二个账号，因此「实际拒绝」只能由 WP3 的 [PV5] 轮次覆盖。
#[test]
fn owner_only_sddl_grants_only_current_user() {
    let sid = sid_of_current_user();
    let sddl = owner_only_sddl(&sid);
    println!("[PV5] 当前用户 SID = {sid}");
    println!("[PV5] 创建 pipe 时使用的 SDDL = {sddl}");

    assert_eq!(sddl, format!("D:P(A;;GA;;;{sid})"));
    assert_eq!(sddl.matches("(A;;").count(), 1, "只应有一条 ACE");
    assert_eq!(sddl.matches("S-1-").count(), 1, "只应出现一个 SID");
    for well_known in [
        ";;;WD", ";;;BU", ";;;AU", ";;;AN", ";;;BG", ";;;IU", ";;;SU", ";;;SY", ";;;BA", ";;;CO",
    ] {
        assert!(!sddl.contains(well_known), "不应给组主体 {well_known} 授权");
    }
}

/// 用例 ④-a：非法 SDDL（SID 不是合法 SID）必须被拒绝，错误来自 OS 而不是被当成空 DACL 继续。
#[test]
fn invalid_sddl_is_rejected() {
    let name = unique_pipe_name("bad-sddl");
    let error =
        create_pipe_server_for_sid(&name, "not-a-sid").expect_err("非法 SID 的 SDDL 必须创建失败");
    let code = error
        .raw_os_error()
        .expect("非法 SDDL 应由 OS 拒绝并给出错误码");
    println!("[PV5] 非法 SDDL 的错误 = {error:?}（raw_os_error = {code}）");
    // `ConvertStringSecurityDescriptorToSecurityDescriptorW` 在本机给出 ERROR_INVALID_ACL（1336）；
    // ERROR_INVALID_SID（1338）是同一族的另一种表述，两者都表示「这个 SDDL 没有被接受」。
    assert!(
        code == 1336 || code == 1338,
        "预期 ERROR_INVALID_ACL(1336) 或 ERROR_INVALID_SID(1338)，实际 {code}"
    );
}

/// 用例 ④-b：pipe 名含内嵌 NUL 必须被拒绝（否则 Win32 会在第一个 NUL 处静默截断名字）。
#[test]
fn pipe_name_with_inner_nul_is_rejected() {
    let error = create_pipe_server("\\\\.\\pipe\\acp-remote-winlocalipc\u{0}truncated")
        .expect_err("内嵌 NUL 的 pipe 名必须被拒绝");
    println!("[PV5] 内嵌 NUL 的错误 = {error:?}");
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
}

/// 用例 ④-c：同一 pipe 名的第二个「首实例」必须失败（`FILE_FLAG_FIRST_PIPE_INSTANCE` 真的生效）。
#[tokio::test]
async fn duplicate_first_instance_is_rejected() {
    let name = unique_pipe_name("duplicate");
    let _first = create_pipe_server(&name).expect("首个实例应创建成功");
    let error = create_pipe_server(&name).expect_err("同名的第二个首实例必须失败");
    println!(
        "[PV5] 重复首实例的错误 = {error:?}（raw_os_error = {:?}）",
        error.raw_os_error()
    );
    // ERROR_ACCESS_DENIED（5）：名字已存在时 `FILE_FLAG_FIRST_PIPE_INSTANCE` 的失败码。
    assert_eq!(error.raw_os_error(), Some(5));
}

/// 用例 ④-d：不在 `\\.\pipe\` 命名空间里的名字必须失败（不能悄悄创建一个别处的端点）。
#[tokio::test]
async fn pipe_name_outside_pipe_namespace_is_rejected() {
    let error = create_pipe_server("acp-remote-winlocalipc-no-namespace")
        .expect_err("缺少 \\\\.\\pipe\\ 前缀的名字必须失败");
    println!(
        "[PV5] 非 pipe 命名空间的错误 = {error:?}（raw_os_error = {:?}）",
        error.raw_os_error()
    );
    // ERROR_INVALID_NAME（123）。
    assert_eq!(error.raw_os_error(), Some(123));
}

/// 用例 ④-e：没有已连接的对端时 `client_user_sid` 必须返回 `Err`（不能给出一个「看起来像」的 SID）。
#[tokio::test]
async fn client_user_sid_requires_a_connected_client() {
    let name = unique_pipe_name("unconnected");
    let server = create_pipe_server(&name).expect("创建 pipe 应成功");
    let error = client_user_sid(&server).expect_err("未连接时不能给出对端 SID");
    println!(
        "[PV5] 未连接对端的错误 = {error:?}（raw_os_error = {:?}）",
        error.raw_os_error()
    );
}

/// 用例 ④-f：runtime 之外的调用必须 panic（与 tokio `ServerOptions::create` 同一约束），**且 panic 路径
/// 同样不留下已创建的 pipe 实例**。
///
/// 这是句柄所有权不变量的直接检验：`CreateNamedPipeW` 已经成功，出问题的是随后 tokio 包装句柄那一步。
/// 如果该路径漏掉句柄（泄漏），该 pipe 名的首实例仍然存在，下面第二次创建会拿到 ERROR_ACCESS_DENIED
/// （重复首实例）而不是成功。日志里那一行 panic 输出是本用例的预期产物，不代表失败（`test result` 行才是判据）。
#[test]
fn runtime_requirement_holds_without_leaking_the_pipe_instance() {
    let name = unique_pipe_name("release");
    let payload = std::panic::catch_unwind(|| create_pipe_server(&name))
        .expect_err("runtime 之外的调用必须 panic");
    let message = panic_message(payload.as_ref());
    println!("[PV5] runtime 之外的 panic = {message}");
    assert!(
        message.contains("no reactor running"),
        "panic 应来自 tokio 的 reactor 断言，实际：{message}"
    );

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
        .expect("构建带 I/O 的 current-thread runtime");
    let server = runtime
        .block_on(async { create_pipe_server(&name) })
        .expect("panic 路径不应留下已创建的 pipe 实例");
    drop(server);
}

/// 取 panic payload 的文本，用于断言它是 tokio 的 reactor 断言而不是别的 panic。
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "<非字符串 panic payload>".to_string()
    }
}

/// 用例 ④-g：同一进程能用 wrapper 创建多个**不同名字**的实例（`FILE_FLAG_FIRST_PIPE_INSTANCE`
/// 只约束同名实例），顺带覆盖「先创建的实例仍在」时的正常路径。
#[tokio::test]
async fn distinct_pipe_names_can_be_created_concurrently() {
    let first_name = unique_pipe_name("distinct-a");
    let second_name = unique_pipe_name("distinct-b");
    let first = create_pipe_server(&first_name).expect("第一个名字应成功");
    let second = create_pipe_server(&second_name).expect("第二个名字应成功");
    assert_ne!(first_name, second_name);
    drop(first);
    drop(second);
    println!("[PV5] 两个不同 pipe 名各自创建成功");
}

/// 用例 ④-h：`ServerOptions`（tokio safe API，WP3 创建后续实例的路径）能在 wrapper 已创建首实例的
/// 同一 pipe 名上再创建实例——这正是 WP3 监听循环的形状（首个实例带 DACL，后续实例沿用该名字的安全设置），
/// 也顺带证明两处 `nMaxInstances` 取值一致（不一致时 OS 会拒绝创建后续实例）。
#[tokio::test]
async fn subsequent_instance_via_tokio_safe_api_is_allowed() {
    let name = unique_pipe_name("subsequent");
    let first = create_pipe_server(&name).expect("首实例应成功");
    let subsequent = ServerOptions::new()
        .create(&name)
        .expect("同名的后续实例应能由 tokio safe API 创建");
    // 只留后续实例：两个实例同时监听时，客户端连到哪一个由 OS 决定，而 `connect()` 只会等自己那一个，
    // 留两个会让用例出现与断言无关的挂起风险。
    drop(first);

    let client = ClientOptions::new()
        .open(&name)
        .expect("客户端应能连到该 pipe 名");
    subsequent.connect().await.expect("后续实例应完成连接");
    drop(client);
    println!("[PV5] 后续实例（tokio safe API）创建 + 连接 OK");
}
