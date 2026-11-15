# revision6 + receipt-supplement：原生安全控制的完整泛型路径（ND-CONTROL-001）

2026-11-01。工程合同，未实现、未编译、未执行SQL。本文补齐 engineering-contract §6–9 的实际调用，保留 closure-contract 的全部阶段证明。所有 factory、NativeCoordinator、NativeRuntimeHandle 和 SourceSlot 的 P 统一要求 `P:NativeControlPort`，该端口继承同 NativeLifecyclePort；Frame/Probe/Ticket/Source/Scope/Lease 全用同 P 的关联类型，应用不导入 ACP、不 downcast、不另取 Connection。此处 Selected 权限批准、独立 session.cancel 的 Web/SQL 受理仍未实现，也不借旧 ProductCommandFact、客户端 DTO 或 bool 铸造授权。

## 1. 精确端口与数据类型

```rust
pub enum ControlIdentity<S> {
    NativeCancel { scope:S, native_session:Arc<str> },
    PermissionCancelled { scope:S, native_session:Arc<str>, rpc:RpcScalar },
    Unsupported { scope:S, rpc:RpcScalar },
}
pub struct CancelledControlReservation<S, Probe> {
    identity: ControlIdentity<S>,
    probe: Probe,
}
impl<S, Probe> CancelledControlReservation<S, Probe> {
    pub fn identity(&self) -> &ControlIdentity<S>;
    pub fn probe(&self) -> &Probe;
}
impl<S> ControlIdentity<S> {
    pub fn scope(&self) -> &S;
    pub fn rpc(&self) -> Option<&RpcScalar>;
    pub fn native_session(&self) -> Option<&str>;
}
pub trait ReservedControl: Send + Sync + 'static {
    type Scope: Clone + Eq + Send + Sync + 'static;
    type Frame: EncodedPromptFrame;
    type Probe: Clone + Send + Sync + 'static;
    fn identity(&self) -> &ControlIdentity<Self::Scope>;
    fn probe(&self) -> Self::Probe;
    fn encode(&self) -> Result<Self::Frame,DispatchError>;
}
pub struct PermissionIdentity<S> {
    pub scope:S, pub rpc:RpcScalar, pub native_session:Arc<str>,
}
pub struct UnsupportedIdentity<S> { pub scope:S, pub rpc:RpcScalar }
pub struct ControlIoOptions { pub write_deadline:Instant }
pub trait NativeControlPort: NativeLifecyclePort {
    type PermissionRequest: Clone + Send + Sync + 'static;
    type UnsupportedRequest: Clone + Send + Sync + 'static;
    type ControlReservation: ReservedControl<Scope=Self::Scope,
        Frame=Self::Frame,Probe=Self::Probe>;
    // Pure extraction of the actual branded object. Never reserve/encode/enqueue/ack here.
    fn permission_from_lease(&self,lease:&Self::Lease)
        -> Result<Option<Self::PermissionRequest>,DispatchError>;
    fn unsupported_from_lease(&self,lease:&Self::Lease)
        -> Result<Option<Self::UnsupportedRequest>,DispatchError>;
    fn permission_identity(&self,request:&Self::PermissionRequest)
        -> Result<PermissionIdentity<Self::Scope>,DispatchError>;
    fn unsupported_identity(&self,request:&Self::UnsupportedRequest)
        -> UnsupportedIdentity<Self::Scope>;
    fn reserve_native_cancel(&self,source:&Self::Source,
        binding:&CurrentCancelBinding<Self::Scope>)
        -> Result<Self::ControlReservation,DispatchError>;
    fn reserve_permission_cancelled(&self,source:&Self::Source,
        request:&Self::PermissionRequest,binding:&PermissionCancelBinding<Self::Scope>)
        -> Result<Self::ControlReservation,DispatchError>;
    fn reserve_unsupported_reply(&self,source:&Self::Source,
        request:&Self::UnsupportedRequest,binding:&UnsupportedReplyBinding<Self::Scope>)
        -> Result<Self::ControlReservation,DispatchError>;
    fn enqueue_control(&self,source:&Self::Source,reservation:Self::ControlReservation,
        frame:Self::Frame,boundary:Arc<dyn ControlWriteBoundary<Self::Scope>>,
        deadline:Instant)
        -> Result<Self::Ticket,EnqueueRejected<Self::ControlReservation,Self::Frame>>;
    fn cancel_reserved_control(&self,reservation:Self::ControlReservation)
        -> CancelledControlReservation<Self::Scope,Self::Probe>;
}
pub trait ControlWriteBoundary<S>: Send + Sync {
    fn check_and_poll(&self,identity:&ControlIdentity<S>,step:DispatchWriteStep,
        cx:&mut Context<'_>,io:&mut dyn DispatchPollIo)
        -> Result<(),DispatchFenceRejection>;
}
```

