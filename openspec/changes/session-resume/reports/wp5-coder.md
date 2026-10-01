# WP5 coder 报告（`agent-host` 的创建回填与进程不在时的恢复）

```yaml
task_id: "2.5"
work_package: WP5
role: coder
phase: implement
agent_context: "coder-E（子 Agent worker，本轮为**新实例**：本 worktree 之前无提交、无报告，故本报告是 WP5 的首次交付）。隔离方式：provisioner 分配的独立 worktree `D:/Project/acp-remote-wt/session-resume-wp5`（分支 `agentic/session-resume-wp5`）+ 独立 `CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp5` + provisioner 提供的 `node_modules` 目录联接（未执行任何 npm install/ci/update）；未继承其它实例的对话或上下文，未自行创建/切换/清理 worktree 或分支。"
target_revision: "4e53fcf9fcd9eb06130f3cab8de7d21e7973cf63"
scope: "crates/agent-host/ 全部（src/** 的内联单测与既有 tests/session.rs）"
result: PASS
resource_cleanup: "worktree、分支、CARGO_TARGET_DIR、node_modules 联接均保留给 provisioner 回收（本角色不自行清理/切换/合并）；未申请独占资源——本 WP 的用例全部在进程内 fake 端口 + 自建子进程上跑，未绑定任何固定 TCP 端口，也未写仓库内共享目录（tempdir / TempFile 由用例自行创建与回收）"
```

- 需求覆盖：R5–R12；并交付 R26「发送的取值就是持久化取值」与 R35/R37 的 agent-host 侧那一半（详见 §5 映射表）
- 证据：`reports/wp5-coder-PV1.log`（PV1 阶段 1，实跑）、`reports/wp5-coder-precommit.log`（`node scripts/pre-commit.mjs` 等价检查）、`reports/wp5-coder-red-window.log`（红窗口实况）
- 结论：**PV1（阶段 1，crate 子集 = `agent-host`）全绿**；WP5 的 Verification 列只有 PV1（不要求 PV2），另额外实跑了 `npm run check`（作为「未改任何合同资产」的机械证据，exit 0）。本报告不覆盖独立 review、TP2 用例与 E2E，也不代替它们的结论。

---

## 1. 起点、包含关系与输入版本（实测原始输出）

在 `D:/Project/acp-remote-wt/session-resume-wp5` 实测（`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp5`）：

```text
$ git rev-parse HEAD
8a08db8e77ac67efee317363ce241261af05c008          # == 派发要求的集成基线，逐字相同

$ git merge-base --is-ancestor 4f7a23554f76915cf5f29cc23e292ce270e014c9 HEAD ; echo $?
0                                                  # true：code:WP3 的 delivery 提交已在起点内

$ git merge-base --is-ancestor 248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb HEAD ; echo $?
0                                                  # true：code:WP1 的 delivery 提交（acp-protocol 的 session/resume DTO）已在起点内

$ git merge-base --is-ancestor 32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992 HEAD ; echo $?
0                                                  # true：WP2 的 delivery 提交（仅供溯源，不在 WP5 的依赖声明内）

$ git branch --show-current
agentic/session-resume-wp5                         # provisioner 已就位，未自行创建/切换/清理
$ git status --porcelain                           # 空：起点干净，无用户未提交改动
```

交付提交与包含关系（合入前核对用）：

```text
base   (起点) = 8a08db8e77ac67efee317363ce241261af05c008
target (交付) = 4e53fcf9fcd9eb06130f3cab8de7d21e7973cf63
$ git merge-base --is-ancestor 8a08db8e… 4e53fcf9…  → 0 (true)
$ git merge-base --is-ancestor 4f7a2355… 4e53fcf9…  → 0 (true)
$ git merge-base --is-ancestor 248d9b9f… 4e53fcf9…  → 0 (true)
$ git rev-parse HEAD^{tree} / 8b3a743^{tree}        → 9fbcdc28… == 9fbcdc28…（见下注）
$ git diff --name-only 8a08db8e…..HEAD | wc -l      → 4（全部在 WP5 写范围内，见 §7）
$ git status --porcelain | wc -l                    → 0（无未提交改动、无暂存文件）
```

分支上**一个**提交（Conventional Commits + 中文主题，scope `agent-host` 取自 `commitlint.config.mjs` 词表）：

```text
4e53fcf feat(agent-host): 落地会话恢复的后端路径与 ACP 会话标识暴露        # ← target
```

