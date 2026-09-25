# WP4a 交接：`app` crate（组合根 + Daemon 生命周期，任务 2.14）

```yaml
task_id: WP4a（tasks.md 2.14；本轮的 CLI 子命令体属 WP4b / 2.15–2.17）
role: 实现 Agent（coder，受限 worker；只写 crates/app/**、root Cargo.toml members、§5 的 storage-sqlite 列、本报告与日志）
phase: apply（WP4 第一段；W3 完成点的一部分）
agent_context: >
  上一轮被宿主 30 分钟超时中止（非代码失败），工作树未提交且完好；本轮按「限定步骤」恢复：
  复核状态 → 复跑 fmt/clippy/test（追加日志，不重跑）→ boundaries + npm run check → 显式路径提交 →
  写本报告。未修改 crates/core、crates/storage-sqlite、crates/server、vendor、schemas、fixtures、
  compatibility、其他 docs（§5 仅新增 storage-sqlite 一列）、planning 文件。
target_revision:
  branch: feat/daemon-cli-and-local-admin
  branch_point: e7b69f0（本轮实现所在的直接父提交为 8051bb1，主 Agent 的 PRO-3 文档提交）
  base: 8051bb1
  implementation_commit: e9b8e1d   # feat(app): 落地组合根、Daemon 生命周期与本地管理通道客户端（18 files, +6389/-16）
  test_commit: c628e87             # test(app): 补完整清理周期与关闭期取消顺序用例（1 file, +51）
  head_at_handoff: c628e87
scope:
  - crates/app/**（新增 crate：src 11 个文件 4835 行 + tests 3 个文件 1297 行）
  - root Cargo.toml（members += "crates/app"）
  - Cargo.lock（新增 app 成员与 clap/toml/fs4/uuid 等已登记依赖的解析结果）
  - docs/MODULE_ARCHITECTURE.md §5 矩阵：新增 storage-sqlite 列 + app 行的 ✓（唯一允许的 §5 改动）
  - openspec/changes/daemon-cli-and-local-admin/reports/{wp4-app-daemon.log, du1-pv1.log, wp4a-handoff.md}
```

## 1. changes（改动清单）

| 文件 | 行数 | 内容 |
|---|---:|---|
| `crates/app/Cargo.toml` | 60 | package `app` + `[[bin]] acp-remote`；依赖逐条对齐 §5 矩阵（含 `agent-host`/`storage-sqlite`/`server`/`identity-*`/`fs4`/`clap`/`toml`） |
| `crates/app/src/main.rs` | 8 | `fn main() -> ExitCode { app::cli::run() }` |
| `crates/app/src/lib.rs` | 35 | 模块与再导出（`LocalAdminClient`/`ClientOutcome`/`Config`/`NodeIdentity`/`DaemonLock`/`LockRecord`/`read_record`） |
| `crates/app/src/config.rs` | 1090 | `CONFIG_REFERENCE.md` 的键名/默认值；consumed / unwired 分流；dev_mode 门控；无新增配置键 |
| `crates/app/src/clock.rs` | 133 | `SystemClock`（`dyn Clock`）+ 毫秒→§1.1 定宽文本 |
| `crates/app/src/identity.rs` | 209 | `NodeIdentity::ensure`（`node-identity/primary`）、`node_id_for(公钥)` |
| `crates/app/src/lock.rs` | 326 | 单实例锁（`fs4`）+ 运行记录（`daemon.instance.json`） |
| `crates/app/src/logging.rs` | 304 | 最小 `tracing` 订阅器（level/format/file，无新依赖） |
| `crates/app/src/compose.rs` | 1090 | 组合根装配、种子导入、`close()`、`CredentialResolver`/`AuditSink`/`RevocationCloser`/`LoggingPublisher`/`UuidIdGenerator` |
| `crates/app/src/daemon.rs` | 1139 | 启动/关闭序列、`DaemonControl`、门闸、接收循环、三个有主任务 |
| `crates/app/src/client.rs` | 336 | 本地通道客户端（组帧/解响应），WP4b 复用 |
| `crates/app/src/cli.rs` | 165 | `daemon start`（本切片唯一子命令）+ stderr 单行 JSON 错误行 |
| `crates/app/tests/support/mod.rs` | 532 | 进程级测试工装（真实二进制 + 独立临时目录 + 就绪轮询 + 强制清理） |
| `crates/app/tests/daemon_lifecycle.rs` | 575 | 10 个 `daemon-lifecycle` 用例（R1–R16 可判定部分） |
| `crates/app/tests/audit_export.rs` | 190 | `audit.export` 端到端（真实 SQLite → 真实文件 + `recordCount`/`sha256`） |
| `Cargo.toml` | ±1 | `members += "crates/app"` |
| `Cargo.lock` | +N | 新成员的依赖闭包 |
| `docs/MODULE_ARCHITECTURE.md` | ±32 | §5 表格新增 `storage-sqlite` 列与 `app` 行 ✓（其余行补空单元格） |

