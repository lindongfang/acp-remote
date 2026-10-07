<!-- MU2（Order 3，成员 WP3 + WP4 + TP2）候选合入前的**独立 merge 类型检视**，Round 1。
     Reviewer 只报告，不修改任何代码、测试、文档、规划文件、verification.md；不切换分支、不提交、不合并、不删除分支或 worktree。 -->

```agentic-handoff
version: 1
task_id: "6.18"
role: reviewer
phase: merge
agent_context:
  agent_id: "review-mu2-candidate-r1"
  isolation: "fork_turns=none（新建独立 reviewer，未参与 WP3/WP4/TP2 的实现、修复、用例编写、候选组装或任何前序检视对话；仅接收调度方传入的角色契约、必查重点与固定版本 SHA）。本轮为只读检视。"
target_revision: "5ba44fd35be2cd3717006c0995a8404c3315f388"
base_revision: "33040d78324ff51be49219fcb3aac054d0100cc9"
scope: "MU2 候选（mode=merge）自 base 33040d78 到候选 5ba44fd 的**合并结果**（26 files / +6788 / -44）。重点：A. WP3 第三轮修复 1550909 的独立复核（分 A.1 R7 路径归一化、A.2 R9 权威 updated_at）；B. 组合行为与冲突解决；C. AC1 与端到端闭合；D. 边界与冻结合同。三个成员各自已通过独立检视（review-w3-r1 F1 已由 1550909 修复、review-* PASS），本报告不重复其内部正确性。"
changes: "只读检视，未修改任何文件；未切换分支、未提交、未合并、未删除分支或 worktree、未运行构建/测试/E2E/PV1/PV2（仅在 .target-wt/probe/ 下用 rustc 编译并运行了一个**独立的** normalize 探针程序，未触碰任何被检视文件）。仅新增本报告。"
issues: "0 CRITICAL / 0 MAJOR / 3 MINOR（F1 P2 文档断言与实现不符、F2/F3 各 P3）/ 3 INFO"
result: PASS
```

# MU2 候选独立检视（merge 类型，Round 1）

## 0. Review Context

| 项 | 取值 |
| --- | --- |
| Review ID / Round | `review-mu2-candidate-r1` / 1（该候选首个 merge 类型检视） |
| Review Type / Stage | `merge` / 候选准备完成、premerge 之前 |
| Delivery Unit | MU2（成员 WP3 = R5–R9 实现、WP4 = agent-host Diff 透传与进程生命周期、TP2 = core 生产者行为判定） |
| Repository | `D:\Project\acp-remote` |
| Base Revision | `33040d78324ff51be49219fcb3aac054d0100cc9`（`refs/heads/main`） |
| Target Revision | `5ba44fd35be2cd3717006c0995a8404c3315f388`（`integ/mu2-wiring` tip） |
| 检视工作区 | `D:\Project\acp-remote\.worktrees\integ`（只读；`git status --porcelain` 为空，HEAD == target） |
| 实际修改文件 | 26 files（`git diff --stat 33040d7 5ba44fd`）：`crates/core/**`（derive.rs 新、broker.rs、lib.rs、model/json.rs、model/mod.rs、ports.rs）、`crates/storage-sqlite/**`（src/session_store.rs、tests/ 9 文件含 2 新）、`crates/agent-host/**`（node.rs 新、host.rs、mapper.rs、process.rs、lib.rs、bin/、tests/session.rs）、`crates/app/**`（compose.rs、tests/node_link_e2e.rs）、`docs/CORE_PORTS_AND_STORAGE.md` |
| 读取的规则与需求 | `specs/core-derived-events/spec.md`（R5–R9 全文）、`specs/workspace-resolution/spec.md`（R7）、`specs/local-agent-host/spec.md`（R8）；`design.md`（D4–D7）、`plan.md`（AC1 行 :996/:1029）、`tasks.md`（4.1）、`AGENTS.md` §4/§9；`docs/SYNC_PROTOCOL.md` §10.2/§10.3、`docs/CORE_PORTS_AND_STORAGE.md` §5.2/§5.4/§6 第 19/21 条 |
| 验证证据（只读消费） | `reports/PV1.log`、`reports/PV2.log`、`reports/receipt-mu2.md`、`reports/review-w3-r1.md`、`reports/review-tp2-r1.md`、`reports/review-wire-r1.md`、`reports/review-ac1-r1.md`、`reports/deliver-wp3-r3.md`、`reports/wire-compose-nodeevents-r1.md`。**执行者与 merger 的自评不作为检视证据。** |
| 限制 | 不重跑 PV1/PV2/AC1；不重复 WP3/WP4/TP2 各自已通过的内部正确性检视；不评审前端/PWA 与 `server::sync` |

### 0.1 结论总览

| 编号 | 严重度 | 位置 | 摘要 |
| --- | --- | --- | --- |
| `review-mu2-candidate-r1-F1` | **MINOR（P2）** | `crates/core/src/ports.rs:1122-1138`、`docs/CORE_PORTS_AND_STORAGE.md:28/808/825/886` | `NodeEventSink` 是**死类型**：文档（含本变更新增的 §5.4 `[决定]`）断言「`agent-host` 用 `NodeEventSink` 交付节点级事件」，但 `agent-host` 与组合根实际都走通用 `EventSink`，类型级隔离在任何生产路径上都不存在 |
| `review-mu2-candidate-r1-F2` | MINOR（P3） | `crates/app/tests/node_link_e2e.rs:1168`（对照 :1296 与 :1503/:1676） | AC1 节点级用例的文档注释承诺「其余会话级事件仍会出现，见下方的正向对照」，但同函数 128 行后的注释写的是「本场景不产生会话级事件」，且 T2/T3 的扇出接收端被命名为 `_fanout` 后从未读取——**正向对照不存在** |
| `review-mu2-candidate-r1-F3` | MINOR（P3） | `crates/storage-sqlite/tests/derived_events_behaviour.rs:1532-1535` | R9 判别用测试的注释把自己的基线说成「上一次**写入**的取值」（实为上一次**事件**的 `at`）；判别力不受影响，但注释与事实不符 |

