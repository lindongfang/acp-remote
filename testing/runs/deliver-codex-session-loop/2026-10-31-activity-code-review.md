# WP2 / 任务2.1 独立代码检视

日期：2026-10-31。执行者：`/root/baseline_reviewer`。只读项目，未参与产品修改。

结论：**限定静态代码检视通过，当前四文件范围未发现阻断问题。** 该结论限纯模型；独立 tester 实际执行结果尚未纳入本报告，不以开发者的13项单元测试或fmt/clippy结果代签验收，也不签任务2.1已经完成。

## 版本

基线 `f6d1f8df63673eeebd6109d4ddb3e95e5b66b906` 加以下候选；实际逐文件SHA-256均与交接一致：

| 文件 | SHA-256 |
| --- | --- |
| app/src/domain/native.rs | 76abadd93d74f8435154568804a00dc7130d3de8c48366b6d09a2ab1f54d67b5 |
| app/src/domain/activity.rs | 4ce09688e51216fb9bd6f1ebac7f43e67152bb569240c4bdf66669f15d9d8b32 |
| app/src/domain/activity/tests.rs | f3dd3b7c6b63550746713a027d9ef95f0726b9bcf3095095a76e6eaf07749630 |
| app/src/domain/mod.rs | b376908d82df340dbf8f106242b193f7d30b8284b7d30584cf4161875884032c |

依据为现行七份spec、实施合同§2、阶段规划审查，以及独立阶段候选CL-043–045/056/066/067/072–074。阶段候选摘要为 `7372c42773a4573eae9388798f9329cbf31629ae6d629011ddc8c910128166d2`。

## 核对

- NativeSessionKey只含显式来源与不透明原生ID；路径、环境和连接不影响目标相等性。Debug不打印原生ID。三类代际独立类型，不等价替代。
- ActivityRegistry按目标键保留单个状态对象，拒绝另一个资源代际认领已有目标并有显式容量；同一对象内部普通输入与原生active/unknown、待确认输入互斥。
- idle观测不清除平台输入；confirm_insertion只解除插入不确定性，不结束Run。可靠拒绝只移除对应输入并保留已知原生活动。
- WorkToken绑定目标、资源代际及不回退的局部序号；finish_work校验当前token。自然完成后迟到取消/终态不结算下一轮，跨目标及旧连接回报被拒绝。局部序号没有冒充ACP原生执行ID。
- cancel_scope不创建Run、前台工作或额外等待。后台unknown/active和权限引用不被前台完成移除；权限关联Run为可选，不由Run终态批量失效。权限引用有界且冲突观察不覆盖原关联。
- interrupt保留输入归属、置前后台未知并关闭当前连接有效性；recover要求旧目标/代际一致、可靠停止或资源收敛，以及身份、唯一归属、恢复和管理核验全部满足。恢复仍保持foreground unknown，不能仅握手解锁；旧输入由返回值继续保存为原未知事实。
- 新模型没有接SQL、JWS、网络writer或进程；没有正常路径unwrap/expect，计数溢出拒绝且不留下半次受理。mod.rs只新增模块声明，旧强刷/正文模块尚未删除但未被此变更接回产品。

## 后续装配必须保持的边界

当前ResourceEpoch由运行代际和连接代际组成；SessionActivity对象内部序号从0递增。当前ActivityRegistry没有移除或重建同目标对象的路径，所审路径不会复用该序号。后续若同一连接内释放/重载后重建对象，必须保留不回退的目标序号或增加加载/资源代际，以防旧token与新对象首个work同值；本报告不授权这种尚未实现的重建接线。

can_accept_input、locally_quiescent和RecoveryFacts都是内层事实模型，不能直接成为外部授权或完整自动回收证据。后续调用者必须提供验证过的actor、目标、真实来源、资源与协议完成事实；不能把裸snapshot idle传成旧输入的finish_work完成，也不能仅因locally_quiescent就关闭原生后台范围未知的实例。

## 验证范围

实际执行：读取源码、开发测试、实际diff及SHA-256；未运行构建或测试。开发者报告的fmt/clippy和13项测试仅为输入信息，独立执行由tester另报。全部真实协议、持久化、跨进程、Web及Windows资源能力仍未验证。本范围没有新增稳定ID阻断问题；全change的BASE-001不因本报告关闭。
