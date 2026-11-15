# ACP 限制分类与旧资产迁移候选

本表分开当前真实用途、固定SDK是否有同一上限、当前spec负责范围及本阶段处置。Q002只明确删除连接累计4096 seen-ID寿命，不按相同数字推导其他4096全部应删。当前仅设计，未修改任一源/旧测试，未把候选工程建议写成用户已确认。

| 当前位置/门槛 | 实际用途与primary | spec归属 | 草稿处置/门槛 |
| --- | --- | --- | --- |
| transport::MAX_FRAME=32MiB：read_frame累计buffer两处、FrameBuffer出站一处 | 整个ACP JSON行/编码正文；固定双版schema无平台32MiB统一上限 | Q001 acp-platform-constraints/structured-history内容上限 | 本公共客户端整链删除。checked长度/try_reserve/真实IO错误，不换更大固定数，不将scratch chunk当总量 |
| RawMessage::parse 96层、131072结构token | 在任何未知内容上先拒绝；固定schema不授权这组阈值 | 同上；raw基础9f2b已给保真替代 | 本阶段真实RawMessage/Strategy/P1/P2入口切raw_json，删除这两项，不能保lazy Value树 |
| domain::views ViewLimits 16MiB/128MiB、target_items4096/node_items32768、MAX_IDENTITIES64、snapshot/StageWorkspace计量；application::history同16/128缓存 | 输出字节/实体/视图引用预算，间接使raw和锚点Dropped/Deferred；不是ACP标准数量 | Q001明确取消正文/输出/实体及快照旧预算 | 必须在无预算共享视图改造消除。此公共IO/Received队列不调用这些额度，也不把P1/P2可选历史适配器作为入出必经。旧实现仍存在的部分逐处列未实现差异，不能签整体CL095/Node输出无限制；Node新接线前须另过同源无预算视图gate |
| P1 HEADER_BYTES32/READ_SCRATCH1024/FRAME_SCRATCH256、P2 ANCHOR_BYTES512/FIELD_COUNT16/TOOL_FIELDS9 | 已有固定元数据布局/小scratch，不是总正文长度；但目前由StageWorkspace预算计费 | 结构格式/旧显示适配器 | 不按数值自动删除格式字段。raw所有未知结构必须独立保存，不能因这组已知字段数截断未知字段；预算触发不得影响真实IO/控制 |
| Strategy.session_listing entries.len>4096 | 拒绝合法响应的条目投影数量；双版sessions array无该maxItems | structured-history基础发现、acp-platform-constraints额外限制；疑似输出实体间接限制 | 推荐删除纯数量拒绝，保每项required字段/原raw；需reviewer按当前spec分类确认，不写用户逐项确认。P0/P1/P2之外列表旧资产由tester提出新预期 |
| SessionId/ToolCallId/MessageId/OptionId parse decoded UTF8<=4096；P0_b；P2 toolID/source key同4096 | 关联/参数标识符保护；SDK定义opaque string无同一maxLength。不同于seen-ID累计4096 | ACP来源/身份与额外限制；可能间接限制输出实体，须逐类型核 | 尚未裁决。候选建议取消纯展示Message/Tool/Option长度上限时同步审所有key载体，Session/native绑定另核；编码前reviewer分类决定实际hunk。不能只删model但留P2/key等隐性拒绝，也不能宣称当前已获确认删除 |
| Strategy.list/replay/listing cwd/cursor4096 | 路径/分页参数控制；SDK string/AbsolutePath描述无4096字节统一阈值 | workspace-registration路径有效性、harness发现、原生配置保真及额外限制 | 尚未裁决。列纯长度与路径/授权/类型验证分别审核；不按数字同值自动删。协议不访问本机FS；App仍验证cwd绝对/存在/授权，cursor仍opaque；不能把长度删除偷换为任意路径授权 |
| domain::native NativeSessionKey native_id4096、schema3 product_targets native_id4096 | 稳定管理原生身份/SQLite布局，位于本公共协议之外；已有表保留 | native-session-recovery身份及迁移 | 本阶段不改旧DDL/NativeKey规则。即使后续协议解析放宽，不能把管理绑定尚不支持的长ID截断/冒合并；需在对应model/迁移gate另核，不能以ACP客户端测试签Node支持 |
| RpcId String1024、精确整数abs<=9007199254740991；生成请求IDchecked同范围 | 非正文RPC关联标量范围；区别数字/字符串；既有控制保护 | 来源/唯一响应/当前控制边界 | 本候选保现有范围，不把它施加到未知raw内容/数字。schema整数helper能读u64/i64不自动放宽RPC许可；如以后改变须单独原始ID/重复证据门槛 |
| transport两方向各MAX_PENDING64；同session setup在途唯一 | 同时未决关联和回复控制，不是连接累计或正文/输出容量 | 当前控制保护，Q002明示保留 | 保64/65既有Capacity/poison或未超限排空再用语义；不将其改成累计quota、不以其限制options/content array |
| server_seen累计4096旧寿命 | 长连接已见Harness request ID累计次数 | Q002明确删除，现实现已移除 | 保原HashSet防重，8193+及后续可处理，不加更大阈值/清理复活旧ID |
| u64序号、usize checked长度、safe请求ID生成范围耗尽 | 实际表示/分配失败，非平台自选内容quota | 工程错误如实表达 | typed Counter/Allocation/Capacity，不回绕，不伪造同scope旧ID；不称无限物理资源 |
| 管理strict_json 8层、JWS/management_ws/pairing认证帧及流控 | 非ACP原生内容通道，已归档身份控制合同 | 原认证/信任长期规格 | 完全不随ACP raw改动；不得将ACP放宽扩散到认证，亦不得反过来用auth帧上限限制ACP未知正文 |
| prompt空array拒绝、known unsupported type、未知MCP/content变体 | 内容合法性/能力而非字节数。SDK prompt array无minItems，v2明确Other保真转发 | session-execution内容验证/版本能力/额外限制 | 空array旧规则列供reviewer核，不暗写新产品要求。已知type保实际cap；v2 Other按原文标准forwarding，不声称Agent支持；v1未知无可编码依据时只限该路径。不得把unsupported转换文本/过滤后发送缩水配置 |

