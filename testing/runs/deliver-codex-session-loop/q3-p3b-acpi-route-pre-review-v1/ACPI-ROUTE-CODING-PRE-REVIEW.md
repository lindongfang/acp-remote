# P3-B ACPI 输入上传真实入口：编码前审查

日期：2026-11-08  
审查类型：独立编码前审查（只读）  
结论：**阻断真实入口编码，暂不放行**

本审查只评估已有 ACPI 编解码和 Node 上传 owner 如何接入已认证管理 WebSocket。没有修改 D 盘产品源码，没有运行 Cargo、测试、构建、网络访问或 npm audit。产品行为以当前 `deliver-codex-session-loop` spec、`implementation-contract.md`、既有管理协议实现和 Node owner 实现为依据。

## 结论依据

已有 E1 审查 `testing/runs/deliver-codex-session-loop/q3-wss-e1-final-limited-review/REVIEW.md` 的范围是 **Rust E1 finite codec/buffer only**，结论明确为 `PASS-finite-data-component`，并保留 `E1-OPEN001 production consumer absent`。同目录的 `2026-11-02-q003-wss-e1-final-limited-review.json` 明确记录 `realWss: false`、`rootInstalled: false`。因此它只能作为 codec/buffer 的复用依据，不能授权本次真实 WSS 路由、ACK writer 或命令接线。

当前独立路由盘点 `session-loop-q003-p3-authenticated-input-route-preparation-v1/ROUTE-INVENTORY.json` 也记录了 `implemented: false`、`compiled: false`、`verifiedFunctionality: false`，并明确要求真实连接 owner、唯一 writer、取消和关闭顺序先于实现收敛。

## 当前接口证据

以下 SHA 是本审查读取到的当前 Root 文件快照；它们是证据指纹，不代表产品已经通过验证。

| 文件 | 关键事实 | 当前 SHA-256 |
| --- | --- | --- |
| `app/src/management_ws.rs` | `receive_frame` 将所有 Binary 作为 `ClientFrame::Shared`；认证循环随后无条件调用 `attachment.control`；`QueuedMessage` 只有 `Text`/`Shared`，writer 没有独立 Binary 分支。 | `28dbdb00628fb8a7ed9f58e22db3c22629641ac72c2f97002f827b51daa2d275` |
| `app/src/management_ws/session_commands.rs` | `start` 只实现 `InputQueue` 与 `InputRequest`；其它已解析 action 返回 `platform_unimplemented`。 | `98d0408b8d9451a8cd64b0220d03f446f07108a0c8eededf1f1c1414fae49e61` |
| `app/src/management_ws/input_upload.rs` | 已有 `InputData::decode_client` 与 32 字节 `InputAck::encode`；ACPI Data 是方向 1，Ack 是方向 2，payload 为 1..16384 字节。 | `e313840d219a5bc2cebdc9e6a3cd2b6eff69331a85caf599baa91140beace53e` |
| `app/src/node/input_upload.rs` | 已有 `UploadOwner::begin/accept_data/finish/status/release`，并在 `NodeInputUploads::open` 中按 SocketViewAuthority 和 SocketLifetime 建立 owner。 | `768ee1415cc9aac6f801c70441be499608c7187b9f7aaa239581f6f11d528d48` |
| `app/src/node/session_services.rs` | `OwnedNodeSessionServices` 已持有 `Arc<NodeInputUploads>`，但字段为 `pub(super)`，管理 WS 没有生产 service handle。 | `6ea4ae3573a82d699056c70669016540439f0562ff2daeae88559f4bc4cf6cc4` |
| `app/src/node/lifecycle.rs` | `install_session_services` 目前只执行 `management.install_queue_reads(services.queue_reads.clone())`；没有上传 service 安装。 | `0e37ee266d950f9cb2fc6fac328a6bc60dddf5a391ec17ae3f10b58d3bdd4178` |
| `app/src/node/session_services/queue_read_development_tests.rs` | 真实 TLS/WSS fixture 已覆盖 queue read；目前明确断言 `input.upload.begin` 返回 `platform_unimplemented`。 | `75a614b06b22b07da154dfeacf18446cf6e1aa72dad18a84bfbe7debffcf150d` |

NodeRuntime 的现有 `input_upload_owner(client)` getter（`app/src/node.rs`）只能在持有 NodeRuntime 的路径使用。`ManagementState` 当前只保存 `NodeQueueReadService` 的 `OnceLock`，所以不能通过重新创建 runtime、storage、registry 或 ACP owner 来绕开安装缺口。

