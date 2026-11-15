# WP3 公共 ACP 客户端与受监督 IO 合同 revision2

2026-10-31。仅编码前工程合同；没有产品修改或运行通过声明。此文件与同目录 limits-and-migration.md 完整取代 v1 的候选工程表述，v1 文件原字节保留。产品依据仍是 inputs.json 固定的当前八 spec/design/implementation-contract；raw 基础合同 9f2b、SDK 2.1.0 stable 双版 schema 不变。此次精化对应独立 ACP-PLAN001–003，不改变已确认产品规则。必须同版独立定义及预审通过后才编码。

实施交付是可实际操作的 ACP 客户端、持续 reader、单 writer 和可取消可 join 的 IO owner；不能仅交 traits/CAS。顺序：本公共客户端 → 必要派发账本及 actual R3 owner 接该 IO → 资源/Node/WSS。此阶段没有 SQL/NativeDispatch/Run 因果推断，不改原 DDL，不声称真实 Harness 或 Windows Job 树验证。

## 1. raw 数据与公开模型

沿用 raw_json::{RawDocument,OwnedRawValue,RawValueRef,RawObject,RawArray,RawKind}，不重写解析器、不增加 Value 兼容入口、隐式 DOM 或递归未知树。以下代码块是公开签名清单，省略实现体；类型名从 adapters::acp 公共 re-export 可达。所有外来数据的 Debug/Display 只输出类型、长度、静态原因，包含路径、env/header、选项、原生 ID、RPC ID、方法名及配置 stamp 的原值均不输出。所有错误不附 parser 原片段。

```rust
impl RawMessage {
 pub fn parse(bytes:Vec<u8>)->Result<Self,AcpError>;
 pub fn bytes(&self)->&[u8];
 pub fn fields(&self)->Result<RawObject<'_>,AcpError>;
 pub fn field(&self,key:&str)->Option<RawValueRef<'_>>;
}
impl RpcId {
 pub fn parse(value:RawValueRef<'_>)->Result<Self,AcpError>;
 pub fn write_json(&self,out:&mut Vec<u8>)->Result<(),AcpError>;
}
// 四种 ID 均有下列签名；只有 SessionId、OptionId 保原 4096 字节保护。
impl MessageId { pub fn parse(value:RawValueRef<'_>)->Result<Self,AcpError>;
 pub fn as_str(&self)->&str; }
// ToolCallId、SessionId、OptionId 同签名。
impl Request {
 pub fn kind(&self)->&RequestKind;
 pub fn params(&self)->RawValueRef<'_>;
 pub fn setup_stamp(&self)->Option<&ConfigSourceStamp>;
}
impl OrderedRawObject {
 pub fn from_raw(value:OwnedRawValue)->Result<Self,AcpError>;
 pub fn len(&self)->usize;
 pub fn is_empty(&self)->bool;
 pub fn get(&self,key:&str)->Option<RawValueRef<'_>>;
 pub fn iter(&self)->impl ExactSizeIterator<Item=(&str,RawValueRef<'_>)>
                            + DoubleEndedIterator + '_;
 pub fn to_raw(&self)->Result<OwnedRawValue,AcpError>;
}
pub enum RawMetadata { Null, Object(OrderedRawObject) }
impl RawMetadata {
 pub fn from_raw(value:OwnedRawValue)->Result<Self,AcpError>;
 pub fn is_null(&self)->bool;
 pub fn as_object(&self)->Option<&OrderedRawObject>;
 pub fn to_raw(&self)->Result<OwnedRawValue,AcpError>;
}
```

RawMessage 私有持 RawDocument，仅接受根 object。parse 保参数的完整原字节（包括 JSON 前后空白）；真实 reader 仅在调用前移除 framing LF 及紧邻它的可选 CR。语法 parse 不验证 jsonrpc/method/id/result/error 路由形状。语法正确而路由拒绝的原文可 take_rejected_message，非法 JSON 不伪造成 RawMessage。fields 的 Result 不用 unwrap/expect 为内部不变量兜底。

RpcId 保现有 Integer(i64)/String(Arc<str>) 的 typed 身份、字符串 decoded UTF8 1024、安全整数 ±9007199254740991；拒 null、分数、指数及原词法 -0，不能经 raw.integer() 将它们归一化放行。v1 schema 容许 null、双版 integer 为 int64；这些拒绝是原平台兼容差异，不冒称协议要求，也不施加到未知 raw 数字。四种原生 ID 均仅收 string、不截断；Message/Tool 去纯长度拒绝，Session/Option 保现有控制保护，见限制表。

OrderedRawObject 私有保存有序 (Arc<str>,OwnedRawValue) 成员，只收 object；iter 依原次序，get 用 decoded key。RawMetadata 仅收 null/object。Clone 分享原子树；有 patch 时结果是浅投影，外层 key 可重新 JSON 编码，未改 key 原位、新 key 按 patch 次序追加，值子树保原词法，不递归 merge。原 Incoming.raw/patch 原文始终另存，不把投影重编码当收到原字节。

Request 私有 scope/kind/完整 raw 参数，仅由同源 Strategy 构造。真实握手产生的 Strategy 使用本 ConnectionScope 身份，不另 mint 一个不能与 IO 对齐的 scope。prompt、_meta、rawInput/rawOutput、未知扩展均保完整原片段。最终 NDJSON 只 compact 字符串外四种 JSON 空白，保数字词法、escape、成员次序，末尾恰一个 LF；未知 JSON 内实际字符串不能包含非法 literal LF。

