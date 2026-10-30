# WP3 编码前独立预核查

日期：2026-10-30。结论：**NOT READY：受影响的协议关联及审批安全接线应先补合同；WP2 2.3 尚未冻结，WP3 依赖门禁亦未解除。** 本报告不是最终产品验收，不撤销此前 WP2 有限通过结论。

独立 reviewer；只读产品与规划，仅写本报告；不继承 coder 自评，不重跑 Harness，不读取凭据，不修改 plan 汇总。读取 AGENTS、config、reviewer 职责、当前 proposal/design/plan/session-contract/tests、相关增量规格、真实探测持久结果与独立 network-retry/readiness 报告，以及实际 access_policy/trust、领域和存储接口。Git 初始存在用户/其他执行者未提交规划和产品修改；未触碰这些修改。没有读取活动 WP2 2.3 工作区，不能评判其候选实现。

## 问题

### W3R-001 — 阻断：审批发行者和交付前撤销之间缺少可实现的安全边界

依据：session-contract §1 要求 WP3 接入真正有效审批发行者；§3 要求逐操作 client_gate/代数复核；§4 要求唯一裁决与交付分开。design Decisions 5 只定义 Run/实例代际/原生请求关联。实际 access_policy.rs:59–68 的 ApprovalPresentation 只有客户端可呈现的 expires_at，:241–273 始终 ApprovalUnverified；ActorContext 是内部存储上下文，不是授权。现有数据库 approvals 唯一键使用 instance/generation/native_request_id，但尚无被指定的可信发行者及防伪授权对象合同。

缺口：谁可把协议请求变成有效审批、它绑定哪些不可变工具/作用范围/选项事实、由谁在裁决事务和交付之间消费，以及撤销/取消/连接退出在这段窗口内的先后关系未明确。仅把默认拒绝替换成“数据库里有 decided”或“可信 actor”无法满足现有安全目标，也可能让已撤销决定继续交付。

最小修正：在 session-contract 增加内部安全接线节，指定只有受 owner 管理的已协商连接可发行待审批事实；绑定 session/Run/instance/generation、原生请求 ID（保留类型）、请求结构与广告选项。规定授权对象不从 Web 时间戳或 ActorContext 构造，决定提交后交付前仍复核 actor 代数、Run/连接有效性、选项及强制刷新；明确 client_gate、会话 owner 与存储事务的统一锁顺序，以及撤销先完成时零交付、交付先越过边界时保留已交付事实。交付未知仍不自动重发。补 CL-004/014–016/031 的窗口断言，由独立 tester 维护/确认。无需新增用户授权：这是落实已确认的安全语义。

### W3R-002 — 阻断：typed toolCallId 关联规则未进入实施合同

依据：design Decisions 4 只规定同源条目更新；全 change 合同没有 toolCallId 定义。DeepSeek 持久 real-probe.json 的 initialAttempt 明示第一次因未把权限 toolCallId 关联此前 tool_call 而取消；preflight 追加记录第二次按同会话工具标识关联 rawInput 后才正确 allow_once。实际探测已证明不能假定 permission 自身包含完整工具命令。

最小修正：定义原生请求 ID、toolCallId、messageId 为不同命名空间，保留 JSON 标量类型，禁止把数字 1 与字符串 "1" 混同；工具索引至少绑定实例连接代际和原生会话，关联当前 Run，禁止跨会话/旧连接借用 rawInput。permission 的工具详情来自已验证关联及请求自带结构，保留原始字段；缺失、冲突、迟到或无法证明同源时明确不支持/失败关闭，绝不猜测或默认批准。若允许等待迟到工具事件，必须有有界期限并在期间保持未授权。补乱序、重复、同 ID 异类型、不同会话复用 ID、旧代际碰撞及无 rawInput 分支。不要把探针的固定命令白名单写成产品审批规则。

### W3R-003 — 编码前文档补实：初始化选择 v1 的解码顺序

依据：design Decisions 0 已有正确原则，spec harness-management 的“按规范协商回退”亦明确；因此不是缺少产品授权。DeepSeek v2-offer.json 实际单次 v2 形状 offer，返回 protocolVersion=1 与 agentInfo/agentCapabilities；当前合同未写逐字段引导算法。

