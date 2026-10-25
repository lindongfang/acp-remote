# Design

## Context

动机、产品范围与成功判据见 [proposal](proposal.md)；行为以三份 delta spec 为准。以下是已读代码后的事实，不是已实现的新能力：

| 接缝 | 当前事实 | 本次必须补齐的边界 |
| --- | --- | --- |
| Web 新建 | `ClientStore.submitCreateSession` 立即派发 `session.create`，成功后导航 | 准备先取得最终 ID，首条才保存，始终同页 |
| ID | core 的 ID 端口分配；旧 create 不接受客户端 sessionId | 保留后端权威，不能改成相信 URL 中的任意 UUID |
| Agent 选项 | initialize 没有模式/配置；真实 `session/new` 才返回 | 必须允许临时真实端点，不能伪造预配置 |
| prompt 受理 | broker 保存命令、任务、事件，完整输入只进内存队列；历史正文依赖 Agent 回显 | 首条正文、附件引用与派发阶段需要新持久接缝 |
| 附件 | 正式上传与清单表引用已存在 owned_session | 首条点击后的暂存不能用假空会话规避外键 |
| 浏览器 | 文字/附件/路径草稿当前仅内存；摘要缓存不接受正文 | 增加独立准备草稿端口，不能放宽 imported 缓存边界 |
| 激活 | 在途变更 `session-activation-lifecycle` 提供容量、代隔离、查看租约 | 作为前置基线使用，临时资源计入同一容量账 |

当前工作区包含大量未提交实现；在途生命周期变更尚有未完成验收。实现前须固定本轮基线，不将这些内容归为本变更的改动，也不提前归档上游变更。

## Goals / Non-Goals

**Goals:** 后端提前分配同一个 ID；ACP Remote 无准备期持久记录；真实设置可调整；标签页草稿能刷新恢复；首条内容先落盘且故障后不丢失、不重复执行；类型、认证、端口和平台边界均显式。

**Non-Goals:** 不改旧 `session.create`、Zed、Node Link 创建；不改旧会话的恢复判据；不把 imported 数据纳入本机准备；不让 app 承载业务；不提供“任何 Agent 永远可恢复”的保证。Agent 自己的 `session/new` 是否写盘不由 ACP Remote 统一保证。已正式派发过用户消息的会话仍只能恢复原 Agent 身份。

## Decisions

### D1. 三个事实分别建模，不能仅靠恢复列为空猜测

区分浏览器准备上下文、Owner 运行期准备资源、正式 owned 会话。准备资源仅内存；正式记录只有首次提交事务才能产生。准备阶段的 Agent 身份是可丢弃的临时资源，不是持久会话的恢复承诺。

正式会话增加**仅新首条路径使用**的初始提交记录，明确 `saved_not_dispatched / dispatch_started / terminal`，并记录是否曾跨越派发边界。它是“正式但尚未首次派发”的唯一证据；不得从 `agent_session_id = NULL`、正文为空、无任务或历史错误推断可以新建。旧会话绝不生成这类记录或补迁移。

准备资源模型：`allocated → preparing → ready → released`，失败是带原因的状态；每次重建分配新的运行期资源代。首次提交冻结当前资源代并转移所有权，避免 lease 到期与提交并发释放正要派发的端点。准备完成、设置修改或释放仅回复发起者，不广播正式会话事件；正式 owned 事件仍先持久化再发布。

### D2. 使用节点签发的准备凭据证明 ID 来源，不持久登记预留

仅内存预留在重启后无法证明浏览器传回的 ID 由 Owner 分配；只信任客户端 UUID 又违反 ID 分配权威。选择由 core 分配 ID，节点签发**准备凭据**，浏览器随准备草稿保存。凭据不是 bearer 登录凭证，不替代每条新 WSS 的认证，不授予 scope。

凭据的 v1 claims 固定绑定：格式版本、ownerNodeId、持久 storeEpoch、canonical origin、deviceId、tabNonce、sessionId、agentId、规范化工作目录与目录来源。storeEpoch 取当前持久 `meta.server_epoch`（现状首次建库生成、正常重启不变），不是进程 epoch；整体重建存储时改变，旧凭据因此失效。`tabNonce` 为每标签页的随机准备归属，不进入 URL；用户改变 Agent/目录必须新 ID/新凭据，不能改 claims。凭据不含文字、附件、Provider/MCP 凭据或任意授权集合。模式/配置可以变化，不放入固定 claims，而在当前准备资源代上验证。

通过现有节点 P-256 身份密钥签名；新增域分离长度前缀 transcript domain `session-preparation`，字段/tag 在 transcripts manifest 唯一登记，使用现有 P1363 64 字节签名及无填充 base64url。不得签普通 JSON，不得直接开放任意字节签名 API。identity-auth 增加纯类型签发/校验入口；私钥仍只经已有 keystore 端口使用。协议 codec 与纯 core 值对象在 adapter 映射，app 仅装配。

