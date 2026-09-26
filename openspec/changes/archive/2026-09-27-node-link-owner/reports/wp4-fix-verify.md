# WP4 修复后验证重跑（任务 3.7 / WP4-fix）— `node-link-owner` / DU1

## Shared Report

- **task_id**：3.7（WP4 交付前 project verify 的**修复后重跑**；权威 `tasks.md:3.7` 原文 =「完成 WP4 交付前 project verify：`[PV3]` + `[PV5]`（本机）。完成条件：逐项通过并留证。」）
- **role / phase**：coder / verify —— 本任务**只执行检查与写报告，未修改被检查仓库的任何文件**（worktree 终点 `git status --porcelain` 仍为 0 行、暂存区为空）。
- **agent_context**：worker 子 Agent（本机 worktree `D:\Project\acp-remote-wt\node-link-owner` 独占会话，未继承 WP4 实现 / fix / review 轮次的对话；只读检查，无清单外编辑）。权威规划根 `D:\Project\acp-remote`（报告与日志写在 `openspec/changes/node-link-owner/reports/`，该目录不入版本控制；`*.log` 已 gitignore）。
- **target_revision**：派单固定提交 `59bf1058851deb5b83c875a02f8046c815a9c4f0`（轮次起点 16:18:01 实测一致，`git status --porcelain` 0 行）→ **轮次进行中被外部提交推进到 `2937850f607ea54792a55649a82b7f6878808830`**。因此本报告含两段证据：**A 段**（从 59bf1058 起执行的原始跑，PV3/存储/PV1 聚合与子检查落在 16:23:17 之前或跨过它）与 **B 段**（在推进后的当前 HEAD `2937850` 上整套复跑，每条命令 `head_before == head_after == 2937850`）。详见「修订漂移」。
- **scope（本任务实际执行的检查）**：
  - `[PV3]` `cargo test --locked -p server -p core -p identity-auth -p node-link-protocol --all-features`（记录计数，确认 conn/握手用例真实执行）；
  - `[PV3+]`（派单点名「存储新用例」的补充只读范围，因该 crate 不在上面的 `-p` 列表内）`cargo test --locked -p storage-sqlite --all-features`；
  - `[PV1]` `npm run verify`（聚合）+ **15 个子检查逐个单独执行、各自记录退出码**（不用 `&&` 掩盖前序失败）；
  - `[PV2]` `node scripts/check-crate-boundaries.mjs`；
  - `[PV5]` `cargo test --locked -p server -p app --all-features`（本机 Windows）。
