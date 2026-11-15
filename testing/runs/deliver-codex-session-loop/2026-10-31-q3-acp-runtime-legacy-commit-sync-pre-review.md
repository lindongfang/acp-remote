# 旧提交取消同步补充：受控红与工程预审

2026-10-31。**91c67e工程方案限定可行；最终修改门禁待tester同版定义核验。** ACP-INTEG-LEGACY-CANCEL-001保持Open，尚未签root组合通过。只允许随后在隔离副本加入最小cfg(test)阶段通知和独立资产同步修订，ACP v3原190及原资产/历史不回写。

## 受控反例已实际确认

亲核root`.tmp/q3-legacy-commit-counterexample/diagnostic-source.json` SHA256 `9868740e5b2d6eeada35b44d272afa1f908b31a70574fff3ecc07c482b72d0af`：190项当前全部匹配；相对ACP v3只C资产一项变化。完整diff仅fixture自有数据根及追加一个diagnostic函数，原函数/断言和产品源码均不改。

亲读run.txt `9f8151c99e78b5f49dc15521515bf6508b5b4a5a2a34eaa9d8e1f5421b03f84c`：真实1项失败，0.10秒；timed_out=true、faulted=true、planned_fault_not_taken=true、durable_rows=0，最后保原count1逻辑的断言红。真实另一连接BEGIN IMMEDIATE使admit卡在BEGIN，Fault在BEGIN后尚未take，不可能到AfterCommitPending。因此此实验确定证明原25ms不能证阶段，**不宣称重现原512失败的确切取消位置**。

## 最小工程边界

补充文档`openspec/.../registration/revision3/legacy-commit-test-synchronization.md` SHA256 `91c67e4632c7dab02d0d536dd02aeaeaa34e788605213e950936d6b7b3fa262c`已亲读。固定旧ProductStore源34074f23及C资产927de2a4保持历史字节。

1. ProductStore与Transaction的共享通知严格cfg(test)，每个独立Store/Fixture新建，begin只在cfg(test)转交。正常构建无该字段、克隆、等待或分支；不改变实际SQL、bound期限、Fault分支语义或正常产品接口。
2. CommitPending在原永久pending之前发；AfterCommitPending仅在真实COMMIT.execute.await成功返回后、原永久pending之前发。通知由实际执行选中故障分支发，不由测试根据配置fault自行补发；proof/control及其他非选中事务不发。每分支原Fixture新建，不能旧permit冒充本次阶段。
3. 必须防丢通知：notify_one保存单次permit，或在admit首poll前构造并enable Notified；不能先notify_waiters再注册造成另一种时间竞态。此为工程细节，无新public API/授权grant。
4. 独立测试真正Box::pin拥有admit Future，和到达通知共同推进；到达后确认该操作仍Pending，再drop Box owner。不能仅drop as_mut得到的Pin引用、脱离spawn任务或用更大sleep猜阶段。
5. 等阶段/清理的防挂上限不改变ProductStore原bounded/5秒操作预算；未达阶段或操作已返回须明确失败，不能继续把早取消当after-commit分支。
6. 原faulted、CommitPending count0/AfterCommitPending count1、随后获取连接及BEGIN IMMEDIATE/ROLLBACK、原资源close全部保留。不能降低并发、放宽计数或吞SQLite错误。如果真实确认COMMIT后仍count0，重新按产品故障调查。

## 后验最低证据

独立定义应绑定91c67e、两个实际阶段、通知注册/来源、真正Pending owner取消、原全部oracle、单项与默认并发完整库；资产由tester另版本维护。实现后逐hunk核cfg(test)排除范围和通知相对COMMIT语句顺序，核正常build、指定1.98.1/source cwd实际两个阶段回归、完整默认root库/check/build。若修订触及更多产品或原生行为须重新界定，不能顺带改旧存储关闭/SQL/认证规则。

旧512红、12孤立绿、受控timer反例、新同步候选及其结果分别留存。新ACP120绿不能代此根组合条件；本预审不提前关闭该中性集成问题。
