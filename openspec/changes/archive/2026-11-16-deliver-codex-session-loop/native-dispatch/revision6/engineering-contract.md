# NativeDispatch revision6：实际所有者、接口与必要事实合同

2026-11-01。状态：冻结工程候选，等待独立 tester/reviewer 同版预审；**不授权编码、SQL 执行或 Node/WSS 接线**。本目录只写设计与只读输入快照。revision4 的完整合同由本revision替代，82ca与a3b4冻结字节保持历史，历史文件不改。本文、interfaces.md、schema6-native-dispatch.sql、verification-contract.md、resource-ownership.md、lifecycle-ports.md、closure-contract.md 合起来是完整合同；不能抽取单张表绕过其它条件。

## 1. 范围、输入与已知实际实现

本版输入为inputs-v5.json固定的最终Root：before清单fa004a9042c0ade9ba835a3f061f0568707c40d354f2f83b4d472f4464aeed88，281核心/306总项实际逐字节全匹配，before/after entries完全一致。独立Root组合报告beb7675f33701b3c5249f9cff97f6b88f6e302a4fd614836f0e519705b17f0ba限定PASS，六阶段工程结果证据84722eed…另固定；这只证明已有ACP/R3/SQL组合，不证明新NativeDispatch。对v4共有路径5项变化和36个新最终路径逐项记录，旧故障writer/248仅历史。v4 freeze82ca/19成员原字节未改，新的NDTv3/004正式review报告已固定。plan/角色/任务另以planning-context.json记录，plan结果追加不混入306源清单。作者只做字节/文本核验和设计，无编译/SQL/产品执行，无依赖或锁改动。

真实定位（均可在快照复核）：

| 当前实际位置 | 事实与此次工程增量 |
| --- | --- |
| app/src/storage.rs::start_queue_service_with_repository | 一次 queue_runtime_claim，在首 await 前保 QueueService supervisor；目前只回 QueueService/NativeQueueDriver。本次新增组合 factory，保同 claim，不能再开 runtime |
| application/queue_owner.rs::RuntimeCore/OwnerState | 一个 NativeSessionKey 对应一个 Arc<OwnerCell>；pending 是管理 B/D/E 状态，active 是局部选择；现无来源 request 账本、native task/source registry |
| queue_owner/service.rs::apply | Submit/Edit 的 D 短锁更新正文/revision/editor；来源元数据须在这一个 D 更新，绝不靠时间追索 |
| session_storage/request_registry.rs | SqliteRequestRegistry 独占 pool+Arc<ConnectionOwner>；begin 在 acquire await 前取 lease，detach 的 SqliteConnection 由 RegistryConnection 接管 |
| registry_connection_owner.rs | lease/close-slot 相同短锁交接；close task 无 timeout，记录 Confirmed/Unconfirmed；取消等待仍保 Arc 槽和物理关闭责任 |
| adapters/acp/transport.rs | Reservation.id 是 Option，控制消息可无RPC；EncodedFrame.bytes 可读；Ticket.wait 是终态 WriteReport，不可 Unknown 后又 Written |
| transport/writer.rs::Proxy::poll | 唯一真实 W.poll 在 Core/ProtocolState 短锁内；此次连续观察检查必须插在同锁同次实际 poll 前 |
| transport/reader.rs/state.rs | 实际 read 完整 LF 在 capture 建 held ReceiptCut；emit 分配整连接 sequence；当前无应用 ack 模式 |
| domain/activity.rs | finish_work 产生工作结果并清 work/input；v2 聚合 idle 不得直接调用；需单独 release_inserted_slot |
| node.rs / clients/web | 尚未接本新ACP；现UI仅管理，本文不把源码存在当产品功能 |

本 loop 包含真实安全准入、唯一原生目标、当前完整配置/资源归属、必要记录及显式安全关闭。历史导航/默认容量自动回收归其它 change，不能因此豁免 Windows 完整 Job 树或共享实例隔离。正文/输出/raw/未知结构保内存，不加正文、实体、累计请求、TTL 限制。原actor只追溯，派发不查当前trusted/撤销，不恢复旧JWS、截止或time_floor。

ND-SEM-001用户原答“保留结束原因，暂停旧队列，手动继续（建议）”已同步到本次最终捕获的spec d2c38379、design dea6d897、implementation cc4f9ef3；主tests123为0561cfab。v3原semantic-rebase/supplemental证据保留在历史目录，v5 inputs-v5直接核最终权威字节。新增7个主定义及本轮80个NDT定义仍未执行，产品Pending解除不等于产品验收或工程门禁通过。

