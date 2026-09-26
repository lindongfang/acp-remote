//! `daemon-lifecycle` 规格的进程级验收（R1–R16 的可判定部分）。
//!
//! 每个用例都启动**真实二进制**（`CARGO_BIN_EXE_acp-remote`）并使用独立临时 `dataDir`：
//! 单实例锁、endpoint 创建、进程退出码、关闭序列顺序只有跨进程才能验证（`support/mod.rs` 的模块注释）。
//! R 号引用 `openspec/changes/daemon-cli-and-local-admin/plan.md` §4.1 与
//! `specs/daemon-lifecycle/spec.md` 的验收条目。

mod support;

use std::net::TcpStream;
use std::time::{Duration, Instant};

use app::{ClientOutcome, DaemonLock, outcome_of};
use serde_json::{Value, json};
use server::local_admin::Method;
#[cfg(unix)]
use support::run_start_once_with_env;
use support::{Daemon, Stdin, failure_code, params, params_of, run_cli, run_start_once};

/// 一个指向真实可执行文件的 Agent profile：`daemon.status.agents[].available` 因此为 `true`
/// （`design.md` 决策 6：「profile 存在且 command 可解析」）。
fn seed_profile(agent_id: &str, default: bool) -> String {
    format!(
        r#"[[agents.profiles]]
agent_id = "{agent_id}"
display_name = "{agent_id} (seed)"
command = "{command}"
args = []
env_allowlist = []
default = {default}
"#,
        command = env!("CARGO_BIN_EXE_acp-remote").replace('\\', "/"),
    )
}

/// `agent.configure` 的参数（用真实可执行文件，保证 `available` 为 `true`）。
fn agent_params(agent_id: &str) -> server::local_admin::JsonObject {
    params_of(json!({
        "agentId": agent_id,
        "displayName": agent_id,
        "command": env!("CARGO_BIN_EXE_acp-remote"),
        "args": [],
        "envAllowlist": [],
        "default": false,
    }))
}

/// R1/R2/R5：全新数据目录 → 取锁、生成 16 字符小写 hex 的 instanceId、创建可用 endpoint、
/// 首次种子导入写入 profile，`daemon.status` 的 §5.2 字段齐全且与本机身份同源。
#[test]
fn a_fresh_start_takes_the_lock_creates_the_endpoint_and_imports_the_seeds() {
    let mut daemon = Daemon::configure("fresh-start", &seed_profile("codex", true));
    daemon.start();

    // R1：单实例锁文件在数据目录里，运行记录与状态里的 instanceId 一致。
    let lock_path = daemon.data_dir().join(app::lock::LOCK_FILE_NAME);
    assert!(
        lock_path.is_file(),
        "锁文件必须存在：{}",
        lock_path.display()
    );
    let record = daemon.record();
    assert_eq!(record.instance_id.len(), 16);
    assert!(
        record
            .instance_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "instanceId 必须是 16 字符小写 hex：{}",
        record.instance_id
    );
    assert_eq!(record.pid, daemon.pid());
    assert!(record.endpoint.is_some(), "运行记录必须给出 endpoint");

    let status = daemon.ok_value(Method::DaemonStatus, params());
    assert_eq!(status["instanceId"], json!(record.instance_id));
    assert_eq!(status["version"], json!(env!("CARGO_PKG_VERSION")));
    // `dataDir` 回显配置给出的文本（Windows 上接受正斜杠），因此比对规范化后的绝对路径。
    let reported = std::fs::canonicalize(status["dataDir"].as_str().expect("dataDir"))
        .expect("dataDir 必须存在");
    assert_eq!(
        reported,
        std::fs::canonicalize(daemon.data_dir()).expect("数据目录必须存在")
    );
    assert_eq!(
        status["publicOrigin"],
        json!("https://acpr-test.example.invalid")
    );
    // `listen` 是实际绑定的网络监听地址（用例的配置是 `127.0.0.1:0`，因此端口由内核分配）：
    // 既要与配置同源（loopback），也要真的可达——不是只写在 status 里的字面量。
    let listen = status["listen"].as_array().expect("listen 是数组");
    assert_eq!(
        listen.len(),
        1,
        "共享 listener 必须且只能报告一个绑定地址：{listen:?}"
    );
    let addr: std::net::SocketAddr = listen[0]
        .as_str()
        .expect("listen 项是文本")
        .parse()
        .expect("listen 项是 ip:port");
    assert!(addr.ip().is_loopback(), "{addr}");
    std::net::TcpStream::connect(addr).expect("status 报告的监听地址必须真的可达");
    assert_eq!(
        status["links"],
        json!([]),
        "本切片没有 Node Link 连接管理器"
    );
    assert!(
        status["uptimeMs"].as_u64().expect("uptimeMs") < 60_000,
        "刚启动的 uptimeMs 必须是毫秒数"
    );

    // 节点身份：nodeId 是本机派生的 UUID 文本，nodePublicKey 是 65 字节未压缩 P-256 公钥的
    // 无填充 base64url（87 字符）。
    let node_id = status["nodeId"].as_str().expect("nodeId");
    assert_eq!(
        node_id.split('-').map(str::len).collect::<Vec<_>>(),
        vec![8, 4, 4, 4, 12]
    );
    let public_key = status["nodePublicKey"].as_str().expect("nodePublicKey");
    assert_eq!(public_key.len(), 87);
    assert!(!public_key.contains('='));
    assert!(!public_key.contains('+'));

    // R5：首次种子导入写入 profile（同一事务里也写了「已初始化」标记；标记的效果由
    // `local_configuration_survives_a_restart_and_seeds_are_not_reimported` 判定）。
    assert_eq!(status["counts"]["devices"], json!(0));
    assert_eq!(status["counts"]["nodes"], json!(0));
    assert_eq!(status["counts"]["exports"], json!(0));
    assert_eq!(status["counts"]["imports"], json!(0));
    assert_eq!(
        status["agents"],
        json!([{ "agentId": "codex", "available": true }])
    );

    // 就绪日志与 endpoint 定位串一致（CLI 依赖运行记录里的同一串）。
    let ready = daemon.log_events("daemon.ready");
    assert_eq!(ready.len(), 1, "恰好一次 daemon.ready：{}", daemon.log());
    assert_eq!(
        ready[0]["endpoint"],
        json!(daemon.endpoint()),
        "日志里的 endpoint 必须与运行记录一致"
    );

    assert!(daemon.stop().success(), "daemon.stop 后必须正常退出");
}

