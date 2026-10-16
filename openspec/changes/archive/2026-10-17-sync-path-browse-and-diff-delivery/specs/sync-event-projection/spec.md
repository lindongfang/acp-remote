# Spec Delta

## ADDED Requirements

### Requirement: 结构化差异行随改动事件下发

`file.changed` 事件的视图在客户端协商了 `core.file-diff.v1` 时 SHALL 携带该次改动的结构化差异行；未协商该 feature 时该键 MUST 缺席，MUST NOT 以空数组或其它占位形态出现。差异行不可用时（超出单条上限等）SHALL 按协议已登记的原因标记为未下发，MUST NOT 以空数组或占位行冒充完整。该键的取值 SHALL 是结构化行数据，MUST NOT 是拼好的文本块——客户端据此逐行区分新增、删除与上下文，而不必解析任何原文。

#### Scenario: 协商后携带差异行

- **WHEN** 一台协商了 `core.file-diff.v1` 的客户端接收一条带类型化 Diff 的文件改动事件
- **THEN** 该事件的视图含结构化差异行，且与同一条事件的行数统计一致

#### Scenario: 未协商时键缺席

- **WHEN** 一台未协商该 feature 的客户端接收同一条事件
- **THEN** 视图里不出现差异行键（不是空数组），其余字段与协商时逐字节一致

#### Scenario: 差异行不可用时显式标记

- **WHEN** 该次改动的差异行因超出单条上限而未随事件下发
- **THEN** 事件携带已登记的不可用原因，客户端可据此呈现明确说明而不是空白的差异区
