<!-- MU1a merger 的 assemble 阶段报告（Round 1）。本轮做目标核实 + 按显式文件清单组装候选；未跑候选 Project Verify、未跑 premerge 门禁、未生成 receipt、未合入。取代同一执行者在 MU1 旧编成下的 merge-mu1-integrate-r1.md（该轮因 TP1 未交付而 BLOCKED；用户裁决把 MU1 拆为 MU1a/MU1b 后，MU1a 不再含 TP1）。 -->

task_id: "MU1a"
role: merger
phase: integrate
agent_context:
  agent_id: "MergerMU1"
  isolation: "fork_turns=none（独立合入执行者；未参与 WP1/WP2 的实现、review 或修复对话，仅继承调度方传入的角色契约、单元编成与规划更新信息）"
target_revision: "refs/heads/main = 353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f（本轮组装前后各核实一次，未移动）；候选提交 = 683dbbb8727b3ddef7e8b2a3d3e5dce056fbf0e4"
scope: "MU1a（模式 independent，成员 WP1 + WP2，顺序组 1，无 TP、无上游）。(1) 组装前重跑目标引用核实；(2) 复用候选工作区 .worktrees/mu1-merge（分支 merge/mu1-candidate，基线 353ba6ef），按调度方给出的显式文件清单与 plan.md Shared File Ownership 的注册顺序（WP1 → WP2）组装差异，逐文件以 git hash-object 比对来源，不做整树复制；(3) 落一个固定候选提交供后续独立检视；(4) 记录待办门禁。刻意不做：候选 Project Verify、候选差异独立 review、workflow check --stage premerge、receipt、合入——按调度指令这些等 DDR Round 6 绑定当前 planningDigest 后再走（premerge 门校验 planningDigest，规划摘要一变 receipt 须重出）。"
changes: "只动候选工作区，主检出与两个源工作区零写入。候选工作区内：新增/修改/删除 29 个跟踪路径（WP1 的 11 个 + WP2 的 7 个，其中 docs/SYNC_PROTOCOL.md 为共享文件按 WP1→WP2 叠加 = 合并后 28 个路径中该文件只计一次；另加 WP1 的 12 个新夹具为新增路径），落为一个候选提交 `683dbbb`；删除 `.merge-staging/tp1-current.patch`（旧阻塞期快照，内容是 WP1+WP2 的合并视图而非 TP1 交付，按调度指令作废）；保留 `.merge-staging/wp1-files/`、`wp2-files/` 逐文件补丁以便审计重建；`node_modules` 仍为指向仓库根的目录联接。未修改 plan.md / tasks.md / verification.md，未改任何产品语义。"
checks:
  - id: "TARGET-REF-RECHECK"
    command: "git rev-parse --abbrev-ref HEAD; git rev-parse --verify refs/heads/main; git log --oneline -1 refs/heads/main（cwd=D:\\Project\\acp-remote，组装前）"
    scope: "目标引用核实（MU1→MU1a 重新核实）"
    environment: "主检出 D:\\Project\\acp-remote"
    exit_code: 0
    log_path: "本报告「目标引用核实」小节"
    result: "`main` / `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f` / `353ba6e Merge remote-tracking branch 'origin/main' into main`。目标自规划以来未移动，与 MU1a 的编成基线同一提交。组装后再次核实同值（见 WORKTREE-HYGIENE）。"
  - id: "ASSEMBLE-WP1"
    command: "逐文件 `git -C .worktrees/wp1 diff HEAD --binary --output=<候选>/.merge-staging/wp1-files/<path>.patch -- <path>` 后在候选 `git apply`（11 个跟踪文件）+ 12 个新夹具按显式路径 cp 并 `git hash-object` 逐个比对"
    scope: "WP1 差异组装"
    environment: ".worktrees/wp1（只读 diff）→ .worktrees/mu1-merge"
    exit_code: 0
    log_path: "本报告「组装结果」小节的文件清单与哈希表"
    result: "11/11 跟踪文件 `APPLIED`（含 `fixtures/sync/v1/valid/sync-snapshot-chunk-config-options.json` 的删除，其 body.resource 为 D1 已移出封闭词表的 `config_options`，随 WP1 一并删除）；12/12 新夹具 `OK` 且复制后 blob 哈希与来源逐一相等。"
  - id: "ASSEMBLE-WP2"
    command: "逐文件 `git -C .worktrees/wp2 diff HEAD --binary` → 候选 `git apply`（6 个独占文件）；共享文件 `docs/SYNC_PROTOCOL.md` 先试 `git apply --3way`（因索引已是 WP1 版本，index 不匹配而失败），改用普通 `git apply` 成功，hunk 以 offset -1 落在 :1025"
    scope: "WP2 差异组装（含唯一共享文件）"
    environment: ".worktrees/wp2（只读 diff）→ .worktrees/mu1-merge（WP1 已就位）"
    exit_code: 0
    log_path: "本报告「组装结果」小节"
    result: "6/6 独占文件 `APPLIED`；共享文档 `docs/SYNC_PROTOCOL.md` `Applied patch cleanly`，唯一 hunk 落在 §10.3（offset -1）。`git status` 无冲突标记、无未合并路径。"
  - id: "HASH-PROVENANCE"
    command: "逐文件 `git hash-object`（工作区字节）与 `git ls-files -s`（索引 blob）分别与 .worktrees/wp1、.worktrees/wp2 比对"
    scope: "来源可追溯性证明"
    environment: "三个工作区"
    exit_code: 0
    log_path: "本报告「组装结果」小节的哈希表"
    result: "13 个独占路径（WP1 7 + WP2 6）的工作区哈希与索引 blob 全部 MATCH 对应来源工作区；12 个新夹具复制后哈希全部一致；候选内不存在清单以外被改动的路径（`git status --porcelain` 仅输出上述 29 个路径）。"
  - id: "SHARED-DOC-COMPOSITION"
    command: "diff -u（候选 docs/SYNC_PROTOCOL.md 分别对 wp1 与 wp2）并提取 @@ hunk 头"
    scope: "共享文档合并正确性"
    environment: ".worktrees/mu1-merge vs wp1 / wp2"
    exit_code: 0
    log_path: "本报告「共享文件」小节"
    result: "候选对 WP1 只差 1 个 hunk（@@ -1025,13 +1025,24 @@，即 WP2 的 §10.3 视图字段）；候选对 WP2 差 6 个 hunk（@@ -100 / -736 / -762 / -792 / -1284 / -1303 @@，即 WP1 的 §3.3 常量清单、§9.4 资源范围与 chunkCount、§11.5 limit 口径）。两侧编辑落在不同小节，叠加后互不覆盖、无语义冲突。"
  - id: "TP1-MIRROR-STALE"
    command: "diff -u .worktrees/wp1/docs/SYNC_PROTOCOL.md .worktrees/tp1/docs/SYNC_PROTOCOL.md | grep -c '^[+-]'"
    scope: "佐证调度方「不得使用 tp1-current.patch」的判断"
    environment: ".worktrees/tp1"
    exit_code: 0
    log_path: "本报告「发现」小节"
    result: "52 行差异。tp1 工作区的镜像缺 WP1 的 M1/M2/M3 修复（§3.3 清单无 `limit`、`chunkCount` 仍是旧的 `8`/`6` 而非修复后的 `1..=3`、快照资源清单未收窄），即它是从**修复前**的 wp1.patch/wp2.patch 拼出来的。该镜像不可作为来源或参照，已删除 tp1-current.patch。"
  - id: "WORKTREE-HYGIENE"
    command: "git status --porcelain -b（主检出、wp1、wp2、mu1-merge）+ git worktree list"
    scope: "未扰动源工作区与主检出的证明"
    environment: "全部工作区"
    exit_code: 0
    log_path: "本报告「工作区卫生」小节"
    result: "主检出 `## main...origin/main`，跟踪文件零改动（未跟踪仍只有 `.target-wt/`、`.worktrees/`、`openspec/changes/sync-scope-and-pwa-client/`）；wp1 仍为 10 个 ` M` + 1 个 ` D` + 12 个未跟踪夹具，wp2 仍为 7 个 ` M` + 0 未跟踪，与组装前逐项一致；源工作区 HEAD 仍为 353ba6ef。组装全程只对源工作区执行 `rev-parse` / `diff` / `hash-object` / `status` 只读命令。"
  - id: "NOT-RUN / BY-INSTRUCTION"
    command: "候选 Project Verify（npm run check；cargo test --locked -p sync-protocol --all-features）、独立候选 review、workflow check --stage premerge、receipt、合入"
    scope: "MU1a 候选与合并门禁"
    environment: "NOT_APPLICABLE"
    exit_code: NOT_APPLICABLE
    log_path: "NOT_AVAILABLE"
    result: "按调度指令未执行，等 DDR Round 6 绑定当前 planningDigest 后再走。未运行任何 Project Verify，未生成也未伪造 receipt，报告中的 PASS 仅限实际执行并附输出的检查。"
