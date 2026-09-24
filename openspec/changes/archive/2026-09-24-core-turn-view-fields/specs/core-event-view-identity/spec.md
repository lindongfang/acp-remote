## Purpose

让 core 在持久化与广播事件时补齐 Sync 事件 view 所要求的 turn 归属与会话版本字段，使下游（Sync/Node Link 客户端）无需再依赖适配器是否知道 core 的内部标识，即可按 `docs/SYNC_PROTOCOL.md` §10.3 的最低字段校验与重放事件。

## ADDED Requirements

### Requirement: 事件 view 的 turn 归属由 core 注入

系统 SHALL 对事件类型属于 `docs/SYNC_PROTOCOL.md` §10.3 要求 `turnId` 的集合、且被 core 归属到某个 turn 的事件，把该 turn 的稳定标识作为 view 的顶层字段 `turnId`（字符串）写入；注入 MUST 在持久化与广播之前完成。注入范围 MUST 与 §10.3 的该事件类型集合一致，MUST NOT 向其它事件类型添加未协商字段。没有 turn 归属的事件 MUST NOT 出现 `turnId` 字段。owned 路径由本端注入；imported 路径的 view 由 Owner 注入后到达，本端 MUST 保留其取值（不重写、不补齐、不伪造），因为 `payloadDigest` 覆盖的是 Owner 给出的视图字节。

#### Scenario: 适配器未产出时由 core 补齐

- **WHEN** 一个被归属到某 turn、且适配器产出的 view 不含 `turnId` 的事件经 owned 路径提交
- **THEN** 持久化的 view 与广播的 view 都含顶层 `turnId`，取值等于 core 为该事件解析出的权威 turn 标识

#### Scenario: 无 turn 归属的事件不伪造字段

- **WHEN** 一个没有 turn 归属的事件（如 `session.created`、`session.info.changed`、`agent.disconnected`）提交
- **THEN** 其 view 不含 `turnId`，且其它字段与未注入时逐字节相同

#### Scenario: imported 路径保留 Owner 注入的归属

- **WHEN** 一个由 Owner 注入、带顶层 `turnId` 的 imported 事件被收下并广播给本地客户端
- **THEN** 广播 view 的 `turnId` 与 Owner 给出的取值逐字节一致（本端不重写、不重复注入），且不因 no-content-cache 而缺失该字段

### Requirement: 已存在的 turnId 冲突必须显式失败

系统 SHALL 在 view 已带顶层 `turnId` 时要求其取值等于 core 的权威 turn 标识。取值不一致时 MUST 拒绝该次提交并返回显式错误，MUST NOT 覆盖原值、MUST NOT 同时保留两个同名字段。取值一致时 MUST 保持该字段字节不变。

#### Scenario: 取值一致时保持原字段

- **WHEN** 适配器产出的 view 已带 `turnId`，且其取值等于 core 解析出的权威 turn 标识
- **THEN** 提交成功，view 中该字段只出现一次且字节与输入一致

#### Scenario: 取值不一致时无副作用地失败

- **WHEN** 适配器产出的 `turnId` 与 core 的权威标识不同
- **THEN** 返回显式错误，不写入任何事件行、不发布任何帧，会话与 turn 状态不变

### Requirement: 会话版本字段与提交结果一致

系统 SHALL 对 `docs/SYNC_PROTOCOL.md` §10.3 要求 `version` 的 view（当前为 `session.mode.changed` 与 `session.config.changed`）注入该次提交后的会话版本，且 MUST 以十进制字符串表示（不得使用 JSON number）。注入值 MUST 等于存储层在同一次提交中返回的会话版本；该事件类型集合 MUST 与 §10.3 的要求保持一致，§10.3 变化时 MUST 在同一变更内同步本清单。

#### Scenario: 状态变更提交后的版本

- **WHEN** 一次提交包含会话状态变更（如模式或配置切换）并成功提交
- **THEN** 该批中 `session.mode.changed`/`session.config.changed` 的 view 含顶层 `version`，取值等于该次提交后存储层返回的会话版本，且为字符串类型

