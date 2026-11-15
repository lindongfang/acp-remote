# revision2 真实socket、线性发送及credit

本文件替代v1的Prepared/Delivered及highest_sent语义。72字节wire格式仍完全沿v1 assembly§3，不把原raw解析为控制JSON；本版精确规定谁可接收/发送、何时可回credit和提交cursor。所有本节类型尚未实现，接口不是运行能力。

## B1. LiveSocketContext的唯一生产issuer

当前AuthenticatedActor只含node/client/key/auth_generation，没有socket字段，不能当stream socket身份。新增**私有且只由management_ws真实握手分支持有**的SocketAuthAttempt，创建时绑定当次真实upgrade的reader、唯一writer、原ConnectionLease、实际challenge connection_id和不可伪造Arc<SocketLifetime>。构造器不从DTO接受connection字符串，不公开actor+id→context方法。

```rust
struct SocketAuthAttempt { /* 原socket lifetime+原ConnectionLease/challenge */ }
struct AuthenticatedSocketReceipt { /* 唯一真实成功authenticate及lease身份 */ }
pub struct LiveSocketContext { /* Arc<SocketLifetime> + actual AuthenticatedActor */ }
impl LiveSocketContext {
    pub fn socket_id(&self)->EntityId;  // 只读平台metadata，不是构造入口
    pub fn client_id(&self)->&str;
    pub fn is_closed(&self)->bool;
}
impl SocketAuthAttempt {
    async fn authenticate(self,proof:&str,policy:&ApplicationPolicy)
        -> Result<AuthenticatedSocketReceipt,SocketAuthenticationRejected>;
}
impl AuthenticatedSocketReceipt {
    fn install(self,authority:&SocketViewAuthority)
        -> Result<(LiveSocketContext,ViewSocketAttachment),SocketInstallRejected>;
}
```

authenticate内部唯一调用实际ApplicationPolicy.authenticate(原challenge connection_id,proof)，再次still_valid，然后原ConnectionLease.authenticate(实际client Id128)成功，才mint该receipt；实际当前代码run_protocol这些语句就是唯一替换调用点。install把同Arc context登记到真实reader/writer的ViewSocketAttachment，不能传另一个socket/sink，失败返原receipt/lease任务归属供原socket关闭。receipt/context无public ctor/Deserialize/Default；LiveSocketContext可Clone仅共享**同socket**身份，不让另一socket安装。SocketLifetime中socket_id随机失败终止当次view安装，不复用默认ID；真实socket断开原AttachmentDrop seal lifetime，并立即seal该socket全部stream，不授予脱离socket的新local reader。

SocketViewAuthority由ManagementState同ApplicationPolicy、TrustStore、可信NativeSource读取资格解析器构建。client_binding核LiveSocketContext的原issuer Arc、同attachment活跃与实际actor后返回opaque ReadClientBinding（含同socket/client/auth_generation/issuer）。with_current取得**同TrustStore.client_gate**，真实require_current_actor，核socket未seal及target的已配置授权来源，再执行固定chunk闭包。身份/target解析错误fail-closed；runtime审批placeholder不参与，当前个人节点管理资格仍足够且不新建团队ACL。跨socket相同actor、公钥/client相同也不能复用stream/cursor。

## B2. 稳定槽与线性对象的完整方法

stream槽由OpenedView所属view supervisor从open起稳定持有；它保committed snapshot/tail cursor、唯一pending chunk（最多16KiB）、permit/packet/read generation和实际task槽。token可在调用者间move，不能复制；Drop不丢稳定槽/潜伏任务，只请求对应阶段abort/seal。网络客户端没有任何token构造入口。

