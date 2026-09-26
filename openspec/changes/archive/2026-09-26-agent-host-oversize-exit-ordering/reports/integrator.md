<!-- 集成 Agent 报告（integrator）。阶段 A = tasks 6.1 + 6.2（核实基线、构造候选）。阶段 B（6.6/6.7）另行下发，本报告不含合入。行为以 specs/local-agent-host/spec.md 为准，方案以 design.md D1–D3 为准。 -->

# 集成报告（integrator / integrate — 阶段 A：6.1 + 6.2）

## Shared Report

- **task_id**: `6.1`（核实目标基线）、`6.2`（构造候选）
- **role**: integrator
- **phase**: integrate
- **agent_context**: 独立集成子 Agent（worker，run id `c28d2fe2-f6d5-4546-be69-eceb499e222e`），
  由主 Agent 单独创建、不兼任；未复用实现者/reviewer 的对话上下文；共享工作树
  `D:\Project\acp-remote` 中的集成执行者；不担任实现 / review / 最终验收角色。
- **target_revision**: `25acb00d6895bcbe1ba36b658e0dd4706fd0347a`（候选=当前分支
  `feat/agent-host-oversize-exit-ordering` tip；树 `116ae4b5128303a95e42ca859683e87fd84f79bf`）
- **scope**（本阶段实际写入）:
  - `openspec/changes/agent-host-oversize-exit-ordering/reports/integrator.md`（本报告，唯一仓库内写入）。
  - 未写任何代码、测试、`docs/`、`schemas/`、`fixtures/`、`compatibility/` 或权威
    `plan.md`/`tasks.md`/`verification.md`。**未执行任何 git 写命令**（无 add/commit/switch/merge/stash/fetch）；
    仅使用只读查询（`git rev-parse`/`status`/`log`/`merge-base`/`rev-list`/`diff`）与 `cargo build`。
- **changes**: 本阶段不产生代码改动（仅新增本报告）。被集成候选的代码改动为已验收 WP1 交付
  `af64e85`（单文件 `crates/agent-host/src/process.rs` +4/−2）。
- **checks**: `cargo build --locked --workspace` → EXIT=0（三次采样，见下）。无 git 写操作。
- **issues**: 无阻断问题。执行中发现分支 tip 被主 Agent 簿记提交前移（见「基线变化重建候选」），
  已按指令以末次 tip 重新固定候选并重跑构建。
- **result**: PASS（6.1 与 6.2 的本地可判定条件均满足；阶段 B 未执行）
- **evidence_paths**:
  - `openspec/changes/agent-host-oversize-exit-ordering/reports/integrator.md`（本报告）
  - 运行时会话产物副本：`…/subagent-artifacts/outputs/c28d2fe2-f6d5-4546-be69-eceb499e222e/openspec/changes/agent-host-oversize-exit-ordering/reports/integrator.md`（内容一致）
- **resource_cleanup**: 仅执行 `cargo build`（共享 `target/`，常规使用），未创建 worktree、容器、端口、
  外部服务或新增依赖；未启动长驻测试进程；`git status --porcelain` 结束为干净（无本 Agent 造成的残留）。

## 执行环境

| 项 | 值 |
| --- | --- |
| 仓库根 | `D:/Project/acp-remote` |
| 工作树类型 | 共享工作树（非独立集成 worktree），当前分支 `feat/agent-host-oversize-exit-ordering` |
| OS / shell | `MINGW64_NT-10.0-26200`（Windows 10.0.26200，Git Bash） |
| git | `git version 2.55.0.windows.5` |
| rustc | `rustc 1.98.1 (48a229cea 2026-09-01)`（版本由 `rust-toolchain.toml` 固定） |
| cargo | `cargo 1.98.1 (797e8a9bc 2026-08-05)` |

## 6.1 核实目标基线