/// R3：第二个 `daemon start` 必须被拒绝（`local.conflict`），且不影响正在运行的实例。
#[test]
fn a_second_start_is_rejected_with_local_conflict_and_leaves_the_first_running() {
    let mut daemon = Daemon::configure("duplicate-start", &seed_profile("codex", true));
    daemon.start();
    let first_instance = daemon.instance_id().to_owned();

    let (status, stdout, stderr) = run_start_once(daemon.config_path());
    assert!(!status.success(), "重复启动必须失败：{stdout}{stderr}");
    assert_eq!(failure_code(&stderr), "local.conflict");
    assert!(
        stderr
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count()
            == 1,
        "stderr 必须恰好一行结构化 JSON：{stderr}"
    );

    // 第一个实例未受影响：仍在运行、instanceId 不变、可继续回答请求。
    assert!(daemon.is_running(), "重复启动不得终止已有实例");
    assert_eq!(daemon.instance_id(), first_instance);
    let status = daemon.ok_value(Method::DaemonStatus, params());
    assert_eq!(status["instanceId"], json!(first_instance));

    // 重复启动的进程不得写坏运行记录或抢占 endpoint。
    assert_eq!(daemon.record().instance_id, first_instance);
    assert!(daemon.stop().success());
}

/// R4（Unix）：endpoint 路径被不安全对象占用 → 拒绝启动；且失败后锁已释放（下一次正常启动成功）。
#[cfg(unix)]
#[test]
fn an_unusable_endpoint_refuses_start_without_degrading() {
    let mut daemon = Daemon::configure("bad-endpoint", &seed_profile("codex", true));
    // `$XDG_RUNTIME_DIR/acp-remote` 是符号链接 → endpoint 路径不安全（§2.1），必须拒绝启动。
    let xdg = daemon.root().join("runtime");
    let elsewhere = daemon.root().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("目录");
    std::os::unix::fs::symlink(&elsewhere, xdg.join("acp-remote")).expect("符号链接");

    // `run_start_once` 不会继承 `Daemon::spawn` 的 `XDG_RUNTIME_DIR`：必须显式传入，
    // 否则 daemon 回落 `<data_dir>/run`，根本碰不到这个符号链接（在 CI 上挂死的根因）。
    let (status, _stdout, stderr) =
        run_start_once_with_env(daemon.config_path(), &[("XDG_RUNTIME_DIR", xdg.as_path())]);
    assert!(!status.success(), "endpoint 不可用必须拒绝启动：{stderr}");
    assert_eq!(failure_code(&stderr), "local.unavailable");
    // 失败关闭而不是降级：不留下运行记录，也没有「无管理通道仍在跑」的进程。
    assert!(
        app::read_record(&daemon.data_dir().join(app::lock::RECORD_FILE_NAME))
            .expect("读取")
            .is_none(),
        "拒绝启动不得发布运行记录"
    );

    // 锁已释放：同一数据目录的下一次启动必须成功（证明失败路径没有留下锁）。
    std::fs::remove_file(xdg.join("acp-remote")).expect("删除符号链接");
    daemon.start();
    assert!(daemon.stop().success());
}

