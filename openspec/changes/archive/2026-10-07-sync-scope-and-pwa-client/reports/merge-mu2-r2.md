---
task_id: "6.16"
role: merger
phase: merge
delivery_unit: MU2
result: BLOCKED
agent_context:
  agent_id: MergerMu2Final
  executor: merger-mu2-2
target_ref: refs/heads/main
target_revision: 5ba44fd35be2cd3717006c0995a8404c3315f388
baseline_revision: 33040d78324ff51be49219fcb3aac054d0100cc9
candidate_ref: integ/mu2-wiring
handoff_index:
  - task_id: "6.16"
    work_package: MU2
    role_phase_stage: merger / merge / main
    round: 2
    executor_agent: merger-mu2-2
    target_revision: 5ba44fd35be2cd3717006c0995a8404c3315f388
    evidence_type_id: "BLOCK-PREMERGE-6.16"
    report_path: reports/merge-mu2-r2.md
    result_evidence_status: "BLOCKED / NEW（合入前门禁 FAIL，未执行快进；未改 verification.md 与 receipt）"
    applicability_source_evidence: "premerge 门禁 23 条 errors（原文见本报告 §2）；第 1 步机械核实全部通过，候选内容凭据 SHA-256 逐字复算相符"
---

# MU2 合入（第 2 轮，merger-mu2-2）

**结论：`BLOCKED`。合入前门禁（`--stage premerge`）结果为 `FAIL`（exit 1，23 条 errors）。按派发硬约束，未执行 `git merge`，未修改 `verification.md`，未修改 receipt。目标引用与候选分支均保持原状。**

- 目标仓库：`D:\Project\acp-remote`
- 目标引用：`refs/heads/main`
- 合入前基线：`33040d78324ff51be49219fcb3aac054d0100cc9`
- 候选引用：`integ/mu2-wiring` = `5ba44fd35be2cd3717006c0995a8404c3315f388`
- 权威登记：`openspec/changes/sync-scope-and-pwa-client/verification.md`

---

## 0. 派发确认（`dispatch --ack`）

```
$ cd /d/Project/acp-remote
$ npx --quiet --no-install openspec-agentic dispatch --change "sync-scope-and-pwa-client" --wp WP3 --executor merger-mu2-2 --ack
openspec-agentic: 接收确认必须绑定当前活跃工作包及其实际执行者
EXIT=1
```

**无法 ack，原因（非本单元可修复）：** 台账 `dispatch-queue.jsonl` 中 **WP3 最新状态为 `fixing`**，认领执行者为 `coder-w3-r3`（attempt 3，2026-10-04T19:30:50.799Z），而非 `ready-to-merge`/merger。MU2 是**交付单元**，台账不支持交付单元粒度（MU1a 曾被记为 `superseded` 并注明"转由 verification 的 Premerge History 登记"）。按派发指令，**未为此改动台账状态**。

当前台账快照（最后一次状态汇总）：

| WP | 最新状态 | 执行者 |
| --- | --- | --- |
| WP1 | merged | review-w1-r1 |
| WP2 | merged | review-w2-r2 |
| TP1 | ready-to-merge | review-tp1-r1 |
| **WP3** | **fixing** | **coder-w3-r3** |
| WP4 | ready-to-merge | review-w4-r2 |
| TP2 | ready-to-merge | review-tp2-r1 |

---

## 1. 合入前机械核实（原始输出）

```
$ cd /d/Project/acp-remote
$ git status --porcelain
?? .target-wt/
?? .worktrees/
?? openspec/changes/sync-scope-and-pwa-client/

$ git rev-parse refs/heads/main
33040d78324ff51be49219fcb3aac054d0100cc9

$ git rev-parse integ/mu2-wiring
5ba44fd35be2cd3717006c0995a8404c3315f388

$ git merge-base --is-ancestor refs/heads/main integ/mu2-wiring
ANCESTOR_EXIT=0        # 成立 → 可快进
```

`status --porcelain` **恰好只输出三个未跟踪项**（`.target-wt/`、`.worktrees/`、`openspec/changes/`），**无任何已跟踪文件的修改或暂存项**。主检出干净。

**冻结合同零 diff（复核通过）：**

