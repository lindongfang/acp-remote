<!-- TP2 交付报告（MU2，第 1 轮）。tester 只交付需求映射、可运行用例与基础检查证据；不判独立 review、合入、E2E 与最终验收。 -->

# TP2 core 生产者行为交付（deliver-tp2-r1）

## Shared Report

- **task_id**: `4.7`（`phase: design`，DESIGN）/ `4.8`（`phase: design-author`，DELIVERY + CHECK）
- **work_package**: TP2（交付单元 MU2）
- **role**: tester（`testing-2-r1`）
- **phase**: design-author（本报告同时承载 `design` 阶段的 DESIGN 行与 `design-author` 阶段的 DELIVERY/CHECK 行）
- **agent_context**: `agent_id: testing-2-r1`，`isolation: fork_turns=none`（新建实例，未继承 WP3/WP4 实现者或任何检视者的对话）
- **target_revision**: `5ba44fd35be2cd3717006c0995a8404c3315f388`（分支 `feat/tp2-behaviour`）
- **scope**: 只新增 `crates/storage-sqlite/tests/derived_events_behaviour.rs`。未触碰 `crates/core/**`、`crates/app/**`、`crates/agent-host/**`、`crates/sync-protocol/**`、`schemas/**`、`docs/**`、`fixtures/**`、`openspec/changes/**`（本报告除外）、`.worktrees/` 下其他包与主检出。

### 交付提交

| 项 | 取值 |
| --- | --- |
| worktree | `D:\Project\acp-remote\.worktrees\tp2` |
| 分支 | `feat/tp2-behaviour` |
| 固定起点 | `3b6fe4136e71e5a8a9b36527440348d9be280ae0`（`integ/mu2-wiring`，含 WP3 + WP4 + 组合根接线 + AC1） |
| 上游修复（WP3 第三轮，本包目标版本已含） | `1550909b13bc59dd2ad635a45eff4de4e9699150` |
| **交付提交（target）** | **`5ba44fd35be2cd3717006c0995a8404c3315f388`** |
| `CARGO_TARGET_DIR` | `D:\Project\acp-remote\.target-wt\tp2`（复用既有目录） |

`git diff --stat 3b6fe41..5ba44fd`（3 files, **2163 insertions(+), 17 deletions(-)**）——其中 WP3 的修复属上游；**本包增量只有 1 个文件**：

```
 crates/core/src/broker.rs                          |   59 +-   ← WP3 第三轮（上游，非本包）
 crates/core/src/derive.rs                          |  254 +-   ← WP3 第三轮（上游，非本包）
 crates/storage-sqlite/tests/derived_events_behaviour.rs | 1867 ++++++++++++++++++++  ← 本包唯一新增
```

`git diff --stat 1550909..5ba44fd`（**本包增量**）：`1 file changed, 1867 insertions(+)`。
对 `crates/core/**`、`crates/app/**`、`crates/agent-host/**`、`schemas/**`、`docs/**`、`fixtures/**`、
`crates/sync-protocol/**`、既有测试文件的**零改动**可直接用 `git diff --stat` 核对。

`git status --porcelain` 交付后为 0 行（工作树干净）。

### 写入范围的落地方式（plan 的一处不可满足项，已在报告登记）

plan 的 TP2 Write Scope 是 `crates/core/src/**/tests.rs` 与 `crates/storage-sqlite/tests/`。**前者在本仓库不可用**：
`crates/core/src/broker.rs` 自己持有内联的 `#[cfg(test)] mod tests`，要新建 `crates/core/src/broker/tests.rs`
必须修改 `broker.rs` 的模块声明，而 `broker.rs` 是 **WP3 的实现文件**、不在 TP2 的写入范围内。
因此本包把 core 生产者行为的判定落在 `crates/storage-sqlite/tests/`（在写入范围内，且 `[PV2]` 的
`-p storage-sqlite` 会执行它）。该目录的 Shared File Ownership 区域注记是「TP2 只新增行为测试文件」——
本包**只新增** `derived_events_behaviour.rs`，**未改任何既有文件**。

### 被测入口是真实的（不使用 mock 绕过被测入口）

全部用例经**公开端口**驱动真实 `acp_core::broker::Broker` + 真实 `storage_sqlite::session_store::SqliteStore`：

| 被测行为 | 真实入口 |
| --- | --- |
| 会话级事件派生（R5–R7、R9） | `Broker::sink(session).send(EndpointEvent)` → `Broker::flush(session)`（= 组合根合并窗口的同一入口） |
| 节点级事件（R8） | `Broker::commit_node_event(event)` |
| 读回与断言 | `SessionStore::load`/`load_recovery`/`list`、`ReadView::replay`/`event_payload`，以及 `owned_event`/`owned_session` 的原始 SQL |

