<!-- 主 Agent 持续维护执行证据与变更历史；判据见 schema.yaml 的 apply instruction 和 procedures/acceptance.md。
     每条记录有唯一记录 ID，关联任务 ID（及任务文档版本）、WP/TP 或交付单元、Check ID/E2E ID；
     Review ID、测试报告 ID 和问题 ID 沿用来源报告；原始日志/报告以可读取的版本化路径引用，不全文复制。
     同一记录可被多处引用；复验新增轮次，不覆盖原失败、版本或失效判断。 -->

## Target

<!-- 记录变更名、仓库路径、目标主分支准确引用与核实方式、各次执行提交/基线；
     文档与代码提交不同时说明关系。 -->

## Handoff Index

<!-- 按 roles/handoff.md 引用每份角色报告；每个证据 ID、阶段和版本单独成行。
     报告路径相对权威 changeDir 或为绝对路径；REUSED 引用原 ID/路径/版本与适用依据；
     INVALID/PENDING 保留旧行、受影响任务和复验要求；同次证据跨角色冲突记录处理结论。 -->

| Task ID | Role / Phase / Stage | Target Revision | Evidence Type / ID | Report Path | Result / Evidence Status | Applicability / Source Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| <!-- tasks.md ID --> | <!-- 角色、阶段 --> | <!-- 固定提交 --> | <!-- 每 ID 一行 --> | <!-- 本角色报告 --> | <!-- PASS/FAIL/BLOCKED；NEW/REUSED/INVALID/PENDING --> | <!-- 差异依据、复用原证据或待补项 --> |

## Checks

<!-- 按 Check ID 记录实际完整命令、目录、代码/脚本/配置版本、环境、退出码、日志及统一入口子检查；
     关联 coder 交接和适用的独立验证报告。复用标明原证据与当前适用性，保留失败/复验历史。 -->

| Check ID / Stage / Work Package | Revision / Base | Scope | Executor | Command / Steps | Environment | Result / Exit Code | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| <!-- PV1 / WP1 --> | <!-- 提交/基线 --> | <!-- 覆盖范围 --> | <!-- 人/Agent --> | <!-- 完整命令、目录或步骤 --> | <!-- 环境/产物版本 --> | <!-- PASS / FAIL / BLOCKED / NOT_APPLICABLE --> | <!-- 原始日志/复用依据 --> |

## Check Plan Changes

<!-- 记录需求/接口澄清的影响分析及 specs、design、plan、tasks、用例的同步版本；
     检查清单/脚本/排除项/测试选择变更时的原/新值、理由、风险覆盖、受影响任务及独立 review 引用。
     代码、用例、产物、配置、环境变化时列出失效证据和重开任务，复用关联原证据与差异。
     无调整时写明无；不删除原始失败记录。 -->

## Dependency Handoffs

<!-- 全部上游已验收提交、下游实际起点、引入方式及包含关系证据；
     上游更新时受影响的传递下游、基线更新、重开任务、失效/复用判断及复验结果。 -->

## Runtime Resources

<!-- 各检查实际使用的数据库/schema、端口、容器、账号及可写目录等命名空间；
     共享资源记录独占使用的开始/结束、负责人和释放结果，发生污染时关联失效检查及重跑证据。
     无共享运行资源时记录依据，不保留空占位。 -->

## Review Findings

<!-- 每次检视/复核记录：Review ID、任务 ID、检视阶段、实际子 Agent ID、检视类型、base/target、
     隔离方式、输入材料路径、报告位置及 PASS/FAIL/BLOCKED。
     CRITICAL/MAJOR 为阻断问题；复核注明新的 Review ID 并按原问题 ID 逐项给出依据。
     单独列出 reviewer 待补的 Check ID、是否影响审查判断、应补齐的门禁及主 Agent 核对结果。
     review PASS 不关闭待补检查；reviewer 不直接修改本文件。 -->

| ID | Revision | Reviewer | Location | Severity / Impact | Resolution | Recheck Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| <!-- 问题 ID；无问题时明确写明 --> | <!-- 检视版本 --> | <!-- 非对应代码作者 --> | <!-- 文件与行 --> | <!-- 影响及是否阻断 --> | <!-- 修复或处理理由 --> | <!-- 复核结果 --> |

## Merge History

<!-- 每次本地合入前按 procedures/workflow-check.md 放置唯一 agentic-premerge 块；
     固定候选/目标、契约摘要、候选检查与报告 SHA-256，required 时包含 stage=candidate E2E。
     新候选更新当前块，旧报告与合入历史保留。 -->

<!-- 记录独立集成 Agent 的实际 ID、上下文继承方式、独立分支/worktree、输入材料及报告路径，
     以及向下一单元推进前的检查证据。 -->
<!-- 每个交付单元记录：规划确定的模式及就绪复核、候选对应测试与关键 E2E 及独立 review、
     本地合入前基线复核与防竞态机制、本地主分支实际提交、结果一致性、必要回归。
     本流程仅记录计划中的本地合入结果。 -->

## Test Design and Authoring

