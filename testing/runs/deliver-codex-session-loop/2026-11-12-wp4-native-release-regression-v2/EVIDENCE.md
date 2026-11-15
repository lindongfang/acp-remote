# WP4 原生释放持久化与同版回归证据

生成日期：2026-11-12

## 结论

本证据绑定同一份实现与测试源码快照，证明原生释放操作已具备以下行为：

- 释放操作在交付前持久登记，并绑定 `actor_id`、认证代际、目标、阶段和必要的失败代码。
- 其他 actor 不能查询或执行该释放操作；旧认证代际不能继续使用原操作。
- 原生释放已成功但终态落库失败时，命令返回 `necessary_storage_unavailable`，结果为 `null`，持久阶段保持 `releasing`，不会误报 `released`。存储恢复后可用同一操作完成，并持久得到 `released`。
- 重启后仍能恢复操作事实；阶段迁移、幂等重试和终态失败重试均由专项测试覆盖。
- ACPI-R01–R10、既有会话/队列/视图/认证路由回归、Web、格式、OpenSpec 结构和差异检查通过。

这只关闭本轮 WP4 原生释放持久化及相关开发回归，不代表 `deliver-codex-session-loop` 已可归档，也不代替独立 tester/reviewer。OpenSpec tracked 进度仍为 **9/28**，4.1 也仍依赖 CL-030、CL-031、真实 Harness/Web 和独立验收。

## 固定源码

- 分支：`codex/session-foundations-replay-planning`
- HEAD：`65ec7815bb0280f4519989d93a7b4ed3b4b37994`
- 隔离源码：`.tmp/wp4-native-release-regression-source-v2`
- `SOURCE.json`：512 个文件
- source set SHA-256：`f9693f4061118a7965cd2821244420322a668b0b7afcf689f94dceec99d971d3`
- `SOURCE.json` SHA-256：`29d5d1cda89459cd534ce4fe243927beb24f83e04f926cb4ca9085315b4766b7`

源码副本固定 `app/` 与 `clients/web/dist/`，排除无法读取的 `app/.tmp/`。因此本摘要绑定实际参与本轮 Cargo 与 Web 产物检查的源码和构建产物，不声称覆盖仓库根目录中的旧 ACL 目录或其他未纳入文件。

## Red Check

### actor 绑定

目录：`../2026-11-12-wp4-approval-release-persistence-red-v1`

- `actor-binding-red.exit.txt`：`exit_code=101`
- 测试：`session_management_routes_approval_and_native_release_services`
- 结果：`0 passed; 1 failed`
- 旧实现下，第二个 actor 查询同一释放操作实际返回 `ok:true` 和 `state:registered`；断言要求拒绝，`left:true / right:false`。这证明原实现没有 actor 绑定，测试不是恒真。
- `native-child-build.exit.txt`：`exit_code=0`
- `processes-after.json`：空数组

### 持久化

目录：`../2026-11-12-wp4-native-release-persistence-red-v1`

- `durable-registration-red.exit.txt`：`exit_code=101`
- 测试：`native_release_registration_has_a_durable_actor_bound_fact`
- 结果：`0 passed; 1 failed`
- 失败为 `no such table: node_native_release_operations`，证明旧实现没有要求的持久事实。
- `processes-after.json`：空数组

## Green Check

目录：`../2026-11-12-wp4-native-release-persistence-green-v3`

| 检查 | 原始输出 | 结果 |
| --- | --- | --- |
| 真实认证 WSS 路由 | `actor-binding-green.stdout.txt` | 1 passed，exit 0 |
| schema 10 增量、重开与故障回滚 | `schema10.stdout.txt` | 2 passed，exit 0 |
| 持久仓储、认证代际与终态失败重试 | `repository.stdout.txt` | 2 passed，exit 0 |
| 原生子进程构建 | `native-child-build.stdout.txt` | exit 0 |
| 进程清理 | `processes-after-tests.json` | `[]` |

`../2026-11-12-wp4-native-release-persistence-green-v2` 是开发过程中的中间 Green，保留用于追溯，不作为最终结论。

## 同版回归

