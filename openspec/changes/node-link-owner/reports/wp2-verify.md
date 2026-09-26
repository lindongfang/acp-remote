# WP2 交付前 Project Verify 报告 — 任务 3.3（`node-link-owner`）

## Shared Report

- **task_id**: 3.3
- **role**: coder
- **phase**: implement
- **stage**: work-package
- **agent_context**: 任务级 coder 子 Agent（worker），不继承其他 WP 的实现对话，只接收本任务固定输入；工具受限（无 subagent/无委派），工作目录固定为变更分支 worktree `D:\Project\acp-remote-wt\node-link-owner`。**只执行检查、只写原始日志与本报告**；未修改任何代码、文档或规划文件（worktree 的 `git status --porcelain` 在轮次前后均为 0 行，见日志 §S9/§S15）。
- **target_revision**: `4e8a1075a48e004808f498447b54e45dbff824ce`（分支 `agentic/node-link-owner`，WP2 交付提交「feat(server): 落地 daemon.listen 的共享 HTTP/WSS 接入面」，`git diff --stat HEAD~1` = 16 个 `crates/server/src/...` 文件 / 4318 insertions）。
- **scope**: 任务 3.3 —— WP2 交付前 project verify：[PV3] `cargo test --locked -p server --all-features`、[PV1] `npm run verify`、[PV2] `node scripts/check-crate-boundaries.mjs`，逐项记录完整命令、工具链版本、退出码与日志路径，核对用例确实执行（无零用例、无全跳过、`transport::net` 真实执行），并核实资源已释放。**不做**独立 review（3.4）、不解释需求、不修复失败、不判定实现验收。
- **changes**: **无代码/文档/规划改动**。本轮是纯检查轮次：worktree 内 0 个被修改文件（日志 §S9 预检与 §S15 收尾两次 `git status --porcelain` 均为 0 行）；本报告与原始日志写在**仓库外的** `D:\Project\acp-remote\openspec\changes\node-link-owner\reports\`（该目录在 `acp-remote` 检出中为 untracked，`git ls-files` 无记录，见「日志位置」一节）。
- **checks**: [PV3] **PASS**、[PV1] **PASS**、[PV2] **PASS**（逐项退出码见下表与日志）。
- **issues**: 无阻断项、无仓库失败项。4 条**我自己留证工具的自查更正**（不影响三条权威命令的结论，已就地保留原始输出以便复核，详见「issues」一节的第 1 条）；1 条范围外提示（`cargo-deny`/`gitleaks` 仍只在 CI 判定）。
- **result**: **PASS**（W1/WP2 轮次；仅代表本任务的三条检查通过，不代表 3.4 独立 review、E2E 或合并已完成）。
- **evidence_paths**:
  - 原始日志（本角色，**追加**在既有 W0/WP1 内容之后的「W1/WP2 轮次」分节）：`openspec/changes/node-link-owner/reports/du1-pv1.log`
    - W0/WP1（任务 3.1，原样保留未改动）：§S1–§S8
    - W1/WP2（本任务）：§S9 预检 / §S10 [PV3] 完整原始输出 / §S11 与 §S11b 用例执行分析（§S11b 为更正后的口径）/ §S12 [PV1] `npm run verify` 完整原始输出 / §S13 逐子检查独立执行与各自退出码 / §S13b 一致性复核 / §S13c 全 workspace 测试的更正口径聚合 / §S14 [PV2] 完整原始输出 / §S15、§S15b、§S15c 资源与工作树收尾 / §S16 行数口径脚注
    - 文件规模：2135 行 / 113,746 字节 → **5962 行 / 336,247 字节**（本次追加 3827 行）
  - 本报告：`openspec/changes/node-link-owner/reports/wp2-verify.md`
  - 实现轮次（2.6）的 [PV3] 记录：`openspec/changes/node-link-owner/reports/wp2-transport-net.log`（本报告是其**独立复跑**，二者互为佐证）
- **resource_cleanup**: 无端口/数据库/容器/账号被创建或留存；只有本地 `npm`/`cargo`/`node` 调用。测试子进程零残留（`acpr-fake-acp-agent.exe` / `cargo.exe` / `rustc.exe` / `acp-remote.exe` / `link.exe` / `server.exe` 全部 0 个，日志 §S15）。临时目录零残留：MSYS `TEMP`/`TMP` 均为 `/tmp`，也正是 net 用例写自签证书的目录，轮次结束后 `ls -la $TEMP | grep -ci acpr` = **0**、`/tmp` 下 `acpr*` 条目 = **0**（TLS 证书由 `TestCertificate::Drop` 删除）。默认监听端口 8765 上 0 个监听者（用例只绑 `127.0.0.1:0`）。**我自己的 3 个 scratch 数据目录已删除**；留在 MSYS `/tmp`（仓库与 worktree 之外）的只有 8 个只读日志包装脚本 `wp2-stage{AB,B2,B3,C,D,E,F,G}.sh`（`/tmp` 下另有 93 个与本任务无关的既有 `.log` 文件，非本轮产生）。worktree 自有构建产物 `target/` 9.7G、`node_modules/` 71M 为 git-ignored 缓存，按任务约定无需清理。

### 环境（实测）

| 项 | 值 |
|---|---|
| 主机 / 外壳 | Windows（本机），git-bash，cwd `D:\Project\acp-remote-wt\node-link-owner` |
| node / npm | `v24.19.0` / `12.0.2`（≥ 合同门禁要求的 22.12） |
| rustc / cargo | `rustc 1.98.1 (48a229cea 2026-09-01)` / `cargo 1.98.1 (797e8a9bc 2026-08-05)`，host `x86_64-pc-windows-msvc`，与 `rust-toolchain.toml` pin 一致 |
| `CARGO_TARGET_DIR` | 未设置；worktree 内无 `.cargo/config.toml`，因此 cargo 使用 **worktree 自有的** `target/`——它与主检出 `D:\Project\acp-remote` 及其他 worktree 物理隔离，满足 plan PV3 行「独立 `CARGO_TARGET_DIR`」的意图（日志 §S9 已记录该判断依据） |
| 轮次时间 | 2026-09-26 10:09:48 → 10:15:14 +0800 |

### 检查结果（逐项退出码）

| ID | 命令（cwd = worktree 根） | 退出码 | 关键结果 | 日志位置 |
|---|---|---|---|---|
| **PV3** | `cargo test --locked -p server --all-features` | **0**（3s） | **189 passed / 0 failed / 0 ignored**；7 个测试目标 + 0 个用例的 doc-test 套件；`transport::net` **72/72 真实执行**；`--list` 声明 189 = 执行 189 | §S10 / §S11b |
| **PV1** | `npm run verify` | **0**（100s） | 合同门禁 + `fmt`/`clippy`/全 workspace 测试全绿；全 workspace 测试 **790 passed / 0 failed / 2 ignored** | §S12 |
| PV1-a1 | `npm run check:schemas` | 0（1s） | `schema fixtures OK: 118 valid, 24 invalid (ajv Draft 2020-12), 39 event views bound` | §S13 C01 |
| PV1-a2 | `npm run check:commands` | 0（0s） | `command catalog OK: 12 commands` | §S13 C02 |
| PV1-a3 | `npm run check:errors` | 0（1s） | `error registry OK: 58 codes across 2 protocols` | §S13 C03 |
| PV1-a4 | `npm run check:features` | 0（0s） | `feature registry OK: 11 feature ids across 2 protocols` | §S13 C04 |
| PV1-a5 | `npm run check:assets` | 0（1s） | `contract assets OK: 17 schemas, 155 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed` | §S13 C05 |
| PV1-a6 | `npm run check:acp` | 0（1s） | `ACP compatibility matrix OK: 25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows, schema validated by ajv)` | §S13 C06 |
| PV1-a7 | `npm run check:docs` | 0（0s） | `doc links OK: 378 relative links, 4066 section refs across 257 markdown files` | §S13 C07 |
| PV1-a8 | `npm run check:boundaries` | 0（1s） | `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致` | §S13 C08 |
| PV1-a9 | `npm run check:drift` | 0（0s） | `contract drift OK: §7 的 36 条 DDL 与 crates\storage-sqlite\src\migrate.rs 逐条一致；§5 的 15 个 trait / 87 个方法签名与 crates\core\src\ports.rs 一致` | §S13 C09 |
| PV1-a10 | `node scripts/agentic-gate.mjs` | 0（1s） | `Installation: PASS`；`Totals: 16 passed, 0 failed (16 items)` | §S13 C10a |
| PV1-a11 | `node scripts/sync-agentic-host-entrypoints.mjs --check` | 0（1s） | `agentic 宿主入口检查完成：17 个文件`（无差异） | §S13 C10b |
| PV1-b1 | `cargo fmt --all -- --check` | 0（1s） | 无输出（无格式差异） | §S13 R01 |
| PV1-b2 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | 0（0s） | `Finished dev profile … in 0.42s`，无诊断（本次为指纹缓存命中：输入与已产出的 clippy 工件一致；同一提交在 2.6 实现轮次已用同一条命令对 `-p server` 全量分析通过） | §S13 R02 |
| PV1-b3 | `cargo test --locked --workspace --all-features` | 0（82s） | **790 passed / 0 failed / 2 ignored**；82 条 `test result` 行（= 82 个顶层测试目标 / doc-test 套件），其中 65 条用例数 >0、17 条为 0 用例，非通过行 0 条；另见 §S16 对 83 条 `running …` 行（多出的一条是 `commit.rs::crash_child` 派生的嵌套子进程）的说明 | §S13 R03 / §S13c / §S16 |
| **PV2** | `node scripts/check-crate-boundaries.mjs` | **0** | `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）`（含 WP1 新增的 `server`→`acpr-wire` 格） | §S14 |

14 个子检查**逐个独立执行**并各自记录退出码（脚本没有任何 `&&` 串联），全部为 0；与 §S12 中按 `package.json` 串联的 `npm run verify` 退出码 0 一致，因此不存在「前序失败被 `&&` 掩盖」的情形（§S13 末尾的逐项清单即原始证据）。

### 用例确实执行（无零用例、无全跳过）

**PV3（`-p server`）**：`--list` 声明 189 个用例，执行输出 189 条 `test … ok`，逐字一致；`transport::net` 声明 72、执行 72（`cargo test -p server --all-features -- --list | grep ': test$' | grep -c '^transport::net'` = 72，执行输出里 `test transport::net…` 的 `... ok` 行同样 = 72）。**不是全跳过**：0 条 `... ignored`、0 条非 `ok` 用例行。
- 7 条 `test result`：`lib` 161、`local_admin_channel` 14、`local_admin_schema_drift` 6、`local_endpoint_naming` 4、`local_endpoint_windows` 4、`local_endpoint_unix` 0、`Doc-tests server` 0。
- 唯一的两个 0 用例套件已逐条交代：`tests/local_endpoint_unix.rs` 整文件是 `#[cfg(unix)]`，本机 host 为 `x86_64-pc-windows-msvc`（Linux CI 会跑到；本平台的对应实现 `tests/local_endpoint_windows.rs` 本次 4/4 执行）；`Doc-tests server` 该提交下没有 doctest。二者都不是「套件被跳过」。

