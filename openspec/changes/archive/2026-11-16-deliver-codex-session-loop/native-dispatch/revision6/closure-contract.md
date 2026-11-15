# revision6 + receipt-supplement：全阶段资源关闭能力与持久入口

2026-11-01。仅工程候选。本合同修复ND-RESOURCE-004；资源记录的**原Pre身份是地址**，真实阶段的私有closure proof是关闭依据，Scope/wire/setup/loadRPC只核确已发生的可选事实。不能为结账prepare/初始化或把Entered改成NeverLaunched。原001–003、Prompt线性、停止政策、Loss独立和真实共享Job义务保持。

## 1. 三类关闭依据与仓储地址分开

- NeverLaunchedProof：唯一LaunchPermit未enter且被不可逆revoke，SQL/quiescence/精确reserved集合；不需/不造任何进程或IO证明。沿原close_unlaunched，不与以下类型互换。
- SourceClosureProof：已enter后，实际拥有资源已真实收敛（或保Unconfirmed、不mint）；带原PreSourceProbe、封闭阶段及精确资源集合。无Scope亦可合法产生；record_source_closed以PreSourceProbe寻址，原子关闭该source及proof中同instance精确资源集。
- ResourceClosureProof：单目标可验证的资源闭合；由SourceConverged、TargetNativeConverged或UnloadedReservationAbandoned三个私有分支构成。record_resource_state以原PreResourceProbe寻址；已Bound只做可选字段比较，不要求为了close新建BoundResourceRecordProof。

SourceLossProbe仍只记录Unknown，不能转以上proof。SQL行/空Option/超时/Err/Drop都不mintclosure。源进程/任务已退出与必要记录可靠是分轴；实际收敛但SQL失败仍由同slot保真实report/原proof和未保存范围，不返slot，不重启/重发。

## 2. 全阶段失败矩阵（真实对象→证据→可调用持久入口）

表中SPre/RPre指从reserve开始就保留的原Pre身份，SB/RB是可选已生成的真实Bound证明；无论位于哪格，close只需原Pre地址。所有SQL确认未知在关闭前由原worker终态+inspect冻结实际已落库字段，不能按本地阶段替数据库补默认。

