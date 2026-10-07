<!-- WP3 独立代码检视报告（交付提交轮，Round 1）。Reviewer 只报告，不修改代码、测试、规划文件、任务状态或 verification.md。 -->

task_id: "3.6"
role: reviewer
phase: branch
agent_context:
  agent_id: "review-w3-r1"
  isolation: "fork_turns=none（新建独立子 Agent，未参与 WP3 的实现或修复对话，不继承任何实现上下文；WP4 的并行检视 review-w4-r1 与本案无关）"
target_revision: "0f70c4c2ddc41d21ba86abb0c6e83ef3da11ddc1"
scope: "WP3（MU2 单元成员，R5–R9 实现侧）的交付提交检视。Base 33040d78324ff51be49219fcb3aac054d0100cc9 -> Target 0f70c4c2ddc41d21ba86abb0c6e83ef3da11ddc1，15 files / +1644 / -19。tasks 3.6 的四个必查重点（行级 diff 边界输入、路径前缀判定、节点级事件三条 DDL CHECK 与无新增 migration、ACP 原文三要素未变），外加要求符合性（R5–R9）、依赖边界、越区改动、check:drift/文档同步、作者开放问题归属。"
changes: "只读检视，未修改任何代码、测试、规划文件、任务状态或 verification.md；未切换分支、未提交、未合并、未运行任何编译/测试/E2E；仅新增本报告。"
issues: "0 CRITICAL / 1 MAJOR / 1 MINOR（等价 P2）/ 2 MINOR（P3）"
result: FAIL

## Review Context

| 项 | 取值 |
| --- | --- |
| Review ID / Round | `review-w3-r1` / 1（交付提交轮；本线程首轮） |
| Review Type / Stage | branch（工作包交付前检视：PV1/PV2 已 PASS，合入 MU2 候选之前） |
| Work Package | WP3（MU2 成员，R5–R9 实现侧） |
| Repository / Worktree | `D:\Project\acp-remote`，固定检视 worktree `.worktrees\wp3`（HEAD `0f70c4c`） |
| Base Revision | `33040d78324ff51be49219fcb3aac054d0100cc9`（target 的父提交） |
| Target Revision | `0f70c4c2ddc41d21ba86abb0c6e83ef3da11ddc1` |
| 实际修改文件 | `crates/core/src/{derive.rs(新,866 行),broker.rs,lib.rs,model/json.rs,model/mod.rs,ports.rs}`、`crates/storage-sqlite/src/session_store.rs`、`crates/storage-sqlite/tests/`（7 文件 19 处 `title: None,`）、`docs/CORE_PORTS_AND_STORAGE.md`；`crates/core/Cargo.toml`、`crates/sync-protocol/**`、`schemas/**`、`fixtures/**`、`crates/storage-sqlite/src/migrate.rs` 零改动（`git diff --stat` 与逐文件 `git diff` 核对） |
| 读取的规则与需求 | `AGENTS.md` §3/§4/§12；`specs/core-derived-events/spec.md`（R5–R9）、`specs/workspace-resolution/spec.md`（R7）、`specs/local-agent-host/spec.md`；`proposal.md`、`design.md`（D4/D5/D6/D7）、`plan.md`（WP3 行 :861、Shared File Ownership :908-931、PV2 :993）；`docs/SYNC_PROTOCOL.md` §9.6/§10.3/§11.5、`docs/CORE_PORTS_AND_STORAGE.md` §3.6/§5/§6/§7、`docs/MODULE_ARCHITECTURE.md` §4.1/§5 |
| 验证证据（只读消费） | `reports/PV1-wp3-verify.log`、`reports/PV1-wp3-rust.log`、`reports/PV1-wp3-agentic.log`、`reports/PV2-wp3.log`（作者报 exit 0）、`reports/deliver-wp3-r1.md`、`verification.md` 的规划缺口登记节、`reports/review-w4-r1.md`（跨包接缝）。**作者自评不作为检视证据**，Reviewer 未执行任何编译/测试/E2E（只读边界）。 |
| 限制 | 不判「用例是否充分」（交 validator / Coverage Index）；不重复跑 Project Verify；不执行 E2E；WP4 的交付不读作本包证据，仅在跨包接缝处读其目标内容作为消费方/生产者事实核对。 |

## 结论总览

| 编号 | 严重度 | 位置 | 摘要 |
| --- | --- | --- | --- |
| `review-w3-r1-F1` | **MAJOR** | `crates/storage-sqlite/src/session_store.rs:1171-1181` | 「显式置空标题」走窄语句，丢掉 `version = version + 1` 与 `updated_at = ?3`（并静默丢弃同一提交里的其它列）：与同一分片的 `session.mode.changed`/`session.config.changed` 合批时触发 `PortError::Corrupt` 并整批丢事件；另与 design D7 的「权威更新时间取 `commit.at`」相悖，且与 core fake 端口对同一输入的版本语义不一致 |
| `review-w3-r1-F2` | MINOR（P2） | `crates/core/src/derive.rs:149-173`（`dedup_key`/`change_id`）、`crates/core/src/broker.rs:2113-2124` | 去重键用**已收窄**的展示路径：同一次工具调用里两个**不同**的越界文件若同名（`displayPath` 都退化为 basename），后者被当重复静默丢弃 |
| `review-w3-r1-F3` | MINOR（P3） | `crates/core/src/broker.rs:666-672`、`crates/core/src/derive.rs:485-491` | 空字符串 `agentId` 通过校验（报错文案声称「非空」，schema 为 `minLength: 1`），可落一条 schema 非法的节点级事件 |
| `review-w3-r1-F4` | MINOR（P3） | `docs/CORE_PORTS_AND_STORAGE.md:28`（正文缺口在 :394） | 版本记录声称「§5.2 的 `StateChange::Update` 增两层可选标题字段」，但 §5.2 正文仍写「新增三个可空列」、未提 title；且 `check:drift` 的 §5 代码块不含 `SessionUpdate`，该字段两侧一致性不进门禁 |

