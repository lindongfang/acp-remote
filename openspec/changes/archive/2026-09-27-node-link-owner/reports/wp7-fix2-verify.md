# WP7-fix2 重跑轮次验证（task 3.13 / WP7）— `node-link-owner` / DU1

## Shared Report

- **task_id**：3.13（WP7；权威 `tasks.md:57` 原文 =「3.13 WP7；前置：2.22；实现 Agent（coder）。完成 WP7 交付前 project verify：[PV4] + [PV5]（本机全链路），核对集成测试真实执行（无全跳过）。完成条件：逐项通过并留证；临时数据目录与证书已清理。」）。本次派单在该两项之外**另点名 [PV1]、[PV2]、[PV3]**，本报告按派单五项全给。
- **role / phase**：coder / verify —— 本任务**只执行检查与写报告，未修改被检查仓库的任何文件**（轮次起点与终点 `git status --porcelain` 均为 0 行、暂存区为空、无 stash；本轮所有写入都在权威规划根的 `reports/` 下）。
- **agent_context**：worker 子 Agent（worktree `D:\Project\acp-remote-wt\node-link-owner` 独占会话；只读检查 + 追加日志与报告）。权威规划根 `D:\Project\acp-remote`，报告与日志写在 `openspec/changes/node-link-owner/reports/`（该目录不入版本控制）。
- **触发原因（本轮为何必须重跑）**：wp7-fix2（RV2-WP7 的 F1/F3）改了 `crates/core/src/broker.rs` 的**会话槽占位语义与会话状态投影**（被放弃的 turn 继续占住 `running`、`dispatch_one` 在占位期不派发下一个排队 turn、仍有排队 turn 时放弃提交把会话投影为 `Queued`），并新增 3 条 broker 用例（F1 两条判别器 + F3 一条边界用例）。`wp7-fix2-handoff.md` 的 `handoff_index` 已把 `[PV5]` 明确记为 **PENDING / STALE**。因此上一轮 `[PV1]`–`[PV5]`（@ `690b9172…`）证据全部不再适用，本轮在 `5c869160` 上**真实重跑五项**。
- **target_revision**：`5c869160e8dab0361b248124f631beba239183c6`（= 派单固定提交 = 分支 `agentic/node-link-owner` 的 HEAD；`git log --oneline -3` 顶部 = `5c86916 test(app): 修正故障注入替身的注释口径`，其父 `008dcef fix(core): 被放弃的 turn 继续占住会话槽直到终态被端点观测到`，再父 `690b917`）。
  preflight（`13:39:47Z`）与全部检查结束后的终态复核（`13:48:33Z`）均实测 `git rev-parse HEAD` = 该值、`git status --porcelain | wc -l` = 0、暂存区 0 行、`git stash list | wc -l` = 0；**无修订漂移**。五条命令各自在执行前后又取了一次 HEAD（日志中的 `head_before`/`head_after`），全部相同。
- **scope（本任务实际执行的检查 + 收尾）**：
  - `[PV3]` `cargo test --locked -p server -p core -p storage-sqlite -p identity-auth -p node-link-protocol --all-features` + 只读 `-- --list` 交叉核对；
  - `[PV4]` `cargo test --locked -p app --all-features`（派单命令原文，含对全链路集成测试真实执行的核对）+ 只读 `-- --list` 交叉核对；
  - `[PV1]` `npm run verify`（聚合 1 次）+ 把 `check`/`check:rust` 展开成的 **15 条子检查逐个独立执行、各自记录退出码**（不用 `&&` 掩盖前序失败）；
  - `[PV2]` `node scripts/check-crate-boundaries.mjs`（门禁脚本本体，不是 npm 包装）；
  - `[PV5]` `cargo test --locked -p server -p app --all-features`（本机 Windows，含 `node_link_e2e` / `node_link_listener` 的真实 listener 轮次）+ 只读 `-- --list` 交叉核对；
  - 定向补充：`cargo test --locked -p app --all-features --test node_link_e2e -- --test-threads=1`（wp7-fix2 handoff 点名的约定形式，共跑 2 次：`du1-pv1.log` §WP7F2-6 与 `pv5-windows-nodelink.log` §WP7F2-PV5-3）；`cargo clippy -v` 只读核实 12 个 workspace crate 的 Fresh 指纹；
  - 收尾：资源残留核实（进程 / 端口 / `/tmp` 的 `acpr-*` 与证书目录）+ worktree 终点状态 + 自建 scratch 清理。
