# 装配、opaque正文传输与交付范围 v1

本文件冻结后续可实现的装配面，不宣称root当前支持新DTO、binary帧或视图查询。认证管理控制沿当前 `/api/access/v1/control`、`acpr.access.v1`，原32KiB文本帧、JSON深度8、认证/命令速率、1MiB/256 outbound背压和writer发送/关闭超时保留。新增正文不放进原strict JSON正文字段，不调用JSON.parse→JSON.stringify搬运原结构。

## 1. 生产装配顺序

1. node在现有Storage/真实ApplicationPolicy之外构建一个SharedViewOwner，注册自有任务supervisor；不让其管理进程、派发、审批或数据库。生产ReadAuthority从同TrustStore/ApplicationPolicy构造。
2. ND SourceSlot prepare阶段取得实际Scope后、开门start之前安装SourceViewCapture/Worker，worker任务先归owner。source实际NativeSourceId来自可信配置，不来自客户端。注册失败标view不可用，不遗弃原IO/Job责任。
3. 同ND target原创建/恢复操作enqueue前安装实际(scope,session)→key/resource观察映射。单原pump借Received调用capture，然后沿ND小事实apply/gap/ack；所有display失败都不进控制gate。root standalone测试可直接同唯一EventReceiver.next pump；生产observed必须用同唯一Lease，不并行跑两个消费者。
4. NativeReadCoordinator与同ND SourceSlot共享原实际Connection/完整setup/operation registry；实现新增NativeHistoryPort。真实Replay response由原单pump分发两个独立事实消费者：ND小事实控制与view完成。不是reader二次消费，也不是view回调控制ack。该production port须最终ND接口独立gate后才可接线。
5. gateway以真实已认证actor调用ViewReadService，按下面协议读取固定chunk；writer最终发送再次验当前actor。Web只安全呈现结构/来源/限制，不把节点视图cache当永久档案。

普通open当前view即可读到已收内容，不等未来增量；有重放时按reason异步协调原生重读，现有内容标Revalidating，失败stale。外部创建会话的列表、发现及执行接管仍归相应ND操作，浏览view不要求伪造Run或先绑定平台执行。未有可信target/resource观察绑定时必须先走真实按标识加载/查看路径，不能直接browser id→假source slot。

## 2. 封闭文本控制DTO

所有新DTO未知字段拒绝，数字序号用十进制字符串，Id128沿现标准；source和session参数长度沿现身份规则，不在控制中传未知正文/标题。一次view.open受现命令窗口计数；后续每个chunk credit是独立有界传输帧，不作为新管理命令，不消耗每分钟120命令额度，不对总正文施加隐含限制。

```json
{"v":1,"type":"command","requestId":"<Id128>","action":"view.open","payload":{"sourceId":"<EntityId>","nativeSessionId":"<actual id>","reason":"first_open"}}
```

reason闭集为first_open、refresh、reopen、reconnect、reacquire；server由实际连接状态校验原因，不把客户端理由当重读成功。open的管理登记仅是读取操作，不复用输入请求登记来保存正文。response为新封闭结果：

```json
{"v":1,"type":"view.opened","requestId":"<same Id128>","streamId":"<Id128>","viewEpoch":"<EntityId>","snapshotRevision":"0","capabilities":{"opaqueChunks":1},"status":{"mode":"current_only","freshness":"current_observation","projection":"available","join":"unassociated_segments","historyIncomplete":true,"cacheRecoverable":false}}
```

status只含平台枚举和bool；原生标题/错误文字/巨大ID均使用内容record引用，不塞入此帧。mode闭集current_only、replay_requested、replaying、replay_complete、replay_incomplete；freshness闭集current_observation、revalidating、stale；projection/join沿工程合同。错误返回当前管理错误信封并加封闭ViewError code，不回显未知正文或配置。新streamId由node生成，绑定该已认证WSS连接和actor、target/viewepoch，不能跨WSS复用；新设备重新open且独立当前验权。

取消使用 `action:"view.close", payload:{streamId}`；它只释放该stream，不清source view、不发session/cancel。显式刷新可重发view.open reason=refresh并复用同在途ReadOperation，独立stream取新H；旧stream可关闭，不能不声明就把其epoch换掉。旧客户端未请求新action仍走原控制文本路径；未经认证收到任何binary直接拒绝，不能以binary旁路认证/命令格式。

