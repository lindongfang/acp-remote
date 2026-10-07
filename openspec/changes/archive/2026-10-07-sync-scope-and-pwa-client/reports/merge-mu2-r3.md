---
task_id: "6.16"
role: merger
phase: merge
delivery_unit: MU2
result: PASS
agent_context:
  agent_id: merger-mu2-3
  executor: merger-mu2-3
target_ref: refs/heads/main
target_revision: 5ba44fd35be2cd3717006c0995a8404c3315f388
baseline_revision: 33040d78324ff51be49219fcb3aac054d0100cc9
candidate_ref: integ/mu2-wiring
merged_commit: 5ba44fd35be2cd3717006c0995a8404c3315f388
merge_type: fast-forward
handoff_index:
  - task_id: "6.16"
    work_package: MU2
    role_phase_stage: merger / merge / main
    round: 3
    executor_agent: merger-mu2-3
    target_revision: 5ba44fd35be2cd3717006c0995a8404c3315f388
    evidence_type_id: "MERGE-MU2-r3"
    report_path: reports/merge-mu2-r3.md
    result_evidence_status: "PASS / NEW（--ff-only 快进 33040d7 → 5ba44fd；主分支三道回归全绿；premerge 门禁仅剩 1 条已裁决已知偏离）"
    applicability_source_evidence: "reports/PV1-main-mu2.log（sha256:938ba23e…ab1f3，2369 行，exit 0）、reports/node-link-e2e-main-mu2.log（sha256:818c6035…7869，6 passed）、premerge 门禁完整 JSON（见 §2）"
---

# MU2 合入（第 3 轮，merger-mu2-3）

**结论：`PASS`。`refs/heads/main` 已由 `33040d78324ff51be49219fcb3aac054d0100cc9` 以 `--ff-only` 快进到 `5ba44fd35be2cd3717006c0995a8404c3315f388`。三道主分支回归全部 exit 0。合入前 premerge 门禁只剩 1 条 error，且为用户 2026-10-04 明确裁决接受的已知偏离。**

- 目标仓库：`D:\Project\acp-remote`
- 目标引用：`refs/heads/main`
- 合入前基线：`33040d78324ff51be49219fcb3aac054d0100cc9`
- 候选引用：`integ/mu2-wiring` = `5ba44fd35be2cd3717006c0995a8404c3315f388`
- 实际合入提交：`5ba44fd35be2cd3717006c0995a8404c3315f388`（纯快进，未产生合并提交）
- 权威登记：`openspec/changes/sync-scope-and-pwa-client/verification.md`（**本单元未改动**）

---

## 0. 派发确认（`dispatch --ack`）与台账状态

```
$ cd /d/Project/acp-remote
$ npx --quiet --no-install openspec-agentic dispatch --change "sync-scope-and-pwa-client" --wp WP3 --executor merger-mu2-3 --ack
openspec-agentic: 接收确认必须绑定当前活跃工作包及其实际执行者
EXIT=1
```

**无法 ack，按派发指令说明、未改动台账状态。** 复跑不带 `--ack` 的派发给出确切原因：

```
$ npx --quiet --no-install openspec-agentic dispatch --change "sync-scope-and-pwa-client" --wp WP3 --executor merger-mu2-3
openspec-agentic: 错误: 工作包 WP3 当前状态为 ready-to-merge，已有主人，不得重复派发
EXIT=1
```

即 WP3 已处于 `ready-to-merge`（主人 `review-w3-r2`），merger 不是 WP3 的认领执行者——这正是本轮派发指令预见的「因 WP3 已 ready-to-merge 而无法 ack」情形。**台账状态未改。**

台账快照（`openspec-agentic workflow status`）：

| WP | 最新状态 | 执行者 |
| --- | --- | --- |
| WP1 | merged | review-w1-r1 |
| WP2 | merged | review-w2-r2 |
| WP3 | ready-to-merge | review-w3-r2 |
| WP4 | ready-to-merge | review-w4-r2 |
| TP1 | merged | review-tp1-r1 |
| TP2 | ready-to-merge | review-tp2-r1 |

