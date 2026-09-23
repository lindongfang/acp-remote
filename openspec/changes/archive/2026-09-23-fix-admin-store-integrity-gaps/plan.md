# fix-admin-store-integrity-gaps 执行计划

## Scope and Contracts

- Specs Revision: 本变更 `specs/` 下两份既有能力的增量规范（`admin-state-persistence`：`## ADDED Requirements` 的「撤销与删除后的写入不得复活资源」「设备与节点记录的活动时间只前进」，共 2 条需求 + 6 个场景；`peer-identity-material`：`## ADDED Requirements` 的「身份材料读取必须核对同行指纹」，1 条需求 + 3 个场景）。
- Design Revision: `design.md` 的 Context（现状事实与行号）与 D1–D6（imported 归属守卫、Export 撤销终态、身份材料读取核对、活动时间 `CASE`、回归用例设计、文档同步点）。
- Convergence Check: 一致。specs 的可观察行为（`NotFound(Export)`、`Conflict(AlreadyExists)`、`InvalidRequest`、`Corrupt`、活动时间三分支）与 design 的错误映射、SQL 形状、检查顺序一一对应；`PortError`/`ConflictKind` 复用既有取值，不需要新值对象、端口签名或 wire 变化，因此不存在影响拆包或验收的冲突。唯一跨文档硬依赖是 §5/§7 的 fenced 块与实现的逐条一致（`check:contract-drift`），本变更不动这些块，已在 WP3 的写入范围与验收中闭合。
- Skip Specs: no（两份能力的增量规范即本次行为契约）。

## Contract Changes

本变更**收口**以下已冻结契约（原/新值、原因与影响随实现写入 `verification.md` 的 Check Plan Changes）：

- `docs/CORE_PORTS_AND_STORAGE.md` §5.2 约束（`RemoteDeliveryStore::upsert_session`/`commit_receipt` 的归属前置与 `NotFound(Export)`）、§5.3 约束（`put_export` 撤销终态、身份材料读取核对指纹、活动时间单调）、§7.4 `[决定]`（失去 Import 的写入必须被拒绝 + 重导入残留缺口注记）、§9 新增判据 30、§11.2 第 5 条的端口级细化。
- `crates/storage-sqlite/src/admin/trust.rs` 的 `load_peer_key` 文档注释（「指纹列不参与判定」→「指纹由公钥派生、读取时必须核对，不一致失败关闭」；模块头无需改动，RV1-F2）。

不改变的契约：`core::ports` 的端口签名与 DTO 形状、`core::model` 值对象与错误枚举取值、§5/§7 的 fenced 代码块与 DDL 文本、Sync/Node Link wire schema、`schemas/`、`fixtures/`、`compatibility/` 的封闭词表与错误码、`LOCAL_ADMIN_PROTOCOL.md` 的方法集与错误映射。

## Coverage Index

