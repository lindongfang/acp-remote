# revision6：完整泛型生命周期端口与实际关联类型

2026-11-01。工程候选，未编码/编译。本文完整替代revision3中“generic P可由内部SourceInstaller自行完成初始化/恢复”的未定义接口；主interfaces保留Prompt/R3/仓储接口并显式增加本文bounds。应用内层不importACP、Windows、sqlx，不downcast P，不由SQL生成Scope/Ready。生产适配仅拥有同实际Connection，禁止为了测试另开旁路连接。

## 1. 泛型等式、配置与真实启动资源

```rust
pub trait OwnedNativeSource: Send + 'static {
    type Streams: Send + 'static;
    fn take_streams(&mut self) -> Result<Self::Streams,DispatchError>;
    fn owner_kind(&self) -> NativeOwnerKind;
    fn request_stop(&self);
    fn shutdown(&mut self,deadline:Instant)
        -> BoxFuture<'_,Result<OwnedSourceShutdown,DispatchError>>;
}
pub trait SourceLaunchPort: Send + Sync + 'static {
    type Plan: Send + Sync + 'static;
    fn same_plan(&self,current:&Self::Plan,candidate:&Self::Plan) -> bool;
    type Owned: OwnedNativeSource;
    fn launch(&self,plan:Arc<Self::Plan>,permit:LaunchPermit,
        adopter:LaunchAdopter<Self::Owned>,deadline:Instant)
        -> BoxFuture<'_,LaunchCompletion>;
}
pub trait ConfigurationPort: Send + Sync + 'static {
    type LaunchPlan: Send + Sync + 'static;
    type Setup: Clone + Send + Sync + 'static;
    fn resolve<'a>(&'a self,p:&'a AcceptedTargetProof)
        -> BoxFuture<'a,Result<ConfiguredSourcePlan<Self::LaunchPlan,Self::Setup>,DispatchError>>;
}
```

ConfiguredSourcePlan<LP,S>完整只读数据：ConfiguredSourceKey、source_id、target/native key、workspace、显式选用harness_id和定义revision、config_identity/revision、`launch:Arc<LP>`、`setups: Vec<(ProtocolVersion,S)>`；每份S保留该版可信参数快照（可明确Known::Unknown），不补空集；它不是已可编码的Loaded配置，P.bind_setup才校验完整性并产生BoundSetup。LP和S是外层关联类型，内层不碰ProcessSpec或SessionSetup；缺配置返回typed错误。

生产ConfigurationPort读实际product_targets/product_sources/product_harness_sources/harnesses/workspaces，核目录和显式选用入口，可信node配置提供器解析config_ref（无私有Harness文件猜测）；默认继承node环境，显式覆盖保真。生产LP=私有WindowsLaunchPlan，内含现ProcessSpec；S=ACP配置适配器持有的AcpSetupCandidate（保SetupSnapshot的Known/Unknown及原扩展）；P.bind_setup经实际SessionSetup::try_from_snapshot后才产生完整BoundSetup。L4合成配置提供器可替代可信配置输入，但必须明确标记，不返回Loaded/Ready，不替代实际SQL/ACP/Job。

LaunchPermit/LaunchAdopter/LaunchCompletion均由application资源owner定义、字段私有：

- LaunchPermit含同SourceSlot Arc identity、runtime/instance、controller generation、唯一nonce，不Clone/Deserialize。唯一方法`enter(self)->Result<EnteredLaunch,RevokedLaunch>`在同controller短锁把Reserved→Entered；若已撤销则不能进入。**任何可能创建进程/fixture流/外部资源的provider调用之前必须enter**。不是“函数返回成功”才算启动。
- LaunchAdopter<Owned>是同稳定SourceSlot的不可Clone交付口，`adopt(EnteredLaunch,Owned)->Result<AdoptedSource,AdoptRejected<Owned>>`把原owner move到该slot；reject返同Owned，调用launch任务仍负责实际shutdown，不能Drop冒join。可在SourceSlot已Stopping时adopt到cleanup状态，拒绝新写但不能拒绝清理归属。
- LaunchCompletion=Adopted{source_nonce}|RejectedBeforeEntry{code}|EnteredFailure{code,cleanup_id}。它只是报告，**不铸NoLaunch/Job退出证明**。Entered失败必须已经由adopter登记真实cleanup owner/cleanup future；若provider错误不能确认资源是否已产生，slot保持EnteredUnconfirmed并封危险写。ordinary caller没有该future所有权。

