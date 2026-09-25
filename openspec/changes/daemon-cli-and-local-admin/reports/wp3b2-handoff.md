# coder 报告 · WP3b2（`server::local_admin` 设备/节点配对方法编排 + WP3 交付前局部验证）

- task_id: `2.11`（配对方法编排）、`2.13`（WP3 交付前局部验证）
- role: coder
- phase: implement
- stage: work-package
- agent_context: 独立子 Agent（worker / coder 角色），主 worktree 的分支 `feat/daemon-cli-and-local-admin`；只继承任务单给出的契约输入（`docs/LOCAL_ADMIN_PROTOCOL.md` §5.3/§5.4/§6、`docs/SECURITY_DESIGN.md` §9.4/§9.5/§10.2/§14、两份协议文档的配对章节、`tasks.md` 2.11/2.13、`core::use_cases`/`core::ports`/`core::model`、`identity-auth` 的配对状态机与授权展开、WP3b1 交接的冻结形状）与主 Agent 对 D1/D2 的裁决，不继承规划阶段对话；本变更各波次串行（同一时刻只有一个写入者）
- base_revision: **`a3a0b47`**（开工时的 HEAD；开工核对时 HEAD 为 `8dd24e4`，其间主 Agent 追加了 `87db7b5`/`305a7e8`/`a3a0b47` 三条规划/证据提交，均未触及 `crates/server/src`）
- target_revision: **`eef75f4`**（本 WP 的代码提交）；交接报告单独一条 `docs(server)` 提交
- scope（写入范围）：`crates/server/**`（含 `Cargo.toml`）、`Cargo.lock`、`openspec/changes/daemon-cli-and-local-admin/reports/`。**未改**任何其它 crate、`docs/`、`schemas/`、`fixtures/`、`vendor/` 与规划文件（`plan.md`/`tasks.md`/`verification.md`/`design.md`/`proposal.md`/`specs/`）
- result: `PASS`（本工作包的检查与交付条件全部满足；**不代表**独立 review（RV1）、WP4 的端到端（PV4）或合并已完成）

## 提交与交付对应

| 提交 | 类型 | 内容 | 覆盖任务 |
| --- | --- | --- | --- |
| `eef75f4` | `feat(server)` | 8 个文件、+3452/−53：新增 `local_admin/pairing.rs`（816 行），扩展 `params.rs`/`router.rs`/`view.rs`/`test_support.rs`/`mod.rs`/`Cargo.toml`/`Cargo.lock` | 2.11、2.13 |
| 本报告提交 | `docs(server)` | `reports/wp3b2-handoff.md`（本文件） | 2.11/2.13 的交接 |

代码提交经 `.husky/pre-commit`（`cargo fmt --check` + `npm run check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`，三步全过）与 `commitlint` 通过；**未使用** `--no-verify`、未强推、未改已有提交。代码提交后 `git status --porcelain` 为空（报告提交后同样应为空）。

## D1/D2 裁决的落实（主 Agent 2026-09-25 裁定）

- **D1（canonical origin）**：`PairingSessions::new(authority, public_origin, closer)` 的 `public_origin` 由 WP4 从 `daemon.public_origin` 注入；server 不读配置、也不经 `DaemonControl::status()` 取字段。`None` 时 `device.pair.begin` 与 `node.pair.begin(mode = "owner")` 在**任何副作用之前**回 `local.unavailable`（`error.message` 与代码注释都写明「成因是 `daemon.public_origin` 未配置，重试前需先配置」），不伪造 URL、不回落 localhost、不创建配对行/内存态/审计。有测试覆盖（`pairing_methods_fail_closed_without_a_public_origin`）。
- **D2（QR payload 组装）**：`crates/server` 新增 `sync-protocol` + `node-link-protocol` 生产依赖（§5 矩阵 server 行本就是 ✓，`check:boundaries` 通过），用两边的 `QrPayload` 类型 + `serde_json` 序列化 `#data=`，**不复制 wire 形状、不手写键名**；`Cargo.lock` 相应新增 server 的两条依赖行（+2 行）。两条 URL 都补了「能被对应 `QrPayload` 反序列化回来」的往返测试（`device_url_round_trips_through_the_sync_qr_payload`、`node_url_round_trips_through_the_node_link_qr_payload`），并断言 URL 里的 `pairingSecret` 与落库摘要一致。

