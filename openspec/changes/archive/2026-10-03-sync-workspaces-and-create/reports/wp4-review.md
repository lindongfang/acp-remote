# WP4 独立检视报告（CR4）

- **被检视版本**：`8af5d21`（`feat(storage): v6 迁移与目录归属列的落盘与投影`），分支 `feat/sync-storage-v6`
- **比对基线**：WP3 已验收提交 `347399f45a3df3093c9f45c3f98769ba96f846c8`；整体变更的存储相关部分对照 `6c093f145aa3d69dcd573a6d94e31b692acb5d4b`
- **工作树**：`D:\Project\acp-remote-wt\wp4-storage`
- **构建目录**：`CARGO_TARGET_DIR=D:\Project\acp-remote-wt\target-wp4-review`
- **检视方式**：只读。未修改任何源码或文档（`git status --porcelain` 在结束时为空）。所有临时目录（变异测试副本、基线副本）已删除。
- **契约来源**：`openspec/changes/sync-workspaces-and-create/design.md` D6、`proposal.md`、`specs/storage-schema-v2-migration/spec.md`、`specs/workspace-resolution/spec.md`；合同落点 `docs/CORE_PORTS_AND_STORAGE.md` §7/§7.2/§7.3/§9/§11.3。

## 结论

**PASS**

WP4 交付的全部内容满足阻塞判据 1–9。迁移是纯追加、读投影按别名 LEFT JOIN 且不回退到路径、恢复路径不碰该列、文档与 `migrate.rs` 逐字一致、红窗口确实关闭、文件范围无越界、无跨工作包耦合。

发现 3 项非阻塞问题（1 项 medium、2 项 low），均不构成 FAIL，见下文「发现」。其中 F-01 是本次唯一需要 coder 修的实际缺陷（一处 UTF-8 替换字符污染）；F-02/F-03 是既有代码的属性，WP4 未引入也未加重，但与 WP4 的改动区域相邻，记录供集成期处置。

---

## 一、文件枚举与范围核对（判据 8）

`git diff --name-only 347399f..HEAD` 的完整输出，11 个文件：

| # | 文件 | 声明范围 | 判定 |
|---|---|---|---|
| 1 | `crates/storage-sqlite/src/migrate.rs` | ✅ 在范围内 | 符合 |
| 2 | `crates/storage-sqlite/src/session_store.rs` | ✅ 在范围内 | 符合 |
| 3 | `crates/storage-sqlite/tests/commit.rs` | ✅ `tests/` | 符合（仅补 `workspace_alias: None`） |
| 4 | `crates/storage-sqlite/tests/contract_v03.rs` | ✅ `tests/` | 符合（同上） |
| 5 | `crates/storage-sqlite/tests/enum_coverage.rs` | ✅ `tests/` | 符合（同上） |
| 6 | `crates/storage-sqlite/tests/migration.rs` | ✅ `tests/` | 符合（重命名 + 列数/版本号跟进） |
| 7 | `crates/storage-sqlite/tests/resume_columns.rs` | ✅ `tests/` | 符合（重命名 + 三列跟进） |
| 8 | `crates/storage-sqlite/tests/retention.rs` | ✅ `tests/` | 符合（仅补 `None`） |
| 9 | `crates/storage-sqlite/tests/session_version_rule.rs` | ✅ `tests/` | 符合（仅补 `None`） |
| 10 | `crates/storage-sqlite/tests/workspace_alias.rs` | ✅ `tests/`（新增） | 符合 |
| 11 | `docs/CORE_PORTS_AND_STORAGE.md` | ✅ 声明的文档区域 | 符合，见下 |

**未越界**：`git diff --stat 347399f..HEAD -- crates/app/`、`-- crates/core/`、`-- crates/sync-protocol/` 全部为空。`crates/app`、`crates/core`、`crates/server`、`crates/identity-auth`、所有 `compatibility/`、所有 `schemas/`、所有 `fixtures/`、`scripts/` 均未被本工作包触碰。工作树无未跟踪改动（`git status --porcelain` 为空）；`reports/` 被 `.gitignore:32` 忽略，故本次报告不进入提交。

**文档改动区域核对**（`git diff -U0` 的 hunk 头）：

