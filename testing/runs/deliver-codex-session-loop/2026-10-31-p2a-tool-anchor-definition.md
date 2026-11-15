# P2A 工具锚点：独立子用例定义

2026-10-31。**仅规划；A–H 全部 not-run。** 最终依据本仓 `openspec/changes/deliver-codex-session-loop/p2a-tool-anchor-contract.md` SHA256 `81d0b6ef3d80dcb295f3231f8f66cc2474ed5b0adf43030f3a4b902806c6b3f7`，以及现行 structured-history、P1、Domain 窗口/预算合同与实际公开端口。输入摘要另存 `p2a-tool-anchor-inputs.json`。原790fff规划/输入逐字保存在 `p2a-tool-anchor-definition-790-original.md` / `p2a-tool-anchor-inputs-790-original.json`；中间8e4e仅补转移表，最终81d0再补初始meta:null。以下是合同编前精化，非根据实现改验收预期；产品尚未编码。不覆盖旧 r4/deb54/c978 等文档，不新增稳定 CL ID，不改变统一 tests 的级别/义务。

本包覆盖“结构身份与来源顺序/工具晚完成”“未知结构载荷”“重放中断或超出预算”“有界缓存/慢客户端”的 **L3 模型及 L4 合成协议子范围**。257 工具不是产品上限；工具身份仅本 source/resource/read scope 内的原生 Tool/id。P2B current 值读取/scanner、首开 delivery、可信轮次、Node/WSS/实际授权、页面安全及完整 L5/L6 仍未验。

## 执行设置和公共判据

冻结产品来源/锁/独立资产摘要前不正式执行。后续使用外部普通依赖 crate、独占 CARGO_TARGET_DIR、现有依赖 `--offline --locked`；不 include 产品私有源码、不改生产或其他 tester 预期。L4 通过真实 Connection initialize/next 取得 Incoming，L3 使用公开 Domain/只读 decoder；全部 synthetic 数据，无数据库、凭据、真实 Harness、网络或 audit。

使用实际协商版 Strategy，包含 offer v2/selected v1；不按 offer 推断 wire。raw oracle 是 peer 发出的整条 JSON-RPC payload bytes，不含传输换行，不以 Value 重序列化代替。apply 只借用 Incoming。原生 tool_call/tool_call_update 的缺/错 toolCallId 可能先被 Connection 拒绝：此时如实记协议层拒绝，不伪称 apply 已返回 anchor=Failed；必要的模块形状负例另以公开可构造 Incoming 标为 L3 误用，不代签真实入口。

用 Snapshot 或单条 ItemLease 检查 retained anchor 的 key/position/512 bytes；**不跨待测 apply 持有仅为观察而取的 snapshot/lease**，避免探针自身费用改变 FIFO/预算路径。需要测保活时才显式保留并计费。独立按 offsets 解码 oracle，不让被测 decode_anchor 同时充当唯一 encoder oracle。固定只读 stats/window_status 检查 seq/缺失/真实淘汰，不为查状态额外申请 snapshot。

接口语义依据合同：ToolProjectionRead::begin/apply/raw_stats；ProjectionStep 分开 raw/anchor；decode_anchor(ViewItem, wire) 及借用 AnchorView 的 position/field/tool_status/tool_kind。允许最终实现调整函数拼写，不能调整所有权、错误、费用及下列预期。InvalidAnchor/Failed 均为显示事实，不产生控制动作。

## A 固定 512 格式和双版小枚举（L3，not-run）

操作：先从真实有效发布取得 anchor，再以独立字节 oracle 检查 magic ACPA、format1、wire1/2、entity4、flags、first/last、origin mask、status/kind、16 组 knowledge/seq/origin/loss 及零保留区。分别单独破坏 magic/version/kind/flags 保留位、长度 0..511/513、27..32 与 320..512、非法 knowledge/origin、first=0、last<first、字段 seq/loss 超 first..last、wire 不符；遍历标准 status/kind 及关键非法联合码。用公开 ViewItem 测无 key/错 namespace；只说明格式验证，不冒称来源授权。

