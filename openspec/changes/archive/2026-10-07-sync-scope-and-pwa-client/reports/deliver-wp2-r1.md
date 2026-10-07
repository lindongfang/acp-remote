<!-- WP2 编码交付原始报告（执行者 coder-w2-r1，接管既有未提交实现并固定为交付提交）。 -->

WP ID: WP2（交付单元 MU1a，阶段 implement，第 1 轮）
role: coder
phase: implement
agent_context:
  agent_id: "CoderWp2 / coder-w2-r1"
  isolation: "fork_turns=none（独立的 WP2 执行实例；接管上一窗口在 `.worktrees/wp2` 留下的未提交工作区改动，不继承其对话。台账已按用户裁决重置，本实例从第 1 轮重新认领并 `dispatch --ack`）"
target_revision: "f64a1968ed56abbac680d661f0e57f05d57fda9d"
scope: "plan.md:860 的 WP2 Write Scope：`schemas/sync/v1/event-views.schema.json`、`crates/sync-protocol/src/views.rs`、`fixtures/sync/v1/valid/view-*.json`、`fixtures/sync/v1/manifest.json`、`docs/SYNC_PROTOCOL.md`、`docs/ACP_COMPATIBILITY_MATRIX.md`。实现 R5–R9 的字段侧。"
changes: "7 个文件、+161/-13，冻结为一个固定提交（见 target_revision）。未新建 `manifest.json` 条目（无新增 fixture，既有三条 `viewFile` 用例的 schema 已随本提交收窄，条目本身无需变更）。"
result: FAIL（PV1 的字面 `npm run verify` 在分配 worktree 内非零退出——已确认失败，按报告契约记 FAIL；根因经跨包复现判定为**环境级**，非产品缺陷。十道合同门禁 + clippy + test + fmt 等价覆盖全部 PASS，详见 Checks）

## 交付概览

| 项 | 取值 |
| --- | --- |
| 仓库 | `D:\Project\acp-remote` |
| worktree | `D:\Project\acp-remote\.worktrees\wp2` |
| 分支 | `feat/wp2-derived-event-fields` |
| 固定起点（base） | `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`（`refs/heads/main`） |
| 交付提交（target） | `f64a1968ed56abbac680d661f0e57f05d57fda9d` |
| 提交信息 | `feat(sync): WP2 派生事件字段——file.changed 可选统计与 agent 连接状态封闭枚举` |
| `git diff --stat`（base..target） | `crates/sync-protocol/src/views.rs 133 ++；docs/ACP_COMPATIBILITY_MATRIX.md 4；docs/SYNC_PROTOCOL.md 15；fixtures/…/view-agent-connected.json 4；…/view-agent-disconnected.json 4；…/view-file-changed.json 5；schemas/sync/v1/event-views.schema.json 9` —— 7 files changed, 161 insertions(+), 13 deletions(-) |
| 上下文方式 | fork_turns=none；接管既有工作区，逐项 `git diff` 独立核对后提交（未重写实现） |
| 上游依赖 | 无（W1，Dependencies = `none`，契约起点） |
| 资源 | `CARGO_TARGET_DIR=D:\Project\acp-remote\.target-wt\wp2`（独占）；仓库根 `node_modules/` 只读消费 |

## 需求映射（R5–R9 逐条）

