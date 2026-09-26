# WP4 交付前 project verify（任务 3.7）— `node-link-owner` / DU1

## Shared Report

- **task_id**：3.7（WP4 交付前 project verify；权威 `tasks.md:3.7` 原文 =「完成 WP4 交付前 project verify：`[PV3]` + `[PV5]`（本机）。完成条件：逐项通过并留证。」）
- **role / phase**：coder / verify（本任务**只执行检查与写报告**，未修改被检查仓库的任何文件）。
- **agent_context**：worker 子 Agent（本机 worktree 独占，未继承 WP4 实现轮次的对话；只读检查）。
- **target_revision**：`319c77b715eb218ee81696aeec8257d268e0930a`（分支 `agentic/node-link-owner`）。轮次起点与终点均实测 `git rev-parse HEAD` = 该值，`git status --porcelain` 为 0 行、`git diff --cached --numstat` 为 0 行。
- **scope（本任务实际执行的四项检查）**：
  - `[PV3]` `cargo test --locked -p server --all-features`，**另加** `cargo test --locked -p server -p core -p identity-auth -p node-link-protocol --all-features`（派单要求，因 WP4 涉及这些 crate）；
  - `[PV1]` `npm run verify`（聚合一次）+ **15 个子检查逐个独立执行并各自记录退出码**；
  - `[PV2]` `node scripts/check-crate-boundaries.mjs`；
  - `[PV5]` `cargo test --locked -p server -p app --all-features`（本机 Windows 轮次）。
