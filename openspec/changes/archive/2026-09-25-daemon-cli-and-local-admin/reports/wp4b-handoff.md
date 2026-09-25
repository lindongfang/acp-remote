# WP4b 交接：完整 CLI、`doctor` 与 `acp-stdio`（任务 2.15–2.17）

```yaml
task_id: WP4b（tasks.md 2.15 / 2.16 / 2.17）
role: 实现 Agent（coder）
phase: apply（WP4 第二段；W3 完成点的 CLI 面）
agent_context: >
  起点 f9d276d（WP4a 之后的文档提交），分支 feat/daemon-cli-and-local-admin。
  写范围只有 crates/app/**、[workspace.dependencies] 的两行依赖登记、Cargo.lock 的解析结果、
  crates/app/tests/** 与 reports/*；未改 docs/**、schemas/**、fixtures/**、compatibility/**、
  crates/{core,server,storage-sqlite,identity-*,agent-host} 与 planning 文件（plan/tasks/verification/proposal/design/specs）。
target_revision:
  branch: feat/daemon-cli-and-local-admin
  base: f9d276d
  implementation_commit: c8e44eb   # feat(app): 补齐 CLI 全部管理子命令、doctor 与 acp-stdio（任务 2.15–2.17）
  head_at_handoff: c8e44eb（本报告为紧随其后的 docs(app) 提交）
scope:
  - crates/app/src/cli.rs（子命令树、统一错误出口、daemon start|stop|status、doctor、参数→params 翻译、单测）
  - crates/app/src/cli/input.rs（--file 读取与凭据无回显录入接缝）
  - crates/app/src/cli/pairing.rs（配对仪式）
  - crates/app/src/stdio.rs（channel 0x02 字节泵）
  - crates/app/src/lock.rs（新增 LockState/probe：CLI 的「是否在运行」判定）
  - crates/app/src/lib.rs（模块登记）、crates/app/Cargo.toml、Cargo.toml、Cargo.lock
  - crates/app/tests/support/mod.rs（CLI 子进程工装）、crates/app/tests/cli_commands.rs（11 个进程级用例）
```

## 1. 子命令 ↔ `LOCAL_ADMIN_PROTOCOL.md` §5.8 映射（逐条）

| CLI 子命令 | 实现的底层 | 代码位置 | 参数来源（kebab → camelCase） |
|---|---|---|---|
| `daemon start` | CLI 进程自身（不经通道） | `cli.rs::daemon_start` | `--config`（全局） |
| `daemon stop` | `daemon.stop` | `cli.rs::daemon_stop` | `--grace-ms` → `graceMs`（缺省 `null`） |
| `daemon status` | `daemon.status`（未运行则在 CLI 内回答） | `cli.rs::daemon_status` | — |
| `doctor` | `daemon.status` + CLI 侧检查（**不是方法**） | `cli.rs::doctor` | — |
| `workspace select` | `workspace.select` | `cli.rs::workspace_select_params` | `--alias`/`--display-name`/`--root-path` → `alias`/`displayName`/`rootPath` |
| `agent configure` | `agent.configure` | `cli.rs::agent_configure_params` | `--file`（与 §5.2 `params` 同形的 JSON） |
| `provider configure` | `provider.configure` | `cli.rs::provider_configure` + `provider_configure_params` | `--provider-id`/`--kind`/`--display-name`/`--field`（可重复）→ `providerId`/`kind`/`displayName`/`values` |
| `device pair` | `device.pair.begin` → `.status`（轮询）→ `.confirm` | `cli/pairing.rs::run`/`await_claim` | `--request`/`--pack`/`--sas`/`--fingerprint`/`--expires-in-ms` → `requestedScopes`/`requestedPacks`/… |
| `device list` | `device.list` | `cli.rs::dispatch` | — |
| `device revoke` | `device.revoke` | `cli.rs::device_revoke_params` | `--device-id` → `deviceId` |
| `node pair` | `node.pair.begin` → `.status`（轮询）→ `.confirm` | `cli/pairing.rs` | `--mode`/`--pairing-url`/`--display-name`/`--grant`/`--sas`/`--fingerprint`/`--expires-in-ms` → `mode`/`pairingUrl`/`displayName`/`requestedGrants` |
| `node list` / `node revoke` | `node.list` / `node.revoke` | `cli.rs::dispatch` / `node_revoke_params` | `--node-id` → `nodeId` |
| `export create` | `export.create` | `cli.rs::export_create_params` | `--file` |
| `export list` / `export revoke` | `export.list` / `export.revoke` | `cli.rs::dispatch` / `export_revoke_params` | `--export-id` → `exportId` |
| `import add` | `import.add` | `cli.rs::import_add_params` | `--import-id`/`--owner-endpoint`/`--owner-node-id`/`--export-id`（逗号分隔、可重复）/`--grant`（同） |
| `import list` / `import remove` | `import.list` / `import.remove` | `cli.rs::dispatch` / `import_remove_params` | `--import-id` → `importId` |
| `acp-stdio` | channel `0x02` 字节泵（不进管理信封） | `stdio.rs::pump`/`pump_stream` | — |

