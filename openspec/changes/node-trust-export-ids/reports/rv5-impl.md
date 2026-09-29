<!-- RV5-IMPL：DU1 集成候选（stage=candidate）的 merge 类型独立复核报告。字段与 handoff_index 组织见 openspec/schemas/agentic/roles/handoff.md。 -->

# RV5-IMPL 独立检视报告（merge / candidate：合并完整性 + 未被覆盖的 delta + 历史 findings 闭环）

| 字段 | 值 |
| --- | --- |
| `task_id` | `6.4`（候选独立 review；并覆盖 2.3/2.4/2.5 的候选面复核） |
| `role` | reviewer（独立检视子 Agent，只读；本轮为候选/合并后复核） |
| `phase` | review |
| `stage` | candidate |
| `agent_context` | 新建 reviewer 子 Agent（本报告作者），**不继承**任何实现、修复、集成或前四轮 review 对话；未参与本变更任何实现/集成/裁决。隔离方式：全新上下文 + 只读读取（候选 worktree、主检出、变更目录、前几轮报告与日志）。工作目录 `D:/Project/acp-remote-wt/export-ids-integration`（候选）与 `D:/Project/acp-remote`（权威树/变更目录）。按调度要求**未执行** `cargo`/`npm`、未执行 E2E、未切分支、未提交、未改动任何被检视文件。 |
| `target_revision` | `64e179c618e690efcc39f64854d59861e9ecbacf`（= `git rev-parse HEAD`，已核实；`git status --porcelain` 为空） |
| `base_revision` | `3cadb12d79456750e40644a2ea53c96380b700e6`（= `refs/heads/main` = `origin/main`，构建前后均未移动，已核实） |
| `scope` | **merge 类型**：① 合并完整性（祖先关系、`--no-ff` 两次合并的冲突解决、候选树是否为两源并集）；② **尚未被任何独立 review 覆盖的 delta**（`cc6faefd` 注释级、`659e5900` 文档引用勘误）在候选树上的复核；③ RV1–RV3-IMPL 已覆盖的两处代码 delta 在候选树上是否仍成立；④ 历史 findings（RV1–RV4-DOCS、RV1–RV3-IMPL、主 Agent 复审-1、跨 WP 待办、ISSUE-1/2/3、风险裁定 1–5）逐条闭环；⑤ RV3-IMPL 自列的未覆盖面在候选阶段的定性；⑥ 门禁证据一致性（读集成 Agent 与主 Agent 的日志，不复跑） |
| `changes` | 未修改任何文件（除本报告）；未改 `verification.md`/`tasks.md`/`plan.md`/`design.md`/`specs/**`；未暂存任何内容 |
| `checks` | 见 §6：候选树门禁**按日志逐项核对**（`npm run check` 10/10、`check:drift` 首绿、workspace `cargo test` 994/0/2、[PV2]、[PV5] 含 `a_narrowing_repair_closes_the_live_attachment`）＋ **本角色自行执行**的只读证据（`git merge-base`/`git diff`/`git grep`/`git show`；见 §2、§5.3 的静态引用核对）。**未复跑** `cargo`/`npm` |
| `issues` | 0 CRITICAL / 0 MAJOR。合并完整性**无缺陷**；两处未被覆盖的 delta **均成立**；历史 findings **全部闭环**（无「未解决」项遗留到候选）。4 条非阻断项：RV5-IMPL-F1（MINOR，后续义务只落在 `verification.md`）、F2（MINOR，收窄后残余扇出窗口未显式写进权威文档/无断言）、F3（SUGGESTION，tasks.md 6.4 的报告路径与本轮实际路径不一致）、F4（SUGGESTION/观察，`git diff --check` 的 trailing whitespace 属既有体例） |
| `result` | **PASS**（候选阶段；无 CRITICAL/MAJOR，按收口约定非阻断项不阻塞） |
| `evidence_paths` | 本报告 `openspec/changes/node-trust-export-ids/reports/rv5-impl.md`；被读证据 `reports/du1-integration.md`、`reports/du1-integration.log`、`reports/du1-pv1-merged.log`、`reports/pv5-windows-nodelink.log`、`reports/{rv1-impl,rv2-impl,rv3-impl}.md`、`reports/{rv1-docs,rv2-docs,rv3-docs,rv4-docs}.md`、`verification.md`、候选 worktree 的源码/测试/文档 |
| `resource_cleanup` | 未创建目录联接、数据库、端口或长驻子进程；唯一临时产物为 `/tmp/rv5/docrefs.py`（本角色自写的只读静态近似脚本，用于核对章节引用归属；**位于仓库外**，不进入版本控制、不影响任何门禁）。两个 worktree 的 `git status --porcelain` 复核为空；主检出的未跟踪变更目录未被我改动（只新增本报告） |

## 1. Review Context

- **Review ID / Type / Stage**：RV5-IMPL（本轮唯一）/ `merge`（候选）/ `candidate`。
- **被检版本**：候选 `64e179c6`（`git rev-parse HEAD` 核实，工作区干净）；两次 `--no-ff` 合并链 `64e179c6` ←（并入 `659e5900`）← `1c632ad8` ←（并入 `cc6faefd`）← base `3cadb12d`。源提交：代码 `cc6faefd`（分支 `agentic/node-trust-export-ids` 的 tip）、文档 `659e5900`（分支 `agentic/node-trust-export-ids-docs` 的 tip）。三个分支 tip 与 base 在本轮复核时都是候选的祖先，且**候选是本分支 tip**（未在候选之后新增提交）。
- **读取的规则与需求**：`openspec/schemas/agentic/roles/reviewer.md`、`roles/handoff.md`；`AGENTS.md` §3/§4/§6/§7/§9/§13；`docs/MODULE_ARCHITECTURE.md` 的依赖矩阵意图；变更目录的 `proposal.md`、`design.md`（含 D8/D9 的 2026-09-27 勘误）、`specs/{local-admin-methods,node-link-owner-server,storage-schema-v2-migration}/spec.md`、`plan.md`（含 R19 覆盖行）、`tasks.md`、`verification.md`（Target / Handoff Index / Checks / Check Plan Changes / Review Findings / Dependency Handoffs / Failures and Retests / Final Assessment）；权威文档 `docs/NODE_LINK_PROTOCOL.md` 的 §8.2/§12.x/§14.2/§15、`docs/CORE_PORTS_AND_STORAGE.md`、`docs/LOCAL_ADMIN_PROTOCOL.md`。
- **复核方法**：先固定版本与目录（`git rev-parse`/`git status`/`git worktree list`），再**自读**两次合并的完整内容与两侧源分支的完整 diff（不依赖任何摘要），然后按「合并完整性 → 未被覆盖 delta → 历史 findings 逐条 → 未覆盖面定性 → 门禁证据一致性」逐项给结论与依据；门禁部分只读日志原始输出并与结论对照。文档/代码交叉引用（尤其中文节号引用）按候选树逐处核对，不采信任何角色自述。
- **限制**（必须随结论一起看）：
  1. 本角色**不执行 `cargo`/`npm`**，因此不产生新的门禁证据；候选门禁结论来自 `du1-integration.log`、`du1-pv1-merged.log`、`pv5-windows-nodelink.log` 的**原始输出核对**（不是复跑）。
  2. 未执行 E2E（本变更 E2E 为 `not-applicable`，替代验证 = [PV5] 受控路径，已在候选执行）。
  3. 变更目录 `openspec/changes/node-trust-export-ids/**` 属主检出的**未跟踪**内容，不在候选树内；因此「权威树全量 `check:docs`」这一判定**不可能**在候选树上取得（见 §6.3，已按 Check Plan Changes 的取位规则处理）。
  4. §3 的静态引用核对用了**本角色自写的只读近似脚本**（`/tmp/rv5/docrefs.py`，镜像 `scripts/check-doc-links.mjs` 的章节引用归属逻辑）作为定位手段；它是**复核线索**，不是门禁证据，正式判定仍须由主 Agent 在权威树实跑（命令见 §6.3）。
  5. 本报告**不代替** Project Verify，也不代表任务可勾选、不代表变更可归档。

