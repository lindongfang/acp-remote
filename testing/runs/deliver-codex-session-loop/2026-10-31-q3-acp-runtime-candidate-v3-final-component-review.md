# ACP candidate-v3 最终组件交付复核

结论：**限定 PASS；下表八个 ACP-IMPL 问题 Closed。允许按固定产品 patch 与 portable-v1 窄合，再做根组合检查。** 本结论不是根整库通过；`ACP-INTEG-LEGACY-CANCEL-001` 仍 Open，旧 C 用例到达同步修复及其组合证据单独审查。

本报告由独立 reviewer 只读核验实际文件、源码、原始日志与断言后形成，未修改产品、合同、独立资产或历史报告，未把开发自测代签独立验收。组件执行由独立 tester 完成，reviewer 本次不重复运行该矩阵。

## 固定输入与同源核验

| 输入 | SHA256 |
| --- | --- |
| 产品 candidate-v3/source-manifest.json | 98393e3f0a5b39d0db9c98f456ec891af8602c4ea3014ed233476d2af622cc85 |
| 产品 build-inputs.json | 3fe26f29438d580a5867ac9580ceb00c2eb6f9b00573d2955279e446a68eb8f5 |
| 独立 acceptance-v3.md | 74510034f5f3feab680add6bedc810ecc68e3e9b9517c911f433d415ae415c88 |
| 独立 HANDOFF.md | 39d33faf8681983c5e3def0b46e137a48618929ccaf4062dd2faac18cb21d31f |
| 独立 delivery-manifest-v3.json | 03093c71551c0e7ebf8a400b447b5dd38249f615b7fcaa0625d8fc1f3e339d8a |
| portable-v1/assets.json | 36ebf5f0e5b93e7d66a2e183b65f4b53ad36d05e03c51c6dc288a5727c974b16 |
| portable-v1/test-only-cargo.patch | 52634bc0f36bf57b340a67e43fdb9556d888a864fefeed548b34e8fd75b52dd2 |

以上位于同一 visualizations base 的 `session-loop-q003-acp-runtime-implementation/candidate-v3` 与 `session-loop-q003-tester/acp-runtime`。合同64cc19be、限制8bb5d345、empty补充7f682592、独立定义f27bfc62/077a994a的绑定不变。

reviewer 再次逐项计算产品固定目录及 tester 实际执行 copy 各190项，均0差异；交接29证据与 portable64文件逐项实算均0差异。前次完整静态复核及构建输入核验见 `candidate-v3-static-review.md`，SHA `7bd963bdcb2466d8fb0bc84b6a48ed41fb591ffe2021abb2f02997bce42ff4d5`，不回写该历史静态报告。

相对182基线只有ACP内14改、8增，共22路径；raw基础、Cargo依赖/锁、SQL、Node、Web及旧独立资产未被产品候选改写。v3相对v2的5路径与精确Rustdoc/FIFO修复已核；权威baseline patch `3cbc956760c0b56749809a3544fd8179ce32dc90516a8335af1fe480432fce6c` 已做只读 apply --check 成功，未由reviewer应用。

## 实际验收与证据限制

最终 `accepted-candidate-v3/final-owner-ultimate.txt` SHA `dc5ae9a0641c1e3e3bb067de0c42766b6db7799c017130fa8ca42ae12a59dd4e`：cwd为固定source，实际 rustc/cargo1.98.1；既有 windows-owner.ps1 完成真实新对象owner检查。默认并发、显式fixture feature，120/120、0失败/忽略、退出0。逐行解析原始日志得到120个唯一通过函数，与 `executed-120-v3.json` 的120名字完全一致：84新增、33旧主函数及3旧导入函数。98定义行逐项有源码/日志或明确静态证据；导入3个名字在日志中带 `ordering::` 前缀，已解析核对，没有少跑。

`compile-fail/results.json`及七份原始JSON诊断已亲读：同imports正对照check0；A/B1/B2为预期E0308（不可变getter、Incoming不能代替Received）；C1为Received私字段构造拒绝；C2/C3为缺From/Deserialize的E0277。主错误span均指向各例main.rs。不是任意失败被算通过。私有Received无可公开构造/反序列化捷径，公有投影先核scope；不提供测试品牌工厂。

独立probe的严格all-target+fixture clippy实际0，fmt0；clippy日志内的增量缓存hard-link退回复制警告属于工具缓存，不是被忽略的Rust/clippy诊断。默认build的JSON `build-finished.success=true`；默认前后child均不存在。依赖库检查不代签根所有target，默认build不代签显式fixture实际执行。

精确动态边界按报告保留：RC01/02日志 `later-written=false`，不声称命中锁外parse期间后来写完的短缝；DL07/08的preAt跨parse及精确等At只静态核锁序/判定，邻近公开期限真实跑。真实OOM、计数MAX不可公开安全触发，未伪造动态通过。正常早响应、半帧、每poll围栏、取消/排空、无runtime Drop与实际join等有公开运行。

## 八项问题关闭

