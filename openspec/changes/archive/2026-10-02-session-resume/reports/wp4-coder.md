# WP4 coder 报告（`storage-sqlite` v5 migration 与恢复列读写）

```yaml
task_id: "2.4"
work_package: WP4
role: coder
phase: implement
agent_context: "coder-D（子 Agent worker，新实例：本 Work Package 此前无实例落笔，worktree 干净、无提交、无报告）。隔离方式：provisioner 分配的独立 worktree `D:/Project/acp-remote-wt/session-resume-wp4`（分支 `agentic/session-resume-wp4`）+ 独立 `CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp4`（冷编译在本轮内跑通）+ provisioner 提供的 `node_modules` 目录联接（未执行任何 npm install/ci/update）；未继承其它实例的对话或上下文。"
target_revision: "4a3882dfb3bf95d76340e35332aa9a8dc491a2d9"
scope: "crates/storage-sqlite/（src 与既有 tests）；docs/CORE_PORTS_AND_STORAGE.md 的 §7 标题、§7.2、§7.3、§9 判据 1/28、文件头版本记录，以及 §11.3 的一个数值（越界披露见 §7）"
result: PASS
resource_cleanup: "worktree、分支、CARGO_TARGET_DIR、node_modules 联接均保留给 provisioner 回收（本角色不自行清理/切换/合并）；未申请独占资源（无用例需要 loopback 监听器或外部 Agent，全部在进程内 SQLite 临时库上跑）；自建的临时 GIT_INDEX_FILE 已随脚本结束不再使用（仅在系统临时目录留下一个空索引文件，不属仓库资源）"
```

- 需求覆盖：R13–R22（`specs/storage-schema-v2-migration` 的 MODIFIED 需求与 6 个 Scenario + ADDED 需求与 1 个 Scenario），并作为 WP3 三值对象/端口的 SQLite 侧实现落点（R17 的写入点、R22 的只读边界）
- 证据：`reports/wp4-coder-PV1.log`、`reports/wp4-coder-PV2.log`、`reports/wp4-coder-precommit.log`、`reports/wp4-coder-red-window.log`
- 结论：**PV1（阶段 1，crate 子集 `-p storage-sqlite`）与 PV2 全绿**。本报告不覆盖独立 review（CR4）、TP2 用例与 E2E，也不代替它们的结论。

---

## 1. 起点、包含关系与输入版本（实测原始输出）

在 `D:/Project/acp-remote-wt/session-resume-wp4` 实测（开工前）：

```text
$ git rev-parse HEAD
8a08db8e77ac67efee317363ce241261af05c008          # == 派发要求的集成基线，逐字相同

$ git merge-base --is-ancestor 4f7a23554f76915cf5f29cc23e292ce270e014c9 HEAD ; echo $?
0                                                  # true：code:WP3 的交付提交已在起点内（WP4 就绪证据）

$ git branch --show-current
agentic/session-resume-wp4                          # provisioner 已就位，未自行创建/切换/清理
$ git status --porcelain | wc -l
0                                                   # 起点干净，无用户未提交改动
```

交付提交与包含关系（合入前核对用）：

```text
base   (起点) = 8a08db8e77ac67efee317363ce241261af05c008
target (交付) = 4a3882dfb3bf95d76340e35332aa9a8dc491a2d9
$ git merge-base --is-ancestor 8a08db8e77ac67efee317363ce241261af05c008 HEAD ; echo $?   → 0 (true)
$ git merge-base --is-ancestor 4f7a23554f76915cf5f29cc23e292ce270e014c9 HEAD ; echo $?   → 0 (true)
$ git diff --stat 8a08db8e..HEAD | tail -1        → 9 files changed, 567 insertions(+), 50 deletions(-)
$ git status --porcelain | wc -l                  → 0（无未提交改动、无暂存文件）
$ git log --oneline 8a08db8e..HEAD | cat
4a3882d feat(storage): v5 migration 追加恢复列并实现 load_recovery 窄读取
```

输入版本：`design.md@D2`（v5 持久化形状，含 2026-09-30 的契约订正）、`docs/CORE_PORTS_AND_STORAGE.md@§7-v5`（WP3 已把 §5 的 rust 块按三个新方法更新）；上游代码事实取自集成基线 `8a08db8e`（含 WP3 交付提交 `4f7a2355`）。