## 2. 模块和数据

- domain/native_dispatch：body-free 的 ID/计划/记录/错误及协议无关枚举。
- application/native_dispatch：唯一线性 Job/Running/SelectedContext、必要缺口、观察pump、工厂组合 supervisor；与R3共享实际 RuntimeCore/OwnerCell，不能建立第二队列。
- adapters/acp/native_dispatch：实际 Reservation/Frame/Ticket 的薄绑定、实际观察lease与小事实分类；不写SQL、不补造 Received。
- session_storage/native_dispatch：schema6真实仓储；通过 SqliteRequestRegistry 内部受信方法取得同pool/ConnectionOwner，不建立第二closer。

DispatchPlan 完整不可变字段：dispatch_id、planned_run_id、canonical_target、NativeSessionKey{source,native_id}、runtime_epoch、owner_id、instance_id、resource{connection_epoch,load_epoch}、item_id、revision、submit_request_id、content_request_id、content_digest、config_identity、config_revision、selection_kind(Waiting|ExistingActive)、prepared_at_ms。planned_run_id 预分配但未写出不表示执行；不插旧 product_runs/commands，不把排队占活动Run。

内容来源由 OwnerState 新增私有 HashMap<ItemId,ContentOrigin>：ContentOrigin{submit_request,content_request,revision,digest}。成功Submit D建立，成功Edit D原子替换content_request/revision/digest，保submit_request；拒绝不动，成功Cancel/最终释放移除，active生命周期仍保来源。来源ID来自当前 OperationProbe，digest来自当前 ValidatedBody，不能调用客户端setter。E(必要 settle)未可靠时已有 pending 阻捕获。元数据与队列同锁同D，不使用大正文Clone。ValidatedBody的 RetainPrompt只返回不可变 Arc/raw引用，O(1)，不要求Body:Clone。

SessionBinding<S>是只读数据：actual scope S、canonical target/key、runtime/owner/instance/resource、wire、完整可信配置引用{identity,revision,setup Arc}。setup涵盖cwd/MCP/additionalDirectories/unknown扩展；未知或不完整只能限制需要该参数路径，不发送空集冒保持。SQL仅存opaque identity/revision，不能从它们恢复setup或写资格。

## 3. 一次claim的唯一生产/验收构造路径

接口详见 interfaces.md。新增 Storage::start_native_queue_service_with_repositories，保现 queue_lifecycle/queue_closed/queue_runtime_claim：

1. 在同 lifecycle 短锁内取唯一claim，生成RuntimeEpoch，先从 self.request_registry() 同实例派生 SqliteNativeDispatch；受信wrap分别包真实两仓储。wrap失败且未注册任务/IO时释放claim。
2. 构造原 QueueService/NativeQueueDriver；创建 NativeCoordinator，用同一个 driver.core Arc，不从target重造队列。内部 RuntimeCore 新增 supervised native/source任务槽及 necessary状态；组合 NativeRuntimeSupervisor 持queue+coordinator。**首await和首IO启动之前**存入Storage原queue_supervisor槽。
3. 注册startup任务到同Arc槽，按原R3 reconcile必要事实，再interrupt_previous_runtime，联合状态可靠才Ready。服务句柄可Starting/Unconfirmed，读取/安全关闭仍可进行。不能仅queue Ready就准native写。
4. factory等待方Drop不取消节点任务/释放claim；Storage仍持组合supervisor。原仅队列factory与组合factory争同claim，不能同时启动；旧仅队列行为保持BlockedEnvironment，无新真实派发。
5. 返回 NativeRuntimeHandles{queue,driver,native}，native是可Clone命令/查询句柄，不能取得Job/Ticket/grant。runtime创建后，经受信 NativeSourceInstaller装配源。SourceInstaller实际就是NativeCoordinator<P,L,G,N>的受信内部流程，完整关联端口及生产类型等式见lifecycle-ports.md，不能由未来Node传bool或泛型内downcast补方法。

