# revision2 原生桥、创建两阶段和监督链

当前根没有本文件新view端口；ND6+receipt supplement也尚未实现。本文件是在其**同P/同source/同单pump**上的明确工程增量，不修改原八份冻结合同/DDL或复活ND4/5。来源必须真实，新增端口完整列出，不能由后续coder临时补fakeScope/Loaded/true authority。

## C1. create没有SID时的可信两阶段

当前实际 `RequestKind::NewSession`无SessionId，真实 `Incoming::Response{id,response:Response::NewSession(session),raw}`才有它。因此create enqueue之前只能登记 `(actual Scope, typedRPC, purpose=Create, same SourceSlot, config/source/workspace)`，**没有NativeSessionKey/ResourceEpoch/Run**。v1“创建也在enqueue前按SID绑定”在本版废止。

```rust
pub struct VerifiedCreateObservation<S> { /* 原create reservation+实际source */ }
impl<S> VerifiedCreateObservation<S> {
    pub fn scope(&self)->&S;
    pub fn rpc(&self)->&RpcScalar;
    pub fn create_id(&self)->EntityId;
    pub fn source(&self)->NativeSourceId;
}
pub struct VerifiedCreatedTarget<S> { /* 同CreateOp+真实NewSession receipt */ }
impl<S> VerifiedCreatedTarget<S> {
    pub fn key(&self)->&NativeSessionKey;
    pub fn scope(&self)->&S;
    pub fn rpc(&self)->&RpcScalar;
    pub fn create_id(&self)->EntityId;
    pub fn response_sequence(&self)->u64;
    pub fn response_receipt_order(&self)->u64;
}
pub enum ViewResourceStamp {
    NativeResource(ResourceEpoch),
    CreatedPendingResource { create_id:EntityId },
    ModuleFixture { fixture_operation:EntityId },
}
impl TargetBindReceipt {
    pub fn key(&self)->&NativeSessionKey;
    pub fn route_version(&self)->u64;
    pub fn captured_cut(&self)->u64;
    pub fn create_id(&self)->Option<EntityId>;
    pub fn origin(&self)->ViewTargetOrigin;
}
pub enum ViewTargetOrigin { ExistingResource,CreatedPendingResource,ModuleFixture }
impl CreateWatchReceipt {
    pub fn create_id(&self)->EntityId;
    pub fn route_version(&self)->u64;
    pub fn captured_cut(&self)->u64;
}
```

这些是观察关联证明，不是执行写grant。NativeResource分支只来自ND6原ObservationTargetBinding<S>中的真实PreResource/BoundSource/ResourceEpoch；Create尚无真实resource时用CreatedPendingResource，不编LoadEpoch或假Ready。ModuleFixture独立类型分支，production factory不能接受它作为ND真实resource。ViewResourceStamp只读投影不反造proof。

真实create owner登记CreateOperation到原SourceSlot stable operation registry，含actualscope、typedRPC、CreateId、原配置引用/同source slot identity、原Reservation/Frame/Probe或Ticket唯一stage；两个不同create同时在途各自独立RPC，不以“最近create”关联。先 `register_create(VerifiedCreateObservation)`形成source marker，才原enqueue。标准早到session/update真实Received原文照常capture到source未分配区，sessionId可解析并保实际值，但**不推断它属于某个create**。ND控制小事实/权限仍沿原source quarantine/实际有效请求处理，不等显示回填或假target。

同单pump收到真实Response::NewSession时，adapter从原actualscope+typedRPC+RequestKind::NewSession的操作登记核同CreateOp。仅此处私有 `CreatedTargetIssuer::from_response(&mut original_create_op,&actual_lease)`消费未resolved状态，取真实SessionId和原配置NativeSourceId，经现NativeSessionKey::from_config构建key；别名/路径不另建来源。返回VerifiedCreatedTarget给capture.resolve_create，原CreateOp标resolved并存原receipt，不能从Response::NewSession裸JSON或别的RPC调用。原response可早于flush，归属关联不等Written；原Ticket仍保到实际终态，由原create owner处理发送/资源账本，不丢唯一句柄。

resolve_create追加RouteBindMarker{CreateId,key,route_version,response_seq,cut=当前retained_frontier}。worker按marker顺序将**同actualscope、标准sessionId精确等于这次真实返回ID**的未分配raw回填到该key，原Received.sequence排一次，不按正文/时间/最近create猜。已归其它known target/epoch的记录不移交；若真实ID与现已管理同key冲突，交唯一原生owner协调并返回Conflict/ReadFailure，不建第二可写owner，不把scope内所有early数据广播给新target。始终无可靠session envelope的raw仍只source保存。

