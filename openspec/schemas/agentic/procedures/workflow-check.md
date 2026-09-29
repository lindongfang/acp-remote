# Workflow Structural Check

计划门同时检查项目 E2E 配置和 `### Main E2E` 决策：`mode` 必须为 `required` 或 `not-applicable`；后者必须填写 `reason`、`basis`、非空 `alternative_checks`，且项目 E2E 启用时必须填写 `downgrade_approval`。配置解析错误使检查 BLOCKED。最终 E2E 门只接受显式 `stage=final` 的执行记录；未标阶段和候选记录均不满足最终门。

`openspec-agentic workflow check --change <name> --stage plan|premerge|final|archive --json`
始终只读，退出码 0 表示结构 PASS，1 表示不一致，2 表示材料/解析/目标不可用（BLOCKED）。
工作目录是代码仓库，`--planning-root` 指向权威规划根；变更位置仍由引擎解析。

`premerge` 在候选提交工作区执行，要求 `HEAD` 为候选提交、本地 `target_ref` 仍指向规划基线，且候选包含该基线。`verification.md` 中的唯一 `agentic-premerge` 块记录候选 Project Verify、独立 review 的报告路径和摘要（并记录 `delivery_unit`）；`## Worktree Handoff` 必须覆盖已开工工作包；**无论 `## Premerge History` 是否有行、是否有当前候选的行，都要先核对该 `delivery_unit` 全部工作包的就绪**（独立 review、ready-to-merge/merged 台账、绑定 WP 与作者的 DELIVERY PASS）；若已填写 `## Premerge History` 则一并核对 receipt（必须是合法 agentic-premerge 块，candidate/target/contract_digest/requirements_digest/delivery_unit 与行一致，该单元每个工作包有独立 review、ready-to-merge/merged 台账与 DELIVERY PASS）。该行在 premerge PASS 之后才写入，由 final 强制并逐行读取 receipt 核对内容、证据摘要与目标版本。单元级候选阶段不执行 E2E，也不接受任何候选 E2E 记录：合入判据是候选 Project Verify、独立 review 与主 Agent 的 Coverage Index 覆盖核对结果。E2E 只在全部单元合入后由最终门核对，且只认一条聚合 `stage=final` 记录与项目配置的聚合命令；`not-applicable` 时最终门只核对 mode、三字段与降级批准，逐项替代检查由计划任务、verification 的 `## Checks`（每个 ID 一行 PASS 与可读证据）与最终验收审计。任一证据失效均非零退出。示例 CI 模板位于 `ci/github-premerge.yml`；使用方需复制到 `.github/workflows/` 并将该检查设为受保护分支的必需状态。机器只能核对报告结构、版本和摘要，reviewer 身份与真实测试内容仍需平台审批和人工审查。

```agentic-premerge
version: 1
delivery_unit: <计划中的交付单元 ID；required 时必填>
target_ref: refs/heads/main
target_commit: <合入前目标提交 SHA>
candidate_commit: <候选 HEAD SHA>
contract_digest: <workflow check --stage plan 输出的摘要>
requirements_digest: <行为契约摘要：proposal + specs；final 用它判断历史合入是否仍成立，执行安排（plan/tasks）变化不影响>
verify:
  result: PASS
  candidate_commit: <候选 HEAD SHA>
  evidence: {path: reports/candidate-verify.log, sha256: "sha256:<报告摘要>"}
review:
  result: PASS
  candidate_commit: <候选 HEAD SHA>
  reviewer: <独立检视人 ID>
  author: <代码作者 ID>
  evidence: {path: reports/candidate-review.log, sha256: "sha256:<报告摘要>"}
# mode 为 not-applicable 时，逐项列出计划中的 alternative_checks：
alternative_checks:
  - name: <计划中的检查名>
    result: PASS
    candidate_commit: <候选 HEAD SHA>
    evidence: {path: reports/alternative.log, sha256: "sha256:<报告摘要>"}
```

