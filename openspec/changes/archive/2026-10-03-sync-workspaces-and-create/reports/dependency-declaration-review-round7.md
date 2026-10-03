# Dependency Declaration Review — reviewer-plan (Round 7)

- Review ID：`reviewer-plan` · Round：7 · Phase / Stage：`phase: plan` / `stage: plan`
- Reviewer：独立只读子 Agent（非任何工作包 Owner；本轮仅用 `read`/`grep`/`glob`，未执行构建、测试或 `git diff`）
- 复核基线：Round 6 的 PASS 结论绑定 `sha256:19bc702f1c78f625f6a048c5c37e0cb8ca1f822c9d00b71ecd3830657a5c4cfe`（`verification.md:60`；本轮不从该摘要继承任何结论）
- 声明的待审变更：`plan.md` 的 `### Main E2E` yaml 块中 `downgrade_approval` 的取值由占位文字替换为用户批准记录（`plan.md:386`）

## Verdict

**PASS**。本轮唯一变更是 `plan.md:386` 一个 yaml 字段的取值替换，**范围受限、语义保持**：E2E 决策本身（`mode: not-applicable`、`reason`、`basis`、`alternative_checks: [ALT1, ALT2]`）逐字未动，替换只把「批准待补」这一占位状态固化为「已获批准 + 原话 + 时间 + 来源 + 限定范围」。无 BLOCKER / MAJOR；无新增 MINOR 级别的计划缺陷（新增 3 条均为**账本登记滞后**，不改变任何门禁判据）。Round 6 承接的 5 条 MINOR/SUGGESTION 全部仍未修复，原样承接、不因此判 FAIL。

## 本轮变更核对

### 变更内容：单点、单字段

| 项 | Round 6 基线内容 | 当前内容 | 证据 |
| --- | --- | --- | --- |
| `downgrade_approval`（`### Main E2E` yaml 块第 5 个字段） | `待用户批准：…需记录用户原话、时间与来源（当前缺失，计划在补齐前无效）` | `已获用户批准。来源：本会话（2026-10-03）。用户原话：「同意 sync-workspaces-and-create 的 Main E2E 记为 not-applicable（2026-10-03，本会话）」。批准范围：仅本变更 sync-workspaces-and-create 的 Main E2E 判为 not-applicable，由替代检查 ALT1（npm run check）与 ALT2（cargo test --locked --workspace --all-features）在最终主分支承担覆盖；不构成对其它变更或对仓库 e2e 开关配置的批准。` | `plan.md:386` |

### 变更受限（同轮锚点核对，行号零漂移）

- `plan.md` 全文仍为 **408 行**（末行 `plan.md:408` 为 Completion Criteria 末条）。Round 6 报告记录的全部锚点在本轮仍指向同一内容：`:6`（Scope 行）、`:281-285`（R53 覆盖行）、`:292-295`（四个 WP 行）、`:297`（Dependency Declaration Review 行）、`:303-306`（四条波次）、`:312-317`（六行共享文件）、`:323-324`（两条交接）、`:332-333` 与 `:335`（资源表与 `R1`/`R2` 定义）、`:342`（Target Ref）、`:350`（Merge Strategy 行）。**行号零漂移 ⇒ 本轮编辑未增删任何行，只可能改写行内字符**，而本轮逐行重读全文（`:1-300`、`:299-408`）显示唯一变化行即 `:386`。
- `tasks.md` 仍为 59 行：任务 1.1–1.5（`:6-10`）、2.1–2.4（`:14-21`）、3.1–3.5（`:25-29`）、4.1–4.2（`:33-34`）、5.1–5.8（`:38-45`）、6.1（`:49`）、7.1–7.3（`:53-55`）、8.1（`:59`）逐条与 Round 6 一致；勾选状态不变（7.3、8.1 仍未勾选）。
- `verification.md` 未变：`## Dependency Declaration Review` 表最大 Round 仍为 6（`:60`）；`## Checks` 行为 `:66` PV1、`:67` ALT1、`:68` ALT2、`:69` PV0；`## Main E2E` 为 `:206`。
- 其余规划件（`proposal.md`、`design.md`、`specs/**` 五份增量、`dispatch-queue.jsonl`）本轮**未重新全量比对**；对 `plan.md` 的依赖声明而言其相关锚点经 grep 复核仍在（`design.md` 的 D1/D6 引用见 `plan.md:294`/`:295`；13 条 Requirement / 40 条 Scenario 的标题清单见下文 MINOR-3）。

### 语义保持（决策本身未被改动）

