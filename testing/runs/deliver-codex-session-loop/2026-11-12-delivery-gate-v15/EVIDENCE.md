# deliver-codex-session-loop v15 证据

## 版本与范围

- 日期：2026-11-12
- 分支：`codex/session-foundations-replay-planning`
- HEAD：`65ec7815bb0280f4519989d93a7b4ed3b4b37994`
- 实际工作树：`D:\Project\acp-remote`
- OpenSpec change：`deliver-codex-session-loop`
- 本证据只证明下列实际执行范围。它不是最终同版 source set，也不代替独立 tester/reviewer。
- 本目录尚未生成 `SOURCE.json` 与 `MANIFEST.json`；后续冻结时必须基于本目录写入时点的实际文件生成，不能沿用旧 v11/v14 摘要。

## 已完成的有效运行

### 4,429 超限帧关闭

- 用例：`node::tests::management_websocket_closes_oversized_transport_frames_with_4429`
- 命令范围：Rust 2024、`cargo test --manifest-path app/Cargo.toml --lib --locked --offline`，单例、`--test-threads=1`
- 环境：真实 Windows owner、绝对隔离 `TEMP`/`TMP`、独占 `CARGO_TARGET_DIR`
- 结果：1 passed，0 failed，912 filtered，0.14s
- 原始输出：`single-4429/test-absolute-temp.stdout.txt`
- SHA-256：`4067CF48DFD6C8D42522F4B063272DA9B8C55D0D90D803DCB05E1564EDC9DE21`

同一目录的 `single-4429/test.stdout.txt` 是相对临时路径导致链接器失败的历史现场，不计通过。其 SHA-256 为 `2685C4A08AA840E1A9C7D8F77A8CEB07BB2380BA2D871341D6F0B48DC2C967`。

### ACPI-R01–R09

- 命令范围：`cargo test --manifest-path app/Cargo.toml --lib --locked --offline acpi_ -- --nocapture --test-threads=1`
- 环境：真实 Windows owner、绝对隔离 `TEMP`/`TMP`、独占 target
- 结果：13 passed，0 failed，900 filtered，9.50s
- 覆盖：ACPI codec 及 R01–R09。
- 原始输出：`acpi-r01-r09/acpi-group.stdout.txt`
- SHA-256：`C5966A2FA64EAAF231F929F1480945DED6D2E4A9B10B24DE2C429702DD973281`
- 运行后进程观察：`acpi-r01-r09/remaining-processes.txt`，未发现本产品/原生开发子进程残留；SHA-256：`2F65611605683139FA5A6E473256BA695DAFFBDDDF5079568E38EB6BD44D99E9`。

### ACPI-R10

三个用例分别以叶名称筛选并实际运行：

| 用例 | 结果 | 原始输出 SHA-256 |
| --- | --- | --- |
| `production_factory_opens_listener_only_after_the_bundle_is_complete` | 1 passed，0.63s | `70732D30069222DE3E93160919A11DCBE6B352375C6C6AC23104519AADFA798A` |
| `production_factory_service_rejection_opens_no_listener_and_joins_workers` | 1 passed，0.63s | `2463128352D8F5F04BF03982D3734B8643338DF226FA938CC9E9C2F4D6423F28` |
| `queue_read_control_only_does_not_report_an_empty_product` | 1 passed，0.15s | `8A240D467CDCBA491E4AE9845DD43A17B55A19D0C9F9A9C46BF1C0311C1D929D` |

合计 3/3 通过。运行后进程观察见 `r10/remaining-processes.txt`，未发现本产品或原生开发子进程残留；SHA-256：`2F65611605683139FA5A6E473256BA695DAFFBDDDF5079568E38EB6BD44D99E9`。

`queue_read_control_only_does_not_report_an_empty_product.filter.stdout.txt` 是完整路径配 `--exact` 造成 0 tests 的失败筛选现场：0 passed，0 failed，913 filtered。该事实保留，不计产品通过。其 SHA-256 为 `5A484E02582E61AE76444C8FFB06E9DEE979490B761118B5D05B7112665138C2`。

