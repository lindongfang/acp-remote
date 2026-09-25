<!-- 按 proposal 的能力清单写增量行为与场景；技术方案和执行安排分别见 design.md、plan.md。 -->

## Purpose
<!-- 仅用于新增能力：用 1-2 句话、至少 50 个字符说明用途，否则严格校验会报告过短。
     已有能力的增量规范应删除本节；更新已有 Purpose 时直接编辑主规范。 -->

## ADDED Requirements

### Requirement: <!-- 需求名称 -->
<!-- 描述可观察行为，使用 SHALL/MUST 表达规范性要求，例如“系统 SHALL 允许用户导出数据”。
     不写内部实现细节。ADDED/MODIFIED 中每项需求至少包含一个场景。 -->

#### Scenario: <!-- 场景名称 -->
- **WHEN** <!-- 触发条件，包含理解行为所需的前置状态 -->
- **THEN** <!-- 具体、可观察的预期结果 -->

<!-- 按需求覆盖正常、异常及边界行为；场景不是详细测试脚本。
     驱动工具、测试数据准备和脚本留到后续测试设计，运行资源由 plan.md 安排。 -->

<!-- 场景标题必须使用四个井号。按需使用 ADDED / MODIFIED / REMOVED / RENAMED Requirements 标题。
     ADDED/MODIFIED 使用上述行为正文与场景格式；MODIFIED 必须包含更新后的完整需求及全部场景，不能只写修改片段。
     REMOVED 使用需求标题及 Reason/Migration；RENAMED 使用 FROM:/TO: 指明原需求标题和新需求标题，
     两者不要求补写 SHALL/MUST 正文或 Scenario。删除不适用的章节及占位内容。 -->

<!-- 已有能力：删除 ## Purpose（已有能力的 Purpose 以主规范为准），只写对应含义的 Requirements 标题。
     RENAMED 用 FROM:/TO: 指明原、新需求标题，不补写正文或 Scenario。

     REMOVED 示例（写入该已有能力的增量文件）：

     ## REMOVED Requirements

     ### Requirement: 旧版导出
     **Reason**: 已由新版导出系统替代。
     **Migration**: 改用 /api/v2/export 接口。 -->
