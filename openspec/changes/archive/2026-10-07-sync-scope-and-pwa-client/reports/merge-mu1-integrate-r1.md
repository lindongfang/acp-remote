<!-- MU1 merger 的 integrate 阶段报告（Round 1）。本轮只做目标核实与候选工作区准备，未构建候选、未跑候选 Project Verify、未跑 premerge 门禁、未合入。 -->

task_id: "MU1"
role: merger
phase: integrate
agent_context:
  agent_id: "MergerMU1"
  isolation: "fork_turns=none（独立合入执行者；未参与 WP1/WP2/TP1 的实现或修复对话，仅继承调度方传入的角色契约与单元编成信息）"
target_revision: "refs/heads/main = 353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f（已用 git rev-parse 核实；候选提交尚不存在，MU1 候选 target_revision 为 NOT_AVAILABLE，见 blockers）"
scope: "MU1（模式 independent，成员 WP1/WP2/TP1，顺序组 1）候选准备。(1) 机械核实目标引用；(2) 在 .worktrees/mu1-merge 建立候选工作区并配平 node_modules；(3) 快照三个源工作区的未提交差异以固定组装输入；(4) 判定并报告阻断项。不含候选构建、候选 Project Verify、premerge、合并。"
changes: "仅新增内容，无任何产品代码/文档/夹具改动：创建 worktree .worktrees/mu1-merge（新分支 merge/mu1-candidate，指向基线 353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f）；在候选内建 node_modules 目录联接指向仓库根 node_modules（与 .worktrees/wp1、.worktrees/wp2 的 node_modules 软链同一模式，只读消费）；在候选内新增未跟踪目录 .merge-staging/ 存放 wp1.patch / wp2.patch / tp1-current.patch 三个 `git diff HEAD --binary` 只读快照，并向 .git/info/exclude 追加一行 `.merge-staging/` 以防误提交；新增本报告文件。三个源工作区与主检出均未被修改（见 checks 的写前/写后比对）。"
checks:
  - id: "TARGET-REF"
    command: "git rev-parse --abbrev-ref HEAD && git rev-parse refs/heads/main（cwd=D:\\Project\\acp-remote）"
    scope: "目标引用核实"
    environment: "主检出 D:\\Project\\acp-remote"
    exit_code: 0
    log_path: "本报告「目标引用核实」小节"
    result: "输出 `main` 与 `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`（两次求值一致）；`git rev-parse --verify refs/heads/main` 同样返回该 SHA，`git log --oneline -3` 显示 `353ba6e Merge remote-tracking branch 'origin/main' into main`。目标核实通过，目标即 refs/heads/main。"
  - id: "CANDIDATE-CREATE"
    command: "git worktree add -b merge/mu1-candidate .worktrees/mu1-merge 353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f"
    scope: "候选工作区创建"
    environment: "仓库 D:\\Project\\acp-remote"
    exit_code: 0
    log_path: "本报告「候选工作区」小节"
    result: "exit 0，`HEAD is now at 353ba6e`。随后 `git -C .worktrees/mu1-merge rev-parse --abbrev-ref HEAD` → `merge/mu1-candidate`，`rev-parse HEAD` → `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`，`git status --porcelain=v1 -b` 仅输出 `## merge/mu1-candidate`（无改动、无未跟踪文件），`git diff --stat HEAD` 为空。候选处于干净基线态。"
  - id: "SOURCE-DIFF-SNAPSHOT"
    command: "git -C .worktrees/wp1|wp2|tp1 diff HEAD --binary --output=.worktrees/mu1-merge/.merge-staging/<wp>.patch"
    scope: "固定组装输入"
    environment: "三个源工作区（只读 `git diff`，不 add/commit/checkout）"
    exit_code: 0
    log_path: ".worktrees/mu1-merge/.merge-staging/"
    result: "三份快照落盘：wp1.patch 54775 字节、wp2.patch 17657 字节、tp1-current.patch 72287 字节。命令只读取工作区差异，未写入源工作区。"
  - id: "TP1-OWNED-ARTIFACTS-ABSENT"
    command: "git -C .worktrees/tp1 status --porcelain --untracked-files=all -- crates/*/tests；git -C .worktrees/tp1 diff HEAD --name-only -- fixtures/sync/v1/manifest.json crates/storage-sqlite；git hash-object 逐文件比对 wp1/wp2/tp1"
    scope: "判定 TP1 是否已交付自有差异"
    environment: ".worktrees/tp1"
    exit_code: 0
    log_path: "本报告「阻断项」小节"
    result: "见 blockers：TP1 工作区目前只是 WP1+WP2 的镜像，没有任何 TP1 自有的契约向量。"
  - id: "WORKTREE-HYGIENE"
    command: "git status --porcelain（四个工作区分别执行）+ git worktree list"
    scope: "未扰动源工作区与主检出的证明"
    environment: "主检出 + wp1 + wp2 + tp1 + mu1-merge"
    exit_code: 0
    log_path: "本报告「工作区卫生」小节"
    result: "写候选前后比对一致：主检出 `## main...origin/main`，跟踪文件零改动（仅原有未跟踪 `.target-wt/`、`.worktrees/`、`openspec/changes/sync-scope-and-pwa-client/`）；wp1 11 个 M + 12 个未跟踪夹具、wp2 7 个 M、无未跟踪文件，与本轮开始时的 `git diff --stat` / `git status` 完全相同。三个源工作区 HEAD 仍为 353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f。"
  - id: "NOT-RUN / BY-INSTRUCTION"
    command: "候选构建、候选 Project Verify（npm run check / cargo test --locked -p sync-protocol --all-features）、workflow check --stage premerge、合入"
    scope: "MU1 候选与合并门禁"
    environment: "NOT_APPLICABLE"
    exit_code: NOT_APPLICABLE
    log_path: "NOT_AVAILABLE"
    result: "本轮按调度指令未执行。未运行任何 Project Verify，未生成 receipt，未产生 `agentic-premerge` 块。"
