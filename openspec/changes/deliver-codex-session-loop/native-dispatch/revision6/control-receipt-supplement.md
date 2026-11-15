# v6 窄补充：无RPC控制取消收据

2026-11-01。只作 ND-CONTROL-001 收据类型消歧，不编码、不执行SQL。基线为 v6 freeze SHA `9d1349f6545459a10cf8551e5f3d9681d4f80e5e1990f8a6237457fd0fee5869`，其12成员和源快照保持原字节。独立报告 `6c7dff54dc360eb47a46350447a04b5271b4ca9dd9649b169b133414d3b33f36` 为本次依据。

本包 `control-ports.md` 和 `closure-contract.md` 是两份完整覆盖文件，其余六份主合同及DDL沿 inputs.json 精确指向原v6摘要。此附录是两处收据接口的唯一类型说明；未覆盖的v6职责、产品规则与门禁全部不变。独立NDT v5最终80定义仍由tester冻结，不把消息交流当定义通过。

## 1. 独立收据与只读接口

```rust
pub struct CancelledControlReservation<S, Probe> {
    identity: ControlIdentity<S>,
    probe: Probe,
}
impl<S, Probe> CancelledControlReservation<S, Probe> {
    pub fn identity(&self) -> &ControlIdentity<S>;
    pub fn probe(&self) -> &Probe;
}
impl<S> ControlIdentity<S> {
    pub fn scope(&self) -> &S;
    pub fn rpc(&self) -> Option<&RpcScalar>;
    pub fn native_session(&self) -> Option<&str>;
}
// NativeControlPort 内唯一替换签名：
fn cancel_reserved_control(&self, reservation: Self::ControlReservation)
    -> CancelledControlReservation<Self::Scope, Self::Probe>;
```

receipt 字段私有，无公开构造、Default、Clone、Deserialize、setter、into_parts或到Reservation/Ticket/Frame的转换。Send/Sync只沿实际S/Probe的相应自动traits；Debug脱敏，不打印session/raw/任意内容。受信adapter构造边界只有这一次原Reservation.cancel的生产调用，应用不能用SQL/DTO或裸ControlIdentity+Probe组装收据。只读identity本身是数据，原Probe即使可Clone也只是观察器，不是未来写资格。

ControlIdentity沿原枚举逐项投影，不新建统一必有RPC类型：

| 实际原kind | scope() | rpc() | native_session() |
| --- | --- | --- | --- |
| NativeCancel | 同原scope引用 | None（原Reservation.id()确为None） | Some(原session) |
| PermissionCancelled | 同原scope引用 | Some(原typed请求ID) | Some(原session) |
| Unsupported | 同原scope引用 | Some(原typed请求ID) | None |

数字和同形string的typed身份继续保留。实际ACP WriteIdentity.id()/Reservation.id()/WriteProbe.id() 的 Option 语义不变；若称它为source/write identity投影，其rpc同样只能是上述真实None/Some，不补伪ID、不用control_id或平台request ID充RPC。原Prompt与Load的checked wrapper仍可以有必有typedRPC getter，因为其合法实例真实具有RPC；不能反向把这个约束套到control。

## 2. 原物消费与SourceSlot阶段签名

```rust
impl<P: NativeControlPort> OwnedControlOperation<P> {
    pub(crate) fn cancel_unenqueued(&mut self, protocol: &P)
        -> Result<(), DispatchError>;
    pub(crate) fn cancelled_receipt(&self)
        -> Option<&CancelledControlReservation<P::Scope, P::Probe>>;
}
```

cancel_unenqueued只在同SourceSlot/原ControlOperation实际持有尚未enqueue的原ControlReservation时执行：在准确stage内move该原对象给P.cancel_reserved_control，adapter先从原wrapper保留原ControlIdentity，再实际consume原ACP Reservation.cancel取得同cell Probe；同步把返回receipt存回同操作私有 `CancelledUnenqueued{receipt, frame_length:Option<u64>}` stage。若旧stage为Enqueued，只返回typed错误，唯一原Ticket仍在原RunningControl，调用方必须走Ticket.request_cancel/wait。没有“传入任意receipt绑定到另一operation”的setter或accept方法。

