<!-- 主 Agent 维护协作计划；规则见 schema.yaml，角色细节见 roles/。
     本文件写安排，tasks 写步骤，verification 写执行历史。 -->

## Scope and Contracts

- Specs Revision: `openspec/changes/session-resume/specs/**`（5 个能力增量；`npx --quiet --no-install openspec validate session-resume --strict` 为 valid；基线 sha256 摘要见 verification.md 的 `## Target`）
- Design Revision: `openspec/changes/session-resume/design.md`（D1 wire 合同、D2 v5 持久化形状、D3 端口与用例顺序、D4 后端恢复与门控、D5 资产同步表、D6 封闭词表门禁的 pack 规则）
- Convergence Check: 一致。specs 的 5 个增量需求与 design 的 D1–D6 一一对应；`session.resume` 的命令名/grant/payload/结果与错误码在两处取值相同；无待澄清项。
  **修订记录（DR1 Round 1，2026-09-30）**：DR1 判定 FAIL（4×MAJOR），本文件按 findings 重排工作包与依赖，见 `## Contract Changes` 与 verification.md 的 `## Dependency Declaration Review`。
- Skip Specs: no

## Contract Changes

DR1 Round 1（`reports/dr1-dependency-review.md`，`sha256:77c651a4…c78eaf96`）判 FAIL。**计划的执行安排被修正，行为契约（proposal/specs）未变**：

> **关于「行为契约未变」的可核对性**：DR1 Round 2 指出 chang 目录在 git 中未跟踪、无 diff 基线，因此该断言无法被机械证伪（只能做一致性核对：Coverage Index 37 行的 `source.heading` 与 5 份 specs 的 37 个标题逐条实存且一一对应）。本变更在 tasks 1.5 之后将把 specs 的基线固定为具体提交/摘要，使后续轮次可机械核对。

| Finding | 原值 | 新值 | 影响 |
| --- | --- | --- | --- |
| DR1-F1/F2/F3 | 封闭词表原子点被拆到 WP2（core 镜像）与 WP5（`commands.json` + 文档），两者同处 W1 且依赖均写 `none` | 合并为**单一工作包 WP2**（node-link-protocol + `commands.json` + `crates/core/src/broker.rs` 的 `required_grant` 臂 + `schemas/node-link/v1` + `fixtures/node-link/v1` + `docs/SYNC_PROTOCOL.md` §11.5 + `docs/SECURITY_DESIGN.md` §10.2 + `docs/NODE_LINK_PROTOCOL.md` + `scripts/check-command-catalog.mjs` + 门禁说明四处），WP2 之后才是 core 语义 WP3 | 消除「同批却互为前置」的死锁；不再有分支级 PV2 不可能产出的情况 |
| DR1-F2 | D1 的 `pack: null` 与 `check-command-catalog.mjs` 的 `name !== "session.create"` 豁免冲突 | 新增决策 **D6**：把豁免从「按命令名」改为「按 transport」——`transport` 不含 `sync` 的命令允许 `pack: null`；脚本改动登记进 WP2 写范围，并同步 AGENTS.md §10、README「合同检查」②、`ci.yml` 注释 | 冻结契约在现行门禁下可产出，且**去掉**了 `session.create` 的硬编码特例 |
| DR1-F4 | TP1 声明 `none` 并在 W1 交付可编译用例（其声明的 PV1 不可能通过） | 拆为 **TP1**（W1：仅需求映射 + 稳定用例 ID，不含可编译代码）与 **TP2**（最后一个波次：用例编写且在含实现的基线上做基础检查），TP2 声明 `code:WP1…WP6` | 满足「测试设计并行、依赖实现的执行等待」，不再出现不可产出的检查 |
| DR1-F5 | WP2 Contract Freeze 缺 `D1`；写范围漏 §3.1/§5.2 | WP3 的 Contract Freeze 补 `design.md@D1`；写范围改为 §2、§3.1、§5.1、§5.2 | core 的命令名/grant 取值与提交形状有明确归属 |
| DR1-F6 | WP3 写范围写「§7.3」而版本常量在 §7.2 | WP4 写范围改为「§7 标题、§7.2（版本常量与升级步骤）、§7.3（DDL）」 | 双写者区域说明与登记一致，仍不相交 |
| DR1-F7 | WP4 未声明对 acp-protocol 响应的依赖 | 由新编号 WP5 显式声明 `code:WP1`（`session/resume` 响应类型化 DTO） | 上游变化会触发下游重跑 |
| DR1-F8 | 引用 `reports/scout-target.log`（不存在） | 改为 `reports/scout-target.md` | 门禁按路径取证据时可读 |

**DR1 Round 2（`reports/dr1-dependency-review-round2.md`，`sha256:17460283…619d5c`）**：F1–F8 全部复核为已解决；新报 2×MAJOR + 2×MINOR，同批修如下：

| Finding | 原值 | 新值 | 影响 |
| --- | --- | --- | --- |
| DR1-F9 | 封闭词表原子点漏第七个强制写目标 | `crates/identity-auth/`（`src/authorization.rs` 的 `GRANTS` 会员 + `tests/authorization.rs` 的既有断言）写入 WP2 写范围并在 Shared File Ownership 登记；`docs/IDENTITY_AND_AUTH_CONTRACT.md` 核实为 **no-op**（该文档不列 grant 会员，仅说明镜像存在） | WP2 的 PV1 在其分支上可产出（否则 `cargo test --workspace` 必红） |
| DR1-F10 | D5 冻结的 `layers.broker = native` 不在 `schemas/acp/compatibility-matrix.schema.json` 的枚举内（`check:acp` 用 ajv 强校验，`additionalProperties: false`） | 改为枚举内的合法值 **`project_and_preserve`**（与 `method.session_prompt` 同值；语义为「core 建公共领域视图并保留 raw」，不新增 schema 枚举） | WP1 的 PV2 可产出；`schemas/acp/` 不需要改 |
| DR1-F11 | `docs/NODE_LINK_PROTOCOL.md` 同步清单只列 §12.5/§12.7/§15 | 补 §10 的 grant→命令表（`grant.remote-work` 会员行） | 避免交付后该文档内部自相矛盾（门禁不覆盖此表） |
| DR1-F12 | `plan.md` 声明「TP2 独占测试目录」与 WP 整 crate 写范围/任务 2.4 冲突 | 该行改为逐文件登记：`crates/storage-sqlite/tests/migration.rs`（Writers=WP4,TP2；Merge Owner=TP2；Order=WP4 → TP2；Re-verify=TP2: PV1），并明确 WP1–WP6 的整 crate 写范围含其 `src/**` 内的 `#[cfg(test)]` 单元测试 | 两包仍不同批（W3/W5），登记与实际一致 |

**DR1 Round 3（`reports/dr1-dependency-review-round3.md`，`sha256:5a82f8bb…ba4f4`）**：F10/F11/F12 复核为已解决；F9 的登记项已闭环，但新报 2×MAJOR + 2×MINOR + 1×SUGGESTION，**根因是我的 PV1 写成 workspace 全量、而工作包是按 crate 切分的**。同批修如下：

| Finding | 原值 | 新值 | 影响 |
| --- | --- | --- | --- |
| DR1-F13 | WP2 落地第 13 个 payload 变体后 `crates/server/src/node_link/command.rs` 的 `core_payload()`（**穷尽匹配、无通配臂**，实测 0 个 `_ =>`）编译失败，而该文件属 WP6(W4) → WP2 的 workspace PV1 不可产出 | **PV1 改为阶段制**（同一 Check ID，两种命令形式）：分支阶段只查**本 WP 拥有的 crate 子集**；集成/候选/主分支才跑 workspace 全量。workspace 在 W1→W4 之间的红窗口与其拥有者在 `## Dependency Handoffs` 与 `## Check Plan Changes` 明写；WP6 写范围扩到**整个 `crates/server/`**，由它补齐 `core_payload` 的 `SessionResume` 臂与路由 | WP2 与其余各包的分支检查可产出；workspace 在候选阶段由 WP6 收口 |
| DR1-F14 | WP3 新增两个必需 trait 方法后，四个下游实现点及既有测试字面量被破：`agent-host`(WP5✓)、`server/node_link/command/tests.rs`(WP6✓)、`app/tests/support/owner.rs`(WP6✓)、`core/broker.rs`(WP3✓)，但 `crates/server/src/local_admin/test_support.rs:1021` 的 `NotTouched` **无任何 WP 拥有** → 候选/主分支 PV1 永远不可绿 | 同 F13 的阶段制 PV1 + WP6 写范围扩至整个 `crates/server/`（含 `src/local_admin/test_support.rs`）；**不采用默认实现方案**（端口保持显式必需方法，保留「每个后端都必须作答」的能力诚实边界），而是把涟漪逐点归到具体 WP | 候选与主分支的 workspace PV1 可达全绿；不再有无主文件 |
| DR1-F15 | WP5/WP6 需扩展的既有测试文件既未登记、其「新增文件」又属 TP2 独占 | Shared File Ownership 补登 `crates/agent-host/tests/`（WP5,TP2）与 `crates/app/tests/`（WP6,TP2）两行（含 Merge Owner/Order/Re-verify） | WP5/WP6 的用例落点有据可依 |
| DR1-F16 | `## Contract Changes` 声称「tasks 1.5 之后把 specs 基线固定为提交/摘要」，但 1.5 正文没有该动作 | tasks 1.5 的完成条件补：把 5 份 specs 与 proposal 的 **sha256 内容摘要**写入 `verification.md` 的 `## Target`（change 目录未跟踪，无提交可用）；摘要本身已写入该节 | 后续轮次可机械核对「行为契约未变」（DR1 Round 4 复核为部分解决 → DR1 Round 5 复核：**已解决**） |
| DR1-F17 | `specs/@design-rev-4` 与 `@design-rev-4` 标签不一致 | 统一为 `@design-rev-4` | 仅人读标识，不影响门禁 |