提交信息合规（钩子在本 worktree 不存在，因此单独校验）：

```text
$ npx --quiet --no-install commitlint --from 8a08db8e77ac67efee317363ce241261af05c008 --to HEAD --verbose
✔   found 0 problems, 0 warnings
```

环境：`rustc 1.98.1 (48a229cea 2026-09-01)` / `cargo 1.98.1 (797e8a9bc 2026-08-05)`（仓库 `rust-toolchain.toml` 固定，与 CI 同一编译器）、`node v24.19.0` / `npm 12.0.2`；全部 cargo 命令带 `--locked`。

---

## 2. 改动文件清单（`git diff --name-only 8a08db8e..HEAD`，9 个，全部在写范围内）

| 文件 | 改动 |
| --- | --- |
| `crates/storage-sqlite/src/migrate.rs` | `FILE_FORMAT_VERSION`/`OWNED_SCHEMA_VERSION` 推进到 **5**（`IMPORTED_SCHEMA_VERSION` 保持 **3**）；`OWNED_SCHEMA_V1` 的 `owned_session` 末尾追加两列；新增 `V5_UPGRADE_OWNED`（只两条 `ALTER TABLE … ADD COLUMN`）与 `file_version < 5` 守卫；`migrate()` 文档注释与段序说明同步 |
| `crates/storage-sqlite/src/session_store.rs` | `SessionUpdate` 的两列按 `CASE WHEN ?N IS NULL` 写入（`None` = 不改该列）；实现必需方法 `SessionStore::load_recovery`（窄读取，两列缺失 → `Ok(None)`） |
| `crates/storage-sqlite/tests/commit.rs`（7 处） | 既有 `SessionUpdate` 字面量补 `agent_session_id: None, workspace_cwd: None` |
| `crates/storage-sqlite/tests/session_version_rule.rs`（3 处） | 同上 |
| `crates/storage-sqlite/tests/retention.rs`（2 处） | 同上 |
| `crates/storage-sqlite/tests/contract_v03.rs`（1 处） | 同上 |
| `crates/storage-sqlite/tests/enum_coverage.rs`（1 处） | 同上 |
| `crates/storage-sqlite/tests/migration.rs` | 5 处 `owned_schema_version == 4` → `5`；v3 用例改写为「v3 → v4 → v5」（现场造库时同时 `DROP` 掉换代列，DDL 逐字节断言按段放行 `owned_node`/`owned_session`）；新增两条用例（见 §3.4）；新增 `create_session` 辅助与顶部 import |
| `docs/CORE_PORTS_AND_STORAGE.md` | §7 标题 `v4` → `v5`；§7.2 版本常量 5/5/3、升级判据与段序、v1→v2 步骤 4 的落盘取值、v3→v4 段末句、**新增 v4→v5 升级段**、迁移测试资产与回滚口径；§7.3 的 `owned_session` DDL 追加两列；§9 判据 1（版本链）与判据 28（owned 列清单 + 旧行保留 + 读回不推导 + 恢复不覆写）；文件头追加 `版本：0.15` 记录；§11.3 的「过新」用例取值 5 → 6 |

**14 处既有 `SessionUpdate` 字面量已逐处复核**：`grep -rn "closed_at: None," crates/storage-sqlite/tests/` 命中数（commit 7 / contract_v03 1 / enum_coverage 1 / retention 2 / session_version_rule 3）= **14**，与 WP3 报告的清单逐文件一致，且每一处都是 `SessionUpdate` 的字段（无其它结构体使用 `closed_at` 字面量字段）。

---

## 3. 实现要点（逐条对应 design D2 与 spec R13–R22）