受信源的真实配置/所有权/Loaded证明完整产生路径见resource-ownership.md（同阶段实现，不能假定已有）。受信源装配是同步 **prepare→adopt→start**：新增ACP Connection::prepare_observed经adopt回调先将唯一IoOwner交已登记SourceSlot（拒绝必须返原IoOwner，adopt前没有IO任务），返回(ObservedStart,Receiver,Acknowledger,Gate)，内部Core/tasks处于started=false；没有任何read/write poll。应用把receiver/ack/gate move到同SourceSlot后，最后consume唯一ObservedStart.start()开门取得Connection；start无分配、无失败、无await。ObservedStart不可Clone，Drop只stop；安装失败SourceSlot仍保原IoOwner直到shutdown/join。不能在已运行spawn后设观察模式，也不能以Drop冒join。完整签名见interfaces.md。

SourceSlot 先注册owner实例/可信binding及取消责任，后start；start回的connection/receiver/ack/gate由同slot保留并启动单pump。reserve仅接无Scope的PreSourceProbe/PreResourceProbe；prepare后SourceSlot一次绑定actualscope并可靠保存scope_binding_id，再开门。配置/资源reserve及实际initialize/resume+写终态+必要资源记录均可靠后才产生VerifiedLoadedBinding（见resource-ownership.md）。源映射 `(actual Scope, sessionId)` → CanonicalNativeKey/Arc<OwnerCell>/当前ResourceBinding 在BoundResource建立且enqueue_resume之前同步建立（仅观察路由，无Loaded/写资格）；Loaded证明完整后才升级Ready；pump不 await RuntimeCore.owner。未绑定的会话事件走§6 source quarantine，后台resolver可await但不阻reader。

SqliteRequestRegistry新增仅crate内部 `native_dispatch_repository(&self)->SqliteNativeDispatch`，clone相同pool/connections Arc；对外wrapper不接收任意pool或closer。测试wrap仅转发该真实实例，不能为方便另建SQLite假仓储。所有new任务经RuntimeCore::register_native_task在spawn之前登记稳定Arc<TaskSlot>（门控启动）；running/stopping与注册同短锁，Stop后不许偷渡任务。任务JoinHandle永不take到可取消局部Vec。

## 4. 唯一Reservation/Frame/Ticket与选择重试链

准确端口见interfaces.md。PromptReservation是私有薄包装，构造仅在真实 reserve_request(Prompt)后检查 `id=Some`、kind=Prompt(session)、scope相等，并保存该typed ID的只读副本；失败返回原Reservation安全取消。它拥有原Reservation，禁止Clone/Deserialize/从SQL构造。Frame是同一 EncodedFrame；EncodedPromptFrame::byte_len_checked仅checked usize→u64，包含最终LF，不重编码。Ticket是实际 WriteTicket唯一move，probe仅只读。

Waiting链：prepare_capture(原OwnedPreparation+完整plan+O(1)retained) → SQL reserve可靠 → 新Job在owner锁外reserve/encode → select_reserved同owner短锁核记录/token/版本/current资源/pending/必要缺口，queue.begin_send恰一次D → SelectedContext。Direct链仅在原DirectSelected且E稳定时capture_direct，不重D。两个链之后共用arm/enqueue，不增第四项。

SelectedContext不可Clone，持原ActiveKey、DispatchToken身份、完整ContentOrigin/plan、retained正文、原resource/config绑定、SelectionStamp{selection_id,owner_order}。Job在select前持PreparedCapture，在select后持SelectedContext；enqueue成功move该SelectedContext与唯一Ticket到RunningDispatch。owner中仅保只读绑定/TransportPending，不复制任务所有权。

arm事务将prepared_zero→selected并写SelectionStamp和当前attempt **同一事务**（§5）；SELECTED指持久必要元数据已确认，不从SQL铸原内存选择。arm提交未知：Job仍独持NeverEnqueued+真实Reservation/Frame；inspect完整一致后仅继续原一次enqueue；无原Job不能恢复写。任何失败不把selected项放回waiting。

实际enqueue只consume原Job一次，失败返同Job和原Reservation/Frame；不得将enqueued状态重置NeverEnqueued。Running持唯一Ticket及SelectedContext，普通请求等待方Drop不影响它。早RPC/idle只更新native轴，**不清active/TransportPending/SelectedContext**；直到实际Ticket终态、必要事实可靠、当前前台/插入/资源条件满足才释放。后续flush仍用原绑定，不能让自己的早响应使最后围栏失效。

