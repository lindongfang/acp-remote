> Q-001（2026-10-31）：本文保留为既有实现子合同，非本轮新接线依据。与当前specs/design冲突的正文、输出、实体/快照容量预算、重放门槛、签名期限及派发再验原actor均已停用；旧测试仅说明原范围。非冲突协议事实可复用，但须按新阶段独立复核。

# P2A：512字节工具anchor与单次raw组合合同候选 revision5

状态：2026-10-31，仅独立预审输入，未编码。本版仅局部精化r4的版本枚举、可观察kind、raw仍保留与最小字段形状；512 offsets/调用次数/预算不变。不签P2B完整current/scanner、首开可读、WSS/真实owner；r4 deb54a5e/旧5dd9/c978/455均不改。

## 1. 可编译API候选

```rust
pub struct ToolProjectionRead { /* private: ObservationRead, ViewHandle, Version,
                                  P2 owner metadata, StageWorkspace */ }
impl ToolProjectionRead {
    pub fn begin(w: &mut ViewWindows, h: &ViewHandle, s: &Strategy)
        -> Result<Self, ProjectionError>;
    pub fn apply(&mut self, w: &mut ViewWindows, input: &Incoming)
        -> Result<ProjectionStep, ProjectionError>;
    pub fn raw_stats(&self) -> ObservationStats;
}
pub fn decode_anchor(item: ViewItem<'_>, wire: Version)
    -> Result<AnchorView<'_>, ProjectionError>;
pub struct AnchorView<'a> { /* only &'a [u8], borrowed key and position */ }
impl AnchorView<'_> {
    pub fn position(&self) -> u64;
    pub fn field(&self, field: AnchorField) -> FieldFact;
    pub fn tool_status(&self) -> ToolStatusFact;
    pub fn tool_kind(&self) -> ToolKindFact;
}
```

ProjectionStep为Copy{raw:ObservationOutcome,anchor:AnchorOutcome}；AnchorOutcome=Ignored|Updated{position}|CreatedPartial{position}|Failed(ProjectionError)。ProjectionError为Copy错误枚举：Observation(ObservationError)、View(ViewError)、InvalidAnchor、MalformedUpdate、UnsupportedAnchorKind、ScanBudget、MissingCurrent（后两项供P2B；P2A不模拟触发）。错误不携正文/ID/路径。apply的Err仅表示raw.observe未取得正常Outcome或绑定/lifecycle错误；有raw结果后anchor阶段失败仍返回ProjectionStep::anchor=Failed，不能丢掉raw是否已录入的事实。显示失败按原read fail(Incomplete)，无网络动作。

ToolStatusFact=Unknown|Pending|InProgress|Completed|Failed|Cancelled|UnrecognizedInProgress{sequence,raw:RawEvidence}|Uninterpreted。ToolKindFact=Unknown|Other|Read|Edit|Delete|Move|Search|Execute|Think|Fetch|SwitchMode|UnrecognizedOther{sequence,raw:RawEvidence}|Uninterpreted。两者均Copy，只是显示知识，不是Run结果或授权。UnrecognizedInProgress/UnrecognizedOther仅wire2合法，分别明确显示fallback为in_progress/other，不把原值改写成这些标准字符串。

FieldFact仍为Copy{knowledge,sequence,origin,last_raw_loss}；RawEvidence=Unverified|NotRecorded，**没有Retained码**。NotRecorded只说明该次P1 Dropped；Unverified即使曾Recorded也不保证当前raw还在，必须由P2B按seq/原生key在实际读取窗口确认。origin是Unknown/Live/DuringResume/DuringReplay小枚举。

只有begin内部创建的私有wrapper，不能从查到anchor/raw DTO重建读取所有者。Incoming未含私有connection token，实际Connection来源仍由持有它的owner负责；本包不把caller声明的target/version当grant。

## 2. 固定512 payload线格式

所有整数little-endian；下列offset精确、无对齐填洞。恰512字节，尾部/保留位必须零；未知tag/version/非法码拒绝InvalidAnchor。外层Domain key必须Tool(namespace)+实际toolCallId，本包不新增namespace。payload不存ID字符串，身份由外层scope/key提供并与输入toolCallId精确核对。