DR1-F1–F4 的 MAJOR 已由上述重排闭环；F5–F8 在同批一并处理。重排后 `workflow check --stage plan` 的 contractDigest 会变化，`## Dependency Declaration Review` 必须由**新的独立 reviewer**在同一 Review ID（DR1）下以新轮次复核。

**DR1 Round 7（W1 交付后由用例审查 CR7 触发，2026-09-30）**：CR1/CR2 判 PASS，CR7 判 **FAIL（2×MAJOR）**；两个 MAJOR 都是**规格文字与已冻结设计相矛盾**（依赖声明审查抓不到，测试设计读到字面才暴露）。本轮修正**行为契约文字**（设计 D3/D4 不变）：

| Finding | 原值（规格文字） | 新值 | 影响 |
| --- | --- | --- | --- |
| CR7-F1 | `specs/local-agent-host/spec.md`：未宣告能力时「MUST NOT **启动 Agent 进程**」 | 「MUST NOT **发送 `session/resume`**」+「MUST 在返回前终止并回收本次为恢复而拉起的子进程」；Scenario 更名为「…且不发送恢复请求」 | 原 MUST 在物理上不可满足（能力只经 `initialize` 得知，而 `initialize` 必先 spawn）。可观察保证一条不少，且变为可满足 |
| CR7-F2 | `specs/workspace-resolution/spec.md`：把「持久化取值为 `NULL`」归入「服务端不可用类错误」 | `NULL` 与「能力不支持」同路径（`nodelink.command.unsupported`）；其余校验失败仍为服务端不可用类 | 消除同一输入两个错误码的分歧（与 D3 对齐）。语义理由：`NULL` = 该会话无恢复数据 = 对本次操作不支持，不是服务端故障 |

同时回写：PV1 阶段 1 证据改为引用各 WP 的报告与其日志（CR1-F1）；`crates/node-link-protocol/tests/` 补登 Shared File Ownership（CR2-F1）；WP5 补 fake ACP agent 的场景与 `--dump-requests` 义务（CR7-F5）；`## Runtime Resources` 补 loopback 监听器一行（CR7-F8）；`docs/SESSION_CONTINUITY_DESIGN.md` 的措辞与状态注记由 WP6 收口（CR1-F2）。修正后的 proposal/specs 摘要基线已刷新在 `verification.md` 的 `## Target`。

**代价：specs 与 plan.md/tasks.md 均被修改 ⇒ contractDigest 变化 ⇒ 必重做依赖声明审查（Round 7）。**

**DR1 Round 9（WP3 实施核对触发的契约订正，2026-09-30）**：coder-C 在开工前实测发现三处**契约级歧义**并主动 BLOCKED 回报（未自行越界解释），经 main 逐条核实其技术主张全部成立后裁定。**行为契约文字订正，specs 的 Requirement 不变**：

| 项 | 原值 | 新值 | 依据 |
| --- | --- | --- | --- |
| D2 两列落盘提交点 | 「`factory.create` 成功返回后、写入 `session.create` 终态的**同一次提交**」——在本架构下**不可实现** | 在 `create_session` 内、`factory.create` 返回后**紧接着提交一次** `StateChange::Update`；`settle_session_create` 签名不变 | 适配层在 settle **之前**已投影 `sessionMeta`（`crates/server/src/node_link/command.rs`），且 `StateChange::Update` 会 bump version → 终态提交会让投影值与落盘值错开，且 `settle_session_create` 拿不到 core 解析的 cwd。改后 **spec R6 字面满足**，版本不自相矛盾，崩溃窗口更稳，WP6 调用点零改动 |
| D3 错误映射归属 | 未指定由谁把「不支持」映射为 `nodelink.command.unsupported` | core 统一返回 `Unavailable(BackendUnsupported)`（新取值、无默认实现）；**WP6** 在 `port_error_code` 加臂映射到既有 wire 码 | core 不吐 wire 码（`InvalidRequest` 漏斗虽有先例但越层）；WP6 已拥有整个 `crates/server/`；不新增任何错误码/feature |
| 恢复数据读取形状 | 未指定 | **窄读取** `SessionStore::load_recovery`（两列不进 `Session`/`SessionSummary`） | 避免把本机规范化路径带进可投影形状，遵守 §3.6 既有边界；代价是 6 个 `SessionStore` 实现点需实现（已逐点归主：WP4 ×1、WP6 ×4、WP3 ×1） |
| WP3 写范围 | §2/§3.1/§5.1/§5.2，**漏了 3 处 core 侧形状** | 增 §3.3 / §3.6 / §4（只加行） | 漏登记会造成文档漂移（`check-contract-drift` 不查这三处，但 AGENTS.md §10 要求同步） |

**代价：本轮修改触及 `design.md` 与 `plan.md` ⇒ `contractDigest` 变化 ⇒ DR1 Round 8 的 PASS 失效，必须由新的独立 reviewer 重审（Round 9）后才允许 WP3 继续写。** coder-C 已收到裁定并被要求「保持存活、暂不落笔」。

**DR1 Round 10（Round 9 的 DR1-F41 及其同批修正，2026-09-30）**：Round 9 判 FAIL（1×MAJOR）。F41 经 main 独立核实**成立**（`settle_session_create` 在 `broker.rs` 硬拒非 `session.create`；`use_cases.rs` 无第二个通用 settle 入口），本轮一并修 F41–F46：

| Finding | 原值 | 新值 |
| --- | --- | --- |
| **F41（MAJOR）** | design D3 步骤 5 把「抬回可交互态**并写终态**」派给 core 用例；但 `SessionResumeResult.remoteSessionRef.exportId` 只有适配层有（Export 可见性是适配器单点策略），core 又无第二个终态入口 ⇒ **无任何包能完成** | `resume_session` 只返回 `SessionId`；**适配层**用与 `session_create_result` 同源的映射投影 `SessionResumeResult`，再经 core 新入口 `settle_session_resume(actor, &RequestId, CommandStatus, Option<CommandResult>, Option<PublicError>) -> bool` 落盘（只终结 `session.resume` 记录）；投影失败按 `session.create` 既有模式结 `uncertain` |
| F42 | 「6 个 `SessionStore` 实现点」/ WP6「三个测试替身」 | **7 处实现 / 6 个文件**；`local_admin/test_support.rs` 含 `NotTouched` 与 `FixedStore` **两个**替身，server 内实为 4 个 |
| F43 | tasks 2.3/2.4/2.6/3.7 未同步 plan 的新义务；2.6「两个必需方法」已失真 | 逐条同步；2.6 改为三个；3.7（CR6）补「能力不支持路径终态必须是 `failed`，`uncertain` 只属崩溃窗口」 |
| F44 | 红窗口段仍写「WP3 新增**两个**必需 trait 方法」 | 改为**三个**（加 `SessionStore::load_recovery`），并补 `storage-sqlite`（新实现）、`FixedStore` 与 `port_error_code` 一条臂 |
| F45 | `SessionRecoveryRecord` 无文档归属；§3.3 与 `session.rs` 注释将失效 | 归入 §3.6（WP3 登记范围）并注明「不进 `Session`/`SessionSummary`」；§3.3 注明 `session.create`/`session.resume` 走专用路径 |
| F46 | R5 正文「在**创建提交**中」比 Scenario 紧；§12.7 示例版本 1；Create→Update 窄窗口 | R5 正文改为「在同一会话创建流程中的提交里」（与 Scenario 一致）；后两项记录在 design D2 的「可观察后果」 |

**代价：触及 `specs` / `design` / `plan` / `tasks` ⇒ 摘要变化 ⇒ Round 9 的 FAIL 结论失效，须由新的独立 reviewer 重审（Round 10）后才允许 WP3 继续落笔。** specs 只改了 R5 的**措辞**（Scenario 与全部 Requirement 的可观察保证不变），需一并刷新 `verification.md` 的 sha256 基线。

