# Provisioner Report — TP2 worktree 重定向到 U1 集成基线 `95051f9`（第 4 次工作区重定向）

> 本报告只覆盖本次 runtime 资源操作：把 TP2（复用 TP1 的）worktree `D:/Project/acp-remote-wt/session-resume-tp1`
> 的起点从 `81e350f…` 重定向到并入 WP6 后的 U1 集成基线 `95051f9…`。
> 本角色不判断证据充分性，不参与需求/设计/实现/审查/验收，也不修改任何权威规划文件
> （`plan.md` / `tasks.md` / `verification.md`），不改 `crates/**`，不提交产品代码。

## Shared Report

| 字段 | 值 |
| --- | --- |
| task_id | `2.8`（TP2 派发前置 · 第 4 次工作区重定向） |
| role | `provisioner` |
| phase | `runtime` |
| agent_context | provisioner 子 Agent，session `01a0f6ac-bafd-76e3-8fe0-502f5aee166b`（parent session `01a0f628-df10-76d3-99e2-df76bf766c9d`；model `space-bunny-free`，provider `opencode-go`）；**新任务级最小上下文，不继承主对话**；非任何 WP Owner，非 tester；`PI_SUBAGENT_CHILD=1` |
| target_revision | `95051f93db15ff867c3207386e402094790434bb`（U1 集成基线，含 WP1–WP6 交付提交） |
| scope | 仅 worktree `D:/Project/acp-remote-wt/session-resume-tp1`（分支 `agentic/session-resume-tp1`）的一次**纯指针移动**；不创建/删除 worktree 或分支；不动其它任何 worktree；不动 `refs/heads/main` |
| changes | NOT_APPLICABLE（本角色不改产品代码、测试脚本或配置文件；本次唯一改动是一条分支指针 + 新增本份未跟踪报告） |
| checks | NOT_APPLICABLE（本角色不判断证据充分性；本次**未运行** `cargo build/test/clippy/fmt`、`npm install/ci/run check`；RESOURCE 行见 `## 4. Handoff Index`） |
| issues | 无阻断项；4 条交接注意项见 `## 2. Observations` 的 O-1 / O-2 / O-3 / O-4 |
| result | **PASS**（runtime 资源就绪；**不表示** TP2 的任何产品检查通过） |
| evidence_paths | 本报告（`openspec/changes/session-resume/reports/provisioner-repoint-tp2.md`）；同型先例 `provisioner-repoint-wp6-and-tp2.md`、`provisioner-repoint-wp4-wp5.md`、`provisioner-repoint-wp3.md`、`provisioner-worktrees.md`；WP6 并入 U1 的 merger 报告 `merge-u1-integrate-wp6.md`；TP1 设计产物 `tp1-test-design.md` |
| resource_cleanup | 本次未创建任何新资源，故无需清理。TP2 worktree/分支（**已重定向，保留待用**）、`CARGO_TARGET_DIR` `D:/Project/acp-remote-target/session-resume-tp1`（**保留，未删除、未清空**）、`node_modules` 联接（未触碰）全部保留。执行前后复核：其它 9 个 worktree、`refs/heads/main`、`integration/session-resume-du1` 均**未移动**（C-21 / C-22） |

---

## 1. Commands（原始命令 + 原始输出）

shell 为 Git Bash；所有 `git` 命令以 `cd D:/Project/acp-remote-wt/session-resume-tp1` 为工作目录（除显式标注 `-C` 者）。
**未执行** `cargo build` / `cargo test` / `cargo clippy` / `cargo fmt` / `npm install` / `npm ci` / `npm run check`；
**未**对 `node_modules` 联接 `rm -rf`；**未** push / amend / merge / 切分支 / 创建或删除 worktree。

### A 组：重定向前的前置核实（任一不符即应停止）

### C-1 目标提交核实（重置前）

```console
$ git rev-parse 95051f9
95051f93db15ff867c3207386e402094790434bb
$ git log -1 --format='%H %P %s' 95051f9
95051f93db15ff867c3207386e402094790434bb 5ab7e9d640034a410ddf5a9cce07b07b10dcf3a3 a7bc596499170a6365e061c70668a203fa2534bb chore(repo): 集成 WP6（node-link owner session.resume 路由与恢复路径收口）到 U1 集成基线
```

**判定**：目标提交存在；两个 parent 恰为 `5ab7e9d…`（WP5 并入点）与 `a7bc596…`（WP6 交付提交），即 `--no-ff` 合并结果。
与 Assignment 描述（parents `5ab7e9d` + `a7bc596`）逐字一致。

### C-2 前置条件①：工作区必须干净

```console
$ git status --porcelain
（无输出）
$ git status --porcelain && echo "---STATUS_END---"
---STATUS_END---
```

**判定**：`git status --porcelain` 输出为空 ⇒ 无已跟踪改动、无未跟踪文件。**前置条件①满足。**

### C-3 前置条件②：分支相对新基线无独有提交

```console
$ git log 95051f9..agentic/session-resume-tp1
（无输出）
$ git log 95051f9..HEAD
（无输出）
```

**判定**：输出为空 ⇒ 分支 `agentic/session-resume-tp1` 不含任何不在 `95051f9` 历史中的提交，
重置**不会丢失任何提交工作**。**前置条件②满足。**

### C-4 前置条件③：当前 HEAD 应为 `81e350f…`

```console
$ git rev-parse HEAD
81e350ff340014265eb7c9251237c799d4357fee
$ git rev-parse --abbrev-ref HEAD
agentic/session-resume-tp1
```

