<!-- Dependency Declaration Review（plan 阶段）原始报告，Round 13。Reviewer 只报告，不修改权威规划文件。 -->

task_id: propose:DDR-sync-scope-and-pwa-client-r1
role: reviewer
phase: plan
agent_context:
  agent_id: "reviewer-plan-ddr-13"
  isolation: "fork_turns=none（新建独立子 Agent，不继承 propose/apply/实现对话；本 Reviewer 不拥有任何工作包，owners 为 coding-1..coding-7、coding-5b、testing-1..testing-4；未参与 Round 1–12 的任何修正）"
target_revision: "plan-v2:sha256:e5b00e62e26459d6dcd522d9f07ffb2bf38607b4daafa96c44b3cee92ad21a66"
scope: "openspec/changes/sync-scope-and-pwa-client/ 的依赖类型声明（code/contract/resource）属实性、Execution Waves 与依赖列一致性、Write Scope 重叠与 Shared File Ownership 登记、Contract Freeze 引用 ⊆ Dependencies 与机械可读性、Dependency Handoffs 完整性、Reviewer 独立性与 tasks.md 的独立检视任务、Coverage Index（叙述表 + agentic-coverage 结构化块）与 5 份 delta spec 的对应、Merge Strategy 交付单元解析与 Order、Runtime Resources 六列完整性；重点为 Round 13 唯一的改动（F13：Contract Freeze 单元格规范化）及其 9 条新增 Serialization Reason，按要求对 6 个指定问题逐条独立复核。"
changes: "只读检视，未修改任何规划文件、代码或 verification.md；仅新增本报告。全部机械判据以内联 node --input-type=module -e 复现（未落盘临时脚本）；未执行任何构建 / 测试 / E2E。"
issues: "F13 的修正经独立复核判定为**已解决且彻底**（10/10 有 contract 依赖的包在 plan 与 premerge 两个阶段均通过冻结可读性判定，premerge 已不再出现 freeze 阻断）。本轮新发现 3 项，全部为 MINOR：F47（`plan.md:884`/`:949` 两处「见 Contract Freeze 列」的指引在本轮改列内容后失效，被移出的冻结路径无法再从该列读回）、F48（`plan.md:863`/`:869` 保留的单条 token `schemas/sync/v1/event-views.schema.json@WP1` 把 `plan.md:921` 明确判归 WP2 独占的文件挂在 WP1 版本号上）、F49（`plan.md:908` 的 capacity 依据写「第 1 批同角色工作包数为 4」，机械复算为 5）。三项均不改变任何批次、依赖边、写范围或检查归属，不阻断 apply 派发。"
result: PASS
evidence_paths: NOT_APPLICABLE
resource_cleanup: "NOT_APPLICABLE（只读检视，未创建 worktree、未分配运行时资源、未在仓库落盘脚本）"

