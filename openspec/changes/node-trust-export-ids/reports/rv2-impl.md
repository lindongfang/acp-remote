# RV2-IMPL 独立代码检视报告（recheck：RV1-IMPL-F4/F5/F6 + R9 覆盖缺口，fix 轮次 `5190924d` → `e4c980c`）

```yaml
task_id: "2.1 / 2.2 / 2.3 / 2.4（合并为主行；逐条见 handoff_index）"
role: reviewer
phase: review
stage: work-package
agent_context: "新建 reviewer 子 Agent（本报告作者）；不继承任何实现/修复对话，未参与本变更任何实现、修复或实现讨论；只读检视（除本报告文件外未修改任何文件），未执行 cargo/npm、未执行 E2E、未切分支、未提交"
target_revision: e4c980c029a97966c3229ac067db9a061817b04d
scope: "git -C D:/Project/acp-remote-wt/export-ids diff 5190924d041dcc7ab9bd93b4541b215f25f26bea..e4c980c029a97966c3229ac067db9a061817b04d 的全部 5 文件（+367/−2，全在 crates/**；HEAD 工作区干净），以及其调用方与数据流：FakeTrust::revoke_node / 真 revoke_node、FakeTrust 与真 settle_pairing（Device 与 Node 两个分支）、Authority::settle 的产出形状、TryRouter 的 node.pair.confirm 全链路、app 集成测试的 support 装配（OwnerNode）、check-crate-boundaries.mjs 与 MODULE_ARCHITECTURE.md §5 矩阵；并顺带核对 plan.md alternative_checks、design.md D9/D8、verification.md 的 F1/F2/F3 文本"
changes: "本次检视为只读：未修改 worktree 内代码、测试、规划或任务状态；仅新增本报告"
checks: "[PV1]（fix 轮次 Rust 侧）已核对（NEW，主 Agent 亲跑日志 reports/du1-pv1-fix.log，我独立逐行核算）；[PV1]（合同门禁 npm run check）、[PV2]、[PV5] 待返回（PENDING）"
issues: "3 条新发现：1 MINOR + 2 SUGGESTION；被复核的 RV1-IMPL-F4/F5/F6 与 R9 缺口全部判「已解决」，未发现新回归、无 CRITICAL/MAJOR"
result: PASS
evidence_paths:
  - "D:/Project/acp-remote/openspec/changes/node-trust-export-ids/reports/rv2-impl.md（本报告）"
  - "D:/Project/acp-remote/openspec/changes/node-trust-export-ids/reports/rv1-impl.md（原报告，本轮 Previous Findings 来源）"
  - "D:/Project/acp-remote/openspec/changes/node-trust-export-ids/reports/du1-pv1-fix.log（主 Agent 亲跑，非实现者自述；我独立核算 86 行 test result / 992 passed / 0 failed / 2 ignored / exit=0）"
  - "D:/Project/acp-remote/openspec/changes/node-trust-export-ids/reports/handoff-coder-impl.md（fix 轮次一节，实现者说明，仅作对照不作为结论依据）"
resource_cleanup: "未创建任何资源（未跑 cargo/npm、未建临时目录、未起进程、未占端口）；本报告为唯一写入"
```

## 1. Review Context

- **Review ID / Type / Stage**：RV2-IMPL / `recheck` / work-package（代码 fix 轮次后）。**Previous Findings**：RV1-IMPL 的 F4（SUGGESTION）、F5（SUGGESTION）、F6（SUGGESTION，主 Agent 裁定「修成显式拒绝」）、R9 组合断言缺口（MINOR，覆盖缺口）。
- **Repository / 版本**：被检 worktree `D:\Project\acp-remote-wt\export-ids`（分支 `agentic/node-trust-export-ids`），**Base `5190924d`（RV1-IMPL 的 target）→ Target `e4c980c`**；`git status --porcelain` 为空（工作区与 target 一致）。fix 轮次由三个提交组成：`d5a33eb`（F6 + 真存储注释）、`1304f16`（F4 + F5）、`e4c980c`（R9）；起点为 RV1-IMPL 的 target `5190924d`。
- **越界文件核对**：`git diff --name-only 5190924d..e4c980c` = 5 个文件，全部在 `crates/**`；`-- docs schemas fixtures compatibility Cargo.toml Cargo.lock openspec` = **0 个文件** ⇒ 无合同资产越界、无新增依赖、本次 fix 未动任何规划文本。
- **读取的规则与需求**：`AGENTS.md` §3（不变量：授权字段不得静默丢弃）、§4/§9；`docs/MODULE_ARCHITECTURE.md` §5 矩阵与其表下注记；`scripts/check-crate-boundaries.mjs`（依赖来源与判据）；本变更的 `design.md`（D1–D10，含 D9 勘误）、`specs/local-admin-methods/spec.md`（R9）、`specs/storage-schema-v2-migration/spec.md`、`plan.md`（含修订后的 `alternative_checks`）、`tasks.md` 2.1–2.4、`verification.md`（Review Findings 表与 Check Plan Changes）。
- **实际检查范围**：自读 base→target 完整 diff（三个提交逐文件）＋被修复点的全部调用方与数据流：`FakeTrust::revoke_node` 与真 `revoke_node` 的列级差异、`settle_pairing` 的两个分支与事务内写入顺序、`Authority::settle` 的产出形状（清单是否为空的唯一来源）、`node.pair.confirm`/`node.revoke` 的 router 侧后置动作、`node.pair.begin`/`claim_pairing`/`approve_node` 是否有「已配对不得重配」守卫、新测试的全部 support 依赖（`support::owner::OwnerNode`、`support::nodelink`）与断言判别力、依赖矩阵与门禁脚本对 dev-dependencies 的判定范围。
- **限制**：本角色不执行 cargo/npm/E2E；测试结论来自主 Agent 的 `du1-pv1-fix.log`（我逐行核算）与代码推演，不来自实现者自述日志。

