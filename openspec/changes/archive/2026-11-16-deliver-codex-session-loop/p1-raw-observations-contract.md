> Q-001（2026-10-31）：本文保留为既有实现子合同，非本轮新接线依据。与当前specs/design冲突的正文、输出、实体/快照容量预算、重放门槛、签名期限及派发再验原actor均已停用；旧测试仅说明原范围。非冲突协议事实可复用，但须按新阶段独立复核。

# P1：有界原始观察记录合同候选

2026-10-31；只读设计，待独立定义/预审，不编码。独立于P0；不签实体current、原位工具、片段聚合、JSON scanner或任务3.2完成，VPROJ-PLAN-002仍由P2解决。本包尽量复用实际Domain端口，只保留真实完整raw通知，不先编写新JSON解析器。

## 1. 输入、所有权和范围

新adapters/acp/view_observations模块接收当前真实Incoming::Update的借用，绑定owner选定的ViewHandle/目标session/协商Strategy。核对sessionId与绑定目标、handle的scope/resource/read；不接受网页声称的origin/seq/version，不保留RawMessage/Value/Arc。Incoming本身尚无私有连接token，实际来自哪个Connection的保证由持有该Connection的owner负责；本纯模块不把这些裸字段伪称已完成真实owner授权。permission、RPC响应和state/activity先分流至各自owner，原始观察不执行控制动作。

ObservationRead用Domain stage_workspace(h,1024)持有固定read状态，收费1536字节、0 items；保留本代际checked receive_seq、显示丢弃首原因/count/first/last边界。handle克隆/固定字段的实际size_of须落在已计费元数据中，不另分配String。标准replay响应不释放当前view状态，Live更新继续使用同代际seq；实际view释放/换代最后Drop才退账。无索引、seen集合、tombstone或后台队列。

## 2. 精确可解码记录

### 最小端口与分类（主执行编前细化）

- `ObservationRead::begin(windows, handle, &Strategy)`仅提取协商Version并保留已计费工作区、句柄和固定计数，不clone Strategy/初始化raw或另建SessionId。身份比较复用handle.target().native_id()。该绑定不证明真实Connection或操作者授权，仍须实际owner负责。
- `observe(&mut self, &mut ViewWindows, &Incoming)`只借用输入。非Update、其他会话，以及本协商v2的state_update返回Ignored，零seq/费用/缺失变化，留给原owner先处理；v1未定义的同名tag仍按未知显示结构保留。匹配的显示Update先校验当前句柄，陈旧则Stale、不影响新代际。method/params/sessionId/update/sessionUpdate须与真实通知形状相符；伪造/不一致输入明确InvalidObservation，零发布，不把这种字段校验冒称来源授权。
- 匹配且当前的显示Update推进checked receive_seq，然后恰好一次预算构建。成功返回Recorded(sequence, PublishFact)；可处理的显示失败返回Dropped(sequence,固定原因)，记首原因、count及first/last seq，并保持Incomplete。count/seq不可表示时不回绕、不再给本read签发显示序号，返回CounterExhausted；不关闭协议pump或取消原生工作。NotUpdate/OtherSession/State不计作显示丢失。
- `stats()`只返回固定Copy计数；first/last是丢弃事件边界，不声称中间每条都丢失。工作区字段最后Drop，借用型记录不延长拥有者寿命、不开放raw Arc/可消费buffer。
- `parse_header(&[u8])`只校验并借用32字节格式和raw，不解析JSON、不证明来源或授权。本版仅接受wire 1/2、三种已知origin 1/2/3、kind1、seq>=1、非空raw、精确声明长度及全零保留位。origin0仅为未来保留，本版拒绝；未知version/kind/origin、短头、长度溢出/不匹配及尾随字节均明确InvalidHeader，不截断。

这些端口只实现原始观察子包；实体锚点、派生current、JSON解释及原位工具规则仍在P2，不能由Ignored分类或raw记录代签。

每次匹配的显示Update（含不能保留者）推进一个receive_seq，失败不复用。每条保留记录是32字节固定二进制前缀加**完整原JSON-RPC通知字节**；Domain key=None，外层ViewItem.position只在成功insert分配，receive_seq不是原生消息ID/position/Run。

