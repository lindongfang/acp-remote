# 管理输入上传 v10 编码与实施合同

日期：2026-11-10。状态：`acpi-route-v10` 已通过独立 tester 与 reviewer；本子合同随活动 change 合入，放行范围仅为 Rust 认证 WSS 上传路由编码门。

本包在 v9 完整方案上修复独立 tester 对候选清单、R05 fixture 和既有反向回归的剩余阻断。v4—v9 原文件保留；本包不把历史 tester 的有条件接收改写成实际验收。文件摘要见 MANIFEST.json，产品及活动文档输入摘要见 SOURCE.json。

## 完整输入与优先关系

- [活动规格](specs/management-input-upload/spec.md) 是 v4 原字节副本；[活动验收定义](tests.md) 由 v4 原文经 v5—v10
  独立审查修订，不再是原字节副本。
- 本文末“完整 v4 技术正文”保留全部 v4 技术选择、八组时序和被动观察约束，不以摘要替代。
- REVIEW-v4-reference.md、TEST-DEFINITION-REVIEW-reference.md 保留原审查，均非本版通过证据。
- 本文只在下述“当前源码适配”与“准确写集”取代 v4 的旧安装范围及旧外部路径。其余合同全部保留。
- [活动设计](design.md) 已插入相同 design 增量。
- [活动验收定义](tests.md) 已追加同版测试正文并保留 CL-001—123；本文件是本合同和完整 v4 技术正文的单份集成文件，不改变验收步骤或预期。

## 当前源码适配

当前 management_ws.rs 有两个独立 OnceLock（queue_reads、queue_management）；lifecycle.rs 先后安装两者。v4 仅提 queue-read/upload，已不足以描述当前完整服务集合。

采用一个 OnceLock<NodeManagementSessionServices>，包含原队列读取、原队列管理、原上传服务。该 facade 在 node/session_services.rs 定义，字段私有，只能由原 OwnedNodeSessionServices 的方法构造，不接受三份外来句柄拼装。它保存原 socket_authority；提供 belongs_to、queue_reads、queue_management、open_upload 方法。ManagementService 的单次安装先验证 authority，再对唯一 OnceLock set；任何失败不改变原集合。仅控制面不安装。`node/session_services/development_tests.rs` 增加生产顺序证据：在只读 Node 的 BeforeServices 暂停时，HTTPS listener 尚未成为产品入口；对照安装成功后队列读取和队列管理仍取原 bundle 实例。直接的结构拒绝测试不冒充真实网络启动失败。

node.rs 仅增加此 facade 以及 UploadOwner 的 crate 内重导出，以跨模块使用私有实现；不公开 NodeInputUploads 或增加它的可变安装入口。现有 UploadOwner 的方法与状态类型均可透过返回值投影使用；如果 Rust 可见性要求额外重导出 UploadPurpose/UploadPhase，则仅重导出这两个现有类型，不改其语义或算法。

原 install_queue_reads/install_queue_management 生产入口由单一 install_session_services 取代。旧“单次安装/foreign node”开发测试改为对完整 bundle 断言，保留 duplicate/foreign 两类错误与原服务继续可用的证据。queue_reads 与 queue_management 命令改从 bundle 取得原服务，不改变登记/consume 业务规则。

四条上传命令与 Binary 共用 attachment 中 canonical UploadOwner。现有 Owner::current 将 RegistryError::InvalidActor 映射为 UploadError::InvalidOwner，而 target_guard 经 current_actor_guard 将认证撤销/存储失败分别压成 ViewError::Revoked/ReadFailure，再被 view_error 压成 InvalidOwner；路由层无法恢复来源。因此新增最小 `UploadRoutingError` 适配器于 node/input_upload.rs：

