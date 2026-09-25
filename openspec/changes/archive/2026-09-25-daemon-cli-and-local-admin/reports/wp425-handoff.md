# WP4 2.25 交接：WP4a/WP4b 收口（三处真实缺口）

```yaml
task_id: 2.25（WP4a 收口；tasks.md 2.25 的 A/B/C 三项）
role: 实现 Agent（coder，受限 worker；只写 crates/app/**、crates/server/src/transport/local/platform/windows.rs、本报告与日志）
phase: apply（RV 前修正）
agent_context: >
  上一轮 WP4a 已交付 crates/app（组合根 + Daemon 生命周期，报告 reports/wp4a-handoff.md）；verification.md
  的裁定③把 `storage.flush_interval_ms` 的 no-op 实现判为不接受并转本任务（2.25）。本轮按任务单实现
  A（合并窗口定时器接线）、B（accept 非 cancel-safe 的源码约束 + 断言）、C（交互式拒绝终结配对会话），
  未改动 core/storage-sqlite/server 的其它文件、权威规划文件（plan/tasks/verification/proposal/design/specs）
  与任何 docs（windows.rs 只是源码文档注释）。
target_revision:
  branch: feat/daemon-cli-and-local-admin
  base: 2791f8b（本轮起点）
  implementation_commit: 7089426   # fix(app): 接线 broker 的合并窗口定时器并终结交互式拒绝的配对会话
  docs_test_commit: a4cb119        # test(server): 声明 accept 非 cancel-safe 并断言取消会丢弃待连接实例
  head_at_handoff: a4cb119（本报告为紧随其后的第三个提交）
scope:
  - crates/app/src/config.rs（`storage.flush_interval_ms` 从「未接线」转为已接线 + 默认值/校验 + 用例）
  - crates/app/src/compose.rs（保留 broker 句柄 + `close()` 释放它）
  - crates/app/src/daemon.rs（合并窗口任务替换 no-op 的 storage_flush + 两个断言级用例）
  - crates/app/src/cli/pairing.rs（交互式拒绝调 `*.pair.reject` + 四个用例）
  - crates/app/tests/daemon_lifecycle.rs（1 行：任务名 `storage_flush` → `merge_window`，断言强度不变）
  - crates/server/src/transport/local/platform/windows.rs（`accept()` 的 cancel-safety 文档 + 1 个用例）
  - openspec/changes/daemon-cli-and-local-admin/reports/{wp425.log, du1-pv1.log, wp425-handoff.md}
```

## 1. A（主项）：`storage.flush_interval_ms` 真正接线为 broker 的合并窗口

### 1.1 契约依据（未重新论证，只落实现）

- `docs/CORE_PORTS_AND_STORAGE.md` §6 第 10 条：`storage.flush_interval_ms = 250` 允许把同一会话短窗口内的
  delta 合并为一次 `commit`；
- `crates/core/src/broker.rs` 模块头：core 不读时钟、不设定时器，缓冲事件**必须**由持有 runtime 的组合根
  调用 `Broker::pump` 驱动（「`storage.flush_interval_ms` 的合并窗口由组合根的定时器触发 `pump`」）；
- `crates/storage-sqlite/src/migrate.rs` 的 `StorageConfig` 注记明写该键**不在**存储配置里（不是刷盘开关）。

### 1.2 实现（最小改动路径）

| 位置 | 改动 |
|---|---|
| `crates/app/src/config.rs` | `Config` 新增 `flush_interval_ms`（默认 `DEFAULT_FLUSH_INTERVAL_MS = 250`，`CONFIG_REFERENCE.md` §5）；`storage.flush_interval_ms` 从 `unwired` 列表移除；`0` 失败关闭（`local.invalid_params`：0 会让每个调度点都枚举会话，是热循环而不是合并窗口） |
| `crates/app/src/compose.rs` | `Composition` 保留 `Arc<Broker>`（同一实例也交给 `UseCases`：`Arc::clone`），新增 `Composition::broker()`；`close()` 的析构里显式 `drop(broker)`——`let Self { .. }` 的 `..` 字段在函数作用域末尾才析构，不显式释放会把检查点变成 `StoreStillShared`（本轮实现时先踩到，已修） |
| `crates/app/src/daemon.rs` | 删除 no-op 的 `spawn_storage_flush`；新增 `MergeWindow` 驱动面 + `BrokerMergeWindow`（`UseCases::list_sessions` 枚举 + `Broker::pump` 驱动）+ `spawn_merge_window`（周期任务，任务名 `merge_window`）；`run` 用 `Duration::from_millis(config.flush_interval_ms)` 装配 |