任务要求：`git rev-parse refs/heads/main` + `git status --porcelain`，开始/结束各一次；核对
`merge-base --is-ancestor` 与 ahead/behind；核对背景值 main = `84a8a0593b2272252a458bfeb0648bc6f1a2f669`。

### 开始采样

```text
$ git rev-parse refs/heads/main
84a8a0593b2272252a458bfeb0648bc6f1a2f669
$ git rev-parse HEAD
dee3e223630deef53621c6717fdb647ca70671e6
$ git branch --show-current
feat/agent-host-oversize-exit-ordering
$ git status --porcelain
（空）
```

- 目标 main 提交：**`84a8a0593b2272252a458bfeb0648bc6f1a2f669`** — 与主 Agent 背景值**一致**，以实际值为准。
- `git merge-base --is-ancestor 84a8a05… HEAD` → **EXIT=0**（main 是 HEAD 的祖先，可安全 ff）。
- `git rev-list --left-right --count main...HEAD` → `0	4`（**behind=0 / ahead=4**）。

### 结束采样（最终固定值）

```text
$ git rev-parse refs/heads/main
84a8a0593b2272252a458bfeb0648bc6f1a2f669
$ git rev-parse HEAD
25acb00d6895bcbe1ba36b658e0dd4706fd0347a
$ git rev-parse HEAD^{tree}
116ae4b5128303a95e42ca859683e87fd84f79bf
$ git merge-base --is-ancestor 84a8a05… HEAD ; echo exit=$?
exit=0
$ git rev-list --left-right --count main...HEAD
0	5
$ git status --porcelain
（空）
```

- 目标 main 提交结束仍为 **`84a8a0593b2272252a458bfeb0648bc6f1a2f669`**（未移动）。
- ahead/behind 结束为 `0	5`（tip 从 `dee3e22` 前移到 `25acb00`，见下）。
- 稳定性复采样（间隔 3s）：HEAD 两次均为 `25acb00…`，`git status --porcelain` 均空。

### 基线变化与防竞态记录

- 开始采样时 tip = `dee3e22`（ahead=4）；执行过程中主 Agent 落地簿记提交
  `25acb00 docs(repo): 登记集成就绪核对 [5.1-5.2]`，tip 前移为 **`25acb00`**（ahead=5）。
- 期间还短暂观察到 `tasks.md` / `verification.md` 处于已暂存（staged）状态，随后被主 Agent
  提交入库、工作树恢复干净——属主 Agent 的簿记写入，**本 Agent 未触碰、未 stash、未回退**。
- 按任务指令「基线变化重建候选」，以**末次 tip `25acb00`** 重新固定候选并重跑构建（见 6.2）。
- 目标 main（`84a8a05`）全程未移动；候选相对 main 始终 `behind=0`，满足后续 `--ff-only` 合入前提（阶段 B 判定）。

## 6.2 构造候选

任务要求：候选=当前分支 tip（固定完整 SHA 与树哈希）；`cargo build --locked --workspace` EXIT=0；
确认工作树干净；`git diff --name-status <main>..<候选> -- crates/` 机械核验代码面=单文件，且
`docs/`/`schemas/`/`fixtures/`/`compatibility/`/根配置零改动。

### 候选固定

| 项 | 值 |
| --- | --- |
| base / 目标主分支 | `refs/heads/main` = `84a8a0593b2272252a458bfeb0648bc6f1a2f669`（树 `ea4919607f75d23c6ae88c5a8f368ff703353d2d`） |
| **候选提交** | **`25acb00d6895bcbe1ba36b658e0dd4706fd0347a`** |
| 候选树 | `116ae4b5128303a95e42ca859683e87fd84f79bf` |
| 上游已验收 WP1 交付 | `af64e85`（其 `process.rs` blob = `46fbe458cb5745b02f084d6391b697eb0c9a248a`） |
| 候选 `process.rs` blob | `46fbe458cb5745b02f084d6391b697eb0c9a248a`（与 `af64e85` **逐字节一致**） |
| main `process.rs` blob | `4e448cbf5524275ad2837b785205b25dab0620f4` |
| ahead/behind vs main | `behind=0 / ahead=5` |