---

## 1. tasks 3.6 四个必查重点

### 1.1 行级 diff 的边界输入 —— **正确（未发现缺陷）**

核对对象：`derive.rs` 的 `line_stats`/`split_lines`/`myers_distance`（`line_stats` 起 `derive.rs:69`，`myers_distance` 起 `:113`）。

| 情形 | 取值路径 | 结论 |
| --- | --- | --- |
| 行数相等但内容不同 | `myers_distance` 求编辑距离 D，再 `lcs=(N+M−D)/2`、`added=M−lcs`、`deleted=N−lcs` | **成立**。既不用行数差，也不重建编辑脚本。`line_stats_counts_replacement_when_line_counts_are_equal` 断言 `(1,1)` |
| 新建文件 | `oldText` 缺席 → `(new_lines, 0)`；`oldText` 为空串也走同一分支 | 成立，与 R6 场景 2 一致 |
| 删除文件 | `oldText` 在、`newText == Some("")` → `(0, N)`（`split_lines("")` 返回 0 行，故不产生幻影新增行） | 成立，与 R6「删除文件反之」一致 |
| **判定不出时省略** | `newText` 缺席 → `stats = None` → `file_changed_view` **整键缺席**（不是 `"0"`） | **成立**。R6 场景 3 的硬要求满足，`a_diff_without_new_text_omits_the_line_counts` 断言两键不存在 |
| 病态输入超预算 | 先判 `N+M > LINE_STAT_MAX_LINES (200_000)` 再分配 `V`；步数预算 `LINE_STAT_WORK_BUDGET (8_000_000)` 用 `checked_sub(1)?` 递减，耗尽返回 `None` | **成立**，与 `design.md` D4 的风险条目与回滚条件 (b) 同口径（省略而非整体回滚） |
| Myers 正确性 | `myers_distance_matches_brute_force_on_all_tiny_pairs`（长度 ≤3、字母表 2 的**穷举**对拍 O(N·M) LCS DP）+ 400 组确定性伪随机对拍 | 成立。`V` 数组下标范围 `[1, 2·max+1]` 且访问 `index±1` 均落在 `2·max+3` 内；`d=0` 时 `k==-d` 取 `V[1]`（全零初始化）与论文一致 |

结论：**无缺陷**；未发现「填零代替未知」或「行数差代替行级差异」的实现错误。

### 1.2 路径规范化前缀判定 —— **正确（未发现缺陷）**

核对对象：`derive.rs` 的 `display_path`（`:296-344`）、`outside_display`、`relative_text`、`normalize`（`:357`）、`canonical_path`（`:384`）、`strip_verbatim`。