/// R4（跨平台）：本切片无法创建显式 endpoint → 在**任何副作用之前**失败关闭，不静默按 `auto` 启动。
#[test]
fn an_explicit_endpoint_configuration_is_refused_before_any_side_effect() {
    let daemon = Daemon::configure_with(
        "explicit-endpoint",
        "[daemon.local_admin]\nendpoint = '\\.\\pipe\\acpr-wp4a-explicit'\n",
        &seed_profile("codex", true),
    );
    let (status, _stdout, stderr) = run_start_once(daemon.config_path());
    assert!(!status.success(), "显式 endpoint 必须拒绝启动：{stderr}");
    assert_eq!(failure_code(&stderr), "local.invalid_params");
    assert!(
        !daemon.data_dir().exists(),
        "配置阶段失败不得创建数据目录（无副作用）"
    );
    assert!(daemon.log().is_empty(), "配置阶段失败不得写日志文件");
}

/// R6/R7：重启不覆盖运行期本地变更，也不重复导入种子。
#[test]
fn local_configuration_survives_a_restart_and_seeds_are_not_reimported() {
    let mut daemon = Daemon::configure("restart", &seed_profile("codex", true));
    daemon.start();

    // 运行期新增第二个 profile（不是种子；模拟 CLI 的 `agent.configure`）。
    let added = daemon.ok_value(Method::AgentConfigure, agent_params("helper"));
    assert_eq!(added["agent"]["agentId"], json!("helper"));
    let before = daemon.ok_value(Method::DaemonStatus, params());
    assert_eq!(
        before["agents"],
        json!([
            { "agentId": "codex", "available": true },
            { "agentId": "helper", "available": true }
        ]),
        "运行期新增的 profile 必须立刻可见"
    );

    assert!(daemon.stop().success());

    // 用**同一份配置与数据目录**重启：种子导入必须被「已初始化」标记挡住。
    let mut second = daemon.configure_in("second", &seed_profile("codex", true));
    second.start();
    assert_eq!(
        second.log_events("daemon.seed_skipped").len(),
        1,
        "重启必须跳过种子导入：{}",
        second.log()
    );
    let after = second.ok_value(Method::DaemonStatus, params());
    assert_eq!(
        after["agents"],
        json!([
            { "agentId": "codex", "available": true },
            { "agentId": "helper", "available": true }
        ]),
        "运行期本地变更必须保留，种子不得覆盖"
    );
    assert!(second.stop().success());
}

/// R8/R9：`daemon.status.counts` 与持久记录一致（用 `export.list`/`device.list`/`import.list` 交叉验证）。
#[test]
fn status_counts_follow_persisted_records() {
    let mut daemon = Daemon::configure("counts", &seed_profile("codex", true));
    daemon.start();

    let workspace_root = daemon.root().join("workspace");
    std::fs::create_dir_all(&workspace_root).expect("工作区目录");
    daemon.ok(
        Method::WorkspaceSelect,
        params_of(json!({
            "alias": "project",
            "displayName": "Project",
            "rootPath": workspace_root.display().to_string(),
        })),
    );
    daemon.ok(
        Method::ExportCreate,
        params_of(json!({
            "exportId": "export-1",
            "displayName": "Export One",
            "agentIds": ["codex"],
            "workspaceAliases": [{ "alias": "project", "displayName": "Project" }],
            "defaultWorkspaceAlias": "project",
            "templates": [{
                "templateId": "template-1",
                "displayName": "Template One",
                "workspaceAlias": "project",
                "params": [],
            }],
            "defaultTemplateId": "template-1",
            "scopes": ["grant.session.read"],
            "cachePolicy": "no-content-cache",
        })),
    );

    let status = daemon.ok_value(Method::DaemonStatus, params());
    let exports = daemon.ok_value(Method::ExportList, params());
    let devices = daemon.ok_value(Method::DeviceList, params());
    let imports = daemon.ok_value(Method::ImportList, params());
    assert_eq!(
        status["counts"]["exports"].as_u64().expect("exports"),
        exports["exports"].as_array().expect("exports 数组").len() as u64
    );
    assert_eq!(status["counts"]["exports"], json!(1));
    assert_eq!(
        status["counts"]["devices"].as_u64().expect("devices"),
        devices["devices"].as_array().expect("devices 数组").len() as u64
    );
    assert_eq!(
        status["counts"]["imports"].as_u64().expect("imports"),
        imports["imports"].as_array().expect("imports 数组").len() as u64
    );
    assert_eq!(status["counts"]["nodes"], json!(0));

    assert!(daemon.stop().success());
}

