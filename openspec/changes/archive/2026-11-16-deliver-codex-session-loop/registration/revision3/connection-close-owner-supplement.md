# REG-INTEG-SQL-001：专用连接关闭所有者门槛补充

2026-10-31。此文件是v3编码前工程补充，不修改R3设计/源v1/v2、根172组合源或旧失败证据；不改变actor、时效、受理/去重/队列事实。需原tester/reviewer同版门槛后实施。

证据：根实际owner全库478项475过、F02/F03/F15均在Area析构即时remove_dir_all得到Os32；独立双测F02+F03 0/2红且0.83s；独立纯SQLx0.8.6对照DetachedDrop 84/96失败、显式DetachedClose 0/96、PooledCloseOnDrop 0/96；取消对照detached 96/96仍独占占用、pooled 0/96，另连接确认未提交INSERT=0。原日志由tester保存。不是靠sleep/retry/串行测试得结论。

已锁sqlx-sqlite0.8.6 connection/mod.rs219–234 close发送Shutdown并await ack，worker.rs323–328先drop(conn/shared)再ack；Drop只signal。sqlx-core0.8.6 PoolConnection.detach即时脱离池计数，pool.close不再涵盖；close_on_drop虽然保pool guard，但take_and_close内5s timeout会释放guard，consuming PoolConnection.close future本身取消也释放floating guard。因此不用库5s或“join了调用任务”冒称DB worker关闭。保留当前detach调度语义，避免Storage max1 pool在SQL屏障期间把可信SQL authority/resolver/query阻塞；不引入新pool数量阈值。

## 窄类型与生命周期

仅新增私有 session_storage/registry_connection_owner.rs：

- `ConnectionOwner`为Arc共享，SqliteRequestRegistry.clone及Storage持有同一个；state短锁持`sealed:bool`、在途ConnectionLease计数及`Vec<Arc<CloseSlot>>`，另Notify；没有SQL正文/actor/队列策略。
- `ConnectionOwner::lease(&Arc<Self>)->Result<ConnectionLease,RegistryError>`是同步操作，在begin第一次await之前登记。sealed时NodeStopping；不允许seal之后新acquire。池acquire失败/取消、BEGIN失败都必须归还lease；返回已取得SqliteConnection后立即由`RegistryConnection{connection:Option<SqliteConnection>,lease}`拥有。
- `RegistryConnection::connection(&mut self)->Result<&mut SqliteConnection,RegistryError>`供现有SQL使用，无正常unwrap。`RegistryConnection::close(self)->Arc<CloseSlot>`同步取出connection，把consuming `SqliteConnection::close()` future交owner，再在同state锁内将lease转为tracked slot；无先减计数/后登记缝隙。
- RegistryConnection Drop（SQL取消/异常/提前返回）走同一个同步转交路径，绝不将未知事务放回pool，不在Drop直接detach丢关闭责任、不spawn未登记任务。
- CloseSlot持`tokio::sync::Mutex<Option<JoinHandle<CloseOutcome>>>`。owned任务不套5s或operation deadline、不由等待方abort；仅await真实Connection.close ack。closed/error均保存封闭CloseOutcome，不保存SQL原文。登记、任务句柄存入owner和lease减计数同短边界；连接错误不吞为成功。
- `CloseSlot::wait(&self)->CloseOutcome`借用槽中handle等待，只有真实join后take；等待取消不移走/丢弃handle。多个等待串行且读取同一已保存结果。完成槽只有取真实join结果后回收，不能常驻无限增长。
- `ConnectionOwner::shutdown(&self)->ConnectionCloseStatus`先短锁seal新lease，再等已有lease全部同步转交/归还，借槽实际join全部关闭任务。Storage.queue_close已有专用异步锁串行关闭，不持普通state/queue/actor锁跨await。shutdown等待被drop时保留sealed、slot及句柄，下一次Storage.close可继续drain。

这里的in-flight计数只负责本仓储取得和持有的连接，不是新增容量上限，不取代SQLx pool；acquire之前登记保证Storage关闭看不到一个尚未交回的registry连接。关闭任务仅做SQLite close，不运行产品SQL/原生writer。Core强持任务及其关闭责任直至结果取得；Storage drop仅请求取消/保留已经启动的关闭责任，不宣称实际join。

## 事务事实与关闭状态分离

