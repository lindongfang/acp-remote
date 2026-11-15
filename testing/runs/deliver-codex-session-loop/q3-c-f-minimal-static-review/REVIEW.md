# C F 最小提案：静态方案门

结论：Pass-proposal-static。对固定 d4b58627… 的最小9路径、三入口和两条回归定义未发现新的稳定阻断。允许据修后固定 stage3 重基，随后送精确源码/补丁复核。本结论不允许直接安装旧 c01522bb… patch、不允许运行旧 runner、不签三入口通过。旧 runner 首行后的 throw 应继续保留；新重基包使用新身份。

本次逐成员实核 freeze 全1623成员，旧源795文件和提案798文件逐SHA核同；9路径符合 manifest，无额外叶。三条固定DDL与包内已冻结 SCHEMA9-v2 去末分号后逐字一致。只读检查没有编译、SQL、DPAPI或正式验收执行。输入/Root/作者/tester旧包均未写。

## 允许的最小机制

- catalog_sql/test_control.rs 的控制器按同一 fence 选择下一真实 kind；最多8 probe、每probe1024事件，溢出显式 Full。Weak 指向原 operation，不保存新 owner/receipt 权威，不提供 ready/reset/任意SQL。GateWaitPending 由原 lock future 的 Pending 产生，COMMIT 的首次 poll/Pending 与实际成功分开；Pending不能证明VM执行。
- acquire 之前的观察与 detach/attach 后的观察分开；原连接已同步装入 RAII 才能 AfterAcquireOwned。Drop 不增加异步任务，只沿原 handoff 存入原 receipt 后记录；正常 finish 才可等待 handoff 屏障。
- owner 的 AfterCloseAck 在真实 close 成功、actual 原子写入后到达。SuppressCloseReport 只抑制报告，保留原 task、actual、join 和 receipt。SuppressCommitReport 只在真实COMMIT成功后抑制报告并走原 Unknown；未添加知识强写。
- management 三点依次在已关闭短读之后、真实解密返回之后、原 ApplyWrite 已接管且最终 BEGIN 之前。没有为 mutation 提前结束原最终事务。恢复沿原 fence.recover，原 cleanup 确认后才允许 recovery read，不能新SELECT洗原Unknown。
- 私有 fixture 只在 test/windows 可见；通过原 registry detached_owned_connection 与原 close 安装固定三表，双头仍8/8。它不是 schema9生产迁移。三个Node测试正文使用真实配对/current actor、真实catalog_core和Windows DPAPI；panic捕获及 ArmedPoint Drop 放行后走原shutdown、原owner Weak和首次DB/WAL/SHM独占检查。该尾不等于整个目录首次删除已证。

## 三入口及回归的有限范围

1. F01：登记只保存、同runtime重建provider后应用、cipher/digest原字节移动、同ID单结果、Unavailable警告。不是冷重开、迁移、双port或全局审计。
2. F02：真实同gate Pending、B未进SQL、A完成后B短读原事实。B无ApplyWrite，不能计成两个最终写事务，不能冒充原012/完整S09。
3. F03：原ApplyWrite实际ACK后失报、原receipt可辨认、provider重建不解除封闭、原recover后新query。self比较只证明该真实槽receipt非缺失且自身一致，不能冒充foreign/missing/替换receipt负例或恢复前后身份完整矩阵。shutdown预期保留真实失报错误。
4. C-REG-REV001 定义保留完整合法settled正例、逐登记链/effect负例、合法admission/time不同于本次尝试；这些向量现在无mutation正文，执行仍0。将来必须给固定有限fixture变更及每向量真实到达条件，再核，不能把仍Registered或BEGIN持有时的损坏算已达。
5. C-REG-REV002 定义要求原gate Pending、同原deadline、seal驱动NodeStopping、A仍持gate、B未进SQL且自身lease已释放，并保留原recovery清理对照。定义合格，完整真实授权/双proof正文仍未提供。当前F没有原owner被动lease计数；如该真实A屏障正文无法用已有原owner完成证据隔离证明B自身lease释放，需要追加只读原owner lease计数观察，不能用GateWaitPending消失替代lease释放，更不能增加setter/新owner/新deadline。该观察只对后续REV002正式正文必要，不阻三入口方案门。

## stage3 重基必须保留

catalog_sql::enter 已变为 kind+原scope+原deadline。把 serial_wait 放进 stage3 现有 d.run(scope.while_open(...)) 的原 lock future位置；保留 GateWaitObservation、原scope取消及获取后原retained failure判断。不能将stage2旧 enter 整段覆回。management新 settled_fact 调用、完整 before 比较、reconcile额外generation/time核对必须原样保留。session_storage/mod.rs 只合cfg测试fixture声明并遵守stage3模块排序；node只加私有测试子模块。其他7路径按原before/after证据逐hunk重基。新包应展示 stage3→重基输出的最小差异、完整输入SHA和真实编译/首次执行结果；此后才给执行门。

完整原34+12仍0；VM/TEMP同连接、迁移故障、crypto Stop、完整mutation、真实双proof、无receipt分支仍缺。D/schema9生产安装、Root合入、WSS/完整产品和strict均未授权。本门不回写原34/12或旧冻结资产。
