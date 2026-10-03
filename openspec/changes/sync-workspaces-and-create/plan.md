<!-- 主 Agent 维护协作计划；规则见 schema.yaml，角色细节见 roles/。
     本文件写安排，tasks 写步骤，verification 写执行历史。 -->

## Scope and Contracts

- Specs Revision: 五份增量规范（`specs/sync-workspace-catalog`、`specs/sync-session-create`、`specs/workspace-resolution`、`specs/scope-expansion`、`specs/storage-schema-v2-migration`），共 13 条 Requirement / 39 条 Scenario，`openspec validate sync-workspaces-and-create --strict` 通过（2026-10-02）。
- Design Revision: `design.md` 的 D1–D9（2026-10-02），冻结两类接口——wire 形状（目录引用、两个快照资源、`session.create` payload 与终态）与 Rust 侧形状（`SessionSummary.workspace`、`SessionUpdate.workspace_alias`、v6 列）。
- Convergence Check: 一致。specs 的每条 Requirement 都能在 design 找到对应决策（目录引用↔D1、快照与门控↔D2、命令与授权↔D3–D5、迁移↔D6、原子点↔D7）；无 requirement 依赖未定接口。唯一已知限制（imported 会话 `workspace` 恒为 `null`）已在 design 的 Risks 与本计划的 Coverage Index 中记录，不阻塞拆包。
- Skip Specs: no

## Contract Changes

无（本次为初始契约）。后续澄清导致需求/接口变化时，改动记录写入 verification 的 `## Check Plan Changes`，本节只引用。

## Coverage Index

