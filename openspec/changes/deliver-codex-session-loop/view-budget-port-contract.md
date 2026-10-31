> Q-001（2026-10-31）：本文保留为既有实现子合同，非本轮新接线依据。与当前specs/design冲突的正文、输出、实体/快照容量预算、重放门槛、签名期限及派发再验原actor均已停用；旧测试仅说明原范围。非冲突协议事实可复用，但须按新阶段独立复核。

# Domain视图预算端口独立实施合同候选 revision2

状态：仅设计候选，待独立tester定义/reviewer预审；不授权ACP投影、真实装配或编码。2026-10-31。由外部45528b1c候选复制后，主执行明确标准库全局OOM边界，外部原稿不改。本合同可独立实施验收，不依赖ACP片段如何合并。

## 1. 范围与既有不变量

基线为domain/views.rs现有纯容器及view-window-contract；它不含JSON、ACP/scanner、消息/工具语义、执行、权限或网络。仍按当前ViewHandle的容器scope、NativeSessionKey、ResourceEpoch、读取generation校验。目标16MiB/4096、节点128MiB/32768、最多64预算身份；旧代际/快照/候选/引用合并计费。保留FIFO新增与原位替换，长流不因一次淘汰永久停止。

新增只解决：借用一条旧值并持有可靠比较身份、预留工作区、预留完整候选后才构建、直接移动发布、不靠snapshot查询失败后的水位。不得用本合同签实际协议保真/分页/慢订阅/真实Harness。本包明确不新增adapter ProjectionState/ProjectionRead及其1024费用，不实现JSON envelope、scanner或134217728扫描上限；这些不是遗漏，属于后续独立包。

## 2. 最小API与类型

以下为拟定可编译形状，最终名称可统一；参数所有权、预算和错误语义不可删减。所有拥有型对象不可Clone，不公开Arc、Charge、可clone底层buffer或从查询重建的写token。

```rust
// 全部是domain类型，字节不解释为JSON。
fn item_lease(&mut self, h: &ViewHandle, key: SourceKey<'_>)
    -> Result<ItemLease, ViewError>;
fn stage_workspace(&mut self, h: &ViewHandle, scratch_bytes: usize)
    -> Result<StageWorkspace, ViewError>;
fn stage_insert(&mut self, h: &ViewHandle, work: StageWorkspace,
    key: Option<SourceKey<'_>>, payload_bytes: usize)
    -> Result<StagedItem, ViewError>;
fn stage_replace(&mut self, h: &ViewHandle, work: StageWorkspace,
    old: ItemLease, payload_bytes: usize)
    -> Result<StagedItem, ViewError>;
fn publish(&mut self, h: &ViewHandle, staged: StagedItem)
    -> Result<PublishFact, ViewError>;
fn abort_stage(&mut self, h: &ViewHandle, staged: StagedItem,
    failure: ViewFailure) -> Result<(), ViewError>;
fn window_status(&self, h: &ViewHandle) -> Result<WindowStatus, ViewError>;

impl ItemLease {
    fn item(&self) -> ViewItem<'_>; // 仅借用
}
impl StageWorkspace {
    fn scratch(&mut self) -> &mut [u8];
}
impl StagedItem {
    // 按字段拆借用，允许扫描旧bytes时向独立候选写入，不clone旧值。
    fn parts(&mut self) -> StageParts<'_>;
}
```

StageParts只含借用的scratch、可选旧ViewItem及限长BoundedWriter；不携带新拥有型复制。BoundedWriter支持write_all(&[u8])->Result及written/remaining，不能seek/grow/取出Vec；每次write是全写或零写。StagedItem不能绕过writer直接修改底层内存。PublishFact/WindowStatus为固定大小Copy值，不包含String/Vec/Arc/target克隆。

WindowStatus至少含state、当前generation、最早/最晚保留position、next_position、evicted_through、evicted_count、retained_items。查询零新堆分配、零新预算/引用，校验与snapshot相同scope；错误后即使没有snapshot空间，也能取到累计淘汰事实。PublishFact至少含本次position、created/replaced及同样的累计淘汰水位。调用者不依赖Err携带完整窗口。

## 3. ItemLease及替换比较

