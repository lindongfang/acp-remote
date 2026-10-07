```agentic-handoff
version: 1
task_id: "4.6"
role: reviewer
phase: test-case
agent_context:
  agent_id: "review-tp1-r1"
  isolation: "fork_turns=none（新建独立 reviewer，未参与 TP1 的用例编写、实现或测试运行，也未继承作者实例 `TesterTp1` 的对话；仅接收调度方传入的角色契约、检视范围与固定版本 SHA）。本轮为只读检视。"
target_revision: "33040d78324ff51be49219fcb3aac054d0100cc9"
scope: "TP1（MU1b 唯一成员）的全部新增测试资产：23 个新 fixture、manifest.json 追加、contract_vectors_r1_r9.rs（17 断言）、snapshot_scope_vectors.rs（8 断言）、schema_drift.rs 的枚举门禁改写、envelope_fixtures.rs 的计数复算、view_projections.rs 与 tests/support/mod.rs 的配套改动。"
changes: "只读检视，未修改任何文件；仅新增本报告。未切换分支、未提交、未合并、未运行构建或测试；仅在只读 worktree `.worktrees/tp1`（HEAD=33040d7，工作树干净）与主 checkout 上执行 git/node 只读查询。"
checks:
  - id: "PV1"
    scope: "工作包 Project Verify（check 十道 + check:rust 三条）"
    command: "npm run verify（主 Agent 复跑，证据 .target-wt/tp1-main-verify.log；本 reviewer 只读消费）"
    result: "PASS（末行 TP1 PV1 exit=0；check:schemas OK、sync-protocol 各 suite 全 ok、contract_vectors_r1_r9 17/17、snapshot_scope_vectors 8/8、schema_drift 6/6、envelope_fixtures 7/7、view_projections 3/3）"
  - id: "R-COUNT"
    scope: "manifest 分类计数独立复算（89/2/21/5/9 与 EXPECTED_VIEW_CASES=38）"
    command: "node 解析 fixtures/sync/v1/manifest.json 分类计数（只读）"
    result: "PASS——与常量逐一相等（见 B 节）"
  - id: "R-SCOPE"
    scope: "ad9ad3c..33040d7 写入范围与共享写点"
    command: "git diff --name-status / --name-only（只读）"
    result: "PASS——5 M + 34 A，全部落在 crates/*/tests/ 与 fixtures/sync/v1/（见 C 节）"
issues: "0 CRITICAL / 0 MAJOR / 3 MINOR / 4 SUGGESTION"
result: "PASS"
evidence_paths:
  - "openspec/changes/sync-scope-and-pwa-client/reports/review-tp1-r1.md"
  - ".target-wt/tp1-main-verify.log"
  - "openspec/changes/sync-scope-and-pwa-client/reports/deliver-tp1-r1.md"
```

# TP1 独立检视（test-case 类型，Round 1）

## 1. Review Context

| 项 | 取值 |
| --- | --- |
| Review ID / Round | `review-tp1-r1` / 1 |
| Review Type / Stage | `test-case` / 工作包编写完成、MU1b 合入前的交付前检视 |
| Work Package | TP1（MU1b 唯一成员） |
| Repository | `D:\Project\acp-remote` |
| Base Revision | `ad9ad3c6b532ce0cc9366c8cc34a217f9f466933`（MU1a 合入提交） |
| Target Revision | `33040d78324ff51be49219fcb3aac054d0100cc9`（分支 `feat/tp1-contract-vectors`） |
| 检视工作区 | `.worktrees/tp1`（只读；HEAD=33040d7，`git status --porcelain` 为空） |

**读取的规则与需求**：`AGENTS.md`（§3 不变量、§9 测试要求）、`roles/reviewer.md` + `roles/_shared/role-report.md`、`specs/sync-snapshot-scope/spec.md`（R1–R4 全文 13 场景）、`specs/core-derived-events/spec.md`（R5–R9）、`specs/local-agent-host/spec.md`、`specs/workspace-resolution/spec.md`、`proposal.md`/`design.md`/`plan.md`（Work Packages 表 TP1 行、共享写点表、PV1）、`docs/SYNC_PROTOCOL.md`/`docs/NODE_LINK_PROTOCOL.md`（按需）。

