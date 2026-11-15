# 根组合旧COMMIT取消用例：只读阶段证据诊断

2026-10-31。**登记ACP-INTEG-LEGACY-CANCEL-001（Open，根组合验收阻断，尚不归类为产品缺陷）。** 静态确认旧25ms用例不能证明进入目标COMMIT挂起点；这足以说明测试同步证据不足，但不替代本次实际失败落点的并发复现/阶段取证。未改源码、资产或原oracle，没有重跑大矩阵。

## 原证据与范围

- 实际根全库日志`.tmp/q3-acp-v3-root-lib.txt` SHA256 `c64bcfdaec1053157fe701fc09b9c8657b4c7510874501e2348a3d12533e8589`，512中511过，唯一失败management独立函数`m_commit_cancellation_distinguishes_rollback_from_durable_unknown`，第1306行count0≠1。退出记录`12db64094f118e49608ccd0688e93d2bff43baa9e013f77ac7b7cb6b171d9aeb`记library101、范围为指定ACP v3全库而非最终集成通过。
- `app/src/session_storage/product_commands.rs` SHA `34074f23858433f20359de5556611abb444e50714e5b5b794cfc70553c080687`；C资产SHA `927de2a44a29027ca48f1040932fd1c84e251245de9f82fda7fbd1ced29434a8`。两次读取前后均一致，且与原182基线相同，ACP190候选未改该区域。
- 精确单测重复12次results.json `2877c425e8b02f414ea27c9c838dd30f317d87ef67f8c88c1295026583ebf3f4`全exit0；亲读首/末原日志各1/1。它们仅说明孤立运行可过，不证明默认并发同步充分。

## 实际用例做了什么

每个Fault分支先构造fixture、target和proof，再设Fault::CommitPending或AfterCommitPending，然后对**整个f.admit**施加25ms timeout。f.admit直接调用ProductStore.admit，不经ProductInputService/authority/P256路径；仍须经过真实BEGIN、actor/clock/target/旧记录/容量查询及多次INSERT，最后才Transaction::commit。

用例只有timeout.is_err、store.faulted及最终表count，没有阶段arrived/watermark、没有确认Future已在对应pending点，也没有先证明COMMIT执行成功。设置Fault只选择将来分支；Fault在BEGIN成功后.take也只能证明事务开始，不证明提交到达。

## 真实提交和取消的顺序

Transaction::commit中：point → CommitPending分支永久Pending → 实际`COMMIT.execute(...).await?` → AfterCommitPending分支永久Pending → finished=true。故AfterCommitPending确实只能在SQL COMMIT成功确认之后到达；若已真实到达该点，随后取消不应回滚这份已提交事实。

但ProductStore.bounded的Guard在外部取消时会fault；Transaction未finished的Drop也会fault。取消发生在COMMIT之前任一await，同样可以“timeout+faulted+count0”。所以原后者断言并没有证明自己的前提；0≠1不能直接解读为“AfterCommitPending已经到达但持久记录丢失”。前者即使得到count0，也可能只是更早取消，并未真实验证目标CommitPending点。

一个纯逻辑/可复现性反例是同fixture另一个真实连接持BEGIN IMMEDIATE，令原admit在BEGIN等待超过25ms；timeout/faulted可出现，Fault尚未take，更不可能到AfterCommitPending，count却为0。此处未执行该反例，也不声称它就是根全库这次的锁争用原因；它只说明25ms不是阶段证明。CPU/磁盘/调度等实际延迟来源仍由根并发诊断取证。

## 最小后续验证/修复门槛

1. 两个分支均须有可信测试阶段到达证据：CommitPending在实际COMMIT之前，AfterCommitPending只在实际COMMIT成功返回之后。不能仅凭计划fault值或经过时间推断。
2. 先驱动同一个真实admit Future至该到达点且实际Pending，再取消/drop它；定时器只能限制等候阶段/收敛，不能当阶段oracle。不得简单把25ms改更大sleep、更改count0/1、减少并行或忽略失败掩盖缺口。
3. 原faulted、count0/1、随后BEGIN IMMEDIATE/ROLLBACK可用性断言全部保留；COMMIT与连接释放/事务rollback各自证明，不把调用Future结束当SQLite worker已关闭。
4. 若需测试专用阶段信号，应严格cfg(test)、就地贴真实SQL前后点，不改正常产品行为或原独立历史文件；新候选/版本化资产逐hunk及独立定义另审。没有授权本报告自行增加hook或改根文件。
5. 保本次512红、12孤立绿和正在进行的默认并发复现原文。先取得真实落点再判产品或资产问题；新ACP120绿不能代替这一根组合失败。修复后仍需实际完整默认root检查/build闭环，当前不签组合交付。

这不自动重开已按原证据关闭的SQL物理关闭问题，也不更改旧management持久未知/rollback语义。它是本次组合验证新发现的阶段证据不足，不能用新协议候选通过或产品源未变化跳过。
