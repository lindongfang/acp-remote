# Local Admin Methods 增量

## REMOVED Requirements

### Requirement: Export 与 Import 管理方法

**Reason**: Export/Import 整族删除（表、方法、CLI 命令）。

**Migration**: 跨节点可用性由节点信任记录直接表达。

### Requirement: 节点配对可见清单的填报与校验

**Reason**: 不再有 Export 清单或 grants；节点配对不携带任何授权集合。

**Migration**: 配对确认只创建信任记录，不填报清单。

## MODIFIED Requirements

### Requirement: 节点配对建立与本地确认

`node.pair.begin mode = "owner"` SHALL 在本节点创建一次性配对并返回二维码 URL；`node.pair.confirm` MUST 在持久提交后才返回，只有 Owner 侧本地确认才创建信任记录。节点展示名 MUST 来自本机配置 `[node] name`，MUST NOT 取自请求体；配对请求 MUST NOT 携带 `grants`、`scopes` 或其他授权集合。本切片 `mode = "access"` 与 `node.rotate-key.begin` MUST 返回 `local.unsupported`。

#### Scenario: Owner 模式节点配对

- **WHEN** 以 `mode = "owner"` 调用 `node.pair.begin`，对端 claim 后本机用户确认
- **THEN** confirm 提交成功后节点记录为 `paired`，不含任何授权集合

#### Scenario: access 模式明确不支持

- **WHEN** 本切片内以 `mode = "access"` 调用 `node.pair.begin`，或调用 `node.rotate-key.begin`
- **THEN** 返回 `local.unsupported`，无状态变更、无对外网络请求
