<!-- TP1 交付报告（MU1b，第 1 轮）。tester 只交付设计、可运行用例与基础检查证据；不判独立 review、合入、E2E 与最终验收。 -->

# TP1 合同与固定向量交付（deliver-tp1-r1）

## Shared Report

- **task_id**: 4.4（`phase: design`，DESIGN）/ 4.5（`phase: design-author`，DELIVERY + CHECK）
- **work_package**: TP1（交付单元 MU1b）
- **role**: tester（`testing-1-r1`）
- **phase**: design-author（本报告同时承载 `design` 阶段的 DESIGN 行与 `design-author` 阶段的 DELIVERY/CHECK 行）
- **agent_context**: `agent_id: TesterTp1`，`isolation: fork_turns=none`（新实例接管既有工作区；未继承 WP1/WP2 或任何产品实现者的对话）
- **target_revision**: `33040d78324ff51be49219fcb3aac054d0100cc9`（分支 `feat/tp1-contract-vectors`；父提交 = MU1a 合入提交 `ad9ad3c6b532ce0cc9366c8cc34a217f9f466933`）
- **scope**: `crates/sync-protocol/tests/` 与 `fixtures/sync/v1/` 的**新增与追加**；不触碰 `crates/sync-protocol/src/`、`schemas/`、`docs/`、`openspec/changes/**`（本报告除外）
- **result**: **PASS（编写交付 + PV1 全绿）**。`npm run verify` 在交付提交上 exit 0（`check` 十道 + `check:rust` 三条）。

### 交付提交

| 项 | 取值 |
| --- | --- |
| worktree | `D:\Project\acp-remote\.worktrees\tp1` |
| 分支 | `feat/tp1-contract-vectors` |
| 固定起点（MU1a 合入提交） | `ad9ad3c6b532ce0cc9366c8cc34a217f9f466933` |
| 交付提交（target） | `33040d78324ff51be49219fcb3aac054d0100cc9` |
| 提交信息 | `test(sync): TP1 合同向量——清单快照、复合游标分页、file.changed 可选字段、state 封闭枚举` |
| `CARGO_TARGET_DIR` | `D:\Project\acp-remote\.target-wt\tp1`（独占） |

`git diff --stat ad9ad3c..33040d7`（39 files changed, **2235 insertions(+), 39 deletions(-)**）：

```
 crates/sync-protocol/tests/contract_vectors_r1_r9.rs | 813 +++++++++++++++++++
 crates/sync-protocol/tests/envelope_fixtures.rs      |  31 +++-
 crates/sync-protocol/tests/schema_drift.rs           | 100 ++++++----
 crates/sync-protocol/tests/snapshot_scope_vectors.rs | 504 ++++++++++++++++++  (新文件，R1–R4 基础形状)
 crates/sync-protocol/tests/support/mod.rs            |   5 +
 crates/sync-protocol/tests/view_projections.rs       |   3 +-
 fixtures/sync/v1/manifest.json                       | 189 ++++++++++++++++
 fixtures/sync/v1/valid/…  (13 条新增)                 | 合计 240 +
 fixtures/sync/v1/invalid/… (11 条新增)                | 合计 130 +
```

---

## 第 0 步：从 MU1a 合入提交重建 worktree

### 0.1 保全的未提交工作（重建前清单）

重建前 `.worktrees/tp1` 的 HEAD = `353ba6e`（旧基线），未提交增量如下（`git stash push -u` 保全，`git stash pop` 无冲突还原，`git status --porcelain` 逐字一致）：

**已跟踪文件的修改（2 个）**

| 文件 | 保全内容 |
| --- | --- |
| `crates/sync-protocol/tests/schema_drift.rs` | 在 `view_enums_match_schema` 内追加的两条硬编码 `AgentConnectedState`/`AgentDisconnectedState` 断言 |
| `fixtures/sync/v1/manifest.json` | 在 `cases` 末尾追加的 9 条用例登记 |

**未跟踪的新文件（10 个）**

```
crates/sync-protocol/tests/snapshot_scope_vectors.rs
fixtures/sync/v1/invalid/command-result-session-read-result-without-has-earlier.json
fixtures/sync/v1/invalid/command-session-read-before-missing-message-id.json
fixtures/sync/v1/invalid/snapshot-chunk-detail-resource-capabilities.json
fixtures/sync/v1/invalid/snapshot-chunk-detail-resource-pending-interactions.json
fixtures/sync/v1/invalid/snapshot-chunk-detail-resource-turns.json
fixtures/sync/v1/valid/command-result-session-read-earliest-page-completed.json
fixtures/sync/v1/valid/command-result-session-read-first-page-completed.json
fixtures/sync/v1/valid/command-session-read-default-page-include-only.json
fixtures/sync/v1/valid/command-session-read-second-page.json
```

保全动作与证据：`git diff > /tmp/tp1/tp1-uncommitted.patch`（87 行）、`git ls-files --others --exclude-standard > /tmp/tp1/tp1-untracked.txt`（10 行）；`git stash push -u -m "tp1-r1-pre-rebase"` → `git reset --hard ad9ad3c6b532ce0cc9366c8cc34a217f9f466933` → `git stash pop`，**无冲突**（`staged 0, unstaged 2, untracked 12` 与重建前逐字相同）。全部 12 个路径的资产随后被并入交付提交。

