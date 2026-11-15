# ACP revision2 限制分类、写范围及旧资产迁移

2026-10-31。仅设计，没有实际删限或通过声明。依据当前负责 spec 和独立 design-review-v1.md 的逐项裁决；不是用户逐数字确认的新规则。v1 限制表和旧 oracle 原字节保留，本表取代其“未裁决”候选语句。正文、输出实体和关联/参数控制按真实用途分别处理，不因同为 4096 自动一起删。

| 当前真实位置/用途 | 固定 SDK 与当前负责范围 | 本阶段准确处置 |
| --- | --- | --- |
| transport::MAX_FRAME=32MiB，read_frame 累计 buffer 两处及出站 FrameBuffer | 双版无此统一阈值；Q001 acp-platform-constraints/structured-history 内容门槛 | 入站/出站/known/unknown 新链全部删。checked 长度、真实 Allocation/IO 错误保留，不换更大常量；固定 scratch 仅分块 |
| RawMessage 96 层和 131072 结构 token | 未知正文亦先拒绝；raw 合同 9f2b 已给 flat 索引替代 | RawMessage/Strategy/P1/P2 raw 入口删除这两项和 Value DOM 回退，深树 Clone/drop 不递归未知内容 |
| Strategy::session_listing sessions.len>4096 | 两版 sessions array 均无 maxItems；输出实体数量 | 删除纯条数拒绝，保每项 required/形状及完整 raw/真实分配失败 |
| model MessageId/ToolCallId decoded UTF8 4096 | opaque string 无同 maxLength；输出结构身份链 | 新公共接收/处理链删除纯长度拒绝，保 string、来源/相等性，不截断/哈希冒身份 |
| P2 parse_patch 的 tool/source Message+Tool key 4096 过滤 | 与上一输出身份同链，原 model 放宽不能被此处再丢掉 | 删除新 raw patch 链的纯长度过滤；完整 raw/真实 key 可取。decode_anchor 与旧 View key 载体见下一行，不能签整个投影长键已支持 |
| P2 decode_anchor、ViewWindows::item_cost/staging 同类 4096 key；ViewLimits 16/128MiB、4096/32768 项、64 身份、snapshot/StageWorkspace 计量；application::history 缓存预算 | 旧输出/显示预算，spec 总体需消除；不是协议控制能力 | 只能作为非必经遗留显示适配器存在，本次真实 IO/Received 队列/原文流不调用。P1/P2 raw API 适配不等于旧预算消失，不声称长 key 完整历史投影或无预算共享视图。Node 新接线前另 gate 整链改造 |
| P1 HEADER_BYTES32/READ_SCRATCH1024/FRAME_SCRATCH256，P2 ANCHOR_BYTES512/FIELD_COUNT16/TOOL_FIELDS9 | 固定已知元数据格式/处理块，并非总内容限值；旧 StageWorkspace 仍可能计费 | 不按同数值删格式。完整未知 raw 独立保留，不因 known 字段数截断未知成员。旧预算只能影响上述可选显示路径，不能卡 reader/权限/真实发送 |
| SessionId/OptionId parse decoded UTF8 4096 | SDK 无同 maxLength；当前非正文 session 管理/原选项控制差异 | 原样保留，不称 SDK 或用户数字要求，不推广到未知正文。无法绑定的长原 ID 明确拒该路径、不截断；P0_b 分类型冲突由 tester 维护 |
| Strategy 既有 list/replay/listing cwd/cursor 4096 | SDK 无统一字节阈值；旧非正文参数/分页控制差异 | 原路径原样保留并记录兼容差异；绝不扩到此前无此 cap 的 new/resume 或 SessionSetup 所有字段。类型、应用路径存在/绝对/授权、字节长度分开；协议不访问 FS，cursor 不是授权 |
| domain NativeSessionKey/native_id 4096 与旧 SQLite native_id CHECK | 公共协议范围外的稳定管理身份/旧布局 | 本阶段不改 model/DDL/迁移，不能拿协议长输出 key 测试签 Node 管理绑定已支持。后续对应 gate 处理，不截断旧记录 |
| RpcId string1024、安全整数、-0/分数/指数拒绝，生成 ID checked 范围 | schema 无1024，整数 int64，v1 null 可合法；当前平台非正文 RPC 关联控制差异 | 保现有精确范围/typed 身份，不经 raw.integer 误放 -0/指数；不施加到未知 raw 数字/字符串，不冒称双版协议都禁止 null |
| 两方向各64同时 pending，同 session resume/replay 在途唯一 | design/implementation-contract 保留现有控制保护 | 保 client Capacity/server65 poison 和零释放/响应释放，不改 options/content 数量，不变连接累计 quota |
| server_seen 旧累计4096 | Q002 明确删除连接寿命门槛 | 不恢复该阈值或更大阈值；HashSet 永久防重，8193+ 新请求合法，不能清理旧 ID 后复活 |
| u64 序号、usize checked 长度、真实 ID 范围/分配耗尽 | 实际表示/工程失败，不是自选内容 quota | typed Capacity/Allocation，不回绕/伪旧身份；不承诺物理无限。ReadReceiptCut 和 generation 同样 checked |
| prompt 空 array 的旧 filter(!empty) | SDK 两版 array 无 minItems，当前 spec 无额外非空规则 | 删除旧非空拒绝；[] 可真实发送，Harness 返回如实报告。已知 shape/required/cap 验证保留 |
| known content type 与 unknown Other、MCP 配置类型/版本能力 | 协议语义/完整配置，不是容量 | 已知 cap 必验，v2 Other 同版标准 raw forwarding、known 不可伪装 Other；v1 按固定 schema，不支持明确限所需路径，不降文本/缩水/自动空配置重试 |
| 管理 strict_json8、JWS/management_ws/pairing 认证帧及流控 | 非 ACP 原生正文通道，原认证/信任合同 | 完全不改，不把 ACP 放宽扩散到认证，也不反过来用管理8层/帧预算限制 ACP |

