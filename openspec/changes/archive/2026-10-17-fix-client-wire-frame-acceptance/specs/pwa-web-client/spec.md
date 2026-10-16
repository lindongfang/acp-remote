# Spec Delta

## ADDED Requirements

### Requirement: 解码边界受理每一条协议合法帧

客户端 SHALL 受理其解码边界上每一条协议合法的 WSS 帧。`fixtures/sync/v1/manifest.json` 中标记为有效、且 schema 指向 `schemas/sync/v1/message.schema.json` 的每条固定样例，其磁盘原文 SHALL 被解码边界受理。这条受理面 SHALL 由固定样例驱动断言，MUST NOT 只由测试内手写样例代表。受理面 MUST NOT 反向收紧：协议允许的组合不得因发送方的语义约定而被客户端拒收。

#### Scenario: 每一条有效固定样例都被受理

- **WHEN** 把 `fixtures/sync/v1/manifest.json` 里全部标记为有效的消息固定样例逐条交给解码边界
- **THEN** 每一条都被受理（不返回拒收），且清单覆盖解码边界支持的每一个消息类型

#### Scenario: 受理面可证伪

- **WHEN** 客户端的封闭词表取值集合或信封判定与协议资产不一致
- **THEN** 客户端门禁（`clients/app` 的检查入口）MUST 变红，MUST NOT 靠测试内手写样例另行通过而保持绿色

#### Scenario: 不因发送方约定而收紧受理面

- **WHEN** 收到协议允许、但发送方按语义约定本不该发出的组合（例如来源类型为设备而身份字段为空）
- **THEN** 客户端必须受理该帧，不得把它判为字段类型错误

### Requirement: 事件来源类型的五值词表与 node 配对规则

客户端 SHALL 把事件来源类型当作封闭的**五值**词表受理：`agent`、`device`、`daemon`、`local_cli`、`node`。来源类型为 `node` 时，客户端 SHALL 要求身份字段 `nodeId` 存在且为 UUID，并要求 `deviceId` 为空。这两个条件是协议对 `node` 唯一的配对规则，客户端 SHALL 在同一处判定，并以可区分的原因拒绝不满足的帧。

#### Scenario: 节点来源的事件被受理

- **WHEN** 收到一条来源类型为 `node`、`nodeId` 为 UUID 且 `deviceId` 为空的合法事件（固定样例 `valid/event-node-origin.json`）
- **THEN** 解码边界受理该帧，事件进入后续的呈现与游标推进路径，不被判为词表外取值

#### Scenario: 缺身份字段时拒绝

- **WHEN** 收到来源类型为 `node` 但根本没有 `nodeId` 键的事件（固定样例 `invalid/event-origin-node-missing-node-id.json`）
- **THEN** 该帧被拒，拒绝原因是「缺必填字段」

#### Scenario: 身份字段为空值时拒绝

- **WHEN** 收到来源类型为 `node`、`nodeId` 存在但取值为空的事件（固定样例 `invalid/event-origin-node-without-node-id.json`）
- **THEN** 该帧被拒，拒绝原因是「字段类型不符」，与上一条的原因可区分

#### Scenario: 冻结形状不被顺手收紧

- **WHEN** 收到未协商来源身份扩展的连接所能收到的冻结形状（只有来源类型与 `deviceId` 两个键，固定样例 `valid/event-origin-legacy-shape.json`）
- **THEN** 该帧仍被受理，MUST NOT 因本次新增的配对规则被判为缺字段

### Requirement: 错误帧的两种信封

客户端 SHALL 受理 `error` 帧的两种信封：认证前的信封省略连接标识与连接序号，认证后的信封两者都出现且形状合法。客户端 SHALL 以「两者同时出现或同时缺席」为判定：只出现其中一个时按信封违规拒收。`control.ping` 与 `control.pong` SHALL 只在认证后的信封下被受理。

#### Scenario: 认证前的错误帧被受理

- **WHEN** 收到一条认证前信封的错误帧（无连接标识与连接序号，固定样例 `valid/error-before-auth.json`）
- **THEN** 解码边界受理该帧，其错误码与说明进入客户端既有的诊断路径，MUST NOT 被判为信封违规

#### Scenario: 认证后的错误帧被受理

- **WHEN** 收到一条带连接标识与连接序号的错误帧
- **THEN** 解码边界受理该帧，错误码与说明进入同一条诊断路径

#### Scenario: 信封只出现一半时拒绝

- **WHEN** 收到一条错误帧，其中连接标识与连接序号只出现其中一个
- **THEN** 该帧被拒，拒绝原因是信封违规

#### Scenario: 心跳帧仍要求认证后信封

- **WHEN** 收到一条不带连接标识与连接序号的心跳帧
- **THEN** 该帧被拒，MUST NOT 因错误帧放宽了信封判定而被一起受理

## MODIFIED Requirements

### Requirement: 与已就绪 Daemon 的端到端可用

上述四项接线就位后，客户端 SHALL 能在与 Daemon 同源的部署下完成配对、建立经过完整认证的连接、并取得目录与会话的快照。这一结果 SHALL 由真实浏览器对真实 Daemon 的验收证明，MUST NOT 以对假客户端/假主机的用例代替。该验收**是否已执行** SHALL 在 `README.md` 与 `docs/FRONTEND_DESIGN.md` 中以同一口径登记，且 MUST 与该验收的真机验收记录一致：MUST NOT 出现一处称已完成、另一处称尚未执行。

#### Scenario: 同源部署下完成配对与连接

- **WHEN** 浏览器从 Daemon 托管的来源加载页面并完成一次配对
- **THEN** 客户端进入已配对并建立已认证连接，呈现连接状态为在线

#### Scenario: 取得目录与会话快照

- **WHEN** 上述连接认证完成并完成首次同步
- **THEN** 目录页呈现本机已登记目录与其会话，数据来自上次完成校验的快照

#### Scenario: 验收证据来自真实浏览器与真实 Daemon

- **WHEN** 本变更的验收被执行
- **THEN** 证据来自真实浏览器内核对真实 Daemon 的实际交互（含一次完整配对与连接），对假主机的用例只作为补充而不是验收依据

#### Scenario: 验收执行状态在两处文档中只有一种口径

- **WHEN** 检查 `README.md` 与 `docs/FRONTEND_DESIGN.md` 关于该验收执行状态的表述
- **THEN** 两者一致（都称已执行或都称未执行），且与真机验收记录的实际结果一致
