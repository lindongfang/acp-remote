# 共享输入队列：纯领域阶段 Q3

2026-10-31。依据当前 session-execution、acp-platform-constraints 及 Q-002；本阶段只定义并实现内存状态机，不把它作为认证、登记持久性、协议终态或真实发送的证明。用户本次已授权基于新 spec 继续实现，取代前次文档阶段的暂停范围。

## 范围与所有者

新增 `domain::input_queue`；每个 `NativeSessionKey` 由应用拥有一个 `InputQueue<T>`，泛型 T 保存已验证的完整结构内容，不转换成文本、不 Clone、不设置字节预算。队列不派生 Serialize/Deserialize/Clone，不访问文件、SQL、时钟、网络或 actor gate；`&mut self` 短操作确定节点处理顺序。应用负责按原生目标复用同一个实例，并在新操作/读取前验当前权限。

`QueueInput<T>` 包含稳定 EntityId、提交 actor EntityId 和内容；项 revision 从 1 开始、初始 editor 为 submitter，编辑保 ID/位置/原提交者并记录最后编辑者，checked_add 失败原样拒绝。重复在本队列仍存在的 ID 拒绝；已移除/完成旧 ID 的永久防重属于后续节点登记阶段，不能通过此容器绕过。正文和错误不自动 Debug 输出，查询仅借用。

等待队列最多 3 项（包括挂起和已准备但未发送的项）；最多一个独立活动项。等待项不分配 Run、不依赖凭证期限。重新构造队列为空，不恢复正文；本阶段不声称证明 SQLite 未落正文或重启旧 ID 拒绝。

## 公开 API 与准备/发送边界

所有修改同步完成。`DispatchReadiness::{Blocked, Ready(ResourceEpoch)}` 是受信应用观察事实，不是客户端可提交的 grant；Ready 必须代表无活动/未知前台、无待插入、资源有效及必要状态可靠。真实接线不得把纯模型构造器当作证据。

- `new(target)`；`target()`、`waiting()` 借用迭代器、`active()` 借用活动项、`failure_suspended()`。
- `submit(input, readiness)` → `Result<Submission, Rejected<QueueInput<T>>>`。通常只入等待队列；正常满额即使 Ready 也拒绝。失败模式中，如 Ready、没有活动项/准备选择、没有其他可执行等待项，则原子直接取得唯一活动归属，不占等待位；三挂起项时允许此路径。其他情况仅在三项以内入队，新项保持可执行。拒绝返回原输入供保留草稿。
- `edit(id, expected_revision, editor, body)`：等待项的版本 CAS，返回被替换内容；拒绝返回新 body。编辑不恢复挂起；使该项旧准备选择失效，保留优先选择。
- `cancel(id, expected_revision)`：移除等待项并返回内容，释放名额及该项准备/优先选择。活动项返回 DispatchStarted，不自动中断。
- `prioritize(id, expected_revision)`：仅标记已有可执行项为下一优先项，不移动其他项、不隐式继续挂起集合；挂起项返回 Suspended，须按规格显式继续。中断必须由应用发起并确认，Blocked 时不派发。最后一次节点处理的优先选择生效；取消该项清除，编辑保留选择。更改优先选择使先前准备 token 失效，不能沿旧选择发送。
- `resume()`：现有非挂起等待项仍在前，挂起项按原相对顺序重新加入其后，清除 suspended，结束失败模式；不复制、不抢占、不清活动项，不重新授权旧提交者。这样已经受理等待的新 D 不被恢复的旧 X/Y 抢先。重试是状态幂等，管理请求去重另验。恢复会失效已有准备，必须在新顺序重做 prepare。
- `prepare(readiness)` → `Result<Option<DispatchToken>, QueueError>`：仅 Ready 且无活动/已有准备时，选择优先项或首个非挂起项，保留原等待位置和名额；生成不透明 token，绑定队列实例、checked 本地序号、项 ID/revision 和 ResourceEpoch。Blocked 返回 None；无可执行项返回 None；重复准备返回 Busy。
- `release_preparation(token)`：零发送解除准备、不移除项；同队列当前 token 才有效。
- `begin_send(token, readiness)`：再次核对准备、项 revision、资源代际和 Ready，原子从等待队列移到独立活动项；Blocked/代际改变不移动并保留准备，调用者可以释放重做。旧 token、跨队列 token 均拒绝。编辑/取消先执行使旧 token 失效，零发送；begin_send 先执行则编辑/取消返回 DispatchStarted。