checks:
  - id: "Target Revision 复核（给定摘要 vs 实测）"
    command: "npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage plan --json（cwd=D:\\Project\\acp-remote）"
    result: "planningDigest=plan-v2:sha256:e5b00e62e26459d6dcd522d9f07ffb2bf38607b4daafa96c44b3cee92ad21a66，与调度者给定值**逐字一致**，故非 BLOCKED。errors 恰为 1 条（『Dependency Declaration Review 的 DDR-sync-scope-and-pwa-client-r1 Plan Revision 未绑定当前规划契约摘要』——本轮审查自身尚未回写的预期项），无 waves / freeze / ownership / coverage 类错误。"
  - id: "F13 判据修正的独立复核（同一正则可读性，逐包 fs.access）"
    command: "node --input-type=module -e（复刻 workflow-check.mjs:91-98 frozenContractReadable 与 197-206 的 frozen/hardDeps；bases=[projectRoot, changeRoot]）"
    result: "12 包单元格：WP1/WP2/TP4=`NOT_APPLICABLE`（无 contract 依赖，readable=false 但被 201-203 行豁免）；其余 9 包全部匹配 ^(\\S+?)@(\\S+)$ 且 fs.access 成功（WP3/WP4/WP7=schemas/sync/v1/event-views.schema.json@WP2、WP5a/TP1/TP3=同文件@WP1、WP5b=fixtures/sync/v1/transcripts/device-proof.json@WP1、WP6=schemas/sync/v1/command.schema.json@WP1、TP2=crates/core/src/ports.rs@WP3）。声明 contract 依赖的 8 个包（WP3/WP4/WP5a/WP5b/WP6/WP7/TP2/TP3）frozen 全为 true。"
  - id: "F13 的实际阻断面复核（premerge 阶段）"
    command: "npx --quiet --no-install openspec-agentic workflow check --change sync-scope-and-pwa-client --stage premerge --json"
    result: "errors 总数 1，其中 Contract Freeze 类 0 条；唯一 error 仍是 DDR 绑定项。对照 `reports/merge-mu2-r2.md:153-155`（Round 12 之前的实测：「当前单元或 code 上游 TP2/WP4/WP3 的 Contract Freeze 未冻结或不可读取」），F13 的阻断已消失。另以 workflow-check.mjs:526-541 的 premergeReviewScope 逐单元重放 8 个交付单元（含 code 边传递闭包），freeze 不可读者 0。"
  - id: "单元格改法的前后对照（可解析性语义）"
    command: "node --input-type=module -e（对新旧单元格文本跑同一条 ^(\\S+?)@(\\S+)$）"
    result: "旧形态 `schemas/sync/v1/event-views.schema.json@WP2`, `schemas/sync/v1/command.schema.json@WP1` → false；旧形态 crates/core/src/ports.rs@WP3；crates/sync-protocol/src/views.rs@WP2（形状基准） → false；新形态单条 path@WPk → true。即改法确实把失配面收敛为 0；同时 confirm 门禁不可再对「一条边一条冻结路径」逐个核对（见下方「非阻断观察 2」）。"
  - id: "被移出路径的信息可回读性（逐条追踪）"
    command: "grep -n 于 plan.md / verification.md / reports（WP1/WP2 产物与各包冻结路径）"
    result: "WP3 移出的 `schemas/sync/v1/command.schema.json@WP1` 仍可读于 `plan.md:902`（W2/WP3 的 Enter Condition，逐字）、`:897`（W4/WP6）、`:980`（MU3c Start）；WP5a 的 `schemas/sync/v1/**@WP1`、`fixtures/sync/v1/manifest.json@WP1` 仍可读于 `plan.md:894` 与 `verification.md:32`；TP1 移出的 `crates/sync-protocol/src/{sync,command,views}.rs@WPk` 仅剩无版本号的路径列表（`plan.md:949`）与写者归属（`plan.md:926-928`），**版本号配对无法回读**（唯一显式指引 `plan.md:949`『见 TP1 的 Contract Freeze 列』本身指向已改列的列）。"
  - id: "Contract Freeze 引用 ⊆ Dependencies（本轮改法的副作用面）"
    command: "node --input-type=module -e（剥离后逐包比对 @ 目标与 code:/contract: 目标集合）"
    result: "越界数 0（Work Packages 列的 10 条 token 的 @ 目标全部落在该包依赖列内）。规范化把 Contract Freeze 单元格由「每条 contract 边一条路径」降为「每包一条」；plan.md 全篇其余各节仍有 12 条 @ 引用（合计 22 条），其目标亦全部可核对。例外说明：`schemas/sync/v1/event-views.schema.json@WP1`（WP5a `plan.md:863`、TP3 `plan.md:869`）@ 目标在依赖列内，但该文件按 `plan.md:921` 的『区域收窄』由 WP2 独占，WP1 只改 command/sync/common——路径与版本号的生产者不一致（记 F48）。"
  - id: "Execution Waves 与 hardDeps 重算 earliest（前后对照）"
    command: "node --input-type=module -e（contract 边按 frozen 判定；code 与未冻结 contract 计入硬依赖）"
    result: "本轮（post-F13）：WP1 1、WP2 1、WP3 1、WP4 1、WP5a 1、WP5b 2、WP6 3、WP7 4、TP1 2、TP2 2、TP3 5、TP4 5。对照 Round 12 报告自述的 pre-F13 值（WP3 2、WP4 2、WP5a 2、WP5b 3、WP6 4、WP7 5、TP2 3、TP3 6、TP4 6），**恰好 9 个包下移**，与本轮新补 Serialization Reason 的 9 个包（WP3/WP4/WP5a/WP5b/WP6/WP7/TP2/TP3/TP4）**完全一致**——改法的因果叙述属实。无环、无 placed 缺失与重复、批次号 W1–W6 严格递增且无重号。"
  - id: "workflow-check.mjs:318-321 的波次/上游矛盾检查"
    command: "node --input-type=module -e（对每个包求 hardDeps 中 waveIndex >= 自身 waveIndex 的上游）"
    result: "12 包该集合全部为空，0 条矛盾。WP3/WP4/WP5a 的 contract 边因 frozen=true 已不计入硬依赖，故其 W2 声明与第 1 批最早层级不冲突（进入 Serialization Reason 分支而非该分支）。"
  - id: "9 条 Serialization Reason 的前提复算（workflow-check.mjs:326-341）"
    command: "node --input-type=module -e（逐包求 prefers 的四个 preconditions：code-dependency=codeDeps>0 且 delayed(codeDeps)；capacity=limit<2 或 sameEarliest>limit）"
    result: "9 条全部为 `later=true`（waveIndex > earliest）且前提成立；无一条为多余或错码。逐包锚点：WP3 codeDeps=[] ⇒ capacity；WP4 codeDeps=[] ⇒ capacity；WP5a codeDeps=[] ⇒ capacity；WP5b codeDeps=[WP5a]，delayed(WP5a: earliest 1 < wave 2)=true ⇒ code-dependency；WP6 codeDeps=[WP5a,WP5b] delayed=true；WP7 codeDeps=[WP5a,WP5b,WP6] delayed=true；TP2 codeDeps=[WP3] delayed(WP3: earliest 1 < wave 2)=true；TP3 codeDeps=[WP5a,WP5b,WP6,WP7] delayed=true；TP4 codeDeps=[WP7] delayed(WP7: earliest 4 < wave 5)=true。未被判 delayed 的实例：TP1（codeDeps=[WP1,WP2]，二者 wave=W1=earliest=1，delayed=false）⇒ 其 W2 行确实无 Serialization Reason，正确。"
  - id: "Serialization Reason 证据路径可核对性"
    command: "node --input-type=module -e（looksLikePath + existsAt）"
    result: "9 条证据路径单一且含分隔符：3 条 `openspec/agentic.yaml`、6 条 `openspec/changes/sync-scope-and-pwa-client/plan.md`；两路径在仓库内均可读。码表取值全部落在 {code-dependency, contract-unfrozen, resource-exclusive, capacity} 内，格式满足 `<码>: <证据路径>`。"
  - id: "capacity 事属实性（第 1 批 coder 数与池上限）"
    command: "node --input-type=module -e + 读 openspec/agentic.yaml"
    result: "`openspec/agentic.yaml` 实测 dispatch.pool.coding=3、testing=2（与 plan 引用一致）。按门禁 sameEarliest 口径（同 role 且 earliest 相同）复算：role=coder 且 earliest=1 的包为 **WP1、WP2、WP3、WP4、WP5a 共 5 个**，5>3 ⇒ capacity 前提成立。注：`plan.md:908` 把该数写成「4」（记 F49）；若按 Round 12 的 pre-F13 口径（contract 边全为硬依赖），第 1 批 coder 仅 WP1/WP2 共 2 个，capacity 前提将不成立——即该理由码是随本轮解析口径变化才成立的。"
  - id: "未引入新缺陷：复核「需要理由却缺失 / 波次与依赖矛盾」"
    command: "node --input-type=module -e（遍历 placed 与 earliest、reason 存在性）"
    result: "12 包中 waveIndex>earliest 者恰为上述 9 个，均有合法 reason；waveIndex<=earliest 的 WP1/WP2/TP1 均为 NOT_APPLICABLE/空，无「多余理由被门禁拒绝」的情况（门禁对多余理由只在前提不成立时才报错）。无重复 placed、无未归入 Execution Waves 的包。"
  - id: "连续性：叙述 Coverage 22 行 Closure Unit"
    command: "node --input-type=module -e（Responsible Units → Merge Strategy Order 取最大值，与该行 Closure Unit 比对）"
    result: "22/22 相等，0 处不符（与 Round 11/12 同结论）。"
  - id: "连续性：结构化 Coverage 102 行 vs 5 份 delta spec 标题"
    command: "node --input-type=module -e（复刻 planning-model sourceHeadings/源键比对：heading 精确匹配 + requirement 归属）"
    result: "102 行、id 无重复；heading 唯一匹配缺失 0；5 份 spec 的 102 个 ### Requirement/#### Scenario 标题被引用缺失 0（双向 102↔102）。spec 实测 REQ/SCN：sync-snapshot-scope 4/13、core-derived-events 5/19、pwa-web-client 11/36、workspace-resolution 1/4、local-agent-host 2/7，与 `plan.md:5` 逐字一致。"
  - id: "连续性：tasks.md 结构与 §6 重编号、独立检视任务"
    command: "node --input-type=module -e（复刻 workflow-check.mjs:1381-1387、161-167 的任务解析与 review 任务判定）"
    result: "复选框行 129 = 任务行 129；编号无重复；[e2e-owned]/[final-verification]/[validation] 各恰 1；6.1–6.63 连续无缺号无重号。12/12 包各恰有 1 条 `[wp:WPi]` 派发任务、至少 1 条不含 [wp:] 但含检视标记的引用任务、Reviewer 非空且 ≠ Owner；派发标签集合 = {WP1,WP2,WP3,WP4,WP5a,WP5b,WP6,WP7,TP1,TP2,TP3,TP4}。"
  - id: "连续性：写重叠登记、交付单元、Handoffs、Runtime Resources"
    command: "node --input-type=module -e（复刻 scopePaths/scopeOverlap、ownershipProblem、checkTokens 单元解析、handoff Upstream 集合）"
    result: "按检查器原样语义，字面重叠 1 组（WP1×WP2 的 `docs/SYNC_PROTOCOL.md`）已登记；登记表 18 行 6 列齐备，多写者行的 Merge Order 覆盖全部 Writers 且 Re-verify 引用真实 Check ID。8 个交付单元全为 independent、Order 1–8 连续，成员并集恰为 12 个已声明 ID；10 行 handoff 的 Upstream 集合与依赖目标集合 10/10 相等（含 WP5b=[WP5a,WP1]）。Runtime Resources 5 行 × 6 列，无空格；资源使用者名称全部命中已声明资源，checkIds={PV1,PV2,PV3,IV1,AC1,AC2,AC3}。"
  - id: "行为契约一致性（本轮未改动，回归）"
    command: "grep / 逐节阅读 proposal、design、5 份 spec 与 plan 的 R1–R22"
    result: "连接状态 12 态（8 常规 + 4 阻断）、before/limit/hasEarlier 分页口径、越界标记与 workspace-resolution 的 MODIFIED 正文、快照三资源与 D1 之间未发现互相矛盾的结果或不可能场景（与 Round 12 同结论）。"