### 复核结论一览

| 原问题 ID | 复核状态 | 一句话依据 |
| --- | --- | --- |
| RV1-IMPL-F4（替身撤销丢清单） | **已解决** | `FakeTrust::revoke_node` 改为 `record.export_ids().to_vec()`，与真存储（只 UPDATE `state`/`revoked_at`/`revoke_reason`）同形；既有 server 断言全部使用空清单，行为等价；无「声称覆盖清单校验」的空转断言 |
| RV1-IMPL-F5（错别字落点） | **已解决** | 实际落点确为 `crates/server/src/node_link/catalog.rs:254`（RV1-IMPL 报告里写的 `core/src/model/identity.rs` 是误记）；改为「只能收窄不放宽」；`core` 的 `broker.rs:658`/`identity.rs:308-309` 本来就是正确表述，全仓库已无「掉宽」；纯注释、无语义漂移 |
| RV1-IMPL-F6（设备方向静默忽略清单） | **已解决** | Device 分支在 `approve_device` 之前显式拒绝非空清单（`InvalidRequest`），事务内零写入零审计；新用例判别力可从代码路径证明；core 侧不加对称限制的理由成立，且不存在可静默丢弃清单的生产路径 |
| RV1-IMPL-R9（缺 router+真存储的组合断言） | **已解决（缺口关闭）** | 新用例经真实 `LocalAdminRouter` + 真 `SqliteStore`；两个错误码各一条、消息级断言指向具体规则、三类零写入均有具名断言且判别性被正对照支撑；放 `crates/app/tests/**` 的理由与矩阵/门禁判定一致 |
| RV1-IMPL-F1 / F2（规划/设计文案） | **已闭环（与实现一致）** | `plan.md` 的 `alternative_checks` 第 1 条与 `design.md` D9 勘误段已与实现（`import_add` 恒 `local.unavailable`；`attach` 两步判定）逐项对上 |
| RV1-IMPL-F3（收窄不作用于既有 attachment） | **裁定结论不矛盾；理由中的可达性论断不准确（新发现 RV2-IMPL-F1，MINOR）** | 「登记为已知限制 + 后续切片要求」与实现一致，且缺口与既有条件②同源、本次未使其变差；但「改 = 撤销重配 / 不可达」与代码不符：**不撤销**的重新配对同样会改写清单，且不会关闭既有连接 |

## 2. 逐项复核（按原问题 ID）

### 2.1 RV1-IMPL-F4：替身撤销保留清单 —— 已解决

- **修复内容**（`1304f16`，`crates/server/src/local_admin/test_support.rs`）：`FakeTrust::revoke_node` 重建 `NodeRecord` 时由 `Vec::new()` 改为 `record.export_ids().to_vec()`，并加注释点明「撤销只改 `state`/`revoked_at`，不清理清单」。
- **与真存储同形**（我逐列核对 `crates/storage-sqlite/src/admin/trust.rs:572-590`）：真 `revoke_node` 只执行 `UPDATE owned_node SET state='revoked', revoked_at=COALESCE(...), revoke_reason=COALESCE(...)`，**不触碰** `export_ids_json`；替身现在也保留清单。`1304f16` 同时在真存储的文档注释里写明同一句（`d5a33eb` 提交内含该注释），两处口径一致。
- **「既有 server 断言无变化」属实**：我逐个核对 server 侧所有会走到 `node.revoke` 的用例——`router.rs:2891`（确认时 `exportIds: []` → 断言 `nodes[0]["exportIds"] == []` → 随后撤销）、`router.rs:3305`（`exportIds: []` → 撤销 → 只断言 `states`）、`router.rs:2687/2704/2724/3349`（撤销的重试/未知 id 路径）——**全部使用空清单**，因此 `to_vec()` 与 `Vec::new()` 在这些断言上等值；加上 fix 轮次 [PV1] 全 workspace 绿（见 §5），可确认无既有断言被改坏。
- **「已知差异」（`FakeTrust::settle_pairing` 的 Node 分支仍不校验清单）不会让某条已存在断言空转**：
  - 我检索了 server 侧所有与清单相关的断言点：`router.rs:3103-3167`（`node_pair_confirm_requires_and_forwards_the_export_ids`）的 ③ 只断言「乱序+重复 → 归一化后回显与落盘同序＝`["export-a","export-b"]`」，其对象是**参数形状与透传**，不是清单校验；①② 覆盖缺字段/类型非法（在 `params::node_pair_confirm` 阶段就拒绝，与存储无关）。
  - `node_link/catalog.rs` 的可见性用例直接构造 `NodeRecord`（不经 settle）；`node_link/resource/tests.rs:686` 的 `row.export_ids().len()` 是「用例前提」断言，也直接构造记录。⇒ **没有任何测试声称覆盖「清单校验」却走替身而恒真**。
  - 因此该已知差异的后果只是「server 层无法验证存储侧校验规则」——而这正是 R9 缺口的成因，已由 §2.4 的 app 层用例改为走真存储后闭合。
