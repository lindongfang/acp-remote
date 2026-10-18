# cli-commands 增量

## MODIFIED Requirements

### Requirement: 配对三步流程与终端输出

`device pair` 与 `node pair` SHALL 固定为 begin → 轮询 status → confirm 三步：`device pair` 的 begin SHALL 打印网页端裸地址（`https://host/pair`，不含 fragment）、`配对码：XXXX-XXXX`、一个编码完整 `pairingUrl` 的终端二维码，以及提示“（移动端请扫码；网页端打开 /pair 后输入配对码）”；完整 fragment 地址 MUST NOT 作为文本打印，只能由二维码承载。`node pair` 的 begin SHALL 保持打印 `pairingUrl`。status 认领后 MUST 展示名称、指纹与过期时间（节点方向另含请求 grants）后才允许确认；confirm 提交的是用户确认的最终集合——设备方向不提交任何授权集合，节点方向提交用户确认的 grants 而非请求值。`node pair --mode access` 在本切片因底层方法返回 `local.unsupported` 而以明确错误结束。

#### Scenario: 交互式设备配对确认

- **WHEN** 用户执行 `device pair`，设备扫码或网页输入配对码认领，终端展示名称、指纹与过期时间后用户输入 `y`
- **THEN** CLI 调用 `device.pair.confirm` 并不提交任何授权集合，成功后输出 `deviceId`

#### Scenario: 终端同时显示 URL 与配对码

- **WHEN** `device pair` 的 begin 成功返回
- **THEN** 终端依次打印 `https://host/pair`（无 fragment）、`配对码：XXXX-XXXX`、一个二维码与指定提示；二维码内容 SHALL 等于完整 `pairingUrl`，完整 `#data=…` 文本 MUST NOT 出现在终端文本中

#### Scenario: 配对码被网页消耗后 CLI 行为不变

- **WHEN** 网页已经用配对码成功 lookup，CLI 仍在轮询 `device.pair.status`
- **THEN** CLI 继续展示认领后的名称、指纹与过期时间并等待 `y/N`，MUST NOT 因码已被消耗而提前结束或改变确认仪式

#### Scenario: 节点配对输出不变

- **WHEN** 用户执行 `node pair`
- **THEN** CLI 仍打印该节点配对的 `pairingUrl`，不打印配对码或终端二维码
