# Dependency Declaration Review — reviewer-plan (Round 6)

- Review ID：`reviewer-plan` · Round：6 · Phase / Stage：`phase: plan` / `stage: plan`
- Reviewer：独立只读 reviewer 子 Agent（非任何工作包 Owner；本轮只用 `read`/`grep`/`glob` 与只读文本提取，未执行构建、测试或 `git diff`）
- 复核基线：Round 5 的 PASS 结论绑定 `sha256:d477497ed80a20db8a7aa4e073d06bd7b7c0661c22c6ccdd5e8e81f842c3da3b`（`verification.md:58`）；本轮对象是**该摘要之后**的唯一契约变更。
- Round 5 不是权威：本轮对第 (2)(3)(4) 组结论逐条独立重取证据，只在证据与 Round 5 一致处沿用其表述。

## Verdict

**PASS**。本轮唯一变更（Merge Strategy 的 `WP / TP` 单元格）范围受限、语义保持，且**恰好消除了 premerge 门禁的机器解析故障**；所有依赖声明、波次、所有权、覆盖索引、红窗口与 E2E 结论均维持。无 BLOCKER、无 MAJOR、无新增 MINOR。Round 5 登记的 5 条 MINOR/SUGGESTION 全部为**尚未修复的既有措辞/登记滞后**，本轮原样承接，不因此判 FAIL。

## 本轮契约变更核对

### 变更内容（单点，且仅一处表格行）

变更只落在 `plan.md` 的 `## Merge Strategy` 数据行 `plan.md:350` 的两个单元格：

| 单元格 | Round 5 内容 | 当前内容 | 证据 |
| --- | --- | --- | --- |
| `WP / TP`（第 2 列） | `WP1, WP2, WP3, WP4；无 TP（Main E2E 非 required）` | `WP1, WP2, WP3, WP4` | `plan.md:350` cell2（按 `\|` 切分的第 2 格，恰好 6 格，与表头 `plan.md:348` 的 6 列一一对应） |
| `Owners: Implementation / Test / Review / Merge`（第 3 列） | `实现 coder-vocab/coder-wire/coder-core/coder-storage；测试 NOT_APPLICABLE…；review reviewer-vocab/…；merge merger-1` | 同左，并**逐字承接**「本单元无 tester 工作包（Main E2E 为 not-applicable，替代检查 ALT1/ALT2 由主 Agent 任务承担）」 | `plan.md:350` cell3 |

该行其余四格与本次改动无关，逐字未动：`Delivery Unit / Mode` = `U1（integrated）`（cell1）、`Start / Readiness` = 「契约冻结于 `design.md@D1–D8`；WP4 需 `code:WP3` 进入集成基线」（cell4）、`Candidate Checks / Critical E2E` = `PV1（npm run verify）` + 关键路径三项（cell5）、`Order` = `WP2 → WP1 → WP4 → WP3` + 与 Merge Order 一致的括注（cell6）。

### 变更是被门禁解析器证实的必要修正（不是可选美化）

门禁源码逐字确认了 Round 5 之后 premerge 报障的机理：

- `node_modules/@dongfanglin/openspec-agentic/src/workflow-check.mjs:710-715`：`Merge Strategy` 表只按表头名取 **`Delivery Unit / Mode`** 与 **`WP / TP`** 两列，`unitWps.set(unit, new Set(checkTokens(mergePlan.pick(cells, 'WP / TP'))))`。
- `workflow-check.mjs:507`：`const checkTokens = (value) => String(value ?? '').split(/[,，、\s/]+/)…` —— 分隔符只有 `, ， 、 空白 /`，**不切分中文分号 `；` 与全角括号**。故旧文本 `WP1, WP2, WP3, WP4；无 TP（Main E2E 非 required）` 产出 8 个 token：`WP1`、`WP2`、`WP3`、`WP4；无`、`TP（Main`、`E2E`、`非`、`required）`，与门禁报告的垃圾条目完全一致。
- `workflow-check.mjs:749-755`：`checkCurrentUnitReadiness()` 用 `readiness([...planned], unit)` 逐 WP 要求 Review Findings 行与台账状态（`ready-to-merge`/`merged`）；垃圾 token 会去查不存在的工作包并直接 `fail`。`:754` 另有「Merge Strategy 未声明其 WP / TP」的兜底。
- 当前 cell2 `WP1, WP2, WP3, WP4` 经同一 `checkTokens` 得到恰好 `{WP1, WP2, WP3, WP4}` 四个 token，与 `## Work Packages` 的 ID 集合（`plan.md:292-295`）**完全相等**：不多、不少、无垃圾。
- 另经全仓检索确认：门禁**从不读取** `Owners` 列做 token 解析（`grep 'Owners'` 在 `node_modules/@dongfanglin/openspec-agentic/src` 下只命中 `workflow-check.mjs:300/301` 的注释残留与其他不相关标识，`e2e-check.mjs` 只解析 Main E2E yaml 字段）。故把说明文字迁入 `Owners` 列对机器解析零风险。