**DR1 Round 11（Round 10 的 F47–F51 处置，2026-09-30）**：Round 10 判 **PASS**（0×CRITICAL/0×MAJOR，F41 闭环），另报 4×MINOR + 1×SUGGESTION。处置：

| Finding | 处置 | 触及契约摘要？ |
| --- | --- | --- |
| **F48（MINOR，但会再触发 WP3 BLOCKED）** | design D3 / plan / tasks 对 resume 的 core 侧入口**互相矛盾**（D3 说「返回 `SessionId`」，plan/tasks 却要求加 `CommandPayload` 变体 + 分发，而 `session.create` 先例**没有**变体、`core_payload` 直接早退）。裁定 **Path A**：`resume_session` 完全镜像 `create_session`，**不加变体、不改 `command_name`、不加通用分发臂**，`accepted`/幂等行自建；WP6 的 `core_payload` 对 `SessionResume` **按 `SessionCreate` 先例早退** | ✅ design/plan/tasks |
| F47（MINOR） | `verification.md` 的 `## Target` 对 `specs/local-agent-host/spec.md` 的摘要**确实陈旧**（批量替换未命中）——已按 `sha256sum` 实算值**整块重写** 6 行并更新括注 | ❌ 否 |
| F49（SUGGESTION） | `settle_session_resume` 的幂等语义只写在 plan/tasks；采纳「**只写 §5.1 的 `[决定]`**」，不扩写范围到 §6 | ✅ tasks |
| F50（MINOR） | `AGENTS.md` §10 要求的 `docs/MODULE_ARCHITECTURE.md` §4.1 同步**无写归属** → 已加入 **WP3** 写范围并登记 | ✅ plan |
| F51（MINOR） | `docs/NODE_LINK_PROTOCOL.md` 在 WP2 后**无后续修改权** → 已加入 **WP6** 写范围（仅两处）并登记（Writers=WP2,WP6） | ✅ plan |

**代价：触及 `design`/`plan`/`tasks` ⇒ 摘要变化 ⇒ Round 10 的 PASS 失效，须 Round 11 独立复核后才放行 WP3。**

**DR1 Round 12（Round 11 复核通过后的残留清理，2026-09-30）**：Round 11 判 **PASS**（0×CRITICAL/0×MAJOR；Path A 成立、16 个实现点全部有主、coder-C 不会再触发 BLOCKED），另报 1×MINOR（F52）+ 1×SUGGESTION（F53）并指出 F47/F48 的残留。本轮清掉全部残留：

| Finding | 处置 |
| --- | --- |
| F48 残留 | `## Shared File Ownership` 的 `crates/core/src/broker.rs` 区域注记原写「WP3 改 `CommandPayload` 变体、`command_name`、分发」，与同一份 plan.md 的 WP3 行**直接矛盾** → 已改为「WP3 改 `resume_session`/`settle_session_resume` 用例、端口与错误枚举（**不新增变体、不改 `command_name`/`kind`/`family`、不加通用分发臂**）」 |
| F47 残留 | `verification.md` 的 `## Target` 括注与节标题仍把 CR7 值当当前值 → 已改为三次变化链（含 Round 10 的 R5 措辞 `1c0b1875…`→`99c6eb8e…`）并更新标题 |
| F52（MINOR） | `docs/CORE_PORTS_AND_STORAGE.md` §9 判据 1/28 的版本链与 owned 列清单断言、以及文件头版本记录**无任何写归属**；v5 落地后会与 §7.2 的新升级段自相矛盾，且门禁不覆盖 → 已把 §9 判据 1/28 与文件头纳入 **WP4** 写范围并在区域注记写明（先例：v3→v4 同批改过）；与 WP3 的区域不相交，不新增重叠 |
| F53（SUGGESTION） | `verification.md` 的 CR2-F3 处置栏把位置记为「§10 措辞」，实为 `docs/NODE_LINK_PROTOCOL.md:662` 的 §12.7 结果投影收窄句 → 已订正并补记 F51 的实际处置 |

**代价：触及 `plan.md` 与 `verification.md`（后者不进摘要）⇒ 摘要变化 ⇒ Round 11 的 PASS 失效，须 Round 12 复核后才放行 WP3。**

**DR1 Round 13（WP3 开工实测触发的第二处契约空档，2026-09-30）**：WP3 的修复轮实例在开工前实测发现 `ResumeSessionRequest.workspace: ResolvedWorkspace` 的 `WorkspaceAlias` **在恢复路径上没有权威来源**（`owned_session` 无 workspace/alias 列；`owned_workspace` 可被重指向/删除，而 SR-R26-1 正是测这一点；规格又禁止按别名重解析）。main 核实后裁定**改契约**（而非反查取 alias 或用保留标签——那会写入不成立的断言）：

| 项 | 原值 | 新值 |
| --- | --- | --- |
| `ResumeSessionRequest` 的 workspace 字段 | `workspace: ResolvedWorkspace`（含 alias） | `workspace_cwd: String`（持久化 canonical path 原文；构造约束同 `ResolvedWorkspace::canonical_path`） |
| 受影响面 | design D3 的形状块与步骤 ③；tasks 2.3；`docs/CORE_PORTS_AND_STORAGE.md` §3.6 的该行（WP3 写范围内）；WP5（agent-host）读该字段 | 同上 |

**代价：触及 `design` / `plan` / `tasks` ⇒ 摘要变化 ⇒ Round 12 的 PASS 失效，须 Round 13 独立复核后才允许 WP3 落笔 `resume_session`。** WP3 的其余七项义务（三值对象除字段名、三端口方法、`UnavailableKind`、两列提交点、`settle_session_resume`、文档同步、`session.rs` 注释）不受阻塞，已指令其先行推进。

**DR1 Round 14（W3 交付后的三处义务补登，2026-09-30）**：CR3（PASS，1×MINOR）、CR4（PASS，1×MINOR + 3×SUGGESTION）、CR5（PASS，2×MINOR + 2×SUGGESTION）各自报出**已实现但未被计划登记**的义务，本轮一并补登：

| 来源 | 义务 | 落点 |
| --- | --- | --- |
| CR3-F1（WP3） | `create_session` 在会话行已提交之后失败时，WP6 须结 `uncertain`（不得结 `failed`，否则出现「Access 收到失败、Owner 侧存在孤儿会话」） | plan WP6 行「额外义务（CR3-F1 · `uncertain` 终态）」+ tasks 2.6 同名义务 |
| CR4-F1（WP4） | `docs/CORE_PORTS_AND_STORAGE.md` **§11.3** 的「过新」用例取值随 `FILE_FORMAT_VERSION=5` 由 5 改为 6——CR4 裁定为**必要连带修正**（先例：0.14 的 v3→v4 同批改过），补登记进 WP4 写范围 | plan WP4 行 + Shared File Ownership 区域注记 + tasks 2.4 |
| CR5-F1（WP5） | fake child 需**新增并列选项** `--dump-request-params <path>`（每行 `{method, params}`），**不改** `--dump-requests` 的冻结语义 | plan WP5 行 + tasks 2.5 |

**代价：触及 `plan.md` / `tasks.md` ⇒ 摘要变化 ⇒ Round 13 的 PASS 失效，须 Round 14 独立复核**（Round 14 已判PASS@`sha256:b7650f00…`，报告 `reports/dr1-dependency-review-round14.md`）。**CR5-F1 的后续已完成**：WP5 修复轮交付 `1376e1b`，CR5 Round 2 复核 PASS（`reports/cr5-review-round2.md`），并已入集成基线 `5ab7e9d`。Round 14 报出的 5 项 MINOR（F57–F61）已于 2026-10-01 一并回写（义务标签改用稳定 finding ID、WP6 写范围的章节订正为 §12.7 的 `:662`、本句改为已完成态、补登 `crates/server/tests/` 的 SFO 行、design D5 补齐两行），**因再次触及 `plan.md`/`tasks.md`/`design.md` ⇒ 须 Round 15 独立复核**。CR5-F2（TP1 的 SR-R12-2 前提须改为「进程复用、单一心跳文件」）、CR4-F2/F3/F4、CR5-F3/F4 均为非阻断建议，记入 `verification.md` 与 TP 侧派发提示，不改契约。

## Coverage Index

