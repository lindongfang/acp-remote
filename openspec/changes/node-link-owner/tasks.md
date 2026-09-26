# Tasks: node-link-owner

## 1. Dependency and Resource Setup

- [x] 1.1 全变更；可选 environment/recon 执行者。在**新的任务级最小上下文**中核实：仓库绝对路径与目标引用（`git rev-parse refs/heads/main`、`git status --porcelain`、`git worktree list`）、工具链版本（`rust-toolchain.toml` 与 `cargo --version`）、Node 版本、`npm ci` 状态、既有合同门禁与 `cargo test --locked --workspace --all-features` 的基线结果、`crates/server` 与 `crates/app` 的实际现状。完成条件：返回结构化事实（命令、退出码、关键输出）并写入 `verification.md` 的运行时基线；若基线本身为红，如实记录并把影响告知主 Agent，不开始实现。
- [x] 1.2 WP1–WP8；实现 Agent（coder）。确认实现所需契约基线（`docs/NODE_LINK_PROTOCOL.md` 现行版、`docs/IDENTITY_AND_AUTH_CONTRACT.md` §5、`docs/CORE_PORTS_AND_STORAGE.md` §5–§7、`docs/CONFIG_REFERENCE.md` §1/§3/§10、`docs/SECURITY_DESIGN.md` §7.1/§9.5/§13.2/§14/§20、`docs/MODULE_ARCHITECTURE.md` §4.4/§4.9/§5、`design.md` D1–D11）与文件所有权（计划「Work Packages」/「Execution Waves」的写范围与单一写入者）。完成条件：记录编码起点与写入范围（WP 表输入清单 + `git rev-parse HEAD`）；契约明确后即可开始编码。
- [x] 1.3 WP1–WP8；实现 Agent（coder）。确定运行资源隔离方式：每个执行者独立的 `CARGO_TARGET_DIR`、集成用例自建且结束时删除的临时 Daemon 数据目录与自签证书 PEM、loopback 一律 `127.0.0.1:0` 随机端口（计划「Runtime Resources」）。完成条件：记录资源名、隔离/独占方法与释放步骤；确认不涉及数据库服务、容器、固定端口、外部账号或网络资源，并把依据写入 `verification.md`。
- [x] 1.4 WP3–WP8；实现 Agent（coder）。在相关集成验证前接入已验收上游形状（WP2 的 transport::net 公开形状、WP4 的连接注册表形状、WP1 的依赖口径），核对实际基线提交与包含关系，关联 `verification.md` 的交接证据（计划「Dependency Handoffs」各行）。完成条件：`cargo build -p server`/`cargo build -p app` 只经公开项消费上游；上游形状变化时按失效列重开相应证据。

## 2. Implementation