ND-RETRY-001入口：`RunningDispatch::into_known_zero(self, StoredAttempt)->Result<SelectedRetry,RunningRejection>`，仅当本同一真实Ticket.wait已经终态Zero、持久全字段同一Zero可靠、没有晚poll可能、active/version/resource/config/owner实例仍当前，才消费Running把SelectedContext交给SelectedRetry。SelectedRetry::next_attempt(options)在锁外调用真实reserve/encode产生新RPC/attempt，不重新D、不回队列、不调用capture_direct、不换planned_run_id。旧Reservation/Frame已被实际writer消费且Ticket终态，不能复活；任何Unknown/Written/Pending、SQL未知或资源变化均不mint重试。重试选项是可信调度器一次真实IO控制，不是自动刷新正文寿命；Cancel/Stopping意图未撤销时不可重试。若不重试，走经证明zero的close/active释放，不能静默重复。

## 5. 真实SQL事务、codec与证明

每个方法均通过同ConnectionOwner lease→acquire.detach→RegistryConnection；BEGIN IMMEDIATE内核完整字段，COMMIT事实与close结果分开。没有owner/ProtocolState锁跨SQL/编码/大parse。

| 仓储方法 | 输入 | 事务内精确行为/返回 |
| --- | --- | --- |
| reserve | DispatchProbe, context, deadline | 核已settled Submit queued/direct_selected的target/item及已settled最新内容Edit的item/revision/digest；首次content=submit。相同dispatch_id比较所有plan字段返回原记录，closed不重开slot；异字段Conflict。新insert prepared_zero+唯一target slot原子。无需actor当前授权 |
| arm | AttemptProbe, SelectedProof, context, deadline | 原slot及全plan一致；SelectedProof必须来自本Job同D，SQL仅接其字段；prepared_zero时原子update selection fields/lifecycle+insert armed。selected时核同SelectionStamp；新attempt必须之前全为Zero；同attempt全字段幂等。current selected不是仅查询行能证明 |
| record_transport | AttemptProbe, TerminalTransportProof | 仅本实际Ticket终态 mint proof，armed→zero/unknown/written；重复同终态全字段幂等，任何异终态Conflict。SQL COMMIT确认丢失保持账面armed，inspect再补同终态；**不允许真实终态unknown→written** |
| record_observation | ObservationProbe, MetadataBatch | 小批完整原子保存；全连接observation无target要求，事实逐行有真target/owner/资源。相同(conn,sequence)/observation id必须全字段等，fact_index从0连续。每attempt事实须核其dispatch的target/source/runtime/instance/conn/load；相反/异批Conflict |
| close | DispatchProbe, ReleaseProof | 锁同target slot；只有精确NeverEnqueued或真实TerminalZero或VerifiedConvergence证明可closed并删原slot。native_settled还要求Ticket终态+必要native/transport可靠；资源收敛须当前owner/资源停止证据，Unknown结果不改成功。同闭合重复返回原记录，异close原因不能重开 |
| inspect | DispatchProbe | 返回完整StoredDispatch(全plan+selection/close)、按attempt身份完整StoredAttempt、完整metadata事实、当前slot等价；不授写能力 |
| interrupt_previous_runtime | old runtime, StartupProof | 独占节点启动/旧runtime已无写任务证据下：prepared_zero无attempt→restart_zero并删slot；armed→transport unknown/restart，written/zero终态不改；selected unknown slot保留责任。无正文/句柄恢复、不重发 |

DispatchProbe持plan+私有owner Arc identity，本私有捕获方法mint；AttemptProbe再有attempt_id、实际scope、wire/typed RPC/frame_len、SelectionStamp；ObservationProbe来自真实Lease及实际binding，含observation_id、actual scope、source/runtime/instance/conn、sequence/receipt_order、节点observed_at、元数据批指纹。它们可读字段但不可公开构造/反序列化。SelectedProof/TerminalTransportProof/ReleaseProof不可Clone，仅相应实际状态转换mint；仓储wrapper可转发不能伪造。

ReleaseProof三个分支：NeverEnqueued{same dispatch/selection, actual job nonce, enqueue_count=0, reservation cancelled/dropped或原对象返还}；TerminalZero{same attempt, actual terminal proof, exact storedZero}；VerifiedConvergence{same owner/scope/resource, native input settled或资源停止证据, actual IO终态/真实join结果, exact StoredFacts集合, necessary gaps空}。需要的所有原资源仍同实例核验。数据行不能mint任一种proof。prepared失效关闭只移旧SQL slot，不取消后来新版本正文；selected关闭才以同ActiveKey安全释放协调位。

