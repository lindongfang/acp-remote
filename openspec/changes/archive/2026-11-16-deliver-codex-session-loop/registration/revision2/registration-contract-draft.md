# Q3 下一 WP2：节点请求登记与必要去重账本草案

2026-10-31。revision2工程方案候选，未独立预审通过、未编码、未执行迁移；只精化v1的仓储注入和完整核验事实及必要测试入口。基线 HEAD 为 `65ec7815bb0280f4519989d93a7b4ed3b4b37994`，工作树含既有未提交实现；原输入摘要见 `../inputs.sha256.json`，v1字节不覆盖。本文件只写指定外部目录。

## 结论与阶段范围

建议下一个独立 gate 同时审查并实施**登记、真实SQLite、队列owner受理及故障围栏**，不要长期只交CAS模型。本文件精化登记账本和迁移；配套 `queue-owner-application-draft.md` 冻结无锁跨SQL的实际应用步骤。R1/R2/R3是同一阶段内部依赖顺序，不是无限延后的新授权轮次。新建 `domain::request_registry`、`application::request_registry` 及 SQLite adapter，保留旧 requests/product_commands/JWS 路径原状，不把它们接回新队列。新布局候选编号为 schema5；只有方案/用例独立检视后才能采用该编号和实现，不声称当前已有 schema5。

登记与消费属于认证授权入口；消费严格要求节点先前登记的 ID 存在。请求 ID 全节点唯一、不透明且只由节点随机产生，客户端不能通过“注册自己给的 ID”复活旧动作。相同 ID/意图只读原事实；不同意图冲突。正文只经过内存验证和摘要计算，不进入 SQL 参数、错误或日志。

本阶段交付目标是实际SQLite账本、唯一owner注册表和真实InputQueue受理/编辑/取消/恢复的完整应用路径；独立证据按L4子范围记录。原生writer、审批保存、真实认证通道及完整L5/L6未由本阶段证明。当前三份草案本身未编码或执行，纯队列历史验收也不证明本阶段。

## 依据与实际差异

行为权威：当前八份 `specs/*/spec.md`，尤其 session-execution 的输入受理、防重、节点接管、无期限/重启、管理故障、节点时间；management-audit 的必要状态/附加审计分离；permission-approvals 的批准先保存。工程边界：design、implementation-contract、management-command-contract、shared-input-queue-contract。独立验收定义：tests.md 的 CL-004/017/018/075–084/088/092/094/096–099/104/112/115。plan 的 Q3 段明确下一步另冻结登记/SQLite，再走 WP3/WP4。

实查结果（路径均相对仓库）：

| 当前实现 | 对下一阶段的影响 |
| --- | --- |
| `domain/requests.rs` 的 ProductRequestIdentity/VerifiedProductRequest/RequestClock | 有 epoch、issued/expires、24h 和持久水位语义，不能冒充新登记证明。保留历史接口，不复用它们作为新请求资格。 |
| `domain/product_commands.rs` 的 ProductCommandFact | run 必填，不能表达尚无 Run 的等待项；ProductAction 无编辑/优先动作。新领域类型独立定义。 |
| `application/product_commands.rs` 的 gates/issue/claim/final_fence | 持 actor/owner gate、签名/期限/原 actor 再验以及旧正文预算仍在；新服务不调用这些入口。 |
| `session_storage/product_commands.rs` 的 admit/claim/checkpoint/terminal | require_actor 和 time_floor、audit slot 与必要事务耦合；不能只改几个参数就声称符合新规格。 |
| `session_storage/product_schema.rs` | 旧 product_commands 强制 expires_at；product_runs 的 accepted 也占唯一前台槽。新登记不插入这两表，不改该索引，不据此拒绝第二第三等待项。 |
| `storage.rs` | SCHEMA_VERSION=4，open 先只读 preflight，后 WAL/FULL/foreign_keys，单连接池；schema.rs 用定义比对和 BEGIN IMMEDIATE 迁移。继承这些启动保护。 |
| `access_policy.rs`、`management_ws.rs` | 已有不可由外部直接构造的 AuthenticatedActor、当前可信代际核验、撤销 gate；WSS 当前只有 node.info/device.list/device.revoke，尚无真实产品动作入口。不能用伪 ActorContext/true 代签接线。 |

本候选只覆盖共享输入相关的五种动作。Harness/目录登记、会话创建/释放及审批决定仍需各自行为与事务合同；不要把通用登记接口自动解释为这些动作已经可用。

## 身份、意图和授权事实

