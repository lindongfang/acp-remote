# WP3 Handoff — 配对 HTTP（claim/status）与配对通道 seam（`node-link-owner` / DU1）

## Shared Report

- **task_id**：2.27（配对通道 seam 补全）、2.7（claim 端点）、2.8（status 端点与限流）、2.9（交付前局部验证）；行 1.4 的依赖接入口径见「依赖包含关系」。
- **role / phase**：coder / implement。
- **agent_context**：worker 子 Agent（本机 worktree 独占，未继承其它 WP 的实现对话）。本 WP 是 W2 波次的唯一写入者，未与其它 Agent 并发。工作目录 `D:\Project\acp-remote-wt\node-link-owner`（分支 `agentic/node-link-owner`）。
- **target_revision**：`84678e4`（本 WP 交付提交）；两个前置提交 `dec2339`（2.7）、`259920a`（2.27）。起点 `13f0a16`（WP3 合同扩展之后、本 WP 之前）。
- **scope**：
  - 2.27：`crates/identity-auth/src/{pairing.rs,types.rs,lib.rs}` + `tests/pairing.rs`、`crates/core/src/use_cases.rs`、`docs/IDENTITY_AND_AUTH_CONTRACT.md`、`docs/CORE_PORTS_AND_STORAGE.md`；
  - 2.7/2.8：`crates/server/src/node_link/**`（新增）、`crates/server/src/lib.rs`、`crates/server/Cargo.toml` + `Cargo.lock`（`p256` dev-dependency）、`crates/server/src/local_admin/test_support.rs`（测试设施）。
  - **未改**任何权威规划工件（`plan.md`/`tasks.md`/`verification.md`）、`schemas/**`、`compatibility/**`、`fixtures/**`、`crates/app/**`、`crates/server/tests/**`（WP7）、`crates/server/src/transport/**`（WP2）。
- **changes**：见「改动与需求映射」。
- **checks**：`[PV3]` 的 fmt / clippy(`-p server`) / `cargo test -p server` / `rg unsafe` 全绿，另有 2.27 的两个受影响 crate 的测试与 `npm run check`；原始输出 `reports/wp3-pairing-http.log`。`[PV5]` 的本机轮次与本 WP 无关（无 `cfg(windows)` 分支）。
- **issues**：无阻断项。需要主 Agent 与 reviewer 知晓的范围事实见「范围与判断偏离」（6 条，含 2 条用户已裁决、3 条测试设施与依赖的连带、1 条 WP2 冻结形状的后果）。
- **result**：**PASS**（本工作包检查全部满足）。**不代表**独立 review（3.6）、`[PV5]`/E2E 或合并已完成。
- **evidence_paths**：`reports/wp3-pairing-http.log`（逐命令原始输出）、本文件。
- **resource_cleanup**：用例只绑定 `127.0.0.1:0` 随机端口（`Harness::stop` 触发关闭排空并释放端口，未起后台进程）；用例自建的临时目录由 `TestWorld`/`TestCertificate` 的 `Drop` 清理；未使用数据库服务、容器、固定端口或外部账号；`target/` 作为缓存保留（被 git 忽略）。

## handoff_index

```yaml
handoff_index:
  - task_id: "2.27"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "259920a"
    evidence_type: CHECK
    evidence_id: NOT_APPLICABLE
    report_path: "reports/wp3-pairing-http.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "pairing 通道 seam（identity-auth 两个新入口 + core 只读入口 + §5.1 顺序规则）在 259920a 上实测：cargo test --locked -p core -p identity-auth --all-features 全绿（core 102、identity-auth 15/20/30/2/9/5/2，含 4 个新 seam 用例与 2 个新 core 用例）；cargo fmt --all -- --check 退 0；cargo clippy --locked --workspace --all-targets --all-features -- -D warnings 退 0（pre-commit 轮次）；npm run check 退 0（10 道，含 check:docs/check:drift）。正式 [PV3] 执行归 2.9/3.5，本行只声称实现的局部检查。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.7"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "84678e4"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/wp3-pairing-http.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 84678e4 上执行 fmt（exit 0）、clippy -p server --all-targets --all-features（exit 0）、cargo test --locked -p server --all-features（lib 189 全绿，其中 node_link 25；集成目标 14/6/4/4 全绿）、rg -n unsafe crates/server/src（零命中）。R17/R19–R25/R30/R31 的 claim 侧场景由 node_link::tests 的 9 个用例覆盖（见本报告映射表）；[R19]/[R20] 的 201 与 ownerProof 验签在真实 loopback listener 上断言。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.8"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "84678e4"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/wp3-pairing-http.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一轮 [PV3]；R26–R29/R32–R35 的 status 侧场景由 node_link::tests 的 4 个用例 + pairing::tests 的 3 个限流/映射用例覆盖；consumed 路径经真实的 complete_auth → consume_pairing 收尾推进（不是预置终态）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.9"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "84678e4"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "reports/wp3-pairing-http.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP3 交付前局部验证（命令集同 2.6）：fmt/clippy(-p server)/cargo test -p server/rg unsafe 全绿且无零用例、全跳过；额外跑了 2.27 的受影响 crate 测试与 npm run check。日志第 0–5 节是逐条原始输出。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.9"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "84678e4"
    evidence_type: VALIDATION
    evidence_id: NOT_APPLICABLE
    report_path: "reports/wp3-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "交付形状冻结：`server::node_link` 的公开面（见本报告「冻结的公开形状」）与 wp3-pairing-http.log 同一提交；**不作为** [PV1]/[PV2] 的证据行（全 workspace `npm run verify` 属 3.5/2.24）。"
    source_evidence: NOT_APPLICABLE
```

