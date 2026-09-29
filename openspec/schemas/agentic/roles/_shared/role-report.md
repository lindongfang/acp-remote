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

```yaml
handoff_index:
  - task_id: "2.1"
    work_package: WP1
    role: coder
    phase: implement
    round: NOT_APPLICABLE
    stage: work-package
    target_revision: "<完整提交 SHA；规划审查可填当前 contractDigest；尚无提交时写 NOT_AVAILABLE 并说明>"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "<相对权威 changeDir 的路径或绝对路径>"
    result: PASS
    evidence_status: NEW
    applicability_basis: "<当前版本、命令、配置、环境、依赖与原证据的关系>"
    source_evidence: NOT_APPLICABLE
```

`task_id` 必须对应权威 tasks.md；`work_package` 填该证据对应的工作包（DELIVERY / REVIEW 行必填，环境、规划行写 NOT_APPLICABLE）；`role` 是报告角色；`phase` 与 `stage` 按下表取值：

| Role | Phase | Stage |
| --- | --- | --- |
| coder | `implement`、`fix` | `work-package` |
| tester | `design-author` | `work-package` |
| tester | `execute`、`retest` | `final-main` |
| reviewer | `plan` | `plan`（目标为当前 `contractDigest`，不是提交 SHA） |
| reviewer | `branch`、`test-case`、`integration`、`merge`、`post-merge` | `work-package`、`candidate` 或 `main`，按本轮 `target_revision` 所绑定的版本；复核轮沿用原类型 |
| validator | `validation` | `final-main` |
| merger | `integrate`、`candidate` | `candidate` |
| merger | `merge` | `main` |
| scout | `recon` | `recon` |
| provisioner | `runtime` | `runtime` |

`stage` 表示该行证据绑定的目标版本：`work-package` 是工作包/用例编写提交，`candidate` 是合入候选
（merger 复用本单元已有的执行 worktree，`integrate` 与 `candidate` 绑同一候选提交），
`main` 是合入后的主分支提交，`final-main` 是全部单元合入后的最终主分支版本，`plan` 是规划契约摘要
（依赖声明审查，`target_revision` 写当时的 `contractDigest`，不要求先有提交），`recon` 与 `runtime`
是尚未绑定代码的环境事实。只有同一阶段的证据才能互相顶替，跨阶段必须分行。
reviewer 的 `phase` 取本轮 Review Type（复核轮沿用原类型与原 Review ID）；`main` 不提交角色报告（它汇总各角色索引进 verification.md），
因此不在表中。合法枚举与绑定关系以本表为准，角色专有字段与判定细节见对应 `roles/*.md`。
`target_revision` 是本行结果所针对的固定提交；环境资源尚未绑定代码时记录已核实的目标提交，
确实尚不存在时用 NOT_AVAILABLE 并说明依赖，不能猜测。`evidence_type` 取 CHECK、E2E、REVIEW、
VALIDATION、RESOURCE 或 DELIVERY；没有计划内检查 ID 的资源/交付行，`evidence_id` 写 NOT_APPLICABLE。
`report_path` 指向可持久读取的本角色报告；执行记录和日志另在角色报告中引用。
`result` 取 PASS、FAIL、BLOCKED 或 NOT_APPLICABLE，不把待补证据写成 PASS。

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
