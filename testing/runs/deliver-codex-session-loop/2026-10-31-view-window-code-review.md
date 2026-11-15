# 纯视图窗口独立代码检视

2026-10-31，reviewer `/root/baseline_reviewer`，只读产品源码，未参与实现。

结论：**当前纯容器静态检视通过，未发现受审范围阻断。** 仍待独立tester同版公开API执行；开发9测试/clippy不代签。此结论不包含结构适配、SQL、WSS、Node权限或真实内存峰值。

固定源码：`app/src/domain/views.rs` SHA-256 `ee531f845af4050809721d88e41fbce0f0bff16235cfdb763715a5dc845ca612`；`app/src/domain/views/tests.rs` `61e240cf36587e8556235d7fcf512903c978d813d4f5a98db38f510e21569e41`，均由reviewer实际读取并自取摘要。依据合同`2d8b7362...f00de8`和独立A–J定义`9bf00176...5241a`，完整摘要见阶段规划报告。

## 核对

- NodeBudget与TargetBudget共享双账本，reserve先计算全局/目标两项checked结果，再同时落账；失败不单边记账。所有reserve/Charge Drop均global→local锁序；TargetBudget最后Drop只取global，不在仍持local锁时级联取得global，没有逆向锁序路径。
- 计数下溢将ledger永久标broken而非回绕制造配额。根、身份、窗口、条目与快照收费明确；目标NativeKey跨所有旧读/资源代际复用同一Arc预算，不按代际重领16MiB。
- 固定64个Box槽、每槽一个当前Window，无全历史HashMap/区间；LinkedList淘汰真实释放节点，可靠键最多当前条目线性查找。键namespace闭合枚举，字符串限4096，字节和键boxed保留；快照用精确boxed slice且预留后才复制引用。现有固定元数据结构处于保守费用内，未发现隐藏保留capacity或动态namespace。
- Charge持TargetBudget，旧快照/旧条目最后释放前预算与身份仍存活；collect只在无窗口且预算无其他引用时回收。ViewHandle只承担scope/定位，公开快照不Clone、不暴露Arc<Item>，bytes/key仅借用；调用者主动复制/保存外层句柄及数据仍需外层预算，不能接无界发送队列。
- scope、NativeKey、ResourceEpoch及checked generation联合校验；跨容器/旧回报不能改当前窗口。Reading重复begin Busy；结束后begin先撤旧，即便预留失败也不回滚旧为最新。旧snapshot冻结自己的代际事实。
- Reading/Finished/Incomplete分开，失败首原因sticky，finish不洗白；history_complete同时要求Finished且无淘汰。该布尔只描述读取保留窗口，不是Run终态或权限。
- insert重复当前键Conflict，无键不做正文去重；replace仅找当前键，缺失不append，双份候选预留失败保留旧内容和其他条目。成功替换保持原位置，旧snapshot仍持旧字节。FIFO淘汰仅本目标且以水位/计数描述，不取消工作或清其他目标。
- snapshot收费包含空快照和每引用，失败不交出伪空成功或新租约；分配失败时RAII Charge归还。Drop顺序先释放内部引用后释放快照费用、budget最后退身份费，未提前退被其他snapshot持有的条目费。
- 模块不依赖旧持久history、文件、网络、SQL、审批或执行控制。raw为惰性字节，不解析/执行HTML或URI，不从来源键构造权威历史身份/轮次。无新增依赖或正常运行显式unwrap/expect/panic。

## 限制

该API调用者是节点内部owner，不凭NativeKey/ResourceEpoch授予客户端权限；后续逐块client_gate检查、结构规则、当前完整性状态与旧快照响应限制仍需真实接线。纯账本不证明总进程RSS或外层复制/队列有界，也不保证资源不足时所有快照都能创建。

本reviewer未运行测试或构建，仅静态阅读实际实现/开发测试和摘要。独立A–J执行及最终候选变化需另审，完整L5/L6及任务3.2未签。

## 同版独立执行与最终限定结论

2026-10-31，已实际读取tester `view-window-acceptance.md`、`view-window-test-final.log`、`view-window-acceptance-source.json`。生产两源、native、domain声明及产品锁与其执行前后快照一致，reviewer再次自取views及开发tests摘要仍为上表值。独立公开API资产`app/tests/acceptance/view_window.rs` SHA-256 `28f2df93e92ebc159a120b21336f6983ad1043856dd11af387d5b7ecaee967d8`，集成入口`cb7e7808201a066f8f47f635c3190624c006070e304f7729939515b2efb791a7`。

最终原始日志实际19项全部ok、0失败/忽略、38 filtered，exit0。覆盖明确费用的目标/节点字节及默认4096/32768条目+引用边界、跨线程Drop、跨代际同目标合账、64身份最后lease、不可变字节、FIFO水位、替换双份失败原子性、scope/状态及snapshot自身费用。测试期望费用取合同literal而非产品常量，未调用开发私有字段制造通过。没有将filtered计为执行或单靠报告路径签结论。

两项未动态分支如实保留：结束后的同目标begin先移除已计费1024窗口再预留同量，在固定limits/独占写的正常公开路径无法制造账本容量失败；其错误传播/不回旧顺序仅独立静态确认，首次begin分配失败另有实际证据。u64内部序号/位置与usize账本极值同样只静态审checked保护，未冒称公开API运行或将开发私有用例当独立执行。系统分配器OOM未注入；128MiB字节边界以缩限路径取证，未宣称整进程RSS。

最终结论：**纯内存视图容器的静态布局/安全检视与约定可达公开API L3子范围通过，当前无未关闭阶段问题。** 不签任务3.2整体、全平台预算、结构适配器、权限撤销/逐块交付、SQL、WSS、浏览器或真实Harness；原完整L5/L6义务继续未验。