```rust
impl Strategy {
 pub fn negotiated(&self)->&Negotiated;
 pub fn version(&self)->Version;
 pub fn new_session(&self,setup:&SessionSetup)->Result<Request,AcpError>;
 pub fn resume(&self,session:SessionId,setup:&SessionSetup)->Result<Request,AcpError>;
 pub fn replay(&self,session:SessionId,setup:&SessionSetup)->Result<Request,AcpError>;
 pub fn prompt(&self,session:SessionId,prompt:OwnedRawValue,
               extra:ExtensionFields)->Result<Request,AcpError>;
 pub fn list_sessions(&self,cwd:Option<&str>,cursor:Option<&str>,
                      extra:ExtensionFields)->Result<Request,AcpError>;
 pub fn close(&self,session:SessionId,extra:ExtensionFields)->Result<Request,AcpError>;
 pub fn prompt_result(&self,value:RawValueRef<'_>)->Result<PromptResult,AcpError>;
 pub fn update(&self,value:RawValueRef<'_>)->Result<SessionEvent,AcpError>;
 pub fn session_listing(&self,value:RawValueRef<'_>)->Result<SessionListing,AcpError>;
 pub fn permission(&self,id:RpcId,raw:RawMessage,params:RawValueRef<'_>)
                   ->Result<PermissionRequest,AcpError>;
 pub fn tool_name_patch(&self,update:RawValueRef<'_>)->Result<ToolNamePatch,AcpError>;
 pub fn tool_metadata_patch(&self,current:&RawMetadata,update:RawValueRef<'_>)
                            ->Result<RawMetadata,AcpError>;
}
pub enum ToolNamePatch { Unchanged, Clear, Set(Arc<str>) }
```

prompt 必须 array，空 [] 合法进入实际 Harness，不保旧 filter(!empty)。已知 type/required/真实 capability 验证继续；v2 Other 按该版 schema 整片 forwarding，但不能将已知 type 包成 Other 绕 cap。v1 未知类型按固定该版可编码范围拒绝，不转文本。tool_name_patch 保 v1 null 不变/v2 null 清空；metadata v1 Unsupported，v2 缺失不变/null 清空/object 顶层替换且 null 值删 key。PromptResult 保 v1 stopReason 完成/v2 messageId 插入差别；SessionEvent 保版本字段/null/增量语义，aggregate idle 不产生 Run 归因。

## 2. 完整配置、列表与权限句柄

配置来源和版本由可信应用给完整快照；类型本身不是授权 grant，协议不扫描私有配置或访问文件系统。Unknown 仅阻所需字段的 setup 路径，不阻不依赖它的 prompt/list/permission。没有 Default。多 Unknown 固定按 Cwd → Mcp → AdditionalDirectories 返回首个 Incomplete；各项 Known(empty) 仍是明确完整空 Client 配置，不代表 Agent 内置工具为空。

```rust
pub enum Known<T> { Unknown, Known(T) }
pub struct ConfigSourceStamp { pub reference:Arc<str>, pub revision:u64 }
pub struct SetupSnapshot {
 pub wire:Version, pub source:ConfigSourceStamp, pub cwd:Known<Arc<str>>,
 pub mcp_servers:Known<Vec<McpServer>>,
 pub additional_directories:Known<Vec<Arc<str>>>, pub extra:ExtensionFields,
}
pub enum SetupField { Cwd, Mcp, AdditionalDirectories }
pub enum SetupError { Incomplete(SetupField), VersionMismatch, InvalidShape,
 ReservedField, UnsupportedEncoding, Allocation }
impl SessionSetup {
 pub fn try_from_snapshot(value:SetupSnapshot)->Result<Self,SetupError>;
 pub fn wire(&self)->Version;
 pub fn source(&self)->&ConfigSourceStamp;
 pub fn cwd(&self)->&str;
 pub fn mcp_servers(&self)->&[McpServer];
 pub fn additional_directories(&self)->&[Arc<str>];
 pub fn extra(&self)->&ExtensionFields;
}
impl ExtensionFields {
 pub fn empty()->Self;
 pub fn from_raw(value:OwnedRawValue)->Result<Self,SetupError>;
 pub fn as_raw(&self)->RawValueRef<'_>;
}
pub enum McpTransport { Stdio, Http, Sse, Other(Arc<str>) }
impl NamedValue {
 pub fn new(name:Arc<str>,value:Arc<str>,extra:ExtensionFields)->Result<Self,SetupError>;
 pub fn name(&self)->&str;
 pub fn value(&self)->&str;
 pub fn extra(&self)->&ExtensionFields;
}
impl McpServer {
 pub fn from_raw(wire:Version,value:OwnedRawValue)->Result<Self,SetupError>;
 pub fn stdio(wire:Version,name:Arc<str>,command:Arc<str>,args:Vec<Arc<str>>,
              env:Vec<NamedValue>,extra:ExtensionFields)->Result<Self,SetupError>;
 pub fn http(wire:Version,name:Arc<str>,url:Arc<str>,headers:Vec<NamedValue>,
             extra:ExtensionFields)->Result<Self,SetupError>;
 pub fn sse_v1(name:Arc<str>,url:Arc<str>,headers:Vec<NamedValue>,
               extra:ExtensionFields)->Result<Self,SetupError>;
 pub fn wire(&self)->Version;
 pub fn transport(&self)->&McpTransport;
 pub fn as_raw(&self)->RawValueRef<'_>;
}
```

