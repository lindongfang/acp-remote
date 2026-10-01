# DR1 Round 14 — 依赖声明审查报告（Review Type: plan）

> 持久化说明：本报告由 reviewer 实例（只读工具集）以全文返回、由主 Agent 原样落盘。run `b7df8d77-f456-4c75-9faf-9a5a339c361e`。

```yaml
task_id: "NOT_APPLICABLE（规划门禁，Round 14）"
work_package: "NOT_APPLICABLE"
role: reviewer
phase: plan
agent_context: >
  第 14 个全新独立 reviewer 实例；未参与前 13 轮，也未参与任何 WP 的实现、
修复或实现讨论。隔离方式由调度者设置（要求 fork_turns="none" / 等效隔离），
  本实例自述只声明收到的输入与自身限制。
target_revision: "sha256:b7650f006ecf71e2dcc742c02b974fff7b2cd5c6b9527654a683a6382205ddc2"
scope: >
  plan.md 的依赖类型、批次/波次、资源互斥与写入归属声明；
  以及 plan.md ↔ proposal.md / specs/** / design.md / tasks.md 的一致性。
changes: "无（只读；未修改任何文件、未运行 cargo/npm、未提交）"
checks: "见 Assessment 的逐问题核对"
issues: "0×CRITICAL / 0×MAJOR / 5×MINOR（DR1-F57…F61）"
result: PASS
evidence_paths:
  - openspec/changes/session-resume/plan.md
  - openspec/changes/session-resume/tasks.md
  - openspec/changes/session-resume/design.md
  - openspec/changes/session-resume/verification.md
  - D:/Project/acp-remote-wt/session-resume-du1/（集成基线工作树，只读取证）
resource_cleanup: "NOT_APPLICABLE（本轮未分配任何资源）"
```

## Review Context

| 项 | 实际取值 |
| --- | --- |
| Review ID / Round | DR1 / **14**（沿用线程，续用编号 F57–F61） |
| Review Type / Stage | `plan` / 计划与依赖声明阶段 |
| Work Package | `NOT_APPLICABLE` |
| Base / Target Revision | `NOT_APPLICABLE` / `sha256:b7650f006ecf71e2dcc742c02b974fff7b2cd5c6b9527654a683a6382205ddc2` |
| Repository | `D:/Project/acp-remote`（规划根，权威） |
| 读取的规则 | `AGENTS.md`（§4 依赖规则、§8 工作流、§9 测试、§10 文档维护、§12 工具链）、`openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md`、`openspec/changes/session-resume/{proposal,design,plan,tasks,verification}.md` |
| 实际检查范围 | ① Round 14 补登记三项义务的 plan/tasks/SFO 三处一致性；② WP6 的依赖声明属实性与写范围覆盖度；③ WP6 义务涉及文件的写入归属；④ PV1/PV2 阶段制与红窗口口径；⑤ 两条用户挂起项的阻塞面；⑥ Round 1–13 已登记处置的抽查（5 处） |
| 使用的证据 | `verification.md` 的 DCR 表（Round 1–13）、`## Check Plan Changes`、`## Review Findings`、`## Dispatch Reconciliation`、`## Worktree Handoff`、`## Failures and Retries`；集成基线工作树 `D:/Project/acp-remote-wt/session-resume-du1` 上的**实际文件内容**（`docs/NODE_LINK_PROTOCOL.md`、`docs/CORE_PORTS_AND_STORAGE.md`、`crates/agent-host/src/bin/acpr-fake-acp-agent.rs`、`crates/*/Cargo.toml`、`crates/*/tests/` 目录清单、`impl SessionStore for` 的 7 处实现点） |
| 限制 | 本实例**无 shell 权限**，未执行 `npx openspec-agentic workflow check --stage plan --json` 复核 contractDigest；摘要绑定依赖调度者给出的取值。已核对 `verification.md` 的 DCR 表最新行为 Round 13（`sha256:1c93193e…`），与「Round 13 之后 plan/tasks 被回写」一致，故本轮必需。未判代码质量、未判测试充分性、未判需求实现。 |

---

## Findings

| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
| --- | --- | --- | --- | --- | --- | --- |
| DR1-F57 | MINOR | `tasks.md` 2.6（WP6）义务编号；对照 `plan.md` WP6 行 | tasks 2.6 出现「额外义务 ①（DR1 Round 9）…②（DR1-F42）…**④（CR3-F1）**…**⑤（DR1-F41）**…**④（DR1-F51）**」——**缺 ③、且 ④ 出现两次**；plan.md WP6 行用的是 ①②③（F41）④（CR3-F1），且 DR1-F51 的文档义务只写在写范围里未编号 | 派发提示、CR6 复核清单与 `Dispatch Reconciliation` 若按编号引用「义务 ④」，会指向两条不同义务；Round 14「逐字一致」的可核对性受损（义务正文本身一致，只有编号错位） | tasks 2.6 重排为 ①…⑥ 连续编号并与 plan WP6 行一一对应（F41=③、CR3-F1=④、F51 的文档义务=⑥），或在两处都改成不带编号的稳定短标识 | 待 Round 15（或主 Agent处置后由新 reviewer 复核） |
| DR1-F58 | MINOR | `plan.md` WP6 行 Write Scope：`docs/NODE_LINK_PROTOCOL.md（仅 §10 收窄句与 §12.7 示例注记，DR1-F51）` | 实测：集成基线工作树中该「收窄句」在 **`:662`，落在 §12.7（§12.7 起于 :605，§12.8 起于 :664）**；而 **§10 是 WP2 的区域**（`grant.remote-work` 会员行，`:409`，即 DR1-F11 的对象）。SFO 行与 tasks 2.6 都正确写作「:662 收窄句 + §12.7 示例注记」 | 只有 plan 的 WP6 写范围单元标错区域；只读该单元的实现者可能去改 §10，与 WP2 的 §10 区域相撞（SFO 的合并顺序与责任人会被绕过） | 把 plan WP6 写范围改为「§12.7 的 `:662` 收窄句 + §12.7 示例注记」，与 SFO/tasks 逐字一致 | 同上 |
| DR1-F59 | MINOR | `design.md:157-158`（D5 文档与机器资产同步表） | D5 的 `docs/CORE_PORTS_AND_STORAGE.md` 行只列 §5.1/§7.3/§2，**缺** plan WP3/WP4 已登记的 §3.1/§3.3/§3.6/§4/§5.2、§7 标题/§7.2、§9 判据 1/28（DR1-F52）、§11.3（CR4-F1）、文件头版本记录；D5 的 `docs/NODE_LINK_PROTOCOL.md` 行**缺** WP6 的两处（`:662` 收窄句、§12.7 版本注记，DR1-F51）。实测这些区域在集成基线上**已实际改动**（§11.3 = `user_version = 6`；§9 判据 1 已含 v1→…→v5；文件头 0.15 条目覆盖 §7 v5/§9/§11.3） | D5 是 design侧声明的「AGENTS.md §10 资产同步」权威表，与 plan 的写范围不一致；门禁不覆盖该表，后续以 D5 为清单的实现者会漏改（本次只是**记录层**不一致，实际交付未漏） | 补齐 D5 两行；或加一句「逐 WP 的具体区域以 plan.md `## Shared File Ownership` 为准，本表只列资产级范围」 | 同上 |
| DR1-F60 | MINOR | `plan.md` `## Contract Changes` 的 **Round 14** 段末句：「CR5-F1 还需 **WP5 的小范围重开**（新增该选项）后由新 reviewer 复核」 | 与事实不符：该义务**已交付并复核**——`crates/agent-host/src/bin/acpr-fake-acp-agent.rs` 在集成基线上已有 `--dump-request-params`（`:17-20` 文档注释、`:40/:61/:81` 参数解析、`:202` 的 `dump_method` 注释），`verification.md` Handoff Index 记录 coder-E2 交付 `1376e1b`（PV1 阶段 1 PASS）与 reviewer-E2 的 **CR5 Round 2 PASS@1376e1b**，并已并入 `5ab7e9d` | 计划文本停留在未来时，读起来像「WP6 派发前还有一个未完成的 WP5 前置」，与 `## Dispatch Reconciliation`（WP5 attempt2 = merged）矛盾。属记录层（与 DR1-F29/F53 同类），不阻塞派发 | 改为「CR5-F1 已由 WP5修复轮 `1376e1b` 交付、CR5 Round 2 复核 PASS（`reports/cr5-review-round2.md`），并入集成基线 `5ab7e9d`」 | 同上 |
| DR1-F61 | MINOR | `plan.md` `## Shared File Ownership` 缺 `crates/server/tests/` 行 | 该目录存在且有 5 个既有集成测试文件（`local_admin_channel.rs`、`local_admin_schema_drift.rs`、`local_endpoint_naming.rs`、`local_endpoint_unix.rs`、`local_endpoint_windows.rs`）。同类目录**都**登记了 Writers/Merge Owner/Order/Re-verify（`crates/agent-host/tests/`=WP5,TP2；`crates/app/tests/`=WP6,TP2；`crates/storage-sqlite/tests/`=WP4,TP2；`crates/node-link-protocol/tests/`=WP2,TP2），唯独 server/tests 没有；而 WP6 的写范围是整个 `crates/server/`（含该目录），TP2 又按通用行独占 `crates/**/tests/` 的**新增**文件 | 该目录是 WP6 与 TP2 唯一的口径交叠面（W4→W5，**不同波次、不并发**，故不构成同批重叠）；缺行使合并负责人/顺序/复跑项在该目录上无据可查 | 补一行：`crates/server/tests/ \| WP6, TP2 \| TP2 \| WP6 → TP2 \| TP2: PV1` | 同上 |

