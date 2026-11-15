# R1 schema 11 只读交付缺口复核

2026-11-13。结论：**未完成，不能报告 R1 验收通过**。当前产品迁移接入未见需要本轮改动产品语义的明确缺陷；剩余工作收敛为以下四项测试/证据缺口。此报告不是测试执行结果，也不签署报告写出后修改的开发测试。

## 版本与范围

- 仓库：`D:/Project/acp-remote`；读取时 HEAD：`65ec7815bb0280f4519989d93a7b4ed3b4b37994`。工作区大量未提交/未跟踪内容，故 HEAD 不代表受审产品内容；实际身份以 `SOURCE-READ.json` 中各叶 SHA-256 为准。
- 核心测试实际读取版：`native_release_schema/tests.rs` = `b84747a7e89742360ee7afebd16254079e534b43bf02268361202a0762baa366`；`node_catalog_schema/tests.rs` = `205289932055614c0772bde00ed3d1201d13d65698f520561db8ba5e3f327186`。写报告前再次核对，仍为这两个摘要。主执行后续增强版需重新绑定并复查，不能直接沿用本报告关闭问题。
- 依据：AGENTS、OpenSpec config/reviewer、code-standards，以及本批 `REBASE-PLAN.md`、`TEST-DELTA.md`、`REVIEW-R1-TEST-GATE.md`。补充只读查看实际 schema/catalog/storage 代码、既有 schema1/schema5 测试和相关设计/规格。
- 仅写本报告目录；未构建、未执行产品测试、未复制工作区、未改产品、未改 Git index、未运行 npm audit、未读取私人数据库或密钥。首次 `git status --short` 对部分既有 run 目录报告 Permission denied；没有进入这些目录，亦未把它们纳入判断。
- 开发方所述 schema11 15/15、catalog 8/8、schema5 7/7 仅为历史输入，本 reviewer 没有复跑或代签。

## 四项有限缺口

### R1-RF-01：迁移事实保留断言不完整（阻断 R1-02/03/08/09/10 的完整判据）

位置：读取版 `native_release_schema/tests.rs:75–122,219–224,267–343,395–467`。

精确 10 fixture 只有一条 release 行；`release_fact()` 只读取 actor_id、actor_auth_generation、target_id。成功测试只比较 release 表/索引 SQL 和这三列，未比较 release 的 operation/request/phase/error/timestamps 全列，也未预置和比较撤销/审批事实、完整身份和其余旧表行。故障/真实 deferred COMMIT/报告丢失沿用同一个窄 helper。精确 9 成功路径也只验证 source canary、旧结构和 release 列名，没有验证身份字段保留。现有实现看起来只有 additive DDL 和版本 UPDATE，但代码观感不等于这些判据已验证。

最小修复：增强同一个 schema10 fixture 与公共事实快照，不另开场景矩阵。预置至少一条非默认全列 release、相关 actor（含撤销事实）、有效审批事实、原身份及旧数据；按稳定顺序逐表逐行逐列快照（BLOB/NULL 保真，node_meta 只单独豁免预期改变的 schema_version）。在已有精确10成功、全部Source precommit、真实COMMIT失败、COMMIT报告丢失及reopen路径复用比较；同时对精确9成功/回滚比较身份。成功只增加Source两表两索引，失败恢复完整原快照；报告丢失保留已提交11的完整快照。主执行已声明本轮承担该开发测试修改，本报告尚未读取或关闭其后续结果。

### R1-RF-02：synthetic_eight 没有真实 metadata 版本行（阻断 R1-11 双头判据）

位置：`node_catalog_schema/tests.rs:18–73,98–136`；`schema.rs:775–785`（fresh仅建CORE与node_state，未插入node_meta身份行）。

`synthetic_eight()` 从 fresh initialize 开始，随后 `UPDATE main.node_meta SET schema_version=8` 更新零行；`head_and_tables()` 只查 pragma 与catalog表数。因此现有 rollback 实际证明的是“无metadata身份行的8布局”，没有证明定义要求的双头 `(8,8)`。Source四对象显式不存在和完整SQL布局回滚比较已具备，无需再扩对象组合。

