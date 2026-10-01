# TP1 测试设计报告（change: session-resume / 交付单元 U1）

<!-- 本报告是角色报告：只覆盖 TP1 范围（测试设计与需求映射）。不修改 plan.md/tasks.md/verification.md。 -->

## Shared Report

- **task_id**: `2.7`（另覆盖 `4.1`）
- **role**: tester
- **phase**: design-author
- **stage**: work-package
- **agent_context**: 子 Agent `tester-A`（TP1），`fork_turns="none"` 隔离，不继承任何实现对话；规划根 = `D:/Project/acp-remote/openspec/changes/session-resume`（未跟踪目录，从主仓库**绝对路径**只读）；代码 worktree = `D:/Project/acp-remote-wt/session-resume-tp1`（分支 `agentic/session-resume-tp1`，基线 `81e350ff340014265eb7c9251237c799d4357fee`）
- **target_revision**: `NOT_AVAILABLE`（设计阶段：本阶段按任务要求不产出代码/用例提交，无可绑定的交付提交；用例绑定到 TP2 的交付提交与各 WP 的固定提交）
- **scope**: 按 plan.md `## Coverage Index` 的 R1–R37 逐行设计场景/步骤/断言、稳定用例 ID、需求映射、真实入口、前置数据构造、用例落点与归属（TP2 或实现者内联）。**不写任何可编译用例，不改 `crates/**`**
- **changes**: 无代码改动；本报告的产物是本文件（`reports/tp1-test-design.md`）
- **checks**: 本阶段计划内检查为 `NOT_APPLICABLE`（plan.md 的 TP1 行：设计阶段不跑 PV1/PV2）。本报告内自洽核对见 §1.2
- **issues**: 见 §7（无 FAIL 级问题；5 项待澄清/风险，均不阻塞设计产出）
- **result**: `PASS`（设计交付齐备：37/37 行有稳定用例 ID、异常/边界路径、真实入口与落点；未编写/未编译部分在 §6 明确标注）
- **evidence_paths**: 本文件；plan.md（Coverage Index）；5 份增量 specs；tasks.md 2.7/4.1/4.2
- **resource_cleanup**: 本阶段无构建、无测试库、无进程、无端口占用（只读文件与 git 查询）；未创建任何 `CARGO_TARGET_DIR` 产物，未占用 `D:/Project/acp-remote-target/session-resume-tp1`

---

## 1. 输入与核对

### 1.1 输入契约（只读）

| 输入 | 路径 | 用途 |
| --- | --- | --- |
| 协作计划 | `openspec/changes/session-resume/plan.md` | Coverage Index、Work Packages、Runtime Resources、PV 定义 |
| 行为契约 | `specs/acp-wire-protocol/spec.md`、`specs/local-agent-host/spec.md`、`specs/storage-schema-v2-migration/spec.md`、`specs/workspace-resolution/spec.md`、`specs/node-link-owner-server/spec.md` | 断言口径（需求与场景原文） |
| 方案契约 | `openspec/changes/session-resume/design.md` D1–D6 | 命令形状、v5 列语义、端口与用例顺序、门控语义、pack 规则 |
| 任务 | `openspec/changes/session-resume/tasks.md` 2.7 / 4.1 / 4.2 | TP1/TP2 边界 |
| 角色指令 | `openspec/schemas/agentic/roles/tester.md`、`roles/_shared/role-report.md` | design-author 交付判据与报告格式 |
| 上游 ACP 形状 | `schemas/acp/v1/upstream/schema.json`（`ResumeSessionRequest`/`ResumeSessionResponse`/`SessionResumeCapabilities`，sha256 由矩阵 pin） | 用例里的字段口径（required `sessionId`+`cwd`；响应只有 `modes`/`configOptions`/`_meta`；能力 `{}` 即支持） |

### 1.2 自洽核对（可复算）

- Coverage Index 的 **37 行 `source.heading` 全部在对应 spec 中逐条实存**（脚本按 `path`+`heading` 逐行 `includes` 比对，缺失 0 条）。
- 37 行的 `tasks` 取值集合 = `{2.1,2.2,2.3,2.4,2.5,2.6,4.2}`，全部在 tasks.md 中实存。
- 本报告引用的 spec 标题、任务编号、WP/TP 编号、文件路径均取自上述输入原文。
- §5 列出的既有测试文件与辅助函数（`TempDir`/`TempFile`/`collector`/`column_specs`/`OwnerNode`/`NodeLinkClient`）均在基线提交上实存（核对方式见 §5.5）。

---

## 2. 需求映射（R1–R37 → 稳定用例 ID）

用例 ID 形如 `SR-<R编号>-<序号>`；ID 一经分配不复用于无关场景。

| 行 | spec / 需求或场景 | 交付单元·WP | 用例 ID | 主落点（TP2 新增文件） | 异常/边界路径 |
| --- | --- | --- | --- | --- | --- |
| R1 | acp-wire-protocol `### Requirement: session/resume 的类型化解码与圆形保真` | U1·WP1 | SR-R1-1, SR-R1-2 | `crates/acp-protocol/tests/session_resume.rs` | ✅ SR-R1-2 |
| R2 | 同上 `#### Scenario: 正常解码并回写未知字段` | U1·WP1 | SR-R2-1 | 同上 | — |
| R3 | 同上 `#### Scenario: 缺少 required 字段被拒绝` | U1·WP1 | SR-R3-1, SR-R3-2 | 同上 | ✅ 本体即异常路径 |
| R4 | 同上 `#### Scenario: session/load 仍为显式不支持` | U1·WP1 | SR-R4-1 | 同上 | ✅ 本体即边界（回归） |
| R5 | local-agent-host `### Requirement: 会话创建时暴露 ACP 会话标识` | U1·WP5 | SR-R5-1, SR-R5-2 | `crates/agent-host/tests/resume.rs` | ✅ SR-R5-2 |
| R6 | 同上 `#### Scenario: 创建成功暴露 ACP 会话标识` | U1·WP5 | SR-R6-1 | 同上 | — |
| R7 | 同上 `#### Scenario: 未取得标识时不编造取值` | U1·WP5 | SR-R7-1, SR-R7-2 | 同上 + `crates/app/tests/session_resume_e2e.rs` | ✅ 本体即异常路径 |
| R8 | 同上 `### Requirement: 进程不在时的会话恢复（session/resume）` | U1·WP5 | SR-R8-1, SR-R8-2 | `crates/agent-host/tests/resume.rs` | ✅ SR-R8-2 |
| R9 | 同上 `#### Scenario: 能力已宣告时恢复到可交互` | U1·WP5 | SR-R9-1 | 同上 | — |
| R10 | 同上 `#### Scenario: 能力未宣告时显式不支持且不启动进程` | U1·WP5 | SR-R10-1, SR-R10-2 | 同上 | ✅ SR-R10-2（显式 `null` 边界） |
| R11 | 同上 `#### Scenario: Agent 拒绝恢复时明确失败` | U1·WP5 | SR-R11-1 | 同上 | ✅ 本体即异常路径 |
| R12 | 同上 `#### Scenario: 反复恢复不产生第二个端点` | U1·WP5 | SR-R12-1, SR-R12-2 | 同上 | ✅ SR-R12-2（残留绑定边界） |
| R13 | storage-schema-v2-migration `### Requirement: 版本常量与 migration 幂等` | U1·WP4 | SR-R13-1, SR-R13-2 | `crates/storage-sqlite/tests/resume_columns.rs` | ✅ SR-R13-2 |
| R14 | 同上 `#### Scenario: 连续两次打开 schema 文本不变` | U1·WP4 | SR-R14-1 | 同上 | — |
| R15 | 同上 `#### Scenario: 升级中途失败整体回滚` | U1·WP4 | SR-R15-1 | 同上 | ✅ 本体即异常路径 |
| R16 | 同上 `#### Scenario: v4 到 v5 升级保留既有会话行且新列为空` | U1·WP4 | SR-R16-1 | 同上 | ✅ 本体即边界（关键路径⑤） |
| R17 | 同上 `#### Scenario: 新列写入后可读回且不推导` | U1·WP4（写路径含 WP3） | SR-R17-1, SR-R17-2 | 同上 | ✅ SR-R17-2 |
| R18 | 同上 `#### Scenario: 升级库与新建库的 owned 列清单相等` | U1·WP4 | SR-R18-1 | 同上 | — |
| R19 | 同上 `#### Scenario: v3 到 v4 升级给既有节点行写空清单` | U1·WP4 | SR-R19-1 | 同上 | — |
| R20 | 同上 `#### Scenario: v2 到 v3 升级保留审计并扩展词表` | U1·WP4 | SR-R20-1 | 同上 | — |
| R21 | 同上 `### Requirement: 恢复所需列的读写边界` | U1·WP4（读路径含 WP3） | SR-R21-1, SR-R21-2 | 同上 | ✅ SR-R21-2（`NULL` 边界，关联关键路径④） |
| R22 | 同上 `#### Scenario: 恢复流程不覆写持久化取值` | U1·WP4+WP6 | SR-R22-1 | `crates/app/tests/session_resume_e2e.rs` | — |
| R23 | workspace-resolution `### Requirement: 恢复时的目录复校验` | U1·WP3 | SR-R23-1, SR-R23-2, SR-R23-3 | `crates/app/tests/session_resume_e2e.rs` | ✅ SR-R23-3 |
| R24 | 同上 `#### Scenario: 目录已被删除时返回不可用且不启动 Agent` | U1·WP3 | SR-R24-1, SR-R24-2 | 同上 | ✅ 本体即异常路径 |
| R25 | 同上 `#### Scenario: 规范化结果变化时拒绝恢复` | U1·WP3 | SR-R25-1, SR-R25-2 | 同上 | ✅ 本体即异常路径 |
| R26 | 同上 `#### Scenario: 目录仍然有效时使用持久化取值` | U1·WP3+WP5 | SR-R26-1, SR-R26-2 | 同上 + `crates/agent-host/tests/resume.rs` | — |
| R27 | node-link-owner-server `### Requirement: 命令授权、幂等与终态` | U1·WP2+WP3+WP6 | SR-R27-1, SR-R27-2, SR-R27-3 | `crates/node-link-protocol/tests/session_resume_command.rs` + `crates/app/tests/session_resume_e2e.rs` | ✅ SR-R27-2、SR-R27-3 |
| R28 | 同上 `#### Scenario: 相同 requestId 重试不重复派发` | U1·WP6 | SR-R28-1 | `crates/app/tests/session_resume_e2e.rs` | — |
| R29 | 同上 `#### Scenario: 同键不同语义被拒绝` | U1·WP6 | SR-R29-1, SR-R29-2 | 同上 | ✅ 本体即异常路径 |
| R30 | 同上 `#### Scenario: 越权命令被拒绝` | U1·WP6 | SR-R30-1 | 同上 | ✅ 本体即异常路径 |
| R31 | 同上 `#### Scenario: 越权恢复被拒绝且先于本机读取` | U1·WP3+WP6 | SR-R31-1, SR-R31-2 | 同上 | ✅ 本体即异常路径（关键路径②） |
| R32 | 同上 `#### Scenario: 崩溃窗口进入 uncertain` | U1·WP3+WP6 | SR-R32-1 | 同上 | ✅ 本体即异常路径（关键路径③） |
| R33 | 同上 `#### Scenario: 命令限流` | U1·WP6 | SR-R33-1 | 同上 | ✅ 本体即边界（限流阈值） |
| R34 | 同上 `### Requirement: session.resume 的 payload 与结果契约` | U1·WP2+WP6 | SR-R34-1, SR-R34-2 | 同上 | ✅ SR-R34-2 |
| R35 | 同上 `#### Scenario: 正常恢复并回传会话引用` | U1·WP5+WP6 | SR-R35-1 | 同上 | — |
| R36 | 同上 `#### Scenario: 携带字段被拒绝且不启动进程` | U1·WP2+WP6 | SR-R36-1, SR-R36-2 | 同上 | ✅ 本体即异常路径 |
| R37 | 同上 `#### Scenario: Agent 不支持恢复时终态失败` | U1·WP1–WP6 | SR-R37-1 | 同上 | ✅ 本体即异常路径 |