**判定**：HEAD == `81e350ff340014265eb7c9251237c799d4357fee`，与 Assignment 给定旧值逐字一致；
检出分支为 `agentic/session-resume-tp1`（TP2 与 TP1 复用同一 worktree / 分支，符合 `plan.md` 工作包表 TP2 行的 `wt/tp1`）。**前置条件③满足。**

### C-5 重定向前：`node_modules` 联接与 `CARGO_TARGET_DIR` 现状

```console
$ ls -ld node_modules
lrwxrwxrwx 1 zhang 197610 34 Sep 30 10:39 node_modules -> /d/Project/acp-remote/node_modules
$ ls -la D:/Project/acp-remote-target/session-resume-tp1
total 4
drwxr-xr-x 1 zhang 197610 0 Sep 30 09:50 .
drwxr-xr-x 1 zhang 197610 0 Sep 30 09:50 ..
```

**判定**：`node_modules` 仍是指向主仓库的符号联接（mtime 仍为 `Sep 30 10:39`）；
`CARGO_TARGET_DIR` 目录存在且**当前为空**（尚无该 worktree 的任何构建产物）。二者均保持原样，本角色**未触碰**。

### C-6 重定向前：reflog 基线（用于事后比对「只新增一条」）

```console
$ git reflog --date=iso -n 5
81e350f HEAD@{2026-09-30 09:50:07 +0800}: reset: moving to HEAD
81e350f HEAD@{2026-09-30 09:50:07 +0800}:
```

**判定**：重定向前 reflog 共 2 条（最新一条为初始 worktree 创建时的 `reset: moving to HEAD`）。

### B 组：重定向操作

### C-7 纯指针移动

```console
$ git reset --hard 95051f9
HEAD is now at 95051f9 chore(repo): 集成 WP6（node-link owner session.resume 路由与恢复路径收口）到 U1 集成基线
=== RESET_EXIT=0 ===
```

**判定**：`git reset --hard` exit 0。因 C-2/C-3 已证明分支无独有提交且工作区干净，
这是一次**纯指针移动**，不丢弃任何提交。

### C 组：重定向后的核实

### C-8 HEAD 对比（重定向前 → 重定向后）

| 项 | 重定向前 | 重定向后 |
| --- | --- | --- |
| `git rev-parse HEAD` | `81e350ff340014265eb7c9251237c799d4357fee` | **`95051f93db15ff867c3207386e402094790434bb`** |
| `refs/heads/agentic/session-resume-tp1` | `81e350f…` | **`95051f9…`** |
| reflog 条目数 | 2 | **3** |

### C-9 祖先关系（重定向后）

```console
$ git merge-base --is-ancestor 95051f9 HEAD
is-ancestor: TRUE (exit 0)
```

**判定**：exit 0 ⇒ `95051f9` 是 `HEAD` 的祖先（HEAD 恰等于它，祖先关系平凡成立）。

### C-10 与基线的差异为空（重定向后）

```console
$ git diff --stat 95051f9 HEAD
（无输出）
$ git diff --cached --stat
（无输出）
$ git status --porcelain
---STATUS_END---
```

**判定**：工作树与暂存区相对 `95051f9` 均无差异 ⇒ 纯指针移动已核实。

### C-11 reflog 只新增一条（重定向后）

```console
$ git reflog --date=iso -n 5
95051f9 HEAD@{2026-10-01 16:55:27 +0800}: reset: moving to 95051f9
81e350f HEAD@{2026-09-30 09:50:07 +0800}: reset: moving to HEAD
81e350f HEAD@{2026-09-30 09:50:07 +0800}:
$ git reflog | wc -l
3
```

**判定**：条目数由 2 增至 3，新增的**恰好一条** `reset: moving to 95051f9`；
无 merge、无 amend、无 commit、无 checkout 条目 ⇒ **纯指针移动核实通过**。

### C-12 新基线的提交谱（确认 WP1–WP6 全部在基线内）

```console
$ git log --oneline 81e350f..HEAD
95051f9 chore(repo): 集成 WP6（node-link owner session.resume 路由与恢复路径收口）到 U1 集成基线
a7bc596 fix(node-link): 收紧 session.resume 的幂等比对并补登记
76ab011 feat(node-link): 路由 session.resume 并收口恢复路径的编译涟漪
5ab7e9d chore(repo): 集成 WP5（agent-host 会话恢复后端路径与 ACP 会话标识暴露）到 U1 集成基线
1376e1b fix(agent-host): 为 fake ACP child 增加 --dump-request-params 选项
b486c53 chore(repo): 集成 WP4（storage-sqlite v5 恢复列与 load_recovery 窄读取）到 U1 集成基线
4a3882d feat(storage): v5 migration 追加恢复列并实现 load_recovery 窄读取
4e53fcf feat(agent-host): 落地会话恢复的后端路径与 ACP 会话标识暴露
8a08db8 chore(repo): 集成 WP3（core session.resume 恢复语义）到 U1 集成基线
4f7a235 docs(core): 修正模块架构 §4.1 的端口方法名写法
3e41f90 feat(core): 落地 session.resume 的恢复用例与终态入口
aac0356 feat(core): 落地会话恢复的端口、值对象与两列落盘提交点
b0a387b chore(repo): 集成 WP2（node-link session.resume 与封闭词表原子点）到 U1 集成基线
e8feaf9 chore(repo): 集成 WP1（acp-protocol session/resume DTO）到 U1 集成基线
32f71f5 feat(node-link): 加入 session.resume 命令与封闭词表原子点
248d9b9 feat(acp): 新增 session/resume 类型化 DTO 并提升矩阵交付节奏
```

