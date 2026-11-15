# 管理输入 revision2 独立复查

2026-10-31，原问题提出者 `/root/baseline_reviewer`。只读新候选和旧冻结来源；未修改、合入产品，未运行用户 Harness 或读取真实身份/数据库。

本次限定结论：**MGT-CODE-001/002 的窄修复静态复查通过，待同版独立原断言及 M 矩阵执行结果核对后关闭问题。** 不用开发33测试代签，不在这里把原候选报告改成通过。

## 冻结与差异

- 新根 `../deliver-codex-session-loop-management-revision2/`，实际读取 HANDOFF、manifest、生产源码和 delta。
- 独立计算 manifest SHA256 `003850e2f9c7594dd9b4805c4479e62a774b24deed873f1e3c2d010d7d8ba5d8`、delta `e5626be221c55bc31e47362b833f7ae826590736e785a28cd30f07826fa1faa3`、完整 patch `fc9b02d38cbb58ab178ec2429945f64a04d8520082d1ca13d81b55c1fd65d931`，均相符。
- 78份新文件逐一核对 manifest，全匹配；同78份旧冻结来源逐一核对 revision1_sha256，全匹配，原目录未被修复覆盖。仅3源变化：应用模块及2份开发测试。
- 新应用模块 SHA256 `d47b96698c5eec1a1618e11a3609aa159b8f49ea1c45a376844230ca821635ff`；应用开发测试 `89352100fa0ea88529271fc88493b25a32f5ac8586be3caad193c4cc3849fcac`；SQL开发测试 `b19aeca2070712ac85d9739daeac2fdc42af29e0b050299aa7fa3dc0df6a8122`。SQL生产模块、DDL、Cargo锁及其他内层模型未改。
- 合同及22组M定义仍按8686d779/009f48b0，不因修复缩减范围或改预期。完整patch的共享mod仍须逐行合并并保留主区views/ACP声明。

## 窄修复核对

**MGT-CODE-001：静态修复符合原要求。** InputCell 的期限移入共享 CellState；tighten_deadline 使用min，旧上传寿命及后续调用均不能续期。真正验签后再直接采样，并在采样前记录单调锚点，用checked_add转换签名剩余时间，避免同步采样/checkpoint延迟被再加到正文寿命。新鲜样本已过期时另确认必要拒绝水位。expire、append、finish、digest、borrow均读取同一cell状态；tighten退出state锁后才retire，没有引入同锁递归。后台supervisor可在ticket/permit仍被保留时清理自己的正文和预算，不依赖以后claim触发。真实时钟装配和运行期全局故障联动仍属于后续接线边界。

**MGT-CODE-002：静态修复符合原失败交错。** 最后异步actor/owner检查、时间采样、body借用全部结束后，再检查shared fault、取消及单调截止；该检查和首次write_once调用之间没有新的await。检测失效先释放正文、按可靠零发送保存结果并拒绝；无法确认保存则仍故障。writer已进入后的错误/取消继续unknown，未被新零发送分支误改成从未发送。该检查是首次消费的线性边界；真实writer内部排队/Drop后资格失效依旧需要可信适配器实现，不能因此宣称真实网络撤销已验。

## 另外核查中的原因分类

父执行提出最后分支用`now>=write_deadline`写Expired，但write_deadline实际合并了操作10秒、writer2秒、请求及对象期限。合同§7真实请求到期才Expired，其他可靠零发送是NotSent→failed。现正在请独立tester验证公共API可达交错：最终同步TimeSource采样延迟跨操作总deadline，而签名期限仍有效。结果出来前仅保留疑点，不伪报已复现漏洞，也不借此否定001/002对原问题的修复。

## 未签范围

本文件尚未签本阶段完整独立M矩阵、主区集成、真实认证/资源/内容/ACP writer、WSS、安全控制、reconciliation、恢复ready或L5/L6。开发33/33及旧模型/签名回归只作交接信息。独立原版反证及29组扩展执行由request_tester持有，须实际读取新快照执行日志/清单后追加最终限定结论；原001/002失败历史保留。