**统计**：37/37 行有 ≥1 个用例 ID（逐行计数：2+1+2+1+2+1+2+2+1+2+1+2+2+1+1+1+2+1+1+1+2+1+3+2+2+2+3+1+2+1+2+1+1+2+1+2+1）；8 个 `### Requirement:` 行（R1/R5/R8/R13/R21/R23/R27/R34）各有 ≥1 条异常或边界路径用例；共 **58** 条用例，Owner 均为 tester-A（TP2）、Reviewer 均为 reviewer-T2。

### 2.1 用例 Ownership and Cases（设计交付判据表）

`Executor` 是将来编写+执行的实例；`Reviewer` 是非作者的新独立实例（与 Author 不同）。

| 用例 ID（组） | 需求场景 | 入口 | 前置数据 | 步骤与断言（摘要，详见 §3） | 用例路径 | Executor | Reviewer |
| --- | --- | --- | --- | --- | --- | --- | --- |
| SR-R1-1/2, SR-R2-1, SR-R3-1/2, SR-R4-1 | R1–R4 | `acp_protocol::message` 的类型化 DTO + `RawDocument`/`Envelope` 纯函数 | 内联字节串（不新增 `fixtures/acp/v1/` 内容） | §3.1 | `crates/acp-protocol/tests/session_resume.rs`（新增） | tester-A（TP2） | reviewer-T2 |
| SR-R5-1/2, SR-R6-1, SR-R7-1, SR-R8-1/2, SR-R9-1, SR-R10-1/2, SR-R11-1, SR-R12-1/2, SR-R26-2 | R5–R12, R26 | `AgentHost` + fake ACP child（真实 stdio 子进程） | 独立 `CARGO_TARGET_DIR`；fake child 场景 + 心跳文件；tempdir | §3.2 | `crates/agent-host/tests/resume.rs`（新增，复用 `tests/support`） | tester-A（TP2） | reviewer-T2 |
| SR-R13-1/2, SR-R14-1, SR-R15-1, SR-R16-1, SR-R17-1/2, SR-R18-1, SR-R19-1, SR-R20-1, SR-R21-1/2 | R13–R21 | 真实 SQLite 临时库（`SqliteStore::open` + 原始 `sqlx` 连接） | 每个用例独立 tempdir（`0700`）；v4 形状由程序化回退构造 | §3.3 | `crates/storage-sqlite/tests/resume_columns.rs`（新增，复用 `tests/support`） | tester-A（TP2） | reviewer-T2 |
| SR-R7-2, SR-R22-1, SR-R23-1/2/3, SR-R24-1/2, SR-R25-1/2, SR-R26-1, SR-R27-3, SR-R28-1, SR-R29-1/2, SR-R30-1, SR-R31-1/2, SR-R32-1, SR-R33-1, SR-R34-1/2, SR-R35-1, SR-R36-1/2, SR-R37-1 | R7, R22–R26, R27, R28–R37 | 真实 Node Link 命令管线（真实 loopback listener + 真实 SQLite + 真实 `identity_auth::Authority`） | 独立 tempdir；`127.0.0.1:0` 内核分配端口；受控后端替身 | §3.4 | `crates/app/tests/session_resume_e2e.rs`（新增，复用 `tests/support`） | tester-A（TP2） | reviewer-T2 |
| SR-R27-1, SR-R27-2, SR-R34-2（wire 段） | R27, R34 | `node_link_protocol::command` 的解码/校验纯函数 + `compatibility/commands/v1/commands.json` | 静态合同文件（只读） | §3.5 | `crates/node-link-protocol/tests/session_resume_command.rs`（新增） | tester-A（TP2） | reviewer-T2 |
| 内联（不新增文件） | R1–R37 的单一实现缝 | 见 §3.6 | — | §3.6 | `crates/acp-protocol/src/**`、`crates/node-link-protocol/src/command.rs`、`crates/identity-auth/tests/authorization.rs`、`crates/core/src/**`、`crates/storage-sqlite/tests/migration.rs`、`crates/agent-host/src/bin/acpr-fake-acp-agent.rs`、`crates/server/**`、`crates/app/tests/support/owner.rs`、`crates/app/tests/support/nodelink.rs` | 各 WP 的实现者（coder-A…F） | reviewer-A…F |

> 说明：`SR-R27-1`/`SR-R27-2`/`SR-R34-2` 在两个边界各有一段断言（wire 解码/校验段 + 端到端段），它们是**同一用例 ID 的两段证据**，Owner/Reviewer 相同，不构成两份用例。

### 2.2 用例 Execution Waves

Main E2E 为 `not-applicable`（plan.md 的 `## Main E2E`），因此**没有 final-main E2E 执行波次**；上表的执行发生在 TP2（W5）与各 WP 的分支阶段，用例本身在 PV1（阶段 1/2）中被 `cargo test` 驱动。

| Wave | 用例集合 | 触发条件 | 绑定检查 | 说明 |
| --- | --- | --- | --- | --- |
| W1（设计） | 本报告全部 58 条（设计态） | 契约冻结 `specs/**@design-rev-4` | NOT_APPLICABLE | TP1：只产出设计；不产出可编译用例 |
| W3（分支，内联用例） | §3.6 的 WP1/WP2/WP3/WP5 内联项 | 各 WP 开工 | PV1（分支，crate 子集）+ PV2（WP1/WP2） | 实现者随代码同批交付 |
| W3–W4（分支，内联用例） | §3.6 的 WP4/WP6 内联项 | WP4/WP6 开工 | PV1（分支）+ PV2 | 含跨 crate 涟漪收口 |
| W5（用例编写+执行） | 全部 TP2 用例（§3.1–§3.5 共 58 条） | 六个 WP 全部进入已验收集成基线 | PV1（阶段 1：各自 crate；TP2 行要求 workspace 全绿） | 用例文件 `crates/**/tests/` 新增 |
| final-main | NOT_APPLICABLE | — | — | Main E2E = not-applicable（替代检查 C1=PV1 全量、C2=PV2） |

---

## 3. 用例详情

统一口径说明：

- 「落点」标注 `TP2` 表示用例由 TP2 写在**新增**文件里；标注 `WPn 内联` 表示按 plan.md 的 Shared File Ownership，该断言落在某 WP 拥有并已在写的既有文件/`src/**` 内联测试里，由该 WP 的实现者编写（TP2 不重复实现，但会在 §3 的对应用例中登记为「需与 WPn 对齐」）。
- 断言只绑定**可观察结果**：错误分类/错误码、返回类型与字段、持久化字节、进程是否启动/是否结束、文件字节、wire 报文原文。
- 不使用内部实现细节（私有函数名、内部字段顺序、日志文本）作为断言依据。
- 所有「进程是否启动」的断言都说明它观察什么（子进程心跳文件是否增长、假后端计数器、`--dump-requests` 文件内容）。

### 3.1 协议层（R1–R4）——`crates/acp-protocol/tests/session_resume.rs`（新增，TP2）

前置数据构造：**全部使用内联字节串**（`&str`/`&[u8]` 常量），不新增 `fixtures/acp/v1/` 下的文件、不改 `manifest.json`——这样避免与 WP1 对 `crates/acp-protocol/` 的整 crate 写范围冲突，也避免 `EXPECTED_CASES` 计数门禁的连带改动。

#### SR-R1-1 `session/resume` 的类型化编码/解码往返
- 来源：R1。场景：用类型化 DTO 构造请求 → 编码 → 再解码回 DTO。
- 步骤：① 用类型化请求 DTO 编码一条 `client→agent` 请求；② 解析产出的报文；③ 用响应 DTO 解码一条携带 `modes`/`configOptions`/`_meta` 的响应。
- 断言：报文 `method == "session/resume"`（与 `methods::find("session/resume").wire_name` 一致）、`params.sessionId` 与 `params.cwd` 等于输入**逐字节**；响应 DTO 的 `modes.currentModeId`、`configOptions` 逐项等于输入；两次编码同一输入产生相同字节（确定性）。
- 落点：TP2。

