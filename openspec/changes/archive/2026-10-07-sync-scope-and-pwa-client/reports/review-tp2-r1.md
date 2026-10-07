```agentic-handoff
version: 1
task_id: "4.8"
role: reviewer
phase: test-case
agent_context:
  agent_id: "review-tp2-r1"
  isolation: "fork_turns=none（新建独立 reviewer，未参与 TP2 的用例编写、实现或测试运行，也未继承作者实例 testing-2-r1 的对话；仅接收调度方传入的角色契约、检视范围与固定版本 SHA）。本轮为只读检视。"
target_revision: "5ba44fd35be2cd3717006c0995a8404c3315f388"
scope: "TP2（MU2 成员）的唯一新增文件 crates/storage-sqlite/tests/derived_events_behaviour.rs（4 个测试提交 7cdc697/f22bf4f/3b63781/5ba44fd，1867 行 / 31 条 tokio::test）。基线 1550909b13bc59dd2ad635a45eff4de4e9699150（WP3 的 R7+R9 修复）。不重复 WP3/WP4/接线/AC1 各轮已通过的检视。"
changes: "只读检视，未修改任何代码、测试、文档、规划文件或 verification.md；未切换分支、未提交、未合并；未运行任何编译/测试/E2E，仅只读消费 PV1/PV2 日志与源码。仅新增本报告。"
issues: "0 CRITICAL / 0 MAJOR / 3 MINOR（均 P3）"
result: PASS
```

# TP2 独立检视（test-case 类型，Round 1）

## 1. Review Context

| 项 | 取值 |
| --- | --- |
| Review ID / Round | `review-tp2-r1` / 1（交付前检视：MU2 收口、合入 MU2 候选之前） |
| Review Type / Stage | `test-case` / 测试工作包交付前检视 |
| Work Package | TP2（MU2 单元成员，core 生产者行为判定 R5–R9） |
| Repository / Worktree | `D:\Project\acp-remote`，只读检视 worktree `.worktrees/tp2`（HEAD `5ba44fd`，工作树干净，`git status --porcelain` 为空） |
| Base Revision | `1550909b13bc59dd2ad635a45eff4de4e9699150`（WP3 的 R7+R9 修复） |
| Target Revision | `5ba44fd35be2cd3717006c0995a8404c3315f388` |
| 实际修改文件 | **仅 1 个新增文件** `crates/storage-sqlite/tests/derived_events_behaviour.rs`（`git diff --stat 1550909..5ba44fd` → 1 file changed, 1867 insertions）。`crates/core/**`、`crates/app/**`、`crates/agent-host/**`、`schemas/**`、`docs/**`、`fixtures/**` 零改动；无新依赖 |
| 读取的规则与需求 | `AGENTS.md` §9（测试要求）；`specs/core-derived-events/spec.md`（R5–R9）、`specs/workspace-resolution/spec.md`（R7）、`specs/local-agent-host/spec.md`（R8 的两条 host 侧场景）；`design.md`（D4/D5/D6/D7）；`plan.md`（TP2 行 :868、Shared File Ownership :908/:923/:926、PV2 :993、MU2 候选 :971） |
| 验证证据（只读消费） | `reports/PV2-tp2-r1.log`（`derived_events_behaviour` 31 passed / 0 failed / 0 ignored，第 475–510 行）；`reports/PV1-tp2-r1.log`（`PV1_EXIT=0`）；`reports/deliver-tp2-r1.md`（作者自评，**不作为检视证据**） |
| 限制 | 不判「用例是否充分之外的产品缺陷」（交 WP3/WP4/AC1 各轮）；不重复跑 Project Verify；不执行 E2E；作者自述的红→绿轨迹由本 reviewer 自行从源码推导，不采信叙述 |

### 1.1 独立复核：31 条与判据面的对应

- 工作树 HEAD 与 target 一致（`git rev-parse HEAD` = `5ba44fd…`），且 `git diff --quiet 5ba44fd -- <文件>` 无输出 → PV2 日志记录的 31 条确实对应 target 版本。
- PV2 日志（`.target-wt/tp2/debug/deps/derived_events_behaviour-…exe`）打印的 31 个用例名与文件内 31 个 `#[tokio::test]` 逐一对应。
- 无 `#[ignore]`、无 `#[cfg(feature)]` 门控、无 `should_panic`（`grep` 零命中）；PV2 中该 suite 是 `31 passed; 0 ignored`。

