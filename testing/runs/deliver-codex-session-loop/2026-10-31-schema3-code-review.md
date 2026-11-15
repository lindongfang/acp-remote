# schema3布局/迁移代码独立检视

2026-10-31，reviewer `/root/baseline_reviewer`，只读项目，未参与产品修改。范围限任务2.3的当前布局、识别、初始化与迁移，不签受理/CAS/容量/派发或新入口正文隔离。

初次结论：**发现SCHEMA-R002，修复及独立复验前不通过本阶段交付。** 数据布局及SCHEMA-R001设计修复已落到DDL，其余初读未发现新的产品决策问题。

## 初次受检版本

基线 `f6d1f8df63673eeebd6109d4ddb3e95e5b66b906` 加以下候选，实际摘要与交接一致：

| 文件 | SHA-256 |
| --- | --- |
| app/src/session_storage/product_schema.rs | 6243339225616835236c2348d357c432620c9d1d15195e40b06f6d73d84755ba |
| app/src/session_storage/schema.rs | 25572edb352abf088cf98f31b9066257fa8fa63d2788313c1aa7fd9d88828c46 |
| app/src/session_storage/mod.rs | 0420bafe7b38dbec78bbe76d55f958879a25a3f922260253fc6c506f1eba21be |
| app/src/storage.rs | e1d66e10d3889fde4cffbe3a57c6f620d8e0d290bcf822e1562b231571a04457 |
| app/src/error.rs | 3554d9ee29885f8db99e19566062a279dbecfa45d5b76d75b69e7006445c9be2 |
| app/src/node.rs | f11a4a1dc018e455f2297d0223841320c566b5a2988e5a28c1062d4e027db6b3 |
| app/src/session_storage/product_schema/tests.rs | c44810a327ad71d26d4148c1d4da6223d51270b884c6701019831945ceb1750c |
| app/src/session_storage/tests.rs | 33d93429ce518775e9e62ea3bae337116d0cd79d21fc81cd7bbf84bca98d2855 |

## SCHEMA-R002

- 严重性：P2，本迁移交付阻断。
- 位置：`app/src/session_storage/schema.rs`，validate中的`if current == 1 && versions.is_empty()`。
- 依据：合同§4A完整结构/身份识别与CL-004子断言C“有历史却缺身份metadata”须只读拒绝，不先打开WAL或写版本。
- 问题：仅schema1在node_meta为空时检查clients/management_commands/security_audit。schema2/3缺失node_meta行时不检查，其他结构正确且已有clients等事实的schema2会通过只读preflight，随后升级schema3并写版本；schema3同样被当作可正常使用。下游身份代码即使之后报错，也不能挽回已经发生的存储改写，亦不符合Storage::open本身的拒绝合同。
- 最小复现路径：从自有完整非空schema2 fixture删除node_meta唯一行，保留clients、旧管理及其余结构，再调用公开Storage::open。当前validate直接跳过身份缺失检查并进入迁移。
- 修复建议：所有已支持版本均识别缺metadata且存在身份依赖/管理事实的状态；不能只复用三张CORE表计数而漏掉旧SESSION或新product无actor事实。合法bootstrap的metadata尚未写入、其余表空，只有必要node_state及初始product_control(epoch,floor=0)的情况继续可打开。使用EXISTS/计数核验，不读取正文值，不删除、恢复或重建身份。
- 必需独立复验：schema2/3非空缺metadata均只读拒绝、数据库哈希/sidecar清单不变；包含旧SESSION事实、新product事实及已前进水位的代表性副本；新空库/bootstrap重开成功不重置epoch。
- 状态：open，已交coder及独立tester。此报告未执行数据库复现，由独立tester实际验证；该路径可由受检条件分支直接确定，不以开发通过结果否认。

## 其余核对