**PV1（全 workspace）**：790 passed / 0 failed / 2 ignored，82 条 `test result` 行、65 套件用例数 >0、17 套件 0 用例（6 个无测试的 lib/bin 目标 + 11 个空 doc-test 套件），非通过行 0 条；`transport::net` 在同一轮里同样执行 72 个用例。计数口径说明（日志 §S16）：该轮共 83 条 `running …` 行而只有 82 条 `test result` 行，多出的一条是声明为 `ignored` 的辅助项 `storage-sqlite tests\commit.rs::crash_child` 被 `crash_recovery_leaves_an_consistent_database` 作为子进程再次拉起时打印的（子进程自己的 result 行不进入父进程的输出流，父进程的 result 行报 15 passed + 1 ignored = 该目标声明的 16 个用例），没有任何用例去向不明。
2 个 `ignored` 是仓库**声明过的**辅助项，不是被跳过的覆盖（日志 §S13c 保留了原始行）：
`storage-sqlite tests\commit.rs::crash_child`（注明「由 `crash_recovery_leaves_an_consistent_database` 拉起」）与
`storage-sqlite tests\migration.rs::regenerate_v2_fixtures`（注明「夹具生成器：只在需要重建 fixtures/storage/v2 时手动运行」）。

**与基线的交叉核对**：任务 3.1 在 `028a4d2` 记录的全 workspace 用例数为 718 passed / 0 failed / 2 ignored（`reports/du1-pv1.log` §S2/§S6，测试标的一览同为 82 条 result 行、65 套件 >0、17 套件 0）。本任务在 `4e8a107` 得到 **790 = 718 + 72**，增量恰为 WP2 新增的 72 个 `transport::net` 用例，且 0 用例套件、`ignored` 项、result 行数完全不变——说明 WP2 只新增用例、没有让任何既有用例消失或被跳过。

