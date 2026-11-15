# revision2 + supplement 读取、source与只读观察完整覆盖页

本页完整覆盖revision2同名页；仅替换启动时固定绝对IO选项为逐新操作可信provider，并追加A9/A10封闭fixture/controller ABI，其他规则不变。基线777a455成员保原字节；具体权威范围与门槛见supplement.md。本文签名均待实现；`BoxFuture<'a,T>=Pin<Box<dyn Future<Output=T>+Send+'a>>`。除列明data/probe可Clone外，owner、registration、target/create/replay proof、cursor控制权、Prepared/Queued/Delivered均无公开构造/Default/Deserialize/Clone/setter。所有字段私有；为可观测提供下列getter，不让tester私改字段反选预期。泛型application不导入ACP/SQLite/WebSocket；外层具体issuer明确如下。

## A1. 类型与合法工厂

```rust
pub enum ViewError {
    Unauthenticated, Revoked, InvalidClient, WrongSocket, InvalidTarget,
    InvalidSource, InvalidScope, InvalidConfiguration, Unsupported,
    AlreadyAttached, CaptureUnavailable, InvalidStage, StaleCursor,
    SourceReleased, SourceReplaced, StreamSealed, NodeStopping,
    AllocationFailure, RandomUnavailable, ProjectionFailure, ProtocolFailure,
    ReadFailure, CaptureGap, SequenceExhausted, LengthUnrepresentable,
    InvalidCursor, InvalidCredit, QueueRejected, TransportFailure,
    ShutdownUnconfirmed,
}
pub trait RetainedRaw: Clone+Send+Sync+'static {
    fn len(&self)->usize;
    fn bytes(&self)->&[u8];
}
pub struct ViewRuntimeOptions {
    pub worker_yield_every: NonZeroUsize,
    pub native_read_policy: Arc<dyn NativeReadOptionsProvider>,
}
pub struct SharedViewServices<S,R,A,C> { /* owner, reads, native_read handle */ }
impl<S,R,A,C> SharedViewServices<S,R,A,C> {
    pub fn reads(&self)->&ViewReadService<S,R,A,C>;
    pub fn views(&self)->SharedViews<S,R>;
    pub fn owner(&self)->&SharedViewOwner<S,R>;
}
pub trait CurrentViewAuthority<C>: Send+Sync+'static {
    fn client_binding(&self,client:&C)->Result<ReadClientBinding,ViewError>;
    fn wait_client_change<'a>(&'a self,client:&'a C)
        -> BoxFuture<'a,Result<(),ViewError>>;
    fn with_current<'a,T,F>(&'a self,client:&'a C,target:&'a NativeSessionKey,f:F)
        -> BoxFuture<'a,Result<T,ViewError>>
        where T:Send+'a,F:FnOnce()->Result<T,ViewError>+Send+'a;
}
```

S:Clone+Eq+Send+Sync+'static，R:RetainedRaw，A:CurrentViewAuthority<C>，C:Send+Sync+'static。ReadClientBinding是opaque、可Clone的只读同issuer身份引用，不能从客户端ID字符串构造；它不是跨chunk权限。with_current为静态泛型使用，不要求object safety。生产A=SocketViewAuthority、C=LiveSocketContext；它们由B1真实认证issuer装配，不提供`AlwaysAllow`生产实现。

生产唯一工厂为**新增明确扩展**：

```rust
impl<P,L,G,N> NativeRuntimeHandle<P,L,G,N>
where P:NativeViewProtocol, L:SourceLaunchPort,
      L::Owned:OwnedNativeSource<Streams=P::Streams>,
      G:ConfigurationPort<LaunchPlan=L::Plan,Setup=P::Setup>,
      N:NativeDispatchRepository+NativeResourceRepository+NativeControlRepository {
    pub(crate) fn start_shared_views<A,C>(&self,authority:Arc<A>,options:ViewRuntimeOptions)
        -> Result<SharedViewServices<P::Scope,P::ViewRaw,A,C>,ViewError>
    where A:CurrentViewAuthority<C>, C:Send+Sync+'static;
}
```

它在原native supervisor/同RuntimeCore中创建**独立view supervisor**并保原runtime Handle，构建SharedViews、NativeReadCoordinator、ViewReadService；在同source-registry安装唯一ViewObserverInstall，source启动前完成。没有第二queue claim、SourceSlot或Connection。AlreadyAttached拒绝第二view owner；若source已start且从未capture，返回CaptureUnavailable，不伪首次已有输出全在；原work继续，可按实际能力另行标准重读。node正常装配必须在任何source activation前调用。此方法不是ND6已有方法，须与本文新增bridge同gate实现。

内部共同构造器（不向网络/外部crate开放）：

```rust
impl<S,R> SharedViewOwner<S,R>
where S:Clone+Eq+Send+Sync+'static,R:RetainedRaw {
    pub(crate) fn start_with_ports<A,C>(authority:Arc<A>,
        reads:Arc<dyn NativeReadControl>, options:ViewRuntimeOptions,
        supervisor:ViewTaskSupervisor)
        -> Result<SharedViewServices<S,R,A,C>,ViewError>
    where A:CurrentViewAuthority<C>,C:Send+Sync+'static;
    pub fn handle(&self)->SharedViews<S,R>;
    pub async fn shutdown(&self,deadline:Instant)->ViewShutdownReport;
}
pub trait NativeReadControl: Send+Sync+'static {
    fn request(&self,target:&NativeSessionKey,reason:ReadReason)
        -> Result<NativeReadRequest,ViewError>;
    fn status(&self,request:&NativeReadRequest)->NativeReadMetadata;
}
```

ViewTaskSupervisor仅由原native受监督装配或A6封闭fixture建立，先登记slot再spawn；它不反造native证明。NativeReadRequest只供查询/订阅，不能取得Binding/Reservation/Ticket/allow，退出reader不取消native owner。worker_yield_every只控制每批处理后让出调度，不限制FIFO/log内容数量。

## A2. 读取句柄、getter与错误边界

