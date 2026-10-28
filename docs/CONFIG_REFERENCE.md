# ACP Remote 配置参考

> 状态：编码前基线
> 版本：0.15
> 日期：2026-10-22
> 修订记录（2026-10-22，0.15，`session-activation-lifecycle` WP1，**合同与实现（WP4 已接线）**）：§4 的 `sessions.idle_timeout_ms` 缺省值由 `0` 改为 **`1800000`（30 分钟）**，并新增 `sessions.max_active_sessions`（正整数，候选默认 **50**，无「无限」取值）。显式 `0` 从此只关闭**按时间**回收，不关闭容量保护；满额时优先回收无人查看、无任务/交互的最久未使用会话，没有安全候选时以 `session.capacity_exceeded` 拒绝新激活。**50 是待实测候选值**（10/25/50 档验收见 `docs/SYNC_PROTOCOL.md` §14 与变更 plan 的 PV6），测试前不得声称它是已验证的安全上限。落地前 `crates/app/src/config.rs` 与 `config.example.toml` 仍按旧默认值 `0` 解析，见 §4 的实现状态注记。
> 修订记录（2026-10-18，0.14）：§1 的 `daemon.listen` 拆分为 `daemon.port` 与可选的 `daemon.bind`（单个 IP）；默认绑定随 TLS 模式分叉（`proxy` → loopback，`direct` → `0.0.0.0` + 尽力 `[::]`），`proxy` 模式禁止非 loopback 绑定；`daemon.public_origin` 接受 `host[:port]` 并归一化为 `https://...`，按“绑定非 loopback 且无 Host 判定来源”失败关闭；新增只读命令 `acp-remote config origin`。旧键 `daemon.listen` 会被明确拒绝。`terminal.*` 已在 0.13 删除；`sessions.*` 已接线；`node_link.*` 仍只解析不生效。
> 修订记录（2026-10-18，0.13）：删除 `terminal.*` 配置段（§6）——仓库内没有任何消费者，终端输出截断上限不是可配置项；写 `[terminal]` 现在会按未知键启动失败。`daemon.instance_lock = "ipc"` 从「解析但不生效」改为**启动失败**，只接受 `file`。§4 的 `sessions.*` 三个键**已接线**（`queue_policy`/`max_queued_turns` 进 broker，`idle_timeout_ms` 进 `agent-host` 的空闲回收）。§3 的 `node_link.*` 仍是「只解析不生效」，留待后续统一处理。其余键名、类型与默认值未变。
> 修订记录（2026-10-16，0.12，`pairing-model`）：§9 的 Import 说明去掉「本地客户端能做什么由设备 scope 决定」这一口径（设备权限层已删除：已认证设备即可执行本节点命令，设备方向没有授权维度），远程侧仍由 Import 的 `grants` 与 Owner 的 Export 决定。**配置键名、类型与默认值均未变**（仍以本节为唯一权威）；限流与协议上限仍是固定 v1 常量。
> 修订记录（2026-09-18）：补充 `daemon.tls.*`、`daemon.local_admin.endpoint`、`logging.*` 与 Owner 侧 `[[exports]]`（原先只有 Access 侧 `[[imports]]`）；明确限流与协议上限是固定 v1 常量而非配置键。
> 修订记录（2026-09-18，0.3）：`[[exports]].grants` 更名为 `scopes`，与 Node Link Export 模型及本地通道 `ExportView` 同名同义（Access 侧 `[[imports]].grants` 是另一概念，不变）。
> 修订记录（2026-09-18，0.4）：新增 `storage.audit_retention_days`（默认 365），审计清理排在容量顺序最后。
> 修订记录（2026-09-18，0.5）：补齐 `storage.attachment_dir`——`CORE_PORTS_AND_STORAGE.md` §7.1 新增的附件目录配置键，此前只在合同里定义、未落到本表；§5.1 增加「备份与拷贝」说明（WAL 模式下不能只拷主库文件）。
> 修订记录（2026-09-23，0.6）：核对「配置与管理状态的权威」与 seed 语义——与 `CORE_PORTS_AND_STORAGE.md` §5.3 的 `LocalConfigStore`/`SeedWrite` 一致（首次初始化单事务导入全部合法 profile、空列表也标记完成、初始化完成后数据库是唯一权威、重启不重导、旧配置不得复活已撤销 Export 或已删除 Import），本轮**不改语义**；凭据注入小节的引用由 §11.6 改为 §5.3，管理状态的持久化边界与升级规则改指 §7.2/§7.3/§7.4 与 §11。
> 修订记录（2026-09-23，0.7）：管理 store 的落盘实现已落地（`crates/storage-sqlite/src/admin/`），标题与「待实现」标记随之去掉；配置键名、默认值与 seed 语义**未变**（仍以本节为唯一权威）。
> 修订记录（2026-09-26，0.8）：按 `node-link-owner`（切片 5）的实绩写回三处——§1 的 `daemon.allowed_hosts` 行补「`public_origin` 与 `allowed_hosts` 都为空（默认配置）时只接受 loopback 形态 `Host`」这一分支（原先只写了「为空时按 `public_origin` 推导」）并在 §1 末尾补 `public_origin` 的 canonical origin 口径与实现现状；§2/§3 加「本交付版本只解析、不消费」注记（`sync.*` 与 `node_link.*` 均被登记为未接线）。键名、类型、默认值**未变**（仍以本节为唯一权威）。
> 修订记录（2026-10-13，0.9）：§2 撤下「三个 `sync.*` 键只解析、不消费」的注记——它们已接线，并写明「只下调」的生效路径与不再进 `Config::unwired`；§2 补记配对 HTTP 的请求体上限固定 16 KiB（非配置键，两面共用）。键名、类型与默认值**未变**（仍以本节为唯一权威）。
> 修订记录（2026-10-14，0.10，`sync-server-connection` WP7）：§2 的「已接线」注记补上代码落点（`lower_of`/`sync_config`/`SyncConfig`/`SessionLimits::negotiate` 与「下调生效、上调被忽略」的自检用例），并如实登记 wire 下限造成的「过小值被协议层夹取而非失败关闭」缺口。键名、类型、默认值与「只下调」口径**均未变**（仍以本节为唯一权威）。
> 修订记录（2026-10-15，0.11，`sync-ui-hosting` WP4）：§1 的 `daemon.listen` 行补上**根目录静态托管**（同一个 listener 同源托管前端静态资源）与它与协议 path 的**优先级关系**（协议 path 先命中；静态面只接住未命中任何协议端点的请求；协议族保留前缀下的未注册子路径仍 404，不被静态面接管）。键名、类型与默认值**未变**（仍以本节为唯一权威）。
> 上位文档：[INITIAL_DESIGN.md](./INITIAL_DESIGN.md)