## 2. 必查项 1：合并完整性

| # | 核查项 | 命令 / 方法 | 结果 |
| --- | --- | --- | --- |
| M1 | 候选是本分支 tip、工作区干净 | `git rev-parse HEAD`；`git status --porcelain` | HEAD = `64e179c618e690efcc39f64854d59861e9ecbacf`；输出为空 ✅ |
| M2 | base 与两个源提交都是候选的**祖先** | `git merge-base --is-ancestor <rev> HEAD`（四条） | `3cadb12d` ✅、`cc6faefd` ✅、`659e5900` ✅、`64e179c6` ✅（全部 `YES`） |
| M3 | 候选相对**代码源 tip**只在文档侧有差异（不夹带、不丢失代码） | `git diff --stat cc6faefd..HEAD -- crates` | **空** ✅（代码面与 `cc6faefd` 逐字节一致） |
| M4 | 候选相对**文档源 tip**只在代码侧有差异 | `git diff --stat 659e5900..HEAD -- docs openspec` | **空** ✅（文档/主规范面与 `659e5900` 逐字节一致） |
| M5 | 候选相对代码源 tip 的差异 = 文档源自身的 diff | `git diff --stat cc6faefd..HEAD -- docs openspec` vs `git diff --stat 3cadb12d..659e5900 -- docs openspec` | 两侧同为「4 files changed, 48 insertions(+), 24 deletions(-)」，文件集与行数**逐项相同** ✅ |
| M6 | 候选相对文档源 tip 的差异 = 代码源自身的 diff | `git diff --stat 659e5900..HEAD` vs `git diff --stat 3cadb12d..cc6faefd` | 两侧同为「28 files changed, 2325 insertions(+), 184 deletions(-)」，文件集与行数**逐项相同** ✅ |
| M7 | 两次合并**无冲突解决**（内容取舍） | `git show --stat --format="%H%n%P%n%s" 1c632ad8`、`git show --stat --format="%H%n%P%n%s" 64e179c6` | 每个合并提交 `vs 第一父` 的 diff = 另一侧源分支自身的 diff（M5/M6）；无手工差异、无冲突标记 ✅ |
| M8 | 无冲突残留标记 | `git grep -n -E "^(<<<<<<<|>>>>>>>|=======)$"` | 无匹配 ✅ |
| M9 | 文件集不相交（两条源分支写入面不重叠） | `git diff --name-only 3cadb12d..cc6faefd`（全在 `crates/**`）与 `git diff --name-only 3cadb12d..659e5900`（`docs/**` + `openspec/specs/storage-schema-v2-migration/spec.md`） | 零交集 ✅（因此「并行分支汇合」在内容层不可能发生冲突，与集成报告「无冲突」一致） |
| M10 | 集成分支没有额外提交（不夹带） | `git log --oneline --graph -12` | 集成分支上的提交 = 2 个 merge + 两个源 tip（及其历史）；无第三方提交 ✅ |
| M11 | **最新主分支新增交互**（merge 类型必查） | `git rev-parse refs/heads/main origin/main`；`git merge-base --is-ancestor refs/heads/main HEAD` | `main` = `origin/main` = `3cadb12d` = base，**自候选构建以来未移动**；因此本轮不存在「base 之后新增的主分支提交与候选的交互或冲突」需要复核 ✅ |
| M12 | 候选树 `docs/**` 与 `crates/**` 都是两源的并集 | M3–M6 的逐侧等价 + M9 的不相交 | 是并集，且两侧都**完整**（无静默丢失、无第三份内容） ✅ |

**合并完整性结论：无缺陷。** 两次合并都是 `ort` 策略下的干净快进式内容合并（每个合并提交相对第一父的差异恰为另一侧自身的差异），候选 = 两条源分支的**精确并集**，没有冲突解决记录需要复核（与集成 Agent 报告的「无冲突」一致），且主分支未在 base 之后移动，故无「最新主分支新增交互」。

## 3. 必查项 2：尚未被独立 review 覆盖的 delta

### 3.1 `cc6faefd`（`chore(server)`，据称纯注释）— ✅ 确认为纯注释，且注释与实现一致

| 核查 | 方法 | 结果 |
| --- | --- | --- |
| **真的只有注释**（非注释行为改动 = 0 行） | `git show cc6faefd --format=""` 提取 `+`/`-` 行（去掉 `+++`/`---`）后过滤掉以 `///`/`//`/`/*`/`*` 开头的行并计数 | **0 行** ✅（逐行过滤后的输出为空；`--format=""` 保证不含提交信息）|
| diff 范围 | `git show cc6faefd` | 2 文件、8 增 3 删：`crates/server/src/local_admin/pairing.rs`（1 增 1 删，纯 `///` 行）与 `crates/server/src/local_admin/router.rs`（7 增 2 删，纯 `///` 注释块） ✅ |
| `§12.6 → §12.3` 指向正确 | 候选树 `docs/NODE_LINK_PROTOCOL.md`：`### 12.3 Catalog 消息` 在第 513 行，其表内第 520/521 行登记 `export.revoked` 与 `node.trust.revoked`；`### 12.6 控制与错误消息` 在第 597 行，表内只有 `link.ping`/`link.pong`、`link.error`、`link.backpressure` 三行 | `§12.3` 正确 ✅（改动后 `pairing.rs:64` 写「`NODE_LINK_PROTOCOL.md` §12.3 的 `export.revoked`」，与该消息的真实登记位置一致） |
| 新注释与**代码实际行为**一致 | 注释断言：撤销推 `node.trust.revoked` 并以 `4410` 关闭并停止重连；重新配对**不**推消息、以 `1000` + close reason 关闭；引用 `§8.2`/`§14.2`/`§15`。对照实现：`crates/app/src/compose.rs:782-790`（`close_node` → `CommandRoute::node_revoked`）与 `:793-802`（`close_node_after_reauth` → `CommandRoute::node_reauth`）；`crates/server/src/node_link/command.rs:1568-1615`（先 `send(NodeTrustRevoked)` 再 `request_close(close::REVOKED, "the node trust was revoked")`）与 `:1634-1652`（仅 `request_close(close::NORMAL, "the node trust was re-confirmed; reconnect to fetch the updated catalog")`，无 `send`）；常量 `crates/server/src/node_link/conn/mod.rs:64`（`NORMAL = 1000`）、`:76`（`REVOKED = 4410`） | 逐条相符 ✅（注释未夸大、未漏）
| 注释内部不再自相矛盾（RV3-IMPL-F1 的原问题） | `crates/server/src/local_admin/router.rs:816`（首段）已无「与 `node.revoke` 同一机制」；`:819` 明写「两条路径的**语义与关闭码不同**」；`:887-888` 写「共用『授权变化 → 关连接』的**时机**，但**不复用**它的撤销语义」 | 已消除 ✅ |
| 候选树里其余 `12.6` 引用是否都合法 | `git grep -n "12\.6"` 于候选树（`docs/` + `crates/`） | 全部合法 ✅：`docs/NODE_LINK_PROTOCOL.md:597`（小节标题）、`crates/node-link-protocol/src/error.rs:2/278`（`link.error`/`link.backpressure`，确属控制与错误消息）、`crates/node-link-protocol/src/pairing.rs:22`（`link.error` body 形状）、`crates/server/src/node_link/conn/session.rs:980/1014/1334`（`link.ping/pong` 的 nonce）。**没有任何一处**把 `node.trust.revoked`/`export.revoked` 归到 `§12.6` |

> 说明：注释级改动**不改变行为**、无随附测试风险；其正确性完全落在「与实现/文档一致」上，上表已逐项核对。

### 3.2 `659e5900`（`docs(node-link)`，`§15` 的 `（§12.6）→（§12.3）`）— ✅ 单一 hunk、无顺带改动

