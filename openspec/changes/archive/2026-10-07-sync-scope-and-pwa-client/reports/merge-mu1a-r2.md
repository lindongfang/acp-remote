```agentic-handoff
version: 1
task_id: "6.6"
role: merger
phase: merge
agent_context:
  agent_id: "merger-mu1a-2"
  isolation: "fork_turns=none（新建独立 merger，未参与 WP1/WP2 的任何实现、修复、候选组装或检视对话；仅接收调度方传入的角色契约、单元编成、plan.md/tasks.md/verification.md 与目标/候选提交 SHA）。本轮只做「推进主分支 → 复跑 premerge 门禁 → 主分支回归」，未重跑候选 PV1、未重做候选组装。"
target_revision: "ad9ad3c6b532ce0cc9366c8cc34a217f9f466933"
scope: "MU1a 合入执行（Attempt 2，执行者 merger-mu1a-2）。(1) 合入前机械核实目标仓库/引用/当前提交/工作区干净；(2) 以 `git merge --ff-only` 把 refs/heads/main 从 b964ae3 快进到候选 ad9ad3c；(3) 合入后复跑 premerge 门禁并抄录完整 JSON；(4) 主分支回归 [PV1]（npm run verify）并落盘证据；(5) 核对合并相对候选无新增差异。合入前基线由 main 事先核实（ancestor 成立、29 files/+746/-222、7 commits），本报告独立复跑逐项确认。有意不做：推送、发布、归档、回滚；不改 verification.md；不删除任何分支或 worktree。"
changes: "refs/heads/main 由 b964ae3 快进到 ad9ad3c（git merge --ff-only，Fast-forward，exit 0）。主检出仅新增两份报告文件：本报告与 reports/PV1-main-mu1a.log（tasks 6.7 指定的证据路径）。未修改任何跟踪的产品文件、schema、fixture、plan.md、tasks.md 或 verification.md。未创建/删除分支或 worktree；未在候选 worktree 之外写入。"
checks:
  - id: "PREMERGE-GATE-6.6"
    work_package: WP1
    command: "npx --quiet --no-install openspec-agentic workflow check --change \"sync-scope-and-pwa-client\" --stage premerge --json（cwd=D:/Project/acp-remote，合入后）"
    scope: "合入后复跑 premerge 门禁（tasks 6.6 要求）"
    environment: "主检出；项目本地引擎 @dongfanglin/openspec-agentic 0.4.0"
    exit_code: 1
    log_path: "reports/merge-mu1a-r2.md（本报告「门禁原始输出」小节）"
    result: "**FAIL（exit 1）**，`result: FAIL`、`candidateCommit: ad9ad3c…`、`targetCommit: ad9ad3c…`、8 条 errors。门禁未 PASS。根因见「门禁失败根因」小节：两类成因为 (A) 门禁把「target = refs/heads/main」与「candidate = HEAD」比较，快进后二者恒等 → 2 条结构性恒假错误；(B) receipt 的 evidence.sha256 写成裸十六进制、缺 `sha256:` 前缀 → 4 条摘要比对失败 + 2 条历史行联动错误。候选内容与主分支回归均无缺陷。"
  - id: "FF-ONLY-MERGE-6.6"
    work_package: WP1
    command: "git rev-parse refs/heads/main merge/mu1a-candidate-r1 && git merge --ff-only merge/mu1a-candidate-r1 && git rev-parse HEAD && git rev-parse refs/heads/main"
    scope: "唯一允许的写操作：快进 refs/heads/main"
    environment: "主检出 D:/Project/acp-remote；Windows"
    exit_code: 0
    log_path: "reports/merge-mu1a-r2.md（本报告「实际合入」小节）"
    result: "**PASS**。输出 `Updating b964ae3..ad9ad3c` + `Fast-forward`，exit 0。合入后 HEAD = refs/heads/main = `ad9ad3c6b532ce0cc9366c8cc34a217f9f466933`。未使用任何非快进 merge。"
  - id: "MAIN-REGRESSION-6.7"
    work_package: WP1
    command: "CARGO_TARGET_DIR=D:/Project/acp-remote/.target-wt/mu1a-main npm run verify（cwd=D:/Project/acp-remote，HEAD=ad9ad3c）"
    scope: "合入后的主分支回归 [PV1] = check 十道 + check:rust 三条"
    environment: "Windows；Node v22.22.0 / npm 10.9.4；独占 CARGO_TARGET_DIR=.target-wt/mu1a-main"
    exit_code: 0
    log_path: "reports/PV1-main-mu1a.log"
    result: "**PASS exit 0**（VERIFY_EXIT=0，日志末行）。check 十道全绿 + check:rust 三条全绿。未命中 daemon_lifecycle 已知 flake。详见「主分支回归」小节。"
  - id: "POST-MERGE-DELTA-6.8"
    work_package: WP1
    command: "git diff --stat merge/mu1a-candidate-r1 HEAD; git rev-parse HEAD^{tree} merge/mu1a-candidate-r1^{tree}"
    scope: "合并相对候选的新增差异（预期为空）"
    environment: "主检出"
    exit_code: 0
    log_path: "reports/merge-mu1a-r2.md（本报告「合并相对候选的差异」小节）"
    result: "**PASS**——`git diff --stat ... HEAD` 空输出；两侧树 SHA 均为 `6b206e6717449bcad64b00d1d674db44ba6768aa`。纯快进，合并结果与候选树逐字节相同，无新增差异。"
  - id: "DISPATCH-ACK"
    work_package: NOT_APPLICABLE
    command: "npx openspec-agentic dispatch --change \"sync-scope-and-pwa-client\" --wp MU1a --executor merger-mu1a-2 --ack（及回退 --wp WP1）"
    scope: "第一步「确认接收」"
    environment: "主检出；引擎 0.4.0"
    exit_code: 1
    log_path: "reports/merge-mu1a-r2.md（本报告「接收确认」小节）"
    result: "**BLOCKED（结构上不可满足）**。引擎 dispatch.mjs:307 要求 `ACTIVE_STATES={coding,reviewing,fixing}` 且 `current.executor===executor`；台账中 MU1a=superseded、WP1/WP2=ready-to-merge，均不在 ACTIVE_STATES，且 CLI 无 merger 角色。按调度方给的回退办法用 `--wp WP1` 亦同样失败（同一校验）。未改动台账（不重开 WP、不伪造 ack）。"
handoff_index:
  - task_id: "6.6"
    work_package: WP1
    role: merger
    phase: merge
    round: NOT_APPLICABLE
    stage: main
    target_revision: "ad9ad3c6b532ce0cc9366c8cc34a217f9f466933"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1a-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "refs/heads/main 以 git merge --ff-only 由 b964ae3 快进到候选 ad9ad3c，exit 0；合入后 HEAD = refs/heads/main = ad9ad3c。主检出跟踪文件零改动。"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.6"
    work_package: WP1
    role: merger
    phase: merge
    round: NOT_APPLICABLE
    stage: main
    target_revision: "ad9ad3c6b532ce0cc9366c8cc34a217f9f466933"
    evidence_type: CHECK
    evidence_id: PREMERGE-GATE-6.6
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1a-r2.md"
    result: FAIL
    evidence_status: NEW
    applicability_basis: "合入后复跑 `workflow check --stage premerge --json` 返回 result=FAIL（exit 1）、8 条 errors。根因为 (A) 快进后门禁的 target(=refs/heads/main) 与 candidate(=HEAD) 恒等，2 条结构性错误无法消除；(B) receipt 的 evidence sha256 缺 `sha256:` 前缀致 6 条摘要/历史联动错误。均非候选内容缺陷。"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.7"
    work_package: WP1
    role: merger
    phase: merge
    round: NOT_APPLICABLE
    stage: main
    target_revision: "ad9ad3c6b532ce0cc9366c8cc34a217f9f466933"
    evidence_type: CHECK
    evidence_id: MAIN-REGRESSION-6.7
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1a-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "主分支回归 npm run verify 于 HEAD=ad9ad3c 实测 exit 0（VERIFY_EXIT=0）；check 十道 + check:rust 三条全绿。原始日志 reports/PV1-main-mu1a.log（sha256 994450ca…，2218 行）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.8"
    work_package: WP1
    role: merger
    phase: merge
    round: NOT_APPLICABLE
    stage: main
    target_revision: "ad9ad3c6b532ce0cc9366c8cc34a217f9f466933"
    evidence_type: CHECK
    evidence_id: POST-MERGE-DELTA-6.8
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1a-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "合并相对候选零差异（diff 空、两树 SHA 同为 6b206e67…），主 Agent 据此可复用原候选 review ID review-mu1a-candidate-r1（PASS）。6.8 的独立 review 本身仍待 main 派发（待补）。"
    source_evidence: NOT_APPLICABLE
issues: "0 CRITICAL。1 项未解决阻断（PREMERGE-GATE-6.6 非 PASS，含 receipt 摘要格式缺陷，归 main 处置）。1 项过程缺陷（dispatch --ack 结构上不可满足，见 DISPATCH-ACK）。6.8 独立 review 待 main 派发（待补）。"
result: FAIL
evidence_paths:
  - "openspec/changes/sync-scope-and-pwa-client/reports/merge-mu1a-r2.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/PV1-main-mu1a.log"
  - "openspec/changes/sync-scope-and-pwa-client/reports/receipt-mu1a.md"
  - "openspec/changes/sync-scope-and-pwa-client/reports/PV1.log"
resource_cleanup: "新增并独占使用 CARGO_TARGET_DIR=D:/Project/acp-remote/.target-wt/mu1a-main（仅本次主分支回归；保留供 main 复核，可随时删除）。未创建分支/worktree，未删除任何分支/worktree。主检出跟踪文件零改动（git status --porcelain 仅三项目录级未跟踪项 + 本报告目录）。"
```