**实际检查范围**：base→target 完整 diff（39 files / +2235 / −39）逐行阅读；两个新测试文件全文；`schema_drift.rs`/`envelope_fixtures.rs`/`view_projections.rs`/`support/mod.rs` 的改动与其消费侧分派点；23 条新 fixture 与 `manifest.json` 的机器复算；`schemas/sync/v1/{event-views,sync,command,common}.schema.json` 与 `compatibility/errors/v1/errors.json` 的对照；`scripts/check-schema-fixtures.mjs` 的 `schemaPointer`/`expectedKeyword` 消费路径。

**验证证据与限制**：PV1 由主 Agent 复跑（`.target-wt/tp1-main-verify.log`），本 reviewer **只读消费、未复跑**。作者自评（`deliver-tp1-r1.md`）不作为检视证据，仅用于核对声明与实测是否一致。所有计数结论均由本 reviewer 独立用 node 对 `manifest.json` 复算。

---

## 2. test-case 判据表逐项结论

| 判据 | 结论 | 依据 |
| --- | --- | --- |
| ID 稳定唯一 | 通过 | 新文件 `contract_vectors_r1_r9.rs`（17 个 `#[test]` 名）与 `snapshot_scope_vectors.rs`（8 个）无重名、无与既有 suite 冲突；用例标识由 Rust 测试函数名承载，不参与分片/阶段重编号。fixture 文件名沿用既有 `valid/`、`invalid/` 目录与既有前缀分组，无碰撞。 |
| 需求映射 | 通过（有 1 项 MINOR） | 新用例均以 `R<n>` 前缀注释/Section 标题归属需求；R1–R9 每条至少一条断言。R9（标题单向）无独立 wire 形状可钉，作者已在报告 A.1 明示「由 WP2 文档侧承接，本包只覆盖 _VIEW_ENUMS_ 完整性」——不构成漏写用例（见 F1）。 |
| 入口真实 | 通过（不适用反例） | 本包被测对象是 schema/fixture 与 Rust 契约镜像本身，入口是 `cargo test -p sync-protocol` 与 `npm run check` 的 schema 门禁；不存在「用 mock 绕过运行期入口」的问题。**未发现绕过**：所有断言都经真实 `Envelope::decode`/类型化 `decode`/`views::project`/ajv（`check:schemas`）读取磁盘上的真实 fixture 与真实 schema。 |
| 前置可满足 | 通过 | 向量引用的初始状态（快照 begin/chunk/end、分页两页、5 条负例）全部是本包新增或 base 已有的磁盘 fixture；无外部服务、无运行期状态依赖。资源表已登记 `node_modules/`（只读）与 `CARGO_TARGET_DIR=.target-wt/tp1`。 |
| 断言可观察 | 通过 | 断言绑定返回值/类型化结果/schema 机器资产/文件原文，未绑定内部实现；负例同时校验 `manifest.valid==false`、`expectedKeyword` 与 Rust 层 `is_err()`，并有「正例仍被接受」的防空转对照。 |
| 正常 + 异常 | 通过（核心项） | 见第 3 节变量核对：R2/R3/R6/R7/R8 的边界与负例均已落到可判定断言。 |
| 无跳过 | 通过 | `contract_vectors_r1_r9.rs`/`snapshot_scope_vectors.rs`/`schema_drift.rs`/`envelope_fixtures.rs` 无 `#[ignore]`/`todo!`/`unimplemented!`/`.skip`；`envelope_fixtures.rs` 新增的第三个计数组是**显式计数并带守卫**（非 WSS 且非 pairing 必须携带 `schemaPointer`，否则 panic），非静默跳过；无「仅截图」式断言。 |
| 资源登记 | 通过 | 所需资源（根 `node_modules/`、`CARGO_TARGET_DIR`）已在 plan 的 Runtime Resources 登记。 |
| 基础检查 | 通过（不宣称已执行） | 语法/导入/用例发现由编译保证；PV1 复跑证据显示 89/23 分类、各 suite 被发现并 ok。静态审查不宣称运行通过，实际执行证据见 `.target-wt/tp1-main-verify.log`。 |

---

## 3. 作者声称的边界是否真的成立（逐项核对）

