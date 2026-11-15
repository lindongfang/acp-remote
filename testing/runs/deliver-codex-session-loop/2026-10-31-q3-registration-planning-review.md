# 登记、真实SQLite与实际队列owner：独立预审 v1

2026-10-31。当前只预审，未放行产品编码。旧Q3纯队列报告不改。只读root及外部草稿，报告独占本registration目录；无凭据/用户数据库访问，无Web audit或外发。审查输入初始SHA见planning-initial-inputs.json。

## 结论与可保留设计

R1内层/应用登记、R2真实SQLite迁移与故障、R3实际InputQueue owner应作为同一gate内的实施依赖顺序。允许WSS与真实ACP writer另gate；不能用登记CAS+假回调代签实际owner，也不能把三者组合称为跨SQL/内存原子事务。当前文档仍有准确接口和故障恢复待定项，**尚未通过方案/用例门槛**。

可保留：节点随机ID/未知ID不insert fallback；登记不等于受理，begin事务当前验权，COMMIT后节点接管；五封闭动作及独立意图；8字节revision不缩i64；无正文/TTL/time_floor；两新增表不改旧历史；applying→内存一次apply→稳定回执的隔离序列；相同settled幂等、不同结果冲突；必要事实和附加audit分离；新runtime不恢复正文/旧apply。无新增用户产品分支需要再确认。

## 编码前阻断门槛

| ID | 级别/位置 | 具体门槛与复现风险 | 必须冻结的工程依据 |
| --- | --- | --- | --- |
| REG-PLAN-001 | P2，登记“最小Rust接口”及owner全篇 | CurrentRequestActor已改由application mint是正确方向，但RegistrationFact/ConsumeLease/OperationProbe等仍仅private注释，可信外部Repository不能合法构造Owned/结果；owner公开操作、读取、恢复、启动、停止签名尚无，独立测试无法编译。 | 精确类型/构造器/getter/返回值/错误表；可信repository返回原始合法事实由application mint或等价可实现布局；恢复lease只能走验证路径；随机/clock/deadline/audit/Storage工厂实际入口。无生产任意setter或客户端bool grant。 |
| REG-PLAN-002 | P1，owner“一个目标一个owner” | 目前只声明每NativeKey复用QueueOwner，没有实际唯一注册表和targetID→NativeKey绑定API。两个可信调用分别new同key的owner即可各接三项，SQL不同request_id都成功，单请求唯一行不能保护共享容量；错误目标/资源观察也可能喂入别的owner。 | 应用级共享注册表/唯一实例拥有者，构造与获取路径、别名同key复用、不同目标独立；目标SQL解析与owner绑定的校验/不可改换边界；D前读取最新有效观察，不沿A/B前的Ready跨SQL使用。WSS装配可后验，真实R3注册表不能省。 |
| REG-PLAN-003 | P1，D至E及pending读取/调度 | D已修改真实InputQueue，但E未稳定；当前说“已有稳定队列可查询”却没有旧稳定投影。直接borrow当前队列会把新edit/submit或cancel结果当稳定事实，保留removed/replaced T也不会自动提供正确旧快照。BeginUnconfirmed/Faulted与旧prepare交接同样需要公开可观察围栏。 | 明确Pending/稳定读取视图及借用安全；可暂时只给处理中状态/可靠元数据，不伪报原始稳定body。所有pending阻prepare/begin/相关发布，原生reader/安全控制独立；D取当前Ready。无writer live R3只能Blocked；受控Ready仅验证模型路径，不可违反已冻结纯队列的“即时writer/不得await沿旧资格”边界或藏第四等待项。 |
| REG-PLAN-004 | P1，COMMIT未知核验/boot | 草稿承诺inspect自身applying后“恢复一次lease”，但没有一次性恢复能力的具体状态转换；也无startup与新begin/旧回报的精确先后。只从applying读出事实不能证明此owner从未D，查询成功不能无条件clear_fault。 | 同owner实例+operation+runtime+本地未apply状态的recover-once算法/接口；已apply只能settle重试；startup先封新动作、旧runtime applying→interrupted后才能开放；旧lease/异步begin或settle回报不能绕过重建围栏。不同连接实际COMMIT前/后未知及恢复分支可测。 |
| REG-PLAN-005 | P2，任务/关闭及audit | Pending说明有节点所有者和join，但没有精确任务生命周期、等待入口取消、故障恢复/关闭API和屏障。停止顺序还在后段才禁止prepare，若等SQL期间另有稳定队列不能让其继续取出。best-effort audit如何报告/超时/跟踪也未定义，不能意外又堵住下一项。 | stop起始同步封新修改与新调度；未commit/commit未知/已commit的caller-drop分别处理；owner跟踪SQL/核验/audit任务，节点取消与join顺序、工程deadline；audit typed元数据和告警可观察，独立失败不设必要全局fault，不脱离owner遗留任务。 |
| REG-PLAN-006 | P2，DDL与迁移/回执验证 | 两表形状方向可用，但跨表动作/项/版本的结果语义仅笼统“检查形状”，精确fault hook及可观测持久事实仍待定。仅DDL CHECK无法防Submit结果换item或Edit错误revision；仅before-COMMIT Err不算提交未知。 | 固定动作→合法outcome关系（Submit用登记ID/rev1、Edit expected+1且溢出拒绝、Cancel/Prioritize精确item/rev等），adapter读写均核验；迁移0–4→5/5幂等/半套5/未知/旧字节保留清单；闭合故障点和真实COMMIT后抑制回报/取消/连接丢弃观测，无生产任意数据库改写后门。 |
| REG-PLAN-007 | P2，阶段说明与独立定义 | 登记草稿开头仍说“此阶段不能证明actual owner”、多处“后续接线/后续L5”，与同gate R1/R2/R3目标混杂；tester v0还含下一writer阶段R4–R6并有register响应丢失措辞歧义。没有同SHA可执行固定定义不能放行。 | 三草稿统一本阶段真实SQL+owner受理范围；未来writer/claim/原生终态明确not-run，零writer不是实际发送通过。register尚未consume丢响应允许节点新随机登记，不需凭空增加register invocation幂等，但不可复活输入。tester按精确API/SQL/owner屏障定义新子断言，旧39只回归，reviewer同SHA复核。 |

