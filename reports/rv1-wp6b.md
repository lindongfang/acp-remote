# rv1-wp6b.md — WP6 两处 review 缺口的闭合改动（独立对抗性复验）

## 0. 检视对象、范围与方法

- 仓库 / 分支：`D:\Project\acp-remote`，`feat/admin-state-persistence-v2`
- 被检视 revision：`git rev-parse HEAD` = `1baea5bd7d4dc824443b4e52e916d97f7bec3c44`（WP6 首版提交，含 `reports/rv1-wp6.md`）+ **未提交工作区**（本轮闭合改动）
- `git status --porcelain`（检视时刻）：

```text
 M crates/core/src/model/identity.rs
 M crates/core/src/model/tests.rs
 M crates/storage-sqlite/src/admin/trust.rs
 M crates/storage-sqlite/tests/admin_store.rs
 M docs/CORE_PORTS_AND_STORAGE.md
?? .omp/
?? .pi/prompts/opsx-verify.md
?? .pi/skills/openspec-verify-change/
```

`??` 三项是宿主安装文件，与本变更无关。`git diff --stat` = 5 文件、+432/−65；`tests/admin_store.rs` 为纯新增（+232，无删除），`grep -c '#\[tokio::test\]'` = **25**。

- 检视过程中工作区另有**他人并发写入**（`M openspec/changes/admin-state-persistence-v2/verification.md`、`M reports/wp6-admin-store-tests.log`）：本报告 §1.5 对 `verification.md` 的引用按读取时刻的行号（第 24–26 条决定、WP6-1..4 复核行）；这两个文件不在本轮改动面内，也未参与结论。

- 检视范围（本轮 5 条闭合改动）：`crates/core/src/model/{identity,tests}.rs`、`crates/storage-sqlite/src/admin/trust.rs`、`crates/storage-sqlite/tests/admin_store.rs`、`docs/CORE_PORTS_AND_STORAGE.md` 的 §3.5/§11.1/§11.2/§11.6。
- 对照的权威：`docs/CORE_PORTS_AND_STORAGE.md` §3.5/§7.3/§11.1/§11.2/§11.3/§11.6、`docs/LOCAL_ADMIN_PROTOCOL.md` §5.3/§5.4/§5.8、`docs/NODE_LINK_PROTOCOL.md` §13.1–§13.4、`docs/SYNC_PROTOCOL.md` §3.2/§7.1/§7.2/§8.1、`docs/INITIAL_DESIGN.md` §11.6/§11.7、`schemas/node-link/v1/pairing.schema.json`、`crates/node-link-protocol/src/pairing.rs`、`crates/sync-protocol/src/pairing.rs`、`crates/storage-sqlite/src/{migrate.rs,error.rs}`、`crates/core/src/{ports.rs,use_cases.rs}`、`reports/rv1-wp6.md`。
- 只读命令与退出码（未修改任何代码/测试/文档；唯一写入是本报告）：
  - `git rev-parse HEAD` / `git status --porcelain` / `git show --stat 1baea5b` / `git diff`（分文件、`-U3`/`-U6`）→ 0
  - `grep -c '#\[tokio::test\]' crates/storage-sqlite/tests/admin_store.rs` → `25`
  - `node scripts/check-contract-drift.mjs` → 0：`contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 87 个方法签名与 crates\core\src\ports.rs 一致`
  - `node scripts/check-crate-boundaries.mjs` → 0：`crate boundaries OK: 6 个 crate 的依赖方向与 §5 矩阵一致`
  - 未运行 `cargo test`/`clippy`/`fmt`（按 assignment：实现者已跑过且全绿）。无 implementation 上下文，逐条对着合同与代码核。

## 1. 逐条核对

### 1.1 条目 1：WP6-2（节点配对批准）与「Node ⇒ access」角色推导

**批准路径的四个落点都在**（对 §11.6 第 4 条与 §11.2 第 2 条）：