```rust
pub enum ReadReason { FirstOpen, ExplicitRefresh, Reopen, Reconnect, Reacquire }
pub enum ReadMode { CurrentOnly, ReplayRequested, Replaying, ReplayComplete, ReplayIncomplete }
pub enum Freshness { CurrentObservation, Revalidating, Stale }
pub enum ProjectionState { Available, PartialKnownStructure, ProjectionUnavailable }
pub enum JoinState { ProvenNativeIds, RebuiltNoStableIds, UnassociatedSegments }
pub enum OrderBasis { NativePosition, ConnectionObservation, Unknown }
pub enum RawNamespace { Message, Tool, Unknown }
pub struct ViewDescriptor { /* 仅安全平台metadata */ }
impl ViewDescriptor {
    pub fn view_epoch(&self)->ViewEpoch;
    pub fn read_epoch(&self)->Option<ReadEpoch>;
    pub fn source_stamp(&self)->ViewSourceStamp;
    pub fn revision(&self)->ViewRevision;
    pub fn read_mode(&self)->ReadMode;
    pub fn freshness(&self)->Freshness;
    pub fn projection(&self)->ProjectionState;
    pub fn join(&self)->JoinState;
    pub fn order_basis(&self)->OrderBasis;
    pub fn history_incomplete(&self)->bool;
    pub fn cache_recoverable(&self)->bool;
    pub fn failure(&self)->Option<ViewError>;
}
pub struct OpenedView { /* 同issuer client + stream稳定Arc槽；唯一read owner */ }
impl OpenedView {
    pub fn stream_id(&self)->EntityId;
    pub fn descriptor(&self)->ViewDescriptor;
    pub fn snapshot_cursor(&self)->SnapshotCursor;
    pub fn delta_cursor(&self)->DeltaCursor;
    pub fn state(&self)->ReadStreamState;
}
impl SnapshotCursor {
    pub fn view_epoch(&self)->ViewEpoch;
    pub fn snapshot_revision(&self)->ViewRevision;
    pub fn next_record(&self)->RawRecordId;
    pub fn byte_offset(&self)->u64;
    pub fn exhausted(&self)->bool;
}
impl DeltaCursor {
    pub fn view_epoch(&self)->ViewEpoch;
    pub fn after_revision(&self)->ViewRevision;
    pub fn next_record(&self)->RawRecordId;
    pub fn byte_offset(&self)->u64;
}
```

SnapshotCursor/DeltaCursor是可Clone**只读采样**，不持推进权、无public构造；新API不再接受客户端递回cursor来修改状态。真正cursor保同stream槽，只有B4真实send fact+credit提交一次。ViewEpoch/ReadEpoch/ViewRevision/RawRecordId提供`as_entity_id()`或`as_u64()`只读数据转换，无任意值→可信cursor/epoch的公开反向转换。ViewSourceStamp只含opaque node绑定ID和source配置EntityId，不把Scope指针地址曝网络；cfg(test) SourceInspection可借actual S作正对照。

```rust
pub enum PrepareNext {
    Chunk(PreparedViewChunk), AwaitCredit, AwaitData, SnapshotFinished, Closed,
}
impl<S,R,A,C> ViewReadService<S,R,A,C>
where S:Clone+Eq+Send+Sync+'static,R:RetainedRaw,
      A:CurrentViewAuthority<C>,C:Send+Sync+'static {
    pub async fn open(&self,client:&C,target:&NativeSessionKey,reason:ReadReason)
        -> Result<OpenedView,ViewError>;
    pub async fn next_snapshot(&self,client:&C,read:&OpenedView)
        -> Result<PrepareNext,ViewError>;
    pub async fn next_delta(&self,client:&C,read:&OpenedView)
        -> Result<PrepareNext,ViewError>;
    pub async fn wait_change(&self,client:&C,read:&OpenedView)
        -> Result<ViewDescriptor,ViewError>;
    pub fn release(&self,read:OpenedView)->ReadCloseReceipt;
}
```

open当前鉴权→取capture cut→无gate等worker barrier→重新当前鉴权并核source/epoch→短view操作捕获H+完整只读metadata→登记stream槽AwaitInitialCredit。H快照按≤H版本和原record不可变byte slices，tail只>H，SnapshotEnd不作ReplayComplete；首开不依赖后续增量。snapshot与delta不从调用者参数取H/offset；next_snapshot阶段已结束返回SnapshotFinished，next_delta在snapshot未确认完成返回InvalidStage。无credit返回AwaitCredit且不复制正文；待数据返回AwaitData；source/readepoch失效为StaleCursor/SourceReplaced而不切换generation。每次调用（含无credit/无数据）和每次唤醒后都current验证；复制最多16KiB，在with_current闭包中核client/source/view/stream并锁定一个不可变record版本，等待worker/网络均在其外。

open/barrier、wait_change和等待数据都同时select authority.wait_client_change与source/view_notify；生产authority先订阅同TrustStore.revision_receiver及SocketLifetime关闭信号，再复核client版本，防止撤销在订阅缝丢失。任意trust revision只唤醒重新with_current，未撤销的其它client继续等；本client撤销返回Revoked/InvalidClient，本socket关闭返回WrongSocket/StreamSealed。没有actor gate跨等待，source永久无新增输出时撤销/断socket仍能终止该consumer。A7 local authority同样用真实TrustStore revision通知及fixture lifetime，不用轮询或固定true。

ReadCloseReceipt getter：stream_id、was_already_closed、last_committed_packet、started_packet:Option<u64>、state(Closed|ClosingAfterStarted)；这是实际关闭状态采样，不含native停止/join证明。release消费唯一OpenedView，内部B5 seal_stream；重复网络view.close按stream registry幂等，只得原closed数据，不再制造新的OpenedView。Drop OpenedView也只请求其stream seal，不清node view。

旧source释放/换代后旧cursor/prepared必须拒绝；spec允许释放时cache失效，故newopen不承诺旧raw仍在。有重放则真实重建，无重放则当前可用内容和明显限制；source仍活、只是刷新失败且旧raw仍在时newopen可明确返回Stale descriptor的现有view。任何旧view如仍保留只作已声明旧来源，不静默以其替代current；tester原SV034在v2独立核对“仍保留/已失效”分支，不由作者修改原预期。

PreparedViewChunk只给`metadata()->ChunkMetadata`、`address()->RecordAddress`、`stream_id()->EntityId`、`packet_sequence()->u64`；RecordAddress getters=view_epoch、record_id、kind、byte_offset、total_length、payload_length、is_last。ChunkMetadata另含snapshot H及raw引用来源范围，不含正文。Prepared/Delivered都无public bytes/R getter、Clone、从metadata构造；合法内部sender在B2方法消费。当前reads API返回的错误均不推进cursor：allocation只本读、InvalidClient/WrongSocket拒绝串用、stale禁止旧代际、Closed只已关闭、SequenceExhausted不wrap。

## A3. source capture、路由命令与观察口

```rust
impl<S,R> SharedViews<S,R> {
    pub(crate) fn install_source(&self,registration:SourceViewRegistration<S>)
        -> Result<InstalledViewSource<S,R>,SourceInstallRejected<S>>;
}
impl<S,R> InstalledViewSource<S,R> {
    pub(crate) fn split(self)->(SourceViewCapture<S,R>,SourceWorkerRegistration<S,R>);
}
impl<S,R> SharedViewOwner<S,R> {
    pub(crate) fn supervise(&self,worker:SourceWorkerRegistration<S,R>)
        -> Result<(),WorkerStartRejected<S,R>>;
}
impl<S,R> SourceViewCapture<S,R> {
    pub(crate) fn bind_known(&mut self,binding:VerifiedViewTarget<S>)
        -> Result<TargetBindReceipt,ViewBindRejected<S>>;
    pub(crate) fn register_create(&mut self,create:VerifiedCreateObservation<S>)
        -> Result<CreateWatchReceipt,CreateWatchRejected<S>>;
    pub(crate) fn resolve_create(&mut self,created:VerifiedCreatedTarget<S>)
        -> Result<TargetBindReceipt,CreatedBindRejected<S>>;
    pub(crate) fn unbind(&mut self,seal:VerifiedTargetViewSeal<S>)->ViewSealReceipt;
    pub(crate) fn source_terminal(&mut self,terminal:&VerifiedSourceTerminal<S>)->ViewSealReceipt;
}
impl AcpViewCapture {
    pub(crate) fn capture_received(&mut self,received:&Received)->CaptureOutcome;
}
```