**未发现 CRITICAL / MAJOR。** A（A.1 + A.2）、B、C、D 四项必查重点逐条给出独立结论（见 §1–§4）。三处 MINOR 均不阻断合入，按判定规则不要求清零。

---

## 1. A. WP3 第三轮修复 `1550909` 的独立复核

`1550909`（"fix(core): WP3 第三轮——前导 `..` 显式计数修 R7 越界泄露，通知只带更新时间时推进权威 updated_at"）是 `5ba44fd` 的祖先（`git log 33040d7..5ba44fd` 第 5 条）。它**从未经过独立的 branch 类型复核**（此前只有修复者 `deliver-wp3-r3.md` 自评与 TP2 的红→绿轨迹间接验证）。本节是它的首次独立复核。

### A.1 R7——前导 `..` 越过根时丢计数（`crates/core/src/derive.rs` 的 `Normalized`/`normalize`/`display_path`）

**结论：修复正确、完整，未发现新逃逸形态；`displayPath` 满足 R7 的「只给文件名、不含回退层级或根的任何片段」。**

#### A.1.1 计数逻辑在各路径形态下的正确性——**逐形态实测通过**

`normalize` 改为逐组件显式计数（`derive.rs:415-450`）：

- `Prefix` → 进 `base`；`RootDir` → 进 `base`、`rooted = true`、`escapes = 0`（一次性作废旧计数）；
- `CurDir` → 忽略；`Normal` → 压栈；`ParentDir` → 先弹栈顶 `Normal`，栈空且未 `rooted` 时 `escapes += 1`（栈空且已 `rooted` 则丢弃）。

我用一个**独立的**探针程序（`rustc` 编译，复制该 `normalize` 实现，运行在 win32）逐形态实测（`root=/work/api` 的路径形态以字符串给出；Windows 形态走真实 `Prefix`/`RootDir` 分支）：

| # | 形态 | 输入 | 实测 `path` / `escapes` | 判定 |
| --- | --- | --- | --- | --- |
| 1 | 普通相对路径 | `src/main.rs` | `src\main.rs` / 0 | 区内 ✓ |
| 2 | 区内含 `..` 未越界 | `sub/../file.txt` | `file.txt` / 0 | 区内 ✓ |
| 3 | 区内多级 `..` | `a/b/../../c.rs` | `c.rs` / 0 | 区内 ✓ |
| 4 | 前导 `..` 越界 | `../etc/passwd` | `..\etc\passwd` / **1** | 越界 ✓ |
| 5 | 前导 `..` 越界（R7 动机输入） | `../../etc/passwd` | `..\..\etc\passwd` / **2** | 越界 ✓ |
| 6 | 多级 | `../../../../a/b/c` | `..\..\..\..\a\b\c` / **4** | 越界 ✓ |
| 7 | `..` 抵消前导段 | `a/../../b` | `..\b` / **1** | 越界 ✓ |
| 8 | 路径即根 | `a/..` | `""` / 0 → 空路径分支 → 越界 | ✓ |
| 9 | 绝对路径含内部 `..` | `/work/api/../api/x` | `\work\api\x` / 0 | 区内（前缀比较） ✓ |
| 10 | 前缀 + 根（盘符） | `C:\..\..\etc\passwd` | `C:\etc\passwd` / **0** | 根段 `..` 被吞、根保留 ✓ |
| 11 | UNC 份额根（`#[cfg(windows)]` 单测） | `\\server\share\..\..\x` | 根保留 / 0 | 按同口径 ✓ |
| 12 | 相对路径恰好等于根 | `..` / `../..` | `..` / 1、`..\..` / 2 | 越界 ✓ |

关键点：**第 10 项是旧实现的病根**（`PathBuf::pop()` 弹掉 `Prefix`/`RootDir` 时返回 `true`）——修复后 `rooted` 为真，`ParentDir` 只弹 `Normal` 栈，根段 `..` 一律吞掉且 `escapes` 恒为 0，盘符根不再被弹掉。`escapes > 0` 时 `normalize` 返回的路径**仍带前导 `..`**（畸形、只用于越界判定，不进入前缀比较），`display_path` 因此直接走 `outside_display`（`derive.rs:311-313`），不下发该路径。

#### A.1.2 是否存在**新的**逃逸形态——构造性尝试未发现

我针对修复引入的新结构逐一构造：

- **「计数非零但被后续 `RootDir` 作废」**：`..\..\C:\x` 在 Windows 上 `C:` 是 `Prefix`（非 `RootDir`），作废只发生在 `RootDir`。此时 `escapes` 仍为 1、路径为 `..\..\C:\x`（含前导 `..`）→ `display_path` 判越界并只给 `x`，**从严**，不构成泄露。✓
- **「栈内 `Normal` 被 `RootDir` 之前的前导 `..` 误抵消」**：`Normal` 只能弹**它前面**的 `..`；前导 `..` 在 `stack` 为空时计数，不进入栈，因此不会被后续 `Normal` 抵消。✓
- **相对分支的 `join_lexically` 二次折叠**（`derive.rs:357-362`）：片段已保证无前导 `..`（先查 `escapes`），拼接后若仍能折叠出前导 `..` 则只可能来自根自身的 `..`——`canonical_path(root)` 已规范化，故不可能。`strip_prefix` 失败即回退越界。✓
- **绝对分支的 `reported_canonical == root_path`**：返回 `(".", true)`。

**关于根正好相等时返回 `(".", true)` 的 spec 判断**：spec 的两条要求可同时满足——`workspace-resolution` R7 场景「工作区之外的派生路径被标记且不回退」要求越界时「下发的展示值**只含该文件名称**」；根自身无文件名，`. ` 是 R7「MUST NOT 含根的任何片段或回退层级」下的合法哨兵。仓库内该语义由 `derive.rs` 的单测 `display_path_treats_the_workspace_root_itself_by_the_spec` 钉住。**判定：可接受，且与 spec 一致**（INFO，非 finding）。

