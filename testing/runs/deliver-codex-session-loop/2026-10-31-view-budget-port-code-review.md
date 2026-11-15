# Domain视图预算端口独立交付检视

2026-10-31，reviewer `/root/baseline_reviewer`。本报告只覆盖d6合同/e0ae独立A–I定义批准的纯Domain端口；只读产品，未改代码、依赖或验收断言，未运行真实Harness。当前为静态检视及同版独立执行证据均已完成的最终限定结论；中间补读记录保留在后文，不能据此关闭完整任务3.2。

## 目标快照与结论

实际读取根`testing/runs/deliver-codex-session-loop/2026-10-31-view-budget-port-source.json`，并逐项重新SHA256核对，10项全部一致：

| 文件 | SHA256 |
| --- | --- |
| domain/views.rs | a22bb6e5e9a0e7fdca3b66e9e3dc18025a37ab57832ea95b44e509d189255f26 |
| domain/views/staging.rs | d2a2a6ec3756777a5758d05ffd51e7a78f9f62fb5158283545854c77b00b0f09 |
| staging/tests.rs | 7533fa576285eac325af8e58250e6856c4ea86eedce08e44b83280ed8940fa53 |
| views/tests.rs | 430354104b022d44072c40342beb82e5c0472c055015df29717144acb98a1579 |
| domain/README.md | a7938db6c6b42641452781c46012d4343676a2b7668cb4f14c0e62e4c4d71c86 |
| 原独立view_window.rs | 28f2df93e92ebc159a120b21336f6983ad1043856dd11af387d5b7ecaee967d8 |
| Cargo.toml | c48046e404e8d10b036eec48163295a1386aa2cbe4baeafbe479fe823638045d |
| Cargo.lock | 87f27b619281b37015c79ceae8d95471c8a4de50f202f4dd8a6d99fa0d27e324 |
| rust-toolchain.toml | 3913c4a1af198d0f924cdd3197b8b4bbc6f48cafb423170596137e0784923ff4 |
| 根预算合同 | d6ee1d36b701912feca8c7fc1d1c345bc24beb0e99bb618c2ecaaf347a631098 |

完整读取实际views.rs、staging.rs、开发测试与Domain README；检查Git当前状态，保护已有未提交改动。独立tester最终定义e0ae135e/gaps2bb1d0be与上一轮门槛不变。本版允许多个同时存活候选，按费用限制，不使用等待队列。

**纯Domain预算端口限定交付通过，未发现本轮具体阻断问题。** 已完成实际代码安全检视，并读取原tester正式报告、同版原始测试/负编译日志、真实退出记录、资产与前后源码清单。开发20绿及clippy只作输入信息，不代替独立执行；最终结论详见末节。

## 所有权、费用与锁

`views.rs:243`的ReadStatusCell持状态锁与原窗口Charge，Window在声明最后持有cell。窗口先释放handle和条目，再退cell；旧workspace/stage仍持强引用时窗口1024不会提前退账。cell不持Window，不构成循环。旧Snapshot仅复制ViewState，不会被后续失败修改。状态锁只用于读/首因sticky失败/finish，不在状态锁内释放Charge或获取预算锁；预算reserve和Charge Drop均先node后target，无新逆序。

`staging.rs:65`工作区先收费512+n再分配精确scratch；charge最后释放，scratch先销毁。StagedItem的old/key/data/候选charge在work之前销毁，工作区512头贯穿其余候选/旧lease清理。未publish Drop先标原cell Structure，再逐字段退自己真正释放的费用；成功publish设published解除失败责任。workspace本身正常Drop仅退账。release/换代后旧工作区仍保有旧cell与原TargetBudget；collect_released仅在无window且预算最后引用消失后回收，64身份及跨代际同账本不绕限。

ItemLease包含私有handle、唯一已付费Arc<Item>和320/1 Charge；读取只给借用，不公开拥有型buffer或重建token入口。多个lease分别收费，共享原对象费一次。stage_replace按值消费lease，持续持强Arc到publish/abort/Drop；没有第二份免费Arc。grant和publish都用仍存活Arc的ptr_eq检查当前item，地址不能在释放后被重新分配造成ABA。scope、NativeKey、runtime/connection/load与读取序号均验证，旧stage Drop只影响其旧cell。