/// R13 的补充（task 2.26）：**没有其他客户端在途**时，`daemon stop` 不必等满宽限。
///
/// 钉住的是本机实测到的真实缺口：CLI 发出 `daemon.stop` 后就 `drop(client)` 并进入等锁循环，但
/// Windows 上 Named Pipe 句柄由挂起的 overlapped 读持有，句柄只在**驱动它的 Tokio runtime** 被驱动或销毁时
/// 才真正 `CloseHandle`；等锁循环全程 `std::thread::sleep`，从不驱动那个被缓存的 current_thread runtime，
/// 于是句柄一直开到 CLI 进程退出——在那之前 Daemon 的 `drain_connections` 看不到对端 EOF，只能等满
/// `shutdown_grace_ms`（实测 `drain_timeout{remaining:1}` 与 `elapsed_ms ≈ grace_ms`）。
///
/// 阈值取宽限的 2/3：本机实测停止耗时 < 200 ms，而「等满宽限」是 3000 ms，两者相差一个数量级，
/// 1 s 的低限同时保证断言在慢机器上不抖动。
#[test]
fn stop_does_not_wait_for_the_full_grace_without_other_clients() {
    const GRACE_MS: u128 = 3000;
    let mut daemon = Daemon::configure("stop-drain", "");
    daemon.start();
    let lock_path = daemon.data_dir().join(app::lock::LOCK_FILE_NAME);
    let grace_arg = GRACE_MS.to_string();

    let started = Instant::now();
    let run = run_cli(
        "daemon-stop-drain",
        Some(daemon.config_path()),
        &["daemon", "stop", "--grace-ms", grace_arg.as_str()],
        Stdin::Null,
    );
    let elapsed = started.elapsed();
    run.assert_success();
    assert!(run.stdout.contains("已停止"), "{}", run.stdout);

    // 锁已释放：CLI 返回时关闭序列已经完成（R13）。
    let lock = DaemonLock::acquire(&lock_path).expect("退出后锁必须可再取");
    drop(lock);
    let status = daemon.wait_exit();
    assert!(status.success(), "关闭序列完成后必须正常退出：{status}");

    // 排空窗口**没有**走到超时分支：Daemon 在宽限内看到 CLI 那条连接结束。
    assert!(
        daemon.log_events("daemon.drain_timeout").is_empty(),
        "无其他客户端在途时不得出现排空超时：日志={}",
        daemon.log()
    );
    assert!(
        elapsed < Duration::from_millis(u64::try_from(GRACE_MS * 2 / 3).expect("阈值")),
        "无其他客户端在途时 `daemon stop` 必须明显早于宽限完成：elapsed_ms={} grace_ms={GRACE_MS} 日志={}",
        elapsed.as_millis(),
        daemon.log()
    );
}

