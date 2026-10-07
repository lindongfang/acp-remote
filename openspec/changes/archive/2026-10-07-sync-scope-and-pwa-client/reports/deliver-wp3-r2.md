<!-- WP3 修复轮次（Attempt 2）交付报告。coder 只交付修复、用例与检查证据；不判独立 review、合入、E2E 与最终验收。 -->

# WP3 修复轮次交付（deliver-wp3-r2）

## Shared Report

- **task_id**: 2.3（`phase: fix`，DELIVERY）/ 3.5（CHECK：PV1 / PV2）
- **work_package**: WP3（交付单元 MU2）
- **role**: coder（`coder-w3-r2`，Attempt 2）
- **phase**: fix（本轮只修复 `review-w3-r1` 的 F1–F4，不重写已通过检视的实现）
- **agent_context**: `agent_id: coder-w3-r2`，`isolation: fork_turns=none`（新实例接管既有 worktree `.worktrees/wp3`；未继承 `coder-w3-r1` 的对话，只带原提交与检视报告）
- **target_revision**: `6bf7a7c3e88c84c06d4112204c4d3cd52a7cfab1`（分支 `feat/wp3`，父提交 = 第 1 轮交付 `0f70c4c2ddc41d21ba86abb0c6e83ef3da11ddc1`）
- **scope**: 只在 `.worktrees/wp3` 内改动 `crates/core/src/`、`crates/storage-sqlite/src/`、`crates/storage-sqlite/tests/`（新增 1 文件）与 `docs/CORE_PORTS_AND_STORAGE.md`；未触碰 `crates/core/Cargo.toml`、`crates/sync-protocol/**`、`schemas/**`、`fixtures/**`、`crates/storage-sqlite/src/migrate.rs`、`crates/core/src/ports.rs`、`openspec/changes/**`（本报告除外）
- **result**: **PASS**（PV1 / PV2 exit 0；F1–F4 逐条修复并各带可判别用例）

### 交付提交

| 项 | 取值 |
| --- | --- |
| worktree | `D:\Project\acp-remote\.worktrees\wp3` |
| 分支 | `feat/wp3` |
| 第 1 轮交付（base） | `0f70c4c2ddc41d21ba86abb0c6e83ef3da11ddc1` |
| 本轮交付（target） | `6bf7a7c3e88c84c06d4112204c4d3cd52a7cfab1` |
| 提交信息 | `fix(sync): WP3 修复轮次——标题置空走通用路径、去重键取原始路径、空 agentId 拒绝` |
| `CARGO_TARGET_DIR` | `D:\Project\acp-remote\.target-wt\wp3`（独占） |

`git diff --stat 0f70c4c..6bf7a7c`（5 files changed, **522 insertions(+), 23 deletions(-)**）：

```
 crates/core/src/broker.rs                  |  69 +++++-
 crates/core/src/derive.rs                  |  61 ++++-
 crates/storage-sqlite/src/session_store.rs |  23 +-
 crates/storage-sqlite/tests/title_write.rs | 388 +++++++++++++++++++++++++++++  (新文件)
 docs/CORE_PORTS_AND_STORAGE.md             |   4 +-
```

---

## 1. F1（MAJOR）显式置空标题丢掉版本递增与更新时间

### 根因

第 1 轮在 `session_store.rs` 里把「显式置空」实现成**替换整条语句**：

```rust
let query = match &update.title {
    Some(None) => sqlx::query_scalar::<_, i64>(
        "UPDATE owned_session SET title = NULL WHERE session_id = ?1 RETURNING version",
    )
    .bind(session.as_str()),
    _ => query.bind(update.title.clone().flatten()),
};
```

该窄语句不含 `version = version + 1`，也不含 `updated_at = ?3`，并且**整个替换**按 `update.mode` 选出的通用
`CASE WHEN` 语句——同一提交里的 `state`/`closed_at`/`current_mode_*`/`agent_session_id`/`workspace_cwd`/
`workspace_alias` 被静默丢弃。

### 修法（不弱化任何检测，不动 broker）

标题的**两层可选**在 SQL 里表达为「值参数 + 哨兵参数」两个绑定，与其余列共用**同一条**语句：

