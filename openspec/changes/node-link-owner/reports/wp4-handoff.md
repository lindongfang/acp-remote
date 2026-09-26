# WP4 Handoff — 握手与连接生命周期（`node-link-owner` / DU1）

## Shared Report

- **task_id**：2.10（握手与信封层）、2.11（limits 与连接健康）、2.12（交付前局部验证）；另有 2.28（合同 seam，本 WP 的前置，同一批次内先单独提交）。
- **role / phase**：coder / implement。
- **agent_context**：worker 子 Agent（本机 worktree 独占；未继承其它 WP 的实现对话）。工作目录 `D:\Project\acp-remote-wt\node-link-owner`（分支 `agentic/node-link-owner`）。
- **target_revision**：`038349b`（本 WP 交付提交）；同批次前置提交 `a811932`（2.28 seam）。起点 `443c6c0`（WP3 之后）。
- **scope**：
  - 2.28：`crates/core/src/ports.rs`（`TrustStore::pairing_for`）、`crates/core/src/use_cases.rs`、`crates/core/src/broker.rs`（测试替身）、`crates/storage-sqlite/src/admin/trust.rs`、`crates/identity-auth/src/types.rs` + `tests/handshake.rs`、`docs/CORE_PORTS_AND_STORAGE.md`（§4/§5）；
  - 2.10/2.11：`crates/server/src/node_link/conn/**`（新增 7 个文件）、`crates/server/src/node_link/mod.rs`、`crates/server/src/local_admin/test_support.rs`（测试设施）。
  - **未改**任何权威规划工件（`plan.md`/`tasks.md`/`verification.md`）、`schemas/**`、`compatibility/**`、`fixtures/**`、`crates/app/**`（WP7 组合根）、`crates/server/src/transport/**`（WP2 冻结形状）、`crates/server/tests/**`。
- **changes**：见「改动与需求映射」与「冻结的公开形状」。
- **checks**：`[PV3]` 的 fmt / clippy（`-p server -p core -p identity-auth`）/ `cargo test`（同三 crate）/ `rg unsafe`（无匹配）/ `npm run check` 全绿；原始输出 `reports/wp4-handshake.log`。
- **issues**：无阻断项。需要主 Agent 与 reviewer 知晓的事实：1 条**合同缺口**（§12.2 的 `node.challenge` 不带 `catalogRevision`，而 §9.4 的两个连接 transcript 都含 tag 6）与 1 条**门禁归属问题**（§2.5 的「JSON 嵌套 64 / 单对象字段 1024 / 单数组元素 10000」目前在 Node Link 路径上没有执行点），详见「合同缺口与归属问题」。两者都不影响本 WP 的 Owner 侧实现。
- **result**：**PASS**（本工作包检查全部满足）。**不代表**独立 review（3.8）、`[PV5]`/E2E 或合并已完成。
- **evidence_paths**：`reports/wp4-handshake.log`、本文件。
- **resource_cleanup**：用例只绑定 `127.0.0.1:0`（`Harness::stop` 触发 listener 关闭与排空），未起后台进程、未用数据库服务/容器/固定端口；`TestWorld` 的临时资源由其 `Drop` 清理；`target/` 作为缓存保留（被 git 忽略）。

## handoff_index