| 偏移（左闭右开） | 编码 |
| --- | --- |
| 0..4 | ASCII ACPA |
| 4 | format_version=1 |
| 5 | wire_version=1或2 |
| 6 | entity_kind=4（Tool；其他kind由P2B另定，不在本包暗示支持） |
| 7 | flags：bit0=base_unavailable，bit1=has_raw_loss，bit2=has_uninterpreted；其余0 |
| 8..16 | first_sequence，非零 |
| 16..24 | last_sequence，>=first_sequence |
| 24 | observed_origin_mask：Live=1、Resume=2、Replay=4、Unknown=8；仅摘要 |
| 25 | tool_status_code，见下文 |
| 26 | tool_kind_code，见下文 |
| 27..32 | 零 |
| 32..48 | knowledge[16]，每槽u8 |
| 48..176 | field_sequence[16]，每槽u64 |
| 176..192 | field_origin[16]，每槽u8：0未知/未赋值，1Live，2Resume，3Replay |
| 192..320 | last_raw_loss[16]，每槽u64，0表示未记录 |
| 320..512 | 零；不当免费动态字段区 |

槽表：0=primary content（Tool content），1=entity_meta，2=name，3=title，4=kind，5=status，6=locations，7=rawInput，8=rawOutput，9=command，10=cwd，11=exitStatus，12=size，13=plan，14/15保留。Tool只适用0..8；9..15本包必须初始Unknown/seq0/origin0/loss0。这些槽给完整P2保留明确字段空间，不表示本包实现terminal/plan/compaction语义。

知识码：0Unknown，1Known，2KnownAbsent，3KnownEmpty，4Partial，5Uninterpreted，6MissingRaw，7Deferred。Deferred明确尚由P2B实现，不能输出为current成功。字段无赋值时seq/origin为0；非零seq必须在first..last内，loss为0或first..last，解码超界拒绝。Known表示已观察到可解释赋值，**不保证可变原value仍可读取**：name/title等任意长度值必须由P2B按seq在同scope的实际窗口确认raw及字段；找不到则读取结果MissingRaw，不能退回更早值。P1 Recorded后，anchor新建本身就可能FIFO移走该raw，后续更新/淘汰也可能移走；不要求依靠免费引用把raw永久钉住。内联小status/kind码不依赖raw继续存在，可保持Known。

status码：0pending、1in_progress、2completed、3failed、4cancelled（仅wire2）、255未识别/无已理解值。kind码：0other、1read、2edit、3delete、4move、5search、6execute、7think、8fetch、9switch_mode、255未识别/无已理解值。初始化两码均255，不把零填充解释成默认pending/other。

decode/Fact按wire和knowledge联合验证：wire1 Known status仅0..3，Known(code4)及Known(code255)一律InvalidAnchor；wire1 Known kind仅0..9。wire1的未知字符串不属于schema枚举，不采用wire2 fallback，也不能编码成Known opaque。wire2 Known status可0..4或255，Known kind可0..9或255；255表示实际未识别原字符串，getter返回UnrecognizedInProgress/UnrecognizedOther，原字符串另由raw读取。raw录入失败时wire2仍保存255及seq、knowledge=MissingRaw，getter可报告同一保守fallback及RawEvidence::NotRecorded，不谎称原值可用。status/kind的MissingRaw(code255)仅wire2未识别原值丢失合法；wire1不接受该组合。Unknown/Uninterpreted必须码255；status/kind的KnownAbsent/KnownEmpty/Partial/Deferred不是本包合法组合，解码拒绝。标准内联码即使raw丢失仍用Known，不退成MissingRaw。

## 3. 逐帧组合与最多尝试

begin先ObservationRead::begin（既有1536 bytes/0 items），另为P2固定wrapper元数据取得stage_workspace(h,512)（1024 bytes/0 items）。实际额外Rust字段size_of须落在这512收费空间，不复制Strategy初始化raw/原生ID。read继续跨replay响应→Live，实际view释放/换代才Drop；旧read费用仍属原目标/节点。

apply同步独占&mut ViewWindows，不await：

1. **恰调用一次**现有ObservationRead::observe(w,input)。Ignored直接回Ignored，Err原样包Observation；Recorded/Dropped的sequence是唯一seq，不能自己再增计数、猜下一seq或再次observe以重试。
2. 只借用input.raw.fields解析协商版工具分支：v1 tool_call/tool_call_update；v2 tool_call_update/tool_call_content_chunk。仍执行下表最小形状校验，不clone Value/String/Arc。已知工具tag用错版返回anchor=Failed(UnsupportedAnchorKind)；其他显示种类anchor=Ignored。不因P1已录raw就声称字段已合法或raw永久保留。
3. 通过现有item_lease(h,SourceKey{Tool,id})查当前anchor，最多一次正常查找。只有MissingItem允许新anchor；Capacity/Stale/Conflict/损坏绝不能改称缺失后换位置。该查找是<=4096保留项线性查找，无LRU/全历史索引。
4. 有anchor：另取stage_workspace(h,512)（1024 bytes/0 items），消费lease给stage_replace(payload=512)，旧/新对象及320/1租约同时收费；复制/编码512头在已收费scratch中，publish沿用原position。raw内容长、Recorded还是Dropped、旧256KiB阈值/single_update/raw-only均不冻结anchor。
5. 真MissingItem：stage_insert(Tool/id,payload=512)一次，first=last=当前seq，base_unavailable=1。即使是完整tool_call，也只称本代际首次观察，不能证明原生新工具/原始位置。新anchor插入可能FIFO淘汰，真实水位由window_status取得。
6. 一帧anchor阶段最多一次lease、一份候选和一次publish；无replace→另位置insert fallback。由于raw已经先完成，后续同步anchor步骤之间无并发修改；CAS冲突/资源换代是失败，不猜重试。P1 raw插入若先真正FIFO移除了旧anchor，步骤3是实际MissingItem；这才是新缺基础anchor的合法边界。费用不足直接Incomplete/Failed保留旧值，不承诺OOM/容量满仍能更新。