```sql
title = CASE WHEN ?9 THEN NULL WHEN ?8 IS NULL THEN title ELSE ?8 END, \
version = version + 1, updated_at = ?3 \
WHERE session_id = ?4 RETURNING version
```

`ModeChange::Set` 分支同形（`?11` / `?10`）。绑定按**参数编号顺序**追加（`?N` 与绑定位置一一对应；
先绑 `?8` 的值、再绑 `?9` 的哨兵）：

```rust
let clear_title = i64::from(matches!(update.title, Some(None)));
let query = query.bind(update.title.clone().flatten()).bind(clear_title);
```

三态的真值表（`?9` = 是否显式置空，`?8` = 新标题值）：

| `update.title` | `?9` | `?8` | `CASE` 结果 |
| --- | --- | --- | --- |
| `None`（不改该列） | 0 | NULL | `title`（原值） |
| `Some(None)`（显式置空） | 1 | NULL | `NULL` |
| `Some(Some(t))`（写入） | 0 | `t` | `t` |

因此置空路径与 SET 路径的 `version`/`updated_at` 语义**完全一致**，且同批其它列一律照写；core 的
`commit_owned` 版本漂移检测（`broker.rs:3362-3368`）不再被触发，同批的 `session.mode.changed` 不会因
`PortError::Corrupt` 与 `mem::take` 一起丢失。**未**改动 broker 的任何检测逻辑。

### 新增用例（`crates/storage-sqlite/tests/title_write.rs`，4 条）

| 用例 | 断言 |
| --- | --- |
| `clearing_a_title_bumps_the_version_and_keeps_the_rest_of_the_batch` | **F1 主用例**：同一批次含「显式置空」+ 已注入 `version="2"` 的 `session.mode.changed`；断言不返回 `Corrupt`、`CommitOutcome.version == 2`、落盘 `version == 2`、`updated_at == commit.at`、`title IS NULL`、同批 `state`/`current_mode_id`/`current_mode_name`/`workspace_alias` 全部落盘、事件已落盘且 view 注入值与返回版本一致 |
| `clearing_a_title_with_a_mode_change_writes_both_columns` | `ModeChange::Set` 分支下置空 + 模式列同时可见（钉住 `?10`/`?11` 的编号绑定） |
| `setting_a_title_writes_it_and_bumps_the_version` | `Some(Some(t))` 写入路径的版本 / 时间 / 同批其它列 |
| `an_unchanged_title_keeps_the_existing_value_and_still_bumps_the_version` | `None` = 不改该列（两层可选的另一半） |

**判别力实证**：把这 3 个源文件临时 `git stash` 回 `0f70c4c` 的 `session_store.rs` 后重跑该文件，
`clearing_a_title_bumps_the_version_and_keeps_the_rest_of_the_batch` 与
`clearing_a_title_with_a_mode_change_writes_both_columns` **在版本断言处失败**（`left: Version(1)`、
`right: Version(2)`）；恢复修复后 4/4 通过。**未**通过放宽断言、删用例或弱化检测消除失败。

---

## 2. F2（MINOR）去重键取自已收窄的展示路径

`DerivedFileChange` 新增**不参与投影**的字段 `origin_key`（工具调用给出的原始 `path` 原文，
在 `display_path()` 收窄**之前**的取值）；`dedup_key()` 与 `change_id()` 改用它：

- `dedup_key()` = `format!("{}\u{1}{}", self.tool_call_id, self.origin_key)`
- `change_id()` = `canonical_uuid(session \u{1} tool_call_id \u{1} origin_key)`
- `file_changed_view()` **一行未改**：越界仍只下发 `file_name()`，原始路径不进任何 view 键。

新增用例：

| 层次 | 用例 | 断言 |
| --- | --- | --- |
| `derive.rs` 单测 | `derived_changes_distinguish_same_named_outside_files` | 两个同名越界文件（`/etc/nginx/nginx.conf`、`/tmp/nginx.conf`）都派生、`display_path` 都是 `nginx.conf`、`dedup_key()` 与 `change_id()` 两两不同、`file_changed_view` 仍只含 `displayPath: nginx.conf` 且不含原始路径片段 |
| `broker.rs` 端到端 | `two_same_named_outside_files_derive_two_distinct_changes` | 真实 `commit_chunk` 路径上落盘**恰好 2 条** `file.changed`、两条 `changeId` 不同 |

