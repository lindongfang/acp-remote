## Scope and Contracts

- Specs Revision: `specs/core-event-view-identity/spec.md`（本变更唯一增量规范，7 条 Requirement / 15 个 Scenario，全部为 ADDED；内容摘要：turn 归属注入、`turnId` 冲突 fail-closed、会话版本注入、版本规则漂移 fail-closed、保真与未知字段不变、重放不二次注入、`prompt` 接受结果不具权威性）
- Design Revision: `design.md`（D1 提交前定稿 turn 归属并以同一值注入；D2 版本推导 + 提交后 fail-closed 断言，备选为新增 `owned_event.session_version` 列；D3 `core::model::json` 的纯文本上位插入；D4 归属单源边界；D5 `TurnAccepted.turn` 语义写实；D6 接口/数据/环境约定）
- Convergence Check: **一致**。specs 的行为验收条件（注入时机、冲突与漂移的显式失败、保真约束）与 design 的接口（不改端口签名）、数据（不加列、不改 DDL）、环境（不新增依赖、无新运行资源）约定无冲突。核对结论：`SessionEndpoint::prompt` 签名不变 ⇒ 适配器无需改行为，`local-agent-host` 无需求变化（proposal 的 Modified Capabilities 为空）；`owned_event.turn_id` 已存在 ⇒ design 无需 DDL 变更。
- Skip Specs: no（存在规范层面的行为变化：新增 core 对 view 的身份/版本注入契约）

## Contract Changes

无合同资产变更（不新增/不修改 `schemas/**`、`fixtures/**`、`compatibility/**`；`docs/SYNC_PROTOCOL.md` §10.3 保持权威）。本变更只在 `docs/CORE_PORTS_AND_STORAGE.md` §5.1/§6/§9 与 `docs/MODULE_ARCHITECTURE.md` §4.1/§4.7 记录既有行为的**已明确化**（注入时机、版本规则、`TurnAccepted.turn` 语义），不改变端口签名或 DDL。

实现期登记了 5 条 Check Plan Change（第 5 条经 RV1 复核补登：PV3 判据按「适配器半 + core 半」的实测口径写明）（原因、原/新值与受影响任务见 `verification.md` 的 Check Plan Changes 段）：① imported 路径改为「保留 Owner 取值」而非「补齐」（端口无 turn 来源，且 `payloadDigest` 覆盖 Owner 字节，重写会破坏摘要）；② 版本推导/比对限定在含要求 `version` 的 view 的提交上，并写明漂移在存储返回后检测（不撤销已落盘行）——两条同时改了 `specs/core-event-view-identity/spec.md` 的文字与 scenario 标题，覆盖索引已同步；③ R7 的「适配器不返回 turn 标识」不可表达（`TurnAccepted.turn: TurnId` 必填）→ 改为占位值语义；④ WP3 实测发现 `storage-sqlite` 的状态变更分支未回填 `origin_epoch`（状态+事件同批时被拒），按 §5.2 的「存储层只校验已有 epoch 时必须一致」修 1 行并加回归用例。

## Coverage Index