issues: "0 CRITICAL / 0 MAJOR / 0 MINOR / 0 SUGGESTION（组装层面）。2 项登记：发现（F1，informational，`.worktrees/tp1` 镜像过期，已规避）与遗留门禁（G1，MU1a 候选尚未验证/未检视/未过 premerge，按指令等待）。"
result: BLOCKED
evidence_paths:
  - "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1a-assemble-r1.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/review-mu1-fixes-r1.md"
  - ".worktrees/mu1-merge/.merge-staging/wp1-files/ 与 wp2-files/（逐文件补丁）"

worktree_handoff:
  - work_package: MU1a-CANDIDATE
    attempt: 1
    worktree: "D:\\Project\\acp-remote\\.worktrees\\mu1-merge"
    baseline_revision: "353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f"
    executor: "MergerMU1"
    received_at: "2026-10-03"

handoff_index:
  - task_id: "MU1a"
    work_package: DELIVERY
    role: merger
    phase: integrate
    round: NOT_APPLICABLE
    stage: candidate
    target_revision: "683dbbb8727b3ddef7e8b2a3d3e5dce056fbf0e4"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1a-assemble-r1.md"
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "MU1a 候选提交 683dbbb8727b3ddef7e8b2a3d3e5dce056fbf0e4 已组装完成并落在分支 merge/mu1-candidate，基线 353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f，目标 refs/heads/main 已核实未移动。组装本身可核（逐文件 blob 哈希与来源工作区逐一相等，无清单外改动）。但该行按 PENDING 记 BLOCKED，因为：候选阶段的 PV1（npm run check 与 cargo test --locked -p sync-protocol --all-features）、独立 candidate review、`agentic-premerge` 块与版本化 receipt 均尚未产出，按调度指令等 DDR Round 6 绑定当前 planningDigest 后执行；planningDigest 一旦变化，全部候选阶段证据与 receipt 须重出。门禁：候选 PV1 PASS + 独立 review PASS + premerge PASS + 目标引用未移动，才允许合入。"
    source_evidence: NOT_APPLICABLE

