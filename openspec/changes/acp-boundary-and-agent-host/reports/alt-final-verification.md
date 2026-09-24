# 替代验证（Main E2E = not-applicable）— 任务 7.1 / 7.2

- 固定版本：`10353951193bb5993325b13c40f3abb8832a6637`（本地 `main`；`plan.md` 的 `alternative_checks` 四项即在此版本执行）。
- 项目开关回顾（任务 1.1）：`openspec-agentic e2e --json` 实测 `enabled = true`、`command = ""`、`maxAttempts = 3`；`plan.md` 的 mode = `not-applicable`，含 `reason`/`basis`/四项 `alternative_checks`/`downgrade_approval`（用户原话「1. 同意 2. 同意」，2026-09-24）。

## 7.1 四项替代检查（逐项命令、退出码、断言）

| # | 检查 ID | 命令 | 退出码 | 结果与断言 |
| --- | --- | --- | --- | --- |
| 1 | PV4 | `cargo test --locked -p acp-protocol -p agent-host --all-features` | **0** | 14 个测试目标全部 `ok`：**87 条 `test ... ok` / 0 failed**（`acp-protocol` 35 + `agent-host` 52：单元 3、catalog 21、session 13、supervision 15）；含 raw 逐字节保真、未知判别子、能力门控、进程树（Windows）与 stderr 有界等断言 |
| 2 | PV2/PV3 | `npm run check` | **0** | 十道合同门禁全绿：schemas（117 valid/23 invalid/39 views）、commands（12）、errors（58）、features（11）、assets（17 schemas/155 fixtures/12 transcript vectors/20 negatives/2 SAS）、acp（25 methods/11 updates/5 content blocks/3 tool content types/19 capabilities/8 invariants/10 families/71 rows）、docs（367 links/2811 refs/143 md）、boundaries（8 crates 与 §5 矩阵一致）、drift（36 DDL + 15 trait/87 签名）、agentic（安装校验 + 6 passed/0 failed） |
| 3 | PV1 | `npm run verify`（= `npm run check` + `cargo fmt --all -- --check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` + `cargo test --locked --workspace --all-features`） | **0** | fmt 无差异；clippy 零告警；**全工作区 378 passed / 0 failed / 2 ignored**（`crash_child` 由崩溃恢复用例在进程内生成；`regenerate_v2_fixtures` 是手工夹具重生成器——两者都带说明性 ignore 理由，属既有 crate 的既有安排，不在本变更新增） |
| 4 | PV5 | `cargo test --locked -p agent-host --all-features tree_`（**本机 Windows x64**） | **0** | `2 passed, 0 failed, 13 filtered out`：`tree_forced_termination_stops_the_whole_process_tree`（友好关闭路径）与 `tree_terminate_while_parent_alive_stops_parent_and_grandchild`（父仍活着时结束整棵树，父退出且孙停止心跳）。**Linux 只覆盖 unix 分支（`cargo check --target x86_64-unknown-linux-gnu` 通过），运行行为由 Linux CI 覆盖，本轮不在本机执行。** |

## 7.2 汇总、资源清理与哈希一致性

**断言覆盖核对**：`plan.md` 的四项 `alternative_checks` 与上表 1–4 一一对应，未遗漏、未替换；`not-applicable` 的判据（无必须运行的 E2E 任务、7.3 只确认固化）已在 `reports/du1-integration.md` 的 E2E 段核对。

**fixture / 矩阵哈希前后一致性**（执行前 → 执行后，逐字节）：

| 对象 | 执行前 | 执行后 | 结论 |
| --- | --- | --- | --- |
| `fixtures/acp/v1/**`（排序后逐文件 sha256 再聚合） | `e5ce7329be68640e268c659dd411c468f530e0352b81ab671fe4850eab380a42` | 同值 | **不变**（执行未改写夹具） |
| `compatibility/acp/v1/matrix.json` | `c79061ef8403acfe3fec724fad767a1b39dc08e43bee1341027d7e0c46d41575` | 同值 | **不变** |

**资源清理结论**：

- 子进程：`tasklist` 中无 `acpr-fake-acp-agent.exe` 残留（计数 0）；集成 Agent 的独立 worktree 已 `git worktree remove --force`，`git worktree list` 仅剩主工作区。
- `CARGO_TARGET_DIR` 未设置（`unset`），全部构建使用默认 `target/`，无共享/预置 target 造成的不确定性。
- **发现的残留（如实记录，未静默忽略）**：fake child 的 `--dump-env` 快照由用例写在系统临时目录的 `acpr-agent-host-env-*.txt`，测试**不自行删除**（失败路径的用例断言文件不存在，成功路径的会留下）。一次全量 `cargo test -p agent-host` 约留下 32 个此类文件；清理前系统临时目录里共有 32 个 `acpr-agent-host-env-*`（已全部人工删除，现为 0）。
- **口径澄清（避免夸大）**：系统临时目录里另有 **9454 个** `acpr-*` 条目（如 `acpr-storage-migrate-fresh-*`、`acpr-storage-imported-*` 等，各约 139 个），它们是**既有 crate（`storage-sqlite` 等）测试留下的目录**，不是本变更的产物，也不在本变更的清理范围内；本变更只对自己产生的 `acpr-agent-host-env-*` 负责。
- 影响面仅为临时目录卫生（不涉及仓库、不涉及产物、不影响任何断言强度），按 MINOR 登记为后续（建议：在 `tests/support/mod.rs` 用带 `Drop` 的临时文件守卫替代裸路径拼接）。

## 未执行 / 不在本机覆盖

- Linux 上 `#[cfg(unix)]` 的**运行**行为（本机只做编译核验）；`cargo-deny`（依赖许可证/advisory）与 `gitleaks`（密钥扫描）**只在 CI 运行**，本地无等价物，不得声称通过。
- 真实 Codex / Oh My Pi 兼容性套件（属可选兼容性套件，本变更的普通测试全部使用 fake ACP child）。