- **同名前缀不误判**：绝对路径分支用 `canonical_path` 规范化后 `Path::strip_prefix` 做**组件**比较，`/work/api` 与 `/work/api-tools/file.txt` 判为越界（`display_path_does_not_treat_a_string_prefix_as_inside` 断言）。**成立**。
- **越界只下发文件名**：`outside_display` 只取 `file_name()`，无任何目录片段、无回退层级（`..` 不会出现）；无名时回退 `.`（schema `minLength: 1` 仍满足）。**成立**。
- **工作区根未登记**：`workspace_root: None` 时绝对分支直接越界；相对分支 `map(...)` 落在 `_ =>` 也越界。**成立**（`display_path_without_a_workspace_root_is_outside`）。
- **相对形式从严**：逃出根（规范化后前导 `ParentDir`）或规范化后为空 → 越界。**成立**。
- **尚不存在的新文件**：`canonicalize` 失败时逐级上溯到第一个存在的祖先再回接剩余组件，使「已 canonical 化的根」与「新文件路径」可比较；Windows 侧剥 `\\?\`/`\\?\UNC\` verbatim 前缀。**成立**。

结论：**无缺陷**。附一条**非发现**的防御性观察（不构成缺陷、不建议在本次变更内改）：绝对分支未像相对分支那样加 `!root.as_os_str().is_empty()` 守卫，若 `workspace_cwd` 为**空串**则 `root_path` 为空、`strip_prefix` 恒成功，会把绝对路径去掉根前缀后作为「工作区内相对路径」下发。该状态在当前写者下**不可达**（`owned_session.workspace_cwd` 只由 `ResolvedWorkspace::canonical_path()` 写入，`SessionRecoveryRecord::try_new` 又要求 `is_absolute`，且 `load_recovery` 两列任一为 `NULL` 即返回 `None`），故不作为 finding。

### 1.3 节点级事件的三条 DDL CHECK 自洽 + 无新增 migration —— **正确（未发现缺陷）**

- **无新增 migration**：`git diff 33040d78 0f70c4c2 -- crates/storage-sqlite/src/migrate.rs` **零行**；`design.md` D6「DDL 已就位、不需要 migration」与 `CORE_PORTS_AND_STORAGE.md` §6 第 21 条一致。**成立**。
- **写入形状与 CHECK 一致**：`commit_node_event`（`broker.rs:640-700`）提交 `OwnedCommit { session: None, turns: [], interactions: [], compacted: [] }`；存储层 `session_id = None` 时 `(session_id, session_sequence, origin_epoch, origin_sequence) = (NULL, NULL, NULL, NULL)`（`session_store.rs:1244-1281` 的 `None => (None, None, None, None)`）。四条约束（`migrate.rs:106-108` 的 `session_id IS NOT NULL OR session_sequence IS NULL`、两条等值约束）**全部满足**，无绕过路径：`kind` 由 `commit_node_event` 强制 `EventKind::State`，`event_type` 限定在 `{agent.connected, agent.disconnected}`；`turn`/`causation` 必须为空；`payload.acp` 必须为 `None`。**成立**。
- **唯一索引不冲突**：`owned_event_session(session_id, session_sequence)` 在 SQLite 下 `(NULL, NULL)` 视为互异，多条节点级事件可共存；`owned_event_origin` 带 `WHERE session_id IS NOT NULL` 谓词，节点级事件不入该索引。**成立**。
- **消费侧（跨边界核对）**：节点级事件的落库与**全局** replay 成立（`storage-sqlite` 的 `replay` 不带会话过滤，`broker.rs` 的用例断言其入 replay 流）；会话级投递路径 `crates/server/src/node_link/resource.rs:1029` 对 `session.is_none()` **显式 return**（不归属、不伪造会话级游标），符合 R8 场景「节点级事件不被会话级投递路径误收」。**成立**。

结论：**无缺陷**。唯一边界项见 F3（`agentId` 空串），属校验强度而非 DDL 自洽。

### 1.4 ACP 原文三要素未变（且断言真的能捕获改动）—— **成立**

- 实现面上 `file.changed` 是**新增**事件，`tool.call.*` 的 `payload`（含 `acp`）原样进提交；`derive` 是纯函数、不持有也不改写 `EventPayload`（`derive.rs` 只读 `ViewJson` 文本）。`commit_owned` → 存储层 `acp_columns(pending.payload.acp.as_ref())` 按三要素原样绑定。
- 断言有效性核验：`deriving_a_file_change_leaves_the_acp_raw_document_untouched`（`broker.rs:9167-9199`）在**提交前**构造 `AcpRaw::available(media_type, raw, Digest)`，派生后从 fake 事件日志按 `event_id` 取回 `event_payloads` 中的 `payload`，断言 `payload.acp.as_ref() == Some(&acp)`。该比较是**整个 `AcpRaw` 的结构相等**（`media_type`/`raw_json`/`byte_length`/`sha256` 四个字段全部参与），且 fake 的 `event_payloads` 是在 **commit 时** 写入的 payload 克隆（`broker.rs:4833`），因此「派生路径改动 `tool.call.*` payload」会被捕获，断言**非恒真**：取回的是提交后的落盘内容，不是测试自己持有的那个值。断言中 `payload.acp` 为 `Some` 而非恒 `None`（否则 `Some(&acp)` 比较会失败），`file.changed` 计数 `assert_eq!(..., 1)` 与前置条件成立。**可以捕获改动**。
- 其映射到「摘要与字节长度」的语义也成立：`AcpRaw::available` 由 `raw_json.len()` 导出 `byte_length`（`model/event.rs:92-104`），故结构相等即字节长度相等；`sha256` 是独立字段，被同一比较覆盖。

结论：**成立**，无缺陷。（该用例跑在 fake 端口上；真实 SQLite 的 ACP 列往返由既有 `event_payload` 保真用例与 `docs/CORE_PORTS_AND_STORAGE.md` §9 判据覆盖，本次未复跑。）

---

## 2. 另需核实的五点

### A. 依赖与边界合规 —— **合规**

- `crates/core/Cargo.toml` **零改动**（不在 15 文件列表内）：依赖仍是 `async-trait`/`thiserror`/`p256`/`sha2` 四项；`p256` 仍为 `default-features = false, features = ["arithmetic"]`（workspace 根 `Cargo.toml:73`，本包未改）。**成立**。
- 行级 Myers diff **自实现**：`derive.rs::myers_distance`（论文 Algorithm 1 的贪心前向搜索，线性空间），未引入任何新 crate。**成立**。
- core **未 import `sync-protocol`**：`grep -rn "sync_protocol|sync-protocol" crates/core/src/` 零命中；事件载荷走 `crates/core/src/model/json.rs` 的 `ViewJson`（本包只新增了 `json_object_members`（同口径包装）与 `array_items`（元素原文切片）两个 crate 内读取面无新类型）。**成立**（`check:boundaries` 报 12 crate 与 §5 矩阵一致，与 `CORE_FORBIDDEN` 不冲突）。
- 跨边界消费侧核对：`file.changed` 的 view 键（`changeId`/`kind`/`displayPath`/`summary`/`addedLines`/`deletedLines`/`outsideWorkspace`）与 `schemas/sync/v1/event-views.schema.json#/$defs/file.changed` 逐字段一致（`decimalString` 用字符串写、`outsideWorkspace` 用布尔、省略键而非 `null`），消费侧 `crates/sync-protocol/src/views.rs:948` 的 `FileChanged` 分支注册齐备；`agent.connected`/`agent.disconnected` 同理（`views.rs:937/938`），`state` 取值与 `VIEW_ENUMS` 登记一致。**无静默丢弃**。

### B. R9 的标题两层可选与 ACP 原文 —— **成立**