```agentic-coverage
version: 1
target_ref: refs/heads/main
rows:
  - id: R1
    source: { path: specs/acp-wire-protocol/spec.md, heading: "### Requirement: `session/resume` 的类型化解码与往返保真" }
    tasks: ["2.1", "4.2"]
    checks: [PV1, PV2]
    evidence: [reports/merge-u1-candidate-PV1-stage2-workspace-test.log, reports/merge-u1-candidate-PV2.log]
  - id: R2
    source: { path: specs/acp-wire-protocol/spec.md, heading: "#### Scenario: 正常解码并回写未知字段" }
    tasks: ["2.1", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R3
    source: { path: specs/acp-wire-protocol/spec.md, heading: "#### Scenario: 缺少 required 字段被拒绝" }
    tasks: ["2.1", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R4
    source: { path: specs/acp-wire-protocol/spec.md, heading: "#### Scenario: session/load 仍为显式不支持" }
    tasks: ["2.1", "4.2"]
    checks: [PV1, PV2]
    evidence: [reports/merge-u1-candidate-PV1-stage2-workspace-test.log, reports/merge-u1-candidate-PV2.log]
  - id: R5
    source: { path: specs/local-agent-host/spec.md, heading: "### Requirement: 会话创建时暴露 ACP 会话标识" }
    tasks: ["2.5", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R6
    source: { path: specs/local-agent-host/spec.md, heading: "#### Scenario: 创建成功暴露 ACP 会话标识" }
    tasks: ["2.5", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R7
    source: { path: specs/local-agent-host/spec.md, heading: "#### Scenario: 未取得标识时不编造取值" }
    tasks: ["2.5", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R8
    source: { path: specs/local-agent-host/spec.md, heading: "### Requirement: 进程不在时的会话恢复（`session/resume`）" }
    tasks: ["2.5", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R9
    source: { path: specs/local-agent-host/spec.md, heading: "#### Scenario: 能力已宣告时恢复到可交互" }
    tasks: ["2.5", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R10
    source: { path: specs/local-agent-host/spec.md, heading: "#### Scenario: 能力未宣告时显式不支持且不发送恢复请求" }
    tasks: ["2.5", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R11
    source: { path: specs/local-agent-host/spec.md, heading: "#### Scenario: Agent 拒绝恢复时明确失败" }
    tasks: ["2.5", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R12
    source: { path: specs/local-agent-host/spec.md, heading: "#### Scenario: 反复恢复不产生第二个端点" }
    tasks: ["2.5", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R13
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "### Requirement: 版本常量与 migration 幂等" }
    tasks: ["2.4", "4.2"]
    checks: [PV1, PV2]
    evidence: [reports/merge-u1-candidate-PV1-stage2-workspace-test.log, reports/merge-u1-candidate-PV2.log]
  - id: R14
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: 连续两次打开 schema 文本不变" }
    tasks: ["2.4", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R15
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: 升级中途失败整体回滚" }
    tasks: ["2.4", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R16
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: v4 到 v5 升级保留既有会话行且新列为空" }
    tasks: ["2.4", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R17
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: 新列写入后可读回且不推导" }
    tasks: ["2.3", "2.4", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R18
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: 升级库与新建库的 owned 列清单相等" }
    tasks: ["2.4", "4.2"]
    checks: [PV1, PV2]
    evidence: [reports/merge-u1-candidate-PV1-stage2-workspace-test.log, reports/merge-u1-candidate-PV2.log]
  - id: R19
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: v3 到 v4 升级给既有节点行写空清单" }
    tasks: ["2.4", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R20
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: v2 到 v3 升级保留审计并扩展词表" }
    tasks: ["2.4", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R21
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "### Requirement: 恢复所需列的读写边界" }
    tasks: ["2.4", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R22
    source: { path: specs/storage-schema-v2-migration/spec.md, heading: "#### Scenario: 恢复流程不覆写持久化取值" }
    tasks: ["2.3", "2.4", "2.6", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R23
    source: { path: specs/workspace-resolution/spec.md, heading: "### Requirement: 恢复时的目录复校验" }
    tasks: ["2.3", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R24
    source: { path: specs/workspace-resolution/spec.md, heading: "#### Scenario: 目录已被删除时返回不可用且不启动 Agent" }
    tasks: ["2.3", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R25
    source: { path: specs/workspace-resolution/spec.md, heading: "#### Scenario: 规范化结果变化时拒绝恢复" }
    tasks: ["2.3", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R26
    source: { path: specs/workspace-resolution/spec.md, heading: "#### Scenario: 目录仍然有效时使用持久化取值" }
    tasks: ["2.3", "2.5", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R27
    source: { path: specs/node-link-owner-server/spec.md, heading: "### Requirement: 命令授权、幂等与终态" }
    tasks: ["2.2", "2.3", "2.6", "4.2"]
    checks: [PV1, PV2]
    evidence: [reports/merge-u1-candidate-PV1-stage2-workspace-test.log, reports/merge-u1-candidate-PV2.log]
  - id: R28
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 相同 requestId 重试不重复派发" }
    tasks: ["2.6", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R29
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 同键不同语义被拒绝" }
    tasks: ["2.6", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R30
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 越权命令被拒绝" }
    tasks: ["2.6", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R31
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 越权恢复被拒绝且先于本机读取" }
    tasks: ["2.3", "2.6", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R32
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 崩溃窗口进入 uncertain" }
    tasks: ["2.6", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R33
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 命令限流" }
    tasks: ["2.6", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R34
    source: { path: specs/node-link-owner-server/spec.md, heading: "### Requirement: `session.resume` 的 payload 与结果契约" }
    tasks: ["2.2", "2.6", "4.2"]
    checks: [PV1, PV2]
    evidence: [reports/merge-u1-candidate-PV1-stage2-workspace-test.log, reports/merge-u1-candidate-PV2.log]
  - id: R35
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 正常恢复并回传会话引用" }
    tasks: ["2.5", "2.6", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R36
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: 携带字段被拒绝且不启动进程" }
    tasks: ["2.2", "2.6", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
  - id: R37
    source: { path: specs/node-link-owner-server/spec.md, heading: "#### Scenario: Agent 不支持恢复时终态失败" }
    tasks: ["2.2", "2.3", "2.5", "2.6", "4.2"]
    checks: [PV1]
    evidence: [reports/PV1.log]
```

## Work Packages