| 失败边界 | 同slot真实所有者/可选阶段 | 可得真实关闭依据 | 合法SQL入口与拒绝情况 |
| --- | --- | --- | --- |
| reserve_source可靠，resource reserve失败/未知 | SPre；资源列表可能空或有提交未知的RPre；permit未enter | revoke+原reserve worker收尾/inspect+NeverLaunched | close_unlaunched；列表未知/仍有SQL worker则不mint |
| 两reserve可靠→尚未enter | SPre/RPre；仅稳定监督任务 | 同controller不可逆revoke，零未来launch+quiet | close_unlaunched；不填Job0/IOjoin |
| enter已发生→Owned尚未adopt | 原launch future/中间guards归监督任务，SPre/RPre，不能revoked | 只有实际provider cleanup若真实端口能报告；当前未取得可验证报告则EnteredUnconfirmed | record_source_state(Pre,EnteredUnknown)及record_resource_state(Pre,Unknown)，**不Released/Closed**；Err/timeout不当未启动 |
| Owned实际adopt→record_spawned失败/未知 | 原L::Owned已在slot；SPre/RPre；SQL可仍reserved/unknown | 原Owned.shutdown真实收敛；Io::NotPrepared；源work/SQL quiet | record_source_closed(SPre,SourceClosureProof)原子源+精确资源集合；不能要求spawned行先补写 |
| record_spawned可靠→take_streams失败/尚未调用prepare | 原Owned/仍持部分或全部stream，SPre/RPre | 关闭实际原Owned/流，Io::NotPrepared；无Scope | 同Pre source closure；不捏Scope/recordBound |
| prepare在BeforeAdopt失败/拒绝 | 原Owned仍在slot；实际PrepareFailure::BeforeAdopt，流已由原P失败路径终态关闭 | 原Owned真实shutdown+Io::BeforeAdoptRejected；没有IO任务，**不填joined=true** | 同Pre source closure；不能改成NeverLaunched |
| IoOwner已adopt，安装中/prepare AfterAdopt失败 | 原Owned+唯一P::IoOwner/InstallationGuard，同slot；可能尚无Start/Scope给应用 | 封prepare/start；等installation_complete；stop/join原IoOwner+真实Owned shutdown。Io::AdoptedUnstartedJoined | 同Pre source closure；缺安装完成或真实join则Pending/Unknown |
| prepare成功→scope尚未bind | 原Start/Receiver/Ack/Gate+原IoOwner/Owned；SPre/RPre；实际scope仅在Start | Drop/stop原Start、原IoOwner真实join、Owned真实shutdown；Io::AdoptedUnstartedJoined | 同Pre source closure；scope SQL保NULL，不为close先bind |
| Scope已memory Bound→record_source_bound失败/未知 | SB存在但SQL scope_binding_id可NULL或原id；resource仍RPre；Start未开门 | 原IoOwner/Owned真实收敛+quiet后完整inspect；已有id仅比较，不写假id | record_source_closed(SPre,SourceClosureProof)，精确集合含RPre；只核实际落库绑定 |
| bound记录可靠→start/init失败或初始化记录未知 | 原Source/IoOwner/Owned；SB有，resource仍RPre、无wire/setup | 原source关闭授权+stop/join IO/Owned；Io::StartedJoined；原Init Ticket真实终态/必要记录协调 | 同Pre source closure；无需初始化或RB。若尚无真实清理则只Unknown |
| init可靠→配置Unknown/不支持或bind_setup失败 | SB/原Source；本目标仍RPre。source可能还拥有其它Loaded目标 | 若整源可关闭：实际Owned+IO收敛；若source仍供其它目标：本目标未有load IO及原有效保护未归此目标可用UnloadedReservationAbandoned | 整源调用Pre source closure；共享单目标调用record_resource_state(RPre,Released(ResourceClosureProof::UnloadedReservationAbandoned))，**不杀其它目标，不要求RB/setup** |
| setup已Bound→load Reservation/Frame已得但begin_load失败/未知 | RB/原LoadReservation+Frame及NeverEnqueued；SQL可reserved或loading | target工作权先封闭、消费原Reservation取消并得实际Zero，或真实enqueued Ticket终态Zero；未可能接受目标IO才可Abandoned | 单目标Pre resource close，源继续；若已经可能发送则不得走Abandoned，必须实际target或整源收敛 |
| 原load已enqueue→早Resumed/flush Pending/可能写 | 原Load Ticket+probe、SB/RB、Owned/IoOwner；SQL可能loading/loaded/unknown | 真实target close与原load IO终态，或被授权整源实际Job+IO收敛；保可能副作用Unknown | Pre resource close(TargetNativeConverged)或Pre source closure；Pending/可能活动不能released |
| load Written+Resumed→loaded SQL失败/未知 | 原actual receipt/Ticket/原binding均在；SQL不一定loaded | 实际target/整源收敛+旧必要记录可靠，完整inspect | 同Pre入口；不为取得关闭参数再造record_loaded/新loadRPC |
| SQL Loaded→Prompt/审批/后台/丢连接 | 原真实resource/Job/IO及Prompt Ticket等 | 沿既有可靠native隔离close/整源关闭、审批范围、IO与必要记录；Loss单独不够 | 同Pre resource/source入口；原Run/unknown结果不改成功；共享源不得单目标kill |

Io已adopt但安装中可能没有应用可读Scope；原IoOwner由同adopt nonce识别，不以Scope字符串替代。source Bound/resource Pre混合是正常关闭输入，类型签名不额外要求wire/setup。

## 3. SourceClosureProof 的合法产生链

```rust
impl<P,L> SourceSlot<P,L>
where P:NativeControlPort,L:SourceLaunchPort,
      L::Owned:OwnedNativeSource<Streams=P::Streams> {
    pub(crate) fn begin_owned_close(&self,authority:SourceCloseAuthority)
        -> Result<(SourceWorkSeal,SourceClosingContext<P,L>),CloseRejected>;
    pub(crate) fn prove_source_converged(&self,sealed:SourceWorkSeal,
        physical:OwnedCleanupEvidence<P>,quiet:SourceSqlQuiescenceProof,
        inspection:SourceInspection)
        -> Result<SourceClosureProof,SourceClosureRejected<P>>;
}
impl<P,L> SourceClosingContext<P,L>
where P:NativeControlPort,L:SourceLaunchPort,
      L::Owned:OwnedNativeSource<Streams=P::Streams> {
    pub(crate) fn collect(&mut self,deadline:Instant)
        -> BoxFuture<'_,Result<OwnedCleanupEvidence<P>,DispatchError>>;
}
impl SourceSqlSequencer {
    pub(crate) fn seal_source_after_cleanup<'a>(&'a self,
        sealed:&'a SourceWorkSeal,ended:&'a PhysicalCleanupIdentity,
        deadline:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<SourceSqlQuiescenceProof,DispatchError>>;
}
```