#### SR-R1-2 `additionalDirectories`/`mcpServers`/`_meta` 的容忍解码与「不得用默认值补齐」
- 来源：R1（边界）。场景：上游 schema 中这三者是可选（`x-deserialize-default-on-error` / `skip-invalid-items`）。
- 步骤：① 解码一个同时带 `additionalDirectories: []`、`mcpServers: []`、`_meta: {...}` 的请求；② 解码一个 `additionalDirectories: "not-an-array"`（上游允许按错误默认）的请求；③ 构造请求时不提供 `cwd`。
- 断言：① 三个可选字段被保留（`_meta` 原文可见）；② 不 panic、不得到补齐后的 `cwd`；③ 构造/解码请求时**必须失败**（不得有 `cwd` 的默认值），错误分类是可区分的 `InvalidField` 类，且不产生 DTO。
- 落点：TP2。

#### SR-R2-1 未知字段与 `_meta` 逐字节保真
- 来源：R2。场景：请求携带上游快照未定义字段与 `_meta`。
- 步骤：① 用内联字节（一行 JSON，含 `"futureField":{...}`、`"params._meta":{"example.dev/x":1}` 与一个超大整数 `9007199254740993`）`RawDocument::parse_bytes`；② 取类型化 `sessionId`/`cwd`；③ `document.encode()`。
- 断言：再编码字节与输入**逐字节相等**；`encode()` 文本中仍含 `futureField`、`9007199254740993`（不得经 `serde_json::Value` 往返——该反例断言已由 `tests/raw_fidelity.rs::byte_exact_round_trip_uses_raw_bytes_not_value_projection` 固定，本用例针对 `session/resume` 重述）。
- 落点：TP2。

#### SR-R3-1 缺少 `cwd` 被拒绝
- 来源：R3。步骤：解码 `{"method":"session/resume","params":{"sessionId":"acp-1"}}`。
- 断言：返回可区分错误（`AcpError::InvalidField{field:"cwd"|"params"}` 类），不发生任何下游调用（本断言是纯函数级，故无副作用可观察对象，错误分类即可区分性判据）。
- 落点：TP2。

#### SR-R3-2 `cwd` 类型不符 / `sessionId` 缺失被拒绝
- 来源：R3（边界）。步骤：① `cwd` 为数字；② `sessionId` 缺失；③ `cwd` 为 `null`。
- 断言：三种输入都失败、都不构造 DTO；错误分类与 SR-R3-1 同类（可区分于「未知方法」与「JSON 语法错误」两类）。
- 落点：TP2。

#### SR-R4-1 `session/load` 仍为显式不支持，`session/resume` 已实现
- 来源：R4（回归边界）。步骤：① `methods::status_of("session/load")`；② `methods::status_of("session/resume")`；③ 对一条 `session/load` 请求执行 `Envelope::classify` + `ensure_direction(ClientToAgent)`；④ 读 `METHODS` 表中两条方法的 `delivery`/`implemented`。
- 断言：`session/load` 仍为 `MethodStatus::NotImplemented`、`Delivery::PostMvp`、`implemented == false`，且分类报 `AcpError::Unsupported`；`session/resume` 为 `Implemented`、`Delivery::ConditionalMvp`；矩阵与 Rust 表的逐条相等由 `tests/matrix_tables.rs` 承担（不在本文件重复）。
- 落点：TP2（既有等价断言在 `tests/raw_fidelity.rs:128` 与 `tests/matrix_tables.rs`，TP2 只做补充声明，不修改这两个既有文件——它们属 WP1 的整 crate 写范围）。

### 3.2 本地后端层（R5–R12、R26）——`crates/agent-host/tests/resume.rs`（新增，TP2）

**硬前置（对 WP5 的依赖，见 §7-Q1）**：`crates/agent-host/src/bin/acpr-fake-acp-agent.rs` 当前**不认识** `session/resume`（`handle()` 无该分支 → 静默忽略，`resume` 会挂到超时）。为让本组用例可执行，fake child 必须新增：

1. `session/resume` 分支：校验 `params.sessionId`/`params.cwd` 形状（缺失或类型不符 → `exit(8)`，与既有 `session/new` 的「形状不合规即退出」同一手法），并按场景返回结果或 JSON-RPC 错误；
2. `--dump-requests <path>`：每收到一个请求就追加一行 `method`（含 `session/new`/`session/resume`/`session/prompt`），使「**发过/没发过哪个方法**」成为文件字节级可观察事实；
3. 场景 `resume-ok`（成功返回 `{}` 或带 `modes`）与 `resume-error`（返回 JSON-RPC error，例如 `{"code":-32000,"message":"unknown session"}`）；
4. 能力宣告经既有 `--capabilities <json>`：支持时 `{"sessionCapabilities":{"resume":{}}}`；不支持时 `{}` 或 `{"sessionCapabilities":{"resume":null}}`。

进程存活观察沿用既有手法：`--heartbeat-file <path>`（心跳文件停止增长 ⇒ 进程已结束），路径用 `support::TempFile` 守卫。

#### SR-R5-1 创建成功后暴露 ACP 会话标识
- 来源：R5。前置：`profile_with("agent-1", FAKE_AGENT, &["--scenario","normal"])`，`--dump-requests` 到 TempFile。
- 步骤：`host.create(&session, CreateSessionRequest::new(agent_ref(), Some(workspace), None, ResourceOrigin::Local), sink)`。
- 断言：`Ok(endpoint)`；`endpoint.reference()` 是 `SessionReference::Owned` 且 session id 等于传入值；`endpoint.agent_session_id() == Some(AgentSessionId::new("acp-session-1"))`——该值等于 fake child 在 `session/new` 响应里给出的 `sessionId`（`--dump-requests` 里恰好一行 `session/new`，证明标识来自协商结果而非本地编造）。
- 落点：TP2。

#### SR-R5-2 启动失败时不产生标识（异常路径）
- 来源：R5（边界）。前置：profile 的 `command` 指向不存在的可执行文件（复用既有 catalog/session 测试的同一手法）。
- 步骤：`host.create(...)`。
- 断言：`Err(PortError::...)`（后端不可用类，与既有启动失败分类一致）；无端点返回，因此 `agent_session_id()` 不可读——即「未取得标识时不产生取值」；`--dump-requests` 文件不存在或为空。
- 落点：TP2。

#### SR-R6-1 连续两次创建各自暴露自己的标识
- 来源：R6。步骤：对同一 `AgentHost` 用两个不同 `SessionId` 各 create 一次。
- 断言：两次的 `agent_session_id()` 分别为 `acp-session-1`、`acp-session-2`（fake child 的 `session_seq`），互不覆盖；两个端点的 `reference()` 分别指向各自的 core 会话。
- 落点：TP2。

#### SR-R7-1 `session/new` 失败/超时后不产生标识
- 来源：R7（异常路径）。前置：① `--scenario slow-initialize`（超过固定启动超时）；② fake child 在 `session/new` 上退出（需 WP5 的场景支持，例如 `--scenario session-new-error`，见 §7-Q1）。
- 步骤：各调用一次 `host.create(...)`。
- 断言：两次都 `Err`；没有端点、没有标识可读（「不编造占位值」的可观察形式）；`--dump-requests` 证明未发生任何后续请求。
- 落点：TP2（agent-host 侧）；与之配对的 core 侧后果见 SR-R7-2。

#### SR-R7-2 创建未取得标识的会话不可恢复（跨层，落 app 层）
- 来源：R7 末句「该会话不被当作可恢复会话」。步骤（app 层，见 §3.4）：让受控后端的 `create` 返回失败 → 该会话行存在但 `agent_session_id IS NULL`（用原始 `sqlx` 只读断言）→ 对该会话提交 `session.resume`。
- 断言：`command.terminal(status="failed")` 且错误码为 `nodelink.command.unsupported`；不产生新会话、不调用后端 `resume`（计数器为 0）。
- 落点：TP2（`crates/app/tests/session_resume_e2e.rs`）。

#### SR-R8-1 宣告能力时的恢复入口端到端可用
- 来源：R8。前置：`--capabilities '{"sessionCapabilities":{"resume":{}}}'`、`--scenario resume-ok`、`--dump-requests`、`--heartbeat-file`；先用同一 profile 创建一个会话并取其 `agent_session_id()`（模拟「曾被创建过、进程已不在」——测试用**新的** `AgentHost` 实例，避免复用进程内绑定）。
- 步骤：`factory.resume(&session, ResumeSessionRequest::new(agent_ref(), AgentSessionId("acp-session-1"), workspace), sink)`。
- 断言：`Ok(endpoint)`；`--dump-requests` 里出现且仅出现一行 `session/resume`（在 `initialize` 之后、`session/new` 之前无调用）；该行 `params.sessionId == "acp-session-1"`、`params.cwd` 等于传入 workspace 的 `canonical_path()` **逐字节**，且不含客户端可注入的其它目录键；`endpoint.agent_session_id() == Some("acp-session-1")`（**响应里没有 sessionId**，因此该断言同时证明标识来自持久化输入而非响应编造）；心跳文件在恢复后仍在增长（进程活着，未在成功后立刻清理）。
- 落点：TP2。

#### SR-R8-2 Agent 拒绝恢复时明确失败且不留绑定（异常路径）
- 来源：R8 末段。前置：能力已宣告 + `--scenario resume-error` + `--dump-requests` + `--heartbeat-file`。
- 步骤：调用 `resume`；随后调用 `host.open(&SessionReference::Owned(OwnedSessionRef::new(session)))`。
- 断言：`resume` 返回 `Err`（明确错误，非 Ok）；`open` 仍失败（既有 `SessionClosed` 语义）⇒ 该会话**未**进入可交互状态；`--dump-requests` 里**没有** `session/new` 行 ⇒ 未静默改走新建会话；心跳文件在断言窗口内停止增长 ⇒ 失败路径没有遗留子进程。
- 落点：TP2。

