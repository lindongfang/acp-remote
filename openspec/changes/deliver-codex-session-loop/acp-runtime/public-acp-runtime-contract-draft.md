# WP3 公共 ACP raw/API、完整配置与真实 IO 合同草稿

2026-10-31。仅工程草稿，不放行编码。inputs.json固定当前spec/design、raw基础层9f2b/127源、SDK2.1.0 stable双版schema及配置盘点。实施次序：本阶段交真实公共ACP客户端和IO owner→下一阶段真实SQLite派发账本+actual R3 owner接本IO→Node/资源/WSS。SQL v3-r3集成缺陷优先，本草稿未改任何源或冻结。

## 1. RawMessage与全部Value入口替换

沿用已验raw_json::{RawDocument,OwnedRawValue,RawValueRef,RawObject,RawArray,RawKind}，不另造解析器，不改其合同。RawMessage私有持RawDocument；`parse(Vec<u8>)->Result<Self,AcpError>`验证JSON object，公开供纯数据解析，不能mint连接来源品牌；`bytes()->&[u8]`保parse参数的完整原字节；真实reader仅先去独立framing LF和紧邻其前的可选CR，parse自身不再修改JSON空白，`fields()->Result<RawObject<'_>,AcpError>`，`field(&str)->Option<RawValueRef<'_>>`，Clone共享。无Map<String,Value>、隐式lazy DOM、Deserialize/Serialize或Value兼容overload。原始JSON空白/escape/数字原词法另保；所有Debug/错误只类型/长度/静态原因。

RpcId保Integer(i64)/String(Arc<str>)及数字/字符串类型身份。`RpcId::parse(RawValueRef<'_>)->Result<Self,AcpError>`只精确整数词法、原安全整数范围；浮点/指数不能借raw.integer归一化成整数；`-0`在当前锁定serde_json路径是float而被拒，新RpcId解析继续拒绝其原词法，尽管raw.integer可对非RPC值返回0。`write_json(&mut Vec<u8>)->Result<(),AcpError>`替代json()->Value，仅编码小标量。SessionId/ToolCallId/MessageId/OptionId的parse同样改RawValueRef，as_str/getter保持；具体旧长度保护的分类见limits表，未据相同数值擅自删除。

Request私有持scope、RequestKind和OwnedRawValue object参数；`kind()->&RequestKind`、`params()->RawValueRef<'_>`、`setup_stamp()->Option<&ConfigSourceStamp>`。只有Strategy构造合法标准请求，外部不能用任意method/params+bool伪造scope。Request参数中的prompt、_meta、rawInput/rawOutput、未知扩展始终是OwnedRawValue或其浅成员，不回Value。Known控制字段可重新编码，未知原片段只compact字符串外JSON空白，不改变数字/escape/字段次序；最终只加一个LF，不能把raw literal LF直接嵌入NDJSON。

RawMessage.parse保持语法层与路由层分离；jsonrpc/id/method/result/error形状在真实reader路由核。有效JSON但关联/控制语义错误时`Connection::take_rejected_message()->Option<RawMessage>`仍保完整原文；非法JSON本身不伪装成RawMessage、不将库错误原文写日志。

| Strategy公开方法 | 固定参数与返回 |
| --- | --- |
| new_session | `(&self,setup:&SessionSetup)->Result<Request,AcpError>` |
| resume / replay | `(&self,session:SessionId,setup:&SessionSetup)->Result<Request,AcpError>`；v1 replay=load、v2 replay=resume+replayFrom:start |
| prompt | `(&self,session:SessionId,prompt:OwnedRawValue,extra:ExtensionFields)->Result<Request,AcpError>`；prompt必须array，逐浅元素验证已知必需字段/实际capability，未知内容保raw；空array是否新增限制只按固定schema/spec审，不沿旧.filter(!empty)默加规则 |
| list_sessions | `(&self,cwd:Option<&str>,cursor:Option<&str>,extra:ExtensionFields)->Result<Request,AcpError>` |
| close | `(&self,session:SessionId,extra:ExtensionFields)->Result<Request,AcpError>` |
| prompt_result / update / session_listing | `(&self,value:RawValueRef<'_>)->Result<PromptResult/SessionEvent/SessionListing,AcpError>` |
| permission | `(&self,id:RpcId,raw:RawMessage,params:RawValueRef<'_>)->Result<PermissionRequest,AcpError>`；纯解析结果无连接品牌，不能直接reply |
| tool_name_patch | `(&self,update:RawValueRef<'_>)->Result<ToolNamePatch,AcpError>`；Unchanged/Clear/Set(Arc<str>)，保v1 null不变/v2清空 |
| tool_metadata_patch | `(&self,current:&RawMetadata,update:RawValueRef<'_>)->Result<RawMetadata,AcpError>`；v1仍Unsupported，v2缺失不变/null清空/object顶层key替换/null删除，不递归merge |