> 说明：该提交曾以 `8b3a743` 落盘（内容相同），随后为满足派发要求「提交后手动跑一次等价检查」而 `git reset --soft HEAD~1`（把内容原样放回暂存区）→ 在 worktree 内跑 `node scripts/pre-commit.mjs` → 用同一份提交信息重新提交，得到现 SHA。两次提交的 **tree 逐字节相同**（`9fbcdc28…`，上表已实测），因此 PV1 的证据对该内容成立；未 rebase、未强推、未改动任何文件内容。

**输入版本**：

- 开工门禁：`design.md@D4` + `specs/local-agent-host/spec.md@design-rev-4` + `plan.md` 的 WP5 行与 `## Dependency Handoffs` 红窗口段 + `tasks.md` 2.5；一并读了 `design.md` D1–D6 与全部「契约订正」注记（尤其 D2 Round 9 的提交点、D3 Round 10 的 `settle_session_resume`、D3 Round 13 的 `workspace_cwd` 不含 alias）。
- 上游接口按代码实测取用（不按文档猜测）：`crates/core/src/ports.rs` 的 `SessionBackendFactory::resume` / `SessionEndpoint::agent_session_id`，`crates/core/src/model/backend.rs` 的 `ResumeSessionRequest { agent, agent_session_id, workspace_cwd }`，`crates/acp-protocol/src/message.rs` 的 `SessionResumeRequest` / `SessionResumeResponse`，`crates/acp-protocol/src/capability.rs` 的 `supports_session_resume()`。
- 未申请澄清、未触发 BLOCKED：契约（含 Round 13 订正后的形状）与代码一致，无歧义。

---

## 2. 实现内容（逐项对应 `tasks.md` 2.5 与 `design.md` D4）

### 2.1 `SessionEndpoint::agent_session_id`（`crates/agent-host/src/session.rs`）

- `AcpSession` 新增字段 `agent_session_id: Option<AgentSessionId>`（`SessionInit` 同步新增该字段；`rebind()` 继承它，重开端点读回同一标识）。
- `AcpSession::agent_session_id(&self) -> Option<&AgentSessionId>` + `impl SessionEndpoint for Endpoint` 的同名方法转调。
- 语义：只有 `session/new` 真的返回了标识、且该标识能构成合法 `AgentSessionId`（非空、≤512 字符、无 NUL）时才是 `Some`；取不出合法值时是 `None`（core 因此两列都不写），**不编造占位值**。恢复得到的端点读回的是**持久化**的标识。

### 2.2 创建时暴露标识（`crates/agent-host/src/host.rs` 的 `create`）

`session/new` 响应解码后立刻 `AgentSessionId::new(&response.session_id).ok()`，与 `acp_session_id` 一起进 `SessionInit`。失败路径（启动失败/超时/Agent 返回错误/响应无法解码）根本没有端点返回，因此 core 拿不到任何标识——后端不读写存储（该写由 core 在创建流程内紧接着的 `StateChange::Update` 完成，见 design D2）。

### 2.3 `SessionBackendFactory::resume`（`crates/agent-host/src/host.rs`）

```text
ensure_runtime_tracked(agent)        // 复用既有启动路径：profile 解析 → 凭据注入边界 → spawn → stdio 分帧
                                     //   → initialize；返回 (runtime, spawned)
read runtime.capabilities            // initialize 之后读**协商到的**能力
if !supports_session_resume()  → 回收本次拉起的子进程 + Err(Unavailable(BackendUnsupported))
                                  // 一个字节都不发；不建绑定；不新建会话
session/resume { sessionId, cwd }    // 取值全部来自 request（持久化记录），不含 alias
   ├─ Err（Agent 拒绝 / 进程消失 / 超时）→ 回收本次拉起的子进程 + Err（明确失败）
   ├─ 响应无法解码                      → 回收本次拉起的子进程 + Err(SpawnFailed→Unavailable(IoError))
   └─ Ok → evict_binding(session)     // 旧绑定先让出（close_session + 摘映射，全目录扫描）
          → 构造端点并 insert_session // 同一 core 会话此时只有一条活跃绑定
```