### 0.2 重建后实现未被回退的实测证据

```
$ git diff ad9ad3c --name-only -- crates/sync-protocol/src/
（空，0 行）

$ git diff ad9ad3c -- crates/sync-protocol/src/ | wc -l
0
```

即交付提交对 `crates/sync-protocol/src/{sync,command,views}.rs` **零改动**——MU1a 的 WP1/WP2 实现完整保留，本包只动 `tests/` 与 `fixtures/`。`git status --porcelain` 在提交后为 0 行（工作树干净）。

---

## A. 设计阶段（`phase: design`，DESIGN）

### A.1 需求映射（R1–R9 的机器可判定部分）

| Req | 本包覆盖的可判定面 | 用例位置 |
| --- | --- | --- |
| **R1** 快照只承载清单类资源 | `snapshotResource` 封闭词表 = `sessions`/`workspaces`/`agents`；五类明细资源逐条不在词表内且各带负例；`chunkCount` = 实际下发资源种类数（3）/未协商目录 feature 时 = 1 | `snapshot_scope_vectors.rs`（5 条）+ `contract_vectors_r1_r9.rs::catalog_snapshot_chunk_count_matches_three_declared_resources`、`sessions_only_snapshot_uses_single_catalog_chunk`、`no_snapshot_vector_carries_a_session_detail_resource` |
| **R2** 会话明细经分页读取 | 省略 `before`/`limit` 的默认页请求形状（两键**不存在**）；`hasEarlier` 必填且真假两态；结果缺 `hasEarlier` 被拒；`limit` 无 wire 上限（服务端收敛而非报错）；保留窗口之前显式 `cursor_expired` | `snapshot_scope_vectors.rs`（4 条）+ `contract_vectors_r1_r9.rs::session_read_limit_has_no_wire_upper_bound`、`retention_window_error_is_explicit_and_typed`、`cursor_invalid_details_reason_is_a_closed_vocabulary` |
| **R3** 游标由时间与标识复合构成 | `before` 必须两键；单字段游标（缺 `createdAt`、缺 `messageId`）与 offset 键被拒；同一 `createdAt` 不重不漏；同刻次序 ≠ `messageId` 字典序 | `snapshot_scope_vectors.rs::single_field_cursor_is_rejected`、`adjacent_pages_join_without_gaps_or_overlap` + `contract_vectors_r1_r9.rs::tie_broken_messages_are_ordered_stably_and_not_by_identifier`、`tied_timestamp_pages_join_without_gaps_or_overlap`、`both_single_field_cursor_directions_are_rejected`、`offset_style_cursor_is_rejected` |
| **R4** 分页只作用于客户端同步面 | Node Link 的 `session.read` 载荷键集恰好 `{include}` 且 `additionalProperties: false` | `snapshot_scope_vectors.rs::node_link_session_read_payload_carries_no_paging_parameters` |
| **R5** 文件改动事件由类型化 Diff 派生 | 本包只覆盖 view 字段侧（产生侧属 WP3/WP4） | 见 R6/R7 行 |
| **R6** 改动行数按行级差异统计 | `addedLines`/`deletedLines` 可选、缺失即合法（解析为 `None` 而非零）；字面量必须是非负十进制串（`+3`/数字/`null` 均拒）；必填集合不变 | `contract_vectors_r1_r9.rs::file_changed_optional_fields_may_be_absent`、`file_changed_optional_fields_carry_their_declared_values`、`file_changed_optional_fields_reject_wrong_shapes` |
| **R7** 展示路径相对化且不泄漏工作区外结构 | `outsideWorkspace` 可选 boolean；`null`/字符串被拒；越界展示路径只给文件名、不含 `..` | 同 R6 三条断言 |
| **R8** Agent 连接状态是节点级生命周期 | 两个 `state` 封闭枚举各自唯一且互不相同；非法取值（自由文本、另一事件取值、未登记值）被拒；节点级事件的 `sessionId`/`sessionSequence` 均为 `null`（键存在） | `contract_vectors_r1_r9.rs::agent_state_enums_are_unique_and_mutually_distinct`、`agent_state_rejects_values_outside_its_enum`、`node_level_agent_events_carry_no_session_identity`、`node_level_agent_events_project_to_their_state` |
| **R9** 会话标题只由 Agent 通知更新 | 命令目录（13 条）中不存在重命名命令；`session.info.changed` 视图形状 | 由 WP2 文档侧承接；本包只在 `schema_drift.rs` 覆盖 `VIEW_ENUMS` 完整性（标题本身无独立 wire 形状可钉） |

### A.2 稳定用例 ID 与场景/步骤/断言

