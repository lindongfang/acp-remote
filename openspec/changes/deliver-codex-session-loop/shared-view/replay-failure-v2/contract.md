# Replay未完成失败：FIFO退役及实际可读分段ABI（设计，未实现）

本页仅补A2/A3/A5/A9与B7段载荷，依据窄解释5f8bd7bc。原八合同、same-read补充及83 oracle不改。不是失败candidate覆盖旧current、长期隐藏同SID live、平台档案或第六种重读政策。新增端口/载荷须独立gate后实现；当前代码没有这些类型/方法。

## 1. 原生窗口出处：新增私有只读端口

实际ACP ReceiptCut.windows已有(session,原cell.key,UpdateOrigin)，reader::route在同State锁核该key仍对应accepted pending，之后emit真实Received。现Received只存origin没有原RPC，不能用最近read、worker当前candidate、本地次序或同SID猜原窗口。新增：

~~~rust
pub(crate) enum ViewReadWindowKind { Resume, Replay }
pub(crate) struct ReceivedReadWindow { /* actual scope, SessionId, RpcId, kind, original cell key */ }
impl Received {
    pub(crate) fn view_read_window(&self)->Option<&ReceivedReadWindow>;
}
impl ReceivedReadWindow {
    pub(crate) fn scope(&self)->ConnectionScope;
    pub(crate) fn session(&self)->&SessionId;
    pub(crate) fn rpc(&self)->&RpcId;
    pub(crate) fn kind(&self)->ViewReadWindowKind;
}
~~~

只由原reader::route已验证的cut.window/pending原cell在同短段mint，随该次Received保存；Live为None，Resume不冒Replay。其它emit默认None。不新增reader、不二次读stdin、不改read窗口/期限/accepted/ack判定，不从JSON/DTO/配置名/bool或当前连接状态事后重建。字段私有，无公开构造/Default/Deserialize/setter；只读Clone保原物，不生Loaded/SourceSlot/Actor/写资格。cell key不发网络，不叫native顺序或轮次。Raw仍原RawMessage O(1) clone，窗口小事实进入RetainedAcpRaw，不clone整个Incoming。

ViewProjector从该原物投影同source已核的typed RPC窗口给worker，不从raw字符串恢复。泛型NativeViewProtocol新增等价私有view_read_window(&Lease)端口，ACP只借同Received；无原物的实现明确Unsupported/Unassociated，不能自报可信flag。生产接线须在最终Native候选串行重基准，不把该端口当已经实现。

## 2. 同原操作的失败marker与赢家

新增ReadFailureMarker<S>含：原source-slot或Module binding identity、actual S、target、原typed RPC、ReadEpoch、binding route代际、captured_cut、递增route_version、ObservedReadFailure静态原因及原物引用。ObservedReadFailure只接实际匹配RemoteError/response deadline、原Zero/Unknown write事实或原protocol/source terminal，不能由caller传成功bool。

只能由原read稳定槽及实际P observe/ACP tracker生产。非Received失败没有新Received.sequence/receipt_order；RemoteError保原scope/RPC/Received，Zero/Unknown保原WriteProbe/WriteReport，terminal保原生命周期对象。marker走同capture producer FIFO；追加失败由预分配fault槽记录，不伪Complete、不input/cancel、不冒原生保存失败。

worker按实际slot/scope/target/原RPC/ReadEpoch/route代际匹配。迟旧marker只更新原段的transport/failure，不能结束新candidate。真实Replayed匹配且前raw边界无gap完整后Complete先赢；后flush Unknown/source失联不退回失败。prefix未处理则按FIFO先处理raw/完成marker，不能把已有响应冒无gap。响应真实但prefix gap为ReplayIncomplete(CaptureGap)，保原响应事实。

失败结束candidate的view占用，保已收raw和出处。原Ticket/Reservation/operation仍留原supervisor到真实安全终态/必要收敛；view退役不授第二RPC资格。A9仅原native owner允许后才新reserve，每个新op一次新provider，join/显示段零provider、不续旧预算。

## 3. 旧H保持，另段实际可读

target展示bundle含principal segment和FIFO追加的unassociated segments。每段独立IndexedJournal/identity域；新段不能按旧tool/message ID或AnchorId更新旧项。segmentId取该段实际ViewEpoch的EntityId，不加段数/正文/引用总上限；它是平台分段数据，不是native身份/轮次/授权。private provenance保actualscope、真实target binding及原RPC/read窗口。

有旧current：失败不替换/清旧principal。原不可变版本/H仍可读，freshness Stale，独立failure可见；aggregate保守ReplayIncomplete/UnassociatedSegments，原完成段的Complete仍在段metadata及原NativeReadRequest。failed candidate冻结为failed-read段追加bundle。真实binding仍有效的后续标准同SID Live进入另一个live-observation段，CurrentOnly/CurrentObservation；不能与旧或失败段跨load合并ID。其字节立即经现授权读取路径展示，不能长期只藏source-only。每次新read边界结束原live段，连续性仅由真实窗口/binding决定。