### 变更范围受限、其余文件未动

- `plan.md` 全文 **408 行**；Round 5 引用的每一个行锚点在本轮仍然指向同一内容：`:6`（Scope 行）、`:281-285`（R53）、`:292-295`（四个 WP）、`:297`（Dependency Declaration Review 行）、`:303-306`（四条波次）、`:312-317`（六行共享文件）、`:323-324`（两条交接）、`:332-335`（资源表与 `R1`/`R2` 定义）、`:342`（Target Ref）、`:350`（Merge Strategy 行）。行号零漂移 ⇒ 本轮编辑**没有增删任何行**，只可能改写行内字符。
- 本轮逐行重读了 `plan.md` 全部 408 行（`:1-43`、覆盖索引块逐字段 `:21-285`、`:286-403`、`:404-408`）：除 `:350` 外无内容变化。
- `proposal.md`（74 行）、`design.md`（155 行）、`tasks.md`（59 行）、`specs/**`（5 份增量）本轮全部重读：Round 5 引用的锚点逐字不变 —— `design.md:38`（D1 取舍）、`design.md:105`（`workspace_alias` 同形窄写入）、`design.md:106`（LEFT JOIN）；`tasks.md:9`（1.4 的 WP4 交接）、`tasks.md:14/16/18/20`（四个 `[wp:WPN] [PV1]`）；`specs/workspace-resolution/spec.md:7`（同门写入正文）、`:14-17`（R53 场景）；`specs/storage-schema-v2-migration/spec.md:7`、`:24-27`、`:51-53`、`:55-58`。
- `proposal.md` 的 Round 5 报告未给出行级锚点（Round 5 只在 Round 1 处置中提及过它），故对其「未变」的判定依据是：本轮通读 74 行内容与 Round 5 的描述性结论（不涉及 WP/波次/合并）无冲突，且它不含任何 `Merge Strategy` 或依赖声明文本（`grep 'Merge Strategy|WP / TP|Owners'` 在本变更目录下只命中 `plan.md:348` 与归档变更）。
- **判定：变更受限、语义保持（semantics-preserving）。** 「无 TP / Main E2E not-applicable / 替代检查 ALT1/ALT2」这一事实陈述被完整保留在 `Owners` 列，读者可见性与信息量不减；被移除的只是机器不可解析的重复标注。

## 受影响结论复核

### (2) 可能被 Merge Strategy 改动扰动的结论