- `Business(UploadError)`：owner 的目标/阶段/冲突/正文/分配/停止等业务事实。文本直接使用 UploadError::code；Binary 关闭 4400；其中 NodeStopping 文本 product_not_ready、Binary 4400。
- `Revoked`：owner 内当前 actor 复核先观察到撤销。文本和 Binary 都只结束原连接为 4410，不生成结果/ACK。
- `StorageUnavailable`：owner 内认证存储、时钟或必要锁的认证复核失败。文本和 Binary 都只结束原连接为 4500，不生成结果/ACK；不把该错误当上传业务 necessary_storage_unavailable。

`begin/finish/status/release/accept_data` 增加只供该路由使用的 typed 方法，内部与现方法共享同一检查/状态变更函数；现有公共方法的返回与队列管理调用行为保持不变。typed 适配不从错误后再次查询来推断原来源，也不重复取得外层 actor gate。新增确定性 WSS 用例：在 owner 显式复核点分别注入撤销和认证存储失败，断言 4410/4500 且无结果或 ACK；上传正文必要存储故障仍为文本 necessary_storage_unavailable / Binary 4400。队列管理里已有的 Upload(_) → invalid_request 保持其原范围，不用于上传路由。

该显式复核点固定为 `node/input_upload.rs` 中 typed `status` 与 typed `accept_data`
共用的 `Owner::typed_entry_guard`。`NodeInputUploads` 持有、`Owner` 克隆一个
`#[cfg(test)] UploadRouteFaults`；`arm_route_fault_for_test(client, fault)` 以原
`LiveSocketContext.connection_id()` 绑定，返回含命中计数的 receipt，并由 RAII
guard 保证未消费故障在用例结束时撤销。`UploadRouteFault` 恰有三种值：
`ActorRevoked`、`ActorStorageUnavailable`、`UploadStorageUnavailable`。
`typed_entry_guard` 在完成 connection/owner/client 校验后、读取上传表或再次
访问认证存储前消费至多一次故障：前两种分别返回 `Revoked`、
`StorageUnavailable`，第三种返回 `Business(UploadError::StorageUnavailable)`；
没有故障时继续原 typed 流程。命中函数只记录原子计数，不等待业务锁，正常
build 不包含该字段、方法或分支。

## 准确写集与串行顺序

本阶段只接 Rust WSS 上传，不将尚未定义的 Web 上传协调、普通输入 UI、审批或恢复接线纳入本次编码门。它们仍由现有 WP4 后续任务承担，不从完整 change 删除或视为完成。

方案通过后按下列顺序合入：

1. 新增活动规格 specs/management-input-upload/spec.md，来自 spec-candidate.md；仅删除导语中尚未合入候选的状态描述。
2. proposal.md New Capabilities 增加一条该 capability 的登记。
3. design.md 插入 DESIGN-DELTA.md 正文。
4. 新增活动 change 的 management-input-upload-contract.md，集成本合同与完整 v4 技术正文并更新同包引用；implementation-contract.md §6 增加指向该子合同的链接。
5. tests.md 追加测试候选全文，仅更新合同链接和候选导语；CL-001—123 原文保留。该机械集成不授权 coder 改验收预期。
6. plan.md 记录独立复核、WP4 串行写集和 WP5/WP6 同版验证；tasks.md 只给现有 4.1 增加 ACPI-R01—R10 完成判据，不新增 checkbox，不勾任何任务。
7. 产品文件：management_ws.rs、management_ws/session_commands.rs、node/session_services.rs、node/lifecycle.rs、node.rs、node/input_upload.rs 以及 node/session_services/queue_read_development_tests.rs、node/session_services/acpi_upload_development_tests.rs 或该目录独立上传/服务装配开发测试叶；若启动失败路径需要确定性测试注入，node/lifecycle.rs 的现有 test hooks 可增加单一只读开关，禁止改变正常路径。如需要模块声明仅在上述现有文件添加。不改正文准备算法、ACPI codec、connection_owner、socket_authority 或 Source facade。

测试专用接缝固定如下：

