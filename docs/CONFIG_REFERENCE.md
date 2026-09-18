# ACP Remote 配置参考

> 状态：编码前基线
> 版本：0.1
> 日期：2026-09-18
> 上位文档：[INITIAL_DESIGN.md](./INITIAL_DESIGN.md)

本文是 Daemon 运行时配置键名、类型、默认值与可否调整的**唯一权威来源**。协议层限额不在此重复定义：

- Sync 的限额与其中哪些可经 `auth.authenticated.limits` 下发，见 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §14。
- Node Link 的限额与其中哪些可经 `node.ready.limits` 下调，见 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §2.5。
- 安全默认值（监听地址、TLS 终止、失败关闭）的约束见 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §7、§8、§9。

配置来源与优先级（高者覆盖低者）：命令行参数 > 配置文件 > 内置默认值。环境变量只用于替换配置文件路径（`ACP_REMOTE_CONFIG`），不承载业务配置，避免把凭据写进进程环境。

## 1. `daemon`

| 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `daemon.data_dir` | path | 平台用户配置目录下的 `acp-remote/` | SQLite、附件、日志的根目录；权限要求见 `SECURITY_DESIGN.md` §13.2 |
| `daemon.listen` | string | `"127.0.0.1:8765"` | 本地监听地址；默认只监听 loopback，监听 `0.0.0.0`/`[::]` 必须显式配置并在启动输出中告警 |
| `daemon.public_origin` | string\\|null | `null` | canonical public origin（`scheme://host[:port]`）；配置了远程入口就必须给出，用于 Origin/Host 校验与配对二维码 |
| `daemon.allowed_hosts` | string[] | `[]` | 反向代理场景下允许的 `Host` 白名单；为空时只接受与 `public_origin` 一致的 Host |
| `daemon.trusted_proxies` | string[] | `[]` | 允许终止 TLS 的同机代理地址；非空时才考虑 `Forwarded`/`X-Forwarded-*` |
| `daemon.instance_lock` | enum | `"file"` | 单实例锁实现：`file`\\|`ipc`；见 `SECURITY_DESIGN.md` §12.1 |
| `daemon.shutdown_grace_ms` | integer | `10000` | 关闭时等待接入层停止、Agent 退出与存储刷新的上限 |

## 2. `sync`

只列出运行时可调项，具体默认值与上限语义以 `SYNC_PROTOCOL.md` §14 为准：

| 键 | 默认值 | 对应协议字段 |
|---|---:|---|
| `sync.max_message_bytes` | `1048576` | `auth.authenticated.limits.maxMessageBytes` |
| `sync.max_prompt_bytes` | `262144` | `auth.authenticated.limits.maxPromptBytes` |
| `sync.max_replay_events_per_batch` | `500` | `auth.authenticated.limits.maxReplayEventsPerBatch` |

上表三项是 v1 中唯一允许按部署下调的 Sync 限额；§14 的其余上限是固定常量，不得通过配置放宽或收窄。

## 3. `node_link`

只列出运行时可调项，默认值与可下调集合以 `NODE_LINK_PROTOCOL.md` §2.5 为准：

| 键 | 默认值 | 对应协议字段 |
|---|---:|---|
| `node_link.max_message_bytes` | `1048576` | `node.ready.limits.maxMessageBytes` |
| `node_link.catalog_snapshot_batch_size` | `500` | `node.ready.limits.catalogSnapshotBatchSize` |
| `node_link.resource_snapshot_batch_size` | `500` | `node.ready.limits.resourceSnapshotBatchSize` |
| `node_link.max_in_flight_commands` | `32` | `node.ready.limits.maxInFlightCommands` |
| `node_link.max_pending_queue_bytes` | `8388608` | `node.ready.limits.maxPendingQueueBytes` |
| `node_link.max_pending_queue_messages` | `2000` | `node.ready.limits.maxPendingQueueMessages` |
| `node_link.heartbeat_interval_ms` | `30000` | `node.ready.limits.heartbeatIntervalMs` |

其余 Node Link 上限（握手超时 15 秒、心跳超时 90 秒、JSON 嵌套深度 64、单对象字段数 1024、单数组元素数 10000、节点名称 128 bytes）是固定 v1 常量。

## 4. `sessions`