## 构建、发布及失败

`stage_insert`先检查key/checked成本/重复键和不可容纳的目标最低成本，再只淘汰当前窗口FIFO前缀以争取共享费用；每次淘汰立即记录水位。后续预算/真实分配失败会保留实际淘汰事实并标Incomplete，不谎称原窗口未变。固定Copy的WindowStatus不分配、不收费、不增引用，低预算下仍可查询。generic Insert不会因淘汰而永久关闭。Replace从不淘汰，旧对象/lease/work/candidate同时收费，峰值不足保持旧值。

writer在已分配slice内checked计算写尾，每次全写或零写；超写立即poison并标原cell Structure，此后不能恢复发布。短写publish失败，写满仍不可见。publish消费候选，重验scope、poison/完整长度、旧Arc或新增键冲突，再checked分配成功Insert位置。Replace沿用位置，失败不推进。key、data及Charge move到新Item，不调用旧replace复制字节；所有正常错误会消费并清理候选。未发现执行、授权、网络、SQLite、ACP/JSON解释或主动cancel路径。

## 实际分配边界与布局证据

实际复读固定rustc1.98.1本地std源码：alloc/raw_vec/mod.rs的try_reserve_exact→grow_exact以checked(len+additional)请求并set_ptr_and_cap(cap)；新空Vec<u8>/String按请求精确预留，填满后len=capacity。alloc/vec/mod.rs的into_boxed_slice先调用shrink_to_fit，而后者只有capacity>len才缩容；因此本版key/scratch/payload满长转换不引入第二份bulk或新分配。`zeroed_exact`和`copy_key`另检查可观察capacity，所有bulk预留在计费之后。固定std证明是分配前容量依据，不能仅用分配后检查代替该证明。

Arc控制块与LinkedList节点依旧走稳定std全局allocator，按d6继承进程级OOM abort，不宣称这些失败可返回Capacity；其保守内存费用仍必须先覆盖。新实现没有unsafe、依赖或夜版工具链，也没有生产模拟故障hook。scratch/key/payload try_reserve_exact失败各有Capacity+原cell Incomplete路径；实际定向null命中仍待独立执行。

已读开发真实日志及其测试源：workspace144、staged360、lease128、parts104、window160、cell32；StagedItem包含work/old的实际字段，而parts为额外拆借控制值，360+104≤512；160+32+Arc双usize≤1024；128≤lease元数据256。旧item/Arc及链表节点仍由原ITEM_BYTES512保守费覆盖。开发测试还对多个长度验证Vec实际capacity和满长转Box指针相同。这些属于开发证据与本次源码核对，最终报告应另列tester的独立布局/capacity和实际故障覆盖，不混称reviewer运行证明。

## 阶段补读记录与范围

等待原tester同版新A–I与原19、公开所有权负编译、选定真实fallible分配路径命中/Drop/费用/淘汰事实，以及锁/布局/capacity原始资产。内部u64极值等公共API不可达分支只能静态核对并如实列出，不能借开发私有测试冒签。

阶段证据补读：已完整亲读tester独占`view-budget-probe/tests/budget.rs`、`tests/allocations.rs`、`tests/support/mod.rs`及同probe的`budget-attempt1.log`/`allocations-attempt1.log`。公开API17/17、独立allocator二进制6/6实际通过；源中期望费用直接来自合同。6次唯一align1 bulk Layout null均断言hits=1：scratch10007；insert/replace的payload12347和key3071；另一次FIFO2后payload12347。均验证Capacity/Incomplete、部分已分配对象销毁及精确费用，替换原值不被破坏，FIFO真实水位仍为2且下一成功Insert位置3。不模拟metadata全局OOM，未耗尽机器内存，未改产品分配器。