重建前先检查当前连接、撤销、所属节点/origin、签名/全部绑定字段；随后先查同 ID 的 owned 首条关联记录：若已正式受理，返回 D3 的 `committed`，不重新准备、不要求仍有临时端点，也不因其旧目录后来失效而阻止读取正式提交状态。只有尚未正式受理的准备才继续复校验最新 Agent/工作目录并重建同 ID 的临时资源。资源代绑定节点运行期的准备实例，不绑定某一条业务连接；每条连接分别认证，操作去重窗口绑定当前连接。密钥/节点身份变化走现有身份变化阻断，不能静默重签成另一个 ID。

主动离开清理浏览器凭据；运行期保留有界释放事实以拒绝旧代操作。没有准备持久墓碑时，不能宣称凭据已在节点重启后被全球永久吊销；其使用始终还要求同一未撤销设备的完整认证。对正式 ID 的重放由持久首条关联记录和唯一约束挡住，不重新创建。本轮不新增正式会话删除或独立清理其首条关联的路径，不增加 `consumed` 状态和 ID 消费墓碑表；未来若加入该清理能力，须另行设计凭据失效机制。不能用该凭据读取其他设备草稿、访问任意目录或恢复旧会话。

### D3. 新增协商能力与临时控制面，保留旧命令含义

拟登记 feature `core.session-preparation.v1`；依赖既有本机目录、命令状态、附件和生命周期实际支持，不能只因 schema 存在而宣告。控制帧族拟为 `session.preparation` / `session.preparation.result`，闭合 action 为 `allocate / restore / configure / renew / release`；分配输入仅 Agent/目录与标签页归属。restore必带receipt，允许携带knownRuntimeEpoch/knownGeneration作为旧状态提示，不能要求旧代与当前代匹配，否则无法跨重启恢复；configure/renew/release携带receipt与待操作资源代。操作 ID 用于连接期有界去重；更改同 operationId 的参数必须拒绝。

这是**临时控制面**，不伪装成必须有持久 terminal event 的 mutation，也不把 Agent 建立副作用包装成无副作用查询。结果是以下闭合分支，临时失败不写 owned_command 或全局事件。限制复用已登记的单帧字节预算、业务频率上限；准备操作串行到会话，临时资源计入节点现有 max_active_sessions，固定租期 30 秒、可见时 10 秒续租。WP1 将本文已定语义编码到机器表、schema、fixture和文档，不由实现者另造分支。

#### D3.1 准备结果与客户端动作

所有 `session.preparation.result.body` 都有 `operationId: UUID`、`action`（回显请求action）、`sessionId: UUID`、`status`；按status做 closed object 的判别联合。公共字段之外各分支必填/禁止字段如下，不能用多个可空字段拼出非法组合：

| status | 分支字段 | 合法来源与客户端动作 |
| --- | --- | --- |
| `ready` | `receipt`、`runtimeEpoch: UUID`、`generation: 正十进制字符串`、`modes`、`configOptions`、`leaseExpiresAt` | allocate/restore/configure/renew成功；展示已确认的真实设置。receipt为D2的闭合claims+signature对象；未提供的Agent选项以协议对应的明确缺席/空值呈现，不伪造 |
| `committed` | `firstSubmissionId: UUID`、`firstRequestId: UUID`、`currentRequestId: UUID`、`firstMessageId: UUID` | restore/configure/renew/release与首次提交竞争、或首条响应丢失后发现已受理；不携带临时代/lease/配置/receipt。保持原URL，用currentRequestId查command.status并session.read，不重建准备、重传附件或重新start |
| `stale` | `reason`（`runtime_epoch_mismatch / generation_mismatch / released`） | 通过认证和凭据核实后发现资源代已失效；不宣告已有正式提交，不自行生成新ID。先核实原start，再用receipt执行restore，然后按当前代重试（restore对有效receipt返回ready） |
| `released` | 无额外字段 | release在首条未受理的准备上成功或幂等成功；只说明临时资源释放，不表示ID已正式提交 |
| `failed` | `error: {code,message,retryable,details}` | 真正的认证后业务失败；使用D6错误映射及既有容量/目录错误，不能用该分支隐含表达已提交。receipt 的 storeEpoch 与当前不一致时用新增不可重试码 `preparation.store_rebuilt`（`details:{}`） |

`stale` 的**唯一**来源是 `configure / renew / release` 与附件 begin/manifest；`restore` 对有效 receipt **永不**返回 `stale`：资源可重建时返回 `ready`（已受理则 `committed`），目录/Agent 不可解析等业务失败走 `failed`；`allocate` 也不返回 `stale`。receipt 的 storeEpoch 与当前持久 storeEpoch 不一致不归入 `stale`：它说明旧存储已整体被替换，统一返回 `failed` + `preparation.store_rebuilt`（不可重试），客户端丢弃 receipt、认证后回目录重新 allocate。restore 携带的 `knownRuntimeEpoch/knownGeneration` 只用于旧状态提示与操作去重，不参与拒绝判定；否则正常重启后会形成 `stale → restore → stale` 的不可恢复环。