- [x] 2.1 WP1；前置：1.2；实现 Agent（coder）。把本轮范围写入 `docs/MODULE_ARCHITECTURE.md`：§5 矩阵为 `server` 行增补 `acpr-wire` ✓ 并写明「只为 ACPR-CJ1 digest」的限定注记（`design.md` D5）、§3.1 预告新增依赖口径、§4.9 现状注记更新为「`node_link` 实现中」。完成条件：文档与 `design.md` 一致，引用关系可被 `check:doc-links` 接受；不改 `MODULE_ARCHITECTURE.md` 的其他语义。
- [x] 2.2 WP1；前置：2.1；实现 Agent（coder）。按 `SECURITY_DESIGN.md` §20 四判据核验 HTTP/WS/TLS 栈候选（axum、tokio-tungstenite、rustls、tokio-rustls、rustls-pemfile；替代候选 axum-server 一并记录取舍），随后在 `Cargo.toml` 的 `[workspace.dependencies]` 登记选定依赖与 feature 增量、`deny.toml` 同步 allow 列表。完成条件：核验证据写入 `reports/wp1-deps.log`（含 `cargo tree`/`cargo metadata` 输出或许可证来源记录）；`cargo metadata --no-deps` 可解析；不抬高 `rust-version`；核验失败即停止并交用户决策，不擅自换栈。
- [x] 2.3 WP1；前置：1.3、2.2；实现 Agent（coder）。跑局部验证：`npm run check`（重点 `check:doc-links`、`check:contract-drift`）与 `node scripts/check-crate-boundaries.mjs`（[PV2]，含新增矩阵格）。完成条件：[PV1]/[PV2] 的 W0 轮次通过，日志写入 `reports/wp1-deps.log`。
- [x] 2.4 WP2；前置：1.4、2.3；实现 Agent（coder）。实现 `server::transport::net` 的 listener 与路由（`design.md` D2/D10）：`daemon.listen` 绑定与失败关闭、非 loopback 告警、path 路由（`/node-link/v1` WS upgrade、`/node-link/v1/pairing/claim|status`、`/sync/v1*` 与其余路径 404）、WS upgrade 规则（subprotocol 白名单、拒绝 `permessage-deflate`）、Host/`allowed_hosts`/`trusted_proxies` 边界。完成条件：[PV3] 中 [R1]–[R11] 场景通过，日志写入 `reports/wp2-transport-net.log`。
- [x] 2.5 WP2；前置：2.4；实现 Agent（coder）。实现 TLS 两种模式与连接级上限（D9/D10）：`proxy` 明文 listener、`direct` 的 rustls 终止（PEM 加载、权限检查、失败关闭、私钥不进日志/`Debug`）、`dev_mode.allow_plaintext` 只允许 loopback、请求体上限（413）与单消息上限（1009，分配前拒绝）。完成条件：[PV3] 覆盖 [R12]–[R18]；`cfg(windows)` 的权限路径用例本机执行（[PV5]）并写入 `reports/pv5-windows-nodelink.log`。
- [x] 2.6 WP2；前置：2.4、2.5；实现 Agent（coder）。跑 WP2 交付前局部验证：`cargo fmt --all -- --check`、`cargo clippy --locked -p server --all-targets --all-features -- -D warnings`、`cargo test --locked -p server --all-features`（[PV3]），并自检 `rg -n "unsafe" crates/server/src` 零命中。完成条件：全绿且无零用例/全跳过；transport::net 公开形状冻结并作为 handoff 交 WP3/WP4。
- [x] 2.25 WP3（合同扩展，D12；用户裁决 A，2026-09-26）；前置：2.6；实现 Agent（coder）。core 用例面扩展：`core::model` 新增 `Actor::PairingClaimant { pairing: PairingId }`（`ActorKind::PairingClaimant` = `'pairing_claimant'`）与 `AuditAction::NodeAuthenticated`/`NodeAuthFailed`；`claim_pairing`/`pairing` 在 LocalCli 之外接受与该配对绑定的 `PairingClaimant`；新增 `consume_pairing(actor, pairing)`（要求 `Actor::Node`/`Device` 且为该配对已批准对端；写集 = 状态转 `consumed` + `terminal_at` + 审计）；`core::ports::TrustStore` 增 `consume_pairing` 写集方法（`PairingConsumption` DTO）。同一改动内同步：`docs/CORE_PORTS_AND_STORAGE.md` §3.5/§4/§5.3、`docs/SECURITY_DESIGN.md` §14.2（两个新动作）、`docs/IDENTITY_AND_AUTH_CONTRACT.md` §5.1（PairingClaimant 构造规则）。完成条件：core 单测覆盖新变体/新入口/绑定校验；`cargo test --locked -p core --all-features` 全绿（[PV3] 范围）；文档与代码一致。
- [x] 2.26 WP3（合同扩展，D12）；前置：2.25；实现 Agent（coder）。storage-sqlite v2 → v3 迁移：`owned_audit`/`imported_audit` 的 `actor_kind` CHECK 增 `'pairing_claimant'`、`action` CHECK 增 `'node.authenticated'`/`'node.auth_failed'`（12-step 表重建，沿用 §11.8 模式）；`TrustStore::consume_pairing` 落盘（状态推进 + 时间戳 + 审计同事务）；`owned_command` 不动。同一改动内同步 `docs/CORE_PORTS_AND_STORAGE.md` §7.2/§7.3/§7.4 并确保漂移门禁通过。完成条件：v3 迁移测试（旧库升级、CHECK 生效、consume 写集原子性、崩溃恢复）全绿（[PV3] 范围）；`npm run check` 全绿（含 `check:drift`）。
- [x] 2.27 WP3（合同 seam 补全，D12 的实现细化；主 Agent 裁决，2026-09-26）；前置：2.26；实现 Agent（coder）。补三处配对通道 seam：① `identity-auth` 新增 `Authority::verify_node_link_pairing_status`（按配对取内存 secret 验 `node-link-pairing-status/v1` HMAC；secret 缺失与 HMAC 失败按既有错误分类），并按 §5.1「全部公开入口在此登记」补登记；② core 用例的配对通道只读入口（claimant 绑定配对的 `pairing_peer` 读取与 approved 节点的已授予 grants 视图；`pairing`/`claim_pairing` 的绑定规则不变；只读必须先于 proof、写集只在 proof 通过后提交、401 不泄露差异）；③ `docs/IDENTITY_AND_AUTH_CONTRACT.md` §5.1 补写该只读顺序规则，`docs/CORE_PORTS_AND_STORAGE.md` §4 同步用例访问规则。完成条件：core/identity-auth 单测覆盖新入口与绑定规则；`cargo test -p core -p identity-auth --all-features` 与 `npm run check` 全绿。
- [x] 2.7 WP3；前置：1.4、2.6、2.25、2.26、2.27；实现 Agent（coder）。实现配对 claim 端点（`NODE_LINK_PROTOCOL.md` §13.2/§13.4、design D2/D3）：`application/json` 与请求体上限、原子检查（存在/未过期/`created`/endpoint host/HMAC）、`ownerProof` 签发、幂等重试（相同 `accessNodeId`/`clientNonce` 返回原 pairing request）、400/401/403/404/409/410 映射、401 不泄露校验差异、安全响应头。完成条件：[PV3] 覆盖 [R17]、[R19]–[R25]、[R30]/[R31]（[R19]/[R20] 的本机轮次计入 [PV5]），日志写入 `reports/wp3-pairing-http.log`。
- [x] 2.8 WP3；前置：2.7；实现 Agent（coder）。实现配对 status 端点与限流（§13.3/§2.5）：一律 200 + body 五状态、`node-link-pairing-status/v1` proof 校验（401）、`requestNonce` 重试规则、claim 10/min/IP 与 status 60/min/pairingId（429）、secret 生命周期（到期清除；首次 WSS 认证成功的提前清除与 WP4 的 `complete_auth` 提交衔接）。完成条件：[PV3] 覆盖 [R26]–[R35]（[R26] 的本机轮次计入 [PV5]）；secret 清除的衔接点与 WP4 确认一致。
- [x] 2.9 WP3；前置：2.7、2.8；实现 Agent（coder）。跑 WP3 交付前局部验证（同 2.6 的命令集，[PV3]）。完成条件：全绿；配对处理器形状冻结。
- [x] 2.28 WP4（合同 seam 补全，主 Agent 裁决 2026-09-26，延续用户裁决 A 的方向）；前置：2.27；实现 Agent（coder）。core 增节点握手只读视图（按 accessNodeId 绑定返回该节点信任记录/`PeerPublicKey`/已批准待消费配对/serverEpoch；不放开成通用读）与认证审计写入口（只追加 `node.authenticated`/`node.auth_failed`）；LocalCli 既有路径行为不变；同步 `docs/CORE_PORTS_AND_STORAGE.md` §4/§5 并复跑 `check:drift`。完成条件：core 单测覆盖新入口与绑定拒绝；`cargo test -p core --all-features` 与 `npm run check` 全绿。
- [x] 2.10 WP4；前置：1.4、2.6、2.28；实现 Agent（coder）。实现握手与信封层（D3）：认证前消息白名单（4401）、15 秒握手超时（4408）、版本交集（4406）与 feature 协商（必需 feature → `feature_required` + `details.features`）、经 `identity-auth::Authority` 三入口的双向认证（每次握手从持久化信任取 `PeerTrust` 快照）、凭据状态映射（`node_revoked`+4410/`node_unknown`）、`complete_auth` 写集同事务提交后才发 `node.ready`、信封与双向 connectionSequence 校验（`sequence_invalid`）、closed object/未知字段（`schema_invalid`）、未知与 post_mvp type（`type_unsupported`）、binary frame（4400）。完成条件：[PV3] 覆盖 [R32]（衔接侧）、[R36]–[R43]（[R36]/[R37]/[R40] 的本机轮次计入 [PV5]）、[R47]–[R50]，日志写入 `reports/wp4-handshake.log`；schema/fixture 漂移测试消费 `fixtures/node-link/v1/manifest.json`。
- [x] 2.11 WP4；前置：2.10；实现 Agent（coder）。实现 limits 与连接健康（D9）：`node.ready.limits` 只下调、固定常量不可配置、认证限流 10/min/IP、心跳 ping/pong 与 90 秒静默关闭（4408）、每连接有界发送队列与高水位行为（停读快照批次、持续过慢断开、不发 `link.backpressure`）、审计接线（认证失败/连接关闭）。完成条件：[PV3] 覆盖 [R44]–[R46]、[R79]–[R81] 与 [R82]/[R83] 的连接侧部分。
- [x] 2.12 WP4；前置：2.10、2.11；实现 Agent（coder）。跑 WP4 交付前局部验证（[PV3]，命令集同 2.6）。完成条件：全绿；连接注册表与信封校验形状冻结并作为 handoff 交 WP5/WP6。
- [x] 2.29 WP4（wire 合同修订，用户裁决 A，2026-09-26，design D13）；前置：2.12；实现 Agent（coder）。`node.challenge` 增加必需字段 `catalogRevision`（decimal string）：同步 `docs/NODE_LINK_PROTOCOL.md` §12.2 表与修订记录、`schemas/node-link/v1/handshake.schema.json`、`fixtures/node-link/v1/`（manifest 与正反用例）、`node-link-protocol` 的 `NodeChallenge` 类型与契约测试、`server::node_link::conn` 的填充/读取。完成条件：合同门禁（含 schema/fixture 校验）与 `cargo test -p node-link-protocol -p server --all-features` 全绿；Access 侧能用该字段验 `nodeProof` 并构造 proof 的用例存在。
- [x] 2.30 WP5 预备（§2.5 JSON 结构上限执行点；design D13 附带项）；前置：2.29；实现 Agent（coder）。在 Node Link 的 wire 解码边界补固定常量执行：JSON 嵌套深度 64、单对象字段数 1024、单数组元素 10000，超限按 `nodelink.protocol.schema_invalid`/连接级规则拒绝。完成条件：三个上限各有正反而例测试；`npm run check` 与相关 cargo 测试全绿。
- [x] 2.13 WP5；前置：1.4、2.12、2.29、2.30；实现 Agent（coder）。实现 catalog 投影（D4/D14）：`catalog.subscribe`/`catalog.snapshot` 从持久化 Export 记录实时投影，可见性按 grant 交集派生（未撤销且 `export.scopes ∩ 该节点信任记录 grants ≠ ∅`；2026-09-26 用户裁决 b，`exportIds` 推后为后续待办）、批次稳定切分（`catalogSnapshotBatchSize`）、Export 条目全字段（含零参数 template 约束的校验）、revision 来源按 D4 核实（复用既有水位优先；确需增补计数器时先同步 `CORE_PORTS_AND_STORAGE.md` §5/§7 与漂移门禁再继续）、`knownRevision` 非空回完整快照。完成条件：[PV3] 覆盖 [R51]–[R53]（[R51]/[R52] 的本机轮次计入 [PV5]），日志写入 `reports/wp5-catalog-resource.log`；若触及存储合同，同步证据一并写入。
- [x] 2.14 WP5；前置：2.13；实现 Agent（coder）。实现 attach 与订阅（D6）：`resource.attach`/`attached` 签发新 generation、旧 generation frame 拒绝（`attach_generation_stale`）、可见性/撤销复核（`export.not_found`/`not_granted`）、`cursor = null` 快照（`session_meta`/`pending_interactions`、无正文、`snapshotDigest` 原始字节规则）与非空 cursor 的 origin 增量重放（epoch 不一致 → `sequence_invalid`）、单资源快照并发为 1。完成条件：[PV3] 覆盖 [R54]–[R59]（[R54]/[R55]/[R57]/[R58] 的本机轮次计入 [PV5]）。
- [x] 2.15 WP5；前置：2.14；实现 Agent（coder）。实现事件扇出与 ACK（D5/D6）：组合根分叉 `EventPublisher` 接线、`CommittedDelivery::Owned` → `resource.event` 映射（origin 三元组、`payloadDigest` = ACPR-CJ1、`payload.view` 最低字段、未登记类型保留 `payload.acp`、256 KiB `rawUnavailable(size_limit)`）、先持久化后发布的路径断言、`resource.ack` 单调性与归属校验。完成条件：[PV3] 覆盖 [R60]–[R65]（[R60]/[R61] 的本机轮次计入 [PV5]）。
- [x] 2.16 WP5；前置：2.13–2.15；实现 Agent（coder）。跑 WP5 交付前局部验证（[PV3]，命令集同 2.6）。完成条件：全绿；事件映射与扇出形状冻结。
- [x] 2.17 WP6；前置：1.4、2.12；实现 Agent（coder）。实现命令管线（D8）：wire 边界校验 → 授权交集（Export grant ∩ 信任记录 grant ∩ capability）→ 幂等（`(ownerNodeId, accessNodeId, requestId)`，重复回首次结果、语义冲突 `idempotency_conflict`）→ 用例派发 → `accepted`/`rejected`/`terminal` 映射（`acceptedAt` 规则、`terminal` 必带 `command`、`completed` 非空 `result` 由 adapter 保证、`uncertain` 透传）、`command.status` 两种形式归一（重查 `terminalEventId` 非空）、`expectedVersion` 强制、越权审计。完成条件：[PV3] 覆盖 [R66]–[R70]（[R66]/[R67]/[R69] 的本机轮次计入 [PV5]）与 [R82]/[R83] 的命令侧部分，日志写入 `reports/wp6-command.log`。
- [x] 2.18 WP6；前置：2.17；实现 Agent（coder）。实现 `session.create` 硬约束与命令限流（§12.7、D8）：四键白名单、禁带字段 → `unsupported_field` + `details.field` 且不创建会话、未导出组合 → `not_granted`（`details.parameter`）、零参数 template 强制、`sessionRef`/`attachmentId`/`attachmentGeneration`/`expectedVersion` 必为 `null`、`accepted(result=null)` → `terminal(SessionCreateResult)`、in-flight 上限与 120/分钟限流（`rate_limited` + `retryAfterMs`、连续超限 4429）。完成条件：[PV3] 覆盖 [R71]–[R75]（[R72]/[R73] 的本机轮次计入 [PV5]）。
- [x] 2.19 WP6；前置：2.17；实现 Agent（coder）。实现撤销传播（D7）：扩展 `local_admin::pairing::ConnectionCloser` 缝（`close_node`/`export_revoked`），组合根以 `node_link` 连接注册表句柄实现；`local_admin` 在 `node.revoke`/`export.revoke` 持久提交后调用（失败只记日志不回滚）；`node_link` 逐消息权威复核授权 + 推送 `export.revoked`/`node.trust.revoked` + 4410 关闭。完成条件：[PV3] 覆盖 [R76]–[R78]（三者的本机轮次计入 [PV5]）；既有 `local_admin` 测试保持全绿（缝扩展不改变既有方法行为）。
- [x] 2.20 WP6；前置：2.17–2.19；实现 Agent（coder）。跑 WP6 交付前局部验证（[PV3]，命令集同 2.6）。完成条件：全绿且无零用例/全跳过。
- [x] 2.21 WP7；前置：1.4、2.6、2.9、2.12、2.16、2.20；实现 Agent（coder）。实现 app 组合根接线（D11）：`daemon.listen`/`daemon.tls.*`/`daemon.allowed_hosts`/`daemon.trusted_proxies` 从 unwired 清单移除并接线、启动序列（恢复 → TLS 配置 → 绑定 listener → 开放接入）、关闭序列含网络 listener 排空（`daemon.shutdown_grace_ms` 宽限）、`daemon.status.listen` 返回实际地址（`links` 仍恒空）、proxy 模式非 loopback 的额外告警。完成条件：[PV4] 覆盖 [R1]–[R4]、[R12]–[R15] 的 daemon 侧（[R12]/[R13]/[R14] 的本机轮次计入 [PV5]），日志写入 `reports/wp7-app-wiring.log`。
- [x] 2.22 WP7；前置：2.21；实现 Agent（coder）。落地受控路径全链路集成测试（切片 5 验收闭环，PV5 主体）：脚本化 fake Access 客户端经真实 loopback listener 完成 配对 claim → 本地确认（经 local_admin 入口）→ status approved → 握手 → `catalog.snapshot`（过滤断言）→ `resource.attach`/`subscribe`（快照 + 增量）→ `resource.event`/`ack`（持久化顺序与去重前提）→ `command.submit`（含 `session.create` 正常路径与 `unsupported_field`/`not_granted` 拒绝）→ 幂等重试 → `export.revoke`/`node.revoke` 撤销传播；TLS direct 用自签证书跑通同一握手。完成条件：测试真实执行并全绿，原始日志写入 `reports/wp7-integration.log` 与 `reports/pv5-windows-nodelink.log`。
- [x] 2.23 WP8；前置：2.22；实现 Agent（coder）。状态写回：`docs/MODULE_ARCHITECTURE.md` §4.9 现状注记（`node_link` 已落地范围）与 §3.1/§5 收尾、`README.md`「仓库当前状态」、`docs/DEVELOPMENT_PLAN.md` §2、`AGENTS.md` §4 模块状态表、`docs/CONFIG_REFERENCE.md` 的 unwired 注记收敛。完成条件：文档陈述与 `cargo metadata`/`cargo tree` 实际一致；不把 `node-link-client`/`server::acp_facade`/`server::sync` 写成已落地。
- [x] 2.24 WP8；前置：2.23；实现 Agent（coder）。跑全量统一入口 `npm run verify`（[PV1]）与 `node scripts/check-crate-boundaries.mjs`（[PV2]）。完成条件：两个检查在固定版本上全绿、日志写入 `reports/du1-pv1.log`；既有门禁因本变更变红时就地修复后再跑，不以「稍后修」结项。