最小修复：fixture 插入真实 node_meta 行并核其存在；把 helper/断言包含 metadata 与 pragma 两头，在每个现有故障点断言 `(8,8)`，保留完整原布局快照及Source四对象为零。主执行已声明本轮承担此修改；仍须新摘要与实际重跑。

### R1-RF-03：Source 注入点只断言任意错误（阻断 R1-05/06 的预期错误类别）

位置：读取版 `native_release_schema/tests.rs:395–423`。

8个Source DDL前后点以及SourceMetadata/SourcePragma/Validation/SourceValidationAfter/Commit，共13个已存在的检查点均用 `.is_err()`。若未来因为更早的结构/FK错误返回，这些测试可能继续绿，却不证明实际到达目标故障点。R1-05明确要求预期 migration error。

最小修复：原13个点改为匹配 `StorageError::MigrationInterrupted`，继续保留每个独立fixture、完整快照和双头断言；不新增故障点。真实 deferred COMMIT 单独保留现有 SQLite ForeignKeyViolation / code `787` 断言，不把它改成注入错误。

### R1-RF-04：独立文件数据库入口与少量身份判据尚未验收（阻断 R1-01/04/09/10 的剩余部分）

位置：读取版 release 测试均使用同一内存pool；reopen为 `schema::initialize(&pool)` 再调用，例见 `tests.rs:313–316,462–467,764–792,906–1010`。当前定义要求L3真实SQLite与L5实际initialize/preflight入口，尚无本批独立执行证据。

交给独立tester的一组有限补例：

1. 自有临时文件数据库 fresh → 实际 Storage::open/close → 新连接/新Storage reopen，分别保存原结构、全部事实、pragma/metadata（无身份行时明确记录count=0，而非伪造 `(11,11)`）。可在fresh成功后写入合法身份，再验证有身份的双头11及行摘要跨close/reopen不变。复用报告丢失fixture执行真正close/reopen，明确“实际已提交、报告丢失”。R1纯schema路径本身没有session RPC producer；零RPC可用实际调用边界和必要计数说明，不要求为此启动真实Harness或扩大到R2。
2. 精确10、无node_meta身份行、所有必要事实表为空且Source四对象不存在 → 成功升级11，证明旧版本空身份检查未查询不存在的Source表；随后完整11无身份空库可重开。
3. 同一11 fixture中用受控损坏手段形成多/冲突node_meta行（正常DDL的singleton约束阻止此状态，fixture需说明注入方式并恢复检查设置），实际入口拒绝且保留原始结构/行。只需此一负例，不扩全身份组合。

已有合法Source注册非空、注册+操作非空且无身份的两种有效FK fixture，已验证行为拒绝；另有孤儿Source行负例。有效FK fixture会先由catalog必要表非空检查拒绝，但仍满足外部拒绝行为。本报告**不要求**再扩Source require_empty端到端矩阵；新增Source两表检查的存在及版本阈值已做代码检视，若测试方需要归因，可用小范围定点测试，不新增产品接口。

## R1-01 至 R1-11 映射与剩余判据

下表“存在”仅指读取版有实现/断言，均未由本 reviewer 执行。