本文是 Daemon 运行时配置键名、类型、默认值与可否调整的**唯一权威来源**。协议层限额不在此重复定义：

- Sync 的限额与其中哪些可经 `auth.authenticated.limits` 下发，见 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §14。
- Node Link 的限额与其中哪些可经 `node.ready.limits` 下调，见 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §2.5。
- 限流与其余协议上限（认证尝试速率、命令速率、配对有效期、单消息与嵌套上限等）都是**固定 v1 常量**，不是配置键；本文件不提供放宽它们的开关。
- 安全默认值（监听地址、TLS 终止、失败关闭）的约束见 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §7、§8、§9。

启动配置来源与优先级（高者覆盖低者）：命令行参数 > 配置文件 > 内置默认值。环境变量只用于替换配置文件路径（`ACP_REMOTE_CONFIG`），不承载业务配置，避免把凭据写进进程环境。此优先级不适用于下面的 Daemon 管理状态。

仓库根目录的 [`config.example.toml`](../config.example.toml) 是逐键模板：默认值全部可运行，`node_link.*` 明确标注为“暂不生效”。`crates/app/src/config.rs` 的单元测试会解析这份文件并断言默认值，模板与解析器不会静默漂移。

### 配置与管理状态的权威（2026-09-23）

| 数据 | 唯一持久化权威 | 写入与生效时机 |
|---|---|---|
| daemon / sync / node_link / sessions / storage / identity / dev_mode / logging 启动参数 | 用户配置文件，经 CLI 参数覆盖 | 启动时读取并校验；v1 不提供文件监听或通用热重载，修改后重启生效 |
| Agent profile | 用户配置文件 `[agent]` | 每次启动读取并校验；改配置后重启生效 |
| 目录标识 → 本机路径 | SQLite 管理记录 | 由成功的会话创建自动记录；已有会话不迁移目录 |
| 设备/节点信任 | SQLite 管理记录 | 本地管理操作提交后生效；撤销按安全协议立即阻断相应访问 |
| Node 私钥 | 平台 keystore | 身份密钥；Provider/MCP 凭据由 Agent 自己持有，不进入本系统 |

