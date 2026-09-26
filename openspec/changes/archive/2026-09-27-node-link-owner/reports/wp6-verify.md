# WP6 交付前 project verify（任务 3.11 / WP6）— `node-link-owner` / DU1

## Shared Report

- **task_id**：3.11（WP6；权威 `tasks.md:55` 原文 =「3.11 WP6；前置：2.20；实现 Agent（coder）。完成 WP6 交付前 project verify：[PV3] + [PV5]（本机）。完成条件：逐项通过并留证。」）。本次派单在该两项之外**另点名 [PV1] 与 [PV2]**，本报告按派单四项全给。
- **role / phase**：coder / verify —— 本任务**只执行检查与写报告，未修改被检查仓库的任何文件**（工作区终点 `git status --porcelain` 仍为 0 行、暂存区为空、无 stash；本轮所有写入都在权威规划根的 `reports/` 下）。
- **agent_context**：worker 子 Agent（worktree `D:\Project\acp-remote-wt\node-link-owner` 独占会话；只读检查 + 追加日志与报告）。权威规划根 `D:\Project\acp-remote`，报告与日志写在 `openspec/changes/node-link-owner/reports/`（该目录不入版本控制）。
- **target_revision**：`8bc2ff3e078549cebed2ab99b5981d5ed37e0581`（= 派单固定提交 = 分支 `agentic/node-link-owner` 的 HEAD，WP6 的最后一个提交 `8bc2ff3 test(node-link): 按 R45 的下调实例覆盖 in-flight 上限`）。
  轮次**起点（`10:52:18Z` preflight）与终点（`11:00:43Z`）**均实测 `git rev-parse HEAD` = 该值、`git status --porcelain | wc -l` = 0、暂存区 0 行、`git stash list | wc -l` = 0；**无修订漂移**。四条命令各自在执行前后又取了一次 HEAD，全部相同（`du1-pv1.log` §WV0/§WV4 的逐条 `HEAD_BEFORE`/`HEAD_AFTER`）。
- **scope（本任务实际执行的检查 + 收尾）**：
  - `[PV3]` `cargo test --locked -p server -p core -p storage-sqlite -p identity-auth -p node-link-protocol --all-features`（派单命令原文，**含对 `command` 用例真实执行的确认**）；
  - `[PV1]` `npm run verify`（聚合 1 次）+ 把 `check`/`check:rust` 展开成的 **15 条子检查逐个独立执行、各自记录退出码**（不用 `&&` 掩盖前序失败）；
  - `[PV2]` `node scripts/check-crate-boundaries.mjs`（raw）；
  - `[PV5]` `cargo test --locked -p server -p app --all-features`（本机 Windows）；
  - 收尾：资源残留核实（进程 / 端口 / `%TEMP%` 的 `acpr-*`）+ worktree 终点状态 + declared 交叉核对（只读 `--list`）。
- **checks**：**四项全部 exit 0，无 FAIL**；无「零用例目标或整 target 静默跳过」被当成通过；无新增进程 / 监听端口 / 临时目录残留。
  逐项见下表与 `reports/du1-pv1.log` 的「W5/WP6 轮次」分节（**追加**，分隔线第 27837 行、标题第 27838 行，§WV0–§WV8）、`reports/pv5-windows-nodelink.log` 的「WP6 轮次」分节（**追加**，分隔线第 3111 行、标题第 3112 行）。
- **result**：**PASS**（本任务四项检查在固定提交 `8bc2ff3e…` 上全部通过）。**不代表** 3.12 的独立 review、WP7（2.21–2.23、3.13）、
  受控路径全链路 E2E（2.22）、6.5（候选替代验证）或合并已完成；也不代表跨实现（Linux/CI）轮次已跑。
- **evidence_paths**：`openspec/changes/node-link-owner/reports/du1-pv1.log`（「W5/WP6 轮次」分节）、
  `openspec/changes/node-link-owner/reports/pv5-windows-nodelink.log`（「WP6 轮次」分节）、本文件。
