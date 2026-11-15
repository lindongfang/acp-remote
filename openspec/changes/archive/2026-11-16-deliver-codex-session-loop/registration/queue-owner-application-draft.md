# Q3 登记账本到共享队列 owner 的应用合同候选

与 `registration-contract-draft.md`、`schema5-registration-draft.sql` 同一个后续独立方案gate；本文件只设计。目标是下一阶段真实接上账本和InputQueue，明确跨SQL/内存边界，不再只证明CAS模型。原生协议发送、终态/资源和WSS最终装配仍按WP3/WP4依赖检视，不用此文件冒签。

## 一个目标一个 owner，SQL不在短锁内

应用按NativeSessionKey复用一个 `QueueOwner<T>`，拥有唯一 `InputQueue<T>`、随机私有owner实例身份、当前RuntimeEpoch、最多一个 `PendingMutation<T>`、原生活动/资源观察引用、节点取消及join句柄。owner不是共享实例全局万能Runtime；审批请求owner、协议reader/writer和资源事实仍各有明确所有者。

具体唯一性：QueueService内部唯一 `Arc<Mutex<HashMap<NativeSessionKey,Arc<OwnerCell<T>>>>>`；resolve_target在锁外完成，map锁内用entry一次创建或取现有owner，不公开QueueOwner::new或替换容器接口。service Clone只clone该Arc，两个调用不能各new owner。同key的target别名必须解析到同map entry；SQL事务再比对CurrentRequestActor所带原生key与product_targets，变化则InvalidTarget，不迁移正文到另一key。不同key各自短锁/pending。生产new为crate私有启动方法，`Storage::start_queue_service(...)`用该Storage的一次启动claim调用它，第二次返回RuntimeAlreadyStarted；成功service关闭前claim不释放。生产还保留DataDirectory独占owner；两个测试pool可竞争账本，但同节点命令必须共享同一service，不能拿第二Storage伪造第二节点绕过目录所有权。

启动精确顺序：Storage完成validate/迁移→申请一次runtime claim→service处于Starting（新动作NodeStarting）→新RuntimeEpoch及空owner map→真实事务interrupt_previous_runtime，把旧applying标interrupted并可靠COMMIT→进入Running。startup提交未知停在Starting，重查旧applying及原事务事实后再完成，不允许注册/consume或从旧正文seed。旧owner实例取消并join；旧SQL回报带旧私有实例身份，即使同target/runtime字段相似也拒绝，绝不重新挂回新map。

PendingMutation用封闭枚举维护状态；每步在同步短锁内取下一条工作意图，释放锁后执行SQL future，再以原owner实例身份和operation ID回来短锁CAS。不能把锁guard带入SQL future，也不从SQL任务回调直接调队列。外部future drop只取消其等待响应；begin提交已确认后的操作由节点任务继续拥有。所有后台任务在owner句柄中跟踪，不spawn脱离owner的首次写任务。

```rust
enum PendingMutation<T> {
    AwaitBegin { key: OperationProbe, body: Option<T>, /* actor/scope, waiter */ },
    BeginUnconfirmed { key: OperationProbe, body: Option<T>, /* 尚未apply */ },
    ReadyToApply { key: OperationProbe, lease: ConsumeLease, body: Option<T> },
    AwaitSettle { key: OperationProbe, lease: ConsumeLease,
        outcome: QueueOutcome, /* removed/replaced T、direct token仅内存保管 */ },
    SettleUnconfirmed { /* 与AwaitSettle相同，但先核验SQL，不再apply */ },
    Faulted { key: OperationProbe, /* 保留已知apply状态及相关内容，不送writer */ },
}
```

这些不是可序列化DTO。具体Rust enum的所有权布局可以精化为内部struct，语义不能合并成“unknown”。AwaitSettle/SettleUnconfirmed本身就是apply已调用过一次的不可复活标记，不能回ReadyToApply；checked本地序号/私有Arc身份防止旧SQL回报在owner重建后生效。序号溢出拒绝新修改但不回绕。

## 最小可执行应用API与查询形状

下一阶段同gate实现下列接口，不由实现者再发明受理点。这里的环境port是可信装配/测试边界，真实协议来源尚待WP3；假authority/环境不能签L5。