- **两层可选真的能表达三分**：`SessionUpdate.title: Option<Option<String>>`（`ports.rs:180-190`）；`storage-sqlite`（`session_store.rs:1171-1181`）与 core fake（`broker.rs:4687-4692`）都按「`None` = 不改列 / `Some(None)` = 置空 / `Some(Some(t))` = 写入」实现。**成立**。
- **读取意图的方式正确**：公共 view 的 `title` 是 §10.3 的 **required** 字段（`string|null`，schema `required: ["title","updatedAt"]`），适配器 `json!({"title": info.title, ...})` 在 `None` 时写 `null`，因此 **view 无法区分「键缺席」与「显式 null」**；读 ACP 原文 `params.update.title` 的**键存在性**是唯一可行且正确的判据（ACP `SessionInfoUpdate` 全部字段可选；上游描述 `title` 为 "Set to null to clear"），且生产路径确实保留 ACP 原文（WP4 的 `update_events` 对 `session_info_changed` 传 `acp`）。降级路径（ACP 不可用）把 view 的 `null` 按 `Unchanged`（保守），不会把「不改」误判成「清空」。**成立**。
- **`updatedAt` 取 Daemon 持久化时间**：`commit_chunk` 只用 view 转发 Agent 自报值，权威时间取 `commit.at` → 存储层 `updated_at = ?3`（`session_store.rs:1143`）。用例断言 `updated_at ≠ 2020-01-01T00:00:00.000Z`。**成立，但仅对 SET 路径**——**CLEAR 路径不写 `updated_at`，见 F1**。
- **单向、无重命名命令**：`commands.json` 无重命名命令；`required_grant("session.rename")` 为 `None`（本包未改 `required_grant`，`check:command-catalog.mjs:257` 与 `commands.json` 双向比对仍绿）。**成立**。

### C. 越区改动是否确为纯机械加法 —— **成立**

- `git diff 33040d78 0f70c4c2 -- crates/storage-sqlite/tests/` 的新增行**只有** `title: None,`（过滤 `^+`、排除 `+++` 与 `title: None,` 后**零行**），删除行**零行**（该目录整体只有新增）。19 处 / 7 文件与作者清单逐处一致（`commit.rs` 7、`workspace_alias.rs` 4、`session_version_rule.rs` 3、`retention.rs` 2、`contract_v03.rs`/`enum_coverage.rs`/`migration.rs` 各 1），插入位置一律在 `mode:` 之后，**未改任何断言、用例名、执行路径或忽略标记**。**成立**。
- 越区判定：plan 的 WP3 Write Scope 是 `crates/core/src/`、`crates/storage-sqlite/src/`；`crates/storage-sqlite/tests/` 不在其中（Shared File Ownership 只登记 TP1/TP2，且区域收窄为「只新增文件」）。该编辑是 `SessionUpdate` 增字段后**编译强制**的连带修改，main 已授权并登记（`verification.md` 的规划缺口登记节）。**合规**（并已在 verification 登记 Merge Order 实际为 WP3 → TP2）。
- **其它越区**：`docs/CORE_PORTS_AND_STORAGE.md` 的更新属 plan「权威文档更新」行要求（与代码同提交）；除该文件外，diff 只含 `crates/core/src/`、`crates/storage-sqlite/src/`、`crates/storage-sqlite/tests/`。`crates/app/src/compose.rs`、`crates/agent-host/**`、`schemas/**`、`fixtures/**`、`crates/sync-protocol/**` 均**零改动**。**无其它越区改动**。

### D. `check:drift` 与文档同步 —— **新增类型一致；`title` 字段不进门禁（见 F4）**

- **`NodeEventSink` 双侧一致**：`docs/CORE_PORTS_AND_STORAGE.md` §5.4 的 ```rust 块新增 `#[derive(Clone)]` + `pub struct NodeEventSink(Arc<dyn Fn(EndpointEvent) + Send + Sync>);`（含 `[决定]` 说明），与 `crates/core/src/ports.rs:1116-1134` 的声明**逐字同形**；按 `scripts/check-contract-drift.mjs` 的归一化口径（去注释/去空白/删尾逗号）比较，元组结构体两侧成员集为空集，**相等**。作者报的 `contract drift OK: §7 的 36 条 DDL…§5 的 15 个 trait / 96 个方法签名` 与 `reports/PV1-wp3-verify.log:54` 一致；门禁自身有「trait ≥10 / 方法 ≥60 / 类型 ≥3」的解析健全性下限，非空转通过。**成立**。
- **`title` 字段无法由该门禁覆盖**：§5.2 的 ```rust 块只声明 `OwnedCommit`/`CommitOutcome` 与各 store trait，**不含 `SessionUpdate`/`StateChange`**（它俩只出现在散文里），因此 `check:drift` 绿**不证明** `title` 在文档与 `ports.rs` 两侧逐字一致。该覆盖缺口是既有状态（`SessionUpdate` 从来不在 §5 代码块内），但本包新增字段后：文档的版本记录（`:28`）与 §6 第 21 条写了该字段的完整语义，而 §5.2 正文的 `[决定]`（`:394`）仍写「`StateChange::Update` 新增**三个**可空列……`None` = 不改该列」，**未提 title**。作为 `Option<Option<String>>`，title 的「`Some(None)` = 显式置空」正是 `[决定]` 那句「`None` = 不改该列」无法覆盖的第三态，故该句现在**不完备**（见 F4，P3，不阻断）。

### E. 作者自报的未验证项与开放问题 —— **均为实现自由度；组合根接线不在 WP3 Write Scope**

逐条判定（判据：spec/design 是否已定；若已定而实现擅自决定 → MAJOR/MINOR）：

| # | 开放问题 | 判定 |
| --- | --- | --- |
| 1 | `file.changed` 的 `kind`/`summary` 口径 | **spec 未约束**：`spec.md` 的 R5–R7 不提 `kind` 取值集合；schema 只要求 `kind: string(1..64)`、`summary: string(≤2048)`；`SYNC_PROTOCOL.md` §10.3 同。属实现自由度（`proposal.md` decision_bounds 明列「字段命名」可自主）。**注意**（供 main 裁决，非 finding）：PWA 原型 `prototypes/acp-remote-pwa.html:1277` 用的是 `kind:"modify"`，与本实现的 `modified` 不同名——WP7 消费前宜冻结词表 |
| 2 | `changeId` 生成方式 | **spec 未约束**：只要求 `common.schema.json#/$defs/uuid` 形状。确定性 `SHA-256(会话+toolCallId+展示路径)` 前 16 字节置 v4 位，形状合法（`canonical_uuid`，用例断言长度/版本位/变体位/字符集），且去重日志保证同一 `(toolCallId, 路径)` 只派发一条，故确定性不引入重复 id。**属自由度**（但见 F2：键取自收窄后路径，会碰撞） |
| 3 | `agent.disconnected` 的可选 `error` 字段由谁构造 | **不属 WP3**：core 只做形状校验（`commit_node_event` 不禁止 `additionalProperties`），构造点在 `agent-host`（WP4）。**属跨包分工**，非 WP3 遗漏 |
| 4 | `agent_capabilities` 触发首次 spawn 是否算「进程建立」 | **不属 WP3**：core 只接收并落库事件，不判定语义；`specs/local-agent-host` 的该语义由 WP4 承担。**属 WP4/AC1** |
| 5 | `workspace_cwd` 为 `NULL` 时是否整体省略 `file.changed` | **design 已定且实现一致**：`design.md` Risks「解析失败时按越界处理（`outsideWorkspace: true` + 只给 basename）」、`CORE_PORTS_AND_STORAGE.md` §6 第 21 条同；R7 的场景「工作区之外的路径被显式标记」据此断言。**实现未擅自决定，无 finding** |