```
@@ -25,0 +26 @@      → 文件头追加 `> 版本：0.17`（在 0.15/0.16 之后追加，未覆盖既有行）
@@ -872 +873 @@       → §7 标题 v5 → v6
@@ -886 +887 @@       → §7.2 版本常量 5/5 → 6/6
@@ -888 +889 @@       → §7.2 升级判据（链尾补 v6 段、user_version = 6 跳过）
@@ -893 +894 @@       → §7.2 第 4 条版本落盘取值 '6'
@@ -896,0 +898,2 @@   → §7.2 新增两条 [决定]（v5→v6 升级段、目录归属的读取投影）
@@ -899,2 +902,2 @@   → §7.2 夹具链、§7.2 回滚
@@ -925 +928,2 @@     → §7.3 owned_session DDL 追加 workspace_alias
@@ -1359 +1363 @@     → §9 判据 1 的版本链
@@ -1389 +1393 @@     → §9 判据 28 的版本链 + ⑤⑥ 两条新子判据
@@ -1486 +1490 @@     → §11.3 「过新」取值 6 → 7
```

全部落在声明的 §7 标题/§7.2/§7.3、§9 判据 1 与 28、§11.3、文件头版本行之内，**§3.1/§3.6/§5/§5.2（WP3 的区域）与 §11.1–§11.2、§11.4–§11.9 未被编辑**。

---

## 二、逐条阻塞判据核验

### 判据 1 — 迁移是纯追加、单事务、幂等 ✅

- **纯追加**：`migrate.rs:652-654` 的 `V6_UPGRADE_OWNED` 只有一条语句 `ALTER TABLE owned_session ADD COLUMN workspace_alias TEXT;`。可空、无默认值。文件中**没有** v6 的 12-step 重建段（重建段只有 `V2_UPGRADE_OWNED`/`V2_UPGRADE_IMPORTED`/`V3_UPGRADE_OWNED`/`V3_UPGRADE_IMPORTED`，都作用于审计表与 `imported_import`，与 `owned_session` 无关）。
- **连续升级链**：`migrate.rs:1006-1039` 的 `if !new_database && file_version < FILE_FORMAT_VERSION` 块内依次 `file_version < 2/3/4/5/6` 五个守卫段，v6 段在最后。v1…v5 库都能走到 v6。
- **同事务**：`migrate.rs:968` `let mut tx = write.begin_with("BEGIN IMMEDIATE")`；v6 段在 `:1034-1036` 于同一 `tx` 上执行；`mark_schema_versions`（`:1038`）、`PRAGMA user_version = 6`（`:1057-1060`）、`tx.commit()`（`:1072`）全在同一事务内。版本号只在全部段结束后统一落盘一次，符合 spec「序号只回填一次、中间版本不单独落盘」。
- **版本常量**：`FILE_FORMAT_VERSION = 6`（`:19`）、`OWNED_SCHEMA_VERSION = 6`（`:21`）、`IMPORTED_SCHEMA_VERSION = 3`（`:23`）——imported 家族按 D6 保持 3，正确。
- **v6 库跳过全部段**：`file_version = 6` 时 `file_version < FILE_FORMAT_VERSION` 为假，整块跳过；`file_version != FILE_FORMAT_VERSION` 也为假，不写 `PRAGMA`。二次打开零 DDL、零行写。
- **DDL 逐字节稳定**：`OWNED_SCHEMA_V1` 全部语句是 `CREATE TABLE/INDEX IF NOT EXISTS`；升级路径是 `ALTER TABLE ADD COLUMN`（原地追加），不是 `CREATE TABLE …_vN` + `RENAME`（后者会被 SQLite 记成带引号表名）。
- **运行位置**：`SqliteStore::open`（`session_store.rs:115-134`）内 `migrate::migrate` 在 `quick_check` 之后、构造 `SqliteStore` 之前，即启动序列的第 2 步（`daemon.rs:1157` `Composition::assemble`），在 `NetIngress::start`（`:1201`，开始监听）之前，满足「监听前」。**见 F-02 关于「单实例锁之后」这一半的偏差**（既有属性，WP4 未引入）。

