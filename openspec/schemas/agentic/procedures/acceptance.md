# Acceptance Procedure

本文件是验收指令，不是待生成的规划 artifact。由 agentic-verify 或宿主的等效入口完整读取。

## Inputs and Target

使用本次 instructions 返回的 changeDir，显式读取其中的 plan.md、tasks.md、
verification.md，以及 proposal、适用 specs 和 design。verification.md 不在 contextFiles
中也必须读取；缺失必要文件或无法读取证据时报告 BLOCKED，不用空报告代替。
skip_specs 必须由有效变更元数据和 CLI skipped 状态确认。

从计划和证据确定代码仓库绝对路径、目标主分支及准确提交，读取实际 Git 状态和版本。
不要将当前工作区 HEAD 默认当作目标主分支，或将本地旧引用当作已确认的远端最新版本。
明确本轮验收的是本地主分支还是远端主分支；无法确认约定目标时报告 BLOCKED。
使用固定版本读取实现；存在影响验收的未提交代码时不能用 HEAD 的测试结论覆盖这些改动。
只有规划/证据文档提交不同于代码提交时，检查实际差异并说明对应关系。

## Evidence Audit

按以下主题逐组核对；通过任务 ID、Check ID、Review ID、测试报告 ID、E2E ID 和问题 ID
追踪到固定版本的原始报告及具体证据。同一材料可引用，不要求重复复制。
旧证据缺少新字段时可用准确来源、版本和位置建立无歧义映射；无法确定关联则列出缺失项，不编造 ID 或结果。

审计组按下表核对本次变更 verification.md 的节；同一节可被多组引用，节名以 `templates/verification.md` 为准；
旧变更缺少某节时，按内容定位并记录映射依据，不跳过该组检查。

| 审计组 | 核对 verification.md 的节 |
| --- | --- |
| Contracts and Coverage | Target、Checks、Check Plan Changes |
| Delivery and Versions | Merge History、Dependency Handoffs、Candidate E2E、Main E2E |
| Project Checks and Resources | Checks、Check Plan Changes、Runtime Resources |
| Independent Reviews | Review Findings |
| E2E Design and Execution | Test Design and Authoring、Candidate E2E、Main E2E、Checks |
| Issue Closure and Evidence Validity | Failures and Retests、Review Findings、Final Assessment |

各角色的 handoff 报告按其字段汇总到 verification.md 的对应节（同一节可来自多个角色；映射仅作定位，
节名以 `templates/verification.md` 为准）：

| 角色 | handoff 主要字段 | 汇入 verification.md 的节 |
| --- | --- | --- |
| main | 全部权威记录与最终判断 | Final Assessment、Check Plan Changes |
| coder | changes / checks / evidence_paths | Checks、Dependency Handoffs、Failures and Retests |
| tester | phase=design-author/execute/retest、checks / evidence_paths | Test Design and Authoring、Candidate E2E、Main E2E、Failures and Retests |
| reviewer | target_revision / issues / result（Review Context、Findings、Assessment） | Review Findings、Failures and Retests |
| validator | observations / evidence_paths（Additional Independent Validation） | Checks、Failures and Retests |
| integrator | target_revision / changes / checks（源/base/candidate/main、防竞态） | Merge History、Dependency Handoffs |
| environment | role / phase / commands / observations / resource_cleanup（changes、checks 为 NOT_APPLICABLE） | Runtime Resources、Target |

### Contracts and Coverage

- 对照 proposal 的 Intent and Constraints 原始来源、硬约束、非目标、成功判据和决策边界，
  核对实际交付方向；不能仅证明生成文档彼此一致。范围或判据改变需有决策依据，已有授权不重复询问。
- 按 plan 的 Coverage Index 逐项核对需求/场景、任务、检查与原始证据；机械引用完整不代表断言有效。
- 核对 specs/design 在生成 plan.md 前的收敛及后续澄清记录：行为验收条件与接口/数据/环境
  一致，相关计划、任务、测试已同步，受影响证据有失效、复验或有依据的复用结论。