- **R3 单字段两个方向被拒** — **成立**。`both_single_field_cursor_directions_are_rejected` 遍历 `single-field-before-created-at`（本包新增）、`before-missing-message-id`、`single-field-before`（WP1）三条，断言解码 `is_err()`、`before` 恰带 1 个分量、`expectedKeyword=="required"`。`snapshot_scope_vectors::single_field_cursor_is_rejected` 另有「两键齐全必须被接受」的对照，排除「整段 payload 被拒」的假阳性。
- **R3 `offset` 游标被拒** — **成立**。`offset_style_cursor_is_rejected` 断言 `additionalProperties` 并校验 `before` 确带 `offset` 且两个排序分量都不存在（排除混淆失败原因）。
- **R3 同 `createdAt` 不重不漏 / tie 顺序 ≠ 字典序** — **成立**。`tied_timestamp_pages_join_without_gaps_or_overlap` 断言两页并集无重复、第二页整体严格更早；`tie_broken_messages_are_ordered_stably_and_not_by_identifier` 断言页内升序且 `tied != sorted(tied)`。实测两条 `messageId`（`f0d27b43…` / `0a1b2c3d…`）的产生顺序确实非字典序，`assert_ne!` 不会因数据设计巧合而空转。
- **R2 `hasEarlier` 缺字段被拒** — **成立**。`session_read_result_declares_whether_earlier_content_exists` 断言缺键解码 `is_err()`，并校验原文 `result` 确无该键、且 `messages` 非空（排除「被拒因空页」）。
- **R2 `limit` 无线上界** — **成立**。`session_read_limit_has_no_wire_upper_bound` 直接断言 `command.schema.json#/$defs/sessionReadLimit` 为 `integer`、`minimum==1`、无 `maximum`。
- **R2 保留窗口 `cursor_expired`** — **成立**。`retention_window_error_is_explicit_and_typed` 断言 `code==SyncCursorInvalid`、`retryable==true`、`details.reason=="cursor_expired"`、消息含「保留」；`cursor_invalid_details_reason_is_a_closed_vocabulary` 断言 registry 词表恰为四值且词表外取值命中 `enum`。
- **R6/R7 可选字段缺失合法** — **成立**。`file_changed_optional_fields_may_be_absent` 断言三字段 `is_none()`，并**直接读原文**证明三键不存在（非「带 null 后解析成 None」），且四基字段齐全。
- **R6/R7 显式 `null` 被拒 / 错误形状被拒** — **成立**。5 条负例经 `views::project` 解码 `is_err()`、`expectedKeyword ∈ {type,pattern}`、基字段保留。schema 无 `null` 分支，故拒绝是 schema 语义。
- **R8 两个枚举取值互不相同 / 非法取值被拒 / 节点级 `sessionId`/`sessionSequence` 为 `null`** — **成立**。`agent_state_enums_are_unique_and_mutually_distinct` 断言 `["connected"]`、`["disconnected"]` 且 `assert_ne!`；`agent_state_rejects_values_outside_its_enum` 断言 3 条负例投影 `Err` + `expectedKeyword=="enum"` + **另一事件合法取值仍被接受**（防空转）；`node_level_agent_events_carry_no_session_identity` 断言两键存在且值为 `null`。

---

## 4. A/B/C/D 四点独立结论

### A. F3 / MU2A-R1-F1 是否真的关闭 —— **已关闭（门禁方向真实、双向、更强），但有 1 项 MINOR + 1 项 SUGGESTION**

- **覆盖性**：`schema_drift.rs::view_enums_match_schema` 第三个方向改为 `mirrors` 表（6 条）驱动，**显式包含** `AgentConnectedState`/`AgentDisconnectedState`（`schema_drift.rs:334-341`），且对每个条目断言 `mirror() == 登记序列`。两个新枚举被真实覆盖，F3/MU2A-R1-F1 的缺口**关闭**。
- **是否更强**：是。`unmirrored` 断言要求 `VIEW_ENUMS` 的每一条都在镜像表里有对照；新增封闭 enum 若只登记 `VIEW_ENUMS` 而不加镜像，门禁立即失败。作者声明的「自动覆盖」成立。
- **能否发现真实漂移（双向判据）**：**是**，且不止一条判据。方向 1（登记→schema 逐条同序）、方向 2（schema 的每个 `enum` 必须被登记）、方向 3（镜像 `ALL/as_str` == 登记序列）、方向 4（`VIEW_ENUMS` 无未对照条目）。**不存在「两侧同时为空」漏洞**：方向 2 用 `collect_enum_pointers` 独立遍历 schema，schema 一旦删光某 `$defs` 的全部 enum，方向 3 会因 `lookup_pointer` panic 而失败（该 `$defs` 仍在 `VIEW_ENUMS` 里）。
- **是否削弱严格性（枚举被整体删除）**：**未削弱原有覆盖**（原硬编码的四条仍在 `mirrors` 内），新增两条。唯一的**新暴露弱点**见 F2（`collect_enum_pointers` 不跟随 `$ref`），但该弱点在**改写前后同样存在**，不是本 diff 引入。