预期：严格恰 512 字节和 little-endian；未赋值槽 seq/origin/loss=0，Tool 未实现槽9..15全 Unknown/0。初始 status/kind=255，不能将零填充当 pending/other。wire1 Known status 只0..3，code4/255拒绝；wire1 Known kind0..9。wire2 Known 标准码或255；未知字符串的255 getter 分别为 UnrecognizedInProgress/UnrecognizedOther。Unknown/Uninterpreted 必须255；status/kind KnownAbsent/KnownEmpty/Partial/Deferred、wire1 MissingRaw(255)均拒绝。wire2 未识别原值 Dropped 时 MissingRaw(255)合法且 NotRecorded；已录不承诺仍留，最多 Unverified，无 Retained 码。getter/decoder 输出只借用或固定 Copy，无原值复制；禁止任意字节损坏变成执行事实。mask 仅摘要，逐槽 origin 才是该赋值来源。

## B 版本、最小形状与一帧原子失败（L3/L4，not-run）

操作：双版覆盖 supported tags；wire1 tool_call 必需 title，tool_call_update 的省略/null；wire2 tool_call_update 和 content_chunk。对合同形状表逐项测试：ID 字符串空/4096 UTF8 bytes 合法、4097及错型；title/name、status/kind、content/locations 数组、_meta object/null；chunk content 必需 object 且 type 必需 string。对每个形状负例先建旧 anchor，再发送“前字段合法、后字段非法”的同帧；另测无旧 anchor。已知工具 tag 错版与普通非工具显示分开。

预期：进入 apply 的合法显示观察先得到 P1 唯一 raw Outcome；之后形状错误返回 Step{raw 原结果, anchor:Failed(MalformedUpdate)}，旧 anchor **所有字节/位置不变**、无新 anchor/本帧部分赋值，读取 sticky Incomplete(Structure)。错版工具 Failed(UnsupportedAnchorKind)，普通非工具显示 raw 正常、anchor Ignored。若协议入口先拒绝则保持真实错误边界，不人为放行。最小数组/对象顶层通过不表示嵌套 schema/current 已完整验证。两版 toolCallId 不规范化、不按 title/正文关联。

## C 逐字段知识、null 与未知 fallback（L3/L4，not-run）

操作：同一工具依序设置各支持槽0..8，分别省略、具体值、null；用原字节保存未知 status/kind 与大值。wire1 测四种 status、十种 kind，cancelled/未来字符串/错型；wire2 测五种 status、未来字符串/错型、null。检查 field seq/origin/loss、getter 和保留 raw 的原字符串。content_chunk 后再发 completed；混合已知 status 和不支持嵌套大结构。

预期：wire1 cancelled/未知 status/未知 kind 均整帧 Failed，无任何本帧 anchor 赋值，不借 wire2 fallback。wire2 未知字符串小码255、保守显示 in_progress/other，原 raw 原值未改。其余按以下有限矩阵逐格断言；R=Recorded、D=Dropped：

| 有效输入/槽 | R knowledge | D knowledge | 小码或边界 |
| --- | --- | --- | --- |
| 省略（v1 initial name例外）；v1 update name/rawInput/rawOutput:null | 保持 | 保持 | knowledge/sequence/origin/lastLoss及小码全部不变 |
| 本版已理解 status/kind 字符串；v2 status/kind:null | Known | Known | 标准小码；v2 null分别pending/other |
| v2未来 status/kind 字符串 | Known | MissingRaw | 小码255；getter Unrecognized，R为Unverified、D为NotRecorded |
| v1 initial _meta:null；v1 update title/status/kind/content/locations/_meta:null | Uninterpreted | Uninterpreted | status/kind码255，不把旧小值当确定结果 |
| name/title具体字符串 | Known | MissingRaw | 不复制字符串入anchor |
| v1 initial name省略/null；v2 name/title/rawInput/rawOutput:null | KnownAbsent | KnownAbsent | 明确无值/清除，小事实不依赖raw |
| v2 content/locations/_meta:null | KnownEmpty | KnownEmpty | null清空；不等同任意JSON null都清空 |
| content/locations数组（包括[]）、entity_meta对象、rawInput/out具体JSON（含v1 initial显式null） | Deferred | MissingRaw | v1 update rawInput/out:null、v2 null优先按前述行 |
| v2 content_chunk.content | Deferred | MissingRaw | 仅槽0赋值；chunk _meta/其他扩展不patch实体槽 |

每个**赋值**格 field_sequence=q、field_origin=本Incoming标签；D置该槽lastLoss=q，R保留既有lastLoss。**保持**格四项/小码均保持；即使整帧Dropped也不冒称未触及字段丢失。整帧最小验证成功才统一应用，header.last=q、origin mask OR本次标签。has_raw_loss恰等于任一槽lastLoss>0；has_uninterpreted恰等于当前任一Uninterpreted，后续明确赋值可解除；base_unavailable创建为1且更新保持。交替 D→R 与 Uninterpreted→Known、clear→省略、未触及字段来源均逐项检查，不把历史loss位随成功清掉。本包不产生Partial；content_chunk可建缺基础anchor，之后completed仍同位。