## 候选最小写集

候选实现必须在 D1 管理 WS 变更之后重新基于当前文件生成精确 patch，禁止整文件覆盖。建议写集如下：

1. `app/src/management_ws.rs`
   - 在 `ManagementState` 增加一次性安装的 Node input-upload service handle，并添加与 queue-read 相同的 SocketViewAuthority/单次安装/foreign-scope 检查。
   - 在认证成功建立 `SocketAttachment` 时为当前 `SocketViewClient` 打开并持有唯一 `UploadOwner`；owner 必须跟随 attachment/lifetime，不能每条命令临时 `open` 后丢掉强引用。
   - 把二进制输入分类为 ACPI Data 与现有共享视图控制帧：只有完整 ACPI Data 头才进入 upload owner；其它非 ACPI 二进制继续走 `attachment.control`，以保持现有 ACPV 控制行为。
   - 为出站队列增加明确的 owned Binary 消息变体和 32 字节 permit 计费；唯一 writer 负责 `Message::Binary`，不能直接从 reader 使用第二 sink。
   - 二进制 ACK 必须在 `accept_data` 成功返回后才排队，并携带 owner 返回的累计 `next_offset`。ACK 只证明上传缓冲接收，不证明正文校验、队列登记、Harness 派发或 socket flush。

2. `app/src/management_ws/session_commands.rs`
   - 接通已有 `InputUploadBegin/Finish/Status/Release`，使用 attachment 中同一 owner 和同一 authenticated `SocketViewClient`。
   - 由于这些方法是 async，必须沿用当前连接拥有的 `PendingReads`/受监督命令任务模型，明确 observer 取消不取消 Node owner 的已接管正文，也不能让 reader 被长时间 SQL/准备工作阻塞。
   - 结果只投影已存在的 owner 事实：begin 返回 `uploadId`；finish 返回进入准备阶段的 `uploadId`；status 返回 `phase`、十进制字符串 `nextOffset`、可选 `bodyId`、可选稳定错误码；release 返回空成功结果。不能把 begin/finish/ACK 表述为 queue accepted 或 execution success。
   - `UploadError::code()` 是现有映射基础：`invalid_request`、`invalid_target`、`invalid_upload_state`、`upload_conflict`、`body_unavailable`、`allocation_failed`、`product_not_ready`、`necessary_storage_unavailable` 等均须保持稳定。

3. `app/src/node/session_services.rs`、`app/src/node.rs`、`app/src/node/lifecycle.rs`
   - 选择并实现一个最小 crate-private service facade/re-export，使管理 WS 可以安装现有 `NodeInputUploads`，但不创建第二份 authority、storage、queue、catalog 或 ACP runtime。
   - 在生命周期 service 安装的同一阶段注册上传 service；安装失败必须阻止正常 product ready，节点停止时先 seal owner 再结束连接。
   - facade 必须保留 `NodeInputUploads::open` 对 SocketViewAuthority、SocketLifetime 和 owner registry 的现有检查。

4. `app/src/node/session_services/queue_read_development_tests.rs` 或同目录新增受控测试叶
   - 只添加真实 WSS/Node fixture 的上传路由验收，不修改 P3-A connection owner 写集和 Source/SQL 写集。

不建议把 `app/src/management_ws/input_upload.rs` 或 `app/src/node/input_upload.rs` 放入首个接线写集；两者已有 codec/owner 事实，只有在新 facade 需要最小可见性调整时才允许精确增加可见性或稳定 getter。任何修改必须由新的 coding gate 明确列出。

## 协议和关闭语义

### 命令

- `input.upload.begin`：输入 `targetId`、`purpose`（`submit`/`edit`），由当前 actor 和 target guard 授权；成功返回新 `uploadId`，不表示已入队。
- `input.upload.finish`：输入 `uploadId`、十进制 `totalBytes`；只有累计 offset 相等且 owner 进入准备阶段时成功。它只表示正文交给既有 preparation owner，不表示 body 已 ready 或已登记。
- `input.upload.status`：返回 owner 当前 phase（`receiving`、`preparing`、`ready`、`failed`）、`nextOffset`、可选 `bodyId`、可选错误码。Ready 只表示可供后续 input register/consume 使用。
- `input.upload.release`：释放当前 owner 的上传条目；重复或未知 ID 必须按 `UploadError` 映射，不静默成功。

### 二进制帧和 ACK

