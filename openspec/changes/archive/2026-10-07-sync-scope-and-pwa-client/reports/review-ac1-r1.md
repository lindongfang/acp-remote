<!-- AC1 补齐提交（MU2 收口，第 1 轮）独立检视（test-case）。Reviewer 只报告，不修改任何代码、测试、规划文件、任务状态或 verification.md；未运行任何编译/测试/E2E。 -->

task_id: "4.1"
role: reviewer
phase: test-case
agent_context:
  agent_id: "review-ac1-r1"
  isolation: "fork_turns=none（新建独立实例；未参与该补测的实现，未继承 WP3/WP4 实现者、接线实现者或任何 reviewer 的对话）"
base_revision: "0cb5e62b1941d97c685924fcbab87cae07c37d4f"
target_revision: "3b6fe4136e71e5a8a9b36527440348d9be280ae0"
scope: "仅 0cb5e62..3b6fe41 的 diff（1 file / +458 / −4：crates/app/tests/node_link_e2e.rs），外加检查是否引入回归。不重复接线轮 review-wire-r1 已覆盖的内容。"
changes: "只读检视，未修改任何文件（含代码、测试、plan/tasks/design/specs/verification.md），未切换分支、未提交、未合并、未运行任何编译/测试/E2E；仅新增本报告 review-ac1-r1.md。"
issues: "0 CRITICAL / 0 MAJOR / 0 MINOR（无 review-ac1-r1-F<n>）。列出 5 条「观察（不计为发现）」与未验证内容。"
result: PASS

## Review Context

| 项 | 取值 |
| --- | --- |
| Review ID / Round | `review-ac1-r1` / 1（该提交的首个检视轮） |
| Review Type / Stage | test-case（AC1 补齐提交，合入 MU2 候选之前） |
| Work Package | MU2 |
| Repository / Worktree | `D:\Project\acp-remote`；检视 worktree `D:\Project\acp-remote\.worktrees\integ`（HEAD `3b6fe4136e71e5a8a9b36527440348d9be280ae0` = target，`git status --porcelain` 为空，只读） |
| Base Revision | `0cb5e62b1941d97c685924fcbab87cae07c37d4f`（接线提交，`git rev-parse` 核对） |
| Target Revision | `3b6fe4136e71e5a8a9b36527440348d9be280ae0` |
| 实际修改文件 | 仅 `crates/app/tests/node_link_e2e.rs`（`git diff --numstat 0cb5e62 3b6fe41` → `458 4`，`--name-status` 仅一行 `M`）；`crates/core/**`、`crates/agent-host/**`、`crates/storage-sqlite/**`、`crates/sync-protocol/**`、`schemas/**`、`docs/**`、`fixtures/**` 零净改动；`Cargo.toml`/`Cargo.lock` 零改动（无新依赖） |
| 删除行 | 4 行：2 行不实注释（review-wire-r1 F2）、1 行 `ac1_config` 旧 doc、1 行 profile 的 `--scenario normal` 硬编码行（改写为参数化）；均为测试文件内部改写，无行为面删除 |
| 读取的规则与需求 | `plan.md`（AC1 行 :996、替代检查表 :1029、MU2 候选检查 :971、tasks 4.1 :70）、`specs/core-derived-events/spec.md`（R5/R6/R7/R9 四组 Requirement + 全部 Scenario）、`specs/workspace-resolution/spec.md`（越界口径）、`design.md`（D4 行级 diff、D5 路径相对化、D6 节点级、D7 标题与权威时间）、`openspec/schemas/agentic/roles/tester.md`（ID 稳定/映射/入口判据） |
| 实现面只读核对 | `crates/core/src/derive.rs`（`display_path` :303-352、`file_changed_view` :464-484、`file_change_event` :489-495、`TitleIntent` :523+）、`crates/core/src/broker.rs`（`commit_chunk` :2015-2170、`pump_locked`/`flush_locked` :1977-2012、R9 行为测试 :9337-9420）、`crates/agent-host/src/mapper.rs`（:203-214）、`crates/agent-host/src/bin/acpr-fake-acp-agent.rs`（`chunked-updates` :563-697）、`crates/storage-sqlite/src/session_store.rs`（`node_link_slice` :2454+、标题三态 UPDATE :1135-1185、`read_view` :2082-2088）、`crates/storage-sqlite/src/migrate.rs`（`owned_event` DDL/CHECK :85-115、`owned_session` :54-70）、`crates/app/src/compose.rs`（`assemble` :202-330、`close` :469-525、`forked_publisher` :978+）、`crates/server/src/node_link/resource.rs`（`publish` :1024-1048） |
| 验证证据（只读消费） | `reports/ac1-complete-r1.md`（作者报告，含 5 组反向对照）、`reports/AC1.log`、`reports/PV1.log`、`reports/PV2.log`、接线轮 `reports/wire-compose-nodeevents-r1.md`。**作者自评不作为检视证据**；reviewer 未执行任何编译/测试/E2E |
| 证据完整性核对（我实际执行） | `sha256sum` 与作者声明逐条一致：`PV1.log` = `17900346451c11f5111cc2e5d5c56af0e8ecbceb97061f88fa35c07628a5f18b`、`PV2.log` = `d908e22bdf58f808ab44e083bf563db23c240d25afc7205454cb5f0e695e057f`、`AC1.log` = `4c7e4513484070d48c16ed2797c3a1bdb78c5e693b2eaef66fb8520b03ba2cad`；`PV1.log:544-555` 含 `Running tests\node_link_e2e.rs` 与 `6 passed; 0 failed`，`AC1.log` 与主 Agent 复跑结论一致（6 passed / 0 failed） |

