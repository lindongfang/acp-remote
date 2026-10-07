<!-- WP1 实现阶段交付报告（Round 1）。coder 只交付产品代码/合同资产、局部检查、工作包 Project Verify 与产品缺陷修复，不判独立 review、E2E 与合入。 -->

task_id: "4.1"
role: coder
phase: implement
agent_context:
  agent_id: "CoderWp1"
  isolation: "fork_turns=none（新实例接管既有未提交实现；未继承上一轮 WP1 作者 coding-1 的对话）"
target_revision: "6779c36678376f02b95a4071b62dca809b221a8b（分支 feat/wp1-sync-snapshot-contract；父提交 = 固定起点 353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f）"
scope: "WP1：R1–R4 的机器合同与文档（schema、fixture、Rust 类型化镜像、文档）。接管本轮开始前已存在于 .worktrees/wp1 的未提交实现，独立逐条核对后提交为固定交付提交，并完成工作包 Project Verify（PV1）。不判 server::sync 服务端语义（design Non-Goals），不写集成/E2E 用例。"
changes: "在 .worktrees/wp1 内提交固定交付提交 6779c36（23 文件，+585/−209）。无越界写入：未触碰 .worktrees/ 下其他包、主仓库工作区、openspec/changes/**（本报告除外）。未执行 push/rebase/merge。诊断期间在 /tmp 下建的几个用于拦截 cargo 的临时包装脚本已全部删除。"
checks:
  - id: "LC1 cargo clippy -p sync-protocol"
    command: "CARGO_TARGET_DIR=D:\\Project\\acp-remote\\.target-wt\\wp1 cargo clippy --locked -p sync-protocol --all-targets -- -D warnings（cwd=.worktrees/wp1）"
    scope: "Local Check（plan.md Verification Strategy 的按包静态检查）"
    environment: "cargo 1.98.1 / rustc 1.98.1（rust-toolchain.toml 钉定）；独占 CARGO_TARGET_DIR"
    exit_code: 0
    log_path: "/d/Project/acp-remote/.target-wt/wp1/logs/clippy-p-sync-protocol.log"
    result: "exit 0，无警告：`Checking sync-protocol v0.0.0 ... Finished dev profile`。"
  - id: "LC2 cargo fmt --all"
    command: "CARGO_TARGET_DIR=D:\\Project\\acp-remote\\.target-wt\\wp1 cargo fmt --all -- --check（cwd=.worktrees/wp1）"
    scope: "Local Check（plan.md 指定的格式检查）"
    environment: "cargo-fmt/rustfmt 1.9.0-stable（48a229cea 2026-09-01）"
    exit_code: 1
    log_path: "/d/Project/acp-remote/.target-wt/wp1/logs/fmt-all-wp1.log（原始报错）、.../fmt-local.log（同一命令第二次复现）"
    result: "**FAIL（环境事实，非本 diff 缺陷）**：`cargo metadata` 报 `vendor/windows-local-ipc` 自认属于主仓库 `D:\\Project\\acp-remote\\Cargo.toml` 的 workspace。已实测：该失败在全部四个 worktree（wp1/wp2/tp1/mu1-merge，含 MU1a 干净候选）**逐字复现**，而主仓库同命令 exit 0，证明与 WP1 改动无关；历史报告 `review-mu1-fixes-r1.md:213` 与 `review-wp2-r2.md:136` 已把该现象记为 worktree 隔离的既存环境事实。等价的正确性证据见 LC3/LC4。"
  - id: "LC3 rustfmt 直检（改动文件）"
    command: "rustfmt --check --edition 2024 crates/sync-protocol/src/command.rs crates/sync-protocol/src/sync.rs crates/sync-protocol/tests/envelope_fixtures.rs（cwd=.worktrees/wp1）"
    scope: "LC2 的等价佐证：对 WP1 实际改动的三个 .rs 直接判格式"
    environment: "rustfmt 1.9.0-stable"
    exit_code: 0
    log_path: "/d/Project/acp-remote/.target-wt/wp1/logs/rustfmt-direct.log（空输出 = 无差异）"
    result: "exit 0，无格式差异。"
  - id: "LC4 cargo fmt -p <各 workspace 成员>"
    command: "for p in acpr-transcript acpr-wire core storage-sqlite sync-protocol identity-auth identity-keystore node-link-protocol acp-protocol agent-host server app; do cargo fmt -p $p -- --check; done（cwd=.worktrees/wp1）"
    scope: "LC2 的等价佐证：`--all` 语义按包逐条展开，覆盖全部 12 个 workspace 成员"
    environment: "同 LC2；`-p` 不触发对 vendor 的 workspace 归属解析"
    exit_code: 0
    log_path: "/d/Project/acp-remote/.target-wt/wp1/logs/fmt-per-member.log"
    result: "12/12 成员 exit 0（FMT_ANY_FAIL=0）。因此 WP1 提交后的整仓格式是正确的；LC2 的失败只来自 `--all` 触发的 workspace 解析。"
  - id: "PV1 check（npm run check）"
    command: "npm run check（cwd=.worktrees/wp1）"
    scope: "工作包 Project Verify 的合同门禁十道脚本"
    environment: "Node v22.22.0 / npm 10.9.4；仓库根 node_modules 只读联接"
    exit_code: 0
    log_path: "/d/Project/acp-remote/.target-wt/wp1/logs/PV1-verify.log（第 6–107 行）"
    result: "十道脚本全 PASS：`check:schemas` 133 valid / 35 invalid（ajv Draft 2020-12）、39 event views bound；`check:commands` 13 commands；`check:errors` 58 codes / 2 protocols；`check:features` 13 feature ids；`check:assets` 17 schemas / **181 fixture files** / 12 transcript vectors re-encoded / 20 negative vectors / 2 SAS；`check:acp` 71 rows；`check:docs` 415 links / 9050 section refs；`check:boundaries` 12 crates；`check:drift` 36 DDL / 15 traits / 96 methods；`check:agentic` PASS + 21 specs passed / 0 failed。"
  - id: "PV1 check:rust / clippy"
    command: "CARGO_TARGET_DIR=D:\\Project\\acp-remote\\.target-wt\\wp1 cargo clippy --locked --workspace --all-targets --all-features -- -D warnings（cwd=.worktrees/wp1）"
    scope: "check:rust 第二条（与脚本参数逐字一致）"
    environment: "独占 CARGO_TARGET_DIR"
    exit_code: 0
    log_path: "/d/Project/acp-remote/.target-wt/wp1/logs/clippy.log"
    result: "exit 0，全 workspace 无警告。"
  - id: "PV1 check:rust / test"
    command: "CARGO_TARGET_DIR=D:\\Project\\acp-remote\\.target-wt\\wp1 cargo test --locked --workspace --all-features（cwd=.worktrees/wp1）"
    scope: "check:rust 第三条（与脚本参数逐字一致）"
    environment: "独占 CARGO_TARGET_DIR；真实进程/文件系统用例（app、server）"
    exit_code: 0
    log_path: "/d/Project/acp-remote/.target-wt/wp1/logs/test-workspace.log（第 2 次运行，含下述 flake）、.../test-workspace-rerun.log（第 3 次运行，TEST_EXIT=0，92 suites ok）"
    result: "三次运行：**第 1 次 exit 0**（此前 154.8s 完成，92 suites ok）；**第 2 次 exit 101**，唯一失败是 `crates/app/tests/daemon_lifecycle.rs::the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown`（panic 文本见「已知 flaky」小节）；**第 3 次 exit 0**（92 suites ok，该用例 `... ok`）。该用例属 `crates/app`——WP1 提交 `git diff --name-only HEAD^ HEAD -- crates/app` 为空，且它在本 worktree 的多次历史报告中即被记为已知 flaky（`.worktrees/mu1-merge/openspec/changes/archive/2026-09-29-node-trust-export-ids/reports/handoff-coder-impl.md:674`、`du1-integration.md:122`）。故判为与 WP1 无关的既存时序 flake，未改断言、未 `#[ignore]`、未跳过。"
  - id: "PV1 check:rust / fmt"
    command: "CARGO_TARGET_DIR=D:\\Project\\acp-remote\\.target-wt\\wp1 cargo fmt --all -- --check（cwd=.worktrees/wp1，作为 npm run verify 的第一步）"
    scope: "check:rust 第一条"
    environment: "同 LC2"
    exit_code: 1
    log_path: "/d/Project/acp-remote/.target-wt/wp1/logs/PV1-verify.log（第 108–144 行，即 verify 的终止点）"
    result: "**FAIL（环境事实）**：`cargo fmt --all` 因 vendor 的 workspace 归属报错，`npm run verify` 因此在该步非零退出（EXIT=1）。该失败与 LC2 同源、与 WP1 改动无关；等价格式证据 LC3/LC4 均 exit 0。"
