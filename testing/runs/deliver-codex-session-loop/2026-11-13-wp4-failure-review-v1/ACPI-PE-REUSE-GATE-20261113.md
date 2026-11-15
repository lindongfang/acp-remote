# 当前源 ACPI/WSS 开发回归的固定 PE 复用门

2026-11-13。结论：**当前 Root 的固定 Rust 源集与已编 library PE、dev child 的既有绑定仍吻合，可复用这两个 PE 做新的限域开发回归，不必仅因 v20 源不同而重新构建。** 此为源码/产物与执行前置门审，不代表任何尚未运行的 ACPI、WSS 或组合场景已通过；正式运行器仍须固定和审查。

## 亲核身份

`2026-11-13-native-view-error-mapping-v1/SOURCE-GREEN.json` 自身 SHA-256 为 `22c7f058f416aca2d48680d49387c5619bdf050ceade940f7c18135198a16472`。本 reviewer 实际枚举并逐叶核对当前 Root 对应完整树，恰515叶，路径/长度/SHA-256 全部相同，canonical 为 `5f076a551d81c3b3ed28b18a0c6bc3d66772969170178962ec3b745c9aaf9942`。范围包括 app 的 src/tests/vendor/dev-fixtures/examples、Cargo/构建配置、工具链及 Windows owner 脚本、三个冻结 Web dist 叶；没有用 HEAD 或部分关键文件代替当前实际字节。

已读取 `ROOT-POST-INSTALL-VERIFICATION.json` 和原 Root post-install 执行记录，再次实际计算产物长度及 SHA-256：

| 产物 | bytes | SHA-256 |
|---|---:|---|
| `.tmp/wp4-owner-20261113-v2-target/debug/deps/acp_remote-e75360741b1915fc.exe` | 66087424 | `b6f7958f489fc04c9db1fa328cc71b1a96ce522341a4c175d0fc6e5a6bb5be3d` |
| `.tmp/wp4-owner-20261113-v2-target/debug/native_dispatch_dev_child.exe` | 690688 | `2bcf2efe3eb8bd1b95e42cb6b821cca208b0a2fb0fac74a0e904d898654ef978` |

本次只读输入与重算收据为 `ACPI-PE-REUSE-INPUTS.json`，SHA-256 `e16953372bacab8b09041bd1671660c48ce850ca234335d6609044ce34a106e7`。index 仍为 `ac76c7ad84c1d4333fc64a710647e60aeb96015eb4407f996a4bf6adc91fbff4`。没有运行测试、`--list`、Cargo、构建或 PE。

## Feature 与入口边界

实际 library Cargo fingerprint 的 features 是 `[]`；child fingerprint 才含 `native-dispatch-dev-fixtures`。不能将该 library PE 称为 all-features 或带 native fixture feature 的测试产物。但 MAP 目标 queue/session-services 测试模块位于 `#[cfg(test)]` + `#[cfg(windows)]`，没有 native fixture feature 门控；R10 lifecycle 测试也在测试模块中。因此空 feature 集不排除这些目标。带 feature 门控的 `native_views_bridge`、approval bridge 或外部 integration-test PE 不在复用批准范围内。

本 reviewer 核当前源中的12个 R01–R09 函数（R06拆四项）加 codec 的一个测试，静态目标为13；R10三个函数分别存在。另核 release route/registration 两个目标。共18个相关测试名的字符串在固定 PE 中存在。字符串存在仅是静态旁证，正式 runner 必须先用该 PE 的 libtest `--list` 实际检查名称与数量，不能据本报告冒称测试枚举已经执行或零测试通过。

若使用 `cargo test --features native-dispatch-dev-fixtures`，Cargo 可能生成另一个 library PE；不能将其结果继续绑定到本报告 b6f795 PE。复用路径应直接调用固定 PE 或先保存其受审副本，再验证副本哈希，不触共享 target 构建。

## 对 MAP 的补充判定

已核 `2026-11-13-wp4-next-verification-map-v1/MAP.md` SHA-256 `07df6061265003e917f1050f9d8057e23dd478e8948b81ebf941c8254fec7862`，其 `inputs.sha256` 的31项当前仍全部吻合。MAP 对旧 v20/旧 release v2/v3 不能当当前结果的限制成立，当前 ACPI-R01–R10 和队列/正文/释放组合仍须新跑。

但不能把“未找到当前完整组合通过”扩大成“没有当前源可复用的测试 PE”，也不能笼统说当前 native release 无同源记录：本轮 FR002 Root 安装后已经在该515源集、该 library PE 上实际执行 native_release 7/7 和 session-command 6/6，先前 reviewer 安装后报告有明确身份。这是现有范围的开发证据，不补齐 ACPI 或整组组合，也不复写历史 MAP。

Rust515绑定的 Web dist 仍为原三个叶；新 Web 错误合同的351源码和本次隔离 build 输出没有覆盖 Root dist。因此复用该 Rust PE 可以测当前冻结 Rust 的受控真实 TLS/WSS fixture，对新 Web UI 的 HTML/JS 服务、页面呈现或浏览器消费没有验证能力。不能将刚通过的 Web helper/socket 测试和此 PE 自行拼接成已经跑过的新 Web 页面完整链路。

## 下一执行门的最小条件

- 固定一份新 runner、完整命令和输出写集；写入全新独占 runtime/log/data 目录，拒绝覆盖旧证据。先设独占 TEMP/TMP，再在同一个实际启动 PowerShell 进程 dot-source `testing/ci/windows-owner.ps1`，只调整新对象默认 owner，不改变现存 ACL/owner。`NATIVE_DISPATCH_DEV_CHILD` 使用已核绝对路径或相同哈希的独立副本。
- 执行前与执行后都核515实际清单、index、library PE 和 child 摘要。若为runner复制，核原件与副本相同。禁止隐式构建、依赖安装、删除私人数据或修改测试预期。
- 先将该 PE 的 `--list` 原输出固定；核 `acpi_` 恰13（12个R场景+codec）、三个R10各自1项，以及每个其余过滤器的准确名称/非零数量。真正运行逐filter保存原stdout/stderr、命令、时间、退出码与实际test结果；`--exact` 时使用列表返回的完整限定名，防止筛选为零。
- 顺序运行，`--test-threads=1`，外层有界deadline；失败/超时按实记录并停止，不用旧pass、deadline或部分test当成功。保存真实Node/连接/worker关闭和child进程/Job观测；进程清理仅限本runner拥有的实例，不扫描或清理既有私人目录。若runner没有足够关闭证据，仅声明实际测试断言，不补写全进程树已清理。

具体开发过滤器可沿用 MAP 的 R01–R10、queue revoke/shared/register/consume/edit/resume、四个 `input_body_open_`、release route/registration 与 `session_storage::native_release::tests::`，以新列表实际核数。`session_loop_acceptance` 的CL024是另一 integration PE，不能从本 library PE推定存在或通过。

这些条件使固定 PE 的新开发执行可审查；不构成独立验收、真实 Codex/DeepSeek Harness、CL030/031、main浏览器、全producer动态故障注入或完整WP4签字。完整任务状态不因本门审改变。
