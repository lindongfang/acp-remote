# Tasks

## 1. core 接缝（WP1）

- [x] 1.1 在 `crates/core/src/use_cases.rs` 新增 Sync 设备的未认证窄只读入口（形如 `sync_device_handshake_view(device_id)`），回传该 deviceId 的信任行、公钥、最近一次配对、本机 head 与 epoch；入口 MUST NOT 放开既有的 `require_local` 守卫（`devices`/`device`/`profiles`/`workspaces` 的可见性不变）。完成判据：新增单元测试断言「未知 deviceId 也返回结构完整的视图（公钥为 `None`）」「该入口不返回任何其它设备」。
- [x] 1.2 在 `use_cases.rs` 新增设备认证收尾入口（形如 `record_device_connected(device_id)`），随写集推进 `owned_device.last_seen_at` 并提交一条 `device.authenticated` 审计；`last_seen_at` MUST 只前进（旧值为空时写入、新值为空时保留、两者非空取较大者）。完成判据：单元测试断言只前进语义的三条分支与审计动作取值。
- [x] 1.3 新增「自有 + 已导入合并且按 scope 过滤」的会话清单读面（形如 `sync_visible_sessions(actor, query)`）：自有会话按 `session.list` 授权、已导入会话额外要求协商 `resource.remote-origin.v1` 的判定由调用方传入；每个条目的 `workspace` 引用 MUST 取持久化别名，MUST NOT 由路径反查。完成判据：单元测试断言「两类来源合并后条数正确」「不允许读取的会话不出现」「别名取自持久化值」。
- [x] 1.4 core 侧自检与文档：确认 `crates/core` 的普通依赖闭包未新增 crate（`AGENTS.md` §10 的 core 依赖白名单不变），并在 `use_cases.rs` 的新入口上写明「为何在 `require_local` 之外」的理由注释。完成判据：`cargo tree -p core --edges normal` 的 crate 名集合与改动前一致。
- [ ] 1.5 新增设备认证失败的审计入口（形如 `record_device_auth_failure(device_id)`）：动作取 `AuditAction::DeviceAuthFailed`、目标为该设备、actor 归因取持久化的设备身份；入口 MUST 在设备**未知**时也能留痕（这正是 `identity-auth` 要求调用方落这一行的场合，见 `IDENTITY_AND_AUTH_CONTRACT.md` 关于 `HandshakeFailure::audit()` 的条款），且 MUST NOT 因未知设备而写出一条能被用于探测存在性的响应差异。完成判据：单元测试覆盖「已知设备的失败留痕」「未知设备的失败同样留痕且 actor 归因不取自握手自报值」。**背景**：既有 `record_node_link_auth` 只接受三个 node 动作（`NodeAuthenticated`/`NodeAuthFailed`/`AuthorizationDenied`），因此 Sync 侧的认证失败当前无处落审计 —— 这是本变更的缺口，由 WP3 的检视发现。

## 2. 接入层来源透传（WP2）

- [x] 2.1 扩展 `crates/server/src/transport/net/` 的 `WsHandler::handle`，增加一个「已校验的请求来源」参数：从升级请求的 `Origin` 头取值并按 §3.2 归一化（小写 scheme、规范化 hostname、显式有效 port、省略默认端口、不含 path/query/fragment 与尾部 `/`）；实现取**三形态互不折叠**的取值（`Known`／头缺席的 `Absent`／头存在但不可归一化的失败关闭），而非「无该头时取 `None`」。完成判据：`transport/net` 的单元测试覆盖三种取值形态（含空值与缺席的区分）。
- [x] 2.2 同步改造**四个**既有实现（`grep -rn "impl WsHandler" crates/` 的四处：`node_link/conn/session.rs` 的 `WsEndpoint` 与 `transport/net/tests.rs` 的 `EchoWsHandler`／`CountingWsHandler`／`IdleWsHandler`；Node Link 不依赖来源，取到值后忽略即可，但必须如实接收）。完成判据：`cargo test -p server` 全绿，且 Node Link 的既有用例无一改动期望值。
- [x] 2.3 在 `transport/net` 的模块文档里写明「来源是协议层概念，`Host` 是虚拟主机概念，两者不可互相替代」，并把归一化口径与 `SYNC_PROTOCOL.md` §3.2 逐条对应。完成判据：`npm run check:docs`。

