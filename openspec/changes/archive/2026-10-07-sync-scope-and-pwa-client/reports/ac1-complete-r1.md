<!-- AC1 收口交付报告（MU2，第 1 轮）。coder 只补测试、跑局部检查与工作包 Project Verify；不判独立 review、合入与最终验收。 -->

task_id: "4.1"
role: coder
phase: implement
agent_context:
  agent_id: "CompleteAc1"
  isolation: "fork_turns=none（新实例接管 `crates/app/tests/` 的补测；未继承 WP3/WP4 实现者、接线实现者或任何 reviewer 的对话）"
target_revision: "3b6fe4136e71e5a8a9b36527440348d9be280ae0"
scope: "仅 `crates/app/tests/node_link_e2e.rs`；`crates/core/**`、`crates/agent-host/**`、`crates/sync-protocol/**`、`schemas/**`、`docs/**`、`fixtures/**` 零净改动（反向对照期间的临时注入已全部还原，见 §D 自证）。"

# AC1 收口：`file.changed`（会话级）与标题更新两项断言的补齐（ac1-complete-r1）

## Shared Report

- **task_id**: `4.1`（AC1；`phase: implement`，DELIVERY + CHECK）
- **work_package**: MU2
- **role**: coder（`CompleteAc1`）
- **phase**: implement
- **agent_context**: `agent_id: "CompleteAc1"`，`isolation: "fork_turns=none"`
- **target_revision**: `3b6fe4136e71e5a8a9b36527440348d9be280ae0`（分支 `integ/mu2-wiring`；父提交 = 接线提交 `0cb5e62`）
- **scope**: 只改 `crates/app/tests/node_link_e2e.rs`（同包测试）。`crates/core/**`、`crates/agent-host/**`、`crates/sync-protocol/**`、`schemas/**`、`docs/**`、`fixtures/**` **零净改动**；无新 crate 依赖。
- **result**: **PASS（六项断言全绿 + 5 组反向对照逐条实测 FAIL + PV1/PV2 exit 0）**

### 交付提交

| 项 | 取值 |
| --- | --- |
| worktree | `D:\Project\acp-remote\.worktrees\integ` |
| 分支 | `integ/mu2-wiring`（**未**提交到 `refs/heads/main`；**未**改动 `feat/wp3`/`feat/wp4`） |
| 父提交（接线提交） | `0cb5e62b1941d97c685924fcbab87cae07c37d4f` |
| **交付提交（target）** | **`3b6fe4136e71e5a8a9b36527440348d9be280ae0`** |
| 提交信息 | `test(app): 补齐 AC1 的 file.changed 落库与 origin 序回放、会话标题更新两项断言` |
| `CARGO_TARGET_DIR` | `D:\Project\acp-remote\.target-wt\mu2int`（复用既有，未新建） |

`git diff --stat 0cb5e62 3b6fe41`（1 file changed, **+458 / −4**）：

```
 crates/app/tests/node_link_e2e.rs | 462 +++++++++++++++++++++++++++++++++++++-
```

---

## A. 背景：AC1 的缺口与本次补齐面

`review-wire-r1` 的 F1（MAJOR）：`plan.md:996` 与 `:1029` 的 AC1 要求四项断言全绿，接线提交只覆盖节点级两项（`agent.connected`/`agent.disconnected` 落库、会话标识为空、不被会话级投递路径误收），而 `file.changed`（会话级）经 owned 路径落库并按 origin 序回放、会话标题在 `session_info_update` 后更新**在 `crates/app/tests/` 下零覆盖**；且 MU2 候选检查（`plan.md:971`）只含 PV1/PV2、不含 AC1，缺口不会被预合入门禁拦下。本次补的正是这**两项**。

- 保留既有节点级用例 `ac1_the_composition_root_persists_node_level_events_without_session_identity`（未改动其断言）；
- 修正 `review-wire-r1` F2 指出的不实注释（原注释称本文件其余用例已用脚本化后端覆盖 Diff/标题场景，与事实不符——脚本化后端不产生这两类事件）；改为如实陈述本文件对 Diff/标题的覆盖位置；
- `ac1_config` 增 `ac1_config_with_scenario(data_dir, cmd, scenario)`，并把 `storage.persist_deltas = true` 写入 AC1 配置（见 §B 口径说明）。

