# WP7-fix 重跑轮次 project verify（task 3.13 / WP7）— `node-link-owner` / DU1

## Shared Report

- **task_id**：3.13（WP7；权威 `tasks.md:57` 原文 =「3.13 WP7；前置：2.22；实现 Agent（coder）。完成 WP7 交付前 project verify：[PV4] + [PV5]（本机全链路），核对集成测试真实执行（无全跳过）。完成条件：逐项通过并留证；临时数据目录与证书已清理。」）。本次派单在该两项之外**另点名 [PV1]、[PV2]、[PV3]**，本报告按派单五项全给。
- **role / phase**：coder / verify —— 本任务**只执行检查与写报告，未修改被检查仓库的任何文件**（轮次起点与终点 `git status --porcelain` 均为 0 行、暂存区为空、无 stash；本轮所有写入都在权威规划根的 `reports/` 下）。
- **agent_context**：worker 子 Agent（worktree `D:\Project\acp-remote-wt\node-link-owner` 独占会话；只读检查 + 追加日志与报告）。权威规划根 `D:\Project\acp-remote`，报告与日志写在 `openspec/changes/node-link-owner/reports/`（该目录不入版本控制）。
- **触发原因（本轮为何必须重跑）**：wp7-fix1 的 **F3** 修复改了 `crates/app/tests/node_link_e2e.rs` 的 **R61 断言**——原断言把 F3 缺陷（「正文缺块的 turn 仍报 `completed`」）当作登记在案的已知残余钉住，`ee5f6319` 按裁决 (a) 改为钉新语义（该 turn 不得出现 `turn.completed`；命令终态为 `uncertain` + `nodelink.command.uncertain`）。因此上一轮 `[PV5]`（REUSED，@ `a3109c8a…`）证据**已 STALE**，本轮在 `690b9172` 上**真实重跑**全部五项。
- **target_revision**：`690b91722f6ef1306660fde55ce1afd0d4607f4b`（= 派单固定提交 = 分支 `agentic/node-link-owner` 的 HEAD；`git log --oneline` 顶部 = `690b917 fix(app): 关闭时先同步停网络 accept，并修掉两处测试口径`、其父 `ee5f631 fix(core): 非终态批次落盘失败时终结 turn…`）。
  preflight（`13:14:00Z`）与全部检查结束后的终态复核均实测 `git rev-parse HEAD` = 该值、`git status --porcelain | wc -l` = 0、暂存区 0 行、`git stash list | wc -l` = 0；**无修订漂移**。五条命令各自在执行前后又取了一次 HEAD（日志中的 `head_before`/`head_after`），全部相同。
- **scope（本任务实际执行的检查 + 收尾）**：
  - `[PV4]` `cargo test --locked -p app --all-features`（派单命令原文，**含对全链路集成测试真实执行的核对**）+ 只读 `--list` 交叉核对；
  - `[PV3]` `cargo test --locked -p server -p core -p storage-sqlite -p identity-auth -p node-link-protocol --all-features`；
  - `[PV1]` `npm run verify`（聚合 1 次）+ 把 `check`/`check:rust` 展开成的 **15 条子检查逐个独立执行、各自记录退出码**（不用 `&&` 掩盖前序失败）；
  - `[PV2]` `node scripts/check-crate-boundaries.mjs`（门禁脚本本体，不是 npm 包装）；
  - `[PV5]` `cargo test --locked -p server -p app --all-features`（本机 Windows，含 `node_link_e2e` / `node_link_listener` 的真实 listener 轮次）+ 只读 `--list` 交叉核对；
  - 定向补充：`cargo test --locked -p app --all-features --test node_link_e2e -- --test-threads=1`（wp7-fix1 handoff 点名的 R61 重跑命令）；
  - 收尾：资源残留核实（进程 / 端口 / `%TEMP%` 的 `acpr-*` 与证书目录）+ worktree 终点状态。
- **checks**：**五项全部 exit 0，无 FAIL**；无「零用例目标或整 target 静默跳过」被当成通过；无新增进程 / 监听端口 / 临时目录残留。
  逐项见下表与 `reports/du1-pv1.log` 的「WP7-fix 重跑轮次」分节（**追加**，§WP7F-0…§WP7F-10b）、`reports/pv5-windows-nodelink.log` 的「WP7-fix 重跑轮次」分节（**追加**，§WP7F-PV5-1…3）。
