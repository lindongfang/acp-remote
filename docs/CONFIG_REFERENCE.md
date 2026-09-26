# ACP Remote 配置参考

> 状态：编码前基线
> 版本：0.6
> 日期：2026-09-18
> 修订记录（2026-09-18）：补充 `daemon.tls.*`、`daemon.local_admin.endpoint`、`logging.*` 与 Owner 侧 `[[exports]]`（原先只有 Access 侧 `[[imports]]`）；明确限流与协议上限是固定 v1 常量而非配置键。
> 修订记录（2026-09-18，0.3）：`[[exports]].grants` 更名为 `scopes`，与 Node Link Export 模型及本地通道 `ExportView` 同名同义（Access 侧 `[[imports]].grants` 是另一概念，不变）。
> 修订记录（2026-09-18，0.4）：新增 `storage.audit_retention_days`（默认 365），审计清理排在容量顺序最后。
> 修订记录（2026-09-18，0.5）：补齐 `storage.attachment_dir`——`CORE_PORTS_AND_STORAGE.md` §7.1 新增的附件目录配置键，此前只在合同里定义、未落到本表；§5.1 增加「备份与拷贝」说明（WAL 模式下不能只拷主库文件）。
> 修订记录（2026-09-23，0.6）：核对「配置与管理状态的权威」与 seed 语义——与 `CORE_PORTS_AND_STORAGE.md` §5.3 的 `LocalConfigStore`/`SeedWrite` 一致（首次初始化单事务导入全部合法 profile、空列表也标记完成、初始化完成后数据库是唯一权威、重启不重导、旧配置不得复活已撤销 Export 或已删除 Import），本轮**不改语义**；凭据注入小节的引用由 §11.6 改为 §5.3，管理状态的持久化边界与升级规则改指 §7.2/§7.3/§7.4 与 §11。
> 修订记录（2026-09-23，0.7）：管理 store 的落盘实现已落地（`crates/storage-sqlite/src/admin/`），标题与「待实现」标记随之去掉；配置键名、默认值与 seed 语义**未变**（仍以本节为唯一权威）。
> 修订记录（2026-09-26，0.8）：按 `node-link-owner`（切片 5）的实绩写回三处——§1 的 `daemon.allowed_hosts` 行补「`public_origin` 与 `allowed_hosts` 都为空（默认配置）时只接受 loopback 形态 `Host`」这一分支（原先只写了「为空时按 `public_origin` 推导」）并在 §1 末尾补 `public_origin` 的 canonical origin 口径与实现现状；§2/§3 加「本交付版本只解析、不消费」注记（`sync.*` 与 `node_link.*` 均被登记为未接线）。键名、类型、默认值**未变**（仍以本节为唯一权威）。
> 上位文档：[INITIAL_DESIGN.md](./INITIAL_DESIGN.md)

本文是 Daemon 运行时配置键名、类型、默认值与可否调整的**唯一权威来源**。协议层限额不在此重复定义：

- Sync 的限额与其中哪些可经 `auth.authenticated.limits` 下发，见 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §14。
- Node Link 的限额与其中哪些可经 `node.ready.limits` 下调，见 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §2.5。
- 限流与其余协议上限（认证尝试速率、命令速率、配对有效期、单消息与嵌套上限等）都是**固定 v1 常量**，不是配置键；本文件不提供放宽它们的开关。
- 安全默认值（监听地址、TLS 终止、失败关闭）的约束见 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §7、§8、§9。

启动配置来源与优先级（高者覆盖低者）：命令行参数 > 配置文件 > 内置默认值。环境变量只用于替换配置文件路径（`ACP_REMOTE_CONFIG`），不承载业务配置，避免把凭据写进进程环境。此优先级不适用于下面的 Daemon 管理状态。

### 配置与管理状态的权威（2026-09-23）

| 数据 | 唯一持久化权威 | 写入与生效时机 |
|---|---|---|
| daemon / sync / node_link / sessions / storage / terminal / identity / dev_mode / logging 启动参数 | 用户配置文件，经 CLI 参数覆盖 | 启动时读取并校验；v1 不提供文件监听或通用热重载，修改后重启生效 |
| Agent profile | SQLite 管理记录 | `agent.configure` 提交后供后续 Agent 进程使用；已有进程不变 |
| workspace alias → 本机路径 | SQLite 管理记录 | `workspace.select` 提交后供新建会话使用；已有会话不迁移目录 |
| 设备/节点信任、Export、Import | SQLite 管理记录 | 本地管理操作提交后生效；撤销按安全协议立即阻断相应访问 |
| Provider/MCP 凭据、Node 私钥 | 平台 keystore | 普通配置与 SQLite 只保存非秘密标识/引用；凭据修改只影响后续使用该配置启动的进程 |