---

## B. 两条新断言的实现与判据口径

两条用例都用**真实组合根 `app::compose::Composition` + 真实 SQLite + 真实 fake ACP 子进程**，场景取 fake 的
`chunked-updates`：它在**同一次 `session/prompt`** 内发出带**类型化 `diff`**（`path: "src/main.rs"`、
`oldText: "let a = 1;"`、`newText: "let a = 2;"`）的 `tool_call`、一条 `tool_call_update`（completed，仅普通
内容块）、以及带 `title: "会话标题"`、`updatedAt: "2026-09-24T10:00:00.000Z"` 的 `session_info_update`。
驱动方式：`UseCases::submit_command` 派发 prompt 后，测试轮询 `Broker::pump` 直到该轮 `turn.completed`
落库——这正是 `crates/app/src/daemon.rs` 的合并窗口在生产里做的同一件事（core 不读时钟、不设定时器）。

### B.1 断言 A —— `ac1_the_composition_root_persists_file_changes_on_the_owned_path_in_origin_order`

驱动**两个**会话各一次 prompt，因此库中恰有**两条** `file.changed`（否则「顺序」无从验证，这是本任务单的硬要求）。逐点断言：

| # | 判据（`plan.md:996/:1029`） | 本用例断言 | 观测面 |
| --- | --- | --- | --- |
| A1 | `file.changed`（会话级）**经 owned 路径落库** | `owned_event` 中 `event_type='file.changed'` 恰 **2 行**，两条的 `session_id` **均非空**且分别属于两个会话，`session_sequence`/`origin_sequence` **均非空** | 真实 SQLite 只读直连（`owned_event`）；`session_id` 非空即「owned 路径」区别于节点级事件（后者为空） |
| A2 | **按 origin 序回放** | 真实读视图的 origin 切片（`node_link_slice`，SQL 侧 `ORDER BY origin_sequence ASC`）取回会话全部事件，其 `origin_sequence` 恰为 `1..=N` 且严格递增；`file.changed` **落在序列内部**（不是派生后追加在批尾）、且**紧邻**产生它的 `tool.call.*`；游标续读 `after = seq(change)−1` 的第一条就是它，`after = seq(change)` 起不再出现它、余下条数为 `N − seq` | `Broker::read_view()` → `ReadView::node_link_slice`（真实 SQLite 读视图） |
| A3 | 由**类型化 Diff** 派生（R5） | 该行 view 的 `kind == "modified"`、`displayPath == "src/main.rs"` | 落库 `payload_json`（派生源是适配器从 ACP 原文投影的 `diff` 键，非 `rawInput`） |
| A4 | 带**行级** `addedLines`/`deletedLines`（R6） | `addedLines == "1"`、`deletedLines == "1"`（`"let a = 1;" → "let a = 2;"` 的行级 Myers 结果） | 落库 `payload_json` |
| A5 | `displayPath` 已相对化、工作区内越界标记缺席（R7/D5） | `outsideWorkspace` 键**不存在** | 落库 `payload_json` |

> **`persist_deltas = true` 的口径说明（为什么 AC1 配置要显式开它）**：默认 `false` 会让 turn 终态后压缩增量
> （把 delta 行标记 `compacted_into`、由一条 `turn.delta_compacted` 替代），而被压行**不进** origin 切片
> （`compacted_into IS NULL`），会在「origin 序恰为 `1..=N`」的序列里留下断层。开 `persist_deltas` 后每轮
> origin 序列连续，正是「按 origin 序回放」要观察的对象。这是配置项、不是实现改动。

### B.2 断言 B —— `ac1_the_composition_root_updates_the_session_title_from_the_agent_notification`