# MU1a 合入执行报告（Round 2）

> **结论速览**：快进**成功**（main `b964ae3` → `ad9ad3c`，`--ff-only`）；主分支回归 [PV1] **全绿**（exit 0）；合并相对候选**零新增差异**。**premerge 门禁复跑为 FAIL**——根因是门禁在快进后 `target == candidate` 使两条错误恒真，加上 receipt 的 `evidence.sha256` 缺 `sha256:` 前缀。**门禁 FAIL 不代表候选内容或回归有问题**；完整 evidence 与根因见下，由 main 处置。**未修改 verification.md**，未删除任何分支或 worktree。

---

## 0. 接收确认（第一步）与回退说明

按调度方指令尝试：

```
npx --quiet --no-install openspec-agentic dispatch --change "sync-scope-and-pwa-client" --wp MU1a --executor merger-mu1a-2 --ack
```

结果：**失败**，`openspec-agentic: 接收确认必须绑定当前活跃工作包及其实际执行者`（exit 1）。

按指令的回退办法改用 `--wp WP1 --executor merger-mu1a-2 --ack`，**同样失败**（同一校验）。

机械定位（`node_modules/@dongfanglin/openspec-agentic/src/dispatch.mjs:36,307`）：

- `ACTIVE_STATES = new Set(['coding','reviewing','fixing'])`；
- `acknowledgeDispatch`：`if (!current || !ACTIVE_STATES.has(current.state) || current.executor !== executor) throw ...`；
- 台账实测（`dispatch-queue.jsonl` reduce 结果）：`WP2 = ready-to-merge`、`WP1 = ready-to-merge`、`MU1a = superseded`。三者均不在 `ACTIVE_STATES`。