```yaml
handoff_index:
  - task_id: "2.28"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "a811932"
    evidence_type: CHECK
    evidence_id: NOT_APPLICABLE
    report_path: "reports/wp4-handshake.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "core 握手只读视图与认证留痕入口（design D3 的 seam 补全，经 supervisor 裁定的方案 A）在 a811932 上实测：cargo test --locked -p core -p identity-auth -p storage-sqlite --all-features 全绿（core 104 含 2 个新用例）；cargo fmt --all -- --check 退 0；cargo clippy --locked ... -D warnings 退 0；node scripts/check-contract-drift.mjs 退 0（§5 的 15 traits/89 methods 与 core::ports 逐条一致）。正式 [PV3] 执行归 2.12/3.7。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.10"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "038349b"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/wp4-handshake.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "工作区 = 038349b（提交后 git status 为空）上的本机轮次：cargo fmt --all -- --check 退 0；cargo clippy --locked -p server -p core -p identity-auth --all-targets --all-features -- -D warnings 退 0；cargo test --locked -p server -p core -p identity-auth --all-features 全绿（server lib 218 通过，其中 node_link::conn 25；core 104）；rg unsafe crates/server/src 无匹配；npm run check 退 0（含 check:drift/check:boundaries/check:docs）。[PV5] 的本机轮次与跨实现互操作随后续任务 3.7/3.9 执行，本行只声称 [PV3]。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.11"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "038349b"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/wp4-handshake.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同 2.10 的同一轮检查（同一次 cargo test/clippy/fmt/npm run check 覆盖 2.10 与 2.11 的全部改动）；[R44]–[R46]、[R79]–[R81] 与 [R82]/[R83] 的连接侧用例在日志里逐条列出（node_link::conn::tests 的 17 个用例）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.12"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "038349b"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/wp4-handshake.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "2.12 的唯一完成条件就是「局部验证全绿 + 冻结形状交接」：命令与输出见日志，冻结形状见本文件「冻结的公开形状（供 WP5/WP6/WP7）」。"
    source_evidence: NOT_APPLICABLE
```

## 改动与需求映射

### 2.28（合同 seam，提交 `a811932`）

| 文件 | 内容 |
|---|---|
| `crates/core/src/ports.rs` | `TrustStore::pairing_for(&PeerIdentity) -> Option<PairingRecord>`（该对端最近一次配对；按 `created_at`/`pairing_id` 降序） |
| `crates/core/src/use_cases.rs` | `NodeLinkHandshakeView { node, public_key, pairing, head }`；`node_link_handshake_view(&NodeId)`（用例面唯一无 actor 的只读入口；视图绑定单个自报 node id；未知 id → 空视图；零写入、不产审计）；`record_node_link_auth(&NodeId, AuditAction, AuditOutcome)`（只接受 `node.authenticated`/`node.auth_failed`，其余 `authorization.scope_denied`）；`ids()`（适配器给每帧取 `MessageId` 的公开只读入口）；2 个新用例 |
| `crates/core/src/broker.rs` | 测试替身补齐新 trait 方法（`pairing_for`、`peer_key` 的来源字段） |
| `crates/storage-sqlite/src/admin/trust.rs` | 真实 `pairing_for`：经既有 `owned_pairing_peer` 子查询 + `PAIRING_COLUMNS`，**不新增表/列** |
| `crates/identity-auth/src/types.rs` + `tests/handshake.rs` | `HandshakeFailure::audit()` 补 `ConnectionKind::NodeLink → node.auth_failed` |
| `docs/CORE_PORTS_AND_STORAGE.md` | §5.3 的 `pairing_for` 签名、§4 的 `NodeLinkHandshake` 行、两条 `[已裁定]` 说明 |

### 2.10 / 2.11（提交 `038349b`）