1. **版本常量**：`FILE_FORMAT_VERSION = 5`、`OWNED_SCHEMA_VERSION = 5`、`IMPORTED_SCHEMA_VERSION = 3`（未动）。新建库由 DDL 常量直接建成 v5 形状；`user_version = 5` 的库跳过全部升级段，因此第二次打开不重写 `sqlite_master`、不写任何行。
2. **只追加、不重建**：`V5_UPGRADE_OWNED` 只有两条 `ALTER TABLE owned_session ADD COLUMN`（两列都可空、**无默认值**），不触碰任何 CHECK，因此不存在 12-step 表重建；`ALTER TABLE ADD COLUMN` 把列追加在末尾，升级库与新建库的列顺序逐项相等。
3. **升级守卫**：v5 段由 `file_version < 5` 守卫，且整段包在既有的 `!new_database && file_version < FILE_FORMAT_VERSION` 分支内。v5 段不单独探测 `owned_session` 是否预先存在——它正是「升级库 vs 新建库」的判据本身（走升级分支即必然已存在），这一点写进了函数体与函数级注释，避免后人照抄 v4 段的 `owned_node_pre_existing`。
4. **`load_recovery`（窄读取）**：`SELECT agent_id, agent_name, agent_session_id, workspace_cwd FROM owned_session WHERE session_id = ?1`，走只读连接池；行不存在或任一列为 `NULL` → `Ok(None)`（`NULL` 不是错误，不补齐、不推导、不用别名解析结果替换）；列值形状非法（库被外部改写）→ 按本 crate 既有惯例报 `ColumnValue` 类损坏错误。两列**不进** `Session`/`SessionSummary`，因此本入口是唯一读取路径。
5. **两列的写入点**：`StateChange::Update` 沿用既有 `CASE WHEN ?N IS NULL THEN 原值 ELSE 新值` 规则（`None` = 不改该列），core 的 `create_session` 只在 `factory.create` 成功后紧接着的一次提交里传 `Some`，恢复流程永远传 `None`，因此恢复不可能覆写已持久化取值（spec R22）。
   - 实现期发现并修掉一个自己引入的缺陷（保留记录）：最初给两条 UPDATE 语句的追加列统一用 `?7`/`?8`，但 `ModeChange::Unchanged` 那条语句只绑定到 `?4`，SQLite 的 `?NNN` 是**按位置绑定**，第 5、6 次 `bind` 会落到未使用的下标上 → `CASE WHEN ?7 IS NULL` 恒真 → **静默不写**。已改为每条语句内部连续编号（Unchanged 用 `?5`/`?6`、Set 用 `?7`/`?8`）并在代码里写明理由；新增的往返用例正是该缺陷的判别力来源（修复前它以「两列都已写入」断言失败）。
6. **新增/改写的测试**（`crates/storage-sqlite/tests/migration.rs`）：
   - `v4_database_upgrades_to_v5_by_appending_the_recovery_columns_only`（新）：造 v4 库（写一条列列有值的会话行 → `DROP` 两列 → 降版本键到 4）→ 升级后逐条断言：其余列逐字节不变、两列为 `NULL` 且**不是空串**、`load_recovery` 返回 `None`（升级不制造「突然可恢复」的假象）、两族列清单与新建库逐项相等、除 `owned_session` 外每条表 DDL 逐字节不变、`user_version` 与版本键为 5/5/3。
   - `recovery_columns_round_trip_and_are_not_rewritten_by_the_resume_flow`（新）：创建会话 + 紧接着的 `StateChange::Update` 写两列（版本 1 → 2）→ 关闭重开 → `load_recovery` 逐字节读回、未写入两列的会话读回 `None` → 走一遍恢复流程的存储交互（`load_recovery` + `session.resume` 的 `accepted` 幂等行，都不带两列）→ 读回值与恢复前 `Some(record)` 相等、会话版本不变 → 库里原始字节：目标会话等于写入值、另一条两列都是 `NULL` 且无空串。
   - `v3_database_upgrades_to_v5_by_appending_the_export_id_and_recovery_columns_only`（改名 + 改写）：既是 R19（v3 到 v4 给既有节点行写空清单）也是 R18/R13 在 v3 起点的证据；造库时同时去掉 v4/v5 才有的列，DDL 逐字节断言按段放行 `owned_node`（v4 段）与 `owned_session`（v5 段）并断言这两张表确实带上新列——**其它表的 DDL 必须逐字节不变**，即 v3 库不会重跑 v2/v3 的 12-step 重建。
   - 既有的幂等/字节稳定/回滚用例（`current_version_database_is_untouched_by_two_consecutive_starts`、`second_open_of_an_upgraded_database_rewrites_nothing`、`a_failed_upgrade_rolls_back_to_v1`、`v1_fixture_upgrades_to_v3_and_preserves_rows`、`v2_fixture_upgrades_to_v3_and_widens_only_the_audit_checks`、`too_new_database_is_rejected_without_writing_rows`）全部在 v5 上通过，其中 v1 用例继续断言两族列清单与新建库逐项相等（现在包含两列）。

