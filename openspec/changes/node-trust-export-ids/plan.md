<!-- 主 Agent 维护协作计划；规则见 schema.yaml，角色细节见 roles/。
     本文件写安排，tasks 写步骤，verification 写执行历史。 -->

## Scope and Contracts

- Specs Revision: 本变更三份增量规范（未提交，内容摘要）：`specs/node-link-owner-server/spec.md` = MODIFIED「Export 过滤的 catalog 投影」（可见性改三条同时成立 + 4 个新场景）；`specs/local-admin-methods/spec.md` = MODIFIED「节点配对与信任方法」（`node.pair.confirm` 的 `exportIds` 必填、三类失败、回显）；`specs/storage-schema-v2-migration/spec.md` = MODIFIED「版本常量与 migration 幂等」（文件格式与 owned 家族 → 4、`owned_node` 末尾追加 `export_ids_json`、旧行置空）。共 3 条需求、15 个场景（覆盖索引 R1–R18：15 行场景 + 3 行需求）。
- Design Revision: `design.md`（未提交，v1）：D1 三条件收窄与两个判定点同口径、D2 JSON 列而非关联表、D3 `ALTER TABLE` + 末尾追加与常量 4/4/3、D4 校验在 `settle_pairing` 事务内、D5 仅本机管理面且 `schemas/`+`fixtures/` 无需改动、D6 CLI 警告不阻断、D7 不新增审计动作、D8 兼容与已知限制、D9 测试策略、D10 影响面。
- Convergence Check: specs 与 design 一致，无待澄清项——四项语义由用户在 2026-09-27 本会话裁决（① 收窄 ② 方案 A ③ 撤销保留 ④ 拒绝无效条目），三项小决策同批确定（`exportIds` 必填、Access 侧孤儿 Import 只文档化、CLI 空清单警告不阻断），`Main E2E` 降级另经用户批准（见下）。无受影响下游。
- Skip Specs: no

## Contract Changes

本变更改变三处合同面（细节与理由见 `design.md` D1/D3/D5；原/新值与影响分析在 apply 开始时写入 `verification.md` 的 Check Plan Changes）：

- `docs/NODE_LINK_PROTOCOL.md` §8.2：信任记录增 `exportIds`，删除「首阶段不使用」的推后口径；可见性改为三条件；写清「目前没有修改入口，改 = 撤销重配」与升级副作用。
- `docs/CORE_PORTS_AND_STORAGE.md`：§3.5（`NodeRecord`/`PairingSettlement`）、§7.2（v3 → v4 段与常量 4/4/3）、§7.3（`owned_node` DDL 末尾加列）、§9 判据 28、§11.6 第 4 条、§11.7/§11.8 说明句。
- `docs/LOCAL_ADMIN_PROTOCOL.md` §5.4：`NodeRecord` 增 `exportIds`、`node.pair.confirm` 的 `params` 增必填 `exportIds`、`result` 回显；按 §8 记为 v1 内合同修订（先例：`node.challenge.catalogRevision`）。

`schemas/local-admin/v1/`、`fixtures/local-admin/v1/`、`compatibility/commands/v1/commands.json` **不改**（方法集、错误码、`local.*` 能力与逐方法形状的拥有者都不变；已核实 envelope schema 只把 `params`/`result` 建模为通用对象）。

## Coverage Index