- SQLite 的管理状态由运行中的 Daemon 单一写入；CLI 经本地 IPC 请求修改，不自行写库、不回写 TOML。持久化边界与升级规则见 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §7.2/§7.3/§7.4 与 §11。
- §7 的 `[[agents.profiles]]` 是**首次初始化种子**：在管理配置首次初始化的单次事务中导入全部合法 profile，并记录初始化完成标记；空列表也记录完成。任一项非法则整批失败。已有同 ID 管理记录与种子不一致时显式报错，不覆盖、不合并。
- 初始化完成后，数据库是 profile 的唯一权威；重启不再导入种子。配置文件仍含 profile 时给出不含参数值的提示，后续修改应使用 `agent.configure`；不能因删空数据库中的某一条 profile 就再次导入它。
- §9 的 TOML 片段是**管理记录的说明性表示**，不是用户配置文件支持的输入；启动配置中出现 `imports`/`exports` 必须明确拒绝并指向本地管理命令，不能忽略它们或据此创建信任。这样旧配置无法在重启时复活已撤销的 Export 或已删除的 Import。
- workspace 路径、信任与授权集合不接受启动参数覆盖；keystore 不可用或 SQLite 写入失败时管理方法返回明确失败，不能仅在内存修改后报告成功。
- 重启加载已提交的管理记录，重建 Agent catalog、Export 与 Import 路由；撤销状态始终优先。恢复完成前不开放业务接入。

## 1. `daemon`

| 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `daemon.data_dir` | path | 平台用户配置目录下的 `acp-remote/` | SQLite、附件、日志的根目录；权限要求见 `SECURITY_DESIGN.md` §13.2 |
| `daemon.listen` | string | `"127.0.0.1:8765"` | HTTP/WSS listener 地址；**Sync、Node Link 与两者的配对 HTTP 共用同一个 listener，按 path 路由**（`/sync/v1`、`/node-link/v1`、`/sync/v1/pairing/*`、`/node-link/v1/pairing/*`）。默认只监听 loopback；监听 `0.0.0.0`/`[::]` 必须显式配置并在启动输出中告警 |
| `daemon.public_origin` | string\\|null | `null` | canonical public origin（`scheme://host[:port]`）；配置了远程入口就必须给出，用于 Origin/Host 校验与配对二维码 |
| `daemon.allowed_hosts` | string[] | `[]` | 反向代理场景下允许的 `Host` 白名单；为空时只接受与 `public_origin` 一致的 Host。**`public_origin` 与 `allowed_hosts` 两者都为空**（默认配置：只监听 loopback）时只接受 loopback 形态的 `Host`（`127.0.0.0/8`、`[::1]`、`localhost`，忽略端口）——这是 `SECURITY_DESIGN.md` §7.2「限制 Host，拒绝任意 Host 转发和 DNS rebinding」在默认配置下的补全，判定与用例在 `crates/server/src/transport/net/host.rs` |
| `daemon.trusted_proxies` | string[] | `[]` | 允许终止 TLS 的同机代理地址；非空时才考虑 `Forwarded`/`X-Forwarded-*` |
| `daemon.instance_lock` | enum | `"file"` | 单实例锁实现：`file`\\|`ipc`；见 `SECURITY_DESIGN.md` §12.1。**当前切片只接线 `file`**（`fs4` 的 OS advisory 文件锁；两种取值的互斥语义相同），显式写 `ipc` 会被解析但在启动时以 `daemon.config_unwired`（debug 级）注明「已解析但不生效」，不静默换实现 |
| `daemon.shutdown_grace_ms` | integer | `10000` | 关闭时等待接入层停止、Agent 退出与存储刷新的上限 |
| `daemon.local_admin.endpoint` | string | `"auto"` | 本地管理通道 endpoint；`auto` 使用平台默认位置（Windows Named Pipe / Unix socket，见 [LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) §2.1）。显式值只用于测试或路径冲突排查，不改变“只允许同一 OS 用户”的授权模型。**当前切片只接受 `auto`**：显式值在配置解析阶段即以 `ConfigError` 失败关闭（`crates/app/src/config.rs`），待 `server::transport::local` 提供显式路径入口后再放开 |
| `daemon.tls.mode` | enum | `"proxy"` | `proxy` = TLS 由同机可信反向代理终止（配合 `daemon.trusted_proxies` 与 `daemon.public_origin`）；`direct` = Daemon 自己终止 TLS，此时 `cert_path`/`key_path` 必需 |
| `daemon.tls.cert_path` | path\|null | `null` | PEM 证书链；`mode = "direct"` 时必需，文件权限按 `SECURITY_DESIGN.md` §13.2 检查 |
| `daemon.tls.key_path` | path\|null | `null` | PEM 私钥；`mode = "direct"` 时必需，不得写入日志、错误信息或崩溃报告 |

