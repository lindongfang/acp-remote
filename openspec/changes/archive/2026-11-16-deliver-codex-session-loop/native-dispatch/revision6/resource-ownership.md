# revision6：真实资源 mint、失联与从未启动的关闭证明

2026-11-01，工程候选。本文与closure-contract.md完整替代revision4资源关闭合同；实际生命周期接口以lifecycle-ports.md为准，所有类型bounds/关联句柄/方法都由同阶段实现。没有未来Node传bool/SQL loaded行的生产入口。原生身份、旧product_resources保护、Shared Source/Windows Job义务不变，不实现自动回收/默认容量/历史导航。

## 1. 受理→实际Loaded的唯一链

同actual R3 D产生ContentOrigin，E稳定后由同owner短锁核item/revision、Submit/Edit已settled且仍当前，mint AcceptedTargetProof。普通请求等待方不拥有source任务，ensure_resource首await前注册稳定SourceSlot到Storage组合supervisor；当前actor撤销不取消已受理责任，新操作/读取另验权。

调用链逐步为：G.resolve→SQL reserve_source(PreSourceProbe)→SQL reserve_resource(PreResourceProbe)→唯一LaunchPermit→L.launch/adopt原Owned→record_source_spawned→take_streams一次→P.prepare_source/adopt原IoOwner→SourceScopeState一次绑定→record_source_bound可靠→P.start_source原token→实际initialize原Reservation/Frame/Ticket+真实Initialized→record_source_initialized可靠→按实际wire P.bind_setup→原resume reservation/frame→begin_load可靠→enqueue原load→实际Resumed/同Ticket Written分轴→record_loaded/record_load_transport可靠→原资源/前台/旧责任/gaps符合→私有VerifiedLoadedBinding→P.attach_prompt_binding→原Prompt链。完整每一端口签名和stage字段见lifecycle-ports。

G真实读product_targets/product_sources/product_harness_sources/harnesses/workspaces，使用显式选用程序/配置及可信config_ref提供器；不按最近入口/显示名/私有文件猜。完整有效setup只内存，Unknown不造空MCP/根；默认继承node环境及显式覆盖保真。配置数据不是Loaded证明。

reserve_resource在同真实BEGIN IMMEDIATE核新native resource和旧product_resources，同target任何未released/unknown旧责任均阻新owner；不为腾位改旧行released。多个目标可共享同ConfiguredSourceKey/相同完整配置的source，目标分别LoadEpoch/唯一资源归属；不同来源同ID不误合。同key并发ensure由source registry同短锁选唯一SourceSlot，共享其source启动future和Initialized结果，不能第二launch。每目标自己的resource reserve成功后才允许该目标load；初始创建source者的两项reserve可靠后才允许首次launch。

v2实际load后须当前可靠Idle，running/requires_action/unknown均阻新的Prompt；初始SessionActivity Unknown。v1仅本节点新启动且完整拥有source、真resume完成、没有旧prompt待决、旧资源已核收敛时按v1 turn语义建立可输入状态；新握手不能证明旧工作停止。有效Permission和后台Unknown独立保护原资源。

## 2. Pre与Bound的SQL端口（无ACP类型）

PreSourceProbe/PreResourceProbe无actualscope；真实prepare返回Scope后只有SourceSlot::bind_prepared能一次绑定，生成BoundSourceProbe<S>。给SQL的SourceBoundRecordProof是该转换产生的协议无关私有投影：原Pre全字段、scope_binding_id、节点bound_at、metadata fingerprint及same sourceSlot identity。它不存Scope Arc，不可从该SQL数据反造BoundSourceProbe。BoundResourceRecordProof相同方式从真实BoundResourceProbe<S>派生，包含原PreResource、scope_binding_id、wire/config及同slot身份。repository无需泛型S、无需importACP。