### 日志位置（可复核性）

原始日志写在本任务指定的 `D:\Project\acp-remote\openspec\changes\node-link-owner\reports\du1-pv1.log`，采用**追加**方式：既有 W0/WP1（任务 3.1）的 §S1–§S8 逐字节保留（文件头前 8 行与 §S8 内容未变），本任务从 §S9 起续写。检查对象（worktree）与留证位置（`acp-remote` 检出）物理分开，因此本任务**不可能**通过写日志影响被检查的仓库内容——这也是 worktree 在轮次前后都保持 `git status --porcelain` 为空的原因之一。

## issues（如实记录）

1. **我自己的留证工具缺陷（3 处，全部就地保留原始输出并另起分节更正；对三条权威命令的结论无影响，非仓库缺陷）**：
   - §S11 的聚合 awk 用了 `/## S10\./,0`（到文件末尾），把 §S11 自己刚回显的 `test result` 行也计了进去 → 打出 `passed=378`、`result-lines=14` 的错误聚合。§S11b 用 `sed -n '/## S10\./,/## S11\./p'` 把范围收敛后重算，得到 **189 passed / 0 failed / 0 ignored / result-lines=7**，与 §S10 原始输出的手工核对一致。
   - §S13 结尾用「14 个变量的拼接 == 13 个零的字面量」做一致性判断（少写一个 0），因此打印了 `INCONSISTENT`。§S13b 更正为：C01–C09、C10a、C10b、R01、R02、R03 共 **14 个子检查全部 exit=0**，与 §S12 的 `npm run verify` exit=0 一致。
   - §S13b 提取全 workspace 测试输出时，`sed` 范围从**整个文件**里第一处匹配 `$ cargo test --locked --workspace --all-features`（W0/WP1 的 §S2 里同一个命令的回显行）开始，吞了 3779 行，导致其打印的 R03 数字（216 个 `transport::net` 匹配、167 个测试二进制、0 用例套件清单）错误。§S13c 先把范围收敛到 §S13 再提取，得到正确的 **82 result 行 / 790 passed / 0 failed / 2 ignored / 65 套件 >0 / 17 套件 0 / 78 个测试二进制 / `transport::net` 72**。
   - 另有 4 处措辞/工具口径更正（§S15b、§S15c、§S16）：`ls -dsh` 在本机 MSYS 上报的是目录项大小（显示 `0`/`16K`）而非递归大小，已改用 `du -sh`（`target` 9.7G、`node_modules` 71M）；「哪些二进制跑了 0 个用例」的 grep 因横幅行不与结果行相邻而无输出，已改用跟踪横幅的 awk 重提；§S15 的结尾行因变量未跨脚本传递而打印了空的 `PV3= PV1=`，真实值为 **PV3=0 / PV1=0 / PV2=0**（§S15c 末尾重申）；§S13c 的 `grep -c '^running [0-9]* tests'` = 78 只计复数形式，含单数形式实为 83 条 `running …` 行 vs 82 条 `test result` 行，多出的一条是 `commit.rs::crash_child` 派生的嵌套子进程（§S16 给出口径与原始片段）。
