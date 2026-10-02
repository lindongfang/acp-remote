# Provisioner Report — WP3 worktree 重新指向 U1 集成基线（tasks 1.3 修正续办）

## Shared Report

| 字段 | 值 |
| --- | --- |
| task_id | `1.3`（修正续办；不新增 WP） |
| role | provisioner |
| phase | runtime |
| stage | runtime |
| agent_context | provisioner 子 Agent，session `01a0f23d-be87-7494-87de-b93502d6249f`（宿主 run 目录 `…/6e4e44c0-ec57-4fd4-8b09-d62aa94ef1b7/run-0`）；全新任务级最小上下文，未继承主对话实现讨论，等效 `fork_turns="none"` |
| target_revision | `b0a387b17f87a9d15d5b07d20f1e16539566c789`（U1 固定集成基线，分支 `integration/session-resume-du1`） |
| scope | 仅 `D:/Project/acp-remote-wt/session-resume-wp3` 的本地分支 `agentic/session-resume-wp3` 指针重定位；不涉及其它 worktree / 分支 / 资源 |
| changes | NOT_APPLICABLE（本角色不改产品代码、测试脚本或配置文件；未新增/修改任何被跟踪文件内容） |
| checks | NOT_APPLICABLE（本角色不判断证据充分性；RESOURCE 行见 `handoff_index`） |
| issues | 无阻断项；2 条交接注意项见 `## Observations` 的 O-4 / O-5 |
| result | **PASS** |
| evidence_paths | 本报告（`openspec/changes/session-resume/reports/provisioner-repoint-wp3.md`）；前序基线创建证据 `openspec/changes/session-resume/reports/provisioner-worktrees.md`；node_modules 联接证据 `openspec/changes/session-resume/reports/provisioner-node-modules.md` |
| resource_cleanup | 本次未创建任何新资源，故无需清理；wp3 worktree 与 `node_modules` 联接**保留**（WP3 仍需开工）。其它 worktree / 分支经复核未移动（见 C-6） |

---

## 1. Commands（原始命令 + 原始输出）

所有命令均在目标 worktree 内执行（`cd D:/Project/acp-remote-wt/session-resume-wp3`），shell 为 Git Bash。

### C-1 认领与身份确认

```console
$ cd D:/Project/acp-remote-wt/session-resume-wp3 && pwd && git rev-parse --abbrev-ref HEAD
/d/Project/acp-remote-wt/session-resume-wp3
agentic/session-resume-wp3
```

### C-2 前置检查①：工作区必须干净

```console
$ git status --porcelain
--- status ---
--- (end status) ---
$ git rev-parse HEAD
81e350ff340014265eb7c9251237c799d4357fee
```

**判定**：`git status --porcelain` 输出为空 ⇒ 工作区干净（无已跟踪改动、无未跟踪文件）。HEAD = `81e350f…`，与 main 描述的旧基线一致。

### C-3 前置检查②：相对目标基线无独有提交

```console
$ git log --oneline b0a387b17f87a9d15d5b07d20f1e16539566c789..agentic/session-resume-wp3
(无输出)
exit=0
```

**判定**：输出为空 ⇒ 该分支相对 `b0a387b` **无独有提交**，重置不会丢失任何提交工作。与 main 的实测一致。

### C-4 目标提交与拓扑前提核验（重置前）

```console
$ git log --oneline -1 b0a387b17f87a9d15d5b07d20f1e16539566c789
b0a387b chore(repo): 集成 WP2（node-link session.resume 与封闭词表原子点）到 U1 集成基线

$ git log --oneline -5 b0a387b17f87a9d15d5b07d20f1e16539566c789
b0a387b chore(repo): 集成 WP2（node-link session.resume 与封闭词表原子点）到 U1 集成基线
e8feaf9 chore(repo): 集成 WP1（acp-protocol session/resume DTO）到 U1 集成基线
32f71f5 feat(node-link): 加入 session.resume 命令与封闭词表原子点
248d9b9 feat(acp): 新增 session/resume 类型化 DTO 并提升矩阵交付节奏
81e350f chore(node-link): 归档 node-trust-export-ids 变更并同步主规范 (#36)

$ git merge-base --is-ancestor 32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992 b0a387b17f87a9d15d5b07d20f1e16539566c789 ; echo $?
0
$ git merge-base --is-ancestor 248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb b0a387b17f87a9d15d5b07d20f1e16539566c789 ; echo $?
0

$ git rev-parse agentic/session-resume-wp3
81e350ff340014265eb7c9251237c799d4357fee
```