- **checks**：**A 段与 B 段四项全部 exit 0**，无 FAIL；无「零用例或全跳过」被当成通过；无新增进程 / 端口 / 临时目录残留。逐项见下表与 `reports/du1-pv1.log` 的「WP4-fix 重跑轮次」分节（§W0–§W9 = A 段，§WB0–§WB9 = B 段，§WC = 本 Agent 的时间线自查更正）、`reports/pv5-windows-nodelink.log` 的同名分节。
- **issues**（四条，均已在日志内留证；前两条需要主 Agent 与 reviewer 知晓）：
  1. **修订漂移**：16:23:17 有外部提交 `2937850`（`docs(core): 对齐握手 seam 文档的入口计数与收尾写集口径（RV2-WP4）`）落入同一分支、同一 worktree，只改 `docs/CORE_PORTS_AND_STORAGE.md`（2 行）与 `docs/IDENTITY_AND_AUTH_CONTRACT.md`（5 行），**无代码 / 合同资产 / fixture 变更**。处置：不修改文件、不切提交，改为在推进后的 HEAD 上整套复跑（B 段），A/B 两段计数逐项一致；**引用本轮结论时以 B 段（2937850）为准**。
  2. **5 个既有 `acpr-storage-*` 临时目录**（MSYS `/tmp`，mtime 16:12:52–16:13:13，全部早于轮次起点 16:18:01）：目录名正是 WP4-fix1b 的 4 条新用例名，mtime 与 `/tmp/wp4fix1b/` 下 fix1b 轮次的红向证据文件一一对应（`red1.txt` 16:12:52、`red2.txt` 16:13:01、`red3.txt` 16:13:07、`clippy.txt` 16:13:17），即来自 fix1b **故意让用例失败**的红向跑（panic 路径下 `TempDir::drop` 未能删掉仍被 sqlite 句柄占用的目录）。**本轮 A+B 两段共 44 条命令调用（A 段 22 + B 段 22；含 2 次 workspace 全量测试、2 次 storage-sqlite、2 次 server+app）没有新增任何临时目录**；按派单「只核实不修改」，本轮**未删除**他人轮次的产物（删除会破坏其可复现性）。
  3. 派单的 `[PV3]` 命令只覆盖 server/core/identity-auth/node-link-protocol 四个 crate，而 WP4-fix1b（59bf1058）**只改了 `crates/storage-sqlite/tests/admin_store.rs`** —— 该文件不在 `-p` 列表内。为满足派单「确认 …存储新用例真实执行」，另跑一条只读命令 `[PV3+]`（§W3/§WB3），并把 PV1 的全 workspace 跑作为第二重覆盖。
  4. 本 Agent 自己的两处留证工具细节，如实登记且不影响结论：`grep -c "panicked at"` 无匹配时返回退出码 1（被 bash 会话当作命令退出码，见 §W7 一带的输出）；`--list` 之外的 `running N tests` 行求和（888）比 harness `passed` 之和（885）多 3，来源是 `crash_child` 被拉起的嵌套子进程各自打印 `running 1 test`（既有已知现象，harness 的 83 条 `test result:` 行是权威计数）。
- **result**：**PASS**（本任务四项检查在 A/B 两个修订上都通过；计数逐项相同）。**不代表** 3.8（RV1/RV2 独立 review）、6.5（候选替代验证）或合并已完成。
- **evidence_paths**：`reports/du1-pv1.log`（「WP4-fix 重跑轮次」分节，**追加**；A 段 §W0–§W9、B 段 §WB0–§WB9、时间线更正 §WC）、`reports/pv5-windows-nodelink.log`（「WP4-fix 重跑轮次」分节，**追加**）、本文件。
- **resource_cleanup**：未创建端口 / 数据库 / 容器 / 账号 / 证书；只有本机 `node`/`npm`/`cargo`/`git` 调用。检查结束后按**精确镜像名**匹配 `cargo.exe`/`rustc.exe`/`link.exe`/`acpr-fake-acp-agent.exe`/`acp-remote.exe`/`acp_remote.exe`/`server.exe`/`app.exe`/`admin_store.exe` = **0**；默认监听端口 8765 的 netstat 条目 = **0**（用例只绑 `127.0.0.1:0`）。MSYS `/tmp` 的 `acpr*` 条目 = **5**，且全部是轮次开始前就存在的（本文 issues 第 2 条）。本轮自身临时产物全部在仓库之外：MSYS `/tmp/wp4fix2/**`（preflight、各命令 raw、15 个子检查输出、驱动脚本）。worktree 自有 `target/` 与 `node_modules/` 是 git-ignored 缓存，按约定保留。

## 修订漂移（必须与结论一起读）

