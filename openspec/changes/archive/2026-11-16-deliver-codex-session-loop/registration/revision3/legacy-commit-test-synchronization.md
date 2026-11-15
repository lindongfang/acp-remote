# 旧管理提交取消测试的阶段同步补充

2026-10-31。仅解决 ACP-INTEG-LEGACY-CANCEL-001 的验收同步，不改变产品路径、SQL、期限或故障语义。公共ACP v3的190源98393e3f保持冻结；真实根512库回归511通过、该旧用例失败，尚不把失败归为提交后丢数据。只读诊断依据为ada6052f报告及根原始日志。

原两源固定：`app/src/session_storage/product_commands.rs` 34074f23858433f20359de5556611abb444e50714e5b5b794cfc70553c080687；独立C资产 `app/tests/registration_acceptance/c/management_independent.rs` 927de2a44a29027ca48f1040932fd1c84e251245de9f82fda7fbd1ced29434a8。两者与根182基线相同，不能把已有25ms假设当作新ACP行为。

## 最小测试端口

- 沿现有 `#[cfg(test)] Fault::{CommitPending,AfterCommitPending}` 两个实际故障点增加私有、每ProductStore实例独有的到达通知。建议ProductStore和Transaction各持一个仅cfg(test)的共享Notify，初始化及begin转交也仅cfg(test)；正常构建不增加字段、分支、等待或接口。
- CommitPending分支在进入其既有永久Pending之前通知；AfterCommitPending分支只在真实COMMIT.execute.await成功后、进入其既有永久Pending之前通知。通知不代替SQL执行，不虚构提交完成，不新增生产yield、sleep、公共授权工厂或模拟仓储。
- 通知只由选中的原故障分支发出；此前proof/control等非故障事务不发，避免旧permit冒充该次阶段。测试各分支使用原独立Fixture，目标和proof先建立后选择fault。

## 独立资产维护

由原独立tester维护同名原用例的同步修订，先经原reviewer核准。使用真正拥有并正在被poll的admit future（例如Box::pin），将该future与到达通知共同推进；等通知实际到达且操作仍Pending后，显式drop拥有者取消。不能只drop一个Pin引用、spawn任务后丢JoinHandle或改变原产品提交路径。

保留两分支所有原oracle：取消后faulted、CommitPending持久行数0/AfterCommitPending行数1、随后取得连接并成功BEGIN IMMEDIATE/ROLLBACK，原资源关闭均不删改。25ms不再证明阶段；适当防挂上限仅让测试在未到达时明确失败，不是输入期限，不延长产品操作预算、不将整库串行化。

## 隔离和验证

root仅在新独立诊断/修复副本维护上述cfg(test)端口；tester只改自己的C资产，公共ACP候选、根未提交修改及原资产字节保留。先核两个故障点的通知相对真实SQL顺序及不会被之前事务消费，再固定端口/资产新摘要供独立复核。验证包括两个真实阶段取消、原count/锁释放、独立单项及默认并发完整库回归；若仍出现真实COMMIT成功后count0，必须按产品故障调查，不能放宽断言。最终根组合仍需真实check/build，公共ACP120例不替代该项。

当前仅工程补充，未获同版独立定义/预审前不修改该端口或原验收资产。受控旧timer反例只能证明其逻辑缺口，不能恢复原512失败当时未记录的确切取消位置。