## Findings

| ID | 严重度 | 位置 | 问题 | 为什么现在提 |
| --- | --- | --- | --- | --- |
| DDR-sync-scope-and-pwa-client-r1-F47 | MINOR | `plan.md:884`、`plan.md:949` | 本轮唯一改动把 Contract Freeze 列由「每条 contract 边一条路径」收敛为「每包一条」后，两处原先成立的指引失效：`plan.md:884` 写「…在 WP3–WP7 与 TP1/TP3 中被直接消费，路径可核对（见 Contract Freeze 列）」，但该列现在只留 1 条路径、已无法据以核对 WP3–WP7/TP1/TP3 的**全部**冻结产物；`plan.md:949`（TP1 的 handoff 行）写「WP1/WP2 各自的冻结产物路径见 TP1 的 Contract Freeze 列」，而其现存 token 只有 `schemas/sync/v1/event-views.schema.json@WP1` 一条。被移出的 3 条（WP3 的 `command.schema.json@WP1`、WP5a 的 `schemas/sync/v1/**, fixtures/sync/v1/manifest.json@WP1`、TP1 的 `crates/sync-protocol/src/{sync,command,views}.rs`）中，多数仍可于别处读回（`plan.md:894/897/902/980`、`verification.md:32`），但 TP1 的 Rust 镜像路径连同其 `@WP1/@WP2` 版本配对已全plan 不可回读——该配对本轮之前正是由这一格承载。此为调度方自查清单所列的「是否存在新问题」项，亦属 Round 13 改动直接造成。 |
| DDR-sync-scope-and-pwa-client-r1-F48 | MINOR | `plan.md:863`（WP5a）、`plan.md:869`（TP3） | 规范化后保留的单条 token 写作 `schemas/sync/v1/event-views.schema.json@WP1`，把该文件的路径与 **WP1** 版本号配对；但 `plan.md:921`（Shared File Ownership）逐字声明「WP1 的 Write Scope 是 `schemas/sync/v1/` 目录级，字面包含本文件；按区域收窄：WP1 只改 `command.schema.json`/`sync.schema.json`/`common.schema.json`，本文件由 **WP2** 独占」，写者实为 WP2（Owner coding-2）。按门禁口径该 token 不算越界（`@WP1` 确在 WP5a/TP3 的依赖列内），但它正是本轮选定代表该包冻结面的那一条，路径与版本号的生产者不一致，且同一单元格形态在 WP3/WP4/WP7 用的是 `@WP2`。 |
| DDR-sync-scope-and-pwa-client-r1-F49 | MINOR | `plan.md:908` | WP3/WP4/WP5a 的 `capacity` 依据写「第 1 批同角色（coder）工作包数为 4，超过 `dispatch.pool.coding = 3`」。按门禁 `sameEarliest` 口径（同 Role 且 earliest 相同）复算，role=coder 且 earliest=1 的包为 WP1、WP2、WP3、WP4、WP5a **共 5 个**（`openspec/agentic.yaml` 的 3 亦经实测确认），措辞与实算相差 1。结论（5>3、窗口不足）不受影响；但该计数是这三条理由码的**唯一**依据，auditor 逐字复算时会读到不一致值。 |