```agentic-coverage
version: 1
target_ref: refs/heads/main
rows:
  - id: R1
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "### Requirement: 事件 view 的 turn 归属由 core 注入"
    tasks: ["2.3", "2.4", "2.7", "2.8"]
    checks: [PV2]
    evidence: [reports/wp2-core-injection.log]
  - id: R2
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "#### Scenario: 适配器未产出时由 core 补齐"
      requirement: "### Requirement: 事件 view 的 turn 归属由 core 注入"
    tasks: ["2.3", "2.7", "2.8"]
    checks: [PV2]
    evidence: [reports/wp2-core-injection.log]
  - id: R3
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "#### Scenario: 无 turn 归属的事件不伪造字段"
      requirement: "### Requirement: 事件 view 的 turn 归属由 core 注入"
    tasks: ["2.3", "2.7", "2.8"]
    checks: [PV2]
    evidence: [reports/wp2-core-injection.log]
  - id: R4
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "#### Scenario: imported 路径保留 Owner 注入的归属"
      requirement: "### Requirement: 事件 view 的 turn 归属由 core 注入"
    tasks: ["2.4", "2.7"]
    checks: [PV2]
    evidence: [reports/wp2-core-injection.log]
  - id: R5
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "### Requirement: 已存在的 turnId 冲突必须显式失败"
    tasks: ["2.2", "2.7"]
    checks: [PV2]
    evidence: [reports/wp2-core-injection.log]
  - id: R6
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "#### Scenario: 取值一致时保持原字段"
      requirement: "### Requirement: 已存在的 turnId 冲突必须显式失败"
    tasks: ["2.2", "2.7"]
    checks: [PV2]
    evidence: [reports/wp2-core-injection.log]
  - id: R7
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "#### Scenario: 取值不一致时无副作用地失败"
      requirement: "### Requirement: 已存在的 turnId 冲突必须显式失败"
    tasks: ["2.2", "2.7"]
    checks: [PV2]
    evidence: [reports/wp2-core-injection.log]
  - id: R8
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "### Requirement: 会话版本字段与提交结果一致"
    tasks: ["2.5", "2.7", "2.9"]
    checks: [PV2, PV5]
    evidence: [reports/wp2-core-injection.log, reports/wp3-storage-version.log]
  - id: R9
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "#### Scenario: 状态变更提交后的版本"
      requirement: "### Requirement: 会话版本字段与提交结果一致"
    tasks: ["2.5", "2.7", "2.9"]
    checks: [PV2, PV5]
    evidence: [reports/wp2-core-injection.log, reports/wp3-storage-version.log]
  - id: R10
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "#### Scenario: 纯事件提交不递增版本"
      requirement: "### Requirement: 会话版本字段与提交结果一致"
    tasks: ["2.5", "2.7"]
    checks: [PV2]
    evidence: [reports/wp2-core-injection.log]
  - id: R11
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "#### Scenario: imported 事件不重复注入"
      requirement: "### Requirement: 会话版本字段与提交结果一致"
    tasks: ["2.4", "2.7"]
    checks: [PV2]
    evidence: [reports/wp2-core-injection.log]
  - id: R12
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "### Requirement: 版本规则的漂移必须显式失败"
    tasks: ["2.5", "2.7", "2.9"]
    checks: [PV2, PV5]
    evidence: [reports/wp2-core-injection.log, reports/wp3-storage-version.log]
  - id: R13
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "#### Scenario: 推导值一致时按注入值提交"
      requirement: "### Requirement: 版本规则的漂移必须显式失败"
    tasks: ["2.5", "2.7", "2.9"]
    checks: [PV2, PV5]
    evidence: [reports/wp2-core-injection.log, reports/wp3-storage-version.log]
  - id: R14
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "#### Scenario: 推导值不一致时中止"
      requirement: "### Requirement: 版本规则的漂移必须显式失败"
    tasks: ["2.5", "2.7"]
    checks: [PV2]
    evidence: [reports/wp2-core-injection.log]
  - id: R15
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "### Requirement: 注入不改变 ACP 保真与既有字段"
    tasks: ["2.7", "2.8"]
    checks: [PV2]
    evidence: [reports/wp2-core-injection.log]
  - id: R16
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "#### Scenario: ACP 三要素逐字节不变"
      requirement: "### Requirement: 注入不改变 ACP 保真与既有字段"
    tasks: ["2.7", "2.8", "2.10"]
    checks: [PV2, PV3]
    evidence: [reports/wp2-core-injection.log, reports/wp4-agent-host-contract.log]
  - id: R17
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "#### Scenario: 未知字段与嵌套结构保留"
      requirement: "### Requirement: 注入不改变 ACP 保真与既有字段"
    tasks: ["2.7", "2.8"]
    checks: [PV2]
    evidence: [reports/wp2-core-injection.log]
  - id: R18
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "### Requirement: 幂等重放不得二次注入"
    tasks: ["2.7"]
    checks: [PV2]
    evidence: [reports/wp2-core-injection.log]
  - id: R19
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "#### Scenario: 重放返回与首次持久化相同的字节"
      requirement: "### Requirement: 幂等重放不得二次注入"
    tasks: ["2.7"]
    checks: [PV2]
    evidence: [reports/wp2-core-injection.log]
  - id: R20
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "### Requirement: prompt 接受结果不是 turn 归属的权威来源"
    tasks: ["2.6", "2.7"]
    checks: [PV2]
    evidence: [reports/wp2-core-injection.log]
  - id: R21
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "#### Scenario: 适配器返回不同占位值不影响归属"
      requirement: "### Requirement: prompt 接受结果不是 turn 归属的权威来源"
    tasks: ["2.6", "2.7"]
    checks: [PV2]
    evidence: [reports/wp2-core-injection.log]
  - id: R22
    source:
      path: specs/core-event-view-identity/spec.md
      heading: "#### Scenario: 适配器返回占位值时归属仍由 core 决定"
      requirement: "### Requirement: prompt 接受结果不是 turn 归属的权威来源"
    tasks: ["2.6", "2.7"]
    checks: [PV2]
    evidence: [reports/wp2-core-injection.log]
```