## 2. 组合根装配图（谁构造谁）

```text
cli::run ──> Config::load(ACP_REMOTE_CONFIG | --config | 平台默认) ──> logging::init ──> daemon::run(Loaded)

Composition::assemble(config)                                # crates/app/src/compose.rs:168
 ├─ SystemClock::new()                    → Arc<dyn Clock>
 ├─ SqliteStore::open(storage_config, &now)                # 全 crate 唯一的 DB 打开点
 │    └─ 同一实例向上转型成 7 个端口：SessionStore / RemoteDeliveryStore / ExportStore /
 │       TrustStore / AuditStore / LocalConfigStore / AttachmentStore
 ├─ OsEntropy::new()                      → Arc<dyn EntropySource>
 ├─ build_keystore(config, entropy)       → Arc<dyn IdentityKeystore>（FileKeystore | EphemeralKeystore）
 ├─ NodeIdentity::ensure(keystore)        → node_id（派生）+ KeyHandle("node-identity/primary")
 ├─ UuidIdGenerator                       → Arc<dyn IdGenerator>
 ├─ KeystoreCredentialResolver(keystore, local_config) → Arc<dyn CredentialResolver>
 ├─ Authority::new(node_id, key, keystore, entropy, clock)
 ├─ AgentHost::new(local_config, credentials, HostConfig::default(), ids, clock)
 │    └─ 向上转型成 Arc<dyn SessionBackendFactory> 与 Arc<dyn AgentCatalog>
 └─ UseCases::new(UseCaseDeps {
        broker: Broker::new(BrokerDeps { store, deliveries, backends, exports,
                                         publisher: LoggingPublisher, clock, ids,
                                         audit: Some(audit_store) },            # persist_deltas ← storage.*
                                   BrokerConfig { persist_deltas, ..Default }),
        store, deliveries, exports, trust, audit, config, attachments,
        catalog, clock, ids })

daemon::run：
  1 config 校验/日志（unwired 逐键 debug、dev_mode 与 public_origin 警告）
  2 Composition::assemble
  3 composition.seed_if_needed()            # 种子 profile + seed_state 单事务（§6/§7 的首次初始化）
  4 use_cases.recover_unsettled(Actor::LocalCli, ReplayLimit)   # 取锁与监听之前
  5 InstanceId::generate() + DaemonLock::acquire(<dataDir>/daemon.lock)
  6 AuditWriter::start(composition.audit_store(), clock, node_id) → (AuditSink: Arc<dyn AuditHook>, writer)
  7 LocalEndpoint::bind(LocalEndpointConfig { data_dir, instance_id, runtime_dir: None }, audit_hook)
  8 lock.publish(LockRecord { instanceId, pid, endpoint })      # endpoint 定位串落盘（CLI 依赖）
  9 AppDaemonControl { …, core: Arc<UseCases>, shutdown }        → Arc<dyn DaemonControl>
 10 LocalAdminRouter::new(LocalAdminDeps { daemon, core, keystore, audit, clock, pairing })
      pairing = PairingSessions::new(authority, public_origin, RevocationCloser)
 11 ShuttingDownGate { inner: router, shutdown } → LocalConnectionHandlers::new(...) → Arc
 12 OwnedTasks：spawn_maintenance / spawn_storage_flush / spawn_signal_watcher
 13 accept_loop(endpoint, handlers, shutdown)                    # 见 §5 的「不可取消」注记
```