- 10张product表与2个部分唯一索引独立于旧正文；新表STRICT，整数安全上界/期限差/摘要/枚举、审计空与已发生三字段一致性均有约束。无Run审批load复合外键及可选Run复合外键符合SCHEMA-R001修复；同目标未释放资源和非终态Run唯一。
- CORE/SESSION定义保持原语义，新版本在同一BEGIN IMMEDIATE内创建兼容旧结构及新管理结构，生成control，依次改metadata/PRAGMA并完整验证后COMMIT；现有3路径不重置control。
- partial product对象在旧1/2识别时拒绝；完整3核对DDL与control行/规范epoch/水位。错误新增静态脱敏文案；node.rs只补穷尽错误匹配，没有管理/协议装配。
- cfg(test)故障入口生产不可见，PRAGMA前与提交前仅作为事务窗口错误，不冒充引擎语句失败；开发测试另有真实延迟外键引起的COMMIT失败。旧升级测试只更新目标版本常量，不删除原保留断言。

本reviewer未执行构建、SQL或产品测试。独立tester在自有副本的逐分支证据待收取；开发测试、clippy仅作输入不代签。新入口尚未接，运行时不读写旧正文继续not-run。

## SCHEMA-R002修复静态复核

schema.rs新SHA-256 `9a80c9e01cb67396eef4bf53eff7d7b3c0b4b1e86373c5b4da4da19dd1c33b2c`；开发product_schema/tests.rs新SHA-256 `185d392bbc8bc161e680c13971e35112a5c866282c31dbfb2868a1956714608d`，其余产品文件沿用初次清单。

已实际读取修复：所有已支持版本在metadata为空时检查CORE历史，schema2/3还通过固定DDL表名逐表EXISTS检查SESSION，schema3检查除control外的新表及control非零水位。合法空bootstrap仍可打开，未读取正文值或删除任何事实。静态代码缺口已补；独立tester已安排只含SESSION/product事实和floor>0的代表性用例及bootstrap重开，实际结果待原始证据读取。旧快照没有实际执行负例，不伪称已运行复现。

## SCHEMA-R003：只读拒绝仍产生WAL侧文件