Windows launcher在已监督source任务内调用现OwnedProcess::spawn；返回OwnedProcess立即move到adopter，再取IO。spawn future与所有中间guard仍归该source任务，Stopping请求取消意图而不abort尚可能持进程的launch future；必须等待真实清理结果或保Unknown槽。实际spawn错误后的OS cleanup需要真实guard报告，不能把Err当零启动。`OwnedNativeSource::take_streams`只能一次，外层Windows封装把stdout/stdin交ACP，保OwnedProcess/独立stderr drain任务。其shutdown复用现OwnedProcess先必要kill解pipe再join，不要求先join成功才kill。

L4 OwnedStreamFixture仅cfg(test)实现上述OwnedNativeSource，持真实双工流和真实fixture任务；owner_kind=TestOwnedStream，不声称Windows Job active=0。正式Windows资源测试使用真实Windows launcher+自有child。

## 2. 协议生命周期端口

```rust
pub trait LifecycleReservation: Send + Sync + 'static {
    type Scope: Clone + Eq + Send + Sync + 'static;
    type Frame: EncodedPromptFrame;
    type Probe: Clone + Send + Sync + 'static;
    fn scope(&self) -> Self::Scope;
    fn rpc(&self) -> &RpcScalar;
    fn kind(&self) -> LifecycleKind;
    fn probe(&self) -> Self::Probe;
    fn encode(&self) -> Result<Self::Frame,DispatchError>;
}
pub trait NativeIoOwner: Send + Sync + 'static {
    fn request_stop(&self);
    fn shutdown(&self,deadline:Instant)
        -> BoxFuture<'_,IoShutdownObservation>;
}
pub trait NativeLifecyclePort: NativeProtocolPort + Sized {
    type Streams: Send + 'static;
    type Setup: Clone + Send + Sync + 'static;
    type BoundSetup: Clone + Send + Sync + 'static;
    type Source: Clone + Send + Sync + 'static;
    type Start: Send + 'static;
    type IoOwner: NativeIoOwner;
    type Receiver: Send + 'static;
    type Lease: Send + 'static;
    type Acknowledger: Clone + Send + Sync + 'static;
    type Gate: Clone + Send + Sync + 'static;
    type RawEvent: Clone + Send + Sync + 'static;
    type InitReservation: LifecycleReservation<Scope=Self::Scope,Frame=Self::Frame,Probe=Self::Probe>;
    type LoadReservation: LifecycleReservation<Scope=Self::Scope,Frame=Self::Frame,Probe=Self::Probe>;
    type CloseReservation: LifecycleReservation<Scope=Self::Scope,Frame=Self::Frame,Probe=Self::Probe>;

    fn prepare_source<A>(&self,streams:Self::Streams,adopt:A)
        -> Result<PreparedProtocol<Self>,PrepareFailure>
        where A:FnOnce(Self::IoOwner)->Result<(),IoAdoptionRejected<Self::IoOwner>>;
    fn prepared_scope(&self,start:&Self::Start) -> Self::Scope;
    fn start_source(&self,start:Self::Start) -> Self::Source;
    fn source_scope(&self,source:&Self::Source) -> Self::Scope;
    fn reserve_initialize(&self,source:&Self::Source,response_deadline:Instant)
        -> Result<Self::InitReservation,DispatchError>;
    fn setups_equal(&self,left:&Self::Setup,right:&Self::Setup) -> Result<bool,DispatchError>;
    fn bind_setup(&self,source:&Self::Source,wire:ProtocolVersion,
        key:&ConfigurationKey,setup:Self::Setup)
        -> Result<Self::BoundSetup,DispatchError>;
    fn setup_matches(&self,source:&Self::Source,current:&Self::BoundSetup,
        candidate:&Self::Setup,key:&ConfigurationKey) -> Result<bool,DispatchError>;
    fn reserve_resume(&self,source:&Self::Source,
        resource:&BoundResourceProbe<Self::Scope>,setup:&Self::BoundSetup,
        wait:ResponseWait) -> Result<Self::LoadReservation,DispatchError>;
    fn enqueue_initialize(&self,source:&Self::Source,reservation:Self::InitReservation,
        frame:Self::Frame,boundary:Arc<dyn LifecycleWriteBoundary<Self::Scope>>,
        deadline:Instant)
        -> Result<Self::Ticket,EnqueueRejected<Self::InitReservation,Self::Frame>>;
    fn enqueue_resume(&self,source:&Self::Source,reservation:Self::LoadReservation,
        frame:Self::Frame,boundary:Arc<dyn LifecycleWriteBoundary<Self::Scope>>,
        deadline:Instant)
        -> Result<Self::Ticket,EnqueueRejected<Self::LoadReservation,Self::Frame>>;
    fn reserve_target_close(&self,source:&Self::Source,
        binding:&CurrentTargetCloseBinding<Self::Scope>,wait:ResponseWait)
        -> Result<Self::CloseReservation,DispatchError>;
    fn enqueue_target_close(&self,source:&Self::Source,reservation:Self::CloseReservation,
        frame:Self::Frame,boundary:Arc<dyn LifecycleWriteBoundary<Self::Scope>>,deadline:Instant)
        -> Result<Self::Ticket,EnqueueRejected<Self::CloseReservation,Self::Frame>>;
    fn next<'a>(&'a self,source:&'a Self::Source,receiver:&'a mut Self::Receiver)
        -> BoxFuture<'a,ProtocolRead<Self::Lease,Self::Scope>>;
    fn view<'a>(&self,lease:&'a Self::Lease)
        -> Result<ObservationView<'a,Self::Scope,Self::RawEvent>,DispatchError>;
    fn cancel_resume(&self,reservation:Self::LoadReservation)
        -> CancelledLifecycleReservation<Self::Scope,Self::Probe>;
    fn probe_progress(&self,probe:&Self::Probe) -> TransportProgress;
    fn fail_observation(&self,source:&Self::Source,lease:Self::Lease,
        error:DispatchError) -> SourceTerminal<Self::Scope>;
    fn acknowledge(&self,ack:&Self::Acknowledger,lease:Self::Lease,
        applied:&AppliedObservation<Self::Scope>)
        -> Result<(),AckRejected<Self::Lease>>;
    fn attach_prompt_binding(&self,source:&Self::Source,
        proof:&VerifiedLoadedBinding<Self::Scope,Self::BoundSetup>,gate:&Self::Gate)
        -> Result<(),DispatchError>;
}
```