issues: "0 CRITICAL / 0 MAJOR / 0 MINOR。PV1 未全绿，唯一原因是 `cargo fmt --all` 在本 worktree 的既存环境失败（已在 LC2/LC3/LC4 与全部历史报告中核实），非本包缺陷。1 项已知 flake（crates/app）已如实记录。1 项基线既存、范围外的不一致（`SessionReadResources.config_options` 的 Rust 镜像与 schema 不同形）转交主 Agent。"
result: "PASS（工作包内可交付面）；PV1 严格口径 **FAIL**（仅 cargo fmt --all 环境步骤）。详见「结论与判定」。"
evidence_paths:
  - "openspec/changes/sync-scope-and-pwa-client/reports/deliver-wp1-r1.md"
  - "/d/Project/acp-remote/.target-wt/wp1/logs/PV1-verify.log"
  - "/d/Project/acp-remote/.target-wt/wp1/logs/fmt-all-wp1.log"
  - "/d/Project/acp-remote/.target-wt/wp1/logs/fmt-per-member.log"
  - "/d/Project/acp-remote/.target-wt/wp1/logs/rustfmt-direct.log"
  - "/d/Project/acp-remote/.target-wt/wp1/logs/clippy.log"
  - "/d/Project/acp-remote/.target-wt/wp1/logs/clippy-p-sync-protocol.log"
  - "/d/Project/acp-remote/.target-wt/wp1/logs/test-workspace.log"
  - "/d/Project/acp-remote/.target-wt/wp1/logs/test-workspace-rerun.log"
