<!-- 按 proposal 的能力清单写增量行为与场景；技术方案见 design.md。
     本增量只改「按 path 的路由与 WebSocket 升级规则」一条：Sync 端点已上线，
     该要求里「/sync/v1 必须 404」的口径与它的场景随之失效。
-->

## MODIFIED Requirements

### Requirement: 按 path 的路由与 WebSocket 升级规则

listener SHALL 按以下 path 提供服务：`/node-link/v1` 接受 WebSocket 升级（subprotocol 固定为 `acp-remote.nodelink.v1.json`）；`/node-link/v1/pairing/claim` 与 `/node-link/v1/pairing/status` 接受 Node Link 的配对 HTTP 请求；`/sync/v1` 接受 WebSocket 升级（subprotocol 固定为 `acp-remote.sync.v1.json`）；`/sync/v1/pairing/claim` 与 `/sync/v1/pairing/status` 接受 Sync 的配对 HTTP 请求。四条 path 组各自只进入自己的协议处理逻辑，MUST NOT 被路由到另一侧的处理逻辑，也 MUST NOT 共享 wire DTO。其余未知 path 一律 404。对 WebSocket 端点的非升级请求 MUST 以协议错误拒绝；协商到 `permessage-deflate` 压缩或缺少各自约定 subprotocol 的升级请求 MUST 被拒绝（`NODE_LINK_PROTOCOL.md` §2.1、`SYNC_PROTOCOL.md` §3.1）。

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

#### Scenario: 两侧不互相进入对方逻辑

- **WHEN** 客户端用 Node Link 的 subprotocol 升级 `/sync/v1`，或用 Sync 的 subprotocol 升级 `/node-link/v1`
- **THEN** 两次升级都被拒绝，连接不建立，任一协议 MUST NOT 处理另一协议的载荷

#### Scenario: 其余未知 path 仍 404

- **WHEN** 客户端请求任何未注册的 path（含 `/`、`/unknown`）
- **THEN** 返回 404，连接不进入任何协议处理逻辑

#### Scenario: 压缩或错误 subprotocol 被拒绝

- **WHEN** 升级请求协商了 `permessage-deflate`，或未声明该端点约定的 subprotocol
- **THEN** 升级被拒绝，连接不建立，不返回业务级错误消息