上表已对齐最终81d0 §4A，包含v1 initial _meta:null；TG01仅定义层关闭。固定SDK2.1.0 v2 ToolCallUpdate明确省略保持、null清空、集合null/[]清空，_meta对象按top-level patch、null清空；content_chunk._meta明确仅chunk scope不patch实体。因此KnownEmpty清除与chunk不动entity_meta有直接依据。[]及非null对象暂Deferred只表示本包未解释current，不声称SDK将[]视为非空。v1 schema对部分null仅给可接受形状，不推断未确认值语义，Uninterpreted是本包明确限制。P2A field Known仅证明曾观察可解释赋值，不等于任意长值当前可读；P2B须按真实窗口另验，不能回退旧值。

## D 257 工具、长 raw 与 late completion 原位（L4，not-run）

操作：在足够预算、无真实淘汰时依次创建257个不同 toolCallId（另有4096字节长ID），交错文字 raw；记录首工具原 position，再在第257个之后发首工具 completed。分别使首工具更新 raw >256KiB、含未知大扩展，并超过旧 single_update/RawOnly 阈值；使用 raw 因预算 Dropped 而旧 anchor 仍在的对照。通过真实 replay→标准响应→Live、单独 resume→Live 输入同工具更新。

预期：257及长ID不触发256表/LRU/softfreeze；定位用原生 Tool/id，始终旧 anchor position，不移到文字之后。晚 completed、raw长或 Dropped 不冻结原位小状态。每次 apply 的 raw_stats 仅前进一 seq/一次丢弃计数；对应保留 raw 的序号/原 bytes/origin精确，anchor first 保留、last及被触及字段 seq/origin推进，未触及字段来源不改。Replay/Resume 后 Live 不归零、不释放read；origin mask 是摘要，不把所有内容改成 Live，不把 DuringReplay 当历史证明。首次工具总 base_unavailable=1、CreatedPartial，不以“完整 tool_call”证明原生初建。

## E raw 缺失、同帧自淘汰与不伪可读（L3/L4，not-run）

操作：独立夹具：①P1 Recorded 后，同一 apply 的新 anchor insert 为取得费用真实 FIFO 淘汰刚录 raw；②先 Recorded name/title/未来 status/kind，后续普通观察使该 raw 淘汰而 anchor 仍在；③raw超限 Dropped、旧 anchor仍在、更新具体长字符串及标准/未来小状态；④raw失败但下一小帧继续。每次通过实际 retained key/seq 检查 raw 是否真的存在，不以 Outcome::Recorded 或锚点存在推定。

预期：①仍如实返回 raw Recorded，但它不等于最终 retained；metadata 标准小 status 仍 Known可读，name/title的Known仅观察事实，P2A不承诺值可读。②旧 metadata 不伪改为 Retained/免费保活；未知 getter 的 Unverified不谎称原字符串在。③具体字符串 MissingRaw、last_raw_loss记录该次seq；未来小码255用 MissingRaw+NotRecorded与保守fallback；标准内联小码仍 Known，附真实loss。④本次raw结果和anchor失败/成功各自保留，后续正常继续、seq不复用；缺失状态不被标准响应或后续成功洗白。不在P2A假造P2B“已解析current/MissingRaw值读取”的动态通过。

## F 真实 FIFO 失锚点与禁止失败换位（L3/L4，not-run）

操作：控制预算使P1 raw insert **先实际移除旧 anchor**，再同工具 completed；与旧anchor仍在、但item_lease引用容量不足、replace候选缺1字节、已损坏512 anchor三组对照。分别填满引用账本、保留必要snapshot/旧对象并计费；观察position和实际evicted水位。用公开Domain造损坏Tool/id条目，不将损坏当MissingItem。另测raw insert已淘汰若干旧项后，anchor阶段仍失败。