**「节点级事件的端到端落库/投递」是否应由 WP3 承担**：**否**。plan 的 WP3 Write Scope 是 `crates/core/src/`、`crates/storage-sqlite/src/`，`crates/app/src/compose.rs` 明确不在其内（plan 全文未把 `crates/app/**` 分配给任何 WP，见 verification.md 的「规划缺口登记：节点级事件的组合根接线无人认领」）。我独立核对：全仓库（除测试与 `ports.rs` 声明）**没有任何生产代码**调用 `Broker::commit_node_event`/`node_submit`，`NodeEventSink` 也**零构造点**（`NodeEventSink::new` 连测试都没调用），WP4 的 `NodeEvents` 复用了通用 `EventSink`。因此 —— **WP3 的交付面（core 侧入口与落库形状）完整**，缺的是组合根接线；该项已由 `review-w4-r1` 报为 MAJOR 并登记，我独立确认其归属判断成立，**不重复开新 finding**。同理，「`file.changed` 端到端（真实 ACP Diff → core）」取决于 WP4 的 `diff` 键投影（已落地，见 `.worktrees/wp4/crates/agent-host/src/mapper.rs:347-371`），WP3 侧消费面已就位。

---

## 3. Findings

### review-w3-r1-F1（MAJOR）「显式置空标题」的窄语句丢掉版本递增与更新时间

**位置**：`crates/storage-sqlite/src/session_store.rs:1171-1181`（`match &update.title { Some(None) => sqlx::query_scalar(...) ... }`）。

**触发条件**：任一次提交的 `StateChange::Update.title == Some(None)`（即 ACP 显式把标题置空经 broker 投影，`broker.rs:2152`）。此时存储层**替换整条 UPDATE 语句**，执行 `UPDATE owned_session SET title = NULL WHERE session_id = ?1 RETURNING version`，`?3`（`commit.at`）不再写入。

**预期 / 实际**：

| 项 | 预期（`CORE_PORTS_AND_STORAGE.md` §5.2/:862 与 `design.md` D7 的明文） | 实际 |
| --- | --- | --- |
| 会话版本 | 「含 `StateChange` 的提交为当前版本 + 1」（core 的 `predict_session_version`（`broker.rs:3420-3436`）与 `session_version_rule.rs` 的断言都建立在这条上）；`CommitOutcome.version` 应返回递增后的值 | 窄语句**不含** `version = version + 1`，`RETURNING version` 返回**未递增**的旧值 |
| `owned_session.updated_at` | D7：「会话的**权威**更新时间取 Daemon 持久化时间（`commit.at`，由存储层写进 `owned_session.updated_at`）」 | 窄语句**不含** `updated_at = ?3`，会话排序列（`session_store.rs:2061` 的 `ORDER BY s.updated_at DESC`）不前进 |
| 同一提交里的其它列（`state`/`closed_at`/`current_mode_*`/`agent_session_id`/`workspace_cwd`/`workspace_alias`） | 按各列原有 `CASE WHEN` 规则写入 | 窄语句**整个替换**了上面按 `update.mode` 选出的语句——这些列一律被**静默丢弃**，不报错 |

**影响（可达性最高的一条）**：`commit_chunk` 的 `state` 是**单条** `SessionUpdate`，`flush_locked` 会把同一合并窗口内所有非终态事件打进**同一批**。因此当一个批次同时含「要求注入会话版本的事件」（`session.mode.changed`/`session.config.changed`，`broker.rs:191` 的 `SESSION_VERSION_VIEW_EVENT_TYPES`——它们由 Agent 的 `current_mode_update`/`config_option_update` 产生，与 `session_info_update` 同属一个 ACP 通知流）**与**「显式置空标题」时：