| 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `sessions.queue_policy` | enum | `"queue"` | 同一会话已有 active turn 时的新 prompt 处理：`queue` 按持久化接受顺序排队；`reject_busy` 直接返回 `session.busy`。v1 默认排队，与 `SYNC_PROTOCOL.md` §11.6 的二选一一致 |
| `sessions.max_queued_turns` | integer | `16` | `queue` 策略下的排队上限，超出返回 `session.busy` |
| `sessions.idle_timeout_ms` | integer | `0` | `0` 表示不因空闲关闭会话；非零时仅影响 Daemon 侧资源，不影响事件与游标 |

## 5. `storage`

| 键 | 默认值 | 说明 |
|---|---:|---|
| `storage.transcript_retention_days` | `90` | 最终消息、结构化事件、权限结果的保留期 |
| `storage.sync_event_retention_days` | `7` | 带 sequence 的同步事件（含短期 delta）保留期；清理后 cursor 超出窗口即触发 snapshot reset |
| `storage.max_total_size_bytes` | `2147483648` | 整库容量上限（2 GiB） |
| `storage.max_session_size_bytes` | `104857600` | 单会话内容上限（100 MiB） |
| `storage.persist_deltas` | `false` | 只影响 turn 完成后是否压缩/清理短期 delta，不允许绕过“先持久化、后广播” |
| `storage.flush_interval_ms` | `250` | 流式事件的批量落盘间隔 |
| `storage.attachment_max_file_bytes` | `20971520` | 单个附件上限（20 MiB） |
| `storage.attachment_max_total_bytes` | `1073741824` | 附件总量上限（1 GiB） |

Access Node 的 `no-content-cache` 不是配置项：它由 Export 固定，Access 侧没有可关闭的开关（`NODE_LINK_PROTOCOL.md` §10、`SECURITY_DESIGN.md` §13.4）。

## 6. `terminal`

| 键 | 默认值 | 说明 |
|---|---:|---|
| `terminal.max_output_per_command_bytes` | `1048576` | 单条命令终端输出上限（1 MiB） |
| `terminal.keep_head_bytes` | `131072` | 截断时保留的头部（128 KiB） |
| `terminal.keep_tail_bytes` | `917504` | 截断时保留的尾部（896 KiB），必须带截断标识 |

## 7. `agents`

```toml
[[agents.profiles]]
agent_id = "codex"
command = "codex-acp"
args = []
env_allowlist = ["PATH", "HOME"]     # 只传递列出的环境变量，不含任何 ACP Remote 密钥
default = true

[[agents.profiles]]
agent_id = "omp"
command = "omp"
args = ["acp"]
```

- `agent_id` 是稳定标识，进入 `SessionSummary.agent.agentId`；启动 profile 与 capability 差异按 `MODULE_ARCHITECTURE.md` §9 只作为数据，不在核心堆积 Agent 名称判断。
- 环境变量白名单是上限而非提示：未列出的变量不得注入子进程（`SECURITY_DESIGN.md` §12.2）。
- workspace、Provider/MCP 凭据不在本文件：它们由本地管理入口（`local.workspace.select`、`local.provider.configure`）在 Daemon 内维护。

## 8. `identity`

| 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `identity.keystore` | enum | `"platform"` | `platform` 使用 DPAPI/CNG、Keychain、Secret Service；`ephemeral` 仅允许显式开发模式 |
| `identity.fail_closed_on_missing_keystore` | boolean | `true` | 平台 keystore 不可用时正式模式拒绝启动；`false` 只能出现在开发模式配置中 |

## 9. `imports`

Access Node 侧每个 Import 一条记录（由 `acp-remote import add` 写入，不由用户手改）：

```toml
[[imports]]
import_id = "imp-1"
owner_endpoint = "wss://owner.example.ts.net/node-link/v1"
owner_node_id = "bdb2ec20-f98c-4d87-b789-e540d527ef87"
export_ids = ["exp-work"]
grants = ["grant.observe", "grant.interact"]
```

- `grant.*` 取值以 [`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json) 为准；有效权限是 `Owner grant ∩ Access local grant ∩ runtime capability`。
- 本地客户端能做什么由 Access 自己的设备 scope 决定，Import 记录只描述远程侧授权。

## 10. `dev_mode`

| 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `dev_mode.enabled` | boolean | `false` | 发布构建默认关闭；启用时启动输出与 UI 必须持续显示非安全状态 |
| `dev_mode.allow_plaintext` | boolean | `false` | 允许明文 HTTP/WS，且只允许 loopback |
| `dev_mode.ephemeral_identity` | boolean | `false` | 使用进程期临时身份，不复用正式设备/节点数据库 |
