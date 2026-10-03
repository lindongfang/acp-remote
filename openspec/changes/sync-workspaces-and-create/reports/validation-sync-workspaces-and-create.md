# Independent Validation — sync-workspaces-and-create

- 任务：`tasks.md` 6.1（`[validation]`），目标修订 `refs/heads/main` @ `a6f6210bcb8c1aacd81a09bf5169cb5eb5e287c7`
- 验证者：validator-1（独立子 Agent，未参与任何实现或 WP review）
- 隔离方式：只读分析 + 四条定向命令（`npm run check`；`openspec-agentic workflow check --stage plan --json`；`--stage final --json`（在仓库根与变更目录各跑一次）；Node/Python 读取 manifest、schema、diff 与日志哈希）。未运行 `cargo`，构建目录约束未被占用；未修改任何源码、文档或既有证据文件。
- 核对基线：`git rev-parse HEAD` = `a6f6210…`；`git status --porcelain` 仅 `?? openspec/changes/sync-workspaces-and-create/`。

## Verdict

**FAIL（针对变更账本；代码与合同面 PASS）**

拆开说，因为两部分结论不同：

- **代码与合同面：PASS。** 47 行有代码落点的覆盖索引行，我逐条独立核实了实现与检查证据，全部成立；`npm run check` 十项门禁在本工作树实跑退出码 0（20/20 agentic 校验项）；`manifest` 用例数我自己重数为 83 = 67 有效消息 + 2 信封层负例 + 9 body 层负例 + 5 pairing，与 `crates/sync-protocol/tests/envelope_fixtures.rs:25-29` 的四个常量完全吻合；`contract_digest`/`requirements_digest` 我用 `openspec-agentic workflow check --stage plan` 独立重算，与 `verification.md` 的 `agentic-premerge` receipt 逐字节一致；四条带 sha256 的证据哈希全部逐字节吻合；`crates/server/src` 确实只有 `local_admin/`、`node_link/`、`transport/`，**不存在 `sync` 模块**——「服务端快照组装/feature 发送侧门控/按设备授权过滤/Sync 面命令处理」确实没有代码落点，本次是合同冻结。合入本身没有发现会使主线不安全的不合格实现。
- **变更账本：FAIL。** 任务 6.1 的通过条件是「每个 Coverage Index 行都有**可读**的实现与检查证据；未覆盖行为 0 或**已如实记录**」。两条都不成立：
  1. plan.md 的 53 行 Coverage Index **每一行**的 `evidence` 都写 `reports/PV1.log`，该文件在变更目录下**不存在**。`openspec-agentic workflow check --stage final` 也机械报出 `证据不可读取：reports/PV1.log`。
  2. `verification.md` 的 CR-2 只申报「6 行无代码落点」（R9/R10/R11/R12/R14/R17），并以「其余 47 行由实现证据支撑」概括收尾。按我逐行核实的口径，**至少还有 R6、R7、R8、R19、R20、R28–R32 共 11 行**同样没有本仓库的代码落点（其中 R19/R20/R28–R32 的底层能力在 core 存在，但 Sync 面的映射与投递不存在），因此「已如实记录」这一条不满足。
  3. 另有三条会直接卡住 tasks 8.1 的 `workflow check --stage final` / 归档：`## Checks` 的 ALT1/ALT2 行格式不合门禁解析、WP3 的 Dispatch Reconciliation 执行者与 `dispatch-queue.jsonl` 台账不一致、`verification.md` 自身被整体复制了一份且两份 `## Checks` 内容不一致。

**含义**：合并可以保留（代码是干净的）；但本变更**不能**在当前账本状态下被判 PASS 并进入 8.1/归档。第 5 节的 F-01～F-04 是必须在归档前修掉的账本缺陷，F-05～F-13 是应当记录、其中多数应转后续变更的缺陷。

## 覆盖索引逐行判定

判定口径：
- **已落地** = 本仓库有代码落点 + 我独立核实了实现与检查证据。
- **仅合同层** = 证据形态是 schema / fixture / 文档 / 枚举门禁，运行期行为不在本仓库（`crates/server/src` 无 `sync` 模块，见 F-04）。这类行本身可以接受，但必须被如实标注。
- 所有 53 行共同的前置缺陷：`reports/PV1.log` 不存在（见 F-01），故逐行证据路径均为悬空。下表的「依据」列给出我**实际核实**的证据，不采用该悬空路径。