`BrokerDeps` 里除后端工厂/标识分配器/时钟/发布器之外**全部注入真实 `SqliteStore`**（`deliveries`/`exports`/
`trust`/`audit` 与 `store` 是同一个实例——`SqliteStore` 实现了这几个端口；装配口径与
`crates/app/tests/support/owner.rs` 一致）。本文件**不手写派生出的 view 文本**：`file.changed` 的每个字节
都由 core 的派生路径产生，测试只断言真实落库结果。唯一未被触达的后端工厂是失败关闭替身（调用即 panic），
使「误经后端」这一耦合在测试里立刻可见。

### 与既有用例的分工（不重复）

- `crates/core/src/{broker,derive}.rs` 的内联测试：以**内存 fake 端口**覆盖派生逻辑本身（含 Myers 与暴力对拍）。
  本文件不重跑那些断言。
- `crates/storage-sqlite/tests/title_write.rs`（WP3 修复轮）：直接 `SessionStore::commit` 的标题**列**语义。
  本文件补的是「ACP 通知 → broker 投影 → 真实列」这条链路，以及 `title_write.rs` 未覆盖的语句分支（见 C 节）。

---

## A. 设计阶段（`phase: design`，DESIGN）

### A.1 需求映射（R5–R9 的行为可判定面）

| Req | spec 场景 | 本包的可判定面 | 用例 ID |
| --- | --- | --- | --- |
| **R5** 文件改动事件由类型化 Diff 派生 | ①含 Diff 产生一个事件 ②不含 Diff 不产生 ③派生不改写 ACP 原文 | 两处不同改动各派一条、同一处重复上报只派一条；无类型化 Diff（含「自由形状 `rawInput` 看似编辑」）不派；携带 Diff 的工具调用落库后 ACP 原文/摘要/字节长度逐字不变 | `TP2-R5-DERIVE-ONCE`、`TP2-R5-NO-DIFF`、`TP2-R5-ACP-UNTOUCHED` |
| **R6** 改动行数按行级差异统计 | ①行数相等但内容不同仍报非零 ②新建文件全部计新增 ③判定不出时省略而非填零 | 五类边界：行数相等内容不同 → `1/1`；新建 → `N/0`；删除 → `0/N`；缺 `newText` → 两键**缺席**；超预算病态输入 → 两键缺席 | `TP2-R6-EQUAL-LINES`、`TP2-R6-NEW-FILE`、`TP2-R6-DELETED-FILE`、`TP2-R6-OMIT-UNKNOWN`、`TP2-R6-BUDGET` |
| **R7** 展示路径相对化且不泄漏工作区外结构 | ①区内相对化 ②区外显式标记且只给文件名 ③同名前缀不得误判区内 | 见 A.3 的 11 条边界矩阵 | `TP2-R7-INSIDE`、`TP2-R7-OUTSIDE`、`TP2-R7-SAME-PREFIX`、`TP2-R7-NO-ROOT`、`TP2-R7-RELATIVE-ESCAPE`、`TP2-R7-ESCAPE-MANY`、`TP2-R7-ROOT-ITSELF`、`TP2-R7-INNER-DOTDOT`、`TP2-R7-ABS-INNER-DOTDOT`、`TP2-R7-ABS-OUTSIDE`、`TP2-R5-R7-SAME-NAMED-OUTSIDE` |
| **R8** Agent 连接状态是节点级生命周期 | ①进程建立产生一次连接 ②进程退出产生一次断开 ③节点级事件不被会话级投递路径误收 ④状态取值来自封闭词表 ⑤不表达会话活跃程度 | 落库 `session_id` 为空且四个会话级定位列全 `NULL`（原始 SQL + replay 两面）；7 类形状违规失败关闭且**一行不写**；会话活跃（文件改动 + 标题更新）不产生任何连接事件 | `TP2-R8-NODE-NULL-SESSION`、`TP2-R8-FAIL-CLOSED`、`TP2-R8-NOT-SESSION-ACTIVITY` |
| **R9** 会话标题只由 Agent 通知更新 | ①首次通知写入 ②只有更新时间保持 ③显式置空呈现未命名 ④客户端无重命名入口 ⑤未生成时保持未命名 | SET/KEEP/CLEAR 三态经真实列与摘要；权威时间不取 Agent 自报值且**必须前进**（含事件 `at` 早于上次写的判别构造）；同批含 `session.mode.changed` 时整批不丢、注入版本与落盘版本一致；命令目录无改名入口且 core 授权镜像同步 | `TP2-R9-SET`、`TP2-R9-KEEP`、`TP2-R9-CLEAR`、`TP2-R9-CLEAR-WITH-MODE-CHANGE`、`TP2-R9-NO-RENAME-ENTRY`、`TP2-R9-STORE-UNCHANGED-CLEAR`、`TP2-R9-STORE-UNCHANGED-KEEP` |

