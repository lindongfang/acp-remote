## MODIFIED Requirements

### Requirement: 节点配对与信任方法

`node.pair.begin mode = "owner"` SHALL 在本节点创建一次性配对并返回二维码 URL；`node.pair.confirm` MUST 在持久提交后才返回，只有 Owner 侧本地确认才创建信任记录并分配初始 `grant.*`。`node.pair.confirm` 的 `params` MUST 携带 `exportIds`（该节点可见的 Export 集合；可为空数组，表示看不到任何 Export）——缺少该字段 MUST 以 `local.invalid_params` 拒绝，不创建信任。每个 `exportId` MUST 在本机存在且未撤销（否则 `local.not_found`），且其 `scopes` 与本次 `grants` 有交集（否则 `local.invalid_params`）；三类失败都 MUST NOT 创建信任记录、推进配对状态或写审计。`node.pair.confirm` 的 `result` 与 `node.list` 的 `NodeRecord` SHALL 回显 `exportIds`。清单 MUST NOT 放宽可见性：有效可见集仍是「未撤销 ∧ `scopes ∩ grants ≠ ∅` ∧ 在 `exportIds` 内」的交集。`node.revoke` 后该节点 MUST NOT 再建立 Node Link 连接，本地停止重连并关闭 active connection。本机对同一 `nodeId` 再次完成 `node.pair.confirm`（重新配对，含密钥变化后的重新配对）时，提交后 MUST 立即关闭该节点的活动连接：关闭码 MUST 为 `1000`（正常关闭）并给出说明性 close reason，且 MUST NOT 推送 `node.trust.revoked`——这是「授权已变化、请重新握手」而非撤销，因此收窄自下一次握手后的新连接生效；只有 `node.revoke` 才推 `node.trust.revoked` 并以 `4410` 关闭、停止重连（两条路径的关闭语义与关闭码 MUST 可区分）。本切片 `mode = "access"`（需对 Owner 的 HTTPS claim）与 `node.rotate-key.begin` MUST 返回 `local.unsupported`，不得自行填充参数形状或伪造 claim。

#### Scenario: Owner 模式节点配对

- **WHEN** 以 `mode = "owner"` 调用 `node.pair.begin`，对端 claim 后经 `node.pair.status` 观察，本机用户确认后调用 `node.pair.confirm`
- **THEN** confirm 提交成功后节点记录为 `paired` 并带初始 grants；拒绝或过期路径不创建信任

#### Scenario: 确认时填报可见清单

- **WHEN** 本机用户确认配对时给出 `grants` 与两个存在的未撤销 `exportIds`（E1、E2）
- **THEN** 信任记录带上该清单，`node.list` 与 confirm 的 `result` 回显同样的集合，对端随后只能看到这两个 Export

#### Scenario: 空清单仍然建立信任

- **WHEN** 本机用户确认配对时给出空 `exportIds`
- **THEN** 信任记录创建成功（对端可连接、可握手），但该节点看不到任何 Export；该结果 MUST NOT 被当作配对失败

#### Scenario: 无效清单条目拒绝确认

- **WHEN** 确认时给出的 `exportId` 在本机不存在或已撤销，或者其 `scopes` 与本次 `grants` 不相交
- **THEN** 分别以 `local.not_found` / `local.invalid_params` 失败，且不创建信任记录、不推进配对状态、不写审计

#### Scenario: 重新配对收窄清单后作废既有连接

- **WHEN** 某节点已 `paired` 且存在活动的 Node Link 连接与已建立的 attachment，本机用户再次对同一 `nodeId` 完成 `node.pair.confirm`（携带更窄的 `exportIds`，例如空数组）
- **THEN** 提交后该节点的活动连接 MUST 立即被关闭，关闭码为 `1000` 且不带 `node.trust.revoked` 通知；对端按 `NODE_LINK_PROTOCOL.md` §15 的重连规则重新握手，新连接上的可见集按已提交的清单重算（因此它看不到被移出清单的 Export），而该节点的信任记录仍为 `paired`（重新配对不是撤销）

#### Scenario: access 模式明确不支持

- **WHEN** 本切片内以 `mode = "access"` 调用 `node.pair.begin`，或调用 `node.rotate-key.begin`
- **THEN** 返回 `local.unsupported`，无状态变更、无对外网络请求