issues: "0 CRITICAL / 0 MAJOR / 0 MINOR / 1 BLOCKER（见 blockers：TP1 未交付，MU1 单元不完整，候选不可构建、不可验证、不可合入）。另记 1 项需在组装时规避的卫生风险：.worktrees/tp1 下有未跟踪的 wp1.patch / wp2.patch（TP1 作者的临时文件），组装时必须按文件清单挑取而非整树搬运。"
result: BLOCKED
evidence_paths:
  - "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1-integrate-r1.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/review-mu1-fixes-r1.md"

worktree_handoff:
  - work_package: MU1-CANDIDATE
    attempt: 1
    worktree: "D:\\Project\\acp-remote\\.worktrees\\mu1-merge"
    baseline_revision: "353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f"
    executor: "MergerMU1"
    received_at: "2026-10-03"

handoff_index:
  - task_id: "MU1"
    work_package: DELIVERY
    role: merger
    phase: integrate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: NOT_AVAILABLE
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1-integrate-r1.md"
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "MU1 候选提交尚不存在（候选工作区停在基线 353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f，尚未组装任何成员差异）。目标引用 refs/heads/main 已机械核实为同一提交。待补：TP1 的交付差异、WP1→WP2→TP1 的组装结果、候选 PV1（npm run check 与 cargo test --locked -p sync-protocol --all-features）的候选阶段 CHECK 索引、独立 candidate review、以及绑定候选提交的 agentic-premerge 块与版本化 receipt。门禁：候选 PV1 与独立 review 均 PASS 且 premerge PASS 且目标引用未移动，才允许合入。"
    source_evidence: NOT_APPLICABLE

---

## 目标引用核实

在 `D:\Project\acp-remote` 实际执行：

```
> git rev-parse --abbrev-ref HEAD
main
> git rev-parse refs/heads/main
353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f
> git rev-parse --verify refs/heads/main
353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f
> git log --oneline -3 refs/heads/main
353ba6e Merge remote-tracking branch 'origin/main' into main
7549d65 docs(repo):归档 sync-workspaces-and-create 并同步五份能力规范
d0347e0 feat(sync): 快照新增目录资源并使 session.create 进入 Sync 面 (#46)
> git status --porcelain=v1 -b
## main...origin/main
```

- 目标引用 = `refs/heads/main`，核实提交 = **`353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`**。
- 该提交同时就是 MU1 的编成基线，与 plan.md `## Target Repository and Main Branch` 记录的规划时取值一致，**目标自规划以来未移动**。
- 主检出跟踪文件零改动；无 `MERGE_HEAD` / rebase / cherry-pick 状态标记（仓库处于空闲态），不存在遗留的半完成合并。
- 结论：目标可核实，非 BLOCKED 项。**本报告的 BLOCKED 来自单元编成不完整，与目标引用无关。**

## 候选工作区