**未新增任何方法**：单测 `cli::tests::the_subcommand_set_matches_the_documented_mapping` 逐条比对 clap 树与 §5.8，
并断言 `daemon doctor`、`session create`、`node rotate-key`、`audit export` 都不存在（`audit.export` 在 §5.8 里没有
CLI 行，因此本切片不给 CLI 入口）。`daemon stop` 额外暴露 `--grace-ms`（§5.2 `graceMs` 的直接映射）；
`device pair --pack`、`--expires-in-ms` 分别是 §5.3 `requestedPacks` 与可收窄的 `expiresInMs` 的直接映射
（没有它们就无法从 CLI 表达这两个已定义参数）——均为**参数**而非新方法。

## 2. 退出码与 stderr JSON 的实现位置

- 唯一出口：`crates/app/src/cli.rs::fail`（成功 `0`；失败 `ExitCode::FAILURE` = `1`）：
  stdout 一行 `acp-remote: <人类可读说明>`；stderr 恰好一行 `failure_line()` 产出的
  `{"code":…,"message":…}`。
- `code` 来源：CLI 侧失败一律经 `Failure::local(LocalErrorCode, …)`（枚举即
  `server::local_admin::LocalErrorCode`，§6 词表的唯一 Rust 镜像）；方法失败经
  `Failure::reported(code, message)` **原样带出** Daemon 的码（不解析 `error.message`）。
- 用法错误（clap）：`cli.rs::parse_failure`，`--help`/`--version` 走 stdout 且成功；其余
  stdout 一行摘要 + stderr 一行 `local.invalid_request`，退出码 `2`（与运行期失败的 `1` 区分，仍然非零）。
- `acp-stdio` 特例：`cli.rs::fail_stream` —— **stdout 只承载 ACP 字节流**，失败时的人类可读说明也不写 stdout
  （仍然写一行 stderr JSON）。这是本 WP 唯一偏离「stdout 一行简述」的地方，理由是 stdout 是 ACP 通道。
- 传输级失败 → `cli.rs::client_failure` 的固定英文短句（不把底层 io 错误的路径带进消息），
  `local.unavailable`（连接失败/连接中断/响应前关闭）或 `local.internal`（响应不是合法 v1 信封）。

## 3. Daemon 未运行时的回答路径（R10/R11，§7）

- `app::lock::probe`（新增）：只看**能否加锁**——`OpenOptions` **不 create**，`NotFound` → `Free`；
  `try_lock` 成功即立刻 `unlock` → `Free`（同时证明没有持有者）；`WouldBlock` → `Held`；
  其他 io 失败 → `Unusable`。因此 `daemon status` 在干净机器上**不创建任何文件**。
- `Context::running_endpoint`：`Free` → `None`（未运行）；`Held` → 读 `daemon.instance.json`
  （缺失/损坏/`endpoint` 为空 → `local.unavailable`，即 R11「有锁但 IPC 不可达」不得报成「未运行」）；
  `Unusable` → `local.unavailable`。
- `daemon status`/`daemon stop` 在 `None` 时打印「未运行」并以 **0** 退出（§7 把它们与「其余方法以非零
  退出码报错」明确区分：这两条要在 CLI 进程内**回答**）；其余管理子命令经 `Context::endpoint()` 直接
  `local.unavailable` 失败。`daemon stop` 在运行中时：调用 `daemon.stop` → 校验 `accepted: true` →
  由本进程关闭连接（不让 Daemon 的排空窗口等 CLI 的读端）→ 轮询 `probe` 直到 `Free`（上限 30 s）才退出。