| ID | Goal / Scenarios | Dependencies | Contract Freeze | Owner | Role | Reviewer | Branch / Worktree | Write Scope | Inputs / Outputs | Verification |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WP1 | `acp-protocol` 的 `session/resume` 类型化 DTO 与矩阵提升（R1–R4） | none | openspec/changes/session-resume/specs/acp-wire-protocol/spec.md@design-rev-4, openspec/changes/session-resume/design.md@D1, D5 | coder-A | coder | reviewer-A | `<worktree：provisioner 分配并登记，见 verification.md 的 ## Worktree Handoff>` | crates/acp-protocol/, compatibility/acp/v1/matrix.json, fixtures/acp/v1/（含 `manifest.json`；DR1-F24 登记）, docs/ACP_COMPATIBILITY_MATRIX.md | 冻结的 method 名与 required 字段，以及 D5 修正后的 layers 取值（`broker: project_and_preserve`）；产出 DTO、契约测试、矩阵行。若要新增 ACP fixture，必须同时登记 `fixtures/acp/v1/manifest.json`（`check:schema-fixtures` 双向断言）；也可只用内联字节，此时不得新增 fixture 目录内容 | PV1（分支：`-p acp-protocol`）, PV2 |
| WP2 | **封闭词表原子点** + node-link wire（R1–R4 无关；R27/R34/R36） | none | openspec/changes/session-resume/design.md@D1, D6 | coder-B | coder | reviewer-B | `<worktree：provisioner 分配并登记，见 verification.md 的 ## Worktree Handoff>` | crates/node-link-protocol/, crates/identity-auth/, compatibility/commands/v1/commands.json, schemas/node-link/v1/, fixtures/node-link/v1/, docs/NODE_LINK_PROTOCOL.md, docs/SYNC_PROTOCOL.md, docs/SECURITY_DESIGN.md, scripts/check-command-catalog.mjs, AGENTS.md, README.md, .github/workflows/ci.yml, crates/core/src/broker.rs | D1 的命令表 + D6 的 pack 规则；产出命令变体、payload/结果类型、schema、fixture、四份文档与脚本的同步改动；`crates/core/src/broker.rs` **只写 `required_grant` 的一条臂**，`crates/identity-auth/` **只写 `GRANTS` 的会员与其既有验收测试的计数**，`docs/SYNC_PROTOCOL.md` 只改 §11.5、`docs/SECURITY_DESIGN.md` 只改 §10.2、`docs/NODE_LINK_PROTOCOL.md` 只改 §10 grant 表与 §12.5/§12.7/§15、`README.md` 只改「合同检查」②；**不得写 `crates/server/`**（第 13 个 payload 变体会让 server 的 `core_payload` 穷尽匹配失配，该收口属 WP6）；区域与合并安排见 Shared File Ownership | PV1（分支：`-p node-link-protocol -p core -p identity-auth`）, PV2 |
| WP3 | `core` 的恢复语义：值对象、端口、用例与命令路由（R17, R22–R27, R31, R34, R36, R37） | code:WP2 | openspec/changes/session-resume/design.md@D1, D2, D3 | coder-C | coder | reviewer-C | `<worktree：provisioner 分配并登记，见 verification.md 的 ## Worktree Handoff>` | crates/core/, docs/CORE_PORTS_AND_STORAGE.md（§2、§3.1、§3.3、§3.6、§4、§5.1、§5.2）, docs/MODULE_ARCHITECTURE.md（§4.1，DR1-F50） | 冻结的端口签名与用例顺序；产出值对象、端口、用例、命令终态入口与错误映射（**入口形状 DR1-F48：完全镜像 `create_session`——不新增 `CommandPayload::SessionResume` 变体、不改 `command_name`/`command_kind`/`family`、不加通用分发臂，`accepted` 与幂等行由 `resume_session` 自建**）。**新方法不提供默认实现**（保留能力诚实边界），因此会打破下游实现点，由 WP5/WP6 在各自 crate 内补齐（见 `## Dependency Handoffs` 的红窗口段）。**值对象形状（DR1 Round 13）**：`ResumeSessionRequest { agent, agent_session_id, workspace_cwd: String }`——**不含 alias**（恢复路径无权威 alias 来源；理由见 design D3 的 Round 13 订正）。**两列的落盘提交点**：在 `create_session` 内、`factory.create` 返回后紧接着提交一次 `StateChange::Update`（**不是终态提交**，理由见 design D2 的契约订正）。**恢复数据窄读取** `SessionStore::load_recovery`（理由见 design D3 的契约订正），并自行修复 core 内 `FakeStore`。**命令终态入口（DR1 Round 10，F41）**：新增 `settle_session_resume(actor, &RequestId, CommandStatus, Option<CommandResult>, Option<PublicError>) -> bool`，与 `settle_session_create` 同形但**只**终结 `session.resume` 记录；`resume_session` 用例只返回 `SessionId`，**不**负责投影或写终态。文档行：§3.3 的 `CommandPayload` 行**注明 `session.create` 与 `session.resume` 均走专用路径、不在枚举中**（不新增变体）、§3.6 加 `ResumeSessionRequest` 与 `SessionRecoveryRecord` 行、§4 的 `SessionLifecycle` 加 `resume_session` 与 `settle_session_resume` 两个入口（**只加行、不动其它内容**） | PV1（分支：`-p core`）, PV2 |
| WP4 | `storage-sqlite` v5 migration 与恢复列读写（R13–R22） | code:WP3 | openspec/changes/session-resume/design.md@D2, docs/CORE_PORTS_AND_STORAGE.md@§7-v5 | coder-D | coder | reviewer-D | `<worktree：provisioner 分配并登记，见 verification.md 的 ## Worktree Handoff>` | crates/storage-sqlite/, docs/CORE_PORTS_AND_STORAGE.md（§7 标题、§7.2、§7.3、**§9 判据 1/28 的版本链与 owned 列清单断言**，DR1-F52；**§11.3 的「过新」用例取值**（`user_version` 随 `FILE_FORMAT_VERSION=5` 由 5 改为 6——CR4-F1 裁定为必要连带修正，先例：0.14 的 v3→v4 同批改过同一处）；以及按既有惯例在文件头追加版本记录） | core 的 `AgentSessionId` 与提交形状；产出 DDL、migration、读写路径、升级测试；修复本 crate 内因提交形状变化而失配的既有测试字面量。**额外义务（DR1 Round 9）**：因 WP3 新增必需 trait 方法 `SessionStore::load_recovery`，本包的 `SqliteStore` 必须实现它（返回含 `agent` / `agent_session_id` / `workspace_cwd` 的 `SessionRecoveryRecord`，两列缺失时为 `None`） | PV1（分支：`-p storage-sqlite`）, PV2 |
| WP5 | `agent-host` 创建回填与进程不在时的恢复（R5–R12, R26, R35, R37） | code:WP1, code:WP3 | openspec/changes/session-resume/design.md@D4, specs/local-agent-host/spec.md@design-rev-4 | coder-E | coder | reviewer-E | `<worktree：provisioner 分配并登记，见 verification.md 的 ## Worktree Handoff>` | crates/agent-host/ | WP1 的响应 DTO 与 WP3 的端口签名；产出恢复路径、能力门控、fake Agent 测试；并在本 crate 内实现 WP3 新增的两个必需 trait 方法。**额外义务（CR7-F5）**：`crates/agent-host/src/bin/acpr-fake-acp-agent.rs` 必须支持 `session/resume` 分支、`resume-ok`/`resume-error`/`session-new-error` 场景与 `--dump-requests <path>`（每次收到请求追加一行 method）；**并新增并列选项 `--dump-request-params <path>`**（每行一条含 `method` 与 `params` 的 JSON），**不改动 `--dump-requests` 的既有语义**（CR5-F1 方案 (a)：R26/R8 的线级 `cwd` 断言需要可观察通道，而直接扩展 `--dump-requests` 会静默改写已冻结的选项语义）；供 TP2 复用；场景名与选项名一旦确定不得在 WP5 内静默改名 | PV1（分支：`-p agent-host`） |
| WP6 | Owner 侧路由、组合根接线与交付级文档（R22, R27–R37） | code:WP1, code:WP2, code:WP3, code:WP4, code:WP5 | openspec/changes/session-resume/design.md@D1, D3, D5 | coder-F | coder | reviewer-F | `<worktree：provisioner 分配并登记，见 verification.md 的 ## Worktree Handoff>` | crates/server/, crates/app/, docs/SESSION_CONTINUITY_DESIGN.md, README.md（「仓库当前状态」）, docs/DEVELOPMENT_PLAN.md, docs/NODE_LINK_PROTOCOL.md（仅 §12.7 的 `:662` 收窄句与 §12.7 示例注记，DR1-F51） | 上游已验收提交；产出路由与接线，并**负责收口跨 crate 编译涟漪**：`crates/server/src/node_link/command.rs` 的 `core_payload()` 新增 `WirePayload::SessionResume` 臂并**按 `SessionCreate` 先例早退**（不映射到 core payload，消除 WP2 引入的 `E0004`）、`crates/server/src/local_admin/test_support.rs` 的 `NotTouched`/`FixedStore` 新方法、`crates/app/tests/support/owner.rs` 的 `ScriptedBackends` 新方法；以及端到端受控路径测试与状态注记。**额外义务（DR1 Round 9 · `port_error_code` 新臂）**：在 `port_error_code` 新增一条臂，把 `UnavailableKind::BackendUnsupported` 映射为 `nodelink.command.unsupported`；**额外义务（DR1-F42 · 五个 `load_recovery` 替身）**：因 WP3 新增必需 trait 方法 `SessionStore::load_recovery`，本 crate 内的**四个**替身（`src/local_admin/test_support.rs` 的 `NotTouched` 与 `FixedStore`、`src/node_link/command/tests.rs` 的 `CommandStore`、`src/node_link/resource/tests.rs` 的 `SliceStore`）与 `crates/app/tests/support/owner.rs` 的 `FlakySessionStore` 均必须实现它。**额外义务（DR1-F41 · 投影与 `settle_session_resume`）**：用与 `session_create_result` **同源**的映射投影 `SessionResumeResult`（`remoteSessionRef.exportId` 只有适配层有），并在 core 恢复可交互后调用 `settle_session_resume` 落终态；投影失败按 `session.create` 既有模式结 `uncertain`；**额外义务（CR3-F1 · `uncertain` 终态）**：`create_session` 在**会话行已提交之后**失败（如两列提交遇 `StorageFull`/`IoError`）时**不得**结 `failed`（会让 Access 收到失败而 Owner 侧存在一个永不回传的孤儿会话），沿用 `session.create` 既有的「不能用 `failed` 撒谎」判据结 **`uncertain`** | PV1（分支：`-p server -p app`）, PV2 |
| TP1 | 测试设计：需求映射与稳定用例 ID（全部 R 行，**不产出可编译用例**） | none | openspec/changes/session-resume/specs/**@design-rev-4 | tester-A | tester | reviewer-T1 | `<worktree：provisioner 分配并登记，见 verification.md 的 ## Worktree Handoff>` | reports/tp1-*.md（设计报告；不写 crates/ 代码） | 冻结的增量 specs 与 D1–D6；产出逐 R 行的场景/步骤/断言与稳定用例 ID | NOT_APPLICABLE（设计阶段不跑 PV；见 tasks 4.1） | **状态（DR1-F71，2026-10-01 用户决定）**：设计已交付（`reports/tp1-test-design.md`），独立评审 CR7 判 FAIL（2×MAJOR，规格侧已修）；**本包不再单独派修复轮**，其修正与可执行用例**并入 TP2**（见 tasks 2.8）。本行保留在表中以记录该设计工作包的历史与交付，不作为 U1 的就绪条件。 |
| TP2 | 用例编写与基础检查（全部 R 行的可执行用例） | code:WP1, code:WP2, code:WP3, code:WP4, code:WP5, code:WP6 | openspec/changes/session-resume/specs/**@design-rev-4 | tester-A | tester | reviewer-T2 | `<worktree：provisioner 分配并登记，见 verification.md 的 ## Worktree Handoff>` | crates/**/tests/ 的**新增**用例文件、reports/tp2-*.md | TP1 的设计产物 + 上游已验收提交；产出可编译用例与基础检查结果 | PV1（workspace，此时应全绿） |