## 3. Branch Validation

- [x] 3.1 WP1；前置：2.3；实现 Agent（coder）。完成 WP1 交付前 project verify：`npm run verify`（[PV1]）与 `node scripts/check-crate-boundaries.mjs`（[PV2]），逐项记录完整命令、工具链版本、退出码与日志路径。完成条件：两项通过；资源已核实释放。
- [x] 3.2 WP1；前置：2.3；独立 reviewer。新建**不继承实现对话**的只读 reviewer 子 Agent，按 `roles/reviewer.md` 检视 WP1 固定版本（矩阵格与依赖口径、§20 核验证据真实性、文档引用归属）。完成条件：`reports/rv1-wp1.md` 记录 Agent ID、版本、隔离方式与结论；阻断项修复后由新子 Agent 复核。
- [x] 3.3 WP2；前置：2.6；实现 Agent（coder）。完成 WP2 交付前 project verify：[PV3] + [PV1]/[PV2]，核对用例确实执行。完成条件：逐项通过并留证；临时端口/目录已释放。
- [x] 3.4 WP2；前置：2.6；独立 reviewer。隔离检视 WP2：TLS 失败关闭与私钥边界、Host/代理头不可绕过、压缩/subprotocol 拒绝、上限在分配前生效、无 unsafe。完成条件：`reports/rv1-wp2.md`；阻断项修复后复核。
- [x] 3.5 WP3；前置：2.9；实现 Agent（coder）。完成 WP3 交付前 project verify：[PV3] + [PV5]（本机）。完成条件：逐项通过并留证。
- [x] 3.6 WP3；前置：2.9；独立 reviewer。隔离检视 WP3：401 不泄露差异、幂等重试不建第二条记录、安全头逐项、secret 生命周期、限流键以真实对端地址为准。完成条件：`reports/rv1-wp3.md`；阻断项修复后复核。
- [x] 3.7 WP4；前置：2.12；实现 Agent（coder）。完成 WP4 交付前 project verify：[PV3] + [PV5]（本机）。完成条件：逐项通过并留证。
- [x] 3.8 WP4；前置：2.12；独立 reviewer。隔离检视 WP4：握手只经 `Authority` 入口、验签公钥只来自持久化快照、认证前白名单与序号规则逐条、limits 只下调、心跳/超时常量、审计不含秘密。完成条件：`reports/rv1-wp4.md`；阻断项修复后复核。
- [x] 3.9 WP5；前置：2.16；实现 Agent（coder）。完成 WP5 交付前 project verify：[PV3] + [PV5]（本机）。完成条件：逐项通过并留证。
- [x] 3.10 WP5；前置：2.16；独立 reviewer。隔离检视 WP5：先持久化后发布、快照无正文、digest 走 ACPR-CJ1、raw 保真与 256 KiB 降级、generation 隔离、epoch 校验；若 WP5 触及存储合同（D4 增补），核对 `CORE_PORTS_AND_STORAGE.md` §5/§7 同步与漂移门禁。完成条件：`reports/rv1-wp5.md`；阻断项修复后复核。
- [x] 3.11 WP6；前置：2.20；实现 Agent（coder）。完成 WP6 交付前 project verify：[PV3] + [PV5]（本机）。完成条件：逐项通过并留证。
- [x] 3.12 WP6；前置：2.20；独立 reviewer。隔离检视 WP6：幂等键完整性与冲突判定、越权拒绝无副作用、`terminal` 形状契约、session.create 四键白名单、撤销推送不回滚提交、缝扩展不改变既有 local_admin 行为。完成条件：`reports/rv1-wp6.md`；阻断项修复后复核。
- [x] 3.13 WP7；前置：2.22；实现 Agent（coder）。完成 WP7 交付前 project verify：[PV4] + [PV5]（本机全链路），核对集成测试真实执行（无全跳过）。完成条件：逐项通过并留证；临时数据目录与证书已清理。
- [x] 3.14 WP7；前置：2.22；独立 reviewer。隔离检视 WP7：组合根零业务规则、启动/关闭顺序、unwired 收敛完整、全链路集成测试的断言覆盖切片 5 验收四条。完成条件：`reports/rv1-wp7.md`；阻断项修复后复核。
- [x] 3.15 WP8；前置：2.24；实现 Agent（coder）。完成 WP8 交付前 project verify：[PV1] + [PV2]。完成条件：两项在固定版本上通过。
- [x] 3.16 WP8；前置：2.24；独立 reviewer。隔离检视 WP8 的文档改动：状态表与 `cargo metadata` 实际一致、未误写未落地 crate 为已落地、unwired 注记收敛无残留。完成条件：`reports/rv1-wp8.md`；阻断项修复后复核。

