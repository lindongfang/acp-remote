# DR1 Round 8 复核报告（Review Type: plan，plan 阶段）
> 本报告由第八个独立 reviewer 实例产出（新实例、只读、无 write 工具，未参与前七轮、未参与任何实现或用例设计）。main 请把本报告原样持久化为 `openspec/changes/session-resume/reports/dr1-dependency-review-round8.md`，再回填 `verification.md:58`（详见文末「给 main 的回填指引」）。
## Shared Report
| 字段 | 取值 |
| --- | --- |
| task_id | NOT_APPLICABLE（规划门禁，非 tasks 行） |
| role / phase / stage | reviewer / plan / plan |
| round | 8 |
| agent_context | 第八个新实例；只读、无 write 工具；未继承实现对话（fork_turns=none）；与前七个 DR1 实例均不同实例；非任何 WP 的 Owner |
| target_revision | `sha256:57b47d8794f92e845f779bc40a091d74020e8b71eb0c2eafe79ee6913ccc5ee5`（本轮 contractDigest，由派发给出；我无 shell/引擎能力，未自行复算） |
| scope | 契约集合（`proposal.md` + 5 份增量 specs + `design.md`）的可满足性与三方一致性；`plan.md` 的依赖/批次/资源/写入归属与 Coverage Index；`verification.md` 的记录层一致性（Target / Handoff Index / DCR / Checks / Review Findings）；DR1-F31…F35 的逐项复核 |
| changes | 无（只读检视，未修改仓库任何文件） |
| checks | 规划阶段无 PV/E2E 可跑（TP1/TP2 的 PV 与本轮无关）：本轮核对的是 plan.md 的 `## Coverage Index`（37 行）、`## Verification Strategy`、`### Main E2E` 决策与三个门禁标记，以及 `verification.md` 的 `## Target` / `## Handoff Index` / `## Dependency Declaration Review` |
| issues | 0×CRITICAL、0×MAJOR、4×MINOR、1×SUGGESTION（全部为 DR1-F36…F40，非阻断） |
| result | **PASS** |
| evidence_paths | 本报告（待 main 持久化）；`openspec/changes/session-resume/reports/dr1-dependency-review-round6.md`（格式基线）、`…-round4.md`、`…-round5.md`（历史轮次，均可读） |
| resource_cleanup | NOT_APPLICABLE（未占用任何构建/端口/数据库资源；只读文件） |
## Review Context
| Review ID + Round | Review Type / Stage | Work Package | Repository | Base / Target Revision | 读取的规则与需求 | 实际检查范围 | 验证证据与限制 | 报告路径 | 核对的 Check / E2E ID |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| DR1 / Round 8 | plan / plan | NOT_APPLICABLE | `D:/Project/acp-remote` | Base = NOT_APPLICABLE；Target = `sha256:57b47d87…5ee5`（当前 contractDigest） | `openspec/schemas/agentic/roles/reviewer.md`、`roles/_shared/role-report.md`、`AGENTS.md` §1/§3/§5/§10、`openspec/schemas/agentic/procedures/workflow-check.md`（plan 阶段判据与 contractDigest 覆盖范围） | `proposal.md`（含 `agentic-intent` 全文）、5 份 `specs/**/spec.md`（全文）、`design.md`（全文）、`plan.md`（全文）、`tasks.md`（全文）、`verification.md`（全文）、`docs/SESSION_CONTINUITY_DESIGN.md`（§4/§8 D4/§10/§12 相关段）、`reports/cr7-review.md`、`reports/tp1-test-design.md`（相关行）、`reports/wp1-coder.md`、`reports/wp2-coder-PV1.log`、`schemas/node-link/v1/command.schema.json`（terminal status 枚举） | 无执行证据（规划审查不得冒称已执行测试）。**限制**：① 无 shell/引擎能力 → 不能复算 `contractDigest` 与 6 行 sha256，只能做文档内自洽核对；② `reports/dr1-dependency-review-round7.md` **在仓库中不可读**（见 DR1-F37）→ 第 7 轮原文无法核对，F31–F35 的复核依据是当前契约文本 + `verification.md` 的转述行；③ 不审代码 diff、不判用例集合齐备性（属 main 的 Coverage Index 与 validator） | 本报告（`reports/dr1-dependency-review-round8.md`，待 main 落盘） | 计划内：Coverage Index 37 行、PV1/PV2、C1/C2（本轮仅核对登记与可产出性，不核对执行结果）；E2E = not-applicable（本轮只核对决策完整性与替代检查登记） |
工作区事实（独立复核）：`watchdog_diff` 显示 `openspec/changes/session-resume/**` 整目录为未跟踪、无被跟踪文件改动，与 `verification.md:10` 的记录一致；该清单中同样**没有** `dr1-dependency-review-round7.md`。
---
## 1. DR1-F31 复核（本轮重点）
**结论：已解决。** 我逐条读了 `proposal.md` 的 `agentic-intent` 四块：
| 块 | 行 | 内容判定 |
| --- | --- | --- |
| `constraints`（6 条） | 26–32 | 第 3 条（:29）现为「未宣告能力的 Agent MUST 显式返回不支持，MUST NOT 静默降级为新建会话（C），也 MUST NOT 发送 `session/resume`，并 MUST 在返回不支持错误前终止并回收本次为恢复而拉起的子进程（DR1-F31：原措辞「MUST NOT 为此启动 Agent 进程」…物理不可满足）」——**现行要求可满足**；旧措辞以「原措辞…物理不可满足」**显式标注为被取代的历史引述**，不构成现行 MUST。其余 5 条（触发通道 B2、授权先于本机读取、必须持久化真实 cwd/标识、最高优先级不变量、依赖方向）均无「不启动进程」类要求 |
| `success_criteria`（5 条） | 40–45 | 第 2 条（:42）「返回明确的不支持错误、未发送 `session/resume`、本次拉起的子进程已回收，会话状态不变，且不降级为新建会话」——**可判真**（且对「根本没拉起进程」的实现也成立，故无空集风险）。第 1/3/4/5 条分别映射 R35+R26、R28+R32、R23–R25、R13–R20，均可判 |
| `assumptions`（2 条） | 49–51 | 第 2 条（:51）已把旧措辞「返回不可用」改为「显式返回不支持（`nodelink.command.unsupported`）」，与 CR7-F2 的规格修正一致 |
| `non_goals`（6 条） | 33–39 | 无该类措辞（不含任何副作用/进程约束声明） |
**三方一致性（逐字核对）：**
| 载体 | 位置 | 现行措辞 |
| --- | --- | --- |
| `proposal.md` | :29 / :42 | MUST NOT 发送 `session/resume` + MUST 在返回不支持错误前终止并回收本次拉起的子进程；MUST NOT 静默降级为新建会话 |
| `specs/local-agent-host/spec.md` | :21 / :23（Requirement 正文）/ :28 / :33（R10 Scenario） | 「能力宣告只能经 `initialize` 协商获得，而 `initialize` 必然已经拉起进程，因此『未宣告』时的可观察保证是：MUST NOT 发送 `session/resume`，且 MUST 在返回明确的不支持错误之前终止并回收本次为恢复而拉起的子进程（不得残留进程、不得留下会话绑定），MUST NOT 改用新建会话代替，也 MUST NOT 伪造成功」 |
| `design.md` | :113–122（D4，尤其 :117 门控在 spawn 之后、`session/resume` 之前；:120 明写「代价：一次恢复失败会短暂启动并清理一个子进程」；:122 CR7-F1 修正注记）/ :48（D1 冻结表：能力不支持 → `command.terminal` 失败，`nodelink.command.unsupported`）/ :103–105（D3 错误映射，含「不降级为新建会话」） | 同序、同语义 |
三者在「不发送 `session/resume`」「终止并回收本次拉起的子进程」「不降级」「不伪造成功」四点上一一对应，且**在 D4 的执行顺序下都可满足**（拉起 → `initialize` → 门控 → 关闭 → 返回不支持）。上游来源文档 `docs/SESSION_CONTINUITY_DESIGN.md` §8 D4（:222）只写「必须显式返回不支持、不得虚报、不得静默降级」，**没有** 「不得启动进程」的说法，因此不构成第四处冲突源。
**成功判据的可判真性（7.1 validator）：** 5 条判据都可判（分别落在 R13–R20 / R23–R25 / R28+R32 / R35+R26 / R37 上）。唯一附带条件是判据 1 依赖未验证假设（目标 Agent 是否真宣告并实现 `resume`）；`plan.md` 的 `## Independent Validation` 与 `proposal.md:50` 已明确要求「无真实 Agent 时记 BLOCKED 而非 PASS」，因此该条属「可判真但可能判 BLOCKED」，不是不可判。
## 2. 全量残留扫描（契约集合：`proposal.md` + 5 份 specs + `design.md`）
检索变体：`启动.{0,6}进程`、`不启动`、`spawn`、`拉起`、`不产生副作用`、`无副作用`。逐处判定：
| # | 位置 | 原文（节选） | 判定 |
| --- | --- | --- | --- |
| 1 | `proposal.md:29` | 「…MUST NOT 发送 `session/resume`…（DR1-F31：原措辞「MUST NOT 为此启动 Agent 进程」…物理不可满足）」 | **现行要求（可满足）+ 明确标注的历史引述**，合格 |
| 2 | `specs/node-link-owner-server/spec.md:29`（R31 越权场景） | 「不读取该会话行、不触碰文件系统、**不启动 Agent 进程**」 | **现行要求且可满足**：授权判定在读取会话行与 `factory.resume` 之前（同文件 :9 + design D3 步骤 1），该路径根本不会到达后端 |
| 3 | `specs/node-link-owner-server/spec.md:45` / `:57`（R34/R36） | 「出现任何键时 MUST 以 `command.rejected`…拒绝，且**不得启动 Agent 进程**或部分应用参数」 | **现行要求且可满足**：payload 校验在解码/路由边界，先于任何后端调用 |
| 4 | `specs/workspace-resolution/spec.md:11` / `:14`（R24） | 「恢复在**调用后端之前**失败…**不启动 Agent 进程**」 | **现行要求且可满足**：需求正文 :7 明写「SHALL 在调用会话后端之前…校验」，即 design D3 步骤 3 先于步骤 4 |
| 5 | `design.md:91` | 「未授权 → 无副作用拒绝」 | 现行要求且可满足（同上，授权先于一切本机读取） |
| 6 | `design.md:94` | 「任一恢复字段为 NULL → 显式不支持…，**不启动进程**」 | 现行要求且可满足：对应 D3 步骤 2，先于步骤 4 |
| 7 | `design.md:96` / `:120` / `:122` | 「`factory.resume(...)`（可能 spawn 进程；这是唯一副作用）」「门控必须发生在 spawn 之后…」 | **诚实说明与修正注记**，与 spec 一致，合格 |
| 8 | `design.md:20`、`design.md:168`、`plan.md:369` | 「不支持就是显式不支持，**且不产生副作用**」「不支持时显式失败、**无副作用**」「能力门控是否在发送 `session/resume` 之前且**失败无副作用**」 | **叙述层残留**：与 `design.md:96`/`:120`（该路径会短暂 spawn 并回收）及 `tasks.md:42`（「失败无**残留进程**」）不一致 → **DR1-F36（MINOR，非现行 MUST）** |
| 9 | `plan.md:53`、`design.md:122`、`verification.md:155` | 「原值（规格文字）『MUST NOT 启动 Agent 进程』」「本变更的措辞载体是 `proposal.md` 与 specs，不是 `docs/SESSION_CONTINUITY_DESIGN.md`」 | **明确标注为修正前历史引述/记录**，合格 |
| 10 | `plan.md:182` / `:243`（Coverage R24/R36 heading） | 「目录已被删除时返回不可用且**不启动 Agent**」「携带字段被拒绝且**不启动进程**」 | 与 specs 标题逐字一致，且属可满足路径（见 #2–#4），合格 |
**结论：契约集合内已不存在「现行要求」形式的物理不可满足措辞。** 唯一同类残留是第 8 项那三处**叙述层**「无副作用」表述（design 的 Goals/Risks 与 plan 的 Code Review 关注点），它们不是 normative MUST、不使任何要求不可满足，故按 MINOR 报（DR1-F36），但它正处在 F31 的同一误读面上，建议顺手订正，以免 WP5 的 CR5 复核再据此要求「完全不 spawn」。
## 3. DR1-F32 复核
**结论：已解决（三处命名位置全部统一），但同一批量替换在 `verification.md` 第四处留下新缺陷（→ DR1-F38）。**
- `plan.md:363`（PV1 行 Evidence 列）：「reports/<wp>-coder.md / <wp>-tester.md 及其引用的日志文件（阶段 1）与 reports/PV1.log（阶段 2）…（CR1-F1：**不再声明不存在的 `reports/PV1-<WP>.log` 路径**）」——旧路径只以否定式出现。
- `plan.md:403`（Completion Criteria）：「各 WP 分支的 PV1（阶段 1）证据在各 WP 的报告及其引用的日志中（CR1-F1/DR1-F32：不再声明不存在的 `reports/PV1-<WP>.log`）」——同上。
- `verification.md:65`（`## Checks` 的 PV1 阶段 1 行 Evidence）：「各 WP 的报告及其引用的日志」——旧路径**零残留**。
- `verification.md:159`（F21 处置栏）：「已拆为三行：…证据 各 WP 的报告与其引用的日志（如 `reports/wp1-coder.md`、`reports/wp2-coder-PV1.log`）…」——两个示例路径**实存**（`reports/wp1-coder.md` 可读；`wp2-coder-PV1.log` 已读其首 5 行，含 worktree/target/revision 头）。
- 全库检索 `PV1-WP` / `PV1-<WP>`：只命中 `plan.md:363` / `:403` 的**否定式**提及，无任何**声明式**残留（`reports/cr1-review.md:49` 是 CR1 报告的原始描述，属历史报告，非计划声明）。
## 4. DR1-F33 复核（含限制声明）
**结论：内部一致性核对通过 → 已解决（在无 shell 的限制下）。**
- 现值 6 行（`verification.md:14–19`）：`proposal.md 7072345b…`、`acp-wire-protocol 88ec88fe…`、`local-agent-host 1c0b1875…`、`node-link-owner-server 2b1a726a…`、`storage-schema-v2-migration e94efd20…`、`workspace-resolution 45b5b693…`，共 6 行，覆盖 proposal + 5 份 specs，无重复文件、无缺项。
- 括注（`:22`）逐个比对：`proposal.md`（`4ca02015…` → `7072345b…`）、`acp-wire-protocol`（`fc7d4093…` → `88ec88fe…`）、`local-agent-host`（`6f7d725d…` → `1c0b1875…`）、`workspace-resolution`（`07264034…` → `45b5b693…`），并明确写「`node-link-owner-server` 与 `storage-schema-v2-migration` 未变」。
- **同一哈希不再被指派给两个不同文件**（F33 的原缺陷即 acp 的旧值被写成 local-agent-host 的旧值）：现在 4 个旧值 `4ca02015/fc7d4093/6f7d725d/07264034` 各出现一次、各自绑定唯一文件；4 个新值与同文件现值行的前 8 位逐字相符；被点名的两份「未变」文件与括注叙述自洽。
- **限制**：我无 shell，不能实算 sha256；且仓库内已无 Round 3 当时的旧摘要记录（对 `4ca02015|6f7d725d|fc7d4093|07264034` 在 `reports/` 内检索零命中，旧基线是就地覆盖的），因此旧值只能做**文档内自洽**核对，无法外部对照。该限制不改变结论——F33 是「同一文档内自相矛盾」类问题，矛盾已消除。
## 5. DR1-F34 复核
**结论：已解决（逐字一致）。**
- `specs/acp-wire-protocol/spec.md:5` = ``### Requirement: `session/resume` 的类型化解码与往返保真``；`plan.md:67`（Coverage Index R1 的 `source.heading`）= 同一字符串，逐字相同（含反引号与空格）。
- 全库「圆形」只剩：`verification.md:158`（F34 的历史 finding 引述，正当）、`reports/tp1-test-design.md:51` 与 `reports/wp1-coder.md:45`（角色报告内的旧引用）。契约集合（proposal/specs/design/plan/tasks）内**零残留**。TP1 报告中的旧引用随其修复轮更新（→ DR1-F40，SUGGESTION）。
## 6. DR1-F35 复核
**结论：已解决。**
- `verification.md:38`（CR1 / tasks 3.2 / reviewer branch / FAIL 无、PASS）、`:39`（CR2 / 3.3）、`:40`（CR7 / 3.8 / FAIL / NEW）、`:41`（DR1 Round 7 / FAIL / NEW）四行齐备，字段完整（含 Work Package 列、Agent/run 标识、Target Revision、Evidence Type/ID）。
- TP1 行（`:37`）：`DELIVERY / NOT_APPLICABLE`，结果写「PASS / NEW（**但 CR7 判 FAIL，本行待修复轮通过后失效**）」，与「CR7 判 FAIL、待修复轮」的状态一致，且未把待修复状态伪装成已闭环。
- 观察（**非缺陷**）：`## Handoff Index` 只含 DR1 的第 2、7 轮，第 1/3/4/5/6 轮只在 `## Dependency Declaration Review`（`:51–:57`）登记。按该节模板注释（`:45–47`：本表逐条记录 Review ID、Round、Plan Revision 与报告路径）与前三轮 reviewer 的一致口径，DCR 表是计划审查结论的权威登记处，因此不构成本轮缺陷；仅提请 main 注意「同一线程的轮次在两处登记粒度不同」。
## 7. 是否引入新不一致
| 检查项 | 结论 | 依据 |
| --- | --- | --- |
| Coverage Index 37 行 heading 与 5 份 specs 标题逐字对应（含 R1 改名） | **通过** | 逐行比对：`acp-wire-protocol` R1–R4（`specs/…:5/:12/:18/:24`）、`local-agent-host` R5–R12（`:5/:12/:18`、Requirement `:19`、Scenarios `:25/:30/:35/:41`）、`storage-schema-v2-migration` R13–R22、`workspace-resolution` R23–R26、`node-link-owner-server` R27–R37（Requirement `:5`、Scenarios `:12/:17/:21/:26/:31/:36`、`### Requirement` `:43`、`#### Scenario` `:50/:54/:59`）。37/37 行均逐字命中，R1（往返保真）与 R10（…且不发送恢复请求）均已同步改名 |
| Coverage Index 的 `tasks` 引用实存 | **通过** | 引用集合 = {2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 4.2}，`tasks.md` 中 2.1–2.8 与 4.2 均存在 |
| `checks` 引用实存 | **通过** | `PV1`/`PV2` 均在 `plan.md` 的 `### Project Verify` 表定义；C1/C2 在 `## Main E2E` 的 `alternative_checks` 与 `tasks.md` 8.1/8.2 的 `[C1]`/`[C2]` 双向对应，并在 `verification.md:69/:70` 各有一行（final 阶段待填结果） |
| Execution Waves 合法性 | **通过** | W1 = WP1/WP2/TP1（依赖均 none）；W2 = WP3（code:WP2 ∈ W1）；W3 = WP4（code:WP3 ∈ W2）、WP5（code:WP1 ∈ W1 + code:WP3 ∈ W2）；W4 = WP6（五上游最晚 W3）；W5 = TP2（六上游）。层级严格晚于其 code 依赖所在层 |
| Serialization Reason 合法且「写入重叠不构成串行理由」 | **通过** | 9 行全为 `NOT_APPLICABLE`；逐波核对同波写集合：W1（WP1: acp-protocol / matrix / fixtures/acp / ACP 文档；WP2: node-link-protocol、identity-auth、commands.json、schemas+fixtures/node-link、4 份文档、脚本、`core/src/broker.rs`、AGENTS/README/ci.yml；TP1: `reports/tp1-*.md`）与 W3（WP4: storage-sqlite + `CORE_PORTS…§7*`；WP5: agent-host）**无交集**；同文件跨波者（`README.md` WP2→WP6、`core/src/broker.rs` WP2→WP3、`CORE_PORTS…` WP3→WP4、两个 `tests/` 目录 WP→TP2）都有 Merge Owner/Merge Order/Re-verify 登记且分处不同波次 |
| 三个门禁标记 | **通过** | `[e2e-owned]`（tasks 8.3）、`[final-verification]`（9.1）、`[validation]`（7.1）各唯一、分开 |
| Main E2E 决策完整性 | **通过** | `mode: not-applicable` + `reason` + `basis` + 非空 `alternative_checks: [C1, C2]` + `downgrade_approval`（含用户原话、时间、来源）齐备 |
| Coverage Index 计数与 5 份 specs 的标题总数 | **通过** | specs 标题 = 4 + 8 + 10 + 4 + 11 = 37，与 Coverage 行数一致 |
---
## Findings
### 复核项（原问题 ID 沿用，按原 ID 逐项确认）
| ID | Severity | Location（本轮 target） | 复核依据 | Recheck 结论 |
| --- | --- | --- | --- | --- |
| DR1-F31 | MAJOR（原） | `proposal.md:29` / `:42` / `:51`；对端 `specs/local-agent-host/spec.md:21/:23/:33`、`design.md:113–122` | 见本文 §1：四块逐条读毕，现行要求只有「MUST NOT 发送 + MUST 终止并回收 + 不降级 + 不伪造成功」，旧措辞以「原措辞…物理不可满足」显式标注为历史；三方逐字一致且可满足 | **已解决** |
| DR1-F32 | MINOR（原） | `plan.md:363` / `:403`、`verification.md:65` / `:159` | 见 §3：三处命名位置全部统一，`PV1-WP` 全库检索只剩两处否定式提及 | **已解决**（第四处替换副作用另记 DR1-F38） |
| DR1-F33 | MINOR（原） | `verification.md:14–19` / `:22` | 见 §4：6 行现值与 4 对旧→新映射内部自洽，同一哈希不再跨文件复用 | **已解决**（无 shell，未实算，已声明限制） |
| DR1-F34 | SUGGESTION（原） | `specs/acp-wire-protocol/spec.md:5`、`plan.md:67` | 见 §5：两处逐字相同 | **已解决** |
| DR1-F35 | SUGGESTION（原） | `verification.md:37–41` | 见 §6：CR1/CR2/CR7/R7 四行齐备，TP1 行状态与 CR7 FAIL 一致 | **已解决** |
### 新发现（本线程连续编号）
| ID | Severity | Location | Trigger / Evidence | Impact | Recommendation（最小修复面） |
| --- | --- | --- | --- | --- | --- |
| DR1-F36 | MINOR | `design.md:20`（Goals 第 3 条）、`design.md:168`（Risks 第 1 条）、`plan.md:369`（Code Review 关注点第 2 项） | 三处均以「无副作用 / 不产生副作用」描述「能力未宣告」路径，而同一 `design.md:96` 写「`factory.resume(...)`（可能 spawn 进程；**这是唯一副作用**）」、`:120` 写「门控必须发生在 spawn 之后…**代价：一次恢复失败会短暂启动并清理一个子进程**」；`tasks.md:42`（CR5 职责）写的是「失败无**残留进程**」。同一执行路径在同一契约文件内被两种口径描述 | 不使任何要求不可满足（这三处是 Goals/Risks/审查关注点叙述，不是 normative MUST），但正处 F31 的同一误读面：CR5 若照 `plan.md:369` 字面判「失败必须完全不 spawn」，会与已冻结的 D4 顺序冲突，重复 F31 式的无效轮次 | 三处各改一句：`design.md:20` →「不支持就是显式不支持，且不留下持久副作用（一次失败会短暂拉起并回收子进程，见 D4）」；`design.md:168` →「不支持时显式失败、无残留进程/无状态变化」；`plan.md:369` →「失败无**残留副作用**（不发送 `session/resume`、无残留进程/绑定、会话状态不变）」，与 `tasks.md:42` 口径统一。**若 main 认为本轮不动 plan.md 更划算**：只改 `design.md` 两处（design 不在 contractDigest 之外的档案，改动会改变 digest）——两处改与三处改都会改变 contractDigest，请自行权衡；本项不阻断 |
| DR1-F37 | MINOR | `verification.md:41`（`## Handoff Index` 第 7 轮行，FAIL/NEW）与 `verification.md:57`（`## Dependency Declaration Review` DR1 Round 7 行，FAIL）均指向 `openspec/changes/session-resume/reports/dr1-dependency-review-round7.md` | 该路径**不可读**：`read` 返回 `ENOENT: no such file or directory`；`find **/dr1-dependency-review-round*.md` 只命中 round 2–6；`watchdog_diff` 的未跟踪清单里也没有它。同时派发说明称「十份报告均在仓库内」，与事实不符（与 Round 4 的同类情形相同） | ① 违反 `roles/_shared/role-report.md`「NEW/REUSED 的报告路径必须可读」；② 使本轮 F31–F35 的复核缺少原始证据（我只能按当前文本 + `verification.md` 转述复核，替换了应逐条比对的原文）；③ 这是**第三次复发**（F19 → F25 → F37），且 F19/F25 的处置语「五份报告全部可读」已被再次证伪 | 二选一：① 把第 7 轮报告原样落盘到该路径（推荐，与 F19/F25 的处置方式一致）；② 若原文已不可得，把 `:41`/`:57` 的 Report Path 改为**可读**的转述记录（例如新增 `reports/dr1-round7-summary.md` 并改写本报告的复核依据来源），并把转述范围写明（哪些结论是转述、哪些已独立复核）。同时建议 main 在派发提示里不再断言「十份报告都在仓库内」，改为先自查报告可读性 |
| DR1-F38 | MINOR | `verification.md:161`（`## Review Findings` 的 CR1 行 Impact 单元格） | 该行现读作「MINOR：声明的 **各 WP 的报告与其引用的日志（如 `reports/wp1-coder.md`、`reports/wp2-coder-PV1.log`） 不存在**，真实证据是各 WP 的报告与其日志」——这是 F32 的批量替换把 CR1 的原始描述（原为 `reports/PV1-WP1.log` / `reports/PV2.log` 不存在）覆盖成了**新措辞**，于是同一句里「声明的 X 不存在」而 X 与后半句几乎同义；且 `reports/wp1-coder.md`（可读）与 `reports/wp2-coder-PV1.log`（已读其首 5 行，含 worktree/CARGO_TARGET_DIR/revision 头）**两者都实存**，故「不存在」在当轮版本为**不实陈述**，并与其 Resolution「已改 PV1 行的证据列…」自相矛盾 | 纯记录层（不影响任何行为契约、写范围或检查可产出性），但 `## Review Findings` 是交付/终验要逐行填的权威表，一条自相矛盾且不实的行会给 validator 与最终验收制造困惑 | 一行改写：把 Impact 恢复为「原声明的日志路径（`reports/PV1-<WP>.log`、`reports/PV2.log`）不存在，真实证据是各 WP 的报告与其引用的日志」；或直接写「（历史）声明的证据路径不存在，已统一下述口径」。改动只落在 `verification.md`，**不影响 contractDigest**，故不会使本轮 PASS 失效 |
| DR1-F39 | MINOR | `specs/node-link-owner-server/spec.md:62`（R37 Scenario：「`command.terminal`（`status = "failed"` **或 `uncertain`**）与 `nodelink.command.unsupported`」）；连带 `reports/tp1-test-design.md:478`（SR-R37-1 照抄该析取） | 同一输入的终态被允许两个取值，而该路径的结果是**确定**的：`design.md:117`（门控后立即关闭子进程并返回不支持）、`design.md:48`（D1 冻结表：「能力不支持 → `command.terminal` 失败，错误码 `nodelink.command.unsupported`」）、同一 spec 的 `:7`（R27：「**无法确认副作用时** MUST 写入 `uncertain` 终态」——本路径的副作用已被明确处理并回收，无不确定性）。`schemas/node-link/v1/command.schema.json:915–919` 的 terminal status 枚举为 `completed|failed|rejected|uncertain`，两者确实不同。另：TP1 报告内对同一类输入存在两种口径——SR-R7-2（`:202`）严格断言 `status="failed"`，SR-R37-1（`:478`）却接受「failed 或 uncertain」 | 不是两份文档间的互斥指派（错误码唯一，析取在同一条 MUST 内声明），因此不构成 CR7-F2 那类「同一输入两个互斥结论」；但它**允许实现用 `uncertain` 掩盖一个本已确定的失败**，削弱 CR7-F1 想守住的能力诚实口径，并让 R37 的可观察判据弱于 R7/R21 的同类断言 | 一行：把 R37 的 THEN 改为「以 `command.terminal`（`status = "failed"`）与 `nodelink.command.unsupported` 结束…」。注意该改动会改变 `specs/local-agent-host`… **不会**——改的是 `specs/node-link-owner-server/spec.md`，会使 contractDigest 变化并需再走一轮；若 main 认为不值得再开轮，可置换为：在 `## Check Plan Changes` 明确「`uncertain` 仅在崩溃窗口（R32）出现；能力不支持路径的终态必须是 `failed`」，并把该口径写进 WP6 与 TP2 的派发提示（不改契约、不改 digest） |
| DR1-F40 | SUGGESTION | `reports/tp1-test-design.md:51`（R1 行的 heading 引述仍为「…与圆形保真」）、`reports/wp1-coder.md:45`（同旧引述） | 契约侧已改为「往返保真」（`specs/acp-wire-protocol/spec.md:5`、`plan.md:67`），但 TP1 的映射表仍逐字引用旧标题 | TP1 报告正在修复轮（CR7 判 FAIL），其 heading 引述若不更新，修复轮后仍会与 `plan.md` 的逐字要求脱节；`wp1-coder.md:45` 属已完成的分支报告，仅为历史标签，无影响 | 在 TP1 修复轮的派发提示里明确：把映射表 R1 的 heading 引述更新为「`### Requirement: session/resume` 的类型化解码与往返保真」（其余 36 行不受影响）。不改任何计划/契约文件 |
**New findings 汇总：0×CRITICAL、0×MAJOR、4×MINOR（F36–F39）、1×SUGGESTION（F40）。**
## Check Plan 核对
- 本轮为 **plan 类型**，计划内检查 = `plan.md` 的 `## Coverage Index`（37 行）、`## Verification Strategy`、`### Main E2E` 决策。我逐行核对了 Coverage 的 `source.heading`（37/37 实存且逐字）、`tasks` 与 `checks` 引用（全部实存/已定义）、Execution Waves 与 Serialization Reason（合法，同波无写入重叠）、Main E2E 的四个必填项与三个门禁标记。
- **不存在待返回的执行证据影响本轮判断**：PV1/PV2/C1/C2 的 PASS 行属后续阶段（`verification.md:65–70` 目前只登记阶段、范围与证据路径，结果列留空，未冒称通过），E2E 决策为 not-applicable。**code review 或 plan review 的 PASS 不等于任何 PV/E2E 通过**，本报告不涉及也不覆盖它们。
- 本轮无 Check Plan 变更（未改任何计划字段），故不需要新的 Check Plan Changes 记录；`verification.md` 的 `## Check Plan Changes` 现有两条（Round 4、Round 6）与机器事实一致，本轮未发现新偏离。
- 计划调整是否仍覆盖约定风险：是——红窗口（F13/F14/F23）、`--no-verify` 例外、`pack` 规则（D6 按 transport）、`identity-auth` 镜像（F9）均已登记，与本轮独立核对结果一致。
## 独立判断
### 1. 当前契约集合是否已无「物理不可满足」与「同一输入两个相反结论」的缺陷？
**是——normative 层已收敛。** 判定与依据：
- **物理不可满足**：已无。规范要求集合 = `proposal.md` 的 `constraints` + 5 份 specs 的 MUST/SHALL + `design.md` 的 D1–D6 冻结项。逐条核对「能力未宣告」路径后只有四条可观测、可实现的要求（不发送 `session/resume`；在返回前终止并回收本次拉起的子进程；不降级；不伪造成功），它们在 D4「拉起 → `initialize` → 门控 → 关闭 → 返回不支持」时序下全部可满足；`proposal` 与 spec 都不再要求「不启动进程」。其余含「不启动 Agent 进程」的要求（R24/R31/R34/R36 与 design D3 步骤 2）都位于**后端调用之前**的路径上，可满足（§2 逐处判定表）。
- **同一输入两个相反结论**：normative 层已无。CR7-F2 的 NULL 路径现只有唯一错误码（`nodelink.command.unsupported`，`specs/workspace-resolution/spec.md:7`、`design.md:105`、`assumption:51`、`node-link-owner-server/spec.md` 的 R34/R37 一致）；「授权失败」「payload 非法」「cwd 校验失败」也各自唯一映射（`nodelink.export.not_granted` / `nodelink.command.unsupported_field` / `nodelink.internal.unavailable`），且 TP1 的关键路径②④正是按三者互不相同做的可区分性设计。**残留的是允许性歧义而非相反结论**：R37 的 status 析取（F39）与「无副作用」叙述（F36）各只有一处/一族，不构成「同一输入两个互斥结论」。
- 因此本轮判 **PASS**（0×CRITICAL / 0×MAJOR），非阻断项为 DR1-F36–F40。
### 2. 非阻断项清单（不阻断交付，建议按顺序处理）
1. **DR1-F37（MINOR）**：第 7 轮报告不可读 + 派发说明不实（第三次复发）。合入前的 premerge/final 审计会核对已填写行与报告路径，一条 FAIL/NEW 且路径不可读的行不应长期悬置。
2. **DR1-F38（MINOR）**：`verification.md:161` 的 CR1 行被替换改坏，现含不实陈述，建议一行订正（只改 verification.md，不影响本轮 PASS）。
3. **DR1-F36（MINOR）**：`design.md:20/:168` 与 `plan.md:369` 的「无副作用」叙述与 D4 的既有成本说明不一致，建议在派发 WP5/CR5 前订正，避免 CR5 照字面要求「完全不 spawn」。
4. **DR1-F39（MINOR）**：R37 的 `status` 析取允许用 `uncertain` 掩盖确定失败；改契约会再触发一轮，若不愿再开轮，请用「不改 digest」的方式（派发提示 + Check Plan Changes 记录）固定口径。
5. **DR1-F40（SUGGESTION）**：TP1 修复轮顺手更新 R1 的 heading 引述（圆形保真 → 往返保真）。
### 3. 盲区：本轮之后同类问题还能靠什么发现？
本轮之所以能清掉 F31，是因为「plan 审查」被显式扩大了范围，做了**跨文件的措辞族扫描 + 与冻结设计的可满足性推理**。这个过程暴露出当前审查手段的三个真实盲区：
1. **「同一句话的多份副本」靠人工记忆维护**。F31 与 F36/F38 的根因完全相同：一处要求被改写后，同一语义的副本散落在 proposal 的 `agentic-intent`、design 的 Goals/Risks、plan 的 Contract Changes/关注点、tasks 的 WP 描述与既有报告里，改写只覆盖了其中一处；而替换动作本身又会把**引述该旧措辞的记录行**改坏（F38）。这不是「实现/用例阶段才能读到字面」的问题——它是**可机械发现**的：任何一次措辞订正后，对整个 change 目录（含 `agentic-intent` 块、design 叙述、plan/tasks 散文、报告）grep 该措辞族（`启动…进程|不启动|spawn|无副作用|不发送`）并逐处判定「现行要求 / 历史引述」，就能挡住 F31/F36/F38。
2. **「要求 vs 实现时序」的可满足性只能靠人推理**。F31 的实质是「能力只能经 `initialize` 得知，而 `initialize` 必先 spawn」——这是**领域事实**（ACP 协商语义）与**冻结时序**（design D4）的交叉推理，plan 阶段只能由 reviewer 主动做，机器门禁（`workflow-check.mjs`）明确不覆盖「声明是否属实/是否合理」。目前这道推理只在这一轮被显式做了，**尚未固化成每次 plan 重评的固定动作**。
3. **断言层面的弱化只有写用例时才暴露**。F39（R37 的 `failed|uncertain` 析取）在契约里读起来完全合规，只有把它与 R27 的 `uncertain` 定义、design D1 的「失败」放在一起，或与 TP1 的 SR-R7-2 对照时，才看得出「同一输入两种终态」。这类问题本轮靠交叉阅读发现，但**不能保证下次不在更细的断言里复发**。
**是否要在 WP3–WP6 的派发提示里加一道「契约可满足性」自检：建议加，且要写成可核对的动作而不是态度要求。** 最小内容（可直接粘进派发提示）：
> 开工前对你的写范围所涉的每一个 R 行（以及你将要改的每条 MUST）：① 写出**可观察判据**（用哪个命令/断言看它）；② 写出**执行时序**（该要求在 design D3/D4 的哪一步、步骤之前还是之后）；③ 逐条自检「这条 MUST 在我的实现路径上是否**做得出来**」，特别是涉及「不启动进程」「不产生副作用」「不读取」「不改写」这类**否定式保证**——若字面做不到，**停下来报 main**，并原样引用该句 + 说明与哪一步时序冲突，**不得自行重新解释**；④ 若你发现任何一处与设计冻结项相矛盾的措辞，同时报出**同一措辞在其它文件里的副本位置**（proposal 的 `agentic-intent`、design 的 Goals/Risks、plan/tasks 散文都算），以便一次改净。
>
> 另外两条与本变更直接相关的口径提醒（来自 DR1 Round 8 的复核）：能力门控发生在 spawn 之后——「不支持」路径的合格判据是**未发送 `session/resume` + 子进程已回收 + 会话状态不变**，不是「完全没有 spawn」；该路径的终态是 `failed`，`uncertain` 只属于崩溃窗口。
**需要 main 注意的收敛代价（供决策，不建议现在做）**：若采纳 F36/F39 的「改契约」方案，`proposal.md`/`design.md`/specs 之一被改 ⇒ contractDigest 变化 ⇒ 本轮 PASS 失效，需再派发第 9 轮。F38/F40 与「回填 Round 8 行」都只改 `verification.md`/角色报告，**不影响 contractDigest**，可以在保住本轮 PASS 的前提下完成。
## Assessment
- **本轮检视结论：PASS**（对应 `target_revision` = `sha256:57b47d87…5ee5`）。0×CRITICAL、0×MAJOR；DR1-F31…F35 逐项复核为**已解决**（F32 的三处命名位置已解决，第四处为文档替换副作用，已另记 F38）；新报 4×MINOR + 1×SUGGESTION，全部不影响依赖声明属实性、写范围归属与任何要求的可满足性。
- **逐检查 ID 证据核对**：`## Coverage Index` 37 行 × `source.heading`/`tasks`/`checks` 逐行实存（§7）；`### Main E2E` 决策四要素齐备、C1/C2 双向登记；三个门禁标记唯一分开；`## Dependency Declaration Review` 最大轮次行（`:58`）目前仍是占位符——按 `procedures/workflow-check.md`「plan 阶段要求 DCR 绑定当前 contractDigest、且取最大 Round」，**plan 门禁在 main 回填该行前仍会红**，但这属 main 的记录动作，不影响本轮的静态判定。
- **待补证据与门禁**：PV1/PV2（各 WP 阶段 1、集成基线/候选/主分支阶段 2）与 C1/C2 的 PASS 行属后续阶段，须在对应交付/验证门禁前补齐；本轮为规划审查，不要求也不核对执行证据（规划审查不得冒称已执行测试）。**本轮结论不覆盖任何 PV/E2E 是否通过。**
- **复用依据**：本轮不复用任何历史轮的结论作为结论依据；仅把 `dr1-dependency-review-round4/5/6.md` 作为「记录格式基线」参照。历史轮次保留、不被覆盖，当前结论以 Round 8（本报告）为准。
## 给 main 的回填指引（记录层，不改 digest、不使本轮 PASS 失效）
1. 把本报告原样落盘到 `openspec/changes/session-resume/reports/dr1-dependency-review-round8.md`（**先落盘再回填**，否则路径仍不可读）。
2. 回填 `verification.md:58`：`Reviewer` = 第八个 DR1 实例标识（`reviewer-DR`，附本实例的 run id 与「与前七轮均不同实例、非任何 WP Owner」）；`Plan Revision` = `sha256:57b47d8794f92e845f779bc40a091d74020e8b71eb0c2eafe79ee6913ccc5ee5`；`Result` = **裸 `PASS`**（不要加 `**…**` 或括注——Round 5/6 已记录门禁判据是「去空白后忽略大小写必须恰好等于 PASS」）；`Report Path` = `openspec/changes/session-resume/reports/dr1-dependency-review-round8.md`（与其余行同写法，`existsAt([changeRoot, projectRoot], …)` 才会命中）。
3. 在 `## Review Findings` 增补 DR1-F36…F40 五行（Severity、Location、Trigger/Evidence、Impact、Recommendation、Recheck 列按现有格式），并把 F31–F35 的 Recheck 列写为「**DR1 Round 8 复核：已解决**」（F32 建议写「已解决；替换副作用转 F38」）。
4. 若同批做 F38/F40 与第 7 轮报告落盘（F37），这些改动只落在 `verification.md` 与 `reports/**`，**不改变 contractDigest**，本轮 PASS 保持有效；**不要**为了顺手清 F36/F39 去改 proposal/design/specs/plan——那会立刻使本轮摘要失效并需要第 9 轮。
```yaml
handoff_index:
  - task_id: NOT_APPLICABLE
    work_package: NOT_APPLICABLE
    role: reviewer
    phase: plan
    stage: plan
    round: 8
    target_revision: "sha256:57b47d8794f92e845f779bc40a091d74020e8b71eb0c2eafe79ee6913ccc5ee5"
    evidence_type: REVIEW
    evidence_id: DR1
    report_path: openspec/changes/session-resume/reports/dr1-dependency-review-round8.md
    result: PASS
    evidence_status: NEW
    applicability_basis: "针对 DR1-F31…F35 修复后的当前 contractDigest 做静态规划审查（新实例、只读、未参与实现）：逐条复核 F31–F35（F31 三方措辞一致性、F32 三处证据路径、F33 Target 括注自洽、F34 R1 标题逐字、F35 Handoff Index 行完整性）；对 proposal.md 的 agentic-intent 四块与 5 份 specs、design.md 做「不启动进程/不产生副作用/spawn」措辞族全量扫描并逐处判定现行要求 vs 历史引述；核对 Coverage Index 37 行 heading/tasks/checks、Execution Waves、Serialization Reason、Main E2E 决策与三个门禁标记。新报 4×MINOR（F36 叙述层『无副作用』残留；F37 第 7 轮报告路径不可读；F38 CR1 行替换副作用致不实陈述；F39 R37 的 status 析取）与 1×SUGGESTION（F40 TP1 报告的 R1 旧标题引述），均非阻断。未执行任何 PV/E2E；未复算 contractDigest 与 6 行 sha256（无 shell），已声明该限制"
    source_evidence: NOT_APPLICABLE
```
```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "返回了 DR1 Round 8 完整复核报告：逐项复核 DR1-F31（proposal.md:29/:42/:51 与 specs/local-agent-host/spec.md:21/:23/:33、design.md:113-122 三方一致，旧措辞已标注为历史引述 → 已解决）、F32（plan.md:363/:403、verification.md:65/:159 已统一，PV1-WP 全库仅剩否定式提及 → 已解决）、F33（verification.md:14-19 六行现值与 :22 括注内部自洽，同一哈希不再跨文件复用 → 已解决，已声明无 shell 限制）、F34（specs/acp-wire-protocol/spec.md:5 与 plan.md:67 逐字相同 → 已解决）、F35（verification.md:37-41 四行齐备、TP1 行状态一致 → 已解决）；措辞族全量扫描给出 10 组逐处判定（现行可满足 6 处 / 历史引述 3 处 / 叙述层残留 1 族）；新报 DR1-F36（MINOR，design.md:20/:168 与 plan.md:369 的『无副作用』与 design.md:96/:120 的成本说明不一致）、F37（MINOR，reports/dr1-dependency-review-round7.md 不可读，ENOENT + find/watchdog_diff 双证，第三次复发）、F38（MINOR，verification.md:161 CR1 行被替换改坏且两个示例路径实存）、F39（MINOR，specs/node-link-owner-server/spec.md:62 的 status 析取）、F40（SUGGESTION，tp1-test-design.md:51 旧标题引述）；Assessment = PASS（0×CRITICAL/0×MAJOR），并给出非阻断清单、盲区分析与 WP3–WP6『契约可满足性自检』建议文本、以及给 main 的记录回填指引"
    }
  ],
  "changedFiles": [],
  "testsAddedOrUpdated": [],
  "commandsRun": [
    {
      "command": "NOT_APPLICABLE（只读审查，无 shell/写权限，未执行任何构建或测试）",
      "result": "not-run",
      "summary": "规划审查不执行 PV1/PV2/C1/C2/E2E；本轮结论不覆盖任何执行证据。需 main 在 premerge/final 前补齐 PV1（各 WP 阶段 1 + 阶段 2 workspace）、PV2 与 C1/C2 的 PASS 行"
    }
  ],
  "validationOutput": [
    "契约集合措辞族扫描（启动…进程/不启动/spawn/无副作用）：现行且可满足 6 处（proposal.md:29；specs/node-link-owner-server/spec.md:29/:45/:57；specs/workspace-resolution/spec.md:11/:14；design.md:91/:94）+ 明确标注的历史引述 3 处（plan.md:53、design.md:122、verification.md:155）→ 无『现行要求形式的物理不可满足措辞』",
    "Coverage Index 37/37 行 heading 与 5 份 specs 标题逐字一致（含 R1『往返保真』、R10『…且不发送恢复请求』改名）；tasks 引用 {2.1..2.6, 4.2} 与 checks {PV1,PV2} 全部实存",
    "Execution Waves W1–W5 严格晚于其 code 依赖所在层；9 行 Serialization Reason 全为 NOT_APPLICABLE，逐波核对写集合无交集，同文件跨波者均有 Merge Owner/Order/Re-verify 登记",
    "verification.md Target 括注内部自洽：4 个旧值各绑定唯一文件、4 个新值与现值行首 8 位相符、被点名『未变』的两份与叙述一致（无 shell，未实算 sha256）",
    "reports/dr1-dependency-review-round7.md 不可读（read=ENOENT；find **/dr1-dependency-review-round*.md 仅命中 round2–6；watchdog_diff 未跟踪清单亦无该文件）"
  ],
  "residualRisks": [
    "无 shell/引擎能力 → 不能复算 contractDigest 与 6 行 sha256；DR1-F33 只能做文档内自洽核对（已在报告中声明）",
    "第 7 轮报告缺失（DR1-F37）→ F31–F35 的复核依据是当前契约文本与 verification.md 转述，而非第 7 轮原文；建议 main 先落盘该报告",
    "DR1-F36/F39 若采纳『改 proposal/design/specs』的修法，会使 contractDigest 变化、本轮 PASS 失效并需第 9 轮；若只做 verification.md/reports 层修复则不失效",
    "verification.md:58（DCR Round 8 行）在 main 回填前仍是占位符，plan 门禁仍会红；回填需 Result 为裸 PASS、Report Path 可读",
    "WP3–WP6 阶段仍存在同类风险：能力门控在 spawn 之后的『无副作用/无残留』口径若被逐字误读，可能再次触发 F31 式无效轮次（已给出派发提示自检文本）"
  ],
  "noStagedFiles": true,
  "diffSummary": "无改动：本会话为只读审查（无 write 工具），未修改、未暂存、未提交任何文件；watchdog_diff 确认 openspec/changes/session-resume/** 整目录仍为未跟踪、无被跟踪文件改动",
  "reviewFindings": [
    "no blockers: 0×CRITICAL / 0×MAJOR（DR1-F31…F35 复核为已解决）",
    "minor: design.md:20, design.md:168, plan.md:369 - 叙述层『无副作用』与 design.md:96/:120 的『会短暂 spawn 并回收』不一致（DR1-F36，非现行 MUST）",
    "minor: verification.md:41, verification.md:57 - 引用的 reports/dr1-dependency-review-round7.md 不可读（DR1-F37，第三次复发，影响本轮原文复核依据）",
    "minor: verification.md:161 - CR1 行被 F32 的批量替换改坏，现含『这两个示例路径不存在』的不实陈述（DR1-F38）",
    "minor: specs/node-link-owner-server/spec.md:62 - R37 允许 status ∈ {failed, uncertain}，与同文件 :7 的 uncertain 定义及 design.md:48/:117 的『失败』口径不一致（DR1-F39）",
    "suggestion: reports/tp1-test-design.md:51 - 映射表 R1 仍引旧标题『圆形保真』，建议随 TP1 修复轮更新（DR1-F40）"
  ],
  "manualNotes": "① 本轮为 DR1 第八轮（recheck，Review Type=plan），我是第八个新实例、只读、无 write 工具，未参与前七轮与任何实现；结论 PASS（0×CRITICAL/0×MAJOR），对应 target_revision sha256:57b47d87…5ee5。② 核心结论：DR1-F31 确已修好且三方（proposal.constraints/success_criteria ↔ specs/local-agent-host R10 ↔ design D4）逐字同序一致、在 D4 时序下可满足；契约集合内已无『现行要求形式的物理不可满足措辞』，也再无『同一输入两个互斥结论』（NULL 路径错误码唯一）。③ 提请 main 优先处理三件事：(1) DR1-F37 —— 第 7 轮报告不在仓库内（ENOENT 实证），而 verification.md:41/:57 是 FAIL/NEW 行，违反 role-report『报告路径必须可读』，且这是 F19→F25 之后的第三次复发；派发说明『十份报告均在仓库内』不实，建议不要再这样断言；(2) DR1-F38 —— verification.md:161 的 CR1 行被 F32 的批量替换改坏，现含可证伪的不实陈述（两个示例路径都实存）；(3) 回填 Round 8 行时 Result 必须是裸 PASS、Report Path 必须先落盘（否则 plan 门禁因最大轮次行解析失败继续红）。④ 明确不建议本轮顺手做的事：为清 F36/F39 去改 proposal/design/specs（会使 digest 变化、本轮 PASS 失效、需第 9 轮）；若必须清，F39 可用『不改契约』的替代方式（派发提示 + Check Plan Changes 记录）固定口径。⑤ 盲区回答：F31 这一类其实**可以机械发现**——任何一次措辞订正后对整个 change 目录 grep 该措辞族并逐处判定『现行要求/历史引述』；真正只能靠人的是对『要求 vs 冻结执行时序』的可满足性推理（本轮已做，建议固化成每次 plan 重评的固定动作），以及断言层的弱化（如 F39 这类析取）只能在写用例/交叉阅读时暴露。⑥ 本报告需 main 原样持久化到 openspec/changes/session-resume/reports/dr1-dependency-review-round8.md；commandsRun 为空是本轮无 shell/测试执行能力所致（规划审查边界），非跳过检查。"
}
```
