# CR-C1 候选独立检视报告（candidate / U1 / Round 1）

> 持久化说明：本报告由 reviewer 实例 `reviewer-C1`（只读工具集，无写权限）以全文返回、由主 Agent 原样落盘。run `f494a2c4-800c-462f-9ad3-92a436a80162`。
> **Target Revision `2ed142dedec2facf8f6e174d1aa165f549cbc47e`**（Base `95051f93db15ff867c3207386e402094790434bb`）。

## 结论

**PASS**（0 CRITICAL / 0 MAJOR）。候选 `2ed142d` 可进入 premerge 门禁（tasks `6.6`），但有一项本机无法闭合的证据缺口必须在 premerge 之前/之中如实保留为 PENDING。

### 关键限制（先声明）

reviewer 的工具集**没有 shell / git 执行能力**，只有只读的文件读取、grep、find、ls 与 `watchdog_diff`（后者明确不含已提交范围）。因此交底要求的「逐条用真实 git 命令复核」中，**依赖 git 对象运算的四项里只有两项能用可读的 git 元数据独立证实**。未证实的一律不当作通过。

> **主 Agent 后续补做**：B①（`crates/*/src/**` 计数 = 0）与 B④（第 3 轮只改注释）的 git 级机械核对，**已由 main 亲自执行并全部证实**，见文末「main 补做的两条机械核对」。

---

## A. 跨工作包新增交互面

### A1 — WP6 产品代码与 TP2 测试的对接：真实链路闭合，但 Agent 后端是替身（不是 `AgentHost`）

**真实的部分**：`crates/app/tests/support/owner.rs:196-264` 用的是组合根**自己的装配函数** `app::daemon::net_config` 与 `app::daemon::register_node_link_paths`（与 `crates/app/src/daemon.rs:986/996` 生产 `NetIngress::start` 是**同一个函数**），真实 `SqliteStore`、真实 `identity_auth::Authority`、真实 `server::transport::net::NetListener`、真实 `LocalAdminRouter`。请求经真实 loopback WSS 报文进入 `crates/server/src/node_link/command.rs:265` 的 `CommandName::SessionResume => self.on_session_resume(...)`，再由 `command.rs:1083` 调 `self.core.resume_session(...)` → `crates/core/src/use_cases.rs:297-307` → `crates/core/src/broker.rs:1489`。**这条链路是闭合的真实链路，断言的是 wire 上的真实终态帧。**

**替身的部分**：`support/owner.rs:169-172` 注入的是 `ScriptedBackends`，**不是生产组合根的 `AgentHost`**（生产在 `crates/app/src/compose.rs:219-228` 用 `host.clone()` 作 `backends`）。也就是说 `core::Broker::resume_session` × **真实 `AgentHost::resume`** 这一对组合**从未被任何测试同时覆盖**：`agent-host` 侧 `crates/agent-host/tests/resume.rs` 直接打 trait 方法，`core` 侧用 fake backends。

**判定**：这是 WP1–WP6 既有的结构性性质（`node_link_e2e.rs` 同样如此），TP2 未改变它，两侧各自都有真实覆盖（`agent-host` 侧连真实 ACP 子进程都拉起了）。按 AGENTS.md §9「普通 CI 使用可控的 fake ACP Agent」，**不构成阻断**；登记为 MINOR + 残余风险（见 CR-C1-F2）。

### A2 — `sqlx` dev 依赖不污染生产构建面：✅ 证实

- `crates/app/Cargo.toml` 中 `sqlx = { workspace = true }` 位于 `[dev-dependencies]` 段内（紧跟 `rustls-pki-types.workspace = true`，在 `[lints]` 之前），**无 `features` 键**。
- `grep sqlx crates/app/src` → **No matches found**：生产代码零引用。
- PV2 `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致`（`merge-u1-candidate-PV2.log:30`）在候选上实跑通过。
- 根 `Cargo.toml:52` 的 `sqlx` 定义未被 TP2 触及。

### A3 — 共享落点 `crates/app/tests/support/owner.rs`：语义一致，但有一处 TP2 引入的过期注释