| 文件 | 对应任务点 | 内容 |
|---|---|---|
| `conn/mod.rs` | 骨架 | 模块职责与依赖纪律文档、`WS_PATH`、`WS_SUBPROTOCOL`、`HANDSHAKE_TIMEOUT`、认证限流常量、`close` 码表（1000/4400/4401/4406/4408/4409/4410/4429/4500）、再导出 |
| `conn/limits.rs` | 2.11 | `NodeLinkConfig`（§3 的七项 + `public_origin`）、`SessionLimits::negotiate`（夹在 `[schema 下限, §2.5 默认值]` → 「只下调」是构造期不变量）、`to_wire()`、供 WP5/WP6 消费的 6 个访问器、3 个单测 |
| `conn/wire.rs` | 2.10 | `judge()`（JSON 语法 → 信封/阶段 → 版本 → 序号 → type；序号只在「信封合法且等于期望值」时前进）、`pre_auth_allowed`、`error_body`、`feature_details`、`rate_limit_details` |
| `conn/handshake.rs` | 2.10 | `SUPPORTED_FEATURES`（features.json 的 mvp 5 条，逐条常量 + 注册表比对用例）、`negotiate_features`、`wire_files/transcript_features`、`peer_trust`（只由持久化快照组装）、`node_endpoint`、`connection_binding`、`pending_pairing`、`peer_kind`、4 个单测 |
| `conn/registry.rs` | 2.11 | `ConnectionHandle`（`send`/`pending`/`saturated`/`request_close`/`connection_id`/`node_id`/`client_ip`/`limits`）、有界发送队列（字节 + 条数双高水位）、`ConnectionRegistry`（`handles`/`handles_for_node`/`close_node`） |
| `conn/session.rs` | 2.10 + 2.11 | `NodeLinkConn`（装配、`with_route`、`registry`）、`WsEndpoint`（`WsHandler` 实现）、`Session` 的准入 → 握手 → 认证收尾 → 业务四段状态机；`MessageRoute`/`RouteOutcome` 分派口 |
| `conn/tests.rs` | 2.10/2.11/2.12 | 真实 loopback WSS + 自带极简客户端（掩码帧、close 码、binary）；17 个用例（见覆盖映射） |
| `node_link/mod.rs` | 骨架 | `pub mod conn;` + 再导出 |
| `local_admin/test_support.rs` | 测试设施 | `TrustStore::pairing_for` 替身、`FixedStore`（只有 `head()` 为真的存储替身）、`FakeIds`（确定性 uuid 生成器）、`FakeTrust::approve_node` 补写 `owned_peer_key` 等价物、`FakeTrust::audits()` 读入口、`TestWorld::with_store` |

## 冻结的公开形状（供 WP5/WP6/WP7）

```text
server::node_link::conn::NodeLinkConn
  new(core: Arc<UseCases>, authority: Arc<Authority>, config: NodeLinkConfig,
      registry: Arc<ConnectionRegistry>) -> Self
  with_route(self, route: Arc<dyn MessageRoute>) -> Self      # WP5/WP6 的组合根接线
  registry(&self) -> &Arc<ConnectionRegistry>                 # WP5 扇出 / WP6 撤销传播
server::node_link::conn::WsEndpoint::handler(conn: Arc<NodeLinkConn>) -> Arc<dyn WsHandler>
  # 组合根（WP7）：listener.register_ws(WS_PATH, WS_SUBPROTOCOL, WsEndpoint::handler(conn))

trait MessageRoute { async fn route(&self, session: &ConnectionHandle, message: &Envelope) -> RouteOutcome }
enum RouteOutcome { Claimed, Unclaimed }   # Unclaimed → 本层回 type_unsupported（绝不静默丢弃）

struct ConnectionHandle            # 认证成功后注册；方法都是同步 try_send 语义
  connection_id() -> &Uuid         # = node.challenge 的 connectionId（对端可见）
  node_id() -> &NodeId             # 对端 Access 节点
  client_ip() -> IpAddr
  limits() -> SessionLimits        # = node.ready.limits 的同值
  send<T: Serialize>(mt: MessageType, body: &T) -> Result<(), SendFault>   # 自动 messageId + 出站序号
  pending() -> PendingQueue { messages, bytes }
  saturated() -> bool               # 高水位（含「刚被拒过一条」）
  request_close(code: u16, reason: &'static str)   # 先排空已入队消息再发 close 帧

enum SendFault { HighWater, Closed, Encoding }   # HighWater = 调用方应停读新快照批次，不要重试

struct ConnectionRegistry
  new() -> Arc<Self>
  handles() -> Vec<Arc<ConnectionHandle>>
  handles_for_node(&NodeId) -> Vec<Arc<ConnectionHandle>>
  close_node(&NodeId, code, reason) -> usize
  active() -> usize

struct SessionLimits   # WP5：catalog_snapshot_batch_size()/resource_snapshot_batch_size()
                       # WP6：max_in_flight_commands()；队列与心跳由本层自用
struct NodeLinkConfig { max_message_bytes, catalog_snapshot_batch_size, resource_snapshot_batch_size,
                        max_in_flight_commands, max_pending_queue_bytes, max_pending_queue_messages,
                        heartbeat_interval_ms, public_origin }
```

