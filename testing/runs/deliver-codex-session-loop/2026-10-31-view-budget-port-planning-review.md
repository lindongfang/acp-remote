# Domain视图预算端口编码前复核

2026-10-31，reviewer `/root/baseline_reviewer`。只读设计和固定工具链来源；未改产品。外部d17/1e8e中间稿不作为编码依据，外部45528b1c保持冻结；当前根合同为`view-budget-port-contract.md` SHA256 `d6ee1d36b701912feca8c7fc1d1c345bc24beb0e99bb618c2ecaaf347a631098`，已实际完整读取。当前产品基线仍纯容器ee531f84。

当前结论：**纯Domain编码门槛通过。** 已实际复读并核实根d6合同、独立tester最终A–I定义和BP-G01–06闭环，未见剩余具体设计阻断。允许按本报告末尾白名单实施；这不批准原ACP投影或DrainOnly，不签任务3.2整体。以下记录已独立核对的工程语义。

## 端口与不变量

- 单条ItemLease按320/1收费，私有Arc只沿用一份；stage_replace按值消费lease，直到比较结束旧Arc持续存活。用仍存活Arc身份比较而非释放后的裸地址，阻止同key/同bytes重新insert的ABA。多lease分别收费，旧数据与新候选峰值不被提前退账。
- Workspace为S(n)=512+n，n是显式请求的opaque scratch容量；标准n=7680对应8192，n=0仍收512。继续由StagedItem拥有至publish/abort/Drop，不虚构100层JSON栈已实现。本包无ProjectionState/JSON/scanner；新布局超过上界必须在实施中停下补合同，不能缩scratch或先分配后收费。
- WindowStatus固定值零分配，在snapshot也Capacity时仍可读取真实水位。Insert预留先淘汰后再失败的副作用不会消失；Replace不淘汰/不改旧值；后续generic Insert可以继续FIFO长流。并发候选按预算收费，publish时重新检查key/同item身份，旧19与长流保证不降级。
- Writer全写或零写，长度0/短写/超写/poison明确；只有完整候选消费publish后可见，position仅成功Insert时推进，Replace原位。字节/key/Charge直接move，不调用旧replace再复制一次；错误Drop只释放自身资源并标正确读取未完整。
- ReadStatusCell是局部读取状态及WINDOW_BYTES费用的共享持有者，不是全局运行时/授权owner。它使无`&mut ViewWindows`的stage Drop能记录Incomplete；窗口/workspace/stage最后引用才退窗口1024，布局必须覆盖Arc/锁/字段。旧snapshot仍冻结当时状态，不保留可重新写资格；旧stage晚Drop只标旧cell，不污染新generation。旧API未使用新workspace时，原19费用与scope行为不应改变。

## 固定工具链的真实分配边界

reviewer独立执行`rustc --version --verbose`及metadata-only探针：固定rustc1.98.1、commit48a229cea、x86_64-pc-windows-msvc。探针仅在本reviewer目录，不运行分配；`Arc::try_new`编译E0658（allocator_api不稳定），`LinkedList::try_push_back`编译E0599（不存在），rustc exit1为预期能力证据，不是产品构建失败。源码为`fallible-std-probe.rs`。

亦实际读固定本地std源码：alloc/src/sync.rs的Arc::new经Box::new、try_new有unstable属性；linked_list.rs push_back经push_back_mut/Box::new_in，不能返回可处理分配错误；raw_vec/mod.rs的grow_exact以checked(len+additional)请求并set_ptr_and_cap(cap)。因此不能假定给Vec数据加fallible封装就使Arc/链表节点全部可恢复，也不能引夜版/新依赖冒领证明。

根d6合同已纠正此前过强工程承诺：逻辑预算/checked溢出及scratch/key/payload可fallible分配失败必须Capacity+Incomplete，真实测试应覆盖；继承既有std元数据Arc/LinkedList全局allocator的进程级OOM边界，不能宣称所有heap错误都语义收敛。该澄清符合既有spec对有界缓存/可处理展示错误的要求，不创造主动cancel路径，也不承诺整个进程对OS/全局OOM免疫，故无需用户新增产品裁决。未收费capacity/普通预算错误不能借此豁免。

固定std实现支持对Vec<u8> reserve-exact路径进一步核实请求/逻辑capacity；最终代码仍须展示具体key/scratch/payload构造及转换，实际capacity/size_of证据与Drop峰值不能只凭公式。若选择另种布局或无法证明上界，应先细化，不为赶进度引入未审unsafe。

测试可以在独立小crate用受控global allocator，仅在确定唯一bulk Layout且arm的一次调用返回null，证明真实fallible路径，而不造成系统内存压力、不加生产任意故障hook。必须确认实际命中该Layout并报告未覆盖元数据全局OOM；不能用模拟hook声称全heap可恢复。

## 最终定义核对与限定放行

已实际完整读取并重新计算SHA256：根合同`d6ee1d36b701912feca8c7fc1d1c345bc24beb0e99bb618c2ecaaf347a631098`；独立`view-budget-port-definition.md`为`e0ae135ee5c1373707510c624958a411d1987f896bee0552c7a4989f4ccc8aba`；独立`view-budget-port-gaps.md`为`2bb1d0be60a3f1c0b7275cb0d9908270e6608df20be2d13fefeb211ac2bbcfd3`。A–I与根合同对齐：消费lease/双次CAS及ABA、S(n)与O+L+S+C峰值、1024旧cell最后引用寿命、失败后零分配WindowStatus、精确writer/position、普通依赖负编译、选定真实fallible bulk分配失败及原19回归均有明确判据。单stage Busy或多stage属于合同允许的工程选择，冻结源码时记录并按相应分支验收。

BP-G01–06可关闭行为定义缺口；这不是实现通过。布局/实际capacity/分配峰值、锁与Drop次序、精确故障命中和公开API执行都仍not-run，须在实现冻结后独立验收。512/1024上界若实际不能覆盖，先停下补合同/期望；不能把本次门槛当成预先证明布局。无需为这些正常实现交接证据继续阻挡已经完整固定的内层方案。

只允许domain/views.rs、新views/staging.rs、开发测试与必要API说明；原独立19断言不改，只有确需的开发私有状态访问机械适配。无SQL/ACP/scanner/JSON/审批/授权/真实writer/WSS；原完整适配VPROJ-PLAN-001保持open。源码冻结和独立验收之后另作实际交付检查。
