# 未冻结 R3：关闭期间应用与最终 claim 围栏

2026-10-31，独立只读过程审查，非交付签字。本次不关闭 START-001、SHUT-001/002，也不修改原合同/测试预期。

## REG-IMPL-SHUT-003：关闭后从仅 Applying 补做新 D（P1）

已亲读实际开发失败日志：`registry-dev-slots-final.txt` SHA256 `8814cba1caa7e63ece7dfa082bfecb55c9c31d3b35a433e7a7b49e9e767b0e4c` 为17项、16过1失败；`shutdown-deadline-slot-retest.txt` SHA256 `1443ac59592ad351638b0ebd03f7dbd40fb9ab9cdbcd673d9d7df61fffa3dceb` 为定向1失败。两者均在 `registry_shutdown_deadline_keeps_claim_until_cancelled_node_task_is_joined` 的重启后 Interrupted 断言失败。日志没有输出实际变体，所以本报告不把实现者所述 Settled 当作日志直接观测。

现有测试路径真实可达：begin AfterCommit 屏障→首 shutdown 立即到期且保 claim→释放/取消原工作→再次 shutdown→启动下一 runtime→查旧登记。冻结 owner 合同 SHA256 `32d1f7728bcd20afb860575e6dcb673070acba398d1e83cc62c61211e00b1314` 168–179行规定：applying 提交而 D 未发生，重启转 interrupted，不恢复 D/body；关闭首先停止新修改，deadline 不能为赶关闭虚构 settled。inspect Applying 本身不授予新应用权。

缺陷机制是关闭收敛路径对未应用 pending 调用一般恢复流程：`run_reconcile` 将 Applying 送入 accept_started，后者可以 mint lease 并执行 D。它把正常 Running 同进程恢复一次的许可，错误延长到 Stopping。修复不能靠改变原 Interrupted 预期。

当前阅读点 service.rs SHA256 `920984110eea4be4ebd53313314ba0829b3a7b87388c40e3554c06c386593436` 已有两项修正：accept_started 在未 applied 分支、同 owner 锁内检查 Running（527行）；shutdown 只为 `!worker && applied` 的 pending 补结算（944行）。这与合同一致，但当前文件仍在开发，未运行独立验收，问题保持 open。

独立关闭门槛：真实 begin COMMIT 后、D 前暂停，先关闭再释放回报或取消原工作；重复关闭不得产生新 queue revision/active/outcome、不得新 settle。第二连接确认保留 Applying，下一 runtime 只转 Interrupted、空正文队列。反向必须覆盖 D 已发生但 E 未可靠确认：允许核验/补同一次 E，不重复 D，保持历史 outcome。原 actor 撤销后节点处理规则不变；这里检查 runtime 停止，不重新检查原 actor。

## REG-IMPL-SHUT-004：最终状态释放与入口检查未共用围栏（P1）

同一 service.rs 阅读点 890–910行在 tasks 锁内检查 Stopped，随后写 Stopping；978行 joined 分支却在单独 `tasks.clear()` 返回、已释放 tasks 锁之后，再清 owners、写 Stopped 和释放共享 claim。Storage factory 133–161行以同一个 `queue_runtime_claim` CAS 取得启动权，并将该 Arc 克隆给每个 runtime。

可允许的线程交错：S1 已实际 join 并完成 tasks.clear，尚未发布 Stopped；S2 取得 tasks 锁并读到 Stopping。此时 S1 不需要该锁，继续发布 Stopped、释放 claim。新 factory 可取得 claim。S2 再写回 Stopping，等关闭互斥后不再命中 Stopped 早返；它看旧空集合，最终再次将共享 claim 置 false，可能释放新 runtime 的 claim。关闭互斥未防住这一点，因为 S2 首次状态写位于该互斥之前。

最小修复：最终 Stopped/本 runtime claim 释放，与入口状态检查/Stopping 转换使用同一个注册围栏；Stopped 不得回退，旧 runtime 不能再次释放新 runtime 的 claim。可保留现有专用关闭互斥及 Core 持句柄结构，无须新增产品行为。最终清理还需保持既有 tasks→owner 的锁顺序。

独立验证：同源静态核对不可再有上述交错；已有 BA/SA/AU 并发关闭/取消等待测试保留。公开 Pending 屏障可达的分支须实际执行；最终无 await 的短边界不得为造测试插生产 yield。追加关闭完成后旧 service 再次 shutdown、再启动新 runtime、旧 service 再次 cancel/shutdown、第三 factory 仍拒绝的代际回归。若最终短缝隙不能由公开接口确定性调度，诚实记录静态覆盖与压力回归，不能冒称已确定性动态命中原缝隙。

本次未保存或执行失败版本源码，不能给失败二进制冒配当前源码 SHA；引用日志只证明原断言实际失败。当前修正源同样不构成冻结交付。003/004均需修复后同源独立证据，未签通过。