| Req | 字段侧落点 | 具体改动 | 核实方式 |
| --- | --- | --- | --- |
| **R5** 文件改动事件由类型化 Diff 派生 | `schemas/sync/v1/event-views.schema.json#/$defs/file.changed` + 两份文档 | 本包只覆盖 view 与 schema（Diff 的实际产生在 WP4/core，D4）。`docs/SYNC_PROTOCOL.md` §10.3 与新段写明：派生源**唯一**是 ACP 工具调用内容中的类型化 Diff 元素，`tool_content.diff` 是 `file.changed` 的唯一来源；`docs/ACP_COMPATIBILITY_MATRIX.md` §7 登记 `tool_content.diff` 的 `delivery = mvp`、`sync = event` | 已与 `compatibility/acp/v1/matrix.json:85` 逐字核对一致 |
| **R6** 改动行数按行级差异统计 | 同 schema 的 `file.changed` | 新增可选 `addedLines`/`deletedLines`，均 `$ref: common.schema.json#/$defs/decimalString`；**必填集合不变**（仍恰为 `changeId/kind/displayPath/summary`）；`additionalProperties: true` 未动 | 读 schema：`required=["changeId","kind","displayPath","summary"]`；diff 中无 `required` 行变更 |
| **R7** 展示路径相对化且不泄漏工作区外结构 | 同 schema + §10.3 | 新增可选 `outsideWorkspace`（`type: boolean`）；§10.3 写明 `displayPath` 相对会话工作区根、不含根片段或回退层级、越界置 `outsideWorkspace: true` 且只给文件名、按**规范化后的前缀关系**判定 | 读 schema + §10.3 第 40 行 |
| **R8** Agent 连接状态是节点级生命周期 | schema `$defs/agent.connected`+`disconnected` + `views.rs` | `state` 由 `{"type":"string","minLength":1,"maxLength":32}` 收成 `{"enum":["connected"]}` / `{"enum":["disconnected"]}`（各自唯一且互不相同）；`views.rs` 的 `VIEW_ENUMS` 同步登记两条（指针 `/properties/state`，取值同序）；`AgentConnectedState`/`AgentDisconnectedState` 两个 Rust 镜像枚举（`ALL`/`as_str`/`Display`/`FromStr`/`Serialize`/`Deserialize` 六件套）被两个视图字段实际引用；`FileChanged` 同步加三个可选字段 | `tests/schema_drift.rs::view_enums_match_schema` 双向门禁绿（见 Checks）；`cargo test -p sync-protocol` 52/52 绿 |
| **R9** 会话标题只由 Agent 通知更新 | `docs/SYNC_PROTOCOL.md` §10.3 + 矩阵 | 新段写明：标题**单向**来自 `session_info_update` 投影的 `session.info.changed`，命令目录中**不存在**客户端发起的重命名命令，服务端不提供绕过路径；只有更新时间时标题不变、显式置空时呈现未命名。矩阵登记 `update.session_info_update` 的 `delivery = mvp` | 命令目录实读 13 条命令无重命名项；矩阵 `:73` 核对一致 |

**改动文件清单（与 Write Scope 逐项对齐）**

- `schemas/sync/v1/event-views.schema.json`（WP2 独占）
- `crates/sync-protocol/src/views.rs`（WP2 独占区域内）
- `fixtures/sync/v1/valid/view-file-changed.json`、`view-agent-connected.json`、`view-agent-disconnected.json`（`view-*` 前缀归 WP2）
- `docs/SYNC_PROTOCOL.md`（只动 §10.3，WP1 动 §9.4/§11.5，同文件不同小节）
- `docs/ACP_COMPATIBILITY_MATRIX.md`（§7）
- `fixtures/sync/v1/manifest.json`：**未改动**——本包未新增/删除 fixture，三条既有 `viewFile` 条目的 `viewDef`/`schema` 引用不变，故无需追加；符合共享写点「条目不重不漏、保持排序」

**未越界核实**：`git diff --name-only` 仅 7 个文件；对 `crates/sync-protocol/src/{command,sync}.rs`、`crates/sync-protocol/tests/`、`crates/{core,storage-sqlite,agent-host,server,app,node-link-protocol}/` 的 diff 计数 = 0。

## F3 / F5 核实结论