## Work Packages

| ID | Goal / Scenarios | Dependencies | Owner | Reviewer | Branch / Worktree | Write Scope | Inputs / Outputs | Verification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | 把注入时机、版本规则与 `TurnAccepted.turn` 语义写进权威文档（覆盖 R1/R8/R12/R20 的契约面） | 无 | 实现 Agent | 独立 reviewer（非文档作者） | 本地分支 `feat/core-turn-view-fields`，主工作区 | `docs/CORE_PORTS_AND_STORAGE.md`（§5.1/§6/§9）、`docs/MODULE_ARCHITECTURE.md`（§4.1/§4.7） | 输入：`design.md` D1/D2/D5/D6、`specs/core-event-view-identity/spec.md`；输出：文档改动 + `reports/wp1-contract-docs.log` | `npm run check`（`[PV1]`）exit 0 |
| WP2 | core 侧实现与回归：文本级注入工具、提交前 turn 归属定稿、owned/imported 注入、版本推导与 fail-closed 断言、`TurnAccepted.turn` 语义固化、全部行为用例 | WP1 的契约口径（可并行，接口以 design D6 为准） | 实现 Agent | 独立 reviewer（非本 WP 作者） | 同上 | `crates/core/src/model/json.rs`、`crates/core/src/broker.rs`、`crates/core/src/model/event.rs`（注释）、`crates/core/src/ports.rs`（仅注释） | 输入：`design.md` D1–D5、`specs/core-event-view-identity/spec.md`；输出：实现 + core 用例 + `reports/wp2-core-injection.log` | `cargo test --locked -p core --all-features`（`[PV2]`）exit 0 |
| WP3 | 真实存储层验证：在真实 SQLite 上验证版本规则（含状态变更 +1、否则不变）与 `CommitOutcome.version` 一致、漂移可检测（覆盖 R8/R9/R12/R13），并修掉实测发现的 `origin_epoch` 未回填缺陷 | WP2 的实现接口 | 实现 Agent | 独立 reviewer | 同上 | `crates/storage-sqlite/tests/**`（新增测试）+ `crates/storage-sqlite/src/session_store.rs`（仅 `Update` 分支回填 `origin_epoch` 的一处修复；不得改 DDL/migrate.rs） | 输入：WP2 交付的注入与断言、`CommitOutcome.version` 语义；输出：集成用例 + `reports/wp3-storage-version.log` | `cargo test --locked -p storage-sqlite --all-features`（`[PV5]`）exit 0 |
| WP4 | 跨 crate 契约核验：用真实适配器输出断言「适配器不产 `turnId`/`version`、且其余 §10.3 最低字段齐备（即 ∪ core 注入后满足 §10.3）」并断言 core 独占类型不被适配器重复生成（覆盖 R16 与整体分工） | WP2、WP3 | 实现 Agent | 独立 reviewer | 同上 | `crates/agent-host/tests/**`（新增契约用例）+ `crates/agent-host/src/lib.rs`（仅边界说明注释：原「已知缺口」已关闭） | 输入：WP2 的 core 行为、`crates/agent-host` 的真实 `SessionEndpoint` 与 fake ACP 子进程；输出：契约用例 + `reports/wp4-agent-host-contract.log` | `cargo test --locked -p core -p agent-host --all-features`（`[PV3]`）exit 0 |

## Execution Waves