---

## 2. 按 test-case 判据表逐项结论

| 判据 | 结论 | 依据 |
| --- | --- | --- |
| ID 稳定唯一 | **PASS** | 31 条各自带 `TP2-R<n>-<NAME>` 注释 ID，与 `deliver-tp2-r1.md` A.2 的映射表逐条对应；无重名函数 |
| 需求映射 | **PASS（有 1 处覆盖缺口，见 F1）** | R5 3 条、R6 5 条、R7 10 条、R8 3 条、R9 8 条 + 2 条夹具/端口健全性。逐条对照 `core-derived-events` 的 5 个 Requirement 与对应 Scenario |
| 入口真实（不得 mock 绕过） | **PASS** | 会话级经 `Broker::sink(session).send(..)` + `Broker::flush(session)`（= 组合根 `compose.rs:1084/1144/1213/1448` 调用的同一 `flush_locked` 路径）；节点级经 `Broker::commit_node_event`（= `compose.rs:956` 的同一入口）；`BrokerDeps` 的 `store/deliveries/exports/trust/audit` 全部注入**真实** `SqliteStore`；唯一替身 `UntouchedBackends` 是失败关闭（`unreachable!`），使「误经后端」立即可见 |
| 前置可满足 | **PASS** | 所有依赖真实文件系统的用例先 `create_dir_all`/`write` 并 `canonicalize`；`a_string_prefix_is_not_treated_as_inside_the_workspace` 显式断言前置（同前缀 ≠ 在其下）防空转；`the_command_catalog_has_no_rename_entry` 断言命令名列表非空防空转 |
| 断言可观察（不得绑内部实现） | **PASS** | 读面为 `SessionStore::load`/`list`/`load_recovery`、`ReadView::replay`/`event_payload` 与 `owned_event`/`owned_session` 的原始 SQL；未断言私有结构或中间变量；`derive.rs`/`broker.rs` 的私有函数不由本文件直接调用（`normalize`/`display_path` 为 `pub(crate)`，本文件不触达） |
| 正常 + 异常路径 | **PASS** | 异常面：`TP2-R5-NO-DIFF`（不派生）、`TP2-R6-OMIT-UNKNOWN`/`BUDGET`（判定不出）、`TP2-R7-OUTSIDE`/`ABS-OUTSIDE`/`NO-ROOT`/`RELATIVE-ESCAPE`/`ESCAPE-MANY`/`SAME-PREFIX`（越界）、`TP2-R8-FAIL-CLOSED`（7 类形状违规全部 `Err` 且零写入） |
| 无 skip | **PASS** | 无 `#[ignore]`/`#[cfg(feature)]`；31/31 真跑 |
| 资源已登记 | **PASS** | 复用 `CARGO_TARGET_DIR=.target-wt/tp2`（`plan.md` Runtime Resources 已登记 WP3/WP4/WP2/TP1/TP2 行）；临时目录用 `TempDir`（`Drop` 尽力清理）；无长驻进程 |
| 基础检查 | **PASS** | PV1 `PV1_EXIT=0`（`cargo fmt --check` + `clippy -D warnings` + 全 workspace test 全绿）；PV2 全绿 |

---

## 3. 必查重点 A–E 的独立结论

### A. R7 的覆盖是否真的封住了泄露、且没把区内路径误判越界 —— **PASS（附 1 条覆盖缺口，F1）**

**A.1 两条原红测确实红在旧实现、绿在新实现（本 reviewer 自行从源码推导，不采信叙述）**

旧实现（`3b6fe41` 的 `derive.rs`，即 `1550909` 的父提交）的 `normalize` 用 `if !out.pop()` 计前导 `..`：