| # | 判据（`plan.md:996/:1029`） | 本用例断言 | 观测面 |
| --- | --- | --- | --- |
| B1 | **会话标题在 `session_info_update` 后更新** | `owned_session.title == Some("会话标题")`（创建时为空 → 通知后为通知取值） | 真实 SQLite `owned_session` |
| B2 | 标题写入走通用状态变更路径（版本递增） | `version > 1` | `owned_session.version` |
| B3 | **`session.info.changed` 事件的存在** | 该会话下 `event_type='session.info.changed'` 恰 **1 行**，`session_id` 非空；其 view `title == "会话标题"` | 真实 SQLite `owned_event` |
| B4 | `updatedAt` 取 **Daemon 持久化时间**，非 Agent 自报值（D7） | 会话 `updated_at != "2026-09-24T10:00:00.000Z"`（Agent 自报值）且 `updated_at >= created_at`；事件 view 的 `updatedAt` **照常转发** Agent 自报值（合同字段） | `owned_session.updated_at` 与落库 view |
| B5 | 会话摘要随之反映标题（读面） | `UseCases::list_sessions` 返回的 `SessionSummary.title() == Some("会话标题")` | 用例面读入口 |

> **未覆盖的三态之一（如实登记）**：fake ACP Agent 只在 `chunked-updates` 里发 `session_info_update` 且
> **恒带非空 `title`**，因此「通知显式置空」与「只带 `updatedAt`」两条路径无法经真实子进程驱动；它们由 WP3 的
> `crates/storage-sqlite/tests/title_write.rs` 与 `crates/core/src/broker.rs` 的 R9 行为测试覆盖（本任务边界
> **不允许**改 `crates/agent-host/**` 去新增场景）。

---

## C. 逐条需求映射（AC1 判据原文 ↔ 断言 ↔ 实测结果）

| AC1 判据（`plan.md:996` / `:1029`） | 断言（本文件用例） | 实测结果 | 反向对照 |
| --- | --- | --- | --- |
| `file.changed`（会话级）经 owned 路径落库 | A1（`ac1_…_persists_file_changes_on_the_owned_path_in_origin_order`） | **PASS** | RC-A1 |
| `file.changed` 按 origin 顺序回放 | A2（同上） | **PASS** | RC-A2 |
| `agent.connected`（节点级）落库后会话标识为空、可按事件库读回、不被会话级投递路径误收 | 既有 `ac1_the_composition_root_persists_node_level_events_without_session_identity`（未改动） | **PASS**（保留绿） | 接线作者已实测（`set_node_events` 置空 → FAIL，见 `wire-compose-nodeevents-r1.md` §3.3） |
| `agent.disconnected` 同形 | 同上 | **PASS**（保留绿） | 同上 |
| 会话标题在 `session_info_update` 后更新 | B1/B2/B3/B4/B5（`ac1_…_updates_the_session_title_from_the_agent_notification`） | **PASS** | RC-B / RC-B2 / RC-C / RC-C2 |

**AC1 六项断言全绿实测**（`reports/AC1.log`，`AC1_EXIT=0`）：

```
running 6 tests
test ac1_the_composition_root_updates_the_session_title_from_the_agent_notification ... ok
test ac1_the_composition_root_persists_node_level_events_without_session_identity ... ok
test tls_direct_terminates_the_same_handshake ... ok
test the_controlled_path_runs_end_to_end_and_revocation_propagates ... ok
test a_narrowing_repair_closes_the_live_attachment ... ok
test ac1_the_composition_root_persists_file_changes_on_the_owned_path_in_origin_order ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.39s
```

---

## D. 反向对照（每条新增断言：撤掉实现后该断言 FAIL）

**方法**：在**交付提交 `3b6fe41` 的同一工作树上**临时注入一处实现改动，`cargo test` 定位到被测断言为
FAIL，随后**逐一还原**。每组下「撤掉了什么 → 哪条断言失败 → 实测输出」。还原自证见 §D 末。

### RC-A1 —— 撤掉 `file.changed` 的派生 → A1 FAIL

- **注入**：`crates/core/src/broker.rs` 的 `commit_chunk` 内，把 `if file_change_event(...) {` 改为
  `if false && file_change_event(...) {`（派生块整段不可达）。
- **失败断言**：A1（落库行数）。
- **实测输出**：

```
thread 'ac1_…_persists_file_changes_on_the_owned_path_in_origin_order' panicked at
crates\app\tests\node_link_e2e.rs:1518:9:
assertion `left == right` failed: 两个会话各派生恰好一条 file.changed；实际 0 行
  left: 0
 right: 2
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 5 filtered out
```