- PendingReads 上传命令 before/after-call 暂停；取消触发改为真实 Node stop。不得用 peer Close 作为该暂停点的取消来源，因为原 reader 在等待 pending future 时不会再次进入 `receive_frame`；停止分支释放后由原 supervisor 等待 connection owner 与 writer join。
- Data accept 成功后、ACK enqueue 前暂停；取消同样由真实 Node stop 触发，确认 ACK 未入队、唯一 1001、owner seal。
- 首次服务集合失败注入只在 `#[cfg(test)]` FactoryTestHooks 增加一次性 `reject_services`。安装顺序在构造 listener 和 router 前；`OwnedNodeSessionServices::start` 完成后立即返回 startup error，使已创建的 body/native/storage 沿原有 Drop/seal/owner 路径收敛。测试在 `ready()` 返回错误后使用原 `NodeProcessOwner::join_actual()`，通过仍由 lifecycle resources 持有的 storage 调用 `prompt_preparation_receipt().join()`，断言报告 `all_joined()`；并用启动前固定的地址确认 HTTPS 连接未开放。不得伪造 join 或依赖 pending 计数。
- control-only 入口固定为 `NodeRuntime::start_control_only_for_test`；它在原 runtime
  内调用 `NodeProcessOwner::begin_control_only_for_test`、等待同一 `ready()`，由
  `install_session_services` 的既有 `control_only` 分支跳过服务构造和安装，同时
  保留原 TLS/WSS 管理入口。测试使用现有认证 pair/client helper 建立真实连接，
  再分别发送 begin、finish、status、release 的有效形状 payload；不使用直接在
  `NodeInputUploads` 上构造的错误替代 WSS 入口。
- R03 的 preparation join 直接沿用 `storage.prompt_preparation_receipt().join()` 与返回 report 的 `observation().all_joined()`，不新增任务观察 API。
- R05 的 L5 主链固定复用 `queue_management/development_tests.rs` 的 `Case`：
  该 `Case` 已启动原 `NodeProcessOwner`/`NodeRuntime`、SQLite、真实认证 WSS 和生产
  session-services bundle。新增测试叶在同一 `Case` 上复用现有
  `ManagementService::observe_next_document_socket_for_test()`，完成 pair/auth 后从
  `DocumentSocketEndpoint` 取得 identity-bound
  `SocketViewClient`。测试叶仿现有 `socket_development::source_raw` 建立
  cfg(test) 模块 source/read，但使用该 endpoint 的原 authority/client，并通过
  新增的 test-only `DocumentSocketEndpoint::attach_shared_read(&OpenedView)` 将
  stream 注册到同一个原 attachment；上传、队列读取、队列管理仍来自生产 bundle。
  然后对同一 peer 发送合法 `TransferHeader::credit`，等待该原 stream
  `ready()`，记录 upload dispatch=0，再按 R05 的坏帧矩阵和精确 16384 字节
  prompt JSON 完成 ACK→finish→status ready/bodyId。
- 该 endpoint 方法只在 `#[cfg(test)]` 存在，只做现有 `SocketAttachment::attach_read`
  已执行的 belongs-to 校验和原 stream 注册，不公开 attachment、不接受外来
  authority、不构造/替换服务 bundle。`management_ws/socket_development.rs` 的
  `Harness` 可继续作为 ACPV 单元级证据来源，但不作为 R05 的 L5 主链。
- R07 FIFO 用一次 `pause_next_write` 暂停首 ACK 的 writer 边界，再入队一个 status；释放 barrier 后按序读到 ACK、status。frame/byte 饱和分支保持 writer 暂停，只断言真实 enqueue/queued/in-flight/byte 使用和下一次申请失败；释放后因 close 优先可以只发送已取出的在途帧，未发送 ACK 必须被 drop 并归还 permit，不要求把 257 个 ACK 全部送达。
- R09 使用上述 connection-bound 一次性故障 receipt。每个矩阵格在 begin 成功后
  才 arm，因此只命中目标连接的 status 或 Data 一次；receipt 的原子命中计数必须
  为 1，消费后不再影响另一连接。撤销与认证存储失败都断言唯一 Close、无
  JSON/ACK，并在原 SocketLifetime 结束时由原 attachment owner 显示
  sealed=true、entries/receiving=0；上传必要存储的 status 格断言唯一文本错误且
  连接仍可关闭，Data 格断言唯一 4400 Close 与相同 seal/条目清理。