- `a_relative_path_escaping_the_root_is_outside` 输入 `../../etc/passwd`。旧路径：第一个 `..` 时 `out` 为空 → `pop()` 返回 `false` → `push("..")`；第二个 `..` 时 `pop()` 弹掉 `".."` 并返回 **`true`** → 计数丢失、`out` 变空 → 随后 `etc`、`passwd` 被当普通段压入，得到 `etc/passwd`（无前导 `ParentDir`）。`display_path` 的相对分支只判 `matches!(components().next(), Some(ParentDir))` → **判为区内**，`displayPath = "etc/passwd"`、无 `outsideWorkspace`。测试断言 `outsideWorkspace:true` 且 `displayPath:"passwd"` → **红** ✓（与 `deliver-tp2-r1.md` B.3 记录的实测 view 一致）。
- `multiple_leading_parent_traversals_keep_only_the_file_name` 输入两条多级 `..` → 旧实现同样判区内、下发 `etc/passwd` / `Users/alice/.ssh/id_rsa` → 断言 `outsideWorkspace:true` 与「不得泄漏 `etc/`/`Users/`/`.ssh/`」→ **红** ✓。

新实现（`1550909`）的 `view` 输出 `outsideWorkspace:true` + 只给 `file_name()` → **绿** ✓。结论：两条断言的判别力**绑定行为**（下发值与越界标记），不是复述实现。

**A.2 5 条「必须仍判区内」的回归防护确实能捕获「一律判越界」的过度修复**

| 用例 | 过度修复（一律判越界）时的表现 | 判别 |
| --- | --- | --- |
| `TP2-R7-INSIDE`（区内深层绝对路径） | 断言 `!contains("outsideWorkspace")` → 红 | ✓ |
| `TP2-R7-INNER-DOTDOT`（`sub/../file.txt`） | 断言 `displayPath:"file.txt"` 且无 `outsideWorkspace` → 红 | ✓ |
| `TP2-R7-ABS-INNER-DOTDOT`（绝对形式 `api/../api/file.txt`） | 同上 → 红 | ✓ |
| `TP2-R7-RELATIVE-INSIDE`（`deep/nested.rs` 相对形式） | 断言无 `outsideWorkspace` → 红 | ✓ |
| `TP2-R7-SAME-PREFIX`（`api` vs `api-tools`） | 该用例测的是**反向**（必须判越界），过度修复时反而「假绿」 | 防的是「按字符串前缀误判为区内」的欠修复，非过度修复 |

结论：真正对「一律判越界」有红/绿判别力的是前 4 条；`SAME-PREFIX` 与 `ROOT-ITSELF` 属另一类边界。覆盖到位。

**A.3 Windows 面 —— 覆盖缺口（F1）**

修复声称支持 `C:\..\..\etc\passwd`（盘符根）与 `\\server\share\..\..\x`（UNC 份额根）。本 reviewer 在新增文件中检索 Windows 形态输入（`C:\`、`\\server`、`UNC`、`verbatim`）——**零命中**：文件中唯一的 Windows 相关代码是测试自身的路径卫生函数 `canonical_dir`（剥 `\\?\`/`\\?\UNC\`）。这两类输入的断言**只存在于** `crates/core/src/derive.rs` 的 `#[cfg(windows)]` 单测 `normalize_counts_leading_parent_traversals_without_popping_the_root`（由 WP3 的 `1550909` 引入）。而 `.github/workflows/ci.yml` 的 5 个 job 全部 `runs-on: ubuntu-latest` → 该 `#[cfg(windows)]` 用例在任何门禁中都不执行。TP2 的新文件因此只覆盖 Unix 形态的 `../`，Windows/UNC 面由**永不运行**的测试承载。已按 P3 记入 F1（结构性成因另见 6.2）。

### B. R9「`updated_at` 必须前进」的判别力 —— **PASS**

本 reviewer 自行核算夹具的时钟读数（`MonotonicClock` 每次 `now()` +1 分钟，`stamp(n)` = `2026-10-05T(hh):(mm):00.000Z`）：