即：**`--ack` 在本状态下结构上不可满足**——MU1a 是交付单元而非工作包 ID（其 superseded 记录已由 main 登记为「dispatch 台账不支持交付单元粒度」），而 WP1/WP2 已 `ready-to-merge`，`--ack` 只接受活跃态 + 匹配执行者。CLI 亦无 merger/交付单元粒度。

**处置**：如实记录，未改动台账（未 `--reopen` WP1/WP2、未伪造 ack、未新增 MU1a 记录——重开工作包会使台账状态回退，属越权）。该缺陷不作为合入阻断，因为「推进主分支 + 回归」是本轮真正目标，且上轮 merger 亦已在台账中留下同一结论。

---

## 1. 合入前机械核实（原始输出）

```
$ git rev-parse refs/heads/main
b964ae313abb8b83fc9acb02fbc062448c5c23bc
$ git rev-parse merge/mu1a-candidate-r1
ad9ad3c6b532ce0cc9366c8cc34a217f9f466933
$ git merge-base --is-ancestor refs/heads/main merge/mu1a-candidate-r1
ANCESTOR: YES (ff possible)          # exit 0
$ git diff --stat refs/heads/main merge/mu1a-candidate-r1
 29 files changed, 746 insertions(+), 222 deletions(-)
$ git log --oneline refs/heads/main..merge/mu1a-candidate-r1
ad9ad3c merge: 并入 WP2 派生事件字段交付（含 b964ae3 构建修复）
906ec42 merge: 并入 WP1 快照收窄与分页命令交付（含 b964ae3 构建修复）
e5a31dc docs(sync): WP2 修复 F6——把 file.changed 行数口径挂到「有无 Diff 元素」
44bcda8 merge: 并入主分支的 vendor workspace 修复
8265bf3 merge: 并入主分支的 vendor workspace 修复
f64a196 feat(sync): WP2 派生事件字段——file.changed 可选统计与 agent 连接状态封闭枚举
6779c36 feat(sync): WP1 快照资源范围收窄 + session.read 分页合同
$ git rev-parse HEAD ; git rev-parse --abbrev-ref HEAD
b964ae313abb8b83fc9acb02fbc062448c5c23bc
main
$ git status --porcelain
?? .target-wt/
?? .worktrees/
?? openspec/changes/sync-scope-and-pwa-client/
```