- **能力门控的位置**：`initialize` 之后、发送 `session/resume` 之前（design D4 与 CR7-F1 的可满足保证）。未宣告时的两个可观察保证都实现：**不发** `session/resume`；**在返回前终止并回收本次为恢复而拉起的子进程**（无残留进程、无会话绑定）。
- **`spawned` 的区别对待**：新增 `ensure_runtime_tracked` 回报「本次是否真的拉起了一个新进程」，只有 `spawned == true` 才在失败路径调用 `discard_spawned_runtime`（复用到的既有进程可能正在服务其它会话，不得被一次失败的恢复连带结束）。旧的 `ensure_runtime` 保留为薄包装，既有调用点零改动。
- **`cwd` 的来源**：`session_resume_params(&agent_session_id, &workspace_cwd)` 用 WP1 的类型化 DTO（`acp_protocol::message::SessionResumeRequest::new(...)`）编码，**不含** workspace 别名、不解析路径、不做任何补齐；`cwd` 与 `sessionId` 就是 `ResumeSessionRequest` 里的字符串本身。
- **响应用 WP1 的类型化 DTO 解码**：`serde_json::from_value::<message::SessionResumeResponse>(value)`（`modes` / `configOptions` 都可缺失，缺失即未宣告，不编造）；解码失败明确失败并清理。
- **同一 core 会话只有一条活跃绑定**：`evict_binding` 扫描全部运行时，命中的旧绑定 `close_session()`（停止接受 turn）后再 `remove_session`，然后才插入新绑定；映射不完整（只有 `by_core` 条目）时只摘条目、不猜端点。
- **凭据注入边界未被绕过**：`resume` 只经 `ensure_runtime` → `launch::resolve_launch`（本 crate 既有的唯一启动入口），没有第二条 spawn 路径、没有读启动配置文件、没有把任何持久化取值注入子进程环境。

### 2.4 fake ACP Agent 的额外义务（CR7-F5，`crates/agent-host/src/bin/acpr-fake-acp-agent.rs`）

| 项 | 落点 | 语义（**冻结，不再改名**） |
| --- | --- | --- |
| `session/resume` 分支 | `handle()` 的新臂 | 先按 pinned schema 校验 `{ sessionId, cwd }` 形状（不合规 `exit(8)`，与既有 `session/new`/`set_mode` 同一手法） |
| 场景 `resume-ok` | 同上（成功分支） | 回 `{}`（`modes`/`configOptions` 都缺失是合法响应） |
| 场景 `resume-error` | 同上 | 回 JSON-RPC error（`-32001`「原生会话已被清理」）→ Agent 拒绝恢复 |
| 场景 `session-new-error` | `session/new` 臂 | 回 JSON-RPC error（`-32002`）→ 创建失败、不产生标识 |
| `--dump-requests <path>` | `main()` 的读循环 + `dump_method()` | **每次收到的带 `method` 的入站报文追加一行 method**（append 模式，逐行） |

模块文档同步登记了三个场景与 `--dump-requests`。既有的场景名、选项名与行为一律未改。

### 2.5 未触碰（按写范围）

`crates/core/`、`crates/acp-protocol/`、`crates/server/`、`crates/storage-sqlite/`、`crates/app/`、`compatibility/`、`schemas/`、`fixtures/`、`docs/`、`scripts/`、`plan.md`、`tasks.md`、`design.md`、`specs/` **零命中**（§7 有机械证据）。

---

## 3. 测试（与被测代码同 crate；未新增 `tests/*.rs` 文件——新增用例文件属 TP2）

`cargo test -p agent-host --all-features`：**62 passed**（基线 55：本 WP 新增 **7 个**测试——`tests/session.rs` 13 → 19，`src` 内联单测 4 → 5；`tests/catalog.rs` 22、`tests/supervision.rs` 15、`tests/view_contract.rs` 1 未动）：