- **残余**：替身与真存储的这条差异按 fixer 说明**刻意保留**（替身在节点方向本就不校验，只补设备方向会「一半像一半不像」）。我同意该取舍：新用例已改走真存储，且无断言依赖替身的校验行为。**判「已解决」，剩余为已登记的已知差异，不作为 finding。**

### 2.2 RV1-IMPL-F5：错别字落点与措辞 —— 已解决

- **落点核对**：`git diff 5190924d..e4c980c` 中唯一的词句改动在 `crates/server/src/node_link/catalog.rs:254`：`清单只能收窄不掉宽` → `清单只能收窄不放宽`。RV1-IMPL 报告正文把位置写成 `core/src/model/identity.rs`（F5 行的表格里给的是 `catalog.rs:247-254`）——fixer 采用了后者，**这是正确落点**（`identity.rs` 与 `broker.rs` 本来就没有该错别字）。
- **core 侧本来正确**：`crates/core/src/broker.rs:658`「清单只收窄不放宽」、`crates/core/src/model/identity.rs:308-309`「只能收窄可见性，不能放宽 `scopes ∩ grants`」——无需改动，fixer 未动它们。
- **无同类错别字残留**：`grep -rn 掉宽 crates/` 零命中；其余「掉」用法均为「去掉/丢掉/删掉/摘掉」等正常词。
- **无语义漂移**：该改动是 `///` 文档注释（`visible_exports` 的说明），代码行为、wire、错误码、门禁资产全部不变。**判「已解决」。**

### 2.3 RV1-IMPL-F6：设备配对携带清单时显式拒绝 —— 已解决

- **落点与位置正确**（`d5a33eb`，`crates/storage-sqlite/src/admin/trust.rs` 的 `TrustStore::settle_pairing`）：`PairingTarget::Device` 分支在既有 `if !granted_grants.is_empty()` 之后新增 `if !granted_export_ids.is_empty() → PortError::InvalidRequest("a device pairing must not carry export ids")`。与 grants 同处、同口径，符合 `AGENTS.md` §3「授权相关字段不得静默丢弃」。
- **「在事务内任何写入之前（零写入零审计）」成立**：我通读了 `settle_pairing` 从 `BEGIN IMMEDIATE` 到该 match 的全部语句——只有 `SELECT`（配对行、peer 行）、纯值校验（终态/已认领/`pending_confirmation`/过期/子集判定），**没有任何 INSERT/UPDATE**；guard 命中即 `return Err`，事务对象被丢弃 ⇒ 整事务回滚，到不了 `approve_device`、`UPDATE owned_pairing`、`insert_audit_rows` 与 `enforce_capacity_gate`。
- **新用例 `a_device_pairing_must_not_carry_export_ids` 有判别力**（`crates/storage-sqlite/tests/admin_store.rs`）：
  - 我核对了输入构造：`device_pairing(30)` 的 `requested_scopes = ["session.read"]`、`requested_grants = ∅`；`approved_settlement()` 的 `granted_scopes = ["session.read"]`、`granted_grants = ∅` ⇒ 子集校验与「设备不得带 grants」守卫**都通过**，失败只能来自新 guard；断言 `assert_invalid_request(error, "a device pairing must not carry export ids")` 是**逐字相等**（不是 `contains`）。
  - 判别性反推：若移除新 guard，`approve_device` 会执行并提交，`expect_err("设备配对带清单必须拒绝落定")` 立即失败 ⇒ 用例必红。这与 fixer 自报的「临时移除守卫 → FAILED」负向对照一致（该对照我未复跑，属实现者自述；但代码路径已可独立证明）。
  - 零写入断言覆盖四类可观察面：`store.devices()` 空、`peer_key(Device)` 为 `None`（身份材料未写）、配对仍 `PendingConfirmation`、原始 SQL 的 `owned_device` 计数 0 与 `PairingAuditApproved` 审计 0。