### B. 计数常量复算 —— **可信；`EXPECTED_TARGETED_PAYLOADS` 未掩盖问题**

按 `manifest.json` 实际条目独立复算（126 条）：

| 分类 | 实测 | 常量 | 一致 |
| --- | --- | --- | --- |
| `valid:true` + message schema | 89 | `EXPECTED_VALID_MESSAGE_CASES=89` | ✓ |
| `invalid` + message schema | 23 | — | — |
| ├ 信封层拒绝 | 2（`ping-without-connection`、`sequence-is-number`） | `EXPECTED_ENVELOPE_REJECTED=2` | ✓ |
| └ body 层拒绝 | 21 | `EXPECTED_BODY_REJECTED=21` | ✓ |
| pairing | 5 | `EXPECTED_SKIPPED_PAIRING=5` | ✓ |
| `schemaPointer` 片段 | 9（8 event-views + 1 common） | `EXPECTED_TARGETED_PAYLOADS=9` | ✓ |
| 带 `viewDef` | 38（34 distinct） | `EXPECTED_VIEW_CASES=38` | ✓ |

- **89/2/21/5/9 与 38 与真实条目数逐一相等**（本 reviewer 独立复算，非采信作者）。差值也对得上：89 = base 73 + 16；21 = base 14 + 7（3 detail-resource + offset-cursor + missing-message-id + without-has-earlier + single-field-created-at）。
- **新 skip 类是否过宽**：**否**。第三个分支带守卫 `assert!(case.schema_pointer.is_some(), …)`——非 WSS 且非 pairing 的条目若无 `schemaPointer` 即 panic，故不可能有「静默漏算」的用例被跳过。本 reviewer 另核实：sync manifest 中**不存在**「message schema + schemaPointer」的条目（不会用片段指针绕过信封层），且**磁盘上无一 fixture 未登记**（missing/extra/dup 均为 0）。
- **guard 是否足够严**：够。它按「enum 分类」而非按「目录」判定：只要未来有人把一条**完整消息**负例也带上 `schemaPointer` 指向片段，`check-schema-fixtures.mjs` 仍会按子模式校验，而 `envelope_fixtures.rs` 会把它计入 `targeted_payloads` 而使常量失配 → 强制人工复核。

### C. 写入范围与共享写点合规 —— **越界零、共享写点合规；`view_projections.rs` / `support/mod.rs` 存在登记缺口（MINOR，建议处置）**

- **Write Scope**：diff 恰为 5 M + 34 A，路径全部落在 `crates/sync-protocol/tests/` 与 `fixtures/sync/v1/`（`fixtures/sync/v1/` 全部为 A，无删除、无改写既有条目）。**未越界**：`crates/sync-protocol/src/` 与 `schemas/` 的 diff 为空。
- **`envelope_fixtures.rs` 共享写点**：本包只改计数常量（73→89、14→21、新增 9）与配套 skip 逻辑，**未回退 WP1** 的任何改动（该文件 base→target 仅计数与 skip 段变化）。与 plan 共享表中的 Merge Order WP1→TP1 一致。
- **`manifest.json` 只增不改**：独立验证「base 的 94 条在 target 中**逐条逐字未变**、无删除、target 是 base 的超序列（subsequence）」，符合共享表「只追加并保持排序、条目不重不漏」。
- **登记缺口（F3，MINOR）**：`crates/sync-protocol/tests/view_projections.rs` 的 `EXPECTED_VIEW_CASES` 由 36→38、`tests/support/mod.rs` 新增 `schema_pointer` 字段，均在 TP1 的 glob 写范围内且与其它包无并发（TP1 是 MU1b 唯一成员；TP2 的测试在 `crates/core`/`storage-sqlite`），**不构成同批冲突或阻断**；但 plan 的 Shared File Ownership 表未单列这两处，建议登记以保持台账完整。