1. `RegistrationId(EntityId)` 是 128 位随机 ID，沿用现有 `crate::random::bytes()` 和 EntityId 的规范 base64url 编解码。随机失败不登记；唯一键碰撞重新取随机，不覆盖旧行。不新增依赖、时间戳或计数上限。
2. `QueueAction` 是封闭枚举：Submit、Edit、Cancel、Prioritize、Resume，存储标签分别为 `input.submit/edit/cancel/prioritize/resume`。不重用语义含混的旧 session.cancel/session.resume。
3. `RegisteredIntent` 固定 schema 版本 1、actor、动作、稳定 target ID、可选 item ID、可选 expected revision、可选 body digest。Submit 无 item/revision，有 body digest；Edit 三者均有；Cancel/Prioritize 有 item/revision，无 body digest；Resume 三者均无。Submit 的新队列项 ID 直接使用本次 RegistrationId 内的 EntityId，不另设可冲突的客户端 item ID。
4. 目标 ID 在登记/消费时解析并校验为既有 product_targets 的 `(source_id,native_id)`。目标引用只表示稳定原生会话，不绑定当前连接/load 代际，不以资源暂忙拒绝登记，也不授予发送资格。派发时实际资源另核验。既有原生 ID 结构规则不是正文预算，本阶段不改变它。
5. body digest 来自受信 `ValidatedBodyPort`，对它实际验证并冻结的完整结构内容返回 `[u8;32]` 绑定；客户端自报摘要只可作对照，不是证据。本阶段不新增JSON canonical格式，不宣称serde_json序列化能无损规范化浮点/指数或任意原始协议结构。登记与消费使用同一受信端口结果，并逐字节比较digest；真实网络DTO/等价表达向量在WP4明确，不能由此草案宣称完整codec已实现。独立owner测试使用显式标注的合成已验证body和可控digest端口，不冒充真实协议验证。
6. 注册服务接收的意图不含客户端 actor；actor 从可信认证句柄解析。正文摘要作为必要内容绑定可持久，正文、正文长度快照、文件路径、审批提示、凭据均不进入新账本。
7. 注册成功仅表示 ID/意图已登记，不代表入队。首次消费必须重新确认本次认证 actor、目标和权限；登记时曾有权不能替代受理时有权。首次消费的 SQL 事务内复核 clients 的 trusted/auth_generation，与撤销 COMMIT 建立明确先后；当前首期所有可信客户端管理本节点，未来细粒度权限必须扩展 authority，而非用布尔 grant。
8. 首次消费 COMMIT 后节点拥有该操作。稳定事实保留登记 actor、登记代际和消费时授权代际；后续处理和 finalize 不再查该 actor、其 gate、在线状态或 auth_generation。新操作/读取仍校验当前读取者/操作者。B 编辑 A 的项是 B 的新登记；B 不能把 A 的旧登记 ID 当作自己的新操作。
9. 同 actor 新认证代际重试旧 ID：当前资格仍有效且固定意图一致时查询原事实；不能因原 auth_generation 不同把已受理动作当新动作。撤销 actor 的新重试/读取仍被认证入口拒绝；其他当前有权 actor 可通过目标查询看到原事实，但不能消费为其自己的动作。

## 最小 Rust 内层与应用端口

下列是候选接口，不是已经编译的实现。domain 不依赖 SQL、WSS、AuthenticatedActor、时钟或 InputQueue<T>；application 声明端口，SQLite adapter 向内实现。所有字段构造通过验证构造器；包含正文或摘要的 Debug 均脱敏。领域 revision 是完整 u64，SQL 使用 8 字节大端 BLOB，不能静默缩成 i64 或 JS safe integer。

```rust
// domain::request_registry
pub struct RegistrationId(EntityId); // Copy/Eq/Hash，规范编解码
pub enum QueueAction { Submit, Edit, Cancel, Prioritize, Resume }
pub struct RegisteredIntent {
    actor: EntityId, action: QueueAction, target: EntityId,
    item: Option<EntityId>, expected_revision: Option<u64>,
    body_digest: Option<[u8; 32]>, // 构造器验证上文形状
}
pub struct RegistrationFact { /* id, intent, register_auth_generation, registered_at */ }
pub struct StoredApplying { pub registration: RegistrationFact,
    pub operation: EntityId, pub runtime: RuntimeEpoch,
    pub admission_auth_generation: i64, pub begun_at_ms: i64 }
pub enum RequestFact {
    Registered(RegistrationFact),
    Applying(StoredApplying),
    Settled { registration: RegistrationFact, outcome: QueueOutcome },
    Interrupted { registration: RegistrationFact },
    Retired,
}
pub enum QueueOutcome {
    Queued { item: EntityId, revision: u64 },
    DirectSelected { item: EntityId, revision: u64 },
    Edited { item: EntityId, revision: u64 },
    Cancelled { item: EntityId, revision: u64 },
    Prioritized { item: EntityId, revision: u64 },
    Resumed { restored: u8 },
    Rejected(QueueRejection),
}
pub enum QueueRejection { Full, DuplicateId, NotFound, RevisionChanged,
    RevisionExhausted, DispatchStarted, Suspended, Busy, SequenceExhausted }
pub enum RegistryError { InvalidIntent, InvalidActor, InvalidTarget,
    UnknownRegistration, Conflict, Retired, RandomUnavailable,
    StorageUnavailable, CommitUnconfirmed, CorruptState, Cancelled,
    NoOwnedOperation, StaleObservation, NodeStopping, NodeStarting, RuntimeAlreadyStarted }
```