```agentic-coverage
version: 1
target_ref: refs/heads/main
rows:
  - id: R1
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "### Requirement: 撤销与删除后的写入不得复活资源"
    tasks: ["2.1", "2.4", "2.5", "2.6"]
    checks: [PV3]
    evidence: [reports/lc1-admin-store.log, reports/lc1-imported.log]
  - id: R2
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "#### Scenario: 已撤销 Export 不能被后续写入清除撤销标记"
      requirement: "### Requirement: 撤销与删除后的写入不得复活资源"
    tasks: ["2.1", "2.4"]
    checks: [PV3]
    evidence: [reports/lc1-admin-store.log]
  - id: R3
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "#### Scenario: 撤销只能经 export.revoke 落库"
      requirement: "### Requirement: 撤销与删除后的写入不得复活资源"
    tasks: ["2.1", "2.4"]
    checks: [PV3]
    evidence: [reports/lc1-admin-store.log]
  - id: R4
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "#### Scenario: 完整移除后的迟到回调不能重建索引"
      requirement: "### Requirement: 撤销与删除后的写入不得复活资源"
    tasks: ["2.5", "2.6"]
    checks: [PV3]
    evidence: [reports/lc1-imported.log]
  - id: R5
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "### Requirement: 设备与节点记录的活动时间只前进"
    tasks: ["2.3", "2.4"]
    checks: [PV3]
    evidence: [reports/lc1-admin-store.log]
  - id: R6
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "#### Scenario: 旧值为空时首次写入不被丢弃"
      requirement: "### Requirement: 设备与节点记录的活动时间只前进"
    tasks: ["2.3", "2.4"]
    checks: [PV3]
    evidence: [reports/lc1-admin-store.log]
  - id: R7
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "#### Scenario: 更早的时间戳不使活动时间倒退"
      requirement: "### Requirement: 设备与节点记录的活动时间只前进"
    tasks: ["2.3", "2.4"]
    checks: [PV3]
    evidence: [reports/lc1-admin-store.log]
  - id: R8
    source:
      path: specs/admin-state-persistence/spec.md
      heading: "#### Scenario: 空值不抹掉既有活动时间"
      requirement: "### Requirement: 设备与节点记录的活动时间只前进"
    tasks: ["2.3", "2.4"]
    checks: [PV3]
    evidence: [reports/lc1-admin-store.log]
  - id: R9
    source:
      path: specs/peer-identity-material/spec.md
      heading: "### Requirement: 身份材料读取必须核对同行指纹"
    tasks: ["2.2", "2.4"]
    checks: [PV3]
    evidence: [reports/lc1-admin-store.log]
  - id: R10
    source:
      path: specs/peer-identity-material/spec.md
      heading: "#### Scenario: 信任材料行的指纹被外部改写"
      requirement: "### Requirement: 身份材料读取必须核对同行指纹"
    tasks: ["2.2", "2.4"]
    checks: [PV3]
    evidence: [reports/lc1-admin-store.log]
  - id: R11
    source:
      path: specs/peer-identity-material/spec.md
      heading: "#### Scenario: 设备记录的指纹与公钥不一致时读取失败"
      requirement: "### Requirement: 身份材料读取必须核对同行指纹"
    tasks: ["2.2", "2.4"]
    checks: [PV3]
    evidence: [reports/lc1-admin-store.log]
  - id: R12
    source:
      path: specs/peer-identity-material/spec.md
      heading: "#### Scenario: 指纹与公钥一致时读取不受影响"
      requirement: "### Requirement: 身份材料读取必须核对同行指纹"
    tasks: ["2.2", "2.4"]
    checks: [PV3]
    evidence: [reports/lc1-admin-store.log]
```

上述 12 行的场景覆盖由 `cargo test -p storage-sqlite --test admin_store --test imported`（LC1）与 `cargo test --locked --workspace --all-features`（PV3）共同证明；文档同步（2.7）由 `npm run check`（PV4/PV5）证明。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Owner | Reviewer | Branch / Worktree | Write Scope | Inputs / Outputs | Verification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | R1、R2、R3、R5–R8：管理 store 的写/读守卫——`put_export` 撤销终态（design D2）、身份材料读取核对同行指纹（D3）、活动时间 `CASE` 单调（D4）；并修正 `trust.rs` 的既有注释 | 无（契约已在 specs/design 冻结） | 实现 Agent（coder） | 非本 WP 作者（RV1） | 独立分支/worktree（执行时确定）。用户已授权本地合入 → 可复用主 worktree，但需与 WP2/WP3 串行化共享文件之外的构建目录 | `crates/storage-sqlite/src/admin/trust.rs`、`crates/storage-sqlite/src/admin/export.rs`、`crates/storage-sqlite/tests/admin_store.rs` | 输入：design D2/D3/D4/D5、`docs/CORE_PORTS_AND_STORAGE.md` §5.3/§7.3/§8；输出：三处守卫实现 + 三组回归用例 + `cargo test -p storage-sqlite --test admin_store` 结果 | LC1、PV1–PV5 |
| WP2 | R1、R4：imported 交付写路径的归属守卫（design D1）——`upsert_session` 与 `commit_receipt` 在同一事务内先验 `(ownerNodeId, exportId)` 归属 | 无（同上） | 实现 Agent（coder） | 非本 WP 作者（RV1） | 独立分支/worktree（执行时确定） | `crates/storage-sqlite/src/session_store.rs`、`crates/storage-sqlite/tests/imported.rs` | 输入：design D1/D5、§11.2 第 5 条；输出：两处守卫 + 移除后回调被拒的回归用例 + `cargo test -p storage-sqlite --test imported` 结果 | LC1、PV1–PV5 |
| WP3 | 文档同步：§5.2/§5.3 约束、§7.4 `[决定]`（含重导入残留缺口注记）、§9 判据 30、§11.2 第 5 条细化 | 无（行为由 specs/design 冻结；实现细节以 design 为准） | 实现 Agent（coder） | 非文档作者（RV1） | 独立分支/worktree（执行时确定） | `docs/CORE_PORTS_AND_STORAGE.md` | 输入：specs 两份增量、design D1–D4/D6；输出：可被 `check:contract-drift` 与 `check-doc-links` 接受的文档正文（不动任何 fenced 代码块） | PV4、PV5；3.1 |

