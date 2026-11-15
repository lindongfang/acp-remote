# revision6 接口与私有能力的完整形状

本文是 engineering-contract.md 的精确接口补充；签名是待预审合同，尚无实现/编译结论。`BoxFuture<'a,T>`表示 Pin<Box<dyn Future<Output=T>+Send+'a>>；所有错误是语义枚举，不含远端/正文/配置字符串。泛型内层不导入ACP或sqlx。

## 1. 协议无关端口

```rust
pub enum RpcScalar { Integer(i64), String(Arc<str>) }
pub enum ResponseWait { At(Instant), Unbounded }
pub struct DispatchIoOptions {
    pub write_deadline: Instant,
    pub response_wait: ResponseWait,
}
pub trait EncodedPromptFrame: Send + 'static {
    fn byte_len_checked(&self) -> Result<u64, DispatchError>;
}
pub trait ReservedPrompt: Send + Sync {
    type Scope: Clone + Eq + Send + Sync + 'static;
    type Frame: EncodedPromptFrame;
    type Probe: Clone + Send + Sync + 'static;
    fn scope(&self) -> Self::Scope;
    fn rpc(&self) -> &RpcScalar;
    fn probe(&self) -> Self::Probe;
    fn encode(&self) -> Result<Self::Frame, DispatchError>;
}
pub trait PromptTicket: Send + Sync {
    type Probe: Clone + Send + Sync + 'static;
    fn probe(&self) -> Self::Probe;
    fn progress(&self) -> TransportProgress;
    fn request_cancel(&self);
    fn wait(&self) -> BoxFuture<'_, TransportFact>;
}
pub struct EnqueueRejected<R, F> {
    pub error: DispatchError,
    pub reservation: R,
    pub frame: F,
}
pub trait NativeProtocolPort: Send + Sync + 'static {
    type Scope: Clone + Eq + Send + Sync + 'static;
    type Retained: Clone + Send + Sync + 'static;
    type Frame: EncodedPromptFrame;
    type Probe: Clone + Send + Sync + 'static;
    type Reservation: ReservedPrompt<Scope=Self::Scope,Frame=Self::Frame,Probe=Self::Probe>;
    type Ticket: PromptTicket<Probe=Self::Probe>;
    fn reserve_prompt(&self, binding: &SessionBinding<Self::Scope>,
        attempt: AttemptId, body: Self::Retained, response: ResponseWait)
        -> Result<Self::Reservation, DispatchError>;
    fn enqueue(&self, reservation: Self::Reservation, frame: Self::Frame,
        boundary: Arc<dyn DispatchWriteBoundary<Self::Scope>>, deadline: Instant)
        -> Result<Self::Ticket, EnqueueRejected<Self::Reservation, Self::Frame>>;
}
pub trait RetainPrompt<T>: Send + Sync {
    type Retained: Clone + Send + Sync + 'static;
    fn retain(&self, body: &ValidatedBody<T>) -> Self::Retained;
}
pub trait DispatchPollIo {
    fn poll(&mut self, cx: &mut Context<'_>)
        -> Poll<Result<DispatchIoProgress, DispatchIoError>>;
}
pub trait DispatchWriteBoundary<S>: Send + Sync {
    fn check_and_poll(&self, write: &BoundWrite<S>, step: DispatchWriteStep,
        cx: &mut Context<'_>, io: &mut dyn DispatchPollIo)
        -> Result<(), DispatchFenceRejection>;
}
```

生产关联等式：`RetainPrompt<V::Body>::Retained = P::Retained`。EncodedFrame只读长度接口直接读原bytes.len并checked转换，原frame唯一move到enqueue。PromptReservation{actual:Reservation,rpc:RpcScalar}两个字段私有；仅真实Prompt reserve成功后checked创建；`rpc()`全域无失败，因为没有不合法包装实例。control Reservation不能实现这个trait，不改原ACP Reservation的通用语义。scope/ID拷贝仅只读数据，不复制grant。

生产Ticket直接实际WriteTicket或只包它的私有newtype；其唯一所有者是Running，不可Clone/Deserialize。WriteProbe只读可Clone。TransportProgress = Reserved|Queued|Writing{accepted,total}|Flushing{total}|Finished(TransportFact)；TransportFact = Zero{reason}|Unknown{accepted,cause}|Written。ZeroReason与WriteFailure按当前ACP逐项同义映射（包括accepted=0 Io→Unknown）。WriteReport终态不可改变；SQL提交未知另用DispatchError::CommitUnconfirmed，不伪装Ticket Unknown。

