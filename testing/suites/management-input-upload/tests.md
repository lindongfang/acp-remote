# management-input-upload Acceptance Suite

- Source change: deliver-codex-session-loop
- Source definitions: openspec/changes/archive/2026-11-16-deliver-codex-session-loop/tests.md
- Long-term specification: openspec/specs/management-input-upload/spec.md
- Stable IDs: ACPI-R01、ACPI-R02、ACPI-R03、ACPI-R04、ACPI-R05、ACPI-R06、ACPI-R07、ACPI-R08、ACPI-R09、ACPI-R10

共用执行条件与归档源用例保持一致，运行结果另存 `testing/runs/deliver-codex-session-loop/`。裸 `specs` 路径在本长期套件中指 `openspec/specs`。

## Cases
### ACPI-R01 服务安装、四命令和真实 ACK

- 用例级别：L5
- 操作步骤：固定生产服务bundle，真实WSS begin(target(2),purpose=submit)，校验结果仅uploadId；状态应receiving/nextOffset="0"。发送ACPI v1 Data：magic ACPI，direction1，reserved0，同upload原16字节ID，big-endian offset0，payload为有效prompt JSON。读取下一帧ACK并确认`status.ready=false`；随后以原uploadId发送finish，轮询同一status至`ready=true`且非空bodyId。再用该target和bodyId执行既有真实input.register/consume，并验证`input.queue`仍使用原队列读取/管理实例。另以独立连接发送非法purpose/多余字段/数字totalBytes对照。以测试安装API重复安装同bundle及外节点bundle，断言拒绝且原bundle的上传、队列读取和队列管理三条路径继续工作。关闭清理。
- 预期结果：ACK恰32字节：ACPI、version1、direction2、reserved0、原ID和big-endian累计offset；ACK后status offset等于payload字节数且尚未ready；finish后同一status最终ready并返回唯一bodyId，原register/consume路径可关联该身份，不能只以Data ACK代替正文完成。success/error均关联原requestId；语法错invalid_request；重复/foreign安装拒绝，不发布半服务且上传、队列读取和队列管理仍可用。begin/ACK/finish不是入队或执行成功。

**既有回归同步：**`app/src/node/session_services/queue_read_development_tests.rs`
中的 `queue_read_upload_and_view_success_are_distinct_from_invalid_payloads` 必须与
R01 同步：合法 `input.upload.begin` 成功且返回 uploadId；合法 `view.open`
（`targetId=target(2)`、`reason=first_open`）进入真实路由并断言冻结的
`view.opened`、`streamId`、`viewEpoch`、十进制字符串 `snapshotRevision`、
`capabilities.opaqueChunks=1` 和封闭 `status`；数字 `targetId` 仍为
`invalid_request`，合法但不存在的 target 仍为 `invalid_target`。不得删除
“真实成功/语法错误/目标不存在”的区分，也不得用 coder 临时预期替代 tester 定义。

### ACPI-R02 同块重试与偏移冲突

- 用例级别：L5
- 操作步骤：begin后发offset0数据块P读ACK，再原样重发最后一块P，读第二ACK并查询status；另起独立连接分别发同offset不同payload、跳过累计offset、已finish后再发Data，各自保留owner被动观察和完整WS输出。
- 预期结果：原样重试返回相同累计offset，正文不重复附加；冲突/错误状态零ACK、零JSON，唯一4400 Close；冲突前owner offset不因拒绝帧推进，连接结束receiving清理。仅承诺最后已接收块重试，不把更旧任意offset视为成功。

### ACPI-R03 准备阶段、失败和字段投影

- 用例级别：L5
- 操作步骤：经现有 FactoryTestHooks.body 注入原 CPU worker 的 CpuEntered 一次性 barrier。WSS 上传有效 prompt JSON 并 finish，等 CPU entered；读取 finish 结果后发 status，必须 preparing/nextOffset 为总字节，bodyId/error 均省略。释放 worker，轮询真实 status 至 ready，确认 bodyId 且无 error。另以单字节 `[` 上传并按原 worker 解析，轮询至 failed。长度不符 case 期望文本 invalid_upload_state 与原 receiving 状态，不应假设它创建 receipt。需要 join 证据时直接调用现有 `storage.prompt_preparation_receipt().join()`，只接受返回报告中 `observation().all_joined()` 为 true；CPU receipt 的 `observe().identity()` 必须与进入 CPU 时相同。deadline 只报告挂死。
- 预期结果：finish仅返回原uploadId，不宣称ready；真实解析成功才ready，畸形JSON经原worker失败后error=invalid_request且无bodyId；长度不符invalid_upload_state，不夺走原buffer；ready不是队列受理或Harness已执行。原 preparation shutdown report完成实际 join，ready 或状态轮询不能替代 join。

