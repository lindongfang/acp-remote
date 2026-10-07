<!-- WP3 修复轮次（Attempt 3）交付报告。coder 只交付修复、用例与检查证据；不判独立 review、合入、E2E 与最终验收。 -->

# WP3 修复轮次交付（deliver-wp3-r3）

## Shared Report

- **task_id**: 2.3（`phase: fix`，DELIVERY）/ 3.5（CHECK：PV1 / PV2）
- **work_package**: WP3（交付单元 MU2）
- **role**: coder（`coder-w3-r3`，Attempt 3）
- **phase**: fix（本轮只修 TP2 行为测试发现的两处产品缺陷，不重写已通过 `review-w3-r2` 的实现）
- **agent_context**: `agent_id: coder-w3-r3`，`isolation: fork_turns=none`（新实例接管 `integ/mu2-wiring`；未继承
  `coder-w3-r1`/`coder-w3-r2` 的任何对话，只带原提交 `3b6fe41`、`review-w3-r2` 报告、TP2 的红测与 Main 的派发说明）
- **target_revision**: `1550909b13bc59dd2ad635a45eff4de4e9699150`（分支 `integ/mu2-wiring`，父提交 = 集成基线 `3b6fe41`）
- **scope**: 只在 `.worktrees/integ` 内改动 `crates/core/src/derive.rs` 与 `crates/core/src/broker.rs`；未触碰
  `crates/core/Cargo.toml`、`crates/sync-protocol/**`、`schemas/**`、`docs/**`、`fixtures/**`、`crates/core/src/ports.rs`、
  `crates/agent-host/**`、`crates/app/**`、`.worktrees/` 下其他包、主检出、`openspec/changes/**`（本报告与其日志除外）
- **result**: **PASS**（PV1 / PV2 exit 0；R7 的 8 个情形 + R9 的 `updated_at` 来源逐条修复并各带可判别用例）

### 交付提交

| 项 | 取值 |
| --- | --- |
| worktree | `D:\Project\acp-remote\.worktrees\integ` |
| 分支 | `integ/mu2-wiring` |
| 集成基线（base） | `3b6fe4136e71e5a8a9b36527440348d9be280ae0` |
| 本轮交付（target） | `1550909b13bc59dd2ad635a45eff4de4e9699150` |
| 提交信息 | `fix(core): WP3 第三轮——前导 .. 显式计数修 R7 越界泄露，通知只带更新时间时推进权威 updated_at` |
| `CARGO_TARGET_DIR` | `D:\Project\acp-remote\.target-wt\mu2int`（复用既有目录，未新建） |

`git diff --stat 3b6fe41..1550909`（2 files changed, **296 insertions(+), 17 deletions(-)**）：

```
 crates/core/src/broker.rs |  59 ++++++++++-
 crates/core/src/derive.rs | 254 +++++++++++++++++++++++++++++++++++++++++++---
```

---

## 1. R7（产品缺陷，TP2 判 MAJOR）路径归一化在前导 `..` 越过根时丢计数

### 根因复核（成立）

`normalize` 用 `PathBuf::pop()` 的**返回值**判断能否回退：

```rust
Component::ParentDir => {
    if !out.pop() { out.push(".."); }   // ← pop() 弹掉根时返回 true，计数因此丢失
}
```

