# 共享视图完整接口候选 v1

本文件是工程签名，尚未实现/编译。`BoxFuture<'a,T> = Pin<Box<dyn Future<Output=T>+Send+'a>>`。所有正文/配置/远端未知值不进入Error Display/Debug。domain不导入ACP；application泛型R由外层adapter绑定当前RawMessage，Scope=S在生产等于真实ConnectionScope。

## 1. 纯领域数据与原文保留

```rust
pub struct ViewEpoch(EntityId);
pub struct ReadEpoch(EntityId);
pub struct AnchorId(u64);
pub struct ViewRevision(u64);
pub struct RawRecordId(u64);
pub enum ViewOrigin { CurrentConnection, StandardReplay, ResumeWindow, Unassociated }
pub enum StructureKnowledge { Known, Omitted, Null, Uninterpreted, ProjectionFailed }
pub enum ReadReason { FirstOpen, ExplicitRefresh, Reopen, Reconnect, Reacquire }
pub enum ViewError {
    Unauthenticated, Revoked, InvalidTarget, InvalidScope, InvalidConfiguration,
    Unsupported, StaleCursor, SourceReleased, SourceReplaced, NodeStopping,
    AllocationFailure, ProjectionFailure, ReadFailure, ProtocolFailure,
    CaptureGap, SequenceExhausted, LengthUnrepresentable, InvalidCursor, InvalidCredit,
    TransportFailure, ShutdownUnconfirmed,
}
pub trait RetainedRaw: Clone + Send + Sync + 'static {
    fn len(&self) -> usize;
    fn bytes(&self) -> &[u8];
}
```

R::clone必须共享原不可变allocation，不按字节复制/解析；生产私有RawMessage wrapper逐项转发len/bytes/clone，无public raw构造通往view capture。bytes借用只在内部worker或经过当前鉴权的chunk复制使用，不能将R直接返回管理API逃逸读取门槛。sequence/revision用checked_add，耗尽标类型化故障，不能wrap复用。EntityId随机生成失败返回错误，不用固定默认ID。

domain的ItemIdentity区分NativeMessage与NativeTool namespace；ID保协议解码UTF8值且不Unicode正规化，原词法引用仍保留。没有原生ID的条目只有平台AnchorId，不把其字符串称native ID。平台revision/rawRecordId是内部流位置，不外推协议幂等或轮次。

## 2. 唯一真实接收路径的capture

```rust
// 以下能力对象字段私有，不可Deserialize；SourceViewRegistration不可Clone。
pub struct SourceViewRegistration<S> { /* 同实际SourceSlot的来源/Scope/任务身份 */ }
pub struct SharedViews<S,R> { /* Clone只读服务handle，非owner */ }
pub struct SharedViewOwner<S,R> { /* 唯一supervisor及registry所有者，不可Clone */ }
pub struct SourceViewCapture<S,R> { /* 唯一producer, actual Scope, ingress */ }
pub struct SourceViewWorker<S,R> { /* 唯一consumer, source registry, task owner */ }
pub enum CaptureOutcome { Retained, DisplayIncomplete(ViewError), Sealed }

impl<S: Clone+Eq+Send+Sync+'static,R:RetainedRaw> SharedViews<S,R> {
    pub(crate) fn install_source(&self, registration: SourceViewRegistration<S>)
        -> Result<(SourceViewCapture<S,R>,SourceViewWorker<S,R>),ViewError>;
}
impl<S: Clone+Eq+Send+Sync+'static,R:RetainedRaw> SharedViewOwner<S,R> {
    pub fn new() -> Result<Self,ViewError>;
    pub fn handle(&self) -> SharedViews<S,R>;
    pub(crate) fn supervise(&self,worker:SourceViewWorker<S,R>) -> Result<(),ViewError>;
}
// ACP专用外层，唯一生产入口；没有capture(raw,seq,scope)公开重载。
impl AcpViewCapture {
    pub(crate) fn capture_received(&mut self, received:&Received) -> CaptureOutcome;
}
```

