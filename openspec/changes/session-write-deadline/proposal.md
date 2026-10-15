# Proposal

## Why

`crates/server/src/transport/net/ws.rs` 的 `send_text`/`recv` 是**没有期限**的裸 await；node_link 与 sync 的会话都把写**内联**在业务循环里（`Event::Outbound` 分支），而心跳、90 秒静默判定与慢消费者看门狗都在同一个 `select!` 的 tick 分支里。因此**对端停止读取、socket 缓冲被填满时，写会一直 Pending，整个 select 停摆** —— 保活与判据在最需要它们的时候不再运行。

实测（变更 `node-link-saturation-recovery` 的相邻项 3.1，2026-10-16，主检出）：

- TLS（`TlsMode::Direct`，真实 WSS 形态）：写 1 MiB 与写 64 MiB **都在 3 秒内没有完成**；
- 明文 + **32 KiB 小帧连续写**（会话的真实写形态）：只写了 **7 条（≈224 KiB）就停住**；
- 仓库自己的用例文档（`conn/tests.rs`）早记下同机同形态的同一现象（「填满 socket 只需约 10～30 次 32 KiB 写」）。
- 单条超大帧（64 MiB）一次性写会返回 `Ok`——那是**写形态的伪影**，不代表对端收到，也不能据此推断容量。

## What Changes

- 给写路径加**期限**（超时按慢消费者/静默口径收尾），或把「对端是否真的在读」纳入饱和判据（按 socket 写完成的字节数记账，而不是只看队列占用）；两者取一或组合，取舍写进 design.md。
- 补回归：对端不读时，连接必须在有界时间内被收尾，且**心跳/静默/看门狗在写卡住期间仍被检查**。
- **非目标**：不改协议语义（不新增 wire 消息、不改 close code 语义）；不改 `NODE_LINK_PROTOCOL.md` 的限流与队列上限数值。

## Capabilities

### New Capabilities

（无。）

### Modified Capabilities

- `node-link-owner-server`：修改「心跳、超时与慢连接隔离」——补上「写路径必须有期限，且写卡住时判据仍被检查」。
- `sync-device-auth`：同步侧同一口径（两条入站适配器共用 `transport::net` 的写路径）。

## Impact

- `crates/server/src/transport/net/ws.rs`（写/读期限）、`crates/server/src/{node_link,sync}/conn/**`（会话收尾与饱和记账）。
- `docs/NODE_LINK_PROTOCOL.md` §2.5、`docs/SYNC_PROTOCOL.md` 的相邻条目（若结论改了口径）。
- 既有用例：`node_link`/`sync` 的饱和与静默用例可能需同步（它们的假设正是「写会卡住」）。