一条**未注册路由时**的行为约定（WP5/WP6 注册后自动消失）：认证后到达的已知但未被认领的 v1 type 一律回
`link.error(nodelink.protocol.type_unsupported)` 并保持连接可用——不是静默丢弃，也没有引入方向表；
Owner→Access 方向的消息（如 `node.trust.revoked`）同样走这条分支。

## 覆盖映射：R → 用例（`specs/node-link-owner-server/spec.md`）

| R | 场景 | 用例（`crates/server/src/node_link/conn/tests.rs`） |
|---|---|---|
| R36 | 握手准入与版本/feature 协商 | `handshake_completes_and_enters_the_business_phase`、`an_envelope_version_other_than_v1_is_closed_with_4406`（4406）、`a_missing_required_feature_is_reported_with_details_and_closed`（4400）、`a_handshake_that_never_gets_a_hello_is_closed_with_4408`（真实等满 15 秒 + 断言关闭发生在窗口之后） |
| R37 | 正常握手完成 | `handshake_completes_and_enters_the_business_phase`：`node.challenge`（无连接字段、Owner 证明可由本机公钥在 `node-link-challenge/v1` 域验证）→ `node.ready`（连接字段 + 序号 1 + `catalogRevision` = 持久化水位 + `serverEpoch` + `limits`）→ 业务阶段 ping/pong |
| R38 | 认证前发送业务消息 | `a_business_message_before_hello_is_closed_with_4401`（`command.submit` 先到 → `type_unsupported` + 4401；无信任行/无审计/无连接登记） |
| R39 | 必需 feature 未满足 | `a_missing_required_feature_is_reported_with_details_and_closed`（`requiredFeatures` 含 Owner 不实现的 ID → `feature_required` + `details.features = ["node-link.node-rotation.v1"]` + 关闭；配对不被消费） |
| R40 | 节点双向认证与凭据状态 | `handshake_completes_and_enters_the_business_phase`（Owner 挑战证明验证 + Access proof 用持久化公钥验证通过 → 同一写集消费配对 + `node.authenticated` 审计） |
| R41 | proof 无效被拒绝 | `an_invalid_proof_is_closed_with_4401_and_audited`（篡改签名 → `proof_invalid` + 4401 + `node.auth_failed`(Failed) 且信任记录不变）、`a_proof_for_another_challenge_is_rejected_with_the_same_code`（nonce 不匹配同码） |
| R42 | 已撤销节点连接被拒绝 | `a_revoked_node_is_closed_with_4410`（签名有效但信任行已撤销 → `node_revoked` + 4410 + `node.auth_failed`(Denied)；验签材料作为墓碑保留） |
| R43 | 未知节点不泄露存在性之外的能力 | `an_unknown_node_gets_a_challenge_and_fails_as_node_unknown`（照常签发挑战、协商不降级 → proof 阶段 `node_unknown` + 4401；不创建信任行） |
| R44 | limits 只下调语义 | `node_ready_echoes_the_negotiated_limits_and_never_raises_them`（下调 3 项生效、2 项越界回落默认值、未出现的项取默认值） |
| R45 | 下调生效 | 同上 + `limits.rs` 的 3 个单测（含 `maxInFlightCommands = 8` 生效、上调一律回落） |
| R46 | 固定常量不可协商 | `protocol_constants_are_not_configurable`（15 s / 90 s 常量 + `NodeLinkConfig` 无对应键 + 任意配置都只能下调）、`a_handshake_that_never_gets_a_hello_is_closed_with_4408`（15 s 实测） |
| R47 | 信封与 connectionSequence 校验 | `envelope_and_sequence_violations_are_rejected_without_closing`（①–⑧ 逐条：跳号/回退/`connectionId` 不匹配 → `sequence_invalid`；缺 `connectionSequence`、未知信封字段、未知 body 字段 → `schema_invalid`；非法 JSON → `invalid_json`；未知 type → `type_unsupported`；每条之后都用 ping/pong 证明连接仍可用且序号未错位）、`a_binary_frame_is_closed_with_4400` |
| R48 | 序号回退被拒绝 | 同上 ①–③（含 `"1"` 在 `"2"` 之后） |
| R49 | post_mvp 消息显式拒绝 | 同上 ⑨（用 manifest 的 `valid/catalog-changed.json` 与 `valid/link-backpressure.json` → `type_unsupported` + 连接可用 + 序号不错位；不发 `link.backpressure`） |
| R50 | 未知字段被拒绝 | 同上 ⑤/⑥（信封与 body 两个层级） |
| R32 | 首次认证清除已批准配对（衔接侧） | `handshake_completes_and_enters_the_business_phase`（`approved → consumed` + `consumed_at` 非空 + 重新读快照不再有待消费配对；协商失败/认证失败路径断言配对仍为 `approved`） |
| R79 | 心跳与超时 | `heartbeat_round_trip_keeps_the_connection_alive`（`heartbeatIntervalMs = 1000` 驱动：`link.ping` 是 `node.ready` 之后首个出站消息、nonce 每次不同、原样回填 `link.pong` 后连接继续可用） |
| R80 | 心跳超时关闭 | `silence_beyond_the_window_is_closed_with_4408`（静默窗口经测试缝调成 2 秒 → 真实连接 4408；持久化状态不变；常量取值由 R46 用例锁定） |
| R81 | 慢连接不影响其他连接 | `a_saturated_connection_is_disconnected_without_affecting_its_peer`（队列下调到 1 条/1 KiB：投递方看到 `HighWater` + `saturated()` + 真有一条未写出消息；对照组连接照常往来；持续过慢后只断慢连接） |
| R82/R83 | 审计与日志边界（连接侧） | `handshake_completes_and_enters_the_business_phase`（`node.authenticated`：actor/`viaNode` = 对端、`target = Pairing`、`localPrincipalRef = null`、无 detail digest）、`an_invalid_proof_is_closed_with_4401_and_audited` / `a_revoked_node_is_closed_with_4410` / `an_unknown_node_gets_a_challenge_and_fails_as_node_unknown`（`node.auth_failed` 三种结果；错误 body 只含登记字段与固定短文本，不含任何凭据/秘密材料） |
| 验收「schema/fixture 漂移」 | — | `fixtures_are_consumed_by_the_handshake_and_error_layers`：逐条消费 `fixtures/node-link/v1/manifest.json` 的**全部** 49 份用例并分类（握手 body 族 4、`link.*` 合法 3 + 反例 1、业务族 39、配对 schema 2）；`valid` 的握手/link body 走类型化 DTO 解码 + JSON 往返保真，`invalid` 的 `link.ping` 必须被信封层拒绝，post_mvp 的 `valid` fixture 必须被显式拒绝，业务族的 body 级反例必须落在用例内登记的清单里（新增 fixture 未归类即失败） |