resource_cleanup:
  - "worktree .worktrees/wp1：`git status --porcelain --untracked-files=all` 为空（全部改动已提交），无残留临时文件。"
  - "诊断临时物：/tmp 下 4 个 cargo 包装脚本与若干 probe/output 文本已删除；未在 worktree 或 .target-wt 下产生清单外的未跟踪目录。"
  - "CARGO_TARGET_DIR=D:\\Project\\acp-remote\\.target-wt\\wp1：**已释放构建产物**（`debug/` 与 `tmp/` 共约 13G 删除），仅保留 `logs/` 下本报告引用的 10 个原始日志（148K），供 reviewer 逐条复核。这是本包独占构建目录，未与 wp2/tp1/mu1a 交叉；由 provisioner/merger 按需回收。"

```agentic-handoff
version: 1
agent_context:
  agent_id: "CoderWp1"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "4.1"
    work_package: WP1
    role: coder
    phase: implement
    round: 1
    attempt: 1
    stage: work-package
    target_revision: "6779c36678376f02b95a4071b62dca809b221a8b"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/deliver-wp1-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 .worktrees/wp1（分支 feat/wp1-sync-snapshot-contract、HEAD=6779c36、父提交=353ba6ef，git rev-parse 已核实）上提交固定交付提交；本行绑定该提交本身（23 文件、+585/−209）。R1–R4 逐条核对见下。"
    source_evidence: NOT_APPLICABLE
  - task_id: "4.1"
    work_package: WP1
    role: coder
    phase: implement
    round: 1
    attempt: 1
    stage: work-package
    target_revision: "6779c36678376f02b95a4071b62dca809b221a8b"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/deliver-wp1-r1.md"
    result: FAIL
    evidence_status: NEW
    applicability_basis: "cwd=.worktrees/wp1 的 `npm run verify`：`check` 十道脚本全 PASS（exit 0）；`check:rust` 的 clippy 与 test exit 0，但第一步 `cargo fmt --all` 因 vendor/windows-local-ipc 的 workspace 归属报错而 exit 1，故 PV1 整体非零退出。该 fmt 失败在四个 worktree（含干净候选 mu1-merge）逐字复现、主仓库同命令 exit 0，历史报告 review-mu1-fixes-r1.md:213 / review-wp2-r2.md:136 已记为既存环境事实；等价格式证据（rustfmt 直检 + 逐成员 cargo fmt -p，12/12）exit 0。日志：.target-wt/wp1/logs/PV1-verify.log。"
    source_evidence: NOT_APPLICABLE
  - task_id: "4.1"
    work_package: WP1
    role: coder
    phase: implement
    round: 1
    attempt: 1
    stage: work-package
    target_revision: "6779c36678376f02b95a4071b62dca809b221a8b"
    evidence_type: CHECK
    evidence_id: "LC1（Local Check，plan.md Verification Strategy）"
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/deliver-wp1-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "`cargo clippy --locked -p sync-protocol --all-targets -- -D warnings`，cwd=.worktrees/wp1，CARGO_TARGET_DIR=.target-wt/wp1，exit 0。log: .target-wt/wp1/logs/clippy-p-sync-protocol.log"
    source_evidence: NOT_APPLICABLE
  - task_id: "4.1"
    work_package: WP1
    role: coder
    phase: implement
    round: 1
    attempt: 1
    stage: work-package
    target_revision: "6779c36678376f02b95a4071b62dca809b221a8b"
    evidence_type: CHECK
    evidence_id: "LC2（Local Check，cargo fmt --all）"
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/deliver-wp1-r1.md"
    result: FAIL
    evidence_status: NEW
    applicability_basis: "`cargo fmt --all -- --check` 在 .worktrees/wp1 exit 1（vendor workspace 归属）。等价证据 LC3（rustfmt 直检改动文件）与 LC4（逐成员 cargo fmt -p，12/12）均 exit 0，log 见 .target-wt/wp1/logs/{rustfmt-direct,fmt-per-member}.log。"
    source_evidence: NOT_APPLICABLE
```