| 定义ID | 读取版实际情况 | 剩余最小条件 |
| --- | --- | --- |
| R1-01 | fresh真实内存SQLite初始化存在；Source四对象和无身份空库重验存在 | RF-04的文件fresh/实际Storage入口证据；区分无身份行与metadata版本值 |
| R1-02 | exact_nine按依赖逆序移除Source四对象再移除release；原release三故障点、完整旧布局/canary/Source无残留断言存在 | RF-01补身份事实比较；新固定版实际执行，不要求9预置release行 |
| R1-03 | exact_ten完整移除Source、保留release布局和一行后迁移；双头11存在 | RF-01逐行逐列全部旧事实快照，成功只新增Source布局 |
| R1-04 | 同一pool重复initialize并比较layout存在，无重建分支可见 | RF-01行/双头快照；RF-04实际close→新open及preflight |
| R1-05 | 4 DDL各before/after，独立fixture、精确10双头/布局/Source零对象比较存在 | RF-03错误类别；RF-01完整事实；实际执行8点 |
| R1-06 | SourceMetadata/SourcePragma及最终Validation前、SourceValidationAfter已接入 | RF-03错误类别；RF-01完整事实；实际执行现有点 |
| R1-07 | 结构拒绝、main/temp触发器、extra index、4种temp shadow、缺表/索引、双头10/11与11/10、高版、旧版部分Source、错误列均存在；注册父FK形状与actor/source/harness/workspace/profile ID/profile revision坏引用、operation复合FK形状/instance坏引用均有具体测试 | 对这些现有实际限定名运行并保留拒绝前后证据，不再要求“所有版本×所有负例”笼统全组合 |
| R1-08 | DeferredCommit在完整validate之后制造真实deferred FK，测试匹配787/FK类别并查回10 | RF-01完整原布局/事实快照，而非仅Source对象数和release三列；实际执行 |
| R1-09 | CommitUnknown在真实commit之后注入；保留双头11/四对象/三列release，同pool重验layout | RF-01全事实；RF-04实际文件重开；报告准确区分实际提交与报告丢失 |
| R1-10 | `current>=11`才查Source空表；`current>=10`保留release检查；合法无身份空11、Source非空拒绝均存在 | RF-01保留事实；RF-04空10升级及多/冲突身份；不扩Source空身份端到端矩阵 |
| R1-11 | exact9/exact10/最新11分离；catalog fixture已按依赖顺序移Source，且明确零残留 | RF-02真实metadata双头8；最后按实际list逐叶绑定精确限定名、数量与新摘要 |

产品代码检视确认：`storage::SCHEMA_VERSION=11`；release仍以`current<10`迁移；Source以`current<11`在同一BEGIN IMMEDIATE中按固定顺序执行main-qualified四DDL，再写metadata/pragma11、完整验证、真实COMMIT。完整11重开验证缺表/索引会拒绝，没有自动修补；现有preflight worker/join及pool/registry owner未在本批被替换。上述为静态检视结论，不是L5运行结果。当前四DDL内容本身已查看，但未进入旧候选目录重新做字节级来源比较，不对旧候选其余实现作背书。

## R2 方案/用例可以复用什么

- `REBASE-PLAN.md` R2/R3与最小验证顺序仍可作为下一轮方案输入，当前四项测试缺口不要求改变产品行为或重新定义Source DDL。R1 schema结果不能代签R2 discover/create、R3认证入口或完整WP4。
- 可复用旧 `MINIMUM-MAPPING.md` 的七组**风险分类/稳定用例ID**及已冻结的“同runtime/receiver、原reserve与typed response匹配、Written/Unknown不重发、真实创建身份、SQL task/receipt所有权、Doc传输与关闭claim保护”约束；本报告没有重新读取并逐条核准旧七组资产，复用权威仍是当前方案第50–55行及旧固定资产本身，不把旧绿结果搬成当前通过。
- 独立tester只补当前变更差异：schema10→11与当前owner接点；Register/Query caller cancellation；原SQL三故障轴；真正调度前的no-SQL-start观测；新result owner长列表解码/增长/最终析构的Node工作与retire槽。尤其 `SSF-SQL-RECONCILIATION001` 继续Open，query不得洗掉原Unknown/Err，reconcile必须join原task并确认原receipt且不发session RPC。
- 开始R2产品写入前，由tester把上述差异落到实际入口与有限用例，reviewer增量确认，不能宣称本R1测试差异门已授权R2。执行先做方案规定的真实child discover、真实create response、WrittenUnknown+重复操作三条最小链路，再按七组风险扩展；沿用实际`--list`全限定名、固定源摘要、运行数量和自然清理证据规则，无需重编不匹配目标以外的无关内容。

## 关闭与交付门

四项缺口关闭后，以**同一组最终源摘要**复查修改并由独立tester覆盖R1-01..11：实际`--list`限定名与数量、真实Red/Green（旧实现失败/新实现通过）、SQLite前后快照与双头、独占临时DB、测试自然退出及清理。已有同版同范围真实证据可引用，但须可读原始记录并核同源身份；不能用开发汇总15/15、catalog8/8或schema5历史7/7代签。

本次状态：静态检视完成；四项为open；测试执行/RedCheck/独立R1验收为not-run（本任务明确只读、禁止构建测试），R2/R3未审实现、未验收。不修改原有历史门/用例定义/任务checkbox。