---

## Assessment（逐条回答本轮 5 个问题）

### Q1 — Round 14 补登记是否自洽？→ **是**（三项义务的正文、归属包、完成判据都成立），但编号与两处措辞有 MINOR 瑕疵

| 来源 | 归属包 | plan 落点 | tasks 落点 | SFO 落点 | 正文一致性 | 完成判据 |
| --- | --- | --- | --- | --- | --- | --- |
| CR3-F1（`create_session` 会话行已提交后失败须结 `uncertain`） | **WP6（W4）** | WP6 行「额外义务 ④（CR3-F1）」 | 2.6「额外义务 ④（CR3-F1）」 | 由 `crates/server/`、`crates/app/` 整 crate 写范围覆盖 | ✅ 语义逐字一致（会话行已提交之后 / `StorageFull`·`IoError` / 不得结 `failed` / 结 `uncertain` / 孤儿会话理由） | ✅ 明确（终态取值 + 触发条件）；`verification.md` 的 CR3-F1 行亦标「WP6 派发时按此执行」 |
| CR4-F1（§11.3「过新」取值随 v5 由 5→6） | **WP4（W3，已 merged）** | WP4 写范围「§11.3 的「过新」用例取值」 | 2.4「§11.3 的「过新」用例取值（CR4-F1）」 | `docs/CORE_PORTS_AND_STORAGE.md` 行区域注记含「§11.3 的「过新」取值（CR4-F1）」 | ✅ 三处均写「随 `FILE_FORMAT_VERSION=5` 由 5 改为 6」；实测集成基线 `docs/CORE_PORTS_AND_STORAGE.md:1479` 已是 `user_version = 6` | ✅ 明确（具体取值）；回写与交付一致 |
| CR5-F1（新增并列选项 `--dump-request-params`，不改 `--dump-requests` 语义） | **WP5（W3，已 merged）** | WP5 行「额外义务（CR7-F5）…**并新增并列选项** `--dump-request-params <path>`」 | 2.5 同名义务 + 「不改 `--dump-requests` 的语义」 | 由 `crates/agent-host/` 整 crate 写范围覆盖 | ✅ 两处措辞一致；实测基线已含该选项且 `--dump-requests` 语义未变 | ✅ 明确（选项名 + 每行 `{method, params}` + 不得改既有语义）；CR5 Round 2 已判 PASS |

结论：**三项义务都具备明确归属包与完成判据**，CR4-F1/CR5-F1 的回写与实际交付一致（抽查证据见上表最后一列）。瑕疵为 DR1-F57（tasks 2.6 编号缺 ③、④ 重号，与 plan 的 ①–④ 不对齐）、DR1-F58（plan WP6 写范围把 §12.7 的 `:662` 误标为 §10）、DR1-F60（Round 14 段末句仍是「还需 WP5 重开」的过期未来时）。三者均为记录层，不改变任何依赖或归属事实。

### Q2 — WP6 是否已具备可派发条件？→ **是**