<!-- 记录 TP ID、任务 ID、设计/编写报告 ID、测试 Agent ID、隔离设置和输入版本；关联需求与 E2E ID、
     用例/脚本提交、基础检查记录及非作者 reviewer 的 Review ID。已有用例保留稳定 ID。
     仅设计完成时明确尚未编写或检查的部分，不据此宣称测试工作包已交付。 -->

## Candidate E2E

<!-- 按单元记录“最新主分支 + 本单元”的固定候选、关键范围和结果；
     逐 ID/尝试明细在 Main E2E 共享表中标记 candidate。 -->

## Main E2E

<!-- 候选与最终两阶段均固定代码/用例提交、产物标识或哈希、配置版本、环境及依赖版本，
     并记录各分片允许的资源命名空间差异。 -->
<!-- 引用测试设计记录；按执行分片、E2E ID、共同版本与独立报告路径核对遗漏、重复及资源污染。
     最终只留一条聚合 stage=final 记录；候选可按单元/分片留证。 -->
<!-- 阶段汇总按唯一 E2E ID 核对计划、执行、通过、失败、受阻及跳过清单，重试次数单列。
     记录 `npx --quiet --no-install openspec-agentic e2e --json` 读到的项目开关值，引用 plan.md 的 Main E2E mode、
     适用性判断及变更历史；降级时引用 downgrade_approval 的批准来源；
     not-applicable 记录 reason、basis、alternative_checks 并将 E2E 标为 NOT_APPLICABLE，
     替代检查在 Checks 中逐项留证；无 E2E 时删除下表。 -->
<!-- required 时记录运行版本识别、平台及驱动、启动与就绪结果、真实依赖或外部替身、
     隔离数据和资源清理结果；人工验证记录执行人和时间。 -->
<!-- 下表由 Candidate E2E 与 Main E2E 共用，按 `E2E ID / Attempt` 列区分阶段（candidate / final）与尝试。 -->

| E2E ID / Attempt | Requirement / Case Version | Executor / Time | Runtime Version / Entry | Actions / Expected / Actual | Result | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| <!-- 用例及尝试序号 --> | <!-- 需求、用例路径/版本 --> | <!-- 实际执行者/时间 --> | <!-- 主分支产物及真实入口 --> | <!-- 步骤与逐项断言/观察 --> | <!-- PASS / FAIL / BLOCKED；跳过保留原因并阻断必要场景 --> | <!-- 报告/日志/适用截图或轨迹 --> |

## Failures and Retests

<!-- 测试失败、环境受阻及其他执行问题沿用来源报告问题 ID，无来源 ID 时分配唯一编号。
     审查发现引用 Review Findings 的原问题及闭环记录，不重复登记。
     保留原失败/受阻及每次复测，当前状态须有明确依据；代码/用例修复关联独立 review，
     环境恢复关联处理及就绪证据。无问题时明确写无。 -->

| Issue ID / Task | Source / Check or E2E ID / Attempt / Version | Owner | Fix / Recovery | Review / Readiness Evidence | Retest Evidence | Current Status / Basis |
| --- | --- | --- | --- | --- | --- | --- |
| <!-- 原问题 ID 及任务 --> | <!-- 原报告、失败或受阻证据 --> | <!-- 实际责任人 --> | <!-- 修复版本或恢复措施 --> | <!-- 适用 Review ID 或环境证据 --> | <!-- 各次复测记录及版本 --> | <!-- 已解决/未解决/无法确认及依据 --> |

## Final Assessment

<!-- 语义审计后保留唯一 agentic-assessment 块指向当前轮次；用 workflow check --stage plan --json
     取得当前 contractDigest 与报告 sha256，填入已审计证据路径/摘要。失效证据须先复核或复验，
     不得仅刷新摘要；final/archive 阶段与最终任务完成条件见 procedures/acceptance.md。 -->
```agentic-assessment
assessment_id: "<当前验收轮次 ID>"
target_commit: "<核实的目标完整提交>"
contract_digest: "sha256:<workflow check 输出的契约摘要>"
result: BLOCKED
evidence:
  - path: reports/PV1.log
    sha256: "sha256:<已审计原始报告的摘要>"
```

<!-- 按 acceptance.md 记录每轮验收、问题、证据失效/复用和当前结论；保留历史，
     证据验收 PASS/FAIL/BLOCKED 与 CLI 状态分别记录。 -->

- Assessment ID / Time: <!-- 唯一轮次、验收时间及执行者 -->
- Target / Task: <!-- 准确目标引用及提交、目标核实证据、最终验收任务 ID -->
- CLI State: <!-- CLI 原始状态、查询时间及原始输出引用；不改写 all_done 含义 -->
- Audit / Evidence: <!-- 各审计组结论、有效证据记录 ID、失效/复用判断；原始报告以引用留存 -->
- Result / Open Issues: <!-- PASS/FAIL/BLOCKED；问题 ID、受阻范围、非阻断项处理结论 -->
- Required Follow-up: <!-- 待复验或修正的任务；PASS 仅对本轮目标及有效证据成立 -->
