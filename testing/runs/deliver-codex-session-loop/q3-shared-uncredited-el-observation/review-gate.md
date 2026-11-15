# Shared E功能层 / L机械层：有限测试资产门槛

结论：**PASS — limited installation / compilation / one original exact invocation**。准在NEW独占D451副本按E后L顺序分别check/apply，必要格式化仅这两测试叶；真实编译、适用fmt/strict之后list确认唯一原quiet函数，再执行一次原exact。保原[V1,V2]循环，V1先红则V2未达，不另跑V2补绿。无Root636产品修改许可，无Shared功能通过结论。

## 实际核验

- 提案freeze `4c938ebda96c7282c1c3ab3c37ea445301d483459f5b4967ad9b72cb8114855d`全部26成员、24引用输入逐SHA核同。
- D原样本freeze `e7048f3bc5618cbfeb45fb369ac6bab73b60b41ea18dba1be03834f8ae0a0b1d`全部483成员及实际451源核同；四份编译/运行前后清单相同，真实归档exe为`a459ae188151758ac38771e039919b579a32bd2f2c36a922d9745c9efbfeeb08`。
- 读原stderr/STATE：D唯一exact V1 0/1，0.17s，notice后同次错误InvalidStage、tag=LaneWriting，先前Copy为1/1/0；正常join/firstremove尾达到，两raw与V2未到。不能追认旧6b6/C未知分支，也不是IO pollPending证明。
- 自有scratch分别严格check/apply E、L四命令全0，逐层后像准确；机器记录 `uncredited-E-L-review/verification.json`。无Rust/协议/进程执行。

## 精确安装范围

E patch：`c8a7edd65ae6614e041a6d1e79a6e2247f48a742293686a9632c4f2659a65850`，只transfer测试叶 `0f18dd02893abbf7e259dd20c8001b5a43eb0f77b3e508cc0736a6df4ff87c9c`→`334665fd4af0691ac0a8154e5e15d825abbd8cdac60cc5a6e5b063db08a1ea6e`。

L patch：`c21cdf2aa39e092f46e8fee3c073ecd2e88292920deb816ee9107c1aa49b04af`，仅在E后叠加transfer及module两测试叶，后像分别`49444467f0994de3e26473de646b71b3212b8ca6b7c4f7a671727e94e5926e66`、`b72a5e669af414798b9f89d9d30ad75f0477ea1c8c84a5fee3dec81ffce7078b`。保存两层精确原件及必要格式差异，原449其它源和生产全部字节保护。

## E保留的可观察义务

原代码确实先frame/current-progress，再取lane锁；Busy可先返回，CandidateReady不锁住随后的lane。Sender在真实prepare轮次中会进入Writing，再处理AwaitCredit。此前AwaitCredit或flushed=1采样不能保证下一次调用仍在同lane状态。因此允许本测试客户端有界让出调度，不改变生产拒绝语义。

old和post-notice两组均必须最终由原方法实际返回InvalidCredit；任何InvalidStage本身不算通过。仅同一次返回InvalidStage且原lasttag确为LaneWriting或PreflightBusy，才可在保护全过后yield再试。其它tag（含None/Switching）、其它错误、意外Ok、状态改变或预算到立即失败。每组在原调用位置一次捕获原deadline，循环/timeout_at始终复用，不刷新期限，不添加睡眠/Native事件/响应或重发RPC。

baseline必须真实Sent且started=flushed>committed；每次前后核原stream、view/read epoch、整个LocalTransferObservation、两cursor、frame/record/header空及原provider/enqueue计数，任一变化失败。它固定显示流，不冻结Source worker；多次Copy采样不成为原子快照/权限/强控短锁缝证明。

已实算从原old drain到B最终1/1/0段全文相同；Credit1开始到函数末的原raw/顺序/大数/End/candidate/0新增RPC/Weak/首关闭和清理全文相同。原IF09的真实InvalidCredit要求与IF10的Writing胜必须拒绝同时保留；不把所有拒绝换成成功。InvalidCredit+None尚不能定位某一锁内return，PreflightBusy也不证明本次授权成功；精确AwaitCredit return/pollPending/同步采用短缝仍未证。

## L与执行边界

L的support在原Module cfg(test,windows)私有树只载入一次，原声明pub(super)供同树transfer引用；没有生产重导出或新挂载。其余仅yield末分号、Result::is_ok方法引用及既有wait_metadata闭包Send+Sync界限。原support正文、实际Rig/控制/断言不改，无新allow/fake-use；原五lint首红保留，不能预称L使strict已绿。

执行需自有target/TMP和原必需SHARED_IF_FIXTURE_ROOT、marker/fixtures约定；源/真实编译exe/list/原始结果/首资源尾分列。出现新编译或strict错误先保存，不自行降oracle或加全局豁免；无法执行或下一业务错误均原样报告，禁止刷新样本。绿色亦只证本次实际到达分支，未自然发生保护负分支仍静态；若预算内始终Writing即按原期限红，不保证测试一定成功。

本许可不签整个IF30/83/RF、CL023、RF06、坏Bytes、Node/WSS/Harness、历史FIO/Busy或用户场景，所有原级别与父false不变。A正文数据适配另门777b，本报告不混入其代码或证据。