| §11.6 第 4 条要求 | 节点路径实现 | 位置 |
|---|---|---|
| `Approved` 时创建信任行 | `NodeRecord::try_new(node_id, peer.display_name(), NodeKind::Access, fingerprint, granted_grants, NodeState::Paired, None, at, None, None)` → `upsert_node` | `trust.rs:967-977` |
| 把 peer 公钥转入 `owned_peer_key` | `bind_peer_key(tx, "node", …)`（与 `put_node` 同一 SQL，唯一入口） | `trust.rs:976`、`trust.rs:1049-1071` |
| 更新配对为 `approved` | `UPDATE owned_pairing SET state='approved', approved_at=?2 WHERE … AND state='pending_confirmation'`，0 行 → `Conflict(Consumed)` | `trust.rs:793-802` |
| 写 `pairing.approved` 审计 | 调用方写集经 `insert_audit_rows` 在 `tx.commit()` 前落库（`UseCases::settle_pairing` 传 `PairingApproved`） | `trust.rs:806`、`use_cases.rs:586-600` |
| 任一步失败全回滚 | 全部写都在 `BEGIN IMMEDIATE` 事务内，`?` 早返回即 drop 回滚 | `trust.rs:693-698`、`trust.rs:806-809` |
| 过期不创建信任 | `at >= expires_at` → `Conflict(Expired)`，在 `approve_*` 之前 | `trust.rs:769-774` |
| 拒绝不创建信任 | `Rejected` 分支只推进终态 + 审计 | `trust.rs:744-764` |

**角色推导成立**，三条互相独立的证据：

1. `LOCAL_ADMIN_PROTOCOL.md:550`：「`--mode owner` 在本机生成二维码并**在本机确认**；`--mode access` 拿用户给的 pairing URL 执行 claim，展示 SAS 后**等待对端（Owner）本机确认**，本机没有 `confirm` 调用」→ 本机只在 owner 方向落定；access 方向本机不调 `settle_pairing`。
2. 配对行只由 Owner 侧创建：二维码由 Owner Node 生成（`NODE_LINK_PROTOCOL.md:681-704`），claim 由 Access Node 发起（§13.2:717-743）→ 节点配对行在本机 ⟺ 本机是该配对的 Owner，对端是 Access。
3. wire 层直接钉死 claimer 角色：`schemas/node-link/v1/pairing.schema.json:46-52` 的 `claimRequest.nodeKind` 是 `allOf: [nodeKind, const "access"]`，描述为「Node Link 配对只允许 Access Node 发起 claim」；`crates/node-link-protocol/src/pairing.rs:109-114` 把它实现为 `AccessNodeKind`（拒绝 `"owner"`）→ 「owner 方向的 claimer 会是 owner」在 DTO 反序列化阶段就不可能出现。

**可达反例排查**：本机 `owned_pairing_peer` 行只能由本机 `claim_pairing` 写入（远端无法写本机库），而 `claim_pairing` 要求本机存在该配对行；本机在 access 方向不调 `claim_pairing`（那是对远端的 HTTP claim，§13.2）也不调 `settle_pairing`（§5.4）→ 未找到任何可达路径使本机落定一个「对端为 owner」的节点配对。残留性质：冻结写集 `PairingSettlementWrite`/`PairingRecord` 不携带角色，因此这是**从协议拓扑推出的结论**，现在已被 §11.2 第 2 条与 §11.6 第 4 条的措辞固化为合同（`docs:1316`、`docs:1381`）；若将来要支持「从配对导入 Owner 角色」，写集必须先扩展——当前的失败方式是把 access 行写错角色，这一点值得在 §11.6 留一句前提（本轮未构成缺陷）。

### 1.2 条目 2：绑定校验的语义与位置