说明：Main E2E mode 为 `not-applicable`，因此不生成独立测试工作包（TP）与 Test Design and Authoring 分组；各工作包自带其行为测试（局部自检），替代验证由主 Agent 在分支验证（3.1）与最终阶段（6.1）统一执行并留证。

## Execution Waves

| Wave | Work Packages | 依赖满足条件 | 并发上限 | 说明 |
| --- | --- | --- | --- | --- |
| W1 并行实现 | WP1 ∥ WP2 ∥ WP3 | 契约（specs/design）已冻结；WP1/WP2/WP3 写入文件互不重叠 | 3 | 三条轨道文件不重叠：`admin/{trust,export}.rs` + `tests/admin_store.rs` ／ `session_store.rs` + `tests/imported.rs` ／ `docs/CORE_PORTS_AND_STORAGE.md` |
| W2 交付前验证 | 3.1（LC1 + PV1–PV5）∥ 3.2（RV1） | W1 三条轨道全部产出 | 2 | PV 与独立 review 并行；review 必须由不继承实现对话的 reviewer 完成 |
| W3 集成与合入 | 4.1 → 4.2 → 5.1–5.8 | 全部 3.x 完成 | 1 | `integrated` 单一交付单元；目标分支只允许一个集成执行者串行更新 |
| W4 最终验证 | 6.1–6.3 → 7.1 | 5.7、5.8 | 1 | not-applicable 的替代验证 + `[e2e-owned]` 门禁 + 最终验收 |

**多 Agent 分派规则**（宿主支持子 Agent / 可开独立会话时）：

- 每个 WP 一个独立 coder 执行者；并行时各自设置 `CARGO_TARGET_DIR`，**不允许两个执行者共用同一个 `target/`**。
- 每个 WP 的交付前独立 review（RV1）必须由**不继承实现对话**的执行者完成；缺少隔离上下文时相关任务记 BLOCKED，不得以自审替代。
- 只有主 Agent 可写 `plan.md`/`tasks.md`/`verification.md`；子 Agent 返回结构化 handoff（固定提交、命令、日志路径、差异范围）。
- 文件单一写入者：`crates/storage-sqlite/src/admin/trust.rs`、`crates/storage-sqlite/src/admin/export.rs`、`crates/storage-sqlite/tests/admin_store.rs`（WP1）；`crates/storage-sqlite/src/session_store.rs`、`crates/storage-sqlite/tests/imported.rs`（WP2）；`docs/CORE_PORTS_AND_STORAGE.md`（WP3）；`reports/*` 按检查 ID 一文件一写者。
- 无并行能力时按 roles 串行执行并如实记录；串行不改变各轨道的完成条件与证据要求。

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| WP1、WP2、WP3 | 无（仅依赖已冻结契约：本变更 specs 两份增量 + design D1–D6） | 契约以本变更工作区内的 `specs/**/spec.md` 与 `design.md` 为准；无跨变更上游交付物 | 变更分支起点（6.1 记录的目标提交） | 三条轨道各自核对 `design.md` 的对应决策编号后再编码 | design 中 D1–D4 的判定顺序/错误映射若变化 → 对应 WP 的候选与 RV1 结论失效，重跑 LC1/PV3 |
| DU1 | WP1、WP2、WP3 | 各 WP 的 LC1/PV1–PV5 证据（`reports/lc1-*.log`、`reports/pv*.log`）与 RV1 报告（`reports/rv1-fix.md`） | W2 完成点 | `npm run verify` 全绿 + 变更 diff 无未登记文件（尤其无 `schemas/`/`fixtures/`/`compatibility/` 改动） | 任一上游变化 → 候选重建、PV1/PV3 重跑 |