```rust
pub enum ValidatedMutation<T> {
    Submit(ValidatedBody<T>),
    Edit { item: EntityId, revision: u64, body: ValidatedBody<T> },
    Cancel { item: EntityId, revision: u64 },
    Prioritize { item: EntityId, revision: u64 }, Resume,
}
pub enum ConsumeReply<T> {
    Resolved { fact: RequestFact, returned: Option<ValidatedMutation<T>> },
    Pending { request: RegistrationId },
    Refused { error: RegistryError, returned: ValidatedMutation<T> },
}
// returned只在该次传入body未被节点采纳/原样退还时Some；Pending不退body、owner继续拥有。
pub enum ProcessingPhase { AwaitBegin, BeginUnconfirmed, ReadyToApply,
    AwaitSettle, SettleUnconfirmed, Faulted }
pub enum QueueRead<'a,T> {
    Stable { queue: &'a InputQueue<ValidatedBody<T>> },
    Processing { phase: ProcessingPhase, applied: bool,
        request: RegistrationId, queue: &'a InputQueue<ValidatedBody<T>> },
}
pub struct ActiveKey { /* private runtime/owner/item/revision/resource; Clone/getters */ }
pub struct OwnedPreparation { /* private同owner身份/DispatchToken/ResourceEpoch */ }
pub struct ExternalWorkKey { /* private owner Arc身份/runtime/resource/checked serial; Clone */ }
pub enum QueueOwnerEvent {
    ActivityChanged,
    Settle { active: ActiveKey, disposition: QueueDisposition },
    ExternalForegroundFailure { work: ExternalWorkKey },
    ExternalForegroundCompleted { work: ExternalWorkKey },
}
pub trait QueueEnvironmentPort: Send + Sync {
    fn readiness(&self, target: &NativeSessionKey) -> DispatchReadiness;
    fn resource_epoch(&self, target: &NativeSessionKey) -> Option<ResourceEpoch>;
    fn poll_event(&self, target: &NativeSessionKey) -> Option<QueueOwnerEvent>;
}
```

QueueEnvironmentPort只有受信实现才能提供真实来源；不能把页面提供的Ready/Settle字段转换为该port。Settle进入owner后必须匹配当前active完整ActiveKey及其本地有效DispatchToken，否则StaleObservation且状态不变；它只表达可信调度决定，不能作为原生结果关联证明。owner的实际InputQueue正文类型为ValidatedBody<T>，其body()/digest()仅借用。ActiveKey只由同owner begin_selection或直接选择生成，公开getter为runtime()/owner()/item()/revision()/resource()，不开放构造器；它不是writer permit。

应用service持有A/V/R/C及 `Arc<dyn QueueEnvironmentPort>`，创建并唯一复用每NativeSessionKey的owner。RequestAuthority增加只读可信 `resolve_target(target:EntityId)->Future<Result<NativeSessionKey,RegistryError>>`，在当前authorize之后调用；repository事务比对同target的source/native，不能由调用者指定第二owner。以下所有async方法返回 `impl Future<Output=...> + Send`。