- SQLite 的管理状态由运行中的 Daemon 单一写入；CLI 经本地 IPC 请求修改，不自行写库、不回写 TOML。持久化边界与升级规则见 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §7.2/§7.3/§7.4 与 §11。
- §7 的 `[agent]` 是 Agent 列表的唯一权威：每次启动读取，非法条目让启动失败关闭；旧库中的 profile 不会自动搬运，需在配置中重新声明。
- §9 说明 `imports`/`exports` 不是配置输入；启动配置中出现它们必须明确拒绝。这样旧配置无法在重启时复活已撤销的信任或已删除的绑定。
- workspace 路径、信任与授权集合不接受启动参数覆盖；keystore 不可用或 SQLite 写入失败时管理方法返回明确失败，不能仅在内存修改后报告成功。
重建 Agent catalog 与节点信任路由；撤销状态始终优先。

## 1. `daemon`

| 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `daemon.data_dir` | path | 平台用户配置目录下的 `acp-remote/` | SQLite、附件、日志的根目录；权限要求见 `SECURITY_DESIGN.md` §13.2 |
| `daemon.port` | integer | `8765` | 共享 HTTP/WSS listener 的**绑定端口**；**Sync、Node Link 与两者的配对 HTTP 共用同一个 listener，按 path 路由**（`/sync/v1`、`/node-link/v1`、`/sync/v1/pairing/*`、`/node-link/v1/pairing/*`），并在**根路径 `/`** 上以同一个 listener **同源托管前端静态资源**（PWA 入口文档与构建产物，见 `FRONTEND_DESIGN.md` §4.3）。**与协议 path 的优先级**：已注册的协议 path 先命中（axum 精确路由，静态面只是 router 的 fallback），静态面只接住**未命中任何协议端点**的请求；协议族**保留前缀**之下的未注册子路径仍按协议面语义 404，**不被静态面接管**（保留前缀由已注册协议 path 的**前两段**派生、按段匹配，因此 `/sync/v1extra` 不属于 `/sync/v1`）。`0` 表示临时端口，**只允许** `dev_mode.enabled = true` 且绑定 loopback（测试与本地调试） |
| `daemon.bind` | string\\|null | `null` | 显式绑定地址（**单个 IP 字面量**，不是白名单）；`null` 时按 TLS 模式取默认：`proxy` → `127.0.0.1`，`direct` → `0.0.0.0` 并尽力补绑 `[::]`。显式值只绑定该地址（`bind = "::"` 表示只监听 IPv6）。**`proxy` 模式只允许 loopback**：非 loopback 绑定在配置解析阶段失败关闭（明文面不得超出 loopback） |
| `daemon.public_origin` | string\\|null | `null` | 对外身份（canonical public origin）。可写 `host[:port]` 或完整 `https://host[:port]`；裸值自动补 `https://`，省略端口按 443。用于 Origin/Host 校验、配对 URL 与设备身份绑定。**按需失败**：绑定非 loopback 且它与 `allowed_hosts` 都为空时启动失败；只有 `allowed_hosts` 时启动告警，配对/Node Link 在使用点报错并提示配置它 |
| `daemon.allowed_hosts` | string[] | `[]` | 反向代理场景下允许的 `Host` 白名单；为空时只接受与 `public_origin` 一致的 Host。**`public_origin` 与 `allowed_hosts` 两者都为空**且只监听 loopback 时只接受 loopback 形态的 `Host`（`127.0.0.0/8`、`[::1]`、`localhost`，忽略端口）——这是 `SECURITY_DESIGN.md` §7.2「限制 Host，拒绝任意 Host 转发和 DNS rebinding」在默认配置下的补全，判定与用例在 `crates/server/src/transport/net/host.rs` |
| `daemon.trusted_proxies` | string[] | `[]` | 允许终止 TLS 的同机代理地址；非空时才考虑 `Forwarded`/`X-Forwarded-*` |
| `daemon.instance_lock` | enum | `"file"` | 单实例锁实现，**只接受 `file`**（`fs4` 的 OS advisory 文件锁）；其它取值（含旧版文档里的 `ipc`）在配置解析阶段即以 `ConfigError` 失败关闭，不静默换实现。见 `SECURITY_DESIGN.md` §12.1 |
| `daemon.shutdown_grace_ms` | integer | `10000` | 关闭时等待接入层停止、Agent 退出与存储刷新的上限 |
| `daemon.local_admin.endpoint` | string | `"auto"` | 本地管理通道 endpoint；`auto` 使用平台默认位置（Windows Named Pipe / Unix socket，见 [LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) §2.1）。显式值只用于测试或路径冲突排查，不改变“只允许同一 OS 用户”的授权模型。**当前切片只接受 `auto`**：显式值在配置解析阶段即以 `ConfigError` 失败关闭（`crates/app/src/config.rs`），待 `server::transport::local` 提供显式路径入口后再放开 |
| `daemon.tls.mode` | enum | `"proxy"` | `proxy` = TLS 由同机可信反向代理终止（配合 `daemon.trusted_proxies` 与 `daemon.public_origin`）；`direct` = Daemon 自己终止 TLS，此时 `cert_path`/`key_path` 必需 |
| `daemon.tls.cert_path` | path\|null | `null` | PEM 证书链；`mode = "direct"` 时必需，文件权限按 `SECURITY_DESIGN.md` §13.2 检查 |
| `daemon.tls.key_path` | path\|null | `null` | PEM 私钥；`mode = "direct"` 时必需，不得写入日志、错误信息或崩溃报告 |