#### Scenario: 纯事件提交不递增版本

- **WHEN** 一次提交只追加事件而不含状态变更
- **THEN** 注入的 `version` 等于该会话当前版本（与提交前相同），且不发生递增

#### Scenario: imported 事件不重复注入

- **WHEN** 本端收到来自 Owner、已带 `version` 的 imported 事件（如 `session.config.changed`）
- **THEN** 本端按 Owner 给出的取值原样广播与索引，不改写该字段（注入由拥有该会话的节点完成），且不因此产生本地会话版本变更

### Requirement: 版本规则的漂移必须显式失败

系统 SHALL 对含 §10.3 要求 `version` 的 view 的提交，在提交前按「含状态变更时版本递增一、否则不变」推导预期会话版本，并在提交后与存储层返回的版本比对。两者不一致时 MUST 显式失败，MUST NOT 以任一方静默覆盖另一方，也 MUST NOT 发布该批事件、MUST NOT 把该批报告为成功。推导与比对只在该类提交上进行，以免为每次事件提交增加一次版本读取。

#### Scenario: 推导值一致时按注入值提交

- **WHEN** 存储层返回的会话版本等于 core 推导的预期版本
- **THEN** 提交成功，注入的 `version` 与该返回值一致

#### Scenario: 推导值不一致时中止

- **WHEN** 存储层返回的会话版本与 core 推导值不同
- **THEN** 返回显式错误、不发布任何帧，且不得把该批报告为成功（比对发生在存储返回之后，本端不撤销已落盘的行）

### Requirement: 注入不改变 ACP 保真与既有字段

注入 MUST NOT 改动 ACP 原文三要素（`acp.rawJson` 的字节、其 sha256 摘要、其字节长度），MUST NOT 丢弃、重排或改写 view 中既有的其它字段（含未知字段与扩展 payload）。

#### Scenario: ACP 三要素逐字节不变

- **WHEN** 一个带 ACP 原文的事件在被注入 `turnId` 后持久化
- **THEN** 其 ACP 原文的字节、sha256 与字节长度与注入前完全相同

#### Scenario: 未知字段与嵌套结构保留

- **WHEN** view 中含未知的顶层字段或嵌套结构
- **THEN** 注入后这些字段仍然存在、取值不变，且按字段读取既有投影（如文本 delta 的字段读取）行为不变

### Requirement: 幂等重放不得二次注入

系统 SHALL 在幂等重放路径（同一 actor 与 requestId 的重复提交）返回首次持久化的 view 字节，MUST NOT 二次注入新字段或产生与该次持久化不同的字节。

#### Scenario: 重放返回与首次持久化相同的字节

- **WHEN** 同一 `(actor, requestId)` 被重复提交并命中幂等重放
- **THEN** 返回的 view 与首次持久化逐字节相同，且不新增事件行、不改变会话版本

### Requirement: prompt 接受结果不是 turn 归属的权威来源

系统 MUST NOT 把适配器 `prompt` 接受结果中的 turn 标识采信为权威值。turn 归属 MUST 始终来自 core 自身分配的标识，并据此完成本能力要求的注入；适配器给出的值 MUST NOT 影响 turn 行的创建数量。

#### Scenario: 适配器返回不同占位值不影响归属

- **WHEN** 适配器返回一个与 core 分配值不同的 turn 标识（如适配器本地占位 id）
- **THEN** core 的 turn 记录、事件归属与 view 的 `turnId` 仍使用 core 分配的权威标识，且不产生第二个 turn 行

#### Scenario: 适配器返回占位值时归属仍由 core 决定

- **WHEN** 适配器返回的接受结果包含一个固定占位值（例如全零 UUID）
- **THEN** 调用仍然成功，turn 行的标识与 view 的 `turnId` 都等于 core 分配的权威标识（不等于该占位值），且该 turn 只产生一行