- `mode: not-applicable` 未变（`plan.md:382`）；`reason`（`:383`）、`basis`（`:384`）、`alternative_checks: [ALT1, ALT2]`（`:385`）逐字未变。
- 机器解析面：`e2e-check.mjs:14-21` 的 `MAIN_E2E_HEADING`/`NEXT_HEADING`/`FIELD_LINE` 与 `REQUIRED_NOT_APPLICABLE_FIELDS = ['reason','basis','alternative_checks']` 对当前块的判定不变；`e2e-check.mjs:35-58` 的 `cleanValue` 只在引号外找 `#`、并剥掉成对引号 —— 新值整体被双引号包裹、内部无 `#`、无未转义的 `"`，故解析出的取值非空，`:119-123` 的 `enabled && !downgrade_approval` 分支由「非空占位」变为「非空批准记录」，`approval: true` 的语义成立，且 `:117` 的三个必填字段不受影响。
- **观察（非缺陷）**：占位文本本身也非空，故 `assessMainE2EDecision` 在本轮之前同样会返回 PASS —— 机械门禁从未因占位而 FAIL。本轮变更的价值不在门禁，而在满足 `openspec/config.yaml:19`「需用户逐变更批准并写入 plan.md 的 downgrade_approval（原话、时间、来源）」与 `openspec/config.yaml:54` 归档 guidance「not-applicable 须有……用户降级批准」这两条**非机械**要求。

## 批准记录判定

**判定：满足 schema 与项目规则的批准记录，且范围限定明确、无越权授权。**

| 要求 | 出处 | 记录中的对应内容 | 结论 |
| --- | --- | --- | --- |
| 记录**用户自己的话** | `openspec/config.yaml:19`（原话、时间、来源）；归档规则 `openspec/config.yaml:54` | `用户原话：「同意 sync-workspaces-and-create 的 Main E2E 记为 not-applicable（2026-10-03，本会话）」`（`plan.md:386`） | 满足：逐字引号内即为用户语句，非概括、非推断 |
| 记录**时间** | 同上 | `2026-10-03`（出现两次：来源句与原话括注），与本会话日期一致 | 满足 |
| 记录**来源** | 同上 | `来源：本会话（2026-10-03）` | 满足：来源为会话而非其它变更的历史批准 |
| 批准**不得被推定** | `openspec/config.yaml:19`「逐变更批准」 | 记录以「已获用户批准 + 原话」形式给出，未从项目级 2026-09-23 确认或其他变更的批准沿用（对比归档变更的写法 `archive/2026-09-24-acp-boundary-and-agent-host/plan.md:616` 显式声明「本记录不沿用 2026-09-23 的首次确认」） | 满足：本变更取本次当次批准 |
| **范围不得被静默放大** | 记录自述 `批准范围：仅本变更 …；不构成对其它变更或对仓库 e2e 开关配置的批准` | 同左 | 满足：范围限定到「本变更 Main E2E 判为 not-applicable」，并显式排除「其它变更」与「仓库 e2e 开关配置」两类越权授权 |

**与计划其余部分的一致性核对（记录本身是否说了计划没做的事）**：

- 记录声称本变更是「合同与类型层变更、无可驱动产品路径」→ 与 `plan.md:383` 的 `reason`、`plan.md:384` 的 `basis`（`openspec/config.yaml` context + `openspec/agentic.yaml:15-16` 的 `e2e.enabled: true` / `command: ""`）一致。
- 记录声称替代覆盖由 ALT1（`npm run check`）与 ALT2（`cargo test --locked --workspace --all-features`）承担 → 与 `plan.md:385`、`:389-390`、`tasks.md:53-54` 的 `[ALT1]`/`[ALT2]` 任务、`verification.md:67-68` 的 `## Checks` 行三方一致，无第三个或缺失的替代检查。
- 记录**未**授权任何计划之外的动作：不涉及合入、推送、e2e 开关配置、其它变更；`tasks.md:55`（7.3 `[e2e-owned]`）与 `tasks.md:59`（8.1）仍为未勾选，未被记录提前宣称完成。

## 受影响结论复核