- **位置正确（零写入）**：`peer.host_binding() != record.host_binding()` 在 `claim_pairing` 的存在性（`trust.rs:581-585`）、目标族（`586-589`）、终态（`590-592`）、`created`（`593-595`）之后，在任何 UPDATE/INSERT（`trust.rs:621-631` 的状态推进与 `trust.rs:651-660` 的 peer 行插入）之前；返回 `Conflict(IdentityMismatch)` 时事务未提交 → 零推进。用例 `claim_with_a_foreign_host_binding_is_rejected` 从库侧证到「状态仍 `created`、`owned_pairing_peer` 0 行、`pairing.claimed` 审计 0 行，且正确回显随后可认领」（`admin_store.rs:834-889`）。
- **与合同一致**：§11.2 第 1 条（改后 `docs:1315`）把「本机绑定一致」定义为 `PairingPeer.host_binding`（claim 回显值：设备 `canonicalOrigin`、节点 `endpoint`）与 `PairingRecord.host_binding`（登记值）**逐字相等**，并写明 host/Origin 级检查（§7.2:421 的 Host/Origin 匹配、`NODE_LINK_PROTOCOL.md:743` + §13.4:810 的 403）由协议层负责且**不替代**这里的判定；§11.6 第 3 条（`docs:1380`）列出 `IdentityMismatch`。文档与代码逐字对得上。
- **比协议更严是否会误拒合法 claim —— 不会**（登记值与回显值同源）：
  - 设备：二维码带 `canonicalOrigin`（`SYNC_PROTOCOL.md:371-378`），claim body 回显同一字段（§7.2:407-415）；两端都是本机自己的 canonical origin。
  - 节点：二维码带 `endpoint`（§13.1:695-704），claim body 的 `endpoint` 与二维码用**同一个** `$defs/endpoint`（`pairing.schema.json:29` 与 `:52`，描述同为「HTTP origin 由该 host 加 https 得到」；`node-link-protocol/src/pairing.rs:252-253` 与 `:281` 同为 `Endpoint`）→ 合法客户端回显的是本机 endpoint 本身，逐字相等。
  - 「不逐字相等但等价」的形式（显式 `:443`、大写 host、尾 `/`）按 §3.2:90-93 本就不合法（canonical origin 是规范化后的小写 scheme + 规范化 hostname + 显式有效 port、省略默认端口、无尾 `/`）；`sync-protocol` 的 `CanonicalOrigin::parse`（`pairing.rs:33-48`）只校验不规范化，因此**规范化责任在产出方**，与 §3.2 一致。
  - 唯一的前提（已写进改后的 §11.2 第 1 条）：调用方必须传「登记时宣告的同一段文本」（本机自己的 origin/endpoint），而不是另做一次可能不同口径的重算。
- 顺序上的小差异（无影响）：绑定比对排在过期比对之前，因此「已过期且绑定不符」返回 `IdentityMismatch` 而非 `Expired`（§11.2 第 1 条的列举顺序是「未过期」在前）。两者都是具名冲突，不构成违规。

### 1.3 条目 3：`pairing_peer` 从配对行回填 + 列常量

- **捷径是否有掩盖风险 —— 没有**：
  - 库里**不存在**可表达「两条绑定不同」的列：`owned_pairing_peer` 的列里没有 `host_binding`（`migrate.rs:270-281`）；`claim_pairing` 只落配对行的值（对端回显值不写任何列），且写 peer 行之前已断言相等（`trust.rs:596-601`）→ 读回路填的值恒等于认领时被接受的值。
  - 「peer 行存在、配对行缺失」无法由代码路径产生：全 crate 无 `DELETE FROM owned_pairing`（`grep` 命中只有 `owned_event`/`owned_interaction`/`owned_attachment*`/`owned_audit` 与 `imported_*`），且两个连接池都开了 `foreign_keys(true)`（`migrate.rs:651`、`migrate.rs:666`），peer 行的 `REFERENCES owned_pairing(pairing_id) ON DELETE CASCADE` 生效 → 只有库被外部改写才可达。
  - 因此 `StorageError::Corrupt("peer row without its pairing row")`（`trust.rs:334`）是恰当分类：与 `settle_pairing` 对「已认领但 peer 行缺失」的判定同类（`trust.rs:734-736`），映射表把 `Corrupt` → `PortError::Corrupt`（`error.rs:88-90`）→ 失败关闭，不是静默降级。
- **列常量与 DDL 逐列对齐**：`PAIRING_COLUMNS`（`trust.rs:51-53`，13 列）与 `owned_pairing`（`migrate.rs:247-266`）同名同序，`pairing_from_row` 的入参顺序与 `PairingRecord::try_new` 一致（`trust.rs:102-124`）；`PAIRING_PEER_COLUMNS`（`trust.rs:56-57`）是 `owned_pairing_peer` 的列子集（`pairing_id` 在 WHERE、`claimed_at` 本类型不用；`fingerprint` 被选中但按 §3.5 由公钥派生、不参与判定，与 `load_peer_key` 口径一致）。无多列/缺列。
- `Corrupt` 这条读路径的分类没有写进 §5.3/§11.6（只在模块注释与 §9 判据 29 的失败关闭里）——信息级，不影响行为。