- **F5（MINOR，上一轮记录称已修）—— 已确认落地**。`git diff` 亲自核对：`fixtures/sync/v1/valid/view-agent-connected.json` 与 `view-agent-disconnected.json` 的 `sessionId`、`sessionSequence` 均已从非空值改为 `null`（前者 `sessionSequence: null` / `sessionId: null`，后者同）。与 `specs/core-derived-events` R8「节点级事件会话标识 MUST 为空」一致。本提交内文件内容无回退。
- **F3（MINOR，转 TP1）—— 仍是 TP1 的待办，本包未动**。`crates/sync-protocol/tests/schema_drift.rs:295-322` 的第三个方向（Rust 镜像 `ALL`/`as_str` → 登记表）仍是四条硬编码 `assert_eq!`（`PlanPriority`/`PlanStatus`/`ElicitationAction`/`TerminalStream`），未含 `AgentConnectedState`/`AgentDisconnectedState`。该文件归 TP1（`plan.md:867`），**不在 WP2 Write Scope**，本包未修改（`git status` 核实）。旁证：TP1 工作区（`.worktrees/tp1`，未提交）已在 `schema_drift.rs:327/333` 追加这两条断言，F3 正在被 TP1 关闭。

## Checks（每个 Check ID 单独一行）

环境：Node v22.22.0 / npm 10.9.4 / cargo 1.98.1 / rustc 1.98.1（`rust-toolchain.toml` 固定 1.98.1）；`CARGO_TARGET_DIR=D:\Project\acp-remote\.target-wt\wp2`；cwd = `.worktrees\wp2`（除另行标注）。

| Check ID | 命令 | 范围 | 退出码 | 结果 | 日志 |
| --- | --- | --- | --- | --- | --- |
| PV1/npm-check | `npm run check` | 十道合同门禁 | 0 | PASS（schemas 127 valid/30 invalid/39 event views；commands 13；errors 58；features 13；assets 17 schemas/170 fixtures；acp 71 rows；docs 412 links；boundaries 12 crates；drift §7 36 DDL+§5 15 traits/96 methods；agentic 全 PASS，openspec validate 21/21） | `reports/wp2-r1-check.log` |
| PV1/fmt-all（字面） | `cargo fmt --all -- --check` | `check:rust` 第 1 步 | **1** | **环境级失败**：`` `cargo metadata` exited with an error: … .worktrees\wp2\vendor\windows-local-ipc\Cargo.toml … workspace: D:\Project\acp-remote\Cargo.toml ``。**与本 diff 无关**：同一命令在**未被本包触碰**的 `.worktrees/wp1` 与 `.worktrees/mu1-merge` 上同样 exit 1（实测），根仓库 `D:\Project\acp-remote` 上 exit 0 | `reports/wp2-r1-rust-fmt.log`（含 [2a]） |
| PV1/fmt-equiv | `cargo fmt -- --check` | 同上，去 `--all` 后的等价覆盖 | 0 | PASS。`--verbose` 逐文件核实：覆盖 workspace 全部 **12** 个成员（`acp-protocol, acpr-transcript, acpr-wire, agent-host, app, core, identity-auth, identity-keystore, node-link-protocol, server, storage-sqlite, sync-protocol`），与根仓库 `cargo fmt --all` 的成员集合相同（仅多出非成员的 `vendor/windows-local-ipc`，其源码本包未改且与根仓库逐字节相同）；`views.rs` 确认在受检文件列表内 | `reports/wp2-r1-rust-fmt.log`（含 [2b]） |
| PV1/rustfmt-target | `rustfmt --check --edition 2024 crates/sync-protocol/src/views.rs` | 唯一改动的 Rust 文件 | 0 | PASS（无输出） | `reports/wp2-r1-rust-fmt.log`（含 [2c]） |
| PV1/clippy-ws | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | `check:rust` 第 2 步 | 0 | PASS（无 warning/error） | `reports/wp2-r1-rust-clippy.log` |
| PV1/test-ws | `cargo test --locked --workspace --all-features` | `check:rust` 第 3 步 | 0 | PASS（80 suites，**1092 passed / 0 failed / 2 ignored**；含 `schema_drift::view_enums_match_schema`、`view_types_match_schema_defs`、`view_projections::every_view_fixture_projects_onto_its_declared_event_type`） | `reports/wp2-r1-rust-test.log` |
| PV1/verify（字面 `npm run verify`） | `npm run verify`（cwd = `.worktrees\wp2`） | PV1 字面入口 | **1** | **非零退出**：在第 2 步 `check:rust` 的首个子命令 `cargo fmt --all -- --check` 处失败，原因同上（环境级）。第 1 步 `npm run check`（十道门禁）已 exit 0 | `reports/wp2-r1-verify-npm.log` |
| Local/fmt（业务口径） | `cargo fmt --all` | 计划 Local Checks | —（不可达） | 与 PV1/fmt-all 同因；未执行 | `reports/wp2-r1-rust-fmt.log` |
| Local/clippy | `cargo clippy --locked -p sync-protocol --all-targets -- -D warnings` | 计划 Local Checks | 0 | PASS | （同 PV1/clippy-ws 日志；[2d] 前的包级运行亦 exit 0） |