```agentic-coverage
version: 1
target_ref: refs/heads/main
rows:
  - id: R1
    source: { path: specs/sync-workspace-catalog/spec.md, heading: "### Requirement: 会话摘要携带目录归属引用" }
    tasks: ["2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R2
    source: { path: specs/sync-workspace-catalog/spec.md, heading: "#### Scenario: 已在已登记目录中创建的会话" }
    tasks: ["2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R3
    source: { path: specs/sync-workspace-catalog/spec.md, heading: "#### Scenario: 没有目录归属的会话" }
    tasks: ["2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R4
    source: { path: specs/sync-workspace-catalog/spec.md, heading: "#### Scenario: 目录重指向后归属不漂移" }
    tasks: ["2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R5
    source: { path: specs/sync-workspace-catalog/spec.md, heading: "#### Scenario: 别名已从本机删除时归属仍稳定" }
    tasks: ["2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R6
    source: { path: specs/sync-workspace-catalog/spec.md, heading: "#### Scenario: imported 会话的归属" }
    tasks: ["2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R7
    source: { path: specs/sync-workspace-catalog/spec.md, heading: "### Requirement: 目录与 Agent 目录随快照下发" }
    tasks: ["2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R8
    source: { path: specs/sync-workspace-catalog/spec.md, heading: "#### Scenario: 首次同步获得完整目录" }
    tasks: ["2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R9
    source: { path: specs/sync-workspace-catalog/spec.md, heading: "#### Scenario: 空目录仍出现在快照中" }
    tasks: ["2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R10
    source: { path: specs/sync-workspace-catalog/spec.md, heading: "#### Scenario: 离线渲染不依赖在线查询" }
    tasks: ["2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R11
    source: { path: specs/sync-workspace-catalog/spec.md, heading: "### Requirement: 目录资源的协商与授权过滤" }
    tasks: ["2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R12
    source: { path: specs/sync-workspace-catalog/spec.md, heading: "#### Scenario: 未协商时不发送该资源" }
    tasks: ["2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R13
    source: { path: specs/sync-workspace-catalog/spec.md, heading: "#### Scenario: 未协商时摘要不含目录字段" }
    tasks: ["2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R14
    source: { path: specs/sync-workspace-catalog/spec.md, heading: "#### Scenario: 缺少目录读取授权的设备不下发" }
    tasks: ["2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R15
    source: { path: specs/sync-workspace-catalog/spec.md, heading: "### Requirement: 目录相关输出不泄漏规范化路径" }
    tasks: ["2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R16
    source: { path: specs/sync-workspace-catalog/spec.md, heading: "#### Scenario: 快照与会话摘要只含引用" }
    tasks: ["2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R17
    source: { path: specs/sync-workspace-catalog/spec.md, heading: "#### Scenario: 错误响应不回显路径" }
    tasks: ["2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R18
    source: { path: specs/sync-session-create/spec.md, heading: "### Requirement: 命令登记与双层授权" }
    tasks: ["2.1", "2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R19
    source: { path: specs/sync-session-create/spec.md, heading: "#### Scenario: 未持 scope 的设备被拒且无副作用" }
    tasks: ["2.1", "2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R20
    source: { path: specs/sync-session-create/spec.md, heading: "#### Scenario: 持 scope 的设备成功创建" }
    tasks: ["2.1", "2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R21
    source: { path: specs/sync-session-create/spec.md, heading: "### Requirement: 载荷形状与拒绝" }
    tasks: ["2.1", "2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R22
    source: { path: specs/sync-session-create/spec.md, heading: "#### Scenario: 携带 cwd 被拒且不创建" }
    tasks: ["2.1", "2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R23
    source: { path: specs/sync-session-create/spec.md, heading: "#### Scenario: 只接受已登记引用" }
    tasks: ["2.1", "2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R24
    source: { path: specs/sync-session-create/spec.md, heading: "### Requirement: 设备级授权语义与副作用" }
    tasks: ["2.1", "2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R25
    source: { path: specs/sync-session-create/spec.md, heading: "#### Scenario: 新登记的目录自动进入已授权范围" }
    tasks: ["2.1", "2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R26
    source: { path: specs/sync-session-create/spec.md, heading: "#### Scenario: 撤销后立即失效" }
    tasks: ["2.1", "2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R27
    source: { path: specs/sync-session-create/spec.md, heading: "### Requirement: 引用失败与解析失败的分类" }
    tasks: ["2.1", "2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R28
    source: { path: specs/sync-session-create/spec.md, heading: "#### Scenario: 未登记别名是参数类错误" }
    tasks: ["2.1", "2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R29
    source: { path: specs/sync-session-create/spec.md, heading: "#### Scenario: 已登记但解析失败是服务端错误" }
    tasks: ["2.1", "2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R30
    source: { path: specs/sync-session-create/spec.md, heading: "### Requirement: 终态与幂等" }
    tasks: ["2.1", "2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R31
    source: { path: specs/sync-session-create/spec.md, heading: "#### Scenario: 重复 requestId 不产生第二个会话" }
    tasks: ["2.1", "2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R32
    source: { path: specs/sync-session-create/spec.md, heading: "#### Scenario: 崩溃窗口进 uncertain" }
    tasks: ["2.1", "2.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R33
    source: { path: specs/workspace-resolution/spec.md, heading: "### Requirement: 创建时别名的持久化与投影来源" }
    tasks: ["2.3", "2.4"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R34
    source: { path: specs/workspace-resolution/spec.md, heading: "#### Scenario: 创建写入别名与路径" }
    tasks: ["2.3", "2.4"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R35
    source: { path: specs/workspace-resolution/spec.md, heading: "#### Scenario: 不按路径反查归属" }
    tasks: ["2.3", "2.4"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R36
    source: { path: specs/workspace-resolution/spec.md, heading: "#### Scenario: 别名重指向不改变既有会话的归属" }
    tasks: ["2.3", "2.4"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R37
    source: { path: specs/scope-expansion/spec.md, heading: "### Requirement: 设备授权包 pack.create-session" }
    tasks: ["2.1"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R38
    source: { path: specs/scope-expansion/spec.md, heading: "#### Scenario: 展开得到命令级 scope" }
    tasks: ["2.1"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R39
    source: { path: specs/scope-expansion/spec.md, heading: "#### Scenario: 包成员漂移被门禁发现" }
    tasks: ["2.1"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R40
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "### Requirement: 版本常量与 migration 幂等" }
    tasks: ["2.4"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R41
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: 连续两次打开 schema 文本不变" }
    tasks: ["2.4"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R42
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: 升级中途失败整体回滚" }
    tasks: ["2.4"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R43
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: v5 到 v6 升级保留既有会话行且新列为空" }
    tasks: ["2.4"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R44
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: 新列写入后可读回且不推导" }
    tasks: ["2.4"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R45
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: v4 到 v5 升级保留既有会话行且新列为空" }
    tasks: ["2.4"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R46
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: 升级库与新建库的 owned 列清单相等" }
    tasks: ["2.4"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R47
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: v3 到 v4 升级给既有节点行写空清单" }
    tasks: ["2.4"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R48
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: v2 到 v3 升级保留审计并扩展词表" }
    tasks: ["2.4"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R49
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "### Requirement: 会话目录归属列的写入与解释" }
    tasks: ["2.4"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R50
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: 写入别名原文" }
    tasks: ["2.4"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R51
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: 恢复不改写目录归属" }
    tasks: ["2.4"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R52
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: NULL 不被路径反查补齐" }
    tasks: ["2.4"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R53
    source: { path: specs/workspace-resolution/spec.md, heading: "#### Scenario: 未取得 ACP 会话标识时三列同为空" }
    tasks: ["2.3", "2.4"]
    checks: [PV1]
    evidence: [reports/PV1.log]
```