- **checks**：**五项全部 exit 0，无 FAIL**；15/15 子检查各自 exit 0，与 `npm run verify` 聚合退出码 0 一致；无「零用例目标或整 target 静默跳过」被当成通过；无新增进程 / 监听端口 / 临时目录残留。
  逐项见下表与 `reports/du1-pv1.log` 的「WP7-fix2 重跑轮次」分节（**追加**，§WP7F2-0…§WP7F2-10）、`reports/pv5-windows-nodelink.log` 的「WP7-fix2 重跑轮次」分节（**追加**，§WP7F2-PV5-1…5）。
- **result**：**PASS**（本任务五项检查在固定提交 `5c869160…` 上全部通过）。**不代表** 3.14 的独立 review、2.23/2.24（WP8 状态写回与统一入口）、WP8 的 project verify、6.5（候选替代验证）或合并已完成；也不代表跨实现（Linux/CI）轮次已跑。
- **evidence_paths**：`openspec/changes/node-link-owner/reports/du1-pv1.log`（「WP7-fix2 重跑轮次」分节）、`openspec/changes/node-link-owner/reports/pv5-windows-nodelink.log`（「WP7-fix2 重跑轮次」分节）、本文件。
- **resource_cleanup**：本轮**未新增**任何进程 / 监听端口 / 数据库 / 容器 / 账号 / 证书 / 依赖；未改 `Cargo.toml`、`Cargo.lock`、`schemas/**`、`compatibility/**`、`fixtures/**`、`docs/**`、`openspec/**`。
  只读检查结束后实测：本轮相关进程（`cargo.exe`/`rustc.exe`/`link.exe`/`acpr-fake-acp-agent.exe`/`acp-remote.exe`/`acp_remote.exe`/`server.exe`/`node-link-e2e.exe`）计数 = **0**；`netstat -ano` 中 `8765` = **0**（grep exit 1）。
  `/tmp` 的 `acpr-*`：**起点 0 个、终点 0 个**（`ls -d /tmp/acpr-*` 两次都 exit 2），因此本轮**没有**可执行的 `acpr-*` 删除动作；`/tmp/acpr-wp7*`（listener 用例的 `TestCertificate` 目录）起点/终点均为 0。
  本 Agent 本轮自建 scratch 在仓库之外（`/tmp/wp7f2v`，11 个文件＝各命令 raw 输出的临时副本），在报告落盘后两步式删除（逐文件 `rm -f` → 逐层 `rmdir`），删除后 `ls -d /tmp/wp7f2v` exit 2。
  **只登记不删除**（派单授权范围仅限 `/tmp/acpr-*`）：早前轮次留下的非 `acpr` scratch `/tmp/wp7e2e.log`、`/tmp/wp7f1/`、`/tmp/wp7fix1/`（mtime ∈ 20:17–21:11 本地，均早于本轮起点 21:39 本地）。
  worktree 自有的 `target/` 与 `node_modules/` 是 git-ignored 构建缓存，按约定保留。

## 检查结果（逐项退出码）