逐项核实结论：

| 项 | 期望（main 给定） | 实测 | 一致 |
| --- | --- | --- | --- |
| 目标仓库 | `D:\Project\acp-remote` | `D:/Project/acp-remote` | ✅ |
| `refs/heads/main` | `b964ae313abb8b83fc9acb02fbc062448c5c23bc` | 同 | ✅ |
| 候选 tip | `ad9ad3c6b532ce0cc9366c8cc34a217f9f466933` | 同 | ✅ |
| ancestor 关系 | 成立（可快进） | `exit 0`，YES | ✅ |
| 相对增量 | 29 files, +746, −222 | 同 | ✅ |
| 7 提交列表 | 见上 | 逐一匹配 | ✅ |
| 工作区 | 仅三项预期未跟踪项 | 恰好 `.target-wt/`、`.worktrees/`、`openspec/changes/sync-scope-and-pwa-client/` | ✅ |

**工作区判定**：`git status --porcelain` 无任何已跟踪文件的修改或暂存项（无 `M`/`A`/`D`/`R` 首列标记），仅三条**目录级未跟踪项**，与预期完全一致。另核实无中断合并态（`.git/MERGE_HEAD` 不存在）。**未触发 BLOCKED。**

---

## 2. 实际合入（`git merge --ff-only`）

合入瞬间再次核实两侧 tip 后执行：

```
$ git rev-parse refs/heads/main merge/mu1a-candidate-r1
b964ae313abb8b83fc9acb02fbc062448c5c23bc
ad9ad3c6b532ce0cc9366c8cc34a217f9f466933
$ git merge --ff-only merge/mu1a-candidate-r1
Updating b964ae3..ad9ad3c
Fast-forward
 ... （29 files changed, 746 insertions(+), 222 deletions(-) 逐文件清单）
$ echo $?
0
$ git rev-parse HEAD
ad9ad3c6b532ce0cc9366c8cc34a217f9f466933
$ git rev-parse refs/heads/main
ad9ad3c6b532ce0cc9366c8cc34a217f9f466933
$ git status --porcelain
?? .target-wt/
?? .worktrees/
?? openspec/changes/sync-scope-and-pwa-client/
```

- **实际合入提交 SHA（`git rev-parse HEAD`）= `ad9ad3c6b532ce0cc9366c8cc34a217f9f466933`**，与 `refs/heads/main` 同步前移。
- 纯快进（`Fast-forward`，无合并提交、无冲突、无内容合并）；**未使用**任何非 `--ff-only` 的 merge，未 `reset`/`checkout`/`rebase`/`clean`。
- 合入后工作区仍仅三条预期未跟踪项，跟踪文件零改动。

---

## 3. premerge 门禁复跑（完整 JSON 原始输出）