单元 MU2 `[WP3, WP4, TP2]` 三包均可合入（`ready-to-merge`/`merged`），与前两轮 merger 见到的「WP3 停在 `fixing`」不同——登记面缺口已由 main 补齐。

---

## 1. 合入前机械核实（原始输出）

```
$ cd /d/Project/acp-remote
$ git status --porcelain
?? .target-wt/
?? .worktrees/
?? openspec/changes/sync-scope-and-pwa-client/
?? verification.md            # ← 见 §1.1：仓库根的游离未跟踪文件

$ git merge-base --is-ancestor refs/heads/main integ/mu2-wiring
ancestor_exit=0               # 成立 → 可快进

$ git rev-parse integ/mu2-wiring
5ba44fd35be2cd3717006c0995a8404c3315f388

$ git rev-parse refs/heads/main
33040d78324ff51be49219fcb3aac054d0100cc9

$ git rev-parse HEAD
33040d78324ff51be49219fcb3aac054d0100cc9

$ git rev-parse --abbrev-ref HEAD
main
```

**判定**：目标明确（`refs/heads/main` 检出）、基线可机械核实、候选为其后代且可快进。**无任何已跟踪文件的修改或暂存项**——因此未触发「已跟踪文件被改动 → BLOCKED」的硬条件。工作区唯一的意外项是仓库根多出一个**未跟踪**的 `verification.md`，见下。

### 1.1 仓库根游离 `verification.md`（未跟踪）—— 机械核实的意外项

`status --porcelain` 预期「只输出三个未跟踪项」（`.target-wt/`、`.worktrees/`、`openspec/changes/`），实际多出第 4 个 **`?? verification.md`**（仓库根，112762 bytes，mtime 2026-10-05 06:18）。

核实结论：**它是游离的未跟踪副本，不是本变更的门禁台账。**

| 核实 | 结果 |
| --- | --- |
| 是否被 git 跟踪 | **否**。`git log --all -- verification.md` 空；`git cat-file -e HEAD:verification.md` → `fatal: path ... not in 'HEAD'` |
| 与门禁实际读取的文件是否同一 | **否**。门禁（`workflow-check.mjs:142/1487/1537`）读的是 `changeRoot/verification.md`（即 `openspec/changes/sync-scope-and-pwa-client/verification.md`）。该文件与门禁 JSON 的 `contractDigest=sha256:ecfc245c…e0e91` 对应；而根游离文件的 `contract_digest=sha256:e4cc671c…`（F13 修正**之前**的旧值），`planning_digest` 亦为旧值 → 根文件是过期快照 |
| main 当前维护的是哪个 | 变更目录内那份（mtime 08:07，含本轮新登记的「已知偏离登记：W2 跨角色并行窗口」节）。根文件 06:18，更早 |
| 差异 | `diff` 仅 56 行：根文件缺 Round 13 DDR 行、缺「Round 13 三项 MINOR」、缺「已知偏离登记」节，且 digest 为旧值 |

**它不是本变更交付面的一部分**（未跟踪、任何提交中都不存在），但它是**主分支回归第 1 道的确定性致因**，见 §4.1。

### 1.2 冻结合同与依赖面零 diff（复核通过）

```
$ git diff --stat refs/heads/main integ/mu2-wiring -- crates/sync-protocol/ schemas/ fixtures/ vendor/ Cargo.toml crates/core/Cargo.toml crates/agent-host/Cargo.toml
（空输出）

$ git diff --name-only refs/heads/main integ/mu2-wiring | grep -i cargo
（空输出）

$ git diff --stat refs/heads/main integ/mu2-wiring | tail -1
26 files changed, 6788 insertions(+), 44 deletions(-)
```

