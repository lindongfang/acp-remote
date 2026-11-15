# 旧正文/强刷接口构建隔离预审

2026-10-31，reviewer `/root/baseline_reviewer`，只读候选与实际依赖图。初读合同SHA256 `c7a34b87d339e5ddc31612734380b62abca92d97e6c21f02ca23e69da238e120`。

结论：**隔离方向符合已确认规格，没有新产品冲突或需要用户再裁决的分支。** 当前仍待独立验收定义核对及最终白名单/路径适配冻结后作正式编码门槛结论；没有据此修改产品。

## 构建边界判断

- 正常库删除旧InputRepository/InputService、application/history及旧approvals的编译入口，并将SessionStore/HistoryStore/ApprovalStore和三个Storage旧工厂只留cfg(test)，能阻止新产品从普通依赖调用旧正文/强刷/必须Run路径。不会因“库也跑单元测试”自动让普通外部依赖拿到这些接口；正负编译测试必须使用正常依赖构建。
- 新ProductRepository/ProductStore、native/activity/requests/views及schema4迁移继续生产可用；正常node/trust依赖没有需要旧会话API的调用。现有身份、撤销、目录锁与原数据不需要改变。旧纯数据类型可以保留，但不能充当新私有proof/permit的构造能力，原独立类型负向边界仍适用。
- session_storage的原实现主体可原样迁至legacy.rs；只调整模块声明/path和test兼容re-export。旧approvals内的`super::history`、`super::require_actor`必须仍指向同一测试历史实现，不能在移动时重新实现SQL或改规则。新生产模块不能被重复声明进legacy或反向依赖legacy。
- 没有需要新Cargo feature、环境开关、迁移、清库或测试专属生产授权入口的理由。所有旧正文表/历史行及schema4 DDL保留，新产品继续不读写旧正文。

## 已发现的具体路径适配

`app/src/session_storage/history/tests.rs:656`以`--exact`硬编码启动`session_storage::history::tests::crash_after_reservation_child`。迁入legacy命名空间后，兼容re-export不改变测试函数在libtest中的实际名字；必须仅修过滤字符串为迁后实际路径，并继续原有`exit93`、`VISIBLE_POSITION=1`、保留区间不重用及未知尾部断言。不能删/ignore该回归，也不能将子进程跑0测试视为成功。

合同允许必要路径适配，属于已授权工程细节；最终文件白名单须明确这一测试文件的过滤路径变动。其余旧业务代码/断言原文保留，迁后测试名可以按`session_storage::legacy::`前缀映射比较，旧用例总数与执行意图不能因目录搬迁减少。

## 独立定义必须体现的证据

- 普通外部小crate对三个Storage工厂、旧application模块/trait/service、旧session_storage模块/类型逐入口负向编译；检查预期E0432/E0599/隐私等准确错误来源，不能凭任意语法/依赖/环境失败算通过。
- 相同正常依赖下EntityId、新ProductStore/ProductInputService/MemoryInputs及原Storage身份/迁移等正向编译。禁止强制整个依赖cfg(test)，禁止添加生产feature恢复旧API。
- 正常cargo build证明生产编译面；实际cfg(test)完整旧单元测试、crash child真实退出/标记、新输入31、迁移24、公开模型以及适用完整检查分别执行，过滤/忽略不能偷偷覆盖旧义务。先保留旧测试清单及源码，再对比迁后清单和实际结果。
- 比对新生产management/domain/schema/product_schema、锁及旧正文fixture事实保持；旧SQL实现主体逐字迁移，只许可模块/path/文档变动。保护已合入schema4版本与shared ProductStore健康，不用旧冻结Storage整文件覆盖。

README及历史开发记录应准确描述“旧逻辑仅用于历史回归”，不将cfg(test)旧通过称为新版产品能力。源文件/文档白名单按父执行最终合同；不顺手清理领域旧类型或无关配置。

本次没有运行测试、数据库、真实Harness或Web audit。正常依赖隔离、完整旧回归与实际新组合全部尚待独立执行，不签整个change或任何真实闭环。

追加定义核对：已实际读取最终补充合同SHA256 `ff8b569f72539a80857b77406b5d799ec86ab1c7909277557ab1f3ba625e196a`。白名单现明确history/tests.rs仅机械修改子进程实际legacy命名空间的--exact路径，exit93/可见标记及全部其他断言不变；该工程缺口已关闭为定义明确，仍待独立子定义再给编码门槛。