| 项 | 结论 | 本轮独立证据 |
| --- | --- | --- |
| E2E 决策字段齐备且 `mode` 未被改动 | CONFIRMED | `plan.md:381-387` 的 yaml 块：`mode: not-applicable`（`:382`）、`reason`（`:383`）、`basis`（`:384`）、`alternative_checks: [ALT1, ALT2]`（`:385`）、`downgrade_approval`（`:386`）；`e2e-check.mjs:21` 的必填集三项齐备、`:119-121` 的 approval 非空条件由 `:386` 满足 |
| `alternative_checks` 仍是计划声明的两个 ID，且被登记为任务 | CONFIRMED | `plan.md:385` 与 `:389-390`（ALT1/ALT2 两条说明行）；`tasks.md:53`（7.1 `[ALT1]`）、`tasks.md:54`（7.2 `[ALT2]`），两条均已勾选且各自写明命令与证据路径；全变更目录内 `\[ALT` 只命中这两行（grep 结果） |
| 两个 ID 都有 `## Checks` 行 | CONFIRMED | `verification.md:67`（ALT1：`npm run check`，Node v22.22.0，PASS，证据 `reports/main-alt1-check.log`）、`verification.md:68`（ALT2：`cargo test --locked --workspace --all-features`，cargo 1.98.1，PASS，证据 `reports/main-workspace-test.log`）；两者均钉在 `refs/heads/main@a6f6210`，与 `verification.md:191` 的说明一致（最终判定由 7.1/7.2 在主分支重跑后写入 Checks） |
| 替代检查的证据路径是否可读 | 部分可读（如实记录） | `glob reports/*.log` **无结果**：`reports/` 下无任何 `.log`。这与仓库策略一致（`.gitignore:22` 的 `*.log` 与 `.gitignore:23-27` 的显式说明「原始证据日志不入库……归档记录已给出命令、环境与结果摘要」），但 `verification.md:67-68` 的两行未像 `verification.md:19/22/32` 那样注明这一点（见 MINOR-3） |
| `not-applicable` 与「无 tester 工作包」一致 | CONFIRMED | `plan.md:350` 的 Owners 单元格「测试 NOT_APPLICABLE: 本单元无 tester 工作包（Main E2E 为 not-applicable，替代检查 ALT1/ALT2 由主 Agent 任务承担）」；`tasks.md` 全文件 grep `[tp:` 与 `tester` **零命中**，2.1–2.4 均为 `[wp:WPN]`（`:14`/`:16`/`:18`/`:20`），7.1/7.2 归「全变更、主 Agent」（`:53-54`）；`verification.md:195`「本变更不设 tester 工作包（TP）」同向 |
| 依赖图仍无环、`code:WP3` 声明仍属实 | CONFIRMED | 声明侧：`plan.md:295`（WP4 Dependencies = `code:WP3`）、`:306`（W2 进入条件）、`:323-324`（两条交接，含预期红窗口与收口方）、`tasks.md:9`（1.4「无 `code:WP3` 则不得开工」）、`:20-21`（2.4「必须等 WP3 进入已验收集成基线」「依赖：`code:WP3`」）。真值侧（代码只读核实）：WP3 侧 `crates/core/src/ports.rs:195` 定义 `SessionUpdate.workspace_alias`，`crates/core/src/broker.rs:1441-1470` 在创建提交里写入别名原文；WP4 侧 `crates/storage-sqlite/src/migrate.rs:19-21`（版本 = 6）、`:652-654`（`ALTER TABLE owned_session ADD COLUMN workspace_alias TEXT`）、`crates/storage-sqlite/src/session_store.rs:67-68`（`LEFT JOIN owned_workspace`）、`:1167-1168`（绑定 `update.workspace_alias`）确实消费 WP3 的端口形状 ⇒ 边方向与真值一致 |
| WP↔范围（工作量）陈述仍属实 | CONFIRMED（含既有措辞滞后） | `plan.md:292`（WP1：R18–R32、R37–R39）、`:293`（WP2：R1–R17）、`:294`（WP3：R33–R36）、`:295`（WP4：R33–R36、R40–R52）与覆盖索引一致（R53 映射到 2.3/2.4，见 `plan.md:283`，但未出现在 WP3/WP4 的 Goal 文本中 → 见 SUGGESTION-4，承接项） |
| 波次、所有权、交接、资源、合并顺序未受影响 | CONFIRMED | `plan.md:303-306`（W1: WP1/WP2/WP3，W2: WP4，理由全 `NOT_APPLICABLE`）；`plan.md:312-317`（六行共享文件及 Merge Order：`WP2 → WP1`、`WP2 → WP3`、`WP4 → WP3`）；`plan.md:350` 的 Order `WP2 → WP1 → WP4 → WP3` 无环且满足全部三条约束；`plan.md:332-333`/`:335`（`R1` 构建目录逐 WP 隔离、`R2` `node_modules` 只读共享，`resource:R1` 绑定见 `:297`/`:335`） |
| Coverage Index 未受影响 | CONFIRMED | `plan.md:17-286` 的 `agentic-coverage` 块行数与内容与 Round 6 相同（本轮逐行重读 `:1-300`），R53 仍为 `plan.md:281-285`；覆盖行未引用 `downgrade_approval`，故本轮变更不影响任何覆盖映射 |