---

## 一、test-case 判据表逐项结论

| # | 判据 | 结论 | 依据 |
| --- | --- | --- | --- |
| 1 | **ID 稳定唯一** | **通过** | 文件共 6 个 `#[test]`，两条新增用例名 `ac1_the_composition_root_persists_file_changes_on_the_owned_path_in_origin_order`（:1494）、`ac1_the_composition_root_updates_the_session_title_from_the_agent_notification`（:1667）与既有 4 条互不冲突；名称是描述性常量、无编号，因此分片/阶段/负责人变化都不触发重编号（`tester.md` 判据：已有稳定 E2E ID 保留，新增用例分配唯一 ID）。两条新 ID 是**首次**出现，无复用。 |
| 2 | **需求映射** | **通过** | 新增两条分别映射 AC1 判据原文的两项（`plan.md:996` 与 `:1029`）：`file.changed`（会话级）经 owned 路径落库 + 按 origin 顺序回放；会话标题在 `session_info_update` 后更新。R5（类型化 Diff 派生，`derive.rs:489-495` 的 `tool.call.*` 三型）、R6（行级统计，D4）、R7（相对化/越界，D5）、R9（标题三态与权威时间，D7）中与这两条相关的正向分支都在断言里被触达（见 §二 A/B）。R5 的「重复派生」与 R5 的「不含 Diff 不派生」两个 Scenario、R9 的「显式置空」「只带 updatedAt」两个 Scenario 不在本入口——见 §三 C 与 §五 未验证内容，作者已如实登记，且这些分支由 PV2 范围内的 core/storage 用例覆盖。 |
| 3 | **入口真实（无 mock 绕过）** | **通过（有口径说明）** | 两条用例的驱动面是 `app::compose::Composition::assemble`（真实组合根，含真实 SQLite 迁移/连接池、真实 `agent-host` 后端与**真实 fake ACP 子进程**），事件从 ACP 原文经 `mapper` → `Broker::commit_chunk` 落盘，断言直接读**真实 SQLite 文件**（只读连接）或**真实读视图** `Broker::read_view()` → `node_link_slice`。无替身存储、无替身后端、无预置事件（与同文件其余三条 `support::owner::OwnerNode` 脚本化后端用例形成对照）。唯一的「非二进制入口」是：测试直接调 `Broker::pump`（:1424 起）代替 `daemon` 的合并窗口定时器，这与 `crates/app/src/daemon.rs` 的窗口做的是同一件事（core 不读时钟、不设定时器），且**不改变被测语义**；`plan.md:1017` 的 basis 已把本文件的既有做法列为「真实 Daemon 入口」的代表。据此判定不构成 mock 绕过（同样口径已被接线轮采纳）。**注意**：这里的「真实 Daemon 进程」是**进程内组合根**，不是 `daemon start` 二进制；这是本文件既有约定，非本 diff 引入，登记为口径而非缺陷。 |
| 4 | **前置可满足** | **通过** | 前置 = 临时数据目录（`support::TempRoot::new`，三条用例标签互异）+ fake ACP 二进制（`fake_acp_agent_binary()` 按 target 目录推断或读 `ACPR_FAKE_ACP_AGENT`，缺失即显式失败而非跳过）+ 场景 `chunked-updates`（`acpr-fake-acp-agent.rs:563-697` 确实在同一 prompt 内发类型化 `diff` 的 `tool_call` 与带 `title`/`updatedAt` 的 `session_info_update`）。我在源码中逐项核对了这三项前置在计划资源内可得。 |
| 5 | **断言可观察** | **通过** | 断言面全部是落盘/读面数据：`owned_event`（`session_id`/`session_sequence`/`origin_sequence`/`payload_json` 行数与字段值）、`owned_session`（`title`/`version`/`created_at`/`updated_at`）、`node_link_slice` 的事件序列与游标分页、`UseCases::list_sessions` 的 `SessionSummary::title()`。无私有字段、无内存结构、无 `Debug` 字符串匹配；`version > 1` 是 §5.2/§6 的合同口径（`title_write.rs` 头注释同源），非实现内部细节。 |
| 6 | **正常 + 异常（可捕获的反例）** | **通过** | 6 条新断言逐条有可捕获反例，且作者给出 5 组反向对照；我对每组做了**独立推导**（§二/§三），每组撤掉的实现点与失败断言一一对应，且注入点彼此独立（派生、SQL 排序、标题窄写入、时间来源、投影、事件行插入），不存在「一处注入连带触发别处检查」的情形。 |
| 7 | **无跳过** | **通过** | 文件内无 `#[ignore]`、无 `todo!`/`unimplemented!`、无 `if ... { return }` 形式的静默跳过；唯一的 `#[cfg(unix)]`（:996）属既有用例。前置缺失时 `fake_acp_agent_binary()` 断言失败而非跳过。 |
| 8 | **基础检查（语法/用例发现）** | **通过（只读复核）** | `AC1.log`：`running 6 tests` → `6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`；`PV1.log:544-555` 观察到同一 target 被执行且 6 passed。用例发现与编译均成立（我未自行编译，见 §五）。 |