## 合同缺口与归属问题（需要主 Agent/reviewer 知晓）

### 1. `node.challenge` 不带 `catalogRevision`，但连接 transcript 包含它（**已上报 supervisor**）

- 事实：`docs/NODE_LINK_PROTOCOL.md` §9.3/§9.4 的两个连接 domain（`node-link-challenge/v1`、
  `node-link-proof/v1`）字段集合都含 tag 6 = `catalogRevision`；而 §12.2 的 `node.challenge` 字段表、
  `schemas/node-link/v1/handshake.schema.json` 的 `nodeChallenge` 与 `fixtures/node-link/v1/valid/node-challenge.json`
  都**没有**该字段，`identity-auth` 的 `NodeLinkChallenge`/`NodeLinkProof` 实现同样固定包含它。
- 后果：Access 侧无法在首次连接上构造/验证这两个证明（它拿不到 revision）：Owner 用 hello 时记录的值
  签挑战、验证明；Access 只有「上次 `node.ready` 缓存过 revision」时才对得上。
- 本 WP 现状：Owner 侧实现正确（同一个 revision 用于签与验），WP4 用例作为「缓存过 revision 的 Access」
  带外取本机水位来签名（`conn/tests.rs` 的 `send_proof` 与文件头说明）。**这不是本 WP 引入的缺口**，
  但会让 WP7 的全链路用例与真实第三方 Access 实现遇到同一个问题。
