# Design: node-link-owner

## Context

现状与约束（动机见 proposal.md 的 Why，此处只列影响方案的事实）：

- `crates/server` 已有 `transport::local`（本地通道）与 `local_admin`；`server::node_link` 不存在，`server::transport` 没有网络部分。`local_admin::pairing` 已有 `ConnectionCloser` 装配缝（`close_device`，组合根注入，当前实现为 `NoConnections`）。
- `crates/node-link-protocol` 已实现 v1 全部 29 个消息类型的类型化 body、信封分派、配对 HTTPS 载荷与 transcript domain/tag 表；它经 `common.rs` 再导出 `acpr-wire` 的 wire 值对象，但**不**再导出 `acpr_wire::cj1`（ACPR-CJ1 规范 JSON，`payloadDigest`/`snapshotDigest` 的前像唯一实现）。
- `identity-auth` 的 `Authority` 提供 NodeLink 握手三入口（`hello`/`verify_proof`/`complete_auth`）与配对状态机（`verify_claim`/`settle`/`pairing_sas`），均为纯状态机；验签公钥只来自调用方传入的 `PeerTrust` 快照（持久化信任的当次读取）。
- core 的 broker 已保证「先持久化后发布」（`SessionStore` 提交成功后 `EventPublisher::publish(CommittedDelivery)`）；`UseCases` 已有 `submit_command`/`create_session`/`command_status`/`replay`/`read_session` 与 Export/信任管理族。
- `app` 已解析 `daemon.listen`/`daemon.tls.*`/`daemon.allowed_hosts`/`daemon.trusted_proxies` 但记入 unwired 清单；Daemon 启动/关闭序列与 `daemon.status` 的 `listen` 恒空。
- 用户裁决（2026-09-26）：TLS 两种模式都接线；配对 HTTP 端点纳入本切片。
- 硬约束：`unsafe_code = "forbid"`、`rust-version = 1.85`、依赖矩阵（§5）、封闭词表单一定义、`npm run check` 门禁。

## Goals / Non-Goals

**Goals:**

- `server::transport::net`：共享 HTTP/WSS listener、path 路由、Host/代理头边界、TLS `proxy`/`direct` 两种模式、连接级上限。
- `server::node_link`：配对 HTTP（claim/status）与 WSS 服务端（握手、catalog、resource、command、控制、撤销传播），行为以三份增量规范为准。
- `app` 组合根接线：listener 纳入启动/关闭序列与 `daemon.status`。

**Non-Goals:**

- 切片 6/7 的任何内容（`node-link-client`、`acp_facade`、`sync`、PWA）；四个 post_mvp 消息的实现；证书热轮换；catalog 增量（`catalog.changed`）；Linux 产品交付。
- 不修改 wire 合同资产与封闭词表；不改 `identity-auth` 状态机语义；不改 core 端口语义方向。

## Decisions

### D1 HTTP/WS/TLS 栈选型：axum + tokio-rustls（核验后登记）

候选栈：**axum 0.8**（HTTP 路由 + `axum::extract::ws` 的 WebSocket，底层 tokio-tungstenite）+ **tokio-rustls / rustls 0.23**（`direct` 模式）+ PEM 解析（原列 `rustls-pemfile 2`；WP1 核验发现其上游已归档，**实际改用 `rustls-pki-types` 的 `pem` 模块**——它本就是 rustls 的传递依赖，不新增供应商；核验证据 `reports/wp1-deps.log`）。理由：

- axum 是 Tokio 生态维护最活跃的 HTTP 栈之一，WS 支持成熟；workspace 已有 Tokio 事实标准，不引入第二个运行时。
- rustls 是纯 Rust TLS，满足 `unsafe_code = "forbid"` 与「使用经过审查的实现」的要求；无原生依赖，Windows/Linux CI 行为一致。
- axum 的 WebSocket 默认不启用 `permessage-deflate`（需显式 feature），天然满足 §2.1「协商到压缩必须拒绝」；subprotocol 白名单在 upgrade 时显式校验。