## 3. Binary chunk v1的精确格式

新增**明确独立的opaque binary transfer codec**，总帧仍小于32KiB；其payload不是控制JSON，不经深度8解析。数据header固定72字节，均大端无符号；原始正文只拷贝精确字节，任何u64在Web用BigInt读取，不经Number。

| 字节 | 长度 | 含义 |
| --- | --- | --- |
| 0 | 4 | ASCII `ACPV` |
| 4 | 1 | 格式版本=1 |
| 5 | 1 | type:1 Data / 2 Credit / 3 Detach |
| 6 | 2 | flags，Data只允许bit0=该record最后片段；其余0 |
| 8 | 16 | 原streamId的16字节 |
| 24 | 8 | packetSequence，从1递增；Credit为已收最高packet，初始0 |
| 32 | 8 | view内部recordId；Credit/Detach必须0 |
| 40 | 8 | 本record byteOffset；Credit/Detach必须0 |
| 48 | 8 | 本record totalLength；Credit/Detach必须0 |
| 56 | 1 | recordKind:1 OriginalRaw / 2 ItemProjection / 3 Status / 4 SnapshotEnd；Credit/Detach=0 |
| 57 | 7 | 保留全0 |
| 64 | 4 | payloadLength，Data 0..16384；Credit/Detach=0 |
| 68 | 4 | 保留全0 |
| 72 | n | 原精确payload片段，无base64/重编码 |

长度必须恰等72+n；totalLength/offset加法checked，offset+n≤totalLength，末片flag与offset+n==totalLength严格一致。空record只允许kind=SnapshotEnd，不能用空OriginalRaw伪造JSON。所有Data的recordId>0；SnapshotEnd使用其自己平台recordId，totalLength=0/offset=0。record总长度不设平台上限；现usize/进程地址空间不可表示时真实ViewError::LengthUnrepresentable明确报告，不转换成16/128MiB预算。DATA maximum只约束一次复制/网络背压，不能限record/stream/view累计量。不同record的offset从0开始；同record必须连续，拒绝错offset而不静默拼接。

OriginalRaw是该target可验证归属的一个原始完整ACP RawMessage，多个binary片段可跨UTF8边界，Web用streaming TextDecoder，不逐chunk强制字符串有效。不把source中无归属通知交给任意会话，也不把其它target原文混入。

ItemProjection是平台自有浅层JSON record，其数字全部decimal strings；只含平台anchor/revision、已核实kind枚举、知识状态和raw片段引用 `{recordId,startByte,endByte}`，原生长message/tool ID、未知kind/状态/数值仍通过raw引用读取。内容块顺序使用原已验证native顺序或明确observation顺序；未证明轮次给null/unknown，不带伪nativeTurn。

Status也是平台自有浅层JSON record，含状态枚举、完成边界的真实source/read epoch/RPC来源说明（RPC在实际允许输出范围以raw引用表达）、限制code；显示文字由Web本地映射，不能让远端提示文本变成管理状态。SnapshotEnd固定空payload；收到后浏览器进入同H的tail，不能把它叫原生ReplayComplete。无native replay完成时也会有SnapshotEnd。

## 4. 信用与撤销的真实顺序

客户端初始发Credit packetSequence=0开启一次chunk许可。每stream最多一个queued/sending/sent未ack的Data；发送成功后客户端确认最高packetSequence=n的Credit才提交该chunk cursor并授予下一次许可。许可不累积，重复≤最高已处理ack无新许可，超过highest_sent或不连续的新ack拒绝；不存在客户端声明“已收完整body”使node提前推进权限。Data发送失败不提交cursor，不自动重发原生操作；该WSS结束，客户端新open可重读节点仍持内容。

每次新增许可：读取worker先等待数据/对应版本（无actor gate），然后取得当前actor gate+actual current权限→只复制最多16KiB→释放gate→进入原有全连接有界outbound；writer再取得同当前actor gate/校验generation→sink.send该binary Data（沿现15秒发送等待）→释放gate→等待下一credit。不能持gate等credit、等全部body、等重放或view worker。status/snapshot-end等不含正文消息也走同writer当前权限围栏。