- 处置选项（本 WP 无权决定，未改 wire/schema/fixture）：① 给 `node.challenge` 增 `catalogRevision`；
  ② 从两个连接 domain 去掉 tag 6；③ 把「Access 缓存/带外获知 revision」写成既定口径（首次连接不可用）。

### 2. §2.5 的三个固定上限在 Node Link 路径上没有执行点

- `JSON nesting depth 64`、`单对象字段数 1024`、`单数组元素数 10000`（§2.5 表，均「不可下调」）目前在
  本仓库 Node Link 路径上无人执行（信封解码用 serde_json 的默认递归上限 128，body 用 serde：
  容器大小无上限）；`单条 WebSocket message 1 MiB` 由 WP2 的传输层以 `1009` 拒绝（本层不再补发第二个
  close 帧），`握手 15 s`/`静默 90 s`/`单 IP 10 次每分钟` 由本 WP 执行。
- 判断：这是**门禁归属**问题（若认为属 WP4，需要一个新的 JSON 限额扫描器或在协议 crate 内实现；
  若认为属 WP5/WP6 的 body 边界，则应在它们的任务里登记）。本 WP 未实现，也未在文档里声称已实现。

## 范围与判断偏离（逐条：原 → 新 → 理由）

### A. 实现选择（模块内部细节，代码注释已写明）

1. **per-IP 在途握手配额**（任务 2.10 的「评估项」结论）：任务书要求「评估是否补 per-IP 在途握手上限」。
   → **实现**：`MAX_IN_FLIGHT_HANDSHAKES_PER_IP = 4` + RAII 凭证（会话任何结束路径都释放），条目数由
   `MAX_TRACKED_IPS`（复用 `transport::net::ratelimit::MAX_TRACKED_KEYS`）封顶。理由：接入层的 TLS 握手池
   是全局的、没有按源地址的闸门，而认证限流只在会话第一步生效；4 的取值依据是「一次正常重连最多同时
   出现旧连接未确认关闭 + 新连接已建立 + 一次立即重试」，留一条余量。成本 ~40 行、无新依赖。
2. **认证限流的接入位置**：WP2 冻结的 `WsHandler` 没有「WS upgrade 之前的按路径准入钩子」→ 把闸门放在
   会话的第一步（任何帧都不再处理，4429 关闭）。效果等价：未认证连接不产生任何业务副作用。
3. **`node.ready.catalogRevision` 的来源**：现成水位没有专用的 catalog 计数器 → 取 `store.head()` 的
   `global_sequence`（跨重启单调的十进制串），并在 `conn/session.rs` 顶部与 handoff 登记为**临时口径**；
   WP5 冻结 catalog revision 语义时必须与本值同源（或按它自己的 D4 结论改由 WP5/WP7 接线）。
   `serverEpoch` 同源取 `head.server_epoch`（与 Sync 的 `serverEpoch` 同义）。
