# 同次读取的私有展示续接：最小 ABI 补充 v1

设计补充，未实现、未执行、未放行。继承有效777a+489e八合同与5609编码门槛，仅给A2缺少的装配续接入口；原83 ID/步骤/预期、A9真实外部重读规则、72字节wire及所有既有限额原字节不变。

**问题与边界。** 首开须立即交已有H，同时受监督地发真实Replay；无可证跨load身份时新候选完成后发布新epoch并使旧cursor/Prepared陈旧。若装配把每次内部陈旧都再作open(Reopen)，慢大snapshot可能反复被下一次Replay替换。本补充只允许原已受理read intent继续展示其那一次已完成原生读取；旧cursor不换代、不复活、不混拼。外部FirstOpen/Refresh/Reopen/Reconnect/Reacquire仍走原校验和真实读取；这里不是新的缓存命中或重读策略。

~~~rust
impl<S,R,A,C> ViewReadService<S,R,A,C>
where S:Clone+Eq+Send+Sync+'static, R:RetainedRaw,
      A:CurrentViewAuthority<C>, C:Send+Sync+'static {
    pub(crate) async fn rebind_same_read(
        &self, client:&C, previous:&OpenedView,
    )->Result<ReadRebind,ViewError>;
}
pub(crate) struct ReadRebind { /* unique new OpenedView + immutable metadata */ }
impl ReadRebind {
    pub(crate) fn metadata(&self)->ReadRebindMetadata;
    pub(crate) fn into_opened(self)->OpenedView;
}
pub(crate) struct ReadRebindMetadata { /* Copy platform data; no native IDs/body */ }
impl ReadRebindMetadata {
    pub(crate) fn previous_stream_id(&self)->EntityId;
    pub(crate) fn new_stream_id(&self)->EntityId;
    pub(crate) fn view_epoch(&self)->ViewEpoch;
    pub(crate) fn read_epoch(&self)->ReadEpoch;
    pub(crate) fn snapshot_revision(&self)->ViewRevision;
    pub(crate) fn source_stamp(&self)->ViewSourceStamp;
}
~~~

**原物与核验顺序。** 原OpenedView稳定槽保小型AcceptedReadIntent引用：同一issuer/binding、client和socket lifetime（ModuleLocal为同local issuer/client nonce，绝不冒socket）、canonical target、actual scope/source绑定代际、原NativeReadRequest及其**第一次绑定后不可变的operation ID/ReadEpoch**。不得跟随查询句柄的后继refresh后偷偷改为新operation；不得从client DTO、stream ID字符串、元数据或bool重造该引用。旧大snapshot可由原监督清理移走，小型续接槽保到显式release/Detach或成功消费。

1. 在原registry短段核同service/intent/issuer及未显式释放；只接受因该请求新view发布而陈旧的旧stream。以唯一nonce占单次Preparing槽；并发续接给InvalidStage，不制造两个新stream。
2. 当前验权，释放actor gate后取原请求真实ReadCompletion及当前只读source/target绑定。必须同actual scope、source/resource或ModuleFixture绑定代际、target，且current view的ReadEpoch正是**该原operation**，其真实Replayed和前置raw边界已完成。不加Written/flush门槛；后来真正source关闭仍拒绝，不能抹掉此前完成事实。
3. 先订阅source/view及当前权限失效通知，再取新capture cut/route version并核条件；在actor/registry/ND锁外等worker barrier。等待或唤醒后重新当前验权，最终短段再核上述身份、scope/绑定、current epoch、nonce和未释放状态；取全新H并登记全新stream/cursor（AwaitInitialCredit，独立随机ID，packet从新流开始）。旧cursor和所有旧Prepared仍陈旧；旧已开始块的真实字节事实分列，零旧cursor commit/零新permit。
4. 原监督槽先保新read/task再公开ReadRebind；成功后原续接槽Consumed。资格、随机/分配、barrier失败或等待取消均不推进旧cursor、不重发RPC、不换旧intent；未公开新stream的Preparing只由同nonce回退，已登记但未交出的新stream由同owner seal/join。返回值/新OpenedView被Drop只关闭该新consumer，不能丢native原物。

**零原生动作。** 本方法不调用NativeReadControl.request、NativeHistoryPort.acquire_read_binding/reserve/enqueue、options provider、ConfigurationPort重新launch、任何输入/审批/cancel或native activity/idle-touch入口；只读当前已发布绑定和原operation事实。因此RPC数/provider调用数均增量0，原native idle预算不更新。原inflight/终态Ticket、完成事实及资源责任仍归原owner。不得以失败内部fallback open(Reopen)。

| 条件 | 必须返回的静态类别 |
| --- | --- |
| CurrentOnly/无原受理请求、未完成、Preparing或Consumed | InvalidStage |
| 原读取Incomplete/前置raw gap | 原静态失败（如CaptureGap），否则ReadFailure |
| 显式release/Detach/原stream释放 | StreamSealed；不得借缓存复活 |
| 源关闭/换actual scope或source/resource代际/节点关闭 | SourceReleased / SourceReplaced / NodeStopping |
| 撤销、跨client/issuer、跨socket、跨target | Revoked / InvalidClient / WrongSocket / InvalidTarget |
| current已是别的operation的ReadEpoch，或等待中再次换代 | StaleCursor；不能续到别人的更新epoch |
| 新ID、容器或传输交付失败 | 原RandomUnavailable/AllocationFailure/QueueRejected等；只收束本consumer |

**显式装配。** ModuleTransfer先报告旧Stale/新stream metadata，重组器清楚结束旧未完成record，再从新H/新流重组，不能拼两epoch。WSS以后必须由同真实Attachment发可见新stream metadata并走原bounded outbound/current writer，未通知则不能发新body；metadata交付失败seal新consumer，不能重读或清节点cache。旧credit/早credit只按旧流SealedIgnored；新流须真实initial credit。所有body每chunk仍当前验权并走B3最终围栏；不扩大控制text32KiB/depth8/outbound1MiB256/120命令限额，不把通知或credit变native命令。本补充不生成Native/SocketFlush/Loaded证明，WSS/Web仍待实现与真实验收。

**定向补充断言（均defined/not-run，原83不改）。** RB01：真实ACP+认证、大慢旧snapshot，一次真实Replay完成后旧游标Stale，续接新H可收完；请求/provider/idle-touch增量0（SV026/029/030/033/038/039/048/049）。RB02：双client、同op订阅正例与跨client/两op/current别的epoch反例，原stream显式release拒绝（SV034/035/047/061/062）。RB03：原真实Replayed早于flush与前置barrier、后来source关闭分别核，不能以Written或普通Resumed完成（SV041–046）。RB04：旧Prepared/早credit与seal竞态零旧commit，新流initial credit/真实flush后才提交（SV063–074）。RB05：订阅缝/等待取消/撤销/换scope/owner shutdown/并发Preparing与随机分配失败，零RPC fallback且原句柄可监督收尾（SV027/031/032/050/056–058）。RB06：Module与WSS新stream metadata及部分record不跨epoch；Module不代WSS、Native/树关闭证据（SV010/024/029/035/065/070/074）。L4/L5及实际idle旁路计数未有可观测实现时如实未执行，不拿0 RPC文字断言冒执行。