RawMetadata固定为Null或Object(OrderedRawObject)。OrderedRawObject以有序(Arc<str>,OwnedRawValue)成员保存，`from_raw(OwnedRawValue)->Result`只接object、`get/iter`只借raw、`to_raw()->Result<OwnedRawValue,AcpError>`浅编码，不能递归clone未知子树。更新保未改字段原位，新键按patch顺序追加；原片段原词法不变，不把“重新生成的浅object空白”冒称原始字节。

Incoming的Response/Update/Permission/UnknownNotification各语义及raw保留；Response::RemoteError仍仅投影code，raw保message/data。PromptResult v1 stopReason终态、v2 messageId插入分别表达。SessionEvent的状态/工具/消息/Other仍不生成Run归因，v1三类chunk可省略或null messageId，v2实际字段规则不混用。所有含untrusted字符串的Debug脱敏。

新增私有构造`Received{scope,sequence,incoming}`，公开scope()/sequence()/incoming()/raw()只读，Clone共享raw。只能真实reader mint；Incoming本身是数据，不是来源grant。P1 ObservationRead::observe和P2 ToolProjectionRead::apply改接`&Received`，begin保存Strategy来源scope并校验一致；测试必须经真实连接取得合法Received或使用别连接的真实Received验证拒绝，不提供fake brand工厂。其Raw字段检查/nullable/局部更新/原位置/重复行为保持P0/P1/P2语义；旧Value测试生成fixture可留测试内，产品路径不回DOM。

## 2. 完整SessionSetup与已知/未知来源

这些均是协议数据，**不是授权grant**，不访问本机文件或私有配置。可信应用负责配置来源、版本、cwd/根的可用性及授权；真正writer前由外部私有fence核配置revision/目标/连接仍有效。

```rust
pub enum Known<T> { Unknown, Known(T) }
pub struct ConfigSourceStamp { pub reference: Arc<str>, pub revision: u64 }
pub struct SetupSnapshot {
  pub wire: Version, pub source: ConfigSourceStamp,
  pub cwd: Known<Arc<str>>,
  pub mcp_servers: Known<Vec<McpServer>>,
  pub additional_directories: Known<Vec<Arc<str>>>,
  pub extra: ExtensionFields,
}
pub struct SessionSetup { /* private complete snapshot */ }
impl SessionSetup {
  pub fn try_from_snapshot(value: SetupSnapshot) -> Result<Self,SetupError>;
  pub fn wire(&self)->Version;
  pub fn source(&self)->&ConfigSourceStamp;
  pub fn cwd(&self)->&str;
  pub fn mcp_servers(&self)->&[McpServer];
  pub fn additional_directories(&self)->&[Arc<str>];
}
```

不实现Default；任一必需配置Unknown→SetupError::Incomplete(Cwd|Mcp|AdditionalDirectories)，不把Unknown变[]/省略，也不因此禁用list/prompt/权限等不需要该未知字段的路径。Known(empty)是完整明确空Client列表，不表示Agent内置工具也空。source stamp公开可构造仍只是数据；应用不因它自报而授权。

ExtensionFields是私有object OwnedRawValue包装，`empty()`、`from_raw(OwnedRawValue)->Result`、`as_raw()`；编码时拒绝与该请求受控字段冲突，不覆盖sessionId/cwd/mcpServers/additionalDirectories/replayFrom。_meta object/null和未知字段保原片段，不泄漏Debug。

McpServer为私有字段的版本绑定数据，公开`from_raw(wire,raw)->Result`及`stdio(wire,name,command,args:Vec<Arc<str>>,env:Vec<NamedValue>,extra)`、`http(wire,name,url,headers:Vec<NamedValue>,extra)`、`sse_v1(name,url,headers,extra)`构造器（均Result）。`wire()/transport()/as_raw()`只读。NamedValue持name/value/extra，值和header/env全部敏感Debug脱敏；Vec保顺序及重复条目，不转map。known配置编码按以下表，未知成员完整保留；同版本raw不重建缩水数据，不自动把v1 raw转换v2。跨版本无法无损转换→仅此setup路径UnencodableConfiguration，应用给完整新快照，不能删除字段“兼容”。