---

## 4. Checks（逐条命令、目录、退出码）

### PV1（阶段 1：本 WP 的 crate 子集 `-p storage-sqlite`，worktree 根，`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp4`）

日志：`reports/wp4-coder-PV1.log`（HEAD `4a3882d`；工作树与索引均干净）

| # | 命令 | 退出码 | 结果 |
| --- | --- | --- | --- |
| 1 | `cargo fmt --all -- --check` | 0 | PASS（无格式差异） |
| 2 | `cargo clippy --locked -p storage-sqlite --all-targets --all-features -- -D warnings` | 0 | PASS（0 warning / 0 error；此前登记的 `E0046`（缺 `load_recovery`）已由本 WP 消除） |
| 3 | `cargo test --locked -p storage-sqlite --all-features` | 0 | PASS：**131 passed / 0 failed / 2 ignored**（3 个 lib 单测 + 128 个集成用例；2 个 ignored 是既有的夹具生成器 `regenerate_v1_fixture` 与 `commit.rs` 的同类手动用例）。本 WP 新增的 2 条用例与改写的 1 条用例均在其中 |

### PV2（`npm run check`，worktree 根）

日志：`reports/wp4-coder-PV2.log`（同一提交；`node v24.19.0` / `npm 12.0.2`；未执行 npm install/ci）

| # | 命令 | 退出码 | 结果 |
| --- | --- | --- | --- |
| 1 | `npm run check` | 0 | PASS（全部合同门禁绿）。关键行：`schema fixtures OK: 120 valid, 26 invalid`、`command catalog OK: 13 commands`、`error registry OK: 58 codes`、`feature registry OK`、`contract assets OK`、`ACP compatibility matrix OK`、`doc links OK: 399 relative links, 6782 section refs`、`crate boundaries OK`、**`contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 96 个方法签名与 crates\core\src\ports.rs 一致`**、`openspec validate` 19/19、agentic 宿主入口 17 文件 |

其中 `check:contract-drift` 是本 WP 最关键的机械判据：§7.3 的 `owned_session` DDL 与 `OWNED_SCHEMA_V1` **逐条一致**（含新增的两列）。

### 未执行 / 不作为判据的检查

| 检查 | 状态 | 依据 |
| --- | --- | --- |
| workspace 全量 PV1（阶段 2：`npm run check:rust`） | **失败（已登记红窗口），不作为本阶段判据** | 见 §6。日志 `reports/wp4-coder-red-window.log`（退出码 101） |
| 等价 pre-commit 检查（`scripts/pre-commit.mjs`） | 退出码 101，失败点仅为 workspace clippy（红窗口） | 见 §6.3。日志 `reports/wp4-coder-precommit.log` |
| `cargo-deny` / `gitleaks` | NOT_RUN | 只在 CI 运行（`AGENTS.md` §8；本机未安装 gitleaks，`scripts/pre-commit.mjs` 的密钥扫描步按设计跳过） |
| E2E | NOT_APPLICABLE | 本变更 Main E2E 记 not-applicable（plan.md 的降级批准），替代检查 C1/C2 在最终主分支执行 |

---

## 5. 需求行映射（`specs/storage-schema-v2-migration/spec.md`）

