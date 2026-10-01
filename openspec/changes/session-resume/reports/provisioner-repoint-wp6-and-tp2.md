# Provisioner Report — WP6 worktree 重定向到 U1 集成基线 `5ab7e9d` + TP2 工作区交接补发（登记）

> 本报告只覆盖本次 runtime 资源操作：一次 worktree 起点重定向（WP6）与一次工作区登记（TP2，**不含任何指针移动**）。
> 本角色不判断证据充分性，不参与需求/设计/实现/审查/验收，也不修改任何权威规划文件（`plan.md` / `tasks.md` / `verification.md`）。

## Shared Report

| 字段 | 值 |
| --- | --- |
| task_id | `1.3`（续办 · 第 3 次重定向先例） |
| role | `provisioner` |
| phase | `runtime` |
| agent_context | provisioner 子 Agent（session `01a0f64c-06e0-72e3-b321-b38908673e7f`，parent session `01a0f628-df10-76d3-99e2-df76bf766c9d`；**新任务级最小上下文，不继承主对话**，非任何 WP Owner，`PI_SUBAGENT_CHILD=1`） |
| target_revision | `5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3`（U1 集成基线，含 WP1–WP5 交付提交） |
| scope | 仅 `D:/Project/acp-remote-wt/session-resume-wp6` 的起点重定向 + TP2 的工作区**登记**；不新增工作包、不新增分支、不新增 worktree；**未移动 TP2 的 worktree 指针** |
| changes | NOT_APPLICABLE（本角色不改产品代码、测试脚本或配置文件；本次未产生任何被跟踪文件的内容改动） |
| checks | NOT_APPLICABLE（本角色不判断证据充分性；本次**未运行** `cargo build/test`、`npm run check`，RESOURCE 行见 `handoff_index`） |
| issues | 无阻断项；4 条交接注意项见 `## 2. Observations` 的 O-1 / O-2 / O-3 / O-4 |
| result | **PASS** |
| evidence_paths | 本报告（`openspec/changes/session-resume/reports/provisioner-repoint-wp6-and-tp2.md`）；初始基线创建证据 `openspec/changes/session-resume/reports/provisioner-worktrees.md`；同型先例 `provisioner-repoint-wp3.md`、`provisioner-repoint-wp4-wp5.md`；WP5 并入 U1 的 merger 报告 `merge-u1-integrate-wp5.md` |
| resource_cleanup | 本次未创建任何新资源，故无需清理；WP6 worktree/分支（已重定向）、TP1/TP2 worktree/分支（**未动**）、`node_modules` 联接、各自 `CARGO_TARGET_DIR` 全部**保留**。执行前后复核其它 7 个 worktree、`refs/heads/main`、`integration/session-resume-du1` 均未移动（C-19 / C-20） |

---

## 1. Commands（原始命令 + 原始输出）

shell 为 Git Bash；所有 `git` 命令以 `-C <worktree>` 或 `cd <worktree>` 为工作目录。
**未执行** `cargo build` / `cargo test` / `npm install` / `npm ci` / `npm run check`；**未**对 `node_modules` 联接 `rm -rf`；
**未** push / amend / merge / 切分支 / 创建或删除 worktree。

### A 组：WP6 重定向前的核实（前置条件）

### C-1 目标提交与新基线核实（重置前）

```console
$ git -C D:/Project/acp-remote-wt/session-resume-wp6 rev-parse HEAD
81e350ff340014265eb7c9251237c799d4357fee
$ git -C D:/Project/acp-remote-wt/session-resume-wp6 rev-parse integration/session-resume-du1
5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3
```

**判定**：任务给定的新基线 `5ab7e9d…` 存在，且**等于**集成分支 `integration/session-resume-du1` 的当前尖端，
即 merger 并入 WP5 后的 U1 集成基线（与 `verification.md` 的 `## Handoff Index` 第 48/49 行登记的
「并入 WP5 → `5ab7e9d640034a…`」逐字一致）。WP6 worktree 原停在 `81e350f…`，与 Assignment 描述一致。

### C-2 分支归属确认

```console
$ git -C D:/Project/acp-remote-wt/session-resume-wp6 rev-parse --abbrev-ref HEAD
agentic/session-resume-wp6
```

**判定**：检出的是计划规定的分支 `agentic/session-resume-wp6`，与 Assignment 表格一致。

### C-3 前置检查①：工作区必须干净

```console
$ git -C D:/Project/acp-remote-wt/session-resume-wp6 status --porcelain
（无输出）
$ echo "lines=$(git -C D:/Project/acp-remote-wt/session-resume-wp6 status --porcelain | wc -l)"
lines=0
```

**判定**：`git status --porcelain` 输出为空（0 行）⇒ 无已跟踪改动、无未跟踪文件。**前置条件①满足。**

### C-4 前置检查②：相对新基线无独有提交

```console
$ git -C D:/Project/acp-remote-wt/session-resume-wp6 log --oneline 81e350f..agentic/session-resume-wp6
（无输出）
$ git -C D:/Project/acp-remote-wt/session-resume-wp6 log --oneline 5ab7e9d..agentic/session-resume-wp6
（无输出）
```

**判定**：两个方向均为空 ⇒ 分支相对 `81e350f…` 与相对新基线 `5ab7e9d…` **均无独有提交**，
重置不会丢失任何提交工作（WP6 在 tasks/plan 语义下仍是首次开工，与
`verification.md` 的 Dispatch Reconciliation 把 WP6 记为未开工一致）。**前置条件②满足。**

### C-5 重定向前：`node_modules` 联接与 `CARGO_TARGET_DIR` 现状

