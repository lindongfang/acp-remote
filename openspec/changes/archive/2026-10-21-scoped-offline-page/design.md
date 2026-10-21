# Design

## Context

当前 PWA 的共享外壳已经在顶栏胶囊表达 12 态连接机，但离线状态同时出现在对话页内容来源条、会话头/会话行来源标签和文件栏提示里。顶栏没有自己的探活计时器：它只在浏览器报告 WebSocket `close` 时变化，而服务端心跳是 30 秒、静默超时是 90 秒，无法支撑“1 秒级准确”的在线/离线判断。

当前配对后业务页面会立刻挂载，即使连接还在 `connecting` / `authenticating`。本轮只处理当前 PWA 的一条设备到节点连接；移动端、Access 多 Owner 切换和标签页共享连接不在范围内。

## Goals / Non-Goals

**Goals:**

- 认证完成后，顶栏只表达在线/离线两档。
- 客户端 1 秒探活、3 秒仅 pong 判定，及时驱动 `reconnecting`。
- 用覆盖内容区的五类连接问题弹窗承载原因、错误码与恢复动作。
- 首次认证完成前不进入业务页面；已配对设备打开 `/pair` 自动回目录。
- 删除对话/文件区域重复的离线状态提示，保留功能性原因。

**Non-Goals:**

- 不修改服务端 30 秒心跳与 90 秒静默超时。
- 不新增 wire 消息、schema 或 Rust crate 依赖。
- 不实现移动端、Access 多 Owner、Owner 切换器或 Web Locks / BroadcastChannel 标签页协调。
- 不引入离线内容缓存、离线命令队列、后台常驻或系统通知。

## Decisions

### D1 — 认证门与配对入口

业务路由只在首次认证完成后挂载。认证前的 `connecting` / `authenticating` 由认证过渡屏承接，顶部不呈现全局顶栏；认证失败时同一个连接问题弹窗显示在过渡屏中央。`/pair`、`/node-link/pair` 与 `/hosts` 保持可达，不显示连接问题弹窗。

已配对设备（本地有设备身份与已配对主机记录）打开 `/pair` 时使用 `replace` 回到 `/`。判定放在配对路由/共享状态层，不靠用户手动点。

### D2 — 顶栏的在线/离线映射

连接机仍是唯一状态源，但顶栏只消费派生出的网络可用性：

```text
online  = replaying | online | incompatible | identity_changed | replaced
offline = disconnected | reconnecting | revoked
```

- `replaying` 算在线：认证已完成，连接可用；历史尚未追平由页面内容区表达。
- `revoked` 算离线：设备已被主机移除，当前没有任何可用连接。
- `incompatible`、`identity_changed`、`replaced` 算在线：网络可达，但业务阻断由弹窗说明。
- 首次认证前的 `connecting` / `authenticating` / `unpaired` / `pairing` 不进入顶栏判定，由认证过渡屏承接。
- 已认证后的 `connecting` / `authenticating` 属于断线重连的重新认证，仍按离线呈现（`hasAuthenticated` 区分首次认证与重连认证），不退回「状态未知」。

映射只在 feature 层实现一次，`Shell` 与弹窗读取同一份模型；组件不得各自写 `switch`。

### D3 — 五类连接问题弹窗

一个弹窗组件、五个变体，不在各页面复制：

| 变体 | 触发 | 顶栏 | 内容 | 动作 |
| --- | --- | --- | --- | --- |
| 普通离线 | `disconnected` / `reconnecting` | 离线 | 连接断开、自动重连进展、第 N 次与下次重试时间 | 无，自动重连 |
| 撤销 | `revoked` | 离线 | 设备已被主机撤销 | `重新配对` → `/pair` |
| 不兼容 | `incompatible` | 在线 | 协议版本或必需能力不兼容 | `重新加载` |
| 身份变化 | `identity_changed` | 在线 | 主机身份与配对记录不一致 | `重新配对` → `/pair` |
| 被顶替 | `replaced` | 在线 | 同一设备的另一条连接接管当前页 | `重新加载` |

弹窗覆盖内容区，顶栏保持可见且胶囊仍可点进 `/hosts`。弹窗不可关闭；只有恢复在线或执行恢复动作后消失。恢复入口路由（`/pair`、`/node-link/pair`、`/hosts`）不渲染弹窗。

错误信息由统一的 `ConnectionProblemModel` 提供：`kind`、标题、说明、`errorCode`、`closeCode`、`detail`、发生时间、重连计数与下次重试时间。技术详情可展开，但不包含私钥、token、配对秘密或请求体。错误码/关闭码取既有连接事件与关闭码表，不新增错误码词表。

### D4 — 客户端探活

探活放在连接层（`SyncConnection`），因为它已经持有 socket、`ClockPort`、`RandomPort` 与认证状态：