所有问题当前open，阻断该阶段编码；这是工程定义缺失，不是产品规则待用户确认。草稿补齐后由本reviewer逐项关闭，不由coder/tester代签。

### REG-PLAN-003/004 补充时序反例

旧active W，旧等待X/Y；新D的begin SQL未返回期间，reader已收到可信W可靠失败。若所有active settle都等E后再应用，则D会先按旧active入队，之后Suspend将X/Y/D全挂起；这可能把失败后受理的新D当成旧项。Resume也可能越过已到的failure而被反向覆盖。仅说“准确事件buffer后按顺序”没有定义事件相对B/C/D的顺序。

需冻结可信观察事件与队列管理应用的单一处理序号/水位、受理与调度的线性化点，以及D之前/之后的终态处理算法。真实ACP终态关联可留WP3，R3仍须用受控可信事件实际证明该顺序；不能通过永远推迟全部终态来避开模型。该反例已交主执行与design，仍属原两项门槛，不另增产品规则。

另需核对无平台Run的原生前台失败：当前InputQueue只有本地active DispatchToken的settle(Suspend)能够挂起，不能为此补造平台输入/Run或改私有字段。主执行已交design评估最小`suspend_waiting()`增量：只挂起等待/失效准备与优先、设置失败模式，不清active、不证明终态；真实来源仍WP3。该方案尚待实际新合同/写范围/独立定义，本reviewer没有提前放行纯队列源修改。

## 验收与整体义务

本gate至少要能实际运行：真实SQLite两独立连接/事务、当前Storage schema5无损迁移；真实InputQueue+实际owner注册表及node-owned任务；同ID只apply一次、Edit只加一次revision、Resume只应用一次；COMMIT真实发生但回报未知时的两条核验恢复；A在begin COMMIT前撤销拒绝、COMMIT后内部推进不重验；整目标pending围栏且其他目标/独立控制继续；崩溃不重放body，停止取消/join无潜伏动作。受控authority/观察来源须明说替身，不能称已接真实认证或原生事件。

保持原任务2.2/2.4/6.2/6.7及相关CL L5/L6未完成；本gate即使通过也不删除真实发送claim/owner、审批、WSS/Web、Harness和Windows树义务。生产清理/附加审计保留尚未实施要单列，不伪签；未知旧ID和保守不清理的拒绝能力仍须本阶段实证。无新依赖、无外发、无真实用户数据操作。

