# CL-004 / schema3迁移与布局复测

2026-10-31。**独立复测13/13通过，限任务2.3的布局/识别/初始化/迁移及已明确的预检边界。** 新快照未发现该子范围的剩余执行失败。SCHEMA-R003状态仍由原独立reviewer依据本次证据复核关闭；本报告不代关其问题，也不签整个CL-004或产品会话闭环通过。

## 预期修订依据与历史保留

父执行修订implementation-contract.md §4A，独立reviewer明确判定原“WAL拒绝时文件清单完全不变”为过强工程假设，七份产品spec没有此承诺。当前依据要求标准read_only读取完整SQLite/WAL提交视图，拒绝不改变主DB、原有WAL字节/提交事实、不删/截断/恢复原文件或写应用DDL/版本；只允许owner私有目录中的非权威SHM维护及原不存在的0字节WAL。原ID仍为CL-004，本轮无新增用户产品决定。

独立定义已在schema3-migration-definition.md的SCHEMA-R003节修订。旧测试完整源码保留为schema3-before-r003-test.rs（原SHA c85d5a632b54efab0ab8fd4416f2ff72948be29c7a0f220384509c52996b584c），旧报告schema3-acceptance.md、首轮7/10失败日志schema3-acceptance-attempt1.log、WAL单例失败schema3-wal-inventory.log及最终11/12日志schema3-acceptance-final.log均保留。没有将旧失败改成旧版本通过。

父执行曾试验immutable漏读已提交WAL后回退；该反证为父执行/原reviewer证据，本tester没有冒领该次执行。本次实际代码仍为read_only(true)，新增直接preflight读77的独立测试能防止用后续RW校验掩盖预检漏读。

## 冻结版本与可持续资产

主仓库独立持续测试：app/tests/acceptance/schema3_migration.rs，经lib.rs cfg(test)模块编入。最终测试SHA-256：`2fb2e326088eb107bcd05ae6a949238c9a073803e64a2f6357886926a5d9dc91`。完整生产/测试/合同/owner前置摘要见schema3-r003-source.json。

关键生产摘要：schema.rs `9a80c9e01cb67396eef4bf53eff7d7b3c0b4b1e86373c5b4da4da19dd1c33b2c`；storage.rs `a91c4125ed82921ee0d68c97e36efba522e7fe60735f38afcc0e227124e79281`；node.rs `c97a0f51f3c878801bcc259452242fac9bd6f3764ec45da50443184fc38c0067`；合同 `d805da41f7b89fe073fb7dcc5d35609a928d7097467504fc7eba1de06b968a8d`。其余product_schema/原管理与身份代码按清单固定；未改产品模型/DDL/预检实现或产品锁。

所有数据库为tester专属外部sql-fixtures/中的自有副本，旧schema1/2 DDL固定从f6d1f8df原schema抽取，不使用当前DDL常量伪造旧版本、不先建3降版。没有用户正文、真实身份资料、Harness或网络依赖。测试完成关闭连接并清理自有临时目录。

## 实际命令与结果

tester独占CARGO_TARGET_DIR=外部activity-target，执行：

`cargo test --manifest-path app/Cargo.toml --locked --offline --lib cl004_ -- --nocapture`

| 执行 | 结果与证据 |
| --- | --- |
| 首次修订预期后的复测 | 13 passed、0 failed、0 ignored，退出0，schema3-r003-retest.log。此前dot-source owner脚本被当前执行策略拒绝，但实际owner/ACL断言通过；没有把脚本前置误报成功。 |
| 仓库owner前置成功后的固定版复测 | 用run-schema3-owner.ps1，在单次子进程以ExecutionPolicy Bypass运行仓库testing/ci/windows-owner.ps1；实际输出“CI new-object ownership matches the process user”。随后13 passed、0 failed、0 ignored，退出0，schema3-r003-owner-retest.log。没有持久改机器/用户执行策略或已有文件owner。 |
| 静态与资产格式 | cargo clippy --manifest-path app/Cargo.toml --lib --tests --locked --offline -- -D warnings退出0，schema3-r003-clippy.log；测试单文件rustfmt检查与git diff --check退出0。 |

仓库owner脚本只调整本次进程token的默认新对象owner，实际文件owner通过独立Get-Acl再次核验。PowerShell将cargo stderr进度装饰为NativeCommandError，以上状态均取Rust实际13测试摘要与进程退出码，不将该装饰误记为测试失败。

## SCHEMA-R003新增实际断言

1. **未checkpoint提交视图**：自有已升级数据库关闭正常应用连接，禁用测试writer自动检查点并执行TRUNCATE检查点；随后在该writer上将版本77及合成审计marker提交到WAL。保持writer打开但不再写入，将主DB+非空WAL复制到另一个独占固定目录，目标无writer且持有DataDirectory锁。直接schema::preflight与open_owned均返回UnknownSchema(77)，独立read_only查询实际看到版本77及committed-wal-marker。主DB与原有非空WAL前后逐字完全相同。没有使用immutable、删WAL或先checkpoint目标来换取结果。
2. **新建空WAL**：对原本无WAL文件、但头部保持WAL模式的自有拒绝副本，实际观察SQLite创建WAL且长度恰好0；允许的新文件只有WAL/SHM，原主DB及身份哨兵逐字不变。不是仅将“文件存在”当作合法。
3. **owner私有权限**：两类副本均经真实Windows DataDirectory.acquire与Storage.open_owned。独立PowerShell Get-Acl逐项读取目录及其文件：owner为当前用户，allow ACE只有该owner，目录DACL受保护，新SHM具有继承的私有ACE；没有用产品自身ACL函数返回值自证成功。
4. **单节点锁**：owner guard在正常迁移和失败预检期间持续存活，第二个DataDirectory.acquire实际返回DataDirectoryBusy；失败返回后仍不能取得第二owner，原guard释放后能再次取得。本轮是固定无writer/独占副本，不宣称支持任意未协调外部writer并发。

## CL-004其余子范围重跑结果

13项包含此前全部代表性分支，已在本次新快照再次运行：原schema1/2无损升级/双版本/重开epoch与水位保留、旧身份/撤销/管理/正文/审批逻辑完整保留、五个真实事务阶段故障回滚、两版真实延迟FK COMMIT失败、未知/矛盾/残缺只读拒绝、R002缺metadata的CORE各类/SESSION/product/floor事实与合法空bootstrap、合法无Run审批及跨目标load/Run真实FK拒绝、STRICT整数和审计四槽形状/90天精确边界。详细子断言沿schema3-migration-definition.md A–E，原报告的范围解释不变。

## 未验证范围

新产品受理/危险派发/单次认领/临时输入消费、10000命令与100000审计槽容量事务、一次预留四槽/不可覆盖API/最终reconciliation消费、水位CAS/时间回退/隔离恢复、真实审批来源及交付，以及新入口运行时从不读写旧正文全部仍未验证。空product表不能证明不存在正文读取；布局通过不能证明这些后续流程已实现。DPAPI、真实网络认证/Harness与Windows完整进程树也不在本轮证据范围。