#### A.1.3 `display_path` 返回值与 R7 的逐条对齐——**通过**

| R7 要求（`specs/core-derived-events/spec.md` 与 `workspace-resolution/spec.md`） | 实现 | 结论 |
| --- | --- | --- |
| 由工具调用给出的路径相对化，MUST NOT 直接下发绝对路径 | 绝对分支 `canonical_path` + `strip_prefix` → `relative_text`（只取 `Component::Normal`） | ✓ |
| 区内展示值不含根的任何片段、不含回退层级 | `relative_text` 跳过 `RootDir`/`Prefix`/`ParentDir` | ✓ |
| 越界时显式标记且展示值只含文件名称 | `outside_display` 只取 `file_name()`，无名回退 `.` | ✓ |
| 判定基于**规范化后的前缀关系**，MUST NOT 仅按字符串前缀 | `Path::strip_prefix` 组件级比较；`display_path_does_not_treat_a_string_prefix_as_inside` 钉住 `/work/api` vs `/work/api-tools` | ✓ |
| 前导 `..` 逃出根 → 越界 | `escapes > 0` 直接越界 | ✓ |

**A.1 结论：PASS。** 未发现新逃逸形态，`displayPath` 严格符合 R7。

### A.2 R9——`updated_at` 的权威来源（`derived::title_intent` + `broker::commit_chunk` + `session_store` 的 `UPDATE`）

**结论：正确。权威时间确实取自 Daemon 提交时钟（`commit.at`），`version` 递增未丢失，且「每次 `session.info.changed` 递增版本」是**显式设计决策**而非缺陷。**

逐条核实：

1. **权威时间 = Daemon 提交时钟**。`commit_chunk` 每批只读一次时钟：`let at = self.now();`（`broker.rs:2021`，→ `self.deps.clock.now()`），原样进 `OwnedCommit { at: at.clone(), .. }`。存储层两条 `UPDATE owned_session` 语句都写 `version = version + 1, updated_at = ?3`（`session_store.rs:1143` / `:1154`），而 `?3` 是绑定链的第三项 `.bind(commit.at.as_str())`（`session_store.rs:1162`）。**事件的 `at` 与 Agent 自报的 `updatedAt` 都不进该列**：前者只进 `PendingEvent`/`ended_at`，后者只在 view 里转发（`event_view["updatedAt"]`，由 `agent-host` 的 mapper 构造）。✓

2. **`version` 递增未丢失**。两条语句的 `version = version + 1` 均无条件、无 `CASE`、无额外谓词。`info_update_seen`（`broker.rs:2046`）置真后，`broker.rs:2241` 的三析取 `session_state.is_some() || title_update.is_some() || info_update_seen` 恒成立 → 必产出 `Some(StateChange::Update(..))` → 存储层必写。`expected_version: None`（不会被拒）、`idempotency: None`（不会被 `replayed` 吸收）。这正是 `review-w3-r1-F1`（旧实现「显式置空」走窄语句丢 `version+1` 与 `updated_at`）的修复面：现在标题三态用**同一个 `CASE` + 哨兵参数**表达（`title = CASE WHEN ?9 THEN NULL WHEN ?8 IS NULL THEN title ELSE ?8 END`），与其余列共用**同一条**语句。✓

3. **`info_update_seen` 不被 `events.is_empty()` 短路**。`persistence_policy("session.info.changed")` 落到 `_ => Durable`（不在 `EPHEMERAL_EVENT_TYPES`，也不在 `ShortTerm` 表）→ `PendingEvent::from_persistence` 返回 `Some` → `events` 非空 → `broker.rs:2183` 的早退不触发。仅带更新时间（`title_intent` 返回 `Unchanged`、`title_update` 保持 `None`）的批次仍产出 `SessionUpdate { state: None, mode: Unchanged, title: None, .. }`，即「只推进 `updated_at`、只递增 `version`、不改其它列」。✓

4. **「每次 `session.info.changed` 都递增版本」是否缺陷——判定：非缺陷（显式设计决策）**。三重书面依据：`docs/CORE_PORTS_AND_STORAGE.md` §6 第 21 条 R9 项（`:887`，本变更新增）明文「**`updatedAt` 取 Daemon 持久化时间**（`commit.at`，由存储层写进 `owned_session.updated_at`）」；`design.md` D7 明文「**`updatedAt` 不采用 Agent 自报值**……会话的权威更新时间取 Daemon 持久化时间」；`broker.rs:2042-2047` 的作者注释说明「只要本批含该事件，就必须产出一次 `SessionUpdate`，让存储层照常写 `updated_at` 与 `version`」。而「含 `StateChange` 的提交为当前版本 + 1」是**先于本变更**的 §6 第 19 条规则。要让权威 `updated_at` 前进到 `commit.at`，就**必须**写一次会话行，而写会话行在既有规则下必然递增版本——两者不可分离。Agent 可驱动的版本增长上界存在（`predict_session_version` 对溢出返回 `Corrupt`），且每次通知**在本变更之前就已**落一条 durable `owned_event`，本变更未新增无界行来源。（INFO-1，非 finding。）

5. **无双重递增**。`commit_chunk` 每次调用只构造**一个** `Option<StateChange>`（`broker.rs:2241-2254`）、只发一次 `OwnedCommit`（`:2255-2265`）→ 一次 `UPDATE` → 一次 `+1`。真实 sink 路径下终态事件由 `flush_locked` **单独成批**（`vec![event]`），因此「info.changed 与终态同批」在真实路径不发生；而存储层的合批情形由 `derived_events_behaviour.rs` 的 `clearing_a_title_in_a_batch_with_a_version_event_keeps_the_whole_batch` 钉住（一次递增、注入版本等于提交后版本）。`crates/storage-sqlite/src/session_store.rs` 全文只有三处 `updated_at` 写（创建 `INSERT` 与两条 `UPDATE`），`commit_chunk` 侧只有一个时间源。✓