## Shared Report 补充

| 项 | 取值 |
| --- | --- |
| WP ID | `WP1`（交付单元 MU1a 成员；MU1a = WP1 + WP2，Order 1，契约起点） |
| 阶段 | `implement`（接管既有未提交实现并固定交付） |
| 实际执行者 / 上下文方式 | `CoderWp1`（`fork_turns=none`，新实例；未继承上一轮 WP1 作者 `coding-1` 的对话） |
| 认领 | `npx --quiet --no-install openspec-agentic dispatch --change sync-scope-and-pwa-client --wp WP1 --executor coder-w1-r1 --ack` → `state: coding`、`attempt: 1`（本包第一个动作） |
| 输入版本 | planningDigest `plan-v2:sha256:4e2fb347b974d0e8b6d347afec7cbd50591524f79ae7b33cf0d98ced853c88e1`；契约文件 proposal.md / design.md / specs/sync-snapshot-scope/spec.md / plan.md / AGENTS.md / docs/SYNC_PROTOCOL.md / docs/NODE_LINK_PROTOCOL.md |
| worktree / 分支 | `D:\Project\acp-remote\.worktrees\wp1` / `feat/wp1-sync-snapshot-contract` |
| 固定起点提交 | `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`（= 交付提交的父提交，已核实） |
| 交付提交 | `6779c36678376f02b95a4071b62dca809b221a8b` |
| 写入范围 | `schemas/sync/v1/{command,sync}.schema.json`、`fixtures/sync/v1/`、`crates/sync-protocol/src/{command,sync}.rs`、`docs/{SYNC_PROTOCOL,NODE_LINK_PROTOCOL}.md`，以及共享写点 `crates/sync-protocol/tests/envelope_fixtures.rs` 的两个计数常量 |
| 依赖包含关系 | 无上游依赖（plan.md W1：Dependencies = `none`，契约起点）；本包产出供 WP3/WP5a/WP5b/WP6/WP7/TP1/TP3 以 `contract:` 消费 |

