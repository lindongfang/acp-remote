# 自动关闭运行时合同

本文件细化 RB-001 的本仓库实现，行为依据仍为本 change 的两份增量规格。用户已授权按职责边界继续实现且不修改上游；本文件不恢复 Harness 内部活动完整性或原子关闭窗口前置。当前合同包含 R1 后续的主树接线，2026-11-16。

## 实施边界

内部领域、协议投影和 schema13 系统操作已经获独立分包检视。以下接线按依赖顺序实施：队列与协议首字节保护、单主目标运行时、可信父子资源集合、三准入与扫描、Web 及完整验收。内部模块通过不等于生产自动回收已启用。完整 tracked tasks 与32个 CP 用例不缩减。

单主目标接线可以独立测试。接线前主树集成已获独立检视，正常launch接线候选已获编码门批准，用于验证节点真实启动行为；最终交付签收仍需父子树、全部加载入口和同版独立验收。不能把“没有子树资源集合实现”写成“没有已知子工作”而选中该树。

## 1. 当前事实与所有者

- 所有事实按原 source instance、runtime、connection 及原容量 reservation 隔离。`SourceSlot` 保留当前连接活动投影；`CapacityResource` 保留原系统操作、候选计时和原关闭 ticket。
- Create 返回身份前的观察不能随意归给某个在途创建。原 Created reply 的身份确认后建立 root 关联；先到达的当前 state 优先于 new 默认 idle。只有该协议版本明确保证 new 初态时才采用初态。
- SourceOnly 与 Create-only 使用同一活动投影，不要求平台 target 或执行 `ResourceSlot` 存在。陌生 session 不因有 update 就自动取得 root 身份。
- 当前 `state_update`、实际工具状态、标准直属父子关系和有效审批均更新投影；原 raw 继续保留。v1 不把同名扩展当 v2 标准关系。新工具缺省 pending、null 回到 pending、未知字符串仍进行中；缺失或格式异常不清除已知活动。完成/失败/取消才解除对应工具保护。
- replay 正文只刷新本次接收时刻，不恢复历史工具/审批状态。v2 resume 结束前的标准当前 state 与历史正文区分；current snapshot 不能仅因处在 resume 接收窗口而被全部丢弃。
- 已知 child 但当前 state 未确认属于保护；它与协议从未提供任何子范围不同。child state 省略不改变已存值；null 使前台未知，不清除已知后台工作。
- 队列及原执行 owner 是本平台输入事实的权威；活动投影不能代替 queue waiting/prepared、pending 必要操作、active、external 或 priority 状态。

## 2. 本平台输入与发送线性化

在 `queue_owner` 提供按 `NativeSessionKey` 获取的自动关闭保护接口，不伪造 target。SourceOnly/Create-only 也使用它；查找或新建空 owner 必须与 `RuntimeCore::owner_for` 的安装锁协调。

自动关闭首字节之前，按现存锁序 `OwnerState → TargetFacts → capacity activity → source.authority.gate → protocol state` 进入短临界区。回调期间不 await、不访问数据库。检查：

1. 原 owner/runtime/native 仍一致，树成员及原 reservation 未变，source 未关闭、未隔离、无必要管理缺口。
2. queue 无 waiting、active 或 preparation，owner 无 pending、active、external、priority handoff/selection；暂停排队项也保护。
3. 已知前台 idle，工具/后台/child/有效审批无保护；resource facts 无 pending input、控制、未协调 observation。
4. 对首个 `Chunk { offset: 0 }` 调用真正 IO proxy。`Pending` 不构成请求已发，下次 poll 重验。仅实际 `Accepted(n>0)` 在同一 owner 临界区内冻结该原系统操作的新输入。

冻结状态在 queue 安装 pending 之前检查，覆盖 Submit/Edit/Resume/Priority 等新操作；旧请求查询及关闭协调可以继续。不得让普通 `Blocked` readiness 将新输入受理为 waiting。返回既有固定冲突/资源不可用错误，不自动排队。冻结属于原系统 operation，解除必须核同 operation；旧 A 的清理不能解封新 B。

`offset>0` 与 flush 只检查原身份及必要生命周期，不因随后到达的忙碌事实撤销半帧。后续事实继续记录，但不中断保证以首个实际写入为界。partial/unknown 保留原 ticket、原操作和冻结，不能改记 Zero。

## 3. 已收到但未应用的观察

现有 observed transport 仅对 prompt 等待接收确认水位，普通 close 可越过已收到未处理的通知。因此自动 close 需要原连接上的专属写模式：

- 在 adapter 的真实 writer/proxy 协议锁内，仅自动 close 的 offset0 等待 `acknowledged == sequence` 且没有未投递的 held observations。
- ACK 后通过原 notify 唤醒，再走上述当前事实复核；不能只在外围查询水位。
- 显式安全 close 保持既有语义；自动模式不能由任意 DTO、自报标志或普通 Close 请求意外获得。
- 首字节接受后不再因接收水位等待而卡住半帧。关闭超时/取消继续使用原 writer 的 Zero/Unknown 分类。

这只约束本节点已收到的事实，与 Harness 未上报或首字节之后的内部活动保证无关。

## 4. 系统操作及关闭闭环

选择候选后，先固定原身份和 `begin_automatic_release`。这一步只登记尝试，不标记 Requested。数据库等待期间新活动仍会被首字节检查捕捉。

自动路径不调用显式释放的取消加载、取消 run 或取消控制前半段。仅复用原 `reserve_target_close/enqueue_target_close`、scope/rpc 校验、同 session Closed 响应和后半必要管理协调。共享实例不停止。