6. **判别测试成立**。`broker.rs:9375-9415` 的 `a_session_info_update_without_a_title_still_advances_the_updated_at` 把通知事件自身 `at` 设为**早于**上一次写入、ACP 原文为 `2020-01-01T00:00:00.000Z`，并断言 `updated_at != "2020-…"`（排除 Agent 自报值）**且** `updated_at > before`（排除事件 `at`）——两种错误来源各被一条独立断言判红。`derived_events_behaviour.rs:1524-1558` 在真实 SQLite 上以同一构造复现（第二次事件 `at = stamp(1)` 早于第一次的 `stamp(51)`，断言 `after > after_first` 且不以 `2020-` 开头）。`unchanged_mode_without_a_title_keeps_it_and_still_advances_the_version` 更强：直接断言 `updated_at == stamp(N)`（等于它自己提供的 `OwnedCommit.at`）。✓

（F3 记的是 `derived_events_behaviour.rs:1532-1535` 的**注释**把基线误说成「上一次**写入**的取值」——它实际是上一次**事件**的 `at`；判别力不受影响，因为若取事件时间，该列会从 `00:51` 倒退到 `00:01` 而被 `after > after_first` 判红。）

**A.2 结论：PASS。** 权威时间确实取自 Daemon 提交时钟；`version` 递增未丢失；「每次通知递增版本」是显式设计决策，非缺陷。

---

## 2. B. 组合行为与冲突解决

### B.1 `crates/core/src/ports.rs` 的共享写点——**两侧无丢失**

**前提修正（基于 `git show` 三方比对的事实）**：WP4 tip `4c4990a` **从未写过** `ports.rs`——其 `ports.rs` 与 base `33040d7` 都是 1121 行、每一项的声明行号逐条相同，且都**没有** `title` 字段与 `NodeEventSink`。WP4 实际消费的端口面是**早已存在的** `EventSink`。因此这不是「两写者合并」，而是**单侧写入**，`git merge feat/wp4`（`559f21c`）在这些文件上根本没有冲突可解。

最终形态（`5ba44fd` 的 `ports.rs` 与 WP3 tip `6bf7a7c` 逐字相同、1160 行）**同时**包含：

- WP3 的两层可选标题：`pub title: Option<Option<String>>`（`ports.rs:189`，三态语义见 `:180-188`）；
- 端口面新增类型 `NodeEventSink`（`ports.rs:1122-1138`）。

`crates/core/src/lib.rs`（新增 `mod derive;`）与 `crates/core/src/model/mod.rs`（`pub(crate) use json::{.., array_items, .., json_object_members, ..}`）也都是 WP3 的形态，WP4 未触碰。**B.1 结论：无任何一侧丢失。**

### B.2 7 个既有测试文件的 19 处机械加法——**纯加法，未改断言或用例语义**

`git diff --numstat 33040d7 5ba44fd -- crates/storage-sqlite/tests/` 逐文件核对：每个文件的新增行数**恰好等于**其新增 `title: None,` 行数（commit.rs 7、contract_v03.rs 1、enum_coverage.rs 1、migration.rs 1、retention.rs 2、session_version_rule.rs 3、workspace_alias.rs 4 = **19**），且全目录删除行数为 **0**。这 7 个文件的 `fn`/`#[tokio::test]`/`assert`/`const`/SQL 指纹在 base 与候选之间除行号位移外完全一致，每个新增行都紧跟在 `mode: ModeChange::Unchanged` 之后、位于 `StateChange::Update(SessionUpdate { .. })` 字面量内；既有 `NewSession { title: None }` 出现处未被触碰。**B.2 结论：只加了字段，未改任何断言或用例语义。**（该编辑是 `SessionUpdate` 增字段后的编译强制连带修改，plan 的测试目录登记缺口已由 main 裁决授权并登记。）

### B.3 `title_write.rs` 与 `derived_events_behaviour.rs` 的同目录共存——**互不冲突、互不重复计数**

- **无同名测试函数**（两者也是不同集成测试二进制，命名冲突不构成编译错误，但确实无重名）。
- **覆盖面互补而非重复**。决定性事实是两者能到达的 SQL 分支不同：`title_write.rs` 的 4 条用例**全部**用 `ModeChange::Set`（即 `?10/?11` 语句）；`derived_events_behaviour.rs:1641` 与 `:1737` 是全仓库**仅有的**两条覆盖 `ModeChange::Unchanged` + 标题（即 `?8/?9` 语句）的用例。这也解释了 TP2 判定 `title_write.rs` 覆盖不足并在**自己文件里**补齐的处置——该补齐把缺口补上，且**未**改 `title_write.rs`（该目录区域注记只允许新增文件）。
- **唯一的重叠是有意的分层覆盖**：「标题在含版本事件的批次里被置空不得丢」在 `title_write.rs:227-311`（`Set` 语句）与 `derived_events_behaviour.rs:1589-1632`（经 broker 全链路）各断言一次，属同一缺陷形状在两个入口的分层验证，**不是**对同一行为的重复计数。
- **计数常量无冲突**。`title_write.rs` 硬编码 `version == 2`，但每条用例在自己的 `temp_dir` 里先 `create_session`；`derived_events_behaviour.rs` 一律用相对断言（`version == before + 1`、注入版本 == 存储版本、`> 1`），其事件计数断言各自限定在自己的会话/夹具目录内。两者**不共享**夹具、常量或计数器（`support::temp_dir` 按名 + pid 命名空间化）。
- **无矛盾期望**：两文件**都**断言 `updated_at` 前进（`title_write.rs:211/262-266/340/385` 等于 `commit.at`；`derived_events_behaviour.rs:1543-1548/1549-1557/1688-1692/1810-1814` 前进且非 Agent 自报值）。全仓库无任何断言 `updated_at` 保持不变。