| 时间 | 事实 |
|---|---|
| 16:18:01 | 轮次起点：`git rev-parse HEAD` = `59bf1058851deb5b83c875a02f8046c815a9c4f0`（= 派单固定提交），`git status --porcelain` 0 行、暂存区 0 行 |
| 16:18:14–16:18:39 | A 段 [PV3]（四 crate）在 59bf1058 上执行 |
| 16:18:51 / 16:18:54–16:18:56 | A 段 `-p server --list` / A 段 [PV3+] storage-sqlite 执行 |
| 16:18:58–16:20:43 | A 段 [PV1] 聚合 `npm run verify` 执行（**完整落在提交之前**） |
| 16:20:56–16:22:45 | A 段 15 个子检查执行（逐个单独执行，**整段落在提交之前**；末项 R03 于 16:22:45 结束） |
| **16:23:17** | **外部提交 `2937850f607ea54792a55649a82b7f6878808830`**（`docs(core)`，2 份文档共 7 行，无代码变更）—— 非本 Agent 的改动 |
| 16:22:55–16:24:15 | A 段 [PV5]（本机 Windows）执行 —— 起于提交之前、止于提交之后（**跨过** 16:23:17），因该提交只改文档且测试二进制在 16:22 前已编译完成，语义不受影响 |
| 16:25:04–16:30:23（B 段） | 在 `2937850` 上整套复跑四条检查（16:25:04–16:28:33）+ 15 个子检查（–16:30:23）；**每条命令前后各取一次 HEAD，全部相等**（`reports/du1-pv1.log` §WB0 的 manifest 逐行列出 `head_before`/`head_after`） |

上表时刻取自每条命令输出文件的 mtime（= 结束时刻）与驱动脚本记录的耗时；本报告最初的一版正文里我给过三个**估算**时刻（PV1 聚合、A 段子检查、A 段 PV5），日志已用 §WC 逐条更正，**以本表与 §WC 为准**。更正后的更强结论：A 段的 [PV3]/[PV3+]/[PV1]（聚合与全部 15 个子检查）**完全来自 59bf1058 的工作树，没有跨修订**；只有 A 段 [PV5] 跨过该时刻。

两段结论一致（计数逐项相同），因此漂移不改变任何一项结论。严格地说：A 段的 [PV3]/[PV3+]/[PV1] 完整跑在 59bf1058 上，只有 A 段 [PV5] 跨过了提交时刻；B 段整体跑在 2937850 上。**引用本轮结论时以 B 段（当前 HEAD）为准**，A 段作为「派单提交上的独立复跑」记录。

## 检查结果（逐项退出码）