---

## 目标引用核实

组装前（`D:\Project\acp-remote`）：

```
> git rev-parse --abbrev-ref HEAD
main
> git rev-parse --verify refs/heads/main
353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f
> git log --oneline -1 refs/heads/main
353ba6e Merge remote-tracking branch 'origin/main' into main
```

组装后再次核实，`refs/heads/main` 仍为 `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`，主检出 `git status --porcelain=v1 -b` 仍为 `## main...origin/main`。目标可核实且未移动，**refs/heads/main 全程未被触碰**。

## 候选工作区

| 项 | 值 |
| --- | --- |
| 路径 | `D:\Project\acp-remote\.worktrees\mu1-merge`（复用上一轮已建的工作区，未重建） |
| 分支 | `merge/mu1-candidate` |
| 基线 | `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f` |
| 候选提交 | **`683dbbb8727b3ddef7e8b2a3d3e5dce056fbf0e4`**（`feat(sync): MU1a 契约候选——WP1 快照收窄/分页命令与 WP2 派生事件字段`，29 files changed, +748 / −225） |
| 工作区状态 | 干净（`git status --porcelain=v1 -b` 仅 `## merge/mu1-candidate`） |
| 依赖 | `node_modules` → `D:\Project\acp-remote\node_modules` 目录联接（只读消费，与 wp1/wp2 同模式） |
| 暂存区 | `.merge-staging/wp1-files/`、`.merge-staging/wp2-files/` 逐文件补丁（未跟踪，已在 `.git/info/exclude`）；`tp1-current.patch` 已删除 |