### RC-A2 —— 撤掉 origin 序（改为逆序）→ A2 FAIL

- **注入**：`crates/storage-sqlite/src/session_store.rs` 的 `node_link_slice`，把
  `ORDER BY origin_sequence ASC` 改为 `ORDER BY origin_sequence DESC`。
- **失败断言**：A2（回放序列恰为 `1..=N`）。
- **实测输出**：

```
thread 'ac1_…_persists_file_changes_on_the_owned_path_in_origin_order' panicked at
crates\app\tests\node_link_e2e.rs:1581:9:
assertion `left == right` failed: 回放必须按 origin 序：序列恰为 1..=N 且严格递增；
实际 [19, 18, 17, 16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1]
  left: [19, 18, 17, 16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1]
 right: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 5 filtered out
```

> 该注入同时证明 A2 不是「恰好等于写入序」的空转断言——入库序与 origin 序不一致时它确实为红；且实测的两条
> `file.changed` 均在序列内部（19 条事件的会话中位于中部附近）。

### RC-B —— 撤掉标题投影（R9）→ B1（及 B5）FAIL

- **注入**：`crates/core/src/broker.rs` 的 `commit_chunk` 内，把
  `if event.event_type.as_str() == "session.info.changed" {` 改为 `if false && … {`（标题窄写入整段不可达）。
- **失败断言**：B1。
- **实测输出**：

```
thread 'ac1_…_updates_the_session_title_from_the_agent_notification' panicked at
crates\app\tests\node_link_e2e.rs:1694:9:
assertion `left == right` failed: R9：标题必须来自 Agent 的 session_info_update
  left: None
 right: Some("会话标题")
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 5 filtered out
```

### RC-B2 —— 改用 Agent 自报 `updatedAt` → B4 FAIL

- **注入**：`crates/storage-sqlite/src/session_store.rs` 的 `UPDATE … title = CASE …`（`ModeChange::Unchanged`
  分支），把 `updated_at = ?3`（Daemon 持久化时间 `commit.at`）改为字面量
  `updated_at = '2026-09-24T10:00:00.000Z'`（Agent 自报值）。
- **失败断言**：B4（权威更新时间来源）。
- **实测输出**：

```
thread 'ac1_…_updates_the_session_title_from_the_agent_notification' panicked at
crates\app\tests\node_link_e2e.rs:1735:9:
assertion `left != right` failed: R9/D7：会话更新时间不得采用 Agent 自报的 updatedAt
  left: "2026-09-24T10:00:00.000Z"
 right: "2026-09-24T10:00:00.000Z"
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 5 filtered out
```

### RC-C / RC-C2 —— 撤掉 `session.info.changed` 的投影 / 落盘 → B3 FAIL

- **RC-C 注入**：`crates/agent-host/src/mapper.rs` 的 `SessionUpdate::SessionInfoUpdate(info) =>` 分支用
  `if false { … }` 包住 `events.push(view_event("session.info.changed", …))`。
  实测：B1 先行 FAIL（`left: None / right: Some("会话标题")`），证明事件源头被撤后标题不再更新。
- **RC-C2 注入**（**隔离 B3 事件存在性断言本身**）：还原 mapper 后，改
  `crates/storage-sqlite/src/session_store.rs` 的事件插入循环，对 `event_type == "session.info.changed"` 的
  行 `continue`（**标题窄写入仍照写**，只丢该事件行）。
- **失败断言**：B3（`session.info.changed` 落库行数）。
- **实测输出**：

```
thread 'ac1_…_updates_the_session_title_from_the_agent_notification' panicked at
crates\app\tests\node_link_e2e.rs:1714:9:
assertion `left == right` failed: R9：一次通知投影恰好一条 session.info.changed；实际 0 行
  left: 0
 right: 1
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 5 filtered out
```

### 反向对照的自证：临时注入已全部还原

所有注入均对**受版本控制**的交付面（`crates/core/**`、`crates/storage-sqlite/**`、`crates/agent-host/**`），
还原后实测：

```
$ git diff --stat 0cb5e62 3b6fe41
 crates/app/tests/node_link_e2e.rs | 462 +++++++++++++++++++++++++++++++++++++-
 1 file changed, 458 insertions(+), 4 deletions(-)

$ git status --porcelain
（空）
```

