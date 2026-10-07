<!-- WP3 交付报告（MU2，第 1 轮）。coder 交付固定提交与可核对证据；不判独立 review、合入或最终验收。 -->

# WP3 core 派生事件生产者交付（deliver-wp3-r1）

## 交付摘要

| 项 | 取值 |
| --- | --- |
| worktree | `D:\Project\acp-remote\.worktrees\wp3` |
| 分支 | `feat/wp3` |
| 固定起点 | `33040d78324ff51be49219fcb3aac054d0100cc9` |
| **交付提交（target）** | `0f70c4c2ddc41d21ba86abb0c6e83ef3da11ddc1` |
| 提交信息 | `feat(sync): core 派生 file.changed 与节点级 Agent 事件，会话标题单向更新（R5–R9）` |
| `CARGO_TARGET_DIR` | `D:\Project\acp-remote\.target-wt\wp3`（独占） |
| `git status --porcelain` | 0 行（提交后工作树干净） |

`git diff --stat 33040d7..0f70c4c`（**15 files changed, 1644 insertions(+), 19 deletions(-)**）：

```
 crates/core/src/broker.rs                          | 657 +++++++++++++++-
 crates/core/src/derive.rs                          | 866 +++++++++++++++++++++  (新文件)
 crates/core/src/lib.rs                             |   5 +
 crates/core/src/model/json.rs                      |  44 ++
 crates/core/src/model/mod.rs                       |   4 +-
 crates/core/src/ports.rs                           |  39 +
 crates/storage-sqlite/src/session_store.rs         |  13 +
 crates/storage-sqlite/tests/commit.rs              |   7 +
 crates/storage-sqlite/tests/contract_v03.rs        |   1 +
 crates/storage-sqlite/tests/enum_coverage.rs       |   1 +
 crates/storage-sqlite/tests/migration.rs           |   1 +
 crates/storage-sqlite/tests/retention.rs           |   2 +
 .../storage-sqlite/tests/session_version_rule.rs   |   3 +
 crates/storage-sqlite/tests/workspace_alias.rs     |   4 +
 docs/CORE_PORTS_AND_STORAGE.md                     |  16 +
```

**范围声明**：`crates/core/src/`、`crates/storage-sqlite/src/`、`docs/CORE_PORTS_AND_STORAGE.md` 三处为 WP3 的 Write Scope。`crates/storage-sqlite/tests/` 的 7 个文件**不在** Write Scope 内，是 R9/D7 增加 `SessionUpdate.title` 后**接口强制的连带编辑**（纯机械加法，逐处插入 `title: None,`，未改任何断言或用例语义）——经 main 于 2026-10-04 明文裁决执行并登记为规划缺口，详见「跨区连带改动」一节。

---

## 实现内容（对应六项要求）

### R5/D4 — `file.changed` 的派生源唯一

- 派生入口只在 `tool.call.started` / `tool.call.updated` / `tool.call.completed`（`derive::file_change_event`），判据是 **view 的 `diff` 键**（`derive::view_carries_diff`）。
- **不读 `rawInput`/`title` 等自由形状字段**：`derive.rs` 只在 view 顶层取 `toolCallId` 与 `diff`（`json_object_members`），无任何对原始输入的解释。
- 派生的 `changeId` 是 `SHA-256(会话 + toolCallId + 展示路径)` 前 16 字节并置 UUIDv4 版本位（`canonical_uuid`）——**确定性**，形状符合 `common.schema.json#/$defs/uuid`。
- **不重复派生**：去重键 `(toolCallId, 展示路径)`，见下节。
- **不改写 ACP 原文**：`file.changed` 是**新增**事件，`tool.call.*` 的 `payload`（含 `acp`）原样进提交；用例 `deriving_a_file_change_leaves_the_acp_raw_document_untouched` 断言 `payload.acp` 与派生前的 `AcpRaw` 逐字段相等（`raw_json`/`sha256`/`byte_length`）。

### R6/D4 — 行级差异统计