最小修正：在版本语义对照中写清先从受限初始化响应读取 selected protocolVersion，验证平台支持且不高于 offer，再用 selected 版本形状严格解析整响应并冻结策略；不能先套 v2 成功 schema 而误拒合法 v1，不能再次 initialize、按程序名选版本或重发业务输入。未知选择、错误形状、错误 ID、无共同版本应明确失败。把真实 offer→selected1 纳入 CL-036 合同夹具，并保留 Codex 双版路径。该项在协议引导编码前补入合同即可；不要求重新做无副作用探测。

## 已足够与尚待验证

- 通用公共传输 + v1/v2 策略的边界合理，禁止厂商分支明确。v1 prompt 响应终态与 v2 messageId/idle 差异及恢复 snapshot 不计完成已有规范和实际有限证据，可以作为实现依据。
- 原生恢复设计足够表达行为：持久绑定原生范围/ID、优先 resume 无重放、不能新建替代或灌历史，恢复前置失败应保留历史。Codex 两版、DeepSeek v1 跨进程同 ID/上下文召回证据支持可行性；不等于异常重放、外部并发写入或产品恢复通过。
- Windows Job Object 所有权、共享实例不得单会话强杀、15 秒关闭预算与任务等待原则已明确，**不以尚无产品 Job 实测阻止编写该平台实现**。实施必须在子进程可执行用户代码前建立树归属（例如 suspended→assign→resume）；不能只 spawn 后追认。assign 失败/嵌套 Job 限制须失败关闭且回收启动进程；禁止 breakaway 或继承 Job 句柄造成逃逸。开发/独立验收应覆盖孙进程为系统 PowerShell、父先退出、强杀超时、节点异常退出及共享引用，不以仅查专用 exe 路径为树清理证据。这些属于平台实现及 CL-030 验收必需项，尚未通过。
- 新增依赖的必要性、许可证、维护、安全、Windows 支持和锁文件仍需 WP3 实际选型后审查。当前未给未选库作预先通过。
- plan Merge Order 禁止 WP3 与 WP2 同改 storage/lib；2.3 冻结、独立审查及串行交接后才允许 WP3 接管。上述文档补实可先行，不需提前写产品。

## 复核方式与固定快照

主执行修正文档后交回本 reviewer，以同 ID 独立复查；tester 若修改验收预期须独立确认。W3R-001/002 未闭环前不开始相关 WP3 产品代码；W3R-003 在引导代码前补实。当前仅预核查，不签 CL-005/030/031/032–041，不勾任何任务。

| 文件 | SHA256 |
| --- | --- |
| proposal.md | C92C15C2CCE6C78ABE741C8264120CCCF7EA22CEF0D3A7FDED39D2DB6058DD66 |
| design.md | D42BEBFB4E93DEF9E8B20CCB5FB710E12EE531CA86F85A2DE7AFD96FB6CD566C |
| plan.md | 0FD070878778AA96385479C79882FC7A2C25F7104F1EFD86F3C5C4A246FF8A8D |
| session-contract.md | A1D47843629CFE32A1B01ECE5A8CB021BF82804C969AD3132166BB0991D3E2C0 |
| tests.md | C757EEA6D4EEEAFE3DE9E7815BD5E11A1E89DF0A1000374BDC5B3CCD8E8ADBC9 |
| app/src/access_policy.rs | 1C8FDD1F47F73CD1B36B2BE51EAD0EACFCD4324C115936E797A3901E921E1544 |
| app/src/trust.rs | 9A53EF143344ED008A1BF4A09C31DC5A1AE26761D969BCD84788FCB1D1383EEE |
| 2026-10-30-deepseek-real-probe.json | DD9BCC445C63740A8DB784FB1B5B2A0DB7FAF4DD07E0E463EDAEFB9CD7C1C402 |
| 2026-10-30-deepseek-v2-offer.json | 716E22A66F02380760FA801A64606AD05FEA7E2F68CFA2236B71049AEEE908DD |

前五行位于本 change 目录，末两行位于本报告同目录。本次未运行产品测试、浏览器、Job Object 或真实 Harness；没有把历史报告的自评当独立最终结果。

## 同日追加：§7–8 修订独立复核

结论：**W3R-001/002/003 的设计合同缺口 Closed；当前不签 WP3 整体 READY。** 本次重新实际读取 session-contract 全部相关旧条款和新增 §7/8，未改产品/规划。修订合同 SHA256=`1CB0A4E67D6B8475B37EE64E1F9F21023046F14A8AD3B0F8EC9009F1EEAA6D7A`。tests 仍为 `C757EEA6D4EEEAFE3DE9E7815BD5E11A1E89DF0A1000374BDC5B3CCD8E8ADBC9`，独立 tester 的新增窗口断言尚未在本次读取版本出现，不能宣称用例检视也已完成。WP2 2.3 冻结/独立复核/串行交接仍是另一项依赖。