**判定**：`b0a387b` 直接继承 `e8feaf9`（含 WP1 `248d9b9`）与 WP2 `32f71f5`；两条 `--is-ancestor` 均返回 0 ⇒ 目标基线确实包含 WP3 的全部规划上游（`code:WP2`，以及 WP1）。

### C-5 重置前祖先关系（说明为何必须重定向）

```console
$ date -Iseconds
2026-09-30T20:15:33+08:00

$ git merge-base --is-ancestor 32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992 HEAD ; echo $?
1
$ git merge-base --is-ancestor 248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb HEAD ; echo $?
1
```

**判定**：重置前 HEAD（`81e350f`）**不含** WP2/WP1 交付提交 ⇒ 旧起点确实缺少 `code:WP2` 前置，WP3 无法按 plan.md 要求开工。这从环境事实层面复现了 coder-C 的 BLOCKED 成因。

### C-6 资源归属复核（执行前，确认只动目标 worktree）

```console
$ git worktree list
D:/Project/acp-remote                           81e350f [main]
D:/Project/acp-remote-wt/export-ids             cc6faef [agentic/node-trust-export-ids]
D:/Project/acp-remote-wt/export-ids-docs        659e590 [agentic/node-trust-export-ids-docs]
D:/Project/acp-remote-wt/export-ids-integration 64e179c [integration/node-trust-export-ids-du1]
D:/Project/acp-remote-wt/nl-owner-integration   654c0c1 [integration/node-link-owner-du1]
D:/Project/acp-remote-wt/node-link-owner        5f62e77 [agentic/node-link-owner]
D:/Project/acp-remote-wt/session-resume-du1     b0a387b [integration/session-resume-du1]
D:/Project/acp-remote-wt/session-resume-tp1     81e350f [agentic/session-resume-tp1]
D:/Project/acp-remote-wt/session-resume-wp1     248d9b9 [agentic/session-resume-wp1]
D:/Project/acp-remote-wt/session-resume-wp2     32f71f5 [agentic/session-resume-wp2]
D:/Project/acp-remote-wt/session-resume-wp3     81e350f [agentic/session-resume-wp3]
D:/Project/acp-remote-wt/session-resume-wp4     81e350f [agentic/session-resume-wp4]
D:/Project/acp-remote-wt/session-resume-wp5     81e350f [agentic/session-resume-wp5]
D:/Project/acp-remote-wt/session-resume-wp6     81e350f [agentic/session-resume-wp6]
```

**判定**：`session-resume-wp3` 处于 `81e350f` 且为本次唯一改写对象。目标 worktree 当前无并发写入者（C-2 的 `git status` 为空，且 WP3 在 `Dispatch Reconciliation` 中状态为 `unstarted`，coder-C 处于等待态），符合 assignment「此刻 idle」前提。

### C-7 执行重置（**仅在该 worktree 内**）

```console
$ date -Iseconds
2026-09-30T20:15:37+08:00

$ git reset --hard b0a387b17f87a9d15d5b07d20f1e16539566c789
HEAD is now at b0a387b chore(repo): 集成 WP2（node-link session.resume 与封闭词表原子点）到 U1 集成基线
reset_exit=0
```

**命令性质说明**：只移动当前分支指针（`--hard` 同步工作树/索引到该提交）。未使用 `--soft`/`--mixed` 之外的任何选项，**未**附带 `--force` 之外的破坏性手段，**未**执行 push/amend/merge/checkout/stash，**未**触及其它 worktree。

### C-8 复核①：HEAD 等于目标基线

```console
$ git rev-parse HEAD
b0a387b17f87a9d15d5b07d20f1e16539566c789

$ git rev-parse agentic/session-resume-wp3
b0a387b17f87a9d15d5b07d20f1e16539566c789
```