| 规则 | 固定v1 | 固定v2 |
| --- | --- | --- |
| new/load/resume | new/load均cwd+mcpServers必填；resume cwd必填；全部使用完整setup。load能力须loadSession=true；resume须sessionCapabilities.resume对象 | 无load；new/resume cwd必填，完整Client MCP仍显式编码；session:{}广告baseline session方法 |
| replay | session/load；不能用仅resume冒充历史 | session/resume + replayFrom:{type:start}；无重放恢复省略replayFrom，不用cursor猜支持 |
| roots | ordered additionalDirectories；非空须sessionCapabilities.additionalDirectories对象广告 | 非空须capabilities.session.additionalDirectories对象广告；空明确无根可省略兼容，不代表保持旧配置 |
| stdio | 无必需type discriminator；name/command/args/env完整必填；所有agent必备 | type:stdio，name/command必填；本完整快照显式args/env；须session.mcp.stdio对象广告，session:{}不含此保证 |
| http | type:http、name/url/headers必填；mcpCapabilities.http=true | type:http、name/url必填；完整headers显式传；须session.mcp.http对象广告 |
| sse/other | sse须type:sse及sse=true；未知transport无该版标准可编码依据时明确限此配置路径 | 无独立sse广告位；Other按该版schema原文保留/转发，不能把sse改http或宣称Agent支持；Known完整同版本配置允许标准opaque forwarding，Agent须拒绝不支持transport，不能用请求探测自动换空配置 |

v2 ContentBlock Other同其schema的标准保真/转发语义，保整个raw；已知image/audio/resource仍核对应广告，不能以Other包装绕过已知type检查。v1未知类型仅按该版实际schema/当前受支持路径处理，不静默降文本。此处是主动传入完整配置/内容的实际请求，不是无用户动作副作用能力探针。

SessionListing新增每entry的raw()和ReportedDirectories::{Omitted,Empty,Known(Vec),Null,Uninterpreted(OwnedRawValue)}；v2描述提null而schema type=array的差异原样标Null，不暗当已授权KnownEmpty，也不丢raw。缺省/空的“报告无额外根”按固定schema说明呈现；MCP未报告不能补KnownEmpty。SourceStamp来源和整套授权由应用另证，不从列表自动mint完整配置。

## 3. 一个可操作的真实公共IO正向路径

移除旧Connection<R,W>外包Mutex路线。保名Connection但改为克隆操作handle，独立EventReceiver和IoOwner，内部真正拆Reader<R>/单Writer<W>/ProtocolState，不依赖SQL/application/未来NativeDispatch类型。

```rust
impl Connection {
 pub fn spawn<R,W>(reader:R,writer:W)
   -> Result<(Connection,EventReceiver,IoOwner),AcpError>
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
 pub fn reserve_permission_response(&self,request:&PermissionRequest,choice:PermissionSelection)
   ->Result<Reservation,AcpError>;
 pub fn reserve_unsupported(&self,request:&UnsupportedRequest)
   ->Result<Reservation,AcpError>;
 pub fn enqueue(&self,reservation:Reservation,frame:EncodedFrame,
   fence:Arc<dyn WriteFence>,write_deadline:Instant)->Result<WriteTicket,AcpError>;
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
}
impl EncodedFrame { pub fn bytes(&self)->&[u8]; }
impl WriteTicket {
 pub fn probe(&self)->WriteProbe;
 pub fn progress(&self)->WriteProgress;
 pub fn request_cancel(&self);
 pub async fn wait(&self)->WriteReport;
 pub async fn cancel_and_wait(&self)->WriteReport;
}
impl WriteProbe { pub fn scope(&self)->ConnectionScope; pub fn id(&self)->Option<&RpcId>;
 pub fn progress(&self)->WriteProgress; }
impl EventReceiver { pub async fn next(&mut self)->Result<Option<Received>,AcpError>; }
impl IoOwner { pub async fn shutdown(&self,deadline:Instant)->IoShutdownReport; }
```

