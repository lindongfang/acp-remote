# Domain同步借用与revision：独立阶段定义

2026-10-31。本文件为编码前规划，**DBR01–08全部not-run**，不改产品、冻结测试或其他tester资产。依据root `domain-borrow-revision-contract.md` r6 SHA256 `0f8b17f3d85bed56b1c8f8917eafb7e2252b14a151b942ef4dc549d8c4c7e321`、原view-window/view-budget-port合同及实际Domain源码；摘要清单为 `domain-borrow-revision-inputs.json`。DBR是既有视图/预算CL下的子断言名，不创建新CL或降低原L5/L6。

最终按root r8 SHA256 `e75e54d7bcc15efb18cab339fbfcd19deed852b7d7f15d0f056860ca0954430e` 的§6A复核冻结。r8已补sealed逐API结果、no-op和精确私有极值hook；DBR-TG01/02在定义层关闭，实际8组仍全部not-run。初读r6仅为变更来源，不与r8混用。

只验证同步HRTB（高阶生命周期约束，防回调内正文借用逃出）端口、绑定stamp及统一ReadMeta版本，不包含ACP/current/scanner/输出队列/16KiB读取预留/授权/WSS，也不等待独立副本的P2A。WindowReadStamp是只借原外部ViewHandle的固定控制元数据，不持有正文、不证明读取授权、不承诺网络发送期间最新。

## 最小接口与固定语义

按r6原签名：`with_window<'h,R>(&self, h:&'h ViewHandle, f:impl for<'w> FnOnce(BorrowedWindow<'w,'h>)->R)->Result<R,ViewError>`；`validate_read_stamp(&self,h:&ViewHandle,stamp:&WindowReadStamp<'_>)->Result<(),ViewError>`。guard提供冻结`status()`、返回外部h生命周期stamp的`stamp()`和借用`items()` ExactSizeIterator；stamp的`revision()`为u64，私有字段，允许Clone/Copy。guard不Clone，不暴露Window/Arc/ReadStatusCell或owned body lease。

with_window先校验h完整scope/target含source/resource/read，再短锁一次复制state/revision（sealed立即SequenceExhausted）；解锁构建冻结header及执行f一次。返回后再短锁，sealed优先SequenceExhausted，revision不一致ReadChanged，否则接受R。失败须先退出锁再销毁R；禁止自动重调f。读取Reading/Finished/Incomplete均可，状态只如实进入冻结header；Incomplete不是with_window自身错误。stamp验证先找当前h、再同一完整same_read比较stamp原handle、再sealed/revision判断，不能只比较两个整数或裸地址。

ReadMeta(state,revision,sealed)共用一个Mutex。原/新insert、replace、FIFO每次实际前缀移除、第一次state变化都与checked版本推进在同一短临界区提交；资源准备/节点分配/预算申请在锁外，旧Item/Charge/stage析构在解锁后。保留node→target预算锁序，不持Meta取得预算锁或调用回调。原WINDOW_BYTES=1024及RAII费用不默改，新增实际size/capacity必须在原收费内取得编译/源码证据。

## 执行方法与隔离

后续优先用外部普通依赖跑公开API；借用负编译与stamp正控制分别保留源码/确切诊断，不用include产品代码伪装公共API。极值仅隔离copy下crate-private cfg(test)窄入口；不在root或生产开放setter。不新增依赖、网络、audit、用户DB/密钥或真实Harness。执行前冻结源码/锁/测试SHA，独占target和自有测试资源。

零分配探针应有单独global allocator测试二进制，提前建立fixture/诊断设施并启用当前线程计数；第一次及重复with_window都测，callback仅做栈上长度/校验和/比较，不format、collect、复制正文、创建线程或输出。调用前后target/node usage与items不增；不能用callback的分配混淆Domain承诺，也不能以0alloc代签0 Arc clone，后者须独立源码审查/必要cfg测试证据。

死锁负例在测试自有独立子进程或等价可真正回收的隔离runner执行，父watchdog有界等待；若挂起只终止/回收本测试child并记录失败。不要用超时后仍留死锁线程的spawn_blocking来假清理。线程/屏障/返回R准备放在分配计数区间外；此类用例不声称整个callback包括线程创建都0alloc。

## DBR01 — 满额无owned引用遍历与冻结header（not-run）