### 交付提交差异（`git diff --stat 353ba6ef 6779c36`）

```
 23 files changed, 585 insertions(+), 209 deletions(-)
 crates/sync-protocol/src/command.rs                                      | 129 ++++++++++++++++-
 crates/sync-protocol/src/sync.rs                                         | 160 ++++++++++-----------
 crates/sync-protocol/tests/envelope_fixtures.rs                          |   4 +-
 docs/NODE_LINK_PROTOCOL.md                                               |   4 +-
 docs/SYNC_PROTOCOL.md                                                    |  50 ++++---
 fixtures/sync/v1/invalid/command-result-session-read-missing-has-earlier.json  |  21 +++
 fixtures/sync/v1/invalid/command-session-read-single-field-before.json   |  19 +++
 fixtures/sync/v1/invalid/snapshot-begin-chunk-count-too-large.json       |  16 +++
 fixtures/sync/v1/invalid/snapshot-chunk-detail-resource-config-options.json | 13 +++
 fixtures/sync/v1/invalid/snapshot-chunk-detail-resource-messages.json    |  13 +++
 fixtures/sync/v1/manifest.json                                           |  62 +++++++-
 fixtures/sync/v1/valid/command-result-session-read-last-page-completed.json | 32 +++++
 fixtures/sync/v1/valid/command-result-session-read-paged-completed.json  |  50 +++++++
 fixtures/sync/v1/valid/command-session-read-default-page.json            |  15 ++
 fixtures/sync/v1/valid/command-session-read-paged.json                   |  20 +++
 fixtures/sync/v1/valid/sync-snapshot-begin.json                          |   2 +-
 fixtures/sync/v1/valid/sync-snapshot-chunk-config-options.json           |  31 ----
 fixtures/sync/v1/valid/sync-snapshot-end.json                            |   2 +-
 fixtures/sync/v1/valid/sync-snapshot-sessions-only-begin.json            |  16 +++
 fixtures/sync/v1/valid/sync-snapshot-sessions-only-chunk.json            |  25 +++
 fixtures/sync/v1/valid/sync-snapshot-sessions-only-end.json              |  16 +++
 schemas/sync/v1/command.schema.json                                      |  26 +++-
 schemas/sync/v1/sync.schema.json                                         |  68 ++-------
```

## 需求符合性逐条（R1–R4）

### R1 快照只承载清单类资源 — 符合

