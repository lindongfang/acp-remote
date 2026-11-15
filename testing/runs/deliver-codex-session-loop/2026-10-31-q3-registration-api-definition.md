# 登记/SQL/实际 owner：固定 revision2 验收定义

2026-10-31。仅定义，产品和新测试均未编码/执行。绑定下列已亲读并核对的revision2三份文档。v1工程缺口已由本版合法端口精化，本文固定验收预期与执行安排，交独立reviewer检查，不代签方案通过或授权产品编码。

| 固定来源 | SHA-256 |
| --- | --- |
| registration-contract-draft.md | d4a2840699b69a689fb3a43a39b7874609a38851052cd094a2e68c6364f1e74c |
| queue-owner-application-draft.md | 32d1f7728bcd20afb860575e6dcb673070acba398d1e83cc62c61211e00b1314 |
| schema5-registration-draft.sql | ba49b4012f96c42220744c0dd9c975035408f0649209eb77ac983aef04fc8a1d |
| freeze-v2.json | 2b60d4a8406eafd9576985ec802a34138cbb2b60e4ad9a02dfbff1c0d67c5a8b |

这些文件均位于 `C:/Users/jack/.codex/visualizations/2026/10/30/01a18dc1-45b2-7f82-981e-964264221cef/session-loop-q003-registration-design/revision2/`。原39公开API资产07f19cd4…及原报告3331737…不改。本文件保留v1全部子断言并精化合法端口，schema DDL未变；不缩减原CL义务，不将纯registry测试替代实际owner。

## 层级、实际与替身

- `REG-QEXT`为新增纯queue L3；下列SQL/owner为L4子阶段。CL原ID/标题和L5/L6义务保留；`.REGnn`只是本阶段子断言，不另立产品规格。
- 用真实Storage/schema5、真实SqliteRequestRegistry、真实QueueService及唯一owner map、真实InputQueue。两个真实独立SQL连接/池对同一自有文件读写。受控RequestAuthority、ValidatedBodyPort、NodeClock、QueueEnvironmentPort、AuditPort是明确可信替身，不证明WSS认证、真实digest codec、ACP来源、writer或原生结果。
- 当前live没有writer，所以环境Blocked；只在明示的内层调度分支使用受控Ready。NativeQueueDriver的prepare/begin_selection只建本地选择、没有正文和首次写许可；所有测试原生发送次数均为0，不把0调用说成真实派发成功。
- 不运行旧全矩阵。schema3/4独立资产需由tester另存版本化候选，保留原断言，仅将Storage当前终点改5；fixture原版DDL/字节和固定1–4版本不变。39原资产在实际新纯queue扩展快照回归，结果另记，不回填历史。

## 固定输入与公共观测

`id(n)=EntityId::from_bytes([n;16])`。自有node_meta按合法schema5初始化；A=id(10)、B=id(11)为trusted、初始generation分别1/2，R=id(12)为revoked。T1=id(40)绑定(source=id(1), native=`reg-q3-synthetic`)，T2=id(41)绑定另一来源同native ID。T1经service.clone和多个合法调用者应同owner，T2独立。所有数据合成，不读取用户目录。

E1={runtime id(101),connection id(102),load id(103)}，E2仅load变104。service runtime由工厂合法产生，测试读取合法ActiveKey/ExternalWorkKey getter关联，不自行传造runtime/owner/serial。W/X/Y/Z/new_d/new_e正文包含不同canary，泛型Body包含不可Clone/Debug的完整结构；trusted validator返回冻结Body与确定digest，客户端没有ValidatedBody写字段。重试重新validate同一合成raw，不clone正文。