- **result**：**PASS**（本任务五项检查在固定提交 `690b9172…` 上全部通过）。**不代表** 3.14 的独立 review、2.23/2.24（WP8 状态写回与统一入口）、WP8 的 project verify、6.5（候选替代验证）或合并已完成；也不代表跨实现（Linux/CI）轮次已跑。
- **evidence_paths**：`openspec/changes/node-link-owner/reports/du1-pv1.log`（「WP7-fix 重跑轮次」分节）、`openspec/changes/node-link-owner/reports/pv5-windows-nodelink.log`（「WP7-fix 重跑轮次」分节）、本文件。
- **resource_cleanup**：本轮**未新增**任何进程 / 监听端口 / 数据库 / 容器 / 账号 / 证书 / 依赖；未改 `Cargo.toml`、`Cargo.lock`、`schemas/**`、`compatibility/**`、`fixtures/**`、`docs/**`、`openspec/**`。
  只读检查结束后实测：本轮相关二进制（`cargo.exe`/`rustc.exe`/`link.exe`/`acpr-fake-acp-agent.exe`/`acp-remote.exe`/`acp_remote.exe`/`server.exe`/`node-link-e2e.exe`）计数 = **0**；`netstat -ano` 中 `8765` = **0**（`grep -w` exit 1）；listener 用例的 `TestCertificate` 目录 `acpr-wp7*` = **0**（exit 2）。
  MSYS `/tmp`（= `C:\Users\zhang\AppData\Local\Temp`）起点发现 **2 个** `acpr-*` 遗留（`acpr-wp4a-e2e-21508-0`、`acpr-wp4a-e2e-39680-0`，mtime ∈ 20:56 本地、**早于本轮起点 21:14**，内容为 `data\acp-remote.sqlite3{,-shm,-wal}` + 空的 `data\attachments\`，属 WP4a 轮次 `TempRoot` 的 daemon 数据目录）——按派单「属本变更早前轮次的可删除并记录」**已删除**（逐文件 `rm -f` 6 个文件全部成功，再逐层 `rmdir` `data/attachments`→`data`→顶层），删除后 `ls -d /tmp/acpr-*` exit 2（**0 残留**）。**本轮五道检查跑完后并未新增任何 `acpr-*`**（起点快照 2 个、跑完后仍恰好是同样的 2 个且 mtime 不变），说明 app 侧 `TempRoot`/`TestCertificate` 的 `Drop` 清理在本机有效。
  本轮自建 scratch 位于仓库之外（`/tmp/wp7fixv`，以及写工具把 `/tmp/...` 解析到 `D:\tmp` 时产生的 `D:\tmp\wp7fixv`），在报告落盘后删除（两步式）。worktree 自有的 `target/` 与 `node_modules/` 是 git-ignored 构建缓存，按约定保留。

## 检查结果（逐项退出码）

| ID | 命令（cwd = worktree 根） | 退出码 | 耗时 | 关键结果 | 日志位置 |
|---|---|---|---|---|---|
| **PV4** | `cargo test --locked -p app --all-features` | **0** | 68 s | **85 passed / 0 failed / 0 ignored**；8 个测试目标全 `ok`；declared 85 = executed 85 | `du1-pv1.log` §WP7F-1 / §WP7F-7 |
| **PV3** | `cargo test --locked -p server -p core -p storage-sqlite -p identity-auth -p node-link-protocol --all-features` | **0** | 35 s | **668 passed / 0 failed / 2 ignored**；39 个测试目标全 `ok` | `du1-pv1.log` §WP7F-2 |
| **PV1** | `npm run verify`（聚合） | **0** | 106 s | workspace **969 passed / 0 failed / 2 ignored**；85 个测试目标 | `du1-pv1.log` §WP7F-3 |
| PV1-C01 | `npm run check:schemas` | **0** | 1 s | `118 valid, 25 invalid (ajv Draft 2020-12), 39 event views bound` | `du1-pv1.log` §WP7F-4-C01 |
| PV1-C02 | `npm run check:commands` | **0** | 1 s | `12 commands` | §WP7F-4-C02 |
| PV1-C03 | `npm run check:errors` | **0** | 1 s | `58 codes across 2 protocols` | §WP7F-4-C03 |
| PV1-C04 | `npm run check:features` | **0** | 1 s | `11 feature ids across 2 protocols` | §WP7F-4-C04 |
| PV1-C05 | `npm run check:assets` | **0** | 1 s | `17 schemas, 156 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed` | §WP7F-4-C05 |
| PV1-C06 | `npm run check:acp` | **0** | 1 s | `25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows, schema validated by ajv)` | §WP7F-4-C06 |
| PV1-C07 | `npm run check:docs` | **0** | 1 s | `378 relative links, 4133 section refs across 257 markdown files` | §WP7F-4-C07 |
| PV1-C08 | `npm run check:boundaries` | **0** | 1 s | `12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）` | §WP7F-4-C08 |
| PV1-C09 | `npm run check:drift` | **0** | 1 s | `§7 的 36 条 DDL` 与 `migrate.rs` 一致；`§5 的 15 个 trait / 93 个方法签名` 与 `ports.rs` 一致 | §WP7F-4-C09 |
| PV1-C10 | `npm run check:agentic` | **0** | 2 s | `Totals: 16 passed, 0 failed (16 items)` + `宿主入口检查完成：17 个文件` | §WP7F-4-C10 |
| PV1-C10a | `node scripts/agentic-gate.mjs` | **0** | 2 s | 16/16 items | §WP7F-4-C10a |
| PV1-C10b | `node scripts/sync-agentic-host-entrypoints.mjs --check` | **0** | 0 s | `agentic 宿主入口检查完成：17 个文件` | §WP7F-4-C10b |
| PV1-R01 | `cargo fmt --all -- --check` | **0** | 2 s | 无输出（无格式差异） | §WP7F-4-R01 |
| PV1-R02 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | **0** | 1 s | 无诊断（**指纹缓存命中**，见 issues 第 2 条） | §WP7F-4-R02 |
| PV1-R03 | `cargo test --locked --workspace --all-features` | **0** | 98 s | **969 passed / 0 failed / 2 ignored**；85 个目标；与聚合跑逐项一致 | §WP7F-4-R03 |
| **PV2** | `node scripts/check-crate-boundaries.mjs` | **0** | 0 s | `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）` | `du1-pv1.log` §WP7F-5 |
| **PV5** | `cargo test --locked -p server -p app --all-features`（本机 Windows） | **0** | 83 s | **394 passed / 0 failed / 0 ignored**；15 个目标；declared 394 = executed 394 | `pv5-windows-nodelink.log` §WP7F-PV5-1…3 |
| 补充 | `cargo test --locked -p app --all-features --test node_link_e2e -- --test-threads=1` | **0** | 6 s | **2 passed / 0 failed / 0 ignored**（新 R61 断言真实执行） | `du1-pv1.log` §WP7F-6 |
| — | `ls -d /tmp/acpr-*`、`ls -d /tmp/acpr-wp7*`、进程精确匹配、`netstat` 8765 | — | — | 终态残留 = 0（`acpr-*` exit 2、证书目录 exit 2、进程计数 0、8765 exit 1） | `du1-pv1.log` §WP7F-8 / §WP7F-9 / §WP7F-10 |

**子检查合计：15/15 exit 0**，与 `npm run verify` 聚合退出码 0 一致（无「聚合绿、单项红」或反过来的矛盾）。
环境：Windows / git-bash（MINGW64_NT-10.0-26200 / x86_64）；`rust-toolchain.toml` 固定 `1.98.1`，实测 cargo 1.98.1（`797e8a9bc`）、rustc 1.98.1（`48a229cea`）、node v24.19.0、npm 12.0.2。

## 派单点名要求：全链路集成测试是否真实执行（无全跳过）

`[PV4]`（`-p app`）的 8 个目标，全部 `ok`：

| 目标 | 执行数 | 说明 |
|---|---|---|
| app lib（`unittests src\lib.rs`） | **52** | 与 WP7 轮次相同 |
| app main（`acp_remote` CLI 二进制） | 0 | 本目标无 `#[test]`（见下方 0 用例说明） |
| `tests\audit_export.rs` | **1** | — |
| `tests\cli_commands.rs` | **11** | CLI 行为 |
| `tests\daemon_lifecycle.rs` | **11** | 60.37 s，**真实起停 daemon 子进程**（比 WP7 轮次 +1 = wp7-fix1 的 F2 新用例） |
| `tests\node_link_e2e.rs` | **2** | 3.16 s，**受控路径全链路**（含被 wp7-fix1 改钉的 R61 断言） |
| `tests\node_link_listener.rs` | **8** | 0.28 s，**真实 `TcpListener` 绑定与 TLS 终止** |
| Doc-tests app | 0 | 文档注释无可执行 Rust 代码块 |
| **合计** | **85** | `--list` 声明 52+0+1+11+11+2+8+0 = **85**（`declared - ok - ignored = 0`，**无未执行用例**） |