CloseRejected保原SourceCloseAuthority和typed错误，普通caller没有这些私有对象。SourceCloseAuthority只由同resource owner验证得到：整节点退出（拥有本节点所有源）、已验证没有其它受保护目标，或所有目标均有明确有效关闭依据。共享source单目标的加载/配置失败不自动取得整源关闭权；单目标只能走§5。身份/归属未知不能杀其它未知进程。

begin_owned_close同slot安装不可逆Closing fence，封新launch/prepare/start/init/load/Prompt/enqueue和该instance新资源reserve，返回不可Clone SourceWorkSeal与由同Arc监督的context；没有迟到permit能在Closing后启动。已在OS/provider调用中的launch由原任务完成/交接，不abort丢句柄。已有IoOwner先实际stop，晚到adopter仍接入cleanup并stop空Core；不能因Stopping拒掉清理owner。

collect借原稳定异步owner槽完成真实Owned.shutdown；若IO已adopt则经原P::IoOwner shutdown等installation_complete后join，Windows必要时先kill Job解pipe，再join读写/stderr（同一总deadline）。普通等待方Drop只丢collect等待，原Owned/IoOwner/report仍在slot，后续继续同对象。不能为清理调用prepare/bind/start/init/setup/load。所有实际生命周期operation保原Reserved/Ticket直到取消/终态，原pump排完已取得小事实后结束；必要记录可继续提交。

OwnedCleanupEvidence<P>字段私有，完整为原PreSource identity/同slot/runtime/instance/config、EnteredLaunch nonce、实际adopted owner identity、SourceWorkSeal identity、阶段transcript、OwnerClosureEvidence、IoClosureEvidence<P>、已归属目标集合及安全关闭授权范围、physical ended identity、节点真实验证时间/元数据fingerprint。

OwnerClosureEvidence = WindowsJob{原OwnedSourceShutdown，真实active_processes=0及stderr实际任务终态}|TestOwnedStream{原fixture任务实际终态}。没有Owned报告就不mintSourceClosureProof；当前Windows launcher若在enter后仅返Err/timeout且不能给真实清理证据，保EnteredUnconfirmed，不能用析构推closed。不得把TestOwnedStream写成WindowsJob0。

IoClosureEvidence<P>为封闭枚举，不能从bool构造：

- NotPrepared{sealed original stage}：从未调用P.prepare_source，原流仍由Owned或同任务终态释放；没有IOjoin字段。
- BeforeAdoptRejected{原PrepareFailure::BeforeAdopt,prepare operation终态}：实际P已关闭其原流/空Core，尚无IO任务；没有IOjoin字段。
- AdoptedUnstartedJoined{原adopt nonce,installation_complete,原P::IoOwner实际joined报告,原Start已Drop或已失效}：确有IO任务但开门权未消费；不能用空任务数组当join。
- StartedJoined{原start/source identity,原实际IoShutdownObservation joined=true,原operation terminal集合/pump ended}：确已开门，不把根process exit当IO完成。

没有Start/Scope时仍能产生前两类；AfterAdopt失败可用第三类而不需伪造Start或SQL绑定。已生成的actual Scope只在内存transcript保持原引用；关闭不需要把它写成scope_binding_id。已存在SQL binding/wire/RPC必须与原actual记录一致；未存在字段保NULL，不能补假值。

物理cleanup后source metadata pump/所有旧SQL操作必须真实结束/核清确认未知，再seal_source_after_cleanup：封未来全部该instance metadata mutation、消费稳定任务槽和最后inspection，mint SourceSqlQuiescenceProof。它含原worker/operation集合、记录结果和gap集合指纹；新reserve/late record_bound/init/loaded在Closing/Closed fence后不能复活行。未可靠必要事实仍有gap时不mint最终SourceClosureProof；实际资源可已停，但持久归属仍Unknown/Pending，不返slot。