2. **范围外**：`cargo-deny`（许可证/来源/advisory）与 `gitleaks`（密钥扫描）在本机不可执行且不属于 `npm run verify`，仍只由 CI 的 `deps`/`advisories`/`secrets` job 判定；本次**未**声称它们通过。
3. **范围外**：`check:docs` 的 section refs 从 W0/WP1 的 4063 变为本次的 4066，是两者修订号不同的结果（`028a4d2` → `af9064e` 是 `docs(deps)` 提交，随后才是 `4e8a107`），与本任务无关；门禁本身 exit 0。
4. **范围外**：`target/` 因增量编译增至 9.7G；属 worktree 自有 git-ignored 产物，任务明确无需清理。

## handoff_index

```yaml
handoff_index:
  - task_id: "3.3"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "4e8a1075a48e004808f498447b54e45dbff824ce"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在该固定提交（分支 agentic/node-link-owner，轮次前后 `git status --porcelain` 均为 0 行）上用本机固定工具链（rust-toolchain.toml=1.98.1 实测一致；node v24.19.0 / npm 12.0.2；Windows x86_64-pc-windows-msvc）执行 `npm run verify`：聚合退出码 0（§S12），且 14 个子检查随后逐个独立执行、各自 exit=0（§S13），排除了 `&&` 掩盖前序失败的可能；其中 `cargo test --locked --workspace --all-features` 真实执行 790 passed / 0 failed / 2 ignored（82 result 行，§S13c）。与任务 3.1 在 028a4d2 的 718 基线相比增量为 +72（恰为 WP2 新增的 transport::net 用例），说明本轮是全新执行而非复用；WP1 的 PV1 证据在另一修订号上，按 plan 的 Dependency Handoffs 不在本 WP 复用范围内。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.3"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "4e8a1075a48e004808f498447b54e45dbff824ce"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一固定提交、同一环境上直接执行 `node scripts/check-crate-boundaries.mjs`：exit=0，输出 `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）`（含 WP1 新增的 `server`→`acpr-wire` 格），完整原始输出见 §S14（同时作为 §S13 C08 的独立复跑）。WP1 的 PV2 证据在 028a4d2，本 WP 按 plan 要求在自己的交付提交上重新取得。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.3"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "4e8a1075a48e004808f498447b54e45dbff824ce"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本任务（3.3）的独立轮次：`cargo test --locked -p server --all-features` 在同一固定提交上 exit=0，189 passed / 0 failed / 0 ignored，7 条 test result 行（6 个测试目标 + `Doc-tests server`）；用例确实执行且无全跳过——`--list` 声明 189 与执行 189 逐字一致，`transport::net` 声明 72 与执行 72 一致，0 条 ignored、0 条非 ok 用例行；两个 0 用例套件（`local_endpoint_unix` 为 `#[cfg(unix)]`、`Doc-tests server` 无 doctest）已逐条交代（§S10/§S11b）。执行环境满足 plan PV3 行的「独立 CARGO_TARGET_DIR」意图：worktree 内无 `.cargo/config.toml` 且 `CARGO_TARGET_DIR` 未设置，cargo 使用 worktree 自有 `target/`，与主检出及其他 worktree 物理隔离（§S9）。实现轮次（2.6 / wp2-transport-net.log）另有一条 PV3 记录，本行是**独立复跑**，二者互为佐证，非复用。"
    source_evidence: NOT_APPLICABLE
```
