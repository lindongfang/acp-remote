# WP3 coder 报告（`session.resume` 的 core 语义）

```yaml
task_id: "2.3"
work_package: WP3
role: coder
phase: implement
agent_context: "coder-C（子 Agent worker，本轮为**新实例**：Work Package 此前空闲超时被杀的实例零落笔——worktree 干净、无提交、无报告，故本报告是 WP3 的首次交付）。隔离方式：provisioner 分配的独立 worktree `D:/Project/acp-remote-wt/session-resume-wp3`（分支 `agentic/session-resume-wp3`）+ 独立 `CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp3`（冷编译已跑通）+ provisioner 提供的 `node_modules` 目录联接（未执行任何 npm install/ci/update）；未继承其它实例的对话或上下文。"
target_revision: "4f7a23554f76915cf5f29cc23e292ce270e014c9"
scope: "crates/core/ 全部；docs/CORE_PORTS_AND_STORAGE.md 的 §2/§3.1/§3.3/§3.6/§4/§5.1/§5.2；docs/MODULE_ARCHITECTURE.md 的 §4.1（未越界，见 §7）"
result: PASS
resource_cleanup: "worktree、分支、CARGO_TARGET_DIR、node_modules 联接均保留给 provisioner 回收（本角色不自行清理/切换/合并）；未申请独占资源（无需 loopback 监听器，单元测试都在进程内 fake 端口上跑）"
```

- 需求覆盖：R17、R22–R27、R31、R34、R36、R37（core 侧那一半；详见 §5 映射表）
- 证据：`reports/wp3-coder-PV1.log`、`reports/wp3-coder-PV2.log`、`reports/wp3-coder-red-window.log`
- 结论：**PV1（阶段 1）与 PV2 全绿**。本报告不覆盖独立 review、TP2 用例与 E2E，也不代替它们的结论。

---

## 1. 起点、包含关系与输入版本（实测）

在 `D:/Project/acp-remote-wt/session-resume-wp3` 实测（`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp3`）：

```text
$ git rev-parse HEAD
b0a387b17f87a9d15d5b07d20f1e16539566c789          # == U1 集成基线，与派发要求逐字相同

$ git merge-base --is-ancestor 32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992 HEAD ; echo $?
0                                                  # true：code:WP2 的交付提交已在起点内（WP2 就绪证据）

$ git merge-base --is-ancestor 248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb HEAD ; echo $?
0                                                  # true：WP1 的交付提交已在起点内

$ git branch --show-current
agentic/session-resume-wp3                          # provisioner 已就位，未自行创建/切换/清理
$ git status --porcelain                            # 空：起点干净，无用户未提交改动
```

交付提交与包含关系（合入前核对用）：

```text
base   (起点) = b0a387b17f87a9d15d5b07d20f1e16539566c789
target (交付) = 4f7a23554f76915cf5f29cc23e292ce270e014c9
$ git merge-base --is-ancestor b0a387b17f87a9d15d5b07d20f1e16539566c789 4f7a2355...  → 0 (true)
$ git merge-base --is-ancestor 32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992 4f7a2355...  → 0 (true)
$ git merge-base --is-ancestor 248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb 4f7a2355...  → 0 (true)
$ git diff --name-only b0a387b1…..HEAD | wc -l   → 11（全部在 WP3 写范围内，见 §7）
$ git status --porcelain | wc -l                → 0（无未提交改动、无暂存文件）
```

分支上三个提交（都是 Conventional Commits + 中文主题；`npx --quiet --no-install commitlint --from <base> --to HEAD --verbose` 判 0 problems）：

```text
aac0356 feat(core): 落地会话恢复的端口、值对象与两列落盘提交点
3e41f90 feat(core): 落地 session.resume 的恢复用例与终态入口
4f7a235 docs(core): 修正模块架构 §4.1 的端口方法名写法        # ← target（仅文档；PV1/PV2 已在其上重跑）
```

`aac0356` 是在等 DR1 Round 13 复核期间落的检查点（当时的设计裁决是「不受阻的七项先做完、`resume_session` 等契约回写」），`3e41f90` 在其上补完 `resume_session`，`4f7a235` 是报告前自查发现的一处文档写法不一致（`agent-session-id` → `agent_session_id`）。三次提交都在同一 worktree 内、按同一契约顺序推进，未 rebase/reset/强推。

**输入版本**：