`generation` 取1..2^63−1，按现有序号编码为无前导零十进制字符串。status=committed不是命令completed：首条可能仍accepted、failed或uncertain，必须查询currentRequestId。新Web同页转换正式模型，确认完整内容已保存后才清浏览器草稿。正式会话及首条关联记录不因正文TTL一起清除；本轮不定义删除后的恢复结果。

判断优先级固定为：当前连接认证/撤销 → 凭据签名与节点/origin/storeEpoch/设备/ID/标签页claims绑定 → 同ID首条持久受理查询 → 仅未受理时检查runtimeEpoch/generation/lease与目录/Agent现状 → 执行动作。受理与restore/release并发在同会话串行门及提交重查下线性化：恢复先完成可返回ready，提交先完成必须返回committed；任何ready都不能越过session.start自身的事务复校验。

首条使用新增 Sync-only mutation `session.start`，命令名仍只在 commands manifest 定义。envelope.sessionId 必须是已签发 ID；payload 是闭合判别联合：`kind=initial` 携带准备凭据、完整用户输入或完整附件清单、模式与配置选择，`kind=continue` 的合同见 D8.1（新请求 ID 与 `retryOfRequestId`，不带正文、附件清单或 receipt）；两者都不能携带 Provider/MCP 凭据。它不透传 Node Link，不改变 ACP `session/new` 或旧 `session.create`。`accepted` 只在 D6 事务提交后发出；最终命令结果沿用持久终态规则，不能用准备控制结果代替业务受理。

重建、configure、附件控制都先认证再验准备归属。旧客户端未协商不得收到新帧/字段；新客户端连旧服务端明确禁用新建准备，不自动 fallback。现有会话仍用原读取、配置和 prompt 路径，不能因看到旧 NULL 恢复列就走 start。

### D4. 提交前设置和只读上下文保持真实

准备通过已有 backend create 接缝取得临时端点；模式/配置操作只作用该资源代，成功值以 Agent 确认为准。重建后先读取当前选项，再按草稿所选值验证、应用，全部确认才允许首条；失效选择保留在 UI 并阻止发送。不能以初始化缓存、默认值或 `localActivity` 冒充 ready。准备期间不允许发送 prompt，也不能为了持久化临时资源偷偷发送空消息。

准备期所需的工作目录路径引用与已有只读文件交互经受认证的准备上下文解析，复用 core 的路径与文件用例端口；server 入站 adapter 不相互调用，也不把未持久 ID 塞给正式会话查询。准备读取不产生附件或正文持久缓存，根外路径和逃逸链接照旧拒绝。若某功能在准备期确实无法安全提供，必须如实呈现不可用原因，不能把准备当作完整正式会话。

### D5. 浏览器准备草稿是专用存储，不是扩大的会话缓存

新增 platform `PreparationDraftPort`，由 feature 状态机调用，页面不直接访问浏览器数据库。准备草稿分区键为 canonical origin + ownerNodeId + deviceId + tabSlotId + sessionId；包括文字、精确配置值、路径引用、附件 Blob/分块内容、准备凭据和未确认提交的 requestId。私钥、配对材料、Provider/MCP 凭据不进入它。正式正文、imported 资源与普通已建会话草稿不得顺带持久化；摘要缓存保持原白名单。

采用 sessionStorage 保存非敏感标签页定位信息，IndexedDB 专库保存完整草稿/Blob；不能把大文件 base64 拼进 sessionStorage，也不能保存 Blob URL 冒充文件。流式保存与恢复受内存工作集约束；不设附件产品大小/数量上限，配额不足明确逐项反馈，不报告完整成功。新保存版本完成前旧完整版本保持可用，不能部分覆盖附件清单。

正常刷新凭同标签页 locator 恢复；显式应用导航、换 Agent/目录先逻辑作废旧草稿再清内容与资源。配置/附件异步写带草稿版本，迟到写不得复活已删除内容。后台 visibility 只更新资源续租，不删除草稿。浏览器 pagehide/unload 不能可靠区分刷新/关闭，不能只靠这些事件决定删除；结合路由意图、导航类型和文档所有权，在下一次启动判定是否为同页 reload，非 reload 的陌生导航不得凭数据库残留恢复。

通过标签页所有权握手识别复制 sessionStorage 的 duplicate/opener 页面；新标签不得直接继承旧准备凭据的活动归属，陌生 URL 查不存在后回 `/`。Q8 保证草稿独立，不改变现有同设备连接顶替规则。异常关闭不承诺浏览器一定执行清理回调；残留逻辑不可恢复，显式离开记录清理墓碑并以 24 小时孤儿清理窗口回收。孤儿清理不能只因后台无心跳就删除仍由活标签页持有的草稿；实现验收须覆盖冻结标签页与 BFCache。

首次 `accepted` 确认且权威读取验证完整消息后，准备草稿转为只含未终结请求关联的恢复记录，删除浏览器正文/Blob；命令终结后删恢复记录。该窄例外须在 FRONTEND/SECURITY 中明确，不混入一般远程命令持久缓存。保存响应丢失时保留完整草稿并先查原请求，查询到已接受才做清理。

### D6. 附件先暂存，再与首条消息一次转正