| 项 | 值 |
| --- | --- |
| 路径 | `D:\Project\acp-remote\.worktrees\mu1-merge` |
| 分支 | `merge/mu1-candidate`（新建，仅指向基线，尚无提交） |
| 基线 / HEAD | `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f` |
| 工作区状态 | 干净：`git status --porcelain=v1 -b` 仅 `## merge/mu1-candidate`；`git diff --stat HEAD` 为空 |
| 依赖 | `node_modules` 为指向 `D:\Project\acp-remote\node_modules` 的目录联接，与 `.worktrees/wp1`、`.worktrees/wp2` 的同名软链同一模式，只读消费 |
| 暂存区 | `.merge-staging/`（未跟踪，已加入 `.git/info/exclude`）：`wp1.patch`、`wp2.patch`、`tp1-current.patch` |

说明：merger 角色约定「复用本单元已有的执行 worktree，不新建」。本单元是 `independent` 模式，三个成员各有自己的执行工作区且其中两个仍在被 review/fix 过程使用（TP1 正在编写中），复用其中任何一个都会污染作者工作区；因此按调度指令在本单元内新建了专用的合入工作区，且不对三个源工作区做任何写入。

## 组装输入快照（尚未应用）

| 源 | 分支 | HEAD | `git diff --stat HEAD` | 未跟踪 | 快照 |
| --- | --- | --- | --- | --- | --- |
| `.worktrees/wp1` | `feat/wp1-sync-snapshot-contract` | `353ba6e` | 11 文件，+331 / −212 | 12 个新夹具（`valid/` 7 + `invalid/` 5） | `wp1.patch` 54775 B（仅跟踪文件；12 个未跟踪夹具需按文件清单另取） |
| `.worktrees/wp2` | `feat/wp2-derived-event-fields` | `353ba6e` | 7 文件，+161 / −13 | 无 | `wp2.patch` 17657 B |
| `.worktrees/tp1` | `feat/tp1-contract-vectors` | `353ba6e` | 17 文件，+492 / −225 | 12 个（与 WP1 相同的夹具）+ `wp1.patch`、`wp2.patch` | `tp1-current.patch` 72287 B |

共享文件的合并顺序取自 plan.md `## Shared File Ownership`：`manifest.json` WP1 → WP2 → TP1；`valid/` 按前缀划分（WP1 `sync-*`/`command-*`、WP2 `view-*`、TP1 只新增）；`docs/SYNC_PROTOCOL.md` WP1（§9.4/§11.5）→ WP2（§10.3）；`command.rs`/`sync.rs` 归 WP1，`views.rs` 归 WP2。三方已核对的现状：WP1 与 WP2 在 `docs/SYNC_PROTOCOL.md` 上是不同小节；WP2 对 `fixtures/sync/v1/manifest.json` 零改动（见 `review-mu1-fixes-r1.md`），故 manifest 的 WP1 → WP2 步实际是空操作，TP1 步才是真正的追加。

## 阻断项

**MU1 单元不完整，候选不可构建、不可验证、不可合入。**

1. **TP1 未交付，MU1 的 `crates/*/tests/` 契约向量不存在。** plan.md `## Work Packages` 的 TP1 定义为「合同与固定向量：快照清单向量、`session.read` 分页向量、`file.changed` 新字段向量、`state` 枚举向量、node-link 侧未受影响断言」，写入范围 `crates/*/tests/` 与 `fixtures/sync/v1/`（只增不改），覆盖 R1–R9 的机器可判定部分，验收 Check 为 PV1。实测 `.worktrees/tp1`：
   - `git status --porcelain --untracked-files=all -- crates/*/tests` 只输出 `M crates/sync-protocol/tests/envelope_fixtures.rs`，而该改动经 blob 比对与 WP1 的同名文件**完全同哈希**（`0e65e8bf…`），即属 WP1 交付内容而非 TP1 自有用例；`crates/storage-sqlite/tests/` 无任何新增。
   - 逐文件 `git hash-object` 比对确认 tp1 工作区目前只是 WP1+WP2 的镜像：`command.rs`、`sync.rs`、`envelope_fixtures.rs`、`command.schema.json`、`sync.schema.json`、`NODE_LINK_PROTOCOL.md`、`manifest.json` 与 wp1 同哈希；`views.rs`、`event-views.schema.json`、`ACP_COMPATIBILITY_MATRIX.md` 与 wp2 同哈希；`SYNC_PROTOCOL.md` 为两者并集（+65 行，= WP1 的 50 + WP2 的 15）。
   - 结论：**TP1 的自有差异（向量文件、manifest 追加、schema_drift 枚举门禁补齐）现在一个字节都不存在**。TP1 作者当前的两个未跟踪文件 `wp1.patch` / `wp2.patch` 是其本地临时物，不是交付物。
   - 后果：即使强行只装 WP1+WP2，得到的是缺三分之二测试覆盖的残缺单元——`review-mu1-fixes-r1.md` 已明确把 `schema_drift.rs` 第三个方向的枚举断言缺口（`AgentConnectedState`/`AgentDisconnectedState` 未进四条硬编码 `assert_eq!`）移交 TP1；plan.md Coverage Index 也把 IV1/EV1 的多条 Requirement 绑定到 TP1。候选若不含 TP1，PV1 的 fixture 计数、manifest 完整性与漂移门禁都无法按计划判定，独立 review 也无对象可审。**故本轮明确不构建候选。**