```agentic-coverage
version: 1
target_ref: refs/heads/main
rows:
  - id: R1
    source:
      path: specs/node-link-owner-server/spec.md
      heading: "#### Scenario: 按信任记录过滤"
      requirement: "### Requirement: Export 过滤的 catalog 投影"
    tasks: ["2.3", "3.1", "7.1"]
    checks: [PV1, PV4, PV5]
    evidence: [reports/du1-pv1.log, reports/pv5-windows-nodelink.log]
  - id: R2
    source:
      path: specs/node-link-owner-server/spec.md
      heading: "#### Scenario: 清单收窄可见集"
      requirement: "### Requirement: Export 过滤的 catalog 投影"
    tasks: ["2.2", "2.3", "3.1", "7.1"]
    checks: [PV1, PV4, PV5]
    evidence: [reports/du1-pv1.log, reports/pv5-windows-nodelink.log]
  - id: R3
    source:
      path: specs/node-link-owner-server/spec.md
      heading: "#### Scenario: 空清单看不到任何 Export"
      requirement: "### Requirement: Export 过滤的 catalog 投影"
    tasks: ["2.2", "2.3", "3.1", "7.1"]
    checks: [PV1, PV4, PV5]
    evidence: [reports/du1-pv1.log, reports/pv5-windows-nodelink.log]
  - id: R4
    source:
      path: specs/node-link-owner-server/spec.md
      heading: "#### Scenario: 清单内 Export 被撤销后不可见"
      requirement: "### Requirement: Export 过滤的 catalog 投影"
    tasks: ["2.3", "3.1", "7.1"]
    checks: [PV1, PV4, PV5]
    evidence: [reports/du1-pv1.log, reports/pv5-windows-nodelink.log]
  - id: R5
    source:
      path: specs/node-link-owner-server/spec.md
      heading: "#### Scenario: 超大批次稳定切分"
      requirement: "### Requirement: Export 过滤的 catalog 投影"
    tasks: ["2.3", "3.1", "7.1"]
    checks: [PV1, PV4, PV5]
    evidence: [reports/du1-pv1.log, reports/pv5-windows-nodelink.log]
  - id: R6
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: Owner 模式节点配对"
      requirement: "### Requirement: 节点配对与信任方法"
    tasks: ["2.1", "2.2", "2.5", "3.1"]
    checks: [PV1, PV3]
    evidence: [reports/du1-pv1.log]
  - id: R7
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: 确认时填报可见清单"
      requirement: "### Requirement: 节点配对与信任方法"
    tasks: ["2.1", "2.2", "2.4", "2.5", "3.1"]
    checks: [PV1, PV3, PV4]
    evidence: [reports/du1-pv1.log]
  - id: R8
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: 空清单仍然建立信任"
      requirement: "### Requirement: 节点配对与信任方法"
    tasks: ["2.2", "2.3", "2.5", "3.1"]
    checks: [PV1, PV3]
    evidence: [reports/du1-pv1.log]
  - id: R9
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: 无效清单条目拒绝确认"
      requirement: "### Requirement: 节点配对与信任方法"
    tasks: ["2.1", "2.2", "2.5", "3.1"]
    checks: [PV1, PV3]
    evidence: [reports/du1-pv1.log]
  - id: R10
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: access 模式明确不支持"
      requirement: "### Requirement: 节点配对与信任方法"
    tasks: ["2.5", "3.1"]
    checks: [PV1]
    evidence: [reports/du1-pv1.log]
  - id: R11
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "#### Scenario: 连续两次打开 schema 文本不变"
      requirement: "### Requirement: 版本常量与 migration 幂等"
    tasks: ["2.1", "3.1"]
    checks: [PV1, PV3]
    evidence: [reports/du1-pv1.log]
  - id: R12
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "#### Scenario: 升级中途失败整体回滚"
      requirement: "### Requirement: 版本常量与 migration 幂等"
    tasks: ["2.1", "3.1"]
    checks: [PV1, PV3]
    evidence: [reports/du1-pv1.log]
  - id: R13
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "#### Scenario: v3 到 v4 升级给既有节点行写空清单"
      requirement: "### Requirement: 版本常量与 migration 幂等"
    tasks: ["2.1", "3.1", "7.1"]
    checks: [PV1, PV3, PV5]
    evidence: [reports/du1-pv1.log, reports/pv5-windows-nodelink.log]
  - id: R14
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "#### Scenario: 升级库与新建库的 owned 列清单相等"
      requirement: "### Requirement: 版本常量与 migration 幂等"
    tasks: ["2.1", "3.1"]
    checks: [PV1, PV3]
    evidence: [reports/du1-pv1.log]
  - id: R15
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "#### Scenario: v2 到 v3 升级保留审计并扩展词表"
      requirement: "### Requirement: 版本常量与 migration 幂等"
    tasks: ["2.1", "3.1"]
    checks: [PV1, PV3]
    evidence: [reports/du1-pv1.log]
  - id: R16
    source:
      path: specs/node-link-owner-server/spec.md
      heading: "### Requirement: Export 过滤的 catalog 投影"
    tasks: ["2.2", "2.3", "3.1"]
    checks: [PV1, PV4]
    evidence: [reports/du1-pv1.log]
  - id: R17
    source:
      path: specs/local-admin-methods/spec.md
      heading: "### Requirement: 节点配对与信任方法"
    tasks: ["2.1", "2.2", "2.4", "3.1"]
    checks: [PV1, PV3, PV4]
    evidence: [reports/du1-pv1.log]
  - id: R18
    source:
      path: specs/storage-schema-v2-migration/spec.md
      heading: "### Requirement: 版本常量与 migration 幂等"
    tasks: ["2.1", "3.1"]
    checks: [PV1, PV3]
    evidence: [reports/du1-pv1.log]
  - id: R19
    source:
      path: specs/local-admin-methods/spec.md
      heading: "#### Scenario: 重新配对收窄清单后作废既有连接"
      requirement: "### Requirement: 节点配对与信任方法"
    tasks: ["2.3", "2.4", "3.1", "7.1"]
    checks: [PV4, PV5]
    evidence: [reports/wp4-app.log, reports/pv5-windows-nodelink.log]
```

