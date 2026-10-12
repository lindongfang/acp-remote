<!-- 按 proposal 的能力清单写增量行为与场景；技术方案见 design.md。
-->

## Purpose

定义 Sync 设备配对的 HTTPS 端点：一次性 claim 的原子校验、状态查询的业务状态承载、浏览器来源与 Host 的边界、响应安全头与固定限流，以及「配对确认只能由本机入口完成」的授权边界。它让未配对的浏览器设备能在不泄露秘密的前提下取得长期信任。

## ADDED Requirements

### Requirement: claim 端点的原子校验

`POST /sync/v1/pairing/claim` SHALL 原子地依次检查：该 pairing 存在、未过期、仍处于 `created`、请求的来源与 Host 与该配对登记时的绑定一致、以及 `pairing-proof` 域的 HMAC 校验正确。任一项不成立时 MUST NOT 写入任何状态。claim 成功后 SHALL 进入等待本机确认的状态，并 SHALL 只成功一次：已进入 `claimed`/等待确认或更后状态时，对同一 `pairingId` 的再次 claim MUST 被拒绝；内容完全相同的重复 claim SHALL 按幂等返回首次结果，内容不同的重复 claim MUST 返回冲突。认领载荷 MUST 携带对端公钥本身而非仅指纹。

#### Scenario: 首次 claim 成功

- **WHEN** 设备用二维码中的配对秘密对 claim 载荷计算 HMAC 并提交，来源与 Host 均与配对登记一致，且该配对仍未被认领
- **THEN** 服务端返回该配对的请求标识与等待确认状态，配对进入等待本机确认，且尚未创建设备信任

#### Scenario: 重复 claim 的两种结果

- **WHEN** 同一配对收到第二个 claim，其内容与首次完全相同
- **THEN** 服务端返回首次结果而不创建第二条配对请求；若第二个 claim 的内容与首次不同，则 MUST 返回冲突

#### Scenario: 校验失败不写入状态

- **WHEN** claim 的 HMAC 无效、或来源与配对绑定不一致、或该配对已过期
- **THEN** 服务端返回对应错误且 MUST NOT 改变配对状态、MUST NOT 创建信任记录或写出任何授权

### Requirement: 配对确认只能由本机入口完成

配对从等待确认到创建信任记录并分配初始 scope 的转换 SHALL 只能经本机用户的本地入口完成；`/sync/v1/pairing/*` 端点自身 MUST NOT 提供确认能力，任何响应 MUST NOT 签发 bearer token、会话凭据或刷新令牌。配对时决定的初始 scope MUST NOT 由设备在 claim 中申请或扩大。

#### Scenario: HTTP 端点不提供确认

- **WHEN** 设备在 claim 之后直接再次请求 claim 或状态端点，期望得到已批准的信任
- **THEN** 它只得到该配对当前的状态，信任记录 MUST NOT 被创建，也不返回任何凭据

#### Scenario: 设备无法扩大申请的 scope

- **WHEN** claim 载荷中出现的 scope 超出本机为该配对登记的请求集合
- **THEN** claim 被拒绝，边界 MUST NOT 按设备自报值放宽

### Requirement: status 端点以业务状态承载结果

`POST /sync/v1/pairing/status` SHALL 校验 `pairing-status` 域的 HMAC，proof 无效时 MUST 返回 401。校验通过后 MUST 一律返回 200，业务状态只由响应体的状态取值表达，取值 SHALL 限于等待确认、已批准、被拒绝、已过期、已被使用五种。已批准时响应 SHALL 只返回该设备的非秘密元数据、授予的 scope 与该节点的身份材料，MUST NOT 在其它状态下返回这些字段。每次轮询 MUST 使用新的请求随机数；只有网络重试才复用原随机数，此时 SHALL 返回原响应。

#### Scenario: 各业务状态都是 200

- **WHEN** 配对处于等待确认、已批准、被拒绝、已过期或已被使用之一，且 proof 有效
- **THEN** 响应 HTTP 状态为 200，业务状态由响应体表达；`410` MUST NOT 用于本端点

#### Scenario: 已批准时只返回必要字段

- **WHEN** 状态为已批准
- **THEN** 响应含设备标识、展示名、授予的 scope 与该节点身份材料；其余状态下这些字段 MUST NOT 出现

#### Scenario: 本机已不持有配对秘密时仍可轮询终态

- **WHEN** 该配对的秘密已按生命周期清除（进程重启或已批准），设备轮询一个终态配对
- **THEN** 服务端仍按 200 返回该终态，而非以认证失败或 404 回应

### Requirement: 配对端点的响应头与请求边界

所有配对 HTTP 响应 SHALL 携带 `Cache-Control: no-store`、`Pragma: no-cache`、`Referrer-Policy: no-referrer` 与 `X-Content-Type-Options: nosniff` 四个响应头，且这些头 MUST 同样出现在接入层在调用处理器之前产生的响应上（请求体超限、方法不匹配、Host 校验失败）。端点 SHALL 只接受同源 `application/json` 请求，请求体 SHALL 有上限（超限返回 413），MUST NOT 使用 Cookie、HTTP Basic Auth 或 URL query 承载配对凭据。

#### Scenario: 预处理器响应也带安全头

- **WHEN** 请求体超过上限而被接入层以 413 拒绝，或方法不匹配而被以 405 拒绝
- **THEN** 该响应仍携带上述四个安全头

#### Scenario: 非 JSON 或跨来源请求被拒绝

- **WHEN** 请求的 `Content-Type` 不是 `application/json`，或请求来源不属于该端点的允许范围
- **THEN** 请求被拒绝，MUST NOT 进入配对校验逻辑

### Requirement: 配对端点的固定限流

配对端点 SHALL 执行固定限流：claim 按来源地址限制为每分钟 10 次、status 按配对标限制为每分钟 60 次；超限 MUST 返回 429。限流计数 MUST 以对端真实连接地址为准，不可信来源的转发头 MUST NOT 影响计数。屡次校验失败 MUST 使该配对失效。

#### Scenario: claim 超限返回 429

- **WHEN** 同一来源地址在一分钟内发起超过 10 次 claim
- **THEN** 超出的请求返回 429，且 MUST NOT 被当作未限流的正常 claim 处理

#### Scenario: 不可信转发头不影响计数

- **WHEN** 直连对端（不在可信代理清单内）携带伪造的转发来源头并反复发起 claim
- **THEN** 限流按其真实连接地址计算，伪造头 MUST NOT 让它绕过限制

#### Scenario: 屡次失败使配对失效

- **WHEN** 同一配对的证明校验失败累计达到 5 次
- **THEN** 该配对失效，后续 claim 与状态查询按失效处理，MUST NOT 仍可被认领
