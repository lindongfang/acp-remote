# Proposal

## Why

桌面浏览器上复制或输入一条约 550–600 字符的配对 URL 不现实；这条长 URL 的 fragment 是给移动 App 扫码用的，不适合网页端。网页端需要一个固定路径 `/pair` + 配对码的入口，但不能因此复制一套配对协议，也不能削弱移动端二维码路径的带外主机公钥锚。

## 用户原始要求与已确认决策

- 网页端启动配对后地址固定为 `/pair`，用户在页面上输入配对码进入配对。
- 二维码继续服务后续的移动端 App；网页端能复用同一套协议就复用，不能复用才另做方案。
- 经确认：配对码使用 **8 位 Crockford 字母数字**（显示为 `XXXX-XXXX`），不另加校验符。
- 经确认：输入配对码后**仍保留终端 `y/N` 确认**。
- 经确认：**移动端只使用二维码**，不提供输码入口；node-link 配对不在本次扩展范围。
- 经确认：lookup 返回完整的 pairing secret；配对码是取回凭据，不是 HMAC 密钥。
- 经确认：lookup 成功即**消耗**配对码；不做 lookup 幂等重放。
- 经确认：所有过期判定以服务端为准；lookup 之前**不显示倒计时**，只显示静态提示“配对码 5 分钟内有效”。
- 经确认：新增错误码 `pairing.code_invalid`；lookup 与 claim 使用**各自独立**的每来源限流桶。
- 经确认：页面以 `prototypes/acp-remote-pwa.html` 的 S0–S12 状态为视觉与可观察行为参考。

## 硬约束

- 移动端二维码载荷与现有 `hostPublicKey` 带外强校验（`SYNC_PROTOCOL.md` §7.2/§7.3）MUST 保持不变。
- claim / status 的 wire、transcript、5 分钟有效期、每配对 5 次失败失效、单次 claim MUST 保持不变。
- 配对码 MUST NOT 作为 HMAC 密钥，也 MUST NOT 降低 pairing secret 的 256 bit 熵；它只用于一次性取回既有 secret。
- lookup 成功 MUST 原子地消耗配对码；同一码的第二次 lookup MUST 返回统一失败，不同请求不能重放。
- 配对确认 MUST 仍只能由本机入口完成；HTTP 端点 MUST NOT 获得确认能力。
- 配对码与 pairing secret MUST NOT 进入日志、URL query、Cookie、地址历史或任何明文持久化字段。
- 所有过期判定以服务端返回的 `expiresAt`/状态为准；客户端本地倒计时 MUST NOT 成为过期事实的来源。
- 新增端点 MUST 继续满足同源 `application/json`、四个安全响应头、固定限流，并用统一失败形状避免暴露配对是否存在。

## What Changes

- 新增 Sync HTTP `POST /sync/v1/pairing/lookup`：请求 `{protocolVersion, pairingCode}`，成功后返回该配对的 `pairingId`、`hostId`、`hostPublicKey`、`canonicalOrigin`、`pairingSecret`、`expiresAt`；成功即消耗配对码，响应沿用配对端点的四个安全响应头且 MUST NOT 缓存。
- 新增 `pairing.code_invalid` 错误码；lookup 的所有失败（码不存在、已过期、已认领、已消耗、来源不符）统一返回 `401` + 该码。
- 新增配对码派生规则：从 pairing secret 做域分离派生 40 bit，编码成 8 位 Crockford base32；Crockford 编解码在 `identity-auth` 内实现，不新增依赖。查找只在内存中的 active pairing 上反查，不新增持久字段；创建时若码碰撞，重新生成 secret/码直到唯一。
- `device.pair.begin` 的返回值新增 `pairingCode`；CLI 同时输出完整 `pairingUrl`（二维码用）与配对码。
- PWA `/pair`：无 fragment 时显示配对码输入（S0–S12 行为对齐桌面原型；S0 不显示数字倒计时，只显示静态有效期提示）；拿到 lookup 结果后复用现有 claim / status / approved 流程。有 fragment 时现有行为不变。移动端入口仍只接受 `#data=`。
- 网页入口的主机公钥来自 lookup 响应（TLS + canonical origin，首次信任），允许任意 canonical origin；移动端保持带外公钥。该分叉写入协议与安全文档。
- 把 daemon 的过期维护接到 `Authority::due_pairings`/`unrecoverable_after_restart`，启动路径调用 `reset_memory`，使 secret 与配对码映射按承诺的生命周期清除。
- 失败面新增固定呈现「配对码不对或已过期，请重新生成配对码」；lookup 响应丢失时也走该呈现，不做自动重试。

## Capabilities

### Modified Capabilities

- `sync-pairing-http`：新增 lookup 端点与 `pairing.code_invalid`；claim 的 secret 来源扩展为「二维码或配对码 lookup」；lookup/claim 各自限流。
- `pwa-web-client`：配对页接受「fragment 载荷」与「无 fragment 的配对码输入」两种入口；对齐桌面原型 S0–S12；新增配对码失败呈现与后端权威倒计时。
- `identity-pairing`：新增配对码派生、单次消耗、碰撞去重与内存查找；secret/码的生命周期与清理接上。
- `local-admin-methods`：`device.pair.begin` 返回值新增 `pairingCode`。
- `cli-commands`：`device pair` 同时打印 URL 与配对码；确认仪式不变。

### New Capabilities

无。

## 非目标

- 不改移动端 App 的入口形态，不让移动端输码。
- 不改 node-link 配对或 `/node-link/pair`。
- 不改 claim / status 的 wire 与 transcript。
- 不引入 PAKE / SPAKE2、lookup 幂等键或新的密码学协议。
- 不把配对码放进 URL path、query、Cookie 或本地持久化。
- 不改变配对有效期、失败次数、确认仪式或设备授权模型。

## 成功判据

- 桌面浏览器打开 `https://<origin>/pair`，输入终端显示的 8 位配对码后完成 lookup；终端仍展示名称与指纹并等待 `y`；确认后页面进入 approved 并完成设备认证。
- 配对码在成功 lookup 后立即失效；同一码第二次 lookup 返回 `401 pairing.code_invalid`，响应不区分配对是否存在。
- lookup 之前不显示倒计时；lookup/claim/status 的服务端结果始终是权威。
- 二维码 / fragment 路径行为与改动前逐条一致，现有 e2e 全绿。
- 码派生、单次消耗、碰撞去重、过期清理都有固定测试；`npm run verify` 全绿。
