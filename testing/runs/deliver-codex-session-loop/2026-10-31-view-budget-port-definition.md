# 视图预算端口增量独立子用例候选

2026-10-31更新。当前依据根view-budget-port-contract.md SHA256 `d6ee1d36b701912feca8c7fc1d1c345bc24beb0e99bb618c2ecaaf347a631098`，由外部Domain revision2最终合同 `45528b1c573c5c2e5a78255fe314db77cfd1b478b507661370481eb99fcdcbae`复制并澄清稳定std全局OOM边界；外部source-manifest SHA256 `94c84e76542e8e7144086f4f3c16a3870f4c0bcda3750b7f46465f9f698403b1`保留原输入归属。另依据根view-window-contract SHA2d8b7362...、structured-history规格及原19项独立验收。当前views.rs SHA `ee531f845af4050809721d88e41fbce0f0bff16235cfdb763715a5dc845ca612`，原view_window.rs SHA `28f2df93e92ebc159a120b21336f6983ad1043856dd11af387d5b7ecaee967d8`。初始定义b94c590f.../缺口b98e2522...逐字保留于*-initial.md；不使用中间d17稿。**新增断言全部not-run，本定义不代替reviewer编码门槛。**

本阶段仅单条ItemLease、计费StageWorkspace、精确候选writer、消费publish/abort与WindowStatus；不实现ACP结构投影、JSON envelope/merge/scanner、ProjectionRead/ProjectionState及其1024额外header、DrainOnly或真实owner。原整体候选§3对两遍构建的要求仅作为不能绕预算的端口前提；不能以本阶段通过宣称raw-span保真、v1/v2 patch、扫描工作134217728上限或整体结构适配已完成。本文采用最终合同的API语义，正式代码名称统一后仅机械适配测试，不创造授权/写token入口。

## 最终公共API判据

item_lease(h,key)返回只借读position/key/bytes的非Clone ItemLease；stage_workspace(h,scratch_bytes)返回非Clone固定长度scratch。stage_insert按值消费workspace，stage_replace同时按值消费workspace与原ItemLease，返回StagedItem。StagedItem.parts仅拆借scratch、可选旧ViewItem和限长BoundedWriter；不产生新拥有型复制。writer支持write_all/written/remaining、每次全写或零写，无seek/grow/取Vec。publish(h,staged)与abort_stage(h,staged,reason)均按值消费候选；publish前重验scope/代际/完整长度/旧item CAS或重复键。

WindowStatus/PublishFact为固定大小Copy值，不含String/Vec/Arc或target克隆；status经与snapshot相同的scope验证，含state/generation、保留首尾位置、next_position、累计淘汰水位/计数与retained_items，读取零堆分配/零新收费/零引用。PublishFact含成功位置/created或replaced及累计淘汰事实。Insert直到成功publish才checked分配position并推进计数，replace不推进；reserve/短写/失败不消耗一个成功发布位置。

合同允许支持多个同时stage，或显式一个开放stage并对额外请求Busy；最终API冻结时记录实际选择，独立测试按该被允许分支验证。Busy不能误标读取失败、不能挂未计费等待队列；即使单stage，仍以现有replace/insert交错验证publish时二次CAS。名称可统一，所有权/错误/计费义务不能删除。

不改统一tests稳定ID/标题。CL-023是旧项原位替换；CL-026/049/050是预算/租约/失败；CL-006/012/051/064是共享目标、scope/资源/读取代际；CL-024/042仅不可变opaque字节及即时快照的内层部分。完整CL/L5/L6、渲染/权限/慢网络/真实Harness仍未验。无用户新增产品决定；工程缺口见view-budget-port-gaps.md。

## 计费独立判据

保持原生产目标16MiB/4096、节点128MiB/32768及64预算身份。测试使用显式缩小限额、合成opaque字节和确定性操作，无数据库、网络、ACP进程、浏览器或Web audit。账本期望取合同数字，不从实现常量计算期望：