```console
$ ls -ld D:/Project/acp-remote-wt/session-resume-wp6/node_modules
lrwxrwxrwx 1 zhang 197610 34 Sep 30 10:39 node_modules -> /d/Project/acp-remote/node_modules
$ ls -ld D:/Project/acp-remote-target/session-resume-wp6
drwxr-xr-x 1 zhang 197610 0 Sep 30 09:50 D:/Project/acp-remote-target/session-resume-wp6
$ git -C D:/Project/acp-remote-wt/session-resume-wp6 status --porcelain --ignored | wc -l
1
```

**判定**：`node_modules` 目录联接存在且指向主仓库（mtime `Sep 30 10:39`，早于本次操作）；
`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp6` 存在（mtime `Sep 30 09:50`，初始创建时刻）。
`--ignored` 清单只有 1 条（即该联接本身）。两者本次**均未被删除、未被清空**。

### C-7 重定向前 reflog（用于对比「只发生指针移动」）

```console
$ git -C D:/Project/acp-remote-wt/session-resume-wp6 reflog -3
81e350f HEAD@{0}: reset: moving to HEAD
81e350f HEAD@{1}:
```

### C-8 执行重置（**仅在 WP6 worktree 内**）

```console
$ git -C D:/Project/acp-remote-wt/session-resume-wp6 reset --hard 5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3
HEAD is now at 5ab7e9d chore(repo): 集成 WP5（agent-host 会话恢复后端路径与 ACP 会话标识暴露）到 U1 集成基线
exit=0
```

**判定**：`git reset --hard` 退出码 0，落在目标提交上。因 C-3 工作区干净、C-4 无独有提交，
本次重置为**纯指针移动**，没有任何未提交或已提交的工作被丢弃。

### C-9 ~ C-13 重定向后复核

```console
$ git -C D:/Project/acp-remote-wt/session-resume-wp6 rev-parse HEAD
5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3
$ echo "lines=$(git -C D:/Project/acp-remote-wt/session-resume-wp6 status --porcelain | wc -l)"
lines=0
$ git -C D:/Project/acp-remote-wt/session-resume-wp6 merge-base --is-ancestor 5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3 HEAD && echo true || echo false
true
$ git -C D:/Project/acp-remote-wt/session-resume-wp6 log -1 --oneline
5ab7e9d chore(repo): 集成 WP5（agent-host 会话恢复后端路径与 ACP 会话标识暴露）到 U1 集成基线
$ git -C D:/Project/acp-remote-wt/session-resume-wp6 reflog -4
5ab7e9d HEAD@{0}: reset: moving to 5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3
81e350f HEAD@{1}: reset: moving to HEAD
81e350f HEAD@{2}:
```

**判定**：Assignment 要求的四项复核全部为真——
① `git rev-parse HEAD` == `5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3`；
② 工作区仍干净（0 行）；
③ `git merge-base --is-ancestor 5ab7e9d HEAD` 为**真**；
④ 顶部提交即 U1 集成基线并入 WP5 的合并提交。
reflog 相对 C-7 **只增加一条** `reset: moving to 5ab7e9d…`，无 commit / amend / rebase / checkout / merge 条目
⇒ 本次操作确为纯指针移动，与「repoint 而非重开」的语义一致。

### C-14 抽查①：`crates/core/src/broker.rs` 确有 WP3 落地的 `session.resume` 臂

```console
$ grep -n "session.resume\|resume_session\|settle_session_resume\|ResumeSessionRequest" \
    D:/Project/acp-remote-wt/session-resume-wp6/crates/core/src/broker.rs | head -20
37:    PromptRequest, PublicError, RemoteSessionRef, RequestId, Resolution, ResumeSessionRequest,
131:        "session.resume" => "grant.remote-work",
1474:    /// `session.resume`（§5.1、§12.7）：进程不在的 owned 会话的恢复入口。
1481:    /// 工作）：`session.resume` 没有 `CommandPayload` 变体，也不走通用 mutation 管线（否则终态会被
1482:    /// 通用臂先提交，`settle_session_resume` 就退化为幂等 no-op）。终态由适配层用同源映射投影
1484:    /// [`Broker::settle_session_resume`] 提交。
1489:    pub async fn resume_session(
1496:        self.authorize(actor, "session.resume", Some(session), request)
1519:            command: "session.resume".to_owned(),
1632:    /// `session.resume` 的终态提交（§5.1、§12.7）。返回本次是否真的写入了终态。
1634:    /// 与 [`Broker::settle_session_create`] **同形**，但**只**终结 `command == "session.resume"` 的
1642:    pub async fn settle_session_resume(
1656:        if record.command() != "session.resume" {
1658:                "settle_session_resume 只能终结 session.resume 的持久记录（§5.1）",
1666:                "session.resume 的持久记录缺少目标会话（§5.1）",
3613:fn resume_request(record: SessionRecoveryRecord) -> Result<ResumeSessionRequest, PortError> {
3619:    ResumeSessionRequest::try_new(record.agent, agent_session_id, workspace_cwd)
3947:        pub(crate) resume_requests: Mutex<Vec<ResumeSessionRequest>>,
4118:        /// 直接写一条命令行（测试用来构造「已 accepted 的 `session.resume` 记录」）。
5682:            request: ResumeSessionRequest,
```

**判定**：WP3 的落地物在起点内可见——`grant` 表有 `"session.resume" => "grant.remote-work"` 臂、
`Broker::resume_session` 存在（:1489）、`Broker::settle_session_resume` 存在（:1642，且带
「只能终结 `session.resume`」的守卫 :1656）、`ResumeSessionRequest` 值对象已 import（:37）。
**抽查①通过。**