操作：空窗口、少量有key/None-key窗口、默认同目标4096对象，以及同节点8个目标各4096、总32768对象分别构造；所有对象实际收费且合法，不借fixture越过上限。先读usage/position/水位，with_window遍历所有条目，核ExactSizeIterator的len和next递减、key/bytes/顺序及首末位置；返回栈上计数/校验和与stamp。重复在Reading、Finished、既有Incomplete三种状态下读取。对照满条数普通snapshot仍需新引用而可能Capacity。

预期：4096/32768真实满额下with_window都可遍历，不需要snapshot/item_lease/额外items配额；Domain调用0新堆分配、0 Arc owned-item clone/租约，调用前后账本与items相同。空窗口也返回准确冻结header。guard.status反复读取不读live cell、不额外锁/分配，guard/stamp/ReadMeta实际栈/堆元数据布局在原费用内证明。此端口只解决内层借阅条件，不能据此签首次客户端交付/输出预算或授权已接线。

## DBR02 — 生命周期负编译和stamp正控制（not-run）

操作：每项独立小crate或逐源编译，尝试将BorrowedWindow、单ViewItem、bytes/key的借用、iterator、Vec<ViewItem>、捕获这些借用的async future从with_window返回，或写进外部槽；尝试Clone guard、公开构造/反序列化stamp及把stamp借用h活期扩成static。正控制返回`guard.stamp()`、固定header/自算数值，复制stamp并用同read的合法handle clone验证；借用h一直活着后再修改windows。返回只持stamp/数值的future可作为正控制，不把一切future都误禁。

预期：正文/iterator/guard逃逸因生命周期/封装确切拒绝，不能只是导入/路径错误；stamp能以原外部h生命周期作为R返回，不强迫保活正文或把with_window对windows的共享借用延长至stamp寿命。h被Drop后继续使用stamp须编译拒绝。stamp Clone/Copy合法，不机械沿用snapshot非Clone限制。调用者在callback内主动复制成自有Vec<u8>属于其另计预算，类型系统不禁止任意复制；不能把这种允许误报Domain提供了免费owned body。

## DBR03 — 完整身份、旧代际与ABA（not-run）

操作：两容器使用相同target/source/native字符串、resource、read generation和revision数字，分别取stamp后交换h/stamp/容器；再逐项改变target/source、nativeID、runtime/connection/load及read generation。用原h的合法clone验证。释放原window并新begin后分别传旧h/旧stamp和新h/旧stamp；保留借h的旧stamp，Drop原容器再创建同数字的新容器。

预期：只有完整same_read且revision当前者成功；错scope/target/source/resource/read均Stale，在sealed/revision比较之前拒绝，不能误归ReadChanged。h clone同完整身份可通过。原h因stamp借用仍活着，scope Arc不会先释放再被地址重用，不靠概率“未碰到地址碰撞”证明ABA；实现必须保存&ViewHandle并比较私有scope身份，不能持裸pointer或序列化(gen,rev)当跨容器凭证。release正常清理旧window，新stamp不能由旧read行恢复。

## DBR04 — 回调/跨线程Drop、冻结状态与失败R析构（not-run）

操作A：预备同cell未publish stage，初始state=Reading。with_window callback先记header/revision，正常Drop该stage，再读guard.status两次，调用windows.window_status/usage及嵌套只读with_window；返回普通R。B：另一线程用屏障精确在callback中Drop同cell stage，等其完成再结束callback。C：预备同cell stageA与stageB，callback Drop A制造首次Incomplete，返回持有B且Drop会释放B的R；记录R析构次数。D：先让state已有Incomplete，再在callback Drop该cell另一stage；另对照旧generation stage在新read callback期间Drop。

预期：A/B不死锁，callback只执行一次，guard header始终为pre时冻结Reading，live state已Incomplete但post返回ReadChanged。C在解锁后Drop R恰一次，B的析构能重新进入同Meta/预算锁而不死锁，不泄漏其charge。D中既有sticky Incomplete的重复fail不改revision，当前with_window可成功并保留原原因；旧generation Drop只影响旧cell，新read/stamp不变。callback可正常清理，但不能发送网络/提交SQL/批准/发布成功页；Domain不能回滚用户I/O，约束须文档明确而非虚称类型强制。