## 3. `DaemonStatus` 每个字段的来源

| 字段 | 来源 | 本切片取值 |
|---|---|---|
| `version` | `env!("CARGO_PKG_VERSION")`（`AppDaemonControl` 构造时固化） | `0.0.0` |
| `instanceId` | `InstanceId::generate()`（`daemon.rs:697`）→ 锁记录 → 控制对象 | 16 字符小写 hex，运行期内不变 |
| `nodeId` | `Composition::identity().node_id()`：由节点身份公钥派生（§6.1） | 规范 UUID 文本 |
| `nodePublicKey` | 同一 `PeerPublicKey`（65 字节 SEC1 未压缩 P-256），序列化为无填充 base64url | 87 字符 |
| `startedAt` | `Clock::now()`（`SystemClock`）在 `assemble` 早期的取值 | §1.1 定宽文本 |
| `uptimeMs` | `composition.started_instant()`（单调 `Instant`）与查询时刻之差 | 毫秒 |
| `dataDir` | `config.data_dir`（配置文本回显） | 绝对路径 |
| `publicOrigin` | `config.public_origin`（未配置 → `null`） | 配置值 |
| `listen` | **恒空数组**（本切片无网络 listener） | `[]` |
| `links` | **恒空数组**（无 Node Link 连接管理器） | `[]` |
| `counts.devices/nodes/exports/imports` | core 查询 `devices/nodes/exports/imports(Actor::LocalCli)` 的行数（查询失败记日志并计 0） | 与持久记录一致 |
| `agents[]` | core `profiles(LocalCli)` + `command_available()`（决策 6：「profile 存在且 command 可解析」） | `{agentId, available}` |

## 4. 锁与 `instanceId`

- **`instanceId`**：`server::transport::local::InstanceId::generate()`（`Uuid::new_v4()` 的 simple 文本前 16 字符 = 64 bit CSPRNG），仅在启动序列取一次；形状由 `InstanceId::new` 校验（16 字符小写 hex）。
- **互斥**：`crates/app/src/lock.rs:106` 的 **全限定**调用
  ```rust
  match fs4::FileExt::try_lock(&file) { … }
  ```
  写成 `file.try_lock()` 会解析到 `std::fs::File::try_lock`（1.89 稳定）而把 MSRV 抬到 1.89，因此必须全限定（同处注释说明）。`TryLockError::WouldBlock` → `LockError::Held` → `DaemonError::AlreadyRunning` → CLI `local.conflict` 非零退出；不杀进程、不降级。`Drop` 里 `fs4::FileExt::unlock`（`lock.rs:158`）。
- **锁文件与运行记录分家（对 design 措辞的必要偏离）**：`<dataDir>/daemon.lock`（只承载「能否加锁」，内容为空）与 `<dataDir>/daemon.instance.json`（`{"instanceId","pid","endpoint"}`，camelCase + `deny_unknown_fields`）。原因：Windows 的字节范围锁是**强制**的，被锁区间连读取都会被拒（本机实测 `ERROR_LOCK_VIOLATION`，os error 33），把记录写进被锁文件会让 CLI **永远读不到 `instanceId`/`endpoint`**。
  - 记录用「临时文件 + rename」原子写出；正常关闭在关闭序列第 5 步删除记录；锁文件**常驻**（unlink + advisory 锁并用会产生两个 inode 各自加锁的经典竞态），因此 CLI 必须**先尝试加锁**再读记录（§7：不把「文件存在」当运行判据）。
  - `endpoint` 字段是 CLI 唯一可行的定位手段：Windows 上 pipe 名含当前用户 SID 摘要，而取 SID 需要 `windows-local-ipc`（§5 矩阵不允许 `app` 依赖它）。**WP4b 的 `daemon status/stop` 依赖本字段**。