`PathBuf::pop()` 在弹掉 `Prefix`/`RootDir` 组件（`C:\`、`\\server\share`）时返回 `true` 而非失败。于是第二个
前导 `..` 把缓冲区的根前缀「成功」弹掉、得到 `PathBuf::new()`，随后的 `etc/passwd` 被当作普通相对路径追加。
`display_path` 里那道守卫 `matches!(normalized.components().next(), Some(Component::ParentDir))` 此时看到的是
`Normal("etc")`，整条路径遂被判为**区内**，把工作区外的**目录结构**下发到面向客户端的 `displayPath` 上。

我在交付提交上以探针程序实测了旧实现的四个形状（`3b6fe41` 的 `derive.rs`），确认根因与修改后行为：

| 报告路径（root=`/work/api`） | `3b6fe41` 实测 | `1550909` 实测 |
| --- | --- | --- |
| `../../etc/passwd` | `("etc/passwd", false)` ← 泄漏 | `("passwd", true)` |
| `../../../a/b` | `("b", true)` | `("b", true)` |
| `/work/api/../api/x` | `("x", false)` | `("x", false)` |
| `src/main.rs` | `("src/main.rs", false)` | `("src/main.rs", false)` |

（`../../../a/b` 在旧实现上恰好也判越界：第三个 `..` 时缓冲区已空，`pop()` 返回 false，于是补了一个 `..`，
守卫随机看到 `ParentDir`。这正说明旧实现的判定依赖缓冲区余量、而非前导 `..` 的真实个数——3 个及以上同样
不可靠。）

### 修法（不依赖 `PathBuf::pop()`，显式计数）

`normalize` 改为返回 `Normalized { path: PathBuf, escapes: usize }`，**逐组件显式统计逃出起点的前导 `..`
个数**：

- `Prefix` → 进入结果前缀；
- `RootDir` → 进入结果前缀，置 `rooted = true`，并把已累计的 `escapes` 一次性**作废**（绝对根之上不可回退：
  `/..` = `/`、`C:\..` = `C:\`）；
- `Normal` → 压入组件栈（它只能取消**它前面**的 `..`，不能取消前导 `..`）；
- `ParentDir` → 优先弹掉栈顶的 `Normal`；栈空时——`rooted` 为真则丢弃（根段不可回退），否则
  `escapes += 1`。

结果路径按 `base + (".." × escapes) + stack` 重建（`escapes > 0` 时它是**畸形**的、只用于越界判定，绝不进入
前缀比较）。

`display_path` 的相对分支改为按 `escapes > 0` 判逃逸；`escapes == 0` 时用新增的 `join_lexically` 把片段拼到根
后面**再折叠一次**——`root.join(segment)` 会让 `sub/../file.txt` 带着未折叠的 `..` 进入 `strip_prefix` 的组件级
字面量比较，从而把合法区内路径误判为越界。

绝对分支（`canonical_path` + `strip_prefix`）一行未改；`canonical_path` 只跟随 `normalize` 的签名改为
`normalize(path).path`，其语义（词法化 + 逐级上溯 canonicalize）保持不变。

### 八个情形的逐条实测（每个都有单测钉住）

| # | 情形 | 期望 | 实测 `(displayPath, outside)` | 用例 |
| --- | --- | --- | --- | --- |
| 1 | `../../etc/passwd`（越过根） | 越界，只剩 `etc/passwd` | `("passwd", true)` | `display_path_escapes_many_leading_parent_traversals`、`..._multi_level_traversal_to_the_declared_file_names` |
| 2 | `../../../a/b`（3 个以上） | 越界 | `("b", true)` | `..._many_leading_parent_traversals`（含 6 级） |
| 3 | `C:\..\..\etc\passwd`（Windows 前缀 + 根） | 越界 | `("passwd", true)`；`normalize` 得 `C:\etc\passwd`、`escapes == 0` | `normalize_counts_leading_parent_traversals_without_popping_the_root`、`display_path_treats_a_windows_parent_traversal_as_outside`（`#[cfg(windows)]`） |
| 4 | `C:\` 根被 `pop()` 吃掉 | 不再丢计数 | `C:\..`→`C:\`、`C:\a\..\..`→`C:\`、`C:\a\b\..`→`C:\a`，`escapes == 0` | `normalize_marks_the_root_under_a_windows_prefix`（`#[cfg(windows)]`） |
| 5 | UNC `\\server\share\..\..\x` | 按 spec 口径，不泄露份额名与层级 | `normalize` 得 `\\server\share\x`、`escapes == 0`；以 `\\server\share\ws` 为根时判越界、只给 `x` | `normalize_...`（`#[cfg(windows)]`）、探针实测 `("x", true)` |
| 6 | `/work/api/../api/x`（区内含 `..`，未越界） | **区内**（回归红线） | `("x", false)` | `display_path_keeps_an_inner_parent_traversal_that_stays_inside` |
| 7 | 恰好等于工作区根 | 按 spec 口径（既非越界下发也非文件） | `(".", true)`（哨兵：不泄露根的任何片段） | `display_path_treats_the_workspace_root_itself_by_the_spec` |
| 8 | `src/main.rs`（普通区内相对路径） | **区内**，行为不得改变 | `("src/main.rs", false)` | `display_path_accepts_a_relative_form_as_workspace_relative`、`display_path_traversal_shapes_stay_outside_on_every_platform` |

