# PR #199：cmd 隐藏退出码环境条目修复

范围：修复[原 CI 失败](https://github.com/lindongfang/acp-remote/actions/runs/41091666859/job/124186833243?pr=199)中平台进程启动的环境校验问题。本记录是开发诊断及回归结果，不代签独立安全检视、会话产品验收或已暂停方案的重基准。

## 原因与复现

原提交 `2a6b2786` 的 CI 为 219 项库测试、4 项 JWS 向量通过，平台测试 3 过/8 失败；错误均为 `InvalidSpec("invalid environment name")`。本机真实用户环境重复得到相同完整检查结果。

最小复现命令：

```powershell
npm.cmd exec --call "cmd /d /c exit 0 && cargo test --manifest-path app/Cargo.toml --locked --test platform_owner stdin_eof_allows_graceful_exit_and_pending_read_is_cancellable -- --exact"
```

修复前连续两次失败；去掉前置 `cmd` 命令的同一测试连续两次通过。隔离诊断只输出保留名称及长度，不输出环境值：失败链中有 `=D:` 和 `=ExitCode`，无空名或超长名称。非零退出码 `cmd /d /c exit 37` 还产生 `=ExitCodeAscii`。原代码只接受继承的驱动器条目，错误拒绝 cmd 正常传下来的退出状态条目。

## 修复边界与回归

仅在系统继承来源额外接受精确名称 `=ExitCode`、`=ExitCodeAscii`，原样编码并继续采用已有 Windows 名称比较、去重及排序。没有清空或过滤父环境，也没有放开其他保留名称；显式 overrides 仍拒绝任何含等号的名称。进程启动方式、句柄清单、Job 所有权、截止时间及错误隐私边界不变。

新增 `cmd_exit_code_environment_is_inherited_without_rejecting_spawn`：真实 cmd 执行 `exit 37`，先确认其退出码及 ASCII 条目确实存在，再通过实际 OwnedProcess 启动和嵌套 fixture 比较驱动器、退出码和 ASCII 条目的原值；不修改当前进程环境或输出继承值。修复前该测试失败于相同环境校验，修复后通过。显式保留键测试新增这两个名称、大小写变体及畸形条目，均仍拒绝。

## 实际验证

- 新增真实 cmd 回归、原最小复现：修复后通过。
- 独立运行全部平台开发测试：12 项通过，包含 fixture 普通入口；不把 fixture 空跑计为额外功能证明。
- 首轮完整 `npm run check`：Web 50 项测试与构建、Rust fmt/clippy 通过；库测试 218 过/1 失败，既有 `management_websocket_closes_oversized_transport_frames_with_4429` 收到连接重置（10054）。原始失败保留在 `.tmp/pr-199-fixed-check.log`，没有修改网络代码或跳过测试。
- 同一版本单项复核上述 WebSocket 测试通过；随后完整 `npm run check` 退出 0：Web 50、Rust 库 219、JWS 向量 4、平台 12 项均通过，fmt/clippy 及 Web 构建通过。两轮均加载仓库 `testing/ci/windows-owner.ps1`，未清理继承环境。最新日志 `.tmp/pr-199-fixed-recheck.log`。
- 已有 WebSocket 关闭偶发失败仍属限制：本次通过不证明该问题已修复；原有独立记录及暂停状态继续保留。

修复文件 SHA-256（原始工作区字节）：

| 文件 | SHA-256 |
| --- | --- |
| `app/src/platform/process_owner.rs` | `D36C45EFE87387A6058E66D632DF4672C6A74EF0AB36EE660CC2E8C6551A3F8B` |
| `app/tests/platform_owner.rs` | `EF463AC541A4F0A7892973401364BB413739890E65B4D216DFD6CBD1610B9BF8` |
| `app/DEPENDENCIES.md` | `E84698DADB717FA0EE971F705917F9627A1880BB5FE71C3A6CEC732367D1CBEA` |

临时名称诊断程序已删除；原始本地检查日志保留于忽略目录，不提交用户环境、凭据或私有正文。