**判定**：六个工作包的交付提交
WP1 `248d9b9`、WP2 `32f71f5`、WP3 `3e41f90`（文档 `4f7a235`）、WP4 `4a3882d`、
WP5 `4e53fcf`（修复 `1376e1b`）、WP6 `76ab011`（修复 `a7bc596`）
与其各自的 `chore(repo): 集成 …` 合并提交全部位于基线内。
满足 `plan.md` 的 `## Dependency Handoffs` 对 TP2 的要求：
「从包含六者交付提交的集成基线开工」，并逐条对应 WP6 行。

### C-13 三处落地抽查（内容层，证明重定向到的是「含实现」的基线）

#### ① WP3（core 恢复用例入口）— `crates/core/src/broker.rs`

```console
$ grep -n 'session\.resume\|SessionResume' crates/core/src/broker.rs
131:        "session.resume" => "grant.remote-work",
1474:    /// `session.resume`（§5.1、§12.7）：进程不在的 owned 会话的恢复入口。
1481:    /// 工作）：`session.resume` 没有 `CommandPayload` 变体，也不走通用 mutation 管线（否则终态会被
1483:    /// `SessionResumeResult`（`remoteSessionRef.exportId` 只有它有）后经
1496:        self.authorize(actor, "session.resume", Some(session), request)
1519:                command: "session.resume".to_owned(),
1632:    /// `session.resume` 的终态提交（§5.1、§12.7）。返回本次是否真的写入了终态。
1634:    /// 与 [`Broker::settle_session_create`] **同形**，但**只**终结 `command == "session.resume"` 的
1636:    /// `InvalidRequest`。结果（`SessionResumeResult`）由适配层投影——`remoteSessionRef.exportId`
1656:        if record.command() != "session.resume" {
1658:            "settle_session_resume 只能终结 session.resume 的持久记录（§5.1）",
1666:                "session.resume 的持久记录缺少目标会话（§5.1）",
4118:        /// 直接写一条命令行（测试用来构造「已 accepted 的 `session.resume` 记录」）。
6377:    /// 已 `accepted` 的 `session.resume` 持久记录（`settle_session_resume` 的定位目标）。
6386:            "session.resume",
6401:    /// §5.1：`settle_session_resume` 只终结 `session.resume` 的持久记录；同一条记录重复终结（或从未
6424:        assert_eq!(after.command(), "session.resume");
6471:    /// §5.1：`session.resume` 的终态入口不得改写别的命令的幂等行（适配层误用，wire 不可达）。
6500:            .expect_err("非 session.resume 的记录不得被终结");
```

**判定**：命中 WP3 的 grant 臂（`:131`）、恢复用例入口 `Broker::resume_session`
（`:1496` `authorize` → `:1519` 写 `command: "session.resume"`）、终态入口 `Broker::settle_session_resume`
（`:1632`–`:1666` 的只终结 `session.resume` 约束）与配套单元测试。**抽查 PASS。**

#### ② WP5（fake ACP child 新增选项）— `crates/agent-host/src/bin/acpr-fake-acp-agent.rs`

```console
$ grep -n 'dump-request-params\|dump_request_params' crates/agent-host/src/bin/acpr-fake-acp-agent.rs
17://! `--dump-request-params <path>` 是与它**并列**的独立选项：每次收到带 `method` 的入站报文，向该文件
40:    dump_request_params: Option<String>,
62:            dump_request_params: None,
81:                ("--dump-request-params", Some(value)) => args.dump_request_params = Some(value),
192:    dump_request_params(args, method, params);
202:/// `--dump-request-params <path>`：追加一行单行 JSON `{"method":…,"params":…}`。
205:fn dump_request_params(args: &Args, method: &str, params: Option<&Value>) {
206:    let Some(path) = &args.dump_request_params else {
889:    fn dump_request_params_keeps_method_and_params_per_line() {
892:        let args = args_from(&["--dump-request-params", &path.to_string_lossy()]);
```

**判定**：命中 WP5 的并列选项 `--dump-request-params`（参数声明 `:40`、解析 `:81`、每请求调用 `:192`、
写单行 `{method, params}` `:202`–`:206`）及其单元测试 `:889`。
同时**未改动**既有 `--dump-requests` 语义（CR5-F1 方案 (a) 的要求，由 main 在 `1376e1b` 上另行核对过）。**抽查 PASS。**

#### ③ WP6（Node Link owner 路由与幂等冲突）— `crates/server/src/node_link/command.rs`

```console
$ grep -n 'on_session_resume\|idempotency_conflict' crates/server/src/node_link/command.rs
17://!    其中 ownerNodeId 是本进程恒定的本机 id），冲突由 core 报 `command.idempotency_conflict`；
21://!    ACPR-CJ1 指纹不同即回 `nodelink.command.idempotency_conflict`，**不**回首次结果。
265:            CommandName::SessionResume => self.on_session_resume(handle, message, &submit).await,
653:        //    `nodelink.command.idempotency_conflict`（§12.5：不同语义不得得到首次结果）。
672:                        "nodelink.command.idempotency_conflict",
757:                    "nodelink.command.idempotency_conflict",
985:    async fn on_session_resume(
1022:        //    `nodelink.command.idempotency_conflict`，**不**触发第二次 spawn。
1045:                        "nodelink.command.idempotency_conflict",
1111:                    "nodelink.command.idempotency_conflict",
2321:        "command.idempotency_conflict" | "nodelink.command.idempotency_conflict" => (
```

