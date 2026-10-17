# Node Link Owner Server 增量

## REMOVED Requirements

### Requirement: Export 可见性过滤与 catalog 快照

**Reason**: Export 整族删除；已配对节点不再按 Export/grants 过滤。

**Migration**: 由「全量 catalog 快照」承接。

### Requirement: Export 条目字段与零参数 template

**Reason**: catalog 不再有 Export 条目。

**Migration**: 目录、Agent 与会话资源直接投影本机目录表与配置。

## ADDED Requirements

### Requirement: 全量 catalog 快照（无 Export 维度）

`catalog.subscribe` 的 `knownRevision` 为 `null` 时表示首次获取；非空时 Owner SHALL 仍返回当前完整快照。`catalog.snapshot` MUST 从本机目录、Agent 配置与会话记录直接投影，不得按 Export 或授权集合过滤：所有已配对且未撤销的节点看到同一份资源清单。`local.*` 能力仍不可远程调用。

#### Scenario: 已配对节点看到全部资源

- **WHEN** 一个已配对且未撤销的 Access 节点完成握手并订阅 catalog
- **THEN** `catalog.snapshot` 包含本机全部目录、Agent 与会话，不受任何 Export 或 grant 影响

#### Scenario: 撤销节点即时失去访问

- **WHEN** 本机撤销该节点后它尝试订阅 catalog
- **THEN** 握手或请求被拒绝，不再返回资源
