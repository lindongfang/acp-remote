# node-link-actor-owner-dimension：让 Node Link 幂等键的两个节点维度分开填

## Why

Sync 事件读面（`sync-server-connection` 的 1.6）的独立检视发现一处**既有的 `node_link` 生产缺陷**，由该读面首次暴露为可观测行为。

`crates/server/src/node_link/command.rs:2433-2439` 的 `node_actor` 把 `Actor::Node` 的两个字段**都填成同一个值**：

```rust
fn node_actor(node: &NodeId) -> Actor {
    Actor::Node {
        node: node.clone(),            // 注释自称 owner
        access_node: node.clone(),     // 调用点传的是 handle.node_id() = 该对端
    }
}
```

八个调用点传入的都是 `handle.node_id()`，即**对端 access node**。于是 `owned_command.actor_id` 落库的复合键 `"{node}/{access_node}"` 两半恒等。

后果不是 wire 投影错误，而是**§12.5 幂等键在持久化表示上退化为单维度**：

```
owner 维度丢失 ⇒ 同一 requestId 在不同 Access 对端之间的命令在 actor 维度上不可区分
⇒ 若 Access 侧复用 requestId，`command_origins` 会把两行折叠进同一个 ("node", requestId) 键
⇒ sync_replay 命中「同 requestId 多行」而整批返回 Corrupt，把【合法历史行误判为不可归因】
```

同时它让 `sync_replay` 的 `origin.nodeId` 报出一个语义维度缺失的值：拆出来的“owner”实际是对端自己，而不是产生该事件的**另一个**节点。

## What Changes

- `node_actor` 改为方法：`node`（owner 维度）取本进程身份 `self.node_id`（由组合根 `authority.local_node()` 赋值），`access_node` 取该连接对端。
- 八个调用点全部改为新签名；`1757` 处经 `pending_snapshot` 取得的 `node` 变量本就是对端，可随之简化。
- 补上被检视指出的两处覆盖缺口（详见 tasks.md）。

## 影响面判据（必须逐条核对）

- **幂等行为**：`owned_command` 的唯一键 `(actor_kind, actor_id, request_id)` 中 node 行的 `actor_id` **文本会改变**（从 `a/a` 变成 `owner/a`）。凡按该文本检索、比对或断言的既有用例必须逐条重跑，不得假定不变。
- **旧库中的旧行**：已落库的 `a/a` 形态行在新代码下回查 `origin.nodeId` 会拆出对端而非 owner。是否需要兼容读取、还是按「事件重放前 Node Link 尚未有真实用户」为由不迁移，需在 design 里给出结论。
- **`127.0.0.1` 之外的部署**：本机身份由 `authority.local_node()` 给出，不依赖部署形态。

## 明确不做什么

- 不引入跨 Access 的 requestId 隔离增强（那属于 Node Link 协议层的独立议题，本变更只修正幂等键的**填法**）。
- 不改 Sync 侧任何代码（`sync_replay` 的读面逻辑正确，缺陷在它读取的数据的写入方）。
