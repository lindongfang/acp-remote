# 公共ACP runtime v1编码前独立预审

2026-10-31。**工程方向限定可行；当前冻结v1不放行编码。** 必须先补齐下述ACP-PLAN001–003，并取得同版独立定义；本次未改产品或代作者冻结新合同。真实客户端、独立reader/单writer及可取消可join的IO owner必须在同一阶段实现，不能用新traits或纯状态模型代交付。

## 输入与已核依据

主合同SHA256 `49f5c9a1aaa819302f99c7ad30dbc7e4a81bf7761945cb4a862cea34b5d23488`；限制表`4b5ae041ddcda2b677b6861e9519cef65318070c2c368b7cf3ce6cf790ce42b8`；freeze-v1 `5d2cd2534ddc2eabc5cefffa11156d984bd764671a7a21ca3c9349b59e5a7f47`。作者inputs.json的22项当前全部逐hash一致。亲读当前model/strategy/transport、P1/P2及ViewWindows相关键路径和固定SDK2.1.0双版schema；不访问网络或私人配置。

行为依据以当前acp-platform-constraints的“额外限制有明确归属与依据”“不新增输入正文和输出缓存容量上限”、structured-history的“结构身份与来源顺序”“实时缓存不承担保存”“当前连接视图由节点共享管理”、native-session-recovery的“标准恢复参数保真”为准。implementation-contract第15/43行及proposal第52行保留既有非正文控制；design第80行和Q-002只删累计4096寿命，不自动删除同值的其他保护。不能把未确认数字写成ACP要求或用户逐项选择。

## 三个编码前工程门槛

### ACP-PLAN001：Reservation未入writer时的RAII责任未闭合

reserve已占client pending、同session resume/replay或server ReplyReserved，但API只有enqueue后的WriteTicket可取消。主合同只明确Ticket Drop，尚未明确Reservation在encode失败、正常放弃、跨cell frame导致enqueue失败、连接关闭/到期后被drop的处理。因此存在合法公开路径只reserve不enqueue而永久占位，或原server request不能再答的歧义。

最小补充：Reservation持唯一未入writer责任；Drop在同state边界确认无真实poll后Zero/Abandoned并释放相应client/setup占位，server reply回Waiting，原server_seen不删除；initialize一经reserve仍封一次尝试。encode(&self)失败不消费Reservation，调用方可重试编码或drop；enqueue只有成功时把责任转Ticket/owned cell，所有错误分支均不能留下无人负责的已占位。固定冲突/已关闭/已过期的结果和优先级；保留WriteProbe可观察零终态，不新建可任意构造的释放grant。

独立定义至少覆盖普通RPC64反复reserve/drop后仍可用、setup同session释放、permission及unsupported零字节放弃后同真实请求可重新reserve一次、错误frame不影响另一cell、初始化放弃后不允许同连接再次initialize。由真正公开spawn路径取得这些对象，不提供fake cell工厂。

### ACP-PLAN002：ClosingDrain与shutdown的具体先后责任须固定

第9节方向正确：FrameAccepted的真实response若先于flush故障取得，即使route提交锁更晚，也不能被丢弃。但“已经读到并解析”与“已取得完整frame”表述还留有边界，shutdown又允许cancel/abort两IO任务。若关闭先abort reader或先清pending/发布terminal error，已取得的完整response仍可能被截掉。

最小补充：精确定义取得边界（例如本reader本次持有完整LF帧并已从framing buffer取出）；关闭先seal写入并保留原body-free关联，再让该已取得frame完成锁外parse/route短步。ClosingDrain不得新增读poll/server grant/write；仅对FrameAccepted且未消费的原记录提交一次。reader必须完成此步或被如实记录为未取得/不可恢复，不能一面宣称保证保留一面任意abort；terminal error入队须在两个IO任务终止和上述route结束之后。超时报告joined:false保留同Core任务句柄责任，后续shutdown继续，不take/detach。