## 独立资产迁移责任与固定接口

tester 保留原 P0/P1/P2 33 例原文件/报告，另存机械迁移及 spec 冲突候选。coder 只维护开发测试，不改独立旧 oracle。

- fields 的 Value 比较改读 Result<RawObject>/RawValueRef；json! 可仅在测试生成原 wire，产品没有 Value overload。Connection<R,W> 改公开 spawn 的三句柄及真实 reserve/encode/enqueue/next/shutdown。原请求 params 断言读取 raw known 字段，未知内容按原词法比对。
- P0 三 chunk omitted/null/string、wrong shape、实际选择版本字段、scope/typedID/重复响应继续。P0_b 对 Message/Tool 去长度拒绝的冲突与 Session/Option 保留分别写新 oracle，不删除整个旧边界例。
- P1 原文、同连接/同 session、Live/Resume/Replay、部分历史不冒完整、状态不归因 Run 的断言保留。Received 必须来自真实 reader；窗口新增按实际 read receipt 冻结，不能用锁外 parse 完成时“现在窗口”替代。
- P2 name/metadata 顶层 patch、KnownAbsent/KnownEmpty/Unknown/Partial/Uninterpreted、原工具位置、rawInput/rawOutput/未知 _meta、坏结构拒绝、跨来源/旧 stamp 拒绝继续；原预算/Deferred 测试只证明遗留可选适配器，不能签新 IO 必经预算。
- Reservation 公开错误所有权为 Result<WriteTicket,EnqueueError>；EnqueueError.into_parts 返回原错误/Reservation/Frame；没有额外 public grant。OrderedRawObject/RawMetadata/Listing/Permission/Unsupported/Received 完整 getter 签名以同版主合同为准。
- ClosingDrain 的确定取得边界为实际 poll_read 返回的 LF bytes，不是 peer 已写入、parse 完成或 consumer next。真实可达 IO 屏障与锁内短步静态证明分列，不加生产 yield/hook 制造命中。ResponseDeadline 所有状态均由实际 IO owner timer 推进，无 consumer 轮询依赖。

## 交付范围与门槛

可写：ACP model/strategy/transport 及必要 reader/writer/state 子模块、setup 数据类型、P1/P2 raw 机械适配、公开 re-export 与开发测试/必要文档。raw_json 原合同与实现不变，不加依赖、不改锁文件、不改 SQL/schema/Node/WSS/管理授权。独立测试候选另交 tester/reviewer gate。

必须交实际可运行客户端和受监督 IO，而非 traits。双版可控真实 AsyncRead/Write、自有协议 child 实际管道都要验证；32MiB+、深树/token、未知数字/escape、完整配置、64/65、8193+、唯一回复、RAII 全失败释放、早 response/partial/控制优先、read receipt 的早到 state/伪 response、期限全阶段、关闭 drain/取消等待/真实 join 皆有实际或明确静态证据，不能零用例算通过。

不能声称：本阶段已实现全部无预算共享视图、long key 全部旧历史投影、真实 Harness/Windows Job 树、持久审批或 NativeDispatch/R3+SQL 实际派发、Node/WSS/Web。后续必要账本只能消费本 IO 的真实 scope/ID/接受和原响应事实，不重验已受理原 actor、未知不重发；具体 SQL 与实际 writer armed 围栏另 gate，本次不预造因果/DDL。