**判定**：PASS — HEAD 与分支引用均为 `b0a387b17f87a9d15d5b07d20f1e16539566c789`（回显 `b0a387b` 为缩写）。

### C-9 复核②：工作区仍干净

```console
$ git status --porcelain
--- (end status) ---

$ git diff --stat b0a387b17f87a9d15d5b07d20f1e16539566c789 --
diff_rc=0
```

**判定**：PASS — `git status --porcelain` 输出为空；`git diff --stat` 对目标基线输出为空（无差异）。工作树内容**逐字节等于** `b0a387b`。

### C-10 复核③④：上游提交已在内（祖先关系为真）

```console
$ git merge-base --is-ancestor 32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992 HEAD ; echo $?
0
$ git merge-base --is-ancestor 248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb HEAD ; echo $?
0
```

**判定**：PASS — WP2 `32f71f5` 与 WP1 `248d9b9` 均为新 HEAD 的祖先（rc=0，真）。

### C-11 上游依赖落地性验证（重定向是否真的解决 BLOCKED）

```console
$ grep -n "session.resume" crates/core/src/broker.rs
130:        "session.resume" => "grant.remote-work",

$ git show 81e350ff340014265eb7c9251237c799d4357fee:crates/core/src/broker.rs | grep -n "session.resume"
(无输出)
old_baseline_grep_rc=1
```

**判定**：新基线的 `crates/core/src/broker.rs:130` 存在 WP2 加入的 `required_grant` 臂；旧基线 `81e350f` 的同文件**不存在**该臂（grep 无匹配）。⇒ 重定向确实把 WP3 所需的 `code:WP2` 前置从「缺失」变为「就位」，而非仅形式化改了指针。

### C-12 无副作用复核：其它分支 / worktree / worktree 登记未被扰动

```console
$ git rev-parse main
81e350ff340014265eb7c9251237c799d4357fee
$ for d in wp4 wp5 wp6 tp1; do printf "%s " "$d"; git -C "D:/Project/acp-remote-wt/session-resume-$d" rev-parse HEAD; done
wp4 81e350ff340014265eb7c9251237c799d4357fee
wp5 81e350ff340014265eb7c9251237c799d4357fee
wp6 81e350ff340014265eb7c9251237c799d4357fee
tp1 81e350ff340014265eb7c9251237c799d4357fee
$ git -C D:/Project/acp-remote-wt/session-resume-du1 rev-parse HEAD
b0a387b17f87a9d15d5b07d20f1e16539566c789
$ git worktree list
（与 C-6 逐行一致，仅 session-resume-wp3 由 81e350f 变为 b0a387b）

$ git rev-parse --abbrev-ref HEAD   # wp3 分支名未变
agentic/session-resume-wp3
```

**判定**：PASS —
`main` 保持 `81e350f…`（**未被本次操作移动**）；
`wp4` / `wp5` / `wp6` / `tp1` 保持 `81e350f…`（**按 assignment 本次不动，实测未动**）；
`du1`（U1 集成基线）保持 `b0a387b…`（未被扰动）；
其余 worktree（export-ids / node-link-owner 等）在 `git worktree list` 中与 C-6 逐行一致；
当前分支名仍为 `agentic/session-resume-wp3`（**未切分支**）。

### C-13 reflog：证明只发生了"指针移动"这一件事

```console
$ git reflog -5 --date=iso
b0a387b HEAD@{2026-09-30 20:15:37 +0800}: reset: moving to b0a387b17f87a9d15d5b07d20f1e16539566c789
81e350f HEAD@{2026-09-30 09:50:04 +0800}: reset: moving to HEAD
81e350f HEAD@{2026-09-30 09:50:04 +0800}: 
```

**判定**：该分支的全部历史事件只有本次一条 `reset: moving to b0a387b…`，以及 09:50 创建 worktree 时的两条。**无** commit / amend / merge / cherry-pick / rebase / checkout 事件 ⇒ 分支上不存在被覆盖或丢弃的独有提交，与 C-3 的空日志互为独立佐证。

### C-14 未被跟踪 / 被忽略资源复核（联接未破坏）