| 稳定ID | 最终同源静态与实际证据；关闭范围 |
| --- | --- |
| ACP-IMPL-WRITER-001 | 实际W.Err在同State锁记录Unknown/Spent/Stop；真实flush成功同锁记Written。`writer001_actual_error_or_flush_then_receiver_drop_inside_legal_fence`实际poll返回后、fence返回前Drop真实receiver，分别保Unknown(Io,0)/Written并实际join。非法后置flush fence的独立例仍封连接/InvalidFence，底层flush只一次。Closed。 |
| ACP-IMPL-READER-001 | State.stop/finish共享Core Notify，直接expiry Stop同样唤醒。公开自主期限例在任何显式shutdown前取得terminal，accepted3仍Unknown Deadline，再None。精确瞬时短缝属静态证明，未冒称复原旧时序。Closed。 |
| ACP-IMPL-READER-002 | 真实R.Err/EOF在poll同锁先Stop。公开双分支在该真实poll打开writer后核无新写、poll计数不越界、票据Zero ConnectionClosed。Closed。 |
| ACP-IMPL-LIFECYCLE-001 | 正常collector和JoinErr/None fallback共用逐槽drain_io，借用等待后才清handle；同总deadline、串行、取消保槽。实际runtime A shutdown_background且W析构已受Condvar门闩阻挡，runtime B首次Pending后取消、短deadline joined=false，放门后joined=true且R/W Weak消失。不是把collector取消当R/W已退出。Closed。 |
| ACP-IMPL-PROMPT-001 | uri string且text/blob至少一支string，非互斥anyOf，未知字段原raw保留。双版10个非法结构最终均拒绝；text/blob/both及一支合法另一支错类型正例全部保留。原1.98.1真实红保留。Closed。 |
| ACP-IMPL-SEEN-001 | seen登记在live建grant判断外；ClosingDrain不豁免连接寿命防重。最终permission/unsupported两分支同真实read cut均duplicate_emitted=false，合法尾帧继续排空、不建新reply grant、首terminal保留。Closed。 |
| ACP-IMPL-DEBUG-001 | Response自定义Debug屏蔽RemoteError数值，公开code/raw读值不改。最终双版Response/Incoming/Received数值canary均无泄漏，并核直接读值及其他类型Debug矩阵。Closed。 |
| ACP-IMPL-REJECTED-001 | rejected为VecDeque，逐有效JSON坏路由push_back、逐次take FIFO；非法JSON不伪造RawMessage。最终双版retained=2、依序原raw再None，尾合法帧排空和首Unmatched终态不变。Closed。 |

前四项为开发过程静态发现：不存在独立固定旧preimage/旧红运行的部分没有补造证据。后四项冻结v1的实际红日志及v1/v2文件均保留；最终绿只配当前v3。类型/时序闭合依据仍见相应过程报告与四问题报告，不以“120全过”替代具体问题证据。

## 旧36迁移与64份包

reviewer已读原5文件完整差异与真实IO bridge；本次又将最终包的5个受保护函数逐断言与原件归一化比较，数量分别10/10、10/10、3/3、3/3、4/4且内容完全一致。原私造Incoming与raw不一致攻击现被Received公开不可构造/不可变约束消除；迁移采用真实异scope→InvalidObservation、同scope异session→Ignored、真实旧view→Stale，结合正1反6与静态来源证明，不宣称同一旧攻击动态重现。删除Message/Tool纯4096门槛的相关长输出正例按冻结分类调整；Session/Option/RPC等原控制语义与旧有限投影预算保留，未伪称完整无限共享视图。

包中 `acp_runtime_acceptance` 下资产与最终实测文件逐字节相同；旧3主target只桥接相对路径不同，2个导入helper与实测文件相同。相对wrapper仅挂载相同资产；7个独立编译probe的main.rs与Cargo.lock与实测原件相同，Cargo.toml只把app依赖落到相对 `../../../../app`。检索无用户绝对源码/临时目录依赖。fixture中的C:/synthetic只是协议数据。

Cargo patch仅增加空opt-in feature与required-features控制的自有Rust child bin，test=false/bench=false，无新依赖、无默认启用；可批准此测试构建边界。合入时复制其余63项，对原Cargo执行patch --check后只apply一次，最终Cargo应与包内reference一致；**不要先复制包内Cargo再apply**。根必须在实际落点做fmt、所有target严格检查、默认build及显式feature的实际fixture运行，不能用独立probe代替。

`io27_actual_ownedprocess_unbuffered_pipe_and_complete_owned_tree`及child源码已亲读：双版真实OwnedProcess未缓冲stdin/stdout、独立stderr drain、握手/new/prompt/嵌套permission/cancel；v1真实广告close，双方实际发送标准session/close并取得Response::Closed之后才IOowner.shutdown，随后本次Job active_processes=0且Incomplete被拒绝。它不是仅owner.shutdown冒充协议close；不是实际Harness、未来原生任务、或多层descendant树能力认证。

## 仍未签署

根旧C timer取消问题与整体512库失败仍由 `ACP-INTEG-LEGACY-CANCEL-001` 独立保持Open。本报告不签SQL派发账本、actual R3 owner接真实IO、NativeDispatch新输入顺序/原生因果、Node/WSS、完整无限共享视图或整个WP2/WP3。未来source/asset改变需新同源记录，不回填当前190与历史120证据。