证据（实测）：`migration.rs` 12 passed / 0 failed / 1 ignored（`regenerate_v1_fixture` 是显式 `#[ignore]` 的夹具生成器），其中 `v3_database_upgrades_to_v6_...`、`v4_database_upgrades_to_v6_...`、`current_version_database_is_untouched_by_two_consecutive_starts`、`second_open_of_an_upgraded_database_rewrites_nothing`、`a_failed_upgrade_rolls_back_to_v1` 全部通过。

### 判据 2 — 旧行得 `NULL` 且被当作未分组 ✅

`workspace_alias.rs:177-311` 的 `v5_database_upgrades_to_v6_...`：造一条带 `agent_session_id='acp-session-0001'`、`workspace_cwd=<已登记目录的路径>`、`version=7`、`state='waiting_input'` 的真实 v5 会话行，并且**把该目录真的登记进 `owned_workspace`**（`:199`），然后升级并断言：

- ① 其余 14 列逐字节不变（`:252-270` 拼接全部列比对）；
- ② `quote(workspace_alias) == "NULL"`（`:273-277`，`quote()` 能区分 `NULL` 与 `''`）；
- ③ 摘要的 `workspace()` 为 `None`（`:230-234`）。

即「旧行 = `NULL` = 未分组，且不被反查补齐」被同一条用例同时钉住。`migration.rs:1137-1156` 的 v4→v6 用例另断言三列皆 `NULL` 且无任何一列被空串填充。

### 判据 3 — 无任何 path→alias 反查 ✅

对 `crates/storage-sqlite` 全文（`src/` 与 `tests/`）grep `workspace_alias` / `owned_workspace` / `workspace_cwd` / `canonical_path` / `cwd` / `canonical` / `normalize`：

- `src/` 中唯一提到 `owned_workspace` 的读路径是 `session_store.rs:68` 的 `LEFT JOIN owned_workspace w ON w.alias = s.workspace_alias`——按**别名**关联，不是按路径。
- `src/` 中 `canonical_path` 只出现在 `admin/local_config.rs`（workspace 记录自身的 CRUD，`WORKSPACE_COLUMNS` 与 upsert）与 `migrate.rs:320` 的建表语句里；**没有任何一条 SQL 用 `canonical_path` 与 `owned_session` 的任何列做比较**。
- `workspace_from_row`（`session_store.rs:476-492`）在 `workspace_alias` 为 `NULL` 时**第一件事**就是 `return Ok(None)`，根本不会去查别的东西。
- `tests/workspace_alias.rs:470` 里有一处 `ON w.canonical_path = s.workspace_cwd`，但它是**反向判别式的自检**（先证明「若实现反查则此用例必失败」的前提成立），不是产品代码的反查。

### 判据 4 — 展示名解析的三种边界 ✅

`workspace_alias.rs:486-608` 的 `the_display_name_falls_back_to_the_alias_and_survives_a_re_pointed_alias`，三段各自有断言：

| 场景 | 断言 | 行 |
|---|---|---|
| 别名已登记 | `display_name == "Acp Remote"`（取 `owned_workspace.display_name`，不是别名、不是路径） | `:523-527` |
| 同一别名重指向别的目录 | `alias` 不变、`display_name` 跟随改名、**且 `owned_session.workspace_cwd` 仍是创建时的原值** | `:555-578` |
| 目录被删除（别名不再登记） | LEFT JOIN 接不上 → 回退为**别名本身**，分组不丢，也不泄漏路径 | `:601-606` |

「别名仍被携带、只是展示名退化」这一点由 `:601` 的 `workspace.alias().as_str() == ALIAS` 单独钉住。

### 判据 5 — 恢复路径不读不改别名 ✅

- `load_recovery`（`session_store.rs:2110-2135`）的 SELECT 列是 `agent_id, agent_name, agent_session_id, workspace_cwd`——**不含 `workspace_alias`**，构造的 `SessionRecoveryRecord` 也没有该字段（WP3 的 core 侧同样没有）。WP4 未触碰该函数（diff 中无 `load_recovery` 相关 hunk）。
- 写入侧：别名走与恢复两列同形的窄写 `CASE WHEN ?N IS NULL THEN workspace_alias`（`session_store.rs:1141`、`:1151`），恢复流程永远传 `None`，因此不可能覆写。
- `workspace_alias.rs:617-724` 的 `the_resume_flow_neither_reads_nor_rewrites_the_alias` 端到端验证：先写入别名 + cwd，走 `load_recovery`（断言能读到 cwd 原文），再做一次不带 `workspace_alias` 的窄写提交，最后比对 `quote(workspace_alias)` 前后逐字节相同且 `workspace_cwd` 未被覆盖。