| 层 | 落点 | 内容 |
| --- | --- | --- |
| schema | `schemas/sync/v1/sync.schema.json#/$defs/snapshotResource` | 枚举收为 `["sessions","workspaces","agents"]`；`snapshotChunk` 的 `messages`/`turns`/`pending_interactions`/`config_options`/`capabilities` 五条 `if/then` 整段删除，只留 sessions/workspaces/agents 三条；五类明细的 `snapshotItem.*` 定义**保留**供 `session.read` 结果复用 |
| schema | 新增 `$defs/snapshotChunkCount` | `type: integer`、`minimum: 1`、`maximum: 3`，由 `snapshotBegin` 与 `snapshotEnd` 共同 `$ref`——`chunkCount` 口径不可能分叉；未协商 `core.local-catalog.v1` 时取 1 |
| Rust 镜像 | `crates/sync-protocol/src/sync.rs` | `SnapshotResource` 去五变体、`ALL` 缩为 3；`SnapshotItems` 同步去五变体，`parse`/`resource`/`len`/`Serialize` 四处分派同步收窄（match 仍穷尽）；`chunk_count` 由 `BoundedU64<0,100_000>` 收为 `BoundedU64<1,3>`（begin 与 end 同源） |
| 清理 | `crates/sync-protocol/src/sync.rs` | 变体删除后失去唯一消费者的孤立类型 `SnapshotItemConfigOption` 一并删除（`git grep SnapshotItemConfigOption` 全仓无命中） |
| 文档 | `docs/SYNC_PROTOCOL.md` §9.4 | 资源种类清单收为三类；新增段落写明五类明细「无论 `origin.kind` 是 `local` 还是 `remote`」一律不进快照，并把「此前只对 imported 会话豁免」显式改写为「全部会话」；新增「明细缺席不是『为空』」段落（对应 spec 第三条 Scenario）；`chunkCount` 1..=3 的下界依据写明 |
| fixture | `fixtures/sync/v1/` | `valid/sync-snapshot-begin.json`、`valid/sync-snapshot-end.json` 的 `chunkCount` 6→3；新增 `valid/sync-snapshot-sessions-only-{begin,chunk,end}.json`（`chunkCount=1`，覆盖「未协商目录 feature」场景）；新增 `invalid/snapshot-chunk-detail-resource-{messages,config-options}.json`（`enum` 拒绝）；新增 `invalid/snapshot-begin-chunk-count-too-large.json`（`chunkCount: 8` → `maximum` 拒绝）；删除 `valid/sync-snapshot-chunk-config-options.json`（其 `resource=config_options` 已被移出词表，保留必然冲突） |

五类明细对 local/remote 一视同仁：`resource` 是全局封闭词表，不带 `origin` 条件。`sessions` 摘要未被改动（`snapshotItem.sessions` 仍是 `sessionSummary` 的 `$ref`）。

### R2 会话明细经分页读取 / R3 游标由时间与标识复合构成 — 符合

| 层 | 落点 | 内容 |
| --- | --- | --- |
| schema | `command.schema.json#/$defs/sessionReadBefore` | `type: object`、`additionalProperties: false`、`required: ["createdAt","messageId"]`——单字段游标在 schema 层直接不成立 |
| schema | `command.schema.json#/$defs/sessionReadLimit` | `type: integer`、`minimum: 1`、**故意无 `maximum`**（超上限由服务端收敛而非报错），理由写在 description |
| schema | `sessionRead.payload` | 挂入可选 `before`/`limit`；`payload` 仍 `additionalProperties: false` + `required: ["include"]`，默认页场景（只给 `include`）形状成立。description 写明 `before`/`limit` 只在本传输面生效（D3） |
| schema | `sessionReadResult` | `hasEarlier` 加入 `required`，定义为 `boolean`；description 说明客户端据此决定是否续翻 |
| Rust 镜像 | `crates/sync-protocol/src/command.rs` | `SessionReadBefore { created_at: Timestamp, message_id: Uuid }`（`deny_unknown_fields`、两字段无 `default`）；`SessionRead { include, before: Option<SessionReadBefore>, limit: Option<UIntAtLeast<1>> }`；`SessionReadResult.has_earlier: bool`（非 Option、无 `default`，缺席即拒）；`parse_payload` 的错误提示同步更新 |
| 文档 | `docs/SYNC_PROTOCOL.md` §11.5 | 逐条写明：`createdAt` 升序 + `messageId` 仅作同时刻 tie-breaker；排序 MUST NOT 依赖 `messageId` 的数值或字典序（回指 §3.3「排序不得依赖 UUID」）；单字段游标与 `offset` 之类额外键一律按 `protocol.schema_invalid` 拒绝；省略 `before` 返回最新一页；**默认页是最近 20 条用户输入**（`role: "user"`）；`limit` 超服务端上限时收敛不报错；到最早一条给 `hasEarlier: false` 且不返回空占位；保留窗口之前用 `sync.cursor_invalid`（`details.reason = "cursor_expired"`） |
| fixture | `fixtures/sync/v1/` | `valid/command-session-read-default-page.json`（只给 include）、`valid/command-session-read-paged.json`（复合游标 + limit=20）、`valid/command-result-session-read-paged-completed.json`（`hasEarlier: true`，含两条 `createdAt` 完全相同的消息以坐实同刻 tie-break）、`valid/command-result-session-read-last-page-completed.json`（`hasEarlier: false`）；负向 `invalid/command-session-read-single-field-before.json`（`required`）、`invalid/command-result-session-read-missing-has-earlier.json`（`required`） |