#### SR-R9-1 恢复后的端点可接受 turn 且只派发一次
- 来源：R9。前置：能力已宣告 + 恢复用 `--scenario resume-ok`，并让 fake child 在 `session/prompt` 上走 `chunked-updates`（需 WP5 允许「resume 场景后再按 prompt 场景分支」，或用一个 `--prompt-scenario` 覆写；若不可行则以 `resume-ok` 的默认 prompt 分支为准，见 §7-Q1）。
- 步骤：`endpoint.prompt(prompt("你好"), at)`；用 `Collector` 等 `turn.completed`；在 turn 未完成时再发一次 `prompt`（`--scenario no-response` 变体不可与前者同时生效，因此该子断言以「并发第二个 prompt 在 core 串行门内被拒/排队」由 SR-R32 与既有 core 用例承担，此处只断言「同一时刻该会话只有一个在途 prompt」的 fake 侧观察）。
- 断言：`Collector` 收到 `agent.message.delta` 与 `turn.completed`（事件类型名精确匹配）；`--dump-requests` 中该会话的 `session/prompt` 行数等于已释放的 prompt 次数（不重复派发）；恢复得到的端点 `reference()` 仍是同一 owned 会话。
- 落点：TP2。

#### SR-R10-1 能力未宣告时显式不支持且不发送请求（关键路径①）
- 来源：R10。前置：`--capabilities '{}'`（不宣告）+ `--dump-requests` + `--heartbeat-file`。
- 步骤：调用 `resume`。
- 断言：① 返回 `Err(PortError::Unavailable(UnsupportedOperation 一类的新取值))`（design D3 的新 `UnavailableKind` 取值；与 SR-R23/SR-R24 的「服务端不可用」**不同**取值 ⇒ 两类错误可区分）；② `--dump-requests` 文件里**没有** `session/resume` 行（报告派发之前就门控）；③ 心跳文件停止增长、子进程结束（`resume` 返回前已清理）；④ 会话没有新绑定（`open` 仍 `SessionClosed`）。
- 口径说明（与 plan.md 的 `## Risks / Trade-offs` 第 2 条一致）：能力只能在 `initialize` 之后得知，因此「不启动子进程」的**可验收形式**是「不发送 `session/resume` + 进程已被清理 + 无残留句柄」。字面「完全不 spawn」在 design D4 下不可满足，见 §7-Q2。
- 落点：TP2。

#### SR-R10-2 `resume: null` 与省略等价（边界）
- 来源：R10（边界）。前置：`--capabilities '{"sessionCapabilities":{"resume":null}}'`。
- 步骤：调用 `resume`；再以 `--capabilities '{}'` 重复一次。
- 断言：两次的错误取值、错误可区分性、`--dump-requests` 无 `session/resume` 三项**完全相同**（`supports_session_resume()` 对 `null` 与省略同判不支持）。
- 落点：TP2。

#### SR-R11-1 Agent 返回错误时不静默降级
- 来源：R11。前置：能力已宣告 + `--scenario resume-error` + `--dump-requests` + `--heartbeat-file`。
- 步骤：调用 `resume`；随后尝试 `open`。
- 断言：`Err`；`--dump-requests` 中**无** `session/new`（不降级为新建）、`session/resume` 恰好一行（不重试）；心跳停止增长；`open` 失败。与 SR-R8-2 的差异：SR-R8-2 断言「不留绑定」，本用例额外断言「不产生新会话标识」——即 `--dump-requests` 无 `session/new`（这是 R11 的『不产生新会话标识』的可观察形式）。
- 落点：TP2。

#### SR-R12-1 重复恢复不产生第二个可派发端点（关键路径⑥）
- 来源：R12。前置：能力已宣告 + `resume-ok`；`--dump-requests`；心跳文件区分两个子进程（每进程一个心跳文件路径，或同一文件 + 断言最终只有一个写入者存活）。
- 步骤：① `resume` 得到 `e1`；② 立刻再 `resume` 同一会话得 `e2`（或 `Err`）；③ 通过 `e1` 发 `prompt`，再通过 `e2`（若存在）发 `prompt`；④ 等待两个子进程状态。
- 断言：最多一个端点能成功接受并完成一个 turn（另一个返回错误或不存在）；`--dump-requests` 中 `session/prompt` 行总数 ≤ 1；结束时只有一个心跳文件仍在增长（`assert` 后不需等待，直接断言「停止增长」在 §5.4 的时序窗口内成立）；同一 core 会话的 `reference()` 只有一个（比较两个端点的 `reference()` 或断言 `e1` 的调用返回错误）。
- 落点：TP2。

#### SR-R12-2 旧绑定仍然活着时再次恢复（边界）
- 来源：R12（边界，「残留旧绑定尚未让出」）。前置：先 `create` 得到一个**活跃**端点（进程在跑），再对同一会话 `resume`。
- 步骤：① `create` → `e_old`（进程 P1，心跳文件 h1）；② `resume`（同一会话，新进程 P2，心跳文件 h2）；③ 分别通过 `e_old`、`e_new` 发 `prompt`；④ 检查 h1 是否最终停止增长。
- 断言：**恰好一个**端点可派发 turn（`assert_eq!(dispatchable_count, 1)`，不预设是哪一个实现选择）；不存在两个子进程同时接受 prompt（`--dump-requests` 的 `session/prompt` 行总数 ≤ 1）；`session/resume` 恰好一行 ⇒ 没有额外进程被拉起。
- 落点：TP2。

#### SR-R26-2 发送给 Agent 的 `cwd` 就是持久化取值（agent-host 侧）
- 来源：R26（WP5 视角）。前置：能力已宣告 + `resume-ok` + `--dump-requests`；传入的 `ResolvedWorkspace` 的 `canonical_path()` 与任何本机别名的解析结果**故意不同**（用一个存在的 tempdir 作为持久化值，另用一个指向别处的 alias 记录）。
- 步骤：`resume`；读 `--dump-requests` 记录的请求行（或由 fake child 把 `session/resume` 的 `params` 全文写入 `--dump-requests` 的目标文件）。 
- 断言：`params.cwd` == 传入 workspace 的 `canonical_path()` 逐字节；不等于别名解析出的路径（证明没有重新解析）。
- 落点：TP2。

### 3.3 存储层（R13–R21）——`crates/storage-sqlite/tests/resume_columns.rs`（新增，TP2）

前置数据构造（R16 的 v4 形状）：沿用既有 `tests/migration.rs::v3_database_upgrades_to_v4_by_appending_the_export_id_column_only` 的手法——先用 `SqliteStore::open` 建当前版本库，再用原始 `raw_write_pool` 连接 `ALTER TABLE owned_session DROP COLUMN agent_session_id` / `DROP COLUMN workspace_cwd`、`PRAGMA user_version = 4`、`UPDATE meta SET value='4' WHERE key='owned_schema_version'`、`PRAGMA wal_checkpoint(TRUNCATE)`（**不新增 `fixtures/storage/v4/` 夹具**，避免与 WP4 的夹具族所有权纠缠）。所有用例使用 `support::temp_dir`（`0700`，`Drop` 自清理）。

#### SR-R13-1 v5 版本常量与两族键
- 来源：R13。步骤：`SqliteStore::open` 一个空 tempdir；`store.metadata()`；原始连接读 `PRAGMA user_version` 与 `meta` 两键。
- 断言：`FILE_FORMAT_VERSION == 5`、`OWNED_SCHEMA_VERSION == 5`、`IMPORTED_SCHEMA_VERSION == 3`；库内三者分别等于 5/5/3；`owned_session` 的两列在 DDL 文本末尾（追加，不重建）。
- 落点：TP2（数字断言）；WP4 需同步修改既有 `tests/migration.rs` 的 `4`→`5` 字面量（既有文件属 WP4，见 §3.6）。

#### SR-R13-2 过新版本拒绝打开且不写任何行（边界）
- 来源：R13（边界）。步骤：复制一个刚建好的库到 tempdir，把 `user_version` 顶到 `FILE_FORMAT_VERSION + 1`，重新 `SqliteStore::open`。
- 断言：`Err`（过新拒绝，与既有 `too_new_database_is_rejected_without_writing_rows` 同分类）；所有既有表的行快照与打开前逐条相等（不写任何行）。
- 落点：TP2（如与既有 `migration.rs` 的等价断言重复，按 tester 指令 1 登记为**复用**并在报告中标注原用例名，不复制实现）。

#### SR-R14-1 连续两次打开 schema 文本不变
- 来源：R14。步骤：打开/关闭两次；每次读 `sqlite_master` 全量 SQL、`meta` 全量键值、`PRAGMA user_version`。
- 断言：两次之间 `sqlite_master` 逐字节相同、`meta` 逐条相同、`user_version` 不变；`owned_session` 的 DDL 含两列且顺序为 `…, agent_session_id, workspace_cwd` 结尾。
- 落点：TP2（复核既有 `current_version_database_is_untouched_by_two_consecutive_starts` 的 v5 版本；等价的在既有文件内的断言登记为复用）。

#### SR-R15-1 升级中途失败整体回滚
- 来源：R15。步骤（两条注入，均按既有「注入冲突对象 → 打开失败 → 去注入 → 重开成功」手法）：
  1. 更早版本段：copy `fixtures/storage/v2/from-v1.sqlite3` → 注入 `CREATE TABLE imported_import_v2 (...)` → 打开 → 断言失败、`user_version` 保持 1、两族键保持 1、无半建表；去注入后重开 → 升级到 5。
  2. v5 段：构造 v4 形状库（见本节前置），注入一个与目标列同名的既有列（`ALTER TABLE owned_session ADD COLUMN agent_session_id INTEGER`，即类型不同）→ 打开。
- 断言（不变量式，避免绑定实现的守卫策略）：打开后**不存在半升级形态**——`user_version != 5` ⇒ 两列都不存在或都不被 v5 段创建；`user_version == 5` ⇒ 两列的列规格（名称/顺序/类型/NOT NULL/默认值）等于新建库；无论成败，任一 `owned_session` 既有行的其它列字节不变；失败后重开（去掉注入）可继续升级或稳定打开（可重复打开）。
- 落点：TP2（是否需要 WP4 提供一个**确定性**的 v5 段故障注入钩子，见 §7-Q3）。

