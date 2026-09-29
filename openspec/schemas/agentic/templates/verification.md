<!-- 主 Agent 持续维护执行证据与变更历史；判据见 schema.yaml 的 apply instruction 和 procedures/acceptance.md。
     每条记录有唯一记录 ID，关联任务 ID（及任务文档版本）、WP/TP 或交付单元、Check ID/E2E ID；
     Review ID、测试报告 ID 和问题 ID 沿用来源报告；原始日志/报告以可读取的版本化路径引用，不全文复制。
     同一记录可被多处引用；复验新增轮次，不覆盖原失败、版本或失效判断。 -->

## Target

<!-- 记录变更名、仓库路径、目标主分支准确引用与核实方式、各次执行提交/基线；
     文档与代码提交不同时说明关系。scout 的 recon 行（phase=recon、commands / observations）
     在此登记核实目标引用与当前提交的原始命令、原始输出及提交值，无法核实时写明 BLOCKED 依据；
     合入前的目标基线核对属 merger，不记在本节。 -->

## Handoff Index

<!-- 按 roles/_shared/role-report.md 引用每份角色报告；每个证据 ID、阶段和版本单独成行。
     报告路径相对权威 changeDir 或为绝对路径；REUSED 引用原 ID/路径/版本与适用依据；INVALID/PENDING
     保留旧行、受影响任务和复验要求，同次证据跨角色冲突记录处理结论。
     Work Package 填该证据对应的工作包（DELIVERY / REVIEW 行必填，环境、规划行写 NOT_APPLICABLE）；
     Round 对检视行填线程内轮次（`Review ID + Round` 为唯一键），非检视行写 NOT_APPLICABLE。
     Executor / Agent 填实际子 Agent ID，供 Dispatch Reconciliation 交叉核对。 -->

| Task ID | Work Package | Role / Phase / Stage | Round | Executor / Agent | Target Revision | Evidence Type / ID | Report Path | Result / Evidence Status | Applicability / Source Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| <!-- tasks.md ID --> | <!-- WP/TP；非工作包行写 NOT_APPLICABLE --> | <!-- role / phase / stage，取 roles/_shared/role-report.md 的枚举表 --> | <!-- 检视轮次或 NOT_APPLICABLE --> | <!-- 实际 Agent ID --> | <!-- 固定提交或 contractDigest --> | <!-- 每 ID 一行 --> | <!-- 本角色报告 --> | <!-- PASS/FAIL/BLOCKED；NEW/REUSED/INVALID/PENDING --> | <!-- 差异依据、复用原证据或待补项 --> |

## Dependency Declaration Review

<!-- plan.md 的 Dependency Declaration Review 字段列出 Review ID；本表逐条记录独立 reviewer 的 PASS、
     Round（检视线程内轮次）、Plan Revision（= workflow check --stage plan 输出的 contractDigest）与原始报告路径；
     计划或契约变化后该行失效。Review ID 复用 roles/reviewer.md 的稳定 ID，通过 Round 关联复判；
     同一 Review ID 多条记录按**最大 Round**取当前结论（旧格式仅单条兼容，多条缺 Round 拒绝，不能按行序选）。 -->

| Review ID | Round | Reviewer | Plan Revision | Result | Report Path |
| --- | --- | --- | --- | --- | --- |
| <!-- plan.md 列出的 Review ID --> | <!-- 线程内轮次，从 1 起 --> | <!-- 非工作包 Owner --> | <!-- sha256:<contractDigest> --> | <!-- PASS --> | <!-- 原始报告路径 --> |

## Checks

<!-- 按 Check ID 记录实际完整命令、目录、代码/脚本/配置版本、环境、退出码、日志及统一入口子检查；
     关联 coder 交接和适用的独立验证报告。复用标明原证据与当前适用性，保留失败/复验历史。
     Main E2E 为 not-applicable 时，每个 alternative_check 在此有一行 PASS 与可读证据。 -->

| Check ID / Stage / Work Package | Revision / Base | Scope | Executor | Command / Steps | Environment | Result / Exit Code | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| <!-- PV1 / WP1 --> | <!-- 提交/基线 --> | <!-- 覆盖范围 --> | <!-- 人/Agent --> | <!-- 完整命令、目录或步骤 --> | <!-- 环境/产物版本 --> | <!-- PASS / FAIL / BLOCKED / NOT_APPLICABLE --> | <!-- 原始日志/复用依据 --> |

