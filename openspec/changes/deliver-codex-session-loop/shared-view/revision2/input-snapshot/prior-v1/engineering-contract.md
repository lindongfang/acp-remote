# 节点共享非持久视图工程候选 v1

2026-11-01。仅设计；本文件与 interfaces.md、assembly-and-delivery.md、verification-reachability.md、inputs-v1.json 构成完整候选。等待独立 tester 定义及 reviewer 编码前检视，当前没有产品编码、SQL 执行、Node/WSS/UI 可用或正式验收结论。

## 1. 输入与范围

当前实际根306项源清单 SHA-256 `fa004a9042c0ade9ba835a3f061f0568707c40d354f2f83b4d472f4464aeed88`；本次重新逐项核0漂移。84份实际输入按原字节复制，清单 `inputs-v1.json` SHA-256 `22834a9d0a679a93c788866906f6ede84aa401233f9fa063e95cfb1540907912`，复制前后0漂移。plan另处于主执行维护，不把未在306内的plan状态宣称已经正式验收。

行为依据为当前 structured-history、native-session-recovery、permission-approvals、acp-platform-constraints 和总体设计中未被Q-001取代的内容。旧view-window/P1/P2预算合同仅解释历史实现。此阶段不改用户规则，不引入历史正文数据库，不删除旧表/正文，不添加依赖、锁文件变化或厂商读取器。真实Harness、独立安全及供应链证据仍分别处理；Web audit按用户拒绝外发保持未执行。

本loop的基础视图包含：当前连接已收内容首开可见、同节点跨浏览器共享、结构与原文保真、已知工具原位更新、来源及限制状态、显式刷新/重开/重连的原生重读协调、可靠重放完成事实、读取撤销与安全交付。history-navigation负责后续完整轮次分页、任意定位及子树导航；本阶段不得以此排除基础读取及完成事实。本文的游标仅流式传输位置，绝不称原生轮次分页。

## 2. 重新核对实际代码

| 实际落点 | 当前事实 | 对新路径的影响 |
| --- | --- | --- |
| adapters/acp/transport.rs | ConnectionScope私有Arc身份、Received字段私有；EventReceiver不可Clone且唯一；next返回真实Received | 不从JSON、SQL、EntityId或bool重造来源；新路径复用同一reader |
| RawMessage/RawDocument | RawMessage只包已验证RawDocument，其Arc共享为常数工作；raw bytes保完整词法、未知数值/键序 | capture只clone该原文引用，不clone整个Incoming或重序列化 |
| 当前transport | 尚无ObservationLease/ack、prepare_observed | 不能把ND候选接口当根已实现 |
| 旧domain/views | 默认16/128MiB、4096/32768项、64身份、引用/快照等累计预算 | 新mandatory视图完全不调用；不改常量冒充完成 |
| P1/P2 | ObservationRead/ToolProjectionRead依赖旧ViewWindows；P2 decode_anchor仍有4096工具键长门槛 | 新adapter可按核实的版本字段规则重写纯转换，不能继承预算或MissingRaw降级 |
| application/history | 正常构建已隔离，旧保存链仅cfg(test) | 不恢复旧永久正文/500ms保存路径 |
| Node/management_ws/Web | 只有既有身份管理装配；尚无本视图源映射和分块读取 | 新工程模块交付不等于节点/UI闭环 |
| NativeSessionKey | 来源配置+原生session；现身份构造器保留4096 session ID规则 | 这是现有session身份边界，不套给message/tool/unknown正文，不另加视图身份总量 |

ND正式冻结输入是revision4/freeze-v4.json `82ca7e5c1861ee228f8e7ad2b8d2fda107a5542d48b9d679e83731615cb1040f`，仍有RESOURCE004工程阻断；复制的revision5是未冻结修订参考。本文依赖其未来单观察pump、真实resource/source绑定和typed RPC操作登记的职责，不依赖其当下可用。最终ND接口gate后必须重基准并重新检视本文新增端口，不能先接假Scope或Trusted=true。

