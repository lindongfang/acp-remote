# WP3 交付前 Project Verify 报告 — 任务 3.5（`node-link-owner`）

## Shared Report

- **task_id**: 3.5
- **role**: coder
- **phase**: implement
- **stage**: work-package
- **agent_context**: 任务级 coder 子 Agent（worker），不继承 WP1/WP2/WP3 的实现对话，只接收本任务固定输入；工具受限（无 subagent / 无委派），工作目录固定为变更分支 worktree `D:\Project\acp-remote-wt\node-link-owner`。**只执行检查、只写原始日志与本报告**；未修改任何代码、文档或规划文件（worktree 的 `git status --porcelain` 在轮次起点 §S17 与终点 §S24/§S26 均为 0 行，暂存区为空）。
- **target_revision**: `d127a802b75d63cd14789ea3ad504d89ce57f3f3`（分支 `agentic/node-link-owner`；WP3 的最后一个提交「fix(server): 配对安全头覆盖接入层预拒绝，并钉死节点 actor 语义」，`git diff --stat HEAD~1` = 8 个文件 / 365 insertions / 45 deletions；`HEAD` 是 `84678e4` 的后继，二者构成 WP3 的交付面）。
- **scope**: 任务 3.5 —— WP3 交付前 project verify。派单列 [PV3]、[PV1]、[PV2] 三项；**权威 `tasks.md:46` 的 3.5 原文要求的是「[PV3] + [PV5]（本机）」**，两者不一致，已向主 Agent 提出并获裁决 (B)：在派单三项之上**附加** [PV5] 本机轮次。本轮共执行四项检查，逐项记录完整命令、工具链版本、退出码与日志路径；核对用例确实执行（无零用例、无全跳过、`node_link` 配对用例真实执行）；核实资源已释放。**不做**独立 review（3.6）、不解释需求、不修复失败、不判定实现验收。
- **changes**: **无代码/文档/规划改动**。本轮是纯检查轮次：worktree 内 0 个被修改文件、0 个暂存文件（§S17、§S24、§S26 三次核实）；本报告与日志写在**仓库外的** `D:\Project\acp-remote\openspec\changes\node-link-owner\reports\`（该目录不在 worktree 内，因此检查动作不可能改变被检查对象的状态）。
- **checks**: [PV3] **PASS**、[PV1] **PASS**、[PV2] **PASS**、[PV5] **PASS**（逐项退出码见下表与日志）。
- **issues**: 无阻断项、无仓库失败项。需要下游知晓的是：(1) 派单检查集与 `tasks.md` 权威判据不一致，已按裁决补 [PV5]；(2) 我自己的留证工具缺陷 4 处（均已就地保留原始输出并另起分节更正，对四项检查的结论无影响，详见「issues」）；(3) `MSYS /tmp` 下有 4 个**更早轮次**残留的 `acpr-storage-*` 临时目录（时间戳早于本轮起点 40 分钟以上），按「只检查不改动」原则原样保留并如实登记为既有发现。
- **result**: **PASS**（W2/WP3 轮次；仅代表本任务四项检查通过，**不代表** 3.6 独立 review、7.1 替代验证或合并已完成）。
- **evidence_paths**:
  - 原始日志（本角色，**追加**在既有 W0/WP1、W1/WP2 内容之后的「W2/WP3 轮次」分节）：`openspec/changes/node-link-owner/reports/du1-pv1.log`
    - W0/WP1（任务 3.1）：§S1–§S8 —— **逐字节保留，未被覆盖**
    - W1/WP2（任务 3.3）：§S9–§S16 —— **逐字节保留，未被覆盖**
    - W2/WP3（本任务）：§S17 预检 / §S18 [PV3] 完整原始输出 / §S19 [PV3] 用例执行分析 / §S20 [PV1] `npm run verify` 完整原始输出 / §S21 [PV1] 逐子检查独立执行与各自退出码（C01–C10、C10a、C10b、R01–R03）/ §S22 [PV2] 完整原始输出 / §S23 [PV5] 本机 Windows 轮次完整原始输出 / §S24 + §S24b 资源与工作树收尾（含两处自查更正）/ §S25 轮次汇总 / §S26 最终工作树状态
    - 文件规模：5962 行 / 336,247 字节 → **9827 行 / 561,214 字节**（本次追加 3865 行）
  - `[PV5]` 留证（按 plan 指定的证据文件，**追加**「WP3 轮次」分节）：`openspec/changes/node-link-owner/reports/pv5-windows-nodelink.log`（450 行 / 31,817 字节，其中本轮 405 行）
  - 本报告：`openspec/changes/node-link-owner/reports/wp3-verify.md`
  - 实现轮次（2.7/2.8/2.9）的 [PV3] 记录：`reports/wp3-pairing-http.log` 与 `reports/wp3-handoff.md`；WP3 合同扩展轮次（2.25/2.26/2.27）的 [PV3] 记录：`reports/wp3-contract.log`、`reports/wp3-contract-handoff.md`。本报告是它们的**同一提交上的独立复跑**，互为佐证而非复用。
- **resource_cleanup**: 无端口/数据库/容器/账号/证书被创建或留存；只有本地 `node`/`npm`/`cargo`/`git` 调用。测试子进程零残留（按**精确进程名**匹配 `acpr-fake-acp-agent.exe` / `acp-remote.exe` / `server.exe` / `app.exe` / `cargo.exe` / `rustc.exe` / `link.exe` 共 0 个，§S24b）。默认监听端口 8765 上 0 个监听者（用例只绑 `127.0.0.1:0`）。本轮没有新建临时目录：`MSYS /tmp` 下 4 个 `acpr-storage-*` 目录的时间戳为 11:13:57–11:20:46，**早于本轮起点 12:02:32**，属更早轮次的既有残留，既非本轮产生、也按「只检查不改动」原则未删除（§S24b）。worktree 自有构建产物 `target/` 18G、`node_modules/` 71M 为 git-ignored 缓存，按任务约定无需清理。

### 环境（实测）

| 项 | 值 |
|---|---|
| 主机 / 外壳 | Windows（本机），git-bash，cwd `D:\Project\acp-remote-wt\node-link-owner` |
| node / npm | `v24.19.0` / `12.0.2`（≥ 合同门禁要求的 22.12） |
| rustc / cargo | `rustc 1.98.1 (48a229cea 2026-09-01)` / `cargo 1.98.1 (797e8a9bc 2026-08-05)`，host `x86_64-pc-windows-msvc`，与 `rust-toolchain.toml` 的 `channel = "1.98.1"` 一致 |
| `CARGO_TARGET_DIR` | 未设置；worktree 内无 `.cargo/config.toml`，因此 cargo 使用 **worktree 自有的** `target/`，与主检出及其他 worktree 物理隔离，满足 plan 的 PV3 行「独立 `CARGO_TARGET_DIR`」意图（§S17 记录了判断依据） |
| 轮次时间 | 2026-09-26 12:02:32 → 12:09:06 +0800（四条命令：PV3 3s、PV1 118s、逐子检查 ≈140s（12:04:57–12:07:16）、PV5 67s） |

### 检查结果（逐项退出码）

| ID | 命令（cwd = worktree 根） | 退出码 | 关键结果 | 日志位置 |
|---|---|---|---|---|
| **PV3** | `cargo test --locked -p server --all-features` | **0**（3s） | **221 passed / 0 failed / 0 ignored**；7 个测试目标；`--list` 声明 221 = 执行 221；`node_link::` **24/24 真实执行**、`transport::net::` 80/80 | §S18 / §S19 |
| **PV1** | `npm run verify` | **0**（118s） | 合同门禁 + `fmt`/`clippy`/全 workspace 测试全绿；workspace **839 passed / 0 failed / 2 ignored** | §S20 |
| PV1-a1 | `npm run check:schemas` | 0 | `schema fixtures OK: 118 valid, 24 invalid (ajv Draft 2020-12), 39 event views bound` | §S21 C01 |
| PV1-a2 | `npm run check:commands` | 0 | `command catalog OK: 12 commands` | §S21 C02 |
| PV1-a3 | `npm run check:errors` | 0 | `error registry OK: 58 codes across 2 protocols` | §S21 C03 |
| PV1-a4 | `npm run check:features` | 0 | `feature registry OK: 11 feature ids across 2 protocols` | §S21 C04 |
| PV1-a5 | `npm run check:assets` | 0 | `contract assets OK: 17 schemas, 155 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed` | §S21 C05 |
| PV1-a6 | `npm run check:acp` | 0 | `ACP compatibility matrix OK: 25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows, schema validated by ajv)` | §S21 C06 |
| PV1-a7 | `npm run check:docs` | 0 | `doc links OK: 378 relative links, 4086 section refs across 257 markdown files` | §S21 C07 |
| PV1-a8 | `npm run check:boundaries` | 0 | `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致` | §S21 C08 |
| PV1-a9 | `npm run check:drift` | 0 | `contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 88 个方法签名与 crates\core\src\ports.rs 一致` | §S21 C09 |
| PV1-a10 | `npm run check:agentic` | 0 | `Installation: PASS`；`Totals: 16 passed, 0 failed (16 items)`；宿主入口 17 个文件（C10a/C10b 两部分也都单独执行且各自 exit=0） | §S21 C10 / C10a / C10b |
| PV1-b1 | `cargo fmt --all -- --check` | 0（1s） | 无输出（无格式差异） | §S21 R01 |
| PV1-b2 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | 0 | `Finished dev profile … in 2.10s`，无诊断 | §S21 R02 |
| PV1-b3 | `cargo test --locked --workspace --all-features` | 0 | **839 passed / 0 failed / 2 ignored**；82 条 `test result` 行、65 套件用例数 >0、17 套件 0 用例，非通过行 0 条 | §S21 R03 |
| **PV2** | `node scripts/check-crate-boundaries.mjs` | **0** | `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）`；`cargo metadata` 实测 12 个 workspace 成员，与之一致（含 WP1 新增的 `server`→`acpr-wire` 格） | §S22 |
| **PV5** | `cargo test --locked -p server -p app --all-features`（Windows 本机） | **0**（67s） | **293 passed / 0 failed / 0 ignored**（server 221 + app 72），13 个测试目标；Windows 分支用例真实执行 | §S23 / `pv5-windows-nodelink.log` |

15 个 PV1 子检查**逐个独立执行**并各自记录退出码（脚本里没有任何 `&&` 串联），全部为 0；与 §S20 中按 `package.json` 串联的 `npm run verify` 退出码 0 一致，因此不存在「前序失败被 `&&` 掩盖」的情形。

### 用例确实执行（无零用例、无全跳过）

**PV3（`-p server`）**：`--list` 声明 221 个用例，执行输出 221 条 `test … ok`，逐字一致（221 == 221）。`node_link::` 声明 24、执行 24；`transport::net::` 声明 80、执行 80。**不是全跳过**：0 条 `... ignored`、0 条非 `ok` 用例行、0 条 `FAILED`/`panicked`。

- 7 条 `test result`：`lib` 193、`local_admin_channel` 14、`local_admin_schema_drift` 6、`local_endpoint_naming` 4、`local_endpoint_unix` **0**、`local_endpoint_windows` 4、`Doc-tests server` 0。
- 两个 0 用例套件已逐条交代：`tests/local_endpoint_unix.rs` 整文件是 `#[cfg(unix)]`，本机 host 为 `x86_64-pc-windows-msvc`（Linux CI 会跑到；本平台对应实现 `tests/local_endpoint_windows.rs` 本次 4/4 执行）；`Doc-tests server` 该提交下没有 doctest。二者都不是「套件被跳过」。