/// R12/R13/R14：`daemon.stop` 先回答 `accepted`，随后按顺序关闭；关闭期间的在途连接得到
/// `local.unavailable` 且不产生任何状态变更；退出后锁释放、WAL 已被 checkpoint。
#[test]
fn stop_is_accepted_first_and_requests_during_shutdown_are_unavailable() {
    let mut daemon = Daemon::configure_with(
        "stop-order",
        "shutdown_grace_ms = 1500\n",
        &seed_profile("codex", true),
    );
    daemon.start();

    // 预先建立一条空闲连接：关闭开始后它仍由已接受的连接处理器持有（在途连接）。
    let mut held = daemon.open_connection();
    assert!(matches!(
        outcome_of(&held.call(Method::DaemonStatus, params())),
        ClientOutcome::Success(_)
    ));

    // R12：`daemon.stop` 的响应**先于**关闭序列的副作用写回，`accepted` 恒为 true。
    let response = daemon.call(Method::DaemonStop, support::stop_params());
    match outcome_of(&response) {
        ClientOutcome::Success(result) => {
            assert_eq!(result.get("accepted"), Some(&Value::Bool(true)))
        }
        ClientOutcome::Failure { code, message } => {
            panic!("daemon.stop 必须成功，实际 {code}：{message}")
        }
    }

    // R14：关闭期间的在途连接必须得到 `local.unavailable`，且不得产生状态变更。
    //
    // 关闭序列会在宽限期内先排空在途连接再释放 endpoint，因此这里重试到**观测到**门闸的拒绝：
    // 两种可接受结果分别是「收到 local.unavailable」（门闸在结束前作答）与「连接被关闭/无法再连」
    // （endpoint 已释放）。两者之外的任何结果都是失败。
    let mut observed_unavailable = false;
    let mut observed_closed = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline && !observed_unavailable {
        match held.try_call(Method::AgentConfigure, agent_params("ghost")) {
            Ok(response) => match outcome_of(&response) {
                ClientOutcome::Failure { code, .. } => {
                    assert_eq!(code, "local.unavailable");
                    observed_unavailable = true;
                }
                ClientOutcome::Success(result) => {
                    panic!("关闭期间的请求必须失败，实际返回 {result:?}")
                }
            },
            Err(error) => {
                assert!(
                    matches!(error, app::client::ClientError::Connect { .. }),
                    "在途连接不应以传输错误结束：{error}"
                );
                observed_closed = true;
                break;
            }
        }
    }
    assert!(
        observed_unavailable || observed_closed,
        "必须在关闭窗口内观测到拒绝或 endpoint 关闭"
    );
    assert!(
        daemon.log_events("daemon.ingress_stopped").len() == 1,
        "关闭序列必须已经开始：{}",
        daemon.log()
    );
    held.close();

    let status = daemon.wait_exit();
    assert!(status.success(), "关闭序列完成后必须正常退出：{status}");

    // R13：关闭顺序（§12.1）：停接入层 → 取消后台任务 → 停 Agent → 刷新存储 → 释放锁。
    let order: Vec<usize> = [
        "daemon.shutdown_begin",
        "daemon.ingress_stopped",
        "daemon.task_stopped",
        "daemon.agents_stopped",
        "daemon.storage_closed",
        "daemon.stopped",
    ]
    .iter()
    .map(|event| {
        daemon
            .log_index(event)
            .unwrap_or_else(|| panic!("缺少 {event}：{}", daemon.log()))
    })
    .collect();
    assert!(
        order.windows(2).all(|pair| pair[0] <= pair[1]),
        "关闭顺序不符合 §12.1：{order:?}\n{}",
        daemon.log()
    );
    assert!(daemon.log_index("daemon.stop_accepted").expect("已接受") < order[1]);

    // 锁已释放：同一数据目录可以再取锁（进程退出后必须有可重入的锁）。
    let lock_path = daemon.data_dir().join(app::lock::LOCK_FILE_NAME);
    let lock = DaemonLock::acquire(&lock_path).expect("退出后锁必须可再取");
    let first_instance = daemon.instance_id().to_owned();
    drop(lock);

    // WAL 已被 checkpoint(TRUNCATE)：`-wal` 文件为空或不存在。
    let wal = daemon.data_dir().join("acp-remote.sqlite-wal");
    if wal.exists() {
        assert_eq!(
            std::fs::metadata(&wal).expect("元数据").len(),
            0,
            "关闭序列必须把 WAL 截断为 0 字节"
        );
    }
    // 运行记录在正常关闭时被清理（锁文件常驻）。
    assert!(
        app::read_record(&daemon.data_dir().join(app::lock::RECORD_FILE_NAME))
            .expect("读取")
            .is_none(),
        "正常关闭必须清理运行记录"
    );

    // R14 的「无状态变更」：重启后 `ghost`/`ghost-2` 都不存在。
    let mut restarted = daemon.configure_in("after-stop", &seed_profile("codex", true));
    restarted.start();
    let agents = restarted.ok_value(Method::DaemonStatus, params());
    assert_eq!(
        agents["agents"],
        json!([{ "agentId": "codex", "available": true }]),
        "关闭期间被拒绝的请求不得留下任何状态"
    );
    assert_ne!(
        restarted.instance_id(),
        first_instance,
        "每次运行的 instanceId 必须不同"
    );
    assert!(restarted.stop().success());
}