`DirectSelected` 只记录 pure queue 的原子独立活动选择，不代表已发送、已插入或 Run 成功。`Settled` 是**本次管理操作结果已稳定**，不是原生工作终态。原受理输入后续状态与该操作回执分开，编辑不改写原 Submit 意图；其历史回执可仍为 Queued，而实时队列中已无该项。

```rust
// application::request_registry，可信装配端口，不是 RPC DTO
pub struct CurrentRequestActor { /* private ActorContext/target/native/Read或Act(action)，应用mint */ }
pub struct ValidatedBody<T> { /* private T/digest，应用从可信port结果mint */ }
pub trait ValidatedBodyPort: Send + Sync {
    type Raw: ?Sized;
    type Body: Send;
    fn validate(&self, raw: &Self::Raw)
        -> Result<(Self::Body, [u8;32]), RegistryError>;
}
pub struct ConsumeLease { /* private registration/operation/runtime/intent */ }
pub struct OperationProbe { /* private登记ID/operation/runtime/完整意图，节点持有 */ }
// lease 不 Clone/Deserialize，非 writer/dispatch permit，Debug 脱敏
pub enum BeginRecord { Started(StoredApplying), Existing(RequestFact) }
pub struct OperationContext {
    pub runtime: RuntimeEpoch, // boot 围栏，绝不是输入期限/request_epoch
    pub observed_at_ms: i64, // 节点系统UNIX毫秒，可负，仅记录
}
pub trait NodeClock: Send + Sync { fn unix_millis(&self) -> i64; }
pub struct RegistryDeadline { /* private monotonic Instant/cancel */ }
// Clone只共享本次工程取消状态，不复制任何授权。
impl RegistryDeadline {
    pub fn new(deadline: tokio::time::Instant) -> Self;
    pub fn cancel(&self);
    pub fn deadline(&self) -> tokio::time::Instant;
    pub fn is_cancelled(&self) -> bool;
}
pub trait RequestAuthority: Send + Sync {
    type Actor: Send + Sync; // 最终由 AuthenticatedActor 适配
    fn authorize(&self, actor: &Self::Actor, target: EntityId, action: QueueAction)
        -> impl Future<Output=Result<ActorContext, RegistryError>> + Send;
    fn authorize_read(&self, actor: &Self::Actor, target: EntityId)
        -> impl Future<Output=Result<ActorContext, RegistryError>> + Send;
    fn resolve_target(&self, target: EntityId)
        -> impl Future<Output=Result<NativeSessionKey, RegistryError>> + Send;
}
pub trait RequestRegistryRepository: Clone + Send + Sync {
    fn register(&self, actor: &CurrentRequestActor, intent: &RegisteredIntent,
        id: RegistrationId, at: &OperationContext, deadline: &RegistryDeadline)
        -> impl Future<Output=Result<RegistrationFact, RegistryError>> + Send;
    fn begin_consume(&self, actor: &CurrentRequestActor, id: RegistrationId,
        intent: &RegisteredIntent, operation: EntityId, at: &OperationContext,
        deadline: &RegistryDeadline)
        -> impl Future<Output=Result<BeginRecord, RegistryError>> + Send;
    fn settle(&self, lease: &ConsumeLease, outcome: &QueueOutcome,
        at: &OperationContext, deadline: &RegistryDeadline)
        -> impl Future<Output=Result<RequestFact, RegistryError>> + Send;
    fn read(&self, reader: &CurrentRequestActor, target: EntityId,
        id: RegistrationId, deadline: &RegistryDeadline)
        -> impl Future<Output=Result<RequestFact, RegistryError>> + Send;
    fn inspect_operation(&self, probe: &OperationProbe, deadline: &RegistryDeadline)
        -> impl Future<Output=Result<RequestFact, RegistryError>> + Send;
    fn interrupt_previous_runtime(&self, current: RuntimeEpoch,
        at: &OperationContext, deadline: &RegistryDeadline)
        -> impl Future<Output=Result<u64, RegistryError>> + Send;
}
```