| 应用方法 | 精确入参/返回 | 说明 |
| --- | --- | --- |
| `Storage::start_queue_service` | `(&self, authority:A, validator:V, clock:C, environment:Arc<dyn QueueEnvironmentPort>, audit:Arc<dyn AuditPort>) -> Result<(QueueService<A,V,SqliteRequestRegistry,C>,NativeQueueDriver<V::Body>),RegistryError>` | 真实Storage工厂，一次claim、生成runtime、创建并驱动startup；内部new不公开。返回句柄不代表Running；startup未知仍返回Starting句柄供核验。第二次调用RuntimeAlreadyStarted。driver私有构造、不反序列化、不作为RPC返回。 |
| `validate_body` | `(&self, raw:&V::Raw) -> Result<ValidatedBody<V::Body>,RegistryError>`（同步） | 应用从受信ValidatedBodyPort结果mint，body/digest无公开可写字段。 |
| `register` | `(&self, actor:&A::Actor, target:EntityId, mutation:&ValidatedMutation<V::Body>) -> Result<RegistrationFact,RegistryError>` | 当前授权、解析target、新随机ID、真实SQL；不调用queue。 |
| `consume` | `(&self, actor:&A::Actor, target:EntityId, id:RegistrationId, mutation:ValidatedMutation<V::Body>) -> ConsumeReply<V::Body>` | 按A–E一次应用；SameID Existing返回原fact及退还本次重复body；确认应用后不返回可重新消费的lease。 |
| `query_request` | `(&self, actor:&A::Actor, target:EntityId, id:RegistrationId) -> Result<RequestFact,RegistryError>` | 当前读取权限；不自动apply或clear_fault。 |
| `startup_status` | `(&self)->StartupStatus`（同步） | `StartupStatus::{Starting,Running,Unconfirmed,Stopping,Stopped}`；只读，不授新动作。 |
| `reconcile_startup` | `(&self)->Result<StartupStatus,RegistryError>` | Starting/Unconfirmed下内部核验旧runtime applying已可靠Interrupted；必要时只补同startup事务，不restore body；成功才Running。 |
| `inspect_queue` | `(&self, actor:&A::Actor, target:EntityId, f:impl for<'a> FnOnce(QueueRead<'a,V::Body>)->U + Send) -> Result<U,RegistryError>` | U:Send。当前读取授权后短锁内同步调用可信投影f；借用不能逃逸，无await/Clone T。f不是原生数据或客户端代码。 |
| `reconcile` | `(&self, target:EntityId, id:RegistrationId) -> Result<RequestFact,RegistryError>` | 节点内部驱动；只使用该owner已持probe/pending。无pending或owner实例不匹配返回NoOwnedOperation，不能恢复旧runtime应用。 |
| `poll_target` | `(&self, target:EntityId) -> Result<(),RegistryError>` | 节点内部处理environment观察/完成及pending SQL回报；不产生网络许可或真实发送，未知观察拒绝。 |
| `shutdown` | `(&self, deadline:RegistryDeadline) -> ShutdownReport` | 不再收新修改，取消/核验/join；报告pending未确认请求ID/各owner是否已join，不含正文。不能把caller drop当正常shutdown。 |

RegistryError另补 `NoOwnedOperation`、`StaleObservation`、`NodeStopping`、`NodeStarting`、`RuntimeAlreadyStarted`；这是应用错误，不新增SQL产品状态。driver错误为 `DriverError::{Registry(RegistryError),Queue(QueueError),SequenceExhausted}`，保留Blocked/StaleToken等精确原因。ShutdownReport公开joined:bool、unresolved:Vec<RegistrationId>、audit_cancelled:u64；joined=false不能宣称所有任务已结束，owner/service继续持有尚未收敛任务责任。RegistryDeadline由应用/节点使用单调Instant和取消构造，既有登记事实无deadline字段。service内部每个SQL任务的返回事件携带owner实例/operation/私有序号，外部不可注入完成事件；故障测试走repository边界真实COMMIT后丢回报，不生产暴露任意setter。

另补NodeStarting、RuntimeAlreadyStarted。SqliteRequestRegistry由Storage::request_registry()返回clone；service内部startup持一次claim，测试两个独立pool的repository实例可以各经同service注入的仓储竞争分支访问同库，不生成第二owner map。RegisteredIntent::new(actor,action,target,item,expected_revision,body_digest)校验封闭形状；RegistrationFact::new(id,intent,register_generation,at)校验generation非负，元数据构造不产生消费资格。ValidatedBody::body()/digest()只借用，受信validator必须冻结T及其摘要，不能通过T的内部可变性在登记后悄悄改内容。

合法旧active测试前提通过同一个真实owner的node内部driver建立，不导入/私写queue，不新增测试setter。driver持service同一注册表和owner身份，只允许短同步模型交接，不提供dispatch_input正文、first-write permit或writer调用：

| driver方法（均async，结果Future+Send） | 精确签名 | 围栏 |
| --- | --- | --- |
| prepare | `(&self,target:EntityId)->Result<Option<OwnedPreparation>,DriverError>` | 同owner无pending、Running，先处理已到观察，取当前可信readiness，再queue.prepare保位。 |
| release | `(&self,preparation:&OwnedPreparation)->Result<(),DriverError>` | owner/runtime/token精确匹配后release_preparation；不能回收新准备。 |
| begin_selection | `(&self,preparation:&OwnedPreparation)->Result<ActiveKey,DriverError>` | 同短边界无pending、最新观察/资源/Ready，再queue.begin_send；返回私有构造的ActiveKey，重复调用拒绝。仅本地独立active选择，未授原生发送。 |
| observe | `(&self,target:EntityId,event:QueueOwnerEvent)->Result<(),DriverError>` | 可在任何pending阶段处理真实受信来源的观察；Settle精确匹配active；ExternalFailure按下文去重；不等SQL。 |
| open_external_work | `(&self,target:EntityId,resource:ResourceEpoch)->Result<ExternalWorkKey,DriverError>` | 当前受信resource匹配且无尚未结束external key，短锁checked递增serial mint opaque key；不造输入/Run，不授writer许可。 |

