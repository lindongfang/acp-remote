//! `cli-commands` 与 `daemon-lifecycle` 规格的进程级验收（R10/R11/R13、R59–R72）。
//!
//! 每个用例都用**真实二进制**（`CARGO_BIN_EXE_acp-remote`）与独立临时 data dir：退出码、stderr 的双通道
//! 形态、配对轮询、`acp-stdio` 的字节泵只有跨进程才能验证（`support/mod.rs` 的模块注释）。
//! 需要 Daemon 的用例才启动它；其余用例只写一份配置（Daemon 未运行本身就是要断言的场景）。

mod support;

use std::path::Path;

use app::lock::{LockRecord, LockState, lock_path, probe, read_record, record_path};
use serde_json::{Value, json};
use support::{CliRun, Daemon, Stdin, dir_entries, run_cli};

/// 一份与 `agent.configure`/`export.create` 同形的 JSON 文件（写入用例自己的临时目录）。
fn write_file(root: &Path, name: &str, content: &str) -> std::path::PathBuf {
    let path = root.join(name);
    std::fs::write(&path, content).expect("写文件");
    path
}

/// 失败时 stderr 必须是**恰好一行**含 `code` 的 JSON，且 stdout 只有人类可读说明（R62/R63）。
fn assert_failure_contract(run: &CliRun, code: &str, forbidden: &[&str]) {
    run.assert_failure(code);
    let lines: Vec<&str> = run
        .stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert_eq!(lines.len(), 1, "失败时 stdout 只有一行简述：{}", run.stdout);
    assert_no_secrets(run, forbidden);
}

/// 只断言失败契约的「退出码 + 一行 stderr JSON」部分（多步仪式会在 stdout 留下进度输出）。
fn assert_failure_only(run: &CliRun, code: &str, forbidden: &[&str]) {
    run.assert_failure(code);
    assert_no_secrets(run, forbidden);
}

/// 不得回显 secret、完整路径或凭据明文。
fn assert_no_secrets(run: &CliRun, forbidden: &[&str]) {
    for text in forbidden {
        assert!(
            !run.stdout.contains(text) && !run.stderr.contains(text),
            "不得回显 `{text}`：stdout={} stderr={}",
            run.stdout,
            run.stderr
        );
    }
}

/// R10（反例断言）：数据目录里放一个**打不开的数据库文件**时，`daemon status` 仍必须成功回答「未运行」。
///
/// 数据库文件故意做成**目录**（任何 SQLite 打开都会失败），并断言此后数据目录里没有多出任何文件
/// （没有 `-wal`/`-shm`/新文件）——CLI **绝不打开数据库**。
#[test]
fn daemon_status_answers_offline_without_opening_the_database() {
    let daemon = Daemon::configure("cli-status-offline", "");
    let data_dir = daemon.data_dir();
    std::fs::create_dir_all(data_dir.join("acp-remote.sqlite3")).expect("伪造不可打开的数据库文件");
    let before = dir_entries(data_dir);
    assert_eq!(before, vec!["acp-remote.sqlite3".to_owned()]);

    let run = run_cli(
        "status-offline",
        Some(daemon.config_path()),
        &["daemon", "status"],
        Stdin::Null,
    );
    run.assert_success();
    assert!(
        run.stdout.contains("未运行"),
        "必须给出「未运行」的结论：{}",
        run.stdout
    );
    assert_eq!(
        dir_entries(data_dir),
        before,
        "判定未运行不得创建锁文件/运行记录，也不得打开数据库"
    );

    // 陈旧运行记录但锁可获取 = 未运行（不得据此去连接任何 endpoint）。
    std::fs::write(
        record_path(data_dir),
        serde_json::to_vec(&LockRecord {
            instance_id: "0123456789abcdef".to_owned(),
            pid: 1,
            endpoint: Some("\\\\.\\pipe\\does-not-exist".to_owned()),
        })
        .expect("编码记录"),
    )
    .expect("写记录");
    assert!(matches!(probe(&lock_path(data_dir)), LockState::Free));
    let run = run_cli(
        "status-stale",
        Some(daemon.config_path()),
        &["daemon", "status"],
        Stdin::Null,
    );
    run.assert_success();
    assert!(run.stdout.contains("未运行"), "{}", run.stdout);
}

