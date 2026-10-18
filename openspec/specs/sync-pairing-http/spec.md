# sync-pairing-http Specification

## Purpose
定义 Sync 设备配对的 HTTPS 端点：一次性 claim 的原子校验、状态查询的业务状态承载、浏览器来源与 Host 的边界、响应安全头与固定限流，以及「配对确认只能由本机入口完成」的授权边界。它让未配对的浏览器设备能在不泄露秘密的前提下取得长期信任。

## Requirements

### Requirement: claim 端点的原子校验

`POST /sync/v1/pairing/claim` SHALL 原子地依次检查：该 pairing 存在、未过期、仍处于 `created`、请求的来源与 Host 与该配对登记时的绑定一致、以及 `pairing-proof` 域的 HMAC 校验正确。任一项不成立时 MUST NOT 写入任何状态。claim 成功后 SHALL 进入等待本机确认的状态，并 SHALL 只成功一次：已进入 `claimed`/等待确认或更后状态时，对同一 `pairingId` 的再次 claim MUST 被拒绝；内容完全相同的重复 claim SHALL 按幂等返回首次结果，内容不同的重复 claim MUST 返回冲突。认领载荷 MUST 携带对端公钥本身而非仅指纹。设备用于计算 HMAC 的配对秘密可以来自二维码载荷，也可以来自配对码 lookup 的成功响应；两条入口进入同一套 claim 校验，不得出现第二套证明规则。

#### Scenario: 首次 claim 成功

- **WHEN** 设备用二维码中的配对秘密对 claim 载荷计算 HMAC 并提交，来源与 Host 均与配对登记一致，且该配对仍未被认领
- **THEN** 服务端返回该配对的请求标识与等待确认状态，配对进入等待本机确认，且尚未创建设备信任

#### Scenario: 配对码入口的 claim 与二维码入口等价

- **WHEN** 设备先用正确配对码取得配对绑定与配对秘密，再用该秘密对 claim 载荷计算 HMAC 并提交
- **THEN** claim 通过同一套原子校验并得到与二维码入口相同的状态转换，MUST NOT 因为秘密来自 lookup 而放宽任何校验

#### Scenario: 重复 claim 的两种结果

- **WHEN** 同一配对收到第二个 claim，其内容与首次完全相同
- **THEN** 服务端返回首次结果而不创建第二条配对请求；若第二个 claim 的内容与首次不同，则 MUST 返回冲突

#### Scenario: 校验失败不写入状态

- **WHEN** claim 的 HMAC 无效、或来源与配对绑定不一致、或该配对已过期
- **THEN** 服务端返回对应错误且 MUST NOT 改变配对状态、MUST NOT 创建信任记录或写出任何授权

### Requirement: 配对确认只能由本机入口完成

配对从等待确认到创建信任记录的转换 SHALL 只能经本机用户的本地入口完成；`/sync/v1/pairing/*` 端点自身 MUST NOT 提供确认能力，任何响应 MUST NOT 签发 bearer token、会话凭据或刷新令牌。设备配对不再分配任何授权集合，因此设备 MUST NOT 通过 claim 申请、扩大或以任何方式影响授权；claim 载荷中出现授权集合字段 MUST 被拒绝。

#### Scenario: HTTP 端点不提供确认

- **WHEN** 设备在 claim 之后直接再次请求 claim 或状态端点，期望得到已批准的信任
- **THEN** 它只得到该配对当前的状态，信任记录 MUST NOT 被创建，也不返回任何凭据

#### Scenario: 设备无法扩大申请的 scope

- **WHEN** claim 载荷中出现任何授权集合字段（如 scope 或 grant）
- **THEN** claim 被拒绝，设备不能通过自报获得或扩大任何授权

### Requirement: status 端点以业务状态承载结果