- 开工门禁：`design.md@D1, D2, D3` + `plan.md` 的 WP3 行 + `tasks.md` 2.3 + 5 份增量 spec；派发提示记为 `sha256:2c04a445…90de`。
- `resume_session` 落笔前收到 main 的 steer：契约已回写、门禁重过 **DR1 Round 13**（`sha256:1c93193e…05ab`），并给出 `ResumeSessionRequest { agent, agent_session_id, workspace_cwd }` 的冻结形状。
- 本人开工前主动 BLOCKED 的那处契约空档（`ResumeSessionRequest.workspace: ResolvedWorkspace` 的 `WorkspaceAlias` 在恢复路径上无来源）已由 main 裁决为 **方案 (B)**，并写入 `design.md` D3 的 Round 13 注记；本实现按订正后的形状落笔，未自行解释。

---

## 2. 实现内容（逐项对应 tasks 2.3）

### 2.1 值对象（`crates/core/src/model/`）

| 值对象 | 落点 | 形状 / 不变量 |
| --- | --- | --- |
| `AgentSessionId` | `model/ids.rs`（`newtype!` + `check_agent_session_id`） | 非空、≤512 字符、无 NUL；新增 `InvalidValue::AgentSessionId`（具名拒绝，与 `AgentId`/`ExportId` 同一惯例）；**上限 512 与 NUL 禁令是本机约束**——该取值不经过任何 wire，因此不引入 schema，理由写在类型注释里 |
| `ResumeSessionRequest` | `model/backend.rs` | `{ agent: AgentRef, agent_session_id: AgentSessionId, workspace_cwd: String }`；`try_new` 按 `ResolvedWorkspace::canonical_path` 的既有口径校验（`require_bounded(1, 4096)` + 无 NUL + `Path::is_absolute()`），失败 → `InvalidValue::Field`；存在性/目录性/`canonicalize` 一致性**不在构造器里**，由用例第 ③ 步做 |
| `SessionRecoveryRecord` | `model/backend.rs` | `{ agent: AgentRef, agent_session_id: Option<AgentSessionId>, workspace_cwd: Option<String> }`；`Option` 直接对应 `owned_session` 两列，`None` = `NULL` = 没有可用于恢复的数据；注释与文档行都写明**两列不进 `Session`/`SessionSummary`**（守住 §3.6 的 `canonical_path` 不进可投影面边界） |

附带：`model/config.rs` 的既有私有谓词 `no_nul` 提为 `pub(crate)`（`ids.rs`/`backend.rs` 复用同一份判断，没有复制第二份谓词）。

### 2.2 端口（`crates/core/src/ports.rs`，**三个方法都不提供默认实现**）

```rust
SessionBackendFactory::resume(&self, session: &SessionId, request: ResumeSessionRequest, sink: EventSink)
    -> Result<Box<dyn SessionEndpoint>, PortError>
SessionEndpoint::agent_session_id(&self) -> Option<&AgentSessionId>
SessionStore::load_recovery(&self, session: &SessionId) -> Result<Option<SessionRecoveryRecord>, PortError>
```

`load_recovery` 的文档写死窄读取契约：会话行不存在、或任一列为 `NULL` 时返回 `Ok(None)`（`NULL` 不是错误、不得推导补齐）。

`SessionUpdate` 追加两个可空列 `agent_session_id: Option<AgentSessionId>` / `workspace_cwd: Option<String>`（`None` = 不改该列）。

### 2.3 `UnavailableKind::BackendUnsupported`（§2 的 core 内部词表）

- 语义 =「后端不支持该操作」（**不是**临时故障）；`ALL` `[Self; 7]` → `[Self; 8]`、`as_str()` → `"backend_unsupported"`。
- `port_error_public` 的**显式臂**：取既有码 `command.unsupported`、`retryable = false`。
- 本地管理适配器无需改：`crates/server/src/local_admin/params.rs:928` 对任意 `Unavailable` 通配映射到 `local.unavailable`，其既有测试 `for kind in UnavailableKind::ALL` 会自动覆盖新取值 → **不新增任何 `local.*` 码**（与 tasks 2.3 一致）。
- `crates/core/src/model/tests.rs` 的 `UnavailableKind::ALL.len()` 断言 7 → 8，并补 `as_str`/`Display` 断言。

### 2.4 两列落盘提交点（design D2 的契约订正）

`Broker::create_session` 内，`factory.create` 成功返回后**紧接着**提交一次 `StateChange::Update`：

- `workspace_cwd` 在 `create` 消费掉 `CreateSessionRequest` **之前**取出（`create.workspace.as_ref().map(|w| w.canonical_path().to_owned())`），来源是 core **自己**已解析的结果，不依赖后端回报；
- `agent_session_id` 取 `endpoint.agent_session_id()`；为 `None` 时**两列都不写**、也不产生这次追加提交（R7）；
- 该提交 `idempotency = None` / `command_terminal = None`，只带 `session: Some(id)` 的 Update。