| # | 测试（新增 7 个） | 覆盖 | 断言要点 |
| --- | --- | --- | --- |
| 1 | `host::tests::resume_params_carry_the_persisted_values_verbatim`（`src/host.rs` 内联单测） | R8/R26 | `session_resume_params` 的 JSON 恰好 `{"sessionId":…,"cwd":…}`：取值逐字来自入参；键集合排序后断言为 `["cwd","sessionId"]`（**不含** alias、默认值或任何补齐字段） |
| 2 | `create_exposes_the_acp_session_id_verbatim`（`tests/session.rs`） | R5/R6 | 创建成功后 `endpoint.agent_session_id() == Some("acp-session-1")`，与 fake child 在 `session/new` 里给出的标识逐字相同 |
| 3 | `failed_session_new_yields_no_endpoint_and_no_identifier` | R7 | `--scenario session-new-error` ⇒ `create` 返回 `Err(InvalidRequest)`；随后 `open` 仍失败（无端点 ⇒ 无标识可交给 core）、无事件 |
| 4 | `resume_restores_an_interactive_endpoint_when_the_capability_is_declared` | R8/R9/R26/R35 | 宣告能力后 `resume` 成功；端点读回**持久化**标识；`reference()` 仍是 core 的 `SessionId`；`prompt` 后收到 `turn.completed`（可交互）；`--dump-requests` 里有 `session/resume`、**没有** `session/new` |
| 5 | `undeclared_resume_capability_is_refused_without_sending_the_request` | R10/R37 | 能力省略 ⇒ `Err(Unavailable(BackendUnsupported))`；dump 里有 `initialize`、**没有** `session/resume`；心跳文件出现后 400 ms 内**不再增长**（进程外证据：本次拉起的子进程已回收）；`runtime_running == false`；`open` 仍失败、无事件 |
| 6 | `agent_refusal_of_resume_fails_explicitly` | R11 | `--scenario resume-error` ⇒ `Err(InvalidRequest)`；dump 里有 `session/resume`、**没有** `session/new`（不静默改走新建）；`open` 失败；心跳停止增长 |
| 7 | `repeated_resume_leaves_a_single_dispatching_binding` | R12 | 连续两次 `resume` 都成功；**旧端点** `prompt` 返回 `Err`（旧绑定已让出）、旧 collector 零事件；**新端点** `prompt` 完成 `turn.completed` |

**测试有效性（反向验证，避免「恒真的断言」）**：把 `discard_spawned_runtime` 临时改为永早退（`if true || !spawned`）后，测试 5 立刻失败并给出原始输出 `心跳仍在增长 left: 9 right: 1`（心跳文件在 400 ms 内从 1 字节涨到 9 字节 ⇒ 子进程确实残留）；恢复实现后重新跑通。该实验只改一次断言外的实现行并已还原（`diff` 与备份逐字节相同，工作树随之回到已验证内容）。

未写 `crates/agent-host/tests/` 的**新增**用例文件、驱动脚本与测试数据（属 TP2 的范围）；本 WP 只在既有 `tests/session.rs` 内扩展。

---

## 4. 检查（实跑记录：完整命令 + 目录 + 退出码）

### 4.1 PV1 阶段 1（分支，crate 子集 = `agent-host`）

日志：`reports/wp5-coder-PV1.log`（HEAD `4e53fcf9…`，`git status --porcelain` 为空）

| # | 命令（cwd = worktree 根；`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp5`） | 退出码 | 结果 |
| --- | --- | --- | --- |
| 1 | `cargo fmt --all -- --check` | 0 | PASS（无 diff） |
| 2 | `cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings` | 0 | PASS（`Checking agent-host` → `Finished`，0 warning） |
| 3 | `cargo test --locked -p agent-host --all-features` | 0 | PASS（`5 + 0 + 22 + 19 + 15 + 1` = **62 passed**、0 failed、0 ignored；doc-tests 0） |

环境：`rustc 1.98.1 (48a229cea 2026-09-01)` / `cargo 1.98.1 (797e8a9bc 2026-08-05)`（仓库 `rust-toolchain.toml` 固定，与 CI 同一编译器）；全部依赖与工具链均 `--locked`。

### 4.2 `node scripts/pre-commit.mjs`（派发要求的等价检查；**仅因红窗口失败**）

日志：`reports/wp5-coder-precommit.log`（在 worktree 根运行 worktree 内的脚本副本，**退出码 101**）

| # | 步骤 | 结果 |
| --- | --- | --- |
| 1 | `cargo fmt --all -- --check` | PASS |
| 2 | `npm run check` | PASS（`schema fixtures OK: 120 valid / 26 invalid`、`command catalog OK: 13 commands`、`error registry OK: 58 codes`、`feature registry OK: 11 ids`、`contract assets OK: 17 schemas / 159 fixture`、`ACP compatibility matrix OK`、`doc links OK: 399 links / 6766 refs`、`crate boundaries OK: 12 crates`、`contract drift OK: §7 36 条 DDL + §5 15 trait / 96 方法签名`、`check:agentic` 全 PASS，`openspec validate` 19/19） |
| 3 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | **FAIL（退出码 101）** —— 全部诊断都落在已登记红窗口的 `storage-sqlite`（WP4，`SqliteStore` 缺 `load_recovery`）与 `server`（WP6：`NotTouched`/`FixedStore`/`CommandStore`/`SliceStore` 的 `load_recovery`/`resume`/`agent_session_id`，以及 `command.rs:2191` 的 `E0004`）；**`agent-host` 已无任何诊断**（日志里它只出现为 `Checking agent-host`） |