1. **依赖声明属实性**（逐条核对 `code:WP1…WP5` 是否真需要上游**代码**）：
   - `code:WP2` ✅ 真需要：`WirePayload::SessionResume`（第 13 个变体）是 `crates/server` 直接依赖的 `node-link-protocol` 类型，`core_payload()` 的穷尽匹配必须补臂。
   - `code:WP3` ✅ 真需要：`SessionStore::load_recovery` / `SessionBackendFactory::resume` / `SessionEndpoint::agent_session_id` / `settle_session_resume` 都由 core 定义，server/app 必须对其作答。
   - `code:WP4` ✅ 真需要：`app` 直接依赖 `storage-sqlite`，`SqliteStore` 必须实现新必需方法（实测 `crates/storage-sqlite/src/session_store.rs:1942`）。
   - `code:WP5` ✅ 真需要：`app` 依赖 `agent-host`（`app/Cargo.toml:33`），受控路径用例要跑真实恢复路径。
   - `code:WP1` ⚠️ **间接但成立**：`crates/server` 与 `crates/app` 的 `Cargo.toml` **不直接依赖** `acp-protocol`（全仓唯一直接依赖者是 `crates/agent-host/Cargo.toml:13`），WP1 的响应 DTO 只经 `agent-host` 的 lib 传递。声明不是虚报（WP6 的 app 级受控路径确实编译 WP1 的代码），但它比其余四条弱一档。不构成「声明不实」，建议在 Dependency Handoffs 的 WP6 行注明这条是**传递依赖**。
   - **无同批互为前置**：WP6 在 W4，上游全部在 W1–W3（Execution Waves 表），不存在 DR1-F1 那类死锁。
2. **与实际集成基线一致**：✅ `verification.md` 记录三次集成均 `--no-ff` 零冲突并实测祖先关系，链为 `81e350f → b0a387b(+WP1/WP2) → 8a08db8(+WP3) → b486c53(+WP4) → **5ab7e9d(+WP5 修复轮 1376e1b)**`；WP6 的 W4 进入条件「WP1/WP2/WP3/WP4/WP5 均已进入已验收集成基线」成立。本轮另以只读方式在 `D:/Project/acp-remote-wt/session-resume-du1` 上核到 WP5 修复产物（`--dump-request-params`）与 WP4 的文档产物（§11.3 = 6），与该基线一致。
3. **写范围覆盖度**：✅ 逐项核对 WP6 必须收口的东西——`core_payload()` 早退臂（`crates/server/src/node_link/command.rs`，整 crate 在写范围内）、**5 个 `SessionStore` 替身**（实测 `impl SessionStore for` 共 7 处：server 4 = `local_admin/test_support.rs:817 NotTouched`、`:961 FixedStore`、`node_link/command/tests.rs:145 CommandStore`、`node_link/resource/tests.rs:265 SliceStore`；app 1 = `app/tests/support/owner.rs:416 FlakySessionStore`；WP3/WP4 各 1）、`port_error_code` 新臂（`command.rs:2065`）、`uncertain` 终态（CR3-F1 义务 ④ + F41 义务 ⑤）、`settle_session_resume` 调用点（⑤）——全部落在 `crates/server/` 与 `crates/app/` 两个整 crate 写范围内，无一遗漏。
4. **一个非计划缺陷的派发前提**：`## Worktree Handoff` 里 WP6 的 baseline 仍是 `81e350f…`。`verification.md` `## Dependency Handoffs` 已把「W4 派发前由 provisioner 重定向到届时的集成基线」固化为流程动作，故这不是计划缺陷，但派发时必须执行（否则 `cargo -p server` 会看到 WP3 之前的旧接口）。

### Q3 — 是否存在未归属的写入面？→ **WP6 的义务全部有归属**；仅 `crates/server/tests/` 缺一行 SFO 登记（DR1-F61）

逐文件核对 WP6 声明义务涉及的目标：

| 目标 | 归属 | 依据 |
| --- | --- | --- |
| `crates/server/**`（`core_payload`、`port_error_code`、`NotTouched`/`FixedStore`/`CommandStore`/`SliceStore`、路由与受控路径） | WP6（整 crate 单写者） | plan WP6 写范围；W4，与 TP2（W5）不并发 |
| `crates/app/**`（`FlakySessionStore`、`ScriptedBackends`、组合根接线、`app/tests/` 受控路径） | WP6 + SFO 行 `crates/app/tests/ (WP6,TP2)` | plan WP6 写范围 |
| `docs/SESSION_CONTINUITY_DESIGN.md`（含 CR1-F2 状态注记） | WP6 单一写者 | plan WP6 写范围；design D5:158 |
| `README.md`「仓库当前状态」 | WP6（W2 改「合同检查」②） | SFO 行 `README.md \| WP2, WP6 \| WP6 \| WP2 → WP6` |
| `docs/DEVELOPMENT_PLAN.md` | WP6 单一写者 | plan WP6 写范围；design D5:159 |
| `docs/NODE_LINK_PROTOCOL.md`（`:662` + §12.7 注记） | WP6（W2 改 §10/§12.5/§12.7/§15） | SFO 行 `docs/NODE_LINK_PROTOCOL.md \| WP2, WP6 \| WP6 \| WP2 → WP6`，区域注记正确 |
| `crates/server/tests/`（WP6 可能扩展既有文件、TP2 新增用例） | **缺 SFO 行** | DR1-F61；两包不同波次，故非阻断 |

