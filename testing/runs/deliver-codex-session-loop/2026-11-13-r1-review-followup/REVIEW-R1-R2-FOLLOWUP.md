# R1/R2 增量独立复查

日期：2026-11-13。角色：独立 reviewer。范围绑定：R1 complete-facts development 冻结 manifest canonical SHA256 `b9de84e397f31d3f7882c807b07ef28b47fe4774f93c6819cacce035498951aa`；R2 定义目录 `2026-11-13-source-r2-definition`；未执行编译、libtest list、动态测试或产品写入。

## R1 finding 复核

- **RF01 关闭（仅开发测试 finding）**：`SOURCE-FROZEN.json` 记录完整旧表逐列快照、两条 release 记录（失败/撤销/审批事实）及成功、13 个 precommit fault、真实 deferred COMMIT 787、报告丢失/重开路径。RESULT 记录 release 15/15、catalog 8/8、schema 7/7；实际读取原始 stdout 与 stderr。开发测试叶 SHA256 与冻结清单一致：`native_release_schema/tests.rs` `60edc48be3a0208f6da64a5df1e67c3a12db7385dd7ed010387401d14afd5494`，`node_catalog_schema/tests.rs` `3b17c32d6864c2cdbcc759ccbfea45ce00de947cc3fd266cb40ace1ee257fef3`。
- **RF02 关闭（仅开发测试 finding）**：catalog `synthetic_eight` 插入真实 `node_meta(singleton=1,schema_version=8,...)`，`head_and_tables` 对元数据与 PRAGMA 做一致性断言；预提交故障逐例保存 identity、双头和零 Source 对象。冻结对应叶 SHA256 同上。
- **RF03 关闭（仅开发测试 finding）**：Source 13 个注入故障均匹配 `StorageError::MigrationInterrupted`；真实 `DeferredCommit` 仍独立匹配 SQLite FK code 787。冻结对应 `native_release_schema/tests.rs` SHA256 同上。

关闭边界：上述是对开发测试叶及其冻结证据的独立复核，不是 R1 file acceptance，也不是独立 R1 完整验收。RF04（真实 file Storage/preflight/close/reopen、无身份精确10及冲突 node_meta）保持 open，等待独立 tester 的 runner、Red/Green 与动态证据。

## R2 方案/用例增量意见

`REBASE-PLAN.md` R2 与 `TEST-DELTA.md`/`ENTRY-AND-OBSERVABILITY.md`/`HANDOFF.md` 一致：首批仅 V01 discover、V02 create response、V03 WrittenUnknown 重复不重发；随后 M01..M07 七组有限风险，不扩为全排列，不越权代签 R3。真实 Node/SQLite/child/原 transport/唯一 ACP receiver、原 configuration/runtime/registry/Doc owner 边界写清；query 不洗 Unknown/Err，reconcile 先 join 原 task/receipt，`SSF-SQL-RECONCILIATION001` 保持 Open。

编码前仅保留两项最小决策：

1. **CPU/retire 槽**：coder 必须选当前 Node-owned decode/Vec-work 槽和最终 payload/partial Vec disposal 槽，写明实际 task identity、start/join 观察与 close 纳管。不得仅以异步包装或旧 Queue/Native 结果代替。
2. **原 SQL receipt**：Register/Query/Response SQL 必须在 schedule 前保留原 task/receipt/close receipt；需提供调度前不可伪造 `NeverScheduled`、SQL-enter 与 caller-cancel 后 owner 保留 identity。query 只能读取已证事实；reconcile 先确认原 receipt/task/close，再按 typed proof 修复，零 session RPC。

这两项补足后可开始 R2 首批写集的只读接线准备；当前未给出产品实现放行或 R2 交付结论。

## 证据与限制

已读取：冻结 `SOURCE-FROZEN.json`、`RESULT.json`、release/catalog/schema 原始 stdout/stderr、`REBASE-PLAN.md`、R2 `TEST-DELTA.md`、`ENTRY-AND-OBSERVABILITY.md`、`HANDOFF.md`、`INPUTS.json`、R1 file acceptance `private_file_acceptance.rs`。当前 file acceptance 目录仅有测试叶，未见 runner/Red/Green 动态结果，故不放行 RF04。
## RF04 runner 静态门（未放行执行）