### 判据 6 — 文档 ✅

- `node scripts/check-contract-drift.mjs` → **exit 0**：
  ```
  contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 96 个方法签名与 crates\core\src\ports.rs 一致
  ```
- §7 的 `owned_session` 与 `owned_workspace` 两段 DDL 我另做了独立的括号配平 + 空白归一比对：**IDENTICAL / IDENTICAL**。
- §9 判据 1（`:1363`）与判据 28（`:1393`）的版本链均已改为 v1→…→v6；判据 28 新增 ⑤⑥ 两条子判据，逐条对应 spec 的「v5 库升级后既有会话行的 `workspace_alias` 为 `NULL`」与「别名写入后按字节读回 / 展示名回退 / 恢复不改写」。
- §11.3（`:1490`）「过新」取值改为 `user_version = 7`，即 `FILE_FORMAT_VERSION + 1`。实现侧 `migration.rs:307` 与 `resume_columns.rs:509` 都用 `FILE_FORMAT_VERSION + 1` 动态计算，不硬编码，改版本时不会漂移。
- 文件头 `> 版本：0.17` 是**追加**在 0.15/0.16 之后（`:26`），既有两行完整保留。全文 UTF-8 合法（round-trip 字节相同、零个 U+FFFD）。
- 附带跑：`check-doc-links.mjs` exit 0（403 相对链接 / 8663 章节引用）、`check-crate-boundaries.mjs` exit 0、`agentic-gate.mjs` exit 0（19 passed / 0 failed，`spec/storage-schema-v2-migration` 与 `spec/workspace-resolution` 均 ✓）。

### 判据 7 — 红窗口确实关闭 ✅（本次最重要的检查）

两条命令都用指定的 `CARGO_TARGET_DIR` 独立跑过，不是复用 coder 的日志。

**`cargo test --locked --workspace --all-features` → exit 0**
- 80 个测试二进制、92 条 `test result` 行、**1084 个测试通过、0 失败**；2 个 `ignored`（`crash_child`、`regenerate_v1_fixture`，都是显式 `#[ignore]` 的辅助用例，非本次引入）。
- 无 crate 被跳过：`core`、`storage-sqlite`、`server`、`app`、`agent-host`、`identity-auth`、`identity-keystore`、`acpr-wire`、`acpr-transcript`、`sync-protocol` 全部出现在运行列表中。完整原始输出保存在 `D:\Project\acp-remote-wt\wp4-test-run3.log`。

**`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` → exit 0**
- `Finished \`dev\` profile ... in 15.89s`，warning/error 计数 0。完整原始输出在 `D:\Project\acp-remote-wt\wp4-clippy.log`。

首次运行时 `app --test daemon_lifecycle::the_periodic_task_runs_again_after_one_full_cycle` 失败过一次（`left: 0, right: 1`，断言「启动初清理恰好一轮」时日志里一条 `daemon.maintenance` 都还没有）。我判定为既有竞态而非 WP4 引入，依据是实测：在 WP4 分支上单跑 3 次全绿，在**基线 `6c093f1`** 的独立副本上单跑 3 次也全绿（该用例的 60s 周期与启动日志的 flush 时序都与存储层无关；`crates/app` 在本工作包内零改动）。随后完整重跑的 exit 0 结果即为最终证据。详见 F-03。

### 判据 9 — 无跨工作包耦合 ✅

- `git log --oneline 6c093f1..HEAD` 只有三个提交：`3603612`/`347399f`（WP3）与 `8af5d21`（WP4）。`git merge-base --is-ancestor 6c093f1 HEAD` 成立——WP4 确实只叠在 WP3 之上，**不含 WP1/WP2 的改动**。
- `crates/sync-protocol/` 在 `347399f..HEAD` 内零改动；特别地 `crates/sync-protocol/tests/envelope_fixtures.rs` 在 `6c093f1..HEAD` 全程 identical——WP4 没有去「修」那些合并期才定值的向量。
- WP4 没有触碰任何 WP1/WP2 拥有的资产（`compatibility/**`、`schemas/**`、`fixtures/sync/**`、`crates/identity-auth/**`、`crates/core/**`）。