独立定义至少有真实完整response取得后与flush错误/关闭竞争、事件先于terminal error、仅Reserved/半帧伪response拒绝、cancelled shutdown等待方再次关闭可join。无需增加生产yield；若精确同步短缝隙不可达，以真实可达IO屏障加源码锁序证明分列，不能伪造命中。

### ACP-PLAN003：外部可编译的私有数据句柄API还需最小表

主路径spawn/reserve/encode/enqueue/ticket/receiver/owner已足够具体。但PermissionRequest从公开字段改私有getter，尚未列出确切名称/返回借用；OrderedRawObject/RawMetadata的from_raw/get/iter/to_raw缺完整错误及迭代签名；SessionListItem的reported directories访问方式及Received各getter借用类型未固定。tester目前仍需猜实际Rust API。

最小补充仅列这些现有类型的公开签名/必要Clone与Debug规则，不加泛化框架或测试特供grant。补上ResponseDeadline::At在Reservation未enqueue、queued零poll、partial、FrameAccepted/已消费response各阶段到期的精确状态/连接结果；明确由真实reader/writer owner推进期限，不能依赖外部next()持续被poll。不得给v1长prompt恢复统一2秒等待。

## 额度逐项裁决

| 门槛 | 本阶段分类及最小处置 |
| --- | --- |
| 整帧32MiB、RawMessage 96层/131072 token | 明确内容门槛，实际入站/出站/known与unknown路径都删；raw flat索引、checked长度及真实Allocation/IO失败替代，不能换更大阈值或隐式Value回退。 |
| sessions.len>4096 | 列表输出实体数量，必须删纯数量拒绝；保必需字段/每项raw和真实分配失败。SDK两版sessions array均无maxItems。 |
| MessageId/ToolCallId 4096，以及P2同一输出身份的长度过滤 | 输出结构身份链，不能以旧实体键预算把合法完整输出拒掉/截断。新公共接收链须去纯长度拒绝；保string形状、原始身份、来源与相等性。P2 parse_patch/decode_anchor及ViewWindows item_cost/staging另有4096必须逐处列账，不能只改model假称全链支持。 |
| ViewWindows 16/128MiB、4096/32768项、64身份、snapshot/workspace计费 | 均为旧输出/视图预算，整体必须后续消除。本阶段Received队列和真实IO不得必经它们；P1/P2若仍为旧可选显示适配器，明确该剩余差异及局部测试范围，不能签完整无预算共享视图或长键完整投影。接Node前须另gate。 |
| SessionId 4096、OptionId 4096 | 可在此阶段原样保留为现有非正文身份/原选项控制差异，不因Q-002同值自动删除。不能称SDK maxLength或用户逐项选定；不得把它扩大到未知raw字符串/正文。长ID若无法管理绑定，明确拒该绑定路径，禁止截断/哈希冒充原ID。 |
| domain NativeSessionKey/native_id及SQLite native_id 4096 | 本阶段写范围之外的原生管理身份/持久布局，保留。新公共协议支持与Node管理绑定能力分列；本阶段不暗改DDL或声称长原生ID已能接管。 |
| cwd/cursor 4096 | 可以原样保留现有list/replay/listing参数控制差异；这是平台保护，SDK无这个统一阈值。不得扩到此前new/resume没有长度门槛的SessionSetup全部路径。类型、绝对路径/存在/授权与字节长度必须分开；协议层不访问本机FS，opaque cursor不作授权依据。任何保留拒绝须列为兼容性限制，不称全ACP支持。 |
| RpcId String1024、安全整数范围、-0/分数/指数原语义 | 保留现有非正文RPC关联控制，不推广到raw内其他数值/字符串。两版schema无1024限制，整数schema为int64；现有安全整数范围是平台差异。v1 schema允许null而现有实现拒绝，也不得冒称双版协议统一禁止。typed string/integer身份、重复/单次响应不放宽。 |
| 两方向64同时pending、同session setup在途唯一 | 明确保留；当前design/implementation-contract直接保留现有同时未决控制。65的行为须维持对应方向现有Capacity/poison语义，不改成内容数组/options数量或连接累计限制。 |
| server_seen累计4096 | 已删除的寿命门槛不得恢复；8193+真实双版处理后仍可用，旧ID防重HashSet不能清理复活旧请求。 |
| 固定元数据header/anchor字段数、小scratch | 格式/处理分块并非总内容门槛，可保。未知成员必须另有完整raw，不能因字段数截断未知结构；计费仍属于上项旧显示适配器差异。 |
| prompt空array | SDK两版prompt array均无minItems；当前负责spec未建立非空新规则。新公共client应删旧filter(!empty)的额外拒绝，交由实际Harness合法性结果如实报告；已知type/必需字段/广告验证继续。 |
| 管理strict_json8/JWS/WSS/pairing | 非ACP正文通道，不改，也不得反向借用其预算封ACP正文。 |