- `validate_body`→`ValidatedMutation::{Submit,Edit{item,revision,body},Cancel,Prioritize,Resume}`；register借用mutation、不调用queue，返回RegistrationFact。Submit item的EntityId通过合法规范编码与EntityId::decode关联，不访问RegistrationId私字段；具体getter若提供可等价使用。consume移动mutation。
- 消费后匹配 `ConsumeReply::Resolved{fact,returned}` / `Pending{request}` / `Refused{error,returned}`。首次成功采纳body不退还；未采纳/Existing重试返回本次原mutation，Pending由owner继续持有。RejectedFull是稳定管理结果，同ID重试不能因空位出现再尝试，重新尝试须新登记。
- RequestFact的Registered/Applying/Settled/Interrupted/Retired是管理事实；Settled.Queued可以与实时queue中已编辑/取消/重启丢body同时成立，不能据历史回执恢复正文。
- `inspect_queue`可信同步闭包中复制**测试投影元数据**：Stable或Processing(phase,applied,request)，waiting有序(id,revision,submitter,editor,suspended,digest/body marker)，active可选同字段，failure_suspended。不clone生产Body、不让借用逃逸。按冻结文档及主执行纠正，Processing可以借用暂定全文，但整个视图不得伪装为Stable或已可靠受理。
- `query_request`必须当前读取授权；`reconcile`是node内部自有pending/probe恢复，不从客户端请求生成lease；`driver.observe`/`poll_target`处理可信事件而非网络DTO。D前排空已进入同owner序的可信观察，再取最新readiness；C/D同短锁无await，不能插任意测试yield。

## 屏障及判定规则

| 屏障 | 真实停点与观测 |
| --- | --- |
| BI | begin INSERT前或COMMIT前，阻塞/失败时第二真实连接看不到Applying，实际queue未D。 |
| BA | begin真实COMMIT成功、回报尚未给owner；第二连接可见Applying，owner仍AwaitBegin/未D；可在此处理失败事件或撤销actor。 |
| SI | D已apply一次，settle UPDATE或COMMIT前；实际queue已变且必须Processing(applied=true)。 |
| SA | settle真实COMMIT成功、回报尚未给owner；第二连接可见Settled，仍不发布Stable成功；恢复只读原结果。 |
| UA | startup真实COMMIT后抑制回报；service非Running，旧Applying实际Interrupted，须可靠核验才开放新动作。 |
| AU | audit append挂起；E已经释放pending，后续管理动作不等AU；关闭取消并join。 |

BI与BA、SI与SA分别单测，COMMIT前Err不能冒充提交后未知。每个hook一次消费；不用sleep推断先后，使用明确到达/放行屏障及第二连接SQL事实。C/D中间不可达，不计测试数量。所有拒绝前后比较必要SQL行及完整queue投影，不只检查错误返回。

## 正常、身份与语义绑定

| 子断言 | 操作步骤（实际API/SQL） | 预期结果 |
| --- | --- | --- |
| CL-104.REG01 | validate Submit X；register(A,T1,&mutation)，查询两张新表并inspect_queue；consume后再查 | 登记仅一registration无operation、queue空；consume可靠C/D/E后一个Settled.Queued、item=登记ID、revision1；无旧product_commands/product_runs插入，无JWS/期限输入。 |
| CL-017.REG01 | X在Registered/Applying/Settled三态同ID同意图重试；在BA/SA分别扣应答 | Registered首次只一次Started；Applying/Settled都不第二次D，Existing返原fact及本次重复body。操作ID不变、queue仅一X；未知响应不等于未发生。 |
| CL-017.REG02 | 两个service.clone并发consume同ID；两个真实repository连接竞争同登记（经with_repository包装同service/合法同crate夹具，不建第二owner） | 至多一operation INSERT及一次D；另一端原事实/等待，无第二lease。真实SQL PK/CAS和实际owner均观测，不能只mock计数。 |
| CL-018.REG01 | 同登记Submit分别改action/target/item/revision/digest；B冒用A登记；对照同A新generation当前有权的同意图 | 已授权且形状合法的意图不匹配为Conflict，无D；跨actor必须在当前身份/原登记意图边界拒绝(InvalidActor或Conflict)，不泄漏旧事实/正文，拒绝后SQL/queue不变。合同未指定两种安全错误的优先，不强加额外oracle。新generation同actor读原事实，不新apply。 |
| CL-018.REG02 | A提交X；B以新登记Edit X rev1→rev2；同编辑ID重试；再原Submit ID携新内容consume | B编辑保持X身份/位置/原submitter，editor=B；重试只回Edited(v2)，不升v3；原Submit意图不可改。 |
| CL-104.REG02 | consume规范形状但从未登记的随机ID、旧schema product_commands ID、已Retired ID；同时新register相同正文 | 前两UnknownRegistration、Retired按Retired拒绝；无INSERT fallback/无D。相同正文新随机登记可新操作，queue容量另判。 |
| CL-104.REG03 | 随机Unavailable与单次Collision封闭故障；登记COMMIT后丢应答，之后重新register同文 | 随机不可用零行；碰撞重取不覆盖旧行；丢应答可留孤儿Registered，新register产生新ID且不消费任何旧ID；不强加register invocation幂等。 |
| CL-080.REG01 | 自有端口夹具置retired=1并删除允许详情，另保Applying保护对照；query/consume/新register | Retired永不新consume，不以记录缺失授新资格；未知同样不回生；生产清理API本阶段不存在，真实清理not-run。不得删除受保护fixture模拟“安全清理”。 |
| CL-081.REG01 | 合成超过10000个已收敛登记；再register/consume；另一分支真实必要存储失败 | 可维护时无旧COMMAND_LIMIT/audit_slot门槛；不驱逐旧身份或责任。必要SQL失败才停相关新动作，queue三位独立；物理Full不能假称audit-only。 |
| CL-079.REG01 | NodeClock依次一天前/边界/以后、负i64与回拨；登记/consume/重试；检旧product_control | 时间只记录，不决定资格/排序/清理；旧epoch/floor值保留不参与新入口，仍无24h统一截止。真实一整天运行不冒领。 |