`ReservedControl` 故意没有 `rpc()->&RpcScalar`：native cancel 是通知，原 Reservation.id() 必须 None；两个原请求响应则必须 Some(原 typed ID)，整数与同形 string 永不合并。ControlIdentity 只是只读投影，不是 grant。三个 reserve 各核对应真实 OutboundKind，不能把 Prompt/Initialize/Load/Selected 包成 ControlReservation。绑定对象所有字段私有、无 Deserialize/公开构造；关联原 request 可 Clone 只共享品牌/raw，不共享回复权。

`cancel_reserved_control`只消费尚未enqueue的原Reservation，先保同原ControlIdentity，再原Reservation.cancel得实际Probe，返回控制专用CancelledControlReservation。字段私有，无public构造/Clone/Deserialize/Default；identity()/probe()只借用观察，不产生Ticket/Frame/写权，实际NativeCancel的rpc()为None，两个回应仍Some原typedID。probe_progress沿LifecyclePort，只有原Finished(Zero)才作无写证明。enqueue后唯一原Ticket由ControlOperation持有；Ticket.request_cancel/wait只控制本地写，绝不代替reserve_native_cancel。encode借用失败仍保原Reservation；enqueue错误返回同原Reservation+Frame。完整SourceSlot取消stage/getters及Load专用收据隔离见本包control-receipt-supplement.md。

## 2. 生产映射与最小构造链

`AcpNativeAdapter` 同时实现 NativeProtocolPort、NativeLifecyclePort、NativeControlPort。P::PermissionRequest/P::UnsupportedRequest 是实际 ACP 请求的私有薄包装，来自同 Lease.received().incoming 的原请求 clone；permission_identity 必须核实际 scope Some，不接受 Strategy 离线解析的无品牌请求。Unsupported 没有 target/session/work，不能捏造一个。

| 端口 | 同原 Source 的真实 ACP 调用 | 必须保留的真实对象 |
| --- | --- | --- |
| reserve_native_cancel | Connection.reserve_cancel(原 SessionId, ExtensionFields::empty()) | 原 OutboundKind::Cancel；id=None；scope 核原 source |
| reserve_permission_cancelled | Connection.reserve_permission_response(&原 PermissionRequest, PermissionSelection::Cancelled) | 原 scope/session/typedID/raw；不提供 Selected 入口 |
| reserve_unsupported_reply | Connection.reserve_unsupported(&原 UnsupportedRequest) | 原 scope/typedID/raw；原 -32601 安全错误，不声称支持方法 |
| encode | 原 Reservation.encode() | 原 EncodedFrame 含 LF；长度 checked，原品牌/key |
| enqueue_control | 同 Connection.enqueue(原 Reservation,原 Frame,真实 control fence,deadline) | 原 WriteTicket；同 writer/ProtocolState/实际 PollIo |
| cancel_reserved_control | 原 Reservation.cancel() | CancelledControlReservation{原ControlIdentity,同cell原Probe}；通知rpc=None，回应Some；没有新Ticket |