**判定**：命中 WP6 的路由分派（`:265` `CommandName::SessionResume => self.on_session_resume(…)`）、
handler 本身（`:985` `async fn on_session_resume`），以及 ACPR-CJ1 指纹比对后的幂等冲突返回
（`:1022` 注释 + `:1045`、`:1111` 两处 `nodelink.command.idempotency_conflict`）与错误码投影 `:2321`。
其中 `:1022` 的注释「**不**触发第二次 spawn」正是 TP1 的 SR-R12-2 断言所需的语义保证。**抽查 PASS。**

### D 组：未扰动其它资源的复核

### C-14 `refs/heads/main` 未移动

```console
$ git -C D:/Project/acp-remote rev-parse refs/heads/main
81e350ff340014265eb7c9251237c799d4357fee
```

**判定**：仍为 `81e350f…`，与重定向前逐字一致 ⇒ **未触碰 main**。

### C-15 `integration/session-resume-du1` 未移动（必须仍停在 `95051f9`）

```console
$ git rev-parse refs/heads/integration/session-resume-du1
95051f93db15ff867c3207386e402094790434bb
$ git -C D:/Project/acp-remote-wt/session-resume-du1 rev-parse HEAD
95051f93db15ff867c3207386e402094790434bb
$ git -C D:/Project/acp-remote-wt/session-resume-du1 status --porcelain
---DU1_END---
```

**判定**：分支尖端与 `session-resume-du1` worktree 的 HEAD 均为 `95051f9…`，工作区干净。
重定向 TP2 没有影响集成分支的任何一侧（`git reset` 只改当前 worktree 所检出的分支指针）。**du1 仍停在 `95051f9`，符合要求。**

### C-16 其它 `session-resume-wp*` worktree 未移动

```console
$ for w in wp1 wp2 wp3 wp4 wp5 wp6; do printf "%s: " "$w"; git -C D:/Project/acp-remote-wt/session-resume-$w rev-parse --short HEAD; done
wp1: 248d9b9
wp2: 32f71f5
wp3: 4f7a235
wp4: 4a3882d
wp5: 1376e1b
wp6: a7bc596
```

**判定**：六个工作包 worktree 的 HEAD 与重定向前逐字一致 ⇒ **未触碰任何 WP worktree**。

### C-17 TP1 设计产物未被本角色改动

```console
$ sha256sum openspec/changes/session-resume/reports/tp1-test-design.md
449bd7b35015eaf6bcf8be6b7ca35d214aa1b002cb4535310d2d685a675ffbfd *openspec/changes/session-resume/reports/tp1-test-design.md
```

**判定**：摘要 `449bd7b35015eaf6bcf8be6b7ca35d214aa1b002cb4535310d2d685a675ffbfd`
与 `verification.md` 的 `## Handoff Index` 第 39 行（TP1 DELIVERY）登记值**逐字一致**
⇒ 本角色未触碰该文件，`## Handoff Index` 的 TP1 DELIVERY 行无需 `INVALID`。

---

## 2. Observations（交接注意项，非阻断）

- **O-1 — 资源就绪 ≠ 产品检查通过。** 本报告的 `PASS` 仅表示
  「worktree 起点已指向 `95051f9…`、工作区干净、无未提交工作丢失、独占资源未被他人占用」这一约定条件满足。
  **不表示** workspace 全量 1041 passed / clippy 0 诊断在**本 worktree** 复跑通过。
  Assignment 明确本次**不得**跑 `cargo test` / `cargo clippy`；TP2 开工时必须自行复跑其 PV1 取得自己的证据。

- **O-2 — `CARGO_TARGET_DIR` 当前为空。** `D:/Project/acp-remote-target/session-resume-tp1` 存在但无任何
  构建产物（C-5）。因此 TP2 的首次 `cargo` 调用会有一次**完整冷构建**成本。
  该目录按 Assignment 要求**保留、未清空、未删除**；执行者只在自己的 target 与 tempdir 内构建与清理。

- **O-3 — `node_modules` 是指向主仓库的共享符号联接**（`-> /d/Project/acp-remote/node_modules`，mtime `Sep 30 10:39`）。
  本角色**未**对其执行任何写操作（未 `npm install/ci/update`，未 `rm -rf`）。
  它是 `npm run check` 合同门禁脚本的读取来源；**任何执行者都不得对该联接做破坏性清理**。

- **O-4 — 本范围无共享运行资源。** 按 `plan.md` 的 `## Runtime Resources`：
  端口由用例用 `127.0.0.1:0` 自行 `bind`（CR7-F8）；SQLite 测试库各自 tempdir；
  `npm run check` 脚本无状态。因此本角色除 worktree/分支/target 目录外**无需分配或独占**任何资源，
  无独占仲裁需求。

- **O-5 — `Received At` 只能报告为上界。** 见 `## 附录 A`：本角色的事后可见时间戳只有 reflog 的
  reset 条目（`2026-10-01T16:55:27+08:00`），它**等于**本轮首次**写**事件而非接收时刻。
  该值满足「不晚于首次执行事件」的约束，但不是接收时刻本身；已在 3.2 中以
  `received_at_precision: UPPER_BOUND` 标注。