所有Rejected提供`error()->ViewError`和`into_original(self)->原registration/worker/binding/create对象`，无已转移后的伪原物。InstalledViewSource在worker登记成功前不使capture可由production pump运行；安装错误无已start worker，原source IO/Job仍同ND负责。capture producer和registration不可Clone；同node唯一registry接受同actualscope同slot identity的重复安装时返回AlreadyAttached，不新建producer。

capture与bind/create/seal是**同一producer顺序中的ControlMarker**，记录capture当前retained frontier和单调route_version。producer只追加固定metadata/raw引用，原文解析/回填在worker；marker追加真实分配失败用预留fault位记录，不能默默漏bind后宣称全量首开。worker处理Bind(K,routeVersion,cut)时，只回填同source/scope、标准envelope可靠session==K.native_id的暂存records，按原Received.sequence保持一次视图投影；已归其它route/epoch不搬移。bound前记录也有原seq/raw，bind后相同cut内不会漏或重。open barrier必须同时达到requested retained cut和该target binding route_version，因而不能在回填尚未完成时先报空首开。缺来源未配置/未知session拒绝绑定，不按最近target猜。

无session通知保source原文但无任意target读取入口；未管理target标准session事件留source未分配区，只有后来可信同session绑定才能回填；未知结构的raw保留不构成授权。不同Scope、非连续sequence、重复Received分别记录展示故障，不进ND ack/gap。terminal没有seq/receiptorder字段，包含实际source slot identity、actualscope、原terminal_id、真实reason及最后已capture frontier的**view采样**；这不是造Received。

cfg(test)只读观察：`SharedViewProbe::source(&actualScope)->SourceInspection`、`target(&NativeSessionKey)->TargetInspection`、`stream(streamId)->StreamInspection`；Probe由owner.test_probe()借用句柄产生，无setter。Source getters=retained/processed frontier、first_missing_sequence、route_version/processed_route_version、worker actual task state；Target=ViewEpoch/ReadEpoch、anchor count、每anchor namespace/first position/latest revision/rawRecord refs、state axes；Stream=B6状态。源native ID/原raw不在Debug，检验完整body仍走合法chunk重组。不会从probe生成source/target/grant。

## A4. 版本投影精确边界

当前原协议解析输入是 `params.sessionId` 与 `params.update.sessionUpdate`，v1/v2协商版来自同真实source Strategy。实际旧P2只识别v1 `tool_call`初始、双版`tool_call_update`和v2 `tool_call_content_chunk`（后者由worker从raw识别，不能因Strategy::update分Other就丢弃）；本版不把P2尚未解释的v2 `tool_call`或v1 content_chunk编成已知语义，保raw+Uninterpreted。已知同generation `toolCallId` namespace原位锚点；每个字段记录知识状态及原raw片段，未知_meta保全部原片段不深合并。

| 字段 | 已固定的转换规则 |
| --- | --- |
| name/title | update省略Keep；v1 update name:null为Keep；v2 null为KnownAbsent；v1初始name省略KnownAbsent、title必须字符串；v1 update title:null为Uninterpreted；错已知type为ProjectionFailure且raw保留 |
| kind/status | 省略Keep；v1 initial null为投影错，v1 update null为Uninterpreted；v2 null为原已固定null枚举事实；已知字符串按实际kind_code/status_code，v2未知字符串保Known原raw值、typed枚举Unknown和展示限制，不降文本；v1未知值仅该投影失败不伪ACP parser拒绝 |
| content/locations | array保顺序并引用；省略Keep；v1 initial null投影错、v1 update null Uninterpreted、v2 null KnownEmpty；错type投影错 |
| _meta | 省略Keep、对象保raw；null v1 Uninterpreted/v2 KnownEmpty；非对象非null投影错 |
| rawInput/rawOutput | 省略Keep；v1 update null Keep；v2 null KnownAbsent；其余保精确raw片段 |
| v2 tool_call_content_chunk.content | 必须对象且type字符串才解释为原同ID的content append evidence；其余投影失败保raw，不猜合并；_meta同上 |

message族为user_message/user_message_chunk/agent_message/agent_message_chunk/agent_thought/agent_thought_chunk，content按实际数组/块顺序保原结构。v1 messageId缺省/null时**每条原session/update是独立chunk anchor**，不因同role/相邻文本拼消息。v2以及v1实际带非null合法messageId时只按同namespace、同source/scope/read epoch、原版本定义的同message identity关联；`*_chunk`已知content块仅在该可证message内按原接收顺序追加，full message更新保原位置且不复制同已知message。未知body形状/字段不猜完整message或Run/轮次，保raw+知识限制。若原版本规范不承诺某full/chunk替换语义，投影保独立evidence版本并标Uninterpreted，不自发去重。

原native message/tool ID不限4096/32KiB，从raw片段读取比较解码UTF8身份，不进浅控制header；Unicode不正规化、类型namespace不混；已有SessionId4096及RPC标量规则独立保持。原tool field具体省略/null/patch oracle沿捕获root tool_projection.rs `tool_patch`/`assignments`实际规则，只提纯转换，不import ViewWindows/ANCHOR固定载体/旧LRU。独立tester SV023逐项对照这些已捕获字段分支，不能由coder放宽期待。

## A5. NativeRead的完整内部签名