- **W3R-001 Closed（设计合同）**：§7 明确 owner 管理的已协商连接为唯一发行源、私有构造及不可变绑定，排除数据库 decided、ActorContext、客户端时间戳直接产生授权。统一 client_gate→会话交付协调区→短事务没有与现有撤销 client_gate→事务顺序形成反向锁依赖；强刷在最终锁区之外完成，并在锁内复核版本，发生变化即释放重做，可避免锁内等待刷盘以及旧刷新结果被当成当前事实。会话取消/终态/代际变化共用协调区，因此能与交付决定排序。交付认领持久化后仍须在同一 gate/owner 协调区到达实际 writer，不能只向异步队列排队；撤销先完成则零交付，提交先发生则保留已越界事实，与 §3 已交网络栈数据无法收回一致。写入受关闭预算约束；崩溃/传输或确认持久化失败进入 unknown 且不重发，满足 §4。实现必须把“实际提交 writer”落实为到传输边界的写入操作；仅 enqueue 完成不符合此已明确合同。对端后续执行不因 writer 成功而获证明。
- **W3R-002 Closed（设计合同）**：§7 已区分请求 ID/toolCallId/messageId 命名空间并保留类型，索引绑定 instance/generation/native session/current Run，保留请求及关联工具原结构，禁止跨范围借参数、冲突覆盖和猜测批准。当前选择不等待未来工具事件自动批准，使缺失/迟到未关联分支可确定失败关闭；没有把测试命令白名单变为产品规则。rawInput 不存在但其他结构足以可信解释的请求仍可表达，这与保留协议语义的规格一致。
- **W3R-003 Closed（设计合同）**：§8 已明确单次 offer、先核响应 ID/selected 版本、再用选中版本严格解码并冻结策略；selected1 采用 v1 响应形状，错误选择/形状拒绝且不重复 initialize 或业务输入。与 DeepSeek 公开真实结果及 harness-management 协商场景一致，没有把选回 v1 描述成对端支持 v2。

本次未发现这些新增合同与现有保存、撤销、唯一裁决或未知不重发要求矛盾；不要求进一步制造产品行为决策。仍需独立 tester 补充并由 reviewer 核读：强刷后版本变化、裁决 COMMIT 后撤销先于 writer、writer 先于撤销、取消/代际先失效、认领后崩溃与确认落库失败、typed ID 跨作用域碰撞，以及 selected1 全形状解码。只有合同关闭，不把拟定断言写成已执行；CL-030/031 与产品安全/Windows 证据保持未验收。本次 `git diff --check` exit0，仅既有 LF/CRLF 提示。

## 同日追加：独立 tester 新窗口断言复核

结论：**WP3 设计/用例定义前置 PASS，W3R-001–003 保持 Closed；不签 WP3 整体 READY。** 实际重算 tests SHA256 为 `2920790D47736EF11667C4430A7A4EAE42AC8C162C0463344A2B2B971671865A`，合同仍为上一节 `1CB0A4E67D6B8475B37EE64E1F9F21023046F14A8AD3B0F8EC9009F1EEAA6D7A`。实际读取新增对应步骤/预期以及 tester 在 contract-test-review 的追加报告；标题计数仍为42。本 reviewer 没有重新执行产品场景，也不把 tester 的标题保持比较改称自己运行了前后标题比较。

- W3R-001：CL-004 在裁决后、认领前、认领后至实际 writer 前设置屏障，并观察 gate/owner/短事务/writer 顺序；CL-014 双向排序覆盖撤销、取消、Run终态、换代；CL-015 覆盖认领后传输/崩溃/确认持久化失败 unknown 及重连重试零二次交付；CL-031 明确统一锁序、锁外强刷、版本变化退出重刷、实际 writer 同协调区及禁止队列绕过。与 §7 一致，断言不会要求持锁时让撤销强行越过它：先失效/先提交分别由屏障在合法顺序窗口验证。
- W3R-002：CL-013 区分可信同域关联可解释与无法关联失败关闭，不要求 rawInput 总存在、不等待未来事件自动批准；CL-016 覆盖类型/命名空间、session/原生会话/Run/instance/generation 碰撞、冲突/迟到及伪造发行来源；CL-031 检视私有发行入口与索引。对应 §7，保留原始字段与未知能力边界，没有照抄探针命令限制。
- W3R-003：CL-036 覆盖单次 v2 offer 选 v1 响应完整解码、selected 版本/响应 ID/形状错误、无共同版本及请求次数；预期冻结所选策略，失败不二次 initialize/业务重发，不把 selected1 写成 v2 支持。对应 §8。

