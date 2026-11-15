# ACP-IMPL-WRITER-001：真实poll结果与Stop的终态窗口

2026-10-31。**开发过程问题成立；当前修复逻辑静态闭合，仍Open待冻结同源独立动态/最终静态复核。** 不把正在修改的workspace当正式候选，不造不存在的旧源码快照或红日志。

## 证据范围

Root在真实工具输出中读到旧Proxy仅保存local outcome、in_poll=false，随后锁外调用failure；coder确认修复前没有独立保存文件/preimage SHA，也没有该旧版实际失败测试日志。reviewer第一次读取时已是修复后：writer.rs SHA256 `6699f6624109deaf3e593bcebef00b346983556dc8bcf0fd1812e46b6a3c8ebb`；state.rs `6f83c5bfa211053715acaa71ac5501d09f87eb9cf23bcc65bb83b427cc0b2c17`；lifecycle.rs `51af511ab20ed75161f96b7275d13f59e5fba49d0645349e15995ae5de9b820b`。均是外部implementation/workspace的未冻结过程字节。

亲读现有State::stop、finish_zero、finish及writer::failure可证旧窗口的结果：stop对Writing accepted=0且!in_poll按已知零终结；failure遇Finished直接保留首终态。因此若真实poll_write已Err而仅local outcome尚未落State，Stop先取锁就会产生假Zero，后续failure不能纠正。accepted=0不足以证明IO Err没有副作用。对应flush成功若只留local结果，Stop也会把已经成功flush的已知Written事实先改成Unknown。前者违反64cc第4/6节IO不明不能假零，后者丢失真实传输完成事实。

## 当前最小修复复核

当前Proxy::poll在真实W.poll返回、原ProtocolState锁仍持有时：

- Err（含返回0/不合法长度所映射IO错误）先spent、finish Unknown{accepted,cause:Io}并Stop，再对外返回；不会留下可被当Writing0零撤销的中间状态。
- 真实flush成功同锁finish Written；之后Stop/failure不覆盖该已知事实。
- 成功短写/完整含LF接受原有同锁accepted/FrameAccepted更新继续，Pending0仍保持已知零可取消。spent保证原server reply在不明或已有接受后不能恢复Waiting。

不必扩public API。替代实现也可以同锁记不可丢失IO事实、再由Stop/失败路径可靠消费，但不能只在锁外增加一个布尔检查。修复须覆盖write/flush、首终态不覆盖、server单次回复及原FrameAccepted响应关联。

非法fence的后置检测仍须封连接并表达InvalidFence（若已有更早真实IO终态，按首因保留）。若真实flush已经完成，保Written仅是传输事实，不证明fence合法、授权或业务成功；独立测试必须同时核错误/实际poll次数/无重复字节，不能仅看Ticket Written就判整个操作成功。

## 后续同源覆盖门槛

1. 真实poll_write Err且accepted=0、已有partial、真实flush Err及flush成功分别与Stop/关闭竞争；Err不可假Zero，已知flush成功不被降成Unknown，accepted范围正确，无后续残帧/重复poll。
2. 一条不加生产yield/hook的确定性公开路径可用：合法fence恰调用一次io.poll；获得真实Err0或Flushed后，在返回Ok之前同步drop其私有持有的EventReceiver，触发真实Core::stop。外部仍持IoOwner，可后续shutdown实际join。该同步Drop非阻塞、不伪造scope，恰位于旧Proxy返回至外层failure/完成记录之间。是否采用由独立tester决定；本建议未编码/未执行，不能写成已复现旧红。
3. 另保真实并发shutdown/cancel回归、无ambient runtime Drop、实际join/Weak回收；合法Pending0取消应仍Zero，不把所有零都改Unknown。
4. Illegal fence的零/多次poll、Err前已poll、实际IO已有结果的组合分别验证；第二次代理调用不得再次碰W。首个真实终态与协议终止错误分别报告，原Received早响应不撤销。

当前源码随开发还可变化，最终只能以冻结清单与独立资产/日志同版关闭本ID。旧动态未执行的事实永久分列，不用新绿冒充旧红或将本静态报告当功能通过。
