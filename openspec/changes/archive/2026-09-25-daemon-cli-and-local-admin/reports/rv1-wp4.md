# RV1-WP4 · 独立代码检视报告（WP4：crates/app 组合根 + Daemon 生命周期 + CLI/doctor/acp-stdio + 2.25/2.26）

> 本文件由 reviewer 子 Agent 的输出工件原样落盘（运行 bd0dd8ec-5c71-4340-9b00-c9047d3d35d1），
> 主 Agent 未改动其结论；后续处置见 verification.md 的 Review Findings 与 RV2 复核。

```yaml
review_id: RV1-WP4
review_type: branch
stage: work-package review
work_package: WP4
base_revision: 8051bb1
target_revision: f39dda9
result: PASS (4 findings, all SUGGESTION/MINOR, none blocking; merge verdict OK with notes)
agent: reviewer (结构只读, deepseek/deepseek-flash, fresh context)
```

## Review

- **result: PASS**（无阻塞、无 CRITICAL/MAJOR；4 条 P2 报告级发现，均不阻断 WP4 收口）
- Merge verdict: **OK with notes**
- Correct（已核实为好的部分）：CLI 确实是薄客户端（`crates/app` 全目录 `rusqlite|sqlx` 零命中，`SqliteStore` 只出现在 `compose.rs`/`daemon.rs`，唯一 `open` 在 `compose.rs:179`，只被 `daemon start` 调用）；退出码/双通道错误契约实现与断言都有反例能力；配对仪式严格 begin→status→confirm、非交互逐字匹配、无「无参数自动确认」、交互拒绝走 `*.pair.reject`；关闭顺序与 `SECURITY_DESIGN.md` §12.1 / `CORE_PORTS_AND_STORAGE.md` §7.1 第 4 条逐条一致；2.25-A 的合并窗口定时器真接线（而非 no-op），2.26 的 runtime 释放修正点顺序正确且带 RED→GREEN。
- Fixed: 无（本任务为只读 review，未做任何编辑）。

---

## 1. 核对清单逐条结论

**① CLI 薄客户端约束 — 成立。** `crates/app/**` 内 `rg` 式核对 `rusqlite|sqlx` 零命中；`SqliteStore` 命中仅：`src/config.rs:20`（只是 `StorageConfig` 别名 import）、`src/compose.rs:41/42/157/179/351/397`、`src/daemon.rs:45/541`（`maintenance_tick` 的参数类型）、`tests/audit_export.rs:4`（注释）。`cli.rs`/`cli/*`/`stdio.rs`/`client.rs`/`lock.rs` 无任何存储引用。CLI 侧共享环境 `Context` 只有 `Loaded + Option<Runtime>`（`cli.rs:429-435`），`daemon start` 之外不存在持 `SqliteStore` 的类型。业务规则为 0：`workspace_select_params` 等只做 kebab→camel 映射（`cli.rs:517-646`），`call_once`（`cli.rs:814`）只「一条连接一条请求」，配对轮询只调 `status`（幂等，不构成 mutation 自动重试）。

**② 退出码与 stderr 契约 — 成立。** 成功 `ExitCode::SUCCESS`（`cli.rs:324`）；`fail`（`cli.rs:937`）＝stdout 一行简述 + stderr `failure_line` 单行 JSON；`parse_failure`（`cli.rs:957`）用法错误退出码 2（非零）且 clap 多行诊断被压成一行；`fail_stream`（`cli.rs:944`）**不写 stdout**，`stdio.rs` 的 stdout 只写帧 body（`stdio.rs:118-120`）。断言有效性：`assert_failure_contract`（`tests/cli_commands.rs:23-33`）同时断言「stdout 恰好一行 + stderr 恰好一行含 code + 禁回显串」，`acp_stdio_*` 两条用例断言 `run.stdout.is_empty()`（`cli_commands.rs:583/605`）。

**③ 未运行时进程内回答 — 成立（断言真有反例能力）。** `running_endpoint`（`cli.rs:474-500`）只做 `lock::probe` + `read_record`；`probe`（`lock.rs:204-222`）`OpenOptions` 不带 `create`，锁不存在→`Free` 且不建文件。`cli_commands.rs:57-104`：把 `acp-remote.sqlite3` 造成**目录**（任何 SQLite 打开必失败）后仍 0 退出并答「未运行」，且 `dir_entries` 前后逐条相同；再放陈旧记录（记录存在、锁可加）仍答「未运行」。若实现退化为打开 DB 或建锁文件，两条断言都会失败——不是永真。

