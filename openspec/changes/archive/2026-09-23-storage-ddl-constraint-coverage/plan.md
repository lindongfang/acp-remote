<!-- 将 design 的技术方案转为协作计划，主 Agent 统一维护。
     本变更 skip_specs: true，行为依据引用既有契约。 -->

## Scope and Contracts

- Specs Revision: 无增量规范（`skip_specs: true`）。既有行为契约：`docs/CORE_PORTS_AND_STORAGE.md` §7.2/§7.3/§7.4（四个目标列的 DDL）与 §9 判据 14 / §10 的枚举一致裁定；`openspec/specs/storage-schema-v2-migration/spec.md`（DDL 文本契约）；`openspec/specs/admin-state-persistence/spec.md`（撤销与重启恢复）；`openspec/specs/peer-identity-material/spec.md`（双角色身份一致与撤销覆盖）。
- Design Revision: `design.md` 的 D1–D4（revoke_reason 的 DDL 断言来源、cache_policy 等值 CHECK 的最小解析扩展、KeyChanged 行为回归的落点、非法值针对真实 CHECK 的断言方式）。
- Convergence Check: 一致。四条设计决策都只新增测试代码：D1/D2 落在 `enum_coverage.rs`，D3/D4 落在 `admin_store.rs`，文件不重叠、无接口依赖；协议、DDL、端口与安全语义均不变，不影响 `npm run check` 的合同资产。
- Skip Specs: `skip_specs: true`。理由：production 代码零改动、无规范层面的可观察行为变化，本变更只补齐既有契约（§7.3/§7.4 的 DDL 取值与 §9 判据 14 的枚举一致要求）在测试中的断言覆盖。依据为上述既有契约路径与标题，不新编需求。

## Contract Changes

无。本变更不改变 `docs/CORE_PORTS_AND_STORAGE.md` §5/§7 的任何端口签名或 DDL，不改变 wire schema、错误码词表与 fixture。若实现过程中发现 DDL 与 core 枚举实际不一致，按 design 的 Risks 停下并报告，不在此变更内修改合同。

## Coverage Index

```agentic-coverage
version: 1
target_ref: refs/heads/main
rows:
  - id: R1
    source:
      path: ../../../docs/CORE_PORTS_AND_STORAGE.md
      heading: "## 9. 验收判据（实现该合同的测试）"
    tasks: ["2.1"]
    checks: [PV3]
    evidence: [reports/lc1-enum-coverage.log]
  - id: R2
    source:
      path: ../../../docs/CORE_PORTS_AND_STORAGE.md
      heading: "### 7.4 `imported_*` 表（无正文）"
    tasks: ["2.1"]
    checks: [PV3]
    evidence: [reports/lc1-enum-coverage.log]
  - id: R3
    source:
      path: ../../../openspec/specs/admin-state-persistence/spec.md
      heading: "### Requirement: 撤销与重启恢复"
    tasks: ["2.2"]
    checks: [PV3]
    evidence: [reports/lc1-admin-store.log]
  - id: R4
    source:
      path: ../../../openspec/specs/peer-identity-material/spec.md
      heading: "### Requirement: 双角色身份一致与撤销覆盖"
    tasks: ["2.2"]
    checks: [PV3]
    evidence: [reports/lc1-admin-store.log]
  - id: R5
    source:
      path: ../../../docs/CORE_PORTS_AND_STORAGE.md
      heading: "### 7.3 `owned_*` 表"
    tasks: ["2.2"]
    checks: [PV3]
    evidence: [reports/lc1-admin-store.log]
```

## Work Packages

| ID | Goal / Scenarios | Dependencies | Owner | Reviewer | Branch / Worktree | Write Scope | Inputs / Outputs | Verification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | R1、R2：在 `enum_coverage.rs` 断言两个 `revoke_reason` 列的 DDL 取值集，并为两个 `cache_policy` 等值 CHECK 提供与 `core::model::CachePolicy` 一致的断言（design D1/D2） | 无 | 实现 Agent | 独立 reviewer（非本 WP 作者） | 独立分支/worktree（实现时确定） | `crates/storage-sqlite/tests/enum_coverage.rs` | 输入：design D1/D2、docs §7.2/§7.3/§7.4；输出：新增断言 + `cargo test -p storage-sqlite --test enum_coverage` 结果 | LC1、PV1–PV5 |
| WP2 | R3、R4、R5：`RevokeReason::KeyChanged` 的设备与节点撤销行为回归（经端口与 `revoke_token()`），以及非法 `revoke_reason` 被真实 CHECK 拒绝（design D3/D4） | 无 | 实现 Agent | 独立 reviewer（非本 WP 作者） | 独立分支/worktree（实现时确定） | `crates/storage-sqlite/tests/admin_store.rs` | 输入：design D3/D4、既有撤销用例辅助；输出：两条行为回归 + 非法值控制组/失败组 + `cargo test -p storage-sqlite --test admin_store` 结果 | LC1、PV1–PV5 |

