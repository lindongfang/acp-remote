# schema4纯布局/迁移实现独立静态检视

2026-10-31，reviewer `/root/baseline_reviewer`。只读主区实际源码及旧冻结副本，未修改产品、数据库或验收预期，未运行真实Harness。

当前结论：**限定静态检视通过，未发现本次布局/迁移阻断。** 等待独立K11及原13适配的实际同版执行证据后，再签布局子阶段交付；开发11例及clippy不能代签独立验收。审批owner、决定/响应/ReplyCell和任务2.5整体均未签。

## 冻结文件

以下SHA256均由reviewer实际计算，与主执行交接相符：

| 文件 | SHA256 |
| --- | --- |
| app/src/session_storage/schema.rs | fcec6f7e929056bdf7809534c7f9566c7bbe77f5ebb0c2a47be1a00e06564d18 |
| app/src/session_storage/product_schema.rs | e3d2d32c0e3ec32f1c6ef4ed4a971f31c4f203bd8ca6d67063f81b4f14aa6c00 |
| app/src/storage.rs | e837ffd3f8a4754ccb4be279aede3ff2483089aec40a2785fa89583a5eaa5be6 |
| app/src/session_storage/product_schema/tests.rs | 5474fa85089a02d630ed8ffdea95a08f9d2a6131b690f97d598e94f32ae57e32 |
| app/src/session_storage/product_schema/delivery_tests.rs | 34822fbb82488901297734709410cf4b9e888644d31bc665efd09b3c7f19fc17 |
| app/Cargo.lock | 87f27b619281b37015c79ceae8d95471c8a4de50f202f4dd8a6d99fa0d27e324 |

依据为已通过的schema4-layout-planning-review、fe621e合同§3A及独立K纯布局定义c37bdb47（另窄定义由tester独立持有）。实际对照原schema3的schema/product_schema源码和Storage基线a91c4125；Storage对该基线只有SCHEMA_VERSION 3→4，尚未合入外部ProductStore管理patch。不能把它与外部revision3的Storage全文直接比较后误判为删除管理功能。

## 核对结果

- CORE、SESSION及原product3 DDL完整未改，只追加独立DELIVERY_DEFINITIONS。新复合unique索引及STRICT attempt表与合同一致；显式NULL交叉CHECK、claimed/transport_written的actual条件、非NULL childclaim及复合FK均保持。DB仅长度/唯一/关联，不宣称SQL canonical128检查或审批应用状态机已实现。
- 只在current<3时生成唯一control和随机epoch；3→4/4重开不写control，不重置epoch/floor、不推断旧delivery为actual。不创建新Run/正文或读取旧正文值为新权威。
- read_only预检后在BEGIN IMMEDIATE内再次验证；创建所需旧结构/3控制行，再CREATE新索引/表，更新node_meta与PRAGMA为4，全布局验证后COMMIT。任一步错误由事务回滚/关闭pool；没有写后还原冒充只读拒绝的路径。
- 支持范围依SCHEMA_VERSION至4；双version矛盾、残缺旧/新定义或control拒绝；schema3夹带任一新对象拒绝，1/2继续拒任何partial product对象。>=4的历史缺meta检查包含新attempt表，沿R002使用EXISTS不读正文，只有合法空bootstrap仍允许；floor非0不被空meta重新初始化。
- 正常preflight仍标准read_only，无immutable/snapshot helper、无删除WAL/SHM。Storage open_owned借用目录owner的生产路径不变，WAL完整视图/原DB与现有WAL不变、非权威SHM及缺失0字节WAL仅沿已审R003边界，实际同版运行证据仍待独立tester。
- typed MigrationFault及全部checkpoint、固定测试表/孤儿行、CommitUnknown均由cfg(test)排除于生产。Ddl首次写前，Product3Ready旧3后，DeliveryIndex/Table各CREATE后，Metadata/Pragma各写前，Validation/Commit各实际边界，符合已冻结时点。
- DeferredCommit真实DEFERRABLE外键孤儿INSERT位于事务内，真实commit才失败；CommitUnknown位于transaction.commit成功之后，返回测试错误而不声称回滚。不开放任意SQL/版本参数，不通过网络/环境开启。开发测试明确区分SQLite787 COMMIT拒绝与durable4但响应未知，并用独立连接验证；这些开发断言不计为独立实际运行。