**④ 配对仪式 — 成立。** `run`（`pairing.rs:374`）→ `await_claim`（`:481`，begin→轮询 status，有效期以 `expiresAt` 字典序 + 330 s 本地硬上限）→ 先 `display`（`:397`，名称/指纹/SAS/请求集合/过期）→ 非交互 `verify_supplied`（`:363`）用 `String != String` 逐字比较（不 trim、不折叠大小写、不允许前缀），不匹配在 `settle_claim` 之前 `?` 返回 ⇒ 不调 confirm、不调 reject、不改状态、非零退出；`Supplied::parse`（`:220`）在「非交互 + 两值缺失」时失败，交互时不返回「自动确认」而是返回 `None`（走终端询问），**不存在无参数自动确认路径**；交互拒绝 `settle_claim`（`:455-470`）先把 `*.pair.reject{pairingId,reason:null}` 调成功，再以 `local.conflict` 失败退出且不调 confirm，reject 自身失败时原样上抛方法码。唯一行为细化（`ask_confirmation` 读 stdin 失败 → `local.internal`，不替用户拒绝）已在 wp425 §6 登记。

**⑤ 子命令↔§5.8 映射 — 成立。** 顶层 8 组 + `doctor` + `acp-stdio`；`the_subcommand_set_matches_the_documented_mapping`（`cli.rs:1002-1046`）逐条比 §5.8 并**反向**断言 `daemon doctor`/`session create`/`node rotate-key`/`audit export` 不存在；进程级用例再断言这 4 条是 `local.invalid_request` 用法错误（`cli_commands.rs:322-335`）。`doctor`（`cli.rs:735-786`）＝CLI 侧检查（config 已加载、dataDir 状态）+（运行中）`daemon.status`，无自造方法；无 `session create|list`。

**⑥ 组合根与生命周期 — 成立。** `fs4::FileExt::try_lock` 全限定（`lock.rs:106`、`probe` 处 `:211`；`unlock` `:158`），与 §3.1 的调用点约束一致，不会静默落到 std 的 1.89 同名方法。`instanceId` = `InstanceId::generate()`（`daemon.rs` 启动序列）→ `lock.publish`（`lock.rs:128`，临时文件 + rename 原子写）；陈旧记录判定＝`probe` 可加锁即未运行（`lock.rs:196-222`）。`nodeId` 由公钥确定性派生（`identity.rs` `node_id_for`：SHA-256(域分离 ‖ 65B SEC1) 前 16 字节 + v8/variant），与 `IDENTITY_AND_AUTH_CONTRACT.md` §7 第 7 条一致。`daemon.status` 各字段来源见 `AppDaemonControl::status`（`daemon.rs:255-330` 附近）：`listen`/`links` 恒为 `Vec::new()`，`counts` 由 core 查询（失败记日志计 0）、`agents` = profiles + `command_available`。endpoint 创建失败 → `DaemonError::Endpoint` → `local.unavailable` 且不发布记录、锁在退出时释放（`daemon_lifecycle.rs:165-190` 断言）。

**⑦ 关闭序列与任务取消 — 成立。** 实测顺序（`daemon.rs:1009-1075`）：`accept_loop` 退出时 drop endpoint（:1024 `ingress_stopped`）→ `drain_connections(deadline = max(grace_ms, 200ms))` → `tasks.cancel_all(deadline.min(now+5s))` + `audit_writer.close()` → `host.shutdown_all` + `drop(host)` → `composition.close()`（含 `wal_checkpoint(TRUNCATE)`，:1064）→ `cleanup_endpoint` + `lock.remove_record()` + `drop(lock)`。等于 §12.1 与 §7.1 第 4 条（「停接入层 → 取消周期任务与信号监听 → 停 Agent → checkpoint」）。三个周期任务全部经 `OwnedTasks { name, cancel: Arc<Notify>, handle }`（`daemon.rs:390-450`）持有；`cancel_all` 先 `notify_waiters()` 再逐个 `await`，只在超时 `abort` 并记 `task_stop_timeout`；「通知丢失」窗口由各循环里的 `shutdown.is_requested()` 兜底 ⇒ 无 detached task。宽限/取消超时都不被吞（`drain_timeout{remaining}`、`task_stop_timeout{task}` 都是 warn）。关闭期新请求由 `ShuttingDownGate` 返回 `local.unavailable`（有单测 + 进程级用例）。