撤销在chunk复制前完成：拒读；复制后排队且撤销先拿gate完成：writer丢该chunk并关闭对应WSS；writer已先持gate进入sink.send：该chunk可完成或发送超时，撤销随后生效，拒下一块。不能声称已发字节被收回。全body的PreparedBuffer/大批已授权chunk禁止存在；每stream仅一块，跨stream仍受原1MiB/256总outbound约束。恶意credit不能反向call控制或为其它actor续读取；解析固定72字节后核same authenticated connection+stream+ack state，不能自动凭streamId授权。

credit是flow-control，不是管理新动作，不走MAX_COMMANDS_PER_WINDOW。它不免当前读取鉴权，不取消原认证、连接数、并发socket、帧、outbound和发送超时规则。outbound满/慢客户端可使该读取失败或断该socket，节点原文/共享view继续保留，pump/ND控制持续；不能因此截断或淘汰节点view，也不记录原生保存失败。sender释放outbound permit只证明网络缓冲回收，不能释放raw log。

## 5. Web保真和安全展示

Web新增原文codec用平面token/字节范围扫描原UTF8，不将整raw交JSON.parse或serde式普通JS对象；数字保词法切片，未知对象和数组保顺序/原片段。可以仅对一个已识别JSON字符串literal使用JSON.parse解码字符串（它不包含数字结构），不能parse→stringify整个RawMessage。platform ItemProjection/Status可用现strict解析，因其完全是本平台受控浅层结构且不承载未知数值。scanner必须是迭代的，深嵌套/大叶数不靠JS递归栈，不新增深度/叶数总预算。

基础页面显示当前source、current-only/replay状态、stale/incomplete和缺失/不关联限制；列表/标题/思考/工具/diff/终端/审批提示均不可信。原raw按惰性需要读取，不为展现保真自动创建DOM或执行HTML/SVG/控制序列。Markdown禁原始HTML；用textContent/安全组件，无innerHTML直接注入。终端控制字符显露或剥离其效果，保原文引用。可信按钮和原生提示文本明确分开，审批选择来自真实审批owner原广告optionId，不由view伪造审批能力。

图片/外链/文件/uri默认安全占位，不自动网络fetch、不携节点/浏览器凭据、不根据原文读本机文件；用户动作触发的真实资源读取仍需独立资源入口校验重定向/最终目标、根范围、大小/类型/响应预算，不在本view chunk API借raw引用开放任意URL代理。没有该入口时明确unsupported，不把链接渲染当已支持附件。自定义协议默认拒绝，只有已有支持并完成明确目标核对/用户确认才能交外部处理。

## 6. 后续写范围与串行合入

本轮设计仅写外部session-loop-q003-shared-view-author目录。编码须先独立gate，并在此目录新workspace完整复制正式基线，旧候选不改。

| 工作包阶段 | 可写范围候选 | 依赖/交付限制 |
| --- | --- | --- |
| SV-module | 新app/src/domain/shared_view.rs及目录；application/shared_views.rs及目录；adapters/acp/shared_view_projection.rs及目录；自有开发测试/README | 不改旧views/P1/P2/SQL/raw parser/ND；内部定义可用真实ACP standalone测试；不代签生产tap |
| SV-mount | domain/application/ACP mod.rs必要导出和只读raw借用适配 | 与ND adapter修改有潜在重叠，单owner串行apply，重跑ACP+ND+view全部合同及实际检查 |
| SV-native-read | ND正式gate后的新增tap、可信target映射及NativeHistoryPort真实同Connection实现 | 属显式新增接口，不能在ND作者未交付时并写其文件；同版再预审 |
| SV-gateway | 新view-stream codec/read-authority模块；management_ws.rs的封闭DTO/binary Outbound+writer；node.rs构造/关闭 | 共享文件单owner串行，继承现有actor gate/outbound回归；不能先改32KiB/depth8 |
| SV-web | 新views原文scanner/renderer/stream模块；main.ts最小页面接线 | 应用ui skill，真实frontend checks与浏览器交互另验；不能只交mock |

tester维护独立验收资产，coder不改oracle；root最终统一合入并重跑实际npm run check、build:rust及含新fixture的strict all-target检查，前端按实际工程独立检查入口。无提交/推送/发布/归档授权。本候选不会修改plan/tasks或提前勾完整视图；后续root以实际完成判据记录阶段事实。