三种部署形态下绑定与对外身份的填法（Node Link 的对端是**另一台机器**，因此它必须真的可达；只绑定 loopback 时 Node Link 只在同机测试下可用）：

| 形态 | `daemon.port` / `daemon.bind` | TLS | `public_origin` | 额外要求 |
|---|---|---|---|---|
| 同机反代（默认 `tls.mode = "proxy"`） | `8765` / 默认 `127.0.0.1` | 由反代终止 | `https://<对外 host>` 或 `<对外 host>` 必填 | 反代必须**透传 WebSocket upgrade**，且不得只转发页面路径而与 `/sync/v1`、`/node-link/v1` 脱节；反代地址列入 `daemon.trusted_proxies` |
| Daemon 自己终止 TLS（`tls.mode = "direct"`） | `8765` / 默认 `0.0.0.0` + `[::]`（启动告警） | `cert_path`/`key_path` 必需 | 必须与证书 host 一致；direct 模式端口不是 443 时写 `host:port` | 证书轮换不改变 Node identity（`SECURITY_DESIGN.md` §9.5） |
| 局域网 / Tailscale / WireGuard 直连 | 同上（或显式某个非 loopback IP） | 仍需 `direct` + 自备证书（如 `tailscale cert`） | 同上 | 不存在「明文 wss」；`dev_mode.allow_plaintext` 只允许 loopback（§10） |