## Execution Waves

- 单批：WP1 与 WP2 无相互依赖、写入文件不重叠，契约已冻结即可并行；单 Agent 环境下串行实现并如实记录。
- 共享接口：无。两个 WP 都只改测试文件，不新增生产代码或公开 API。
- 环境依赖：无外部服务；SQLite 临时目录由 `temp_dir`（含进程 id）隔离。

## Dependency Handoffs

无代码依赖：本变更为单仓库测试补丁，没有跨变更的上游交付物。实现基线为目标仓库 `refs/heads/main` 的当前提交（由 5.1 机械核实）。

## Runtime Resources

| Checks / Work Packages | Resource | Isolation / Configuration | Exclusive Scheduling | Owner / Cleanup |
| --- | --- | --- | --- | --- |
| WP1、WP2、LC1、PV1–PV5 | 无共享运行资源（SQLite 临时目录 + `target/` 构建目录） | `temp_dir` 按进程 id 隔离数据目录；并行分片必须各自设置 `CARGO_TARGET_DIR` | 不需要独占；`target/` 并行时按分片隔离即可 | 执行者；测试仅清理自身创建的临时目录 |

## Target Repository and Main Branch

- Code Repository: `D:\Project\acp-remote`
- Target Kind / Ref: local，`refs/heads/main`
- Version Confirmation Owner: 主 Agent（机械核实可交 environment/recon）
- Confirmation Method / Evidence: `git rev-parse refs/heads/main` 与 `git status --porcelain`，结果与核实时间记录在 `verification.md`

## Merge Strategy

模式：`integrated`。本变更是一个逻辑补丁（两个测试文件 + 一处最小解析扩展），单独交付没有意义，作为一个交付单元整体合入。

| Delivery Unit / Mode | WP / TP | Owners: Implementation / Test / Review / Merge | Start / Readiness | Candidate Checks / Critical E2E | Order |
| --- | --- | --- | --- | --- | --- |
| DU1 / integrated | WP1、WP2（各自自带行为测试，无独立 TP） | 实现 Agent / 实现 Agent（局部自检）/ 独立 reviewer / 独立 integrator（按授权） | 既有契约已冻结 + 依赖边界确认 | PV1、PV2、PV3、PV4、PV5；无关键 E2E（mode = not-applicable） | 唯一单元，先候选后合入 |

- Integration Branch / Worktree: **不适用**——用户于 2026-09-23 本会话授权「直接本地合并」（变体 A）：在本地 `refs/heads/main` 直接提交已验证的候选，不使用独立集成分支或 worktree。
- Source Revision / Handoff: 规划基线为 `refs/heads/main` 的 `d9cd3c4`；候选即工作区改动（`diff-sha256=e356ac17eb7af75b35a5e0b7397fb50367877154de6576957eab0e1f94968d48`），合入前由 5.1 重新核实基线。
- Authorization / Report Path: **用户已授权本地合入**（2026-09-23 本会话，原话：「直接本地合并」）：允许独立 integrator 在本地 `refs/heads/main` 提交该候选；**不含**推送、创建 PR、发布与归档——远端仍由用户自行处理（`AGENTS.md` §8 的 PR 规则不被本授权改变）。集成报告输出到 `openspec/changes/storage-ddl-constraint-coverage/reports/MU1.md`。

## Verification Strategy

### Local Checks

- LC1：`cargo test -p storage-sqlite --test enum_coverage --test admin_store`（在仓库根执行）。覆盖本次全部新增断言；日志写入 `reports/lc1-enum-coverage.log` 与 `reports/lc1-admin-store.log`。

### Project Verify