### D. 测试资产能否捕获回归 —— **对所列关键风险均能捕获；1 项潜在盲区以 SUGGESTION 记录**

1. **复合游标退化为单字段** — **能捕获**。`single_field_cursor_is_rejected` 与 `both_single_field_cursor_directions_are_rejected` 覆盖两个方向，且都带「两键齐全必须被接受」对照；`snapshot_scope_vectors` 与 `contract_vectors` 分别以 fixture 与内联 body 双路验证。
2. **`chunkCount` 口径漂移** — **能捕获**。`catalog_snapshot_chunk_count_matches_three_declared_resources` 断言 `begin.chunkCount == end.chunkCount == SnapshotResource::ALL.len()`；`sessions_only_snapshot_uses_single_catalog_chunk` 断言单类时 `==1`。schema 侧 `snapshotChunkCount` 的 `maximum:3` 与 `SnapshotResource::ALL.len()` 由 `snapshot_resources_match_schema_enum` 双向绑定，故枚举收窄/放宽都会红。
3. **`file.changed` 必填集合被误改** — **能捕获**。`file_changed_optional_fields_carry_their_declared_values` 直接断言 schema 的 `required == ["changeId","kind","displayPath","summary"]`；`file_changed_optional_fields_may_be_absent` 断言四基字段在原文中出现。
4. **`state` 枚举被放宽** — **能捕获**。`agent_state_enums_are_unique_and_mutually_distinct` 断言 `ALL` 恰为单值，`agent_state_rejects_values_outside_its_enum` 断言词表外取值投影 `Err` 且 `expectedKeyword=="enum"`；ajv 侧 3 条负例同时变红。
5. **节点级事件被塞入 `sessionId`** — **能捕获**。`node_level_agent_events_carry_no_session_identity` 断言两键存在且值为 `null`，投射另行验证。

**「看起来覆盖了、实际断言不到位」的排查**：本轮**未发现**此类空转。作者对每处负例都补了防空转对照（正例仍被接受、失败点自证、基字段保留、`expectedKeyword` 三重判据）。唯一潜在盲区见 F4（`file.changed` 负例集未含 `deletedLines` 的 `null`/`number` 与 `addedLines` 的格式非法），但不改变各负例本身的有效性。

---