## 3. Sync 连接面（WP3）

- [ ] 3.1 新建 `crates/server/src/sync/` 与 `crates/server/src/sync/conn/`，在 `crates/server/src/lib.rs` 注册 `pub mod sync;`；模块文档写明它只调用 `core::use_cases` 与 `identity-auth`，不 import `storage-sqlite`、不与 `server::node_link` 互相调用。完成判据：`npm run check:boundaries` 通过。
- [ ] 3.2 定义 `/sync/v1` 的 path 与 subprotocol 常量（`acp-remote.sync.v1.json`，取自 `compatibility/errors/v1/errors.json`），并实现 WS 升级后的会话骨架：只接受 text 帧、binary 帧关闭连接。完成判据：单元测试覆盖「binary 帧被拒绝且不进入协议逻辑」。
- [ ] 3.3 实现四消息握手：`auth.client_hello` → `auth.server_challenge` → `auth.client_proof` → `auth.authenticated`，经 `identity-auth` 的 `SyncDevice` 入口完成双向 challenge-response；未知设备 MUST 照常签发结构完整的挑战（不得用错误类别区分是否存在）；认证完成前只接受 hello 与 proof，收到业务消息以 `4401` 关闭；整个认证 MUST 在 15 秒内完成，超时以 `4408` 关闭。完成判据：单元测试覆盖「正常握手」「认证前业务消息 → 4401」「超时 → 4408」「未知设备照常拿到挑战且 proof 阶段失败为 `auth.proof_invalid`」。
- [ ] 3.4 在握手中接入已验证来源：来源与设备记录绑定逐字比较，不一致时以 `auth.origin_mismatch` 拒绝；transcript 的 `canonicalOrigin` 字段 MUST 取服务端判定值。**无来源（`Absent`）与非 `https://` 来源都走拒绝路径**（`identity-auth` 对 `SyncDevice` 强制要求 `canonical_origin` 为 `Some`，且 `CanonicalOrigin::parse` 只收 `https://`），MUST NOT 以空字节串放行。完成判据：单元测试覆盖「来源一致通过」「来源不一致拒绝」「无来源拒绝」「非 https 来源拒绝」。
- [ ] 3.5 实现连接注册表（`(hostId, deviceId)` → 连接句柄的有界映射）与出站账本：待发送队列双上限（8 MiB 或 2000 条，先到者触发），达到高水位时回 `SendFault` 供投递层停读新的重放批次；出站 `connectionSequence` 的分配与入队在同一临界区，被高水位拒绝的投递 MUST NOT 消耗序号。注册表 MUST NOT 被当作事实来源（授权判定仍读持久记录）。完成判据：单元测试覆盖双上限触发、被拒投递不消耗序号、注册表查询不返回已撤销设备。
- [ ] 3.6 实现单设备替换：新连接认证成功后以 `4411` 关闭旧连接，旧连接关闭 MUST NOT 撤销设备；设备撤销提交后按 deviceId 关闭全部活动连接（`4410`）并拒绝后续认证。完成判据：单元测试覆盖「新连接顶替旧连接且设备仍可用」「撤销后活动连接收到 4410」「撤销后重新认证被拒」。
- [ ] 3.7 实现心跳与静默超时：按 `heartbeatIntervalMs`（默认 30 秒）发 `control.ping`，校验 `control.pong` 的 nonce 回填；连续 90 秒无入站消息或 pong 时关闭。完成判据：单元测试覆盖「正常回应保持连接」「静默超时关闭」。
- [ ] 3.8 实现 close code 映射与协议错误下发：按 `errors.json` 的 Sync close 码表映射 `4400`/`4401`/`4403`/`4406`/`4408`/`4409`/`4410`/`4411`/`4429`/`4500`；错误消息体为 `code`/`message`/`retryable`/`correlationId`/`details` 结构。完成判据：单元测试逐条断言映射表与错误体字段，且 `correlationId` 不出现在日志中的敏感字段里。
- [ ] 3.9 **（WP3 检视 finding 1）** 修「未知 `type`」路径：`Envelope::decode` 把所有 serde 错误包成 `Malformed`，因此未知 `type` 走不到 `UnknownType` 臂、落到 `malformed_inbound`，错误码错（回 `protocol.schema_invalid` 而非 §4.2 要求的 `protocol.type_unsupported`）**且不推进入站期望序号** → 客户端下一条被判 `sequence_invalid`、同样不推进 → 连接退化为只出不进的哑管道直到 90 秒静默超时。修法：让该分支在失败路径上重读 `type` 以区分「词表外」与「schema 非法」（同包对 `protocolVersion` 已有同款做法），并把**入站期望序号按对端已发出的序号照常推进**（§4.1：序号描述对端已发出的消息，本机是否接受不影响它已被消耗）。完成判据：新增用例覆盖「未知 type → `protocol.type_unsupported`」「其后一条序号正常递增的合法消息 MUST NOT 被判序号错误」，且第二条断言在回退修法时变红。
- [ ] 3.10 **（WP3 检视 finding 2）** 收紧慢消费者宽限的计时口径：现实现「占用回落即清除饱和计时」会让**持续抖动**的消费者永远攒不满宽限（任何一次排空到上限之下都把计时清零），因此永不断开。修法：计时只在「确实脱离饱和并保持脱离」时清除，或改为累计/取窗口最大值 —— 由实现者选定并在模块文档里写明为什么该口径与 §14 的「**持续**过慢则断开」一致。完成判据：单元测试覆盖「瞬时突发后回落 → 不触发断开」「持续在限之上 → 宽限后触发断开」两侧。
- [ ] 3.11 **（WP3 检视补充项）** 补齐 §14 的认证前固定上限与两处口径偏差：① 首条 `auth.client_hello` MUST 在 5 秒内到达（超时 `4408`）；② 首条 hello 的消息体上限 32 KiB；③ 单节点并发 WSS 连接上限 256（超限拒绝新连接）；④ 心跳 `control.ping` 发送遇到高水位时 MUST NOT 直接以 1000 关闭（那是把一级背压当致命错误），应按 §14 的两级口径处理。①②③ 对应新规范「认证前的固定上限与序号推进」。完成判据：每项各有一条用例，且 ①②③ 的取值与 `docs/SYNC_PROTOCOL.md` §14 的表格逐条相等。