三种部署形态下这三个键的填法（Node Link 的对端是**另一台机器**，因此它必须真的可达；`daemon.listen` 只监听 loopback 时 Node Link 只在同机测试下可用）：

| 形态 | `daemon.listen` | TLS | `public_origin` | 额外要求 |
|---|---|---|---|---|
| 同机反代（默认 `tls.mode = "proxy"`） | `127.0.0.1:8765` | 由反代终止 | `https://<对外 host>` 必填 | 反代必须**透传 WebSocket upgrade**，且不得只转发页面路径而与 `/sync/v1`、`/node-link/v1` 脱节；反代地址列入 `daemon.trusted_proxies` |
| Daemon 自己终止 TLS（`tls.mode = "direct"`） | 可为 `0.0.0.0:8765`（必须显式+启动告警） | `cert_path`/`key_path` 必需 | 必须与证书 host 一致 | 证书轮换不改变 Node identity（`SECURITY_DESIGN.md` §9.5） |
| 局域网 / Tailscale / WireGuard 直连 | 非 loopback 地址（同上告警） | 仍需 `direct` + 自备证书（如 `tailscale cert`） | 同上 | 不存在「明文 wss」；`dev_mode.allow_plaintext` 只允许 loopback（§10） |

- `public_origin` 是 Origin/Host 校验、PWA canonical origin 与配对 URL/endpoint 的**权威 host**；Node Link 不依赖 Origin（对端不是浏览器），但仍受 `Host`/`daemon.allowed_hosts` 与 `SECURITY_DESIGN.md` §7.1 的 TLS 边界约束。
- `public_origin` 的取值口径是 canonical origin（`https://<authority>`），与身份侧 `CanonicalOrigin::parse`（`crates/identity-auth/src/types.rs`，只认 `https://` 前缀）一致。**实现现状（已知偏差）**：`server::transport::net` 的 `HostPolicy` 只要求它能解析成 `http`/`https` 的 origin（`crates/server/src/transport/net/host.rs`），因此 `http://` 字面量能过 Host 边界、却会在配对 URL 派生处失败关闭；本交付版本未收紧这一点，收紧时两处必须同时改。

## 2. `sync`
只列出运行时可调项，具体默认值与上限语义以 `SYNC_PROTOCOL.md` §14 为准：

| 键 | 默认值 | 对应协议字段 |
|---|---:|---|
| `sync.max_message_bytes` | `1048576` | `auth.authenticated.limits.maxMessageBytes` |
| `sync.max_prompt_bytes` | `262144` | `auth.authenticated.limits.maxPromptBytes` |
| `sync.max_replay_events_per_batch` | `500` | `auth.authenticated.limits.maxReplayEventsPerBatch` |

上表三项是 v1 中唯一允许按部署下调的 Sync 限额；§14 的其余上限是固定常量，不得通过配置放宽或收窄。

**本交付版本未接线（2026-09-26）**：上表三个键与 §3 的 `node_link.*` 一样，当前只被解析（类型由 `serde` 判定）、不被消费——`server::sync` 尚未落地，因此显式配置它们**不生效**（被登记进 `Config::unwired` 并以 debug 级 `daemon.config_unwired` 逐键上报）；判定与理由见 §3 的注记。

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

