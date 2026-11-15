# Q3 合同的公开 API 验收精化（候选 v2）

2026-10-31。补充而不改写 `q003-queue-definition.md`。依据新增 `shared-input-queue-contract.md`；具体摘要见本文件最后的冻结清单。本文保持定义状态，未写产品或测试源码，未运行用例。tester 不代签 reviewer。主合同最小 accessor/返回类型及错误次序已补齐，以下为该固定 API 的可编码验收定义，交 reviewer 复核。

## 精化决策与可验证边界

1. `InputQueue<T>` 为单目标、`&mut self` 状态机；应用负责每 NativeKey 复用实例。因此只执行两个操作先后顺序及同实例单归属，不声称公共注册表/多线程 barrier 已验证。两个相同 target 的新实例只用于 token 防串，不证明系统允许存在两个 owner。
2. `prepare` 保留原位置与等待位；`begin_send` 为等待→active 的最后交接。准备期可编辑/取消，旧 token 失效；begin 先处理后编辑/取消报告 DispatchStarted。
3. `Blocked` 是合成可信事实。模型接口没有 active/unknown/pending/资源不可用的独立枚举，不能将调用四遍 Blocked 算作四种真实来源覆盖；这里只验一次共同阻挡，具体事实来源仍在 WP3/L5。
4. 失败模式三挂起项的直发只在 Ready、无 active/准备/可执行等待项时原子建 active；不经第四等待项。直接 token 表示交接开始，非字节已写。
5. 恢复时已有新等待 D 先于旧 X/Y；活动 D 不抢占。挂起项 prioritize 返回 Suspended，需显式 resume；这些是现有 Q-002 新输入优先和显式继续的工程精化，未发现与负责规格冲突。
6. `settle(Advance/Suspend)` 是可信应用完成核验后的调度决定，不是原生成功/失败来源。begin 之后确定零发送仍通过 active 生命周期处理；不会自动回队/释放第四等待位。可能发送不能用模型自行 settle 模拟已核验；必须在阶段报告保留此调用者责任。

## 固定合成输入

- `id(n) = EntityId::from_bytes([n;16])`。源 S1=id(1)、S2=id(2)，原生 ID 为 `q3-synthetic-session`；K1=(S1,ID)，K2=(S2,同ID)，K3=(S1,`q3-other`)。
- E1={runtime: id(101), connection:id(102), load:id(103)}；E2 仅 load=id(104)，E3 仅 connection=id(105)。ready1=`Ready(E1)`、ready2=`Ready(E2)`、blocked=`Blocked`。
- A=id(10)、B=id(11)。X/Y/Z/D/E/W 分别 id(21)..id(26)，初始正文 `X/v1` 等，初始 revision=1。编辑 Y 为 `Y/v2`，editor=B。
- Rust 泛型正文主正控制使用自定义 `Payload { marker: String, structure: Vec<(String, Vec<u8>)> }`，不实现 Clone/Debug/Serialize/Deserialize。另以 String 测 32MiB+1 内容；不新增依赖。
- 所有等待快照比较 `(id,revision,submitter,last_editor,body,suspended)` 的有序向量；失败前后逐字段比较，不能靠错误返回 alone 判断没有副作用。token 可 Clone，但不用正文 Clone。
- 下表 `q` 初始 new(K1)，`[X*,Y*]` 表示挂起；`suspend(X,Y,...)` 夹具固定为先用 W 经 prepare→begin 建立 active，再将 X/Y/... 入等待，最后 `settle(Wtoken,Suspend)`，不写私有状态。`advance` 同理调用当前有效 token 的 settle(Advance)。

## 固定公开 API 断言

表中每行是 L3 操作步骤与预期结果，均复用现有 CL ID，仅为子阶段，不改变原主用例级别。下表早先使用的语义错误词 Capacity/旧CAS拒绝均由末尾固定错误表映射，禁止依据实现输出反选预期。