独立allocator使用thread-local Cell固定Trace及128个固定事件槽，guard结束前记录无分配，不足槽位会断言拒绝；唯一arm只匹配已知bulk尺寸/align且一次命中，范围外转交System。公开layout独立输出144/360/128/104与WindowStatus96；另观察root实际总分配≤8192、begin元数据分配≤2048，scratch3079/key2099/payload12347各一次精确Layout，成功发布key/data指针与分配指针相同且realloc=0；10000次WindowStatus记录零分配。旧ReadStatusCell私有32/window160的独立编译不可直接size_of，仍以源码字段布局及开发实测区别说明，不冒称来自公开probe。

此补读尚不替代最终source前后清单、BP-H负编译和原19回归。PS将Cargo正常stderr包为NativeCommandError，原日志测试结果为17及6全过；最终实际退出信息由tester总报告另附，不把包装文本当产品失败，也不从正常测试输出补造退出记录。

后续正式执行素材已继续读取：`view-budget-probe/test-final-clean.log`的20个公共API+6个allocator+原19实际45/45，无忽略；新补同容器跨target、仅lease不保活window/full stage Drop、UTF8键4096/4097三个源断言亦已亲读。`clippy-final.log`检查完成。`view-budget-negative-results.json`逐条包含14个真实预期拒绝及16个首尾/逐例后正控制：4×Clone E0277、4×move E0382、2×借用逃逸 E0515、2×私有字段 E0616、无writer extraction E0599、snapshot冒充lease E0308，均落在目标表达式primary span，非语法/依赖缺失造成。普通probe实际产品path依赖，未复刻内层代码。最终结论仍待tester正式报告、exit和前后source/lock清单固定，不提前代其签验收。

补读根`2026-10-31-view-budget-port-full-check.txt`及`full-build.txt`：日志包含Web50、Rust库377、JWS4、平台12和公开API69全过，格式/clippy与实际构建完成；主执行记录两入口实际exit0。此处复用已读的工程日志，不声称reviewer重新执行，也不将原69冒充本轮尚待交付的新A–I独立验收。开发增量缓存hardlink fallback为复制缓存的环境警告，已读最终工程日志没有源码lint失败。

本阶段不批准ProjectionState、JSON envelope/scanner/merge、整体ACP结构投影、DrainOnly或真实owner/SQL/WSS；整体投影VPROJ-PLAN-001不因纯Domain通过而关闭。无完整CL、真实Harness或完整任务3.2完成结论。

## 最终限定结论

已实际完整读取原tester正式`view-budget-port-acceptance.md`并重算其SHA256为`a87b08f8af73f321d52b8c229d6349192161404696a7a9b5d7027fcd2cc5b71a`，A–I逐项覆盖与上述原始证据一致。最终执行为新20行为+6独立allocator+原19=45/45、0失败/0忽略，14个目标负编译及16个正控制；runtime-exit.json和clippy-exit.json均实际exit0。原stderr包装、中间17项及hardlink复制警告日志保留，不当产品失败或隐去原事实。

view-budget-before/after的10项冻结清单同版；本reviewer另对最终9项资产重新SHA256全部匹配，并实际解析product/probe两锁的name/version/source/checksum：261产品包、238探针包、237共享完全一致，仅多探针自身且少24未使用开发图项。无新registry版本解析或产品锁变更。公开布局/实际分配/capacity/pointer与费用释放覆盖本轮512工作区头/1024窗口及旧cell寿命；私有字段布局、不可达极值与全局allocator限制已明确区分，不借运行45项宣称全状态或全heap恢复。

据此，**仅本次纯Domain预算构建端口子阶段可收口通过**。新增独立资产仍在外部持续probe，根完整check中的69项不包括新增26项；若后续入仓，应保持allocator为独立测试二进制并按实际根组合重新验证，不能把这次外部执行冒充仓库入口已接线。原19字节未变且同版已重跑。

无完整任务3.2/完整CL、ACP结构保真、渲染/分页/授权、实际owner/pump/WSS或真实Harness结论；这些仍须后续方案/测试/交付门槛。整体ACP revision2另有独立预审：原VPROJ-PLAN-001仅在后续c978方案定义层关闭，新002/003仍Open；这与本Domain纯端口交付结论相互独立。