merger 角色约定「复用本单元已有的执行 worktree，不新建」。本单元两成员各有自己的执行工作区且均处于已 review/待合入状态，复用会污染作者工作区，故沿用上一轮在单元内新建的合入工作区；组装全程只对源工作区执行只读命令。

## 组装结果

只取调度方给出的显式清单，逐文件应用与比对，无整树复制。

### WP1（`.worktrees/wp1`，`feat/wp1-sync-snapshot-contract`）— 11 跟踪 + 12 新增

| 路径 | 候选 blob | 来源 | 结果 |
| --- | --- | --- | --- |
| `schemas/sync/v1/sync.schema.json` | `f97f0134721aff5a802fb1e039e556e05549f39a` | wp1 | MATCH |
| `schemas/sync/v1/command.schema.json` | `3a7423ec1fcf331cb8af1aac00b0b3e3f27a91dd` | wp1 | MATCH |
| `crates/sync-protocol/src/sync.rs` | `6cbfc535f7c68d9dfc64f133fb5a173ecb37fb6c` | wp1 | MATCH |
| `crates/sync-protocol/src/command.rs` | `a87aac085d6a3350f7d37018021ef7b69788edcb` | wp1 | MATCH |
| `crates/sync-protocol/tests/envelope_fixtures.rs` | `0e65e8bf0f45b050d56034c45aef2fa17aa2b815` | wp1 | MATCH |
| `docs/NODE_LINK_PROTOCOL.md` | `41eb816bba55afccd498afb7635ad77e20d598b7` | wp1 | MATCH |
| `fixtures/sync/v1/manifest.json` | `54f5418f6512ea9b53cd6f4b0abc88c7a9a36ee7` | wp1（WP2 零改动，见 `review-mu1-fixes-r1.md`） | MATCH |
| `fixtures/sync/v1/valid/sync-snapshot-begin.json` | `3cf9f8fa69c9d91766f6eb5a6bc0efdba70a1d93` | wp1 | MATCH |
| `fixtures/sync/v1/valid/sync-snapshot-end.json` | `004b4701f6a8e21858e0c4ae363e186b562e3ad1` | wp1 | MATCH |
| `fixtures/sync/v1/valid/sync-snapshot-chunk-config-options.json` | — | wp1（删除） | 候选内已删除，与 wp1 一致 |
| `fixtures/sync/v1/valid/command-session-read-paged.json` | `ac2c3c3322d09dfce5662024904bd9729f8ad621` | wp1 新增 | OK |
| `fixtures/sync/v1/valid/command-session-read-default-page.json` | `cc916984d4277eb553f954f6858d7237fb7ac799` | wp1 新增 | OK |
| `fixtures/sync/v1/valid/command-result-session-read-paged-completed.json` | `dde7749fc9e151172549c8d042f038f312dd01ed` | wp1 新增 | OK |
| `fixtures/sync/v1/valid/command-result-session-read-last-page-completed.json` | `899b6a003fc93f02d1abe6f20917964e563d13be` | wp1 新增 | OK |
| `fixtures/sync/v1/valid/sync-snapshot-sessions-only-begin.json` | `20dba1b7fe6e22fb540b1eaf738e7583dd6b3c48` | wp1 新增 | OK |
| `fixtures/sync/v1/valid/sync-snapshot-sessions-only-chunk.json` | `e98280114bde0b0b63dad250b888cfbf2797a479` | wp1 新增 | OK |
| `fixtures/sync/v1/valid/sync-snapshot-sessions-only-end.json` | `769642ec6afca69e86cfb9698a59fa1ceb0bed76` | wp1 新增 | OK |
| `fixtures/sync/v1/invalid/command-result-session-read-missing-has-earlier.json` | `b86abc78506968914472c19fa0c5172b32e4b3ed` | wp1 新增 | OK |
| `fixtures/sync/v1/invalid/command-session-read-single-field-before.json` | `6312770344b2c8d3c07d682b76ec8ee0a1e7b163` | wp1 新增 | OK |
| `fixtures/sync/v1/invalid/snapshot-begin-chunk-count-too-large.json` | `49aa5392d75e807194da9a0b96cc944908154611` | wp1 新增 | OK |
| `fixtures/sync/v1/invalid/snapshot-chunk-detail-resource-config-options.json` | `c1610ad33f6ed1e20f63bbbab5a6700c5a8f3753` | wp1 新增 | OK |
| `fixtures/sync/v1/invalid/snapshot-chunk-detail-resource-messages.json` | `9c43e9d4b099be1229a1c05c38316e54a1641bf1` | wp1 新增 | OK |