所有future Send。spawn同步捕获Handle::try_current，失败typed RuntimeUnavailable；首次await前存入同Core的两条IO任务及cancel/join责任。不得ambient spawn依赖Drop线程context。IoOwner不Clone，Connection/WriteProbe/ConnectionScope可Clone；scope私有Arc身份、Eq仅同实例，不Serialize/外部构造。IoOwner Drop仅同步停新动作/取消任务，不称join成功；shutdown用专用生命周期锁和Core始终可达的JoinHandle槽，借用等待，取消不take丢句柄，真实join后才移除。deadline耗尽返回joined:false，后续shutdown可继续；不新设2/5/10秒常量。

最小正向：spawn→reserve_initialize→reservation.encode→enqueue(应用私有fence)→receiver.next得到Initialized策略→strategy.new_session(完整setup)→同一路reserve/encode/enqueue→策略prompt→同一路→Received响应/通知。真实实现能接OwnedProcess.take_io的stdout/stdin，stderr由进程owner独立drain；本层不创建/杀进程或伪造资源Ready。测试可用可观测AsyncRead/Write及自有协议child，不能只实现trait/CAS结束。

Reservation不Clone，私有同scope/cell；encode只做内存浅envelope+raw compact，不I/O、不改first-poll状态，在共享锁外。EncodedFrame私有同cell标记+Vec字节；无任意字节工厂、无Serialize/泄漏Debug。encode多个副本也只有原Reservation可enqueue一次；交叉cell/连接frame拒绝。reserve前核同Strategy scope/能力/初始化相位，已有64未决时按现有Capacity处理，不静默丢字段；本阶段不新增累计次数上限。

保现有initialize客户端info/name/version及空已实现clientCapabilities/capabilities；只广告已有处理，不因setup未知字段造新能力。每连接一次initialize，最高已实现版preferred；合法选v1只用v1字段。初始化失败、已选版业务失败不自动降版重发；已reserve后放弃初始化仍封该次尝试，不重试同连接。

## 4. 最后每个poll的外部私有围栏

协议层定义可由后续App私有对象实现的小接口，不反向依赖其domain/SQL，也不提供public bool许可工厂：

```rust
pub enum WriteStep { Chunk{offset:usize,total:usize}, Flush{total:usize} }
pub enum IoProgress { Accepted(usize), Flushed }
pub trait PollIo {
 fn poll(&mut self,cx:&mut Context<'_>)->Poll<Result<IoProgress,AcpError>>;
}
pub trait WriteFence: Send+Sync {
 fn check_and_poll(&self,identity:&WriteIdentity,step:WriteStep,
   cx:&mut Context<'_>,io:&mut dyn PollIo)->Result<(),FenceRejection>;
}
```

WriteIdentity私有构造，只读scope()/id()/kind()/setup_stamp()；后续App fence捕获自己的私有active/lease/资源/取消/配置revision，核对本identity后短锁内调用真实PollIo一次，无await。Rejected封闭为Stopped/Cancelled/StaleTarget/RevisionChanged/NotReady/RequiredStateUnavailable。协议提供的PollIo代理只能真正调用当前独占writer一次，自己记录实际返回值/offset；不能凭fence自报字节数生成Written。Ok必须恰一次poll，Err必须零次；不合约返回InvalidFence，保真实已观察字节范围，不伪零。

每次poll_write和poll_flush都走这个接口；Pending之后下一次仍重核，offset必须与实际累计接受字节相符。真实非阻塞poll及状态更新在ProtocolState短锁中，且外部App短锁包在外面；reader只持ProtocolState短锁，释放后才发事件，不反向调用App。禁止把SQL/大编码/解析/审批等待放在这些锁里。W是受信独占字节流，不得有独立后台写或Drop自主首写；生产绑定使用实际unbuffered管道。注入buffered/mock W时证明范围仅为该AsyncWrite层，不自动等于原生端已接收。

## 5. 写与响应的准确二维状态

WriteProgress固定Reserved/Queued/Writing{accepted,total}/Flushing{total}/Finished(WriteReport)。WriteReport仅传输轴：Zero{reason}/Written/Unknown{accepted,cause}；reason/cause封闭静态枚举，无正文。RPC结果由Received独立到达，Written不是ACK/插入/完成。IoShutdownReport为joined:bool、outstanding:Vec<WriteProbe>、error:Option<AcpError>；joined要求两IO task真实join、后续无poll。

