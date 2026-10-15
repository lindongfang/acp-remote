# Design

## Context

动机见 `proposal.md` - Why。这里只记约束与现状事实：

- `Actor::Node` 的语义是 `node` = owner、`access_node` = 对端（`crates/core/src/model/identity.rs:123`），`id_text()` 把它写成 `"{node}/{access_node}"`（`:150-152`）。
- 生产中唯一的构造点 `node_actor()`（`crates/server/src/node_link/command.rs:2433-2439`）两个字段传入同一个值 `handle.node_id()`，因此落库的复合键两半恒等。
- 八个调用点（`grep -n "node_actor(" crates/server/src/node_link/command.rs`）都只拿得到对端；owner 身份在同一个 `self` 上：`NodeLinkService.node_id`（`:174`，由组合根 `authority.local_node()` 赋值，`:199`），`pending_snapshot()`（`:1782`）取的也是对端。
- 规格早已要求两个维度分开（`openspec/specs/node-link-owner-server/spec.md` 的「mutation 幂等键与冲突」：幂等键 MUST 至少含 `(ownerNodeId, accessNodeId, requestId)`）—— 所以这是**实现违反规格**，不是规格缺口。
- `owned_command` 的唯一键是 `(actor_kind, actor_id, request_id)`（§7.3），`actor_id` 是单列文本，因此 owner 维度只能编码进该文本。
- `sync_replay`（PR #106 已合入）按 `origin_kind = node` 时经 `causation` 回查该列，并用 `node_id_from_composite_key` 拆出第一段作为 `origin.nodeId`。

## Goals / Non-Goals

**Goals:**

- `owned_command.actor_id` 上 node 行的文本变为 `"{owner}/{access}"`，两半**不同**（同一 owner 视角下）。
- 八个调用点行为一致，`origin.nodeId` 指向对端节点而不是 owner 自身。
- 旧库里的旧形态行仍可被读面正确处理（不制造 `Corrupt` 误判）。

**Non-Goals:**

- 不改 wire：`(ownerNodeId, accessNodeId, requestId)` 三要素的传输形状不变。
- 不引入跨 Access 的 requestId 隔离增强（属 Node Link 协议的独立议题）。
- 不动 `crates/core` 的 `Actor` 建模（`Actor::Node` 已正确表达两个维度）。
- 不做"把历史行改写成新形态"以外的数据修复（见 Migration Plan）。

## Decisions

### D1：`node_actor` 改为方法，owner 从 `self` 取

```rust
fn node_actor(&self, access_node: &NodeId) -> Actor {
    Actor::Node { node: self.node_id.clone(), access_node: access_node.clone() }
}
```

**为什么**：owner 身份本来就只存在于 `self`（组合根装配时给定、进程内恒定），八处调用点全都有 `&self`。把 owner 作为形参传入只会得到一个没人能正确提供的参数。

**备选**：把 `self.node_id` 作为额外形参 `node_actor(owner, access)` —— 否决，因为每个调用点都要多传一个每次都相同的值，且无法防止传错。

**连带清理**：`poll_pending` 里经 `pending_snapshot()` 取得的变量名 `node` 就是对端，随之改为 `access_node`，消除命名带来的误导。

### D2：旧库行按"事实形态"读，不做数据迁移

读面 `node_id_from_composite_key` 拆出的第一段在旧库里等于对端。这个值**不是**新口径下的 owner，但它确实是"该命令实际记录的节点维度"。

**决定**：不写迁移把旧行改写成 `"{owner}/{access}"`。理由：

1. 旧形态行只有一种来源 —— 本缺陷写入的 node 命令；它们在旧口径下两半相同，改写需要**知道**当时的 owner，而库里没有第二个值可恢复（owner 维度从未落库）。
2. 任何"猜一个 owner"的改写都是伪造数据。
3. 读面拿到的值仍然指向对端节点，即"另一个节点"，语义上没错，只是少了 owner 维度 —— 与 `sync_replay` 所需的 `origin.nodeId`（产生该事件的另一个节点）**恰好一致**。

**代价（如实记）**：旧行的 `origin.nodeId` 是对端而非 owner 视角下的区分值。由于 Node Link 至今没有真实用户（本阶段仍是纵向切片），接受此代价；若将来出现需要区分"同一对端、不同 owner"的场景，再立变更处理。

### D3：不新增 `FILE_FORMAT_VERSION`

迁移只升级 schema；本变更不升级 schema，因此**不 bump 版本号**（`migrate.rs:19` 的 7 保持不变）。这与 D2 是一体的：不迁数据就不需要新版本段。

### D4：判别力靠"键的可观察行为"，不是靠 SQL 文本

用例断言的是：同一 `requestId` 下两条不同对端的命令**都能落库且互不覆盖**（旧实现会把两行进同一个 `("node", requestId)` 键），以及 `origin.nodeId` 等于对端。

## Risks / Trade-offs

- **[既有幂等重放口径变严]** → 旧实现里"同一对端重复提交同一 requestId"仍返回首次结果（这点不变，唯一键后两段都相同）；新增的是"不同对端同 requestId 不再冲突"，这是规格原本要求的行为，不是放宽。
- **[`command.status` 的回查路径]** → `on_status`/`poll_pending` 用 `node_actor(...)` 构造 actor 去查 `owned_command`；改成新口径后，查旧行会因键不匹配而 `not_found`。缓解：本阶段无真实远程命令数据；用例里显式覆盖"旧行查不到"这一已知行为，而不是掩盖它。
- **[测试替身的形状]** → 若替身仍写 `a/a` 形态，用例会假绿。缓解：D4 的断言直接比对两半是否不同。
- **[遗漏调用点]** → 八处中漏改一处会造成同一连接内两种 actor 形状。缓解：`node_actor` 改为方法后，签名从 `(node: &NodeId)` 变为 `(&self, access_node: &NodeId)`，漏改的调用点**无法编译**。

## Migration Plan

1. 改 `node_actor` 签名与实现（D1），八处调用点随之更新。
2. 更新 `node_link` 既有用例的期望值（凡按 `a/a` 文本断言的）。
3. 新增用例（D4 的形状）。
4. 不 bump `FILE_FORMAT_VERSION`，不写迁移段（D2/D3）。
5. 回滚策略：纯代码回滚即可（schema 未变，旧库仍可读；回滚后读面退回"拆出对端"的行为，仍是确定的）。

## Open Questions

无。D2 的"将来是否需要 owner 视角"可能改变任务拆分，因此不在 Open Questions 里延后 —— 已按"本阶段无真实数据"作出决定并记录代价。