SourceViewRegistration只由同ND SourceSlot实际scope绑定之后、start之前的安装路径mint，包含：真实NativeSourceId、actual Scope、同source slot opaque identity、instance/runtime/connection metadata及监督任务注册引用。数据库行/原生短ID不能mint；view模块不能创建ConnectionScope。注册source失败只禁止该source的view，不能丢IoOwner/reader责任或降为假Scope；节点可继续其它已实现独立能力，按实际显示失败。

actual `(Scope, SessionId)`→NativeSessionKey和资源观察绑定由真实ND加载/创建owner在其原操作enqueue前安装。`bind_target`/`unbind_target`仅收该owner的私有`VerifiedViewTarget<S>`，包含同source slot identity、当前原NativeSessionKey/ResourceEpoch、actual scope及原operation身份；不是Loaded写grant，允许加载早到事件观察，但不允许未知target从浏览器自报绑定。该类型最终须与ND正式接口对齐，未有此端口不能装配生产；纯模块测试必须使用真实ACP+隔离可信source fixture显式边界。

capture对同actualscope Received顺序调用一次；错误不能panic，不return控制故障给ND。它保存raw共享引用与真实source sequence/receipt_order；origin/Incoming分类如需额外克隆native ID，留给worker从原raw解释，不在capture复制无限长字段。当前root standalone EventReceiver使用同唯一pump调用capture；未来observed路径借Lease.received()调用，capture从不获得Lease/ack所有权。必须在同原pump范围完成原文retain，而不是另建会遗漏首开已有数据的第二订阅。

后续ND必须提供仅借原文tap：

```rust
// 应用装配端口，非当前root已有方法。
pub trait ViewCaptureTap<E>: Send {
    type Scope: Clone+Eq+Send+Sync+'static;
    fn capture(&mut self, event:&E) -> CaptureOutcome;
    fn source_terminal(&mut self, terminal:&VerifiedSourceTerminal<Self::Scope>);
}
```

生产E=真实Received（或由原Lease仅借出的它）；ND单pump在apply/ack前调用，忽略DisplayIncomplete的控制意义，仅保展示状态。`VerifiedSourceTerminal<S>`字段私有，含actualscope、同source slot opaque identity、原source terminal_id、实际reason(EOF/IO/Protocol/Stopped)；只由adapter从同实际reader Err/None或ND SourceTerminal核scope后mint，没有Received sequence/receipt_order字段。terminal seal按最终已capture frontier drain，不能伪造Received序号；不能用此tap替代ND小事实apply/gap/ack。

## 3. 共享snapshot/read API

```rust
pub struct ViewDescriptor { /* 平台view_epoch, source, flags, revision; 无正文 */ }
pub struct SnapshotCursor { /* actor/connection binding, view epoch,H,scan position */ }
pub struct DeltaCursor { /* actor/connection binding, view epoch,after revision */ }
pub struct OpenedView {
    pub descriptor: ViewDescriptor,
    pub snapshot: SnapshotCursor,
    pub tail: DeltaCursor,
}
pub enum StreamRecordKind { OriginalRaw, ItemProjection, Status, SnapshotEnd }
pub struct RecordAddress { /* view epoch,record id,kind,byte offset,total len */ }
pub struct PreparedViewChunk { /* 私有bytes,address,stream binding,next cursor */ }
pub struct DeliveredChunk { /* same prepared chunk + 原发送成功事实 */ }

pub trait ViewReadAuthority<A>: Send+Sync {
    fn with_current<'a,T,F>(&'a self,actor:&'a A,target:&'a NativeSessionKey,f:F)
        -> BoxFuture<'a,Result<T,ViewError>>
        where T:Send+'a,F:FnOnce()->Result<T,ViewError>+Send+'a;
}
impl<S,R,A,G> ViewReadService<S,R,A,G>
where S:Clone+Eq+Send+Sync+'static,R:RetainedRaw,G:ViewReadAuthority<A> {
    pub async fn open(&self, actor:&A,target:&NativeSessionKey,reason:ReadReason)
        -> Result<OpenedView,ViewError>;
    pub async fn next_snapshot(&self,actor:&A,cursor:&SnapshotCursor)
        -> Result<PreparedViewChunk,ViewError>;
    pub async fn next_delta(&self,actor:&A,cursor:&DeltaCursor)
        -> Result<Option<PreparedViewChunk>,ViewError>;
    pub async fn wait_change(&self,actor:&A,cursor:&DeltaCursor)
        -> Result<ViewDescriptor,ViewError>;
    pub fn release(&self, opened:OpenedView);
}
```

