## ADDED Requirements

### Requirement: 配对入口校验来源与安全上下文

配对入口 SHALL 在接受二维码内容之前校验两件事：二维码声明的 `canonicalOrigin` 与本页面的实际来源一致，且本页面处于**安全上下文**（`isSecureContext`）。任一不满足时 MUST 以可区分的方式失败，且 MUST NOT 发出任何配对请求（claim/status）。

#### Scenario: 来源不符时明确失败

- **WHEN** 用户把「另一台主机」的配对二维码内容粘进本机配对页（`canonicalOrigin` 与当前来源不同）
- **THEN** 页面 SHALL 立即进入可区分的失败态、给出可读原因，且**一个配对请求都不发**（claim/status 计数为 0）

#### Scenario: 非安全上下文下拒绝配对

- **WHEN** 页面运行在不具备安全上下文的来源下（例如明文 http 的非 loopback 来源）
- **THEN** 配对入口 SHALL 明确拒绝并说明原因（不是等到 WebCrypto 报错或静默降级），MUST NOT 尝试用可导出形式的密钥替代