1. `finalize_owned_views` 按「含 `StateChange` ⇒ current + 1」注入 `version` 并记 `predicted`；
2. 存储层走窄语句 → 返回未递增的版本；
3. `commit_owned`（`broker.rs:3362-3368`）比对不一致 → 返回 `PortError::Corrupt("存储层返回的会话版本与 core 推导不一致")`；
4. `flush_locked` 已 `std::mem::take` 掉整个缓冲区，`?` 直接向上返回 → **该批次的所有事件（含那条 `session.mode.changed` 与 `session.info.changed`）永久丢失**，只剩一条告警日志（`daemon.rs` 的 `daemon.merge_window_failed`）。`Corrupt` 不属 `Unavailable`，不走 §6 第 9 条的 `uncertain`/放弃 turn 路径。

另一处独立证据：core 的 fake 端口（`broker.rs:4687-4692`）对同一输入 `Some(None)` 会走通用的 `Session::try_new`，**版本 +1 且 `updated_at = at`**——同一个端口契约的两份实现在可观察输出（`CommitOutcome.version`、`updated_at`）上不一致，且现有用例只覆盖 `title: None`（`session_version_rule.rs`）与 fake，掩盖了该分歧。

**修复建议**：不要替换语句，把「显式置空」编码进 `CASE`，让版本递增与 `updated_at` 对两条路径一视同仁。

```suggestion
                // `title` 是**两层可选**：`None` = 不改该列；`Some(_)` = 本提交要写该列，
                // 其中内层 `None` = 显式置空、内层 `Some(text)` = 写入该标题。
                // 用**一个**哨兵参数把「置空」与「不改」区分开，从而复用同一条语句：
                // 版本递增与 `updated_at` 对两条路径一致（§5.2/§9 判据 31；`design.md` D7）。
                let title_bind = match &update.title {
                    None => sqlx::types::Json::<Option<String>>(None),
                    Some(value) => sqlx::types::Json::<Option<String>>(value.clone()),
                };
```

（等价的更小改法：保留现语句，仅把窄语句改成 `UPDATE owned_session SET title = NULL, version = version + 1, updated_at = ?2 WHERE session_id = ?1 RETURNING version` 并 `bind(commit.at.as_str())`。注意仍会丢同一提交里的其它列，故推荐上面的单语句方案。）

**置信度**：0.7（窄语句的文本与缺失的两个赋值是读码即得的事实；`Corrupt` 路径依赖「同批含 version 事件」这一可达但不被现有用例覆盖的组合）。

### review-w3-r1-F2（MINOR）去重键取自已收窄的展示路径，越界同名文件互相吞并

**位置**：`crates/core/src/derive.rs:149-172`（`dedup_key` = `toolCallId + '\u{1}' + display_path`）、消费点 `crates/core/src/broker.rs:2113-2124`。

**触发条件**：一次工具调用的 `diff` 数组里有两个**不同**的元素，两者都在工作区之外（或被判为越界），且 `file_name()` 相同（例如 `/etc/nginx/nginx.conf` 与 `/tmp/nginx.conf`）。`outside_display`（`derive.rs:326-334`）按合同**只下发文件名**，因此两个元素的 `display_path` 都是 `nginx.conf`，`dedup_key` 相同。

**预期 / 实际**：R5 要求「MUST NOT 为**同一处改动**重复派生」，即不同改动应各派一条；实际第二条 `change.derived` 被 `file_change_plan.iter().any(...) || slot.file_changes.contains(...)` 判为重复而 `continue`，**静默丢失**（无日志、无错误）。同一碰撞也会让两者的 `changeId` 相同（`change_id` 同样只取展示路径）。

**影响**：客户端少一条 `file.changed`，UI 的「本 turn 改了哪些文件」不完整；越界路径恰恰是设计上必然退化为 basename 的一类（D5）。

**修复建议**：去重键改用**工具调用给出的原始路径**（`display_path` 之前的 `path`），展示值仍用收窄后的 `display_path`。

```suggestion
        let (display_path, outside) = display_path(workspace_root, &path);
        // 去重键必须区分「不同处改动」：展示路径是**有损**的（越界时只留 basename），
        // 两个越界同名文件会因此碰撞；工具调用给出的原始路径才是同一处改动的稳定标识。
        derived.push(DerivedFileChange {
            tool_call_id: tool_call_id.to_owned(),
            kind,
            display_path,
            outside,
            stats,
            origin_key: path,
        });
```
`origin_key` 为非投影字段，`dedup_key()` 与 `change_id()` 改用它（`format!("{}\u{{1}}{}", self.tool_call_id, self.origin_key)`），`file_changed_view` 不变。

（最小改法：给 `DerivedFileChange` 加一个非投影字段 `origin_key: String` 存 `path` 原文，`dedup_key()` 用它；`file_changed_view` 不变。）

### review-w3-r1-F3（MINOR）空字符串 `agentId` 通过节点级事件校验

**位置**：`crates/core/src/broker.rs:666-672`（`if agent_id.is_none() { Err(...必须带非空 agentId...) }`）、取值面 `crates/core/src/derive.rs:485-491`（`node_event_parts`）。

**触发条件**：`commit_node_event` 收到 view 为 `{"agentId":"","state":"connected"}` 的 `agent.connected`。

**预期 / 实际**：报错文案与合同（`docs/CORE_PORTS_AND_STORAGE.md` §6 第 21 条「校验…`agentId`…」）声称校验「非空」`agentId`，schema 为 `agentId: {minLength: 1, maxLength: 128}`（`schemas/sync/v1/event-views.schema.json#/$defs/agent.connected`，已逐字核对）；实际 `string_member` 对 JSON `""` 返回 `Some("")`，`is_none()` 为假 → **通过**，落一条 `agentId` 为空串的 schema 非法事件，消费端（`sync-protocol` 的 `NonEmptyText<128>`）解析失败。