| 核查 | 方法 | 结果 |
| --- | --- | --- |
| **单一 hunk / 最小改动** | `git show 659e5900` | 1 文件、1 增 1 删、单一 hunk `@@ -927,7 +927,7 @@`（`docs/NODE_LINK_PROTOCOL.md` 第 930 行整行替换） ✅ |
| 全文件已无错误形式的 `§12.6` | 候选树 `git grep -n "12\.6" -- docs/NODE_LINK_PROTOCOL.md` | 仅剩第 597 行的小节标题（合法） ✅ |
| **未**顺带改动其它引用 | diff 逐字读：该行除 `（§12.6）`→`（§12.3）` 外**逐字未动**（含同行的 `§8.2`、`§15` 的语义限定句）；同文件其它 `§12.4`/`§12.3` 引用（第 627/931 行）与 `§14.2` 表注（第 906 行）| 未改动 ✅ |
| `§12.3`/`§12.4` 本就正确（未被「顺手改错」） | `docs/NODE_LINK_PROTOCOL.md:627` 的 `§12.3`（`session.create` 可见 Export 判定，登记在 §12.3/§12.4 的 Catalog/Resource 语义）、`:931` 的 `§12.3`（catalog 是连接期内存数据，§12.3 即 Catalog 消息）、`:906` 的 `§14.2` 表注（1000 vs 4410） | 均正确 ✅ |
| 勘误是否**只**修真实指错处 | 代码侧同类（`crates/server/src/local_admin/pairing.rs:64`）由 `cc6faefd` 同批修；候选树全仓已无「`node.trust.revoked`/`export.revoked` → §12.6」的错误引用 | ✅（RV3-IMPL-F2 的逐处判断在候选树上兑现） |

### 3.3 另两处代码 delta 在**候选树上**是否仍成立（RV2/RV3-IMPL 已审，本轮按候选树再确认）

| delta | 候选树证据 | 结论 |
| --- | --- | --- |
| `close_node_after_reauth` **分流**真实存在 | 端口 `crates/server/src/local_admin/pairing.rs:62-66`（trait 声明，文档明写「不得推 `node.trust.revoked`、不得以 4410 关闭」）；组合根 `crates/app/src/compose.rs:793-802`（走 `node_reauth`，日志事件 `daemon.reauth_connection_sweep`）；调用点 `crates/server/src/local_admin/router.rs:~888`（`node_pair_confirm` **提交后**调 `self.deps.pairing.close_node_after_reauth(&node_id)`） | 成立 ✅ |
| `1000` + reason | `command.rs:1636-1642`：`handle.request_close(close::NORMAL, "the node trust was re-confirmed; reconnect to fetch the updated catalog")`；`conn/mod.rs:64` = `1000`；文档 `docs/NODE_LINK_PROTOCOL.md:280`（§8.2）与 `:906`（§14.2 表注）与 `:930`（§15）口径一致 | 成立 ✅ |
| `node_reauth` **不推消息** | `command.rs:1634-1652` 全函数无 `handle.send(...)`（仅 `request_close` + `forget_connection` + 结构化日志）；撤销路径 `:1568-1615` 仍先 `send` 再 `request_close`；候选 e2e 正向断言「关前 `pending` 队列里没有 `node.trust.revoked`」（`crates/app/tests/node_link_e2e.rs:848-856`） | 成立 ✅ |
| 两条路径在**用例层可区分** | `crates/app/tests/node_link_e2e.rs:835-843`（重新配对 → close code == `1000`）、`:848-856`（无 `node.trust.revoked`）、`:700-706`（撤销 → 收到 `node.trust.revoked` 且 close code == `4410`）；替身两表 `crates/server/src/local_admin/test_support.rs`（`closed_nodes()` / `closed_nodes_after_reauth()`） | 成立 ✅ |

## 4. 必查项 3：历史 findings 逐条闭环（按候选树核对）

判定口径：**已解决** = 在候选树（源码/测试/文档/规划文本）中验证到修复事实；**未解决** = 候选树仍存在原问题；**无法确认** = 本轮手段不足。