### ACPI-R04 释放、未知身份和双连接隔离

- 用例级别：L5
- 操作步骤：A/B两个authenticated socket各begin；A release自己的upload，结果固定released=true，再status/重复release；分别对未知随机ID及B使用A的ID发送status/release。另用新A upload让B发送Data，记录Close；A连接仍保留时发自身合法Data，再验证第三个有效连接。最后建立 A2 upload并记录ID，正常关闭A2；以新认证连接A3分别发送该ID的status、release和Data，并通过原连接观察记录确认旧owner已seal，不使用同时存活的B代替断线证据。
- 预期结果：释放真实改变原条目；release后status为body_unavailable，重复release沿原owner幂等接受released=true；未知/foreign文本为body_unavailable（有效actor下，不泄漏归属）；foreign Binary零ACK/JSON、4400，只关闭B；A及其他有效连接可继续。A2关闭后新连接不能复用旧upload所有权：status/release为body_unavailable，Data零ACK/JSON、4400。

### ACPI-R05 ACPI/ACPV分类、格式和上限

- 用例级别：L5
- 操作步骤：在现有 `queue_management/development_tests.rs` `Case` 的生产 Node 上新增独立认证 socket。连接前调用现有`ManagementService::observe_next_document_socket_for_test()`取得`DocumentSocketEndpoint`；pair/auth后由其identity-bound client建立与`socket_development::source_raw`同类型的cfg(test)模块source/read，并调用新增test-only`DocumentSocketEndpoint::attach_shared_read(&read)`注册到同一个原attachment。上传、队列读取、队列管理仍取生产bundle。用`TransferHeader::credit(stream.id(),0).encode(&[])`发送合法ACPV credit，等待原`stream.ready()`为true并记录upload dispatch计数零。每个坏帧用独立认证连接：四字节ACPI、31字节ACPI头、version错、reserved非零、direction2 ACK反发、0 payload、16385字节payload、offset溢出；另发非ACPI非法binary及32769字节binary。另起上述生产Node fixture的独立认证连接 begin，发送精确16384字节有效prompt JSON Data（helper构造并断言长度，不靠空格截断破坏JSON），读ACK、finish并轮询status至ready/bodyId。endpoint只复用原attachment注册方法，不公开attachment或service handle。测试记录分类计数与完整有序输出。
- 预期结果：合法 ACPV credit 使原 stream ready 且upload dispatch计数零；以完整ACPI magic起始的坏帧不进入ACPV；各ACPI格式/方向错误零ACK/JSON、4400，非ACPI非法控制沿原4400；大于32KiB使用4429。精确16384字节有效Data得到一个32字节ACK，finish/status真实ready；不新增总正文大小限额。

### ACPI-R06 可取消边界、停止优先级与原任务收敛

**R06 取消来源澄清：**PendingReads future 暂停期间，同一原 reader 不会再次轮询 `receive_frame`，因此不得通过 peer Close 解除该暂停；取消统一由真实 Node stop 触发。Before/After-call transfer 和 InputAcceptedBeforeAck 均为一次性、无业务锁暂停点；测试触发 Node stop、等 control 非 ready 与原 future Drop 后释放 barrier，再由 `NodeProcessOwner::join_actual()` 读原 owner 结束。AfterCall finish 还要先用 preparation observer 确认真实 receipt 已 Registered，停止后释放 CPU barrier，并以 `storage.prompt_preparation_receipt().join()` 的 report 判实际 join。