- `Fixture::new` 不读时钟（`Broker::new` 无 `self.now()`；`store.commit` 的 `at` 由参数给出）。`flush` → `flush_locked` → `commit_chunk` **恰好一次** `let at = self.now()`（`broker.rs:2021`），`commit_owned` 不额外读时钟。
- 首次 `push_info_update(.., stamp(51))`：commit.at = 第 1 次读数 = `stamp(0)` → `after_first.updated_at = stamp(0)`。
- 第二次 `push_info_update(.., stamp(1))`：commit.at = 第 2 次读数 = `stamp(1)` → `after_second.updated_at = stamp(1)`。

三种来源下的判据表现（断言逐条核对）：

| 来源假设 | 第一次 / 第二次 `updated_at` | 断言 `>` 前进 | 断言 `!starts_with("2020-")` |
| --- | --- | --- | --- |
| Daemon 提交时钟（现行） | `stamp(0)` / `stamp(1)` | **绿** | **绿** |
| 事件自身 `at`（`commit.at = event.at`） | `stamp(51)` / `stamp(1)` | **红**（倒退） | 绿 |
| Agent 自报 2020 字面量 | `2020-…` | 绿/红不定 | **红** |

三条来源**各自**至少被一条断言判红 → 判别力成立。作者注释「事件的 `at` 故意取一个早于上一次写入的取值（`stamp(1)` < `stamp(51)`）」在「用事件 `at` 当权威」的假设下自洽（此时上一次落地值就是 `stamp(51)`），断言失败时指向的字段是 `Session::updated_at`、来源是提交时钟 —— 指向正确。

另：断言比较的是固定宽度时间戳字符串的字典序（`"2026-10-05T00:01:00.000Z" > "2026-10-05T00:00:00.000Z"`），与时间序同序，无隐藏假设。

### C. 其余需求的行为覆盖 —— **PASS（附 2 条覆盖缺口，F2/F3）**

逐条核对 R5–R9 与 D4–D7：

| 需求 / 场景 | 覆盖 | 判断 |
| --- | --- | --- |
| R5 派生唯一性（同批重复上报） | `TP2-R5-DERIVE-ONCE` | **到位**（同一次 `flush` 的 `started`+`updated` 入同一 `commit_chunk`，由批内 `file_change_plan` 去重） |
| R5 派生唯一性（跨合并窗口） | **无** | **缺口 F2**：`Slot::file_changes` 的持久去重日志在该用例中始终为空，从未被触及 |
| R5 不派生（含 `rawInput` 干扰） | `TP2-R5-NO-DIFF` | 到位 |
| R5 原文三要素不变 | `TP2-R5-ACP-UNTOUCHED` | 到位（结构相等 + 字节长度 `== acp_raw.len()`） |
| R6 行数相等但内容不同 | `TP2-R6-EQUAL-LINES` | 到位（`(1,1)`，行数差口径会报 0） |
| R6 新建 / 删除 | `TP2-R6-NEW-FILE` / `DELETED-FILE` | 到位 |
| R6 判定不出时省略而非填零 | `TP2-R6-OMIT-UNKNOWN`（缺 `newText`）+ `BUDGET`（超预算） | 到位；`!contains("addedLines")` 断言子串缺席，且同断言要求四个必填字段仍在 → 确实区分「这一项未知」与「整条退化」 |
| R7 区内相对化 / 越界显式标记 / 同名前缀 | 10 条 | 到位（A 节逐条核过） |
| R8 节点级 `session_id` 为空 + 四个定位列 NULL | `TP2-R8-NODE-NULL-SESSION` | 到位（原始 SQL + replay 面双读） |
| R8 失败关闭、不静默丢弃/降级 | `TP2-R8-FAIL-CLOSED` | 到位（7 类 + turn + ACP = 9 种违规全 `Err`，且 `event_count` 不变） |
| R8 不复用不重复上报 / 断开不早于退出判定 | 不在本文件 | **正确外置**：属 WP4 写入范围（`agent-host/tests/session.rs` 已有 `profile_process_reports_connect_once_and_reuse_adds_nothing`、`oversize_exit_reports_disconnect_after_the_exit_verdict`）；`deliver-tp2-r1.md` D 节已如实登记为未验证 |
| R8 不表达会话活跃 | `TP2-R8-NOT-SESSION-ACTIVITY` | 到位 |
| R9 标题单向更新 | `TP2-R9-SET`/`KEEP`/`CLEAR`/`CLEAR-WITH-MODE-CHANGE` | 到位 |
| R9 无重命名命令 | `TP2-R9-NO-RENAME-ENTRY` | 到位（目录 13 条命令逐条回查 core 授权镜像，防空转） |
| R9 直连端口的两条补充分支 | `TP2-R9-STORE-UNCHANGED-KEEP` / `-CLEAR` | 分支确实补齐（见 D 节），但 `-CLEAR` 的注释对 `title_write.rs` 既有覆盖面的描述**不准确**（F3） |

