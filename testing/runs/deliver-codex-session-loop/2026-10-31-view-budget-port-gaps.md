# 视图预算端口缺口闭环与待验证证据

2026-10-31更新。当前依据根合同d6ee1d36...，外部45528b1c/manifest94c84e76...原稿未改；初读1d4046...对应旧清单逐字保留view-budget-port-gaps-initial.md（b98e2522...）。本轮只有定义核对，没有新增实现/编译/运行验收。

| 原缺口 | 最终合同如何固定 | 状态 |
| --- | --- | --- |
| BP-G01 所有权与scope | 所有拥有对象非Clone；workspace/stage绑同read；stage_replace按值消费已付费ItemLease；stage持scratch到publish/abort/Drop。ReadStatusCell及窗口1024费用活到最后workspace/stage，旧Drop只改原cell，不污染新读 | Closed（行为定义）；布局/真实Drop尚未验 |
| BP-G02 CAS/ABA | lease私有强Arc从授予至stage比较始终存活，stage grant与publish两次校验，不Drop后裸地址比较；没有第二免费Arc/引用；不得从snapshot/position制造lease，不可改key | Closed（行为定义）；真实交错和源码持有链待验 |
| BP-G03 失败淘汰事实 | 固定大小Copy WindowStatus零堆分配/收费/引用，scope同snapshot，可查state/首尾/next/累计eviction/retained_items；淘汰即时更新，即使snapshot Capacity仍可查询 | Closed（行为定义）；零分配/分配失败后水位待验 |
| BP-G04 writer/位置 | write_all每次全写或零写，overrun WriteTooLong+永久poison/Structure，poison发布PoisonedStage，短写ShortWrite；满写仍需消费publish；成功Insert才推进position；grant后未发布Drop/abort标原cell失败 | Closed（行为定义）；实际错误/Drop/位置待验 |
| BP-G05 费用/布局/capacity | S(n)=512+n，标准7680→8192，0长收费512；候选512+key+payload capacity，lease320/1；先收费再fallible分配，明确不以Vec多余capacity/转换峰值隐藏。d6分开固定std全局OOM abort与选定可返回分配错误 | Closed（验收判据）；512/1024实际上界、allocation/capacity/峰值证据尚未提供，不能写已证明 |
| BP-G06 纯domain范围 | 无ProjectionState/Read/1024header，无JSON/envelope/scanner/134217728计数，无ACP策略/DrainOnly。保留通用FIFO长流；外部VPROJ-PLAN-001仍独立阻断整体投影 | Closed（范围定义）；不授权其他包 |

没有新用户产品决策，也未发现d6根合同新增产品边界冲突。纯domain用例定义已可供reviewer门槛核对；不能把“行为定义Closed”写成产品/实现Passed。

实施/正式验收仍需固定并提供：

1. 实际API名称/错误类型及单开放stage Busy或多stage的允许分支选择；两者都须计费且不挂未计费队列。此为合同允许的工程选择，不需用户重新决定产品规则。
2. scratch/key/payload的fallible分配边界、适用阶段/错误证据。可在独立普通crate使用scoped GlobalAlloc仅对唯一bulk Layout返回null并记录实际命中，避免生产hook和真实耗尽内存；不误击Arc/list元数据，不用pre-hook模拟宣称真实全局OOM恢复。无需等待此实现细节才能冻结定义，源码后核对具体Layout；若实际API不可测再补充。
3. 512工作区头、1024 ReadStatusCell/窗口、候选item控制块/列表节点及所有动态capacity/临时峰值的实际编译/源码证据；若上界超出或无法证明，先补合同/预期，不事后扩费冒称已满足。
4. 原19项同版回归和新BP-A–I独立公开API/负编译，逐项区分动态/静态/未执行。必要unsafe RAII/精确Layout另交独立审查；全局OOM/OS故障不做耗尽内存实验，不当正常cache fail。

以上为实现后执行证据/最终代码交接条件，不是当前定义继续open的阻断。本文A–I可冻结供纯Domain门槛，未观察实际实现缺陷。本轮不再沿用初稿缺失API的open结论阻挡已经固定的纯预算行为，也不越过reviewer独立放行。