Observed 下旧 enqueue 只禁止 Prompt；控制仍走这条原 enqueue，不能调用不受围栏的替代 writer。ControlFenceBridge 从原 WriteIdentity 核实际 kind/scope/可选 ID 与该 ControlSlot 后转 ControlWriteBoundary；它只转发同原 PollIo。真实 writer Proxy 在同 ProtocolState guard 核 stopped/cancel/deadline，再调用实际 W.poll，沿原 writer 调度和控制优先级；不在 view/identity/extraction 里 IO。Prompt 的连续 ack/held-LF 门保持专属，不阻上述安全控制；控制也不假 ack、跳 sequence 或释放 held。

## 3. Intent 的真实来源和一次交接

所有 Intent 无 public 构造、不可 Clone/Deserialize。SourceSlot 在 enqueue 前登记 ControlSlot 与原 operation，首 await 前归组合 supervisor。没有来自客户端 request ID、裸 RequestFact 或 SQL 查询结果的 constructor。

```rust
impl<T:Send+'static> NativeQueueDriver<T> {
    pub(crate) fn take_priority_handoff(&self,target:EntityId)
        -> Result<Option<PriorityHandoff>,DispatchError>;
}
impl<P,L,G,N> NativeCoordinator<P,L,G,N>
where P:NativeControlPort, L:SourceLaunchPort,
      L::Owned:OwnedNativeSource<Streams=P::Streams>,
      G:ConfigurationPort<LaunchPlan=L::Plan,Setup=P::Setup>,
      N:NativeDispatchRepository+NativeResourceRepository+NativeControlRepository {
    pub(crate) fn capture_priority(&self,h:PriorityHandoff,options:ControlIoOptions)
        -> Result<OwnedControlBatch<P>,ControlCaptureRejected<PriorityHandoff>>;
    pub(crate) fn capture_shutdown(&self,c:&ClosingAuthority,
        source:&SourceSlotKey,options:ControlIoOptions)
        -> Result<OwnedControlBatch<P>,DispatchError>;
    pub(crate) fn capture_unsupported(&self,source:&SourceSlotKey,
        lease:&P::Lease,options:ControlIoOptions)
        -> Result<Option<OwnedControlOperation<P>>,DispatchError>;
    pub(crate) fn prepare_owned_control(&self,op:&mut OwnedControlOperation<P>)
        -> Result<(),DispatchError>;
    pub(crate) fn enqueue_owned_control(&self,op:OwnedControlOperation<P>)
        -> Result<RunningControl<P>,ControlEnqueueRejected<P>>;
    pub(crate) fn into_control_zero<'a>(&'a self,running:RunningControl<P>,
        stored:&'a StoredControl)
        -> BoxFuture<'a,Result<ControlZeroRetry<P>,ControlRunningRejected<P>>>;
    pub(crate) fn into_reserved_control_zero<'a>(&'a self,op:OwnedControlOperation<P>,
        stored:&'a StoredControl)
        -> BoxFuture<'a,Result<ControlZeroRetry<P>,ControlOperationRejected<P>>>;
    pub(crate) fn retry_control(&self,retry:ControlZeroRetry<P>,deadline:Instant)
        -> Result<OwnedControlOperation<P>,ControlRetryRejected<P>>;
}
```

PriorityHandoff 的唯一 mint 点是现 R3 QueueService::finish 内：原 Pending.applied=true，原 ConsumeLease/OperationProbe 精确匹配可靠 Settled(Prioritized{item,revision})，同 OwnerState 当前优先项仍是该 item/revision；在移出 Pending 的同短锁将原 owner Arc identity、原 D stamp、registration/operation/runtime、item/revision 放进 OwnerState 新的内部 handoff 槽，且同步设 target control hold。必须是本 runtime 确实 apply 的那次 D；Existing(fact) 的重放、Rejected、Queue Cancel、Edit、Resume、单纯 SQL 查询均不能 mint。该槽先于回复管理 caller；callerDrop 不丢控制责任。一个 handoff 槽不覆盖另一未完成 handoff，原 pending/优先动作按现 R3 串行规则等待/保责任。take_priority_handoff 仅从同 owner 槽 move 一次，放入已登记组合任务，不重验原 actor。

