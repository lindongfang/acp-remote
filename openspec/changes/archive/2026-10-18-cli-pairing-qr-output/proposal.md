# Proposal

## Why

`device pair` 现在把带 `#data=…` 的完整配对 URL 作为第一行打印。网页用户会直接打开这个二维码入口，看到旧的自动认领流程，而不会去裸 `/pair` 输入 8 位配对码；终端也没有二维码，移动端只能复制长地址。提示语还把“扫码/打开完整地址”和“网页端 /pair”混在同一行。

## What Changes

- `device pair` 的 begin 输出改为：网页端裸地址 `https://host/pair`（无 fragment）、`配对码：XXXX-XXXX`、终端二维码、提示“（移动端请扫码；网页端打开 /pair 后输入配对码）”。
- 终端二维码编码完整的 `pairingUrl`（含 `#data=…`，移动端扫码用）；完整 fragment 地址 MUST NOT 再作为文本打印。
- `node pair` 的 begin 输出保持不变（仍打印 `pairingUrl`）。
- `crates/app` 新增纯 Rust 的 `qrcode` 依赖（`default-features = false`），只在 CLI 终端渲染使用。
- 同步 `cli-commands` 规格、`docs/LOCAL_ADMIN_PROTOCOL.md` §5.8 的 CLI 输出说明与测试。

## Capabilities

### Modified Capabilities

- `cli-commands`：`device pair` 的终端输出与二维码。

### New Capabilities

无。

## 非目标

- 不改 `device.pair.begin` 的 wire 或返回值；`pairingUrl`/`pairingCode` 仍原样返回。
- 不删除 `/pair#data=…` 入口：它仍是移动端二维码的入口，终端二维码编码的就是它。
- 不改配对码派生、lookup、限流或本机 `y/N` 确认仪式。

## 成功判据

- `device pair` 输出依次包含裸 `/pair` 地址、配对码、终端二维码、指定提示；完整 `#data=` 地址不出现在文本里。
- 二维码内容等于完整 `pairingUrl`，可被扫码读取。
- `node pair` 输出与改动前一致。
- `cargo test -p app --test cli_commands`、`cargo clippy -p app --all-targets`、`npm run check:docs`、`openspec validate --strict` 通过。
