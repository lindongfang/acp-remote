# 审批 owner：取消范围、单次响应与持有预算独立增补定义

2026-10-31。仅为编码前的独立定义，本文全部新增断言 **not-run**。不改生产、统一 tests 或已有验收预期。以下 AO 标签是既有 A–K / CL 的子断言名，不另分配 CL ID；与 `testing/runs/deliver-codex-session-loop/2026-10-31-approval-owner-definition.md` 的 A/B/E/G/H/I/J 联合使用。

本次依据 root `approval-owner-contract.md` SHA256 `6cd3167b648ae98a110140af8ddd1493092ea990e53eb5b4dcf1f084971ad591` 的 §1A/2A/5、permission-approvals spec SHA256 `58cf6faacddce9c774eb6344d34f7a7b5b4e21e045f8589a965069a4658636c6`、implementation-contract §8/8A，以及实际本地官方 SDK 2.1.0 两版 schema。版本与读取来源摘要另存 `approval-owner-cancellation-budget-inputs.json`。SDK 包版本不是协商的 ACP wire 版本。

当前 schema4 已实现且其独立迁移 24 项通过，这是布局证据；审批 owner、本文取消/账本/ReplyCell 产品行为均未实现或未验。旧 owner 定义中“schema4 未实施/DDL 未定”已过时，不能据此重开已关闭布局问题；其来源 API、单次消费及运行事务门槛仍有效。本定义不把本次合同补充当成整体编码放行，实际门禁交 reviewer。

## 本地双版语义核对与差异

本次亲读以下本地 JSON 指针；不发网络请求，也不运行 Web npm audit。

| 来源 | 精确范围及独立解释 |
| --- | --- |
| v1 `/$defs/RequestPermissionOutcome/oneOf/0/description`；`/$defs/ClientNotification/properties/params/anyOf/0/anyOf/0/description`；`/$defs/CancelNotification` | 本节点发送 session/cancel 取消 ongoing prompt turn 时，须对该会话全部 pending request_permission 以 cancelled 回应。不能要求逐请求证明属于某 Run/tool 后才纳入；也不能把此段扩大为跨会话批量取消，或宣称 v1 有 v2 后台/aggregate-state 完成合同。 |
| v2 `/$defs/RequestPermissionOutcome/anyOf/0/description`；`/$defs/ClientNotification/properties/params/anyOf/0/anyOf/0/description` | 明文是该 session 全部 pending 权限请求，包含后台；其他 session 不受影响。后台 cancelled 被当拒绝。session/cancel 对工作取消主要为前台及尚未插入 prompts，后台工作 MAY 停止；权限全部安全回应不证明后台工作全部停止。取消结束活动前台才要求 cancelled idle；原本无前台不要求额外 idle。 |
| v1/v2 `/$defs/CloseSessionRequest/description` | close 按 session/cancel 取消相关工作再释放 session 资源。v1 以已广告 sessionCapabilities.close 为前置；v2 取消活动前台时 final updates 与 cancelled idle 先于成功 close 响应，原本 idle 不要求多一条 idle。v2 prompt 的插入/错误响应合同独立，不能由审批 owner 凭 close ACK 推断。 |
| v2 `/$defs/ProtocolLevelNotification/properties/params/anyOf/0/description` 及其 `anyOf/0/description` | `$/` 通知一般允许忽略；若采用 cancel_request 处理合同，对原请求须发有效结果（可取消标记）或 -32800，通知本身没有响应。取消原请求活动及嵌套活动是 MAY，不是“只清显示，无执行影响”。 |
| v1 `/$defs/CancelRequestNotification`；根 `/anyOf/2/properties/params/anyOf/0/description` 及其 `anyOf/0/description` | v1 完整通用协议通知合同位于顶层 ProtocolLevel 分支，不在 `$defs/ProtocolLevelNotification`。与 v2 一样允许忽略 `$/` 通知；若采用处理路径，原请求须发合法结果或 -32800，取消对应及嵌套活动为 MAY。不能因只查 `$defs` 而误称 v1 缺少响应依据。双版均需冻结具体输入路由与选用的合法结果路径；不扩张为任意同值RPC批量取消。 |
| v1/v2 `/$defs/RequestId` | v1 schema 允许 null/int64/string，v2 禁 null。已交付公共 RpcId 仍以整型/字符串及其既有安全限制建模。本阶段必测 7 与 "7"、scope/方向隔离；不把 owner 工作偷换成已支持 v1 null RPC 的声明。v1 null 若需纳入 owner，必须明确独立子范围及现有 parser 的拒绝行为，不能误称 v1 schema 禁 null。 |