按派发提示与 `plan.md` 的「红窗口内的提交钩子（DR1-F23）」条款，本次提交用 `git commit --no-verify` 落地（理由：仅因已登记红窗口的 workspace clippy 失败，非本 WP 缺陷，收口责任人 WP4/WP6）。另需如实记录一条**环境事实**：本 worktree 实测 `core.hooksPath` = `.husky/_` 而该目录不存在（`verification.md` 已登记所有 WP worktree 同此），git 因此静默跳过 pre-commit/commit-msg——所以上面的脚本是**手动实跑**的等价检查，而不是钩子自动跑出来的。

### 4.3 红窗口实况（**不是**通过条件）

日志：`reports/wp5-coder-red-window.log`（`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`，退出码 101）。剩余诊断逐条落在：

```text
crates/storage-sqlite/src/session_store.rs:1942   E0046  SqliteStore 缺 load_recovery            （WP4）
crates/server/src/local_admin/test_support.rs:817 E0046  NotTouched 缺 load_recovery             （WP6）
crates/server/src/local_admin/test_support.rs:961 E0046  FixedStore 缺 load_recovery             （WP6）
crates/server/src/local_admin/test_support.rs:1021 E0046 NotTouched 缺 resume                     （WP6）
crates/server/src/node_link/command/tests.rs:145  E0046  CommandStore 缺 load_recovery           （WP6）
crates/server/src/node_link/command/tests.rs:437  E0046  FakeBackends 缺 resume                   （WP6）
crates/server/src/node_link/command/tests.rs:484  E0046  FakeEndpoint 缺 agent_session_id        （WP6）
crates/server/src/node_link/resource/tests.rs:265 E0046  SliceStore 缺 load_recovery             （WP6）
crates/server/src/node_link/command.rs:2191       E0004  core_payload() 未覆盖 SessionResume     （WP6）
```

与本 WP 的对比：基线（`8a08db8e`）上 `agent-host` 有 2 条 `E0046`（`resume`、`agent_session_id`），现已被本 WP 消除；本 WP **未新增**任何 workspace 诊断。

---

## 5. 需求行映射（WP5 的 R 行）

| R 行 | 本 WP 的落点 | 证据 |
| --- | --- | --- |
| R5（`会话创建时暴露 ACP 会话标识`） | `create` 把 `session/new` 的标识交给端点（core 再落盘，写两列在 core） | §3 表 2；PV1 |
| R6（`创建成功暴露 ACP 会话标识`） | 同上，`agent_session_id()` 逐字等于 Agent 返回值 | §3 表 2 |
| R7（`未取得标识时不编造取值`） | `session/new` 未成功完成 ⇒ 无端点（唯一暴露口），core 拿不到标识；`session/new` 返回的标识构造不出合法 `AgentSessionId` 时给 `None` 而非占位值 | §3 表 3 + §2.1 |
| R8（`进程不在时的会话恢复`） | `resume` 按 `request.agent` 重新解析 profile → 复用既有启动路径拉起子进程 → `initialize` → 发送 `session/resume { sessionId, cwd }` → 登记端点 | §2.3、§3 表 4 |
| R9（`能力已宣告时恢复到可交互`） | 成功后端点接受 `prompt` 并完成 turn；同一 core 会话只有一个可派发端点（turn 串行由既有机制保证） | §3 表 4、表 7；PV1 |
| R10（`能力未宣告时显式不支持且不发送恢复请求`） | 门控在 `initialize` 之后、发送之前：不发送 + 回收本次拉起的子进程 + 返回明确不支持；不新建会话、不伪造成功 | §3 表 5（含心跳与 dump 双重证据） |
| R11（`Agent 拒绝恢复时明确失败`） | `request` 的错误原样转成 `PortError`，清理本次拉起的进程，不进入可交互状态、不改走新建 | §3 表 6 |
| R12（`反复恢复不产生第二个端点`） | `evict_binding`：旧绑定 `close_session` + 摘映射后才插入新绑定 | §3 表 7 |
| R26（`目录仍然有效时使用持久化取值`；WP5 视角） | 发送的 `cwd` 就是 `request.workspace_cwd` 原文，参数里没有别名、没有重解析 | §3 表 1（键集合与取值）、表 4 |
| R35（`正常恢复并回传会话引用`；agent-host 侧） | `resume` 返回的端点 `reference()` 是 core 给的 `SessionId`，与入参相同 | §3 表 4 |
| R37（`Agent 不支持恢复时终态失败`；agent-host 侧） | 后端统一以 `Unavailable(BackendUnsupported)` 表达「不支持」，由 WP6 映射为既有 wire 码 | §3 表 5 |

