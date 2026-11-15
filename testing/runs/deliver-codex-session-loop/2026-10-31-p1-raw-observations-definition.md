# P1 raw观察记录：独立执行定义

2026-10-31。只读定义，以下 **8组全部not-run**，不编码生产、不修改统一86例/其他tester资产。输入为外部revision3 `p1-raw-observations-contract.md` SHA256 `8e2413b6f43c781cfee2afb33239ec0a7cfdcf2a46db469a133c6fc165b1c3b3`，当前structured-history及implementation-contract §6/6A/7、实际Domain ports与本地SDK双版schema；来源清单为 `p1-raw-observations-inputs.json`。

最终追加核对：本仓 `openspec/changes/deliver-codex-session-loop/p1-raw-observations-contract.md` SHA256 `a071335b22ed7b028323f1e629d9df3460e858b1434b0adf43aa76d3659f1f3a` 已冻结最小端口。以下按该最终合同精化；外部8e2413原稿及最初inputs不改。最终输入单独保存为 `p1-raw-observations-inputs-final.json`。P1-TG01–03定义缺口已关闭，实际8组仍全部not-run，是否达到编码前门槛由原reviewer独立决定。

本子包只保留完整原始JSON-RPC Update观察，不做scanner、typed current、实体合并、索引/新namespace、原位工具、输入/批准/持久历史或分页。VPROJ-PLAN-002及既有VP-S05/S13的实体原位更新义务保持pending，不能用“不聚合”永久取消。旧28组中的P2未实现分支不记pass，也不沿用其索引/扫描预算为本包制造额外依赖。

## 最小API和确定顺序（按最终合同冻结）

以下是可独立适配的语义接口，不指定最终Rust拼写：

- `ObservationRead::begin(windows, handle, &Strategy)`：先由当前真实owner提供所属ViewHandle和协商Strategy，Domain先验证handle、预留read workspace(h,1024)，总1536字节/0 items。只提取Version，保存固定状态/handle；session复用handle.target().native_id()比对，不clone完整Strategy（内含initialize raw）或无计费长SessionId字符串。来源/权限由实际Connection owner保证，当前Incoming没有私有连接token；begin不是客户端授权入口。
- `observe(windows, &Incoming)`：先按枚举/目标分流；非Update、不同session、本协商v2的目标State分别作为NotUpdate/OtherSession/StateFirst原因返回Ignored。这三类不收费、不推进seq/position、不记显示缺失、不改读取状态；v1未定义的同名state_update tag仍是未知显示观察，不借名字消费v2控制语义。匹配显示Update先验证当前handle，再检查真实通知method/params/sessionId/update/sessionUpdate形状一致性，伪造/不一致输入返回InvalidObservation、零发布，不冒称已验证来源。合格显示Update才checked推进seq，即使后续显示失败也不复用。版本取begin所提取协商值，origin取真实Update；无网页可填的version/origin/seq参数。
- 成功返回Recorded{seq,position,实际window_status}；预算/构建显示失败返回Dropped{seq,reason,实际window_status}或等价有语义结果，保留sticky首原因/count/first/last seq。没有全局DrainOnly、自动重试或待显示队列。外部Domain/结构错误不得全压成Capacity；匹配显示Update的陈旧/foreign handle返回Stale，不污染当前新read。发生在构造前的NotUpdate等不是Dropped。
- seq起于0、首记录为1；值u64::MAX可作为最后一个已分配seq，再来匹配显示Update返回CounterExhausted，原read标Incomplete(SequenceExhausted)，不发布/复用/回绕。后续仅本read显示登记仍拒绝，其他分流继续；新begin可重建。drop count等checked统计无法推进同样返回CounterExhausted，不制造假边界。极值用仅cfg(test)窄fixture证明，生产没有任意counter setter。
- `stats()`只读固定Copy元数据，不生成snapshot/复制raw；`parse_header(&[u8])`返回固定header及借用raw片段，零分配，只验证记录格式而非JSON语义/Connection来源/权限。对非法头返回InvalidHeader，不改任何view/计数、不panic、不把未知kind变聊天。

头版本1已固定：magic ACPO、envelopeVersion=1、wireVersion∈{1,2}、origin∈{1,2,3}、recordKind=1、seq>0、rawLength>0、保留字节全0、32+rawLength与整个slice长度严格相等。origin0是未来保留值，本版encoder/parser均拒绝，不读成Unknown或Live。

Drop仅释放本read所有费用，不自动cancel/close/finish/release原生工作或伪造完成。持有旧read workspace跨view释放/换代仍向原目标/节点收费到最后Drop；new窗口不能因old Drop/错误被标失败。

## 实际执行设置

后续L3使用公开Domain/ObservationRead与合成数据；L4用真实Connection双工peer，经实际initialize/replay/next取得Incoming。不从裸字段构造对象代签真实入口。固定同版源码/锁/资产SHA，独占target与临时资源，使用现有依赖离线运行；日志只写长度/计数/摘要，原始synthetic fixture可外部保存。不读用户数据库、密钥、私有会话；不访问网络/浏览器/audit。