可观察后果与 design D2 记录一致：新建会话在创建提交（v1）+ 紧随的 Update（v2）后**可见版本为 2**；单元测试直接断言了 v2（`create_session_writes_the_recovery_columns_right_after_create`）与「未取得标识时 v1”（`create_session_writes_no_recovery_columns_without_an_agent_session_id`）。

### 2.5 `resume_session` 用例（严格按 D3 顺序）

`crates/core/src/use_cases.rs`：`resume_session(actor, RequestId, Digest, SessionId) -> SessionId`（薄入口）→ `crates/core/src/broker.rs` 的实现：

```text
① authorize("session.resume", session)     // grant.remote-work，先于一切本机读取
② store.load_recovery(session)             // 窄读取；None → Unavailable(BackendUnsupported)，不启动进程
③ resume_request(record) + revalidate_resume_workspace(&request.workspace_cwd)
   // 构造失败（相对/空串/超长/含 NUL，只能经库篡改到达）→ Unavailable(IoError)
   // 复校验失败（不存在/非目录/canonicalize 与持久化值不同）→ Unavailable(IoError)；不回退到别名重解析
④ 自建 accepted + 幂等行（session: Some(<会话 id>)）→ 重放早退 → factory.resume(...)   // 唯一副作用
⑤ 返回 SessionId；不投影、不写终态
```

- **Path A**：未加 `CommandPayload::SessionResume` 变体、未改 `command_name`/`command_kind`/`family`、未加通用分发臂；`accepted` 行与幂等行由本方法自建（与 `create_session` 同一事务形态）。
- **幂等行携带 `session: Some(<会话 id>)`**（与 `session.create` 的 `session: None` 相对），`docs/CORE_PORTS_AND_STORAGE.md` §6 第 20 条字面保持为真。
- 权威取值只有持久化记录：请求里没有 alias（Round 13 订正），客户端 payload 是空对象，core 无从接收客户端提供的 agent/cwd。
- 通过 `self.authorize(actor, "session.resume", Some(session), ..)` 传入目标会话 → Owner 侧判定会要求某个未撤销 Export 覆盖该会话的 agent（与 `NODE_LINK_PROTOCOL.md` §12.5 的会话命令口径一致）。
- 未触碰 `required_grant`（WP2 已落地 `"session.resume" => "grant.remote-work"`；本 WP 只确认它对 `session.resume` 返回 `Some("grant.remote-work")`）。

### 2.6 `settle_session_resume`（F41 的分工）

`Broker::settle_session_resume(actor, &RequestId, CommandStatus, Option<CommandResult>, Option<PublicError>) -> bool` + `UseCases` 入口，与 `settle_session_create` **同形**：

- 只终结 `record.command() == "session.resume"` 的持久记录；记着别的命令时 `InvalidRequest` 且零写入；
- 没有持久记录或记录已终结 → 幂等 no-op（`false`）；落盘失败不报成功（行仍 `accepted`，由 §6 第 16 条启动恢复终结为 `uncertain`）；
- `completed/failed` 分别写 `command.completed`/`command.failed` 事件，其他终态写 `command.uncertain`（原因文案为「无法确认会话是否已恢复」）；
- 幂等与形状语义只写在 `docs/CORE_PORTS_AND_STORAGE.md` **§5.1 的 `[决定]`** 段（F49 的闭环要求），**未改 §6 第 20 条**；
- 不负责投影：`SessionResumeResult`（`remoteSessionRef.exportId`）留 WP6。

### 2.7 文档同步（只加行 / 只改既有决定句）