- **checks**：四项全部 **exit 0**，无 FAIL，无零用例/全跳过，无端口/进程/临时目录残留。详见下表与 `reports/du1-pv1.log` §S27–§S36、`reports/pv5-windows-nodelink.log` 的「WP4 轮次」分节。
- **issues**：无阻断项。需要主 Agent 与 reviewer 知晓三条事实：① 派单所列 `[PV1]`/`[PV2]` 并未写进 `tasks.md:3.7` 那一行（该行只要求 `[PV3] + [PV5]`），但它们属 plan W8 的「PV1–PV5」与 `AGENTS.md` §8/§11 的通用收口要求，本轮一并执行并留证（口径与 3.5 轮一致）；② 我自己的留证工具有 4 处缺陷，全部就地如实记录并留档（见「issues」第 2 条与日志 §S34b），对四项检查结论无影响；③ WP3 轮登记的 4 个 `acpr-storage-*` 临时目录在本轮结束时**已不存在**，但本轮没有做运行前快照，因此「是谁在 12:08–15:39 之间删除的」无法由本任务确立（详见「issues」第 3 条）。
- **result**：**PASS**（本任务的四项检查全部满足）。**不代表** 3.8（RV1 独立 review）、6.5（候选替代验证）或合并已完成。
- **evidence_paths**：`reports/du1-pv1.log`（§S27–§S36，**追加**）、`reports/pv5-windows-nodelink.log`（「WP4 轮次」分节，**追加**）、本文件。
- **resource_cleanup**：未创建端口/数据库/容器/账号/证书；只有本机 `node`/`npm`/`cargo`/`git` 调用。测试子进程零残留（按**精确进程名**匹配 `acpr-fake-acp-agent.exe`/`acp-remote.exe`/`server.exe`/`app.exe`/`cargo.exe`/`rustc.exe`/`link.exe` = **0**，§S34）；默认监听端口 8765 上 0 个监听者（用例只绑 `127.0.0.1:0`）；MSYS `/tmp` 下 `acpr*` 条目 **0**（含 0 个 storage 测试临时目录）。本轮的临时产物全部在仓库之外（`D:\Project\acp-remote\...\reports\` 三个文件 + MSYS `/tmp/wp4/**` 抓包文件），其中 `D:\tmp\wp4\`（叙述分段文件的暂存目录）在消费后已删除。worktree 自有 `target/`（本 worktree 独占的 `CARGO_TARGET_DIR`）与 `node_modules/` 是 git-ignored 缓存，按约定保留。

## handoff_index

```yaml
handoff_index:
  - task_id: "3.7"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "319c77b715eb218ee81696aeec8257d268e0930a"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在固定提交 319c77b（分支 agentic/node-link-owner；轮次起点 §S27 与终点 §S34/§S36 的 `git status --porcelain` 均为 0 行、暂存区为空）上，用本机固定工具链（rust-toolchain.toml = 1.98.1，实测 rustc 1.98.1 / cargo 1.98.1；node v24.19.0 / npm 12.0.2；host x86_64-pc-windows-msvc）执行 `npm run verify`：聚合退出码 **0**（§S31，15:40:42→15:42:27，约 105 s），其中 `cargo test --locked --workspace --all-features` 真实执行 **876 passed / 0 failed / 2 ignored**（83 条 result 行，17 个 0 用例套件，非通过行 0 条）。为排除 `&&` 掩盖前序失败，随后把 `check` + `check:rust` 展开成的 **15 个子检查逐个单独执行、各自记录退出码**（§S32：C01–C10、C10a agentic-gate.mjs、C10b sync-agentic-host-entrypoints --check、R01 fmt、R02 clippy、R03 全 workspace 测试），**15/15 均 exit 0**，且 R03 独立复跑的计数与聚合跑逐项一致（876 / 0 / 2，result 行 83、0 用例套件 17、ignored 2）。2 个 ignored 都是仓库声明过的辅助项（storage-sqlite 的 `crash_child` 子进程目标与 `regenerate_v1_fixture` 夹具生成器），不是被跳过的覆盖。与 3.5 轮在 d127a802 的 839 基线相比 +37（server 221→248 的 27 个 WP4 新用例、node-link-protocol 的 structure_limits 等），ignored 仍为 2、0 用例套件仍为 17，说明这是全新执行而非复用上一轮结果。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.7"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "319c77b715eb218ee81696aeec8257d268e0930a"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一固定提交、同一环境下直接执行 `node scripts/check-crate-boundaries.mjs`：退出码 **0**，输出 `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）`（完整原始输出见 §S33，并作为 §S32 的 C08 单独复跑一次亦为 0）。旁证（不属 [PV2] 判据）：`cargo metadata --no-deps` 实测 workspace 成员 = **12**（acp-protocol、acpr-transcript、acpr-wire、agent-host、app、core、identity-auth、identity-keystore、node-link-protocol、server、storage-sqlite、sync-protocol），与门禁口径一致；WP1 新增的 `server`→`acpr-wire` 格与 WP4 的 seam 改动都没有改变依赖形状。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.7"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "319c77b715eb218ee81696aeec8257d268e0930a"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本任务的独立轮次：① 权威命令 `cargo test --locked -p server --all-features` 退出码 **0**，**248 passed / 0 failed / 0 ignored**，7 个测试目标（§S28）；`--list` 声明 248 与执行 248 逐字一致，0 条 ignored、0 条非 `ok` 用例行、0 条 `FAILED`、0 条 `panicked at`（§S30）。② 派单要求的附加范围 `cargo test --locked -p server -p core -p identity-auth -p node-link-protocol --all-features` 退出码 **0**，**472 passed / 0 failed / 0 ignored**（core 104、identity-auth 81 + 2 doctest、node-link-protocol 37、server 248）（§S29）。③ 派单点名要求确认的 `node_link::conn` 与握手用例**真实执行**：声明 27（`--list`）== 执行 27（全 `... ok`），构成 = 状态机 19 + `handshake::tests` 5 + `limits::tests` 3，含 `handshake_completes_and_enters_the_business_phase`、`a_handshake_that_never_gets_a_hello_is_closed_with_4408`（真实等满 15 s）、`silence_beyond_the_window_is_closed_with_4408`、`envelope_and_sequence_violations_are_rejected_without_closing`、`an_invalid_proof_is_closed_with_4401_and_audited`、`the_challenge_catalog_revision_is_the_proof_transcript_source` 等（§S30 逐条列出）。④ 两个 0 用例目标已逐条交代：`tests/local_endpoint_unix.rs` 整文件 `#[cfg(unix)]`（本机 host 为 x86_64-pc-windows-msvc，不参与编译，由 Linux CI 覆盖；本平台对照面 `local_endpoint_windows.rs` 本次 4/4 执行），`Doc-tests server` 该提交下没有 doctest。⑤ 执行环境满足 plan 对 [PV3] 行的「独立 CARGO_TARGET_DIR」意图：`CARGO_TARGET_DIR` 未设置且 worktree 内无 `.cargo/config.toml`，cargo 使用本 worktree 自有的 `target/`，与主检出及其他 worktree 物理隔离（§S27）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.7"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "319c77b715eb218ee81696aeec8257d268e0930a"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "openspec/changes/node-link-owner/reports/pv5-windows-nodelink.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "权威 `tasks.md:3.7` 的 [PV5]（本机）行。在本机 Windows（MINGW64 / x86_64-pc-windows-msvc，cargo 1.98.1）上执行 `cargo test --locked -p server -p app --all-features`：退出码 **0**（15:44:31→15:45:52，81 s），**320 passed / 0 failed / 0 ignored**（server 248 + app 72），13 个测试目标；`--list` 声明 320 == 执行 320，0 条 `FAILED`/`panicked at`/非 `ok` 用例行。WP4 主题在本次本机轮次中真实执行：`node_link::conn` **27/27**（握手、序号、心跳、慢连接、limits 只下调等连接层用例）。平台差异用例真实执行并留证：`transport::net::tests::direct_mode_permissions_are_unverifiable_on_this_platform`、`transport::net::permissions::tests::windows_inspection_is_unverifiable`、`transport::local::platform::windows::tests::a_failed_connect_leaves_the_endpoint_usable`、`transport::local::platform::windows::tests::a_cancelled_accept_drops_the_pending_instance_and_kills_the_connected_client`，以及 `tests/local_endpoint_windows.rs` 4/4。**如实记录未执行项**：`tests/local_endpoint_unix.rs` 整文件 `#[cfg(unix)]`，本机不参与编译（编译为 0 个用例），由 Linux CI 覆盖——这是平台差异的真实状态，不是通过；真实第二 OS 账号的跨用户 ACL 端到端检查本轮同样未执行（平台限制，沿用 WP2 轮登记）。跨 WP 的完整受控链路（配对→握手→catalog→attach→event→command→撤销）仍是 WP7 的 [PV5] 行（2.22 / 3.13 / 6.5），本任务不含。原始输出在本文件所属日志的「WP4 轮次」分节。"
    source_evidence: NOT_APPLICABLE
```

## 检查结果（逐项退出码）

| ID | 命令（cwd = worktree 根） | 退出码 | 关键结果 | 日志位置 |
|---|---|---|---|---|
| **PV3** | `cargo test --locked -p server --all-features` | **0** | **248 passed / 0 failed / 0 ignored**；7 个测试目标；`--list` 声明 248 == 执行 248；`node_link::conn` **27/27 真实执行** | §S28 / §S30 |
| PV3+ | `cargo test --locked -p server -p core -p identity-auth -p node-link-protocol --all-features` | **0** | **472 passed / 0 failed / 0 ignored**（core 104 / identity-auth 81+2 doctest / node-link-protocol 37 / server 248） | §S29 / §S30 |
| **PV1** | `npm run verify` | **0**（105 s） | 合同门禁 + `fmt`/`clippy`/全 workspace 测试全绿；workspace **876 passed / 0 failed / 2 ignored** | §S31 |
| PV1-C01 | `npm run check:schemas` | 0 | `118 valid, 25 invalid (ajv Draft 2020-12), 39 event views bound` | §S32 |
| PV1-C02 | `npm run check:commands` | 0 | `12 commands` | §S32 |
| PV1-C03 | `npm run check:errors` | 0 | `58 codes across 2 protocols` | §S32 |
| PV1-C04 | `npm run check:features` | 0 | `11 feature ids across 2 protocols` | §S32 |
| PV1-C05 | `npm run check:assets` | 0 | `17 schemas, 156 fixture files, 12 transcript vectors …, 20 negative vectors …` | §S32 |
| PV1-C06 | `npm run check:acp` | 0 | `25 methods, 11 updates, 71 rows`（ajv 校验） | §S32 |
| PV1-C07 | `npm run check:docs` | 0 | `378 relative links, 4095 section refs, 257 md files` | §S32 |
| PV1-C08 | `npm run check:boundaries` | 0 | `12 个 crate 的依赖方向与 §5 矩阵一致` | §S32 |
| PV1-C09 | `npm run check:drift` | 0 | `§7 的 36 条 DDL` + `§5 的 15 个 trait / 89 个方法签名` 与代码一致 | §S32 |
| PV1-C10 | `npm run check:agentic` | 0 | `Installation: PASS`；`Totals: 16 passed, 0 failed (16 items)`；宿主入口 17 个文件 | §S32 |
| PV1-C10a | `node scripts/agentic-gate.mjs` | 0 | `Installation: PASS`；16/16 items | §S32 |
| PV1-C10b | `node scripts/sync-agentic-host-entrypoints.mjs --check` | 0 | `agentic 宿主入口检查完成：17 个文件` | §S32 |
| PV1-R01 | `cargo fmt --all -- --check` | 0 | 无输出（无格式差异） | §S32 |
| PV1-R02 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | 0 | `Finished dev profile …`，无诊断 | §S32 |
| PV1-R03 | `cargo test --locked --workspace --all-features` | 0 | **876 passed / 0 failed / 2 ignored**；83 条 result 行、17 个 0 用例套件 | §S32 |
| **PV2** | `node scripts/check-crate-boundaries.mjs` | **0** | `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）`；`cargo metadata` 实测 12 个成员 | §S33 |
| **PV5** | `cargo test --locked -p server -p app --all-features`（本机 Windows） | **0**（81 s） | **320 passed / 0 failed / 0 ignored**（server 248 + app 72），13 个测试目标 | `pv5-windows-nodelink.log`「WP4 轮次」 |

**没有任何一条命令用 `&&` 串联后才记退出码**：15 个 PV1 子检查由一个 bash 驱动函数逐个单独执行，`$?` 在各自命令后立即写入 `/tmp/wp4/subs/summary.txt`（脚本内 `set -u`，无 `set -e`、无 `&&`），驱动原文与逐项原始输出见 §S32。聚合 `npm run verify` 与逐子检查两套结果一致（876 / 0 / 2），因此不存在「前序失败被 `&&` 掩盖」的情形。

## 用例确实执行（无零用例、无全跳过）

- **PV3（`-p server`）**：声明 248 == 执行 248；0 条 `... ignored`、0 条非 `ok` 用例行、0 条 `FAILED`、0 条 `panicked at`。
- **`node_link::conn` 与握手（派单点名）**：声明 27 == 执行 27，全部 `ok`；逐条清单见 §S30 与 `pv5-windows-nodelink.log` 的「WP4 轮次」分节。相较 WP3 轮只覆盖配对 HTTP 段，本轮把手握/序号/心跳/慢连接/limits 的连接层用例真实跑了一遍。
- **0 用例目标逐条交代**：`tests/local_endpoint_unix.rs`（整文件 `#[cfg(unix)]`，本机不参与编译，Linux CI 覆盖）与 `Doc-tests server`（该提交无 doctest）。二者都不是「套件被跳过」。
- **PV1（全 workspace）**：876 passed / 0 failed / 2 ignored；83 条 result 行、17 个 0 用例套件、非通过行 0 条。2 个 `ignored` 都是仓库**声明过的**辅助项（`crates/storage-sqlite/tests/commit.rs::crash_child` 由 `crash_recovery_leaves_an_consistent_database` 作为子进程拉起；`crates/storage-sqlite/tests/migration.rs::regenerate_v1_fixture` 是手动运行的夹具生成器），不是被跳过的覆盖。本轮抓到的 83 条 `Running` 行与 83 条 result 行相等，因为被拉起的 `crash_child` 子进程会把它自己的 `Running ...` 行打印进同一输出流。
- **PV5（本机 Windows）**：320 passed / 0 failed / 0 ignored；13 个目标；声明 320 == 执行 320；Windows 分支用例真实执行。

## 与上一轮（WP3 / 任务 3.5，提交 `d127a802`）的交叉核对

| 口径 | WP3（d127a802） | 本轮（319c77b） | 差值 |
|---|---|---|---|
| server 用例 | 221 | **248** | +27（WP4 的 `node_link::conn` 27 个用例 + 2.29/2.30 新增的 wire 修订用例） |
| workspace 用例 | 839 | **876** | +37（server +27，node-link-protocol +6 等） |
| PV5 用例 | 293 | **320** | +27 |
| `ignored` 辅助项 | 2 | **2** | 0 |
| 0 用例套件 | 17 | **17** | 0 |
| 文档小节引用 | 4086 | **4095** | +9（WP4 的文档新增） |
| §5 端口签名 | 88 | **89** | +1（WP4 seam `TrustStore::pairing_for`） |
| fixture 文件 / 非法 schema fixture | 155 / 24 | **156 / 25** | +1 / +1（2.29 的 `catalogRevision` 正反用例） |

ignored 与 0 用例套件计数不变、非通过行 0 条，说明 WP4 只新增用例，没有让既有用例消失或被跳过。

## 环境（实测）

| 项 | 值 |
|---|---|
| 主机 / 外壳 | Windows（本机），git-bash（MINGW64），cwd `D:\Project\acp-remote-wt\node-link-owner` |
| node / npm | `v24.19.0` / `12.0.2`（≥ 合同门禁要求的 22.12） |
| rustc / cargo | `rustc 1.98.1 (48a229cea 2026-09-01)` / `cargo 1.98.1 (797e8a9bc 2026-08-05)`，host `x86_64-pc-windows-msvc`，与 `rust-toolchain.toml` 的 `channel = "1.98.1"` 一致 |
| `CARGO_TARGET_DIR` | 未设置；worktree 内无 `.cargo/config.toml`，因此 cargo 使用本 worktree 自有的 `target/` |
| 轮次时间 | 2026-09-26 15:39:46 → 15:45:58 +0800（PV3 两次 ≈20 s+22 s、PV1 聚合 105 s、15 个子检查 ≈110 s、PV2 <1 s、PV5 81 s） |

## 日志位置与追加完整性（可复核性）

- `reports/du1-pv1.log`：**追加**「W3/WP4 轮次」分节（§S27 预检 / §S28 [PV3] 原始完整输出 / §S29 [PV3] 四 crate 原始输出 / §S30 [PV3] 执行分析 / §S31 [PV1] 聚合原始输出 / §S32 [PV1] 逐子检查原始输出与退出码 / §S33 [PV2] 原始输出 / §S34 资源与工作树收尾 / §S35 轮次汇总 / §S36 最终工作树状态 / §S34b 自查与更正）。追加前 **9827 行 / 561,214 字节 / md5 `acaaa11293e659316510c6624f7d743a`**，追加后 **14,307 行 / 822,382 字节**；追加后文件**前 561,214 字节的 md5 仍为 `acaaa11293e659316510c6624f7d743a`**，即既有 S1–S26（W0/WP1、W1/WP2、W2/WP3）逐字节保留、未被覆盖。
- `reports/pv5-windows-nodelink.log`：**追加**「WP4 轮次」分节。追加前 **450 行 / 31,817 字节 / md5 `3b6dd4901a90e2806e3ee5f7e0d57ca5`**，追加后 **924 行 / 66,317 字节**；前 31,817 字节的 md5 仍为原值。
- 检查对象（`D:\Project\acp-remote-wt\node-link-owner`）与留证位置（`D:\Project\acp-remote` 检出）物理分开，因此本轮**不可能**通过写日志影响被检查仓库的内容（§S34 实测 `git status --porcelain` = 0 行）。

## issues（如实记录）

1. **派单检查集与 `tasks.md:3.7` 的差异（非仓库缺陷）**：派单列 `[PV3]`+`[PV1]`+`[PV2]`+`[PV5]`；权威 `tasks.md:3.7` 只写「`[PV3]` + `[PV5]`（本机）」。`[PV1]`/`[PV2]` 属 plan W8「PV1–PV5」与 `AGENTS.md` §8/§11 的通用收口要求，且本轮涉及 `core`/`identity-auth`/`node-link-protocol` 与合同资产，故一并执行并留证（与 3.5 轮的口径一致）。本轮**没有**出现 3.5 轮那种「主 Agent 裁决补充」的情形——派单本身已含四项，无需新裁决。
2. **我自己 4 处留证工具缺陷（全部就地保留原始输出并另起分节更正；对四项检查结论无影响，非仓库缺陷）**：
   - **首次追加脚本被命令长度上限截断**：bash 报 `here-document … delimited by end-of-file`，尾部 `bash …` 未执行，**未产生任何追加**；当时核实日志仍是原样（md5 `acaaa112…`）。
   - **叙述分段文件写错位置**：文件写入工具把字面路径 `/tmp/...` 解析为 Windows 原生路径，分段文件实际落在 `D:\tmp\wp4\`（MSYS `/tmp` 是 `C:\Users\zhang\AppData\Local\Temp`），导致第一次 `drive.sh` 追加出来的原始抓包**缺少分节标题**（13,801 行 / 792,106 字节）。该次部分追加**已刻意回滚**：备份为 `/tmp/wp4/du1-mangled-backup.log`，用 `truncate -s 561214` 恢复到原字节数并核实 md5 = `acaaa112…`，修正路径后重跑追加，得到现在完整的 §S27–§S36。（同一 `/tmp` 解析差异也是 §S33 里 `require('/tmp/...')` 抛 `MODULE_NOT_FOUND` 的原因。）
   - **进程名检查第一次「0 个」是错的原因**：git-bash 把 `tasklist` 的 `/fo` `/nh` 重写成路径，tasklist 拒绝参数，awk 因此什么也没看到。已用 `MSYS_NO_PATHCONV=1` 重跑并先打印原始输出佐证，0 才是真实值（§S34）。
   - **一次未锚定的 grep 误报**：宽松的 `grep -i "FAILED|panicked"` 命中 9/27 行，全部是测试名（`a_failed_settlement_…`、`a_failed_connect_…`）与 `test result:` 行里的字面 `0 failed`；改用锚定的 `^test .* FAILED$` 与 `panicked at` 后为 0（§S30）。原始误报行未删除，一并留在日志里。
3. **`acpr-storage-*` 临时目录：WP3 记录存在、本轮结束时不存在（既有状态，非本轮产生）**：`§S24b`（12:08）记录了 4 个 `acpr-storage-*` 目录（mtime 11:13:57–11:20:46，属更早的一次被中断的 storage 运行），当时刻意**未删除**以保留证据。本轮 15:45:58 实测 MSYS `/tmp` 下 `acpr*` 条目为 **0**。本轮**没有做运行前快照**，因此「是谁在 12:08–15:39 之间删除了它们」无法由本任务确立（本轮自身只执行检查命令，未删除任何东西；`D:\tmp` 下另有 5 个 11:16–11:20 的 `acpr-*.cjs/.md` 旧脚本文件，与本轮无关，也未删除）。本轮自己跑过的 storage 目标（R03 全 workspace、四 crate 追加轮）**没有留下任何新残留**。这是**既有**状态，不构成本轮 FAIL；是否需要在后续轮次冻结该目录作为资源项，请主 Agent 判断。
4. `[PV3]` 的两条命令都在 `-p server` 上重复执行（§S28 与 §S29 各一次），因此 server lib 的 15 s 级用例被真实跑了两遍——这是派单「另加 `-p core -p identity-auth -p node-link-protocol`」的直译结果（保留权威命令原样，再补一次超集），不是重复计数；报告里两处计数分别列示、不叠加。

## 未执行项（不粉饰）

1. **独立 review（任务 3.8 / RV1）未执行**——本报告不声称它通过；WP4 的只读隔离检视归 3.8。
2. **候选/替代验证（6.5）与跨 WP 全链路不属本任务**：完整受控链路（配对 → 握手 → catalog → attach → event → command → 撤销）是 WP7 的 `[PV5]` 行（2.22 / 3.13 / 6.5），本任务只覆盖 WP4 的连接层。
3. **`cargo-deny`（`deps`/`advisories`）与 `gitleaks`（`secrets`）只在 CI 运行**，本地没有等价物，本轮未执行；「本地绿」不等于这两类判定通过。
4. **`#[cfg(unix)]` 的集成目标**（`tests/local_endpoint_unix.rs`）在本机 Windows 不参与编译，本轮无法执行，由 Linux CI 覆盖；真实第二 OS 账号的跨用户 ACL 端到端检查（WP2 轮已登记）本轮同样未执行（平台限制）。