`POST /sync/v1/pairing/status` SHALL 校验 `pairing-status` 域的 HMAC，proof 无效时 MUST 返回 401。校验通过后 MUST 一律返回 200，业务状态只由响应体的状态取值表达，取值 SHALL 限于等待确认、已批准、被拒绝、已过期、已被使用五种。已批准时响应 SHALL 只返回该设备的非秘密元数据与该节点的身份材料，MUST NOT 在其它状态下返回这些字段，也 MUST NOT 返回任何授权集合。每次轮询 MUST 使用新的请求随机数；只有网络重试才复用原随机数，此时 SHALL 返回原响应。

#### Scenario: 各业务状态都是 200

- **WHEN** 配对处于等待确认、已批准、被拒绝、已过期或已被使用之一，且 proof 有效
- **THEN** 响应 HTTP 状态为 200，业务状态由响应体表达；`410` MUST NOT 用于本端点

#### Scenario: 已批准时只返回必要字段

- **WHEN** 状态为已批准
- **THEN** 响应含设备标识、展示名与该节点身份材料；其余状态下这些字段 MUST NOT 出现，任何状态下都 MUST NOT 出现授权集合字段

#### Scenario: 本机已不持有配对秘密时仍可轮询终态

- **WHEN** 客户端已按协议清除内存中的配对秘密，但仍在轮询该配对的终态
- **THEN** 只要请求的 proof 有效，响应仍为 200 且业务状态照常表达，MUST NOT 因秘密已被清除而改变状态语义

### Requirement: 配对端点的响应头与请求边界

所有配对 HTTP 响应 SHALL 携带 `Cache-Control: no-store`、`Pragma: no-cache`、`Referrer-Policy: no-referrer` 与 `X-Content-Type-Options: nosniff` 四个响应头，且这些头 MUST 同样出现在接入层在调用处理器之前产生的响应上（请求体超限、方法不匹配、Host 校验失败）。端点 SHALL 只接受同源 `application/json` 请求，请求体 SHALL 有上限（超限返回 413），MUST NOT 使用 Cookie、HTTP Basic Auth 或 URL query 承载配对凭据。

#### Scenario: 预处理器响应也带安全头

- **WHEN** 请求体超过上限而被接入层以 413 拒绝，或方法不匹配而被以 405 拒绝
- **THEN** 该响应仍携带上述四个安全头

#### Scenario: 非 JSON 或跨来源请求被拒绝

- **WHEN** 请求的 `Content-Type` 不是 `application/json`，或请求来源不属于该端点的允许范围
- **THEN** 请求被拒绝，MUST NOT 进入配对校验逻辑

### Requirement: 配对端点的固定限流

配对端点 SHALL 执行固定限流：claim 按来源地址限制为每分钟 10 次，lookup 按来源地址限制为每分钟 10 次，status 按配对标限制为每分钟 60 次；超限 MUST 返回 429。lookup 与 claim MUST 使用各自独立的额度，不得因为一次成功或失败的 lookup 消耗 claim 的额度，反之亦然。限流计数 MUST 以对端真实连接地址为准，不可信来源的转发头 MUST NOT 影响计数。屡次 claim/status 证明校验失败 MUST 使该配对失效；lookup 的失败不指向某一配对，不增加该配对的证明失败计数。

#### Scenario: claim 超限返回 429

- **WHEN** 同一来源地址在一分钟内发起超过 10 次 claim
- **THEN** 超出的请求返回 429，且 MUST NOT 被当作未限流的正常 claim 处理

#### Scenario: lookup 与 claim 额度互不占用

- **WHEN** 同一来源地址在一分钟内先做一次成功 lookup，随后发起一次 claim
- **THEN** claim 仍在其独立的 10 次/分钟额度内，MUST NOT 因 lookup 已被使用而被 429

#### Scenario: 不可信转发头不影响计数

- **WHEN** 直连对端（不在可信代理清单内）携带伪造的转发来源头并反复发起 claim 或 lookup
- **THEN** 限流按其真实连接地址计算，伪造头 MUST NOT 让它绕过限制

#### Scenario: 屡次失败使配对失效

- **WHEN** 同一配对的证明校验失败累计达到 5 次
- **THEN** 该配对失效，后续 claim 与状态查询按失效处理，MUST NOT 仍可被认领

### Requirement: 配对码 lookup 端点