## 改动与需求映射

### 2.27（seam 补全，提交 `259920a`）

| 文件 | 内容 |
|---|---|
| `crates/identity-auth/src/types.rs` | 新增 `PairingRequestMaterial { server_nonce, pairing_request_id }`（**非秘密**，与 secret 同生同灭） |
| `crates/identity-auth/src/pairing.rs` | 新增 `Authority::verify_node_link_pairing_status(&NodeLinkPairingStatus, &PairingProof) -> Result<(), PairingError>`（按 `pairingId` 取内存 secret；缺失 → `SecretUnavailable`；`ownerNodeId` 必须为本节点 → 否则 `ClaimMismatch`；HMAC 失败 → `Proof(Hmac)`）与 `Authority::pairing_request_material(&PairingId) -> Option<PairingRequestMaterial>` |
| `crates/identity-auth/src/lib.rs` | 再导出 `PairingRequestMaterial` |
| `crates/identity-auth/tests/pairing.rs` | 4 个新用例：status proof 往返、foreign owner 拒绝、secret 缺失（restart）、请求材料随 secret 同生同灭 |
| `crates/core/src/use_cases.rs` | 新增 `PairingChannelView { record, peer, node }` 与只读入口 `UseCases::pairing_channel_view(actor, id)`（gate 仍是 `require_pairing_access`；`node` 只在 `approved`/`consumed` 时读该对端的 `NodeKind::Access` 行）；2 个新用例（绑定式读取 + 已授予集合来源） |
| `docs/IDENTITY_AND_AUTH_CONTRACT.md` | §4.3 补「非秘密请求材料与 secret 同生同灭」；§5.1 登记两个新公开入口 + claim 路径的读取顺序规则（绑定 actor 只读先于 proof 校验、写集只在 proof 通过后提交、401 不泄露差异） |
| `docs/CORE_PORTS_AND_STORAGE.md` | §4 的 `PairingChannel` 行增 `pairing_channel_view` 与 actor 规则段；§10 增一条 `[已裁定]`（2026-09-26 seam 补全） |

### 2.7（claim，提交 `dec2339`）

| 文件 | 对应任务点 | 内容 |
|---|---|---|
| `crates/server/src/node_link/mod.rs` | 骨架 | 模块职责/依赖纪律文档、`pub mod pairing`、`#[cfg(test)] mod tests`、再导出 `CLAIM_PATH`/`STATUS_PATH`/`PairingHttp`/`PairingHttpConfig`；**不注册路由、不绑 listener**（WP7） |
| `crates/server/src/node_link/pairing.rs` | 2.7 全部 | `PairingHttp::{new, claim_handler}`、claim 的原子检查链（限流 → Content-Type → 解码 → 只读该配对 → 410/409 → endpoint host 403 → `Authority::verify_claim` → 写集/幂等 → 201）、`ClaimInput`、`claimed_pairing`、`claim_response_body`、四个安全头、`httpError` 组装（含 `correlationId`） |
| `crates/server/src/lib.rs` | 骨架 | crate 文档的落地清单加入 `node_link`（`sync`/`acp_facade` 仍写明未落地）；`pub mod node_link;` |
| `crates/server/src/node_link/tests.rs` | R17/R19–R25/R30/R31 | 真实 loopback listener + 自带最小 HTTP/1.1 客户端；用例见映射表 |
| `crates/server/Cargo.toml` + `Cargo.lock` | 测试设施 | `p256` 进入 `[dev-dependencies]`（与 identity-auth 同一 workspace 版本与 feature） |
| `crates/server/src/local_admin/test_support.rs` | 测试设施 | `FakeKeystore::sign` 用标量 1（其公钥 = `test_public_key()` 的基点 G）真实签出 P-256 签名，使 ownerProof 能在用例里被同一公钥验证 |