对旧 gaps 的精确更新：AP-TG02 的双版会话取消范围及单请求取消合法结果/-32800均已有本地依据足以定义；具体输入接入、选择处理策略和可观察事件仍未冻结。AP-TG05 已给出保守公式、保留引用和响应缓冲，原“没有费用公式”不再成立；实际账本转移/公共观察 API 及复制前预留证明仍待补。AP-TG03/04/06/07 不能靠 §1A/2A 自动关闭。

## 执行共同设置

L3 验证公开协议/单次状态/预算对象；L4 使用自有 schema4 SQLite 副本与两个真实 pool、同版双工 Connection/可观察异步 writer 验证 SQL 失败与 wire。所有 actor、会话、请求、正文、密钥均合成；不读用户身份。先固定 source/lock/test SHA；独占 target 和临时数据。不以 mock success 证明真正 COMMIT 或 peer 执行。每个 case 记录类型化 RPC、session/connection scope、writer 进入次数和解码后的响应种类/数量，日志只写摘要/计数，不打印原正文。

用确定性屏障定位：观察成功、决定 COMMIT、claim COMMIT、ReplyCell 消费前、Consumed 后但 writer 未交字节、首次可能写后、write/flush 返回前、结果 COMMIT。取消/Drop/超时均先终止潜伏发送，join 并关闭自有资源；停止前后核对持有统计。不能用概率 sleep 代替竞争顺序。真实 wall/mono 超时只在预算本身需要时使用，同源绝对截止不因重试刷新。

## 取消与单次响应子断言（A/B/E/G/H/I；CL-015/016/033/037/041/067/068/075–078）