- 反例断言（任务点名）：`cli_commands::daemon_status_answers_offline_without_opening_the_database` 在数据目录里
  放一个**故意做成目录**的 `acp-remote.sqlite3`（任何 SQLite 打开都会失败），`daemon status` 仍成功回答
  「未运行」，并断言数据目录内容**逐条不变**（没有新建锁文件/运行记录/`-wal`/`-shm`）；同一用例再放一份
  陈旧运行记录（endpoint 指向不存在的 pipe）→ 仍然回答「未运行」。

## 4. 配对仪式（R64–R66）

`cli/pairing.rs`：`begin` → 打印 `pairingUrl`（内存 → 终端，不落日志/文件/错误）→ 每 500 ms 轮询 `status`
（有效期以返回的 `expiresAt` 为准，§1.1 定宽文本按字典序比较，本地再加 330 s 硬上限）→ 认领后**先**打印
名称/指纹/SAS/请求集合/过期时间 → `confirm` 提交**展示过的**集合（不是 `--request`/`--grant` 原文）。

- 状态词表按 §5.3/§5.4 分流（设备 `claimed` 可确认；节点要等到 `pending_confirmation`；未知词 → `local.internal`）；
  `expired`/`rejected` → `local.expired`；`approved` → `local.conflict`。
- 非交互（`stdin` 不是终端）必须同时给出 `--sas`/`--fingerprint`，否则在 **begin 之前**以
  `local.invalid_request` 失败（不修改任何状态；没有「无参数自动确认」开关）；给了两个之后经
  `verify_supplied` **逐字**比对（不 trim、不折叠大小写、不允许前缀），不一致即以 `local.invalid_params` 失败、
  **不调 confirm**、错误消息也不回显这两个值。
- 交互式只接受 `y`/`Y`；其余输入（含 EOF）不调用任何方法、不修改状态（本切片也**不**代用户调 `*.pair.reject`）。
- `node pair --mode access` 不在 CLI 侧特判：方法的 `local.unsupported` 由统一出口原样带出（进程级用例断言）。

## 5. 凭据与 `--file`（R67–R69）

- `provider configure`：非秘密字段走 flags；`values` 的每一项由 `TerminalPrompt`（`std::io::IsTerminal`
  先判 stdin 是否终端，然后 `eprint!` 提示 + `rpassword` 的**默认配置**＝从终端设备读且关回显）逐项读取。
  **不接受**凭据作为命令行参数或文件输入（这两个 flag 不存在，clap 直接拒绝 —— 单测断言）。
  无交互终端时在读取凭据与连接之前失败（`local.invalid_request`），进程级用例断言 stdout/stderr 都没有 prompt。
- 无回显路径的可单测接缝：`input::read_secret_line(rpassword::Config)` + `SecretPrompt` trait；
  单测用 `ConfigBuilder::input_data(...).output_writer(共享汇)` 断言读取返回值正确且**值没有被写回输出**
  （R68 的机制级证据；真实终端的端到端录入无法自动化，见 §9）。
- `agent configure --file`：文件即 §5.2 `params`；顶层字段名必须在 §5.2 的六个之内，否则
  `local.invalid_params` —— 这正是「文件不得含凭据值」的实现（凭据只能以字段值出现，而六个字段名里没有承载
  凭据的字段），且**不发送**；JSON 非法/顶层不是对象/文件不可读同样是 `local.invalid_params`（与方法侧同码）。
  错误消息只带文件名，不带完整路径。
- `export create --file`：文件即 `params`（嵌套结构交方法侧）；结构非法 → `local.invalid_params`，
  `exportId` 已存在 → `local.conflict`（两个码都由方法侧产生，CLI 不另造）。

## 6. `doctor`（R70/R71）

同一二进制、同一进程：先做 CLI 侧检查（配置可加载；`data_dir` 存在/未初始化/不是目录），再按
`running_endpoint()` 决定是否连接。离线（`Free`）时**不尝试连接**、不建锁、不打开数据库，打印
`daemon 未运行（没有有效单实例锁）` 并以 0 退出；运行中则调 `daemon.status` 并展示
`version`/`instanceId`/`nodeId`/`uptimeMs`/`counts`。有锁但通道不可达 → 非零 `local.unavailable`。
**未新增任何管理方法**。进程级用例断言离线路径不新建任何文件、在线路径包含运行记录的 `instanceId`。