| 行 | 判定 | 依据 |
| --- | --- | --- |
| R1 会话摘要携带目录归属引用 | 已落地 | `crates/sync-protocol/src/common.rs:466` `Option<Nullable<WorkspaceRef>>` 三态；`crates/sync-protocol/tests/body_constraints.rs:188`；`schemas/sync/v1/common.schema.json#/$defs/sessionSummary` 的 `required` 不含 `workspace` |
| R2 已在已登记目录中创建的会话 | 已落地 | `fixtures/sync/v1/valid/sync-snapshot-chunk-sessions-workspace.json` 首个 item；`crates/core/src/use_cases.rs:2139` `create_session_persists_the_resolved_alias_next_to_the_canonical_path` |
| R3 没有目录归属的会话 | 已落地 | 同 fixture 第二个 item（`"workspace": null`）+ `crates/storage-sqlite/tests/workspace_alias.rs:386` 读回 `None` |
| R4 目录重指向后归属不漂移 | 已落地 | `crates/core/src/use_cases.rs:2192` `repointing_a_workspace_keeps_the_persisted_ownership`（断言 `repo|Moved` + cwd 不被改写）；`workspace_alias.rs:621` |
| R5 别名已删除时归属仍稳定 | 已落地 | `crates/storage-sqlite/src/session_store.rs:474-497` `workspace_from_row` 的 LEFT JOIN 失败回退别名；`workspace_alias.rs:621` |
| R6 imported 会话的归属 | **仅合同层** | fixture 第三个 item（remote origin + `workspace:null`）+ `docs/SYNC_PROTOCOL.md:875` 注记 + `common.rs:458` 注释。**无投影代码**（Sync 面无组装器）。verification 未把它列入 CR-2 → F-04 |
| R7 目录与 Agent 目录随快照下发 | **仅合同层** | `crates/sync-protocol/src/sync.rs:102-103/456-477` 枚举与解析；`schemas/sync/v1/sync.schema.json` `snapshotResource` 枚举 6→8。**全仓无任何 `SnapshotResource::Workspaces` 的构造点** → F-04 |
| R8 首次同步获得完整目录 | **仅合同层** | 同 R7；快照组装无实现 → F-04 |
| R9 空目录仍出现在快照中 | 仅合同层（已申报） | `docs/SYNC_PROTOCOL.md:805` 末句 + `fixtures/.../sync-snapshot-chunk-workspaces.json` 的 `scratch` 项 |
| R10 离线渲染不依赖在线查询 | 仅合同层（已申报） | `docs/SYNC_PROTOCOL.md` §9.4/§10.3 文字 + `docs/FRONTEND_DESIGN.md` §7 |
| R11 目录资源的协商与授权过滤 | 仅合同层（已申报） | schema 无发送侧实现；`SYNC_PROTOCOL.md:805` 写明读取归 `session.list` scope |
| R12 未协商时不发送该资源 | 仅合同层（已申报） | 文档 MUST NOT 句；无发送侧代码 |
| R13 未协商时摘要不含目录字段 | 已落地 | `body_constraints.rs:188-220` 逐字节断言「缺席 ≠ `null`」；`common.schema.json` `required` 不含该键 |
| R14 缺少目录读取授权的设备不下发 | 仅合同层（已申报） | 仅文档 |
| R15 目录相关输出不泄漏规范化路径 | 已落地 | `workspaceRef` 无路径字段且 `additionalProperties:false`；`fixtures/sync/v1/invalid/snapshot-chunk-workspace-item-has-path.json`；`body_constraints.rs:170-174` `leaked_path.is_err()` |
| R16 快照与会话摘要只含引用 | 已落地 | 同 R15 + `grep canonicalPath` 在 `schemas/`、`fixtures/sync/`、`docs/SYNC*` 中只命中该负例与禁令句 |
| R17 错误响应不回显路径 | 仅合同层（已申报） | `errors.json` 未变（design D4），映射只在 `design.md` D4 表 |
| R18 命令登记与双层授权 | 已落地 | `compatibility/commands/v1/commands.json:16/24`；两份协议 schema 枚举；`crates/sync-protocol/src/command.rs` `CommandName::ALL` 11→12；`crates/identity-auth/src/authorization.rs:38` `PACKS`；`check-command-catalog.mjs` 六处一致门禁（ALT1 日志 `command catalog OK: 13 commands`） |
| R19 未持 scope 的设备被拒且无副作用 | **仅合同层** | `broker.rs:1390` 的 `authorize(actor,"session.create",…)` 是传输无关的核心能力，但 **Sync 面无入口**（`grep CommandName::` 在 `crates/server` 只命中 `node_link`）→ F-04 |
| R20 持 scope 的设备成功创建 | **仅合同层** | 同 R19；`DEVELOPMENT_PLAN.md:62` 明写 `server::sync` 仍待实现 → F-04 |
| R21 载荷形状与拒绝 | 已落地（形状层） | `command.schema.json#/$defs/sessionCreate` `additionalProperties:false` + `deny_unknown_fields`；`fixtures/.../invalid/command-session-create-with-cwd.json`、`…-with-session-id.json` |
| R22 携带 cwd 被拒且不创建 | 已落地（拒绝层） | 同 R21 负例；「不创建/不启动进程」的副作用无 Sync 实现 → 并入 F-04 |
| R23 只接受已登记引用 | 部分 | 形状校验通过有 fixture；「进入授权与解析流程」无实现 → F-04 |
| R24 设备级授权语义与副作用 | 部分（文档+包） | `docs/SECURITY_DESIGN.md` §9.4 显式句（diff 可见新增）+ `pack.create-session` 不进任何 preset（`commands.json:26-29` presets 未含）；运行期判定无实现 |
| R25 新登记目录自动进入已授权范围 | 部分（同 R24） | 同上 |
| R26 撤销后立即失效 | 部分 | `identity-auth` 的撤销能力为既有代码，`session.create` 现已是设备 scope；Sync 面无实现 |
| R27 引用失败与解析失败分类 | 部分 | `design.md` D4 映射表 + `sync-session-create/spec.md` §Scenario；无映射代码 → F-04 |
| R28 未登记别名是参数类错误 | **仅合同层** | 映射仅在 D4 表与 spec → F-04 |
| R29 已登记但解析失败是服务端错误 | **仅合同层** | 同上 → F-04 |
| R30 终态与幂等 | 部分 | core 的 `accepted`→终态与 requestId 幂等（`broker.rs:1428 outcome.replayed`）已存在且传输无关；**Sync 面终态投递与结果编码无实现** → F-04 |
| R31 重复 requestId 不产生第二个会话 | 部分（同 R30） | 同上 |
| R32 崩溃窗口进 uncertain | 部分（同 R30） | core `settle_session_create` 已存在；Sync 面无投递 |
| R33 创建时别名的持久化与投影来源 | 已落地 | `broker.rs:1441-1470`；`ports.rs:186-196`；`docs/CORE_PORTS_AND_STORAGE.md` §3.1/§5.2 |
| R34 创建写入别名与路径 | 已落地 | `use_cases.rs:2139` 断言 `workspace_aliases` 与 `recoveries.workspace_cwd` 同行写入 |
| R35 不按路径反查归属 | 已落地 | `use_cases.rs:2167` + `workspace_alias.rs:566` `a_null_alias_is_never_back_filled_from_the_canonical_path` |
| R36 别名重指向不改变既有归属 | 已落地 | `use_cases.rs:2192` + `workspace_alias.rs:621` |
| R37 设备授权包 pack.create-session | 已落地 | `authorization.rs:38`；`identity-auth/tests/authorization.rs:43` `packs_match_machine_catalog`（编译期读 `commands.json`） |
| R38 展开得到命令级 scope | 已落地 | `identity-auth/tests/authorization.rs` 新增 `create_session_pack_expands_to_session_create_and_stays_out_of_presets` |
| R39 包成员漂移被门禁发现 | 已落地（门禁位置与措辞略有出入） | 同上，双向比对；但它属于 `npm run check:rust` 而非 `npm run check`，spec 写「合同检查失败」→ F-16 |
| R40 版本常量与 migration 幂等 | 已落地 | `migrate.rs` `FILE_FORMAT_VERSION = 6` + `V6_UPGRADE_OWNED`；`migration.rs:185 current_version_database_is_untouched_by_two_consecutive_starts` |
| R41 连续两次打开 schema 文本不变 | 已落地 | 同上 |
| R42 升级中途失败整体回滚 | 已落地 | `migration.rs:1391 a_failed_upgrade_rolls_back_to_v1` |
| R43 v5→v6 保留既有行且新列为空 | 已落地 | `workspace_alias.rs:181 v5_database_upgrades_to_v6_by_appending_a_null_workspace_alias_column` |
| R44 新列写入后可读回且不推导 | 已落地 | `workspace_alias.rs:323`（含库文件字节级 `quoted_alias` 复核） |
| R45 v4→v5 保留既有行 | 已落地 | `migration.rs:1032 v4_database_upgrades_to_v6_by_appending_the_recovery_columns_only` |
| R46 升级库与新建库 owned 列清单相等 | 已落地 | `migration.rs:611-630` 与 `:930-955`（`column_specs` 逐项比对，含 `NOT NULL`/默认值/末尾序） |
| R47 v3→v4 给既有节点行写空清单 | 已落地 | `migration.rs:815 v3_database_upgrades_to_v6_…` |
| R48 v2→v3 保留审计并扩展词表 | 已落地 | `migration.rs:659 v2_fixture_upgrades_to_v3_and_widens_only_the_audit_checks` |
| R49 会话目录归属列的写入与解释 | 已落地 | `session_store.rs:1138/1148` 两条 UPDATE 的 `CASE WHEN ?N IS NULL THEN col ELSE ?N END`；`workspace_alias.rs` 全组 |
| R50 写入别名原文 | 已落地 | `workspace_alias.rs:323`（`quoted_alias` 断言库内字节等于写入原文） |
| R51 恢复不改写目录归属 | 已落地 | `workspace_alias.rs:752 the_resume_flow_neither_reads_nor_rewrites_the_alias` + `use_cases.rs` `resume_session_neither_reads_nor_rewrites_the_workspace_alias` |
| R52 NULL 不被路径反查补齐 | 已落地 | `workspace_alias.rs:566` + `session_store.rs:474-486` |
| R53 未取得 ACP 会话标识时三列同为空 | **部分（结构在、断言缺）** | `broker.rs:1455-1470` 单一 `if let Some(agent_session_id)` 同门三列；但**没有任何测试直接断言**「解析出别名 + `agent_session_id()` 为 `None` → 别名列亦为 NULL」。既有 alias 测试都先 `set_agent_session_id`，`create_session_writes_no_recovery_columns_without_an_agent_session_id`（`use_cases.rs:2268`）传的是 `None` workspace。把 `workspace_alias` 移出同门，全套测试仍绿 → F-11 |