P::Frame/P::Probe/P::Ticket继承同NativeProtocolPort类型；原Frame只读byte_len_checked含LF，实际Ticket的wait终态不变。PromptTicket名字保兼容，但它的实现就是实际WriteTicket，初始化/恢复也能使用同只读进度/终态端口；没有第二Ticket。InitReservation/LoadReservation是不同私有checked newtype，各自拥有原实际Reservation，核Some typedRPC/实际kind/scope。LifecycleKind=Initialize|Resume{native session,load epoch,configuration key}|Close{native session,load epoch}，只是只读投影。

PrepareFailure=BeforeAdopt{code}|AfterAdopt{code,installation_id}，均typed且不带正文。BeforeAdopt保证未spawn任何IO任务：fallible预分配失败时原流在P内关闭；adopter拒绝必须以IoAdoptionRejected<I>{code,owner:I}返同IoOwner给P，P只stop空Core/Drop原流，无asyncjoin。AfterAdopt保证同slot已持唯一原IoOwner及安装guard/handles，错误不取出owner；后续由该slot真实shutdown/join。不能在同步Result隐含异步join。

PreparedProtocol<P>字段为`start:P::Start, receiver:P::Receiver, acknowledger:P::Acknowledger, gate:P::Gate`，无公开构造。实际P::IoOwner已通过adopt move到同SourceSlot；start尚未消费，任何read/write poll=0。P::prepare_source生产转发真实ACP prepare_observed的adopt-before-spawn接口；安装中installing guard、同槽handle注册和concurrent shutdown沿interfaces冻结规则。adopt拒绝时尚无IO任务；成功后安装失败/恐慌由同slot真实IO owner保join。start_source只消费原token开门，无新分配/失败/await。

