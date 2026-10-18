# node-link-listener Specification

## Purpose
定义 Daemon 共享网络接入面的可观察行为：`daemon.listen` 的绑定与失败关闭、非 loopback 监听的显式告警、按 path 的路由（Node Link WSS 与配对 HTTP 可用、Sync 与其余路径明确拒绝）、Host 与代理头边界校验，以及 TLS `proxy`/`direct` 两种终止模式，使 Node Link 端点只在受信 TLS 边界内可达。

## Requirements

### Requirement: 共享 listener 绑定与失败关闭

Daemon 启动时 SHALL 在恢复管理记录之后、开放业务接入之前绑定 `daemon.port`（默认 `8765`）与 `daemon.bind` 决定的共享 HTTP/WSS listener，Node Link 的 WSS 端点与配对 HTTP 端点共用该 listener 并按 path 路由。`daemon.bind` 缺省时，`proxy` 模式绑定 `127.0.0.1`，`direct` 模式绑定 `0.0.0.0` 并尽力补绑 `[::]`；显式 `daemon.bind` 只绑定该地址。绑定失败（端口占用、地址非法、权限不足）时 Daemon MUST 拒绝启动并给出明确错误，不得降级为无网络接入运行，也不得只绑定部分端点。`proxy` 模式 MUST 拒绝非 loopback 的 `daemon.bind`（明文面不得超出 loopback）。非 loopback 监听 MUST 在启动输出中给出告警。`daemon.status` 的 `listen` 字段 SHALL 返回实际绑定的监听地址清单。

#### Scenario: 默认 loopback 启动

- **WHEN** `proxy` 模式且配置未显式设置 `daemon.bind`，Daemon 启动成功
- **THEN** listener 绑定在 `127.0.0.1:8765`，`daemon.status` 的 `listen` 返回该地址，配对与 Node Link 端点在该地址上可达

#### Scenario: direct 模式默认绑定所有接口

- **WHEN** `daemon.tls.mode = "direct"` 且配置未显式设置 `daemon.bind`
- **THEN** Daemon 绑定 `0.0.0.0:8765` 并尽力补绑 `[::]:8765`，启动输出包含非 loopback 告警

#### Scenario: 绑定失败即拒绝启动

- **WHEN** `daemon.port` 与 `daemon.bind` 组合出的地址已被其他进程占用
- **THEN** Daemon 拒绝启动并以明确错误退出，本地管理通道与业务接入均不开放，不残留半初始化的监听

#### Scenario: proxy 模式拒绝非 loopback 绑定

- **WHEN** `daemon.tls.mode = "proxy"` 且 `daemon.bind` 是非 loopback 地址
- **THEN** Daemon 在配置解析阶段拒绝启动，并说明 proxy 模式只允许 loopback

### Requirement: 按 path 的服务清单

listener SHALL 按以下 path 提供服务：`/node-link/v1` 与 `/sync/v1` 接受 WebSocket 升级，各自的 subprotocol 固定为 `acp-remote.nodelink.v1.json` 与 `acp-remote.sync.v1.json`；两侧各自的 `/pairing/claim` 与 `/pairing/status` 接受对应的配对 HTTP 请求。其余未知 path 一律 404。对 WebSocket 端点的非升级请求 MUST 以协议错误拒绝；协商到 `permessage-deflate` 压缩或缺少各自约定 subprotocol 的升级请求 MUST 被拒绝。

#### Scenario: Node Link 端点正常升级

- **WHEN** 客户端向 `/node-link/v1` 发起携带 `acp-remote.nodelink.v1.json` subprotocol 且不请求压缩的 WebSocket 升级
- **THEN** 升级成功，连接进入认证前状态，等待 `node.hello`

#### Scenario: Sync 端点正常升级

- **WHEN** 客户端向 `/sync/v1` 发起携带 `acp-remote.sync.v1.json` subprotocol 且不请求压缩的 WebSocket 升级
- **THEN** 升级成功，连接进入认证前状态，等待 `auth.client_hello`

#### Scenario: Sync 配对端点可用

- **WHEN** 客户端向 `/sync/v1/pairing/claim` 或 `/sync/v1/pairing/status` 发起 POST JSON 请求
- **THEN** 请求进入 Sync 配对处理逻辑，不再返回 404，Node Link 的处理逻辑 MUST NOT 被触及

#### Scenario: Sync 路径明确不可用

- **WHEN** 客户端请求 Sync 路径之外的未注册路径，或以非升级方式请求 `/sync/v1`、以非 POST 方式请求 `/sync/v1/pairing/*`
- **THEN** 返回 404（非升级/方法不匹配时按接入层的协议错误处理），请求 MUST NOT 被静默路由到 Node Link 的处理逻辑，也不产生会话或认证状态

#### Scenario: 其余未知 path 仍 404

- **WHEN** 客户端请求任何未注册的 path（含 `/`、`/unknown`）
- **THEN** 返回 404，连接不进入任何协议处理逻辑