### 2.8（status + 限流，提交 `84678e4`）

| 文件 | 对应任务点 | 内容 |
|---|---|---|
| `crates/server/src/node_link/pairing.rs` | 2.8 全部 | `status_handler`/`status`（一律 200 + 五状态；proof 校验 401；terminal-without-secret 回退；`approved` 的 node/owner 块；`requestNonce` 重放缓存）、`PairingIdWindow`（60/分钟/`pairingId`）、`StatusInput`、`wire_status`、`decode_status`、`proof_invalid`/`ok_body`；模块内 3 个限流/映射/缓存单测 |
| `crates/server/src/node_link/tests.rs` | R26–R29/R32–R35 | 4 个 status 用例（五状态、401、重试原响应、限流+安全头/无 secret） |
| `crates/server/src/local_admin/test_support.rs` | 测试设施 | `FakeTrust::consume_pairing` 按存储层语义落地（consumed 路径走真实 `complete_auth` → `consume_pairing`） |
| `crates/server/src/node_link/mod.rs` | 文档 | 两条固定限流的说明 |

## 冻结的公开形状（供 WP4/WP7 消费）

```text
// crates/server/src/node_link/pairing.rs（值对象来自 transport::net 与两个协议/core crate，不重复导出）
pub const CLAIM_PATH: &str = "/node-link/v1/pairing/claim";     // §13.2
pub const STATUS_PATH: &str = "/node-link/v1/pairing/status";   // §13.3

/// 组合根注入的配置快照（本模块不读配置文件）。
pub struct PairingHttpConfig {
    pub public_origin: Option<String>,   // = daemon.public_origin（CONFIG_REFERENCE.md §1）
}

pub struct PairingHttp;
impl PairingHttp {
    /// core 用例面 + 身份状态机 + 配置快照（三者都是组合根持有）。
    pub fn new(core: Arc<UseCases>, authority: Arc<Authority>, config: PairingHttpConfig) -> Self;
    /// 注册到 CLAIM_PATH 的 HttpHandler。
    pub fn claim_handler(self: &Arc<Self>) -> Arc<dyn HttpHandler>;
    /// 注册到 STATUS_PATH 的 HttpHandler。
    pub fn status_handler(self: &Arc<Self>) -> Arc<dyn HttpHandler>;
}

// 固定的 limit（§2.5，不可配置、不出现在配置键里）：
//   claim  10 次/分钟/IP        —— 键 = PeerInfo::client_ip，用 transport::net::SlidingWindowLimiter
//   status 60 次/分钟/pairingId —— 键 = pairingId，用本模块的 PairingIdWindow（同窗口语义）
// 超限一律 429 + code nodelink.resource.rate_limited + details.retryAfterMs（registry 唯一登记字段）。

// 2.27 的 seam（WP4 会用到后半段）：
//   identity_auth::Authority::verify_node_link_pairing_status(&NodeLinkPairingStatus, &PairingProof)
//   identity_auth::Authority::pairing_request_material(&PairingId) -> Option<PairingRequestMaterial>
//   core::use_cases::{PairingChannelView, UseCases::pairing_channel_view}
```

**WP7 接线要点**：`let pairing = Arc::new(PairingHttp::new(core, authority, PairingHttpConfig { public_origin }));` → `listener.register_post(CLAIM_PATH, pairing.claim_handler())?; listener.register_post(STATUS_PATH, pairing.status_handler())?;`。两个 handler 必须来自**同一个** `PairingHttp`（限流与重放缓存是端点级状态）。WP4 的握手收尾用 `Authority::complete_auth` → `UseCases::consume_pairing`（本轮已经在用例里跑通该顺序）。

## 覆盖映射：R17/R19–R35 → 用例（`specs/node-link-pairing-http/spec.md`）