- **core 侧未加对称限制的理由成立**：`PairingSettlement::validate()` 只接受 `&self`（`identity.rs:909-917`，`Approved` 直接 `Ok(())`），确实拿不到 `PairingTarget`；而 `granted_grants` 的「设备不得携带」这一孪生规则**也只**在存储层的同一分支（core 里没有对应校验）。若在 `UseCases::settle_pairing` 再加一条，同一条规则就被拆到两处（正是本次要对齐的反面）。⇒ 按「同一处对称」只落在存储层是自洽的。
- **不存在可静默丢弃清单的生产路径**：`UseCases::settle_pairing` 的生产调用点穷举为 `local_admin/router.rs` 的 603（设备，`IdentityAuthoirty::settle` 产出）、850（节点，紧接着 `.with_granted_export_ids(confirm.export_ids)`）、989（拒绝分支）；`Authority::settle` 的 `Approve` 路径**只**调 `PairingSettlement::approved(...)`（`identity-auth/src/pairing.rs:308-311`），而 `approved()` 把 `granted_export_ids` 固定为 `Vec::new()`（`identity.rs:868-874`）⇒ 非空清单**只能**来自节点分支的 `with_granted_export_ids`。设备方向因此是「构造错误」而非可达路径，guard 属防御性硬化；`reject` 变体没有该字段，也无处丢弃。**判「已解决」。**

### 2.4 RV1-IMPL-R9：无效清单条目的组合断言 —— 已解决（缺口关闭）

- **确实走 router + 真存储**：新用例用 `support::owner::OwnerNode::admin/call`，其内部是 `self.router.handle(request)`，而 `OwnerNode::start_with` 用同一个 `Arc<SqliteStore>` 装配全部端口（`let trust: Arc<dyn TrustStore> = store.clone();` 并注入 `UseCases`/`LocalAdminRouter`），配对 claim 走真实 loopback listener（`127.0.0.1:0`）+ 真实 `Authority`。⇒ `settle_pairing` 落在真 `SqliteStore`，`NotFound(Export)` → `map_port_error` → `local.not_found` 的**组合**路径被真实执行（这正是 R9 缺的那一步）。
- **断言强度**（两个用例各覆盖一条失败分支）：
  - 错误分类与归因：`local.not_found` + 消息含 `export.never-created`（`EntityRef::Export` 的 `Display` = `export:<id>`，`map_port_error` 保留适配器原因 ⇒ 该断言只能由「存储层报出这个具体 id」满足）；`local.invalid_params` + 消息含 `must intersect the granted grants`（`validate_export_ids` 的固定文案，`require_confirm_subset` 的文案是 `must not exceed` ⇒ 能区分判定点）。
  - 零写入三件：不建信任（`node.list` 的 `nodes` 为空）、不写审计（`audit.export` 读真实 SQLite 落下的文件、按 `categories=["node.paired"]` 计数为 0）、不推进配对状态（同一份配对随后仍能成功确认，并把清单落到信任行上）。
  - **判别性有正对照**：第一例在成功确认后用**另一个**输出路径再数一次 `node.paired` = 1，证明前一次的 0 不是「过滤器把记录全滤掉」造成的空转；第二例用「换成清单内 id 仍能确认」证明配对未进终态。
  - 真实入口未被 mock 绕过：唯一替身是脚本化 Access 客户端（与既有 `node_link_e2e` 同口径）；Agent 后端不参与本用例。
- **放 `crates/app/tests/**` 的理由成立**：
  - `docs/MODULE_ARCHITECTURE.md` §5 矩阵中 `server` 行、`storage-sqlite` 列为**空白**；`scripts/check-crate-boundaries.mjs` 从 `cargo metadata` 取 `packages[].dependencies`（按脚本注释即 `[dependencies]`/`[dev-dependencies]`/`[build-dependencies]` 全部）并只对**指向工作区成员**的边（`dependency.path` 存在）逐条比对，`server → storage-sqlite` 会命中「违反 §5 依赖矩阵（该格为空白）」⇒ **dev-dependency 同样被拦**，fixer 的判断正确。
  - `app` 是组合根：它的矩阵行全为 ✓，天然同时具备真 router 与真存储，且已有 `crates/app/tests/audit_export.rs` 的「为什么放在 app」先例。⇒ 位置选择无「违反依赖方向」问题。
- **残余（不阻断，登记）**：R9 的第三类失败（清单条目**已撤销**）在 router 组合层仍无独立断言；但它在存储层由 `admin_store.rs::node_pairing_approval_rejects_invalid_export_ids_without_writing` 逐项覆盖，且与「不存在」**共用**同一条 `NotFound(EntityRef::Export)` → `local.not_found` 映射（`validate_export_ids` 同一分支），故 router 层的两条断言已足以固定「两个错误码各自映射正确」。**不要求补测。** **判「已解决」。**