即：唯一改动就是测试文件；三个受注入的文件回到交付提交的字节（`git status` 干净即为自证）。

---

## E. 交付面范围与边界

| 项 | 结果 |
| --- | --- |
| 改动文件集 | 仅 `crates/app/tests/node_link_e2e.rs`（1 file） |
| `crates/core/**` | 零净改动（反向对照的临时注入已还原） |
| `crates/agent-host/**` | 零净改动（同上） |
| `crates/storage-sqlite/**` | 零净改动（同上；注：该 crate 不在本任务边界明列，但反向对照按等价方式处理并还原） |
| `crates/sync-protocol/**`、`schemas/**`、`docs/**`、`fixtures/**` | 零改动 |
| 新 crate 依赖 | 无（`check:boundaries` 12 crate 全绿） |
| `feat/wp3` / `feat/wp4` | 未改动 |
| `refs/heads/main` | 未提交（当前 `main` 仍为 `33040d7`） |

---

## F. Checks

`handoff_index` 的 CHECK 行与本节的 `checks:` 同源；`log_path` 相对变更目录解析。

```yaml
checks:
  - id: PV1
    work_package: MU2
    command: "CARGO_TARGET_DIR='D:\\Project\\acp-remote\\.target-wt\\mu2int' npm run verify（cwd=.worktrees/integ）"
    scope: "工作包 Project Verify：check 十道 + check:rust 三条"
    exit_code: 0
    log_path: "reports/PV1.log"
    result: "PASS：check 十道全绿（schemas 149/51、assets 213 fixture files、boundaries 12 crates、drift 36 DDL/15 traits/96 methods、agentic 21 specs）+ check:rust 三条全绿（fmt/clippy/test）"
  - id: PV2
    work_package: MU2
    command: "CARGO_TARGET_DIR='D:\\Project\\acp-remote\\.target-wt\\mu2int' cargo test --locked -p core -p storage-sqlite -p agent-host --all-features（cwd=.worktrees/integ）"
    scope: "core 派生事件/行级 diff/越界标记/节点级事件/标题单向更新的行为测试"
    exit_code: 0
    log_path: "reports/PV2.log"
    result: "PASS：全 suite ok、0 failed（含 core 179 passed、storage-sqlite 43 passed、title_write 4/4、agent-host 各 suite 全 ok）"
  - id: AC1
    work_package: MU2
    command: "CARGO_TARGET_DIR='D:\\Project\\acp-remote\\.target-wt\\mu2int' cargo test --locked -p app --test node_link_e2e（cwd=.worktrees/integ）"
    scope: "AC1 替代检查：真实组合根 + 真实 SQLite + fake ACP Agent 上的六项断言"
    exit_code: 0
    log_path: "reports/AC1.log"
    result: "PASS：6 passed / 0 failed——节点级两项（保留）+ file.changed 会话级落库与 origin 序回放（新增）+ 会话标题更新（新增）"
```

| Check ID | 命令 | 范围 | 退出码 | 结果 | 日志 |
| --- | --- | --- | --- | --- | --- |
| `PV1` | `CARGO_TARGET_DIR='D:\Project\acp-remote\.target-wt\mu2int' npm run verify`（cwd=.worktrees/integ） | `check` 十道 + `check:rust` 三条 | **0** | 全部 PASS | `reports/PV1.log`（1887 行，`sha256:17900346…`） |
| `PV2` | `CARGO_TARGET_DIR='D:\Project\acp-remote\.target-wt\mu2int' cargo test --locked -p core -p storage-sqlite -p agent-host --all-features` | core/storage-sqlite/agent-host 行为测试 | **0** | 全 suite ok、0 failed | `reports/PV2.log`（623 行，`sha256:d908e22b…`） |
| `AC1` | `CARGO_TARGET_DIR='D:\Project\acp-remote\.target-wt\mu2int' cargo test --locked -p app --test node_link_e2e` | 替代检查 AC1 六项断言 | **0** | 6 passed / 0 failed | `reports/AC1.log`（`sha256:4c7e4513…`） |

### E.1 关于 `daemon_lifecycle.rs` 的已知 flake