### A.2 稳定用例 ID 与场景/步骤/断言

| Case ID | 场景 | 入口 | 断言（可观察） |
| --- | --- | --- | --- |
| `TP2-R5-DERIVE-ONCE` | 两处不同改动 + 同一处重复上报 | `sink` + `flush` | `file.changed` 恰 2 条；展示路径相对化且不含根片段 |
| `TP2-R5-NO-DIFF` | 无类型化 Diff（含 `rawInput` 干扰） | 同上 | `file.changed` 0 条；同批 `tool.call.started` 仍落库 |
| `TP2-R5-ACP-UNTOUCHED` | 派生 + 真实存储往返 ACP 原文 | 同上 + `ReadView::event_payload` | `payload.acp == AcpRaw::available(原本)`（结构相等，含字节长度与摘要） |
| `TP2-R6-EQUAL-LINES` | 两行换成两行、内容不同 | 同上 | `addedLines:"1"` 且 `deletedLines:"1"`（行数差口径会报 0） |
| `TP2-R6-NEW-FILE` | `oldText` 缺席 | 同上 | `addedLines:"3"`/`deletedLines:"0"`/`kind:"added"` |
| `TP2-R6-DELETED-FILE` | `newText = ""` | 同上 | `addedLines:"0"`/`deletedLines:"4"`/`kind:"deleted"` |
| `TP2-R6-OMIT-UNKNOWN` | 缺 `newText` | 同上 | 两键**子串缺席**；四个必填字段仍在 |
| `TP2-R6-BUDGET` | 4000 行整体替换（D=8000，超步数预算） | 同上 | 两键缺席（不得报出一个算错的数） |
| `TP2-R7-INSIDE` | 区内深层绝对路径 | 同上 | `displayPath:"deep/nested/file.txt"`；无根片段、无 `..`、无 `outsideWorkspace` |
| `TP2-R7-OUTSIDE` | 区外绝对路径 | 同上 | `outsideWorkspace:true` + `displayPath:"nginx.conf"`；无目录结构 |
| `TP2-R7-SAME-PREFIX` | `api` vs `api-tools`（同字符串前缀） | 同上 | 前置条件断言同前缀（防空转）；判越界且只给文件名 |
| `TP2-R7-NO-ROOT` | `workspace_cwd` 为 `NULL` | 同上 | 从严越界 + 只给文件名；不含任何层级 |
| `TP2-R7-RELATIVE-ESCAPE` | `../etc/passwd`（1 级） | 同上 | 越界 + `displayPath:"passwd"` |
| `TP2-R7-ESCAPE-MANY` | `../../etc/passwd` 与 `../../Users/alice/.ssh/id_rsa` | 同上 | 两条都越界、都只给文件名、都无 `..`、都不含 `etc/`/`Users/`/`.ssh/` |
| `TP2-R7-ROOT-ITSELF` | 报告路径 == 工作目录根 | 同上 | 不含根片段、不含 `..` |
| `TP2-R7-INNER-DOTDOT` | `sub/../file.txt` | 同上 | 规范化后仍在区内 → 不标越界 |
| `TP2-R7-ABS-INNER-DOTDOT` | `…/api/../api/file.txt`（绝对形式含内部 `..`） | 同上 | 仍判区内、`displayPath:"api/file.txt"`、不下发 `..` |
| `TP2-R7-ABS-OUTSIDE` | 另一个区外临时目录下的文件 | 同上 | 越界 + 只给文件名 |
| `TP2-R5-R7-SAME-NAMED-OUTSIDE` | `/etc/nginx/nginx.conf` 与 `/tmp/nginx.conf` | 同上 | 恰 2 条；`changeId` 互不相同；展示值仍只给文件名；不泄漏 `/etc`、`/tmp` |
| `TP2-R8-NODE-NULL-SESSION` | 连接 + 断开各一次 | `commit_node_event` | `session_id IS NULL`（原始 SQL）+ 四个定位列全 `NULL`；replay 面一致；仍有全局序号 |
| `TP2-R8-FAIL-CLOSED` | 7 类形状违规（类型/`state` 词表/缺与空串 `agentId`/类别/turn/ACP 原文） | 同上 | 全部 `Err`；`owned_event` 总行数不变（一行不写） |
| `TP2-R8-NOT-SESSION-ACTIVITY` | 会话侧文件改动 + 标题更新 | `sink` + `flush` | 节点级事件 0 条；会话状态只由会话行承载 |
| `TP2-R9-SET` | 首次通知带标题 | 同上 | 标题写入；摘要反映；`updated_at` 不取 Agent 自报值；事件 view 仍转发自报 `updatedAt` |
| `TP2-R9-KEEP` | 通知只带更新时间，且事件 `at` **早于**上次写入 | 同上 | 标题不变；`updated_at` **仍前进**（取提交时钟，不得倒退） |
| `TP2-R9-CLEAR` | 通知显式置空 | 同上 | 标题为 `None`；摘要为 `None` |
| `TP2-R9-CLEAR-WITH-MODE-CHANGE` | 置空 + 同批 `session.mode.changed` | 同上 | 提交成功、两条事件都在、注入版本 == 落盘版本、版本 >1 |
| `TP2-R9-NO-RENAME-ENTRY` | 命令目录 + core 授权镜像 | `include_str!` + `required_grant` | 无任何改名命令；目录每条命令都在镜像里（防空转） |
| `TP2-R9-STORE-UNCHANGED-CLEAR` | `Unchanged` + `Some(None)` + 同批状态/恢复列 | `SessionStore::commit` | 版本 +1、`updated_at` 前进、标题 `NULL`、`state`/`agent_session_id`/`workspace_cwd` 三列不丢 |
| `TP2-R9-STORE-UNCHANGED-KEEP` | `Unchanged` + `None` | 同上 | 标题保留、版本 +1、`updated_at` 前进 |
| `TP2-FIXTURE-SANITY` | 夹具防空转 | `sink` + `flush` + 文件存在性 | 库文件存在；提交前后 `owned_event` 行数 0 → 1 |