ExtensionFields 仅收 object；每个构造/编码路径按 decoded key 拒相应受控字段碰撞（含 escaped 同名），不覆盖 sessionId/cwd/mcpServers/additionalDirectories/replayFrom 等。_meta 的 object/null 和未知成员保 raw。输出 compact 整 object 后按受检外花括号拼接成员片段，未知 key 的 escape 和次序不重新序列化。McpServer 同版 raw 整片保留；跨版不能无损转码仅此 setup 报 UnencodableConfiguration，不删字段/换空配置重试。env/header 用 Vec 保顺序和重复项，不转 map。

现有 4096 参数检查的精确放置：Strategy::list_sessions 只检查其 Some(cwd)/Some(cursor)；Strategy::replay 只检查 setup.cwd；Strategy::session_listing 只检查 entry.cwd/nextCursor。均以 decoded UTF8 字节、超限 Capacity 保原语义。SessionSetup::try_from_snapshot、McpServer 及 Strategy::new_session/resume 不新增该长度 cap，也不将 replay 的检查抽进共享 setup 构造器。SessionListing 条数检查删除，与上述字段检查独立。

| 配置/能力 | 固定 v1 | 固定 v2 |
| --- | --- | --- |
| new/resume/replay | new/load cwd+mcpServers 必填，resume cwd 必填；应用全部提供完整 setup；load 重放须 loadSession=true，resume 须独立 resume 对象广告 | new/resume cwd 必填，完整 MCP 显式编码；session:{} 广告 baseline session 方法；无 load |
| replay | session/load；resume 不冒重放 | session/resume + replayFrom:{type:start}；普通恢复省 replayFrom，不猜 cursor 能力 |
| additionalDirectories | 非空须 sessionCapabilities.additionalDirectories 对象 | 非空须 capabilities.session.additionalDirectories 对象；两版明确空可兼容省略，绝不保留旧未知值 |
| stdio | 无必需 type，name/command/args/env 必填，Agent 必备 | type:stdio，name/command 必填；完整快照 args/env 显式；须 session.mcp.stdio 对象，session:{} 不代替 |
| http | type:http，name/url/headers 必填，mcpCapabilities.http=true | type:http，name/url 必填，完整 headers 显式；须 session.mcp.http 对象 |
| sse/Other | sse 须 type:sse、sse=true；无该版可编码依据的 unknown transport 明确限此路径 | Other 同版标准 opaque forwarding，不将 sse 改 http，不虚报 Agent 支持；不能把 known type 包 Other 绕 cap。不支持由 Agent 拒绝，不重试缩水配置 |

列表报告不 mint SessionSetup 或授权。ReportedDirectories 的 Omitted/Empty 按固定 schema 的报告语义呈现；v2 prose 的 null 与 schema array 差异显式标 Null，不暗当 KnownEmpty；MCP 未报告不能补空。每项和整个列表保存原 raw。

```rust
pub enum ReportedDirectories {
 Omitted, Empty, Known(Vec<Arc<str>>), Null, Uninterpreted(OwnedRawValue),
}
impl ReportedDirectories {
 pub fn known(&self)->Option<&[Arc<str>]>;
}
impl SessionListItem {
 pub fn session(&self)->&SessionId;
 pub fn cwd(&self)->&str;
 pub fn additional_directories(&self)->&ReportedDirectories;
 pub fn raw(&self)->RawValueRef<'_>;
}
impl SessionListing {
 pub fn sessions(&self)->&[SessionListItem];
 pub fn next_cursor(&self)->Option<&str>;
 pub fn raw(&self)->RawValueRef<'_>;
}
impl PermissionRequest {
 pub fn id(&self)->&RpcId;
 pub fn session(&self)->&SessionId;
 pub fn tool(&self)->Option<&ToolCallId>;
 pub fn supported_subject(&self)->bool;
 pub fn options(&self)->&[PermissionChoice];
 pub fn raw(&self)->&RawMessage;
 pub fn scope(&self)->Option<ConnectionScope>;
}
impl PermissionChoice {
 pub fn id(&self)->&OptionId;
 pub fn kind(&self)->&str;
 pub fn supported(&self)->bool;
 pub fn raw(&self)->RawValueRef<'_>;
}
impl UnsupportedRequest {
 pub fn id(&self)->&RpcId;
 pub fn method(&self)->&str;
 pub fn raw(&self)->&RawMessage;
 pub fn scope(&self)->ConnectionScope;
}
```

known() 仅 Empty 返回 Some(&[])、Known 返回其 slice，其余 None；不把 Unknown 报告默认为空。上述私有数据类型与 Known/SetupSnapshot/SetupError/ConfigSourceStamp/McpTransport/ReportedDirectories 均可 Clone，敏感 Debug 脱敏；引用不提供修改底层对象的方法。Strategy.permission 是纯解析，scope=None；真实 reader 的 PermissionRequest scope=Some(本连接)，UnsupportedRequest 仅真实 reader 构造。supported_subject/supported 为旧诊断投影，不是自动许可；未知 subject/kind 仍允许人手选择原广告中唯一匹配选项。