---

## 三、对 coder 自述的逐条独立复核

**(a) 两条 UPDATE 变体都改、`load_recovery` 不动 —— 判断：正确，无空洞。**

`ModeChange::Unchanged` 用 `?5/?6/?7`、`ModeChange::Set` 用 `?7/?8/?9`，两个语句各自连续编号，中间不留空。SQLite 的 `?NNN` 是按位置绑定，编号空洞会让后续 bind 落到未使用的下标上（`?7 IS NULL` 恒真 → 静默不写）——代码在 `:1132-1133` 明确写下了这个陷阱。我逐位核对了 bind 顺序：`:1157-1168` 依次 bind `state / closed_at / commit.at / session`，`Set` 时再 bind `mode_id / mode_name`，最后统一 bind `agent_session_id / workspace_cwd / workspace_alias`。对 `Unchanged`：位置 5/6/7 = 后三者 ✅；对 `Set`：位置 5/6 = mode、7/8/9 = 后三者 ✅。两个变体的编号都对。

「`NULL` 别名静默清空既有值」这个疑问：`CASE WHEN ?N IS NULL THEN workspace_alias` 的语义是 `None` = 不改该列，因此**不存在**清空路径。这与 `agent_session_id`/`workspace_cwd` 完全同形，也与 core 侧 `ports.rs:195` 的 `None` 语义一致。**但这里有一个真实的覆盖缺口**——见 F-04。

**(b) 测试跟进是否为必要后果、是否弱化断言 —— 判断：全部是必要后果，无一削弱。**

| 改动 | 是否必要 | 断言强度 |
|---|---|---|
| `migration.rs` 三个 `owned_schema_version` 断言 5→6 | 必要（常量推进的必然后果） | 不变 |
| `v3_database_upgrades_to_v5_...` → `..._to_v6_...`，追加列断言 2→3 列 | 必要（重命名匹配行为） | **增强**：从「过滤出两列」改为「断言追加的三列恰为 `owned_session` 的最后三列」且逐项校验 `(name, notnull, dflt)` |
| `resume_columns.rs` 的 `rewind_to_v4` 多 DROP 一列 | **必要**——否则 v6 段 `ADD COLUMN` 撞 duplicate column | 增强 |
| 同文件 12→15 列、cid `0..15`、DDL 尾部三列文本 | 必要 | 增强（多断言一列的类型） |
| 五个测试文件补 `workspace_alias: None` | 必要（`SessionUpdate` 新增必填字段） | 不变（纯字面量补齐） |

特别地，`migration.rs:975-979` 的新断言 `session_specs[len-3..] == appended_specs` 比原来的「过滤后比对」更强：它同时钉住了顺序，且保证没有第四列混进追加集合。

**(c) `workspace_alias.rs` 五条用例是否钉住 spec 场景、还是复述实现 —— 判断：非空洞，我用变异测试实测了判别力。**

我在工作树的独立副本上注入了五个变异体（副本已删除，源树未动）：

| 变异 | 内容 | 结果 |
|---|---|---|
| **A** | JOIN 额外加 `OR w.canonical_path = s.workspace_cwd`（只有 JOIN 侧反查） | **未失败** — 但这是**纵深防御**而非测试空洞：`workspace_from_row:477` 先判 `workspace_alias` 为 `NULL` 就 `return Ok(None)`，根本不读 JOIN 结果。要真正反查必须同时改 COALESCE。 |
| **B** | 真实反查实现：`COALESCE(s.workspace_alias, w.alias)` + 路径 JOIN | **2 条用例失败**（`a_null_alias_is_never_back_filled_from_the_canonical_path`、`v5_database_upgrades_to_v6_...`），报错精确指出 `left: Some(WorkspaceRef{...}), right: None` ✅ |
| **C** | 去掉「JOIN 接不上回退为别名」 | **1 条失败**（display-name 用例），`left: "ungrouped", right: "acp.remote"` ✅ |
| **D** | 恢复流程把别名写成 `?7`（去掉窄写守卫） | **1 条失败**（resume 用例），`left: "NULL", right: "'acp.remote'"` ✅ |
| **E** | 迁移段改成 `NOT NULL DEFAULT ''`（空串占位） | **2 条失败**（`migration.rs` 的 v3→v6 与 v4→v6 用例）✅ |
| **F** | `ModeChange::Set` 变体的三列 bind 编号整体错位两位 | **全部通过 ❌** |