- ACPI-V6-005 的具名回归
  `queue_read_upload_and_view_success_are_distinct_from_invalid_payloads` 必须同步覆盖：
  `input.upload.begin` 的有效形状次调用断言成功并取得 uploadId；合法
  `view.open`（`targetId` + `reason=first_open`）进入真实路由并断言冻结的
  `view.opened` 响应；保留数字 `targetId` 的 `invalid_request` 和格式合法但
  不存在 target 的 `invalid_target`。该维护不改变 R01 或 parser 验收预期。

产品修改前再次核对这些文件的实际摘要和已有修改，若变化则重新评估差异，不能整文件覆盖用户改动。正式独立验收和交付检视使用实现后新的同版源码快照，不使用本次规划摘要替代。

## 证据与待审事项

ACPI-R01—R10 是本版待独立 tester 重新接收的验收定义。R01 补齐
finish→ready→bodyId 完整结束判据；R09 固定三类来源与两入口的生产复核
seam、命中计数和实际清理观察；R10 固定现有 control-only factory 入口、真实
认证 WSS 和四命令。R05 固定到生产 `Case`，并明确 test-only endpoint 的唯一
注册能力。历史 tester 有条件接收只覆盖 R01—R08 的旧范围，不能代签新增场景。
allocation_failed / necessary_storage_unavailable 仍须区分 owner 层真实事实与
WSS 路由故障注入；本版不声称错误表所有故障已动态覆盖。新增 seam 沿完整 v4
合同，只在允许叶内实现；若做不到，先报告具体超出范围原因。

历史 tester 报告引用的 v4 reviewer 摘要 `B72C…19F9` 与当前留存原文摘要 `C7AD…2037` 不一致。两文件不视为同一证据；本轮不猜测原因，后者原文作为输入保留，前者缺失状态继续显式记录。若要复用历史 tester 对 v4 场景的接收，需其作者核实证据版本。

此包没有执行 Cargo、产品测试或真实 Harness，没有改活动 specs/产品代码、Git
index、数据库、密钥或原生会话。v8/v9 tester 阻断由 v10 定义修订关闭，仍须
新的独立 tester 接收；历史 reviewer 问题是否得到解决由独立 reviewer 判定，
主执行不代签原报告的问题关闭。

## 完整 v4 技术正文（原字节保留）

# ACPI v4 编码前合同候选

2026-11-08。本文修复 v3 独立复审提出的问题，尚待该 reviewer 复查；本作者不将 findings 标为 Closed。权威行为候选为 `spec-candidate-v4.md`，全集验收定义为 `tests-candidate-v4.md`，准确写集为 `WRITESET-v4.md`。

## 维持并明确的设计选择

- 一个 `OnceLock<ManagementSessionServices>` 不可拆分地发布 queue-read/upload facade；从同一个原 `OwnedNodeSessionServices` 构造，分别校验同一 socket authority 后单次 set。安装在 router/HTTPS 启动前完成（当前 lifecycle 的实际顺序有此依据）；重复/foreign authority/set 失败是 startup failure，control-only 不安装而返回 product_not_ready。禁止双 OnceLock 预检后分步发布。
- 一个 attachment 只 open 一个 canonical `UploadOwner`；PendingReads 可 clone 同一身份，不能按请求重开。四命令进入原连接受监督 futures，不新建 detached spawn。Binary reader 调原 accept_data，成功 ACK 走原 outbound 的新增 Binary 变体及唯一 writer；writer 在 Binary 发送前沿文本同一 actor gate复核，不改 Shared 分支语义。
- 已观察 stop 优先1001；运行中 owner 单独返回 NodeStopping 才是文本 product_not_ready/Binary4400。Malformed、foreign、offset冲突、运行中上传必要存储错误统一4400，不ACK、不JSON；只有认证 gate 的存储/内部错误沿现有4500关闭，避免与上传业务错误混同。撤销4410、预算4429。每个连接只有原writer负责Close，observer/join保P3-A原语义。
- revoke commit 后下一次actor gate复核拒绝尚未发送ACK；协议/attachment结束seal原lifetime，不要求数据库事务内同步seal。允许发送前已持actor gate的原发送先完成再commit revoke，不承诺追回已写出的字节。