StoredFacts：observation header全字段、顺序完整fact数组、batch_fingerprint、各requirement_id及对应metadata fingerprint；构造函数验证全量相等。DispatchInspection完整返回上表，不只bool。StoredDispatch/Attempt是数据，任何重新写资格须本地私有对象+同Arc验证。

codec：EntityId严格现22字符decode；u64为exact8字节unsigned BE，读取长度/类型不符CorruptState，禁止SQLite signed cast；u64排序只用相同8字节BLOB。RPC integer为i64::to_be_bytes精确含负数并核当前协议合法范围，string为原解码UTF8字节（不JSON引号/Unicode规范化）；类型标签不合并数字/同形string，拒绝错型/非法UTF8/长度。frame_len为同原frame含LF长度>0；Zero accepted=0、Written accepted=frame_len、Unknown accepted≤frame_len，Err0仍Unknown。SQL NOT NULL/CHECK/FK保护结构；仓储负责全plan/来源/跨表/当前proof语义，坏记录不容错补默认。错误只typed code，不带raw/配置/正文。

## 6. 连续确认、目标路由与必要缺口

Observed模式的Lease唯一，不Clone；ack/gate可Clone但只能真实Core产生。所有合法Received，包括无target事件都必须经过同单pump，按sequence连续同步apply后ack；不能把无SQL写等同漏ack。raw先共享不可变引用给非持久视图路径，显示/parse延迟不挡小事实。pump的大JSON结构提取在owner锁外；同步apply仅小字段，不能await SQL/用户/reader受命令锁。

| Incoming完整分类 | 同步apply与真实路由 | 必要SQL/ack |
| --- | --- | --- |
| Response::Initialized | 由真实(scope,typedRPC) operation registry核初始化，更新wire/capabilities，非任意target | 无dispatch事实；初始化的资源必要状态由其负责记录，先立相应source gap再ack |
| NewSession | 原创建operation绑定来源/资源，把返回原生ID交可信target resolver；不造prompt Run | 创建必要事实由创建owner管；无target未解决时source quarantine，不能用假target写dispatch SQL；仍ack |
| Resumed/Replayed | 原operation精确绑定session/config；独立结束resume/replay窗口，不结束Run | 必要加载/资源事实进对应gap；显示重放完成分轴；ack |
| Sessions | 真实list operation，保原listing/raw，不能按列表当执行状态 | 无dispatch SQL；ack |
| Closed | 原close operation+精确scope/session/resource，先封相关新写；返回不是全树已关证明 | 必要资源事实入gap，审批按真生命周期；ack |
| Prompt::Completed(v1) | `(scope,RpcScalar)`→唯一实际attempt，保stopReason；未到FrameAccepted的伪响应仍由ACP拒绝 | 原attempt结果必要事实，立gap再ack；不清未终态Ticket |
| Prompt::Inserted(v2) | 同上精确attempt与messageId；插入维度独立，不能“最近Run” | 必要插入事实，立gap再ack；保早idle及原TransportPending |
| RemoteError | 同RPC operation，prompt只记原输入拒绝/方法错误，资源/前台不猜无副作用 | 关联actual attempt时必要事实；其它归对应operation；无造target；ack |
| Permission | 使用原真实PermissionRequest scope/session/typedID/广告选项。当前有效请求存在先同步保护该资源，即使无Run/无前后台细分 | 必要存在/失效元数据入gap；批准决定仍另阶段严格保存；不等用户或裁决再ack |
| Update::State | 由scope/session映射实际owner/resource。v2 running与requires_action均Active阻写；idle按§8；未知state立即Unknown阻写，不沿用idle | 保存body-free原状态类别、known reason及transition/interval；raw保未知原值。状态小事实必要，立gap再ack |
| Update::Tool/Message/Other | 原target共享原结构/raw；不从工具文本猜foreground/Run/permission | 通常无dispatch必要SQL；若解析为独立可靠资源/审批事实按其负责者先apply/立gap；ack |
| UnsupportedRequest | 原scope/request交安全unsupported通路，仅一次真实响应；unknown方法不宣称支持 | 无dispatch target，ack；其响应不被Prompt barrier自锁 |
| UnknownNotification | 保原raw/扩展。无已支持调度语义不推断成功/idle，不重造target | 无必要dispatch SQL；ack。若含无法确认的目标/状态扩展只是显示未知，不让未知值覆盖真实状态 |
| receiver Err/None | 非Received，无sequence可ack；同步source停止/所有绑定目标Unknown、封新写、失效旧真实审批范围 | 从实际P::next Terminal mint独立SourceLossProbe：header及真实目标小批入gap，无sequence/receipt_order，record_source_loss单独保存；Loss只证Unknown，不能作为任何Convergence/Release证明 |