这些用例级别仍声明 fixture 与真实组件边界，缺故障入口应 not-run，不要求用户重复人工批准。已有产品断言保持待实现/未执行，不因文档通过而提前勾任务。**剩余开工依赖为 WP2 2.3 固定候选的独立检视/验收及串行交接；WP3 新运行依赖仍按实际选型审查。** 若冻结实现迫使合同改变，应重新按受影响问题和用例复核；本结论仅解除本报告发现的设计/用例定义阻断。

## 同日追加：§9 Web 投影编码预核查

目标合同 SHA256：`D628F04647CEECE7B112E631859C6F97EF75FDD225EB284CBE393B2AB2A6952E`。实际核读 §2/3/5/9 与 harness-management、structured-history 既有相关规则；没有读取或评判 UI 活动实现。**当前 §9 不能作为完整管理/Web 接线合同通过；下列新问题不重开 W3R-001–003，也不阻断与之无关的分块纯工具准备。**

### W3R-004 — 阻断列表接线：合法列表项可超帧，缺少承载形式

§2 允许一个 Harness 的 arguments 合计16KiB、executable4096字节、name80标量；§9 HarnessSummary 原样返回这些字段，每页最多32项。即使不计 JSON 转义，一个合法列表页即可超过32KiB；含大量需要 JSON 转义的合法参数时，单项本身也可能超过32KiB。workspace 的32个4096字节路径同样超限。§9 超预算 projectionRef 分支明确仅覆盖 snapshot/history，list 没有引用形式或独立详情读取动作。单纯减少页项数不能解决单项超帧，客户端裁掉参数或节点超限发送均违反现有合同。

最小修正：为全部 list 定义完整、可校验且绑定原捕获版本/游标的引用或明确详情引用形式；逐字段给出结果形状与引用读取方式，不把分页 cursor 当字节续块。可复用 projection manifest/内容读取机制，但 harness/workspace 列表没有 sessionId，必须明确认证范围与 content.read 当前必填 sessionId 如何对应，不能硬造会话或引入未认证旁路。另一可行做法是列表只给固定小元数据并定义受认证的配置详情读取；二者择一写实。保留合法输入预算，不静默裁剪或降级。tester 应覆盖单项转义后超帧及多项总和超帧。

### W3R-005 — 阻断 Harness 能力视图接线：必要能力/版本投影缺失

§9 HarnessSummary 只有定义ID/名称/入口/参数/configRef；SessionState 也没有协议或能力验证视图。harness-management Requirement“登记可追溯的 Harness”要求保存并呈现版本、适配器版本及已验证/未验证/不支持能力，且版本兼容 Requirement 要求呈现实际协议版本及验证状态。当前封闭 action 只有 harness.list/register，没有另一个可以取这些信息的详情接口，前端无法按合同显示真实状态，只能自行猜测或省略。

最小修正：明确受认证可读的 Harness/连接能力投影，包含程序/适配器版本、实际协商协议版本（尚未协商明确 null）和所需能力的状态及固定错误/限制依据；区分广告支持与真实验证，不能把 initialize capability=true 自动标已验证。状态应绑定实际程序/配置或连接证据范围，不以名称认领旧证据。可通过列表固定小摘要与详情引用组成，不要求直接把全部结构塞入32KiB。该项落实既有规格，无需新产品授权。

### W3R-006 — 接线前修正：unsubscribe 精确字段集合有冲突

§5 写“取消用同字段 type=unsubscribe”，意味着 afterRevision/pageSize 亦在形状内；新 §9 精确写 unsubscribe 只含 v/type/requestId/resource/sessionId。§2 又要求未知字段拒绝。两端严格实现不同段落会互相拒绝。

最小修正：在 §5 直接改为引用 §9 的精确 unsubscribe 形状，说明不携带 afterRevision/pageSize，成功结果及后续排队边界按 §9；不能仅让客户端猜新段落是否隐式取代旧段落。无需更改订阅成功先 result→baseline→增量的顺序。