| 面 | 结果 |
| --- | --- |
| `crates/sync-protocol/` | 零改动 ✓ |
| `schemas/` | 零改动 ✓ |
| `fixtures/` | 零改动 ✓ |
| `vendor/` | 零改动 ✓ |
| 所有 `Cargo.toml`（含派发单点名的两个） | 零改动 ✓ |
| 增量总量 | 26 files / +6788 / −44 ✓（与派发事实、`reports/merge-mu2-r2.md` 一致） |

**步骤 1 判定：PASS。** 14 个未跟踪项中唯一意外项（根 `verification.md`）经核实为游离副本，处理见 §4.1。

---

## 2. premerge 门禁（合入前，完整原始 JSON）

在**候选工作区**内运行、`--planning-root` 指向主检出（这是本变更既有的调用形态，与 `reports/merge-mu2-r2.md` 一致）：

```
$ cd /d/Project/acp-remote/.worktrees/integ
$ npx --quiet --no-install openspec-agentic workflow check \
    --change "sync-scope-and-pwa-client" --planning-root D:/Project/acp-remote --stage premerge --json
EXIT=1
```

### 完整 JSON 输出（原文）

```json
{
  "result": "FAIL",
  "stage": "premerge",
  "contractDigest": "sha256:ecfc245c119d41b1b76a7bfdde9b11f93e751628a894f4b56f1fc2dba90e0e91",
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
    "同一批次同时有 coder 与 tester 已开工（W2（coder：WP3、WP4 ∥ tester：TP1）），但没有任何跨角色并行窗口：契约明确时编码与测试设计必须并行（开工/交付时间由 openspec-agentic dispatch 写入）"
  ],
  "boundary": "仅检查结构、引用和版本；不证明用户批准、角色独立性或测试真实性。",
  "planningDigest": "plan-v2:sha256:78d7d1f6a0818326acab3fc17170a2b452453c42d6a5ac09632a68d1140d0229",
  "candidateCommit": "5ba44fd35be2cd3717006c0995a8404c3315f388"
}
```

### 2.1 逐项核对：errors **恰好只有一条**

前两轮 merger（`merge-mu2-r1`/`merge-mu2-r2`）见到的 21/23 条登记面缺口（Review Findings 缺行、Handoff Index 缺 DELIVERY PASS 行、Worktree Handoff 缺 attempt 行、TP1 State 不一致、Contract Freeze 不可读×3、Premerge History、跨角色并行窗口）**已全部清零**，本轮仅剩 **1 条**，且正是用户裁决的已知偏离。**未出现任何其它 error。**

同时复核凭据未被推翻：

```
$ sha256sum openspec/changes/sync-scope-and-pwa-client/reports/PV1.log \
             openspec/changes/sync-scope-and-pwa-client/reports/review-mu2-candidate-r1.md
8b218a4a33a76b07d654a76f1cc8b05c7e4951adb6bbd30b883681c5dc0c2abb  …/PV1.log
6a55ead436c4d799437a2538cece2aeb2381da18287791de0b26faed263885e8  …/review-mu2-candidate-r1.md
```

与门禁 JSON 自带的 `evidence[].sha256` **逐字相等**；门禁**未**报任何 `receipt … 报告摘要不匹配` / `… 不可读取` / `reviewer 与 author` / `candidate_commit 与该行不一致` / `目标提交不是候选提交的祖先` 类错误。故候选 PV1 与独立检视 PASS 的结论**未被推翻**。

### 2.2 仅剩的一条 error = 用户已裁决的已知偏离（引用 `verification.md` 登记节）

**error 原文**：

> 同一批次同时有 coder 与 tester 已开工（W2（coder：WP3、WP4 ∥ tester：TP1）），但没有任何跨角色并行窗口：契约明确时编码与测试设计必须并行（开工/交付时间由 openspec-agentic dispatch 写入）

**登记位置**：`openspec/changes/sync-scope-and-pwa-client/verification.md` 的 **「### 已知偏离登记：W2 跨角色并行窗口（用户 2026-10-04 裁决选项 b）」** 节。该节逐字登记：