## DBR05 — 原/新变更统一revision与真实错误FIFO（not-run）

操作：新read rev0。分别测试原insert/stage publish insert、原replace/stage publish replace（包括key/bytes完全相同的新对象）、finish Reading→Finished和重复finish、首次fail和重复fail/不同reason、未publish stage Drop、writer overrun/poison/short publish、workspace/candidate分配失败首次变Incomplete、lease CAS冲突。另用完全合法的小items预算：window有两个可靠key对象，各持一个item_lease，items已满；原insert与stage_insert分别因无法回收被lease持有的对象而实际FIFO移除两项后仍Err(Capacity)。还覆盖FIFO后成功publish、真实bulk null分配失败（同版独立allocator资产或隔离探针）。

预期：每次真实publish/replace各+1，即使bytes相同；每次实际FIFO移除各+1，随后若首次使state Incomplete再+1，返回Err不抹掉水位或漏版本。已poison/已有Incomplete后stage Drop不能重复推进。无变化的workspace/lease/snapshot成功申请和正常Drop、metadata只读、重复fail/finish及零发布且无状态变化的早期拒绝不凭调用次数加版本。原insert重复key无state变化不加；staged Conflict/InvalidKey若沿原合同首次使state Incomplete则须+1，不能以“没发布”漏掉这项变化。各路径用独立确定顺序取stamp与预期delta，不能只看最终stamp失效；fail不覆盖首原因。body位置/费用/原错误码沿旧合同，不为加revision先释放旧条目或偷偷insert/replace互转。涉及实际分配失败未执行时须逐分支not-run，不能拿预算Capacity代签malloc null。

## DBR06 — post线性化与不自动撤销/重试（not-run）

操作：callback不改变cell，post成功后将返回的stamp交调用者，再由预备stage Drop使状态变化，调用validate_read_stamp。另callback中制造ReadChanged并仅写入调用者已预留的私有buffer，在Err后检查调用者将该candidate作废；返回次数与callback执行次数计数。安排R的Drop在Ok返回后才释放一个同cell stage，与DBR04失败R解锁析构对照。

预期：第一次Ok证明其post比较点一致，后来的stamp验证ReadChanged；不能把后来变化倒算为第一次读取错误，也不能宣传Ok保证未来网络发送时仍最新。成功返回R不由Domain提前Drop；其后由调用者Drop导致的新变化按实际时间推进。ReadChanged/SequenceExhausted均不自动重复callback、不发布候选、不发协议/管理动作；调用者按后续owner/授权提交协议负责输出。私有buffer副作用可丢弃，不把Domain作为可回滚任意closure的事务系统。

## DBR07 — MAX封闭、no-op和窄私有极值（not-run）

操作：在隔离cfg(test)副本通过私有`RevisionBoundary::{Penultimate,Last}`及`set_revision_boundary_for_test`设当前read revision，不改变state/sealed/items/position/ledger。MAX-1下最后一个真实publish/replace/state/FIFO变化能到MAX；MAX未sealed时先取stamp/with_window，下一需要真实变化的各入口分别尝试。首次失败也由stage Drop/overrun触发一组；已有Incomplete时重复fail保持no-op。另让callback中Drop stage在MAX触发sealed，再postcheck；最终release/新begin/旧stage清理。构造需要的workspace/stage/lease/BoundedWriter在封闭前完成，封闭后逐API验表，含候选已poison/short、missing key、wrong scope/旧read和预算不足的错误优先级。

预期：到MAX的最后一次合法变化成功；下一真实变化sealed、不回绕，不改变items/position/watermark，首原因若原非Incomplete则变SequenceExhausted，否则保留旧原因。旧stamp/with_window/publish均拒绝SequenceExhausted，callback预检sealed时不调用；post时sealed优先于ReadChanged。未sealed的MAX且已Incomplete，重复fail返回Ok且首原因/rev不变；Finished重复finish同样Ok；Incomplete finish仍Incomplete，都不因no-op封闭。真正sealed后显式fail/finish变为SequenceExhausted。仅收费退账不耗版本。Drop无panic，release/换代可回收，旧cell不污染新rev0。hook仅当前未sealed read可用、stale拒绝、sealed不能复位；Last→Penultimate必须返回Conflict且不改任何状态，不得降低计数或绕过封闭，不能把注入MAX签为实际运行2^64次。其余API按r8下表，不偷加统一读禁令。