#### SR-R16-1 v4→v5 保留既有行且新列为 `NULL`（关键路径⑤）
- 来源：R16。前置：v4 形状库 + 一行完整 `owned_session`（每个其它列写入已知字节，`title`/`mode`/`closed_at` 等含 `NULL` 与非 `NULL` 两种取值）。
- 步骤：打开 → 读该行全部列（`SELECT quote(col) …` 逐列或用 `column_names` + 动态拼接）。
- 断言：其余每列与升级前逐字节相同；`agent_session_id IS NULL` 且 `workspace_cwd IS NULL`（**断言 `IS NULL`，显式排除空串**：`assert!(quote(col).is_null())` 或 SQL `typeof(col)='null'`）；两列不是 `''`、不是别名、不是占位路径。
- 落点：TP2（与关键路径④的「不可恢复」判定在 SR-R21-2 与 SR-R37-1 交叉验证）。

#### SR-R17-1 新列写入后按字节读回，未写入的行保持 `NULL`
- 来源：R17。前置：经 core 的创建提交（`OwnedCommit{state: Create(NewSession{…})}`，携带两列取值）写入一行；另一行不带这两列。
- 步骤：关闭 → 重新打开 → 分别读两行两列。
- 断言：写入行的两列与写入值逐字节相等；另一行为 `NULL`（既非空串也非推导值）。
- 落点：TP2。

#### SR-R17-2 空串/非法取值不得成为「可恢复」取值（边界）
- 来源：R17（边界）。步骤：① 值对象层构造 `AgentSessionId::new("")`、含 NUL 的取值、超长取值；② 尝试以空串写入 `workspace_cwd`。
- 断言：① 值对象构造返回 `Err(InvalidValue...)`（类型层拒绝，三例都不产生取值）；② 存储写入路径要么拒绝、要么读回仍是 `NULL`——即不存在「空串被当作可恢复数据」的可观察结果（若 WP3/WP4 选择在存储层拒绝，则断言错误分类；若在类型层已不可能，则断言「无法构造出该写入」并记录为类型层覆盖）。
- 落点：TP2（值对象的那半条与 WP3 的内联值对象用例重叠，登记为对齐而非重复）。
- 说明：此项**不**断言具体错误类型名（WP3 决定），只断言「不存在可恢复的空串行」这一可观察不变量。

#### SR-R18-1 升级库与新建库的 owned 家族列清单相等
- 来源：R18。步骤：对 v4 升级库与新建库逐表 `support::column_specs`；对 `owned_session` 单独取末两项。
- 断言：所有 owned 表逐项相等；`owned_session` 末两项为 `("agent_session_id", TEXT, notnull=false, default=None)` 与 `("workspace_cwd", TEXT, false, None)`，且顺序固定（追加序）。
- 落点：TP2。

#### SR-R19-1 v3→v5 给既有节点行写空清单
- 来源：R19。步骤：构造 v3 形状库（按既有 `v3_database_upgrades_to_v4_…` 手法回退 `owned_node.export_ids_json` 与版本）+ 一行已配对节点行 → 打开到 v5。
- 断言：`export_ids_json == '[]'`；该行其它列逐字节不变；`owned_node` 其余 DDL 不变（未重跑 12-step 重建）。
- 落点：TP2（等价断言已在既有 `migration.rs`，登记复用 + 目标版本改为 5）。

#### SR-R20-1 v2→v5 保留审计并保持词表检查
- 来源：R20。步骤：用 `fixtures/storage/v2/empty.sqlite3` 造含审计行的 v2 库（既有手法）→ 打开到 v5。
- 断言：全部审计行与 `audit_id`/序列原样保留；可写入 `actor_kind='pairing_claimant'`、`action='node.authenticated'`/`'node.auth_failed'`；词表外取值仍被 CHECK 拒绝；`owned_command.actor_kind` 仍是三值 CHECK。
- 落点：TP2（复核既有 `v2_fixture_upgrades_to_v3_and_widens_only_the_audit_checks` 在 v5 目标上仍成立）。

#### SR-R21-1 恢复流程不写这两列（字节比对）
- 来源：R21。前置：一行已带两列取值的会话（经创建流程写入）。
- 步骤：① 记录 `SELECT quote(agent_session_id), quote(workspace_cwd) FROM owned_session WHERE session_id = ?`；② 走一次**成功**的恢复；③ 再走一次**失败**的恢复（例如能力未宣告的后端）；④ 每次之后重新读同样的 `quote()`。
- 断言：两次恢复之后四个字节串与恢复前逐一相等（成功与失败路径都不改写）；`version`（会话版本）在恢复成功时按状态迁移规则变化、在恢复失败时不变（用于证明「状态变了但恢复列没变」）。
- 落点：TP2（WP3/WP4 的读路径是它的被观察对象）。

#### SR-R21-2 `NULL` 的读回语义是「不可恢复」而非「损坏」（边界，关联关键路径④）
- 来源：R21 + R13 的 `NULL` 语义。前置：用原始 SQL 把某会话两列置为 `NULL`。
- 步骤：① 直接读该行（`SessionStore::load`/恢复读取路径）；② 对该会话执行恢复用例/命令。
- 断言：① 读取返回 `Some`（不是 `NotFound`、不是 `Corrupt`），两列读取结果为 `None`；② 恢复以「不可用/不支持」类结果失败（R19 里 node-link 层表现为 `nodelink.command.unsupported`），不启动后端、不写入任何目录取值。
- 落点：TP2（存储层断言在 `resume_columns.rs`，端到端断言在 `session_resume_e2e.rs` 的 SR-R37-1/§3.4）。

### 3.4 真实 Node Link 命令管线（R7、R22–R26、R27、R28–R37）——`crates/app/tests/session_resume_e2e.rs`（新增，TP2）

**入口（真实，不是 mock 绕过）**：与既有 `crates/app/tests/node_link_e2e.rs` 同源——`support::OwnerNode`（真实 SQLite、真实 `identity_auth::Authority`、真实 `transport::net` listener、`app::daemon::register_node_link_paths` 的同一批装配点）+ `support::nodelink::NodeLinkClient`（真实 WSS/loopback 报文与握手）。仅两处测试替身（与既有用例同源，已在 `support/owner.rs` 文档注释中声明）：脚本化 Agent 后端、可控失败的 `SessionStore` 装饰器。

新增/扩展的测试支撑（登记范围见 §7-Q4）：
- `ScriptedBackends::resume`（WP6 实现，属 `crates/app/tests/support/owner.rs`，Shared File Ownership 已登记 WP6 → TP2）；
- TP2 在该文件内追加的**观察点**：`resume` 调用计数、最近一次收到的 `ResumeSessionRequest`（用于断言 `cwd`/`agent_session_id` 取值）、可配置的 `resume` 返回结果（成功 / 「后端不支持」/ 其他错误）；
- 前置数据构造：`owner.data_dir()` 下的真实库文件用 `sqlx` 原始连接做只读断言；必要时（SR-R25-1）直接 `UPDATE owned_session SET workspace_cwd = <非规范化绝对路径>` 造前置值。

#### SR-R22-1 成功与失败恢复都不覆写两列
- 来源：R22。步骤：`session.create` → 读 `quote()` 两列 → `session.resume`（成功）→ 再读 → `session.resume`（后端失败）→ 再读。
- 断言：两次恢复后两列 `quote()` 字节与创建后完全相同；`remoteSessionRef` 仍指向同一会话。
- 落点：TP2（与 SR-R21-1 同口径、不同层：这里走真实 wire 管线）。

#### SR-R23-1 复校验通过并使用持久化取值（别名指向别处）
- 来源：R23。前置：① `workspace.select` 一个真实 tempdir 作为 alias `demo` → `session.create`（持久化该目录）；② 再用 `workspace.select` 把 **同一 alias** 指向另一个真实目录；③ 让受控后端表现为「该会话的进程已不在」（脚本后端不保留可用端口；`resume` 路径不依赖既有绑定，因此这步只需记录「既有端点不再可用」作为对照）。
- 步骤：`session.resume`（payload `{}`）。
- 断言：`command.terminal` 为 `completed`（或按实现的成功状态）且后端收到的 `ResumeSessionRequest.workspace.canonical_path()` == 创建时持久化的路径（**不等于**别名现在指向的目录）⇒ 证明没有按别名重新解析；两列不被改写。
- 落点：TP2。

#### SR-R23-2 每次恢复都重新复校验
- 来源：R23（「它曾经合法」不是跳过理由）。步骤：① 第一次 `resume` 成功；② 删除该目录（或 `chmod 000`，unix）；③ 第二次 `resume`（同一进程、同一 store，不重启）。
- 断言：第二次 `terminal(status="failed")` 且错误码 `nodelink.internal.unavailable`（服务端不可用类）；后端 `resume` 计数仍为 1（第二次未被调用）。
- 落点：TP2。

#### SR-R23-3 首次恢复即失败时不回退到「最接近」的目录（边界）
- 来源：R23。前置：先删除持久化目录，再在同名路径上新建一个**不同**的目录（或把 alias 指向它）。
- 步骤：`session.resume`。
- 断言：`terminal(status="failed")` + `nodelink.internal.unavailable`；后端 `resume` 计数 0；两列字节不变（没有写入新解析出的路径）。
- 落点：TP2。

#### SR-R24-1 目录被删除 → 不可用且不启动 Agent
- 来源：R24。步骤：`session.create` → 删除持久化目录 → `session.resume`。
- 断言：`command.terminal(status="failed")`，错误码 `nodelink.internal.unavailable`（与 SR-R31 的 `nodelink.export.not_granted`、与 R10/R37 的 `nodelink.command.unsupported` 三者互不相同 ⇒ 分类型可区分性有证据）；后端 `resume` 计数 0；没有新会话行（会话总数不变）；两列不变。
- 落点：TP2。