---

## 3. Structured Handoff Record

### 3.1 结构化交接记录（表格形式，便于主 Agent 直接回填 `## Worktree Handoff`）

| Work Package | Attempt | Worktree 绝对路径 | 分支 | Baseline Revision（实测，含旧值） | Provisioner | Executor | Received At | 是否 BLOCKED 及原因 | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| TP2 | 1（重定向） | `D:/Project/acp-remote-wt/session-resume-tp1`（计划规定与 TP1 复用；两包不并发，TP1 已不再单独派发） | `agentic/session-resume-tp1` | **新：`95051f93db15ff867c3207386e402094790434bb`**<br>**旧：`81e350ff340014265eb7c9251237c799d4357fee`** | provisioner 子 Agent，session `01a0f6ac-bafd-76e3-8fe0-502f5aee166b`（parent `01a0f628-df10-76d3-99e2-df76bf766c9d`，新任务级最小上下文，非任何 WP Owner） | **tester-A（待认领）** | **`2026-10-01T16:55:27+08:00`**（上界说明见 O-5） | **否（PASS）** | `openspec/changes/session-resume/reports/provisioner-repoint-tp2.md` |

### 3.2 YAML 形式（worktree_handoff）

```yaml
worktree_handoff:
  - work_package: TP2
    attempt: 1
    record_kind: "baseline_repoint（对既有 Attempt=1 记录的修订，非新 attempt）"
    worktree: "D:/Project/acp-remote-wt/session-resume-tp1"
    branch: "agentic/session-resume-tp1"
    baseline_revision: "95051f93db15ff867c3207386e402094790434bb"
    previous_baseline_revision: "81e350ff340014265eb7c9251237c799d4357fee"
    superseded_baseline_revision: "81e350ff340014265eb7c9251237c799d4357fee（= previous；该值先前被显式标注为「非最终」，见 reports/provisioner-repoint-wp6-and-tp2.md 的 TP2 登记行）"
    baseline_source: "提交 95051f9（chore(repo): 集成 WP6 … 到 U1 集成基线），parents 5ab7e9d + a7bc596；分支 integration/session-resume-du1 尖端（worktree D:/Project/acp-remote-wt/session-resume-du1，merger 产出，报告 reports/merge-u1-integrate-wp6.md）"
    cargo_target_dir: "D:/Project/acp-remote-target/session-resume-tp1（保留，未清理、未删除；当前为空，尚无构建产物）"
    provisioner: "provisioner 子 Agent，session 01a0f6ac-bafd-76e3-8fe0-502f5aee166b，parent 01a0f628-df10-76d3-99e2-df76bf766c9d，新任务级最小上下文，非任何 WP Owner"
    executor_claimed: "tester-A（plan.md 工作包表 TP2 行指派；尚未在 dispatch-queue.jsonl 开工，认领待主 Agent 派发）"
    reviewer_assigned: "reviewer-T2（plan.md 指派，未派发）"
    received_at: "2026-10-01T16:55:27+08:00"
    received_at_precision: "UPPER_BOUND"
    revision_supersedes: true
    supersedes_received_at: "2026-10-01T15:10:08+08:00（provisioner-repoint-wp6-and-tp2.md 的 TP2 登记行；更早的 2026-09-30T09:50:17+08:00 为 provisioner-worktrees.md 的 TP1/TP2 共用登记行）"
    change_description: >-
      起点由 81e350f 重新指向并入 WP6 后的 U1 集成基线 95051f9。该基线经实测含 WP1 248d9b9、WP2 32f71f5、
      WP3 3e41f90/4f7a235、WP4 4a3882d、WP5 4e53fcf/1376e1b、WP6 76ab011/a7bc596 六个交付提交
      及其各自 chore(repo): 集成 合并提交，满足 plan.md 对 TP2「从包含六者交付提交的集成基线开工」
      （依赖 code:WP1…WP6）的要求。分支相对新基线无独有提交、重置前工作区干净，
      故本次为纯指针移动、无任何工作丢失（reflog 仅新增一条 reset 条目为证）。
    verifications:
      worktree_clean_before: PASS
      no_branch_only_commits_before: PASS
      head_equals_expected_previous_value: PASS
      head_equals_baseline: PASS
      baseline_is_ancestor_of_head: PASS
      diff_vs_baseline_empty: PASS
      reflog_exactly_one_new_entry: PASS
      spotcheck_wp3_session_resume_arm_in_broker_rs: PASS
      spotcheck_wp5_dump_request_params_in_agent_host: PASS
      spotcheck_wp6_on_session_resume_and_idempotency_conflict: PASS
      all_six_upstream_deliveries_in_baseline: PASS
      node_modules_link_intact: PASS
      cargo_target_dir_preserved: PASS
      main_untouched: PASS
      du1_untouched_still_at_95051f9: PASS
      other_worktrees_untouched: PASS
      tp1_design_report_unchanged: PASS
    preconditions_met: true
    blocked: false
    ready_to_work: true
    ready_to_work_qualifier: "仅指 runtime 资源就绪；PV1（workspace 全量）须由 TP2 执行者自行复跑取证，本角色未运行任何 cargo/npm 检查。"
    evidence: "openspec/changes/session-resume/reports/provisioner-repoint-tp2.md"
    provisioner_note: >-
      本行需由主 Agent 校验后写入 verification.md 的 ## Worktree Handoff，
      替换 provisioner-repoint-wp6-and-tp2.md 的 TP2 登记行（其中 Baseline Revision 栏原被显式标注为
      「实测 81e350f…（非最终）」、Received At 为 2026-10-01T15:10:08+08:00）。
      同表第 179 行的 TP1 行请按第 5 节的建议标注为已并入 TP2（详见 ## 5. TP1 那一行是否仍然成立）。
```