- 原 ticket Written 后才能记录 Requested。
- 原请求未入队或原 ticket 确认 Zero 且没有关闭依据，记录 Withdrawn、恢复该原操作冻结，不返额。
- 明确远端失败记录 Failed；无确认、partial、超时或提交不明记录 Unknown。两者保留责任及名额，不自动用新 operation/ticket 重发。
- 成功响应后保留原关闭事实，协调在途读取及必要 resource/approval/control 管理；所有条件满足后 `finish_automatic_release` 同事务返原额。
- 管理保存失败不撤销真实关闭事实、不返额；后续只重试同一必要记录。显式释放及 source shutdown 与系统 operation 对同一原责任串行，不双重返额。

父子读取只取得 root 原生加载资源，具体依据见 [主树身份与子目标读取合同](tree-read-runtime.md)。schema13 捕获该 root 的原 reservation 集合；内存系统操作另固定完整 native 成员集合、各成员的原队列 owner 以及节点执行/读取责任。两类集合不能互相冒充，也不能据 root Closed 宣称 Harness 内部 child 已销毁。

多成员队列按稳定原生身份排序、去重，以循环持有原 owner 锁后执行一次发送回调，不按后代数量递归。成员变化或队列修订变化须重新评估资格。Close 支持来自当前原连接实际协商能力，不以协议版本号代替。

## 5. 三准入、排序及调度

同一应用层 `admit_capacity` 包装原子 reserve：Create、执行加载、SourceOnly replay 均调用。先尝试 reserve；仅 `SessionCapacityFull` 选择一个当前候选，以最近有效活动、稳定原身份排序。关闭及必要管理成功后重试原 reserve 一次；并发者可先抢到空额，输家明确拒绝，不循环关闭其他树、不等待自动排队。

压力选择保存原排序键 `(candidate_since, source_id, native_id, load_epoch)`。每次 offset=0 写入前，先在原目标锁之外重新扫描其他候选，记录扫描开始时刻及其他候选的最小完整排序键，再在原目标临界区核对当前资格。当前资格起点不能早于原选择，且必须严格早于这次扫描起点、排序不晚于其他最早候选；同值时按完整身份键比较。检查中遇到不合格或无法确认的候选清除旧资格起点，恢复后从当前单调时刻起算，不能让后来恢复的候选沿用旧起点绕过排序检查。

SQL 等待期间原目标收到真实重放且仍为唯一候选时，可以按其更新后的起点发起请求；不能把“起点变化”直接等同永久豁免。若另一候选现已更早，原操作在首字节前撤回，保留原名额，本次压力请求不改选第二棵树。这是节点已知事实的发送前检查，不声明上游原子空闲窗口。

候选检查不持有申请者的 install/capacity close 锁跨树调用，以避免A申请锁住A并回收B、B申请锁住B并回收A。候选选择及自动操作串行锁只保护自动选择流程；不把 SQLite 的最终名额权威转移到内存。

runtime 进入 Running 后启动唯一30秒扫描任务，使用 `TaskRegistry`，检查 stop 并在 shutdown join。启动去重标记与任务实际登记凭据分开；管理状态和容量压力准入只依据成功登记后的原TaskSlot，登记失败不能发布已启用状态，启动调用方须fail_all。时间采用单调时钟；只收当前活动及真实 replay 才刷新；资格从不可用变为可用时重新开始30分钟。单次扫描失败保留原责任并继续响应 stop，不能产生 detached cleanup。

压力路径和扫描调用同一候选与请求方法。重启后旧时钟与连接活动不复用；必要账本保额，重新建立当前请求依据。

## 6. 父子资源与读取责任

标准 `subagent_update` 仅证明当前立即父会话创建并拥有 child；不从 tool 文本引用、fork、用户输入或模糊 ID 推导。需要持久保存来源+原生父子关系及可追溯 observation，并拒绝循环、跨源、双父及与已验证 root 冲突。重连后持久关系可用于解析身份，但不能证明当前活动。

直接子目标加载须先恢复可管理的 root，并将 child 读取/在途操作归同一 reservation。原生自动投递 child events 不额外执行 child resume；如历史读取确需协议请求，必须遵循该版本只读能力及 child 操作限制。根不可恢复、被删除或关联不可靠时按规格拒绝，不猜根。

根 close 的响应不能无条件替代独立 child 资源的物理关闭确认。本实现不为自动投递的 child 单独取得原生加载资源；节点仍须登记并等待其真实读取任务结束。共享来源的 worker 不随某一棵树关闭；该树的 Reader、NativeRead、Cleanup 则必须保留原视图代际与 root authority，并等待实际 JoinHandle。封闭视图或设置 finished 标志不能代替 join；任务异常保留原名额与收敛失败。

## 7. 可执行验证

保留32个 CP 用例。新增开发检查至少覆盖：首poll Pending 后到达忙碌观察；已收到但未ACK的 busy；排队/待管理请求与无owner安装竞争；首字节之后新活动继续保留原请求；半帧故障 Unknown；Zero解封及旧操作不解封新操作；三个满额入口；扫描 stop/join；Create-only与SourceOnly保护。

父子检查覆盖真实关系接收、重复/冲突/重连、child unknown/active保护、直接加载归root、最后成员未收敛保额。协议受控peer仅证明本仓库合同；真实Codex 50树与实际30分钟仍单独验收。

2026-11-16独立真实小样本：隔离身份下 initialize 成功，list/new 返回 AuthRequired(-32000)，未获得可关闭会话；因此真实 close/50/30/child尚未执行。探针零退出及Job清理成功不能计作上述正向能力通过。专属认证是实际验收环境前置，不是修改上游的理由。