## 冻结v1实读复查（尚未放行）

已亲读主合同b2c03bcb9393d37aaac40eae6c32cbefff0e21eaac6aebedee0e83bcf4077a05、owner合同4109374c74b5773daf16c4ebbdb924c5080a86e74af27e4c5afb0086e2bd0363、DDL ba49b4012f96c42220744c0dd9c975035408f0649209eb77ac983aef04fc8a1d，freeze-v1清单50b76853d88d714e32250e43987a292533a2a1b23732c0844e6e1ab52bd0dbc0。

- REG-PLAN-002：设计部分closed。Service唯一Arc注册表、每Storage一次启动claim/目录所有权、target/native事务复核及D前最新readiness已明文闭合；实际装配及并发仍待实现验收。
- REG-PLAN-003：设计部分closed。Processing可借用当前全文但整个视图明确暂定/applied，不能称Stable；pending阻调度而可信Settle即时短锁消费，C/D同短边界无await，失败→D与D→失败准确分开。新增suspend_waiting及ExternalWorkKey单次消费不造Run、不清local active，重复旧W不挂新D。纯方法新增写范围与tester QEXT/EXT独立定义仍需最终同版复核。
- REG-PLAN-005：设计部分closed。stop起始同步封新修改/调度，caller等待与node任务区分，未确认返回责任清单/join false不假成功，audit单独typed元数据/告警及跟踪取消，不阻下一管理动作。准确方法表已足够定义这部分预期，实际任务关闭仍待代码/测试。
- REG-PLAN-006：设计部分closed。补逐动作outcome精确关系及读写两向检验、真实COMMIT前/后故障、增量迁移/旧独立资产按tester新版本而非删断言。合法独立故障注入入口仍受001未关闭限制。
- REG-PLAN-001：仍open。Service唯一公开启动工厂固定SqliteRequestRegistry且new为crate私有，独立外部真实repository包装器没有合法注入R的入口；cfg(test)故障点不在外部path依赖可见。需要使用同一次Storage启动claim、委托真实SQL的trusted包装工厂精确签名。直接选择D也需仅node driver可读的现active key入口，不可伪造ActiveKey。
- REG-PLAN-004：仍open。begin的Started(StoredApplying)可由应用mint lease，但inspect_operation仍仅返回RequestFact::Applying{registration,operation}，缺runtime/admission generation/begun_at；与恢复“核对StoredApplying全部字段”不一致。须给足够的OperationInspection返回结构或同样可验证的精确恢复算法，不允许填假metadata或绕过原probe核验。
- REG-PLAN-007：仍open。R1/R2/R3阶段范围已统一，不再把实际owner推给无限下游；等待上述API修订及tester绑定最终SHA的完整固定用例。原生writer/真实来源/WSS等依赖明确后gate，未被删掉。

已读tester事件顺序及无Run候选。C/D同短锁不可插入，不强造生产yield；真实COMMIT完成但owner未处理返回的故障屏障属于D前分支，D后/AwaitSettle失败是必测反向对照。两种证据不能冒计为不存在的C/D内部交错。v1冻结字节保留，剩余修改由design输出revision2后复查。

## revision2接口复查

已逐hunk阅读revision2与v1差异；主合同d4a2840699b69a689fb3a43a39b7874609a38851052cd094a2e68c6364f1e74c、owner32d1f7728bcd20afb860575e6dcb673070acba398d1e83cc62c61211e00b1314，DDL仍ba49b401…；freeze-v2清单2b60d4a8406eafd9576985ec802a34138cbb2b60e4ad9a02dfbff1c0d67c5a8b。Git no-index diff exit1只表示两版文档有差异，不是校验失败；未写文件或转换换行。

- REG-PLAN-001：设计部分closed。Storage增加with_repository工厂，接本Storage真实SqliteRegistry并由可信F包装成R；默认工厂identity委托，同一次claim/map/startup，不创建第二owner。外部集成可真实inner COMMIT后丢应答，不依赖cfg(test)可见性；同crate隔离测试另覆盖真实事务内/迁移/random闭合故障点，两类不混报。补RegistryDeadline合法构造和driver.active_key当前opaque只读入口，直接D可通过受信driver合法结算，仍没有正文/first-write许可。
- REG-PLAN-004：设计部分closed。RequestFact::Applying现在携带完整StoredApplying，核验真实SQL runtime/admission generation/begun_at及登记元数据；本地pending匹配已知坐标并一次消费未D状态，已D仅补settle。startup开始后保留Starting/Unconfirmed句柄和claim，不Err后偷重置；只有尚未发startup SQL的随机/wrap失败可回收claim，shutdown joined=false保留责任。
- REG-PLAN-007：仍open；其余六项设计门槛已关闭。等待tester同revision2确切API/SQL/owner事件/新纯扩展/旧迁移资产预期的固定定义后，才可放行R1/R2/R3整体编码。未运行或声明产品能力。