### 1.4 条目 4：两个新用例能否证伪

两个用例的断言对象都是可观察结果（端口返回值 + 经端口或 raw pool 读到的行），能拦住实现回退：

| 假想回退 | 失败点 |
|---|---|
| `settle_pairing` 的节点分支改回 `InvalidRequest` | `node_pairing_approval_persists_access_trust` 在 `.expect("approve node pairing")` 处 panic（`admin_store.rs:745-770`），后续 `TrustRecordRef::Node` 相等断言也不会执行 |
| 删掉绑定比对（`trust.rs:599-601`） | `claim_with_a_foreign_host_binding_is_rejected` 在 `.expect_err(...)` 处失败（`admin_store.rs:846-853`）——该调用会返回 `Ok` |
| 删掉 `settle_pairing` 的 `pending_confirmation` 守卫 | `a_pairing_can_be_settled_only_once` 的两条 `assert_conflict(…, ConflictKind::Consumed)`（`admin_store.rs:897-928`）与 `approved_at == at(2)` 失败 |
| 不写 `owned_peer_key` / 不改配对状态 | 节点用例的 `peer_key(...).is_some()`、重启后逐字节相等、`pairing().state() == Approved`、`owned_node` 行数 = 1、`node.paired` 审计 = 1 全部失败 |

无恒真断言：节点用例断言的每一项（节点行 kind/state/`owner_endpoint`/grants/指纹、配对状态、`owned_peer_key` 重启后字节相等、`owned_node` 行数、审计行数）都要求新写路径真的执行；外部绑定用例的 `pairing_peer(...).is_none()`、peer 行数 0、审计 0 也都依赖「拒绝发生在写之前」而不依赖前置注入的必然值。用例末尾都补了正向路径（正确回显可认领 / 移除障碍后同一落定成功），排除「写集本身非法」的假阳性。

一处措辞（非缺陷）：节点用例的文档注释写「写 `node.paired` 审计」，而写集内容是测试自己构造的 `AuditAction::NodePaired`；生产调用方 `UseCases::settle_pairing` 写的是 `PairingApproved`（`use_cases.rs:589-594`，与 §11.6 第 4 条的 `pairing.approved` 一致）。存储层只负责「审计随写集落库」，断言因此仍然有效；`use_cases.rs:489` 的「配对确认路径负责写 `node.paired`」是 core 侧的既有措辞（不在本轮改动面内）。

### 1.5 条目 5：WP6-1 / WP6-3 / WP6-4 的残留

- **WP6-1（采定与重复批准）——已闭合**：`settle_pairing` 在读取行后按正向谓词只放行 `pending_confirmation`，`approved` 等非终态 → `Conflict(Consumed)`、终态 → `Expired`/`Consumed`，且该判定**在任何写入之前**（`trust.rs:710-726`；注释说明了两条曾经的坏路径）。用例 `a_pairing_can_be_settled_only_once`（`admin_store.rs:892-941`）断言重复批准与「批准后拒绝」都是 `Conflict(Consumed)`、`approved_at` 仍是首次 `at(2)`、状态仍 `approved`、`pairing.approved` 审计 1 条而 `pairing.rejected` 0 条。改后的 §11.2 第 2 条也把这条写成合同（`docs:1316`）。
- **WP6-3（回滚注入位置）——已闭合**：`a_constraint_failure_during_approval_leaves_no_half_authorization` 现在跑两次注入——`BEFORE INSERT ON owned_device`（写集第一个写）与 `BEFORE INSERT ON owned_audit`（写集最后一个写），共用 `approval_rollback_under`，对四个面断言回到调用前（配对仍 `pending_confirmation` 且 `approved_at` 为空、设备行不存在、`owned_peer_key` 无行、`owned_device`/`owned_peer_key` 行数 0、`pairing.approved` 审计 0）再移除触发器让同一写集成功（`admin_store.rs:2481-2596`）。注入点落在 spec 列举的「配对确认」路径上，且后一个注入证明的是**整事务回滚**而不是「尚未写到那一步」。
- **WP6-4（`host_binding` 不再写空串）——已闭合**：登记写真实值（`trust.rs:543-544`）、读取还原（`trust.rs:117`）、认领比对（`trust.rs:596-601`）、对端行回填（`trust.rs:328-335`）；模型侧 `PairingRecord`/`PairingPeer` 都要求非空且 ≤2048（`identity.rs:476`、`identity.rs:717`，空 → `InvalidValue::Empty`），负例进常驻测试（`model/tests.rs:1570-1586`、`model/tests.rs:1632-1642`）；§3.5/§11.1/§11.2/§11.6 同步（`docs:138`、`docs:142`、`docs:1297`、`docs:1315`、`docs:1380`）。同时代码头不再声称「记录在案的合同缺口见 verification.md」——`trust.rs:7-13` 改为陈述两条机器事实来源，`verification.md:74-76/110-111` 也补了第 24–26 条决定与闭合记录，与代码一致。