- **门禁现状**：MU2 的 premerge 门禁在完成 F13 修正与全部登记补齐后，仅剩**一条** error——即本条。
- **事实**：`dispatch-queue.jsonl` 中 TP1（tester）于 `2026-10-03T11:47Z` 开工、WP3/WP4（coder）于 `2026-10-03T19:31Z` 开工，二者无时间重叠。
- **为何结构性、与执行质量无关**：`plan.md` 的 Execution Waves 对 TP1 的 Enter Condition 逐字写明「TP1 的 `schema_drift.rs` 与负例向量**必须编译并跑在 WP1/WP2 的交付提交上**，因此实际开工落在 MU1a 合入之后；该等待是 **Merge Strategy 的 Order 约束**，不是 code/contract/resource 依赖，故静态层级仍为 W2」。即**静态波次把 TP1 归入 W2，而合入顺序强制它等到 MU1a 之后**——W2 内的「编码与测试设计并行」在时间上不可能成立。改 plan 去声称「它们能并行」等于编造执行事实。
- **用户裁决（2026-10-04，选项 b）**：采纳本条为**已知偏离**，不在 plan.md 中改写波次声明、不重开 DDR。

**merger 处置**：本条为门禁唯一 FAIL 项，且属用户裁决接受的已知偏离；派发方已明确授权「在记录该偏离的前提下完成合入」。故按授权继续，**未改 plan.md、未改台账时序**。

**最终验收义务**（引自登记节，供 `## Final Assessment` 复述）：本条与「F13 修正后 Round 13 的三项 MINOR（F47/F48/F49）」、「`reports/review-w3-r2.md` 缺失」、「TP2 检视提出的 Windows/UNC 面在 CI 零运行与跨窗口去重未覆盖（F1/F2）」一并列入。

---

## 3. 快进主分支（`--ff-only`）

```
$ cd /d/Project/acp-remote
$ git merge --ff-only integ/mu2-wiring
Updating 33040d7..5ba44fd
Fast-forward
 crates/agent-host/src/bin/acpr-fake-acp-agent.rs   |   12 +
 crates/agent-host/src/host.rs                      |  162 +-
 crates/agent-host/src/lib.rs                       |    2 +
 crates/agent-host/src/mapper.rs                    |  369 +++-
 crates/agent-host/src/node.rs                      |  547 ++++++
 crates/agent-host/src/process.rs                   |  114 +-
 crates/agent-host/tests/session.rs                 |  444 ++++-
 crates/app/src/compose.rs                          |  149 +-
 crates/app/tests/node_link_e2e.rs                  |  723 ++++++++
 crates/core/src/broker.rs                          |  777 +++++++-
 crates/core/src/derive.rs                          | 1137 ++++++++++++
 crates/core/src/lib.rs                             |    5 +
 crates/core/src/model/json.rs                      |   44 +
 crates/core/src/model/mod.rs                       |    4 +-
 crates/core/src/ports.rs                           |   39 +
 crates/storage-sqlite/src/session_store.rs         |   12 +
 crates/storage-sqlite/tests/commit.rs              |    7 +
 crates/storage-sqlite/tests/contract_v03.rs        |    1 +
 .../tests/derived_events_behaviour.rs              | 1867 ++++++++++++++++++++
 crates/storage-sqlite/tests/enum_coverage.rs       |    1 +
 crates/storage-sqlite/tests/migration.rs           |    1 +
 crates/storage-sqlite/tests/retention.rs           |    2 +
 .../storage-sqlite/tests/session_version_rule.rs   |    3 +
 crates/storage-sqlite/tests/title_write.rs         |  388 ++++
 crates/storage-sqlite/tests/workspace_alias.rs     |    4 +
 docs/CORE_PORTS_AND_STORAGE.md                     |   18 +-
 26 files changed, 6788 insertions(+), 44 deletions(-)
 create mode 100644 crates/agent-host/src/node.rs
 create mode 100644 crates/core/src/derive.rs
 create mode 100644 crates/storage-sqlite/tests/derived_events_behaviour.rs
 create mode 100644 crates/storage-sqlite/tests/title_write.rs
MERGE EXIT=0

$ git rev-parse HEAD
5ba44fd35be2cd3717006c0995a8404c3315f388

$ git rev-parse refs/heads/main
5ba44fd35be2cd3717006c0995a8404c3315f388
```