```rust
pub enum ReadStreamState {
    AwaitInitialCredit, ReadyToPrepare, Preparing, Prepared, Queued,
    WaitingSinkReady, Started, Flushing, SentAwaitCredit,
    Closed, ClosingAfterStarted,
}
pub enum StreamRecordKind { OriginalRaw, ItemProjection, Status, SnapshotEnd }
pub struct PreparedViewChunk { /* 同stream/slot/phase nonce，无公开bytes */ }
pub struct QueuedViewChunk { /* move原Prepared，原outbound permit */ }
pub struct StartedViewChunk { /* 只由真实start_send成功生成 */ }
pub struct DeliveredViewChunk { /* 只由真实poll_flush成功生成；无public ctor */ }
pub struct ViewQueueRejected { /* error+原PreparedViewChunk */ }
impl ViewQueueRejected {
    pub fn error(&self)->ViewError;
    pub fn into_original(self)->PreparedViewChunk;
}
impl PreparedViewChunk {
    pub fn metadata(&self)->ChunkMetadata;
    pub fn address(&self)->RecordAddress;
    pub fn stream_id(&self)->EntityId;
    pub fn packet_sequence(&self)->u64;
    pub fn abort(self)->PreparedAbortReceipt;
}
impl DeliveredViewChunk {
    pub fn stream_id(&self)->EntityId;
    pub fn packet_sequence(&self)->u64;
    pub fn source_was_sealed(&self)->bool;
    pub fn committed(&self)->bool;
}
impl ViewSocketAttachment {
    pub(crate) fn enqueue(&self,prepared:PreparedViewChunk)
        -> Result<(),ViewQueueRejected>;
    pub(crate) fn receive_credit(&self,wire:&[u8])->Result<CreditReceipt,ViewError>;
    pub(crate) fn receive_detach(&self,wire:&[u8])->Result<ReadCloseReceipt,ViewError>;
    pub(crate) fn close_stream(&self,stream:EntityId)->ReadCloseReceipt;
}
impl ViewSocketWriter {
    pub(crate) async fn drive_next(&mut self)->SocketWriteOutcome;
}
```

enqueue精确move原Prepared到原全连接outbound的Queued分支，先核同attachment/stream/phase nonce。满/错socket/已seal时返回同原Prepared，cursor仍未提交；调用者可abort（仅原Prepared未入队时），或原有outbound permit可用后重试**同Prepared**，不能另prepare第二块。enqueue成功后原token已move，只有原WSS writer可取Queued；不向caller返回可取消其实际发送的新句柄。等待drive_next被取消不把Queued/Started从稳定writer槽take到caller局部遗弃；原writer监督任务继续或在真正socket关闭时统一seal/记录实际SendFailed，不自动重造RPC。

PreparedAbortReceipt getters=stream/packet、never_started=true、cursor_unchanged=true；这些事实只由阶段核Prepared而得，不接受bool参数。abort后closed当前stream（避免同packet可重复授权歧义），新open可重新读仍保raw；仅准备分配失败在还无token时可回ReadyToPrepare原permit，由supervisor重试或报告本读失败。

SocketWriteOutcome闭集：Sent{metadata,commit:CommitDisposition}、RejectedBeforeStart{metadata,error}、FailedOrUnknown{metadata,error}、SocketClosed。它是只读结果，不是Delivered构造入口。CommitDisposition=AwaitingPeerCredit|Committed|SealedNoCommit。所有tokenmetadata/getter只含平台stream/view/record/revision/长度；Debug不输出native ID/raw/配置。原data bytes仅在真实writer内部转换Message::Binary，不能将内部RetainedRaw交外部管理调用方。

## B3. 实际begin-send与source/view/stream seal

SendRegistry是专职view发送metadata模块：一个短同步mutex管理source/view/socket/stream当前代际及每streamphase；它不持正文log/Projection/SQL任务或网络句柄。seal_source、seal_view、seal_stream与**最后start_send**在同registry锁线性化，seal不取actor gate、不await网络/worker、不释放大raw。

```rust
pub(crate) trait ViewBinarySink: private::Sealed {
    fn poll_ready(&mut self,cx:&mut Context<'_>)->Poll<Result<(),SocketWriteError>>;
    fn start_send(&mut self,message:Message)->Result<(),SocketWriteError>;
    fn poll_flush(&mut self,cx:&mut Context<'_>)->Poll<Result<(),SocketWriteError>>;
}
impl ViewSocketWriter {
    fn start_ready_chunk(&mut self,queued:QueuedViewChunk,
        authority:&CurrentSocketWriteGuard)
        -> Result<StartedViewChunk,BeginSendRejected>;
    fn finish_flush(&mut self,started:StartedViewChunk,
        fact:ActualSocketFlushFact)->SocketWriteOutcome;
}
```