## Work Packages

| ID | Goal / Scenarios | Dependencies | Owner | Reviewer | Branch / Worktree | Write Scope | Inputs / Outputs | Verification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | 存储：v3 → v4 迁移、`owned_node` 清单列读写、`settle_pairing` 事务内校验（R11–R15、R6–R9 的落盘面） | 无（契约已冻结） | 实现 Agent（coder），独立 worktree | 独立 reviewer（不继承实现对话） | `agentic/node-trust-export-ids`（独立 worktree） | `crates/storage-sqlite/src/migrate.rs`、`crates/storage-sqlite/src/admin/**`、`crates/storage-sqlite/tests/{migration,admin_store}.rs` | 输入：`CORE_PORTS_AND_STORAGE.md` §7.2/§7.3/§9、`design.md` D2/D3/D4；输出：新 DDL 常量、v4 段、校验与编解码、迁移/写集用例 | [PV3] |
| WP2 | core：`NodeRecord.export_ids`、`PairingSettlement::Approved.granted_export_ids`、Owner 侧授权判定加清单条件（R1–R4、R6–R9） | WP1 的形状（并行编写，接口以 §3.5 冻结形状为准） | 实现 Agent（coder） | 独立 reviewer | 同上 | `crates/core/src/model/**`、`crates/core/src/{broker.rs,use_cases.rs,ports.rs}`、`crates/core/src/model/tests.rs` | 输入：`design.md` D1/D4/D5；输出：模型字段与构造校验、授权条件、单测 | [PV1] |
| WP3 | server：`visible_exports` 加第三个条件、catalog/resource 结论一致（R1–R5） | WP2 的模型字段 | 实现 Agent（coder） | 独立 reviewer | 同上 | `crates/server/src/node_link/catalog.rs`、`crates/server/src/node_link/**` 的测试 | 输入：`design.md` D1；输出：单一判定点实现与用例 | [PV4] |
| WP4 | app：CLI `node pair confirm --export-id`、空清单警告、`node list` 展示（R7、R8） | WP1/WP2 的对称形状 | 实现 Agent（coder） | 独立 reviewer | 同上 | `crates/app/src/cli/**` | 输入：`LOCAL_ADMIN_PROTOCOL.md` §5.4；输出：参数收集、警告、展示与 CLI 用例 | [PV4] |
| WP5 | 文档与规范：三处合同面同步、主规范 Purpose 勘误、`NODE_LINK_PROTOCOL` 修订记录（全部 R） | 与 WP1–WP4 并行，交付前收口 | 实现 Agent（coder）；主 Agent 核对 | 独立 reviewer（文档一致性） | 同上 | `docs/NODE_LINK_PROTOCOL.md`、`docs/CORE_PORTS_AND_STORAGE.md`、`docs/LOCAL_ADMIN_PROTOCOL.md`、`openspec/specs/storage-schema-v2-migration/spec.md`（仅 Purpose 勘误，呈 tasks.md 2.5） | 输入：proposal/design/specs；输出：与实现和门禁一致的文档 | [PV1]（`check:docs`/`check:drift`） |

## Execution Waves

- Wave 0（并行起步）：WP2（core 模型字段与构造校验）与 WP5（文档文本）可并行；WP1 的 DDL 常量文本需要与 WP5 的 §7.3 同步，由 WP1 负责写入代码侧、WP5 负责文档侧，两处文本以 `design.md` D3 为唯一口径（漂移门禁在交付前断言相等）。
- Wave 1：WP1（迁移与写集校验）在 WP2 的模型字段可用后完成；WP3/WP4 在 WP1 的形状冻结后开始。
- Wave 2：WP3、WP4 并行收尾。
- 共享文件单一写入者：`crates/core/src/{ports.rs,use_cases.rs}` 只由 WP2 写；`crates/storage-sqlite/src/admin/**` 只由 WP1 写；`docs/**` 只由 WP5 写。并发上限 = 3 个 worktree（构建隔离用独立 `CARGO_TARGET_DIR`）。

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| WP1 | WP2 | 规划阶段：`CORE_PORTS_AND_STORAGE.md` §3.5 的冻结形状；接入前引用 verification 中已验收的 WP2 提交 | 主分支 `3cadb12d` | `cargo build -p storage-sqlite` 只经公开类型消费 core | core 模型字段或构造规则变化 → WP1 复验 |
| WP3、WP4 | WP1、WP2 | 同上：§5/§7 的端口与 DDL 形状 | 主分支 `3cadb12d` | `cargo build -p server -p app` 只经公开项 | 可见性判定或 CLI 参数形状变化 → 对应 WP 复验 |
| WP5 | WP1–WP4 | 文档必须与实现、DDL 文本、门禁一致 | 主分支 `3cadb12d` | `npm run check`（`check:docs`/`check:drift`）在交付前跑通 | 任一实现调整 → WP5 同步并复跑门禁 |