capture_priority 在同 owner/resource 短锁 drain 已观察小事实，核原 queue priority 仍当前，并从真实 SessionActivity.cancel_scope() 捕获原 WorkToken/serial 与当时有效 Permission 引用，连同原 BoundSource/PreResource/epoch 转私有 CurrentCancelBinding 和逐请求 PermissionCancelBinding。没有 work/当前已可靠自然结束时不造新的等待结束标志；仍处理 captured scope 的有效 Permission。若优先项已编辑/取消，不将它发送，也不为新 revision 续授权；已产生的控制按原捕获范围安全收尾。capture 失败返回原 handoff，hold 仍由原 owner 保持待处理/合法终结，不能丢后立刻放新输入。

ClosingAuthority 仅组合 supervisor 从实际 Running→Closing 原子转换得到，带原 runtime nonce、source slot/代际和可验证关机范围。capture_shutdown 不靠 DB 成功、actor 或网络 DTO；同 actualscope 存在且初始化实际已完成时才能 reserve native control。PreScope/未初始化不能硬发 cancel；沿 closure-contract 真 Owned cleanup。共享源单目标关闭不获全源 kill 权。

capture_unsupported 从同原 Lease 提取 request，校验 scope/seq/source，登记 source 控制槽并 clone 原品牌；这是协议规定的自动安全错误 intent，无 actor/target。pump 仍同步 apply→登记 obligation→ack 原 Lease，实际编码/写由独立受监督控制任务进行，不在 pump 等 SQL/IO。视图继续纯读取。

此阶段没有新的 standalone SessionCancel 管理受理 API。原 domain ProductAction 的枚举、旧产品 SQL accepted 行都不是本 runtime 控制授权。当前需要的实际 native cancel 由已授权优先中断与真实关机链到达；未来接 standalone 管理取消必须交真实受理 capability，不把普通队列 Cancel 改成原生取消。

ControlIoOptions由可信调度器给本次控制实际单调write预算，非输入/队列TTL；关机批不超过原source总shutdown deadline，不因保存重试偷偷续命。纯capture可同步保选定options，过期在原enqueue/实际writer形成真实Zero/Unknown；不用固定新增秒数，不要求cancel有response deadline/RPC。

## 4. 权限取消范围与唯一回复权

Permission 保存在 SourceSlot 请求表：键为实际 scope+原 typed RPC，值持原 P::PermissionRequest、原 resource/owner/epoch、观察序、有效状态和唯一 ResponseClaim。该表由真实 Lease 同步登记，target 路由可用 ObservationTargetBinding，不需 BoundResource/完整 setup/Loaded。未绑定 session 持 source quarantine 并等待真实 resolver，不能凭 session 字符串选别的 target；未解释前不代发权限拒绝。

PermissionCancelBinding 只能由当前已授权 PriorityHandoff/ClosingAuthority 的真实捕获范围匹配该原请求并原子占有 ResponseClaim 后生成。scope/resource/原 typedID/raw 品牌必须一致、请求当前仍有效、未被另一允许/拒绝/取消 owner 认领；reserve 再由实际 ACP 核原 digest 和 ServerStatus::Waiting。binding 不把请求 unknown subject/kind 当理由，收到 unknown Permission 仍保护和等待人工；只有原取消意图所覆盖的有效请求才回复 Cancelled。有效 allow 未来仍须严格保存且原 optionId 唯一，该阶段未接 allow SQL owner，不预先宣称完成。

后续在同 captured work/资源上实际观察到的 Permission，由同 target cancel intent 的范围检查再次认领，不能把新 work/新资源的请求并入旧批。安全控制任务不依赖用户决定才 ack；没有取消 intent 则只保观察与保护。