### 已可用与非阻断实现约束

- Item/Turn 保留原始结构、源位置、类型与版本；baseline 区分 committed turns/liveItems 并按相同 itemId 较高版本安装，未把状态快照当终态；正常保存版本不进入文案。这些方向与 §5/spec 一致。
- snapshot/history 的引用分支与 inline 分支排他，manifest part 是逻辑页内部承载，不是下一逻辑页；同轮多个连续片段合并而非切成多页，pageSize/所选集合保持不变，未发现静默缩页。
- manifest 不嵌套、对象16MiB、数组完整校验后安装且无法容纳明确失败，预算在有限对象范围内可实现。实现还应校验 projectionType/head 的 snapshotId/revision/liveRevision/pageSize 与外层一致、part field 为该类型允许字段、同轮片段 ordinal/位置一致及重复/漏段；不能仅因各块 SHA 正确就接受结构拼错。建议 tester 加入这些完整性分支；这是执行现有“不混快照/不完整不安装”要求，不另设产品能力。
- prompt 输入保留标准 ContentBlock 原结构并由已选策略校验能力，不把未知块压为文本；最小 Web text 块有明确形状。多模态编码应引用 WP3 已固定版本 schema，不由 UI 猜测；未支持的类型显式失败。
- §9 result→baseline→增量顺序可以与 §5 原子捕获兼容：注册/捕获/成功结果和基线入队应由同一订阅协调操作排序，后续变化不得插队。慢速 content.read 期间客户端可有界等待/重新取基线，不能安装部分旧基线后悄悄覆盖更新。当前没有产品执行证据。

本轮只追加报告，未修改合同/产品/用例。W3R-004–006 修订后由本 reviewer 原 ID 复核，独立 tester 应维护对应断言；不把文档审查当 CL-030/031 已通过。

## 同日追加：节点登记详情引用及 W3R-004–006 复查

实际读取合同新 §9 与 §2/3/5/8，目标 SHA256=`8525D119622A02EF3C397E2A8310051D1A8942548D043A5E39327007206323BB`。未改产品/合同/用例。

- **W3R-004 Closed（设计合同）**：Harness/Workspace 列表采用短 summary 和 detailsRef，按完整 UTF-8 帧预算选至多32项，不裁字段、容不下一项明确失败；长配置通过标准分块读取，解决单项/多项超帧。`sessionId:null` 仅可解析节点签发的登记详情/能力元数据引用，绑定 actor/认证代数/快照/60分钟租约并逐块 gate 复核；会话正文必须非null会话ID，input上传不接受null，禁止任意路径签发。该限定是节点登记元数据可读范围的编码细化，不把 null 当权限通配符，没有发现新增未认证旁路。实现应按引用记录的内部类型先匹配 node-metadata/session-content，再复核身份与权限，不可只凭请求 sessionId 是否 null 切换授权。需要 tester 验证将会话引用改传null、节点引用带伪会话、另一actor/旧代数/过期租约及任意路径均不能越权。
- **W3R-006 Closed（设计合同）**：§5 已明确引用 §9 精确 unsubscribe 字段并排除 afterRevision/pageSize；成功顺序、停止入队与已交网络栈边界一致。
- **W3R-005 核心缺口已补，保留一项类型修正待关闭**：HarnessDetails 已定义程序/适配器/协商协议版本及逐能力 unverified/verified/unsupported/incompatible；广告原始事实保留引用，未协商明确null，不把注册或广告支持当实测。与既有 spec 的能力可见要求一致。但该段写“三个版本字段为已证实字符串或null”，将 protocolVersion 也变成字符串；§8 与已核验 ACP wire 使用整数1/2，§9 开头又说版本为安全整数，ItemProjection 同名 protocolVersion 没有另定义字符串转换。两端仍可能分别实现1与"1"而无法严格解码。**最小修正：programVersion/adapterVersion 为字符串或null；HarnessDetails.protocolVersion 为已确认正安全整数或null，ItemProjection.protocolVersion 同一整数类型。将开头“版本”缩窄为 revision/liveRevision/contentVersion/committedVersion 等计数版本，避免把程序版本误读整数。** 这是字段编码澄清，不需新产品决策。