---

## 二、A：两条新断言是否真的对应 AC1 判据

### A1–A5（`file.changed` 会话级 + origin 序回放）——**成立**

- **会话级（owned 路径）**：断言 ① 查 `owned_event` 中 `event_type='file.changed'` **恰 2 行**，并用 `rows.map(|(session, ..)| session.as_deref().expect("owned 路径：session_id 非空"))` 在**取值时强制非空**，再把两个 owner 排序后与 `[first, second]` 排序比对（:1512-1535）。两个会话各一次 prompt，因此「分属两会话 + `session_id` 非空」被同时钉住——这正是与节点级事件（`session_id IS NULL`，`owned_event` 的 CHECK 成对约束）的本质区别。**核对成立**：`ac1_prompt_once` 每次 `create_session` 用不同 `request_fingerprint`（`ac1-session-file-a` / `file-b`）与不同 `session`，`Broker` 按会话分 slot，去重日志 `slot.file_changes` 是**按会话**的（`broker.rs` 的 `Slot::file_changes`），因此两会话各派生 1 条；跨会话不会互相去重。另外断言了 `session_sequence`/`origin_sequence` 均非空（:1542-1545）。
- **至少两条事件**：整个 origin 序列（会话 A）被要求 `seqs == 1..=N` 且 `seqs.len() >= 4`（:1578-1583），实测 N=19（见 RC-A2 记录）。这是「顺序」可判定的必要前提，成立。
- **顺序是否真被验证**：`node_link_slice` 的 SQL 由存储层固定为 `... AND compacted_into IS NULL ORDER BY origin_sequence ASC LIMIT ?3`（`session_store.rs:2480-2489`，我实读到该语句），因此 `seqs == 1..=N` 的断言直接绑定**真实 SQL 的排序结果**；全集通过 `Broker::read_view()` 取（`broker.rs:1863-1865` 转发到 `store.read_view()`），不是测试侧自行排序。再叠加「`file.changed` 必须落在序列内部（`position > 0 && position+1 < len`）」与「紧邻 `tool.call.*`」两条（:1585-1599），能抓住「派生后追加在批尾」这一类错误实现。
- **游标续读**：`after = change_seq − 1` 的第一条必须**就是**该 `file.changed`；`after = change_seq` 起不得再出现它，且余下条数必须恰为 `N − change_seq`（:1601-1643）。这两个方向（前一条定位 + 后一条排除）是对「按 origin 定位」的正反两侧检查，非单侧空转。
- **RC-A2 是否覆盖顺序部分——成立**：注入把 `ORDER BY origin_sequence ASC` 改成 `DESC`，实测序列变为 `[19..1]` 并使 :1581（`assert_eq!(seqs, 1..=N)`）失败。即断言**不是**只查「有这几行」；把 ASC→DESC 直接翻红，说明顺序维度被真实绑定到 SQL 排序。**附带**：该注入同时会让 :1592 的「内部位置」与 :1618 的游标定位失败，属同一根因的连带红，不影响「顺序被独立覆盖」的结论。
- **A3/A4/A5（视图字段）**：`kind == "modified"`、`displayPath == "src/main.rs"`、`addedLines == "1"`、`deletedLines == "1"`、`outsideWorkspace` 键缺席（:1547-1555）。字段取值与 `derive.rs` 的 `file_changed_view`（:464-484，`stats` 为 `None` 时整键缺席、`outside` 为真时才写 `outsideWorkspace`）及 D4 的 Myers 口径一致；`"let a = 1;" → "let a = 2;"` 的行级结果为 1 增 1 删，逻辑自洽。

