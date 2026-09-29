## MODIFIED Requirements

### Requirement: Export 过滤的 catalog 投影

`catalog.subscribe` 的 `knownRevision` 为 `null` 时表示首次获取；非空时 Owner SHALL 仍返回当前完整快照（`catalog.changed` 增量属 post_mvp，本切片不实现）。`catalog.snapshot` SHALL 从 Owner 自己的持久化 Export 记录投影，只包含该 Access **可见**的 Export——可见性规则（`NODE_LINK_PROTOCOL.md` §8.2，2026-09-27 用户裁决）：① 该 Export 未撤销、② `export.scopes ∩ 该节点信任记录的 grants ≠ ∅`、③ `export.exportId ∈ 该节点信任记录的 exportIds`，三条同时成立才可见。清单只收窄不放宽：condition ③ 为空的节点 MUST 看不到任何 Export，即使其 `scopes` 与 `grants` 相交。批次内条目顺序 MUST 稳定，批次大小不超过协商的 `catalogSnapshotBatchSize`（默认 500）。Export 条目 SHALL 携带 §12.3 登记的全部字段（含 `defaultWorkspaceAlias`、`templates`、`scopes`、`capabilityCeilingRef`、`cachePolicy = "no-content-cache"`、`revoked` 标记）；首切片发布的 template MUST 是零参数（`params = []`）。catalog 是绑定当前连接的内存投影，Owner 不得为 Access 额外持久化副本。

#### Scenario: 按信任记录过滤

- **WHEN** Owner 有两个 Export（E1、E2），E2 的 scopes 与该 Access 信任记录的 grants 不相交（E1 相交），完成握手后订阅 catalog
- **THEN** `catalog.snapshot` 只含 E1 的条目，E2 的任何字段不出现在响应中

#### Scenario: 清单收窄可见集

- **WHEN** Owner 有两个 Export（E1、E2），两者的 scopes 都与该节点的 grants 相交，但该节点信任记录的 `exportIds` 只列了 E1，完成握手后订阅 catalog
- **THEN** `catalog.snapshot` 只含 E1 的条目，E2 不出现（即使其 `scopes ∩ grants` 非空）

#### Scenario: 空清单看不到任何 Export

- **WHEN** 某节点信任记录的 `exportIds` 为空集合，且本机存在多个与其 grants 相交的未撤销 Export
- **THEN** 握手与 catalog 请求正常完成，但 `catalog.snapshot` 的可见集为空，不返回任何 Export 条目

#### Scenario: 清单内 Export 被撤销后不可见

- **WHEN** E1 在该节点 `exportIds` 内，随后被 `export.revoke`，Access 重新订阅 catalog
- **THEN** E1 不再出现在 `catalog.snapshot` 中（可见性仍要求未撤销），且该节点的 `exportIds` 条目本身 MUST NOT 被级联清理

#### Scenario: 超大批次稳定切分

- **WHEN** 可见 Export 条目数超过协商批次大小
- **THEN** snapshot 分成多条消息，批次内顺序稳定，接收方按序拼接后得到完整视图