- 参考实现位置：`DaemonLock::acquire`（`lock.rs:95`）、`publish`（`lock.rs:128`）、`lock_path`（`lock.rs:165`）、`record_path`（`lock.rs:170`）、`read_record`（`lock.rs:180`）。

## 5. 启动序列与关闭序列（实际顺序）

**启动**：load config → `Composition::assemble`（含 `SqliteStore::open` 与迁移）→ `seed_if_needed`（单事务写种子 profile + 「已初始化」标记）→ `recover_unsettled` → 取单实例锁 → 起审计写任务 → `LocalEndpoint::bind` → 发布运行记录 → 控制对象/路由/门闸 → 三个后台任务 → `daemon.ready` → 接受循环。

**关闭**（`daemon.rs::close`，与 §12.1 逐条对应，事件名即日志锚点）：

| 顺序 | 动作 | 日志锚点 |
|---:|---|---|
| 0 | `daemon.stop` 请求被接受（响应先写回） | `daemon.stop_accepted` |
| — | 进入关闭序列 | `daemon.shutdown_begin`（含 `reason` 与 `grace_ms`） |
| 1 | 停接入层：`accept_loop` 退出时 drop 接收端 + `acceptor.abort()` → 接受任务结束并 drop `endpoint`（释放 listener） | `daemon.ingress_stopped` |
| 2 | 排空在途连接（宽限 = `max(graceMs|配置值, 200ms)`；超时 abort 并记 `daemon.drain_timeout`） | `daemon.drain` |
| 3 | 取消后台任务（协作式；超时才 abort）+ 关闭审计写任务 | `daemon.task_stopped` ×3 |
| 4 | 停止 Agent 进程（`AgentHost::shutdown_all`，本切片无已启动 Agent 立即返回）+ `drop(host)` | `daemon.agents_stopped` |
| 5 | 刷新存储：`Composition::close()` 释放全部共享句柄 → `SqliteStore::close()`（含 `wal_checkpoint(TRUNCATE)`）；仍被共享则**显式报错** `StoreStillShared`（不静默跳过） | `daemon.storage_closed` |
| 6 | 清理 endpoint 残留（Unix socket；不跟随符号链接）+ 删除运行记录 + 释放单实例锁 | `daemon.stopped` |

**`instance_id`/锁失败路径**：锁被占用 → `local.conflict`；endpoint 创建失败（符号链接/非 socket/权限不可保证/已被占用）→ `local.unavailable`；配置非法 → `local.invalid_params`；全部为**显式错误退出**（stdout 一行人类可读 + stderr 恰好一行 `{"code","message"}`）。

## 6. 周期任务与取消方式

- `spawn_maintenance`：启动初清理**立即执行**（`reason = "startup"`），之后每 60 s（`MAINTENANCE_INTERVAL`）一轮 `prune` + `expire_pairings` + `sweep_orphans(limit 256)`；三者各自失败只记 `daemon.maintenance_failed` 并继续下一步；成功记 `daemon.maintenance`（字段 `reason/removed_events/still_over_limit/expired_pairings/swept_orphans`）。
- `spawn_storage_flush`：按 `storage.flush_interval_ms` 周期运行；**本切片是文档化的空转**——所有写入都是「一次调用 = 一个事务」，`SqliteStore` 未暴露 checkpoint 接口（且 `app` 不得改 storage），真正的刷盘在关闭序列第 5 步统一完成。
- `spawn_signal_watcher`：Ctrl+C / SIGTERM → `ShutdownSignal::request_from_signal(reason)`（reason 进关闭日志）。
- **所有者与取消路径**：`OwnedTasks { name, Arc<Notify>, JoinHandle }`；`cancel_all(deadline)` 先 `notify_waiters()` 再逐个 `await`（只有超时才 `abort` 并记 `daemon.task_stop_timeout`），任务自行在等待点退出 → 无 detached task、不会在事务中途被 abort。
- 机制级覆盖：`daemon::tests::owned_tasks_stop_a_due_periodic_task_on_cancel`（20 ms 短周期、取消后计数不再增长）；进程级覆盖：R15/R16 两个用例（见 §7）。

