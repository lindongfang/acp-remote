# Shared Role Report Contract

本文件是所有角色报告共用的最小交接索引契约。主 Agent 派发时将全文与对应角色指令一起传入；
角色只填写自己执行或检视的范围，不汇总其他角色，也不修改权威 plan.md、tasks.md、verification.md。

## Shared Report

每份角色报告先按 `task_id / role / phase / agent_context / target_revision / scope / changes / checks /
issues / result / evidence_paths / resource_cleanup` 组织，再补角色专有内容。无值字段写空或
NOT_APPLICABLE；`agent_context` 写实际 Agent ID 与隔离/继承方式，`target_revision` 写固定目标，
`evidence_paths` 引用可读的本角色原始报告和日志。报告只覆盖本次任务，不复制其他角色私有对话。
已确认失败记 FAIL；没有已确认失败但缺必要输入、执行、隔离或证据记 BLOCKED；该阶段全部适用条件
满足才记 PASS。不适用须说明依据，不以局部 PASS 宣称下游交付或最终验收通过；并存失败与受阻时
保留两类问题、总结论为 FAIL。失败和复验保留原问题及证据，复用注明原记录与当前适用依据。

报告中的 `handoff_index` 是数组，每个任务、阶段和证据 ID 一行。多个 Check/E2E/Review ID 必须
拆成多行；同一报告路径可以被多行引用。候选与已合入主分支使用不同的 `stage` 和 `target_revision`，
不能用一个 PASS 覆盖两个阶段。最小字段如下，角色可增加专有字段：

```agentic-handoff
version: 1
agent_context:
  agent_id: "<实际宿主 Agent ID>"
  isolation: "fork_turns=none"
handoff_index:
  - task_id: "2.1"
    work_package: WP1
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "<完整提交 SHA；规划审查填当前 planningDigest；尚无提交时写 NOT_AVAILABLE 并说明>"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "<相对权威 changeDir 的路径或绝对路径>"
    result: PASS
    evidence_status: NEW
    applicability_basis: "<当前版本、命令、配置、环境、依赖与原证据的关系>"
    source_evidence: NOT_APPLICABLE
```

`task_id` 必须对应权威 tasks.md；propose 的规划审查使用 `propose:<Review ID>` 并与 plan.md 声明的 Review ID 核对，
不把规划审查挂在 apply 的资源任务下；旧数字任务 ID 继续接受。`work_package` 填该证据对应的工作包
（DELIVERY / REVIEW 行必填，环境、规划行写 NOT_APPLICABLE）；`role` 是报告角色；`phase` 与 `stage` 按下表取值：

| Role | Phase | Stage |
| --- | --- | --- |
| coder | `implement`、`fix` | `work-package` |
| tester | `design`、`design-author` | `work-package`；design 仅登记 DESIGN，不声明编写交付 |
| tester | `execute`、`retest` | `final-main` |
| reviewer | `plan` | `plan`（目标为当前 `planningDigest`，不是提交 SHA；旧 contractDigest 记录仍按全文核对） |
| reviewer | `branch`、`test-case`、`integration`、`merge`、`post-merge` | `work-package`、`candidate` 或 `main`，按本轮 `target_revision` 所绑定的版本；复核轮沿用原类型 |
| validator | `validation` | `final-main` |
| merger | `integrate`、`candidate` | `candidate` |
| merger | `merge` | `main` |
| scout | `recon` | `recon` |
| provisioner | `runtime` | `runtime` |