### A.3 R7 的边界矩阵（11 条）

| 输入 | 期望 | 用例 |
| --- | --- | --- |
| 区内深层绝对路径 | 相对化、无根片段、无 `..`、不标越界 | `TP2-R7-INSIDE` |
| 区外绝对路径 | 越界 + 只给文件名 | `TP2-R7-OUTSIDE`、`TP2-R7-ABS-OUTSIDE` |
| 同字符串前缀（`…\api` vs `…\api-tools`） | 越界 + 只给文件名 | `TP2-R7-SAME-PREFIX` |
| 工作区根未登记（`workspace_cwd = NULL`） | 从严越界 + 只给文件名 | `TP2-R7-NO-ROOT` |
| `../etc/passwd`（1 级） | 越界 + `passwd` | `TP2-R7-RELATIVE-ESCAPE` |
| `../../etc/passwd`、`../../Users/alice/.ssh/id_rsa`（多级） | 越界 + 只给文件名 | `TP2-R7-ESCAPE-MANY` |
| 恰好是工作目录根 | 不得下发根片段或 `..` | `TP2-R7-ROOT-ITSELF` |
| `sub/../file.txt` | 仍区内 | `TP2-R7-INNER-DOTDOT` |
| 绝对形式含内部 `..`（`api/../api/file.txt`） | 仍区内、不下发 `..` | `TP2-R7-ABS-INNER-DOTDOT` |
| 同名越界文件 ×2（`/etc/nginx/nginx.conf`、`/tmp/nginx.conf`） | 两条事件、`changeId` 不同 | `TP2-R5-R7-SAME-NAMED-OUTSIDE` |

### A.4 约定（入口与观察点）

- **应用入口**：`Broker::sink` + `Broker::flush`（会话级）、`Broker::commit_node_event`（节点级）。
- **观察点**：`owned_session`/`owned_event` 的原始 SQL、`SessionStore::load`/`load_recovery`/`list`、
  `ReadView::replay`/`event_payload`。
- **前置条件**：`SqliteStore::open` 在已存在的临时数据目录上；工作目录根按**产品列写路径**
  （`SessionUpdate.workspace_cwd`）登记，未登记时为 `NULL`；时间戳一律来自测试的单调时钟（core 不读系统时间）。

---

## B. 交付与实测（`phase: design-author`）

### B.1 交付提交

**`5ba44fd35be2cd3717006c0995a8404c3315f388`**（分支 `feat/tp2-behaviour`）。提交链：

```
5ba44fd test(storage): TP2 把 updated_at 前进升级为判别 Daemon 提交时钟的断言（事件 at 早于上次写入仍须前进）
3b63781 test(storage): TP2 补绝对路径含内部 .. 的 R7 回归防护
f22bf4f test(storage): TP2 补 ModeChange::Unchanged 分支的标题保留用例与 5 条 R7 回归防护
7cdc697 test(storage): TP2 core 生产者行为判定（R5–R9）——派生唯一性、行级 diff 边界、越界标识、节点级事件、标题单向更新
1550909 fix(core): WP3 第三轮——前导 .. 显式计数修 R7 越界泄露，通知只带更新时间时推进权威 updated_at  ← 上游
3b6fe41 test(app): 补齐 AC1 的 file.changed 落库与 origin 序回放、会话标题更新两项断言  ← 固定起点
```

交付提交**包含上游修复 `1550909`**，因此从 `5ba44fd` 即可复现 PV2 全绿（无需另取上游提交）。

### B.2 `[PV2]`（本包主检查）：行为测试全绿