合法事件有session但未绑定时：source记录UnresolvedSession{scope,session,sequence}，该源全部Prompt暂封（影响范围未知不能猜单target）；raw保留，ack仍前进，受信后台resolver核配置/原生唯一身份后将小事实重放到真实owner并生成必要记录。不能将该事件标成已处理目标后丢弃，也不能从外部session字符串造新Run；只在所有unresolved均解释/安全收敛后解除该source quarantine。错scope/目标映射矛盾立即source隔离并停危险写，保原始片段及安全收敛；不以伪target满足DDL。

必要缺口采用集合而非布尔：`RequiredGapKey{runtime_instance:Arc identity, owner_instance:Arc identity或source slot identity, actual_scope, resource, generation:u64}`；每次已apply的必要批在ack前分配独立RequirementId、冻结metadata fingerprint并插入Pending集合。持久worker取该ID+原批，不在owner锁内SQL；A/B可并发。成功只消费同key+id+全字段StoredFacts匹配的那个缺口，不能清同target其它缺口；同ID失败/提交未知仍Pending。乱序成功允许，但只有所有当前资源和未收敛前资源的必要缺口为空才允许相应危险写。重试相同批/ID不能铸新事实或绕过失败。

scope/resource替换创建新generation，旧worker成功只更新旧集合，不改新集合；旧责任未核销仍以资源归属挡新source。counter溢出是明确StorageUnavailable封相关写，不加人为数量阈值。inspect找回原批并逐字段相等才能确认该缺口，查询成功本身不清任何项。raw/显示/分页/附加audit永不进入此必要集合。故障期安全拒绝/可验证cancel/close可独立继续，其未保存必要事实留自己缺口，不能因“安全”虚报落库。

supported_subject=false不使真实Permission失效；未知主题/option kind只限制语义自动判断，人工原optionId选择仍按当前规格与唯一决定持久边界，不能加unknown禁选门。

无target的Received可只应用source/operation事实后ack，不要求每sequence都有SQL行。node_native_observations以connection+sequence唯一，但它只存有必要批的sequence，可有数字间隙；该SQL表**不是持久ack frontier**。连续ack的唯一权威是运行中的实际Core，重启不从SQL最大seq恢复写资格。

## 7. held LF与每次真实poll的原子门

观察状态在startup门前建立：routed_sequence=0、acked_sequence=0、outstanding lease map、held complete帧使用原ReceiptCut。旧普通enqueue在Observed连接上拒绝Prompt并返原Reservation/Frame；Connection clone也不能绕过。enqueue_observed核gate为同Core，仅将private dispatch fence和same gate绑原Command。其他request/control仍有各自围栏，不扩大授权。

锁序固定：owner state → 当前资源短guard → ProtocolState（Core.state）；reader只进ProtocolState，不反向回调owner；invalidate先取owner再资源，不持资源锁回调owner。应用同步apply释放owner后ack只进ProtocolState。所有目标短锁由节点固定native key顺序拿取；通常每事实单目标；单Received多目标只连续短步，必要批原子SQL在后。

最后真实Proxy::poll内持**同一个**ProtocolState guard：先核真实scope/gate、cancel/deadline/closed、`acked==routed && held.remaining==0`，再恰一次W.poll_write/flush，再更新accepted/FrameAccepted/terminal。中间无await、无释放guard、无回调reader。reader实际poll_read/capture同锁，故cut只能先于检查或晚于本次真实poll，不能在二者间插入。

当前Job的写资格与“下一条可调度”分开：同SelectedContext一旦FrameAccepted，早RPC/idle/自身running只是原输入事实，不以它们清active或把该job自己的flush判NotReady。真实scope/resource/config失效、Stop/cancel仍按实际范围停止；必要状态worker确已失败可停止相关危险IO并保Unknown范围。新小事实正在可靠保存期间既有同Job可完成原flush，但不得授下一条enqueue/首次写；全新的派发始终要求必要缺口空。这个例外只收敛已被FrameAccepted的同原帧，不放行尚未开始的新输入或改变的正文。