无旧current：不造旧cache。实际部分内容或空内容及真实失败/限制成为可读bundle，ReplayIncomplete；不为统一退candidate丢唯一raw。后续Live另段。无可靠归属通知/未管理target/真正失效binding仍source-only，不扩大准入。

旧snapshot固定H及段集合；失败只追加>H段事件，不混旧正文/不切generation。tail依FIFO发failed/live边界、raw及投影；纯显示不发RPC。新成功candidate发布时才按原规则换展示ViewEpoch并seal旧流，旧cursor/Prepared Stale。只有原成功read intent且current正其ReadEpoch才可same-read续接；失败read不得借分段续接。

## 4. A2/B7实际读取入口与载荷

沿既有open/next_snapshot/next_delta/wait_change和ModuleViewTransfer.next_record/credit/detach，不加绕过五个外部ReadReason的open_segment。每次外部open仍按原规则尝试标准读；同bundle展示旧Stale/当前段，不隐式重发RPC。逐chunk current auth、三类seal、真实flush+credit、16KiB块和72字节header不变。

Status在每段首个raw/projection前传平台schema：

~~~json
{"type":"view_segment","phase":"start","segmentId":"<platform EntityId>",
 "segmentViewEpoch":"<platform EntityId>","readEpoch":"<platform EntityId or null>",
 "origin":"retained_current|failed_replay|live_observation",
 "readMode":"CurrentOnly|Replaying|ReplayComplete|ReplayIncomplete",
 "freshness":"CurrentObservation|Revalidating|Stale","join":"UnassociatedSegments",
 "sourceBindingId":"<platform EntityId>","sourceId":"<platform configuration EntityId>",
 "beginSequence":"<decimal u64 or null>","throughSequence":"<decimal u64 or null>",
 "failure":"<static ViewError or null>","projection":"<ProjectionState>",
 "completionResponseSequence":"<decimal u64 or null>","open":true}
~~~

phase=start/update/end；end仅平台段结束，不冒ReplayComplete。snapshot段end的open=true只表示到该H，非native窗口结束；tail continuation/update明示同段ID。完成字段仅该段真实标准Replay proof+完整prefix；Live/Resume/failed不得继承旁段Complete。无native SID/RPC/tool/message ID、正文、路径或凭据。所有未知值/大数只经OriginalRaw与原片段引用，不parse/reencode。

Status recordId在自身namespace checked递增。same-read新流仍先内部metadata→真Credit0→packet1 stream_rebound Status→真flush/credit，再segment Status/body。Status只提交自身位置，不推进raw byte cursor/授额外permit；旧credit不转新流。

RecordAddress新增只读segment_id()->EntityId、segment_view_epoch()->ViewEpoch；原view_epoch仍该展示bundle cursor代际。raw recordId仍source唯一RawRecordId，每个ItemProjection/evidence JSON含segmentId。展示anchor ID是bundle内实际append-only映射：首位置保持、新段另分配平台数据ID，不跨load更新；新成功bundle重建映射。AnchorMetadata新增同两段getter；原anchors/anchor_id/anchor(id)实际枚举，无反向cursor构造或grant。

## 5. 严格回填及下一read

已归属raw的segmentId/原RPC read窗口或Live provenance不可清回unassigned。新read只吸收同真实Replay窗口原RPC/scope/SID原物，包括reserve→bind间queued且窗口匹配的raw；失败段/旧read/Live永不混入新Complete。同SID或seq≤cut且未assignment不够。

首次真实target binding时，更早可靠同SID unassigned Live/Resume只回填独立CurrentOnly observation段；无SID仍source-only。有新Replay窗口时只有原RPC窗口匹配部分入Replay段。新成功current只声明自身标准Replay范围，旧failed/live不拼入此次完整历史；共享原文allocation不改出处。

bind/failure/live路由都在同producer FIFO；旧native owner准许后，worker按序结束旧candidate/建新candidate，不让ack/control/必要SQL等大列表投影或析构。增长在worker/reader owner、短锁外try_reserve；随机/分配失败静态报告并保稳定槽，无quota/TTL。大引用由原view supervisor真实回收/join。source/target真失效仍seal，segment metadata绝不恢复准入。

## 6. 验证边界

cases.md RF01–RF12均定义/未执行，不增删原83 oracle。Module不代NativeIdle/ND/WSS；WIP5、28开发绿或child单测不证明此未实现ABI。新分支独立定义/gate后才编码，其余授权工作继续。