API的cursor/PreparedViewChunk不允许public构造、Deserialize、body bytes直接getter；网关在同crate的封闭发送端消费。cursor与actor+当前客户端连接绑定，使用时还核actual target/scope/viewepoch。release只撤本读handle，不清节点view或取消work。open先当前鉴权，取capture cut并await barrier，再在短view操作下取得H+两cursor；等待期间source换代/释放返回明确错误。next_*每次在with_current内只做固定上限chunk的内存读取/复制和版本核验，不能持gate等worker、遍历全部snapshot或网络；等待数据先释放gate，唤醒后重新鉴权。

`with_current`生产实现复用ApplicationPolicy::require_current_actor及TrustStore::client_gate，在同gate内再次验证actor.auth_generation和当前Trusted；检查target属于已授权配置来源。当前个人节点管理资格不是客户端自报bool；ApplicationPolicy::request_runtime_capability的placeholder不作为真实协议审批或view读取grant。当前范围暂无团队角色，不擅自新增ACL语义。

实际发送还由gateway writer再次取得同client_gate/current_actor围栏，持到该chunk sink.send的真实完成或既有发送超时；队列中已复制bytes不构成发送资格，撤销先完成则丢弃。已开始发送chunk先持gate时可先完成，撤销等待该次固定chunk边界；不能声称撤回已发字节。每次至多预备/排队一个该stream的chunk，不为整body预授权、预排整body或持续持gate。

## 4. 原生重读端口：明确尚须同阶段补齐

```rust
pub enum ReadTransportState { Reserved, Queued, Writing, Written, Unknown, Zero }
pub enum ReadResponseWait { At(Instant), Unbounded }
pub struct NativeReadOptions { pub write_deadline:Instant, pub response_wait:ReadResponseWait }
pub struct ReadCompletionProof<S,R> { /* 私有真实Replay响应/操作匹配及原raw */ }
pub enum ReplayReadStatus<S,R> { Pending, Completed(ReadCompletionProof<S,R>), Incomplete(ViewError) }
pub enum ReadEnqueueError<R,O> {
    RetainedReservation { error:ViewError, reservation:R },
    RetainedOperation { error:ViewError, operation:O },
}
pub struct ReadCancelError<R> { /* 私有error+原reservation，可into_original */ }
pub struct NativeReadStatus<S,R> {
    pub transport: ReadTransportState,
    pub completion: ReplayReadStatus<S,R>,
    // 实际scope/read epoch/typed RPC及完成receipt为私有可信引用
}
pub trait NativeHistoryPort: Send+Sync+'static {
    type Scope: Clone+Eq+Send+Sync+'static;
    type Raw: RetainedRaw;
    type BoundSetup: Clone+Send+Sync+'static;
    type Reservation: Send+'static;
    type Operation: Send+'static;
    type Binding: Send+Sync+'static;
    fn acquire_read_binding<'a>(&'a self,target:&'a NativeSessionKey,reason:ReadReason)
        -> BoxFuture<'a,Result<Self::Binding,ViewError>>;
    fn scope(&self,binding:&Self::Binding)->Self::Scope;
    fn reserve_replay(&self,binding:&Self::Binding,read:ReadEpoch,options:NativeReadOptions)
        -> Result<Self::Reservation,ViewError>;
    fn enqueue_replay<'a>(&'a self,binding:&'a Self::Binding,
        reservation:Self::Reservation)
        -> BoxFuture<'a,Result<Self::Operation,ReadEnqueueError<Self::Reservation,Self::Operation>>>;
    fn observe<'a>(&'a self,operation:&'a mut Self::Operation)
        -> BoxFuture<'a,Result<NativeReadStatus<Self::Scope,Self::Raw>,ViewError>>;
    fn cancel_reserved(&self,reservation:Self::Reservation)
        -> Result<ReadTransportState,ReadCancelError<Self::Reservation>>;
}
```