| 文档 | 改动 |
| --- | --- |
| `docs/CORE_PORTS_AND_STORAGE.md` §2 | `UnavailableKind` 枚举加 `BackendUnsupported`，并加一条 `[决定]` 说明其语义与三处映射（`command.unsupported` / `nodelink.command.unsupported` / `local.unavailable`，不新增任何码） |
| §3.1 | 值对象表加 `AgentSessionId` 行（含「不经过 wire，故上限是本机约束」与「不得编造占位值」） |
| §3.3 | `CommandPayload` 行注明 `session.create` 与 `session.resume` **均**走专用路径、不在枚举中（**未新增变体**） |
| §3.6 | 加 `ResumeSessionRequest`（新形状、不含 alias）与 `SessionRecoveryRecord`（两列不进可投影面、只经 `load_recovery` 进出）两行 |
| §4 | `SessionLifecycle` 行加 `resume_session(actor, RequestId, Digest, SessionId) -> SessionId`、`settle_session_resume(actor, &RequestId, CommandStatus, Option<CommandResult>, Option<PublicError>) -> bool`（调用方仍是 `server::node_link`） |
| §5.1 | rust 块加 `resume` / `agent_session_id` 两条签名（与代码**逐字一致**，靠 `check-contract-drift` 机械核对）；另加一条 `[决定]` 写全 `resume_session` 的顺序、两类不支持的路径、终态归属与 `settle_session_resume` 的同形/幂等语义 |
| §5.2 | rust 块加 `load_recovery` 签名；加一条 `[决定]` 写「`StateChange::Update` 新增两个可空列、写入点在 `create` 返回后、写入后只读、MUST NOT 被恢复流程覆写」 |
| `docs/MODULE_ARCHITECTURE.md` §4.1 | 会话后端端口行补 `resume`/`agent_session_id`、`SessionStore` 行补 `load_recovery`、值对象清单加 `AgentSessionId / ResumeSessionRequest / SessionRecoveryRecord`（DR1-F50） |
| `crates/core/src/model/session.rs` | 既有注释补 `session.resume`（`CommandPayload` 按命令名一对一；两者皆不在其中） |

### 2.8 未触碰（按写范围与 Path A）

`crates/server/`、`crates/storage-sqlite/`、`crates/agent-host/`、`crates/app/`、`plan.md`、`tasks.md`、`design.md`、`specs/`、`compatibility/`、其余 docs 一律未改；`check-contract-drift.mjs` 等脚本未改；`CommandPayload`/命令名/`required_grant` 未改。

---

## 3. 单元测试（与被测代码同文件/同 `#[cfg(test)]` 模块，随实现交付）

`cargo test -p core --all-features`：**136 passed**（基线 121，新增 **15** 个 `#[test]`）：

| 测试 | 覆盖 |
| --- | --- |
| `model::tests::agent_session_id_is_bounded_and_nul_free` | R17：空串 / 513 字符 / 含 NUL 一律 `InvalidValue::AgentSessionId`；512 字符合法；`PortError::InvalidRequest` 文案 |
| `model::tests::resume_session_request_validates_the_persisted_working_directory` | R23/R26：绝对路径合法；空串/相对路径/含 NUL/超长 → `InvalidValue::Field` |
| `model::tests::session_recovery_record_reports_absent_columns_as_none` | §3.6：`None` = `NULL` 的读回语义 |
| `model::tests::port_error_and_kinds_are_wired_to_the_model_errors`（更新） | §2：`ALL.len() == 8` + `backend_unsupported` + `Display` |
| `broker::tests::load_recovery_reports_null_columns_as_absent` | §3.6/§5.2：会话不存在与任一列 `NULL` 都读成 `None`；两列都在时逐字返回 |
| `broker::tests::backend_unsupported_maps_to_the_existing_unsupported_code` | §2/§5.1：`port_error_public` → `command.unsupported`、不可重试 |
| `broker::tests::settle_session_resume_terminates_only_its_own_record_and_is_idempotent` | R27/R34：只终结 `session.resume`；重复终结/无记录都是 no-op 且不覆盖首次结果 |
| `broker::tests::settle_session_resume_rejects_other_commands` | R27/R34：`session.prompt` 的记录不得被改写（`InvalidRequest` + 零写入） |
| `use_cases::tests::create_session_writes_the_recovery_columns_right_after_create` | R6/R17：两列在 `create` 返回后落盘、`workspace_cwd` 是 `canonicalize` 结果、可见版本 2 |
| `use_cases::tests::create_session_writes_no_recovery_columns_without_an_agent_session_id` | R7：未取得标识 → 两列都不写、只有 v1 |
| `use_cases::tests::resume_session_uses_the_persisted_values_and_commits_an_accepted_row` | R26/R27/R31：后端收到的 `agent`/`agent_session_id`/`workspace_cwd` **逐字**等于持久化值；写 `accepted` + 幂等行且 `session = Some(目标会话)`；只返回 `SessionId` |
| `use_cases::tests::resume_session_replay_does_not_spawn_a_second_endpoint` | R28：同键重试只发生一次副作用 |
| `use_cases::tests::resume_session_revalidates_the_persisted_directory_before_calling_the_backend` | R23/R24/R25：目录被删、`canonicalize` 与持久化值不同（追加 `.`）、持久化值含 NUL —— 全部在调用后端**之前**失败、错误为 `Unavailable(IoError)`、后端零调用、持久化取值未被改写 |
| `use_cases::tests::resume_session_reports_missing_recovery_data_as_unsupported` | R21-2/R37：两列 `NULL` → `Unavailable(BackendUnsupported)`、不启动进程、会话数不变（不降级为新建） |
| `use_cases::tests::resume_session_authorizes_before_reading_the_session_row` | R31：越权 → `authorization.scope_denied`，且 `load_recovery` 调用次数为 **0**（「先于本机读取」的可观察证据，fake store 上加了一个计数器） |
| `use_cases::tests::resume_session_propagates_the_backend_unsupported_error` | R37：后端不支持原样上抛、行仍 `accepted`（等适配层结 failed），随后 `settle_session_resume` 可把它推进到 `completed` |