```
$ git diff --stat refs/heads/main..integ/mu2-wiring -- crates/sync-protocol/ schemas/ fixtures/ vendor/ crates/core/Cargo.toml crates/agent-host/Cargo.toml
（空输出）
FROZEN_EXIT=0

$ git diff --stat refs/heads/main..integ/mu2-wiring | tail -1
26 files changed, 6788 insertions(+), 44 deletions(-)
```

| 面 | 结果 |
| --- | --- |
| `crates/sync-protocol/` | 零改动 ✓ |
| `schemas/` | 零改动 ✓ |
| `fixtures/` | 零改动 ✓ |
| `vendor/` | 零改动 ✓ |
| `crates/core/Cargo.toml` | 零改动 ✓ |
| `crates/agent-host/Cargo.toml` | 零改动 ✓ |
| 增量总量 | 26 files / +6788 / −44 ✓（与派发事实一致） |

**候选工作区核实：**

```
$ git -C .worktrees/integ rev-parse HEAD
5ba44fd35be2cd3717006c0995a8404c3315f388
$ git -C .worktrees/integ status --porcelain
（空）
```

**步骤 1 判定：PASS。** 目标明确（`refs/heads/main`）、基线可机械核实、候选为其后代且可快进、冻结合同与派发事实逐项吻合。

---

## 2. premerge 门禁（合入前，exit 1 → FAIL）

在**候选工作区**内运行、`--planning-root` 指向主检出（在主检出内运行会因 target==HEAD 恒报"候选提交必须有待合入的变更"，不可用）：

```
$ cd /d/Project/acp-remote/.worktrees/integ
$ npx --quiet --no-install openspec-agentic workflow check \
    --change "sync-scope-and-pwa-client" --stage premerge \
    --planning-root D:/Project/acp-remote --json
EXIT=1
```

**确定性复核**：同一命令连跑两次，stdout 的 md5 均为 `ab0decf962ac73e9ebf5868501d91664`——非瞬时/干扰态，FAIL 稳定可复现。

### 完整 JSON 输出（原文）

```json
{
  "result": "FAIL",
  "stage": "premerge",
  "contractDigest": "sha256:e4cc671c11eeac9f64cf259271ca7821a896a9a25fcbbbda4d4d16fb7e896aa5",
  "requirementsDigest": "sha256:c2eb41a848e4e9bd24f4e03550709fe3a44c560574fe45c4bbf040fb34cf2752",
  "targetCommit": "33040d78324ff51be49219fcb3aac054d0100cc9",
  "evidence": [
    {
      "path": "reports/PV1.log",
      "sha256": "sha256:8b218a4a33a76b07d654a76f1cc8b05c7e4951adb6bbd30b883681c5dc0c2abb"
    },
    {
      "path": "reports/review-mu2-candidate-r1.md",
      "sha256": "sha256:6a55ead436c4d799437a2538cece2aeb2381da18287791de0b26faed263885e8"
    }
  ],
  "errors": [
    "工作包 TP1 的 State（ready-to-merge）与台账（merged）不一致",
    "当前单元或 code 上游 TP2 的 Contract Freeze 未冻结或不可读取",
    "当前单元或 code 上游 WP4 的 Contract Freeze 未冻结或不可读取",
    "当前单元或 code 上游 WP3 的 Contract Freeze 未冻结或不可读取",
    "工作包 WP3 缺少独立 review 记录（当前单元或必要依赖）",
    "工作包 WP4 缺少独立 review 记录（当前单元或必要依赖）",
    "工作包 TP2 缺少独立 review 记录（当前单元或必要依赖）",
    "Worktree Handoff 的 WP3 第 1 轮 Received At（2026-10-04T12:05:00Z）晚于该轮首次执行事件（coding，2026-10-04T03:31:33.681Z）：执行者未在开工前接收 worktree",
    "Worktree Handoff 未覆盖已开工工作包 WP3 第 2 轮 的尝试",
    "Worktree Handoff 未覆盖已开工工作包 WP3 第 3 轮 的尝试",
    "Worktree Handoff 的 WP4 第 1 轮 Received At（2026-10-04T12:05:00Z）晚于该轮首次执行事件（coding，2026-10-04T03:31:38.819Z）：执行者未在开工前接收 worktree",
    "Worktree Handoff 未覆盖已开工工作包 WP4 第 2 轮 的尝试",
    "Worktree Handoff 未覆盖已开工工作包 TP2 第 1 轮 的尝试",
    "TP2 缺少测试编写阶段的 DELIVERY PASS，设计文档不能完成测试包",
    "Premerge History 的 MU2-r1 结果必须是 PASS",
    "Premerge History 的 MU2-r1 尚未给出工作包 WP3 的独立 review 记录",
    "Premerge History 的 MU2-r1 的工作包 WP3 台账状态必须是 ready-to-merge 或 merged（当前：fixing）",
    "Premerge History 的 MU2-r1 的工作包 WP3 缺少绑定该工作包且属于本轮认领执行者 coder-w3-r3 的 Handoff Index DELIVERY PASS 行",
    "Premerge History 的 MU2-r1 尚未给出工作包 WP4 的独立 review 记录",
    "Premerge History 的 MU2-r1 的工作包 WP4 缺少绑定该工作包且属于本轮认领执行者 coder-w4-r2 的 Handoff Index DELIVERY PASS 行",
    "Premerge History 的 MU2-r1 尚未给出工作包 TP2 的独立 review 记录",
    "Premerge History 的 MU2-r1 的工作包 TP2 缺少绑定该工作包且属于本轮认领执行者 testing-2-r1 的 Handoff Index DELIVERY PASS 行",
    "同一批次同时有 coder 与 tester 已开工（W2（coder：WP3、WP4 ∥ tester：TP1）），但没有任何跨角色并行窗口：契约明确时编码与测试设计必须并行（开工/交付时间由 openspec-agentic dispatch 写入）"
  ],
  "boundary": "仅检查结构、引用和版本；不证明用户批准、角色独立性或测试真实性。",
  "planningDigest": "plan-v2:sha256:4e2fb347b974d0e8b6d347afec7cbd50591524f79ae7b33cf0d98ced853c88e1",
  "candidateCommit": "5ba44fd35be2cd3717006c0995a8404c3315f388"
}
```