BoundWrite<S>字段为scope、typed_rpc、native key、runtime/owner/instance/resource、config identity/revision；由adapter从真实WriteIdentity核scope/RPC/RequestKind::Prompt(session)再附加本job不可变完整config引用生成。Prompt实际WriteIdentity.setup_stamp为None，不从None造配置。Boundary私有实现核同owner实例/active/source/resource/complete config/最新小事实/必要gap，保持owner+resource短guard调用io.poll恰一次；真实协议屏障在随后PollIo进入ProtocolState guard内核，拒绝不调用W。adapter栈上bridge只转发原真实PollIo，不持有另一writer。

## 2. R3可信捕获与线性对象

```rust
impl<T: Send + 'static> NativeQueueDriver<T> {
    pub(crate) fn prepare_capture<'a,R>(&'a self, target: EntityId, retain: &'a R)
        -> BoxFuture<'a,Result<Option<PreparedCapture<R::Retained>>,DispatchError>>
        where R: RetainPrompt<T>;
    pub(crate) fn capture_direct<'a,R>(&'a self, target: EntityId,
        active: &'a ActiveKey, retain: &'a R)
        -> BoxFuture<'a,Result<DirectCapture<R::Retained>,DispatchError>>
        where R: RetainPrompt<T>;
}
impl<P,L,G,N> NativeCoordinator<P,L,G,N>
where P:NativeControlPort, L:SourceLaunchPort,
      L::Owned:OwnedNativeSource<Streams=P::Streams>,
      G:ConfigurationPort<LaunchPlan=L::Plan,Setup=P::Setup>,
      N:NativeDispatchRepository+NativeResourceRepository+NativeControlRepository {
    pub(crate) fn new_job(&self, capture: PreparedCapture<P::Retained>,
        stored: &StoredDispatch, options: DispatchIoOptions)
        -> Result<DispatchJob<P>,OwnedJobError<P>>;
    pub(crate) fn new_direct_job(&self, capture: DirectCapture<P::Retained>,
        stored: &StoredDispatch, options: DispatchIoOptions)
        -> Result<DispatchJob<P>,OwnedJobError<P>>;
    pub(crate) fn select_reserved<'a>(&'a self, job: &'a mut DispatchJob<P>,
        stored: &'a StoredDispatch) -> BoxFuture<'a,Result<(),DispatchError>>;
    pub(crate) fn bind_armed(&self, job: &mut DispatchJob<P>,
        stored: &StoredAttempt) -> Result<(),DispatchError>;
    pub(crate) fn enqueue_job(&self, job: DispatchJob<P>)
        -> Result<RunningDispatch<P>,EnqueueRejectedJob<P>>;
    pub(crate) fn into_known_zero<'a>(&'a self, running: RunningDispatch<P>,
        stored: &'a StoredAttempt)
        -> BoxFuture<'a,Result<SelectedRetry<P>,RunningRejection<P>>>;
    pub(crate) fn next_attempt(&self, retry: SelectedRetry<P>,
        options: DispatchIoOptions) -> Result<DispatchJob<P>,OwnedRetryError<P>>;
}
```

上述均是内部受信路径，管理API不暴露；真实job创建和每次转移在已监督任务内执行。prepare/capture的await只用于resolver取得已登记owner，随后同owner短锁核Ready、drain小事实、无pending，O(1)retain；不await SQL。SourceInstaller已登记pump的owner映射，pump本身不await resolver。新增纯queue只读prepared_input(token)复用require_prepared，不改变状态。