/// R12/R13 + RV1-WP7-F2：`daemon.stop` 返回 accepted 之后，网络 listener 必须**已停止 accept**。
///
/// 可观测的后果是「新连接不再被服务」：本地排空窗口内进程仍存活（下面用地一条本地长连接把它撞开），
/// 而监听地址在这整个窗口里等不到任何 HTTP 响应。旧实现把触发推到了 `await` 点（本地排空之后），
/// 因此同一条请求在当时会被正常作答。
///
/// 不用「TCP 连接被拒」做判据：应用的 accept 循环停下后监听套接字仍然打开（在途连接排空期间由
/// `axum::serve` 持有），内核 backlog 会照常完成握手——`connect` 仍会成功，它不能区分「已停 accept」
/// 与「仍在 accept」。
#[test]
fn the_network_listener_stops_serving_new_connections_before_the_local_drain_finishes() {
    let mut daemon = Daemon::configure_with("stop-accept", "shutdown_grace_ms = 5000\n", "");
    daemon.start();
    let status = daemon.ok_value(Method::DaemonStatus, params());
    let listen = status["listen"].as_array().expect("listen 是数组");
    assert_eq!(listen.len(), 1, "共享 listener 只报告一个地址：{listen:?}");
    let addr: std::net::SocketAddr = listen[0]
        .as_str()
        .expect("listen 项是文本")
        .parse()
        .expect("listen 项是 ip:port");
    assert!(
        probe_http(addr, Duration::from_secs(2)),
        "就绪后的监听地址必须真的服务请求"
    );

    // 在途本地连接：关闭序列的本地排空必须等它结束，Daemon 因此在断言期间一直存活。
    let mut held = daemon.open_connection();
    assert!(matches!(
        outcome_of(&held.call(Method::DaemonStatus, params())),
        ClientOutcome::Success(_)
    ));
    let response = daemon.call(Method::DaemonStop, support::stop_params());
    assert!(
        matches!(outcome_of(&response), ClientOutcome::Success(_)),
        "daemon.stop 必须被接受"
    );

    // 关闭序列必须先宣布停接入层（该日志在**同步**触发之后，见 `close` 的步骤 1）。
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && daemon.log_events("daemon.ingress_stopped").is_empty() {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        daemon.log_events("daemon.ingress_stopped").len(),
        1,
        "关闭序列必须已经开始：{}",
        daemon.log()
    );
    assert!(
        !probe_http(addr, Duration::from_millis(500)),
        "停接入层之后不得再服务新连接（本地排空窗口内 listener 仍在 accept）：{}",
        daemon.log()
    );
    assert!(
        daemon.is_running(),
        "拒绝必须是停 accept 的结果，而不是进程已退出"
    );

    held.close();
    assert!(daemon.wait_exit().success(), "关闭序列完成后必须正常退出");
}