小计：已落地 33 行、仅合同层 16 行、部分 4 行（R23/R24/R25/R26/R27/R30/R31/R32 中口径为「部分」者按 wire/catalog/doc 证据计），其中**未如实标注**的仅合同层行为为 R6、R7、R8、R19、R20、R28–R32。

## 假设裁定

### (a) 设备级 `session.create` 授权覆盖「当时及此后新增的全部已登记 workspace 与已配置 Agent」——**PASS（文档侧成立，运行期无实现）**

- 安全文档：`docs/SECURITY_DESIGN.md` §9.4 新增段（diff `@@ -236,6 +237,7 @@`）明写「配对授予的 scope 是**设备级**的，不是配对瞬间的资源快照……判定以『该 workspace 当前是否已登记、该 Agent 当前是否已配置』为准」，并在 §10.2 条目追加一段「**`session.create` 的授权是设备级而不是资源级，因此有明确副作用**」。
- 命令目录：`compatibility/commands/v1/commands.json:16` `pack: "pack.create-session"`、`:24` 该包只含 `session.create`、`:26-29` 两个 preset 均未含它。
- 身份镜像：`crates/identity-auth/src/authorization.rs:11-15` 的文档注释逐字复述同一语义，`:38` 登记该包；`crates/identity-auth/tests/authorization.rs` 新增用例断言「展开为 `session.create` 且不进任何 preset」。
- 与 spec 一致：`specs/sync-session-create/spec.md` §「设备级授权语义与副作用」+ 两个场景；`specs/scope-expansion/spec.md` 要求不进预设。
- 裁定：作为**合同**这一假设被如实落到安全文档、命令目录与授权镜像三处；作为**运行时行为**它没有落点（`crates/server/src` 无 `sync` 模块），这与 design D9 的声明一致，不构成假设被推翻，但也不得被表述为「已实现」。