**判别力实证**：临时回退 `derive.rs` 后重跑端到端用例，实测只有 **1** 条
（`assert_eq!(changes.len(), 2)` 失败，输出中仅一条 `displayPath: nginx.conf`）；恢复后通过。

---

## 3. F3（MINOR）空字符串 `agentId` 通过节点级事件校验

`crates/core/src/broker.rs` 的 `commit_node_event` 改为按**形状**判定：

```rust
if agent_id.as_deref().is_none_or(str::is_empty) {
```

`derive::node_event_parts` 对 JSON `""` 返回 `Some("")`，旧判据 `agent_id.is_none()` 因此放行空串，
会落一条 `agentId` 违反 schema `minLength: 1`（`schemas/sync/v1/event-views.schema.json#/$defs/agent.connected`）
的事件，消费端 `sync-protocol` 的 `NonEmptyText<128>` 解析失败。

用例：既有 `node_level_events_fail_closed_on_shape_violations` 新增一条
`{"agentId":"","state":"connected"}` → `Agent.connected` 必须 `Err` 的断言（与既有「缺 agentId」「state
词表不符」「类别不是 state」并列）。

---

## 4. F4（MINOR）`§5.2` 的 `[决定]` 未随新增标题字段更新

`docs/CORE_PORTS_AND_STORAGE.md`：

1. `:394`（§5.2 的唯一那条 `[决定]`）末尾追加 `title` 的两层可选语义：`Option<Option<String>>`，
   `None` = 不改该列、`Some(None)` = **显式置空**（写 `NULL`）、`Some(Some(text))` = 写入（≤512 字符）；
   明确指出「单层口径覆盖不了置空这第三态」，并说明存储层因此需要**哨兵参数**、且置空与其余列共用同一条
   `UPDATE`（因此版本递增与 `updated_at` 对两条路径一致），语义/来源/失败关闭口径指向 §6 第 21 条。