| R 行（Scenario） | 落点 | 证据 |
| --- | --- | --- |
| R13 版本常量与 migration 幂等（MODIFIED 需求正文） | 三常量 5/5/3；v5 段单事务、`file_version < 5` 守卫、可重复打开；`NULL` 语义与「只追加」写进 DDL 注释与 §7.2 | PV1（`fresh_directory_is_created_at_the_current_version`、`current_version_database_is_untouched_by_two_consecutive_starts`）、PV2（contract drift） |
| R14 连续两次打开 schema 文本不变 | 升级库第二次打开跳过全部升级段 | PV1（`second_open_of_an_upgraded_database_rewrites_nothing`、v4/v3 两条新用例的两族列清单断言） |
| R15 升级中途失败整体回滚 | 既有 v1 回滚用例在 v5 上仍绿（注入 DDL 冲突 → `user_version` 与两族版本保持 v1、v0 行集不变、重试可升级） | PV1（`a_failed_upgrade_rolls_back_to_v1`） |
| R16 v4 到 v5 升级保留既有会话行且新列为空 | **新增用例**：其余列逐字节不变、两列 `NULL`（非空串）、不被当作可恢复会话 | PV1（`v4_database_upgrades_to_v5_by_appending_the_recovery_columns_only`） |
| R17 新列写入后可读回且不推导 | `StateChange::Update` 的 `Some` 写入 + `load_recovery` 逐字节读回；未写入的会话读回 `NULL`（不出现空串/别名/占位路径） | PV1（`recovery_columns_round_trip_and_are_not_rewritten_by_the_resume_flow`） |
| R18 升级库与新建库的 owned 列清单相等 | `column_specs`/`column_names` 逐表比对（v1、v2、v3、v4 四条升级路径）；两列必须是 `owned_session` 末尾且可空无默认值 | PV1（四条升级用例） |
| R19 v3 到 v4 升级给既有节点行写空清单（并继续升级到 v5） | 改写的 v3 用例：`export_ids_json = '[]'`、其余列逐字不变；该库同时走过 v5 段 | PV1（`v3_database_upgrades_to_v5_by_appending_the_export_id_and_recovery_columns_only`） |
| R20 v2 到 v3 升级保留审计并扩展词表（并继续升级到 v5） | 既有 v2 用例在 v5 常量下仍绿（审计行、`audit_id` 与序列不回退、新取值可写、`owned_command` 三值） | PV1（`v2_fixture_upgrades_to_v3_and_widens_only_the_audit_checks`） |
| R21 恢复所需列的读写边界（ADDED 需求正文） | 存储层只在 `agent_session_id`/`workspace_cwd` 被显式提供时写入；恢复只读（`load_recovery` 是唯一读取入口，且不写任何列） | PV1（往返用例）、代码（`session_store.rs` 的两列 `CASE WHEN` 与 `load_recovery`） |
| R22 恢复流程不覆写持久化取值 | 恢复流程的存储交互（`load_recovery` + `session.resume` 的 `accepted` 幂等行）后，两列与读回值逐字节相同、版本不变 | PV1（同一条往返用例的第 ②③ 步） |

WP3 交接义务（DR1-F42）：`SqliteStore` 已实现必需的 `SessionStore::load_recovery`（返回含 `agent`/`agent_session_id`/`workspace_cwd` 的 `SessionRecoveryRecord`，两列缺失时为 `None`），本 crate 内的 `E0046` 已消除。

---

## 6. 红窗口说明（**不是**本 WP 的失败）

### 6.1 现象

`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`（`reports/wp4-coder-red-window.log` 与更完整的 `reports/wp4-coder-precommit.log` 第 3/3 步）退出码 **101**，唯二来源是已登记红窗口的两个下游责任包：

```text
error[E0046]: not all trait items implemented, missing: `resume`          → crates\agent-host\src\host.rs:449:1        （WP5）
error[E0046]: not all trait items implemented, missing: `agent_session_id` → crates\agent-host\src\session.rs:746:1     （WP5）
error[E0046]: not all trait items implemented, missing: `load_recovery`/`resume`/`agent_session_id`
                                                                           → crates\server\src\local_admin\test_support.rs:817/961/1021、crates\server\src\node_link\command\tests.rs:145/437/484、crates\server\src\node_link\resource\tests.rs:265（WP6）
error[E0004]: non-exhaustive patterns: `CommandPayload::SessionResume(_)` not covered
                                                                           → crates\server\src\node_link\command.rs:2191:25（WP6）
```