## 7. 测试清单（进程级用真实二进制 + 独立临时目录）

`cargo test --locked -p app --all-features` → **EXIT 0**：27 单元 + 1（`audit_export`）+ 9（`daemon_lifecycle`）= 37 passed，0 failed，0 ignored。日志：`reports/wp4-app-daemon.log`（含每组命令的显式 `EXIT(...)` 行）。

| 用例（文件） | 覆盖的 R | 要点 |
|---|---|---|
| `a_fresh_start_takes_the_lock_creates_the_endpoint_and_imports_the_seeds` | **R1, R2, R5/R6（种子写入侧）, R9（字段）** | 锁文件存在；`instanceId` 16 hex 且与 status 一致、pid 一致；endpoint 可用（status 能回答）；§5.2 字段齐全、`listen`/`links` 为 `[]`、`counts` 全 0、种子 profile `available=true`；`daemon.ready.endpoint` == 锁记录 endpoint |
| `a_second_start_is_rejected_with_local_conflict_and_leaves_the_first_running` | **R3** | 第二个进程非零退出、stderr 恰好一行 `local.conflict`；第一个实例仍运行、instanceId 不变、记录未被改写 |
| `an_unusable_endpoint_refuses_start_without_degrading`（`#[cfg(unix)]`） | **R4** | `$XDG_RUNTIME_DIR/acp-remote` 为符号链接 → 拒绝启动（`local.unavailable`）、不发布运行记录；移除符号链接后同一数据目录可正常启动（**锁已释放**） |
| `an_explicit_endpoint_configuration_is_refused_before_any_side_effect` | **R4（补充，跨平台）** | 本切片无法创建显式 endpoint → 配置阶段失败（`local.invalid_params`），不建数据目录、不写日志（失败关闭、无副作用、不静默按 `auto` 启动） |
| `local_configuration_survives_a_restart_and_seeds_are_not_reimported` | **R6（标记效果）, R7** | 运行期 `agent.configure` 新增 profile → 停止 → 同数据目录重启：日志 `daemon.seed_skipped` 恰一次，两个 profile 均在（种子不覆盖运行期改动） |
| `status_counts_follow_persisted_records` | **R8, R9** | `counts.exports/devices/imports` 与 `export.list`/`device.list`/`import.list` 的行数逐项一致（建 1 个 Export 后 = 1） |
| `the_status_result_keeps_the_documented_field_set` | **R9** | 字段集合恰为 §5.2 的 12 个；`startedAt` 为 §1.1 定宽文本 |
| `stop_is_accepted_first_and_requests_during_shutdown_are_unavailable` | **R12, R13, R14** | `daemon.stop` 回 `{accepted:true}` → 关闭期间在途连接得到 `local.unavailable`（或 endpoint 关闭）→ 进程 0 退出 → 关闭事件顺序单调（begin→ingress→tasks→agents→storage→stopped）→ 锁可再取 → `-wal` 0 字节 → 记录被删除 → 重启后 `ghost`/`ghost-2` 不存在（**无状态变更**） |
| `the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown` | **R15, R16** | 启动初清理存在（`reason=startup`）；关闭 < 10 s（不等 60 s）；三个任务都记 `daemon.task_stopped` 且早于 `daemon.stopped`；取消后无新 tick |
| `the_periodic_task_runs_again_after_one_full_cycle`（约 61 s） | **R15, R16** | 启动初清理恰一轮 → 5 s 内无第二轮（周期不自旋）→ 75 s 内出现 `reason=periodic` 的第二轮 → 关闭时 `daemon.task_stopped` 早于 `daemon.agents_stopped` 与 `daemon.storage_closed`（**先于停 Agent/刷盘被取消**） |
| `audit_export_writes_the_real_file_and_reports_matching_count_and_digest` | **R53/R54/R55（补充证据，PV4）** | 真实 `Arc<SqliteStore>` 作 `AuditStore`：`provider.configure` + `export.create` 产生审计行 → `audit.export` 写出真实文件；`recordCount` == 行数、`sha256` == 文件字节摘要、每行含文档化列、actorKind=cli/outcome=success；**凭据值不出现**；重复导出同路径 = `local.conflict` 且不改写文件；`categories` 过滤生效（jsonl + csv） |

