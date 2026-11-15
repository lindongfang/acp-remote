# R1/R2/R3 最终限定交付复核

2026-10-31。**PASS：固定candidate-v2产品与portable-v4验收资产的R1/R2/R3子范围通过独立复核，可窄合入根工作区。** 已包含真实SQLite和实际QueueService/InputQueue owner动态证据，不是纯CAS模型或仅编译结论。根实际集成/组合工程检查仍须执行，本报告不代签尚未执行的root检查或完整WP2/WP3。

## 同源与资产

- 产品136项清单 `5eeac76995a33c7e2ed410d3100b117052a405e67dcfd32b8052c15637163ba3` 已亲核。正式执行后，tester pristine136项当前仍全部同hash；before/after无差异。v1保留，v2仅service的身份绑定修复，不冒配v1失败或开发证据为v2结果。
- 正式独立报告 `acceptance.md` SHA256 `408e901b96bfc11327a8b68d4cd2416fbd76b74ed489417e46078e6e3dcc44ef`；HANDOFF `6f33c007bec1dc72b2058174b2dbedf2a55ab4d81c4761a657ddd00d0ec8022e`；delivery-manifest `9aabfb87677349325cb8990066d70d2462786bac38ab307b571e0900bf386b26`，其10项文档均实际匹配。
- 仅取portable-v4/package：28/28资产匹配 `401787d135e8d775ddccc11afa02a6164bdc0cd3bbec0a7d05ec2ffa85fd9dbe`。挂载patch `692d41c962ef422284a4c6e8f10b3db9f7671e185b18ce250d3fb5cb33188d17` 同已审版本；portable实际136原路径只有五个已审config/cfg(test)挂载变动，其他131同hash。Cargo.lock原87f27b61不变，无新增外部包；测试probe仅增加本地package。
- 相对已审v2资产，仅F追加真实owner的F20、P在既有两例追加release/旧token及Completed/poll触达；完整diff无旧断言删除或放宽。C root-ready和management两处终点版本适配、原deadline helper及原39保持此前审定字节，旧资产/报告留存。

## 实际验收与定义映射

亲读原始日志、正式资产、49行固定ID映射及事件/扩展补充。原49主定义与报告49行集合完全相同，无缺失/新增规则。executed-tests.json SHA256 `81a7c0f6ceb9f5c9857aa8057fb5e898503b9d39a1cc0b17aff639239fe4110e` 的154条均唯一，对应实际日志中的完整函数，无重复计数：

| 层 | 不同Rust测试函数实际通过 |
| --- | --- |
| 公开层 | 102＝P51＋OWNER2＋公开factory1＋shutdown1＋restart1＋QEXT7＋原39 |
| 封闭真实SQL | 21＝registry20（含新增F20）＋真实factory1 |
| schema5迁移 | 7 |
| 旧schema3/4保真 | 13＋11 |
| 合计 | 154 |

这是各同源批次及增强后定向重跑组成的已通过集合，不伪称最后一条命令同时跑完154项；循环分支、alias和重跑不加总。P51、原39、OWNER2、F原批次、F09强化/F20、M7/C24等原日志已核。真实F20在Edit D已到v2后分别于SettleUpdate和BeforeCommit实际回滚，Processing applied=true保全文，随后只补同E、不改成v3。公开新增release/poll分支各定向实际通过。

真实硬退出5窗口均child exit93；同字节portable restart显式feature再跑1/1。BA/SA/AU关闭的并发/取消共6分支，原日志全部first_poll_pending=true；joined=false始终核claim拒绝，joined=true及最终Storage.close均核Future内Weak消失、真实SQL与无晚调用。SQL前wrapper故障、真实COMMIT前回滚、真实COMMIT后未知及硬退出严格分列，未把内存与SQL称为一笔原子事务。

## reviewer问题逐项关闭

以下仅针对此产品SHA与本次子范围；关闭由reviewer结合源码与独立证据决定，不沿用coder自评。

| 问题 | 决定与依据 |
| --- | --- |
| REG-IMPL-START-001 | **关闭**。Storage在join完成后按Arc同一性清supervisor；公开factory与真实Before/AfterCommit startup取消等待方后，第二factory拒绝、close不靠释放门闩即可收敛、Weak不可升级及无晚SQL。 |
| REG-IMPL-SHUT-001 | **关闭**。专用关闭互斥串行join；真实BA/SA/AU并发关闭不因另一路持有句柄而假joined；未join不放claim。 |
| REG-IMPL-SHUT-002 | **关闭**。Core槽持续持JoinHandle、关闭只借用等待；首poll Pending后drop同一关闭Future，Storage仍实际取消/join，Weak及SQL/无晚调用通过。 |
| REG-IMPL-SHUT-003 | **关闭**。Stopping未applied分支不mint新D，真实BA Stop后仍Applying，下一runtime Interrupted/空queue；D已发生反向只补E。 |
| REG-IMPL-SHUT-004 | **关闭**。入口、cancel、最终Stopped/claim同注册围栏；真实旧service并发32次关闭/16次factory尝试不释放新runtime claim。无await短缝隙由同源静态不可交错性证明，动态仅签合法代际回归。 |
| REG-IMPL-OWNER-001 | **关闭**。最终tasks/state锁内按请求id/target绑定probe/worker/ticket；旧A真实inspect await跨B建立及16轮×8恢复回归不读/接管B，B自身可恢复一次。锁间无await短缝隙不冒称动态确定性命中。 |

原CL075 Err及shutdown spawned Pending断言的测试资产问题已按冻结定义纠偏，原失败资产/日志/中间patch保存；最终P为c5127afd…，shutdown为ee294643…，没有改产品迎合错误断言。getter/序列MAX不可公开到达，保持开发/静态证据，不列公开动态通过。

## 构建边界及根集成条件

实际portable-v4 fmt与显式fixture feature all-target严格Clippy均0，默认build0。亲读default compiler-artifact JSON，产品lib/bin的features=[]，没有registration_crash_probe；无feature显式请求child实际以required-features拒绝101。未用旧target残留exe判断产物。空feature仅门控测试child和restart wrapper，未公开私有fault hook/lease或修改运行逻辑。

可依固定产品patch纳入28资产并应用五处挂载。根须核最终清单、原39及锁文件，实际owner环境执行适用fmt/严格Clippy/check/build与回归；fixture重启必须显式feature，默认0测试不算通过。旧management版本候选本次只编译，旧矩阵由root实际集成检查另做。raw基础层后合，并对真实组合源另复验，不能把两份隔离报告当作组合已通过。

本次证明的是节点登记、schema5真实必要账本、唯一实际queue owner及其受信端口下的调度/失败/关闭责任。**不签真实WSS鉴权、原生资源资格来源、ACP writer/首写/插入/终态关联、真实Harness、生产清理、Windows真实进程树或整个WP2/WP3。** 抽象Blocked/合成Ready不冒充真实wire证据；Web audit依用户要求未外发。未提交、推送或修改根产品。