### 2.1 关键归因：**门禁 FAIL 不是候选内容缺陷，也不是凭据损坏**

receipt（`reports/receipt-mu2.md` 与 `verification.md` 内联同名块的 `evidence`）两侧 SHA-256 由本 merger **独立复算逐字相符**：

```
$ cd .../reports && sha256sum PV1.log review-mu2-candidate-r1.md
sha256:8b218a4a33a76b07d654a76f1cc8b05c7e4951adb6bbd30b883681c5dc0c2abb  PV1.log
sha256:6a55ead436c4d799437a2538cece2aeb2381da18287791de0b26faed263885e8  review-mu2-candidate-r1.md
```

门禁 JSON 自身的 `evidence[].sha256` 与之**逐字相等**，且门禁**未**报任何 `receipt 的 … 报告摘要不匹配` / `… 不可读取` / `reviewer 与 author` / `candidate_commit 与该行不一致` / `目标提交不是候选提交的祖先` 类错误。故：**候选 PV1 与独立检视 PASS 的结论未被本单元推翻**；失败的是一组**流程/登记门禁项**。

### 2.2 23 条 errors 分组（按 `workflow-check.mjs` 触发点）

**(A) 交付单元的独立 review 未登记进 `## Review Findings`（6 条）** — 源检查 `workflow-check.mjs:517-518`
`verification.md` 的 `## Review Findings` 表**最大 Work Package 为 TP1 / WP2**，**完全没有 WP3、WP4、TP2 的行**（现存 9 行：WP1×2、WP2×3、WP1、WP2、TP1×2）。虽有实体报告 `reports/review-w3-r1.md`/`review-w3-r2.md`/`review-w4-r1.md`/`review-w4-r2.md`/`review-tp2-r1.md`，但登记表未收录 → WP3/WP4/TP2 均报"缺少独立 review 记录"。
→ errors #5、#6、#7、#16、#19、#21。

**(B) 本轮 receipt 的 Work Package 集合与计划不符（1 条）** — 源检查 `workflow-check.mjs:910-918`
receipt `delivery_unit: MU2`。计划 `## Merge Strategy` 定义 **MU2 = WP3 + WP4 + TP2**（Order 3，与派发一致）。`## Premerge History` 行的 Work Packages 列亦为 `WP3, WP4, TP2`。但门禁解析出的 `unitWps` 集合未命中这四包，致 `Premerge History 的 MU2-r1 结果必须是 PASS` 与派生项全数触发。
→ errors #15、#16–#22（含 (C)、(D) 的派生检查）。