raw oracle是发送的完整通知payload bytes（不含stdio换行，CRLF由原transport去掉），不可用Value重序列化作为预期。读取snapshot时只借用记录字节，比较逐byte和全部header offsets；不让header parser本身同时充当encoder oracle。

## R1 双版真实wire与完整raw保真（not-run）

操作：分别协商v1/v2，peer发送真实session/update通知，覆盖工具、合法消息chunk、未知sessionUpdate/ContentBlock，以及嵌套未知字段、外层_meta、params扩展、乱序/空白、1e+02、-0、UTF8、`\u0000`与`\u0061`转义。在发送peer保留确切payload字节；经next及observe发布，再从snapshot取记录。重复相同正文及同nativeID也逐条观察。

预期：前32字节按固定offset/little-endian独立检查，尾部与整条原通知逐byte相等，包含jsonrpc/method/params/sessionId/update/外层_meta，不截断或规范化。每次匹配显示Update一条独立观察、key=None，不按nativeID/正文去重，不伪typed实体或Run。无需完整current解释未知字段。P0尚未交付时不以手造v1 null Incoming冒充该实际入口通过；v1 null案例单列依赖，省略/合法字符串路径仍可独立验。

## R2 origin、seq、分流与新代际（not-run）

操作：replay窗口内记录A1，正常响应后同代际Live记录A2；单独resume窗口验证DuringResume，其他session B并发。交错Permission、Response、UnsupportedRequest、UnknownNotification、目标State与不同session Update，并在预算失败帧后再录成功帧；结束并新begin。仅cfg(test)把seq推至MAX-1验证边界。

预期：A1/A2分别保存原DuringReplay/Live，一条一个origin不覆写；DuringReplay仅窗口事实，不证明“历史”。标准响应不释放read状态/归零seq，Live继续递增；新代际重新首seq1。NotUpdate/OtherSession/本协商v2的StateFirst均Ignored且无本read副作用，activity先被独立owner处理；v1同名未知state_update正常保留为显示观察。Dropped占seq但不占position，下一成功记录seq可有间隔；首/末舍弃seq只表示边界，不宣称中间连续丢失。CounterExhausted无回绕/新publish，仍可分流权限/v2 State，不自动重新加载/取消。

## R3 FIFO持续近端与单条超限恢复（not-run）

操作：在无长期snapshot阻塞且node尚有空间时，真实合法显示流分别超过3倍4096条与3倍16MiB，确认多轮FIFO；另用合法缩限fixture精确触达单目标/节点上限。插入一条合法ACP帧但32+raw+固定费用超视图预算的大Update，再发小Update；注入一次insert预留已淘汰部分条目后的分配失败。

预期：保留最近成功观察，不停在首次淘汰处；当前帧只一次insert，无replace/fallback/无界retry。单条不能装入时不复制本条，Dropped/Incomplete(Capacity)及真实水位可查；下一合法小帧仍能保存，失败后的seq不复用。分配失败前已经实际FIFO淘汰不能谎报原窗口没变。超限只指显示，非原生保存失败；成功response/finish不能洗白Incomplete。缩限结果与生产默认长流分别报告。

## R4 显示失败后真实peer继续pump（not-run）

操作：真实Connection的A观察产生预算/构建显示失败；peer紧接Permission、目标State、B显示Update和有效replay响应。合成调用owner持续next并先分流控制/权限；观察peer接收方向完整wire。另以EOF、错响应ID、缺必需v2snapshot、超32MiB/深度96及原绝对deadline作真协议故障对照。

预期：A不持有Connection/writer，不发$/cancel_request、session/cancel/close/额外prompt，不关连接、不清pending、不延长期限，权限独立交给处理端口，B仍可更新；响应结束读取但Incomplete保留。v2 State交activity且不作为本包raw显示记录重复结算；v1同名tag没有v2控制语义。真协议错误按现有Connection关闭/未知边界，不能吞成普通Dropped继续宣称协议正常。此夹具证明可分流/零控制写，不代签实际node公平调度/授权/Harness运行。

## R5 费用、capacity与Drop（not-run）

操作：独立核算创建前后target/node usage：read1536/0；处理帧时workspace768/0；候选512+(32+raw.len的实际capacity)/1，key=None。将预算收窄到缺1字节/恰能预留，保留snapshot、旧read/workspace跨换代/release，再逐个Drop；覆盖new预留失败、frame scratch失败、候选失败/短写/abort和成功publish。观察只读stats不另分配snapshot。