- `daemon.port` 是**绑定端口**；`public_origin` 里的端口是**对外端口**。反代模式下两者通常不同（8765 vs 443），不要互相推导。
- `public_origin` 的输入是 `host[:port]`（如 `work-pc.tail.ts.net`、`100.101.102.103:8765`），也可以继续写完整 `https://...`；`http://` 只允许开发明文模式，且**只用于本机 Host 判定**——配对与设备身份仍要求 `https://`（身份侧 canonical origin 只接受该前缀）。IPv6 用方括号（`[fd7a:...]:8765`）。
- `public_origin` 是 Origin/Host 校验、PWA canonical origin 与配对 URL/endpoint 的**权威 host**；Node Link 不依赖 Origin（对端不是浏览器），但仍受 `Host`/`daemon.allowed_hosts` 与 `SECURITY_DESIGN.md` §7.1 的 TLS 边界约束。
- `public_origin` 归一化后的取值口径仍是 canonical origin（`https://<authority>`），与身份侧 `CanonicalOrigin::parse`（`crates/identity-auth/src/types.rs`，只认 `https://` 前缀）一致。**实现现状（已知偏差）**：`server::transport::net` 的 `HostPolicy` 只要求它能解析成 `http`/`https` 的 origin（`crates/server/src/transport/net/host.rs`），因此 `http://` 字面量能过 Host 边界、却会在配对 URL 派生处失败关闭；本交付版本未收紧这一点，收紧时两处必须同时改。
- **旧键迁移**：`daemon.listen` 已拆分并会在配置解析阶段被明确拒绝；请改用 `daemon.port` 与可选的 `daemon.bind`。`acp-remote config origin` 可检测本机地址并打印建议值（只读，不写配置文件）。

## 2. `sync`
只列出运行时可调项，具体默认值与上限语义以 `SYNC_PROTOCOL.md` §14 为准：

| 键 | 默认值 | 对应协议字段 |
|---|---:|---|
| `sync.max_message_bytes` | `1048576` | `auth.authenticated.limits.maxMessageBytes` |
| `sync.max_prompt_bytes` | `262144` | `auth.authenticated.limits.maxPromptBytes` |
| `sync.max_replay_events_per_batch` | `500` | `auth.authenticated.limits.maxReplayEventsPerBatch` |

上表三项是 v1 中唯一允许按部署下调的 Sync 限额；§14 的其余上限是固定常量，不得通过配置放宽或收窄。

**已接线（`sync-server-connection` 任务 6.3）**：三个键从「只解析」变为**真正消费**，取值口径是「只下调」——配置值高于上表默认值时**忽略配置、取默认值**（默认值同时是上界，来自 `SYNC_PROTOCOL.md` §14），低于默认值时生效并把该值下发到 `auth.authenticated.limits`。生效路径：`app::config` 在解析期收敛（`crates/app/src/config.rs` 的 `lower_of`，三项各与协议默认值取小）→ `daemon::sync_config` 映射（`crates/app/src/daemon.rs`，`sync.*` 的唯一消费点）→ `server::sync::conn::SyncConfig` → 协议层 `SessionLimits::negotiate`（`crates/server/src/sync/conn/limits.rs`）再按 `[wire 下限, §14 默认值]` 夹一次。因此这三个键不再出现在 `Config::unwired`（启动日志不再逐键报 `daemon.config_unwired`；`crates/app/src/config.rs` 的自检用例同时断言「下调生效」与「上调被忽略」两条都走到协议层协商结果上）。