采用方案 A：复用 `wss://<host>/sync/v1/attachments`、现有 `acp-remote.sync.v1.json` 子协议及附件帧，不新增准备专用上传端点/帧族。点击发送之前只写浏览器；点击后才传文件。旧正式上传body分支及外键约束保持原样，新准备分支不得先插空会话绕过约束。

#### D6.1 协商、帧与字段

附件连接自身必须完整执行challenge-response，并同时协商 `core.attachment-transfer.v1` 和 `core.session-preparation.v1` 才能使用准备分支。业务连接的selectedFeatures、connectionId或已认证状态不能替代附件连接的认证；仅有旧附件feature仍可按原格式上传正式会话附件。未协商准备feature却提交准备分支，回 `protocol.feature_required`（details.features含缺失feature），不触碰临时或正式存储；若连基础附件feature都未协商，沿用现有 `attachment.negotiation_required` + 4400关闭规则。

新增闭合类型 `PreparationAttachmentTarget = {kind:"preparation",sessionId:UUID,receipt:PreparationReceipt,runtimeEpoch:UUID,generation:PositiveDecimalString}`。receipt是D2完整claims+signature对象，不是业务connectionId或登录token。runtimeEpoch为本节点当前准备运行期UUID，正常进程重启改变；generation取1..2^63−1且按十进制字符串编码。签名receipt本身跨正常重启有效，target中的运行期字段必须匹配当前live准备。

| 复用帧 | 准备分支body精确定义/行为 |
| --- | --- |
| `attachment.begin` | `{transferId:UUID,target:PreparationAttachmentTarget,subject:{kind:"device",id:UUID},totalLength:DecimalString,chunkSize:32768,mediaType:string,fileName:string}`；除target外复用旧字段类型、长度与取值限制。旧分支仍用顶层sessionId；两分支oneOf，禁止同时有sessionId和target，禁止target中出现formal类型绕过旧分支 |
| `attachment.chunk` / `attachment.chunk_ack` | body不变：transferId、offset/data或确认字段。每块不重复附带大receipt；服务端必须从已核验的运行期传输记录恢复完整target并重新校验身份/归属/资源状态 |
| `attachment.seal` / `attachment.sealed` | body不变；封存校验长度和流式SHA-256，sealed返回的attachmentId尚未成为正式消息引用，只能被同target的准备清单使用 |
| `attachment.status` / `attachment.abort` | body不变；只允许当前附件连接已重绑定的传输。返回receiving/sealed/aborted等原状态不等于session.start已受理 |
| `attachment.manifest_begin` | `{submissionId:UUID,target:PreparationAttachmentTarget,subject:{kind:"device",id:UUID},attachmentCount:DecimalString}`；旧分支仍用顶层sessionId，同样oneOf互斥 |
| `attachment.manifest_page` / `attachment.manifest_sealed` | 原body与顺序摘要算法不变，所有transferId必须已封存在同设备/同sessionId/同tabNonce/同runtimeEpoch/generation准备下。manifest_sealed只表明准备清单可被start认领 |
| `attachment.error` | 复用 `{code,message,retryable,transferId,details}`；仅协商准备feature时允许新增可选 `submissionId:UUID`，清单错误必带它且transferId为null；传输错误带transferId，无法安全识别标识时均省略可选ID/置transferId=null，不能回显未核实的资源详情 |
| `attachment.read` / `attachment.read_chunk` | 不扩展准备读取：准备预览使用浏览器原文件；尚未转正的attachmentId不能经正式读取接口读取，返回既有 `attachment.transfer_not_found`。转正后正常走正式会话读取 |

每块32768字节、每传输最多1块未确认、每设备2/每节点8活动传输、6000帧/60秒预算及最多64项清单页均沿用原合同。普通成功上传不额外逐块验签；第一次begin/清单begin和连接重绑验签，后续逐帧验证当前认证与不可变target绑定。新字段必须在WP1同步schema/共享正反fixture与Rust/TS，不允许实现者临时添加。

#### D6.2 跨连接归属、重绑与并发顺序

开始/重绑顺序固定：附件连接当前已认证且未撤销 → 双feature已协商 → closed-body形状及尺寸 → 从连接身份取得deviceId并与subject及receipt.deviceId相等 → 验证receipt签名和owner/origin/storeEpoch/sessionId/tabNonce/Agent/目录绑定 → 在同会话串行门下查首条持久受理 → 仅未受理时核对target runtimeEpoch/generation、lease及准备可上传状态 → 登记/恢复传输。准备证明永远不代替连接认证；subject是被校验的声明，不是身份来源。

运行期记录固定 `(transferId,authenticatedDeviceId,sessionId,tabNonce,runtimeEpoch,generation,immutableFileMetadata,boundAttachmentConnectionId)`，清单同理固定submissionId与有序transferIds。每个chunk/seal/status/abort/manifest_page再次检查连接身份/撤销、boundConnectionId、准备未正式受理、资源代与清单归属。不检查另一条业务连接是否恰好仍存活，也不通过其connectionId借用认证。

