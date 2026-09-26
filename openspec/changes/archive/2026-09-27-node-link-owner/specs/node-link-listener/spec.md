# node-link-listener Specification

## Purpose

定义 Daemon 共享网络接入面的可观察行为：`daemon.listen` 的绑定与失败关闭、非 loopback 监听的显式告警、按 path 的路由（Node Link WSS 与配对 HTTP 可用、Sync 与其余路径明确拒绝）、Host 与代理头边界校验，以及 TLS `proxy`/`direct` 两种终止模式，使 Node Link 端点只在受信 TLS 边界内可达。

## ADDED Requirements

### Requirement: 共享 listener 绑定与失败关闭

Daemon 启动时 SHALL 在恢复管理记录之后、开放业务接入之前绑定 `daemon.listen`（默认 `127.0.0.1:8765`）上的共享 HTTP/WSS listener，Node Link 的 WSS 端点与配对 HTTP 端点共用该 listener 并按 path 路由。绑定失败（端口占用、地址非法、权限不足）时 Daemon MUST 拒绝启动并给出明确错误，不得降级为无网络接入运行，也不得只绑定部分端点。监听地址为非 loopback（如 `0.0.0.0`/`[::]`）时 MUST 是显式配置且在启动输出中给出告警。`daemon.status` 的 `listen` 字段 SHALL 返回实际绑定的监听地址清单。

#### Scenario: 默认 loopback 启动

- **WHEN** 配置未显式设置 `daemon.listen`，Daemon 启动成功
- **THEN** listener 绑定在 `127.0.0.1:8765`，`daemon.status` 的 `listen` 返回该地址，配对与 Node Link 端点在该地址上可达

#### Scenario: 绑定失败即拒绝启动

- **WHEN** `daemon.listen` 配置的地址已被其他进程占用
- **THEN** Daemon 拒绝启动并以明确错误退出，本地管理通道与业务接入均不开放，不残留半初始化的监听

#### Scenario: 非 loopback 监听告警

- **WHEN** 用户显式配置 `daemon.listen = "0.0.0.0:8765"` 并启动 Daemon
- **THEN** Daemon 正常启动，且启动输出中包含「监听非 loopback 地址」的告警；未显式配置时 Daemon 不得自行绑定非 loopback 地址

### Requirement: 按 path 的路由与 WebSocket 升级规则

listener SHALL 只按以下 path 提供服务：`/node-link/v1` 接受 WebSocket 升级（subprotocol 固定为 `acp-remote.nodelink.v1.json`）；`/node-link/v1/pairing/claim` 与 `/node-link/v1/pairing/status` 接受配对 HTTP 请求。`/sync/v1` 与 `/sync/v1/pairing/*` 在 `server::sync` 缺席期间 MUST 返回明确 404，不得被静默路由到 Node Link 处理逻辑；其余未知 path 一律 404。对 `/node-link/v1` 的非升级请求 MUST 以协议错误拒绝；协商到 `permessage-deflate` 压缩或缺少约定 subprotocol 的升级请求 MUST 被拒绝（`NODE_LINK_PROTOCOL.md` §2.1）。

#### Scenario: Node Link 端点正常升级

- **WHEN** 客户端向 `/node-link/v1` 发起携带 `acp-remote.nodelink.v1.json` subprotocol 且不请求压缩的 WebSocket 升级
- **THEN** 升级成功，连接进入认证前状态，等待 `node.hello`

#### Scenario: Sync 路径明确不可用

- **WHEN** 客户端请求 `/sync/v1` 或 `/sync/v1/pairing/claim`（任意方法）
- **THEN** 返回 404，连接不进入任何协议处理逻辑，不产生会话或认证状态

#### Scenario: 压缩或错误 subprotocol 被拒绝

- **WHEN** 升级请求协商了 `permessage-deflate`，或未声明 `acp-remote.nodelink.v1.json` subprotocol
- **THEN** 升级被拒绝，连接不建立，不返回业务级错误消息

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