- 五个替身（`FlakySessionStore` / `ScriptedCatalog` / `ScriptedBackends` / `ScriptedEndpoint` / `Parked`）与 TP2 新增的 `overwrite_persisted_workspace_cwd`（`:330-356`）与 `recovery_reads` 计数（`:134-137`）共存于同一文件，**编译与语义无冲突**（候选 clippy `-D warnings` 0 诊断、app e2e 8 passed 即为机器证据）。
- TP2 把 `ScriptedBackends::create` 由 `agent_session_id: None` 改为 `Some(scripted_agent_session_id(session))`（`owner.rs:650`）。对 WP6 的影响面逐条查过：新增的那次 `commit_owned`（`broker.rs:1446-1468`）`events` 为空、`state: None`，因此 ① 不会多发 `resource.event`（不影响 `node_link_e2e.rs:523-526` 的 `["turn.queued","turn.started"]` 精确序列断言）；② `FlakySessionStore::matches_fault` 用 `commit.events.iter().any(...)` 匹配，**空 events 永不命中**，R61 故障注入不被误伤；③ 不改变 `sessionMeta.state`。WP6 三条 e2e 在候选上实跑通过（`merge-u1-candidate-PV1-stage2-workspace-test.log:408-413`）。**未破坏 WP6 语义。**
- **但**：`owner.rs:744` 的字段注释仍写着「创建脚本端点**不产生**（ACP 会话标识）」，与 `:650` 现在**确实产生**矛盾。见 CR-C1-F1。

### A4 — `crates/storage-sqlite/tests/` 与 `crates/agent-host/tests/` 共存：✅ 证实无冲突

- `crates/storage-sqlite/tests/` 现有 13 个目标，`resume_columns.rs`（TP2）与 `migration.rs`（WP4）并列，各自 `mod support;` 复用同一 `support::{temp_dir, raw_pool, raw_write_pool}`，无同名符号冲突；候选实跑 `resume_columns` **6 passed**。
- `crates/agent-host/tests/` 现有 6 个目标，`resume.rs`（TP2，7 passed）与 `session.rs`（WP5，19 passed）并列，常量 `RESUME_LINE_SESSION` 在两文件各自独立定义、互不干扰（各自是独立测试 crate）。
- `#[ignore]` 全仓仅 2 处（`storage-sqlite/tests/commit.rs:1100` `crash_child`、`migration.rs:1618` `regenerate_v1_fixture`），**均非 TP2 文件**，与日志中 2 条 ignored 逐名对应。

### A5 — 封闭词表四处一致：✅ 内容层抽查全部一致

| 位置 | 取值 |
|---|---|
| `compatibility/commands/v1/commands.json:17` | `session.resume`，`kind: mutation`，`grant: grant.remote-work`，`pack: null`，`transport: ["node_link"]`，`delivery: conditional_mvp` |
| `compatibility/commands/v1/commands.json:34` | `grant.remote-work: ["session.create", "session.resume"]` |
| `crates/identity-auth/src/authorization.rs:71` | `("grant.remote-work", &["session.create", "session.resume"])` |
| `crates/core/src/broker.rs:130` | `"session.resume" => "grant.remote-work"` |
| `compatibility/errors/v1/errors.json:79/81/82/83/84/74` | `nodelink.command.unsupported` / `.idempotency_conflict` / `.unsupported_field` / `.uncertain` / `nodelink.internal.unavailable` / `nodelink.export.not_granted` — 全部已登记，**未新增错误码**（PV2 `error registry OK: 58 codes across 2 protocols`） |
| `schemas/node-link/v1/command.schema.json:62/785/1087` | 命令枚举含 `session.resume`，payload `maxProperties: 0` |
| `docs/NODE_LINK_PROTOCOL.md:409/623` | 表格与 payload 规则同步 |

`pack: null` 只落在 `transport` 不含 `sync` 的两条 Node-Link-only 命令上，符合门禁判据。

---

## B. 合入过程与机械证据的独立复核

### 前置事实（可读 git 元数据，直接证实）

| 命令（等价读取） | 实际内容 | 判定 |
|---|---|---|
| `.git/refs/heads/integration/session-resume-du1` | `2ed142dedec2facf8f6e174d1aa165f549cbc47e` | ✅ 候选 ref 正确 |
| `.git/worktrees/session-resume-du1/HEAD` | `ref: refs/heads/integration/session-resume-du1` | ✅ |
| `.git/refs/heads/main` | `81e350ff340014265eb7c9251237c799d4357fee`（loose ref 优先于 packed-refs） | ✅ main 未移动 |
| `.git/worktrees/session-resume-du1/ORIG_HEAD` | `95051f93db15ff867c3207386e402094790434bb` | ✅ 合并前 HEAD 独立证实 |
| `.git/logs/refs/heads/integration/session-resume-du1:9` | `95051f9 → 2ed142d … merge agentic/session-resume-tp1: Merge made by the 'ort' strategy.` | ✅ 父提交对 `(95051f9, 3484541)`、策略 `ort` 证实 |