### 1.6 条目 6：本轮新增的回归风险

- **`host_binding` 与 wire 语义**：字段名是存储层命名，注释与 §3.5 都把设备映射到 `canonicalOrigin`、节点映射到 `endpoint`（`identity.rs:431-433`、`identity.rs:695-697`、`docs:138`、`docs:142`），与实际 wire 字段一致；落库的只是 origin/endpoint 文本，`pairingSecret`/HMAC/QR payload 仍不进库（§13.1:705）。
- **上限口径**：模型 1..=2048 字符（`identity.rs:476/717`）与 endpoint 的 `maxLength: 2048`（`pairing.schema.json:14-21`、`node-link-protocol/src/pairing.rs:36-37`）、`is_endpoint` 的 ≤2048（`ids.rs:139-141`）、`CanonicalOrigin::parse` 的 ≤2048（`sync-protocol/src/pairing.rs:36`）一致；模型对设备只做长度约束（形状由协议边界负责）是其文档声明的口径，不产生「协议接受而模型拒绝」或反向的洞。
- **`approve_node` 与 `put_node` 守卫一致**：两者都（a）比对已绑定 `owned_peer_key` 的指纹、（b）逐个比对既有角色行的指纹、（c）拒绝 `Revoked` 行；`put_node` 额外的「记录指纹必须由同一公钥派生」在 `approve_node` 里由构造保证（指纹取自它随后要绑定/写行的那枚公钥，`trust.rs:956` 与 `trust.rs:976`）。未发现绕过。
- **本地授权未被绕过**：存储层不复核 actor（既有架构，`UseCases::settle_pairing` 的 `require_local` 在 `use_cases.rs:586`）；本轮新增代码没有引入新的入口，也没有把授权判定下沉或删除。
- **审计与容量门方向正确**：批准分支在 `commit` 前调 `enforce_capacity_gate`（`trust.rs:807`），拒绝分支不调（不新增行）；与 §11.2 第 4/5 条「撤销/移除必须保持可用」的方向一致。审计动作仍由调用方写集携带，存储层不合成也不校验（设备/节点路径同口径），与 §11.6「一次调用 = 一个写集（含审计）」一致。
- **两个新风险见 Findings**：`WP6B-1`（基线提交写过的库里 `host_binding = ''` 的存量行）与 `WP6B-3`（已撤销身份经配对恢复）。

### 1.7 条目 7：文档与实现一致性

| 文档位置 | 代码 | 结论 |
|---|---|---|
| §3.5 `PairingRecord`（`docs:138`） | `identity.rs:436-508`、`546-552` | 一致（字段、顺序、`1..=2048`、空 → `Empty`、状态/时间戳校验） |
| §3.5 `PairingPeer`（`docs:142`） | `identity.rs:699-745` | 一致（含「`owned_pairing_peer` 不单独存该列、读取时从配对行回填」↔ `trust.rs:328-335`） |
| §11.1 `owned_pairing`（`docs:1297`） | `migrate.rs:255` + `trust.rs:543-544` | 一致（设备 canonical origin / 节点本机 endpoint） |
| §11.2 第 1/2 条（`docs:1315-1316`） | `trust.rs:596-601`、`722-726`、`769-809` | 一致（逐字相等 + `IdentityMismatch` + 零推进；只接受 `pending_confirmation`；节点 `access` 行、`owner_endpoint` 为空） |
| §11.6 第 3/4 条（`docs:1380-1381`） | `trust.rs:562-686`、`689-812` | 一致（含「身份未被撤销」的守卫描述） |
| §11.1 `owned_pairing_peer`（`docs:1298`） | `migrate.rs:270-281` | **不一致（既有措辞，本轮被新措辞放大）**：该行说表内有「非秘密 endpoint 引用」，实际列里没有任何 endpoint 列；见 `WP6B-2` |