| 子断言 | 操作步骤 | 预期与取证 |
| --- | --- | --- |
| AO-C01 v1 session cancel | 同一连接建立会话 A 的三个真实 pending 权限请求（整数7、字符串"7"及另一ID），含无可靠 Run/工具类别的合法请求；会话 B 建一个 pending。节点对 A 发送取消 ongoing prompt turn。 | A 三个各一份 cancelled；B 零响应且仍可按原广告裁决。原标量类型完全保留，不借最近 Run 筛选。继续 pump 原生最终工具/取消结果，不把“发出 notification”当 prompt 已结束。 |
| AO-C02 v2 foreground/background/idle | 对 A 同时保留前台、后台、无可证明前后台分类的真实 pending；B 对照。分别在 active 与已 idle 无前台时取消 A。 | A 每个 pending 都 cancelled 一次，B 不动；不依赖 tool/Run 分类。active 的 final updates 与 cancelled idle 正确接收；idle 无额外 idle 也不误报失败。不把后台 cancelled 解释为全部后台工作终止。 |
| AO-C03 cancel repeat and races | 对 C01/02 分别重复 session cancel；在消费前和 Consumed 后交错 selected、session cancel、safe cancel；先完成一个 selected 再取消会话其他 pending。 | 每个真实 RPC 总响应至多一次；先消费者固定 actual-kind。已 Consumed 的 RPC 不得再 safe cancelled；其他 pending 正常收敛。允许已持久 user allow / planned selected / 实际 cancelled 三者不同，但不写第二人工决定。 |
| AO-C04 single RPC cancellation | 协议对端发 `$/cancel_request` 指向其原先发出的 permission RPC 整数7；同连接字符串"7"、另一个方法的相反方向数值7及另一连接整数7同时存在。再测未知/已消费ID、重复通知、错类型/数组 requestId。 | 只有已验证同方向、同连接、原标量 RPC 可以进入取消处理，其他保持原资格或明确错误；通知本身零 response。双版采用处理路径时，原请求只发所冻结的合法 cancelled 结果或 -32800 一次；未消费前完成正常合法响应也不被重复答复。允许忽略通知的路径不把对象标为已取消，不凭未验证 raw 触发 safe/selected；处理取消不证明对端或全部嵌套活动已经停止。 |
| AO-C05 close scope/order | 广告 close 后关闭 A，保留 B 与共享实例另一目标。v1、v2分别执行；v1 未广告 close 对照。v2 active 对端先发 final updates/cancelled idle 再 ACK，另构造 ACK 先到的违规序列；idle close 不发额外 idle。 | A pending 沿同一单次取消入口结束；B/共享实例不被无条件杀死。v1 不支持时写前拒绝。v2 规范顺序可观察，违规不冒报规范完成。prompt response 仍按真实插入事实独立处理；close ACK 不证明进程树退出或其他 session 停止。 |
| AO-C06 non-invalidating facts | 登记无Run/后台请求后依次发生 Run终态、foreground idle、load/replay响应、展示缓存失败、浏览器断开；对照真正 RPC 取消、已回复、连接/resource换代及可靠专属工作结束且请求确实失效。 | 前组请求仍 pending 可正常验证，不造 Run/Turn；后组零旧 selected，按协议资格安全回应或零 writer。actor撤销只撤该actor读取/危险资格，不把所有后台请求自动过期。 |
| AO-C07 no DB needed for safety | 观察无行、决定实际COMMIT失败、决定成功未知、claim失败三分支持续保持数据库不可写，真实 RPC仍 pending。节点发安全结束；再测无 pending、原scope/raw不符与已Consumed selected。 | 零 selected；有效 pending 有界一次 safe cancelled（或冻结的单请求取消合法错误）；无危险permit/SQL成功也可执行。其他分支零writer。仅保留有界节点回执/资源未知并明确记录未确认，不能伪造持久attempt、决定或claim。 |
| AO-C08 Consumed is irreversible | 消费 selected 后，分别阻塞 writer 首次poll前、交一部分字节后、flush前，触发 future Drop、deadline、EOF、resource失效或 SQL结果失败，再重复 safe/end/查询/恢复。也对 cancelled 做同组。 | 从 Consumed 起不回 Pending，不改变actual-kind、不发第二答复；即使能证明尚未交字节也不生成第二次消费，可能写过只报Unknown。不能用not_sent状态或DB空actual恢复原资格。先前决定/claimed持久状态不当作可重派凭证。transport_written只证明write/flush，无执行ACK。 |
| AO-C09 late queue and stop | writer序列队列中保留尚未消费项，取消/停止owner并释放gate后让队列晚被poll；另一目标正常请求继续。停止中验证safe结束、连接关闭与join顺序。 | 迟到项零首次写，潜伏资格在释放gate前失效；已经Consumed项保持Unknown/Written。所有任务有owner并有界join；不能为一个目标停掉共享实例。完整Windows树/实际Harness单独未验。 |

## 持有账本子断言（J；CL-025/026/065/068/078及既有容量/资源子范围）

单位为字节，1MiB=1048576；8MiB=8388608；32MiB=33554432。独立期望计算只读借用实际持有结构及其 capacity；不能复制 JSON 来计算费用，也不能只调用产品 cost() 再拿同一函数作 oracle。

按 §2A：V 为本构建 `size_of::<serde_json::Value>()`。每 Value 首收 V；String另加capacity；Array另加128+capacity×V+各子Value费用；Object另加512+256×字段数+各key.capacity+各子Value费用。请求总额为2048+nativeSessionId UTF-8长度+字符串RPC UTF-8长度（整数无此项）+完整raw长度+上述JSON费用+可选toolId字节+每选项(64+optionId字节+kind字节)。保守重复项不减。全部求和/乘法checked；根8192、连接1024+32768响应预留、显示快照256+每引用64另外按其owner计费。计费不等于RSS。