#### SR-R24-2 目录不可访问（权限/形态）同样不可用（边界）
- 来源：R24（「或不可访问」）。步骤：`#[cfg(unix)]`：把持久化目录 `chmod 000`（若以 root 运行则改为把目录替换为**普通文件**，覆盖「是目录」判定）；非 unix：把目录替换为同名普通文件。
- 断言：同 SR-R24-1（不可用类错误、后端计数 0、不写目录取值）。平台受限分支在报告中标注实际执行的平台（Windows 开发机不跑 `#[cfg(unix)]` 变体，见 §5.6）。
- 落点：TP2。

#### SR-R25-1 规范化结果与持久化取值不同 → 拒绝（平台无关）
- 来源：R25。前置：用原始 `sqlx` 把 `owned_session.workspace_cwd` 改写为一个**仍然存在**、但**未被规范化**的绝对路径（例如在真实目录后追加 `/./` 或双重分隔符；`ResolvedWorkspace` 只禁止 `..` 与相对路径，允许 `.` 组件）。
- 步骤：`session.resume`。
- 断言：`terminal(status="failed")` + `nodelink.internal.unavailable`；后端 `resume` 计数 0；**持久化取值未被改写**（仍是那个未规范化字符串，证明「不使用新解析出的路径」）；fake 后端若被调用则其收到的路径必须等于持久化值（反向守卫）。
- 落点：TP2。

#### SR-R25-2 符号链接指向别处 → 拒绝（`#[cfg(unix)]`）
- 来源：R25。前置：创建 `real-a` 与 `real-b`；`session.create` 绑定 `link → real-a`（持久化 canonicalize 后的 `real-a`）；把 `link` 重指到 `real-b`。
- 步骤：`session.resume`。
- 断言：`terminal(status="failed")` + `nodelink.internal.unavailable`；后端计数 0。平台受限：Windows 开发机不执行（改为 BLOCKED 记录或跳过并在报告中说明）。
- 落点：TP2。

#### SR-R26-1 目录有效时按持久化取值发送
- 来源：R26。前置：同 SR-R23-1（alias 指向别处）。
- 步骤：`session.resume`（成功）。
- 断言：后端收到的 `ResumeSessionRequest` 中 `workspace.canonical_path()` == 创建时持久化值（逐字节），且 `agent_session_id` == 持久化值；`terminal.result.remoteSessionRef` 指向该会话。
- 落点：TP2。

#### SR-R27-1 命令在 wire 上被识别为 mutation 且授权取 `grant.remote-work`
- 来源：R27。步骤：解码一条 `command.submit` 报文（`command == "session.resume"`、`payload == {}`、`expectedVersion == null`、`sessionRef`/`attachmentId`/`attachmentGeneration` 非 `null`）。
- 断言：解码为 `CommandPayload` 的 `session.resume` 变体；`command_name()`（Rust 侧）与 `compatibility/commands/v1/commands.json` 的 `session.resume` 行取值一致（`kind == "mutation"`、`grant == "grant.remote-work"`、`pack == null`、`transport` 含 `node_link`、`delivery == "conditional_mvp"`）；`required_grant` 的 Rust 镜像对该命令返回 `Some("grant.remote-work")`。
- 落点：TP2（`crates/node-link-protocol/tests/session_resume_command.rs`）；`crates/core/src/broker.rs` 的镜像臂由 WP3 内联断言（§3.6）。

#### SR-R27-2 `expectedVersion` 非 `null` / `sessionRef` 为 `null` 被拒（边界）
- 来源：R27（边界）。步骤：构造两种非法 `command.submit` 报文。
- 断言：都在解码/校验边界被拒（`expectedVersion` 必须 `null`、session-scoped 命令必须带非 `null` `sessionRef` 与 `attachmentId`/`attachmentGeneration`）；不产生副作用（纯函数级断言 + 在 §3.4 的端到端版本中再断言不使用户会话变化）。
- 落点：TP2。

#### SR-R27-3 终态形状（`accepted` 无 result，`completed` 带结果，`failed` 带错误）
- 来源：R27。步骤：在真实管线里各做一次成功与一次失败提交。
- 断言：`command.accepted.acceptedAt` 非 `null` 且 `result == null`；`command.terminal` 带提交时的 `command` 名；`completed` 的 `terminal.result` 是 `SessionResumeResult`（有 `remoteSessionRef`、`sessionMeta`），不是 `{}`；`failed` 的 `terminal.error.code` 非空且按场景等于 `nodelink.command.unsupported`/`nodelink.internal.unavailable`。
- 落点：TP2。

#### SR-R28-1 相同 `requestId` 重试不重复派发
- 来源：R28。前置：成功路径；后端 `resume` 计数器。
- 步骤：`session.resume`（requestId = R）→ 等终态 → 用**完全相同**的 body 再提交一次 → 通过 `command.status` 重查。
- 断言：第二次返回与首次相同的终态（`terminalEventId` 相等或结果 `remoteSessionRef` 相等）；后端 `resume` 计数 == 1；会话/端点没有第二个绑定（`session.prompt` 只派发一次）。
- 落点：TP2。

#### SR-R29-1 同键不同命令被拒
- 来源：R29。步骤：先用 requestId = R 提交 `session.resume`，再用同一 R 提交 `session.prompt`。
- 断言：第二次 `command.rejected` + `nodelink.command.idempotency_conflict`；后端 `resume` 计数与 prompt 派发次数都不变（无第二次副作用）。
- 落点：TP2。

#### SR-R29-2 同键不同语义（目标会话不同）被拒（边界）
- 来源：R29（边界「`sessionRef` … 语义不同」）。步骤：对两条不同会话使用同一 requestId 提交 `session.resume`。
- 断言：第二次 `command.rejected` + `nodelink.command.idempotency_conflict`；第二条会话未被恢复（其状态与两列不变）。
- 落点：TP2。

#### SR-R30-1 越权命令被拒（`grant.observe` 下的 `session.prompt`）
- 来源：R30。前置：一个只授予 `grant.observe` 的配对节点（既有 `Chain` 手法：Export scopes 与 confirm grants 只给观察）。
- 步骤：提交 `session.prompt`。
- 断言：`command.rejected` + `nodelink.export.not_granted`；会话状态不变（`session.list`/`command.status` 可核对）；**审计有该次拒绝的记录**（经本地管理 `audit.export` 读取，断言本次 `requestId` 的拒绝条目存在且 `outcome` 为拒绝）。
- 落点：TP2（若与既有 `node_link_e2e.rs` 的等价断言重复，登记复用并注明原用例）。

#### SR-R31-1 越权恢复先于本机读取（关键路径②）
- 来源：R31。前置：① `grant.observe` 节点；② 目标会话的持久化 `workspace_cwd` **指向一个已被删除的目录**（构造方式：创建后删除目录，或 SQL 改写到一个不存在的绝对路径）。
- 步骤：提交 `session.resume`。
- 断言：`command.rejected` + `nodelink.export.not_granted`（**不是** `nodelink.internal.unavailable`）——若实现先读会话/先校验目录，错误码必然不同，因此该断言直接判定「授权先于本机读取与文件系统访问」；后端 `resume` 计数 0；无新会话行。
- 落点：TP2。

#### SR-R31-2 不因会话是否存在而可区分（边界）
- 来源：R31（「响应不因会话是否存在而不同」）。步骤：同一 observe 节点分别提交指向① 存在且可见的会话、② 不存在的 `sessionRef` 的 `session.resume`。
- 断言：两次的拒绝消息类型与错误码**完全相同**、`details` 不泄露目标会话存在性（不出现「not found」与「not granted」的差别），且两次都不产生后端调用与文件系统迹象（后端计数 0）。
- 落点：TP2。

#### SR-R32-1 崩溃窗口进入 `uncertain`（关键路径③）
- 来源：R32。前置：`OwnerNode::fail_commits_with(Some(<仅在终态视图出现的标记>))`；成功路径的后端。
- 步骤：① 打开故障注入；② 提交 `session.resume`；③ 断言命令停在非终态（回复为 `command.accepted`、`result == null`，且 `owner.rejected_commits` 增长，证明注入命中）；④ 调用**组合根在启动时调用的同一入口** `core.recover_unsettled(&Actor::LocalCli, ReplayLimit::new(16))`；⑤ 用 `command.status` 重查同一 `requestId`。
- 断言：重查返回 `command.terminal`，`status == "uncertain"`，带结构化 `error`（不猜成功/失败）；后端 `resume` 计数仍为 1（不自动重试副作用）；会话两列不变；不存在第二个端点/会话。
- 口径与限定（写入本报告的风险项）：本用例在**同一进程内**触发启动恢复路径（调用的是组合根使用的同一函数），不是真实的进程重启；真实进程级重启（`support::Daemon` + 同一 `data_dir` 重启）作为可选强化，见 §7-Q5。
- 落点：TP2（`crates/app/tests/session_resume_e2e.rs`）。**TP2 前置核实项**：确认 `flaky` store 的 marker 只命中终态提交的事件视图（accepted 路径若也含 `requestId` 文本，则需改用「先放行 accepted、再打开注入」的时序或请 WP6 提供一个受控挂钩）。

#### SR-R33-1 命令限流仍覆盖新命令（边界）
- 来源：R33。步骤：单连接在一分钟内提交 > 120 条命令（混入 `session.resume` 与查询命令）。
- 断言：超限命令收到 `link.error`，错误码 `nodelink.resource.rate_limited`、`retryable == true`（`details.retryAfterMs` 存在时按既有断言核对其类型）；持续超限时连接以 `4429` 关闭；`session.resume` 的越限不产生额外后端调用。
- 落点：TP2（等价既有断言在 `crates/server/src/node_link/command/tests.rs`；在此登记为目标版本上的复核/复用）。

#### SR-R34-1 成功时结果是 `SessionResumeResult` 且回传会话引用
- 来源：R34。步骤：成功恢复后读 `command.terminal.result`。
- 断言：`result` 是 object 且含 `remoteSessionRef` 与 `sessionMeta`（与创建时的 `remoteSessionRef`/`sessionMeta` 形状与取值一致）；`payload` 为 `{}` 时接受（不因空 payload 被拒）；`accepted.result == null`。
- 落点：TP2。