## 严格 Clippy

- 命令范围：`cargo clippy --all-targets --all-features --locked --offline -- -D warnings`
- 环境：真实 Windows owner、绝对隔离 `TEMP`/`TMP`、独占 target
- 结果：退出码 101。
- `lib` 因 354 个前置错误失败，`lib test` 因 386 个前置错误失败。
- 原始输出：`clippy-all-targets/clippy.stdout.txt`
- SHA-256：`3D9F93CB24282E09806CA712A80BC705E8DAE26B0E00C8F983E72DA76B7377E5`
- 主要类别包括 `significant_drop_tightening`、`too_many_lines`、`redundant_pub_crate`、`many_single_char_names`、`missing_errors_doc`、`option_if_let_else`、`cast_possible_truncation` 和 `future_not_send`。
- 未使用 `-Awarnings`、lint allow、移除 lint 分类或修改门禁冒充通过。

因此 WP1.4 继续保持待办，CL-001/002 的有效检查门尚未满足。

## 关闭 watcher 诊断清理

正式关闭 watcher 保留在：

- `app/src/session_storage/registry_connection_owner.rs`
- `app/src/session_storage/request_registry.rs`

此前用于定位问题的 `shutdown_phase`、槽位快照和跨层测试 getter 已删除。强化后的测试仍要求关闭时对实际 owner task 执行 join，并在时限内完成；不能用只观察状态位代替实际资源回收。

清理前已有两组有效证据：

- 关闭语义组合 8/8：`../2026-11-12-delivery-gate-v14/close-watcher-semantics-v14h`
- 关闭关键字回归 132/132，70.20s：`../2026-11-12-delivery-gate-v14/close-keyword-v14d`

这两组结果对应诊断清理前的源码。清理后已重新运行：

- 受限沙箱运行：100 passed、17 failed，68.57s。
  - 原始输出：`close-keyword-v15/close.stdout.txt`
  - SHA-256：`9A277AE5C9F2322098FC382554A2D0F62C074644E1706CB9432B54738BB9A8D0`
  - 失败集中在 Windows socket 错误 10013 和 owner named pipe 错误 5；失败列表含真实 TLS、配对、共享视图和 Job 用例。该结果不计产品通过。
- 同一源码在非沙箱 Windows owner 上下文串行重跑：117 passed、0 failed、796 filtered、69.21s。
  - 原始输出：`close-keyword-v15-owner/close.stdout.txt`
  - SHA-256：`11953FD0C17CA5DED1AC556FF3A0A5358465DB21D04ECFFA3525BDCB75CB3A02`
  - 结构化结果：`close-keyword-v15-owner/result.json`，退出码 0，0 个残留 `acp-remote`/`native_dispatch_dev_child` 进程；SHA-256：`7048DB2EA6A1C546954F69729AC120EEE1CB9B81114BA4B08BAA16DE52C702A1`。
  - 当前源码经 `close` 关键字筛选得到的是 117 项，不是诊断代码删除前的 132 项；不回填已不存在的测试数量。

## 尚未完成

- 关闭关键字的当前源码有效 owner 结果为 117/117；受限沙箱失败现场也已保留。
- 完整 Rust `--all-targets --all-features --locked --offline` 尚未在本证据目录复跑。
- Web `check/test/build`、Rust `fmt`、OpenSpec strict、`git diff --check` 尚未在当前最终源码稳定后复跑。
- 4.1 的生产 WSS 视图、审批、原生释放、逐块读取及撤销路由仍在实现。
- 真实 Codex/DeepSeek、可信 Web、SQLite、Windows Job 完整树及独立 tester/reviewer 验收仍未完成。

任何实现或测试文件在本文件写入后再发生变化，都必须重新运行受影响检查；最终 `SOURCE.json` 只能在实际集成版本的检查完成后生成。