漂移门禁与边界门禁在改后工作区仍为 0（§5/§7 未动，本轮只改 §3.5/§11.1/§11.2/§11.6 的文字）。

## 2. 缺口闭合核对

| 缺口 | 结论 | 证据（文件:行 / 命令） |
|---|---|---|
| WP6-1 采定守卫与重复批准 | **已闭合** | `trust.rs:710-726`（只放行 `pending_confirmation`、`approved` → `Consumed`、写入之前）、`trust.rs:793-802`（条件 UPDATE + 0 行 → `Consumed`）；`admin_store.rs:892-941`（重复批准/批准后拒绝都是 `Consumed`、`approved_at` 不被改写、审计 1 : 0）；`docs:1316` |
| WP6-2 节点配对批准 | **已闭合** | `trust.rs:946-977`（`approve_node`：`NodeKind::Access`、`Paired`、`owner_endpoint = None`、指纹取自 peer 公钥、`upsert_node` + `bind_peer_key`）、`trust.rs:780-792`（目标分派）；`admin_store.rs:731-830`（端口返回值、角色/状态/grants/指纹、配对 `approved`、`owned_node` 1 行、`node.paired` 1 行、重启后公钥逐字节相等）；角色推导见 §1.1（`LOCAL_ADMIN_PROTOCOL.md:550`、`NODE_LINK_PROTOCOL.md:681-743`、`pairing.schema.json:46-52`、`node-link-protocol/src/pairing.rs:109-114`） |
| WP6-3 回滚注入位置 | **已闭合** | `admin_store.rs:2481-2497`（`owned_device` + `owned_audit` 两处注入）、`admin_store.rs:2501-2596`（四面对照 + 移除注入后同一写集成功） |
| WP6-4 `host_binding` 写空串 | **已闭合** | `trust.rs:543-544`（写真实绑定）、`trust.rs:117`（读取还原）、`trust.rs:596-601`（认领逐字比对、零推进）、`trust.rs:328-335`（对端行回填）、`identity.rs:476/717` + `model/tests.rs:1570-1586/1631-1642`（模型非空负例）、`docs:138/142/1297/1315/1380`；代码头不再声称未登记的缺口（`trust.rs:7-13` ↔ `verification.md:74-76/110-111`） |

## 3. Findings