**B.3 结论：无重复计数、无矛盾期望。** TP2 的补齐未造成同一行为被两处重复断言，也未与 `title_write.rs` 冲突。

---

## 3. C. AC1 与端到端闭合

### C.1 `crates/app/src/compose.rs` 只装配、不承载业务规则——**通过**

逐项核实新增代码（`node_event_sink` `:939-975`、`assemble` 的接线 `:307-312`、`close` 的排空屏障 `:469-522`）：

- **无事件类型白名单、无 `state` 词表校验、无 `agentId`/kind/turn/ACP 检查**——全部在 `core` 的 `Broker::commit_node_event`（`broker.rs:640-706`）；
- **无重试、无降级、无回退写入路径**：`commit_node_event` 失败即记一条 `error` 级日志（`port_error_token` 收敛成因、不回显可能含 SQL/路径的内层文本），不重试、不写替代路径；
- **不构造会话标识、不伪造游标**：闭包只 `Weak::upgrade` + `spawn`；
- `close` 的排空与 `NodeEventsPending`（5 s 上限、超时如实上报）属**装配生命周期**，非业务规则。

`close` 的强引用环也确已断开（sink 持 `Weak<Broker>`，`compose.rs:309/940`），否则 `Arc::try_unwrap(store)` 会以 `StoreStillShared` 失败、破坏正常关闭。**C.1 结论：只装配，无业务规则。**

### C.2 AC1 的四条 plan 判据逐条有对应且可证伪的断言——**通过**

`plan.md:996`（Project Verify 行）与 `:1029`（替代检查表行）逐字列出**四项**断言（外加明文的「不含广播断言」排除项，属作者决策，非缺口）。映射：

| plan 判据（plan.md:996/:1029 原话） | 候选断言（file:line） | 可证伪性 |
| --- | --- | --- |
| A1 `file.changed`（会话级）经 owned 路径**落库**并按 **origin 顺序回放** | 落库 `node_link_e2e.rs:1512-1545`（2 行、`session_id` 非空、两套游标齐备）；origin 序 `:1558-1583`（`seqs == 1..=N`、`ORDER BY origin_sequence`）+ `:1585-1599`（内部位置 + 紧邻 `tool.call.*`）+ `:1602-1644`（游标两侧续读） | ✓ 非空转：`:1578` 先断言 `seqs.len() >= 4`；`:1590` 的内部位置断言能抓「派生事件被追加在批尾」；`:1594` 的邻接断言能抓「派生被挪到另一次提交」 |
| A2 `agent.connected`（节点级）落库后可按事件库读回且会话标识为空 | `node_link_e2e.rs:1226-1279`（直读真实 SQLite，恰 1 行；`session_id`/`session_sequence`/`origin_epoch`/`origin_sequence` 四者皆 NULL） | ✓ 直接查列 NULL 性；存储若回填 `origin_sequence` 即失败 |
| A3 会话级投递路径不误收它 | `node_link_e2e.rs:1282-1304`（排空 `forked_publisher()` 实际注入的扇出接收端，断言无 `agent.*`） | ✓ 实质断言 `:1297` 可证伪（若 `NodeLinkPublisher::publish` 的 `session.is_none()` 早退被移除即失败）；弱断言 `:1301` 见 F2 |
| A4 会话标题在 `session_info_update` 后更新 | `node_link_e2e.rs:1682-1697`（`owned_session.title`/`version`）+ `:1699-1728`（`session.info.changed` 投影）+ `:1741-1760`（`list_sessions` 摘要） | ✓ 全部读真实落盘状态；`ac1_prompt_once` 已要求 `turn.completed` 落盘，非空数据上通过 |

**无 plan 判据缺少对应断言。** 候选的「六项断言」= A1–A4 + 两项**超额**（R32 的 `agent.disconnected` 落库 `:1318-1326`、`session.info.changed` 投影 `:1699-1720`），是超出 plan 的补充而非缺口。

### C.3 节点级事件与会话级事件的落库隔离——**成立**

生产侧：`Broker::commit_node_event` 提交 `OwnedCommit { session: None, .. }` → 存储层 `session_id` 为 `None` 时四列取 `(None, None, None, None)`（由 §7.3 的成对 CHECK 约束）→ `publish` 包成 `CommittedDelivery::Owned`。
消费侧（**diff 之外**，已按要求读取）：`crates/server/src/node_link/resource.rs:1029-1032` 的 `NodeLinkPublisher::publish` 对 `event.session.is_none()` **显式 `return`**（不归属到任意会话、不伪造会话级游标），`ResourceRoute::fan_out`（`:152-155`）重复该守卫。两侧**一致**：节点级事件落库并进入全局 replay，但**不**被会话级投递路径承接。**无静默错投、无中途丢弃。** `CommittedDelivery::Owned` 对节点级事件确实 `session_sequence`/`origin_epoch`/`origin_sequence` 全为 `None`，仅 `global_sequence` 有值。**C.3 结论：落库隔离成立。**

---

## 4. D. 边界与冻结合同