**核验任务（实现第一批次执行并留证）**：按 `SECURITY_DESIGN.md` §20 四判据核验 axum/tokio-tungstenite/rustls/tokio-rustls/rustls-pki-types 的语义适配、许可证（须落在 `deny.toml` allow 内）、MSRV ≤ 1.85 与维护状态；结论与证据写入 `reports/wp1-deps.log`。（WP1 已执行：provider 定为 rustls 自带 `ring`，落选 `aws-lc-rs` 与 `rustls-rustcrypto`，理由见 Check Plan Changes 与 `reports/wp1-deps.log`。）**核验失败时不擅自换约束**：回到用户决策（换栈或本切片只接线 `proxy`）。替代候选（如 `axum-server` 托管 TLS）在核验记录中一并给出取舍。

### D2 模块划分：transport::net 承载连接级规则，node_link 承载协议语义

```text
server::transport::net      listener 绑定、TLS 终止（proxy/direct）、Host/代理头边界、
                            path 路由、请求体上限、WS upgrade 规则（subprotocol/拒绝压缩）
server::node_link::pairing  claim/status HTTP 处理器（调用 identity-auth 配对状态机 + core 用例）
server::node_link::conn     连接生命周期：握手状态机驱动、信封/序号校验、心跳/超时、发送队列
server::node_link::catalog  catalog.subscribe/snapshot 投影
server::node_link::resource attach/generation 注册表、snapshot、event 扇出、ack
server::node_link::command  command 管线（授权、幂等、派发、终态映射、限流）
```

- `transport::net` 不含任何 Node Link 业务语义（与 `transport::local` 同层级：连接与字节/帧规则）；`node_link` 各模块只经 `core::use_cases` 与 `identity-auth` 入口工作，不查询 SQLite、不调用其他 adapter。
- wire/core mapper 位于 `server::node_link` 内部（§4.4：Node Link wire ↔ core 的映射属于本 adapter）；`acp-protocol` 只在需要构造/保真 `payload.acp` 时出现。
- `/sync/v1*` 在路由层明确 404，不经过 `node_link`。

### D3 握手接线：每次握手从持久化信任取当次快照

- `node.hello` 解码后：经 core 用例读取该 `accessNodeId` 的信任记录与验签公钥，组装 `PeerTrust`（未知节点 `public_key = None`，`Authority::hello` 照常签发挑战，不泄露存在性）；feature 交集由 adapter 计算（词表来自 `node-link-protocol`/registry）后作为 `negotiated_features` 传入。
- `Authority::hello` 返回挑战与 `host_proof`（`node-link-challenge/v1`）→ `node.challenge`；`node.proof` → `Authority::verify_proof`（消费挑战、核对绑定与快照主体一致性）。
- 凭据状态按 `IDENTITY_AND_AUTH_CONTRACT.md` §5.1 的既定分工：`Revoked`/`Unknown` 不是握手失败，由 adapter 映射为 `nodelink.auth.node_revoked`（4410）/`nodelink.auth.node_unknown` 并关闭；只有签名/绑定/挑战不成立才是 `proof_invalid`。
- `complete_auth` 返回的收尾副作用（配对转 `consumed`、`last_seen`、审计）由 adapter 经 core 写集在**同一事务**提交后才发 `node.ready`；提交失败即关闭连接（失败关闭，不半认证）。
- 认证限流（10 次/分钟/IP）在连接准入处计数（以真实对端地址为准，`PeerInfo.client_ip`）。**实现注记（RV1-WP4-F7 修正，2026-09-26）**：WP2 冻结的 `WsHandler` 形状没有 upgrade 前的按路径准入钩子，实际接线在 upgrade 之后的会话第一步（拒绝 = `link.error(nodelink.resource.rate_limited, retryable=true, details.retryAfterMs)` + 4429）；对端可见效果等价（未认证连接不产生业务副作用）。

### D4 catalog 投影与 revision 来源

