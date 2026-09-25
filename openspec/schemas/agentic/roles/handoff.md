# Shared Handoff Index Contract

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
    role: coder
    phase: implement
    stage: work-package
    target_revision: "<完整提交 SHA；尚无提交时写 NOT_AVAILABLE 并说明>"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "<相对权威 changeDir 的路径或绝对路径>"
    result: PASS
    evidence_status: NEW
    applicability_basis: "<当前版本、命令、配置、环境、依赖与原证据的关系>"
    source_evidence: NOT_APPLICABLE
```

`task_id` 必须对应权威 tasks.md；`role` 是报告角色；`phase` 使用角色定义的阶段；
`stage` 取 `recon`、`runtime`、`work-package`、`candidate`、`main` 或 `final-main` 中的适用值。
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
同一 ID 可在不同任务、阶段或目标版本重新执行。逐条按任务 ID、阶段、目标版本和原始报告核对。
不同角色声称引用同一次证据记录时，来源、版本和结果才必须一致；同一 ID 的不同次执行
分别保留各自结论，不能互相覆盖。真正冲突须报告给主 Agent，不得自行改写他人报告或替全局判 PASS。