ACP reply 的 server grant 在首个实际接受字节时已 spent，不是 Written 时才 spent。因此每次 poll 不要求 ServerStatus 仍 Waiting，也不能 reserve 第二回复。开始前核有效且 claim 当前；开始后只准原 ControlSlot 的 ReplyContinuation 在同 source/资源代际下完成同帧/flush，保 target hold，不能转成 Selected 或换请求。scope/资源替换、真实 source close 或不再可验证范围则停止原写并如实 Unknown/清理。Written 仅证明 Cancelled 响应完整写出；对该原请求同步标 response-written 并立必要 gap，必要记录可靠后才解除该请求的保护。Unknown 保保护/Unknown 责任直到真实原请求失效或资源收敛，不用本地 Ticket.cancel 清审批。

## 5. 所有权、旧取消迟到与真实完成轴

收据类型补充：未enqueue取消由同OwnedControlOperation::cancel_unenqueued(&mut self,&P)消费原Reservation，并将本控制专用CancelledControlReservation存入CancelledUnenqueued{receipt,frame_length:Option<u64>}；cancelled_receipt()仅借用返回该原收据。into_reserved_control_zero仍消费同op+可靠StoredControl，核该receipt同原Probe的实际Zero。完整签名与不可伪造边界见本包control-receipt-supplement.md §1–2；旧Load的CancelledLifecycleReservation不用于任何控制stage。

OwnedControlOperation<P> 不可 Clone：持原 intent、同 ControlSlot nonce、binding、原 request（若有）、元数据 probe、deadline。私有阶段为 Claimed{未reserve}→Reserved{原Reservation/Probe,可选原Frame}→Prepared{原Reservation+Frame}→Enqueued{唯一原Ticket}→Terminal{实际报告}；capture只登记并保原品牌，不编码/IO。受监督worker调用prepare_owned_control在同槽转发本文reserve/encode，任一失败借用原op仍保准确阶段/原物；enqueue_owned_control只消费Prepared，错stage拒绝返原op。OwnedControlBatch持原intent、同barrier及这些operation的完整精确集合，没有第二Ticket列表。RunningControl持同slot与Ticket，callerDrop不能移除supervisor槽。ControlEnqueueRejected.into_original返回同OwnedControlOperation（含EnqueueError原两物），ControlCaptureRejected.into_original返回原handoff。清理先消费原Reservation.cancel，或对原Ticket.request_cancel并wait；未知不能伪造Zero。不可恢复的实际失败令源保Unknown并走真cleanup，不循环补新control。

ControlZeroRetry 是唯一可重试入口：into_control_zero 实际 await 同唯一 Ticket.wait 得 Zero，核同 operation 的原 terminal StoredControl 已可靠且全字段一致，才消费 Running 并 move 同原 intent/request/claim 至 ControlZeroRetry；失败返原 Running。未enqueue取消经into_reserved_control_zero核同槽原Probe的实际Zero与可靠StoredControl，失败返原OwnedControlOperation；不能拿裸Probe构造。retry_control再核同current source/scope/resource/work/仍live原request与同barrier，消费该对象后调用本端口原reserve并encode；新control_id/attempt_serial只增一次，保原obligation_id、旧Zero及metadata链。ControlRetryRejected.into_original返回私有枚举OriginalRetry(尚未建立下一attempt时的原ControlZeroRetry)或OwnedAttempt(已经建立的下一OwnedControlOperation及其准确stage/原Reservation/Frame)；不能在新Reservation尚活或新Zero未可靠时把旧retry再次交出。旧Ticket已真终态、底层Zero后原reply grant恢复Waiting，才能合法reserve同原请求；任何Unknown/Written都无此转换。重试不重D、不新受理、不换raw品牌、不解除hold，也不因安全路径豁免旧Zero可靠门槛。

CurrentCancelBinding 捕获 scope/原 native session、原 PreResource/owner/resource epoch、原 CancelScope/WorkToken/serial、intent identity、同 target cancel barrier。每次实际 Chunk/Flush，control fence 按 owner→resource→原 PollIo 锁序检查：同 runtime/source/资源代际、原 control slot、原 intent 仍授权、没有新的 work/input。开始前必须 cancellation_is_current；已接受同帧后可在原 work 已真实 terminal、但 barrier 仍原批且未交接任何新 work 的状态完成原帧和 flush。自然完成不能让旧 ControlSlot 变成“当前新 work”的授权。