- Owner 侧 catalog 从自己的 SQLite Export 记录投影（`NODE_LINK_PROTOCOL.md` §12.3 的既定决定），按该 Access 信任记录的 `exportIds` 过滤；**每次订阅实时投影**，不做进程内缓存副本——撤销因此天然即时生效（D7 的推送只是通知，授权判定始终以持久化记录为准）。
- `catalogRevision`/`snapshot.revision` 需要一个跨重启单调的十进制串。**首选**复用管理存储已有的单调变更水位（审计/变更序号）；实现期先核实 `CORE_PORTS_AND_STORAGE.md` §7 的现有列是否可导出该值（只读使用，不改表结构）。**若不存在可导出水位**：在 core/storage 增补一个管理变更计数器（Export/信任写集提交时 +1），并在同一变更内同步 `CORE_PORTS_AND_STORAGE.md` §5/§7 与漂移门禁——这是本变更唯一可能触及 core/存储合同的点，触及即停下手头实现先同步合同。
- `knownRevision` 非空时仍回完整快照（`catalog.changed` 属 post_mvp）；`export.revoked` 推送不受 revision 机制影响。

### D5 digest 计算：为 `server` 增补 `acpr-wire` 依赖格

`payloadDigest`/`snapshotDigest` 的前像是 ACPR-CJ1（`SYNC_PROTOCOL.md` §3.3/§9.4，`NODE_LINK_PROTOCOL.md` §12.4），其唯一实现位于叶子 crate `acpr-wire::cj1`。`node-link-protocol` 只再导出值对象、不再导出 `cj1`。

- **决策**：`docs/MODULE_ARCHITECTURE.md` §5 矩阵为 `server` 行增补 `acpr-wire` ✓（`check:boundaries` 以该矩阵为判据，文档与门禁同批更新），`server` 只使用 `acpr_wire::cj1` 与摘要计算，不取协议语义。SHA-256 用 `sha2`（workspace 已有，经 `identity-auth` 验证）。
- 曾考虑的替代：经 `node-link-protocol` 再导出 `cj1`——被否，因为这会让协议 crate 承载「计算服务」而非 wire 定义，且 Sync 侧将来同样需要时会出现两个再导出入口。

### D6 事件扇出：组合根注入的 EventPublisher 分叉 + 每会话订阅注册表

- 组合根装配 broker 时使用**分叉 `EventPublisher`**：一路保持现有语义，一路转发给 `server::node_link` 的连接注册表。`resource.event` 只从 `CommittedDelivery::Owned` 映射（imported 投递不跨节点再导出，§4 拓扑规则）。
- 每个连接维护「attachment → (sessionId, generation, 已 ACK 水位)」注册表；事件按 sessionId 匹配活跃 attachment 投递，慢连接进入每连接有界发送队列（高水位停读快照、持续过慢断开，D9）。
- `resource.subscribe(cursor = null)` 的快照数据来自 core 的读视图（`session_meta`/`pending_interactions` 两种 item）；`cursor` 非空走 `UseCases::replay` 的 origin cursor 增量； epoch 由 adapter 核对（不一致 → `sequence_invalid`）。
- 256 KiB 内嵌上限与 `rawAcp.rawUnavailable(size_limit)` 在 mapper 层执行；`payload.view` 的最低字段按 §11.4 共享合同（事件 fixture 复用 `schemas/sync/v1/event-views.schema.json` 的 `$defs` 校验）。
- 命令终态（`command.terminal`）同样先经 core 持久化再由 adapter 发送；`completed` 的非空 `result` 由 adapter 在映射期保证（§12.5 的既定分工）。

### D7 撤销传播：扩展切片 4 的 ConnectionCloser 装配缝 + 逐消息权威复核

- **机制**：把 `server::local_admin::pairing::ConnectionCloser` 扩展为撤销通知缝（新增 `close_node(&NodeId)` 与 `export_revoked(&ExportId)`，或等价的重命名端口）；组合根以 `server::node_link` 连接注册表的句柄实现它，`local_admin` 在 `node.revoke`/`export.revoke` **持久提交成功后**调用——推送/关闭失败只记日志，不回滚已提交的撤销。
- **权威判定不依赖推送**：`node_link` 在处理 `resource.attach`、`command.submit` 与 catalog 订阅时按当次持久化记录复核授权（D4 的实时投影同原则），因此即使推送丢失，已撤销的 Export/节点也无法继续取资源或发命令；推送只负责「立即通知」这一协议义务。**推送范围（窄口径，RV1-WP6-F5）**：`export.revoked` 只推向「持有该 Export 的 attachment/订阅」的活跃连接；只订阅 catalog 未 attach 的连接不在推送面（其下次订阅的实时投影已排除已撤销 Export，命令复核兜底）。
- 该扩展不引入 adapter 间调用：`local_admin` 面向自己定义的端口编程，实现由组合根装配（与切片 4 `close_device` 同一模式）。