| ID | 命令（cwd = worktree 根） | 退出码 | 耗时 | 关键结果 | 日志位置 |
|---|---|---|---|---|---|
| **PV3** | `cargo test --locked -p server -p core -p storage-sqlite -p identity-auth -p node-link-protocol --all-features` | **0** | 34 s | **671 passed / 0 failed / 2 ignored**；39 个测试目标全 `ok`；core lib **118**（base `690b9172` 为 115，**+3**） | `du1-pv1.log` §WP7F2-2 / §WP7F2-2b |
| **PV4** | `cargo test --locked -p app --all-features` | **0** | 67 s | **85 passed / 0 failed / 0 ignored**；8 个测试目标全 `ok`；declared 85 = executed 85 | `du1-pv1.log` §WP7F2-1 |
| **PV1** | `npm run verify`（聚合） | **0** | 108 s | workspace **972 passed / 0 failed / 2 ignored**；85 个测试目标；10 道门禁 + agentic 16 items 全绿 | `du1-pv1.log` §WP7F2-3 |
| PV1-C01 | `npm run check:schemas` | **0** | 2 s | `118 valid, 25 invalid (ajv Draft 2020-12), 39 event views bound` | §WP7F2-4 C01 |
| PV1-C02 | `npm run check:commands` | **0** | 0 s | `12 commands` | §WP7F2-4 C02 |
| PV1-C03 | `npm run check:errors` | **0** | 1 s | `58 codes across 2 protocols` | §WP7F2-4 C03 |
| PV1-C04 | `npm run check:features` | **0** | 1 s | `11 feature ids across 2 protocols` | §WP7F2-4 C04 |
| PV1-C05 | `npm run check:assets` | **0** | 1 s | `17 schemas, 156 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed` | §WP7F2-4 C05 |
| PV1-C06 | `npm run check:acp` | **0** | 0 s | `25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows, schema validated by ajv)` | §WP7F2-4 C06 |
| PV1-C07 | `npm run check:docs` | **0** | 0 s | `378 relative links, 4135 section refs across 257 markdown files`（上一轮 4133 → 本轮 4135，来自 fix2 的 §6 文档改动） | §WP7F2-4 C07 |
| PV1-C08 | `npm run check:boundaries` | **0** | 0 s | `12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）` | §WP7F2-4 C08 |
| PV1-C09 | `npm run check:drift` | **0** | 0 s | `§7 的 36 条 DDL` 与 `migrate.rs` 一致；`§5 的 15 个 trait / 93 个方法签名` 与 `ports.rs` 一致 | §WP7F2-4 C09 |
| PV1-C10 | `npm run check:agentic` | **0** | 2 s | `Totals: 16 passed, 0 failed (16 items)` + `宿主入口检查完成：17 个文件` | §WP7F2-4 C10 |
| PV1-C10a | `node scripts/agentic-gate.mjs` | **0** | 2 s | 16/16 items | §WP7F2-4 C10a |
| PV1-C10b | `node scripts/sync-agentic-host-entrypoints.mjs --check` | **0** | 0 s | `agentic 宿主入口检查完成：17 个文件` | §WP7F2-4 C10b |
| PV1-R01 | `cargo fmt --all -- --check` | **0** | 1 s | 无输出（无格式差异） | §WP7F2-4 R01 |
| PV1-R02 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | **0** | 1 s | 无诊断（**指纹缓存命中**，见 issues 第 1 条；`-v` 复核 12/12 workspace crate = Fresh） | §WP7F2-4 R02 / §WP7F2-4b |
| PV1-R03 | `cargo test --locked --workspace --all-features` | **0** | 99 s | **972 passed / 0 failed / 2 ignored**；85 个目标；与聚合跑逐项一致 | §WP7F2-4 R03 |
| **PV2** | `node scripts/check-crate-boundaries.mjs` | **0** | 0 s | `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）` | `du1-pv1.log` §WP7F2-5 |
| **PV5** | `cargo test --locked -p server -p app --all-features`（本机 Windows） | **0** | 83 s | **394 passed / 0 failed / 0 ignored**；15 个目标；declared 394 = executed 394 | `pv5-windows-nodelink.log` §WP7F2-PV5-1…2 |
| 补充 | `cargo test --locked -p app --all-features --test node_link_e2e -- --test-threads=1` | **0** | 6 s | **2 passed / 0 failed / 0 ignored**（受控路径全链路 + TLS direct，5.29 s） | `du1-pv1.log` §WP7F2-6、`pv5-windows-nodelink.log` §WP7F2-PV5-3 |
| — | `ls -d /tmp/acpr-*`、`ls -d /tmp/acpr-wp7*`、进程精确匹配、`netstat` 8765、worktree 终点 | — | — | 起点与终点均为 0 残留（`acpr-*` exit 2、证书目录 exit 2、进程计数 0、8765 exit 1、status 0 行） | `du1-pv1.log` §WP7F2-0 / §WP7F2-8 / §WP7F2-9 / §WP7F2-10 |

