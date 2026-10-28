# 网页配对使用一次性配对码，移动端继续使用带外二维码载荷

## Status

accepted（2026-10-18）

> **部分被取代**（2026-10-18，[ADR-0012](./0012-device-qr-reference-format.md)）：本 ADR 关于网页入口使用一次性配对码、`POST /sync/v1/pairing/lookup` 与网页 TOFU 信任链的决策**继续有效**；其中“移动端继续使用完整 `#data=` 二维码载荷、网页与二维码两个设备入口并存”的**设备二维码决定**已被 ADR-0012 取代。设备二维码改为 `#q2=` 短引用格式，PWA 只使用配对码，旧 `#data=` 明确退出。

## Context

配对载荷（`https://<origin>/pair#data=…`）同时携带主机公钥与一次性 pairing secret，是移动端扫码建立首次信任的带外锚。桌面浏览器复制这样一条约 550–600 字符的 URL 不现实；但网页端又不能因此复制一套 claim/status 协议，或把 256 bit 的配对秘密降成一个人手输入的短码。

## Decision

网页端使用固定 `/pair` + 8 位 Crockford 配对码：配对码由 pairing secret 做域分离派生（40 bit），只作为**一次性取回凭据**；`POST /sync/v1/pairing/lookup` 成功返回完整配对绑定与 256 bit secret，之后的 claim/status/transcript 与二维码入口完全共用。配对码在成功 lookup 后立即消耗，第二次 lookup 统一返回 `pairing.code_invalid`。

移动端继续只使用二维码 fragment 载荷，保留带外主机公钥强校验。网页入口没有带外载荷，其主机公钥来自同源 lookup 响应，信任链是 TLS + canonical origin 的首次信任（TOFU）；允许任意 canonical origin（含 LAN/Tailscale/loopback），输入页显示当前 origin 与 HTTPS 标识。40 bit 的码只在 lookup 与 claim **各自独立**的每来源 10 次/分钟限流下才可接受，lookup 失败不增加配对的证明失败计数。终端 `y/N` 确认仪式保持不变。完整安全边界见 `docs/SECURITY_DESIGN.md` §5.1，wire 见 `docs/SYNC_PROTOCOL.md` §7.1/§7.5。

## Considered Options

- **压缩 fragment**：保留全部安全属性，但手输仍然偏长，且没有解决“网页端需要输入”的产品诉求。
- **SAS 指纹比对**：能检测同源错路由，但重新引入已被 `pairing-model` 删除的人眼比对机制；本次选择在网页端接受 TOFU，把强锚留给移动端。
- **6 位纯数字**：20 bit 在 lookup 的在线猜测面下不足；8 位 Crockford 约 40 bit，配合每来源限流才可接受。
- **独立生成配对码并持久化摘要**：需要第二份状态与清理路径；从 secret 派生可以让码与 secret 同生同灭。

## Consequences

- 网页端丢失“同源代理错路由到另一台 daemon”的检测能力；这属于部署异常，终端侧会停留在等待认领，可观察。移动端不受影响。
- lookup 成功即消耗码；如果响应在网络上丢失，用户必须重新 `device pair`。页面固定提示“配对码不对或已过期，请重新生成配对码”，不做自动重试。
- 配对码输入页在 lookup 之前不显示倒计时，只显示静态“配对码 5 分钟内有效。”；lookup 成功后显示服务端 `expiresAt`。所有过期判定以服务端为准。页面行为与存储边界见 `docs/FRONTEND_DESIGN.md` §4.1/§4.5/§9。
- 配对码映射只存在于 `identity-auth` 的内存中，并随 secret 在过期、批准后首次认证、拒绝后到期或重启时清除；不新增 SQLite 字段。

修订（2026-10-18）：用户点击「继续配对」视为显式重新配对，允许本次绑定公钥不同于同源旧记录。仅在电脑批准且主机证明、批准响应身份校验通过后替换旧记录；进入页面、等待确认或失败不修改旧记录。正常 WSS 重连仍使用已保存公钥，身份变化仍阻断。