target cancel barrier 从捕获/可靠优先 handoff 起到所有本批 control 操作无未来 poll 前一直阻挡该 target 下一 Prompt/首次写，包含旧 native cancel Queued、Writing{accepted:0}/Pending0、Flushing 和待清理错误。旧 work 先自然完成时：未 enqueue 的原 Reservation 实际 cancel→Zero；已 enqueue 调同原 Ticket.request_cancel→wait。只有确切 Zero 才可无写结束旧 cancel；若已经 Written，终态保证不会再 poll，但仍需真实可靠旧前台已结束/旧输入插入+transport终态、相应必要事实已可靠后才放优先项。Written 不是 native Cancelled、finish_work 或 idle 证据；不等“第二次 cancelled”事件。

部分写/Err0/flush失败的 Unknown 不可重发，原 source 保 Unknown/新输入封锁，等实际源收敛/合法核验；实际 ACP 原 writer 故障/部分取消会 stop Core，不能只摘 barrier 继续复用该连接。自然完成+Pending0 的旧 cancel 必须真实零写终态或源真实关闭，不能仅凭 WorkToken 已旧而让 Y 先 enqueue，再期待 fence 碰巧拒绝迟到 cancel。v2 原输入插入/旧 transport pending 仍独立阻挡，安全控制不能清它们。

WorkToken/CancelScope 仅本地围栏，不证明 ACP 提供 work causality。完整 cancel 已写在新 Prompt 之前不另声称 native 已执行；业务结束只依原版可验证前台/原 RPC 语义。优先中断只推进原已选项，普通 cancelled 的停止映射与旧队列暂停保持既有用户决定。

UnsupportedReplyBinding 只需当前真实 source/scope/原请求 claim；无假 resource/work。PermissionCancelled 使用上述请求/捕获范围围栏。它们没有 Prompt 写资格。普通新输入、Selected allow 和危险 load 不能构造 ControlBinding，不能借故障时安全路径发送。

## 6. 必要SQL故障下的安全出口与元数据

控制意图/当前范围在本地可靠 owner 与真实 ACP 上核验，不要求先写本次 SQL 才能做可验证安全 cancel/Cancelled/unsupported/close；故障时不重验原 actor，也不接受新的裸管理 DTO。旧 R3 Prioritized 必须已有可靠 E；若 E 自身未知，还没有该优先中断 intent，普通发送保持阻挡。关机 intent 和原品牌自动 unsupported 不依赖该 E。

安全执行不清已有 RequiredGap，也不因行缺失冒充持久成功。登记control先分配本地obligation/control ID并保hold；还没有Reservation/帧时没有伪造transport事实。实际Prepared或实际Probe/Ticket终态产生时，在同短锁为该阶段分配同source/target generation的必要requirement并冻结body-free ControlRecordProof，再交worker；故障/提交未知留原槽、原proof和该ID。控制可先完成实际IO，必要记录仍重试原proof。Permission response-written的解除保护与新Prompt必须等相同必要记录可靠；unsupported源义务完成记录失败也留source gap，不能用它解除普通发送。纯进程物理cleanup不等待SQL成功，但DB责任仍由原SourceSlot保留。

```rust
pub trait NativeControlRepository: Send + Sync + 'static {
    fn record_control<'a>(&'a self,p:&'a ControlRecordProof,
        c:&'a OperationContext,d:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<StoredControl,DispatchError>>;
    fn inspect_control<'a>(&'a self,p:&'a ControlRecordProbe,d:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<ControlInspection,DispatchError>>;
}
```