| Case ID | 场景 | 步骤（入口） | 断言（可观察） |
| --- | --- | --- | --- |
| `TP1-R1-CATALOG` | 协商目录 feature 的快照承载三类清单资源 | 读 `sync-snapshot-catalog-{begin,chunk-workspaces,chunk-agents,end}.json` → `Envelope`+类型化 body | `chunkCount` begin==end 且 == `SnapshotResource::ALL.len()`；资源并集为三类 |
| `TP1-R1-SINGLE` | 未协商目录 feature 时缩为单类 | 读 `sync-snapshot-catalog-sessions-only-{begin,chunk}.json` | `chunkCount`==1；`resource`==`sessions`；items 非空且无目录/明细字段 |
| `TP1-R1-EXCLUDE` | 五类明细资源不进快照 | `FORBIDDEN_IN_SNAPSHOT` 表 × manifest 声明 + 类型化解码 | 逐条 `parse::<SnapshotResource>()` 得 `Enumerated{field:"resource"}`；负例 `valid:false`+`expectedKeyword:"enum"` |
| `TP1-R2-DEFAULTPAGE` | 省略分页参数时用默认页 | 读 `command-session-read-default-page-include-only.json` | `before`/`limit` 在解码后与原始 payload 里**都不存在** |
| `TP1-R2-HASEARLIER` | 有无更早内容两态 | 读 first-page / earliest-page 结果 | `hasEarlier` true / false；false 时 `messages` 非空 |
| `TP1-R2-MISSINGHAS` | 缺 `hasEarlier` 被拒 | 读 `command-result-session-read-result-without-has-earlier.json` | 类型化解码 `is_err()`；原文 `result` 里确无该键 |
| `TP1-R2-LIMITNOSUP` | 超上限 limit 被收敛而非报错 | 读 `command.schema.json#/$defs/sessionReadLimit` | 无 `maximum`；请求向量 `limit`==2 逐字承载 |
| `TP1-R2-EXPIRED` | 保留窗口之前显式不可读 | 读 `error-cursor-expired.json` → `ErrorBody` | `code==SyncCursorInvalid`、`retryable==true`、`details.reason=="cursor_expired"`、消息含「保留」 |
| `TP1-R2-EXPIREDWORD` | `reason` 是封闭词表 | 读 `errors.json` + 负例 | registry 词表恰为四值；词表外取值 `enum` 负例 |
| `TP1-R3-COMPOSITE` | 复合游标两键齐全 | 读 `command-session-read-second-page.json` | `before.createdAt`/`before.messageId` 逐字匹配；`limit`==20 |
| `TP1-R3-SINGLEFIELD` | 单字段游标被拒（两方向） | 三个 fixture + 内联 body | 三者解码 `is_err()`；`before` 恰带 1 个分量；`expectedKeyword=="required"`；两键齐全时必须被接受 |
| `TP1-R3-OFFSET` | offset 式游标被拒 | 读 `command-session-read-offset-cursor.json` | 解码 `is_err()`；`before` 带 `offset`；`expectedKeyword=="additionalProperties"` |
| `TP1-R3-TIE` | 同刻多条不重不漏 | 读同刻两页向量 | 页内升序；并集无重复；第二页整体严格更早；同刻次序 ≠ 字典序 |
| `TP1-R4-NODELINK` | Node Link 未受影响 | 遍历 `schemas/node-link/v1/command.schema.json` | 每个含 `include` 的载荷键集 == `{include}` 且 `additionalProperties:false`；防空转守卫 |
| `TP1-R6-OMITTED` | 行数判定不出时省略 | 读 `view-file-changed-stats-omitted.json` → `views::project("file.changed")` | 三字段 `is_none()`；原始 view 里三键**不存在**；四基字段齐全 |
| `TP1-R6-R7-SHAPE` | 新字段非法形状被拒 | 5 条负例 → `FileChanged` 解码 | 全部 `is_err()`；`expectedKeyword ∈ {type,pattern}`；基字段保留（失败点确为该字段） |
| `TP1-R7-OUTSIDE` | 工作区外标记 | 读 `view-file-changed-outside-workspace.json` | `outside_workspace==Some(true)`；`displayPath=="outside.txt"` 无 `..` |
| `TP1-R8-STATE` | `state` 封闭枚举 | `VIEW_ENUMS` + `ALL`/`as_str` | 两枚举各自唯一且互不相同；与登记逐条同序 |
| `TP1-R8-STATENEG` | 非法 `state` 被拒 | 3 条负例 → `views::project` | 全部 `Err`；`expectedKeyword=="enum"`；另一事件取值仍被接受（防空转） |
| `TP1-R8-NODELEVEL` | 节点级事件无会话标识 | 读 `view-agent-{connected,disconnected}.json` → `event::Body` | `session_id`/`session_sequence` 均 `is_null()` 且**键存在** |
| `TP1-F3-ENUMMIRROR` | 枚举镜像门禁自动覆盖 | `schema_drift.rs::view_enums_match_schema` | 双向：登记条目 ↔ Rust 镜像 `ALL`/`as_str` 逐条同序；`VIEW_ENUMS` 有未对照条目即失败 |

### A.3 设计与用例覆盖的缺口（明确不在本包）

- **产生侧行为**（Diff 派生唯一性、行级 diff 边界、路径规范化前缀判定、进程生命周期上报、标题通知映射）：属 WP3/WP4/TP2，本包只钉 wire 形状。
- **浏览器 E2E**：本包不涉及（`clients/app` 尚未落地，Main E2E 在 plan 中为 `not-applicable`）。
- **`session.read` 的跨页拼接语义**中「服务端按什么顺序取页」属 WP3；本包只钉客户端可观察到的页内/页间性质。