## Check Plan Changes

<!-- 记录需求/接口澄清的影响分析与 specs、design、plan、tasks、用例的同步版本；检查范围或选择变更时写原/新值、
     理由、风险覆盖、受影响任务及独立 review 引用。代码、用例、产物、配置、环境变化时列出失效证据和重开任务；无调整时写明无。 -->

## Dependency Handoffs

<!-- 全部上游已验收提交、下游实际起点、引入方式及包含关系证据；
     上游更新时受影响的传递下游、基线更新、重开任务、失效/复用判断及复验结果。 -->

## Runtime Resources

<!-- 各检查实际使用的数据库/schema、端口、容器、账号及可写目录等命名空间；
     共享资源记录独占使用的开始/结束、负责人和释放结果，发生污染时关联失效检查及重跑证据。
     无共享运行资源时记录依据，不保留空占位。 -->

## Worktree Handoff

<!-- 由 main 汇总登记（provisioner 只返回结构化记录，不直接写本文件）：按 (WP, Attempt) 每轮尝试一行，
     保留历史，记录 worktree、基线提交、provisioner、本轮认领执行者与开工前接收时间。
     Received At 不得晚于该轮首次执行事件（首次为 coding，重开为 fixing）；Executor 绑定本轮实现者/测试作者，
     不随 reviewer/merger 接管窗口变动。merger 复用本单元已有的执行 worktree，不新建。
     旧记录缺少 Attempt 时仅在能唯一映射到单轮执行时兼容，多轮必须逐轮补录。 -->

| Work Package | Attempt | Worktree | Baseline Revision | Provisioner | Executor | Received At | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| <!-- WP/TP --> | <!-- 尝试轮次，从 1 起 --> | <!-- plan 的 Branch / Worktree --> | <!-- 固定提交 --> | <!-- provisioner ID，不得等于 Executor --> | <!-- 本轮认领执行者（作者） --> | <!-- 该轮开工前时间戳 --> | <!-- provisioner 报告 --> |

## Dispatch Reconciliation

<!-- 按 plan.md 的 Work Packages 逐包对账：Attempt 为当前轮次，Executor 与 Handoff Index 一致，
     State 与 dispatch-queue.jsonl 一致；退役旧包保留 superseded 行及原因。
     final/archive 要求全部现役工作包为 merged；premerge 校验已填写行。
     并发窗口以台账时间戳判定，不证明实际并行；工作包状态必须落盘。 -->

| Work Package | Attempt | Executor | State | Evidence |
| --- | --- | --- | --- | --- |
| <!-- WP1 --> | <!-- 1 --> | <!-- 当前实例的 Agent ID --> | <!-- 与台账一致的当前状态 --> | <!-- 台账/报告路径 --> |

## Review Findings

<!-- 每次检视/复核记录 Review ID、Work Package、任务 ID、检视阶段、实际子 Agent ID、检视类型、base/target、
     隔离方式、输入材料、报告位置及 PASS/FAIL/BLOCKED。Work Package 是机器契约：final/archive 要求每个 WP 至少一行，
     且 Reviewer 不得是该 WP 的 Owner；CRITICAL/MAJOR 为阻断问题，必须有 Resolution；**复核复用原 Review ID**，
     同一 ID 的每轮各占一行并按 Revision 降序排列，逐项给出原问题 ID 的依据；待补 Check ID 及应补齐的门禁另列。 -->

| ID | Work Package | Revision | Reviewer | Location | Severity / Impact | Resolution | Recheck Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| <!-- 问题 ID；无问题时明确写无 --> | <!-- WP/TP ID，供逐工作包核对 --> | <!-- 检视版本 --> | <!-- 非对应代码作者 --> | <!-- 文件与行 --> | <!-- 影响及是否阻断 --> | <!-- 修复或处理理由 --> | <!-- 复核结果 --> |

## Merge History

<!-- 每次合入前按 procedures/workflow-check.md 放置唯一 agentic-premerge 块；记录 merger 身份、
     候选门禁、基线复核、防竞态合入、主分支结果及回归证据。旧报告与合入历史保留。 -->