## 3. 分层与所有者

| 所有者 | 唯一责任及生命周期 | 不负责 |
| --- | --- | --- |
| Native SourceSlot（ND阶段） | 唯一Connection/reader/IoOwner、实际Scope、pump和原生进程；按真实source/resource登记路由 | 不渲染、不读客户端、不持正文库 |
| AcpViewCapture（ACP外层） | 从该pump借用真实Received；核原Scope/连续sequence，O(1)共享raw；只持原文handoff端 | 不ack、不消费Lease、不控制SQL围栏、不取消执行 |
| SourceViewIngress（application） | 一个source的仅内存无总预算FIFO、唯一producer/capture，唯一worker detach；已入队frontier和预分配故障状态 | 不持view/client/SQL锁，不parse/render原文 |
| SharedViewOwner（application，每节点一个registry） | 按NativeSessionKey和actual Scope管理视图；各source worker有自有任务/取消/join；只对同target轻量状态串行 | 不接管原生资源生命周期，不成为万能Runtime |
| NativeReadCoordinator（application） | 同NativeSessionKey协调一个在途原生重读；借同SourceSlot完整可信配置和资源；节点拥有Operation/Ticket | 不拥有第二reader，不由浏览器决定源作用域 |
| domain/shared_view | 纯来源状态、epoch/revision、知识/限制和原位锚点规则 | 不导入ACP DTO、SQLite、WebSocket、RawDocument |
| adapter/projection | 在worker将相应协商版原文解成结构引用和已核实patch | 不更改原文、不猜未知语义 |
| ViewReadService/gateway | 每次读取和发送chunk当前鉴权；只交付被授权target与generation的数据 | 不授予跨body/跨chunk永久读取grant |

registry的target键为真实NativeSessionKey，slot另存actual Scope并用Eq核对；ConnectionScope没有Hash要求，不用指针数字/伪epoch替代。ViewEpoch/ReadEpoch/ViewRevision是平台展示标记，Scope是实际协议连接来源，ResourceEpoch是资源归属，互不替代。客户端只获得opaque视图句柄；句柄不构成权限。

同target仍由ND唯一资源归属协调；换启动别名不能多建并发写owner。不同显式来源同短ID分开。相同target的新连接替换旧view scope；旧scope的迟到capture/读结果不能发布到新slot。旧view允许标记stale后暂留，但所有旧cursor实际读取必须失败，由新open重新取得明确旧视图展示的专门句柄；不能静默继续旧流当最新。

## 4. 无预算原文交接与实际分配失败

capture入口只有实际Received（observed模式借lease.received()），无公开raw/seq/scope拼接构造器。source安装时注册capture实际Scope、可信来源及路由绑定；同pump所有Received都按原sequence进入一次capture；原raw、sequence、receipt_order及只读classification共享保留。非session通知不得被随意投给某个target，未知但具有已验证标准session envelope的结构可保留为该target原文；完全无可靠归属者仅source内暂留，不通过会话接口泄给任意target。

接收顺序固定：借真实Received → capture原RawMessage引用（不作正文遍历）→ 原ND小事实apply/gap → ND ack。capture失败只更新**展示故障**，不得进入AppliedObservation、必要SQL gap、ObservedWriteGate或取消执行。若ND自身无法分类控制事实，它沿自己的失败路径处理；不能把展示解析失败升级成它的ObservationFault。

handoff用一个source私有的短临界区无界FIFO：producer只核sequence/try_reserve追加固定大小引用记录；consumer只在同锁内将完整批次容器detach，parse、投影、客户端请求、权限、网络、释放大正文均在锁外。无任何reader/view/client/SQL路径可取得该锁；不得持其它应用锁进入它；禁止在pump await队列容量、worker、通知确认或客户端。真实物理分配/调度不是硬实时保证，try_reserve在实际需要增长时可能搬移metadata，不能虚报严格恒定延迟；验收须证明慢worker、慢reader、blocked projection不持handoff锁或阻控制进展。若独立检视认为该实现不可达上述短交接，须先改合同并复核，不能改用有界内容channel。