| 子断言 | 操作步骤 | 预期与取证 |
| --- | --- | --- |
| AO-B01 exact formula | 使用ASCII/多字节UTF-8、数值/字符串RPC、长capacity短len字符串与数组、嵌套空对象/数组、未知扩展/多选项。独立遍历实际容量逐项核算；测试只读元数据不得提供无计费raw clone。 | raw、parsed、索引/option、fixed、tool和scope费用完整；容量不偷换len。根及连接固定账也加入节点总额。深度96/token131072前置保持，超限明确拒绝；checked溢出零登记零复制。 |
| AO-B02 per request boundary | 构造费用恰好1MiB与首次超过1MiB的合法帧；若公有容器capacity导致精确字节不可达，固定该平台可达的最大合法/最小非法，另在纯checked模型测精确等于。保留原32MiB公共帧接收边界对照。 | 请求≤1MiB才可拥有；超限在registry插入/clone之前拒绝，不把能解析大帧当能长期保留。限制明确、零自动selected，真实pending仍一次safe结束。记录实际可达性，不能伪称自然路径精确命中。 |
| AO-B03 connection and node bytes | 不超单请求与各session64限制，用多个session使某conn费用逼近8MiB；再用多个conn使节点逼近32MiB。逐项插入/失败/Drop后核对三层统计。 | 边界以内成功、超过拒绝且所有统计原子回滚，无负计数；固定连接缓冲和root8192计入节点。连接未满不代表节点有空间；一个连接满不影响仍有预算的另连接。统计不足以证明先计费，须配分配/ownership边界证据。 |
| AO-B04 count limits | 合法小对象使单NativeSessionKey第64/65当前引用、节点第64/65仍有负债connection identity、第256/257活请求对象、第256/257显示lease分别可达，避免别的上限先触发。 | 各精确计数边界受保护；超限不占槽、不增加正文、零selected。旧无效请求仍被lease/writer持有时算活对象，旧conn只要有负债仍算身份；资源换代不免费释放或复制。不可达组合如实列阻塞的先行上限，不造非法fixture。 |
| AO-B05 shared references and generations | 一个请求多份受计费显示引用；使请求失效/owner退役，但保留旧显示lease，再在新connection/resource观察相似raw。空snapshot与含引用snapshot均测，全部逐个Drop。 | 每snapshot256、每ref64计费；只共享同一已计费对象，不重复免账复制；旧正文最后引用Drop才退账，不能因owner失效先退。显示仍可展示不等于裁决仍有效，旧scope不能复活；最后退账回到对应root/conn基线。 |
| AO-B06 precharge transient peak | 在节点仅余不足clone/序列化/投影峰值预算时申请显示编码和新观察；分别在预留后、拥有前、registry插入前、构建失败及future取消时故障。 | 没有“先clone/serialize再核费”的瞬时保留绕过；失败只释放本次reservation，不影响已有对象。需冻结allocator/owner转移窄测试观察点；单看事后usage归零不能签先计费通过。 |
| AO-B07 reserved writer buffer | 正文账本逼满时使用合法最坏JSON转义RPC≤1024字节、optionId≤4096字节生成selected及safe cancelled；阻塞writer时再申请同conn另一响应/lease。 | 使用已经计费的32768缓冲串行复用，最坏响应在限内；超出明确错误，不截断/改写ID、不临时无限分配、不脱离owner排队。writer借用直至future结束/取消仍算请求负债，不能写开始就退账。正文满也不无限阻止safe结束。 |
| AO-B08 failure cleanup/isolation | 观察预算失败、显示失败、actor撤销、浏览器断开、session取消、conn EOF、stop和writerUnknown分别保留/Drop旧lease，等待所有owner任务join。数据库保留合成正文标记对照，日志/错误检查只作不存在性比较。 | 只释放自己引用；浏览器断开不结束节点owner，撤销不保留读取能力；旧lease尚在不假归零。持久表/审计/错误不写正文/标题/路径/原optionId；原决定/去重/责任不删，Unknown不改not_sent。自有最后引用释放后统计回基线，无无主任务。 |

## 来源证明、ReplyCell与类型负向的具体门槛

当前实际代码（不把旧 application/approvals 当新owner）表明：`PermissionRequest` 是 Clone 且公开可改字段，私有 connection tag只由真实 Connection路由附加；`Strategy::permission_response` 只编码响应，不提供来源授权。`Connection::permission_response` 重验私有tag、server_pending的raw摘要、原typed ID与重新解析广告，然后移除pending再write；write期间drop会保持连接Closed。该边界已由§8A测试覆盖一部分，但没有新的内部 Observation/ReplyCell/节点回执或持有账本端口。当前 `cancel` 只写通知，`$/cancel_request` 仍为 UnknownNotification；不可据此声称已处理§1A。