---

## B. 固定向量（负例与新增用例）

`fixtures/sync/v1/` 只增不改：WP1/WP2 的既有条目**未改动一条**；`manifest.json` 仅在 `cases` 末尾追加 23 条（103 → 126）。

### B.1 新增合法向量（13 条）

| fixture | 覆盖 |
| --- | --- |
| `valid/sync-snapshot-catalog-begin.json` / `-chunk-workspaces` / `-chunk-agents` / `-end.json` | R1 三类清单资源 + `chunkCount: 3` 口径一致 |
| `valid/sync-snapshot-catalog-sessions-only-{begin,chunk}.json` | R1 未协商目录 feature 时缩为单类，`chunkCount: 1` |
| `valid/command-session-read-tied-timestamps-page.json` / `valid/command-result-session-read-tied-timestamps-{page,next-page}.json` | R3 同一 `createdAt` 两条消息跨页不重不漏 |
| `valid/error-cursor-expired.json` | R2 保留窗口之前显式 `sync.cursor_invalid` + `details.reason: cursor_expired` |
| `valid/view-file-changed-stats-omitted.json` | R6 三可选字段缺失合法（省略而非填零） |
| `valid/view-file-changed-outside-workspace.json` | R7 `outsideWorkspace: true` + 只给文件名 |

### B.2 新增负例（11 条）

| fixture | 失败点（manifest `expectedKeyword`） |
| --- | --- |
| `invalid/snapshot-chunk-detail-resource-{turns,pending-interactions,capabilities}.json` | `enum`（`items` 非空且逐字段合法，唯一失败原因是 `resource` 越表） |
| `invalid/command-session-read-offset-cursor.json` | `additionalProperties`（offset 式游标） |
| `invalid/command-session-read-single-field-before-created-at.json` | `required`（缺 `messageId`） |
| `invalid/error-cursor-expired-unknown-reason.json` | `enum`（`details.reason` 词表外） |
| `invalid/view-agent-connected-state-{unknown,free-text}.json` / `invalid/view-agent-disconnected-state-idiom.json` | `enum`（`state` 封闭词表） |
| `invalid/view-file-changed-added-lines-{null,number}.json` / `-deleted-lines-signed.json` / `-outside-workspace-{string,null}.json` | `type`/`pattern`（可选字段出现时形状必须正确） |

> **`schemaPointer` 用法**：上述 8 条视图负例是**片段对象**（不是完整 WSS 消息），按仓库既有先例（`fixtures/acp/v1` 的 `ToolCallLocation`）经 `schemaPointer` 指向 `event-views.schema.json#/$defs/*`；`error-cursor-expired-unknown-reason.json` 指向 `common.schema.json#/$defs/errorCode`。三步各自验证其失败原因：ajv（`check:schemas` 的 `expectedKeyword`）、类型化层（Rust 断言）、失败点自证（`before`/`state`/基字段保留）。

---

## C. Rust 契约测试

### C.1 新增 `crates/sync-protocol/tests/contract_vectors_r1_r9.rs`（17 条）

覆盖 R1 清单快照与 `chunkCount`、R2 保留窗口与 `limit` 上限语义、R3 同刻不重不漏/游标形状、R6/R7 `file.changed` 三字段、R8 `state` 枚举与节点级语义。与既有 `snapshot_scope_vectors.rs`（8 条，R1–R4 基础形状）分工互补。