## 7. `acp-stdio`（R72）

`app::stdio`：连接运行记录里的 endpoint（**不**打开数据库、不装配任何核心组件——本模块不依赖
`core`/`storage-sqlite`/`agent-host`），然后 `tokio::select!` 双向泵：

- stdin → Daemon：每块 ≤ 8 KiB（远小于 1 MiB 帧上限）按 `u32be length | 0x02 | payload` 组帧；字节原样
  透传（含 NUL、非 UTF-8），不做行缓冲、不假设文本；stdin EOF 时半关闭写端并继续转发读方向。
- Daemon → stdout：帧 payload 原样写出，channel 不是 `0x02` 即失败（帧非法）。
- 明确失败：`Connect`（Daemon 未运行/正在关闭，且 CLI 在连接前已用锁判定过）→ `local.unavailable`；
  **收到任何 ACP 字节前连接被关闭**（facade 缺席期的既知行为）→ 「daemon 未提供 ACP 流」→ 非零
  `local.unavailable`；stdin 在任何字节传出前结束 → 同样明确失败（都不静默按成功退出）。
- stdout 只承载 ACP 字节：连失败简述都不写 stdout（§2 的 `fail_stream`）。

## 8. 测试清单（进程级用真实二进制 + 每用例独立临时 data dir）

`cargo test --locked -p app --all-features` → **EXIT 0**：43 单元 + 1（`audit_export`）+ 11（`cli_commands`）
+ 9（`daemon_lifecycle`）= 64 passed / 0 failed / 0 ignored。日志：`reports/wp4b-cli.log`（逐项显式 `EXIT(...)`）。