预期：仅真实MissingItem可一次stage_insert，返回新的CreatedPartial位置、first=last本seq/base_unavailable=1，不回收已淘汰的旧位置。lease Capacity、损坏、Stale/Conflict不当“没找到”：anchor Failed、没有另位置fallback/第二次observe/重试。旧anchor仍在的replace失败保留原字节/位置，不隐式淘汰来给候选腾位。raw阶段真实淘汰不能被失败后声称撤回；raw Outcome仍在Step，window_status给真实水位。对同步apply内部CAS竞态没有公开暂停口：不加生产hook/并发假象，内层CAS/ABA实际原资产回归、P2A调用路径静态核对单次消费旧lease；报告区分这两类证据。

## G 同账本峰值、失败、scope 与封装（L3/L4，not-run）

操作：从root8192/identity/window基线算usage：read=P1的1536+P2额外1024，长期0items；P1每帧768结束后才anchor阶段；anchor stage1024、候选512+ID实际capacity+512/1、旧ItemLease320/1及旧object并存。分别新建/替换在关键峰值缺1/恰足，长ID/多工具/保留snapshot、release/新resource/read后保留旧wrapper再Drop；begin第二步预留失败。公开测size_of固定字段，定向bulk allocator测试只针对可隔离fallible分配；元数据global OOM边界不扩充。scope:另一容器/源/旧generation/旧read。Ignored/Err/Recorded/Dropped各分支比seq和wire计数；负编译尝试Clone read、从DTO/anchor重建owner、借用AnchorView逃逸为static。

预期：先收费后拥有，payload固定512、没有Value/String/Arc clone/全历史索引；1024额外持久空间足够实际wrapper固定字段。begin失败不留P1 workspace费用；旧read/status cell/snapshot按原目标节点留账至最后Drop，旧失败不污染新read。每帧最多一次lease/候选/publish；raw frame与anchor frame不免费并存。显示失败只本read Incomplete，peer仍可处理权限/State/其他目标/响应，零cancel/close/prompt/控制写；Ignored零本read序号副作用，raw.observe的Err原样Observation包装，不伪Step。Stale/binding错误不污染新scope。不能以非Clone证明已实现实际Connection token/actor授权；当前来源保证仍外层owner责任。Checked极值如无普通API可达路径只列静态/原P1证据，绝不把未执行极值算通过。

## H 同版回归、差异和交付范围（L3/L4，not-run）

操作：源/锁/资产前后摘要；本包新公开API/真实wire探针、严格clippy/fmt，按变化判断重跑原P1 revision2的11项、既有公共69项、预算20+allocator6；P0真实ID行为、发现17/权限12若未包含在所选二进制则显式补跑并去重统计。保留首次失败和原预期；原资产owner的同版适用证据可引用，不能用开发测试代签。审查实现diff不改Domain/P1预算语义、SQL/锁/SDK，不引入私有Harness解析或新namespace。

预期：记录真实执行数量/0忽略/退出码、源快照和逐A–H子断言状态；编译负向有新API正控制且错误确因被测边界。原L5/L6级别/完整CL义务保留，本包通过只关闭有证据的工具锚点子范围。首开无后续输出/满4096 delivery、current值解析和raw scanner、完整SDK嵌套字段、可信轮次/分页、真实owner/撤销/WSS/Web/真实Harness仍not-run。无网络/audit/用户数据访问。本定义完成不代表产品验收通过。

## 编码前具体余缺口

**P2A-TG01 closed-definition（C）：精确content/null槽码。** 原790fff中content_chunk的Deferred/Partial二选一、v1 update content/locations/_meta:null四项转移未确定，已由主执行8e4e §4A固定R/D逐行表；81d0进一步补齐v1 initial _meta:null=Uninterpreted。C组按最终表冻结：chunk仅content槽R Deferred/D MissingRaw，本包不产Partial；NoOp四项不变、赋值q/origin、D记录loss而R保留旧loss；v2 null按已确认语义KnownEmpty/Absent，小标准码始终Known，flag从当前槽重算。已独立核对固定SDK两版形状及v2清除/chunk scope，未见与有限知识范围冲突；v1未确认null保留Uninterpreted而非猜clear/保持。没有用户新增产品决策、没有实施结果或动态通过。

**可测性界限，不新增产品阻断：** 同步独占apply内部CAS与u64极值无公开触发入口；以对应Domain动态原资产+固定源码检查补足，并在正式报告列未独立动态执行。RawEvidence故意没有Retained，P2B尚未实现，E组只证明真实raw存在性和小事实边界，不制造值读取API。metadata Arc/list全局OOM继承既有边界，不要求进程级所有分配恢复。当前无本tester尚未关闭的定义缺口；整体编码门槛仍交原reviewer独立决定，本tester不以本定义代签实施通过。
