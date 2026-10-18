# Spec Delta

## MODIFIED Requirements

### Requirement: 配对三步流程与终端输出

`device pair` 与 `node pair` SHALL 固定为 begin → 轮询 status → confirm 三步：设备 begin SHALL 打印 PWA 裸网页地址、`XXXX-XXXX` 配对码及编码新 `pairingUrl` 的实心黑白短二维码，并在二维码图案前打印 `二维码：` 标签；完整 fragment MUST NOT 作为文本输出或保存文件。节点 begin 仍打印原节点 `pairingUrl`；status 认领后 MUST 展示名称、指纹与过期时间（节点方向另含请求 grants）后才允许确认；confirm 提交的是用户确认的最终集合——设备方向不提交任何授权集合，节点方向提交用户确认的 grants 而非请求值。`node pair --mode access` 在本切片因底层方法返回 `local.unsupported` 而以明确错误结束。

#### Scenario: 交互式设备配对确认

- **WHEN** 用户执行 `device pair`，原生 App 扫码认领或桌面/手机 PWA 输入配对码认领，终端展示名称、指纹与过期时间后用户输入 `y`
- **THEN** CLI 调用 `device.pair.confirm` 并不提交任何授权集合，成功后输出 `deviceId`

#### Scenario: 终端同时显示 URL 与配对码

- **WHEN** `device.pair.begin` 成功返回
- **THEN** CLI 打印裸 `/pair` 网页地址、8 位配对码和编码完整新 pairingUrl 的二维码图案，并在图案前打印 `二维码：` 标签；完整 fragment 不作为文本打印，所有凭据不得写入 shell 历史或文件

#### Scenario: 配对码被网页消耗后 CLI 行为不变

- **WHEN** 网页已经用配对码成功 lookup，CLI 仍在轮询 `device.pair.status`
- **THEN** CLI 继续展示认领后的名称、指纹与过期时间并等待 `y/N`，MUST NOT 因码已被消耗而提前结束或改变确认仪式