| R | 场景 | 用例（`crates/server/src/node_link/`） |
|---|---|---|
| R17 | 配对请求体超限 | `tests::claim_body_over_the_transport_limit_is_rejected_with_413`（真实 listener，80 KiB → 413） |
| R19 | claim 成功路径 | `tests::claim_enters_pending_confirmation_and_returns_a_verifiable_owner_proof` |
| R20 | 合法 claim 进入待确认 | 同上（201 + `status=pending_confirmation` + `expiresAt` + `pairingRequestId`/`serverNonce` 来自本机材料 + ownerProof 用 `ownerPublicKey` 验签通过 + 记录转 `pending_confirmation` + 对端行固定） |
| R21 | 重复 claim 被拒绝 | `tests::repeated_claim_from_another_access_node_is_rejected_with_409`（+ 对端行不被替换 + `pairing_count == 1`） |
| R22 | 失败语义与状态码 | `tests::malformed_claim_body_is_rejected_with_400`（400 + invalid_json/schema_invalid，含 Content-Type）、`unknown_pairing_is_rejected_with_404`、`endpoint_host_outside_the_configured_origin_is_rejected_with_403`、`claiming_without_a_configured_origin_is_rejected_with_403`、`expired_pairing_is_rejected_with_410`、`repeated_claim_..._409` |
| R23 | proof 无效不泄露差异 | `tests::invalid_proof_is_unauthorized_without_leaking_the_difference`（三种失败形状逐字段相等，`details == {}`，状态不变） |
| R24 | 相同内容重试幂等 | `tests::retrying_the_same_claim_returns_the_original_pairing_request`（同 `pairingRequestId`/`serverNonce`、不建第二条记录；不同内容 → 409） |
| R25 | 过期配对返回 410 | `tests::expired_pairing_is_rejected_with_410` |
| R26 | status 查询语义 | `tests::status_reports_the_five_business_states_with_200`、`status_with_an_invalid_proof_is_unauthorized` |
| R27 | 五状态都以 200 表达 | `tests::status_reports_the_five_business_states_with_200`（pending_confirmation/approved/rejected/expired/consumed，`approved` 断言字段数 = 6 且不签发凭据） |
| R28 | status proof 无效 401 | `tests::status_with_an_invalid_proof_is_unauthorized`（篡改 proof、另一把 secret、未登记 pairingId，均 401 且无 `status` 字段） |
| R29 | 重试复用 nonce 返回原响应 | `tests::status_retry_with_the_same_nonce_returns_the_original_response`（批准后用原 nonce 仍得原 body；新 nonce 得最新状态） |
| R30 | 安全响应头与凭据边界 | `tests::every_pairing_response_carries_the_four_security_headers`、`status_responses_carry_the_security_headers_and_no_secret` |
| R31 | 响应头齐全 + 无 secret | 同上 + `tests::pairing_responses_never_carry_the_pairing_secret`（成功与错误 body 都不含 secret 文本） |
| R32 | secret 按期清除 | `tests::status_reports_the_five_business_states_with_200`（expired：`due_pairings` 清 secret 后 `has_secret == false`，status 200 + expired；consumed：`complete_auth` 提前清除，status 200 + consumed）+ `identity-auth` 的 `node_link_status_proof_requires_the_in_memory_secret`、`pairing_request_material_is_readable_until_the_secret_is_cleared` |
| R33 | 配对限流 | `tests::claim_rate_limit_returns_429_after_ten_attempts_per_ip`、`tests::status_rate_limit_returns_429_after_sixty_queries_per_pairing`、`pairing::tests::pairing_id_window_*` |
| R34 | claim 超限 429 | `tests::claim_rate_limit_returns_429_after_ten_attempts_per_ip`（同 IP 第 11 次 429 + `retryAfterMs` + 状态不推进） |
| R35 | status 超限 429 | `tests::status_rate_limit_returns_429_after_sixty_queries_per_pairing`（第 61 次 429） |

**secret 清除的 WP4 衔接点**（R32 的后半）：`Authority::complete_auth(fact, Some(pairing), at)` 会在**已批准**配对上清除内存 secret 并返回 `consume_pairing = Some(id)`；WP4 必须在同一事务用 `UseCases::consume_pairing(&actor, &id)` 落盘后才发 `node.ready`。本轮用例已按这个顺序跑通（`status_reports_the_five_business_states_with_200` 的第 ⑤ 段），但**握手侧的接线**（谁调用、失败即关闭）属 WP4（2.10）。