结论：判据 3/4/5 与 spec「v5 库升级后既有会话行为未分组」「不反查」都有真实的判别力，不是复述实现。**唯一空洞是变异 F**，构成 F-04。

---

## 四、发现

### F-01 — `workspace_alias.rs` 文件头有 3 个 U+FFFD 替换字符（编码污染）
- **严重度**：medium
- **位置**：`crates/storage-sqlite/tests/workspace_alias.rs:5`
- **问题**：第 5 行 `//! \`migration.rs\` 已有��� v1 → v6 连续升级保留性…` 中，`已有` 与 ` v1` 之间有 **3 个连续的 U+FFFD（`EF BF BD`）** 替换字符（已用 `od -An -tx1` 逐字节确认）。原意显然是 `已有的`。
- **影响**：纯注释，不影响编译与行为（clippy/测试全绿）。但这是**本工作包新引入的**编码损坏：全仓 `git grep` U+FFFD 只有两处命中，另一处是 2026-10-02 归档的 `session-resume` 报告（历史文件，不在本次范围）。它会污染后续任何按字节比对文档/注释的工具，也会让读者看到乱码。
- **最小修复**：把 `已有���` 改为 `已有的`（删掉 3 个 `EF BF BD`，补 1 个 `的` = `E7 9A 84`）。不需要动其它任何内容。

### F-02 — migration 实际发生在单实例锁**之前**，与 §11.3/§7.2 的措辞不符
- **严重度**：low（既有属性，WP4 未引入）
- **位置**：`crates/app/src/daemon.rs:1157`（`Composition::assemble` → `SqliteStore::open` → `migrate`）早于 `:1192` 的 `DaemonLock::acquire`；文档 `docs/CORE_PORTS_AND_STORAGE.md:1490` 写的是「migration 在取得单实例锁后、监听前完成」。
- **问题**：代码顺序是「装配组合根（含 migration）→ 取锁 → 绑定 listener」。即 **「监听前」成立，「锁后」不成立**。这是 WP4 之前就有的偏差：`git show 6c093f1:crates/app/src/daemon.rs` 的行号与顺序完全相同，`git show 6c093f1:docs/...` 的同一句也是「取得单实例锁后」。WP4 只改了这句话里的「过新」取值数字，没有触碰顺序语义。
- **影响**：判据 1 的后半句（migration 在单实例锁之后）**字面上不成立**。实际风险有限——`BEGIN IMMEDIATE` + `busy_timeout=5000` 保证并发 migration 串行，且 CLI 侧 `cli.rs:727` 在起进程前会先 `lock::probe` 拒绝「已有实例在跑」的情形。但合同与实现的口径确实不一致，且第二个进程会在拿到锁之前就已完成 migration。
- **最小修复**（**不属于 WP4 范围，需集成期处置**）：二选一——改 `daemon.rs` 的顺序（把 `Composition::assemble` 拆成「取存储」与「装配其余」两步，先取锁），或改文档措辞为「migration 在装配组合根时、绑定监听前完成」。**不应由 WP4 单独修改**：`crates/app/**` 不在 WP4 的声明写入范围内。

### F-03 — `daemon_lifecycle::the_periodic_task_runs_again_after_one_full_cycle` 存在既有竞态
- **严重度**：low（既有属性，WP4 未引入）
- **位置**：`crates/app/tests/daemon_lifecycle.rs:648-653`
- **问题**：用例在 `daemon.start()`（只等到运行记录发布 + `daemon.status` 可答）之后立刻断言 `daemon.maintenance` 日志恰好 1 条。启动清理那一轮是后台任务里跑的，其日志行与 `daemon.ready` 之间没有同步，在负载高时可能尚未 flush。首次完整跑时它失败过一次（`left: 0, right: 1`）。
- **影响**：整轮判据 7 的验证需要在噪声环境下复跑。但它**不是 WP4 引入的**：`crates/app` 在 `347399f..HEAD` 内零改动；我在基线 `6c093f1` 的独立副本上单跑该用例 3 次全绿，在 WP4 分支上单跑 3 次也全绿，完整 workspace 重跑 exit 0。
- **最小修复**（**不属于 WP4 范围**）：在该断言前轮询等待首条 `daemon.maintenance` 出现（带上限），或让 `start()` 也等待启动清理完成。记录在此供集成期处置。