候选 tip 相对 `af64e85` 多出的两个提交（`e544ac9`、`dee3e22`、`25acb00`）均为
`openspec/changes/agent-host-oversize-exit-ordering/**` 簿记文档提交，**不含代码改动**；
候选代码 blob 与已验收交付逐字节相同，说明候选的代码面等于 RV1/PV1/PV2 已验收的版本。

### 构建

| # | 采样时 tip | 命令 | 退出码 | 关键结果 |
| --- | --- | --- | --- | --- |
| 1 | `dee3e22` | `cargo build --locked --workspace` | `EXIT=0` | `Finished dev profile … in 0.19s`（已是最新，未触发编译） |
| 2 | `dee3e22`（touch 后） | `cargo build --locked --workspace` | `EXIT=0` | 强制重编 `agent-host` + `app`：`Compiling agent-host`/`Compiling app`，`Finished … in 2.75s`（证明候选源码可真实编译） |
| 3 | **`25acb00`（固定候选）** | `cargo build --locked --workspace` | `EXIT=0` | `Finished dev profile … in 0.24s`；构建前后 `HEAD` 均为 `25acb00…`（未漂移） |

> 采样 2 前的 `touch crates/agent-host/src/process.rs` 只改 mtime、不改内容，用于让采样 1 的
> 全缓存结果不至于空泛；采样 3 针对最终固定候选再次执行任务要求的命令并确认 tip 稳定。

### 工作树状态

```text
$ git status --porcelain
（空）            # 结束态；本 Agent 未产生未提交改动
$ git diff --cached --name-only
（空）            # 无暂存
```

工作树在开始与结束（重新固定候选后）均为**干净**；执行过程中短暂出现的 staged 行由主 Agent
簿记提交吸收，本 Agent 未对其做任何操作。

### 代码面机械核验

```text
$ git diff --name-status 84a8a05…25acb00 -- crates/
M	crates/agent-host/src/process.rs          # 有且仅有这一行

$ git diff --stat 84a8a05…25acb00 -- crates/
 crates/agent-host/src/process.rs | 6 ++++--
 1 file changed, 4 insertions(+), 2 deletions(-)

$ git diff --name-status 84a8a05…25acb00 -- docs/ schemas/ fixtures/ compatibility/
（空）            # 零改动

$ git diff --name-status 84a8a05…25acb00 -- '*.toml' '*.json' '*.mjs' '*.yml' '*.yaml' ':!openspec/**'
（空）            # 根配置零改动
```

- 代码面 = **单文件 `crates/agent-host/src/process.rs`、+4/−2**（仅 `abort_agent` 语句顺序与其注释；
  与 WP1 交接/RV1 记录一致）。
- 全量 `git diff --name-status main..候选` 的其余条目全部为
  `openspec/changes/agent-host-oversize-exit-ordering/**` 新增的规划/报告文档（`A`），无其它 crate、
  无根配置、无 `docs/`/`schemas/`/`fixtures/`/`compatibility/` 改动。

## handoff_index