**枚举形状与取舍**：`MERGE_WINDOW_STATES = [queued, running, waiting_input, waiting_permission]`，
`SessionQuery { only: None, states, limit: None }`。

- 为什么不取全表：`SessionQuery` 没有 offset/cursor（`core::ports` §4 的查询形状），**无法分页续取**；
  状态过滤在 SQL 侧（`SELECT … FROM owned_session WHERE state IN (?,?,?,?)`，实测语句见 §5 的探针日志），
  返回集被限制在「正在跑或排队」的少数会话。
- 为什么「只取活动会话」是安全的：会话状态转换与触发它的事件批次**在同一事务**提交（§6 第 1/2 条），
  因此「持久状态是 `idle`/`failed`/`closed`」蕴含「该会话没有待提交缓冲」；反过来，有内存缓冲的会话
  （deltas 在跑、turn 在排队）持久状态必然非终态。`pump` 内的 flush 先 `take` 走全部缓冲再分批提交，
  期间不会有新 turn 被派发（pump 持有该会话的门闸）。

### 1.3 错误与取消

- 不吞错：单会话 `pump` 失败记 `daemon.merge_window_failed`（`session_id` + `error = port_error_token(...)`
  的**错误类别**），本轮其余会话继续；枚举失败记 `daemon.merge_window_scan_failed`；两者都不结束任务
  （缓冲还在内存里，下个周期重试）。
- 日志口径：空转轮 `debug`（`daemon.merge_window`，字段 `ticks`/`interval_ms`），有活动会话才 `info`
  （再加 `sessions`/`pumped`）——250ms 一级的稳定噪声不进 info。
- 所有权与取消：与既有周期任务同一套 `OwnedTasks { name, Arc<Notify>, JoinHandle }` + `cancel_all`，
  取消是协作式的（`select!` 的 `notified()` 分支 + 循环内的 `shutdown.is_requested()` 兜底）。

### 1.4 证据（断言级 + 端到端）

1. **断言级（spy）**：`daemon::tests::the_merge_window_pumps_every_active_session_each_interval_until_cancelled`
   —— 注入 `MergeWindowSpy`（2 个会话，20ms 间隔）：断言 `pumps == scans × 2`（每个 tick 对每个会话
   各 `pump` 一次）、tick 数与 20ms 间隔相符（`scans <= elapsed_ms/20 + 2`，捕获「间隔被忽略的热循环」）、
   `cancel_all` 之后 `(scans, pumps)` 冻结（取消后不再调用）。
2. **断言级（失败路径）**：`daemon::tests::a_failing_session_does_not_stop_the_rest_of_the_round_or_the_task`
   —— 1 个会话注入 `Unavailable(StorageFull)`：断言每轮恰好 1 次失败、另一会话仍被 `pump`、任务继续跑到
   至少 2 轮，`shutdown.request` 后任务自行退出。
3. **配置接线**：`config::tests::defaults_match_the_reference_table`（默认 250）、
   `config::tests::consumed_keys_are_applied`（显式 `flush_interval_ms = 500` 生效）、
   `invalid_values_fail_closed`（`0` 被拒）、`unwired_sections_are_accepted_and_reported`（不再登记为未接线）。
