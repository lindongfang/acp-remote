# C 扩展 F 十八路径：独立有限执行门

结论：Pass-limited-execution-gate。对固定 stage3 的扩展十八路径、十七入口及新增有限观察未发现阻断独立编译/运行的稳定语义问题。允许独立 tester 在新 execution-source 安装下面精确扩展，先实际类型检查，按真实 --list 逐入口运行并记录所有红/未达。不是编译成功、测试通过或正式34+12全部验收通过。最小三入口原门独立有效，不必等本扩展通过才能运行。

输入 configuration-independent-assets-v2 freeze SHA256 55f4ebb4a4fc6be8d91c3c6b623e5a37646165892aa7ae1fe2bdbe178ae70ac7 /2431成员，本reviewer已逐长度与SHA核同。基线仍为 candidate-stage3 freeze 2965dd57d361802afbd9c18bc3bb90fec1099755527c098eac19ba4bf69b1853，795源manifest cad55b913fbb59683a798a29627ebd2d51e1f028dc8123c8692730d43afc913f，不能换成Root789、Native+C803组合或四叶style维护候选。

## 精确安装路线（二选一）

- 从上述795源重建：只安装 EXTENDED-F-PROPOSAL.patch，SHA256 94f4e3a97a4b6284f0d2968366a994f45e5ab63a784b0545735ce06ebe31aa1e；18路径=7改11新，最终806源。
- 从本reviewer已准最小F精确798源重建：只安装 ADDITIONAL-F-ONLY.patch，SHA256 c053dc33743ccdf0aa08f4b89257ccc00f0e85364c762a3e8842f9779833686f；15路径=7改8新，最终同806源。

不得在已装最小F的源上叠完整patch，也不得把两扩展patch叠加。18路径before/after及15增量before/after均逐SHA核同manifest，stage3其余788源逐SHA相同，recovery_guard完整函数块逐字保持。完整与增量正反向 git apply --check --whitespace=error-all 共四项均exit0。原34条steps/expected逐字符串SHA与旧独立适用清单一致；12条operative定义逐字段相同。无apply、cargo、SQL、DPAPI、VM或验收实跑由本reviewer执行。

## 新钩子边界检视

原 GateWaitObservation / d.run(scope.while_open(...)) / retained unknown / 完整settled链不动；额外catalog改动只有cfg(test)下在原已拥有CatalogConnection、BeforeBegin之后安装有限脚本。默认无arm不做SQL。SqlScript是固定枚举，不能输入任意SQL；role绑定原槽。TEMP/table/view/trigger和main负例都仅在自有DB、实际下一业务BEGIN使用的同一连接上安装；不是另开pool伪造同连接。

VmBeforeBegin用原连接固定递归CTE及SQLx安全progress handler，是BEGIN前合成VM子轴，不可计原业务BEGIN内部VM取消。ActualCommitCallback用同连接SQLx安全commit hook，回调不能被计为COMMIT成功或持久化；原AfterCommitActual、原报告知识和原实际关闭ACK仍各自独立。已查本机锁定SQLx0.8.6原接口文档/实现：commit/progress回调true继续，false拒绝；私有Condvar最多10秒后false，VmGate Drop放行，无新SQL后台任务。每次callback到达不提供业务receipt。最终执行必须记录callback是否实际到达、是否超时，不能把回调触发数当成功提交数。

原owner新增passive_catalog_observation仅锁读leases/slots/sealed，无setter、reset、新owner或新deadline。可用于C-REG-REV002明确核B释放自己的lease；本次正文实际只测apply waiter，不能冒全部register/query/resolve/resolve_read五入口。其它四轴及原recovery-after-seal正式对照仍缺。

Probe有限观察仍最多8probe/每probe1024事件；原Point arm只能消费并释放后再替换，另有一个有限SQL selector槽。保持Weak指向原CatalogOperation；新增比较只返回Missing/SameOriginalSlot/DifferentSlotSameOwner/ForeignOwner，不泄漏receipt或提供知识强写。

S09 fixture仅Install意图，通过原registry.detached_owned_connection、实际BEGIN IMMEDIATE、原close收据完成受控变更；不穿越catalog gate冒第二合法apply。完整结算像用真实DPAPI解原cipher并调用生产确定性effect writer，负例固定改profile/result/登记字段，settled.fact与FK再次检查。三观察点均在最终业务BEGIN前，未移动hook提前提交。非法NULL/admission/result_at若被CHECK先拒，记录ConstraintRejected，不能计最终snapshot核验已达。