- `derive::line_stats(old, new)`：对两段文本跑**行级 Myers diff**（`myers_distance`，论文 Algorithm 1 的贪心前向搜索，`O((N+M)·D)` 时间、`O(N+M)` 空间），由编辑距离 D 反解 `lcs = (N+M−D)/2`，从而 `added = M−lcs`、`deleted = N−lcs`。**MUST NOT 用行数差**——`line_stats_counts_replacement_when_line_counts_are_equal` 断言「行数相等但内容不同」报出 `(1, 1)`。
- 新建：`old` 为 `None` → `(new_lines, 0)`；删除（`new == ""`）→ `(0, old_lines)`。
- **判定不出时省略**：`newText` 缺席 → `stats = None` → view 里 `addedLines`/`deletedLines` **两个键整个缺席**（`file_changed_view` 只在 `Some` 时写出），用例 `a_diff_without_new_text_omits_the_line_counts` 断言两键不存在。
- 病态输入保护：工作预算 `LINE_STAT_WORK_BUDGET = 8_000_000` 与行数硬上限 `LINE_STAT_MAX_LINES = 200_000`，超出即返回 `None`（省略行数，与 `design.md` D4 的回滚条件 (b) 口径一致）。
- `split_lines` 只按 `\n` 切分且**空文本 → 0 行**（末尾换行不额外产生空行），因此「整文件换空」报 `(0, N)` 而不是 `(1, N)`。

### R7/D5 — 路径相对化与越界

- `derive::display_path(workspace_root, reported)`：
  - 绝对路径且经规范化位于工作目录根之下 → 下发 `/` 分隔相对形式（不含根片段、不含回退层级）；
  - 否则 → `outside = true` 且展示值只取 `file_name()`（无目录片段、无回退层级）；
  - 相对形式按「相对工作目录根」解释；逃出根（前导 `..`）或根未登记 → 按越界处理（**绝不**静默下发绝对路径）。