本轮 `PV1` 的 `cargo test --workspace` 中**未命中**该非确定性失败（PV1 exit 0、无 `FAILED`；实测日志
`reports/PV1.log`）。未命中不等于消除，未对其做任何改动、未加 `#[ignore]`、未弱化断言。

### E.2 门禁过程中的两次红及其如实登记

1. **`cargo fmt --all -- --check` 首轮失败**：新增代码有多处 rustfmt 非规范换行（PV1 exit 1）。执行
   `cargo fmt --all` 后复跑，`--check` exit 0。**未**放宽门禁。
2. **`cargo clippy … -D warnings` 首轮失败**：`node_link_e2e.rs:1511` 的
   `Vec<(Option<String>, Option<i64>, Option<i64>, String)>` 触发 `clippy::type_complexity`。加局部
   `type FileChangedRow = …` 别名后复跑 exit 0。**未**加 `#[allow]` 绕过。

两次都发生在**本交付提交内容自身**，已随提交修正（`3b6fe41` 上的 PV1 为 exit 0）。

---

## G. 未验证内容（明确列出）

- **通知的三态之「显式置空」「只带 updatedAt」路径**：无法经 fake ACP 子进程驱动（该 fake 只在
  `chunked-updates` 发恒非空的 `title`）；由 WP3 的 `title_write.rs` 与 `broker.rs` R9 测试覆盖。本任务边界
  不允许改 `crates/agent-host/**` 加场景。
- **越界路径（`outsideWorkspace=true` + 只下发文件名）**：fake 的 `chunked-updates` 只发工作区内相对路径
  `src/main.rs`，无越界 Diff；该语义由 `crates/core/src/broker.rs` 的 R7 行为测试（`outside_workspace`）与
  TP1 的固定向量覆盖。
- **`addedLines`/`deletedLines` 的「判定不出时省略」分支**：本用例覆盖的是「能判定」正向
  （`0→1`/`1→1`），省略分支由 core/TP1 覆盖。
- **节点级事件的广播**：明确不在 AC1 范围（`plan.md` 的 AC1 行已排除广播断言；节点级事件当前无投递通道）。
- **`deps`/`advisories`/`secrets` 三个 CI-only job**：本地无等价物，未执行亦未声称通过。
- **真实 Agent（Codex/OMP）对拍**：未做（AC1 用 fake ACP Agent，与 plan 的环境列一致）。

---

## evidence_paths

- 交付 worktree：`.worktrees/integ`（分支 `integ/mu2-wiring`，HEAD `3b6fe4136e71e5a8a9b36527440348d9be280ae0`）
- 本报告：`openspec/changes/sync-scope-and-pwa-client/reports/ac1-complete-r1.md`
- AC1 日志：`reports/AC1.log`（`sha256:4c7e4513484070d48c16ed2797c3a1bdb78c5e693b2eaef66fb8520b03ba2cad`）
- PV1 日志：`reports/PV1.log`（`sha256:17900346451c11f5111cc2e5d5c56af0e8ecbceb97061f88fa35c07628a5f18b`）
- PV2 日志：`reports/PV2.log`（`sha256:d908e22bdf58f808ab44e083bf563db23c240d25afc7205454cb5f0e695e057f`）
- `CARGO_TARGET_DIR`：`D:\Project\acp-remote\.target-wt\mu2int`（复用既有，未新建）

## resource_cleanup

- 未启动任何长驻进程；`CARGO_TARGET_DIR=.target-wt/mu2int` 为既有目录（复用，未新建）。
- 反向对照期间对 `crates/core/src/broker.rs`、`crates/storage-sqlite/src/session_store.rs`、
  `crates/agent-host/src/mapper.rs` 的临时注入**已全部还原**（`git status --porcelain` 为空、唯一改动为测试文件）。
- 未 push、未 merge、未归档、未动 `feat/wp3`/`feat/wp4`/`refs/heads/main`。

### handoff_index