```
$ npx --quiet --no-install openspec-agentic workflow check --change "sync-scope-and-pwa-client" --stage premerge --json
{
  "result": "FAIL",
  "stage": "premerge",
  "contractDigest": "sha256:e4cc671c11eeac9f64cf259271ca7821a896a9a25fcbbbda4d4d16fb7e896aa5",
  "requirementsDigest": "sha256:c2eb41a848e4e9bd24f4e03550709fe3a44c560574fe45c4bbf040fb34cf2752",
  "targetCommit": "ad9ad3c6b532ce0cc9366c8cc34a217f9f466933",
  "evidence": [],
  "errors": [
    "候选提交必须有待合入的变更",
    "候选报告的目标基线已移动",
    "verify 报告摘要不匹配",
    "review 报告摘要不匹配",
    "Premerge History 的 MU1a-r1 receipt 的 verify 报告摘要不匹配：reports/PV1.log",
    "Premerge History 的 MU1a-r1 receipt 的 review 报告摘要不匹配：reports/review-mu1a-candidate-r1.md",
    "Premerge History 的 MU1a-r1 结果必须是 PASS",
    "Premerge History 的 MU1a-r1 目标提交（b964ae313abb8b83fc9acb02fbc062448c5c23bc）与当前基线不一致"
  ],
  "boundary": "仅检查结构、引用和版本；不证明用户批准、角色独立性或测试真实性。",
  "planningDigest": "plan-v2:sha256:4e2fb347b974d0e8b6d347afec7cbd50591524f79ae7b33cf0d98ced853c88e1",
  "candidateCommit": "ad9ad3c6b532ce0cc9366c8cc34a217f9f466933"
}
$ echo $?
1
```

`candidateCommit = ad9ad3c…`（与候选一致）；但 `result = FAIL`，**非 PASS**。按调度方指令：**未做任何「迎合门禁」的补救**——未改 `verification.md`、未改 receipt 内容、未重跑掩盖。由 main 处置。

### 3.1 门禁失败根因（机械取证的）

**类别 A — 快进后两条错误恒真（结构性，任何配置下都不可消除）**

门禁 `inspectPremerge`（`workflow-check.mjs:1486-1496`）以 `target = git rev-parse refs/heads/main`、`candidate = git rev-parse HEAD`，并断言：

- `if (candidate === target) fail('候选提交必须有待合入的变更')`；
- `if (receipt.target_commit !== target) fail('候选报告的目标基线已移动')`。

一旦 `git merge --ff-only` 把 `refs/heads/main` 推进到候选，**`refs/heads/main == HEAD == ad9ad3c`**，前者必真、后者因 receipt 记录的是合入前基线 `b964ae3` 而必真。这是**快进式合入固有的**：该门禁本是「合入前 CI 预检」（在候选 worktree、`refs/heads/main` 仍指基线时运行才会 PASS）。合入后复跑它，这两条无法 PASS。

> 依据：`node_modules/@dongfanglin/openspec-agentic/assets/openspec/schemas/agentic/procedures/workflow-check.md` 明言「`premerge` 在候选提交工作区执行，要求 `HEAD` 为候选提交、**本地 `target_ref` 仍指向规划基线**」。本次合入后 `target_ref` 已因合入本身前移，前提不再成立。

**类别 B — receipt 的 `evidence.sha256` 缺 `sha256:` 前缀（真实缺陷）**

`workflow-check.mjs:899` 判定 `if (digest(await fs.readFile(evidenceFile)) !== evidence.sha256) fail(...)`，而 `digest()` 返回 `sha256:<hex>`（`workflow-contract.mjs:6`）。receipt-mu1a.md（与 verification.md 内嵌块）现写：

```yaml
verify:
    sha256: fa308ab101ca3b745cf3a085baf0d6ae173d659c9b200089cae25987adf0de01   # 裸 hex，缺前缀
review:
    sha256: 451d4c9a7153cd3dd587a8de3cc2b7965f72671816a9463474560a2462aeefce   # 裸 hex，缺前缀
```

独立复算（直接调用扩展自身函数）证明**文件内容本身正确、仅前缀缺失**：