| 复核项 | 结果 |
|---|---|
| **① 产品与测试之外零越界** | **reviewer 未证实**（无 git 运算）。间接旁证：候选工作树相对 `2ed142d` **零改动**；`crates/app/src` 零 `sqlx` 引用。**⇒ 已由 main 补做并证实（见文末）** |
| **② `4146611` 是 amend 前的 SHA** | **✅ 证实，且证据比 merger 给的更强**。`.git/logs/refs/heads/agentic/session-resume-tp1:7` 逐字记载两行：`a02e2fd… 4146611… commit: test(app): 补 R25 …`（`1790848236 +0800`）与 `4146611… f44301e… commit (amend): test(app): 补 R25 …`（`1790848295 +0800`）⇒ **reflog 直接记载 amend 关系、同父、59 秒差**。另：`.git/lost-found/commit/4146611a2e70…` 文件存在 ⇒ 曾执行 `git fsck --lost-found` 且判为**悬空**；`4146611` 在 `refs/heads/**` 与 `packed-refs` 中**均无命中** ⇒ 不可达。**三点全部成立。** ⚠️ 补充 merger 未提的事实：`.git/lost-found/commit/` 下有**数百个**条目，说明本仓历史上跑过 `git fsck --lost-found`；这不是缺陷，但若将来有人跑 `git gc`，`4146611` 会被回收 |
| **③ 清单两行** | **构造性证实（未跑 diff）**。`crates/app/Cargo.toml` 的 dev 段恰为 `sqlx = { workspace = true }`（无 features 键）；`Cargo.lock:135` 的 `app` 依赖表在 `"sha2 0.11.0"` 与 `"storage-sqlite"` 之间新增 `"sqlx"`；`Cargo.lock:2107-2111` 的 `[[package]] name = "sqlx" / version = "0.8.6" / source = registry+…crates.io-index / checksum = 1fefb89…d97dc` 完整存在。**「未新增 `[[package]]` 条目」可由构造证明**：`storage-sqlite`（TP2 未触及）本身就依赖 `sqlx`（`Cargo.lock:2309`），故该 package 块必然先于 TP2 存在 |
| **④ TP2 第 3 轮（`3484541`）只改注释** | **reviewer 未证实**（无 git）。可做的旁证：merger 引用的 `+/-` 摘录在目标文件中**逐字命中**。**⇒ 已由 main 补做并证实（见文末）** |

---

## C. 候选级检查证据的可信度（不重跑）

- **12 个 crate 全覆盖：✅ 证实。** 日志 `Doc-tests` 段逐条列出 12 个 crate（`:1560-1632`），与 `crate boundaries OK: 12 个 crate` 一致。
- **1074 = 1041 + 33：✅ 数字自洽（reviewer 逐行求和复核）。**
  - 候选日志全部 `^test result: ok. N passed` 行求和 = **1074**；其中 2 行含 `1 ignored`（`:1350` 18+1、`:1413` 12+1）。
  - 基线日志同样求和 = **1041**，同样 2 行含 `1 ignored`。
  - TP2 五个新测试目标：`acp-protocol/session_resume` **7**、`storage-sqlite/resume_columns` **6**、`agent-host/resume` **7**、`node-link-protocol/session_resume_command` **5**、`app/session_resume_e2e` **8** = **33**。1041 + 33 = **1074**；failed 两侧均 0，ignored 两侧均 2 且逐名相同。
  - 顺带确认 merger 对前序报告的订正是对的：基线日志确实含 2 条 ignored，`merge-u1-integrate-wp6.md` 写「0 ignored」是叙述漏记。