| 状态/相邻事实 | 响应可接受性 | 取消/超时与连接 |
| --- | --- | --- |
| Reserved/编码中/Queued从未poll | 不能接受该ID的response；保raw并UnmatchedResponse/poison，不消费成合法结果 | Zero；撤销后不能留下首写命令。非initialize可继续其他请求 |
| 实际poll均Pending、accepted=0且无IO错误 | 尚无完整帧，不接受response | 取消确认后Zero，移除该pending；不统一DeliveryUnknown |
| 部分字节0<accepted<total | 尚无完整LF帧，不接受response | Unknown；整连接poison并停两IO，不继续剩余帧/不穿插控制/不自动重发 |
| 完整帧已被W接受、flush或Written记录尚未完成 | 可按原scope/typed ID解析一次合法response，原事实立即保留/发Received | 后来flush错/取消为传输Unknown，但不撤销已证response、也不重复消费 |
| flush成功 | Written；RPC可仍pending或早已消费 | 长模型/插入等待不套首次写deadline；无自动补发 |
| 任一poll Err但不能证明零 | 记录真实不确定范围 | Unknown并poison；不能因accepted计数0就断言对端零副作用 |

实际poll和accepted/FrameAccepted标记在同ProtocolState短锁：reader不能在LF已被writer接受但本地标记尚未更新的缝隙丢合法response。仅预留或半帧不可借猜中ID吃伪响应。解析/大raw索引在reader锁外，路由提交前重核scope/phase/cell状态，避免解析期间取消导致错收。已处理Received先于后来的终止错误交给消费者，不改成未发生。

响应等待与写deadline分离；v1长prompt使用Unbounded或调用方真实执行策略，不自动2秒断长运行。初始化/setup/list/插入等按用途由调用方显式给response deadline，既有deadline过期协议异常仍不授权重发。response完成与write完成是两条轴，容量释放按真实RPC完成/零撤销，不要求SQL先补Written。

WriteTicket唯一持写意图责任，Drop在同state边界同步request_cancel；借用wait future被drop仅停等待，不丢ticket。App须把ticket保在node-owned dispatch任务，浏览器等待方drop不drop它。cancel_and_wait先同步标撤销再等writer实际确认；取消等待本身drop不丢Core命令/JoinHandle。Core在每poll重新核取消，确认后不能晚首写。任一已接受字节或不能核验的错误不能假零。

## 6. 真实reader、控制和单次reply

reader单独持续读/RawMessage parse/路由；SQL/外部App慢、writer背压/flush等待、EventReceiver.next调用方暂不poll，都不阻止它接收权限/控制。事件用无固定内容/条数上限的owned队列保存Received，不经ViewWindows预算；实际分配失败不承诺全部可优雅恢复。唯一EventReceiver属于协议应用owner，不是某浏览器订阅；丢弃它表示应用放弃消费，封新写并按owned shutdown处理，不把普通页面展示失败当协议owner drop。

同一writer在尚未开始的命令中优先控制（permission response、标准unsupported reply、cancel及close），普通prompt/setup/list不冒充控制；不在半帧中插入另一帧。当前partial等待超时/取消则poison，不能虚报中断已发；reader仍独立至poison/明确关闭，不等长prompt结果才处理审批。关闭直接撤销/丢弃owned IO，不能用自动flush残留未发frame绕过fence；底层IO owner不得有自主延迟首写。

保持两方向各64同时pending及已见server ID终身防重复，8193及更后合法请求继续；不清server_seen复活旧ID，不新增累计阈值。RPC字符串/整数同形不同身份，原始字节digest+连接scope来源核验不退化。UnsupportedRequest改为私有品牌handle，Incoming给该handle及原raw；不能仅拿另连接同形id响应。PermissionRequest只读getter及Clone，Strategy.permission产物未品牌，真实reader才品牌；reply工厂从原raw核真实session/选项，任何已知字段伪造不改原广告。

PermissionSelection为Selected(OptionId)/Cancelled。选择必须在原广告中恰一匹配；未知主题/选项提示不作为整单禁选理由，按当前permission spec可人工原选项选择，不自动允许。真实决定先保存/当前actor授权由App外部fence/下一审批owner负责，此协议数据不是批准grant。