```
$ cd D:\Project\acp-remote\.worktrees\tp2
$ CARGO_TARGET_DIR='D:\Project\acp-remote\.target-wt\tp2' cargo test --locked -p core -p storage-sqlite -p agent-host --all-features
PV2_EXIT=0
```

28 个 suite 全部 `ok`，**463 passed / 0 failed / 2 ignored**（2 个 ignored 是既有用例自带说明的
`commit.rs` 的 crash-recovery 子用例与 `migration.rs` 的夹具生成器，均**不属本包**；本包无任何 `#[ignore]`）。
本包新增 suite 的实测输出：

```
running 31 tests
test a_deleted_file_counts_every_line_as_deleted ... ok
test a_new_file_counts_every_line_as_added ... ok
test a_notification_without_a_title_keeps_the_existing_one ... ok
test a_relative_form_inside_the_workspace_is_kept_relative ... ok
test a_relative_path_escaping_the_root_is_outside ... ok
test a_string_prefix_is_not_treated_as_inside_the_workspace ... ok
test a_tool_call_without_a_typed_diff_derives_nothing ... ok
test an_absolute_path_with_an_inner_dotdot_that_stays_inside_is_inside ... ok
test an_explicit_null_clears_the_title_into_unnamed ... ok
test an_inside_path_is_relativized_without_the_root_fragment ... ok
test an_inner_dotdot_that_stays_inside_is_inside ... ok
test an_outside_path_is_marked_and_carries_only_the_file_name ... ok
test an_over_budget_diff_omits_the_line_counts ... ok
test an_unregistered_workspace_root_falls_back_to_outside ... ok
test another_absolute_outside_path_keeps_only_the_file_name ... ok
test clearing_a_title_in_a_batch_with_a_version_event_keeps_the_whole_batch ... ok
test deriving_leaves_the_acp_document_byte_identical_through_real_storage ... ok
test equal_line_counts_still_report_a_non_zero_change ... ok
test multiple_leading_parent_traversals_keep_only_the_file_name ... ok
test node_level_events_land_without_session_identity ... ok
test node_level_shape_violations_fail_closed_without_writing ... ok
test session_activity_never_manufactures_a_connection_event ... ok
test the_command_catalog_has_no_rename_entry ... ok
test the_fixture_commits_into_a_real_database_file ... ok
test the_first_notification_writes_the_title_and_the_summary_follows ... ok
test the_workspace_root_itself_never_leaks_its_fragment ... ok
test two_distinct_changes_derive_two_and_a_repeated_one_derives_once ... ok
test two_same_named_outside_files_derive_two_changes_on_real_storage ... ok
test unchanged_mode_title_clear_writes_every_column_in_the_batch ... ok
test unchanged_mode_without_a_title_keeps_it_and_still_advances_the_version ... ok
test unknown_line_counts_are_omitted_rather_than_zeroed ... ok

test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### B.3 红→绿的实测轨迹（不采信修复者自述，本包独立复跑）

| 版本 | 本包 suite | 说明 |
| --- | --- | --- |
| `3b6fe41`（固定起点，含「上一轮」缺陷复现状态） | **29 passed / 2 failed** | 失败恰为 `TP2-R7-RELATIVE-ESCAPE` 与 `TP2-R7-ESCAPE-MANY` |
| `1550909`（WP3 第三轮修复） | **31 passed / 0 failed** | 本包独立复跑确认 |

基线两条红测的实测落库 view（即缺陷证据）：

```json
{"changeId":"…","kind":"modified","displayPath":"etc/passwd","summary":"modified etc/passwd","addedLines":"1","deletedLines":"1"}
```

**根因**（本包定位，WP3 采用同口径修复）：`derive.rs::normalize` 原先用 `!out.pop()` 计前导 `..`，而
`PathBuf::pop()` 在弹出 `Prefix`/`RootDir`（`C:\`、`\\server\share`）时返回 `true`，第二个前导 `..`
把根弹掉、计数丢失，`../../etc/passwd` 被当成区内相对路径，把工作区外的目录结构下发到 `displayPath`。
spec 出处：`core-derived-events` R7 与 `workspace-resolution` R7「展示路径 MUST NOT 包含回退层级或工作目录根
的任何片段」；`design.md` D5 明确把 `../../etc/passwd` 点名为动机输入。修复后 `normalize` 改为
`Normalized { path, escapes }` 逐组件显式计数。

**回归防护（防修复过度）**：修复改的是路径归一化的公共逻辑，本包为此补了 5 条「必须仍然区内」的用例——
`TP2-R7-INSIDE`、`TP2-R7-INNER-DOTDOT`、`TP2-R7-ABS-INNER-DOTDOT`、`TP2-R7-ROOT-ITSELF`、
`TP2-R7-SAME-PREFIX`（含前置条件断言，防空转），另加 `TP2-R7-NO-ROOT`、`TP2-R7-OUTSIDE`、
`TP2-R7-ABS-OUTSIDE`。这 8 条在 `1550909` 上全绿。

### B.4 `[PV1]`（Project Verify）

见 `checks` 段的 `PV1` 行（命令、`cwd`、日志路径、退出码）。

---

## C. 对「WP3 自带的 `title_write.rs` 覆盖是否足够」的判断

**结论：不足，但缺口很小——本包已在自己的范围内补齐，未改 `title_write.rs`（符合区域收窄注记）。**

`title_write.rs` 现有 4 条用例：`SET`（`ModeChange::Set` 语句）、`CLEAR + 同批 version 事件`（`Unchanged`
语句，其 `session_update()` 默认 `Unchanged`）、`None` 不改（`Set` 语句）、`CLEAR + ModeChange::Set`（`Set` 语句）。
逐条对照存储层语句分支（`session_store.rs` 的 `match &update.mode`，标题 CASE 为
`CASE WHEN ?clear THEN NULL WHEN ?value IS NULL THEN title ELSE ?value END`）：

| 语句分支 | 标题三态 | `title_write.rs` | 本包补充 |
| --- | --- | --- | --- |
| `ModeChange::Set` | `Some(Some)` | ✅ `setting_a_title_writes_it_and_bumps_the_version` | — |
| `ModeChange::Set` | `None` | ✅ `an_unchanged_title_keeps_the_existing_value...` | — |
| `ModeChange::Set` | `Some(None)` | ✅ `clearing_a_title_with_a_mode_change_writes_both_columns` | — |
| `ModeChange::Unchanged` | `Some(None)` | ✅ `clearing_a_title_bumps_the_version_and_keeps_the_rest_of_the_batch` | 经 broker 的真实组合根路径（`TP2-R9-CLEAR-WITH-MODE-CHANGE`） |
| `ModeChange::Unchanged` | `None` | ❌ **缺** | ✅ `TP2-R9-STORE-UNCHANGED-KEEP`（钉住 `WHEN ?value IS NULL THEN title` 这一支） |
| `ModeChange::Unchanged` | `Some(Some)` | ❌ **缺（直连端口层）** | ✅ `TP2-R9-STORE-UNCHANGED-CLEAR` 的前置写入 + `TP2-R9-SET` 的端到端链路 |

**具体缺两条**：① `ModeChange::Unchanged + title: None`（「不改该列」在该语句下的 `CASE` 分支）；
② `ModeChange::Unchanged + title: Some(Some(_))` 的直连端口用例。**关键缺口是 ①**：`title_write.rs` 覆盖
`Unchanged` 语句的两条用例里，置空那条确实走 `Unchanged`，但「不改」那条走的是 `Set`（注释也写明
「`Set` 的语句里标题哨兵是 `?11`」），所以 `Unchanged` 语句的 `?value IS NULL THEN title` 分支在
**直接端口层**没有用例。而组合根路径（broker 的 `commit_chunk`）恒用 `ModeChange::Unchanged`——
这恰是生产最常走的语句。本包用 `TP2-R9-STORE-UNCHANGED-KEEP` 与 `TP2-R9-STORE-UNCHANGED-CLEAR` 补上，
两条都在 `crates/storage-sqlite/tests/`（TP2 写入范围）内。

---

## D. 未验证内容

- **进程生命周期的上报时机（R8 的 host 侧）**：`crates/agent-host` 的「首次建立只上报一次」「复用既有
  进程不重复上报」「超限退出时断开不早于退出判定」「未绑定 `NodeEvents` 不静默丢弃」属 **WP4 的写入范围**，
  TP2 不改该目录。这些断言由 WP4 的用例承载并在 `[PV2]` 的 `-p agent-host` 中执行；本包**未**在自己
  的用例里复核它们，因此**不**把它们写成本包的 PASS。
- **真实 ACP 子进程与端到端组合根（AC1）**：位于 `crates/app/tests/node_link_e2e.rs`，属 main 的收口项与
  `crates/app/tests/`（不在 TP2 写入范围，且 `[PV2]` 的包列表不含 `app`）。本包未执行。
- **浏览器 E2E / PWA 客户端**：本包不涉及。
- **`crates/app/tests/daemon_lifecycle.rs` 的已知非确定性 flake**：`[PV2]` 不含 `app`；`[PV1]` 的
  `check:rust` 含 `--workspace`，本轮**未命中**该 flake（见 PV1 日志）；未使用 `#[ignore]`、未删用例、未弱化断言。