未写 `crates/*/tests/` 的集成用例与驱动脚本（那是 TP2 的范围）。

---

## 4. 检查（实跑记录，完整命令 + 目录 + 退出码）

### PV1 阶段 1（分支检查，crate 子集 = `core`）

日志：`reports/wp3-coder-PV1.log`（HEAD `4f7a2355…`，工作树 0 项未提交改动）

| # | 命令（cwd = worktree 根；`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp3`） | 退出码 | 结果 |
| --- | --- | --- | --- |
| 1 | `cargo fmt --all -- --check` | 0 | PASS（无 diff） |
| 2 | `cargo clippy --locked -p core --all-targets --all-features -- -D warnings` | 0 | PASS（`Checking core` → `Finished`，0 warning） |
| 3 | `cargo test --locked -p core --all-features` | 0 | PASS（`running 136 tests` → `136 passed; 0 failed; 0 ignored`，doc-tests 0） |

环境：`rustc 1.98.1 (48a229cea 2026-09-01)` / `cargo 1.98.1 (797e8a9bc 2026-08-05)`（仓库 `rust-toolchain.toml` 固定，与 CI 同一编译器）；工具链与全部依赖均为 `--locked`。

### PV2（`npm run check`，worktree 根）

日志：`reports/wp3-coder-PV2.log`（HEAD `4f7a2355…`，工作树 0 项未提交改动；`node v24.19.0` / `npm 12.0.2`）

| # | 命令 | 退出码 | 结果 |
| --- | --- | --- | --- |
| 4 | `npm run check` | 0 | PASS（全部合同门禁绿：`schema fixtures OK`、`command catalog OK: 13 commands`、`error registry OK: 58 codes`、`feature registry OK`、`contract assets OK`、`ACP compatibility matrix OK`、`doc links OK: 399 relative links / 6766 section refs`、`crate boundaries OK`、`contract drift OK: §7 的 36 条 DDL 逐条一致；§5 的 15 个 trait / 96 个方法签名与 ports.rs 一致`、`openspec validate` 19/19、agentic 宿主入口 17 文件） |

其中 `contract drift` 是本 WP 最关键的机械判据：**§5.1/§5.2 的新签名与 `crates/core/src/ports.rs` 逐字一致**（方法数 93 → 96；93 是在 base 提交的 `ports.rs` 上用同一扫描口径独立复算得到的）。

### 红窗口证据（**不是**通过条件）

日志：`reports/wp3-coder-red-window.log`（`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`，退出码 101）。实测停在：

```text
error[E0046]: not all trait items implemented, missing: `resume`
   --> crates/agent-host/src/host.rs:449:1
error[E0046]: not all trait items implemented, missing: `agent_session_id`
   --> crates/agent-host/src/session.rs:746:1
error: could not compile `agent-host` (lib) due to 2 previous errors
```

另单跑 `cargo clippy -p storage-sqlite --all-targets` 得 `error[E0046]: … missing: load_recovery`（`crates/storage-sqlite/src/session_store.rs` 的 `SqliteStore`）。两者都在 `plan.md` 已登记的红窗口与责任人之内（WP5 / WP4），**不是** WP3 的失败，也未被用来当作通过条件。

---

## 5. 需求行映射（WP3 的 R 行）