- 严重性：P2，当前迁移子阶段交付阻断。
- 位置：schema.rs的preflight连接选项仅read_only(true)，以及真实WAL模式拒绝分支。
- 依据：冻结CL-004迁移定义子断言C明确要求未知/矛盾/残缺库的拒绝不新增WAL/SHM，数据库字节和文件清单保持；不能先打开写/WAL入口再修回去。
- 已收到父执行者转述独立tester首轮7/10，其中3个WAL模式负例在拒绝时新增WAL/SHM，主DB字节及拒绝本身通过；本段记录时尚待读取原始日志，不能把转述写成reviewer亲自执行。
- 判断：此现象不能仅因是SQLite默认行为而视作通过。SQLite官方说明只读WAL连接可以在可写目录创建侧文件，故read_only并不保证当前合同所需的零文件副作用。[SQLite WAL只读规则](https://www.sqlite.org/wal.html#read_only_databases)
- 修复边界：不得删除既有WAL/SHM、改成DELETE模式测试代签或忽略WAL。immutable跳过锁与变更检测，官方警告文件仍变时可能错读；只有真实无writer/固定快照条件成立才可考虑。必须用已提交但尚未checkpoint、重要schema/事实仍在WAL中的副本证明完整读取，不得只检查主DB。[SQLite immutable规则](https://www.sqlite.org/uri.html#uriimmutable)
- 状态：open。需具体修复、新源摘要，以及独立WAL不丢提交/拒绝无文件副作用的实际证据后原reviewer复查。

SCHEMA-R002不能因主DB拒绝已正确就掩盖SCHEMA-R003；两项分别跟踪。本阶段仍未通过，不能勾迁移完整交付。

## 独立首轮原始证据复核

已实际读取tester的`schema3-acceptance.md`、`schema3-acceptance-run-start.json`、`schema3-acceptance-attempt1.log`和`schema3-wal-inventory.log`。业务schema摘要为修复后的9a80c9...，其余业务文件与上表一致；lib另增加cfg(test)路径模块，测试资产是独立维护的真实SQLite验收。

首轮日志实际执行10项，7过3失败；三项失败的断言均精确显示原清单`identity.key,node.sqlite3`被新增`node.sqlite3-shm,node.sqlite3-wal`，exit101。独立保留的WAL清单主动单例也实际0/1、exit101，未被删掉或skip。之后DELETE对照及WAL主库字节有限检查的11项通过不替代该失败。SCHEMA-R003现有独立原始证据支持，保持open。

SCHEMA-R002：**Closed（代码及缺身份拒绝语义）**。新源已补全识别路径，独立实际结果证明非空缺metadata拒绝及主DB未改；DELETE稳定副本的缺metadata只读清单、仅SESSION/product事实、floor>0和合法bootstrap已执行通过。WAL侧文件不变的义务由SCHEMA-R003继续阻断，不因本项关闭被抹去。

独立证据也实际覆盖两种旧版本无损升级/重开、五事务检查点、真实引擎FK提交错误787及回滚、无Run跨load/跨Run FK与STRICT/审计空槽约束。它们是已验证子范围；当前阶段整体仍因SCHEMA-R003不通过。SQL受理/派发等原未验证范围不变。

随后实际读取`schema3-acceptance-final.log`：同schema9a80c9...的最终完整主动测试12项，11通过、1失败、0忽略，exit101；唯一失败为WAL文件清单，原SCHEMA-R003未被跳过。tester记录该测试文件摘要`c85d5a632b54efab0ab8fd4416f2ff72948be29c7a0f220384509c52996b584c`。当前最终结论仍为阶段受阻。

## SCHEMA-R003最终修订复核

2026-10-31，同一reviewer实际读取新合同、最终生产路径、独立`schema3-r003-acceptance.md`、`schema3-r003-source.json`及`schema3-r003-owner-retest.log`。工程修订依据见独立`schema3-r003-contract-correction-decision.md`，未新增用户产品决定，旧失败/原测试完整保留。

实际当前源摘要与独立清单匹配：schema.rs `9a80c9e01cb67396eef4bf53eff7d7b3c0b4b1e86373c5b4da4da19dd1c33b2c`；storage.rs `a91c4125ed82921ee0d68c97e36efba522e7fe60735f38afcc0e227124e79281`；node.rs `c97a0f51f3c878801bcc259452242fac9bd6f3764ec45da50443184fc38c0067`；合同`d805da41f7b89fe073fb7dcc5d35609a928d7097467504fc7eba1de06b968a8d`；独立验收源`2fb2e326088eb107bcd05ae6a949238c9a073803e64a2f6357886926a5d9dc91`。其余DDL/错误/锁文件按独立source清单固定。

代码已回退immutable及固定句柄候选，preflight继续标准read_only；生产node先取得DataDirectory并借用Storage::open_owned，目录guard保持在node所有者中。direct open明确caller独占/隔离副本契约。没有增加绕过节点身份或数据库权限的入口。

原始最终日志实际13项全部ok、0失败/忽略，exit0；输出明确新WAL恰为0字节，以及两次owner/private ACL和SHM继承PASS。仓库owner前置成功的执行与先前脚本策略拒绝的执行分列，没有持久修改机器策略。独立实际断言包含：重要version77及marker仅在未checkpoint提交WAL中，直接preflight和open_owned均拒绝UnknownSchema(77)，不是依赖后续RW校验兜底；独立只读查询看到marker；主DB及原非空WAL逐字不变；仅允许的SHM/空WAL，owner私有继承ACL；第二owner锁拒绝、guard释放后可重新取得。

SCHEMA-R003：**Closed（工程合同修正及同版独立复验）**。没有把旧12例或immutable漏读反证改成通过。新边界保持持久事实、身份/ACL及未知结构拒绝，允许的只有SQLite非权威辅助文件。原失败是旧过强文件清单合同下的真实失败，保留历史归属。

最终限定交付结论：**任务2.3的布局/识别/初始化/迁移及新明确预检边界检视通过；当前子范围无未关闭阻断。** 真实两旧版、COMMIT回滚、R001/R002和STRICT约束已同版重跑。完整任务2.3的新产品路径运行时不读写旧正文、CL-004其余受理/认领/容量/CAS/派发/审批及真实Harness/Web仍未验；不能据此勾整个产品任务或change完成。