| 子断言 | 操作步骤 | 预期结果 |
| --- | --- | --- |
| CL-087.API01 | blocked 依序 submit X/Y/Z/E；独立分支交换最后 Z/E 顺序；另对 ready1 的正常空队列 submit X | 正常路径前三项 Queued rev1，末项 Capacity 且返还原输入；所有顺序不覆盖。正常 ready1 也先 Queued，不偷设直接 active。 |
| CL-087.API02 | 正常 submit X/Y，prepare ready1得t，submit Z，再submit E；检查等待及active | 准备X仍在等待原位并计位；前三满，E拒绝；active为空。 |
| CL-087.API03 | q1(K1)、q2(K2)、q3(K3)各入三项；在q1 prepare后尝试将t用于new(K1)的q4 | 不同目标各有独立三位；同键不同实例不能接受t，q1仍原状。只验token实例围栏。 |
| CL-088.API01 | X/Y/Z，edit(Y,1,B,newbody)，再以旧rev1分别 edit/cancel/prioritize(Y) | 成功返还Y旧body，Y rev2、位置及submitter=A不变、last_editor=B；三项不增；各旧CAS均拒绝且全状态不变，新edit body可取回。 |
| CL-088.API02 | cancel(X,1)，再cancel(X,1)，再submit E | 第一次返还X，等待为Y/Z；第二次NotFound且不变；E占释放位，等待Y/Z/E。 |
| CL-089.API01 | X/Y/Z，prepare→begin X；期间prepare/正常submit+调度，最后settle X Advance，再依次Y/Z | active存在时不能准备第二项；新等待只按余位；原X/Y/Z FIFO，各token只一次settle。入队不建立Run。 |
| CL-090.API01 | X/Y/Z，prioritize(Y,1)，prepare blocked，再prepare ready1→begin；settle后再prepare | Blocked零归属；Ready选择Y；等待其余X/Z原序，随后X再Z。 |
| CL-090.API02 | X/Y/Z，prepare X得tx，prioritize(Y,1)，用tx begin；重新prepare得ty；edit(Y,1,B,v2)，再prepare | 改优先使tx失效；Y编辑使ty失效但优先仍保留，最新prepare选择Y rev2。失败begin不移动项。 |
| CL-090.API03 | X/Y/Z，prioritize Y后prioritize Z；cancel Z；prepare ready1 | 最后优先为Z，取消后清选择；随后选择X（不会恢复前一个Y选择）。 |
| CL-090.API04 | 正常X/Y/Z满，submit D ready1；active旧工作存在时prioritize等待Y，再prepare ready1 | 正常D容量拒绝；优先不是中断完成或活动解除，不能prepare新active。纯模型不声称已发中断。 |
| CL-092.API01 | X的submitter=A，经正常prepare/begin/dispatch_input/settle全程不提供A当前actor/gate | 公开调用可执行且归属仍A；不证明真实撤销后的Node鉴权，API不得要求旧actor重验。 |
| CL-095.API01 | 非Clone/Debug结构Payload入队、edit、prepare/begin、dispatch_input、settle | 完整结构逐字段一致，编辑旧body及settle原输入可由调用者取回；无text降级/正文Clone要求。 |
| CL-095.API02 | 单String正文32MiB+1入队再大编辑，另加两项并检查第四项 | 大正文逐字一致、仍一位；第四按数量拒绝。真实分配失败单独记录，不称平台内容封顶。 |
| CL-096.API01 | 只通过new/submit/query等公开API处理顺序，检查所有签名没有客户端时间/统一截止 | X/Y/Z由处理顺序确定；领域无时钟输入，不执行伪造壁钟覆盖；节点校时留L5。 |
| CL-097.API01 | 入X，多次无状态查询/Blocked prepare后edit/cancel/重新入队 | 无TTL或credential输入，观察操作不会清理；不把循环次数当一天长等待证明。 |
| CL-098.API01 | new(K1)入X/Y、edit X，drop；再次new(K1) | 全新实例等待/active为空、failure_suspended=false；无持久化/丢失提示自动产物。本项不验证SQL/节点重启。 |
| CL-099.API01 | 分支A prepare X→cancel X→begin旧t；分支B prepare X→begin X→cancel/edit X | A取消成功，旧t拒绝，X零active；B操作均DispatchStarted，active正文不被改写/自动中断。 |
| CL-099.API02 | prepare X(rev1)→edit X为rev2→begin旧t；重做prepare→begin | 旧t拒绝，X rev2仍在原位；新token发送rev2。失败旧t不使新准备或active失效。 |
| CL-099.API03 | prepare ready1得t；begin(t,blocked)，begin(t,ready2)，再begin(t,ready1) | 前两次明确阻挡/epoch不符且保留等待+同一准备；最后成功原子转active，等待减一。 |
| CL-099.API04 | prepare ready1得t；release t；用旧t begin/release；重新prepare得t2 | release不移除/重排等待；旧t不可消费且不改状态；t2可用。重复prepare在已有准备且Ready时Busy。 |
| CL-099.API05 | begin(t)后clone t，重复begin；settle t Advance后，用旧clone dispatch_input/settle；再创建新active | 重复begin不能新取项；第一次settle返输入，之后旧token拒绝且不结束新active。跨目标/同键异实例token也不得访问当前body。 |
| CL-110.API01 | suspend(X,Y,Z)，submit D ready1；读等待/active，尝试submit E ready1；settle D Advance后submit E ready1 | D直接Dispatching、旧三位仍挂起，E在D active时Capacity；D成功后旧项不恢复，E可再直接Dispatching。 |
| CL-110.API02 | 三项挂起，edit X，prioritize X，prepare ready1，read snapshots | 编辑仍挂起；prioritize返回Suspended且不变；prepare None，无自动恢复。 |
| CL-111.API01 | suspend(X,Y)，submit D blocked，再submit E ready1；prepare ready1→begin→Advance | D为第三等待且非挂起；E因为已有可执行等待D且满额必须拒绝，不能直发越过D；prepare选择D，完成后X/Y仍挂起。 |
| CL-111.API02 | suspend(X)，D blocked入队，E blocked入队；prepare/begin D；settle D Suspend | 剩余X/E全部挂起，保相对顺序，E不自动派发，D不重新回队。 |
| CL-112.API01 | suspend(X,Y)，D ready1直接active；resume两次，prepare ready1；D Advance，再连续派发 | 两次resume不复制；D仍active，prepare受阻；之后X再Y，各一项；failure_suspended=false。 |
| CL-112.API02 | suspend(X,Y)，D blocked入第三位；resume；prepare/begin/Advance逐项 | 恢复等待精确为D/X/Y；D先发，旧X在Y前。无新受理ID/正文副本。 |
| CL-112.API03 | suspend(X,Y)，D blocked；prepare ready1选择D得t；resume；begin旧t，重做prepare | resume失效t但不复制；旧t拒绝且等待D/X/Y不变；新prepare仍选D。 |
| CL-113.API01 | 三挂起，D→E和E→D分别submit ready1 | 仅先者取得独立active，后者Capacity并返原输入；等待始终原三项，无隐藏第四项。 |
| CL-113.API02 | 三挂起，submit D blocked；另suspend(X,Y)，D blocked后prepare得t，再submit E ready1 | 前分支容量拒绝；后分支已有准备/可执行D，E满额拒绝，不能直发绕准备。 |
| CL-113.API03 | 三挂起直接D，后续条件Blocked或epoch改变；不调用settle，尝试E；核对active D及旧三位 | 准备之外的直接D已经active，不能自动退回等待/丢弃来腾位；E拒绝。可由有效token经已核验settle结束，但不是模型证明安全收敛。 |
| CL-115.API01 | suspend(X,Y)，B edit X、cancel Y；D直接active；resume；D Advance，再派X | X保原身份/submitter/位置及B编辑归属，取消释放一位；D前X仍挂起；继续不抢占，D后X只派一次。 |
| CL-115.API02 | 挂起X经resume后prepare得t；cancel(X,rev)→begin旧t，对照begin→cancel及旧rev操作 | 取消与发送按顺序只一方成功；旧rev拒绝无变化，无自动中断或隐式恢复其他项。 |
| CL-087.API04 | X等待时以同ID不同body submit；X active时再次同IDsubmit；正常满额时用已有IDsubmit | 均DuplicateId先于Full，Rejected.value保留新原输入；原body/顺序/active不变。已完成ID不在本容器历史拒绝义务内，不冒称持久去重。 |
| CL-088.API03 | X同文edit(X,1,B,原body)，对比缺失ID/wrongrev及active ID+wrongrev | 同文也推进到rev2并记录B；缺失NotFound、wrongrev RevisionChanged、active一律DispatchStarted优先；拒绝返新body且不改快照。初始editor固定为submitter。 |
| CL-099.API06 | active存在+blocked调用prepare；准备存在+blocked调用prepare；已失效t在blocked下begin；有效t blocked及ready2 begin | prepare前两分支Busy；begin依次StaleToken、Blocked、ResourceChanged；拒绝全状态及有效准备不变，可再release重做。 |
| CL-112.API04 | 正常队列prepare X得t，resume；另失败模式下D准备后resume | 正常resume返回0且t继续有效；失败退出/确实恢复返回真实恢复数并失效准备；两次恢复不复制；恢复后的第二次完整no-op不再失效新token。 |
| CL-099.API07 | 三挂起直接active D或普通begin后，不实际发送；在归属保留期尝试新输入；由owner明确settle(Advance)取回D | 没有自动回队/重受理；旧三位保持且D active时新输入Full；settle只取回原entry并释放active，旧挂起不恢复；旧token不能访问下一active。此是确定零发送后的显式模型交接，真实NotSent来源和元数据未验。 |
| CL-095.API03 | 使用无Debug正文Payload构造QueueInput/Rejected，格式化Debug；错误及token也格式化，body内置canary | 接口不要求T:Debug，Debug中不含canary/结构正文；QueueError无正文。生产安全仍须实际调用路径验证。 |

