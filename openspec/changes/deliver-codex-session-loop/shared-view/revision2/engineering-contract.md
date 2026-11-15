# 共享视图工程合同 revision2

2026-11-01。设计only；未编码、编译、运行SQL或产品测试。v1冻结 `57267f52c75e71bcd2498cc65ca93eb3cb0bd9fd69a038b4763521871dc8576c` 的92成员原字节全部保留，本版不改历史。独立tester定义包 `ffb482a96d50fec9c2b9678bb1a3c2f4ca44600878464c21a7409d0ce8e1d767`，83个稳定分支全部defined/not-run；本次逐项映射见 case-mapping.md，映射不是独立预审或执行通过。

## R2-1. 版本与负责范围

实际根306项重新逐hash相同：before `fa004a9042c0ade9ba835a3f061f0568707c40d354f2f83b4d472f4464aeed88`，after `f01458de3d6aef7d96caafeee73e77094d518f6fddcac0609f4c58365052e8b0`。当前原生工程依赖是ND v6 `9d1349f6545459a10cf8551e5f3d9681d4f80e5e1990f8a6237457fd0fee5869`，叠加收据补充 `ec30e1bf0f23ba86469db0a3ec6b4bb16e4cd6796cbcbe309d09438293658410`。八份有效ND合同/DDL中control-ports和closure取补充覆盖字节，其余六份取v6原字节；control-receipt-supplement补充只读收据说明。有效overlay复制到input-snapshot/native-effective，**不再以ND v4或live v5作为当前装配依据**。

ND v6及补充仍是未实现工程接口；其编码gate由原独立角色另管，不将作者冻结或80定义当已编译/产品能力。新view追加端口也必须同版独立gate，不能因ND通过就自动通过。442份捕获输入逐字节固定于inputs-v2.json `60839ee6d4afea8f9d766b45cbc43672fb1c74eac3f39dd5746095589267bdee`，捕获期间零漂移，包括v1完整包、tester8成员、ND有效输入和当前306源。

当前负责行为仍为主change八份specs/Q-001/Q-002及现设计，CL-001–123和tester SV-001–083原ID/预期不由作者修改。产品/安全/数据权威未增加新取舍，不请求用户重授权限。

## R2-2. 完整合同的取代关系

本版 read-and-source-api.md **整体替代v1 interfaces.md的公开/装配签名**；send-and-credit-api.md整体替代其Prepared/Delivered/cursor以及v1 assembly§4的发送/信用接口；native-assembly.md替代v1所有ND4/5依赖和“create发送前按SID绑定”的错误表述。v1关于无总预算、原文/结构保真、来源身份区别、source FIFO、首开cut→H snapshot/tail、安全DOM/资源、不持久化、慢展示隔离和关闭责任的其余规则继续有效，已按原字节复制到 input-snapshot/prior-v1；存在上述接口重叠时仅本版有权威，不能两种API混接。

72字节opaque帧、Data payload最多16384、控制text32KiB/depth8、全连接outbound1MiB/256、真实管理命令120/60秒、既有writer15秒/close2秒均按v1明确格式保留；credit不是新command。record/body/node总量、条目、身份、引用、snapshot、版本无总quota/TTL，网络chunk/公平调度批次不能变成内容总量预算。

## R2-3. 语义与故障状态表