| 用例（文件） | 覆盖的 R | 要点 |
|---|---|---|
| `cli_commands::daemon_status_answers_offline_without_opening_the_database` | **R10** | 数据目录里放**打不开的数据库文件**（目录形式）仍成功回答「未运行」；目录内容逐条不变；陈旧记录 + 可加锁的锁 → 仍「未运行」 |
| `cli_commands::daemon_status_reports_the_running_instance` | **R10**, R9 | 运行中经本地通道调用 `daemon.status`，`instanceId` 与运行记录一致、`links` 为 `[]` |
| `cli_commands::daemon_status_fails_when_the_lock_is_held_but_the_channel_is_unreachable` | **R11** | 持锁 + 指向不存在 endpoint 的记录 → 非零 `local.unavailable`（**不是**「未运行」）；记录缺失（仍持锁）同样 `local.unavailable` |
| `cli_commands::daemon_stop_waits_for_the_lock_and_the_process_exit` | **R13**, R14 | `daemon stop` 0 退出、锁已释放、记录已删除、Daemon 进程 0 退出；再次 `daemon stop` 回答「未运行」 |
| `cli_commands::management_commands_fail_clearly_when_the_daemon_is_not_running` | **R59, R61, R69** | 13 个管理子命令逐条非零 + 一行 `local.unavailable`；`provider configure` 在非终端先失败（`local.invalid_request`，无 prompt、不回显 stdin 里的值）；4 个自造入口是用法错误（非零 + 一行 JSON） |
| `cli_commands::management_commands_use_the_local_channel_and_report_method_failures` | **R60, R62, R63** | `workspace select` 成功并展示方法返回值；`device revoke`/`export revoke` 的 `local.not_found` 走「stdout 一行 + stderr 恰好一行含 `code` 的 JSON」，且不回显数据目录路径 |
| `cli_commands::file_inputs_map_validation_errors_like_the_method_side` | **R67** | `agent configure` 合法文件成功；含 `values`（凭据）字段 → `local.invalid_params` 且不回显凭据值（`sk-secret`）；未知字段、非法 JSON 同码；`export create` 合法 → 成功、重复 → `local.conflict`、缺字段 → `local.invalid_params`；`export list`/`import list` 走本地通道 |
| `cli_commands::doctor_completes_offline_and_combines_status_when_running` | **R70, R71** | 离线：0 退出 + 「未运行」+ 数据目录内容不变（不建锁、不打开数据库）；在线：0 退出 + 「运行中」+ `daemon.status` 的 `instanceId` |
| `cli_commands::acp_stdio_fails_clearly_offline_without_polluting_stdout` | **R72** | Daemon 未运行 + 含 NUL 的二进制 stdin：非零 + 一行 `local.unavailable`，**stdout 为空** |
| `cli_commands::acp_stdio_reports_the_missing_facade_and_keeps_stdout_clean` | **R72** | 运行中但 facade 缺席（即连即关）：非零 + 一行 `local.unavailable`，stdout 为空，Daemon 日志有 `local_admin.acp_facade_unavailable`（证明 0x02 首帧确实到达服务端） |
| `cli_commands::pairing_requires_exact_non_interactive_values_and_honours_expiry` | **R64, R66** | 非交互缺 `--sas`/`--fingerprint`（或只给一个）在 begin 前失败；两个都给 + 1 s 有效期 → 轮询到 `local.expired`（stdout 有 `pairingUrl`）；`node pair --mode access` → `local.unsupported` |
| `cli::tests::the_subcommand_set_matches_the_documented_mapping` | **R59** | clap 树与 §5.8 逐条一致；自造子命令不存在 |
| `cli::tests::flags_map_onto_the_documented_wire_field_names` | **R59** | 每个子命令的 flags → camelCase `params`；凭据值没有命令行/文件入口 |
| `cli::tests::the_failure_line_is_one_json_object_with_a_code` | **R62, R63** | stderr 行恰好一行、含 `code`；Daemon 返回的词表外码原样带出 |
| `cli::input::tests::the_no_echo_reader_returns_the_value_without_echoing_it` | **R68** | 关回显读取返回值且不把值写回输出汇（机制级证据） |
| `cli::input::tests::credentials_are_read_one_field_at_a_time_and_never_trimmed` | **R67, R68** | 逐项读取、凭据值原样（不 trim）进入 `values` |
| `cli::input::tests::a_non_object_file_and_an_unregistered_field_are_rejected_with_invalid_params` | **R67** | 顶层非对象/非法 JSON/未登记字段（含凭据字段）→ `local.invalid_params`，消息不回显凭据值与完整路径 |
| `cli::pairing::tests::supplied_values_must_match_the_status_values_verbatim` | **R66** | 前缀/大小写/空白/长度差异一律失败，消息不回显 SAS/指纹 |
| `cli::pairing::tests::non_interactive_confirmation_requires_both_values` | **R66** | 两个值必须同时给出；不得无参数自动确认 |
| `cli::pairing::tests::pairing_params_match_the_wire_fields` | **R64** | `begin`/`confirm` 的 `params` 字段名与 §5.3/§5.4 同名 |
| `cli::pairing::tests::the_claim_display_shows_every_required_field` | **R64, R65** | 展示块含名称/指纹/SAS/请求集合/过期时间（确认前的核对材料） |
| `cli::pairing::tests::pairing_states_are_classified_per_direction` | **R64** | 设备/节点的状态词表分流、终态与未知词 |
| `cli::pairing::tests::a_status_without_the_claim_fields_is_an_internal_error` | **R64** | `status` 缺字段 → `local.internal`（不静默用空值确认） |
| `lock::tests::probing_reports_free_held_and_never_creates_the_lock_file` | **R10** | `probe` 的三态且**不创建**锁文件 |
| `stdio::tests::binary_payloads_round_trip_through_both_directions` | **R72** | 含 NUL/非 UTF-8 的字节双向原样透传；有字节流过时对端关闭 = 正常结束 |
| `stdio::tests::an_empty_stdin_is_an_explicit_failure` | **R72** | stdin 未传出任何字节即明确失败 |
| `stdio::tests::an_immediately_closed_connection_means_no_acp_stream` | **R72** | 即连即关 → `NoStream`，stdout 保持为空 |
| `stdio::tests::frames_match_the_documented_shape` | **R72** | 组帧 = `u32be length | 0x02 | payload`，不进管理信封 |

## 9. checks（逐检查 ID 的 handoff_index）