预期：先reserve再拥有/复制，header32使用已收费scratch，持久只有Domain不可变bytes；没有Strategy/initialize raw/Value/RawMessage/独立长session字符串/Arc免费保活。长期与perframeworkspace固定字段size_of/capacity有实测上界，费用归同目标/节点，发布后只退本帧workspace；snapshot/旧对象未Drop不提前退账，最后引用才退回对应基线。old read跨代仍计旧账，Drop/abort仅影响自身所属cell，不污染新read或取消工作。预算端口同版已证明部分引用原tester报告，本模块新增字段/复制峰值另取证；普通OOM abort边界不冒称已可返回Capacity。

## R6 header decoder负面与借用输出（not-run）

操作：对实际记录逐字段改变magic、envelope/wire/origin/kind、seq0、rawLength0/MAX、非零reserved；长度0..31逐个截断、尾部短1/多1、两个record直接拼接；构造头正确但raw非JSON的slice。检查返回值只借用输入raw、可读header固定值，不分配或修改输入/窗口。

预期：格式不满足固定版本即InvalidHeader，checked长度转换不溢出/panic，不截尾“兼容”；raw长度与整slice严格相等。origin0本版拒绝，不能读成Unknown或Live。头正确但raw不合法JSON时parser只说明“头格式可解码”，绝不产出已验证Update/来源proof/授权；P1生产记录只有真实Incoming路径写入，本包不新增JSON scanner。借用Record/Header可合理Copy/Clone，不得由此取得免费拥有正文或延长Domain租约。

## R7 误用、陈旧scope与固定统计（not-run）

操作：same read持有后view release/新generation、另一容器foreign handle、同session不同resource/source；同时保留旧snapshot。对当前与旧read分别送匹配/非匹配Incoming；对真实Incoming可改公开字段作误用负例，覆盖method/params/sessionId/update/sessionUpdate形状或关联不一致。用窄test fixture触达seq/drop-count checked极值。外部负编译尝试Clone ObservationRead、公开counter setter、从裸position/snapshot构造写stage，以及owning raw Arc出口。

预期：匹配显示Update在当前性验证失败时Stale，零新publish/seq/drop统计变更，不影响新window；NotUpdate/OtherSession/v2 StateFirst仍只是Ignored分流结果。不一致形状返回InvalidObservation零发布，不把字段校验冒称来源proof；不能用同短sessionId证明跨Connection来源，实际owner绑定边界明确未装配，不要求本包造授权token。计数/边界固定O(1)、checked，不累积seen/队列/tombstone；CounterExhausted明确拒绝本read显示登记但不阻断控制。ObservationRead只有一个seq所有者，不能Clone或反序列化制造并行登记；safe借用header并不需禁Clone。编译错误要确因封装限制，不拿导入错误当通过。

## R8 同版回归和未完成义务（not-run）

操作：固定新模块/ACP mod/root Domain源码、锁和8组测试SHA；运行格式、clippy、新P1公共API、原view19、预算端口所属原资产及ACP发现/重放/AP权限回归；复核diff只在授权模块/声明/测试范围。不修改其他tester资产或原预期，原owner维护其证据。真实P0 null入口若同版可用则加入R1，否则准确not-run。

预期：原scope/费用/状态/原始ACP语义不回退，无新namespace、scanner/current/SQL/WSS/依赖或控制写端口。P1输出只表示RawUpdateObservation，不能称完整消息、工具/终端current、原位实体或完整轮次；VPROJ-PLAN-002、可信分页、实际node来源/撤销/共享调度、Web安全与真实Harness/Windows完整树及完整L5/L6继续pending/not-run。没有网络/audit外发。本定义完成不是产品测试通过。

## 编码前小缺口关闭（定义层）

**P1-TG01 closed-definition：分流、seq与错误结果。** a071335b合同§2“最小端口与分类”已明确Ignored三类、仅协商v2 State控制、v1同名未知tag保留、先current handle后显示seq、InvalidObservation、Recorded/Dropped与CounterExhausted只关本read。R2/R4/R7已按该双版区别和错误名精化；仅定义缺口关闭，不是动态通过。

**P1-TG02 closed-definition：头解析范围。** 同合同§2已采用严格版本：origin0拒绝、seq>=1、raw非空、reserved全0、精确length和尾字节拒绝，InvalidHeader且格式-only不解析JSON/授权。R6按冻结值验，无待选Unknown策略。

**P1-TG03 closed-definition：固定收费对象实际拥有内容。** 同合同begin仅提取Strategy Version、复用handle native ID、不clone Strategy/初始化raw或另建SessionId，1536/768及最后Drop费用固定。R5保留实际size_of/capacity、复制前收费和旧代际退账验证；源码布局证据仍待编码后验，不因定义关闭签实际分配安全。

上述三项已由原提出tester对最终a071335b合同逐项核对关闭，未改原8组保真/滚动/计费预期，只精化版本分流、结果名与strict header。无需新增用户产品决策；交原reviewer限定编码前门槛，不等P2或完整node。此刻全部not-run，完整P2实体原位与change义务不变。P0实际独立验收由其执行者报告，本轮不重复运行或代签。