## 真实事务、故障与一次应用

| 子断言 | 操作步骤 | 预期结果 |
| --- | --- | --- |
| CL-075.REG01 | RegisterInsert/BeginInsert/SettleUpdate各语句前失败及对应COMMIT前真实回滚；另正常对照 | register失败无新ID事实；begin失败无Applying/D；settle前失败时可能已有D，保Processing/已知结果、禁止再次D和伪报Stable，不能把所有必要失败都称零内存变化。 |
| CL-075.REG02 | BA真实提交后ReturnUnconfirmed；另连接确认自身operation/runtime；node reconcile两次 | 尚未D时只在同owner且Applying(StoredApplying)完整匹配后恢复一次应用权，最后仅一item/编辑版本；不再begin、不重新验已受理原actor。 |
| CL-075.REG03 | begin未知但真实ROLLBACK/registered无operation，actor随后撤销；reconcile/重试 | 不沿旧授权D；没有已受理事实时须新当前授权begin。query/inspect不自动制造operation/lease。 |
| CL-076.REG01 | Edit D已v2，SA真实settle提交后ReturnUnconfirmed，reconcile及同ID重试竞争 | 查相同Settled.Edited(v2)，只清pending/发布原结果，不再edit到v3；旧body不落SQL。 |
| CL-076.REG02 | Edit/Resume D已apply，SI失败，inspect仍自身Applying；两次reconcile | 只重试同lease/outcome settle，不回ReadyToApply；后发生的可信失败/队列状态保留，不能借补记录再次resume。 |
| CL-075.REG04 | 对Applying(StoredApplying)的operation/runtime/意图/消费generation/begun_at逐项不匹配或真实读取不可用 | 保明确fault/未知范围，无D/无clear-fault副作用；错误事实不抢占真实operation。有效核验使用真实SQL事实；负向单字段篡改仅可信wrapper的显式故障分支，不计正常实仓储保证。 |
| CL-017.REG03 | 正常queue Full导致第一次consume稳定Rejected.Full；取消其他item后同ID重试，再新ID提交同文 | 原拒绝回执固定、重试不D；新ID可按新容量受理。响应returned保未采纳本次body，不藏第四项。 |
| CL-092.REG01 | register后、begin事务检查前真实SQL撤销A；对照BA已COMMIT后撤销A | 前者InvalidActor、无D；后者节点继续D/E，settle和私有probe不require_actor(A)。A新query/新动作拒绝，B有权可读/新编辑。不是WSS撤销集成证据。 |
| CL-092.REG02 | C后drop调用者Future/响应接收者；对BI/BA/SI/SA各停点分别取消等待 | 已受理由node-owned任务继续、最终只一次D；begin已发而COMMIT未知必须持有pending核验，不以drop猜未提交；取消者不能拿回节点已采纳body再重复执行。 |
| CL-094.REG01 | AuditPort Unavailable/Full/挂起/Cancelled，真实ledger始终可靠；对照必要表写失败 | 前者register/consume/settle照常稳定且take_audit_warnings可见，不伪报保存、无ledger fault；后者停止相关新动作，不以audit警告掩盖必要失败。 |
| CL-083.REG01 | AU挂起后继续第二个合法管理操作，再shutdown | E已释放调度围栏，第二操作不等audit；shutdown取消并join audit，报告audit_cancelled真实数量。90天/100000保留/实际清理端口未实现，相关原CL分支not-run。 |
| CL-084.REG01 | 必要状态故障期间/恢复后只提供可证管理事实，核验后query与reconcile | 不把补记管理结果冒称原native结果或故障时已保存；不按缺记录重跑。真实原生安全控制补记属后续发送gate，当前不伪造成功。 |