### D8 命令管线：边界校验 → 授权交集 → 幂等 → 用例派发 → 终态映射

1. wire DTO 边界完成 schema/长度/枚举校验（closed object；`session.create` 的禁带字段映射为更具体的 `nodelink.command.unsupported_field` 而非通用 `schema_invalid`）。
2. 授权交集：Export grant ∩ 信任记录 grant ∩ capability——**capability 支由 Agent 后端的显式失败承担**（`capability.unsupported_by_*` → 终态 failed），adapter 不做 capability 前置判定；补「capability → 命令」映射前不得声称已做三方交集（RV1-WP6-F7）。越权 → `command.rejected(nodelink.export.not_granted)`，可带 `details.parameter`。
3. 幂等：`(ownerNodeId, accessNodeId, requestId)` 经 core 的幂等记录判定；重复返回首次结果，语义冲突 → `idempotency_conflict`。
4. 派发：查询命令同步完成（结果放 `accepted.result` 或直接 `terminal`）；mutation 同步接受、终态由 core 的持久化 terminal event 产生；崩溃窗口由 core 进 `uncertain`，adapter 原样透传。
5. `command.status` 两种形式（独立消息 / `command.submit{command:"command.status"}`）在 adapter 归一到同一处理器：已终结 mutation → `command.terminal`（重查回复的 `terminalEventId` 非空）；**未终结的 mutation → 同形 `command.accepted`**（terminal 词表无 `accepted`，不能伪造 `uncertain`——Q1(a)）；**查询命令的 requestId 不落持久记录，重查回 `nodelink.command.not_found`**（RV1-WP6-F4）。被接受 mutation 的终态由提交方连接上**有所有者的 watcher**（连接断开即取消）推送 `command.terminal`；崩溃窗口由 core 进 `uncertain`，adapter 原样透传。
6. in-flight 与 120/分钟限流在 conn 层计数；`session.create` 的 `SessionCreateResult` 由 adapter 从 core 创建结果组装（`sessionId` 永远来自 Owner）。

### D9 限流、心跳与慢连接：固定常量 + 内存计数器

- 固定常量（§2.5：握手 15 s、心跳超时 90 s、嵌套 64、字段 1024、数组 10000、认证 10/min/IP、命令 120/min、配对 claim 10/min/IP、status 60/min/pairingId）以常量实现，不出现在配置键里；可下调七项从 `CONFIG_REFERENCE.md` §3 读取并经 `node.ready.limits` 下发。
- 限流用进程内令牌桶/滑动窗口，键分别为对端真实 IP（经 D2 的代理头边界过滤后）、连接、`pairingId`；超限响应按规范（429 / `rate_limited` + `retryAfterMs` / 4429）。
- 每连接发送队列按 `maxPendingQueueBytes`/`maxPendingQueueMessages` 双上限；高水位停读快照批次，持续过慢断开连接（发 `1000` 或协议错误关闭均可，但不得发送 post_mvp 的 `link.backpressure`），Access 凭 cursor 重放补齐。

### D10 TLS direct：rustls 终止 + 启动期一次性加载

- `direct` 模式在 listener 绑定前加载 `cert_path`/`key_path`：文件缺失/不可读/PEM 解析失败/权限不符即拒绝启动；私钥字节只在内存停留于 rustls 配置构建期，不进日志、错误消息、`Debug`。
- 权限检查按 `SECURITY_DESIGN.md` §13.2 的平台机制执行；Unix 路径（权限位）由 Linux CI 覆盖，Windows 形态按同节对应机制核验并在实现报告中留证。
- 不支持热轮换：证书变更经重启生效（`SECURITY_DESIGN.md` §9.5：不改变 Node identity）。
- `proxy` 模式 Daemon 起明文 HTTP/WSS：它只在「同机可信反代终止 TLS」的部署形态下对外可达；`daemon.listen` 为非 loopback 且 `tls.mode = "proxy"` 时启动输出额外告警（明文面暴露在本机之外的风险）。`dev_mode.allow_plaintext` 只对 loopback 生效。