```console
$ ls -ld node_modules
lrwxrwxrwx 1 zhang 197610 34 Sep 30 10:39 node_modules -> /d/Project/acp-remote/node_modules

$ git status --porcelain --ignored=matching | wc -l
1

$ ls -la | head -20
drwxr-xr-x 1 zhang 197610    0 Sep 30 20:15 .
drwxr-xr-x 1 zhang 197610    0 Sep 30 09:50 ..
drwxr-xr-x 1 zhang 197610    0 Sep 30 09:50 .agents
-rw-r--r-- 1 zhang 197610  1230 Sep 30 09:50 .editorconfig
-rw-r--r-- 1 zhang 197610    64 Sep 30 09:50 .git
…
drwxr-xr-x 1 zhang 197610    0 Sep 30 20:15 .pi
-rw-r--r-- 1 zhang 197610 40013 Sep 30 20:15 AGENTS.md
drwxr-xr-x 1 zhang 197610    0 Sep 30 09:50 Cargo.lock
…
-rw-r--r-- 1 zhang 197610    0 Sep 30 10:39 node_modules -> ...
```

**判定**：
`node_modules` 仍为指向 `/d/Project/acp-remote/node_modules` 的目录联接（mtime 10:39 = 创建时刻，**本次未被触碰**；未执行 `npm install/ci/update`，未 `rm -rf` 联接）；
被忽略条目计数为 1（即 `node_modules`），无新增；
未跟踪文件计数为 0，无残留临时文件。

### C-15 重置对工作树内容的实际影响范围（供 main 正确解读）

```console
$ git diff --stat 81e350ff340014265eb7c9251237c799d4357fee b0a387b17f87a9d15d5b07d20f1e16539566c789 | tail -5
 .../command-terminal-session-resume-completed.json |  28 +++
 schemas/node-link/v1/command.schema.json           |  75 ++++++-
 schemas/node-link/v1/common.schema.json            |  10 +
 scripts/check-command-catalog.mjs                  |   5 +-
 25 files changed, 645 insertions(+), 41 deletions(-)

$ git diff --stat b0a387b17f87a9d15d5b07d20f1e16539566c789 --    # 重置后对基线
（空）
```

**判定与说明**：两个基线之间有 25 个文件、+645/−41 行差异，这**正是** WP1/WP2 已被集成的内容；`--hard` 因此按预期把这些上游改动物化进 wp3 的工作树（`AGENTS.md`、`README.md` 等文件 mtime 变为 20:15）。这不是"丢失本地工作"，因为重置前 `git status --porcelain` 为空（C-2）且无独有提交（C-3）：工作树里**没有任何未提交内容**可以丢失。重置后对基线的 `git diff --stat` 为空（C-9），证明工作树内容 == 基线，WP3 的起点是干净且确定的。

---

## 2. Observations

- **O-1 重定向动机经环境事实验证**：旧起点 `81e350f` 的 `crates/core/src/broker.rs` 缺少 WP2 加入的 `"session.resume" => "grant.remote-work"` 臂；新基线 `b0a387b` 在第 130 行具备该臂（C-11）。plan.md §Dependency Handoffs 把 `crates/core/src/broker.rs` 标为 WP2 → WP3 的串行写点，因此该缺臂正是 WP3（`crates/core/`）不可开工的实际成因，而非形式化流程问题。
- **O-2 目标基线的拓扑完整**：`b0a387b` 直接包含 WP1 `248d9b9`（经 `e8feaf9`）与 WP2 `32f71f5`，两者均被 `--is-ancestor` 判为真（C-4 / C-10），满足 plan.md §Dependency Handoffs 第 312 行「WP3 从包含 WP2 交付提交的集成基线开工」。
- **O-3 未提前移动下游（遵循 assignment 的有意克制）**：`wp4`/`wp5`/`wp6`/`tp1` 仍钉在 `81e350f`（C-12）。按依赖表，WP4/WP5 等 WP3、WP6 等全部，故现在把它们挪到 `b0a387b` 会产生"看似就绪"的误导；本次**刻意不动**，留待 main 在其派发前再次派本角色重定向。
- **O-4 交接注意项（交 main）**：`verification.md` 的 `## Worktree Handoff` 现有 WP3 行（Baseline Revision `81e350f…`、Received At `2026-09-30T09:50:17+08:00`）**已不再反映事实**，必须替换为本报告 §3 的新行。同一文档中绑定 `81e350f…` 的 `RESOURCE / NOT_APPLICABLE` 行按 `roles/provisioner.md`「目标版本一移动即失效」的规则，对 WP3 部分应视为失效并新增 `b0a387b…` 绑定行（WP4/WP5/WP6/TP1 仍留在 `81e350f`，其原有基线描述仍成立，但 `du1` 已随 merger 移至 `b0a387b`，需 main 自行界定该行的适用范围）。
- **O-5 环境既有红窗口（事实提示，非本次引入）**：`verification.md` 第 5.1 行记录 U1 基线 `b0a387b` 存在**已登记红窗口** `crates/server/src/node_link/command.rs:2191` `E0004`（`core_payload()` 未覆盖 `CommandPayload::SessionResume`，归属 WP6/W4）。该红窗口属于 `crates/server`，而 WP3 的 PV1 为分支级 `-p core`（plan.md 第 259 行），故 WP3 的 `-p core` 构建不受其影响；仅当 WP3 执行 workspace 级命令时才会遇到。**本角色不判断证据充分性**，此处仅记录环境事实供 main 与 coder-C 参考。
- **O-6 未触碰范围**：本次未 push、未 amend、未 merge、未切分支、未创建新 worktree、未安装依赖、未删除联接、未修改任何被跟踪文件内容、未改动 `openspec/**` 规划文件。报告仅新增一个未跟踪文件（该目录整体未跟踪，见 C-16）。

