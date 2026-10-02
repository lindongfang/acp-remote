# 调度与返工

主 Agent 按工作包事件推进流水线。Execution Waves 表示最早开工层级，tasks 分组表示职责，
两者都不是要求整批完成后才能进入下一步的屏障。台账仍使用既有状态、角色池与重开次数限制。

## propose 收尾

规划、独立依赖声明审查和 `workflow check --stage plan` 全部在 propose 完成。
规划报告的 task_id 使用 `propose:<Review ID>`，与 plan.md 的稳定 ID 核对，不借用 apply 的资源准备任务。
先用机械检查获取 planningDigest 与 contractDigest 并修正可检测的结构问题，再由独立 reviewer 审查固定版本；
检查工作包依赖、写入归属与资源安排，同时核对相关契约的可满足性和跨文件一致性。
缺少审查记录时机械检查非 PASS 属于预期，不能把它当作已完成门禁。

本轮发现集中登记，区分阻断项、非阻断建议和记录格式问题。主 Agent 在既定用户意图内一次
修正全部阻断项，同步 proposal、specs、design、plan、tasks；尤其检查同一结论在多个文件中的引用。
修正后先重跑机械检查，固定新摘要再派发一轮独立复核，沿用 Review ID、问题 ID 并递增 Round。
非阻断建议不要求清零；选择采纳时合并修订，避免每修一项就改变摘要再派发一轮。

planningDigest（plan-v2）绑定需求、设计、依赖/波次/写入职责与资源、合并安排；
执行者、worktree 路径、报告路径和 apply 后填用例表不参与，机器仍核对这些字段的独立性与有效性。
检查命令变化通过影响分析重验检查证据，不要求依赖 reviewer 重审；改变依赖等规划语义才重新审查。
contractDigest 仍包含整个 plan.md 与 tasks 的任务描述，用于当前候选及最终证据。
旧规划报告仍按旧全文摘要核对，不能把旧 PASS 换上新摘要；verification 的格式修正不改变规划摘要。
原报告与目标不变时只修正登记并重跑机械检查，无须再找 reviewer 出一份相同结论。

plan 门禁通过后运行 `workflow snapshot --change <name> --output reports/plan-r1.json` 保存基线。
快照不可覆盖；再次批准后保存新路径。改动后运行
`workflow impact --change <name> --baseline reports/plan-r1.json`，得到 affected、unaffected、canContinue 与逐包原因。
需求按 Coverage Index 的任务归属映射到 WP，再传播到 code/contract 下游；全局契约、资源、
合并安排、池容量或无法映射的变化保守影响全体。缺快照时先建立可核实基线，不猜测哪些包无关。
仅 canContinue 中已开工且自身依赖/契约/资源仍就绪的包继续编码或编写；受影响包等待修订门禁。
若还有摘要过期之外的结构错误，canContinue 为空；合入与最终验收始终要求完整当前门禁 PASS。

## apply 逐包推进

每次选择新包前运行 `workflow status --change <name> [--planning-root <权威根>] [--json]`，
查看 plan 门禁、coding/testing 窗口、逐包状态/阻塞/待确认条件、接收与心跳诊断及下一步建议。
pending 包按尚未完成的 code/contract 传递下游数量降序、最长依赖链降序、计划表顺序排序；
这只是依赖影响排序，不是耗时预测。优先准备队列受池容量和已知独占资源冲突约束，
同一轮建议不重复分配独占资源；实际派发前重新核实门禁、基线、契约、资源及 worktree 交接。
同单元 integrated 基线已按 roles/_shared/role-report.md 登记接收时，视图核对当前契约、
本轮上游 DELIVERY/Verify/独立 review、基线 candidate Verify/独立 review、报告哈希及 Git 包含关系，
同时核对当前集成交付索引，后续失败或失效撤销解锁；planning_digest 绑定冻结契约内容，
文件内容变化后旧基线失效，不因新的规划审查 PASS 恢复，也不自动换绑历史摘要。
以一个覆盖全部单元内 code 上游的有效基线解锁下游。跨单元 code 上游仍要求合入。
无结构化接收记录时，main 按既有规则人工核实后推进，不为符合视图而补造历史或等待主分支合入。
Integration Baselines 中的失效原因单独展示，不自动删除历史或改台账；诊断不证明进程存活或资源实际释放。
mergeQueue 按完整交付单元核对本轮源提交、Verify 和最新独立 review；成员未就绪或跨单元上游未合入时不准备候选。
Order 全为非负整数时按顺序组执行，同值允许按可解锁的 pending code 下游数、未完成传递下游数、最长链、表序排序；
全部留空时视作同组。文字/混合顺序保守沿用表序，不自行解释授权重排。
recommendedMerge 每次只列一个可准备候选的单元；目标串行合入、候选门禁与合入后回归仍照常执行。
门禁非 PASS 不给出首次开工建议；既有已开工无关包的继续条件仍按 workflow impact 判定。