| 阶段 | 输入与完成条件 |
| --- | --- |
| plan | proposal 的 agentic-intent、plan 的 agentic-coverage、design、规范/既有契约、tasks；校验意图字段、源标题、需求/场景覆盖、任务和检查引用、三个门禁标记（[e2e-owned] / [final-verification] / [validation]）各唯一且分开，Main E2E 决策完整（not-applicable 时 alternative_checks 必须是可引用的检查 ID 且在任务中以 [ID] 声明）；并校验执行计划：工作包依赖类型（code / contract / resource）、Execution Waves 静态层级与 (WP, 层级) 的 Serialization Reason（仅 code-dependency / contract-unfrozen / resource-exclusive / capacity 四个码，写入重叠不构成串行理由；capacity 需池上限 < 2 或同最早层级同角色工作包数超出池上限）、独占资源互斥、写入重叠的 Shared File Ownership 登记（含合入顺序覆盖全部写入者、重跑项引用计划 Check ID）、Work Packages 的 Role 列取值（若存在只接受 coder / tester）、Reviewer 非空且不等于 Owner、required 时至少一个 tester 工作包、每个工作包有独立 review 任务、Independent Validation 表与唯一 [validation] 任务、每个工作包恰好一个带 [wp:WPn] 的派发任务、Dependency Declaration Review；并要求 verification.md 的 `## Dependency Declaration Review` 逐条 PASS、Reviewer 非工作包 Owner、`Review ID + Round` 一致、Plan Revision 绑定当前 contractDigest、报告可读；同一 Review ID 多条记录按**最大 Round**取当前结论（旧格式仅单条兼容，多条缺 Round 拒绝）。证据可尚不存在（依赖声明审查报告除外）。 |
| premerge | plan 的全部要求，加当前候选提交、未移动的目标基线、候选 Verify、独立 review 报告摘要（干净 rebase 可按改动内容指纹复用）与 Coverage Index 覆盖核对结果；**单元级候选阶段没有 E2E**，不核对任何候选 E2E 记录；`## Worktree Handoff` 需按 (WP, Attempt) 覆盖已开工工作包的每轮尝试（worktree 与计划一致、基线可核实、Provisioner 必须来自已登记的 provisioner 交接行且其报告被 Worktree Handoff 引用、Executor 绑定该轮认领执行者而非 reviewer/merger 接管者、Received At 不晚于该轮首次执行事件（首次 coding / 重开 fixing）、旧记录缺 Attempt 时仅能唯一映射到单轮才兼容）；若已填写 `## Premerge History` 则核对其 receipt 为合法 agentic-premerge 块且字段与行一致；并至少校验已填写的 Dispatch Reconciliation 行（含 Evidence 路径可读）、台账流转合法与台账 role 与计划 Role 一致、可选 dispatch-records.jsonl 的自洽性，并在已开工工作包中存在可并发组时要求至少一对同角色占用窗口真实重叠；同层同时有 coder 与 tester 已开工时，还需至少一对跨角色重叠窗口。 |
| final | 上述全部，加 verification 的唯一 agentic-assessment；HEAD、计划目标引用和验收提交一致；契约/原始证据摘要一致；只允许最终验收任务待办；只读 e2e check PASS；`## Dependency Declaration Review` 仍绑定当前契约摘要；`## Worktree Handoff` 按 (WP, Attempt) 覆盖全部已开工工作包的每轮尝试；每个 Merge History 行在 `## Premerge History` 有对应 Candidate 的 PASS，且其 receipt 为合法 agentic-premerge 块、candidate/target/contract_digest/delivery_unit/证据摘要与行及目标版本一致，交付单元一致；交付单元工作包集合与计划 Merge Strategy 的 WP / TP 一致；Dispatch Reconciliation 覆盖全部工作包、Attempt/Executor/State/Evidence 与 dispatch-queue.jsonl 及 Handoff Index（Executor / Agent）一致、最终状态为 merged（计划里已删除的工作包须以 superseded 保留对账行）；Merge History 至少一行，且同一目标分支单一 Merger、Candidate 是 Merged 的祖先、Merged 在目标引用上；Review Findings 覆盖全部工作包（Reviewer 不得为该 WP 的 Owner、阻断项需 Resolution）；台账事件 version/at/attempt 自洽。 |
| archive | final 的全部要求，且包括最终验收在内的所有任务完成；检查发生在归档操作之前。 |

