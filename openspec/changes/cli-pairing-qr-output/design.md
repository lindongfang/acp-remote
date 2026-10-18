# Design

## Context

`device pair` 的 begin 返回 `pairingUrl`（`https://host/pair#data=<base64url-json>`，含主机公钥与一次性 secret）和 `pairingCode`。网页端的新入口是裸 `/pair` + 配对码；移动端仍用 fragment 载荷。当前 CLI 只把 `pairingUrl` 作为文本打印，网页用户会打开 fragment 入口，看不到配对码输入；终端也没有二维码。

## Goals / Non-Goals

**Goals:**

- 终端输出把“网页端”和“移动端”分开：裸 `/pair` 给网页，二维码给移动端。
- 二维码内容与 `pairingUrl` 逐字节相同，不改变协议或管理面返回值。
- 依赖只进入 `app`（CLI），不进入 core/协议 crate。

**Non-Goals:**

- 不删除或改变 `/pair#data=…` 入口；二维码仍然依赖它。
- 不改 `device.pair.begin` 的返回值、配对码派生或确认仪式。
- 不为 `node pair` 增加配对码/二维码。

## Decisions

### D1：按 target 分流输出

`await_claim` 目前同时服务 `device pair` 与 `node pair`。只在 `target == Device` 分支改变 begin 的输出：

```text
网页端：https://host/pair
配对码：ETAB-32HZ

<二维码>

（移动端请扫码；网页端打开 /pair 后输入配对码）
```

`node pair` 继续打印原来的 `pairingUrl`，不打印配对码或二维码。

### D2：裸地址由 fragment 截断得到

`pairingUrl` 是 `https://host/pair#data=…`。网页端地址是 `split_once('#')` 的前半段；不引入第二份 origin 拼接逻辑，避免与 `public_origin` 规范化分叉。

### D3：二维码编码完整 `pairingUrl`

二维码必须编码完整的 `pairingUrl`（含 fragment），否则移动端扫码拿不到主机公钥与 secret。二维码矩阵不把 URL 作为文本输出；完整 fragment 文本 MUST NOT 出现在终端。CLI 仍然只在内存与终端持有该 URL。

### D4：依赖与渲染

- 在 `crates/app/Cargo.toml` 增加 `qrcode = { version = "0.14", default-features = false }`：纯 Rust、MIT/Apache-2.0、无原生依赖；只用于终端。
- 用 `qrcode::QrCode::new(pairing_url.as_bytes())` 生成矩阵，`render::unicode::Dense1x2` 渲染（半块字符，约 1 字符宽/模块、2 模块高）。
- 生成失败时返回 `local.internal` 的 CLI 失败，错误消息不回显 URL/配对码；配对本身仍可在有效期内被网页端使用，用户可重跑 `device pair`。二维码容量远大于该 URL，正常不会失败。
- 终端不支持 Unicode 半块字符时仍会显示块字符或替代字形；这是终端能力问题，不改变协议。

### D5：测试

- 单元测试：裸地址截断、二维码渲染非空且包含块字符、完整 URL 作为二维码输入。
- 集成测试：`device pair` 输出依次包含 `网页端：`、`配对码：`、指定提示，且不含 `#data=`；`node pair` 输出不含配对码/二维码。
- 不引入二维码解码器；`qrcode` crate 的编码是信任边界，测试验证的是“编码的是哪段内容”。

## Risks / Trade-offs

- **终端 Unicode 支持**：Windows 旧控制台可能显示方块；用户需要 Windows Terminal 或支持 Unicode 的终端。CLI 仍会把裸地址与配对码打印出来，网页路径不依赖二维码。
- **二维码宽度**：完整 URL 约 550 字符，二维码约 77–97 模块宽，半块渲染约 77–97 列；窄终端会折行。这是移动端扫码的代价；网页端不受影响。
- **旧书签**：用户仍可能打开旧的 `#data=` 地址，页面会走 fragment 入口；这是移动端入口，不能删除。新的 CLI 输出会明确区分两个地址。
- **依赖**：新增 `qrcode` 只进 `app`；需要 `cargo-deny` 的许可证/来源检查（CI 的 deps job）。

## Migration Plan

无数据/部署迁移。回滚删除 `qrcode` 依赖与输出分支即可；管理面返回值不变，旧客户端不受影响。
