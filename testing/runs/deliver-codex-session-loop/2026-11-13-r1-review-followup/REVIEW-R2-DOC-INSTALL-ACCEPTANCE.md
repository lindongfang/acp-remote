# R2 Doc 七叶安装后验

2026-11-13，独立 reviewer 只读复核。结论：**Doc 七叶窄切片可接受并保持窄集成；不代表完整 R2、Source producer、R3 或 WP4 通过。**

## 固定身份与安装边界

- Root 安装记录 `2026-11-13-r2-doc-slice-development/ROOT-INSTALL.json`：517 叶逐叶匹配，canonical `21137d04eb1cfcdce87342d363d29ccaa9734b2f75f716aaec760fe2cd8061f1`，index 未变。
- 本次安装后逐叶复核同时读取 Root 工作树与隔离源 `SOURCE-GREEN.json`：两侧均 517/517 匹配，计算 canonical 与清单一致；实际 index SHA `ac76c7ad84c1d4333fc64a710647e60aeb96015eb4407f996a4bf6adc91fbff4`。
- 固定测试 PE SHA `7d8789785a2728360850daa858140b7beaeaa7948da66a0c2b806fcd0af57fc1`；独立 tester 的 `RESULT.json` 前后身份一致。七个变更叶为 `document_source.rs`、`queue_owner/document_read.rs`、`queue_owner/service.rs`、`node.rs`、`node/document_payload.rs`、`node/document_transfer.rs`、`node/document_transfer/development_tests.rs`，未漂移 R1 冻结叶。
- 安装后核验机器记录：`REVIEW-R2-DOC-INSTALL-READ.json`，SHA `e8a51c6b4c46af3f36af702f62d05df00d96ba4b4a4f34e7bfdc23079cdeeeca`。该记录包含逐叶结果、安装清单、PE、index、all-targets 结果及七叶相对 R1 的实际 delta。

## 独立动态证据

独立 tester 目录 `2026-11-13-r2-doc-independent` 的 `RESULT.json` SHA `2c34356c56749791461baef108c4b7b48db9d37ede77396f706457c99c9f9397`，`REPORT.md` SHA `8c814818253d79a8260a5513d7bfa3160fdb3237fc5fdf86b76bf3a7608c41cd`，证据清单 SHA `aa2c2512c1fdd7faa36ee5371a54bb273bb35a88666afd859d01ebd8b0d77f89`。

- `--list` 实际发现 947 项；七个固定 selector 各为 `1 passed / 0 failed / 0 ignored`。
- 八个 list/test Job 均 `naturalExitAndEmptyJob=true`、`stopInvoked=false`、`activeJobProcessesAfterExit=0`、无超时；各自 TEMP/TMP、p1、rpc 目录自然为空，未用删除操作补证据。
- 七项覆盖同一生产 disposal owner 的 payload slot、From 后原 allocation pointer/bytes/len、snapshot scope/current/retire、原 Queue 到真实 socket/final credit、edit/cancel seal、同 actor 另一 socket 拒绝、descriptor canonical decimal/null、completed/empty reaping 与 publication failure。六个 Node 用例的 shutdown/join/registry/area 清理断言均在原日志中出现；第 2 项日志没有额外 shutdown marker，但其 Job 自然退出且测试通过，不能据此扩大范围。
- 原 API Red 只读复核恰有两条 E0308；三 API 叶回置 R1，Red 清单是 512 叶。Green 517 叶中缺少的五个支持叶（Web dist 三叶、`rust-toolchain.toml`、`testing/ci/windows-owner.ps1`）未被错误宣称为 Red 覆盖。首次把 512 按 517 校验的 preflight 失败发生在任何 list/test 进程前，已原样保留在 `PREFLIGHT-ERROR-v1.json`，不计产品失败。

## 受影响检查

Root 安装后的 `root-check-all.result.json`：`cargo check --all-targets --all-features --locked --offline` exit 0、non-timeout、natural/empty Job、Stop=false、active0，编译错误数为 0。`root-default-build.result.json` 的默认 `cargo build --locked --offline` 同样 exit 0、natural/empty、Stop=false、active0。`root-format-check.result.json` 与 Root 的 `git diff --check` 记录为 0；这些是 Root 执行的机器证据，我只读复核记录与身份，不将其改写为独立 tester 动态。

## 范围结论

本次关闭的仅是七叶 Doc payload/snapshot/唯一 owner 窄切片及其受影响检查。Source producer、长 typed Source 响应、CPU buffer/retire 全量写集、M01–M07、完整 R2/R3、真实 Harness/WP4 仍未验收；Source coder 本轮中间 18 叶也不构成实现证据。后续可在保持单一 `DocumentDisposalOwner<NodeDocumentPayload>`、不新增 Storage 登记、不改 R1 冻结证据的前提下继续 R2 串行写集。