## 最终编码前结论：R1/R2/R3同gate方案及用例通过

已亲读独立tester两份固定定义：v2-fixed-definition.md SHA `43783ca802a6d0430104316276401df21da88ab66e4ac3fa9fb48d7bfab7af89`；v2-event-and-extension-definition.md SHA `9a024d01a72361e1615729fdce24639b9d01f6e5bbb22c60816267bc2c1d6edd`；定义manifest SHA `b3b5366ac50990cdb2b72e92232335dc5cf0b992ac69b49789828b0c07cda2fe`。逐行核对49行CL子断言、QEXT7、EVT12（02不可达不计执行）、EXT8，别名映射同一测试不得重复充数。

固定定义覆盖：节点登记和未知ID拒绝、逐意图冲突、真实两连接/真实SQLite事务、实际共享owner一次apply、BI/BA/SI/SA/UA/AU屏障、完整Applying核验和恢复单次lease、actor C前后撤销、当前读取、Processing暂定全文、caller drop/node-owned责任、shutdown/join、附加audit隔离、无TTL/正文落盘、schema0–4到5及旧数据保真、故障与半布局、逐动作结果语义，以及可靠失败事件在D前/后与Resume的反向矩阵。无Run工作由合法ExternalWorkKey一次消费，旧W重放不挂新D；local W由真实service及driver合法建立，未伪造active或Run。suspend_waiting精确返回新挂起数、保active、失效准备/优先及原39同版回归均有固定预期。

P外部integration通过真实Storage with_repository包装，仅inner真实COMMIT后抑制回报；F/M隔离同crate测试用闭合cfg(test)故障/迁移控制器，保overlay和逆向证明，不用外部库无法访问的私有hook；C由tester版本化旧迁移资产，仅当前终点改5、固定fixture1–4/旧DDL/表计数和旧断言保留。源码目标名和封闭hook实际模块位置允许实施后机械映射，不允许改故障时点或oracle。生产清理/审计保留、真实writer/来源/WSS等未实现分支准确单列，不转换成fake或LX。

已由本reviewer逐项核验定义manifest的38个来源、2定义、4合同/清单，共44摘要全匹配；v1/v2冻结7项也全匹配，v1原字节仍在。精确结果见planning-final-inputs.json及freeze-v1-v2-check.json。**REG-PLAN-007 closed**；REG-PLAN-001–007均由本reviewer复查关闭，无新增编码前阻断。

允许主执行按revision2已列写范围串行实现R1→R2→R3，含最小纯队列suspend_waiting/settle复用、真实Storage schema5增量及实际QueueService/driver/任务责任。各依赖属于同一授权阶段，不能只交CAS/假仓储就结束此gate。旧独立迁移资产须由tester给新候选，reviewer逐hunk核对后再集成；旧Q3的39资产/历史报告不改，更新源实际重验另记。

这只是**方案与验收定义通过**，当前没有本阶段产品代码、真实迁移或新增测试运行结果，未给交付通过。源码固定后仍须独立实际验收和同版交付检视；R1/R2/R3本阶段必需项未运行/失败不得交付。整体2.2/2.4/6.2/6.7及原CL L5/L6不因本结论勾选。

本结论不放行真实ACP writer/首次写许可、原生终态归因、审批owner、WSS/Web或Windows/Harness装配。那些门槛保留明确后续依赖，不能用当前driver/受控Ready或0 writer调用冒充真实发送；没有writer的live环境保持Blocked。无新用户产品决定、无外发审计/新依赖/实际数据库操作，不提交或改Git索引。

允许按原字节将本新报告及固定合同/定义归入本次root新文件/证据，并在plan记录准确范围与SHA；纯交付记账无需递归摘要，不改写v1、旧报告或扩大完成范围。
