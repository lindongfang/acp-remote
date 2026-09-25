# 集成 Agent · Phase A 报告（候选构造与候选 Project Verify，5.1/6.1/6.2/6.3）

> 由 integrator 子 Agent 输出工件原样落盘（运行 6d132632），主 Agent 未改动结论。

Phase A 完成。候选已固定，[PV1]–[PV5] 全绿，未做任何代码/规划改动。

## Agent 与工作区
- **Agent**：独立集成子 Agent（角色 `integrator`），宿主 pi；会话 `PI_SESSION_ID=01a0d88f-2a78-7138-b91a-8b19eaeeb5a8`，`PI_SUBAGENT_CHILD=1`，父会话 `01a0d761-fd04-76f2-a9bb-45e6b748f9d3`。**新起子 Agent，不继承实现/测试/reviewer 对话**（`roles/integrator.md` 全文 + plan.md + 契约/证据清单即全部输入）。宿主未暴露更细的运行 ID 别名 → 以上会话标识为可核实标识。
- **工作目录**：`D:\Project\acp-remote`（唯一 worktree，分支 `feat/daemon-cli-and-local-admin`；按 plan.md 第 654 行定案复用主 worktree，串行独占）。
- **提交**：source/candidate `90c816a6f88fb7925c84fbca269b1c946745e9e3`；base = **main** `ab62773d8768f3c6a8424f6aefa862a738482eb8`（与 `verification.md` 1.1 记录一致；`merge-base --is-ancestor main HEAD` = 0 ⇒ 无冲突解决差异）。基线与候选在本轮**开始与结束时均未移动**，工作区两次 `status --porcelain` 为空。

## 逐项命令与退出码（全部在候选提交上执行，完整输出在 `reports/candidate-verify.log`）
| 项 | 命令 | EXIT |
|---|---|---|
| 6.2 | `cargo build --locked --workspace --all-features`；vendor `cargo build` | 0 / 0 |
| PV1 | `npm run verify`（另单独留 4 子项：`npm run check`=0、`cargo fmt --all -- --check`=0、clippy workspace=0、`cargo test --workspace`=0） | **0** |
| PV2 | `node scripts/check-crate-boundaries.mjs` | **0**（12 crate 与 §5 一致） |
| PV3 | `cargo test --locked -p server --all-features` | **0**（7 目标 / 117 passed / 0 failed / 0 ignored） |
| PV4 | `cargo test --locked -p app --all-features` | **0**（6 目标 / 72 passed / 0 failed / 0 ignored） |
| PV5 | vendor `cargo test --all-features` | **0**（11 passed） |

计数：**82 个测试目标 / 718 passed / 0 failed / 2 ignored**（`crash_child`、`regenerate_v2_fixtures` 自述忽略）；plan 的 PV5 平台命令另有 server 3 passed / app **0 passed**（名称过滤命中问题，见下）。日志 3033 行，`sha256=5215a54c4583c60800c8861573cafdd59ad3897262ee380def4567b2055d75e4`，写后未再改动。

## 候选 vs 目标基线新增交互面（供 6.4 reviewer）
`git diff --name-status main..HEAD` = **105 文件**（A=86/M=19）。要点：新 crate `crates/server/**`(29)、`crates/app/**`(19)，均加入 members（`cargo metadata` members=12）；workspace 外 path crate `vendor/windows-local-ipc/**`(4) 经 `exclude`；登记面 `Cargo.toml`（members +2、exclude +1、tokio/nix features、clap/toml/fs4/rpassword）、`Cargo.lock`(+287 行 / 273 package)、`deny.toml`(+14 行说明，未改 allow/deny 键)、`.gitignore`(+7)、`openspec/config.yaml`；既有 crate 唯一功能改动是 `storage-sqlite` 的 `AuditStore`（不改 DDL）；词表资产 `envelope.schema.json`/`check-command-catalog.mjs`/local-admin fixtures；文档 6 个 + README/AGENTS。

## 未解决项 / 下一步
- **F1（P2）**：plan 的 PV5 平台命令是测试名过滤 ⇒ app 轮 0 命中、server 轮 3 命中；该场景真实覆盖在同一轮 [PV3]（`local_endpoint_windows.rs` 4 passed）与 [PV4]（`daemon_lifecycle.rs` 10 + `cli_commands.rs` 11，真实子进程+pipe）。建议主 Agent 在 `verification.md` 注明并以 PV3/PV4 作为该场景证据（我未改规划文件）。
- **F2（P2）**：`verification.md` 记录冻结轮「730 passed」，原始 `wp4wp5-final-verify.log` 实测为 **717**（本次候选 718）；属文档计数偏差，待主 Agent 处置。
- **F3（P3，范围外）**：`storage-sqlite` 测试自 2026-09-18 起累计遗留 16191 个 `/tmp/acpr-*` 临时目录（早于本基线 ⇒ 既有行为），未删除，仅登记。
- **Phase B 未做**（等主 Agent 明确交接）：premerge 门（需先固化候选证据块）、合入 `refs/heads/main`（须复核仍为 `ab62773…`）、6.7 主分支回归、6.8 检视。候选 PASS **不等于**已合入或最终验收 PASS。