#### SR-R34-2 `expectedVersion` 非 `null` 与非法 `sessionRef` 组合被拒（边界）
- 来源：R34（「MUST 为 `null`」）。步骤：提交 `expectedVersion` 非 `null` 的 `session.resume`。
- 断言：`command.rejected`（schema/校验类，`nodelink` 既有错误码之一，不新增码）；后端 `resume` 计数 0；会话不变。
- 落点：TP2（wire 级在 `session_resume_command.rs`，端到端在 `session_resume_e2e.rs`）。

#### SR-R35-1 正常恢复并回传会话引用（端到端）
- 来源：R35。步骤：配对 + 握手 + `catalog.subscribe` → `session.create`（拿 `remoteSessionRef` 与 `sessionMeta`）→ 重新握手（或按受控后端让旧绑定不再可用）→ `session.resume`（payload `{}`）→ `session.prompt`。
- 断言：先收到 `command.accepted`（`result == null`）；随后 `command.terminal(status="completed")`，`terminal.result.remoteSessionRef` 与创建时一致、`sessionMeta` 字段齐全；随后 `session.prompt` 被接受并产生 `session.create` 时的同一批事件（`resource.event`/`turn.completed` 按既有断言口径），证明会话真的可继续。
- 落点：TP2。

#### SR-R36-1 `payload` 携带 `cwd` 被拒且不启动进程
- 来源：R36。步骤：提交 `payload = {"cwd": "<任意路径>"}` 的 `session.resume`。
- 断言：`command.rejected` + `nodelink.command.unsupported_field`，`details.field == "cwd"`；后端 `resume` 计数 0；会话状态与两列不变；无审计副作用变化以外的状态变化。
- 落点：TP2。

#### SR-R36-2 `payload` 携带 `agentId` 或任意未知键被拒（边界）
- 来源：R36（「出现任何键」）。步骤：分别提交 `{"agentId": "codex"}`、`{"foo": 1}`、`{"cwd": null}`。
- 断言：三次都是 `nodelink.command.unsupported_field` 且 `details.field` 分别等于被拒字段名（`cwd: null` 也属于「出现键」，`details.field == "cwd"`）；后端计数 0（三次累计）。
- 落点：TP2。

#### SR-R37-1 Agent 不支持恢复时终态失败且不新建会话
- 来源：R37。前置：受控后端配置为返回「后端不支持该操作」（WP3 的新 `UnavailableKind` 取值透过 R37 的映射）。
- 步骤：提交 `session.resume`；随后 `session.list`。
- 断言：`command.terminal` 的 `status` 为 `"failed"` 或 `"uncertain"`，`error.code == "nodelink.command.unsupported"`；会话总数不变（没有新建会话）；该会话仍不可交互（后续 `session.prompt` 按既有「未就绪」错误失败，而不是成功）；两列未被改写。
- 落点：TP2。

### 3.5 Node Link wire 层（R27、R34 的静态部分）——`crates/node-link-protocol/tests/session_resume_command.rs`（新增，TP2）

- 前置数据：只读静态合同 `compatibility/commands/v1/commands.json`、`schemas/node-link/v1/command.schema.json`、`fixtures/node-link/v1/manifest.json`（WP2 会新增 `valid/command-submit-session-resume.json`；TP2 复用该夹具与 manifest，不新增）。
- SR-R27-1 / SR-R27-2 / SR-R34-2 的 wire 断言（见 §3.4 对应条目）：命令名、`kind`、`grant`、`pack`、`transport`、`delivery`、payload 变体解码、`expectedVersion`/`sessionRef` 的 null 规则。
- 与既有门禁的关系：命令词表的四处一致由 `npm run check`（`check:command-catalog`）承担，命令名与 schema 的逐条相等由既有 `tests/schema_drift.rs` 承担；本文件只补 `session.resume` 特有的解码/校验断言，不重复整套词表门禁。
- 落点：TP2。

### 3.6 内联用例（由实现者编写，不新增文件）

这些断言落在各 WP 拥有、并在其写范围内的既有文件里（plan.md 的「逐文件登记」与「整 crate 写范围含 `src/**` 内联测试」），TP2 不重复实现，但会在 §2 的映射表里登记为对齐项。

| 断言 | 对应行 | 落点文件 | 归属 |
| --- | --- | --- | --- |
| `AgentSessionId` 值对象：非空、长度上限、无 NUL；`ResumeSessionRequest` 构造校验 | R1, R17 | `crates/core/src/model/{ids.rs,backend.rs}` 内联 `#[cfg(test)]` | WP3 |
| `UnavailableKind` 新取值进入 `ALL`/`as_str`/`Display`/`port_error_public` 显式覆盖；`model/tests.rs` 的 `ALL.len()` 从 7 改为 8 | R10, R37 | `crates/core/src/model/error.rs` + `crates/core/src/model/tests.rs`（既有断言 `:2208`） | WP3 |
| `required_grant("session.resume") == Some("grant.remote-work")`；`command_name`/`command_kind`/分发臂 | R27, R34 | `crates/core/src/broker.rs` 内联测试 | WP3 |
| `resume_session` 用例顺序（授权 → 读行 → 复校验 → 后端）：失败路径不留副作用、每会话串行门 | R23, R31 | `crates/core/src/broker.rs` / `use_cases.rs` 内联测试 | WP3 |
| `GRANTS` 会员 + `tests/authorization.rs` 的 `scopes.len()`（6 → 7）与会员断言 | R27 | `crates/identity-auth/src/authorization.rs` + `crates/identity-auth/tests/authorization.rs`（既有文件） | WP2 |
| 既有 `tests/migration.rs` 的版本字面量（`owned_schema_version == 4` → `5`、`v3→v4` 用例改名/期望值）与 v5 列在升级用例中的期望 | R13–R19 | `crates/storage-sqlite/tests/migration.rs`（既有文件，WP4/TP2 共享，Merge Owner = TP2） | WP4 |
| `core_payload()` 的 `SessionResume` 臂、`NotTouched`（`crates/server/src/local_admin/test_support.rs`）与 `ScriptedBackends`（`crates/app/tests/support/owner.rs`）的必需方法实现；`command/tests.rs` 的越权/限流/崩溃窗口等价断言 | R27, R30–R33 | `crates/server/**`、`crates/app/tests/support/owner.rs` | WP6 |
| fake ACP child 的 `session/resume` 分支、`resume-ok`/`resume-error` 场景、`--dump-requests`（§3.2 前置） | R8–R12, R26 | `crates/agent-host/src/bin/acpr-fake-acp-agent.rs` | WP5 |

---

## 4. 关键路径覆盖（plan.md 点名的六条）

| # | 关键路径 | 主用例 | 可观察判据 |
| --- | --- | --- | --- |
| ① | 能力未宣告时必须显式不支持且不启动/不发送 | SR-R10-1、SR-R10-2 | `Err`（与「服务端不可用」不同的取值）+ `--dump-requests` 无 `session/resume` + 心跳停止（进程已清理）+ `open` 仍失败；`resume:null` 与省略同判 |
| ② | 越权恢复先于本机读取与文件系统访问 | SR-R31-1、SR-R31-2 | 目标会话的持久化目录**不存在**时仍得到 `nodelink.export.not_granted`（而非 `nodelink.internal.unavailable`）+ 后端计数 0 + 存在/不存在会话响应不可区分 |
| ③ | 崩溃窗口进入 `uncertain` | SR-R32-1 | 终态提交注入失败 → 命令停在非终态 → 调用组合根的同一恢复入口 → `command.status` 返回 `status="uncertain"` + 后端未重试 + 两列不变 |
| ④ | `NULL` 列不得被当作可恢复 | SR-R16-1、SR-R21-2、SR-R37-1 | 升级后两列 `IS NULL`（排除空串）+ 读取返回 `Some` 但两列为 `None` + 恢复以 `nodelink.command.unsupported` 失败、无新会话、无后端调用 |
| ⑤ | v4→v5 升级保留既有行且新列为空 | SR-R16-1、SR-R18-1 | 其余列逐字节不变 + 两列为 `NULL` + 新列在列清单末尾且规格与新建库逐项相等 |
| ⑥ | 重复恢复不产生第二个端点 | SR-R12-1、SR-R12-2、SR-R28-1 | 恰好一个端点可派发 turn + `session/prompt` 无重复行 + 单键重试后端计数 == 1 |

---

## 5. 真实入口、前置数据与资源

### 5.1 恢复路径的真实入口（不是 mock 绕过）
- **本地后端层**：`AgentHost`（`agent_host::AgentHost`，端口 `SessionBackendFactory`/`SessionEndpoint`）驱动**真实 stdio 子进程** `acpr-fake-acp-agent`（`CARGO_BIN_EXE_acpr-fake-acp-agent`），报文经 LF 分帧 JSON 与真实 Agent 一致；`initialize` 协商能力后按真实宣告门控。
- **存储层**：真实 SQLite 文件 + 真实 `storage_sqlite::migrate` 升级路径；断言读的是库文件字节（`quote()`/`sqlite_master`），不是内存模型。
- **Owner 侧命令管线**：真实 loopback listener（`app::daemon::net_config` + `register_node_link_paths`）上的真实 WSS 报文、真实握手、真实 `identity_auth::Authority`、真实 SQLite 审计与幂等行；仅 Agent 后端与 `SessionStore` 故障注入是受控替身（与既有 `node_link_e2e.rs` 同源，已在其模块注释中声明）。