三项均不改变任何批次、依赖边、写范围、检查归属或交付单元 Order；`PASS` 判定不受影响。

## Assessment

### 一、用户指定的 6 个问题，逐条结论

**1. 每个保留的 `path@pkg` 是否确实可解析且文件存在？被移出的路径是否没有丢失信息？**
前半：**属实**。10 条 token 全部满足 `^(\S+?)@(\S+)$` 且 `fs.access` 成功（WP3/WP4/WP7 的 `schemas/sync/v1/event-views.schema.json@WP2`、WP5a/TP1/TP3 的 `schemas/sync/v1/event-views.schema.json@WP1`、WP5b 的 `fixtures/sync/v1/transcripts/device-proof.json@WP1`、WP6 的 `schemas/sync/v1/command.schema.json@WP1`、TP2 的 `crates/core/src/ports.rs@WP3`）；声明 contract 依赖的 8 包 `frozen` 全为 true。**premerge 阶段**实测 Contract Freeze 类 error 为 0（改动前 `reports/merge-mu2-r2.md:153-155` 有 3 条），说明该修正确实打穿了原阻断面。
后半：**多数未丢失，但非全部**。被移出路径可回读处：`command.schema.json@WP1` → `plan.md:902`（W2/WP3 的 Enter Condition，逐字）/`:897`/`:980`；WP5a 的 `schemas/sync/v1/**@WP1`、`fixtures/sync/v1/manifest.json@WP1` → `plan.md:894` 与 `verification.md:32`；`schemas/sync/v1/**, fixtures/sync/v1/transcripts/**@WP1`（TP3）→ `plan.md:899`；`crates/core/src/ports.rs@WP3`（TP2）→ `plan.md:868` 自身与 `:902`；`device-proof.json@WP1`（WP5b）→ `plan.md:944`。**丢失项**：TP1 的 `crates/sync-protocol/src/{sync,command,views}.rs@WP1/@WP2` 现在全 plan 只剩 `plan.md:949` 一段无版本号的路径枚举与 `plan.md:926-928` 的写者归属，版本配对无处可读（记 F47）。另 `plan.md:884` 与 `:949` 两处「见 Contract Freeze 列」的指路文本已指向不再承载该信息的列。