### B1–B5（标题经 `session_info_update` 更新）——**成立**

- **标题取值被断言**：`title == Some("会话标题")`（:1693-1697），且创建时为空由 `create_session` 恒写 `title: None`（`broker.rs:1528-1531` 的 `NewSession { title: None, .. }`）保证，因此「空 → 通知取值」是真实的状态迁移。
- **`session.info.changed` 事件存在且带 `session_id`**：恰好 1 行、`session_id == 会话`、`view.title == "会话标题"`（:1705-1727）。
- **`updatedAt` 是否真区分两个来源**：断言分三层——(a) **事件 view** 的 `updatedAt` **必须等于** Agent 自报的 `2026-09-24T10:00:00.000Z`（:1729-1731），(b) **会话行**的 `updated_at` **必须不等于**该字面量（:1734-1737），(c) `updated_at >= created_at`（:1738-1741）。关键在于 (a)+(b) 是**同一用例内对同一字面量的双向断言**：若实现把会话权威时间也改成 Agent 自报值，(b) 立即翻红（RC-B2 实测正是 `left: "2026-09-24T10:00:00.000Z"`）；若实现停止转发 Agent 值到 view，(a) 翻红。因此「只是不等于某个字面量」的过弱风险被 (a) 抵消——(a) 证明该字面量确实来自 Agent 通知，(b) 证明会话行未采用它。时钟来源核对：`mapper.rs:203-214` 事件 view 用 `info.updated_at`（否则 `clock.now()`）；`session_store.rs` 的会话 `updated_at = ?3` 绑的是 `commit.at`（Daemon 时间，`broker.rs:2021` 的 `self.now()`）。两个来源在实现上确实不同，(a)/(b) 的组合可判真。
- **标题单向（无重命名命令路径）**：本用例不构造任何重命名命令；标题进入的唯一通道是 `session.info.changed`（`broker.rs:2145-2156` 的窄写入），且 R9 的「无重命名入口」由 `broker.rs:9420+` 的既有行为测试锁定。本用例的贡献是「该通道在真实组合根上确实生效」，成立。
- **读面一致**：`list_sessions` 的 `SessionSummary::title()` 反映该标题（:1745-1760），覆盖目录页读面。

---

## 三、B：五组反向对照的可信度（逐组独立推导）

方法核对：作者称在交付提交同一工作树上注入 `crates/core/**`、`crates/storage-sqlite/**`、`crates/agent-host/**` 后逐一还原，并以 `git diff --stat 0cb5e62 3b6fe41`（1 file）与空 `git status --porcelain` 自证。我在检视 worktree 独立复核：`git diff --numstat 0cb5e62 3b6fe41` = `458 4` 单文件、`git status --porcelain`（integ worktree）为空 —— **还原事实成立**。

