<!-- 执行安排：把 specs 的行为要求与 design 的技术方案转成可派发的并行工作包。
     plan 只写“谁在哪个隔离工作区做什么、按什么顺序合入、怎么验”，不重复 specs/design 的内容。 -->

## Execution Model

- 派发：主 Agent 用 `task` 工具的 batch 形状一次性扇出多个工作包；每个工作包是**一次 spawn**。
- 隔离：`task.isolation`（`patch` 模式、`apply: false`）。工作包之间不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；工作包不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply`，每次合入后跑主检出 Project Verify。
- 验证：隔离工作区内**没有** `target/` 与 `node_modules/`，因此仓库级检查只在主检出执行。
- 检视：每个工作包由一个**独立** spawn（不与作者同实例）读 patch 与 `agent://<id>` 后给出结构化结论。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 落账时序改为三笔（受理只落幂等行 / 执行 / 落账：会话行 + 回填 + 终态），失败零会话行；窗口期事件在 broker 内有界排队、落账后按序写库再广播，超限失败关闭（`sync-session-create`「终态与幂等」、`sync-command-handling`「mutation 先接受后终态」、`local-agent-host`「会话创建时暴露 ACP 会话标识」） | none | wp-coder | `crates/core/src/`、`crates/storage-sqlite/src/`、`crates/agent-host/src/`、`crates/app/src/compose.rs` 与其它 `IdGenerator` 实现点（`crates/agent-host/tests/support/mod.rs`、`crates/storage-sqlite/tests/derived_events_behaviour.rs`、`crates/server/src/local_admin/test_support.rs` 只补新方法） | patch：三笔提交、事件排队与超限口径、崩溃窗口的新形状、失败分类可观察、`IdGenerator::session_id` | `cargo test -p core -p storage-sqlite -p agent-host`（主检出） |
| WP2 | 两个面的终态判据统一为「可证明 ⇒ `failed` / 说不清 ⇒ `uncertain`」，Node Link 换掉"有记录就 uncertain"的替身；命令级结构化日志（`sync-command-handling` 新增要求、`node-link-owner-server` 两条、`local-agent-host` 失败分类） | code:WP1 | wp-coder | `crates/server/src/` | patch：判据、日志字段、`failed` 的错误码复用 `internal.unavailable` | `cargo test -p server` |
| WP3 | 权威文档写回：`CORE_PORTS` §3.1/§5/§6 第 16/20/22 条（含"恢复走有会话分支"的改写与 `IdGenerator::session_id` 的 id 来源改写）、`SYNC_PROTOCOL` §11.2/§11.5、`NODE_LINK_PROTOCOL` 相应小节、`SECURITY_DESIGN` §14 日志字段 | code:WP1（§5 的代码块与 `ports.rs` 被漂移门禁逐条比对，必须在 WP1 定版之后写） | wp-coder | `docs/` | patch：四处文档与实现口径一致 | `npm run check`（`check:docs` 判据） |
| WP4 | 协议级端到端用例：失败零行、`accepted` → 终态 `failed`、同 `requestId` 重发返回首次终态、崩溃窗口恢复结 `uncertain`、窗口期事件不丢不重与顺序、超限走失败关闭 | code:WP1, code:WP2 | wp-tester | `crates/app/tests/` | patch：可执行用例与断言 | `cargo test -p app` |

> **写范围勘误（apply 阶段核实）**：WP1 原写范围只列 `crates/core/src/`，但目标时序在实现上必须同时改三处，已并入该行：
> - `storage-sqlite`：受理提交不得建会话行；落账提交要在**同一次事务**里建会话行 + 回填幂等行 + 写终态；无会话终态（失败与崩溃窗口）要有可定位的写入路径；
> - `agent-host`：失败分类必须让调用方区分「可证明 Agent 侧未产生会话」与「说不清」——今天 `SpawnFailed`/`AgentExited`/`NotRunning` 共用 `Unavailable(IoError)`，不足以支撑判据；
> - `IdGenerator`：`SessionBackendFactory::create` 必须先拿到 `SessionId`，而受理阶段又不能写会话行（`owned_command.session_id` 有外键），因此 session 标识改为 **core 分配**（新增 `IdGenerator::session_id()`），全部实现点（`app` 组合根与三处测试替身）同批补上。
>
> `docs/CORE_PORTS_AND_STORAGE.md` 归 WP3 独自负责（§5 与 `ports.rs` 被漂移门禁逐条比对，WP1 不碰 `docs/`）。

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 无重叠（core 时序是其余工作的前置） | `cargo test -p core -p storage-sqlite -p agent-host`（另跑 `cargo check -p server -p app`） |
| 2 | WP3 | 写范围 `docs/` 与其它包不重叠，可早合入以固定口径 | `npm run check` |
| 3 | WP2 | 依赖 WP1 的新形状 | `cargo test -p server` |
| 4 | WP4 | 依赖 WP1 + WP2 的行为 | `cargo test -p app` |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cargo test -p core -p server -p app`（主检出根） | 全部通过 |
| PV2 | `npm run check`（主检出根） | 十项合同门禁全绿（本变更不新增错误码、不改 wire 形状） |
| PV3 | `cargo clippy --workspace --all-targets -- -D warnings`（主检出根） | 无 warning |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 三笔提交的原子性边界（哪些写入必须同事务）、失败零行的每条路径、事件排队是否仍满足"先落库再发布"、超限口径是否复用既有实现 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer | 判据是否两面一致、是否真的不再用"记录存在"当替身、日志是否可能带出路径或凭据 | 无 CRITICAL/MAJOR 未闭环项 |
| WP3 | reviewer | 文档是否与实现逐条对齐（尤其 §6 第 22 条的"有会话分支"是否已改写） | 无 CRITICAL/MAJOR 未闭环项 |
| WP4 | reviewer | 用例是否可证伪（断言零会话行与终态类别，而不是断言代码路径）、是否覆盖幂等重发与恢复 | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: required
reason: 本变更的价值是"失败路径的记录可信且可诊断"，最有力的证据是在真实 Daemon + 真实页面 + 真实 Agent 上观察到 accepted → 终态 failed、库里零会话行、且日志里能看到主体与成因。
basis: docs/WORKFLOW.md §7 按被测面选 prelude：本判据落在 Web/PWA 面，用 browser prelude 驱动真实页面；只启动成功、退出码为零、截图都不算通过。
alternative_checks:
  - WP4 的协议级端到端用例覆盖同样的行为（失败零行、终态类别、幂等重发、恢复），作为回归；它不替代真机判据
```