## 4. Sync 投递面（WP4）

- [ ] 4.1 实现 `sync.subscribe` 的 cursor 判定：`null` 走首次同步；格式非法、epoch 不匹配、超出 head、早于保留窗口分别回 `sync.cursor_invalid` 且 `details.reason` 取四个可区分取值；MUST NOT 钳制到 head。完成判据：单元测试逐条覆盖四种 reason，并断言越界 cursor 不会按 head 继续投递。
- [ ] 4.2 实现 `sync.reset_required` 的主动下发与四种 `reason`（`initial_sync`/`epoch_mismatch`/`cursor_expired`/`cache_incompatible`）。完成判据：单元测试覆盖四种 reason 的下发条件。
- [ ] 4.3 实现增量重放：从 cursor 之后按全局序号升序投递可见事件，与实时投递建立 barrier（重放期间的新事件排在 `sync.caught_up` 之后）；按设备 scope 过滤；权限过滤造成的序号空洞 MUST 合法且不报错。完成判据：单元测试覆盖「按序投递」「重放期间实时事件不越过屏障」「被过滤的空洞不报错且屏障游标高于该段」。
- [ ] 4.4 实现快照组装：`sessions` 恒发一个 chunk（空集合也发空数组），`workspaces`/`agents` 受 `core.local-catalog.v1` 门控（未协商时不下发、不以空数组占位），`chunkCount` 与实际块数一致且取值在 1..=3；会话明细资源 MUST NOT 进入快照。完成判据：单元测试覆盖「协商目录 feature → 3 块」「未协商 → 1 块且无目录资源」「空会话集合 → 1 块」「明细资源不出现在快照」。
- [ ] 4.5 实现 `snapshotDigest`：对每条 chunk **原始 UTF-8 文本字节**分别求 SHA-256，按 `chunkIndex` 顺序连接后再求 SHA-256，结果按无填充 base64url。完成判据：单元测试用收到的帧文本独立复算摘要并与声明值逐字节比对；另加反例断言「按重新序列化字节计算会得到不同值」。
- [ ] 4.6 在快照 `sessions` 资源中投影设备可见清单：自有与已导入会话合并、按 scope 过滤、已导入会话受 `resource.remote-origin.v1` 门控、`origin` 字段区分来源并携带来源标识与可达性。完成判据：单元测试覆盖「两类来源共存」「未协商来源 feature 时外来会话被过滤且无占位」「无权会话不出现」。
- [ ] 4.7 实现 `sync.ack`：只前进、回退被忽略并记诊断、超过本连接已发送最高游标时回 `sync.cursor_invalid`；ACK MUST NOT 推进事件库 head；服务端保存的设备 ACK 最大值 MUST NOT 被用来决定新连接的恢复位置。完成判据：单元测试覆盖「屏障 ACK 被接受」「回退被忽略且诊断可见」「超 head 回 `sync.cursor_invalid`」「以更旧 cursor 重订阅时从该 cursor 起投，而非从服务端最大 ACK 起投」。