## Work Packages

| ID | Goal / Scenarios | Dependencies | Contract Freeze | Owner | Role | Reviewer | Branch / Worktree | Write Scope | Inputs / Outputs | Verification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | R18–R32 的命令与词表面、R37–R39：把 `session.create` 加入 Sync 词表族（命令目录、两份协议 schema、features、身份镜像、文档表格） | none | `openspec/changes/sync-workspaces-and-create/design.md@D3,D4,D5,D7,D8` | coder-vocab | coder | reviewer-vocab | `feat/sync-vocab` | `compatibility/commands/v1/commands.json`, `compatibility/features/v1/features.json`, `schemas/sync/v1/command.schema.json`, `schemas/node-link/v1/command.schema.json`, `crates/sync-protocol/src/command.rs`, `crates/node-link-protocol/src/command.rs`, `crates/identity-auth/src/authorization.rs`, `crates/identity-auth/tests/authorization.rs`, `fixtures/sync/v1/`, `docs/SYNC_PROTOCOL.md`, `docs/SECURITY_DESIGN.md`, `docs/FRONTEND_DESIGN.md`, `docs/DEVELOPMENT_PLAN.md`, `README.md` | 冻结的 wire 合同（design D3/D4）→ 词表与文档同步；命令与 feature 协商向量及其 manifest 登记见 Shared File Ownership 的 `fixtures/sync/v1/` 与 `fixtures/sync/v1/manifest.json` 两行；产出 `npm run check` 与 `cargo test -p sync-protocol -p identity-auth`；`grant` 保持 `grant.remote-work`，故不触碰 `crates/core/src/broker.rs` | PV1 |
| WP2 | R1–R17：目录引用与两个快照资源的 wire 形状（schema、Rust 类型、固定向量） | none | `openspec/changes/sync-workspaces-and-create/design.md@D1,D2` | coder-wire | coder | reviewer-wire | `feat/sync-catalog-wire` | `schemas/sync/v1/common.schema.json`, `schemas/sync/v1/sync.schema.json`, `crates/sync-protocol/src/common.rs`, `crates/sync-protocol/src/sync.rs`, `crates/sync-protocol/tests/`, `fixtures/sync/v1/`, `crates/server/src/node_link/command.rs`, `crates/server/src/node_link/resource.rs`, `docs/SYNC_PROTOCOL.md` | 冻结的 wire 形状（design D1/D2）→ schema + 类型 + 向量 + manifest 登记（见 Shared File Ownership 的 `fixtures/sync/v1/` 与 `fixtures/sync/v1/manifest.json` 两行）；`crates/sync-protocol/tests/` 默认不需改，偏离「未协商时缺席」时只允许在该目录补测试；产出 `cargo test -p sync-protocol`、`cargo clippy -p sync-protocol -p server`、`npm run check` | PV1 |
| WP3 | R33–R36 的 core 侧：`SessionSummary` 的目录引用与 `SessionUpdate` 的别名写入 | none | `openspec/changes/sync-workspaces-and-create/design.md@D1,D6` | coder-core | coder | reviewer-core | `feat/sync-core-projection` | `crates/core/src/model/session.rs`, `crates/core/src/model/ids.rs`, `crates/core/src/ports.rs`, `crates/core/src/use_cases.rs`, `crates/core/src/broker.rs`, `crates/server/src/node_link/command/tests.rs`, `crates/server/src/node_link/resource/tests.rs`, `crates/app/**`（条件性：核实当前无 `SessionSummary`/`SessionUpdate` 构造点，仅在 core 接口变更确实波及时修改）, `docs/CORE_PORTS_AND_STORAGE.md`（§3.1/§3.6/§5 的可投影形状与窄写入字段）, `docs/MODULE_ARCHITECTURE.md`（§4.1 值对象清单新增 `WorkspaceRef`） | core 的投影与写入形状（含 `WorkspaceRef` 值对象）；产出 `cargo test -p core`、`cargo clippy -p core -p server`、`node scripts/check-contract-drift.mjs`（§5 部分） | PV1 |
| WP4 | R33–R36、R40–R52 的存储侧：v6 迁移、别名列读写、投影期 JOIN | code:WP3 | `openspec/changes/sync-workspaces-and-create/design.md@D6` | coder-storage | coder | reviewer-storage | `feat/sync-storage-v6` | `crates/storage-sqlite/src/migrate.rs`, `crates/storage-sqlite/src/session_store.rs`, `crates/storage-sqlite/tests/`, `docs/CORE_PORTS_AND_STORAGE.md`（§7 标题/§7.2/§7.3、§9 判据 1/28 的版本链、§11.3 的「过新」取值、文件头版本记录） | core 的端口形状（WP3 已验收）→ v6 DDL 与读写路径 + 版本链文字；产出 `cargo test -p storage-sqlite`、`cargo clippy -p storage-sqlite`、`node scripts/check-contract-drift.mjs`（§7 部分） | PV1 |