2. `:883`（§6 第 21 条的 `file.changed` 派生源条目）把去重键口径从「`(toolCallId, 展示路径)`」改为
   「`(toolCallId, 工具调用给出的原始 `path` 原文)`」，并写明**不是**展示路径的理由（越界收窄是有损的、
   同名越界文件会碰撞），与 F2 的实现同步。该行是 F2 的直接文档面——reviewer 在 `review-w3-r1` 的 §2.D
   已指出这两段正文不进门禁（`check:drift` 的 §5 ```rust 块不含 `SessionUpdate`），故一并补齐。

---

## 5. 未改动项（复核已通过、按要求保持原样）

行级 Myers diff 与省略口径、规范化前缀判定与越界收窄、节点级事件三条 DDL CHECK 自洽（零新增 migration）、
ACP 原文三要素保真——四项一行未动。`crates/core/src/ports.rs` 的 `SessionUpdate` 形状未动（F1 只改存储层与
broker 的**校验**，不改 `ports.rs`）。`crates/storage-sqlite/tests/` 的既有 19 处 `title: None,` 未动。

---

## Checks（实际命令 / 目录 / 环境 / 退出码 / 子检查 / 日志）

| ID | 命令 | 工作目录 | 环境 | 退出码 | 子检查 |
| --- | --- | --- | --- | --- | --- |
| **PV1** | `npm run verify` | `.worktrees\wp3` | `CARGO_TARGET_DIR=D:\Project\acp-remote\.target-wt\wp3`；Node v22.22.0 / npm 10.9.4；rust-toolchain 1.98.1 | **0** | 见下 |
| **PV2** | `cargo test --locked -p core -p storage-sqlite -p agent-host --all-features` | `.worktrees\wp3` | 同上；fake ACP Agent，无外部服务 | **0** | 全部套件 0 failed |

**Local Checks**：`cargo fmt --all`（exit 0）；`cargo clippy --locked -p core -p storage-sqlite --all-targets -- -D warnings`（exit 0，零 warning）。

### PV1 子检查逐条（`reports/PV1-wp3-r2.log`）

- `check:schemas` → `schema fixtures OK: 149 valid, 51 invalid (ajv Draft 2020-12), 41 event views bound`
- `check:commands` → `command catalog OK: 13 commands`
- `check:errors` → `error registry OK: 58 codes across 2 protocols`
- `check:features` → `feature registry OK: 13 feature ids across 2 protocols`
- `check:assets` → `contract assets OK: 17 schemas, 213 fixture files, 12 transcript vectors …`
- `check:acp` → `ACP compatibility matrix OK: 25 methods, 11 updates, …`
- `check:docs` → `doc links OK: 415 relative links, 9058 section refs across 518 markdown files`（section refs 9005 → 9058，本次文档改动后 `check:docs` 仍绿）
- `check:boundaries` → `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致`
- `check:drift` → `contract drift OK: §7 的 36 条 DDL … §5 的 15 个 trait / 96 个方法签名与 crates\core\src\ports.rs 一致`
- `check:agentic` → worktree 内原不可执行（`node_modules/` 不随 worktree 提交）；本轮在 worktree 内建立指向主检出
  `node_modules/` 的目录联接（junction，只读共享，未写任何包内容），因此该步**在 worktree 内实跑**：
  `doctor` 六项 PASS（toolchain / openspec / config / schema / verification skill / AGENTS.md / manifest）+ `openspec validate --all --strict` **21 passed, 0 failed** + 宿主入口 17 个文件。
- `check:rust` → `cargo fmt --all -- --check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` + `cargo test --locked --workspace --all-features`；**最终串行运行 exit 0**（fmt/clippy 通过，workspace 全部套件 0 failed，含 `daemon_lifecycle` 12/12）。此前若干次运行命中下述**已知非确定性 flake**。

### 已知 flake 的处理（如实报告）

`crates/app/tests/daemon_lifecycle.rs` 在 PV1 的 `cargo test --workspace` 段内命中**多次**失败，
且**每次失败的用例不同**（本包 diff 对 `crates/app/**` **零改动**，`git diff --stat 0f70c4c..6bf7a7c`
不含 `crates/app`）：

| 观察 | 失败用例 | 断言 |
| --- | --- | --- |
| A：PV1 首轮（归档 `reports/PV1-wp3-r2-first-run.log`） | `the_periodic_task_runs_again_after_one_full_cycle` | `daemon_lifecycle.rs:648` 启动初维护轮次 `left: 0 / right: 1` |
| B：文件级顺序复跑 1 | —（全绿） | `12 passed; 0 failed`（60.66s） |
| C：文件级顺序复跑 2 | `the_periodic_task_runs_again_after_one_full_cycle` | `11 passed; 1 failed` |
| D：一次与另一运行争用同一 target 目录的执行 | `stop_does_not_wait_for_the_full_grace_without_other_clients` | `daemon_lifecycle.rs:369` drain 时序 |

同一二进制、同一工作树、同一命令的多次运行给出**相反**结果且失败用例漂移，确认为任务书点名的非确定性
时序 flake（高负载/并发构建下更易命中）。按任务书要求复跑该文件（≤2 次，见 `reports/PV1-wp3-r2-flake-rerun.log`），
**未**使用 `#[ignore]`、未删测试、未弱化或收紧断言。最终**串行**重跑 PV1（无并发作业，唯一一次已完成的
`npm run verify`）**exit 0**，该文件 12/12 通过（见下表与 `reports/PV1-wp3-r2.log:507`）。

### PV2 子检查逐条（`reports/PV2-wp3-r2.log`）

```
agent-host: lib 5 + bin 5 + catalog 22 + resume 7 + session 19 + supervision 15 + view_contract 1 = 74 passed; 0 failed
core lib: 179 passed; 0 failed（第 1 轮 177 → 179：新增 derive 单测 1 条 + broker 端到端 1 条）
storage-sqlite: lib 3 / admin_audit 15 / admin_store 43 / attachments 5 / commit 18(+1 ignored) /
                compaction_recovery 3 / contract_v03 5 / enum_coverage 2 / imported 10 /
                migration 12(+1 ignored) / permissions 4 / resume_columns 6 / retention 8 /
                session_version_rule 3 / title_write 4（本轮新增）/ workspace_alias 6
                — 全部 0 failed
总计 0 failed（含 Doc-tests 0 failed；两处 ignored 为既有 baseline，本包未触碰）
```

（逐套件计数取自 `reports/PV2-wp3-r2.log` 的 `test result:` 行；`title_write` 为本轮新增的 4 条用例，
`derive`/`broker` 的新用例计入 core lib 的 177 → 179。）

**命中 flake 情况（PV1 的 `--workspace` 全量测试段）**：`crates/app/tests/daemon_lifecycle.rs` 在上述多次 PV1
运行中命中失败（失败用例在 `the_periodic_task_runs_again_after_one_full_cycle` 与
`stop_does_not_wait_for_the_full_grace_without_other_clients` 之间漂移，非确定性、与本包无关，详见上节），
按任务书复跑该文件 2 次取证并存档；**最终串行 PV1 exit 0**（该文件 12/12）。PV2
（`-p core -p storage-sqlite -p agent-host`）**未命中**任何 flake，一次通过。

### 证据日志

| 检查 | 日志路径（变更目录相对） |
| --- | --- |
| PV1（`npm run verify`，最终串行 exit 0） | `reports/PV1-wp3-r2.log` |
| PV1 首轮（命中 flake，归档备查） | `reports/PV1-wp3-r2-first-run.log` |
| PV1 命中 flake 的文件级复跑与说明 | `reports/PV1-wp3-r2-flake-rerun.log` |
| PV2 | `reports/PV2-wp3-r2.log` |

---

## 未验证内容

- **未执行 E2E**（`crates/app/tests/node_link_e2e.rs` 的 AC1 等）：WP3 的 Write Scope 不含组合根接线，
  节点级事件的端到端投递仍不可观察（`review-w3-r1` 的 §2.E 与 `review-w4-r1` 的 MAJOR F1 已登记该组合根缺口）。
- **未验证** Windows 上的 symlink/junction 实机行为与 verbatim/UNC 路径（`strip_verbatim` 只有 temp 目录用例）；
  F2 的用例用 `/etc/...` 与 `/tmp/...` 的绝对形式，在 Windows 上同样落进「越界 + 收窄为文件名」分支
  （已由测试通过证实），但未覆盖「两个同名**区内**文件」的第三种组合（区内时 `display_path` 已是完整相对路径，
  两种键取值本就相同，不构成缺陷）。
- **未验证** `title_write.rs` 之外的标题列往返（真实 Daemon 端到端由 R9 的下游消费面承担）。
- **未复跑** `review-w3-r1` 已通过的四个必查重点的独立复核（本轮判为不需重写；其复核由新一轮独立 reviewer 承担）。
- **不判**用例集合是否齐备（Coverage Index / validator）与前端渲染口径（WP7）。
- 本轮在 worktree 内建立的 `node_modules` **目录联接**仅为让 `check:agentic` 在 worktree 内可执行（只读共享主检出
  的包内容，未修改任何包文件、未提交）；worktree 由 provisioner 回收，该联接不属于交付内容。

---

```agentic-handoff
version: 1
agent_context:
  agent_id: "coder-w3-r2"
  isolation: "fork_turns=none（新实现实例接管既有 worktree .worktrees/wp3；未继承 coder-w3-r1 的对话，只携带第 1 轮提交 0f70c4c 与 review-w3-r1 报告）"
handoff_index:
  - task_id: "2.3"
    work_package: WP3
    role: coder
    phase: fix
    round: 2
    stage: work-package
    attempt: 2
    target_revision: "6bf7a7c3e88c84c06d4112204c4d3cd52a7cfab1"
    evidence_type: DELIVERY
    evidence_id: deliver-wp3-r2
    report_path: "reports/deliver-wp3-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 .worktrees/wp3（分支 feat/wp3，base=0f70c4c）上修复 review-w3-r1 的 F1（MAJOR）与 F2–F4，固定为提交 6bf7a7c（5 files changed, +522/−23）。F1：title=Some(None) 不再替换成只写 title 的窄语句，改为在同一条 UPDATE 里用哨兵参数表达三态（CASE WHEN ?N THEN NULL WHEN ?M IS NULL THEN title ELSE ?M END），version+1/updated_at=commit.at 与同批其它列一律照写，与同批 session.mode.changed 共存时不再触发 core 的版本漂移 Corrupt；新增 crates/storage-sqlite/tests/title_write.rs（4 用例）并以回退源码实证判别力。F2：DerivedFileChange 增非投影字段 origin_key（原始 path 原文），dedup_key()/change_id() 改用它，file_changed_view 输出不变；同名越界文件不再互相吞并（derive 单测 + broker 端到端各 1 条）。F3：commit_node_event 按形状拒绝空串 agentId。F4：docs §5.2 的 [决定] 补 title 两层可选语义并指向 §6 第 21 条、§6 第 21 条去重键口径同步。未改动四个已通过检视的实现与 ports.rs。PV1 串行 exit 0，PV2 exit 0。"
    source_evidence: "openspec/changes/sync-scope-and-pwa-client/reports/review-w3-r1.md"
  - task_id: "3.5"
    work_package: WP3
    role: coder
    phase: fix
    round: 2
    stage: work-package
    target_revision: "6bf7a7c3e88c84c06d4112204c4d3cd52a7cfab1"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/deliver-wp3-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "npm run verify（cwd=.worktrees/wp3，CARGO_TARGET_DIR=D:\\Project\\acp-remote\\.target-wt\\wp3，Node v22.22.0 / npm 10.9.4 / rust-toolchain 1.98.1）在 6bf7a7c 上 exit 0：check 十道全绿（schemas 149/51/41、commands 13、errors 58、features 13、assets 17/213、acp 25/11、docs 415 links/9058 refs、boundaries 12 crates、drift 36 DDL + 15 traits/96 methods、agentic doctor 6 项 PASS + validate 21 passed；本轮在 worktree 内建 node_modules 目录联接使 check:agentic 实跑）与 check:rust 三条全绿（fmt/clippy/workspace test，daemon_lifecycle 12/12）。此前多次运行命中该文件的已知非确定性时序 flake（失败用例在两条时序用例间漂移），已按任务书复跑并存档，未使用 #[ignore]/删测试/弱化断言。"
    source_evidence: reports/PV1-wp3-r2.log
  - task_id: "3.5"
    work_package: WP3
    role: coder
    phase: fix
    round: 2
    stage: work-package
    target_revision: "6bf7a7c3e88c84c06d4112204c4d3cd52a7cfab1"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "reports/deliver-wp3-r2.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "cargo test --locked -p core -p storage-sqlite -p agent-host --all-features（cwd=.worktrees/wp3，CARGO_TARGET_DIR=.target-wt/wp3）在 6bf7a7c 上 exit 0：core lib 179 passed/0 failed（第 1 轮 177 → 179，新增 derive 单测 1 条 + broker 端到端 1 条）、storage-sqlite 全部套件 0 failed（含本轮新增 title_write 4/4）、agent-host 22 passed。一次通过，无 flake。"
    source_evidence: reports/PV2-wp3-r2.log
checks:
  - id: PV1
    work_package: WP3
    stage: work-package
    command: "npm run verify (cwd=.worktrees/wp3, CARGO_TARGET_DIR=D:\\Project\\acp-remote\\.target-wt\\wp3, Node v22.22.0)"
    environment: "Windows；Node v22.22.0 / npm 10.9.4；rust-toolchain 1.98.1（x86_64-pc-windows-msvc）；CARGO_TARGET_DIR 独占"
    scope: "check 十道（schemas/commands/errors/features/assets/acp/docs/boundaries/drift/agentic）+ check:rust 三条（fmt --check / clippy -D warnings / workspace 全量 test）"
    exit_code: 0
    log_path: reports/PV1-wp3-r2.log
    result: PASS
  - id: PV2
    work_package: WP3
    stage: work-package
    command: "cargo test --locked -p core -p storage-sqlite -p agent-host --all-features (cwd=.worktrees/wp3, CARGO_TARGET_DIR=D:\\Project\\acp-remote\\.target-wt\\wp3)"
    environment: "Windows；rust-toolchain 1.98.1；fake ACP Agent，无外部服务；CARGO_TARGET_DIR 独占"
    scope: "core（含 derive/broker 派生与校验用例）、storage-sqlite（含新增 title_write 4 用例）、agent-host 的全部测试目标"
    exit_code: 0
    log_path: reports/PV2-wp3-r2.log
    result: PASS
test_delivery:
  WP3:
    kind: automated
    artifacts:
      - reports/PV1-wp3-r2.log
      - reports/PV2-wp3-r2.log
      - reports/PV1-wp3-r2-flake-rerun.log
      - reports/PV1-wp3-r2-first-run.log
    basic_checks:
      - PV1
      - PV2
```