#### Scenario: 压缩或错误 subprotocol 被拒绝

- **WHEN** 升级请求协商了 `permessage-deflate`，或未声明该端点约定的 subprotocol
- **THEN** 升级被拒绝，连接不建立，不返回业务级错误消息

### Requirement: 跨协议不互相进入对方逻辑

四条 path 组各自只进入自己的协议处理逻辑，MUST NOT 被路由到另一侧的处理逻辑，也 MUST NOT 共享 wire DTO。

#### Scenario: 两侧不互相进入对方逻辑

- **WHEN** 客户端用 Node Link 的 subprotocol 升级 `/sync/v1`，或用 Sync 的 subprotocol 升级 `/node-link/v1`
- **THEN** 两次升级都被拒绝，连接不建立，任一协议 MUST NOT 处理另一协议的载荷

### Requirement: Host 与代理头边界

listener SHALL 校验请求 `Host`：`daemon.allowed_hosts` 为空时只接受与 `daemon.public_origin` 一致的 Host，非空时接受白名单内的 Host；不匹配的 Host MUST 被拒绝且返回明确错误，不得进入路由。`Forwarded`/`X-Forwarded-*` 头 MUST 只在连接对端地址属于 `daemon.trusted_proxies` 时被采信，其余来源的转发头一律忽略。Node Link 不依赖 Origin 校验（对端不是浏览器），但 Host 边界对 Node Link 同样生效（`CONFIG_REFERENCE.md` §1）。

#### Scenario: Host 不匹配被拒绝

- **WHEN** Daemon 配置 `public_origin = "https://owner.example.com"`，收到 `Host: evil.example.com` 的请求
- **THEN** 请求被拒绝并返回明确错误状态，不进入任何 path 的处理逻辑

#### Scenario: 不可信来源的转发头被忽略

- **WHEN** 直连客户端（不在 `trusted_proxies` 内）在请求中携带 `X-Forwarded-For` 头
- **THEN** 该头被忽略，限流与日志以对端真实连接地址为准

### Requirement: TLS 两种终止模式

Daemon SHALL 支持 `CONFIG_REFERENCE.md` §1 的两种 TLS 模式并全部接线。`daemon.tls.mode = "proxy"`（默认）时 Daemon 在监听地址上提供明文 HTTP/WSS，TLS 由同机可信反代终止，配合 `public_origin`/`allowed_hosts`/`trusted_proxies` 使用。`daemon.tls.mode = "direct"` 时 Daemon 自行终止 TLS：`cert_path`/`key_path` 必需，文件缺失、不可读、解析失败或权限不符（`SECURITY_DESIGN.md` §13.2）时 MUST 拒绝启动；私钥内容 MUST NOT 进入日志、错误消息或崩溃报告。除 `dev_mode.allow_plaintext` 且监听地址为 loopback 外，不存在明文 WSS 接入；证书变更经重启生效，不改变 Node identity。

#### Scenario: direct 模式正常终止 TLS

- **WHEN** 配置 `tls.mode = "direct"` 且 `cert_path`/`key_path` 指向权限合规的有效 PEM 文件，Daemon 启动
- **THEN** listener 只接受 TLS 握手成功的连接，明文连接无法建立

#### Scenario: direct 模式证书缺失即拒绝启动

- **WHEN** 配置 `tls.mode = "direct"` 但 `cert_path` 指向的文件不存在或权限不符
- **THEN** Daemon 拒绝启动并给出不含私钥内容的明确错误，不降级为明文监听

#### Scenario: 非 loopback 明文被拒绝

- **WHEN** 监听地址为非 loopback 且未配置 `direct` TLS（`proxy` 模式下反代不在本进程内），仅以明文对外暴露
- **THEN** 该形态只可能在用户显式选择 `proxy` 并自行保证同机反代时成立；Daemon 自身不得提供「非 loopback + 无 TLS 终止安排」以外的第三种明文形态，`dev_mode.allow_plaintext` 对非 loopback 地址不生效

### Requirement: 请求体与单消息上限

配对 HTTP 端点的请求体 SHALL 有上限（`NODE_LINK_PROTOCOL.md` §13.4：超限返回 413），只接受 `application/json`，不使用 Cookie、HTTP Basic Auth 或 URL query 承载配对凭据。WebSocket 单条消息超过协商的 `maxMessageBytes`（默认 1 MiB，`§2.5`）时 MUST 在分配大对象前以 close code 1009 拒绝；收到 binary frame MUST 返回 `link.error`（`nodelink.protocol.invalid_json`）并以 4400 关闭。

#### Scenario: 配对请求体超限

- **WHEN** 客户端向配对端点提交超过上限的请求体
- **THEN** 返回 413，不解析请求内容，不产生配对状态变化

#### Scenario: 超大 WebSocket 消息被拒绝

- **WHEN** 已认证连接发送超过 `maxMessageBytes` 的 text frame
- **THEN** 连接以 1009 关闭，Daemon 不为该消息分配完整缓冲区