prove_source_converged仅同owner/Entered/真实Evidence/完整quiescence/SourceInspection一致才mint不可Clone proof；错误SourceClosureRejected持同sealed/physical/quiet/inspection，可into_original取回，不丢report。proof完整保原PreSource和**全部**同instance源/目标行的精确expected metadata、原已发生可选binding/wire/load字段、已确认必要事实指纹与closure_id/时间。SourceInspection不是证据，只用于把真实清理事实对准原记录。

## 4. 仓储关闭签名与原子事务

完整NativeResourceRepository其余方法不变，以下三项以本revision签名为唯一关闭/失败地址：

```rust
fn record_source_state<'a>(&'a self,p:&'a PreSourceProbe,
    f:&'a SourceStateProof,c:&'a OperationContext,d:&'a RegistryDeadline)
    -> BoxFuture<'a,Result<StoredSource,DispatchError>>;
fn record_source_closed<'a>(&'a self,p:&'a PreSourceProbe,
    f:&'a SourceClosureProof,c:&'a OperationContext,d:&'a RegistryDeadline)
    -> BoxFuture<'a,Result<StoredSourceClosure,DispatchError>>;
fn record_resource_state<'a>(&'a self,p:&'a PreResourceProbe,
    f:&'a ResourceStateProof,c:&'a OperationContext,d:&'a RegistryDeadline)
    -> BoxFuture<'a,Result<StoredResource,DispatchError>>;
```

SourceStateProof仅同slot实际EnteredUnknown/Closing事件，ResourceStateProof=Unknown(真实源/目标失败)|Releasing(当前合法关闭意图)|Released(ResourceClosureProof)；类型均私有，不因地址是Pre就授Released。record_source_state可在实际scope未生成时保存真实启动未知；不能用record_source_loss代替。

record_source_closed的单一BEGIN IMMEDIATE逐项核：原Pre全字段/runtime/instance/config；source当前reserved/spawned/initialized/closing/unknown中的实际行；source proof的完整Resource集合必须与SQL同instance集合一致；所有已经存在binding/wire/RPC/transport/receipt按原inspection精确比，未存在不要求新增。只接受SourceClosureProof真实Owner+Io stage及quiescence，不接受Loss/NeverLaunched/SQL行。

核过后同事务把source置closed、close_basis=owned_converged，保存closure stage/owner/io分类及元数据fingerprint；把该proof覆盖的尚未released新资源置released、release_basis=owned_converged，写同一closure identity/fingerprint和各目标实际阶段。已released资源只在它本来就精确属于该inspection集合且保原release事实时保持原值，不覆盖另一合法closure。不改旧product_resources，不删行，不填缺失scope/wire/loadRPC，不改变任何已知/未知Run或native结果。

StoredSourceClosure完整返回source逐列、资源逐列及closure_id/metadata fingerprint、每行原状态与新状态；COMMIT未知保原proof/时间/ID，inspect全等才确认；同closure相同全字段重试幂等，异proof/旧generation/多漏资源Conflict，closed不重开。record_resource_state(Released)同样比原Pre+其完整expected行及实际closure proof，存真实release依据；不能只以has_scope=false选一条宽松SQL。

物理close不依赖必要SQL恢复才开始；**最终released记账**依赖必要事实和本事务可靠。数据库失败只保实际report及Unknown/Pending，不重新spawn或补协议操作。现有ConnectionOwner lease/真正close所有权不变。

## 5. 单目标ResourceClosure与共享source

ResourceClosureProof字段私有，以原PreResource identity寻址，并含同slot/target WorkSeal、目标SQL quiescence/精确ResourceInspection、closure_id/节点时间、已有可选binding/wire/load字段和元数据指纹。三个分支：