- **`error-cursor-expired` 的 `details.reason` 词表**：TP1 已登记的既有缺口，与本包无关。
- **R6 的 Myers 距离正确性**：本包只覆盖「省不省、省得对不对」的可观察结果（含超预算省略）；Myers 算法
  本身的正确性由 `derive.rs` 内联的暴力对拍用例承载，本包不重跑。

---

## evidence_paths

- 交付 worktree：`.worktrees/tp2`（分支 `feat/tp2-behaviour`，HEAD `5ba44fd35be2cd3717006c0995a8404c3315f388`）
- 本报告：`openspec/changes/sync-scope-and-pwa-client/reports/deliver-tp2-r1.md`
- PV2 日志：`reports/PV2-tp2-r1.log`（`cargo test --locked -p core -p storage-sqlite -p agent-host --all-features`）
- PV1 日志：`reports/PV1-tp2-r1.log`（`npm run verify`）
- 上游修复：`1550909b13bc59dd2ad635a45eff4de4e9699150`（WP3 第三轮）
- `CARGO_TARGET_DIR`：`D:\Project\acp-remote\.target-wt\tp2`

## resource_cleanup

- 复用既有 `CARGO_TARGET_DIR=D:\Project\acp-remote\.target-wt\tp2`，**未新建** target 目录（磁盘提示）。
- `.worktrees/tp2/node_modules` 为指向主检出 `node_modules` 的 junction（gitignored），供 `npm run verify`
  的 `check:agentic` 解析项目本地引擎；未写入任何依赖。