跳过的用例与原因（如实记录）：
- `an_unusable_endpoint_refuses_start_without_degrading` 是 `#[cfg(unix)]`：在 Windows 开发机上**不编译**（不计入 passed/skipped），由 Linux CI 的 `checks` job 执行；
- 无 `#[ignore]`、无「全跳过」的目标，也没有 0 用例的测试目标（`main.rs`/doc-tests 的 0 用例是空目标，非跳过）。

## 8. checks（逐检查 ID 的 handoff_index）

| index | 检查 ID | 命令 / 判据 | 结果 | 证据 |
|---|---|---|---|---|
| 1 | **PV4**（本 WP 主检查，任务 2.14） | `cargo test --locked -p app --all-features`：R1–R9/R12–R16 的进程级用例全部执行且通过（37 passed / 0 failed / 0 ignored） | **PASS** | `reports/wp4-app-daemon.log`（`EXIT(test)=0`） |
| 2 | **PV4**（fmt/clippy 子项） | `cargo fmt --all -- --check` → EXIT 0；`cargo clippy --locked -p app --all-targets --all-features -- -D warnings` → EXIT 0 | **PASS** | `reports/wp4-app-daemon.log` |
| 3 | **PV4**（自检） | `rg -n "unsafe" crates/app/src` 零命中；`rg -n "rusqlite\|sqlx" crates/app/src crates/app/tests` 零命中（驱动名不进入 `app`；`SqliteStore` 只出现在组合根与维护任务的参数类型） | **PASS** | `reports/wp4-app-daemon.log` 末尾自检段 |
| 4 | **PV2** | `node scripts/check-crate-boundaries.mjs`：12 个 crate 与 §5 矩阵一致（新增 `storage-sqlite` 列 + `app` 行的真实 path 依赖边成立） | **PASS** | `reports/du1-pv1.log`（`EXIT(check-crate-boundaries)=0`） |
| 5 | **PV1**（合同门禁部分） | `npm run check`（合同门禁全量：doc-links/契约漂移/封闭词表/agentic/边界/schema/fixture 等，串行） | **PASS**（3 次独立运行均 EXIT 0） | `reports/du1-pv1.log`（`EXIT(npm-run-check)=0`） |
| 6 | **PV1**（workspace 子项） | `cargo test --locked --workspace --all-features`（682 passed / 0 failed）；workspace 形式的 `cargo fmt --check` 与 `cargo clippy -D warnings` 由 `.husky/pre-commit` 在 `e9b8e1d`、`c628e87` 两次提交上执行并通过 | **PASS** | `reports/du1-pv1.log`（`EXIT(cargo-test-workspace)=0`） |
| 7 | **PV5**（R4 的 Windows 端） | 本机 Windows Named Pipe 语义（SDDL/对端 SID/pipe 名）属 WP2/WP3 的证据域 | **NOT RUN（不属于本 WP）** | `reports/pv5-windows-ipc.log`（既有） |
| 8 | **PV3** | `server` crate 的用例与本 WP 无关，未重跑 | **NOT RUN（不属于本 WP）** | `reports/wp3-*.log`（既有） |

## 9. issues（未执行项、越界判定与待澄清）

**未执行 / 未覆盖（如实记录）**