**实际合入提交 SHA：`5ba44fd35be2cd3717006c0995a8404c3315f388`**（=`integ/mu2-wiring` 的 tip，=`candidateCommit`）。纯快进，未产生合并提交。

---

## 4. 主分支回归（三道）

目标目录复用既有 `CARGO_TARGET_DIR=D:\Project\acp-remote\.target-wt\mu2-main`（未为每次检查新建）。

### 4.1 第 1 道：`npm run verify`（tasks 6.20 指定的 PG1）

**逐字命令**：

```
$ cd /d/Project/acp-remote
$ CARGO_TARGET_DIR=D:/Project/acp-remote/.target-wt/mu2-main npm run verify > openspec/changes/sync-scope-and-pwa-client/reports/PV1-main-mu2.log 2>&1
EXIT=0
```

**结果：PASS。**
- 日志：`openspec/changes/sync-scope-and-pwa-client/reports/PV1-main-mu2.log`
- 行数：**2369**；sha256：**`938ba23ef6f554f33cb81c2431dcaad4b322c74a40a3101a0d6f592fa69ab1f3`**
- `check` 十道全绿（`doc links OK: 3732 relative links, 81986 section refs across 4725 markdown files`）；`check:rust` 的 `cargo fmt --all -- --check` + `clippy --workspace --all-targets --all-features -D warnings` + `cargo test --workspace --all-features` 全绿。
- **已知 flake `crates/app/tests/daemon_lifecycle.rs` 未命中**（无需复跑）。

> **首次执行的失败与处置（必读）**：本道**首次执行 exit 1**，失败点在 `check:docs`（`scripts/check-doc-links.mjs`），报 45 条 `§X.Y 在 verification.md 中不存在`。失败原始日志已**原样保留**为 `reports/PV1-main-mu2-attempt1-doclink-fail.log`（sha256 `d384f86f66c30bb60322fbd1197ba03c1aa0e3f9b1a42799a2a1d3ad70d9ce69`）。
>
> **根因（经验证，非候选代码缺陷）**：`check-doc-links.mjs` 的 `resolveDocumentName`（脚本第 116-128 行）对「裸 `verification.md`」引用按候选 `[同目录, 仓库根, 仓库根/docs]` 解析。存在**仓库根的游离未跟踪 `verification.md`**（§1.1）时，归档变更 `reports/*.md` 中所有裸 `verification.md` 的 `§`-引用被**误归属到该根文件**（候选 #2 HIT）；该根文件是过期快照、不含被引用的数字小节（`14.1`/`4.1`/`3.5`），45 条引用全部判失配。这些引用本意指向**各自变更自己的** `verification.md`（`../verification.md`），而脚本不检查该候选——根文件**不存在**时引用不被归属、按设计**不判定**，故为绿灯。
>
> 该根文件解出 `contract_digest=sha256:e4cc671c…`（F13 修正前旧值），与门禁实际读取的 `changeRoot/verification.md`（`sha256:ecfc245c…e0e91`）**不是同一文件**。
>
> **处置**：将游离文件**移动**（非删除）至 `.target-wt/stray-preserved/root-verification-587c3f6d.md.bak`（sha256 原值 `587c3f6d3f4e45132fe0921c292d000e077f7a50287a76803d07c616faa6015e`），未改动变更目录内的 `verification.md`。移动后 `npm run check:docs` 立即由 45 error 变为 `doc links OK … 0 error`，证实因果；随后完整 `npm run verify` exit 0。
>
> **披露**：本单元提交时把该未跟踪文件从仓库根移入保留备份，**未删除、未改任何已跟踪内容、未动变更目录内的登记 `verification.md`**。是否恢复/删除该游离副本留待 main 裁决（恢复命令：`mv .target-wt/stray-preserved/root-verification-587c3f6d.md.bak verification.md`）。**注意**：只要该文件回到仓库根，`npm run verify` 的 `check:docs` 会再次因同样的误归属而 FAIL。

