# WP4 独立检视报告 · 第二轮（CR4-R2）

- **被检视版本**：`1a60dc2`（`test(storage): 修复目录归属列用例的编码污染并补齐 ModeChange::Set 的别名覆盖`），分支 `feat/sync-storage-v6`，HEAD = `1a60dc207927027ad5052055c5e01b92a5ef41fa`
- **复核目标**：上一轮 `reports/wp4-review.md` 的 **F-01**（`workspace_alias.rs:5` 的 3 个 U+FFFD）与 **F-04**（`ModeChange::Set` 变体的别名 bind 编号无覆盖）是否被正确修复且修复本身非空洞
- **基线**：`8af5d21`（上一轮被检视版本）、WP4 分支点 `347399f`、整体变更基线 `6c093f1`
- **工作树**：`D:\Project\acp-remote-wt\wp4-storage`（全程只读；结束时 `git status --porcelain` 为空、`HEAD^{tree}` 仍为 `a8f8cc4b062a8b8042e76fbfbc6c53c1e3c00da9`）
- **构建目录**：`CARGO_TARGET_DIR=D:\Project\acp-remote-wt\target-wp4-review2`（基线 `6c093f1` 副本另用 `target-wp4-review2-base`）

## 结论

**PASS**

F-01 与 F-04 均已正确修复，且修复是**实质性的、非空洞的**——F-04 的新用例经变异实测具有真实的判别力（杀死 Round-1 变异 F，而既有五条用例全部放过同一变异，这个不对称正是 F-04 的全部要害）。`1a60dc2` 只动了 `crates/storage-sqlite/tests/workspace_alias.rs` 一个文件；SQL、迁移段、文档、`crates/app/**` 全部逐字节未动。Round-1 的判据 1–9 承重项独立复核后仍全部成立。红窗口关闭：`cargo test --workspace --all-features` **EXIT=0（1085 passed / 0 failed）**，`cargo clippy --workspace --all-targets --all-features -- -D warnings` **EXIT=0（0 warning）**。

过程中命中了 `crates/app/tests/daemon_lifecycle.rs` 的既有竞态，**判定为 F-03 的同一根因、与本次变更无因果关系**，证据见 §五。F-02、F-03 保持未修（符合预期）。

未发现阻塞问题。新增 1 项 low 观察项（O-01，见 §四）。

---

## 一、F-01 —— 编码污染：逐字节确认已修复 ✅

**范围扫描（本轮重跑，不复用上一轮结论）**：`git diff --name-only 6c093f1..HEAD` 的 20 个文件逐一按字节扫描 U+FFFD（`\xef\xbf\xbd`）并校验 UTF-8 合法性：

```
   0  utf8-ok    crates/core/src/broker.rs
   0  utf8-ok    crates/core/src/model/ids.rs
   0  utf8-ok    crates/core/src/model/session.rs
   0  utf8-ok    crates/core/src/model/tests.rs
   0  utf8-ok    crates/core/src/ports.rs
   0  utf8-ok    crates/core/src/use_cases.rs
   0  utf8-ok    crates/server/src/node_link/command/tests.rs
   0  utf8-ok    crates/server/src/node_link/resource/tests.rs
   0  utf8-ok    crates/storage-sqlite/src/migrate.rs
   0  utf8-ok    crates/storage-sqlite/src/session_store.rs
   0  utf8-ok    crates/storage-sqlite/tests/commit.rs
   0  utf8-ok    crates/storage-sqlite/tests/contract_v03.rs
   0  utf8-ok    crates/storage-sqlite/tests/enum_coverage.rs
   0  utf8-ok    crates/storage-sqlite/tests/migration.rs
   0  utf8-ok    crates/storage-sqlite/tests/resume_columns.rs
   0  utf8-ok    crates/storage-sqlite/tests/retention.rs
   0  utf8-ok    crates/storage-sqlite/tests/session_version_rule.rs
   0  utf8-ok    crates/storage-sqlite/tests/workspace_alias.rs
   0  utf8-ok    docs/CORE_PORTS_AND_STORAGE.md
   0  utf8-ok    docs/MODULE_ARCHITECTURE.md
files with FFFD: 0
```

**该文件整体的 U+FFFD 计数**：`8af5d21` = **3** → `1a60dc2` = **0**。

**第 5 行的逐字节比对**（从两个 blob 分别切出第 5 行并 hexdump，唯一差异就在这一行）：

```
OLD (8af5d21) : ... e5 b7 b2 e6 9c 89 | ef bf bd ef bf bd ef bf bd | 20 76 31 ...
                                     └ 已有            └ 3×U+FFFD      └ " v1"
NEW (1a60dc2) : ... e5 b7 b2 e6 9c 89 | e7 9a 84          | 20 76 31 ...
                                     └ 已有      └ 的

OLD dec: //! `migration.rs` 已有��� v1 → v6 连续升级保留性与 `resume_columns.rs` 的恢复两列判据，只补目录归属列
NEW dec: //! `migration.rs` 已有的 v1 → v6 连续升级保留性与 `resume_columns.rs` 的恢复两列判据，只补目录归属列
```