**子检查合计：15/15 exit 0**，与 `npm run verify` 聚合退出码 0 一致（无「聚合绿、单项红」或反过来的矛盾）。
环境：Windows / git-bash（MINGW64_NT-10.0-26200 / x86_64）；`rust-toolchain.toml` 固定 `1.98.1`，实测 cargo 1.98.1（`797e8a9bc`）、rustc 1.98.1（`48a229cea`）、node v24.19.0、npm 12.0.2、git 2.55.0.windows.5。

## 派单点名要求：全链路集成测试是否真实执行（无全跳过）

`[PV4]`（`-p app`）的 8 个目标，全部 `ok`：

| 目标 | 执行数 | 说明 |
|---|---|---|
| app lib（`unittests src\lib.rs`） | **52** | 与上一轮相同 |
| app main（`acp_remote` CLI 二进制） | 0 | 本目标无 `#[test]`（见下方 0 用例说明） |
| `tests\audit_export.rs` | **1** | — |
| `tests\cli_commands.rs` | **11** | CLI 行为 |
| `tests\daemon_lifecycle.rs` | **11** | 60.41 s，**真实起停 daemon 子进程** |
| `tests\node_link_e2e.rs` | **2** | 3.19 s，**受控路径全链路** |
| `tests\node_link_listener.rs` | **8** | 0.31 s，**真实 `TcpListener` 绑定与 TLS 终止** |
| Doc-tests app | 0 | 文档注释无可执行 Rust 代码块 |
| **合计** | **85** | `-- --list` 声明 52+0+1+11+11+2+8+0 = **85**（`declared - ok - ignored = 0`，**无未执行用例**） |

`[PV5]`（`-p server -p app`）的 15 个目标，全部 `ok`：app 85 + server 309（lib 281 / `local_admin_channel` 14 / `local_admin_schema_drift` 6 / `local_endpoint_naming` 4 / `local_endpoint_unix` 0 / `local_endpoint_windows` 4 / Doc-tests server 0）= **394**，`-- --list` 声明 **394 == 394**。

**本次改动的判别用例确实执行并通过**（这是本轮重跑的核心目的）：wp7-fix2 新增的 3 条用例在 `[PV3]` 的 core lib（118 条）中逐条 `ok`（§WP7F2-2b）：
- `broker::tests::a_late_terminal_of_an_abandoned_turn_does_not_complete_the_next_turn`（F1 判别器：不得提前派发 T2、无归属迟到终态按 T1 归属并丢弃、T2 命令仍 `Accepted`）
- `broker::tests::an_unobserved_abandoned_turn_hold_is_released_after_the_bounded_rounds`（F1 兜底：轮次用尽前不派发、用尽后必须派发）
- `broker::tests::a_late_event_of_an_abandoned_turn_is_dropped_instead_of_degraded_without_a_turn`（F3 边界：可归属时丢弃 vs 无归属落库，含会话级事件阳性对照）

受控路径全链路未因会话槽占位而回退：`tests\node_link_e2e.rs` **2/2 ok**（`the_controlled_path_runs_end_to_end_and_revocation_propagates`、`tls_direct_terminates_the_same_handshake`），并且约定形式的**定向单线程重跑**（`--test node_link_e2e -- --test-threads=1`）同样 **2/2 ok**（§WP7F2-6、§WP7F2-PV5-3，各 6 s）。
真实 listener / 进程轮次逐条来自 raw 输出的 `... ok` 行：
- `tests\node_link_listener.rs`（8/8 ok）：`an_invalid_listen_literal_refuses_startup`、`plaintext_dev_mode_is_refused_off_loopback`、`direct_tls_without_readable_pem_refuses_startup`、`an_occupied_listen_address_refuses_startup`、`the_wired_listener_keys_are_no_longer_reported_as_unwired`、`proxy_mode_off_loopback_starts_with_a_warning`、`the_configured_listen_address_is_bound_and_reported`、`direct_tls_terminates_tls_and_refuses_plaintext`。
- `tests\daemon_lifecycle.rs`（11/11 ok）：含 `a_fresh_start_takes_the_lock_creates_the_endpoint_and_imports_the_seeds`、`the_periodic_task_runs_at_least_once_and_is_cancelled_at_shutdown`、`the_network_listener_stops_serving_new_connections_before_the_local_drain_finishes` 等真实进程生命周期轮次。