### 3.3 本次单独成立的资源行（供 `## Handoff Index` 回填）

```yaml
resource_row:
  work_package: TP2
  task_id: "2.8"
  role: provisioner
  phase: runtime
  round: NOT_APPLICABLE
  stage: runtime
  target_revision: "95051f93db15ff867c3207386e402094790434bb"
  evidence_type: RESOURCE
  evidence_id: NOT_APPLICABLE
  report_path: "openspec/changes/session-resume/reports/provisioner-repoint-tp2.md"
  result: PASS
  evidence_status: NEW
  source_evidence: NOT_APPLICABLE
  invalidates:
    - "verification.md ## Handoff Index 中 provisioner-repoint-wp6-and-tp2.md 的 TP2 RESOURCE 行（绑定 81e350f…；该行自己已声明「WP6 并入后必须由 provisioner 再次修订」）"
  applicability_basis: >-
    在 worktree D:/Project/acp-remote-wt/session-resume-tp1 内实测：重置前 git status --porcelain 为 0 行（C-2）、
    git log 95051f9..agentic/session-resume-tp1 与 95051f9..HEAD 均无输出（C-3，无独有提交）、
    git rev-parse HEAD == 81e350ff340014265eb7c9251237c799d4357fee（C-4，与 Assignment 给定旧值一致）；
    git reset --hard 95051f9 exit 0（C-7）后 git rev-parse HEAD == 95051f93db15ff867c3207386e402094790434bb、
    merge-base --is-ancestor 95051f9 HEAD 为真（C-9）、git diff --stat 95051f9 HEAD 与 --cached 均为空、
    git status --porcelain 仍 0 行（C-10）、reflog 由 2 条增至 3 条且新增恰一条
    "reset: moving to 95051f9"（C-11，纯指针移动）。基线内容抽查：C-12 列出 WP1–WP6 六个交付提交全部在
    81e350f..95051f9 内；C-13 三处落地均命中——crates/core/src/broker.rs 的 grant 臂(:131)、
    resume_session(:1496/:1519)、settle_session_resume(:1632/:1656) 及其单元测试；
    crates/agent-host/src/bin/acpr-fake-acp-agent.rs 的 --dump-request-params(:40/:81/:192/:202) 及其单元测试(:889)；
    crates/server/src/node_link/command.rs 的 on_session_resume(:265/:985) 与
    nodelink.command.idempotency_conflict(:1045/:1111，含「不触发第二次 spawn」保证)。
    未扰动复核：refs/heads/main 仍为 81e350f…（C-14）；integration/session-resume-du1 与
    session-resume-du1 worktree 均仍为 95051f9…且工作区干净（C-15）；wp1..wp6 六个 worktree 的 HEAD 逐字未变
    （C-16）；tp1-test-design.md 的 sha256 仍为 449bd7b3…（C-17，与 Handoff Index 第 39 行一致）。
    node_modules 联接（mtime Sep 30 10:39）与 CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-tp1
    均保留（C-5，未 npm ci、未 rm -rf、未构建、未测试、未运行 npm run check）。
    本行绑定 95051f9…；该提交一旦被移动（新的集成基线或 main 前进），本行失效、TP2 必须重新重定向并重跑。
    本行只表示 runtime 资源就绪，不表示 TP2 的 PV1 或任何产品检查通过。
```

---

## 4. Handoff Index

```yaml
handoff_index:
  - task_id: "2.8"
    work_package: TP2
    role: provisioner
    phase: runtime
    round: NOT_APPLICABLE
    stage: runtime
    target_revision: "95051f93db15ff867c3207386e402094790434bb"
    evidence_type: RESOURCE
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/session-resume/reports/provisioner-repoint-tp2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "见上 3.3 resource_row.applicability_basis"
    source_evidence: NOT_APPLICABLE
```

---

## 5. TP1 那一行是否仍然成立

**结论：`verification.md` 的 `## Worktree Handoff` 第 179 行（TP1）所描述的「worktree + 分支」这一资源事实仍然成立，
但作为 TP2 的交接记录它已被本报告取代，需要主 Agent 显式标注；本角色不修改该文件。**

分三层说明：

1. **资源事实层：仍然成立，无需改动。**
   该行指向的 worktree `D:/Project/acp-remote-wt/session-resume-tp1` 与分支 `agentic/session-resume-tp1`
   都仍然存在、仍然检出该分支（`plan.md` 工作包表 TP2 行的 `wt/tp1` 也规定 TP2 复用同一 worktree，
   「两包分处 W1/W5，不并发」）。本次重定向只是把这条**共享** worktree 的指针从 `81e350f…` 移到 `95051f9…`，
   因此第 179 行里除 `Baseline Revision` 外的字段（路径、分支、Executor=tester-A）依然准确。