repository中的register是内部trusted adapter，不能直接公开为“给定id注册”的网络功能；只有应用服务生成随机id。唯一真实应用入口以配套QueueService的register/consume/query_request为准，不另公开能绕过实际owner的begin服务。RequestAuthority返回已验证ActorContext，应用把它和resolve_target得到的native key、本次target/Read或Act(action)封装成CurrentRequestActor；外部实现无需构造私有wrapper，也没有公开bool工厂。ValidatedBody同理由应用从受信port结果构造。settle/startup是节点内部owner路径，没有原actor入参。repository检查CurrentRequestActor作用域及target/native绑定，不得拿只读或其他目标句柄consume；register/begin/read事务复核**本次**actor当前可信代际，后续settle只看lease/operation/runtime/CAS。

OperationProbe 在begin前由应用保存于pending，不能从客户端反序列化。inspect_operation是内部核验，不是当前读取接口或dispatch许可；逐字段比对原登记意图/operation/runtime，不能拿一个probe查其他事实。它不重验已消费的原actor，解决actor撤销后COMMIT未知仍需由节点核验的问题。查询结果本身不自动给出第二执行权；如何在同进程未应用状态恢复一次lease由配套owner状态机决定，跨进程绝不恢复应用。

revision2明确：inspect_operation返回的Applying现在携带**完整StoredApplying**，与begin成功的Started使用同一领域结构，包含实际SQL读出的runtime/admission_generation/begun_at及完整RegistrationFact。不能从不完整RequestFact补默认值或伪造时刻。BeginUnconfirmed本地pending保留原operation/runtime/intent、本次actor代际和传给事务的节点时刻；恢复时逐一匹配这些已知坐标，验证其他登记元数据来自本次真实SQL行且形状正确，再消费未D状态mint一次lease。已D的AwaitSettle只能据同一完整事实补settle；RequestFact::Applying本身不授恢复权。

可实现性：repository只返回可构造的StoredApplying事实，不mint ConsumeLease；application在同owner AwaitBegin或BeginUnconfirmed且未apply时核对原operation/runtime/intent并消费该pending状态，唯一私有函数mint一次lease。RegistrationFact/StoredApplying是元数据事实，可有验证形状的公开构造器，并非调用方授权。CurrentRequestActor只读getter为context()/target()/native()/action()（Read返回None）；ConsumeLease/OperationProbe只读getter为registration_id()/operation()/runtime()/intent()，适配器可取SQL绑定但不能外部制造租约。RegisteredIntent提供actor()/action()/target()/item()/expected_revision()/body_digest()；RegistrationFact提供id()/intent()/register_auth_generation()/registered_at_ms()。全都不提供可改字段、解除fault或重新apply的setter。

本阶段不得给应用服务提供“直接执行 QueueInput”的通用回调而冒称真实 owner。独立测试使用真实InputQueue、实际owner注册表和真实SQLite；authority/原生观察是明确标注的受信端口替身，不能签L5。不以假仓储或原39纯队列通过代签本阶段。

## 登记与消费的事务语义

### 登记

BEGIN IMMEDIATE 内核对 actor 当前可信代际、目标匹配及意图形状，INSERT 新 registry 行，COMMIT 确认后返回 ID。输入结构验证/摘要在事务前内存完成；事务不接正文。不能把唯一冲突 UPDATE 成新意图；ID 碰撞只由应用重新生成随机。lost registration response 没有受理副作用，客户端可请求新登记；应用不得从内容相同推断应消费某个未知 ID。

### 首次消费与重试

BEGIN IMMEDIATE → 当前actor/目标核验 → SELECT registry必须存在 → 完整逐字段比对意图 → retired拒绝 → 查operation → 无operation才INSERT applying → COMMIT。actor不符或目标无权不泄漏旧事实；已存在operation返回Existing，不再次返回Started。读不到registry返回UnknownRegistration，**没有INSERT/UPSERT fallback**。登记行存在但没有operation与“不存在登记行”是两种明确事实。

租约包含新随机operation ID、节点runtime、登记ID和固定意图，不是TTL。相同请求并发只有一个applying INSERT成功；另一个读原事实。SQL commit未知/超时/取消不得直接给Started或lease/成功；丢弃脱离池的连接并阻止相关新动作，用原OperationProbe读持久事实核验，不重新INSERT或重跑队列。同进程明确尚未apply且核验到自己原applying时可恢复一次应用权；不是第二次consume或跨重启重跑。延续现有detached transaction的所有者/取消经验，不复制audit/time_floor耦合或固定产品超时。

### 稳定回执

settle 只允许同 request/operation/runtime 的 applying 行转换为 settled；检查 outcome 的动作、项 ID、revision/数量形状。相同已 settled 回执幂等返回，不同回执 Conflict。不得改写原 registry actor/意图；不再 require_actor。记录 Rejected 也不是原生失败或 Unknown。

必须用行数与精确身份 CAS，不能 UPDATE 未命中仍报告成功；未知提交后 read 只返回原事实，不产生执行 lease。故障恢复核验与重新准入单独处理，查询不自动解除 fault。