补充：`escapes` 的逐级语义（`../etc/passwd`→1、`../../etc/passwd`→2、`../../../../a/b`→4、`a/../b`→0、
`a/../../b`→1）由 `normalize_counts_leading_traversals_on_relative_paths` 钉住。

**关于第 7 行的一处既有边界（如实登记，非本轮引入）**：在 Windows 上以 `/work/api`（**无盘符前缀**、
对 Windows 而言不是绝对路径）作根时，相对分支会把该串当作「相对根的片段」解释，于是「恰好是根」这条
形状判出 `("api", true)` 而非 `(".", true)`——`displayPath` 是根的最后一段。我以同一探针在基线 `3b6fe41`
上复测，**旧实现给出完全相同的 `("api", true)`**，因此这不是本轮引入的回归；它要求「工作区根在 Windows
上是前缀less 的 Unix 形式」这一在真实产品路径下不会出现的合成输入（工作区根由
`workspace-resolution` 规范化，Windows 上恒带盘符）。真实形状（带盘符根的 `C:\...`、UNC，以及临时目录）
均按第 3–5、7 行判定。此处不擅自扩大改动面（第 7 行的产品口径本身正确，风险在根形状而非判定逻辑）。

### 判别力实证（回退源码）

1. 把 TP2 的 `crates/storage-sqlite/tests/derived_events_behaviour.rs` 复制为本地探针（不提交，跑完即删），
   在**基线** `3b6fe41` 的 `derive.rs` 上重跑：**29 passed / 2 failed**——恰为
   `a_relative_path_escaping_the_root_is_outside` 与 `multiple_leading_parent_traversals_keep_only_the_file_name`，
   与 TP2 报告的 2 条红测逐条对应。
2. 恢复本提交的 `derive.rs` 后再跑同一探针：**31 passed / 0 failed**。
3. `cargo test -p core` 的 `display_path_*` 共 12 条全绿（含新增 6 条）。

**未**通过「遇 `..` 一律判越界」回避：第 6、8 行的区内回归红线各有断言拦住。

---

## 2. R9（Main 转达 TP2 的登记，违反 `design.md` D7）只有更新时间时 `updated_at` 不前进

### 根因

`commit_chunk` 只在 `title_intent` 产出 `Set`/`Clear` 时才填 `title_update`，`state` 的组装判据是
`session_state.is_some() || title_update.is_some()`。因此「通知只带 `updatedAt`」（标题意图 `Unchanged`）
**不产出任何 `SessionUpdate`**：本批照常落 `session.info.changed` 事件，但 `owned_session` 一行不写，
`updated_at` 停在旧值上。而它与 F1 同源——`design.md` D7 要求权威更新时间取 Daemon 持久化时间，
`SYNC_PROTOCOL.md` §10.2 的目录排序按 `updated_at DESC`，该列不前进排序就是错的。

### 修法

新增 `info_update_seen`（本批是否出现过 `session.info.changed`），并把它计入 `state` 组的判据：

```rust
let state = if session_state.is_some() || title_update.is_some() || info_update_seen {
    Some(StateChange::Update(SessionUpdate { title: title_update, .. }))
```

`title_update` 仍是两层可选（`None` = 不改该列），因此只带更新的通知**只**推进 `updated_at` 与 `version`，
既有标题一律不变；存储层的三次态语义与哨兵参数（F1 的修复）一行未动。事件 view 的 `updatedAt` 照常转发
Agent 自报值（转发与权威时间是两件事）。

### 用例与判别力

- 新增 `crates/core/src/broker.rs::tests::a_session_info_update_without_a_title_still_advances_the_updated_at`：
  先以 `submit_prompt` 制造较早基线 → 推入一条 ACP 原文**无 `title` 键**、且 `event.at` 早于基线的
  `session.info.changed` → flush 后断言 ① `updated_at != "2020-01-01T00:00:00.000Z"`（不取 Agent 自报值）
  ② `updated_at > before`（**必须前进**）。