| 组 | 撤掉的东西 | 对应断言 | 失败是否由该断言触发 | 判定 |
| --- | --- | --- | --- | --- |
| RC-A1 | `broker.rs` 的 `file.changed` 派生块置为不可达 | A1（行数） | 是：`left: 0 / right: 2`，点出的正是「恰 2 行」断言。撤回派生后**不可能**有别的断言先失败（该会话的 origin 序列仍在，只是没有 `file.changed` 行；游标断言在 A1 之后才执行） | **可信** |
| RC-A2 | `session_store.rs` 的 `ORDER BY ... ASC` → `DESC` | A2（顺序） | 是：`[19..1]` vs `1..19`，直接命中 `seqs == 1..=N`。该注入**只**动排序，派生与落库不变，因此失败唯一归因于顺序 | **可信**，且证明 A2 有独立判别力 |
| RC-B | `broker.rs:2145` 的标题窄写入置为不可达 | B1（标题值） | 是：`left: None / right: Some("会话标题")`。事件仍投影、`session.info.changed` 仍落库，因此 B3 不会连带红——失败唯一归因于标题未写入 | **可信** |
| RC-B2 | `session_store.rs` 的 `updated_at = ?3` → 字面量 `2026-09-24T10:00:00.000Z` | B4（权威时间来源） | 是：`left == right` 于 `assert_ne!`。只动时间列，标题与事件行不变，失败唯一归因于时间来源 | **可信**，且正好证伪「(b) 只是不等字面量」的取巧读法 |
| RC-C | `agent-host/mapper.rs` 的 `session.info.changed` 投影置为不可达 | B1（源头） | 是：`left: None`。事件源头被撤 → 标题不再更新，同时 `session.info.changed` 行也不会落库（因此会**连带**触发 B3）——作者如实记录为「B1 先行 FAIL」，与执行顺序一致（B1 在 B3 之前） | **可信** |
| RC-C2 | 保留 mapper，改在存储事件插入循环对 `session.info.changed` 行 `continue`（**标题窄写入照写**） | B3（事件存在性） | 是：`left: 0 / right: 1`。此时标题**仍被写入**，B1 会通过，因此失败唯一归因于「事件行未落库」 | **可信** |

**RC-C / RC-C2 的两步拆分——必要且正确**：若只有 RC-C，则 B1 与 B3 会同时红，无法证明 B3 这条**行数**断言有独立判别力（B3 可能是被标题失败「顺带」带红的）。RC-C2 构造了「标题写入保持不变、只丢事件行」的注入，把 B3 隔离出来单独翻红，正是 `test-case` 判据 6「每条断言要有能捕获的反例」所要求的粒度。两步各自命中断言 B 的不同部分（B1 的标题通道 vs B3 的事件存在性），拆分方向正确。

**五组注入点的独立性核对**：RC-A1（core 派生）、RC-A2（存储排序）、RC-B（core 标题窄写）、RC-B2（存储时间列）、RC-C（适配器投影）、RC-C2（存储事件插入）——六个注入点分布于 3 个 crate 的 5 个不同代码位置，无重叠；每组撤掉的东西都能对应到**具体某条**断言，且失败点与其它断言的可达性分析一致。**未发现「撤掉的东西并不对应某条断言」或「失败由别的检查连带触发」的情形。**

**局限（如实声明）**：以上是对作者反向对照记录的**静态复核**（注入点存在性、失败断言的可归因性、还原自证），我**未重跑**这 5 组注入。主 Agent 的独立复跑仅覆盖无注入的 6 passed。

---

## 四、C：作者自报的未验证项是否可接受（逐条）