说明：本变更是单仓库、单 crate 的守卫补丁，没有跨变更上游代码依赖；WP3 的文档文本依赖 design 的决策而不是 WP1/WP2 的代码产物，因此可与实现并行并在同一交付单元内一起验证。

## Runtime Resources

| Checks / Work Packages | Resource | Isolation / Configuration | Exclusive Scheduling | Owner / Cleanup |
| --- | --- | --- | --- | --- |
| WP1、WP2、LC1、PV1–PV3 | Rust 构建目录 `target/` | 每个并行执行者设自己的 `CARGO_TARGET_DIR`；同一 `target/` 不得并发写 | 并行分片时独占各自构建目录 | 各执行者，只清理自身产生的临时数据库与构建目录 |
| WP1、WP2 回归用例 | 临时 SQLite 文件（`support::temp_dir`，含进程 id）与 raw pool 连接 | 每个用例独立临时目录；不读写 `fixtures/`（本变更不需要夹具） | 无（文件级隔离） | 实现 Agent；仅清理自身创建的临时目录 |
| PV4、PV5 | Node ≥ 22.12 与仓库内 `node_modules` | 只读脚本，不写仓库以外的状态 | 无 | 主 Agent |

说明：本变更不涉及数据库服务、容器、端口、账号或外部服务等共享运行资源；`target/` 与 SQLite 临时目录是唯一共享设施，已按上表隔离。

## Target Repository and Main Branch

- Code Repository: `D:\Project\acp-remote`
- Target Kind / Ref: local；`refs/heads/main`
- Version Confirmation Owner: 主 Agent（机械核实可派发 environment/recon）
- Confirmation Method / Evidence: `git rev-parse refs/heads/main`、`git status --porcelain`（记录基线提交与核实时间于 `verification.md`）

## Merge Strategy

| Delivery Unit / Mode | WP / TP | Owners: Implementation / Test / Review / Merge | Start / Readiness | Candidate Checks / Critical E2E | Order |
| --- | --- | --- | --- | --- | --- |
| DU1 / integrated | WP1、WP2、WP3（各 WP 自带行为测试，无独立 TP） | 实现 Agent / 实现 Agent（局部自检）/ 独立 reviewer / 独立 integrator（按用户授权） | 契约定稿 + 写入范围确认 | PV1、PV2、PV3、PV4、PV5；无关键 E2E（mode = not-applicable） | 唯一单元，先候选后合入 |

- Integration Branch / Worktree: 用户已授权本地合入（见下），因此在主 worktree 内串行交付；若并行实现分片，各分片用独立 worktree + `CARGO_TARGET_DIR`，最终由集成 Agent 在主 worktree 组合为单一候选。
- Source Revision / Handoff: 规划基线为 `refs/heads/main` 的 `1ef6640`；实际候选与固定基线在 5.1/5.2 记录。
- Authorization / Report Path: **用户已授权本地合入 `refs/heads/main`**（2026-09-23 本会话；用户先后回复「A同意，B a」与「A同意，B  b」，按后一条（更晚的更正）记录为选 (b)，授权原话即该两条回复，回应主 Agent 的选项「(b) 现在就授权本地合入 `main`（请给原话；仍不含推送、PR、发布与归档）」）：允许独立 integrator 在本地 `refs/heads/main` 提交该候选；**不含**推送、创建 PR、发布与归档——远端与归档仍由用户自行处理（`AGENTS.md` §8 的 PR 规则不被本授权改变）。集成报告输出到 `openspec/changes/fix-admin-store-integrity-gaps/reports/du1.md`。
- 采用 integrated 的理由：三条轨道的可观察行为由同一批 specs 增量约束，且 PV4/PV5 的合同门禁在 `main` 上按整体判定（`npm run check` 读整仓库文档与依赖），单独合入任一条轨道都会让文档与实现暂时不一致，因此作为一个交付单元整体合入。