```rust
pub enum ReadResponseWait { At(Instant), Unbounded }
pub struct NativeReadOptions { pub write_deadline:Instant,pub response_wait:ReadResponseWait }
pub enum ReadTransportState { Reserved,Queued,Writing,Written,Unknown,Zero }
pub struct ReadCompletionProof<S,R> { /* 原Replay操作+真实response+原raw */ }
impl<S,R> ReadCompletionProof<S,R> {
    pub fn scope(&self)->&S;
    pub fn read_epoch(&self)->ReadEpoch;
    pub fn target(&self)->&NativeSessionKey;
    pub fn rpc(&self)->&RpcScalar;
    pub fn sequence(&self)->u64;
    pub fn receipt_order(&self)->u64;
}
pub enum ReplayReadStatus<'a,S,R> { Pending,Completed(&'a ReadCompletionProof<S,R>),Incomplete(ViewError) }
pub struct NativeReadStatus<'a,S,R> {
    pub transport:ReadTransportState,
    pub completion:ReplayReadStatus<'a,S,R>,
}
pub enum ReadEnqueueError<R,O> {
    RetainedReservation{error:ViewError,reservation:R},
    RetainedOperation{error:ViewError,operation:O},
}
pub struct ReadCancelError<R> { /* error+同原R */ }
impl<R> ReadCancelError<R> {
    pub fn error(&self)->ViewError;
    pub fn into_original(self)->R;
}
pub trait NativeHistoryPort: Send+Sync+'static {
    type Scope:Clone+Eq+Send+Sync+'static;
    type Raw:RetainedRaw;
    type Binding:Send+Sync+'static;
    type Reservation:Send+'static;
    type Operation:Send+'static;
    fn acquire_read_binding<'a>(&'a self,target:&'a NativeSessionKey,reason:ReadReason)
        -> BoxFuture<'a,Result<Self::Binding,ViewError>>;
    fn scope<'a>(&self,binding:&'a Self::Binding)->&'a Self::Scope;
    fn reserve_replay(&self,binding:&Self::Binding,read:ReadEpoch,options:NativeReadOptions)
        -> Result<Self::Reservation,ViewError>;
    fn enqueue_replay<'a>(&'a self,binding:&'a Self::Binding,reservation:Self::Reservation)
        -> BoxFuture<'a,Result<Self::Operation,ReadEnqueueError<Self::Reservation,Self::Operation>>>;
    fn observe<'a>(&'a self,operation:&'a mut Self::Operation)
        -> BoxFuture<'a,Result<NativeReadStatus<'a,Self::Scope,Self::Raw>,ViewError>>;
    fn cancel_reserved(&self,reservation:Self::Reservation)
        -> Result<ReadTransportState,ReadCancelError<Self::Reservation>>;
}
```

此trait生产实现是 `NativeHistoryBridge<P,L,G,N>`，持原NativeRuntimeHandle/同P与同source registry；具体新增协议方法见C2。Coordinator共同构造精确为：

```rust
impl<H:NativeHistoryPort> NativeReadCoordinator<H> {
    pub(crate) fn new(bridge:Arc<H>,views:SharedViews<H::Scope,H::Raw>,
        supervisor:ViewTaskSupervisor,options:Arc<dyn NativeReadOptionsProvider>)
        -> Result<Arc<Self>,ViewError>;
}
```

它实现NativeReadControl；生产H::Scope=P::Scope且H::Raw=P::ViewRaw，工厂静态保证等式，不downcast。ViewTaskSupervisor可Clone的部分只是同一稳定owner的任务登记handle，不新建监督owner；其唯一关闭责任仍SharedViewOwner。擦除成NativeReadControl只保request/status数据入口，不擦除Scope/Reservation参与的内部协议路径。request按NativeSessionKey建立/订阅同在途operation；FirstOpen/Refresh等均重新验证source/config/能力，不能只返cache冒重读；desired_refresh合并一次后继。Binding只来自真实ND当前scope/resource+完整配置，工厂不接受裸sourceID或Loaded bool。

ReadEpoch只在同协调器新操作登记时由现EntityId随机生成；失败不reserve/写。reserve实际原Replay typed RPC先登记再enqueue，真实early Replayed可在flush前到，完成proof由同pump操作匹配产生并保同Operation稳定槽。observe只借该原proof，因此可重复查Completed且不Clone/重新mint；view仅将其原字段/raw引用交worker，不能获得写/ack能力。worker完成proof以前raw barrier无缺失即ReplayComplete；不等Written，保原Ticket到真实终态。ReadEnqueueError提供error()、into_original()->enum原R/O，无丢owner。cancel_reserved仅同原Reservation真实cancel+probe Zero，不存在cancel_inflight；等待方取消不遗弃original operation/不session/cancel。

## A6. 封闭L3真实ACP source fixture

只在cfg(test)模块提供（外部compile-fail正对照通过独立probe feature只导出fixture facade，不导出issuer字段）：

```rust
pub struct AcpViewModuleFixture { /* real ACP/source/view task slots */ }
pub struct FixtureSourceConfig { pub source:NativeSourceId,pub entry_alias:Arc<str> }
impl AcpViewModuleFixture {
    pub async fn start(config:FixtureSourceConfig,wire:FixturePeerWire,auth:Arc<LocalViewAuthFixture>)
        -> Result<Self,ViewError>;
    pub async fn resume_existing(&self,session:SessionId,setup:SessionSetup)
        -> Result<FixtureTarget,ViewError>;
    pub async fn create(&self,setup:SessionSetup)->Result<FixtureTarget,ViewError>;
    pub async fn replay(&self,target:&FixtureTarget,setup:SessionSetup,options:NativeReadOptions)
        -> Result<NativeReadRequest,ViewError>;
    pub fn views(&self)->SharedViews<ConnectionScope,RetainedAcpRaw>;
    pub fn reads(&self)->&ViewReadService<ConnectionScope,RetainedAcpRaw,
        LocalFixtureViewAuthority,AuthenticatedLocalViewClient>;
    pub async fn open_transfer(&self,client:&AuthenticatedLocalViewClient,
        target:&FixtureTarget,reason:ReadReason)->Result<ModuleViewTransfer,ViewError>;
    pub fn probe(&self)->SharedViewProbe<ConnectionScope>;
    pub async fn shutdown(&self,deadline:Instant)->FixtureShutdownReport;
}
```

FixturePeerWire是自有真实duplex/子进程wire所有者，不是任意Received列表。start实际Connection::spawn(reader,writer)，先将唯一IoOwner/receiver/pump任务登记同fixture稳定slot，再reserve_preferred_initialize→encode→enqueue，唯一EventReceiver.next真实Received进入capture。fixture仅从实际Connection.scope() mint FixtureSourceRegistration，类型分支为ModuleFixture，不能传入生产ND的SourceViewRegistration/ObservationTargetBinding。resume_existing先从原RequestKind::Resume和真实Reservation.id/scope登记已知目标，再enqueue；create从原NewSession请求/真实typedRPC登记无SIDwatch，收到真实NewSession后走C1同两阶段规则。FixtureTarget私有、getters=key/actualscope/origin(ModuleFixture)/originalRPC，不含Loaded=true或伪ResourceEpoch。

replay同样真实Strategy.replay→reserve_request→原Reservation/Frame/Ticket，登记原actualscope/typedRPC/target/ReadEpoch后enqueue；原单EventReceiver取得真实Replayed响应才生ModuleFixture来源的完成evidence，不能从fixture方法参数或raw字面量直接mint。它与production completion相同读取事实算法但绑定origin明确，不能传成生产ND资源/Loaded/ack grant；原Ticket/IO由同fixture监督并join。这样SV042/044/045/048的L3子断言可经真实ACP操作抵达，仍不代L4原ND受监督路径。