## 实际 owner、读取、driver 与关闭

| 子断言 | 操作步骤 | 预期结果 |
| --- | --- | --- |
| CL-087.REG01 | 同T1的两个service.clone及A/B屏障争第三位，BA/SI各暂停；不同T2另consume | 同T1同owner+三位，只一新项最终Queued另一Rejected.Full；pending中不提前成功/隐藏第四项。不同key可推进，queue等待不建Run。 |
| CL-087.REG02 | 同Storage第二次start_queue_service/with_repository；启动UA未核验时register/consume/driver.prepare；reconcile_startup | 第二工厂RuntimeAlreadyStarted；startup COMMIT未知返回Ok句柄且Unconfirmed，已知COMMIT前失败返回Starting；未Running新动作NodeStarting；核验后才能Running。随机/wrap在owned startup之前失败可释放本次claim，startup一旦发出就保句柄/claim；joined=false不释放。 |
| CL-087.REG03 | resolve_target返回旧native绑定，第二SQL连接在授权后更改目标实际source/native，再begin | InvalidTarget，不能把原正文移入新key/借第二owner通过；不得仅信调用者target别名。所有queue投影/必要行不变。 |
| CL-099.REG01 | SI暂停Edit/Cancel/Resume，inspect_queue与query_request；E后再inspect | D前Processing applied=false；D后Processing applied=true，当前暂定内容可借用且不Clone，整个视图不称Stable；仅E可靠后Stable，历史回执不等实时队列。 |
| CL-099.REG02 | 合法W Submit→consume→driver.prepare→begin_selection；随后Edit/Cancel W；另prepare X后Cancel/Edit X先D | local active动作返回稳定Rejected.DispatchStarted，不自动中断；准备期Cancel/Edit使token失效，旧begin_selection拒绝。driver无body/first-write许可，原生writer0。 |
| CL-099.REG03 | driver.prepare取E1；延迟SQL新操作pending时begin_selection；E后把environment换Blocked/E2再begin | pending时不开始选择；E后必须最新观测/Ready/epoch，Blocked/ResourceChanged精确原因，保原item/准备而不发送；不能用A/B旧Ready。 |
| CL-099.REG04 | shutdown同时挡begin/settle，测试截止前正常解除与deadline耗尽；关闭后新consume/prepare/begin | 起始短步停止新修改和调度，已提交责任尽力核验settle，joined与unresolved准确；joined=false仍持责任，不伪称所有任务结束。真正join后无晚D/E；不测试Windows进程树。 |
| CL-099.REG05 | 正常shutdown+重开Storage新service；用旧OwnedPreparation/ActiveKey/ExternalWorkKey到新driver | StaleToken/StaleObservation按精确层级拒绝，queue不变。禁止活owner任意替换；“旧SQL回报跨joined之后”若合法API不可达则静态防御分列，不造生产callback入口。 |
| CL-098.REG01 | Registered未consume、Applying已commit未D、D已发生settle未定、Settled各窗口终止自有owner/进程并重开 | Registered可按新当前权限首次consume；已consume旧runtime Applying→Interrupted、Settled保历史回执，但queue空且不重D/恢复body/专门丢失提示。管理Interrupted不称nativeUnknown。 |
| CL-098.REG02 | 新Submit/Edit canary，查询新表列/参数、main/WAL/journal/测试临时文件；旧schema正文用不同canary | 新正文/旧编辑全文不持久化；必要digest/回执不能恢复正文。旧正文原样保留，不能为通过扫描删除历史数据；无正文base64/压缩替代字段。 |