- 客户端 Data 帧固定 `ACPI`、version 1、direction 1、保留字节为零、16 字节 upload ID、BE u64 offset、非空 payload，payload 最大 16384。
- 服务端只在 `accept_data` 成功后生成方向 2 的 32 字节 ACK，并使用 owner 返回的累计 offset。重复的同 offset/同 payload 可返回同一累计 ACK；冲突不得推进 offset 或伪造 ACK。
- 当前管理 transport 的单帧上限为 32 KiB；它足以承载 32 字节头加 16384 字节 payload，但实现必须保留现有 4429 rate-limit 行为。

### 错误和关闭

- 文本命令解析错误继续返回结构化 `invalid_request`；owner 错误必须使用现有稳定错误码，不能降级为 `platform_unimplemented`、空结果或 harness unsupported。
- 无 request ID 的二进制 Data 错误不能伪造 JSON result。Malformed ACPI、方向错误、未知 owner、offset conflict、已释放状态或节点停止必须走明确的 frame-level close/error 方案；建议沿用 4400 `invalid frame`（具体 reason 需在 coding gate 固定），不得 ACK 错误帧。
- 超过现有传输上限继续使用 4429 `limit exceeded`；认证失效仍由当前唯一 writer 发出 4410 revoked；节点停止仍使用 1001 going away。二进制 ACK 和文本结果都必须经过同一 writer 的关闭顺序和 backpressure。
- 连接断开时，未交接的 owner buffer 必须由 SocketLifetime participant 释放；已经交给 body preparation 的正文不得因 reader observer 取消而被重复释放或重放。

## 测试映射

| 编号 | 必须覆盖的真实行为 | 通过条件 |
| --- | --- | --- |
| ACPI-R01 | 已认证 WSS：begin → Data → Binary ACK | ACK 精确 32 字节、ID 相同、offset 为 owner 返回累计值；只走唯一 writer |
| ACPI-R02 | 同块重试与 offset conflict | 同块同 offset 幂等 ACK；不同 payload 或跳 offset 返回明确错误且 offset 不前移 |
| ACPI-R03 | finish/status 生命周期 | Receiving → Preparing → Ready/Failed，status 字段和错误码与 owner 事实一致 |
| ACPI-R04 | release、未知 ID、第二客户端使用第一客户端 ID | 成功释放；未知/foreign owner 被拒绝，不泄漏正文或 ACK |
| ACPI-R05 | malformed/反向/超长 ACPI 与 ACPV 控制二进制并存 | ACPI 错误不进入 `attachment.control`；合法 ACPV 控制行为不回归；超限仍 4429 |
| ACPI-R06 | NodeStopping、SocketLifetime seal、reader observer 取消 | 命令返回 `product_not_ready` 或 1001；owner 由原连接/lifetime 负责释放；无 detached upload task |
| ACPI-R07 | ACK 与后续文本命令排序及 writer 背压 | ACK 和 JSON result 保持同一 outbound 顺序；队列满时不直接写第二 sink，按现有关闭语义结束 |
| ACPI-R08 | 认证/撤销和双客户端共享 Node 资源 | 当前 actor gate、SocketViewAuthority、target guard 均生效；不因 upload ID 的随机关联值授予权限 |

现有 `app/src/node/input_upload/development_tests.rs` 可继续作为 owner 级事实测试，但不能替代 ACPI-R01～R08 的真实 WSS 验收。既有 E1 的 7 个独立 codec 用例和 4 个开发 codec 用例也不能代签生产 consumer。

## 放行条件与当前判定

在以下材料获得新的独立编码前审查通过前，本范围**阻断**：

1. 固定 `NodeInputUploads` 的生产 facade/re-export 及生命周期安装 API，逐项说明 authority、lifetime、seal、单次安装和节点停止行为。
2. 固定 `SocketAttachment` 中 owner 强引用和连接关闭顺序，说明命令 future、reader observer、Node body preparation 的取消/接管边界。
3. 固定 `QueuedMessage::Binary` 的 permit 计费、唯一 writer、actor gate、ACK 与文本结果排序及 4400/4429/4410/1001 语义。
4. 固定四个文本命令的 JSON result schema 和全部 `UploadError` 映射，确认不引入新的 capability gate 或伪造 harness 能力。
5. 提交与当前 D1/P3-A 不重叠的精确候选 diff、源成员清单和 ACPI-R01～R08 独立测试定义；随后才允许 coder 在独占工作区实现。

当前判定不是产品缺陷关闭，也不是完整 P3-B 完成声明；它只记录真实 ACPI consumer 尚未获编码授权。

