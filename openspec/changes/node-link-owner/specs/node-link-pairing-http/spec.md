# node-link-pairing-http Specification

## Purpose

定义 Owner 侧节点配对 HTTP 端点（`POST /node-link/v1/pairing/claim` 与 `POST /node-link/v1/pairing/status`）的可观察行为：claim 的原子检查、证明校验与幂等重试、status 的一律 200 语义与五种业务状态、§13.4 的 HTTP 状态码映射、安全响应头、配对限流与 pairing secret 生命周期，使 Access 节点能安全完成一次性配对而 Owner 不泄露任何校验差异。

## ADDED Requirements

### Requirement: claim 成功路径

`POST /node-link/v1/pairing/claim` SHALL 原子地依次检查：pairing 存在、未过期、仍为 `created`、请求 `endpoint` 的 host 与 Owner 配置一致、HMAC `proof`（`node-link-pairing-proof/v1` transcript，key 为 `pairingSecret`）正确。全部通过时 MUST 返回 201，body 含 `pairingRequestId`、`serverNonce`、`ownerProof`（Owner Node Identity Key 对 `node-link-pairing-owner-proof/v1` transcript 的 P1363 签名）、`status = "pending_confirmation"` 与 `expiresAt`，并将配对推进到 `claimed`/`pending_confirmation`；此后 MUST 不再接受第二个 claim。配对从 `pending_confirmation` 到建立信任记录、分配初始 `grant.*`，只能经本机用户的本地确认入口完成，HTTP 端点自身 MUST NOT 提供确认能力，且任何响应都不签发 bearer token。

#### Scenario: 合法 claim 进入待确认

- **WHEN** Access 节点持有效二维码 payload，向未过期、未 claim 的配对提交字段齐全且 HMAC 正确的 claim 请求
- **THEN** Owner 返回 201 与 `pending_confirmation`，响应中的 `ownerProof` 可用二维码内的 `ownerPublicKey` 验证通过，本地管理入口可查询到该待确认配对

#### Scenario: 重复 claim 被拒绝

- **WHEN** 配对已进入 `pending_confirmation`（或更后状态），另一 Access 节点（不同 `accessNodeId`/`clientNonce`）对同一 `pairingId` 提交 claim
- **THEN** 返回 409，已有配对记录不受影响

### Requirement: claim 失败语义与幂等重试

claim 的失败 SHALL 严格按 §13.4 映射：JSON 或字段 schema 无效 → 400；HMAC proof 无效 → 401 且响应 MUST NOT 泄露具体校验差异（不区分是哪一个字段或哪一种校验失败）；`endpoint` host 不允许 → 403；`pairingId` 不存在 → 404；已被 claim 或状态转换冲突 → 409；pairing 已过期或已 consumed → 410（仅 claim 端点）。网络响应丢失后，Access 用相同 claim 内容（相同 `accessNodeId` 与 `clientNonce`）重试时 MUST 返回原 pairing request 而不是创建第二条记录；claim 内容不同的重试 MUST 返回 409。错误 body SHALL 使用 `code`/`message`/`retryable`/`correlationId`/`details` 结构。

#### Scenario: proof 无效返回 401 且不泄露差异

- **WHEN** 客户端提交 HMAC 错误的 claim（其余字段合法）
- **THEN** 返回 401 与结构化错误 body，响应内容与「字段合法但 proof 错误」的其它失败情形不可区分，配对状态不变

#### Scenario: 相同内容的网络重试幂等

- **WHEN** Access 的 claim 请求已成功但响应丢失，随后以相同 `accessNodeId`/`clientNonce` 重试同一 claim
- **THEN** Owner 返回原 pairing request（相同 `pairingRequestId`），不创建第二条配对记录

#### Scenario: 过期配对返回 410

- **WHEN** 客户端对已超过 `expiresAt` 的配对提交 claim
- **THEN** 返回 410，配对不产生任何状态推进

### Requirement: status 查询语义

