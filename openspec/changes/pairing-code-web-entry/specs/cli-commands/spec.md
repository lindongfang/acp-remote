# cli-commands 增量

## MODIFIED Requirements

### Requirement: 配对安全确认仪式

`device pair` 与 `node pair` SHALL 固定为 begin → 轮询 status → confirm 三步：begin 打印 `pairingUrl`，并在 `device pair` 方向同时打印 `pairingCode`（以 `XXXX-XXXX` 可读形式）；status 认领后 MUST 展示名称、指纹与过期时间（节点方向另含请求 grants）后才允许确认；confirm 提交的是用户确认的最终集合——设备方向不提交任何授权集合，节点方向提交用户确认的 grants 而非请求值。非交互场景 MUST 显式传入 `--fingerprint` 且与 status 返回值逐字匹配才调 confirm，不匹配时以非零退出且不修改任何状态；MUST NOT 提供「无参数自动确认」开关，也 MUST NOT 再要求或接受短验证码参数。`pairingUrl`、`pairingCode` 与其中的 secret 只存在于内存与终端展示，不得写入 shell 历史、日志或文件。`node pair --mode access` 在本切片因底层方法返回 `local.unsupported` 而以明确错误结束。

#### Scenario: 交互式设备配对确认

- **WHEN** 用户执行 `device pair`，设备扫码认领或网页输入配对码认领，终端展示名称、指纹与过期时间后用户输入 `y`
- **THEN** CLI 调用 `device.pair.confirm` 并不提交任何授权集合，成功后输出 `deviceId`

#### Scenario: 终端同时显示 URL 与配对码

- **WHEN** `device.pair.begin` 成功返回
- **THEN** CLI 既打印完整 `pairingUrl`（供二维码/移动端使用），也打印 8 位配对码（供网页 `/pair` 输入），两种形式都不得写入 shell 历史或文件

#### Scenario: 配对码被网页消耗后 CLI 行为不变

- **WHEN** 网页已经用配对码成功 lookup，CLI 仍在轮询 `device.pair.status`
- **THEN** CLI 继续展示认领后的名称、指纹与过期时间并等待 `y/N`，MUST NOT 因码已被消耗而提前结束或改变确认仪式

#### Scenario: 非交互配对逐字校验

- **WHEN** 以 `--fingerprint` 执行配对且它与 `status` 返回值逐字一致
- **THEN** 调用 confirm；不匹配时以非零退出、不调用 confirm、不修改任何状态