### 2.5 顺带核对：F1 / F2 的文案修正与实现一致

- **F1（`import.add`）**：`crates/server/src/local_admin/router.rs:437-443` 的 `import_add` 无论参数是否合法都返回 `LocalErrorCode::Unavailable`（注释亦如此）；`plan.md:245` 的新文案「`import.add` 在本切片**恒**返回 `local.unavailable`（Access 侧客户端未落地 → 无 catalog 快照），因此不断言它的 `local.not_found`」与实现一致；`design.md` D9 同句一致。
- **F2（`attach` 错误码）**：`resource.rs` 的 `on_attach` 是实现的两步判定——① `!owner_matches`（`ownerNodeId` 非本机）或 `export_id`/`session_id` 解析失败 → `ErrorCode::ExportNotFound`；② `catalog::export_is_visible` 返回 `Ok(false)`（未配对/未导出/已撤销/不相交/**不在清单内**）→ `ErrorCode::ExportNotGranted`。`design.md` D9 勘误段的两句与之一一对应（含「`export.not_found` 只用于 `ownerNodeId` 不是本机或 id 不可解析」），且两个码都在冻结词表内、`compatibility/**` 零改动 ⇒ 「不改 wire」的裁定成立。
- **`verification.md` 的 Check Plan Changes** 已登记该勘误（原值/新值/理由/影响齐全），与该两处文本互不矛盾。

### 2.6 顺带核对：F3 的裁定记录与实现

- **裁定结论与实现不矛盾**：「收窄不作用于已建立的 attachment」确实是现状——`snapshot`/`replay` 只走 core 的 `node_link_session_view`/`node_link_replay` 前置（已配对 ∧ Export 未撤销 ∧ 会话 Agent 属于该 Export），不含条件②也不含条件③；把它登记为**已知限制 + 后续切片要求**（新增运行期编辑入口时必须同时把清单条件加进读 seam 或强制重握手）与代码一致，本次也确实未加运行期校验。命令面、catalog、`resource.attach` 三处都按**当次持久记录**实时判定，因此可观测的授权面没有被放宽。
- **但理由中的可达性论断不准确** ⇒ 见 Findings 的 **RV2-IMPL-F1**（MINOR）：`approve_node` 对既有的 `paired` 行没有状态守卫（只校验指纹一致），`claim_pairing` 也没有「该对端已配对」的守卫，而 `node.pair.confirm` **不**调用 `close_node`（只有 `node.revoke` 调）⇒「不撤销、直接重新配对」即可改写清单，且既有连接与 attachment 继续存活。裁定结论（已知限制）与缺口相对条件②的**既有性**不受影响，故不阻断；需要的是更正措辞，并据此把「后续切片要求」的触发条件写准。

## 3. Findings（本轮新发现）

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| RV2-IMPL-F1 | MINOR | 裁定文本：`verification.md:96`（RV1-IMPL-F3 行）与 `plan.md:15`/`tasks.md:17`/`verification.md:50` 的同一措辞；权威文档：`docs/NODE_LINK_PROTOCOL.md` §8.2（docs worktree `f47f9da` 第 278 行「没有『配对后修改清单』的入口，修改 = `node.revoke` + 重新配对」）、`design.md` D8 | 代码依据（target `e4c980c`）：`crates/storage-sqlite/src/admin/trust.rs::approve_node`（无状态守卫，只比指纹；`upsert_node` 的 `ON CONFLICT ... DO UPDATE SET ..., export_ids_json = excluded.export_ids_json` 会覆盖清单）、`claim_pairing`（无「已配对不得再认领」守卫）、`crates/server/src/local_admin/router.rs:935`（`close_node` 只在 `node.revoke` 调用；`node.pair.confirm` 不关闭任何连接）⇒ 「同一 Access 节点**未撤销**时重新配对并收窄清单」在当前 API 面**可达**，而既有 attachment 的 `snapshot`/`replay` 不复查清单（§2.6） | 措辞与事实不符：裁定理由写成「不可达 / 改 = 撤销重配」，会让后续读者误以为该缺口的唯一触发条件是 `node.revoke`（而 `revoke` 会关连接，从而以为缺口永不发生）。裁定的**结论**与实现不矛盾，且缺口相对既有条件②同源、本次未使其变差，因此不阻断 | 把「修改 = `node.revoke` + 重新配对」改为「目前只能重新配对（`node.pair.confirm`）改写清单；撤销只是常见前置，不是必需」；并把「后续切片要求」的触发条件写成「新增任何运行期清单编辑能力，**以及重新配对路径本身**——必须同时把清单条件加进读 seam 的硬校验或强制重握手（例如在重新落定时关闭该节点连接）」。（措辞与文档由主 Agent/WP5 侧修改，本角色不改。） | 新增（本轮未解决，非阻断） |
| RV2-IMPL-F2 | SUGGESTION | `crates/server/src/node_link/catalog.rs:619-627`（`visibility_narrows_to_the_nominated_export_ids_without_widening` 末段） | `visible_exports(Some(&node_row), &exports)` 只接受 `&NodeRecord`，纯函数无副作用 ⇒ `assert_eq!(node_row.export_ids(), [E1, E2])` 的「清单不被级联清理」**恒为真**，不可能失败（RV1-IMPL 的 R4 行把它列为证据之一） | 无功能影响；仅是一处「看起来有判别力、实际不可失败」的断言，会让覆盖表对 R4 的强度略微高估。R4 的**真实**覆盖来自 `admin_store.rs`（`revoke_export` 后逐字核对 `export_ids_json` 列），我的覆盖判断据此不下调 | 删除该断言，或改为经真存储的 `revoke_export` → 读该列的断言（后者已有，可直接改为引用） | 新增（非阻断；**属既有代码，本次 fix 未引入**） |
| RV2-IMPL-F3 | SUGGESTION | `crates/server/src/local_admin/router.rs:3143-3167`（`node_pair_confirm_requires_and_forwards_the_export_ids` ③） | 该步确认 `exportIds = ["export-b","export-a","export-b"]`，而该用例的 `FakeTrust` world 里**不存在**这些 Export；断言能通过只是因为替身不校验清单（真实存储会回 `local.not_found`） | 无功能影响、也**不是**「声称覆盖清单校验却恒真」的空转断言（该用例的主体是参数必填/类型/归一化透传）。但它是一处替身与真存储的**行为分歧点**，改名的读者可能误以为「这些 id 合法」 | 在 ③ 上加一行注释点明「本步只验证参数形状与归一化，id 是否存在由存储层用例覆盖」，或把示例 id 换成「本 world 里真实存在的名字」以免读者误解 | 新增（非阻断） |

**未发现问题**：其余复核点（F4 的替身保真、F5 的注释、F6 的 guard 位置与负向对照、R9 的两个用例与放置理由、F1/F2 文案）均无新发现；本轮无 CRITICAL、无 MAJOR。

## 4. 回归检查（fix diff 范围）

| 检查项 | 结论 | 依据 |
| --- | --- | --- |
| 改动范围只含 `crates/**` | ✅ | `git diff --name-only 5190924d..e4c980c` = 5 文件（`app/tests/node_pair_export_ids.rs`、`server/src/local_admin/test_support.rs`、`server/src/node_link/catalog.rs`、`storage-sqlite/src/admin/trust.rs`、`storage-sqlite/tests/admin_store.rs`） |
| `docs/schemas/fixtures/compatibility/Cargo.toml/Cargo.lock/openspec` 零改动 | ✅ | 同一条 `git diff --name-only ... -- <这些路径>` = **0**；`Cargo.*` 零改动 ⇒ 无新增依赖 |
| 无被跳过的测试、无弱化断言 | ✅ | diff 中无 `#[ignore]`/`allow(`；删除行只有 `FakeTrust` 的 `Vec::new()`（一行，替换而非删除断言）；`ignored` 计数与 base 同形（2） |
| 替身改动不改变既有断言 | ✅ | server 侧所有 `node.revoke` 路径的信任行清单均为空（§2.1 逐条列举）⇒ 替换等值；[PV1] fix 轮次全绿佐证 |
| 新增用例不泄漏临时目录/端口 | ✅ | app 侧用 `support::TempRoot`（`Drop` 内 10×50ms 重试删除，panic 展开路径同样生效）、storage 侧用 `support::TempDir`（`Drop` 兜底）；监听一律 `127.0.0.1:0`；用例末尾显式 `owner.stop()`；失败时 `block_on` 的 runtime 随栈展开销毁任务。与既有 `node_link_e2e`/`admin_store` 同款，无新增泄漏面 |
| 新测试目标被自动发现且能编译 | ✅（间接） | `crates/app/Cargo.toml` 无显式 `[[test]]`，走自动发现；support 的两个模块与所用符号（`OwnerNode::{start,admin,call,data_dir,stop,addr}`、`nodelink::{ticket_from_url,claim_request,post_json,verify_owner_proof,nonce_text}`、`server::node_link::CLAIM_PATH`）我逐个核对存在且签名匹配；[PV1] fix 轮次的 86 行 summary 含 `node_pair_export_ids`（2 passed）与 `admin_store`（43 passed） |
| 工作区状态 | ✅ | target 上 `git status --porcelain` 为空，检视版本稳定（非滚动工作区） |

## 5. 检查证据核对

- **[PV1]（fix 轮次，Rust 侧）＝ 已核对（NEW，独立核算）**。`reports/du1-pv1-fix.log`（`e4c980c`，2026-09-27 09:22）：`cargo fmt --all -- --check` `exit=0`；`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` `exit=0`；`cargo test --locked --workspace --all-features` 共 **86** 个 `test result` 行、`passed` 求和 **992**、`failed` 求和 **0**、`ignored` 求和 **2**、末尾 `exit=0`（我用 awk 逐行重算，与实现者与主 Agent 的陈述一致）。相对 base `5190924d` 的 989（85 行）＝ **+3**（`node_pair_export_ids` 2 + `admin_store` 1），与 fix diff 新增用例数吻合。
  这是**主 Agent 亲跑**的证据（非实现者自述），我按「命令 / 退出码 / 逐目标汇总」逐项核对。
- **[PV1]（合同门禁 `npm run check`）＝ 待核对（PENDING，集成阶段）**。`check:drift`/`check:docs`/`check:agentic` 需在含完整变更目录的权威树、且代码与文档两支并入后跑。**不影响**本轮判断：本 fix 未动任何合同资产、文档、脚本、依赖（§4 已证零改动），门禁结果不可能因这 5 个文件而变化（`check:drift` 只看 `migrate.rs`/`ports.rs`，本次未动）。
- **[PV2]（`check-crate-boundaries.mjs`）＝ 待核对（PENDING，集成阶段）**。本轮 fix **未新增任何依赖**（含 dev-dependency），因此它对本轮结论的影响面为零；但按规则不能替它记 PASS。
- **[PV5]（受控路径 E2E 留证）＝ 待核对（PENDING）**，本轮不涉及 E2E 变更；fix 前的 `alternative_checks` 文案问题（RV1-IMPL-F1）已由主 Agent 修正（§2.5）。
- **不据本报告宣称**：本轮结论只覆盖 `e4c980c` 的静态代码检视 + 对 [PV1] Rust 侧日志的独立核算；**不代替** Project Verify、合同门禁与 [PV5]，也不代表可归档。

## 6. Assessment

- **结论：PASS（对应 Target Revision `e4c980c029a97966c3229ac067db9a061817b04d`）**。在只读、未继承实现/修复对话的隔离条件下，我自读了 base→target 完整 diff 与全部受影响调用链，逐项复核 RV1-IMPL 的 4 项并检查回归：**F4/F5/F6/R9 全部判「已解决」**；顺带核对的 F1/F2 文案与实现一致、F3 的裁定结论与实现不矛盾；**未发现 CRITICAL/MAJOR，未发现修复引入的回归**。新发现 1 MINOR（RV2-IMPL-F1，修正裁定理由中的可达性论断 + 后文措辞）与 2 SUGGESTION（RV2-IMPL-F2/F3，均为既有代码的非阻断项），按收口约定不阻塞收口。
- **不可据本报告宣称的事**：本轮**不代替** Project Verify（[PV1] 合同门禁/[PV2]/[PV5] 仍 PENDING），不代表任务状态可勾选，也不代表变更可归档。
- **残余不确定项**：① RV2-IMPL-F1 的触发路径我通过 `approve_node`/`claim_pairing`/`close_node` 三处的代码穷举确认「不撤销可重配」，但**未实际构造**该端到端场景（需真 listener + 二次 claim）；若主 Agent 认为需要运行期证据，应作为用例交测试角色，本角色只报静态结论。② fixer 自报的 F6 负向对照（临时移除 guard → 用例失败）我**未复跑**，但已从代码路径独立证明其判别力（§2.3）。③ R9 的「已撤销清单条目」在 router 组合层无独立断言（§2.4 残余，已说明为何不要求补测）。

### handoff_index

```yaml
handoff_index:
  - task_id: "2.1"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: e4c980c029a97966c3229ac067db9a061817b04d
    evidence_type: REVIEW
    evidence_id: RV2-IMPL
    report_path: openspec/changes/node-trust-export-ids/reports/rv2-impl.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 fix 轮次 HEAD e4c980c（起点 5190924d）上只读复核 RV1-IMPL-F4/F5/F6 + R9 覆盖缺口与回归：storage 侧 F6 guard 与其用例（事务内零写入零审计、判别力由代码路径独立证明）"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.2"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: e4c980c029a97966c3229ac067db9a061817b04d
    evidence_type: REVIEW
    evidence_id: RV2-IMPL
    report_path: openspec/changes/node-trust-export-ids/reports/rv2-impl.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "同上；core 侧未加对称限制的理由经 `PairingSettlement::validate()` 签名与 `Authority::settle` 产出形状核对，确认无「可静默丢弃清单」的生产路径（本轮 fix 未动 core）"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.3"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: e4c980c029a97966c3229ac067db9a061817b04d
    evidence_type: REVIEW
    evidence_id: RV2-IMPL
    report_path: openspec/changes/node-trust-export-ids/reports/rv2-impl.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "同上；server 侧 F4（FakeTrust::revoke_node 保留清单）与 F5（catalog.rs:254 注释）已解决，并核对既有 server 断言全部使用空清单、无空转断言（RV2-IMPL-F3 为 SUGGESTION 级备注）"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.4"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: e4c980c029a97966c3229ac067db9a061817b04d
    evidence_type: REVIEW
    evidence_id: RV2-IMPL
    report_path: openspec/changes/node-trust-export-ids/reports/rv2-impl.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "同上；R9 缺口关闭：新用例经真实 LocalAdminRouter + 真 SqliteStore（OwnerNode 装配已核对），两条失败分支的错误码/消息/零写入断言具判别性；放置理由与 §5 矩阵及 check-crate-boundaries.mjs 的判定范围一致"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.1 / 2.2 / 2.3 / 2.4"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: e4c980c029a97966c3229ac067db9a061817b04d
    evidence_type: CHECK
    evidence_id: PV1
    report_path: openspec/changes/node-trust-export-ids/reports/rv2-impl.md
    result: PASS
    evidence_status: REUSED
    applicability_basis: "复用主 Agent 在**同一** target_revision 上亲跑的 reports/du1-pv1-fix.log（fmt/clippy/test 三条，exit=0；86 行 test result、992 passed、0 failed、2 ignored）——实施者为主 Agent 而非实现者；本角色只做逐行独立核算，未重跑"
    source_evidence:
      id: PV1
      report_path: openspec/changes/node-trust-export-ids/reports/du1-pv1-fix.log
      target_revision: e4c980c029a97966c3229ac067db9a061817b04d
  - task_id: "2.1 / 2.2 / 2.3 / 2.4"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: e4c980c029a97966c3229ac067db9a061817b04d
    evidence_type: CHECK
    evidence_id: PV2
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "node scripts/check-crate-boundaries.mjs 本轮未执行（本角色不跑 npm；且门禁按裁定在集成树跑）。因本轮 fix 未新增任何依赖（含 dev），该门禁不改变本轮代码结论；待检查执行者在任务 3.1/6.3 补跑"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.1 / 2.2 / 2.3 / 2.4"
    role: reviewer
    phase: review
    stage: work-package
    target_revision: e4c980c029a97966c3229ac067db9a061817b04d
    evidence_type: CHECK
    evidence_id: "PV5 / npm run check（合同门禁）"
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "reports/pv5-windows-nodelink.log 与合并树上的 npm run check 尚未产生；本轮 fix 零改动合同资产/文档/脚本/依赖，故不影响本轮判断，但按规则不得据此记 PASS"
    source_evidence: NOT_APPLICABLE
```

### 返回给主 Agent 的摘要

- **结论：PASS**（Target `e4c980c029a97966c3229ac067db9a061817b04d`；无 CRITICAL/MAJOR、无新回归）。
- **复核状态**：**F4 已解决**（替身与真存储同形，既有 server 断言全部空清单、无空转断言）；**F5 已解决**（错别字确在 `server/src/node_link/catalog.rs:254`，core 侧本就正确，纯注释）；**F6 已解决**（Device 分支在 `approve_device` 之前逐字拒绝、事务内零写入零审计；用例判别力可由「移除 guard 即必然成功提交」独立证明；core 不加对称校验的理由成立，无静默丢弃清单的生产路径）；**R9 缺口已关闭**（真 router + 真 `SqliteStore`，两条失败分支错误码+消息+零写入均具判别性，并有正对照；放 `crates/app/tests/**` 的理由与 §5 矩阵/门禁脚本一致——该脚本**连 dev-dependency 一起判**）。
- **新发现**：`RV2-IMPL-F1`（**MINOR**）F3 裁定理由里的「不可达 / 改 = 撤销重配」与代码不符——`approve_node` 对 `paired` 行无状态守卫、`claim_pairing` 无「已配对」守卫、`node.pair.confirm` 不调 `close_node`，因此**不撤销也能重新配对改写清单**且既有连接/attachment 存活；裁定**结论**（已知限制 + 后续切片要求）不受影响、缺口与既有条件②同源，故不阻断，建议更正 `verification.md`/`plan.md`/`docs/NODE_LINK_PROTOCOL.md` §8.2/`design.md` D8 措辞并把后续要求的触发条件写准。`RV2-IMPL-F2`/`F3`（SUGGESTION，均为既有代码的非阻断项）。
- **待核对**：[PV1] 合同门禁（`npm run check`）、[PV2]、[PV5] 仍 PENDING（集成阶段）；[PV1] fix 轮次 Rust 侧已由主 Agent 亲跑并被我独立核算（86 行 / 992 passed / 0 failed / 2 ignored / exit=0）。
- **报告路径**：`openspec/changes/node-trust-export-ids/reports/rv2-impl.md`。
- **我未做**：修改任何代码/测试/规划/任务状态/`verification.md`，跑 cargo/npm，执行 E2E，切分支或提交。