item_lease只在线性扫描的当前保留条目中匹配(namespace,id)。成功先预留320字节（256元数据+64引用）及1引用，再clone现有item Arc进私有lease；不clone bytes/key，不建索引，不冻结整个窗口。MissingItem/InvalidKey/Stale零新租约。旧对象原始item费用另仍存在。

lease绑定原容器scope、目标、resource、读取generation、私有item Arc身份及position。位置/key相同不代表同一对象；同key经历一次replace后，旧lease即不能替换新的对象。真正比较使用仍由lease强引用持有的私有Arc身份，不能拿调用者提供的position/ID充当比较成功。比较不是只保存裸地址：旧Arc分配在lease被消费进stage后仍一直存活，直到比较结束才释放，故淘汰后同key/相同bytes新insert不能复用该分配，防止ABA；不得改成Drop旧Arc后再比较裸指针。此约束须由源码与实际交错验收共同核对。

stage_replace **按值消费**ItemLease，沿用其中已计费的唯一引用；caller此后不能Drop或重用该lease。StagedItem保留它到publish/abort/Drop，没有第二个免费Arc引用，亦不自动再收第二份320费用。仍需额外lease的调用者只能重新调用item_lease并再次付费。独立取得的多个lease可读同一旧对象；其中一个发布替换后，其他旧lease发布必须Conflict，不能覆盖。

stage_replace预留前重验lease属于当前handle及当前同key/item；publish再次比较，防止构建期间被其他替换改变。不能从snapshot/ViewItem/外部字段创建ItemLease。候选保留原position和原key，不允许caller换key/目标来重解释旧lease。

## 4. 工作区、候选与读状态寿命

费用候选：

| 对象 | 同时计目标及节点bytes | items |
| --- | --- | --- |
| StageWorkspace | 512固定头上界+显式scratch_bytes容量；标准7680请求共8192 | 0 |
| ItemLease（包括被replace stage消费的那份） | 320 | 1 |
| 候选item（key只复制一次） | 512 + key UTF-8长度 + payload capacity | 1 |

Domain工作区显式申请scratch_bytes，收费512+scratch_bytes，返回精确该长度的opaque scratch；零长度仍收512，不提供grow。原独立定义S=8192对应标准scratch请求7680，其他请求按同公式，既定目标/节点总限额不变。512为候选固定头的待编译核实上界。Domain不规定scanner栈或JSON深度，不提供另一免费池。stage_workspace先checked收费再分配；预检如果只需计数，也必须先取得工作区，不能先分配临时结构再纳账。512头实际若超上界，或实现还要额外capacity/临时峰值，须先补合同与独立期望，不能减scratch长度或事后补费冒称已覆盖。scanner100层的实际布局不由本包证明。

StagedItem继续持有工作区费用直到publish/abort/Drop，writer完成不提前退scratch账。发布成功后候选item及其Charge直接移入窗口；工作区和旧lease同时Drop；旧item若仍有snapshot/其他lease则继续收费，最终Drop才退账。失败/取消Drop释放本阶段所拥有的全部候选/scratch/lease，没有脱离owner任务或外部副作用。

为确保writer失败或被丢弃不会悄悄得到“完整读取”，拟将每窗口现有state与WINDOW_BYTES费用移入私有ReadStatusCell，由窗口及已授予的workspace/stage共同引用。该cell属于一个固定读取scope/generation；不是全局运行时或另一个budget。WINDOW_BYTES费用随最后引用Drop才退账，因此释放窗口后还活着的stage不能免费保留它。Arc/锁/cell及stage字段必须在原窗口1024/工作区头512等保守费用内取得布局证据；未证明则编码前追加计费细化，不能假定覆盖。

- workspace预留/分配失败：若handle当前，原读取标Incomplete(Capacity)，不创建stage。
- 正常丢弃尚未转换成候选的workspace：只退账，不表示一条来源更新已被接受；需要报告预检失败的caller显式fail原读取。
- stage授予后：有且只有一次publish/abort。任何writer错误、短写后publish、显式abort或未publish直接Drop，利用scope绑定的ReadStatusCell标记原读取Incomplete；首个原因sticky。即使已release/换代，也只影响旧cell，不能污染新读取。
- stage成功publish解除其Drop失败标志。writer借用结束但stage尚未publish时不提前清除责任。
- ReadStatusCell不改变snapshot的冻结语义：已取得快照记录当时状态；当前快照读取cell当前状态。finish只接受原owner完成，不能擦除任何Incomplete。