## 3. 实际客户端入口及唯一责任转交

```rust
impl Connection {
 pub fn spawn<R,W>(reader:R,writer:W)
   ->Result<(Connection,EventReceiver,IoOwner),AcpError>
 where R:AsyncRead+Unpin+Send+'static,W:AsyncWrite+Unpin+Send+'static;
 pub fn scope(&self)->ConnectionScope;
 pub fn strategy(&self)->Option<Strategy>;
 pub fn reserve_initialize(&self,offer:Version,response_deadline:Instant)
   ->Result<Reservation,AcpError>;
 pub fn reserve_preferred_initialize(&self,response_deadline:Instant)
   ->Result<Reservation,AcpError>;
 pub fn reserve_request(&self,request:Request,response_deadline:ResponseDeadline)
   ->Result<Reservation,AcpError>;
 pub fn reserve_cancel(&self,session:SessionId,extra:ExtensionFields)
   ->Result<Reservation,AcpError>;
 pub fn reserve_permission_response(&self,request:&PermissionRequest,
   choice:PermissionSelection)->Result<Reservation,AcpError>;
 pub fn reserve_unsupported(&self,request:&UnsupportedRequest)
   ->Result<Reservation,AcpError>;
 pub fn enqueue(&self,reservation:Reservation,frame:EncodedFrame,
   fence:Arc<dyn WriteFence>,write_deadline:Instant)->Result<WriteTicket,EnqueueError>;
 pub fn pending_count(&self)->usize;
 pub fn server_pending_count(&self)->usize;
 pub fn is_closed(&self)->bool;
 pub fn take_rejected_message(&self)->Option<RawMessage>;
}
pub enum ResponseDeadline { At(Instant), Unbounded }
impl Reservation {
 pub fn scope(&self)->ConnectionScope;
 pub fn id(&self)->Option<&RpcId>;
 pub fn kind(&self)->&OutboundKind;
 pub fn setup_stamp(&self)->Option<&ConfigSourceStamp>;
 pub fn encode(&self)->Result<EncodedFrame,AcpError>;
 pub fn probe(&self)->WriteProbe;
 pub fn cancel(self)->WriteProbe;
}
impl EncodedFrame { pub fn bytes(&self)->&[u8]; }
impl EnqueueError {
 pub fn error(&self)->AcpError;
 pub fn into_parts(self)->(AcpError,Reservation,EncodedFrame);
}
impl WriteTicket {
 pub fn probe(&self)->WriteProbe;
 pub fn progress(&self)->WriteProgress;
 pub fn request_cancel(&self);
 pub async fn wait(&self)->WriteReport;
 pub async fn cancel_and_wait(&self)->WriteReport;
}
impl WriteProbe {
 pub fn scope(&self)->ConnectionScope;
 pub fn id(&self)->Option<&RpcId>;
 pub fn progress(&self)->WriteProgress;
}
impl EventReceiver { pub async fn next(&mut self)->Result<Option<Received>,AcpError>; }
impl IoOwner { pub async fn shutdown(&self,deadline:Instant)->IoShutdownReport; }
```

Reservation、EncodedFrame、EnqueueError、WriteTicket、EventReceiver、IoOwner 均不 Clone；Connection/WriteProbe/ConnectionScope 可 Clone。ConnectionScope 私有 Arc 身份、Eq/PartialEq 仅同连接，无 public 构造或反序列化。所有公开异步 Future 为 Send。spawn 同步捕获 Handle::try_current，失败 typed RuntimeUnavailable；首 await 前同 Core 保存两个 IO task 的取消/join 责任。所有 Drop 无 ambient runtime 依赖，不在无 runtime 线程 panic。

**唯一 RAII 所有权表：**

| 时点 | 唯一动作责任与错误/Drop |
| --- | --- |
| reserve 成功 | Reservation 唯一持未 enqueue 责任，Core 持 cell 事实和定时责任。client RPC pending、同 session setup 位及 server ReplyReserved 已占位，但无 writer 指令 |
| encode(&self) 成功/失败 | 不转交责任，不 I/O；失败保 Reservation 可重试/drop。Frame 只持私有 scope+cell generation 绑定和 bytes；不持/释放任何槽。多次 encode 不增加可发送次数 |
| enqueue 校验失败 | 返回 EnqueueError，包含原 Reservation 和传入 Frame，零入队/零 poll。into_parts 可取回重试；直接 drop error 等于 drop 其 Reservation，Frame 的 Drop 无槽副作用 |
| enqueue 成功 | 同 ProtocolState 短锁一次消费 Reservation 的 Armed 标记并建立 writer owned cell；返回 Ticket 唯一取消意图责任，Reservation 析构不再释放。不存在先消费后失败且无人持有的分支 |
| 未 enqueue Reservation Drop | 同 state 短锁，将仍属它且未终态 cell 记 Zero(Abandoned)，释放其 client pending/setup 位；server ReplyReserved 恢复同原请求 Waiting。server_seen 永不删。已先由期限/关闭终结则不覆写其首个终态 |
| Reservation.cancel(self) | 同上，以 Zero(Cancelled) 请求终结并返回只读 probe；已存在终态保持不变，不建立可伪造释放 grant |
| Ticket Drop/request_cancel | 同步标取消，停止后续首 poll；已排队由 IO owner 确认终态。Ticket Drop 的零原因为 Abandoned，显式 request_cancel/cancel_and_wait 为 Cancelled；首个生效原因保持。借用 wait Future Drop 仅停止等待，不丢 Ticket/Core 任务 |