| 项 | 结论 | 本轮独立证据 |
| --- | --- | --- |
| 交付单元的工作包集合恰为 `{WP1, WP2, WP3, WP4}` | CONFIRMED | `plan.md:350` cell2 字面 `WP1, WP2, WP3, WP4`；`workflow-check.mjs:507` 的 `checkTokens` 对该串产出恰好 4 个 token；与 `plan.md:292-295` 的 WP ID 集合相等；与 `## Execution Waves` 的 WP1/WP2/WP3/WP4 相等（`plan.md:303-306`） |
| `Owners` 列仍完整分配实现/测试/评审/合并，无自相矛盾 | CONFIRMED | `plan.md:350` cell3：实现 = `coder-vocab/coder-wire/coder-core/coder-storage` 四人 ↔ `plan.md:292-295` 的 Owner 列一一对应；review = `reviewer-vocab/reviewer-wire/reviewer-core/reviewer-storage` ↔ 同表 Reviewer 列；merge = `merger-1` ↔ `tasks.md:38`、`:39`、`:40`、`:43`（5.1/5.2/5.3/5.6 的「U1（合入负责人 merger-1）」）；测试 = `NOT_APPLICABLE`，与 `plan.md:294`/`:295` 的 Verification 列（各 WP 自带局部检查）、`plan.md:366`（PV1）与 `tasks.md:14-21` 的 `[PV1]` 标记一致，**不与任何 tester 角色声明冲突**；补入的括注「Main E2E 为 not-applicable，替代检查 ALT1/ALT2 由主 Agent 任务承担」与 `plan.md:382-386`、`plan.md:389-390`、`tasks.md:53-54` 的 `[ALT1]`/`[ALT2]` 一致 |
| `Start / Readiness` 未变且与依赖声明相符 | CONFIRMED | `plan.md:350` cell4 = 「契约冻结于 `design.md@D1–D8`；WP4 需 `code:WP3` 进入集成基线」；`plan.md:292-295` 的 Contract Freeze 列引用的 `@D1,D2` / `@D1,D6` / `@D3,D4,D5,D7,D8` / `@D6` 全部 ⊆ `D1–D8`；`plan.md:306`（W2 Enter Condition）「WP3 进入已验收集成基线」与之同向 |
| `Candidate Checks / Critical E2E` 未变 | CONFIRMED | `plan.md:350` cell5 = `PV1` + 三条关键路径；`## Project Verify` 只有 `PV1` 一行（`plan.md:366`），且 `tasks.md:14/16/18/20/25/40` 均以 `PV1` 声明，无第二个候选检查 ID 可被遗漏 |
| `Order` = `WP2 → WP1 → WP4 → WP3` 与 `## Shared File Ownership` 每一条 Merge Order 相容 | CONFIRMED | 共享文件表的四条 Merge Order：`WP2 → WP1`（`plan.md:312` `docs/SYNC_PROTOCOL.md`、`:313` `fixtures/sync/v1/`、`:316` `crates/sync-protocol/src/`、`:317` `fixtures/sync/v1/manifest.json`）、`WP2 → WP3`（`plan.md:314` `crates/server/src/node_link/`）、`WP4 → WP3`（`plan.md:315` `docs/CORE_PORTS_AND_STORAGE.md`）。约束集 {WP2<WP1, WP2<WP3, WP4<WP3}；`WP2 → WP1 → WP4 → WP3` 满足全部三条（WP1<WP4 为自由边），且该序**无环** |
| WP4 的 `code:WP3` 由「WP3 是 WP4 head 的祖先」满足 | CONFIRMED（声明层面） | `plan.md:295`（WP4 Dependencies = `code:WP3`）、`plan.md:323`（交接行：候选必须包含 WP3 的交付提交，且基线上 `cargo test -p core` 与 `check-contract-drift.mjs` 通过）、`plan.md:306`（W2 进入条件）、`plan.md:350` cell4、`tasks.md:9`（1.4：核对 `crates/core` 端口形状的实际基线与包含关系；「无 `code:WP3` 则不得开工」）、`tasks.md:20`（2.4：「必须等 WP3 进入已验收集成基线」）、`tasks.md:21`（WP4 依赖行 `依赖：code:WP3`）。运行期「祖先」事实需在 5.x 由 merger 实测，本轮为规划期声明核对 |

### (3) Round 5 结论的低成本复查

