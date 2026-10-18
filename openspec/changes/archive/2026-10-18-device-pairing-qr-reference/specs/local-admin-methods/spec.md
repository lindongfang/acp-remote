# Spec Delta

## MODIFIED Requirements

### Requirement: 设备配对建立与配对码

`device.pair.begin` SHALL 创建一次性配对并返回 `pairingId`、`pairingUrl`、`pairingCode` 与 `expiresAt`（有效期固定 5 分钟，`expiresInMs` 只能收窄），不再返回任何授权集合，也不接受任何授权集合参数；`pairingUrl` SHALL 是仅包含版本、32 字节独立扫码取回凭据与32 字节完整主机公钥指纹的新设备短二维码地址，不再生成旧设备完整资料格式；它只经本次管理响应返回，不得进入日志、后续状态查询或文件。`pairingCode` SHALL 是网页入口使用的 8 位 Crockford 配对码，只用于取回该配对的绑定与配对秘密；成功 lookup 一次后即被消耗，尚未认领时 `pairingUrl` 中的扫码取回凭据仍可使用，不因网页配对码消耗而失效。`pairingCode` MUST NOT 替代 `pairingUrl`，MUST NOT 被持久化或写入日志。

#### Scenario: 配对码只返回一次且不落库

- **WHEN** `device.pair.begin` 返回 `pairingCode`
- **THEN** 该配对码只出现在本次管理响应中，后续 `device.pair.status`/`device.list` MUST NOT 再返回它，日志与持久化也 MUST NOT 出现其明文

#### Scenario: 配对码被消耗后管理面不再可用它

- **WHEN** 配对码已通过 lookup 成功取回配对绑定
- **THEN** 再次用同一配对码调用 lookup MUST 失败；`device.pair.status`/`confirm` 仍按该配对的真实状态返回，不因码被消耗而改变