### 5.2 前置数据构造方式
| 需要的前置 | 构造方式 |
| --- | --- |
| 独立数据目录 | `storage-sqlite` 用 `support::temp_dir(name)`（`0700`，`Drop` 自清理）；`app` 用 `support::TempRoot`；`agent-host` 用 `std::env::temp_dir()` 下的 `TempFile` |
| 独立构建缓存 | `CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-tp1`（执行 TP2 时设置） |
| 网络端口 | `listen = "127.0.0.1:0"`（内核分配），断言用 `listener.local_addrs()[0]`，不写死端口 |
| v4 形状库 | 建当前版本库后用原始 `sqlx` 回退两列与版本（§3.3 前置） |
| v2/v3 形状库 | 复制既有夹具 `fixtures/storage/v2/*.sqlite3` 或程序化回退 |
| 会话行与两列的任意取值（含 `NULL`、非规范化路径） | 原始 `sqlx` 连接直接 `UPDATE owned_session`（只用于**构造前置**，断言仍经产品路径读取） |
| Agent 能力宣告 | fake child `--capabilities '{"sessionCapabilities":{"resume":{}}}'` / `'{}'` / `'{"sessionCapabilities":{"resume":null}}'` |
| 「进程是否启动/结束」 | `--heartbeat-file <path>`（50 ms 追加一字节）+ `--dump-requests <path>`（每次收到的 `method` 一行） |
| 越权节点 | 既有 `Chain` 手法：Export scopes 与 `node.pair.confirm` 的 grants 只给 `grant.observe` |

### 5.3 时序与超时
- 既有约定可直接复用：`support::nodelink::{IO_TIMEOUT=10s, MESSAGE_TIMEOUT=15s}`、`Collector::wait_for/wait_for_type`（20 ms 轮询 + 超时返回布尔而非悬挂）。
- 进程结束的判定用「心跳文件在某窗口内停止增长」+「句柄返回」，不用固定 `sleep` 作为唯一依据；窗口取值在各用例里显式写出（例如 2 s 观察窗 + 10 s 上限）。

### 5.4 断言可观察性总则
优先级：协议错误码/结果类型 > 持久化字节（`quote()`、`sqlite_master`、`meta`）> 派发计数 > 进程存在性（心跳）> 请求痕迹（`--dump-requests`）。**不使用**日志文本、私有字段或「调用次数之外的时间序」作为主要判据。

### 5.5 基线核对方式
§1.2、§5.1、§3 中引用的文件与辅助函数均在基线提交 `81e350ff340014265eb7c9251237c799d4357fee` 上通过只读检索核对存在（例如 `crates/app/tests/support/{owner.rs,nodelink.rs}`、`crates/agent-host/src/bin/acpr-fake-acp-agent.rs`、`crates/storage-sqlite/tests/support/mod.rs` 的 `temp_dir`/`column_specs`/`raw_write_pool`）。

### 5.6 平台差异（必须如实记录）
- 未启动任何构建/测试，因此本阶段无平台判定结果；预期差异在 TP2：
  - `#[cfg(unix)]` 变体（SR-R24-2 的 `chmod`、SR-R25-2 的 symlink）在 Windows 开发机不执行 → 记 BLOCKED 或跳过并说明，不冒充通过。
  - `TempDir` 在 Linux runner 上要求 `0700`（既有 `support::temp_dir` 已处理）。
  - Windows 上 `canonicalize` 返回 `\\?\` 前缀，因此**不得**在断言里硬编码路径文本形状，只比较「持久化值 == 收到的值」与「是否等于别名解析值」。

---

## 6. 明确「尚未编写 / 尚未编译」的部分

1. **本阶段没有写任何可编译用例**：`crates/**` 零改动；§3 的 58 条用例全部是设计态，`crates/acp-protocol/tests/session_resume.rs`、`crates/agent-host/tests/resume.rs`、`crates/storage-sqlite/tests/resume_columns.rs`、`crates/app/tests/session_resume_e2e.rs`、`crates/node-link-protocol/tests/session_resume_command.rs` **都还不存在**（属 TP2，W5）。
2. **未编译、未执行、未跑基础检查**：本阶段按 plan.md 的 TP1 行「NOT_APPLICABLE（设计阶段不跑 PV）」不执行 `cargo test`/`npm run check`；因此**没有任何** PV1/PV2 证据，也未声称任何用例通过。
3. **引用到的未实现 API**（TP2 才可编译）：`SessionBackendFactory::resume`、`SessionEndpoint::agent_session_id`、`ResumeSessionRequest`、`AgentSessionId`、`CommandName::SessionResume`、`CommandPayload` 的 `session.resume` 变体、`SessionResumeResult`、`UnavailableKind` 的新取值、`acp-protocol` 的类型化 `session/resume` DTO、`owned_session` 的两个新列、`ScriptedBackends::resume`。
4. **对 WP5 的硬前置尚未实现**：fake ACP child 的 `session/resume` 分支、`resume-ok`/`resume-error` 场景、`--dump-requests`（§3.2）。
5. **未核实的实现细节**（TP2 开工前必须先核实，见 §7）：崩溃窗口的故障 marker 命中在哪个提交、v5 段是否有确定性故障注入点、恢复读取路径的形状（两列是由 `SessionSnapshot`/`Session` 携带还是新增专用端口方法）。最后一项**不影响**本设计的断言入口：所有断言都经 §3.4 的真实管线或 §3.3 的原始 SQL，不依赖该形状选择。

---

## 7. 风险、盲区与待澄清问题

- **Q1（硬前置，需 WP5 承接）**：`crates/agent-host/src/bin/acpr-fake-acp-agent.rs` 属 WP5 的整 crate 写范围，本设计需要它新增 `session/resume` 处理、`resume-ok`/`resume-error` 场景与 `--dump-requests`。plan.md 的 WP5 行只写了「用 fake ACP Agent 覆盖 R5–R12」，**未在 Shared File Ownership 登记该文件**。建议：沿 WP5 承接（推荐），或在 plan.md 登记 TP2 对该文件的写范围。**本设计按「WP5 承接」编写**；若主 Agent 选择后者，§3.2 的落点归属需改成 TP2。
- **Q2（契约口径，已在 plan.md 缓解）**：R10 的 spec 原文是「MUST NOT 启动 Agent 子进程」，而 design D4 明确门控只能发生在 spawn + `initialize` 之后（代价是短暂拉起并清理一个子进程）。本设计按 plan.md `## Risks / Trade-offs` 已批准的口径断言（「不发送 `session/resume` + 不留下进程」）。若后续要求字面口径，则 R10 在 D4 下**不可满足**，应先改 spec 或 D4，再改用例。
- **Q3（R15 的注入点）**：v5 段只有 `ALTER TABLE … ADD COLUMN`，是否存在**确定性**的「事务中途失败」注入点取决于 WP4 的实现（是否按列存在性守卫）。本设计用不变量式断言（不存在半升级形态 + 可重复打开）避免绑定守卫策略；若 WP4 能提供确定性钩子，SR-R15-1 应改为直接命中 v5 段。
- **Q4（共享测试支撑的写范围）**：TP2 需要 `crates/app/tests/support/owner.rs`（`ScriptedBackends::resume` 的观察点：调用计数、最近一次 `ResumeSessionRequest`、可配置结果）与 `crates/app/tests/support/nodelink.rs`（如 `session_resume_body` 辅助）。两文件所在目录已登记（`crates/app/tests/`：Writers WP6,TP2；Merge Owner TP2），因此 TP2 的追加在册，但**必须作为新增内容在 TP2 报告中逐项列出**，避免影响 WP6 已写内容。
- **Q5（R32 的强度和 TP2 前置核实）**：SR-R32-1 走「真实管线 + 组合根同一恢复入口」，不是真实进程重启。TP2 需先核实故障 marker 只命中终态提交视图（accepted 路径的事件是否含 `requestId` 文本）；若也命中，需要时序钩子或 WP6 提供一个受控挂钩。真实进程级重启（`support::Daemon` + 同一 `data_dir`）可作可选强化，但需要撑起 CLI/配置与配对状态。
- **R-1（覆盖盲区，供 reviewer/validator 注意）**：`UnavailableKind` 新取值的「完整性」目前只有 `crates/core/src/model/tests.rs:2208` 的计数断言与 `crates/server/src/local_admin/params.rs:1562` 的 `ALL` 遍历；Coverage Index 没有一行专门覆盖「新取值进入公开错误映射」。本设计把它登记为 §3.6 的 WP3 内联项（R10/R37 的支撑证据），并在此提示：若 reviewer 认为它需要独立 R 行，属 plan.md 的覆盖索引调整，不在 TP1 权限内。
- **R-2（潜在重复）**：R30/R33/R19/R20/R14 与既有用例存在等价断言；TP2 必须按 tester 指令 1 登记为**复用**并注明原用例名，不得复制成第二份实现（否则会掩盖原用例的失效）。
- **R-3（跨 crate 涟漪新增点，plan.md 未登记）**：`crates/core/src/model/tests.rs:2208` 的 `UnavailableKind::ALL.len() == 7` 在 WP3 加取值后必然失败（属 core，WP3 自洽）；`crates/server/src/local_admin/params.rs:1562` 遍历 `ALL`（不破编译，自动通过）。建议在 plan.md 的涟漪清单里补上前者。

---

## 8. handoff_index

```yaml
handoff_index:
  - task_id: "2.7"
    work_package: TP1
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: NOT_AVAILABLE
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: reports/tp1-test-design.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "设计阶段尚无代码提交；契约冻结于 openspec/changes/session-resume/specs/**@design-rev-4 与 design.md@D1–D6，本报告按 Coverage Index 的 37 行逐行产出用例 ID/场景/步骤/断言与落点，未产出可编译用例（属 TP2）"
    source_evidence: NOT_APPLICABLE
  - task_id: "4.1"
    work_package: TP1
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: NOT_AVAILABLE
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: reports/tp1-test-design.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "同 2.7：需求映射与稳定用例 ID（SR-R1-1 … SR-R37-1，共 58 条）已提交；检查项按 plan.md 记 NOT_APPLICABLE（设计阶段不跑 PV1/PV2）"
    source_evidence: NOT_APPLICABLE
```

**待审事项**：本报告需由非作者的新独立 reviewer（plan.md 的 reviewer-T1，CR7/tasks 3.8）逐 R 行核对场景覆盖、真实入口与断言可观察性；本阶段不要求可编译用例与执行材料。