| 私有对象 | 完整持有字段/构造责任 | 可读投影 |
| --- | --- | --- |
| PreparedCapture<B> | 原OwnedPreparation、DispatchPlan、ContentOrigin、B、SessionBinding；同真实owner短锁mint | plan只读；不能取出body/拼接token |
| DirectCapture<B> | 原ActiveKey/原DispatchToken、DispatchPlan、ContentOrigin、B、binding；仅E稳定DirectSelected | plan只读 |
| DispatchJob<P> | 阶段Prepared或SelectedContext，actual Reservation、原Frame、AttemptProbe、NeverEnqueued bool内部状态、options | plan()/attempt()；无public构造/字段setter |
| SelectedContext<B> | 原ActiveKey与DispatchToken身份、plan/origin/B/binding、SelectionStamp、同owner Arc、同调度任务nonce | 无外部构造；不是从DB复制的token |
| RunningDispatch<P> | move来的SelectedContext、唯一actual Ticket、只读Probe、AttemptProbe、终态缓存；原Job已消费 | progress/结果查询无cancel所有权 |
| SelectedRetry<P> | move来的SelectedContext、旧actual终态Zero proof、精确StoredZero、attempt历史 | 只供next_attempt消费一次 |
| AppliedObservation<S> | 同实际scope/sequence/receipt_order、同步各owner处理序、已立必要RequirementId集合、source quarantine结果 | ack核对，不可由SQL/网络生成 |

除data/probe另明确可Clone外，上表能力对象都不可Clone/Deserialize；任何Drop仅保取消/清理责任，不伪Zero或回队列。错误类型OwnedJobError/EnqueueRejectedJob/RunningRejection/OwnedRetryError有error()和into_original()返回同原线性对象；new_job在reserve失败前返原capture，reserve/encode已取得真实对象时Drop在同受监督任务安全取消/收敛，不给调用方拼接grant。into_known_zero必须实际await同Ticket.wait先有终态；拒绝仍返同Running，不把Pending消费丢失。bind_armed只有全字段可靠相同才设Armed。

SelectionStamp在Waiting成功begin_send的同短锁D或原DirectSelected的同D建立；D处理序单调，不是系统时间。SQL未知重试保原stamp，不生成新D。准备期Edit/Cancel使旧Prepared token失效，new_job或select错误保原等待项/编辑结果，旧SQL用NeverEnqueued闭合。

## 3. 仓储及证明接口

```rust
pub trait NativeDispatchRepository: Send + Sync + 'static {
    fn reserve<'a>(&'a self, p: &'a DispatchProbe, c: &'a OperationContext,
        d: &'a RegistryDeadline) -> BoxFuture<'a,Result<StoredDispatch,DispatchError>>;
    fn arm<'a>(&'a self, p: &'a AttemptProbe, s: &'a SelectedProof,
        c: &'a OperationContext, d: &'a RegistryDeadline)
        -> BoxFuture<'a,Result<StoredAttempt,DispatchError>>;
    fn record_transport<'a>(&'a self, p: &'a AttemptProbe,
        f: &'a TerminalTransportProof, c: &'a OperationContext,
        d: &'a RegistryDeadline) -> BoxFuture<'a,Result<StoredAttempt,DispatchError>>;
    fn record_observation<'a>(&'a self, p: &'a ObservationProbe,
        batch: &'a MetadataBatch, c: &'a OperationContext, d: &'a RegistryDeadline)
        -> BoxFuture<'a,Result<StoredFacts,DispatchError>>;
    fn record_convergence<'a>(&'a self, p: &'a ConvergenceProbe,
        c: &'a OperationContext, d: &'a RegistryDeadline)
        -> BoxFuture<'a,Result<StoredConvergence,DispatchError>>;
    fn close<'a>(&'a self, p: &'a DispatchProbe, proof: &'a ReleaseProof,
        c: &'a OperationContext, d: &'a RegistryDeadline)
        -> BoxFuture<'a,Result<StoredDispatch,DispatchError>>;
    fn inspect<'a>(&'a self,p: &'a DispatchProbe,d: &'a RegistryDeadline)
        -> BoxFuture<'a,Result<DispatchInspection,DispatchError>>;
    fn interrupt_previous_runtime<'a>(&'a self, p: &'a StartupProof,
        c: &'a OperationContext,d: &'a RegistryDeadline)
        -> BoxFuture<'a,Result<RecoverySummary,DispatchError>>;
}
```

record_convergence仅接受实际收敛证明：同resource实际scope/owner、convergence_id、kind(JoinedOwnedSource|NativeTargetClosedVerified|OldRuntimeStopped)、验证时间、元数据摘要及真实IO/Job树范围。只有实际resource owner mint，StoredConvergence完整返回；握手/RemoteError/reader失联均不构成收敛。无Received的reader终止使用独立NativeResourceRepository::record_source_loss(SourceLossProbe)，完整字段/类型与禁止release条件见resource-ownership.md §3；从未启动关闭使用close_unlaunched(NeverLaunchedProof) §4，不伪造ConvergenceProbe。