**`node_link` 配对用例真实执行（本任务点名要求）**：24 个 `node_link::` 用例全部 `ok`，覆盖 WP3 的三条交付线：

- claim 端点：`claim_enters_pending_confirmation_and_returns_a_verifiable_owner_proof`、`retrying_the_same_claim_returns_the_original_pairing_request`（幂等重试）、`repeated_claim_from_another_access_node_is_rejected_with_409`、`malformed_claim_body_is_rejected_with_400`、`expired_pairing_is_rejected_with_410`、`unknown_pairing_is_rejected_with_404`、`claiming_without_a_configured_origin_is_rejected_with_403`、`claim_body_over_the_transport_limit_is_rejected_with_413`、`claim_rate_limit_returns_429_after_ten_attempts_per_ip`；
- status 端点：`status_reports_the_five_business_states_with_200`、`status_with_an_invalid_proof_is_unauthorized`、`status_retry_with_the_same_nonce_returns_the_original_response`、`status_rate_limit_returns_429_after_sixty_queries_per_pairing`、`wire_status_covers_the_five_external_states`；
- 安全头/秘密边界（本提交新增）：`access_layer_rejections_on_the_pairing_paths_carry_the_security_headers`、`every_pairing_response_carries_the_four_security_headers`、`status_responses_carry_the_security_headers_and_no_secret`、`pairing_responses_never_carry_the_pairing_secret`、`invalid_proof_is_unauthorized_without_leaking_the_difference`、`endpoint_host_outside_the_configured_origin_is_rejected_with_403`。