| R 行 | 本 WP 的落点 | 证据 |
| --- | --- | --- |
| R17（storage-schema `新列写入后可读回且不推导`） | 值对象层的非法取值拒绝（空串/NUL/超长）、`create_session` 的两列写入、`load_recovery` 的逐字读回 | §3 表 1、5、9、10；PV1 |
| R22（`恢复流程不覆写持久化取值`） | `resume_session` 全程不产生 `StateChange`，两列无任何写入路径；失败路径测试断言 `workspace_cwd` 字节不变 | §3 表 13 |
| R23（`恢复时的目录复校验`） | `revalidate_resume_workspace`（绝对、存在、是目录、`canonicalize` 逐字相等；不回退） | §3 表 2、13 |
| R24（`目录已被删除时返回不可用且不启动 Agent`） | 同上，`Unavailable(IoError)`，后端零调用 | §3 表 13 |
| R25（`规范化结果变化时拒绝恢复`） | 同上（`canonicalize` 不一致 → 拒绝，且不写新解析结果） | §3 表 13 |
| R26（`目录仍然有效时使用持久化取值`） | 请求 `workspace_cwd` = 持久化原文（不含 alias），后端收到的值逐字相同 | §3 表 11 |
| R27（`命令授权、幂等与终态`） | `grant.remote-work` 授权（复用 WP2 的 `required_grant`）、自建 `accepted`+幂等行、`settle_session_resume` 只终结自己的记录 | §3 表 7、8、11、12 |
| R31（`越权恢复被拒绝且先于本机读取`） | authorize 是第一个动作；测试断言拒绝时 `load_recovery` 调用数为 0（因此无法区分会话是否存在） | §3 表 15 |
| R34（`session.resume` 的 payload 与结果契约） | core 侧：输入全部来自持久化记录（客户端 payload 无法进入后端）、`accepted` 行的形状、终态入口的形状 | §3 表 7、8、11 |
| R36（`携带字段被拒绝且不启动进程`） | wire 侧判据（WP2 的 schema/fixture + WP6 的 handler）；core 侧的结构性保证是「请求里没有可被客户端填充的字段」 | §2.5 |
| R37（`Agent 不支持恢复时终态失败`） | 后端报「不支持」→ `Unavailable(BackendUnsupported)` 原样上抛、行仍 `accepted`，由适配层结 `failed`；两列 `NULL` 走同一条路径且不启动进程 | §3 表 14、16 |

---

## 6. 实现期做的判断（附理由，供 reviewer 逐条核）

1. **`accepted` 行落在 cwd 复校验之后、`factory.resume` 之前**。理由：D3 的编号顺序（①授权 ②读取 ③复校验 ④副作用）字面成立，同时满足「先回 accepted 再做工作」——唯一副作用（可能 spawn）在提交之后。后果是**提交前失败（两列 `NULL`、cwd 复校验失败、持久化值非法）不留持久记录**，与 `session.create` 的 workspace 解析失败同形；`crates/server/src/node_link/command.rs` 已有这条无记录路径（结 `command.terminal` 的确定类失败），WP6 按同一先例处理即可。
2. **三类失败的分类**：两列 `NULL`/行不存在/后端报不支持 → `Unavailable(BackendUnsupported)`（Node Link 终态码 `nodelink.command.unsupported`）；cwd 复校验失败或持久化值无法构造 → `Unavailable(IoError)`（`nodelink.internal.unavailable`）。二者不混用，测试逐条断言了取值。
3. **`AgentSessionId` 的上限取 512 字符**：契约只要求「非空、有长度上限、无 NUL」，没有给具体数值；该取值不经过任何 wire，因此上限是本机约束，取 512 并在类型注释里说明理由（够覆盖实际 Agent 的不透明标识，且能安全落进 `TEXT`/日志/子进程参数）。
4. **`model/config.rs` 的 `no_nul` 提为 `pub(crate)`**：`ids.rs`/`backend.rs` 复用同一谓词，不复制第二份实现（仓库禁止 `common`/`utils`，但同一 `core::model` 内的 `pub(crate)` 谓词已有先例：`ids.rs` 的 `require_bounded`/`char_count` 已被 `backend.rs` 复用）。
5. **两列写入用两个独立 `Option` 字段而非一个打包结构**：派发的任务书与 design D2 给的提交形状就是 `StateChange::Update { …, agent_session_id: Some(id), workspace_cwd: Some(cwd) }`；「两列一起写或都不写」的不变量由 `create_session` 的调用点保证（`agent_session_id()` 为 `None` 时整体不提交），并在 §5.2 的 `[决定]` 与代码注释里写明。

---

## 7. 写入范围与越界

**未越界**：`git diff --name-only <base>..HEAD` 恰好 11 个文件，全部落在 WP3 的写范围内：

```text
crates/core/src/{broker.rs,ports.rs,use_cases.rs}
crates/core/src/model/{backend.rs,config.rs,error.rs,ids.rs,session.rs,tests.rs}
docs/CORE_PORTS_AND_STORAGE.md   （仅 §2、§3.1、§3.3、§3.6、§4、§5.1、§5.2）
docs/MODULE_ARCHITECTURE.md      （仅 §4.1）
```