### 4.2 第 2 道：`-p core -p storage-sqlite -p agent-host --all-features`

```
$ cd /d/Project/acp-remote
$ CARGO_TARGET_DIR=D:/Project/acp-remote/.target-wt/mu2-main cargo test --locked -p core -p storage-sqlite -p agent-host --all-features
EXIT=0
```

**结果：PASS**。各 suite `test result: ok`，无 `FAILED`、无 `failed>0`；含内联/集成 + doc-tests 全绿。

### 4.3 第 3 道：`-p app --test node_link_e2e`（AC1 六项断言）

```
$ cd /d/Project/acp-remote
$ CARGO_TARGET_DIR=D:/Project/acp-remote/.target-wt/mu2-main cargo test --locked -p app --test node_link_e2e
EXIT=0
```

**结果：PASS（6 passed / 0 failed / 0 ignored）**。这是 MU2 候选检查（`plan.md:971`）不含 AC1 的补充判据——AC1 六项断言才是 R8 落库与端到端闭合的实际判据：

```
test ac1_the_composition_root_updates_the_session_title_from_the_agent_notification ... ok
test ac1_the_composition_root_persists_node_level_events_without_session_identity ... ok
test tls_direct_terminates_the_same_handshake ... ok
test the_controlled_path_runs_end_to_end_and_revocation_propagates ... ok
test a_narrowing_repair_closes_the_live_attachment ... ok
test ac1_the_composition_root_persists_file_changes_on_the_owned_path_in_origin_order ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.38s
```

- 日志：`openspec/changes/sync-scope-and-pwa-client/reports/node-link-e2e-main-mu2.log`（sha256 `818c60357335e6db3a507d9b7b34c151a69dbc81469814b4adc58a34f9227869`）

### 回归汇总

| # | 命令 | 结果 | 日志 |
| --- | --- | --- | --- |
| 1 | `npm run verify` | **PASS exit 0**（2369 行） | `reports/PV1-main-mu2.log`（sha256 `938ba23e…ab1f3`） |
| — | 首次执行（环境致因失败，保留） | FAIL exit 1（`check:docs` 45 条） | `reports/PV1-main-mu2-attempt1-doclink-fail.log`（sha256 `d384f86f…d9ce69`） |
| 2 | `cargo test --locked -p core -p storage-sqlite -p agent-host --all-features` | **PASS exit 0** | （stdout，已核对） |
| 3 | `cargo test --locked -p app --test node_link_e2e` | **PASS exit 0**（6/6） | `reports/node-link-e2e-main-mu2.log`（sha256 `818c6035…7869`） |

已知 flake（`crates/app/tests/daemon_lifecycle.rs`）三道均**未命中**，无 flake 复跑。

---

## 5. 合并相对候选的新增差异（步骤 4）

**零新增差异**（纯 `--ff-only`）。

```
$ cd /d/Project/acp-remote
$ git rev-parse HEAD
5ba44fd35be2cd3717006c0995a8404c3315f388
$ git rev-parse integ/mu2-wiring
5ba44fd35be2cd3717006c0995a8404c3315f388      # 与 HEAD 逐字相等

$ git diff --stat integ/mu2-wiring HEAD
（空输出）

$ git rev-parse HEAD^{tree}
bb4d5be189ef831a99f9ef8ee86d4e1ebaa24849
$ git rev-parse integ/mu2-wiring^{tree}
bb4d5be189ef831a99f9ef8ee86d4e1ebaa24849      # tree SHA 二者相同
```

依据：合入后 `HEAD` 与候选 `integ/mu2-wiring` 指向**同一提交**、`git diff` 为空、两侧 **tree SHA 同为 `bb4d5be1…4849`**。纯快进不产生合并提交，故不存在「合并后 vs 候选」的差异。