FIFO、source保留日志、item索引、原文引用、版本、snapshot、活跃view身份均不设字节/条数/引用/累计身份/TTL总预算，不按16/128MiB、4096/32768、64等驱逐。单次传输chunk和每客户端同时发送chunk的数量上限是网络背压，不能反馈capture或驱逐节点内容。浏览器断开/最后订阅退出也不清节点view。自动资源回收属其它change；当前只有真实source/resource释放、scope换代或节点停止可使对应view失效。

可捕获分配失败：每个新增容器增长用try_reserve，类型化AllocationFailure；capture失败用source安装时已有Atomic故障位+首个丢失sequence槽记录，不再为报错分配；worker已醒/下一open必读该位，保留已有正文并标incomplete。不能仅丢某raw再继续宣称完整；该读代际不可再Complete。projection分配失败标ProjectionUnavailable；raw保留成功时仍可安全读取原结构。read/buffer分配失败只使本次读取失败，不改变原生保存或工作结果。不会通过回收旧正文制造“重试成功”假完整；经真正新重读可建立新epoch。

std Arc创建/clone的引用计数溢出及运行时/标准库某些基础分配可能直接abort；本文不声称可恢复真实全进程OOM。新source创建的大对象须尽量fallible预留并在失败时拒绝该view安装而保原生control职责；既有RawDocument Arc为现实际输入，不扩大本次捕获分配。故障注入只能证明显式失败分支，真实OOM未执行须记录；不能用模拟OOM代签物理无限容量。

## 5. 结构投影与顺序

worker保留不可变FrameRecord原文；projection条目只引用原片段/字节范围与raw record，未知数值不经f64或serde_json::Value，工具及message原生ID不继承P2 4096门槛；缺字段/省略/null/未知各自保留。version-specific reducer处理已知字段，禁止通用JSON深合并。此阶段复用当前已核实ACP版本及原合同证据，不访问网络更新规范、不按厂商分流；新增协议解释须再固定正式依据和tester预期。

每个entry有平台AnchorId与首次可证来源位置。已知同toolCallId、同source/scope、同read generation的后续tool_call_update更新同原anchor，后续文字位置不动；允许先收到update后收到full：最早可靠同ID观测建立anchor，后续补已知字段，不把full再插一次。跨读代际不假设tool/message ID稳定，除非标准及实际source明确支持，默认整视图重建。ID类型/namespace严格区分，message与tool相同字符串不合并。

有可靠messageId时仅按该版本定义更新/追加同message；v1等缺ID的chunk按实际流序分别保留并标识为chunk，不用“相邻同role”猜完整消息。已验证版本允许的内容块追加只在可证同一原生message内执行。未知结构作为UnknownStructured{raw_ref,kind?}，安全占位+惰性原文可读，不能降成普通聊天文本或直接执行。

顺序依据是原生所暴露的顺序/位置，流中首次出现顺序仅报告为“本连接观测顺序”。actual Received.sequence及receipt_order只用于来源接收围栏/重放边界，不伪成原生全历史顺序、父子关系或每输入轮次。若无原生顺序/关系证明，说明缺失；不按类型重排、不用本地时间/正文/摘要去重、不将平台Run/queue顺序补成原生轮次。父子关系仅保实际引用与已验证关系，未知不猜子执行树。

item每次变化生成不可变version指向原raw；Snapshot固定H revision后，读该anchor最后一个revision≤H的版本，后续变化进delta日志，原文不被原地覆盖。旧version生命周期与该view一起，不以snapshot预算淘汰。实现可优化索引但必须保持版本可读和原文保真。