### C-16 报告落盘不引入被跟踪改动

```console
$ cd D:/Project/acp-remote && git check-ignore -v openspec/changes/session-resume/reports/provisioner-worktrees.md ; echo $?
1
$ git ls-files --error-unmatch openspec/changes/session-resume/reports/provisioner-worktrees.md
error: pathspec '…' did not match any file(s) known to git
$ git status --porcelain | head -20
?? openspec/changes/session-resume/
```

**判定**：`openspec/changes/session-resume/` 整体为未跟踪目录，本报告写入不产生任何被跟踪文件改动，与既有各角色报告的处理方式一致。

---

## 3. Structured Handoff Record — (WP3, Attempt 1) 重新指向

> **本轮为对既有 Attempt=1 基线的重定向**（非 fixing 重开）。WP3 在 tasks/plan 语义下仍是首次开工（`unstarted`），无前次执行事件，故 `attempt` 保持 `1`，仅更正 `baseline_revision` 与 `received_at`。

```yaml
worktree_handoff:
  - work_package: WP3
    attempt: 1
    worktree: "D:/Project/acp-remote-wt/session-resume-wp3"
    branch: "agentic/session-resume-wp3"
    baseline_revision: "b0a387b17f87a9d15d5b07d20f1e16539566c789"
    previous_baseline_revision: "81e350ff340014265eb7c9251237c799d4357fee"
    baseline_source: "分支 integration/session-resume-du1（U1 固定集成基线，merger-A 产出；worktree D:/Project/acp-remote-wt/session-resume-du1）"
    provisioner: "provisioner 子 Agent（session 01a0f23d-be87-7494-87de-b93502d6249f，新上下文，非任何 WP Owner）"
    executor_claimed: "coder-C"
    received_at: "2026-09-30T20:15:39+08:00"
    revision_supersedes: true
    supersedes_received_at: "2026-09-30T09:50:17+08:00"
    change_description: "起点由 81e350f 重新指向 U1 集成基线 b0a387b17f87a9d15d5b07d20f1e16539566c789：该基线含 WP2 交付提交 32f71f5（crates/core/src/broker.rs 的 required_grant 臂）与 WP1 交付提交 248d9b9，满足 plan.md 对 WP3「从包含 WP2 交付提交的集成基线开工」的要求。分支无独有提交、重置前工作区干净，故无任何工作丢失。"
    verifications:
      head_equals_baseline: PASS
      worktree_clean_after: PASS
      ancestor_wp2_32f71f5: PASS
      ancestor_wp1_248d9b9: PASS
      other_worktrees_untouched: PASS
    preconditions_met: true
    evidence: "openspec/changes/session-resume/reports/provisioner-repoint-wp3.md"
    provisioner_note: "本行需由 main 校验后写入 verification.md 的 ## Worktree Handoff，替换现有 WP3 行（原基线 81e350f…、Received At 09:50:17）。"
```