ControlRecordProof 从原 ControlSlot stage 冻结：control_id、obligation_id、attempt_serial、phase、requirement_id、原 runtime/source/instance/connection、scope_binding_id、optional 原 resource/load_epoch/owner/work_serial、intent_kind 与 intent_reference（可靠 priority registration operation 或 supervisor shutdown_id 或原 unsupported obligation_id）、control kind、同原 typedRPC（cancel 则无）、原frame含LF长度（实际encode未成功且从未enqueue的Zero可None）、实际 transport fact/原因与 accepted、节点时间、metadata fingerprint。Frame None不是空帧，不能用于Prepared/Unknown/Written。reserve未成功且无Reservation时只关闭本地未尝试义务，不写transport Zero。不含正文/raw/config、actor凭据、原native session字符串（SQL目标由原resource确定）。不能从SQL恢复品牌/意图/写权。

record_control 按 `(control_id,phase)` 只插入不可变事实，phase=reserved或terminal；后者state才是zero/unknown/written。可从尚无行直接插入terminal（安全 IO 不以先存 Reserved 为条件），同键全字段精确相同才幂等，否则Conflict，绝不改终态。每条有独立 requirement_id；terminal先可靠时迟Reserved worker只能插其历史事实，不回退内存阶段或清其它ID。共享obligation_id+单调attempt_serial关联合法Zero重试，仓储核同原intent/request/前Zero链；无请求TypedID的cancel不补RPC。提交未知inspect只确认原phase全字段，不给未知/丢失旧操作补Zero。body-free codec沿主合同exact u64/typedRPC/fingerprint。

ControlInspection返回Absent或该原control_id的完整reserved/terminal事实集合；StoredControl返回实际已核同phase的全部元数据与requirement_id。仓储另核source/runtime/instance/connection/scope_binding跨表相同、resource属于同source/owner/epoch、intent和kind及可选typedID匹配、同phase元数据完全相等；FK不替这些语义检查。重启只解释旧metadata及资源责任，不从控制facts恢复request品牌、重发cancel/reply或默认权限已失效；沿原旧runtime无未来写及实际资源收敛路径处理。

SourceSqlSequencer.seal_source_after_cleanup 和 TargetWorkSeal 的 quiescence 现在显式包括全部 ControlSlot、callerDrop cleanup、真实 Ticket 终态/无未来 poll、控制元数据 worker outcomes、inspection 结果及 pending requirements；物理 source 关闭不伪 control Written，记录未终态者按实际原 probe/cleanup 可证报告，未证则 Unknown。close 只在完整精确集合/必要记录已核可靠后持久 release；必要SQL仍故障则保 Closing/Unknown 责任，即使已物理清理。无 ControlOperation 可在 source 已持久 closed 后复活。

原 target close 的 CurrentTargetCloseBinding 也从上述真实 ClosingAuthority/有效范围生成，沿 LifecyclePort 原 Strategy.close/reserve_request/同 Ticket。它可绕 Prompt frontier/非授权必要SQL缺口做安全收敛，但不绕实际版 close 能力、同 scope/resource 和 unique owner；Written/Closed 字样仍不是完整关闭证明。PreScope 禁止为了控制创建 scope/初始化。

## 7. 失败边界与清理矩阵