### C-15 抽查②：`crates/agent-host/` 确有 WP5 落地的 `--dump-request-params`

```console
$ grep -rn "dump-request-params" \
    D:/Project/acp-remote-wt/session-resume-wp6/crates/agent-host/src/bin/acpr-fake-acp-agent.rs | head -5
17://! `--dump-request-params <path>` 是与它**并列**的独立选项：每次收到带 `method` 的入站报文，向该文件
81:                ("--dump-request-params", Some(value)) => args.dump_request_params = Some(value),
202:/// `--dump-request-params <path>`：追加一行单行 JSON `{"method":…,"params":…}`。
892:        let args = args_from(&["--dump-request-params", &path.to_string_lossy()]);
931:            "--dump-request-params",
```

**判定**：WP5 交付的并列选项存在（选项登记 :81、行为注释 :202、至少两处测试落点 :892/:931），
且注释逐字写明「与它**并列**的独立选项」——与 `verification.md` 登记的 CR5-F1 结论（并列方案 (a)、
不改 `--dump-requests` 语义）一致。**抽查②通过。**

### C-16 拓扑前提：五个上游交付提交均为新基线祖先

```console
$ for c in 248d9b9 32f71f5 4f7a2355 4a3882d 1376e1b 81e350f; do
    git -C D:/Project/acp-remote-wt/session-resume-wp6 merge-base --is-ancestor $c HEAD && echo "$c -> ancestor" || echo "$c -> NOT ancestor"; done
248d9b9 -> ancestor
32f71f5 -> ancestor
4f7a2355 -> ancestor
4a3882d -> ancestor
1376e1b -> ancestor
81e350f -> ancestor
```

**判定**：WP1 `248d9b9`、WP2 `32f71f5`、WP3 `4f7a2355`、WP4 `4a3882d`、WP5 `1376e1b`
（及原始 base `81e350f`）**全部**为新基线的祖先 ⇒ 满足 `plan.md` 的 `## Dependency Handoffs` 对 WP6 的要求
「从包含五者交付提交的集成基线开工」（依赖 `code:WP1…WP5`）。
（各提交对应哪个工作包，依据 `verification.md` 的 `## Handoff Index`；本角色只确认拓扑，不复核合并内容。）

### C-17 内容级复核：工作树与索引相对新基线零差异

```console
$ git -C D:/Project/acp-remote-wt/session-resume-wp6 diff --stat 5ab7e9d
（无输出, exit=0）
```

**判定**：工作树内容 = `5ab7e9d…` 的完整检出，无本地改动残留。

### C-18 联接与构建缓存未被破坏（重定向后）

```console
$ ls -ld D:/Project/acp-remote-wt/session-resume-wp6/node_modules
lrwxrwxrwx 1 zhang 197610 34 Sep 30 10:39 node_modules -> /d/Project/acp-remote/node_modules
$ ls -ld D:/Project/acp-remote-target/session-resume-wp6
drwxr-xr-x 1 zhang 197610 0 Sep 30 09:50 D:/Project/acp-remote-target/session-resume-wp6
```

**判定**：两者与 C-5 完全一致（mtime 仍为 `Sep 30 10:39` / `Sep 30 09:50`，均早于本次操作）⇒ 未被触碰、未被删除或清空。

### C-19 ~ C-21 无副作用复核：其它 worktree / 分支 / 主分支未被扰动

```console
$ git worktree list
D:/Project/acp-remote                           81e350f [main]
D:/Project/acp-remote-wt/export-ids             cc6faef [agentic/node-trust-export-ids]
D:/Project/acp-remote-wt/export-ids-docs        659e590 [agentic/node-trust-export-ids-docs]
D:/Project/acp-remote-wt/export-ids-integration 64e179c [integration/node-trust-export-ids-du1]
D:/Project/acp-remote-wt/nl-owner-integration   654c0c1 [integration/node-link-owner-du1]
D:/Project/acp-remote-wt/node-link-owner        5f62e77 [agentic/node-link-owner]
D:/Project/acp-remote-wt/session-resume-du1     5ab7e9d [integration/session-resume-du1]
D:/Project/acp-remote-wt/session-resume-tp1     81e350f [agentic/session-resume-tp1]
D:/Project/acp-remote-wt/session-resume-wp1     248d9b9 [agentic/session-resume-wp1]
D:/Project/acp-remote-wt/session-resume-wp2     32f71f5 [agentic/session-resume-wp2]
D:/Project/acp-remote-wt/session-resume-wp3     4f7a235 [agentic/session-resume-wp3]
D:/Project/acp-remote-wt/session-resume-wp4     4a3882d [agentic/session-resume-wp4]
D:/Project/acp-remote-wt/session-resume-wp5     1376e1b [agentic/session-resume-wp5]
D:/Project/acp-remote-wt/session-resume-wp6     5ab7e9d [agentic/session-resume-wp6]

$ git rev-parse <refs>
refs/heads/main                                      81e350ff340014265eb7c9251237c799d4357fee
refs/heads/agentic/session-resume-wp1                248d9b9fa69a1e6375a3b9708fdaa2a127dc7ccb
refs/heads/agentic/session-resume-wp2                32f71f5d5f08145cfd3e3bddd7d6f6a46a0a5992
refs/heads/agentic/session-resume-wp3                4f7a23554f76915cf5f29cc23e292ce270e014c9
refs/heads/agentic/session-resume-wp4                4a3882dfb3bf95d76340e35332aa9a8dc491a2d9
refs/heads/agentic/session-resume-wp5                1376e1b5c5bbd66d022a447e92a3179e50f173fd
refs/heads/agentic/session-resume-wp6                5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3
refs/heads/agentic/session-resume-tp1                81e350ff340014265eb7c9251237c799d4357fee
refs/heads/integration/session-resume-du1            5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3

$ git -C D:/Project/acp-remote status --porcelain
?? openspec/changes/session-resume/
```