## WP4 要实现的形状（本 WP 冻结）

```rust
use server::local_admin::{ConnectionCloser, NoConnections, PairingSessions, LocalAdminDeps};

// 1) 撤销后关闭 active connection 的窄钩子（server 不持连接表）
#[async_trait::async_trait]
pub trait ConnectionCloser: Send + Sync {
    async fn close_device(&self, device: &core::model::DeviceId);   // 无连接时 no-op
    async fn close_node(&self, node: &core::model::NodeId);         // 关闭连接并停止本地重连
}
pub struct NoConnections;   // 两个动作都是 no-op 的实现（本切片没有网络 listener/重连任务）

// 2) 配对注入口（组合根构造）
impl PairingSessions {
    pub fn new(
        authority: Arc<identity_auth::Authority>,   // 组合根持有的**同一个**状态机实例
        public_origin: Option<String>,              // daemon.public_origin（未配置传 None）
        closer: Arc<dyn ConnectionCloser>,
    ) -> Self;
    pub fn authority(&self) -> &Authority;          // 重启终结/过期扫描也用同一实例
}
```

`LocalAdminDeps` 新增一个字段：`pub pairing: Arc<PairingSessions>`（其余五个字段不变）。

WP4 的三条装配约束：

1. 传给 `PairingSessions::new` 的 `Arc<Authority>` 必须与组合根做「重启终结（`reset_memory` + `unrecoverable_after_restart`）」与「周期过期扫描（`due_pairings`）」用的**同一个实例**——`device.pair.confirm` 的 `mark_pairing_approved` 落在内存态上，用两个实例会让「首次 WSS 认证成功后消费配对」永远看不到批准；另外 key handle/熵源/时钟注入给 `Authority` 的 `Clock` 应与 `LocalAdminDeps::clock` 同源（本层用 `Clock::now()` 判定窗口与过期）。
2. `public_origin` 取 `daemon.public_origin`（`CONFIG_REFERENCE.md` §1 的权威 host）；未配置时配对 begin 会回 `local.unavailable`（预期行为，不是缺陷）。
3. `ConnectionCloser` 由组合根实现（`server` 不含连接表）；撤销已提交，关闭失败只能记日志，不得回滚。

## 方法 → 结果形状摘要（§5.3/§5.4）