### D11 app 接线：启动/关闭序列与 status

- 启动：恢复管理记录 → 构建 TLS 配置（`direct` 时失败即拒）→ 绑定 listener（失败即拒）→ 开放本地通道与网络接入。`daemon.listen`/`daemon.tls.*`/`daemon.allowed_hosts`/`daemon.trusted_proxies` 从 unwired 清单移除。
- 关闭：「停接入层」扩展为同时停止网络 listener 的 accept 并排空在途连接至 `daemon.shutdown_grace_ms` 宽限上限，随后才取消周期任务、停 Agent、刷新存储。
- `daemon.status.listen` 返回实际绑定地址；`links[]` 仍恒空（出站重连管理器属切片 6，见 `NODE_LINK_PROTOCOL.md` §15）。
- **行为变化通告**：此前配置了 `daemon.listen` 等键只会得到 debug 级 unwired 注记，本切片起真实生效——写入 `CONFIG_REFERENCE.md` 的注记收敛与 README/DEVELOPMENT_PLAN 的状态更新。

### D12 配对通道的 Actor 与 core 用例面扩展（用户裁决 A，2026-09-26）

实施中发现切片 1 合同的空档：配对 HTTP（claim/status）与「首次认证成功 → `consumed`」在 core 用例面没有合法入口——`claim_pairing`/`pairing()` 要求 `Actor::LocalCli`，而身份合同禁止网络适配器构造它；`consumed` 没有任何写入口。用户已裁决按**方案 A** 扩展 core：

- **`core::model` 新增 `Actor::PairingClaimant { pairing: PairingId }`**（`ActorKind::PairingClaimant` = `'pairing_claimant'`）：配对通道主体——由 pairingSecret proof 验证、尚无持久身份的认领方。身份合同 §5.1 增补构造规则：只能由配对 HTTP 端点在 claim/status 的证明校验路径上构造（绑定该配对的只读可在校验前，写集只在 proof 通过后提交），且**绑定该配对**（actor.pairing == 目标 pairing）。
- **用例面**：`claim_pairing` 与 `pairing`（状态读取）在 LocalCli 之外接受 `PairingClaimant`（配对 id 必须匹配，否则拒绝）；新增 `consume_pairing(actor, pairing)`（要求 `Actor::Node`/`Device` 且与该配对已批准的对端一致；写集 = 状态转 `consumed` + `terminal_at` + 审计）。审计动作新增 `node.authenticated`/`node.auth_failed`（WP4 节点握手留痕；`consumed` 转移不另设动作，随该写集记录）。
- **存储**：`owned_audit`/`imported_audit` 的 `actor_kind` CHECK 增 `'pairing_claimant'`、`action` CHECK 增两个 node 动作——SQLite 不能改 CHECK，按 §11.8 的 12-step 表重建走 **v2 → v3 migration**；`owned_command` 不动（claimant 永不可提交命令）。
- **端口**：`TrustStore` 增 `consume_pairing` 写集方法（`PairingConsumption` DTO）；§5 冻结签名同步扩展（新增方法允许，不改既有语义）。
- **同步范围**：`CORE_PORTS_AND_STORAGE.md` §3.5/§4/§5.3/§7.2/§7.3/§7.4、`SECURITY_DESIGN.md` §14.2（两个新动作）、`IDENTITY_AND_AUTH_CONTRACT.md` §5.1（构造规则）；漂移门禁随动。Sync 切片 7 的设备配对 HTTP 直接复用同一批入口。
- ** seam 补全（2026-09-26 主 Agent 裁决，任务 2.27）**：实现期发现三处缺环，同属本决策的实现细化——① `identity-auth` 没有「用配对 secret 验 status HMAC」的公开入口（新增 `Authority::verify_node_link_pairing_status` 并按 §5.1 登记）；② claim 的幂等重试需要读「已固定对端行」、status approved 需要读已授予 grants，core 增配对通道**绑定只读**入口（claimant 只能读自己配对的记录/对端/已授予集合）；③ §5.1 补写顺序规则「绑定 actor 的只读先于 proof 验证，写集只在 proof 通过后提交，401 仍不泄露差异；status 路径先只读再验 proof」。status `approved` 返回的 grants 取**已授予**集合（`owned_node`），非请求值。