- 核对需求或范围澄清已同步 proposal.md 的范围、能力清单与影响，列出的能力路径与 specs
  文件保持一致。
- 核对需求到实现及测试/观察证据的映射，以及设计和执行计划的一致性。
  发现真实行为错误或未满足的必要要求时判 FAIL，不因默认 verify 将其列为 WARNING 而放行。

### Delivery and Versions

- 核对规划时确定的交付单元、模式和就绪复核；逐个取最新主分支构造候选，required 时对应测试
  及关键候选 E2E 必须通过才合入。检查合入前基线复核和防竞态记录，实际结果一致性及必要回归
  通过后才处理下一功能单元。无新增差异不强制重复同范围人工 review，但需原结论及差异依据。
- 候选与最终 E2E 分别审计：同轮固定代码/用例、产物标识或哈希、配置及环境/依赖版本，
  明确允许的分片命名空间差异。候选子集不替代最终规定完整覆盖；变化后的证据逐项判断有效性。

- 核对每个工作包的依赖交接记录：相关集成验证前的上游已验收提交、下游验证基线和包含关系。
  上游更新后的下游任务及证据应已重新评估，受影响部分有重跑和复核结果。
- 核对每次候选、实际合入和主分支检查的版本、顺序、范围及证据；前次失败或受阻时，
  后续功能合入应暂停。恢复需有修复/回滚后的主分支检查、此前失败/受阻检查（包括 E2E）
  的通过证据和更新候选记录。
- 核对独立集成 Agent 的实际 ID、上下文方式、独立分支/worktree、交接输入及原始报告，
  确认主 Agent 未兼任且未复用实现者、测试 Agent 或 reviewer；允许继承必要对话和延续集成上下文。
  检查批次集成、候选、合入及主分支检查的实际执行者、授权和串行更新记录。
  未建立独立执行角色或缺必要证明时保持 BLOCKED；已确认角色违规为 FAIL，不能仅凭合并结果放行。

### Project Checks and Resources

- 按 Project Verify 清单的稳定检查 ID，逐项读取实际完整命令、工作目录、退出码、运行环境、
  代码及脚本/配置版本和日志；统一入口需覆盖全部约定子检查，不能只看最后命令的退出码。
  核对约定测试实际被发现并执行，检查未运行、全部跳过、失败被吞掉或范围弱化的情况。
  清单/脚本/排除项调整必须有原/新值、理由、风险覆盖及独立 review 依据；修改计划不能抹除失败。
  复用证据需与当前代码内容、命令/配置、环境及范围匹配，并考虑提交/分支身份依赖。
  实现 Agent 执行命令无需编码对话隔离；若计划另有独立探索性验证，则核对其隔离及执行证据。
  并发运行的数据库、端口、容器、账号等资源需有隔离记录，或串行占用记录。
  无法确认共享资源是否污染结果时，将受影响证据记为待复验，而非直接通过。

### Independent Reviews

- 逐项读取独立 review 报告，核对 Review ID、任务及阶段、实际子 Agent ID、上下文隔离、base/target 和范围；
  CRITICAL/MAJOR 必须有复核闭环。复用结论应引用原 review ID，并解释版本差异和适用性。
- 逐项核对报告列出的待补 Check ID、影响及主 Agent 后续核对证据；必要项须已完成，
  范围/配置变化或新问题须有独立复核。不能用静态 review PASS 代替尚未通过的测试或证据核对。
  用例编写阶段不要求后期运行材料，但候选及最终执行准备所需的分配、版本和资源检查不能遗漏。

### E2E Design and Execution