1. SourceConverged：引用同实际SourceClosureProof的该目标投影；通常由record_source_closed原子写全，单目标幂等检查不丢全源集合。不能用它无授权kill共享source。
2. TargetNativeConverged：沿当前版真实会话隔离close/输入终态/有效审批范围及旧资源核验，绑定原真实scope/目标；必要时源保持给其它目标。只能实际发生过相应阶段时提供其证据，不要求新建setup来取得close参数。
3. UnloadedReservationAbandoned：只关闭未发生任何可能目标load/input IO的新管理reservation，不声称原生会话idle/历史完整/后台结束。由同TargetLifecycleController不可逆revoke未来load/enqueue、原operation未enqueued或真实终态Zero、无该资源已接受load/Prompt/有效审批/其它受保护原生活动、目标旧SQL worker quiescent和完整inspection共同mint；不需wire/setup/RB。源及其它目标继续运行。若已可能写、收到可靠load接受、该资源已有保护事实或状态不能核验则拒此分支，保责任直到真实target/全源收敛。

对begin_load提交未知但原LoadReservation仍NeverEnqueued：原target work权先封，调用P.cancel_resume消耗**同一个**Reservation，取真实Probe终态Zero，保原RPC/raw Frame disposed；源scope当前仅用于核实际handle，不把它补入原SQL未记录列。若原Ticket已得到，则只能实际wait同Ticket终态Zero；Unknown/Written不准Abandoned。SQL事务可将原loading+armed行按这个真实ReservedZero写成zero并保存原原因，但绝不从缺Ticket推Zero。

泛型所需增量端口明确为：

```rust
pub trait NativeLifecyclePort: NativeProtocolPort + Sized {
    // 完整关联类型/方法已并入lifecycle-ports.md；此处仅标新增签名
    fn cancel_resume(&self,reservation:Self::LoadReservation)
        -> CancelledLifecycleReservation<Self::Scope,Self::Probe>;
    fn probe_progress(&self,probe:&Self::Probe) -> TransportProgress;
}
```

生产cancel_resume消耗原checked LoadReservation，转发原ACP Reservation.cancel()得到同WriteProbe；返回私有Cancel receipt{scope,typedRPC,probe,kind}，不创建新Ticket/Frame/连接。它只证明原未enqueueReservation无未来poll，TargetWorkSeal仍负责阻止第二操作。Probe只读进度，实际Finished(Zero)才mintReservedZeroEvidence；NoReady或空句柄不能替代。

TargetLifecycleController::revoke_load与已在enqueue之间同owner短锁竞争，成功者取得唯一TargetWorkSeal；封该目标所有后续load/Prompt，不阻其它目标操作。TargetSqlSequencer::seal_after_work等待该load epoch原reserve/begin_load/transport等worker终态并inspect，产生TargetSqlQuiescenceProof；不取全Source seal以免阻别的目标。ResourceSlot::prove_unloaded_abandon(seal,actual NoTargetIoEvidence,quiet,inspection)只核上述真实条件，错误返同原objects，不调用source shutdown。

release_basis=unloaded_reservation_abandoned只解除这次未加载的管理reservation，保已知原生Unknown及独立source保护；不能据此给下一输入写资格。后来真实事件仍按原source实际映射/隔离规则处理，不丢未识别target事件或复活旧load epoch。

## 6. Closing期间的事实与quiescence

SourceWorkSeal封的是新副作用/新reserve资格，不封原operation历史事实保存。实际stop后，原Ticket/receiver/worker先产生真实终态（包括Unknown/未确认范围），必要SQL按原顺序保存/核验；业务短锁不跨等待。持久Closing可以等这些原stage worker处理完再写，不能先把SQL行改成一种使原必要fact永久不可保存的状态。若在Closing/Unknown下补原真实fact，仓储只填该事实的原RPC/receipt/transport字段并保持Closing/Unknown，**不得推进回initialized/loaded或产生Ready**。已有Closed/released行不被迟到worker复活。

未完成init/load不是虚构Initialized/Loaded；原真实中断由SourceStateProof/ResourceStateProof的Unknown分支保存现有真实RPC及可知范围（不存在的字段仍NULL），只在精确覆盖原RequirementId/metadata fingerprint后关闭那个缺口。Missing/native结果不能只因physical cleanup被改成功；未覆盖必要缺口继续Pending，不能mint最终released proof。最终SourceSqlQuiescenceProof在原物理任务/pump结束、旧SQL操作全部真实终态/inspect之后才产生，包含每个必要缺口对应的确切Stored事实或已持久Unknown范围；没有迟到worker可插新行或推进旧生命周期。

同一源整关闭集合和单target放弃集合分别用source/target sequencer，不以全局锁阻其它source。sql COMMIT确认未知仍保原Proof和实际Owner报告，inspect比字段；关闭查询不铸造任何新Scope/Native写资格。