```rust
pub trait NativeResourceRepository: Send + Sync + 'static {
    fn reserve_source<'a>(&'a self,p:&'a PreSourceProbe,c:&'a OperationContext,d:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<StoredSource,DispatchError>>;
    fn reserve_resource<'a>(&'a self,p:&'a PreResourceProbe,c:&'a OperationContext,d:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<StoredResource,DispatchError>>;
    fn record_source_spawned<'a>(&'a self,p:&'a PreSourceProbe,f:&'a SpawnedSourceProof,c:&'a OperationContext,d:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<StoredSource,DispatchError>>;
    fn record_source_bound<'a>(&'a self,p:&'a SourceBoundRecordProof,c:&'a OperationContext,d:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<StoredSource,DispatchError>>;
    fn record_source_initialized<'a>(&'a self,p:&'a SourceBoundRecordProof,f:&'a InitializedProof,c:&'a OperationContext,d:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<StoredSource,DispatchError>>;
    fn begin_load<'a>(&'a self,p:&'a BoundResourceRecordProof,f:&'a ReservedLoadProof,c:&'a OperationContext,d:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<StoredResource,DispatchError>>;
    fn record_load_transport<'a>(&'a self,p:&'a BoundResourceRecordProof,f:&'a LoadTransportProof,c:&'a OperationContext,d:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<StoredResource,DispatchError>>;
    fn record_loaded<'a>(&'a self,p:&'a BoundResourceRecordProof,f:&'a LoadedObservationProof,c:&'a OperationContext,d:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<StoredResource,DispatchError>>;
    fn record_resource_state<'a>(&'a self,p:&'a PreResourceProbe,f:&'a ResourceStateProof,c:&'a OperationContext,d:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<StoredResource,DispatchError>>;
    fn record_source_state<'a>(&'a self,p:&'a PreSourceProbe,f:&'a SourceStateProof,c:&'a OperationContext,d:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<StoredSource,DispatchError>>;
    fn record_source_closed<'a>(&'a self,p:&'a PreSourceProbe,f:&'a SourceClosureProof,c:&'a OperationContext,d:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<StoredSourceClosure,DispatchError>>;
    fn record_source_loss<'a>(&'a self,p:&'a SourceLossProbe,c:&'a OperationContext,d:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<StoredSourceLoss,DispatchError>>;
    fn close_unlaunched<'a>(&'a self,p:&'a NeverLaunchedProof,c:&'a OperationContext,d:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<StoredNeverLaunchedClosure,DispatchError>>;
    fn inspect_resource<'a>(&'a self,p:&'a PreResourceProbe,c:&'a OperationContext,d:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<ResourceInspection,DispatchError>>;
    fn inspect_source<'a>(&'a self,p:&'a PreSourceProbe,c:&'a OperationContext,d:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<SourceInspection,DispatchError>>;
    fn interrupt_resources<'a>(&'a self,p:&'a StartupProof,c:&'a OperationContext,d:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<ResourceRecoverySummary,DispatchError>>;
}
```

全部通过同SqliteRequestRegistry受信派生pool/ConnectionOwner；事务lease/detach/真实close顺序不改。StoredSource/StoredResource逐列完整返回DDL，不补默认。SourceInspection含source全部字段、该instance所有资源完整列表、loss记录及旧product_resources冲突；ResourceInspection含相应source/资源/旧冲突/必要事实；仅数据，不mint任何live proof。所有COMMIT未知只inspect全字段等价，原typed对象/许可仍存在才继续原动作，否则仅安全收敛。

事务表：reserve写reserved纯元数据；record_source_spawned核同entered launch实际owner已adopt，reserved→spawned；record_source_bound核同spawned与原配置，只能scope_binding_id NULL→原id或同id幂等，异idConflict；record_source_initialized核原RPC/真实receipt+Written、Bound记录可靠，spawned→initialized，记录actual wire；begin_load核同Initialized/Bound，reserved→loading，原RPC/原frame长度一次写；record_loaded仅原真实Resumed及原resource，loading→loaded，不暗含transportWritten；record_load_transport armed→单一真实终态，异终态Conflict；record_resource_state(Released)与record_source_closed以原Pre为地址，closure-contract.md中的stage-specific私有真实证明为依据；Scope/setup不是早期关闭的必填前提。NeverLaunched只用独立close_unlaunched端口。