| 真实事实 | 读取/投影/连接状态 | 操作及控制影响 |
| --- | --- | --- |
| CurrentOnly原文均保留 | HistoryIncomplete=true，Projection按实际，CurrentObservation | 不冒完整历史；输入/恢复/审批能力独立 |
| 未知但合法结构 | PartialKnownStructure，raw完整 | 不当协议错/丢失，不自动拒人工真实审批 |
| raw保留成功，投影try_reserve失败 | ProjectionUnavailable；原raw仍可读，完成轴不改 | 不进ND gap/ack，不取消work |
| capture失败/不连续真实seq | source预留fault位+首缺seq，受影响读边界Incomplete | 不生成控制ObservationFault；其它source独立 |
| 完成proof以前存在raw gap | ReplayIncomplete(CaptureGap)，保原协议响应事实 | 不能宣称完整展示；不要求原生重发输入 |
| 原Replay登记匹配真实Replayed，先前相关raw处理完无gap | ReplayComplete(ReadCompletionProof) | 不等待Written/flush，不解除执行/审批门 |
| 此后flush Err/Unknown或source失联 | 原ReadCompletion保留；Transport=Unknown、Freshness=Stale | 不捏完成边界前缺失；原native责任仍由ND |
| 完成之后新的capture失败 | 仅后续观测范围Incomplete | 不抹已完成历史段 |
| 原wire非法，ACP拒绝且reader terminal | 无伪Received；真实terminal分轴处理 | 原ND按实际source loss，view不代ND事实 |
| chunk分配/网络/credit故障 | 仅该读/stream失败，cursor不提前提交 | raw不淘汰；不报原生保存失败 |
| source/view/stream先seal、queued块尚未start_send | 当前stream封闭/旧cursor stale，零该次start_send | 不等待actor gate/网络flush，不阻ND ack |
| 实际start_send先赢、后seal | 仅已开始固定块可完成；SendSucceededAfterSeal或SendFailed事实分列 | 不给下一块，不声称已发字节撤回 |

ProjectionUnavailable是状态，ProjectionFailure是错误类别，二者不与ProtocolFailure混用。ViewError封闭类别见read API；Display/Debug只有类别/平台安全长度，禁止未知正文/长nativeID/配置/远端数值canary。

## R2-4. 分配、counter与关闭精化

source安装时预分配fault槽：AtomicU8 failure_kind、AtomicU64 first_missing_sequence(0=unset)、AtomicU64 retained_frontier、AtomicBool sealed及Notify。producer唯一，首次失败以compare_exchange(0,actual_seq)记录；重复失败不覆盖更早seq。先release存故障再notify_waiters；open/worker先注册Notify再acquire读fault/frontier，无consumer时也能在随后open观察。通知不分配新错误正文；实际断代seal与终止记录不造Received seq。不同合法Received可同receipt_order，去重仅看actualscope+Received.sequence连续性，不按receipt_order判重复。

所有新容器增长try_reserve；共享raw clone为当前RawMessage底层Arc引用，不clone整个Incoming。ID随机失败返回RandomUnavailable且未注册新stream/view；已注册任务仍归原supervisor清理。checked counter耗尽返回SequenceExhausted，不wrap复用或恢复旧身份。无安全可达的私有MAX仅静态/未执行，不添加生产setter。真实全进程OOM/Arc引用溢出abort不声称可恢复或已测。

view supervisor在创建时捕获可信Tokio Handle，source worker/read/cleanup task首await前登记稳定Arc槽。Drop在无ambient runtime线程只seal/取消，通过原Handle登记/唤醒已经存在的cleanup责任，不调用Handle::current、不panic、不称join。shutdown并发调用共享同closure operation和截止结果；等待方取消不take JoinHandle到局部、不放弃任务；真实join后才标joined。raw大版本析构在专用cleanup任务spawn_blocking分批释放；任务及JoinHandle仍登记原槽，不在pump/SendRegistry/actor/ND业务锁下析构；若既有flat RawDocument深结构drop成本不可中断，记录cleanup尚未join，不能冒截止内完成。

source/target解除只seal对应view/send registry并发出失效事件，native关闭独立继续；view join/drain的截止不成为ND安全控制/必要SQL围栏。NativeReadOperation已可能写则仍归原native supervisor，view关闭报告pendingNativeReads实际数量，不能session/cancel来结束显示。正常库无旧history恢复入口；旧正文数据不删除/读回。

## R2-5. 工作范围与门禁

只写本新revision2目录；根/旧v1/ND/tester包不动。拟编码范围仍先SV-module（domain/application/ACP新view模块）后SV-mount/native bridge/gateway/Web；共享mod.rs、ND SourceSlot、management_ws/node等实际重叠文件由root按最终基线单owner串行合入。tester保持独立，reviewer NativeGate优先；作者本文件仅将SV-PLAN001–006标为“已提出精确修订，待原提出者/独立reviewer确认”，不自行关闭问题。

完整L4真实ND/队列/SQL、L5实际认证WSS/Web、L6真实Harness/Windows职责不因先行模块而删除。L3真实ACP source fixture的明确正链仅证明模块层，不代生产SourceSlot/actor/socket/树清理。未有同版独立编码前PASS不编码；未获要求不提交、推送、发布或归档。