## 5. Sync 配对 HTTP（WP5）

- [ ] 5.1 实现 `POST /sync/v1/pairing/claim` 的原子校验：pairing 存在、未过期、仍为 `created`、来源与 Host 与配对绑定一致、`pairing-proof` 域 HMAC 正确；任一项失败 MUST NOT 写状态。完成判据：单元测试覆盖五个校验点各自的失败分支均不改变配对状态与信任记录。
- [ ] 5.2 实现 claim 的一次性与幂等：已认领后同一 `pairingId` 再次 claim MUST 被拒；内容完全相同的重复 claim 返回首次结果，内容不同返回冲突。完成判据：单元测试覆盖「第二次相同内容 → 幂等返回原结果」「第二次不同内容 → 冲突」。
- [ ] 5.3 实现 `POST /sync/v1/pairing/status`：校验 `pairing-status` 域 HMAC（无效 → 401），校验通过后一律 200，业务状态只由响应体表达（五值封闭词表）；仅 `approved` 返回设备非秘密元数据、授予 scope 与节点身份材料；本机已不持有配对秘密时终态仍按 200 返回。完成判据：单元测试逐条覆盖五种状态、proof 无效的 401、非 approved 状态下不出现设备与主机字段。
- [ ] 5.4 实现四个安全响应头（`Cache-Control: no-store`、`Pragma: no-cache`、`Referrer-Policy: no-referrer`、`X-Content-Type-Options: nosniff`）并确保它们同样出现在接入层预处理响应上（413 请求体超限、405 方法不匹配、Host 校验失败）。完成判据：单元测试断言 200/400/401/403/404/409/410/413/429 各响应都带这四个头。
- [ ] 5.5 实现请求边界与限流：只接受同源 `application/json`、请求体有上限、不用 Cookie/Basic Auth/query 承载凭据；claim 每 IP 10 次/分钟、status 每 pairingId 60 次/分钟，超限 429；计数以真实连接地址为准（不可信转发头不生效）；证明校验失败累计 5 次使配对失效。完成判据：单元测试覆盖「超限 429」「伪造转发头不绕过限流」「5 次失败后配对失效且不可认领」。
- [ ] 5.6 敏感信息边界：确认 pairing secret、HMAC 输入与完整认证载荷不出现在日志、错误消息与测试快照中。完成判据：单元测试以固定 secret 驱动失败路径，断言生成的结构化日志与错误消息文本不含该 secret 的任何字面量。

## 6. 组合根接线（WP6）

