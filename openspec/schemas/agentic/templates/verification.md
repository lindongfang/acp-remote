<!-- 主 Agent 持续汇总实际执行证据，供交付检查和 final-verification 使用。
     当前策略与安排在 plan.md，步骤与进度在 tasks.md；本文件保存执行及变更历史。
     记录与判定规则见 schema.yaml 的 apply instruction 和 procedures/acceptance.md。
     每条记录有唯一记录 ID，关联任务 ID（及任务文档版本）、WP/TP 或交付单元、Check ID/E2E ID；
     Review ID、测试报告 ID 和问题 ID 沿用来源报告；原始日志/报告以可读取的版本化路径引用，不全文复制。
     同一记录可被多处引用；复验新增轮次，不覆盖原失败、版本或失效判断。 -->

## Target

<!-- 变更名、仓库绝对路径、约定本地/远端主分支准确引用、版本确认负责人及核实方式；
     各次执行固定自身提交及基线。文档提交与代码提交不同时说明关系。
     本文件是 apply 期间的证据记录，不是开始实现前的必备 artifact。 -->

## Checks

<!-- 实现工作包关联 roles/coder.md 的交接记录：实际执行者、上下文方式、输入版本、
     固定起点/交付提交、写入范围、检查和修复报告。
     Project Verify 按计划 Check ID 逐项记录实际完整命令、工作目录、代码及脚本/配置版本、
     环境、退出码和完整日志路径；统一入口关联子检查结果。
     复用时明确标注"复用"、原证据及当前适用性，不冒充本次执行；保留失败和复验历史。
     计划含附加独立验证时，补记实际验证 Agent ID、隔离设置、roles/validator.md 及输入和报告。 -->

| Check ID / Stage / Work Package | Revision / Base | Scope | Executor | Command / Steps | Environment | Result / Exit Code | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| <!-- 如 PV1 / WP1 Project Verify --> | <!-- 实际版本 --> | <!-- 检查范围 --> | <!-- 执行者 --> | <!-- 完整命令、目录或观察步骤 --> | <!-- 环境/构建版本 --> | <!-- PASS / FAIL / BLOCKED / NOT_APPLICABLE --> | <!-- 完整日志、子检查结果或复用依据 --> |

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

<!-- 记录独立集成 Agent 的实际 ID、上下文继承方式、独立分支/worktree、输入材料及报告路径，
     以及向下一单元推进前的检查证据。 -->
<!-- 每个交付单元记录：规划确定的模式及就绪复核、候选对应测试与关键 E2E 及独立 review、
     本地合入前基线复核与防竞态机制、本地主分支实际提交、结果一致性、必要回归。
     远端操作另记明确授权依据、实际远端引用与结果；本地合入本身不附带 push。 -->

## Test Design and Authoring

<!-- 记录 TP ID、任务 ID、设计/编写报告 ID、测试 Agent ID、隔离设置和输入版本；关联需求与 E2E ID、
     用例/脚本提交、基础检查记录及非作者 reviewer 的 Review ID。已有用例保留稳定 ID。
     仅设计完成时明确尚未编写或检查的部分，不据此宣称测试工作包已交付。 -->

## Candidate E2E

<!-- 按交付单元记录候选关键范围和实际结果；逐 ID/尝试明细写在本节下方 `## Main E2E` 内的共享表，
     候选行在 `E2E ID / Attempt` 列以 candidate 标识，两阶段共用一张表；
     目标是"最新主分支 + 本交付单元"的固定候选，不是用例作者分支。 -->

## Main E2E

<!-- 候选与最终两阶段均固定代码/用例提交、产物标识或哈希、配置版本、环境及依赖版本，
     并记录各分片允许的资源命名空间差异。 -->
<!-- 引用 Test Design and Authoring 的共享记录；记录执行批次/分片、分配的 E2E ID、
     共同的代码/用例/运行构建版本和每分片独立报告路径，按 ID/版本/尝试序号检查遗漏、重复、
     混合版本和资源污染。最终完整 E2E 是单入口：分片在 x-agentic.e2e.command 内部并行，只写一条
     stage=final 记录；候选可按交付单元/分片分别留证，由主 Agent 汇总。 -->
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

<!-- 除逐轮历史外，保留唯一 agentic-assessment 块指向当前轮次。先完成语义审计，再填写它；
     运行 workflow check --stage plan --json 得到当前 contractDigest 和已存在证据的 sha256，
     将审计通过的报告路径/摘要填入 evidence，可增加覆盖索引之外的 review/资源报告。
     不能为消除失效提示直接刷新摘要：必须先按原问题完成影响分析、复核/复验并保留历史。
     final 阶段运行 workflow check --stage final，只允许最终验收自身待办；PASS 后才勾选该任务。
     归档前运行 --stage archive，要求所有任务完成。两种检查均只读，不自动合并或归档。
     摘要检测内容变化，不证明报告真实性、用户授权或角色独立性。 -->
```agentic-assessment
assessment_id: "<当前验收轮次 ID>"
target_commit: "<核实的目标完整提交>"
contract_digest: "sha256:<workflow check 输出的契约摘要>"
result: BLOCKED
evidence:
  - path: reports/PV1.log
    sha256: "sha256:<已审计原始报告的摘要>"
```

<!-- 按 acceptance.md 执行，记录失败与阻断项、证据失效/复用判断、重跑结果，
     以及 /opsx:verify 的一致性结论；单独记录证据验收 PASS/FAIL/BLOCKED，不能用 CLI all_done 代替。
     每轮验收新增记录并标明当前轮次，保留历史；旧 PASS 不自动适用于新版本。 -->

- Assessment ID / Time: <!-- 唯一轮次、验收时间及执行者 -->
- Target / Task: <!-- 准确目标引用及提交、目标核实证据、最终验收任务 ID -->
- CLI State: <!-- CLI 原始状态、查询时间及原始输出引用；不改写 all_done 含义 -->
- Audit / Evidence: <!-- 各审计组结论、有效证据记录 ID、失效/复用判断；原始报告以引用留存 -->
- Result / Open Issues: <!-- PASS/FAIL/BLOCKED；问题 ID、受阻范围、非阻断项处理结论 -->
- Required Follow-up: <!-- 待复验或修正的任务；PASS 仅对本轮目标及有效证据成立 -->