**本交付版本未接线（2026-09-26，`node-link-owner` 切片 5）**：上表七个键能被解析（仅类型层面），但 `app` 的配置加载**不消费它们**——它们被登记进 `Config::unwired`，启动时以 debug 级 `daemon.config_unwired` 逐键上报，也就是说显式配置该批键在当前交付版本里**不生效**（`net_config` 取 `NetConfig::default()` 的值）。之所以不影响行为：`NetConfig::default()` 与 `server::node_link` 的 `NodeLinkConfig::default()` 都取本节/`NODE_LINK_PROTOCOL.md` §2.5 的默认值，因此「接线后按配置下调」与当前实现的下发值一致；接线（含非法与越界取值的拒绝）属后续切片。

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
| `storage.flush_interval_ms` | `250` | 同一会话短窗口内 delta 合并为一次 `commit` 的窗口（broker 的合并窗口，由组合根定时器枚举非终态会话并逐个驱动 `Broker::pump`；`CORE_PORTS_AND_STORAGE.md` §6 第 10 条；**不是**存储刷盘间隔） |
| `storage.attachment_max_file_bytes` | `20971520` | 单个附件上限（20 MiB） |
| `storage.attachment_max_total_bytes` | `1073741824` | 附件总量上限（1 GiB） |
| `storage.attachment_dir` | `<data_dir>/attachments` | 内容寻址的附件目录（`CORE_PORTS_AND_STORAGE.md` §7.1）；权限与数据库目录相同，见 `SECURITY_DESIGN.md` §13.2 |
| `storage.audit_retention_days` | `365` | 审计记录（`owned_audit`/`imported_audit`）保留期；容量清理时审计排在最后，超限优先拒绝写入而不是丢审计 |

Access Node 的 `no-content-cache` 不是配置项：它由 Export 固定，Access 侧没有可关闭的开关（`NODE_LINK_PROTOCOL.md` §10、`SECURITY_DESIGN.md` §13.4）。

### 5.1 备份与拷贝

SQLite 在 WAL 模式下把最近的写入放在主库文件旁边的 `-wal` 与 `-shm` 文件里，因此**只拷贝 `acp-remote.sqlite3` 而不带这两个文件，可能得到陈旧甚至损坏的副本**。备份时要么同时拷贝这三个文件，要么先做一次 checkpoint（Daemon 正常关闭时会执行 `wal_checkpoint(TRUNCATE)`，见 `CORE_PORTS_AND_STORAGE.md` §7.1）。附件目录（`storage.attachment_dir`）与数据库是同一份数据的两个部分，必须一起备份。

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

# 凭据到环境变量的显式绑定：值从平台 keystore 读取，永不落本文件（见下方"凭据注入"）。
# name 必须同时出现在 env_allowlist 里；provider_id 必须已由 local.provider.configure 建立。
[[agents.profiles.env]]
provider_id = "openai"
field = "api_key"
name = "OPENAI_API_KEY"

[[agents.profiles]]
agent_id = "omp"
command = "omp"
args = ["acp"]
```

- `agent_id` 是稳定标识，进入 `SessionSummary.agent.agentId`；启动 profile 与 capability 差异按 `MODULE_ARCHITECTURE.md` §9 只作为数据，不在核心堆积 Agent 名称判断。
- 环境变量白名单是上限而非提示：未列出的变量不得注入子进程（`SECURITY_DESIGN.md` §12.2）。
- **凭据注入**：`env` 绑定只说“哪个 Provider 字段写进哪个环境变量名”，**值**由平台 keystore 持有，启动子进程时经 `CredentialResolver` 解析（`CORE_PORTS_AND_STORAGE.md` §5.3）。未绑定、未列入 `env_allowlist` 或 keystore 不可用 → 启动失败关闭，不得静默跳过；Node/Device 私钥与 pairing secret 永不注入。
- workspace、Provider/MCP 凭据值不在本文件：它们由本地管理入口（`local.workspace.select`、`local.provider.configure`）在 Daemon 内维护。
- Agent 进程的启动/短请求/关闭超时、单条 ACP 消息上限、stderr 缓冲上限、空闲回收规则都是**固定 v1 常量**（不是本文件的配置键）：见 `SECURITY_DESIGN.md` §12.2 的表；只有会话空闲回收复用本文档 §4 的 `sessions.idle_timeout_ms`。

## 8. `identity`

| 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `identity.keystore` | enum | `"platform"` | `platform` 使用平台安全存储：Windows 当前档位是 DPAPI（当前用户 scope）包裹私钥、签名在进程内完成，macOS 是 Keychain，Linux 是 Secret Service；不可导出的硬件保护档位（CNG/TPM）需单独 ADR（`SECURITY_DESIGN.md` §9.2/§20）。`ephemeral` 仅允许显式开发模式 |
| `identity.fail_closed_on_missing_keystore` | boolean | `true` | 平台 keystore 不可用时正式模式拒绝启动；`false` 只能出现在开发模式配置中 |

## 9. `imports` / `exports`

Access Node 侧每个 Import 一条记录（由 `acp-remote import add` 请求 Daemon 写入 SQLite，不由用户手改）。下例仅为管理记录的 TOML 表示，不是启动配置输入：

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

### `exports`（Owner 侧）

Owner Node 的 Export 由本地管理入口写入 SQLite（`local.export.manage`，见 [LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) §5），同样不由用户手改。下例仅为管理记录的 TOML 表示：

```toml
[[exports]]
export_id = "exp-work"
display_name = "公司 Agent"
agent_ids = ["codex"]
default_workspace_alias = "company-agent-default"
default_template_id = "default"
scopes = ["grant.observe", "grant.interact", "grant.remote-work"]
cache_policy = "no-content-cache"