- **W1**：WP1（`docs/**`）∥ WP2（`crates/core/**`）——写范围不重叠，接口以 design D6 冻结，可并行。
- **W2**：WP3（`crates/storage-sqlite/tests/**`）∥ WP4（`crates/agent-host/tests/**`）——两者依赖 WP2 的实现落地（不是契约），文件互不重叠；共享 `target/`，由单一写入者按顺序跑测试（见 Runtime Resources）。
- 并发上限：2 个 WP 并行；`crates/core/src/broker.rs` 只允许 WP2 写入（单写者），任何 review 修复按原 WP ID 重新派发。
- 每批结束即执行该批的交付前检查（PV1/PV2/PV3/PV5）并留证，不等全部完成。

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| WP3 | WP2 | 规划阶段：core 的注入口径（design D1/D2/D6）与 `CommitOutcome.version` 语义；接入前需引用 `reports/wp2-core-injection.log` 的通过证据 | 本地主分支当前提交（apply 时核实） | WP3 只经 `core::ports` 的 `SessionStore` 抽象与 `OwnedCommit`/`CommitOutcome` 驱动真实存储，不复制 broker 内部逻辑；包含关系检查：`cargo test -p storage-sqlite` 必须能编译出引用 `acp_core::ports` 的集成用例 | core 的提交/版本语义或端口签名变化 ⇒ WP3 重跑 |
| WP4 | WP2、WP3 | 规划阶段：适配器产出 ACP 派生字段、core 补身份/版本的分工（design D1/D6）；接入前需引用 WP2/WP3 的通过证据 | 同上 | WP4 只用 `agent-host` 的真实 `SessionEndpoint` + fake ACP 子进程与 core 的模型类型，不复制注入逻辑（broker 的真实提交已由 WP2 用例覆盖）；包含关系检查：`cargo test -p agent-host` 能编译并跑通 `tests/view_contract.rs` | core 注入语义或 `agent-host` 的 view 产出变化 ⇒ WP4 重跑 |

## Runtime Resources

| Checks / Work Packages | Resource | Isolation / Configuration | Exclusive Scheduling | Owner / Cleanup |
| --- | --- | --- | --- | --- |
| WP2（`[PV2]`）、WP3（`[PV5]`）、WP4（`[PV3]`）、`[PV1]`/`[PV4]` | `target/` 构建目录（cargo 共享）、测试用临时 SQLite 文件 | 测试库文件由用例在系统临时目录自建自清（沿用 `storage-sqlite` 既有模式）；若并发跑测试分片，须显式隔离 `CARGO_TARGET_DIR` | 不隔离时必须串行：同一时间只允许一个执行者运行 cargo 测试/构建 | 实现 Agent 分配；异常时释放临时文件与目录；只清理自身资源 |

无数据库服务、端口、容器、外部账号或网络依赖（`cargo-deny`/`gitleaks` 仍只在 CI 判定，本地无等价物，不得声称通过）。

## Target Repository and Main Branch

- Code Repository: `D:/Project/acp-remote`
- Target Kind / Ref: `local` / `refs/heads/main`（不推送远端；远端操作需另行授权）
- Version Confirmation Owner: 主 Agent（机械核实可交 environment/recon）
- Confirmation Method / Evidence: `git rev-parse refs/heads/main` 与 `git rev-parse HEAD`（两者必须一致，并记录查询时间）；`git status --porcelain` 必须无被跟踪文件改动；实际提交与核实结果写入 `verification.md`

## Merge Strategy

| Delivery Unit / Mode | WP / TP | Owners: Implementation / Test / Review / Merge | Start / Readiness | Candidate Checks / Critical E2E | Order |
| --- | --- | --- | --- | --- | --- |
| DU1 / **integrated** | WP1–WP4（同一变更内接口依赖：WP1 契约口径 → WP2 实现 → WP3/WP4 集成验证） | 实现：实现 Agent（按 WP 单写者）；测试：随 WP 内的回归用例（本变更 Main E2E 为 not-applicable，不生成 TP）；Review：独立 reviewer（每个 WP 一个隔离上下文 + DU1 候选/合并 review）；Merge：主 Agent | 契约：`specs`/`design` 收敛（已核对）；实现：WP1/WP2 可开工；资源：无独占资源，`target/` 串行 | 候选：`npm run verify`（`[PV4]`）+ `cargo test -p core -p agent-host`（`[PV3]`）；关键需求路径：turn 注入、版本注入与漂移失败、保真不变 | 单一交付单元，按 WP1→WP2→（WP3∥WP4）→DU1 合入 |

- Integration Branch / Worktree: 本地分支 `feat/core-turn-view-fields`；集成 Agent 使用独立链接 worktree（仓库外路径，用后清理）
- Source Revision / Handoff: apply 开始时固定 `refs/heads/main` 的实际提交为基线；WP1/WP2 完成后交回主 Agent 调度 WP3/WP4
- Operation Boundary / Report Path: apply 覆盖**本地**合入；不推送、不发布、不回滚；集成报告路径 `reports/du1-integration.md`

## Verification Strategy

### Local Checks

开发中的快速检查（不留证要求）：`cargo fmt --all`、`cargo clippy --locked -p core --all-targets --all-features -- -D warnings`、`cargo test --locked -p core --all-features`；文档改动后跑 `npm run check:docs` 与 `npm run check:boundaries`。

### Project Verify