## 同版独立复验与原问题关闭

2026-10-31追加：已实际读取request_tester的`management-independent-revision2.log`、其exit、`management-revision2-final-exit.json`、`management-test-unchanged-revision2.json`及初始来源清单。独立原29项同预期执行29通过、0失败/忽略、exit0；其中正文到期与最后await中共享fault的原失败断言都通过。原公开活动/请求模型21项回归、lib测试目标clippy各exit0，外部负向编译预期拒绝另记，不作正常成功编译冒称。

reviewer另行比较当前独立副本和冻结78源：仅SQL源多3行cfg(test)include，已实际逐行diff，生产逻辑无改；应用源仍为d47b9669。原29项资产执行时两副本摘要均为`24baf5783582bbd25d8d006f0a7500fe7441f6f65fa691499c3488f2f852bfbc`。后续为下面003新例加入2行测试模块声明，没有改变原29项断言，当前该测试源摘要因此与原执行清单不同，未把后续资产冒充原运行资产。

**原提出者正式关闭MGT-CODE-001、MGT-CODE-002（revision2限定实现与同版独立原断言通过）。** 原版a590/39b4的失败证据仍保留；本次关闭不批准真实来源、网关或writer发行者，也不关闭其他完整CL/L5/L6义务。

## MGT-CODE-003 — P2，open：把内部预算截止误记成签名请求过期

这是父执行在窄diff提出、reviewer核合同后由独立tester新增公共API复现的另一问题，不是001/002重新打开。位置为revision2 `application/product_commands.rs:1212–1221`的最后消费检查：`write_deadline`合并操作总预算、writer阶段预算、请求及对象期限，`now>=write_deadline`却直接选择`InputResult::Expired`并返回Expired错误。

合同§7规定真实请求到期才记expired，其他可靠零发送原因记failed。内部10秒操作预算耗尽时，即使原24h身份仍有效，候选也会把command及result记为expired，错误表达身份资格和拒绝依据。此问题没有导致重复发送；实际零writer和正文清理成立，但持久事实不准确，仍应在本阶段合入前修正。

已实际读取`management-revision2-deadline-semantics.log`、exit及独立新源`management_deadline_semantics.rs`：最后refresh完成后，可信TimeSource同步采样延迟10.3秒，返回NOW+10300的真实可验RequestTime，24h签名仍通过独立再验；writer=0、正文=(0,0)，但command.status实测expired、预期failed，1项失败、exit101、实际运行10.41秒。不是只凭不可达私有状态或猜测报告问题。

窄修要求：保留分别可辨认的签名有效期、对象寿命和操作/阶段绝对预算；只有真实签名截止到达才InputResult::Expired/Expired错误。对象/操作/阶段耗尽或取消而可靠零发送时，按实际原因NotSent→failed及Cancelled等准确错误处理；共享fault继续保持，不重新排队、不续期、不新增permit。可能已发送之后仍unknown。不得把新例预期改成expired来换绿。

最终结论：**revision2原两项已关闭，但因MGT-CODE-003仍不通过合入门禁。** 请保留原版及revision2冻结、失败和29项通过日志，另版窄修后同版运行原29项及新原因分类用例，再由原提出者复查003。当前未合入主工作区。

追加证据：已读取正式`management-revision2-acceptance.md`、终态来源清单及`management-revision2-deadline-semantics-expanded.log`/新增源码。终态78源仍全部匹配。003现有两个独立失败分支，第二个为公开reserve的500ms对象寿命、final refresh暂停700ms、24h token再次验签仍有效：writer=0、正文=(0,0)、command=failed，但API返回Error::Expired，其领域语义为“product request has expired”。这同属003的原因混用，新增对象TTL分支不重开001/002。两项合计0通过/2失败、0忽略、exit101、10.42秒，原29项通过仍单列保留。revision3须共同修正持久状态与错误分类，不仅把最后一个status分支换名。