## 工程缺口状态（只表示测试输入已可定义）

- Q003-API-001/003/005/006/007/008：新合同已精化主要行为；单目标owner、prepare/begin、Ready抽象、Suspend全部剩余、D/X/Y顺序、挂起prioritize拒绝均可形成固定oracle。005的真实来源、006的真实零发送/未知收敛仍属后续，不宣称关闭全产品缺口。
- Q003-API-002/004/009：合同末尾签名、getter、错误次序、同文编辑推进、resume完整no-op和begin后不回队均已给出；可公开覆盖见API03/API06/API04/API07新增行。极值失败保旧准备由私有开发测试及独立静态检视覆盖，公开入口未动态触达。真实Run结果另记。
- Q003-API-010：极值只私有开发测试，独立公开API报告明确未动态触达u64::MAX，不通过生产setter补洞；独立reviewer读取开发极值测试与实现，不能由tester自评代签。
- 本文定义已核对末尾精确返回表；可交reviewer检查阶段编码门槛。阶段放行后再写独立Rust integration源码并对固定产品版本正式验收。产品代码尚未执行，表中全部状态为未运行。

## 固定 API 映射与错误表

验收导入 `acp_remote::domain::input_queue::*`，复用 `domain::EntityId`、`domain::native::{NativeSessionKey,ResourceEpoch,...}`。没有生产setter或私有字段访问。