4. **端到端（真实二进制 + debug 日志）**：`reports/wp425.log` 末尾的探针段——同一台开发机上用
   `flush_interval_ms = 100` 与缺省（250）各起一次真实 Daemon，日志里 `daemon.merge_window` 的
   `interval_ms` 分别为 100/250，tick 时间戳间隔与之一致（100ms 配置：`ticks` 1→2→3 的 `ts` 为
   `…59.547Z`→`…59.657Z`→`…59.767Z`；缺省 250ms 配置：`…45.350Z`→`…45.601Z`→`…45.854Z`），
   每轮 2 次 tick 之间都能看到枚举用的 `SELECT … FROM owned_session WHERE state IN (?, ?, ?, ?)`；
   关闭时 `daemon.task_stopped{task="merge_window"}` 早于 `daemon.storage_closed`。
5. **本切片是否有端到端可观察效果**：本切片的本地通道方法集不含 `session.create`（`LOCAL_ADMIN_PROTOCOL.md`
   §5.8），因此**没有任何 owned 会话可供 `pump`**：真实 Daemon 里 `list_sessions` 恒回空集，任务每轮只记
   一条 debug（`本轮无活动会话`），没有可观察的落盘/广播差异。它的端到端效果属于接入 `session.create`/流式
   delta 写路径之后（即 WP6 及以后）。因此本轮的可验证证据**必须**是上面 1–3 的断言 + 4 的 tick 计数与
   间隔，而不是「某条 delta 被合并落盘」。

## 2. B：`accept()` 的非 cancel-safe 约束钉进源码

- 位置：`crates/server/src/transport/local/platform/windows.rs:66` 起，`LocalEndpoint::accept` 的文档注释新增
  `# 本 future **不是 cancel-safe**` 段：说明 `take_pending()` 会把待连接 pipe 实例**移出** `self`，
  一旦 future 在 `connect()` 完成前被丢掉（`select!` 另一分支胜出 / `timeout` 到期 / 任务 abort），
  该实例会连同**已经连上它但尚未被处理**的客户端一起被关闭（客户端 `open()` 已成功却立刻看到 EOF），
  并写明调用方 MUST 在**专用任务**里串行调用、结果投递给其余逻辑（指向 `crates/app/src/daemon.rs`
  的「自有 accept 任务 + 容量 1 channel + 循环只 `recv`」），以及「取消安全需要幂等接缝，本模块暂不提供」。
- **断言用例（已加）**：`platform::windows::tests::a_cancelled_accept_drops_the_pending_instance_and_kills_the_connected_client`
  —— 手动把 `accept` future 轮询一次（无客户端时挂起，实例已被移出）→ 客户端 `open()` 连上这个已移出的实例
  → `drop(future)`（等价于 `select!`/`timeout` 取消）→ 断言 `endpoint.pending.is_none()`（实例没有被放回）
  → 客户端读到的不是服务端回复而是 `Ok(0)`/错误 → 再断言 endpoint 仍能接受下一条连接。
  既有断言（`a_failed_connect_leaves_the_endpoint_usable`）未被改写。
- 探针（一次性，已删除，不入库）：先用 `crates/server/tests/zz_probe_cancel.rs` 确认「被取消后客户端确实
  收到 `Ok(0)`，写返回 `BrokenPipe(232)`」，据此确定用例里的 200ms 等待（让 reactor 处理连接的完成包）
  是必要的、不是掩盖竞态。

## 3. C：交互式拒绝时终结配对会话

- 调用点：`crates/app/src/cli/pairing.rs` 的 `run` 把「认领后的落定」抽成
  `settle_claim(calls, target, pairingId, claim, answer)`：
  - `Answer::Confirmed` → `*.pair.confirm`（提交展示过的请求集合，行为不变）；
  - `Answer::Rejected` → **先调 `*.pair.reject`（`{ pairingId, reason: null }`）**，成功后再以
    `local.conflict` 失败退出（消息含被调用的方法名）；`reject` 自身失败时把方法返回的码**原样**带出
    （不吞错、不改判成功、不转而调 `confirm`）。
- 交互回答：`ask_confirmation` 返回 `Answer`；`answer_for` 只有 `y`/`Y`（去空白）算确认，其余（含空行、
  EOF）都算拒绝。**读 stdin 失败**（不是 EOF）不再当作拒绝：意图未知时不确认也**不**替用户拒绝，以
  `local.internal` 退出（旧实现把它归入「未确认」；这是本项唯一的行为细化，已在代码注释写明）。