`stage` 表示该行证据绑定的目标版本：`work-package` 是工作包/用例编写提交，`candidate` 是合入候选
（merger 复用本单元已有的执行 worktree，`integrate` 与 `candidate` 绑同一候选提交），
`main` 是合入后的主分支提交，`final-main` 是全部单元合入后的最终主分支版本，`plan` 是规划契约摘要
（依赖声明审查，`target_revision` 写当时的 `planningDigest`，不要求先有提交），`recon` 与 `runtime`
是尚未绑定代码的环境事实。只有同一阶段的证据才能互相顶替，跨阶段必须分行。
reviewer 的 `phase` 取本轮 Review Type（复核轮沿用原类型与原 Review ID）；`main` 不提交角色报告（它汇总各角色索引进 verification.md），
因此不在表中。合法枚举与绑定关系以本表为准，角色专有字段与判定细节见对应 `roles/*.md`。
`target_revision` 是本行结果所针对的固定提交；环境资源尚未绑定代码时记录已核实的目标提交，
确实尚不存在时用 NOT_AVAILABLE 并说明依赖，不能猜测。`evidence_type` 取 CHECK、E2E、REVIEW、
VALIDATION、RESOURCE 或 DELIVERY；没有计划内检查 ID 的资源/交付行，`evidence_id` 写 NOT_APPLICABLE。
`report_path` 指向可持久读取的本角色报告；执行记录和日志另在角色报告中引用。
`result` 取 PASS、FAIL、BLOCKED 或 NOT_APPLICABLE，不把待补证据写成 PASS。
测试设计阶段 `phase: design` 使用 `evidence_type: DESIGN`；可运行用例/脚本与基础检查齐备后，
在 `phase: design-author` 登记 DELIVERY 和 CHECK，不把设计文档当作编写交付。

原始报告可以是完整 JSON/YAML 或 Markdown 内唯一的 `agentic-handoff` 块。`version: 1`、
`agent_context.agent_id`、`agent_context.isolation` 和非空 `handoff_index` 是自动导入所需字段；
每行 `report_path` 若填写必须指向这份原始报告。main 使用
`workflow record --change <name> --input <报告路径> [--dry-run]` 自动登记适用索引、结果和工具计算的报告哈希。
旧 Markdown 报告仍可按原方式登记，不通过伪造结构化报告迁移历史事实。

为使 `workflow status` 机械核对重开后的交付，工作包 DELIVERY 索引在第 2 轮及以后另填
`attempt: <本轮 Attempt>`；导入核对该轮台账作者，旧报告不补造字段。缺少字段时仍按原流程人工核实，
状态视图不把旧交付当作本轮候选准备证据。

merger 的 `integrate` 报告可附以下可选结构，记录 integrated 单元内已验收上游的固定集成基线。
main 核对并确认接收后调用 `workflow record`，自动追加 `## Integration Baselines` 和报告哈希；
导入仅登记接收，不宣称 Verify/review 或提交包含关系通过。新基线使用新 ID 和新报告路径，旧记录不覆盖。

```yaml
integration_baseline:
  id: IB1
  delivery_unit: DU1
  revision: "<完整集成提交 SHA>"
  contract_digest: "<当前 workflow check --stage plan 的 contractDigest>"
  planning_digest: "<当前 workflow check --stage plan 的 planningDigest；含冻结契约文件内容>"
  work_packages:
    - id: WP1
      attempt: 1
      source_revision: "<WP1 本轮交付提交 SHA>"
```

每个列出的 WP 必须有本报告中同一 revision 的 `merger / integrate / candidate` DELIVERY PASS/NEW
索引；候选 Verify 的 CHECK 索引逐 WP 绑定计划 Verification ID，并提供 checks 原始命令、退出码和日志。
独立 reviewer 另交同一集成提交的 candidate REVIEW 报告，reviewer 不得是产品作者或本基线 merger。
源提交也必须有本轮作者的 work-package DELIVERY、计划内 Verify 和独立 review，测试包须交可运行产物。
main 按共用导入规则先登记来源与检视报告，再确认基线报告；status 读取已接收表、原始报告和哈希，
逐包核对最新有效集成交付索引、Attempt、当前契约及 Git 包含关系；同版本后续 DELIVERY
FAIL/BLOCKED/INVALID/PENDING 撤销解锁，原始报告里的旧 PASS 不替代当前索引。
planning_digest 绑定规划模型及冻结契约文件内容，文件变化后即使重新通过规划审查，旧基线仍失效。
新报告填写该字段；旧无该字段的基线仅在计划没有冻结文件时兼容，不自动给历史报告补新摘要。
它只解锁同一个 integrated 单元的 code 下游，
跨单元上游仍须已合入当前目标；下游的全部单元内 code 上游须被同一个有效基线覆盖。
缺少结构化证据时保留人工核实路径，不为了符合状态视图而补造历史。

| Baseline ID | Delivery Unit | Work Packages | Baseline Revision | Contract Digest | Result | Received At | Report Path |
| --- | --- | --- | --- | --- | --- | --- | --- |