### (b) `workspace` 在任何 schema / fixture / 文档 / Rust 类型里都不携带规范化文件系统路径——**PASS**

- Schema：`schemas/sync/v1/common.schema.json#/$defs/workspaceRef` 只有 `alias`（1..=64）与 `displayName`（1..=128），`additionalProperties:false`，无路径键；`sessionSummary.workspace` 为 `oneOf[workspaceRef, null]`。
- Fixtures：`grep canonicalPath` 在 `fixtures/sync/v1/` 只命中**负例** `invalid/snapshot-chunk-workspace-item-has-path.json`（期望关键词 `additionalProperties`）；`fixtures/storage/v2/*.sqlite3` 的 `canonical_path` 是本机库表 DDL，不属对端可见输出。
- 文档：`docs/SYNC_PROTOCOL.md:805` 是禁令句；`docs/SECURITY_DESIGN.md` §11.2 把 `displayName` 定为不可信内容渲染。
- Rust：`crates/core/src/model/ids.rs:418-458` `WorkspaceRef` 无路径字段且注释写明理由；`crates/sync-protocol/src/common.rs:407-466` 同步。
- 唯一「路径味」是设计文档 D1 被否决方案表第 1 行提到 `canonical_path`——那是决策记录，不是输出。
- 裁定：成立。附一条盲点：原型 `prototypes/acp-remote-pwa.html:1915` 的创建弹层仍渲染 `dir.path`，本次以 `docs/FRONTEND_DESIGN.md` §4.1「目录项只显示展示名与别名，不显示或不可编辑规范化路径」显式覆盖，属已记录的刻意分歧（见 F-07 的同类问题）。

### (c) 打包写入门（alias + `agent_session_id` + `workspace_cwd` 三列同时 `NULL`）四处一致——**PASS**

- Spec：`specs/workspace-resolution/spec.md:7`「**别名与恢复所需的两列同门写入**……只有本次创建取得 Agent（ACP）侧会话标识时才一并落盘；未取得标识时三列同为 `NULL`」，并有专设场景（覆盖索引 R53）。
- 文档：`docs/CORE_PORTS_AND_STORAGE.md` §3.6 的 `[决定]`「`StateChange::Update` 新增三个可空列……它们只在 `create_session` 里……」逐字符合该裁定；§7.2 的 v5→v6 段说明列语义。
- 代码：`crates/core/src/broker.rs:1455-1470` 是唯一的提交点——整个 `OwnedCommit`（含 `agent_session_id`/`workspace_cwd`/`workspace_alias`）包在**同一个** `if let Some(agent_session_id) = endpoint.agent_session_id().cloned()` 内。
- 存储：`crates/storage-sqlite/src/session_store.rs:1138/1148` 用 `CASE WHEN ?N IS NULL THEN col ELSE ?N END`，因此恢复流程（永远传 `None`）不可能覆写任何一列。
- 裁定：四处口径一致，实现层面成立。唯一的弱点是这条门控**没有直接测试**（F-11）。

### (d) 除 `verification.md` 记录的内容外，没有新增或改名任何错误码 / feature ID / 命令名——**PASS**

- 错误码：`compatibility/errors/v1/errors.json` 与两份协议 schema 的错误枚举**不在** `6c093f1..a6f6210` 的 diff 列表中；ALT1 日志 `error registry OK: 58 codes across 2 protocols`，与 design D4「不新增任何错误码」一致。
- Feature ID：`compatibility/features/v1/features.json` 的 sync 段由 5 增至 7，新增恰为 `core.local-catalog.v1` 与 `core.session-create.v1`；全仓 `grep core.session-create.v1` 命中的位置（features.json、`SYNC_PROTOCOL.md` §5.2/§11.5/§16.2、`DEVELOPMENT_PLAN.md`、`FRONTEND_DESIGN.md`、fixture、README）与 `verification.md` 记录一致，无第三个新 ID。ALT1 `feature registry OK: 13 feature ids`（基线 11）。
- 命令名：`session.create` 未改名，仅 `transport` 由 `["node_link"]` 扩为 `["sync","node_link"]`、`pack` 由 `null` 设为 `pack.create-session`；Sync 枚举 11→12（`session.resume` 仍只经 Node Link），Node Link 枚举仍 13。ALT1 `command catalog OK: 13 commands`，`check-command-catalog.mjs` 对六处做集合相等断言。
- 裁定：成立。