若 Drop 为 zero server reply，恢复 Waiting 仅在原同 scope/原 generation 的 live 请求仍存在时；已 ClosingDrain 不再 mint 可回答 grant，但 seen 保留。已接受任意字节或 IO 不明时 server reply 为 Spent，不能第二次回复。RPC ID 生成不回收重用；释放普通 pending 不复活 ID。

enqueue 校验顺序固定：先 reservation.scope 是否本连接（InvalidRequestScope）；再 Frame.scope/cell generation 是否恰该 Reservation（InvalidFrame）；再该 cell 首个终态（对应 Deadline/Cancelled/Closed）；再 phase 是否允写（Closed）；最后 write_deadline 是否已到（将该零 cell 终结 Deadline 并返回 Deadline）。参数冲突错误不改另一个 Frame 所属 cell，错误所有权仍在返回值。成功后不得重复 enqueue 同 Reservation。reserve_* 在占槽前验证来源/请求形状/phase/capacity；已过 response_deadline 返回 Deadline 而不占槽。reserve_initialize 已成功即永久封一次尝试，随后弃置、encode/enqueue 失败或零超时也不允许同连接再 initialize；重复 initialize 优先 AlreadyInitialized，不以 Closed 掩盖重试禁令。

两方向各 64 **同时** pending 保留，同 session resume/replay 从 reserve 互斥、零释放。server 第 65 及重复 request 保原 Capacity/poison 语义；普通 client 第 65 返回 Capacity、不暗 poison。server_seen 为连接寿命 HashSet，无累计阈值、无清理复活；8193/更后新 ID 继续处理。初始化只广告已实现客户端能力，不自动降版/重发。

两方向 ID 表独立，numeric/string 同形仍不同 typed 身份。server_pending 保原 raw 字节 SHA256 与连接 scope 的核验；reply reserve 从真实原广告校验 session/选项，不能信重建投影。终态只读 Probe 不保 Request/EncodedFrame/prompt/完整 setup；Core writer 槽在终态释放这些缓冲。调用方仍自行持有的 Frame/RawMessage/Reservation 数据按其普通 Rust 所有权存活，不冒称可强制清除外部副本。

## 4. 每一次实际 writer poll 的私有外部围栏

```rust
pub enum WriteStep { Chunk{offset:usize,total:usize}, Flush{total:usize} }
pub enum IoProgress { Accepted(usize), Flushed }
pub enum FenceRejection {
 Stopped, Cancelled, StaleTarget, RevisionChanged, NotReady, RequiredStateUnavailable,
}
pub trait PollIo {
 fn poll(&mut self,cx:&mut Context<'_>)->Poll<Result<IoProgress,AcpError>>;
}
pub trait WriteFence:Send+Sync {
 fn check_and_poll(&self,identity:&WriteIdentity,step:WriteStep,cx:&mut Context<'_>,
                   io:&mut dyn PollIo)->Result<(),FenceRejection>;
}
impl WriteIdentity {
 pub fn scope(&self)->ConnectionScope;
 pub fn id(&self)->Option<&RpcId>;
 pub fn kind(&self)->&OutboundKind;
 pub fn setup_stamp(&self)->Option<&ConfigSourceStamp>;
}
```

应用私有 fence 取得自己的短锁，核 identity/取消/配置 revision/资源，锁内恰一次 io.poll(cx)，更新自己本次 poll 记录再返回 Ok；即使实际返回 Pending/Err，合法 fence 仍返回 Ok。拒绝必须未调用 io 时 Err。协议代理只能真实调用当前独占 W 一次，第二次调用不再碰 W 并记录 InvalidFence；不信 fence 自报字节。Ok 零次、尝试两次、Err 前已 poll 均 InvalidFence，保持实际 accepted 范围，不假 Zero/Written。

每次 poll_write 和 poll_flush 都围栏，Pending 后再 poll 必须重验。外部 App 锁包住内部 ProtocolState 短锁、真实非阻塞 poll 和 offset/FrameAccepted 更新；reader 只持后者、不反向调用 App。编码、parse、digest、SQL/授权等待不得在这些锁内。仅 Reserved/Queued 或真实 Pending0 可 Zero；任何可能副作用的 IO Err 即使计数 0 仍 Unknown。FrameAccepted 是完整含 LF 字节已被 W 接受，不是 flush、ACK、插入或完成。

W 是受信独占 AsyncWrite，不能后台自主发送或 Drop 首写/flush；生产使用实际 unbuffered 管道。可控 buffered/mock W 的证明仅到其 AsyncWrite 层，不等于对端接收。正常关闭不 poll_shutdown/flush 未发残帧绕 fence；若需要 flush，只能正常 writer 命令的 fenced poll_flush。

## 5. 读取线性化点、来源窗口与完整帧取得

真实 reader 持独占 R，单 reader 顺序完成 framing/parse/route；SQL、consumer 不 next、writer 背压均不阻它。不能用隐藏预读 BufReader 将无标记旧 bytes 日后当新观察。处理 scratch 是分块，不是总帧容量。