## 后续交付边界

仍需独立真实1/2/非空3→4及4重开、数据/旧DDL/epochfloor保留、每hook回滚与真实COMMIT/成功未知、新NULL矩阵/FK/STRICT、完整未checkpoint WAL和目录锁/ACL实证。raw SQL约束通过不等于attempt/投影/result应用事务、真实来源或单次ReplyCell已经正确实现。

外部管理revision3仍基于schema3，主区该布局未合管理；之后只能逐hunk合入，保留版本4/新迁移及主区views/ACP声明，并在实际合并快照跑管理31/原模型/布局回归及适用完整检查。外部3绿、旧schema3报告和本次静态通过均不能代签主区4的实际新组合。原管理001/002/003关闭归属各自revision，旧失败日志不覆盖。

本轮未执行测试、Web npm audit、供应链或真实Harness。完整审批/L5/L6、reconciliation及危险ready恢复未交付，未提交/推送。

## 同版独立执行后最终限定结论

2026-10-31追加：**schema4纯布局/迁移子阶段限定通过。** reviewer已实际读取独立tester的`schema4-acceptance.md`、`schema4-acceptance-source.json`、`schema4-owner-final.log`、`schema4-clippy-final-after-history.log`与实际owner包装脚本，并检查独立资产的pinned夹具、逐字段/DDL快照及WAL-only/ACL/锁断言。没有用开发11项代签。

新K11及原3意图适配13合计24通过、0失败/忽略、274过滤，实际20.71秒，独立报告exit0；lib/tests clippy `-D warnings`最终exit0。原自身lint失败与初次测试日志保留。reviewer对执行终态清单13文件再独立计算，全部相符，生产3源及锁仍为上表冻结值。新独立源`9c82a6e1964d0648d93ec0cdbc6397eacb915fe8fd0a0af9ee598a561078a56c`；原3适配源`501c45a597b8a423ab4936e4dc4271e508eeb8c8e79d8fa1a02ead3908e173a5`；入口`f13a23576f46c510e60644c7de56ba6fa865738f1d1ce0f64fc01aff6cb2069f`。完整合同执行SHA40b32014含放行状态说明，§3A实施边界仍沿已审fe621e；未混为整体owner获准。

实际证据包括：pinned真1/2/非空3→4及4重开，原3epoch/floor、决定/有无Run审批/五类delivery/24槽和旧全部定义/行/合成key哨兵保留，新增attempt空；24个precommit错误窗口完整回滚，3次真实deferred FK COMMIT错误787完整回滚，3次真实COMMIT成功后丢回执在故障pool未关闭时独立连接证明durable4；72组合NULL真值矩阵、PK/unique/复合FK/STRICT/安全时间与SQL仅length22边界；缺身份包含仅新attempt事实拒绝；真正已提交未checkpoint WAL的version77/marker由preflight及独立read-only看到，原DB/非空WAL字节不变，新缺失WAL仅0字节，ACL继承/目录锁实测。

原13只按新目标版本/表数和pinned真3负例作窄适配，没有删WAL、FK或缺身份义务；旧schema3源、R003原失败及原13通过报告保留原归属。raw SQL事务回滚只证明数据库，不代签未来应用attempt/投影/result编排。

此结论允许交付该布局子阶段并继续已授权的管理patch逐hunk集成；不关闭任务2.5整体、完整CL-004或审批owner编码门禁。管理revision3将改变Storage及模块等实际组合，须保留4布局/版本，另在合并快照运行管理31及布局/模型/适用完整检查，不能直接复用本快照或外部schema3结论冒称组合已经执行。