其他新增的同 item/version 不同 contentRef 按校验字节比较、committedVersion 单独推进说明，消除了把租约引用当正文身份的风险，符合 §5。本轮仍不签 Web 接线整体通过；W3R-005 上述小修正及独立 tester 新断言待复核，其余产品/平台验收状态不变。

## 同日追加：W3R-005 关闭与 §9 整体设计结论

实际读取修订后的 §9 类型行，合同 SHA256=`C51B9AB5015D80BA36735C9E0C056E2BC9A93C0DCF9A324D39D51D5560928CAB`。计数版本与程序版本已区分：programVersion/adapterVersion 为已证实字符串或null；HarnessDetails.protocolVersion 为正安全整数或null；ItemProjection.protocolVersion 亦为正安全整数，并明确 ACP1/2、平台输入1。**W3R-005 Closed（设计合同）。** W3R-004/006 的关闭保持，未发现该修正引入其他冲突。

**§9 整体设计定义 PASS。** 当前已明确列表短摘要/详情承载、能力与版本投影、state/item/turn/approval、内联与引用排他分支、不可变manifest/同轮续片、结构输入以及订阅结果→基线→增量顺序，结合 §2/3/5 具有可实现的预算和权限边界。该结论仅解除所核查的设计缺口，不要求增加产品授权，也不签实际实现/真实传输/安全验收通过。

待独立 tester 补并交 reviewer 核读的 §9 用例仍保留：单项转义/多项超帧、详情和节点元数据引用类型/身份/租约/撤销越权拒绝、版本字段严格类型、能力广告与验证事实区分、manifest头/片段一致性、同item/version换引用不误报冲突以及unsubscribe精确字段和订阅原子顺序。本次未改 tests 或产品；未执行这些新增断言。WP2依赖交接及各实际包冻结/验收状态按主执行和对应独立报告处理，不由本合同复核代签。

## 同日追加：§9 live 承载澄清

实际读取当前合同 SHA256=`CBF1BFE389CA3824884918C0BA9147B117C472359D396A7A132F150204B7D071` 的 §9。**设计定义 PASS，未发现新增阻断。** snapshot.items 合并所选提交条目与当前 Run 全部 live 条目，包含尚未提交的新 item；baseline 保留 turns/liveItems 两数组后按相同 itemId/较高版本安装；history 仅对逻辑页所选轮次合入捕获时 live，不插入页外当前 Run，不把捕获后新输出拼入冻结页。三种承载有明确用途，不把快照实时尾部规则误套为历史旧页扩页，也不因无后续增量遗漏当前尾部。§5 的稳定边界、原子捕获和 live/committed 分离保持成立。tester 新增断言仍待独立核读，未执行产品场景。

## 同日追加：§9 用例前置及 WP3 隔离拆分短复核

实际重算 tests=`1CEB400EC05147BAEDD1DBF5E02DF371A8B61684FE85A0AF9AB3E3A1A6DFB717`，合同仍=`CBF1BFE389CA3824884918C0BA9147B117C472359D396A7A132F150204B7D071`；读取 CL-005/024/027/031/042 新步骤与预期。**§9 设计/用例定义前置 PASS。** 新断言覆盖短列表/详情预算、能力与版本类型、node metadata null范围/认证代数/租约/撤销、manifest与逻辑页分离、输入标准块、引用变化按字节校验以及订阅顺序；CL-027明确旧页不插当前Run、冻结后不拼新输出，CL-042分别核验snapshot合并items与baseline两数组安装，包含全新未提交item和静止尾部。未发现用例与live澄清冲突；只是定义前置，所有产品执行仍未由本次验证。

读取 plan Parallel Assessment，SHA256=`25AA37C44D69C347F532B8DF459EA70206575B6A74D9F0C2A3406BE31C6EE808`。协议包限公共AsyncRead/AsyncWrite与策略、不启动进程；平台包限platform/与manifest现有windows-sys JobObjects feature、DEPENDENCIES，无会话数据库；共享lib/node/policy/trust最后串行装配，写owner不冲突。WP2独立验收及主D检查前不放行写入，当前2.3外部probe进行中不能提前开始。现有windows-sys0.61.2已锁定，单独增加JobObjects feature无需虚构新crate审查，但仍需记录新增API必要性、unsafe/句柄/继承/Job归属与Windows行为检查；核验锁和解析依赖实际没有新增/升级，而不是仅看清单名字。原依赖MIT/Apache-2.0及项目维护依据可按同版本范围复用。

### W3R-007 — 平台独立交付检查前须补：实际编译/测试入口未安排