### F-04 — `ModeChange::Set` 变体的别名 bind 编号无任何测试覆盖
- **严重度**：low
- **位置**：`crates/storage-sqlite/src/session_store.rs:1149-1151`（`Set` 变体的 `?7/?8/?9`）
- **问题**：我把 `Set` 变体的三列编号整体错位两位（`?7→?5`、`?8→?6`、`?9→?7`），**整个 `storage-sqlite` 测试套件全绿**。原因是全仓唯一使用 `ModeChange::Set` 的测试（`session_version_rule.rs:166`）传的是 `workspace_alias: None`，core 的 `broker.rs:1021`（`set_mode` 路径）也传 `None`。于是「`Set` 变体 + 非空别名」这条组合从未被真正执行过——**恰恰是代码注释 `:1132-1133` 自己警告的 bind 编号陷阱所在的那条路径**。
- **影响**：当前实现是正确的（我逐位核对过 bind 顺序，两个变体的编号都对）。风险是**回归无网**：将来有人调整 bind 顺序或在这条语句里加列，`Set` 变体的静默不写不会被任何测试发现。
- **最小修复**：在 `workspace_alias.rs` 里加一条（或扩到现有某条）用例，走 `ModeChange::Set` 且 `workspace_alias: Some(...)`，断言别名确实被写入。约 20 行。

---

## 五、判据结论汇总

| 判据 | 结论 | 关键证据 |
|---|---|---|
| 1 纯追加 / 单事务 / 幂等 / 链尾 v6 / v6 库跳过 / 锁后监听前 | ✅（后半句有 F-02 的既有偏差） | `migrate.rs:652-654, 1006-1039, 1057-1072`；`migration.rs` 12 passed |
| 2 旧行 `NULL` + 未分组 + 不可恢复 | ✅ | `workspace_alias.rs:230, 273, 302`；`migration.rs:1137-1156` |
| 3 无 path→alias 反查 | ✅ | 全 crate grep；唯一的 JOIN 按 alias；`workspace_from_row:477` 先判 NULL；变异 B 被 2 条用例杀死 |
| 4 展示名三边界 + cwd 不被改写 | ✅ | `workspace_alias.rs:523, 555-578, 601-606`；变异 C 被杀死 |
| 5 恢复不读不改别名 | ✅ | `session_store.rs:2110-2113`（SELECT 不含该列）；`:1141/:1151` 窄写；变异 D 被杀死 |
| 6 文档（drift 门禁 / §7 逐字 / §9 链 / §11.3 / 文件头） | ✅ | drift gate exit 0；独立括号配平比对 IDENTICAL；UTF-8 合法 |
| 7 **红窗口关闭（test + clippy）** | ✅ | **test exit 0（1084 passed / 0 failed，80 个二进制，无 crate 跳过）；clippy exit 0（0 warning）** |
| 8 无范围越界 | ✅ | 11 个文件全部在声明范围内；文档 hunk 全在声明区域；`crates/app`、`crates/core`、`sync-protocol` 零改动 |
| 9 无跨工作包耦合 | ✅ | `6c093f1..HEAD` 只有 WP3+WP4；`sync-protocol/tests/envelope_fixtures.rs` 全程 identical |

---

## 六、我实际运行的命令与原始输出