## 旧P0/P1/P2与回归的精确维护责任

- tester保留原资产/报告字节，另出机械API候选：RawMessage.fields的Value比较→RawObject/RawValueRef只读；fixture json!可仅在测试生成原wire，不能成为产品Value兼容入口；Connection<R,W> alias→真实spawn的Connection/EventReceiver/IoOwner；next_raw helper实际等待Received；Request参数断言读raw字段而不normalized全树。
- P0三chunk omitted/null/string、wrong shape、实际选择v1字段、raw字节/来源/顺序、v2粗投影与重复RPC断言保留。P0_b4096属于上表未裁决项，不由coder删/弱化；reviewer分类若改变门槛，再由tester给冲突预期候选。
- P1完整原文、外部Source品牌/同session、Live/Resume/Replay窗口、部分历史不冒完整、不因预算掉记录影响控制的断言保留；读Received必须经真实Connection，不添加可伪造品牌工厂。
- P2 name/metadata顶层patch、Unknown/KnownAbsent/KnownEmpty/Partial/Uninterpreted区别、原工具位置、rawInput/rawOutput/未知_meta保真、坏结构拒绝、跨资源引用与旧stamp拒绝保留；有容量/Deferred旧预期冲突由tester按Q001另维护，不回填旧通过报告。
- 本阶段新增必须实际走公开spawn/reserve/encode/enqueue/receiver/owner.shutdown，覆盖writer背压仍收到嵌套权限；SQL等待只模拟外部consumer工作，不称真实Node/SQL已接线。下一阶段真实dispatch须实际SQL+R3 owner+同IO，不把本接口gate当原生派发通过。

## 本阶段结束可声称与不可声称

可以：公共ACP raw全链无32MiB/96/token门槛、完整配置无隐式清空、独立reader与单writer、来源/唯一回复/取消及半帧/早响应状态按真实IO计数验证；旧精确范围回归有新同源证据。

不能：本次所有数字限额已被用户逐项批准删除；旧ViewWindows/history实现已无预算；配置列表来源即授权；raw基础或协议替身证明真实Harness/Windows Job树/NativeDispatch SQL/Node/WSS/Web已通过。关联/参数控制分类未过独立gate前不编码。