**2. 选取的那一条是否足以代表该包的冻结面（丢失一条是否会掩盖依赖缺失）？**
**不会掩盖**。逐包复算：① 8 个有 contract 依赖的包全部通过 `frozen=true`，即门禁判定「已冻结」（`workflow-check.mjs:197-206`）——丢失的条目不会被据此误判为未冻结；② 每条 contract 依赖的目标在 Dependencies 列中逐一枚举（WP3=`contract:WP1, contract:WP2`，WP5a/WP7 同，WP5b=WP1，WP6=WP1，TP2=WP2，TP3=WP1），且 10 行 Dependency Handoffs 的 Upstream 集合与依赖目标集合 10/10 相等，故「是否存在该依赖」仍可逐条核对；③ premerge 的临时候选面（`workflow-check.mjs:526-541` 的 `premergeReviewScope`）在 8 个交付单元上 0 条 freeze 错误。**相反方向的代价**（非 F13 的问题，见观察 2）：门禁不再可能对「逐条 contract 边是否逐条有冻结路径」判不合格——但这在本变更里不产生未检出的缺陷，因为代表性路径的覆盖性由人的语义审查（本审查）承担。以 WP3 为例，被丢的 `command.schema.json@WP1` 这一条依赖仍在 Dependencies 列且可在 `plan.md:902` 核对，不构成掩盖。