plan规定平台仅写platform/、Cargo.toml仅加feature，platform/mod.rs内声明模块，lib声明由后续串行owner处理；同时要求平台独立冻结/检查后才合入。当前Rust crate未引用新增platform根模块时，常规cargo fmt/clippy/test不会编译这些源文件，仅在platform/mod.rs里声明子模块不能使其成为crate可达代码。协议新增根模块也需同样关注，不能以无目标检查的exit0充当新包编译通过。

最小修正：在拆分写范围中指定独占的实际编译测试入口，例如平台独有 `app/tests/platform_owner.rs` 通过明确path引用平台根，或临时独立验证harness（不进入产品、显式版本/依赖/命令及清理所有者）。选择后写明真实Cargo目标/命令，证明Job启动/stdio/shutdown接口确被编译并能运行Windows测试；协议包也登记其入口。若选择共享lib临时声明则必须改成串行owner交接，不能让两包并写lib。**本项阻断“独立检查完成”的声明，不阻断在既有开工门禁满足后开始非共享实现；交付检查前必须解决。** 该缺口不要求新产品/安全决策，只需可执行验证安排。

有所有权的异步stdio与shutdown/Job回收结果作为端口方向可实现，具体Rust类型/取消与句柄借用期限应在两包开工交接时写明；不能把取走stdio理解为转移或丢掉Job清理责任。平台独立测试应含创建前Job归属、失败清理、系统路径孙进程、父先退出、超时和Drop，最终共享实例隔离/安全接线仍由集成验收处理。本次未改产品/plan/tests，未运行新平台代码。

## 同日追加：W3R-007 入口复查关闭

实际读取主D `app/src/lib.rs` 与现有 `app/src/platform/mod.rs`：lib已存在 `pub mod platform`，平台根并非未引用新目录。此前 W3R-007 对现有平台入口的判断有误，本节明确纠正，不将原错误前提继续作为阻断。

plan新“实际编译入口”段已指定平台在既有mod加入cfg(windows) process_owner声明、模块测试及独占app/tests/platform_owner.rs；协议由同一装配owner在lib声明adapters及adapters/mod.rs，参数化测试随模块编译。它们均能成为实际Cargo目标，不需要两包并写lib；平台manifest/模块独占与既有交接顺序没有冲突。Windows真实owner/独立target、检查确实覆盖新入口的要求已写实。

**W3R-007 Closed；WP3隔离拆分的计划/入口定义 PASS。** 固定plan SHA256=`F4A18CB042AD91E75ACED17920946718C2ED3528BDD795518B2372C6D42C31E8`。实际新模块和测试尚未由本reviewer编译/运行，不据计划关闭签平台能力通过；WP2独立验收及主D集成检查等开工依赖继续保留。

## 同日追加：§10 catalog/生命周期/通用ledger 编码前预核查

实际读取新§10及相关§2/3/4/7/9，合同SHA256=`F4E8BDD72935DF71A7983E04EFE1A3522909533DDF90A6FE1AD532E94C47258C`；plan=`D304FAE6442210D9341CE6536DCCCC8C5B2BCA59946A2903BB8B47011AD4574D`。仅文档只读核查，不签新实现。该节补现有动作存储端口，未引入新Web动作/私有协议；共用actor/request全局去重、容量、原子Command/目标/审计/revision，session create原子creating、原生绑定及生命周期Command/expected state/bindingVersion CAS、失败/未知不重派、release失败不released、resume原ID不替代以及重启未知/审批来源失效均符合已有产品约束。明确内部owner并非ActorContext，实际安全接线继续§7。schema1保留/严格验证/原子迁移不清早期草案库，未新增权限授权。

plan隔离catalog owner与WP3 adapters/platform目录无重叠，schema/ledger及原input/approval兼容调整归该单一owner，主D不并行写产品；合入后协议工作区单次同步、重跑WP2检查，可执行。实际实现前还需独立tester核对新断言，不把沿用CL编号当覆盖已完成。

### W3R-008 — 编码前小范围补实：非会话Command结果引用未落入可读类型集合

§10要求登记Command允许session_id=null，command.get返回resultRef，引用正文包含动作/目标/已提交结果，并说“作用范围遵守§3/9”。但§9当前明确null **仅适用于节点签发登记详情/能力元数据**，会话内容要求对应非null ID。harness.register/workspace.register的Command结果是动作执行事实，并非HarnessDetails/WorkspaceDetails或能力元数据，不能凭现有类型假定它能走null读取；也不应创建假会话或让所有命令结果进入通配node范围。

