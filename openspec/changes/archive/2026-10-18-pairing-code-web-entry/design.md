# Design

## Context

现状（`SYNC_PROTOCOL.md` §7.1–§7.4、`IDENTITY_AND_AUTH_CONTRACT.md` §4、`pwa-web-client` 的配对要求）：

- 配对载荷是 `https://<origin>/pair#data=<base64url(JSON)>`，JSON 含 `hostId`、`hostPublicKey`、`canonicalOrigin`、`pairingId`、`pairingSecret`、`expiresAt`。
- 设备端用 fragment 里的 32 字节 secret 计算 claim/status 的 HMAC；用 fragment 里的主机公钥验证 claim 响应的 `hostProof`，并在 approved 响应上要求公钥逐字节相同。
- secret 只在内存中存在，持久化只有摘要；5 分钟有效期、每配对 5 次失败失效、单次 claim。
- 前端配对页只从 fragment 读载荷，无片段时显示「没有配对信息」。
- `device.pair.begin` 返回 `pairingId`/`pairingUrl`/`expiresAt`；CLI 打印 URL 并在认领后等待 `y/N`。

侦察事实（本次实现必须遵守）：

- `identity-auth` 的 secret 在私有 `State.secrets: BTreeMap<PairingId, PairingMaterial>` 中，没有遍历 API，也没有按码查找；反查必须新增在 `identity-auth` 内。
- `begin_pairing` 已经生成 `PairingSecret` 并通过 `PairingDraft.secret` 交给调用方；`device.pair.begin` 已拿到该 secret，可直接派生/返回配对码。
- claim 严格按 `pairingId` 查；新增 lookup 是唯一从码进入的路径。
- `due_pairings`、`unrecoverable_after_restart`、`reset_memory` 目前在 daemon/server 没有生产调用者；`expire_pairings` 只清持久记录，不清内存 secret。本次要补上这条接线。

桌面原型 `prototypes/acp-remote-pwa.html` 已画出 S0–S12 共 13 个状态，是网页入口的视觉与可观察行为参考：

- S0 输入配对码（8 格、粘贴 `XXXX-XXXX`、显示 origin 与 HTTPS）。
- S1 lookup、S2 claim、S3 等待电脑确认、S4 已批准进目录。
- S5 被拒绝、S6 配对码不对或已过期、S7 地址过期、S8 已使用、S9 无法识别、S10 来源不一致、S11 需要 HTTPS、S12 请求未能完成。

## Goals / Non-Goals

**Goals:**

- 网页端有一个固定 `/pair` + 8 位配对码的入口，且与二维码入口共用 claim/status/transcript。
- 移动端二维码载荷与主机公钥强校验逐条不变。
- 配对码不降低 pairing secret 的熵，不引入新的持久化秘密，单次 lookup 后立即失效。
- 失败面统一、限流可测、过期清理按承诺生效、可回滚。

**Non-Goals:**

- 不实现 PAKE/SPAKE2；不做移动端输码；不做 node-link 配对码；不做 lookup 幂等重放。
- 不改 claim/status wire、transcript、绑定校验、确认仪式。
- 不把配对码做成新的授权维度或设备身份。

## Decisions

### D1：两种入口，一套状态机，原型是呈现参考

`/pair` 仍是唯一页面。有 `data=` fragment 时走现有 `startPairingFromAddress`；没有 fragment 时进入 `awaiting_code`（对应原型 S0），提交后进入 `looking_up`（S1），lookup 成功进入 `claiming`（S2），之后复用现有 `pending_confirmation`/`approved`/终态。`/node-link/pair` 不提供配对码，仍按无载荷处理。

理由：`pairing-page-open-url` 的「单一入口、单一目的」原则保留；新增的只是载荷获取方式。原型的 S0–S12 直接映射到 `PairingPageModel` 的阶段与失败类别，避免第二套呈现。

替代方案：单独做一个 `/pair/code` 页面。否决：会产生第二套路由、第二套失败呈现与第二份 e2e。

### D2：配对码是「取回 secret 的凭据」，不是 HMAC 密钥

`pairingCode` 由 secret 域分离派生：

```text
digest = HMAC-SHA256(key = pairingSecret, msg = "acp-remote/pairing-code/v1")
code   = CrockfordBase32(digest[0..5])        // 40 bit, 8 chars
```

lookup 成功后返回**完整的 256 bit `pairingSecret`**，之后的 claim/status/transcript 完全不变。

理由：如果直接把 8 位码当 HMAC 密钥，pairing secret 的熵会从 256 bit 降到 40 bit，二维码路径也会一起变弱；而 40 bit 的 HMAC 密钥在一次证明泄露后可被离线枚举。让码只负责在线取回 secret，离线攻击面仍是 256 bit。

