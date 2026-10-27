<!-- 计划描述执行安排，不重复 specs/design。遵循 docs/WORKFLOW.md，不绑定宿主、模型或子代理 API。 -->

## Execution Model

- 执行：当前执行者默认串行完成，记录实际基线和用户未提交改动。
- 隔离：需要并行写入时使用独立工作区；范围重叠的工作串行处理。
- 交付：实际 diff、patch 或提交，加变更说明与验证结果；不要求私有会话 URI。
- 合入：直接修改核对 diff；patch 先校验再应用；提交按依赖合入。
- 验证：写明实际目录和依赖，最终检查覆盖待提交的集成结果。
- 复核：按风险选择执行者检查或独立复核，不强制每包启动检视进程。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| <!-- WP1 --> | <!-- 需求与可观察结果 --> | <!-- none / WPn --> | <!-- 实际执行者，不限角色名 --> | <!-- 相对路径 --> | <!-- diff / patch / commit 与报告 --> | <!-- 命令及执行目录 --> |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| <!-- 1 --> | <!-- WP1 --> | <!-- 依赖或范围重叠 --> | <!-- 集成后检查 --> |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| <!-- PV1 --> | <!-- 完整命令、目录和依赖 --> | <!-- 真实通过条件 --> |

## Code Review

<!-- 低风险改动可由执行者检查；协议、安全、持久化、并发和跨模块风险安排独立复核。
     不指定内置 reviewer、固定模型或独立启动要求；记录范围、发现及闭环证据。 -->

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| <!-- WP1 --> | <!-- 执行者检查 / 独立复核者 --> | <!-- 具体风险 --> | <!-- 无未闭环阻断问题 --> |

## Main E2E

```yaml
mode: not-applicable # required | not-applicable
reason: "" # not-applicable 时必填
basis: "" # not-applicable 时必填
alternative_checks: [] # not-applicable 时必填
```

<!-- 使用当前可用的真实浏览器或桌面测试工具，按行为选择真实组件，不绑定宿主接口。
     断言可观察结果，记录真实命令、环境、通过/失败/跳过，不能用截图或启动成功代替验收。 -->

## Risks

<!-- 执行风险、未执行检查、限制及缓解。 -->