- Dependency Declaration Review: reviewer-plan（独立于全部 WP Owner）；以 `phase: plan`、`stage: plan` 核实 `code:WP3`（storage 在 `ports.rs` 的 `SessionUpdate`/`SessionSummary` 落地前无法编译）与 `resource:R1`（构建目录确实可隔离）声明属实，目标为当前 contractDigest；结果为 PASS 时记入 verification 的 `## Dependency Declaration Review`。

## Execution Waves

| Wave | Work Package | Enter Condition | Serialization Reason |
| --- | --- | --- | --- |
| W1 | WP1 | 契约冻结于 `openspec/changes/sync-workspaces-and-create/design.md@D3,D4,D5,D7,D8` | NOT_APPLICABLE |
| W1 | WP2 | 契约冻结于 `openspec/changes/sync-workspaces-and-create/design.md@D1,D2` | NOT_APPLICABLE |
| W1 | WP3 | 契约冻结于 `openspec/changes/sync-workspaces-and-create/design.md@D1,D6` | NOT_APPLICABLE |
| W2 | WP4 | WP3 进入已验收集成基线（`crates/core` 端口形状已落地） | NOT_APPLICABLE |

## Shared File Ownership

| File | Writers (WP) | Merge Owner | Merge Order | Re-verify After Merge | Region Note |
| --- | --- | --- | --- | --- | --- |
| `docs/SYNC_PROTOCOL.md` | WP1, WP2 | WP1 | WP2 → WP1 | WP1: PV1 | WP1 改 §5.2/§11.3/§11.5/§16.2，WP2 改 §9.4/§9.6/§10.3，章节不重叠；PV1 即 `npm run check` + `npm run check:rust` |
| `fixtures/sync/v1/` | WP1, WP2 | WP1 | WP2 → WP1 | WP1: PV1 | 本行**不含** `manifest.json`（该文件见下一行）：只覆盖各自的向量文件，WP1 写命令与 feature 协商向量，WP2 写快照资源向量 |
| `crates/server/src/node_link/` | WP2, WP3 | WP2 | WP2 → WP3 | WP2: PV1 | WP2 改非测试文件中的 wire 投影构造处（`command.rs` 写 `workspace: None`；`resource.rs` 组装的是 node-link 自己的 `SnapshotItems::SessionMeta`，与本次冻结的 sync wire 形状无关，核实后无改动），WP3 改测试文件（`command/tests.rs`/`resource/tests.rs` 的 `SessionSummary::try_new` 调用点） |
| `docs/CORE_PORTS_AND_STORAGE.md` | WP3, WP4 | WP3 | WP4 → WP3 | WP3: PV1 | WP3 改 §3.1/§3.6/§5，WP4 改 §7 标题/§7.2/§7.3、§9 判据 1/28 的版本链、§11.3 的「过新」取值，章节不重叠；**文件头 `> 版本：` 链由 WP3 与 WP4 各自追加一行**（该区域两方共写），按 WP4 → WP3 顺序由 WP3 定稿 |
| `crates/sync-protocol/src/` | WP1, WP2 | WP1 | WP2 → WP1 | WP1: PV1 | WP1 改 `command.rs`，WP2 改 `common.rs` 与 `sync.rs`；文件级不相交 |
| `fixtures/sync/v1/manifest.json` | WP1, WP2 | WP1 | WP2 → WP1 | WP1: PV1 | 共同写入区域：WP1 只追加命令与 feature 协商向量的 cases 条目，WP2 只追加 workspaces/agents 快照资源向量的条目。`check:schemas` 对「已登记但文件不存在」与「文件存在但未登记」双向报错，因此**分支级约束**是：每个 WP 必须在自己的分支内把向量文件与其登记条目原子落地，该分支的 `npm run check` 才绿；向量文件与条目同属一个 WP 的同一次提交，故集成时两种合入顺序均为绿，不存在预期红窗口 |

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| WP4 | WP3 | 规划期写验收条件：WP3 的 `crates/core` 端口与值对象改动通过独立 review 并进入单元集成基线（`verification.md` 的 Handoff Index） | 单元集成基线提交（接入时引用 verification 中已确认起点） | 候选包含 WP3 的交付提交，且基线上 `cargo test -p core` 与 `node scripts/check-contract-drift.mjs` 通过（该脚本两侧同次运行；WP4 分支上 §5 与 `ports.rs` 已由 WP3 对齐，应自然通过）；storage-sqlite 的编译与测试证据归 WP4 自身交付验收 | WP3 端口形状或 `docs/CORE_PORTS_AND_STORAGE.md` §5 变化时，WP4 的读取路径与 §7 描述重验 |
| （集成窗口）storage-sqlite | WP3 | 规划期登记：无下游工作包，但有明确的收口方 | WP3 进入已验收集成基线之后、WP4 交付之前 | **预期红窗口**：WP3 给 core `SessionSummary` / `SessionUpdate` 增字段后，`crates/storage-sqlite/src/session_store.rs` 的位置参数调用与 `crates/storage-sqlite/tests/` 的 15 处 `SessionUpdate` 全字段字面量会编译失败（E0063），这些文件属 WP4 写范围，故由 WP4（W2）收口。窗口内 `PV1` 的 workspace 级结果**预期为红**，不得据此判定 WP3 未交付；以 WP3 的 crate 子集检查与 WP4 交付后的 workspace 级结果为准 | WP3 若改用给 `NewSession` 加字段而非 `SessionUpdate`，红窗口范围同步变化，仍由 WP4 收口 |

