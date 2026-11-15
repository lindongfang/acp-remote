# Schema6组合：独立真实SQL执行

结论：**63个不同函数的最终结果62通过、1个旧库开发预期失败**，不能报全组通过。新独立schema组10/10及旧独立schema5/3/4共41个函数全部通过，涵盖新3函数/70 SQL列约束向量、旧6拒绝前后双版本头、原14 ordinal/真实787及旧数据保护。库开发测试仍有latest5旧断言；原字节和首红保留，不判新生产缺陷、不自行改断言。正式Native80仍0，不关NoOwner。

## 精确来源与范围

亲读gate ae333eeecf2031dbf583c516b779913e4b56533bc8cc201ba469e044a179d335。新独占implementation-v19从固定API06 5442d931…及v18测试装配383源逐hash复制；生产侧唯一替换native_dispatch_schema.rs为批准完整fe155c0d82b5aa587dee8d5000f5dc5e959f00e95974c4db07a886e536a2e0ad，安装cefad1e37…有效资产补丁一次。这是**明确schema-only hybrid**，保留API06旧provider/control，不是作者新实现候选。386源中380原字节保持；只有生产schema1项及批准测试5路径改变。实际14条runtime DDL、批准SQL478262bb…与独立golden逐条按既有normalized规则全等，原迁移/检查入口、0..5全保。未读作者活copy或写Root/作者/旧冻结目录。

source-v19.json SHA 5468163aef4ee918aa17ecd0c2d8a7ffa8e1f40a7a209efe8793b612278badaa；同一实际lib exe SHA 7011ebc4447e0817c3667306bc909028336ef7916b58fdd94f192e4f792c9e75。child仅编译，本SQL矩阵未启动provider child。原a3a0的55成员、header补充23成员、v18的87附件和完整383源执行后复核仍相同。

## 实际执行

Windows、源码cwd工具链1.98.1、原87f27锁，locked/offline和专用target。同一PowerShell先dot-source原windows-owner.ps1，再按已授权本机提权运行自有副本/临时库。child build、lib test --no-run实际均0，未运行strict fmt/clippy。清单574个函数中选择本范围63项，未选511项不算通过；13组实际函数名称/计数逐一核对。最终不同函数62绿1红，累计67次函数调用含四个原长TEMP首红，累计调用62绿5红，0忽略；不能把重试次数冒新用例数。

- 新/原Native独立schema10/10；旧独立schema5为7/7、schema3为13/13、schema4为11/11。原断言和旧fixture保持，latest版本既有SM迁移与单独step5仍验5的责任分开。
- 70向量为source30/resource12/control28，由固定编译JSON与两实际函数确定性全组循环覆盖；精确检查SQLite275/787/1299。它们是3个Rust函数中的SQL子case，不是70个Rust测试，也不是私有物理proof。
- 原14 ordinal逐0..13实际执行对应DDL后触发checkpoint，回滚后第二连接仍5/native对象0/whole旧snapshot保全，再正常迁移验6；ordinal14没有虚构故障并正常成功。原真实DeferredCommit精确787/ForeignKeyViolation/消息/回滚保全pin5实际通过。旧6布局拒绝的前后两个独立连接均读(6,6)，全snapshot/key不变。
- 原开发schema回归Native2、product/delivery11、legacy4项通过；Storage metadata2及原same-pool物理关闭2通过。真实max1、observer clone、late-return屏障、首次/重复Storage.close和DB/WAL/SHM Windows FileShare.None原断言全部到达；没有把源码静读当物理结果。

## 保留的失败与到达范围

**NDT-SCHEMA-DEV-001（Open-test-maintenance）**：库内registry_schema5_addition_is_atomic_at_every_new_migration_checkpoint真实退出101，tests.rs125 left6/right5。原第一RegistrationTable故障回滚、preserved4和注册对象0已到；正常Storage::open/close到最新6后，原preserved(c,5)仍要求旧latest5。该函数后续字段及余7故障格未到，不能以其它绿替它签过。本轮不改它，也不据旧预期不适用报生产bug；作者已收精确定位。

原legacy4函数首次在很长的自有phase TEMP路径打开fixture即SQLite14，全部raw stderr/actual101保留。只把本次TEMP/TMP phase缩为legacy01..04，保持同exe/source/filter/原assert/owner条件，4函数均完整通过。该对照限定为执行路径条件，未修改生产或测试预期，不把原环境红抹成无失败。

库存脚本首试错误使用session_storage::tests命名空间，实际模块为legacy::tests；在库存阶段拒绝0匹配，未冒0例通过。原脚本/list保留，另版修过滤路径后确认63入口；一次生成脚本shell引号解析失败也明确记在inventory-first-attempt.json，未跑Rust/SQL。未执行的green-only finalizer草稿保留并注释废止，实际本报告由finalize-schema-execution-02.mjs按真实红绿生成。

## 资源、未执行和交接

每次TEMP/TMP无剩余，workspace自有.tmp递归无剩余文件。所有已启动Rust进程退出，原close/pool/Area清理及物理独占断言执行完成；没有额外删除或用户数据库/系统ACL操作。本轮保留源码、原始日志和空自有目录供复核，到此安全结束释放槽。

完整asset patch继承原两条SQL whitespace warning，stderr保留；不宣称fmt/clippy通过。未重跑v18 Native Case2，不扩大其既有有限关闭结论。SQL直接列值只验证schema，不能铸NeverCreated/SuspendedRootExited物理证明、Job0、Scope、Loaded、Received或Permission grant。provider12、SourcePermission12、实际精确原资源集合/no-native-effects8-query仓储和错误拒闭、延后Tokio/Mio关闭算法仍等待完整固定合法候选；不能据本混合SQL绿关闭NoOwner或Native80。原80 ID/标题/级别/预期保持，formal80-status.json逐ID仍not-run-final-candidate。
