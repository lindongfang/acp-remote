# ACP reader/lifecycle窄范围开发过程检视

2026-10-31。只读实际workspace；**不是冻结交付PASS。** 发现并登记下列三个问题，已向root/coder即时报告，作者期间作出修复，最后读取的修复逻辑静态闭合；三个ID仍Open待最终冻结同源独立动态及静态复核。没有把未编译、未填测试或开发lint错误当产品缺陷。

## 读取前后版本

首读transport/lifecycle/reader/state分别为：

- `1bd0e6fe0f0f9a07fe100348c30bb098debfd3e6e86585f397c8f88b8a34ff5d`
- `51af511ab20ed75161f96b7275d13f59e5fba49d0645349e15995ae5de9b820b`
- `1de74057cd9b6ed3341df9c68871d0d25230c3791dc6e52adfaad1c3988f2950`
- `6f83c5bfa211053715acaa71ac5501d09f87eb9cf23bcc65bb83b427cc0b2c17`

检视期间实际变化，不能称整轮同源。最后一轮四文件前后hash一致，分别为：

- transport：`370d4069967c73ce4c88d963866f91550e7b4aafe60cdb14d1c4ca29e7c3a9ac`
- lifecycle：`cd74e8dd385de60eca282e817c4e2a9261fd5b9b3c366c6560022b693b1b1a0f`
- reader：`bb2ae23ad1423b7bf838162c3bf417397f10ba55695aef49555cbbeefb280579`
- state：`9a216577f8f0d7b20b1107278db68d7f62bdd23e321dece9aa5b286135fc33e7`

一次读取前后暂时一致不等于已冻结；最终候选仍须重新核完整源清单。问题旧动态均未执行，没有制造旧红日志；实际读到的旧逻辑及后续修复分开记录。

## ACP-IMPL-READER-001：expire进入Stop却未唤醒writer

首读reader在循环顶端/实际read闭包执行State::expire，若stopped则直接break或Ok(None)。原State::stop只更新状态/终结cell，没有通知Core.notify。writer空队列等待n.await，或真实W Pending；collector又等writer退出。因此在timer分支尚未Ready、进入read闭包时At已到的边界，reader自己expire→Stop后退出并丢timer，writer可能永远得不到唤醒，terminal不能发布；不能依赖调用者再显式shutdown救活。Written后等response的场景尤为明确，原cell已完成，不存在仍会唤醒writer的写进度。

最小修复是让所有State停止/完成转移统一唤醒协议任务，而非只补timer分支。后读State持有与Core相同Arc<Notify>，stop和finish都notify_waiters，覆盖直接expire退出及其它调用者路径，静态闭合。

后验：真实owner timer在无consumer next/wait、R Pending且writer空闲/真实Pending时到期，自主发布terminal并使两IO退出，不能先外部shutdown才显现；正常Zero期限及Written/Unknown双轴仍按合同。不加生产yield强造计时短缝；该具体短缝以锁序/统一wake证明和合法时序回归分列。

## ACP-IMPL-READER-002：真实read错误封口在锁外

首读R.poll_read Err/EOF发生于ProtocolState短锁内，却只返回local Err；释放State后外层才Core.stop。两者之间新writer实际poll或reserve/enqueue仍可能按live进入。已观察到不可继续读的错误与封口没有同一状态边界。

后读在原实际read poll的State锁内先记对应Io/DeliveryUnknown/InvalidMessage Stop，再返回；外层仍负责唤醒/结束。与writer真实poll共享同一锁，已封口后不会穿入新poll，且Stop之前取得的完整LF仍走原batch排空。静态闭合。

后验：真实Err/EOF与新写/关闭竞争、Stop后无新read/write/flush，已取多LF排空、首错误不改写。明确实际read取得，不拿peer buffer写入冒充。不能将之后成功的新版回归写成旧版动态复现。

## ACP-IMPL-LIFECYCLE-001：collector失败被等同两IO已join

旧shutdown使用timeout(async{await collector；JoinErr仅core.stop(Io)}).is_ok()作为joined。collector await返回Err后async仍正常结束，尚在Core.io_tasks中的两IO handle未必已实际join，却可joined:true。公开来源是原runtime A被关闭/取消collector，再在runtime B对保留的IoOwner.shutdown；collector取消不是两IO已退出的证明。shutdown_background时另一worker上的真实IO future析构还可能未完成。

最小修复不能只在首轮返回false后把collector槽清空，让下次None分支假true；必须保留Core任务槽，借用await逐个真实join，覆盖同一个deadline及取消安全，并在确认drain后承担terminal发布责任。

最后亲读lifecycle：正常collector与shutdown fallback复用私有drain_io；shutdown等待collector，即使JoinErr也继续逐slot真实join，collectorNone的重复关闭仍drain；handle只在await完成后take，IO记录只在全部完成后clear；随后publish_terminal。普通并发关闭由专用shutdown锁串行，等待future取消仅释放借用锁，句柄仍在Core。此逻辑静态闭合，不扩public API。

独立tester提出的确定性公开回归限定可行：runtime A真实spawn/握手后shutdown_background，自有W仅在Drop用Condvar门闩暂缓析构（poll仍非阻塞、无后台写/Drop flush），确认真实析构已到且Weak仍活；runtime B在未释门时短deadline必须joined:false，借用等待取消可续；释门后最终真join并且R/W Weak消失。测试失败也须RAII先释门，不持State/App锁跨此阻塞Drop。它证明自有AsyncWrite析构/原runtime取消边界，不证明真实Harness或OS worker/Job树。尚未编译执行，不算通过。

## 本次未发现新问题的具体部分

- 启动started屏障在两IO slot和collector slot登记后才打开；即使随后立即Stop，started判断优先已启动状态，collector仍负责join，未看到“第二handle尚未登记即清空”的正常路径。
- 普通shutdown串行锁及Slot.handle借用await没有take后等待；取消和并发等待不丢JoinHandle。修复后的异常fallback也沿此所有权。
- IoOwner/EventReceiver Drop只同步Stop，监督任务已在spawn中由捕获runtime Handle建立，Drop没有ambient spawn。无runtime线程Drop本身未见新增依赖ambient context。
- reader按已取得LF批次逐帧parse/route，Stop不abort持帧reader；collector在IO结束后设置terminal，EventReceiver先pop队列、terminal Err只在本体ended切换时一次交付，之后None。重复publish仅重设同bool/唤醒，未见重复错误队列插入。

这些是窄静态检视结论，不是整体支持声明。writer既有ACP-IMPL-WRITER-001及本次三个ID仍须最终固定版本独立实测闭环；本报告不改合同、源码、产品规则、API或已冻结历史证据。