## 5. Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| review-tp1-r1-F1 | MINOR | `crates/sync-protocol/tests/contract_vectors_r1_r9.rs:1-14`（模块文档）/ 报告 A.1 | R5/R9 无本包独立断言（R5 仅覆盖 view 字段侧、R9 由 WP2 文档侧承接）。触发：按「每个 Requirement 至少一个 ID」核对本包时成立。 | R5/R9 的行为面属 WP3/WP4/TP2 与文档侧，本包已明确划界；但用例文件头与 A.1 表未把 R5/R9 显式标为「本包无对应断言」，读者易误以为 R5–R9 全覆盖。 | 在文件头/报告映射表中把 R5、R9 显式标注为「本包不钉、归 WP3/WP4/TP2 与文档」，与 R1–R4/R6–R8 区分。 | 未复核 |
| review-tp1-r1-F2 | MINOR | `crates/sync-protocol/tests/schema_drift.rs:53-66`（`collect_enum_pointers`） | 该函数按模块注释不跟随 `$ref`。触发：若被登记的 `enum` 移入 `$ref` 目标，方向 2 不再枚举到它。实测：当前 `event-views.schema.json` 内联 enum 无 `$ref` 包裹，四个方向今日均真实生效。 | 方向 3 仍会因 `lookup_pointer` panic 而失败，故不是静默漏洞；仅「新增 `$ref` 包裹的 enum」时方向 2 可能漏检。属**改写前后同样存在**的既有弱点，非本 diff 引入。 | 可选：在 `collect_enum_pointers` 跟随 `$ref`（保持 `properties` 不进入），或加断言「`VIEW_ENUMS` 的每个 pointer 在 schema 中确为字面 `enum`」（已有 `lookup_pointer` 校验，可加注释说明该边界）。 | 未复核 |
| review-tp1-r1-F3 | MINOR | `openspec/changes/sync-scope-and-pwa-client/plan.md`（Shared File Ownership 表） | `crates/sync-protocol/tests/view_projections.rs`（`EXPECTED_VIEW_CASES` 36→38）与 `crates/sync-protocol/tests/support/mod.rs`（新增 `schema_pointer`）被本包改动但未在共享表登记。 | 两处均在 TP1 自身 glob 范围且无并发写者，不阻断；登记缺口降低台账可核对性。 | 在共享表 `crates/sync-protocol/tests/` 行补注这两处的归属与「只增不改」约束，或新增一行登记。 | 未复核 |
| review-tp1-r1-F4 | SUGGESTION | `crates/sync-protocol/tests/contract_vectors_r1_r9.rs:63-79`（`FILE_CHANGED_NEGATIVES`） | 5 条负例中 `deletedLines` 仅覆盖 `+3`（pattern），`addedLines` 仅覆盖 `null`/number 与 `"12"` 正例；`deletedLines` 的 `null`/number 与 `addedLines` 的 `+N` 未覆盖。 | 两字段同 `$ref` 同一 `decimalString`，覆盖已足够；仅对称性可更整齐。 | 可选补 1–2 条对称负例，或不改（现有覆盖已足）。 | 未复核 |
| review-tp1-r1-F5 | SUGGESTION | `crates/sync-protocol/tests/contract_vectors_r1_r9.rs:328-331` | `FileChanged` 的「格式」断言仅落在 schema 的 `pattern`（`deletedLines`），`addedLines` 无格式负例。 | 同 F4，覆盖已足。 | 无强制动作。 | 未复核 |
| review-tp1-r1-F6 | SUGGESTION | `crates/sync-protocol/tests/snapshot_scope_vectors.rs:159-176` | `snapshot_resource_enum_excludes_every_session_detail_resource` 的循环体对负例 fixture 只断言解析错误，未像 `every_forbidden_detail_resource_has_a_registered_negative_chunk_fixture` 那样同时校验 `expectedKeyword=="enum"`（后者已覆盖）。 | 两条测试存在部分重叠，非缺陷。 | 无强制动作；如需去重可保留其一。 | 未复核 |
| review-tp1-r1-F7 | SUGGESTION | `crates/sync-protocol/tests/contract_vectors_r1_r9.rs:472-500` | `retention_window_error_is_explicit_and_typed` 的「消息含『保留』」断言绑定中文文案。 | 文案变动会误红；但通道级 `code`/`reason` 已足够判定，文案断言属附加。 | 若后续文案本地化，改为断言 `code`+`reason` 即可，或保留为「提示可读性」的非契约断言。 | 未复核 |

---

## 6. 闭环复核（前一/前序发现）

| 原问题 ID | 原位置 | 本轮结论 | 复核依据 |
| --- | --- | --- | --- |
| `review-wp2-r1` F3 | `crates/sync-protocol/tests/schema_drift.rs:295-322` | **已关闭** | target 上第三个方向已改为 `mirrors` 驱动并**显式登记** `AgentConnectedState`/`AgentDisconnectedState`（:334-341），且 `unmirrored` 双向断言保证新增 enum 自动纳入；PV1 中 `view_enums_match_schema ... ok`（`.target-wt/tp1-main-verify.log:1697`）。 |
| `review-mu1a-candidate-r1` MU2A-R1-F1 | 同一位置（候选 blob 与 base 相同） | **已关闭** | 同上；缺口的成因是「断言在旧基线未提交工作区、无提交可绑定」，本包已把驱动式改写**固化进交付提交**（`git diff ad9ad3c..33040d7 -- crates/sync-protocol/tests/schema_drift.rs` 非空）。 |
| `review-wp2-r1` F5 | 两个节点级 fixture 的 null 化 | 已关闭（前序） | target 的 `view-agent-{connected,disconnected}.json` 的 `sessionId`/`sessionSequence` 均为 `null`（本包新断言 `node_level_agent_events_carry_no_session_identity` 覆盖）。 |

---

## 7. Assessment

### 本轮结论：**PASS**（Target Revision `33040d78324ff51be49219fcb3aac054d0100cc9`）