| 原 ID | 级别（原） | 本轮复核位置（候选 `64e179c6`） | 结论 | 依据 |
| --- | --- | --- | --- | --- |
| RV1-DOCS-F1 | MAJOR | `docs/CORE_PORTS_AND_STORAGE.md:793`（§6 第 5 条） | **已解决** | 该条已是三条件（未撤销 ∧ `export.scopes ∩ node.grants ≠ ∅` ∧ 在 `exportIds` 内），并写明「清单为空或目标 Export 不在清单内一律不放行」；与 `docs/NODE_LINK_PROTOCOL.md:274`（§8.2 三条件）同口径 |
| RV1-DOCS-F2 | MINOR | `docs/CORE_PORTS_AND_STORAGE.md:222` | **已解决** | 单点可见性策略处已写三条件并指明「权威定义见 `NODE_LINK_PROTOCOL.md` §8.2 与本文件 §3.5」，同时要求 core 的 Owner 侧判定同口径 |
| RV1-DOCS-F3 | MINOR | `docs/CORE_PORTS_AND_STORAGE.md:1454` | **已解决** | 「过新」用例取值已是 `user_version = 5`（= `FILE_FORMAT_VERSION + 1`），与 §7.2 的 4/4/3 及 `migrate.rs` 常量自洽 |
| RV1-DOCS-F4 | MINOR | `docs/CORE_PORTS_AND_STORAGE.md:1361` | **已解决** | §9 判据 32 已存在且为 1…32 连续（无重号缺号），子项 ①–⑤ 齐备（含 ③ 三条件与 ④ core 侧同口径、⑤ 撤销不级联） |
| RV1-DOCS-F5 | SUGGESTION | `docs/NODE_LINK_PROTOCOL.md:7`（2026-09-26 历史修订行） | **已解决** | 历史原文保留，行尾追加「（其中 `exportIds` 推后一条已被 2026-09-27 的修订取代，见上一条）」，与 RV1 的裁定「保留原文 + 补半句」一致 |
| RV1-DOCS-F6 | SUGGESTION | `plan.md` WP5 行（第 186 行）Write Scope | **已解决** | 已列入 `openspec/specs/storage-schema-v2-migration/spec.md`（仅 Purpose 勘误），与候选里该文件的实际改动一致 |
| RV2-DOCS-F1 | MINOR | `docs/CORE_PORTS_AND_STORAGE.md:22` | **已解决** | 文件头版本行 0.14 已在版本列表末尾、单行体例与 0.12/0.13 一致；1–21 行与 base 逐字相同 |
| RV2-DOCS-F2 | MINOR | `verification.md` 的 Check Plan Changes（主 Agent 自持文件） | **已解决** | 该处已写「追加为判据 32」并注明 RV1 原文写 31 的溯源差异，与文档实际一致 |
| RV2-DOCS-F3 | SUGGESTION | `docs/CORE_PORTS_AND_STORAGE.md:1361` 判据 32 ④ | **已解决** | ④ 已含 core 侧「清单为空/不在清单内 → 该 Export 的会话命令不可用」，与 §6 第 5 条呼应 |
| RV3-DOCS-F1 | MINOR | `docs/CORE_PORTS_AND_STORAGE.md:22` | **已解决** | 三处不实/漏记均已修：节号写 `§4`（本文件「单点可见性策略」在第 222 行的 §4 区间内，「§5.3」区间内 0 处）、改口径为「**本次未改任何端口签名：§5 的 rust 块与 `crates/core/src/ports.rs` 均未变；§7 的 SQL 块已同批更新**」、枚举补 `§9` 判据 1/28、`§10`、`§11.3` |
| RV3-DOCS-F2 | SUGGESTION | `docs/CORE_PORTS_AND_STORAGE.md:1361` 首行来源列表 | **已解决** | 已改为「本文件 §3.5、§7.2、§7.3、§11.6 第 4 条、§6 第 5 条，`LOCAL_ADMIN_PROTOCOL.md` §5.4，`NODE_LINK_PROTOCOL.md` §8.2」（裸 `§5.4` 已消除） |
| RV4-DOCS-F1 | MINOR | `docs/CORE_PORTS_AND_STORAGE.md:22` | **已解决**（本轮补上其独立复核，原 Resolution 写「待集成阶段 review 覆盖」） | 机械核实：该行全角括号 **3 开 / 3 闭**（已平衡），且 `39be2f77..f47f9da` 的修订只删行尾单个 `）` |
| RV4-DOCS-X1（= OOS-1 / ISSUE-1） | MAJOR 类 | `verification.md`（主检出，未跟踪变更目录） | **已解决** | 主 Agent 已把「本文件 §3.5」改为指名 `CORE_PORTS_AND_STORAGE.md` §3.5。**本轮独立静态核对**：用只读近似脚本按 `check-doc-links.mjs` 的归属规则扫当前主检出（355 个 md），**0 problem**；该脚本对**修复前**的同款句式能复现 1 条 `§3.5 在 docs/LOCAL_ADMIN_PROTOCOL.md 中不存在`（合成样例验证），对修复后的句式 0 条 ⇒ 原问题确已消失（正式判定见 §6.3） |
| RV1-IMPL-F1 / F2 | MINOR | `plan.md` 的 `alternative_checks` 第 1 条；`design.md` D9 勘误段 | **已解决** | 两处已不含「`import.add` 返回 `local.not_found`」；`design.md:87-90` 明写本切片 `import.add` 恒回 `local.unavailable`，且勘误段写明 `attach` 的两步判定（未知 id → `nodelink.export.not_found`；可见性复核失败 → `nodelink.export.not_granted`），与 `crates/server/src/node_link/resource.rs` 的实现一致 |
| RV1-IMPL-F3 | MINOR | `close_node_after_reauth` 全链路 + 文档 §8.2 | **已解决**（其原「不可达」裁定已被 RV2-IMPL-F1 推翻并改为修复） | 候选树三道证据：提交后关连接（`router.rs` 的 `node_pair_confirm` 尾段）、专用关闭路径（`command.rs:1634-1652`）、文档口径（`NODE_LINK_PROTOCOL.md:280`「下一次握手后的新连接生效」）；`crates/app/tests/node_link_e2e.rs:723-880` 的 e2e 用例覆盖该路径 |
| RV1-IMPL-F4 | SUGGESTION | `crates/server/src/local_admin/test_support.rs:1412-1430` | **已解决** | 假存储 `revoke_node` 保留清单（`NodeRecord::try_new(...)` 原样带过 `export_ids`，并有「撤销只改三列、清单逐字保留」的注释），与真存储同形 |
| RV1-IMPL-F5 | SUGGESTION | `crates/core/src/model/identity.rs:309` 附近 | **已解决** | 原错别字「只能收窄不掉宽」在候选树中已不存在（`git grep` 无匹配），现为「不能放宽 `scopes ∩ grants`」 |
| RV1-IMPL-F6 | SUGGESTION | `crates/storage-sqlite/src/admin/trust.rs:865-879` | **已解决** | `PairingTarget::Device` 分支对**非空** `granted_export_ids` 显式 `InvalidRequest("a device pairing must not carry export ids")`，与同分支的 grants 拒绝对称，并附「授权相关字段不得被静默丢弃」的注释 |
| RV1-IMPL-R9 | MINOR（覆盖缺口） | `crates/app/tests/node_pair_export_ids.rs:198/248` | **已解决** | 两个用例走**真实 router + 真实 SQLite**（模块头注释解释了为何只能在 `app` 做组合），断言「错误分类 + 零写入（`node.list` 为空、审计 0）+ 配对未推进（随后合法清单仍能确认）+ 成功路径确有 `node.paired` 审计」 |
| RV1-IMPL-R14 | 部分覆盖（裁定接受） | `crates/storage-sqlite/tests/migration.rs` | **已解决（按裁定：本次不扩测）** | v3 → v4 与新建库两条主路径有列清单断言；v2 路径无 `column_specs` 断言属既有测试资产范围，主 Agent 已书面裁定接受——本轮不重开 |
| RV2-IMPL-F1 | MINOR（推翻原裁定） | 同 RV1-IMPL-F3 位置 | **已解决** | 见上；这是本变更最重要的一处行为修复，候选树与 e2e 一致 |
| RV2-IMPL-F2 | SUGGESTION（既有） | `crates/server/src/node_link/catalog.rs` 测试断言 | **已解决（按裁定：不在本次改）** | 书面裁定接受（既有测试资产、不影响行为正确性）；候选树未改，与裁定一致 |
| RV2-IMPL-F3 | SUGGESTION（既有） | `crates/server/src/local_admin/router.rs` 测试替身 | **已解决（按裁定：接受）** | 关键路径已由 R9 的真存储组合断言覆盖；裁定与候选树一致 |
| RV3-IMPL-F1 | MINOR | `crates/server/src/local_admin/router.rs:816` | **已解决** | 首段改为「信任行提交成功后关闭该节点的现有活动连接，强制重新握手后按新清单重算」，不再写「与 `node.revoke` 同一机制」；`:819` 与 `:887` 的措辞自洽 |
| RV3-IMPL-F2 | MINOR（既有错误引用） | `docs/NODE_LINK_PROTOCOL.md:930` + `crates/server/src/local_admin/pairing.rs:64` | **已解决** | 两处均已为 `§12.3`（本轮按候选树逐处核对，见 §3.1/§3.2） |
| RV3-IMPL-F3 | MINOR（规划面） | `specs/local-admin-methods/spec.md` + `design.md` D8 + `plan.md` | **已解决** | delta spec 的 REQUIREMENT 已含「提交后 MUST 关闭活动连接、关闭码 MUST 为 `1000`、MUST NOT 推送 `node.trust.revoked`、两条路径 MUST 可区分」，并新增场景 `#### Scenario: 重新配对收窄清单后作废既有连接`；`design.md:78` 的 D8 已改为「同一**时机**、不同**语义与关闭码**」；`plan.md` 已加 **R19**（tasks 2.3/2.4/3.1/7.1，checks [PV4]/[PV5]） |
| RV3-IMPL-F4 | SUGGESTION | `crates/server/src/node_link/command/tests.rs:2268-2271` | **未解决（已登记为 ISSUE-3 技术债，与裁定一致）** | 恒真断言**仍在**（未被悄悄删掉，也没被伪装成判别性断言），且与 `:2239`、`:2407` 的既有同款写法一致、`:2074/:2094` 才是可失败的写法；主 Agent 已裁定「接受并登记」。属**低风险**、不阻断候选 |
| RV3-IMPL-F5 | SUGGESTION | `crates/app/tests/support/nodelink.rs` 的 `close_code()` | **未解决（按裁定接受）** | support 客户端仍丢弃 close reason（`Frame::Close(code, _)`），e2e 只断言 code；reason 的一致性由「代码/用例/文档逐字比对」+ 单测的 `close_request()` 断言承载——裁定与候选树一致，非阻断 |
| RV3-IMPL-F6 | SUGGESTION | `crates/server/src/local_admin/router.rs` 的 `device_pair_confirm`；`pairing.rs` 的 trait | **未解决（登记为后续切片要求；与裁定一致）** | 设备面仍无 `close_device_after_reauth`；本切片无设备连接（`server::sync` 未落地），与本次节点语义不冲突。**见 RV5-IMPL-F1**（登记位置的持久性） |
| RV3-IMPL-F7 | SUGGESTION | `crates/server/src/node_link/conn/registry.rs:358` | **未解决（按裁定：不在本次合并）** | 该方法在候选树中仍**无调用者**（全仓 `.close_node(` 的两处分别是 trait 调用与 `PairingSessions` 转发），两条新路径各自手写循环——与「base 既有死代码、不在本次合并」的裁定一致 |
| 主 Agent 复审-1（关闭码语义） | MAJOR（已修） | 见 §3.3 | **已解决（闭环）** | 两条路径**真正可区分**：撤销 → `send(node.trust.revoked)` + `close(4410)`（`command.rs:1568-1615`，e2e 断言见 `node_link_e2e.rs:700-706`）；重新配对 → **不推**消息 + `close(1000)` + 准确 reason（`command.rs:1634-1652`，e2e 断言见 `:835-856`）。日志事件、替身两表、单测与 e2e 四层都可区分，无「合成一个计数导致假绿」 |
| 跨 WP 待办-1（`migrate.rs` 的 v3 守卫） | — | `crates/storage-sqlite/src/migrate.rs:987/991` | **已解决** | `if file_version < 3 {...}`（v3 段）与 `if file_version < 4 && owned_node_pre_existing {...}`（v4 段）都在；`owned_node_pre_existing` 在 `:947` 于任何重建之前取样，并有解释「不守卫会重复执行 12-step 重建」的注释 |
| 跨 WP 待办-2（`params.rs` 的旧形状注释） | — | `crates/server/src/local_admin/params.rs:744-751` | **已解决** | 注释已写 `{ pairingId, grants, exportIds }` 与「`exportIds` 是**必需**字段：缺失或不是字符串数组一律 `local.invalid_params`」，与 §5.4 及 `reject_unknown_fields(params, &["pairingId","grants","exportIds"])` 的实现一致 |
| RISK-1（attach 错误码裁定） | 裁定 | `crates/server/src/node_link/resource.rs` + `design.md` D9 | **已解决** | 裁定与实现一致（两步判定，词表未改），`design.md` 的勘误段已就地记录 |
| RISK-2（`PairingSettlement` 形状） | 裁定 | `crates/core/src/model/identity.rs`、`crates/server/src/local_admin/router.rs` 的 settle 后附加 | **已解决** | 唯一生产结算路径在 settle 后 `with_granted_export_ids(...)`；附加处有注释；设备方向带非空清单被显式拒绝（RV1-IMPL-F6）⇒ 无「忘附加即静默放权」的旁路 |
| RISK-3（列序与迁移守卫） | 裁定 | `crates/storage-sqlite/src/migrate.rs` + `CORE_PORTS_AND_STORAGE.md:1054` | **已解决** | 常量 4/4/3、v3/v4 守卫、新列在 `revoke_reason` 之后；文档 §7.3 的 DDL 与代码同批更新，且候选树 `check:drift` 首绿（§6.1） |
| RISK-4（CLI 空清单警告走 stdout） | 裁定 | `crates/app/src/cli/pairing.rs` | **已解决（与裁定一致）** | 警告走 stdout、失败时 stderr 只一行 JSON；`crates/app` 的 CLI 用例覆盖（`an_empty_export_id_list_warns_without_blocking` 在 [PV5] 日志中 `ok`） |
| RISK-5（`node list` 展示） | 裁定 | `crates/server/src/local_admin/view.rs` + `crates/app/src/cli.rs` | **已解决** | 清单由 daemon 侧投影带出并原样打印；`node_pair_export_ids.rs` 断言 `node.list` 回显与 confirm 一致 |
| 方法论纠正-1（门禁取位） | — | 见 §6.3 | **已解决（规则已登记；权威树判定仍待实跑）** | 取位规则已写进 Check Plan Changes；本轮按该规则处理，并把「权威树 `check:docs`」记为待办（§6.3） |
| ISSUE-1（主检出 `check:docs` 变红） | MAJOR 类 | 同 RV4-DOCS-X1 | **已解决** | 见上；本轮静态复核 0 problem（正式实跑见 §6.3） |
| ISSUE-2（`daemon_lifecycle` 既有 flaky） | 未解决（非本变更引入） | `crates/app/tests/daemon_lifecycle.rs` | **未解决，但已闭环处置（不影响候选结论）** | 该文件与 base **零差异**（`git diff --stat 3cadb12d..HEAD -- <file>` 为空）、无 `#[ignore]`、断言未被弱化；候选两轮（workspace 测试 + `-p app`）**均首跑通过**（见 §6.2）。登记为「建议单独变更修」的风险 |
| ISSUE-3（恒真断言技术债） | 未解决（低风险，已登记） | 同 RV3-IMPL-F4 | **未解决，已登记** | 与裁定一致 |

