# ACP Remote 本地管理通道

> 状态：编码前契约；切片 4（`daemon-cli-and-local-admin`）的**本地通道部分已落地**（`server::transport::local` + `server::local_admin` + `app`），`server::acp_facade` 待切片 6——实现期差异只允许出现在 §3.1 末尾的实现状态注记里，不改本契约的任何语义  
> 版本：1.5（2026-10-15，`sync-ui-hosting` WP4：`daemon.status` 的 `result` 增加 `uiBuildVersion`（`string | null`，`null` = 版本缺失），取自本次二进制嵌入并托管的前端资源；按 §8 属**兼容变更**（新增 `result` 字段），方法集与 §6 的本地错误码集合**不变**）  
> 版本：1.4（2026-09-27，v1 内合同修订，未实现未发布：`node.pair.confirm` 的 `params` 增加**必填** `exportIds`（可为空数组）、`result` 回显它，`NodeRecord`（因而 `node.list`）增加 `exportIds`；按 §8 属不兼容变更，因 v1 未发布、CLI 与 Daemon 同版本发布，按 v1 内合同修订处理，先例为 `node.challenge.catalogRevision`；并同步「改清单 = 重新配对（落定后关闭活动连接，收窄自下一次握手生效）」）  
> 版本：1.3（2026-09-25：方法名允许段内连字符；`node.rotate-key.begin` 进入 v1 方法词表，修正 §5.7 与机器词表的不一致）  
> 版本：1.2（2026-09-25：新增 §3.1 实现状态注记——`server::acp_facade` 落地前，Daemon 对 `0x02` 连接在 framing 校验后即连即关；`daemon-cli-and-local-admin` 变更的 design.md 决策 5）  
> 版本：1.1（2026-09-23：新增 §3.1 `0x02` ACP 流的会话生命周期；管理载荷的 envelope 与错误码改为机器表达，目录见 [`schemas/local-admin/v1/`](../schemas/local-admin/v1/)）  
> 日期：2026-09-18  
> 上位文档：[INITIAL_DESIGN.md](./INITIAL_DESIGN.md)  
> 已接受决策：[ADR-0004](./adr/0004-local-admin-transport.md)  
> 安全约束：[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §8、§9.4、§9.5、§12.1、§13.2、§14  
> 模块边界：[MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) §4.9、§4.10  
> 命令与本地能力：[`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json)、[`SECURITY_DESIGN.md`](./SECURITY_DESIGN.md) §10.3

## 1. 文档职责

本文冻结 CLI（`app::cli`）与运行中的 Daemon（`server::local_admin`、`server::acp_facade`）之间本地通道的 endpoint 与访问控制、framing、两类载荷（管理信封与 ACP 流）的会话语义、v1 方法集及其 `params`/`result` 形状、本地错误码，以及失败关闭与生命周期规则。

ADR-0004 只给出架构决策，其“结果”段遗留的“请求/响应编码与版本标识”由本文定义；本文是该通道的**唯一权威来源**。本文不定义：

- Sync wire（[SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md)）与 Node Link wire（[NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md)）；
- ACP 本身与 `acp_facade` 的 ACP 语义（[ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md)）；
- Daemon 配置键与默认值（[CONFIG_REFERENCE.md](./CONFIG_REFERENCE.md)）、core 业务语义（[MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md)）；
- 平台 key 存储与配对密码学状态机（[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §9、[IDENTITY_AND_AUTH_CONTRACT.md](./IDENTITY_AND_AUTH_CONTRACT.md)、[SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §6/§7、[NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §9/§13）；本文只定义这些方法在本地通道上的 `params`/`result`。

边界规则：

- 管理方法承载的是 `core::use_cases` 的同一套请求/结果，不新增“管理专用”业务语义（ADR-0004 决策 3）。
- 本地通道不监听 TCP、不使用 HTTP、不涉及 Origin/Host/TLS，也不引入本地 token 或设备身份（ADR-0004 决策 4、7）。
- 通道内的 JSON **不是** Sync/Node Link 的 wire schema：它不与 `schemas/sync`、`schemas/node-link` 共享定义，也不进入那两棵 fixture 树。它自己的机器表达只在 [`schemas/local-admin/v1/`](../schemas/local-admin/v1/)（envelope、错误码与 framing 常量）与 [`fixtures/local-admin/v1/`](../fixtures/local-admin/v1/)，**不覆盖** `0x02` 的 ACP 字节流（那一段只有分帧与上限是合同，§3.1）。

### 1.1 通用编码规则

| 项目 | 规则 |
|---|---|
| 字符编码 | UTF-8，无 BOM；不允许重复对象键、`NaN`、`Infinity` 或尾随内容 |
| 字段命名 | JSON 字段用 camelCase（与 Sync/Node Link 一致）；CLI 命令行参数用 kebab-case，映射只发生在 `app::cli` |
| UUID | 带连字符的小写 canonical 文本，与 `SYNC_PROTOCOL.md` §3 规则相同 |
| 时间戳 | UTC RFC 3339，毫秒精度强制，例如 `2026-09-18T09:12:03.412Z` |
| 二进制 | 无填充 base64url |
| 路径 | 绝对路径；Daemon 使用前必须规范化并校验（`SECURITY_DESIGN.md` §12.3、§13.2） |
| 信封 | closed object：未知字段、未知 `v` 一律不接受 |
| `params` | closed object：未知字段 → `local.invalid_params` |
| `result` | 客户端必须忽略未知 `result` 字段，保证新旧 CLI 与 Daemon 混合时可读 |

## 2. Transport 与端点

### 2.1 endpoint 位置

| 平台 | endpoint |
|---|---|
| Windows | Named Pipe `\\.\pipe\acp-remote-<userSidHashHex>-<instanceId>` |
| Unix | `$XDG_RUNTIME_DIR/acp-remote/<instanceId>.sock`，目录 `0700`、socket 文件 `0600` |

- `<userSidHashHex>` = 当前 OS 用户标识的 SHA-256 摘要的**小写十六进制前 16 个字符**。输入字节固定为：Windows 取 SID 字符串形式（如 `S-1-5-21-…`）的 UTF-8 字节；Unix 取十进制 UID 的 UTF-8 字节（如 `1000`）。两种情形都不得附加结尾换行或前缀。
- `<instanceId>` = 单实例锁持有者在获取锁时生成的 16 字符小写十六进制串（64 bit CSPRNG），写入锁文件供 CLI 读取；同一 Daemon 运行期内不变，重启后重新生成（`SECURITY_DESIGN.md` §12.1、`CONFIG_REFERENCE.md` 的 `daemon.instance_lock`）。
- `XDG_RUNTIME_DIR` 未设置时，Unix 回落到 `<daemon.data_dir>/run/<instanceId>.sock`，目录权限同为 `0700`。
- Windows 的 pipe 名只用于定位，**不是**授权手段：授权由 ACL 承担，名字不视为秘密。

### 2.2 访问控制只按 OS 用户

- endpoint 只允许启动 Daemon 的同一 OS 用户连接：Windows 创建 Named Pipe 时使用仅含该用户 SID 的 SDDL；Unix 使用目录 `0700` 与 socket `0600`。
- 连接建立后必须校验对端凭据，不能只依赖 endpoint 权限：
  - Windows：`GetNamedPipeClientProcessId` 取客户端进程，`OpenProcessToken` + `GetTokenInformation(TokenUser)` 取 SID，与 Daemon 进程的用户 SID 比对。
  - Unix：`SO_PEERCRED` 取对端 `uid`，与 Daemon 的有效 `uid` 比对。
- 凭据不一致时：不发送任何 frame，立即关闭连接，并记一条 `SECURITY_DESIGN.md` §14.2 的 `authorization.denied` 审计事件。
- 不引入本地 token、不复用设备或节点身份、不开放 loopback HTTP 管理面（ADR-0004 决策 4、7）。本通道不进入远程攻击面，也不受设备 scope 或 Export grant 影响。

## 3. Framing

每个 frame：

```text
u32be length | payload(length bytes)
```

- `length` 只计 payload 字节数，不含 4 字节头。
- `length > 1_048_576`（1 MiB）→ 关闭连接。
- `length == 0` → 关闭连接（channel 字节是强制的）。
- payload 第 1 字节是 channel：

| channel | 载荷 | 归属 |
|---|---|---|
| `0x01` | 管理请求/响应，UTF-8 JSON；一帧一条完整信封，不跨帧、不合并 | `server::local_admin` |
| `0x02` | ACP 字节流，对本地管理完全不透明 | `server::acp_facade` |

- 第一条 frame 的 channel 决定该连接的用途。同一连接上出现与首帧不同的 channel → 关闭连接：管理连接可在一次会话内承载多个请求/响应，ACP 连接是长期双向的，两者不混用。
- `0x02` 的长度分帧由本层负责；ACP 自身的 JSON-RPC 分帧仍由 facade 负责，本地管理不解析其内容。
- ACP 流的背压按 `SECURITY_DESIGN.md` §15：单方向未消费字节上限 1 MiB，达到上限时对应方向的读取方停止拉取，不得缓存无界数据。
- framing 错误（长度非法、channel 未知、混用 channel）一律关闭连接且不返回错误帧：此时链路本身不可信，错误帧同样不可信。

### 3.1 `0x02` ACP 流的会话生命周期

`[待实现]` §3 只分配了 channel 与分帧；本节冻结那条长期双向 ACP 流的会话语义。它决定 `acp-remote acp-stdio` 能否工作，以及能否满足「facade 常驻 Daemon、CLI 只是字节泵、不得自行打开数据库或启动第二套核心」（[MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) §4.9）。

**分帧与上限**

- 一帧恰好一个完整的 ACP JSON-RPC 文本消息：不跨帧、不合并，UTF-8 无 BOM；空帧按 §3 关闭连接。
- ACP 层单消息上限 = 1 MiB，与 §3 的帧上限同值（合法取等号）。facade 必须在 ACP 层先判上限并对超限消息**返回 ACP 错误**，不得生成超过帧上限的帧；一旦出现超过帧上限的帧，就是本地通道的 framing 错误，按 §3 关闭连接并记结构化警告。
- ACP 自身的 JSON-RPC 分帧（换行/长度前缀）仍由 facade 负责；本地管理不解析内容（§3）。

**attachment 与关联**

- 每次 `0x02` 连接建立即分配一个新的 `FacadeAttachmentId`，连接结束即作废。该标识与 Node Link 的 `attachmentId`/`attachmentGeneration` 是同一概念的不同层实例（[MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) §4.9），不共用字段也不跨层传递。
- 响应与取消必须按 `(FacadeAttachmentId, ACP request id)` 关联；已作废 attachment 的响应/延迟帧一律丢弃并计入结构化警告，不得命中新 attachment 的会话绑定。
- ACP 上游请求的 `id` 不保证是整数（实测 Zed 使用 UUID 字符串，[ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) §6）；facade 保存与回填时必须保持原始类型与字面量。

**并发**

- 一条 `0x02` 连接上可以并存多个 ACP 会话（Zed 用一个 agent stdio 连接管理多会话）。
- 「同一会话最多一个 active turn」由 core 判定，facade **不得**重复实现或绕过：facade 只按 core 的 `CommandReceipt` 与终态事件回填 ACP 响应。
- 单连接未完成的上游请求上限 32（与 §4 第 2 条同值）；超限时 facade 以 ACP 错误回复并**停止读取该方向**，不得静默丢弃或无限缓存。

**断线与崩溃**

- CLI 进程退出或连接 EOF：facade 结束该 attachment 上的全部 ACP 会话绑定，并把本端发起的 pending 权限/elicitation 请求标记为「上游已断开」。
- 上游断开**不得**代答权限请求，也不得按 Export grant 自动允许/拒绝（[ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) §6）：Owner 侧的 `permission.requested` 保持 pending，直到新连接重新送达并得到真实答复。
- 已经在运行的 turn **不因**上游断开而取消：它由 Owner/Agent 继续，事件照常先持久化再广播（`AGENTS.md` §4）。

**重连与幂等**

- 重连是新 attachment：facade 用本地持久状态（owned）或 Owner 重放（imported）重建会话视图，不重放副作用命令。
- 会话创建仍走稳定 `requestId`：`session/new` → `command.submit{command:"session.create"}`（[ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) §6），重连后重复的 `session/new` 不得创建第二个会话。
- Daemon 未运行或正在关闭：CLI 以明确错误退出（§7），不得自行打开数据库、不得启动第二套核心。

**边界**

- 本地通道不解析 ACP 内容（§3）；facade 不直接调用 `node-link-client`，远程会话经 core 的用例面与后端端口访问（[MODULE_ARCHITECTURE.md](./MODULE_ARCHITECTURE.md) §4.9）。
- Node Link / Sync 错误码到 ACP 错误的映射表在 [ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) §6，本地不新增第二张表。
- 本节的机器表达范围：管理载荷（channel `0x01`）的 envelope 与错误码在 [`schemas/local-admin/v1/`](../schemas/local-admin/v1/)；`0x02` 的字节流本身**不建 schema**（它对本地管理不透明，只有分帧与上限是合同）。

**实现状态**

> `[现状]`（2026-09-25，切片 4 `daemon-cli-and-local-admin`）`server::acp_facade` 尚未落地。在该实现缺席期间，Daemon 对 `0x02` 连接的处理是：完成 §3 的 framing 校验（首帧 channel、帧上限、空帧规则、channel 不混用）后**立即关闭连接并记一条结构化警告**（facade 未装配），不返回错误帧、不转发任何字节，也不让该连接占用可用的 `FacadeAttachmentId`；`acp-remote acp-stdio` 侦测到连接被立即关闭时以明确错误（Daemon 未提供 ACP 流）非零退出。
>
> 这是 `daemon-cli-and-local-admin` 变更的 design.md 决策 5 在 facade 缺席期的行为，**不改变本节任何语义**：`0x02` 的会话语义、attachment 生命周期、并发上限与重连幂等仍是落地目标；切片 6 接入 facade 时只替换分发目标，framing 与 attachment 生命周期代码不动。

## 4. 管理信封

```text
请求  { "v": 1, "id": "<uuid>", "method": "<name>", "params": { … } }
成功  { "v": 1, "id": "<same uuid>", "ok": true,  "result": { … } }
失败  { "v": 1, "id": "<same uuid>", "ok": false, "error": { "code": "<code>", "message": "<text>" } }
```

| 字段 | 类型 | 规则 |
|---|---|---|
| `v` | integer | 通道版本；v1 只接受 `1` |
| `id` | UUID | 请求标识；同一连接上未完成请求的 `id` 必须唯一 |
| `method` | string | 小写点分 ASCII，匹配 `^[a-z][a-z0-9]*(\.[a-z0-9]+(-[a-z0-9]+)*)*$`（段内允许连字符，段首/段尾不得为连字符） |
| `params` | object | 无参数方法必须发送 `{}`，不用 `null` |
| `ok` | boolean | 响应判别字段 |
| `result` | object | `ok = true` 时存在；无返回值的方法返回 `{}` |
| `error` | object | `ok = false` 时存在，仅含 `code`（string）与 `message`（string） |

规则：

1. `v != 1` → 关闭连接：版本未知时对端语义不可判定，不能用错误帧回答。
2. 响应必须回带与请求相同的 `id`。同一连接同时最多 32 条未完成请求，超出属于协议滥用 → 关闭连接。
3. 未知 `method` → `local.unsupported`；这使新增方法成为向后兼容变更。
4. `params` 缺字段、类型不符或出现未知字段 → `local.invalid_params`；信封本身非法（`id`/`method` 缺失或命名非法、`params` 不是 object）→ `local.invalid_request`。
5. 每个请求恰好一个响应；v1 不定义服务端主动通知，ACP 流不受此约束。
6. `error.message` 是简短英文描述，只用于本地日志与 CLI 输出，不含 secret、凭据值、堆栈或完整敏感路径；`result` 永不回显凭据值（§5.2 `daemon.status`/`agent.list`）。

## 5. 方法集（v1）

### 5.1 方法与 `local.*` 能力的对应

`SECURITY_DESIGN.md` §10.3 的 7 个 `local.*` 能力与方法的对应关系如下，不允许出现没有方法承载的能力，也不允许出现不属于任何能力的管理方法：

| `local.*` 能力 | 方法 | 承载层 |
|---|---|---|
| `local.agent.configure` | `agent.list`（只读；Agent 声明由配置文件提供） | `core::use_cases` 本地配置族 |
| `local.device.manage` | `device.pair.begin`、`device.pair.status`、`device.pair.confirm`、`device.pair.reject`、`device.list`、`device.revoke`、`node.pair.begin`、`node.pair.status`、`node.pair.confirm`、`node.pair.reject`、`node.list`、`node.revoke` | `DeviceManagement` |
| `local.node.rotate-key` | `node.rotate-key.begin`（`post_mvp`，§5.7） | `DeviceManagement`（与 `node.rotate-key.*` 消息同批定义） |
| `local.audit.export` | `audit.export` | 审计仓储的本地读取端口（不经业务用例） |

补充说明：

- `daemon.status`、`daemon.stop` 不对应任何 `local.*` 能力：它们是没有 scope 判定的 daemon 生命周期调用，由组合根回答。
- 本表是 ADR-0004 决策 3 的三个 use case 名的展开：身份、Export 与远程目录相关方法走 `DeviceManagement`／`ExportManagement`／`RemoteCatalogQueries`；本地配置与 daemon 生命周期方法分别走本地配置族与组合根，不引入第二套管理语义。

### 5.2 Daemon 生命周期与本地配置

管理配置的持久化权威、首次 profile 导入、重启恢复与生效时机以 [CONFIG_REFERENCE.md](./CONFIG_REFERENCE.md) 的“配置与管理状态的权威”为准；本节只定义请求与响应。本节方法是 v1 合同，其**本地通道部分已按状态行落地**（`server::transport::local` + `server::local_admin` + `app`）；`server::acp_facade` 相关语义待切片 6，实现期差异见 §3.1 末尾的实现状态注记。

#### `daemon.status`

- `params`：`{}`
- `result`：

```text
version       string                 # acp-remote 包版本
instanceId    string                 # §2.1 的实例标识
nodeId        UUID
nodePublicKey string                 # base64url 65 bytes
startedAt     timestamp
uptimeMs      integer
dataDir       string                 # 绝对路径
listen        string[]               # 实际监听地址
publicOrigin  string | null
counts        { devices: integer, nodes: integer, exports: integer, imports: integer }
agents        { agentId: string, available: boolean }[]
links         { nodeId: UUID, state: "connected" | "connecting" | "offline", lastConnectedAt: timestamp | null, nextRetryAt: timestamp | null }[]
uiBuildVersion string | null         # 本次二进制嵌入并托管的前端构建版本
```

- `uiBuildVersion` 是本次二进制**嵌入并被托管**的前端构建版本（`FRONTEND_DESIGN.md` §4.3 的「版本可检查地关联」）：取值与浏览器拿到的静态资源出自**同一份**嵌入资源表（前端构建钩子写入产物的 `build-version.json`，见 `clients/app/scripts/write-build-version.mjs`），不是 Daemon 自己算的产物哈希。`null` 表示**版本缺失**（占位入口，或产物没有可识别的版本文件），是显式缺失语义而不是「未知」或空串——不编造版本。它是 `result` 里的一个**字段**，**不改变**本文件的方法集与 §6 的本地错误码集合。

- `links` 是组合根持有的运行时状态（`NODE_LINK_PROTOCOL.md` §15 的连接策略），**不是**持久记录；它解释 `import.add`/`session.create` 为何返回 `local.unavailable`。未配对或已撤销的节点不出现在 `links` 里。

- 由组合根回答，不经 `core::use_cases`；`daemon start` 不经过本通道（ADR-0004 决策 1）。
- Daemon 未运行时由 CLI 在进程内回答，见 §7。

#### `daemon.stop`

- `params`：`{ graceMs: integer | null }`；`null` 表示使用 `daemon.shutdown_grace_ms`，非空时取值 `0`–`60000`。
- `result`：`{ accepted: true }`，`accepted` 恒为 `true`；失败必须走 `error`，不得用 `false` 表达。
- 响应在关闭序列开始时发出；CLI 随后等待锁释放与连接关闭。关闭顺序按 `SECURITY_DESIGN.md` §12.1 与 `AGENTS.md` §7。
- 响应写回后由**服务端结束该连接**：发出 `daemon.stop` 的这条连接不会再收到任何响应，也不进入关闭序列的连接排空等待。这与「关闭序列进行中**新**请求以 `local.unavailable` 失败关闭」是两件事、互不覆盖——后者约束关闭开始后到达的新请求，本条约束的是**发出 `daemon.stop` 的连接自身**（它的语义是请求关闭，不是新请求）。

#### `agent.list`（`local.agent.configure`，只读）

- `params`：`{}`
- `result`：

```text
agents { agentId: string, command: string, args: string[], default: boolean,
         resolvedPath: string, exists: boolean }[]
```

- Agent 声明来自配置文件 `[agent]`（`CONFIG_REFERENCE.md` §7），本方法只读取配置解析结果与可执行文件探测结果，不写任何状态、不启动进程。
- `resolvedPath` 是命令解析后的路径（带路径分隔符时为原值，否则在 `PATH` 中查找）；找不到时仍返回解析尝试的结果且 `exists = false`。
- `agent.configure` 与 `provider.configure` 已删除：调用它们一律返回 `local.unsupported`。

### 5.3 设备配对与信任（`local.device.manage`）

`DeviceRecord`（`device.list` 与配对结果共用）：

```text
deviceId            UUID
displayName         string                # ≤128
publicKeyFingerprint string               # SHA-256(65 bytes 未压缩 P-256 公钥原始字节) 的小写 hex，64 字符，无分隔符
state               "pending" | "active" | "revoked"
createdAt           timestamp
lastSeenAt          timestamp | null
revokedAt           timestamp | null
```

设备记录**不含任何授权集合**：通过设备认证即可执行本节点全部业务命令（`SECURITY_DESIGN.md` §10.1）。

#### `device.pair.begin`

- `params`：`{ expiresInMs: integer | null }`——设备配对不申请任何授权词汇，本方法**只**接受这一个键；出现 `requestedScopes`/`requestedPacks` 之类的键即 `local.invalid_params`（`params` 是 closed object）。
- `result`：`{ pairingId, pairingUrl, expiresAt }`；不返回任何授权集合。
- 配对有效期固定 5 分钟（`SYNC_PROTOCOL.md` §7、§14）；`expiresInMs` 只能收窄，不能延长。
- `pairingUrl` 只经本通道返回；fragment、`pairingSecret` 与完整 QR payload 不得进入日志或 analytics（`SECURITY_DESIGN.md` §14.1）。

#### `device.pair.status`

- `params`：`{ pairingId }`
- `result`：

```text
state                "pending" | "claimed" | "approved" | "rejected" | "expired"
displayName          string | null
publicKeyFingerprint string | null
deviceId             UUID | null
expiresAt            timestamp
```

- claim 之前只返回 `state` 与 `expiresAt`，其余字段为 `null`。
- 不返回短验证码，也不返回任何授权集合（设备配对不授予授权；理由见 `SECURITY_DESIGN.md` §5.1）。

#### `device.pair.confirm`

- `params`：`{ pairingId }`——设备方向不提交任何授权集合（授权集合不是本方法的输入）。
- `result`：`{ deviceId, confirmedAt }`。
- 只有本地用户显式确认后设备记录才获得 `active`；本方法必须等持久状态提交成功后才返回。

#### `device.pair.reject`

- `params`：`{ pairingId, reason: string | null }`
- `result`：`{}`

#### `device.list`

- `params`：`{}`
- `result`：`{ devices: DeviceRecord[] }`，包含 `pending` 与 `revoked` 记录，由 CLI 决定展示方式。

#### `device.revoke`

- `params`：`{ deviceId }`
- `result`：`{ deviceId, revokedAt }`
- 必须在持久状态提交后立即关闭该设备的 active connection 才返回（`SECURITY_DESIGN.md` §9.5、§10.1）。

### 5.4 节点配对与信任（`local.device.manage`）

`NodeRecord`（`node.list` 与配对结果共用）：

```text
nodeId                  UUID
displayName             string              # ≤128
kind                    "access" | "owner"  # 该记录代表的对端角色：access = 我们信任的 Access Node；owner = 我们导入的 Owner Node
nodePublicKeyFingerprint string             # 同 DeviceRecord 的指纹规则
grants                  string[]            # grant.* 子集
exportIds               string[]            # 该节点可见 Export 的白名单（去重、字典序；空数组 = 看不到任何 Export；语义见 NODE_LINK_PROTOCOL.md §8.2）
state                   "pending" | "paired" | "revoked"
ownerEndpoint           string | null       # kind = "owner" 时为 wss:// 端点
createdAt               timestamp
lastConnectedAt         timestamp | null
revokedAt               timestamp | null
```

#### `node.pair.begin`

- `params`：

```text
mode            "owner" | "access"
pairingUrl      string | null   # mode = "owner" 时必须为 null；mode = "access" 时为用户提供的 Owner 二维码 URL
displayName     string          # ≤128，本节点在对端界面上的展示名（nodeName）
requestedGrants string[]        # mode = "access" 时携带，取值限于 commands.json 的 grant.*
```

- `mode = "owner"`：本节点创建一次性配对并返回二维码 URL，claim 之前不知道对端身份。
- `mode = "access"`：Daemon 按 `NODE_LINK_PROTOCOL.md` §13.2 向 `pairingUrl` 中的 `endpoint` 执行 claim；`pairingUrl` 与其中的 secret 只存在于本次调用与内存，读取后立即清除。
- `result`：

```text
pairingId               UUID
pairingUrl              string | null    # owner：新建 URL；access：回显输入
state                   "pending" | "claimed" | "pending_confirmation" | "approved" | "rejected" | "expired"
expiresAt               timestamp
peerNodeId              UUID | null      # claim 之后可确定
peerPublicKeyFingerprint string | null
```

#### `node.pair.status`

- `params`：`{ pairingId }`
- `result`：`{ state, peerNodeId, peerPublicKeyFingerprint, requestedGrants, expiresAt, lastRefreshError }`；除 `expiresAt` 与 `state` 外均可为 `null`。本协议不派生也不返回短验证码（理由见 `SECURITY_DESIGN.md` §5.1）。
- `mode = "access"` 且状态非终态时，Daemon 按 `NODE_LINK_PROTOCOL.md` §13.3 向 Owner 刷新状态；刷新失败不改变已有状态，只在 `lastRefreshError`（string \| null）中给出简短原因。

#### `node.pair.confirm`

- `params`：`{ pairingId, grants: string[], exportIds: string[] }`——`exportIds` 是**必填**字段（取值可以是空数组）；缺少该字段 → `local.invalid_params`，且不创建信任。`schemas/local-admin/v1/envelope.schema.json` 只把 `params`/`result` 建模为通用对象（逐方法形状由本文拥有），因此「必填」由 Daemon 在运行期强制，不写进 JSON Schema。
- `result`：`{ nodeId, grants, exportIds, confirmedAt }`——`exportIds` 回显本次填报的最终清单。
- 只有 Owner 侧本地确认才创建信任记录并分配初始 `grant.*`（`NODE_LINK_PROTOCOL.md` §13.2）；必须在持久状态提交后才返回。
- `exportIds` 是「该节点可见 Export」的白名单，**只能收窄**：可见性 = `NODE_LINK_PROTOCOL.md` §8.2 的三个条件（未撤销 ∧ `export.scopes ∩ grants ≠ ∅` ∧ `exportId ∈ exportIds`）。每个 `exportId` 必须本机存在且未撤销，否则 `local.not_found`；其 `scopes` 必须与本次 `grants` 有交集，否则 `local.invalid_params`。三类失败（缺少字段、不存在/已撤销、与 `grants` 不相交）都不创建信任、不推进配对状态、不写审计。空清单合法，等于该节点看不到任何 Export（即使 `scopes` 与 `grants` 相交）。
- 清单只在确认时填报，没有配对后修改入口（已知限制，见 `NODE_LINK_PROTOCOL.md` §8.2）：改清单只能重新配对——要么先 `node.revoke` 再重新配对，要么直接重新配对；两种做法都在落定后立即关闭该节点的活动连接，收窄因此自下一次握手后的新连接生效。`export.revoke` 不级联清理清单条目。v3 及更早的库升级到 v4 后既有节点行的清单为空，这类配对需要在本机重新确认才能恢复可见（`NODE_LINK_PROTOCOL.md` §8.2 的升级副作用）。

#### `node.pair.reject`

- `params`：`{ pairingId, reason: string | null }`
- `result`：`{}`

#### `node.list`

- `params`：`{}`
- `result`：`{ nodes: NodeRecord[] }`

#### `node.revoke`

- `params`：`{ nodeId }`
- `result`：`{ nodeId, revokedAt }`
- 撤销后该节点不得再建立 Node Link 连接，本地必须停止重连并关闭 active connection（`SECURITY_DESIGN.md` §9.5）。

### 5.6 审计导出（`local.audit.export`）

#### `audit.export`

- `params`：

```text
outputPath string                  # 绝对路径；文件已存在 → local.conflict
format     "jsonl" | "csv"
since      timestamp | null
until      timestamp | null
categories string[]                # SECURITY_DESIGN.md §14.2 的审计类别名，可为空数组但必须显式给出
```

- `result`：`{ outputPath, recordCount: integer, sha256: string }`；`sha256` 是导出文件的小写十六进制摘要。
- 只包含 `SECURITY_DESIGN.md` §14.2 的元数据字段，**不得**包含 prompt、回复、diff、终端、ACP `rawJson`、附件内容、凭据、pairing secret 或 QR payload（§14.1）；审计导出不能成为第二份聊天记录。
- 时间区间与类别过滤在 Daemon 内完成；输出按记录时间升序，`jsonl` 每行一个对象，`csv` 首行为列名。

### 5.7 明确推迟（`local.node.rotate-key`）

#### `node.rotate-key.begin`

- `node.rotate-key.begin` 属于 `post_mvp`：本轮**只登记方法名**，`params`/`result` 与 `NODE_LINK_PROTOCOL.md` §12.3 的 `node.rotate-key.request`／`node.rotate-key.result` 同批定义，不提前发明字段。
- 它表达“本地用户请求本节点轮换 Node Identity Key”，对应 `local.node.rotate-key`；轮换后所有已配对设备与节点必须重新配对，不能靠普通 endpoint 更新掩盖密钥变化（`SECURITY_DESIGN.md` §9.1、§9.5）。
- 在字段定义落地前调用本方法返回 `local.unsupported`；实现不得自行填充参数形状。

### 5.8 CLI 子命令与方法的映射

`[决定]` 本节是 **CLI 子命令 ↔ 本地方法/业务命令的唯一映射**。`app::cli` 只做参数解析、展示与轮询，**不复制业务规则、不自行读写 SQLite**（[ADR-0004](./adr/0004-local-admin-transport.md) 决策 3）；内部仍走同一个 `core::use_cases`。

| CLI 子命令 | 通道 | 底层 | 说明 |
|---|---|---|---|
| `daemon start` | 不经本地通道 | CLI 进程自身 | 单实例锁失败必须给明确错误，不得强杀进程（§7） |
| `daemon stop` / `daemon status` | 本地方法 | `daemon.stop` / `daemon.status` | 由组合根回答，不经用例层 |
| `doctor` | 本地方法 + CLI 侧检查 | `daemon.status` + 本地检查 | 离线时在 CLI 进程内完成（ADR-0004 决策 1）；**不新增方法**（不得自造 `daemon.doctor`） |
| `agent list` | 本地方法 | `agent.list` | 只读：打印配置文件解析出的 id、命令、argv、默认标记与可执行文件是否存在 |
| `device pair` | 本地方法 ×3 | `device.pair.begin` → `device.pair.status`（轮询）→ `device.pair.confirm` | 多步流程见下；参数只有 `--fingerprint`（非交互确认）与 `--expires-in-ms`——设备方向没有授权词汇 |
| `device list` / `device revoke` | 本地方法 | 1:1 | |
| `node pair` | 本地方法 ×3 | `node.pair.begin`（`mode = owner\|access`）→ `node.pair.status` → `node.pair.confirm` | 两个方向的差异见下；参数为 `--mode`/`--pairing-url`/`--display-name`/`--grant`/`--export-id` 与非交互的 `--fingerprint` |
| `node list` / `node revoke` | 本地方法 | 1:1 | |
| `acp-stdio` | channel `0x02` | stdin/stdout 字节泵 | 不进管理信封；会话语义见 §3.1 |

**首切片不提供 `session create`**，`session list` 标为 `post_mvp`：业务命令（`session.*`、`permission.resolve` 等）在 `compatibility/commands/v1/commands.json` 里的 transport 只有 `sync` 与 `node_link`，**本地通道不承载业务命令**；本地会话的唯一入口是 `acp-remote acp-stdio`（Zed → `server::acp_facade` → `core::use_cases::create_session`，不经 wire 命令）。若要给 CLI 加会话语义，必须先决定是否为命令引入 `local` transport（那时需同步 `commands.json`、`broker::required_grant` 镜像与那条“四处一致”门禁），**不得**在本地通道另造同名方法。

**配对流程（安全仪式）**：`SECURITY_DESIGN.md` §9.4 要求确认界面显示名称、指纹与过期时间（节点方向另显示待授予的 `grant.*`）；`SYNC_PROTOCOL.md` §7.0 要求确认只能经本地入口。因此 `device pair` / `node pair` 的 CLI 流程固定为：

1. `begin`：拿到 `pairingId` 与 `pairingUrl`，打印 URL 与二维码（如终端支持）；
2. 轮询 `status`：未认领时继续等（有效期 5 分钟，到期报 `local.expired`）；认领后**必须**展示名称、指纹与过期时间，节点方向另展示待授予的 `grant.*`；
3. 用户输入确认后调 `confirm`：设备方向**不提交任何授权集合**，节点方向把用户确认过的 `grants` 与本次点名的 `--export-id` 清单作为最终集合提交。

示例（交互式设备配对）：

```text
$ acp-remote device pair
https://home.example.invalid/sync/v1/pairing#…（含 pairing secret，只在本终端展示）
正在等待设备认领…（有效期至 2026-09-23T14:31:00.000Z，Ctrl-C 取消）

设备已认领：
  名称            Zhang's Phone
  指纹            ab12…（64 位 hex）
  过期            2026-09-23T14:31:00.000Z
确认与本机记录该设备（不授予任何授权）？[y/N]: y
已配对：deviceId=2ae1c07c-…
```

节点方向多一行待授予的 `grant.*`（授权只存在于跨节点方向）：

```text
$ acp-remote node pair --mode owner --display-name my-node --grant grant.session.read --export-id export-1
…
节点已认领：
  名称            my-node
  指纹            cd34…（64 位 hex）
  待授予 grants   grant.session.read
  过期            2026-09-23T14:31:00.000Z
确认授予以上 grants（1 项）？[y/N]: y
已配对：nodeId=7f30a1b2-…
```

非交互场景（脚本、远程 shell）必须把从另一台设备读到的指纹显式传入，**不得**提供“无参数自动确认”开关：

```text
$ acp-remote device pair --fingerprint ab12…（64 位 hex）
```

`--fingerprint` 必须与 `status` 返回值逐字匹配才调 `confirm`（不 trim、不折叠大小写、不允许前缀匹配）；不匹配时以非零退出且不修改任何状态。短验证码与设备方向的授权词汇都没有参数：`--sas`/`--request`/`--pack` 不是本 CLI 的开关（出现即用法错误，不得被静默忽略；理由见 `SECURITY_DESIGN.md` §5.1）。`node pair` 的方向差异：`--mode owner` 在本机生成二维码并**在本机确认**；`--mode access` 拿用户给的 pairing URL 执行 claim，**等待对端（Owner）本机确认**，本机没有 `confirm` 调用。`pairingUrl` 与其中的 secret 只允许在内存与终端展示中存在，不得写入 shell 历史、日志或文件（`SECURITY_DESIGN.md` §14.1）。

**退出码与错误输出**：成功 `0`；失败非零。失败时 stdout 只输出人类可读的简短说明，机器可读信息以一行结构化 JSON 输出到 **stderr**（至少含 `code`，取值来自 §6 或 `compatibility/errors/v1/errors.json`）。CLI **不得**把 `error.message` 当稳定契约解析，也不得在输出中回显 secret 或完整路径（§4 第 6 条）。

## 6. 本地错误码

| code | 含义 | retryable |
|---|---|---|
| `local.unsupported` | 未知 `method`，或方法在当前 delivery 阶段尚未实现 | 否 |
| `local.invalid_request` | 信封非法：`id` 缺失/非 UUID/重复、`method` 缺失或命名非法、`params` 不是 object | 否 |
| `local.invalid_params` | 方法参数非法：缺字段、类型不符、越界或出现未知字段 | 否 |
| `local.not_found` | 目标 `deviceId`/`nodeId`/`pairingId`/`exportId`/`importId` 不存在，或引用的目录标识未在本机目录表记录 | 否 |
| `local.conflict` | 目标存在但当前状态不允许该操作（pairing 已确认、`exportId` 已存在、输出文件已存在） | 否 |
| `local.expired` | `pairingId` 已过期或已被拒绝 | 否 |
| `local.remote_error` | `node.pair.*` 对 Owner 的 HTTPS 调用失败或返回错误（`NODE_LINK_PROTOCOL.md` §13.4） | 视远端状态 |
| `local.unavailable` | Daemon 正在关闭、存储不可用、keystore 不可用或依赖未就绪 | 是 |
| `local.internal` | 未预期内部错误；`message` 不得包含堆栈、SQL 或敏感路径 | 是 |

- 命名与 wire 协议一致：小写点分 ASCII，匹配 `^[a-z][a-z0-9]*(\.[a-z0-9]+)*$`。`local.` 前缀与 Sync、Node Link 的错误码空间不重叠。
- 连接级故障（未知 channel、帧超长、混用 channel、超出未完成请求上限、`v != 1`）不返回错误帧，直接关闭连接（§3、§4）。
- 本表不属于 `compatibility/errors/v1/errors.json`：该文件只登记出现在 WSS wire 上的错误码；本地错误码由本文与 `server::local_admin` 拥有，不复用 `SYNC_PROTOCOL.md` §12.2 或 `NODE_LINK_PROTOCOL.md` §14.1 的码。

## 7. 生命周期与失败关闭

- **Daemon 未运行**：CLI 先读单实例锁判定。没有有效锁时，`daemon status`/`daemon stop` 由 CLI 在进程内回答；其余方法以非零退出码与明确 stderr 报错。CLI **不得**因此打开数据库或启动第二套核心（ADR-0004 决策 5、`SECURITY_DESIGN.md` §8）。
- **`daemon start`** 不经过本通道，由 CLI 前台进程内完成（ADR-0004 决策 1）。
- **Daemon 运行中**：`daemon status` 与 `daemon stop` 经本地 IPC，由组合根提供状态或启动正常关闭；有有效锁但 IPC 不可达时明确报错，不视为“未运行”，不自动强杀、不直接读库。
- **连接中断**：CLI 必须报错退出，不得降级为直接读写 SQLite。重试由用户显式发起，且：
  - `*.list`、`*.status` 幂等，可安全重复；
  - `device.pair.begin`、`node.pair.begin` 每次调用创建新配对，旧配对自然过期，不构成第二次接受；
  - `export.create`、`import.add` 重试得到 `local.conflict`；`*.revoke`、`*.remove` 重试得到 `local.not_found`；
  - CLI 不得自动重试任何 mutation 方法。
- **endpoint 创建失败**（权限不符、路径被占用、socket 被替换为符号链接、目录权限不符、旧 socket 仍存活）→ 正式模式拒绝启动，不得降级为“无管理通道”或临时暴露无认证 endpoint（ADR-0004 决策 8、`SECURITY_DESIGN.md` §8）。
- **撤销与关闭顺序**：`device.revoke`、`node.revoke`、`export.revoke` 在持久状态提交后立即生效并关闭对应 active connection（`SECURITY_DESIGN.md` §9.5）；节点方向的 `node.pair.confirm` 在信任行提交后同样关闭该节点的活动连接（收窄/重新授权自下一次握手生效，`NODE_LINK_PROTOCOL.md` §8.2）；Daemon 关闭按停止接入 → 取消任务 → 关闭 Agent → 刷新存储 → 清理进程树的顺序执行（`SECURITY_DESIGN.md` §12.1）。
- **审计与日志**：拒绝连接、`SECURITY_DESIGN.md` §14.2 类别覆盖的安全动作（配对批准/拒绝、撤销、凭据配置等）记对应审计事件；**方法失败只记结构化日志**——§14.2 的封闭类别里没有「方法失败」，不得复用 `authorization.denied` 把参数错误与 scope 拒绝混为一谈。日志遵守 §14.1 的允许字段。

## 8. 版本与演进

- `v` 是通道版本，v1 只接受 `1`。framing、endpoint 授权模型或信封字段语义的不兼容变更必须新增 `v` 值（或新增 ADR），不能在同一 `v` 内静默改变语义。
- 兼容变更：新增方法（旧 Daemon 返回 `local.unsupported`，新 CLI 必须处理它）、新增可选 `params` 字段（实现方必须容忍缺失）、新增 `result` 字段（客户端忽略未知字段，§1.1）。
- 不兼容变更示例：给已有方法增加必填 `params` 字段、改变已有字段含义、改变 `result` 中已有字段的类型。
- 方法集、envelope、framing、endpoint 位置或授权规则变化时，必须更新本文；触发条件见 `AGENTS.md` §9。