| index | 检查 ID | 命令 / 判据 | 结果 | 证据 |
|---|---|---|---|---|
| 1 | **PV4**（本 WP 主检查） | `cargo test --locked -p app --all-features`：43 单元 + 1 + 11 + 9 = 64 passed / 0 failed / 0 ignored | **PASS** | `reports/wp4b-cli.log`（`EXIT(test)=0`） |
| 2 | **PV4** | `cargo fmt --all -- --check` → 0；`cargo clippy --locked -p app --all-targets --all-features -- -D warnings` → 0 | **PASS** | `reports/wp4b-cli.log`（`EXIT(fmt)=0`、`EXIT(clippy)=0`） |
| 3 | **PV4**（自检） | `rg -n "unsafe" crates/app/src` 零命中；`rg -n "rusqlite\|sqlx" crates/app/src/cli.rs crates/app/src/cli crates/app/src/stdio.rs crates/app/src/client.rs` 零命中；`SqliteStore` 在 CLI/stdio 里只出现于注释 | **PASS** | `reports/wp4b-cli.log` 末尾自检段 |
| 4 | **PV2** | `node scripts/check-crate-boundaries.mjs`：12 个 crate 与 §5 矩阵一致（新依赖 `rpassword` 只进 `app`） | **PASS** | `reports/du1-pv1.log`（`EXIT(check-crate-boundaries)=0`） |
| 5 | **PV1** | `npm run check`（合同门禁全量，串行） | **PASS** | `reports/du1-pv1.log`（`EXIT(npm-run-check)=0`） |
| 6 | **PV1**（提交门） | `.husky/pre-commit` 在 `c8e44eb` 上跑 fmt + `npm run check` + workspace clippy（`-D warnings`） | **PASS** | 提交输出（3 步通过；本地 gitleaks 未安装，按既有约定跳过） |
| 7 | **PV5** | Windows Named Pipe 语义属 WP2/WP3 的证据域；本 WP 的 `acp-stdio`/CLI 路径已在 Windows 开发机上跑通（11 个进程级用例） | **NOT RUN（不属于本 WP）** | `reports/pv5-windows-ipc.log`（既有） |
| 8 | **PV3** | `server` crate 用例与本 WP 无关，未重跑（未改 `crates/server/**`） | **NOT RUN（不属于本 WP）** | `reports/wp3-*.log`（既有） |

## 10. `rpassword` 的 §20 核验结论（本地实测）

| 项 | 结论 | 依据 |
|---|---|---|
| 版本 | **7.5.4**（最新非 yanked；`Cargo.lock` 固定） | `index.crates.io/rp/as/rpassword`（47 个版本，逐个 `yanked=false`） |
| 许可证 | **Apache-2.0**（`Cargo.toml` 的 `license` 字段；**不是** design.md 写的「MIT/Apache-2.0 双许可」——以实测为准，单许可与仓库 `license = "Apache-2.0"` 兼容） | 解包的 `rpassword-7.5.4/Cargo.toml` |
| MSRV | `rust-version = "1.85"`，与 workspace `rust-version = 1.85` 恰好一致（edition 2024 同码） | 同上 |
| 维护状态 | 作者 Conrad Kleinespel（`github.com/conradkleinespel/rpassword`，与 `rtoolbox` 同作者）；7.5.x 在 2026-04→2026-05 连续发布（7.5.0/1/2/3/4），近期活跃 | 索引 `pubtime` |
| 传递依赖 | `rtoolbox ^0.0`（→ `libc`（unix）/`windows-sys 0.61`（windows），其 `serde`/`serde_json` 是**可选 feature 且未开启**）、`libc ^0.2`（unix）、`windows-sys ^0.61`（windows）。`libc` 与 `windows-sys` 的版本都**已在 lock 中**（`Locking 2 packages`：只有 `rpassword` 与 `rtoolbox` 是新包） | `cargo fetch` 输出 + `Cargo.lock` |
| `unsafe` | `rpassword` 与 `rtoolbox` 的**内部**都有 `unsafe`（termios/`isatty`/`GetConsoleMode`/`ReadConsoleW`、`write_volatile` 清零），这是「关回显」与「安全清零」的必要实现手段；两个 crate 的公开 API 全是 safe，仓库的 `unsafe_code = "forbid"` 只约束自己的 crate | 解包源码逐处 `grep -n unsafe` |
| 是否进入 `core` 闭包 | **否**：只登记在 `[workspace.dependencies]`，只有 `crates/app` 依赖它（`check:boundaries` 与 `check:boundaries` 的 allow-list 判据均通过） | `EXIT(check-crate-boundaries)=0` |
| 接受性 | **接受**（不触发「停下并报告」）：单许可、MSRV 与固定工具链一致、无新增传递依赖版本、公开 API 全 safe、只负责「关回显读一行」。**残余风险**：`rtoolbox 0.0.x` 自称「no backwards compatibility guarantees」，版本面窄且可能随时变化；缓解是 `Cargo.lock` 固定 + 只用一个函数（`read_password_with_config`），升级需重跑 §20 核验 | 本表 |