这是一项明确的内层布局增量，尚无size_of/真实编译证明。实现者不得在报告中把上述512/1024当成已实测；若root选择更小等价布局，须独立检视确认所有仍活引用和Drop失败状态均有同账本费用。ReadStatusCell控制引用不是item/snapshot引用，不另占items；其元数据引用存储必须落在已经收费的窗口/工作区固定头中，不能隐藏额外免费分配。

## 5. 预留及构建

### 5.1 stage_insert

先校验handle/key长度/重复键、payload大小及checked费用；单候选不可能容纳于目标预算时直接Capacity，不能先把窗口清空。合法候选只有在同目标与节点账本均成功预留后，才分配payload/key。不足时沿现有FIFO弹出本窗口最早条目重试，直到成功或无可淘汰条目；snapshot/lease持有的旧对象仍收费。不能淘汰其他目标、退掉旧租约费用或新开预算身份绕限。

预留期间可能已经淘汰一些旧项；后续分配失败返回Capacity并标Incomplete，**不能声称原窗口未变化**。每次淘汰立即checked更新水位/计数，window_status随时可见，查询不分配snapshot。不因一次淘汰把后续insert永久关闭。Duplicate key Conflict零淘汰；非法key/溢出在可能计算处尽早拒绝。

stage_insert此时不分配position、不插入候选半成品。多个已预留未发布insert可能同时存在，全部收费；publish前再次查重复key。同key抢先发布后，其他候选Conflict并Drop，不做replace。可选择限制每读取只有一个开放stage，但若这样做，必须显式Busy且不误标读取失败；不能靠未计费队列等它。

### 5.2 stage_replace

不淘汰任何条目。旧对象/被消费lease、工作区及完整候选同时收费，容量失败保留当前原值和位置。旧lease比较失败Stale/Conflict时不修改新对象；候选未能预留不得先释放原item以挤出空间。

### 5.3 writer及分配

payload_bytes是精确最终长度。候选writer只能在固定已预留capacity内顺序写；超长为新有语义错误WriteTooLong，零写本次输入并poison stage、原读取Incomplete(Structure)；后续不能恢复继续发布。publish时written小于要求为ShortWrite，同样失败。写满后仍须publish校验，写满本身不是可见条目。

所有实际拥有的容器capacity、key buffer、对象头与临时分配峰值须在分配前收费。实现可用fallible exact allocation；不得把Vec备用容量或into_boxed_slice的双份临时峰值藏起来。底层allocator对齐/实现开销与容器可观察capacity分开说明：本预算是公布的保守对象/capacity合同，不声称等于操作系统RSS。具体size_of、capacity上界、转换是否移动/复制以及分配失败路径留编码验收；不能事后发现大capacity才补费，不能为通过测试降低实际峰值观测精度。Vec::try_reserve_exact若用于此路径，须结合固定工具链std源码及实际capacity核对其分配请求/转换上界；不能只在分配后超额拒绝就称已经先收费。无法证明时可窄用std::alloc的精确Layout、经审查RAII释放及有语义分配失败，不新增依赖，不隐含OOM panic/handle_alloc_error路径；必要unsafe范围须独立检查。无法证明某路径上界则先细化合同，不凭猜测完成。

## 6. publish、位置和错误

publish只消费StagedItem一次。重验h与stage的容器scope/目标/resource/generation、stage无poison且written恰等于payload_bytes；Replace还要当前item指针相同，Insert还要key仍不存在。所有检查在修改当前窗口之前完成。

Insert成功时才checked取得下一个position并发布，next_position只推进成功次数；Replace沿用旧position，不推进。SequenceExhausted零新发布并标原读取Incomplete(SequenceExhausted)。发布只移动已收费payload/key/Charge，不重新clone或serialize；底层列表节点/Arc控制块的保守上界已在ITEM_BYTES中，必须先收费。可返回的预算/容量/构建错误不先替换原值。

