# Spec Delta

## MODIFIED Requirements

### Requirement: 目录相关输出不泄漏规范化路径

目录相关的一切对端可见输出 MUST 只含 `alias` 与 `displayName`（会话摘要中的目录字段亦然），包括快照资源与快照 item、会话摘要、事件载荷、错误 `details`、`command.result` 与审计记录的前像。规范化路径、其父级片段与末段 MUST NOT 出现在这些输出中的任何字段（含摘要计算的输入）；客户端自己提供的目录 MUST NOT 被回显为对端可见字段。

#### Scenario: 快照与会话摘要只含引用

- **WHEN** 检查一次完整快照与全部会话摘要
- **THEN** 其中任何字段都不含本机规范化路径明文或它的分段，客户端自己提供的目录也不出现

#### Scenario: 错误响应不回显路径

- **WHEN** 一次目录相关请求因形状或解析失败被拒
- **THEN** 错误 `details` 只含别名或字段名，不含任何路径明文

## ADDED Requirements

### Requirement: 目录资源的协商与下发范围

两个新增资源 SHALL 受 feature `core.local-catalog.v1` 门控：客户端未协商该 feature 时 MUST NOT 收到它们，也 MUST NOT 收到空数组占位；`body.requiredFeatures` 含该 feature 而对方未协商时，接收方 MUST 返回 `protocol.feature_required`。会话摘要的 `workspace` 字段 SHALL 是同一 feature 门控下的可选字段：未协商该 feature 的客户端 MUST NOT 在会话摘要中收到该字段（SKIP），且它 MUST NOT 出现在摘要的必填集合中，以免旧客户端因未知字段拒绝整条消息。目录资源不再按设备授权过滤：任何已认证设备都收到本节点完整的已登记目录与已配置 Agent 集合。

#### Scenario: 未协商时不发送该资源

- **WHEN** 客户端未在握手协商中声明 `core.local-catalog.v1`
- **THEN** 快照中既不出现 `workspaces` 与 `agents` 资源，也不出现空数组替代

#### Scenario: 未协商时摘要不含目录字段

- **WHEN** 一台未协商 `core.local-catalog.v1` 的旧客户端接收会话摘要
- **THEN** 摘要中不出现 `workspace` 字段（不是 `null`，而是该字段缺席），整条消息仍能通过该客户端自己的 schema 校验

#### Scenario: 已认证设备收到完整目录

- **WHEN** 一台已认证设备协商了 `core.local-catalog.v1`
- **THEN** 它收到的快照含本节点全部已登记目录与已配置 Agent，不因设备维度而缺少任何条目

## REMOVED Requirements

### Requirement: 目录资源的协商与授权过滤

**Reason**：设备配对不再授予授权，`资源 SHALL 按设备授权过滤` 这一半前提消失；目录资源的 feature 门控与摘要字段的可选性仍然有效，已并入本 delta 的新增要求。

**Migration**：见新增要求「目录资源的协商与下发范围」。