- 根R=8192仅节点；当前目标基础B=1024+nativeID UTF8长度+1024窗口。
- 原/新条目O/C=512+可靠key UTF8长度+blob实际被拥有容量；若精确boxed slice且无额外capacity则长度即容量。闭合namespace无额外动态键。
- 单条ItemLease L=320字节、1引用，此外旧对象O继续按1对象计费；MissingItem零新租约。
- StageWorkspace S(n)=512固定头上界+n显式scratch容量，items=0；标准n=7680对应原S=8192，n=0仍收费512且返回零长scratch，其他n按同公式。Domain不规定scanner100层栈；固定头实际若超512或额外动态capacity/临时峰值不能覆盖，先补合同/独立期望再实施，不能减scratch或事后补费。候选C另计1对象，不能把scratch当正文池。
- 快照保持256+64×引用数，引用数同时计items。候选、lease、stage、旧快照/旧代际及其他目标一律同账本。

替换最小峰值为`B+O+L+S(n)+C`，再加其他保留项/快照/已有候选。stage消费的是已收费lease，不另加一份L/引用；另借第二lease仍另加L。items至少旧对象1+lease引用1+候选对象1；旧snapshot的引用另加，不能只数当前window。

state与窗口1024 Charge移入私有ReadStatusCell，由当前window与workspace/stage共同持有。release/换代后，只要旧workspace/stage未Drop，旧cell的1024仍计原目标/节点，新增当前window再计自己1024；不是共享一个免费cell。最后引用Drop才退旧1024。普通snapshot冻结当时状态，不持活状态语义；旧快照不被stage后来的失败修改。cell控制引用不计item引用名额，但其Arc/锁/字段布局必须落在1024窗口/512工作区头等已收费上界。纯ItemLease不等于workspace/cell，不要求它独立保活窗口费用。

所有对象Drop只退自己真正不再拥有的费用。原blob在有旧lease/snapshot时不得因替换/释放/换代提前退账。正常未转换workspace Drop只退账，不代表已授予一条来源更新；已授予stage未publish Drop则必须将所属原cell标Incomplete(Structure)，不能污染新代际。

## BP-A：单条查找租约（CL-023/026/049）

步骤：建窗口多项，同key借一个/多个lease，另取全窗口snapshot；查不存在key、错namespace/目标/scope/读取/资源代际；持lease期间替换、淘汰、release、新begin和Drop registry。以target/node字节与items的恰好/差1边界重复借用和失败。

预期：只返回匹配单项，借用不冻结/复制整个窗口；每lease精确增加320/1，任意其他窗口条目数量不改变这项成本。不存在/失效零lease和计费变化；只有不可变position/key/bytes借用，旧字节可以在失去当前资格后继续读，但不因此重新发布。多个lease共享旧blob一次对象费，逐个Drop退引用费，最后旧引用退blob。公开无Clone、into_bytes、可自由克隆Arc或脱离lease的借用。

## BP-B：scratch先收费、布局与Drop（CL-026/049/050）

步骤：分别目标/节点恰好/差1收费创建n=0、1、7680的workspace，实际scratch.len等于请求且不可grow；usize::MAX在checked时拒绝不分配。多个workspace/旧workspace跨资源换代并存，和snapshot/lease争共享空间；申请后不构建直接Drop、错误reserve后Drop、调用者提前退出。检查size_of和所有实际capacity，不只读usage。

预期：申请前预留512+n完整费用，恰好可成、少1拒绝且零对象残留，当前handle的workspace预留/分配失败标Incomplete(Capacity)；foreign/stale不误改新读取。正常Drop尚未转换的workspace只退账，不将读取设失败；若预检失败由caller显式fail。workspace消费入stage后全程保留512+n，到publish/abort/Drop才退，writer完成不提前退scratch。失效后workspace/stage还同时保留旧cell1024并占原目标/节点/64身份，不能换代免费建新池。公开API不依赖ACP/serde_json。size_of及所有动态capacity证据覆盖512头/cell/控制引用，新增Vec/String/索引/临时buffer没有免费容量；scanner栈不由本包证明。

## BP-C：精确writer和候选失败（CL-024/026/049/050）

步骤：长度0、1、分段写至精确N、恰好N后再写1、单次写N+1、少写1就publish；0字节write和中途Drop；key4096/4097 UTF8字节；reserve usize::MAX或会checked溢出的长度，在分配前拒绝；伪装HTML/NUL/非UTF8/JSON外观字节。与已存在snapshot/lease组合到候选字节/条数限额。

