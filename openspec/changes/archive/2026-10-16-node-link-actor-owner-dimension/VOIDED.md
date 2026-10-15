# 作废：node-link-actor-owner-dimension

**结论：本变更作废。原缺陷报告不成立 ——「幂等键缺 owner 维度」是误判。**

## 事实链（三条独立证据）

1. **core 的 `Actor::Node::node` 被冻结为「对端」节点 id**，不是 Owner 自身：
   `docs/CORE_PORTS_AND_STORAGE.md:164` —— 「`Node` 的 `node` 是**对端（claimant／连接对端）节点 id**（**本轮钉死**）：Owner 侧即 claim 里声明的 `accessNodeId`，Access 侧即本地 Import 的 `ownerNodeId`（`NODE_LINK_PROTOCOL.md` §12.5）」。
2. **授权只查这一半**：`crates/core/src/broker.rs:841` 的 `Actor::Node { node, .. } => self.node_allowed(node, command, session)`；core 用例 `node_command_records_the_node_origin` 把该口径钉死（改用 `access_node` 即失败）。
3. **键的形状不构成折叠**：`command_origins`（`session_store.rs:720-769`）的键是 `(actor_kind, request_id)`，`actor_id` 只是值；两半相同 ⇒ 两行的 `actor_id` 也相同 ⇒ 命中 `Entry::Occupied(_) => {}`（不置 `None`） ⇒ **不会**触发 `Corrupt`。

## 我原先的判断错在哪

我把 `node_id_from_composite_key` 拆出的第一段读成"owner"，据此判定"库里两半恒等 ⇒ 幂等键退化为单维度 ⇒ 不同 Access 对端复用同一 requestId 会被误判为不可归因"。

但 core 的语义里第一段**就是对端**；而 `sync_replay` 的 `origin.nodeId` 需要的是「产生该事件的**另一个**节点」—— 拆出的值恰好满足。**原实现是正确的。**

## 上行检视的报告为何也不成立

它担忧的 `Corrupt` 需要回查出**不同的** `actor_id`；而在 core 语义下，两个不同 Access 对端的 `Actor::Node` 会给出不同的 `access_node`（第二段）、相同的 `node`（第一段，都是 claim 里的对端 id）⇒ `id_text()` 不同 ⇒ 键不同 ⇒ 不折叠。

## 代价（如实记）

实现包 `NodeLinkOwnerDim` 按 D1 完整实现后实跑：**18 条 server 用例失败**（命令在 `Broker::authorize` 即被拒，`nodelink.export.not_granted`），反向改 core 也撞 core 自己的用例。它**拒绝交付**一个跑不过测试的实现，并把阻塞点与四条出路报回 —— 这个判断是对的。

## 不再做的事

- 不改 `node_actor`（core 授权模型要求两半同值）。
- 不 bump `FILE_FORMAT_VERSION`，不写迁移。
- 不改 `node_id_from_composite_key`（其"拆第一段"的口径与 core 语义一致）。