每帧除既有P1 frame768之外，只增加anchor stage1024、候选512+keylen+512/1及成功lookup的lease320/1；P1 frame已结束Drop后才进入anchor，不能计成免费并存。计费/actual capacity/全局allocator OOM边界继承Domain，不引入堆分配的anchor DOM。

## 4. P2A有限字段行为

- v1 status字符串仅pending/in_progress/completed/failed，kind仅码表0..9的原字符串；cancelled或其他未知字符串是MalformedUpdate，原raw保留/缺失据实说明，**不提交任何本帧anchor赋值**、不走v2 fallback。v2新增cancelled；未来status字符串以255编码、显示in_progress，未来kind字符串以255编码、显示other，原值仍在原raw而非被fallback替换。v2 status:null→Known pending、kind:null→Known other；v1 update中对应null未核实→Uninterpreted，初始tool_call不能发这类null。省略保留旧字段知识，初始缺省不擅自填默认值。
- name/title具体字符串：P1 Recorded→Known及seq/origin，**仅表示观察时有该原值，不保证其raw现仍保留**；Dropped→MissingRaw+loss seq。v1 tool_call初始name省略/null表示无名称，和tool_call_update的name省略/null保持旧值明确区分；v2 name/title:null→KnownAbsent；v1 update title:null→Uninterpreted。P2A不返回任意长度字符串，P2B必须实际按seq查证。
- rawInput/rawOutput具体值与非null数组/meta/content等大结构，本包只记显式触及seq/origin；Recorded为Deferred，Dropped为MissingRaw/loss，**不签已合并current**。v1 tool_call_update已确认rawInput/rawOutput null保持；v1 tool_call初始显式null作为原始JSON载荷留Deferred，不借更新规则变成省略。v2 rawInput/rawOutput:null为KnownAbsent，content/locations/_meta:null为KnownEmpty，不需复制原大值。
- v2 tool_call_content_chunk只使content按下表进入Deferred或MissingRaw及last seq推进，status/kind保持；本包不产生Partial。它仍可建立缺基础Tool anchor，后续status在同一原位。v1同tag未知，不建anchor。
- 任意raw Dropped但小status/kind可理解时，仍原位更新该小控制值Known，同时last_raw_loss记录原观察缺失；真实P1逐观察origin/seq不被anchor单标签替代。已知current小值不赋予执行权。

最小形状校验在任何anchor候选/赋值前完成；失败为anchor=Failed(MalformedUpdate)、原读取Incomplete(Structure)，旧anchor零改变。raw.observe已执行的Recorded/Dropped事实仍回传；不主动关闭/cancel连接。

| 实际tag/字段 | P2A必须校验；Deferred边界 |
| --- | --- |
| 全部受支持工具tag | toolCallId必须字符串且UTF-8<=4096（空串沿现有opaque ID规则，不自加非空限制）；字段错类型/无ID不建anchor成功。 |
| v1 tool_call | title必需字符串；name省略/string/null；kind/status省略或本版枚举字符串，不能null。content/locations若出现必须数组，_meta若出现必须object/null。rawInput/rawOutput任意JSON。 |
| v1 tool_call_update | title/name省略/string/null；kind/status省略/本版枚举字符串/null。content/locations省略/array/null，_meta省略/object/null；null语义按本稿已证/Uninterpreted区别。 |
| v2 tool_call_update | title/name省略/string/null，kind/status省略/任意string/null；content/locations省略/array/null，_meta省略/object/null；rawInput/rawOutput任意JSON。 |
| v2 tool_call_content_chunk | toolCallId及content必需；content必须object且type为string，_meta省略/object/null；没有title必需条件。 |
| 大字段/嵌套内容 | 数组顶层与本包实际读取的字段形状已检；ContentBlock/ToolCallContent/locations内部元素、_meta键值语义及完整派生current仍Deferred给P2B。不得把Deferred报成已通过全部schema或完整字段支持。 |