- **resource_cleanup**：本轮**未新增**任何进程 / 监听端口 / 数据库 / 容器 / 账号 / 证书 / 依赖；未改 `Cargo.toml`、`Cargo.lock`、`schemas/**`、`compatibility/**`、`fixtures/**`、`docs/**`、`openspec/**`。
  只读检查结束后实测：精确镜像名匹配 `cargo.exe`/`rustc.exe`/`link.exe`/`acpr-fake-acp-agent.exe`/`acp-remote.exe`/`acp_remote.exe`/`server.exe`/`app.exe`/`admin_store.exe` = **0**（`grep` exit 1）；`netstat -ano` 中 `8765` = **0**（`grep -w` exit 1）。
  MSYS `/tmp`（= `C:\Users\zhang\AppData\Local\Temp`）中发现 **1 项 `acpr-*` 遗留**：`/tmp/acpr-wp4a-maintenance-period-34212-8`（365K，WP4a 轮次的 daemon 临时目录：maintenance-period 配置、daemon 日志、`data/` 下的 SQLite 三件套与 instance/lock 文件；由本变更早前轮次创建，非本轮产物）——按派单「属本变更早前轮次的可删除并记录」**已删除**（逐文件 + 逐层 `rmdir`，全部 exit 0），删除后 `ls -d /tmp/acpr-*` exit 2。
  本轮自建 scratch `/tmp/wp6v/`（仓库外）在报告落盘后删除。worktree 自有的 `target/` 与 `node_modules/` 是 git-ignored 构建缓存，按约定保留。

## 检查结果（逐项退出码）

| ID | 命令（cwd = worktree 根） | 退出码 | 耗时 | 关键结果 | 日志位置 |
|---|---|---|---|---|---|
| **PV3** | `cargo test --locked -p server -p core -p storage-sqlite -p identity-auth -p node-link-protocol --all-features` | **0** | 38 s | **654 passed / 0 failed / 2 ignored**；39 个测试目标全 `ok`；declared 656 = 654 + 2 ignored | `du1-pv1.log` §WV1/§WV2/§WV7c |
| **PV1** | `npm run verify`（聚合） | **0** | 106 s | workspace **942 passed / 0 failed / 2 ignored**；83 个测试目标；declared 944 = 942 + 2 ignored | `du1-pv1.log` §WV3/§WV7c |
| PV1-C01 | `npm run check:schemas` | **0** | 1 s | `118 valid, 25 invalid (ajv Draft 2020-12), 39 event views bound` | `du1-pv1.log` §WV4 |
| PV1-C02 | `npm run check:commands` | **0** | 0 s | `12 commands` | 同上 |
| PV1-C03 | `npm run check:errors` | **0** | 0 s | `58 codes across 2 protocols` | 同上 |
| PV1-C04 | `npm run check:features` | **0** | 1 s | `11 feature ids across 2 protocols` | 同上 |
| PV1-C05 | `npm run check:assets` | **0** | 1 s | `17 schemas, 156 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed` | 同上 |
| PV1-C06 | `npm run check:acp` | **0** | 1 s | `25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows, ajv validated)` | 同上 |
| PV1-C07 | `npm run check:docs` | **0** | 1 s | `378 relative links, 4121 section refs across 257 markdown files` | 同上 |
| PV1-C08 | `npm run check:boundaries` | **0** | 1 s | `12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）` | 同上 |
| PV1-C09 | `npm run check:drift` | **0** | 1 s | `§7 的 36 条 DDL` 与 `migrate.rs` 一致；`§5 的 15 个 trait / 93 个方法签名` 与 `ports.rs` 一致 | 同上 |
| PV1-C10 | `npm run check:agentic` | **0** | 2 s | `Totals: 16 passed, 0 failed (16 items)` + `宿主入口检查完成：17 个文件` | 同上 |
| PV1-C10a | `node scripts/agentic-gate.mjs` | **0** | 1 s | 16/16 items | 同上 |
| PV1-C10b | `node scripts/sync-agentic-host-entrypoints.mjs --check` | **0** | 0 s | `agentic 宿主入口检查完成：17 个文件` | 同上 |
| PV1-R01 | `cargo fmt --all -- --check` | **0** | 1 s | 无输出（无格式差异） | 同上 |
| PV1-R02 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | **0** | 1 s | 无诊断（**指纹缓存命中**，见 issues 第 2 条） | 同上 |
| PV1-R03 | `cargo test --locked --workspace --all-features` | **0** | 97 s | **942 passed / 0 failed / 2 ignored**；83 个目标；与聚合跑逐项一致 | 同上 |
| **PV2** | `node scripts/check-crate-boundaries.mjs` | **0** | 1 s | `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）` | `du1-pv1.log` §WV5 |
| **PV5** | `cargo test --locked -p server -p app --all-features`（本机 Windows） | **0** | 81 s | **376 passed / 0 failed / 0 ignored**；13 个目标；declared 376 == executed 376 | `pv5-windows-nodelink.log` §WP6-PV5-1…5 |
| — | `ls -d /tmp/acpr-*`、进程/端口精确匹配 | — | — | 删除后残留 = 0（`acpr-*` exit 2、进程 exit 1、端口 exit 1） | `du1-pv1.log` §WV7 |