### 结构化交接记录（表格形式，便于 main 直接回填）

| Work Package | Attempt | Worktree | Branch | Baseline Revision | 上一基线 | Provisioner | Executor | Received At | Result | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP3 | 1（重定向） | D:/Project/acp-remote-wt/session-resume-wp3 | agentic/session-resume-wp3 | `b0a387b17f87a9d15d5b07d20f1e16539566c789` | `81e350ff340014265eb7c9251237c799d4357fee` | provisioner 子 Agent（session 01a0f23d…） | coder-C（待开工） | 2026-09-30T20:15:39+08:00 | PASS | reports/provisioner-repoint-wp3.md |

---

## 4. Handoff Index

```yaml
handoff_index:
  - task_id: "1.3"
    work_package: WP3
    role: provisioner
    phase: runtime
    round: NOT_APPLICABLE
    stage: runtime
    target_revision: "b0a387b17f87a9d15d5b07d20f1e16539566c789"
    evidence_type: RESOURCE
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/session-resume/reports/provisioner-repoint-wp3.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在目标 worktree D:/Project/acp-remote-wt/session-resume-wp3 内实测：重置前 git status --porcelain 为空且 b0a387b..分支 日志为空（无独有提交）；git reset --hard 至 b0a387b 后 HEAD/分支引用均等于 b0a387b17f87a9d15d5b07d20f1e16539566c789、git status --porcelain 为空、对基线 git diff --stat 为空、32f71f5 与 248d9b9 均为 HEAD 祖先（rc=0）。wp4/wp5/wp6/tp1 保持 81e350f、main 保持 81e350f、du1 保持 b0a387b，均未被扰动。node_modules 目录联接未被触碰（未 npm ci，未 rm -rf）。RESOURCE 行绑定 b0a387b…；该提交一旦被移动，本行失效、WP3 必须重跑。"
    source_evidence: NOT_APPLICABLE
```

---

## 5. Resource Cleanup

| 资源 | 处置 | 复核 |
| --- | --- | --- |
| worktree `D:/Project/acp-remote-wt/session-resume-wp3` | **保留**（WP3 即将开工） | `git worktree list` 中该行 baseline 已为 `b0a387b`（C-12） |
| 分支 `agentic/session-resume-wp3` | **保留**，指针已更新 | `git rev-parse` = `b0a387b…`（C-8）；分支名未变（C-12） |
| `node_modules` 目录联接（wp3） | **保留**，未触碰 | `ls -ld node_modules` 仍指向 `/d/Project/acp-remote/node_modules`，mtime 10:39（C-14） |
| `CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp3` | **保留** | 本次未构建、未清理 |
| 本次新建的共享资源（端口/容器/数据库/账号） | 无 | 本任务未分配任何共享资源；`openspec/config.yaml` 记录「暂无共享运行资源」 |
| 报告文件 | 新增 1 个未跟踪文件 | `openspec/changes/session-resume/` 整体未跟踪（C-16） |

**结论**：本次操作仅移动一个分支指针；无新资源、无残留、无越界释放他人实例。

---

## 6. Result

**PASS** — WP3 worktree 的起点已从 `81e350f…` 重新指向 U1 固定集成基线 `b0a387b17f87a9d15d5b07d20f1e16539566c789`，分支无独有提交、重置前后工作区均干净、三条拓扑复核项全部为真、其它 worktree / 分支 / 联接 / 主分支均未被扰动。

`runtime` 的 PASS 仅表示约定资源与就绪条件满足；不代表任何产品检查、代码审查或最终验收通过。

### 待 main 处理的两项（不阻断本角色 PASS）

1. 用 §3 的记录替换 `verification.md` `## Worktree Handoff` 的 WP3 行，并据规则更新绑定 `81e350f…` 的 `RESOURCE` 行对 WP3 的适用范围（O-4）。
2. 在 WP4/WP5/WP6/TP1 派发前，按依赖表再次派本角色做各自的重定向；**不建议**提前把它们移到 `b0a387b`（O-3）。