- 非交互路径**保持现状**：`--sas`/`--fingerprint` 不匹配仍在 `verify_supplied` 直接失败（不 confirm、
  不 reject、不改状态、非零退出），`run` 里的分支顺序保证它在 `settle_claim` 之前。
- 可注入接缝：`PairCalls`（生产实现 `OneshotCalls`，每条方法一条一次性连接，与原有 `confirm` 的
  `call_once` 行为一致）。**断言（常驻）**：
  - `an_interactive_rejection_calls_reject_and_never_confirm`：两个方向各断言调用序列**恰好**是
    `[device.pair.reject]` / `[node.pair.reject]`（即「调了 reject 且没调 confirm」）、`params` 为
    `{"pairingId":"P","reason":null}`、退出码非零（`fail()` 的真实出口）且 stderr 行 JSON 的 `code`
    为 `local.conflict`；
  - `a_failed_reject_exits_with_the_reported_code`：注入方法侧失败 `local.invalid_params` → 断言退出码非零、
    stderr 行 `code == "local.invalid_params"`（原样带出方法的码）、仍不调 `confirm`；
  - `a_confirmation_calls_only_confirm`：确认路径**只**调 `confirm`（不调 reject）；
  - `only_an_explicit_yes_is_a_confirmation`：回答词表。
- 方法可用性核对：`device.pair.reject`/`node.pair.reject` 在 25 项词表内（`Method::as_str`），局部能力
  `local.device.manage`（`LOCAL_ADMIN_PROTOCOL.md` §5.1）；`identity-auth` 的 `settle(Reject)` 允许
  `Created` 与 `PendingConfirmation`（=`claimed`）两种状态，因此拒绝一个已认领的配对是合法的状态转换。

## 4. checks（逐检查 ID 的 handoff_index）

| index | 检查 ID | 命令 / 判据 | 结果 | 证据 |
|---|---|---|---|---|
| 1 | **2.25 完成条件①** | `cargo fmt --all -- --check` | **PASS**（EXIT 0） | `reports/wp425.log`（`EXIT(cargo fmt …)=0`） |
| 2 | **2.25 完成条件②** | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | **PASS**（EXIT 0） | `reports/wp425.log`（`EXIT(cargo clippy …)=0`） |
| 3 | **2.25 完成条件③** | `cargo test --locked -p server --all-features`：89 lib passed / 0 failed / **0 ignored**（含 1 个新增用例） | **PASS**（EXIT 0） | `reports/wp425.log`（`EXIT(cargo test --locked -p server --all-features)=0`） |
| 4 | **2.25 完成条件④ / PV4** | `cargo test --locked -p app --all-features`：49 lib（含 2 个新增合并窗口用例与 4 个配对新用例）+ 1 audit_export + 11 cli_commands + 9 daemon_lifecycle，全部 0 failed / **0 ignored** | **PASS**（EXIT 0） | `reports/wp425.log`（`EXIT(cargo test --locked -p app --all-features)=0`） |
| 5 | **PV1（合同门禁）** | `npm run check`（doc-links / 合同漂移 / 封闭词表 / 边界 / schema / fixture / agentic 全量串行） | **PASS**（EXIT 0） | `reports/du1-pv1.log`（`EXIT(npm run check)=0`） |
| 6 | **A 的端到端证据** | 真实二进制 + `[logging] level="debug"`：`flush_interval_ms` 100 与缺省 250 两次运行，日志 `daemon.merge_window` 的 `interval_ms` 与 tick 间隔一致，且每次枚举都执行 `SELECT … FROM owned_session WHERE state IN (?,?,?,?)`；关闭时 `task_stopped{merge_window}` 早于 `storage_closed` | **PASS** | `reports/wp425.log` 末尾探针段（本机证据，日志文件按 `.gitignore` 不入库） |
| 7 | **B 的机制证据** | 一次性探针 `crates/server/tests/zz_probe_cancel.rs`（已删除）：取消后客户端 `read -> Ok(0)`、`write -> BrokenPipe(232)`，随后 `accept` 仍可用 | **PASS** | `reports/wp425.log` 末尾探针段 |
| 8 | **PV5（Windows IPC 语义）** | 本 WP 未新增 IPC 语义面；`accept` 的取消语义属本机可判定（见 index 7），CI 的 Linux runner 不编译该 `cfg(windows)` 用例 | **NOT RUN（不属于本 WP）** | `reports/pv5-windows-ipc.log`（既有） |
| 9 | **PV3（core/storage）** | `core`/`storage-sqlite` 未改动（本轮只改 `crates/app` 与 `server` 的一处文档注释 + 一个用例）；workspace 级 clippy/fmt 覆盖编译面 | **NOT RUN（未改动，按范围跳过）** | 本轮 `git diff --stat`（本报告 §5） |