## Runtime Resources

| Checks / Work Packages | Resource | Isolation / Configuration | Exclusive Scheduling | Owner / Cleanup |
| --- | --- | --- | --- | --- |
| WP1–WP5、[PV1]–[PV5] | 构建目录、临时数据目录、loopback 端口 | 每个执行者独立 worktree + 独立 `CARGO_TARGET_DIR`；集成用例自建临时 Daemon 数据目录与自签证书，loopback 一律 `127.0.0.1:0` 随机端口 | 无需独占：不用数据库服务、容器、固定端口、外部账号或网络资源（依据：`design.md` D9 与本仓库既有测试形态） | 各执行者自建自删；主 Agent 在验收前核实无残留 |

## Target Repository and Main Branch

- Code Repository: `D:\Project\acp-remote`
- Target Ref: `refs/heads/main` = `3cadb12d79456750e40644a2ea53c96380b700e6`（2026-09-27 核实；本地与 `origin/main` 同 SHA）
- Version Confirmation Owner: 主 Agent（机械核实可交 environment/recon，目标选择仍由主 Agent 确认）
- Confirmation Method / Evidence: `git rev-parse refs/heads/main` / `origin/main`、`git status --porcelain`，结果写入 `verification.md` 的运行时基线与 Target

## Merge Strategy

| Delivery Unit / Mode | WP / TP | Owners: Implementation / Test / Review / Merge | Start / Readiness | Candidate Checks / Critical E2E | Order |
| --- | --- | --- | --- | --- | --- |
| DU1 / integrated | WP1–WP5 | 实现：coder 子 Agent（各 WP）；审查：独立 reviewer 子 Agent；集成/合入：独立集成 Agent（不兼任实现与审查） | 契约（本 plan 与 specs/design）冻结；`refs/heads/main` 核实为 `3cadb12d` | [PV1]、[PV2]、[PV3]、[PV4]、[PV5]（`not-applicable` 下的替代验证） | 单一单元 |

- Integration Branch / Worktree: 独立集成分支/worktree（路径在 apply 的交接中登记）
- Source Revision / Handoff: 固定源提交（各 WP 已验收提交）与 `reports/` 中的 WP handoff、RV1 报告
- Local Merge Conditions / Report Path: 候选 [PV1]–[PV5] 全绿 + RV1 无阻断 + 基线复核一致；报告写入 `reports/du1-merge.md`；满足后合入本地 `refs/heads/main`，交付 `origin/main` 走 `AGENTS.md` §8 的 PR 路径

## Verification Strategy

### Local Checks

各 WP 交付前：`cargo fmt --all -- --check`、`cargo clippy --locked -p <crate> --all-targets --all-features -- -D warnings`、`cargo test --locked -p <crate> --all-features`（涉及合同面时加 `npm run check`）。

### Project Verify

| Check ID | Stages / Work Packages | Command / Working Directory | Configuration / Environment | Scope / Pass Criteria | Evidence |
| --- | --- | --- | --- | --- | --- |
| PV1 | 分支、候选、主分支 | `npm run verify`（=`npm run check` 全部门禁 + `cargo fmt`/`clippy`/`cargo test --locked --workspace --all-features`），仓库根 | 固定工具链（`rust-toolchain.toml`）、Node ≥ 22.12；无外部服务 | 全部子门禁 exit 0、workspace 测试 0 failed；含 `check:docs`/`check:drift`/`check:agentic` | `reports/du1-pv1.log` |
| PV2 | 分支、候选、主分支 | `node scripts/check-crate-boundaries.mjs`，仓库根 | 同上 | 依赖方向与 §5 矩阵一致 | `reports/du1-pv1.log` |
| PV3 | WP1、WP2、候选 | `cargo test --locked -p storage-sqlite -p core --all-features`，仓库根 | 同上 | 迁移（旧库升级/幂等/列清单）、写集校验、模型构造与授权判定全绿，无零用例 | `reports/du1-pv1.log` |
| PV4 | WP3、WP4、候选 | `cargo test --locked -p server -p app --all-features` | 同上 | 可见性两处同口径、CLI 参数与展示全绿 | `reports/du1-pv1.log` |
| PV5 | 候选、主分支（替代验证主体） | `cargo test --locked -p server -p app --all-features` 的受控路径全链路用例（`node_link_e2e` 等），本机 Windows | 同上；loopback 随机端口 + 临时数据目录 | 受控路径按新口径跑通：配两个 Export、确认时只给一个 → 目录只见其一、另一 Export 的 attach 被拒、`import.add` 得到 `local.not_found` | `reports/pv5-windows-nodelink.log` |