未追上：先可靠注册Waker/Notify（检查在登记后重读，避免丢wake），返回Pending；外层合法fence仍只调用一次PollIo，但实际W.poll为0次。保同cell/Ticket/attempt/正文，不报Zero、不重D、不续命。ack、emit、frame_done、cancel/stop均wake；多帧批必须每帧route或明确拒绝/terminal后释放held，不因pop事件清held。仅完整LF视为已取得，OS尚未poll/半帧不冒事实；真实大parse在held期间仍阻Prompt。任何leaseDrop未ack令该source进入observation fault、stop危险写，不自动补ack。

安全cancel/permission拒绝/unsupported/close不依赖Prompt专属frontier，但必须仍核其实际请求/目标/资源作用范围；旧cancel不得打新work，不能借安全路径发送允许或新Prompt。

## 8. 原生前台、输入确认与用户已确认的停止映射

独立内存轴：InputDelivery{attempt,awaiting_insertion,native_response}、TransportPending{selected,ticket terminal/stored}、SessionForeground{Unknown|Active|Idle,raw_state,interval,last_transition}、后台/审批/资源轴。requires_action是合法Active，缺Permission仍阻新派发；raw state与unknown reason都保留。

v2 aggregate idle不得调用SessionActivity::finish_work。新增纯协调 `release_inserted_slot(&WorkToken)->Result<PlatformInput,ActivityError>`：仅当前input已确认插入、当前foreground可靠Idle、对应native/transport必要事实可靠且Ticket真终态时清input/work，不写ForegroundOutcome或Run成功。无cause关联时Run保持Unknown/原生结果未证明；可解除协调阻挡不等于成功。v1只由原RPC结果可靠关联原input。

UpdateOrigin DuringResume/DuringReplay仅receipt窗口。初始Unknown→idle、窗口内snapshot、重复idle及未知reason只更新上下文，不生成新的成功/失败/停止、不开/完成Run、不重复挂起新D。已有真实Live running/requires_action区间→Live idle且known reason才生成一次前台终止；本地ExternalWorkKey仅防重复消费，不证明属于某Run。running快照可建立防重复区间但快照内旧stop不当新终止；后续Live状态必须核真实过渡。新running覆盖早idle后，迟insert只解该输入插入未知，当前Active仍阻下一条。

| 真实协议依据 | 精确事实/Run | 正常旧队列处置 |
| --- | --- | --- |
| v1 原Prompt RPC end_turn | 原input可靠成功 | 其它围栏满足后Advance |
| v1 原RPC max_tokens / max_turn_requests / refusal | Stopped(原reason)，不改Success/Failed | Suspend当时旧等待项；显式继续恢复 |
| v1 原RPC普通cancelled | Stopped(cancelled)，不声称零发送/副作用回滚 | Suspend旧等待项；显式继续恢复 |
| v1 RemoteError | 方法错误/可能副作用范围保留，可靠input拒绝不自动证明整个前台失败 | 原生前台可证failed才Suspend；不可证保Unknown核验，不伪Advance |
| v2 Prompt messageId | 只原输入已插入；无Run成功 | 等当前前台与原TransportPending，不独自Advance |
| v2 新可靠前台终止 end_turn | 会话前台成功；无输入因果不补Run成功 | 满足输入/transport/资源条件后可Advance，无须第二idle |
| v2 新可靠前台终止 error | 会话前台可靠失败；可发生insert前，不猜哪个Run失败 | 同ExternalWorkKey只一次Suspend当时旧项 |
| v2 新可靠终止 max_tokens / max_turn_requests / refusal / 普通cancelled | Stopped(原reason)前台事实；Run只在另有可靠因果时关联 | 同区间一次Suspend旧项；显式继续恢复 |
| 任意unknown state/reason、snapshot/repeated stop、丢失响应 | 原raw保留/Unknown，不猜可靠结束 | 不造新Suspend/Advance，不重发，保核验/安全收敛出口 |

用户授权保留停止原因，对refusal还保固定SDK的后续上下文语义raw，不改写原内容。SQL result_kind新增stopped，与reason组合明确校验，不将v1 Completed枚举名等同业务success。普通cancel与显式“立即发送”不同：已有授权的优先中断以原控制捕获scope/work、确认旧前台结束后只发原选项，其他相对顺序保留；自然完成竞态接受真实终态，不等第二cancelled。该控制不被普通cancel的暂停规则卡死已选项，也不隐式恢复此前已失败挂起旧项；有效Permission仍按协议实际取消范围逐一响应。

时序要求：running→idle→insert先保idle和pending，insert到后在原Ticket终态/必要事实可靠后释放，不等第二idle；running→idle→新running→insert则新Active继续阻；无attempt原生活动不造Run；旧interval重复failure/stopped不能挂起后来新D；早RPC不解除尚需flush的active。