[[exports.workspace_aliases]]
alias = "company-agent-default"
display_name = "公司默认工作区"

[[exports.templates]]
template_id = "default"
display_name = "默认"
workspace_alias = "company-agent-default"
# 首切片 template 必须无参数（NODE_LINK_PROTOCOL.md §10）。下面这段 params 是 post_mvp 的
# 说明性表示，不是首切片的合法输入；有参 template 需要先定「谁提供值」与「影响什么」。
# [[exports.templates.params]]
# name = "branch"
# type = "string"
# required = false
# pattern = "^[a-zA-Z0-9._/-]{1,128}$"
# enum = []
```

- `cache_policy` 在 v1 固定为 `no-content-cache`；管理写入拒绝其他值，启动加载发现非法持久值则失败关闭，不允许绕过 Owner 的内容策略。
- `export_id`、`agent_ids`、`workspace_aliases`、`templates` 与 `scopes` 一起构成 `session.create` 参数（`agentId`/`exportId`/`workspaceAlias`/`templateParams`）的唯一可引用集合；未列出的取值一律拒绝（`nodelink.export.not_granted` 或 `nodelink.command.unsupported_field`）。`scopes` 与 Node Link Export 模型的同名字段同义（`NODE_LINK_PROTOCOL.md` §10）；Access 侧 `[[imports]].grants` 是另一概念（本节点自己的授权子集），不随此改名。
- `default_workspace_alias` 必须出现在同一条目的 `workspace_aliases` 中，`default_template_id` 必须出现在 `templates` 中；首切片恰好一个 workspace alias 与一个 template，且该 template **不得声明参数**（`NODE_LINK_PROTOCOL.md` §10）。
- `[[exports.workspace_aliases]].alias` 就是本机 `owned_workspace.alias`（同一命名空间）：`export.create` 必须校验每个 alias 已在 `local.workspace.select` 建立，Export 只引用符号名、**不复制路径**（`SECURITY_DESIGN.md` §12.3）。
- `templates.params[].type` ∈ `string|boolean|integer`；`pattern` 只对 `string` 生效；`enum` 为空数组表示不限制取值。`session.create.payload.templateParams` 的键必须来自这里。
- 原始 workspace 路径与 Provider/MCP 凭据不出现在本文件：它们由 `local.workspace.select`、`local.provider.configure` 在 Daemon 内维护（`SECURITY_DESIGN.md` §12.3）。

## 10. `dev_mode`

| 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `dev_mode.enabled` | boolean | `false` | 发布构建默认关闭；启用时启动输出与 UI 必须持续显示非安全状态 |
| `dev_mode.allow_plaintext` | boolean | `false` | 允许明文 HTTP/WS，且只允许 loopback |
| `dev_mode.ephemeral_identity` | boolean | `false` | 使用进程期临时身份，不复用正式设备/节点数据库 |

## 11. `logging`

| 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `logging.level` | enum | `"info"` | `trace`\|`debug`\|`info`\|`warn`\|`error` |
| `logging.format` | enum | `"text"` | `text`\|`json`；允许字段与脱敏要求见 `SECURITY_DESIGN.md` §14.1，`json` 不改变允许字段集合 |
| `logging.file` | path\|null | `null` | `null` 表示输出到 stderr；写入文件时权限按 `SECURITY_DESIGN.md` §13.2，且不得包含密钥、token 或完整 prompt |