- 用例级别：L5
- 操作步骤：每分支独立 Node fixture，四命令分别在 PendingReads 的 BeforeCall 和 AfterCall 一次性 barrier 暂停（无 owner/actor/lifetime 锁）；触发真实 Node stop，等 control 非 ready、原 connection 进入终止，释放 barrier，再确认 future drop、owner seal 和 connection/writer join，不出现取消后的后台补执行。finish BeforeCall 分支无 preparation receipt；AfterCall 分支用原 preparation observer 确认唯一 receipt 已 Registered，并暂停原 CPU；Node stop 后释放 CPU barrier，用 `storage.prompt_preparation_receipt().join()` 的 report 等待同一 task 实际 join，不再从旧 socket 查询。accept_data 返回后在 InputAcceptedBeforeAck 暂停，触发 Node stop；该 await 由原 ConnectionStop 取消，确认零 ACK、1001 与 owner 最终 seal。stop优先分支：在活 socket 先发送坏 ACPI，等 BeforeInputDispatch entered（已接收未 decode），触发真实 Node stop并等 control非 ready，释放barrier；确认处理前 lifecycle复核、dispatch次数0和唯一1001，不向已关闭连接发送第二帧。反向对照：不触发 stop，先让坏帧 dispatch并观察4400，随后停止Node。另由测试仅 seal 原 owner、保持 Node运行，分别发 status 与 Data，观察文本 product_not_ready 和 Binary 4400。
- 预期结果：取消不补执行、不重放、不伪造成功；BeforeCall零新增条目/receipt，AfterCall以真实原事实处理，结束后receiving=0、sealed=true；已移交内容仅有一个原receipt，observer取消不等于worker join。stop先于dispatch复核时1001、decode/owner dispatch次数零，不向已关闭socket再发帧；格式先被处理分支4400且只一个Close。运行中owner单独sealed时文本product_not_ready、Binary4400；Node/连接所有自有原任务实际join或明确unknown，不能以pending=0代签。

### ACPI-R07 FIFO、背压和精确预算回收

- 用例级别：L5（byte饱和分支另明确使用测试资源占用，不冒充网络自然填满）
- 操作步骤：FIFO 分支在 begin响应排空后设置原 `pause_next_write`（writer 已取出下一帧、actor gate前），发 Data 并等 writer entered，确认 ACK 已被取出：byte-used=32、queued=0、in-flight=1；发送 status 并等其 enqueue count 增加，释放 writer，以有序 helper 恰好读到 ACK 后 status 文本，等 send/drop 计数及预算归零。frame饱和独立 case 保持同一个首 ACK 在 writer pause 中，对最近块做256次真实幂等 Binary 重试，等每次 enqueue 观察累计达到 1 in-flight+256 queued，再发一次真实重试触发4429；随后释放 writer，断言 close ready 优先，已取出的 ACK可发送，未发送 queued ACK全部 drop且 permit归还。byte饱和独立 case 同样先暂停 writer，由 fixture 暂持原 byte semaphore 的 `MAX_OUTBOUND_BYTES - 31` permits（无假业务帧、无容量调整），WSS 发合法 Data 使32-byte ACK预算获取失败，观察4429；RAII释放 fixture permits，不要求伪造已发 ACK。
- 预期结果：FIFO分支只承诺成功 enqueue 的 ACK在后续文本前发送；byte permit包括in-flight并在send/drop后归还，writer取出时queued slot归还。frame分支证明1 in-flight+256 queued后下一次申请失败并close=4429；释放后因close优先不保证257个ACK均送达，queued ACK实际drop且预算回收。byte分支证明32-byte申请失败走4429，不外推自然慢网络已耗尽1MiB。两分支不另开sink，发送关闭都由原writer。

### ACPI-R08 撤销、目标校验和其他actor隔离

- 用例级别：L5
- 操作步骤：使用现有真实 Case 的有效 A/B actor 和 target(2)，分别以格式合法但不存在 target(99) begin 期望 invalid_target，以语法不可解析的 target 字符串 begin 期望 invalid_request；不虚构逐 target ACL。A begin 并排空文本结果后设置 `pause_next_write`；A 发 Data，等 writer 取出 ACK、但尚未取得发送前 actor gate 的 entered barrier（in-flight=1、queued=0、byte-used=32）。B 通过真实 revoke 入口撤销 A 并等待事务 commit；commit 返回不作为 seal 证据。释放 writer，以有序 WSS helper 读到 A 唯一4410 Close；等原 protocol 结束、同 attachment 原 UploadOwner sealed 且 receiving=0，再等原 connection owner 实际 join。B 随后在自己的连接 begin/Data/status 成功。反向 case 不在持 actor gate 的 WriteLocked barrier 暂停时等待 commit：让已经取得 gate 的发送先返回，再由 B commit，记录发送先于撤销的线性顺序。
- 预期结果：COMMIT先于writer gate复核的case无A ACK，4410，最终sealed且entries/receiving=0；B ACK/status仍正常。writer gate先于COMMIT的对照允许该次原发送完成，不谎称撤销追回已发送字节；此后A操作均拒绝。目标校验不被连接认证替代，不同actor/socket不串owner；事务返回前不强求同步seal，但原协议结束后必须seal。