**判定**：
- `main` 仍为 `81e350f…`（**未动**，符合 Assignment 硬边界）；
- `session-resume-wp5` 仍 `1376e1b`、`session-resume-du1` 仍 `5ab7e9d`（merger 独占，本角色只读未写）；
- `tp1` 仍 `81e350f…`（TP2 只登记、不重定向）；
- 其它变更目录（`export-ids*`、`nl-owner-*`、`node-link-owner`）的 worktree 注册与分支尖端均与 C-1 前一致；
- worktree 集合未新增、未移除、未移动任何其它 worktree；
- 主仓库工作区只有 `openspec/changes/session-resume/` 一个未跟踪目录（本报告写入其中，
  不产生任何被跟踪文件改动——`git ls-files --error-unmatch` 对该目录下的 `plan.md` 无法匹配，见 C-25）。

### C-22 ~ C-24 B 组：TP2 工作区（**只核实与登记，未做任何指针移动**）

```console
$ git -C D:/Project/acp-remote-wt/session-resume-tp1 rev-parse --abbrev-ref HEAD
agentic/session-resume-tp1
$ git -C D:/Project/acp-remote-wt/session-resume-tp1 rev-parse HEAD
81e350ff340014265eb7c9251237c799d4357fee
$ echo "lines=$(git -C D:/Project/acp-remote-wt/session-resume-tp1 status --porcelain | wc -l)"
lines=0
$ git -C D:/Project/acp-remote-wt/session-resume-tp1 log --oneline 81e350f..agentic/session-resume-tp1
（无输出）
$ ls -ld D:/Project/acp-remote-wt/session-resume-tp1/node_modules
lrwxrwxrwx 1 zhang 197610 34 Sep 30 10:39 node_modules -> /d/Project/acp-remote/node_modules
$ ls -ld D:/Project/acp-remote-target/session-resume-tp1
drwxr-xr-x 1 zhang 197610 0 Sep 30 09:50 D:/Project/acp-remote-target/session-resume-tp1
$ git -C D:/Project/acp-remote-wt/session-resume-tp1 reflog -3
81e350f HEAD@{0}: reset: moving to HEAD
81e350f HEAD@{1}:
```

**判定**：
- TP1 **确无代码提交**（`81e350f..agentic/session-resume-tp1` 无输出），只交付了设计报告
  `reports/tp1-test-design.md`；因此该 worktree 仍停在原始起点 `81e350f…` 且工作区干净，与 Assignment 描述一致。
- reflog 只有初始创建条目，**本次未对该 worktree 执行任何写操作**（C-19 中该行亦仍为 `81e350f`）。
- 该 worktree 现在同时服务 TP2（`plan.md` 的工作包表 TP2 行登记 worktree 为 `wt/tp1`，
  W5 波次，与 TP1/W1 不并发；`## Shared File Ownership` 中 TP2 为 Merge Owner、Order 为各上游 → TP2）。
- **实测起点 = `81e350ff340014265eb7c9251237c799d4357fee`**，但这不是 TP2 的计划起点：
  `plan.md` 的 `## Dependency Handoffs` 要求 TP2「从包含六者（WP1–WP6）交付提交的集成基线开工」，
  而 WP6 **尚未**并入 U1（U1 当前 tip `5ab7e9d` 只含 WP1–WP5）⇒ TP2 开工前必须由 provisioner **再次重定向**。

### C-25 报告落盘不引入被跟踪改动

```console
$ git -C D:/Project/acp-remote ls-files --error-unmatch openspec/changes/session-resume/plan.md
error: pathspec 'openspec/changes/session-resume/plan.md' did not match any file(s) known to git
```

**判定**：`openspec/changes/session-resume/` 整体为未跟踪目录，本报告写入不产生任何被跟踪文件改动，
与既有各角色报告（`provisioner-worktrees.md`、`provisioner-repoint-wp3.md`、`provisioner-repoint-wp4-wp5.md`）一致。

### C-26 硬边界自证：本次未执行构建/测试/安装类命令

```console
# 本次会话在两个 worktree 内执行的命令类别仅有：
#   git rev-parse / status --porcelain / log / reset --hard / reflog / merge-base --is-ancestor /
#   diff --stat / worktree list / rev-parse <refs> / ls-files
#   grep（只读抽查）、ls -ld（只读）、date、env
# 未执行：cargo build / cargo test / cargo clippy / cargo fmt / npm install / npm ci /
#         npm update / npm run check
```

**判定**：符合 Assignment 的硬边界（本角色不是检查执行者；预热构建会白烧时间并可能触发 husky 钩子）。

---

## 2. Observations

- **O-1（WP6 交接语义）**：本次为对既有 `(WP6, Attempt=1)` 基线的**重定向**，不是 fixing 重开，也不是新 attempt。
  WP6 在 tasks/plan 语义下仍是首次开工、无前次执行事件，故 `attempt` 保持 `1`，仅更正 `baseline_revision` 与 `received_at`。
  **请主 Agent 校验后据此替换 `## Worktree Handoff` 的 WP6 行**（现为 `81e350f…` / `Received At 2026-09-30T09:50:17+08:00` /
  evidence `reports/provisioner-worktrees.md`）。本角色不直接改 `verification.md`。