1. **R10「Daemon 未运行时查询状态」与 R11「有锁但 IPC 不可达」**：tasks 标注为 2.15，属 **WP4b**（CLI `daemon status/stop` 的进程内回答路径）。本切片 CLI 只提供 `daemon start`，未实现这两条。
2. **R4 的 Windows 本机证据**：Windows 上 endpoint 由 OS 命名空间分配（pipe 名含用户 SID 摘要 + instanceId），无法用配置构造「路径被占用」；本机可判定的是「显式 endpoint 配置被拒绝且无副作用」（跨平台用例）与配置/权限类失败关闭。符号链接抢占路径的用例是 `#[cfg(unix)]`，只在 Linux CI 生效。R4 的 Windows 端语义仍以 `reports/pv5-windows-ipc.log` 为准。
3. **R13 的「CLI 等待到连接关闭与锁释放后退出」**：Daemon 侧已满足（关闭序列完成才退出、锁在最后释放）；CLI 侧的等待与退出码属 WP4b。
4. **平台 keystore 路径未在 CI 覆盖**：集成测试统一在 `dev_mode.enabled = true` + `identity.keystore = "ephemeral"` 下运行（Linux runner 无平台 keystore，而本切片要求 fail-closed）。`FileKeystore`（Windows DPAPI）路径只有单元级/本机验证。
5. **`storage.flush_interval_ms` 的周期任务是空转**（§6 已说明）：本包不得改 `storage-sqlite`，`SqliteStore` 也没有 checkpoint 接口。
6. **`RevocationCloser` 本切片无可关闭的连接表**：`device.revoke`/`node.revoke` 的「关闭该设备/节点的活动连接」在此表现为结构化日志 `closed_connections = 0`（`EntityRef` 无 connection 变体，且本切片只有单条本地通道）。
7. **`AuditSink` 的记录形状是选择而非规范直读**：`actor = LocalCli`、`target = EntityRef::Node(本机 nodeId)`、`outcome = Denied`、`action = authorization.denied`、无 `detail_digest`（`owned_audit` 的列与 CHECK 固定，连接身份不进 `target`）。此外 `AuditHook` 是同步的而 `append` 是异步的，因此经有界通道（容量 64）+ 由组合根持有的写任务落库；通道满/关闭时丢弃并记 `authorization.denied_audit_dropped`。
8. **`logging` 自研最小订阅器**：本变更未登记 `tracing-subscriber` 依赖，故组合根用 `tracing` 自带类型实现 level/format/file；`logging.file` 为 `null` 时写 stderr（不双写）。
9. **`daemon.instance_lock = "ipc"` 被当作「已知但未接线」**：本切片锁语义等价（同一文件、同一 OS 级互斥），已在 `config.unwired` 中登记并 debug 日志。
10. **`docs/MODULE_ARCHITECTURE.md` §5 表格下方的注记仍写着「`storage-sqlite` 仍只作为『行』出现」**：本次只允许改该矩阵列，故该句留待 **WP5（2.19）** 与「已落地」收口一并同步。**这是本报告主动登记的文档漂移，不是遗漏。**
11. **`LocalEndpoint::accept()` 不可取消（跨 crate 约束，本轮已在 `app` 规避）**：Windows 实现会先 `take_pending()` 把待连接的 pipe 实例**移出** `self`，因此让 `select!` 取消 `accept()` 会把该实例连同已经连上它的客户端一起丢掉——客户端 `open()` 成功却在服务端看到 EOF（本轮实测复现：`调用: Closed`，服务端无任何日志）。`app` 现在由一个自有任务执行 accept，结果经容量 1 通道交给循环，循环只 `recv`。建议在 `server` 侧文档注记或提供幂等接缝（属 WP3 范围，本包不得改）。
12. **`expect` 使用点**：仅限可判定不会失败处（`Uuid::new_v4()` 文本、`EPOCH_TEXT` 常量回退、`serde_json` 固定形状序列化）。测试与工装内的 `expect` 属测试代码。
13. **`nodeId` 派生**：`SHA-256("acp-remote/node-id/v1" ‖ 65 字节 SEC1 公钥)` 前 16 字节按 UUID v8 变体格式化。仓库中不存在本机 `NodeId` 的权威持久位置（`meta` 只有 `server_epoch/created_at/last_prune_at`，配置无该键，keystore 只存密钥），这样保证 `nodeId`/`nodePublicKey` 同源。代价：**密钥轮换会改变 `nodeId`**（本切片轮换回 `local.unsupported`）。