## 盲点与发现

**F-01｜MAJOR｜53 行覆盖索引的行级证据文件不存在**
`plan.md` 的 `agentic-coverage` 块中 R1–R53 **每一行**的 `evidence` 都是 `[reports/PV1.log]`，`tasks.md:40`（5.3）同样指定该路径。实测 `reports/` 下无此文件；`openspec-agentic workflow check --stage final` 独立报出 `证据不可读取：reports/PV1.log`。这直接违反 `plan.md:407` 的完成条件「`verification.md` 的 Coverage Index 逐行有可读证据」与 6.1 的通过条件。实际可读的替代证据是 `reports/main-alt1-check.log`、`reports/main-workspace-test.log`、`reports/candidate-verify-round2.log`。

**F-02｜MAJOR｜`## Checks` 的 ALT1/ALT2 行会被 `workflow check` 判不合格**
`verification.md:66-67` 的 `Result / Exit Code` 写成 `**PASS** — schemas 127/30、…`，而门禁用 `/^\s*PASS\b/i` 匹配（`node_modules/@dongfanglin/openspec-agentic/src/workflow-check.mjs:921`），`**` 开头导致失败；`Evidence` 写成 `` `reports/main-alt1-check.log`（sha256:…） ``，门禁把整格当单一路径 `path.resolve`（同文件 `:46-51`），必然判「不可读取」。两条错误在 `final` 阶段实测复现。修法：结果格只留 `PASS`（明细移到同行的 Scope/Notes 或 `## Failures and Retries`），证据格只留裸路径（sha256 放 receipt 或另起一列）。PV1 行的证据格列了三个逗号分隔路径，同样过不了 `existsAt`。

**F-03｜MAJOR｜WP3 的 Dispatch Reconciliation 执行者与台账不一致**
`verification.md:103` 记 `WP3 | 2 | merger-1 | merged`，但 `dispatch-queue.jsonl` 中 WP3 attempt 2 的最终记录是 `{"wp":"WP3","executor":"merger-1-r2","state":"merged"}`（WP3 只有 `ready-to-merge` 那一步是 `merger-1`）。`verification.md:36` 主张「台账执行者仍是首轮 merger-1 标记的 merger-1，不允许改派」，这与台账文件本身矛盾。`final` 阶段实测报「工作包 WP3 的执行者与台账不一致：对账 merger-1，台账 merger-1-r2」。

**F-04｜MAJOR｜「无代码落点」的申报不完整（6 行 vs 至少 17 行）**
`verification.md:167/193/425/451`（CR-2）只申报 R9、R10、R11、R12、R14、R17 六行，并在 `:191/199/449/457` 以「其余 47 行覆盖索引由实现证据支撑」收尾。我逐行核实后认为至少还有 **R6、R7、R8、R19、R20、R28、R29、R30、R31、R32** 同样没有 Sync 面的代码落点：`crates/server/src` 只有 `local_admin/`、`node_link/`、`transport/`（`ls` 实测），`grep SnapshotResource::Workspaces` 全仓只命中 `crates/sync-protocol/src/sync.rs` 的枚举与解析（**无任何构造点**），`grep CommandName:: crates/server` 只命中 `node_link`。R19/R20/R28–R32 的底层能力（`broker.rs:1390` 的 `authorize`、`outcome.replayed` 幂等、`settle_session_create` 的 `accepted→completed/failed/uncertain`）确实存在且传输无关，这比 CR-2 里那 6 行强，但「Sync 面无投递/无映射」这一事实必须被登记，而不是被 47 行的概括吸收。CR-2 的方向判断（`crates/server/src` 无 `sync` 模块）我独立确认成立，问题在于它**低估了自己的范围**。

**F-05｜MINOR｜ALT2 的通过数与日志不符**
`verification.md:67` 写「92 个 test result 行、1093 passed / 0 failed」。我按 `reports/main-workspace-test.log` 逐行重算：92 行 `test result:`、**1092 passed / 0 failed / 2 ignored**，日志中无任何 `FAILED`。同一份 `verification.md:149/161` 的候选 R2 段写的也是 1092。属记录笔误，但它是带 sha256 的证据行，数字应当与被引用文件一致。

**F-06｜MINOR｜`reports/main-verify.log` 是一次失败运行，却被列为 PV1 的 PASS 证据**
`verification.md:65` 的 PV1 行结果为「PASS（分两次记录，见下）」，证据列包含 `reports/main-verify.log`（仅注「首跑」）。该日志结尾是 `error: test failed, to rerun pass '-p app --test daemon_lifecycle'`，`the_periodic_task_runs_again_after_one_full_cycle ... FAILED`（`left: 0, right: 1`，2.26s）。失败事实在 `:189/197` 有披露，但 PV1 行本身读起来像三份全绿日志。