/// R10：Daemon 运行时 `daemon status` 经本地通道问 Daemon，`instanceId` 与运行记录一致。
#[test]
fn daemon_status_reports_the_running_instance() {
    let mut daemon = Daemon::configure("cli-status-online", "");
    daemon.start();
    let run = run_cli(
        "status-online",
        Some(daemon.config_path()),
        &["daemon", "status"],
        Stdin::Null,
    );
    run.assert_success();
    let parsed: Value = serde_json::from_str(run.stdout.trim()).expect("stdout 是 result JSON");
    assert_eq!(
        parsed["instanceId"].as_str(),
        Some(daemon.instance_id()),
        "instanceId 必须与运行记录一致"
    );
    assert_eq!(parsed["links"], json!([]));
    daemon.stop();
}

/// R11：有锁但 IPC 不可达时，`daemon status` 必须明确报错（不当成「未运行」，不直接读库）。
#[test]
fn daemon_status_fails_when_the_lock_is_held_but_the_channel_is_unreachable() {
    let daemon = Daemon::configure("cli-status-held", "");
    let data_dir = daemon.data_dir();
    std::fs::create_dir_all(data_dir).expect("数据目录");
    // 本进程持有锁 + 发布一个指向不存在 endpoint 的记录：与「Daemon 在运行但连不上」等价。
    let mut held = app::DaemonLock::acquire(&lock_path(data_dir)).expect("取锁");
    held.publish(&LockRecord {
        instance_id: "fedcba9876543210".to_owned(),
        pid: 1,
        endpoint: Some(if cfg!(windows) {
            "\\\\.\\pipe\\acpr-wp4b-missing".to_owned()
        } else {
            data_dir.join("missing.sock").display().to_string()
        }),
    })
    .expect("发布记录");

    let run = run_cli(
        "status-held",
        Some(daemon.config_path()),
        &["daemon", "status"],
        Stdin::Null,
    );
    assert_failure_contract(&run, "local.unavailable", &[]);
    assert!(
        !run.stdout.contains("未运行"),
        "有锁但 IPC 不可达不得报成「未运行」：{}",
        run.stdout
    );

    // 记录缺失（仍持锁）同样是 `local.unavailable`。
    std::fs::remove_file(record_path(data_dir)).expect("删记录");
    let run = run_cli(
        "status-held-norecord",
        Some(daemon.config_path()),
        &["daemon", "status"],
        Stdin::Null,
    );
    assert_failure_contract(&run, "local.unavailable", &[]);
    drop(held);
}

/// R13：`daemon stop` 经 CLI 调用后等待锁释放；Daemon 进程退出、记录被删除。
#[test]
fn daemon_stop_waits_for_the_lock_and_the_process_exit() {
    let mut daemon = Daemon::configure("cli-stop", "");
    daemon.start();
    let lock = lock_path(daemon.data_dir());
    assert!(matches!(probe(&lock), LockState::Held));

    let run = run_cli(
        "stop",
        Some(daemon.config_path()),
        &["daemon", "stop"],
        Stdin::Null,
    );
    run.assert_success();
    assert!(run.stdout.contains("已停止"), "{}", run.stdout);
    assert!(
        matches!(probe(&lock), LockState::Free),
        "CLI 必须在锁释放后才退出"
    );
    assert_eq!(
        read_record(&record_path(daemon.data_dir())).expect("可读"),
        None,
        "正常关闭必须删除运行记录"
    );
    // Daemon 前台进程已退出（CLI 返回时关闭序列已完成）。
    let status = daemon.wait_exit();
    assert!(status.success(), "Daemon 必须 0 退出：{status}");

    // 停止后 cStatus 回答「未运行」（同一条 CLI 路径）。
    let run = run_cli(
        "stop-again",
        Some(daemon.config_path()),
        &["daemon", "stop"],
        Stdin::Null,
    );
    run.assert_success();
    assert!(run.stdout.contains("未运行"), "{}", run.stdout);
}

