# ResourcesConverged 退役与 waiting 处置：现有合同独立解释

结论：**现有规则足够，不需用户重复确认。** 合法资源收敛可安全退役同原active协调令牌；它不改变Run/native结果，也不独自决定全部waiting的suspended值。按触发事实分别处理，不能统一`ResourcesConverged => Suspend`，也不能统一`=> 清挂起并自动下一条`。本报告只解释已授权边界，不签任何WIP、Native80或具体关闭实现。

本轮只读下列实际文件和独立备忘，未改Root/作者/tester，未运行或关闭SNAPSHOT001及其它产品问题。

## 权威范围与固定输入

- session-execution：`d2c38379b6edba8c7556f13dc7cdf5ccdffd48a9de018caad2d31c398ce516c7`，同会话互斥、失败/非成功暂停、异常后重新准入、未知不重发。
- native-session-recovery：`c013e151444238361d96802e0e7519d8cc31fa42f6b788fe4413805cac79cb29`，22–25/42–54：原身份恢复、旧执行与结果独立，资源与必要记录未确认不报释放，不重发输入。
- implementation-contract：`cc4f9ef3cc80e2494af63312364cd157ebdb163d3caa10c9135880b0fe8a54ec` §2/4；design：`dea6d897889b6b007082d28480c17b137f8337ff21e4942f05f86f491298efaa` §2第25行/§3：资源失效暂停核验，条件解决保旧挂起状态；核验或收敛后按真实状态放行，不编造成功。
- management-command-contract：`873f642441cdd267acee4cd9cde6c72e886fce6f3fbb06280fc5036dc589b350` §2：临时active等待、unknown核验、资源失效暂停，禁止自动换目标/新建会话。
- shared-input-queue：`61987328a83927c414fba8a2b085f0796f05792a8072ae9ad11d6187d8e1c466` §完成/失败/未知：settle是可信应用核验后的调度决定，模型Advance不等于成功、不恢复既有挂起，收敛后的未知结果独立保存；settle也不是自动重试许可。
- Native6 engineering：`205b14c45b41fc3641a06d7dd9f7d20bcc05b5239a2aa5b92679b50bf659df76` §4/8/9；interfaces：`cfd17582ca5c23634e24e508d49e76d9b465f4987d9d9fc4c6cd9df4cb4749df`；有效closure：`4e23ec0810bd0d436c610123dba416f92eb161e62ced227bf91f679910810dcd` §4–6。
- tester备忘实际匹配 `c775e1f19dbe9ee1e0ef7edf434b52fca5ee7d9d418ec9792ab2ad0d952c809e`；原NDT021/062/069/071/081/112定义已对照。其备忘是证据索引，不另建行为权威。

Native §8表末行“不造新Suspend/Advance”禁止**从unknown/snapshot/丢响应自身**推断可靠业务终止或新的队列政策；不否定另一条真实收敛证明可安全移除旧协调占用，也不能否定真实独立资源失效的暂停规则。反过来，资源暂停不能被记录成native Failed/Stopped。两个范围没有产品冲突。

## 触发事实分类

| 真实触发 | 原active/结果 | waiting与下一条 |
| --- | --- | --- |
| 只有unknown、snapshot/repeated stop、丢响应、超时、断线或新握手；没有合法收敛/可靠结束 | 保原协调与未知责任，不能凭此释放active；不finish_work、不造Run终态 | 不凭这类协议事实新增Suspend/Advance；保原waiting状态，但前台/插入/资源/必要缺口仍阻新派发。 |
| 同原owner/resource合法VerifiedConvergence及必要记账可靠，**只足以证明旧协调责任已安全结束** | 可一次性关闭原dispatch slot并退役同原ActiveKey；旧transport Unknown/native结果Unknown原样，不回waiting、不新受理 | 退役本身保各waiting原身份、顺序、正文和suspended；不清已有挂起、不因“Unknown”添失败暂停，也不自行授下一条写权。 |
| 上行同时或此前已有独立、可靠的**当前资源失效/实际释放/停止新执行**事实 | 资源失效保护独立保留；收敛可关闭旧责任，但不补业务结果 | 按该资源事实暂停/核验，不得因active退役或资源重建自动恢复旧挂起项。若用队列Suspend实现这次暂停，必须以该失效事件为依据，一次作用于事件当时waiting集合；不得把旧资源事件迟到再挂后来新D。 |
| 另有可靠当前前台Failed或用户已确认的普通Stopped原因 | 依对应真实原因，不能拿收敛替代cause或Run关联 | 沿原失败/Stopped Suspend；旧项只显式继续恢复，新输入按既有优先规则且仍受所有互斥/重新准入条件限制。 |
| 合法立即发送priority/显式关闭/节点停止中的收敛 | 只解除对应原批、原slot及实际范围；不杀共享实例其他目标、不把closed写成成功 | 原priority授权仍只原选项及原围栏，既有暂停集合不自动恢复；关闭/Stopping期间不得因退役而重新ensure_resource。旧runtime已停止不授权修改新runtime队列。 |

