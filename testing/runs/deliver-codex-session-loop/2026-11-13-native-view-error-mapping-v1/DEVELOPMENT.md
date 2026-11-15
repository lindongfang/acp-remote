# FR002 服务端原生视图错误分类开发记录

2026-11-13。仅服务端错误投影与开发单元测试；独立验收和交付检视另存，不由本记录代签。

旧实现把 `ViewError::NativeFailure` 的全部原因压为 `protocol_failure`。按原 finding reviewer 的 `FR002-PLAN-REVIEW.md`，本候选仅在该分支接入紧邻的私有穷尽映射：配置、目标、来源、范围、停止、释放和长度采用既有对应码；三个协议类保持协议错误；其余运行时、存储和平台来源错误保守返回 `read_failed`。所有29个 `DispatchError` 显式列举，新增变体需要编译期重新判断。直接 `ViewError` 映射、认证关闭、原生错误事实、释放、ACL及公开响应结构未改。

基线为 `BASELINE.json` 的514叶；最终 `SOURCE-GREEN.json` 的515叶 canonical 为 `5f076a551d81c3b3ed28b18a0c6bc3d66772969170178962ec3b745c9aaf9942`。清单自身 SHA-256 为 `22c7f058f416aca2d48680d49387c5619bdf050ceade940f7c18135198a16472`。规则及实际生成脚本见 JSON、对应TSV、`freeze-source.mjs`；运行后已重新逐叶核验。唯一变化为 `app/src/management_ws/session_commands.rs` 和新 `session_commands/view_error_development.rs`。

## 实际验证

Rust/rustc 1.98.1，locked/offline，串行测试，隔离 TEMP/P1/RPC，Windows owner 前置；本 coder 独占 `.tmp/wp4-owner-20261113-v2-target`，构建期间没有并发共享使用者。精确命令与时间见 `run-check.ps1` 和逐轮 `RESULT.json`。

| 运行 | 结果 | 实际覆盖 |
| --- | --- | --- |
| red-v1 | exit101，2过2败 | 旧映射在配置分类及完整响应首项失败；直接码/协议类通过 |
| red-v2 | exit101，2过2败 | 补第二平台payload和SqlClose false后的最终测试，在同一旧映射首项失败 |
| green-v1 view-error | exit0，4/4 | 30个非ProtocolFault输入、17协议子项、30直接ViewError输入、5个完整序列化输入全部到达 |
| green-v1 session-command | exit0，6/6 | 四专项重复执行及原两项队列投影测试 |
| green-v1 native-release | exit0，7/7 | 原完整服务路由、actor绑定、持久化及schema10故障/重开 |
| green-v1 fmt | exit0 | cargo fmt --all -- --check |

实际为13个不同顶层测试，不能把重复执行累计为17个不同场景。两个Red失败循环均在首行 `InvalidConfiguration` 停止，后续输入只在Green实际遍历；不声称Red已逐个触发。第二SourcePlatform序列化输入由独立tester预读提出，本coder增加断言后重新跑最终Red，未放宽预期。

最终实际执行的 lib PE 与 dev child 已精确复制到 `artifacts/`，大小和SHA固定在 `DEVELOPMENT-RESULT.json`。Root产品尚未安装此候选。Git index保持原摘要。既有编译warnings可见原stderr，未宣称strict已通过。

初次 PowerShell freeze脚本调用以-1结束且没有生成身份文件，保留脚本和事实；后用Node自包含脚本重新读取实际文件树生成完整规范身份。未将工具失败当产品Red。

## 限制

未执行完整Rust、strict Clippy、真实外部Harness、完整可信Web和端到端错误呈现。现有Web错误码闭集缺少部分既有服务端码，另需合同对齐，本次服务端通过不能代签Web。`npm audit`按用户要求未执行。完整任务不勾，FR002由原提出者在同版独立证据后复核。