| Check ID | Stages / Work Packages | Command / Working Directory | Configuration / Environment | Scope / Pass Criteria | Evidence |
| --- | --- | --- | --- | --- | --- |
| PV1 | WP1（交付前）、DU1（候选与主分支）、最终替代验证 | `npm run check`（`D:/Project/acp-remote`；统一入口含十道合同门禁：schemas/commands/errors/features/assets/acp/docs/boundaries/drift/agentic） | Node ≥ 22.12；不需要网络；`cargo metadata` 供 `check:boundaries`；`check:drift` 比对 §7 DDL 与 §5 端口签名 | 十道门禁逐道 exit 0；`check:boundaries` 必须仍报「8 个 crate」且 core 依赖闭包等于冻结 allow-list（本变更不得改 allow-list）；`check:drift` 必须仍与 `crates/storage-sqlite/src/migrate.rs`、`crates/core/src/ports.rs` 一致（本变更不得改端口签名或 DDL） | `reports/wp1-contract-docs.log` |
| PV2 | WP2（交付前）、DU1（候选与主分支）、最终替代验证 | `cargo test --locked -p core --all-features`（`D:/Project/acp-remote`） | rustc 以 `rust-toolchain.toml` 为准；无网络（`--locked`）；内存 fake 存储 + broker harness | exit 0 且无失败/零用例/整体跳过；必须覆盖：注入补齐（R2）、无归属不伪造（R3）、imported 保留 Owner 给出的取值（R4/R11；不注入、不重写、不补齐）、冲突一致与失败（R6/R7）、版本一致与漂移失败（R9/R10/R13/R14）、保真与未知字段（R16/R17）、重放字节一致（R19）、`TurnAccepted.turn` 不采信（R21/R22） | `reports/wp2-core-injection.log` |
| PV3 | WP4（交付前）、DU1、最终替代验证 | `cargo test --locked -p core -p agent-host --all-features` | 同上；跨 crate（`agent-host` 依赖 `core`） | exit 0；必须覆盖（两半合起来才是 R16 的完整证据，见 Check Plan Change 5）：① 适配器产出的 view 不含 `turnId`/`version`，且其余 §10.3 最低字段齐备、ACP 三要素在适配器侧一致（`crates/agent-host/tests/view_contract.rs`）；② core 注入后 view 满足 §10.3、ACP 三要素逐字节不变（PV2 的 core 用例） | `reports/wp4-agent-host-contract.log` |
| PV4 | DU1（候选、主分支）、最终替代验证 | `npm run verify`（= `npm run check` + `cargo fmt --all -- --check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` + `cargo test --locked --workspace --all-features`） | 本机 Windows x64、rustc 1.98.1、Node 24.19.0 | 整条入口 exit 0；全工作区测试无失败（既有 2 条 `#[ignore]` 带理由属其它 crate，不得新增） | `reports/du1-pv1.log`、`reports/du1-main-verify.log` |
| PV5 | WP3（交付前）、DU1、最终替代验证 | `cargo test --locked -p storage-sqlite --all-features` | 真实 SQLite（临时文件）；`--locked` | exit 0；必须覆盖：真实 store 下 `version = version + 1` 与 core 注入值一致、无状态提交不递增、core 的漂移断言在真实实现上不误报（R8/R9/R12/R13） | `reports/wp3-storage-version.log` |

### Code Review

- **WP1**：独立 reviewer 核对文档口径与实现/规范一致（注入时机、版本规则、`TurnAccepted.turn` 语义），不得夸大「已实现」；阻断标准：与 `specs`/`design` 或代码事实不符、把计划写成现状。报告 `reports/rv1-wp1.md`。
- **WP2**：独立 reviewer 核对注入实现（是否真的在提交前、是否可能双源、是否可能静默覆盖既有字段、重放路径是否二次注入）、失败关闭路径（冲突/漂移是否真的无副作用）、保真约束（ACP 三要素、未知字段），并**亲自**用变异自检（改坏实现 → 对应用例必须变红 → 还原）验证断言强度，不接受实现者自述。报告 `reports/rv1-wp2.md`。
- **WP3/WP4**：独立 reviewer 核对集成用例是否真的经过真实 store / 真实适配器（不是再包一层 fake），以及断言是否可证伪。报告 `reports/rv1-wp3.md`、`reports/rv1-wp4.md`。
- **DU1**：独立 reviewer 复核候选版本全区间（`agent-host`/`storage-sqlite` 测试改动是否只加断言不改行为、`crates/core` 是否仍不引入被禁依赖、`docs` 与代码是否一致），以及合并是否引入候选之外的新差异。报告 `reports/rv1-du1.md`。
- 每个 WP 的 reviewer 必须是**新的隔离上下文**，且不得是本 WP 的作者；CRITICAL/MAJOR 必须有复核闭环。

