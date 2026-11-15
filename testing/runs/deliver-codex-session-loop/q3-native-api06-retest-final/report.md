# API06 independent SQL/PRECLOSE migration retest v18

结论：按原reviewer883d0356限定门槛，仅两固定测试补丁，在新独占固定API06副本真实编译并完整执行原5函数，5/5绿。两个SQL函数的普通等待者保留/drop四分支均到全部有效后续断言；三SQL预留/Bound函数首、重复、cleanup与物理清理均通过。Case2和Bound本次切片现可记实际通过，不签原Native80/产品完成。原NoOwner001保持Open，PRECLOSE001/002既有有限Closed不扩大。

## 输入和源

亲读并核原门槛883d0356f262f3491bf04daf1c17f6afa409485cbb584180b1f0300191949739。v17 freeze052214…全94成员+383源码共477项重新核实，新copy仅应用2469a085…与ec995808…，两个补丁均先check0再apply0；after恰为4ede0826…与5ba78611…，另外381文件原字节不变。实际API06生产仍为5442d931…/356源，Root322→205b补丁应用证据与135保护资产核验沿固定v17输入保留，没有混作者活copy/新provider/sourcePermission/新DDL。Root322实际在测试后再核无漂移。

本copy实际child build0及acceptance-feature lib --no-run0，locked/offline、copy cwd原1.98.1/87f27锁和专用tester target；未做安装、audit或网络，没有真实用户库/会话/凭据/真实Harness/Node/UI访问。

## 同源实际结果

同exe548992262e50ae1594639296022394a6558485f2d9ec58f2ff0de369e34a8207，child69881135a3e5b8a4c3d29c1c09f1406518a60d3e46c5b0d4ccf35bafe2c73449。

- SQL两函数2/2，实际Rust0，2.64s/569filtered。真实deferred INSERT成功、COMMIT787+ForeignKeyViolation及真实事务Busy5对照达到；Native原CommitUnconfirmed与cleanup分轴。AfterCloseAck在原repository.reserve使用的同RegistryConnection/CloseSlot实际到达，再控制注入报告丢失；不是由global Pending/后续新连接close代替原ack。Known COMMIT行在第二连接观察为已提交，实际原返回严格SqlCloseUnconfirmed{true}，普通/drop两分支原owner镜像结果均一致。所有后续原Arc/probe/body/D、无额外arm/RPC、释放原任务后同D唯一一次RPC、原注册计数均完整通过。
- 三SQL阶段3/3，实际Rust0，6.54s/568filtered。source/resource reserve真实ABORT的两个原分支仍无launch/adopt/take与假Job0，首/重复/cleanup joined+metadata与Storage.close全过。Bound一次屏障实际位于原Owned.shutdown callback，读取的是原同source首次StorageUnavailable；未把旧v17公开ensure的Stopped当原错误。屏障内原reader/write poll0、preparedSome、内存scope有而SQLscope/wire/initializeRPC空、原child/descendant markers与原body检查均到达。释放后才调用原WindowsOwnedSource.shutdown，真正actualJob0=1及原IoOwner AdoptedUnstartedJoined，source Closed/io_unstarted和resource Released，首次/重复/cleanup以及物理close全过。barrier到达没有被当作Joined。

每个SQL分支首cleanup实际joined=true/metadata=true/physical=true，真实787最终聚合closer Complete，受控报告丢失最终聚合Unconfirmed；不会把已确认COMMIT改成未提交。全部两个phase temporary_remaining=[]，当前无在飞编译/SQL/受控进程。受控report-loss是故障注入，实际COMMIT/物理ack/SQL/协议IO/Job与IOjoin是真实路径，不冒作真实OS close失败复现。

## 保留和限制

v17原5函数3绿2红、原Stopped/旧typed断言及未达范围全保持冻结；v11/v15更早失败不回写。门槛批准的类型与窗口迁移没有删除原prepared/poll0/body/单RPC/首关闭或物理后验断言；本次不再有那些未达断言的空白。

继承脚本日志scope字段仍含历史文字API05，Bound stderr仍用共享旧文案actual ensure result；准确版本由本输入/源manifest/API06 5442d和exe hash确定，Bound实际首次错误来源为受审补丁的原source.memory.error，不能把两处旧文案作接口/版本事实。原始日志不静默改写。

本轮只执行这五函数/七分支，未执行完整Native80、全部库/原120/其余共享/CF矩阵或strict全特性检查，不扩大早期限定Closed。新combined DDL资产按Root调度下一轮另版精确候选并经原reviewer预审；本轮没有读取或修改新DDL/旧正确预期，没有接入未冻结的实现。此处安全结束释放独立tester槽。