### D13 `node.challenge` 补 `catalogRevision`（用户裁决 A，2026-09-26）

WP4 实现时发现 v1 wire 的内部不一致：两个连接 transcript 域（`node-link-challenge/v1`/`node-link-proof/v1`）都含 tag 6 `catalogRevision`，但 `node.challenge`/`node.hello` 的消息体都没有该字段——Access 首次连接无法验 `nodeProof` 也无法构造自己的 proof，握手不可能完成。用户裁决按**方案 A** 修订（v1 未发布，合同内修订）：`node.challenge` 增加必需字段 `catalogRevision`（decimal string）。同步面：`NODE_LINK_PROTOCOL.md` §12.2 表与修订记录、`schemas/node-link/v1/handshake.schema.json`、`fixtures/node-link/v1/`（manifest 与正反用例）、`node-link-protocol` 的 `NodeChallenge` 类型与契约测试、`server::node_link::conn` 的填充/读取。

另（实现既定冻结上限，非合同变更）：§2.5 的 JSON 嵌套深度 64 / 单对象字段 1024 / 单数组元素 10000 在 Node Link 路径补执行点（wire 边界解码侧），列入 WP5（任务 2.29 之后）。

### D14 catalog 可见性规则（用户裁决 b，2026-09-26）

WP5 勘察发现 `NODE_LINK_PROTOCOL.md` §8.2 的信任记录形状写有 `exportIds`（该 Access 可见的 Export 集合），但实现里没有该字段。用户裁决：**首阶段按 grant 交集派生可见性**（未撤销且 `export.scopes ∩ 该节点信任记录 grants ≠ ∅` 可见），`exportIds` 的独立可见性维度记为**后续阶段待办**（届时需 core model + `owned_node` 加列迁移 + `node.pair.confirm` 参数 + LOCAL_ADMIN_PROTOCOL/CLI/文档全套）。同步面：`NODE_LINK_PROTOCOL.md` §8.2 文字修订（含待办注记）、本变更 spec 的 R51/R52 口径。过滤仍是目录层表现，命令层授权交集不变（§10 的有效权限规则不受影响）。

## Risks / Trade-offs

- [rustls/axum 栈核验不通过（MSRV、许可证或维护状态）] → 实现第一批次的核验任务先于编码；失败即回到用户决策，不在编码中临时换栈。
- [catalog revision 无现成持久化水位，需要动 core/存储合同] → D4 已限定为「唯一可能触及点」，触及即先同步 `CORE_PORTS_AND_STORAGE.md` §5/§7 与漂移门禁再继续；若用户不希望本切片动存储合同，可退化为「每连接单调的内存 revision + 每次重连全量快照」（语义仍合规，因为 Access 的 catalog 本就是连接期内存数据），该退化方案在 plan 中登记为备选。
- [每事件 ACPR-CJ1 + SHA-256 的计算开销] → digest 只对有 `payload` 的事件计算一次；快照 digest 复用原始 chunk 字节，不引入二次序列化；事件速率受 broker 与命令限流约束，不构成热点。
- [明文 proxy 模式被误用到非 loopback] → D10 的启动告警 + `CONFIG_REFERENCE.md` 三形态表已有约束；不引入「自动探测反代」之类的隐式行为。
- [旧配置语义变化（unwired 键生效）] → D11 的通告；这些键此前不生效，不存在「依赖旧行为」的正当用法。
- [Windows 上的证书私钥权限检查与 Unix 语义不同] → 按 §13.2 的平台机制实现，Windows 本机留证、Linux CI 覆盖 Unix 路径；无法核验时如实记 BLOCKED 而非降级。
- [候选合入/推送节奏]：本地合入按流程执行；远端 main 交付走 AGENTS.md §8 的 PR 路径，`deps`/`advisories`/`secrets` 三个 CI job 无本地等价物，验收中如实记录。

## Open Questions

无。会改变规范、方案或任务拆分的问题已在生成规划前与用户裁决（TLS 范围、配对 HTTP 归属、E2E 降级）；D4 的 revision 来源是可在实现期安全核实的未知项，两个出口（复用水位 / 合同内增补计数器，备选内存 revision）都不改变外部行为约定。