**⑧ 合并窗口定时器（2.25-A）— 成立（一处低概率断言瑕疵见 F1）。** `spawn_merge_window`（`daemon.rs:661-735`）用 `tokio::time::interval(interval)` + `MissedTickBehavior::Delay`，每 tick `sessions()` → 逐会话 `pump`；`interval` 来自 `config.flush_interval_ms`（`daemon.rs:860` 附近），`0` 在配置层失败关闭（`config.rs` `flush_interval_ms == 0` → `local.invalid_params`）。枚举严格取非终态 `MERGE_WINDOW_STATES`（`daemon.rs:613-618`），过滤在 SQL 侧（`crates/storage-sqlite/src/session_store.rs:1916-1918` `state IN (...)`）——是「非终态子集」，不是全表；`SessionQuery` 无分页（`limit: None`）因此返回所有活动会话，不漏会话的依据是「状态转换与触发它的事件批次同事务」（§6 第 1/2 条，代码注释已写明依据）。`Actor::LocalCli` 在 `broker.authorize` 恒通过（`core/src/broker.rs:543`），因此枚举不会被授权拒绝。单会话 `pump` 失败 → `merge_window_failed` warn + 继续本轮其余会话 + 不结束任务；枚举失败 → `merge_window_scan_failed` warn + `continue`：既不静默也不致命。断言证据非永真：`while spy.scans() < 2` 保证任务真跑过、`scans <= elapsed_ms/20 + 2` 能捕获热循环、`pumps == scans*2` 需要「每 tick 对两个会话都 pump」才成立；`wp4wp5-final-verify.log:358` 显示该用例在冻结版本上实跑通过。**两处相等断言不是快照一致读**（F1）。

**⑨ 凭据与秘密 — 成立。** `provider configure` 只有 `--provider-id/--kind/--display-name/--field`（`cli.rs:167-183`）；`--value`/`--file` 被 clap 拒绝（`cli.rs:1160-1185` 单测断言）。值只经 `TerminalPrompt`（`cli/input.rs:84-96`：stdin 非终端 → `local.invalid_request`，**先于**读取与任何方法调用）→ `read_secret_line`（`input.rs:128-135`，rpassword 默认配置＝终端设备 + 关回显）。秘密不出现在输出：`provider_configure_params` 只把值放进 `params`；`ProviderConfigure` 结构体不含值字段；`failure_line` 只含 `code/message`；`read_params_file`（`input.rs:28-74`）对非白名单字段返回 `local.invalid_params` 且消息只带文件**名**、不回显值（单测断言 `!message.contains("sk-x")` 与 `!contains(dir)`）；`client_failure`（`cli.rs:870-898`）用固定英文短句，不带底层 io 路径。`pairingUrl` 只 `println!` 到终端（`pairing.rs:492`），不进日志/错误/文件；SAS/指纹不匹配的失败消息不回显这两个值（单测 + 进程级禁回显断言）。

**⑩ 2.26 runtime 释放 — 成立。** `Context::release_runtime`（`cli.rs:463-470`）显式 `take()` + `drop(Runtime)`；调用点（`cli.rs:705-711`）严格在 `drop(client)`（位于 `block_on` 的 async 块尾）之后、`wait_for_lock_release`（只 `probe` + `thread::sleep`）之前。调用后同函数不再取 runtime（`Context::runtime()` 若有调用会新建 runtime，但该路径无调用点）⇒ 无新竞态。回归断言 `daemon_lifecycle.rs:319-357`（无其它在途客户端时断言无 `drain_timeout` 且 `elapsed < 2/3 grace` + 锁可再取）具备区分能力：wp426 §4 留了 RED 3023 ms/EXIT 101 → GREEN 0.29 s。