- 未启动长驻进程；未改动 `.worktrees/` 下其他包、主检出、`schemas/`、`docs/`、`fixtures/`。
- 未执行 push / merge / archive；未修改 `verification.md` 或任何规划文件。
- 上游修复提交 `1550909` 由 WP3 执行者引入本分支（`git rebase` 到该提交之上）；本包**未**修改其内容。

---

## checks

```yaml
checks:
  - id: PV2
    work_package: TP2
    command: "CARGO_TARGET_DIR='D:\\Project\\acp-remote\\.target-wt\\tp2' cargo test --locked -p core -p storage-sqlite -p agent-host --all-features（cwd=.worktrees/tp2）"
    scope: "工作包 Project Verify：MU2 三个 crate 的行为测试（含本包新增的 derived_events_behaviour 31 条）"
    exit_code: 0
    log_path: "reports/PV2-tp2-r1.log"
    result: "PASS：28 个 suite 全 ok，463 passed / 0 failed / 2 ignored（2 个既有的带说明 ignore，非本包）"
  - id: PV1
    work_package: TP2
    command: "CARGO_TARGET_DIR='D:\\Project\\acp-remote\\.target-wt\\tp2' npm run verify（cwd=.worktrees/tp2）"
    scope: "工作包 Project Verify：check 十道 + check:rust 三条"
    exit_code: 0
    log_path: "reports/PV1-tp2-r1.log"
    result: "PASS（PV1_EXIT=0）：check 十道全绿（schemas 149/51、commands 13、errors 58/2、features 13、assets 213 fixture files、acp 71 rows、docs 415 links/9058 refs、boundaries 12 crates、drift 36 DDL/15 traits/96 methods、agentic 21 specs + 17 host entrypoints）+ check:rust 三条全绿（fmt/clippy --workspace/test --workspace，96 个 suite 全 ok、1229 passed / 0 failed）"
```

---

## 交接要点（给 main）

1. **交付提交**：`5ba44fd35be2cd3717006c0995a8404c3315f388`（分支 `feat/tp2-behaviour`），父为上游修复
   `1550909`（WP3 第三轮），固定起点 `3b6fe41`。MU2 候选可直接以它构造。
2. **本包对产品代码零改动**：`git diff --stat 1550909..5ba44fd` 只有 `derived_events_behaviour.rs`（+1867）。
   `crates/core/src/{derive,broker}.rs` 的改动全部来自上游 WP3 修复提交 `1550909`，非本包。
3. **R7 红→绿已独立复跑**：`3b6fe41` 上 29/2，`1550909` 上 **31/0**（本包不采信修复者自述）。
4. **`title_write.rs` 判断**：不足（缺 `ModeChange::Unchanged + title:None` 的 `?value IS NULL THEN title`
   分支，以及一个直连端口的 `Unchanged + Some(Some)`）；本包已在自己的文件内补齐，未改 `title_write.rs`。
5. **待办**：tasks 3.19 的 `reports/PV2.log`——本包按检查 ID 交付 `reports/PV2-tp2-r1.log`；
   若门禁要求固定的 `reports/PV2.log` 文件名，请告知，我可按该名另存一份副本。
6. **review 归属**：tasks 4.9 需非作者 reviewer 检视固定提交 `5ba44fd`。