另有两处「看似覆盖、实际不到位」但不构成阻断：

1. `TP2-R7-ROOT-ITSELF` 断言仅 `!contains(&canonical) && !contains("..")`，未断言 `outsideWorkspace:true` 与 `displayPath != root`。对「不得下发根的任何片段」这一目标足够，但比同类的 `OUTSIDE` 用例弱。**记为观察，不列为 finding**（其目标已达成，且核心断言在 `derive.rs` 单测 `display_path_treats_the_workspace_root_itself_by_the_spec` 中已被钉住）。
2. `TP2-R7-ABS-INNER-DOTDOT` 与 `TP2-R5-DERIVE-ONCE` 的 `!view.contains("..")` 是全 view 子串检查，若未来 view 加入任意含 `..` 的字段会误红 —— 但当前域名下无此风险，**不列为 finding**。

### D. 写入范围与区域收窄 —— **PASS（处置正确，1 处注释不准确见 F3）**

**D.1 `crates/core/src/**/tests.rs` 不可用、全部落在 `crates/storage-sqlite/tests/`**

本 reviewer 独立复核该判断：`crates/core/src/` 下 `broker.rs:6227`、`derive.rs:651`、`use_cases.rs:1667` 各自持有内联 `#[cfg(test)] mod tests`，只有 `model/mod.rs:153` 用外部 `mod tests;`（即 `model/tests.rs`）。因此新建 `crates/core/src/broker/tests.rs` 或 `derive/tests.rs` **必须**改 `broker.rs` / `derive.rs` 的模块声明——而这两个文件是 WP3 的实现产物、不在 TP2 的 Write Scope（`plan.md:868` 的 `crates/core/src/**/tests.rs` 语义上属 TP2，`plan.md:923` 的区域注记写明「WP3 改实现文件，TP2 只改同目录的 tests.rs 测试模块」）。故 TP2 的处置**正确**：plan 的该条字面可用性不成立，实现形态与规划不匹配。

该不匹配**已在 `verification.md:275` 登记**（「又一处规划与实现形态不匹配（登记备查）」），措辞与本 reviewer 的结论一致。**不重复登记**，无需新增 finding。

**D.2 未改 `title_write.rs`、把补充判据放进自己的文件**

本 reviewer 独立复核作者对 `title_write.rs` 覆盖面的判断，并**修正其中一处不准确**：

| 语句分支 | 标题三态 | `title_write.rs` 实测 | 结论 |
| --- | --- | --- | --- |
| `ModeChange::Set` | `Some(Some)` | `setting_a_title_writes_it_and_bumps_the_version`（`session_update` 默认 `Set`） | 有 |
| `ModeChange::Set` | `None` | `an_unchanged_title_keeps_the_existing_value_and_still_bumps_the_version`（`session_update(None)` → `Set`） | 有 |
| `ModeChange::Set` | `Some(None)` | `clearing_a_title_with_a_mode_change_writes_both_columns` | 有 |
| `ModeChange::Unchanged` | `Some(None)` | `clearing_a_title_bumps_the_version_and_keeps_the_rest_of_the_batch` | 有 |
| `ModeChange::Unchanged` | `None` | **无** | TP2 `TP2-R9-STORE-UNCHANGED-KEEP` 补 |
| `ModeChange::Unchanged` | `Some(Some)` | **无**（直连端口层） | TP2 `TP2-R9-STORE-UNCHANGED-CLEAR` 的前置写入 + `TP2-R9-SET` 的端到端链路补 |