2. **已验收上游证据已核实可引用。** WP1/WP2 的独立 review 与修复复核结论 PASS 见 `reports/review-mu1-fixes-r1.md`（result PASS，0 CRITICAL/MAJOR/MINOR/SUGGESTION，WP1/WP2 各自 `npm run check` 与 `cargo test --locked -p sync-protocol --all-features` exit 0）。该证据绑定的是两个工作区各自的未提交差异（`WORKTREE-DIRTY` 目标），与 MU1 的候选阶段（`stage: candidate`）不是同一 target_revision，组装成候选后**不能以它顶替候选阶段的 PV1 与 candidate review**，必须在候选上重跑并另取独立 review。

3. **卫生风险（组装时规避，不阻断）。** `.worktrees/tp1` 下有未跟踪的 `wp1.patch` / `wp2.patch`。组装 TP1 时必须按 TP1 的实际新增文件清单逐个取用，不得整树搬运，否则会把这两个临时文件带进候选。

4. **非阻断的既有不一致（沿用 `review-mu1-fixes-r1.md` 的登记，本轮未判、也未修复）。** `SessionReadResources.config_options` 的 Rust 扁平 `Option<Vec<ConfigOptionView>>` 与 schema `$defs/sessionItem.config_options` 的按会话归组形状不一致——基线 `353ba6e` 即如此，非本单元引入。

## 下一轮（候选构建与合入）的前置条件

按顺序，全部满足才继续：

1. 收到 TP1 的交付（自有差异 + 工作区卫生恢复），并按 WP1 → WP2 → TP1 的注册顺序在 `.worktrees/mu1-merge` 内组装三份差异；共享文件冲突若涉及需求/接口取舍则回交主 Agent 裁决，产品缺陷回 WP1/WP2 作者，测试缺陷回 TP1。
2. 在候选工作区执行 MU1 的 Project Verify：cwd = `.worktrees/mu1-merge` 的 `npm run check`，以及 `cargo test --locked -p sync-protocol --all-features`（仓库根 `target/`），逐条记录命令、退出码与日志。
3. 把候选差异交主 Agent 调度**独立** candidate review（reviewer 不得是 WP1/WP2/TP1 的作者或本 merger），等待与候选提交绑定的有效报告。
4. 由主 Agent 在 `verification.md` 固化 `agentic-premerge` 块（含 `delivery_unit: MU1`），再在候选工作区运行 `openspec-agentic workflow check --change sync-scope-and-pwa-client --stage premerge --planning-root <权威规划根> --json`；仅在 PASS 且 `refs/heads/main` 仍为 `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f` 时继续。
5. 仅在 PASS 之后把该块持久化为版本化 receipt，并把单元全部工作包、目标/候选提交与结果（含 `handoff_index`）返回主 Agent 登记 `## Premerge History`（本角色不改 `verification.md`）。
6. 合入前再次核对目标基线，用串行协调防止竞态；合入后核对实际结果与候选一致并执行主分支 Project Verify。

## 确认

- **未合入任何内容**：`refs/heads/main` 仍为 `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`（本轮开始与结束时各核实一次，一致），主检出 `git status --porcelain=v1 -b` 仍为 `## main...origin/main`。
- 未执行 `git push`，未 rebase / 未改写 `refs/heads/main` 历史，未创建 tag，未修改 `plan.md` / `tasks.md` / `verification.md`。
- 未修改三个源工作区（只执行了只读的 `rev-parse` / `diff` / `status` / `hash-object`）。
- 未运行任何 Project Verify，未生成或伪造 receipt；报告中出现的 PASS 仅限我实际执行并附上输出的检查（TARGET-REF、CANDIDATE-CREATE、SOURCE-DIFF-SNAPSHOT、TP1-OWNED-ARTIFACTS-ABSENT、WORKTREE-HYGIENE）。
- 候选工作区 `.worktrees/mu1-merge` 与分支 `merge/mu1-candidate` 保留给下一轮复用，其中不含任何产品改动；待 TP1 交付后继续。

BLOCKED