```agentic-handoff
version: 1
agent_context:
  agent_id: "testing-2-r1"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "4.7"
    work_package: TP2
    role: tester
    phase: design
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "5ba44fd35be2cd3717006c0995a8404c3315f388"
    evidence_type: DESIGN
    evidence_id: tp2-design-r1
    report_path: "reports/deliver-tp2-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "TP2 对 R5–R9 行为可判定面的需求映射、稳定用例 ID 与场景/步骤/断言，见本报告 A 节（含 R7 的 11 条边界矩阵与 A.4 的应用入口/观察点约定）；同批交付的可运行用例已在本提交中。"
    source_evidence: NOT_APPLICABLE
  - task_id: "4.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "5ba44fd35be2cd3717006c0995a8404c3315f388"
    evidence_type: DELIVERY
    evidence_id: tp2-delivery-r1
    report_path: "reports/deliver-tp2-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "固定提交 5ba44fd = 上游 WP3 修复 1550909 + 1 个新增测试文件（crates/storage-sqlite/tests/derived_events_behaviour.rs，1867 行 / 31 条用例，全部经真实 Broker + 真实 SqliteStore 的公开端口驱动）；对 crates/core/**、crates/app/**、crates/agent-host/**、schemas/**、docs/**、fixtures/**、crates/sync-protocol/** 与既有测试文件零改动。DELIVERY 声明「可运行用例齐备」，PV2 已在同一提交上 exit 0。"
    source_evidence: NOT_APPLICABLE
  - task_id: "4.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "5ba44fd35be2cd3717006c0995a8404c3315f388"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "reports/deliver-tp2-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo test --locked -p core -p storage-sqlite -p agent-host --all-features 在交付提交 5ba44fd 上 exit 0：28 个 suite 全 ok，463 passed / 0 failed / 2 ignored（2 个 ignore 为既有用例自带说明，非本包）；本包新增 suite derived_events_behaviour 31/31。日志 reports/PV2-tp2-r1.log。"
    source_evidence: NOT_APPLICABLE
  - task_id: "4.8"
    work_package: TP2
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "5ba44fd35be2cd3717006c0995a8404c3315f388"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/deliver-tp2-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "npm run verify 在交付提交 5ba44fd 上 exit 0：check 十道全绿（schemas 149/51、commands 13、errors 58/2、features 13、assets 213 fixture files、acp 71 rows、docs 415 links/9058 refs、boundaries 12 crates、drift 36 DDL/15 traits/96 methods、agentic 21 specs + 17 host entrypoints）与 check:rust 三条全绿（fmt、clippy --workspace --all-targets --all-features、test --workspace --all-features：96 suite ok / 1229 passed / 0 failed）；已知 flake daemon_lifecycle 未命中。日志 reports/PV1-tp2-r1.log。"
    source_evidence: NOT_APPLICABLE
checks:
  - id: PV2
    work_package: TP2
    command: "CARGO_TARGET_DIR='D:\\Project\\acp-remote\\.target-wt\\tp2' cargo test --locked -p core -p storage-sqlite -p agent-host --all-features (cwd=.worktrees/tp2)"
    scope: "工作包 Project Verify：MU2 三个 crate 的行为测试（含本包新增 31 条）"
    environment: "Windows x64；rustc 1.98.1 / cargo 1.98.1（仓库 rust-toolchain.toml 固定）；CARGO_TARGET_DIR=.target-wt/tp2（复用既有目录）；无外部服务；行为测试用真实 SQLite 文件与假后端夹具"
    exit_code: 0
    log_path: reports/PV2-tp2-r1.log
    result: PASS
  - id: PV1
    work_package: TP2
    command: "CARGO_TARGET_DIR='D:\\Project\\acp-remote\\.target-wt\\tp2' npm run verify (cwd=.worktrees/tp2)"
    scope: "工作包 Project Verify：check 十道 + check:rust 三条"
    environment: "Windows x64；Node v22.22.0；npm run check 的项目本地引擎经 .worktrees/tp2/node_modules junction 解析；CARGO_TARGET_DIR=.target-wt/tp2"
    exit_code: 0
    log_path: reports/PV1-tp2-r1.log
    result: PASS
test_delivery:
  TP2:
    kind: automated
    artifacts:
      - reports/tp2/derived_events_behaviour.rs
    basic_checks:
      - PV2
      - PV1
```

> **`artifacts` 的路径口径**：该字段相对**变更目录**解析，因此这里登记交接副本
> `reports/tp2/derived_events_behaviour.rs`（交付提交 `5ba44fd` 的
> `crates/storage-sqlite/tests/derived_events_behaviour.rs` 的副本；除本副本顶部 4 行来源说明外内容相同，
> 已用 `diff`（行尾 CRLF/LF 归一后）核对：归一后两侧 sha256 均为
> `f5c9e6c37052092b26b5bb9d2eb13102473755eb135c7b4df59cf854cec4732e`）。
> 权威可运行副本在 worktree 的 `crates/storage-sqlite/tests/` 下，由 `PV2` 的
> `cargo test --locked -p storage-sqlite --test derived_events_behaviour` 执行（31/31）。