BindReceipt getters=key、route_version、captured_cut、create_id?:、origin(ExistingResource|CreatedPendingResource|ModuleFixture)。open取得cut时同时记录该key当前route_version，barrier等待processed_frontier≥cut且processed_route_version≥binding.version，故source已保early数据、response到达后即使无新增量也能回填首开。若marker分配失败，预留故障位记录路由不完整，open不能返回假空Complete。create失败/RemoteError/无真实NewSession时未分配数据保source，不伪key；原create owner的取消/SQL/资源责任不由view解决。

已知SID的resume/replay不同：在原actualscope和可信观察resource binding已核、实际RequestKind::Resume/Replay typedRPC分配后、enqueue前 `bind_known(VerifiedViewTarget)`，同样marker+cut回填；不要求Loaded才能观察早到事件，但不能据此发Prompt。unbind/源替换带真实旧binding generation，先B3 seal旧source/view/stream，再通知worker失效；旧read/create response只归旧operation候选，不能覆盖新epoch。

## C2. 同P的新增协议端口

```rust
pub enum ViewSetupKind { Replay{native_session:Arc<str>}, Create }
pub trait ReservedViewSetup: Send+Sync+'static {
    type Scope:Clone+Eq+Send+Sync+'static;
    type Frame:EncodedPromptFrame;
    type Probe:Clone+Send+Sync+'static;
    fn scope(&self)->Self::Scope;
    fn rpc(&self)->&RpcScalar;
    fn kind(&self)->ViewSetupKind;
    fn probe(&self)->Self::Probe;
    fn encode(&self)->Result<Self::Frame,ViewError>;
}
pub trait NativeViewProtocol: NativeControlPort {
    type ViewRaw:RetainedRaw;
    type ViewReservation:ReservedViewSetup<Scope=Self::Scope,Frame=Self::Frame,Probe=Self::Probe>;
    fn retain_view_observation(&self,lease:&Self::Lease)
        -> Result<ViewObservation<Self::Scope,Self::ViewRaw>,ViewError>;
    fn reserve_view_replay(&self,source:&Self::Source,
        binding:&VerifiedHistoryBinding<Self::Scope,Self::BoundSetup>,wait:ResponseWait)
        -> Result<Self::ViewReservation,ViewError>;
    fn reserve_view_create(&self,source:&Self::Source,
        binding:&VerifiedCreateSourceBinding<Self::Scope,Self::BoundSetup>,wait:ResponseWait)
        -> Result<Self::ViewReservation,ViewError>;
    fn enqueue_view_setup(&self,source:&Self::Source,reservation:Self::ViewReservation,
        frame:Self::Frame,boundary:Arc<dyn ViewSetupWriteBoundary<Self::Scope>>,deadline:Instant)
        -> Result<Self::Ticket,EnqueueRejected<Self::ViewReservation,Self::Frame>>;
    fn cancel_view_setup(&self,reservation:Self::ViewReservation)
        -> CancelledViewSetup<Self::Scope,Self::Probe>;
}
pub trait ViewSetupWriteBoundary<S>:Send+Sync {
    fn check_and_poll(&self,write:&ViewSetupWriteIdentity<S>,step:DispatchWriteStep,
        cx:&mut Context<'_>,io:&mut dyn DispatchPollIo)->Result<(),DispatchFenceRejection>;
}
```

P继承完整NativeControlPort→NativeLifecyclePort→NativeProtocolPort，不把同P降成只trait Prompt；L/G/N bounds完整沿A1/ND6三仓储。P::Frame/Probe/Ticket/Source/Scope/Lease保持同一组实际类型。ViewObservation字段私有，只由原adapter读同Lease.received()生成：actualscope、actual sequence/receipt_order、原raw O(1)clone、原version、有限message类别及标准sessionId引用/实际operation分类；不会复制巨大tool/message IDs或构造第二Received。它是tap保留data，不是ack proof；不允许应用从裸ObservationView/SQL组装它。

retain_view_observation只借原Lease，不消费/ack/IO；生产R=RetainedAcpRaw只薄包原RawMessage，capture failure/projection failure仅view错误，ND仍独立P.view→小事实apply/gap→P.acknowledge。若原ND P.view本身控制分类错误沿其原P.fail_observation；不能因本新增保留失败去调用它。shared tap接口私有 `ViewObserverInstall<P>::on_lease(&P,&P::Lease)` 内部取真实data并capture，无返回控制fatal类别；所有Received含无target帧都retain一次。