`grep -n "ModeChange::" title_write.rs` 只出现 `Set`（:168、:364）——`title_write.rs` **从未**构造 `ModeChange::Unchanged`。因此作者报告 C 节表格里「`Unchanged + title: None` ✅ 由 `an_unchanged_title_keeps_the_existing_value…` 覆盖」一行是**错的**；正确事实是：该分支本就缺失，正是 TP2 补上的两条。缺口定位（组合根 `commit_chunk` 恒用 `Unchanged`，是生产最常走的语句）与补充处置**均正确**；未改 `title_write.rs`（该目录区域注记「TP2 只新增行为测试文件」）亦**正确**。仅注释表述有误（F3）。

TP2 补充的两条确实钉住了 `session_store.rs:1142` 的 `title = CASE WHEN ?9 THEN NULL WHEN ?8 IS NULL THEN title ELSE ?8 END`：`-KEEP` 走 `?9=0 且 ?8 IS NULL` 支，`-CLEAR` 走 `?9=1` 支且同批绑定 `?1..?9` 全部可见（`state`/`agent_session_id`/`workspace_cwd` 三列不丢的断言即为此）。作者声明的「缺 `Unchanged + Some(Some)` 直连端口用例」属实且未补直连版本——但该链路被 `-CLEAR` 的前置写入（同样 `Unchanged + Some(Some)`）实际执行到了，故不构成缺口。

### E. 门禁盲区 —— **PASS（TP2 的 31 条确经 PV2 执行）**

- `plan.md:993` 的 PV2 命令 = `cargo test --locked -p core -p storage-sqlite -p agent-host --all-features`，含 `-p storage-sqlite` → `crates/storage-sqlite/tests/` 下所有集成测试（含本文件）被执行。
- 实证：`reports/PV2-tp2-r1.log` 第 475 行 `Running tests\derived_events_behaviour.rs`，第 510 行 `test result: ok. 31 passed; 0 failed; 0 ignored`。
- `plan.md:971` 的 MU2 候选检查为 `PV1, PV2`，**不含 AC1**。AC1 = `cargo test -p app --test node_link_e2e`，其对象是 `crates/app/tests/`（TP2 未触及，`deliver-tp2-r1.md` D 节已登记为未验证）。因此**不存在**「TP2 的行为断言穿过 MU2 门禁」的情形——本文件的全部 31 条都在候选检查 PV2 的真实执行面内。
- 附带确认：PV2 中另有 2 处 `1 ignored`（分别属 `cargo test -p agent-host` 的既有 suite 与其他 suite），**与本文件无关**（本文件 0 ignored），非本次弱化。

---

## 4. 发现（findings）

### `review-tp2-r1-F1`（MINOR / P3）：R7 未覆盖 Windows 盘符根与 UNC 份额根，修复的 Windows 声明无门禁承载