即：**删掉 3 个 `EF BF BD`（9 字节）、补上 1 个 `的`（`E7 9A 84`，3 字节）**，净 −6 字节；该行其余全部字节（含中文标点 `E3 80 82`、`E3 80 81`、反引号、空格）逐一相同。**没有第二个字节被改动**——文件层面其余差异全部来自新增用例的追加（见 §二）。

文件规模：`8af5d21` 725 行 / 28390 字节 → `1a60dc2` 859 行 / 33816 字节。

**判定**：F-01 已在**字节层面**精确修复，无附带污染，无越界改动。✅

---

## 二、F-04 —— `ModeChange::Set` + 非空别名的覆盖：存在且非空洞 ✅

### 2.1 存在性与风格

新用例 `a_mode_change_commit_also_persists_the_workspace_alias` 位于 `crates/storage-sqlite/tests/workspace_alias.rs:437-554`，配套的章节分隔注释在 `:425-427`，模块文档新增条目在 `:16-17`。风格与既有五条用例一致：

- 同一套 helper（`temp_dir` / `at` / `create_session` / `raw_pool` / `raw_write_pool` / `register_workspace` / `quoted_alias` / `absolute_path` / `ALIAS` / `SESSION` / `EPOCH`），未新增第二套约定；
- 取证全部走真实 SQLite 文件与 `SqliteStore::open`（与 `storage_sqlite::migrate`）路径，断言读的是库字节（`quote()` / `PRAGMA` / 端口读路径），没有引入 mock；
- 断言消息带判别力说明（`"`ModeChange::Set` 变体下别名不得被 bind 编号错位吞掉"`），并在文档注释里写明「本用例是它的唯一防线」。

### 2.2 断言内容（逐条核对）

| # | 行 | 断言 | 读路径 |
|---|---|---|---|
| ① | `:478` | `written.version.get() == 2` | 提交返回值 |
| ② | `:494-499` | `current_mode()` 读回 `mode_id == "code"`、`display_name == "Code"` | `store.load()`（窄读取聚合路径） |
| ③ | `:513-518` | `workspace.alias().as_str() == ALIAS`、`display_name() == "Acp Remote"` | `store.list(SessionQuery{ only: Some([session]) })`（摘要投影 + LEFT JOIN） |
| ④ | `:526-534` | `agent_session_id == "acp-session-1"`、`workspace_cwd == absolute_path("set-mode")` | `store.load_recovery()` |
| ⑤ | `:539-552` | `quote(workspace_alias) == "'acp.remote'"`、`quote(current_mode_id) == "'code'"` | 裸 SQL 直读库文件 |

**同一批提交里同时给了** `ModeChange::Set(ModeRef::try_new(ModeId::new("code"), "Code"))`（`:458-461`）与 `workspace_alias: Some(ALIAS.to_owned())`（`:466`）——这正是 F-04 指出的、Round-1 变异 F 全绿放过的那条组合。`workspace_cwd`（`:465`）与 `agent_session_id`（`:464`）也一并给了非空值，于是同一条语句上的**全部五个 bind 位置**都被断言覆盖（模式 `?5`/`?6`、恢复两列 `?7`/`?8`、归属 `?9`）。

### 2.3 非空洞性 —— 变异实测（含与既有五条的不对称）

**方法**：在**工作树之外**（`D:\Project\acp-remote-wt\wp4r2-mut`，`git archive HEAD` 的独立副本）把 `Set` 变体的三列 bind 编号整体错位两位（Round-1 变异 F 的同一手法）：

```
agent_session_id = CASE WHEN ?5 IS NULL ... ELSE ?5 END   (原 ?7)
workspace_cwd    = CASE WHEN ?6 IS NULL ... ELSE ?6 END   (原 ?8)
workspace_alias  = CASE WHEN ?7 IS NULL ... ELSE ?7 END   (原 ?9)
```

（`current_mode_id = ?5, current_mode_name = ?6` 保持不动——于是 `?5`/`?6` 被两个表达式共享，正是代码注释 `:1132-1133` 警告的「编号空洞 / 错位会让绑定落到别的列上」的最坏形态。）

**结果**（`cargo test --locked -p storage-sqlite --all-features`，EXIT=101）：