编码：40 bit 恰好是 8 个 5-bit 组，无取模偏差。Crockford 字母表为 `0123456789ABCDEFGHJKMNPQRSTVWXYZ`（不含 `I`/`L`/`O`/`U`）；输入归一化把 `I`/`L` 映射为 `1`、`O` 映射为 `0`，大小写不敏感，允许一个 `-` 分隔符。不另加校验符（8 位固定）。

替代方案：独立生成 8 位码并单独存 digest。否决：要新增持久字段或第二份内存映射；派生方案零迁移，且码与 secret 天然同生同灭。

### D3：码派生与查找放在 `identity-auth`，只在内存，单次消耗

- `identity-auth` 新增纯函数 `derive_pairing_code(secret) -> PairingCode`（域分离、40 bit、Crockford）。
- `Authority` 在创建配对时把 `code -> PairingId` 放进与 secret 同一生命周期的内存映射；查找使用常数时间比较。
- 一次成功查找**原子地移除**该码；第二次查找统一失败（`pairing.code_invalid`）。码被消耗不改变配对状态，secret 仍可用于 claim。
- 若创建的码与另一个活跃配对的码碰撞，重新生成 secret（从而得到新码）直到唯一；不向用户报错。
- `due_pairings`、`unrecoverable_after_restart`、`reset_memory` 以及批准后清除 secret 的路径都同步移除码映射。
- 不新增 SQLite 字段、不新增 migration。

理由：secret 本来就在 `Authority` 内存里；把码映射放在同一处，重启即清空，符合「不能凭摘要恢复 secret」。

### D4：lookup 端点的语义与失败

`POST /sync/v1/pairing/lookup`：

```json
{ "protocolVersion": 1, "pairingCode": "ABCD-EFGH" }
```

成功 200：

```json
{
  "protocolVersion": 1,
  "pairingId": "…",
  "hostId": "…",
  "hostPublicKey": "<base64url-65>",
  "canonicalOrigin": "https://…",
  "pairingSecret": "<base64url-32>",
  "expiresAt": "2026-10-18T…Z"
}
```

- 只对 `created`、未过期、来源/Host 与登记绑定一致的配对成功；lookup 不推进状态，但成功即消耗码。
- 所有失败统一 `401` + **`pairing.code_invalid`**（新增错误码），不区分配对不存在、过期、已认领、已使用、已消耗。
- 与 claim **各自独立**的每来源 10 次/分钟额度；status 仍按配对标 60 次/分钟。lookup 失败不增加配对的证明失败计数。
- body 16 KiB；四个安全响应头；`no-store`；处理路径不得记录请求/响应体；错误消息不得回显码。
- 码输入先归一化（去掉一个 `-`、大写、`I/L→1`、`O→0`），再比较。

**不做 lookup 幂等键**：成功 lookup 即消耗码，响应丢失时用户重新输入同一码会得到统一失败。页面固定提示「配对码不对或已过期，请重新生成配对码」；恢复路径是重新 `device pair`。这是单次语义的显式代价，不在本版加入 `lookupRequestId`。

替代方案：让 claim 直接接受 `pairingCode`。否决：claim 的 transcript 需要 `pairingId`/`hostId`/`hostPublicKey`，客户端在算 HMAC 前必须知道这些；lookup 正好提供它们，且不改变 claim 的 wire。

### D5：网页入口的主机公钥 = 同源 lookup 的首次信任

二维码入口的锚仍是 fragment 里的带外公钥（`§7.2/§7.3` 原样）。网页入口没有带外载荷，lookup 响应经 TLS 与 canonical origin 取得，客户端把它作为本次配对的绑定公钥：

- claim 的 `hostProof` 用该公钥验证；
- approved 响应的公钥必须与该公钥逐字节相同；
- 若该 canonical origin 已有不同公钥的已配对记录，失败关闭（主机身份变化），不覆盖。

允许任意 canonical origin（LAN/Tailscale 也可），因为网页 JS 本来就由这个 origin 提供；origin 被攻破时 fragment 里的公钥也无法阻止恶意 JS 跳过校验。丢失的只是「同源代理错路由到另一台 daemon」的检测。配对码输入页显示当前 canonical origin，让用户能发现打开的是另一台机器。

### D6：前端状态、倒计时与失败呈现