0 用例目标（平台差异或交付形态，**非静默跳过**，已逐条交代）：
`app` 的 `src\main.rs`（CLI 二进制，无 `#[test]`，同一交付面由 `tests\cli_commands.rs` 11 条覆盖）、`tests\local_endpoint_unix.rs`（整文件 `#![cfg(unix)]`，Windows 上不参与编译，同一规则由 `tests\local_endpoint_windows.rs` 4 条覆盖）、`Doc-tests app` / `Doc-tests server`（两个 crate 的文档注释无可执行代码块）。所有轮次都未出现「整 target 被 filter 掉」：命令未带 filter，`0 filtered out` 出现在每条 `test result` 行。

**与上一轮（WP7-fix @ `690b9172`）的计数对照**（说明本轮没有静默减少覆盖，且 fix2 的新用例在本机真的跑了）：

| 项 | WP7-fix 轮次（`690b9172`） | 本轮（`5c869160`） | 差值 | 说明 |
|---|---|---|---|---|
| `[PV1]`/`[R03]` workspace | 969 / 0 / 2（85 目标） | **972 / 0 / 2（85 目标）** | **+3** | 全部落在 core lib（fix2 的 3 条 broker 用例） |
| `[PV4]` app | 85 / 0 / 0（8 目标） | **85 / 0 / 0（8 目标）** | 0 | fix2 未改 app 侧测试 |
| `[PV5]` 本机 Windows | 394 / 0 / 0（15 目标） | **394 / 0 / 0（15 目标）** | 0 | 同上：fix2 的改动在 core，而 `-p server -p app` 只把 core 作为依赖编译 |
| `[PV3]` core lib | 115 | **118** | **+3** | 3 条 fix2 用例 |
| server 侧 | 309 | **309** | 0 | lib 281 / 14 / 6 / 4 / 0 / 4 逐目标不变 |
| storage-sqlite / identity-auth / node-link-protocol | 124 / 81 / 37 | **124 / 81 / 37** | 0 | 逐目标不变 |
| 声明 vs 执行 | 968+ = 966+2 | **PV3 673 = 671+2、PV4 85 = 85、PV5 394 = 394** | — | 无未执行用例 |

## issues（需要主 Agent / reviewer 知晓的事实）

1. **`npm run check:rust` 的 clippy 是 cargo 指纹缓存命中（如实登记，不是造假也不是跳过）**：`[PV1]` 聚合内的 `cargo clippy` 与单独执行的 R02 都只花约 1 s（输出仅 `Finished dev profile … in 0.66s`、无任何 `Checking` 行），即工作区源码与上次 clippy 指纹一致（fix2 两个提交的 `.husky/pre-commit` 已对**同一份源码**跑过 workspace clippy）。两次都 **exit 0、零诊断**，判定有效。
   为让该判定可核查，本轮加了一条只读补充（§WP7F2-4b）：`cargo clippy --locked --workspace --all-targets --all-features -v -- -D warnings` 显示 **12/12 workspace crate 全为 `Fresh`**（`Checking` 行 = 0、`warning`/`error` 行 = 0、`Fresh` 行 236），说明当前源码确实已被 clippy-driver 检查过且指纹一致。
   仍未执行的是「换 target dir 强制重跑」——它需要重编全部依赖（成本高、且会在仓库外新产生 GB 级目录），本轮按既往口径**不做**。