```
running 6 tests
test a_null_alias_is_never_back_filled_from_the_canonical_path ... ok
test the_written_alias_round_trips_verbatim_and_an_unwritten_row_stays_none ... ok
test the_resume_flow_neither_reads_nor_rewrites_the_alias ... ok
test v5_database_upgrades_to_v6_by_appending_a_null_workspace_alias_column ... ok
test the_display_name_falls_back_to_the_alias_and_survives_a_re_pointed_alias ... ok
test a_mode_change_commit_also_persists_the_workspace_alias ... FAILED

failures:
---- a_mode_change_commit_also_persists_the_workspace_alias stdout ----
thread 'a_mode_change_commit_also_persists_the_workspace_alias' panicked at
  crates\storage-sqlite\tests\workspace_alias.rs:513:5:
  left: "acp-session-1"
 right: "acp.remote"

test result: FAILED. 5 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

**不对称性（F-04 的要害）**：

| | 变异 F 下的结果 |
|---|---|
| Round-1 既有五条用例 | **5 passed**（全绿放过） |
| Round-1 时全仓 `storage-sqlite` 套件 | **全绿**（F-04 原文：「整个 `storage-sqlite` 测试套件全绿」） |
| 本轮新增用例 | **1 failed**，`left: "acp-session-1", right: "acp.remote"` |

变异后 `workspace_alias` 落进了 `agent_session_id` 的取值位置（`"acp-session-1"`），用例在 `:513` 的第一条别名断言上就精确报错。**同一变异在修复前漏网、在修复后被杀死**——这是判别力的直接证据，不是「断言更严」的推断。

变异失败时 `workspace_alias.rs:513` 的行号与提交一致；其余测试二进制（`admin_store` 43 passed、`migration` 12 passed、`resume_columns` 6 passed、`commit` 18 passed、`imported` 10 passed、`retention` 8 passed 等）全部仍然通过，说明变异确实只打在这条语句上、没有造成连带崩溃。

**源码树字节同一性（变异实验后）**：

```
$ git rev-parse HEAD^{tree}
a8f8cc4b062a8b8042e76fbfbc6c53c1e3c00da9
$ git status --porcelain      → 无输出
$ git diff HEAD --stat        → 无输出
```

工作树的 `HEAD^{tree}` 与实验前完全一致、`git status` 与 `git diff HEAD` 均为空 ⇒ **变异只发生在工作树外的副本里，源树逐字节未动**。

**判定**：F-04 已修复，新用例**非空洞**。✅

---

## 三、`1a60dc2` 的文件范围与回归核对

```
$ git diff --name-status 8af5d21..HEAD
M       crates/storage-sqlite/tests/workspace_alias.rs

$ git diff --stat 8af5d21..HEAD
 crates/storage-sqlite/tests/workspace_alias.rs | 139 +++++++++++++++++++++++++++++++-
 1 file changed, 137 insertions(+), 2 deletions(-)
```

与预期完全一致：**只有 `crates/storage-sqlite/tests/workspace_alias.rs`**。

范围外零位移的独立确认：

```
$ git diff --name-only 8af5d21..HEAD -- crates/storage-sqlite/src/   → 空（SQL / 迁移段未动）
$ git diff --name-only 8af5d21..HEAD -- docs/                      → 空（文档未动）
$ git diff --name-only 8af5d21..HEAD -- crates/                     → 仅 workspace_alias.rs
$ git diff --stat   347399f..HEAD -- crates/app/                   → 空
$ git diff --stat   347399f..HEAD -- crates/core/                  → 空
$ git diff --stat   347399f..HEAD -- crates/sync-protocol/         → 空
```

`1a60dc2` 对该测试文件的改动内容（`git diff 8af5d21..1a60dc2`）：3 处 hunk——

1. `:5` F-01 的编码修复（1 删 1 增）；
2. `:13-25` 模块文档新增一条「模式变更同批写入」说明、`use acp_core::model::{...}` 补 `ModeId, ModeRef`（1 删 1 增 + 2 行新增）；
3. `:422+` 追加分隔注释与整条新用例（`:425-554`）。

**没有**任何对既有五条用例的改动（`git diff` 的 3 个 hunk 中没有一个落在 `:174-724` 的既有用例体内）。**无回归**。✅

---

## 四、F-02 / F-03 保持未修（符合预期）✅

```
$ git diff --stat 347399f..HEAD -- crates/app/
（空）

$ git diff --stat 6c093f1 HEAD -- crates/app/
（空）