同target同transferId、相同不可变文件声明的重复begin，返回原 `attachment.status`，不重置偏移/已接收字节；相同ID不同声明返回binding_mismatch。附件连接断开释放它占用的活动传输预算，但保留节点仍存活的有界传输元数据与暂存内容至闲置清理。新附件连接重新完整认证，并重发带完整target的begin（同transferId）才可重绑；同设备但仅持transferId的chunk不能接管。重绑串行转移boundConnectionId，旧连接迟到帧被拒，不同时允许两个连接修改一个传输。manifest_begin相同声明对开放清单幂等接受，不新增进度回复帧；已封存则返回原manifest_sealed。重新发送原有页面按既有顺序/重复页规则幂等；不同声明拒绝。

节点重启丢失运行期记录：先restore查询是否 `committed`；若是就读取正式会话，不重传。若仍未受理则取得新的runtimeEpoch/generation，同一业务会话ID下生成新transferId/submissionId重传；旧代transferId返回找不到或过代错误，不从裸临时文件猜测进度。资源重建导致旧代清单失效，必须按当前代重新封存。所有这些情况都不能自动派发prompt。

seal/manifest封存与start认领在同session门下排序。start最后一次核验全部文件、主体、target、摘要、真实能力和配置后，冻结清单与字节直到提交完成；再原子提交会话/输入/附件引用。若start先提交，任何随后到达的准备上传操作返回already_committed，不再修改清单；若seal先结束，start可认领。受理成功不能与孤儿清理并发删除其字节。

#### D6.3 错误与客户端动作（本变更冻结）

认证后能够安全回复的附件业务拒绝走attachment.error；认证阶段、设备撤销或帧过大时遵守原认证/error/close规则，不为尚未认证的连接发送附件回复。准备控制面发生同类业务错误时走status=failed/error。新增码只在WP1登记于现有errors机器词表，不在代码散落定义。错误message不参与客户端分支判定，details只能有登记字段。

| 条件 | code / retryable / details | 客户端固定动作 |
| --- | --- | --- |
| 未认证或撤销 | 沿用 `auth.required / auth.device_revoked`、原连接关闭规则 | 停止传输；重新认证/撤销处理，不能拿receipt绕过 |
| 未协商feature | `protocol.feature_required` / false / `{features:[缺失feature]}`，或基础附件协商规则 | 禁用该能力，不降级成旧正式上传、不创建空会话 |
| 形状/字段/尺寸不合法 | `protocol.schema_invalid` 或 `protocol.message_too_large` / false / 原登记details | 修正输入；无存储副作用 |
| 伪造receipt、跨设备/会话/标签页、subject与连接不符、ID重用不同声明、未重绑连接、混用正式与准备资源 | `attachment.binding_mismatch` / false / `{}` | 保留浏览器输入，终止这次传输，不泄露目标存在性或具体哪个签名字段失败 |
| 核实归属后发现运行期或代失效/已释放 | 新增 `preparation.stale` / true / `{reason:"runtime_epoch_mismatch"或"generation_mismatch"或"released"}` | 先查原start，restore；只有仍未受理才取得新target重建上传，不能盲重发旧帧 |
| 核实归属后发现 receipt 的 storeEpoch 已因存储整体重建而失效 | 新增 `preparation.store_rebuilt` / false / `{}` | 不重放旧 receipt、不 restore；认证后回目录重新 allocate 新 ID，旧上传内容作废 |
| 归属有效但首条已正式受理 | 新增 `preparation.already_committed` / false / `{sessionId:UUID}` | 停止上传，restore取得committed关联，再同页读正式状态，不再次start |
| 同一live目标内transferId/submissionId不存在或闲置过期 | `attachment.transfer_not_found` / false / `{}` | 核实start未受理后创建新传输/清单，不认为消息已失败或重新创建会话 |
| 偏移不符/同偏移不同块/摘要不符/已封存 | 沿用 `attachment.offset_mismatch / attachment.chunk_conflict / attachment.digest_mismatch / attachment.sealed` 及原retryable/details | 仅按既有传输恢复规则处理，不改为业务提交或部分发送 |
| 预算不足/准备容量不足 | `attachment.budget_exhausted / session.capacity_exceeded` / true / 原登记details | 退避或等待准备资源，不中断其他受保护会话 |
| 磁盘不足/暂存不可读 | `internal.unavailable` / true / `{}` | 显示资源失败，保存前不受理、不派发；已受理则先查权威状态 |

失效判定必须在身份和签名绑定核实后，不能向陌生主体返回当前运行期/正式session信息。已核实且有首条记录时，committed优先于runtime过代、lease过期和目录失效；这一优先级同时适用于restore和附件开始/重绑。

#### D6.4 转正只认领，不再次上传或复制文件

暂存复用已有块/流控/哈希机制，经独立准备暂存端口（core 纯端口 `PreparationAttachmentStaging`，WP1 以 CORE_PORTS §5.11 的 `rust-planned` 块冻结，WP2 在 `storage-sqlite` 实现并测试）传入，不横向调用storage实现。该端口只维护运行期传输/清单元数据与 Owner 私有内容区中的字节，不写带 `owned_session` 外键的行；分块接收、流式摘要、封存后不可变、24 小时与启动回收、配额都复用现有预算，不在 server 另造一套。文件第一次上传即写入Owner私有、正式附件可引用的内容目录，封存后字节不可变；临时/正式区别在元数据与访问权，不是另一份文件。