| ID | 命令（cwd = worktree 根） | A 段（59bf1058） | B 段（2937850） | 关键结果 | 日志位置 |
|---|---|---|---|---|---|
| **PV3** | `cargo test --locked -p server -p core -p identity-auth -p node-link-protocol --all-features` | **0**（25 s） | **0**（19 s） | **477 passed / 0 failed / 0 ignored**（两段相同）；25 条 result 行全 `ok`；`node_link::conn` declared 32 == 执行 32 | §W1/§W2、§WB1/§WB2 |
| PV3+ | `cargo test --locked -p storage-sqlite --all-features` | **0**（2 s） | **0**（3 s） | **120 passed / 0 failed / 2 ignored**；`tests\admin_store.rs` 40 passed，含 fix1b 的 **4 条新用例逐条 `ok`** | §W3、§WB3 |
| **PV1** | `npm run verify`（聚合） | **0**（105 s） | **0**（104 s） | workspace **885 passed / 0 failed / 2 ignored**；83 条 result 行全 `ok`；0 用例目标 17 个 | §W4、§WB4 |
| PV1-C01 | `npm run check:schemas` | 0 | 0（输出与 A 段逐字节相同） | `118 valid, 25 invalid (ajv Draft 2020-12), 39 event views bound` | §W5、§WB5 |
| PV1-C02 | `npm run check:commands` | 0 | 0（同） | `12 commands` | 同上 |
| PV1-C03 | `npm run check:errors` | 0 | 0（同） | `58 codes across 2 protocols` | 同上 |
| PV1-C04 | `npm run check:features` | 0 | 0（同） | `11 feature ids across 2 protocols` | 同上 |
| PV1-C05 | `npm run check:assets` | 0 | 0（同） | `17 schemas, 156 fixture files, 12 transcript vectors …, 20 negative vectors …` | 同上 |
| PV1-C06 | `npm run check:acp` | 0 | 0（同） | `25 methods, 11 updates, 71 rows`（ajv 校验） | 同上 |
| PV1-C07 | `npm run check:docs` | 0 | 0（同） | `378 relative links, 4109 section refs, 257 md files` | 同上 |
| PV1-C08 | `npm run check:boundaries` | 0 | 0（同） | `12 个 crate 的依赖方向与 §5 矩阵一致` | 同上 |
| PV1-C09 | `npm run check:drift` | 0 | 0（同） | `§7 的 36 条 DDL` + `§5 的 15 个 trait / 90 个方法签名` 与代码一致 | 同上 |
| PV1-C10 | `npm run check:agentic` | 0 | 0（同） | `Installation: PASS`；`Totals: 16 passed, 0 failed (16 items)`；宿主入口 17 个文件 | 同上 |
| PV1-C10a | `node scripts/agentic-gate.mjs` | 0 | 0（同） | 16/16 items | 同上 |
| PV1-C10b | `node scripts/sync-agentic-host-entrypoints.mjs --check` | 0 | 0（同） | `agentic 宿主入口检查完成：17 个文件` | 同上 |
| PV1-R01 | `cargo fmt --all -- --check` | 0 | 0（同） | 无输出（无格式差异） | 同上 |
| PV1-R02 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | 0 | 0（同） | `Finished dev profile …`，无诊断 | 同上 |
| PV1-R03 | `cargo test --locked --workspace --all-features` | 0（97 s） | 0（97 s） | **885 passed / 0 failed / 2 ignored**；83 条 result 行、17 个 0 用例套件 | 同上 |
| **PV2** | `node scripts/check-crate-boundaries.mjs` | **0** | **0** | `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）`；两段输出**逐字节相同**（md5 `6cb53603f09a0d489c6531a0a905bde5`） | §W6、§WB6 |
| **PV5** | `cargo test --locked -p server -p app --all-features`（本机 Windows） | **0**（80 s） | **0**（81 s） | **325 passed / 0 failed / 0 ignored**（server 253 + app 72），13 个测试目标；declared 325 == 执行 325 | `pv5-windows-nodelink.log` 同名分节 |

**没有任何一条命令是用 `&&` 串联后才记退出码**：15 个 PV1 子检查由一个 bash 驱动函数逐个单独执行（脚本内 `set -u`、无 `set -e`、无 `&&`），`$?` 在各自命令后立即写入 summary；驱动原文与逐项原始输出见 §W5/§WB5。聚合 `npm run verify` 与逐子检查两套结果一致（885/0/2、17 个 0 用例套件），因此不存在「前序失败被 `&&` 掩盖」的情形。B 段 15 个子检查中 **C01–C10b 与 R01/R02 的输出与 A 段逐字节相同**（md5 见 §WB5 表），R03 只有执行顺序/耗时差异。

## 用例确实执行（无零用例、无全跳过）