$ git rev-parse 6c093f1:crates/app/tests/daemon_lifecycle.rs HEAD:crates/app/tests/daemon_lifecycle.rs
9d4356825b74596ae0870958d38c1654afe14621
9d4356825b74596ae0870958d38c1654afe14621        ← 同一 blob，逐字节相同
```

`crates/app/**` 在整个 WP4 分支（`347399f..HEAD`）以及自基线 `6c093f1` 起**零 diff**。F-02（migration 在取单实例锁之前）与 F-03（`daemon_lifecycle` 周期任务日志的既有竞态）**没有被顺手静默修掉**，范围证据保持完整。✅

---

## 五、`daemon_lifecycle.rs` 命中项的判定：**F-03 的同一根因，与本次变更无因果关系**

### 5.1 命中事实

首轮 `cargo test --locked --workspace --all-features`（EXIT=101）：

```
test the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown ... FAILED
...
thread '...' panicked at crates\app\tests\daemon_lifecycle.rs:603:5:
启动后必须已经跑过一轮清理：
{"event":"daemon.dev_mode",...,"ts":"2026-10-02T17:24:54.505Z"}
{"event":"daemon.ephemeral_identity",...,"ts":"2026-10-02T17:24:54.568Z"}
{"event":"daemon.seed_imported",...,"ts":"2026-10-02T17:24:54.578Z"}
{"address":"127.0.0.1:51336","event":"net.listener_bound",...,"ts":"2026-10-02T17:24:54.581Z"}
{"event":"daemon.ingress_ready",...,"ts":"2026-10-02T17:24:54.582Z"}
{"event":"daemon.ready",...,"ts":"2026-10-02T17:24:54.584Z"}

test result: FAILED. 11 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
error: test failed, to rerun pass `-p app --test daemon_lifecycle`
```

即：`daemon.ready` 已打出（17:24:54.584Z），但日志里**一条 `daemon.maintenance` 都还没有**——断言 `!ticks.is_empty()` 在 `daemon.start()` 返回后立即执行。

### 5.2 与 F-03 是同一根因

Round-1 的 F-03 记的是同一文件、同一竞态的另一处断言：`the_periodic_task_runs_again_after_one_full_cycle` 在 `:648-653` 断言「启动初清理**恰好**一轮」（`left: 0, right: 1`）。本轮命中的是 `:596-612` 的 `the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown`，断言「启动后**至少**跑过一轮」（`!ticks.is_empty()`）。**两处的被断言对象、断言时刻、与 `start()` 的关系完全相同**，只是「恰好一条」与「至少一条」的措辞差别。两条断言都会在「后台启动初清理那一轮尚未 flush」时失败。

### 5.3 结构性证据（不依赖采样运气）

`crates/app/src/daemon.rs`：

- `:1270` `spawn_maintenance(&mut tasks, &composition, Arc::clone(&shutdown));` —— 周期任务是 `tokio::spawn` 出去的**后台任务**；
- `:620-641` 该任务体内 `startup = true`，进入 `loop` 的第一次 `tokio::select!` 之后才调 `maintenance_tick(...)`（`:631`），而 `maintenance_tick` 的**日志行在函数末尾**（`:692-696`）——即首轮清理至少要走完 `prune` + `expire_pairings` + `sweep_orphans` 三次存储往返才写出第一行；
- `:1284-1288` `daemon.ready` 的 `tracing::info!` 与 `:1270` 之间**没有任何 await 或 join**——`start()` 的「就绪」判据与后台首轮清理之间不存在同步。

⇒ 「`start()` 返回时首轮清理日志是否已落盘」在代码里是**未定义的调度结果**，这是 `crates/app` 的既有性质，与 `crates/storage-sqlite` 无关。

### 5.4 采样证据

| 环境 | 采样 | 结果 |
|---|---|---|
| WP4 分支，单独 `--exact` 单跑该快用例 | 12 次 | **12 passed / 0 failed** |
| WP4 分支，`--test-threads=1` 跑两条 periodic 用例 | 3 次 | 1 pass / **1 failed（`..._again_after_one_full_cycle` `:648`）** / 1 pass |
| WP4 分支，**整个 workspace**（首跑） | 1 次 | **1 failed（`..._at_least_once_...` `:603`）**，`EXIT=101` |
| WP4 分支，**整个 workspace**（重跑 2） | 1 次 | **全绿** `EXIT=0`（80 二进制 / 1085 / 0 / 2） |
| WP4 分支，**整个 workspace**（`--no-fail-fast`） | 1 次 | **全绿** `EXIT=0`（80 二进制 / 1085 / 0 / 2） |
| 基线 `6c093f1` 独立副本（工作树外），periodic 两条 | 3 次 | 3 / 3 全 passed |
| 基线 `6c093f1` 独立副本（工作树外），**整个 workspace** | 1 次 | **全绿** `EXIT=0`（79 二进制 / 1074 / 0 / 2） |

**诚实说明**：WP4 分支整仓跑 1/3 命中、2/3 全绿；基线 1/1 整仓跑全绿、3/3 单跑全绿。**这组采样本身不足以断言「基线绝不会命中」**——竞态的两侧抽到不同结果完全正常，基线全绿也只是抽到了好的一侧（Round-1 的 reviewer 在基线上单跑 3 次同样全绿，本轮基线整仓跑也全绿，但两边的失败都在 WP4 侧被观察到过）。**定案靠 §5.3 的结构性证据 + §四 的 blob 同一性**，而不是靠采样比例：

1. `crates/app/tests/daemon_lifecycle.rs` 在 `6c093f1` 与 `HEAD` 是**同一个 blob id**，产生断言的那段代码逐字节相同；
2. `crates/app/**` 自基线起零 diff；
3. 断言的对象是 **app 自己的后台任务日志**，与 `workspace_alias` 列、v6 迁移段、读投影没有任何调用关系；
4. 该用例走的是**全新临时数据目录**，`migrate()` 中 `if !new_database && file_version < FILE_FORMAT_VERSION`（`migrate.rs:1006`）为假 ⇒ **v6 升级段一次都不执行**，WP4 相对基线在这条路径上的唯一增量是 `OWNED_SCHEMA_V1` 里多一列 `workspace_alias` 的建表——一个只改 CREATE TABLE 文本、不改行数不改时序的差异。

**结论：这是 F-03 的同一处既有竞态（同一文件、同一「start() 返回 ≠ 后台首轮已落盘」的时序假设），不是 WP4 引入，也不因 WP4 而加重到可归因的程度。** 按 Round-1 的处置建议，仍应在集成期（U1 单元或独立修复包）为 `:603` 与 `:648` 两处补「轮询等待首条 `daemon.maintenance`（带上限）」，**不得在 WP4 分支上顺手改**。

### 5.5 红窗口的最终判定

首跑命中 §五 的既有竞态后，重跑整个 workspace **两次**（一次默认 fail-fast、一次 `--no-fail-fast`），**两次都 EXIT=0**，数字完全一致：

```
WP4 (1a60dc2) 重跑 2（默认 fail-fast）        EXIT=0
WP4 (1a60dc2) 重跑 3（--no-fail-fast）        EXIT=0
BASE (6c093f1) 独立副本同命令                EXIT=0
80 个测试二进制（"     Running" 行）
92 条 "test result:" 行
1085 passed / 0 failed / 2 ignored（crash_child、regenerate_v1_fixture，均为显式 #[ignore]）
"test result: FAILED" 出现 0 次
"panicked at" 出现 0 次
```

`workspace_alias` 测试二进制本身 **6 passed / 0 failed**（含新用例）：

```
running 6 tests
test a_mode_change_commit_also_persists_the_workspace_alias ... ok
test the_resume_flow_neither_reads_nor_rewrites_the_alias ... ok
test a_null_alias_is_never_back_filled_from_the_canonical_path ... ok
test the_written_alias_round_trips_verbatim_and_an_unwritten_row_stays_none ... ok
test v5_database_upgrades_to_v6_by_appending_a_null_workspace_alias_column ... ok
test the_display_name_falls_back_to_the_alias_and_survives_a_re_pointed_alias ... ok
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

（1085 = Round-1 的 1084 + F-04 新增的 1 条，数字自洽。）

### 5.6 基线对照（同一命令的完整 workspace 跑）

为排除「WP4 让这条路径整体变慢」的残余可能，在**工作树外**的基线 `6c093f1` 副本上跑了**同一条** `cargo test --locked --workspace --all-features`：

```
BASE (6c093f1) EXIT=0
  "     Running" 行数：79   "test result:" 行数：91
  passed=1074  failed=0  ignored=2
  "test result: FAILED" 出现 0 次；"panicked at" 出现 0 次
  完整日志：D:\Project\acp-remote-wt\wp4r2-base-test.log

WP4 (1a60dc2) EXIT=0（重跑 2 与 --no-fail-fast 各一次，两次数字完全一致）
  "     Running" 行数：80   "test result:" 行数：92
  passed=1085  failed=0  ignored=2
```

79 → 80 个二进制、1074 → 1085 条测试，差额 11 条 = WP3 + WP4 引入的新用例（含 F-04 的 1 条），两边 `failed=0`。基线这一跑没有命中竞态，但**这不构成「基线免疫」的证明**——真正的结论依据是 §5.3 的结构证据与 §四 的 blob 同一性：**产生断言的代码与被断言的调度行为都与本次存储层变更无关**。

---

## 六、Round-1 判据的独立复核（承重项重跑，不沿用上轮结论）

| 判据 | 本轮独立证据 | 结论 |
|---|---|---|
| 1 纯追加 / 无重建 | `migrate.rs:652-654` `V6_UPGRADE_OWNED` 全仓唯一一条 `ALTER TABLE owned_session ADD COLUMN workspace_alias TEXT;`（`grep -c "ADD COLUMN workspace_alias"` 全 `crates/` 只有 `migrate.rs` = 1，其余文件全 0）；无 `owned_session` 的 `RENAME` / 12-step 重建段 | ✅ |
| 1 单事务 + 连续链 | `migrate.rs:968` `begin_with("BEGIN IMMEDIATE")`；`:1006` 的 `if !new_database && file_version < FILE_FORMAT_VERSION` 块内依次 `file_version < 2/3/4/5/6` 五个守卫（`:1020-1036`），v6 在链尾；`mark_schema_versions`（`:1038`）、`PRAGMA user_version`（`:1057-1060`）、`tx.commit()`（`:1072`）同事务；`file_version == 6` 时整块跳过 | ✅ |
| 2 旧行 `NULL` 且未分组 | `workspace_alias.rs` 的 v5→v6 用例断言其余 14 列逐字节不变 + `quote(workspace_alias) == "NULL"` + 摘要 `workspace()` 为 `None`；`migration.rs:1137` 附近另断言三列皆 `NULL` | ✅ |
| 3 无 path→alias 反查 | 全 crate grep：`src/` 中 `owned_workspace` 的唯一读关联是 `session_store.rs:67-68` `LEFT JOIN owned_workspace w ON w.alias = s.workspace_alias`（**按别名**）；`src/` 中无任何 SQL 用 `canonical_path` 与 `owned_session` 的列比较；`workspace_from_row`（`:476-479`）在 `workspace_alias` 为 `NULL` 时**第一件事**就是 `return Ok(None)`；`tests/workspace_alias.rs:604-606` 的 `ON w.canonical_path = s.workspace_cwd` 是**反向判别式的自检**（先证明「若实现反查则必失败」的前提），非产品代码 | ✅ |
| 4 展示名 + 别名回退 | `workspace_from_row:484-485`：`opt_text("workspace_display_name")?.unwrap_or_else(\|\| alias.as_str().to_owned())`；用例三段（已登记取 `display_name` / 别名重指向归属不变且 `workspace_cwd` 保持原值 / 目录删除后回退为别名本身）各自有断言 | ✅ |
| 5 恢复不读不改别名 | `load_recovery`（`:2110-2113`）SELECT 列为 `agent_id, agent_name, agent_session_id, workspace_cwd`，**不含 `workspace_alias`**，构造的 `SessionRecoveryRecord` 也无该字段；写入侧 `?7/?9` 的 `CASE WHEN ... IS NULL THEN workspace_alias` 使 `None` = 不改该列，恢复流程永远传 `None` | ✅ |
| 6 文档 | `node scripts/check-contract-drift.mjs` **EXIT=0**（§七）；另做**独立**的括号配平 + 注释剥离 + 空白归一比对：`owned_session` **IDENTICAL**、`owned_workspace` **IDENTICAL**；§9 判据 1（`:1363`）与判据 28（`:1393`）版本链均为 v1→…→v6；§11.3（`:1490`）「过新」取值 `user_version = 7`；§7.2（`:887`）`v6 = 6` / owned 6 / imported 3；§7.2 新增两条 `[决定]`（`:898` v5→v6 段、`:899` 读取投影）；§7.3 DDL（`:929`）含 `workspace_alias TEXT -- … NULL = 未分组（不得按 workspace_cwd 反查补齐）`；文件头 `> 版本：0.17` 为追加在 0.15/0.16 之后，两行完整保留 | ✅ |
| 7 红窗口关闭 | `cargo test --locked --workspace --all-features --no-fail-fast` **EXIT=0**（1085/0/2）；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` **EXIT=0**（0 warning） | ✅（`daemon_lifecycle` 竞态见 §五，判定为既有） |
| 8 无范围越界 | `8af5d21..HEAD` 只有 1 个文件；`347399f..HEAD` 11 个文件全在 WP4 声明范围（`crates/storage-sqlite/**` + `docs/CORE_PORTS_AND_STORAGE.md`）；`crates/app`、`crates/core`、`crates/sync-protocol` 零 diff | ✅ |
| 9 无跨工作包耦合 | `git log --oneline 6c093f1..HEAD` = `1a60dc2` / `8af5d21` / `347399f` / `3603612`；`347399f..HEAD` 未触碰 WP1/WP2 资产（`compatibility/**`、`schemas/**`、`fixtures/**`、`crates/identity-auth/**`、`crates/core/**`） | ✅ |

---

## 七、我实际运行的命令与原始输出

```
$ cd D:/Project/acp-remote-wt/wp4-storage

# 1) 契约漂移门禁
$ node scripts/check-contract-drift.mjs
contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 96 个方法签名与 crates\core\src\ports.rs 一致
EXIT=0

# 附带的另外两个门禁
$ node scripts/check-doc-links.mjs
doc links OK: 403 relative links, 8689 section refs across 486 markdown files
EXIT=0
$ node scripts/check-crate-boundaries.mjs
crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）
EXIT=0

# 2) clippy（红窗口右半）
$ CARGO_TARGET_DIR=D:/Project/acp-remote-wt/target-wp4-review2 \
    cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
EXIT=0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 16.00s
  warning/error 计数：0

# 3) workspace 测试（红窗口左半，首跑命中 §五 的既有竞态）
$ CARGO_TARGET_DIR=D:/Project/acp-remote-wt/target-wp4-review2 \
    cargo test --locked --workspace --all-features
EXIT=101
  唯一失败：app --test daemon_lifecycle ::
         the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown（daemon_lifecycle.rs:603）
  完整日志：D:\Project\acp-remote-wt\wp4r2-test.log

# 4) workspace 测试 · 重跑 2（默认 fail-fast）
$ CARGO_TARGET_DIR=D:/Project/acp-remote-wt/target-wp4-review2 \
    cargo test --locked --workspace --all-features
EXIT=0
  "     Running" 行数：80
  "test result:" 行数：92
  passed=1085  failed=0  ignored=2
  "test result: FAILED" 出现 0 次
  完整日志：D:\Project\acp-remote-wt\wp4r2-test-run2.log

# 5) workspace 测试 · 重跑 3（--no-fail-fast）
$ CARGO_TARGET_DIR=D:/Project/acp-remote-wt/target-wp4-review2 \
    cargo test --locked --workspace --all-features --no-fail-fast
EXIT=0
  "     Running" 行数：80
  "test result:" 行数：92
  passed=1085  failed=0  ignored=2（crash_child、regenerate_v1_fixture，均为显式 #[ignore]）
  "test result: FAILED" 出现 0 次；"panicked at" 出现 0 次
  workspace_alias 二进制：6 passed / 0 failed
  完整日志：D:\Project\acp-remote-wt\wp4r2-test-nofailfast.log

# 6) 基线 6c093f1 副本（工作树外）的同一条整仓命令
$ cd D:/Project/acp-remote-wt/wp4r2-base
$ CARGO_TARGET_DIR=D:/Project/acp-remote-wt/target-wp4-review2-base \
    cargo test --locked --workspace --all-features
EXIT=0
  "     Running" 行数：79
  "test result:" 行数：91
  passed=1074  failed=0  ignored=2
  "test result: FAILED" 出现 0 次；"panicked at" 出现 0 次
  完整日志：D:\Project\acp-remote-wt\wp4r2-base-test.log

# 7) F-04 变异（工作树外的副本 D:\Project\acp-remote-wt\wp4r2-mut，Set 变体 ?7/?8/?9 → ?5/?6/?7）
$ cd D:/Project/acp-remote-wt/wp4r2-mut
$ CARGO_TARGET_DIR=D:/Project/acp-remote-wt/target-wp4-review2 \
    cargo test --locked -p storage-sqlite --all-features
EXIT=101
  workspace_alias 二进制：5 passed / 1 failed
  唯一失败：a_mode_change_commit_also_persists_the_workspace_alias
  panicked at crates\storage-sqlite\tests\workspace_alias.rs:513:5
    left: "acp-session-1"
   right: "acp.remote"
  其余全部 storage-sqlite 测试二进制仍全绿（admin_store 43、migration 12、resume_columns 6、
  commit 18、imported 10、retention 8、session_version_rule 3、enum_coverage 2 …）
  完整日志：D:\Project\acp-remote-wt\wp4r2-mut-test.log
  ★ 变异前后工作树 HEAD^{tree} 均 = a8f8cc4b062a8b8042e76fbfbc6c53c1e3c00da9，
    git status --porcelain 与 git diff HEAD 均为空

# 8) daemon_lifecycle 采样
WP4 分支，--exact the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown × 12
  → 12 passed / 0 failed
WP4 分支，--test-threads=1 the_periodic_task_runs × 3
  → RUN1 2 passed；RUN2 1 passed / 1 FAILED（..._again_after_one_full_cycle, :648）；RUN3 2 passed
基线 6c093f1 副本，--test-threads=1 the_periodic_task_runs × 3
  → 3 / 3 全 passed
基线 6c093f1 副本，整仓 cargo test --locked --workspace --all-features × 1
  → EXIT=0（79 二进制 / 1074 passed / 0 failed / 2 ignored）

# 9) 范围核对
$ git diff --name-status 8af5d21..HEAD        → M crates/storage-sqlite/tests/workspace_alias.rs（仅此一个）
$ git diff --name-only 8af5d21..HEAD -- docs/                       → 空
$ git diff --name-only 8af5d21..HEAD -- crates/storage-sqlite/src/  → 空
$ git diff --stat 347399f..HEAD -- crates/app/                      → 空
$ git diff --stat 6c093f1 HEAD -- crates/app/                       → 空
$ git rev-parse 6c093f1:crates/app/tests/daemon_lifecycle.rs HEAD:… → 同一 blob 9d43568…

# 10) U+FFFD 扫描（6c093f1..HEAD 的 20 个文件）
files with FFFD: 0   （workspace_alias.rs: 8af5d21 = 3 → 1a60dc2 = 0）

# 11) §7 DDL 独立比对（括号配平 + 去注释 + 空白归一，不依赖 drift 脚本）
owned_session:   IDENTICAL
owned_workspace: IDENTICAL

# 12) hook 路径（无 --no-verify 旁路的证据）
$ git config --get core.hooksPath        → .husky/_
$ git rev-parse --git-path hooks          → .husky/_
$ ls .husky/_/                            → h husky.sh pre-commit commit-msg …（钩子已 install）
$ cat .husky/pre-commit                   → node scripts/pre-commit.mjs
$ cat .husky/commit-msg                   → npx --no -- commitlint --edit "$1"
$ ls node_modules/@commitlint/cli/cli.js  → 存在（commit-msg 钩子能真正执行）
$ git rev-parse HEAD                      → 1a60dc2…（提交在分支上、工作树干净）
```

**清理（已完成并实测）**：变异副本 `D:\Project\acp-remote-wt\wp4r2-mut` 与基线副本 `D:\Project\acp-remote-wt\wp4r2-base` **已删除**（`ls` 确认 `No such file or directory`）。构建目录 `target-wp4-review2`、`target-wp4-review2-base` 保留在 `D:\Project\acp-remote-wt\` 下；7 个原始日志 `wp4r2-*.log` 一并保留。删除副本后再次核对工作树：`HEAD^{tree}` 仍为 `a8f8cc4b…`、`git status --porcelain` 为空、`git diff HEAD` 为空——**本轮全程未修改任何源码或文档**。

---

## 八、发现

### O-01 —— 新用例的文档注释里有一处把 `?5/?6` 说成「模式」、但没有点明它们与恢复两列共用编号

- **严重度**：low（仅注释措辞，无行为影响）
- **位置**：`crates/storage-sqlite/tests/workspace_alias.rs:432-436`（新用例的文档注释）
- **问题**：注释写「`?5/?6` 是模式，`?7/?8/?9` 才是恢复两列与目录归属列」。在**正确**实现下这是对的（`?5`/`?6` 只被 `current_mode_id`/`current_mode_name` 用到，`?7`/`?8`/`?9` 是恢复两列与归属列）；但紧接着的下一句把失效模式描述成「绑定落到**别的列**上」，而把这条用例**真正杀死**的那个变异（`?7/?8/?9` → `?5/?6/?7`）造成的恰恰是「同一个下标被两个表达式共享」——此时 `?5` 既当 `current_mode_id` 又当 `agent_session_id`。注释没有把这层「编号共享」讲透，读者按字面会以为 `?5` 只会「落到别的列」。
- **影响**：零行为影响。判别力由 §二 的实测证明，不依赖这段注释。仅是可读性/可维护性上的措辞精度问题。
- **最小修复**（可选，不阻塞）：在 `:432-436` 补半句「错位后同一个下标会被两个 SET 表达式共享（如 `?5` 同时是 `current_mode_id` 与 `agent_session_id`），于是恢复两列与归属列会取到模式的值——本用例的失败信息 `left: "acp-session-1"` 正是这个形态」。

**无其他发现。** 未发现阻塞项或需要在集成前修掉的问题。

---

## 九、结论汇总

| 项 | 判定 | 关键证据 |
|---|---|---|
| **F-01** | ✅ 已修，字节级精确 | `8af5d21`=3 → `1a60dc2`=0 个 U+FFFD；第 5 行只删 `ef bf bd ef bf bd ef bf bd`、补 `e7 9a 84`，其余字节逐一相同；`6c093f1..HEAD` 的 20 个文件全部 0 个 U+FFFD 且 UTF-8 合法 |
| **F-04** | ✅ 已修，**非空洞** | `a_mode_change_commit_also_persists_the_workspace_alias`（`:437-554`）同批提交 `ModeChange::Set` + `Some(ALIAS)`，经 `load`/`list`/`load_recovery` + 裸 SQL 五路断言；变异 `?7/?8/?9→?5/?6/?7` 下 **5 passed / 1 failed**（`left: "acp-session-1", right: "acp.remote"`），既有五条全放过同一变异——不对称成立 |
| 变异后源码树 | ✅ 字节同一 | `HEAD^{tree}` 恒为 `a8f8cc4b…`；`git status --porcelain` 与 `git diff HEAD` 均为空 |
| **无回归** | ✅ | `8af5d21..HEAD` 只有 `workspace_alias.rs` 一个文件（+137/−2）；`src/`、`docs/` 零 diff；既有五条用例无一处被改 |
| **F-02 / F-03 未被静默修掉** | ✅ | `crates/app/**` 在 `347399f..HEAD` 与 `6c093f1..HEAD` 均零 diff；`daemon_lifecycle.rs` 两端同一 blob `9d43568…` |
| **Round-1 判据 1–9** | ✅ 全部承重项重跑仍成立 | 见 §六 表 |
| **红窗口** | ✅ 关闭 | `cargo test --locked --workspace --all-features` **重跑 2 次均 EXIT=0**（默认 fail-fast 一次 + `--no-fail-fast` 一次，各 80 二进制 / 1085 passed / 0 failed / 2 ignored，数字完全一致）；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` **EXIT=0**（0 warning）。首跑的那一次 EXIT=101 已定性为既有竞态，见下一行 |
| **drift 门禁** | ✅ | `check-contract-drift.mjs` EXIT=0；另做独立 DDL 括号配平比对 `owned_session`/`owned_workspace` 均 IDENTICAL |
| **`daemon_lifecycle` 命中项** | **既有（F-03 同一根因），非本次引入** | 结构证据：`daemon.rs:1270` spawn 后台任务 vs `:1284` `daemon.ready`，中间无同步；首轮日志在 `maintenance_tick` 末尾 `:692`，需走完 `prune`+`expire_pairings`+`sweep_orphans` 三次存储往返。归属证据：`crates/app/**` 零 diff、`daemon_lifecycle.rs` 两端同一 blob `9d43568…`；该用例走全新临时目录 ⇒ `migrate.rs:1006` 的 `if !new_database && …` 为假，**v6 段一次都不执行**。采样：WP4 侧整仓 1 命中 / 3、基线侧整仓 1 未命中 / 1、单跑 3 未命中 / 3——采样不作定案依据，定案靠结构与归属 |
| **Hooks** | ✅ 无 `--no-verify` 旁路 | `core.hooksPath = .husky/_` 且 `git rev-parse --git-path hooks` 同解；`.husky/_/` 已 install（`h`/`pre-commit`/`commit-msg` 存在）；`node_modules/@commitlint/cli/cli.js` 存在 ⇒ `commit-msg` 钩子可真实执行；提交 `1a60dc2` 在分支上、Conventional Commits 前缀 `test(storage):` 合规、工作树干净 |
| 新发现 | 1 项 low（O-01，注释措辞） | 见 §八；不阻塞 |

---

## 十、给集成期的建议

1. **WP4 可以放行。** F-01 与 F-04 已闭合，且 F-04 的闭合质量高于「补一条断言」——变异实验证明新用例是那条 bind 编号陷阱路径的**唯一防线**。
2. **F-02 与 F-03 仍未处置**，建议在 U1 集成单元或单开修复包处理，不要在 WP4 分支上顺手改：
   - F-02：`crates/app/src/daemon.rs` 里 `Composition::assemble`（含 `SqliteStore::open` → `migrate`）早于 `DaemonLock::acquire`，与 `docs/CORE_PORTS_AND_STORAGE.md:1490`「migration 在取得单实例锁后、监听前完成」的措辞不符——「监听前」成立、「锁后」不成立。二选一：改顺序，或改文档措辞。
   - F-03：`crates/app/tests/daemon_lifecycle.rs:603` 与 `:648` 两处都假设「`daemon.start()` 返回 ⇒ 后台启动初清理已落盘」，代码里不成立。建议两处都改成「轮询等待首条 `daemon.maintenance`（带上限）」。**本轮实测该竞态在负载下确实会命中**（首跑 workspace 直接 EXIT=101），集成期应在 CI 机器上同样复跑 `--no-fail-fast` 以免被误判为变更引入。
3. **PV0 的分支级 hook 证据缺口已由本轮关闭**：两条工作区级命令各自独立跑出 EXIT=0，且 `core.hooksPath` 与 `.husky/_/` 的 install 状态已核实。