| 方法 | 实现路径 | result（字段顺序即文档顺序） | 主要错误码 |
| --- | --- | --- | --- |
| `device.pair.begin` | `expand_device_request` 展开 → `Authority::begin_pairing` → URL（`sync-protocol` 的 `QrPayload`）→ `UseCases::create_pairing` | `{ pairingId, pairingUrl, expiresAt, scopes }`（`scopes` = 展开后的独立命令 scope，已去重排序） | `local.invalid_params`（未知 pack/scope、`local.*`、窗口越界/非整数、缺/未知字段）、`local.unavailable`（`daemon.public_origin` 未配置、keystore 不可用）、`local.conflict`（配对 id 撞车，理论不可达）、`local.internal` |
| `device.pair.status` | `UseCases::pairing`（读回）＋`pairing_peer`（已认领时）→ `Authority::pairing_sas` → `Authority::pairing_status` | `{ state, displayName, publicKeyFingerprint, sas, requestedScopes, deviceId, expiresAt }`；claim 前除 `state`/`expiresAt` 外全 `null` | `local.not_found`（未知 id/族不匹配）、`local.invalid_params` |
| `device.pair.confirm` | 前置判定 → `Authority::settle(Approve)` → `UseCases::settle_pairing` → **提交成功后** `Authority::mark_pairing_approved` → 读回持久 `approvedAt` | `{ deviceId, scopes, confirmedAt }`（`scopes` = 用户确认的最终集合；`confirmedAt` 取回持久值） | `local.expired`、`local.conflict`、`local.invalid_params`（集合越界）、端口类（写集失败如 `local.unavailable`）、`local.internal` |
| `device.pair.reject` | 前置判定 → `Authority::settle(Reject)` → `UseCases::settle_pairing` | `{}` | `local.expired`、`local.conflict`、`local.invalid_params`（`reason` > 256）、`local.not_found` |
| `device.list` | `UseCases::devices` → `view::device` | `{ devices: DeviceRecord[] }`（含 `pending`/`active`/`revoked`） | `local.invalid_params`（未知参数）、端口类 |
| `device.revoke` | `UseCases::revoke_device` → **提交后** `PairingSessions::close_device` → 读回 `revokedAt` | `{ deviceId, revokedAt }` | `local.not_found`（未知设备）、端口类、`local.internal`（读不回） |
| `node.pair.begin`（owner） | `expand_node_request` → `Authority::begin_pairing`（`PairingSpec::Node{endpoint, kind: owner}`）→ URL（`node-link-protocol` 的 `QrPayload`）→ `create_pairing` | `{ pairingId, pairingUrl, state:"pending", expiresAt, sas:null, peerNodeId:null, peerPublicKeyFingerprint:null }`；窗口固定 5 分钟（本方法无 `expiresInMs`） | 同 `device.pair.begin`；`mode = "access"` → **`local.unsupported`** |
| `node.pair.status` | 同 `device.pair.status`（`PairingTarget::Node`） | `{ state, peerNodeId, peerPublicKeyFingerprint, sas, requestedGrants, expiresAt, lastRefreshError:null }` | `local.not_found`、`local.invalid_params` |
| `node.pair.confirm` | 同 `device.pair.confirm`（grants 集合） | `{ nodeId, grants, confirmedAt }`；落库记录 `kind = "access"`、`state = "paired"`、`ownerEndpoint = null` | 同 `device.pair.confirm` |
| `node.pair.reject` | 同 `device.pair.reject` | `{}` | 同 `device.pair.reject` |
| `node.list` | `UseCases::nodes` → `view::node` | `{ nodes: NodeRecord[] }` | `local.invalid_params`、端口类 |
| `node.revoke` | `UseCases::revoke_node` → **提交后** `close_node` → `nodes_for` 读回 | `{ nodeId, revokedAt }` | `local.not_found`、端口类、`local.internal`（任一角色行读不回） |

`pairingUrl` 形状：`https://<public_origin>/pair#data=<base64url(json)>`（设备）与
`https://<public_origin>/node-link/pair#data=<base64url(json)>`（节点）；payload 由两个协议 crate 的
`QrPayload` 序列化，fragment 里含 `pairingSecret`。它**只**作为 `result` 字段出现：不进日志、不进
`error.message`（有断言）、不进 `Debug`（`PairingSessions` 的 `Debug` 只输出 `has_public_origin`）。

## 错误映射增量（判定点写清）

- **`local.expired`** 的三个判定点：
  1. `params::require_pending_confirmation`：记录状态为 `rejected`/`expired`，**或**（`created`/`pending_confirmation`）且 `now >= expiresAt`（时钟判定，不依赖存储的过期扫描是否已跑）。这是 spec「配对超过 5 分钟未确认，或对已过期/已拒绝的 `pairingId` 调用 confirm → `local.expired`」的实现点——存储对 `rejected` 返回的是 `ConflictKind::Consumed`，直接透传会变成 `local.conflict`，因此本层前置判定。
  2. `params::map_port_error`：`PortError::Conflict(ConflictKind::Expired)`（存储的过期路径）。
  3. `params::map_pairing_error`：`PairingError::Expired`（SAS 派生时的过期判定）。