**3. 两个 `capacity` 判断是否属实？**
**结论属实，论据有误**。`openspec/agentic.yaml` 实测 `dispatch.pool.coding = 3`、`testing = 2`，与 plan 引用一致。第 1 批（earliest=1）coder 数为 **5**（WP1/WP2/WP3/WP4/WP5a），5>3，故 WP3/WP4/WP5a 的 `capacity` 前提成立（`limit<2 || sameEarliest>limit`）。`plan.md:908` 写作「4」（记 F49）。另一处口径变化需记明：这是**本轮改动后才成立**的理由码——按 Round 12 的旧解析口径（contract 边一律计硬依赖），第 1 批 coder 仅 WP1/WP2 共 2 个，`capacity` 前提反而**不成立**（此时 `plan.md:333` 行 absence-check 会拒绝该码）。因此本轮把「contract 边现在被正确识别为已冻结」与「理由码同步调整」两件事做了，二者是绑定的，属实。

**4. 六个 `code-dependency` 的 `delayed` 前提是否逐条成立？**
**逐条成立**。按 `workflow-check.mjs:336-341` 口径（`delayed(ids) = ∃ id: placed(id).waveIndex > earliest(id)`）复算：WP5b 的 `WP5a`（earliest 1 < wave W2）、WP6 的 `WP5a/WP5b`（1<2、2<3）、WP7 的 `WP5a/WP5b/WP6`（1<2、2<3、3<4）、TP2 的 `WP3`（1<2）、TP3 的 `WP5a/WP5b/WP6/WP7`（1<2、2<3、3<4、4<5）、TP4 的 `WP7`（4<5）——六包的 `preconditions['code-dependency']` 全为 true。反向核对未被赋予该码的实例：TP1 的 codeDeps 为 `WP1/WP2`，二者 wave=W1 与 earliest=1 相等，`delayed=false`，故其 W2 行填 `NOT_APPLICABLE` 正确，未引入「需要理由却缺失」。

**5. 是否引入新问题？**
波次与依赖矛盾（`:318-321`）：12 包的 `hardDeps` 中「上游 waveIndex ≥ 自身」集合全部为空，0 条。缺理由：仅在 9 个「waveIndex>earliest」的包上要求理由，该 9 个均有合法码且证据为可读路径；WP1/WP2/TP1 无理由属正确。新问题是 F47/F48/F49 三项 MINOR（前者为指引/版本配对失效，中者为保留 token 的生产者归属，后者为计数文本）。三项均不改变批次、依赖边、写范围、检查归属与 8 个交付单元 Order。另需说明一个**本轮产生但已由 plan 文本自身覆盖**的形态：`schemas/sync/v1/**@WP1` 这类 glob 冻结引用（`plan.md:894`、`:899`）目标目录 `schemas/sync/v1/` 在规划阶段存在、但语义上「按 `:921` 的区域收窄，event-views.schema.json 归 WP2」与「`**@WP1` 含该文件」在字面上有张力——该张力在 Round 9/10/11/12 已存在且未成立为发现（非本轮引入、无机械后果），本轮仅记录为观察 1。

