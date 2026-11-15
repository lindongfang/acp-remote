# Writer 期限仲裁修复方案

2026-11-01。ACP-IMPL-WRITER-DEADLINE-001；工程修复草案，等待独立最小复现和编码前门槛。既有公共运行时合同“写轴、响应轴与期限表”的 write_deadline、传输结果和“控制优先、唯一 reply 与 ClosingDrain”的关闭责任仍为权威，本补充不改 API、产品期限或结果定义。

## 现象和固定输入

独立旧权限验收 P12 在真实 PermissionReply 已 enqueue、实际 AsyncWrite 返回 Pending 且接受零字节后，Ticket.wait 长时间不结束。测试角色正在以进程外 watchdog 最小化，尚不能将未完成用例记通过。Root 未修改生产 writer；实际受测源为 `2026-10-31-q3-acp-dev-lint-source-v2.json` 的248项，仅四份开发测试相对原ACP v3有lint修订。

实际 writer::write 在 poll_fn attempt 中处理 Finished、取消、到期和 Stop，再经真实 Proxy 做最终同锁检查及IO；末尾 biased select 却按 notify、deadline、attempt 排序。deadline 分支仅重新进入循环，下一轮已到期 sleep 仍先就绪，真正终结操作无法获得 poll。Tokio cooperative yield 不构成分支公平性保证。

独立最小红及排除替代假设由 tester 保存，正式门槛固定其源码、命令、原日志、父进程超时和真实 IO 阶段；不把静态阅读当动态复现。三项待检假设为期限分支饥饿、终态通知遗漏、自有 Pending writer 唤醒缺陷，按最小复现及对照分别收敛。

## 唯一拟定生产修订

令同一个 attempt 在 biased select 中先 poll，再等待状态通知和绝对期限。attempt 的现有短锁检查决定已生效终态；实际 poll 前仍由 Proxy 再核状态和每次 fence。通知和 timer 只唤醒重新检查，不成为无限抢先的完成空分支。

不新增 sleep、轮询间隔、随机公平性或统一超时；不移动真实 provider 结果的同锁记录，不改变 ClosingDrain、控制优先、取消、deadline冲突优先、Reservation/Ticket 所有权或首次终态。原有 failure 函数继续负责实际接受字节与结果分类，不在等待方伪造结果或取消所有者。

## 实施和验证边界

Root coder 仅在 `.tmp/q3-writer-deadline-fix/workspace` 修生产 writer 及补开发回归；基线248源和前像已另存，不改根或独立资产。独立 tester 扩展实际 PermissionReply 的 Pending0、partial、flush Pending 和已 Written 对照，保早到 Received、无 consumer 时推进和真实 owner join 的断言；具体资产及执行定义由 tester/reviewer 冻结。新增开发例使用实际 AsyncRead/AsyncWrite 和阶段通知，不伪造收到的请求或写资格。

修复后运行适用定向回归、格式和严格检查；独立测试同版重验并由原提出者复核。之后 root 窄合固定 patch，重新冻结实际组合并执行完整检查、显式 fixture 与最终构建，才可关闭相关集成问题。此草案不签任何已有组件问题重新通过或整体闭环交付。