已复核私有四例及 `runner.mjs`/`run_owned.ps1`。四例所用 `Storage::open/close`、`SqliteFixtureScope`、真实原生命周期 owner、preflight、Windows exclusive share probe 与 CommitUnknown 后第二次新 Storage reopen 均绑定当前实现，scope合理；四例断言覆盖fresh无身份 `(pragma=11, metadata=0)`、受控身份后 reopen、空精确10、冲突身份只读拒绝与提交已发生而报告丢失。程序预定跑完整冻结516叶 plus仅测试 overlay，受影响叶预期19+8+7；Green完整限定名逐例执行并要求 `running 1 test`，Red只期待 fresh test `schema10 != schema11`，反转限 `storage.rs` 常量与 `schema.rs` Source接入，保留overlay断言。

当前**暂不放行** `prepare/compile/list/dynamic`，因为审查时有以下三条具体缺口，已发给 tester：

1. Green/Red command 共用 `R1_EVIDENCE_DIR`，Red fresh运行会覆盖Green同名事件，应按阶段或运行标签分隔并在结果manifest引用。
2. 编译前应对 isolated source 的516文件及实际 test overlay 做本地身份校验并固定其摘要；Root原manifest校验不能证明copy后实际PE的完整输入未变化。Red亦记录两叶许可差异及断言overlay不变，finally后校验两叶恢复。
3. `run_owned.ps1` 在应用子进程 `TEMP/TMP` 前 dot-source `windows-owner.ps1`；该脚本通过 `[IO.Path]::GetTempPath()` 创建 ownerProbe，因此父wrapper会触碰默认私人临时目录。须先把本wrapper临时目录绑到 runner owned scratch。

每个测试的失败清理由 tester 补充自有目录兜底和留证策略；不覆盖任何其他区域。上述项修复后只需复查该runner的有限差异再给放行，不扩用例矩阵。

## 本报告固定源SHA

R1 516叶 canonical SHA256 `b9de84e397f31d3f7882c807b07ef28b47fe4774f93c6819cacce035498951aa`；冻结manifest六个 changed leaves 均与实际读取摘要一致，见 `SOURCE-FROZEN.json`。R2定义的输入身份以其 `INPUTS.json` 文件摘要集为界；当前Root共享未提交，HEAD `65ec7815bb0280f4519989d93a7b4ed3b4b37994` 不替代工作区源身份。文件 acceptance runner/overlay 本轮只读，不固定为可执行受测版本，不运行任何命令。

## RF04 程序门增量结论（已放行有限执行）

以上“未放行”是修正前的审查状态，现由本节及 `RF04-PROGRAM-GATE.md` 的固定程序门取代。三条具体程序缺口均已关闭：证据按 `evidence/<label>` 隔离；`verifyCopy` 在编译前逐核516叶、overlay及程序SHA，在Red恢复后和Green最终再次留证；wrapper读spec后立即应用自有TEMP/TMP，再进入worker分支和加载owner脚本/Add-Type。已独立逐核既有副本全516叶，除已允许的测试模块声明外零差异，私有overlay一致；未复制、构建、list或运行测试。

正式门：`RF04-PROGRAM-GATE.md`，SHA256 `5532a8a119a3440c3e04ac62320f92b9de616bade1a8eeb9898e2e77ff75c591`。绑定 `runner.mjs` `cdbf8608b707848ff01508beeccdbafbdf213c74402c9b125c1af1a3d11d3eb3`、`run_owned.ps1` `bda21c9f12472883cd9a22e692bfd50325ed5c94932197f06df5890990d2944e`、`private_file_acceptance.rs` `f005e4f07671a16d166d705f69db5db8e36362ea0b9266528a50166a1cb85d43`，附加测试叶 `0c8757f53b5ae0ee6ea22cc2bce580014a34807a4c93724c8e8b951eda8be4a7`。已直接发给独立tester，允许先1项旧10 Red，恢复并核摘要后重编/list，再跑19+8+7共34项Green；完整名逐例恰好1test，原owner自然关闭并零活跃进程。

此门仅确认程序静态审查及有限运行范围。RF04仍Open，须复核真实动态结果、原始日志、Red oracle、文件快照与清理证据后才可关闭；R1完整独立验收与R2产品写集均未由此门代签。

## 最终独立动态后验

2026-11-13：以上Open为历史阶段状态，由本目录 `REVIEW-R1-ACCEPTANCE.md` 与 `REVIEW-R1-ACCEPTANCE.json` 的实际后验结论取代。当前冻结516叶的Red1精确10≠11、Green34逐项真实通过且所有list/test原Job自然归零、Stop=false，四文件用例全快照/13独占probe/4自然cleanup及最终Root/copy/index/程序/PE均复核通过。RF04关闭，R1-01…11有限独立验收通过。编译vctip-only owned Stop仍保留为监督异常，不冒称编译自然空；不代签R2/R3或完整WP4。R2/Doc各有限写入门的R1前置条件现已满足。