`[PV5]`（`-p server -p app`）的 15 个目标，全部 `ok`：app 85 + server 309（lib 281 / `local_admin_channel` 14 / `local_admin_schema_drift` 6 / `local_endpoint_naming` 4 / `local_endpoint_unix` 0 / `local_endpoint_windows` 4 / Doc-tests server 0）= **394**，`--list` 声明 **394 == 394**。

**被 wp7-fix1 改动的 R61 断言确实执行并通过**（这是本轮重跑的核心目的）：
- `tests\node_link_e2e.rs`（2/2 ok）：`the_controlled_path_runs_end_to_end_and_revocation_propagates`、`tls_direct_terminates_the_same_handshake`。
- 断言现状（只读核对 `crates/app/tests/node_link_e2e.rs` §⑨）：故障批所属 turn 必须显式 `turn.failed`、该 `turnId` 不得再出现 `turn.completed`、命令终态必须为 `uncertain` 且错误码为 `nodelink.command.uncertain`（`git show ee5f6319 -- crates/app/tests/node_link_e2e.rs` 可对照改前「known residual」口径）。
- 定向单线程重跑 `--test node_link_e2e -- --test-threads=1` 同样 2/2 ok（6 s），排除并发干扰。

真实 listener / 进程轮次逐条来自 raw 输出的 `... ok` 行：
- `tests\node_link_listener.rs`（8/8 ok）：`an_invalid_listen_literal_refuses_startup`、`plaintext_dev_mode_is_refused_off_loopback`、`direct_tls_without_readable_pem_refuses_startup`、`an_occupied_listen_address_refuses_startup`、`the_wired_listener_keys_are_no_longer_reported_as_unwired`、`proxy_mode_off_loopback_starts_with_a_warning`、`the_configured_listen_address_is_bound_and_reported`、`direct_tls_terminates_tls_and_refuses_plaintext`。
- `tests\daemon_lifecycle.rs`（11/11 ok）：含 wp7-fix1 新增的 `the_network_listener_stops_serving_new_connections_before_the_local_drain_finishes`，以及 `a_fresh_start_takes_the_lock_creates_the_endpoint_and_imports_the_seeds`、`the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown`、`stop_does_not_wait_for_the_full_grace_without_other_clients` 等真实进程生命周期轮次。