**F-07｜MINOR｜`prototypes/IMPLEMENTATION-GAPS.md` 在合并后已过期**
该文件（非权威，自称与权威文档冲突时以权威文档为准）的 C1 条目断言「`owned_session` 无目录列；`SessionSummary` 九个字段无 workspace；`sessionSummary` 是 `additionalProperties: false`（暗示无 workspace）」，C2 断言「`session.create` 的 `transport` 仅 `["node_link"]`」，C4 把「目录绝对路径是否下发」列为**未决产品决策**。这三条均已被本次变更推翻或裁定（design D1 明确否决下发路径）。下一个变更的作者若以该文件切分任务会被直接误导。

**F-08｜MINOR｜Review Findings 的两条相对证据路径不可读**
`verification.md:111` 引 `reports/wp1-review.md`、`:117-118` 引 `reports/wp4-review.md`；变更目录下二者均不存在，实际位于 `D:\Project\acp-remote-wt\wp1-vocab\reports\` 与 `D:\Project\acp-remote-wt\wp4-storage\reports\`（工作树内，被 `.gitignore` 排除）。对照：wp1/wp2/wp3 的 round2 报告已复制进变更目录，WP4 的首轮 review 没有。

**F-09｜MINOR｜`## Dependency Declaration Review` 跳过 Round 3**
表（`verification.md:55-59`）只登记 Round 1/2/4/5/6。`reports/dependency-declaration-review-round3.md` 存在且结论为 **FAIL**（1 BLOCKER + 1 MAJOR + 2 MINOR + 2 SUGGESTION，plan 摘要 `sha256:8172eeac…`），该摘要未出现在任何台账里。依赖声明历史因此缺一轮。

**F-10｜MINOR｜`plan.md` 指定的合入报告路径从未产出**
`plan.md:354` 的 `Local Merge Conditions / Report Path` 写 `reports/merge-U1.md`，该文件不存在（合入结果写进了 `verification.md` 的 `## Merge History`）。

**F-11｜MINOR｜R53 的核心断言缺失（三列同门未被测试钉住）**
`crates/core/src/broker.rs:1455-1470` 的同门在结构上正确，但没有任何测试覆盖「解析出合法别名 + `agent_session_id()` 返回 `None`」这一组合：`use_cases.rs` 的三个 alias 测试都先调 `set_agent_session_id`；`create_session_writes_no_recovery_columns_without_an_agent_session_id`（`:2268`）传的是 `None` workspace，因此它对 `workspace_alias` 零判别力。把 `workspace_alias` 移出同门（仍在同一次 commit 里写）会让全套测试保持绿。考虑到 R53 是设计负责人**新增**的裁定产物，建议补一条断言。

**F-12｜MINOR｜`docs/CORE_PORTS_AND_STORAGE.md` §11.3 与 `crates/app/src/daemon.rs` 的顺序矛盾被原样承接**
`verification.md:117`（WP4-F-02）判为既有问题、不在本变更修复，我确认属实：`docs/CORE_PORTS_AND_STORAGE.md:1490` 写「migration 在取得单实例锁后、监听前完成」，而 `crates/app/src/daemon.rs:1157` 的 `Composition::assemble` 早于 `:1192` 的 `DaemonLock::acquire`。但本次变更的 `specs/storage-schema-v2-migration/spec.md:7`（MODIFIED Requirement）与 `CORE_PORTS_AND_STORAGE.md` 0.17 头都**原句承接**了这句断言，等于在一次修订里重新确认了一个已知为假的命题。同类矛盾还见 `:1343`（初清理应在锁与 migration 之后、监听之前）。

**F-13｜MINOR｜`daemon_lifecycle.rs:648` 的偶发失败与 F-12 同源，修法应选代码侧**
该用例在 `daemon.start()` 之后立即断言恰好一条 `daemon.maintenance`，而启动 prune 实际在后台任务里（失败那次耗时 2.3s、通过那次需 60s——正是断言在维护周期触发前就已执行）。把断言改成等待是治标；`CORE_PORTS_AND_STORAGE.md:1343` 承诺的「锁+migration 之后、监听之前同步完成一次初清理」在实现里并不成立。`verification.md:189` 已如实记录 4 次里挂 1 次，本轮 ALT2 日志中它通过（耗时 >60s）。

**F-14｜INFO｜`verification.md` 自身被整体复制了一份，且两份内容不一致**
`## Target`…`## Final Assessment` 全部出现两次（`:5-259` 与 `:266-518`）。关键差异：第二份 `## Checks`（`:322`）**只有 PV0 一行**，缺 PV1/ALT1/ALT2；而 `## Candidate Builds` 的标题本身被写坏成 `## Candidate Builds（候选构建经过## Candidate Builds（…×7`（`:135` 与 `:393`），候选 R1/R2 的叙述被逐字重复 3–7 次，`## Merge History` 的同一句注记重复 8 次。当前 `workflow check` 读的是第一份，所以没暴露；换一份解析实现或换个人读，就会读到缺 ALT 行的第二份。

**F-15｜INFO｜`## Independent Validation` 表为空**
`verification.md:241-244` 的表头已建但无数据行。本报告产出后需回填（任务 6.1 的记账动作）。

**F-16｜INFO｜R39 的门禁位置与 spec 措辞略有出入**
spec 写「合同检查失败并指出不一致项」。实际由 `crates/identity-auth/tests/authorization.rs:43 packs_match_machine_catalog` 承担——它在**编译期**用 `include_str!` 读真实的 `commands.json`，属 `npm run check:rust`（`npm run verify` 的一部分），不属于 `npm run check` 十项。门禁强度足够，但「合同检查」一词会让人以为在 `npm run check` 里。