- **O-2（WP6 执行者字段）**：`plan.md` 的工作包表为 WP6 指派 `coder-F`（reviewer-F），worktree `wt/wp6`。
  WP6 **尚未在 `dispatch-queue.jsonl` 开工**，因此交接记录里的执行者是**计划指派值、认领待主 Agent 派发**，
  不是已认领事实。`received_at`（15:09:53+08:00）早于未来首次 coding 事件，满足角色契约。
- **O-3（TP2 起点不可提前固定）**：TP2 的上游是 WP1–WP6（W5 波次），而 U1 当前 tip `5ab7e9d` **只含 WP1–WP5**。
  因此本次**故意不**把 TP2 的起点定为 `5ab7e9d`：那会制造「看似就绪」的误导，并让 TP2 在缺少 WP6
  （`crates/server/` 收口 + `core_payload` 的 `SessionResume` 臂 + 四个替身新方法）的情况下开工，
  其 PV1（workspace 全量）按 `plan.md` 的「预期红窗口」判定必然不绿。
  **TP2 开工前置条件**：WP6 交付并经独立 review 后由 merger 并入 U1 → provisioner 再次把 `wt/tp1` 重定向到
  新的 U1 tip → 届时本行 `Baseline Revision` 需再次修订（O-5 同样适用）。
  另注：`verification.md` 登记 TP1 的 CR7 判定为 **FAIL**（`cr7-review.md`，2×MAJOR），TP1 的修复轮尚未完成；
  TP2 开工前该线程须先结案，否则 TP2 的验收链会带着未闭合的 CR7。本角色只如实记录，不裁决。
- **O-4（资源版本绑定 / 本次未分配新资源）**：WP6 的 `RESOURCE` 行绑定目标版本 `5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3`；
  一旦 `integration/session-resume-du1` 再被 merger 推进（例如并入 WP6、形成 TP2 的开工基线），该行即失效，
  受影响的下游（TP2）必须由 provisioner 重新核对/重定向后重跑。
  本次未请求端口、容器、数据库、账号或缓存目录：`plan.md` 的 `## Runtime Resources` 判定 session-resume 范围
  「暂无共享运行资源」（SQLite 各用临时库文件、loopback 用 `127.0.0.1:0`、合同门禁脚本无状态），
  故无需分配，也不存在独占仲裁。
- **O-5（隔离与边界）**：全程未 push / amend / merge / 切分支 / 创建或删除 worktree / 改动任何被跟踪文件 /
  执行 `npm install|ci|update` / 对 `node_modules` 联接 `rm -rf` / 清理任何 `CARGO_TARGET_DIR`；
  未连接、重置或清理任何其他执行者的实例或数据。唯一被移动的指针是 `refs/heads/agentic/session-resume-wp6`。
- **O-6（WP6 的已知证据缺口，登记供派发提示用）**：`verification.md` 第 49 行登记 U1 第三轮的证据缺口仍未关闭
  ——`crates/app` 从未被成功编译（被 `server` 的失配遮蔽）。该缺口的关闭条件已写进 WP6 派发提示。
  本角色只如实记录，不判断证据充分性，也不预判 WP6 是否能关闭它。

---

## 3. Structured Handoff Record — (WP6, Attempt 1) 重定向 + (TP2, Attempt 1) 登记

> WP6 为对既有 Attempt=1 基线的重定向（非 fixing 重开），`attempt` 保持 `1`。
> TP2 为首次登记（此前 `## Worktree Handoff` 的 TP2 行为「尚未登记」），`attempt` = `1`。
> 两条记录的 `Received At` 均早于各自未来首次执行事件，满足角色契约。