## 失败/继续与受信事件真实顺序

这里统一用实际service+SQLite+owner+合法driver。无RunW用open_external_work(T1,E1)取得opaque key，local W用正常Submit/consume和prepare/begin_selection取得ActiveKey；不伪造任何key。下列均原生writer0。

| 子断言 | 步骤 | 预期结果 |
| --- | --- | --- |
| CL-110.REG01 | X/Y等待，无RunW已open；new_d的BA应答被扣住；driver.observe(ExternalForegroundFailure{W})成功后放应答 | 失败先处理只挂X/Y，D之后加入为非挂起；live Blocked下Queued。事件不能等整个pending/E后再挂新D。BI、BA两个可达分支分别记录。 |
| CL-111.REG01 | W同前；new_d先D入第三位，SI暂停，再observe失败，最后E | X/Y/new_d全挂起；E稳定Queued不恢复。与上项构成反向oracle；C/D中间无await，不插测试yield。 |
| CL-112.REG01 | X/Y挂起，W可信工作；Resume的BA前先Fail、再D；对照Resume先D、SI时再Fail | Fail→Resume最终恢复；Resume→Fail最终挂起。历史Resumed数量与当前状态独立，同ID重试不得重新resume解除新失败。 |
| CL-112.REG02 | 受控local active new_d，X/Y挂起；Resume一次+重试；另D已等待时Resume | 不抢占active D、不复制；无active分支恢复顺序D/X/Y。原actor不重新授权，继续是B新登记/当前验权。 |
| CL-110.REG02 | Fail W先挂X/Y，new_d后来加入；再observe同W Failure/Completed；同E1合法open W2再失败 | 旧/重复W StaleObservation且new_d不挂起；W2新serial可独立失败挂当时等待。仅ResourceEpoch不能当单次消费证据。 |
| CL-110.REG03 | 无Run ExternalFailure在local active D+等待X/Y时处理；重复事件和资源变E2对照 | 只挂等待，不清/settle D，不返资源、不证明idle/停止；旧resource/owner/runtime key拒绝，完整状态不变。 |
| CL-110.REG04 | local W失败先于D，受控D后direct选中；driver.active_key(T1)取得D实际key；再次用旧W ActiveKey Advance/Suspend | 旧W StaleObservation，不结算D；active_key仅clone实际opaque key，无active返回None，不从历史DirectSelected元数据造key。 |
| CL-113.REG01 | 可信失败三旧项挂起，受控Ready同时consume新D/E（各已登记），依真实owner顺序处理 | 至多一个DirectSelected独立active，另一Full；等待始终旧三项，不清旧项/藏第四位；D settle回执未知时active仍围栏。明确这是受控调度算法，不是实际发送。 |
| CL-113.REG02 | 三挂起且environment Blocked，或已有active/prepare/可执行等待项；新输入尝试 | 不绕过阻挡，总等待满则Full；普通正常满额Ready仍Full；无writerlive绝不DirectSelected。 |
| CL-111.REG02 | X/Y挂起，新D Blocked占第三位，E Full；合法受控选D，Advance与Suspend各分支 | D先于旧项；Advance不恢复旧项；Suspend挂当时全部剩余，D不重入队。只记录调度决定，不冒称真实原生成功/失败。 |
| CL-112.REG03 | 上述事件交错BA/SA提交后回报未知及SI失败，再同ownerreconcile | 事件一次消费、管理D一次apply；仅补同outcome settle，不重放事件、不额外挂新输入、不清后来可靠失败。 |