无其他代码依赖：WP1 与 WP2 只经冻结契约（design D1–D5）耦合，不互读代码。

## Runtime Resources

| Checks / Work Packages | Resource | Isolation / Configuration | Exclusive Scheduling | Allocate / Isolate / Start / Ready / Cleanup | Use / Cleanup Boundary |
| --- | --- | --- | --- | --- | --- |
| WP1–WP4、PV1 | 构建目录（`CARGO_TARGET_DIR`） | 每个 WP 一个独立目录（`target-wp<N>`），由 provisioner 分配并以环境变量注入 | NOT_APPLICABLE | provisioner 分配、创建、就绪检查与回收 | 执行者只在自己的目录内构建；不得删除或复用他人目录 |
| WP1–WP4、PV1 | Node 依赖（`node_modules`） | 只读共享（`npm ci` 已完成）；存在性已由 1.1 核实 | NOT_APPLICABLE | provisioner 保证 `npm ci` 已执行且版本与 lock 一致 | 只读使用，不写入 |

资源标识：`R1` = 构建目录（`CARGO_TARGET_DIR`），`R2` = Node 依赖（`node_modules`）。`plan.md` 的 Dependency Declaration Review 中的 `resource:R1` 即指本表 `R1` 行。

无数据库、端口、容器或外部服务需求；`cargo test` 使用临时目录，属执行者自建自清。

## Target Repository and Main Branch