fixture配置source EntityId只是受信自有测试命名空间，别名不建第二view key；同fixture source registry复用同key并要求真实原reserve/resume链，原生主资源容量不由L3证明。fixture的shutdown真实join其ACP IO/peer/pump/view任务，owner_kind=TestOwnedStream不说Windows Job0。异scope/重复Received攻击仅cfg(test) `reobserve_from(&mut source,&Received)`调用**同生产capture_received**，它的Received仍来自另一真实Connection；不提供手拼Received或修改state字段。

open_transfer调用同真实next_snapshot/delta，但使用B7封闭的真实duplex模块传输；公开ModuleViewTransfer::next_record()/credit()/detach()只操作实际peer codec，CollectedRecord::bytes()暴露的是该已鉴权client实际收到的字节，不是Prepared或内部R getter。所有本地发送证据类型不同于WSS，不能拿L3此路径签SV063–074的actual WSS分支。

读取fixture不使用AlwaysAllow。L3公开facade的ReadFixtureClient来源/资格按照与独立tester协商的最终签名固定（本版A7）；生产LiveSocketContext完全不同issuer，绝不把fixtureclient转换成生产socket。SV008/053等observed ack/必要SQL只能走实际ND6 L4，不由此fixture代签。

## A7. L3读取资格的封闭正链

```rust
pub struct LocalViewAuthFixture { /* real temp TrustStore/PairingService/ApplicationPolicy */ }
pub struct AuthenticatedLocalViewClient { /* real AuthenticatedActor+fixture lifetime nonce */ }
impl LocalViewAuthFixture {
    pub async fn start(area:OwnedTestArea)->Result<Self,ViewError>;
    pub async fn authenticate(&self,client:&FixtureSigningClient)
        -> Result<AuthenticatedLocalViewClient,ViewError>;
    pub fn authority(&self)->Arc<LocalFixtureViewAuthority>;
    pub async fn revoke(&self,client:&FixtureSigningClient)->Result<(),ViewError>;
    pub async fn close(&self)->Result<(),ViewError>;
}
```

start在其OwnedTestArea用实际NodeRuntime::start(自有NodeConfig/端口,FixtureProtector)建立真实SQLite/身份/配对服务；只在node::shared_view_test_support（cfg(test)或新shared-view-acceptance-fixtures特性）合法借该runtime的pairing/trust字段，不由跨模块public setter取得。authenticate精确沿现node.rs::pair_browser/policy_actor正链：ipc::begin_pairing(runtime.ipc_address())→原PairingService.lookup(code,actual requestId)→自有SigningKey签PairClaim→pairing.claim→实际PairingSession.next_event pending→session.approve→next_event approved clientId；然后runtime.application_policy().issue_authentication_challenge(Some(actual clientId),actual publicKey,nonce)→签AuthProof含原lease.connection_id/challenge字段→真实policy.authenticate。没有直接INSERT trusted或actor私字段拼接，FixtureProtector只服务自有测试密钥，非生产安全证据。

LocalFixtureViewAuthority::with_current持同TrustStore.client_gate并真实require_current_actor，核同fixture lifetime nonce/已登记target来源；revoke由该node子模块持原TrustStore调用revoke(自有owner action identity,新真实requestId,原clientId)，实际事务/COMMIT/同gate不替换。client_binding由该同一fixture认证成功后生成local-client nonce，不叫socket身份，跨fixture拒绝。close沿ipc::stop+原NodeRuntime.wait_for_stop真实join并关闭自有Storage，不拿Drop冒关闭。L3因此包含真实小型auth存储/节点owner，但不当已验L5 WSS；所有LiveSocketContext/跨socket/actual sender断言仍走L4/L5实际WSS。新fixture特性默认关闭，只导出facade合法正链及只读数据，不导出source/proof/actor issuer，Cargo挂载与ND共享文件合入串行复验。

## A8. 状态及关闭getter的精确签名

```rust
pub enum ViewTaskState { Registered,Running,Sealing,Joined,JoinUnconfirmed }
pub struct ViewShutdownReport { /* 同closure operation真实观察 */ }
impl ViewShutdownReport {
    pub fn capture_sealed(&self)->bool;
    pub fn worker_joined(&self)->bool;
    pub fn readers_joined(&self)->bool;
    pub fn cleanup_joined(&self)->bool;
    pub fn pending_native_reads(&self)->usize;
    pub fn failure(&self)->Option<ViewError>;
}
impl ReadCloseReceipt {
    pub fn stream_id(&self)->EntityId;
    pub fn was_already_closed(&self)->bool;
    pub fn last_committed_packet(&self)->u64;
    pub fn started_packet(&self)->Option<u64>;
    pub fn state(&self)->ReadStreamState;
}
impl<S> SourceInspection<S> {
    pub fn scope(&self)->&S;
    pub fn retained_frontier(&self)->u64;
    pub fn processed_frontier(&self)->u64;
    pub fn first_missing_sequence(&self)->Option<u64>;
    pub fn route_version(&self)->u64;
    pub fn processed_route_version(&self)->u64;
    pub fn task_state(&self)->ViewTaskState;
}
impl TargetInspection {
    pub fn descriptor(&self)->ViewDescriptor;
    pub fn anchor_count(&self)->usize;
    pub fn anchor(&self,id:AnchorId)->Option<AnchorMetadata>;
}
impl AnchorMetadata {
    pub fn namespace(&self)->RawNamespace;
    pub fn first_position(&self)->u64;
    pub fn last_revision(&self)->ViewRevision;
    pub fn raw_records(&self)->&[RawRecordId];
}
impl NativeReadRequest {
    pub fn request_id(&self)->EntityId;
    pub fn read_epoch(&self)->Option<ReadEpoch>;
}
impl NativeReadMetadata {
    pub fn read_epoch(&self)->Option<ReadEpoch>;
    pub fn read_mode(&self)->ReadMode;
    pub fn transport(&self)->ReadTransportState;
    pub fn failure(&self)->Option<ViewError>;
}
```

shutdown未真实join返回对应false/JoinUnconfirmed及失败类别，返回report不代表成功；源IO/Windows Job状态没有这些view报告字段，不可据此推native归零。Source/TargetInspection只在cfg(test)/fixture facade提供；生产HTTP只从已授权read.descriptor取得必要metadata，不提供全source全target未鉴权枚举。只读data返回bool不构成接收bool铸grant。

## A9. 每个新实际读取操作的可信IO预算（SV-PLAN007）

`Instant`明确是与当前ACP一致的`tokio::time::Instant`。NativeReadOptions仍保持绝对write_deadline、At(Instant)|Unbounded，**不改其原协议映射**；只将A1服务选项与A5 Coordinator::new的启动时固定NativeReadOptions替换为可信provider。open/request的客户端DTO不接deadline，客户端不能延长已在途操作。