| 项 | 结论 | 本轮独立证据 |
| --- | --- | --- |
| Coverage Index 仍为 53 行且格式良好 | CONFIRMED | 逐行枚举 `^  - id: R` 命中 **53** 处，`R1`…`R53` 连续无重号（`plan.md:21`–`:285`）；块头 `version: 1` / `target_ref: refs/heads/main`（`plan.md:18-19`）；每行四个字段齐全 —— 抽取 `^    (source\|tasks\|checks\|evidence):` 得 **53×4 = 212** 行，`evidence` 末行为 `plan.md:285`（R53） |
| Coverage Index 与五份增量规范标题双射 | CONFIRMED | 规范侧标题计数（`grep '^#{3,4} (Requirement\|Scenario):'`）：Requirement 13 条（`scope-expansion:5`、`storage-schema-v2-migration:5`/`:51`、`sync-session-create:9`/`:23`/`:37`/`:51`/`:65`、`sync-workspace-catalog:9`/`:38`/`:57`/`:76`、`workspace-resolution:5`），Scenario 40 条（2+11+10+13+4），合计 53。覆盖行按 `source.path`+`source.heading` 配对：`sync-workspace-catalog` 17 行 R1–R17、`sync-session-create` 15 行 R18–R32、`workspace-resolution` 5 行 R33–R36+R53、`scope-expansion` 3 行 R37–R39、`storage-schema-v2-migration` 13 行 R40–R52 —— 逐条命中上列标题，missing = 0、dups = 0、extra = 0 |
| `tasks` 引用均存在且带 `[PV1]` | CONFIRMED | 覆盖行只用 `2.1`/`2.2`/`2.3`/`2.4`；对应 `tasks.md:14`、`:16`、`:18`、`:20`，四条均以 `[wp:WPN] [PV1]` 声明；`checks` 全为 `[PV1]`，`PV1` 在 `plan.md:366` 定义、在 `tasks.md:53/54` 之外亦被 `tasks.md:25`、`:40` 引用 |
| `## Dependency Handoffs` 仍登记两条红窗口相关行 | CONFIRMED | `plan.md:323`（WP4←WP3，含关系与基线检查条件）、`plan.md:324`（集成窗口 storage-sqlite ← WP3，**预期红窗口**、收口方 WP4（W2））；`:326` 明写无其他代码依赖 |
| `## Runtime Resources` 仍定义 `R1`/`R2` | CONFIRMED | `plan.md:332`（`R1` = 构建目录 `CARGO_TARGET_DIR`，每 WP 独立 `target-wp<N>`，`Exclusive Scheduling = NOT_APPLICABLE`）、`plan.md:333`（`R2` = `node_modules`，只读共享，`NOT_APPLICABLE`，只读使用不写入）、`:335` 资源标识定义句把 `resource:R1` 显式绑定到 `R1` 行；`:297` 的 Dependency Declaration Review 行复述 `resource:R1` 的可隔离声明 |
| `## Work Packages` 仍有四个 WP，Reviewer 非空且不等于 Owner | CONFIRMED | 表头含门禁必需列 ID / Reviewer / Dependencies / Contract Freeze / Write Scope / Verification（`plan.md:290`；必需列清单见 `workflow-check.mjs:109`）；`plan.md:292` coder-vocab/reviewer-vocab、`:293` coder-wire/reviewer-wire、`:294` coder-core/reviewer-core、`:295` coder-storage/reviewer-storage —— 四条 Reviewer 均非空且与 Owner 不同；`plan.md:297` 的 `reviewer-plan` 独立于全部 Owner |
| `## Execution Waves` 未变（W1: WP1/WP2/WP3，W2: WP4，四条理由全 `NOT_APPLICABLE`） | CONFIRMED | `plan.md:303`（W1/WP1）、`:304`（W1/WP2）、`:305`（W1/WP3）四条 Serialization Reason 均为 `NOT_APPLICABLE`；`plan.md:306`（W2/WP4）同理。W1 三包依赖皆 `none`（`plan.md:292-294`），W2 唯一依赖 WP3（`plan.md:295`），无逆序 |
| `## Independent Validation` 表与唯一 `[validation]` 任务仍在 | CONFIRMED | `plan.md:394-396` 表头 5 列 + 唯一数据行 `6.1 [validation]`；`tasks.md:49` 的 6.1 带 `[validation]` 标记；全变更目录内 `[validation]` 只出现这 2 处（1 处表、1 处任务），无重复任务 |
| Main E2E 仍 `not-applicable` 且四个字段齐备 | CONFIRMED | `plan.md:381-387` 的 yaml 块含 `mode: not-applicable`（`:382`）、`reason`（`:383`）、`basis`（`:384`）、`alternative_checks: [ALT1, ALT2]`（`:385`）、`downgrade_approval`（`:386`）；`openspec/agentic.yaml:14-16` 的 `e2e.enabled: true` / `command: ""` 与 `basis` 的陈述一致；`downgrade_approval` 仍是占位文字（见「未决事项」），不影响计划有效性 |

### (4) 依赖图无环、声明属实