| 判据 | 结论 | 依据 |
| --- | --- | --- |
| `crates/core` 依赖闭包仍为 `async-trait`/`thiserror`/`p256`(仅 `arithmetic`)/`sha2` | **PASS** | `crates/core/Cargo.toml` 与 base 逐字节相同；`git diff 33040d7 5ba44fd -- crates/core/Cargo.toml` 为空；`Cargo.lock` 的 `core` 块两侧一致；workspace 根 `p256` 仍为 `default-features = false, features = ["arithmetic"]`；`derive.rs` 的唯一外部依赖是 `sha2` |
| 行级 Myers diff 仍为自实现 | **PASS** | `derive.rs:96-131` 手写（论文 Algorithm 1 贪心前向搜索）；`Cargo.lock` 无 `similar`/`imara-diff`/`diff` 等。我另行审读了索引与预算：`index ∈ [1, 2·max+1]` 恒在 `v.len() = 2·max+3` 内；`budget.checked_sub(1)?` 两处耗尽即返回 `None`（省略两项而非填零）；`LINE_STAT_MAX_LINES` 在分配前检查；`N=0`/`M=0` 的返回值正确（并被 `tiny_sequences` 的 15×15 全对拍覆盖，含 `max=0`） |
| `crates/sync-protocol/**`、`schemas/**`、`fixtures/**` 零改动 | **PASS** | `git diff --stat 33040d7 5ba44fd -- crates/sync-protocol/ schemas/ fixtures/ vendor/` **为空** |
| `crates/agent-host/**` 未 import `sync-protocol` | **PASS** | `crates/agent-host/Cargo.toml` 与 base 逐字节相同、无 `sync-protocol` 边；`git grep sync[_-]protocol` 在 agent-host 全目录**只命中文档注释**（`node.rs:208`、`mapper.rs:6/67`），零代码引用 |
| 无新 crate 依赖或 migration | **PASS** | 根 `Cargo.toml` 的 12 个 workspace 成员两侧一致；无新增 `crates/*/Cargo.toml`；`crates/storage-sqlite/src/migrate.rs` 两侧逐字节相同，版本常量 `FILE_FORMAT_VERSION=6`/`OWNED_SCHEMA_VERSION=6`/`IMPORTED_SCHEMA_VERSION=3` 未变；`session_store.rs` 无任何 DDL/`PRAGMA user_version` 写（唯一出现是 `health()` 的读） |

**D 结论：四项全部 PASS。**

---

## 5. Findings

### `review-mu2-candidate-r1-F1`（MINOR / P2）`NodeEventSink` 是死类型：文档断言的类型级隔离在任何生产路径上都不存在

- **位置**：`crates/core/src/ports.rs:1122-1138`（声明）；对照 `crates/agent-host/src/node.rs:54-55`、`crates/app/src/compose.rs:939-944` 与 `docs/CORE_PORTS_AND_STORAGE.md:28/808/825/886`。
- **触发条件**：任何使用节点级事件路径的构建。
- **预期 / 实际**：`ports.rs:1118-1121` 的文档注释与 `docs/CORE_PORTS_AND_STORAGE.md:825`（本变更新增的 §5.4 `[决定]`）都断言「**节点级事件不走 `EventSink`**……`agent-host` 用 `NodeEventSink` 交付**已构造好的 `EndpointEvent`**」，并且「单独成一个类型……是为了让『节点级事件不得进入会话槽位』在类型上可见」。**实际**：`agent-host` 的 `NodeEvents` 持有 `sink: Option<EventSink>`（`node.rs:54-55`，由 `EventSink` 构造于 `:81`），组合根的 `node_event_sink(...)` **返回** `EventSink`（`compose.rs:939-944`）并被注入（`:308-312`）。全仓库对 `NodeEventSink` 的引用**只有** `crates/core/src/ports.rs` 的声明与文档——无生产者、无消费者、无测试。
- **影响**：无运行时行为变化（`Broker::commit_node_event` 仍做全部校验）。失去的是协议文档对外宣称的不变量：两个类型都是 `Arc<dyn Fn(EndpointEvent)>` 的 newtype、形状完全同构，因此在类型上**无法**阻止未来的 `NodeEvents` 接线被喂给 `Broker::sink(session)`——正是 `docs:825` 声称该类型存在以使其不可表达的那种错归属。`scripts/check-contract-drift.mjs` 无法发现该分歧（它只比对 §5 的 rust 代码块与 `ports.rs`，而两处都声明了 `NodeEventSink`，比对通过）。
- **建议**：三处取齐其一——(a) 让 `agent_host::NodeEvents` 持 `acp_core::ports::NodeEventSink` 并让 `compose::node_event_sink` 返回它；或 (b) 从 `ports.rs` 删除 `NodeEventSink`，并把 `docs/CORE_PORTS_AND_STORAGE.md:28/808/825/886` 改为「节点级事件复用 `EventSink` 的形状，仅由 `Broker::commit_node_event` 的校验区分」。**修与不修都需在合入前定论**，因为当前文档断言与实现不符。

### `review-mu2-candidate-r1-F2`（MINOR / P3）AC1 节点级用例的注释承诺了一个不存在的正向对照，其空队列断言为空转

- **位置**：`crates/app/tests/node_link_e2e.rs:1168`（对照同函数 `:1296`，以及 `:1503` / `:1676` 的 `_fanout`）。
- **触发条件**：按注释阅读该用例。
- **预期 / 实际**：`:1168` 写「因此扇出队列里**不出现**任何节点级事件（**其余会话级事件仍会出现，见下方的正向对照**）」。**实际**：同函数 `:1296` 写的是**相反**的话——「本场景（`normal`）不产生会话级事件，队列应恰为空」；且「下方的正向对照」并不存在：T1 场景是 `normal`，`create_session` 的两次提交都 `events: Vec::new()` 且不 `publish`，唯一发布的是节点级事件（被 `session.is_none()` 丢弃），故扇出队列**零事件**；T2/T3 虽绑定接收端却命名为 `_fanout` 并从未读取（`:1503`、`:1676`）。
- **影响**：`seen.is_empty()`（`:1301-1304`）无法区分「发布者正确丢弃了节点事件」与「发布者从未被触达 / 通道已死」；实质断言 `:1297`（无 `agent.*`）本身可证伪、故 AC1 整体仍成立，但证据质量与一句事实错误的注释需要修正。
- **建议（最小，仅改注释）**：把 `:1168` 的括号内容改为「本场景 `normal` 不产生会话级事件，队列应恰为空（见 `:1296`）」，或删除「见下方的正向对照」半句。**建议（更优，增加真对照）**：在 T2 或 T3（`chunked-updates` 必产生会话级事件）把 `_fanout` 改名并断言该接收端至少收到一条非 `agent.*` 事件、且零 `agent.*` 事件——这一处补充即可让 `:1168` 的判断为真。