`POST /sync/v1/pairing/lookup` SHALL 用配对码换取一个仍可认领的配对的公开绑定与 pairing secret。请求体 SHALL 只含 `protocolVersion` 与 `pairingCode`；`pairingCode` SHALL 是 8 位 Crockford base32（大小写不敏感，可带一个 `-` 分隔符，服务端先归一化再比较）。成功响应 SHALL 返回 `protocolVersion`、`pairingId`、`hostId`、`hostPublicKey`、`canonicalOrigin`、`pairingSecret` 与 `expiresAt`。lookup MUST NOT 推进配对状态、MUST NOT 创建信任，也 MUST NOT 返回任何授权集合。

#### Scenario: 正确配对码取得配对绑定

- **WHEN** 浏览器在配对有效期内提交一个仍可认领配对的正确配对码
- **THEN** 服务端返回该配对的标识、主机身份材料、规范化来源、配对秘密与过期时间，且配对状态仍为可认领

#### Scenario: lookup 不推进配对状态

- **WHEN** 一次正确的 lookup 成功返回
- **THEN** 该配对仍可被 claim 一次，lookup 本身 MUST NOT 把它标记为已认领或已批准

#### Scenario: 主机公钥与二维码载荷一致

- **WHEN** 同一配对既有二维码载荷、又被配对码 lookup 成功返回
- **THEN** 两处 `hostPublicKey` 与 `hostId` 逐字节相同，MUST NOT 出现两套主机身份

### Requirement: lookup 成功即消耗配对码

lookup 成功 MUST 原子地消耗配对码：同一码的第二次 lookup MUST 统一失败，即使第一次的响应在网络上丢失。该消耗不改变配对状态，配对仍可用第一次返回的 secret 被 claim 一次。调用方 MUST NOT 通过重复 lookup 重放或恢复已消耗的码；恢复路径是重新创建一次配对。

#### Scenario: 成功 lookup 后码立即失效

- **WHEN** 同一配对码在成功 lookup 之后再次被提交
- **THEN** 服务端返回统一的 401 `pairing.code_invalid`，MUST NOT 再次返回 secret 或任何可区分状态

#### Scenario: 响应丢失后不能重放

- **WHEN** 第一次 lookup 已在服务端成功消耗配对码，但响应没有送达调用方
- **THEN** 调用方重试同一码仍得到统一的 401，MUST NOT 通过重试取回 secret；页面按固定失败文案结束

### Requirement: lookup 的失败与凭据边界

任何 lookup 失败——配对码格式不符、不存在、已过期、已被认领、已被使用、已被消耗、来源与 Host 不匹配——都 MUST 返回同一个 `401` 与同一个错误码 `pairing.code_invalid`，响应内容与形状 MUST NOT 区分这些情况。lookup 的请求与响应 MUST 遵守配对端点既有的同源 `application/json`、16 KiB 请求体上限、四个安全响应头与 `no-store` 规则；`pairingCode` 与 `pairingSecret` MUST NOT 进入日志、URL query、Cookie 或任何明文持久化字段。

#### Scenario: 错误配对码不泄露存在性

- **WHEN** 分别提交「不存在的配对码」「存在但已消耗的配对码」以及「格式合法的随机配对码」
- **THEN** 三者都返回相同的 401 与 `pairing.code_invalid`，状态码、响应体形状与错误码不可区分

#### Scenario: 过期或已认领的配对不可 lookup

- **WHEN** 配对码对应的配对已经过期、已被认领或已被使用
- **THEN** lookup 返回统一的 401，MUST NOT 返回 secret、主机材料或任何可区分状态

#### Scenario: 来源不一致不返回任何材料

- **WHEN** 请求的实际来源或 Host 与该配对登记时的绑定不一致
- **THEN** lookup 返回统一的 401，MUST NOT 进入配对码查找或返回任何配对材料

#### Scenario: 响应不可缓存且不记录秘密

- **WHEN** lookup 成功或失败
- **THEN** 响应都带四个安全响应头与 `no-store`，且日志中不出现 `pairingCode` 或 `pairingSecret`