| evidence | receipt 存储 | 门禁计算 `digest()` | `存储 == 计算` | `"sha256:"+存储 == 计算` |
| --- | --- | --- | --- | --- |
| `reports/PV1.log` | `fa308ab1…f0de01` | `sha256:fa308ab1…f0de01` | ❌ | **✅** |
| `reports/review-mu1a-candidate-r1.md` | `451d4c9a…aeefce` | `sha256:451d4c9a…aeefce` | ❌ | **✅** |

即磁盘摘要与 receipt 记的十六进制**逐字符相同**，仅缺 `sha256:` 前缀 → 4 条「摘要不匹配」（当前块 verify/review 各 1、历史行 receipt verify/review 各 1）。`Premerge History 的 MU1a-r1 结果必须是 PASS` 与「目标提交与当前基线不一致」两条，是历史行 `MU1a-r1` 因 receipt 摘要不匹配被判非 PASS / 与合入后基线 `ad9ad3c` 不再一致所致的**联动**。

**权威格式依据**（该仓库两个已归档、曾 PASS 的 receipt 与模板一致）：

- 模板 `procedures/workflow-check.md:25,31`：`evidence: {path: reports/candidate-verify.log, sha256: "sha256:<报告摘要>"}`；
- `openspec/changes/archive/2026-09-25-daemon-cli-and-local-admin/verification.md:349-360` 与 `…/2026-09-25-agent-host-spawn-failure-coverage/verification.md:88-99`：均为 `sha256: "sha256:<hex>"`。

**修复方向（归 main）**：把两份同源 `agentic-premerge` 块（`reports/receipt-mu1a.md` 与 `verification.md` 内嵌块）的 `verify.evidence.sha256` / `review.evidence.sha256` 由 `<hex>` 改为 `"sha256:<hex>"` 即可消掉类别 B 的 6 条错误。**类别 A 的 2 条在本轮快进后无法消除**；`candidateCommit` 已随合入前移，receipt 的 `target_commit=b964ae3` 同理不再等于 `refs/heads/main`。由 main 决定登记口径（本报告不自改 receipt，不触碰 verification.md）。

> 复核说明：本轮**未**重跑候选 PV1（不在本轮范围；候选 PV1 已由 `reports/PV1.log`（sha256 经复核相符）作证）。本轮的回归是合入后主分支实跑，见 §4。

---

## 4. 主分支回归 [PV1]

命令（在主检出、合入后的真实主分支上执行）：

```
CARGO_TARGET_DIR=D:/Project/acp-remote/.target-wt/mu1a-main npm run verify
```

- **cwd** = `D:\Project\acp-remote`；**HEAD** = `ad9ad3c6b532ce0cc9366c8cc34a217f9f466933`。
- **完整输出**：`openspec/changes/sync-scope-and-pwa-client/reports/PV1-main-mu1a.log`（**2218 行**，sha256 `994450cae404480a7ee7e81e7843bdf68d041f7921814231762e6e984e4112d3`）。
- **结果：PASS，exit 0**（日志末行 `VERIFY_EXIT=0`）。

### 4.1 `check` 十道脚本（全绿）

| # | 脚本 | 关键输出（日志行） |
| --- | --- | --- |
| 1 | `check:schemas` | `schema fixtures OK: 133 valid, 35 invalid (ajv Draft 2020-12), 39 event views bound`（L18） |
| 2 | `check:commands` | `command catalog OK: 13 commands`（L23） |
| 3 | `check:errors` | `error registry OK: 58 codes across 2 protocols`（L28） |
| 4 | `check:features` | `feature registry OK: 13 feature ids across 2 protocols`（L33） |
| 5 | `check:assets` | `contract assets OK: 17 schemas, 181 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed`（L38） |
| 6 | `check:acp` | `ACP compatibility matrix OK: 25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows, schema validated by ajv)`（L43） |
| 7 | `check:docs` | `doc links OK: 2069 relative links, 45577 section refs across 2628 markdown files`（L48） |
| 8 | `check:boundaries` | `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致`（L54） |
| 9 | `check:drift` | `contract drift OK: §7 的 36 条 DDL … 一致；§5 的 15 个 trait / 96 个方法签名 … 一致`（L59） |
| 10 | `check:agentic` | `doctor` 8 项 PASS（toolchain/openspec/config/schema/schema validation/verification skill/AGENTS.md/manifest，L64-72）+ `openspec validate --all --strict`；`Totals: 22 passed, 0 failed (22 items)`（L110） |