### R4 分页只作用于客户端同步面 — 符合（并断言 Node Link DTO 未受影响）

| 核查 | 命令/判据 | 结果 |
| --- | --- | --- |
| Node Link 目录零改动 | `git status --porcelain -- crates/node-link-protocol schemas/node-link`；`git diff --stat HEAD -- ...` | 均为空——`crates/node-link-protocol/src/command.rs` 的 `SessionRead { include }`（`deny_unknown_fields`）与 `schemas/node-link/v1/command.schema.json#/$defs/submitSessionRead.payload`（`{ include }` + `additionalProperties: false`）原样未动 |
| 结果通道不外溢 | `crates/node-link-protocol/src/command.rs` 的 `CommandResultPayload` | `session.read` 走 `Object(RawObject)` 开放分支，Sync 侧 `sessionReadResult` 新增的必填 `hasEarlier` 不可能顺着 Node Link 的结果通道外溢 |
| 文档两侧写明适用面 | `docs/SYNC_PROTOCOL.md` §11.5；`docs/NODE_LINK_PROTOCOL.md` §12.7 | Sync 侧写明「只在 `schemas/sync/v1/command.schema.json` 生效」并回指；Node Link 侧命令表标注「分页参数只存在于 Sync 面，带了按 `nodelink.command.unsupported_field` 拒绝」并新增段落说明这是有意边界（`no-content-cache`，§12.4）而非遗漏 |
| schema 侧无回流 | `schemas/sync/v1/command.schema.json` 的 `sessionRead` description | 显式声明 Node Link 面同名命令载荷不含这两个键、行为与本变更前一致 |

## 重点项：`envelope_fixtures.rs` 两个计数常量（WP1 与 TP1 的共享写点）

- **当前值（本提交）**：`EXPECTED_VALID_MESSAGE_CASES = 73`、`EXPECTED_BODY_REJECTED = 14`（另两个未改动常量 `EXPECTED_ENVELOPE_REJECTED = 2`、`EXPECTED_SKIPPED_PAIRING = 5`）。
- **依据（对 WP1 自身向量的中间值，逐条可复算）**：基线为 `67 / 9`。WP1 在 `valid/` **新增 7 个** WSS 正向夹具、**删除 1 个**（`sync-snapshot-chunk-config-options.json`，其 `resource=config_options` 已被 D1 移出词表）→ `67 + 7 − 1 = 73`；在 `invalid/` **新增 5 个** WSS 负例 → `9 + 5 = 14`；pairing 的 5 个未变。
- **实测对照（在提交 `6779c36` 的工作树上用 manifest 逐条统计）**：WSS valid = **73**、WSS invalid = **16**、其中信封层负例 = 2 → body 层拒绝 = **14**、pairing = **5**。与常量逐条一致。
- **中间值说明**：TP1 会在本轮向量之外再追加 9 条（4 valid + 5 invalid，见 `test-tp1-r1.md`），届时 MU1a 的最终值应为 **77 / 19**（正是 `merge-mu1a-assemble-r1.md` 记载的临时值）。按 plan.md 的 Shared File Ownership，最终值由 MU1b 的 TP1 在 MU1a 合入提交上定；WP1 只负责自己向量对应的中间值。
- **`check:assets` 计数门禁的实测**：`check:assets` 报 **181 fixture files**，**未**在 `scripts/*.mjs` 中发现任何写死的夹具条数断言（`grep EXPECTED_/expected_count/minCases` 在 `scripts/` 无命中），因此不存在「组装后条目总数对不上」的机器门禁——本包告警的风险在脚本层不成立；唯一可能的漂移点是上面两个 Rust 常量，已按各自向量数自洽。