ViewBinarySink私有sealed，生产唯一实现薄包原实际SplitSink<WebSocket,Message>；测试只能真实WSS或原真实sink的cfg(test)暂停观察，不能fake bool/一个Ok实现mintDelivered。ActualSocketFlushFact只有实际writer `poll_flush` 返回Ready(Ok)或实际错误/timeout分支内部mint，无public构造/Clone。CurrentSocketWriteGuard仅由writer同client_gate+真实current_actor验证产生，持原gate且核同原SocketLifetime；caller不能从A.with_current闭包外带出guard。

drive_next具体步骤：取Queued→取得当前actor gate+真实current核socket/target→**锁外**poll_ready等待（沿既有15秒总发送等待，不能因每次Pending重置）→准备原binary Message→start_ready_chunk。后者在同SendRegistry短锁核：source actualscope绑定未seal且与原ViewSourceStamp/Resource generation相同、ViewEpoch/ReadEpoch仍当前、same socket+stream未seal、same Prepared/Queued nonce和packet、仍唯一pending、guard同socket。合法则在**仍持该短锁时恰一次调用原sink.start_send(Message::Binary)**；其成功后在同锁写Started(n)，释放锁返回StartedViewChunk。没有先设Started再await ready的空隙，没有在registry内await。失败返回BeginSendRejected，含实际never_started或start_attempt_failed类别及原metadata，**不返回已被sink消费过的Prepared**。

之后实际poll_flush在registry锁外，持actor gate直到flush成功/Err/原15秒截止；source/view/stream seal可立刻取得短锁，不能被慢flush拖住原ND ack/关闭。sourceSeal先赢：旧Queued零start_send；start_send先赢：这一个固定块可能已进入系统/对端，后seal记录ClosingAfterStarted，不能撤回。即使start_send报Err，也不能声称网络字节一定未暴露，只记录FailedOrUnknown且零cursorcommit，不重试该socket帧制造重复。

native小事实路径只调用 `ViewSendSeals::seal_source(&VerifiedSourceViewSeal)` 或 `seal_view(&VerifiedViewEpochSeal)`；它们使用同短registry锁标tombstone+原binding generation，notify在锁外，绝不await actor guard、flush或stream cleanup。source/view/socket seal只改相应共享generation flag，**不在锁内遍历所有stream或析构正文**；各stream最后发送/prepare读同flag拒绝，后台清理/唤醒在锁外。view.close/Detach用seal_stream同样规则。registry从不反向取native owner/source/resource/ProtocolState锁；ND→registry单向短调用之后原业务锁释放，cleanup无反向依赖。此seal是view责任，不作为native cancel/allow/SQL的新围栏。

## B4. early credit、发送事实与cursor原子提交

```rust
pub enum CreditDisposition { InitialPermit, Duplicate, HeldEarly, CommittedAndPermitted, SealedIgnored }
pub struct CreditReceipt { /* 只读stream/ack/disposition */ }
impl CreditReceipt {
    pub fn stream_id(&self)->EntityId;
    pub fn acknowledged_packet(&self)->u64;
    pub fn disposition(&self)->CreditDisposition;
    pub fn committed_packet(&self)->u64;
}
```

credit解析仅固定72字节type2：原streamId、packetSequence，全部其它字段必须0；同真实Attachment关联context，无caller自报actor/socket参数。Detach type3同固定头，packetSequence及其它非stream字段必须0。binary不消耗120管理command；每个真正Data复制/发仍各自current验权。

| 原phase/packet | 收到credit n | registry内唯一结果 |
| --- | --- | --- |
| AwaitInitialCredit | n=0 | permit=1，ReadyToPrepare；重复0以后无新增permit |
| Prepared/Queued/WaitingSinkReady(p) | n=p | InvalidCredit：尚未真实start_send，不能凭猜packet跳未发 |
| Started/Flushing(p) | n=p | HeldEarly：记pending_ack=Some(p)，不增permit、不commitcursor |
| SentAwaitCredit(p) | n=p | 同一短操作核source/view/stream仍有效，消费Delivered事实+该credit，commit next cursor、committed_packet=p、permit=1 |
| 任意活跃phase | n≤highest_committed | Duplicate，无新增permit、不读下一块 |
| 任意活跃phase | n>active_packet或跳跃未发 | InvalidCredit，不为未来预留许可 |
| 已sealphase | 同socket原stream credit | SealedIgnored，只返回已知关闭metadata，不commit/发下一块 |