**6. 连续性项是否仍自洽？**
**自洽**。叙述 Coverage 22/22 行 Closure Unit 相等；结构化 Coverage 102 行与 5 份 spec 的 102 个标题双向精确匹配（heading 唯一匹配缺失 0、未被引用标题 0、id 无重复），`plan.md:5` 的 4/13、5/19、11/36、1/4、2/7 与 `grep -c` 实测逐字一致；tasks.md 129 行复选框 = 129 个唯一 X.Y 编号，6.1–6.63 连续无缺无重，三个唯一标记各恰 1 条，12/12 包各恰 1 条 `[wp:]` 派发任务且各有独立检视任务、Reviewer 非空且 ≠ Owner；Shared File Ownership 18 行 6 列齐备（多写者行 Merge Order 覆盖全部 Writers、Re-verify 引用真实 Check ID）；8 个交付单元全 independent、成员并集 = 12 个已声明 ID、Order 1–8 连续；10 行 handoff Upstream 集合与依赖目标集合 10/10 相等；Runtime Resources 5 行 × 6 列无空格、使用者名称全部命中已声明资源；行为契约（12 态、分页 before/limit/hasEarlier、越界标记、快照三资源）未发现互相矛盾的结果。

### 二、非阻断观察（按角色指令列出，不单列 ID）

1. **保留单条 token 的结构性代价**：门禁在 Work Packages 列只看「有没有一条可解析冻结路径」（`workflow-check.mjs:197-206`），「该路径是否覆盖全部 contract 边」被明确列为不做机械校验（`workflow-check.md:115`）。本轮据此收敛是合理取舍；但既然 `plan.md:894/899/902/944` 已逐条保留了其余冻结路径并附版本，把 Contract Freeze 单元格的路径与 `:921` 的写者归属对齐（见 F48）即可在不破坏可解析性的前提下提高精度。`schemas/sync/v1/**@WP1`（WP5a↔TP3 的 Enter Condition）与 `:921` 的张力属既有形态，不单列。
2. **`EV1` 无定义**：叙述 Coverage 的 Final Checks 列出现 `EV1`（结构化的 `checks` 只用 PV1/PV2/PV3/AC1/AC2/AC3，`Independent Validation` 表只有 IV1）。同为 Round 12 已记录的非阻断观察，本轮未变，不阻断 `checkIds` 相关机制（：292 与 ownershipProblem 只读结构化块的 checks）。
3. **叙述 Coverage 的 Responsible Units 与结构化块的 tasks 推导仍有偏差**：按「需求标题 → 结构化行 tasks → planning-model 正则取包」复算，11 行存在单侧归属（如 R6–R9、R19 结构化侧多出 TP2/TP3；R11、R15、R16、R20 叙述侧多出 TP3/TP4；R21/R22 的目标在既有 `openspec/specs/local-agent-host/spec.md` 无对应增量源路径）。这与 Round 12 记录的同类观察同源（其报 6 行）、非本轮引入、且不参与任何机械门禁，仅作记录——但按「同源机制宜一次性收敛」的一贯处理方式，若后续再开修正轮可一并处理。Round 11 的唯一发现 F46 本轮复核仍为已解决状态（`tasks.md:65` 的 4.4 只映射 TP1；逐包 Coverage 复算 WP1=17、WP2=29、WP3=29、WP4=19、WP5a=12、WP5b=10、WP6=31、WP7=22、TP1=46、TP2=34、TP3=37、TP4=14，与 Round 12 记载一致）。

### 三、结论

**PASS。**

理由：Target Revision 经实测与给定值逐字一致；本轮唯一改动（F13 的 Contract Freeze 单元格规范化）与由此新增的 9 条 Serialization Reason 经独立复核**成立且彻底**——plan 阶段 8 个 contract 包全部 `frozen=true`，premerge 阶段 Contract Freeze 类 error 为 0（改动前实测 3 条），premerge 的单元级冻结面重放 0 错误；9 个包的波次下移与理由码一一对应、前提逐条成立；依赖类型声明（code/contract/resource）无越界、无环、无波次矛盾；连续性 6 项全部自洽。**不存在未解决的 CRITICAL/MAJOR**，故按验收规则判 PASS。本轮新发现 F47/F48/F49 三项 MINOR，均不改变任何批次、依赖边、写范围、检查归属与交付单元 Order，不阻断 apply 派发；建议与观察 3 一并在下一修正轮作纯文本收敛。