```yaml
worktree_handoff:
  - work_package: WP6
    attempt: 1
    record_kind: "baseline_repoint（对既有 Attempt=1 记录的修订，非新 attempt）"
    worktree: "D:/Project/acp-remote-wt/session-resume-wp6"
    branch: "agentic/session-resume-wp6"
    baseline_revision: "5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3"
    previous_baseline_revision: "81e350ff340014265eb7c9251237c799d4357fee"
    baseline_source: "分支 integration/session-resume-du1（U1 集成基线，含 WP1 248d9b9、WP2 32f71f5、WP3 4f7a2355、WP4 4a3882d、WP5 1376e1b；worktree D:/Project/acp-remote-wt/session-resume-du1，merger 产出，报告 reports/merge-u1-integrate-wp5.md）"
    cargo_target_dir: "D:/Project/acp-remote-target/session-resume-wp6（保留，未清理、未删除）"
    provisioner: "provisioner 子 Agent（session 01a0f64c-06e0-72e3-b321-b38908673e7f，parent 01a0f628-df10-76d3-99e2-df76bf766c9d，新任务级最小上下文，非任何 WP Owner）"
    executor_claimed: "coder-F（plan.md 工作包表指派；尚未在 dispatch-queue.jsonl 开工，认领待主 Agent 派发）"
    reviewer_assigned: "reviewer-F（plan.md 指派，未派发）"
    received_at: "2026-10-01T15:09:53+08:00"
    revision_supersedes: true
    supersedes_received_at: "2026-09-30T09:50:17+08:00"
    change_description: "起点由 81e350f 重新指向并入 WP5 的 U1 集成基线 5ab7e9d。该基线经实测含 WP1–WP5 五个交付提交（merge-base --is-ancestor 六项全真，含原始 base 81e350f），满足 plan.md 对 WP6「从包含五者交付提交的集成基线开工」（依赖 code:WP1…WP5）的要求。分支相对新基线无独有提交、重置前工作区干净，故无任何工作丢失。"
    verifications:
      worktree_clean_before: PASS
      no_branch_only_commits_before: PASS
      head_equals_baseline: PASS
      worktree_clean_after: PASS
      baseline_is_ancestor_of_head: PASS
      spotcheck_wp3_session_resume_arm_in_broker_rs: PASS
      spotcheck_wp5_dump_request_params_in_agent_host: PASS
      all_five_upstream_deliveries_are_ancestors: PASS
      diff_vs_baseline_empty: PASS
      node_modules_link_intact: PASS
      cargo_target_dir_preserved: PASS
      other_worktrees_untouched: PASS
      main_untouched: PASS
    preconditions_met: true
    blocked: false
    evidence: "openspec/changes/session-resume/reports/provisioner-repoint-wp6-and-tp2.md"
    provisioner_note: "本行需由主 Agent 校验后写入 verification.md 的 ## Worktree Handoff，替换现有 WP6 行（原基线 81e350f…、Received At 2026-09-30T09:50:17+08:00、evidence reports/provisioner-worktrees.md）。"

  - work_package: TP2
    attempt: 1
    record_kind: "首次登记（registration only；本轮未移动该 worktree 指针）"
    worktree: "D:/Project/acp-remote-wt/session-resume-tp1"
    branch: "agentic/session-resume-tp1"
    baseline_revision_measured: "81e350ff340014265eb7c9251237c799d4357fee"
    baseline_revision_is_final_for_tp2: false
    baseline_planned_requirement: "plan.md 的 ## Dependency Handoffs（TP2 行）：从包含 WP1–WP6 六者交付提交的集成基线开工；上游变更 → TP2 重跑 PV1 与受影响用例。U1 当前 tip 为 5ab7e9d（仅含 WP1–WP5），WP6 尚未并入，故 TP2 不得在当前基线上开工。"
    planned_repoint_required: true
    planned_repoint_trigger: "WP6 交付提交经独立 review PASS 并由 merger 并入 integration/session-resume-du1 之后，由 provisioner 再次把本 worktree 重定向到新的 U1 tip，届时本记录需再次修订 Baseline Revision。"
    worktree_reuse_note: "plan.md 工作包表登记 TP2 的 worktree 为 wt/tp1，与 TP1 复用同一 worktree；两包分处 W1/W5 波次，不并发（## Shared File Ownership 中 TP2 为 Merge Owner，Order 为各上游 → TP2，Re-verify 为 TP2: PV1）。TP1 当前只交付了设计报告 reports/tp1-test-design.md，无代码提交，故该 worktree 仍停在原始起点 81e350f 且干净。"
    cargo_target_dir: "D:/Project/acp-remote-target/session-resume-tp1（保留，未清理、未删除）"
    resource_acquisition: "预分配（初次 provisioner run 4c0fe451… 于 2026-09-30T09:50:17+08:00 建立，报告 reports/provisioner-worktrees.md）；node_modules 目录联接由 provisioner run cd8a1f6d… 建立（报告 reports/provisioner-node-modules.md），仍指向 /d/Project/acp-remote/node_modules。"
    resource_release: "核实 worktree 无未提交改动 → git worktree remove D:/Project/acp-remote-wt/session-resume-tp1 → （合入后）git branch -d agentic/session-resume-tp1 → rm -rf D:/Project/acp-remote-target/session-resume-tp1 → git worktree list 复核。注意：因 TP1 与 TP2 复用同一 worktree/分支，回收必须在 TP1 修复轮与 TP2 均结案之后执行。"
    provisioner: "provisioner 子 Agent（session 01a0f64c-06e0-72e3-b321-b38908673e7f，parent 01a0f628-df10-76d3-99e2-df76bf766c9d，新任务级最小上下文，非任何 WP Owner、亦非 TP1/TP2 的 tester）"
    executor_claimed: "tester-A（plan.md 工作包表为 TP2 指派 tester-A / reviewer-T2；尚未在 dispatch-queue.jsonl 开工，认领待主 Agent 派发）"
    reviewer_assigned: "reviewer-T2（plan.md 指派，未派发）"
    received_at: "2026-10-01T15:10:08+08:00"
    change_description: "补发 verification.md 的 ## Worktree Handoff 中 TP2 行缺失的结构化交接记录。本轮只做登记与实测，未对该 worktree 执行 reset / 切分支 / 任何写操作（reflog 与 worktree list 双重留证）。"
    verifications:
      worktree_exists: PASS
      branch_matches_plan: PASS
      worktree_clean: PASS
      no_tp1_code_commits_as_stated: PASS
      pointer_unchanged_this_round: PASS
      node_modules_link_intact: PASS
      cargo_target_dir_preserved: PASS
    preconditions_met: true
    registration_only: true
    ready_to_work: false
    ready_to_work_reason: "起点尚未指向含 WP6 的集成基线；且 verification.md 登记的 TP1 CR7 判定仍为 FAIL（cr7-review.md，2×MAJOR），TP1 修复轮未结案。两者均非本角色裁决范围。"
    blocked: false
    evidence: "openspec/changes/session-resume/reports/provisioner-repoint-wp6-and-tp2.md"
    provisioner_note: "本行需由主 Agent 校验后写入 verification.md 的 ## Worktree Handoff，替换现有标注为「尚未登记」的 TP2 行。回填时 Baseline Revision 栏请写明「实测 81e350f…（非最终），须在 WP6 并入后由 provisioner 再次重定向」。"
```

### 结构化交接记录（表格形式，便于主 Agent 直接回填）