```
running 17 tests
test agent_state_enums_are_unique_and_mutually_distinct ... ok
test retention_window_error_is_explicit_and_typed ... ok
test tie_broken_messages_are_ordered_stably_and_not_by_identifier ... ok
test file_changed_optional_fields_carry_their_declared_values ... ok
test tied_timestamp_pages_join_without_gaps_or_overlap ... ok
test node_level_agent_events_project_to_their_state ... ok
test session_read_limit_has_no_wire_upper_bound ... ok
test file_changed_optional_fields_may_be_absent ... ok
test no_snapshot_vector_carries_a_session_detail_resource ... ok
test sessions_only_snapshot_uses_single_catalog_chunk ... ok
test cursor_invalid_details_reason_is_a_closed_vocabulary ... ok
test agent_state_rejects_values_outside_its_enum ... ok
test offset_style_cursor_is_rejected ... ok
test node_level_agent_events_carry_no_session_identity ... ok
test catalog_snapshot_chunk_count_matches_three_declared_resources ... ok
test both_single_field_cursor_directions_are_rejected ... ok
test file_changed_optional_fields_reject_wrong_shapes ... ok

test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### C.2 关闭 F3 / MU2A-R1-F1：`schema_drift.rs` 枚举等值断言

既有发现（`review-wp2-r1` 的 F3 与 `review-mu1a-candidate-r1` 的 MU2A-R1-F1，结论一致：**门禁覆盖缺口，非错误值**，明确归 TP1）已落地。**按建议把第三方向改为遍历 `VIEW_ENUMS` 驱动**，而非继续硬编码枚举清单：

- 对照表 `mirrors: &[(&str, &str, Mirror)]` 登记 6 个封闭 enum（`PlanPriority`/`PlanStatus`/`ElicitationAction`/`TerminalStream`/`AgentConnectedState`/`AgentDisconnectedState`）；
- 双向断言：每个登记条目必须有镜像且 `ALL`/`as_str` 与登记**逐条同序**；`VIEW_ENUMS` 里任何**未对照**的条目都会失败（`unmirrored` 断言），因此新增封闭 enum 时门禁**自动覆盖**，不再需要每次手工同步；
- 证据：`schema_drift.rs` 行 285–359 的 diff（+100/−39），`view_enums_match_schema ... ok`。

> 前一执行者称已在工作区追加过：独立检视核实**确为真**（重建前 `git status` 显示 `schema_drift.rs` 已修改，且内容是两条硬编码断言）。但那是旧基线 `353ba6e` 上的未提交内容，**不在任何提交里**；本包在新基线 `ad9ad3c` 上重做并改为驱动式，同时把它固定进交付提交。

### C.3 关联计数常量（同批更新）

| 文件 | 常量 | 旧值 → 新值 | 依据 |
| --- | --- | --- | --- |
| `envelope_fixtures.rs` | `EXPECTED_VALID_MESSAGE_CASES` | 73 → **89** | 合法 message-schema 用例实测 |
| `envelope_fixtures.rs` | `EXPECTED_ENVELOPE_REJECTED` | 2 → **2** | 未变 |
| `envelope_fixtures.rs` | `EXPECTED_BODY_REJECTED` | 14 → **21** | body 层拒绝的负例实测 |
| `envelope_fixtures.rs` | `EXPECTED_TARGETED_PAYLOADS` | （新增）**9** | `schemaPointer` 片段用例 |
| `view_projections.rs` | `EXPECTED_VIEW_CASES` | 36 → **38** | 两条新 `file.changed` 视图夹具 |

### C.4 `envelope_fixtures.rs` 的 `schemaPointer` 跳过类（本包引入）

新增的 8 条视图负例与 1 条 errorCode 负例是**片段对象**而非完整 WSS 消息，不能走「信封 + 类型化 body」；此前该测试会对非 message-schema 且非 pairing 的用例直接 `panic`。本包新增第三类计数（`EXPECTED_TARGETED_PAYLOADS = 9`），并保留「非 pairing 就必须带 `schemaPointer`」的守卫——它同时防止日后有人把一条**完整消息**负例误登记成片段而逃过闸门。为读取该字段，`tests/support/mod.rs` 的 `ManifestCase` 增加 `schema_pointer`（`#[allow(dead_code)]` 模块，无回归）。

---

## C′. 两个计数常量的逐条计数依据（实测，非估算）

`fixtures/sync/v1/manifest.json` 共 **126** 条：`valid`+message-schema **89**、`invalid`+message-schema **23**、pairing **5**、`schemaPointer` 片段 **9**。

**`EXPECTED_VALID_MESSAGE_CASES = 89`** —— 全部 `valid:true` 且 `schema` 指向 `message.schema.json` 的条目：

- MU1a（`ad9ad3c`）实测 **73**（WP1 定下的中间值）；
- 本包新增 **16**：4（`sync-snapshot-catalog-{begin,chunk-workspaces,chunk-agents,end}`）+ 2（`sync-snapshot-catalog-sessions-only-{begin,chunk}`）+ 3（`command-session-read-tied-timestamps-page`、`command-result-session-read-tied-timestamps-{page,next-page}`）+ 1（`error-cursor-expired`）+ 2（`command-session-read-default-page-include-only`、`command-session-read-second-page`）+ 2（`command-result-session-read-{first,earliest}-page-completed`）+ 2（`view-file-changed-{stats-omitted,outside-workspace}`）；
- 73 + 16 = **89**。

**`EXPECTED_BODY_REJECTED = 21`** —— 信封合法、body 层被类型化拒绝的负例：

- MU1a 实测 **14**；
- 本包新增 **7**：3（`snapshot-chunk-detail-resource-{turns,pending-interactions,capabilities}`）+ 1（`command-session-read-offset-cursor`）+ 1（`command-session-read-before-missing-message-id`）+ 1（`command-result-session-read-result-without-has-earlier`）+ 1（`command-session-read-single-field-before-created-at`）；
- 14 + 7 = **21**。

**`EXPECTED_ENVELOPE_REJECTED = 2`**（未变）：`invalid/ping-without-connection.json`、`invalid/sequence-is-number.json`。

**`EXPECTED_SKIPPED_PAIRING = 5`**（未变）：`valid/pairing-*.json` 与 `valid/pairing-status-request.json`。

**`EXPECTED_TARGETED_PAYLOADS = 9`**（新增）：8 条 `event-views.schema.json#/$defs/*` 负例 + 1 条 `common.schema.json#/$defs/errorCode` 负例。

**`EXPECTED_VIEW_CASES = 38`**（`view_projections.rs`）：MU1a 的 36 + 本包 `view-file-changed-{stats-omitted,outside-workspace}` 的 2 条 `viewDef` 绑定。

实测输出（`--nocapture`）：