原Frame若已encode，保已检查长度用于原metadata后由同slot丢弃；未encode成功则长度None，只可按v6实际NeverEnqueued Zero分支保存，不伪空帧。receipt的identity来自这次同原Reservation，不能从活动target重建；Probe来自同一次实际cancel，而非registry查询或其它连接。方法的stage错误不取走/丢失原对象，callerDrop/panic保同supervisor槽负责cleanup。

receipt不单独宣称FinishedZero；同slot只读P.probe_progress(receipt.probe())，确为Finished(Zero)后才形成本次原operation的终态事实。None/Some RPC按identity原分支保存；native cancel的SQL rpc两列仍NULL，其它两类仍原typedID。ReturnedProbe Reserved/Unknown/Written不得捏Zero，原必要gap/期满/实际source停止按v6原责任处理。

SourceWorkSeal/TargetWorkSeal及SourceSqlQuiescence/TargetSqlQuiescence沿v6完整集合，未enqueue控制项现在收集该**控制专用**receipt与同操作身份/原frame可选长度；不会把它当Load取消receipt，也不要求receipt有typedRPC。它只支持原operation的实际无未来poll/Zero核验，不能代替Owned/IO/Job关闭证明、native终态、Permission失效或原必要SQL可靠证明。

v6 `into_reserved_control_zero(op, stored)` 的参数签名不变，但只接受同op的CancelledUnenqueued阶段：取该receipt真实Probe终态Zero，核该原control_id/obligation/attempt/current原品牌/完整StoredControl，再沿既有线性转换。原receipt一并move保在ControlZeroRetry历史中；任何只读probe/identity拷贝均无权mint该对象。enqueue错误返原Reservation+Frame的既有路径，若随后取消，统一走cancel_unenqueued，不另造收据。

## 3. Load专用取消不变

以下原NativeLifecyclePort签名与Load receipt结构完全不变：

```rust
fn cancel_resume(&self, reservation: Self::LoadReservation)
    -> CancelledLifecycleReservation<Self::Scope, Self::Probe>;
// 原私有Load收据：{ scope, typedRPC, probe, kind }
```

LoadReservation仍由实际checked Resume Reservation产生，原id必须Some。closure-contract原UnloadedReservationAbandoned证明继续只消费这类真实Load取消/原TicketZero，不接受CancelledControlReservation代替。控制收据不增加新的close/abandon/release构造入口。

## 4. 直接引用清单与验收对照

本包control-ports §1已替换唯一直接错误返回类型，新增精确receipt/getters；§2实际映射表、§5阶段存储/knownZero转换及失败表统一引用本控制收据。closure-contract保原Load签名/必有RPC，在尾部加入控制专用阶段签名和seal集合约束。lifecycle-ports中的cancel_resume原返回类型保原文，interfaces没有第三处控制取消签名需要替换；其它v6合同通过同包控制附件链接到本覆盖文件，inputs.json列完整overlay映射。

独立tester可在同80ID/标题下补精确子断言：实际reserve_cancel→原ControlReservation→cancel_unenqueued→cancel_reserved_control→同Probe FinishedZero，receipt.identity.rpc()==None；另外两种真实品牌请求的取消receipt仍Some原typedID。核scope/原request/原cell归属、未enqueue对照已enqueue/已spent路径、无public ctor/Clone/Deserialize/任意probe→grant转换。旧Load的SomeRPC正例与错把control receipt传Load证明的负编译仍须保留。仅getter返回None/Some的mock不能替实际ACP路径。

本轮只hash/文本静态核对；实际编译/SQL/动态运行均0，独立gate仍pending。类型修正没有改变DDL、actor受理、停止/unknown权限政策、已确认scope或历史v6字节。