## 11. issues（未执行项、就地裁定与待澄清）

**就地裁定（如实登记，均可一行回退）**

1. **`daemon status`/`daemon stop` 在未运行时的退出码 = 0**（输出「未运行」）。依据 §7 把这两条与
   「其余方法以非零退出码与明确 stderr 报错」并列区分：它们在未运行时仍要**在进程内回答**，即成功。若上游要求
   「未运行」也算失败，改 `cli.rs::daemon_status`/`daemon_stop` 各一行即可。
2. **用法错误（clap）退出码 = 2**，stdout 一行摘要 + stderr 一行 `local.invalid_request`。依据是「失败非零 +
   机器可读一行 JSON」这条统一契约覆盖所有失败；`2` 使「用法错误」与「运行期失败（1）」可区分。
3. **`provider configure` 先判终端、后判 Daemon**：无交互终端时即使 Daemon 未运行也回
   `local.invalid_request`（R69 要求的「不读取凭据、不调用方法」两条顺序都满足，且该判定纯 CLI 侧、无副作用）。
4. **交互式确认输入 `n`/EOF 时不调用 `*.pair.reject`**：CLI 不替用户做未请求的状态变更；未确认的配对自然到期
   （未确认前不可能是信任记录）。
5. **非交互比对失败用 `local.invalid_params`**（§6 里没有更贴切的码；不匹配是「用户给的参数与状态不符」）。
   `approved` 状态用 `local.conflict`、`rejected`/`expired` 用 `local.expired`（§6 原文：`local.expired` 服务
   「已过期或已被拒绝」）。
6. **`acp-stdio` 失败不写 stdout**（`fail_stream`）：stdout 是 ACP 通道，写人类可读文本会污染协议流。
7. **`acp-stdio` 在 stdin 未传出任何字节即 EOF 时失败**（`NoInput`）：此时无法建立 channel（§3 要求首帧定用途，
   而空载荷帧非法），静默退出会把「没有流」伪装成成功。
8. **`device pair` 新增 `--pack` 与 `--expires-in-ms`**（§5.3 已定义的参数；没有它们无法从 CLI 表达
   `requestedPacks` 与「收窄有效期」）。`daemon stop` 同理新增 `--grace-ms`。
9. **`--config` 改为全局参数**（`acp-remote --config X daemon status` 与 `daemon start --config X` 都可用）：
   §5.8 只规定子命令↔方法，未规定配置来源；这样所有子命令共用一条配置解析路径。
10. **成功输出 = `result` 的紧凑 JSON**：§5.2–§5.6 的字段名/类型就是展示模型，`result` 永不回显凭据值，
    因此不另造渲染层（`device pair` 额外打印一行 `已配对：deviceId=…`，与 §5.8 的示例一致）。

**未执行 / 未覆盖（如实记录）**

1. **真实终端上的凭据录入（R68 的端到端）无法自动化**：测试进程没有交互终端，`provider configure` 的真实录入
   路径只能靠 rpassword 的配置接缝单测（值与不回显都断言了）+ `IsTerminal` 门禁的进程级用例。真机人工验证属
   W5/验收阶段的可选项。
2. **交互式配对确认（R65 的 `y` 分支）无法进程级驱动**：需要一台真实设备 claim 配对（`server::sync` 不在本
   切片），因此 R65 覆盖到「认领后展示块」与「提交展示过的集合」的实现与单测，以及非交互分支；`y` 分支代码
   路径（`ask_confirmation`）只有单测覆盖（`Claim::display` + 状态分流）。
3. **`device pair`/`node pair` 的默认 5 分钟窗口**未在进程级用例里等满（用例用 `--expires-in-ms 1000` 走同一
   条到期路径）；「默认 5 分钟由方法侧固定」由 `server` 用例与 §5.3 合同负责。