- E2E mode 只接受 required 或 not-applicable。required 必须有最终主分支实际运行版本、
  全部约定场景和 PASS 证据；环境不可用是 BLOCKED，实际测试失败是 FAIL。
  not-applicable 必须有 reason、basis 和非空 alternative_checks，替代检查逐项通过；
  E2E 本身保留 NOT_APPLICABLE。替代验证必须是另列的主 Agent 任务，不并入 [e2e-owned] 行；
  该行在 not-applicable 下只确认“不适用判据已按计划固化”，完成条件同为 e2e check PASS。
  检查 mode 变更依据及历史，不因失败自动降级。
- apply 内最终验收时运行 `npx --quiet --no-install openspec-agentic e2e check --change <变更> --json`：它给出项目开关值、
  该变更的 mode 与降级批准结论（PASS / FAIL / BLOCKED），以及 required 的执行记录是否成功且对当前版本有效。
  单变更检查要求变更已完成：仅扩展拥有的最终 E2E 行与正在执行的最终验收行（[final-verification]）可待办，
  其余任务未勾完即 BLOCKED（查全部模式才把进行中的变更记为 IN_PROGRESS 不参与判定）；
  两种 mode 都保留该 `[e2e-owned]` 行：required 时它是最终主分支完整 E2E 的执行/汇总行，not-applicable 时是
  不适用判据确认行；判 PASS 时该检查都按标记自动勾选该行、非 PASS 时自动回退（行级单一所有者，主 Agent 不手勾也不手动回退）；
  只在已授权“执行验收并更新进度”时运行该回写模式；用户仅要求只读核查时加 `--no-write`，不回写、只报告需要修正的任务；
  同时确认该行已按完成条件勾选（未 PASS 应保持待办），框的状态应与检查结论一致；
  not-applicable 下实际替代验证任务应先完成，否则检查判 BLOCKED、不会回写该行。
  开关为 true 或缺省时 mode 必须为 required；计划写 not-applicable 时核对 plan.md 的 downgrade_approval
  是否可追溯到用户的显式批准（原话、时间、来源）。缺少该记录或无法追溯时判 FAIL：计划违反项目开关，
  要求回写为 required、补齐 E2E 任务并按 required 重新验收，不按环境受阻记 BLOCKED；
  开关为 false 时按上一条核对三字段，不因关闭而免除 reason、basis、非空 alternative_checks。
  该检查只做结构比对，不证明 E2E 真的执行过；required 的执行依据是变更目录下的执行记录
  （`npx --quiet --no-install openspec-agentic e2e run` 写入，含命令、退出码、提交与输出片段）：
  无成功记录即 FAIL；配置了 `x-agentic.e2e.command` 时人工记录不能替代自动执行，且成功记录的 command 必须与该命令一致；
  最近一次相关尝试必须成功（更新的失败不能被更早的 pass 掩盖），候选阶段记录（`--stage candidate`）不满足最终门；
  连续失败达到 `x-agentic.e2e.maxAttempts`（默认 3）时判 BLOCKED：核对已保留的失败尝试、问题 ID 与已尝试方案，
  确认自动重跑确已停止；不得把“达到上限”当 FAIL 继续循环，也不得删除记录或自行提高上限续圈（提高上限需用户决定）；
  记录停在旧提交且之后有变更目录之外的改动时视为失效，须重跑。人工记录须有执行人与证据引用。
  记录本身不能证明测试真实性，实际执行、入口、分片与断言仍按以下条目核对。
- required 时按计划 E2E ID 核对用例编写/独立审查记录及执行者，确认代码、用例和运行产物版本。
  核对 TP 的设计/编写及执行 Agent ID、产品编码对话隔离设置和输入版本；用例作者不得自审。
  核对完整用例清单与各执行分片的分配/返回 ID，编写分组可重排但不能漏测或把重试计为新覆盖。
  各分片需对应同一固定目标版本及明确的资源隔离/独占证据；版本变化后的旧结果须有适用性
  判断及受影响复验，不能把不同版本的通过摘要相加。测试用例与脚本基础检查不替代 E2E。
  检查启动与就绪证据，并确认 Web 界面场景通过真实浏览器操作、Tauri 等桌面场景通过实际应用
  及原生链路完成；API 检查或模拟原生接口不能替代这些场景。核对依赖替身未绕过被测核心链路。
  逐项核对操作、预期、实际结果和断言，人工用例需实际执行人、时间及可复查的观察证据。
  仅启动成功、截图或退出码为零不能证明通过。计划与执行 ID/数量需一致，必要用例遗漏、
  跳过或无法执行时阻断；产品启动/断言失败为 FAIL，缺环境/驱动为 BLOCKED，不把产品故障
  当作环境问题。重试核对全部尝试、失败原因和复验依据，不能只接受最后一次成功摘要。