| 未覆盖项 | 是否属本次该覆盖 | 结论 |
| --- | --- | --- |
| **通知「显式置空」** | 否（本次） | AC1 判据原文（`plan.md:996`/`:1029`）只要求「会话标题在 `session_info_update` 后更新」，未点名置空分支。置空属 R9 的另一个 Scenario（`specs/core-derived-events/spec.md`「通知清空标题时呈现为未命名」），其可满足性由 `crates/storage-sqlite/tests/title_write.rs`（三态 UPDATE 的版本/时间/同批列不丢）与 `crates/core/src/broker.rs:9403-9420` 的 R9 行为测试覆盖，且**这两处都在 PV2 范围内**。WP3 本轮刚修的「窄 UPDATE 丢 `version`/`updated_at`」在该文件有直接断言（`title_write.rs:1-11` 的判别力说明 + 事件版本注入断言），因此**不需要** AC1 再覆盖一次；AC1 若覆盖它，反而要求改 `crates/agent-host/**` 的 fake 场景（超出本任务边界，作者亦如此说明）。接受。 |
| **通知「只带 updatedAt」** | 否（本次） | 同上，属 R9 部分更新语义，由 `broker.rs:9360-9385` 的「缺 title 的通知不得清空标题」覆盖，在 PV2 内。接受。 |
| **越界路径（`outsideWorkspace=true` + 只下发文件名）** | 否 | AC1 判据原文**不含**越界断言；越界口径属 `specs/workspace-resolution/spec.md` 的 R7 Scenario 组，按 `plan.md` 的包划分属 TP2（`tasks.md:4.7` 明列「同名前缀误判防护」）与 TP1 的固定向量，并由 `core` 的 `derive.rs:713-735` 单测覆盖。接受。 |
| **`addedLines`/`deletedLines` 的「判定不出时省略」分支** | 否 | 同上属 R6 的第三个 Scenario，归 TP2/PV2（`tasks.md:4.7` 明列「判定不出时省略」）。本用例已覆盖正向（能判定）一侧，方向正确。接受。 |
| **AC1 广播** | 否 | `plan.md:996` 明写「**不含广播断言**：节点级事件当前无投递通道」，与 `design.md` D6 一致。接受。 |
| **CI-only job（`deps`/`advisories`/`secrets`）** | 否 | `plan.md` 的 Merge Conditions 已明确本地无等价物、不得声称通过。接受。 |
| **真实 Agent（Codex/OMP）对拍** | 否 | AC1 环境列即「fake ACP Agent」，与计划一致。接受。 |

**「显式置空」单独追问的回答**：AC1 **不应该**覆盖它。① 判据原文没有它；② 它有专门且更精确的覆盖面（存储层三态 SQL + core R9 行为测试），且这两处是本轮刚修的缺陷所在的直接回归点；③ 要在 AC1 里驱动它必须改 `crates/agent-host/**` 的 fake 场景，超出本提交边界。把边界外的东西塞进 AC1 只会引入「靠改被测面来喂场景」的坏味道。

---

## 五、D：门禁盲区的确认

**作者的描述——准确，但需要一处精确化。**

- **准确的部分**：`plan.md:971` 的 MU2 行 Candidate Checks 列**确实仍为 `PV1, PV2`**（我逐字核对该行），AC1 作为**命名检查**（`plan.md:996` 的 AC1 行、`:1029` 的替代检查表）不在 MU2 候选检查内；因此「若候选只按 PV1/PV2 判定，不会有任何门禁要求存在一份绑定 target_revision 的 `reports/AC1.log`」——这个**证据绑定盲区**属实，本轮补齐属主动行为而非门禁驱动。
- **需要精确化的部分**：**执行面**并非完全漏网——`PV1` = `npm run verify` → `check:rust` = `cargo fmt --all -- --check && cargo clippy … && cargo test --locked --workspace --all-features`（仓库根 `package.json:13,25`），workspace 含 `crates/app`，因此 `node_link_e2e` 会被 PV1 执行。实测证据：`reports/PV1.log:544-555` 有 `Running tests\node_link_e2e.rs` 与 `6 passed`。所以准确表述是：**「AC1 的判据不构成任何门禁的判定输入」**（布线前 4 条断言全绿也能让 PV1 通过，这正是 review-wire-r1 F1 的实质），而**不是**「该文件不在任何门禁里执行」。这一区分影响后续风险评估：本轮之后**再删/弱化** AC1 断言仍不会被 PV1/PV2 的**判定标签**捕获（只有 `cargo test` 失败才会），因此 reviewer/merger 仍应显式跑一次 AC1 并绑定 target 版本。
- **本轮之后 AC1 是否还有别的漏网路径**（我独立枚举）：
  1. **命名证据的版本绑定**：`reports/AC1.log` 由作者在其 worktree 产出；若合并/rebase 后候选提交与 `3b6fe41` 不同，AC1.log 不自动失效也不自动重跑（PV1 重跑能覆盖执行面，但不能产生「AC1 = PASS」这一标签）。→ **仍存在**，需 merger/reviewer 显式重跑并在 verification 的 `## Checks` 记 target 版本。
  2. **干净 target 目录下的前置**：AC1 的命令是 `cargo test --locked -p app --test node_link_e2e`。`-p app` **不会**构建 `agent-host` 的 `acpr-fake-acp-agent` bin；`fake_acp_agent_binary()` 在缺失时**显式失败**（不跳过）。因此严格单独执行 AC1（无 PV1/PV2 先行的 target 目录、未设 `ACPR_FAKE_ACP_AGENT`）会红。这是**既有** helper 的性质（接线轮引入，不在本 diff 内），但读者应知道 AC1 的真实前置是「同 target 目录已有 fake agent 二进制」。
  3. **IV1**：最终固定版本复核会再次执行 `npm run verify` + 全包测试，届时覆盖执行面；但 IV1 的关注点是覆盖充分性，不是 AC1 标签。