| Work Package | Attempt | Worktree 绝对路径 | 分支 | Baseline Revision（实测） | 上一基线 | Provisioner | Executor | Received At | 是否 BLOCKED | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP6 | 1（重定向） | `D:/Project/acp-remote-wt/session-resume-wp6` | `agentic/session-resume-wp6` | **`5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3`** | `81e350ff340014265eb7c9251237c799d4357fee` | provisioner 子 Agent（session `01a0f64c-…`） | coder-F（待认领） | 2026-10-01T15:09:53+08:00 | 否（PASS） | `reports/provisioner-repoint-wp6-and-tp2.md` |
| TP2 | 1（仅登记） | `D:/Project/acp-remote-wt/session-resume-tp1`（与 TP1 复用；两包不并发） | `agentic/session-resume-tp1` | `81e350ff340014265eb7c9251237c799d4357fee`（**实测值；非 TP2 最终起点**，计划要求为 WP6 并入后的 U1 tip，届时须再次重定向） | 无（首次登记） | provisioner 子 Agent（session `01a0f64c-…`） | tester-A（待认领） | 2026-10-01T15:10:08+08:00 | 否（登记成功，但 **not ready to work**） | `reports/provisioner-repoint-wp6-and-tp2.md` |

---

## 4. Handoff Index

```yaml
handoff_index:
  - task_id: "1.3"
    work_package: WP6
    role: provisioner
    phase: runtime
    round: NOT_APPLICABLE
    stage: runtime
    target_revision: "5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3"
    evidence_type: RESOURCE
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/session-resume/reports/provisioner-repoint-wp6-and-tp2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本次在 worktree D:/Project/acp-remote-wt/session-resume-wp6 内实测：重置前 git status --porcelain 为 0 行且 81e350f..agentic/session-resume-wp6 与 5ab7e9d..agentic/session-resume-wp6 均无输出（无独有提交）；git reset --hard 至 5ab7e9d（exit 0）后 git rev-parse HEAD == 5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3、status 仍 0 行、merge-base --is-ancestor 5ab7e9d HEAD 为真、git diff --stat 5ab7e9d 为空、reflog 仅新增一条 reset 条目。内容抽查：crates/core/src/broker.rs 命中 session.resume 的 grant 臂(:131)、Broker::resume_session(:1489)、Broker::settle_session_resume(:1642)；crates/agent-host/src/bin/acpr-fake-acp-agent.rs 命中 --dump-request-params(:81/:202/:892/:931)。拓扑：WP1 248d9b9 / WP2 32f71f5 / WP3 4f7a2355 / WP4 4a3882d / WP5 1376e1b / base 81e350f 六项 --is-ancestor 全真。其它 worktree/分支未扰动（wp5 1376e1b、tp1 81e350f、du1 5ab7e9d、main 81e350f）。node_modules 联接与 CARGO_TARGET_DIR 均保留（未 npm ci，未 rm -rf，未构建未测试）。RESOURCE 行绑定 5ab7e9d…；该提交一旦被移动，本行失效、WP6 必须重跑。"
    source_evidence: NOT_APPLICABLE
  - task_id: "1.3"
    work_package: TP2
    role: provisioner
    phase: runtime
    round: NOT_APPLICABLE
    stage: runtime
    target_revision: "81e350ff340014265eb7c9251237c799d4357fee（实测起点，非最终；计划目标为并入 WP6 后的 U1 tip）"
    evidence_type: RESOURCE
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/session-resume/reports/provisioner-repoint-wp6-and-tp2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "登记型证据（registration only，本轮无写操作）：worktree D:/Project/acp-remote-wt/session-resume-tp1 存在、检出 agentic/session-resume-tp1、git status --porcelain 为 0 行、git rev-parse HEAD == 81e350ff340014265eb7c9251237c799d4357fee、81e350f..agentic/session-resume-tp1 无输出（TP1 确无代码提交）、reflog 仅有初始创建条目且本轮未新增、worktree list 中该行仍为 81e350f（证明指针未被本轮移动）。node_modules 联接仍指向 /d/Project/acp-remote/node_modules（mtime Sep 30 10:39），CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-tp1 保留（mtime Sep 30 09:50）。本记录不表示 TP2 可开工：按 plan.md 的 ## Dependency Handoffs，TP2 须从包含 WP1–WP6 六者交付提交的集成基线开工，而 U1 当前 tip 5ab7e9d 只含 WP1–WP5；WP6 并入后本行必须由 provisioner 再次修订。RESOURCE 行绑定 81e350f…（实测起点），该值一旦被重定向，本行失效、TP2 必须重跑。"
    source_evidence: NOT_APPLICABLE
```

---

## 5. Resource Cleanup