LifecycleWriteBoundary<S>::check_and_poll与DispatchWriteBoundary有同样恰一次实际PollIo约定，参数改成`LifecycleWriteIdentity<S>{scope,rpc,kind,source/instance/runtime,configuration key,load_epoch?}`，kind真实由Reservation/WriteIdentity提取；initialize无load/setup，resume必须完整相同setup binding。initialize/resume在source/资源必要记录未可靠、scope变动、Stopping、取消或错typedRPC时拒绝原写。初始化/恢复不受Prompt专属ack屏障自锁，但仍受自己真实来源/配置/资源/取消围栏；不能借此enqueue Prompt。安全Close独立按control-ports.md当前ClosingAuthority/真实范围处理，不因Prompt门或非授权记录故障无条件封安全收敛，仍核真实scope/资源/实际协议能力和原RPC，不放宽最终release证明。

ObservationView完整字段：scope、sequence、receipt_order、raw共享引用`&P::RawEvent`、`InboundSmallView`。InboundSmallView覆盖主合同全部Incoming：Initialized{rpc,wire,CapabilityFacts}、Resumed{rpc,native_session}、Replayed、NewSession、Sessions、Closed、PromptV1{rpc,reason}、PromptV2{rpc,message}、RemoteError{rpc,code}、Permission{当前真实请求opaque引用}、State{native_session,raw state,reason,origin}、Tool/Message/Other、UnsupportedRequest、UnknownNotification。小事实只是adapter从实际Lease读取，原raw含所有未知结构继续非持久共享，不在应用按最近Run猜关联；CapabilityFacts仅供实际已实现的版本/恢复/重放/配置准入，未知原广告仍保raw。

`ProtocolRead<L,S> = Received(L) | Terminal(SourceTerminal<S>)`。SourceTerminal字段私有，只由实际P::next将原receiver Err/None与source_scope封装，或P.fail_observation实际停止同Core后封装一次：terminal_id、actualscope、reason(EOF|IO|Protocol|Stopped|ObservationFault)、观察节点单调顺序取样；**没有sequence/receipt_order**，不伪造Received。重复next终止返回同terminal_id，便于同批幂等，不能每次新造loss。view错误意味着真实lease无法安全分类，应用调用P.fail_observation消费该原lease；适配器先共享保留原raw、实际stop同source Core并不ack该lease，再返回reason=ObservationFault的SourceTerminal（同source首次terminal_id，非Received）。危险写已封；这只证source进入Unknown，不证IOjoin/native停止。不能无接口地在泛型应用mint terminal。

AckRejected<L>返原lease和typed错误；ack必须比较真实lease.scope/seq与AppliedObservation，确认所有小事实已apply/gap已立再转真实ack。attach_prompt_binding只登记原source、gate和私有VerifiedLoadedBinding为生产P::reserve_prompt的可用binding；它不是通用setter。必须核同scope/配置，SQL行不能调用它造binding。NativeProtocolPort::reserve_prompt仍使用同source registry里的原Connection，enqueue走原enqueue_observed，不开第二连接。

## 3. 工厂/协调器的完整约束

```rust
pub struct NativeRuntimeHandles<A,V,R,N,C,P,L,G>
where V:ValidatedBodyPort, P:NativeControlPort,
      L:SourceLaunchPort, L::Owned:OwnedNativeSource<Streams=P::Streams>,
      G:ConfigurationPort<LaunchPlan=L::Plan,Setup=P::Setup>,
      N:NativeDispatchRepository+NativeResourceRepository+NativeControlRepository {
    pub queue:QueueService<A,V,R,C>,
    pub driver:NativeQueueDriver<V::Body>,
    pub native:NativeRuntimeHandle<P,L,G,N>,
}
impl Storage {
    pub async fn start_native_queue_service_with_repositories<A,V,C,P,L,G,R,N,FR,FN>(
        &self,authority:A,validator:V,clock:C,
        retain:Arc<dyn RetainPrompt<V::Body,Retained=P::Retained>>,
        protocol:Arc<P>,launcher:Arc<L>,configuration:Arc<G>,audit:Arc<dyn AuditPort>,
        wrap_registry:FR,wrap_native:FN)
        -> Result<NativeRuntimeHandles<A,V,R,N,C,P,L,G>,DispatchError>
    where A:RequestAuthority+'static, V:ValidatedBodyPort+'static, C:NodeClock+'static,
          P:NativeControlPort,
          L:SourceLaunchPort, L::Owned:OwnedNativeSource<Streams=P::Streams>,
          G:ConfigurationPort<LaunchPlan=L::Plan,Setup=P::Setup>,
          R:RequestRegistryRepository+'static,
          N:NativeDispatchRepository+NativeResourceRepository+NativeControlRepository,
          FR:FnOnce(SqliteRequestRegistry)->Result<R,DispatchError>+Send,
          FN:FnOnce(SqliteNativeDispatch)->Result<N,DispatchError>+Send;
}
```