## 3. 无Received的SourceLoss（ND-RESOURCE-002）

来源是P::next返回的同真实source terminal对象（receiver Err/None），或P.fail_observation消费原lease实际stop同Core后返回ObservationFault terminal，不是客户端/数据库/timeout猜测。主pump先同sourceSlot封全部新危险写、把涉及目标foreground/资源标Unknown、保原input/transport/审批作用范围，再由`SourceLossProbe::from_terminal(bound_source,&SourceTerminal,affected_slots)`私有mint。该方法核actualscope等于BoundSourceProbe scope、同Arc owner/instance/runtime/config；没有Scope时不能调用它，launch失败另走EnteredFailure责任。

SourceLossProbe完整字段：loss_id=原terminal_id、source_id/runtime/instance/connection_epoch/scope_binding_id、reason(EOF/IO/Protocol/Stopped/ObservationFault)、节点observed_at、同source slot identity、source RequirementId、metadata fingerprint、冻结有序AffectedTarget列表。每项只取实际owned target/resource slot：target/native key、owner_id/load_epoch、owner_order、RequirementId、metadata fingerprint；可为0目标（如所有目标已释放但源正在关）。**不含observation sequence、receipt_order或Job/IO完成字段**。

record_source_loss同事务写source_losses header+loss_targets完整小批，并将该source未closed状态及涉及未released资源标unknown（已closed/released不复活），核source Bound/每target实际resource/source/runtime/epoch；相同loss_id必须全部字段同，异批Conflict；COMMIT确认丢失后重试/inspect仍原loss_id/采样/列表，不造新sequence或新终止。必要记录失败进原RequiredGap集合，保存成功只关该记录缺口，**绝不移除SourceState::Lost/Unknown或旧资源责任**。

SourceLossProbe与ConvergenceProbe为两个不同类型，仓储方法也分开。ConvergenceProbe仅JoinedOwnedSource/NativeTargetClosedVerified/OldRuntimeStopped，不含Loss；DDL从node_native_convergences移除connection_lost，单独表保存loss。ReleaseProof::VerifiedConvergence、ResourceStateProof::Released、SourceClosureProof不接SourceLossProbe/StoredSourceLoss；负编译和真实仓储负例应拒。后续即使loss落库可靠，也只有新的实际native/Job/IO收敛证明和必要记录可靠后才能释放slot/资源或重新Loaded。失联本身不证明前后台已停、不改变Run为成功/Stopped。

## 4. 从未启动的独立关闭（ND-RESOURCE-003）

SourceSlot创建时同时创建LaunchController（同Arc identity、runtime/instance/generation）、唯一LaunchPermit；phase只允许`Reserved -> Entered -> Owned|EnteredUnconfirmed -> Closing -> Converged`或`Reserved -> RevokedNeverEntered -> ClosedNeverLaunched`。没有Entered→Reserved回退，没有查询行→Controller构造。

`SourceSlot::revoke_unentered()`同controller短锁不可逆Reserved→RevokedNeverEntered并使所有未来permit.enter失败。可以在launch future已登记但尚未enter时赢得该竞争：它以后即使poll也只能得到Revoked，**不能调用实际provider**；若enter先赢则返回AlreadyEntered，无NeverLaunched。普通caller Drop和deadline不能代替此转换。

在同source任务消费完此前所有source/resource reserve结果（含提交确认未知的inspect）、确认无任何可能迟到的新reservation SQL worker并与source registry封新相关操作后，`seal_unlaunched(self,inspection)`消费RevokedNeverEntered控制权，mint不可Clone NeverLaunchedProof。字段为同SourceSlot identity、runtime/instance/source/config/preprobe全量、controller generation+revoke_nonce、原资源reserve尝试集合、完整SourceInspection中的已提交reserved资源列表、SQL操作quiescence token、closure_id、原节点时间/metadata fingerprint。没有IOjoined、Jobactive、进程exit或nativeclose字段。