只有真实start_send成功之后允许HoldEarly；Started写入与实际start_send在同短锁，真实peer早credit只能在此后取锁，不能被误判“高于highest_sent”。highest_started是原StartSend事实计数，highest_flushed只在实际flush成功发布；**不再用highest_sent单变量合并两个事实**。

finish_flush取原ActualSocketFlushFact，在同registry短锁核原phase/Started token：Ok生成DeliveredViewChunk并写highest_flushed=p；若pending_ack==p且各seal仍有效，**同一次短操作**消费该ack和Delivered并提交cursor，转ReadyToPrepare；没有earlyAck则SentAwaitCredit。Err/timeout转FailedOrUnknown→seal_stream、cursor不变，任何pending_ack丢弃且不授许可。source/view/stream已seal时即便flush Ok，记录真实SentSucceededAfterSeal/SealedNoCommit，不提交旧cursor或准备下一块，保已开始字节事实。

commit_cursor只在上述两处真实Delivered+匹配credit交汇处调用，私有签名 `fn commit_delivered(&mut StreamSlot,delivered:DeliveredViewChunk,ack:ValidatedCredit)->CommitReceipt`；ValidatedCredit只由receive_credit同socket同stream校验mint，不可Clone/Deserialize。方法消费两原物、核原phase nonce，重复ack只能读取committed metadata。不能凭bool、network peer.write、Prepared、SnapshotEnd、原native WriteTicket Written铸Delivered或推进cursor。

SnapshotEnd也是唯一Data包，空payload/recordId>0，其cursor提交后才由Snapshot phase转Tail(after=H)；早credit与send失败规则完全相同，不让SnapshotEnd发送前进入tail漏记录。一个超大record每个chunk commit只推进该固定record byte offset，source版本在准备时锁定raw引用，后续anchor变化不换该record内容。stream允许累计任意多packet直到真实u64表示耗尽，不新加全body/packet配额。

## B5. 取消/归还与原对象责任表

| 边界 | 原物仍归谁 | 关闭/错误结果 |
| --- | --- | --- |
| open/barrier等待读者取消 | 稳定read task/view owner；未交正文 | 只撤该consumer，另一读者/source继续 |
| Preparing分配失败 | 原slot仍有source cursor/permit；无Prepared | typed错误零推进，raw仍node保留 |
| Prepared被Drop/abort | 同slot请求seal，未入outbound | 原metadata NeverStarted，不生成Delivered |
| enqueue拒绝 | ViewQueueRejected持原Prepared | into_original返同token；不可造新nonce重排同stream |
| enqueue成功，等待者Drop | 原outbound+稳定writer任务持Queued | caller无取消所有权，socket/seal按实际边界 |
| sink ready Pending时seal | 稳定writer持Queued，锁外等 | 后ready最终围栏拒绝；可主动wake退出，不等网络才seal |
| start_send成功/flush Pending时seal | 原writer持Started及同15秒deadline | 只已开始块可完成，零下一块，ND继续 |
| flush成功，credit尚未来 | 原slot持Delivered metadata/原cursor-next | SentAwaitCredit；无body重复发送，close释放本stream |
| earlycredit已保，flush失败 | 同slot写实际失败/封stream | 不commit，不能以peer ack覆盖本地send失败事实 |
| stream关闭/最后reader断线 | read/send cleanup任务 | 不清node raw，不session/cancel，new socket重新open |

所有错误的getter/into_original只在表述原物仍存在的阶段提供；进入真实sink后不可能“退回原Message”则不编造返回。原node内容源保留足以新open重读，但不自动续发原生输入或replay RPC。

## B6. 无正文测试观察点与实际可达性

StreamInspection getters=state、view_epoch、source_stamp、highest_started、highest_flushed、highest_committed、pending_ack、queued_count(0/1)、permit(0/1)、record address、seal_reason、actual start_send/flush result计数。只有view owner的cfg(test) Probe返回，无setter。hooks只停真实边界：BeforeChunkCopy、AfterChunkQueuedBeforeWriterGate、WriterGateHeld、BeforeSinkReady、AfterStartSendBeforeFlush、AfterFlushFactBeforePublish、BeforeCommitCursor；sourceSeal hook不持registry锁暂停。合法早credit测试停在AfterStartSendBeforeFlush/AfterFlushFactBeforePublish，真实peer实际收Data回Credit，再放行真实writer；伪未来ack对照在BeforeSinkReady。不能生产yield制造缝。