| 声明 | 结论 | 证据 |
| --- | --- | --- |
| 图无环 | CONFIRMED | `parseDependencies`（`workflow-check.mjs:80-87`）按 `[,，、]` 切分：`plan.md:292`/`:293`/`:294` 的 Dependencies = `none` → `[]`；`plan.md:295` = `code:WP3` → 一条边。全图唯一边 `WP4 → WP3`，WP1/WP2/WP3 无出边、无回边，无自环 |
| WP4 `code:WP3` 属实 | CONFIRMED | 声明侧证据同 (2) 表最后一行；真值侧：WP4 的写范围（`plan.md:295`：`crates/storage-sqlite/src/migrate.rs`、`session_store.rs`、`tests/`）消费 WP3 在 `crates/core/src/ports.rs` / `model/session.rs` / `broker.rs`（`plan.md:294`）定义的 `SessionUpdate.workspace_alias` / `SessionSummary.workspace`，契约冻结于 `design.md@D6`（`plan.md:295` Contract Freeze 列），方向与依赖边一致 |
| WP1–WP3 `none` 属实 | CONFIRMED | WP1/WP2 只经冻结契约耦合（`plan.md:292` `@D3,D4,D5,D7,D8`、`:293` `@D1,D2`；`plan.md:326` 明写「WP1 与 WP2 只经冻结契约耦合，不互读代码」）；WP3 只增 core 侧值对象与字段（`plan.md:294` `@D1,D6`），无任何 WP 的声明写范围落入 WP1/WP2 的写范围之外的新前置 |
| `resource:R1` 可隔离 | CONFIRMED | `plan.md:332` 逐 WP 独立 `target-wp<N>` 且由 provisioner 以环境变量注入、`Exclusive Scheduling = NOT_APPLICABLE`；`plan.md:297` 复述该声明；`tasks.md:8`（1.3）把分配/隔离列为可勾选任务 |
| `resource:R2` 只读共享 | CONFIRMED | `plan.md:333` 明确「只读共享（`npm ci` 已完成）」「只读使用，不写入」，`Exclusive Scheduling = NOT_APPLICABLE`；`plan.md:337` 声明无独占资源；`tasks.md:8` 另要求确认 `node_modules` 就绪 |

## 未决事项

以下均为**已如实登记、但不阻断本轮 PASS** 的开放项：

1. **Main E2E 降级批准占位（`plan.md:386`）**：`downgrade_approval` 现为「待用户批准：…需记录用户原话、时间与来源（当前缺失，计划在补齐前无效）」。`openspec/agentic.yaml:15` 为 `e2e.enabled: true`，故该占位在 final 阶段必须由真实用户原话替换，否则 `e2e check` 不可能 PASS。替代路径已就位（`plan.md:389-390` 的 ALT1/ALT2 + `tasks.md:53-54`）。**不影响计划有效性与本轮依赖声明结论。**
2. **本轮 PASS 尚未登记进 `verification.md`**：`verification.md:55-58` 的 `## Dependency Declaration Review` 表当前最大 Round 为 5（`:58`，绑定 `sha256:d477497e…`）。由于 `plan.md:350` 已改动，契约摘要必然移动；主 Agent 需新增 Round 6 行并写入**用当前计划重算**的摘要与本报告路径（门禁要求 Plan Revision 等于当前 `contractDigest`，见 `workflow-check.mjs:566-568`）。本轮只读，未执行摘要计算。
3. **Round 5 承接项（全部未修复，均为措辞/登记滞后，不影响任何门禁判据）**：
   - MINOR-1：`specs/storage-schema-v2-migration/spec.md:57-58` 的场景 `WHEN` 未含「取得 ACP 会话标识」，与 R53（`specs/workspace-resolution/spec.md:16-17`）在「别名已解析但无 ACP 标识」这一输入上互斥；需求正文 `:53` 无门限，单 delta 内无法消解。最小修复：`:53` 补同门口径，或 `:57` 的 `WHEN` 补「，且本次创建取得了 ACP 会话标识」。
   - MINOR-2：`specs/workspace-resolution/spec.md:11` 的 `WHEN` 未带门限（可由同需求正文 `:7` 消解，仅措辞）。
   - MINOR-3：`plan.md:6` 仍写「共 13 条 Requirement / **39** 条 Scenario」，实际为 13 / 40（R53 未回写该行）。
   - SUGGESTION-4：`plan.md:294`（WP3 Goal `R33–R36 的 core 侧`）与 `plan.md:295`（WP4 Goal `R33–R36、R40–R52 的存储侧`）均未列 R53，尽管覆盖索引已把 R53 映射到 `tasks: ["2.3","2.4"]`（`plan.md:283`）。
   - SUGGESTION-5：`crates/sync-protocol/tests/envelope_fixtures.rs`（WP1 与 WP2 双写）与 `crates/core/src/model/tests.rs`（WP3 写）尚未登记进 Shared File Ownership / WP3 写范围；收口规则已在 `verification.md:49` 给出（67 / 9 / 2 / 5，总 83），属**下一版计划**的工作。

## 发现的问题

**无新增问题。** 本轮唯一的计划改动（`plan.md:350` 的 `WP / TP` 单元格，及把同一句事实迁入相邻 `Owners` 单元格）是范围受限、语义保持的修正，并且正是 premerge 门禁 `checkTokens`（`workflow-check.mjs:507`、`710-715`、`749-755`）所要求的形式。Round 5 的 5 条 MINOR/SUGGESTION 原样承接于「未决事项」，本轮不重复列条、不因此判 FAIL。