```
sync envelope: 89 条合法消息全部保真并类型化往返一致，2 条被信封层拒绝，21 条被 body 层拒绝，
跳过 5 条非 WSS 用例、9 条 schemaPointer 子模式用例
```

---

## D. Project Verify（PV1）

```
$ cd .worktrees/tp1
$ CARGO_TARGET_DIR='D:\Project\acp-remote\.target-wt\tp1' npm run verify
VERIFY_EXIT=0
```

命令范围与 `package.json` 逐字一致：`npm run check && npm run check:rust`。

| 门禁 | 结果 |
| --- | --- |
| `check:schemas` | PASS — `schema fixtures OK: 149 valid, 51 invalid (ajv Draft 2020-12), 41 event views bound` |
| `check:commands` | PASS — 13 commands |
| `check:errors` | PASS — 58 codes / 2 protocols |
| `check:features` | PASS — 13 feature ids |
| `check:assets` | PASS — 17 schemas, **213 fixture files**, 12 transcript vectors, 20 negative vectors, 2 SAS |
| `check:acp` | PASS — 25 methods / 11 updates / 5 content blocks / 3 tool content types / 19 capabilities / 71 rows |
| `check:docs` | PASS — 415 links / 9052 section refs |
| `check:boundaries` | PASS — 12 crates |
| `check:drift` | PASS — 36 DDL / 15 traits / 96 methods |
| `check:agentic` | PASS — agentic 0.4.0 / openspec 1.13.0；21 specs passed / 0 failed；17 个宿主入口文件 |
| `check:rust` ① `cargo fmt --all -- --check` | PASS（本包全程按 LF 落盘后 exit 0） |
| `check:rust` ② `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS |
| `check:rust` ③ `cargo test --locked --workspace --all-features` | PASS — 全 suite ok，**0 failed** |

- **日志路径**：`D:\Project\acp-remote\.target-wt\tp1\logs\PV1-verify.log`（1802 行，`sha256:468ecae4349b1603450a22caec6aba1c3caded7d809a048dec1713b1bdd80c54`）
- **已知 flake**：本轮**未出现** `crates/app/tests/daemon_lifecycle.rs` 的非确定性失败；`cargo test --locked --workspace --all-features` 两次完整运行均 exit 0（121s / 212s，92 suites ok）。未修改该用例，未加 `#[ignore]`。
- **未削减门禁**：全包**未**删用例、未弱化断言、未加 `#[ignore]`、未放宽 schema。唯一放宽的是 `envelope_fixtures.rs` 新增的片段跳过类，其语义是「片段不走信封」，并带守卫（非 pairing 必须带 `schemaPointer`）。

---

## E. 未验证内容与待补项

### 未验证（本包不涉及）

- **浏览器 E2E**：`clients/app` 尚未落地，Main E2E 在 plan 中为 `not-applicable`；本包不含任何浏览器用例。
- **产生侧行为**：`file.changed` 的实际派生（Diff 唯一性、行级 Myers diff、规范化前缀判定）属 WP3/WP4，本包只钉 view 形状；`agent.connected`/`disconnected` 的落库与投递属 WP3/WP4。
- **`session.read` 服务端取页实现**：属 WP3；本包只钉客户端可观察的页内/页间性质与 wire 形状。
- **deps / advisories / secrets 三个 CI-only job**：本地无等价物，未执行亦未声称通过。

### 待补项

- **TP1 review（tasks 4.6 / 3.18）**：本包作者不判独立 review，须由非作者 reviewer 检视固定提交 `33040d7`。
- **`session_sequence` 与 `sessionId` 的其它节点级事件**：本包只覆盖 `agent.connected`/`agent.disconnected` 两条既有夹具；若后续新增节点级事件类型，需同步扩展 `node_level_agent_events_carry_no_session_identity` 的夹具表。
- **`error-cursor-expired-unknown-reason.json` 的 `details.reason` 词表在 wire schema 层仍无机器约束**（`common.schema.json` 的 `details` 是裸 `{"type":"object"}`，词表只在 `errors.json` registry 里）。本包用 `schemaPointer` 指向 `errorCode` 词表以证明「错误码是封闭词表」，但 **`details.reason` 的词表本身在 schema 侧不可判**——这是既有的合同缺口，本包未扩大范围去改 schema，仅在此登记交 main 裁决。

---

## checks

`handoff_index` 的 CHECK 行（`PV1`）与本节同源；`log_path` 已复制到变更目录内以便登记。

```yaml
checks:
  - id: PV1
    work_package: TP1
    command: "CARGO_TARGET_DIR='D:\\Project\\acp-remote\\.target-wt\\tp1' npm run verify（cwd=.worktrees/tp1）"
    scope: "工作包 Project Verify：check 十道 + check:rust 三条"
    exit_code: 0
    log_path: "reports/PV1-tp1.log"
    result: "PASS：check 十道全绿（schemas 149/51、assets 213 fixture files、agentic 21 specs）+ check:rust 三条全绿（fmt/clippy/test）"
  - id: BT1
    work_package: TP1
    command: "cargo test --locked -p sync-protocol --all-features（cwd=.worktrees/tp1）"
    scope: "本包向量与全部 sync-protocol 测试"
    exit_code: 0
    log_path: "reports/PV1-tp1.log"
    result: "PASS：snapshot_scope_vectors 8/8、contract_vectors_r1_r9 17/17、schema_drift 6/6、envelope_fixtures 7/7、view_projections 3/3，其余 suite 全 ok"
  - id: BT2
    work_package: TP1
    command: "node scripts/check-schema-fixtures.mjs（cwd=.worktrees/tp1）"
    scope: "23 条新 fixture 的 ajv 层校验与 expectedKeyword"
    exit_code: 0
    log_path: "reports/PV1-tp1.log"
    result: "PASS：149 valid, 51 invalid (ajv Draft 2020-12), 41 event views bound"
```