- **位置**：`crates/storage-sqlite/tests/derived_events_behaviour.rs:1111-1148`（R7 逃逸用例组；同组 10 条全部使用 Unix 形态输入）。
- **触发条件**：检视 target 新增文件中的 R7 输入集合。
- **预期 / 实际**：D5 与 `workspace-resolution` R7 的越界判定必须基于规范化后的前缀关系，而 WP3 修复明确声称覆盖 `C:\..\..\etc\passwd`（`pop()` 会弹掉盘符根）与 `\\server\share\..\..\x`（UNC 份额根）。**实际**：新增文件中 `C:\`、`\\server`、`UNC` 零命中；这两类输入仅由 `crates/core/src/derive.rs` 的 `#[cfg(windows)]` 单测承载，而 `.github/workflows/ci.yml` 5 个 job 全为 `ubuntu-latest` → 该用例在任何门禁中都不运行。
- **影响**：修复恰好针对的正是 `pop()` 弹掉 `Prefix`/`RootDir` 这一 Windows 形态缺陷；其回归防护在自动化门禁中不存在。若后续重构 `normalize` 重新引入对根组件的依赖，Unix 侧的 10 条 R7 用例仍会全绿。
- **说明**：这是**结构性**缺口 —— `derive.rs::normalize` 为 `pub(crate)`，Linux 上 `C:\..\..` 会被解析为含 `\` 的普通相对路径，无法在 Linux 承载盘符根语义。因此建议按 6.2 的方式登记，而非强求 TP2 在 Linux 上写出等价断言。

### `review-tp2-r1-F2`（MINOR / P3）：R5 的「同一处改动只派生一次」只覆盖批内去重，未覆盖跨合并窗口的持久去重日志

- **位置**：`crates/storage-sqlite/tests/derived_events_behaviour.rs:525-552`。
- **触发条件**：该用例把 `tool.call.started` 与 `tool.call.updated`（同一 `toolCallId` + 同一 Diff）push 进同一槽位后**只调用一次** `flush()`。
- **预期 / 实际**：生产路径由组合根定时器按 `storage.flush_interval_ms`（默认 250ms）驱动 `pump`，`started` 与 `updated` 极可能落在**不同**合并窗口。去重因此有两条独立来源：批内 `file_change_plan` 与跨批的 `Slot::file_changes` 日志（`broker.rs:2276-2286`，容量 `FILE_CHANGE_DEDUP_CAPACITY = 4096`）。**实际**：该用例两次事件进入同一 `commit_chunk`（`flush_locked` 只在终态事件处切批，本例两者均非终态），命中的是 `file_change_plan`；`Slot::file_changes` 在首个批次时恒为空，从未被查询。
- **影响**：若有人以「`file_change_plan` 已足够」为由删除或误改持久日志，本用例仍全绿，而真实跨窗口的重复上报会派生两条 `file.changed`（违反 R5「MUST NOT 为同一处改动重复派生」，且破坏 `changeId` 唯一性）。仓库内目前**无任何用例**覆盖该跨批路径（`broker.rs:9081` 的 `a_repeated_diff_element_is_derived_only_once` 同样只用一次 `submit_prompt`/`flush`）。
- **建议**：在 R5 组内补一条变体——push `tool.call.started` → `flush()` → push 携带同一 Diff 的 `tool.call.updated` → 再 `flush()`，断言 `file.changed` 仍恰 1 条。

```
first flush:  push(started, diff)              -> flush() -> 1 file.changed
second flush: push(updated, same diff)         -> flush() -> 仍 1 file.changed（不得为 2）
```

### `review-tp2-r1-F3`（MINOR / P3）：`TP2-R9-STORE-UNCHANGED-CLEAR` 的注释对 `title_write.rs` 既有覆盖面描述不准确

- **位置**：`crates/storage-sqlite/tests/derived_events_behaviour.rs:1731-1736`。
- **触发条件**：阅读该用例的文档注释，与 `crates/storage-sqlite/tests/title_write.rs` 实测对照。
- **预期 / 实际**：注释断言「既有用例覆盖 `Unchanged + title: None` 与 `Set + Some(_)`」。**实际**：`title_write.rs` 从未构造 `ModeChange::Unchanged`（`grep "ModeChange::"` 仅 :168/:364 两条，均为 `Set`）；其「不改标题」用例 `an_unchanged_title_keeps_the_existing_value_and_still_bumps_the_version` 走的是 `Set` 语句（该用例自己的注释也写明「`Set` 的语句里标题哨兵是 `?11`」）。真实情况是 `Unchanged + None` **此前就是缺口**，正是本包 `TP2-R9-STORE-UNCHANGED-KEEP` 补上的。
- **影响**：同一份交付内两处相互矛盾（`deliver-tp2-r1.md` C 节表格已把该行标为 ❌ 缺），读者按注释会误以为该分支已被既有用例覆盖，从而在后续重构中放心删掉本包用例。
- **建议**：把该句改为「既有用例覆盖 `Unchanged + Some(None)`（置空）与 `Set` 语句下的三态；`Unchanged + None` 与 `Unchanged + Some(Some)` 此前无用例」，与 `deliver-tp2-r1.md` C 节表格一致。

---

## 5. 结论

**PASS** —— 无 CRITICAL / MAJOR；3 条 MINOR（均 P3、非阻断）。

依据：唯一新增文件未越区（1 file / +1867，无新依赖）；入口真实（公开端口 + 真实 SQLite，唯一替身失败关闭）；31 条断言可观察、无 skip、经 PV2 真实执行且全绿；R7 的两条原红测判别力经源码独立推导成立（旧实现判区内并下发 `etc/passwd`，与作者记录的实测 view 一致），4 条「必须仍判区内」用例可捕获过度修复；R9 的三来源判别力经时钟读数核算成立；plan 的 `crates/core/src/**/tests.rs` 不可用已核实并已在 `verification.md:275` 登记；未改 `title_write.rs` 的处置正确。

F1/F2/F3 均为覆盖精度/注释准确性问题，不改变「31 条行为判定在 target 版本上真实、可辨别」这一结论，按判定规则不要求清零。

---

## 6. 实际检查范围与未验证内容

### 6.1 实际检查范围

- `git log/show/diff` 只读：`1550909..5ba44fd` 的 4 个提交与唯一文件的完整内容（1867 行逐段读取）；`7cdc697`/`f22bf4f`/`3b63781`/`5ba44fd` 逐提交的用例名/计数差异。
- 实现侧只读源码核对（判断断言判别力所需）：`crates/core/src/derive.rs`（`display_path`/`normalize`/`line_stats`/`myers_distance`/`file_changed_view`/`title_intent`/内联单测）、`crates/core/src/broker.rs`（`sink`/`flush`/`flush_locked`/`commit_chunk`/`commit_node_event`/去重日志/内联单测）、`crates/storage-sqlite/src/session_store.rs`（两条 UPDATE 语句与标题 CASE）、`crates/storage-sqlite/tests/title_write.rs`（4 条用例与 `ModeChange` 使用）、`crates/app/src/compose.rs`（`flush`/`commit_node_event` 的真实调用点）、`crates/storage-sqlite/tests/support/mod.rs`、`.github/workflows/ci.yml`。
- 只读消费 PV1/PV2 日志与 `deliver-tp2-r1.md`；复核工作树 HEAD == target 且 `git status` 干净。
- 为核实「31 条会被 PV2 执行」而读取 `plan.md` 的 PV2/MU2/TP2/Shared File Ownership 行与 `verification.md` 的规划缺口登记节。

### 6.2 未验证内容

- **未执行任何编译/测试/E2E**：`31 passed` 与 `PV1_EXIT=0` 来自只读消费的证据日志，本 reviewer 未复跑（契约要求）。
- **Windows/UNC 运行面**：F1 所指的 `#[cfg(windows)]` 单测未被执行（本机为 win32，但本 reviewer 未运行任何测试；CI 为 Linux）。因此「`C:\..\..\etc\passwd` 与 UNC 在真实 Windows 上判越界」这一行为**未被任何本轮证据证实**，仅由源码推导。
- **R8 的 host 侧上报时机**（首次只上报一次、复用不重复、断开不早于退出判定、未绑定 `NodeEvents` 不静默丢弃）：属 WP4 写入范围，本包未覆盖、本轮未验证（`agent-host/tests/session.rs` 存在同名用例，但其 PASS 不由 TP2 的证据承载）。
- **AC1 端到端组合根**（`crates/app/tests/node_link_e2e.rs`）：TP2 未触及且不在 PV2 的包列表内，未验证。
- **前端/PWA、`server::sync`**：本包不涉及。
- **F2 所指的跨合并窗口去重**：本 reviewer 未构造运行实例，判断基于 `flush_locked` 的切批逻辑（仅终态事件切批）与 `Slot::file_changes` 仅在 `commit_owned` 成功后填充的源码推导。
- **临时目录清理**：本机 `%TEMP%` 存在大量 `acpr-storage-*` 残留目录（含本文件各前缀）。本 reviewer 检索发现既有 suite（如 `title_write`、`tp4-*`）同样有残留，属仓库既有的 Windows 平台行为，**不计为本包引入的问题**，故未列为 finding。
- **`addedLines`/`deletedLines` 的 schema 合法性**（`decimalString` 形态）与 `outsideWorkspace` 的 schema 登记：由 WP2 的 `views.rs`/`schema` 侧及 PV1 的 `check:schemas`/`check:drift` 承载，本包只断言「键存在/缺席与取值」，未复核 schema 一致性。