**闭环结论**：所有**已确认修复**的 findings（含唯一的两条 MAJOR 类：RV1-DOCS-F1、主 Agent 复审-1 与越界的 RV4-DOCS-X1）在候选树上**均已解决**。未解决项只有 4 条**经主 Agent 明文裁定接受**的既有/后续项（RV3-IMPL-F4/F5/F6/F7、RV2-IMPL-F2/F3、RV1-IMPL-R14）与 2 条已登记的残余风险（ISSUE-2 flaky、ISSUE-3 断言债）——它们都不是本变更引入的缺陷，也不构成候选阻断。

## 5. 必查项 4：RV3-IMPL 自列的「仍未被覆盖的检查面」在候选阶段的定性

| # | RV3-IMPL 列出的未覆盖面 | 本轮判断 | 依据 / 建议 |
| --- | --- | --- | --- |
| ① | **收窄后的实时扇出窗口**：旧连接在关闭生效前仍可能收到一条已收窄 Export 的 `resource.event`（未被用例或文档量化） | **不阻断本次候选**；属「已文档化的生效边界 + 后续验证点」 | 机制：`request_close` 是**请求**，会话在下一次循环顶部才 `break`，且要先排空已入队消息（`CLOSE_DRAIN_BUDGET = 2s`）。本次修复把暴露面从「直到 Access 自愿重连为止（无界）」收窄为「≤ 一个会话 tick + 排空预算」。权威文档已把生效边界写成「下一次握手后的新连接」（`docs/NODE_LINK_PROTOCOL.md:280`），因此**不构成契约与实现的矛盾**；但该残余窗口在权威文档里只是隐含、没有一句显式说明，也没有断言。→ 登记为本轮 **RV5-IMPL-F2**（MINOR，建议在 §8.2 补一句量化说明并把「确认返回后旧连接不再收到新事件」列为切片 7 的验证点）。**不需要**在本次修代码 |
| ② | **Access 侧真实自动重连**（`node-link-client` 未落地）只能算契约级 | **不阻断本次候选**（已文档化的后续义务） | §15 的重连/退避规则已写清「1000 与重新配对 → 仍按退避自动重连」，而实现侧不存在可执行的 Access 客户端，因此本变更**无法**产生该证据；这是本切片范围的**边界**而非缺陷。`node-link-client` 落地时必须列为必测项（候选交付说明与验证记录里已登记） |
| ③ | **三条负向对照未由独立角色复跑**（均为实现者自报日志） | **不阻断本次候选**；记为残余风险 | 实现者日志含命令、原始失败输出、退出码、失败行号与还原后复跑结果（`reports/wp-fix-reauth.log` 三节），且本审查已按代码独立复述其**判别力**（`close_code()` 会把 close 前的文本帧收进 `pending`；撤销路径必然先 `send` 后 close）。独立复跑不是本工作流对该类对照的强制要求；候选 [PV5] 已重跑正向断言。→ 保持为「已记录、未独立复现」的残余风险 |
| ④ | **设备面缺口**：`device.pair.confirm` 改 scopes 后不作废连接、trait 无 `close_device_after_reauth` | **不阻断本次候选**；属后续切片要求 | 本切片无设备连接（`server::sync` 未落地，`close_device` 是记录型 no-op），无可观测漏洞；现在新增空实现才是推测性 scaffolding。→ 见 **RV5-IMPL-F1**（该项目前只登记在 `verification.md` 的 Review Findings/未覆盖面两处） |
| ⑤（§4 第 5 条） | **多连接并发**：同一节点两条连接时计数与清理顺序无用例 | **不阻断本次候选** | 两条路径都用 `handles_for_node` 一次取全量并逐个 `request_close`，逻辑等价于「全部作废」；缺的是**测试强度**而非行为正确性，登记为后续补测点 |
| ⑥ | close **reason 的线上字节**无断言、support 客户端丢弃 reason | **不阻断**（= RV3-IMPL-F5，裁定接受） | 单测直接断言注册的 `(code, reason)` 对，且 `session.rs` 把该对原样交给 close 帧；文档写「形如」，不要求逐字线上断言 |
| ⑦ | `close_code()` 判别力/可复用性 | **不阻断** | 属测试资产改进（若要断言 reason，需先改 support 客户端），已登记 |

**小结**：这些面**都不是候选阻断项**——其中 ②④ 属「后续切片必须做的义务」、①③⑤⑥⑦ 属「测试强度/证据完整性」的可登记项。**必须在本次修的一条也没有**；唯一建议的「本次内可做的小补」是 RV5-IMPL-F2 的那一句文档说明（非阻断，可由主 Agent 决定是否随归档前收口）。