## 证据完整性

**逐项哈希核对（我重算 vs `verification.md` 记录）**

| 记录位置 | 记录值 | 实测 | 结论 |
| --- | --- | --- | --- |
| `:66` ALT1 | `sha256:c8978e2082de58f1f81e557841e0b678f28ab780ef31d1b9e7b4e6497c9b2d42` | 同 | ✅ 一致 |
| `:67` ALT2 | `sha256:1992850d2a57bcb0aa916895a9e2479e83d33b3fcd0d25b5ea4f68e51436603d` | 同 | ✅ 一致 |
| `:217/228/232` premerge verify+ALT1+ALT2 | `sha256:c328abf04e77e8e2f227f44761bffbbfc9b789920841083a3e359b3f506dac05` | `candidate-verify-round2.log` 同 | ✅ 一致 |
| `:223` premerge review | `sha256:43bb3eefb94e4064ac1c61e131746a37f2089f195e4f72a50d0be00e8bb19045` | `candidate-review.md` 同 | ✅ 一致 |
| `:212` `contract_digest` | `sha256:19bc702f…` | `openspec-agentic workflow check --stage plan` 独立输出同值 | ✅ 一致 |
| `:213` `requirements_digest` | `sha256:3b6c490b…` | 同上，同值 | ✅ 一致 |
| `:65` `main-verify.log` | 未记哈希 | `def60e51e78ff4a0b887fcce31eb0d7a60bb4ed5558169694aeadf6924c3d3bd` | ⚠️ 无记录值可比；且该文件是失败运行（F-06） |

注：`verification.md` 的 `## Dependency Declaration Review` 表列名为「Plan Revision」而 receipt 列名为 `contract_digest`；两者不是同一口径（实测 `plan.md` 单文件 sha256 = `b9f957c2…`，与任一记录值都不同）。Round 6 报告自述 plan.md 为 408 行且「除 `:350` 外无内容变化」，与当前文件（408 行、`:350` 即 Merge Strategy 行）吻合，故判定这是**契约集合摘要**而非 plan.md 文件哈希，命名不一致属表述瑕疵而非伪造。

**引用路径可读性（我脚本枚举 `verification.md`/`plan.md`/`tasks.md` 中全部 `reports/*` 引用）**

- 可读（14）：`candidate-review.md`、`candidate-verify-round2.log`、`dependency-declaration-review-round1/2/4/5/6.md`、`main-alt1-check.log`、`main-verify.log`、`main-workspace-test.log`、`provisioner-handoffs.md`、`wp1/wp2/wp3/wp4-review-round2.md`。
- **不可读（6）**：`reports/PV1.log`（53 行覆盖索引全部引用，F-01）、`reports/wp1-review.md`（F-08）、`reports/wp4-review.md`（F-08）、`reports/merge-U1.md`（F-10）、`reports/ALT1.log` 与 `reports/ALT2.log`（tasks 7.1/7.2 指定，任务尚未执行，属**预期**缺失，不计缺陷）。`reports/validation-sync-workspaces-and-create.md` 由本报告补上。
- 另有一个**存在但未登记**的文件：`reports/dependency-declaration-review-round3.md`（F-09）。
- 分支级证据（`D:\Project\acp-remote-wt\wp1-vocab\reports\`、`wp2-wire\reports\`、`wp3-core\reports\`、`wp4-storage\reports\`）实测仍在，四份 round2 报告与 `wp1-review.md`/`wp4-review.md` 均存在；这些路径以绝对路径 + 「工作树内，未入库」标注，读者可定位，但不在版本化路径内。

**「有无产物支撑」的声明抽查**

- `:66` ALT1 行的十个数字（schemas 127/30、commands 13、errors 58、features 13、assets 17 schemas/170 fixtures、acp 71 rows、docs 409 links、boundaries 12 crates、drift §7 36 DDL + §5 15 traits/96 方法、agentic 20/20）**逐项与 `reports/main-alt1-check.log` 一致**；我在本工作树重跑 `npm run check` 得同样十项全绿、`Totals: 20 passed, 0 failed`、退出码 0。
- `:67` ALT2 行的「92 个 test result 行」与实测一致；「1093 passed」与实测 1092 不一致（F-05）。
- `:191/199/449/457`「其余 47 行由实现证据支撑」——**不成立**（F-04）。
- `:167/193/425/451` CR-2 关于「`crates/server/src` 只有 `local_admin/`、`node_link/`、`transport/`，不存在 `sync` 模块」——**成立且我独立复核**；但其「六行」的范围**不完整**（F-04）。
- `:187` 「自行重数 manifest 得 83 = 67+2+9+5」——**成立**：我独立重数得 `cases.length = 83`、`valid = 72`、其中 pairing 5 条（故有效消息 67）、invalid 11 条（2 条为信封层 `ping-without-connection`/`sequence-is-number`，9 条在 body 层被拒），与四个常量全部吻合。
- `:149/161` 候选 R2 的 `agentic 19/19` 与 ALT1 的 `agentic 20/20` 不一致，但两者是不同时间、不同 markdown 集合的两次运行（ALT1 日志显示扫了 507 个 markdown 文件，含本变更目录下的报告），可解释，非缺陷。

## 后续建议

**必须在归档前修（阻塞 8.1 / 归档，但不是代码问题）**

1. 补出或改指 `reports/PV1.log`：把 53 行覆盖索引的 `evidence` 改成实际存在的三份日志（`main-alt1-check.log` / `main-workspace-test.log` / `candidate-verify-round2.log`），或新增一份汇总日志并写明它对应 `a6f6210`。同时建议在 `verification.md` 里补一张**逐行**的覆盖判定表（行 → 实现证据 → 检查证据），以真正满足 `plan.md:407`。
2. 修 `## Checks` 三行：结果格只写 `PASS`，明细另置；证据格只写裸路径，sha256 移到 receipt 或加一列。（F-02）
3. 对齐 `## Dispatch Reconciliation` 的 WP3 Executor 与 `dispatch-queue.jsonl`（二选一：改表或改台账），并删除 `:36` 那段与台账矛盾的辩解。（F-03）
4. 清理 `verification.md` 的整体重复：删除 `:262-518` 这一份（或第一份），修好 `## Candidate Builds` 被写坏的标题，去掉 `## Merge History` 后重复 8 次的注记。（F-14）
5. 把 CR-2 的申报范围从 6 行扩到实际范围（至少补 R6/R7/R8/R19/R20/R28–R32，或改成按「证据形态」分类的统一口径），并据此改写「其余 47 行由实现证据支撑」。同时在 `verification.md` 记一条：本仓库当前确实不存在 `server::sync` 端到端路径，这与 plan 的 Main E2E `not-applicable` 一致。（F-04）
6. 修正 ALT2 的 `1093` → `1092`，并把 ALT1/ALT2 的 ignored 数（2）一并写上，避免下次再出现同类笔误。（F-05）
7. 把 `main-verify.log` 在 PV1 行里的呈现改清楚（例如标为「失败运行，见 `## Failures and Retests`」），不要让它以「首跑」的中性措辞出现在 PASS 证据列。（F-06）