NativeCoordinator<P,L,G,N>严格沿同等bounds，SourceSlot字段持`Option<L::Owned>,Option<P::IoOwner>,Option<PreparedProtocol<P>>,Option<P::Source>`以及私有stage enum、原Init/LoadOperation、同Arc supervised tasks；不以类型擦除或downcast躲bounds。生命周期和Prompt端口由同一个P实现，sourceHandle/Scope/Ticket等式不依赖运行时猜测。P/L/G/N的构造从factory传入并首await前存组合supervisor，同原R3 claim/owner/ConnectionOwner；所有spawn/observe/SQL交接在该任务链，无外部caller拥有Job/Reservation。

共享source（含初始化未完成）前L.same_plan比较完整程序/args/cwd/env覆写，P.setups_equal逐已实现wire比较原ConfiguredSourcePlan中完整setup集合，不能仅比identity；P.setup_matches比较完整当前bound setup与候选setup（含MCP/额外根/未知扩展/版本），身份stamp相等本身不够。不等则不能借原Loaded/config续stamp。计划与setup共享不可变Arc，无日志或SQL配置正文。

生产实例：P=AcpNativeAdapter，L=WindowsSourceLauncher，G=SqliteNativeConfigurationProvider，N=SqliteNativeDispatch（三仓储trait），R=SqliteRequestRegistry。P::Source=实际Connection；P::Start=实际ObservedStart；P::IoOwner薄包实际IoOwner；Receiver/Lease/Ack/Gate直接实际观察类型；Frame=实际EncodedFrame；Probe=实际WriteProbe；Ticket=实际WriteTicket；Init/Load/Prompt wrapper各只持原真实Reservation。P::Streams为适配器拥有的stdin/stdout pair，WindowsOwnedSource.take_streams把原ProcessIo的stdin/stdout move成该pair，stderr留同owner任务；LP和Setup只在相应外层解释。L4可换G可信合成配置输入，P必须实际ACP，Windows测试L仍真实；TestOwnedStream明确标记，不能替代NDT-105/113的真实Job。

## 4. Pre→Bound Scope与完整方法调用表

`PreSourceProbe`：source_id/instance_id/runtime_epoch/connection_epoch/config identity+revision/owner_kind/created_at、原AcceptedTargetProof来源及同SourceSlot Arc identity；**无actual Scope字段**。`PreResourceProbe`再含target/nativekey/owner_id/load_epoch/reserved_at。二者由实际受理来源+可信配置mint，SQL reserve只接Pre。

真实launch/prepare后 `SourceSlot::bind_prepared(pre,&StoredSource,&PreparedProtocol<P>) -> BoundSourceProbe<P::Scope>`：核同Arc/全部Pre字段、record_source_spawned可靠、P::prepared_scope真实token；同短锁`ScopeState::Unbound→Bound{actualscope,scope_binding_id}`恰一次。重复同实例同scope返回原只读binding，异scope Conflict并封源；没有基于epoch的Scope构造函数。BoundSourceProbe不可公开构造，包含原Pre、actualscope和新scope_binding_id。`record_source_bound`可靠保存opaque scope_binding_id后才能start，SQL仍不能恢复actualscope。

`BoundResourceProbe<S>` = 原PreResource+同BoundSourceProbe+当前原setup binding+已协商wire；只由同source/resource slot核精确StoredResource和BoundSource、P::bind_setup成功后mint。LoadReservation随后绑定其actualRPC/原frame长度，形成私有ReservedLoadProof。`DispatchProbe/AttemptProbe`只有取得最终VerifiedLoadedBinding后使用原actualscope。`ObservationProbe`可以由已一次BoundSource/BoundResource及真实Lease提前mint，用于加载中当前小事实；它没有写资格，不要求Loaded。实际(scope,session)到ResourceSlot的观察映射在enqueue_resume之前登记，因此早state/Permission可以准确路由；仅VerifiedLoadedBinding才升级输入Ready。