```yaml
handoff_index:
  - task_id: "6.1"
    role: integrator
    phase: integrate
    stage: candidate
    target_revision: "25acb00d6895bcbe1ba36b658e0dd4706fd0347a"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: openspec/changes/agent-host-oversize-exit-ordering/reports/integrator.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "本次在共享工作树 D:/Project/acp-remote 只读核实目标基线：开始/结束两次 git rev-parse refs/heads/main 均为 84a8a0593b2272252a458bfeb0648bc6f1a2f669（与背景值一致，以实际值为准）；merge-base --is-ancestor main HEAD EXIT=0；rev-list --left-right --count main...HEAD 结束为 0	5；两次 git status --porcelain 均空。target_revision 写候选 tip 25acb00（6.1 所固定的目标版本）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.2"
    role: integrator
    phase: integrate
    stage: candidate
    target_revision: "25acb00d6895bcbe1ba36b658e0dd4706fd0347a"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: openspec/changes/agent-host-oversize-exit-ordering/reports/integrator.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "基于已核实基线（main 84a8a05，未移动）以未次分支 tip 固定候选 25acb00（树 116ae4b5128303a95e42ca859683e87fd84f79bf）；cargo build --locked --workspace EXIT=0（三次采样，其中一次强制重编 agent-host+app）；git diff --name-status main..候选 -- crates/ 仅 M crates/agent-host/src/process.rs（+4/−2）；docs/schemas/fixtures/compatibility/根配置零改动；工作树结束干净。候选 process.rs blob 46fbe458… 与已验收 af64e85 逐字节一致。"
    source_evidence: NOT_APPLICABLE
```

## 未解决项与下一步

- **未解决项**：无阻断项。候选构建 PASS 只代表 6.2 本地可判定条件满足，**不等于**已合入或
  最终验收通过（候选 PASS ≠ main PASS）。
- **阶段 A 结论**：基线已核实且未漂移（main `84a8a05`）；候选固定为 `25acb00`，
  `cargo build --locked --workspace` EXIT=0，代码面单文件 +4/−2、无文档/协议/根配置改动。
- **下一步（由主 Agent 调度，不在本阶段）**:
  1. 候选轮 [PV1] `npm run verify` + [PV2] 三连跑（tasks 6.3），以及候选独立 reviewer（6.4）与
     Main E2E not-applicable 核对（6.5）。
  2. premerge 门 `openspec-agentic workflow check … --stage premerge` PASS 后，另行下发阶段 B：
     本地合入 `refs/heads/main`（优先 `--ff-only`，不推送）+ 主分支回归 [PV1]（tasks 6.6/6.7）。
  3. 若合入前分支 tip 或 main 再次移动，按「基线变化重建候选」以末次 tip 重新固定并重验。

---

<!-- 阶段 B = tasks 6.6 + 6.7（本地 ff 合入 + 主分支回归）。本阶段不提交、不 push。 -->

# 集成报告（integrator / integrate — 阶段 B：6.6 + 6.7）

## Shared Report（阶段 B）

- **task_id**: `6.6`（本地合入 `refs/heads/main`）、`6.7`（主分支回归 [PV1]）
- **role**: integrator
- **phase**: integrate
- **agent_context**: 同一独立集成子 Agent（worker，run id `c28d2fe2-f6d5-4546-be69-eceb499e222e`，阶段 B 续用）。
  由主 Agent 单独创建、不兼任；未复用实现者/reviewer 对话；共享工作树 `D:\Project\acp-remote` 中的
  唯一集成执行者；不担任实现 / review / 最终验收角色。
- **target_revision**: `25acb00d6895bcbe1ba36b658e0dd4706fd0347a`（候选=合入后 main，树 `116ae4b5128303a95e42ca859683e87fd84f79bf`）
- **scope**（阶段 B 实际写入）:
  - `openspec/changes/agent-host-oversize-exit-ordering/reports/integrator.md`（追加本阶段 B 报告）
  - `openspec/changes/agent-host-oversize-exit-ordering/reports/main-verify.log`（6.7 回归原始输出）
  - 仓库外备份三份（见「备份」），并在步骤 6 写回工作树。
  - **未提交、未 push**；未修改任何代码、测试、`docs/`、`schemas/`、`fixtures/`、`compatibility/`
    或权威 `plan.md`/`tasks.md`/`verification.md` 的已跟踪内容（tasks/verification 的未提交改动原样保留）。