`Submission::{Queued{id,revision}, Dispatching(DispatchToken)}` 的直接派发与 begin_send 都只是节点发送交接开始，不声称已交字节、已插入或完成。新直发只能在可信 owner 已核对可即时开始发送时调用；没有 writer 就不能把 Ready 当作藏第四等待项的手段。调用者开始真实传输前必须处于同一短协调边界完成最后检查，不得在中间 await 后沿旧资格发送。需要等待的普通选择必须走 prepare，并在最后边界 begin_send。活动内容保留在队列，由有效 token 借用 `dispatch_input(token)`，不复制正文；token 可 Clone，但同一 active 只有一次 settle，旧 token 不得访问新活动。

## 完成、失败与未知

`settle(token, disposition)` 返回原活动输入并释放活动归属。`QueueDisposition::{Advance, Suspend}` 是可信应用已完成协议/资源核验后的调度决定，不是原生结果或 Run 成功字段；Advance 不恢复既有挂起项，Suspend 将此刻所有剩余等待项挂起、保留相对顺序及内容，并进入失败模式。可靠失败必须 Suspend；真正中断/确定零发送如何记 Run 由应用单独处理。旧工作仍可能活动/插入未确认时不能调用 settle 放行，应保留活动项；资源收敛后的未知结果仍须单独保存，不能借 Advance 编造成成功。

失败模式的新 D 不能立即派发时按三项总名额入队为非挂起；条件满足后跳过旧挂起项，D 先发。D 成功使用 Advance，旧项仍挂起；D 失败 Suspend，此刻剩余项都挂起且清除旧优先选择。D 期间 resume 只恢复等待项，D 不被抢占。挂起项不会因编辑、prioritize、新项成功或资源恢复而自动恢复。

token 绑定私有 Arc 实例身份及单调 checked u64 序号；最大序号后新准备/直发拒绝且状态不变，不回绕、不降低、不复活旧 token。项 revision 溢出同理。测试可通过私有 cfg(test) 模块直接构造极值；无生产 setter。纯模型不储存已完成 ID 历史，不收集客户端时间或校时信息。

## 验证与后续

独立 tester 按 tests.md 的 CL-087–092/095–099/110–113/115 定义 L3 子断言；真实认证、SQLite登记、协议互斥/终态、WSS/Web和完整进程树仍需 L4–L6，不能因模型通过勾完整 2.2/2.4/6.2/6.7。开发测试覆盖版本/序号极值，独立公开 API 测试覆盖三名额、两种先后、优先、失败新项、resume和实例隔离。旧视图预算及签名实现保持原历史范围，本阶段不把它们接回新队列。

## 最小可执行 Rust 接口（编码前精化）