**PV1（全 workspace）**：839 passed / 0 failed / 2 ignored；82 条 `test result` 行、65 套件用例数 >0、17 套件 0 用例（6 个无测试的 lib/bin 目标 + 11 个空 doc-test 套件），非通过行 0 条。83 条 `running …` 行与 82 条 result 行的差额沿用 W1/WP2 轮次 §S16 已核实的解释：多出的一条是声明为 `ignored` 的辅助项 `storage-sqlite tests\commit.rs::crash_child` 被 `crash_recovery_leaves_an_consistent_database` 作为子进程再次拉起时打印的，父进程的 result 行报 15 passed + 1 ignored = 该目标声明的 16 个用例，没有用例去向不明。

2 个 `ignored` 都是仓库**声明过的**辅助项，不是被跳过的覆盖：`crates/storage-sqlite/tests/commit.rs::crash_child`（注明「由 `crash_recovery_leaves_an_consistent_database` 拉起」）与 `crates/storage-sqlite/tests/migration.rs::regenerate_v1_fixture`（注明「夹具生成器：只在需要重建 `fixtures/storage/v2/from-v1.sqlite3` 时手动运行」）。

**PV5（本机 Windows，`-p server -p app`）**：293 passed / 0 failed / 0 ignored（server 221 + app 72），13 个测试目标：app `lib` 50 / `main` 0 / `audit_export` 1 / `cli_commands` 11 / `daemon_lifecycle` 10；server `lib` 193 / `local_admin_channel` 14 / `local_admin_schema_drift` 6 / `local_endpoint_naming` 4 / `local_endpoint_unix` 0 / `local_endpoint_windows` 4；`Doc-tests app` 0 / `Doc-tests server` 0。本机真实执行的平台相关用例：`transport::net::tests::direct_mode_permissions_are_unverifiable_on_this_platform`、`transport::net::permissions::tests::windows_inspection_is_unverifiable`、`transport::local::platform::windows::tests::a_failed_connect_leaves_the_endpoint_usable`、`transport::local::platform::windows::tests::a_cancelled_accept_drops_the_pending_instance_and_kills_the_connected_client`，外加 `tests/local_endpoint_windows.rs` 4/4。**如实记录**：`tests/local_endpoint_unix.rs`（整文件 `#[cfg(unix)]`）在本机不参与编译，由 Linux CI 覆盖——这是平台差异的真实状态，不是通过。