- test-case 判据表九项**全部通过**（ID 稳定唯一、需求映射、入口真实、前置可满足、断言可观察、正常+异常、无跳过、资源登记、基础检查）。
- **A（F3/MU2A-R1-F1）**：门禁改写**真实覆盖**两个新枚举、**双向且更强**、仍能发现真实漂移，未削弱原有严格性；缺口**已关闭**。
- **B（计数）**：89/2/21/5/9 与 `EXPECTED_VIEW_CASES=38` 经独立复算**逐一比对相等**；新 skip 类**带严格守卫**，未掩盖问题。
- **C（写入范围）**：diff 全部落在 `crates/*/tests/` 与 `fixtures/sync/v1/`，**零越界**；`envelope_fixtures.rs` 共享写点未回退 WP1；`manifest.json` 经子序列验证为纯追加、既有条目逐字未变。`view_projections.rs`/`support/mod.rs` 的登记缺口记 MINOR（F3），不阻断。
- **D（回归捕获）**：五条关键风险**均可捕获**，负例均有防空转对照；未发现「看似覆盖、断言不到位」。
- **未解决的 CRITICAL/MAJOR：无**。3 项 MINOR + 4 项 SUGGESTION 均为非阻断，建议交测试 Agent 选择性处置。

### 证据状态

| 检查 | 状态 | 结论 | 是否影响本轮判断 |
| --- | --- | --- | --- |
| PV1（`.target-wt/tp1-main-verify.log`，主 Agent 复跑） | 已只读消费，末行 `TP1 PV1 exit=0` | `check` 十道 + `check:rust` 三条全绿；`contract_vectors_r1_r9` 17/17、`snapshot_scope_vectors` 8/8、`schema_drift` 6/6、`envelope_fixtures` 7/7、`view_projections` 3/3 | **是**（作为 PV1 证据采信；reviewer 未复跑，符合只读边界） |
| manifest 分类计数独立复算 | 已执行（只读 node 脚本） | 89/2/21/5/9 与 38 与常量逐一相等 | 是 |
| manifest 追加性/子序列核验 | 已执行（只读 node 脚本） | base 94 条逐字未变、无删除、target 为超序列 | 是 |
| 写入范围核验 | 已执行（`git diff --name-status`） | 5 M + 34 A，无越界 | 是 |
| 作品 deps/advisories/secrets 三个 CI-only job | 待补（本地无等价物） | 未执行，不作为本包证据 | 否 |
| IV1 / AC1–AC3 / E2E | 待补（MU2/MU3 及主分支阶段） | 本包无 E2E；产生侧行为属 WP3/WP4/TP2 | 否 |

### 实际检查范围

- 固定版本全 diff：`git diff ad9ad3c..33040d7`（39 文件 / +2235 / −39）逐行阅读，含两个新测试文件全文、`schema_drift.rs` 全部四方向、`envelope_fixtures.rs` 的 skip/计数逻辑、`support/mod.rs` 与 `view_projections.rs` 的改动。
- 机器资产对照：`schemas/sync/v1/{event-views,sync,command,common}.schema.json`、`compatibility/errors/v1/errors.json`、`scripts/check-schema-fixtures.mjs` 的 `schemaPointer`/`expectedKeyword` 消费路径。
- fixture 全量：23 条新增 fixture 逐条阅读，126 条 manifest 条目分类计数。
- 需求/规划对照：`specs/{sync-snapshot-scope,core-derived-events,local-agent-host,workspace-resolution}`、`plan.md`（Coverage Index、Work Packages、共享写点、PV1）、`AGENTS.md`。
- 前序复核：`review-wp2-r1` F3/F5、`review-mu1a-candidate-r1` MU2A-R1-F1。

### 未验证内容

- **未执行任何构建/测试**（只读边界）；PV1 结论引用主 Agent 复跑日志，未独立复跑。
- **未做变异检验实测**：作者报告 D 节的变异推演未由本人复跑（其自评不作为证据）；本报告的回归捕获判断基于代码/机器资产的静态判据。
- **产生侧行为**（Diff 派生唯一性、行级 Myers diff 口径、规范化前缀判定的 `../` 边界、节点级事件落库与投递、标题通知映射）属 WP3/WP4/TP2，本包不判。
- **浏览器 E2E**：本次变更 main E2E 为 `not-applicable`，本包不含浏览器用例。
- **`sync.cursor_invalid.details.reason` 的 wire schema 层词表**：`common.schema.json` 的 `details` 为裸 object，词表仅在 registry；作者已在报告 E 节登记该既有缺口，本 reviewer 确认该缺口**存在且非本包引入**，是否扩大范围改 schema 由主 Agent 裁决。
