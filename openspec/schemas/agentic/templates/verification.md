<!-- main 从 propose 规划审查起维护证据，apply 续记执行/变更历史；判据见 tasks/apply instruction、procedures/acceptance.md。
     每条记录有唯一 ID，关联任务 ID/文档版本、WP/TP 或单元、Check/E2E ID；Review/测试报告/问题 ID 沿用来源。
     原始报告/日志用可读版本化路径引用，可多处共用，不全文复制；复验增轮，不覆盖失败、版本或失效判断。 -->

## Target

<!-- 记录变更名、仓库路径、目标主分支准确引用/核实方式、各次提交/基线，文档与代码提交不同时说明关系。
     scout recon（phase=recon、commands/observations）登记目标引用/当前提交的原始命令、输出、提交值；无法核实写 BLOCKED 依据。
     merger 的合入前基线核对不记本节。 -->

## Handoff Index

<!-- 按 roles/_shared/role-report.md 引用各角色报告，每个证据 ID/阶段/版本单独成行，路径相对权威 changeDir 或为绝对路径。
     REUSED 填原 ID/路径/版本与依据；INVALID/PENDING 保留旧行、受影响任务/复验要求；同次证据跨角色冲突记处理结论。
     Work Package：DELIVERY/REVIEW 必填对应 WP，环境/规划填 NOT_APPLICABLE。
     Round：检视线程内轮次，唯一键 `Review ID + Round`；非检视填 NOT_APPLICABLE。
     Executor / Agent 填实际子 Agent ID，与 Dispatch Reconciliation 核对。 -->

| Task ID | Work Package | Role / Phase / Stage | Round | Executor / Agent | Target Revision | Evidence Type / ID | Report Path | Result / Evidence Status | Applicability / Source Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| <!-- tasks.md ID --> | <!-- WP/TP；非工作包行写 NOT_APPLICABLE --> | <!-- role / phase / stage，取 roles/_shared/role-report.md 的枚举表 --> | <!-- 检视轮次或 NOT_APPLICABLE --> | <!-- 实际 Agent ID --> | <!-- 固定提交；规划新报告填 planningDigest，旧报告保留原 contractDigest --> | <!-- 每 ID 一行 --> | <!-- 本角色报告 --> | <!-- PASS/FAIL/BLOCKED；NEW/REUSED/INVALID/PENDING --> | <!-- 差异依据、复用原证据或待补项 --> |

## Handoff Receipts

<!-- workflow record 计算/登记原始结构化报告 SHA-256，禁手写替代；报告用独立版本路径。
     同次导入幂等，原路径内容变化拒绝覆盖；各 workflow 门核对哈希，旧报告无表按原规则核对。 -->

| Report Path | SHA-256 | Executor / Agent | Isolation |
| --- | --- | --- | --- |

## Dependency Declaration Review

<!-- propose 收尾建立本节、Target/Handoff Index 等适用节，apply 续记执行证据。
     逐个 plan 的 Dependency Declaration Review ID 记独立 reviewer PASS、线程 Round、CLI planningDigest 和原报告；
     计划/契约变化使该行失效。复用 roles/reviewer.md 稳定 ID，以最大 Round 取当前结论，不按行序；
     旧无 Round 格式仅兼容单条，多条拒绝。 -->

| Review ID | Round | Reviewer | Plan Revision | Result | Report Path |
| --- | --- | --- | --- | --- | --- |
| <!-- plan.md 列出的 Review ID --> | <!-- 线程内轮次，从 1 起 --> | <!-- 非工作包 Owner --> | <!-- CLI 返回的 planningDigest；旧记录保留原 contractDigest --> | <!-- PASS --> | <!-- 原始报告路径 --> |

## Checks

<!-- workflow record 导入适用索引、Checks/Handoff Receipts，哈希规则见上节。
     workflow reconcile 按当前台账/已登记报告刷新对账，不改原始报告或任务复选框。 -->

<!-- 每 Check ID 记完整命令/目录、代码/脚本/配置版本、环境、退出码、日志/统一入口子检查，关联 coder 交接/适用独立验证报告。
     复用记原证据/当前适用性，保留失败/复验；not-applicable 时每个 alternative_check 有 PASS 行/可读证据。 -->

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

<!-- provisioner 返回结构化记录，main 登记，子角色不写本文件；每 (WP, Attempt) 一行、保留历史，字段见表。
     Received At 不晚于本轮首次 coding/fixing；Executor 固定本轮实现者/测试作者，不随 reviewer/merger 接管变动。
     merger 复用本单元执行 worktree，不新建；旧缺 Attempt 仅唯一映射单轮时兼容，多轮须逐轮补录。 -->

| Work Package | Attempt | Worktree | Baseline Revision | Provisioner | Executor | Received At | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| <!-- WP/TP --> | <!-- 尝试轮次，从 1 起 --> | <!-- plan 的 Branch / Worktree --> | <!-- 固定提交 --> | <!-- provisioner ID，不得等于 Executor --> | <!-- 本轮认领执行者（作者） --> | <!-- 该轮开工前时间戳 --> | <!-- provisioner 报告 --> |