**子检查合计：15/15 exit 0**，与 `npm run verify` 聚合退出码 0 一致（无「聚合绿、单项红」或反过来的矛盾）。
环境：Windows / git-bash；`rust-toolchain.toml` 固定 `1.98.1`，实测 cargo 1.98.1（`797e8a9bc`）、rustc 1.98.1（`48a229cea`）、node v24.19.0、npm 12.0.2。

## 派单点名要求：`command` 用例是否真实执行

派单要求「记录计数；确认 command 用例真实执行」。逐条留证：

| 模块 | 声明数（只读 `--list`） | 执行数（`... ok`） | WP6 新增 |
|---|---|---|---|
| `node_link::command::tests` | **25** | **25** | 25（本 WP 全新模块） |
| `node_link::catalog::tests` | **6** | **6** | 0（WP5-fix1） |
| `node_link::resource::tests` | **19** | **19** | 2（`current_attachment_session_ref` / `revoke_export` 两个 additive 方法） |
| `local_admin::router::tests` | **35** | **35** | 1（R76 撤销通知） |
| `node_link::*` 合计 | **106** | **106** | 27 |
| `server` lib 合计 | **276** | **276** | 28 |

25 条 `node_link::command::tests` 用例名在 `du1-pv1.log` §WV2 逐条列出（每条带原文行号 + `... ok`），覆盖 R66–R75、R77、R82/R83 的适配层判定；
另两条 WP6 关键新增用例也在同一节逐条列出：
`use_cases::tests::owner_side_node_commands_without_a_session_use_the_trust_record_and_a_covering_export`（core 授权判定修正，`3638ebd`）与
`local_admin::router::tests::export_revoke_notifies_the_connection_closer_after_the_persisted_commit`（撤销缝，`4cbdf8a`）。
`declared - ok - ignored = 0`，没有任何 WP6 用例被静默跳过。

**与 WP5-fix 轮次（`27ad3f80`）的计数对照**（说明本轮没有静默减少覆盖，且新增用例在 Windows 上真的跑了）：

| 项 | WP5-fix 轮次 | 本轮（`8bc2ff3e`） | 差值 | 说明 |
|---|---|---|---|---|
| `[PV3]` 计数 | 625 / 0 / 2（39 目标） | **654 / 0 / 2（39 目标）** | **+29** | server lib +28（command 25 + resource 2 + router 1）+ core lib +1 |
| `[PV1]`/`[R03]` workspace | 913 / 0 / 2（83 目标） | **942 / 0 / 2（83 目标）** | **+29** | 同上，既有用例一条未减 |
| `[PV5]` 本机 Windows | 348 / 0 / 0（13 目标） | **376 / 0 / 0（13 目标）** | **+28** | 增量全部落在 server lib |
| `node_link::conn` 形状（WP4 冻结） | 32/32 全绿 | **32/32 全绿**（24 + handshake 5 + limits 3） | 0 | 未被 WP6 改坏 |
| 声明 vs 执行 | 915 = 913 + 2 | **944 = 942 + 2** | +29 | 无未执行用例 |

## issues（需要主 Agent / reviewer 知晓的事实）