- 认证完成后启动；每条连接独立维护未回应 ping 的 `(nonce, sentAt)`。
- 页面可见时每 1000ms 发一条 `control.ping`；nonce 为 16 字节 CSPRNG，base64url 无填充。
- 只接受 nonce 匹配本连接未回应 ping 的 `control.pong`。业务事件、服务端 `control.ping`、命令结果都不重置计时。
- 最老未回应 ping 超过 3000ms：本地 `close()` 当前 socket，并通过既有事件流发一次 `closed`（`retryable: true`）。状态机进入 `reconnecting`，组合根按既有退避重连。
- 页面隐藏：停止发送并取消计时；不判定离线。页面可见：立即补发一条，并给约 3000ms 宽限。
- 连接关闭、阻断或重建时清理全部计时与未回应 nonce。

可见性通过既有 `LifecyclePort` 从组合根传入，连接层不直接读 `document`。`visibility` 只控制探活，不改变既有重连循环；`pagehide` 仍断开连接。

服务端 30 秒心跳与 90 秒静默超时保持不变。客户端探活不新增 wire 消息，服务端现有的 `control.ping` 处理会原样回 `control.pong`。

### D5 — 普通离线的重连节奏

沿用现有指数退避：500ms → 1s → 2s → … → 30s 上限，不设硬性最大次数，恢复 `online` 后清零。弹窗展示当前次数与下次重试时间，不提供重试按钮——普通离线是纯自动路径。

### D6 — 删除重复状态提示

移除：

- `ConversationStream` 的内容来源条（`data-connection-strip`、`data-content-origin`、`data-synced-at`）与 `originOfflineLabel` 小标签；
- `SessionRow` 的来源离线标签；
- `SessionFilesPanel` 的 `filesOffline` 提示。

同时删除：

- 目录页与目录详情页的内容来源判定（`contentOrigin`）与同步时间（`lastSyncedAtMs`）展示；
- 目录页根节点上的 `data-content-origin` / `data-synced-at` 机器属性，以及目录详情页的来源条；
- 目录模型、`DirectoryDetailModel` 与页面 props 中不再被消费的字段。

保留：

- 页面可访问时的功能性原因：单个 imported 来源离线、附件未就绪、提交进行中；
- 文件刷新失败原因、命令错误、加载/空态。

设备级离线弹窗覆盖内容区时，页面本身不可交互，功能性原因不需要在弹窗下展示；此时弹窗就是离线的唯一说明。因此输入模型 SHALL 把“设备连接离线”和“单个 imported 来源离线”拆开：设备连接离线仍禁用提交，但不显示离线原因文字；单个来源离线、附件未就绪、提交进行中、命令失败等页面可访问时的原因继续显示。

### D7 — 文档与兼容

- `docs/FRONTEND_DESIGN.md` §4/§5/§9 写回认证门、顶栏在线/离线、五类弹窗、探活与删除项。
- `docs/SYNC_PROTOCOL.md` §13 增加“客户端探活”说明，明确它复用 `control.ping/pong`、仅 PWA 行为、不改变 30 秒/90 秒服务端常量。
- PWA spec 通过本变更 delta 修改共享外壳、删除同步时间要求，并调整 imported 来源离线条款。
- 无数据库 migration；无 wire/schema/fixture 变更，因此不新增 fixture。`npm run check` 仍需通过文档与 OpenSpec 门禁。

## Risks / Trade-offs

- **仅 pong 会误判背压**：服务端在高水位时可能丢弃 pong。连续 3 秒无匹配 pong 会重连；这是“服务端不能及时回应探针”的显式结果，重连后探活会重新建立。若实测误判频繁，可把阈值调大，但不改回“任意入站消息”。
- **1 秒探活增加流量**：每条可见连接每秒一条小控制帧。相对业务事件量可忽略，但需在真机确认移动网络耗电与延迟。
- **顶栏在线 + 阻断弹窗**：`incompatible`、`identity_changed`、`replaced` 时顶栏仍显示在线，弹窗必须明确“网络在线但当前连接不可用”，避免用户误解。
- **认证门改变启动体验**：配对后到认证完成之间出现过渡屏；如果认证慢，用户看不到目录。这是“认证前不进入页面”的直接代价。
- **删除全部同步时间**：用户不再从任何页面看到数据新鲜度；离线时只能通过顶栏与弹窗知道连接状态，无法判断当前列表是实时还是上次快照。若后续需要，另立变更提供独立的数据新鲜度入口，但不恢复各区域的离线状态徽标。
- **探活暂停与后台连接**：隐藏时停止探活，但仍保留 socket 与服务端心跳；这依赖服务端 30 秒心跳继续保活，不能单独删除服务端心跳。
- **单节点范围**：本轮不实现多 Owner 切换；将来接入“当前来源”时复用同一派生模型，不在五类弹窗上另造一套。