## 4. Test Design and Authoring

不适用：Main E2E mode 为 `not-applicable`（计划已含 2026-09-26 用户降级批准），不生成 TP 与用例设计任务；各 WP 的行为测试由实现 Agent 自带（2.x），受控路径全链路集成测试归入 WP7（2.22），最终替代验证见 7.1。

## 5. Integration Readiness

- [x] 5.1 主 Agent（仅一次，不随交付单元复制）。单独创建独立集成 Agent，显式交接 `roles/integrator.md` 全文、本计划与相关契约、源提交及已验收证据、独立集成 worktree、目标分支 `refs/heads/main` 与授权边界（apply 已授权本地合入；推送远端、回滚、发布需另行授权）。完成条件：记录集成 Agent 的实际 ID、上下文方式与交接清单；主 Agent 不兼任集成执行者；宿主缺少独立执行能力时本任务与第 6 组合并入相关任务记 BLOCKED，并如实上报。
- [x] 5.2 DU1；前置：3.1–3.16；主 Agent。复核 DU1 的预定模式（`integrated`）与组成（WP1–WP8），核对 [PV1]–[PV5] 与 RV1 的有效证据，确认无未解决的阻断项与未登记漂移；`integrated` 模式下引用组合后的 verify/review 结果，不重复单 WP 的检查。完成条件：计划与依赖已同步（无待更新项）、就绪判据全部满足；变化先同步计划与依赖再进入第 6 组。