```rust
pub struct NativeReadStartContext { /* 同协调器新operation实际取样，字段私有 */ }
impl NativeReadStartContext {
    pub fn target(&self)->&NativeSessionKey;
    pub fn reason(&self)->ReadReason;
    pub fn read_epoch(&self)->ReadEpoch;
    pub fn now(&self)->Instant;
}
pub trait NativeReadOptionsProvider: Send+Sync+'static {
    fn for_new_operation(&self,context:&NativeReadStartContext)
        -> Result<NativeReadOptions,ViewError>;
}
pub enum ReadResponseBudget { After(Duration), Unbounded }
pub struct MonotonicReadOptionsProvider { /* 可信配置Duration，不含启动Instant */ }
impl MonotonicReadOptionsProvider {
    pub fn new(write_wait:Duration,response:ReadResponseBudget)->Self;
}
impl NativeReadOptionsProvider for MonotonicReadOptionsProvider {
    fn for_new_operation(&self,context:&NativeReadStartContext)
        -> Result<NativeReadOptions,ViewError>;
}
pub struct NativeReadBudgetSelection { /* 同operation已选不可变参数采样 */ }
impl NativeReadBudgetSelection {
    pub fn read_epoch(&self)->ReadEpoch;
    pub fn selected_at(&self)->Instant;
    pub fn write_deadline(&self)->Instant;
    pub fn response_wait(&self)->ReadResponseWait;
}
impl NativeReadMetadata {
    pub fn selected_options(&self)->Option<NativeReadBudgetSelection>;
}
```

MonotonicReadOptionsProvider每次只计算`context.now().checked_add(write_wait)`及After时的独立`checked_add(response duration)`；Unbounded原样。表示溢出返回ViewError::ReadFailure，不截断/换Unbounded/重置clock。Duration来自可信节点调度配置，不在本合同设统一秒数，不新增正文/队列TTL。测试可用自己的受信provider记录实际context与调用次数，它只能返回等待参数，不能生成Scope/Binding/Received/完成proof。

实际顺序精确如下：

1. request按NativeSessionKey协调；已有在途operation只建立订阅或desired_refresh，**零provider调用**，保原ReadEpoch/typedRPC/已选options/Ticket。waiter重试、页面重连、worker推进均不得重新取预算。
2. 真正要开始新operation，先在原supervisor登记operation/ReadEpoch并acquire当前真实binding；不持actor/source/view/ProtocolState锁等待。binding成功、实际能力和配置仍可用后，立即取当时`Instant::now()`构造私有NativeReadStartContext。
3. **每个新operation恰一次**调用provider，成功把返回参数完整存入同operation稳定槽；失败保该读失败事实，尚未reserve RPC。然后仅用此原参数reserve_replay→原encode/enqueue/ResponseDeadline。长binding等待不会消耗一个启动时旧Instant；reserve以后编码/IO实际花费仍计入该次已选预算，不自动续期。
4. 原operation终态后，若desired_refresh确需后继，创建新的ReadEpoch，重新完成binding/步骤2–3；这是新的操作预算，旧终态/旧Deadline不变。没有隐式用新RPC覆盖旧可能已写请求。

Coordinator::new现在持`Arc<dyn NativeReadOptionsProvider>`，而不是NativeReadOptions；ViewRuntimeOptions原IO预算字段替换为native_read_policy，worker_yield_every保持。NativeHistoryPort::reserve_replay(... options:NativeReadOptions)仍是**该次**参数入口，不变。provider只负责此view新Replay操作的write/RPC等待，ND原initialize/load/close的独立IO选项仍归其原owner，不在此重置。A6 fixture.replay每调用本来已有明确一次性NativeReadOptions参数，可用于精确期限/错误分支；其服务层request/open仍走同provider，不能把fixture显式参数误用为服务全生命周期配置。

可达核验：SV038/039/049保持原期待；观察第一次新op只选一次、重复join零调用、第一次终态且其deadline已过以后新op的selected_at/deadline来自新context、后继refresh另取一次。所有参数与原实际P reserve/enqueue映射独立核，不用加大timeout掩盖失败，不需要真实等待多年。

## A10. 封闭fixture参数、所有者与测试controller（SV-PLAN008）

本节仅在`cfg(test)`或默认关闭`shared-view-acceptance-fixtures` facade可调用；不新增管理HTTP/WSS动作，不向生产公开grant/proof/state setter。错误只有静态类别，禁止输出私钥、原生/正文/配置canary。

### A10.1 唯一自有目录及实际任务owner

```rust
pub struct OwnedTestArea { /* 唯一新child+canonical parent/child+owner nonce */ }
pub enum FixtureError {
    InvalidArea, Allocation, RandomUnavailable, Io, Timeout, Cancelled,
    AlreadyAdopted, AlreadyTaken, ControlBusy, Unsupported, JoinUnconfirmed,
}
impl OwnedTestArea {
    pub fn create(parent:&Path,label:&str)->Result<Self,FixtureError>;
    pub fn path(&self)->&Path;
    pub fn area_id(&self)->EntityId;
    pub fn install_tls(&self,certificate_pem:&[u8],private_key_pem:&[u8])
        -> Result<(),FixtureError>;
    pub async fn cleanup(&mut self)->Result<AreaCleanupReport,FixtureError>;
}
pub struct FixtureTaskOwner { /* 捕获原runtime Handle及稳定Arc资源/任务槽 */ }
pub struct FixtureOwnerHandle { /* 可Clone，只指同owner */ }
impl FixtureTaskOwner {
    pub fn new(area:&OwnedTestArea,runtime:tokio::runtime::Handle)
        -> Result<Self,FixtureError>;
    pub fn handle(&self)->FixtureOwnerHandle;
    pub async fn shutdown(&self,deadline:Instant)->FixtureOwnerReport;
}
impl FixtureOwnerReport {
    pub fn pending_tasks(&self)->usize;
    pub fn pending_resources(&self)->usize;
    pub fn all_joined(&self)->bool;
    pub fn failure(&self)->Option<FixtureError>;
}
impl AreaCleanupReport {
    pub fn removed(&self)->bool;
    pub fn pending_resources(&self)->usize;
}
```

create接受测试调用者明确选定parent（如其自有临时目录），canonicalize该parent，仅用安全label前缀+新CSPRNG nonce调用create_dir创建**此前不存在的唯一child**。不接固定全局ROOT、不复用用户目录、label不得成为相对路径。保存canonical parent/child、area_id和私有owner marker；碰撞只选新child，不能清已有目录。失败只清本次已实际创建且路径核对通过的空/自有child；未创建成功无删除动作。

cleanup首先等关联任务/Node/Storage真实关闭（或返回pending/JoinUnconfirmed）；不接受caller传all_joined bool。再核当前canonical child仍为原child且在原parent内、owner marker匹配，拒绝被替换路径/越界/未经支持的symlink或Windows reparse，**只递归删除这个确认过的child**。它不接任意delete路径，不读/删用户DB/原生会话。Drop仅请求原owner停止，不能递归删除或冒join；未关闭资源/数据库时保area与责任以便稍后cleanup。