## 9. 停止、重启与迁移

所有source/resource失败先查closure-contract.md全阶段矩阵：原Pre只作记录地址，Owned未prepare或source Bound/resource Pre均可用真实stage closure证据闭合，不补造Scope/setup。source/resource reserved而从未enter launcher的关闭，只按resource-ownership.md §4消费唯一LaunchPermit封未来launch并以NeverLaunchedProof调用close_unlaunched。若enter已发生，Err/timeout/Drop不能当未启动；若reader终止，只记SourceLoss而非收敛。prepare同步失败表已按精确typed归属统一：adopt前无IO任务，adopt后原IoOwner/installation guard在同SourceSlot真实清理，不在同步Result隐含asyncjoin。

组合supervisor的关闭顺序必须实现而非只列名称：

1. lifecycle封新factory/source/job，所有危险write boundary同步Stopping；停止新native任务登记，仍允许当前可验证安全收敛和必要SQL补记。
2. 对每SourceSlot/Running发取消意图，独立pump继续排空实际已取得LF及小事实；不先abort pump再丢receipt，不借数据库故障无限等待控制。
3. 请求真实Ticket/IO收敛；自有独占source在同一总deadline内先协议grace，再按现OwnedProcess.shutdown必要时终止完整Job来解开pipe，随后join原IoOwner及stderr；不能先无界join挡住kill。joined:false/树Incomplete保同Arc槽/claim，后台或下次close继续。共享实例只有全部归属允许才kill；单目标不能因此杀他会话，未知归属保责任。
4. 已join source最后drain必要worker及本次安全收敛事实（失败/未知如实记录），再关闭R3管理受理并按原B/D/E关闭规则完成/保留Applying，join全部组合任务。claim只能在组合全join后释放，不能原QueueService先释放而native仍活；组合模式下claim释放从原QueueService移到组合supervisor，同队列only模式行为不变。
5. Storage原close仍等待组合supervisor真实join后才 `request_registry.close_connections()`：seal共享ConnectionOwner，等leases==0且全部close slots真实join，再pool.close。lease.handoff与close-slot注册原锁不改；不timeout物理SqliteConnection.close。COMMIT成功后close失败只报Unconfirmed，不撤销事实/重D。
6. Storage.close等待方取消/并发调用保持同supervisor槽；Storage Drop仅同步cancel+seal，不宣称join完成。取消、panic或状态锁poison只保cleanup责任，不授新写。

迁移仅追加配套schema6对象（含本阶段最小source/resource必要归属表），在原BEGIN IMMEDIATE中完整跑0/1/2/3/4/5原阶段再append6；exact schema覆盖新表索引约束，node_meta/user_version同事务变6，未知future布局拒绝，故障整回滚。旧身份/撤销/管理/正文/DDL均保原字节；不打开用户数据库、不隐式清私有文件。已存在旧runtime armed记录不能从缺body推零，不恢复任何Job/队列/正文。prepared无arm只有证旧runtime无未来写才restart_zero；unknown slot须原生核验或合法资源收敛后才能解除。

本次没有运行SQL/编译/验收。独立门禁与本次问题逐项答复见 verification-contract.md；只有tester/reviewer对这组新hash同版预审通过且权威spec摘要更新后，才可能进入实际编码。


## revision6 控制路径的精确工程补充

[control-ports.md](control-ports.md)是§6–9安全控制的唯一精确接口补充：同P的NativeControlPort、三种原生控制和唯一ControlOperation，完整R3优先/原Lease/关机intent链、错误返还、当前范围和旧取消无未来poll门、必要SQL故障出口。安全权限Cancelled只在已授权取消范围，unknown主题/kind本身不触发拒绝；普通Queue Cancel不升级native取消。独立session.cancel Web/SQL受理及人工Selected批准仍另phase；本轮不假已接Node。

本包DDL在原10表3索引基础追加1个body-free control facts表，故为11表3索引；原schema1..5与锁文件保持。控制事实不是Received，不造sequence/receipt；control_id+phase不可变事实先后不回退，安全IO不以先存Reserved为条件，必要缺口不清前不授新输入。其完整仓储/失败矩阵和验收可达性同附件。v5的RESOURCE004原Pre/真实stage close证明全部保留，仅source/target seal精确集合显式加入控制operation及其必要worker。