4. **未知节点的失败映射**：`Authority::verify_proof` 对所有原因只给一个 `HandshakeFailure`（`identity-auth`
   的 `public_class` 刻意收敛）。→ 适配器只用**持久化快照**区分：快照里没有可用公钥 → `node_unknown`
   （R43），其余 → `proof_invalid`（R41）；两者都以 4401 关闭，细节只进结构化日志。
5. **协议错误的 close 码**：binary 帧 → 4400、认证前业务消息 → 4401、版本 → 4406、证明无效/未知 →
   4401、已撤销 → 4410、`ScopeReduced` → 4409（v1 没有更贴切的码，按失败关闭）、限流 → 4429、
   静默/握手超时 → 4408、内部不可用 → 4500、慢消费者 → 1000（协议未规定，取正常关闭并留
   `node_link.slow_consumer` 日志）。
6. **序号与拒绝的关系**（实现期发现并修正）：`connectionSequence` 属于「对端发出的消息」。因此只要
   信封合法且序号等于期望值就**先记账**，即使随后因 post_mvp/未知 body 字段/未实现的 type 拒绝该消息；
   反之信封无法解析（非法 JSON、未知 type、缺连接字段）或序号不匹配时不记账。修正前的版本会让被拒消息
   之后的连接永久错位（一次 post_mvp 拒绝即触发后续 `sequence_invalid`），用例
   `envelope_and_sequence_violations_are_rejected_without_closing` 已锁定该行为。
7. **`WsError::MessageTooLarge` 不再补发第二个 close 帧**：WP2 的传输层已经按 §2.5 发过 `1009`，本层原
   实现会再发一个 4400。现在改为直接放弃连接（`Ending::TransportClosed`），并保留日志。

### B. 测试设施（都是测试专用，不改生产行为）

1. **`NodeLinkConn::with_test_windows`（`#[cfg(test)]`）**：90 秒静默与 30 秒慢消费者宽限在用例里不可
   等待，而它们又是**固定常量**（不属于可下调的 `node.ready.limits`）。→ 生产路径只有
   `SessionWindows::from_protocol()` 一个来源，`#[cfg(test)]` 构造器只让用例把窗口调短；常量取值由
   `protocol_constants_are_not_configurable` 与 R36 的 15 秒实测锁定，机制由两条真实连接覆盖。
   备选（paused clock）不可行：静默判定用 `std::time::Instant`，暂停 tokio 时钟不改变它。
2. **`local_admin/test_support.rs` 的替身补全**（否则测试世界无法跑握手）：`TrustStore::pairing_for` 替身、
   `FixedStore`（唯一「真」的方法是 `head()`，其余仍 `unreachable!`，避免替身比真实存储宽容）、
   `FakeIds` 从「全 unreachable」改为确定性 uuid 生成器（`UseCases::ids()` 需要它）、`TestWorld::with_store`、
   `FakeTrust::audits()` 读入口。
3. **`FakeTrust::approve_node` 补写验签材料**：真实存储在配对批准时把 `owned_pairing_peer.public_key`
   转入 `owned_peer_key`（§11.6 第 4 条），替身此前漏了这一步，导致 `peer_key` 永远为空——那样
   「未知节点」与「已配对节点」在用例里无法区分。已按存储层语义补上。
4. **自带极简 WebSocket 客户端**：`transport::net::test_client` 是 WP2 的私有测试模块（不可从 `conn` 的
   测试引用），且本模块需要断言语义（close 码、binary、帧掩码、出站序号）。→ 在 `conn/tests.rs` 内实现
   ~180 行客户端，不修改 WP2 文件、不新增依赖。