- **`local.conflict`**：记录状态为 `created` 且未过期（尚未认领就确认/拒绝）、或 `approved`/`consumed`（重复确认）；以及 `map_port_error` 的其余 `ConflictKind`、`map_pairing_error` 的 `WrongState`/`NotClaimable`。
- **`local.not_found`**：未知 `pairingId`、目标族不匹配（`device.*` 查节点配对或反之——该方法的目标域里没有它）、未知 `deviceId`/`nodeId`（存储 `NotFound`）。
- **`local.invalid_params`**：未知/拼错的 pack、scope、grant 名（含任何 `local.*`）；`expiresInMs` 非整数或不在 `1..=300000`；confirm 的集合超出登记请求值；`reason` 超过 256 字符；目标族不匹配的字段（设备带 `grants`、节点带 `scopes` 都是未知字段）；非 canonical 的 id；缺字段与未知字段。
- **`local.unavailable`**：`daemon.public_origin` 未配置（配对 begin）；keystore 不可用（取本机身份公钥）；`PortError::Unavailable`；`PairingError::SecretUnavailable`/`EntropyUnavailable`。
- **`local.internal`**：`daemon.public_origin` 不是 canonical origin；已认领但库中无对端行（库被外部改写）；落定没有创建预期族别的信任记录；撤销/确认后读不回持久时间戳；transcript/证明/绑定不一致等状态机内部拒绝。**不转述**内层文本（`error.message` 是简短英文，内层 `PairingError`/`AuthorizationError` 的诊断文本不出现在响应里）。
- **`local.unsupported`**：`node.pair.begin` 的 `mode = "access"`（专用消息，说明本切片不发 claim）、`node.rotate-key.begin`（§5.7，形状未落地）。

## 关键口径（本层在契约沉默处做的收窄，请主 Agent 决定是否写回文档）

1. **状态 token 映射**（`pairing::device_pairing_state_token` / `node_pairing_state_token`）：核心状态是
   `created|claimed|pending_confirmation|approved|rejected|expired|consumed`（`claimed` 从不落库）。
   §5.3 的词表是 `pending|claimed|approved|rejected|expired`，§5.4 多一个 `pending_confirmation`，
   两份词表都没有 `consumed`。实现：`created → "pending"`；设备族 `pending_confirmation → "claimed"`、
   节点族 `pending_confirmation → "pending_confirmation"`；`consumed → "approved"`（不新增词表取值）。
2. **status 的请求集合 = 登记值**：`requestedScopes`/`requestedGrants` 取配对记录的登记集合。
   claim 的请求子集没有被 `storage-sqlite` 持久化（`owned_pairing_peer` 无 requested 列，
   `claim_pairing` 保留记录的集合），因此 status 只能展示上限；若产品要求展示设备真正的请求子集，
   需要存储层加列（超出本 WP 写范围）。
3. **`node.pair.begin` 的 `requestedGrants` 在 owner 模式是授权上限**：§5.4 写「`mode = "access"` 时携带」，
   但若无非空上限，`node.pair.confirm` 的最终 grants 必然为空，§5.4 自己的场景（「确认后创建信任记录并
   分配初始 `grant.*`」）就无法成立。实现按「登记上限」处理，并在 `params.rs` 的文档注释中写明。
4. **`node.pair.begin` 的 `displayName`** 存进 `PairingRecord.display_name`（claim 前唯一的名称槽位）；
   对端名称在 claim 后由 `owned_pairing_peer.display_name` 提供。
5. **`preset.*` 不在 `device.pair.begin` 的输入域**：§5.3 的原文是「取值分别限于 `pack.*` 与命令名集合」，
   因此 `preset.*` 传给 `requestedPacks` 会得到 `local.invalid_params`；CLI 若提供 `--preset` 应先在本机
   展开成 packs（这样也保持展开词表只有 `identity_auth::authorization` 一份）。请确认这个收窄。
6. **时间算术**：core 的 `Timestamp` 是不透明文本、不暴露日期算法，`identity-auth` 只导出「秒 → Timestamp」，
   因此 `pairing.rs` 内含一个私有 `window::add_millis`（Hinnant 的 `days_from_civil`/`civil_from_days`），
   与 `storage-sqlite::migrate::window` 同因同法；`expiresInMs` 因此按毫秒精确生效（不是整秒向下取整）。
   若后续要消重，建议下沉成叶子 crate（本 WP 未做）。
7. **SAS 在内存 secret 已清除时呈现 `null`**（重启后被清理、或已过期）：状态本身仍可查（CLI 需要看到终态），
   结构化日志记一条 warn，**绝不**伪造 SAS；`Authority::pairing_sas` 的其它失败按实现缺陷回 `local.internal`。