1. **`tasks.md` 的 3.11 只写 [PV3] + [PV5]，本次派单另点了 [PV1] 与 [PV2]**：本报告四项全给，handoff 仍分四行逐项标注证据节，不把额外检查当成额外完成度声明（多给不等于多声称）。
2. **`npm run check:rust` 的 clippy 是 cargo 指纹缓存命中（如实登记，不是造假也不是跳过）**：`[PV1]` 聚合内的 `cargo clippy` 与单独执行的 R02 都只花 1 s、无任何 `Checking` 行，即工作区源码与上次 clippy 指纹一致（WP6 各提交前的 `.husky/pre-commit` 已对**同一份源码**跑过 workspace clippy），cargo 直接判为 fresh。两次都 **exit 0、零诊断**，判定有效；但「本轮新跑了一次 clippy-driver」这句话不成立。若要「强制重跑」的证据需换 target dir 或改文件 mtime —— 前者成本高、后者违反本轮「不修改文件」约束，**均未执行**。
3. **子检查条数的口径**：`package.json` 的 `verify` = `check` + `check:rust`。`check` 有 10 个 npm script，其中 `check:agentic` 内部串了 2 条命令；`check:rust` 内部串了 3 条命令。本轮既执行了 10 个 npm script，又把 `check:agentic` 的 2 条与 `check:rust` 的 3 条展开单独各跑一次，故可独立执行的命令是 **15 条**（C01–C09、C10、C10a、C10b、R01、R02、R03），**15/15 exit 0**。若 reviewer 只按「npm script 计数」看是 13 条（10 + 3）；口径差异已在 §WV4 逐条列明命令与退出码，结论不变。
4. **清理动作的机制说明**：本机工具的通用安全确认**拦截了递归删除型命令**（含 `find … -delete` 与循环内 `rm`），`du1-pv1.log` §WV7 有原始记录；因此改用等价的逐文件删除 → 逐层 `rmdir`（`data/attachments` → `data` → 根目录），12 步全部 exit 0。删除对象是 WP4a 轮次遗留的、位于仓库之外的 daemon 临时目录；**没有触碰仓库内任何文件**。
5. **`%TEMP%` 里仍有其它轮次（非 `acpr-*`）的 scratch，按派单授权范围保留**：`/tmp/wp1*`、`/tmp/wp2*`、`/tmp/wp3*`、`/tmp/wp4*`、`/tmp/wp5*`、`/tmp/wp6_*.rs|txt` 以及若干脚本/日志（清单见 §WV7）。它们**不是 `acpr-*` 前缀**（不是被测用例产生的临时 daemon 目录），mtime 早于本轮起点，属本变更早前轮次的产物；派单只授权删 `/tmp/acpr-*`，故仅登记不删除。若主 Agent 希望在归档前一并清空这些 scratch，需要再给一次明确授权。
6. **本轮是只读轮次**：未改任何文件（含未改 `tasks.md` / `verification.md` / `plan.md` / `docs/**`）；`tests/local_endpoint_unix.rs` 在本机是 0 用例目标（整文件 `#[cfg(unix)]`，Windows 上被编译掉），由 Linux CI 覆盖，不是静默跳过；`acp_remote` CLI 二进制与 `Doc-tests app/server` 的 0 条目标也逐条交代（§WV2、§WP6-PV5-4）。
7. **未执行（如实记录，与 WP4/WP5 轮次口径一致）**：`cargo-deny`（依赖许可证/来源/advisory）与 `gitleaks`（密钥扫描）**只在 CI 运行**，本地无等价物、不在 `npm run verify` 里；本机未装 gitleaks。因此「本地绿」不等于这两类判定通过，合并前仍以 CI 的 `deps`/`advisories`/`secrets` 三个 job 为准。
8. **`check:docs` 的 section refs 从 4114 涨到 4121**：WP6 新增了 `command.rs` 的模块契约注释与 `CORE_PORTS_AND_STORAGE.md` 的引用点，文档引用门禁仍 exit 0（只增不减，无悬空引用）。

## handoff_index

