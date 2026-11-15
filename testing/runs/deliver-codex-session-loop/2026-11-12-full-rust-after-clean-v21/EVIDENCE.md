# 清理后完整 Rust 长跑 v21

## 结论

D 盘清理后，同一 v20 source set 的完整 Rust `--all-targets` 串行长跑得到：

- **878 passed / 41 failed / 0 ignored**
- 268 个过滤后的子进程调用中，两次 crash-child 重入各得到 0 passed / 1 failed；
- 主测试目标在 760.72 秒后给出最终汇总，整个 owner 脚本耗时 865.181 秒；
- Cargo 正常退出为 101，目标进程树在运行后为空。

这证明 v19 的 68 个失败中，大部分确由 D 盘耗尽引起。v21 的 41 个失败全部落在依赖 Windows 当前用户 DPAPI 的身份、Node 启动和 shared-view 用例族，没有出现清理后仍独立的业务数据断言失败。

## 源码和环境

- source set：v20，904 个受控文件
- source set SHA-256：`14A9BE497B966717DC1D72FFE989BB9BE46F7DB9F306B1018008C527BD3DE40D`
- HEAD：`65ec7815bb0280f4519989d93a7b4ed3b4b37994`
- 分支：`codex/session-foundations-replay-planning`
- 使用新源码副本、新数据根、新 `TEMP/TMP`、`--locked --offline`、Windows owner preflight 和串行测试。
- 数据根位于 C 盘专用 visualization 目录，避免再次耗尽 D 盘的证据目录。
- `ENVIRONMENT.txt`、`started-at.txt`、`duration-seconds.txt`、`processes-after.json` 保存实际运行参数和清理结果。

## 失败分类

执行身份是专用 Windows 账户 `faraday\codexsandboxoffline`，其 `USERPROFILE` 环境值指向 `C:\Users\jack`，但该账户的 `HKEY_USERS\<SID>` 配置项未加载。对此身份直接调用 Windows `ProtectedData.Protect(CurrentUser)`，系统返回：

> 数据保护操作未成功。这可能是由于未为当前线程的用户上下文加载用户配置文件导致的。

因此：

- `platform::windows::tests::dpapi_round_trip_uses_current_user_scope` 在真实 DPAPI 调用处得到 `Protection`；
- 4 个 identity/DPAPI 用例无法进入身份持久化或 crash-point 业务断言；
- 3 个直接启动真实 Node 的 shared-view 用例得到 `Identity(Platform(Protection))`；
- 1 个真实 Node 停止期限用例得到同一错误；
- 29 个 shared-view module 用例、3 个 auth-close fixture 用例和 1 个 formal-auth fixture 用例在 Node 启动前映射为保守的 `ReadFailure`，这 33 项属于同一 DPAPI 根因的派生失败。

这是执行账户缺少用户配置文件造成的 Windows 环境阻断。不得用测试 protector、放宽 `Protection`、伪造身份或宣称用户隔离通过来消除这些失败。

## 证明清理前污染

v19 的同名长跑为 851 passed / 68 failed，并且测试汇总后 Cargo 不退出。v21 中以下此前因 `database or disk is full` 失败的组均实际通过：

- lifecycle 与真实 SQLite owner join；
- catalog/body/document/input upload；
- Node owner IPC、认证和 identity 正常路径；
- management socket 与 pool-close；
- session storage、product commands 和物理关闭矩阵。

v19 失败现场保存在 `../2026-11-12-wp4-regression-v19/`，不得删除或改写成通过。

## 对 1.4 的影响

完整 Rust 长跑已不再挂起，失败范围也缩到可解释的 DPAPI 环境门，但 1.4 仍不能完成：

- 需要在加载用户配置文件的交互式 Windows 身份下复验上述 41 项；
- strict Clippy 仍为 exit 101；
- 依赖审查/锁文件、可运行 README 和用户明确禁止执行的 `npm audit` 限制仍需准确记录；
- CL-030/031 和真实 Codex/DeepSeek Harness 尚未由本结果覆盖。

本报告是主执行/coder 的原始运行记录，不是独立 tester/reviewer 通过报告。