2. **一次命令形式写错但已如实留证**：`-- --list` 起初被我写成 cargo 级参数（`--list` 而非 `-- --list`），cargo 报 usage 错误、exit=1（§WP7F2-7 前 5 条）。**这不是门禁失败**；日志保留了错误输出，并在同节给出更正命令与重跑结果（declared：app 85 / server 309 / PV3 集合 673 / PV5 集合 394，cargo_exit 全 0）。
3. **`[PV5]` 计数与上一轮相同属于预期，不是覆盖减少**：fix2 的可判别用例（3 条 broker 用例）属 core lib，落在 `[PV3]`；`[PV5]` 的命令 `-p server -p app` 只把 core 当依赖编译，因此 394 不变。真正证明「fix2 未破坏全链路」的是 `tests\node_link_e2e.rs` 2/2（含约定形式的单线程重跑 2/2）。
4. **子检查条数的口径**：`package.json` 的 `verify` = `check` + `check:rust`。`check` 有 10 个 npm script，其中 `check:agentic` 内部串了 2 条命令；`check:rust` 内部串了 3 条命令。本轮既执行了 10 个 npm script，又把 `check:agentic` 的 2 条与 `check:rust` 的 3 条展开单独各跑一次，故可独立执行的命令是 **15 条**（C01–C09、C10、C10a、C10b、R01、R02、R03），**15/15 exit 0**。若 reviewer 只按「npm script 计数」看是 13 条（10 + 3）；口径差异已在 §WP7F2-4 逐条列明命令与退出码，结论不变。
5. **`check:docs` 的 section ref 数从 4133 变 4135**：来自 fix2 对 `docs/CORE_PORTS_AND_STORAGE.md` 的 §6 文本改动（新增交叉引用）。`check:drift` 不变（fix2 只动 §6，§7 DDL 与 §5 端口签名未变），与「fix2 未改端口/DDL」的实现事实一致。
6. **资源残留为零，本轮没有可删的 `acpr-*`**：起点与终点 `/tmp/acpr-*` 均为 0（两次 `ls` 都 exit 2），`/tmp/acpr-wp7*` 证书目录同样为 0，说明 app 侧 `TempRoot`/`TestCertificate` 的 `Drop` 清理在本机有效、且上一轮的清理已彻底。
   记录瑕疵（无副作用）：写日志的 `echo` 里用了 shell 反引号，两条只读的 `ls -d /tmp/acpr-*/TestCertificate` / `ls -d /tmp/acpr-wp7*` 被当作命令替换、当行文本残缺；日志同节已用 `<<'EOF'` 的无展开写法补了一条「澄清-更正」，结论不受影响。早前轮次的**非 acpr** scratch（`/tmp/wp7e2e.log`、`/tmp/wp7f1/`、`/tmp/wp7fix1/`）**只登记不删除**——派单授权范围是 `/tmp/acpr-*`；若主 Agent 希望在归档前一并清空，需要再给一次明确授权。
7. **本 Agent 自建 scratch 已清理**：`/tmp/wp7f2v`（11 个临时文件）在报告落盘后两步式删除（本机工具的递归删除型命令会被安全确认拦截，故改用「逐文件 `rm -f` → 逐层 `rmdir`」），删除对象全在仓库之外、未触碰仓库内任何文件。
8. **本轮是只读轮次**：未改任何文件（含未改 `tasks.md` / `verification.md` / `plan.md` / `docs/**`、未改任何源码）；除权威规划根 `reports/` 下的日志与报告外没有其它写入。worktree 终点 `git status --porcelain` = 0 行、暂存区 0 行、无 stash。
9. **未执行（如实记录，与既往轮次口径一致）**：`cargo-deny`（依赖许可证/来源/advisory）与 `gitleaks`（密钥扫描）**只在 CI 运行**，本地无等价物、不在 `npm run verify` 里；本机未装 gitleaks。因此「本地绿」不等于这两类判定通过，合并前仍以 CI 的 `deps`/`advisories`/`secrets` 三个 job 为准。
10. **边界**：本轮只覆盖**本机 Windows**；`tests\local_endpoint_unix.rs` 在 Linux/CI 上才会真正执行（同一规则本机由 `local_endpoint_windows.rs` 覆盖），跨实现轮次不在 3.13 范围内。

## handoff_index