8. **配对 id 由 server 用 `uuid` v4 生成**：`IdGenerator::pairing_id` 目前没有任何生产调用方，
   `UseCases` 也没有暴露 `ids`。若主 Agent 希望统一走端口，需要在 `LocalAdminDeps` 增一个
   `Arc<dyn IdGenerator>`（WP4 可一并做，属小改动）。

## checks（逐 Check ID 汇总）

| Check ID | 命令 / 目录 | 配置与环境 | 结果 | 日志 |
| --- | --- | --- | --- | --- |
| [PV3] | `cargo test --locked -p server --all-features` @ 仓库根 | Windows x64；cargo/rustc 1.98.1；`--locked` | 退出码 0；**111 个用例**通过、0 失败、0 ignored（lib 83 + channel 14 + schema drift 6 + naming 4 + unix 0 + windows 4）；新增 18 个用例（`pairing.rs` 6 + `router.rs` 12） | `reports/wp3-server-methods.log` 最终轮 (3)(3b)、`reports/wp3-server-pairing.log` (P1)–(P4) |
| [PV3] | `cargo clippy --locked -p server --all-targets --all-features -- -D warnings` | Windows x64 | 退出码 0，无警告 | `reports/wp3-server-methods.log` 最终轮 (2) |
| [PV3]（跨目标编译） | `cargo clippy --locked -p server --all-targets --all-features --target x86_64-unknown-linux-gnu -- -D warnings` | 已安装 Linux std；**只编译不执行** | 退出码 0 | `reports/wp3-server-methods.log` 最终轮 (4) |
| [PV3] | `cargo fmt --all -- --check` | 默认 rustfmt | 退出码 0 | `reports/wp3-server-methods.log` 最终轮 (1) |
| [PV2] | `node scripts/check-crate-boundaries.mjs` @ 仓库根 | Node v24.19.0；`cargo metadata --no-deps` | 退出码 0：`crate boundaries OK: 11 个 crate …`；`cargo tree -p server` 的工作区内依赖新增 `sync-protocol`、`node-link-protocol`（§5 矩阵 server 行 ✓） | `reports/wp3-server-methods.log` 最终轮 (5)(5b) |
| [PV1] | `npm run verify`（= `npm run check` 的 10 道门禁 + fmt + workspace clippy + workspace test）@ 仓库根 | Node v24.19.0 / npm 12.0.2；离线 | 退出码 0；workspace **626 个用例**通过、0 失败、2 ignored（既有 `#[ignore]`，非本轮新增）；10 道合同门禁全绿 | `reports/wp3-server-methods.log` 最终轮 (10)；`reports/du1-pv1.log`（`EXIT(npm run check)=0`） |
| PC1 | `npm run check`（单独一轮） | 同上 | 退出码 0 | `reports/wp3-server-methods.log` 最终轮 (6)、`reports/du1-pv1.log` |
| LC1 | `rg -n "unsafe" crates/server/src` | ripgrep | 退出码 1（**零命中**） | 最终轮 (7) |
| LC2 | `rg -n "reqwest|hyper|ureq|TcpStream" crates/server/src`；`rg -n "http" crates/server/src \| rg -v "https?://"` | ripgrep | 两条都退出码 1（**零命中**）：无 HTTP 客户端、无裸 socket API、无裸 `http` 标识符；`http` 只出现在 URL 字面量与文档注释里 → 证明配对方法不发出站请求 | 最终轮 (7b) |
| LC3 | 自检「正常路径无 `unwrap()`/`expect()`/`panic!`」 | 脚本逐文件取首个 `#[cfg(test)]` 行号 | 285 处命中全部落在测试代码内（各文件 `#[cfg(test)] mod tests`，`test_support.rs` 由文件首行 `#![cfg(test)]` 自我声明）；`violations: []` | 最终轮 (8) |
| LC4 | 提交钩子（`.husky/pre-commit` 三步）与 `commitlint` | 本机 | 三步全过、commitlint 通过；未使用 `--no-verify` | 提交输出（`eef75f4`） |

### 新增测试清单（18 个，全部无 `#[ignore]`、无跳过）