/// R61：Daemon 未运行时，除 `daemon start|stop|status`/`doctor` 外的管理子命令明确失败（不打开数据库）。
#[test]
fn management_commands_fail_clearly_when_the_daemon_is_not_running() {
    let daemon = Daemon::configure("cli-offline", "");
    let root = daemon.root();
    let empty = write_file(root, "empty.json", "{}");
    let empty = empty.display().to_string();
    let profile = write_file(
        root,
        "profile.json",
        r#"{"agentId":"a","displayName":"A","command":"c","args":[],"envAllowlist":[],"default":false}"#,
    );
    let profile = profile.display().to_string();

    let cases: Vec<Vec<String>> = vec![
        vec![
            "workspace".into(),
            "select".into(),
            "--alias".into(),
            "ws".into(),
            "--display-name".into(),
            "W".into(),
            "--root-path".into(),
            root.display().to_string(),
        ],
        vec!["agent".into(), "configure".into(), "--file".into(), profile],
        vec![
            "agent".into(),
            "configure".into(),
            "--file".into(),
            empty.clone(),
        ],
        vec!["device".into(), "list".into()],
        vec![
            "device".into(),
            "revoke".into(),
            "--device-id".into(),
            "2ae1c07c-0000-4000-8000-000000000001".into(),
        ],
        vec!["node".into(), "list".into()],
        vec![
            "node".into(),
            "revoke".into(),
            "--node-id".into(),
            "2ae1c07c-0000-4000-8000-000000000001".into(),
        ],
        vec![
            "export".into(),
            "create".into(),
            "--file".into(),
            empty.clone(),
        ],
        vec!["export".into(), "list".into()],
        vec![
            "export".into(),
            "revoke".into(),
            "--export-id".into(),
            "e".into(),
        ],
        vec![
            "import".into(),
            "add".into(),
            "--import-id".into(),
            "i".into(),
            "--owner-endpoint".into(),
            "wss://o.example.invalid".into(),
            "--owner-node-id".into(),
            "2ae1c07c-0000-4000-8000-000000000001".into(),
        ],
        vec!["import".into(), "list".into()],
        vec![
            "import".into(),
            "remove".into(),
            "--import-id".into(),
            "i".into(),
        ],
    ];
    for args in cases {
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let run = run_cli(
            "offline",
            Some(daemon.config_path()),
            &borrowed,
            Stdin::Null,
        );
        assert_failure_contract(&run, "local.unavailable", &[]);
    }

    // `provider configure` 在非交互环境**先**失败（不读凭据、不调用方法，R69）。
    let run = run_cli(
        "provider-offline",
        Some(daemon.config_path()),
        &[
            "provider",
            "configure",
            "--provider-id",
            "openai",
            "--kind",
            "provider",
            "--display-name",
            "OpenAI",
            "--field",
            "api_key",
        ],
        Stdin::Bytes(b"sk-should-not-be-read\n".to_vec()),
    );
    assert_failure_contract(&run, "local.invalid_request", &["sk-should-not-be-read"]);
    assert!(
        !run.stdout.contains("（不回显）: ") && !run.stderr.contains("（不回显）: "),
        "非交互环境不得进入凭据提示：stdout={} stderr={}",
        run.stdout,
        run.stderr
    );

    // 用法错误也是非零 + 一行 JSON（且不得出现 self-made 子命令）。
    for args in [
        vec!["daemon", "doctor"],
        vec!["session", "create"],
        vec!["node", "rotate-key"],
        vec!["audit", "export"],
    ] {
        let run = run_cli("usage", None, &args, Stdin::Null);
        assert_failure_contract(&run, "local.invalid_request", &[]);
    }
}