## 6. 必查项 5/6：门禁证据一致性与 ISSUE-2

### 6.1 候选门禁证据逐项核对（只读日志，未复跑）

| 检查 | 日志原始输出（逐字摘录） | 与结论一致？ |
| --- | --- | --- |
| `npm run check`（10 道子门禁） | `reports/du1-integration.log`：`schema fixtures OK: 118 valid, 25 invalid …`、`command catalog OK: 12 commands`、`error registry OK: 58 codes`、`feature registry OK: 11 feature ids`、`contract assets OK: 17 schemas, 156 fixture files, 12 transcript vectors … 2 SAS values recomputed`、`ACP compatibility matrix OK: 25 methods …`、`doc links OK: 380 relative links, 5131 section refs across 324 markdown files`、`crate boundaries OK: 12 个 crate`、`contract drift OK: …`、`agentic gate: Installation: PASS + 19 specs passed, 0 failed + agentic 宿主入口检查完成：17 个文件`，末尾 `EXIT_CODE=0` | ✅ 一致（数了 10 条 `OK`/PASS 行 + EXIT_CODE=0；「10/10」成立） |
| `check:drift` **首绿** | 同上：`contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 93 个方法签名与 crates\core\src\ports.rs 一致` | ✅ 一致（在本合并树上文档侧 §7.3 与代码侧 `migrate.rs` 首次同时存在；无差异行） |
| `[PV1]` Rust 侧 | `reports/du1-pv1-merged.log`：`FMT_EXIT=0`、`CLIPPY_EXIT=0`、`### [PV1-c] cargo test --locked --workspace --all-features` → `TEST_EXIT=0`；我按 `test result:` 行独立求和：**86 个 target、994 passed / 0 failed / 2 ignored**（与集成报告一致；全文件无 `FAILED`/`error:`/`panicked`） | ✅ 一致（994/0/2） |
| `[PV2]` 依赖方向 | 同上：`crate boundaries OK: 12 个 crate…` + `PV2_EXIT=0`；`du1-integration.log` 内亦有一次 | ✅ 一致（12 crate） |
| `[PV3]` | `du1-pv1-merged.log` 的 `### [PV3] cargo test --locked -p storage-sqlite -p core --all-features`：16 个 target、**250 passed / 0 failed / 2 ignored**，`PV3_EXIT=0` | ✅ 一致（无 0 用例异常、无失败）；注：两个 ignored 即 §6.4 的既有夹具/子进程生成器 |
| `[PV5]`（含任务点名的用例） | `reports/pv5-windows-nodelink.log`：`test a_narrowing_repair_closes_the_live_attachment ... ok`、`test the_controlled_path_runs_end_to_end_and_revocation_propagates ... ok`、`test tls_direct_terminates_the_same_handshake ... ok`（3 passed / 0 failed）与 `node_pair_confirm_rejects_{an_unknown_export_id_without_writing,an_export_id_disjoint_from_the_grants} ... ok`（2 passed / 0 failed），`PV5_EXIT=0` | ✅ 一致（受控路径在候选上真的执行了，且包含收窄修复用例） |
| 未跟踪变更目录不在候选树 | 集成报告 §4.3 自述；`git ls-files`/目录实测 | ✅ 一致（详见 §6.3 的取位缺口判定） |

> 说明：以上均为**读日志**核对，本角色不产生新的 Project Verify 证据；`check:drift` 的「首绿」结论的可信度来自原始成功行与「无差异行」两个事实。

### 6.2 ISSUE-2（既有 flaky）核对

| 核查 | 方法 | 结果 |
| --- | --- | --- |
| 候选两轮都跑到了该用例且**通过** | `grep` 两份日志 | `du1-pv1-merged.log:814` 与 `pv5-windows-nodelink.log:112` 均为 `test the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown ... ok` ✅ |
| **没有** `#[ignore]`/跳过/弱化 | 候选树 `grep -rn "^#\[ignore" crates/` 计数与逐条比对 base | 候选 **2** 条、base 也是 **2** 条且**逐字相同**（`commit.rs` 的 `crash_child`、`migration.rs` 的 `regenerate_v1_fixture`），全在 `storage-sqlite` 的测试文件里；`daemon_lifecycle.rs` 无任何 `ignore` ✅ |
| 断言未被弱化 | `git diff --stat 3cadb12d..HEAD -- crates/app/tests/daemon_lifecycle.rs`；读该用例正文 | 该文件**零差异**（不在本变更改动面内）；用例仍断言首轮 tick 已发生、`reason == "startup"`、关闭 < 10s、`daemon.task_stopped` ✅ |
| `2 ignored` 的来源 | 与 base 的 `#[ignore]` 逐条比对 | 是 base 既有的两个辅助生成器（子进程崩溃夹具、夹具重建器），**非本次新增跳过** ✅ |

### 6.3 `check:docs` 的范围缺口与判定

- **事实**：`openspec/changes/node-trust-export-ids/**`（含 `verification.md` 与 `reports/**`）是主检出的**未跟踪**内容，不在候选分支树内。候选树的 `check:docs` 实际扫描 **324** 个 md（`du1-integration.log` 的 `380 relative links, 5131 section refs across 324 markdown files`），**不包含**变更目录；而该目录的文件（尤其 `verification.md` 与各角色报告）在**合入本地 `main` 后**会进入扫描范围。
- **为什么这不是候选缺陷**：这是「取位」问题而非内容问题——变更目录从未被任何分支跟踪（`git ls-files openspec/changes/node-trust-export-ids` = 0），因此候选树按设计不可能包含它。主 Agent 已在 `verification.md` 的 Check Plan Changes 登记该取位规则，并把权威树全量判定登记为合入后的任务（6.7/7.1）；`tasks.md` 6.7 也要求「核对实际主分支结果与候选一致性，完成 [PV1]/[PV2] 主分支回归」。
- **对**本轮候选结论**的影响**：**不影响**。① 差别只在「被扫描的 md 文件集」，而候选树内的文档面 `check:docs` 已经 exit 0；② 变更目录里唯一曾导致红的就是 RV4-DOCS-X1 的 `verification.md` 句式，该处已修（§4 表内条目）；③ 本轮以只读近似脚本独立扫了**当前主检出**（含变更目录，355 个 md）→ **0 problem**；同一脚本在候选树上的输出与门禁日志**完全吻合**（324 文件 / 5131 章节引用 / 2524 未判定）⇒ 近似可信，且它在合成样例上能复现修复前的那 1 条报错。**但**：我的报告本身就是该目录下**新增**的一个 md，因此正式判定仍必须在**含本报告的权威树**上实跑一次。
- **需补的证据（请主 Agent 执行，属合入后/最终验收门禁，不是候选阻断项）**：
  - `cd D:\Project\acp-remote && node scripts/check-doc-links.mjs`（权威树；期望 exit 0。这是 `check:docs` 子门禁；如需完整门禁则跑 `npm run check`）。
  - 记录时的期望值：文件数 ≥ 355（含本报告），且不应出现任何 `§N 在 <doc> 中不存在` 行。

### 6.4 与 Project Verify 并行/未返回项的核对状态

本变更的 Project Verify（[PV1]–[PV5]）与关键门禁在候选阶段**均已返回 PASS 并留日志**（§6.1），无「待核对」项；唯一 PENDING 的是**合入后**的权威树 `check:docs`（§6.3，作用域在 main/final-main 阶段）。RV3-IMPL 当年记 PENDING 的 [PV2]/[PV5] 已在本轮以候选日志闭合。