## v3 findings 对应的实质修订

1. proposal 只允许一行 capability 增量，其他文字明确禁止改，消除允许/禁止冲突。
2. tests 全文独立包含 R01—R08，不引用 v2 映射补位。
3. R06 stop case 先发送坏帧，在新 `BeforeInputDispatch` barrier 已收到但未decode/调用owner处暂停，触发真实Node stop，再释放barrier；生产路径必须在dispatch前复核关闭状态，1001且dispatch次数零。测试不向已关闭socket发消息，不将select同时ready的随机先后当stop优先证明。
4. cancellation 定为边界前/后可测模型，不企图在无await的owner同步临界区中途取消。四命令future在owner调用前、owner返回后/结果enqueue前可暂停；accept_data在返回后/ACK enqueue前可暂停。新barrier不得跨owner table锁、lifetime短锁或actor gate持锁暂停。
5. R07 区分queued frame slot与writer已取走但仍持字节permit的in-flight frame。R08使用现有pause_next_write（actor gate之前）；不能使用WriteLocked暂停后等待revoke commit，否则形成测试自锁。

## 现存与新增 seam 的具体边界

**现存且可复用，未动态验证：** `FactoryTestHooks.body` 允许原preparation point observer；`CpuEntered`/`BeforeResultPublish`可用条件变量只暂停原CPU worker，async reader保持运行；畸形 JSON 产生真实准备失败。`UploadOwner::observation()`已有 sealed/entries/receiving/preparing/ready；原 preparation shutdown port 的 passive observation 包含 supervisor、CPU、disposal、sealed 和 active-use 状态，原 join wait 返回 only-after-joined 报告。Case双socket/真实TLS/WSS/SQLite/生产factory沿用。Management已有 `pause_next_write`（writer入队后、actor gate前）、测试 enqueue barrier、预算 semaphore/mpsc容量、`queue_failure_snapshot`、真实 TrustStore revoke。**现有 `WriteLocked` seam 在 actor gate 已持有之后，不能在暂停它时等待 revoke commit。**

**需编码前审查明确接受的窄新增项（均在上述五叶）：**

- `BeforeInputDispatch`：完整Binary已由原reader读出、仍未decode/执行；entered/release，释放后立即复核stop/control/actor状态，再dispatch。只在cfg(test)可安装，不保留payload到日志。
- `BeforeInputDispatch` 必须接在 stop/control 优先选择之后、Binary decode/owner调用之前，并在释放 barrier 后再次确认生命周期仍运行；这样先触发stop再放行能确定地断言 dispatch=0、唯一1001。若实际接线使读取与 stop 同时ready且无既有优先顺序，本合同的确定性 case 失败，不能用调度碰巧顺序代替。
- `UploadCommandBeforeCall(action)`、`UploadCommandAfterCall(action)`：连接PendingReads内暂停，分别在owner async调用前、调用返回后。一次性按action命中，不持任何业务锁。四命令都使用同一机制，不加后台任务。
- `InputAcceptedBeforeAck`：accept_data已返回、actor guard已释放且ACK尚未入队。暂停等待必须选择原ConnectionStop；stop胜出即1001，不挂住节点退出。peer Close仍由原reader的既有模型处理，不声称忙reader会立即读peer Close。
- 被动连接观察：同一owner的测试observer、PendingReads当前数/已创建与已drop数、protocol已结束；observer不另建owner、不seal/重放命令。receipt观察只读原身份，记录真实join分类；退出必须等原监督者，不能用计数零代实际join。
- outbound观察按连接返回byte used、queued slots、in-flight count、ACK enqueue/send/drop计数；计数覆盖QueuedFrame生命周期，writer取走后queued slot归还但byte permit仍在。frame满分支用原 Binary ACK 事件创建帧；由第一帧触发现有 pause_next_write 后，依序发出足够的、对最近块精确幂等的真实 Binary 重试事件填充256待发slot。byte饱和分支仅允许fixture暂持原 semaphore 的 `MAX_OUTBOUND_BYTES - 31` 个原预算 permit（ACK 正好32字节），不注入伪业务帧、不修改生产容量或绕过唯一 writer；到达饱和前需先通过 observe barrier 证明permit已占用。