该表属于 verification.md 的可选 `## Integration Baselines`，由 main 通过导入维护；不放入 plan.md。

报告可附 `findings`（id / work_package / severity / location / impact / resolution）、
`checks`（id / work_package / command / scope / environment / exit_code / log_path）与
`worktree_handoff`（work_package / attempt / worktree / baseline_revision / executor / received_at）。
导入 Review Findings 时 `id` 保留原问题 ID，Review ID / Round / Result 来自 REVIEW 索引的 evidence_id / round / result；
同一 Review ID + Work Package + Round 可包含多个问题，同轮 Result 一致，按最大 Round 选当前结论。
编写交付须附按 TP ID 分组的 `test_delivery`：

```yaml
test_delivery:
  TP1:
    kind: automated # 或已有明确执行方案的 manual
    artifacts: [tests/e2e/example.mjs] # 相对权威 changeDir，或可读绝对路径
    basic_checks: [BT1] # 同报告 CHECK PASS 索引与 checks 项，含命令、零退出码及日志
```

调度输入另附变更名、权威 planning-root、WP ID、实际 executor ID 和 require-ack 标记。
标记开启时，作者及同窗口接管的 reviewer 收到任务后各自运行
`dispatch --change <name> --wp <WP> --executor <实际 ID> --ack [--planning-root <权威根>]`；
持续执行中以相同输入使用 `--heartbeat` 报告进展。规划审查和非工作包环境任务不要求该确认。
main 通过 `--diagnose` 查看未接收或停滞，确认记录与心跳均是执行者报告，不宣称证明进程存活。

`evidence_status` 只取以下值：

- `NEW`：在 `target_revision` 及所列版本/环境执行或检视，有本次原始证据。
- `REUSED`：复用 `source_evidence` 指向的原 ID、报告路径及版本；`applicability_basis` 逐项解释差异为何不影响结论。
- `INVALID`：原证据因版本、范围、命令、配置、环境、依赖或资源变化已失效；列出受影响任务与复验要求。
- `PENDING`：必要报告尚未返回或检查尚未执行；`result` 为 BLOCKED，`report_path` 写 NOT_AVAILABLE，列出待补 ID 和门禁。

`source_evidence` 对 REUSED 必填 `{id: <原 ID>, report_path: <原报告路径>, target_revision: <原提交>}`；
其他状态写 NOT_APPLICABLE，INVALID 如需追踪旧记录也使用同一对象格式。INVALID 的当前结果
不得写 PASS；PENDING 的 `result` 必须为 BLOCKED。目标提交尚不存在或无法核实时，不能报告该任务 PASS。
NEW/REUSED 的报告路径必须可读，且 ID、任务、阶段、目标版本和实际报告一致；
INVALID/PENDING 不能完成受影响任务。Check/E2E/Review ID 标识检查或用例，不单独标识一次证据记录；
同一 ID 可在不同任务、阶段或目标版本重新执行。**Review ID 在一条检视线程内复用**：
同一检视对象、同一类型、同一阶段的复核轮沿用该 ID，轮次由**显式 `round` 字段**区分
（`round` 在线程内从 1 递增；同一目标也允许再次审查，例如补齐材料后从 BLOCKED 重新判断）；
仅在类型/对象/阶段变化时新发 Review ID。`Review ID + Round` 是该检视行的唯一键；
同一线程有多轮时，当前结论以**显式 `round` 最大**的一轮为准（不是“最高 `target_revision`”，
Git SHA 无高低序），历史轮次保留不覆盖。问题 ID `<Review ID>-F<n>` 在线程内连续编号，不因轮次重置。
`round` 对非检视行写 `NOT_APPLICABLE`；检视行必填且与对应 `roles/*.md` 报告的轮次一致。
新一轮未完成或受阻时不得回退引用旧轮 PASS；历史阻断问题必须逐 ID 明确闭环。
逐条按任务 ID、阶段、目标版本和原始报告核对。
不同角色声称引用同一次证据记录时，来源、版本和结果才必须一致；同一 ID 的不同次执行
分别保留各自结论，不能互相覆盖。真正冲突须报告给主 Agent，不得自行改写他人报告或替全局判 PASS。