| 受监督任务顺序 | 仅用本文端口的实际调用/对象 | SQL/proof门槛 |
| --- | --- | --- |
| 受理来源 | actual driver.capture AcceptedTargetProof；G.resolve得到LP+version Setup | R3 E稳定、原actor不重验 |
| 唯一准备 | reserve_source(PreSource)、reserve_resource(PreResource) | 两COMMIT可靠；claim/任务先归Storage |
| 启动 | L.launch(LP,LaunchPermit,Adopter,deadline) | permit.enter在真实provider调用前；原Owned先adopt |
| 流交接 | sourceSlot持L::Owned，take_streams一次；P.prepare_source(streams,adoptIo) | 原owned/IoOwner均同slot；IOstart前=0poll |
| actualscope | P.prepared_scope；bind_prepared一次；record_source_bound(BoundSource) | opaque绑定记录可靠；无DB反造scope |
| 开门/接收 | P.start_source原Start；supervised pump调用P.next/view/ack | pump在应用小事实后连续ack，source绑定先注册 |
| initialize | P.reserve_initialize→encode同Frame→P.enqueue_initialize | 原SourceBound记录/围栏；RPC operation先登记，原Ticket保留 |
| 初始化完成 | pump真Initialized receipt + 同Init Ticket.wait Written | record_source_initialized完整可靠，strategy/capabilities保raw；不等同Loaded |
| 版配置 | 从G原setups选实际wire，P.bind_setup | Unknown/不支持拒相应load，不用空集 |
| load准备 | mint BoundResourceProbe；P.reserve_resume→encode原frame | begin_load原RPC/length可靠；未知inspect只能原对象继续 |
| load写 | P.enqueue_resume原Reservation/Frame→同Ticket | operation先登记；writer真实poll围栏，无第二RPC |
| load接受 | pump真Resumed核原scope/RPC/session；保早response | record_loaded(BoundResource,receipt)和record_load_transport(actualTicket)分轴可靠 |
| Loaded | 同slot/原source/setup，实际load Written、真response、native foreground/旧资源/全部必要gap符合 | 私有mint VerifiedLoadedBinding<S,P::BoundSetup>；P.attach_prompt_binding；NativeEnvironment实时计算Ready |
| prompt | 原R3 capture/select/arm/实际P.reserve_prompt/enqueue_observed | 全部既有v3 Prompt线性/ack/held/poll约束仍保留 |

InitOperation/LoadOperation为不可Clone的应用私有enum：Reserved{原res+frame+probe+NeverEnqueued}→Enqueued{同Ticket+probe}→Terminal{actual terminal}；响应轴另存实际lease receipt及raw共享引用。早response不能清仍需flush的operation；begin_load COMMIT未知保原Reserved对象；enqueue失败返同res/frame；可能写后不造新RPC。操作registry键actual(scope,typedRPC,purpose)在enqueue前登记，只从实际端口分配ID，不用platform request ID填RPC。外部NewSession不替换原目标Resume，错误不业务降版重发。

## 5. 当前实际ACP适配落点及等待预算

真实initialize使用Connection::reserve_preferred_initialize(Instant)，当前接口没有Unbounded，因此生命周期端口也只收Instant，不把Unbounded虚构成底层能力；实际load用Connection.strategy()?.resume(session,&SessionSetup)→Connection::reserve_request(request,ResponseDeadline)，可按实际ResponseWait::At/Unbounded映射。reserve失败未产生句柄，encode失败仍由原operation持Reservation；enqueue错误用原EnqueueError.into_parts返同实际对象。原Init/Load包装只核相应RequestKind，不改变协议one-shot initialize语义。