### 4.2 `check:rust` 三条（全绿）

日志 L114：`cargo fmt --all -- --check && cargo clippy --locked --workspace --all-targets --all-features -- -D warnings && cargo test --locked --workspace --all-features`

- `cargo fmt --all -- --check`：**无输出**（无格式差异），通过。
- `cargo clippy … -- -D warnings`：编译 `Finished` 无 warning/error，`-D warnings` 通过。
- `cargo test --locked --workspace --all-features`：全部测试二进制 `0 failed`（含 lib、各集成测试与 doc-tests），末尾 `VERIFY_EXIT=0`。

### 4.3 已知 flake

`crates/app/tests/daemon_lifecycle.rs` 的已知非确定性 flake **本次未命中**（回归一次通过，无需重跑取证）。故本报告不附加 flake 重跑记录。

---

## 5. 合并相对候选的差异（[6.8] 机械输入）

```
$ git diff --stat merge/mu1a-candidate-r1 HEAD
（空输出）
$ git rev-parse HEAD^{tree} merge/mu1a-candidate-r1^{tree}
6b206e6717449bcad64b00d1d674db44ba6768aa
6b206e6717449bcad64b00d1d674db44ba6768aa
```

**结论：本单元无新增差异。** 依据：(a) 合入为纯 `--ff-only` 快进，未产生合并提交，故 `HEAD` 即候选提交本身（`ad9ad3c6…`）；(b) `merge/mu1a-candidate-r1` 与 `HEAD` 的 `git diff --stat` 为空；(c) 两侧树 SHA **同一**（`6b206e6717449bcad64b00d1d674db44ba6768aa`）。

**供 main 登记 6.8 复用**：由于合并结果与候选树逐字节相同、无任何新增/修改路径，6.8 的合并后检视可**复用原候选 review ID `review-mu1a-candidate-r1`（Round 1，PASS，`reports/review-mu1a-candidate-r1.md`）**，无需新开检视线程；如需独立复核，仍由 main 另派 reviewer（**待补**）。

---

## 6. 资源与边界

- **新增资源**：`CARGO_TARGET_DIR=D:/Project/acp-remote/.target-wt/mu1a-main`（仅本次主分支回归，独占）。保留供 main 复核，**随时可删**。
- **未创建/未删除**：未新建或删除任何分支、worktree；**未触碰** `.worktrees/mu1-merge`、`.worktrees/wp1`、`.worktrees/wp2`、`.worktrees/tp1` 或 `merge/mu1a-candidate`、`merge/mu1a-candidate-r1`。
- **主检出写入范围**：仅 `reports/merge-mu1a-r2.md`（本报告）与 `reports/PV1-main-mu1a.log`（tasks 6.7 指定证据），均在未跟踪的变更目录内；跟踪文件零改动。
- **未修改**：`verification.md`、`plan.md`、`tasks.md`、`receipt-mu1a.md`、任何产品代码/schema/fixture。
- **未执行/未声称**：`deps` / `advisories` / `secrets` 三个 CI-only job 本地无等价物，**未执行、未声称通过**；未推送、未发布、未归档、未回滚（本变更未授权远端交付，走 `AGENTS.md` §8 的 PR 路径）。

## 7. 待补与阻断

| 项 | 状态 | 说明 |
| --- | --- | --- |
| PREMERGE-GATE-6.6 | **FAIL（未解决）** | 8 条 errors；类别 A（快进后 target==candidate，2 条）本轮不可消除；类别 B（receipt evidence 缺 `sha256:` 前缀，6 条）可由 main 一处修正消解。归 main 处置。 |
| 6.8 独立 review | **待补** | 合并相对候选零差异，建议复用 `review-mu1a-candidate-r1`；若需独立轮次由 main 派发。 |
| DISPATCH-ACK | BLOCKED（结构不可满足） | 见 §0；未越权改台账。 |

**未做补救**：未改 `verification.md`、未改 receipt 内容以迎合门禁、未重开工作包台账。全部原始输出已如实留证。