server request Waiting→ReplyReserved→实际发送/Spent；零字节撤销可恢复同真实请求Waiting，已接收字节或IO不明则Spent，绝不第二reply。server_seen仍保该ID，重复request/已答旧ID按原错误/poison语义拒绝。cancel notification写成功不自动使所有权限失效，也不清Run；范围由版本/真实请求生命周期后续owner判，旧取消不能影响新输入。

同session在途resume/replay互斥从reserve占位；零撤销释放。DuringResume/Replay观察窗口仅在完整请求帧接受后打开，响应前同源同session的current-state可满足v2既有snapshot条件；只有Reserved/Queued的自发update仍Live，不能伪快照。v2缺snapshot响应仍MissingSnapshot；snapshot/aggregate idle不是Run完成，未知自发活动不猜最近输入，早终态后迟插入不要求第二idle（本层只给事实，下一实际reducer另gate）。P0/P1/P2 raw顺序/来源/null/增量语义不因API换掉。

## 7. 额度分类、写范围与独立门槛

具体逐处表见limits-and-migration-draft.md。Q001明确删除frame/正文结构/输出预算，不按数字相同把nativeID/cwd/cursor/list控制自动删掉；未决关联/参数限制须reviewer按spec归类后才能作为编码输入，不写已获用户逐项确认。

拟写model/strategy/transport及reader/writer/state子模块、SessionSetup、P1/P2 raw适配/开发测试、必要mod/ACP文档；raw_json基础API/锁文件不改，管理strict_json八层及WSS/JWS/pairing规则不扩散。旧ViewWindows/history缓存预算列为剩余差异，不接成新公共客户端必经门槛，不借本阶段签完整无限输出/Node能力。

本阶段必须交真实可用公共客户端与可取消可join的IO owner，不能只trait/CAS。独立tester维护旧P0/P1/P2机械适配和新spec冲突候选，旧oracle/报告原字节保留；coder不删断言。至少双版实际握手/完整setup与未知片段往返、writer Pending期间权限到达、半帧/零发送取消、早response先于flush、预留伪response拒绝、64/65及8193+、跨scope reply、replay snapshot/live时窗、所有poll/flush fence、Drop无runtime线程/并发取消关闭/真实join。再用自有协议child+真实管道跑操作路径；真实Harness/Job完整树、资源收敛、SQL派发、WSS/Web明确分列，不能外推L6。独立定义/预审同版通过后才编码。

## 8. 具体数据类型与可执行fence示例

OutboundKind固定Request(RequestKind)/Cancel(SessionId)/PermissionReply{session:SessionId,id:RpcId}/UnsupportedReply(RpcId)；RequestKind沿Initialize(Version)/NewSession/Prompt(SessionId)/Resume(SessionId)/Replay(SessionId)/ListSessions/Close(SessionId)。WriteIdentity和WriteProbe仅getter，不提供构造器/反序列化；回复方向ID在OutboundKind区分，不与client-generated pending表混合。PermissionSelection为Selected(OptionId)/Cancelled。

WriteReport为Copy的枚举Zero{reason:ZeroReason}/Written/Unknown{accepted:usize,cause:WriteFailure}；ZeroReason=Cancelled/Deadline/FenceRejected/ConnectionClosed/Abandoned，WriteFailure=Io/Deadline/Cancelled/FenceRejected/InvalidFence/ConnectionLost。AcpError保现有枚举语义，新增RuntimeUnavailable/Allocation/Cancelled/InvalidFence/ConfigurationIncomplete/ConfigurationVersionMismatch/UnencodableConfiguration静态错误，不含外部原文。

IO任务本身持current-write RAII guard：若shutdown/abort丢future，按已记录实际poll/accepted/flush状态完成对应cell并唤醒ticket；绝不能只abort JoinHandle让wait永不终态。初始化/已开始写的未决RPC不因关闭变“从未发生”；尚未poll的queued/reserved则Zero。IoShutdownReport补含unresolved_requests:Vec<RpcId>，仅必要关联元数据；两IO task实际join可joined:true但仍有error/原生结果未知，不能把joined当原生成功。shutdown只关闭自己持有的IO，进程/Job资源仍归外层真实owner。

合法fence的主体形式是：取得应用自己的短状态锁→验证identity/取消/配置版本/资源→`let actual = io.poll(cx)`→更新自己的单次poll记录→返回Ok。即使actual是Pending或Err也返回Ok表示完成一次合法围栏调用；真实IO outcome由协议代理记录。拒绝必须在调用io之前返回Err。这是一段真实IO的同步临界区，不是异步bool授权检查；不能将io保存/搬到别的task稍后执行。