即：**没有「无主文件」**（DR1-F14 的病根未复发），唯一缺口是 server/tests 目录的合并安排未登记。

### Q4 — PV1/PV2 阶段制与红窗口口径是否自洽？→ **自洽**

- **PV1**：阶段1 = 分支按本 WP crate 子集（`-p acp-protocol` / `-p node-link-protocol -p core -p identity-auth` / `-p core` / `-p storage-sqlite` / `-p agent-host` / `-p server -p app`），阶段 2 = 「**全部 WP 集成后的**集成基线 / 候选 / 主分支」跑 `npm run check:rust`。与 `### Local Checks`（「不能代替阶段 2」）、W2 进入条件（「PV1 **阶段 1** 与 PV2 PASS」）、`## Completion Criteria`（「PV1 阶段 2 workspace 全量 + 各 WP 阶段 1 开发期证据」）四处口径一致。
- **PV2**：四阶段（分支 1 / 集成基线 2 / 候选 / 主分支），工作包集合 WP1–WP4、WP6（不含 TP2/TP5），与 tasks 的 `[PV2]` 标注一致；`npm run check` 不受红窗口影响（合同改动与镜像同包同提交）——已被三次集成实测（PV2 均 exit 0）。
- **红窗口**：当前 **8 条**（`E0046`×7 + `E0004`×1），全部落在 `crates/server/`（`local_admin/test_support.rs`×3、`node_link/command/tests.rs`×3、`node_link/resource/tests.rs`×1、`node_link/command.rs`×1），`agent-host` 与 `storage-sqlite` 已转绿 ⇒ **全部归 WP6**，与「收口责任人 = WP4/WP5/WP6，其中 server 侧全部归 WP6」一致。plan 未把「8 条」写死进正文（只列方法与责任人），因此不存在计数漂移风险。`app` 的 3 处诊断仍是「静态证据 + 被 `server` 遮蔽」的已知缺口（verification 如实登记），但 `crates/app/` 在 WP6 写范围内，结论不受影响。
- **派发条件一致性**：红窗口内各 WP 阶段 1 只查自己 crate ⇒ WP6 派发时它的 `-p server -p app` 必须自行全绿（红窗口的最后 8 条正好都在这两个 crate）⇒ 该检查在其分支上**可产出**，与派发条件一致。

### Q5 — 两条挂起项是否已消解？→ **均未消解，但都不是 WP6 的实际阻塞**

- **TP 存废**：`## Failures and Retries` 明确「不阻塞 merger 与 W2–**W4**；阻塞 **W5 · TP2**」，TP1 停在 `fixing` 且不派发实例。WP6 是 **W4** 且其义务（路由、接线、涟漪收口、两处文档、CR3-F1 终态）不依赖 TP1 的设计产物 ⇒ **不是 WP6 的阻塞**。注意它是 TP2（W5）开工的**硬前置**，届时未决会挡住 W5。
- **validator 未验证假设**（目标 Agent 是否真的宣告 `sessionCapabilities.resume`）：登记为「不阻塞当前实现，先挂着」，只影响 **7.1 独立验证**（无真实 Agent 时须记 BLOCKED 而非 PASS）⇒ 与 WP6 无关。
- 因此 **WP6 的实际前置只有两条**：① 本轮计划门禁 PASS；② worktree 重定向到 `5ab7e9d…`（verification 已固化的流程动作）。

### Round 1–13 已登记处置的抽查（不逐条重判）

抽查 5 处，全部**确已落地**：① F52/F55（§9 判据 1/28 + 文件头）→ 集成基线 `docs/CORE_PORTS_AND_STORAGE.md:1352` 判据 1 已含 v1→…→v5、文件头 0.15 条目覆盖 §7 v5/§9/§11.3；② CR4-F1（§11.3 = 6）→ `:1479` 已是 `user_version = 6`；③ CR5-F1（`--dump-request-params`）→ fake agent `:17-20/:40/:81/:202` 已在基线；④ F42（7 处实现 / 6 文件）→ 实测 `impl SessionStore for` 恰 7 处、6 个文件；⑤ F51/F56（`docs/NODE_LINK_PROTOCOL.md` 归属与行号）→ SFO 行写 `:662`，实测在集成基线**确为 :662**（§12.7 区间 605–663），与 tasks 2.6 逐字一致（仅 plan WP6 写范围单元误标 §10，见 DR1-F58）。