生产R3没有WP3 writer/持久发送资格时，环境readiness一律Blocked。L4受控环境明确模拟“具备可即时开始的调度条件”，先用真实service登记/consume W，再通过driver.prepare/begin_selection建立W本地active，之后真实service受理X/Y、D和Resume；没有任何原生字节发送，不能把该夹具叫实际writer。失败三挂起新D的Ready也只在此明确受控环境下测试，选中后SQL等待仍保独立active，E后不沿旧Ready自动写出。下一WP3可在prepare与begin_selection之间可靠保存零发送准备事实，在active选择后可靠保存允许首次写事实，再另grant正文/传输；当前driver没有那些能力。

无平台Run的原生W单独覆盖：本gate增加纯队列 `suspend_waiting(&mut self)->usize`，将**此刻**全部等待项挂起、清prepared和priority、置failure_suspended，返回新挂起数量；保留项/顺序/正文/revision/editor，不造Run、不清active、不证明原生已经结束，checked序号不回退。已有settle(token,Suspend)释放该精确active后复用此方法。同步方法不产生SQL/时钟/actor依赖。新增源写范围仅input_queue.rs及开发测试，旧39断言作为原快照回归，新增用例/预审后再实施。

ExternalWorkKey只有driver.open_external_work能mint，不接受调用方自报serial；getter仅runtime()/owner()/resource()/serial()。它绑定私有owner Arc身份、runtime、资源代际和checked本地serial；同资源新工作须取得新key，不猜最近Run。已有未结束external key时open返回Busy，不用新key覆盖可能活动的旧工作；serial耗尽状态不变，不回绕。可信协议owner须只在能可靠识别外部前台时开key，真实归因仍WP3；受控测试提供相同语义来源。

driver收到ExternalForegroundFailure只在key是当前owner同runtime、同当前受信resource的**尚未消费当前external key**时短锁一次suspend_waiting，并消费该key；重复/旧W/旧owner返回StaleObservation且完整状态不变，不在新D进入后再次挂D。Completed只消费同样有效key，不挂起/不恢复队列。消费外部key不清local active、不清待插入、不返资源、不把idle当failure；下一发送仍核对当前真实readiness。资源换代时旧key失效但不从新握手推断旧工作收敛。此围栏只防node迟到/重放，不冒充ACP关联/停止证明；本地单调serial和当前key即可拒绝旧事件，不新增累计ID阈值。D前/后按同owner短锁顺序生效，未知原生活动仍Blocked。

internal步骤接口固定为：`install_pending`（同步B）、`on_begin_result`（同步C）、`apply_once`（同步D）、`on_settle_result`（同步E）、`inspect_and_reconcile`（异步、无短锁跨await）、`on_environment_event`（同步）、`stop_admission`（同步）及 `join_tasks`（异步）。它们只属于service/owner内部，独立测试经公开consume/query/inspect/reconcile/shutdown及repository/environment端口屏障观察，不靠读私有enum或假装执行过SQL。

lease/probe构造路径：应用先保存OperationProbe；repository begin只返回StoredApplying/Existing事实。`on_begin_result`或`inspect_and_reconcile`唯一私有 `mint_lease` 消费同owner的未应用pending，核对StoredApplying全部字段后移入ReadyToApply，故不能二次mint。repository只需getter进行SQL绑定；其外部测试实现可构造元数据事实，不需私有CurrentRequestActor或lease构造器。

## 精确受理步骤