同步SecretProtector wrapper在真实DPAPI delegate前/后暂停，不替代OS密码学，也不声称OS调用可取消。原operation lease和真实Node shutdown Pending负责证明同步段仍归原owner；测试自有任务只在失败清理时abort，先放行gate并实际join，不改生产任务。timeout标志必须为false才计该轴有效。

## 十七入口能证明的有限范围

| 入口 | 本次可执行范围 / 不可外推边界 |
| --- | --- |
| F01-F03 | 沿原最小门；同cipher/重复单结果、同gate后短读、原ACK后报告丢失及原cleanup。 |
| F04 | 三点各17个受控向量=51，各独立Node/DB；完整Install结算像及负例。未覆盖全部绑定effect、动作矩阵。ConstraintRejected与Applied分列。 |
| C-REG-REV002 | A仍暂停时seal使B apply waiter以NodeStopping结束，原leases恰减少1且无BeforeAcquire；不是五入口都已测。 |
| F05 | 八个固定TEMP/main namespace子轴在同连接安装，实际封闭错误；不是schema9迁移验收。 |
| F06 | BEGIN前合成VM与原COMMIT安全回调两子轴callerDrop、原责任和原reconcile；不签BEGIN内部VM或回调即提交。 |
| F07 | 原Storage.start_queue_service、真实配对/current authority产生accepted/read两proof；同core双port比较及revoke后已接受owner仍有效。自有workspace/target夹具不构造proof；本向量无recipes，不能签完整recipe解析/真实Harness。 |
| F08 | protect/unprotect前后四个真实DPAPI wrapper点，原lease与真实shutdown Pending；不是OS crypto Stop。 |
| F09 | 原owner实际shutdown/Weak/首次物理尾后，registered ID正常冷重开与真实DPAPI；双头仍8，非schema9生产升级。 |
| F10 | 九个callerDrop点和自然BeforeAcquire无receipt分支，原commit/cleanup与显式recovery；不是全部timeout/error矩阵。 |
| F11 | 原真实COMMIT成功后丢报告，保持Unknown和原gate；原清理及单次RecoveryRead验证metadata，不重做apply/profile。 |
| F12 | RecoveryRead自身COMMIT报告、close报告未知和候选read Drop三轴，不覆盖原业务Unknown；每次显式恢复至多一读，不能拿新query清原Unknown。 |
| F13 | 原ApplyWrite close暂停，实际同owner无关槽及真实foreign owner另槽完成COMMIT/close；只返回身份关系，原cleanup不变，最后仍经原receipt恢复。 |
| F14 | 两个不同publication ID真实串行竞争，A committed/B rejected且两者均达到原ApplyWrite BEGIN/COMMIT；不同于F02第二调用仅短读，不合并统计。 |
| F15 | ProfileKey全u64纯值、Next最大值拒绝、有限静态错误格式；canary不在字面枚举中不等于真实敏感失败矩阵或不可伪造性完整证明。 |
| F16 | 双wire配方raw数字片段/未知MCP/Windows UTF16 code units等纯codec子轴；不是协议协商或真实Harness。 |

contracts为外部普通consumer编译资产，允许在独立新目录建立同候选消费者构建。正例先同库构建成功，负例仅命中指定private actor/module/receipt/raw profile构造诊断才计有效；缺crate/依赖/链接、模块当值或无关类型错误不能算隐私证明。没有默认factory装配证明。

## 实际执行与后验要求

新执行包先固定806全源清单，记录安装路径及patch身份；--locked --offline，独占自有target/data。原proposal runner的throw保留，在新包生成绑定本门的runner。先cargo --no-run和实际--list，确认配置模块含原3+新增14共17条，嵌套模块使用实际全名逐 --exact，禁止零匹配算通过。失败保留首次退出、原始日志、source/lock前后、每向量实际到达/Applied/ConstraintRejected/未达，不能删红重跑后覆盖。类型错误需新修正版或精确维护增量再核，不靠更改产品语义/独立预期让测试转绿。

每个有Node的入口保持原shutdown/join、原owner Weak、首次DB/WAL/SHM独占观察；已有尾不等整个目录首次删除。已知CLEANUP001机制与内部worker收据设计缺口不因该测试的pool.size0、owner计数0或future已join而关闭，物理红必须保留。

本门只授独立execution-source中的观察/夹具安装、编译和十七入口实际运行；代码存在性和本静态门均非能力证明。完整34+12父项保持未验，生产schema9迁移/双头9、D默认装配、Root安装C/F、strict、真实Harness/WSS整体及完整产品仍未授权。测试style四叶维护未混入。原最小三入口可继续独立完成，不受扩展结果牵连。