```yaml
handoff_index:
  - task_id: "3.11"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "8bc2ff3e078549cebed2ab99b5981d5ed37e0581"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "固定提交 8bc2ff3e（分支 agentic/node-link-owner；§WV0 起点与 §WV7b 终点的 `git rev-parse HEAD` 一致、`git status --porcelain | wc -l` 均为 0、暂存区 0 行、无 stash）上，用固定工具链（rust-toolchain.toml = 1.98.1，实测 cargo 1.98.1 / rustc 1.98.1；node v24.19.0 / npm 12.0.2）执行 `npm run verify`：**聚合退出码 0**（§WV3，106 s），其中 `cargo test --locked --workspace --all-features` 真实执行 **942 passed / 0 failed / 2 ignored**、83 个测试目标；10 条合同门禁要点行（schemas 118 valid/25 invalid、commands 12、errors 58、features 11、assets 17 schemas+156 fixtures、acp 25 methods+71 rows、docs 378 links+4121 refs、boundaries 12 crate、drift 36 DDL+15 trait/93 方法、agentic 16/16 items+17 宿主入口）原样留存。为排除 `&&` 掩盖前序失败，把 `check` 的 10 个 npm script 与 `check:agentic`/`check:rust` 展开成的命令共 **15 条逐个单独执行、各自记录 $?**（§WV4：C01..C09、C10、C10a、C10b、R01、R02、R03），**15/15 均 exit 0**，且每条命令前后各取一次 HEAD 全部相同（无修订漂移）；R03 独立复跑与聚合跑计数逐项一致（942/0/2、83 目标）。只读 `--list` 交叉核对（§WV7c）workspace declared **944 = 942 + 2 ignored**，无未执行用例。**如实登记**：R01 fmt 无输出；R02 clippy 两次都是 cargo 指纹缓存命中（1 s、无 Checking、exit 0 无诊断），故「本轮新跑了 clippy-driver」不成立，详见报告 issues 第 2 条。2 条 ignored（storage-sqlite 的 crash 子进程目标与 v1 夹具生成器）是仓库声明过的辅助项，不是被跳过的覆盖。四条命令在工作区内无副作用：全部跑完后 `git status --porcelain` 仍 0 行。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.11"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "8bc2ff3e078549cebed2ab99b5981d5ed37e0581"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一固定提交、同一环境下直接执行门禁脚本本体 `node scripts/check-crate-boundaries.mjs`（不是只跑 `npm run check:boundaries` 包装）：**退出码 0**、1 s，输出 `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）`（§WV5 raw 全文）。该脚本以 `MODULE_ARCHITECTURE.md` §5 的依赖矩阵为唯一判据、用 `cargo metadata` 校验每个 crate 的实际依赖，并硬约束 `core` 不引入 runtime/DB/HTTP/子进程/wire protocol 依赖；WP6 新增/触及的 `crates/server/src/node_link/command.rs` 与 `crates/core/src/broker.rs` 未改变任何 crate 的依赖方向（`server -> core + wire protocols`、`core` 无新依赖）。同一判定在 §WV4 的 C08 里独立复跑一次，同样 exit 0。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.11"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "8bc2ff3e078549cebed2ab99b5981d5ed37e0581"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一固定提交上执行派单原文命令 `cargo test --locked -p server -p core -p storage-sqlite -p identity-auth -p node-link-protocol --all-features`：**退出码 0**、38 s、**654 passed / 0 failed / 2 ignored**、39 个测试目标全部 ok（§WV1 raw 全文含逐条 `test … ok` 行；§WV2 为按 crate 归并与 declared vs executed）。**派单点名要求确认的 command 用例真实执行**：`node_link::command::tests` 声明 **25** == 执行 **25**，25 条用例名在 §WV2 逐条列出；`node_link::catalog::tests` 6/6、`node_link::resource::tests` 19/19、`local_admin::router::tests` 35/35、`node_link::*` 合计 106/106、`server` lib 276/276（§WV7c 的只读 `--list` 交叉核对：PV3 集 declared **656 = 654 + 2 ignored**，declared - ok - ignored = 0）。按 crate 汇总 654 = core 108 + identity-auth 83 + node-link-protocol 37 + server 304 + storage-sqlite 122；2 条 ignored（storage-sqlite 的 crash 子进程目标与 v1 夹具生成器）与 0 用例目标（`local_endpoint_unix.rs` 整文件 `#[cfg(unix)]`、identity_auth lib 与各 crate 无 doctest）已逐条交代，不是通过。相对 WP5-fix 轮次（625）**+29** = server lib +28（command 25 + resource 2 + router 1）+ core lib +1，逐项对上。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.11"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "8bc2ff3e078549cebed2ab99b5981d5ed37e0581"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "openspec/changes/node-link-owner/reports/pv5-windows-nodelink.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本机 Windows（MINGW64_NT / x86_64-pc-windows-msvc，cargo 1.98.1）上执行 `cargo test --locked -p server -p app --all-features`：**退出码 0**、81 s、**376 passed / 0 failed / 0 ignored**、13 个测试目标；只读 `--list` 声明 **376 == 执行 376**（§WP6-PV5-1…5，raw 全文含逐条 `… ok` 行）。WP6 主题在本机真实执行：`node_link::command::tests` 25/25、`node_link::resource::tests` 19/19、`node_link::catalog::tests` 6/6、`local_admin::router::tests` 35/35、`node_link::conn` 32/32（24 + handshake 5 + limits 3，WP4 冻结形状未被改坏）；相对 WP5-fix 轮次（348）**正好 +28**，全部落在 server lib（248 → 276），与新增用例逐一对上，证明新用例不是 Linux-only。平台差异真实留证：`tests/local_endpoint_windows.rs` 4/4 执行通过，`tests/local_endpoint_unix.rs` 0 条（整文件 `#[cfg(unix)]`，本机不参与编译、由 Linux CI 覆盖）；4 个 0 用例目标（app `main.rs`、`local_endpoint_unix.rs`、`Doc-tests app`、`Doc-tests server`）已逐条交代，不是静默跳过。命令跑完后 worktree 仍干净（`git status --porcelain` 0 行），无新增进程/端口/`/tmp` 的 `acpr-*` 残留（原有 1 项 WP4a 遗留的 `acpr-*` 已清理并记录）。"
    source_evidence: NOT_APPLICABLE
```