| 步骤 | 短锁内事实 | 锁外动作 | 下一步/失败 |
| --- | --- | --- | --- |
| A 当前认证/结构验证 | 不修改队列 | 可信authority解析本次actor/权限；正文在内存验证并计算固定digest | 验证失败无新消费，正文仍属客户端草稿或本次内存输入；不调用旧MemoryInputs预算 |
| B 确定节点处理顺序 | 目标无其他pending时安装AwaitBegin；保留actor作用域、请求完整意图、正文、operation及probe | 发起begin_consume | pending期间不发此目标新输入；其他目标可继续 |
| C 持久占用请求 | 回报owner/operation匹配且Started时应用mint一次lease转ReadyToApply；同短锁立即执行D，不在C/D之间await或释放锁。Existing只回原事实并清本次pending/退还本次重复正文 | C/D间无锁外动作 | 必要提交已确认才可能继续；旧actor之后撤销不再参与此操作 |
| D 内存应用一次 | ReadyToApply在同一短操作内消费状态、调用对应纯队列方法一次，立刻转AwaitSettle且保存结果 | 发起settle(同lease/结果) | 所有QueueError转固定QueueOutcome::Rejected，不panic、不把泛型正文写SQL |
| E 持久稳定结果 | settle回报匹配且同结果后清pending，发布本次管理回执；移出/被替换正文按内存所有权释放 | 独立best-effort audit | 不把audit成功当受理条件；不因audit失败回滚队列/重跑操作 |

节点顺序以B/D的owner处理顺序为准，不按客户端时间或SQL结束先后重排。已有pending时，后续新操作保留在入口/owner有明确所有者的**尚未受理**等待意图，不能提前答成功或隐藏成第四队列项；不新增无规格依据的“因为SQL忙所以永久拒绝”产品规则。复用现有非正文通道流控处理真实入口压力，不在此添加正文容量上限。对同ID的查询/重试可直接读取applying事实，无须再排入修改队列。

ReadyToApply→AwaitSettle在一次短同步调用内完成，无await、无可取消SQL交接点。纯InputQueue操作无回调，不产生部分外部副作用；意外panic/进程崩溃不能在同owner中捕获后猜测重跑，进入重启语义。正常路径不使用unwrap/expect/panic。

当前可信actor的撤销边界是begin事务内clients核验与COMMIT；登记不是受理授权。D/E均不获取原actor gate、不再require_current。完成后发送响应仍须走当前读取者的通道围栏，响应因撤销被抑制不撤销既有节点操作。

## 五种动作与pure queue的准确对应

- Submit：`queue.submit(QueueInput{id:registration_id,submitter:原登记actor,body}, readiness)`。Queued保存id/revision；Dispatching保存原DispatchToken在owner，结果为DirectSelected。拒绝返回原输入供本次草稿处理，不把它保为隐藏第四等待项。正常满额拒绝；失败三挂起仅在可信可即时开始发送的Ready前提下原子取得独立active。
- Edit：`queue.edit(item,expected_revision,本次actor,new_body)`；原submitter/位置不变，editor和revision更新。成功回执记录新版本，旧body只内存持有至E后释放。重试Existing不再次edit，即使新正文相同也不推进版本。活动项DispatchStarted，不自动中断。
- Cancel：`queue.cancel(item,expected_revision)`；保留被移出QueueEntry至E；成功后不可能沿该项旧prepare发送。活动项DispatchStarted；不转换成原生中断。
- Prioritize：`queue.prioritize(item,expected_revision)`；只改已有可执行项的选择，挂起项仍须显式Resume。E前不能为该意图发原生中断；E后由独立原生owner确认旧前台结束再发送，旧中断不关联新输入。新优先选择使旧准备token失效。
- Resume：`queue.resume()`一次，保留恢复项数，非挂起新项在前、旧挂起相对顺序在后；不复制/重新受理旧正文、不重验原submitter、不抢占active。相同登记重试只回原恢复事实，不再调用resume。

所谓readiness必须由实际资源/前台/待插入/必要存储与writer owner提供；客户端bool、纯模型构造器及登记成功均不是Ready。若WP3发送端口未接好，此阶段的真实应用测试可以明确用Blocked/可控owner证明入队受理，**不能宣称真实异常满三挂起直发已可用**；该分支必须在最终发送装配中补验，不能永久删掉已确认产品能力。

## pending期间的队列和控制规则

采用整目标的**调度围栏**：任何pending状态存在时，不发该目标新的prepare/begin_send，不对外发布pending修改为已稳定成功。D之后没有保留旧body快照；inspect_queue具体返回Processing{applied:true,current queue借用}，其中整个当前视图明确暂定，不能包装为稳定全快照。D之前返回applied:false同样标明操作处理中；无pending才Stable。这样无需Clone T，不丢已知内存应用事实，也不把原/新混报为稳定结果。此围栏只约束新输入/队列变更，不让reader等SQL。