---

## 六、发现

**无。** 本提交未发现满足全部判据（可证影响、可行动、非有意设计、由本 patch 引入、无未声明假设、力度相称）的问题；**不存在 CRITICAL/MAJOR/MINOR 级别的发现**，故无 `review-ac1-r1-F<n>` 条目。

### 观察（不计为发现；均不满足「可证影响」或属既有面）

1. **`updated_at != 字面量` 单看偏弱，但被同用例的 `event_view.updatedAt == 字面量` 补强**——两者合起来可判真（§二 B）。若单独抽走 view 侧的相等断言，判别力会下降。建议（非必需）：未来可将「Daemon 持久化时间」再与 `created_at`/测试开始时刻做区间断言。
2. **`outsideWorkspace` 缺席断言**只覆盖工作区内一侧（`get(...).is_none()`）；工作区外一侧由 core 单测与 TP 向量覆盖，AC1 不必重复。
3. **测试 A 未显式 `drop(read)` 就调用 `Composition::close()`**，与存储层测试的既有惯例（`commit.rs`/`retention.rs`/`contract_v03.rs`/`compaction_recovery.rs` 均 `drop(view); store.close()`，并注明否则 `close()` 会等一个不结束的连接）不同。作者实测与主 Agent 复跑均在 6.39 s 内完成 6 passed，说明当前 sqlx 0.8.6 + 该配置下未阻塞；**我无法从静态阅读证伪「不会阻塞」，也无法证明「会阻塞」**，按判据不予立为发现。仅登记为可读性/健壮性观察：显式 `drop(read)` 会更贴合仓库既有惯例。
4. **两条新用例的报告行号与实际断言行存在 ±1～2 的偏移**（如 RC-A1 记 `:1518`，对应断言在 `:1519`；RC-A2 记 `:1581`，对应断言在 `:1579`），可解释为反向对照期间的编辑/格式化时序差异。作者自评不作为检视证据，故不据此立论；此处仅提示复核者以断言内容（而非行号）对拍。
5. **「真实 Daemon 进程」的实现口径**是**进程内组合根 + 真实子进程 + 真实 SQLite**，非 `daemon start` 二进制；与同文件既有用例一致，`plan.md:1017` 的 basis 已把它列为该文件的做法。登记为口径说明。

---

## 七、实际检查范围