- **changes**: 阶段 B 不产生代码改动；合入把候选 `25acb00` 以 ff 落到 `refs/heads/main`。
- **checks**: `git merge --ff-only`（EXIT=0）、`npm run verify`（main，EXIT=0）。
- **issues**: 执行前发现工作树有**第三个**未提交跟踪文件（`reports/wp1-handoff.md`），不在任务前提
  （仅两个文件）内。已 `contact_supervisor` 求决策，主 Agent 选 (a)：备份到仓库外并 `git checkout --`
  三个文件后继续。无其它阻断。
- **result**: PASS（6.6、6.7 本地条件均满足）
- **evidence_paths**:
  - `openspec/changes/agent-host-oversize-exit-ordering/reports/integrator.md`（本报告）
  - `openspec/changes/agent-host-oversize-exit-ordering/reports/main-verify.log`（6.7 原始输出）
  - 运行时会话产物副本（阶段 A+B 合并版）：`…/subagent-artifacts/outputs/c28d2fe2-f6d5-4546-be69-eceb499e222e/openspec/changes/agent-host-oversize-exit-ordering/reports/integrator.md`
- **resource_cleanup**: 仅 `cargo`/`npm` 构建与测试（共享 `target/`、常规使用）；未创建 worktree、容器、
  端口、外部服务或新增依赖；回归前后 `$TEMP/acpr-*` 计数均为 0；无遗留测试进程。

## 前置条件（主 Agent 已满足，本阶段仅核对引用）

- 候选轮 [PV1] PASS：`reports/candidate-pv1-verify.log`
- 候选轮 [PV2] 三连跑全绿：`reports/stress-runs.log`
- RV2 候选 review PASS：`reports/rv2-candidate.md`
- Main E2E not-applicable 核对：完成
- premerge 门 PASS（`verification.md` 的 `agentic-premerge` 块）：`target_commit=84a8a0593b2272252a458bfeb0648bc6f1a2f669`、`candidate_commit=25acb00d6895bcbe1ba36b658e0dd4706fd0347a`

## 步骤 1 — 备份（仓库外）

| 源（工作树） | 备份路径 | 字节数 | sha256 |
| --- | --- | --- | --- |
| `…/verification.md` | `D:\Project\verification-with-block-aho.md` | 9156 | `d39c5eb28cedbc452275d95387fabddda07f666563d371fcf3f3ad1bc29f2087` |
| `…/tasks.md` | `D:\Project\tasks-with-ticks-aho.md` | 6920 | `c39508faa0c7175a3b711b79040b57948fb2b987f414187fb79c48a5311a2f23` |
| `…/reports/wp1-handoff.md` | `D:\Project\wp1-handoff-with-fix-aho.md` | 13206 | `8f83ad756ec69bd681b0cf5be731baa9cc74190dc4e3a5f5c645af23c7831d74` |

## 步骤 2 — 状态核对

```text
$ git rev-parse HEAD
25acb00d6895bcbe1ba36b658e0dd4706fd0347a     # = 预期候选 ✓
$ git rev-parse refs/heads/main
84a8a0593b2272252a458bfeb0648bc6f1a2f669     # = 预期基线 ✓（未移动）
$ git branch --show-current
feat/agent-host-oversize-exit-ordering
$ git status --porcelain
 M …/reports/wp1-handoff.md      # <-- 任务前提未列出的第三个跟踪改动（见下）
 M …/tasks.md
 M …/verification.md
?? …/reports/integrator.md
?? …/reports/rv2-candidate.md
```

- **前提偏离**：任务称只有 `verification.md` 与 `tasks.md` 两个跟踪文件有未提交改动，实测还有
  `reports/wp1-handoff.md`（同为本分支独有、main 上不存在的跟踪文件）。`git cat-file -e
  refs/heads/main:…/reports/wp1-handoff.md` → `fatal, exit 128`（main 无此文件）。
  其本地改动为单行：第 93 行「56 行」→「55 行」（index `b76e90f..335e890`）。
- 由于该文件未提交且 main 上不存在，`git switch main` 会与另两个文件同样因「本地改动将被覆盖」被拒；
  步骤 3 原样只 checkout 两个文件时，步骤 4 会失败。