所有附件封存后，`session.start`携带同准备ID、receipt、runtimeEpoch/generation、原PromptContentBlock有序content、准备submissionId与所选模式/配置；其中attachment_ref只能引用该清单已封存的attachmentId。受理事务创建正式会话、目录归属、完整首条输入、稳定请求/任务/阶段以及附件正式记录/引用。任一步失败全回滚、不派发，文件仍是未认领字节。正文与附件无需第二次上传，也不得先建正式会话再补齐附件。

普通单次成功流程每个文件只上传一次；封存使用上传时维护的流式摘要，转正只核验已封存事实并原子认领元数据，不为转正再次全量读/哈希或复制内容，也不发送逐文件第二轮数据。允许文件存在性/权限等必要复核。数据库事务不承诺文件系统跨系统原子性；不能将昂贵复制放入事务或会话门中。临时目录与正式内容目录必须在同一受管内容区，避免跨卷移动隐含全量复制。

受理失败的内容成为无引用孤儿，启动/24小时清理回收，活动传输和冻结待认领内容不误删。受理成功后即使尚未派发，也按已提交消息引用保留，不能按未提交上传TTL清除。旧正式会话附件表及协议不变。本方案不保证重启后未受理上传免于重传，但保证正常成功流程无额外全文件传输/复制；实测分别记录上传、封存和提交耗时，不虚构固定毫秒保证。

### D7. 扩展 owned 提交口，首条正文不能再依赖 Agent 回显

在 `core::model/ports` 增加纯类型的首条持久输入及事务写集，扩展已有 `SessionStore::commit_with_submission` 或同语义专用事务口，避免 UI/adapter 分几笔写。完整输入包含有序内容块、精确值和扩展容器，不把 ACP DTO 或 wire JSON 解析器引入 core；原文保真仍经协议规定的 rawJson 载体，领域视图不充当完整消息。

SQLite 增加 owned 初始提交记录（建议表名 `owned_first_submission`）：session_id 唯一、`first_submission_id` 唯一（服务端生成）、稳定 user_message_id、原始主体/请求、完整输入和摘要、已选配置、派发阶段及曾否派发、当前尝试 requestId/结果。WP1 在 CORE_PORTS 新增**顶层计划小节**（如 `## 12.`）用 `sql-planned` 冻结 DDL，不写进 §7——`check:drift` 会把 §7 的 sql 块与 `migrate.rs` 逐条比对，实现前写入即报红；WP2 以 v16 migration 落地后把块移入 §7、删除计划小节，并同步更新 `scripts/check-contract-drift.mjs` 的 v15 版本断言。准备附件暂存端口与首条事务口的计划形状同样放在 §5.11 的 `rust-planned` 块，不改 §5.2 `SessionStore` 与 §5.3 既有附件端口的方法集；WP2 实现后必须把同名 trait 移入常规 §5 块（drift 逐方法比对），否则文档会一直把已实现能力写成计划。首次受理的同一事务写入，供restore明确投影 `committed`；本轮保留其会话/首条/当前请求关联，不增加删除正式会话或独立清除此关联的功能。正文按既有保留策略清理不等于删除身份关联；不新增ID消费或删除墓碑表，也不迁移旧会话。

正式消息投影与初始事件在同事务写；Owned 会话只留一份首条消息身份。回显关联使用 backend 每次 prompt 调用的内部派发尝试上下文（firstSubmissionId/userMessageId/requestId + 固定Agent身份/资源代），随该调用的事件sink传给mapper；FIFO响应结算屏障结束该尝试关联。不得以相同文本或 Agent messageId 作为匹配依据。可证明属于该尝试的 user chunks 保存原始事件，但投影为同一 userMessageId 的回显补充，不覆盖已保存用户原文；回显内容不同则显式显示为 Agent 回显。响应后/无上下文/旧资源代的 user 事件不得猜测归并，保留为有来源说明的独立 Agent 输入事件，不能吞掉或改写首条。

下一次有意发送相同文字绑定新的消息/派发上下文，因此不被合并。没有 Agent 回显也能完整读取。测试必须覆盖不同messageId回显、异内容、合法第二条相同文字、无可证明关联的迟到事件和重复投递；不把全部 user 更新一律当作首条回显。关联字段与事件投影在 WP1 冻结到共享合同，避免实现阶段另造隐式规则。

准备期 Agent 标识不是正式恢复承诺。正式保存时可以尚无可承诺的 agent_session_id；**只有新初始提交记录证明从未派发**才允许重建准备端点，不能走普通 resume 的 NULL 兜底。在持久写入 `dispatch_started` 的同事务中，确认并固定 Agent 会话标识、当前资源代与本次尝试；之后遵守原身份恢复不变量，绝不能再换 Agent 身份补发。