- **diff**：`git diff --numstat/--name-status 0cb5e62 3b6fe41`（单文件 +458/−4）、`git show 0cb5e62:...` 对拍父版本行号、`--name-status` 确认无其它文件；检视 worktree `git status --porcelain` 为空、`HEAD == 3b6fe41`。
- **新增代码全文**：`crates/app/tests/node_link_e2e.rs:1055-1062`（注释改写）、`:1123-1150`（`ac1_config` 参数化 + `persist_deltas`）、`:1326-1767`（两条新用例、4 个新 helper、`FileChangedRow` 别名）。
- **被测实现只读核对**（用于判断断言是否绑定真实行为、反例是否可达）：`core::derive` 的 `display_path`/`file_changed_view`/`file_change_event`/`TitleIntent`；`core::broker` 的 `commit_chunk`（派生点、标题窄写入、状态合批）、`pump_locked`/`flush_locked`、R9 行为测试、`Slot::file_changes` 去重语义；`agent-host` 的 `mapper.rs` 投影与 fake agent 的 `chunked-updates` 场景；`storage-sqlite` 的 `node_link_slice` SQL、标题三态 UPDATE、`read_view`、`owned_event`/`owned_session` DDL 与 CHECK；`app::compose` 的 `assemble`/`close`/`forked_publisher`；`server::node_link::resource::publish` 的节点级丢弃点。
- **证据链只读消费**：`AC1.log`（6 passed）、`PV1.log`（含 `node_link_e2e` 的 6 passed 段与末尾 exit 0）、`PV2.log`；三份日志 `sha256` 与作者声明逐条一致。
- **需求/决策文本**：`plan.md`（:70/:971/:996/:1017/:1029）、`specs/core-derived-events/spec.md` 全文、`specs/workspace-resolution/spec.md`、`design.md`（D4/D5/D6/D7）、`tasks.md`（4.1/4.7）、`openspec/schemas/agentic/roles/tester.md`。
- **回归面**：唯一被改写的既有测试面是 `ac1_config`（现含 `[storage] persist_deltas = true`）。核对：节点级用例（`ac1_config` 的唯一既有调用点 `:1185`）只创建会话、不 prompt，因此不受 delta 压缩策略影响；其断言仍全绿（PV1/AC1 日志）。`ac1_config` 的语义扩展（scenario 参数化）对既有调用点默认 `normal`，与原行为等价。

## 八、未验证内容（明确列出）

- **未运行任何编译、测试、E2E、fmt/clippy**。`AC1.log`/`PV1.log`/`PV2.log` 的通过结论来自作者与主 Agent 的执行，我只做了**只读消费**与哈希一致性核对；本报告的「通过」是**静态审查结论**，不宣称实际运行通过。
- **未重跑 5 组反向对照**（RC-A1/A2/B/B2/C/C2）。§三的判定是对注入点与失败归因的静态复核。
- **未验证**：cache/浮点无关；未验证性能与并发压力下 `pump` 轮询（50 ms × 20 s 上限）在三会话以上的表现；未验证非 Windows 平台下 `fake_acp_agent_binary()` 与路径相对化的行为（本检视为 win32，`display_path` 的分支未在真实运行中观察）。
- **未验证**：`Composition::close()` 在**持有只读视图**时的阻塞语义（§六观察 3）——我既未运行也未能在源码层证伪，因此不构成发现也不构成通过依据。
- **未验证**：R9 的「显式置空」「只带 updatedAt」两态与 R6「判定不出时省略」分支在真实子进程路径上的行为（作者已如实登记，其可满足性来自 PV2 范围的 core/storage 用例，我未执行那些用例）。
- **未验证**：`MU2` 候选的最终合并提交（本轮对象是 `3b6fe41`，不是合并结果）；rebase 后是否需要重跑 AC1 属 merger 职责。

---

## 九、结论

**PASS。** 本提交按 AC1 的判据原文补齐了两项缺失断言，用例 ID 唯一稳定、需求映射正确、入口为真实组合根 + 真实 SQLite + 真实 fake ACP 子进程（无 mock 绕过）、前置在计划资源内可得、断言绑定真实落盘与真实读视图、每条断言都有可捕获的反例（5 组反向对照在静态复核下逐组可信，RC-C/RC-C2 的两步拆分必要且正确）、无跳过、基础检查通过；未引入回归（唯一既有面改写 `ac1_config` 对既有用例语义等价）。作者自报的未覆盖项均属 AC1 判据之外、且各有更精确的覆盖面（PV2 范围），不构成本轮缺口。

**无 CRITICAL/MAJOR 发现**，故不触发 FAIL。

**给 merger / 后续轮次的要点**：
1. AC1 未进入 `plan.md:971` 的 MU2 候选检查列这一事实**不变**；合入前需 reviewer/merger **显式**执行 `cargo test --locked -p app --test node_link_e2e` 并在 verification 的 `## Checks` 绑定**候选提交**版本，方可声称 AC1 PASS（PV1 会执行该 target，但不产生 AC1 这一判定标签）。
2. 执行 AC1 的命令需要同 target 目录下已有 `acpr-fake-acp-agent`（先跑 PV1/PV2，或设 `ACPR_FAKE_ACP_AGENT`），否则前置断言会如实失败。
