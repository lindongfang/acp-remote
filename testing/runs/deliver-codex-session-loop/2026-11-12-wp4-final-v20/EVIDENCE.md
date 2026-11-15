# WP4 final v20 同版回归证据

## 范围和源码快照

- 时间：2026-11-12
- 分支：`codex/session-foundations-replay-planning`
- HEAD：`65ec7815bb0280f4519989d93a7b4ed3b4b37994`
- Change：`deliver-codex-session-loop`
- 受控源码文件：904
- source set SHA-256：`14A9BE497B966717DC1D72FFE989BB9BE46F7DB9F306B1018008C527BD3DE40D`
- `SOURCE.json` 的稳定摘要和文件清单是本轮同版边界。运行使用新源码副本、隔离 `TEMP/TMP`、独立 `CARGO_TARGET_DIR`、`--locked --offline` 和非沙箱 Windows owner 身份。
- 本证据由主执行/coder 生成，不是独立 tester 或 reviewer 报告。

## Red Check

首轮运行器没有把 `P1_DATA_ROOT` 传入 socket-independent 测试。该轮 15 例中只有不依赖数据根的 wire oracle 通过，另外 14 例在共同初始化处以 `exclusive data root` 失败。原始现场已用同一冻结源码副本、去掉该环境变量重新保存：

- `socket-independent.red.stdout.txt`
- `socket-independent.red.exit.txt`：101
- 测试摘要：1 passed / 14 failed / 904 filtered

运行器随后只在专用数据目录中设置绝对路径 `P1_DATA_ROOT`，没有改变产品代码、测试断言或 source set。

## 绿跑结果

`results.txt` 中所有入口均为 exit 0：

| 入口 | 有效测试结果 |
|---|---|
| native dispatch child build | exit 0 |
| ACPI codec 与 R01-R09 | 13 passed |
| R10 listener bundle gate | 1 passed |
| R10 service rejection rollback and joins | 1 passed |
| R10 control-only no empty product | 1 passed |
| CL-009 已受理输入与设备撤销 | 1 passed |
| CL-017 SQL fault / compare-and-set | 1 passed |
| CL-024 view window | 2 passed |
| registration owner acceptance | 51 passed |
| ACP view observations | 11 passed |
| ACP tool projection | 19 passed |
| session loop acceptance | 57 passed |
| shared input queue acceptance | 39 passed |
| socket-independent acceptance | 15 passed |

ACPI 的 13 个用例共同覆盖 ACPI codec 和 R01-R09；R10 的三项另行逐个执行。以上结果是同一 source set 下的真实 Rust、Windows owner、SQLite/TLS/IPC 和开发夹具组合，但不代表完整 Rust 测试门通过。

运行完成后的 `acp-remote` 与 `native_dispatch_dev_child` 目标进程快照为 `[]`，见 `processes-after.json`。

## 其他同版检查

- Web `check`、`test`、`build` 全部通过：31 个测试文件、680 项测试。证据见 `../2026-11-12-wp4-body-read/`。
- `cargo fmt --manifest-path app/Cargo.toml -- --check`：exit 0。
- `openspec validate --all --strict`：11/11 passed。
- `git diff --check`：exit 0，仅保留既有换行提示。

## 未通过或未覆盖

- 完整 Rust all-targets 测试未通过。v19 当前源码长跑的测试结果为 851 passed / 68 failed / 735.28s；测试最终结果输出后，Cargo 及 owned 进程树仍未退出，被运行所有者终止。`full-rust-owner.exit.txt = -1` 是终止造成，不是正常测试退出码。现场见 `../2026-11-12-wp4-regression-v19/full-rust-owner.*`。
- strict Clippy 仍 exit 101，414 个诊断。不得用关闭警告或放宽 `-D warnings` 代替。
- 当前结果没有完成 CL-030/CL-031、完整真实 Codex/DeepSeek Harness、可信 Web、最终 SQLite/Windows Job 组合，也没有独立 WP5/WP6 同版报告。
- `npm audit` 未执行；这是用户明确限制，不记为通过。

因此 v20 关闭 WP4 的同版开发回归证据缺口，但 4.1、1.4、5.2 和 6.10 仍不能仅凭本文件勾选或归档。
