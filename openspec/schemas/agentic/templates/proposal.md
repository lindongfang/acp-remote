<!-- 本文件定义变更动机、范围、能力与影响；行为细节放入 specs，技术方案放入 design.md，
     协作安排放入 plan.md。specs 根据能力清单和范围定义行为要求及验收场景；
     design 根据动机、范围和影响制定方案。二者共同收敛后作为 plan.md 的输入，再生成 tasks.md。
     skip_specs 时，plan.md 依据 proposal、design 及明确引用的既有行为契约生成。 -->

## Why

<!-- 说明本次变更的动机：解决什么问题或抓住什么机会？为什么现在做？用 1-2 句话概括。 -->

## What Changes

<!-- 用项目列表明确新增、修改或移除的内容；必要时说明本次不包含的内容。
     破坏性变更标记为 **BREAKING**。 -->

## Intent and Constraints

<!-- 引用用户原始要求的原话、会话/工单/文件位置和时间；区分用户已确定事项与 Agent 假设。
     已有授权直接引用，不重复请求确认。只有超出 decision_bounds、改变硬约束/范围/成功判据的
     新取舍才重新澄清；文档彼此一致不能替代与原始意图一致。每个字段使用非空字符串列表，
     无非目标或假设时明确写“无”及依据。不要把下列示例当作用户授权。 -->
```agentic-intent
sources:
  - "<原始要求、时间与可追溯来源>"
constraints:
  - "<必须遵守的产品/技术/时间约束>"
non_goals:
  - "<明确不做的事项>"
success_criteria:
  - "<用户可观察、可验证的成功判据>"
decision_bounds:
  - "<可自主决定的范围；改变哪些事项需要用户决策>"
assumptions:
  - "<尚未确认的推断及影响；阻塞性未知必须在拆包前解决>"
```

## Capabilities

<!-- 先检查现有规范，区分新增能力与已有能力的需求变化。
     仅当规范层面的行为不变时（如不改变行为的纯重构、工具或文档变更），
     在该变更的 .openspec.yaml 中设置 skip_specs: true，说明理由并引用既有行为契约。
     此时两类能力清单均无变更，删除占位项；否则至少声明一项能力并生成对应增量规范。
     不得为通过校验而编造需求。 -->

### New Capabilities
<!-- 列出新增能力。新增路径段使用 kebab-case，例如 user-auth 或 identity/user-auth，
     遵循项目现有规范的组织方式。每项对应 specs/<capability-path>/spec.md。 -->
- `<capability-path>`: <能力概述>

### Modified Capabilities
<!-- 仅列出需求发生变化的已有能力，不包含纯实现调整。
     每项需要一个增量规范文件，沿用 openspec/specs/ 下的原有完整路径；无需求变化则留空。
     完全没有能力变化（纯重构、工具或文档变更）时，必须在该变更的 .openspec.yaml 中
     设置 skip_specs: true，否则 npx --quiet --no-install openspec validate <change> --strict 会拒绝零增量的变更。 -->
- `<existing-capability-path>`: <需求变化>

## Impact

<!-- 列出受影响的代码、API、依赖和系统。 -->