### WP2（`.worktrees/wp2`，`feat/wp2-derived-event-fields`）— 7 跟踪（含共享文档）

| 路径 | 候选 blob | 来源 | 结果 |
| --- | --- | --- | --- |
| `schemas/sync/v1/event-views.schema.json` | `4a8898b2557af8fa17ccfb8209115a54b524925b` | wp2 | MATCH |
| `crates/sync-protocol/src/views.rs` | `03a2eed766624a23c63f64827e15fc937938cddc` | wp2 | MATCH |
| `docs/ACP_COMPATIBILITY_MATRIX.md` | `794046af4923cfc7d8faa2fc02520df5bfd55ce2` | wp2 | MATCH |
| `fixtures/sync/v1/valid/view-agent-connected.json` | `669b43fe871237da8cea5deefd31a7a9d0fb0d0d` | wp2 | MATCH |
| `fixtures/sync/v1/valid/view-agent-disconnected.json` | `85449f1bf83c0f7672346dd6b6fde0869ccc6257` | wp2 | MATCH |
| `fixtures/sync/v1/valid/view-file-changed.json` | `7403004d06810a4c0b62ed66bfeecf736f28aaa5` | wp2 | MATCH |
| `docs/SYNC_PROTOCOL.md` | （共享，无单一来源 blob） | wp1 先落、wp2 §10.3 后叠 | 见下节 |

`git status --porcelain` 在候选内只输出上述 29 个路径，无清单外改动、无冲突标记。

## 共享文件

唯一共享文件是 `docs/SYNC_PROTOCOL.md`（plan.md Shared File Ownership：WP1 改 §9.4/§11.5，WP2 改 §10.3，注册顺序 WP1 → WP2）。

- WP1 版本先应用，随后 WP2 的补丁以普通 `git apply` 干净落下，hunk 报 `succeeded at 1025 (offset -1 lines)`；`git apply --3way` 因索引已是 WP1 版本（`does not match index`）不可用，属预期，不影响结果。
- 候选对 WP1 只差 **1 个 hunk**（`@@ -1025,13 +1025,24 @@`，WP2 的 §10.3 节点级事件/派生字段说明）。
- 候选对 WP2 差 **6 个 hunk**（`@@ -100 @@` §3.3 常量清单补 `limit`、`@@ -736 @@` 与 `@@ -762 @@` §9.4 快照范围与 chunkCount、`@@ -792 @@` §9.4 资源表、`@@ -1284 @@` 与 `@@ -1303 @@` §11.5 `limit` 口径），全部是 WP1 的内容且完整保留。
- 结论：两侧编辑落在不同小节，叠加后互不覆盖，**无需要裁决的语义冲突**，无需回交主 Agent。

`fixtures/sync/v1/manifest.json` 在计划中登记了 WP1 → WP2 → TP1 三写者，但 WP2 对该文件零改动（`review-mu1-fixes-r1.md` 已核实其 7 个改动文件中不含 manifest），故本单元的 manifest 直接采用 WP1 版本 `54f5418f…`；TP1 追加条目归 MU1b。