“独立资源事实”不能只由`result==Unknown`、一个ResourcesConverged枚举名或测试布尔产生。也不能反向忽略实际资源已关闭：JoinedOwnedSource、实际NativeTargetClosed通常同时使其原资源失效，必须检查其真实kind/closure reason及原资源保护状态，执行相应暂停/重新准入；不得为了构造“纯退役不暂停”正例伪造资源仍Ready。

若当前允许的真实proof路径没有“无独立失效”的可达分支，该分支只保合同解释/静态不变性，不编造新的Convergence kind或fake Ready来跑。interfaces现有JoinedOwnedSource/NativeTargetClosedVerified/OldRuntimeStopped及closure的Source/Target/Abandoned证明互不替代；无load/input的新reservation Abandoned也不能拿来关闭已有active dispatch。

## 已授权的最小实现与恢复边界

1. 仅同实际原resource owner mint私有ConvergenceProbe/ReleaseProof，核原runtime、owner、scope（实际存在才核）、resource/dispatch/attempt与完整实际阶段。原Owned/IO/控制/安装/接收pump及必要SQL集合真实收敛；缺证据、JoinUnconfirmed、COMMIT未确认或必要gap仍保Unknown/Pending，不提前返slot。必要未知范围可被可靠保存，不要求把业务未知改成已知才能关闭。
2. 先可靠record_convergence/精确close原dispatch（提交未知按同proof inspect/幂等核验），再由原owner以同ActiveKey执行一次协调退役。旧克隆/旧资源/重复事件无效，迟到事实不能影响新active。不得调用finish_work伪原生结果，不把真实Ticket Unknown变Written/Zero，也不把资源released当Run Success/Failed/Stopped。
3. 若内部复用`settle(...,Advance)`实现“仅释放active、保waiting原状态”，这是**纯模型调度操作**，需明确记为ResourcesConverged退役，不能走“native成功”分支、发成功事件或因此循环取下一项。若真实资源失效要求暂停，则以该独立原因选Suspend/已有失效保护；不能只按Unknown决定布尔。
4. 退役后保当前资源/来源不可用、Closing、未知归属和准入fence；不因原run从map移除就自动新spawn/握手/重发。下一条不同的已受理输入，只有旧前台/资源确已收敛、原生身份与唯一归属已核、可信完整配置和所需真实恢复完成、必要状态可靠、当前前台/插入/transport保护满足后才能获得新的合法绑定和写权。新握手/恢复成功单独不够，不能静默新建替代会话。
5. 未新增挂起且之后完整重新准入已合法满足时，可按原节点队列规则处理**不同waiting项**；不能凭空要求重新授权原提交者或加新产品确认。已有挂起仍须有权显式继续，资源恢复/收敛/新输入成功均不等于resume；继续本身也不能绕过未知/资源准入。原可能已发送active的同请求重试始终返回原Unknown，不回队/换attempt重发。

上述是现有规则可实现的退役/保护，不授权新增普通cancel、审批或控制接口。本轮不替作者选择具体私有方法名/事件字段，也不把SQL与内存串联说成原子；实际实现须继续保同owner顺序及pending/幂等责任。

## 独立最小断言

- 无proof/错owner或旧resource/不完整Job或IO/必要SQL未知时：原active责任不退、slot不返、无新prompt；snapshot running/两idle(cancelled)不能产生新终态/队列政策。SNAPSHOT001新红包另轮复验，本轮未关闭。
- 合法实际收敛、同proof可靠记账后：原dispatch只关一次、原active只退一次；result和真实Ticket终态仍Unknown，finish_work/成功/停止事件计数0；原active正文不回等待，无原请求重发或自动ensure_resource旁路。
- 无独立失效时保waiting全部原状态（只在合法可达来源下验证）；另有实际资源失效时验证其原因/一次性暂停、原队列顺序/内容保留；先前suspended集合不被清，新D不被旧失效重复消费挂起。不要对所有ResourcesConverged硬编码同一true/false oracle。
- 仅新握手、身份/归属未核、恢复未完成、前台/插入未知及必要gap任一存在：即使active已退也零新prompt。完整合法重新准入后只可不同可执行waiting项；已有挂起未继续仍不发，重复原请求仍Unknown。
- 共享source单目标收敛不关闭/暂停无关目标；普通callerDrop不当join；节点Stopping和旧runtime证明不得触发新来源或新runtime队列操作。真实原proof/Slot/SQL/IO计数与当前队列状态均需同版记录，不能以普通inspector数据mint收敛。

此解释可用于作者最小退役实现和tester按触发事实补子断言，原80及其标题/oracle不改。当前没有产品语义冲突需要问用户；若未来提出“Unknown一概手动继续”或“仅资源释放即自动新建来源并重发”，那是超出现有依据的新取舍，不能混入本解释。