## 7. Findings（本轮新发现，RV5-IMPL-Fn）

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation |
| --- | --- | --- | --- | --- | --- |
| RV5-IMPL-F1 | MINOR（非阻断） | `verification.md` 的 Review Findings（RV3-IMPL-F6 行与「仍未覆盖的检查面」段）；`design.md` 的 D8「已知限制」段 | RV3-IMPL-F6（设备面同口径）与 RV3-IMPL 的未覆盖面 ②（Access 侧真实重连）在候选树中**只**登记在 `verification.md` 的 Review Findings 与那段归档备注里；`design.md` D8/D10 与 `plan.md`/`tasks.md` 无对应「后续切片要求」行（已核实：`design.md` 全文无「切片 7」「设备连接」相关条目） | 后续切片（设备/`server::sync`）开工时，这些义务只存在于一份 review 记录/归档备注中，容易被漏读——正是 RV2-IMPL-F1 那类「缺口在切片边界上复现」的成因 | 归档前把两条义务写进 `design.md` 的 D8「已知限制/后续」或 `plan.md` 的后续切片注记（一行文字，无需改代码）；若主 Agent 判定 `verification.md` 已足够，可在该处注明「归档后以 archive 内本行为准」 |
| RV5-IMPL-F2 | MINOR（非阻断） | `docs/NODE_LINK_PROTOCOL.md:280`（§8.2「收窄在落定后于连接边界强制」）；`crates/app/tests/node_link_e2e.rs:723-880` | 文档只写「收窄**自下一次握手后的新连接**生效」，**没有**一句显式说明「关闭生效前旧连接仍可能收到一条已收窄 Export 的事件」，也没有断言量化该残余窗口；实现侧 `request_close` 是请求、会话要排空已入队消息后才发 close 帧（预算 2 s） | 语义上不算矛盾（生效边界已写），但「收窄后到连接真正关闭」这段窗口的**残余可见性**在权威文档里只是隐含，读者可能误以为「confirm 返回即不可见」；将来新增运行期改清单入口时该窗口会变大 | 在 §8.2 该段补一句显式限定（例如「关闭是请求，旧连接在关闭生效前仍可能投递一条已入队事件；因此收窄只保证自下一次握手后的新连接生效」），并把「确认返回后旧连接不再收到新事件」登记为切片 7 的验证点。**是否本次收口由主 Agent 裁定**（纯文档一句） |
| RV5-IMPL-F3 | SUGGESTION（非阻断） | `tasks.md` 任务 6.4 与 `verification.md` 的 Handoff Index | `tasks.md` 6.4 写完成条件为「报告（`reports/rv2-du1.md`）」，而 `verification.md` 的 Handoff Index 行与本轮实际产出都是 `reports/rv5-impl.md`（本轮调度指令亦指定后者）；候选树侧不存在 `reports/rv2-du1.md` | 归档后的任务记录会指向一个不存在的报告路径（低风险，不影响交付正确性） | 归档前二选一：把 `tasks.md` 6.4 的路径改为 `reports/rv5-impl.md`，或在 6.4 的完成条件里注明「候选阶段 review 报告以 Handoff Index 登记的 `rv5-impl.md` 为准」 |
| RV5-IMPL-F4 | SUGGESTION（观察，非缺陷） | `docs/NODE_LINK_PROTOCOL.md:5/7`、`docs/LOCAL_ADMIN_PROTOCOL.md:4` | `git diff --check 3cadb12d..HEAD` 报三处新增/改写行 `trailing whitespace` | 无（`git diff --check` 不是本仓库的门禁；这两个文件头 blockquote **所有**行都以两个空格结尾作为硬换行，base 亦如此——我逐行核对了 `cat -A`/`od` 的 EOL 字节） | 无需修改；仅记录以免后续误判为「行尾卫生回归」 |

## 8. Assessment

### 8.1 结论

**PASS**（候选 `64e179c618e690efcc39f64854d59861e9ecbacf`，merge / candidate）。

- **合并完整性**：无缺陷。base 与两个源 tip 都是候选祖先（M2）；候选的 `crates/**` 与 `659e5900` 逐字节一致、`docs/**`+`openspec/specs/**` 与 `cc6faefd` 一侧逐字节一致（M3/M4），且两侧差异分别等于「对方源分支自身的 diff」（M5/M6）⇒ **精确并集，无静默丢失、无夹带**；两次 `--no-ff` 合并**没有冲突解决记录**（M7/M8/M9），与集成报告一致；`refs/heads/main` 仍是 base（M11），因此**没有**「最新主分支新增交互」需要复核；无冲突标记残留。
- **未被任何独立 review 覆盖的两处 delta**：`cc6faefd` **确认纯注释**（非注释行为改动 0 行，已用逐行过滤计数证明），新注释与 `compose.rs`/`command.rs`/`conn::close` 的**实际行为逐条一致**，`§12.3`（Catalog 消息，第 513/520/521 行）**指向正确**；`659e5900` **单个 hunk、1 增 1 删、只改那一行**，全文件已无错误形式的 `§12.6`，且**未**顺带改动同行的 `§8.2`/`§15` 或其它 `§12.3`/`§12.4`/`§14.2` 引用（它们本就正确）。
- **RV2/RV3-IMPL 已审的两处代码 delta 在候选树上仍成立**：`close_node_after_reauth` 分流（端口 → 组合根 → `node_reauth`）、`1000` + reason、`node_reauth` **不推**消息，三层+用例层证据齐备。
- **历史 findings 闭环**：两条 MAJOR 类（RV1-DOCS-F1、主 Agent 复审-1）与越界的 RV4-DOCS-X1/ISSUE-1 均**已解决**；其余全部 MINOR/SUGGESTION 要么已修（RV1-DOCS-F2–F6、RV2-DOCS-F1/F3、RV3-DOCS-F1/F2、RV4-DOCS-F1、RV1-IMPL-F1–F6/R9、RV2-IMPL-F1、RV3-IMPL-F1–F3、跨 WP 待办 1/2、RISK-1–5），要么是主 Agent 明文裁定接受的既有/后续项（RV1-IMPL-R14、RV2-IMPL-F2/F3、RV3-IMPL-F4–F7）。**无「已确认缺陷被遗留」**。
- **RV3-IMPL 的未覆盖面**在候选阶段**均不构成阻断**（§5）：②④ 是后续切片义务，①③⑤⑥⑦ 是测试强度/证据完整性登记项。
- **门禁证据一致**：`npm run check` 10/10 且 `check:drift` 首绿、workspace 994/0/2、`[PV2]` 12 crate、`[PV5]` 含任务点名的收窄修复用例——日志原始输出与集成报告结论**逐项一致**；ISSUE-2 的 flaky 用例两轮均通过，**无** `#[ignore]`、无跳过、无弱化断言（候选 tree 的 ignore 集合与 base 完全相同）。
- **`check:docs` 的取位缺口不影响本轮候选结论**（§6.3）：候选树按设计不含（未跟踪的）变更目录；本轮以只在仓库外运行的只读近似脚本核对**当前主检出**（含变更目录）为 **0 problem**，且该脚本在合成样例上能复现修复前的报错、在候选树上的输出与门禁日志逐字吻合。**正式判定仍须在含本报告的权威树实跑一次**（命令见 §6.3），这是**合入后**的门禁，不是候选阻断项。

### 8.2 逐检查 ID 结论

| 检查 ID / 来源 | 核对方式 | 结论 |
| --- | --- | --- |
| `[PV1]`（`npm run check` 10 道子门禁 + Rust 三命令）/ candidate | 读 `du1-integration.log`、`du1-pv1-merged.log` 原始输出并独立求和 | **PASS / REUSED**（NEW 由集成/主 Agent 产出；我按日志核对：10/10、994/0/2、`TEST_EXIT=0`/`EXIT_CODE=0`） |
| `check:drift` / candidate | 读成功行与差异行 | **PASS / REUSED**（§7 的 36 条 DDL、§5 的 15 trait/93 签名逐条一致） |
| `[PV2]` / candidate | 读 `PV2_EXIT=0` 与 `crate boundaries OK: 12 个 crate` | **PASS / REUSED** |
| `[PV5]`（替代验证主体）/ candidate | 读受控路径用例清单与 `PV5_EXIT=0`，并**自读用例源码**核对断言判别力 | **PASS / REUSED** |
| `check:docs` / 权威树（含变更目录）| **本轮未执行**（本角色不跑门禁；主 Agent 自持门禁） | **PENDING / BLOCKED**（见 §6.3；**属合入后 main/final-main 阶段的取位项**，不影响候选结论） |
| E2E | `plan.md` 记 `not-applicable`（含理由/依据/`downgrade_approval`），替代验证 = `[PV5]` | **NOT_APPLICABLE**（本轮不重判） |
| 本报告自身的结论 | 只覆盖候选阶段的 REVIEW；不代替 Project Verify，不代表任务可勾选/可归档 | — |