1. 确认 propose 门禁有效；逐包核对依赖、冻结契约、资源和角色池容量，选出本次可开工包。
2. provisioner 只准备这些包的 worktree 和所需依赖，逐包返回就绪交接；main 即时登记，
   该包可启动便派发并以 `--require-ack` 认领，不等待所有资源准备完毕，不提前为后续波次创建 worktree。
   作者收到任务后以实际 ID 运行 `dispatch --change <name> --wp <WP> --executor <ID> --ack`。
3. 作者返回固定提交、可读原始报告与结构化 handoff；main 接收该包必要索引后立即释放作者，
   在该包同一窗口启动独立 reviewer 并登记 reviewing；接管 reviewer 独立确认接收，不能复用作者的 ack。
   其他包继续编码、编写或检视。
4. reviewer 返回即登记该包结论；PASS 独立进入 ready-to-merge，FAIL 分派对应作者修复，
   BLOCKED 记录具体缺项。修复后以同一 Review ID 的新 Round 独立复核，受影响证据同步更新。
5. 仅在交付单元要求的 WP/TP 全部就绪时汇合；目标主分支合入仍由 merger 串行更新，
   最终验收等待所有必要交付与检查完成。

报告在每次交付时保存到约定的持久路径，main 从结构化 handoff 登记最小必要索引。
main 用 `workflow record --change <name> --input <原始报告路径> [--dry-run]` 自动登记适用的
Handoff Index、Dependency Declaration Review、Review Findings、Checks、Worktree Handoff 与报告哈希。
导入幂等，冲突或非法字段整次拒绝，不覆盖历史证据、不自动勾选任务。台账流转后用
`workflow reconcile --change <name>` 刷新对账；没有对应原始报告时不伪造索引。
不等整批结束后补落盘、统一更名或手工重写所有报告；复核报告使用独立版本路径，保留历史。
登记 reviewing、fixing 前必须具备实际接管实例与输入；台账不能代替派发。
若需等待契约或用户决策，先登记 blocked 及原因释放窗口，满足条件后按既有重开规则恢复。
尚未首次认领的包保持 pending，在 tasks/verification 记录受阻原因，不为写 blocked 而虚构一次开工。
Worktree Handoff 的 Received At 必须在本轮 coding/fixing 之前，Attempt 与实际认领一致。
main 用 `dispatch --change <name> --diagnose --idle-minutes 10` 检查未确认和停滞；
执行者确认后可用 `--heartbeat` 报告进展。UNCONFIRMED 表示新协议未接收，LEGACY_UNVERIFIED
表示旧记录缺少可核实确认；ACKNOWLEDGED 也不等于进程当前存活。诊断不自动销毁实例或释放窗口。

## apply 契约与门禁修正

main 汇总同一原因涉及的发现、现有用户决定、原/新契约及受影响 WP/TP 和证据，
集中更新权威规划文件；子角色继续负责各自产品代码或测试产物的修复，不能并发写权威规划记录。
无关工作继续运行；契约待定的包受阻，已返回的旧版本证据保留并说明失效或复用依据。
新摘要的规划门禁通过后，派发受影响作者修复及独立复核。契约的共同决策与版本冻结有先后关系，
通过提前检查和一次收敛减少轮次，不并发修改同一份契约以追求并发数。

仅新增且会改变用户已定行为、范围或验收条件的取舍需要澄清。执行既定选择、
同步遗漏引用、修正证据格式以及继续已授权的测试包不重复请求裁决。
required E2E 的 TP 保留并交付可运行用例/脚本；不能因只交设计文档或返工耗时就直接取消测试包。

机械格式检查失败先修记录：Dependency Declaration Review 字段仅填稳定 Review ID，
Result 单元格仅填 PASS/FAIL/BLOCKED/NOT_APPLICABLE，Plan Revision 直接填 CLI 返回的当前 planningDigest，
说明、轮次和哈希括注写对应字段或说明栏。不得为格式错误修改契约或伪造 reviewer PASS。

重开使用 `--retry-kind implementation|contract|environment|runtime --reason <依据>`。
Attempt 持续加一，四类各有 3 次上限；首次 coding 计入 implementation，旧无类型的尝试仍计实现额度。
契约修订、环境恢复和实例恢复不消耗实现额度，但各自也不能无限重试。
main 根据真实失败原因分类并留依据，不能把产品缺陷改记成恢复原因绕过上限。