- Dependency Declaration Review: DR1（独立 reviewer `reviewer-DR`；不代表任何 WP）以 `phase: plan`、`stage: plan` 核实上表 `code:WPn` 声明是否真需要上游**代码**（而非仅冻结契约）、Contract Freeze 路径是否可读且已冻结、`resource:Rn` 是否真不可隔离；结果逐条写入 verification.md 的 `## Dependency Declaration Review`（Reviewer 非 Owner、Review ID + Round、Plan Revision = 当前 contractDigest、报告可读）。开工前门禁；contractDigest 变化后重新派发（DR1 Round 1 已判 FAIL 并触发本轮重排）。

## Execution Waves

| Wave | Work Package | Enter Condition | Serialization Reason |
| --- | --- | --- | --- |
| W1 | WP1 | 契约冻结于 openspec/changes/session-resume/design.md@D1, D5 | NOT_APPLICABLE |
| W1 | WP2 | 契约冻结于 openspec/changes/session-resume/design.md@D1, D6 | NOT_APPLICABLE |
| W1 | TP1 | 契约冻结于 openspec/changes/session-resume/specs/@design-rev-4 | NOT_APPLICABLE | DR1-F71：按用户决定并入 TP2，本行的交付作为 TP2 的输入，不单独开工修复轮。 |
| W2 | WP3 | code:WP2 已进入「已验收集成基线」（含 WP2 交付提交 + **PV1 阶段 1** 与 PV2 PASS + 独立 review PASS + main 已接收） | NOT_APPLICABLE |
| W3 | WP4 | code:WP3 已进入已验收集成基线 | NOT_APPLICABLE |
| W3 | WP5 | code:WP1 与 code:WP3 均已进入已验收集成基线 | NOT_APPLICABLE |
| W4 | WP6 | code:WP1/WP2/WP3/WP4/WP5 均已进入已验收集成基线 | NOT_APPLICABLE |
| W5 | TP2 | code:WP1–WP6 均已进入已验收集成基线 | NOT_APPLICABLE |

> 「已验收集成基线」的完整含义见 W2 行括号内定义：上游交付提交 + **PV1 阶段 1** 与适用的 PV2 PASS + 独立 review 通过 + main 已接收（DR1-F27）。

## Shared File Ownership

| File | Writers (WP) | Merge Owner | Merge Order | Re-verify After Merge | Region Note |
| --- | --- | --- | --- | --- | --- |
| crates/core/src/broker.rs | WP2, WP3 | WP3 | WP2 → WP3 | WP3: PV1, PV2 | WP2 只改 `required_grant` 的一条臂（`"session.resume" => "grant.remote-work"`，位于 `_ => return None` 之前）；WP3 改 `resume_session`/`settle_session_resume` 用例、端口与错误枚举（**不新增 `CommandPayload` 变体、不改 `command_name`/`command_kind`/`family`、不加通用分发臂**；`accepted`/幂等行由 `resume_session` 自建），互不重叠。两包分处 W1/W2，实际不同时写 |
| crates/identity-auth/src/authorization.rs | WP2 | WP2 | WP2 | PV1, PV2 | 单一写者：只把 `session.resume` 加进 `GRANTS` 的 `grant.remote-work` 会员 |
| crates/identity-auth/tests/authorization.rs | WP2 | WP2 | WP2 | PV1 | 单一写者：既有验收测试（`scopes.len()` 计数与会员断言）随会员变化同步；该文件属既有文件修改，不归 TP2 |
| crates/storage-sqlite/tests/ | WP4, TP2 | TP2 | WP4 → TP2 | TP2: PV1 | 既有集成测试目录（含 `migration.rs` 的「连续两次打开字节不变」幂等基准、`commit.rs`/`retention.rs`/`enum_coverage.rs` 等含 `OwnedCommit{…}`/`NewSession{…}` 全字段字面量的文件）。WP4 会因提交形状变化逐处修复这些字面量；TP2 在其后新增/扩展用例。两包分处 W3/W5，不并发 |
| crates/node-link-protocol/tests/ | WP2, TP2 | TP2 | WP2 → TP2 | TP2: PV1 | 既有测试目录（`schema_drift.rs`、`envelope_fixtures.rs` 等）。CR2-F1：WP2 因新增 fixture 必须同步 `envelope_fixtures.rs` 的两个硬编码计数常量（38→40、9→10），未增删用例、未弱化断言；TP2 只新增文件。两包分处 W1/W5，不并发 |
| docs/CORE_PORTS_AND_STORAGE.md | WP3, WP4 | WP4 | WP3 → WP4 | WP4: PV2 | WP3 改 §2（错误枚举）、§3.1（新值对象）、§3.3（`CommandPayload` 枚举）、§3.6（`ResumeSessionRequest` 与 `SessionRecoveryRecord` 行）、§4（`SessionLifecycle` 的 `resume_session` 与 `settle_session_resume`）、§5.1（端口签名）、§5.2（提交形状与窄读取）；WP4 改 §7 标题、§7.2（版本常量与升级步骤）、§7.3（DDL）、**§9 判据 1/28**（版本链 `v1→…→v5` 与 owned 列清单断言；先例：v3→v4 同批改过）、**§11.3 的「过新」取值**（CR4-F1）与文件头版本记录。§9 与 WP3 的 §2/§3.x/§4/§5.x 不相交，不新增重叠。区域不相交；两包分处 W2/W3，实际不同时写 |
| docs/SYNC_PROTOCOL.md | WP2 | WP2 | WP2 | PV2 | 单一写者，只改 §11.5 命令表与相邻计数文本 |
| docs/SECURITY_DESIGN.md | WP2 | WP2 | WP2 | PV2 | 单一写者，只改 §10.2 命令、scope、pack 与 grant 表格 |
| AGENTS.md | WP2 | WP2 | WP2 | PV2 | 单一写者，只改 §10 的封闭词表门禁说明句 |
| scripts/check-command-catalog.mjs | WP2 | WP2 | WP2 | PV2 | 单一写者 |
| .github/workflows/ci.yml | WP2 | WP2 | WP2 | PV2 | 单一写者，只改 `npm run check` 注释 |
| README.md | WP2, WP6 | WP6 | WP2 → WP6 | WP6: PV2 | WP2 改「合同检查」②（封闭词表门禁的 pack 规则措辞）；WP6 改「仓库当前状态」表。区域不相交 |
| compatibility/acp/v1/matrix.json | WP1 | WP1 | WP1 | PV2 | 单一写者 |
| compatibility/commands/v1/commands.json | WP2 | WP2 | WP2 | PV2 | 单一写者 |
| schemas/node-link/v1/、fixtures/node-link/v1/ | WP2 | WP2 | WP2 | PV2 | 单一写者 |
| crates/acp-protocol/src/lib.rs | WP1 | WP1 | WP1 | PV1 | 单一写者 |
| crates/core/src/lib.rs | WP3 | WP3 | WP3 | PV1 | 单一写者 |
| docs/MODULE_ARCHITECTURE.md | WP3 | WP3 | WP3 | PV2 | DR1-F50：`AGENTS.md` §10 要求 core 端口签名/值对象变化时更新 `docs/CORE_PORTS_AND_STORAGE.md` 并同步 `docs/MODULE_ARCHITECTURE.md` §4.1，此前无 WP 拥有该文件。区域=§4.1 的会话后端端口行（补 `resume`/`agent_session_id`）、`SessionStore` 行（补 `load_recovery`）、值对象清单（加 3 项）。单一写者 |
| docs/NODE_LINK_PROTOCOL.md | WP2, WP6 | WP6 | WP2 → WP6 | WP6: PV2 | DR1-F51：WP2 交付后该文件后续修改权原为无主（CR2-F3 已登记）。WP6 只补两处：:662 收窄句加 `session.resume`、§12.7 示例加「可见版本为 2」的非规范注记。两包分处 W1/W4，不并发 |
| crates/**/tests/ 的新增用例文件 | TP2 | TP2 | TP2 | PV1 | TP2 独占**新增**测试文件；WP1–WP6 的写范围含各自 `src/**`（含内联 `#[cfg(test)]` 单元测试）与上表逐文件登记的既有测试目录/文件 |
| crates/agent-host/tests/ | WP5, TP2 | TP2 | WP5 → TP2 | TP2: PV1 | 既有集成测试目录（如 `session.rs`）。WP5 为覆盖 R5–R12 会在此扩展；TP2 在其后新增用例。两包分处 W3/W5，不并发 |
| crates/app/tests/ | WP6, TP2 | TP2 | WP6 → TP2 | TP2: PV1 | 既有端到端受控路径测试目录（如 `node_link_e2e.rs`、`support/owner.rs`）。WP6 补 `session.resume` 的受控路径与 `ScriptedBackends` 新方法；TP2 在其后新增用例。两包分处 W4/W5，不并发 |
| crates/server/tests/ | WP6, TP2 | TP2 | WP6 → TP2 | TP2: PV1 | **DR1-F61 补登**：既有集成测试目录（`local_admin_channel.rs`、`local_admin_schema_drift.rs`、`local_endpoint_naming.rs`、`local_endpoint_unix.rs`、`local_endpoint_windows.rs`）。与 `crates/app/tests/` 同为 WP6↔TP2 的交叠面，两包分处 W4/W5、不并发，故不构成同批重叠 |

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| WP3 | WP2 | 规划时写验收条件：WP2 交付提交 + PV1/PV2 PASS + 独立 review PASS；接入前引用 verification 的已验收提交 | 从包含 WP2 交付提交的集成基线开工 | 合入前核对 `git merge-base --is-ancestor <WP2 提交> <WP3 候选>` | WP2 改变命令名/grant/pack 取值或 `required_grant` 臂 → WP3/WP5/WP6/TP2 重跑 PV1/PV2 与受影响用例 |
| WP4 | WP3 | 同上（WP3 交付提交 + PASS + review） | 从包含 WP3 交付提交的集成基线开工 | 同上 | WP3 变更 core 端口、值对象或提交形状 → WP4/WP5/WP6/TP2 重跑 |
| WP5 | WP1, WP3 | 同上（WP1 与 WP3 各自交付提交 + PASS + review） | 从包含两者交付提交的集成基线开工 | 逐条核对祖先关系 | WP1 变更 `session/resume` 响应 DTO → WP5/WP6/TP2 重跑；WP3 变更端口 → WP5/WP6/TP2 重跑 |
| WP6 | WP1, WP2, WP3, WP4, WP5 | 同上（五者各自交付提交 + PASS + review） | 从包含五者交付提交的集成基线开工 | 逐条核对祖先关系 | 任一上游变化 → WP6/TP2 重跑 PV1/PV2 与受控路径用例 |
| TP2 | WP1, WP2, WP3, WP4, WP5, WP6 | 同上（六者各自交付提交 + PASS + review） | 从包含六者交付提交的集成基线开工 | 逐条核对祖先关系 | 任一上游变化 → TP2 重跑 PV1 与受影响用例 |