**⑪ 依赖与边界 — 成立（一处登记缺漏见 F3）。** `crates/app` 的实际依赖全部落在 §5 矩阵 `app` 行的 ✓ 格子（core/storage-sqlite/identity-auth/identity-keystore/agent-host/server），未使用 acp-protocol/sync-protocol/node-link-protocol/acpr-*/windows-local-ipc（允许但不使用）；`check-crate-boundaries` EXIT=0（`wp4wp5-final-verify.log:30`「12 个 crate…与 §5 矩阵一致」）。`Cargo.lock:114-135` 的 `app` 条目与 `crates/app/Cargo.toml` 逐项一致；rpassword 7.5.4 + rtoolbox 0.0.6 是本次唯一两个新包（diff 工件第 216-241 行），与 wp4b §10 声明的「Locking 2 packages」一致；rpassword 只被 `app` 引用，不进 core 闭包（core 直接依赖仍为 async-trait/thiserror/p256/sha2）。`deny.toml` `[licenses] allow` 含 `"Apache-2.0"`，因此 rpassword 的**单许可**可通过；`rtoolbox` 的许可无任何文档登记，只有 CI 的 `deps` job 能判定。

**⑫ 测试有效性抽样 — 见 §3 表。** 抽 12 条：10 条具备反例能力（其中「不打开数据库」「stdout 为空」「顺序单调」「字段集合」类断言是真反例），1 条机制强但快照不一致（F1），1 条强度不足（`toml_error_detail` 的单行性质无任何断言）。

---

## 2. 发现清单（RV1-WP4-F1…F4）

### RV1-WP4-F1 — 合并窗口 spy 断言不是快照一致读（可低概率假失败）
- 位置（target 版本）：`crates/app/src/daemon.rs:1325-1334`（`the_merge_window_pumps_every_active_session_each_interval_until_cancelled`）与 `:1376-1383`（`a_failing_session_does_not_stop_the_rest_of_the_round_or_the_task`）。
- 触发条件：`let scans = spy.scans();` 与 `assert_eq!(spy.pumps(), scans * 2)` 之间若发生一次 tick 边界（20 ms 周期）：任务先 `scans += 1`，再逐个 `pump`（`pumps += 1/2`），即可读到 `pumps ∈ {2·scans+1, 2·scans+2}` → 断言失败。第二处（`pumps == scans*2` / `failures == scans`）同理。
- 预期/实际：预期对**同一快照**比较；实际是两次独立 load，跨 tick 时值不自洽。
- 影响：仅测试稳定性，出现概率极低（窗口 ~ns / 周期 20 ms，约 1e-5 量级），不是产品缺陷；冻结轮 PV1 未触发。不影响判据覆盖度（用例本身覆盖「按间隔运行」「失败不中止」是有效的）。
- 最小修复：把两处相等断言移到 `tasks.cancel_all(...)` 之后，对已有的 `frozen = (spy.scans(), spy.pumps())` 快照断言（测试 1 已具备该变量）；或让 spy 提供一次返回 `(scans, pumps, failures)` 的元组读。
- 严重度：**SUGGESTION**（P2，report-only）。

### RV1-WP4-F2 — `MODULE_ARCHITECTURE.md` §4.10 的关闭顺序与「刷盘」措辞与权威合同/实现矛盾
- 位置：`docs/MODULE_ARCHITECTURE.md` §4.10「后台任务的唯一清单」末两条（原文：「存储批量刷盘（`storage.flush_interval_ms`）」/「关闭顺序：停周期任务 → 停接入层 → 停 Agent → `wal_checkpoint(TRUNCATE)`」）。
- 触发条件：读者按 §4.10 判断关闭顺序或周期任务语义。
- 预期/实际：预期与 `SECURITY_DESIGN.md` §12.1、`CORE_PORTS_AND_STORAGE.md` §7.1 第 4 条一致（「停接入层（排空在途连接）→ 取消周期任务与信号监听（必须先于停 Agent）→ 停 Agent → 最后 `wal_checkpoint(TRUNCATE)`」，且 §7.1 明写「周期任务的取消位置…**不是**『先于接入层』」）；实际 §4.10 写的是「停周期任务 → 停接入层」。实现（`daemon.rs:1024→1042→1064`）与 §12.1/§7.1 一致，因此是 §4.10 陈旧。另 `§7.5` 第 5 条要求「§4.10 的后台任务清单必须与本节一致」，这行正是它要求收敛的对象。第二个措辞问题：该任务实际驱动的是 broker 的 **delta 合并窗口**（`Broker::pump`，`CORE_PORTS_AND_STORAGE.md` §6 第 10 条），不是「存储批量刷盘」（`storage-sqlite` 不参与，`StorageConfig` 注释已明写该键不在存储配置里）。
- 影响：仅文档；但属「权威文档互相矛盾」这一类，本变更已在 `CORE_PORTS_AND_STORAGE.md`（两处）、`AGENTS.md` §9、`DEVELOPMENT_PLAN.md` §3 清理了同类陈旧陈述，§4.10 这一行漏了。**注意：该行不在本 diff 的改动 hunk 内**（本范围只改 §3/§3.1/§4.1/§4.9/§4.10 状态行/§5 注记），因此不作为交付阻塞。
- 最小修复：该条改为「停接入层（并排空在途连接至宽限上限）→ 取消周期任务与信号监听 → 停 Agent → `wal_checkpoint(TRUNCATE)`」，并把「存储批量刷盘」改为「broker 的 delta 合并窗口（`storage.flush_interval_ms`，由组合根定时器驱动 `Broker::pump`）」。
- 严重度：**MINOR**（P2，报告级）。