- 低于协议默认值是合法用法（收紧本机限额）；再往下的 wire 下限由协议层兜住，不会下发对端无法接受的 `limits`。**已知缺口（已登记，未修）**：wire 下限（1024 / 1 / 1）是协议层的私有常量，组合根取不到，因此过小的值由协议层夹到下限而不是失败关闭式拒绝（`tasks.md` WP6 任务 6.3 记下的二选一补救）。
- **配对 HTTP 的请求体上限固定 16 KiB**（`POST /sync/v1/pairing/claim`、`POST /sync/v1/pairing/status`；`SYNC_PROTOCOL.md` §14 冻结的数值）：它不是配置键，也不能按部署放宽；超限由接入层在调用处理器前返回 413，且该响应同样带 `SYNC_PROTOCOL.md` §7.1 的四个安全头（实现见 `crates/server/src/sync/pairing.rs` 的 `SECURITY_HEADERS`/`default_response_headers`，并经 `crates/app/src/daemon.rs` 的 `register_post` 传给接入层）。接入层的请求体上限是**每 listener 一个值**（`crates/server/src/transport/net/mod.rs` 的 `DEFAULT_MAX_BODY_BYTES`），Node Link 的配对端点共用同一个 16 KiB（`NODE_LINK_PROTOCOL.md` §13.4 只要求「有上限」，未冻结数值，因此对两面都合规）。

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

### 3.1 `[node_link.access]`（Access 侧出站）

本机作为 **Access Node** 导入其他 Owner 时使用；只承载出站 TLS 的信任来源，不改变任何协议限额。

| 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `node_link.access.trust_roots` | string[] | `[]` | 追加到**系统根存储**的固定 CA（PEM 文件绝对路径）；空数组 = 只信任系统根 |

- 信任集合 = 操作系统根存储 ∪ 本表列出的证书。列出的文件**不会**替换系统根，也无法关闭系统根。
- 每条必须是非空绝对路径；相对路径会随工作目录漂移，解析期即拒绝。文件存在性与 PEM 内容在启动时校验：任一路径不可读或不含证书 → `daemon start` 失败关闭（`daemon.trust_*` 日志只记数量与文件名，不打印路径内容）。
- 系统根与自定义 CA 都为空时同样失败关闭：不存在「空锚」或「跳过校验」的运行形态。
- 生效路径：`app::config`（本键）→ `app::trust::OutboundTrust::load`（系统根 + PEM）→ `node-link-client` 的 `TlsTrust::Roots`（配对 claim/status 与 WSS 业务/附件连接共用同一份构造）。信任锚只在启动时加载，修改后需重启 Daemon。
- **Owner 端点只允许 `wss://`**：持久化的 Owner 信任记录（core 的 `NodeRecord`）与配对 URL 的 `endpoint` 都只接受 `wss://`，因此出站 Owner 链路不存在明文运行形态。`node_link-client` 仍支持 `TlsTrust::PlaintextDev` 供独立传输测试使用，但 `AccessManager` 对 `http://`/`ws://` Owner 端点显式拒绝（`TrustError::PlaintextNotSupported`）；`dev_mode.enabled && dev_mode.allow_plaintext` 只服务入站监听与配对请求的 dev 形态，不放宽这里的出站链路。
- `daemon.tls.*` 是**入站**（本机作为 Owner 终止 TLS）的设置，两者互不影响，也不互相回退。

**实现现状（2026-10-20，`conversation-area-complete` WP5）**：本键已接线；`node pair --mode access` 的 claim/status 与启动时的 Owner 连接实例化都使用这里加载的信任锚。出站传输**按每条 Owner 端点**构造（`node-link-client` 的 `TransportFactory`）：`wss://` 用 `TlsTrust::Roots`，明文 scheme 在进入 `node-link-client` 之前就被拒绝；`ClientConfig.max_message_bytes` 随同一份配置进入该传输的帧/消息上限，不再是构造后被丢弃的字段。