MetadataBatch字段为Observation header及有序Vec<NativeFactMetadata>；header={observation_id,runtime,source_id,instance_id,connection_epoch,sequence,receipt_order,observed_at_ms,batch_fingerprint}，不含target。NativeFactMetadata={fact_index,requirement_id,target_id,owner_id,load_epoch,owner_order,attempt_id?,fact_kind,native_message_id?,result_kind?,stop_reason?,transition,interval_serial?,native_request_id_kind?,native_request_id_bytes?}。body-free规范化元数据编码固定字段顺序、整数BE/字符串长度前缀，batch fingerprint只摘要这些元数据，不hash正文代替正文归档。不同raw但同必要metadata是同事实；raw另外保内存。范围/枚举约束见DDL及主合同。

DispatchError封闭变体：NodeStopping,RuntimeAlreadyStarted,StorageUnavailable,CommitUnconfirmed,CorruptState,Conflict,InvalidTarget,InvalidSource,InvalidScope,InvalidResource,InvalidConfiguration,InvalidPromptReservation,LengthUnrepresentable,StaleSelection,RevisionChanged,NotReady,RequiredStateUnavailable,SequenceExhausted,ProtocolUnavailable,ProtocolRejected,EnqueueRejected,ObservationOrder,ObservationUnapplied,ObservationDropped,ResourceUnconfirmed,Stopped。底层ACP错误按实际code映射保必要分类，不把所有IO失败抹成零；附加审计错误不进必要DispatchError gate。错误Display/Debug只变体，不输出任何raw/config/body/credential。

closure-contract.md对source/resource收尾增加协议无关原Pre寻址与真实阶段证明；它不改变本节dispatch ReleaseProof的严格范围，不接受SourceLoss或空句柄充作收敛。

## 4. 组合factory与真实ACP观察API

工厂与NativeCoordinator的完整P/L/G关联类型及bounds以同包[lifecycle-ports.md](lifecycle-ports.md) §1–3为唯一接口定义：P:NativeControlPort（继承同NativeProtocolPort），L::Owned::Streams=P::Streams，G的LaunchPlan/Setup等式分别绑定L::Plan/P::Setup。R3/Prompt数据端口不变。不能只以P:NativeProtocolPort调用资源生命周期；生产同AcpNativeAdapter/WindowsSourceLauncher/真实配置和SQLite构造链见该附件§4。

为消除首IO先于owner接管的空隙，冻结为以下**唯一**准备API（若主文有闭包表述，以本节签名为精确替代）：

```rust
impl Connection {
    pub fn prepare_observed<R,W,A>(reader:R,writer:W,adopt:A)
        -> Result<(ObservedStart, ObservationReceiver, ObservationAcknowledger,
                   ObservedWriteGate), ObservedPrepareError>
    where R:AsyncRead+Unpin+Send+'static,W:AsyncWrite+Unpin+Send+'static,
          A:FnOnce(IoOwner)->Result<(),IoAdoptionError>;
    pub fn enqueue_observed(&self, reservation:Reservation, frame:EncodedFrame,
        fence:Arc<dyn WriteFence>, deadline:Instant, gate:&ObservedWriteGate)
        -> Result<WriteTicket,EnqueueError>;
}
impl ObservedStart {
    pub fn scope(&self) -> ConnectionScope; // 只读数据，无写权限
    pub fn start(self) -> Connection;
}
impl ObservationReceiver {
    pub async fn next(&mut self) -> Result<Option<ObservationLease>,AcpError>;
}
impl ObservationLease { pub fn received(&self) -> &Received; }
impl ObservationAcknowledger {
    pub fn acknowledge(&self, lease:ObservationLease) -> Result<(),AcknowledgeError>;
}
impl AcknowledgeError {
    pub fn error(&self) -> AcpError;
    pub fn into_lease(self) -> ObservationLease;
}
```