| 边界/故障 | 原唯一所有者与处理 | 可持久事实/新输入 |
| --- | --- | --- |
| 可靠 priority E 前失败/未知 | 原 R3 Pending/lease，未 mint handoff | 不发送 native cancel；E待核；新输入仍原门槛 |
| E成功、callerDrop、capture前 | OwnerState handoff+hold 已归 supervisor；重试同handoff | 不重验actor/不mint第二D；不会丢控制后放Y |
| 当前优先项已编辑/取消 | 原batch完成原范围安全清理；不替新revision发送 | 释放只依原控制终态+事实，旧挂起项不暗恢复 |
| 原 request 外源/过期/重复认领 | 同原request/claim保留，真实reserve拒绝 | 无IO；不会clone另一份响应；必要矛盾隔离 |
| reserve失败未得Reservation | 原 intent+request owner；typed错误，重查同current或实际cleanup | 无造Zero/Written；未初始化可走Owned关闭 |
| encode失败 | 同control operation仍持原Reservation；实际cancel取probe | 原Zero才记Zero；保原scope/request |
| enqueue失败 | 错误返同Reservation+Frame给原slot，实际cancel/收尾 | 无重建frame/ID；只能真实终态 |
| Queued/Pending0时旧work自然完成 | 原Ticket.request_cancel→wait；barrier不摘 | 真Zero或真source收敛后才考虑Y |
| 首字节后自然完成/请求已spent | 原slot Reply/CancelContinuation完成同帧，仍hold | 不再要求server Waiting；不mint第二响应 |
| Written先于native终态/insert | 唯一Ticket真终态保留，业务轴继续等待 | 不finish_work/不造Cancelled；Y仍阻挡 |
| IO Err0/部分/flush Unknown | 同Ticket/原source Unknown；不重发，真实IO/Owned收敛 | 保gap/保护；不能复用停掉连接发Y |
| 必要SQL reserve/terminal故障 | 原proof+requirement ID和worker保责任；可执行已验证安全IO | 普通Prompt/Selected仍阻；安全不冒落库 |
| task panic/caller cancellation/Storage.close并发 | 原supervisor slot，stop本地写、保Ticket与IoOwner、同总deadline收敛 | 不能Drop等于join；最终SQL关闭顺序沿closure |
| scope/resource替换或真实source loss | 原control停止/实际终态，旧回包只落旧代际 | 新源不继承request/intent；loss仍非release |

## 8. 验收可达性（定义由独立tester维护）

同80稳定ID/标题不由作者改写。本附件为 NDT-037/052/071/073 等既有定义提供真实路径；tester 决定补充子断言/组合。必须实际 P=AcpNativeAdapter、原真实对端 IO，不能只 FakeProtocolPort 返回 Written。

- 原 UnsupportedRequest Lease→纯提取→source slot claim→原 reserve_unsupported/encode/enqueue，peer 得原 typedID 且仅一次 -32601；同形ID、foreign品牌、同请求双claim、Zero后仍current的同原重试、Unknown后不得重发分别核验。重试必须先保存原终态，保原 obligation/品牌，不能同时活两Ticket。
- 实际 Permission 原请求→保护+ack；unknown subject/kind 不触发自动取消。真实 priority/Closing 捕获后仅覆盖有效原请求，peer 得 Cancelled；Selected接口在本port不存在；错scope/原raw/重复选择由真实ACP拒绝。首字节spent后flush仍同原票，不能因server表已移除造第二回应。
- 可靠R3 Prioritized真实D/E→唯一handoff→同原cancel scope→peer收到无id session/cancel；Queue Cancel/SQL fact replay不发nativecancel；撤销原actor后仍只完成原已受理动作。
- reader held LF/未ack时 Prompt 实际W.poll=0，当前安全control实际可poll并保持连续ack责任；控制fence拒绝时真实W.poll=0，合法每步仅同原PollIo一次。
- 旧前台自然完成而cancel处于Queued/Pending0，强制peer writer Pending；Y不可enqueue/首写；实际撤旧票Zero后才按其它输入轴推进，旧cancel不会迟到打Y。部分/flush Unknown用真writer故障+source清理，不能换一个假written票。
- SQL开始/commit未知/终态保存故障注入下，已有可靠优先intent或关机/自动unsupported可真正发送安全control；缺口不清，新Prompt/允许不能发送。所有故障槽由Storage.close找回，source seal统计control任务与SQLworker；实际Windows owned树清理另依现定义。
- factory只给完整 NativeControlPort 的编译正例；只有NativeLifecyclePort、伪造Binding/Request wrapper、把Cancel套ReservedPrompt、DTO→grant、另取Connection绕路为编译负例/结构检查。端口签名无全域control rpc()，无Selected入口，无正文快照、数量/TTL新增。

以上均是冻结待验收合同，运行数0。ND-CONTROL-001 只能由独立同版门禁认定方案答复；本作者不自行标动态Closed。
