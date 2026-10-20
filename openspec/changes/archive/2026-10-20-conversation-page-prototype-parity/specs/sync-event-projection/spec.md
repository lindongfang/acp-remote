# Spec Delta

## ADDED Requirements

### Requirement: 工具调用详情随工具事件下发

`tool.call.started`、`tool.call.updated` 与 `tool.call.completed` 的 `payload.view` 在 Agent 提供了工具输入、工具输出或文本内容时 SHALL 携带 `toolDetail`。它只包含三个可选部分：`input`（ACP `rawInput` 的 JSON 值）、`output`（ACP `rawOutput` 的 JSON 值）与 `text`（ACP 工具内容里文本块的文本数组）。不认识的 content 类型 MUST NOT 被文本化或猜测成上述任一部分。ACP 未提供对应字段时该部分 MUST 缺席，MUST NOT 以下发空对象、空数组或零值冒充。

#### Scenario: 工具开始时携带输入

- **WHEN** Agent 的 `tool_call` 更新含 `rawInput`
- **THEN** 对应 `tool.call.started` 的视图携带 `toolDetail.input`，取值与 Agent 提供的 JSON 值等价

#### Scenario: 工具完成时携带输出与文本内容

- **WHEN** Agent 的 `tool_call_update` 含 `rawOutput` 与文本类 content 块
- **THEN** 对应 `tool.call.completed` 的视图携带 `toolDetail.output` 与 `toolDetail.text`，文本块顺序与 Agent 提供的一致

#### Scenario: 没有工具详情

- **WHEN** Agent 未提供 `rawInput`、`rawOutput` 或文本类 content 块
- **THEN** 视图不携带 `toolDetail`，且 MUST NOT 因此让事件或视图缺失

#### Scenario: 不认识的 content 不被文本化

- **WHEN** 工具内容只含协议未识别的 content 类型
- **THEN** `toolDetail` 不携带该内容，客户端仍可经原文诊断查看完整 ACP 原文

### Requirement: 工具调用详情的上限与截断

`toolDetail` 的每个部分 SHALL 受 v1 固定字节上限约束，并 SHALL 能通过 `view` 的 ACPR-CJ1 值域校验（数字为有限整数且绝对值不超过 2^53−1，嵌套不超过规定深度）。某一部分超过上限或无法以该值域表达时，该部分 MUST 缺席，并 MUST 在 `truncatedParts` 中列出对应的部分名；MUST NOT 下发被截断或无法保真的 JSON 片段。`truncatedParts` 只取 `"input"`、`"output"`、`"text"`，没有截断时 MUST 缺席。`toolDetail` MUST NOT 重复结构化差异行或终端输出已经承载的内容，也 MUST NOT 改变 `acp.rawJson` 的字节、摘要或长度。

#### Scenario: 单部分超限时显式标记

- **WHEN** `rawOutput` 的序列化字节超过 v1 固定上限
- **THEN** `toolDetail.output` 缺席，`truncatedParts` 含 `"output"`，其余未超限部分照常下发

#### Scenario: 多个部分超限时登记并集

- **WHEN** 同一条工具调用里输入与文本内容都超过各自上限
- **THEN** 两个部分都缺席，`truncatedParts` 同时含 `"input"` 与 `"text"`，MUST NOT 只登记其中一个

#### Scenario: 无法通过 ACPR-CJ1 的部分缺席

- **WHEN** `rawInput` 含非整数浮点或超出 2^53−1 的整数
- **THEN** `toolDetail.input` 缺席，`truncatedParts` 含 `"input"`，不得让该事件因 `view` 校验失败而无法提交

#### Scenario: 原文保真不受影响

- **WHEN** 事件携带 `toolDetail`
- **THEN** `acp.rawJson` 的字节、摘要与长度与不携带时逐字节相同