**跨 crate 编译涟漪与预期红窗口（DR1-F13/F14/F44，已登记）**：WP2 落地第 13 个 wire payload 变体后，`crates/server/src/node_link/command.rs` 的 `core_payload()`（穷尽匹配、无通配臂）会失配；**WP3 新增三个必需 trait 方法**（`SessionBackendFactory::resume`、`SessionEndpoint::agent_session_id`、`SessionStore::load_recovery`）后，`crates/agent-host/`（WP5）、`crates/storage-sqlite/src/session_store.rs` 的**新实现**（WP4）、server 侧的**四个**替身（`NotTouched`、`FixedStore`、`CommandStore`、`SliceStore`）与 `crates/app/tests/support/owner.rs` 的 `FlakySessionStore`（WP6）会失配；WP6 另需在 `port_error_code` 加一条臂（`UnavailableKind::BackendUnsupported` → `nodelink.command.unsupported`）。收口责任人不变：**WP4 / WP5 / WP6**。因此：

- 从 WP2 合入集成基线起、到 WP6 合入为止，**workspace 级 PV1 预期为红**；这是已知、有主、有界的窗口，**不是失败**。各 WP 在分支上的 PV1 只查自己拥有的 crate（见 PV1 的阶段定义）。
- 收口责任人：WP6（写范围已扩到整个 `crates/server/`，并拥有 `crates/app/`）负责补齐 `core_payload` 的 `SessionResume` 臂、`NotTouched` 与 `ScriptedBackends` 的新方法，以及受控路径与 `session.resume` 的路由；WP5 负责 `crates/agent-host/` 内的两个方法实现；WP4 负责 `crates/storage-sqlite/` 内失配的测试字面量。
- **候选阶段的 workspace PV1（阶段 2）必须全绿**，否则不得合入（见 `## Completion Criteria`）。上表每个下游行的 Invalidation 列已覆盖该窗口内的重跑范围。
- **红窗口内的提交钩子（DR1-F23）**：`.husky/pre-commit` → `scripts/pre-commit.mjs` 在涉及 Rust 的提交上跑 **workspace 全量** `cargo clippy -D warnings`，因此红窗口内 WP3–WP6 的 Rust 提交会被钩子拒绝。按 `AGENTS.md` §8，钩子是「更早发现失败」而非门禁本体，此时可用 `git commit --no-verify`；这不改变 PV1 阶段 1/阶段 2 的判定责任，也不得成为跳过其它钩子检查的借口（非 Rust 部分仍须为绿）。

## Runtime Resources

| Checks / Work Packages | Resource | Isolation / Configuration | Exclusive Scheduling | Allocate / Isolate / Start / Ready / Cleanup | Use / Cleanup Boundary |
| --- | --- | --- | --- | --- | --- |
| WP1–WP6, TP1, TP2, PV1 | Rust 构建缓存与测试临时目录 | 每个 worktree 使用独立 `CARGO_TARGET_DIR`；测试用各自 tempdir，不共享数据目录 | NOT_APPLICABLE | provisioner 为每个 (WP, Attempt) 指定 `CARGO_TARGET_DIR` 并写入 worktree 交接 | 执行者只在自己的 target 与 tempdir 内构建与清理 |
| WP4, WP6, TP2, PV1 | 本地 SQLite 数据库文件 | 测试与受控路径各自创建独立临时库文件，不复用仓库内数据目录 | NOT_APPLICABLE | provisioner 只确认无仓库内共享库文件被占用 | 执行者只清理自己创建的库文件 |
| WP1, WP5, WP6, TP2, PV1 | 本地 loopback TCP 监听器 | 每个用例用 `127.0.0.1:0` 由内核分配临时端口，各 worktree/进程互不共享，因此无互斥需求（CR7-F8） | NOT_APPLICABLE | 由用例自行 `bind("127.0.0.1:0")` 并在结束时间放；不占用固定端口，不需 provisioner 分配 | 执行者只使用自己进程内绑定的监听器 |
| WP1, WP2, WP4, WP6, TP2, PV1, PV2 | `npm run check` 的合同门禁脚本 | 只读仓库文件，无状态；须在已合入全部相关资产的候选/主分支上运行 | NOT_APPLICABLE | 由检查执行者运行，不需 provisioner 分配 | 无持久资源 |

## Target Repository and Main Branch

- Code Repository: `D:\Project\acp-remote`
- Target Ref: `refs/heads/main`（scout 于 tasks 1.1 核实：`81e350ff340014265eb7c9251237c799d4357fee`；候选构建与合入瞬间由 merger 复核）
- Version Confirmation Owner: 规划期只读调查 = scout；候选/合入瞬间 = merger；目标选择 = main
- Confirmation Method / Evidence: `git rev-parse refs/heads/main` 与 `git status --porcelain` 的输出记录在 verification.md 的 `## Target` 与 `reports/scout-target.md`

## Merge Strategy

| Delivery Unit / Mode | WP / TP | Owners: Implementation / Test / Review / Merge | Start / Readiness | Candidate Checks / Critical E2E | Order |
| --- | --- | --- | --- | --- | --- |
| U1 / integrated | WP1, WP2, WP3, WP4, WP5, WP6, TP2 | 实现：coder-A…coder-F；测试：tester-A（TP1/TP2）；检视：reviewer-A…F、reviewer-T1、reviewer-T2；合入：merger-A | 契约冻结（design@D1–D4/D6）+ W1 工作包可开工；WP3 等 WP2；WP4 等 WP3；WP5 等 WP1+WP3；WP6 等五上游；TP2 等六上游 | PV1、PV2；关键路径：`session.resume` 的授权/幂等/终态与「能力未宣告即显式不支持」 | WP1, WP2 → WP3 → WP4, WP5 → WP6 → TP2 | **DR1-F71**：TP1 的测试设计并入 TP2，故不在本单元的就绪名单内（其台账终态为 `fixing`，不作为合入就绪条件）。 |