**(C) Handoff Index 缺 DELIVERY PASS 行（3 条）** — 源检查 `workflow-check.mjs:831-837`
对每包按"本轮认领执行者"核对交付证据行：WP3 需绑定 `coder-w3-r3`、WP4 需 `coder-w4-r2`、TP2 需 `testing-2-r1`。`## Handoff Index` 中 **无任何 WP3 / WP4 / TP2 的 DELIVERY 行**（现存 DELIVERY 行至 `deliver-wp2-r2`、TP1 等）。
→ errors #18、#20、#22。

**(D) Contract Freeze 不可读（3 条）** — 源检查 `workflow-check.mjs:536-538、premergeReviewScope`
WP3/WP4/TP2 均含 `contract:*` 依赖，检查要求其 `Contract Freeze` 列路径可读：
- WP3：`schemas/sync/v1/event-views.schema.json@WP2`、`schemas/sync/v1/command.schema.json@WP1`
- WP4：`schemas/sync/v1/event-views.schema.json@WP2`
- TP2：`crates/core/src/ports.rs@WP3`；`crates/sync-protocol/src/views.rs@WP2`（形状基准）
→ errors #2、#3、#4。

**(E) Worktree Handoff 与台账时序/覆盖不符（5 条）** — 源检查 `workflow-check.mjs:724、762`
- WP3/WP4 第 1 轮 `Received At`（`2026-10-04T12:05:00Z`）**晚于**台账首次 `coding`（WP3 `03:31:33.681Z` / WP4 `03:31:38.819Z`）约 8.5 小时；
- WP3 第 2、3 轮、WP4 第 2 轮、TP2 第 1 轮在台账中已开工，但 `## Worktree Handoff` 表（仅 6 行，至 WP4 第 1 轮）**无对应尝试行**。
→ errors #8–#12。

**(F) TP2 测试包前置缺失（1 条）** — 源检查 `workflow-check.mjs` Design-Author 分支（`Test Authoring Protocol: staged-v2`）
「TP2 缺少测试编写阶段的 DELIVERY PASS，设计文档不能完成测试包」。TP2 已有 `reports/test-tp2-r1.md` 与 `reports/review-tp2-r1.md`，但其 design-author DELIVERY 行未按门禁要求的绑定形态登记。
→ error #13。

**(G) 跨角色并行窗口缺失（1 条）** — 源检查：
「同一批次同时有 coder 与 tester 已开工（W2：coder WP3、WP4 ∥ tester TP1），但没有任何跨角色并行窗口」。
→ error #23。

**(H) TP1 State 不一致（1 条）**
「工作包 TP1 的 State（ready-to-merge）与台账（merged）不一致」——`## Dispatch Reconciliation` 记 TP1 = `ready-to-merge`，而 `dispatch-queue.jsonl` 最新为 `merged`（2026-10-04T03:30:06.225Z）。
→ error #1。

**小结：(A)–(H) 全部为 `verification.md`/`dispatch-queue.jsonl` 的登记·台账·时序一致性问题，无一条指向候选 26 文件内容本身。** 候选内容凭据（PV1、独立检视）完整可信，但"合入前门禁 PASS"这一**合入前置条件未满足**。

---

## 3. 快进主分支

**未执行。** 依据派发指令「若非 PASS：不合入」。

```
$ git -C D:/Project/acp-remote rev-parse refs/heads/main
33040d78324ff51be49219fcb3aac054d0100cc9     # 仍停在合入前基线，未移动
```

合入提交 SHA：**不存在**（未产生任何合并/快进提交）。

---

## 4. 主分支回归

**未执行**（步骤 3 的前置条件不成立）。

| # | 命令 | 状态 |
| --- | --- | --- |
| 4.1 | `npm run verify`（cwd 主检出，`CARGO_TARGET_DIR=.target-wt/mu2-main`） | 未执行 |
| 4.2 | `cargo test --locked -p core -p storage-sqlite -p agent-host --all-features` | 未执行 |
| 4.3 | `cargo test --locked -p app --test node_link_e2e` | 未执行 |