固定稳定工具链中Arc::try_new仍属不稳定allocator_api，LinkedList没有try_push_back。本包继承既有Arc/LinkedList全局allocator的进程级OOM abort边界，不声称所有heap故障均可返回Capacity。逻辑预算、checked溢出及scratch/key/payload的fallible分配失败必须返回Capacity并标Incomplete；对此实际注入验证，不用模拟hook冒称已证明全进程OOM可恢复。全局allocator耗尽、进程或操作系统故障不能记成正常展示失败收敛，也不作为主动取消原生执行的API。本澄清不改变缓存预算或现行spec，不新增依赖、夜版工具链或自有引用计数器；若将来声称全部heap故障可恢复，须另行设计与审查。

| 错误/动作 | 可见结果及读取状态 |
| --- | --- |
| foreign/旧h、错误stage scope | Stale；不改当前新读取，不发布。已授予stage Drop仅标其原cell。 |
| lease被新item替换、insert重复key | Conflict；不覆盖。当前相关读取Incomplete(Structure)；无偷偷insert/replace互转。 |
| 找不到lease key | MissingItem；零分配；不代表Domain判断来源丢失，caller决定其语义。 |
| invalid key/size算术溢出/容量失败 | InvalidKey或Capacity；当前构建失败按Structure/Capacity标Incomplete；不回绕。 |
| writer超长/poison/短写 | WriteTooLong/PoisonedStage/ShortWrite；零发布，原读取Incomplete(Structure)。 |
| stage丢弃/abort | 所有费用按实际租约Drop，原读取Incomplete(指定原因/Structure)；不影响新代际。 |
| 成功 | 可见一个完整不可变item，费用移动一次，原owner之后仍可继续FIFO insert/replace。 |

新增错误是Domain字节构建错误，不能复用“签名过期”等管理错误。显式abort_stage的handle若已过期仍应先让stage安全Drop并只标旧cell，再返回Stale；不能泄漏费用。异常锁状态沿原ledger broken关闭新增分配，不能用unwrap/panic或回绕恢复room。

## 7. 实施与独立验收边界

预期改动：仅domain/views.rs、其开发测试与必要API说明；root串行处理共享文档。没有JSON/scanner/ACP、新广告、SQL、WSS、权限或真实writer。现有insert/replace/snapshot行为及19项独立测试必须回归，新增API不能让旧scope/费用/状态约束失效。

独立测试须覆盖：

1. 各构建步骤之前已收费；scratch/lease/候选/旧快照同时占用；Drop、abort、发布精确退账与旧代际存活身份。
2. 同key换值的旧lease CAS失败；消费lease不能Clone/再用；snapshot字节或裸position不能构造写身份；多租约分别收费。
3. 构建时释放/换代，staged publish不能写新读取；Drop失败只标旧cell；ReadStatusCell费用活到最后stage消失。
4. insert预留发生FIFO淘汰后注入分配失败，Err后window_status零分配仍报告真实累计水位；后续insert能继续滚动近端窗口。
5. Replace容量失败/短写/overrun/poison/重复publish/position耗尽不破坏旧值，不隐式淘汰；未publish Drop标Incomplete。
6. 目标/节点bytes及items上限、64身份、checked极值；小预算真实路径与生产默认算术边界分别报告，不用非法fixture宣称最大规模。
7. 实际编译、size_of/capacity及分配峰值/故障证据；API负编译封装；fmt/clippy/既有view回归。没有实际执行不能写已证明。

编译布局、分配证据和新API实现均未执行；此文不是通过报告。已对照独立tester的view-budget-port-definition.md（b94c590fd5daab2d1aed7d31b6872f8e92071a18e346769c5eb7e49f6bc2b9a3）及gaps的BP-G01–06逐项固定语义；未改其资产/预期，也不代其确认无缺口。root取得独立定义及reviewer放行后才可实施。ACP结构投影revision2文档尚未完成/未创建，按root要求暂停，不能用Domain通过代签整个任务3.2；旧整体合同的淘汰后DrainOnly仍受VPROJ-PLAN-001阻断，不作为本包规则。
