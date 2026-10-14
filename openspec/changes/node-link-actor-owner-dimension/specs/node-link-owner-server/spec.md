## MODIFIED Requirements

### Requirement: mutation 幂等键与冲突

可重试 mutation 的幂等键 MUST 至少含 `(ownerNodeId, accessNodeId, requestId)`：同一键重复提交 MUST 返回首次结果；`command`、`sessionRef`、`expectedVersion` 或解码后 `payload` 语义不同 MUST 返回 `nodelink.command.idempotency_conflict`。

**持久化表示必须保留全部两个节点维度**：`owned_command.actor_id` 上 node 行的文本 MUST 是 `"{ownerNodeId}/{accessNodeId}"`，其中 `ownerNodeId` 取**本 Owner 节点**的身份、`accessNodeId` 取发出该命令的 Access 对端。两个维度 MUST NOT 被填成同一个值 —— 那会让幂等键在存储里退化为单维度，来自不同 Access 对端的同一 `requestId` 因而不可区分，Owner 侧既无法拒绝复用也无法为每条事件归因。

由该字段回投影出的事件来源（`origin.nodeId`）因此 MUST 指向**产生该事件的另一个节点**，而不是对端自述的身份。

#### Scenario: 相同 requestId 重试不重复派发

- **WHEN** Access 以相同 `(requestId, command, payload)` 重复提交同一 mutation
- **THEN** Owner 返回首次的 accepted/terminal 结果，副作用只发生一次

#### Scenario: 同键不同语义被拒绝

- **WHEN** Access 以相同 `requestId` 但不同 `payload` 提交命令
- **THEN** Owner 返回 `nodelink.command.idempotency_conflict`，不执行第二次副作用

#### Scenario: 不同 Access 对端复用同一 requestId

- **WHEN** Access A 以某 `requestId` 提交命令并已落盘
- **AND** Access B 以同一 `requestId` 提交（`command` 或 `payload` 与 A 的相同）
- **THEN** Owner MUST NOT 把它当作 A 的重复提交而返回 A 的首次结果
- **AND** 两条命令 MUST 以不同的幂等键落盘（owner 相同、access 不同）

#### Scenario: node 类事件的 origin.nodeId 指向对端节点

- **WHEN** Access 节点经命令诱发一条事件，Owner 侧读回该事件并投影 `origin.nodeId`
- **THEN** 该值 MUST 等于该 Access 对端的节点标识，且 MUST NOT 等于 Owner 自身
- **AND** `owned_command` 里对应行的 `actor_id` MUST 形如 `"{ownerNodeId}/{accessNodeId}"` 且两半不同
