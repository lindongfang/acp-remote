<!-- 按 proposal 的能力清单写增量行为与场景；技术方案见 design.md。
-->

## ADDED Requirements

### Requirement: 每次消息级拒绝都推进入站期望序号

入站期望序号描述**对端已发出的消息**，本机是否接受它不影响它已被消耗。因此对**形状类**拒绝（未登记的信封字段、缺 `messageId`、body 不是 JSON 等）与连接字段禁用类拒绝，服务端 MUST 照常推进期望序号；MUST NOT 让后续序号正常递增的合法消息被判 `sequence_invalid`，也 MUST NOT 以「静默丢弃后续消息直到静默超时」代替推进。只有**连序号本身都读不出来**的帧（无法判定它消耗了多少）才不推进。

#### Scenario: 形状错误之后的一条合法消息

- **WHEN** 一条形状不合法的帧被拒绝（未登记信封字段 / 缺 `messageId` / body 非 JSON 之一）
- **THEN** 其后一条序号正常递增的合法消息 MUST NOT 被判 `sequence_invalid`，并能得到正常处理

#### Scenario: 未知 type 与形状错误同口径

- **WHEN** 先后收到「词表外 `type`」与「形状不合法」两类被拒帧
- **THEN** 两类都必须推进期望序号，MUST NOT 出现「一类推进、另一类不推进」的差异
