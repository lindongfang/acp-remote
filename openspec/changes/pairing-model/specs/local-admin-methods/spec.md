# Spec Delta

## MODIFIED Requirements

### Requirement: 设备配对与信任方法

`device.pair.begin` SHALL 创建一次性配对并返回 `pairingId`、`pairingUrl` 与 `expiresAt`（有效期固定 5 分钟，`expiresInMs` 只能收窄），不再返回任何授权集合，也不接受任何授权集合参数；`pairingUrl` 只经本通道返回，不得进入日志。`device.pair.status` 在 claim 前只返回 `state` 与 `expiresAt`，claim 后返回名称与指纹，不返回 SAS，也不返回任何授权集合。`device.pair.confirm` MUST 在持久状态提交成功后才返回，且不接受授权集合参数。`device.pair.reject` 终结配对。`device.list` 返回含 `pending`/`revoked` 的全部记录，且不返回授权集合；`device.revoke` MUST 在持久提交后立即关闭该设备的 active connection 才返回。

#### Scenario: 设备配对全流程

- **WHEN** 依次调用 `device.pair.begin`、设备端完成 claim 后轮询 `device.pair.status`、用户确认后调用 `device.pair.confirm`
- **THEN** begin 返回 URL 与过期时间；status 在 claim 前只暴露 `state`/`expiresAt`、claim 后暴露名称与指纹；confirm 提交成功后设备记录为 `active`，全程没有授权集合字段

#### Scenario: 过期与拒绝不创建信任

- **WHEN** 配对超过 5 分钟未确认，或对已过期/已拒绝的 `pairingId` 调用 `confirm`
- **THEN** 返回 `local.expired`，不创建任何设备信任记录

#### Scenario: 撤销立即生效

- **WHEN** 对 `active` 设备调用 `device.revoke`
- **THEN** 持久状态提交后该设备的 active connection 被关闭，方法才返回 `{ deviceId, revokedAt }`