| 资源 | 处置 | 复核 |
| --- | --- | --- |
| worktree `D:/Project/acp-remote-wt/session-resume-wp6` | **保留**（WP6 待开工） | `git worktree list` 中该行 baseline 已为 `5ab7e9d`（C-19） |
| 分支 `agentic/session-resume-wp6` | **保留**，指针已更新至 `5ab7e9d…` | C-20；分支名未变（C-2）；reflog 仅一条新 reset（C-13） |
| worktree `D:/Project/acp-remote-wt/session-resume-tp1`（TP1 兼 TP2） | **保留，未触碰** | C-22 / C-19：该行仍为 `81e350f`，reflog 无新增 |
| 分支 `agentic/session-resume-tp1` | **保留，未触碰** | C-20：`refs/heads/agentic/session-resume-tp1` = `81e350f…` |
| 其它 5 个 worktree（`session-resume-wp1`–`wp5`、`session-resume-du1`） | **保留，未触碰** | C-19 / C-20：尖端与重定向前逐字一致 |
| 其它变更目录 worktree（`export-ids*`、`nl-owner-*`、`node-link-owner`） | **保留，未触碰** | C-19：worktree 注册与分支尖端无变化 |
| `refs/heads/main` | **保留，未触碰** | C-20：仍为 `81e350f…` |
| `node_modules` 目录联接（wp6 / tp1） | **保留，未触碰** | C-5 / C-18 / C-22：仍指向 `/d/Project/acp-remote/node_modules`，mtime 仍为 `Sep 30 10:39` |
| `CARGO_TARGET_DIR`（wp6 / tp1，本次涉及的两个） | **保留，未删除、未清空** | C-18 / C-22：mtime 仍为 `Sep 30 09:50` |
| 其余 6 个 `CARGO_TARGET_DIR` | **保留，未删除、未清空** | 本次未访问其内容（除 C-19 的目录存在性列举） |
| 本次新建的共享资源（端口/容器/数据库/账号/缓存） | 无 | O-4：plan.md 判定本范围无共享运行资源 |
| 报告文件 | 新增 1 个未跟踪文件 | C-25：`openspec/changes/session-resume/` 整体未跟踪 |

**结论**：本次仅移动一条分支指针（`refs/heads/agentic/session-resume-wp6`）并新增一份未跟踪报告；
无新资源、无残留、无越界释放他人实例、未清理任何 target 目录。

---

## 6. 本次**未**做的事（明确清单）

1. **未**运行 `cargo build` / `cargo test` / `cargo clippy` / `cargo fmt`（预热构建会白烧时间，且可能触发 husky 钩子；本角色不是检查执行者）。
2. **未**运行 `npm run check` / `npm install` / `npm ci` / `npm update` / 任何 npm 脚本。
3. **未**对任何 `node_modules` 目录联接执行 `rm -rf`（两者仍指向主仓库，mtime 未变）。
4. **未**删除、未清空 `D:/Project/acp-remote-target/session-resume-wp6`（及任何其它 `CARGO_TARGET_DIR`）——仅确认其存在且可用。
5. **未**触碰其它 7 个 worktree（含 `session-resume-wp5`、`session-resume-du1`、`session-resume-tp1`）：未 reset、未切分支、未清理、未创建、未删除。
6. **未**移动 `refs/heads/main`（仍为 `81e350f…`），**未**移动 `integration/session-resume-du1`（merger 独占，本角色只读）。
7. **未**重定向 TP2 的 worktree 指针——本轮 TP2 只做登记；这是有意的（O-3），避免制造「看似就绪」的误导。
8. **未**修改 `plan.md` / `tasks.md` / `verification.md`（回填由主 Agent 做）。
9. **未**修改任何 `crates/**` 文件，**未**提交任何产品代码，**未**改动任何被跟踪文件。
10. **未** push / amend / merge / rebase / 切分支 / 创建或删除分支或 worktree / `git gc` / `git prune`。
11. **未**裁决 TP1 的 CR7（FAIL）是否阻塞 TP2——如实记录，不越界判断证据充分性（O-3）。

---

## 7. Result

**PASS** — 两件事均已完成：

- **(A) WP6 重定向**：worktree `D:/Project/acp-remote-wt/session-resume-wp6`（分支 `agentic/session-resume-wp6`）
  的起点已从 `81e350f…` 重新指向并入 WP5 的 U1 集成基线 **`5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3`**。
  重定向前工作区干净（0 行）且分支相对新基线无独有提交；重定向后 HEAD 等于基线、`--is-ancestor` 为真、
  工作区仍干净、内容与基线零差异；两项内容抽查（WP3 的 `session.resume` 臂、WP5 的 `--dump-request-params`）命中；
  五个上游交付提交全部为祖先；其它 worktree / 分支 / 联接 / 构建缓存 / 主分支均未被扰动。
  **未 BLOCKED。**
- **(B) TP2 登记**：已返回 `(TP2, Attempt=1)` 结构化交接记录（worktree `D:/Project/acp-remote-wt/session-resume-tp1`、
  分支 `agentic/session-resume-tp1`、`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-tp1`、
  实测起点 `81e350f…`、资源取得方式、释放方法），实测确认 TP1 确无代码提交、该 worktree 干净且指针未被本轮移动。
  记录如实标注：**TP2 的最终起点尚未确定**，须在 WP6 并入 U1 后由 provisioner 再次重定向。
  **未 BLOCKED**（登记本身成功；但 TP2 处于 not-ready-to-work，理由已记录）。

### 待主 Agent 处理（不阻断本角色 PASS）

1. 校验本报告后，把 `## Worktree Handoff` 的 **WP6 行**更新为
   `Baseline Revision = 5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3`、`Received At = 2026-10-01T15:09:53+08:00`、
   evidence 指向本报告；并在 `## Handoff Index` 增补/更新 WP6 的 `RESOURCE` 行（本角色不直接改 `verification.md`）。
2. 校验本报告后，把 `## Worktree Handoff` 的 **TP2 行**从「尚未登记」替换为本报告 §3 的 TP2 记录；
   建议在 Baseline Revision 栏写明「实测 `81e350f…`（非最终），须在 WP6 并入后再次重定向」，
   并在 `## Handoff Index` 增补 TP2 的 `RESOURCE` 行（同样绑定 `81e350f…`，重定向后失效）。
3. 更新前请重新确认 `integration/session-resume-du1` 仍为 `5ab7e9d…`；若已被 merger 推进（例如 WP6 已并入），
   WP6 这一行需退回本角色重新核对，而 TP2 的登记行则应直接进入「再次重定向」流程。