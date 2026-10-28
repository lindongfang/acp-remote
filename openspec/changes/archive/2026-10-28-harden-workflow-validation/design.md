# Design

## Context

见 proposal.md。当前任务关联使用字符串包含，YAML配置使用正则读取字面文本，规范引用尝试多个根目录，缺失tests对所有规划阶段一律报错。

## Goals / Non-Goals

使结构门禁可靠拒绝已复现的错误，并保留正常增量规划能力。结构通过仍不代表独立验收完成，不自动执行Markdown内命令，不修改产品协议或历史记录。

## Decisions

1. 任务只从注释和代码围栏外的真实checkbox行解析。按完整ID匹配，WP1不匹配WP10或WP1-extra；每个任务须引用至少一个已声明工作包。普通文本、标题、注释和代码示例不计入。
2. Main E2E用yaml 2.9.1实际解析，要求唯一YAML配置块、唯一键和普通对象；reason/basis为非空字符串，alternative_checks为非空字符串列表。required模式只允许省略或空值，不接受错误类型。拒绝重复键、解析错误和别名展开。yaml已在锁文件中由OpenSpec使用，ISC许可、无新增传递依赖、兼容Node；登记为直接开发依赖以避免依赖偶然提升。
3. 显式openspec/specs路径只解析长期根；活动变更的裸specs路径只解析当前变更，长期suite的裸路径解析长期根。不得跨根回退，场景名称/覆盖仍由reviewer核查。
4. 新建空变更可暂缺proposal；已存在产物立即校验。design出现要求proposal；tests出现要求proposal/design；plan出现要求tests并递归检查其前置；tasks出现要求proposal/design/tests/plan。仅存在规划前缀不导致缺后续产物报错，实施前必须完整产物、npm run check和独立检视。
   全入口不能继续无条件validate --all：长期规范始终全量strict；空/proposal/design且未出现specs的前缀暂不做delta strict，已有specs路径（含空目录）、tests/plan/tasks或skip_specs声明时逐变更strict。坏delta和未声明skip_specs的缺规范不得借阶段豁免，真实全入口回归验证该接线。独立reviewer以PRE-003确认这是原阶段目标的必要接线。
5. tester首次按proposal/specs/design编写tests，plan产生后再复核执行安排。计划模板保存受测摘要、执行者、逐例结果、未执行项、关闭依据、归档后复核和证据复用判定；需求/预期/实现变化均需独立重新判断，纯移动核对映射及内容一致性。

## Risks / Trade-offs

- Markdown任务边界可能误识别示例 → 共用注释/围栏过滤并覆盖WP前缀和连接符反例。
- YAML类型混淆 → 使用现有解析库并逐字段验证null、空字符串、列表及错误类型。
- 分阶段允许不完整产物不能成为开工豁免 → 保留schema apply依赖，配置要求实施前运行完整门禁。
- 原始报告不入库 → 必需摘要入plan，报告只是附加证据。

## Migration Plan

只适用于现行活动变更和长期suite；旧归档保留原文。无产品部署/数据迁移。回滚撤销本次脚本、规则与直接依赖登记。