| Merge ID | Delivery Unit | Target Ref | Merger | Candidate Commit | Merged Commit | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| <!-- M1 --> | <!-- 单元 --> | <!-- refs/heads/main --> | <!-- 合入执行者 ID --> | <!-- 候选提交 --> | <!-- 合入后提交 --> | <!-- 报告/记录路径 --> |

<!-- 每个交付单元一次本地合入一行；机械校验：单一 Merger、Candidate 是 Merged 的祖先且 Merged 在目标引用上；final/archive 要求至少一行。 -->

## Premerge History

<!-- 每个交付单元在 premerge PASS 后登记一行；receipt 为当时持久化的 agentic-premerge 块，
     其行为契约摘要（requirements_digest）仍等于当前值即成立；plan/tasks 等执行安排变化不使其失效。
     Work Packages 与计划的单元组成一致；final 核对 receipt、Merge History 与对应版本。 -->

| Merge ID | Delivery Unit | Work Packages | Target Commit | Candidate Commit | Result | Receipt Path |
| --- | --- | --- | --- | --- | --- | --- |
| <!-- M1 --> | <!-- 单元 ID --> | <!-- 该单元全部 WP/TP --> | <!-- 合入前目标提交 --> | <!-- 候选提交 --> | <!-- PASS --> | <!-- receipt 报告路径 --> |

## Test Design and Authoring

<!-- 记录 TP ID、任务 ID、设计/编写报告 ID、测试 Agent ID、隔离设置和输入版本；关联需求与 E2E ID、用例/脚本提交、
     基础检查记录及非作者 reviewer 的 Review ID。已有用例保留稳定 ID；仅设计完成时明确尚未编写或检查的部分。
     design-author 作者提交产物即释放实例；execute/retest 每次独立派发新建实例（不是每条用例/每次调用新建），
     从持久材料恢复上下文：当前最终主分支、用例/产物版本、本轮 E2E ID 与资源，retest 还附原问题 ID、失败证据、
     修复提交与 review 结论。每轮只允许一次聚合 E2E 入口调用。 -->

## Independent Validation

<!-- 本表必须至少一行，覆盖计划 `## Independent Validation` 的每个验证任务；Result 只允许 PASS / FAIL / BLOCKED，
     final/archive 要求每个任务都有 PASS 记录；Report Path 与计划对应，并注明 Project Verify PV1（主分支只读核对）。 -->

| Task | Target Revision | Result | Report Path |
| --- | --- | --- | --- |
| <!-- tasks.md 的 [validation] 任务 ID --> | <!-- 验收的固定目标提交 --> | <!-- PASS / FAIL / BLOCKED --> | <!-- 独立报告路径 --> |

## Main E2E

<!-- 仅记录最终主分支 E2E：每轮一条聚合 stage=final 记录，按 E2E ID / Attempt 留逐次证据；
     固定代码、用例、产物、配置和环境版本，记录分片资源、真实入口、就绪及清理结果。
     引用项目开关、Main E2E mode 与测试设计；降级引用批准来源。
     not-applicable 记录 reason、basis、alternative_checks，E2E 标为 NOT_APPLICABLE，替代检查写入 Checks，并删除下表。 -->

| E2E ID / Attempt | Requirement / Case Version | Executor / Time | Runtime Version / Entry | Actions / Expected / Actual | Result | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| <!-- 用例及尝试序号 --> | <!-- 需求、用例路径/版本 --> | <!-- 实际执行者/时间 --> | <!-- 主分支产物及真实入口 --> | <!-- 步骤与逐项断言/观察 --> | <!-- PASS / FAIL / BLOCKED；跳过保留原因并阻断必要场景 --> | <!-- 报告/日志/适用截图或轨迹 --> |

## Failures and Retests

<!-- 测试失败、环境受阻及其他执行问题沿用来源报告问题 ID，无来源 ID 时分配唯一编号；审查发现引用
     Review Findings 的原问题及闭环记录，不重复登记。保留原失败/受阻及每次复测，当前状态须有明确依据；
     代码/用例修复关联独立 review，环境恢复关联就绪证据。无问题时明确写无。 -->

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