- Code Repository: `D:\Project\acp-remote`
- Target Ref: `refs/heads/main`（规划时提交 `6c093f145aa3d69dcd573a6d94e31b692acb5d4b`）
- Version Confirmation Owner: 规划期只读调查由 scout（任务 1.1）执行；候选构建与合入瞬间的基线复核由 merger 执行；目标选择由 main 确认
- Confirmation Method / Evidence: 规划期读取 `.git/refs/heads/main` 的实际内容；合入期由 merger 用 `git rev-parse refs/heads/main` 复核并把引用与结果记入 `verification.md`。不默认 HEAD 即目标。

## Merge Strategy

| Delivery Unit / Mode | WP / TP | Owners: Implementation / Test / Review / Merge | Start / Readiness | Candidate Checks / Critical E2E | Order |
| --- | --- | --- | --- | --- | --- |
| U1（integrated） | WP1, WP2, WP3, WP4 | 实现 coder-vocab/coder-wire/coder-core/coder-storage；测试 NOT_APPLICABLE: 本单元无 tester 工作包（Main E2E 为 not-applicable，替代检查 ALT1/ALT2 由主 Agent 任务承担）；review reviewer-vocab/reviewer-wire/reviewer-core/reviewer-storage；merge merger-1 | 契约冻结于 `design.md@D1–D8`；WP4 需 `code:WP3` 进入集成基线 | PV1（`npm run verify`）；关键路径：命令目录六处一致、快照资源与新字段的门控、v6 迁移只追加且旧行可读 | 全部 WP 交付后按 **WP2 → WP1 → WP4 → WP3** 汇总到单元集成基线（与 Shared File Ownership 的 Merge Order 完全一致：`docs/SYNC_PROTOCOL.md`/`fixtures/**`/`crates/sync-protocol/src/` 三处由 WP2 先落、WP1 定稿，`docs/CORE_PORTS_AND_STORAGE.md` 由 WP4 先落、WP3 定稿），再建一次候选 |

- Merge Worktree: 由 provisioner 按 `(WP, Attempt)` 创建并交接，路径在接入时记入 `verification.md` 的 `## Worktree Handoff`；merger 不新建工作树
- Source Revision / Handoff: 固定各 WP 的已验收提交与全部包含关系证据（`verification.md` 的 Dependency Handoffs）
- Local Merge Conditions / Report Path: 候选通过 PV1、独立 review 与 Coverage Index 核对，且满足仓库规则（提交信息规范、分支保护要求）后直接本地合入；报告路径 `reports/merge-U1.md`

## Verification Strategy

### Local Checks

各 WP 在自己的 worktree 内按改动范围执行：`cargo fmt --all -- --check`；`cargo clippy --locked -p <crate> --all-targets --all-features -- -D warnings`；`cargo test --locked -p <crate> --all-features`；涉及合同资产时执行 `node scripts/check-command-catalog.mjs`、`node scripts/check-features.mjs`、`node scripts/check-contract-drift.mjs`、`node scripts/check-schema-fixtures.mjs`、`node scripts/check-contract-assets.mjs` 等定向脚本。局部检查不代替 PV1。

### Project Verify

| Check ID | Stages / Work Packages | Command / Working Directory | Configuration / Environment | Scope / Pass Criteria | Evidence |
| --- | --- | --- | --- | --- | --- |
| PV1 | 分支（各 WP，按下表的 crate 子集）、集成基线、候选、主分支 | `npm run verify`（= `npm run check` + `npm run check:rust`），工作目录 `D:\Project\acp-remote` | Node ≥ 22.12；Rust 工具链由 `rust-toolchain.toml` 固定；无需网络；构建目录由 provisioner 隔离 | 分支阶段：WP1/WP2/WP3/WP4 各自执行 `npm run check` + `cargo clippy/test -p <其拥有改动的 crate>`（见 Local Checks 与 Work Packages 的 Verification 列），**不含 workspace 级 clippy/test**，因为 WP3 交付到 WP4 落地之间存在已登记的 storage-sqlite 预期红窗口；集成基线/候选/主分支阶段：`npm run check` 十项合同门禁与 `cargo fmt/clippy/test --workspace --all-features` 全部通过；子检查未发现测试或全跳过不算 PASS | `reports/PV1.log`（含逐项门禁输出与退出码；分支阶段按 WP 分文件记录） |