```yaml
handoff_index:
  - task_id: "3.13"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "5c869160e8dab0361b248124f631beba239183c6"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "固定提交 5c869160（分支 agentic/node-link-owner；preflight @13:39:47Z 与终态复核 @13:48:33Z 的 `git rev-parse HEAD` 一致、`git status --porcelain | wc -l` 均为 0、暂存区 0 行、无 stash）上，用固定工具链（rust-toolchain.toml = 1.98.1，实测 cargo 1.98.1 / rustc 1.98.1；node v24.19.0 / npm 12.0.2）执行 `npm run verify`：**聚合退出码 0**（§WP7F2-3，108 s），其中 `cargo test --locked --workspace --all-features` 真实执行 **972 passed / 0 failed / 2 ignored**、85 个测试目标。为排除 `&&` 掩盖前序失败，把 `check` 的 10 个 npm script 与 `check:agentic`/`check:rust` 展开成的命令共 **15 条逐个单独执行、各自记录 $?**（§WP7F2-4：C01..C09、C10、C10a、C10b、R01、R02、R03），**15/15 均 exit 0**；R03 独立复跑与聚合跑计数逐项一致（972/0/2、85 目标）。只读 `-- --list` 交叉核对无未执行用例。核心计数变化 = core lib +3（fix2 的 3 条 broker 用例），其余 crate 逐目标不变。**如实登记**：R02 clippy 两次都是 cargo 指纹缓存命中（0.66 s、无 Checking 行），并用 `cargo clippy -v` 核实 12/12 workspace crate 为 Fresh（§WP7F2-4b）；另有一处 `--list` 命令形式写错（cargo 报 usage、exit=1）已留证并在同节更正重跑（§WP7F2-7），不是门禁失败。2 条 ignored 是仓库声明过的辅助项（storage-sqlite 的 crash 子进程目标与 v1 夹具生成器）。五条命令在工作区内无副作用：全部跑完后 `git status --porcelain` 仍 0 行。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.13"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "5c869160e8dab0361b248124f631beba239183c6"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一固定提交、同一环境下直接执行门禁脚本本体 `node scripts/check-crate-boundaries.mjs`（不是只跑 `npm run check:boundaries` 包装）：**退出码 0**，输出 `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）`（§WP7F2-5 raw 全文，含 head_before/head_after 均为 5c869160…）。该脚本以 `MODULE_ARCHITECTURE.md` §5 的依赖矩阵为唯一判据、用 `cargo metadata` 校验每个 crate 的实际依赖，并硬约束 `core` 不引入 runtime/DB/HTTP/子进程/wire protocol 依赖；fix2 只改了 `crates/core/src/broker.rs`（同 crate 内改动）、`crates/app/tests/support/owner.rs`（注释）与 `docs/CORE_PORTS_AND_STORAGE.md`，**未改变任何 crate 的依赖方向**（`app -> server / backends / identity-auth / core`、`server -> core + wire protocols`）。同一判定在 §WP7F2-4 的 C08 里独立复跑一次，同样 exit 0。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.13"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "5c869160e8dab0361b248124f631beba239183c6"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一固定提交上执行 `cargo test --locked -p server -p core -p storage-sqlite -p identity-auth -p node-link-protocol --all-features`：**退出码 0**、34 s、**671 passed / 0 failed / 2 ignored**、39 个测试目标全部 ok（§WP7F2-2 raw 全文含逐目标 `test … ok` 行与每条 `0 filtered out` 行）。只读 `-- --list` 声明 **673 = 671 + 2 ignored**，无未执行用例。**本轮 fix2 的判别证据在此**：core lib **118**（= fix2 base 115 + 3），新增三条逐条真实执行并通过——`broker::tests::a_late_terminal_of_an_abandoned_turn_does_not_complete_the_next_turn`（F1：不得提前派发 T2、无归属迟到终态按 T1 归属并丢弃、T2 命令仍 `Accepted`）、`broker::tests::an_unobserved_abandoned_turn_hold_is_released_after_the_bounded_rounds`（F1 兜底：轮次用尽前不派发、用尽后必须派发）、`broker::tests::a_late_event_of_an_abandoned_turn_is_dropped_instead_of_degraded_without_a_turn`（F3 边界，含会话级事件阳性对照）（§WP7F2-2b）。其余 crate 与上一轮逐目标相同：server lib 281 + local_admin 系列 28（14/6/4/0/4）、storage-sqlite 124（含 2 ignored）、identity-auth 81、node-link-protocol 37、5 个 Doc-tests 目标。2 条 ignored 为仓库声明的辅助项，不是被跳过的覆盖。所有命令前后 HEAD 相同（无修订漂移）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.13"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "5c869160e8dab0361b248124f631beba239183c6"
    evidence_type: CHECK
    evidence_id: PV4
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一固定提交上执行派单原文命令 `cargo test --locked -p app --all-features`：**退出码 0**、67 s、**85 passed / 0 failed / 0 ignored**、8 个测试目标全部 ok（§WP7F2-1 raw 全文含逐条 `test … ok` 行）。**派单点名要求确认的「全链路集成测试真实执行、无零用例/全跳过」**：只读 `-- --list` 交叉核对 declared **85 = executed 85**（52+0+1+11+11+2+8+0，declared - ok - ignored = 0），命令未带 filter 且每条 `test result` 行都是 `0 filtered out`。**受控路径全链路真实执行并通过**：`tests\\node_link_e2e.rs` **2/2 ok**（`the_controlled_path_runs_end_to_end_and_revocation_propagates` = 配对→本地确认→握手→catalog 过滤→attach/subscribe→event/ack→命令→幂等→R61 故障注入→撤销传播；`tls_direct_terminates_the_same_handshake` = 自签证书 TLS direct 同一握手，3.19 s）；另有约定形式的定向单线程重跑 `--test node_link_e2e -- --test-threads=1` **2/2 ok**（§WP7F2-6，5.29 s）——fix2 的会话槽占位改动未破坏该场景。`tests\\node_link_listener.rs` **8/8 ok**（真实 `TcpListener` 绑定与 TLS 终止、失败关闭，0.31 s）、`tests\\daemon_lifecycle.rs` **11/11 ok**（60.41 s，真实起停 daemon）、app lib 52、`audit_export` 1、`cli_commands` 11。0 用例目标已逐条交代（app `main.rs` 由 cli_commands 覆盖、Doc-tests app 无代码块），不是静默跳过。计数与上一轮（app 85）相同：fix2 的改动在 core，app 侧测试未变，可判别用例在 [PV3]。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.13"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "5c869160e8dab0361b248124f631beba239183c6"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "openspec/changes/node-link-owner/reports/pv5-windows-nodelink.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本机 Windows（MINGW64_NT-10.0-26200 / x86_64-pc-windows-msvc，cargo 1.98.1）上执行 `cargo test --locked -p server -p app --all-features`：**退出码 0**、83 s、**394 passed / 0 failed / 0 ignored**、15 个测试目标全部 ok（§WP7F2-PV5-1…2，raw 全文含逐条 `… ok` 行）；只读 `-- --list` 声明 **394 == 394**。**这是本轮为 STALE/PENDING 的上一轮 [PV5] 重跑的核心证据**（wp7-fix2 改了 core 的会话槽占位与会话状态投影后，rv2-wp7 记录的 [PV5] 必须在候选 revision 上以约定形式重跑）：受控路径全链路 `tests\\node_link_e2e.rs` **2/2 ok**，且 `cargo test --locked -p app --all-features --test node_link_e2e -- --test-threads=1` 同样 **2/2 ok**（§WP7F2-PV5-3，5.33 s）；`tests\\node_link_listener.rs` 8/8（真实 bind / TLS 终止 / 失败关闭）、`tests\\daemon_lifecycle.rs` 11/11（60.36 s 真实起停）。逐目标：app 85（lib 52 / main 0 / audit_export 1 / cli_commands 11 / daemon_lifecycle 11 / node_link_e2e 2 / node_link_listener 8 / Doc-tests 0）+ server 309（lib 281 / local_admin_channel 14 / local_admin_schema_drift 6 / local_endpoint_naming 4 / local_endpoint_unix 0 / local_endpoint_windows 4 / Doc-tests 0）= 394。计数与上一轮（394，@ 690b9172）**逐目标相同**且属预期——fix2 的改动在 core，本命令只把 core 当依赖编译；fix2 的可判别用例在 core lib，由 [PV3] 承载（core lib 115 → 118）。平台差异真实留证：`tests\\local_endpoint_windows.rs` 4/4 执行通过，`tests\\local_endpoint_unix.rs` 0 条（整文件 `#[cfg(unix)]`，本机不参与编译、由 Linux CI 覆盖）；4 个 0 用例目标已逐条交代，不是静默跳过。跑完后 worktree 仍干净（`git status --porcelain` 0 行），无新增进程/端口/`acpr-*` 或证书目录残留（本轮起点与终点 `/tmp/acpr-*` 均为 0；自建 scratch /tmp/wp7f2v 已清理）。"
    source_evidence: NOT_APPLICABLE
```