### 必须采用的可重现事件顺序

- **R03**：生产 factory 注入一次性 CPU hook；收到 `CpuEntered` 后读 status=preparing，释放原 CPU worker，再仅通过 status 轮询至 ready/failed。条件等待使用 Notify/oneshot 注册后复查当前观测，deadline只报告挂死。状态读取本身可驱动 receipt `try_ready`；CPU join 与 status-ready 是不同观测，若需证明 join，必须查询原 shutdown port 相同 preparation identity，等真实 `Joined`，不能从 ready 推断。
- **R06 四命令取消**：每个命令单独 WSS 连接、独立一次性 before-call 或 after-call barrier；发送命令并等 entered；peer Close 由测试对端发送后，必须先等被动 `protocol ended` 和原reader/connection owner对pending future的Drop观察，再释放已无消费者的 barrier并等原 supervisor join。这样Close在future仍暂停时完成取消，不会由barrier release抢先让业务继续。after-call finish先由原准备观察确认同一 receipt 已 Registered并暂停原 CPU；protocol ended后释放 CPU barrier、等待同一 receipt真实join，不从新连接读取旧身份。Binary accept-to-ACK窗口由原 ConnectionStop取消；不假设reader await owner时还能读取peer Close，此分支以Node stop作取消触发。
- **R07 FIFO**：暂停 writer 仅用 `pause_next_write`；等待 `in_flight=1, queued=0, byte_used=32` 后，才发送后续 status 并等 enqueue barrier，通过原有序 WSS helper断言 ACK在文本前。frame saturation 需等 first in-flight 及逐个 enqueue 计数精确到 `1 + 256` 后触发下一次幂等 Data；由首个待发ACK pause 期间 reader 继续接收合法 Binary，发送下一文本/二进制时不得跳过前序 WS 输出。byte fixture permit在 case结束RAII归还，分别观测使用量及回收值为原预算。
- **R08**：先 A 写入 ACK 并在 `pause_next_write` entered（尚未 actor gate），B 执行真实 revoke 并等 commit；release 后 writer 的 gate 观察 revoked 并唯一 close 4410。反向顺序只能在持 actor gate前不暂停或释放 writer继续发送，待发送返回后再由 B commit；不能在 `WriteLocked` barrier 持 gate暂停时等待同一 revoke。

### 撤销与 seal 的线性顺序

真实撤销提交由发送前 actor gate 串行化。若 commit 先发生，尚未越过 gate 的输出拒绝，原 protocol 按既有 4410 路径结束；**seal由撤销之后原 protocol/attachment结束路径触发**，然后只读原 attachment owner 的 sealed/entries/receiving 观测并等待原 owner join。撤销调用返回本身不是 seal 收据。若 actor gate 先成功，那个已授权发送可完成后 revoke commit；合同不声称可追回 writer 已交给网络 sink 的字节。A/B隔离须以撤销前后 A 的拒绝与 B 的成功对照。

上述是确定的新增接口合同，不是声称现有实现已具备；若reviewer不接受，必须修合同再编码。测试case都规定触发和释放顺序、deadline只作挂死检测，不以sleep推断状态。缺入口时不转人工、不假通过。

## 静态范围与后续门

本次仅写C盘小型候选，无Root修改、无新工作区/target、无构建。应先由独立tester检查全集定义、原reviewer复查v3 findings及spec引用，再由Root合入三项文档；实际产品编码和编译/真实WSS独立验收另执行。不得因候选结构正确而勾完整任务。