2. **交接记录层：已被取代。**
   第 179 行的 `Baseline Revision` 是 `81e350f…`、`Received At` 是 `2026-09-30T09:50:17+08:00`，
   它描述的是**本 worktree 在 TP1 开工时的起点**。TP2 声明 `code:WP1…WP6`，必须从含 WP6 的基线开工，
   该起点已不满足 TP2。建议主 Agent：
   - 保留第 179 行不动（它是 TP1 设计阶段的历史记录，且 `## Handoff Index` 第 39 行 TP1 DELIVERY 仍然有效，
     见 C-17），但在其备注列加一句「TP1 已不再单独派发；其 worktree 现由 TP2 复用，起点已重定向至 `95051f9…`
     （见 `reports/provisioner-repoint-tp2.md`）」；**不要**改写它的 `Baseline Revision`——那会篡改历史记录。
   - 用 3.2 的 TP2 行替换 `provisioner-repoint-wp6-and-tp2.md` 那条 TP2 登记行（原基线 `81e350f…`、
     `Received At 2026-10-01T15:10:08+08:00`，且该行自己已写明「须在 WP6 并入后由 provisioner 再次重定向」——
     本次正是执行该承诺）。

3. **对本次重定向有无影响：无。**
   TP1 只交付了设计报告（`reports/tp1-test-design.md`），**没有任何代码提交**（C-3 实测
   `95051f9..agentic/session-resume-tp1` 为空，与「TP1 只交付设计报告、无代码提交」一致）。
   因此 TP1 是否单独派发、以及其内容何时并入 TP2，都**不改变**重定向的事实依据：
   本次移动的是一条**尚无任何提交在其上的分支指针**，故 TP1 的存废对结果无影响。
   唯一需要主 Agent 记住的语义变化是：**TP2 的输入不再是「TP1 的已修正版设计」这一独立交付物，
   而是「用户 2026-10-01 决定把 TP1 内容并入 TP2」后的产物**。TP2 执行者应以
   `reports/tp1-test-design.md`（sha256 `449bd7b3…`，含 CR7 Round 2 前的原设计）与
   `verification.md` 的 `## Review Findings`（CR7-F1…F8、CR5-F2）为准，自行在 TP2 内完成设计修正与用例编写；
   这属于**内容侧**问题，不影响本报告的资源交接。

---

## 6. Resource Cleanup

| 资源 | 处置 | 复核 |
| --- | --- | --- |
| worktree `D:/Project/acp-remote-wt/session-resume-tp1` | **保留**（TP2 待开工，已就绪） | C-8 / C-9：HEAD 已为 `95051f9…` |
| 分支 `agentic/session-resume-tp1` | **保留**，指针已更新至 `95051f9…` | C-8：分支名未变（C-4）；reflog 仅一条新 reset（C-11） |
| `CARGO_TARGET_DIR` `D:/Project/acp-remote-target/session-resume-tp1` | **保留，未删除、未清空** | C-5：目录仍存在（当前为空） |
| `node_modules` 目录联接（tp1） | **保留，未触碰** | C-5：仍指向 `/d/Project/acp-remote/node_modules`，mtime 仍为 `Sep 30 10:39` |
| worktree `session-resume-du1` 与分支 `integration/session-resume-du1` | **保留，未触碰** | C-15：尖端与 worktree HEAD 均仍为 `95051f9…`，工作区干净 |
| worktree `session-resume-wp1`…`wp6`（6 个） | **保留，未触碰** | C-16：HEAD 逐字未变（`248d9b9`/`32f71f5`/`4f7a235`/`4a3882d`/`1376e1b`/`a7bc596`） |
| 其它变更目录 worktree（`export-ids*`、`nl-owner-*`、`node-link-owner`，4 个） | **保留，未触碰** | C-6 前置的 `git worktree list`：共 14 个 worktree，注册项未减少、未新增 |
| `refs/heads/main` | **保留，未触碰** | C-14：仍为 `81e350f…` |
| TP1 设计产物 `reports/tp1-test-design.md` | **保留，未触碰** | C-17：sha256 仍为 `449bd7b3…` |
| 本次新建的共享资源（端口/容器/数据库/账号/缓存） | 无 | O-4：本范围无共享运行资源 |
| 本报告文件 | 新增 1 个未跟踪文件（`openspec/changes/session-resume/` 整体未跟踪） | `git status --porcelain openspec/changes/` → `?? openspec/changes/session-resume/` |

**结论**：本次仅移动一条分支指针（`refs/heads/agentic/session-resume-tp1`：`81e350f…` → `95051f9…`）
并新增一份未跟踪报告；无新资源、无残留、无越界释放他人实例、未清理任何 target 目录。

---

## 7. 本次**未**做的事（明确清单）

### 7.1 未触碰的资源 / 分支（按 Assignment 明令）

1. **未**触碰 `session-resume-wp1`、`-wp2`、`-wp3`、`-wp4`、`-wp5`、`-wp6` 六个 worktree 或其分支
   （实测 HEAD 逐字未变，C-16）。
2. **未**触碰 `session-resume-du1` worktree 或分支 `integration/session-resume-du1`
   —— 仍停在 `95051f9…`，工作区干净（C-15）。
3. **未**触碰其它变更目录的 worktree（`export-ids`、`export-ids-docs`、`export-ids-integration`、
   `nl-owner-integration`、`node-link-owner`），worktree 总数仍为 14。
4. **未**移动 `refs/heads/main` —— 仍为 `81e350ff340014265eb7c9251237c799d4357fee`（C-14）。
5. **未**删除、未清空 `D:/Project/acp-remote-target/session-resume-tp1`（C-5）。
6. **未**触碰 `node_modules` 符号联接；**未**对其执行 `rm -rf` 或任何写操作（C-5）。
7. **未**创建或删除任何 worktree / 分支；**未** checkout / 切分支；**未**新建临时分支。