`agent-host`/`server` 两个 crate 之外没有任何诊断；`storage-sqlite` 在本轮完整编译通过（日志里 `Checking storage-sqlite …` 之后无该 crate 的任何 error/warning），WP3 报告中登记的 `crates/storage-sqlite` 的 `E0046`（缺 `load_recovery`）已由本 WP 消除。

### 6.2 判定

这与 `plan.md` 的 `## Dependency Handoffs` 红窗口段逐条一致：**WP4 只负责本 crate 内失配的测试字面量与 `load_recovery` 的新实现**；`agent-host` 归 WP5、`server`/`app` 归 WP6。本 WP 未改任何其它 crate，也未为了让 workspace 变绿而动 `crates/server/`。

### 6.3 提交钩子与 `--no-verify`

- 本 worktree 与其它 worktree 一样**没有 `.husky/_`**（`git config core.hooksPath = .husky/_`，但该目录不存在），因此 git **静默跳过** pre-commit/commit-msg——本次提交**没有使用** `git commit --no-verify`（它也没有被触发）；提交信息另外用 `npx --quiet --no-install commitlint --from <base> --to <HEAD> --verbose` 校验为 0 problems。
- 为得到「等价检查」而非「钩子被跳过」，提交后在 worktree 根跑了一次 `scripts/pre-commit.mjs`（用临时 `GIT_INDEX_FILE` 让它像提交前那样看到本次的 9 个文件；否则索引为空会直接短路成 exit 0）：**退出码 101**，`1/3 cargo fmt --all -- --check` 与 `2/3 npm.cmd run check` 均通过，唯一失败的是 `3/3 cargo clippy --locked --workspace …`，失败原因就是 §6.1 的已登记红窗口（`plan.md` 明确允许此时用 `--no-verify`）。日志：`reports/wp4-coder-precommit.log`。

---

## 7. 写入范围与越界

**声明范围内**：`crates/storage-sqlite/`（`src/` 与既有 `tests/`）、`docs/CORE_PORTS_AND_STORAGE.md` 的 §7 标题/§7.2/§7.3/§9 判据 1 与 28/文件头版本记录（新增 `版本：0.15` 条目，同批覆盖 WP3 的 §5 rust 块与 §2/§3.1/§3.3/§3.6/§4 变化，以及本 WP 的 v5 变化）。

**一处越出声明范围，主动披露**：`docs/CORE_PORTS_AND_STORAGE.md` **§11.3** 的一个数值——「『过新』用例使用高于新版本的值（`user_version = 5`，即 `FILE_FORMAT_VERSION + 1`）」改为 `user_version = 6`。理由：`FILE_FORMAT_VERSION` 推进到 5 后原括号里的 5 不再等于 `FILE_FORMAT_VERSION + 1`，原句变成**不成立的断言**；先例是 0.14 的 v3→v4 变更同批改过同一处（「§11.3 的『过新』用例取值改为 5」）。改的是 1 个数字，未触及 §11.3 的其它文字。判断口径：`plan.md` 的 WP4 写范围把该文件限定为「§7 标题、§7.2、§7.3、§9 判据 1/28、文件头」，§11.3 未登记给任何写者；若主 Agent 认为该处应保持原样，请指示——它不改变实现、测试或任何门禁结论。

**未动**：`crates/core/`、`crates/agent-host/`、`crates/server/`、`crates/app/`、`crates/acp-protocol/`、`crates/node-link-protocol/`、`compatibility/`、`schemas/`、`fixtures/`、`scripts/`、`package.json`、`plan.md`、`tasks.md`、`design.md`、`specs/`、其它 docs（`git diff --stat` 的 9 个文件即全部）。

**行尾一致性**：`session_store.rs` 与 `tests/migration.rs` 在编辑过程中一度被写成 LF，已改回与 worktree 一致的 CRLF（`git diff --stat` 无行尾噪声；提交内容经 git 的 `autocrlf` 归一化，与仓库既有约定无关差异）。

---

## 8. 资源释放