此端口的Binding不由view构造，实际同ND资源owner mint，必须包含当前actualscope/source/resource、原完整已授权配置、实际协商版及standard replay能力、同目标加载互斥协调引用。获取可能激活/加载source，沿原ND资源容量/管理责任，不占view全局锁，不新开第二原生归属。它不授予后续actor读取权限，不带原actor持续授权门槛。尚无可信完整配置则InvalidConfiguration，只限制相应原生读取；不发空setup。

NativeReadOptions由可信节点调度器为实际写与RPC等待分别选择，write_deadline转原enqueue截止，response_wait逐项转当前ResponseDeadline；不新增统一输入/队列/缓存TTL，不用模型慢判断重发。标准重放传输成本可随完整历史增长，允许Unbounded响应等待；节点关闭/源真实终止仍由监督所有者收敛，不留失主任务。

生产reserve_replay唯一实现为同真实Connection.strategy().replay(real SessionId,&完整SessionSetup)→reserve_request，同typedRPC/scope及RequestKind::Replay在操作registry和view candidate登记后才enqueue；v1真实session/load与v2 replayFrom路径沿当前Strategy，不手写厂商RPC。reservation内部持原实际Reservation、EncodedFrame、只读Probe，失败返回同原线性对象，encode/enqueue失败不遗弃潜伏write。enqueue_replay是owned supervisor交接，首await前登记任务和原reservation；错误RetainedReservation只在实际未交接且原对象存在时返回，若已可能enqueue则返回RetainedOperation，不能返回新造reservation或只保error丢owner。

Operation owns原Ticket/typedRPC/ReadEpoch及响应订阅（该订阅仅接单pump分发的匹配事实，不是第二reader）；transport和completion两轴并行。真实ReplayCompletedProof由adapter从原RequestKind::Replay登记的匹配Received::Response::Replayed生成，含actualscope、typedRPC、目标、ReadEpoch、真实sequence/receipt_order和raw引用；不能由bool/JSON/裸Resumed生成。**observe可在flush尚Pending或后来Unknown时返回Completed**；本view worker处理完proof boundary之前raw后取得完整视图事实，不等Written。Operation仍保未终态Ticket，原ND资源责任继续，不因view完成提前Drop它。

此trait上面签名的`enqueue_replay`错误精确类型为`ReadEnqueueError<Self::Reservation,Self::Operation>`，所有Associated types由同AcpNativeAdapter/source registry具体实现。它是待新增port，ND目前仅有resume port，最终需独立gate同版接口，不能假定已支持。配置/资源加载失败沿ND，不由view伪造Loaded。

在途observe future取消只取消等待方；Operation仍由NativeReadCoordinator监督保留。没有cancel_inflight方法：当前协议未提供无副作用取消重放的通用保证。不能调用执行session/cancel结束展示。预留取消核同原Reservation.cancel的真实Zero；不返回无证据Zero，不对已可能write的operation造新RPC。

## 5. 监督关闭端口

```rust
pub struct ViewShutdownReport {
    pub capture_sealed: bool,
    pub worker_joined: bool,
    pub readers_joined: bool,
    pub pending_native_reads: usize,
    // 全部为实际任务观察；非原生IO/Job关闭proof
}
impl<S,R> SharedViewOwner<S,R> {
    pub async fn shutdown(&self,deadline:Instant)->Result<ViewShutdownReport,ViewError>;
}
```

Drop只seal/取消，不声称join；真实join由同node supervisor保handles直到完成。未终态NativeReadOperation转留同native supervisor，report列pending，不detach或说已取消原生RPC。数据释放在cleanup任务无gate/ack锁下进行，旧用户正文库不属于这些对象，不允许被清理。没有SQL/schema接口，view状态全部非持久。
