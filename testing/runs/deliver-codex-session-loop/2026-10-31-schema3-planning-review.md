# schema3布局/迁移子阶段独立方案检视

2026-10-31，reviewer `/root/baseline_reviewer`，只读项目。范围限任务2.3的DDL/结构识别/增量迁移；不含产品受理、水位CAS、容量事务或派发。

初次结论：**存在一项应在布局编码前修正的数据库完整性缺口，待复核；其余受审迁移方向未发现产品决策阻断。** 该问题属于已授权工程细节，不需用户再次裁决产品规则。

受审合同SHA-256 `2fa401b656de20c1723008cdff7175c463d0a2850dd22f7278d56b98cfa5f54f`，独立tester `schema3-migration-definition.md` SHA-256 `873f8a6371f0dc1e664183bbe9fad73bd65c6b2339b0b167219fdb33561044e4`。实际读取现有schema2识别/初始化源码，未运行SQL或打开任何数据库。

## SCHEMA-R001

- 严重性：P2，阶段编码前布局问题。
- 位置：implementation-contract.md §4A，product_resources / product_approvals行。
- 依据：审批必须绑定真实原生目标及当前资源；无Run审批同样要求目标/加载归属一致。
- 当前结构分别以target_id引用product_targets、load_epoch引用product_resources；合法目标A的审批可引用目标B的加载资源。run_id=NULL时(target_id,run_id)复合外键被SQLite跳过，不能弥补这一关联缺口。
- 修正建议：product_resources增加UNIQUE(target_id,load_epoch)，product_approvals以(target_id,load_epoch)复合外键引用它；保留run_id可空及其与target_id的复合外键。
- 独立断言：两目标各有resource；同目标/同load且run=NULL成功，跨目标load无论run为空与否均FK拒绝；有run时跨目标run也拒绝。
- 状态：open，待新合同/用例定义复核。本阶段没有已编译DDL可验，不把模型建议写成产品已经实现。

## 其余核对

- product_前缀新表不引用旧Turn/Item/content/approval，旧CORE/SESSION保留不删；schema1/空库先建立兼容旧空结构，schema2只加新结构。旧正文/审批事实不迁成新权威，正常打开不重新派发。
- 新control在初始化事务生成唯一请求代际，水位0；完整schema3重开只校验，不重置epoch或水位。运行/连接/load代际不替代该持久代际。
- preflight以只读连接核验，带锁写入后BEGIN IMMEDIATE再核验，DDL/两版本/完整3验证及COMMIT处于同一事务；未知、矛盾、残缺拒绝不重建。空库初始node_meta尚未建立的既有身份初始化顺序需实现时保持，不用强制metadata行破坏新安装。
- cfg(test)内部故障点不进入生产；DDL前/中/后、元版本、PRAGMA版本、最终验证和真实deferred-FK COMMIT错误独立覆盖。PRAGMA窗口不能拿metadata trigger失败代替。COMMIT实际失败回滚与已提交但结果未知严格分开。
- 独立用例A–E覆盖两类固定旧fixture、逐行/BLOB/身份无损、正常重开、只读拒绝文件副作用、真实事务回滚及布局隔离。fixture不使用变化中的新DDL生成所谓旧基线；新入口尚不存在时运行时不读写旧正文保持not-run。

四个固定审计slot预留作为后续受理设计已明确计入100000容量，空slot不伪装已发生事实。后续需明确reconciliation在最终协调完成时消费，或提供其他有界方式记录多次核验，避免首次unknown占用后不能记录最终收敛；不以当前单纯建表通过签容量/核验行为实现。

全部迁移及产品用例未执行；此报告仅定义检视，未修改产品或数据库。

## 修订复核与最终阶段结论

2026-10-31，同一reviewer实际重读修订合同与独立tester补充：合同SHA-256 `8390ee26d36063db39123bb2aae5d79b9c135d82890560bdb4cb86a0a974ba4e`；tester定义SHA-256 `1e5d2b5c96dbc56501a3661ac97a372485d7fa816d2d0b60d692b50afbacacc2`。

SCHEMA-R001：**Closed（设计）**。product_resources现明确UNIQUE(target_id,load_epoch)，product_approvals使用对应复合FK；run可空的复合FK继续保留。tester已纳入合法无Run同目标load、跨目标load即使run=NULL也拒绝，以及有Run跨目标拒绝的独立SQL断言。该关闭只说明设计及用例缺口已补，不声明DDL实现或运行结果通过。

审计reconciliation现明确仅在必要管理协调完成时消费；中途待核验观察不占完最终记录，不恢复危险受理。后续显式安全控制以自己的Command留存事实，不改写旧未知。此收口与当前布局方向相容，实际事务/容量行为仍后续另审。

最终结论：**任务2.3的布局/识别/初始化与迁移子阶段方案和独立用例定义通过，可以开始该限定范围编码。** 旧表保留、无Run审批归属、control幂等、两版本事务及真实引擎COMMIT失败定义明确；没有需用户新裁决的分支。

准确证据边界：cfg(test)“PRAGMA前”注入只证明metadata已变后的事务错误可回滚，不能宣称SQLite实际拒绝PRAGMA语句；独立定义已作相同澄清。真实deferred-FK COMMIT错误仍是必需独立分支，不能以提交前hook替代。正常空库身份metadata建立顺序与确切DDL枚举/NULL一致性须在实现检视和真实约束测试中核对。

当前未运行迁移或数据库测试。后续受理/认领、必要水位CAS、10000/100000容量、真实审批来源交付、旧正文运行时读写隔离和危险派发均不在本阶段放行范围。