| Check ID | 命令 | 范围 | 退出码 | 结果 | 日志 |
| --- | --- | --- | --- | --- | --- |
| `PV1` | `CARGO_TARGET_DIR='D:\Project\acp-remote\.target-wt\tp1' npm run verify`（cwd=.worktrees/tp1） | 工作包 Project Verify：`check` 十道 + `check:rust` 三条 | **0** | 全部 PASS | `reports/PV1-tp1.log`（`.target-wt/tp1/logs/PV1-verify.log` 的副本，`sha256:468ecae…`） |
| `BT1` | `cargo test --locked -p sync-protocol --all-features`（cwd=.worktrees/tp1） | 本包向量与全部 sync-protocol 测试 | **0** | snapshot_scope_vectors 8/8、contract_vectors_r1_r9 17/17、schema_drift 6/6、envelope_fixtures 7/7、view_projections 3/3、其余 5 个 suite 全 ok | 同 PV1 日志（第 1600–1700 行段） |
| `BT2` | `node scripts/check-schema-fixtures.mjs`（cwd=.worktrees/tp1） | 23 条新 fixture 的 ajv 层校验与 `expectedKeyword` | **0** | `149 valid, 51 invalid, 41 event views bound` | 同 PV1 日志第 13 行 |

### 变异检验（每条新向量都因它声明的原因而红）

| 变异 | 预期与实测 |
| --- | --- |
| `sync.schema.json#/$defs/snapshotResource` 加回 `turns` | `invalid/snapshot-chunk-detail-resource-turns.json` 变为「invalid fixture accepted」，`check:schemas` exit 1；`snapshot_scope_vectors.rs` 对应断言同时红 |
| `event-views.schema.json#/$defs/agent.connected.state` 加回自由文本分支 | 三条 `state` 负例变为 accepted，`expectedKeyword:"enum"` 不符 → exit 1；`views.rs` 若不同步，`views::project` 与 `ALL` 断言红 |
| `file.changed` 的 `addedLines` 允许 `null` | `view-file-changed-added-lines-null.json` 变为 accepted → exit 1 |
| 把 `sessionReadBefore` 的 `required` 缩为单键 | `command-session-read-before-missing-message-id.json` / `-single-field-before-created-at.json` 变为 accepted → exit 1；Rust 侧 `single_field_cursor_is_rejected` 红 |
| `VIEW_ENUMS` 新增一条却不同步 Rust 镜像 | `view_enums_match_schema` 的 `unmirrored` 断言直接失败（F3 的正面判据） |

> 说明：变异检验中的「加回/放宽」是**推理推演 + 与既有 BT4 一致的判据结构**；本轮实际执行的是上述门禁的**零退出码基线**，未改写 schema 或实现文件（`git diff ad9ad3c -- crates/sync-protocol/src schemas/` 为空，可核对）。两处**已实测**的负例自证：`check-schema-fixtures.mjs` 对 9 条片段负例报出的失败关键字与我登记的 `expectedKeyword` 逐条一致（首轮曾因指向错误 schema 报 `required`，修正指向后全部转为 `enum`/`type`/`pattern`）。

---


---

## evidence_paths

- 交付 worktree：`.worktrees/tp1`（分支 `feat/tp1-contract-vectors`，HEAD `33040d78324ff51be49219fcb3aac054d0100cc9`）
- 本报告：`openspec/changes/sync-scope-and-pwa-client/reports/deliver-tp1-r1.md`
- PV1 日志：`/d/Project/acp-remote/.target-wt/tp1/logs/PV1-verify.log`（`sha256:468ecae4349b1603450a22caec6aba1c3caded7d809a048dec1713b1bdd80c54`）
- `CARGO_TARGET_DIR`：`D:\Project\acp-remote\.target-wt\tp1`（与主 checkout 隔离）
- 重建前的保全快照：`/tmp/tp1/tp1-uncommitted.patch`、`/tmp/tp1/tp1-untracked.txt`

## resource_cleanup

- 未启动任何长驻进程；`CARGO_TARGET_DIR=.target-wt/tp1` 为 provisioner 分配、本包未在退出前删除（供 reviewer/merger 复核）。
- `.worktrees/tp1` 内的 `node_modules` junction（指向主 checkout，gitignored）保留以便后续复核者跑 `npm run check`。
- 未改动 `.worktrees/` 下其他包、主 checkout、`schemas/`、`docs/`、`openspec/changes/**` 的受版本控制文件（本报告除外）。未执行 push / merge / archive。