### Code Review

reviewer-vocab / reviewer-wire / reviewer-core / reviewer-storage 各自以独立上下文只读检视对应 WP 的固定版本，使用 `roles/reviewer.md`。关注点与阻断标准：

- 命令目录六处（`commands.json`、两份协议 schema、`SYNC_PROTOCOL.md` §11.5、`SECURITY_DESIGN.md` §10.2、`broker::required_grant`）集合相等，`pack.create-session` 与 `identity-auth` 的 `PACKS` 逐项一致（阻断项）。
- 目录相关 schema、文档示例与 fixture 中不得出现规范化路径明文（阻断项）。
- `SessionSummary.workspace` 在 schema 中为可选、未协商时缺席；快照资源受 feature 门控与授权过滤（阻断项）。
- v6 迁移只做 `ALTER TABLE ... ADD COLUMN`，不触发表重建；旧行为 `NULL`；`docs/CORE_PORTS_AND_STORAGE.md` §7 与 `migrate.rs` 逐字一致（阻断项）。

候选阶段的 review 只审 rebase 新产生的冲突解决与新增交互；rebase 干净且改动内容指纹未变时可复用原结论并记录原报告编号。

### Main E2E

```yaml
mode: not-applicable
reason: 本变更是合同与类型层变更，仓库当前不存在可端到端运行的产品路径（server::sync、/ui 托管与前端工程均未落地），没有可被真实入口驱动的被测场景。
basis: openspec/config.yaml 的 context 记录「当前阶段没有可端到端运行的产品路径，Main E2E 按变更记 not-applicable」；openspec/agentic.yaml 的 e2e.command 为空。
alternative_checks: [ALT1, ALT2]
downgrade_approval: "已获用户批准。来源：本会话（2026-10-03）。用户原话：「同意 sync-workspaces-and-create 的 Main E2E 记为 not-applicable（2026-10-03，本会话）」。批准范围：仅本变更 `sync-workspaces-and-create` 的 Main E2E 判为 not-applicable，由替代检查 ALT1（npm run check）与 ALT2（cargo test --locked --workspace --all-features）在最终主分支承担覆盖；不构成对其它变更或对仓库 e2e 开关配置的批准。"
```

- ALT1：在最终主分支固定提交上运行 `npm run check`（十项合同门禁），全绿；证据 `reports/ALT1.log`。
- ALT2：在最终主分支固定提交上运行 `cargo test --locked --workspace --all-features`，全绿；证据 `reports/ALT2.log`。

## Independent Validation

| Task | Target Revision | Assignment | Pass Condition | Report Path |
| --- | --- | --- | --- | --- |
| 6.1 `[validation]` | 全部单元合入后的 `refs/heads/main` 固定提交 | validator-1：核对 Coverage Index 的覆盖充分性；验证 design 的假设（feature 门控下 `workspace` 字段缺席而非 `null`、v6 只追加且旧行 `NULL` 语义、无规范化路径出现在任何 fixture/schema 示例中）；指出风险盲区 | 每个 Coverage Index 行都有可读的实现与检查证据；未覆盖行为 0 或已如实记录；假设判定逐条给出 PASS/FAIL | `reports/validation-sync-workspaces-and-create.md` |

## Failure and Recovery

- 局部失败：由新的实现实例按原 WP ID 重新派发修复（`--reopen --reason`），重做上限 3 轮；修复后换新 reviewer 复核，并重跑受影响检查与 PV1。
- 合同或接口变化：先更新 `specs/`、`design.md`、本计划与 `tasks.md`，重评 Coverage Index 与受影响证据有效性，只暂停受影响工作。
- 资源异常：构建目录或 `node_modules` 被污染时由 provisioner 回收重建，执行者不自行处理共享资源。
- 回滚：本次不提供 v6 → v5 的降级工具；生产回滚需单独授权并先降级库形状（见 design 的 Migration Plan）。

## Completion Criteria

- 最终主分支固定提交上 PV1、独立验证（6.1）与替代检查 ALT1/ALT2 全部 PASS；`verification.md` 的 Coverage Index 逐行有可读证据。
- 无 FAIL / BLOCKED 项；`workflow check --stage final` 通过；由 `agentic-verify` 入口完成最终验收后才报告具备归档条件。