## 已知 flaky（如实记录，非本包缺陷）

`crates/app/tests/daemon_lifecycle.rs::the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown`
在第 2 次 `cargo test --workspace`（`test-workspace.log:386`）失败：

```
thread 'the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown' panicked at crates\app\tests\daemon_lifecycle.rs:603:5:
启动后必须已经跑过一轮清理：{...}
```

- 该 target 属 `crates/app`；WP1 提交 `git diff --name-only HEAD^ HEAD -- crates/app` **为空**。
- 本轮第 1、3 次运行均 exit 0（`the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown ... ok`）。
- 该用例在本 worktree 的历史交付里已被登记为已知 flaky（`.worktrees/mu1-merge/openspec/changes/archive/2026-09-29-node-trust-export-ids/reports/handoff-coder-impl.md:674`、`du1-integration.md:122`、`rv3-impl.md:107`）。
- 未改断言、未 `#[ignore]`、未跳过、未以「重跑取第二次结果」掩盖。

## 范围外观察（转交主 Agent，不在本轮判定）

- **`SessionReadResources.config_options` 的 Rust 镜像与 schema 不同形，且为基线既存**：Rust 是扁平的 `Option<Vec<ConfigOptionView>>`（`crates/sync-protocol/src/common.rs:269` 的 `ConfigOptionView`），而 `command.schema.json` 让它 `$ref` `sync.schema.json#/$defs/snapshotItem.config_options`（带 `sessionId`+`configOptions`+`version` 的按会话归组形状）。用 `git show 353ba6e` 核实，两侧在基线提交上就是这一状态，**不是 WP1 引入**；本轮未改。`review-wp1-r1.md` 与 `review-mu1-fixes-r1.md` 均已移交此点，建议主 Agent 单独派一轮修复。
- **服务端语义未实现**：spec 的「默认页 20 条用户输入」「同一 `before` 重复读取相同结果」「超上限收敛」「保留窗口报 `cursor_expired`」目前只有规范文本，`server::sync` 未落地，无生产者。属 design 的既定边界（Non-Goals / Migration Plan）。
- **独立 review、E2E、IV1 未返回**：本报告不把它们计为结论，主 Agent 据此调度独立 reviewer 有提交可绑（`6779c36`）。

## 结论与判定

- **工作包内可交付面 PASS**：R1–R4 在 schema / Rust 镜像 / fixture / 文档四层上互相一致；写入范围与声明一致；Node Link 侧零改动；独立逐条核对未发现与冻结契约不符之处，无需修改既有实现。
- **PV1 严格口径 FAIL，且失败点与 WP1 无关**：`npm run verify` 的 `check` 十道脚本全 PASS（exit 0），`check:rust` 的 clippy 与 test exit 0，唯 `cargo fmt --all` 因 worktree 内 vendor 的 workspace 归属报错而 exit 1。该失败在四个 worktree（含 MU1a 干净候选 `mu1-merge`）逐字复现、主仓库同命令 exit 0，历史报告已记为既存环境事实；等价格式证据（`rustfmt --check` 直检改动文件、逐成员 `cargo fmt -p` 12/12）均 exit 0。按「不能声称已通过」的要求，此处如实报 FAIL，不弱化断言、不删用例。
- **待主 Agent 处理**：① 以本提交 `6779c36` 调度独立 reviewer；② 若需 PV1 严格全绿，需先由 provisioner 修复 worktree 内 `cargo fmt --all` 的 vendor workspace 归属（例如在 worktree 内为 `vendor/windows-local-ipc` 补空 `[workspace]` 或调整 exclude 口径）——此修复超出 WP1 写入范围，本包不得自行跨出分区改动；③ 上文的范围外 `config_options` 镜像不一致是否单独派修。