**唯一接收线性化点：** 在 ProtocolState 短锁内调用实际 R.poll_read；Ready(Ok) 增加的字节与该次实际读取结果在锁内一并取得本地 ReadReceiptCut，然后释放锁。cut 私有且保存本地单调 receipt_order、取样 Instant、当时已经 FrameAccepted 的 client (typed ID,cell generation) 集合以及当时已打开的 (session,read-window generation) 集合。最多 64 pending 的浅元数据快照可分享，不含正文；计数 checked 耗尽 typed Capacity 并 Stop，不回绕。此 order 只证明本地处理顺序，不证明原生发送时间、Run 因果或网络实际发生时刻。

writer 实际 poll 返回及 FrameAccepted 更新使用同一短锁。因此 reader 不会在 W 已接受 LF 却未补本地 FrameAccepted 的缝隙捕获 response。反之，读到完整 response LF 时若只有 Reserved/Pending0/partial，后来的 FrameAccepted 绝不能补资格：路由必须同时满足 cut 包含该 ID+generation、当前 body-free 原记录未消费以及合法原 scope/typed ID，缺一即原 raw + UnmatchedResponse/poison。

每个 frame 以**包含其终止 LF 的实际 read 批次 cut**作为 receipt；帧 prefix 可以来自更早 read。一次 read 返回多帧，共用 cut、按 LF 字节序分别路由。一个完整帧的“已取得”定义是：终止 LF 已在 Stop 之前的实际 Ready read 返回字节中，reader 已拥有这些字节；不要求已脱离 framing buffer/已 parse。短锁内同步记录该批次 bytes 的取得和 LF 边界（只扫描本次有限 scratch）；大 detach、raw 索引和 projection 在锁外。这样关闭不能丢掉同批次尚未拆出的第二个完整帧。无 LF 的最终 prefix 不是已取得完整帧，Stop 后不得继续读补齐。EOF 前已有 LF 帧照常路由，EOF 半帧如实错误。

read-window 仅在对应 resume/replay 请求 FrameAccepted 的同一短边界打开。UpdateOrigin 取 cut 的窗口资格，并在逐帧 route 时核同 generation 仍未被更前的 response 关闭；不在 parse 完成时根据“当前有 resume”补发 DuringResume/Replay。若同批次先 response 后 update，前一帧关闭窗口，后一帧 Live；若先 state 后 response，则可满足原 v2 snapshot 规则。先收到完整 state、锁外 parse 时才打开 resume 的反例仍 Live。原 window generation 从不复用，旧 cut 不借新窗口。P1/P2 不能重算该 origin；v2 MissingSnapshot 继续，窗口/snapshot/aggregate idle 不 mint Run 终态。

```rust
impl Received {
 pub fn scope(&self)->ConnectionScope;
 pub fn sequence(&self)->u64;
 pub fn receipt_order(&self)->u64;
 pub fn incoming(&self)->&Incoming;
 pub fn raw(&self)->&RawMessage;
}
```

Received 可 Clone、私有构造，仅真实 reader mint；sequence 是路由输出序，receipt_order 是本地读取 cut 序，多帧可同 receipt_order；均无 public factory/Deserialize。Incoming 保 Response{id,response,raw}/Update{event,origin,raw}/Permission(PermissionRequest)/UnknownNotification{raw}，Unsupported 改 UnsupportedRequest(UnsupportedRequest) 品牌句柄。Response 与 SessionEvent/PromptResult 的既有结构语义保留；可 Clone，Debug 全脱敏。Response::RemoteError 仅投影 code，message/data 完整在 raw。P1 ObservationRead::observe、P2 ToolProjectionRead::apply 接 &Received，begin 保存 Strategy scope，拒外连接的真实 Received，不能伪造品牌。

## 6. 写轴、响应轴与期限表

```rust
pub enum RequestKind { Initialize(Version), NewSession, Prompt(SessionId),
 Resume(SessionId), Replay(SessionId), ListSessions, Close(SessionId) }
pub enum OutboundKind { Request(RequestKind), Cancel(SessionId),
 PermissionReply{session:SessionId,id:RpcId}, UnsupportedReply(RpcId) }
pub enum WriteProgress { Reserved, Queued, Writing{accepted:usize,total:usize},
 Flushing{total:usize}, Finished(WriteReport) }
pub enum WriteReport { Zero{reason:ZeroReason}, Written,
 Unknown{accepted:usize,cause:WriteFailure} }
pub enum ZeroReason { Cancelled, Deadline, FenceRejected, ConnectionClosed, Abandoned }
pub enum WriteFailure { Io, Deadline, Cancelled, FenceRejected, InvalidFence, ConnectionLost }
pub struct IoShutdownReport {
 pub joined:bool, pub outstanding:Vec<WriteProbe>,
 pub unresolved_requests:Vec<RpcId>, pub error:Option<AcpError>,
}
```

WriteProgress/WriteReport/ZeroReason/WriteFailure/ResponseDeadline 为 Clone+Copy+Debug，RequestKind/OutboundKind Clone+脱敏 Debug。传输结果和 RPC 结果独立：Written 仅完成实际 full-frame+flush；合法 response 可以先于 flush/WriteReport/SQL补记，立即 Received，不能等 Written；后来 flush Err 不撤销它，也不能第二次消费。已 Written 不因后来响应超时改成 Unknown，响应异常仍使连接关闭并列 unresolved。

