//! `daemon-lifecycle` 规格的进程级验收（R1–R16 的可判定部分）。
//!
//! 每个用例都启动**真实二进制**（`CARGO_BIN_EXE_acp-remote`）并使用独立临时 `dataDir`：
//! 单实例锁、endpoint 创建、进程退出码、关闭序列顺序只有跨进程才能验证（`support/mod.rs` 的模块注释）。
//! R 号（R1–R16）见 `openspec/changes/archive/2026-09-25-daemon-cli-and-local-admin/tasks.md` 的 2.14
//! 完成条件（该变更的归档产物里没有单独的 `plan.md`——本仓库的规划 `plan` artifact 是后来才加入
//! schema 的），验收条目见 `specs/daemon-lifecycle/spec.md`。

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
        r#"[agent]
{default_line}{agent_id} = ["{command}"]
"#,
        default_line = if default {
            format!("default = \"{agent_id}\"\n")
        } else {
            String::new()
        },
        command = env!("CARGO_BIN_EXE_acp-remote").replace('\\', "/"),
    )
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

/// R6/R7：Agent 列表每次启动都从配置读取；重启不保留已删除的声明。
#[test]
fn local_configuration_survives_a_restart_and_seeds_are_not_reimported() {
    let command = env!("CARGO_BIN_EXE_acp-remote").replace('\\', "/");
    let agents = format!(
        "[agent]\ndefault = \"codex\"\ncodex = [\"{command}\"]\nhelper = [\"{command}\"]\n"
    );
    let mut daemon = Daemon::configure("restart", &agents);
    daemon.start();
    let before = daemon.ok_value(Method::DaemonStatus, params());
    assert_eq!(
        before["agents"],
        json!([
            { "agentId": "codex", "available": true },
            { "agentId": "helper", "available": true }
        ])
    );
    assert!(daemon.stop().success());

    // 重启时配置是唯一权威：删掉一个声明，列表随之收紧。
    let mut second = daemon.configure_in("second", &seed_profile("codex", true));
    second.start();
    let after = second.ok_value(Method::DaemonStatus, params());
    assert_eq!(
        after["agents"],
        json!([{ "agentId": "codex", "available": true }]),
        "Agent 列表必须来自本次启动的配置"
    );
    assert!(second.stop().success());
}

/// R8/R9：`daemon.status.counts` 与持久记录一致（用 `device.list` 交叉验证）。
#[test]
fn status_counts_follow_persisted_records() {
    let mut daemon = Daemon::configure("counts", &seed_profile("codex", true));
    daemon.start();

    let status = daemon.ok_value(Method::DaemonStatus, params());
    let devices = daemon.ok_value(Method::DeviceList, params());
    assert_eq!(
        status["counts"]["devices"].as_u64().expect("devices"),
        devices["devices"].as_array().expect("devices 数组").len() as u64
    );
    assert_eq!(status["counts"]["nodes"], json!(0));

    assert!(daemon.stop().success());
}

/// 取 `daemon.drain` 事件里的 `drain_ms`（**排空阶段**的耗时）。
///
/// 断言必须落在排空阶段而不是整个关闭序列：后者还含与排空无关的阶段（WAL checkpoint、存储刷新），
/// 在负载高的机器上可达秒级。回退形态下（把关闭请求者计入排空）该值会等于宽限，实测 2999。
fn drain_ms_of(daemon: &Daemon) -> u64 {
    let events = daemon.log_events("daemon.drain");
    assert!(
        !events.is_empty(),
        "必须留下 daemon.drain 日志以判定排空耗时：{}",
        daemon.log()
    );
    events[0]["drain_ms"]
        .as_u64()
        .unwrap_or_else(|| panic!("daemon.drain 必须带 drain_ms：{:?}", events[0]))
}