- 判别力实证：把 `state` 判据临时改回 `... || false` 后重跑，该用例在断言 ② 失败；恢复后 189 条全绿。
- **未**弱化既有断言：`a_session_info_update_without_a_title_keeps_the_existing_one`（标题不变）与
  `a_session_info_update_writes_the_title_from_the_agent_notification`（标题写入 + 不取自报值）均原样保留且仍绿。

### 预期 TP2 断言由红转绿 / 新增转绿

| TP2 用例 / 断言 | 现状 | 本提交后 |
| --- | --- | --- |
| `a_relative_path_escaping_the_root_is_outside`（`../..` 两级） | **RED** | **GREEN** |
| `multiple_leading_parent_traversals_keep_only_the_file_name`（`../../etc/passwd` + `../../Users/alice/.ssh/id_rsa`） | **RED** | **GREEN** |
| `a_relative_form_inside_the_workspace_is_kept_relative` | GREEN | 仍绿 |
| `an_inner_dotdot_that_stays_inside_is_inside`（`sub/../file.txt`） | GREEN | 仍绿 |
| `the_workspace_root_itself_never_leaks_its_fragment` | GREEN | 仍绿 |
| `an_inside_path_is_relativized_without_the_root_fragment` | GREEN | 仍绿 |
| `a_string_prefix_is_not_treated_as_inside_the_workspace` | GREEN | 仍绿 |
| `an_unregistered_workspace_root_falls_back_to_outside` | GREEN | 仍绿 |
| `an_outside_path_is_marked_and_carries_only_the_file_name` | GREEN | 仍绿 |
| `another_absolute_outside_path_keeps_only_the_file_name` | GREEN | 仍绿 |
| `an_absolute_path_with_an_inner_dotdot_that_stays_inside_is_inside`（TP2 新增） | GREEN | 仍绿 |
| `a_notification_without_a_title_keeps_the_existing_one`（若 TP2 把「`updated_at` 必须前进」升级为红测） | 未断言前进 | **可转 GREEN**（本提交后该列前进到提交时钟） |

合计：TP2 的 **2 条红测由红转绿**，5 条 R7 回归防护 + 既有 3 条保持绿；若 TP2 就 R9 补上「必须前进」断言，
它也会转绿。我在本地以复制 TP2 测试文件的临时探针实测：**31 passed / 0 failed**（跑完即删，未提交）。

---

## 3. 未改动项（复核已通过、按要求保持原样）

行级 Myers diff 与省略口径、规范化前缀判定（`/a/b` vs `/a/bc`）、节点级事件 DDL CHECK、ACP 原文保真、
标题两层可选与哨兵参数三态——均一行未动。`crates/core/src/ports.rs` 的 `SessionUpdate` 形状未动
（本轮两处修复都只改 core 的判定与提交组装）。改动触及 `display_path`/`normalize` 与 `commit_chunk` 的公共
逻辑，因此连带复跑了它们下游的全部既有用例（PV1 全 workspace、PV2 三个 crate），均绿。

---

## Checks（实际命令 / 目录 / 环境 / 退出码 / 子检查 / 日志）

| ID | 命令 | 工作目录 | 环境 | 退出码 | 子检查 |
| --- | --- | --- | --- | --- | --- |
| **PV1** | `npm run verify` | `.worktrees\integ` | `CARGO_TARGET_DIR=D:\Project\acp-remote\.target-wt\mu2int`；Node v22.22.0 / npm 10.9.4；rust-toolchain 1.98.1 | **0** | 见下 |
| **PV2** | `cargo test --locked -p core -p storage-sqlite -p agent-host --all-features` | `.worktrees\integ` | 同上；fake ACP Agent，无外部服务 | **0** | 27 个 `test result: ok`，合计 432 passed、0 failed |

**Local Checks**：`cargo fmt --all`（exit 0）；`cargo clippy --locked -p core -p storage-sqlite --all-targets -- -D warnings`（exit 0，零 warning）。

### PV1 子检查逐条（`reports/PV1-wp3-r3.log`）