---

## 结论**PASS**（0×CRITICAL / 0×MAJOR）。结论绑定 `sha256:b7650f006ecf71e2dcc742c02b974fff7b2cd5c6b9527654a683a6382205ddc2`。

- 本轮 5 项发现全部为 MINOR（DR1-F57…F61），均为**记录层/措辞层**：不改依赖声明、不改写入归属事实、不改任何行为契约（proposal/specs/design D1–D6 未被本轮问题触及）。
- **不阻塞 WP6 派发。** 派发前请主 Agent 完成两件与本结论无关但必要的事：① 在 `verification.md` 的 `## Dependency Declaration Review` 追加 **DR1 / Round 14 / `sha256:b7650f00…` / PASS** 行并指向本报告；② 由 provisioner 把 `wt/session-resume-wp6` 重指向集成基线 `5ab7e9d…` 并回填 `## Worktree Handoff`。建议把 5 项 MINOR 的处置**合并到同一轮回写**（若只改 `plan.md`/`design.md` 会再次改变 contractDigest 并使本 PASS 失效；若全部只落在 `verification.md` 与派发提示，则本 PASS 保持有效）。

```yaml
handoff_index:
  - task_id: "NOT_APPLICABLE（规划门禁，Round 14）"
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    round: 14
    stage: plan
    target_revision: "sha256:b7650f006ecf71e2dcc742c02b974fff7b2cd5c6b9527654a683a6382205ddc2"
    evidence_type: REVIEW
    evidence_id: DR1
    report_path: "openspec/changes/session-resume/reports/dr1-dependency-review-round14.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: >
      Review Type=plan，stage=plan，目标是规划契约摘要（非提交 SHA）。核对对象为
      openspec/changes/session-resume/{plan,tasks,design,proposal,specs/**,verification}.md 的实际文本，
      外加集成基线工作树 D:/Project/acp-remote-wt/session-resume-du1 的只读取证
      （NODE_LINK_PROTOCOL.md:662/§12.7 区间、CORE_PORTS_AND_STORAGE.md:1352/1479/文件头、
      acpr-fake-acp-agent.rs 的 --dump-request-params、7 处 impl SessionStore、crates/*/Cargo.toml 依赖面、
      crates/*/tests 目录清单）。Range of checks: 依赖声明属实性（code:WP1…WP5）、Contract Freeze 路径可读性、
      波次与资源互斥、Shared File Ownership 覆盖、PV1/PV2 阶段制与红窗口、挂起项阻塞面。
      限制：本实例无 shell，未执行 workflow check 复核 contractDigest（依赖调度者给定取值）；
      未判代码质量/测试充分性/需求实现。历史轮次 Round 1–13 保留，本轮为最大 round 的当前结论。
    source_evidence: NOT_APPLICABLE
```

---

## 简答（给主 Agent）

- **判定：PASS**（0×CRITICAL / 0×MAJOR），绑定 `sha256:b7650f00…`；5×MINOR：tasks 2.6 义务编号缺③/④重号、plan WP6 写范围把 §12.7 的 `:662` 误标 §10、design D5 资产表落后于 plan 写范围、plan Round 14 段仍写「WP5 还需重开」的过期未来时、`crates/server/tests/` 缺一行 SFO。
- **是否阻塞 WP6 派发：不阻塞**（前置只有：本轮 PASS + worktree 重指向 `5ab7e9d`）。
- Q1 三项补登记正文逐字一致、归属与判据齐备（CR3-F1→WP6、CR4-F1→WP4、CR5-F1→WP5，后两项已交付并与基线实测一致）；Q2 依赖声明属实（`code:WP1` 为经 agent-host 的传递依赖，不虚报）、W4 无同批互斥、写范围完整覆盖 8 条红窗口与 5 个替身；Q3 无「无主文件」，仅 server/tests 缺登记；Q4 PV1/PV2 四阶段口径与红窗口责任人一致、WP6 的 `-p server -p app` 阶段 1 可产出；Q5 TP 存废与 validator 假设均**不**阻塞 W4（TP 存废硬阻塞 W5/TP2，validator 假设只影响 7.1）。