## Verification Strategy

### Local Checks

- LC1: `cargo test -p storage-sqlite --test admin_store --test imported`（仓库根）。覆盖本次新增的 12 个规格场景断言；日志写入 `reports/lc1-admin-store.log` 与 `reports/lc1-imported.log`。

### Project Verify

| Check ID | Stages / Work Packages | Command / Working Directory | Configuration / Environment | Scope / Pass Criteria | Evidence |
| --- | --- | --- | --- | --- | --- |
| PV1 | WP1–WP3、候选、主分支 | `cargo fmt --all -- --check`（仓库根） | 工具链由 `rust-toolchain.toml` 固定 | 无格式差异 | `reports/pv1-fmt.log` |
| PV2 | WP1–WP3、候选、主分支 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`（仓库根） | 同上 | 无警告 | `reports/pv2-clippy.log` |
| PV3 | WP1–WP3、候选、主分支 | `cargo test --locked --workspace --all-features`（仓库根） | 同上 | 全部测试通过；新增用例确实被执行（无 0 用例/全跳过误判） | `reports/pv3-cargo-test.log` |
| PV4 | WP3、候选、主分支 | `npm run check`（仓库根） | Node ≥ 22.12；`npm ci` 已完成 | 全部合同门禁通过，尤其 `check:contract-drift`（§5/§7 与 `ports.rs`/`migrate.rs` 逐条一致）与文档引用门禁 | `reports/pv4-check.log` |
| PV5 | DU1、候选、主分支 | `npm run verify`（仓库根） | Node ≥ 22.12 + 固定 Rust 工具链 | PV4 与 `check:rust`（fmt/clippy/test）全绿 | `reports/pv5-verify.log` |

`cargo-deny` 与 `gitleaks` 只在 CI 运行，本地没有等价物；执行说明中必须如实记录为「未在本地执行」。

### Code Review

RV1：由不继承实现对话的独立 reviewer（`roles/reviewer.md`）在固定候选版本上只读检视 diff，重点：

- 四条守卫是否都在既有写事务内、失败关闭先于任何写入，是否存在「部分写入后再报错」的路径；
- `put_export` 的判定顺序是否确为「先拒 `revoked_at` 非空（`InvalidRequest`）」→「已撤销 + 未撤销 → `Conflict(AlreadyExists)`」，且 `revoked_at` 不会被 UPDATE 清空；
- `upsert_session`/`commit_receipt` 的归属校验是否在同一事务、错误是否为 `NotFound(Export(exportId))`、是否真的阻止 `local_sequence` 推进；
- 身份材料读取核对是否覆盖 `owned_peer_key`、`owned_pairing_peer`、`owned_device` 的单读与列表读三条路径，且错误为 `Corrupt`；
- 活动时间是否用显式 `CASE` 且三分支正确（尤其旧值为空时不丢值），未引入标量 `MAX` 的 NULL 语义；
- 是否出现与实现同源的手抄期望值导致断言空转；是否越界改动了 DDL、端口签名、wire、`compatibility/` 或 `LOCAL_ADMIN_PROTOCOL.md`；
- 文档同步是否避开 §5/§7 的 fenced 代码块，且新增判据没有改动既有判据编号与措辞。

### Main E2E

```yaml
mode: not-applicable
reason: "本变更只修 crates/storage-sqlite 的写路径守卫与读取期校验（core 零改动，无 wire/DDL 变化）；server、app、daemon、CLI 与前端尚未实现，仓库中不存在可端到端运行的产品入口（无监听器、无 CLI 子命令、无前端），因此无法在真实入口执行端到端场景。"
basis: "openspec/config.yaml 的 context 已记录「当前阶段没有可端到端运行的产品路径，Main E2E 按变更记 not-applicable，需用户逐变更批准」，且 x-agentic.e2e.command 为空、x-agentic.e2e.enabled 为 true；本变更范围是库级持久化守卫，不改 wire 协议与 DDL。"
alternative_checks:
  - "LC1：cargo test -p storage-sqlite --test admin_store --test imported —— 直接覆盖本变更 12 个规格场景（移除后回调被拒、Export 撤销终态、身份材料读取失败关闭、活动时间三分支）。"
  - "PV3：cargo test --locked --workspace --all-features —— 全量回归，证明守卫未破坏既有 owned/imported 写路径与迁移用例。"
  - "PV4/PV5：npm run check / npm run verify —— 合同漂移门禁与文档引用门禁，证明 §5/§7 的冻结文本与实现仍逐条一致。"