### 7.2 未运行的检查（按 Assignment 明令：预热构建会白烧时间并可能触发钩子问题）

8. **未**运行 `cargo build`（含 `--release`、含任何 `-p` 变体）。
9. **未**运行 `cargo test`（任何 crate、任何 workspace、任何 `--all-features` 组合）。
10. **未**运行 `cargo clippy`、`cargo fmt`、`cargo deny`、`cargo tree`。
11. **未**运行 `npm install`、`npm ci`、`npm run check`、`npm run verify`、`npm run check:rust`
    或任何 `npm run check:*` 子门禁。
12. **未**执行 `npx openspec-agentic dispatch …` —— 工作包状态登记（`coding`）由主 Agent 在执行者开工前完成，
    不属本角色职责。
13. **未**执行任何 `.husky` 钩子触发路径（因此未产生 commit，无钩子风险）。

> **因此：workspace 全量 1041 passed / clippy 0 诊断是 merger 在 `95051f9` 上取得的既有结论，
> 本角色既未复跑、也不复述为「本次已验证」。TP2 开工时必须自行复跑其 PV1 并产出自己的证据。**

### 7.3 未修改的文件（按 Assignment 明令：回填由主 Agent 做）

14. **未**修改 `openspec/changes/session-resume/plan.md`。
15. **未**修改 `openspec/changes/session-resume/tasks.md`。
16. **未**修改 `openspec/changes/session-resume/verification.md`（含其 `## Worktree Handoff`、
    `## Handoff Index`、`## Dispatch Reconciliation`、`## Failures and Retries`）。
17. **未**修改 `crates/**` 下任何文件（本次 `git diff --stat` 为空即证，C-10）。
18. **未**修改任何设计 / 规格 / schema / fixture / `compatibility/**` 资产。
19. **未**修改任何 `docs/**` 文档。
20. **未**修改 `reports/tp1-test-design.md`（sha256 未变，C-17）。

### 7.4 未提交的代码 / 未推送

21. **未**创建任何提交（`git reflog` 只新增一条 `reset` 条目，无 `commit` 条目，C-11）。
22. **未** `push`、**未** `amend`、**未** `merge`、**未** 打 tag、**未**改写任何历史。
23. **未**暂存任何文件（`git diff --cached --stat` 为空，C-10）。
24. **未**提交产品代码。

### 7.5 未越界的判断（角色边界）

25. **未**判断任何证据是否充分；**未**给出设计、实现或审查结论；**未**评估 TP2 的用例设计质量。
26. **未**对 CR7 修复轮、CR5-F2 前提修正等**内容侧**问题下结论 —— 第 5 节只陈述资源事实与对交接的影响。
27. **未**新增、删除或修改工作包定义、依赖关系或检查 ID。
28. **未**分配独占资源（本范围无共享运行资源需仲裁，O-4）。

---

## 8. 遗留风险与建议下一步

| 风险 | 说明 | 建议下一步（属主 Agent / 执行者，非本角色） |
| --- | --- | --- |
| 冷构建成本 | `CARGO_TARGET_DIR` 为空，TP2 首次 `cargo` 会完整编译 workspace（O-2） | 派发时告知 tester-A 预留首次构建时间；建议一次性 `cargo test --locked --workspace --all-features` 以摊薄成本 |
| `Received At` 为上界 | 本角色只在事后可从 reflog 读到时间戳，无法给出接收的精确时刻（O-5） | 若工作流要求精确接收时刻，请在派发前由主 Agent 记录派发时刻，并与本行的上界并存 |
| RESOURCE 行随基线移动失效 | 本行绑定 `95051f9…`；若 U1 再次并入新提交或 main 前进 | 届时必须再次重定向本 worktree 并重跑本报告的 A/C 组核实 |
| TP1 合并进 TP2 的内容侧工作 | CR7-F1…F8、CR5-F2 的修正仍需在 TP2 内完成（C-17 证明设计产物未改） | 派发提示中把 `verification.md` 的 `## Review Findings`（CR7/CR5-F2 行）与 `reports/tp1-test-design.md` 一并交给 tester-A |
| PV1 证据未产生 | 本角色按分工未运行任何检查（O-1） | TP2 自行复跑 PV1（workspace 全量）并产出 `reports/tp2-*.md` 与日志 |

---

## 附录 A：O-5 — `Received At` 的精度声明

`roles/provisioner.md` 要求「开工前接收时间，`Received At` 不得晚于该轮首次执行事件（首次为 coding）」。

- 本角色的**首次执行事件**是 C-2/C-3/C-4 的前置核实（在 `git reset` 之前）。
- 我手上唯一带机器时间戳的记录是 reflog 的 reset 条目：`2026-10-01 16:55:27 +0800`，
  以及紧随其后的 `date --iso-8601=seconds`：`2026-10-01T16:56:08+08:00`。
- 因此我报告的 `Received At = 2026-10-01T16:55:27+08:00` 只能是**上界**
  （= 本轮首次**写**事件的时刻）：接收与前置核实发生在该时刻**之前**，故该值满足「不晚于」的要求，
  但**不是**接收时刻本身。

结论：**满足**「`Received At` 不得晚于首次执行事件」的约束（以可核实的上界形式）。
精确接收时刻须由派发方在派发时记录。已在 3.2 中以 `received_at_precision: UPPER_BOUND` 标注此事实，
避免下游把它误读为精确接收时刻。