### RV1-WP4-F3 — 新依赖 `rpassword` 未在 §3.1 的依赖登记里，且 `crates/app/Cargo.toml` 头注释陈旧
- 位置：`docs/MODULE_ARCHITECTURE.md` §3.1 第 143-149 行（「本切片新增的依赖口径」逐条记了 tokio/nix feature、`clap 4.6`、`toml 1.1`、`fs4 1.1`、vendor，**没有 rpassword**）；`crates/app/Cargo.toml:1`（「本切片只落地 `daemon start`；其余 CLI 子命令与 `acp-stdio` 属 WP4b」）；`crates/app/Cargo.toml:44-45` 与根 `Cargo.toml:95-97` 的注释把选型结论指向「WP4b 报告与 `docs/MODULE_ARCHITECTURE.md` §3.1」。
- 触发条件：后续维护者按 §3.1 复核新版依赖的版本/许可证/MSRV（该列表就是这类结论的落点）。
- 预期/实际：预期 §3.1 有 rpassword 条目（如同批 clap/toml/fs4）；实际 `docs/` 全文 grep `rpassword|rtoolbox` **零命中**，登记只存在于变更内 `design.md` §7 与 `verification.md:109`。同时 `crates/app/Cargo.toml` 的第一行注释在 WP4b 已交付后仍说 CLI 子命令「属 WP4b」。
- 影响：无行为影响；属依赖登记与源码注释漂移（注释指向不存在的小节）。
- 最小修复：§3.1 增一条（`rpassword 7.5`、Apache-2.0 单许可、MSRV 1.85、传递依赖 `rtoolbox 0.0.x` 的兼容性残余风险），或把两处注释的指向改为变更内 `design.md` §7；并删/改 `crates/app/Cargo.toml` 的陈旧注释行。
- 严重度：**SUGGESTION**（P2）。

### RV1-WP4-F4 — `plan.md` 的 PV4 证据链指向不存在的日志文件
- 位置：`openspec/changes/daemon-cli-and-local-admin/plan.md:105/113/482/490/498/505/513/520/528/536/543/551/559/566/574/582`（R10–R72 的 `evidence`）与 `:676`（PV4 检查定义行的证据列）——均写 `reports/wp4-app-cli.log`；同行的 `.../reports/wp4-app-daemon.log` 存在，`wp4-app-cli.log` **不存在**（全仓 `find` 无此文件）。
- 触发条件：复核者/最终验收按 check plan 的 evidence 列找 CLI 证据。
- 预期/实际：实际 PV4 的 CLI 证据是 `openspec/changes/daemon-cli-and-local-admin/reports/wp4b-cli.log`（wp4b-handoff §8、`wp4wp5-final-verify.log:375-390`），且 2.25/2.26 的断言证据在 `wp425.log`/`wp426.log`，plan 未登记。
- 影响：证据可追溯性（不影响代码、门禁或测试结论——`verification.md` 记的是实际文件名）。
- 最小修复：把 16 处 `reports/wp4-app-cli.log` 改为 `openspec/changes/daemon-cli-and-local-admin/reports/wp4b-cli.log`，并在 PV4 行的证据列补 `wp425.log`/`wp426.log`。
- 严重度：**SUGGESTION**（P2；plan.md 属判据工件、不在冻结 diff 的写范围内，故仅报告）。

---

## 3. 测试有效性抽样（12 条，判定其「实现退化时能否失败」）