/// 向监听地址发一条最小 HTTP 请求，返回「是否收到响应」。连接失败、写入失败与等不到任何字节都算
/// 「没有响应」（停 accept 之后三种情形都会出现：套接字已关闭、backlog 里的连接无人认领）。
fn probe_http(addr: std::net::SocketAddr, wait: Duration) -> bool {
    use std::io::{Read as _, Write as _};
    let Ok(mut stream) = TcpStream::connect(addr) else {
        return false;
    };
    stream.set_read_timeout(Some(wait)).expect("读超时");
    stream.set_write_timeout(Some(wait)).expect("写超时");
    let request = format!(
        "GET / HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        support::nodelink::PUBLIC_HOST
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut buffer = [0u8; 64];
    matches!(stream.read(&mut buffer), Ok(read) if read > 0)
}

/// R15/R16：周期任务至少在启动时跑过一次，并在关闭序列里被取消（不遗留 detached task）。
#[test]
fn the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown() {
    let mut daemon = Daemon::configure("maintenance", &seed_profile("codex", true));
    daemon.start();

    // R15：首轮清理在启动时立即执行（`reason = "startup"`），不是等到 60s 后。
    let ticks = daemon.log_events("daemon.maintenance");
    assert!(
        !ticks.is_empty(),
        "启动后必须已经跑过一轮清理：{}",
        daemon.log()
    );
    assert_eq!(ticks[0]["reason"], json!("startup"));
    assert!(ticks[0]["removed_events"].as_u64().is_some());
    assert!(ticks[0]["expired_pairings"].as_u64().is_some());
    assert!(ticks[0]["swept_orphans"].as_u64().is_some());

    let started = std::time::Instant::now();
    assert!(daemon.stop().success());
    // R16：取消发生在关闭序列内（`daemon.task_stopped`），且不必等 60s 的周期。
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "关闭不得等待下一个 60s 周期（实际 {:?}）",
        started.elapsed()
    );
    let stopped = daemon.log_events("daemon.task_stopped");
    let names: Vec<&str> = stopped
        .iter()
        .filter_map(|value| value["task"].as_str())
        .collect();
    assert!(
        names.contains(&"maintenance"),
        "维护任务必须在关闭序列被取消：{:?}",
        daemon.log()
    );
    assert!(names.contains(&"merge_window"), "{:?}", daemon.log());
    assert!(names.contains(&"signal_watcher"), "{:?}", daemon.log());
    assert!(
        daemon.log_index("daemon.task_stopped").expect("取消")
            < daemon.log_index("daemon.stopped").expect("完成")
    );
    // 取消后不得再出现新的周期清理（进程已退出，日志不会再增长）。
    assert_eq!(daemon.log_events("daemon.maintenance").len(), ticks.len());
}

/// R15/R16（周期本身）：Daemon 运行**超过一个清理周期**后，第二轮清理以 `reason = "periodic"` 出现，
/// 且周期不是自旋（启动后 5s 内不得有第二轮）。本用例是唯一会等满一个 60s 周期的用例，
/// 因此单独成一个测试函数（总时长约 70s）。
#[test]
fn the_periodic_task_runs_again_after_one_full_cycle() {
    let mut daemon = Daemon::configure("maintenance-period", &seed_profile("codex", true));
    daemon.start();
    assert_eq!(
        daemon.log_events("daemon.maintenance").len(),
        1,
        "启动初清理恰好一轮：{}",
        daemon.log()
    );

    // 周期不是自旋：5s 内不得出现第二轮。
    std::thread::sleep(std::time::Duration::from_secs(5));
    assert_eq!(
        daemon.log_events("daemon.maintenance").len(),
        1,
        "清理周期不得短于 5s（实测 {} 轮）",
        daemon.log_events("daemon.maintenance").len()
    );

    // 等待第二个周期（上限 75s）：`MAINTENANCE_INTERVAL = 60s`，留足调度余量。
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(75);
    loop {
        let ticks = daemon.log_events("daemon.maintenance");
        if ticks.len() >= 2 {
            assert_eq!(ticks[1]["reason"], json!("periodic"));
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "60s 周期内必须再执行一次清理（已观测 {} 轮）：{}",
            ticks.len(),
            daemon.log()
        );
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    // R16 的「周期任务先于 Agent 停止被取消」：取消事件的顺序在关闭序列日志里可判定。
    assert!(daemon.stop().success());
    assert!(
        daemon.log_index("daemon.task_stopped").expect("取消")
            < daemon.log_index("daemon.agents_stopped").expect("停 Agent")
    );
    assert!(
        daemon.log_index("daemon.task_stopped").expect("取消")
            < daemon.log_index("daemon.storage_closed").expect("刷盘")
    );
}

/// 交叉检查：`daemon.status` 的 `result` 是开放容器，但 CLI 只依赖这几个字段（§5.2）。
#[test]
fn the_status_result_keeps_the_documented_field_set() {
    let mut daemon = Daemon::configure("status-fields", &seed_profile("codex", true));
    daemon.start();
    let status = Value::Object(daemon.ok(Method::DaemonStatus, params()));
    let mut keys: Vec<&str> = status
        .as_object()
        .expect("对象")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "agents",
            "counts",
            "dataDir",
            "instanceId",
            "links",
            "listen",
            "nodeId",
            "nodePublicKey",
            "publicOrigin",
            "startedAt",
            "uptimeMs",
            "version",
        ]
    );
    let started_at = status["startedAt"].as_str().expect("startedAt");
    assert!(
        started_at.ends_with('Z'),
        "时间戳必须是 §1.1 的文本：{started_at}"
    );
    assert_eq!(started_at.len(), 24, "{started_at}");
    assert!(daemon.stop().success());
}