`crates/server/`、`crates/storage-sqlite/`、`crates/agent-host/`、`crates/app/`、`plan.md`、`tasks.md`、`design.md`、`specs/` 零命中；`docs/CORE_PORTS_AND_STORAGE.md` 的**文件头版本记录**与 §7/§9 亦未改（DR1-F52 把文件头归 WP4，见 §10 待澄清 1）。`git status --porcelain` 为空（无暂存文件）。

---

## 8. 跨 crate 涟漪与交接清单（收口责任人：WP4 / WP5 / WP6）

### 8.1 7 个 `SessionStore` 实现点（6 个文件）——**本 WP 只实现第 1 个**

| # | 文件 / 类型 | 责任人 | 状态 |
| --- | --- | --- | --- |
| 1 | `crates/core/src/broker.rs` / `FakeStore` | **WP3（本报告）** | ✅ 已实现（含 fake 的 `RecoveryColumns` 列存储、`seed_recovery` 与 `load_recovery` 调用计数） |
| 2 | `crates/storage-sqlite/src/session_store.rs` / `SqliteStore` | WP4 | ❌ 待实现（新实现，不只是测试字面量） |
| 3 | `crates/app/tests/support/owner.rs` / `FlakySessionStore` | WP6 | ❌ 待实现 |
| 4 | `crates/server/src/local_admin/test_support.rs` / `NotTouched` | WP6 | ❌ 待实现 |
| 5 | `crates/server/src/local_admin/test_support.rs` / `FixedStore` | WP6 | ❌ 待实现 |
| 6 | `crates/server/src/node_link/command/tests.rs` / `CommandStore` | WP6 | ❌ 待实现 |
| 7 | `crates/server/src/node_link/resource/tests.rs` / `SliceStore` | WP6 | ❌ 待实现 |

### 8.2 另外两个必需 trait 方法的实现点（本 WP 也已把 core 侧补齐）

| trait 方法 | core（WP3） | agent-host（WP5） | server / app 替身（WP6） |
| --- | --- | --- | --- |
| `SessionBackendFactory::resume` | ✅ `FakeBackend` | ❌ `host.rs` 的 `AgentHost` | ❌ `command/tests.rs` 的 `FakeBackends`、`local_admin/test_support.rs` 的 `NotTouched`、`app/tests/support/owner.rs` 的 `ScriptedBackends` |
| `SessionEndpoint::agent_session_id` | ✅ `FakeEndpoint` | ❌ `session.rs` 的 `Endpoint` | ❌ `command/tests.rs` 的 `FakeEndpoint`、`app/tests/support/owner.rs` 的 `ScriptedEndpoint` |

### 8.3 `SessionUpdate` 两个新字段带来的字面量涟漪（**已登记在 plan 的 Shared File Ownership**）

| 文件 | 需要补 `agent_session_id: None, workspace_cwd: None` 的字面量数 | 责任人 |
| --- | --- | --- |
| `crates/storage-sqlite/tests/commit.rs` | 7 | WP4 |
| `crates/storage-sqlite/tests/session_version_rule.rs` | 3 | WP4 |
| `crates/storage-sqlite/tests/retention.rs` | 2 | WP4 |
| `crates/storage-sqlite/tests/contract_v03.rs` | 1 | WP4 |
| `crates/storage-sqlite/tests/enum_coverage.rs` | 1 | WP4 |

（`plan.md` 的该行注记已写「WP4 会因提交形状变化逐处修复这些字面量」；core 内 `broker.rs` 的 12 处同类字面量已在本 WP 内一并修完。）

### 8.4 交给 WP6 的两条实现义务（本 WP 只提供服务端 half）

1. `port_error_code` 加一条臂：`UnavailableKind::BackendUnsupported` → 既有码 `nodelink.command.unsupported`（core 不吐 wire 码）。
2. 用与 `session_create_result` 同源的映射投影 `SessionResumeResult` 并调用 `settle_session_resume`；提交前失败（无持久记录）按 `session.create` 的既有「确定类失败」先例结 `command.terminal`（`crates/server/src/node_link/command.rs` 已有该分支）。

---

## 9. 红窗口与本地钩子的事实说明