### ACPI-R09 认证失败来源与 typed 错误边界

- 用例级别：L5
- 操作步骤：使用 `NodeInputUploads::arm_route_fault_for_test(client, fault)` 建立的 connection-bound 一次性故障 receipt。每格在新真实认证连接上先成功 begin 并取得uploadId；随后 arm `ActorRevoked`、`ActorStorageUnavailable` 或 `UploadStorageUnavailable`，按固定3×2矩阵向同一连接的 typed `status(uploadId)` 或 typed `accept_data(uploadId,合法ACPI Data)` 发一次请求，不重试。arm 的调用点是 typed status/Data 共用的 `Owner::typed_entry_guard`：完成 connection/owner/client 校验后、读取上传表或再次访问认证存储前消费一次。记录 receipt 原子命中数、完整有序 WSS 帧、owner observation 与协议/connection owner join。另起连接证明同一 receipt 不影响另一 connection；业务 offset conflict 仍为零ACK/JSON的唯一4400。
- 预期结果：receipt 每格恰命中1次，消费后的重复请求不再注入。`ActorRevoked` 的status与Data均返回唯一4410 Close，零JSON/ACK；`ActorStorageUnavailable` 均返回唯一4500 Close，不出现invalid_request/necessary_storage_unavailable/ACK；`UploadStorageUnavailable` 的status返回唯一`necessary_storage_unavailable`文本且无ACK，Data返回唯一4400 Close且零ACK/JSON。撤销/认证存储两类关闭及上传存储的Data关闭后，原SocketLifetime结束时同一attachment的Owner `sealed=true`、entries与receiving均为0，原protocol/connection owner实际join；上传存储的status文本不伪造连接关闭，自行关闭后仍须封存清理。三类来源不得串码，失败后再次成功请求不能改写已发生的帧和命中事实。

### ACPI-R10 原子发布、启动顺序和四命令 control-only

- 用例级别：L5
- 操作步骤：使用原生产 factory 的 BeforeServices 暂停点确认此时 HTTPS listener 尚未创建，`TcpStream::connect` 到已固定的目标地址返回连接失败；释放后确认同一 bundle 同时支撑上传、队列读取和队列管理。设置 `FactoryTestHooks.reject_services` 使完整服务对象构造后、router/listener 创建前返回 startup error；`NodeProcessOwner::ready()` 必须失败，随后固定地址仍不可连接，并从仍由 lifecycle resources 持有的 storage 调用 `prompt_preparation_receipt().join()`，最后调用原 `NodeProcessOwner::join_actual()`。另在独立 fixture 调用现有 `NodeRuntime::start_control_only_for_test(area.config(fixed_address), FakeProtector)`；该方法经 `NodeProcessOwner::begin_control_only_for_test` 跳过 session-services 构造/安装，但保留真实TLS/WSS。使用与常规用例相同的pair/auth helper连接后，分别以合法形状发送begin，以及对固定16字节ID发送finish/status/release；重连旧身份操作复用R04的A2/A3步骤。重复与foreign bundle安装仍拒绝，已有完整bundle不被部分替换。
- 预期结果：正常入口只在完整bundle发布后开放；首次构造后的注入失败使Node ready失败、HTTPS不监听，preparation shutdown report实际`all_joined()`且原lifecycle owner `join_actual()`成功，不以pending=0代签。`start_control_only_for_test`启动的真实认证WSS中，begin/finish/status/release四命令均返回`product_not_ready`、result为null且无uploadId/ACK；重连不能恢复旧上传所有权。重复/foreign安装失败时上传、队列读取及队列管理三条路径继续使用原bundle。