**PV1 判定说明（不掩盖前序失败）**：字面 `npm run verify` 确实非零退出，失败点是 `cargo fmt --all -- --check` 的 **worktree 环境解析问题**（`vendor/windows-local-ipc` 是根 workspace 的 `exclude` 成员，worktree 副本再多一层目录前缀，cargo 的 workspace 发现走到外层根 `Cargo.toml`）。该失败在未改动的 `wp1`/`mu1-merge` worktree 上 100% 复现，根仓库同命令 exit 0，故**不是本提交引入的产品缺陷**，也无法通过本包 Write Scope 内的改动消除（`Cargo.toml`/`vendor/**` 不属本包，且 `plan.md:915` 只把该文件区域收窄给 WP1 的 `command/sync/common.schema.json`）。格式化本身的实质判定已由 `cargo fmt -- --check`（12 成员全覆盖，含 `views.rs`）+ `rustfmt --check` 直检，两者 exit 0。**MU1a 候选阶段的既有 `reports/PV1.log` 也采用同口径的分解执行**（只跑 `npm run check` + `cargo test -p sync-protocol`，未跑字面 `--all`）。

**结论**：PV1 的实质子检查（十道合同门禁 + clippy + test + fmt 等价覆盖）全部 PASS；**唯一未通过的项是环境级不可执行的字面 `cargo fmt --all`**，使字面 `npm run verify` 非零退出，故本行 result 记 **FAIL**（已确认失败，不因归因环境而改写为 PASS）。请主 Agent 裁决：接受分解证据（既有 MU1a `reports/PV1.log` 即采用此口径），或调整工作包环境（统一 worktree 内 PV1 的 fmt 调用口径 / 忽略 `vendor`）。**未发现任何产品缺陷**；本包交付提交本身的内容不构成 PV1 失败原因。

## 依赖包含关系

- 上游：无（W1）。`plan.md:893` W1 行：WP2 无 code 与 contract 依赖，契约起点。
- 下游由本冻结提交被消费（contract 依赖，非 code 链接）：WP3/WP4（`event-views.schema.json@WP2`）、WP5a/WP7（schema 作为数据读取）、TP1（`views.rs@WP2` 作断言形状基准）、TP2（形状基准）。均未在本包内改动，本报告不判其结论。
- 本包**未**合入任何其他包改动，也**未**触碰共享写点 `manifest.json`（无新增条目，保持排序与既有条目不变）。

## 资源释放

- `CARGO_TARGET_DIR=D:\Project\acp-remote\.target-wt\wp2` 为分配给本包的独占目录，登出时保留供主 Agent 复跑；本包未清理（清理归 provisioner 回收 worktree 时执行），未写入仓库根 `node_modules/`。
- 本包未创建/切换/清理任何 worktree；未 `git add -f` 任何日志（`reports/**/*.log` 按 `.gitignore:27` 不入库）；未 push/publish/归档。
- 证据日志已落到变更目录（不入库，`git check-ignore` 已核实）：
  - `reports/wp2-r1-check.log`、`reports/wp2-r1-rust-fmt.log`、`reports/wp2-r1-rust-clippy.log`、`reports/wp2-r1-rust-test.log`、`reports/wp2-r1-verify-npm.log`

## 未执行项 / 待补