## 6. 来源、重放与限制状态

状态分轴，禁止一个complete bool包揽：

| 轴 | 状态与证据 |
| --- | --- |
| Connection | Current(actual Scope) / Replaced / Released / NodeStopping |
| Read | CurrentOnly / ReplayRequested / Replaying(ReadEpoch,actualRPC) / ReplayComplete(真实完成证据) / ReplayIncomplete(reason)；完成事实与写入状态独立 |
| Freshness | CurrentObservation / Revalidating / Stale(reason,last_successful_read) |
| Projection | Available / PartialKnownStructure / ProjectionUnavailable |
| Join | ProvenNativeIds / RebuiltNoStableIds / UnassociatedSegments |

无重放连接仍CurrentOnly，可输入/审批/恢复的其它能力独立；醒目限制为“历史不完整；节点释放该连接或重启后，当前缓存无法补回”。current-only即使目前所有观测均有，也不叫完整原生历史。未知扩展使Projection=PartialKnownStructure，不等于已经丢raw；真实capture丢失或协议断开前未完成则ReplayIncomplete，不因随后idle/新输入成功清除。

显式刷新、重开、重连及重新接管都经ReadCoordinator重新校验当前来源/配置/能力。已有当前view可立即以明确来源和Revalidating状态交付，同时发起实际支持的原生重读；不能把“返回现有cache”当重新读取。失败保旧view为Stale，错误单独表达，重试不发送输入、不伪Run、不复活审批。无重放时重新校验当前source后返回CurrentOnly限制，不扫描私有目录。

同NativeSessionKey一个在途读取操作；并发open/refresh订阅同ReadOperation，不重发resume/replay。新读取不得压过尚在途且可能写出的旧RPC来开第二次；要求刷新更新的调用标记desired_refresh，在当前操作终态后合并启动一次后续重读。两操作之间ReadEpoch不同，不因内容相同拼接。

加载、无重放恢复、标准重放是不同目的。NativeReadPort借同真实资源owner和完整已授权Client配置；配置未知或所需能力不可用只失败该读取路径。不能发空MCP/根目录当保持不变，不能在新scope用旧配置stamp冒完整来源。

ReplayEpoch在原真实RPC操作登记后、enqueue前建立候选view；用实际Scope+typedRPC+target+版本+read epoch匹配响应。当前ACP的DuringResume/DuringReplay是接收窗口归属，不保证其中每条就是历史、也不证明稳定item身份。候选记录该窗口内全部结构；不能证明live/replay无重叠时将先前current和本次replay分成明确来源段，不按正文合并。只有可证明同ID语义才合并；无跨load稳定ID默认重建：新epoch替代旧当前pointer，旧cursor stale，迟到旧RPC只能归旧候选。

ReplayComplete必须：原真实Replay操作登记匹配Scope+typedRPC+target+ReadEpoch的Response::Replayed（当前v1 load/v2 replay响应由Strategy按原RequestKind真实分类）、该响应之前捕获的所有本读相关raw已worker消费、对应范围没有capture丢失或无法解释为本次读取的结构错误。**不以WriteTicket Written或flush完成作为读取完成前提**；真实响应可早于flush，原WriteReport随后Unknown不撤销已经取得的协议完成事实。Transport状态另存Reserved/Queued/Writing/Written/Unknown/Zero，只用于如实解释传输和后续资源责任，不能抹掉原生读取完成。

只有Resumed的无重放操作永不证明ReplayComplete。匹配完成事实取得之前发生RemoteError/EOF/协议失败/实际读取不能继续则本读Incomplete；之后失联仍保完成事实，Connection/Freshness标旧，不捏造该完成边界前的历史缺失。完成边界之后新capture失败影响后续当前观测范围，不回写已完整读取的历史段。旧scope/ReadEpoch的完成事实可以归旧候选记录，不能发布覆盖当前view。Complete只表示本次标准接口暴露内容的重放完成，不承诺没有协议未暴露历史、外部之后修改或全部未知结构已能渲染。