---

## 6. 实现期做的判断（附理由，供 reviewer 逐条核）

1. **`spawned` 标志而不是「失败就无条件关进程」**：能力未宣告时规格要求回收的是「**本次为恢复而拉起**的子进程」。若复用到的既有进程正在服务其它会话，把整个 Agent 进程关掉会破坏无关会话；若不复用、每次恢复都另起进程，则与 `create` 的既有语义不一致且更贵。因此把「是否真的拉起了新进程」作为 `ensure_runtime` 的返回值暴露出来（私有方法，既有调用点零改动）。
2. **失败路径的分类**：Agent 拒绝/进程消失/超时 → 沿用 `HostError::to_port_error()` 的既有映射（`InvalidRequest` / `Unavailable(IoError)` 等）；能力未宣告 → `Unavailable(BackendUnsupported)`（new value 由 WP3 落地）；响应无法解码 → `SpawnFailed` → `Unavailable(IoError)`。未新增错误变体、未改既有映射表（`error.rs` 的 19 项逐变体测试仍绿）。
3. **`session_resume_params` 用 WP1 的类型化 DTO 序列化**：字段名（`sessionId`/`cwd`）因此来自 ACP wire 的单一机器定义，而不是在本 crate 里再抄一遍字符串；该函数的编码失败映射沿用本文件 `initialize` 对「自编码失败」的既有写法（`HostError::IdUnavailable`）。取值来源只有 `ResumeSessionRequest` 的两个字段。
4. **`session/new` 返回的标识取不出合法 `AgentSessionId` 时给 `None`（而不是让 `create` 失败）**：那是一个「Agent 给了奇怪标识但会话仍可用」的边角；让创建失败会改变既有可观察行为（超出本 WP 范围），而 `None` 恰好满足 R7「未取得标识时不编造取值」且 core 因此两列都不写、该会话不被当作可恢复会话。
5. **`--dump-requests` 严格按派发契约只写 method**（一行一个 method，含通知类方法）。副作用见 §8 第 1 条：TP1 的个别断言原本想从该文件读 `params`，需要 main 裁定（已在实现完成后即时向主 Agent 报了 progress_update）。
6. **恢复对「进程仍活着」的会话复用既有进程**（沿用 `ensure_runtime` 语义），只让旧绑定先让出。R12 的可观察保证（同一会话最多一个可派发端点）成立且更省资源；但 TP1 的 SR-R12-2 假设了「第二个进程 P2 + 第二个心跳文件 h2」，与本实现不符（见 §8 第 2 条）。

---

## 7. 写入范围与越界

**未越界**：`git diff --name-only <base>..HEAD` 恰好 4 个文件，全部落在 WP5 的写范围（`crates/agent-host/`）内：

```text
crates/agent-host/src/bin/acpr-fake-acp-agent.rs
crates/agent-host/src/host.rs
crates/agent-host/src/session.rs
crates/agent-host/tests/session.rs
```

按派发要求顺带核对了「未改任何合同资产」：`compatibility/`、`schemas/`、`fixtures/`、`docs/` 的 diff **零命中**；并且 §4.2 的 `npm run check` 在那份内容上 exit 0（13 命令 / 58 错误码 / 11 feature / 17 schema / 159 fixture / 12 crate 依赖方向 / 合同漂移 / 文档引用 / agentic 入口全绿）——这是「未造成合同漂移」的机械判据。`git status --porcelain` 为空（无暂存文件）。

---

## 8. 给下游的接口事实、未执行项与待澄清问题

### 8.1 给 TP2 的接口事实（已即时回报主 Agent；都不阻塞本 WP 交付）