新增纯 `suspend_waiting()->usize` 独立测试沿REG-QEXT01–07：返回**新挂起**数量(3→重复0→resume后再3)，空队列返回0但置失败模式；清prepare/priority，metadata/正文不变；保local active及token；settle(Suspend)复用但旧token一次消费。原39不改源码、仅同新快照回归，极值无生产setter，公开动态不可触达的MAX分列开发/静态证据。

## schema5 / SQL精确资产

| 子断言 | 步骤 | 预期结果 |
| --- | --- | --- |
| CL-004.REG01 | 合法0/1/2/3/4合成副本分别Storage::open→5，再重开5 | 两新STRICT表+查询索引与固定DDL一致，已有meta和pragma同5；无node_meta空库仍无身份。全部旧表/索引/行字节保留，仅schema_version按合同改；不导入旧command/正文/Run。 |
| CL-004.REG02 | RegistrationTable/OperationTable/RegistrationIndex/RegistrationMetadata/RegistrationPragma/Validation/Commit各故障，叠旧迁移故障点 | 整个BEGIN IMMEDIATE原子回滚，无半套5/混合meta；真实COMMIT后未知重开可识别5，不能降库。每起点适用故障点记录，不重复计有效用例。 |
| CL-004.REG03 | unknown77、5号头但缺表、0–4头部带部分/完整新表、旧定义被改、新表定义被改、身份历史缺meta | 按既有UnknownSchema/UnrecognizedSchema/MissingNodeMeta语义拒绝且不改库；不能把5缺表说成UnknownSchema5。WAL-only已提交事实必须被预检看见，不用immutable。 |
| CL-004.REG04 | 新表STRICT/FK/PK/UNIQUE/CHECK：错误ID长度、错列型/8字节revision/32字节digest、action形状、phase/result形状、重复op/request、失联actor/target | 真实SQL拒绝，未半行；有效revision0/1/u64::MAX按8字节大端往返，不截断i64/JS安全数，节点时间i64含负值。adapter还校验canonical ID，而非仅length。 |
| CL-018.REG03 | 用封闭真实repo写/settle动作结果交叉表；另以合成SQL造DDL形状合法但语义矛盾行再query/inspect | Submit仅Queued/DirectSelected(item=登记ID,rev1)；Edit仅Edited(item原绑定,rev=expected+1)；Cancel/Prioritize保expected；Resume仅0..3。写错InvalidIntent、读错CorruptState，不借正文猜回执。合法Reject集合逐动作完全核对；Busy等不可稳定落库。 |
| CL-082.REG01 | startup打断旧runtime Applying，保留Settled/Registered；当前runtime不打断；StartupUpdate/COMMIT前后故障及UA | CAS只中断旧Applying并保原operation/intent；未可靠startup先不Running。COMMIT未知核验后完成启动，不从旧正文seed；节点时钟回拨不锁资格，旧floor保持。 |

## revision2 端口映射及执行资产安排

### 公开真实SQL + 实际owner目标 P

使用 `Storage::start_queue_service_with_repository(authority,validator,clock,environment,audit,wrap)`。`wrap`收到该Storage真实SqliteRequestRegistry，返回实现RequestRegistryRepository的受控包装器；两个启动方法共用一次claim。包装器默认原样转发全部方法及完整返回事实；目标方法的**真实inner.await成功之后**才执行AfterCommitPause/ReturnUnconfirmed。正常分支和故障分支分别留日志/SQL证据。这样BA/SA/UA均可外部integration执行，不获得私有grant或库cfg(test)。