协议reader继续接收，原生活动/资源可以更新；审批、中断与安全关闭各走独立owner和当前授权，不等待普通队列pending。**可信active settle始终可在owner短锁即时消费，不等pending的SQL/E完成**。所有管理应用、SQL完成及观察以同owner短锁确定处理顺序；C/D在同一短边界，D前先消费已经进入该owner待处理队列的全部可信观察，再读当前readiness，不沿A/B的Ready跨SQL使用。观察来源入队不持owner锁等SQL；poll_target在任意pending阶段均可短步处理，不以pending为由buffer。

因此W失败先处理、D后应用时，先Suspend当时旧X/Y，再把D作为失败后新项处理；D先应用、W失败后处理时，D确实处于失败当时的等待集合，按真实当时状态Suspend。Resume同理：失败→Resume恢复旧项，Resume→失败再次挂起当时项。AwaitSettle期间Settle可改变live queue，历史QueueOutcome仍描述D当时的管理事实；Processing视图保持暂定，不需把它改写成原生终态。Settle必须以当前owner/runtime/完整active key对应到服务持有的有效DispatchToken，仅调用该token一次；迟到W不能结算新D。内部处理顺序不是ACP归因证明，不新增原生事件日志/协议序号要求。

pending前已有prepare不立即等于已发送。D若Edit/Cancel/Prioritize/Resume，会按纯模型规则使token失效；若pending为无关操作，E后也必须重新核对当前token/资源。已begin_send的活动项不回等待位，相关Cancel/Edit按DispatchStarted处理。native旧工作完成不会因pending被误报告仍活动，但下一新发送可暂等必要状态稳定。

如果在B之前真正begin_send已先处理，活动身份已确定；后续取消失败。若取消B/D先处理，调度围栏阻止begin_send，取消完成后旧token查不到，零发送。不能持有脱离owner的“已取出待写任务”越过该围栏。

## COMMIT未知与同进程核验

| 状态/持久核验结果 | 可做的恢复 | 绝不能做 |
| --- | --- | --- |
| AwaitBegin提交未知，owner仍是同实例且从未D；inspect读到自身operation/runtime的applying | 确认原COMMIT成功后由应用恢复一次lease，转ReadyToApply | 重新consume生成第二operation，或重验已提交原actor当派发授权 |
| AwaitBegin未知，确认registry仍Registered、没有operation | 无已消费事实；须按当前actor重新授权，之后才可重新begin | 沿旧授权盲重试、把查不到operation当查不到registry而注册旧ID |
| AwaitBegin未知，读到其他operation或意图不一致 | Conflict/故障，清本次未apply内容；已有真实事实保留 | 抢占原请求、借相同ID另执行 |
| AwaitSettle提交未知，读到相同settled结果 | 结束pending并发布原事实 | 再次调用队列方法 |
| AwaitSettle未知，读到原applying且同进程保留已应用结果 | 只重试同lease/同outcome的settle；其COMMIT可靠后解除围栏 | 回ReadyToApply、重复edit/resume、重放正文 |
| 任一核验不可用/结构矛盾 | 保留Faulted/实际不确定范围及安全控制，阻止受影响新输入；显式核验恢复 | 把查询成功自动当健康复位、为回收名额丢账、虚报成功 |

inspect_operation使用节点私有probe而不是原actor凭证，所以actor被撤销不阻断节点收敛。它只核验自己操作，不授权当前客户端读取。未apply的同进程恢复必须有同owner实例和本地状态证明；没有该证明一律不重跑。

## 崩溃、关闭与发送的必需围栏

| 崩溃窗口 | 可可靠说明的事实 | 重启处理 |
| --- | --- | --- |
| register未提交/响应丢失，尚无begin | 没有输入受理 | 不按内容找旧ID；已登记未消费可在新提交、当前权限下首次消费 |
| applying提交，D之前 | 本次管理应用未确认，尚无允许writer的事实 | 旧runtime applying→interrupted，清空queue，不恢复lease/body或D |
| D之后，settle提交前/未知 | 内存可能已改，但没有稳定交付资格 | 同样interrupted，不重跑D，不称nativeUnknown；进程内可按上表核验 |
| settled queued/edit/resume之后，尚未原生发送 | 历史管理结果稳定，正文仅内存 | 队列为空，保历史回执，不恢复正文/专门丢失提示，不再次consume |
| direct active选择后、相关必要提交前 | 独立active占位但不等于字节已交付 | 无writer能力；进程内确定零发送可明确settle回收，不自动回队 |
| 原生发送事实armed之后 | 本文件的两张登记表不足证明零发送/插入/完成 | 必须按下一段的发送必要事实核验；不能统一按管理applying重启规则处理 |