## 6. Merge Unit

- [x] 6.1 DU1；主 Agent（机械核实可交 environment/recon）；前置：5.2。按计划核实目标仓库及主分支当前提交（`git rev-parse refs/heads/main` 等），记录准确引用及核实证据；无法确认目标时保持 BLOCKED。
- [x] 6.2 DU1；集成负责人；前置：6.1。基于已核实基线构造 DU1 候选，固定基线和候选版本，记录组成与构建结果。
- [x] 6.3 DU1；检查执行者；前置：6.2。完成候选 Project Verify：[PV1]（`npm run verify`）与 [PV2]（`check-crate-boundaries`），将版本、范围、结果及有效复用依据关联到 `verification.md`。
- [x] 6.4 DU1；独立 reviewer；前置：6.2，可与 6.3 并行。只读检视固定候选的新增交互与冲突解决，修复后独立复核，记录隔离设置、版本及报告（`reports/rv2-du1.md`）。
- [x] 6.5 DU1；前置：固定候选及所需资源。E2E `not-applicable`：核对计划中的理由、依据与 `downgrade_approval` 记录有效，并完成适用替代检查的候选轮次——候选上执行 [PV5] 受控路径全链路集成测试（`cargo test --locked -p server -p app --all-features` 的相关用例，本机 Windows）并把原始日志写入 `reports/pv5-windows-nodelink.log`；连续失败计入 `x-agentic.e2e.maxAttempts`（默认 3），达到后停止自动重跑并交用户决策。
- [x] 6.6 DU1；合并负责人；前置：6.3、6.4、6.5。确认候选检查、独立 review 与替代验证通过，核实仓库规则与本地主分支基线，并运行 `npx --quiet --no-install openspec-agentic workflow check --change node-link-owner --stage premerge --planning-root <权威规划根> --json`（非 PASS 不合入）；以条件更新或串行合并机制防止竞态，直接合入 `refs/heads/main`，无须再次询问用户，记录实际提交；基线变化时重开受影响候选任务。
- [x] 6.7 DU1；检查执行者；前置：6.6。核对实际主分支结果与候选一致性，完成 [PV1]/[PV2] 主分支回归；有效复用逐项记录原证据及适用性。
- [x] 6.8 DU1；独立 reviewer；前置：6.6。独立检视合并新增差异；无新增差异由主 Agent 记录依据及原 review ID，不强制同范围重复审查，可与 6.7 并行。