构造断言由实际状态验证：从未成功enter；从未拥有process/fixture owner；未调用take_streams；未prepare actualscope；未有IoOwner/IO任务、Init/Load Reservation/enqueue；所有reserve futures/worker终态已由同原任务消费，禁止后续SQL再插本instance。空Option只是辅助断言，**核心证据是唯一permit不可逆撤销+无未来entry**，不能凭“没拿到handle”推零启动。

`close_unlaunched`的单一BEGIN IMMEDIATE：

1. 核同SourceSlot proof投影的原source全字段、lifecycle='reserved'、scope_binding_id及初始化字段均NULL；核该instance所有资源正好等于proof提交集合，全部reserved且无loadRPC/transport/receipt；核无dispatch/实际attempt/loss/bound记录，不借旧product_resources行。
2. 把这些本instance新reserved资源原地标released，release_basis='never_launched'、release_stage='never_entered'、release证据=同closure fingerprint；source原地closed、close_basis='never_launched'、close_stage='never_entered'、owner/io evidence='not_applicable'（无job count）、同fingerprint。**不删行、不改旧资源、不重开slot**。source成功但resource明确失败时集合可以空；resource提交未知必须先同原task inspect确认集合，不可漏一个迟到提交。
3. 同proof重试返回完整StoredNeverLaunchedClosure；同instance已closed相同原plan/basis/fingerprint幂等，异proof/异字段Conflict。COMMIT未知仍controller已撤销，零未来launch；inspect确认闭合后才移除registry活槽，保原tombstone拒旧instance重启。未确认保持责任，不生成新permit。

两项reserve可靠后未launch停止、source已提交而resource reserve失败/不确认后核清、launch任务注册但未enter被revoke，均从这一合法分支闭合。它不出具OwnedConverged或Windows树证据。

一旦enter已发生，Err/timeout/callerDrop不能改NeverLaunched。实际Owned但未prepare、BeforeAdopt失败、Io已adopt未start、source已Bound/resource仍Pre及Loaded阶段，全部按closure-contract.md的矩阵/typed Evidence及Pre寻址入口闭合；没有实际报告时只保存EnteredUnknown/资源Unknown，不伪造Scope/IOjoin/Job0。共享source单目标尚未load的reservation可按该附件UnloadedReservationAbandoned关闭而不杀其它目标。

## 5. 失败、安装并发与关闭责任统一表

| 真实边界 | 所有者/允许的证明 |
| --- | --- |
| Storage claim/组合supervisor之前失败 | 无新任务IO，释放仅尚未注册的claim；保旧R3/用户数据 |
| source task/两reserve Pending或callerDrop | Storage组合supervisor持任务/Pre probes；不launch；未知提交inspect原事实，不重新受理 |
| reserve已可靠但permit未enter | 同Controller revoke+SQL quiescence后独立NeverLaunched，不能伪Job0 |
| L.launch enter之后任何错误 | 同source任务/adopter负责真实cleanup或EnteredUnconfirmed；不是未启动 |
| take_streams/record spawned失败 | 原Owned先adopt，真实Job/fixture owner仍在slot；清理原资源，不替代launch |
| P.prepare_source adopt之前拒绝/分配失败 | 完整Core/槽预分配在先，尚无spawn IO任务；原流由P错误路径Drop/返其owner清理，process仍在sourceSlot；不声称曾有IOjoin |
| adopt成功、安装中失败或panic | 同SourceSlot已经持唯一P::IoOwner；InstallationGuard封新写并保handle登记，shutdown等installation_complete后真实join。同步Result不隐含asyncjoin |
| start token Drop或bound SQL失败 | 真实actualscope与IoOwner仍原slot，tokenDrop停止门控Core；由同owner清理，不能NeverLaunched |
| init/load可能写、早response、SQL未知 | 原Operation持原Reservation/Frame或Ticket；不重造RPC；pump持续，unsafe grant封闭 |
| reader terminal | mintSourceLoss→保存Unknown缺口；不出具任何released/Converged |
| Storage.close被取消或并发close | 组合Arc槽、Job/IO/SQLclose owner一直可达；不take JoinHandle到可取消局部Vec |

