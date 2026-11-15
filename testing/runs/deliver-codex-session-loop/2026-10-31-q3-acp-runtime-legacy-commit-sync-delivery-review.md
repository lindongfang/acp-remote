# 旧提交取消同步小修交付复核

结论：**测试端口与独立同步资产限定 PASS，可窄合。** `ACP-INTEG-LEGACY-CANCEL-001` 的同步缺口已有实际修复与独立库回归证据；其根组合终态仍 Open，等待根最终 source 清单及完整 check/all-target lint/build。ACP组件八个问题的Closed不代签此项。

本次只读复核，未改源/测试/历史证据。依据工程补充91c67e46、独立四行定义b0326ee7及门禁073c36bf；原真实并发失败和受控锁反例仍保留。反例证明旧25ms无法证明到达AfterCommitPending，不证明原失败确切时点，更不证明真实已提交记录丢失。

| 固定文件 | SHA256 |
| --- | --- |
| root port-v1/product_commands.rs | a6860bcd8c22352a754a9e66d26c90050ffb5125049beb5e9bf7d7f4213a0ef3 |
| root port-v1/from-baseline.patch | 7bcf099dc4748372d995f6039a173e168a4292e199249ae2a19e7b49b4608e95 |
| root port-v1/manifest.json | ec62724b96786a8d9af6390660467210f796c188ea863310417205ae51edcf49 |
| tester revision3/management_independent.rs | ff592e4e16479751314a78e522796d734a7d065218ed5c4641eea03a14c7aa04 |
| tester revision3/candidate-root.patch | 6ed196ceead7d9ff562610875a09a2633d0733d73ac287ed06d653e3f674b453 |
| tester revision3/acceptance.md | b66b2830dec5b3b4c0ba38116f9eaf617102850b0c6c13614be48cad9364e22d |
| tester revision3/delivery-manifest.json | 66daab6715c4db5788de95f9b936ae480052e5987a8fa2af365cc27ea71c0f08 |
| tester revision3/run-owner.txt | 51e0c23abea42ee2a5a328b0dbba64c6279d330e198246abce53e5ab187fe98e |

Root port目录为 `D:/Project/acp-remote/.tmp/q3-legacy-commit-fix/port-v1`；tester目录为同visualizations base下 `session-loop-q003-tester/registration/revision3`。所有14份交接证据已逐项实算匹配；reviewer亲算实际执行副本190文件与其声明actual hash均匹配，前后清单逐字节相同。对ACP v3的190基线仅Store端口和C资产两处差异，其余188及实际Web dist保持。

## 精确修复责任

1. Store仅10行新增。ProductStore/Transaction新增Arc Notify及构造/clone均cfg(test)；通知分别在既有CommitPending分支原pending之前，以及真实COMMIT.execute.await成功后的AfterCommitPending原pending之前。正常构建没有字段、通知或额外等待；未移动SQL、故障点、返回结果、期限或Transaction Drop责任。
2. 每故障分支新Fixture独立Notify。target/proof完成后才选择fault；通知只在所选故障分支发出，前置proof不会遗留permit。notify_one允许真实到达早于Notified第一次poll，不靠notify_waiters猜时序。
3. 同名C函数真正Box::pin持有admit future；poll_fn共同推进admit与通知，并要求admit一直Pending。到达后再次实际poll确认Pending，随后drop整个Box；未仅drop借用Pin、未detach后台任务。10秒只限制测试等候，不代替阶段证明。
4. 局部块保证Notified借用在Fixture.close之前结束；首稿E0505和旧文件保留。原faulted、count0/1、连接重新acquire、BEGIN IMMEDIATE/ROLLBACK、drop connection、Fixture.close全部保留。

reviewer亲自归一化换行比较：目标函数之外完全相同；从原faulted断言到原清理结束逐字符相同。原29测试函数未删改或新增替代计数。该变化修复oracle的到达同步，不改变产品接受/提交规则，也不通过加大timer、串行化或删断言掩盖失败。

## 真实验证

亲读run-owner.ps1及原始run-owner.txt：实际cwd为固定source，Rust/Cargo1.98.1，原windows-owner.ps1的新对象owner检查成功；fmt通过；同名定向函数1/1（函数内真实提交前/后两分支）、511 filtered；随后默认并发完整库512/512，0失败/忽略/filtered，44.04s，最终exit0。未设置test-threads或靠重试/删除等待稳定。

tester正常库check/clippy--lib实际0属于已观察工具输出，未补造成raw日志；不冒签完整all-target或根build。reviewer本次核文件和原日志，不将独立回归说成本人另跑。

## 合入条件

固定Store文件/其patch二选一应用一次；C文件/candidate-root.patch二选一应用一次，不新增挂载或Cargo变化。根先核前像和patch --check，再固定ACP产品+64包+本小修的最终组合清单，实际运行完整默认检查、含fixture的所有target严格lint、显式真实child测试及默认build；前后源无漂移后再关闭集成问题。原190/120、根第一次512失败、原C、首稿和反例证据均不回写。