- `check:schemas` → `schema fixtures OK: 149 valid, 51 invalid (ajv Draft 2020-12), 41 event views bound`
- `check:commands` → `command catalog OK: 13 commands`
- `check:errors` → `error registry OK: 58 codes across 2 protocols`
- `check:features` → `feature registry OK: 13 feature ids across 2 protocols`
- `check:assets` → `contract assets OK: 17 schemas, 213 fixture files, 12 transcript vectors …`
- `check:acp` → `ACP compatibility matrix OK: 25 methods, 11 updates, 5 content blocks, …`
- `check:docs` → `doc links OK: 415 relative links, 9058 section refs across 518 markdown files`
- `check:boundaries` → `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致`
- `check:drift` → `contract drift OK: §7 的 36 条 DDL … §5 的 15 个 trait / 96 个方法签名与 crates\core\src\ports.rs 一致`
- `check:agentic` → `Totals: 21 passed, 0 failed (21 items)`（worktree 内的 `node_modules` 目录联接由前轮建立、
  只读共享主检出，本轮未改）
- `check:rust` → `cargo fmt --all -- --check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`
  + `cargo test --locked --workspace --all-features`；**一次串行运行 exit 0**，95 个 `test result: ok`。
  `daemon_lifecycle.rs` **12/12 通过**（`the_periodic_task_runs_again_after_one_full_cycle` 在 60s 标记后 ok）——
  本轮 **未命中**任务书点名的非确定性 flake，因此未复跑。

### PV2 子检查逐条（`reports/PV2-wp3-r3.log`）

```
core lib: 189 passed; 0 failed（第 2 轮 179 → 189：本轮新增 derive 单测 6 条 + broker 单测 1 条，以及
                                  上一轮已计入的用例）
storage-sqlite: 各套件（admin_audit 15 / admin_store 43 / attachments 5 / commit 18(+1 ignored) /
                compaction_recovery 3 / contract_v03 5 / enum_coverage 2 / imported 10 / migration 12(+1 ignored) /
                permissions 4 / resume_columns 6 / retention 8 / session_version_rule 3 / title_write 4 /
                workspace_alias 6 / lib 3 / …）全部 0 failed
agent-host: lib 5 / bin 5 / catalog 22 / resume 7 / session 19 / supervision 15 / view_contract 1 等全部 0 failed
合计 27 个 test result: ok / 432 passed / 0 failed（两处 ignored 为既有 baseline，本包未触碰）
```

### 证据日志

| 检查 | 日志路径（变更目录相对） |
| --- | --- |
| PV1（`npm run verify`，串行 exit 0） | `reports/PV1-wp3-r3.log` |
| PV2 | `reports/PV2-wp3-r3.log` |

---

## 未验证内容

- **未执行 E2E**（`crates/app/tests/node_link_e2e.rs` 的 AC1 等）：不在 WP3 的写入范围，且 `[PV2]` 的
  `-p` 列表不含 `app`。
- **未验证** Windows 上的 symlink/junction 实机行为与 verbatim 前缀路径在**真实磁盘**上的端到端口径
  （`strip_verbatim` 的实机行为不在本轮改动面内）；UNC 的判定以词法单测与探针钉住，未接真实网络共享。
- **未判定** `crates/app/tests/daemon_lifecycle.rs` 的既有非确定性 flake（本轮未命中）。
- **不判**用例集合是否齐备（Coverage Index / validator）、不判 R9 的端到端组合根口径（属 main 的 AC1 收口项）。
- 本轮为取证复制过 TP2 的测试文件与临时探针（均未提交、已删除）；工作区与索引在本提交后干净
  （`git status --short` 为空）。

---