本稿本地复核SDK2.1.0 schema/v1与v2的$defs.ToolCallStatus/ToolKind/ToolCall(required toolCallId,title)/ToolCallUpdate；v2 unknown status规范描述明确still in progress，unknown kind明确显示other。固定schema SHA沿原候选：v1=1c57095acf9c0844054dd1fdc3624c121b3fdc24209ecc987fd6748c705fdef5，v2=28c7251e820511da257cdb55d30e2c30bbbe59226305b04e3b751dc655e0778d。未访问网络。

### 4A. P2A精确槽转移（主执行补齐编前TG01）

下表在整帧最小形状验证成功后一次性应用。R表示P1 Recorded，D表示Dropped；q/origin为该次P1结果序号及原Incoming标签。赋值行统一写field_sequence=q、field_origin=origin；D同时令该槽last_raw_loss=q，R保留原last_raw_loss，不清除历史丢失事实。保持行四项及小码均不变。header.last_sequence仍推进q，origin mask加入本次标签。新anchor未赋值槽全Unknown/seq0/origin0/loss0；status/kind小码初始255。

| 版/输入及槽 | R后knowledge | D后knowledge | 小值及边界 |
| --- | --- | --- | --- |
| 任意受支持工具帧省略字段（v1 initial name另行）；v1 update name/rawInput/rawOutput:null | 保持 | 保持 | 不因整帧raw丢失而改动没有语义赋值的字段 |
| 本版已理解status/kind字符串；v2 status/kind:null | Known | Known | 写本版小码；v2 null分别pending/other，小值不依赖raw继续存在 |
| v2未知status/kind字符串 | Known | MissingRaw | 小码255；对应UnrecognizedInProgress/UnrecognizedOther，原字符串是否可读另证 |
| v1 update title/kind/status/content/locations/_meta:null；v1 initial _meta:null | Uninterpreted | Uninterpreted | 不猜保留/清空；status/kind小码改255，旧小值不能冒充本次已确定结果 |
| name/title具体字符串 | Known | MissingRaw | 不复制字符串进anchor；仅记录赋值事实与来源 |
| v1 initial name省略/null；v2 name/title/rawInput/rawOutput:null | KnownAbsent | KnownAbsent | 明确无值/清除事实不依赖raw；v1 update的name:null不走本行 |
| v2 content/locations/_meta:null | KnownEmpty | KnownEmpty | 明确清空；非null数组（包括[]）/对象在本小包仍按下一行Deferred，不冒签完整内容解释 |
| content/locations数组、entity_meta对象、rawInput/rawOutput具体JSON（含v1 initial显式null） | Deferred | MissingRaw | 不声称已经合并；v1 update的rawInput/rawOutput:null先按保持行；v2对应null先按清除行 |
| v2 tool_call_content_chunk.content | Deferred | MissingRaw | 仅槽0赋值；chunk的_meta及其他扩展字段只在原raw中，绝不patch实体_meta/status/kind/name等槽 |

本包对槽0/1/6不产生Partial；代码4保留给已定义的后续字段解释，不能用其替代本表确定结果。已支持的工具update以外字段不触及任何槽。v1未知枚举/任何已规定错型在验证阶段失败，不能部分应用上述行。

has_raw_loss按当前16槽任一last_raw_loss>0计算（观察登记的历史丢失，不推断后续FIFO是否仍保留raw）；has_uninterpreted按当前任一knowledge=Uninterpreted计算，可随后续明确赋值解除；base_unavailable在本包新建时为1，更新时保持。P2B判断当前值是否可读仍必须实际查raw，不能仅凭这两个摘要位。

## 5. 最小独立门槛

257个及长ID工具都仍留时，首工具late completed原位；原raw>256KiB/RawOnly或丢弃而anchor仍留，同预期原位。对照真FIFO失anchor才CreatedPartial。Replay→Live同anchor小状态与每条P1原观察origin/seq精确；mask只摘要。另必须测wire1 Known(code4/255) decode拒绝、v1 cancelled/未知status及未知kind不提交anchor、v2 unknown status/kind各自fallback且原值不改、tool_kind getter、P1 Recorded后同帧anchor insert淘汰raw及后续淘汰时任意长度值不得伪可读、内联status仍Known、缺title/错类型/错版零anchor。512 offset/非法码/保留位、实际峰值/size_of、短写/Capacity/CAS、scope及Counter错误逐项验证；原P0/P1/Domain回归保持。P1原raw知识不代签P2B。

当前未实现/未验证；尤其满4096窗口时ItemLease也需1引用，可能Capacity，本包**不签首开在途可读**。P2B另提零分配借用读取及分块一致性合同，不修改当前冻结Domain/P1。真实owner授权/来源装配、完整SDK字段current、可信轮次/WSS/渲染仍是完整change待完成义务。