```rust
impl StreamInspection {
    pub fn state(&self)->ReadStreamState;
    pub fn view_epoch(&self)->ViewEpoch;
    pub fn source_stamp(&self)->ViewSourceStamp;
    pub fn highest_started(&self)->u64;
    pub fn highest_flushed(&self)->u64;
    pub fn highest_committed(&self)->u64;
    pub fn pending_ack(&self)->Option<u64>;
    pub fn queued_count(&self)->u8;
    pub fn permit(&self)->u8;
    pub fn address(&self)->Option<RecordAddress>;
    pub fn seal_reason(&self)->Option<ViewError>;
    pub fn start_send_calls(&self)->u64;
    pub fn flush_successes(&self)->u64;
    pub fn flush_failures(&self)->u64;
}
impl<S> ViewSendSeals<S> {
    pub(crate) fn seal_source(&self,proof:&VerifiedSourceViewSeal<S>)->ViewSealReceipt;
    pub(crate) fn seal_view(&self,proof:&VerifiedViewEpochSeal<S>)->ViewSealReceipt;
}
```

VerifiedViewEpochSeal只由同view owner真实current epoch替换转换产生，包含同target/old epoch/new epoch/source stamp及原read operation；不是任意epoch数据setter。source proof来源见C3。ViewSealReceipt getter=已封source/view平台stamp、was_already_sealed、started_chunks_count（已开始且未结束的实际数量）；采样不承诺已发字节撤回、raw已析构或native已停。

原existingwriter_holding_actor_gate/revoke_commit_discards_queued_result保持完整语义，binary通过同实际writer/gate路径。新SourceSeal与EarlyCredit边界是本版新增设计，必须独立实际WSS测试；L3codec/fixture可验证游标状态机，但不能以fake sink返回Ok代真网络线性事实。

## B7. L3模块实际字节传输（不伪WSS收据）

为使A6/A7 source+read模块在ND/WSS接线前有合法可执行正链，默认关闭fixture facade提供ModuleViewTransfer，其client binding是明确ModuleLocal，不能安装ViewSocketAttachment，Socket sender拒绝该kind；反之LocalFixtureSender也拒绝Socket kind。它们共享cursor状态算法但不用互相可转换的Delivered类型。

```rust
pub struct ModuleViewTransfer { /* real duplex peer+same local client/task slots */ }
pub struct CollectedModuleRecord { /* actual peer-received bytes+record metadata */ }
impl ModuleViewTransfer {
    pub async fn next_record(&mut self)->Result<CollectedModuleRecord,ViewError>;
    pub async fn credit(&mut self,packet:u64)->Result<(),ViewError>;
    pub async fn detach(&mut self)->Result<ReadCloseReceipt,ViewError>;
}
impl CollectedModuleRecord {
    pub fn address(&self)->RecordAddress;
    pub fn bytes(&self)->&[u8];
}
```

open_transfer先将原client/stream slot及真实duplex两端/LocalFixtureSender/receiver任务登记同view supervisor，之后才poll IO；peer只从真实duplex读取72字节header+原payload，credit实际编码写回另一端。LocalFixtureSender用当前A7真policy gate/current_actor再写，单chunk不预授全body权限。

本地最后围栏是SendRegistry同短锁内**第一次真实AsyncWrite::poll_write接受>0字节**：Pending不标Started，Err即使accepted=0也不伪已送达；首次正接受后该固定chunk可继续，其余source seal不阻ND。全部header+payload真实accepted后poll_flush Ok才mint私有ActualLocalIoFlushFact。它只供ModuleLocal分支的本地Delivered事实，不是ActualSocketFlushFact，不存在From/cast到WSS Delivered；同commit算法仍要求匹配真实peer credit才推进cursor。测试不能提供一个always-Ok writer来算“真实收到”；必须真实duplex peer逐字节重组断言，慢peer和错credit走相同codec/实际IO。

这条路径使SV011–036等模块字节/状态断言可实际触达，不宣称WebSocket start_send/网络writer/SourceSlot ack已验；对应SV065/070/074等L4/L5分支仍必须B3真实WebSocket。fixture原始Collected bytes是已经交付测试client的数据，不开放未当前鉴权的源raw读取接口。