reserve_view_replay在同实际Connection.strategy().replay(真实SessionId,&完整SessionSetup)→reserve_request；reserve_view_create为strategy.new_session(完整SessionSetup)→reserve_request。包装检查原RequestKind分别Replay或NewSession、Some typedRPC/actualscope，保原Reservation/EncodedFrame/Probe，不能把Prompt/Cancel/Load回包当这个品牌。create扩展只是原创建owner的真实观察/数据来源接口，**不是新增Web create管理授权入口**；创建受理和必要管理账本继续由负责创建的owner，不借view HTTP/DTO实现动作。

VerifiedHistoryBinding含同原SourceSlot实际source/BoundSource、真实canonical key/当前观察resource、完整已授权BoundSetup及版本/standard_replay，只有当前ND资源owner协调取得；无参数/Unknown时仅该读取InvalidConfiguration，不能空MCP/根覆盖。VerifiedCreateSourceBinding来自原已授权创建操作的source配置/完整setup和同actualscope，**无SID/NativeSessionKey/ResourceEpoch**；不能沿HistoryBinding硬填默认SID。此两私有binding提供只读scope/config identity与version getter，不向public暴露完整配置。

enqueue_view_setup转同Connection原enqueue（Observed仅Prompt专属连续ack门，不将普通replay显示加入它）；boundary保同原WriteIdentity scope/typedRPC/真实RequestKind、原完整config/source/必要resource与取消/关闭围栏，同真实PollIo，不另拿writer。ViewSetupWriteIdentity完整只读字段：actualscope、typedRPC、kind、source_id/instance/runtime、config identity/revision、optional native target+真实resource或CreateId；optional并非伪值。长parse/SQL/viewworker/actor读取gate不进入该实际native poll围栏。

CancelledViewSetup字段私有=原scope、原typedRPC、原kind、同cell Probe；getters scope/rpc/kind/probe，无Clone/Deserialize/into_reservation。adapter唯一原Reservation.cancel产生，真实P.probe_progress确认FinishedZero后才ReadTransportState::Zero；控制专用CancelledControlReservation（通知RPC=None）和ND原Load CancelledLifecycleReservation不能混用。enqueue错误仍返同原Reservation/Frame；app cancel_reserved错误into_original保原对象，不丢所有权。

## C3. 安装与单观察pump的可调用顺序

新增 `ViewObserverInstall<P>`只由A1 native.start_shared_views在原source registry登记。内部方法：

```rust
impl<P:NativeViewProtocol> ViewObserverInstall<P> {
    pub(crate) fn source_bound(&self,source:&BoundSourceProbe<P::Scope>,
        task_owner:&NativeSourceTaskRegistration)->Result<ViewSourceInstallation<P>,ViewError>;
    pub(crate) fn observe_known_target(&self,binding:&ObservationTargetBinding<P::Scope>)
        -> Result<VerifiedViewTarget<P::Scope>,ViewError>;
    pub(crate) fn create_reserved(&self,op:&CreateOperation<P>)
        -> Result<VerifiedCreateObservation<P::Scope>,ViewError>;
    pub(crate) fn created_from_lease(&self,op:&mut CreateOperation<P>,lease:&P::Lease)
        -> Result<VerifiedCreatedTarget<P::Scope>,ViewError>;
    pub(crate) fn replay_completed_from_lease(&self,op:&ReplayOperation<P>,lease:&P::Lease)
        -> Result<ReadCompletionProof<P::Scope,P::ViewRaw>,ViewError>;
    pub(crate) fn source_stopping(&self,source:&BoundSourceProbe<P::Scope>,
        intent:&NativeSourceStopIntent)->Result<VerifiedSourceViewSeal<P::Scope>,ViewError>;
    pub(crate) fn target_stopping(&self,target:&ObservationTargetBinding<P::Scope>,
        intent:&NativeTargetStopIntent)->Result<VerifiedTargetViewSeal<P::Scope>,ViewError>;
}
```

上述BoundSourceProbe/ObservationTargetBinding原ND6私有对象由其本模块边界合法投影给view installer，不公开field构造；桥方法必须实现于有权借这些原对象的application/native_dispatch装配模块。不能让一般view子模块越过Rust隐私访问，也不能靠新增public bool/getter反造source。NativeSourceTaskRegistration是原source supervisor登记receipt，不是新线程自报；ViewSourceInstallation持独占capture+已监督worker注册，所有成员安装成功后原ObservedStart.start开门。view安装失败原source still由ND管理，明确CaptureUnavailable，不遗弃IoOwner。