### handoff_index

```agentic-handoff
version: 1
agent_context:
  agent_id: "testing-1-r1"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "4.4"
    work_package: TP1
    role: tester
    phase: design
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "33040d78324ff51be49219fcb3aac054d0100cc9"
    evidence_type: DESIGN
    evidence_id: tp1-design-r1
    report_path: "reports/deliver-tp1-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "TP1 的 R1–R9 机器可判定部分的需求映射、稳定用例 ID 与场景/步骤/断言，见本报告 A 节；同批交付的可运行用例与固定向量已在本提交中。"
    source_evidence: NOT_APPLICABLE
  - task_id: "4.5"
    work_package: TP1
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "33040d78324ff51be49219fcb3aac054d0100cc9"
    evidence_type: DELIVERY
    evidence_id: tp1-delivery-r1
    report_path: "reports/deliver-tp1-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "固定提交 33040d7 = MU1a 合入提交 ad9ad3c + 39 个文件的测试/向量增量（23 条新 fixture、新增 contract_vectors_r1_r9.rs、schema_drift 枚举断言补齐、两个计数常量复算）；对 crates/sync-protocol/src/ 与 schemas/ 零改动。DELIVERY 声明「可运行用例与固定向量齐备」，PV1 已在同一提交上 exit 0。"
    source_evidence: NOT_APPLICABLE
  - task_id: "4.5"
    work_package: TP1
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "33040d78324ff51be49219fcb3aac054d0100cc9"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/deliver-tp1-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "npm run verify 在交付提交上 exit 0：check 十道全绿（check:schemas 149/51、check:assets 213 fixture files、check:agentic 21 specs）与 check:rust 三条全绿（fmt/clippy/test）。日志 .target-wt/tp1/logs/PV1-verify.log。"
    source_evidence: NOT_APPLICABLE
  - task_id: "4.5"
    work_package: TP1
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "33040d78324ff51be49219fcb3aac054d0100cc9"
    evidence_type: CHECK
    evidence_id: BT1
    report_path: "reports/deliver-tp1-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo test --locked -p sync-protocol --all-features 在交付提交 33040d7 上 exit 0：snapshot_scope_vectors 8/8、contract_vectors_r1_r9 17/17、schema_drift 6/6、envelope_fixtures 7/7、view_projections 3/3。"
    source_evidence: reports/PV1-tp1.log
  - task_id: "4.5"
    work_package: TP1
    role: tester
    phase: design-author
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "33040d78324ff51be49219fcb3aac054d0100cc9"
    evidence_type: CHECK
    evidence_id: BT2
    report_path: "reports/deliver-tp1-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "node scripts/check-schema-fixtures.mjs 在交付提交 33040d7 上 exit 0：149 valid, 51 invalid (ajv Draft 2020-12), 41 event views bound。"
    source_evidence: reports/PV1-tp1.log
checks:
  - id: PV1
    work_package: TP1
    command: "npm run verify (cwd=.worktrees/tp1, CARGO_TARGET_DIR=.target-wt/tp1)"
    exit_code: 0
    log_path: reports/PV1-tp1.log
    result: PASS
  - id: BT1
    work_package: TP1
    command: "cargo test --locked -p sync-protocol --all-features (cwd=.worktrees/tp1)"
    exit_code: 0
    log_path: reports/PV1-tp1.log
    result: PASS
  - id: BT2
    work_package: TP1
    command: "node scripts/check-schema-fixtures.mjs (cwd=.worktrees/tp1)"
    exit_code: 0
    log_path: reports/PV1-tp1.log
    result: PASS
test_delivery:
  TP1:
    kind: automated
    artifacts:
      - reports/PV1-tp1.log
    basic_checks:
      - BT1
      - BT2
      - PV1

```

---

## 交接要点（给 main）

1. **交付提交**：`33040d78324ff51be49219fcb3aac054d0100cc9`（分支 `feat/tp1-contract-vectors`，父 `ad9ad3c`）。MU1b 候选可直接以它构造。
2. **`envelope_fixtures.rs` 的共享写点已完成**：最终计数 **89 / 2 / 21 / 5**，另新增 `EXPECTED_TARGETED_PAYLOADS = 9`；`view_projections.rs` 的 `EXPECTED_VIEW_CASES` 由 36 → **38**（此常量在 plan 的 Shared File Ownership 未登记，但同一测试文件的既有断言在新增视图夹具后必然失败，故一并更新）。
3. **F3 / MU2A-R1-F1 已关闭**：`schema_drift.rs` 的第三方向改为 `VIEW_ENUMS` 驱动并双向断言，新增封闭 enum 自动覆盖。
4. **待 main 裁决的既有缺口**：`error-cursor-expired-unknown-reason.json` 揭示 `sync.cursor_invalid` 的 `details.reason` 词表在 **wire schema 层不可判**（`common.schema.json` 的 `details` 是裸 object，词表只在 registry）。本包未改 schema，仅按 `schemaPointer` 指向 `errorCode` 证明错误码封闭性，并在 E 节登记。
5. **review 归属**：tasks 4.6 / 3.18 需要非作者 reviewer 检视固定提交 `33040d7`。