单目标放弃证明的精确构造口：

```rust
impl TargetLifecycleController {
    pub(crate) fn revoke_load(&self)
        -> Result<TargetWorkSeal,TargetSealError>;
}
impl TargetSqlSequencer {
    pub(crate) fn seal_after_work<'a>(&'a self,sealed:&'a TargetWorkSeal,
        deadline:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<TargetSqlQuiescenceProof,DispatchError>>;
}
impl ResourceSlot {
    pub(crate) fn prove_unloaded_abandon(&self,sealed:TargetWorkSeal,
        io:NoTargetIoEvidence,quiet:TargetSqlQuiescenceProof,
        inspection:ResourceInspection)
        -> Result<ResourceClosureProof,ResourceClosureRejected>;
}
```

NoTargetIoEvidence=NeverReservedLoad(同controller从未授load写权的sealed transcript)|ReservedCancelled(原cancel_resume receipt和实际Probe终态Zero)|TicketZero(原真实Ticket终态Zero及可靠StoredTransport)。各分支都还核没有该epoch已接受load/Prompt和有效保护；只由原私有operation/controller产生，不从Option::None/SQL缺行构造。ResourceClosureRejected返同原seal/evidence/quiet/inspection，不使第二enqueue复活。

Source/Target SQL quiescence仅跟踪必要资源/唯一派发/相关协议事实；附加audit、显示/raw投影和慢订阅不进入该缺口集合，不因其失败/等待阻实际安全关闭。已有dispatch slot的解除仍用原真实job/active与VerifiedConvergence的既有close链，不把资源released当Run成功或第二次发送资格。


## revision6 控制operation加入各stage精确集合

控制相关stage仍按原14格判定scope是否实际存在；无scope/未初始化时不为cleanup追加prepare/init或发不合法native cancel。已经产生的控制operation包括native cancel、PermissionCancelled、Unsupported和原target close，连同原Ticket、ReplyContinuation、旧cancel barrier、callerDrop cleanup及其metadata worker一起由原slot收敛。全量work/sql seal不得漏这些项；真实scope/资源替换后只关闭旧代际，原claim保留。

精确控制端口、原品牌/Reservation/Frame错误返还、必要SQL故障时安全执行且保gap、每stage失败表见[control-ports.md](control-ports.md)。Written只证明本次控制帧已写，不能充本附件Owned/IO/Job证据；SourceLoss仍非收敛，Pre地址仍非授权。当前target安全close的授权从同ClosingAuthority/当前范围得到，不靠SQL数据铸grant；实际协议能力/真实Closed与最终收敛门槛不变。

## 控制收据窄补充：不复用Load的必有RPC结构

本包[control-receipt-supplement.md](control-receipt-supplement.md)给完整形状。SourceSlot收尾调用的精确stage签名为：

```rust
impl<P: NativeControlPort> OwnedControlOperation<P> {
    pub(crate) fn cancel_unenqueued(&mut self, protocol: &P)
        -> Result<(), DispatchError>;
    pub(crate) fn cancelled_receipt(&self)
        -> Option<&CancelledControlReservation<P::Scope, P::Probe>>;
}
// 原NativeControlPort专用：
fn cancel_reserved_control(&self, reservation: Self::ControlReservation)
    -> CancelledControlReservation<Self::Scope, Self::Probe>;
```

cancel_unenqueued从该原operation移出原未enqueue控制Reservation，调用原P取消，同步将原identity与同cell Probe的私有receipt存入同操作CancelledUnenqueued阶段；没有接收外部receipt的setter。Enqueued stage拒此方法，保原Ticket走真实cancel/wait。Source/Target seal和SQLquiescence收集这个原控制stage/receipt，identity.rpc()按NativeCancel为None、两类reply为Some原typedID；同Probe实际FinishedZero才作Zero事实，收据本身不是关闭或再写grant。

本附件§5的cancel_resume签名与CancelledLifecycleReservation{scope,typedRPC,probe,kind}完全不变，仍只属于checked LoadReservation且RPC真实必有。UnloadedReservationAbandoned的ReservedCancelled仍只接原Load receipt，不接受本控制receipt。Source/Target最终release不因这个类型拆分放宽证据、SQL可靠性或真实任务/Job清理。