- worktree `D:/Project/acp-remote-wt/session-resume-wp4`、分支 `agentic/session-resume-wp4`、`CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp4`、`node_modules` 目录联接：**保留**，由 provisioner/merger 回收；本角色未清理、未切换、未推送、未合并。
- 测试资源：全部用例各自在 `temp_dir` 临时目录内创建并自清理的 SQLite 库；未占用仓库内任何数据目录或固定端口（无用例绑定监听器）。
- 本角色未申请独占资源；`npm install/ci/update` 未执行。

---

## 9. 未执行项与待澄清问题

1. **未执行**：workspace 全量 PV1（阶段 2）与候选/主分支回归——按 plan.md 的定义属候选阶段检查，且当前 workspace 处于已登记红窗口；`cargo-deny` 与 `gitleaks` 只在 CI 运行，本机未执行（`gitleaks` 未安装）。
2. **未执行**：独立 review（CR4）、TP2 用例、E2E——由主 Agent 另行派发；本报告的 PASS 只覆盖 PV1 阶段 1 与 PV2。
3. **待澄清（次要，不阻塞）**：§7 的 §11.3 数值同步（见 §7 的披露）。若无异议，请由主 Agent 在候选中保留；若要回退，只需把 `user_version = 6` 改回原文并删掉 0.15 条目里的对应短句——实现与门禁不受影响。
4. **已知限制（继承自设计，非缺陷）**：`agent_session_id`/`workspace_cwd` 的「写入一次、此后只读」由调用方（core 的 `create_session`）保证；存储层按 §5.2 的 `[决定]` 实现「`None` = 不改该列」。若后续希望存储层硬拒「覆盖非空值」，属契约变更，需要新的裁决。
5. **回滚口径**：代码回滚到 v5 之前的版本时，v5 库会被既有的「版本过新拒绝启动」拦住（不静默降级、不丢数据）；本次不提供降级工具，回滚需恢复升级前备份并单独授权（已在 §7.2 写明）。

---

## 10. `handoff_index`

```yaml
handoff_index:
  - task_id: "2.4"
    work_package: WP4
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "4a3882dfb3bf95d76340e35332aa9a8dc491a2d9"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/session-resume/reports/wp4-coder.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 agentic/session-resume-wp4 的交付提交 4a3882d 上实跑（工作树与索引 0 项未提交改动）：阶段 1 的 crate 子集 = storage-sqlite（plan.md 的 PV1 阶段定义与 WP4 的 Verification 列 `-p storage-sqlite`）；命令与参数为 cargo fmt --all -- --check / cargo clippy --locked -p storage-sqlite --all-targets --all-features -- -D warnings / cargo test --locked -p storage-sqlite --all-features，三者退出码均为 0（131 passed / 0 failed / 2 ignored）；环境 rustc 1.98.1 / cargo 1.98.1（rust-toolchain.toml 固定）、CARGO_TARGET_DIR=D:/Project/acp-remote-target/session-resume-wp4；原始输出见 reports/wp4-coder-PV1.log；workspace 全量属已登记红窗口（WP5/WP6 收口），不作为本阶段判据（reports/wp4-coder-red-window.log、reports/wp4-coder-precommit.log）"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.4"
    work_package: WP4
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "4a3882dfb3bf95d76340e35332aa9a8dc491a2d9"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/session-resume/reports/wp4-coder.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一提交、worktree 根实跑 npm run check（退出码 0，node v24.19.0 / npm 12.0.2，未执行 npm install/ci）：合同门禁全绿，其中 check:contract-drift 断言 §7 的 36 条 DDL 与 crates/storage-sqlite/src/migrate.rs 逐条一致（本 WP 新增的两列已逐字同步进 §7.3 与 §7.2 的 v5 段）、§5 的 15 个 trait / 96 个方法签名与 core/src/ports.rs 一致；check:command-catalog 仍为 13 命令、crate boundaries / doc links / ACP 矩阵均绿；原始输出见 reports/wp4-coder-PV2.log"
    source_evidence: NOT_APPLICABLE
```

> 说明：本报告不含独立 review（CR4）、TP2 用例与 E2E 的结论——它们各自独立派发；PV1 的阶段 2（集成基线 workspace 全量）由主 Agent 在候选阶段执行，当前处于已登记红窗口（WP5/WP6 收口）。