### 与上一轮（WP2 / 任务 3.3，提交 `4e8a107`）的交叉核对

| 口径 | WP2（4e8a107） | WP3（d127a802） | 差值 |
|---|---|---|---|
| server 用例 | 189 | **221** | +32（本提交新增 4 个安全头用例 + WP3 合同扩展期新增的 node_link/core seam 用例） |
| workspace 用例 | 790 | **839** | +49（server lib 161→193；其余来自 `core` / `identity-auth` / `storage-sqlite` 的合同扩展） |
| `ignored` 辅助项 | 2 | **2** | 0 |
| 0 用例套件 | 17 | **17** | 0 |
| 文档小节引用 | 4066 | **4086** | +20（WP3 的文档新增） |

- 2 个 `ignored` 中有一个**改了名字**：`migration.rs` 的夹具生成器在本分支由 `regenerate_v2_fixtures` 改名为 `regenerate_v1_fixture`（`git log -S` 定位为提交 `13f0a16`，WP3 合同扩展的 v2→v3 夹具工作），**计数不变、没有任何覆盖消失**。
- 0 用例套件、result 行数、非通过行数完全不变，说明 WP3 只新增用例，没有让既有用例消失或被跳过。

### 检查集与权威判据的差异（主 Agent 裁决记录）

- 派单列出的是 `[PV3]` + `[PV1]` + `[PV2]`。
- 权威 `openspec/changes/node-link-owner/tasks.md:46` 对 3.5 的原文是：**「[PV3] + [PV5]（本机）。完成条件：逐项通过并留证。」**（同节 3.7/3.9 等同为 `[PV3] + [PV5]（本机）`；`plan.md:556` 定义 [PV5] = `cargo test --locked -p server -p app --all-features` 的 `cfg(windows)` 用例 / 本机执行并留证，证据文件 `reports/pv5-windows-nodelink.log`，该文件头已写明「WP3–WP7 与最终验收继续追加」）。
- 该差异被提出后，主 Agent 裁决 **(B)**：在派单三项之上附加 [PV5] 本机轮次，并补进本报告的 handoff_index。本轮据此执行四项检查。**[PV1]/[PV2] 虽未写入 `tasks.md` 的 3.5 行，但它们是 plan 的 W8「PV1–PV5」与 `AGENTS.md` §8/§11 的通用收口要求，且 WP3 同时改动了 `core`/`storage-sqlite`/`identity-auth` 与合同文档，故一并执行并留证。**