- **ignored 归属：✅ 证实为既有项。** 全仓 `#[ignore]` 仅 2 处，**TP2 新增 0 条**。
- **失败痕迹：✅ 无。** 候选日志 grep `FAILED|failures:|^error|panicked` → **No matches found**；clippy 日志 grep `^(error|warning)` → **No matches found**，`CLIPPY_EXIT=0`；PV2 `Totals: 19 passed, 0 failed (19 items)`。
- **CI 专属判定缺口：明确登记。** `cargo-deny`（`deps`/`advisories`）、`gitleaks`（`secrets`）、`commits` job **本机 NOT_EXECUTED，未执行 ≠ 通过**。CI 五个 job 均 `runs-on: ubuntu-latest`，`checks` job 跑 `npm run check:rust` ⇒ **Linux 上会真正编译并执行那两条 `#[cfg(unix)]` 用例**。

---

## D. 已知 PENDING 项：如实保留，✅ 三问全部成立

1. **候选的报告/日志中没有任何一处把它们写成已通过。** grep `reports/` 全目录下两个用例名 + `PENDING` + `CR8-F6`：所有命中都写着「未执行」「PENDING」「不得记 PASS」「待 Linux CI」（`merge-u1-candidate.md:265/435`、`tp2-test-design.md:103/168`、`tp2-tester.md:926-928`、`tasks.md:74`、`cr8-review.md:77`）—— **无一处记 PASS**。
2. **两条用例仍在文件里，未被 `#[ignore]` / 删除 / 改写。** `session_resume_e2e.rs:1406` `#[cfg(unix)]` + `:1408 fn a_persisted_directory_replaced_by_a_symlink_…`、`:1512` `#[cfg(unix)]` + `:1514 fn an_inaccessible_persisted_directory_…`；辅助 `ModeRestore` 在 `:1597/:1602/:1611`。全文件 `#[ignore]` 零命中，cfg 命中全部是 `#[cfg(unix)]`（无 `not(unix)` 反转）。
3. **机器证据成立。** 候选日志该目标 `running 8 tests … 8 passed; 0 failed; 0 ignored`，名单 8 条不含这两条；`--list` 输出同样 8 条，两个名字零出现。`--list` 会列出 ignored 用例而本目标 0 ignored ⇒ 排除「被 `#[ignore]` 掩盖」。

**状态：PENDING，待 Linux CI `checks` job（`ubuntu-latest`）首次编译 + 执行。这是本变更唯一一个本机无法闭合的证据缺口，premerge 不得记 PASS。**

---

## Findings

| ID | Severity | Location | Impact | Recommendation |
| --- | --- | --- | --- | --- |
| CR-C1-F1 | MINOR | `crates/app/tests/support/owner.rs:744`（对照 `:650`） | TP2 把 `ScriptedBackends::create` 的 `agent_session_id` 由 `None` 改为 `Some(...)`，但 `ScriptedEndpoint` 字段的 doc 注释仍写「创建脚本端点**不产生**」，**注释与代码直接矛盾**。纯注释，不影响任何断言或运行行为。这是**只有把 WP6 与 TP2 落在同一文件里通读才能发现**的跨包残留 | 删掉「；创建脚本端点不产生」这半句，或改为「创建时由 `create` 现场生成、恢复时由 `ResumeSessionRequest` 带入」 |
| CR-C1-F2 | MINOR | 覆盖面：`core::Broker::resume_session`（`broker.rs:1489`）× 生产 `AgentHost::resume`（`compose.rs:228` 注入） | `session_resume_e2e.rs` 注入 `ScriptedBackends` 而非生产 `AgentHost`；二者的**组合**从未被任何测试覆盖 ⇒「替身能过、真实装配走不通」的理论空档仍存在（虽然极窄：`Broker::resume_session` 在 `backends.resume` 返回后只保存 endpoint、不回读 `agent_session_id()`，跨实现假设很少）。两侧各自有真实覆盖。**非阻断**——AGENTS.md §9 明确允许普通 CI 用可控 fake ACP Agent，且这是 WP1–WP6 既有结构、TP2 未改变 | 不要求本轮修复。后续切片可让 `session_resume_e2e.rs` 增加一条走真实 `AgentHost`（以 `acpr-fake-acp-agent` 作 profile command）的变体；或登记为已知覆盖边界 |

**未发现 CRITICAL / MAJOR。**

---

## Assessment