最小修正：明确节点范围引用类型集合加入受限“登记Command结果元数据”，绑定该Command的actor、认证代数、动作/目标和只读租约；会话Command结果仍绑定真实session并复核Command可访问性。节点类/会话类不得互换，结果不含正文/凭据/恢复秘密，也不借此增加任意路径读取。同步说明§3会话限定的这个受限类型例外，并让独立tester覆盖跨actor/null伪装/错会话/过期/撤销拒绝。**本项阻断§10完整端口开工前置，修正和独立用例核读后即可复查；不要求新产品决策。** 其他§10生命周期/原子性未发现需额外决策的阻断。

## 同日追加：W3R-008 合同复查关闭

实际读取§3/9/10修订，合同SHA256=`828A97CB8C2F2765CD5D116C2777D53A456CA7C5775E6B2DB5C57FC28DC19738`。§3已引用受限节点只读例外，§9明确null允许§10限定登记Command结果；§10指定catalog_command_result仅harness.register/workspace.register，仅由command.get为当前actor自身Command签发，绑定代数/Command/60分钟租约且逐块gate复核。正文限定command/action/目标/status与已提交ID/revision结果，不带定义参数/正文/凭据，其他会话Command结果必须真实session范围，禁止通配或任意文件。**W3R-008 Closed（合同缺口），§10整体设计定义PASS。** 未引入新产品行为或权限决策。独立tester新增§10断言及reviewer用例核读尚待完成，因此本节不单独签catalog完整开工门槛或实现验收。

## 同日追加：§10 新用例定义预核查

实际重算tests=`9D75A416F00960944DFDDC1C03BDD8DA4AAF4C70540DA7801DB6E221584EED9D`，合同仍828A97CB...。读取CL-004/005/010/011/017/020/022/031追加步骤/预期：已明确全部动作共享actor/request和容量、typed catalog目标不造session、定义/Command/audit/node revision同事务、creating/resuming原生确认前不loaded、owner一次认领/Command状态bindingVersion CAS、释放失败责任、取消不提前终态和受限catalog_command_result null范围。与§10相符，未新增产品授权，也未将用例定义当运行证据。

### W3R-009 — 定义前置尚缺一项：生命周期操作重启窗口没有明确断言

§10要求所有未完成原生操作重启为中断/未知、旧owner/审批来源失效、不自动重试。当前CL-004原有重启步骤明确针对输入/Run；新追加create原子/CAS故障与CL-022注入成功/失败/未知回报，没有明确在create/cancel/release/resume受理/一次认领后或原生结果落库前终止节点并重启。现有输入重启用例不能自动证明新的通用ledger和生命周期分支。

最小补充：在CL-004或022列明这些动作在accepted、一次认领后、原生结果提交前重启，核对Command/lifecycle/旧绑定及审批来源；未完成保留中断/未知，旧claim/迟到回报不得推进当前绑定，派发计数不增加、无自动补发，未知原输入事实不改成功。独立tester维护该一组步骤/预期即可，不增case ID、不要求新产品行为。**本项补齐并复核前，catalog完整编码前用例门槛仍未解除；W3R-008合同关闭保持。** 其他已核定义没有发现阻断。

## 同日追加：W3R-009 复查关闭，catalog 编码前定义门槛通过

实际重算tests SHA256=`5FD9D9975EB1CB0C29694339489705C1ECA474AE6E3D5FC9DAE8C9759B3275F7`，读取CL-004新增四动作重启矩阵及预期。session.create/cancel/release/resume分别在accepted后、一次owner claim后、原生结果产生未持久化前终止/重启，明确比对Command/lifecycle/bindingVersion/原生ID/审批来源及原生派发计数，并尝试旧claim迟到回报和原请求重试；预期未完成保留中断/未知、不自动重派、原绑定及未知事实不伪造成功。与合同828A97CB... §10一致，补齐上一节缺口。

**W3R-009 Closed；§10 catalog 的设计/用例定义前置PASS，可在plan限定独立owner与写范围内开始实现。** W3R-008关闭保持，不新增产品行为或权限授权。该门槛只允许编码，不把新产品断言not-run改成通过，不预签迁移、owner安全接线或最终交付；实现后的独立测试/审查及集成检查仍全部保留。