`reports/PV1-main-mu2.log`：**未创建**（无输出可写入；在当前 BLOCKED 状态下写入空/伪造日志会违反凭据要求）。

---

## 5. 合并相对候选的新增差异

**N/A。** 未发生合入，故不存在"合并后 vs 候选"的差异。可核实的等价事实：

```
$ git -C D:/Project/acp-remote rev-parse refs/heads/main   # 33040d78…（基线未动）
$ git -C D:/Project/acp-remote rev-parse integ/mu2-wiring  # 5ba44fd3…（候选未动）
```

两者均与步骤 1 快照逐字一致，**候选分支与全部 worktree 均未被删除或改动**（`git worktree list` 8 项完好）。

---

## 6. 合入条件核对

| 条件 | 状态 |
| --- | --- |
| 候选建立在当前最新主分支上 | **满足**（`merge-base --is-ancestor` exit 0） |
| 冻结合同零 diff | **满足**（6 个面全空） |
| premerge 门禁 PASS | **不满足 → FAIL（23 errors）** |
| `npm run verify` 全绿 | 未执行（前置不满足） |
| `node_link_e2e` 六项断言全绿 | 未执行（前置不满足） |
| `deps`/`advisories`/`secrets` | **本地无等价物，未执行、未声称通过** |
| 推送授权 | **无（本变更未授权推送，未推送）** |

**未满足合入条件，故未合入。**

---

## 7. 资源释放与副作用声明

- **未执行任何** `git merge` / `checkout` / `reset` / `branch -d` / `worktree remove`。
- 主检出 `D:\Project\acp-remote`：`status --porcelain` 仍**仅三个预期未跟踪项**，无暂存/修改残留，**未卡在合并态**。
- 分支：`integ/mu2-wiring`、`merge/mu1a-candidate-r1`、`merge/mu1-candidate`、`feat/*` 全部保留。
- Worktree：8 个（主检出 + `integ`/`mu1-merge`/`tp1`/`tp2`/`wp1`/`wp2`/`wp3`/`wp4`）全部保留。
- 磁盘：本单元**未运行任何 Cargo/npm 构建**，未新建 `CARGO_TARGET_DIR`，无缓存累积（复用既有 `.target-wt/`，未扩大占用）。
- **未修改** `verification.md`（含其内联 `agentic-premerge` 块）、未修改 `reports/receipt-mu2.md`、未修改 `dispatch-queue.jsonl`、未创建 `reports/PV1-main-mu2.log`。
- 本单元未声称执行任何本地无等价物的检查。

---

## 8. 处置建议（交 main）

`refs/heads/main` 不得推进，直至以下登记缺陷清零（均在**主检出**侧，属 main 的登记职责，非候选代码缺陷）：

1. `## Review Findings` 补 WP3（`review-w3-r2` PASS）、WP4（`review-w4-r2` PASS）、TP2（`review-tp2-r1` PASS）三行。
2. `## Handoff Index` 补 WP3/WP4/TP2 的 DELIVERY PASS 行，执行者分别绑定 `coder-w3-r3` / `coder-w4-r2` / `testing-2-r1`，`Target Revision` = `5ba44fd3…`。
3. `## Worktree Handoff` 补 WP3 第 2/3 轮、WP4 第 2 轮、TP2 第 1 轮行，并修正 WP3/WP4 第 1 轮 `Received At`（不得晚于台账 `coding` 事件时刻）。
4. 澄清 receipt `delivery_unit: MU2` 的 WP 集合解析（WP3+WP4+TP2）与 `test-tp2-r1` 的 design-author DELIVERY 绑定形态。
5. 修 `## Dispatch Reconciliation` 中 TP1 的 State（`ready-to-merge` → `merged`）。
6. 台账中 **WP3 停在 `fixing`（`coder-w3-r3`）**：需推进为 `ready-to-merge`/`merged` 后方可再度尝试合入；这也是本单元 `dispatch --ack` 失败的直接原因。
7. 跨角色并行窗口缺失（error #23）：属里程碑历史时序，须由 main 判定是否按已知偏离登记而非强行改时序。

清零后以**新的独立 merger**重跑 `--stage premerge`；PASS 后 `git merge --ff-only integ/mu2-wiring`，再执行三步主分支回归。