**应转为后续变更（不阻塞本变更）**

8. **daemon 启动顺序与文档对齐**：把 `crates/app/src/daemon.rs` 的 `Composition::assemble`（内含 migration）移到 `DaemonLock::acquire` 之后，或改写 `CORE_PORTS_AND_STORAGE.md` §11.3/§7.4 与 `specs/storage-schema-v2-migration` 的那句断言。这一并解决 F-12，并顺带把 `daemon_lifecycle.rs:648` 的偶发从根上消掉——按文档口径，初清理应当在锁与 migration 之后、监听之前**同步**完成，把断言改成等待只是掩盖。（F-12/F-13）
9. **R53 的断言补齐**：在 `crates/core/src/use_cases.rs` 加一条「解析出别名但 `agent_session_id()` 为 `None` → `workspace_aliases` 与 `recoveries` 同时为空、版本仍为 1」的用例，把三列同门钉死。（F-11）
10. **`server::sync` 落地**：把 CR-2 点名的全部仅合同层行为（快照组装、feature 发送侧门控、按设备授权过滤、`session.create` 的双层授权/失败分类/终态与幂等投递）在一个变更里实现，并为每一条补运行时测试。本变更已把接口冻好，这是最省事的接入点。
11. **`prototypes/IMPLEMENTATION-GAPS.md` 刷新**：C1/C2/C4 三条已过期（C4 的「路径是否下发」已由 design D1 裁定为不下发）；顺带在原型里把创建弹层的 `dir.path`（`prototypes/acp-remote-pwa.html:1915`）去掉，与 `FRONTEND_DESIGN.md` §4.1 对齐。（F-07）
12. **`reports/wp1-review.md` / `reports/wp4-review.md` 入库**：与 round2 报告一样复制进变更目录，使 Review Findings 的 Recheck Evidence 列全部指向版本化路径。（F-08）
13. **依赖声明历史补 Round 3**：在 `## Dependency Declaration Review` 表补一行（FAIL，`sha256:8172eeac…`）并链接其报告，保持「每轮尝试都有台账」的完整性。（F-09）
14. **`plan.md` 的报告路径**：把 `reports/merge-U1.md` 改为实际使用的路径，或在下一个变更里产出该文件；`plan.md` 的修改会使 `contract_digest` 变化，因此更适合放进下一次修订而不是现在动。（F-10）
15. **`plan.md` 的 Shared File Ownership 补登 `crates/sync-protocol/tests/envelope_fixtures.rs`**：`verification.md:49` 自己记录了这个实现期缺口（WP1/WP2 都越界改了它，且 `EXPECTED_BODY_REJECTED` 被 git **静默自动合并**两次、两次靠手工纠正）。这属于计划缺陷而非实现缺陷，应在下一版计划里登记成 Shared File Ownership 行，而不是继续靠 merger 的人工收口规则。
16. **「未分组」桶的客户端口径**：`workspace: null` 在 spec 里被定义为原型的「未分组」，但没有任何文档说明客户端必须自行合成该桶、以及 `workspaces` 资源里不会出现这一项。建议在 `SYNC_PROTOCOL.md` §9.4 或 `FRONTEND_DESIGN.md` §4.1 补一句。（原型 `DIRS` 里的 `unsorted` 假目录见 `prototypes/acp-remote-pwa.html:735`）