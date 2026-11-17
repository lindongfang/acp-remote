# Tests

<!-- 仅在 schema: spec-driven 变更进入实施准备时由独立 tester 创建，经 node testing/suites/verify-suite.mjs --change <name> 与 npm run check:spec 校验通过后交 reviewer 编码前合审。
只写可重复执行的用例定义；运行日志另存本地被忽略的 testing/runs/<change>/，精简摘要回填 plan.md。
保留稳定编号和标题，每例正文只必填“用例级别、操作步骤、预期结果”三项。
LX 仅用于 Harness 无法通过工具验证、必须人工判断的场景；缺环境/凭据/入口记 not-run，不自动转 LX。
补齐真实内容后删除示例占位行。完整规则见 openspec/config.yaml。 -->

## 共用执行条件与真实边界

<!-- 按组说明一次隔离数据目录、端口、受管进程树清理、真实 Harness 与可控替身边界。 -->

## 需求关联

<!-- 逐行 100% 覆盖本变更 specs/<capability>/spec.md 中全部 ADDED / MODIFIED 的 Requirement 与 Scenario（多场景可用顿号“、”分隔）。
跨能力长期回归另表使用 openspec/specs/<capability>/spec.md；skip_specs: true 的变更改为：| proposal.md | Success Criteria | SC1 | CAP-001 |。 -->

| 当前增量路径 | Requirement | Scenario | 用例 |
| --- | --- | --- | --- |
| specs/<capability>/spec.md | <Requirement 名称> | <Scenario 名称> | CAP-001 |

## Cases

### CAP-001 用例标题

- 用例级别：L5
- 操作步骤：替换为可执行的操作或命令，必要前提与清理要求直接写在步骤里
- 预期结果：替换为可观察、可独立判断通过或不通过的结果