**本交付版本未接线（2026-09-26，`node-link-owner` 切片 5）**：上表七个键能被解析（仅类型层面），但 `app` 的配置加载**不消费它们**——它们被登记进 `Config::unwired`，启动时以 debug 级 `daemon.config_unwired` 逐键上报，也就是说显式配置该批键在当前交付版本里**不生效**（`net_config` 取 `NetConfig::default()` 的值）。之所以不影响行为：`NetConfig::default()` 与 `server::node_link` 的 `NodeLinkConfig::default()` 都取本节/`NODE_LINK_PROTOCOL.md` §2.5 的默认值，因此「接线后按配置下调」与当前实现的下发值一致；接线（含非法与越界取值的拒绝）属后续切片。

## 4. `sessions`

| 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `sessions.queue_policy` | enum | `"queue"` | 同一会话已有 active turn 时的新 prompt 处理：`queue` 按持久化接受顺序排队；`reject_busy` 直接返回 `session.busy`。v1 默认排队，与 `SYNC_PROTOCOL.md` §11.6 的二选一一致 |
| `sessions.max_queued_turns` | integer | `16` | `queue` 策略下的排队上限，超出返回 `session.busy` |
| `sessions.idle_timeout_ms` | integer | `1800000` | 无人查看、无任务/交互且资源已释放确认后开始计时的保留期（30 分钟）。显式 `0` **只**关闭按时间回收，**不**关闭容量保护；非零时只影响 Daemon 侧运行资源，不影响事件、历史、游标与命令终态语义 |
| `sessions.max_active_sessions` | integer | `50` | 本机所有 Agent 合计可同时占用运行资源的 owned 会话数上限（正整数，**没有**表示无限的取值）。`ready`/激活中/回收中，以及仍占真实运行资源的会话都计入；待处理会话计入，历史会话不计入。满额时优先回收无人查看、无任务/交互的最久未使用会话（同时间以稳定会话标识为平局规则），没有安全候选时以 `session.capacity_exceeded` 拒绝新激活 |

四个键的目标语义如上，且均已接线（WP4）：`queue_policy`/`max_queued_turns` 注入 broker；`idle_timeout_ms`（缺省 1800000，显式 `0` 只关闭按时间回收）与 `max_active_sessions`（正整数，缺省候选 50）注入 `LocalSessionLifecycle`，非法值在配置解析阶段启动失败。`max_active_sessions` 的 50 仍是**待实测候选值**（PV6 的 10/25/50 档），不得声称已验证安全。

- **保留期起点**：最后一个保护条件（查看者、未结束/排队任务、有效待处理交互、停止未确认的执行）消失时开始计时；任一条件重新出现即取消原计时，全部解除后从头计时。页面阅读、网络心跳、配置查询与快照拉取都不刷新它。
- **容量回收可早于保留期**：满额且存在安全候选时提前回收最久未使用者；`idle_timeout_ms = 0` 不改变这条策略。
- **50 是候选值**：交付前必须测量 10/25/50 个恢复会话的内存、切换耗时与回收效果（真实硬件/Agent 版本/上下文规模），实测不支持时携证据重新确认；在此之前不得把它写成已验证的安全上限，也不得把「会话数」当成内存硬上限宣传。
- **非法取值失败关闭**：`max_active_sessions` 必须是正整数（`0`、负数、非整数一律在配置解析阶段拒绝，不退化成「无限」）；`queue_policy` 的未知取值与超过 `u32` 的排队上限同样失败关闭。
- **控制帧与保留期不新增配置键**：查看租约 30 秒、可见页面续租间隔 10 秒与查看控制帧速率 120 帧/分钟都是固定 v1 常量（[`SYNC_PROTOCOL.md`](./SYNC_PROTOCOL.md) §14 的预算表），不得由部署配置下调或放大。容量只有 `sessions.max_active_sessions` 一个键，不存在准备槽位或第二个「准备数上限」；`sessions.idle_timeout_ms = 0` 只关闭按时间回收，不影响容量保护或查看租约的到期清理。

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

它固定由 Owner 的 `no-content-cache` 策略决定

### 5.1 备份与拷贝