预期：必须先为候选固定元数据/key/实际输出容量收费，再分配/拥有；writer不会grow/realloc成未收费容量，不能截断输入后当成功。write_all超过remaining返回WriteTooLong，本次零写、written不推进、stage永久poison且原cell Incomplete(Structure)；随后publish返回PoisonedStage而零发布。写少1publish返回ShortWrite、零发布且原cell Incomplete(Structure)。写满仍不可见，只有成功publish才出现完整item；中途未发布Drop/显式abort准确释放候选/scratch/被消费lease并标原cell首个失败原因。opaque字节逐字保留，本阶段不解析/规范化JSON。公开usize::MAX长度可以真实触发checked拒绝而不尝试巨型分配；内部u64极值若公共不可达，独立静态检查并记动态未覆盖，不沿用开发私有测试冒签。

## BP-D：无复制move发布与替换峰值（CL-023/026/049）

步骤：分别无snapshot、有旧lease/多个snapshot时，目标/节点只够最终值却不够O+L+S+C峰值、峰值差1、恰好峰值。reserve后writer完整再publish；保留旧lease和新snapshot，对照Drop候选不publish。比较发布前后对象/引用/候选/scratch账，按源码/layout核对无第二份blob复制。

预期：峰值不足保留旧值/位置/其他条目且替换不淘汰；恰好成功新候选Charge+bytes直接move入原位置，不能调用现有replace(&[u8])再偷偷复制。stage_replace按值消费并持有原lease，其320/1费用不翻倍、不提前退；成功发布后workspace与此lease Drop，旧blob仍有其他snapshot/lease则继续收费。其他旧snapshot/lease仍见旧版，当前snapshot见新版。未publish候选Drop释放C/S/其拥有L但不改旧内容，原cell标Incomplete；成功发布解除stage Drop失败标志，不能随后又误标失败。publish消费候选，不可重复使用/克隆。大对象“看起来usage没变”不够，必须独立核对实际分配容器、实际capacity、列表节点/Arc控制块与转换峰值；若无法从公开行为观测copy次数，源码与布局核对单列，不伪称运行证明零copy。

## BP-E：旧item CAS与ABA（CL-012/023/051/064）

步骤：同key借两个旧lease，stage_replace各按值消费，先发布一个再发布另一个；若最终选择单开放stage，则第二grant明确Busy，另用现有replace在首stage构建期间更新验证二次CAS，并用第二旧lease后续grant验证预检查Conflict。另在staging期间FIFO淘汰后同key重新insert（包括相同bytes）、或换读取/资源/容器；再尝试旧候选。调用者使用已消费lease必须编译失败，不要求它还能提前Drop。

预期：grant预检与publish两次核对当前key/私有Arc身份；同key/位置文字/内容不代表同item，后来的完整对象不能被旧候选覆盖。原Arc由已收费且被消费的lease一直强持有至比较结束，防止释放后裸地址ABA，不允许改成Drop旧Arc再裸指针比较。Stale不改当前新读取；同读取Conflict不覆盖内容/顺序，但标该读取Incomplete(Structure)。候选被消费/Drop准确释放，无长期全历史索引、没有第二个免费Arc/引用。不能从snapshot/ViewItem/裸position创建写身份，也不能为replace更改原key/目标。

## BP-F：跨scope/目标/代际的预算与发布（CL-006/026/049/051/064）

步骤：两个容器局部代际相同；A workspace/lease/candidate配B handle，原目标对象配另一目标，逐一换read/load/connection/runtime。失效后仍保留stage/candidate/lease/snapshot再建新窗口，逐项Drop，重复至目标/节点上限；64身份均只剩旧stage或lease/candidate时尝试第65个，部分和最后Drop后再试。

预期：所有能力绑定原容器和原NativeKey/资源/读取代际，不能跨ledger重新挂账或发布；错误不影响B的对象和账。旧对象退役前仍合计原目标与节点限额/身份名额，不能有第二16MiB池。window已release但旧workspace/stage尚活时，旧ReadStatusCell1024仍收费；其Drop只改旧cell，不污染当前新读取。旧snapshot冻结的state不因旧stage后来的失败变化。最后Drop且无window才能回收身份；abort旧h也安全Drop原stage后Stale，不泄漏或修改新代际。collect_released/begin/release原语义保留。

## BP-G：Insert FIFO、失败事实与快照（CL-026/042/049/050）

步骤：以stage Insert连续远超预算的独立实体/无键opaque项，另目标作为对照；持lease使FIFO退引用但不能释放旧对象；reserve已淘汰部分前缀后仍Capacity/构建短写/Drop；reserve成功后同key先被其他insert占用再publish。snapshot分别在预留中/未发布/发布后及低预算失败时取。