```
$ cd D:/Project/acp-remote-wt/wp4-storage

$ CARGO_TARGET_DIR=D:/Project/acp-remote-wt/target-wp4-review \
    cargo test --locked --workspace --all-features
EXIT=0
  测试二进制数（"     Running" 行）：80
  "test result:" 行数：92
  通过测试总数：1084
  失败数：0
  ignored：2（crash_child、regenerate_v1_fixture，均为显式 #[ignore] 的辅助用例）
  完整日志：D:\Project\acp-remote-wt\wp4-test-run3.log

$ CARGO_TARGET_DIR=D:/Project/acp-remote-wt/target-wp4-review \
    cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
EXIT=0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 15.89s
  warning/error 计数：0
  完整日志：D:\Project\acp-remote-wt\wp4-clippy.log

$ node scripts/check-contract-drift.mjs
EXIT=0
contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 96 个方法签名与 crates\core\src\ports.rs 一致

$ node scripts/check-doc-links.mjs          → EXIT=0
doc links OK: 403 relative links, 8663 section refs across 485 markdown files
$ node scripts/check-crate-boundaries.mjs   → EXIT=0
crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）
$ node scripts/agentic-gate.mjs             → EXIT=0
Totals: 19 passed, 0 failed (19 items)   [含 ✓ spec/storage-schema-v2-migration、✓ spec/workspace-resolution]

# 独立的 §7 DDL 括号配平 + 空白归一比对（不依赖 drift 脚本）
owned_session: IDENTICAL
owned_workspace: IDENTICAL

# U+FFFD 扫描（WP4 触及的 5 个源/测试文件）
crates/storage-sqlite/tests/workspace_alias.rs count=3   ← F-01
crates/storage-sqlite/src/session_store.rs clean
crates/storage-sqlite/src/migrate.rs clean
crates/storage-sqlite/tests/migration.rs clean
crates/storage-sqlite/tests/resume_columns.rs clean

# 变异测试（全部在 git archive HEAD 的独立副本上进行，副本已删除）
变异 A（仅 JOIN 加路径条件）        → 5 passed（被 workspace_from_row 的 NULL 早退挡住 = 纵深防御）
变异 B（COALESCE + 路径 JOIN）      → 2 failed ✅ 杀死
变异 C（去掉别名回退）              → 1 failed ✅ 杀死
变异 D（恢复覆写别名）              → 1 failed ✅ 杀死
变异 E（迁移写 NOT NULL DEFAULT ''）→ 2 failed ✅ 杀死
变异 F（Set 变体编号错位两位）      → 全绿 ❌ 未杀死（F-04）

# F-03 的基线对照
WP4 分支单跑该 app 用例 3 次        → 3 passed
基线 6c093f1 独立副本单跑 3 次      → 3 passed

# 范围核对
$ git diff --name-only 347399f..HEAD   → 11 个文件（见 §一表格）
$ git diff --stat 347399f..HEAD -- crates/app/       → 空
$ git diff --stat 347399f..HEAD -- crates/core/      → 空
$ git diff --stat 347399f..HEAD -- crates/sync-protocol/ → 空
$ git diff 6c093f1..HEAD --stat -- crates/sync-protocol/tests/envelope_fixtures.rs → 空
$ git log --oneline 6c093f1..HEAD → 8af5d21 / 347399f / 3603612
$ git merge-base --is-ancestor 6c093f1 HEAD → 成立
$ git status --porcelain → 空（结束时，源与文档均未被修改）
```

**清理**：变异副本 `wp4-mutcheck` 与基线副本 `wp4-base-check` 已删除；两个独立 `CARGO_TARGET_DIR`（`target-wp4-mut`、`target-wp4-base`）保留在 `D:\Project\acp-remote-wt\` 下，按需自行删除。`target-wp4-review` 是本次约定的构建目录，保留。

---

## 七、给集成期的建议

1. **F-01 建议在集成前修掉**（一行注释的编码修复，零风险）。F-04 可在同一轮里一并补上（约 20 行测试）。二者都需要新的实现实例承担——按 tasks.md 3.5，修复后应由独立 reviewer 复核。
2. **F-02 与 F-03 不属于 WP4**：两者的根因都在 `crates/app/**`，而 `crates/app` 不在 WP4 的声明写入范围内（WP4 对它零改动）。建议在 U1 集成单元里处置，或单开一个修复工作包；**不要**在 WP4 分支上顺手改，否则会破坏本工作包的范围证据。
3. **判据 7 已由本次实测关闭**：`verification.md` 里 PV0 记录的分支级 hook 证据缺口，现在有两条 exit 0 的工作区级命令输出作为独立证据（`wp4-test-run3.log` / `wp4-clippy.log`，共 1084 个测试 + 0 warning）。