1. **`--dump-requests <path>` 只写 method，不含 `params`**（按派发契约「每次收到请求追加一行 method」）。TP1 设计的 SR-R8-1（`params.sessionId`）与 SR-R26-2（`params.cwd` 逐字节）无法从该文件读取；TP1 自己给过备选写法（「或由 fake child 把 `session/resume` 的 `params` 全文写入同一目标文件」）。本 WP 未擅自改变该选项语义。若维持 method-only，TP2 可用的等价证据是：① fake child 对 `session/resume` 做 pinned-schema 形状校验（形状不合规 `exit(8)` ⇒ 请求形状可观察）；② 本 WP 的内联单测 `resume_params_carry_the_persisted_values_verbatim` 钉死参数取值与键集合。
2. **`resume` 对进程仍活着的会话复用既有进程**（不拉起第二个进程），旧绑定先让出。TP1 的 SR-R12-2 里「P2 / 第二个心跳文件 h2」的前提不成立；该用例的可观察保证（恰好一个端点可派发、`session/resume` 恰好一行）仍然成立。
3. **`resume-ok` 场景下 `session/prompt` 走默认分支**（发一条 `agent_message_chunk` + `end_turn`），不叠加 `chunked-updates`；这与 TP1 §7-Q1 的 fallback 一致（未新增 `--prompt-scenario` 选项——派发只冻结了三个场景与 `--dump-requests`）。
4. **`ResumeSessionRequest` 已按 design D3 的 Round 13 订正**：字段是 `workspace_cwd: String`（不含 alias），TP2 直接传持久化的 canonical path 字符串即可（不需要 `ResolvedWorkspace`）。

### 8.2 未执行（有意，非跳过）

1. workspace 全量 `cargo clippy/test`（红窗口，已登记，责任 WP4/WP6）——分支阶段不做通过条件；本 WP 的回归已用 `-p agent-host` 全量覆盖。
2. `crates/agent-host/tests/` 的**新增**用例文件、驱动脚本与测试数据（TP2 的范围）。
3. 真实 Codex / Oh My Pi Agent 的 `sessionCapabilities.resume` 只读侦察（`design.md` 的 Risks 列为独立验证任务；无真实 Agent 时记 BLOCKED）。本 WP 只按 fake Agent 断言门控逻辑，**不**声称任何真实 Agent 支持恢复。
4. CI 专属判定（`cargo-deny`、`gitleaks`）——本地无等价物；本 WP 未引入任何新依赖、未触碰密钥。

### 8.3 待澄清（请 main 裁定，均不阻塞 WP5 交付）

1. §8.1 第 1 条：`--dump-requests` 的格式是否需要扩展为「method + params」（会改变已冻结的选项语义，需明写并同步 TP1 设计）；还是维持 method-only 并让 TP2 改用 §8.1 给出的等价证据。
2. §8.1 第 2 条：`resume` 复用活着的进程是否符合预期（本 WP 按「最小改动 + 不改变 `ensure_runtime` 既有语义」实现），或需要改为「恢复一律另起进程」。若裁定为后者，属新的行为选择，需由 main 回写契约后再派发修复轮。

---

## 9. `handoff_index`

```yaml
handoff_index:
  - task_id: "2.5"
    work_package: WP5
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "4e53fcf9fcd9eb06130f3cab8de7d21e7973cf63"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/session-resume/reports/wp5-coder.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 agentic/session-resume-wp5 的交付提交 4e53fcf9… 上实跑（git status --porcelain 为空）：阶段 1 的 crate 子集 = agent-host（plan.md 的 PV1 阶段定义）；命令与参数为 cargo fmt --all -- --check / cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings / cargo test --locked -p agent-host --all-features，三者退出码均为 0（62 passed）；环境 rustc 1.98.1 / cargo 1.98.1（rust-toolchain.toml 固定）、CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp5；原始输出见 reports/wp5-coder-PV1.log；workspace 全量属已登记红窗口（WP4/WP6 收口），不作为本阶段判据（reports/wp5-coder-red-window.log）"
    source_evidence: NOT_APPLICABLE
```

> 说明：本报告不含独立 review、TP2 用例与 E2E 的结论——它们各自独立派发；WP5 的 Verification 列只有 PV1（无 PV2），因此不生成 PV2 行。§4.2 的 `npm run check` 与 §4.3 的红窗口日志是**附加证据**，不作为 WP5 的通过条件。