prepare先fallible预分配完整Core/IO/collector槽，再以adopt把唯一IoOwner移入已登记SourceSlot，**在adopt成功前不spawn任务**。adopt拒绝必须返IoAdoptionError{error,owner:原IoOwner}，P/ACP取回同owner仅stop空Core并Drop本地原流，无已运行异步任务；错误可into_parts取回原对象。成功后才向预建槽登记/启动门控任务，内部InstallationGuard在完成前维持installing=true；IoOwner.shutdown必须等待installation_complete，不能在槽尚未赋handle时误报joined。unwind guard同步stop并结束安装状态，已登记source保相同owner/槽继续真实join。无read/write poll，receiver可Pending。ObservedStart不可Clone/Deserialize，Drop stop；SourceSlot必须先move保IoOwner/receiver/ack/gate，登记到组合supervisor，最后consume start token开门（此步无分配、无失败、无await）。同一源只有一个start，Connection clone在开门后才取得。SourceSlot若安装失败仍保该IoOwner直到真实shutdown/join；不能只drop并冒join。原Connection::spawn standalone保持旧API/语义，观察模式不提供可旁路的EventReceiver；旧enqueue拒Observed Prompt。所有新增类型Debug脱敏。

ObservedPrepareError的phase=BeforeAdopt或AfterAdopt，含sanitized AcpError及AfterAdopt installation_id；只由实际构造器产生。BeforeAdopt错误无已启动任务，adopter拒绝通过IoAdoptionError返原owner到构造器处理空Core；AfterAdopt错误不返/复制IoOwner，原SourceSlot已经持它及所有安装槽。P.prepare_source按该真实phase映射PrepareFailure，不用SQL/任意bool猜安装状态。

ObservationLease收到后顺序操作：adapter分类小事实→应用同步apply并立gap→mint AppliedObservation→adapter核lease.scope/seq=proof精确相等→实际ack消费lease。Ack拒绝非next sequence/错Core/重复，不改变frontier并返原lease；drop未ack会停止该source危险写。raw共享不提供第二ack所有者。

## 5. 确定性测试屏障（仅cfg(test)/独立同crate fixture）

不能在生产API加yield、伪造Received/grant或可写状态setter。冻结 `AcpObservationHooks` 作为cfg(test)只观察/暂停器：

- AfterReadCapturedBeforeRoute{scope,receipt_order,held_complete_frames}：真实poll_read已返回且Core.capture在锁内完成，**释放Core锁之后**才await屏障；不持ProtocolState锁睡眠。测试等arrived后才提交Prompt，保证held LF真实存在。
- AfterEmitBeforeLeaseDelivery{scope,sequence}：已emit/routed，receiver尚未交付；只暂停异步receiver，不更改sequence。
- AfterLeaseDeliveryBeforeApply{scope,sequence}：在应用pump调用同步apply前由独立同crate pump hook暂停；事件已pop不等于应用处理。
- AfterApplyBeforeAck{scope,sequence}：已真实小事实+gap插入，尚未ack；释放owner短锁后暂停。
- BeforeActualWriterPoll{scope,typed_rpc,step}：**仅同步计数/try_send见证，不可阻塞/await/回调**；位于同ProtocolState guard内屏障检查后紧邻W.poll。记录lock_epoch递增调试序用于静态+动态证明，无状态改写。

所有hooks只从真实边界调用，permit释放才能继续；关闭可取消等待，Drop不自动ack。arrived通知包含实际边界身份和计数，不记录正文。fixture可用真实AsyncRead/Write控制accepted/flush，但peer.write不是reader receipt证据。ND-TEST001动态验证前四屏障与真实writer次数，TOCTOU在最后点只静态核guard未drop+并发回归，不伪称动态命中不存在的await缝。


## revision6 控制端口的同版约束

本包 factory/协调器/NativeRuntimeHandle/SourceSlot统一以P:NativeControlPort（继承同NativeLifecyclePort）和N:NativeDispatchRepository+NativeResourceRepository+NativeControlRepository为完整约束；其余关联等式不变。完整ControlReservation/原Permission与Unsupported品牌/ControlWriteBoundary、R3可靠E优先handoff和ClosingAuthority生产构造、同原Reservation.encode/enqueue/Ticket、失败返原物和late-cancel barrier以[control-ports.md](control-ports.md)为权威。纯view/Lease提取没有IO；所有写仍是同P/同实际Connection/原writer。