- `CurrentRequestActor::{context,target,native,action}`、Lease/Probe的registration_id/operation/runtime/intent只读getter用于逐字委托，不构造grant。`RegisteredIntent::new`和`RegistrationFact::new`仅作合法元数据/负向形状验证，不凭构造结果绕过repository。
- Applying明确为 `RequestFact::Applying(StoredApplying{registration,operation,runtime,admission_auth_generation,begun_at_ms})`；比较真实读出的全部坐标。reconcile只能消费该owner“未D”的pending一次；已经D的状态只能补同lease/outcome settle。read与inspect都不能返回第二次apply权。
- 测试可读 `Storage::request_registry()` clone。真正两连接CAS竞争若需要另一repository实例，用同crate隔离资产建立第二真实pool并走同repository实现；不能为此再启动第二QueueService绕过唯一claim。公开P目标仍有service.clone并发及独立pool已提交事实观察。
- 公开 `RegistryDeadline::new(tokio::time::Instant)`、cancel/deadline/is_cancelled合法构造关闭/取消测试。工程deadline不持久、不使登记到期。
- `driver.active_key(target)`取得DirectSelected或begin_selection当前合法ActiveKey；无active返回None。旧/重复key通过observe拒绝，无构造器、无序列化、无正文或writer权限。
- 跨actor/错误目标分支至少固定“拒绝、无事实泄漏、SQL与queue不变”；合同没指定同一安全拒绝的完整错误优先时，不为迁就实现换预期，也不强加一个未定义码。合同明确的UnknownRegistration/Retired/Conflict、driver Blocked/ResourceChanged/StaleToken和StaleObservation逐类核对。具体私有hook类型名由受审实现提供后按同一封闭位置接入，不把它改成产品状态setter。

### 隔离同crate真实故障与迁移目标 F/M

外部integration crate不编译依赖库的cfg(test)，因此**不假定P能拿到内部故障控制器**。独立tester在固定源的隔离副本生成两个受控测试覆盖：

1. 在新 `session_storage/request_registry.rs` 末尾仅追加一个 `#[cfg(test)]` 的独立测试模块声明，模块文件在tester资产目录；独立资产为该模块的子模块，可调用冻结实现中的封闭RegisterInsert/BeginInsert/SettleUpdate/StartupUpdate和Commit前故障。测试通过实际SQLite事务核对回滚/断连，不使用调用前wrapper Err冒充rollback。
2. 在 `session_storage/schema.rs` 对应cfg(test)范围挂入独立迁移模块，使用原MigrationFault及新RegistrationTable/OperationTable/RegistrationIndex/RegistrationMetadata/RegistrationPragma/Validation/Commit封闭点；原DeferredCommit/CommitUnknown保留真实语义。随机Unavailable/单次Collision同样只接封闭cfg(test)测试入口，不在生产暴露任意id/random setter。

若实现将封闭控制器放在别的同crate测试模块，只机械调整覆盖声明位置/路径，记录精确overlay，重新核对无生产行为差异。隔离源清单区分“原生产冻结文件字节”与“仅cfg(test)模块声明的测试overlay”；保留patch、前后SHA及逆向证明，不能把覆盖副本冒充逐字未改的根源。根产品目录不写。

预计命令按实际目标名冻结为P的独立integration目标、F/M的`cargo test --lib <独立模块过滤>`。目标名和模块路径属于实施布局，不能先宣称已经可编译或测试已启动。每目标检查非零实际执行数与断言，只有定义不记pass。受限环境/故障接口缺失时相关项未执行并阻断本阶段必需验收，不改成LX或mock成功。

### 旧迁移资产当前版本候选 C

tester单独复制schema3_migration/schema4_migration及其support到外部版本化候选；保留原文件和历史报告SHA。当前Storage终点(4,4)/pragma4改为5，旧fixture1–4、PINNED_PRODUCT3、10/11张product表计数和delivery约束不动；追加node_request新表空/无旧command转换断言。reviewer逐hunk核对版本适配，不能由coder改旧oracle、删除/ignore原测试来过关。实际在固定schema5源运行全部原适用断言及新增M用例，单独记录数量，不和39纯queue重复计数。

## 当前定义状态

REG-API-G01/G02/G03的v1接口缺口已在revision2具备可执行路径；G04的deadline/启动claim规则也已明确。剩余测试代码布局/封闭hook类型名可在实现冻结后机械映射，不能改变本文件的故障时点与oracle。没有新增产品行为待用户决定。此为tester的定义可执行性判断，方案放行/问题关闭由独立reviewer负责。

未实现/运行：所有P/F/M/C新目标、MAX独立私有触达、真实WSS/ACP/writer/Harness及Windows树；不把stage可编码定义写成实际能力。