读写两向都校验动作与结果，DDL形状CHECK不能代替下表；违反者写入返回InvalidIntent，读到既有矛盾返回CorruptState，不取正文补猜：

| 动作 | 唯一合法成功结果 | 合法确定队列拒绝 |
| --- | --- | --- |
| Submit | Queued或DirectSelected；item=本登记ID，revision=1 | DuplicateId、Full、SequenceExhausted |
| Edit | Edited；item=登记item，revision=expected.checked_add(1) | DispatchStarted、NotFound、RevisionChanged、RevisionExhausted |
| Cancel | Cancelled；item=登记item，revision=expected | DispatchStarted、NotFound、RevisionChanged |
| Prioritize | Prioritized；item=登记item，revision=expected | DispatchStarted、NotFound、RevisionChanged、Suspended |
| Resume | Resumed；restored为0–3 | 纯resume无队列拒绝；前置认证/SQL故障不是QueueOutcome |

QueueRejection枚举中的Busy等未在上表列出的组合均不能落稳定回执；driver的Busy/Blocked等按DriverError返回，不偷塞到管理结果。空Resume成功0是合法幂等事实。

闭合故障点（只在cfg(test)测试控制器，生产无任意SQL/字段setter）：RegisterInsert/BeginInsert/SettleUpdate/StartupUpdate各语句前；RegisterCommit/BeginCommit/SettleCommit/StartupCommit各真实COMMIT前与**实际COMMIT成功后**。动作AfterCommitReturnUnconfirmed或AfterCommitPause抑制回报；BeforeCommitFail确保真实ROLLBACK；Pause允许测试取消future并观察detached连接退出。每个hook一次消费，不能用COMMIT前Err冒提交未知。随机Unavailable/单次Collision以及迁移RegistrationTable/OperationTable/RegistrationIndex/RegistrationMetadata/RegistrationPragma/Validation/Commit各点同样封闭测试hook。可观察证据为真实SQL行/版本、公开RequestFact、Processing视图、driver调用计数、owned任务join结果；不以测试hook日志代替COMMIT证据。

测试编译边界：外部integration crate不假定库cfg(test)可用。它通过配套Storage::start_queue_service_with_repository将真实SqliteRequestRegistry包一层R；wrapper对目标方法先await真实inner完成，再可控返回CommitUnconfirmed或等待屏障，提供真正COMMIT后未知/取消证据；其余方法逐字委托inner，不能返回伪成功或修改private grant。精确SQL语句前/迁移/random的cfg(test)故障点由tester另在隔离副本的同crate test模块资产使用，生产构建无控制器；两种证据分别记录，不把wrapper调用前Err冒充真实事务回滚。此修订不要求外部integration crate获得私有hook。

### 内存队列与 SQL 的非原子边界（后续接线门槛）

SQLite事务不能同时提交InputQueue<T>。本阶段实际owner按配套合同实施：持久applying → 短同步队列操作一次 → 保存稳定回执 → 发布管理受理事实。队列协调锁不跨SQL await；owner保留一次执行结果并用pending隔离调度，可信Settle在任意pending阶段仍即时按短锁顺序处理。reader/审批/中断独立。纯token或稳定受理本身都不授原生首次写许可。

在 applying 提交后、内存修改前、修改后、settle 提交前后各设可观测屏障。pending 操作的正文与结果由节点 owner 拥有，不依赖浏览器 future；调用者断线不取消已受理节点责任。pending 未稳定前不得将 Submit/Edit 新内容交 writer，也不能提前报告取消成功后仍走旧准备发送。确定取消/编辑/发送顺序、待稳定期间的控制响应及整目标隔离方式在配套 `queue-owner-application-draft.md` 中精化，须与本文件同gate独立检视；本草案不以文字建议代签它。

此子阶段没有任何 writer/Run/approval native side effect；applying 崩溃表示管理操作应用结果未确认，不能称原生副作用 Unknown。未来原生发送须有独立必要持久事实、唯一 claim、资源代际和最后传输围栏，**未完成这些门槛不能接 writer**。旧 accepted Run 唯一索引不充当队列锁。

## 重启、时间、清理与审计