| 分组 | 用例 |
| --- | --- |
| `pairing.rs`（6） | `device_url_round_trips_through_the_sync_qr_payload`、`node_url_round_trips_through_the_node_link_qr_payload`、`a_missing_or_malformed_origin_fails_closed`、`the_pairing_window_is_capped_at_five_minutes`、`state_tokens_stay_inside_the_documented_vocabularies`、`millis_shift_handles_every_component_rollover` |
| `router.rs`（12） | `device_pairing_full_flow_reaches_an_active_device`、`expired_and_rejected_pairings_never_create_trust`、`confirm_rejects_sets_beyond_the_requested_ones`、`revoke_closes_the_connection_after_the_commit_and_list_reflects_it`、`a_failed_settlement_keeps_the_memory_state_strict`、`node_owner_pairing_flow_pairs_a_node_with_initial_grants`、`node_access_mode_is_unsupported_and_creates_nothing`、`pairing_methods_fail_closed_without_a_public_origin`、`unknown_and_family_mismatched_pairings_answer_not_found`、`pairing_begin_validates_names_windows_and_shapes`、`node_confirm_and_reject_error_paths_answer_the_documented_codes`、`list_methods_reject_unknown_parameters` |

逐方法「成功路径 + 错误路径」对应关系见 `reports/wp3-server-pairing.log` 的 (P5) 节；spec
[R42]–[R48] 的映射：

- [R42]（设备配对与信任方法）→ `device_pairing_full_flow_reaches_an_active_device` + `pairing_begin_validates_names_windows_and_shapes`；
- [R43]（设备配对全流程）→ 同上（begin 的 URL/展开 scopes/`expiresAt`；claim 前 status 只暴露 `state`/`expiresAt`；claim 后指纹/SAS；confirm 后 `active`）；
- [R44]（过期与拒绝不创建信任）→ `expired_and_rejected_pairings_never_create_trust`（含 `local.expired` 与 `trust.device_count() == 0`）；
- [R45]（撤销立即生效）→ `revoke_closes_the_connection_after_the_commit_and_list_reflects_it`（`RecordingCloser` 断言提交后关闭，`device.list` 呈现 `revoked` + `revokedAt`）；
- [R46]/[R47]（节点配对与 Owner 模式）→ `node_owner_pairing_flow_pairs_a_node_with_initial_grants`（含 `node.revoke`）；
- [R48]（access 模式明确不支持）→ `node_access_mode_is_unsupported_and_creates_nothing`（`local.unsupported`、零配对行、LC2 证明无出站）；
- `pairing_methods_fail_closed_without_a_public_origin` 与 `a_failed_settlement_keeps_the_memory_state_strict` 覆盖
  D1 的失败关闭与「写集提交失败 → 不得产生内存已批准、库无信任」。

## 已知限制与偏差

1. **`PairingClaimOutcome`（HTTPS claim 返回类型）在本地通道无用**：claim 属 `server::sync`/`server::node_link` 的
   HTTPS 端点（后续切片）。本地通道只**读**库里的对端事实，测试用 `FakeTrust::claim_pairing` 按存储层同款语义
   模拟 claim（状态守卫、peer 行、`pending_confirmation`）。
2. **测试替身刻意照抄存储语义**：`FakeTrust` 的状态守卫、终态冲突、子集校验、`COALESCE(revoked_at)`、
   批准时 `created_at = at` 且 `last_seen_at`/`last_connected_at` 为 `None` 等都与
   `storage-sqlite/src/admin/trust.rs` 对齐，避免验证一个不存在的存储行为；但**它不是**存储层实现，PV4 仍需真实 SQLite。
3. **本机未执行 Linux/Unix 运行**：Unix 相关路径只做了 `--target x86_64-unknown-linux-gnu` 的 clippy 编译；
   真实执行属 Linux CI（本 WP 不含平台分支）。
4. **`cargo-deny`（`deps`/`advisories`）与 `gitleaks` 只在 CI 运行**：本地无等价物，本轮未执行，不声称通过；
   新增的 `sync-protocol`/`node-link-protocol` 是 workspace 内 path 依赖（`deny.toml` 的 path 来源登记已有先例），
   但按仓库口径仍以 CI 判定为准。