4. **`import add` 的 `local.unavailable`（无 catalog 快照）路径**未进程级验证（本切片没有 Node Link 连接，
   `server` 侧已有用例）；CLI 侧只覆盖「未运行时失败」与参数映射单测。
5. **`doctor` 的在线分支只断言 `daemon.status` 的组合**，未对每个本地检查做负例（配置非法在 `Context::resolve`
   阶段就失败关闭，`data_dir` 不是目录的负例只有代码路径，没有专门用例）。
6. **`--file` 的顶层字段名白名单只用于 `agent configure`**：`export create` 的嵌套结构完全交方法侧（不复制
   §5.5 的校验规则），因此 CLI 侧不额外拦截嵌套字段里的凭据样式内容——§5.5 的 `export.create` 本来也没有凭据字段。
7. **本机未安装 `gitleaks`**：密钥扫描在本地跳过（`.husky/pre-commit` 已如实提示，不视为通过），由 CI 的
   `secrets` job 判定；`cargo-deny`/`advisories` 同理（`rpassword`/`rtoolbox` 的许可证已按本报告 §10 人工核对，
   但 `deny.toml` 的机器判定只在 CI 跑）。

**待澄清**

1. §11.1 的「未运行 = 退出码 0」是否是上游期望的「约定的明确输出/退出码」？若不是，需要指定「未运行」对应的
   错误码（`local.not_found`/`local.unavailable`）。
2. §11.2 的「用法错误退出码 2」是否需要纳入合同文档（`LOCAL_ADMIN_PROTOCOL.md` §5.8 只写「失败非零」）。
3. `rpassword` 的实际许可证是 **Apache-2.0 单许可**（design.md 写的是双许可），且带一个 `rtoolbox 0.0.x`
   传递依赖。是否需要把这两条事实写回 `design.md`/`MODULE_ARCHITECTURE.md` §3.1（属文档收口，本包不写 docs）。

## 12. result

- **交付完成**：`app::cli` 提供 §5.8 的全部子命令与唯一错误出口；`doctor` 与 `acp-stdio` 落地；CLI 侧零业务规则、
  不打开 SQLite（判定「是否运行」只读锁与实例记录）。
- **判定**：本 WP 的主检查 **PV4 PASS**；PV1/PV2 复核 **PASS**。
- **无未通过项**：未弱化任何断言、未使用 `#[ignore]`、未跳过任何可执行用例；上面列出的全部是「按任务分工不在本包
  范围内」「无法自动化的真机路径」或「需要上游裁定的就地裁定」。
- **提交**：`c8e44eb`（实现与测试）、本报告的 `docs(app)` 提交。**未 push、未开 PR、未合并**（无授权）。

## 13. evidence_paths

- `openspec/changes/daemon-cli-and-local-admin/reports/wp4b-cli.log`：fmt / clippy（`-p app`）/ 全量 `-p app` 测试
  的完整输出与显式 `EXIT(...)` 行 + 自检段（`rg unsafe` / `rg rusqlite|sqlx` / `rg SqliteStore`）。
- `openspec/changes/daemon-cli-and-local-admin/reports/du1-pv1.log`：追加段含 `check-crate-boundaries`（EXIT 0）与
  `npm run check`（EXIT 0）。
- `openspec/changes/daemon-cli-and-local-admin/reports/wp4b-handoff.md`：本报告。
- 两份 `.log` 按 `.gitignore` 的 `openspec/changes/**/reports/**/*.log` 约定**不入库**（与 WP1–WP4a 一致）：
  它们是本机证据文件，复核时需在本机重新生成（命令与参数见 §9）。

## 14. resource_cleanup

- CLI 用例的 stdout/stderr 临时目录由工装 `run_cli` 在子进程结束后删除；Daemon 用例的临时目录由 `TempRoot::Drop`
  删除（Windows 带重试）；`Daemon::drop` 对未退出进程 `kill` + `wait`（无遗留 Daemon 进程）。
- 每个 CLI 子进程都有 60 s 上限：超时即 `kill` 并让用例以「超时」失败，不会挂住测试运行（本轮唯一一次挂住是单元测试
  里的 `duplex` 半边未 drop，已修）。
- 构建目录仍为仓库共享 `target/`；未清理其他执行者的产物。
- 本轮结束后 `git status` 仅剩本报告（写完后即提交）。