## 范围与判断偏离（逐条：原 → 新 → 理由）

### A. 2 条已获 supervisor 裁决（本轮上报后批准）

1. **扩展 D12 的 seam**（identity-auth 两个新公开入口 + core 一个只读入口 + §5.1 顺序规则 + §4/§10 同步）：`node-link-owner` 的 D12 只覆盖了 core 的 Actor/用例面与 storage 的 v3 词表，配对 HTTP 端点无法完成 status proof 校验、也读不到幂等比对与已授予集合。supervisor 批准按方案 A 补全，并要求 status 的 `approved` 取**已授予**集合（本实现即如此）。
2. **status 在 secret 已清除时的语义**：R27/R32（`consumed`/`expired` 也要 200 + 状态）与 R26/R28（proof 无效 → 401）只在下面这一种读法下同时成立——**secret 仍在时一律先验 proof（失败即 401，终态也一样）；secret 已清除时只对终态（consumed/expired/rejected）回 200 + 该状态，非终态回 401**。理由是 `consumed ⟹ secret 已清除`（`complete_auth` 是唯一入口），不存在「consumed 且能验 proof」的样本；`rejected` 在窗口内仍拿 secret，因此仍要验 proof。已按该读法实现并在此登记，请 reviewer 按同一口径核对。

### B. 1 条 WP2 冻结形状的后果（未改 WP2 文件）

3. **`transport::net` 的限流器键固定为 `IpAddr`**：§2.5 的 status 限流键是 `pairingId`，因此 `server::node_link::pairing` 内实现了同构的 `PairingIdWindow`（同窗口语义、同 `MAX_TRACKED_KEYS` 上限、同失败关闭），并各自有用例。**没有**把 `SlidingWindowLimiter` 参数化（那要改 `crates/server/src/transport/net/**`，属 WP2 的写入范围）。若 reviewer/主 Agent 认为应当合并成一个泛型实现，改动约 15 行且要把 WP2 纳入复验范围。

### C. 3 条测试设施与依赖的连带（都是测试专用，不改生产行为）

4. `crates/server/src/local_admin/test_support.rs`
   - `FakeKeystore::sign`：原来 `unreachable!`（local_admin 的既有路径从不签名）；claim 成功路径必须真的签出 P-256 签名，ownerProof 才能在用例里被 `ownerPublicKey` 验证（R20）。用的私钥是标量 1，其公钥正好是 `test_public_key()`（基点 G）——**不是**密码学材料，只存在于 `cfg(test)`。
   - `FakeTrust::consume_pairing`：2.25 留下的 `unreachable!` 桩改为按存储层语义（§11.6 第 8 条）实现，使 status 的 `consumed` 路径能在真实的 `complete_auth` → `consume_pairing` 上跑通（而不是预置终态）。
   - 两处都不改变 local_admin 的任何既有行为（其 13 个测试与整套 server 测试全绿）。
5. `crates/server/Cargo.toml` + `Cargo.lock`：`p256` 进入 `[dev-dependencies]`（与 core/identity-auth 同一 workspace 版本；`identity-auth` 已经以 `ecdsa` feature 依赖它，因此图上没有新版本）。`Cargo.lock` 只在 `server` 包的依赖表加一行。
6. `crates/server/src/node_link/tests.rs` 自带一个约 70 行的最小 HTTP/1.1 客户端，而没有复用 `transport::net::test_client`：后者是 `transport::net` 的**私有** `#[cfg(test)] mod`，复用需要把它的可见性改成 `pub(crate)`（又一次动 WP2 的文件）。用例因此自己驱动真实 loopback listener 上的请求与响应解析。

### D. 实现选择（模块内部细节，已在代码注释里写明）