## Dispatch Reconciliation

<!-- 可选集成基线接收表位于 ## Integration Baselines：由 main 确认后 workflow record 导入 merger 的
     integration_baseline；字段与原始结构见 roles/_shared/role-report.md。旧变更无表继续按既有规则核实基线。 -->

<!-- 按 plan Work Packages 对账：Attempt 为当前轮，Executor 对 Handoff Index，State 对 dispatch-queue.jsonl；保留 superseded/原因。
     final/archive 全部现役包须 merged，premerge 校验已填行；状态落盘，窗口按台账时间戳，不证明实际并行。 -->

| Work Package | Attempt | Executor | State | Evidence |
| --- | --- | --- | --- | --- |
| <!-- WP1 --> | <!-- 1 --> | <!-- 当前实例的 Agent ID --> | <!-- 与台账一致的当前状态 --> | <!-- 台账/报告路径 --> |

## Review Findings

<!-- premerge：contract 依赖核对 Contract Freeze 路径@版本/可读性，不要求提供方实现 review/交付；
     当前单元/code 上游消费的契约未冻结或不可读仍禁合入。只要求当前单元/传递 code 上游最新轮 PASS、阻断闭环，
     无关单元 FAIL/BLOCKED 不阻断本单元；全表结构/身份/轮次仍须有效，final/archive 全量核对。 -->

<!-- 每次检视/复核记 Review ID、WP、任务/阶段、实际 Agent ID、类型、base/target、隔离、输入、报告、PASS/FAIL/BLOCKED。
     final/archive 每 WP 至少一行，Reviewer 非该 WP Owner；CRITICAL/MAJOR 必填 Resolution。
     复核复用原 Review ID，按 Review ID + Work Package 取最大正整数 Round，不按 Revision 排序；
     同轮多问题的 Result 须一致，新轮 FAIL/BLOCKED 不回退旧 PASS。保留历史，按原问题 ID 记闭环，另列待补 Check/门禁。
     手填/workflow record 共用下表；旧单轮可保留，多轮/导入前须迁移表头。 -->

| ID | Review ID | Round | Result | Work Package | Revision | Reviewer | Location | Severity / Impact | Resolution | Recheck Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| <!-- 原问题 ID；无问题写 NONE --> | <!-- 稳定 review ID --> | <!-- 正整数 --> | <!-- PASS / FAIL / BLOCKED --> | <!-- WP/TP ID --> | <!-- 检视版本 --> | <!-- 非对应代码作者 --> | <!-- 文件与行 --> | <!-- 影响及是否阻断 --> | <!-- 修复或处理理由 --> | <!-- 原始报告/复核证据路径 --> |

## Merge History

<!-- 每次合入前按 procedures/workflow-check.md 放置唯一 agentic-premerge 块；记录 merger 身份、
     候选门禁、基线复核、防竞态合入、主分支结果及回归证据。旧报告与合入历史保留。 -->

| Merge ID | Delivery Unit | Target Ref | Merger | Candidate Commit | Merged Commit | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| <!-- M1 --> | <!-- 单元 --> | <!-- refs/heads/main --> | <!-- 合入执行者 ID --> | <!-- 候选提交 --> | <!-- 合入后提交 --> | <!-- 报告/记录路径 --> |

<!-- 每次本地合入一行，含原单元的修复合入；Candidate 是 Merged 的祖先且 Merged 在目标引用上。
     同一时刻仅一个 Merger；身份更替必须补以下窗口与接管表，单一实例旧记录兼容。 -->

## Merger Windows

<!-- 启用接管时覆盖每个 Merge ID；Started At 为开始构建/处理候选时间，Released At 为完成合入及回归后释放时间。
     使用带时区 ISO 时间，窗口不得重叠；会话中断须从持久证据确认前任已停止并释放，不得假定超时等于释放。 -->
| Merge ID | Merger | Started At | Released At |
| --- | --- | --- | --- |

## Merger Takeovers

<!-- 每次相邻窗口的身份更替一行；Released At 等于前窗口结束，Received At 不早于释放且不晚于继任开工。
     Target Commit 固定交接时目标版本，包含前次 Merged 且为继任 Candidate 的祖先。
     Evidence 记录前任释放/停止、继任接收、目标核实及未完成工作；机械校验不证明进程互斥，语义验收核对原始证据。 -->
| Previous Merge ID | Next Merge ID | Target Ref | From | To | Target Commit | Released At | Received At | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |

## Premerge History

<!-- 每单元 premerge PASS 后登记；receipt 为当时持久化 agentic-premerge 块，requirements_digest 等于当前值即有效，
     plan/tasks 执行安排变化不使其失效；失效按 Receipt Revalidations 关联当前有效替代。
     Work Packages 与计划单元组成一致；final 核对 receipt/Merge History/版本。 -->