```agentic-handoff
version: 1
agent_context:
  agent_id: "CompleteAc1"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "4.1"
    work_package: MU2
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "3b6fe4136e71e5a8a9b36527440348d9be280ae0"
    evidence_type: DELIVERY
    evidence_id: ac1-complete-r1
    report_path: "reports/ac1-complete-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在接线提交 0cb5e62 之上补齐 AC1 缺的两项断言：file.changed（会话级）经 owned 路径落库并按 origin 序回放、会话标题在 session_info_update 后更新；连同既有节点级两项，AC1 六项断言实测全绿（reports/AC1.log，6 passed / 0 failed）。仅改 crates/app/tests/node_link_e2e.rs；对 crates/core/**、crates/agent-host/**、crates/storage-sqlite/**、crates/sync-protocol/**、schemas/**、docs/**、fixtures/** 零净改动（反向对照的临时注入已还原，git status 干净）。5 组反向对照逐条实测 FAIL。"
    source_evidence: NOT_APPLICABLE
  - task_id: "4.1"
    work_package: MU2
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "3b6fe4136e71e5a8a9b36527440348d9be280ae0"
    evidence_type: CHECK
    evidence_id: AC1
    report_path: "reports/ac1-complete-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo test --locked -p app --test node_link_e2e 在交付提交 3b6fe41 上 exit 0：6 passed / 0 failed（节点级两项 + file.changed 会话级落库与 origin 序回放 + 会话标题更新）。日志 reports/AC1.log。"
    source_evidence: reports/AC1.log
  - task_id: "4.1"
    work_package: MU2
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "3b6fe4136e71e5a8a9b36527440348d9be280ae0"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/ac1-complete-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "npm run verify 在交付提交 3b6fe41 上 exit 0：check 十道全绿 + check:rust 三条全绿（fmt/clippy/test）。日志 reports/PV1.log。"
    source_evidence: reports/PV1.log
  - task_id: "4.1"
    work_package: MU2
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "3b6fe4136e71e5a8a9b36527440348d9be280ae0"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "reports/ac1-complete-r1.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo test --locked -p core -p storage-sqlite -p agent-host --all-features 在交付提交 3b6fe41 上 exit 0：全 suite ok、0 failed。日志 reports/PV2.log。"
    source_evidence: reports/PV2.log
checks:
  - id: AC1
    work_package: MU2
    command: "cargo test --locked -p app --test node_link_e2e (cwd=.worktrees/integ, CARGO_TARGET_DIR=.target-wt/mu2int)"
    exit_code: 0
    log_path: reports/AC1.log
    result: PASS
  - id: PV1
    work_package: MU2
    command: "npm run verify (cwd=.worktrees/integ, CARGO_TARGET_DIR=.target-wt/mu2int)"
    exit_code: 0
    log_path: reports/PV1.log
    result: PASS
  - id: PV2
    work_package: MU2
    command: "cargo test --locked -p core -p storage-sqlite -p agent-host --all-features (cwd=.worktrees/integ, CARGO_TARGET_DIR=.target-wt/mu2int)"
    exit_code: 0
    log_path: reports/PV2.log
    result: PASS
test_delivery:
  MU2:
    kind: automated
    artifacts:
      - reports/AC1.log
      - reports/PV1.log
      - reports/PV2.log
    basic_checks:
      - AC1
      - PV1
      - PV2
```

---

## 交接要点（给 main）

1. **交付提交**：`3b6fe4136e71e5a8a9b36527440348d9be280ae0`（分支 `integ/mu2-wiring`，父 `0cb5e62`）。单文件、单提交，未混入其它交付。
2. **`review-wire-r1` F1 已按建议路径①闭合**（本提交内补两项断言，无需书面例外）；F2（不实注释）亦已改正。
3. **MU2 候选检查仍只含 PV1/PV2（不含 AC1）**：本次是**主动**补齐，不改变 `plan.md:971` 的门禁事实——若后续 MU2 候选仍不跑 AC1，需 reviewer/merger 显式执行 `cargo test -p app --test node_link_e2e` 方可声称 AC1 PASS。
4. **待复核**：由 main 调度**非作者** reviewer 复核本提交（重点：A2 的 origin 序判据是否绑定真实 SQL 排序、B4 的 `updatedAt` 来源判据是否可证伪、反向对照是否可复现），随后走 merger / premerge。本次**未**自审、**未**合入 main、**未**声称 MU2 闭环。