- **本轮静态检视结论：PASS**，绑定 **Target `2ed142dedec2facf8f6e174d1aa165f549cbc47e`**（ref 独立读出确认），Base `95051f93db15ff867c3207386e402094790434bb`（`ORIG_HEAD` 独立读出确认）。工作区在该版本上零改动。
- **隔离方式**：全新 `reviewer-C1` 实例，**未参与** WP1–WP6/TP1/TP2 的任何实现或整合，未继承任何实现对话。
- **未重判已 PASS 的内容**：CR1–CR8 各包内部结论一律沿用。
- **待补证据**：
  1. **`CR8-F6` / 2 条 `#[cfg(unix)]` 用例** —— `evidence_status: PENDING`，待 Linux CI `checks` job 首次编译+执行。**应在哪个门禁前补齐：合入 `main` 之前（6.7 的主分支回归）或最迟在最终验收前；失败则回 TP2 开新问题编号。**
  2. **CI 专属判定** —— `cargo-deny`、`gitleaks`、`commits` job：**NOT_EXECUTED**。**应在哪个门禁前补齐：push 后 CI 五 job 全绿。**
  3. **B① / B④ 两条缺 git 对象运算的机械核对** —— reviewer 明确标注未证实、不采信。**⇒ 已由 main 补做（见下节），全部证实。**
  4. **B② / B③** —— 已由 reviewer 独立证实（reflog + lost-found + 文件内容），无需补。
- **未发现需要重开已结案任务的发现。**

---

## main 补做的两条机械核对（2026-10-01，reviewer 无 git 能力）

**B① 产品与测试之外零越界 —— ✅ 证实。** `git diff --name-only 95051f9..2ed142d` 实测 8 个文件：
`Cargo.lock`、`crates/acp-protocol/tests/session_resume.rs`、`crates/agent-host/tests/resume.rs`、`crates/app/Cargo.toml`、`crates/app/tests/session_resume_e2e.rs`、`crates/app/tests/support/owner.rs`、`crates/node-link-protocol/tests/session_resume_command.rs`、`crates/storage-sqlite/tests/resume_columns.rs`。
⇒ `crates/*/src/**` 计数 = **0**；`compatibility/` + `schemas/` + `fixtures/` 计数 = **0**；其余全在 `crates/*/tests/**` 与**已获授权的**两处清单行（`Cargo.lock`、`crates/app/Cargo.toml`）。

**B④ TP2 第 3 轮（`3484541`）只改注释 —— ✅ 证实，且为决定性证据。** `git diff --stat f44301e..3484541` = 恰好 3 个 `crates/**/tests/**` 文件（`session_resume_e2e.rs` +22/−3、`support/owner.rs` +9/−4、`resume_columns.rs` +19/−5）。过滤掉注释行后，**非注释改动行数 = 0**；全部 `+/-` 行中**不以 `//` 或 `///` 开头的行数 = 0**。
⇒ **38 行改动全部是注释，断言、阈值、用例增删零改动**。CR8-F7（证据与版本绑定）的机械闭环**不再只由 merger 单方背书**，已由 main 独立证实。

### handoff_index

- `task_id: "6.4"` · `work_package: U1` · `role: reviewer` · `phase: candidate` · `round: 1` · `stage: candidate` · `target_revision: "2ed142dedec2facf8f6e174d1aa165f549cbc47e"` · `evidence_type: REVIEW` · `evidence_id: CR-C1` · `report_path: "openspec/changes/session-resume/reports/cr-c1-candidate-review.md"` · `result: PASS` · `evidence_status: NEW`
- `applicability_basis`: 全新只读实例，未参与任何工作包或整合；检视对象为固定候选 2ed142d 的「八包共存」新增交互面、合入过程机械证据、候选级检查证据可信度、PENDING 项如实保留。0 CRITICAL / 0 MAJOR、2 项 MINOR。工具限制：无 shell/git 执行能力，B① 与 B④ 未取得 diff 级证据并如实标注为未证实（**后已由 main 补做并证实**）。
- `task_id: "2.8"` · `work_package: TP2` · `role: reviewer` · `evidence_type: E2E` · `evidence_id: CR8-F6-UNIX-VARIANTS` · `report_path: "openspec/changes/session-resume/reports/merge-u1-candidate-app-e2e-list.log"` · `result: BLOCKED` · `evidence_status: PENDING` · 适用于那 2 条 `#[cfg(unix)]` 用例的候选级状态确认。