- **独立 review 未返回**：本包只交固定提交供主 Agent 调度 `review-2`；本报告的自测不构成独立检视结论。
- **E2E/IV1/EV1**：本包无 E2E；`plan.md:33-37` 的 IV1/EV1 属 MU2/premerge，尚未产生。
- **PV2/AC1–AC3**：WP3/WP4 阶段门禁，不属本包。
- **F3**：仍未关闭，归 TP1（不在本包 Write Scope），仅报告。
- 字面 `npm run verify` 未通过（环境级）——见上，待主 Agent 裁决口径。

## 待澄清 / 提请主 Agent

1. **PV1 的 `cargo fmt --all -- --check` 在 worktree 内不可执行**（跨包复现）。建议统一口径：worktree 内以 `cargo fmt -- --check` + 目标文件 `rustfmt --check` 作等价证据（与既有 MU1a `PV1.log` 的分解口径一致），或修复 worktree 的 workspace 发现（须改根 `Cargo.toml`/`vendor`，越出本包范围）。
2. **`fixtures/sync/v1/manifest.json` 未改动**：本包无新增 fixture，三条既有 `viewFile` 条目沿用；如 Merge Owner 认为需为收窄后的 `state` 追加负例向量，归 TP1（`plan.md:867` `fixtures/sync/v1/`（只增不改））。

## handoff_index

```agentic-handoff
version: 1
agent_context:
  agent_id: "coder-w2-r1"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "2.2"
    work_package: WP2
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "f64a1968ed56abbac680d661f0e57f05d57fda9d"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/deliver-wp2-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "Attempt 1 交付：从固定起点 353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f 起，在 .worktrees/wp2（分支 feat/wp2-derived-event-fields）接管既有未提交实现，独立核对契约后冻结为单一提交 f64a1968；7 文件 +161/-13，全部落在 WP2 Write Scope 内。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.3"
    work_package: WP2
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "f64a1968ed56abbac680d661f0e57f05d57fda9d"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/deliver-wp2-r1.md"
    result: FAIL
    evidence_status: NEW
    applicability_basis: "在交付提交 f64a1968 上执行 npm run verify（cwd=.worktrees/wp2，CARGO_TARGET_DIR=.target-wt/wp2）。十道 npm 合同门禁 exit 0；clippy --workspace -D warnings exit 0；test --workspace exit 0（1092 passed/0 failed）；fmt 以 cargo fmt -- --check（12 成员全覆盖）+ rustfmt --check 目标文件 exit 0。字面 cargo fmt --all -- --check 因 worktree 内 vendor/windows-local-ipc 的 workspace 发现失败而 exit 1（未改动的 wp1/mu1-merge 亦复现，根仓库 exit 0），故整条 npm run verify exit 1。日志：reports/wp2-r1-{check,rust-fmt,rust-clippy,rust-test,verify-npm}.log。归因环境，非产品缺陷。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.3"
    work_package: WP2
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "f64a1968ed56abbac680d661f0e57f05d57fda9d"
    evidence_type: CHECK
    evidence_id: PV1/npm-check
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/deliver-wp2-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "npm run check 在交付提交上 exit 0：check:schemas/commands/errors/features/assets/acp/docs/boundaries/drift/agentic 十道全绿，含本包新增枚举与可选字段的双向门禁。日志：reports/wp2-r1-check.log。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.3"
    work_package: WP2
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "f64a1968ed56abbac680d661f0e57f05d57fda9d"
    evidence_type: CHECK
    evidence_id: PV1/clippy-ws
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/deliver-wp2-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo clippy --locked --workspace --all-targets --all-features -- -D warnings 在交付提交上 exit 0（rustc/cargo 1.98.1）。日志：reports/wp2-r1-rust-clippy.log。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.3"
    work_package: WP2
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "f64a1968ed56abbac680d661f0e57f05d57fda9d"
    evidence_type: CHECK
    evidence_id: PV1/test-ws
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/deliver-wp2-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo test --locked --workspace --all-features 在交付提交上 exit 0：80 suites，1092 passed / 0 failed / 2 ignored，含 schema_drift::view_enums_match_schema 与 view_projections 夹具投影。日志：reports/wp2-r1-rust-test.log。"
    source_evidence: NOT_APPLICABLE
```