源source/current绑定需按固定owner→resource→ProtocolState锁序；安装状态另同source slot安装锁，不持ProtocolState调用adopter，shutdown通过installing/notify等待，不能拿空task列表早报joined。所有普通等待方Drop都不取消节点任务、不解除runtime claim；SourceSlot任务失败由组合supervisor的Arc槽保收尾。

关闭先封新source/jobs/Prompt、继续小事实/安全控制，再按当前OwnedProcess真实约束：对独占或整个node拥有可关闭的source在同总deadline先协议grace，必要时原OwnedProcess.shutdown TerminateJobObject解卡pipe，再join原ACP/stdio/stderr；根exit≠树active0，不重置deadline。共享source单target只可用已验证隔离close；不能隔离明确拒并保责任，不能杀另target。

全部source/native任务和必要SQLworker收敛后才Storage→共享ConnectionOwner.seal/drain→真实SqliteConnection.close→pool.close。真实关闭与记录失败分列；未确认记录/Unknown资源不返slot。Drop只请求stop不冒join。NeverLaunched、真实stage OwnedConverged、UnloadedReservationAbandoned与SourceLoss类型及持久basis严格分开。完整关闭proof/字段/事务见closure-contract.md，本文不得以旧Bound方法参数收窄那些Pre入口。

### 未启动证明的精确能力转换

```rust
impl SourceLaunchController {
    pub(crate) fn revoke_unentered(&self)
        -> Result<RevokedLaunchAuthority,LaunchRevocationError>;
}
impl SourceSqlSequencer {
    pub(crate) fn seal_reservations<'a>(&'a self,
        revoked:&'a RevokedLaunchAuthority,deadline:&'a RegistryDeadline)
        -> BoxFuture<'a,Result<SqlQuiescenceProof,DispatchError>>;
}
impl<P,L> SourceSlot<P,L>
where P:NativeControlPort,L:SourceLaunchPort,
      L::Owned:OwnedNativeSource<Streams=P::Streams> {
    pub(crate) fn prove_never_launched(&self,revoked:RevokedLaunchAuthority,
        quiet:SqlQuiescenceProof,inspection:SourceInspection)
        -> Result<NeverLaunchedProof,NeverLaunchRejected>;
}
```

Controller只有Reserved→RevokedNeverEntered的成功者取得唯一不可Clone RevokedLaunchAuthority；AlreadyEntered/AlreadyRevoked无新cap。NeverLaunchRejected持原revoked/quiet/inspection和typed错误，可into_original返还原对象，不丢清理责任。SourceSlot本体由Arc监督，不消费Arc；消费的是独占未来启动权。SourceSqlSequencer属于同SourceSlot，所有该instance source/resource reserve任务在首await前取其私有operation token并登记稳定任务槽；seal先封未来reserve，再等待原操作真实终态/必要inspect，不能以cancel waiter/空局部Vec推quiet。quiescence proof含same slot/generation、原operation集合及完整结果fingerprint，late reserve无法在seal后登记。close_unlaunched是sealed状态唯一允许的后续闭合SQL操作，native init/load/dispatch路径均已被revoke封死。

物理OwnedNativeSource保存在同SourceSlot稳定Arc的异步mutex槽中；take_streams只在短同步段取一次，shutdown借用原owner跨await时只占该cleanup槽，不持R3 owner/resource/ProtocolState业务短锁。close waiter取消只释放guard，原Owned仍在槽，后续关闭继续同对象；不take到caller局部变量丢失归属。唯一launch future的中间资源在已监督任务内，进入adopter后也遵守此规则。


## revision6 原生控制所有权

[control-ports.md](control-ports.md)完整定义NativeControlPort与NativeControlRepository。N的组合bound增加该仓储；ControlOperation/Ticket/request claim及元数据worker归同SourceSlot/组合supervisor。SourceWorkSeal/TargetWorkSeal必须覆盖旧cancel barrier、所有控制operation与真实终态/无未来poll；SourceSqlQuiescenceProof/TargetSqlQuiescence包括它们的精确持久facts集合、未确认提交及worker终态。不得释放后迟到补写重新造Loaded；物理cleanup可在SQL故障继续，持久Released仍等可靠精确proof。