停止顺序：**起始短边界同时停止新修改和新prepare/begin/发送**→取消尚未提交的本地等待→保留/核验已提交pending结果并尽可能settle→通知各原生owner按独立协议安全关闭→取消/终结独立audit任务→join所有本owner任务→释放内存正文与队列。尚未开始begin的caller drop可退还正文；begin future已发出而提交状态未知时即转node-owned pending，不能假设drop令SQL未提交；已可靠begin后caller drop只停响应等待。工程deadline耗尽时保留持久applying/已知缺失范围，不能为赶关闭伪装settled成功；旧未commit或未知SQL连接detach/drop不回池，重启不自动重跑。只清本次owned任务，完整Windows树属于实际process owner另验。

附加audit精确端口：`AuditPort::append(&AuditMetadata)->Pin<Box<dyn Future<Output=Result<(),AuditFailure>>+Send+'_>>`，AuditMetadata只含request/actor/action/target、管理outcome和节点i64事件时刻；AuditFailure封闭为Unavailable/Full/Cancelled。E后释放pending和调度围栏，再由service跟踪的独立任务append；该任务超时/取消通过`take_audit_warnings()->Vec<AuditWarning>`返回可见告警，绝不置必要ledger fault或阻塞下一管理动作。service startup接入`Arc<dyn AuditPort>`，当前真实附加audit保留/清理实现不在本DDL；默认NotConfigured返回Unavailable告警而非虚报保存成功，测试明确注入可控附加audit适配器。shutdown会取消并join其owned任务，不能spawn后忘记。

**发送必要事实接口不能省略。** 最终装配须先冻结独立 `NativeDispatchRepository`（及增量DDL），至少保存dispatch/claim ID、稳定input/item/content revision及当前内容来源操作、原生目标、资源代际、唯一归属、已准备但零发送/已允许首次写/确定未发/可能已发/已写事实；不能从QueueOutcome::DirectSelected或settled推导这些事实。发送claim不以原actor当前状态为条件，不以accepted-Run索引占等待位。批准另须其唯一决定先可靠提交。

普通选择：prepare保原等待位→锁外提交零发送准备元数据→短边界核对token/revision/current ResourceEpoch并begin_send→锁外提交允许首次写的必要事实→owner最后核对资源/取消/归属后交真实writer。任何一步提交不明均不首次写；begin_send后资格变化仍保独立active，不藏第四等待项。已可能写出的未知不得自动重发。准备阶段取消/编辑先发生则准备CAS作废，零发送；begin_send先发生则管理取消/编辑失败。直接3挂起路径从独立active开始进入同一持久写资格阶段，绝不把它降回等待队列来掩盖writer缺失。

writer最后围栏、异步交接取消、插入/终态和资源事实仍是WP3独立方案的必要输入。本阶段可以真实完成**队列受理owner**并在明确发送端口上暂停；没有该发送合同/DDL时不可接实际writer。这是已有WP2→WP3依赖，不能用此条件继续把登记受理拆成无尽纯模型包。

## 同gate独立验证的最小屏障矩阵

真实SQLite+真实InputQueue，屏障覆盖A/B/C/D/E所有边界；同ID并发只apply一次，Edit只加一次revision，Resume重试不再次分组，原actor撤销在C前拒绝、C后D/E照常。settle未知分别给applying和settled核验结果，断线/取消等待方不丢节点pending；owner实例重建拒绝旧SQL回报；每个崩溃窗口重启不复原正文、不重跑D。

人为延迟SQL时reader/审批/中断的独立测试owner仍能消费，pending只围栏本目标输入调度；另一目标受理可并行。取消/编辑先于prepare交接、begin_send先于取消均有原调用计数；发送端口未实现则writer“零调用”只能证明禁止越界，不能代签真实发送。

独立tester定义断言、reviewer预审同一摘要后再编码；正式结果不能勾整个CL-087/092/098/099/112/115的L5/L6。阶段交付同时包含真实owner代码与SQLite证据，不只交假仓储测试。