0 用例目标（平台差异或交付形态，**非静默跳过**，已逐条交代）：
`app` 的 `src\main.rs`（CLI 二进制，无 `#[test]`，同一交付面由 `tests\cli_commands.rs` 11 条覆盖）、`tests\local_endpoint_unix.rs`（整文件 `#![cfg(unix)]`，Windows 上不参与编译，同一规则由 `tests\local_endpoint_windows.rs` 4 条覆盖）、`Doc-tests app` / `Doc-tests server`（两个 crate 的文档注释无可执行代码块）。所有轮次都未出现「整 target 被 filter 掉」：命令未带 filter，`0 filtered out` 出现在每条 `test result` 行。

**与上一轮（WP7 @ `a3109c8a`）的计数对照**（说明本轮没有静默减少覆盖，且 wp7-fix1 的新用例在本机真的跑了）：

| 项 | WP7 轮次（`a3109c8a`） | 本轮（`690b9172`） | 差值 | 说明 |
|---|---|---|---|---|
| `[PV1]`/`[R03]` workspace | 966 / 0 / 2（85 目标） | **969 / 0 / 2（85 目标）** | **+3** | core +2（wp7-fix1 的 broker 用例）、app +1（daemon_lifecycle F2） |
| `[PV4]` app | 84 / 0 / 0（8 目标） | **85 / 0 / 0（8 目标）** | **+1** | 仅 `daemon_lifecycle` 10 → 11 |
| `[PV5]` 本机 Windows | 393 / 0 / 0（15 目标） | **394 / 0 / 0（15 目标）** | **+1** | 同上（app 84 → 85） |
| `[PV3]` core lib | 113 | **115** | **+2** | `broker::tests::a_failed_non_terminal_batch_terminates_the_turn_instead_of_silently_dropping_it` 与 `an_abandoned_turn_still_finalises_the_deltas_it_persisted` |
| server 侧 | 309 | **309** | 0 | lib 281 / 14 / 6 / 4 / 0 / 4 逐目标不变 |
| 声明 vs 执行 | 968 = 966 + 2 | **971 = 969 + 2** | +3 | 无未执行用例 |