```rust
pub const MAX_WAITING_INPUTS: usize = 3;
pub struct QueueInput<T> { pub id: EntityId, pub submitter: EntityId, pub body: T }
pub struct Rejected<T> { pub error: QueueError, pub value: T }
pub struct QueueEntry<T> { /* private */ }
// entry: id()->EntityId, input()->&QueueInput<T>, revision()->u64,
//        editor()->EntityId, suspended()->bool, into_input(self)->QueueInput<T>
pub enum DispatchReadiness { Blocked, Ready(ResourceEpoch) } // Copy/Eq/Debug
pub struct DispatchToken { /* private instance/serial/item/revision/epoch; Clone, redacted Debug */ }
pub enum Submission { Queued { id: EntityId, revision: u64 }, Dispatching(DispatchToken) }
pub enum QueueDisposition { Advance, Suspend } // Copy/Eq/Debug
pub enum QueueError { Full, DuplicateId, NotFound, RevisionChanged, RevisionExhausted,
    DispatchStarted, Suspended, Busy, StaleToken, Blocked, ResourceChanged, SequenceExhausted }
// QueueError: Copy/Eq/Debug/Error; QueueInput/Rejected Debug不包含正文/泛型值。
// 所有以下签名属于 impl<T> InputQueue<T>；所有getter不Clone正文。
pub fn new(target: NativeSessionKey) -> Self;
pub fn target(&self) -> &NativeSessionKey;
pub fn waiting(&self) -> impl ExactSizeIterator<Item = &QueueEntry<T>>;
pub fn active(&self) -> Option<&QueueEntry<T>>;
pub fn failure_suspended(&self) -> bool;
pub fn submit(&mut self, input: QueueInput<T>, readiness: DispatchReadiness)
    -> Result<Submission, Rejected<QueueInput<T>>>;
pub fn edit(&mut self, id: EntityId, revision: u64, editor: EntityId, body: T)
    -> Result<T, Rejected<T>>;
pub fn cancel(&mut self, id: EntityId, revision: u64) -> Result<QueueEntry<T>, QueueError>;
pub fn prioritize(&mut self, id: EntityId, revision: u64) -> Result<(), QueueError>;
pub fn resume(&mut self) -> usize;
pub fn prepare(&mut self, readiness: DispatchReadiness) -> Result<Option<DispatchToken>, QueueError>;
pub fn release_preparation(&mut self, token: &DispatchToken) -> Result<(), QueueError>;
pub fn begin_send(&mut self, token: &DispatchToken, readiness: DispatchReadiness) -> Result<(), QueueError>;
pub fn dispatch_input(&self, token: &DispatchToken) -> Result<&QueueEntry<T>, QueueError>;
pub fn settle(&mut self, token: &DispatchToken, disposition: QueueDisposition)
    -> Result<QueueEntry<T>, QueueError>;
```

错误先后：submit先检查仍存在ID，之后判断直发资格/序号或容量；edit/cancel/prioritize先检查是否活动ID（DispatchStarted），再NotFound、revision不匹配、其他前提。edit即使内容相同也作为新版本推进（容器不比较泛型内容）；重复管理请求须由登记层短路，不能再调用edit。revision溢出不替换正文、不失效旧准备。prioritize在版本通过后拒绝挂起项。

prepare先检查活动/已准备并返回Busy（含Blocked），再Blocked返回None，再选可执行项；无项返回None而不耗序号。begin_send先验证本实例当前准备token及项版本，再Blocked、Ready但资源不同；分别返回StaleToken、Blocked、ResourceChanged，全部不改等待/准备。可release后按新Ready重做。dispatch_input/settle只接受本实例当前活动token；旧clone在settle后全部失效。

resume返回实际恢复项数；没有失败模式也无挂起项时是完整no-op，保留已有准备token。失败模式退出或确实恢复项时失效准备；使用最多三项的稳定分组把非挂起项放前，不改变两组各自相对顺序。活动项不变。没有正文/节点时间入参或隐藏期限。

直发或begin_send之后若最后传输条件失效，仍保留唯一活动归属而不藏入第四等待位；确定零发送可由owner明确settle并另记NotSent，返回原QueueEntry供owner处理，绝不自动重新受理或回队。可能已发送则维持归属直到可靠协调。`settle`不是自动重试许可，且不能把模型Advance误记为原生成功。未来owner必须实测这个边界，本阶段只验证状态机。Blocked抽象统一覆盖active/unknown/待插入/无效资源，不能说四种真实协议条件已分别动态验证。