- **PV3（四 crate）**：477 passed / 0 failed / 0 ignored；`-p server --list` declared **253** == `-p server` 实际执行 253（225 lib + 14 + 6 + 4 + 0 + 4）。**派单点名确认的 `node_link::conn`（连接层与握手）declared 32 == 执行 32，全部 `ok`**，含 `handshake_completes_and_enters_the_business_phase`、`a_handshake_that_never_gets_a_hello_is_closed_with_4408`（真实等满 15 s 窗口）、`silence_beyond_the_window_is_closed_with_4408`、`an_invalid_proof_is_closed_with_4401_and_audited`、`the_challenge_catalog_revision_is_the_proof_transcript_source`、`a_rejected_delivery_does_not_consume_the_outbound_sequence`（fix 轮 2917401 的主题）、`authenticated_connections_do_not_hold_the_in_flight_handshake_quota`、`the_fifth_in_flight_handshake_from_one_address_is_refused` 等；逐条名单见 §W2。
- **`[PV3+]` 存储新用例（派单点名）**：`tests\admin_store.rs` **40 passed**，fix1b（59bf1058）新增的 4 条逐条真实执行并 `ok`：`record_node_connected_advances_the_access_row_once_forward`、`consume_pairing_advances_the_node_row_of_a_node_peer`、`record_node_connected_refuses_missing_rows_and_wrong_roles`、`a_failed_audit_write_leaves_last_connected_at_untouched`（两段都跑、都 `ok`）。该文件的 2 个 `ignored` 在 storage-sqlite 的 commit/migration 目标里，是仓库声明过的辅助项。
- **0 用例目标逐条交代**（两段相同）：`tests\local_endpoint_unix.rs` 整文件 `#[cfg(unix)]`（本机 host `x86_64-pc-windows-msvc`，不参与编译），由 Linux CI 覆盖；`tests/local_endpoint_windows.rs` 本平台对照面本次 **4/4** 执行；`identity_auth` lib（用例都在 integration 文件里）与若干 `Doc-tests`/空 `main` 目标本就无用例。**没有一个是「套件被 #\[ignore\] 跳过」**（唯一 2 个 `#[ignore]` 是 `crash_child` 子进程目标与夹具生成器，仓库已声明）。
- **PV1（全 workspace）**：885 passed / 0 failed / 2 ignored；83 条 `Running`/`Doc-tests` 行 == 83 条 `test result:` 行；0 用例目标 17 个（已逐类交代）；2 个 `ignored` = `crates/storage-sqlite/tests/commit.rs:1092` 的 `crash_child` 与 `crates/storage-sqlite/tests/migration.rs:970` 的 `regenerate_v1_fixture`。
  已知输出怪癖（既有现象，两段都出现）：`crash_child` 子进程把 `test crash_child ... `（无换行）写进同一 stdout，偶见 `test crash_child ... test <某用例> ... ok` 的粘接行；它不影响 harness 计数（两段 passed 之和都是 885，`... ok` 出现次数也是 885）。
- **PV5（本机 Windows）**：325 passed / 0 failed / 0 ignored；13 个目标；declared 325 == 执行 325；`node_link::conn` **32/32**；`cfg(windows)` 分支用例真实执行（`direct_mode_permissions_are_unverifiable_on_this_platform`、`windows_inspection_is_unverifiable`、`a_failed_connect_leaves_the_endpoint_usable`、`a_cancelled_accept_drops_the_pending_instance_and_kills_the_connected_client`、`tests/local_endpoint_windows.rs` 4/4）。
- **如实记录未执行项**：`tests/local_endpoint_unix.rs`（平台差异，Linux CI 覆盖）；真实第二 OS 账号的跨用户 ACL 端到端检查（平台限制，沿用 WP2 轮登记）；跨 WP 完整受控链路（配对 → 握手 → catalog → attach → event → command → 撤销）属 WP7 的 [PV5] 行（2.22 / 3.13 / 6.5），本任务不含。

## 与上一轮（WP4 / 任务 3.7，提交 `319c77b`）的交叉核对

| 口径 | WP4 轮（319c77b） | 本轮 A 段（59bf1058） | 本轮 B 段（2937850） | 差值（相对 319c77b） |
|---|---|---|---|---|
| `-p server` 用例 | 248 | **253** | **253** | +5（2917401 的连接层用例） |
| `node_link::conn` 用例 | 27 | **32** | **32** | +5 |
| 四 crate 合计 | 472 | **477** | **477** | +5 |
| workspace 用例 | 876 | **885** | **885** | +9（server +5、storage-sqlite +4） |
| PV5 用例 | 320 | **325** | **325** | +5 |
| `ignored` 辅助项 | 2 | **2** | **2** | 0 |
| 0 用例目标 | 17 | **17** | **17** | 0 |
| 文档小节引用（C07） | 4095 | **4109** | **4109** | +14 |
| §5 端口方法签名（C09） | 89 | **90** | **90** | +1 |