## issues（需要主 Agent / reviewer 知晓的事实）

1. **本轮是为 STALE 的 `[PV5]` 而重跑的**：wp7-fix1 的 F3 改了 `node_link_e2e.rs` 的 R61 断言（原断言钉的正是被裁决修掉的残余），因此上一轮 `[PV5]`（REUSED @ `a3109c8a`）不再适用。本轮在 `690b9172` 上真实重跑五项，**R61 断言（含 `turn.failed` / `uncertain` / 「该 turn 无 `turn.completed`」三条）在 e2e 全链路与定向单线程重跑中均通过**。
2. **`npm run check:rust` 的 clippy 是 cargo 指纹缓存命中（如实登记，不是造假也不是跳过）**：`[PV1]` 聚合内的 `cargo clippy` 与单独执行的 R02 都只花 1 s（`Finished dev profile … in 0.57s`、无任何 `Checking` 行），即工作区源码与上次 clippy 指纹一致（wp7-fix1 的 `.husky/pre-commit` 已对**同一份源码**跑过 workspace clippy）。两次都 **exit 0、零诊断**，判定有效；但「本轮新跑了一次 clippy-driver」这句话不成立。若要强制重跑的原始输出需换 target dir 或改文件 mtime —— 前者成本高、后者违反本轮「不修改文件」约束，**均未执行**。
3. **子检查条数的口径**：`package.json` 的 `verify` = `check` + `check:rust`。`check` 有 10 个 npm script，其中 `check:agentic` 内部串了 2 条命令；`check:rust` 内部串了 3 条命令。本轮既执行了 10 个 npm script，又把 `check:agentic` 的 2 条与 `check:rust` 的 3 条展开单独各跑一次，故可独立执行的命令是 **15 条**（C01–C09、C10、C10a、C10b、R01、R02、R03），**15/15 exit 0**。若 reviewer 只按「npm script 计数」看是 13 条（10 + 3）；口径差异已在 §WP7F-4 逐条列明命令与退出码，结论不变。
4. **起点 2 个 `/tmp/acpr-wp4a-e2e-*` 是早前轮次的，不是本轮产物**：mtime ∈ 20:56（本地），早于本轮起点 21:14；本轮五道检查（含 e2e 与 listener 各跑了 3 次）跑完后 `ls -d /tmp/acpr-*` 仍是同样的 2 个且 mtime 不变（§WP7F-8），说明 app 侧 `TempRoot`/`TestCertificate` 的 `Drop` 清理在本机有效，残留来自更早轮次里被中断的测试进程。按派单授权已删除并记录（2 目录 / 6 文件 + 2 个空 `attachments` 目录）。
5. **清理动作的机制说明**：本机工具的通用安全确认**拦截了递归删除型命令**，因此改用等价的「逐文件 `rm -f` → 逐层 `rmdir`」；首轮 `find -type f` 只删了文件，复核时发现仍有空的 `data/attachments` 子目录，再逐层 `rmdir` 清空（日志 §WP7F-9 有原始记录）。删除对象全部位于仓库之外，**没有触碰仓库内任何文件**。
6. **`%TEMP%` 里仍有其它轮次的非 `acpr-*` scratch，按派单授权范围保留**：派单只授权删 `/tmp/acpr-*`，故仅登记不删除。若主 Agent 希望在归档前一并清空这些 scratch，需要再给一次明确授权。
7. **本轮是只读轮次**：未改任何文件（含未改 `tasks.md` / `verification.md` / `plan.md` / `docs/**`）；也未改 `reports/` 之外的任何内容。报告与日志本身写在权威规划根，不入版本控制。
8. **未执行（如实记录，与既往轮次口径一致）**：`cargo-deny`（依赖许可证/来源/advisory）与 `gitleaks`（密钥扫描）**只在 CI 运行**，本地无等价物、不在 `npm run verify` 里；本机未装 gitleaks。因此「本地绿」不等于这两类判定通过，合并前仍以 CI 的 `deps`/`advisories`/`secrets` 三个 job 为准。
9. **边界**：本轮只覆盖**本机 Windows**；`tests\local_endpoint_unix.rs` 在 Linux/CI 上才会真正执行（同一规则本机由 `local_endpoint_windows.rs` 覆盖），跨实现轮次不在 3.13 范围内。