### Code Review

独立 reviewer（`roles/reviewer.md`，不继承实现对话），逐 WP 检视并出具 `reports/rv1-<wp>.md`：可见性两处必须同口径（清单为空时既看不到也不可用）、迁移不得默认放权且列清单与新建库相等、`settle_pairing` 校验与写入同事务、CLI 警告不阻断且不引入新开关、文档与 DDL/门禁一致、无 `unwrap`/detached task 类问题。CRITICAL/MAJOR 必须修复并由新 reviewer 复核；文档 WP 另做一致性检视。

### Main E2E

```yaml
mode: not-applicable
reason: "本变更的行为只能通过 Owner 侧入站面观察（配对确认时的清单填报与 Node Link 目录/资源过滤），而 Access 侧客户端（node-link-client）属后续切片尚未落地；当前不存在可被真实用户操作的产品端到端路径，只有测试内的脚本化 Access 客户端。"
basis: "与上一变更（node-link-owner，2026-09-26 用户批准同一降级口径）相同的项目形态：受控路径全链路集成测试用真实 loopback listener、真实 SQLite 与真实签名材料驱动 Owner 侧入站面，且本仓库既有约定把该形态记作替代验证（见该变更 verification.md 的 Main E2E 与 7.1 轮次）。"
alternative_checks: ["cargo test --locked -p server -p app --all-features 的受控路径全链路用例（[PV5]）：配两个 Export、确认时只给一个，验证目录只见其一、另一 Export 的 attach 被拒为 nodelink.export.not_granted；import.add 在本切片**恒**返回 local.unavailable（Access 侧客户端未落地 → 无 catalog 快照），因此不断言它的 local.not_found", "cargo test --locked -p storage-sqlite -p core --all-features（[PV3]）：v3 → v4 迁移的旧行置空/幂等/列清单相等，以及 settle_pairing 的三类校验与失败零写入", "npm run verify（[PV1]）与 node scripts/check-crate-boundaries.mjs（[PV2]）：合同门禁（含 check:docs/check:drift）与全 workspace 测试"]
downgrade_approval: "2026-09-27，本会话，用户原话：「同意」——回应主 Agent 的降级建议原话：「我建议同样记 not-applicable，替代验证 = 本变更相关的 cargo 测试（含受控路径全链路：配两个 Export、只给一个、验证目录只见其一且另一个 attach 被拒）+ npm run verify」。即批准本变更 Main E2E 记 not-applicable，以上述替代验证代替；来源为用户对本会话提问的直接批准，不沿用 2026-09-26 的首次确认。"
```

## Failure and Recovery

- 修复按原 WP ID 重新派发，修复后由新的独立 reviewer 复核；上游（core 模型/DDL 形状）变化时按 `Dependency Handoffs` 的失效列重开受影响 WP。
- 迁移用例失败先判断是"夹具/断言"还是"迁移语句"；后者必须整段回滚重写，不允许用改夹具掩盖。
- 资源异常：临时数据目录/证书由用例自建自删；残留由主 Agent 在验收前清理并登记。
- 回滚：v4 库对旧二进制是"版本过新"拒绝打开，回滚 = 恢复备份 + 回退二进制；代码侧回滚需另行授权。

## Completion Criteria

- 最终主分支版本（`refs/heads/main` 的实际提交）上：[PV1]–[PV4] 全绿，[PV5] 在本机 Windows 执行并留证；RV1 无未解决阻断项；E2E 记 `not-applicable` 且替代验证通过并留证。
- `verification.md` 具备：运行时基线、Target 与目标提交、Checks、Check Plan Changes、Handoff Index、Review Findings、Merge History、Candidate/Main E2E（`not-applicable` + 替代验证）、Runtime Resources、Final Assessment（唯一 `agentic-assessment` 块）。
- 阻断清零：无未闭环 FAIL/BLOCKED，无未解决 CRITICAL/MAJOR；`openspec-agentic workflow check --stage plan|premerge|final|archive` 与 `e2e check` 在各阶段全 PASS；按 `.agents/skills/agentic-verify/SKILL.md` 完成最终验收后方可报告可归档。
