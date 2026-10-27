# sync-event-projection Specification

## Purpose
定义 Sync 事件的下发形状：把已持久化的事件投影为客户端可直接呈现的结构化视图，按协商结果装配 ACP 原文并如实标记不可用，对未登记内容显式降级而不是丢弃。它让客户端的对话页、变更文件右栏与诊断入口建立在稳定的字段合同上，而不是自行解析 Agent 私有负载。

## Requirements

### Requirement: 事件视图的最低字段投影

每条下发的事件 SHALL 携带 `payload.view` 对象，其字段 SHALL 覆盖该事件类型在权威协议中登记的最低必填集合；已登记类型 SHALL NOT 下发缺少这些字段的视图。视图 SHALL 是开放对象：协议未定义的扩展字段与未来协议字段 MUST 原样下发，MUST NOT 被过滤、重排或丢弃。视图中的序号类字段 SHALL 以十进制字符串表示，结构性常量 SHALL 保持整数。

#### Scenario: 每类事件都带最低字段

- **WHEN** 服务端下发任意一条已登记类型的事件
- **THEN** 其 `payload.view` 含该类型的最低必填字段，客户端无需回查协议即可取得这些值

#### Scenario: 未定义的扩展字段原样保留

- **WHEN** 某事件的原始视图含协议未定义的字段
- **THEN** 下发时该字段仍在，取值与原始字节一致，MUST NOT 被丢弃或改写

#### Scenario: 序号以字符串承载

- **WHEN** 事件的视图含会话内序号、全局序号或增量子序号
- **THEN** 这些取值在线上为十进制字符串，结构性常量（如嵌套上限、块索引上限）保持整数

### Requirement: 消息与轮次的投影

消息增量类事件 SHALL 携带消息标识、轮次标识与增量子序号；轮次状态类事件 SHALL 携带轮次标识与其状态。增量在**每条消息内**的序号 SHALL 从零开始严格递增；空洞 SHALL 不被视为协议错误，也不表示丢包。承载消息正文的事件 MUST NOT 作为结构化历史条目下发——消息正文的权威只在按会话读取的明细资源里。

#### Scenario: 增量按消息定序

- **WHEN** 一个轮次内某条消息产生多条增量
- **THEN** 这些增量的增量子序号在该消息内从零起严格递增，客户端按此顺序拼接

#### Scenario: 正文事件不作为历史条目

- **WHEN** 客户端按会话读取结构化历史条目
- **THEN** 承载消息正文的增量与收尾事件 MUST NOT 出现在条目里，条目只含结构化事件

### Requirement: ACP 原文的装配与不可用标记

当客户端协商了携带 ACP 原文的能力时，事件 SHALL 在专用容器中携带原文。原文 `rawJson` 解码后的字节 MUST 与 Agent 发出的 JSON 文档逐字节一致（空白、键顺序与数字文本形式均保留），MUST NOT 经过通用 DTO 解析后重新序列化。因单条上限、保留期或存储原因无法下发时，SHALL 按协议已登记的原因标记为不可用并给出字节长度，MUST NOT 用占位内容或截断内容冒充完整原文。未协商该能力时 SHALL NOT 携带原文容器。

#### Scenario: 原文逐字节保真

- **WHEN** 某事件的原始 JSON 文档含超出 64 位范围的整数字面量或未识别字段
- **THEN** 下发的原文解码后与原始文档逐字节相同，客户端按文本处理不丢精度

#### Scenario: 超限时显式降级

- **WHEN** 某事件的内嵌原文超过单条内嵌上限
- **THEN** 该事件携带不可用标记与已登记原因、并给出字节长度，MUST NOT 下发被截断的原文

#### Scenario: 未协商时不携带原文容器

- **WHEN** 客户端未协商携带 ACP 原文的能力
- **THEN** 事件不含该容器，其余字段与协商时一致

### Requirement: 未登记内容的显式降级

对权威协议未登记的事件类型、以及未登记的 ACP 判别子，服务端 SHALL 下发该事件并保留其原文或明确标记原文不可用；MUST NOT 静默丢弃，MUST NOT 把结构化事件降级为普通文本，也 MUST NOT 因未识别而中断同会话其它事件的投递。设备无权读取的字段与资源 MUST NOT 进入视图与原文。

#### Scenario: 未登记判别子仍可见

- **WHEN** Agent 发出一个权威协议未登记的判别子
- **THEN** 对应事件仍被下发（带原文或不可用标记），客户端可据此呈现降级卡片，MUST NOT 出现整条事件缺失

#### Scenario: 未识别不阻断后续事件

- **WHEN** 一条未登记事件被下发
- **THEN** 同一会话后续已登记事件的投递不受影响，序号与字段保持完整

#### Scenario: 无权内容不进入视图

- **WHEN** 某事件的原文或视图含当前设备无权读取的字段
- **THEN** 该字段不出现在下发内容中，且不因缺席被客户端解释为空值

### Requirement: 终态类事件与设备撤销事件的下发

命令终态类事件 SHALL 携带请求标识，并在失败与不确定两类中分别给出结构化错误与「可能已到达 Agent」的标记。设备撤销事件 SHALL 只下发给仍有权查看设备状态的客户端。这两类事件 MUST 与产生它们的持久化记录同源，MUST NOT 由适配层另行推导。

#### Scenario: 失败终态携带结构化错误

- **WHEN** 某个已接受的 mutation 以失败终结
- **THEN** 对应终态事件携带请求标识与结构化错误对象，客户端无需自行归类

#### Scenario: 不确定终态标记可能已达

- **WHEN** 某个已接受的 mutation 进入结果不确定
- **THEN** 终态事件携带请求标识与「可能已到达外部 Agent」的显式标记，客户端据此要求用户确认而不是自动重试

#### Scenario: 撤销事件只发给有权者

- **WHEN** 某设备被撤销
- **THEN** 该事件只下发给仍有权查看设备状态的客户端，其余客户端 MUST NOT 收到它

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

### Requirement: 会话删除事件具有类型化最小视图

`session.deleted` 的 `payload.view` SHALL 至少包含被删除会话的稳定 `sessionId`。视图 MUST 表达权威记录已删除，MUST NOT 借标题、关闭状态或普通摘要更新掩盖删除语义。

#### Scenario: 删除视图携带会话标识

- **WHEN** 服务端投影一条 `session.deleted`
- **THEN** 客户端可从 `payload.view.sessionId` 唯一确定要移除的会话，无需自行解析 ACP 原文

### Requirement: 删除投影是静默数据事实

客户端 SHOULD 以删除事件更新本地权威投影，但该投影 MUST NOT 被产品解释为向用户发送“会话已关闭”或“空会话已删除”的通知。事件本身不得包含用户草稿、会话正文或删除原因文案。

#### Scenario: 删除当前空会话

- **WHEN** 当前页面所在会话收到 `session.deleted`
- **THEN** 客户端移除目录、摘要缓存和当前会话投影并返回目录，但不显示删除通知

### Requirement: 删除投影可重复应用

重复投递或重放同一 `session.deleted` SHALL 对客户端投影保持幂等。目标已从本地投影移除时，客户端 MUST 将重复事件视为已应用且不产生第二次导航或其它副作用。

#### Scenario: 删除事件重放

- **WHEN** 客户端因重连再次收到同一删除事件
- **THEN** 会话继续保持移除状态，不发生重复通知或二次跳转