`POST /node-link/v1/pairing/status` SHALL 校验 `node-link-pairing-status/v1` 域的 HMAC `proof`（key 为 `pairingSecret`），proof 无效返回 401。校验通过后 MUST 一律返回 200，业务状态只由 body 的 `status` 表达：`pending_confirmation`/`approved`/`rejected`/`expired`/`consumed`；410 不得出现在该端点。`approved` 响应只返回该节点的非秘密元数据、`grant.*` 与 Owner identity（`grant.*` 取**已授予**集合），不签发任何凭据。每次轮询 SHALL 使用新 `requestNonce`；只有网络重试才复用原 nonce，此时 MUST 返回原响应。**secret 生命周期分支**（与「secret 最迟在 `expiresAt` 清除、`approved` 配对在首次 WSS 认证成功时提前清除」自洽）：本机已不持有该配对的 secret 时，终态配对（`rejected`/`expired`/`consumed`）MUST 仍按 200 报告其业务状态（终态判定不依赖对端输入），非终态配对 MUST 返回 401。

#### Scenario: 各业务状态都以 200 表达

- **WHEN** 配对分别处于 `pending_confirmation`、`approved`、`rejected`、`expired`、`consumed` 时，Access 以有效 proof 查询状态
- **THEN** 五种情形都返回 200，区别只在 body 的 `status` 字段

#### Scenario: status 的 proof 无效返回 401

- **WHEN** 本机仍持有该配对的 secret，客户端以错误 `pairingSecret` 计算的 proof 查询状态
- **THEN** 返回 401，不返回任何业务状态信息

#### Scenario: secret 已清除后的终态查询

- **WHEN** 配对已到达终态（`rejected`/`expired`/`consumed`）且本机已按生命周期规则清除其 secret，客户端查询状态（无论 proof 如何）
- **THEN** 返回 200 且 body 的 `status` 为该终态；非终态配对在 secret 缺失时一律返回 401

#### Scenario: 网络重试复用 nonce 返回原响应

- **WHEN** Access 的 status 响应丢失后以相同 `requestNonce` 重试
- **THEN** 返回与原响应一致的结果；正常使用新 nonce 的轮询不受重试记录影响

### Requirement: 安全响应头与凭据边界

所有配对 HTTP 响应 SHALL 携带 `Cache-Control: no-store`、`Pragma: no-cache`、`Referrer-Policy: no-referrer`、`X-Content-Type-Options: nosniff` 四个响应头。配对端点只接受 `application/json`；`pairingSecret`、完整二维码 payload 与 fragment MUST NOT 进入日志、错误消息或审计记录。`pairingSecret` 最迟在 `expiresAt` 清除；`approved` 配对在首次 WSS 认证成功时提前清除。

#### Scenario: 响应头齐全

- **WHEN** 客户端向任一配对端点发起请求（无论成功或失败）
- **THEN** 响应包含全部四个安全头，且 body 与日志中不出现 `pairingSecret` 或完整二维码 payload

#### Scenario: secret 按期清除

- **WHEN** 配对到达 `expiresAt`（或 `approved` 配对的首次 WSS 认证成功）
- **THEN** Owner 侧不再持有可用于该配对的 `pairingSecret`，事后的 claim/status 请求按过期语义处理

### Requirement: 配对限流

配对端点 SHALL 执行固定限流（`NODE_LINK_PROTOCOL.md` §2.5，不可配置）：claim 每 IP 10 次/分钟、status 每 `pairingId` 60 次/分钟；超限 MUST 返回 429。限流计数以对端真实连接地址为准（不可信来源的转发头不生效，见 `node-link-listener` 的 Host 与代理头边界）。

#### Scenario: claim 超限返回 429

- **WHEN** 同一 IP 在一分钟内发起第 11 次 claim 请求
- **THEN** 返回 429，该次请求不进入配对状态机

#### Scenario: status 超限返回 429

- **WHEN** 同一 `pairingId` 在一分钟内被查询第 61 次
- **THEN** 返回 429，正常轮询窗口恢复后查询继续可用