## 实际检查范围

- 版本与门禁：`workflow check --stage plan --json`（退出码 1，planningDigest 与给定值一致，errors 仅 DDR 绑定项）、`workflow check --stage premerge --json`（errors 1，freeze 类 0）。
- `plan.md` 全文 1065 行：Work Packages 12 行（Dependencies/Contract Freeze/Write Scope/Owner/Role/Reviewer/Verification 逐格）、Execution Waves 12 行 + 散文依据、Shared File Ownership 18 行、Dependency Handoffs 10 行、Runtime Resources 5 行、Merge Strategy 8 单元、Coverage Index 叙述 22 行与 `agentic-coverage` 102 行、Project Verify 7 行、Independent Validation、Contract Changes、Failure and Recovery、Completion Criteria。
- `tasks.md` 129 行复选框任务（§6 的 6.1–6.63、三处唯一标记、`[wp:]` 标签、独立检视任务文本）；`verification.md` 的 Target/Handoff/DDR 相关行；5 份 delta spec 的 Requirement/Scenario 标题与计数；`proposal.md`/`design.md` 的行为口径；`openspec/agentic.yaml` 的 `dispatch.pool`。
- 门禁源码：`node_modules/@dongfanglin/openspec-agentic/src/workflow-check.mjs`（`frozenContractReadable` 91-98、`parseReason` 60-67、`parseDependencies` 82-89、frozen/hardDeps/earliest 197-222、波次与理由 223-345、独占资源与写重叠 347-400、review/validation 161-195、`premergeReviewScope` 526-541、coverage 1376-1425、`checkConcurrencyProbe` 1292-1340）、`planning-model.mjs`（37-81）、`workflow-contract.mjs`，以及 assets 下的 `templates/plan.md`、`procedures/workflow-check.md`、`procedures/acceptance.md`、`schema.yaml`。
- 历史对照证据：`reports/merge-mu2-r2.md:153-155`、`reports/ddr-sync-scope-and-pwa-client-r1[012].md`、`verification.md` 的相关记载。

## 未验证内容

1. **未执行任何构建、测试、Project Verify 或 E2E**（PV1/PV2/PV3/IV1/AC1–AC3 的实际运行状态不在本审查范围）；结论只覆盖静态结构、引用与门禁判据。
2. **无法取得 plan.md 的逐行版本 diff**：`git status --porcelain` 显示 `openspec/changes/sync-scope-and-pwa-client/` 未被跟踪，`git log --all -- <plan.md>` 无输出，故「本轮改了什么」无法以 diff 证明。9 包波次下移的前后对照系依据 ①「本轮新补 Serialization Reason 的恰为同 9 包」、②Round 12 报告自述的 pre-F13 earliest 值、③`merge-mu2-r2.md` 的 3 条 freeze 错误记载 交叉推断，而非逐字 diff；F13 修正本身的**有效性**则由 plan/premerge 两个阶段的门禁实测直接证明（非推断）。
3. **未复核 9 条理由码的「语义合理性」以外的事实**（如 MU1a 是否真的必须先于 WP5a 合入、MU2 是否真的先于 MU3a）：`schema.yaml` 明确「理由前提之外的『是否合理』由独立 reviewer 判断」，本轮只核对门禁前提与声明的自洽性；不排除存在未被声明为依赖的真实前置。
4. **未读取 `reports/` 下 30 个 `.log` 与历史交付报告的内容**（`PV1.log`/`PV2.log`/`AC1.log` 的哈希由门禁产出，未核对日志内容本身）；未审计 `verification.md` 中 Round 8 之前的 Review Findings 行是否与各轮原始报告逐条对齐（仅核对与本轮范围相关的行）。
5. **未验证交付单元在 apply 阶段的实际行为**：本审查为开工前门禁，不证明调度、并发窗口、worktree 交接或合并流程在运行时成立。
6. **未修改任何规划文件、代码或 `verification.md`**；本报告是唯一新增产物。