5. **方法失败审计**：与 WP3b1 同口径——§14.2 没有「方法失败」类别，路由层失败只写结构化日志（method + 错误码，
   不含参数、不含 URL）；配对批准/拒绝/创建/认领与撤销的审计由 core 写集同事务提交，**未绕过 core 直接写库**。
6. **`pairingUrl` 的正文字节没有进入任何日志**：新增的 warn 日志只带方法名；`Debug` 实现已收窄。测试也未打印 URL
   （`reports/wp3-server-pairing.log` 只有测试名与结果）。

## 未执行项与待澄清问题（交主 Agent）

1. **状态 token 映射（口径 1）**：设备族的 `pending_confirmation → "claimed"`、`consumed → "approved"` 是
   「以现有词表表达既有状态」的收窄。若主 Agent 认为 §5.3/§5.4 的词表应扩成
   `pending|claimed|pending_confirmation|approved|consumed|rejected|expired`，需要同时改文档、schema enum 与 fixture，
   并让 `npm run check` 通过（属不兼容的 `result` 语义变更）。
2. **status 的请求集合是登记值而非 claim 子集（口径 2）**：需要决定是「接受展示上限」还是让存储层持久化 claim 的
   请求集合（后者超出本 WP 写范围）。
3. **`requestedGrants` 在 owner 模式是上限（口径 3）**：建议在 §5.4 补一句，否则下一个实现者会按字面把 owner 模式的
   上限当成空集，使 `node.pair.confirm` 永远无法授予任何 grant。
4. **`preset.*` 被拒（口径 5）**：需要确认「CLI 侧展开 preset」是期望路径；若 Daemon 需要直接接受 `preset.*`，
   那是对 §5.3 `params` 的扩展（新字段或放宽取值域）。
5. **`*.revoke` 重试语义与存储层不一致（新发现）**：`LOCAL_ADMIN_PROTOCOL.md` 的「重试与失败语义」要求
   `*.revoke` 重试得到 `local.not_found`，但 `storage-sqlite::revoke_device`/`revoke_node` 用
   `COALESCE(revoked_at, ?)`，重复撤销**成功返回**原来的时间（不是错误）。本 WP 只映射存储的行为，未改存储；
   需要主 Agent 决定是补存储的「已撤销 → `ConflictKind`/`NotFound`」还是调整文档口径。
6. **时间算术下沉（口径 6）**：`window::add_millis` 与 `storage-sqlite::migrate::window` 是同一套公历算法的两份实现；
   若后续新增第三个使用者，建议下沉为叶子 crate（ADR 级别的小决策，本 WP 未做）。
7. **配对 id 的来源（口径 8）**：若统一走 `IdGenerator::pairing_id`，需要给 `LocalAdminDeps` 加 `Arc<dyn IdGenerator>`
   并让 WP4 注入——请确认是否要在 WP4 之前改。
8. **`specs/local-admin-methods` 与 `tasks.md` 2.11/2.13 的勾选、`verification.md` 的证据登记**由主 Agent 负责（本 WP 不改规划文件）；
   独立 review（RV1 的 WP3b2 部分）与 WP4 的 [PV4] 端到端尚未返回，本报告不把它们写成 PASS。

## 资源释放

| 资源 | 归属 | 状态 |
| --- | --- | --- |
| 测试用临时目录 | WP3b1 的 `TestWorld` | 本轮新增用例不使用临时目录；既有 `acpr-wp3b1-router-*` 仍随 `TestWorld::drop` 清理 |
| 本机 Named Pipe / Unix socket | 不属于本 WP | 未创建任何 endpoint（传输层未改动） |
| `reports/wp3-server-pairing.log` | 本 WP | 新建（按仓库约定 `*.log` 不入库） |
| `reports/wp3-server-methods.log`、`reports/du1-pv1.log` | 本 WP（追加） | 追加最终轮，未覆盖既有轮次 |
| 仓库根 `target/` | 共享 | 未清空（增量缓存）；Linux 交叉编译产物在同目录下 |
| `git` 工作区 | 本 WP | 代码提交后 `git status --porcelain` 为空；仅显式路径入暂存（未用 `git add -A`） |