| r8 sealed子矩阵（本组内） | 固定预期 |
| --- | --- |
| with_window、validate_read_stamp | SequenceExhausted，callback不调用；不能重新取stamp成功 |
| window_status、snapshot、item_lease | 原诊断读取/查找/容量规则仍可用；状态如实Incomplete，snapshot/lease仍收费且不提供stamp一致性承诺。合法但不存在key仍MissingItem、额度不足仍Capacity，不把“sealed优先”泛化到该明确例外行 |
| stage_workspace、insert、replace、stage_insert、stage_replace、publish、finish、fail | 保留各API原参数/跨read身份检查；匹配当前cell后SequenceExhausted优先Conflict/MissingItem/Incomplete/poison/short/容量，零新工作区/发布/FIFO，不覆盖首原因 |
| 已有BoundedWriter | write_all复制前检查原cell sealed并返回SequenceExhausted，written/正文不变；written/remaining和workspace scratch只为已有私有候选访问，不发布 |
| abort_stage、stage Drop | abort总消费清理；foreign/stale仍Stale，matching sealed为SequenceExhausted；Drop只安全退账，不panic、不重改state/rev |
| release/collect/begin及已有snapshot/lease/usage/Drop | 原行为；新read rev0，旧sealed不解除。旧快照的冻结header仍是取得时事实，不伪装成当前一致读取 |

## DBR08 — 费用/锁序源码与同版回归（not-run）

操作：固定ReadMeta/ReadStatusCell/guard/stamp及Mutex/Arc控制块实际布局，核WINDOW_BYTES1024和已有工作区头收费足以覆盖；审查原/新所有变更是否准备资源在Meta锁外、短锁内checked+实际发布、解锁再释放Item/Charge/R。查std LinkedList节点移动/pop路径是否在锁内触发Item/Charge析构，不能仅把pop返回Arc就宣称没有任何节点释放。跑原view19、预算20+6、P1原11及适用公开回归、负编译与root fmt/clippy，记录各自同版source/命令/数量。

预期：不持Meta回调/await/取node或target锁/Drop stage、Item、Charge；原node→target锁序不反转。无先解锁预增rev再无条件发布的sealed竞态；失败发生FIFO后的水位和版本都是真实已提交事实。0alloc/0owned-body引用、RAII/CAS/原费用不回退；无法证明新布局在常量内须先复审，不默加免费metadata或改预算。此scope不改ACP/current/scanner/SQL/WSS/输出队列或依赖，原P1/private极值/真实Harness等证据范围分别保留。

## 定义余缺关闭与编码前门槛

**DBR-TG01 closed-definition。** r8§6A已固定“诊断legacy读取可用但无stamp承诺、新构建/写入sealed拒绝、Drop照常退账、release可回收、新read独立rev0”逐API表，以及未sealed MAX时no-op返回。上方DBR07按此冻结，不新增“sealed后所有legacy读取均拒绝”的规则。错误优先级句按表中确实做sealed检查的API适用，不能吞掉诊断读取行的原MissingItem/Capacity。状态/身份优先级与原参数检查并无新增产品分支。

**DBR-TG02 closed-definition。** r8§6A已给私有cfg(test)枚举`RevisionBoundary::{Penultimate,Last}`和`ViewWindows::set_revision_boundary_for_test(&mut self,&ViewHandle,RevisionBoundary)->Result<(),ViewError>`，只设置当前未sealed read的revision、不改scope/state/ledger、不unseal/复位，Last→Penultimate返回Conflict且不改变状态，生产/普通依赖不可见。极值在隔离同模块子测试执行，公开行为与内部注入证据分列。ReadMeta/guard/stamp实际布局及锁/析构实现仍须编码后验；定义关闭不是编译/动态通过。

以上8组按r8冻结，TG01/02由原提出tester在定义层关闭，交reviewer限定编码前门槛后即可只编码Domain小端口，不等待P2A/current/WSS。此刻没有新增运行结果，也不因合同文字存在关闭满额交付、P2工具原位、真实来源权限、实际节点/Windows/Harness或完整L5/L6义务。禁止网络/audit外发保持不变。