| ID | 严重度 | 类别 | 位置 | 问题 | 证据 | 建议 |
|---|---|---|---|---|---|---|
| WP6B-1 | 提示 | 升级兼容（合同/DDL 边界） | `crates/core/src/model/identity.rs:476`、`crates/storage-sqlite/src/admin/trust.rs:117` | 本轮起 `PairingRecord::try_new` 拒绝空绑定，但 §7.3 的 `owned_pairing.host_binding` 只有 `NOT NULL`、没有非空 CHECK，而 WP6 首版提交（`1baea5b`）一直写 `''`。因此**被首版提交写过的库**里存量的配对行（`host_binding = ''`）在本二进制下：`TrustStore::pairing` 读回 → `InvalidValue::Empty` → `StorageError::InvalidRequest` → `PortError::InvalidRequest("value does not satisfy its domain shape")`；`claim_pairing`/`settle_pairing` 在同一处失败（`trust.rs:581`/`709` 经 `pairing_from_row`），该配对在到期前不可认领，且错误分类是「参数错误」而不是 `Corrupt`。影响有限（配对是一次性 5 分钟窗口，`expire_pairings` 只按 `pairing_id` 扫描、不解析该列，仍会把它终结），但错误分类会误导调用方 | 首版写入点：`1baea5b` 的 `trust.rs` `.bind("")`；模型校验：`identity.rs:476`；读回：`trust.rs:117`；DDL 无 CHECK：`migrate.rs:255`；映射：`error.rs:112-117` → `error.rs:81-113`；扫描不受影响：`trust.rs:824-831` | 二选一：① 在打开库时（或首次扫描时）把 `host_binding = ''` 且未终态的配对一并终结/清理，并写一条具名审计；② 让读路径把空绑定当「无绑定」处理并给出具名冲突（例如 `Corrupt("pairing has no host binding")`），避免落到 `InvalidRequest` 的通用文案 |
| WP6B-2 | 提示 | 文档（措辞残留） | `docs/CORE_PORTS_AND_STORAGE.md:1298` | §11.1 的 `owned_pairing_peer` 行仍声称该表含「非秘密 endpoint 引用」，但该表没有任何 endpoint 列（也不该有：本轮新写的 §3.5 明说绑定不单独存、读取时从 `owned_pairing` 回填）。同一行的「角色」措辞也容易与「写集不携带角色、由流程推导」的新决定混淆 | `docs:1298`（措辞）vs `migrate.rs:270-281`（列）vs `docs:142`（新措辞）；`trust.rs:55-57`（读列注释） | 把该行改为只描述真实列（peer ID、名称、公钥、指纹、clientNonce、claimed_at），并注明绑定与角色都不在该表（指向 §3.5/§11.2 第 2 条） |
| WP6B-3 | 提示 | 合同一致性（需合同侧确认） | `crates/storage-sqlite/src/admin/trust.rs:962-966` | 本轮新实现的节点批准路径在既有角色行已 `Revoked` 时返回 `Conflict(IdentityMismatch)`（与 `put_node`、与既有设备路径同口径）。而 `peer-identity-material` 能力的场景写「该身份保持已撤销，**只有按协议完成重新配对才能恢复**」，`settle_pairing`（`node.pair.confirm`/`device.pair.confirm`）恰恰是这条协议路径：一个被撤销的 `NodeId` 因此只能永远停在 `revoked`（§11.3 又要求撤销 tombstone 不因容量压力删除），除非改用新身份。是否允许「同一 id 经重新配对复活」属合同选择，但两处权威文本目前方向相反 | 守卫：`trust.rs:962-966`；改后的合同把守卫写成规则：`docs:1381`（「…身份未被撤销」）、`docs:1379`（`put_node` 只禁普通 upsert）；相反的场景：`openspec/changes/admin-state-persistence-v2/specs/peer-identity-material/spec.md:65-66`；撤销永久化的依据：`docs:1328`；新身份重配的依据：`SECURITY_DESIGN.md:519`、`docs/INITIAL_DESIGN.md:525-531` | 二选一并在同一处对齐：① 明确「撤销是改身份前不可逆」→ 把 spec 场景的措辞改成「只能以新身份重新配对」；② 明确「同一 id 可经重新配对恢复」→ 两条 `approve_*` 需要一条 revocation-aware 分支（例如仅在 `RevokeReason::UserRequested` 且指纹与既存材料一致时允许），并补回归用例 |

以上三条均为提示级：无「合同违规导致功能不可用」或「数据/授权错误」性质的项。

## 4. 结论

**无未解决阻断项。** 本轮 5 条改动面与 `reports/rv1-wp6.md` 的 4 条 findings 逐条核对的结果是：WP6-1、WP6-2、WP6-3、WP6-4 全部**已闭合**（节点批准真的落 `owned_node` 的 `access` 行 + 身份材料 + `approved` + 审计；认领阶段的绑定比对逐字相等、零推进；两个新用例与改造后的回滚注入用例都是可证伪的；模型/文档与实现逐条对得上）。两道 Node 门禁（`check-contract-drift`、`check-crate-boundaries`）在改后工作区仍退出 0。

3 条提示级 finding 不阻断合入：`WP6B-1` 只影响被首版提交写过的库且会自愈（到期扫描），`WP6B-2` 是 §11.1 一处既有措辞，`WP6B-3` 需要合同侧在「撤销是否可经重新配对恢复」上二选一后对齐文本。

**本轮未覆盖**：`server`/`identity-auth` 把 `canonicalOrigin`/`endpoint` 送进写集的接线（尚无实现，属后续 WP）、`migrate.rs`/夹具（W0 稳定）、`cargo` 三道门禁（实现者已跑，本轮按 assignment 未复跑）。闭合结论依赖两条调用方前提，建议在接线时固化为测试：① 登记时写入的绑定必须是本机自己的 canonical origin / 本机 endpoint，且认领时传入同一段文本；② 节点配对只由 owner 方向产生（对端角色由协议拓扑推导，不在写集里）。