finish先计算原COMMIT/ROLLBACK及既有Before/AfterCommit hook结果，再同步把conn交close槽，等待该槽（可随operation deadline取消）。关闭job自身独立存在。所有明确错误分支、BEGIN失败、outer deadline/cancel和caller-drop都由guard走同一归还。

COMMIT成功事实不因close错误改成rollback/新资格，也不导致D重做：transaction原Result保持原语义，关闭错误独立记录ConnectionCloseStatus::Unconfirmed。取消等待尚未拿到原结果时仍沿现有commit-unconfirmed/local probe恢复，不凭drop猜rollback。已apply的D只有重复E；未D不能在Stopping补做。关闭错误不写敏感日志、不塞入附加audit冒称必要状态正常；实际SQL错误仍按原仓储语义处理。

Storage.close保持原`async fn close(&self)->()`签名，顺序为原queue stop/join（其必要收敛SQL仍可用）→ConnectionOwner seal/drain/真实join→pool.close。追加只读`pub(crate) fn registry_close_status(&self)->ConnectionCloseStatus`，enum固定Pending/Complete/Unconfirmed；Complete要求所有lease归还、所有slot真实join且CloseOutcome都确认close ack。状态检查和最后完成条件使用同state围栏，不允许最后注册缝隙。

Node只在现有storage timeout调用处追加该typed结果检查：超出既有shutdown deadline报告未闭；完成但Unconfirmed/Pending报告静态“request registry SQLite connection closure could not be confirmed”。不改业务回执、不带SQL/正文、不把close异常当COMMIT回滚。不修改既有截止数值；Node期限只限制等待并报告失败，不abort关闭job或将SQLx5s当关闭成功。Storage对象仍在时，取消一次close等待后可重复close并继续join。未确认关闭不能发布ShutdownResult::Complete。

## 封闭测试接口与独立门槛

既有R3 fault/random hook签名和独立资产不改。新增仅cfg(test)的`SqliteRequestRegistry::connection_closer_for_test()->Arc<ConnectionCloseController>`；控制器在新模块test_hooks中，只能`pause_next(point:ClosePoint,barrier:Arc<CloseBarrier>)->Result<(),RegistryError>`，一次arm/一次命中；ClosePoint为BeforeClose与AfterCloseAck。CloseBarrier::new()->Arc、arrived().await、release()。若需关闭失败报告oracle，另加`fail_next_result_after_ack()->Result<(),RegistryError>`，实际SQLite close仍先完成，再模拟失去确认报告，不能冒称真IO错误或跳过真实关闭。没有生产开关或任意conn注入。

最小独立验证：

1. 原默认并发20F、原F02/F03紧红循环、原全库默认并发重跑；立即独占DB/删除只作用自有fixture，不sleep/retry或改变业务断言。
2. 正常COMMIT/ROLLBACK/BEGIN失败、Before/AfterCommit unknown路径，Storage.close后即刻Windows share_mode(0)打开自有DB；原必要事实/完整u64断言不变。
3. 实际BEGIN或SQL任务取消→rollback与close分别证明；close槽BeforeClose/AfterCloseAck已Pending时取消operation/第一次Storage.close，第二次close仍等同槽，不另关、不丢任务，释放屏障后才Complete。
4. 同时close/acquire/正常归还最后边界，sealed后新begin拒绝；在途已有连接必须被覆盖，不以计数0早报完成；slot真实join后回收。
5. COMMIT已成功+模拟close确认报告错误：原登记/Applying/Settled事实仍存在且不重新D；registry_close_status为Unconfirmed、Node最终失败静态可见。模拟类别与真实close IO证据分列。
6. R3已有真实SQL authority下Begin屏障期间reader/driver事件仍可达；旧39、R3P/F/M/C、factory/drop/并发关闭/槽回收/OWNER001和Raw30组合不退化。测试oracle由tester维护。

拟写范围：新私有module及开发tests、session_storage/mod.rs、request_registry.rs、storage.rs，必要node.rs关库结果一处typed检查/开发单测；不改旧schema/DDL、根独立测试或Raw实现。v3在原隔离workspace实施，v1/v2字节保留；root当前172组合源不并写。独立tester/reviewer门槛通过后再编码，交付用新候选及补丁按hunk合回保留root的v4 test-only hook和Raw基础层。