## Main E2E 真机验收记录（2026-10-16，本机 Windows x64）

环境：`target\debug\acp-remote.exe daemon start --config D:\tmp\acpr-settle\daemon.toml`（dev 模式、`daemon.tls.mode = direct` + 临时自签证书、`public_origin = https://127.0.0.1:18781`、`[logging] level = info` 写 `D:\tmp\acpr-settle\daemon.log`）；数据目录 `D:\tmp\acpr-settle\data`（空库起）；本机登记目录别名 `demo`（`D:\tmp\acpr-settle\ws`）；Agent profile `broken-agent` 的 `command = "acpr-not-a-real-command-xyz"`（**可证明未产生会话**的失败类别）；真实页面用 `browser` prelude 驱动（托管 Chromium，`Security.setIgnoreCertificateErrors` 自签证书放行），配对走本机管理通道（`device.pair.begin` → 页面认领 → `device.pair.confirm`）。

操作：页面进 `/dir/demo` → 「在这个目录里新建会话」→ 选 `Broken Agent` → 「创建会话」，浏览器侧用 CDP 抓真实 WSS 帧。

三重观察证据（同时成立）：

1. **受理 → 终态失败**（同一 `requestId` 的两次 `command.result`，逐字取自 CDP 抓到的 WSS 帧）：
   - 出站：`{"command":"session.create","payload":{"workspaceAlias":"demo","agentId":"broken-agent"}}`；
   - 回包 1：`{"status":"accepted","acceptedAt":"2026-10-16T14:29:09.351Z","terminalEventId":null,"result":null,"error":null}`；
   - 回包 2：`{"status":"failed","acceptedAt":"2026-10-16T14:29:09.351Z","terminalEventId":"1fde0fe6-8103-498d-84b5-1cb4f8a635aa","result":null,"error":{"code":"internal.unavailable","message":"后端未能建立会话","retryable":true,"details":{}}}`。
2. **库里零会话行**（对运行中的库只读查询）：`select count(*) from owned_session` = **0**；`owned_command` 里该命令一行：`session_id = NULL`、`status = failed`、`terminal_event_id` 非空、`error_code = internal.unavailable`、`result_json = NULL`。
3. **日志含主体与成因**（`daemon.log`，`logging.format = text`）：
   - `INFO server::command_log message="the accepted command is admitted; the terminal follows as a persisted event" event=sync.command_accepted device_id=b7b5a74a-a6b5-4246-b9bf-e057183669a3 command=session.create session_id=None request_id=399ed1bd-dbf6-49c1-980d-bbd5fd460176 turn=None`
   - `INFO server::command_log message="the accepted command reached its terminal state" event=sync.command_terminal device_id=b7b5a74a-… command=session.create session_id=None request_id=399ed1bd-… terminal=failed cause=unavailable`

页面侧可观察结果：创建弹层给出「后端未能建立会话」，目录页会话数仍为 0（截图 `D:\tmp\acpr-settle\e2e-create-failed.png`）。

**如实登记的边界**：本记录覆盖的是「可证明未产生会话 ⇒ `failed`」这一支（也是本变更的核心症状）；「说不清 ⇒ `uncertain`」与「同 `requestId` 重发返回首次终态」「崩溃窗口恢复」由 `cargo test -p app` 的协议级用例覆盖（回归），未在真机上重复。真机现场用的 Agent profile 指向不存在的可执行文件，因此**没有**真的拉起 Agent 子进程——这正是不产生会话的证据来源。

## Risks

- **核心时序重构是本批最重的一处**：三笔提交牵动恢复、幂等重放与终态观察者三条既有路径，WP1 的检视必须逐条核对"受理后、落账前"这个新状态。
- **文档与实现的合入顺序**：WP3 早合入可以让口径先固定，但它描述的行为要到 WP1/WP2 才成立；合入后必须重跑 `npm run check`。
- **与 `pairing-model` 的同文件冲突**：两者都改 `SYNC_PROTOCOL.md` 与 `sync-session-create` / `sync-command-handling`（落在不同要求块），合入必须**串行**，后合入者重跑 Project Verify。