ResponseDeadline::At 是从成功 reserve 即生效的绝对单调期限，Unbounded 无响应计时；cancel/reply 没有 client response deadline。真实 owner 的 reader 事件循环同时等待 R、状态通知、最近期限 timer，writer 每次实际 poll 前也核期限；期限推进不依赖 EventReceiver.next、Ticket.wait 或任何外部 consumer。合法相关 cell 状态入口在返回 live 状态前处理已到期限；enqueue 的来源/Frame 参数检查仍按第3节优先，不借错误 Frame 操作另一 cell。已成功 reserve 的 initialize 即使零超时仍封该次尝试；reserve 当场已过期则不占位、不封尝试。编码不更改期限。

| 到 ResponseDeadline::At 时的实际状态 | 固定结果 |
| --- | --- |
| Reserved（未 enqueue/encode 中）、Queued、Writing accepted=0 且仅 Pending | Zero(Deadline)，移除 client pending/setup 位，取消后绝无首写；非 initialize 连接仍可处理其他项。保留中的 Reservation/Ticket/Probe 看到同首个终态，Drop 不覆盖 |
| 0<accepted<total，或已有不能证零的 poll Err | Unknown{accepted,cause:Deadline}（若已有更早失败保原原因），同步 Stop/poison；不续残帧/不穿插 control/不重发 |
| FrameAccepted、flush 尚未成功、response 未消费 | Unknown(Deadline) 并 Stop；保 body-free 关联供已取得且合资格的旧 response 排空，无新 write |
| Written、response 未消费 | WriteReport 保 Written；响应轴 Deadline、Stop，unresolved 列原 ID；不能称回滚/零副作用 |
| 合法 response 已消费，flush 仍 Pending | response deadline 已解除；仍须每 poll fence 和独立 write_deadline。后续 write 超时/错误可 Unknown，但原 Received 不撤销 |
| 合法 response 已消费、Written | 两轴稳定，后续过该 At 不做任何事 |

**cut 与期限竞争：** 只有 receipt Instant 严格早于 At 的完整 response 候选能满足该期限（恰等于 At 按已到）。read 成功时的 cut 注册为 reader 正在持有的有限完整帧集合；在锁外 parse 尚未完成前，timer 不得因同一记录“尚未 route”先删掉它。若该 pre-At cut 包含该 FrameAccepted ID+generation，先将它标 ExpiryAwaitingCapturedFrames，暂停该 cell 的新 writer poll，通知 reader 处理已经取得的这些帧，不读新字节；按原字节序 parse/route 完后：有合格且有效匹配 response 则消费并解除期限，否则按上表完成到期/Stop。没有合资格 held cut（Reserved/partial 等）不延迟到期。该内部状态不是 native ACK、不是新 deadline 或容量；它只保护已到字节，不等待未来 response。receipt 不早于 At 的候选不能满足期限，保 rejected raw，终止原因仍 Deadline。正常帧 parse 失败按真实 InvalidMessage 关闭，不制造回应。reader 逐批先 route 完再 poll 下一批，因此 held cut 集合有限且没有重排。

write_deadline 从 enqueue 给入，仅约束该帧写+flush，Written 后解除，不继续卡长 prompt。到期时零已知→Zero(Deadline)，部分/full-before-flush→Unknown(Deadline)+Stop；已有合法 response 不撤销。若 write_deadline 与 response deadline 同一边界一起到达，先处理真实已消费响应，再处理写进度；写 report 原因均 Deadline，terminal error Deadline，无互相重做。其余并发取消/失败以同 ProtocolState 短锁第一个已生效终态为准。initialize/setup/list/插入按用途显式给响应期限；v1 长模型 prompt 允许 Unbounded，不加统一两秒或新固定更大等待。

## 7. 控制优先、唯一 reply 与 ClosingDrain

reader 独立持续接收权限/控制，即使 writer Pending/flush 背压或应用模拟 SQL 等待。Received 交付队列无固定正文/条数预算，不必经 ViewWindows；实际 Allocation/IO 失败如实表达，不承诺物理无限。唯一 EventReceiver 属应用协议 owner；普通页面订阅不是它。next 借用 Future Drop 不丢队列。EventReceiver 本体 Drop 代表 owner 放弃消费，同步 Stop；不等普通浏览器等待者断开。

同一 writer 下一个未开始 frame 优先 permission/unsupported/cancel/close。普通当前 cell 即使已 poll，但始终 Pending0 且无 IO Err，可停在 offset0 让控制先行，保同 cell/Ticket/body/attempt；恢复仍每 poll fence。若期间普通项取消则绝不恢复；control poison 使原普通已知零项结束。已有部分接受绝不切帧，控制只能等完成或异常关闭。server 64 和 client 64 各自独立；一个 reply 写任何字节或不明即 Spent，成功也只一次。零 Reservation/Ticket 放弃或确认取消可恢复同请求 Waiting，seen 不删。选择只从真实原 raw 广告恰一匹配 OptionId；跨连接/未品牌/重同 ID 选项拒绝，原请求不被错误消耗。cancel notification 写成不自动清全部权限或 Run。

**统一 Stop 与排空顺序（适用于显式 shutdown、owner Drop、写/读异常、poison）：**