---

## 6. 资源释放与副作用声明

- **未执行任何** `git push` / `git reset` / `git checkout` / `git branch -d` / `git worktree remove` / `git rebase`。
- 分支保留（10 个）：`main`（→ `5ba44fd`）、`integ/mu2-wiring`、`feat/wp1..wp4`、`feat/tp1-contract-vectors`、`feat/tp2-behaviour`、`merge/mu1-candidate`、`merge/mu1a-candidate-r1` 全部保留。
- Worktree 保留（9 个）：主检出 + `integ`/`mu1-merge`/`tp1`/`tp2`/`wp1`/`wp2`/`wp3`/`wp4`，全部完好（`git worktree list` 9 项）。
- 主检出 `status --porcelain`：合入后仍**仅三个预期未跟踪项**（`.target-wt/`、`.worktrees/`、`openspec/changes/sync-scope-and-pwa-client/`）；**未卡在合并态**（无 `MERGE_HEAD`）。
- 磁盘：复用既有 `CARGO_TARGET_DIR=.target-wt/mu2-main`，未新建 target 目录；未触发 1455/LNK1102（D: 盘 567 GB 可用）。
- **未修改** `openspec/changes/sync-scope-and-pwa-client/verification.md`（含其内联 `agentic-premerge` 块）、**未修改** `reports/receipt-mu2.md`、**未修改** `dispatch-queue.jsonl`、**未改 `plan.md`、未改 `tasks.md`**。
- 本单元创建的路径：`reports/PV1-main-mu2.log`（tasks 6.20 指定）、`reports/PV1-main-mu2-attempt1-doclink-fail.log`（失败原始日志）、`reports/node-link-e2e-main-mu2.log`、`.target-wt/stray-preserved/root-verification-587c3f6d.md.bak`（游离文件保留）、本报告。
- **未声称执行**任何本地无等价物的检查（`deps` / `advisories` / `secrets`）；未推送、未发布、未归档。

---

## 7. 合入条件核对

| 条件 | 状态 |
| --- | --- |
| 候选建立在当前最新主分支上 | **满足**（`merge-base --is-ancestor` exit 0） |
| 冻结合同与依赖面零 diff | **满足**（6 个面全空） |
| 合入方式 `--ff-only` | **满足**（实际即 fast-forward，`33040d7..5ba44fd`） |
| premerge 门禁 | 仅剩 1 条 error，**= 用户已裁决的已知偏离**（§2.2），授权合入 |
| `npm run verify` 全绿 | **满足**（exit 0，`reports/PV1-main-mu2.log`） |
| `-p core -p storage-sqlite -p agent-host --all-features` | **满足**（exit 0） |
| `node_link_e2e` 六项断言全绿 | **满足**（6/6，`reports/node-link-e2e-main-mu2.log`） |
| 合并相对候选新增差异 | **零**（同提交、tree SHA 相同） |
| `deps` / `advisories` / `secrets` | 本地无等价物，未执行、未声称通过 |
| 推送授权 | 无（未推送） |

**合入完成：`refs/heads/main` = `5ba44fd35be2cd3717006c0995a8404c3315f388`。**

---

## 8. 交 main 的三项

1. **游离文件裁决**：仓库根 `verification.md`（未跟踪、F13 前旧快照）已移入 `.target-wt/stray-preserved/root-verification-587c3f6d.md.bak` 以消除 `check:docs` 的确定性误判。请裁决恢复/删除；**只要它回到仓库根，`npm run verify` 会再次 FAIL**（§4.1）。
2. **下游解锁**：MU2 已合入主分支，MU3a（WP5a）的「等待计划前序单元 MU2 合入及回归」前置可从计划台账侧核销。
3. **最终验收复述清单**：`## Final Assessment` 须复述 §2.2 列出的已知项（W2 跨角色并行窗口偏离、F47/F48/F49、`review-w3-r2.md` 缺失、TP2 的 F1/F2）。