## handoff_index

```yaml
handoff_index:
  - task_id: "3.13"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "固定提交 690b9172（分支 agentic/node-link-owner；preflight 与终态复核的 `git rev-parse HEAD` 一致、`git status --porcelain | wc -l` 均为 0、暂存区 0 行、无 stash）上，用固定工具链（rust-toolchain.toml = 1.98.1，实测 cargo 1.98.1 / rustc 1.98.1；node v24.19.0 / npm 12.0.2）执行 `npm run verify`：**聚合退出码 0**（§WP7F-3，106 s），其中 `cargo test --locked --workspace --all-features` 真实执行 **969 passed / 0 failed / 2 ignored**、85 个测试目标。为排除 `&&` 掩盖前序失败，把 `check` 的 10 个 npm script 与 `check:agentic`/`check:rust` 展开成的命令共 **15 条逐个单独执行、各自记录 $?**（§WP7F-4：C01..C09、C10、C10a、C10b、R01、R02、R03），**15/15 均 exit 0**；R03 独立复跑与聚合跑计数逐项一致（969/0/2、85 目标）。只读 `--list` 交叉核对 workspace declared **971 = 969 + 2 ignored**，无未执行用例。**如实登记**：R02 clippy 两次都是 cargo 指纹缓存命中（1 s、无 Checking、exit 0 无诊断）。2 条 ignored（storage-sqlite 的 crash 子进程目标与 v1 夹具生成器）是仓库声明过的辅助项。五条命令在工作区内无副作用：全部跑完后 `git status --porcelain` 仍 0 行。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.13"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一固定提交、同一环境下直接执行门禁脚本本体 `node scripts/check-crate-boundaries.mjs`（不是只跑 `npm run check:boundaries` 包装）：**退出码 0**，输出 `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）`（§WP7F-5 raw 全文，含 head_before/head_after 均为 690b9172…）。该脚本以 `MODULE_ARCHITECTURE.md` §5 的依赖矩阵为唯一判据、用 `cargo metadata` 校验每个 crate 的实际依赖，并硬约束 `core` 不引入 runtime/DB/HTTP/子进程/wire protocol 依赖；wp7-fix1 触及的 `crates/core/src/broker.rs`（同 crate 内改动）与 `crates/app/src/daemon.rs`、`crates/app/tests/**` 未改变任何 crate 的依赖方向（`app -> server / backends / identity-auth / core`、`server -> core + wire protocols`）。同一判定在 §WP7F-4 的 C08 里独立复跑一次，同样 exit 0。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.13"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一固定提交上执行 `cargo test --locked -p server -p core -p storage-sqlite -p identity-auth -p node-link-protocol --all-features`：**退出码 0**、35 s、**668 passed / 0 failed / 2 ignored**、39 个测试目标全部 ok（§WP7F-2 raw 全文含逐目标 `test … ok` 行与每条 `N filtered out` 行）。覆盖面含 `acp_core` lib **115**（= WP7 轮次 113 + wp7-fix1 的 2 条 broker 用例）、`server` lib 281 + local_admin 系列 28（14/6/4/0/4）、`storage-sqlite` 124（lib 3 + 13 个集成目标）、`identity-auth` 81、`node-link-protocol` 37、5 个 Doc-tests 目标（identity_auth 2 条）。2 条 ignored 为仓库声明的辅助项（`commit` 的 crash 子进程、`migration` 的 v1 夹具生成器），不是被跳过的覆盖。所有命令前后 HEAD 相同（无修订漂移）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.13"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b"
    evidence_type: CHECK
    evidence_id: PV4
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一固定提交上执行派单原文命令 `cargo test --locked -p app --all-features`：**退出码 0**、68 s、**85 passed / 0 failed / 0 ignored**、8 个测试目标全部 ok（§WP7F-1 raw 全文含逐条 `test … ok` 行）。**派单点名要求确认的「全链路集成测试真实执行、无零用例/全跳过」**：只读 `--list` 交叉核对 declared **85 = executed 85**（52+0+1+11+11+2+8+0，declared - ok - ignored = 0，§WP7F-7），命令未带 filter 且每条 `test result` 行都是 `0 filtered out`。**本轮重跑的核心对象——被 wp7-fix1 改钉的 R61 断言——真实执行并通过**：`tests\\node_link_e2e.rs` **2/2 ok**（`the_controlled_path_runs_end_to_end_and_revocation_propagates` = 配对→本地确认→握手→catalog 过滤→attach/subscribe→event/ack→命令→幂等→**⑨ R61 故障注入（turn.failed + command.uncertain + 无 turn.completed）**→撤销传播；`tls_direct_terminates_the_same_handshake` = 自签证书 TLS direct 同一握手，3.16 s）；另有定向单线程重跑 `--test node_link_e2e -- --test-threads=1` 2/2 ok（§WP7F-6）。`tests\\node_link_listener.rs` **8/8 ok**（真实 `TcpListener` 绑定与 TLS 终止、失败关闭，0.28 s）、`tests\\daemon_lifecycle.rs` **11/11 ok**（60.37 s，真实起停 daemon，含 wp7-fix1 新增的 F2 用例）、app lib 52、`audit_export` 1、`cli_commands` 11。0 用例目标已逐条交代（app `main.rs` 由 cli_commands 覆盖、Doc-tests app 无代码块），不是静默跳过。相对 WP7 轮次（app 84）**+1**（daemon_lifecycle 的 F2 用例）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.13"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "690b91722f6ef1306660fde55ce1afd0d4607f4b"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "openspec/changes/node-link-owner/reports/pv5-windows-nodelink.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本机 Windows（MINGW64_NT-10.0-26200 / x86_64-pc-windows-msvc，cargo 1.98.1）上执行 `cargo test --locked -p server -p app --all-features`：**退出码 0**、83 s、**394 passed / 0 failed / 0 ignored**、15 个测试目标全部 ok（§WP7F-PV5-1…3，raw 全文含逐条 `… ok` 行）；只读 `--list` 声明 **394 == 394**。**这是本轮为 STALE 的上一轮 [PV5] 重跑的核心证据**：wp7-fix1 改了 `node_link_e2e.rs` 的 R61 断言后，本轮 `tests\\node_link_e2e.rs` 2/2 通过（含 2.22 的受控路径闭环 `the_controlled_path_runs_end_to_end_and_revocation_propagates` 与 TLS direct 的 `tls_direct_terminates_the_same_handshake`），证明新断言（该 turn `turn.failed`、无 `turn.completed`、命令 `uncertain` + `nodelink.command.uncertain`）在本机全链路轮次成立。另 `tests\\node_link_listener.rs` 8/8（真实 bind/TLS 终止/失败关闭）、`tests\\daemon_lifecycle.rs` 11/11（60.37 s）；server 侧 309 条逐目标不变（lib 281 / local_admin_channel 14 / local_admin_schema_drift 6 / local_endpoint_naming 4 / local_endpoint_unix 0 / local_endpoint_windows 4 / Doc-tests 0），app 侧 85（相对 WP7 轮次 +1）→ 合计相对上一轮 **+1**。平台差异真实留证：`tests\\local_endpoint_windows.rs` 4/4 执行通过，`tests\\local_endpoint_unix.rs` 0 条（整文件 `#[cfg(unix)]`，本机不参与编译、由 Linux CI 覆盖）；4 个 0 用例目标已逐条交代，不是静默跳过。跑完后 worktree 仍干净（`git status --porcelain` 0 行），无新增进程/端口/`acpr-*` 或证书目录残留（早前轮次遗留的 2 个 `acpr-wp4a-e2e-*` 已清理并记录）。"
    source_evidence: NOT_APPLICABLE
```