install_tls只以create_new写本child固定测试cert/key相对位置，不接受其它路径、不覆写原文件；真实格式和匹配由原Node TLS loader核。测试执行者用既有rcgen **dev-dependency**或自有合成PEM生成输入，再调用它；默认关闭的library facade不引入rcgen正常依赖/锁变更。A7 start缺TLS则typed Io/InvalidArea失败，不创建假证书或连接用户已有node；自有listen bind失败按原owner收尾并报错，不把端口探测当保证绑定成功。key正文不进Debug/错误。

FixtureTaskOwner通过area的内部稳定registry登记所有资源槽，资源先adopt再IO/任务poll；首await前保JoinHandle槽，取消等待方不take走句柄。无ambient runtime线程Drop只seal/cancel并用原捕获Handle通知cleanup，不Handle::current、不假join。A7 LocalViewAuthFixture::start消费area后保同registry；close仍按原真实Node/SQL关闭路径，再调用该原area.cleanup，若其它fixture资源尚活返回错误且保area供再次close，不能提前删除。

### A10.2 真实duplex/child原物交接与可驱动peer

```rust
pub struct FixturePeerWire { /* node读写端+原资源lease；无Scope */ }
pub struct FixturePeer { /* 同owner中的实际peer端唯一命令/读取handle */ }
pub struct AdoptRejected<T> { /* error+尚未adopt的原物 */ }
impl<T> AdoptRejected<T> {
    pub fn error(&self)->FixtureError;
    pub fn into_original(self)->T;
}
impl FixturePeerWire {
    pub fn from_duplex(owner:&FixtureOwnerHandle,
        pair:(tokio::io::DuplexStream,tokio::io::DuplexStream))
        -> Result<(Self,FixturePeer),AdoptRejected<(tokio::io::DuplexStream,tokio::io::DuplexStream)>>;
    #[cfg(windows)]
    pub fn from_child_owned(owner:&FixtureOwnerHandle,
        child:crate::platform::process_owner::OwnedProcess)
        -> Result<Self,ChildWireAdoptError>;
}
impl FixturePeer {
    pub async fn write_bytes(&mut self,bytes:&[u8],deadline:Instant)->Result<(),FixtureError>;
    pub async fn next_node_frame(&mut self,deadline:Instant)
        -> Result<Option<Vec<u8>>,FixtureError>;
    pub async fn close(&mut self,deadline:Instant)->FixtureOwnerReport;
}
#[cfg(windows)]
pub enum ChildWireAdoptError {
    BeforeAdopt(AdoptRejected<crate::platform::process_owner::OwnedProcess>),
    Adopted { error:FixtureError, cleanup:FixtureResourceLease },
}
impl FixtureResourceLease {
    pub async fn shutdown(&self,deadline:Instant)->FixtureOwnerReport;
}
impl AcpViewModuleFixture {
    pub fn scope(&self)->ConnectionScope;
    pub fn resource_owner(&self)->FixtureOwnerHandle;
}
```

from_duplex调用者可直接传`tokio::io::duplex(nonzero_capacity)`原tuple；两端move进该owner稳定resource slot，node端实际split后交A6 Connection::spawn的reader/writer，peer端实际split由FixturePeer命令handle借用。先fallible预分配/登记槽，未adopt错误返同两原流；adopt后不再用可失败的新构造丢归属。peer.write_bytes向真实另一端写原wire（可含故意不合法数据），next_node_frame按真实LF读取node原wire，不伪Received；没有平台32MiB帧预算。peer.write完成不当实际reader receipt，仍由唯一EventReceiver.next判源事件。peerDrop只请求关闭其端，原owner保任务/端点清理，fixture.shutdown仅关自身resource lease，不误停同owner的其它source。

Windows child先由实际OwnedProcess::spawn(ProcessSpec,deadline)得到原owner，from_child_owned同步adopt到同稳定fixture slot后才原take_io()一次；stdin/stdout move进ACP，stderr由同slot的独立drain任务保留，不丢给调用者无主。预分配失败BeforeAdopt返原OwnedProcess；adopt后take_io/安装失败只返Adopted cleanup lease，原owner/已取IO仍同slot负责真实shutdown。AlreadyTaken不伪造新IO/Scope。原OwnedProcess.shutdown按总deadline实际kill解pipe、实际Job active_processes/原ACP IO及stderr join分列；root exit不算树0，未确认继续保原slot。duplex报告owner_kind=TestOwnedStream，绝无Windows Job0声明。

FixturePeerWire在构造阶段**没有Scope**；A6 start消费它，在已监督任务中实际Connection::spawn后才由该Connection.scope()产生私有FixtureSourceRegistration；scope()只clone该真实Scope。init/resume/create/replay均原Reservation/Received正链，不能从from_duplex参数、fixtureID或test controller铸Scope/Actor/Loaded/proof。

### A10.3 合成测试密钥的合法构造

```rust
pub struct FixtureSigningClient { /* 自有实际p256 SigningKey，不支持Debug/Serialize */ }
impl FixtureSigningClient {
    pub fn generate()->Result<Self,FixtureError>;
    pub fn public_key(&self)->PublicKeySec1;
}
impl AuthenticatedLocalViewClient {
    pub fn client_id(&self)->&str;
}
```

generate用既有crate::random::bytes和p256::ecdsa::SigningKey::from_slice生成合法自有测试私钥；随机源失败静态RandomUnavailable，非法标量重新取真正随机值，不固定默认key或新增密码学算法/依赖。临时seed用已有zeroize清理，私钥从不输出/Debug；public_key是真实SEC1公钥。clientId只在A7真实Pairing批准/真实AuthProof验证后由AuthenticatedLocalViewClient getter返回，不允许SigningClient先自报Trusted/clientId。错误/构造失败没有fake Actor兜底。

### A10.4 真实暂停与观察控制器

```rust
pub enum ViewHookPoint {
    AfterCaptureEnqueue, BeforeWorkerDetach, AfterWorkerCutProcessed,
    AfterSnapshotH, BeforeSnapshotEnd, BeforeChunkCopy,
    AfterChunkQueuedBeforeWriterGate, WriterGateHeld, BeforeSinkReady,
    AfterStartSendBeforeFlush, AfterFlushFactBeforePublish,
    BeforeCommitCursor, BeforeViewWorkerJoin,
}
pub enum HookSelector<S> { Any, Source(S), Target(NativeSessionKey), Stream(EntityId), Read(ReadEpoch) }
pub struct ViewTestController<S> { /* 只指当前实际owner的cfgtest controller */ }
pub struct ViewPauseGate<S> { /* 唯一arm_id+同controller稳定槽 */ }
pub struct ViewObservationWatch<S> { /* observe-only arm，不暂停原任务 */ }
pub struct HookObservation<S> { /* 实际命中只读metadata */ }
impl<S> ViewTestController<S> {
    pub fn arm_pause(&self,point:ViewHookPoint,selector:HookSelector<S>)
        -> Result<ViewPauseGate<S>,FixtureError>;
    pub fn observe_once(&self,point:ViewHookPoint,selector:HookSelector<S>)
        -> Result<ViewObservationWatch<S>,FixtureError>;
}
impl<S> ViewPauseGate<S> {
    pub async fn arrived(&mut self)->Result<HookObservation<S>,FixtureError>;
    pub fn release(self);
}
impl<S> ViewObservationWatch<S> {
    pub async fn arrived(&mut self)->Result<HookObservation<S>,FixtureError>;
}
impl<S> HookObservation<S> {
    pub fn point(&self)->ViewHookPoint;
    pub fn scope(&self)->Option<&S>;
    pub fn sequence(&self)->Option<u64>;
    pub fn view_epoch(&self)->Option<ViewEpoch>;
    pub fn stream_id(&self)->Option<EntityId>;
    pub fn packet_sequence(&self)->Option<u64>;
    pub fn snapshot_revision(&self)->Option<ViewRevision>;
}
impl<S,R> SharedViewOwner<S,R> {
    pub fn test_controller(&self)->ViewTestController<S>;
}
impl AcpViewModuleFixture {
    pub fn test_controller(&self)->ViewTestController<ConnectionScope>;
}
impl NodeRuntime {
    pub fn view_test_controller(&self)->Result<ViewTestController<ConnectionScope>,FixtureError>;
}
```