fix 轮新增的 4 条存储用例（admin_store 36 → 40）已在本轮逐条执行；两段计数完全一致，说明这不是复用上一轮结果。

## 残留资源核实（A+B 之后）

- 进程（精确镜像名，`tasklist /FI "IMAGENAME eq <name>"`）= **0**：cargo / rustc / link / acpr-fake-acp-agent / acp-remote / acp_remote / server / app / admin_store。
- 端口 **8765** 的 netstat 条目 = **0**；本轮用例只绑 `127.0.0.1:0`。
- MSYS `/tmp`（= `os.tmpdir()` = Rust `std::env::temp_dir()` 的同一目录 `C:\Users\zhang\AppData\Local\Temp`）的 `acpr*` 条目 = **5**，与轮次起点快照完全相同（mtime ≤ 16:13:13，全部早于 16:18:01）→ **本轮没有新增任何临时目录**；这 5 条的来源与处置见「issues」第 2 条。
- worktree：`git rev-parse HEAD` = `2937850f607ea54792a55649a82b7f6878808830`，`git status --porcelain` **0 行**，`git diff --cached --numstat` **0 行**；本轮未创建、未修改仓库内任何文件。
- 两份日志均为**纯追加**：`du1-pv1.log` 由 14,307 行 / 822,382 字节增至 22,867 行 / 1,328,595 字节，前 822,382 字节 md5 仍为 `9b409f68cb0c51476753f32b152518ae`；`pv5-windows-nodelink.log` 由 924 行 / 66,317 字节增至 2,541 行 / 185,327 字节，前 66,317 字节 md5 仍为 `cfb8a9ba4530e7130c694f0914d8c0ce`。

## handoff_index