5. **`server/local_admin/test_support.rs` 随 seam 一起提交在 `a811932`**：`TrustStore` 新增方法后，
   替身必须同批实现，否则该提交的 workspace 测试无法编译（分批提交的目的是不丢进度，不是让中间提交
   留下不可编译状态）。

## 不确定项与残留风险（供 reviewer 判定）

| 项 | 说明 | 本 WP 的处置 |
|---|---|---|
| `catalogRevision` 的最终语义 | 见「合同缺口」第 1/3 条：wire 不带该字段 + 本 WP 用全局水位充当 | 登记为临时口径，WP5 冻结时同步核销 |
| §2.5 三个 JSON 限额 | 见「合同缺口」第 2 条 | 未实现、未声称 |
| 「Owner→Access 方向消息」的判定 | 本层不看方向，未注册路由的类型一律 `type_unsupported` | 显式拒绝（非静默丢弃），WP5/WP6 注册后自然收敛 |
| 慢消费者的断开 close 码 | 协议未规定；取 1000 + 结构化日志 | 与 Access 的「老连接断开」语义一致；如需专用码请裁决 |
| `messageId` 重复 | R47 的「不重放业务结果」由命令层（WP6 的幂等）承担，本层只分配不改写 | 本层不缓存 messageId；如 reviewer 认为要在此拒绝，需新的裁决 |
| 审计行去重 | 首次成功认证的 `node.authenticated` 由 `consume_pairing` 写集写入；重复成功认证由适配器补写 | 两条路径都覆盖，重复认证用例属后续任务（本轮无「重复成功认证」的断言） |

## 依赖包含关系（1.4 / Dependency Handoffs 的 WP4 行）

- 消费 WP2 的冻结形状：`NetListener::{bind, register_ws, local_addrs, serve}`、`WsConnection::{recv, send_text, close}`、
  `WsHandler`（`self: Arc<Self>`）、`PeerInfo::{client_ip, shutdown}`、`SlidingWindowLimiter::{check, RateLimit}`、
  `WsError::{MessageTooLarge, Transport}`、`WsMessage::{Text, Binary}`、`transport::net::ratelimit::MAX_TRACKED_KEYS`。
  **未改** WP2 的任何文件。
- 消费 WP3 的配对事实：`PairingHttp` 的批准路径经真实本地通道（`node.pair.begin` → 认领 → `node.pair.confirm`）
  在用例里复现，`approved` 记录的 `host_binding` 是 `node.challenge` 绑定的权威值。
- 消费 WP3 合同扩展：`Authority::{hello, verify_proof, complete_auth}`、`ConnectionKind::NodeLink`、
  `PeerTrust`/`CredentialStatus`、`HandshakeFailure::audit()`。
- 提供 2.28 的 seam 给 WP4 自身；提供 `MessageRoute`/`ConnectionRegistry`/`ConnectionHandle` 给 WP5/WP6；
  提供 `NodeLinkConn`/`WS_PATH`/`WS_SUBPROTOCOL` 给 WP7 的组合根。

## 未执行项（不粉饰）

- 未执行 `[PV5]`（Windows 端到端 / 跨实现互操作）：属任务 3.7/3.9 与本机端到端轮次，本 WP 只声称 `[PV3]`；
  其中 `catalogRevision` 缺口会让脚本化 Access 需要同一处带外取值。
- 未执行 `cargo deny` / `gitleaks`（只在 CI）：本机未安装对应二进制，与 WP2/WP3 handoff 的口径一致。
- 未接线到组合根（`app`）：`NodeLinkConn` 只被注册到测试 listener；生产 listener 绑定与关闭序列属 WP7。
  因此本 WP 不声称「Node Link 已能在真实 daemon 上服务」。
- 未做真正的第三方 Access 互操作（没有 Rust 侧 `node-link-client`）：证明的 Access 一半由测试内固定私钥
  （标量 1）扮演，与 `identity-auth` 的固定向量语义一致，但不等于跨实现验证。