| # | 断言位置 | 抽样断言 | 反例能力判定 |
|---:|---|---|---|
| 1 | `tests/cli_commands.rs:57-104` | 数据目录内 `acp-remote.sqlite3` 为目录仍 0 退出答「未运行」，且目录内容逐条不变 | **强**：CLI 一旦打开 DB 或建锁/记录/`-wal` 即失败 |
| 2 | `tests/cli_commands.rs:128-169` | 持锁 + 记录指向不存在 endpoint → `local.unavailable`，且 stdout 不含「未运行」；记录缺失同码 | **强**（含两个方向） |
| 3 | `tests/cli_commands.rs:212-336` | 13 个管理子命令在未运行时逐条非零 + 一行 `local.unavailable` + stdout 恰好一行；`provider configure` 非终端 `local.invalid_request` 且无 prompt；4 个自造入口用法错误 | **强**（覆盖映射与门禁两侧） |
| 4 | `tests/cli_commands.rs:570-587` / `589-609` | `acp-stdio` 失败时 `stdout.is_empty()`；在线时 Daemon 日志含 `local_admin.acp_facade_unavailable` | **强**（后者还证明 0x02 首帧确达服务端） |
| 5 | `tests/cli_commands.rs:612-661` | 非交互缺/半给 `--sas`/`--fingerprint` → `local.invalid_request` 且 **stdout 恰好一行**（间接证明 begin 未被调用）；1 s 有效期 → `local.expired` 且 stdout 含 `pairingUrl`；`node pair --mode access` → `local.unsupported` | 中–强：`--sas` 不匹配的分支只由单测覆盖，进程级未构造「有人认领」的配对（本切片无 `server::sync`），且未直接断言「未创建配对记录」 |
| 6 | `tests/daemon_lifecycle.rs:319-357` | 无其它客户端时无 `drain_timeout` 且 `elapsed < 2/3 grace`，锁可再取 | **强**（RED/GREEN 已留证：3023 ms/EXIT 101 → 0.29 s） |
| 7 | `tests/daemon_lifecycle.rs:360-490` | 关闭期在途连接得 `local.unavailable` 或连接已关闭；关闭事件顺序单调；`-wal` 0 字节；记录被删；重启后 `ghost` 不存在 | **强**（顺序用 `<=` 而非 `<`，但每个事件各占一行，等价于严格序；「无状态变更」靠重启后复查，可接受） |
| 8 | `tests/daemon_lifecycle.rs:591-624` | `daemon.status` 字段集合恰为 §5.2 的 12 个 + `startedAt` 定宽 24 字符 | **强**（多字段/少字段都会失败） |
| 9 | `src/cli/pairing.rs:778-800` | 前缀/大小写/空白/长度差异 6 组反例一律 `local.invalid_params` 且不回显 SAS/指纹 | **强** |
| 10 | `src/cli/pairing.rs:706-762` | 拒绝路径方法序列**恰好** `[device.pair.reject]`/`[node.pair.reject]`、params `{pairingId,reason:null}`、退出码非零；确认路径只调 confirm；reject 失败原样带出方法码 | **强**（注入式 spy，正是 2.25-C 要求的反例能力） |
| 11 | `src/daemon.rs:1305-1350`、`:1360-1390` | 合并窗口 spy：`scans ≥ 2`、`pumps == scans*2`、`scans <= elapsed/20+2`、取消后冻结；失败会话每轮恰 1 次失败且其余会话仍 pump | 机制强，但相等断言**不是快照一致读**（F1，低概率假失败） |
| 12 | `src/config.rs:460-477`（`toml_error_detail`）+ `config.rs:1054-1076` | 只有 `message.contains(...)`，**没有任何断言**固定「单行、不含源码片段、≤240 字符」 | **强度不足**：把实现改回原样多行诊断，单测照过；而 CLI 的 stdout「一行简述」会随之破形，现有用例没有「非法 TOML 走 CLI」的路径。最小修复：给 `toml_error_detail` 补 1 条断言 `!contains('\n')` 且不含配置源码片段的单测 |

---

## 4. 需调度者执行的命令清单