/// R13 的补充（task 2.26；`daemon-shutdown-drain` 1.2）：**没有其他客户端在途**时，`daemon stop` 不必等满宽限。
///
/// 钉住的是本机实测到的真实缺口：CLI 发出 `daemon.stop` 后就 `drop(client)` 并进入等锁循环，但
/// Windows 上 Named Pipe 句柄由挂起的 overlapped 读持有，句柄只在**驱动它的 Tokio runtime** 被驱动或销毁时
/// 才真正 `CloseHandle`；等锁循环全程 `std::thread::sleep`，从不驱动那个被缓存的 current_thread runtime，
/// 于是句柄一直开到 CLI 进程退出——在那之前 Daemon 的 `drain_connections` 看不到对端 EOF，只能等满
/// `shutdown_grace_ms`（实测 `drain_timeout{remaining:1}` 与 `elapsed_ms ≈ grace_ms`）。
///
/// 用例因此**不依赖「客户端进程何时被调度」**：②里关闭请求者由用例自己持有，收到 `accepted` 之后故意拖延
/// 关闭连接——本用例直到断言结束都不关它（回退形态下它会一直被保持到宽限，Daemon 只能等满宽限再中止它），
/// 而正确形态下关闭序列仍必须在远小于 `graceMs` 内推进到结束、不出现 `daemon.drain_timeout`，并且那条连接
/// 的结束原因是「关闭请求自身的连接」（`reason = "shutdown_requested"`）——它被**识别**出来，而不是按
/// 「谁先关/谁最后关」猜出来的。
///
/// **阈值取宽限的 2/3，且断言落在「排空阶段」而不是整个关闭序列**：排空才是本次的契约，而关闭序列
/// 还包含 `wal_checkpoint(TRUNCATE)` 等与排空无关的阶段 —— 在负载高的机器上那些阶段可达秒级，用
/// 「整个关闭耗时 < 2/3 宽限」当阈值会把不相干的慢也算进来（CI 上实测过一次 `elapsed_ms=2002` vs
/// `threshold_ms=2000` 的误报）。判据取日志里的 `daemon.drain.drain_ms`：回退形态下它会等于宽限
/// （实测 2999），正确形态下只有几十毫秒，两者相差一个数量级。
#[test]
fn stop_does_not_wait_for_the_full_grace_without_other_clients() {
    const GRACE_MS: u64 = 3000;
    let threshold = Duration::from_millis(GRACE_MS * 2 / 3);

    // ① CLI 路径：`daemon stop` 的客户端进程就是关闭请求者，CLI 自己的耗时同样不得接近宽限。
    let mut daemon = Daemon::configure("stop-drain", "");
    daemon.start();
    let lock_path = daemon.data_dir().join(app::lock::LOCK_FILE_NAME);
    let grace_arg = GRACE_MS.to_string();

    let run = run_cli(
        "daemon-stop-drain",
        Some(daemon.config_path()),
        &["daemon", "stop", "--grace-ms", grace_arg.as_str()],
        Stdin::Null,
    );
    run.assert_success();
    assert!(run.stdout.contains("已停止"), "{}", run.stdout);

    // 锁已释放：CLI 返回时关闭序列已经完成（R13）。
    let lock = DaemonLock::acquire(&lock_path).expect("退出后锁必须可再取");
    drop(lock);
    let status = daemon.wait_exit();
    assert!(status.success(), "关闭序列完成后必须正常退出：{status}");

    // 排空窗口**没有**走到超时分支：Daemon 不问「发出关闭请求的那条连接」要 EOF。
    assert!(
        daemon.log_events("daemon.drain_timeout").is_empty(),
        "无其他客户端在途时不得出现排空超时：日志={}",
        daemon.log()
    );
    assert!(
        drain_ms_of(&daemon) < threshold.as_millis() as u64,
        "无其他客户端在途时排空必须明显早于宽限完成：drain_ms={} grace_ms={GRACE_MS} 日志={}",
        drain_ms_of(&daemon),
        daemon.log()
    );

    // ② 确定性探针：关闭请求者收到 `accepted` 之后**故意拖延关闭连接**（本用例直到断言结束都不关它）。
    //    关闭序列若把它当在途连接排空，耗时就会跟着本进程的调度走——那正是 CI 上掷硬币的那条路径。
    let mut delayed = daemon.configure_in("stop-drain-held", "");
    delayed.start();
    let mut requester = delayed.open_connection();
    let started = Instant::now();
    let response = requester.call(
        Method::DaemonStop,
        params_of(json!({ "graceMs": GRACE_MS })),
    );
    match outcome_of(&response) {
        ClientOutcome::Success(result) => {
            assert_eq!(result.get("accepted"), Some(&Value::Bool(true)))
        }
        ClientOutcome::Failure { code, message } => {
            panic!("daemon.stop 必须成功，实际 {code}：{message}")
        }
    }
    // 连接保持打开：关闭序列必须在阈值内走完（进程退出也算「已走完」——日志可能随进程收尾才落盘）。
    let deadline = started + threshold;
    while Instant::now() < deadline
        && delayed.is_running()
        && delayed.log_events("daemon.stopped").is_empty()
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    // 连接始终没有被本用例关闭：排空必须**识别**它是关闭请求者而不等它（同①：断言落在排空阶段，
    // 因为整个关闭序列还包含与排空无关的阶段）。回退形态下排空会等满宽限（drain_ms ≈ grace）。
    assert!(
        drain_ms_of(&delayed) < threshold.as_millis() as u64,
        "关闭请求者拖延关闭连接时，排空必须明显早于宽限完成：drain_ms={} threshold_ms={} 日志={}",
        drain_ms_of(&delayed),
        threshold.as_millis(),
        delayed.log()
    );
    // 连接始终没有被本用例关闭：进程退出与锁释放都不依赖对端结束。
    assert!(
        delayed.wait_exit().success(),
        "关闭序列完成后必须正常退出：日志={}",
        delayed.log()
    );
    assert_eq!(
        delayed.log_events("daemon.stopped").len(),
        1,
        "关闭序列必须已经走完：日志={}",
        delayed.log()
    );
    assert!(
        delayed.log_events("daemon.drain_timeout").is_empty(),
        "关闭请求者不得进入排空等待：日志={}",
        delayed.log()
    );
    assert!(
        delayed
            .log_events("local_admin.connection_closed")
            .iter()
            .any(|event| event["reason"] == "shutdown_requested"),
        "关闭请求自身的连接必须被识别并在写回响应后自行结束（而不是等对端 EOF）：日志={}",
        delayed.log()
    );
    requester.close();
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
        match held.try_call(Method::AgentList, params()) {
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

/// R12/R13：`daemon.stop` 返回 accepted 之后，网络 listener 必须**已停止 accept**。
///
/// 可观测的后果是「新连接不再被服务」：本地排空窗口内进程仍存活（下面用地一条本地长连接把它撞开），
/// 而监听地址在这整个窗口里等不到任何 HTTP 响应。旧实现把触发推到了 `await` 点（本地排空之后），
/// 因此同一条请求在当时会被正常作答。
///
/// 不用「单次采样」做判据：停 accept 确实会让 `axum::serve` 立刻丢弃监听套接字、此后 `connect` 被拒，
/// 但日志（`daemon.ingress_stopped`）与接受循环真正跳出之间隔着一个信号生效窗口——那一瞬的采样无论成败
/// 都不能定论。判据因此落在有界预算内的「有响应 → 无响应」转变上，而不落在某一个瞬间。
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
    // 判据从「日志出现后的一次采样」改成「有界预算内的状态转变」：`trigger_shutdown()` 只发取消信号，
    // 接受循环此刻可能仍在 `accept`，内核 backlog 也会照常完成握手，所以「日志」与「真正拒绝」之间隔着
    // 一个信号生效窗口。观察点因此落在「探针开始收不到响应」这个行为转变上，而不是日志出现的那一瞬。
    //
    // 就绪阶段已用同一地址断言过探针能成功（上面的 `probe_http(addr, 2s)`），因此这里「有响应 → 无响应」
    // 的转变必须真实发生。
    //
    // 预算必须**严格且宽裕地**小于实现自身的强制停止时刻：本用例配置 `shutdown_grace_ms = 5000`，
    // 关闭序列按 `deadline = started + max(grace_ms, MIN_DRAIN_MS)`（`crates/app/src/daemon.rs`）计时，
    // 到点会对仍在 accept 的任务 `handle.abort()`，监听套接字随之关闭。若预算与之重合（5s），
    // 那么「接受循环从不停」这条回归会在预算末端恰好被强杀掩盖成假绿（末次探针落在 abort 之后失败）。
    // 取 2s：正常实现是毫秒级停止，约三个数量级的余量足够吸收调度抖动，又严格早于 5s 的强杀。
    let probing_since = Instant::now();
    let deadline = probing_since + Duration::from_secs(2);
    // 单次 200ms 超时不足以充当「停 accept」的证据（排空/调度抖动都可能制造一次孤立失败）；
    // 要求**连续两次**探针失败才确认转变，成功一次即重置计数。
    let mut refused_after = None;
    let mut consecutive_failures = 0u32;
    while Instant::now() < deadline {
        // 「进程已退出」不能当作「停止接受」的证据：先确认还活着，再看探针结果。
        let alive = daemon.is_running();
        assert!(
            alive,
            "在确认停 accept 之前进程就退出了——「进程已退出」不能充当「停止接受新连接」的证据：{}",
            daemon.log()
        );
        if probe_http(addr, Duration::from_millis(200)) {
            consecutive_failures = 0;
        } else {
            consecutive_failures += 1;
            if consecutive_failures >= 2 {
                refused_after = Some(probing_since.elapsed());
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // 预算内始终有响应 ⇒ 接受循环没停（实现坏了，必须红）；循环自然结束意味着从未确认转变。
    let refused_after = refused_after.unwrap_or_else(|| {
        panic!(
            "2 秒预算内监听地址始终在服务新连接（接受循环没停）：{}",
            daemon.log()
        )
    });
    // 转变确实发生过（就绪探针成功过，`refused_after` 也因此存在），且必须来自停 accept 而非进程退出。
    let alive_after = daemon.is_running();
    assert!(
        alive_after,
        "停 accept 的转变在 {refused_after:?} 内出现，但进程此时必须仍存活：{}",
        daemon.log()
    );

    held.close();
    assert!(daemon.wait_exit().success(), "关闭序列完成后必须正常退出");
}

/// 向监听地址发一条最小 HTTP 请求，返回「是否收到响应」。连接失败、连接超时、写入失败与等不到任何字节
/// 都算「没有响应」；监听套接字由 `axum::serve` 持有，收到关闭信号（= 停 accept）时它立刻丢弃该套接字
/// （宽限只用在等在途连接上），因此从那一刻起 `connect` 会被拒绝；在此之前，backlog 里已完成握手的连接
/// 会一直无人认领，表现为写入成功而读等不到字节。两者都发生在进程仍存活期间，故本判据不区分、也不依赖
/// 它们。
///
/// `connect` 也必须纳入超时：Windows 上被拒绝的 loopback 连接要约 2 秒才返回错误，只给 `read`/`write`
/// 设超时会让一次失败探针的成本（≈2s）远超调用方给出的 `wait`，把「有界预算」变成空话——实测一次失败
/// 探针耗时 `2.0491723s` / `2.0524813s`，且整段里本函数均未收到任何字节（耗时全在 `connect` 返回上，
/// 不是读等待）。
fn probe_http(addr: std::net::SocketAddr, wait: Duration) -> bool {
    use std::io::{Read as _, Write as _};
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, wait) else {
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
    //
    // 等待预算取 5s：它**远小于** 60s 的维护周期，因此「等到下一轮」不可能通过这条断言；而它足以
    // 吸收「异步任务被调度 + 日志落盘」的延迟（零预算会把该延迟误判成「没跑」）。
    let ticks = daemon.wait_for_event("daemon.maintenance", std::time::Duration::from_secs(5));
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
    // 同上一个用例：首轮由异步任务产生且经日志落盘，零预算断言会把调度延迟误判成「没跑」。
    // 预算 5s 远小于 60s 周期，因此「等到下一轮」不可能让下面的计数变成 1。
    let startup = daemon.wait_for_event("daemon.maintenance", std::time::Duration::from_secs(5));
    assert_eq!(startup.len(), 1, "启动初清理恰好一轮：{}", daemon.log());

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

/// 交叉检查：`daemon.status` 的 `result` 是开放容器，但字段集合由 §5.2 冻结。CLI 只消费其中几个字段；
/// `uiBuildVersion`（`sync-ui-hosting` 的 D3）不在 CLI 的消费面上，但同属这份字段集合，因此一并钉住。
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
            "uiBuildVersion",
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

#[test]
fn the_daemon_uses_the_file_clock_only_when_explicitly_injected() {
    let injected = Daemon::configure("test-file-clock", &seed_profile("codex", true));
    let clock_path = injected.root().join("acceptance.clock");
    std::fs::write(&clock_path, "2026-10-26T00:29:59.999Z").expect("seed test clock");
    let mut injected = injected
        .with_env("ACPR_TEST_CLOCK_FILE", clock_path.as_os_str())
        .with_env("ACPR_TEST_RECLAIM_INTERVAL_MS", "5");
    injected.start();
    let injected_status = injected.ok_value(Method::DaemonStatus, params());
    assert_eq!(
        injected_status["startedAt"],
        json!("2026-10-26T00:29:59.999Z"),
        "the real daemon must consume the externally supplied acceptance clock"
    );
    assert!(injected.stop().success());

    let mut default = Daemon::configure("default-system-clock", &seed_profile("codex", true));
    default.start();
    let default_status = default.ok_value(Method::DaemonStatus, params());
    assert_ne!(
        default_status["startedAt"],
        json!("2026-10-26T00:29:59.999Z"),
        "without the explicit dev-only environment seam the daemon must use SystemClock"
    );
    assert!(default.stop().success());
}

/// R13：宽限被前面的步骤用满（在途连接把排空预算耗尽）时，关闭仍必须正常收尾——后台任务的取消
/// 窗口此时已经过期，但不得因此留下 detached 任务或陈旧运行记录。
///
/// 钉住的是 CI 上 `cli_commands::daemon_stop_waits_for_the_lock_and_the_process_exit` 随机失败的根因：
/// `OwnedTasks::cancel_all` 的超时分支只记日志并丢掉 `JoinHandle`（= detach），任务继续持有端口克隆，
/// `Composition::close` 随机报 `StoreStillShared`，而清理（删除运行记录）在 `?` 之后——外部于是看到
/// 「锁可获取、运行记录还在」，Daemon 也以非零码退出。
#[test]
fn stop_cleans_up_even_when_the_drain_budget_is_spent() {
    // `shutdown_grace_ms = 0` 被 `MIN_DRAIN_MS`（200 ms）托底：在途连接会把这段预算用满，
    // 后面的取消期限因此已经过期（超时分支）。
    let mut daemon = Daemon::configure_with("stop-budget-spent", "shutdown_grace_ms = 0\n", "");
    daemon.start();
    let record_path = app::lock::record_path(daemon.data_dir());
    let lock_path = app::lock::lock_path(daemon.data_dir());

    // 在途本地连接：它不会自行结束，本地排空只能等满宽限。
    let mut held = daemon.open_connection();
    assert!(matches!(
        outcome_of(&held.call(Method::DaemonStatus, params())),
        ClientOutcome::Success(_)
    ));

    let run = run_cli(
        "stop-budget-spent-cli",
        Some(daemon.config_path()),
        &["daemon", "stop"],
        Stdin::Null,
    );
    run.assert_success();
    assert!(run.stdout.contains("已停止"), "{}", run.stdout);

    // 外部可见的不变量：记录已删除、锁已释放，且 Daemon 正常退出（存储真的被刷新）。
    assert_eq!(
        app::lock::read_record(&record_path).expect("可读"),
        None,
        "正常关闭必须删除运行记录：日志={}",
        daemon.log()
    );
    assert!(
        matches!(app::lock::probe(&lock_path), app::lock::LockState::Free),
        "CLI 必须在锁释放后才退出"
    );
    let status = daemon.wait_exit();
    assert!(
        status.success(),
        "清理与存储刷新都必须完成（未留下持有存储句柄的任务）：{status} 日志={}",
        daemon.log()
    );
    assert_eq!(
        daemon.log_events("daemon.storage_closed").len(),
        1,
        "存储必须被真正刷新：日志={}",
        daemon.log()
    );
    // 规格「关闭时的连接排空」的**第三个场景**（宽限耗尽仍关闭剩余连接）：`held` 是本用例持有的
    // 非请求者长连接，它直到宽限结束都不结束，因此排空必须留下 `daemon.drain_timeout` 这一可观察记录。
    // 这是**正向**判据——只有排空真的走到超时分支（在途连接直到 deadline 仍未结束）时才会成立：
    // 若 `held` 在预算内结束，或排空不再等待就返回，该记录不会出现。「关闭请求者不计入排空」的方向由
    // `stop_does_not_wait_for_the_full_grace_without_other_clients` 的两条空断言判定（本用例另有 `held`，
    // 故对请求者是否计入并不敏感）。
    let drain_timeout = daemon.log_events("daemon.drain_timeout");
    assert_eq!(
        drain_timeout.len(),
        1,
        "宽限耗尽仍关闭剩余连接必须留下恰好一条排空超时记录：日志={}",
        daemon.log()
    );
    assert!(
        drain_timeout[0]["remaining"]
            .as_u64()
            .is_some_and(|remaining| remaining >= 1),
        "排空超时记录必须报告被中止的剩余连接数（>=1）：日志={}",
        daemon.log()
    );
    held.close();
}

/// 配置没有 `[agent]`、旧库又有 profile 时必须记录 `daemon.agents_discarded` 与条数（v10 迁移已删表）。
#[test]
fn discarded_legacy_agents_are_reported_at_startup() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let daemon = Daemon::configure("agents-discarded", "");
    let storage = storage_sqlite::migrate::StorageConfig::new(daemon.data_dir().to_path_buf());
    let at = acp_core::model::Timestamp::new("2026-10-17T00:00:00.000Z").expect("timestamp");
    let store = runtime
        .block_on(storage_sqlite::session_store::SqliteStore::open(
            storage.clone(),
            &at,
        ))
        .expect("create database");
    runtime.block_on(store.close());
    let pool = runtime
        .block_on(sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(storage.database_path())
                .create_if_missing(false),
        ))
        .expect("raw pool");
    runtime
        .block_on(
            sqlx::raw_sql(
                "CREATE TABLE owned_agent_profile (agent_id TEXT PRIMARY KEY);
         INSERT INTO owned_agent_profile (agent_id) VALUES ('old-a'), ('old-b');
         UPDATE meta SET value = '9' WHERE key = 'owned_schema_version';
         PRAGMA user_version = 9;",
            )
            .execute(&pool),
        )
        .expect("seed v9");
    runtime.block_on(pool.close());

    let mut daemon = daemon;
    daemon.start();
    let events = daemon.log_events("daemon.agents_discarded");
    assert_eq!(events.len(), 1, "{}", daemon.log());
    assert_eq!(events[0]["count"], json!(2), "{}", daemon.log());
    assert!(daemon.stop().success());
}

/// 非法 `[agent]` 条目在启动时失败关闭：stderr 指出键名与规则，不回显命令原文。
#[test]
fn invalid_agent_configuration_fails_closed_without_echoing_the_command() {
    let daemon = Daemon::configure(
        "invalid-agent",
        "[agent]\nbad = \"\"\nleaky = [\"secret-command-value\", \"\"]\n",
    );
    let (status, _stdout, stderr) = run_start_once(daemon.config_path());
    assert!(!status.success(), "非法 [agent] 必须拒绝启动：{stderr}");
    assert_eq!(failure_code(&stderr), "local.invalid_params");
    assert!(stderr.contains("agent."), "错误必须指出键名：{stderr}");
    assert!(
        !stderr.contains("secret-command-value"),
        "错误不得回显命令原文：{stderr}"
    );
    assert!(!daemon.data_dir().exists(), "配置阶段失败不得创建数据目录");
}