- `PairingPageModel` 增加 `awaiting_code`、`looking_up` 与 `code_invalid`；阶段与原型 S0–S12 一一对应。
- S0：8 个单字符格，大小写不敏感、`I/L→1`、`O→0`、粘贴 `XXXX-XXXX` 或连续 8 位；显示 canonical origin 与 HTTPS 标识。
- **倒计时只从 lookup 之后开始**：S0 不显示数字倒计时，只显示静态提示“配对码 5 分钟内有效”。页面在 lookup 之前不知道真实 `expiresAt`，因此不得显示本地估算；lookup 成功后立即用响应里的 `expiresAt` 启动倒计时。claim/status 的服务端状态始终优先，服务端返回过期时停止轮询并进入对应呈现。
- **与桌面原型的一处有意偏离**：原型 S0 从页面进入起显示 5:00；实现改为静态提示、lookup 后才出现倒计时，因为页面在 lookup 前无法知道配对已经生成了多久。原型的 S1–S12 呈现与文案继续保留。
- 失败：lookup 的任何失败（含响应丢失）统一呈现「配对码不对或已过期，请重新生成配对码」，不回显后端 `message`、不显示配对是否存在、不提供页内重试。
- 码只活在 `ClientStore` 私有字段，终态/卸载时清除；不写 URL、Cookie、localStorage、sessionStorage。
- 终端 `y/N` 不变；approved 后仍自动进目录。

### D7：管理面与 CLI

- `device.pair.begin` 返回值新增 `pairingCode`（同时仍返回完整 `pairingUrl`）。这是加法字段；旧客户端忽略它。
- CLI 打印 URL 与配对码，例如：

```text
https://127.0.0.1:18765/pair
配对码：ABCD-EFGH
（移动端请扫码/打开上面的完整地址；网页端打开 /pair 后输入配对码）
```

- 不改 `device.pair.status`/`confirm`/`reject`；配对码不参与确认。码被网页消耗后，CLI 继续轮询并等待 `y/N`。

### D8：wire 资产、清理接线与测试

- `schemas/sync/v1/pairing.schema.json` 新增 `lookupRequest`/`lookupResponse` 两个 `$defs` 并加入 `oneOf`；`compatibility/errors/v1/errors.json` 新增 `pairing.code_invalid`（`retryable: false`，无 details）。
- fixture 覆盖成功、字段缺失、错误长度、错误码形状；`sync-protocol` 新增对应 DTO 与 schema 往返/负例测试。
- `identity-auth` 新增码派生的固定向量（至少 3 组 secret→code）、单次消耗、碰撞去重、secret 清除后查找失败、重启不恢复的测试；Crockford 编解码自己实现，不新增依赖。
- `server::sync::pairing` 新增 lookup 的原子性/统一失败/单次消耗/独立限流测试；daemon 的过期维护接到 `due_pairings`/`unrecoverable_after_restart`，启动路径调用 `reset_memory`。
- 前端新增模型单测与页面 e2e：S0–S12、S0 无数字倒计时与静态提示、错误文案、码不落盘、lookup 后使用 `expiresAt`、二维码路径回归。
- 仓库级验证仍在主检出跑 `npm run verify`。

## Risks / Trade-offs

- **单次 lookup 的响应丢失**：码已被消耗，用户只能重新 `device pair`。这是 Q3=b 的显式代价；页面用固定文案说明，不做自动重试。若后续要消除，需要另加 `lookupRequestId` 与服务端缓存，属于新的 wire 变更。
- **lookup 前的信息不足**：S0 不显示倒计时，用户只能从终端看到真实截止时间。缓解：S0 显示静态“5 分钟内有效”，终端打印“有效期至 …”；lookup 成功后立即显示服务端 `expiresAt`。这比显示一个可能错误的本地 5:00 更诚实。
- **网页入口是 TOFU**：同源代理错路由/多 daemon 场景下，页面可能把错误主机当作配对目标。缓解：canonical origin 绑定、TLS、终端仍显示确认、失败关闭已有记录；页面显示 origin；移动端不受影响。
- **码碰撞**：40 bit 空间下同时活跃的配对极少；创建时重新生成 secret/码保证唯一，不暴露给用户。
- **过期清理接线扩大范围**：补接 `due_pairings`/`reset_memory` 会触及 daemon 维护路径，需要回归现有配对生命周期测试；这是安全文档已承诺的行为，不是新功能。
- **旧客户端**：多出的 `pairingCode` 字段是加法；旧 PWA 不读取它，二维码路径不变。

## Migration Plan

无数据库迁移、无部署形态变化。`device.pair.begin` 只增加响应字段；旧 CLI 忽略它，旧 PWA 继续走 fragment。新增 `pairing.code_invalid` 是错误词表的加法，旧客户端不会收到该码。过期清理接线只影响内存 secret 的清除时机，不改变持久状态。回滚删除 lookup 路由、`pairingCode` 字段、错误码与前端表单即可；已完成的配对与设备记录不受影响。移动端二维码路径在本次变更中零行为变化。