- boot 产生新的 RuntimeEpoch，startup 只把旧 runtime 的 applying 行 CAS 标为 interrupted，保留 operation/原 actor/意图。旧 lease 不能 settle。没有新正文入队，不重新调用 edit/resume，不恢复旧 prepare token。interrupted 只表示管理应用未确认；零原生发送保证依赖上述禁止未稳定内容写出的接线门槛。
- 已 settled 回执保留原历史事实，当前队列重建为空；只展示正常队列状态，不新增专门丢失提示。新的登记且尚未消费的 ID 可以在重启后经当前权限和新上传正文首次消费，它从未入队；**已消费**旧 ID 无论有无正文都只返回原事实，不能借正文重传复活。
- NodeClock返回节点系统UNIX毫秒i64，允许前于epoch的负数；事件记录用该值，不接客户端时间，不比较过去/未来，不算统一expires，不读写旧time_floor，不以旧request_epoch不可用封锁新登记。真实系统取样不引入“缺时钟则永久停派发”分支；单调deadline/cancel仅控制本次工程等待，不使登记/已受理请求到期。可控NodeClock只用于测试校时顺序。
- 最小阶段不提供生产清理 API，不删除 registry/operation 行。未来可清理详细回执，但必须原子先置 retired，再删除可独立删除的详情，永久保留 ID/原意图拒绝能力；applying、未收敛原生/审批/资源责任不可清。retired 只返回 Retired，不再消费。此策略不使用审计90天当登记寿命，也没有10000命令默认门槛。
- audit 是独立 best-effort 端口/事务，不能作为 register/begin/settle 的前置条件或同一事务中可能令必要事实回滚的写入；仅 audit 失败/满额返回告警，必要事实成功仍可继续。告警中无正文/凭据。此阶段不实现审计保留/清理，也不借旧四个保留 slot 取得必要去重保证。
- 物理必要数据库损坏/SQLITE_FULL 自然可能同时影响 audit 和 ledger：此时按 ledger 失败阻止相关新动作，不能把它伪装成“仅附加审计失败”。独立故障注入必须区分两个条件。

## 纯增量 SQLite 候选 DDL

见同目录 `schema5-registration-draft.sql`。只新增两个表和一个查询索引，引用既有 clients/product_targets；不 ALTER/DROP 旧表/索引，不复制旧正文或旧 command。新表无 expires/time_floor、run 必填、原生正文、result_json/raw blob。revision 是固定8字节 BLOB，不含正文。表中 CHECK 保护形状；adapter 再核对 ID 规范编码、目标及动作结果语义。

此 DDL 只承载**登记及队列管理请求回执**。它没有持久等待顺序、原生发送 claim/结果、审批唯一决定或资源账本，不能独自放行相关派发。后续按 actual owner 合同提出新的增量 DDL，不将空缺用旧 product_runs 路径补齐。

## 迁移兼容与失败策略

1. 读取现有 preflight 的真实 committed WAL；不使用 immutable、不复制在线 main 文件冒充一致快照。保留 DataDirectory owner/ACL/目录锁规则。未知版本、旧定义改动、残缺库、缺失 node_meta 等按现有规则拒绝，不自动清库。
2. 将 SCHEMA_VERSION 变为5是下一阶段受审后代码改动。validate 保留 CORE/SESSION/product definitions/delivery definitions 的原字面定义和原版本规则；对5增加新定义校验，node_meta 缺失时新身份相关表必须空。对0–4增加新表/索引的部分布局检测，避免旧版本带半套新表被当正常迁移。
3. 一个 BEGIN IMMEDIATE 内从0/1/2/3先按既有顺序完整建到4，再执行新 DDL，然后同时更新已有 node_meta.schema_version=5 和 PRAGMA user_version=5，最后 validate5 并 COMMIT。node_meta 为空的新库仍保持为空，让现有身份初始化按5写入；不能凭新表制造节点身份。中途任何 DDL/metadata/pragma/validation/commit 失败回滚，新旧布局不能混合。
4. 全部旧表记录、索引定义、身份/信任/撤销、key_file_ref/public_key、schema4 response-attempt 和旧 content_objects/approval raw bytes 原样保留；只允许 schema_version 元数据变化。旧 product_control 的 epoch/floor 保留原值但不驱动新输入。
5. 不把旧 product_commands/session_commands 导入新登记，更不能转换旧 accepted 为新队列项或重发任务。新账本查无旧 ID 必须 UnknownRegistration；当前无客户端自选注册 ID 的入口，所以缺行不授新资格。原 schema4 未收敛工作/资源仍应独立保护，不能因新账本为空宣布资源空闲；真实 startup/资源装配另 gate。
6. 无损不等于旧可执行程序能读 schema5。旧程序因未知版本正常拒绝；不自动降低 user_version 或删除新表。上线前由用户既有备份流程保留一致副本；此任务不访问或备份用户实际数据库。回退程序须明确支持5或使用经授权的一致迁移前副本，不能覆盖用户运行后的新管理事实。
7. cancel/drop 的事务连接不可回池；BEGIN/COMMIT 未知保留不确定范围，启动后重新 validate/read。迁移失败测试核对原主库/WAL/侧文件和权限，但此草案没有实际执行任何数据库操作。

### 旧schema3/4独立资产如何跟随当前schema5

旧独立资产不能由coder改oracle。tester在独占目录基于输入SHA生成**新版本候选**，reviewer核对仅受目标版本影响的差异后再集成；原文件摘要和既有阶段报告保持原快照，不把新运行结果回填旧报告。所有原断言保留在当前候选中运行，不删除、ignore或只跑新测试。

