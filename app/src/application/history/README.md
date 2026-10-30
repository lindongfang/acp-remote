# 历史保存开发接口

实现依据是 [session-contract §5–6](../../../../openspec/changes/deliver-codex-session-loop/session-contract.md)。本文说明开发接口，不替代行为规格。当前只有应用、领域和真实SQLite模块；OutputControl测试端口不能证明真实Harness已停。

## 两种计量口径

保存触发累计每次受理的结构化完整快照原始payload字节，不含管理信封、base64、网络字节或固定元数据开销。同条目替换会累加新快照payload字节；淘汰旧快照不减少本轮累计量。500ms和64KiB先到者触发，未改变阈值。

容量按协调器保留的不同(item,version) payload，加每版本完整kind/protocol/protocol-version UTF-8字节及1024字节保守容器/身份开销，再加每scope的1024字节开销。共享Arc的同版本正文只计一次；inflight旧版与新dirty版本同时保留时分别计量。空条目也收费；SourceId固定128bit，元数据有非空/长度检查，原生大ID和未知字段在payload内完整保留并按全部字节计量。SQLite自身页缓存/编码工作缓冲不属脏缓存计量，由单写者和单对象上限另外约束。

节点所有scope共享同一短临界区原子检查/保留预算，按会话聚合16MiB、节点128MiB；高水位是整数ceil(4*limit/5)。不得每会话各建一个伪全局预算。

领取flush只转走本批累计量与首脏时钟；inflight期间新变化另起累计与时钟。旧ack只推进相应提交版本，不删除更高版本、不清其累计量。测试覆盖65535/65536、499/500ms及旧ack期间新版本65535+1。

## 所有权与调用顺序

- 协议所有者以真实Run/Turn attach OutputScope，事务预留位置区间。begin_item先于页面发布或异步解码；同源更新复用ItemHandle，其他可见条目使用不同SourceId，不靠正文哈希合并。
- begin/update接收完整结构快照，协议增量到快照的转换由以后WP3逐版本适配器负责；本模块不猜协议、也不降成文本。未确认同源时建立独立条目。
- 每节点创建一个HistoryTask，它拥有保存任务集合、取消信号和JoinHandle；独立时限检查不会被阻塞存储future挡住。shutdown取消并等待子任务，再强制保存；Drop只取消并请求停止，不宣称完整关闭。正常Run/Harness退出顺序仍由节点资源owner负责。
- 条目结束、可靠Run终态、审批前、释放、关闭调用对应forced flush。失败或RetryDeferred不能继续审批交付或报告释放成功。来源静止且终态可靠后，finish_run保存并退役干净scope缓存元数据，不释放Harness；错误scope保留管理责任。
- 新输入经过HistoryCoordinator.submit_input预算门禁再进InputService；repository调用不是认证产品入口。WP3仍须持有actor gate，WP4仍须逐块认证/撤销writer协调。

失败停止保留可能缺失的位置范围与被拒字节数，不保留被拒正文。写失败先保留内存错误，后续事务成功才能保存错误事实；成功清零暂时失败计数/时间，但达到停止阈值后不自动恢复输出，也不擦除旧Run已有的中断/缺失事实。持久完整性事实按Run独立记录，避免新Run暂时失败恢复清掉旧Run的不完整历史。旧blob回收仅限purpose=output，不能回收尚未受理的input引用；pin_version在同一个BEGIN IMMEDIATE事务内读取当前引用并登记pin，与版本替换/回收事务串行，不存在先裸读再建立pin的窗口。读取pin仅负责不可变存储引用和回收；网络租约、60分钟时限、权限由后续网关owner负责。startup在开放会话前调用recover_interrupted，旧pin失效、回收无引用旧输出并标记可能丢尾；已预留区间不复用。本文不宣称已集成这些节点/Web调用。

## 开发检查

根目录设置独立CARGO_TARGET_DIR，Windows真实资源检查先执行testing/ci/windows-owner.ps1进程owner前置；history专项为cargo test --manifest-path app/Cargo.toml --locked --lib history::，完整Rust检查仍按根README。原始日志/退出码在工作区.tmp/wp2-2.2-logs/；开发自测不是独立L5/L6验收。