```yaml
handoff_index:
  - task_id: "3.7"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "2937850f607ea54792a55649a82b7f6878808830"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "B 段（当前 HEAD）。轮次起点实测 HEAD = 派单固定提交 59bf1058（git status 0 行），16:23:17 有外部纯文档提交把 HEAD 推进到 2937850（docs/CORE_PORTS_AND_STORAGE.md 2 行 + docs/IDENTITY_AND_AUTH_CONTRACT.md 5 行，无代码变更），故在推进后的 HEAD 上整套复跑，本行针对该修订。执行 `npm run verify`（= package.json 的 check && check:rust）：聚合退出码 **0**（104 s，16:3x），其中 `cargo test --locked --workspace --all-features` 真实执行 **885 passed / 0 failed / 2 ignored**（83 条 result 行全部 ok、0 用例目标 17 个、非 ok result 行 0 条、FAILED 0、panicked 0）。为排除 `&&` 掩盖前序失败，随后把 check + check:rust 展开成的 **15 个子检查逐个单独执行并各自记录 `$?`**（§WB5：C01–C10、C10a agentic-gate.mjs、C10b sync-agentic-host-entrypoints --check、R01 fmt、R02 clippy、R03 全 workspace 测试），15/15 均 exit 0；其中 C01–C10b 与 R01/R02 的输出与 A 段（59bf1058 起）逐字节相同（md5 见 §WB5 表），R03 计数一致（885/0/2、83 行、17 个 0 用例目标）。注意：C07（文档门禁）与 C09（合同漂移门禁）在两段逐字节相同，即 16:23:17 的文档提交既未让门禁变红也未改变其判定输出（C07 仍 378 links / 4109 section refs / 257 文件；C09 仍 36 条 DDL、§5 的 15 个 trait / 90 个方法签名）；2 个 ignored 是仓库声明过的辅助项（commit.rs 的 crash_child、migration.rs 的 regenerate_v1_fixture）。与 WP4 轮（319c77b）的 876/0/2 相比 +9（server +5、storage-sqlite +4），ignored 仍 2、0 用例目标仍 17，说明这是全新执行。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.7"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "59bf1058851deb5b83c875a02f8046c815a9c4f0"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "A 段（派单固定提交）。在同一 worktree 上先于 B 段执行：聚合 `npm run verify` 退出码 **0**（105 s，16:18:58→16:20:43，完整落在 16:23:17 的文档提交之前），`cargo test --locked --workspace --all-features` = **885 passed / 0 failed / 2 ignored**（83 条 result 行、17 个 0 用例目标）；15 个子检查逐个单独执行（无 `&&`、无 `set -e`，驱动原文见 §W5）全部 exit 0（driver 自身 exit 0，109 s，16:20:56→16:22:45，**整段在 16:23:17 之前结束**）。A 段与 B 段的 passed/failed/ignored 及 0 用例目标数逐项相同，故该提交上的结论与当前 HEAD 一致。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.7"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "2937850f607ea54792a55649a82b7f6878808830"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "B 段（当前 HEAD，见上一条 PV1 行的修订说明）。直接执行 `node scripts/check-crate-boundaries.mjs`：退出码 **0**，输出 `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）`（§WB6 的原始输出；该输出与 A 段逐字节相同，两文件 md5 均为 6cb53603f09a0d489c6531a0a905bde5）。同一条命令在 B 段的 15 个子检查里以 C08 再跑一次，exit 0 且输出 md5 相同。旁证（不属 [PV2] 判据）：`cargo metadata --no-deps` 实测 workspace 成员 = **12**（acp-protocol、acpr-transcript、acpr-wire、agent-host、app、core、identity-auth、identity-keystore、node-link-protocol、server、storage-sqlite、sync-protocol），与门禁口径一致；fix 轮对 server::node_link::conn、core、storage-sqlite 的改动没有改变依赖形状。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.7"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "59bf1058851deb5b83c875a02f8046c815a9c4f0"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "A 段（派单固定提交）：同一命令退出码 **0**，输出与 B 段逐字节相同（md5 6cb53603f09a0d489c6531a0a905bde5），§W6 留原始输出。依赖矩阵的判据（cargo metadata + MODULE_ARCHITECTURE §5）在两个修订之间没有变化，因为 2937850 只改文档。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.7"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "2937850f607ea54792a55649a82b7f6878808830"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "B 段（当前 HEAD）。派单要求的 `cargo test --locked -p server -p core -p identity-auth -p node-link-protocol --all-features` 退出码 **0**（19 s），**477 passed / 0 failed / 0 ignored**，25 条 `test result:` 行全部 ok、非 ok 行 0 条（§WB1）；`--list` 对 `-p server` 声明 **253** == 实际执行 253（225 lib + 14 + 6 + 4 + 0 + 4，§WB2）。派单点名要确认的 `node_link::conn`（连接层 + 握手）declared **32** == 执行 **32**，全部 `... ok`，含 handshake_completes_and_enters_the_business_phase、a_handshake_that_never_gets_a_hello_is_closed_with_4408（真实等满 15 s）、silence_beyond_the_window_is_closed_with_4408、an_invalid_proof_is_closed_with_4401_and_audited、the_challenge_catalog_revision_is_the_proof_transcript_source、a_rejected_delivery_does_not_consume_the_outbound_sequence、authenticated_connections_do_not_hold_the_in_flight_handshake_quota、the_fifth_in_flight_handshake_from_one_address_is_refused 等（逐条名单 §W2）。派单同时点名「存储新用例」：该 crate 不在本命令的 `-p` 列表内，故另跑 `cargo test --locked -p storage-sqlite --all-features`（B 段 exit 0、**120 passed / 0 failed / 2 ignored**，§WB3），其中 `tests\\admin_store.rs` = 40 passed，fix1b 新增的 4 条逐条 `ok`（record_node_connected_advances_the_access_row_once_forward、consume_pairing_advances_the_node_row_of_a_node_peer、record_node_connected_refuses_missing_rows_and_wrong_roles、a_failed_audit_write_leaves_last_connected_at_untouched），2 个 ignored 为仓库声明过的 crash_child / regenerate_v1_fixture。5 个 0 用例目标逐条交代：identity_auth lib、tests\\local_endpoint_unix.rs（整文件 #[cfg(unix)]，本机 host x86_64-pc-windows-msvc 不参与编译，Linux CI 覆盖）、3 个无 doctest 的 Doc-tests。执行环境满足 plan 对 [PV3] 行的独立目标目录意图：CARGO_TARGET_DIR 未设置、worktree 内无 .cargo/config.toml，cargo 使用本 worktree 自有的 target/。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.7"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "59bf1058851deb5b83c875a02f8046c815a9c4f0"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "A 段（派单固定提交，§W1/§W2/§W3）：四 crate 命令退出码 **0**（25 s），**477 passed / 0 failed / 0 ignored**；`-p server` declared 253 == 执行 253、`node_link::conn` declared 32 == 执行 32；存储附加命令 exit 0、**120 passed / 0 failed / 2 ignored**、admin_store 40 passed（含 4 条 fix1b 新用例逐条 ok）。用例名集合与 B 段完全相同（477/477 行，按名字排序后 diff 为空），harness 计数逐项一致。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.7"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "2937850f607ea54792a55649a82b7f6878808830"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "openspec/changes/node-link-owner/reports/pv5-windows-nodelink.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "B 段（当前 HEAD）。权威 tasks.md:3.7 的 [PV5]（本机）行：在本机 Windows（MINGW64 / x86_64-pc-windows-msvc，cargo 1.98.1 / rustc 1.98.1）执行 `cargo test --locked -p server -p app --all-features`：退出码 **0**（81 s，head_before == head_after == 2937850），**325 passed / 0 failed / 0 ignored**（server 253 + app 72），13 个测试目标；`--list` 声明 325 == 执行 325；非 ok result 行 0 条、FAILED 0 条、panicked 0 条。连接/握手层真实执行：`node_link::conn` **32/32**。平台差异用例真实执行：transport::net::tests::direct_mode_permissions_are_unverifiable_on_this_platform、transport::net::permissions::tests::windows_inspection_is_unverifiable、transport::local::platform::windows::tests::a_failed_connect_leaves_the_endpoint_usable、transport::local::platform::windows::tests::a_cancelled_accept_drops_the_pending_instance_and_kills_the_connected_client，以及 tests/local_endpoint_windows.rs 4/4。如实记录未执行项：tests/local_endpoint_unix.rs 整文件 #[cfg(unix)]（本机不参与编译，Linux CI 覆盖；这是平台差异的真实状态，不是通过）；真实第二 OS 账号的跨用户 ACL 端到端检查本轮同样未执行（平台限制，沿用 WP2 轮登记）。4 个 0 用例目标已逐条交代（app 的 src\\main.rs、tests/local_endpoint_unix.rs、Doc-tests app/server）。跨 WP 的完整受控链路（配对→握手→catalog→attach→event→command→撤销）仍是 WP7 的 [PV5] 行（2.22 / 3.13 / 6.5），本任务不含。无进程/端口/临时目录新增残留。原始输出在本文件所属日志的「WP4-fix 重跑轮次」分节。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.7"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "59bf1058851deb5b83c875a02f8046c815a9c4f0"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "openspec/changes/node-link-owner/reports/pv5-windows-nodelink.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "A 段：同一条本机 Windows 命令的实测窗口为 16:22:55 → 16:24:15（由结束 mtime 16:24:15 与 80 s 耗时反推），即**跨过** 16:23:17（起于该纯文档提交之前、止于其后）；该提交只改两份文档且测试二进制在 16:22 之前已编译完成，故测试语义与 59bf1058 相同，退出码 **0**、**325 passed / 0 failed / 0 ignored**、13 个目标、declared 325 == 执行 325、`node_link::conn` 32/32。A 段与 B 段的逐目标计数与用例名集合逐项相同（按名字排序后 diff 为空），故两个修订上的 [PV5] 结论一致。"
    source_evidence: NOT_APPLICABLE
```
