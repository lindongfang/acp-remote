# Tasks

## 1. WP1：CLI 输出与终端二维码

- [x] 1.1 在 `crates/app/Cargo.toml` 增加 `qrcode = { version = "0.14", default-features = false }`，并在 `Cargo.lock` 锁定；不进入 core/协议 crate。
- [x] 1.2 在 `crates/app/src/cli/pairing.rs` 增加：裸地址截断、`QrCode::new` + `render::unicode::Dense1x2` 渲染；生成失败返回 `local.internal` 且不回显 URL/配对码。
- [x] 1.3 改 `device pair` 的 begin 输出为「网页端：裸地址 / 配对码 / 二维码 / 指定提示」，完整 `#data=` 不再作为文本打印；`node pair` 继续打印 `pairingUrl`。
- [x] 1.4 更新 `crates/app/tests/cli_commands.rs`：断言设备输出包含裸地址、配对码、提示、块字符且不含 `#data=`；node 输出不含配对码/二维码。
- [x] 1.5 更新 `docs/LOCAL_ADMIN_PROTOCOL.md` §5.8 的 CLI 输出示例与说明。
- [x] 1.6 运行 `cargo test -p app --test cli_commands`、`cargo clippy -p app --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`npm run check:docs`、`npm run check:openspec`；全绿后提交分支与 PR。