| Check ID | Stages / Work Packages | Command / Working Directory | Configuration / Environment | Scope / Pass Criteria | Evidence |
| --- | --- | --- | --- | --- | --- |
| PV1 | WP1、WP2、候选、主分支 | `cargo fmt --all -- --check`（仓库根） | 工具链由 `rust-toolchain.toml` 固定 | 无格式差异 | `reports/pv1-fmt.log` |
| PV2 | WP1、WP2、候选、主分支 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`（仓库根） | 同上 | 无警告 | `reports/pv2-clippy.log` |
| PV3 | WP1、WP2、候选、主分支 | `cargo test --locked --workspace --all-features`（仓库根） | 同上 | 全部测试通过，无 0 用例/全跳过误判 | `reports/pv3-cargo-test.log` |
| PV4 | DU1、候选、主分支 | `npm run check`（仓库根） | Node ≥ 22.12；`npm ci` 已完成 | 全部合同门禁通过（本变更不改合同资产，作为回归确认） | `reports/pv4-check.log` |
| PV5 | DU1、候选、主分支 | `npm run verify`（仓库根） | Node ≥ 22.12 + 固定 Rust 工具链 | PV4 与 `check:rust`（fmt/clippy/test）全绿 | `reports/pv5-verify.log` |

`cargo-deny` 与 `gitleaks` 只在 CI 运行，本地没有等价物；执行说明中必须如实记录为「未在本地执行」。

### Code Review

RV1：由不继承实现对话的独立 reviewer（`roles/reviewer.md`）在固定候选版本上只读检视 diff，重点：

- 是否真的零生产代码改动（无公开 API、无 DDL 变更）；
- `KeyChanged` 测试是否真的经端口与 `revoke_token()` 落库并能读回 `key_changed`；
- 非法值测试是否针对真实 CHECK（含合法 token 的控制组）而不是重复实现判定；
- 等值 CHECK 解析扩展是否最小且未影响既有 26 条 IN 用例；
- 是否出现与 DDL 同源的手抄字符串导致断言空转。

### Main E2E

```yaml
mode: not-applicable
reason: "本变更是 storage-sqlite 的测试覆盖补丁：生产代码零改动，没有可端到端运行的产品路径（x-agentic.e2e.command 为空）。"
basis: "openspec/config.yaml 的 context 已记录「当前阶段没有可端到端运行的产品路径，Main E2E 按变更记 not-applicable，需用户逐变更批准」，且 x-agentic.e2e.command 为空；本变更不改 wire 协议、DDL 或产品行为。"
alternative_checks: ["LC1：cargo test -p storage-sqlite --test enum_coverage --test admin_store", "PV1：cargo fmt --all -- --check", "PV2：cargo clippy --locked --workspace --all-targets --all-features -- -D warnings", "PV3：cargo test --locked --workspace --all-features", "PV4：npm run check", "PV5：npm run verify"]
downgrade_approval: "2026-09-23，本会话，用户原话：「同意」（回应主 Agent 提议：「本变更（storage-ddl-constraint-coverage）同意 Main E2E 记 not-applicable，替代验证为 4 个目标列的定向 cargo 测试 + npm run verify（npm run check + cargo fmt/clippy/test）。」）。"
```

## Failure and Recovery

- 定向测试失败且原因是 DDL 与 `revoke_token()` / `CachePolicy` 不一致：**停止**，按 design 的 Risks 报告为待决策缺陷，不在本变更内修改 DDL 或合同（会触及 §7 与合同漂移门禁）。
- 断言脆性（SQLite 错误措辞、DDL 文本格式）：按原 WP ID 在同一分支修复，只放宽措辞匹配，不放宽断言对象。
- 契约或写入范围变化：受影响 WP 的候选证据与 RV1 结论失效，重开对应任务并重跑 PV1–PV5。
- 主分支失败：停止后续操作，按同一验证路径交付修复；不出现「红着继续合」。
- 共享资源异常：清理测试自身创建的临时目录；`target/` 污染时用 `cargo clean -p storage-sqlite` 后重跑。
- 无 E2E 执行，故不涉及 E2E 失败上限。
- 未执行项：`cargo-deny` 与 `gitleaks` 本地没有等价物，只在 CI 运行。

## Completion Criteria

- `verification.md` 记录 LC1 与 PV1–PV5 的命令、版本、环境、退出码与日志，以及「`cargo-deny`/`gitleaks` 未在本地执行」的说明。
- RV1 无未解决阻断项；DU1 在主分支版本上 PV1–PV5 全绿。
- Main E2E 记 `NOT_APPLICABLE` 且替代验证（LC1 + PV1–PV5）已完成并留证。
- 合入 5.6 需要新的显式用户授权；未获授权时本变更停留在候选阶段，不报告为可归档。
- 最终验收使用 `agentic-verify` 入口，并运行 `openspec-agentic workflow check --stage final`。