**跳过数**：`-p app` 与 `-p server` 两套运行的 `ignored` 均为 **0**；唯一不在本机执行的是
`#[cfg(windows)]`/`#[cfg(unix)]` 的条件编译用例（`an_unusable_endpoint_refuses_start_without_degrading`
只在 Linux CI 编译；本轮新增的 Windows 用例只在 Windows 编译），它们既不计 passed 也不计 ignored。

## 5. 改动清单（`git diff --stat`，base 2791f8b → a4cb119）

```text
 crates/app/src/cli/pairing.rs                       | 269 ++++++++++++++++--
 crates/app/src/compose.rs                            |  50 ++--
 crates/app/src/config.rs                             |  29 +-
 crates/app/src/daemon.rs                             | 314 +++++++++++++++++++--
 crates/app/tests/daemon_lifecycle.rs                 |   2 +-
 crates/server/src/transport/local/platform/windows.rs|  74 +++++
 6 files changed, 664 insertions(+), 74 deletions(-)
```

未改：`crates/core/**`、`crates/storage-sqlite/**`（除 compose 侧的析构顺序外与存储无关）、`server` 的其它文件、
`schemas/**`、`fixtures/**`、`compatibility/**`、`docs/**`（B 项只在源码文档注释里）、权威规划文件。

## 6. issues（未执行项、偏离说明与待澄清）

1. **关闭序列的「先停周期任务」与两份权威文档的措辞冲突**：`CORE_PORTS_AND_STORAGE.md` §7.1 第 4 条写
   「关闭顺序中先停周期任务，再停接入层与 Agent，最后 `wal_checkpoint(TRUNCATE)`」，而
   `SECURITY_DESIGN.md` §12.1 写「关闭时按顺序停止接入、取消任务、关闭 Agent、刷新存储」——现有实现
   （WP4a，已被 R13 用例断言）是 §12.1 的顺序：**停接入层 → 停周期任务 → 停 Agent → checkpoint**。
   本轮**没有**改变该顺序（改它要动 `accept_loop`/`close` 的移交结构并改写既有断言），合并窗口任务与
   `maintenance`/`signal_watcher` 同一批、同一处被取消，因此它在**停 Agent 与刷盘之前**被停止（§7.1 第 4 条
   的实质要求）。若 reviewer 要求严格按 §7.1 的字面顺序（周期任务先于接入层），那是一次独立的关闭结构改动，
   需要主 Agent 裁定后单独下发。
2. **本机 `daemon stop` 的关闭序列会等满 `grace_ms`（10s）后才继续**（与本次改动无关）：探针里两次运行都出现
   `daemon.drain_timeout{remaining:1}` → `daemon.task_stop_timeout{task:"maintenance"}` → `daemon.stopped`
   （`elapsed_ms ≈ 10000`）。涉及的是**未改动**的 `drain_connections`/`serve_connection`/CLI `daemon_stop`
   路径（`daemon_stop` 在等锁之前已显式 `drop(client)`）。`daemon.task_stop_timeout` 的直接成因是
   `close()` 把任务取消期限写成 `deadline.min(now + TASK_STOP_TIMEOUT)`，而 drain 已把 `deadline` 用尽。
   集成用例（R12–R14/R15–R16）在本轮全绿，因此这是**场景相关**的既有行为，登记为待查项，本轮未修
   （超出 2.25 的授权范围）。