- 已 `contact_supervisor`（reason=need_decision）上报该偏差；主 Agent 回复：该 56→55 是主 Agent 修
  RV2-F1 时留下的未提交改动（候选 `25acb00` 冻结后修的报告级问题，故意不随候选），
  **选 (a)**：同样备份到仓库外 → `git checkout --` 三个文件 → 继续步骤 4/5；步骤 6 写回三份备份，
  该修正由主 Agent 在合并后簿记提交中随 RV2 登记入库。本报告按 (a) 执行。

## 步骤 3 — 清理工作树跟踪改动

```text
$ git checkout -- …/verification.md …/tasks.md …/reports/wp1-handoff.md
exit=0
$ git status --porcelain
?? …/reports/integrator.md
?? …/reports/rv2-candidate.md      # 仅剩未跟踪文件，不影响切换/合并
```

## 步骤 4 — 本地 ff 合入（6.6）

```text
$ git switch main
Switched to branch 'main'          # exit=0
$ git rev-parse HEAD
84a8a0593b2272252a458bfeb0648bc6f1a2f669     # 合入前 main
$ git merge --ff-only feat/agent-host-oversize-exit-ordering
Updating 84a8a05..25acb00
Fast-forward                                    # exit=0，非 ff 会在此失败
 crates/agent-host/src/process.rs                   |   6 +-
 …（其余为 openspec/changes/agent-host-oversize-exit-ordering/** 文档）
 10 files changed, 755 insertions(+), 2 deletions(-)
$ git rev-parse HEAD
25acb00d6895bcbe1ba36b658e0dd4706fd0347a     # = 候选 ✓
$ git rev-parse HEAD^{tree}
116ae4b5128303a95e42ca859683e87fd84f79bf     # = 候选树 ✓
$ git diff feat/agent-host-oversize-exit-ordering main --stat
（空）                                          # main == 分支 tip，无残余差异 ✓
```

| 项 | 值 |
| --- | --- |
| 合入前 main | `84a8a0593b2272252a458bfeb0648bc6f1a2f669` |
| **合入后 main（= 候选）** | **`25acb00d6895bcbe1ba36b658e0dd4706fd0347a`** |
| 合入后 main 树 | `116ae4b5128303a95e42ca859683e87fd84f79bf` |
| 合入方式 | `--ff-only`（EXIT=0，"Fast-forward 84a8a05..25acb00"），**未 push** |

## 步骤 5 — 主分支回归 [PV1]（6.7）

```text
$ npm run verify            # 在 main @ 25acb00 上执行
EXIT=0
```

- 原始输出：`openspec/changes/agent-host-oversize-exit-ordering/reports/main-verify.log`
  （68137 字节，sha256 `b42ed5837d009386f2b8dad08d117b70e99fc864be88cf545e4750e36ad58f2d`）。
- 合同门禁：`Totals: 17 passed, 0 failed (17 items)`（schemas/commands/errors/features/assets/acp/docs/
  boundaries/drift/agentic 全 PASS）。
- Rust 门禁：`cargo fmt --all -- --check` + `cargo clippy … -D warnings` + `cargo test --locked --workspace
  --all-features` 全绿；日志含 **82** 处 `test result: ok`，`0 failed`。
- 关键用例：`test oversize_frame_ends_the_agent_and_fails_pending_requests ... ok`（日志第 299 行）。
- `$TEMP/acpr-*` 计数：跑前 /tmp = 0、`C:\Users\zhang\AppData\Local\Temp` = 0；跑后两者均 = 0。

| 项 | 值 |
| --- | --- |
| 命令 | `npm run verify` |
| 分支/提交 | `main` @ `25acb00` |
| 退出码 | `EXIT=0` |
| 日志 | `reports/main-verify.log`（68137 B，sha256 `b42ed583…`） |
| `acpr-*` 跑前/跑后 | /tmp 0/0；Windows TEMP 0/0 |