- **前缀判定用规范化后的组件关系**（`canonical_path` + `Path::strip_prefix`），不是字符串前缀：`display_path_does_not_treat_a_string_prefix_as_inside` 断言 `/work/api` 与 `/work/api-tools/file.txt` 判为越界。
- `canonical_path` 处理「Agent 报出的新文件尚不存在」：`canonicalize` 失败时逐级上溯到第一个存在的祖先、规范化后把剩余组件接回；Windows 上剥 `\\?\`/`\\?\UNC\` verbatim 前缀（`strip_verbatim`，`cfg(windows)`）。
- 工作目录根经 `SessionStore::load_recovery` 窄读取 `owned_session.workspace_cwd`（`Broker::workspace_root`）——该列不进任何可投影形状，故走窄入口；`None`（未登记 workspace 或非可恢复会话）时按越界处理。

### R8/D6 — 节点级 Agent 事件

- `ports.rs` 新增 `NodeEventSink`（与 `EventSink` 同形的 `Arc<dyn Fn(EndpointEvent)>` 包装类型 + `Clone`/`send`）：语义是「**不属于任何会话**的事件」，使「节点级事件不得进入会话槽位」在类型上可见。已写入 `docs/CORE_PORTS_AND_STORAGE.md` §5.4 的 rust 块（`check:drift` 通过）。
- **core 侧提交入口**：`Broker::commit_node_event(event: EndpointEvent)`（`node_submit` 是其薄封装）。校验**失败关闭**（返回 `PortError::InvalidRequest`）：
  - `event_type` ∈ `{agent.connected, agent.disconnected}`；
  - `kind == EventKind::State`；
  - `turn`/`causation` 均为 `None`（不属于任何 turn，也不由命令触发）；
  - `payload.acp` 必须为 `None`；
  - view 带非空 `agentId`（`derive::node_event_parts`）；
  - `state` 与该事件类型的唯一取值一致（`connected` / `disconnected`，封闭词表）。
- 提交形状是 `OwnedCommit { session: None, ... }`：`owned_event` 的 `session_id`/`session_sequence`/`origin_epoch`/`origin_sequence` 四列由**既有** CHECK 约束为同时为空。**未新增 migration、未改 DDL**。
- 用例：`a_node_level_agent_event_is_committed_without_session_identity`（四列均 `None`、仍有 `global_sequence`、进入 replay 流）与 `node_level_events_fail_closed_on_shape_violations`（四类形状违规逐一拒绝）。

### R9/D7 — 会话标题单向更新

- `ports.rs` 的 `SessionUpdate` 新增 **两层可选** `title: Option<Option<String>>`（`None` = 不改该列、`Some(None)` = 显式置空、`Some(Some(text))` = 写入）；`storage-sqlite` 与 core 的 fake store 都按该列语义实现。
- `broker.rs` 在 `session.info.changed` 事件上投影标题：`derive::title_intent(acp_raw, view)`：
  - **权威来源是 ACP 原文**（`payload.acp.rawJson` 的 `params.update.title` **键是否存在**）——公共 view 的 `title` 是 §10.3 的 required 字段（`string|null`），键永远存在，区分不了「缺席」与「显式 null」；
  - 键缺席 → `Unchanged`（保持既有标题）；键存在且 `null` → `Clear`；字符串 → `Set`；
  - ACP 原文不可用/结构不符时**退回 view**：`null` 按 `Unchanged` 处理（最保守解释，绝不把「不改」误判成「清空」）；字符串照写；
  - 标题 > 512 字符 → `PortError::InvalidRequest`（失败关闭，不写入违反值对象不变量的取值）。
- `updatedAt`：view 照常转发 Agent 自报值；会话的**权威**更新时间取 Daemon 持久化时间（`commit.at` → 存储层写 `owned_session.updated_at`）。用例 `a_session_info_update_writes_the_title_from_the_agent_notification` 断言会话 `updated_at` ≠ Agent 自报的 `2020-01-01T00:00:00.000Z`。
- **无重命名入口**：未新增命令；`required_grant` 的命令镜像不含任何重命名命令，用例 `a_freshly_renamed_session_has_no_rename_command_exists` 断言 `required_grant("session.rename")` 为 `None`。

### 去重（R5 的「同一处改动不重复派生」）

派生的去重键是 `(toolCallId, 展示路径)`，去重状态是 `Slot.file_changes`（插入顺序 FIFO，容量 `FILE_CHANGE_DEDUP_CAPACITY = 4096`），**只登记已提交的键**（落盘失败的批次不得吃掉重试的派生）。

同一个工具调用的 `tool.call.started` 与 `tool.call.updated` 会落在**同一次 `commit_chunk`** 里，因此命中判据有两处：已提交日志 `slot.file_changes` **与** 本批已计划的 `file_change_plan`（此为该轮修复点，见「修复记录」）。

---

## `crates/core/src/ports.rs` 签名改动（供 WP3 → WP4 共享写点合并）

**只改 `SessionUpdate` 等状态面与 §5.4 的基础设施面；`SessionEndpoint` 一行未动。**

1. `pub struct SessionUpdate` — 新增字段（插在 `state` 之后、`mode` 之前）：
   ```rust
   pub title: Option<Option<String>>,
   ```
2. `pub enum StateChange` — 新增属性 **（仅为 clippy 门禁，形状未变）**：
   ```rust
   #[allow(clippy::large_enum_variant)]
   ```
   说明：`Update(SessionUpdate)` 现为 296 字节而 `Create(NewSession)` 为 72 字节；装箱会改动全部调用点与匹配点写法，收益只是省下每次提交一次的枚举尺寸，故显式保留按值载荷。
3. `pub struct NodeEventSink(Arc<dyn Fn(EndpointEvent) + Send + Sync>)` — **新增类型**（§5.4，已在文档 rust 块登记）：
   ```rust
   #[derive(Clone)] pub struct NodeEventSink(...);
   impl NodeEventSink { pub fn new(f: impl Fn(EndpointEvent) + Send + Sync + 'static) -> Self; pub fn send(&self, event: EndpointEvent); }
   impl std::fmt::Debug for NodeEventSink
   ```
4. `EventSink` / `EventPublisher` / `Clock` / `IdGenerator` / `SessionEndpoint` / 全部 store trait：**零改动**。

**文档同步**：`docs/CORE_PORTS_AND_STORAGE.md` 版本 0.18 一行 + §5.4 rust 块的 `NodeEventSink` + §5.4 的两条 `[决定]`（节点级事件不走 `EventSink`；节点级事实的产生点在适配器）+ §6 新增第 21 条。`check:drift` 实测：`contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 96 个方法签名与 crates\core\src\ports.rs 一致`。

### WP3 → WP4 的接缝（供 main 接线，组合根不在任一包的 Write Scope）

- `agent-host` 侧（WP4 已确认）：`AgentHost` 经组合根注入的 `NodeEventSink` 交付**已构造好的 `EndpointEvent`**（`EventKind::State`、`event_type ∈ {agent.connected, agent.disconnected}`、`turn/causation: None`、`payload.acp: None`、view 为 `{"agentId":…,"state":…}`）。
- 组合根应把该 sink 接到 `Broker::node_submit(event).await`（或直接 `commit_node_event`）：`crates/app/src/compose.rs` 的 `AgentHost::new(...)` 处。
- 标题判定（R9）依赖 WP4 的 `session_info_update` 事件**保留 `payload.acp`**（`mapper.rs` 现状即如此，已与 WP4 对齐并请求不要在后续改动中清掉）。

---

## 跨区连带改动（`crates/storage-sqlite/tests/`，逐文件逐处）

**背景**：`SessionUpdate` 是公开结构体、调用点全是 struct literal。R9/D7 增字段后，`crates/storage-sqlite/tests/` 的既有 literal 必须补 `title: None,`，否则 `cargo test -p storage-sqlite` **不能编译**，而 tasks 3.5 要求 WP3 自己跑 PV2（含 storage-sqlite）。plan 的 Shared File Ownership 把该目录登记为「TP1 只新增契约向量用例文件、TP2 只新增行为测试文件」——**既有文件的连带编辑无人认领**，属规划缺口；main 已核实并裁决由 WP3 执行、在 verification 登记。

改动一律是**纯机械加法**：在 `mode:` 之后插入一行 `title: None,`，**未改任何断言、未改任何用例语义、未增删任何测试**。逐处清单（共 **19** 处 / 7 文件；仓库内无任何用 `..Default::default()` 的 literal）：

| 文件 | 处数 | 行号（交付提交中） |
| --- | --- | --- |
| `crates/storage-sqlite/tests/commit.rs` | 7 | 135, 272, 824, 913, 1184, 1221, 1252 |
| `crates/storage-sqlite/tests/contract_v03.rs` | 1 | 259 |
| `crates/storage-sqlite/tests/enum_coverage.rs` | 1 | 404 |
| `crates/storage-sqlite/tests/migration.rs` | 1 | 1233 |
| `crates/storage-sqlite/tests/retention.rs` | 2 | 601, 736 |
| `crates/storage-sqlite/tests/session_version_rule.rs` | 3 | 165, 226, 272 |
| `crates/storage-sqlite/tests/workspace_alias.rs` | 4 | 350, 458, 772, 819 |

**给 merger 的合并要求（WP3 → TP2）**：`crates/storage-sqlite/tests/` 这一行的 Merge Order 实际是 **WP3 → WP1 → TP2**（WP1 无本目录改动，TP1 已随 MU1b 合入，故实际为 **WP3 → TP2**）。WP3 必须先合入，TP2 才能在其上新增行为测试；候选组装时按 WP3 → TP2 复核并重跑 PV2。WP3 的这几处与 TP2 的**新增**文件不构成同一处冲突。`crates/core/src/**/tests.rs` 同理：WP3 只改 `broker.rs` 内的测试模块，未新建 `tests.rs`。

---

## Checks（实际命令 / 目录 / 环境 / 退出码 / 子检查 / 日志）

| ID | 命令 | 工作目录 | 环境 | 退出码 | 子检查 |
| --- | --- | --- | --- | --- | --- |
| **PV1** | `npm run verify` | `.worktrees\wp3` | `CARGO_TARGET_DIR=D:\Project\acp-remote\.target-wt\wp3`；Node v22 / rust-toolchain 1.98.1 | **0** | 见下 |
| **PV1-agentic** | `npm run check:agentic` | `D:\Project\acp-remote`（主检出） | 同上 | **0** | 宿主入口 17 个文件；`openspec validate --all --strict` 22 passed / 0 failed |
| **PV2** | `cargo test --locked -p core -p storage-sqlite -p agent-host --all-features` | `.worktrees\wp3` | 同上；fake ACP Agent，无外部服务 | **0** | 全部套件 0 failed |

**Local Checks**：`cargo fmt --all`（exit 0）+ `cargo clippy --locked -p core -p storage-sqlite --all-targets -- -D warnings`（exit 0，无 warning）。

### PV1 子检查逐条

- `check:schemas` → `schema fixtures OK: 149 valid, 51 invalid (ajv Draft 2020-12), 41 event views bound`
- `check:commands` / `check:errors` / `check:features` / `check:assets` / `check:acp` → 全部通过
- `check:docs` → `doc links OK: 415 relative links, 9057 section refs across 518 markdown files`
- `check:boundaries` → `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）`（**core 依赖闭包未变**）
- `check:drift` → `contract drift OK: §7 的 36 条 DDL … §5 的 15 个 trait / 96 个方法签名与 crates\core\src\ports.rs 一致`
- `check:agentic` → **在 worktree 内不可执行**（`node_modules/` 不随 worktree 提交，`scripts/agentic-gate.mjs` 找不到 `node_modules/@dongfanglin/openspec-agentic/bin/openspec-agentic.mjs`）；该门禁读仓库根的 `openspec/`（主检出）且不读任何被本包改动的代码，因此在主检出 `D:\Project\acp-remote` 上跑同一命令取证：**exit 0**，`openspec validate --all --strict` 含 `✓ change/sync-scope-and-pwa-client`、22 passed / 0 failed，宿主入口 17 个文件。
- `check:rust` → `cargo fmt --all -- --check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` + `cargo test --locked --workspace --all-features`，**exit 0**，全部套件 0 failed。

### PV2 子检查逐条（`-p core -p storage-sqlite -p agent-host --all-features`）

```
core lib (含 derive/broker 的新用例)  177 passed; 0 failed
storage-sqlite: commit 18 (+1 ignored，既有 baseline) / session_version_rule 3 / workspace_alias 6 /
                migration 12 (+1 ignored，既有 baseline) / retention 8 / enum_coverage 2 / contract_v03 5 /
                resume_columns 6 / permissions 4 / compaction_recovery 3 / attachments 5 / imported 10
agent-host: 22 passed
总计 0 failed
```

**命中 flake 情况**：**未出现** `crates/app/tests/daemon_lifecycle.rs` 的非确定性失败（PV1 的 `--workspace` 全量测试与 PV2 均一次通过）。未删测试、未弱化断言、未加 `#[ignore]`。两处 `ignored`（`commit.rs`/`migration.rs` 各 1）是既有 baseline 状态，本包未触碰。

### 证据日志

| 检查 | 日志路径（变更目录相对） | sha256 |
| --- | --- | --- |
| PV1（`npm run verify`） | `reports/PV1-wp3-verify.log` | `52257fafb64df08cacb165e1b4c4fdaf55bdef1478c8367be93b84b3409d7828` |
| PV1 的 `check:agentic` | `reports/PV1-wp3-agentic.log` | `3fcce2e065a25db34933860cf403366cd91e6cae6c7157273e6b5ff49128243a` |
| PV1 的 `check:rust` | `reports/PV1-wp3-rust.log` | `108f77e41918ab0f0082bd79c1bf756b059409b71d7e896daab69aa71aba8fe5` |
| PV2 | `reports/PV2-wp3.log` | `58280be8296b5b11918435ca1160b077577aab9b7511ed87fa66cb76a3a86f82` |

原件另存于 `D:\Project\acp-remote\.target-wt\wp3\logs\{PV1-verify,PV1-agentic,PV1-rust,PV2}.log`。

---

## 修复记录（本轮内）

1. **批内去重缺口**（R5）：`tool.call.started` 与 `tool.call.updated` 落在**同一次 `commit_chunk`**，而 `slot.file_changes` 只在提交成功后登记，导致同一处改动派生出 2 条 `file.changed`。修复：命中判据增加**本批已计划的 `file_change_plan`**（`lock(&slot.file_changes).contains(&key) || file_change_plan.iter().any(...)`）。用例 `a_repeated_diff_element_is_derived_only_once` 现为 1 条。
2. `derive::title_intent` 的降级路径（ACP 原文不可用）把 view 的 `null` 误判为 `Clear` → 增加 `clear_is_explicit` 形参，降级路径按 `Unchanged`（保守）。
3. `split_lines("")` 曾返回 1 行，使「整文件换空」报 `(1, N)` → 改为空文本 0 行。
4. `kind` 判据：`oldText` 在而 `newText` 缺席时曾判 `deleted` → 改为 `modified`（不猜改动方向）。
5. `display_path` 曾把相对形式无条件当工作区内 → 改为必须能与已登记的根比较，否则按越界（从严）。
6. clippy：`while_let_loop`（`canonical_path`）与 `large_enum_variant`（`StateChange`）两处按最小侵入修复。
7. 沿用的规则要求：测试辅助改用 `std::sync::LazyLock`（不引入 `once_cell`）。

---

## 未验证内容

- **真实 Daemon 路径（AC1）**：本包只在 memory fake 端口上验证；`cargo test -p app --test node_link_e2e` 的 AC1 断言（真实进程、真实 SQLite、`file.changed` 按 origin 顺序回放、节点级事件不被会话级投递路径误收、标题更新）**未执行**，留给 AC1。
- **节点级事件的投递/广播**：本切片只交付**产生与持久化**；`server::sync` 未落地，`crates/server/src/node_link/resource.rs` 的会话级路径会丢弃会话标识为空的事件。**未验证任何客户端可见性**。
- **WP4 的 Diff 投影**：`agent-host` 填充 view 的 `diff` 键（Option B）**尚未落地**；本包的派生用例用测试构造的 view 覆盖。端到端「真实 ACP Diff → core file.changed」未验证。
- **前端/浏览器**：不涉及（本包无 `clients/app`）。
- **`deps` / `advisories` / `secrets` 三个 CI-only job**：本地无等价物，未执行亦未声称通过。
- **路径相对化在 Windows 上的 verbatim/UNC 实机行为**：`strip_verbatim` 已按 `cfg(windows)` 实现，但用例覆盖的是 temp 目录下的常规路径（本机 `canonicalize` 已实测生效）；UNC 网络路径未实测。

## 待澄清问题

1. **`file.changed` 的 `summary` 与 `kind` 口径**：合同只把 `summary` 列为必填字符串、`kind` 列为 1..64 自由文本，未定义取值集合。我按「`kind` ∈ {`added`,`modified`,`deleted`}、`summary` = `"<kind> <displayPath>"`」的最小口径投影（不含 diff 正文摘要，因为 core 不依赖协议 crate、也不解析 ACP 原文结构）。若 review-3 期望另一种口径，需要明确规则后再调（当前实现集中在 `derive::DerivedFileChange::summary/kind` 一处）。
2. **`changeId` 的确定性**：我取「会话 + toolCallId + 展示路径」的 SHA-256 派生 UUID（确定性、形状合法）。合同未规定生成方式；若要求随机 UUID 或要求跨会话稳定，需要明确。
3. **节点级事件的视图 `summary`/错误字段**：`agent.disconnected` 的 view 允许可选 `error`（`common.schema.json#/$defs/publicError`），我**不构造**它（由 WP4 按进程退出原因自行投影），core 只做形状校验；若要求 core 补齐，需要明确来源。
4. **`agent_capabilities` 触发的首次 spawn 是否算「进程建立」**：WP4 报告称该路径也会上报一次 `agent.connected`。core 侧只接收事件、不判断语义，故不阻塞；但 R8「进程首次建立并可服务会话」的口径最终由 review-3/AC1 判定。
5. **`workspace_cwd` 为 `NULL` 的会话**：按「无法证明在工作区内」处理（越界 + 只给文件名）——这是 `design.md` Risks 记录的既有口径，若期望其它行为（例如整体省略 `file.changed`），需要明确。

---

```agentic-handoff
version: 1
agent_context:
  agent_id: "coder-w3-r1"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "2.3"
    work_package: WP3
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "0f70c4c2ddc41d21ba86abb0c6e83ef3da11ddc1"
    evidence_type: DELIVERY
    evidence_id: wp3-delivery-r1
    report_path: "reports/deliver-wp3-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "固定提交 0f70c4c = 33040d7 + 15 个文件的实现增量（新增 crates/core/src/derive.rs 866 行、broker.rs 派生与节点级提交、ports.rs 的 SessionUpdate.title 与 NodeEventSink、storage 的标题列写入、docs §5/§6 同步）。R5–R9 的实现侧齐备：类型化 Diff 派生唯一且不重复、行级 Myers diff 与省略口径、规范化前缀相对化与越界标记、节点级 agent.* 落库（不新增 migration、四列同时为空）、标题两层可选与单向更新（updatedAt 取 Daemon 持久化时间、无重命名命令）。crates/storage-sqlite/tests/ 的 19 处属接口强制的跨区连带编辑，已逐处列出并交 merger 按 WP3 → TP2 顺序复核。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.5"
    work_package: WP3
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "0f70c4c2ddc41d21ba86abb0c6e83ef3da11ddc1"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/deliver-wp3-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "npm run verify（cwd=.worktrees/wp3，CARGO_TARGET_DIR=D:\\Project\\acp-remote\\.target-wt\\wp3）在交付提交 0f70c4c 上 exit 0：check 十道全绿（check:schemas 149/51、check:docs 415 links/9057 refs、check:boundaries 12 crates、check:drift 36 DDL/15 traits/96 methods）与 check:rust 三条全绿（fmt/clippy/test）。check:agentic 在 worktree 内因 node_modules 不随 worktree 提交而不可执行，已在主检出对同一命令取证 exit 0（22 passed / 0 failed）。日志 reports/PV1-wp3-verify.log、PV1-wp3-agentic.log、PV1-wp3-rust.log。"
    source_evidence: reports/PV1-wp3-verify.log
  - task_id: "3.5"
    work_package: WP3
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "0f70c4c2ddc41d21ba86abb0c6e83ef3da11ddc1"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "reports/deliver-wp3-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo test --locked -p core -p storage-sqlite -p agent-host --all-features（cwd=.worktrees/wp3，CARGO_TARGET_DIR=.target-wt/wp3）在交付提交 0f70c4c 上 exit 0：core lib 177 passed/0 failed（含 derive 的 Myers 对拍与行数、相对化、标题口径用例与 broker 的派生/去重/越界/节点级/标题用例）、storage-sqlite 全部套件 0 failed、agent-host 22 passed。无 flake。日志 reports/PV2-wp3.log。"
    source_evidence: reports/PV2-wp3.log
checks:
  - id: PV1
    work_package: WP3
    command: "npm run verify (cwd=.worktrees/wp3, CARGO_TARGET_DIR=D:\\Project\\acp-remote\\.target-wt\\wp3)"
    exit_code: 0
    log_path: reports/PV1-wp3-verify.log
    result: PASS
  - id: PV2
    work_package: WP3
    command: "cargo test --locked -p core -p storage-sqlite -p agent-host --all-features (cwd=.worktrees/wp3, CARGO_TARGET_DIR=D:\\Project\\acp-remote\\.target-wt\\wp3)"
    exit_code: 0
    log_path: reports/PV2-wp3.log
    result: PASS
test_delivery:
  WP3:
    kind: automated
    artifacts:
      - reports/PV2-wp3.log
      - reports/PV1-wp3-verify.log
      - reports/PV1-wp3-agentic.log
      - reports/PV1-wp3-rust.log
    basic_checks:
      - PV1
      - PV2
```