### D8. 将“可能派发”标记放在外部调用之前

每个 session 的既有串行门保护首次保存、继续与其他 prompt；数据库唯一约束和 compare-and-set 阶段转移是跨重试的事实，不依赖内存队列。

| 窗口 | 持久事实与可观察行为 | 恢复动作 |
| --- | --- | --- |
| 未完成附件/验证 | 无正式行；仍为准备 | 保留浏览器草稿；显式重试上传/准备 |
| 首条保存事务失败 | 全部回滚；无 accepted | 同页同 ID 可重试，不派发 |
| 保存成功、未写 dispatch_started | 正式行、完整用户消息、请求均存在；可证明未派发 | 查询返回阶段；用户显式继续，禁止启动后自动跑任务 |
| dispatch_started 已提交但尚未写 pipe | 已固定 Agent 身份，存在可能派发窗口 | 崩溃后保守 uncertain，即使实际可能没发出去也不自动重发 |
| 已发送但未收到最终结果 | 命令和任务已受理，用户消息只一份 | 正常观察；崩溃窗口 uncertain，先查询/恢复原身份 |
| 已有终态 | 原首次结果权威 | 同 requestId 重放只读首次结果 |

首个 `session.start` 在保存后返回 accepted；正常终态与第一轮实际任务结果收敛，不能将仅保存成功当作 completed。可证明从未派发的设置/建立失败写 failed，但保留正式会话与首条内容。用户显式继续使用**新的请求 ID**和 `retryOfRequestId` 引用失败尝试；输入正文固定指向同一 first submission，新的已选配置另行验证并记录，不能再追加首条消息。相同原 requestId 必须返回原终态，绝不能将 failed 改回 accepted 或再次执行。同一初始提交多个继续尝试通过阶段 CAS 只允许一个跨越首次派发边界；曾可能派发后拒绝这一类继续。

尚为 accepted 且未派发的崩溃记录，启动只收敛为明确可证明未派发的失败，保留正文并等待显式继续；与既有 recover_unsettled 区分，不能一律 uncertain，也不能自动消费持久输入队列。`command.status` 的新协商投影应给出可核实的首次提交阶段/继续条件，不能让客户端按超时猜测。所有启动恢复与事件终态仍在 owned 提交口产生。

#### D8.1 显式继续的闭合合同

- **命令与载荷**：继续仍走 Sync-only mutation `session.start`，由 `payload.kind` 判别。`kind:"continue"` 的闭合字段是 `firstSubmissionId: UUID`、`retryOfRequestId: UUID`，以及可选的 `mode: string` / `configOptions: object`（只允许本次重新验证后要应用的值，缺省表示沿用记录的选择）；MUST NOT 携带正文、附件清单、`receipt`、`runtimeEpoch`/`generation`。`envelope.sessionId` 必须是已有正式会话，且与 `firstSubmissionId` 所属会话相同。
- **`firstSubmissionId` 的来源**：由服务端在首次受理事务生成，与 `sessionId` 一对一，写入 `owned_first_submission.first_submission_id`（唯一列）；`kind:initial` **不携带**该字段。它经 `session.start` 的持久终态、`command.status` 的协商投影与准备的 `committed` 结果返回，因此刷新、响应丢失后客户端总能重新取得。它不等于附件清单的 `submissionId`（附件清单标识：附件帧定义它，`kind:initial` 的 `session.start` 用它认领清单；它不是首条提交身份），也不等于 `firstMessageId`（持久用户消息身份）。
- **准入**（同会话串行门内，按顺序）：当前连接已认证且设备未撤销 → owned 会话存在且 Agent/目录仍可解析 → `owned_first_submission` 存在且 `firstSubmissionId` 相等（不存在或不等均回 `command.idempotency_conflict`）→ **若首次提交已有成功终态，回 `command.idempotency_conflict`** → **若阶段为 `dispatch_started`、`everDispatched=true` 或首个命令为 uncertain，回 `command.uncertain`**（不是 failed，不重建、不派发）→ 仅当阶段为可证明未派发时，要求 `retryOfRequestId` 指向本会话一个持久终态为 failed 的 `session.start` 命令（否则回 `command.idempotency_conflict`）。
- **端点与事务**：在串行门内按持久 Agent/目录重建或复用临时端点，重新读取并应用本次选择的配置；端点建立或配置复核失败时写该次 attempt 的 failed 终态、保持 `everDispatched=false`，正文与首条身份不变，仍可再次显式继续。端点就绪后，在**同一事务**写入 `dispatch_started`、固定 Agent 会话标识、本次 attempt 的 requestId 与 `retryOfRequestId`，然后才允许调用 Agent。事务提交后无法确认结果 → 既有 `uncertain`，绝不自动重发。
- **幂等与并发**：每次继续携带新的稳定 requestId；同 requestId 重放只返回该 attempt 的持久终态。不同 requestId 并发继续由阶段 CAS 只允许一个从“已失败且从未派发”进入 `dispatch_started`，其余回 `command.idempotency_conflict`（details 不含资源信息）。历史 attempt 的结果由既有 `owned_command` 持久记录提供，不新增 attempt 表。
- **投影**：协商了新 feature 的 `command.status` 投影给出 `stage`（`saved_not_dispatched / dispatch_started / terminal`）、`firstSubmissionId`、`firstRequestId`、`currentRequestId`、`everDispatched` 与 `canContinue`；未协商的旧客户端不新增字段。正文始终只有一份首条消息。