冻结实现前必须提供以下最小可验端口，不要求生产开放任意SQL/字段setter/伪造token：

1. **唯一来源入口**：从真实 Incoming::Permission取得opaque Observation；target/ResourceEpoch/instance等由可信source-owner绑定。必须在 Connection私有pending/raw仍匹配时验证，防止克隆可见字段、换raw、同义不同字节、重复scope等重铸来源；raw摘要明确为 `RawMessage::bytes()`（当前去掉换行/可选CR的真实帧payload），不能先重序列化JSON。持久摘要不是proof。
2. **统一消费端口**：selected携带必要COMMIT明确成功的单次危险permit；safe不要求该permit，但两者必须消费同一不可逆ReplyCell。需要可观察 Pending/Consumed(actual-kind)/Written/Unknown 或等价只读状态，以及消费屏障/实际writer回执；不能在两个writer入口各设自己的“发送一次”。
3. **取消事件端口**：真实session cancel/close与已验证单RPC取消的方向/scope原标量关联；单请求通知采用/忽略策略及v1/v2结果路径先冻结。未知通知原raw不能自动授予取消权，也不能把RPC字符串化混用。
4. **账本端口**：只读root/conn/request/lease费用与存活计数、可信预算配置/测试缩限策略、拥有前reservation转移与最后Drop观察、writer预留借用。空snapshot固定费属于节点；多连接snapshot的固定费如何归conn、parsed根Map怎样适用根Value费用、所有者固定开销从哪一刻退账需明确，避免不同实现各算一遍却都宣称合同成立。
5. **故障端口**：存储逐步/真实deferred COMMIT/成功未知、消费前后与writer首次可能传输、账本预留/拥有失败；仅cfg(test)窄枚举或外部port替身，不开放生产绕过授权。公开API至少能独立观察wire、DB事实和预算，不靠读私有变量推断真实授权。

外部负向编译待类型名冻结后逐项运行：Observation/source proof无公开字段构造、Default、Deserialize或从裸PermissionRequest/Value/DTO/摘要转换；危险ticket/permit不可Clone/Copy/Serialize/Deserialize且不能从DB行/CommandFact重建；ReplyCell不可公开new/reset/构造Pending或拷贝消费权；显示lease/原raw不得通过无计费Clone/into_arc取得免费保活；回执不可由裸actual-kind/transport状态伪造。共享非授权只读句柄若合理Clone，不机械禁止，必须动态证明只共享同一账本与同一单次cell。每个期望拒绝需确切编译错误而非路径/导入错误；类型拒绝不能代替并发/Drop/wire行为。

## 门禁与后续状态

这份取消/预算定义自身已可交独立审查，不等待完整未来owner方案。可以先冻结并实现独立的来源/ReplyCell取消模型与持有账本子阶段：取消组最低需要上述1–3及单次writer观察/故障屏障，账本组最低需要4及预留/拥有失败屏障；它们不必等待全部审批DML、签名DTO或最终恢复协调。尚无危险COMMIT证明的纯阶段不得输出产品selected资格；SQL/危险交付组等所属端口冻结后再执行。不能把纯模型预期或测试合成source当真实节点来源证据。

§1A的双版session范围、非失效事实及Consumed后无第二答复，与当前permission spec一致，可作为以上独立预期。§2A已足以建立精确保守账本/边界候选；复制前预留与所有引用退账仍必须靠实际API/源码/故障验证。上述端口与细节补齐并经reviewer限定后才能编码具体owner子阶段；无须新增用户产品决定。

本文没有运行任何新增测试，不给AP-TG03/04/06/07、完整owner或schema4新能力签完成。最终恢复reconciliation、未知来源范围与危险ready、实际认证/撤销/资源owner、WSS/Web、真实Harness与Windows完整树、完整L5/L6仍按原义务未验；safe一次收敛或文档通过均不代表这些已完成。所有外发Web审计保持未执行。