范围纠正：亦已实际读implementation-contract的清理更正。此前“本change不实现清理”不是可继续采用的全change规则；按更高优先的现行spec，只有已审当前事务subphase保守不清理，完整变更仍须安全清理可删除审计部分、保护有效/未收敛去重及资源依赖，并证明清理后不会重派。已签schema/管理子阶段不因此扩成CL-080–084完整通过，也不回写冻结旧报告/代码；此隔离阶段不能借搬迁删除任何数据。

## 最终独立定义及编码门槛

2026-10-31追加：实际完整读取独立`legacy-isolation-definition.md`并独立计算SHA256 `78247c554953af730dba57773d6b524284d73db6822c48510418ee93ab7ae963`，与ff8b569f合同一致。**正式方案/独立定义门槛通过，允许父执行在当前root4完整检查收口后按白名单编码。** 本次是方案放行，不是隔离已经实现或执行通过；当前完整入口是否通过仍由实际运行证据决定。

逐项核对充分：

- 11个普通依赖负向入口每次单独构建，要求准确目标span及诊断，不以任意报错代签；普通依赖正控制实际调用新Storage.product_commands工厂且导入ProductInputService/MemoryInputs/稳定领域类型，不强制依赖test cfg、不新增feature、无执行SQL/派发。
- 外部锁只允许本地探针根包，第三方依赖与现有产品锁对比；构建/环境缺失另记，不冒充API隔离。正控制本身拒绝test cfg，仍须核命令/配置没有全依赖强制test。
- reviewer已自行从4份当前旧测试源码提取实际test名，与保存old-tests.json逐名排序比较：22/12/12/10，合计56，全部一致、差异0、ignore0。定义要求迁后实际二进制逐名映射及真实执行，不用新增测试凑数；application历史审批无独立test模块的事实也准确，没有虚构旧用例数量。
- 真child父用例必须未因环境变量预置早退，单独`--exact --nocapture`实际执行，保留exit93/可见位置标记和数据库区间断言；迁后真实命名空间明确，零匹配exit0不能通过。
- 原旧实现体逐字迁移，模块/path及唯一child过滤字符串为机械差异，业务断言不变；新管理/DDL/签名/模型/锁等固定源保持。正常build与cfg(test)完整旧回归有不同证据，不能仅cargo test当正常边界证明。
- 最终组合重跑迁移24及管理31，明确保留新正文不入旧表/SQLite、旧管理/撤销事实不改、未知不重派；这里的旧测试保留不把旧强刷或必须Run规则恢复成当前产品权威。

无剩余必须用户裁决或编码前设计阻断。此门槛不授权白名单外重构、旧数据删除、生产开关、新审批owner/WSS/真实Harness，也不关闭任务2.3其余义务或全change。冻结移动后源码和独立执行齐备后，另行静态与交付审查。

## 首次机械编译后的三文件白名单补充

2026-10-31追加：已实际复读合同SHA256 `77521442e75e30ba64725efee8320abc07fc00ee6cef6a3ad934f5bf77d4fb88`、独立定义新SHA256 `dea0fe2b883e5a5f958ad0de0a656de3a101c29e49316907f80e997f4f79150c`，并检视当前legacy模块声明、两个SQL文件及全src的旧domain引用关系。**同意这三文件仅机械属性修正，独立13入口定义充分，阶段方案门槛继续有效。** 不需要用户新增决策，不把首编译失败伪报为业务断言失败。

1. `session_storage/history.rs`和`approvals.rs`各只给原cfg(test)子模块指定真实物理`history/tests.rs`/`approvals/tests.rs`，避免经显式path搬移后误加载父tests.rs。原12+10旧SQL用例文件、业务方法/SQL及断言保持，不能靠重复父tests凑数量。
2. `domain/mod.rs`仅将旧history/approvals两个声明cfg(test)。当前这些领域类型的使用者为已隔离的旧app/SQL/测试，新ProductStore/new domain/node/trust无反向依赖；隐藏它们使旧crate-private claim字段不再留在正常构建，并保留test中的原历史回归。不得以allow(dead_code)或业务方法改动掩盖问题；EntityId/ActorContext/根纯值/新模型不变。
3. 独立N12/N13分别验证旧domain::history::HistoryItem及domain::approvals::ApprovalBinding在普通依赖不可达；原正控制、旧56逐名/真实child93-marker及24/31组合判据保持。原11定义保存旧78247全文归属；新增domain/mod前态独立保存，不覆盖初始基线。

以上只是解决已确认隔离规则下的模块装载与编译依赖，不批准新行为。移前完整root4检查即使通过，也不能代签移动后；最终须冻结所有实际属性/模块/path变化，重跑独立正负构建、实际旧56与child、24/31及适用完整检查后再交付。