- `QueueInput { id, submitter, body }` 构造已受理输入；`waiting()` 的 `ExactSizeIterator` 计数，条目用 `id()/input()/revision()/editor()/suspended()`取快照。`active()`可读活动条目；初始 editor 必须等于 submitter。`cancel/settle`返回entry，`into_input()`取回原正文；`edit`成功返回被替换T，失败通过 `Rejected { error,value }`保留新T。不得通过clone entry/body避开所有权。
- submit成功须匹配 `Submission::Queued{id,revision}` 或 `Dispatching(token)`。prepare成功 `Some(token)`与无项/Blocked的None分别断言，释放/发送/完成一律传 `&DispatchToken`；tokens只测试合法clone及跨实例使用，不自行伪造私有值。
- 固定错误：Capacity→`Full`；仍存在同ID→`DuplicateId`；缺失→`NotFound`；旧revision→`RevisionChanged`；活动ID的edit/cancel/prioritize→`DispatchStarted`；挂起优先→`Suspended`；active或prepared时prepare→`Busy`；不再有效或不同实例token→`StaleToken`；有效准备被当前Blocked阻挡→`Blocked`；Ready的ResourceEpoch不同→`ResourceChanged`。
- 错误顺序：submit重复ID优先；edit/cancel/prioritize先active ID，再NotFound，再RevisionChanged，再其他条件。prepare先Busy，再Blocked None，再选择；无可执行项None。begin先StaleToken，再Blocked，再ResourceChanged，失败均不移动/释放准备。
- `resume()->usize`精确恢复项数。`failure_suspended()`证明失败模式状态；Advance保持旧挂起及失败模式，resume退出。`MAX_WAITING_INPUTS==3`作常量正控制。`RevisionExhausted/SequenceExhausted`公开错误存在，但自然公开调用无法在合理资源内抵达MAX；只核对开发私有测试证据与reviewer结论，不写通过计数。
- 不要求Queue/InputQueue/QueueEntry/正文Clone或序列化；Payload无这些trait的正编译验证是本阶段库边界证据。未执行trait负编译之前，不宣称证明类型绝不实现Serialize/Clone。

## 冻结清单

- 主合同：`D:/Project/acp-remote/openspec/changes/deliver-codex-session-loop/shared-input-queue-contract.md`，SHA-256 `862ffc220d06ad71220be1da61e8f5c7853c285175e32a47457dbac753b081ec`。
- 基线语义定义：`q003-queue-definition.md`，SHA-256 `753ee4746e2ff578c28322ab907eb87fbd519e425e134e19800d47a737c59867`。
- 初始缺口记录：`q003-engineering-gaps.md`，SHA-256 `a3f451a3385bba73a3d679c625171d44739f983ef4c4458b2154cea5c63d24bb`；该文件保留首次状态，本补充给出已精化部分，不回填历史。
- 行为原文和角色输入快照：`inputs.json`，SHA-256 `683973e79a0312880b83aff464a7bb2e98f8f8bab38e59ae3a653ad782aff4aa`。
- 本文新增CL子断言不完成原L5/L6。报告要求逐行映射实际Rust测试名、运行状态及断言数量；允许一测试覆盖多个子行，但不得把Blocked重复调用充作实际原生四状态覆盖。