```yaml
handoff_index:
  - task_id: "2.11"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "eef75f4"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3b2-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP3b2 轮：cargo fmt --all -- --check（退出码 0）、cargo clippy --locked -p server --all-targets --all-features -- -D warnings（退出码 0）、cargo test --locked -p server --all-features（退出码 0；111 个用例通过、0 失败、0 ignored；新增 18 个）在 Windows x64 本机执行；[R42]–[R48] 的配对族行为由 local_admin::pairing 与 local_admin::router::tests 的 18 个用例逐条覆盖（成功 + 错误路径），映射见 reports/wp3-server-pairing.log 的 (P5)。跨目标 clippy（--target x86_64-unknown-linux-gnu）退出码 0。日志 reports/wp3-server-methods.log 最终轮的 (1)(2)(3)(3b)(4) 节。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.11"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "eef75f4"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3b2-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "D2 裁决新增 sync-protocol/node-link-protocol 两条生产依赖边后的 [PV2] 轮：node scripts/check-crate-boundaries.mjs 退出码 0（crate boundaries OK: 11 个 crate …），cargo tree -p server 显示两条新边均在 §5 矩阵 server 行允许范围内；Cargo.lock 只新增 server 的两行依赖（+2）。日志 reports/wp3-server-methods.log 最终轮的 (5)(5b) 节。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.13"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "eef75f4"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3b2-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP3b2 轮：npm run check（10 道合同门禁全绿，退出码 0；reports/du1-pv1.log 记 EXIT(npm run check)=0）与 npm run verify（workspace 626 通过 / 0 失败 / 2 既有 ignored，退出码 0）在仓库根执行；代码提交经 .husky/pre-commit 三步与 commitlint 通过。日志 reports/wp3-server-methods.log 最终轮的 (6)(10) 节。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.13"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "eef75f4"
    evidence_type: VALIDATION
    evidence_id: LC1
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3b2-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "附加本地检查（2.13 的自检项）：rg -n \"unsafe\" crates/server/src 零命中（退出码 1）；rg -n \"reqwest|hyper|ureq|TcpStream\" 与 rg -n \"http\"（排除 https?:// 字面量）都零命中，证明配对方法不发任何出站请求；「正常路径无 unwrap/expect/panic」自检 285 处命中全部落在测试代码内（violations: []）。日志 reports/wp3-server-methods.log 最终轮的 (7)(7b)(8) 节。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.11"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "eef75f4"
    evidence_type: DELIVERY
    evidence_id: NOT_APPLICABLE
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3b2-handoff.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "WP3b2 交付物：eef75f4（8 个文件、+3452/−53：新增 crates/server/src/local_admin/pairing.rs，扩展 params/router/view/test_support/mod.rs 与 Cargo.toml/Cargo.lock）。对 WP4 冻结的形状见本报告「WP4 要实现的形状」：ConnectionCloser/NoConnections/PairingSessions::new 与 LocalAdminDeps 的 pairing 字段。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.13"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "eef75f4"
    evidence_type: CHECK
    evidence_id: PV4
    report_path: openspec/changes/daemon-cli-and-local-admin/reports/wp3b2-handoff.md
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "方法级端到端（真实 SQLite + 真实 keystore + 真实 IPC + 真实 `identity-auth` 状态机装配）属 WP4 的集成轮，本 WP 用 fake 端口（FakeTrust 照抄存储语义）覆盖；另有 8 条待澄清口径与「*.revoke 重试语义与 storage-sqlite 不一致」的发现需主 Agent 裁决。本行不写成 PASS。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.6"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: "eef75f4"
    evidence_type: REVIEW
    evidence_id: RV1
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "WP3b2 的独立 review 尚未返回；须由不继承本实现对话的执行者检视配对方法的错误码判定点（`local.expired`/`local.conflict`）、`pairingUrl` 是否可能进日志/错误、确认集合的子集校验、撤销后关闭连接的顺序，以及 fake 是否照抄了存储语义。本报告不把自检当作其结论。"
    source_evidence: NOT_APPLICABLE
```