/// R60/R62/R63：运行中的 Daemon 上，管理子命令经本地通道调用对应方法并展示结果；方法失败走双通道契约。
#[test]
fn management_commands_use_the_local_channel_and_report_method_failures() {
    let mut daemon = Daemon::configure("cli-online", "");
    daemon.start();

    // workspace select：CLI 只发参数，Daemon 侧 canonicalize 并持久化；展示的是方法返回值。
    let run = run_cli(
        "workspace",
        Some(daemon.config_path()),
        &[
            "workspace",
            "select",
            "--alias",
            "ws",
            "--display-name",
            "工作区",
            "--root-path",
            &daemon.root().display().to_string(),
        ],
        Stdin::Null,
    );
    run.assert_success();
    let parsed: Value = serde_json::from_str(run.stdout.trim()).expect("result JSON");
    assert_eq!(parsed["workspace"]["alias"], json!("ws"));
    assert_eq!(parsed["workspace"]["displayName"], json!("工作区"));

    // 方法级失败：`local.not_found` 必须按双通道形态输出，且不回显数据目录路径。
    let data_dir = daemon.data_dir().display().to_string();
    let run = run_cli(
        "not-found",
        Some(daemon.config_path()),
        &[
            "device",
            "revoke",
            "--device-id",
            "2ae1c07c-0000-4000-8000-000000000001",
        ],
        Stdin::Null,
    );
    assert_failure_contract(&run, "local.not_found", &[&data_dir]);
    assert!(
        run.stderr.contains("\"code\""),
        "stderr 必须含 code：{}",
        run.stderr
    );

    // export.revoke 的 `local.not_found` 走同一条出口。
    let run = run_cli(
        "export-not-found",
        Some(daemon.config_path()),
        &["export", "revoke", "--export-id", "missing-export"],
        Stdin::Null,
    );
    assert_failure_contract(&run, "local.not_found", &[&data_dir]);

    daemon.stop();
}