预期：只淘汰自身最早前缀，保留最新成功窗口，generic Insert仍可在淘汰后继续长流；本阶段不采用adapter DrainOnly作为停止内层插入的理由。单候选不可能装入目标预算时在淘汰前拒绝；duplicate key Conflict零淘汰。快照看不到半成品；第一次成功publish后立即可见，不等待新通知。reserve前缀淘汰后失败必须保留真实水位/计数并显式未完整，不能回报窗口“未变”；即使snapshot也Capacity，WindowStatus仍零新分配/收费查询成功并给累计水位和retained范围。成功PublishFact的累计事实与status对应。reserve不分配position，短写/poison/Drop/冲突都不推进next_position，只有成功insert推进；replace沿旧位置。重复key冲突不变成replace，同读取构建Conflict标Structure，replace从不借此FIFO。Finished有淘汰/缺失仍不完整，先前Incomplete不能finish洗白。其他目标/执行/管理不受影响。

## BP-H：公开所有权与负编译（CL-003/026/049）

在实际普通dependency里先编译新端口正控制，再逐项尝试Clone ItemLease/StageWorkspace/StagedItem、使用publish后已move候选、让bytes/key引用逃离lease、访问内部Charge/Arc/自由所有权。预期为精确E0277/E0382/lifetime/private API诊断，不以语法/缺依赖错误冒领。names/lifetimes待最终API冻结再给准确source。无Clone/move-only不单独证明账本，仍与A–G实际Drop/usage/容量一起验。

## BP-I：原19回归与边界检视

原view_window.rs 19项保持原意、无删/ignore/弱化，在同版重跑；旧scope/状态/键/精确费用/生产4096与32768计数/64身份等原保证不得破坏。独立核对domain仅依赖稳定内层和std，未引入ACP/JSON DOM/SQL/文件/network/writer执行、授权或取消路径；opaque内容不产生管理动作。对候选writer准确容量、workspace大小、数据布局和锁/Drop顺序做源码/编译布局证据核对，区分动态覆盖与静态检视。

## 分配故障证据的精确边界

d6合同明确：逻辑预算、checked溢出以及scratch/key/payload的fallible分配失败必须Capacity+Incomplete，旧原值不得先破坏；这些选定可返回失败路径需要实际可控注入与Drop/usage/水位断言。独立普通crate可用scoped GlobalAlloc，在唯一已识别bulk Layout的一次分配返回null，范围外转交系统allocator；用无分配计数记录命中，避免真实耗尽内存和误击Arc/list元数据。具体Layout按冻结实现核对，不能凭猜测尺寸失败就计通过。它验证选定实际fallible路径，不是生产hook，也不是全进程OOM恢复。分配前模拟hook必须另标，不能相互冒称。本定义无需新增生产故障接口，若实现后发现具体API不可测再补充。

固定std的Arc/LinkedList节点等进程级全局allocator OOM abort继承原边界，必须先按ITEM_BYTES等保守上界收费，但本轮不要求或冒称所有heap故障可转Capacity。全局耗尽/OS或进程故障不记普通展示失败收敛，不由本API主动取消原生工作。测试不实际耗尽机器内存；std相关限制由root/reviewer提供说明，本tester本轮未运行OOM或重测该std负编译。若新增unsafe精确Layout分配，须独立核对fallible返回/初始化/边界/RAII释放和所有实际capacity，不能用abort边界豁免已承诺的可返回路径。

## 阶段交付门槛

d6根合同（保留45528b1c行为）已明确主要API/所有权/失败/CAS/范围行为，原BP-G01–06可关闭“行为定义”层，本文A–I可冻结供纯Domain实施门槛；无需等待实现函数命名完成，也不预造生产开口。布局/实际容量/分配峰值和故障路径仍为实现后必需证据，未有编译/运行就不签通过。实际函数名与单/多开放stage的允许工程选择在交接源码时机械适配；发现具体不可测API再登记补充。

随后仅在获授权独占路径新增独立公共API资产；记录版本/锁/实际用例数/期望费用/实测usage+capacity/Drop次序、编译诊断/失败/未执行及原19回归。不新增稳定CL ID，不将预算端口通过当ACP投影/DrainOnly策略或L5/L6通过。本轮无新用户产品决策，未发现最终合同新的产品边界冲突。