输出 contractDigest 是 proposal、design、plan、tasks、元数据、全部增量规范和覆盖索引引用的契约
内容摘要；任务复选框进度不参与。evidence 返回已存在的计划报告的路径和 SHA-256，供审计后留存。
规范标题/任务描述/计划变化都会使旧契约失效。required E2E 每轮执行记录保存该摘要，旧记录未绑定时需重跑。
规划字段应在正式验证前固定；后续用例/任务调整先做影响分析，不为刷新摘要而跳过复验。

路径相对 changeDir（也可绝对路径），证据应使用独立且可持久读取的版本化报告文件，不引用
verification.md 自身，避免自引用摘要。当前验收块只保留一个，原轮次正文和原始报告持续保留。
检查不写报告、不勾选任务、不运行 E2E；E2E owned 行仍由 e2e check 独占更新。

兼容旧变更：旧 e2e 命令在没有 agentic-coverage 时保留既有行为；新 workflow check 不默默放行，
报告缺失材料。恢复旧变更时依据可核实资料补齐，不编造历史授权或证据。
plan 阶段现在要求 `## Dependency Declaration Review`（绑定当前 contractDigest）；premerge/final 要求 `## Worktree Handoff` 与 `## Premerge History`；
not-applicable 的 `alternative_checks` 需在任务中以 [ID] 声明并在 `## Checks` 逐项 PASS。在途变更按提示补齐这些节后再继续。
在途变更补上旧的 plan.md 会因缺 Work Packages / Execution Waves 而先判不合格；
这与“老变更缺少 plan 或验收任务时先补齐”一致，先按提示补齐并重过 plan 检查再继续实现。
旧变更走到 final/archive 还需在 verification.md 补 Dispatch Reconciliation 表（覆盖每个工作包，
Executor 登记到 Handoff Index 的 Executor / Agent）与 dispatch-queue.jsonl 台账；
台账缺失、状态不是 merged、流转非法或同一工作包重复开工都会在该阶段不合格。
旧格式兼容：`## Worktree Handoff` 缺 `Attempt` 时，仅在该工作包只有一轮尝试（能唯一映射）时放行，
多轮尝试必须逐轮补录，不得猜测；`## Dependency Declaration Review` 缺 `Round` 时，仅在该 Review ID 只有一条记录时兼容。
旧 Handoff Index 缺 `Work Package` 列时无法核对交付证据归属，premerge/final 会拒绝，须补列后重跑。
旧 premerge receipt 缺 `requirements_digest` 时，仅在全文 `contract_digest` 未变时兼容；全文摘要已变则拒绝，须迁移该凭据（补 `requirements_digest`）或补充可核对的复验依据。
旧 plan.md 里的 Host Parallel Dispatch / maxConcurrency 行已不再使用，可直接删除；
不再有“宿主不支持并发”的降级通道（默认宿主可并发）。
Contract Freeze 的版本号、Serialization Reason 与 Dependency Declaration Review 的文本语义不在机械校验范围：
版本号只是人读标识，理由前提之外的“是否合理”和声明是否属实由独立 reviewer 判断。

机器只比对本地 Git 引用；计划中的目标须为已核实的本地主分支 refs/heads/*。
合入入口应在验证后使用固定提交和条件更新防竞态；检查本身不锁定目标分支。
检查不会验证真实用户批准、角色隔离、断言语义或部署实例身份，这些仍按 acceptance.md 审计。
内容摘要可检测变化，不是签名或防篡改证明。

受控归档入口应先执行 --stage archive，只有退出码为 0 且语义验收仍有效才调用上游 archive。
CI 必须针对指定 --change 检查，不能用“全部变更扫描 PASS”代替。该扩展不修改上游 archive/merge，
直接调用上游命令仍可能绕过；项目需在自身受控入口/分支保护中配置强制调用。

E2E 用语统一：每轮最终完整 E2E 只运行一个聚合入口，分片在入口内部执行；失败修复后可以新开一轮，
仍遵守重试上限。四个顺序步骤分别是执行、汇总和清理、[e2e-owned] 门禁检查、[final-verification] 验收。
“执行一次”从不表示整个变更只能尝试一次；门禁行不承担执行或汇总任务。