### 8.3 本报告**不能**据以宣称的事

- 不宣称 `cargo`/`npm` 在本轮被复跑（本角色按调度要求未执行）；所有门禁结论均为**日志核对**。
- 不宣称权威树（含 `openspec/changes/node-trust-export-ids/**`）的 `check:docs` 已通过——本轮只有**静态近似**（0 problem）与命令建议。
- 不宣称 E2E 通过（本变更为 `not-applicable`，替代验证已在候选执行并留证）。
- 不宣称候选已合入、可归档、或任务可勾选（合并与最终验收由主 Agent 按 schema 流程判断）。
- 不代替 6.8（合入后的 post-merge review）：若合入提交与候选无差异，主 Agent 可按规则复用本轮 review ID；**有差异必须重开复核**。

## handoff_index

```yaml
handoff_index:
  - task_id: "6.4"
    role: reviewer
    phase: review
    stage: candidate
    target_revision: "64e179c618e690efcc39f64854d59861e9ecbacf"
    evidence_type: REVIEW
    evidence_id: RV5-IMPL
    report_path: "openspec/changes/node-trust-export-ids/reports/rv5-impl.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "merge 类型候选复核：祖先关系与两侧逐字节等价（git merge-base --is-ancestor / git diff 逐侧为 0）、合并无冲突解决、未被独立 review 的两处 delta（cc6faefd 注释级、659e5900 §15 的 §12.6→§12.3）在候选树上逐项成立、RV2/RV3-IMPL 已审的两处代码 delta 仍成立、RV1–RV4 各轮 findings 与主 Agent 复审-1/跨 WP 待办/ISSUE 逐条闭环；门禁证据按 du1-integration.log / du1-pv1-merged.log / pv5-windows-nodelink.log 的原始输出核对（10/10、check:drift 首绿、994/0/2、[PV2] 12 crate、[PV5] 含 a_narrowing_repair_closes_the_live_attachment）。0 CRITICAL / 0 MAJOR；4 条非阻断项（F1 MINOR、F2 MINOR、F3/F4 SUGGESTION）"
    source_evidence: NOT_APPLICABLE

  - task_id: "2.3 / 2.4 / 2.5"
    role: reviewer
    phase: review
    stage: candidate
    target_revision: "64e179c618e690efcc39f64854d59861e9ecbacf"
    evidence_type: REVIEW
    evidence_id: RV5-IMPL
    report_path: "openspec/changes/node-trust-export-ids/reports/rv5-impl.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在候选树上复核工作包面：RV3-IMPL-F1/F2/F3 的修复事实（router.rs:816 注释、NODE_LINK_PROTOCOL.md:930 与 pairing.rs:64 的 §12.3、delta spec 的新场景 + design.md D8 + plan.md R19）、RV1-IMPL-F4/F5/F6/R9、跨 WP 待办 1/2、ISSUE-2 的 flaky 未被跳过或弱化；RV3-IMPL-F4–F7 仍为已裁定接受的既有/后续项"
    source_evidence: NOT_APPLICABLE

  - task_id: "6.7 / 7.1"
    role: reviewer
    phase: review
    stage: final-main
    target_revision: NOT_AVAILABLE
    evidence_type: CHECK
    evidence_id: "check:docs（权威树，含变更目录）"
    report_path: NOT_AVAILABLE
    result: BLOCKED
    evidence_status: PENDING
    applicability_basis: "候选树按设计不含未跟踪的 openspec/changes/node-trust-export-ids/**（git ls-files = 0），因此该判定只能在合入后的权威树取得；本轮以仓库外只读近似脚本核对主检出为 0 problem（并验证该近似能复现修复前的报错），但这不是门禁证据。待补命令：cd D:\\Project\\acp-remote && node scripts/check-doc-links.mjs（期望 exit 0，文件数 ≥ 355，无 §N 不存在行）；需要完整门禁时跑 npm run check。属合入后门禁，不阻断候选结论"
    source_evidence: NOT_APPLICABLE
```

### 返回给主 Agent 的摘要

- **结论**：**PASS**（候选 `64e179c618e690efcc39f64854d59861e9ecbacf`；0 CRITICAL / 0 MAJOR；4 条非阻断：F1/F2 MINOR、F3/F4 SUGGESTION）。
- **合并完整性**：无缺陷。两源与 base 均为候选祖先；候选 = 两源**精确并集**（`crates/**` 与代码源、`docs/**`+`openspec/specs/**` 与文档源各自逐字节一致，两侧差异恰等于对方源分支自身的 diff）；两次合并**无冲突解决**、无冲突标记；`main`/`origin/main` 仍等于 base ⇒ 无「最新主分支新增交互」。
- **未覆盖 delta**：`cc6faefd` **确实只有注释**（非注释行 0），注释与 `compose.rs`/`command.rs`/`conn::close` 行为一致，`§12.3` 指向正确（Catalog 消息，第 513/520/521 行；`§12.6` 是控制与错误消息）；`659e5900` 单一 hunk、只改那一行、全文件错误 `§12.6` 清零、未顺带改动其它引用。RV2/RV3-IMPL 审过的分流/`1000`+reason/不推消息在候选树上**仍成立**。
- **历史 findings**：**全部闭环**——已修的都在候选树验证到修复事实（含两条 MAJOR 类与 RV4-DOCS-X1/ISSUE-1）；未解决的只有经你明文裁定接受的既有/后续项（RV1-IMPL-R14、RV2-IMPL-F2/F3、RV3-IMPL-F4/F5/F6/F7）与已登记残余风险（ISSUE-2 flaky、ISSUE-3 断言债）。**主 Agent 复审-1 已闭环**：撤销 = 推消息 + `4410`；重新配对 = 不推消息 + `1000` + 准确 reason，四层（端口/组合根/node_link/用例）可区分。
- **RV3-IMPL 的未覆盖面**：**无一条阻断候选**；②（Access 侧真实重连）④（设备面）是后续切片义务，其余是测试强度/证据完整性登记项。唯一建议的「本次小补」是 F2 的那一句文档限定（非阻断，你可决定是否随归档前收口）。
- **门禁证据**：与集成报告**一致**（10/10、`check:drift` 首绿行逐字对上、workspace 994/0/2、[PV2] 12 crate、[PV5] 含 `a_narrowing_repair_closes_the_live_attachment`）；ISSUE-2 两轮首跑通过，候选树 `#[ignore]` 集合与 base **完全相同**（2 条，均为既有夹具/子进程生成器），断言未弱化。
- **`check:docs` 缺口**：不影响本轮候选结论（候选树按设计不含未跟踪变更目录）；本轮以仓库外只读近似核对当前主检出（含变更目录）**0 problem**，且该近似在合成样例上能复现修复前的报错、在候选树上的数字与门禁日志逐字吻合。**待办**：合入后请在权威树实跑 `cd D:\Project\acp-remote && node scripts/check-doc-links.mjs`（期望 exit 0；含本报告后文件数 ≥ 355），并登记到 6.7/7.1。
- **建议你处置的 3 件小事（均非阻断）**：①（F1）把「设备面同口径 + Access 侧真实重连」两条后续义务从 `verification.md` 补写进 `design.md` D8 或 `plan.md` 的后续切片注记；②（F2）在 `docs/NODE_LINK_PROTOCOL.md` §8.2 补一句「关闭是请求，旧连接在关闭生效前仍可能投递一条已入队事件」并登记切片 7 验证点；③（F3）统一 `tasks.md` 6.4 的报告路径（现写 `reports/rv2-du1.md`，实际为 `reports/rv5-impl.md`）。
- **报告路径**：`openspec/changes/node-trust-export-ids/reports/rv5-impl.md`。
- **我未做**：修改任何代码/测试/文档/规划/任务状态/`verification.md`，跑 `cargo`/`npm`/E2E，切分支或提交（唯一写入是本报告；唯一临时文件在仓库外的 `/tmp/rv5/docrefs.py`）。