1. **workspace 全量不是本 WP 的通过条件**（已登记红窗口）：本 WP 的新方法会打破 `agent-host`（WP5）、`storage-sqlite`（WP4）、`server`/`app` 的四个替身（WP6）。证据见 §4 的 `wp3-coder-red-window.log`。分支 PV1 因此只跑 `-p core`（阶段 1），与 `plan.md` 的 PV1 阶段定义一致。
2. **`.husky/pre-commit` 在本 worktree 实际没有执行**：`git config core.hooksPath` = `.husky/_`，而该目录在本 worktree 不存在（`.husky/` 下只有 `commit-msg` 与 `pre-commit` 两个文件），git 因此静默跳过钩子——所以本次**没有使用** `git commit --no-verify`（它没有被触发）。为避免「本地钩子没跑却被当成通过」，我用 `npx --quiet --no-install commitlint --from <base> --to HEAD --verbose` 单独校验了提交信息（0 problems），并以 PV1/PV2 的实跑记录为准；workspace 全量 `cargo clippy` 的红窗口状态见上一条与日志。

---

## 10. 未执行项与待澄清问题

**未执行（有意，非跳过）**：

1. workspace 全量 `cargo clippy/test`（红窗口，已登记，责任 WP4/WP5/WP6）——分支阶段不做通过条件。
2. `crates/*/tests/` 的集成/E2E 用例、驱动脚本与测试数据（TP2 的范围）。
3. 真实 Codex / Oh My Pi Agent 的 `sessionCapabilities.resume` 只读侦察（`design.md` 的 Risks 列为独立验证任务；无真实 Agent 时记 BLOCKED）。
4. CI 专属判定（`cargo-deny`、`gitleaks`）——本地无等价物，本次未引入任何依赖或密钥。

**待澄清 / 请 main 裁定（都不阻塞本 WP 的交付）**：

1. **`docs/CORE_PORTS_AND_STORAGE.md` 的文件头版本记录**：该文件按惯例每次合同改动追加一条 `> 版本：0.x（…）`，而本次已改 §2/§3.1/§3.3/§3.6/§4/§5.1/§5.2 与 §5 的 rust 块（+3 方法）。文件头不在 WP3 的写范围（DR1-F52 明确把「文件头版本记录」归 WP4），且 WP4 是该文件的 Merge Owner（顺序 WP3 → WP4）——我按范围纪律**没有改**，请确认由 WP4 在其版本记录里一并写明本次的 §5 rust 块变化（+`resume`/`agent_session_id`/`load_recovery`），或另行授权我补一行。
2. `AgentSessionId` 的 512 字符上限是我按「本机约束、不经过 wire」的定位选定的取值（契约只要求「有长度上限」）；若 reviewer 认为该数值需要登记进 `docs/CORE_PORTS_AND_STORAGE.md` 的行（目前行内写的是「≤512 字符」，已与代码一致），请一并确认口径。

---

## 11. `handoff_index`

```yaml
handoff_index:
  - task_id: "2.3"
    work_package: WP3
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "4f7a23554f76915cf5f29cc23e292ce270e014c9"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/session-resume/reports/wp3-coder.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 agentic/session-resume-wp3 的交付提交 4f7a2355… 上实跑（工作树 0 项未提交改动）：阶段 1 的 crate 子集 = core（plan.md 的 PV1 阶段定义）；命令与参数为 cargo fmt --all -- --check / cargo clippy --locked -p core --all-targets --all-features -- -D warnings / cargo test --locked -p core --all-features，三者退出码均为 0（136 passed）；环境 rustc 1.98.1 / cargo 1.98.1（rust-toolchain.toml 固定）、CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp3；原始输出见 reports/wp3-coder-PV1.log；workspace 全量属已登记红窗口（WP4/WP5/WP6 收口），不作为本阶段判据（reports/wp3-coder-red-window.log）"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.3"
    work_package: WP3
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "4f7a23554f76915cf5f29cc23e292ce270e014c9"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/session-resume/reports/wp3-coder.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一提交、worktree 根实跑 npm run check（退出码 0，node v24.19.0 / npm 12.0.2，未执行 npm install/ci）：合同门禁全绿，其中 check:contract-drift 断言 §5 的 15 个 trait / 96 个方法签名与 crates/core/src/ports.rs 逐条一致（本 WP 新增的三个方法已逐字同步进 §5.1/§5.2），check:command-catalog 仍为 13 命令、词表四处一致；原始输出见 reports/wp3-coder-PV2.log"
    source_evidence: NOT_APPLICABLE
```

> 说明：本报告不含独立 review、TP2 用例与 E2E 的结论——它们各自独立派发；PV1 的阶段 2（集成基线 workspace 全量）由主 Agent 在候选阶段执行。