### `review-mu2-candidate-r1-F3`（MINOR / P3）R9 判别测试的注释把自己的基线说成「上一次写入的取值」

- **位置**：`crates/storage-sqlite/tests/derived_events_behaviour.rs:1532-1535`。
- **触发条件**：阅读该用例的判别理由注释。
- **预期 / 实际**：注释称第二次通知的 `at`「早于**上一次写入**的取值（`stamp(1)` < `stamp(51)`）」。`stamp(51)` 是**上一次事件**的 `at`，而非上一次写入的 `updated_at`——后者的来源是夹具的 `MonotonicClock`（首次 broker 提交读到 minute 0，故为 `stamp(0)`）。
- **影响**：判别力**不受影响**（若实现取事件时间，该列会从 `00:51` 倒退到 `00:01`，`after > after_first` 仍会判红）；仅注释与事实不符，会误导后续读者重建该用例的判别逻辑。
- **建议**：改写为「上一次**事件**的 `at` 为 `stamp(51)`；若取事件时间，该列会从 `00:51` 倒退到 `00:01`」。

---

## 6. INFO（非发现，记录备查）

- **INFO-1（设计决策，非缺陷）**：每次 `session.info.changed`（含只带更新时间、`Unchanged` 的那些）都会递增 `version`。这是让权威 `updated_at` 前进到 `commit.at` 的**必要**代价（写会话行在既有 §6 第 19 条规则下必然 `+1`），且 §6 第 21 条 R9 项、`design.md` D7、`broker.rs:2042-2047` 三处明文如此。已确认 Agent 可驱动的版本增长有上界（溢出 → `Corrupt`），且本变更未新增无界行来源。
- **INFO-2（设计决策，非缺陷）**：工作区根正好等于被报告路径时返回 `(".", true)`。可接受且与 R7 一致（`. ` 是不含根片段、不含回退层级的合法哨兵），由 `derive.rs` 单测钉住。
- **INFO-3（覆盖缺口，非缺陷）**：`LINE_STAT_MAX_LINES = 200_000` 的硬上限在仓库内**无任何测试**（`grep 200_000` 在 `crates/core/src` 只命中常量本身）；预算耗尽路径由 `derived_events_behaviour.rs` 覆盖，但上限路径没有。行为正确，仅缺回归防护。

---

## 7. 结论

**PASS** —— 无 CRITICAL / MAJOR。

- **A.1（R7）**：PASS。修复正确完整；逐形态（相对/绝对/盘符根/UNC/区内含 `..`）实测与源码推导一致，未发现新逃逸形态；`displayPath` 严格符合 `core-derived-events` R7 与 `workspace-resolution` R7；根正好相等返回 `(".", true)` 属可接受语义。
- **A.2（R9）**：PASS。权威时间取自 Daemon 提交时钟（`commit.at` → `updated_at = ?3`），非事件自报时间或 Agent 声明；`version` 递增未丢失；「每次通知递增版本」为显式设计决策，非缺陷；无双重递增。
- **B**：PASS。`ports.rs` 两侧无丢失（实为单侧写入）；19 处为纯加法、未改断言语义；两测试文件互不冲突、互不重复计数。
- **C**：PASS。组合根只装配；四条 plan 判据逐条有可证伪断言；节点级/会话级落库隔离成立且两侧一致。
- **D**：PASS。依赖闭包、自实现 Myers、冻结合同零改动、agent-host 边界、无新依赖/迁移，五项全部成立。

三处 MINOR（F1 P2、F2/F3 P3）均不阻断合入。**F1 需在合入前定论修法**（改实现或改文档），因其为「文档断言与实现不符」而非纯风格问题；F2/F3 建议随附修复。

```agentic-handoff
version: 1
agent_context:
  agent_id: "review-mu2-candidate-r1"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "6.18"
    work_package: MU2
    role: reviewer
    phase: merge
    round: 1
    stage: candidate
    target_revision: "5ba44fd35be2cd3717006c0995a8404c3315f388"
    evidence_type: REVIEW
    evidence_id: review-mu2-candidate-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/review-mu2-candidate-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在候选 5ba44fd35be2cd3717006c0995a8404c3315f388（基线 33040d78）上只读检视 MU2 的 26 文件合并结果。A：WP3 第三轮修复 1550909 首次独立复核——R7 前导 .. 显式计数在相对/绝对/盘符根/UNC/区内含 .. 各形态实测正确、无新逃逸形态、displayPath 严格符合 R7；R9 权威 updated_at 取自 Daemon 提交时钟、version 递增未丢失、每次通知递增版本为显式设计决策。B：ports.rs 两侧无丢失、19 处机械加法为纯加法、两测试文件无重复计数与矛盾。C：组合根只装配、AC1 四条 plan 判据逐条可证伪、节点级与会话级落库隔离成立。D：依赖闭包/自实现 Myers/冻结合同零改动/agent-host 边界/无新依赖迁移 全部成立。0 CRITICAL / 0 MAJOR / 3 MINOR（F1 NodeEventSink 死类型致文档断言与实现不符 P2；F2 AC1 用例注释承诺不存在的正向对照 P3；F3 R9 测试注释基线表述有误 P3）/ 3 INFO。"
    source_evidence: NOT_APPLICABLE
```

## 8. Changes

只读检视，未修改任何代码、测试、文档、规划文件、任务状态或 `verification.md`；未切换分支、未提交、未合并、未删除分支或 worktree；未运行构建/测试/E2E/PV1/PV2。仅在 `.target-wt/probe/` 下用 `rustc` 编写并运行了一个**独立的** `normalize` 探针程序（复刻被检视函数以实测各路径形态，未写入任何被检视文件）。**仅新增本报告**（`openspec/changes/sync-scope-and-pwa-client/reports/review-mu2-candidate-r1.md`）。