- 将测试设计/编写报告与运行报告分开核对，通过报告 ID 关联 TP、任务及 E2E ID；
  保留既有用例身份，分片或负责人变化不应造成漏测或重复计数。分别核对逐次结果、分片结论及阶段汇总。

### Issue Closure and Evidence Validity

- 按原问题 ID 核对 Review Findings 及 Failures and Retests，关联原失败/受阻、责任人、修复或恢复、
  适用独立 Review ID/环境就绪证据及新版本复测。不能以不相关的 PASS 关闭原问题。
  已解决须有有效依据；未解决或无法确认的问题按影响保留阻断，非阻断项也须有处理结论。

历史 FAIL/BLOCKED 不必删除；只有明确被后续有效证据解决的记录才不再阻断。
版本改变时不能只看最新一行 PASS：逐项判断旧证据是否仍覆盖最终版本，缺少依据则待复验。

## Tasks and Result

通过 tasks.md 中唯一的 `[final-verification]` 标记定位正在执行的最终验收任务，
编号变化不影响识别。旧任务无标记时，由任务语义明确识别并记录唯一 ID；存在歧义则 BLOCKED。
只有本任务可在验收期间保持待办，其他未完成任务都阻断验收，不允许整体忽略未完成列表。

先完成上述检查，再形成结论：已确认的失败为 FAIL；没有已确认失败但缺资料、未运行或有
未完成任务时为 BLOCKED；全部适用检查通过、证据对应目标版本且无阻断项时才为 PASS。
同时存在失败和受阻时，两类问题都列出，总结论为 FAIL。

在已授权执行最终验收的会话中，由主 Agent 将结果、目标提交、当前验收任务 ID、
有效证据引用、失效/复用判断及未解决项写入 verification.md 的 Final Assessment。
每轮新增唯一验收 ID、时间及执行者，记录各审计组结论、准确目标与核实证据、CLI 原始状态及查询时间、
有效证据记录 ID、未解决问题 ID 和复验/任务修正要求。标识当前轮次，不覆盖历史。
CLI 状态独立记录并引用原始输出，不以证据结论改写 CLI 状态；更新任务后重新查询时注明查询时点。
PASS 后才勾选最终验收任务；FAIL/BLOCKED 时保持或恢复该任务待办，并恢复已失效检查的任务待办。
保留历史记录。若用户仅要求只读核查，则只用 `e2e check --no-write` 等只读命令，只报告结论和需要修正的任务，不写文件。

保存结论前再次核对目标主分支版本；若变化，重新评估受影响证据后再形成结论。
在勾选最终验收任务之前，按 templates/verification.md 维护当前 agentic-assessment，执行
`npx --quiet --no-install openspec-agentic workflow check --change <变更> --stage final --json`；
非 PASS 不完成验收。归档前用 --stage archive，所有任务必须已完成。测试 worktree 传
`--planning-root <权威规划根>`。命令始终只读；只读核查时不补写缺失块，报告需修正的字段。
检查协议及远端引用边界见 procedures/workflow-check.md；原始历史不能因刷新摘要而删除。
输出应分别说明 CLI 任务状态和本次验收结论。仅 PASS 可报告“该版本可归档”，
归档前目标版本或证据再变化时需重新验收。此步骤不自动合并、回滚或归档。