7. **状态类拒绝的错误码**：Node Link v1 的错误码词表（24 个）里没有配对专属码，因此 400 用 `protocol.invalid_json`/`protocol.schema_invalid`、401/403/404/409/410 统一用 `nodelink.auth.proof_invalid`（§13.4 对「不同 claim 内容返回 409」也正是要求该 code），429 用 `nodelink.resource.rate_limited` + `details.retryAfterMs`（registry 唯一登记的 details 字段），500 用 `nodelink.internal.unavailable`；`retryable` 一律取 `ErrorCode::default_retryable()`。HTTP 状态码才是 §13.4 规定的对外区分点。
8. **claim 的请求集合**：Node Link 的 claim 请求不带 scopes/grants，因此 `ClaimFields.requested` 与 `ClaimedPairing.requested` 都取配对行**登记的**集合（与 `local_admin` 读回对端事实的口径一致）；幂等比对因此两侧同源。对端行上的 `requested` 不落库（存储层保留登记值）。
9. **claim 的证明入口**：`ownerNodeId` 不参与字段映射——证明 transcript 的 `ownerNodeId` 由状态机固定用本节点装配（`verify_claim_proof`），声明了别的 owner 的 claim 必然验不过 → 401。
10. **重放缓存**：`(pairingId, requestNonce) → 首次 200 响应体`，只在身份判定之后读取（401 路径不回业务状态），条目上限 `MAX_STATUS_REPLAYS = 1024`（先入先出淘汰）；进程内、不持久化（§13.3 的重试是连接级语义）。
11. **未登记的 `pairingId`**：status 端点一律 401（不区分存在性；§13.4 的 404 是 claim 端点的行），claim 端点按任务书的顺序先判存在性 → 404。

## 不确定项与残留风险（供 reviewer 判定）

1. **四个安全头不覆盖接入层在调用处理器前产生的响应**：413（请求体超限）与 Host 不匹配的 400 由 `transport::net` 的通用路径直接产生（它不知道这是配对端点），因此**不带**这四个头。要让 R30/R31 覆盖这两个状态码，需要 WP2 按 path 附加响应头（本 WP 无法实现）。本 WP 的证据覆盖：处理器产生的全部响应（201/400/401/403/404/409/410/429/500）都带四个头。
2. **`status` 的 `consumed` 窗口依赖 R32 的读法**：若 reviewer 认为「secret 已清除后一律 401」更安全，则 R27 的 `consumed` 场景无法满足（`consumed ⟹ secret 已清除`）。两条要求的取舍已在上文 A.2 说明。
3. **`PairingIdWindow` 与 `SlidingWindowLimiter` 是两份实现**：语义一致、各有边界用例，但存在漂移风险（见 B.3 的合并建议）。
4. **`FakeKeystore::sign` 的私钥是标量 1**：只用于让签名在用例里可验证；若将来有人把它用在非测试路径会立刻被 `#[cfg(test)]` 挡住（`test_support` 模块本身是 `#![cfg(test)]`）。
5. **`cargo-deny`/`gitleaks` 与全 workspace `verify` 未在本机执行**：前者只在 CI 生效，后者属 3.5/2.24。

## 依赖包含关系（1.4 / Dependency Handoffs 的 WP3 行）

- 上游：WP2 冻结的 `transport::net` 形状（`NetConfig`/`NetListener::bind|register_post|local_addrs|serve`、`HttpHandler`/`HttpRequest`/`HttpResponse`/`PeerInfo`、`SlidingWindowLimiter`/`RateLimit`/`MAX_TRACKED_KEYS`、`Shutdown`）只经公开项消费；WP3 合同扩展（2.25/2.26）的 `Actor::PairingClaimant`、`claim_pairing`/`pairing`/`consume_pairing`、`TrustStore::consume_pairing`、v3 词表只经 `core::use_cases`/`core::ports` 消费。
- 包含关系核对：`cargo build -p server`/`cargo test -p server` 只经上述公开项编译；`crates/server/src/node_link/**` 不 import `axum` 的路由/提取器（只用其再导出的 `StatusCode`/`HeaderMap`/`Bytes` 等值类型，WP2 的模块文档明确该路径），不查 SQLite，不调用 `local_admin`（测试里复用 `local_admin::test_support` 是同一 crate 的测试设施，生产路径不跨 adapter）。
- 失效条件：`transport::net` 形状变化 → 本 WP 的 `[PV3]` 复验；2.27 的 seam 签名变化 → 复验并同步 §5.1/§4 登记。

## 未执行项（不粉饰）

1. `npm run verify`（全 workspace `cargo test`）属 3.5/2.24，本 WP 只跑 `-p server`、`-p core`、`-p identity-auth` 与 `npm run check`。
2. `[PV5]` 本机轮次不涉及本 WP（`node_link` 没有 `cfg(windows)` 分支）；`#[cfg(unix)]` 的既有集成目标在本机不编译，由 Linux CI 覆盖。
3. 独立 review（3.6）与 `[PV4]`（WP7 的组合根接线）尚未执行——本报告不声称它们通过。
4. `crates/app` 未接线：`daemon.listen` 仍不会注册这两个 path（2.21）。
