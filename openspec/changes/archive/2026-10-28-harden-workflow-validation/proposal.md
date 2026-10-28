# Proposal

## Why

最近一次工作流检视复现三种结构检查误通过，并发现规划阶段与实施阶段、验收证据留存规则不够明确。

## Success Criteria

用户原始要求：「最近一次改动是调整spec流程，帮忙完整的检视一下，看看实现是否有问题。另外看看spec流程是否有需要补充的」；检视后授权：「那修复吧」。

修复工作包任务引用误判、Main E2E 空 YAML 值误判和长期规范引用落入增量目录的问题，并增加正常/异常回归。补充规划与开工边界、tester首次输入顺序、验收摘要和证据失效规则；明确场景引用及人工覆盖复核边界。现有产品协议、安全与模块边界不变；历史归档不改写，不提交或发布。

## What Changes

- 精确识别真实任务中的工作包ID；采用已锁定的 YAML 库校验 Main E2E 类型与值。
- 显式区分长期规范与当前变更的增量规范路径。
- 允许分阶段保存规划，在出现下游产物时检查前置产物；实施前仍须完整检查和独立检视。
- 补充配置、角色与计划模板，并同步长期回归用例。

## Capabilities

### New Capabilities

无；只修开发工具链，使用 skip_specs。

### Modified Capabilities

无产品行为变化。

## Impact

scripts/openspec-test-cases.mjs、scripts/tests/openspec-tests.test.mjs、package.json及lock、openspec/config.yaml、role、项目schema和自写模板、testing/suites。

## Exploration

用户在三项缺陷及流程补充建议的检视结论后明确授权修复。沿用既有三角色独立、固定受测版本和历史归档不可改写的约束；术语表已复核，本次无需新增术语或产品架构ADR。具体规范场景的自动覆盖分析不纳入此次修复，文件存在性由脚本校验、场景及覆盖由reviewer核对；避免以文件存在冒充场景覆盖。