### Main E2E

```yaml
mode: not-applicable
reason: "项目当前没有可端到端运行的产品路径（`server`/`app`/`identity-*`/前端尚未实现，无 Daemon/CLI/Sync 入口）；本次改动落在 core 库内部的 broker 提交与 view 组装，唯一真实入口是库 API，没有可驱动的端到端场景。"
basis: "`openspec/config.yaml` 的 `x-agentic.e2e` 实测为 enabled=true、command=''（`npx --quiet --no-install openspec-agentic e2e --json`）；同文件记录 2026-09-23 用户确认「保持命令为空、首个实现切片在 plan.md 写 downgrade_approval」，并明确每个切片须各自记录当次批准；本变更按该约定逐变更取得批准。"
alternative_checks:
  - "`npm run verify`（`[PV4]`）：十道合同门禁 + fmt + clippy + 全工作区测试，覆盖依赖集/端口签名/DDL/矩阵未漂移与无回归；证据 `reports/du1-main-verify.log`"
  - "`cargo test --locked -p core --all-features`（`[PV2]`）：注入、冲突、版本一致与漂移失败、重放、保真与未知字段的全部行为用例；证据 `reports/wp2-core-injection.log`"
  - "`cargo test --locked -p storage-sqlite --all-features`（`[PV5]`）：真实存储层上的版本规则与注入一致性；证据 `reports/wp3-storage-version.log`"
  - "`cargo test --locked -p core -p agent-host --all-features`（`[PV3]`）：适配器产出不含 `turnId`/`version` 且其余 §10.3 最低字段齐备（结合 `[PV2]` 的 core 注入用例）；证据 `reports/wp4-agent-host-contract.log`"
downgrade_approval: "用户 2026-09-24 本会话批准原话：「同意本变更 Main E2E 记 not-applicable」（来源：本变更启动前的范围确认提问第 1 项；同一条回复第 2 项为「同意」（同意把 RV-DU1-F6 纳入本变更））"
```

不生成 TP 与 E2E 用例设计（not-applicable）；替代验证的四个检查在 Final E2E 阶段逐项执行并留证。

## Failure and Recovery

- 失败回路：任一 PV 失败即在该 WP 内以原 WP ID 派发修复，修复后重跑该 PV 与受影响的下游 PV；不得只改断言让绿灯通过（断言弱化必须记录理由与受影响的需求行）。
- 传递失效：`crates/core` 的注入语义或端口/版本语义变化 ⇒ WP3/WP4 的用例与证据重跑；`specs/core-event-view-identity/spec.md` 的任何 Requirement/Scenario 改动 ⇒ Coverage Index 与受影响任务同步后重跑 plan 阶段检查。
- 主分支回滚条件：合入后 `[PV4]` 在主分支失败且无法在变更内修复时，用 `git revert` 回滚该合入提交（本地），保留现场与日志，并把问题、已尝试方案与证据交用户决策。
- 共享资源异常：若 cargo 构建目录被污染（编译产物与源码不一致导致假失败），按 Runtime Resources 重新串行执行并重跑受影响检查，记录重跑原因；临时 SQLite 文件由用例自清，异常残留由该 WP 的 owner 清理。
- 本变更 Main E2E 为 not-applicable，不涉及 E2E 重试上限。

## Completion Criteria

- `verification.md` 必须记录：每个 PV 的完整命令、目录、退出码、环境与日志路径；每个 WP 的独立 review 报告与结论；DU1 的候选/合入/主分支三段提交与主分支复跑；替代验证四项结果；失败与复验历史（含因预期变化而更新的断言）；未解决项（非阻断）与处置结论。
- 最终主分支版本固定为 apply 结束时的 `refs/heads/main` 实际提交，且 `HEAD == refs/heads/main`、工作区无被跟踪文件改动。
- 阻断问题清零：无未解决的 CRITICAL/MAJOR review 发现；无失败/受阻的 PV；`workflow check --stage final` 与 `e2e check` 判 PASS 后，由 `[final-verification]` 任务按 `.agents/skills/agentic-verify/SKILL.md` 完成最终验收。
- 本计划不代表已获得推送、回滚或发布授权；归档按 `.agents/skills/agentic-verify/SKILL.md` 与归档流程另行执行。