- [ ] 6.1 在 `crates/app/src/daemon.rs` 注册 `/sync/v1`（WS，subprotocol 常量）与两条 pairing POST（带默认安全响应头），并在 `crates/app/src/compose.rs` 装配 `server::sync` 所需的依赖。完成判据：`cargo test -p app` 中的监听级用例断言「`/sync/v1` 带 Sync subprotocol 升级成功」「带 Node Link subprotocol 升级被拒」「两条 pairing path 进入 Sync 逻辑，不再返回 404」。
- [ ] 6.2 为 `EventPublisher` 增加 Sync 扇出分支（现有 `ForkedPublisher` 只扇给 Node Link）：投递时按各连接的 scope 与已协商 feature 过滤，队列沿用 WP3 的有界出站账本，达到高水位时停读新的重放批次。完成判据：`cargo test -p app` 断言「同一事件在不同 scope 的连接上可见性不同」「高水位时投递层被告知停止读取」，并断言 Broker 的提交路径不因慢客户端被阻塞。
- [ ] 6.3 接线 `sync.*` 三个配置键：`sync.max_message_bytes`/`sync.max_prompt_bytes`/`sync.max_replay_events_per_batch` 从解析变为消费，取值**只下调**（高于协议默认值时取默认值），并同时把它们从 `Config::unwired` 移除。完成判据：`crates/app/src/config.rs` 既有的「已接线键不得再报未接线」自检覆盖这三个键；新增用例断言下调生效、上调被忽略。
- [ ] 6.4 把 `close_device` 的撤销钩子接到真实 Sync 连接表（今天恒报「没有可关闭的连接」）。完成判据：`cargo test -p app` 断言撤销后活动连接被关闭且后续认证被拒。

## 7. 文档同步（WP7）

- [ ] 7.1 更新 `docs/SYNC_PROTOCOL.md`：§3.1/§7/§8/§9 按本次实绩写回（端点已可用、认证与投递已落地），并保持「计划」类措辞只用于仍未实现的部分。完成判据：`npm run check:docs`。
- [ ] 7.2 更新 `docs/CONFIG_REFERENCE.md` §2：撤下「三个 `sync.*` 键未接线」的注记并写明取值口径只下调。完成判据：`npm run check:docs`。
- [ ] 7.3 修正文档冲突：把 `docs/FRONTEND_DESIGN.md` §4.3 与 `docs/INITIAL_DESIGN.md` §6.3 中的 `/sync` 改为 `/sync/v1`，并注明以 wire 权威为准（`AGENTS.md` §9）。完成判据：全仓 `grep` 不再出现把 Sync WebSocket 端点写作 `/sync`（不带 `/v1`）的表述；`npm run check:docs`。
- [ ] 7.4 更新 `docs/MODULE_ARCHITECTURE.md`（§3 状态行、§3/§4.9 的 `[现状]`）与 `docs/SECURITY_DESIGN.md`（Origin 校验与撤销传播的实绩）与 `README.md`（仓库当前状态表、`server` 行、尚未开始清单）。完成判据：`npm run check:docs`、`npm run check:drift`。

## 8. 协议级端到端验收（WP8）

- [ ] 8.1 在 `crates/app/tests/support/sync.rs` 实现脚本化 fake Sync 客户端（经真实 WSS 连接到真实 loopback listener），支持四消息握手、订阅、快照接收与摘要复算、ACK 发送；在 `crates/app/tests/support/mod.rs` 登记模块。完成判据：客户端能对真实 listener 完成一次完整认证并收到 `auth.authenticated`（在该用例自身运行时可见）。
- [ ] 8.2 在 `crates/app/tests/sync_e2e.rs` 实现主链路：真实 SQLite + 真实 `Authority` + 真实本机管理通道完成「配对 claim → 本机确认 → status approved」；随后设备侧完成握手 → 订阅 → 快照（三类资源齐备）→ 快照摘要按收到的原始帧独立复算并比对 → `sync.caught_up` → ACK。完成判据：用例断言各阶段的可观察结果，且摘要复算与声明值逐字节相同。
- [ ] 8.3 在同一文件中覆盖边界：撤销设备后连接收到 `4410` 且后续认证被拒；新连接顶替旧连接时旧连接收到 `4411`；以错误 epoch、超 head、早于保留窗口三种 cursor 订阅分别得到三种 `details.reason`；以更旧 cursor 重订阅时投递从该 cursor 起。完成判据：四条边界各有独立断言，且每条都能在实现错误时变红（不得只断言「未抛错」）。
- [ ] 8.4 在报告里如实登记隔离区的限制：WP8 交付的是用例代码，真实运行由主 Agent 在合入后按 PV5 执行；不得声称已在隔离区通过。完成判据：`agent://<id>` 的报告含该说明与作者在隔离区能做的核对结果。