重放来源不能关联不阻止展示实际得到内容；报告UnassociatedSegments及成本。读取中出现live但不可分辨时保持窗口事实，不能在Complete时声称“已无损合流”。重放完成/current-state不改变ND输入准入或原生活动结果，投影层不发dispatch完成事实。

## 7. 首开快照和增量的无竞态边界

1. 当前权限校验后，open对该source读取capture retained_frontier cut；该cut只是原实际Received sequence。若capture失败故障位已立，返回incomplete而非假缺失0。
2. worker的barrier(cut)在不持owner/客户端锁时等待已有handoff处理到cut；Notify先注册再核frontier，首开即使没有后续输出仍必唤醒并读取已有内容。取消等待只取消本次open。
3. 单target投影owner一次短状态操作取得ViewEpoch和SnapshotRevision=H，构造只读SnapshotCursor与DeltaCursor(after=H)。此操作不复制全部正文或anchor数组，不await权限/网络/SQL。
4. 快照读取按≤H版本流式交付，后续revision>H的变化保在delta日志。客户端完成snapshot后从H继续tail，不靠订阅时间猜水位；在open cut前后、H取样前后、首chunk与tail切换之间发生的每个变化均有唯一log revision，不丢也不重。
5. await_changes用“先订阅Notify、再比较当前revision/epoch/failure、再await”的模式；唤醒只是提示，客户端必须重新拉数据。没有每客户端正文push队列，无慢读背压传回pump。

快照和delta cursor是平台流式读取位置，不能作为native continuation token、原生轮次或执行完成证据。cut不含尚未被协议解析emit的远端字节；收到的定义仍以真实Received为准，不把peer.write当reader receipt。source terminal标记需在既有已捕获队列drain后按实际顺序发布，只影响view来源/完整性，不伪造Received。

## 8. 取消、释放与关闭

browser断线→停止其cursor/task/未发chunk，保节点view和原生工作；用户取消view读取→同样只detach订阅，不调用session/cancel。尚未enqueue原生读取reservation可由ReadOperation真实cancel并核Zero；已可能写的原读取仍由ReadCoordinator保原Ticket/RPC到真实终态，不因最后浏览器退出遗弃操作，不自动重发。当前ACP无通用无副作用replay cancel能力，不能虚构接口或用执行cancel顶替。

source换代/资源释放由真实resource owner通知：先seal该view新open/读chunk，标generation失效；终止对应wake/reader；保已开始chunk不可撤回事实；worker按capture最终frontier drain并标实际完整性；原生IO/进程关闭仍归ND原顺序，view关闭不成为其必要SQL或执行停止门槛。view可在独立截止时间内取消自己的projection任务并join；超时报告ViewShutdownUnconfirmed，不冒已join，源控制shutdown继续。节点supervisor保未join任务责任，不能detach遗弃。

节点停止：先seal view新请求并取消gateway等待/发送→seal capture（pump随后不再交接，只记closed状态且不阻控制）→worker处理已接收批次或记录未drain实际范围→取消/join自有worker/read服务任务→在不持handoff/view/client/ND锁时释放raw。ND source/IO/Windows Job关闭并行沿其自己的真实proof；view释放不能冒充原生成功。销毁大raw或全部版本的耗时只在worker/cleanup任务，不能在pump ack、actor gate或writer poll内发生。

## 9. 交付边界

此工程候选提供可编码的无预算view vertical模块、真实ACP测试入口与未来装配端口。实施可先交模块+独立L3/L4，并保留L5/L6未验证；没有Node/WSS真实接线、当前授权chunk发送及Web安全渲染就不能勾产品基础视图完成，也不能以traits/替身代签Node可用。未来文件写范围与串行合入规则见assembly-and-delivery.md。