3. **`storage.flush_interval_ms` 只校验下界**（`>= 1`）：`CONFIG_REFERENCE.md` §5 没有登记上界，因此不擅自
   发明（`daemon.shutdown_grace_ms` 的 0..=60000 上界是文档登记过的）。
4. **A 项的端到端可观察性受切片限制**（见 §1.4 第 5 点）：本切片没有 owned 会话，任务每轮只记 debug
   （`本轮无活动会话`）。为避免「看不出它在跑」，探针用 `[logging] level="debug"` 取得真实 tick 计数与间隔；
   生产默认 `info` 级下只有在**有活动会话**或失败时才有行。
5. **`MergeWindow` 的枚举口径依赖「状态与事件同事务提交」这一不变量**：若将来某个写路径先写内存缓冲、
   后单独提交状态（例如新的 imported/owned 混合路径），本口径需要随之复核。已在代码注释里写明依据。
6. **`ask_confirmation` 的读失败语义细化**（stdin 读失败 → `local.internal`、不 reject）：旧实现把它归入
   「未确认」（`local.conflict`）。没有既有用例覆盖该分支；若 reviewer 认为应保持旧码，改动只有两行。
7. **无 `#[ignore]`、无弱化断言**：`daemon_lifecycle.rs` 的唯一改动是把任务名断言从 `storage_flush` 改为
   `merge_window`（内容与强度不变：仍断言该任务在关闭序列被取消且早于 `daemon.stopped`）。
8. **本地 `gitleaks` 未安装**（`.husky/pre-commit` 已如实提示）：密钥扫描在本地被跳过，不视为通过，由 CI 的
   `secrets` job 判定。
9. **B 项在非 Windows 上的等价物未改动**：`platform/unix.rs` 的 `accept` 逐字未改（任务单把写范围限定在
   `windows.rs`）；Unix socket 的 `accept` 是否会吞掉已连接的客户端，属 WP2/WP3 的语义面，本轮未评估。

## 7. result

- **A/B/C 三项均已实现**，全部以断言级证据 + 端到端探针覆盖；`fmt`/`clippy`/`-p app`/`-p server`/`npm run check`
  全绿（留档显式退出码行）。
- **判定**：本任务（2.25）的检查 **PASS**；未通过项 0；未执行项为 §6.1（顺序措辞冲突，已按既有实现保持并说明）、
  §6.2（既有 `daemon stop` 排空超时，超出授权范围）、以及范围外的 PV3/PV5。
- **提交**：`7089426`（A+C）、`a4cb119`（B），本报告为第三个提交。**未 push、未开 PR、未合并**（无授权）。

## 8. evidence_paths

- `openspec/changes/daemon-cli-and-local-admin/reports/wp425.log`：fmt / clippy / `-p server` / `-p app` 四组命令
  的完整输出与显式 `EXIT(...)` 行，以及两组真实二进制探针（合并窗口 tick 与 cancel-safe 取消后果）的记录。
- `openspec/changes/daemon-cli-and-local-admin/reports/du1-pv1.log`：`npm run check` 全量输出与 `EXIT(...)` 行。
- `openspec/changes/daemon-cli-and-local-admin/reports/wp425-handoff.md`：本报告。
- 两份 `.log` 按 `.gitignore`（`openspec/changes/**/reports/**/*.log`）**不入库**，是本机证据文件：复核时按 §4 的
  命令重新生成即可（参数与工作目录一致）。

## 9. resource_cleanup

- 一次性探针文件 `crates/server/tests/zz_probe_cancel.rs` 已删除（`git status` 无残留）；探针用的两个临时目录
  （`/tmp/tmp.*`，含 data/config/log）在系统临时目录下，本进程退出后无后台 Daemon（两次探针都执行了
  `daemon stop` 且进程已 `wait` 回收）。
- 本轮 `git status --porcelain` 只会有本报告（写完后立即提交）；未创建 worktree、未改动其它执行者的产物。
- 构建目录仍是仓库共享 `target/`（未设 `CARGO_TARGET_DIR`）。