/// R67：`--file` 的两种错误码映射（`local.invalid_params` / `local.conflict`），`agent configure` 的文件
/// 不得含凭据值。
#[test]
fn file_inputs_map_validation_errors_like_the_method_side() {
    let mut daemon = Daemon::configure("cli-files", "");
    daemon.start();
    let config = daemon.config_path().to_path_buf();
    let root = daemon.root().to_path_buf();

    // agent.configure：合法文件 → 成功；未知字段（含凭据字段 `values`）→ local.invalid_params。
    let ok = write_file(
        &root,
        "agent-ok.json",
        r#"{"agentId":"codex-local","displayName":"Codex","command":"acp-remote","args":[],"envAllowlist":[],"default":false}"#,
    );
    let run = run_cli(
        "agent-ok",
        Some(&config),
        &["agent", "configure", "--file", &ok.display().to_string()],
        Stdin::Null,
    );
    run.assert_success();
    assert!(run.stdout.contains("codex-local"), "{}", run.stdout);

    for (name, body) in [
        (
            "agent-credentials.json",
            r#"{"agentId":"a","displayName":"A","command":"c","args":[],"envAllowlist":[],"default":false,"values":{"api_key":"sk-secret"}}"#,
        ),
        (
            "agent-unknown.json",
            r#"{"agentId":"a","displayName":"A","command":"c","args":[],"envAllowlist":[],"default":false,"extra":1}"#,
        ),
    ] {
        let path = write_file(&root, name, body);
        let run = run_cli(
            name,
            Some(&config),
            &["agent", "configure", "--file", &path.display().to_string()],
            Stdin::Null,
        );
        assert_failure_contract(&run, "local.invalid_params", &["sk-secret"]);
    }
    let broken = write_file(&root, "agent-broken.json", "{ not json");
    let run = run_cli(
        "agent-broken",
        Some(&config),
        &[
            "agent",
            "configure",
            "--file",
            &broken.display().to_string(),
        ],
        Stdin::Null,
    );
    assert_failure_contract(&run, "local.invalid_params", &[]);

    // export.create：先把别名 `ws` 建好（方法侧要求 alias 已在 owned_workspace 里），再验证三种映射。
    let run = run_cli(
        "export-workspace",
        Some(&config),
        &[
            "workspace",
            "select",
            "--alias",
            "ws",
            "--display-name",
            "工作区",
            "--root-path",
            &root.display().to_string(),
        ],
        Stdin::Null,
    );
    run.assert_success();
    let export = write_file(
        &root,
        "export.json",
        r#"{"exportId":"exp-1","displayName":"Export One","agentIds":["codex-local"],
"workspaceAliases":[{"alias":"ws","displayName":"工作区"}],"defaultWorkspaceAlias":"ws",
"templates":[{"templateId":"tpl-1","displayName":"Tpl","workspaceAlias":"ws","params":[]}],
"defaultTemplateId":"tpl-1","scopes":["grant.session.read"],"cachePolicy":"no-content-cache"}"#,
    );
    let run = run_cli(
        "export-ok",
        Some(&config),
        &["export", "create", "--file", &export.display().to_string()],
        Stdin::Null,
    );
    run.assert_success();
    assert!(run.stdout.contains("exp-1"), "{}", run.stdout);

    let run = run_cli(
        "export-conflict",
        Some(&config),
        &["export", "create", "--file", &export.display().to_string()],
        Stdin::Null,
    );
    assert_failure_contract(&run, "local.conflict", &[]);

    let incomplete = write_file(&root, "export-incomplete.json", r#"{"exportId":"exp-2"}"#);
    let run = run_cli(
        "export-invalid",
        Some(&config),
        &[
            "export",
            "create",
            "--file",
            &incomplete.display().to_string(),
        ],
        Stdin::Null,
    );
    assert_failure_contract(&run, "local.invalid_params", &[]);

    // 列表命令走本地通道并展示 result（Import 为空即为空数组）。
    let run = run_cli(
        "export-list",
        Some(&config),
        &["export", "list"],
        Stdin::Null,
    );
    run.assert_success();
    assert!(run.stdout.contains("exp-1"), "{}", run.stdout);
    let run = run_cli(
        "import-list",
        Some(&config),
        &["import", "list"],
        Stdin::Null,
    );
    run.assert_success();
    assert!(run.stdout.contains("imports"), "{}", run.stdout);

    daemon.stop();
}

/// R70/R71：`doctor` 离线时在 CLI 进程内完成（不连接、不建锁、不打开数据库），在线时组合 `daemon.status`。
#[test]
fn doctor_completes_offline_and_combines_status_when_running() {
    let daemon = Daemon::configure("cli-doctor", "");
    // 数据目录连都不存在：离线结论仍然要给出，且不得创建任何东西。
    std::fs::create_dir_all(daemon.data_dir()).expect("数据目录");
    let before = dir_entries(daemon.data_dir());
    let run = run_cli(
        "doctor-offline",
        Some(daemon.config_path()),
        &["doctor"],
        Stdin::Null,
    );
    run.assert_success();
    assert!(run.stdout.contains("未运行"), "{}", run.stdout);
    assert_eq!(
        dir_entries(daemon.data_dir()),
        before,
        "doctor 不得创建文件"
    );

    let mut daemon = Daemon::configure("cli-doctor-online", "");
    daemon.start();
    let run = run_cli(
        "doctor-online",
        Some(daemon.config_path()),
        &["doctor"],
        Stdin::Null,
    );
    run.assert_success();
    assert!(run.stdout.contains("运行中"), "{}", run.stdout);
    assert!(
        run.stdout.contains(daemon.instance_id()),
        "doctor 必须组合 daemon.status：{}",
        run.stdout
    );
    daemon.stop();
}

