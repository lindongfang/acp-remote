# Spec Delta

## MODIFIED Requirements

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