## 7. Final E2E

- [x] 7.1 全变更；主 Agent；前置：主分支检查（6.7、6.8）。执行最终替代验证（`not-applicable` 路径）：在最终主分支版本上重跑 [PV1]（`npm run verify`）、[PV3]/[PV4] 与 [PV5] 受控路径全链路集成测试，逐项记录命令、版本、退出码与日志路径（`reports/du1-pv1.log`、`reports/pv5-windows-nodelink.log`、`reports/wp7-integration.log`），并核实临时资源已清理。
- [ ] 7.2 全变更；主 Agent；前置：7.1。汇总替代验证证据：核对 `not-applicable` 的理由/依据/`downgrade_approval` 三要素仍有效、Coverage Index 的 R1–R83 全部有有效证据、无未解决 FAIL/BLOCKED；全部必要检查通过后完成。
- [ ] 7.3 [e2e-owned] 全变更；扩展；前置：7.2。运行 `npx --quiet --no-install openspec-agentic e2e check --change node-link-owner`，仅 PASS 自动勾选；此行只检查门禁（不适用判据已按计划固化且替代验证已完成），不执行测试或汇总。

## 8. Final Verification

- [ ] 8.1 [final-verification] 使用 agentic-verify 执行最终验收（/opsx:verify 同样读取该入口），核对用户意图（切片 5 原文与三项裁决）、需求（R1–R83）、设计（D1–D11）、计划、任务与最终主分支证据；记录当前 agentic-assessment 后运行 `npx --quiet --no-install openspec-agentic workflow check --change node-link-owner --stage final --json`，全部通过才完成；验收结论单独报告 PASS / FAIL / BLOCKED，不以 CLI 的 `all_done` 代替。