所有Controller构造getter仅在本节cfg/test feature下；Node getter只取本实例**已装配的实际view owner**，没有时Unsupported，不能顺便建立SourceSlot/Actor/视图。HookSelector只是匹配数据，不铸其含有的Scope/ReadEpoch；Source必须传真实连接已有Scope，未知选择器永不匹配，不能改源状态。

arm是单次，命中一次才Arrived；同point/selector已有arm返回ControlBusy，不悄替换。arrived取消只取消等待方，gate仍由调用者持有；release消费唯一gate解除一次。gate Drop同步release/disarm并notify，旧arm_id不能释放后来新arm；owner.shutdown也取消所有已arm等待，原任务select owner cancellation，不能因忘release阻关机。ObservationWatch Drop仅撤观察，不改变生产任务。没有public setter/yield/重写seq或ActualFlushFact生成接口。

pause执行点一律在SendRegistry、view/ingress及ND业务锁外；BeforeChunkCopy位于取得actor gate之前，AfterSnapshotH在短view/actor段退出后。WriterGateHeld和AfterStartSendBeforeFlush只允许observe_once，arm_pause对其返回Unsupported；它们的actual actor guard不得被新测试暂停器人为长持。原既有WSS actor-gate专项回归保持，不由本补充改其oracle；实际IO Pending按原机制自然持gate，不伪flush。

**AfterFlushFactBeforePublish**：原actual sink.poll_flush已经Ready(Ok)，先释放actor gate；原Started/fact仍在稳定writer槽，尚未publish Flushed/commit。此时在无registry/actor锁处通知并可pause；peer实际已收到数据后回credit可HoldEarly。release后finish_flush重新核seal/nonce，在同短registry锁发布真实fact/按原规则commit；不从hook伪造Ok。BeforeCommitCursor同样在拿registry锁之前pause，原Delivered/ValidatedCredit留稳定槽，恢复后重新核seal/phase；不能在短seal锁内await gate。

### A10.5 单次命名故障注入（不是系统OOM）

```rust
pub enum ViewFalliblePoint {
    CaptureAppendReserve, RouteMarkerReserve, ProjectionMetadataReserve, ReadChunkReserve,
    NewViewEpochRandom, NewReadEpochRandom, NewStreamIdRandom,
}
pub struct ViewFaultArm<S> { /* single arm_id，匹配一次实际调用 */ }
pub struct ViewFaultObservation<S> { /* 原失败点/实际source/seq/read/stream metadata */ }
impl<S> ViewTestController<S> {
    pub fn fail_next(&self,point:ViewFalliblePoint,selector:HookSelector<S>)
        -> Result<ViewFaultArm<S>,FixtureError>;
}
impl<S> ViewFaultArm<S> {
    pub async fn triggered(&mut self)->Result<ViewFaultObservation<S>,FixtureError>;
    pub fn disarm(self);
}
impl<S> ViewFaultObservation<S> {
    pub fn point(&self)->ViewFalliblePoint;
    pub fn scope(&self)->Option<&S>;
    pub fn sequence(&self)->Option<u64>;
    pub fn stream_id(&self)->Option<EntityId>;
    pub fn read_epoch(&self)->Option<ReadEpoch>;
}
```

仅在新路径命名点前的同实际调用位置消费arm：四个Reserve点返回与新try_reserve失败相同的AllocationFailure；三个Random点在新实际EntityId取样路径返回RandomUnavailable，不创建dummy ID。故障类型由枚举固定，不能传任意Error/正文/数字当失败原因，不能注入原raw parser、全局allocator、ACP receipt或native决定/SQL。no-match不消费arm；matched后触发一次即清除。disarm/Drop只取消尚未触发的同arm，取消triggered等待不修改真实失败；owner停止取消未触发arm并唤醒，旧nonce不影响新arm。

这是诚实验证**受控错误分支**，不是物理OOM/随机设备真实失败，更不是容量quota；不按累计字节/对象数自动触发，不提高旧预算或用回收旧raw“修复”。capture失败仍用预分配首missing_seq槽，projection失败raw仍可读，chunk失败只本read，random失败不mint任何身份。SV054/055及随机失败静态/动态证据分别标注，原83预期不改。Local/WSS两种真实IO收据仍完全按B7/B3隔离，controller没有转换或伪造接口。

### A10.6 合法正编译的最小装配顺序

1. 在测试自有parent调用OwnedTestArea::create，install_tls写自有合成TLS材料；以当前可信runtime Handle构建FixtureTaskOwner并保唯一owner，再将area move给LocalViewAuthFixture::start。
2. FixtureSigningClient::generate→A7实际pair/签proof/authenticate取得AuthenticatedLocalViewClient；零actor字段拼接。
3. `let owner_handle=owner.handle()`；`tokio::io::duplex(capacity)`真实tuple move给FixturePeerWire::from_duplex(&owner_handle,pair)，取得wire+可驱动peer；或Windows将原OwnedProcess move给from_child_owned(&owner_handle,child)（无另一个自行读stdout的reader）。
4. A6 start(config,wire,auth)→真实ACP init，peer通过write_bytes/next_node_frame交互；fixture.scope()仅取真实Scope。controller可arm安全边界/单次故障，arrived/triggered之后才做竞争，最后release/disarm。
5. resume_existing/create/replay/open_transfer全沿原真实源和当前auth；结束fixture.shutdown→peer.close/FixtureTaskOwner.shutdown实际资源join→auth.close实际Node/Storage/area清理。某步错误也保原owner/错误内同原物或cleanup lease，不能靠Drop声称join。

上述是可调用fixture ABI及工程调用顺序，未编译/执行，不能把示意装配当测试通过；ND作者正在编码的workspace不属于本补充写范围。