## 发现的问题

**无 BLOCKER / MAJOR。以下 3 条为本轮新增，全部是账本登记滞后，不改变任何门禁判据，也不影响计划有效性。**

1. **MINOR（本轮新增，登记滞后）** `verification.md:206` 的 `## Main E2E` 仍写「降级批准：见 plan.md 的 `downgrade_approval` 字段（**待用户确认后替换为原话、时间与来源**）」，而 `plan.md:386` 已落地真实原话/时间/来源。两处表述现已矛盾，最终验收人按 `verification.md` 阅读会得到「批准仍缺」的过期结论。
   最小修复：把 `verification.md:206` 末尾括注替换为「降级批准：已记录于 `plan.md:386`（用户 2026-10-03 本会话原话「同意 sync-workspaces-and-create 的 Main E2E 记为 not-applicable」；范围仅限本变更）」。

2. **MINOR（本轮新增，登记滞后）** `verification.md:53-60` 的 `## Dependency Declaration Review` 表最大 Round 仍为 6（`:60`，绑定 `sha256:19bc702f…`）。`plan.md:386` 改动后契约摘要必然移动，门禁要求 Plan Revision 等于当前 `contractDigest`。
   最小修复：主 Agent 追加一行 `reviewer-plan | 7 | reviewer-plan | <按当前计划重算的摘要> | PASS | reports/dependency-declaration-review-round7.md`。本轮只读，未计算摘要。

3. **MINOR（本轮新增，仅表述）** `verification.md:67-68` 的 ALT1/ALT2 证据路径（`reports/main-alt1-check.log`、`reports/main-workspace-test.log`）在库内**不可读**（`glob` 无 `.log`）。这是 `.gitignore:22` 与 `.gitignore:23-27` 明示的既定策略，不是缺陷；但与 `verification.md:19`/`:22`/`:32` 对工作树日志「未入库」的显式注明相比，这两行缺同样的注明，归档阶段易被误判为证据缺失。
   最小修复：在 `verification.md:69` 前补一句「ALT1/ALT2 原始日志按 `.gitignore` 的 `*.log` 策略不入库，凭据为上表命令、环境与结果摘要（可在 `a6f6210` 上复现）」。

**承接项（Round 5/6 已登记，本轮复核仍未修复，原样承接）：**

4. **MINOR-3** `plan.md:6` 仍写「共 13 条 Requirement / **39** 条 Scenario」；本轮按 `specs/**` 的 `^#{3,4} (Requirement|Scenario):` 重新计数为 **13 / 40**（scope-expansion 1+2、storage-schema-v2-migration 2+11、sync-session-create 5+10、sync-workspace-catalog 4+13、workspace-resolution 1+4）。最小修复：`plan.md:6` 的 `39` 改为 `40`。
5. **MINOR-1** `specs/storage-schema-v2-migration/spec.md:55-58` 的场景 `WHEN`（`:57`「一个会话创建请求携带已登记且解析成功的 workspace 别名」）仍未含「取得 ACP 会话标识」，与 R53（`specs/workspace-resolution/spec.md:14-17`，未取得标识时三列同为空）在「别名已解析但无 ACP 标识」输入上仍互斥；需求正文 `:53` 亦无门限。最小修复：`:53` 补同门口径，或 `:57` 的 `WHEN` 补「，且本次创建取得了 ACP 会话标识」。
6. **MINOR-2** `specs/workspace-resolution/spec.md:11` 的 `WHEN` 仍未带门限（可由同需求正文 `:7` 消解，仅措辞）。
7. **SUGGESTION-4** `plan.md:294`（WP3 Goal `R33–R36 的 core 侧`）与 `plan.md:295`（WP4 Goal `R33–R36、R40–R52 的存储侧`）仍未列 R53，尽管覆盖索引已把 R53 映射到 `tasks: ["2.3","2.4"]`（`plan.md:283`）。
8. **SUGGESTION-5** `crates/sync-protocol/tests/envelope_fixtures.rs`（WP1 与 WP2 双写）与 `crates/core/src/model/tests.rs`（WP3 写）仍未登记进 Shared File Ownership / WP3 写范围；收口规则已在 `verification.md:49` 给出，属下一版计划的工作。