| # | 命令 | 目的 / 说明 |
|---:|---|---|
| 1 | `cargo test --locked -p app --all-features` @ f39dda9 | 复核附录断言集（49 lib + 1 + 11 + 10）；已在 `wp4wp5-final-verify.log` 内 EXIT=0，无需重跑除非另有改动 |
| 2 | `cargo test --locked -p app --all-features -- --nocapture`（重复 3–5 次） | 仅用于评估 F1 的稳定性；不改代码时不必要 |
| 3 | `npm run check`（或 `npm run verify`） | 若采纳 F2/F3/F4 任一并改了 `docs/**` 或 `plan.md`，必须重跑合同门禁（doc-links/drift/agentic 等） |
| 4 | CI `deps` / `advisories` / `secrets` 三个 job（本地无等价物） | 判定新增 `rpassword 7.5.4` + `rtoolbox 0.0.6` 的许可与 advisory（`rtoolbox` 的许可证目前**没有任何文档登记**）；AGENTS.md §8/§10 明写本地绿不等于这三项通过 |
| 5 | Linux CI `checks` job | 执行 `#[cfg(unix)]` 的 `an_unusable_endpoint_refuses_start_without_degrading`（本机 Windows 不编译、不计入 730 passed）；本机同样不编译 `#[cfg(windows)]` 的 accept 取消用例 |
| 6 | 若要做合并窗口端到端验证 | 本切片**无法**构造 owned 会话（`LOCAL_ADMIN_PROTOCOL.md` §5.8 不含 `session.create`），因此没有可用命令；只能等 `session.create`/流式写路径接入后再验（见 §5） |

---

## 5. 未覆盖 / 无法验证（明确声明）

1. 我是结构只读子 Agent（无 shell），**未运行任何 `git`/`cargo`/`npm` 命令**，也**未独立复算**测试通过性；所有结论来自 f39dda9 的工作树文本 + 调度者提供的 `rv-input-wp4.diff.log` 与 `wp4wp5-final-verify.log`（后者我核对了 app 各测试目标的行数与 `test result: ok` 行：lib 49 / audit_export 1 / cli_commands 11 / daemon_lifecycle 10，ignored 全 0）。
2. **rtoolbox 0.0.6 的许可证/advisory**无法核实（无 registry/网络访问）；`deny.toml` 只保证 rpassword 的 Apache-2.0 单许可可过。
3. **合并窗口的端到端效果无法验证**：本切片没有 owned 会话，`list_sessions` 恒回空集，`pump` 从不真正落盘任何 delta；现有证据只有 (a) spy 断言、(b) 实现者探针里真实 Daemon 每 tick 执行 `SELECT … FROM owned_session WHERE state IN (?,?,?,?)` 与 `interval_ms` 匹配（wp425 §1.4，我未复算）。「某条 delta 被合并为一次 commit」属后续切片。
4. **R68 的真实终端录入**与 **R65 的交互式 `y` 分支**没有进程级覆盖（需要真 TTY / 真实设备 claim 配对），仅有单测接缝；我未做真机验证。
5. **平台 keystore 路径**（`FileKeystore`/DPAPI）在本机集成测试中被 `dev_mode + ephemeral` 绕过，未被这些用例覆盖（wp4a 已如实登记）。
6. 我未观察到 F1 的实际失败（它是代码级推断的低概率竞态），因此「F1 是否真的会假失败」不能算已复现；保守表述为「不需要修，若顺手可在同一改动内改成快照后断言」。
7. `#[cfg(unix)]` / `#[cfg(windows)]` 两处平台用例各自只在对应平台编译，我没有跨平台证据。

---

```yaml
handoff_index:
  task_id: "3.8"
  role: reviewer
  phase: work-package review
  stage: work-package review（WP4 交付前）
  target_revision: f39dda9
  evidence_type: REVIEW
  evidence_id: RV1
  report_path: openspec/changes/daemon-cli-and-local-admin/reports/rv1-wp4.md
  result: PASS
  evidence_status: NEW
  applicability_basis: >
    RV1-WP4：对 WP4（WP4a 组合根/Daemon 生命周期 + WP4b CLI/doctor/acp-stdio + 收口 2.25/2.26）冻结版 f39dda9
    的独立静态对抗 review（base 8051bb1）。依据 diff 工件
    openspec/changes/daemon-cli-and-local-admin/reports/rv-input-wp4.diff.log 与 target 版本文件全文；
    PV1/PV2 由调度者提供的 reports/wp4wp5-final-verify.log 佐证（EXIT=0，82 目标 / 730 passed / 0 failed / 2 ignored，
    check-crate-boundaries EXIT=0）。checklist ①–⑫ 逐条给出 target 版本行号证据；发现 4 条均为 P2 报告级，
    无 CRITICAL/MAJOR，故 result=PASS 且 merge verdict=OK with notes。
  source_evidence: NOT_APPLICABLE
```