SQLite 在 WAL 模式下把最近的写入放在主库文件旁边的 `-wal` 与 `-shm` 文件里，因此**只拷贝 `acp-remote.sqlite3` 而不带这两个文件，可能得到陈旧甚至损坏的副本**。备份时要么同时拷贝这三个文件，要么先做一次 checkpoint（Daemon 正常关闭时会执行 `wal_checkpoint(TRUNCATE)`，见 `CORE_PORTS_AND_STORAGE.md` §7.1）。附件目录（`storage.attachment_dir`）与数据库是同一份数据的两个部分，必须一起备份。

## 6. 终端输出上限（不是配置键）

v1 不提供终端输出截断的实现（ACP 终端类方法仍是 `post_mvp`，见 `ACP_COMPATIBILITY_MATRIX.md`），因此**不存在 `terminal.*` 配置段**；配置里写 `[terminal]` 会按未知键让 `daemon start` 失败关闭。未来实现时若需要按部署调整，必须先在本文件登记键名、类型、默认值与失败关闭规则。

## 7. `[agent]`（Agent 声明）

```toml
[node]
name = "家里的台式机"          # 可选：本机节点展示名（1..=128 字符，不进请求体）

[agent]
default = "omp"                # 可选；缺省则没有默认 Agent
omp   = ["C:/tools/omp.exe", "acp"]   # 值 = 单条命令字符串，或完整 argv 数组
codex = "C:/tools/codex-acp.exe"
```

- `[agent]` 每次启动读取，是 Agent 列表的**唯一权威**；id 就是键名，进入 `SessionSummary.agent.agentId`。
- 值是命令字符串或非空 argv 数组（首项为可执行文件）。空命令、argv 含空串、非法 id、`default` 指向未声明项都会让 `daemon start` 失败关闭，错误只指出键名与规则，不回显命令原文。
- 子进程环境在 `env_clear()` 后只注入固定的一组必要变量：`PATH`、`HOME`、`USERPROFILE`、`SystemRoot`、`TEMP`、`TMP`（宿主缺失时跳过）；不再有 `env_allowlist` 或 Provider 凭据绑定。`ACPR_*` / `ACP_REMOTE_*` 前缀永不注入。
- Provider/MCP 凭据由 Agent 自己持有；本系统不保存、不注入、不代管。Node 私钥与 pairing secret 也不进入子进程。
- **升级说明（v10，不可逆）**：升级会删除 `owned_agent_profile` 与 `owned_provider_ref`；Agent 不会从旧库自动搬运。若配置没有 `[agent]` 且旧库存在 profile，启动日志记录 `daemon.agents_discarded` 与条数，并提示“请在配置的 `[agent]` 中重新声明”。
- Agent 进程的启动/短请求/关闭超时、单条 ACP 消息上限、stderr 缓冲上限、空闲回收规则都是**固定 v1 常量**（不是本文件的配置键）：见 `SECURITY_DESIGN.md` §12.2 的表；只有会话空闲回收复用本文档 §4 的 `sessions.idle_timeout_ms`，节点会话容量上限复用 §4 的 `sessions.max_active_sessions`。

## 8. `identity`

| 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `identity.keystore` | enum | `"platform"` | `platform` 使用平台安全存储：Windows 当前档位是 DPAPI（当前用户 scope）包裹私钥、签名在进程内完成，macOS 是 Keychain，Linux 是 Secret Service；不可导出的硬件保护档位（CNG/TPM）需单独 ADR（`SECURITY_DESIGN.md` §9.2/§20）。`ephemeral` 仅允许显式开发模式 |
| `identity.fail_closed_on_missing_keystore` | boolean | `true` | 平台 keystore 不可用时正式模式拒绝启动；`false` 只能出现在开发模式配置中 |

## 9. 节点信任（无 `imports` / `exports`）

配对成功后节点信任记录即表达「该节点可使用本机全部资源」，不存在 `[[imports]]`/`[[exports]]` 配置或等价的管理记录。启动配置里出现这两个表必须明确拒绝。

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