| 偏移 | 编码 |
| --- | --- |
| 0..4 | ASCII ACPO |
| 4 | envelope版本1 |
| 5 | 实际wire版本1或2 |
| 6 | origin：1=Live、2=DuringResume、3=DuringReplay；无证据不得伪填，本版拒绝预留值0 |
| 7 | record_kind=1，表示RawUpdateObservation |
| 8..16 | receive_seq，u64 little-endian |
| 16..24 | raw字节长度，u64 little-endian |
| 24..32 | 全零保留字段 |
| 32.. | Incoming.raw.bytes原字节，不规范化、不截断 |

长度checked且严格匹配。raw包含params/sessionId/update/未知字段/外层_meta，保真范围明确为整个原通知，而非仅update对象。原类型从raw的sessionUpdate/content等结构解释，不把工具patch呈现成普通聊天。本包提供安全惰性raw观察，不虚报已实现typed current。

**逐条origin与seq随raw保存**，不会因同实体后续Live而覆盖早期DuringReplay。消费者需要摘要时只能报告ObservedOrigins/Mixed，并以各记录为准；DuringReplay只是RPC窗口标签，不等于该通知必定历史。缺失记录只报真实缺失边界，不能替其补标签。新代际重新起seq，不能混用旧标签。

## 3. 预算和失败

先取得每帧stage_workspace(h,256)，总768字节，用已计费scratch构建32字节头/计长及固定上下文。计算32+raw.len，再stage_insert(None,payload_bytes)预留512+实际payload capacity及1对象，之后才复制前缀/raw；无to_vec/Value clone/解码旧blob/合并。具体实际API名字以root冻结版本核对，不能先假定签名已实现。

Domain FIFO可多次淘汰，Err后window_status零分配取得真实水位；不需要snapshot才能知道淘汰。当前帧只尝试一次insert，不能fit则Incomplete(Capacity)/固定舍弃统计，下一Update仍照常尝试。没有全局DrainOnly，不等待浏览器或无限队列；原始single frame可大于视图预算，拒绝复制本条显示数据，不等于原生保存失败。

32MiB帧/深度96/token131072仍由Connection负责。本包不新增100层scanner或134217728扫描计数：只定长编码、checked计长和有界raw复制，不能以本包通过签以后P2 raw-span parser已正确。全局allocator OOM继承root已声明abort边界，不假称全部heap失败可返回Capacity。

纯显示失败零$/cancel_request/session cancel/close、零连接关闭/输入/批准；owner继续Connection::next并按原绝对deadline分流。真正Connection协议错误仍原路处理；本包不吞掉已被协议层拒绝的帧，也不改connection关闭策略。

## 4. 验收门槛

- 实际Connection双版输入→记录完整raw字节逐字相等，含未知tag/字段、转义、1e+02、NUL转义；没有新的JSON scanner或规范化。v1合法null入口依P0，不绕过失败入口伪测。
- header offsets/版本/长度精确；丢弃、重放响应、Live继续及换代后seq正确；每条origin与raw一一对应，Mixed不当历史证明（解决003在观察记录层的定义缺口，原提出者判断）。
- 长流多次超4096条/16MiB仍保留近端窗口；目标/节点、snapshot/旧stage/旧代际同账本；先收费/实际capacity/Drop证据，与Domain独立报告逐范围引用。
- 单条超限后下一合法通知可保存；Err后真实水位可查，Incomplete不可被标准response洗白；状态/权限/别目标仍pump，wire零cancel/close。
- 公开输出只借用snapshot记录字节，无免费Clone/Arc/正文逸出；原view/预算/ACP发现权限回归，实际布局与编译诊断另证。
- 本包输出为Observation，不完成VP-S05/S13工具anchor/current；不把“不聚合”作为永久排除原位义务的理由。P2通过前整体投影仍未完成。

## 5. 来源与实施边界

Domain依赖root view-budget-port-contract.md（原d6ee1d36...，实现冻结后再核对实际API/摘要）。SDK2.1.0 v1 schema SHA1c57095acf9c0844054dd1fdc3624c121b3fdc24209ecc987fd6748c705fdef5、v2 SHA28c7251e820511da257cdb55d30e2c30bbbe59226305b04e3b751dc655e0778d；Incoming.raw/origin来自现有transport，不改变其语义。

建议写范围仅新模块/开发测试与ACP mod声明，无Domain namespace变更、SQL/依赖/WSS/真实owner。尚未编码/运行，无网络/浏览器/Web audit。独立定义需限定P1范围而非沿旧28组把P2未实现分支记pass。