```agentic-handoff
version: 1
agent_context:
  agent_id: "coder-w3-r3"
  isolation: "fork_turns=none（新实现实例接管 .worktrees/integ 的 integ/mu2-wiring；未继承 coder-w3-r1/coder-w3-r2 的对话，只携带集成基线 3b6fe41、review-w3-r2 报告与 TP2 的红测）"
handoff_index:
  - task_id: "2.3"
    work_package: WP3
    role: coder
    phase: fix
    round: 3
    stage: work-package
    attempt: 3
    target_revision: "1550909b13bc59dd2ad635a45eff4de4e9699150"
    evidence_type: DELIVERY
    evidence_id: deliver-wp3-r3
    report_path: "reports/deliver-wp3-r3.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 .worktrees/integ（integ/mu2-wiring，base=3b6fe41）上修复 TP2 行为测试发现的两处产品缺陷，固定为提交 1550909（2 files changed, +296/−17）。R7：normalize 不再依赖 PathBuf::pop() 的返回值计数前导 `..`——改为返回 Normalized { path, escapes } 逐组件显式计数，Prefix/RootDir 入前缀并作废已累计的 ..，Normal 压栈，ParentDir 优先弹 Normal、栈空时按是否已 rooted 决定丢弃或 escapes+1；display_path 的相对分支按 escapes>0 判逃逸，并用新增的 join_lexically 折叠片段内部的 .. 使 sub/../file.txt 仍判区内。八个情形逐条覆盖并各带单测（含 Windows 盘符根与 UNC）。判别力：以复制 TP2 测试文件的临时探针在基线 3b6fe41 上实测 29/2（恰为 TP2 的两条红测）、在本提交上 31/0。R9：commit_chunk 新增 info_update_seen，只要本批含 session.info.changed 就产出一次 SessionUpdate（title: None = 不改该列），使 owned_session.updated_at 前进到 commit.at（design D7 / SYNC_PROTOCOL §10.2 的 updated_at DESC 排序），新增 broker 单测并以临时回退实证判别力。未改动四个已通过 review-w3-r2 的实现与 ports.rs。PV1 一次串行 exit 0（daemon_lifecycle 12/12，未命中 flake），PV2 exit 0。"
    source_evidence: "openspec/changes/sync-scope-and-pwa-client/reports/review-w3-r2.md"
  - task_id: "3.5"
    work_package: WP3
    role: coder
    phase: fix
    round: 3
    stage: work-package
    attempt: 3
    target_revision: "1550909b13bc59dd2ad635a45eff4de4e9699150"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/deliver-wp3-r3.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "npm run verify（cwd=.worktrees/integ，CARGO_TARGET_DIR=D:\\Project\\acp-remote\\.target-wt\\mu2int，Node v22.22.0 / npm 10.9.4 / rust-toolchain 1.98.1）在 1550909 上 exit 0：check 十道全绿（schemas 149/51/41、commands 13、errors 58、features 13、assets 17/213、acp 25/11、docs 415 links/9058 refs、boundaries 12 crates、drift 36 DDL + 15 traits/96 methods、agentic 21 passed）与 check:rust 三条全绿（fmt / clippy -D warnings / workspace test，95 个 test result: ok，daemon_lifecycle 12/12，一次串行运行无 flake）。"
    source_evidence: reports/PV1-wp3-r3.log
  - task_id: "3.5"
    work_package: WP3
    role: coder
    phase: fix
    round: 3
    stage: work-package
    attempt: 3
    target_revision: "1550909b13bc59dd2ad635a45eff4de4e9699150"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "reports/deliver-wp3-r3.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo test --locked -p core -p storage-sqlite -p agent-host --all-features 在 1550909 上 exit 0：27 个 test result: ok、合计 432 passed / 0 failed（core lib 189，含本轮新增的 6 条 derive 路径单测与 1 条 broker 单测；storage-sqlite 与 agent-host 各套件全绿，两处 ignored 为既有 baseline）。"
    source_evidence: reports/PV2-wp3-r3.log
checks:
  - id: PV1
    work_package: WP3
    command: "npm run verify (cwd=.worktrees/integ, CARGO_TARGET_DIR=D:/Project/acp-remote/.target-wt/mu2int)"
    exit_code: 0
    log_path: reports/PV1-wp3-r3.log
    result: PASS
  - id: PV2
    work_package: WP3
    command: "cargo test --locked -p core -p storage-sqlite -p agent-host --all-features (cwd=.worktrees/integ, CARGO_TARGET_DIR=D:/Project/acp-remote/.target-wt/mu2int)"
    exit_code: 0
    log_path: reports/PV2-wp3-r3.log
    result: PASS
test_delivery:
  WP3:
    kind: automated
    artifacts:
      - reports/PV1-wp3-r3.log
      - reports/PV2-wp3-r3.log
    basic_checks:
      - PV1
      - PV2
```