### D9. 现有模块边界与准备容量统一

- core 拥有 ID、准备生命周期、首次提交/继续的业务规则与纯端口；依赖闭包不变。
- agent-host 通过现有 backend/endpoint 机制管理临时端点、真实选项、关闭、代隔离和进程树；新建临时端点不要求 SessionStore 中已有行。没有临时 endpoint 不能伪装可发送。
- storage-sqlite 实现 owned 原子首条提交、读投影与迁移，以及 core 的准备附件暂存纯端口（运行期元数据 + 同一受管内容区，无 `owned_session` 外键行），不读取 Agent stdio。附件正式与准备暂存经组合根注入的端口协调，不横向 import。
- identity-auth 负责准备凭据纯签名/验证契约；identity-keystore 只沿已有平台端口签名；新增签名域登记到 transcript 机器表，不复制到协议以外的常量表。
- server::sync 独立接控制帧、首次命令、附件准备目标及 DTO mapper，只调用 core 用例；不调用 Node Link/local_admin 等同级 adapter。
- app 接线这些端口、计时和关闭顺序；临时资源关闭先于 Host/Store 关闭，所有任务有取消/回收所有者。
- 前端 protocol/sync-client/feature/platform/page 单向依赖；准备状态独立于正式 conversation summary，UI 不直接网络/存储。

临时与正式 ready 资源共用现有节点容量预算。准备采用可见租约、失租可回收，正在首条提交原子转移的资源不能被并发回收；关闭失败/未知照旧占位，不能靠删除内存索引伪造腾出名额。不得启动第二个旧进程级 idle sweep。

## Risks / Trade-offs

- 新签名用途与浏览器准备正文存储扩大安全面 → 专用域/类型入口、设备绑定、认证优先、无秘密日志、标签页隔离、退出清除；签名证明不升级为登录凭据。
- 服务不保存预留却要跨重启保留 ID → 使用可验证的准备凭据；不用随意 UUID 或持久 prepared 表规避约束；同 ID 正式受理后不再分配。
- 浏览器无法保证关闭/崩溃时执行脚本 → 正常导航保证逻辑清理，异常残留不可被陌生页面恢复，后台活草稿不误删；物理孤儿清理不能冒充可靠关闭通知。
- 模式配置需要真正初始化 Agent，可能有 Agent 自身副作用 → 用户已确认临时 Agent；不发送 prompt，真实容量/回收与错误显式。
- 原 prompt 正文和附件表结构不满足首条先存 → 新事务写集与 owned migration；覆盖事务故障注入、回显归并、TTL、旧库升级和旧命令回归。
- 初始派发后 Agent 永久丢失仍可能无法恢复 → 保留持久输入与真实 uncertain，明确剩余风险；本变更不能宣称消除了所有 Agent 恢复失败。
- 在途生命周期实现/验收尚未结束 → 新变更依赖其最终基线，相关共用文件串行合入；不修改其历史任务完成状态。

## Migration Plan

1. 先在独立 WP1 冻结新增 feature、控制帧/动作、start/retry payload、证明字段与 transcript、错误投影、core 端口/DDL 和浏览器存储白名单；更新权威文档/ADR-0014，保持“计划”标识直到实现与验证完成。当前 proposal 不改机器资产。
2. 实现只增 owned 首条表/字段及浏览器独立准备库的版本化 migration；禁止回填历史会话、禁止扫描空正文改状态，Access imported 数据白名单不变。为 schema 升级提供旧库测试。
3. 组合根确认所需端口齐备后才宣告 feature；server 先兼容旧客户端，Web 再消费新 feature。未协商禁用新入口，保留旧会话读取/交互。
4. 在隔离测试节点验证准备无记录、真实 Codex 进程/Daemon 重启、附件首条与所有崩溃窗口，再开放新入口。不停止/重建用户当前 Daemon，也不读取/修改真实旧会话作为验收材料。
5. 回滚优先关闭新入口及 feature 宣告，不删除新增表或已经接受的首条记录。回滚二进制必须仍能读出新初始阶段并拒绝不安全派发；不能直接回到不理解新 schema/NULL 初始身份的旧二进制。浏览器库降级只清理未提交准备，不删除服务器已接受的消息或把它变成新的草稿。

## Open Questions

无须再次向用户确认的产品分支。可延后的只有具体代码文件拆分、现有端口扩展的 Rust 命名和浏览器异常清理的实现细节；这些不得改变固定 ID、认证、未提交不落服务端、完整首条先存与不自动重发的约束。WP1 必须先给出可执行的字段/DDL/错误与 transcript 合同，不能把安全协议、首次派发阶段或原子性推迟到编码时猜测。