## 9. 实际检查范围

- **版本与拓扑**：`git log/diff --stat/--name-status --numstat 33040d7 5ba44fd`（26 files / +6788 / -44）；`git worktree list`/`branch -a`；`.worktrees/integ` 的 `git status --porcelain`（空）与 `rev-parse HEAD`（`5ba44fd…`）。对 `crates/sync-protocol/ schemas/ fixtures/ vendor/ crates/core/Cargo.toml crates/agent-host/Cargo.toml Cargo.toml Cargo.lock` 的零 diff 核对。
- **完整读取的实现**：`crates/core/src/derive.rs`（`line_stats`/`split_lines`/`myers_distance`/`derived_file_changes`/`derive_from_elements`/`display_path`/`join_lexically`/`outside_display`/`relative_text`/`Normalized`/`normalize`/`canonical_path`/`strip_verbatim`/`canonical_uuid`/`file_changed_view`/`file_change_event`/`node_event_parts`/`view_carries_diff`/`TitleIntent`/`title_intent`/`title_from_raw`/`acp_update_members` 及其单测）；`crates/core/src/broker.rs`（`persistence_policy`、`commit_node_event`/`node_submit`、`workspace_root`/`derive_file_changes`、`commit_chunk` 全循环与状态组装、`commit_owned`/`finalize_owned_views`/`predict_session_version`、`publish`、R5/R8/R9 内联单测）；`crates/storage-sqlite/src/session_store.rs`（`StateChange::Create/Update` 分支、两条 `UPDATE owned_session` 与哨兵绑定、`node_link_slice`、`INSERT` 的 `updated_at`）。
- **组合与接线**：`crates/app/src/compose.rs`（`assemble`、`node_event_sink`、`close`、`forked_publisher`）；`crates/app/tests/node_link_e2e.rs`（AC1 段 `:1046-1767` 全部辅助函数与 3 条 `#[test]` 的逐断言核读）。
- **WP4 面**：`crates/agent-host/src/node.rs`（`NodeEvents`/`NodeEventError`/`ExitMark`/`AgentLifecycle`/`connected_event`/`disconnected_event`/`report_connected`/`report_disconnected`）、`host.rs`（`set_node_events`/`node_events_bound`/runtime 生命周期与 `report_connected` 时机）、`node.rs` 与 `compose.rs` 的类型接线核对。
- **消费侧（diff 之外，按跨边界要求）**：`crates/server/src/node_link/resource.rs:1024-1047`（`NodeLinkPublisher::publish` 的 `session.is_none()` 丢弃点）与 `:152-155`（`fan_out` 重复守卫）。
- **契约与需求对照**：`specs/core-derived-events/spec.md`（R5–R9 全文）、`specs/workspace-resolution/spec.md`（R7 全文）、`specs/local-agent-host/spec.md`；`design.md` D4–D7；`plan.md:990-1050`（AC1 两行）、`tasks.md:62`；`docs/SYNC_PROTOCOL.md` §10.2/§10.3（`session.info.changed` 与 `file.changed` 的最低字段、`session_info_update` 映射）；`docs/CORE_PORTS_AND_STORAGE.md` §5.2/§5.4/§6 第 19/21 条、§7.3；`AGENTS.md` §4/§9/§12。
- **存储测试**：`crates/storage-sqlite/tests/title_write.rs`（全文）、`derived_events_behaviour.rs`（R9 段与计数断言）、7 个既有文件的机械加法逐文件核对。
- **独立实测**：在 `.target-wt/probe/` 下用 `rustc` 编译并运行了复刻 `normalize` 的探针，覆盖 19 种路径形态（含 Windows `Prefix`/`RootDir`）。
- **只读消费的证据**：`reports/PV1.log`、`reports/PV2.log`、`reports/receipt-mu2.md`、`reports/review-w3-r1.md`、`reports/review-tp2-r1.md`、`reports/review-wire-r1.md`、`reports/review-ac1-r1.md`、`reports/deliver-wp3-r3.md`、`reports/wire-compose-nodeevents-r1.md`。

## 10. 未验证内容

- **未执行**任何编译、`cargo test`、`cargo clippy`、`cargo fmt`、`npm run verify`、`check:drift` 或 E2E；PV1/PV2 exit 0 与 AC1 全绿**只读消费**主 Agent 的 `reports/PV1.log`/`PV2.log`，本轮未复跑，不宣称实际运行通过。
- **未运行** AC1 用例本身：其中「不被会话级投递路径误收」的正向对照缺失（F2）是**静态读码推演**（追踪 `create_session` 的两次提交与唯一发布路径），未通过运行复现。
- **未验证** Windows/UNC 的实机行为（A.1 的盘符根与 UNC 形态由 `#[cfg(windows)]` 单测承载，而 `.github/workflows/ci.yml` 全为 `ubuntu-latest` → 该类用例在任何门禁中都不执行）。我用探针在本机 win32 上实测了 `normalize` 的词法层，但**未**运行 `display_path` 的完整链路（`canonicalize`/verbatim 剥离）与真实 symlink/junction 场景。
- **未验证** WP4 的 host 侧上报时机语义（首次只上报一次、复用不重复、断开不早于退出判定）：属 WP4 写入范围，其 PASS 不由本报告承载。
- **未验证** `crates/app/tests/node_link_e2e.rs` 之外的前端/PWA、`server::sync`、以及 `derive.rs` 行数统计的 schema 合法性（`decimalString` 形态）——后者由 WP2 的 `views.rs`/schema 侧与 PV1 的 `check:schemas`/`check:drift` 承载。
- **未逐字节**比对 `crates/sync-protocol/**`/`schemas/**`/`fixtures/**`（以 `git diff --stat` 零 diff 为准，未逐文件校验和）；`crates/agent-host` 的边界结论基于 Cargo/Cargo.lock 与 import 面，未逐行审读其全部生产代码。