- Merge Worktree: 由 provisioner 创建并交接的单元级执行 worktree（`wt/u1` / `D:\Project\acp-remote-wt\session-resume-du1`），main 校验交接后写入 verification.md 的 `## Worktree Handoff`；merger 不新建
- Source Revision / Handoff: 各 WP 的固定交付提交；候选基于当时最新的 `refs/heads/main`
- Local Merge Conditions / Report Path: 候选 Project Verify（PV1/PV2）PASS + 独立 review PASS + Coverage Index 覆盖核对通过 + `agentic-premerge` 块 PASS；握手报告 reports/merge-u1.md

- 说明：本变更所有 crate 之间存在编译期依赖（core 端口 → storage/agent-host → server），且「未宣告能力即显式不支持」的验收必须看到整条链路，因此选择 **integrated** 单一交付单元；集成基线在 W2 开工前形成，不等待全部工作包完成。

## Verification Strategy

### Local Checks

各 WP Owner 在提交前运行与改动范围对应的 `cargo fmt`、`cargo clippy -p <crate>` 与 `cargo test -p <crate>`（使用本 WP 的 `CARGO_TARGET_DIR`）。TP2 运行其用例所在 crate 的测试。局部自检不能代替 PV1 的**阶段 2**（workspace 全量）与 PV2。

### Project Verify

| Check ID | Stages / Work Packages | Command / Working Directory | Configuration / Environment | Scope / Pass Criteria | Evidence |
| --- | --- | --- | --- | --- | --- |
| PV1 | 分支（阶段 1，按 WP 的 crate 子集）/ 集成基线（阶段 2，全部 WP 集成后、候选前）/ 候选 / 主分支；全部 WP/TP | 阶段 1（分支，各 WP 交付前）：`cargo fmt --all -- --check` 后按写范围选 `cargo clippy --locked -p <本 WP crate 列表> --all-targets --all-features -- -D warnings && cargo test --locked -p <本 WP crate 列表> --all-features`；各 WP 的 crate 列表见 Work Packages 的 Verification 列。阶段 2（**全部 WP 集成后的集成基线**、候选、主分支）：`npm run check:rust`（= `cargo fmt --all -- --check && cargo clippy --locked --workspace --all-targets --all-features -- -D warnings && cargo test --locked --workspace --all-features`，仓库根；见 `AGENTS.md` §8） | Rust 工具链由 `rust-toolchain.toml` 固定；独立 `CARGO_TARGET_DIR` | 阶段 1：本 WP 拥有的 crate 编译与测试全绿（开发期子集，**不替代**阶段 2）；阶段 2：全 workspace 编译与测试全绿。任一子检查失败、零测试或全跳过即 FAIL | reports/<wp>-coder.md / <wp>-tester.md 及其引用的日志文件（阶段 1）与 reports/PV1.log（阶段 2），含每条子命令的版本、退出码与输出（CR1-F1：不再声明不存在的 `reports/PV1-<WP>.log` 路径） |
| PV2 | 分支（阶段 1）/ 集成基线（阶段 2）/ 候选 / 主分支；WP1、WP2、WP3、WP4、WP6 | `npm run check`（仓库根） | Node ≥ 22.12；合同门禁（含 `check:command-catalog`、`check:contract-drift`、`check:boundaries`、`check:acp`、`check:docs`、`check:agentic`） | 全部门禁 PASS；封闭词表四处一致、合同漂移、依赖方向、固定向量与文档引用无漂移。PV2 不受红窗口影响：每处合同改动与它的镜像/文档同包同提交 | reports/PV2.log |

### Code Review

- 每个 WP 由非其 Owner 的独立 reviewer 在 `fork_turns="none"` 的隔离上下文中只读检视固定提交的 diff 与契约（`roles/reviewer.md`），逐 WP 记录 Review ID、版本与报告；CRITICAL/MAJOR 必须有 Resolution。
- 关注点：授权是否真的先于本机读取与文件系统访问；能力门控是否在发送 `session/resume` 之前且失败无副作用；`agent_session_id`/`workspace_cwd` 是否只写一次且恢复只读；`NULL` 是否被误当作可恢复；DDL 追加是否未触发 12-step 重建；封闭词表四处是否同批一致、`pack: null` 是否只落在 node_link-only 命令上。
- 合入前的候选 review 只审 rebase 新产生的冲突解决与新增交互；干净 rebase 且改动内容指纹未变时可复用原结论，记录 `fingerprint` 与 `reused_from`。

### Main E2E

```yaml
mode: not-applicable
reason: "本切片没有可端到端运行的产品路径。B（真恢复）的远程触发面在 Access 侧，而 node-link-client 与 server::acp_facade 尚未落地、前端工程未开始；Owner 侧只能以受控路径的集成用例验证，不构成真实入口的端到端场景。"
basis: "openspec/agentic.yaml 的 e2e.command 为空（当前阶段无可用 E2E 入口）；docs/DEVELOPMENT_PLAN.md §2 记录切片 6/7 尚未交付；项目画像 openspec/config.yaml 已就本阶段约定按变更记 not-applicable 并逐变更记录批准。"
alternative_checks: [C1, C2]
downgrade_approval: "用户 2026-09-30 （本会话，Asia/Shanghai）原话：「同意 session-resume 变更的 E2E 记 not-applicable，替代验证用 cargo test 全量 + npm run check」（来源：本会话用户消息，2026-09-30）；仅供本变更使用，不代表其它变更"
```

替代检查：`C1` = PV1（`cargo test --locked --workspace --all-features`，覆盖改动 crate 的单元与集成用例，含 fake ACP Agent 驱动的恢复路径）；`C2` = PV2（`npm run check`，覆盖合同漂移、依赖方向、封闭词表与 ACP 固定向量）。

## Independent Validation

| Task | Target Revision | Assignment | Pass Condition | Report Path |
| --- | --- | --- | --- | --- |
| 7.1 | 全部交付单元合入后的 `refs/heads/main` 固定提交（apply 时写入） | 以 `roles/validator.md` 独立验证：覆盖充分性（Coverage Index 的 37 行是否都有可读证据）、待验证假设（目标 Agent 是否宣告并实现 `sessionCapabilities.resume`；无真实 Agent 时应记 BLOCKED 而非 PASS）、迁移与能力门控的风险盲区 | 结论为 PASS：需求覆盖有独立证据，未验证假设被明确标注为 BLOCKED 或提供只读侦察证据，未发现阻断项 | reports/validation-session-resume.md |

## E2E Execution Plan（apply 阶段生成，执行前冻结）

不适用（Main E2E mode = not-applicable）。

## Failure and Recovery

- 失败修复由原 WP 的新实现实例承担（`--reopen --reason`），重做上限 3 轮；修复后必须由新的独立 reviewer 复核，并重跑受影响的 PV1/PV2 与用例。
- 上游（WP1/WP2/WP3/WP4/WP5）在某下游集成后发生变化时，该下游与受影响用例重开；纯环境恢复（例如 `CARGO_TARGET_DIR` 被污染）需留就绪证据并重跑受影响检查。
- 回滚：本次不做数据库降级工具；代码回滚会使 v5 库被「过新版本拒绝打开」拦住，必须在交付说明中明确该限制，且回滚需用户单独授权。
- 连续 E2E 失败达 `e2e.maxAttempts` 时停止自动重跑并交用户决策（本变更为 not-applicable，不适用该路径，但保留条款）。

## Completion Criteria

- 最终主分支固定提交上：**PV1（阶段 2：workspace 全量）** 与 PV2 全绿且证据可读（reports/PV1.log、reports/PV2.log）；各 WP 分支的 PV1（阶段 1：按 crate 子集）证据在各 WP 的报告及其引用的日志中（CR1-F1/DR1-F32：不再声明不存在的 `reports/PV1-<WP>.log`），仅作开发期证据，不替代阶段 2。
- verification.md 的 `## Dispatch Reconciliation` 逐工作包与台账一致且状态为 merged；`## Dependency Declaration Review`、`## Worktree Handoff`、`## Premerge History`、`## Merge History`、`## Review Findings`、`## Checks` 均按模板逐行填写且无 CRITICAL/MAJOR 未闭环项。
- Coverage Index 的 37 行全部有实现与检查证据；`[validation]`（7.1）结论为 PASS（或对未验证假设给出 BLOCKED 并说明）。
- Main E2E 记 NOT_APPLICABLE，且 reason/basis/alternative_checks 齐备、C1/C2 在 `## Checks` 中有 PASS 行、`downgrade_approval` 含用户当次批准原话/时间/来源。
- `verification.md` 的 `## Target` 已记录 5 份 specs 与 proposal 的 sha256 基线摘要（tasks 1.5），供后续轮次机械核对「行为契约未变」。
- 通过 `.agents/skills/agentic-verify/SKILL.md` 的最终验收并运行 `openspec-agentic workflow check --change session-resume --stage final --json` 为 PASS。