### 日志位置（可复核性）

原始日志写在任务指定的 `D:\Project\acp-remote\openspec\changes\node-link-owner\reports\du1-pv1.log`，采用**追加**方式：既有 W0/WP1（§S1–§S8）与 W1/WP2（§S9–§S16）逐字节保留，本任务的「W2/WP3 轮次」从 §S17 起续写（分节一览见「evidence_paths」）。检查对象（worktree `D:\Project\acp-remote-wt\node-link-owner`）与留证位置（`D:\Project\acp-remote` 检出）物理分开，因此本轮**不可能**通过写日志影响被检查的仓库内容。

## issues（如实记录）

1. **派单检查集与权威 `tasks.md` 不一致（已裁决，非仓库缺陷）**：派单列 [PV3]/[PV1]/[PV2]，`tasks.md:46` 要求 [PV3] + [PV5]（本机）。已提请注意，主 Agent 裁决 (B) 附加 [PV5]，本轮据此执行并留证。若后续轮次沿用「派单即检查集」，建议在派单文案里显式核对 `tasks.md` 的 3.x 行。
2. **我自己的留证工具缺陷 4 处（全部就地保留原始输出并另起分节更正；对四项检查的结论无影响，非仓库缺陷）**：
   - **中文 `printf` 的 `--` 解析**：用于画分节下划线的 `printf '-----…'` 被 bash 当成选项，导致 §S19、§S21、§S24b 三处的分节下划线缺失（只是装饰行，正文完整）。已就地补回（§S19/§S21 的修正写在同一轮的收尾里；§S24b 的下划线在 §S24b 内补回）。
   - **node 的 `/tmp` 路径**：§S22 末尾我自加的 `cargo metadata` 探针用 `require("/tmp/meta.json")`，node 在 Windows 上把它解析成 `D:\tmp\meta.json` 而非 MSYS 的 `/tmp`，抛出 `MODULE_NOT_FOUND`（错误打在终端、未进日志）。§S22 的该段已改写为「说明这次自查勘误 + 修正后的结果」，并注明该探针**不属于 [PV2]**、对 [PV2] 结论无影响。
   - **并行追加交错**：S25 与 S26 两个分节由同一轮的两个 `>>` 追加并发写入，发生行级交错（S26 的标题与其部分内容被拆开）。已把 S24b 之后的尾部整段重写为连贯的 §S25 → §S26，S17–S24b 的内容未受影响（分节一览可复核）。
   - **未锚定的进程名 grep**：§S24 用 `grep -iE "…|app\.exe|…"` 匹配进程，substring 命中把无关的 `unsecapp.exe` / `LockApp.exe` / `wetype_server.exe` 也列了出来，看起来像有残留。§S24b 用**精确进程名**（`grep -xiE`）重查，结果 **0**，确认没有测试子进程残留。
