# Proposal

## Why

<!-- 1-2 句话说明解决什么问题或机会、为什么现在做。 -->

## Success Criteria

<!-- 原样保留用户原始要求、硬约束、非目标与本变更可验证的成功判据（建议使用 SC1、SC2 稳定编号）；后续产物与归档均对照本节核验。 -->

## What Changes

<!-- 具体列出新增、修改或移除的行为变化；破坏性变更标注 **BREAKING**。 -->

## Capabilities

### New Capabilities
<!-- 引入的新长期能力（扁平单层 kebab-case，如 user-auth，不使用嵌套子目录），每项对应创建 specs/<capability>/spec.md。无新增写“无”。 -->
- `<capability>`: <能力覆盖的行为边界简述>

### Modified Capabilities
<!-- 规范级行为发生变化的既有能力（使用 openspec/specs/ 下精确单层目录名），每项对应创建增量 specs/<existing-capability>/spec.md。无修改写“无”。纯重构/工具/文档变更若无任何能力增改，在 .openspec.yaml 设置 skip_specs: true；未排期开工的远期规划请在 .openspec.yaml 设置 x-deferred: true（须至少声明一个能力，不可设 skip_specs: true）。 -->
- `<existing-capability>`: <修改的需求范围简述>

## Impact

<!-- 受影响的模块、协议/接口、依赖、存储或跨变更前置关系。 -->

## Exploration

<!-- 记录用户明确确认的决策依据（不得将建议或沉默写成确认）、术语表复核与按需 ADR；编码前必须消除阻断未知。原地维护当前有效内容，不在顶部堆叠历史修订横幅。 -->