/// R72：`acp-stdio` 在 Daemon 未运行时以明确错误退出，且 stdout 不被污染（含 NUL 的二进制输入不回写）。
#[test]
fn acp_stdio_fails_clearly_offline_without_polluting_stdout() {
    let daemon = Daemon::configure("cli-stdio-offline", "");
    std::fs::create_dir_all(daemon.data_dir()).expect("数据目录");
    let run = run_cli(
        "pairing-offline-stdio",
        Some(daemon.config_path()),
        &["acp-stdio"],
        Stdin::Bytes(vec![0x00, 0x41, 0xff, b'\n']),
    );
    assert_failure_only(&run, "local.unavailable", &[]);
    assert!(
        run.stdout.is_empty(),
        "acp-stdio 的 stdout 只能承载 ACP 字节流：{:?}",
        run.stdout
    );
}

/// R72：Daemon 运行中但 facade 缺席（本切片）→ 连接被立即关闭 → 明确错误、stdout 保持为空。
#[test]
fn acp_stdio_reports_the_missing_facade_and_keeps_stdout_clean() {
    let mut daemon = Daemon::configure("cli-stdio-online", "");
    daemon.start();
    let run = run_cli(
        "stdio-online",
        Some(daemon.config_path()),
        &["acp-stdio"],
        Stdin::Bytes(vec![0x00, 0x7f, 0x80]),
    );
    // `acp-stdio` 的 stdout 是 ACP 通道：失败也必须走 stderr（因此这里只断言退出码 + stderr JSON）。
    assert_failure_only(&run, "local.unavailable", &[]);
    assert!(run.stdout.is_empty(), "{:?}", run.stdout);
    // 服务端确实收到了 0x02 连接（framing 校验后立即关闭并记结构化警告）。
    assert!(
        daemon.log().contains("local_admin.acp_facade_unavailable"),
        "Daemon 必须记录 facade 缺席的 0x02 关闭：{}",
        daemon.log()
    );
    daemon.stop();
}

/// R64/R66：配对仪式的非交互门禁（缺 `--sas`/`--fingerprint` 即失败，不调用任何方法）与到期处理。
#[test]
fn pairing_requires_exact_non_interactive_values_and_honours_expiry() {
    let mut daemon = Daemon::configure("cli-pairing", "");
    daemon.start();

    // 非交互且缺两个值：必须在 begin 之前失败（不创建任何配对状态）。
    let run = run_cli(
        "pair-missing",
        Some(daemon.config_path()),
        &["device", "pair", "--request", "session.read"],
        Stdin::Null,
    );
    assert_failure_contract(&run, "local.invalid_request", &[]);

    // 只给一个也不行。
    let run = run_cli(
        "pair-half",
        Some(daemon.config_path()),
        &[
            "device",
            "pair",
            "--request",
            "session.read",
            "--sas",
            "000000",
        ],
        Stdin::Null,
    );
    assert_failure_contract(&run, "local.invalid_request", &[]);

    // 两个都给（本机非交互路径）但没有人认领：收窄有效期后以 `local.expired` 结束（轮询与到期由 CLI 负责）。
    let fingerprint = "0".repeat(64);
    let run = run_cli(
        "pair-expired",
        Some(daemon.config_path()),
        &[
            "device",
            "pair",
            "--request",
            "session.read",
            "--sas",
            "000000",
            "--fingerprint",
            &fingerprint,
            "--expires-in-ms",
            "1000",
        ],
        Stdin::Null,
    );
    assert_failure_only(&run, "local.expired", &[]);
    assert!(
        run.stdout.contains("https://"),
        "begin 必须打印 pairingUrl：{}",
        run.stdout
    );

    // `node pair --mode access` 在本切片由方法侧回 `local.unsupported`（CLI 原样带出，不自行编造错误码）。
    let run = run_cli(
        "node-access",
        Some(daemon.config_path()),
        &[
            "node",
            "pair",
            "--mode",
            "access",
            "--pairing-url",
            "https://owner.example.invalid/#s=x",
            "--display-name",
            "Node",
            "--sas",
            "000000",
            "--fingerprint",
            &fingerprint,
        ],
        Stdin::Null,
    );
    assert_failure_contract(&run, "local.unsupported", &[]);

    daemon.stop();
}