## 发现

**F1（informational，已规避）**：`.worktrees/tp1` 的工作区镜像**过期**。把 `.worktrees/wp1/docs/SYNC_PROTOCOL.md` 与 `.worktrees/tp1/docs/SYNC_PROTOCOL.md` 对比有 52 行差异，tp1 副本缺 WP1 的 M1/M2/M3 修复（§3.3 常量清单无 `limit`、`chunkCount` 仍是旧的 `8`/`6` 而非修复后的 `1..=3`、快照资源清单未收窄）——说明它是从**修复前**的 `wp1.patch`/`wp2.patch` 拼出来的。这独立佐证了调度方「`tp1-current.patch` 不要用」的判断：该文件不是 TP1 的交付。我已删除 `tp1-current.patch`，并全程未从 tp1 取用任何内容。TP1 工作区另有两个未跟踪的 `wp1.patch`/`wp2.patch` 临时文件，属 TP1 作者的本地物，MU1b 组装时须按显式清单挑取。

**G1（门禁待办，非缺陷）**：MU1a 候选尚未验证、尚未独立检视、未过 premerge、无 receipt。按调度指令等 DDR Round 6 绑定当前 planningDigest 后再走。另注：plan.md Shared File Ownership 已登记 `crates/sync-protocol/tests/envelope_fixtures.rs` 为 WP1 → TP1 共享，且说明两个用例计数常量「MU1a 由 WP1 定其中间值，MU1b 由 TP1 在 MU1a 合入提交上定最终值」——因此 `npm run check` 的 `check:assets`/计数类门禁在 MU1a 候选上是否通过，必须以候选实测为准，本轮未运行、不作任何预判。

## 下一轮（候选验证与合入）的前置条件

1. DDR Round 6 PASS 且 `verification.md` 的规划审查绑定当前 planningDigest（否则 receipt 需重出，本候选的验证证据也需重跑）。
2. 在候选工作区执行 MU1a 的 PV1：cwd = `.worktrees/mu1-merge` 的 `npm run check`，以及 `cargo test --locked -p sync-protocol --all-features`（仓库根 `target/`），逐条记录命令、退出码与日志；若任一 FAIL，先判断是候选组装问题还是 MU1a/MU1b 拆分后的门禁归属问题，再决定重建或回交作者。
3. 把候选差异交主 Agent 调度**独立** candidate review（reviewer 不得是 WP1/WP2 的作者或本 merger），取得绑定 `683dbbb` 的有效报告；`review-mu1-fixes-r1.md` 绑定的是工作区脏状态，不能顶替候选阶段证据。
4. 主 Agent 在 `verification.md` 固化 `agentic-premerge` 块（含 `delivery_unit: MU1a`），再在候选工作区运行 `openspec-agentic workflow check --change sync-scope-and-pwa-client --stage premerge --planning-root <权威规划根> --json`；仅在 PASS 且 `refs/heads/main` 仍为 `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f` 时继续。
5. 仅在 PASS 之后持久化版本化 receipt，并把单元全部工作包、目标/候选提交与结果（含 `handoff_index`）返回主 Agent 登记 `## Premerge History`（本角色不改 `verification.md`）。
6. 合入前再次核对目标基线并串行防竞态；合入后核对实际结果与候选一致，执行主分支 Project Verify。

## 确认

- **`refs/heads/main` 全程未被触碰**：组装前后各核实一次，均为 `353ba6ef4b9a32c0e73f53c4d5b5cb1ececf2d0f`；主检出跟踪文件零改动。
- **未合入任何内容**：候选只存在于本地分支 `merge/mu1-candidate`，主分支未前移。
- 未执行 `git push`，未 rebase，未改写 `refs/heads/main` 历史，未创建 tag，未修改 `plan.md` / `tasks.md` / `verification.md`。
- 未修改 wp1、wp2、tp1 三个源工作区（只执行只读命令），也未从 tp1 取用任何内容。
- 未运行任何 Project Verify，未生成也未伪造 receipt；报告中出现的 PASS/MATCH 仅限我实际执行并附输出的检查。

BLOCKED