| 实查旧资产 | schema5后的精确处理 |
| --- | --- |
| `app/tests/acceptance/schema3_migration.rs` | 主要版本断言已引用SCHEMA_VERSION，原 `product_*` 非control表数10、空审批交付表数4等继续不变，因为新增表用node_request_前缀；不得因多两张新表顺手改这些旧表oracle。新表空/无旧内容转换另加断言。 |
| `app/tests/acceptance/schema4_migration.rs` 的 verify_four、重开user_version | 当前Storage::open目标变5后，tester新候选将**当前终点** `(4,4)`/pragma4改为5；固定schema1/2/3/4 fixture的版本数字、PINNED_PRODUCT3原DDL、旧delivery约束和11张product_ STRICT表计数不变。fixture不能用新Storage创建后降版本来伪造旧库。 |
| `session_storage/product_schema/tests.rs` 与 delivery_tests.rs | 开发测试当前终点4改SCHEMA_VERSION；旧product/schema4定义与错误布局预期保留。旧“future=5”夹具若缺新两表，当前应以UnrecognizedSchema拒绝而非称UnknownSchema5；真正future测试使用77。不得因支持5而让“4号头部+半套新表”通过。 |
| MigrationFault旧Metadata/Pragma/DeliveryIndex/DeliveryTable等 | 原1→4阶段和旧故障点保持原位置/语义；在4之后追加RegistrationTable/OperationTable/RegistrationIndex/RegistrationMetadata/RegistrationPragma等新点。所有阶段仍属于一个BEGIN IMMEDIATE，失败从任何起点完整回滚。旧DeferredCommit/CommitUnknown继续验真实事务，新回报未知成功终点为5并另记录证据。 |

initialize_for_test/原生产initialize的职责仍是迁移到当前支持版，不为了旧测试另造能降库/随意改目标的生产接口；新增阶段接在原4布局建立之后。按旧fixture确认4布局和原字节，再升级5。旧独立资产在新源上的受影响预期必须由tester明确版本化，这属于正常规格重基准，不降低事务/保真验收。

## 具体后续增量写范围与合入顺序

| 子包 | 可写范围（待独立方案通过后） | 明确交付边界 |
| --- | --- | --- |
| R1 内层/应用登记 | 新 `app/src/domain/request_registry.rs`、其开发测试；新 `application/request_registry.rs`、开发测试；domain/application mod 声明 | 意图形状、防重分支、可信 authority/lease、应用假仓储合同；不改纯队列源或旧请求实现。 |
| R2 真实仓储/迁移 | 新 `session_storage/request_registry.rs`、`registration_schema.rs` 及开发测试；`schema.rs`、`session_storage/mod.rs`、`storage.rs` 必要接入；新独立 acceptance 资产由 tester 拥有 | schema5候选、真实SQLite故障/竞争、无正文写入；保留全部旧布局。R1之后串行，schema/storage均共享入口。 |
| R3 actual owner 受理 | 同gate通过后新 `application/queue_owner.rs`、开发测试/声明；`domain/input_queue.rs`和开发测试仅新增suspend_waiting及settle复用；`storage.rs`增加启动claim/工厂 | 实际唯一owner、配对NativeQueueDriver、真实InputQueue+账本、一次应用与native无Run失败可信来源围栏同阶段实现。原生发送日志/真实writer/协议归因/WSS仍归WP3/4；旧39保回归，不预勾完整任务。 |
| R4 WSS/Web | **另 gate** 后 management_ws/node/lib/clients/web、封闭DTO和正文交接 | 真实认证授权、A/B查询、撤销、无额外签名；继承现有认证控制帧规则，不以旧输入预算交接正文。 |

阶段文档只追加当前合同/plan 的准确状态；tests.md 由 tester 维护，历史报告不重写。实现前重新检查工作树，声明 source snapshot 和独占构建 target；不提交/推送、不改索引。开发检查按 README/package.json，最终真实工程检查覆盖集成源，未运行的 Web audit 保持用户选择 not-run。

## 独立验收必要断言（交给 tester 定义，不代写 tests.md）