**影响**：core 是节点级事件的**唯一**提交入口，其校验是最后一道形状防线；上游一旦给出空 `agentId`（或未来新增生产者），非法视图会被持久化并进入全局 replay。当前仓库内唯一生产者（WP4 的 `agent-host`）用真实 `AgentId`，故当日无运行时故障。

**修复建议**：按形状而非缺席判定（复用既有 `require_bounded` 口径）。

```suggestion
        let (agent_id, state) = derive::node_event_parts(event.payload.view.as_str());
        if agent_id.as_deref().is_none_or(str::is_empty) {
            return Err(PortError::InvalidRequest(
                "节点级 Agent 事件的 view 必须带非空 agentId（§10.3）",
            ));
        }
```

### review-w3-r1-F4（MINOR）§5.2 的 `[决定]` 未随新增字段更新

**位置**：`docs/CORE_PORTS_AND_STORAGE.md:28`（本次新增的版本 0.18 记录，声称「§5.2 的 `StateChange::Update` 增**两层可选**标题字段」）；应有而未更新的正文是 `:394` 的 §5.2 `[决定]`（对照 `:887` 的 §6 第 21 条）。

**触发条件**：读者依 §5.2 判断「`StateChange::Update` 能改哪些列」。

**预期 / 实际**：`docs/CORE_PORTS_AND_STORAGE.md:28` 声称「§5.2 的 `StateChange::Update` 增**两层可选**标题字段」，但 §5.2 正文唯一那条 `[决定]`（`:394`）仍是「新增**三个**可空列……`None` = 不改该列」，未提 title，且其「`None` = 不改该列」的单层口径**恰好漏掉** `Some(None)` 这个第三态。因 §5 的 ```rust 块不含 `SessionUpdate`/`StateChange`，`check:drift` 无法覆盖该断言（见 D）。

**影响**：文档自相矛盾，读者会按单层可选理解 title 列；不阻断构建，但把 F1 这类实现偏差的对照面变模糊。

**修复建议**：在 `:394` 那条 `[决定]` 末尾追加一句说明 title 列的两层可选语义并指向 §6 第 21 条（或把该条拆成「窄写列清单 + title 的两层可选」两条）。

---

## Assessment

**结论：FAIL（针对 Target Revision `0f70c4c2ddc41d21ba86abb0c6e83ef3da11ddc1`）**

- 未关闭的 **MAJOR**：`review-w3-r1-F1`（标题显式置空路径丢版本递增与 `updated_at`；同批含 `session.mode.changed`/`session.config.changed` 时整批 `Corrupt` 且事件丢失）。按角色判据「存在已确认且未解决的 CRITICAL/MAJOR → FAIL」。
- 其余：F2（P2）、F3（P3）、F4（P3）为非阻断项，但建议在 F1 的修复轮次一并处理（F2 是同一处派生键的逻辑瑕疵）。
- 四个必查重点中，**1.1 行级 diff 边界、1.2 路径前缀判定、1.3 节点级 DDL 自洽与无 migration、1.4 ACP 三要素未变** 全部通过；A/C/D 合规，B 通过但受 F1 影响（CLEAR 路径的 `updated_at`）。

## 实际检查范围

- 版本与范围：`git log`/`git diff --stat 33040d78..0f70c4c2`（15 files/+1644/−19）、逐文件 `git diff`（`broker.rs`、`derive.rs`、`lib.rs`、`model/json.rs`、`model/mod.rs`、`ports.rs`、`session_store.rs`、7 个 storage 测试、`docs/CORE_PORTS_AND_STORAGE.md`）、`git diff --name-only` 越区核对、`git diff -- crates/storage-sqlite/src/migrate.rs`（零行）、`git diff -- crates/sync-protocol/ schemas/ fixtures/`（零行）。
- 完整读取的实现：`crates/core/src/derive.rs` 全文 866 行（含 Myers、`split_lines`、`line_stats`、`display_path`/`normalize`/`canonical_path`/`strip_verbatim`、`title_intent`、`file_changed_view`、`view_carries_diff` 及其单测）；`broker.rs` 的 `commit_node_event`/`node_submit`/`workspace_root`/`derive_file_changes`、`commit_chunk` 全循环、状态组装与提交后登记、`commit_owned`/`finalize_owned_views`/`predict_session_version`/`event_origin`；`storage-sqlite` 的 `commit_owned` 的 `StateChange::Update` 分支与事件插入循环、`load_recovery`、`expires_at`、`acp_columns`、`replay`、`read_session`。
- 契约与需求对照：`specs/core-derived-events/spec.md`（R5–R9 全文）、`specs/workspace-resolution/spec.md`、`specs/local-agent-host/spec.md`、`proposal.md`、`design.md`（D4–D7、Risks）、`plan.md`（WP 表 :861、Shared File Ownership :908-931、PV2 :993、E2E 降级依据 :1017）、`AGENTS.md`；`docs/SYNC_PROTOCOL.md` §9.4/§9.6/§10.2/§10.3、`docs/CORE_PORTS_AND_STORAGE.md` §3.6/§5.2/§5.4/§6/§7.2/§7.3/§9 判据 31、`docs/MODULE_ARCHITECTURE.md` §4.1/§5。
- 机器合同逐字节核对：`schemas/sync/v1/event-views.schema.json` 的 `file.changed`/`agent.connected`/`agent.disconnected`/`session.info.changed`/`tool.call.*` 与 `common.schema.json` 的 `decimalString`/`uuid`/`sessionSummary`/`timestamp`；`schemas/acp/v1/upstream/schema.json` 的 `Diff`/`SessionInfoUpdate`；`crates/sync-protocol/src/views.rs` 的 `view_registry!` 与 `VIEW_ENUMS`；`fixtures/sync/v1/valid/view-{file-changed,file-changed-stats-omitted,file-changed-outside-workspace,agent-connected}.json` 的键集合与序列号。
- 门禁机制阅读（判定绿是否有效）：`scripts/check-contract-drift.mjs` 全文（`scanDeclarations`/`normalizeSignature`/`checkPorts`/`checkDdl` 与解析健全性下限）、`scripts/check-command-catalog.mjs:78-104/257`、`package.json` 的 `check` 链；`docs/CORE_PORTS_AND_STORAGE.md` §5 rust 块逐条提取核对（15 trait 均在、`NodeEventSink` 双侧同形、`SessionUpdate`/`StateChange` 不进门禁）。
- 跨边界消费侧（在 diff 之外）：`crates/server/src/node_link/resource.rs:1024-1048` 与 `:152-160`（会话级投递对 `session.is_none()` 显式返回）、`crates/app/src/compose.rs:532-556`（`LoggingPublisher`）与 `:219`（`AgentHost::new` 未接节点级缝）、`crates/app/src/daemon.rs:719-833`（合并窗口驱动与失败日志）、`crates/agent-host/src/mapper.rs:149-213/290-340`（`session_info_update`→`session.info.changed` 保留 `payload.acp`）与 `.worktrees/wp4` 的 `diff_elements` 投影、`crates/acp-protocol/src/content.rs:218-232`（`Diff` 形状）。
- 只读消费的证据：`reports/PV1-wp3-verify.log`（含 `schema fixtures OK: 149 valid, 51 invalid, 41 event views bound`、`crate boundaries OK: 12 crates`、`contract drift OK: 36 DDL / 15 trait / 96 methods`）、`reports/PV1-wp3-rust.log`、`reports/PV1-wp3-agentic.log`、`reports/PV2-wp3.log`、`reports/deliver-wp3-r1.md`、`verification.md`（规划缺口登记节）、`reports/review-w4-r1.md`。
- 拓扑与归属：`git merge-base`/分支状态、plan 的 Write Scope 与 Shared File Ownership、`crates/app/**` 是否被分配给任何 WP（未分配，与 verification.md 的登记一致）。

## 未验证内容

- **未执行**任何编译、`cargo test`、`cargo clippy`、`cargo fmt`、`npm run verify`、`check:drift` 或 E2E（只读边界）；PV1/PV2 的结论**只读消费**作者日志，本次未复跑，不宣称实际运行通过。F1 的 `Corrupt` 触发路径是**静态推演**（读 core 的预测/比对逻辑与存储层 SQL），未通过运行复现。
- **未验证**真实 SQLite 上的标题列往返（`Some(None)`/`Some(Some(t))` 两路径）：既有的 `storage-sqlite` 测试全部只传 `title: None`，本包未新增该列的行为用例（用例充分性交 validator / Coverage Index）。
- **未验证**真实 Daemon 端到端：节点级事件的组合根接线（`crates/app/src/compose.rs`）不在本次范围内且尚未接线，故 R8 的端到端落库/投递不可观察；`crates/app/tests/node_link_e2e.rs` 的 AC1 未执行。
- **未验证** Windows 上的 verbatim/UNC 实机行为（`strip_verbatim` 只有 temp 目录常规路径的用例覆盖）与 symlink/junction 场景（用例走 `canonicalize` 的 temp 目录）。
- **未验证** WP4 交付中与 WP3 相关的部分是否在后续提交中变化（本报告只核对 `.worktrees/wp4` 当前内容作为「消费方/生产者是否给出 `diff` 键与保留 ACP 原文」的事实依据，WP4 的检视结论以 `reports/review-w4-r1.md` 为准）。
- **不判**用例集合是否齐备（Coverage Index）与前端渲染口径（WP7）；`file.changed` 的 `kind` 词表 vs 原型 `modify` 的差异只作提示，不构成 finding。

## Changes

只读检视，未修改任何代码、测试、规划文件、任务状态或 `verification.md`；未切换分支、未提交、未合并、未运行任何写文件的构建或测试。仅新增本报告（`openspec/changes/sync-scope-and-pwa-client/reports/review-w3-r1.md`）。

```agentic-handoff
version: 1
agent_context:
  agent_id: "review-w3-r1"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "3.6"
    work_package: WP3
    role: reviewer
    phase: branch
    round: 1
    stage: work-package
    target_revision: "0f70c4c2ddc41d21ba86abb0c6e83ef3da11ddc1"
    evidence_type: REVIEW
    evidence_id: review-w3-r1
    report_path: "openspec/changes/sync-scope-and-pwa-client/reports/review-w3-r1.md"
    result: FAIL
    evidence_status: NEW
    applicability_basis: "在固定提交 0f70c4c2ddc41d21ba86abb0c6e83ef3da11ddc1（基线 33040d78）上只读检视 WP3 的 15 文件增量：四个必查重点（行级 diff 边界输入与省略口径、规范化前缀判定与越界收窄、节点级事件三条 DDL CHECK 自洽与零 migration、ACP 原文三要素未变且断言非恒真）全部通过；依赖闭包（4 依赖、p256 仅 arithmetic、自实现 Myers、未 import sync-protocol）与越区改动（storage 测试 19 处纯加法）合规；1 条 MAJOR（session_store.rs 的 title=Some(None) 窄语句丢 version+1/updated_at，与同批 version 事件共存时整批 Corrupt 丢事件）+ 3 条非阻断 MINOR。"
    source_evidence: NOT_APPLICABLE
```