NativeSourceStopIntent/NativeTargetStopIntent是桥对ND6已实际受监督的Stopping/SourceWorkSeal/TargetWorkSeal操作的私有只读引用，字段含同slot identity/generation/closure operation；不从bool/客户端DTO构造，不替代其实际seal/quiescence。source_stopping只将同实际BoundSource投影成view发送seal，在未有Scope阶段无需view seal，因为尚未有view source；不会为清理创建Scope。实际SourceTerminal也可封该view，terminal id仍沿原P来源。fixture用其真正owner.shutdown/request_stop事件的独立ModuleFixture seal分支，无手拼NativeSourceStopIntent。

原producer顺序：取得同P.next唯一Lease→借retain raw给source ingress→原ND P.view小事实分类/同步apply及必要gap→原P.acknowledge消费该Lease。view异步worker从不获得Lease/Acknowledger。Create/Replay响应小事实分发同原operation registry；显示完成/route marker只是额外data投递，不反向等待viewworker。adapter端有原实际Received/typedRPC/RequestKind校验，应用不能拿Incoming.clone造新receipt。

ReadCompletionProof匹配Scope+typedRPC+target+ReadEpoch，真实Response::Replayed（v1原Replay request被Strategy编码session/load，v2 replayFrom=start）+worker处理该proof前所有raw无gap；ReadTransport独立。普通Response::Resumed、裸NewSession、Written、idle/平台H/SnapshotEnd均不能mintComplete。真实Replayed先到flush后Err仍保完成；Operation留同原Ticket直至实际终态，observerDrop不能释放native责任。

## C4. 真实关闭与新增端口范围

ND6 SourceWorkSeal/TargetWorkSeal/SQL quiescence覆盖其全部native操作和控制收据的规则原样保留。新增CreateOperation/ReplayOperation（实际发生才存在）注册同source的原task/SQL sequencer集合；可能已enqueue时保持同Ticket，无通用取消重放原生能力。关闭先停止新read request/create intent并seal source/view发送（B3同步短操作），真实native安全控制/OwnedProcess/IO关闭沿ND独立证明，view worker/join不挂其必要SQL/actor/控制ack。没有ActualScope阶段绝不为cleanup调用prepare/start造Scope。

这些额外操作没有聊天正文SQL/视图持久表；创建必要元数据由原创建owner保存，view不将CreatedPendingResource当已Loaded计费/执行证明。ND原控制取消收据rpc=None/Some分支与本新必有RPC的view/setup收据明确分开，不把frame_count/packet/run_id填RPC。

## C5. L3/L4/L5的真实入口与写范围

- L3：A6实际Connection::spawn/唯一EventReceiver/IoOwner；已知SID和create按真实Reservation/Response取得私有fixture绑定，标准raw经同capture/reducer；A7 actual ApplicationPolicy小型配对/认证正链，reader没有socket身份主张。fixture内唯一producer从原receiver取出后记录，首开已有queued output也drain，不第二reader。SourceSeal/sequence/失败注入只调用实际view路径，不能改source flags。
- L4：使用ND6正式实现后的Storage::start_native_queue_service_with_repositories原工厂、真实SqliteRequestRegistry派生三仓储、P=AcpNativeAdapter、原QueueDriver/SourceSlot/ObservedStart、合成可信配置和TestOwnedStream（或真实Windows launcher）。A1追加view工厂插入source激活前；不以L3registry或纯trait替代此层。实际ND保护/ack/必要SQL故障方可结论。
- L5：真实NodeRuntime/WSS认证SocketAuthAttempt/原ConnectionLease/TrustStore COMMIT、原actualsocket writer及新binarycodec；C=LiveSocketContext。新Web scanner/renderer安全交互实际浏览器验证，不能借A7local actor或假sink代签。
- L6：专用真实Harness/原生来源/Windows Job树/真实Web/存储；不能产生的真实分支未执行，不降替身。

仅新增fixture默认关闭feature及相应test-support facade，生产管理入口不导出issuer；Rust原module声明/Node test_support/Cargo挂载与ND root同步修改需单owner串行。root最终同源码hash重跑实际检查/构建/前端/适用strict alltarget，原306绿不代增量。