**待上游裁定（本报告不自行决定）**

1. 上述 §9.13 的 `nodeId` 派生方式与「轮换即换 id」的代价是否接受？若要求解耦，需要 core/storage 增加权威存储位置（超出本包范围）。
2. 锁记录从被锁文件**分家**为 `daemon.instance.json`（`design.md` 写的是「锁文件内容是 instanceId 与 pid」）：Windows 强制字节范围锁使「同文件读写」不可行，请确认该偏离并登记到权威文档（WP5）。
3. `storage.flush_interval_ms` 周期任务为空转（依赖 storage 暴露 checkpoint 接口）是否接受？
4. `RevocationCloser` 的 `closed_connections = 0` 与 `AuditSink` 的审计行形状（§9.7）是否接受？
5. 是否要求 `server` 侧为 `accept()` 的取消安全补文档注记或幂等接缝（§9.11）？
6. 集成测试统一走 dev-mode + ephemeral keystore（§9.4）：是否需要在 W5 增加一次本机平台 keystore 的手动/半自动验证记录？

## 10. result

- **交付完成**：`crates/app` 落地并可运行 `acp-remote daemon start`（前台、单实例锁、endpoint、周期任务、关闭序列、本地管理通道客户端与 `daemon.status`/`daemon.stop` 服务端接线）。
- **判定**：本 WP 的检查（PV4）**PASS**；PV1/PV2 的本次复核 **PASS**；R10/R11 与部分 CLI 侧行为按计划留给 WP4b（非缺陷）。
- **无未通过项**：本报告未弱化任何断言、未使用 `#[ignore]`、未跳过任何可执行用例；上面列出的全部是「按任务分工不在本包范围内」或「需要上游裁定的实现选择」。
- **提交**：`e9b8e1d`（实现）、`c628e87`（60 s 完整周期用例）、本报告的 `docs(app)` 提交。**未 push、未开 PR、未合并**（无授权）。

## 11. evidence_paths

- `openspec/changes/daemon-cli-and-local-admin/reports/wp4-app-daemon.log`：fmt / clippy / test（`-p app`）三组命令的完整输出与显式 `EXIT(...)` 行 + 自检段（`rg unsafe` / `rg rusqlite|sqlx` / `rg SqliteStore`）。
- `openspec/changes/daemon-cli-and-local-admin/reports/du1-pv1.log`：`check-crate-boundaries`（EXIT 0）、`npm run check`×3（EXIT 0）、`cargo test --locked --workspace --all-features`（EXIT 0，682 passed）。
- `openspec/changes/daemon-cli-and-local-admin/reports/wp4a-handoff.md`：本报告。
- 两份 `.log` 按 `.gitignore` 的 `openspec/changes/**/reports/**/*.log` 约定**不入库**（与 WP1–WP3 的日志一致）：它们是本机证据文件，复核时需在本机重新生成（命令与参数见 §8）。本报告是本 WP 唯一的入库证据索引。

## 12. resource_cleanup

- 集成用例的临时目录由工装 `Drop` 删除（Windows 上带重试；每个用例一个独立 `dataDir` + 独立 `XDG_RUNTIME_DIR`）；用例失败路径也会 `kill` 子进程并 `wait`（无遗留 Daemon 进程）。
- 本轮结束后：`git status --porcelain` 仅剩本报告（写完后即提交）；worktree 中无临时/调试文件（早前用于调试的 `.wp4a-*.tmp` 与临时 `ACPR_WP4A_KEEP_TEMP` 钩子均已删除）。
- 构建目录仍为仓库共享 `target/`（未设 `CARGO_TARGET_DIR`）；未清理其他执行者的产物。
- 本地 `gitleaks` 未安装，密钥扫描在本地被跳过（`.husky/pre-commit` 已如实提示，不视为通过），由 CI 的 `secrets` job 判定。