SourceIoOptions完整字段：spawn_deadline:Instant、initialize_write_deadline:Instant、initialize_response_deadline:Instant、load_write_deadline:Instant、load_response_wait:ResponseWait。它们是可信调度器为一次实际IO/握手选定的独立单调等待预算，不新增输入或队列TTL，不统一固定秒数、不因长正文/队列等待换RPC续命。无法在初始预算内完成则如实记录实际Zero/Unknown/Written及资源范围，安全收敛另有其同总deadline。

IoShutdownObservation完整字段与原IoOwner.shutdown报告对应：joined、outstanding probes、unresolved typed RPC、sanitized error；只是实际查询结果，由同slot实际join路径解释，false不转true。OwnedSourceShutdown为WindowsJob{真实ShutdownDisposition,active_processes,root_exit_code,stderr_joined}或TestOwnedStream{真实fixture task join报告}，不得混用。尚未prepare IO用IoStage::NotPrepared的实际stage证明，不填joined=true。

生命周期最后poll接口完整为：

```rust
pub trait LifecycleWriteBoundary<S>: Send + Sync {
    fn check_and_poll(&self,write:&LifecycleWriteIdentity<S>,step:DispatchWriteStep,
        cx:&mut Context<'_>,io:&mut dyn DispatchPollIo)
        -> Result<(),DispatchFenceRejection>;
}
```

它保同原实际PollIo，拒绝不调用provider，合法调用恰一次；setup/source身份错误不靠新frame重建来修复。所有以上新关联对象与错误类型由同阶段应用/外层适配实现，不等待未来Node新增隐式方法。

## 6. 关闭地址与阶段证据（ND-RESOURCE-004）

source/resource原Pre地址从reserve开始一直保留；Source Bound/Resource Bound只在实际发生时存在，不作为所有清理的通用入参。完整stage矩阵、SourceClosingContext.collect/IoClosureEvidence、seal/quiescence及record_source_closed(Pre,SourceClosureProof)/record_resource_state(Pre,ResourceStateProof)定义见closure-contract.md。不会为close调用本端口prepare/start/initialize/bind_setup/resume。

新cancel_resume/probe_progress只服务实际ReservedZero证据：转发同原ACP Reservation.cancel→WriteProbe.progress，消耗原Reservation而不造Ticket；已经enqueue只等同原Ticket，Unknown/Written不得变Zero。TargetWorkSeal阻后续第二enqueue，未BoundResource的共享目标可安全放弃尚未load管理reservation（具备该附件完整证明时），source其它目标继续。

共享target真正协议close由新增reserve_target_close/enqueue_target_close调用同原Connection的Strategy::close→reserve_request→原Reservation.encode/enqueue，CloseReservation为检查kind/scope/SomeRPC的私有薄包装；不另开连接。CurrentTargetCloseBinding由当前resource owner在实际source Scope/原Pre目标/旧资源代际和有效安全关闭授权上mint，无完整新setup要求。缺实际版close能力返回Unsupported并保归属，不kill其它target。原Closed receipt必须带原typed RPC并与该CloseOperation匹配，WriteTicket Written与该版可靠目标收敛语义共同才可TargetNativeConverged；裸write或Closed字样不独自证明完整树归零，Shared Source仍拥有整个Job。这个通道不为PreScope源清理制造Scope；PreScope只能真实Owned关闭或Unknown。

观察路由在source Bound后可以用私有ObservationTargetBinding<S>{原PreResource+原BoundSource实际scope+canonical NativeSessionKey/epoch}建立，不要求setup/RB/Loaded；它不授写。这样initialize/load期间真实Permission/State仍可同步处理并保存保护，BoundResourceProbe继续只代表已核完整setup的写准备，不能混作观察资格。已有有效保护会阻UnloadedReservationAbandoned，不因缺setup丢弃请求。


## revision6 控制端口的同版约束

本包 factory/协调器/NativeRuntimeHandle/SourceSlot统一以P:NativeControlPort（继承同NativeLifecyclePort）和N:NativeDispatchRepository+NativeResourceRepository+NativeControlRepository为完整约束；其余关联等式不变。完整ControlReservation/原Permission与Unsupported品牌/ControlWriteBoundary、R3可靠E优先handoff和ClosingAuthority生产构造、同原Reservation.encode/enqueue/Ticket、失败返原物和late-cancel barrier以[control-ports.md](control-ports.md)为权威。纯view/Lease提取没有IO；所有写仍是同P/同实际Connection/原writer。