以上是按现有授权范围划分的最小工程处置，保留项不是“用户确认了该数字”的新结论。若作者希望额外放宽身份/参数控制，可以另明确同数据链及用例；本次并不以相同数值推导删除，也不新增更严保护。当前未发现必须改变已确认产品规则才能实现本公共客户端的冲突，无需据此反复问用户。

## 已确认可行的版本/数据/IO边界

- 固定schema v1 `1c57095acf9c0844054dd1fdc3624c121b3fdc24209ecc987fd6748c705fdef5`、v2 `28c7251e820511da257cdb55d30e2c30bbbe59226305b04e3b751dc655e0778d`。v1 stdio无type discriminator且name/command/args/env必需；v2 type:stdio及name/command必需，stdio/http须各自session.mcp广告；v1 http/sse为boolean广告。草稿这些字段和additionalDirectories能力位置正确。
- v1 loadSession才load重放；resume对象不代表重放；v2 session:{}提供baseline、resume replayFrom:start与无replay分开。Known(empty)与Unknown分开、不Default、不把列表来源自动当授权，满足配置保真；跨版本无法无损编码只限制该setup路径。
- v2 Other MCP/content允许标准保真forward，且明确排除已知type，不得绕过已知capability检查；不支持由Agent拒绝，不能重试空配置。列表additionalDirectories的null/schema差异单列，不升格KnownEmpty。无需厂商猜测或协商副作用探针。
- 原RawDocument基础层能支持本次浅对象/数组迁移；原raw/数字/escape独立保留。RawMetadata是有序浅投影而非收到原字节，明示重编码外层key、保原value子片段的边界正确。Debug/错误脱敏覆盖所有新句柄和配置env/header。
- 私有cell绑定的Reservation/EncodedFrame、真实reader mint Received、外部短App锁包每poll PollIo、ProtocolState只在真实poll+计数短步持锁，方向可实现。大编码/parse/digest均在锁外。必须以真实accepted/LF及flush事实分别验证，不能信fence自报字节；fence恰一次/零次约束及InvalidFence需实测。
- 零发送取消、半帧poison、FrameAccepted早response、flush失败保原响应、Writing0让控制先行、每次Pending后重核fence这些时序不能只做静态enum。真实管道child和背压权限到达是本stage必交；OwnedProcess.take_io存在可用对接入口，但本层不负责进程/Job树回收，不因此取得原生Ready或完成事实。

## 后续门禁

作者另出revision保v1字节，补齐三项及限制表；tester按同SHA固定机械迁移/新冲突预期与真实IO oracle后再最终预审。旧P0/P1/P2资产原断言/报告保留，变化逐hunk复核，不由coder删除。阶段结束还需同源真实编译、公开API/实际IO、并发取消关闭和全库组合检查。

此报告不签Node/WSS、必要派发账本、actual R3 owner接真实IO、原生Run因果终态、真实Harness/Job树或完整无限共享视图；这些仍须后续各自gate。不能把本次合同审查写成旧32MiB等容量已实际删除。