- 身份来源：伪造 actor/ID/布尔权限不能注册；节点随机失败/碰撞不覆盖；未认证/撤销不能新登记或读取。登记响应丢失不触发队列/原生动作。（CL-009/104，L3/L4；真实入口另L5）
- 同 ID 同意图的 applying/settled/重启/retired 分支均不返回第二 lease；异 action/target/item/revision/body digest 冲突；B新ID编辑A项与B冒用A登记分开；同文新ID可新登记。（CL-017/018/080/088/104）
- 两独立连接/事务同时 consume 同 ID，至多一 INSERT/lease；不同 ID 不受 audit slot/旧 command10000 上限；目标等待不插旧 Run。（CL-004/081/087）
- register/begin/settle 各语句、COMMIT 前后故障/未知/drop/cancel；rollback 无半行，commit未知不返回执行权，重试只读原事实。lease精确CAS、旧runtime和旧operation不能settle；相同settle幂等、不同结果冲突。（CL-004/075/076/104）
- actor在登记后消费前被撤销必须拒绝；消费COMMIT后撤销，内部settle无需原actor/gate且保原事实；当前query/新操作仍拒绝。真实派发撤销语义留待owner/WSS测试。（CL-092/104）
- 校时前后/一天以上不改登记资格；旧floor只读/不调用，新账本不依赖其值；工程deadline按单调时间。revision0/1/u64MAX和大端编码往返，不截断。（CL-079/082/096/097）
- 自有 schema0/1/2/3/4 库升级、已到5幂等、unknown/残缺/定义异常/半套5拒绝；真实 WAL-only committed、0字节侧文件、ACL/目录锁和全部故障点。旧身份、撤销、全部旧正文/审批原字节及旧schema4尝试表摘要不变。（CL-004）
- 合成正文/旧编辑全文只在内存，检查新参数和真实SQLite main/WAL/journal/临时路径均无合成标记，旧正文读取接口不被新路径调用。账本的摘要不是正文恢复依据。（CL-062/095/098）
- 仅附加audit失败/满额仍可登记消费settle，可靠事实不能误报audit成功；必要账本失败则停止相关新动作，且不是全局吞掉安全控制。（CL-083/084/094）
- 生产清理未实现明确not-run；用端口夹具模拟retired/详情已删/未知ID，全部不能被当新动作。不得为验清理而删除受保护真实数据库记录。（CL-080/083）
- applying崩溃不重跑内存修改、不恢复正文；settled历史回执与当前空队列分开。实际owner的各屏障、编辑/取消/发送竞态、失败直接选择、resume幂等、writer数量和任务关闭必须后续独立L5，不以账本测试代签。（CL-098/099/112/115）

## 编码前待收敛项与潜在歧义

这些是具体工程/行为边界，不把建议或沉默写成已确认。独立 reviewer/tester须先核对；涉及新增用户可见规则时回到主执行者，不由coder定产品。

1. **同内容的等价表达**：本阶段只接可信ValidatedBodyPort的结构内容及digest，不定义另一套JSON canonical。tests CL-018含“同内容合法表达”，WP4须固定相应网络表示向量；不能删字段造成相同digest，也不能把已有类型的正常等价表示误拒。本阶段真实owner使用该可信端口，测试明确合成边界；不阻断账本与owner实现，也不宣称网络codec已验。
2. **队列满等确定拒绝固定回执**：按session-execution“同身份同内容返回原事实”和“原队列项或处理结果”，首次进入Applying后确定Full等也存稳定Rejected，同ID重试回原拒绝，不再尝试修改队列。容量改变后希望新尝试须新登记；这属于防重工程精化，交独立reviewer核对，不另设用户许可阻断，不新增身份期限或容量限制。
3. **管理应用中断与恢复**：Interrupted只表述管理应用未确认，不等于“取消成功”“原生Unknown”或允许跨重启重跑。配套owner合同明确同进程未D的唯一lease恢复、已D只补settle，以及Processing借用视图；本阶段按此实现和验证，不把它再留作R3待定。
4. **登记请求本身丢响应**：本候选刻意不接收客户端指定ID，也不增加另一套可复活的bootstrap nonce。允许产生未消费登记；客户端取得ID之前没有输入受理。若产品要求register本身幂等，必须单独定义认证通道内的register invocation身份，不能递归套旧TTL/JWS。现规格并未要求每次登记恰一ID。
5. **登记清理寿命**：当前最小实现不清理必要身份。未来若要永久删除registry行，需要证明节点不会再生成旧身份且未知ID永不注册；本候选选保留最小tombstone，代价是必要元数据增长，不新增暗阈值。实际清理策略属下游，不在此草案中提前实施。
6. **网络输入与可信port边界**：ValidatedBodyPort/RequestAuthority是可信装配接口，不对客户端公开ActorContext、摘要即授权或bool工厂。WP4必须从已有AuthenticatedActor和真实结构验证构造它们；此阶段公开trait的测试实现只能证明声明的替身前提。

## 本次实际完成和未执行

已只读核对当前文件、Git HEAD/未提交状态、schema4定义、迁移算法、Storage和认证入口，并输出本草案和DDL文本。没有实现产品、执行编译/测试/迁移/用户数据库访问，没有Web audit、依赖新增或外发，没有提交/推送/发布。SHA冻结仅证明这些文档字节，不是方案通过或产品能力证明。