## 步骤 6 — 写回备份并核对

```text
$ cp D:\Project\verification-with-block-aho.md  …/verification.md  ; cmp  -> OK
$ cp D:\Project\tasks-with-ticks-aho.md         …/tasks.md         ; cmp  -> OK
$ cp D:\Project\wp1-handoff-with-fix-aho.md     …/reports/wp1-handoff.md ; cmp -> OK
```

写回后 sha256 与备份一致（`d39c5eb2…` / `c39508fa…` / `8f83ad75…`）。

## 结束状态

```text
$ git rev-parse HEAD
25acb00d6895bcbe1ba36b658e0dd4706fd0347a     # main 停在候选
$ git status --porcelain
 M …/reports/wp1-handoff.md      # 主 Agent 的 56→55 未提交修正，已原样恢复
 M …/tasks.md                    # 6.1–6.5 勾选，已原样恢复
 M …/verification.md             # premerge 块与 6.3/6.5 记录，已原样恢复
?? …/reports/integrator.md
?? …/reports/rv2-candidate.md
$ git diff --cached --name-only
（空）                            # 无暂存；未提交、未 push
```

## handoff_index（阶段 B）

```yaml
handoff_index:
  - task_id: "6.6"
    role: integrator
    phase: integrate
    stage: main
    target_revision: "25acb00d6895bcbe1ba36b658e0dd4706fd0347a"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: openspec/changes/agent-host-oversize-exit-ordering/reports/integrator.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "在共享工作树以 ff-only 合入：合入前 main=84a8a0593b2272252a458bfeb0648bc6f1a2f669，git merge --ff-only feat/agent-host-oversize-exit-ordering EXIT=0（Fast-forward 84a8a05..25acb00）；合入后 HEAD=25acb00（树 116ae4b5128303a95e42ca859683e87fd84f79bf），git diff feat/agent-host-oversize-exit-ordering main --stat 为空；未 push。前置证据引用：candidate-pv1-verify.log（PV1）、stress-runs.log（PV2）、rv2-candidate.md（RV2）、verification.md agentic-premerge 块（target_commit=84a8a05，candidate_commit=25acb00）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "6.7"
    role: integrator
    phase: integrate
    stage: main
    target_revision: "25acb00d6895bcbe1ba36b658e0dd4706fd0347a"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: openspec/changes/agent-host-oversize-exit-ordering/reports/integrator.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "在合入后的 main @ 25acb00 运行 npm run verify：EXIT=0；合同门禁 Totals: 17 passed, 0 failed；Rust 门禁 fmt/clippy/test 全绿（82 处 test result: ok，0 failed，含 oversize_frame 用例）。原始输出 reports/main-verify.log（68137 B，sha256 b42ed5837d009386f2b8dad08d117b70e99fc864be88cf545e4750e36ad58f2d）；$TEMP/acpr-* 跑前/跑后均 0/0。"
    source_evidence: NOT_APPLICABLE
```

## 未解决项与下一步（阶段 B）

- **未解决项**：无阻断项。
- **阶段 B 结论**：候选 `25acb00` 已以 `--ff-only` 合入本地 `refs/heads/main`（未 push），
  主分支回归 [PV1] EXIT=0 全绿；三份仓库外备份已原样写回工作树（cmp/sha256 一致），
  主 Agent 的未提交改动未被破坏。
- **遗留（交主 Agent）**：
  1. 工作树仍有三份未提交改动（`verification.md`、`tasks.md`、`reports/wp1-handoff.md`），
     按主 Agent 说明将在合并后簿记提交中随 RV2 登记一并入库（本阶段不提交）。
  2. tasks 6.8（合入差异审查）：相对已验收候选无新增差异（`git diff 候选..main` 为空），可直接引用既有 RV2。
  3. 后续 7.x / 8.1 最终验收由主 Agent 执行。
