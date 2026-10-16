# Spec Delta

## MODIFIED Requirements

### Requirement: 配对安全确认仪式

`device pair` 与 `node pair` SHALL 固定为 begin → 轮询 status → confirm 三步：begin 打印 `pairingUrl`；status 认领后 MUST 展示名称、指纹与过期时间（节点方向另含请求 grants）后才允许确认；confirm 提交的是用户确认的最终集合——设备方向不提交任何授权集合，节点方向提交用户确认的 grants 而非请求值。非交互场景 MUST 显式传入 `--fingerprint` 且与 status 返回值逐字匹配才调 confirm，不匹配时以非零退出且不修改任何状态；MUST NOT 提供「无参数自动确认」开关，也 MUST NOT 再要求或接受短验证码参数。`pairingUrl` 与其中的 secret 只存在于内存与终端展示，不得写入 shell 历史、日志或文件。`node pair --mode access` 在本切片因底层方法返回 `local.unsupported` 而以明确错误结束。

#### Scenario: 交互式设备配对确认

- **WHEN** 用户执行 `device pair`，设备扫码认领，终端展示名称、指纹与过期时间后用户输入 `y`
- **THEN** CLI 调用 `device.pair.confirm` 并不提交任何授权集合，成功后输出 `deviceId`

#### Scenario: 非交互配对逐字校验

- **WHEN** 以 `--fingerprint` 执行配对且它与 `status` 返回值逐字一致
- **THEN** 调用 confirm；不匹配时以非零退出、不调用 confirm、不修改任何状态