3. **`MSYS /tmp` 下的 4 个 `acpr-storage-*` 临时目录（既有发现，非本轮产生）**：`acpr-storage-admin-pairing-consume-{expired,peer}-44584`、`acpr-storage-commit-update-semantics-{30772,39460}`，mtime 为 11:13:57–11:20:46，**早于本轮起点 12:02:32** 40 分钟以上，PID 与本轮无关。它们的产生者是 `crates/storage-sqlite/tests/support/mod.rs:93` 的 `temp_dir()`（`acpr-storage-{用例名}-{pid}`，由 `TempDir::drop` 尽力清理并重试 10 次）——即某次**被中断**的 storage 测试运行没有走到 `Drop`（`wp3-contract-handoff.md` 记录过一次中断后 resume 的运行）。**如实记录、按「只检查不改动」原则未删除**（删掉会毁掉那次中断的证据）。本轮自己在 12:02–12:08 跑了同一批 storage 目标，**没有新增任何残留**（最新的 `acpr*` 条目仍是 11:20:46）。这是**既有**状态，不构成本轮 FAIL；是否清理/是否算 3.5 的资源问题请主 Agent 判断。

## handoff_index

```yaml
handoff_index:
  - task_id: "3.5"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在该固定提交（分支 agentic/node-link-owner，轮次起点 §S17 与终点 §S24/§S26 的 `git status --porcelain` 均为 0 行、暂存区为空）上用本机固定工具链（rust-toolchain.toml=1.98.1 实测一致；node v24.19.0 / npm 12.0.2；Windows x86_64-pc-windows-msvc）执行 `npm run verify`：聚合退出码 0（§S20，118s），且 15 个子检查随后**逐个独立执行、各自 exit=0**（§S21 的 C01–C10、C10a、C10b、R01–R03），排除了 `&&` 掩盖前序失败的可能；其中 `cargo test --locked --workspace --all-features` 真实执行 839 passed / 0 failed / 2 ignored（82 条 result 行，非通过行 0 条）。本轮范围覆盖了 WP3 改动的 crate：server lib 193、core 102、storage-sqlite（admin_store 36 / migration 8+1 ignored / commit 15+1 ignored 等）与 identity-auth（authorization 15 / handshake 20 / pairing 30 …）。与 3.3 在 4e8a107 的 790 基线相比增量为 +49（server +32，其余来自 WP3 的 core/identity-auth/storage 合同扩展），且已核实没有既有用例消失或被跳过（ignored 仍为 2、0 用例套件仍为 17，§S25）——说明这是全新执行，不是复用 WP2 的结果。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.5"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一固定提交、同一环境上直接执行 `node scripts/check-crate-boundaries.mjs`：exit=0，输出 `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）`（含 WP1 新增的 `server`→`acpr-wire` 格），完整原始输出见 §S22，并同时作为 §S21 的 C08 单独复跑；旁证：`cargo metadata --no-deps` 实测 12 个 workspace 成员（acp-protocol、acpr-transcript、acpr-wire、agent-host、app、core、identity-auth、identity-keystore、node-link-protocol、server、storage-sqlite、sync-protocol），与门禁口径一致。WP1/WP2 的 PV2 证据在 028a4d2 / 4e8a107，本 WP 按 plan 要求在自己的交付提交上重新取得。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.5"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本任务的独立轮次：`cargo test --locked -p server --all-features` 在同一固定提交上 exit=0，221 passed / 0 failed / 0 ignored，7 条 test result 行（6 个测试目标 + `Doc-tests server`）；用例确实执行且无全跳过——`--list` 声明 221 与执行 221 逐字一致、`node_link::` 24/24、`transport::net::` 80/80，0 条 ignored、0 条非 ok 用例行；两个 0 用例套件（`local_endpoint_unix` 为 `#[cfg(unix)]`、`Doc-tests server` 无 doctest）已逐条交代（§S18/§S19）。派单点名的 `node_link` 配对用例全部真实执行，覆盖 claim/status 端点、限流、幂等重试与安全头（用例清单见「用例确实执行」一节）。执行环境满足 plan PV3 行的「独立 CARGO_TARGET_DIR」意图：worktree 内无 `.cargo/config.toml` 且 `CARGO_TARGET_DIR` 未设置，cargo 使用 worktree 自有 `target/`，与主检出及其他 worktree 物理隔离（§S17）。实现轮次（2.7/2.8/2.9 与 2.25–2.27）另有 [PV3] 记录（wp3-pairing-http.log / wp3-contract.log），本行是**同一提交上的独立复跑**，互为佐证而非复用。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.5"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "d127a802b75d63cd14789ea3ad504d89ce57f3f3"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "openspec/changes/node-link-owner/reports/pv5-windows-nodelink.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "权威 tasks.md:46 的 3.5 要求 [PV5]（本机）；派单漏列，经主 Agent 裁决 (B) 附加执行。在本机 Windows（`x86_64-pc-windows-msvc`，cargo 1.98.1）上执行 `cargo test --locked -p server -p app --all-features`：exit=0（67s），293 passed / 0 failed / 0 ignored（server 221 + app 72），13 个测试目标；平台差异用例**真实执行**并留证：`transport::net::tests::direct_mode_permissions_are_unverifiable_on_this_platform`、`transport::net::permissions::tests::windows_inspection_is_unverifiable`、`transport::local::platform::windows::tests::a_failed_connect_leaves_the_endpoint_usable`、`transport::local::platform::windows::tests::a_cancelled_accept_drops_the_pending_instance_and_kills_the_connected_client`，以及 `tests/local_endpoint_windows.rs` 4/4。**如实记录未执行项**：`tests/local_endpoint_unix.rs` 整文件 `#[cfg(unix)]`，本机不参与编译，由 Linux CI 覆盖；真实第二 OS 账号的跨用户 ACL 端到端检查因平台限制仍未执行；跨 WP 的完整受控链路（配对→握手→catalog→attach→event→command→撤销）属 WP7 的 [PV5] 行，本任务不含。原始输出两处留存：`pv5-windows-nodelink.log` 的「WP3 轮次」分节（本行 report_path）与 `du1-pv1.log` 的 §S23（同一轮次的完整原始输出）。"
    source_evidence: NOT_APPLICABLE
```

## 未执行项（不粉饰）

1. 独立复验（RV2）与任务 3.6 的独立 review 未执行——本报告不声称它们通过。
2. `cargo-deny`（`deps`/`advisories`）与 `gitleaks`（`secrets`）**只在 CI 运行**，本地没有等价物，本轮未执行；「本地绿」不等于这两类判定通过。
3. `#[cfg(unix)]` 的集成目标（`tests/local_endpoint_unix.rs`）在本机 Windows 不参与编译，本轮无法执行，由 Linux CI 覆盖。
4. 真实第二 OS 账号的跨用户 ACL 端到端检查（WP2 轮已登记的未执行项）本轮未执行。
5. 跨 WP 的完整受控链路集成测试（配对 → 握手 → catalog → attach → event → command → 撤销）属 WP7 的 [PV5] 行，本任务范围不含；本轮只覆盖 WP3 的配对 HTTP 段。
6. 本轮未对 4 个既有 `acpr-storage-*` 残留目录做清理（见 issues 第 3 条），也未做「这些残留是否会影响后续轮次」的实验。