reader大raw parse/known字段投影及digest在共享state锁外，route提交前短锁重核记录。EventReceiver只是交付队列，reader不会await它的容量或SQL；每次Incoming只能由该真实Received源发出一次。clone RawMessage/Received延长对应raw文档内存存活期，WriteProbe在terminal后只保必要元数据，必须释放EncodedFrame/prompt缓冲；不因readonly probe常驻保住正文或完整session setup。无需新增内容阈值。

## 9. 补齐具体构造与关闭期路由

SetupField=Cwd/Mcp/AdditionalDirectories；SetupError=Incomplete(SetupField)/VersionMismatch/InvalidShape/ReservedField/UnsupportedEncoding/Allocation，均静态脱敏Error。ExtensionFields::from_raw(OwnedRawValue)->Result<Self,SetupError>、as_raw()->RawValueRef；SessionSetup::extra()->&ExtensionFields。McpServer的from_raw/stdio/http/sse_v1均返回Result<Self,SetupError>，name/command/url为Arc<str>，stdio args为Vec<Arc<str>>、env/headers为Vec<NamedValue>，extra为ExtensionFields。McpTransport=Stdio/Http/Sse/Other(Arc<str>)，wire()/transport()/as_raw()只读；NamedValue::new(name:Arc<str>,value:Arc<str>,extra:ExtensionFields)->Result<Self,SetupError>，name()/value()/extra()只读且Debug不输出这些值。

RawMessage.fields返回Result<RawObject<'_>,AcpError>，虽然parse已保证根object，仍不为私有不变量使用unwrap/expect，也不新增raw_json强制object工厂。所有调用用?取借用；不存在旧Map兼容getter。field单键读取缺失为None，不触发DOM。Raw层错误转换为静态AcpError，源码片段不附错误。

关闭时保留body-free原pending关联，不把“writer已poison”当“旧输入从未发生”。reader已读到并解析的完整response，即使其提交state锁晚于writer的flush错误，也可在ClosingDrain中只消费原FrameAccepted、同scope、尚未响应的那条记录一次；任何仅Reserved/半帧ID仍拒绝。ClosingDrain不再建立新的server request grant、不允许任何新write，不等待新数据，只处理已取得的完整frame，然后结束reader。未完整/未取得的帧保实际未知，不承诺从已关闭连接补齐。

事件队列的终止错误在两个IO任务完成并完成上述已取frame路由后放在尾部，已取得Received不能被早到的write错误截掉。用户仍先看到原事实再看到终止错误；EventReceiver.next借用future被drop不丢队列。IoOwner joined是资源任务终止，不改变已发/插入/未知；原始Received也不因后来sourceClosed变成可交付旧审批的授权。

优先级只选下一个未开始frame。control可越过普通Queued；普通frame即使已poll但全Pending0、没有任何接受字节/IO错误，也可由同一writer把同一cell停在offset0让control先写，不取消输入、不生成第二attempt、不丢ticket/正文。该cell的Writing{accepted:0}仍是进行中，不能接受其response；之后每poll照常再过外部fence，取消确认后绝不resume。控制成功或零终态后可恢复原cell，若control导致poison则原cell按已知零发送结束。普通frame若已部分接受不能切帧，控制只能等待其明确完成或异常关闭，不能插入字节/虚报已发。此规则不允许交错NDJSON字节，也不把未知中断伪记成功。

本阶段不增加schema或任何NativeDispatch关联类型；后续只需把其私有armed lease封在WriteFence中，把真实Reservation.id/scope保存必要账本，再enqueue同EncodedFrame即可。public IO内部不执行SQL；Response先于WriteReport/SQL补记时各自交给App，App必须按相同真实RPC绑定分别保存，不能要求reader等待SQL。

编码补充：ExtensionFields验证用decoded key检查受控字段冲突，输出时compact其整个object后按受检外层花括号边界拼接成员原片段，保未知键的escape词法/顺序，不逐键重序列化。完整McpServer raw和prompt raw亦整片段嵌入。RawMetadata的无patch情况共享原对象；有patch时结果是新的浅投影对象，外层key转义可重新JSON编码、未知值子树仍保持原片段，原Incoming.raw/patch原文始终另存，不能把该投影结果冒充收到的完整原字节。