downgrade_approval: "2026-09-23，本会话，用户回复原话：「A同意，B a」与「A同意，B  b」，回应主 Agent 提议：「A. 本变更（fix-admin-store-integrity-gaps）Main E2E 记 not-applicable，替代验证 = LC1（cargo test -p storage-sqlite --test admin_store --test imported）+ PV1–PV5（cargo fmt/clippy/test、npm run check、npm run verify）」；配置要求逐变更记录，故按本次回复记录。"
```

#### E2E Ownership and Cases

不适用（mode = not-applicable），本节按模板要求删除；实际替代验证已列为主 Agent 任务（6.1）并在 Coverage Index 中引用。

## Failure and Recovery

- 失败修复：按原 WP ID 重新派发（不新增 WP），修复后重跑该 WP 的 LC1/PV3 与 RV1；受影响的下游（含 DU1）按 Dependency Handoffs 的失效列重开。
- 传递下游失效：若 design D1–D4 的判定顺序或错误映射在实现中被改变，WP1/WP2 的既有证据与 RV1 结论失效，必须重建候选并重跑 PV1–PV5。
- 实现期发现既有库存在指纹与公钥不一致的行（读取期失败关闭被触发）：**停止**并把该行的定位信息报告为待决策缺陷；不在本变更内新增自动修复、迁移或跳过逻辑。
- 实现期发现 `put_export` 的撤销终态会影响某个已存在的内部调用路径：**停止**并报告，不擅自放宽规则（例如改成静默保留 `revoked_at`）。
- 主分支失败：`main` 上的 `npm run verify` 失败时停止后续操作，按同一验证路径交付修复；必要时回滚该交付单元（`git revert` 或恢复候选前提交），不允许「红着继续合」。
- 共享资源异常：SQLite 临时目录异常（磁盘满、文件占用）时清理由测试创建的临时目录；`target/` 污染时用 `cargo clean -p storage-sqlite` 后重跑受影响检查。
- 无 E2E 执行，故不涉及 E2E 失败上限；若后续有人把 mode 改为 required，必须先取得新的用户批准并补齐 E2E 任务。
- 未执行项：`cargo-deny` 与 `gitleaks` 本地没有等价物，只在 CI 运行；最终验收中明确记录为「未在本地执行」。

## Completion Criteria

- 全部任务勾选（`verification.md` 中逐 ID 关联证据；结果仅 PASS/FAIL/BLOCKED/NOT_APPLICABLE）。
- 最终主分支版本（`refs/heads/main` 的实际提交）上：PV1–PV5 全绿；RV1 无未解决阻断项；LC1 覆盖 Coverage Index 的 12 行；`npm run check` 的漂移与文档引用门禁无差异。
- 12 个规格场景（R1–R12）都有可读的断言证据；重导入残留缺口已按用户意见登记为显式非目标与文档注记，未伪装为已闭合。
- 阻断问题清零：无未闭环的 FAIL/BLOCKED、无未登记漂移、无秘密材料落库或进日志。
- 最终验收由主 Agent 按 `.agents/skills/agentic-verify/SKILL.md` 执行，并在验收块中以唯一 `[final-verification]` 任务记录；E2E 记 NOT_APPLICABLE 且替代验证已留证。
- 本计划只覆盖本地合入授权；不构成推送、创建 PR、发布或归档授权。