1. 同 ProtocolState 短锁同步从 live → ClosingDrain，封 reserve/enqueue、所有后续真实 write/flush poll 和所有新 read poll。writer 若正在真实非阻塞 poll，该 poll 与 Stop 由同锁确定先后；Stop 不等待 App/SQL。保原 FrameAccepted/RPC/window 的 body-free generation 关联及已经取得的 cut，不先清表。未入/已入零项得到相应 Zero；已有 accepted 或 IO 不明按实际 Unknown；已 Written 保原值。
2. 唤醒 writer 结束当前命令、释放 frame 缓冲并退出，不主动 flush。唤醒 reader，**不 abort 它**；它按原字节序拆出并完成 Stop 前已取得的所有完整 LF 帧的锁外 parse/短锁 route。ClosingDrain 禁新 server pending/reply grant/write；但仍交付取得帧的完整原数据。此时 Permission/Unsupported 也可有真实来源品牌（来源数据不是 grant），reserve_* 因 Closed 拒绝，不能建立 Waiting/ReplyReserved。已存在 grant 也不能再发送。
3. response 只对 cut 合格、原 FrameAccepted、同 scope/typed ID+generation、尚未消费的旧记录消费一次，并服从上节 pre-At 期限规则；Reserved/半帧伪 response 仍拒绝。已取得 update 保其 receipt window origin；排空不补读无 LF prefix、不复活配置授权，不猜原生归因。排空中后续路由错误保原 rejected raw，不覆盖第一个 terminal 原因，也不截掉后面完整帧；每帧独立按原关联/形状核验。
4. reader 完成已取得集合、两 IO task 实际退出后，由同 Core 一次把终止标记放事件尾。EventReceiver.next 先交所有 Received，再若有首个 terminal error 返回 Err 一次，随后永远 Ok(None)；主动健康 shutdown 无 error 则队列后 Ok(None)。没有读任务被 abort 后仍宣称保帧的路径。
5. IoOwner.shutdown 借用 Core handle 槽串行完成真实 join，再删除槽，joined:true 还要求无未来 IO poll；不以 Vec 空/发出 abort/期限到达假 joined。等待 Future Drop 不 take/detach；再次或并发 shutdown 收敛同 Core。deadline 到达只返回 joined:false + 未完成 probes/必要 unresolved/error，保原句柄和 reader 排空责任，不发布提前 terminal。后续 shutdown 可继续。IoOwner Drop 只同步 Stop，不谎报 join，也不 abort 正持帧 reader；使用已捕获 runtime Handle 的监督路径继续持 Core 至 IO 退出，应用的正常关闭责任仍是显式 shutdown。

没有关闭时收到更多新数据的义务。自定义 R/W 必须遵守 AsyncRead/Write 非阻塞 poll；CPU 卡死/任务 panic/实际分配失败不能被宣称为已保全部帧或已 join，错误/未收敛如实报告。不能为命中短缝在生产加 yield/hook；真实 read 到达/背压/flush 错误屏障、默认并发和实际锁序静态证明分别记录覆盖，不把“写入 peer buffer”冒充本 reader 已 poll_read 取得。

## 8. 实际交付路径与检查边界

正向公开路径：spawn → reserve_preferred_initialize → encode → enqueue(应用私有 fence) → next 得 Initialized Strategy → 完整 SessionSetup → Strategy.new_session/resume → 同样 reserve/encode/enqueue → prompt → next 的 response/update/permission → 原请求 reserve_permission_response/encode/enqueue → shutdown 实际 join。同工厂接 OwnedProcess.take_io 的 stdout/stdin，stderr 与进程/Job 由外 owner drain/回收；本层不代替它。测试既有可控 AsyncRead/Write，也必须有自有协议 child 的真实 unbuffered 管道路径，不能以 traits、duplex 或 CAS 代交。

最小写范围：ACP model/strategy/transport 与必要 reader/writer/state 子模块、完整 setup/P1/P2 raw 适配及开发测试/re-export；不增依赖、不改 raw_json 基础合同/锁文件/管理 strict_json8/JWS/WSS/pairing/原 schema。旧 P0/P1/P2 机械 API 迁移和新 spec 冲突预期由独立 tester 另存，不由 coder 删除旧 oracle 或回填旧通过报告。全部旧长键/视图预算剩余差异见限制表，不签完整无预算共享视图。

必要同源验收：双版原始 wire/未知大数/深树/32MiB+、完整配置和敏感 Debug；64/65 与累计 8193+；Reservation 各错误和 Drop 释放、错误 Frame 无他槽副作用；每 poll/flush fence、Pending0 控制优先、partial poison、早 response 后 flushErr；实际读 receipt 先于 resume/first-write 的反例；ClosingDrain 多 LF 帧及响应先 terminal、期限所有状态和未 next 仍到期；无 ambient runtime Drop、关闭等待方取消/并发及真实 join；自有协议 child。静态/真实可达屏障与未执行极限分别记录。下阶段实际 SQL+R3+IO、持久审批、原生终态/插入关联、真实 Harness/资源/Node/WSS 分列 gate，本合同不预先签其通过。

AcpError 保原枚举并增加 RuntimeUnavailable/Allocation/Cancelled/InvalidFence/InvalidFrame/ConfigurationIncomplete/ConfigurationVersionMismatch/UnencodableConfiguration，均 Copy 静态类别；SetupError 到上述配置错误按对应类别映射，不附原文。不存在 public bool 工厂、任意 scope/Received/cell grant 构造或任意 bytes Frame 工厂。