以下结果均由本目录的 `run.ps1` 在真实 Windows 当前用户环境、固定 source set 和隔离 `CARGO_TARGET_DIR` 下产生。

| 入口 | 测试数 | 退出码 | 原始输出 |
| --- | ---: | ---: | --- |
| `registration_owner_acceptance` | 51 | 0 | `registration-owner.stdout.txt` |
| `session_loop_acceptance` | 57 | 0 | `session-loop.stdout.txt` |
| `acp_tool_projection_acceptance` | 19 | 0 | `tool-projection.stdout.txt` |
| `acp_view_observations_acceptance` | 11 | 0 | `view-observations.stdout.txt` |
| `shared_input_queue_acceptance` | 39 | 0 | `shared-input-queue.stdout.txt` |
| socket-independent | 15 | 0 | `socket-independent.stdout.txt` |
| `node::shared_view_tests::` | 3 | 0 | `shared-view-real.stdout.txt` |

`processes-after-tests.json` 为 `[]`，两段回归结束后没有残留 `acp-remote` 或 `native_dispatch_dev_child` 进程。

`shared-view.stdout.txt` 使用旧过滤器时实际为 **0 个测试**，因此不作为通过证据；有效的共享视图检查以 `shared-view-real.stdout.txt` 的 3/3 为准。

## ACPI R01–R10

`run-acpi.ps1` 在同一 source set 和真实 Windows 当前用户环境串行运行：

| 范围 | 原始输出 | 结果 |
| --- | --- | --- |
| codec 与 R01–R09 | `acpi-r01-r09.stdout.txt` | 13 passed，0 failed，exit 0 |
| R10 完整 bundle 后才开放 listener | `acpi-r10-open-after-bundle.stdout.txt` | 1 passed，exit 0 |
| R10 服务拒绝时不开放 listener并完成 join | `acpi-r10-rejection-no-listener.stdout.txt` | 1 passed，exit 0 |
| R10 control-only 四命令均报告 `product_not_ready` | `acpi-r10-control-only.stdout.txt` | 1 passed，exit 0 |

`processes-after-acpi.json` 为 `[]`。

## 工程门禁

| 检查 | 结果 | 原始输出 |
| --- | --- | --- |
| `cargo fmt --check` | exit 0 | `cargo-fmt.stdout.txt` |
| `git diff --check` | exit 0，仅 LF/CRLF 提示 | `git-diff-check.stdout.txt` |
| OpenSpec `validate --all --strict` | 11/11 passed | `openspec-strict.stdout.txt` |
| Web `npm run check` | exit 0 | `web-check.stdout.txt` |
| Web `npm run test` | 32 文件、683 测试全过 | `web-test.stdout.txt` |
| Web `npm run build` | exit 0 | `web-build.stdout.txt` |

`web-dist-before.json` 与 `web-dist-after.json` 的 SHA-256 完全相同，构建前后 `dist` 的三个文件及内容哈希一致。

## 环境与负向记录

- `../2026-11-12-wp4-native-release-regression-v1/registration-owner.exit.txt` 为 `exit_code=101`。日志显示 Cargo 扫描仓库根目录的 `app/.tmp/q3-wp1-14-cldb-example-20261109b/raw-worker` 时收到 `os error 5`，尚未进入产品断言。该 ACL 目录保持原状，没有强制删除或改变所有权。
- 为避免旧 ACL 目录干扰，本版使用隔离源码副本；真实 Windows owner runner、认证 WSS、SQLite、子进程所有权和 Job 行为仍由原始测试执行，不以静态检查替代。

## 未关闭范围

- 1.4 仍受完整 Rust 检查、Clippy、依赖/供应链审查和可运行文档门禁阻断。本证据不把 414 条历史 Clippy 诊断改写成通过。
- 4.1 仍缺 CL-030、CL-031 及最终真实 Codex/DeepSeek、可信 Web、SQLite、Windows Job 组合验收。
- 4.2、5.x、6.4–6.7 和 6.10 仍待完成。
- 本报告由主执行生成；它只记录原始测试结果与限制，不是独立 tester/reviewer 签字，也不授权提交、推送、发布或归档。