| Merge ID | Delivery Unit | Work Packages | Target Commit | Candidate Commit | Result | Receipt Path |
| --- | --- | --- | --- | --- | --- | --- |
| <!-- M1 --> | <!-- 单元 ID --> | <!-- 该单元全部 WP/TP --> | <!-- 合入前目标提交 --> | <!-- 候选提交 --> | <!-- PASS --> | <!-- receipt 报告路径 --> |

## Receipt Revalidations

<!-- 原/替代 receipt 缺 work_packages 时，从已核对 Premerge History 恢复，保留原文件；显式集合须与历史一致，不以恢复覆盖矛盾。 -->

<!-- 需求调整保留原 receipt/报告，旧摘要失效不改历史；每个失效 Merge ID 指向当前需求下已 review/Verify/合入的替代 ID。
     替代复用原 Delivery Unit、覆盖原全部 WP/TP、候选包含历史候选。
     Original SHA-256 填原 receipt 摘要；Approval Evidence 引用用户授权/影响分析；Revalidation Evidence 记原问题、失效范围、
     修复或无需代码修复理由、当前需求逐项 review/复验。旧关联证据留报告，表中失效 ID 直接指当前有效 receipt，禁循环/失效替代。
     最终 E2E/全变更验收仍绑当前版本。 -->
| Invalidated Merge ID | Replacement Merge ID | Original SHA-256 | Approval Evidence | Revalidation Evidence |
| --- | --- | --- | --- | --- |

## Test Design and Authoring

<!-- 记 TP/任务/设计编写报告/测试 Agent ID、隔离、输入版本；关联需求/E2E ID、用例脚本提交、基础检查、非作者 Review ID。
     既有用例保留稳定 ID，设计交付注明未编写/检查部分。
     design-author 交产物即释放；execute/retest 每轮独立新实例（非每用例/调用），从持久材料恢复最终主分支、
     用例/产物版本、本轮 E2E ID/资源；retest 附原问题 ID、失败证据、修复提交/review。每轮一次聚合 E2E 入口。 -->

## Independent Validation

<!-- 至少一行，覆盖计划 Independent Validation 每任务，Result 仅 PASS/FAIL/BLOCKED，final/archive 逐任务须 PASS；
     Report Path 对应计划，注明 Project Verify PV1（主分支只读核对）。 -->

| Task | Target Revision | Result | Report Path |
| --- | --- | --- | --- |
| <!-- tasks.md 的 [validation] 任务 ID --> | <!-- 验收的固定目标提交 --> | <!-- PASS / FAIL / BLOCKED --> | <!-- 独立报告路径 --> |

## Main E2E

<!-- 仅最终主分支 E2E，每轮一条聚合 stage=final，按 E2E ID/Attempt 留证；固定代码/用例/产物/配置/环境版本，
     记分片资源、真实入口、就绪/清理，引用项目开关、Main E2E mode、测试设计及降级批准来源。
     not-applicable 填 reason/basis/alternative_checks，E2E 记 NOT_APPLICABLE，替代检查写 Checks，删除下表。 -->

| E2E ID / Attempt | Requirement / Case Version | Executor / Time | Runtime Version / Entry | Actions / Expected / Actual | Result | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| <!-- 用例及尝试序号 --> | <!-- 需求、用例路径/版本 --> | <!-- 实际执行者/时间 --> | <!-- 主分支产物及真实入口 --> | <!-- 步骤与逐项断言/观察 --> | <!-- PASS / FAIL / BLOCKED；跳过保留原因并阻断必要场景 --> | <!-- 报告/日志/适用截图或轨迹 --> |

## Failures and Retests

<!-- 执行问题沿用来源问题 ID，无 ID 分配唯一编号；审查发现引用 Review Findings 原问题/闭环，不重复登记。
     保留原失败/受阻/各次复测，当前状态有依据；代码/用例修复关联独立 review，环境恢复关联就绪证据，无问题写无。 -->

| Issue ID / Task | Source / Check or E2E ID / Attempt / Version | Owner | Fix / Recovery | Review / Readiness Evidence | Retest Evidence | Current Status / Basis |
| --- | --- | --- | --- | --- | --- | --- |
| <!-- 原问题 ID 及任务 --> | <!-- 原报告、失败或受阻证据 --> | <!-- 实际责任人 --> | <!-- 修复版本或恢复措施 --> | <!-- 适用 Review ID 或环境证据 --> | <!-- 各次复测记录及版本 --> | <!-- 已解决/未解决/无法确认及依据 --> |

## Final Assessment

<!-- 语义审计后唯一 agentic-assessment 指当前轮；用 workflow check --stage plan --json 取 contractDigest/报告 sha256，
     填已审计路径/摘要。失效证据先复核/复验，禁仅刷新摘要；final/archive 完成判据见 procedures/acceptance.md。 -->
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
