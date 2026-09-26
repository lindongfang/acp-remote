# 最终替代验证轮次报告（`node-link-owner`；任务 7.1 · stage = final-main）

> 本报告只覆盖任务 7.1（最终替代验证 = Main E2E `not-applicable` 路径上的替代检查全套）。
> 本 Agent 是**只读执行者**：只运行检查并存证，未修改仓库任何被版本控制的文件（唯一写入 = 本文件与两个
> 追加日志，均在 `openspec/changes/node-link-owner/reports/` 下）。
> 本报告不做验收判定，也不声称 7.2（证据汇总）、7.3（e2e check）、8.1（最终验收）或归档已完成。

## Shared Report

| 字段 | 值 |
| --- | --- |
| `task_id` | 7.1 |
| `role` | coder（只读验证执行者；本轮不产生任何代码/文档改动） |
| `phase` | verify（最终替代验证：`not-applicable` 路径上的替代检查） |
| `stage` | final-main |
| `agent_context` | worker 子 Agent（coder 角色的验证轮次），工作目录 `D:\Project\acp-remote`（main 检出）。未继承实现对话，只按派单执行检查并追加日志/报告 |
| `target_revision` | `392efb791015b6a86a99ec9fd2ead45fe8c2881a`（= `refs/heads/main`，起点 FA-0-git 与终点 FA-3-Final 各核实一次，均相同；`origin/main` 仍为 `94a64e1f…`，即本地领先远端——本报告只以本地 `refs/heads/main` 为判据） |
| `scope` | ①[PV1] `npm run verify`（含 15 条可独立执行的子命令逐条退出码 + 1 次聚合入口）；②[PV3] `cargo test --locked -p server --all-features`；③[PV4] `cargo test --locked -p app --all-features`；④[PV5] `cargo test --locked -p server -p app --all-features`（本机 Windows 受控路径全链路）+ 约定形式 `--test node_link_e2e -- --test-threads=1`；⑤[PV2] 门禁脚本本体；⑥只读 `-- --list` 交叉核对；⑦临时资源清理核实 |
| `checks` | **全部 exit 0，无 FAIL/BLOCKED**：[PV1] 聚合 0 + 15/15 子命令 0；[PV2] 0；[PV3] 0（309/0/0）；[PV4] 0（85/0/0）；[PV5] 0（394/0/0，declared == executed）。逐项见本报告 §2–§5，原始输出见 `reports/du1-pv1.log` 与 `reports/pv5-windows-nodelink.log` 的「最终替代验证轮次（7.1）」分节 |
| `issues` | 无阻断项。3 条需主 Agent 知晓的事实见 §7：①`verification.md` 第 318 行把 6.7 的逐项记录指向 `reports/du1-integrate.md` 里一个并不存在的编号小节（引用悬空，但所需的**主分支判定面已由本轮 NEW 证据覆盖**）；②clippy 子项是 cargo 指纹缓存命中（已用只读 `-v` 轮次核实为 Fresh）；③`check:docs` 的扫描计数随规划工件变化（结论不变） |
| `result` | **PASS（final-main 替代检查）**：在最终主分支版本 `392efb79` 上，plan.md `alternative_checks` 四项（[PV1]/[PV3]/[PV4]/[PV5]）全部真实重跑并 exit 0，测试计数与基线逐目标一致、无未执行用例、无资源残留。**候选/主分支的替代检查 PASS ≠ 最终验收（8.1）PASS** |
| `evidence_paths` | `reports/du1-pv1.log`（「最终替代验证轮次（7.1）」分节，4007 行：FA-0…FA-6）、`reports/pv5-windows-nodelink.log`（同名分节，1054 行：FA-PV5-1…FA-PV5-4b）、本文件 |
| `resource_cleanup` | 本轮**无新增残留、也无可删除的 `acpr-*`**：跑完后 `acpr*`/`cargo`/`rustc`/测试进程 0、`:8765` 监听 0、`/tmp/acpr-*` 与 `%TEMP%\acpr-*` 计数均为 0（§6）。本 Agent 自建 scratch `/tmp/pv71` 为**仓库外**的原始输出副本，报告落盘后删除（§6）。未改 `Cargo.toml`/`Cargo.lock`/`crates/**`/`docs/**`/`schemas/**`/`compatibility/**`/`fixtures/**`，未动本轮开始前就存在的 2 行工作区改动 |

## 1. 运行前提与修订核实（FA-0）

| 核对项 | 命令 | 结果 |
| --- | --- | --- |
| 目标修订 | `git rev-parse HEAD` | `392efb791015b6a86a99ec9fd2ead45fe8c2881a` |
| 本地 main | `git rev-parse refs/heads/main` | 同上（两者一致） |
| 工作区 | `git status --porcelain` | 2 行：` M openspec/changes/node-link-owner/tasks.md`、` M openspec/changes/node-link-owner/verification.md`——**本轮开始前即存在**的规划工件改动，非本 Agent 引入；全程未变 |
| 暂存区 / stash | `git diff --cached --name-only` / `git stash list` | 0 行 / 0 条 |
| 与上一主分支检查轮的差异 | `git diff --stat 654c0c19 392efb79` | 只有两个文件：`tasks.md`（+2/−2）、`verification.md`（+39/−2），全在 `openspec/changes/node-link-owner/` |
| 零代码变化（复用依据本体） | `git diff --stat 654c0c19 392efb79 -- . ':(exclude)openspec/'` | **空**（无任何 crate/docs/schema/fixture/Cargo 变化）；`654c0c19` 是 `392efb79` 的祖先（`merge-base --is-ancestor` exit 0） |
| 工具链 | `node -v` / `npm -v` / `cargo -V` / `rustc -V` / `rust-toolchain.toml` | node v24.19.0 / npm 12.0.2 / cargo 1.98.1（797e8a9bc）/ rustc 1.98.1（48a229cea）/ channel = 1.98.1（与实测一致） |
| 运行时 | `uname -a` / `nproc` / `cargo metadata … target_directory` | MINGW64_NT-10.0-26200 / x86_64、16 逻辑核、`D:\Project\acp-remote\target` |
| 全部检查跑完后复核 | `git rev-parse HEAD` + `git status --porcelain \| wc -l` + `git rev-parse refs/heads/main` | `392efb79…` / 2（同前，未变）/ `392efb79…`——**无修订漂移** |

Main E2E 的 `not-applicable` 三要素（本轮只核对仍有效，不重新裁决）：`plan.md` 的 Main E2E 块含
`mode: not-applicable`、`reason`（`node-link-client` 与 `server::acp_facade` 未实现、不存在第二个真实节点，
最大可运行闭环是脚本化 fake Access ↔ 真实 Daemon 的集成测试）、`basis`（`openspec/config.yaml` 的 context
规则 + `x-agentic.e2e.command` 为空 + 切片 2/3/4 同款已归档处理）、`downgrade_approval`（2026-09-26 用户原话
「1B 2A 3批准」第 3 项）与 4 项 `alternative_checks`；`verification.md` 的 Main E2E 节记 `NOT_APPLICABLE`
并引用同一批替代检查。本轮把这 4 项替代检查在主分支版本上真实重跑（下节起）。

## 2. [PV1] `npm run verify`：逐子项独立执行 + 聚合入口

为了避免 `&&` 掩盖前序失败，`verify = check + check:rust` 被展开成**可独立执行的 15 条命令**
（`check` 的 10 个 npm script，其中 `check:agentic` 内串 2 条命令；`check:rust` 内串 3 条命令），
逐条单独执行并各自记录退出码；最后再跑一次聚合入口 `npm run verify`。原始输出：
`reports/du1-pv1.log` §FA-1（含每条命令的完整 stdout/stderr 与 `EXIT=` 行）。

| ID | 命令（cwd = `D:\Project\acp-remote`） | 退出码 | 关键输出 |
| --- | --- | --- | --- |
| C01 | `npm run check:schemas` | **0** | `118 valid, 25 invalid (ajv Draft 2020-12), 39 event views bound` |
| C02 | `npm run check:commands` | **0** | `command catalog OK: 12 commands` |
| C03 | `npm run check:errors` | **0** | `error registry OK: 58 codes across 2 protocols` |
| C04 | `npm run check:features` | **0** | `feature registry OK: 11 feature ids across 2 protocols` |
| C05 | `npm run check:assets` | **0** | `17 schemas, 156 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed` |
| C06 | `npm run check:acp` | **0** | `25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows)` |
| C07 | `npm run check:docs` | **0** | `doc links OK: 381 relative links, 5844 section refs across 333 markdown files`（扫到本轮更新过的规划工件） |
| C08 | `npm run check:boundaries` | **0** | `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致` |
| C09 | `npm run check:drift` | **0** | `§7 的 36 条 DDL` 与 `migrate.rs` 一致；`§5 的 15 个 trait / 93 个方法签名` 与 `ports.rs` 一致 |
| C10 | `npm run check:agentic` | **0** | `Totals: 17 passed, 0 failed (17 items)` + `宿主入口检查完成：17 个文件` |
| C10a | `node scripts/agentic-gate.mjs` | **0** | 同上 17 items（单独展开复跑） |
| C10b | `node scripts/sync-agentic-host-entrypoints.mjs --check` | **0** | `agentic 宿主入口检查完成：17 个文件` |
| R01 | `cargo fmt --all -- --check` | **0** | 无输出（无格式差异） |
| R02 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | **0** | `Finished dev profile … in 0.62s`、零诊断（指纹缓存命中，见 §7 第 2 条） |
| R03 | `cargo test --locked --workspace --all-features` | **0** | 85 个测试目标、**973 passed / 0 failed / 2 ignored**、99 s |
| **PV1** | `npm run verify`（聚合入口） | **0** | 同上十项门禁全绿 + fmt/clippy 0 + workspace **973 passed / 0 failed / 2 ignored**、108 s |

**子检查合计 15/15 exit 0**，与聚合退出码 0 一致（无「聚合绿、单项红」或反向矛盾）。2 条 `ignored` 与基线同数
（仓库声明过的 `storage-sqlite` 辅助项），不是被跳过的覆盖。R03 与聚合跑逐项计数一致（973/0/2、85 目标）。

## 3. [PV2] 边界门禁（同轮补充证据）

直接执行门禁脚本本体（不是 npm 包装）：`node scripts/check-crate-boundaries.mjs` → **exit 0**，
输出 `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）`。
同一判定在 §2 的 C08 里独立复跑一次，同样 exit 0。原始输出：`reports/du1-pv1.log` §FA-3。

## 4. [PV3] / [PV4]：本机重跑（本轮 NEW）与复用依据

**复用依据（写清）**：上一主分支检查轮为 `654c0c19`——它与候选候选提交是**同一个提交对象**（6.6 用
`merge --ff-only` 合入），而 `392efb79` 相对它的差异面只有两个 `openspec/` 规划工件（见 §1 的 diff 行），
即**零代码变化**、且 `openspec/` 不在 cargo 测试面，因此按「零代码变化」其 [PV3]/[PV4] 证据在原理上可复用。
**但本轮并不依赖复用**：两项都在 `392efb79` 上真实重跑（更保守），给出 NEW 证据。

| ID | 命令 | 退出码 | 计数 | declared(`-- --list`) | 日志 |
| --- | --- | --- | --- | --- | --- |
| [PV3] | `cargo test --locked -p server --all-features` | **0** | 7 目标 **309 passed / 0 failed / 0 ignored**（38 s） | 309 == 309 | `du1-pv1.log` §FA-2 |
| [PV4] | `cargo test --locked -p app --all-features` | **0** | 8 目标 **85 passed / 0 failed / 0 ignored**（67 s） | 85 == 85 | `du1-pv1.log` §FA-2 |

- [PV3] 覆盖面（与 plan.md 的 alternative_check 描述一致）：真实 listener 绑定/路由/Host 边界/TLS 失败关闭、
  配对 HTTP 全状态码与幂等、握手与信封/序号规则、catalog 过滤、attach/generation、snapshot/replay、
  event 持久化顺序与 raw 保真、ack 单调性、命令授权/幂等/终态/`session.create` 约束、撤销传播、心跳与慢连接、
  审计边界，以及 schema/fixture 漂移测试（消费同一 manifest）。server lib 281 + local_admin 系列 28
  （channel 14 / schema_drift 6 / naming 4 / unix 0 / windows 4）+ Doc-tests 0。
- [PV4] 覆盖面：配置 unwired 收敛、启动/关闭序列含网络 listener、`daemon.status.listen` 实际地址；
  app lib 52 + `audit_export` 1 + `cli_commands` 11 + `daemon_lifecycle` 11（真实起停 daemon 子进程）+
  `node_link_e2e` 2（受控路径全链路）+ `node_link_listener` 8（真实 `TcpListener`）+ Doc-tests 0。
- 已登记 flake（`verification.md` 的 EX8 项 `the_status_result_keeps_the_documented_field_set`）本轮在
  [PV4] 与 [PV5] 两轮均 **PASS**，未复现（原始 `... ok` 行见两个日志的对应分节）。

## 5. [PV5] 本机 Windows 受控路径全链路集成测试

| 命令 | 退出码 | 计数 | declared | 日志 |
| --- | --- | --- | --- | --- |
| `cargo test --locked -p server -p app --all-features` | **0** | 15 目标 **394 passed / 0 failed / 0 ignored**（83 s） | 394 == 394（`-- --list`，0 filtered out） | `pv5-windows-nodelink.log` §FA-PV5-1/2/4 |
| `cargo test --locked -p app --all-features --test node_link_e2e -- --test-threads=1`（约定形式） | **0** | **2 passed / 0 failed / 0 ignored**（5.30 s） | — | `pv5-windows-nodelink.log` §FA-PV5-3；`du1-pv1.log` §FA-3 |

- 受控路径全链路逐条通过：`the_controlled_path_runs_end_to_end_and_revocation_propagates`（配对 claim →
  本地确认 → 握手 → catalog.snapshot → attach → snapshot/event/ack → command（含 `session.create` 正常与
  各拒绝路径）→ 撤销传播）与 `tls_direct_terminates_the_same_handshake`（自签证书 TLS direct 同一握手）；
  约定形式的单线程定向重跑同样 2/2。
- 真实 listener / 进程轮次：`node_link_listener` 8/8（真实 bind、TLS 终止、失败关闭）、`daemon_lifecycle` 11/11
  （60.40 s 真实起停）、`local_endpoint_windows` 4/4。
- 0 用例目标逐条交代（**不是静默跳过**）：app `src\main.rs`（无 `#[test]`，交付面由 `cli_commands` 11 条覆盖）、
  `tests\local_endpoint_unix.rs`（整文件 `#![cfg(unix)]`，Windows 不参与编译，同一规则本机由
  `local_endpoint_windows` 4 条覆盖、Linux 由 CI 跑）、`Doc-tests app` / `Doc-tests server`（无可执行文档代码块）。
- 计数与上一轮（`5c869160`：394/0/0、15 目标）逐目标相同，属预期：本变更自那以后零代码变化，
  该命令把 core 当依赖编译；受控路径全链路仍 2/2。**边界**：本节只声称本机 Windows 轮次，
  跨实现（Linux/CI）轮次不在本轮范围。

## 6. 临时资源清理核实

跑完全部检查后只读核实（原始输出：`du1-pv1.log` §FA-4）：

- 进程：`tasklist` 匹配 `acpr|acp_remote|cargo.exe|rustc|node_link` = **0**。
- 端口：`netstat -ano` 中 `:8765` = **0**（用例一律 `127.0.0.1:0` 随机端口）。
- 临时目录：`ls -d /tmp/acpr-*` → `No such file or directory`（exit 2）、计数 **0**；`/tmp/acpr-*/TestCertificate`
  不存在；Windows 侧 `C:\Users\zhang\AppData\Local\Temp\acpr-*` 计数 **0**。
  → **本轮与早前轮次都没有可删除的 `acpr-*` 资源**（app 侧 `TempRoot`/证书目录的 Drop 清理在本机有效）。
- 早前轮次的**非 `acpr`** scratch（`/tmp/wp4*`、`/tmp/wp7*` 等 22 项）超出派单授权范围，**只登记不删除**；
  如需清空请另行授权。本 Agent 自建 scratch `/tmp/pv71`（仓库外的命令原始输出副本）在报告落盘后删除。
- Git 侧：工作区仍是本轮开始前的 2 行规划工件改动、暂存区 0 行、无 stash、无新增未跟踪文件（除本报告）。
- 本 Agent 自建 scratch `/tmp/pv71`（95 个条目，仓库外的命令原始输出副本）已两步式删除（逐文件 `rm -f` →
  逐层 `rmdir`），删除后 `ls -d /tmp/pv71` 报 `No such file or directory`；删除动作与终点状态记在
  `du1-pv1.log` §FA-6。
- 写报告后复跑（核实新写入的报告/日志自身不破坏门禁）：`npm run check`（十项合同门禁）→ **exit 0**；
  `node scripts/check-doc-links.mjs` → **exit 0**，`381 relative links, 5857 section refs across 334 markdown files`
  （对比写报告前的 C07：381 / 5844 / 333——文件 +1 与 refs +13 均来自本报告且被接受）。原始输出：`du1-pv1.log` §FA-5。

## 7. 需主 Agent 知晓的事实（observations，均非阻断）

1. **`verification.md` 对 6.7 记录的引用悬空**：该文件把「主分支检查（6.7 前半）」的逐项记录指向
   `reports/du1-integrate.md` 的一个编号小节；实测该报告在 `392efb79` 的**跟踪版本**（258 行）里只有 9 个编号小节、
   内容是 round 2（候选 `92fb9fdc`），既没有主分支记录也没有被引用的那个编号小节；规划根目录下的副本（391 行）
   含 round 3 记录（编号 0）与相同 9 个后续小节，同样没有该被引用的编号小节。同一行的「跟踪版本较新，含 round 3」
   也与跟踪版本内容不符（round 3 只存在于规划根副本）。**影响可控**：本轮已在主分支版本上给出
   [PV1]/[PV2]/[PV3]/[PV4]/[PV5] 的 NEW 证据，正覆盖 6.7 所需的判定面；但引用文本与「跟踪/副本哪个权威」
   应由主 Agent 在 7.2/8.1 前修正，避免最终验收按悬空路径取证据。本 Agent 为只读轮次，未改动这两个文件。
2. **clippy 子项是 cargo 指纹缓存命中（如实登记，不是跳过）**：R02 只花 0.62 s、输出仅 `Finished dev profile`、
   无 `Checking` 行，说明当前源码与上次 clippy 指纹一致。为让判定可核查，追加只读轮次
   `cargo clippy … -v -- -D warnings`：Fresh 行 **236**、`Checking` 行 **0**、`^warning` **0**、`^error` **0**
   （覆盖当前 workspace 全部本地 crate）。未做「换 target dir 强制重编」（成本高、且在仓库外新产生 GB 级目录，
   按既往口径不做）。
3. **`check:docs` 扫描计数随规划工件变化**：本轮 C07 为 381 相对链接 / 5844 section refs / 333 个 markdown 文件
   （round 3 的规划根对照运行为 379 / 5764 / 333）。差异来自 `openspec/` 规划工件的文本变化，
   属门禁的实际作用面，exit 0 不变。
4. **未执行（与既往口径一致，如实记录）**：`cargo-deny`（许可证/来源/advisory）与 `gitleaks`（密钥扫描）
   只在 CI 运行，本地无等价物、不在 `npm run verify` 里；因此「本地绿」不等于这两类判定通过，
   仍以 CI 的 `deps`/`advisories`/`secrets` 三个 job 为准。另外，本轮不改动代码，`origin/main`（`94a64e1f`）
   与本地 `refs/heads/main`（`392efb79`）不一致属于合入后的既定状态，本轮不做远端核实。

## 8. handoff_index

```yaml
handoff_index:
  - task_id: "7.1"
    role: coder
    phase: verify
    stage: final-main
    target_revision: "392efb791015b6a86a99ec9fd2ead45fe8c2881a"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在最终主分支版本 392efb79 上执行聚合入口 npm run verify：exit 0（108 s），十项合同门禁 + cargo fmt + cargo clippy + cargo test --locked --workspace --all-features（85 目标 973 passed / 0 failed / 2 ignored）。为排除 && 掩盖前序失败，把 verify 展开成 15 条可独立执行的命令逐条单独运行并各自记录退出码（C01..C09、C10、C10a、C10b、R01、R02、R03），15/15 exit 0，与聚合退出码一致；R03 独立复跑与聚合逐项计数一致。原始输出见 reports/du1-pv1.log 的「最终替代验证轮次（7.1）」§FA-1（每条命令含完整输出与 EXIT 行）；起点/终点 HEAD 均为 392efb79、git status 未变（§FA-0/§FA-3-Final）。check:docs 实际扫到本轮更新过的规划工件（381 相对链接 / 5844 section refs / 333 文件）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "7.1"
    role: coder
    phase: verify
    stage: final-main
    target_revision: "392efb791015b6a86a99ec9fd2ead45fe8c2881a"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 392efb79 上直接执行门禁脚本本体 node scripts/check-crate-boundaries.mjs：exit 0，输出 `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）`；同一判定在 §FA-1 的 C08（npm run check:boundaries）独立复跑一次，同样 exit 0。该脚本以 MODULE_ARCHITECTURE.md §5 矩阵为唯一判据、用 cargo metadata 校验实际依赖并硬约束 core 不引入 runtime/DB/HTTP/子进程/wire protocol 依赖；本轮完全未改代码（§FA-0-diff654 的空 diff），因此矩阵格与依赖方向不可能变化。原始输出见 reports/du1-pv1.log §FA-3。"
    source_evidence: NOT_APPLICABLE
  - task_id: "7.1"
    role: coder
    phase: verify
    stage: final-main
    target_revision: "392efb791015b6a86a99ec9fd2ead45fe8c2881a"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 392efb79 上执行 cargo test --locked -p server --all-features：exit 0（38 s）、7 目标 309 passed / 0 failed / 0 ignored；只读 `-- --list` 声明 declared_server=309，与执行数相等、无未执行用例。覆盖面与 plan.md alternative_checks 的 [PV3] 条目一致（listener/路由/Host 边界/TLS 失败关闭、配对 HTTP 全状态码与幂等、握手与信封/序号、catalog 过滤、attach/generation、snapshot/replay、event 顺序与 raw 保真、ack 单调性、命令授权/幂等/终态/session.create、撤销传播、心跳与慢连接、审计边界、schema/fixture 漂移）。复用依据（可选、本轮未依赖）：上一主分支检查轮 654c0c19 与本轮之间 `git diff --stat -- . ':(exclude)openspec/'` 为空（零代码变化）。原始输出见 reports/du1-pv1.log §FA-2（含逐目标 test result 行）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "7.1"
    role: coder
    phase: verify
    stage: final-main
    target_revision: "392efb791015b6a86a99ec9fd2ead45fe8c2881a"
    evidence_type: CHECK
    evidence_id: PV4
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 392efb79 上执行 cargo test --locked -p app --all-features：exit 0（67 s）、8 目标 85 passed / 0 failed / 0 ignored；只读 `-- --list` 声明 declared_app=85，相等、无未执行用例。含真实起停 daemon（daemon_lifecycle 11，60.40 s 级轮次）、受控路径全链路（node_link_e2e 2）与真实 TcpListener（node_link_listener 8）。已登记 flake EX8 的用例 the_status_result_keeps_the_documented_field_set 本轮 PASS。复用依据（可选、本轮未依赖）：654c0c19 → 392efb79 零代码变化（同 [PV3] 行）。原始输出见 reports/du1-pv1.log §FA-2。"
    source_evidence: NOT_APPLICABLE
  - task_id: "7.1"
    role: coder
    phase: verify
    stage: final-main
    target_revision: "392efb791015b6a86a99ec9fd2ead45fe8c2881a"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "openspec/changes/node-link-owner/reports/pv5-windows-nodelink.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本机 Windows 上执行 cargo test --locked -p server -p app --all-features：exit 0（83 s）、15 目标 394 passed / 0 failed / 0 ignored；只读 `-- --list` 声明 394 == 执行 394（85+309，0 filtered out），无未执行用例、无整目标静默跳过。受控路径全链路 node_link_e2e 2/2（the_controlled_path_runs_end_to_end_and_revocation_propagates、tls_direct_terminates_the_same_handshake），且约定形式 cargo test --locked -p app --all-features --test node_link_e2e -- --test-threads=1 亦 2/2（5.30 s，exit 0）；node_link_listener 8/8（真实 bind/TLS 终止/失败关闭）、daemon_lifecycle 11/11（真实起停）、local_endpoint_windows 4/4；0 用例目标（app main.rs、local_endpoint_unix、两个 Doc-tests）已逐条交代。计数与上一轮（5c869160：394/0/0、15 目标）逐目标相同且属预期（本轮零代码变化）。原始输出见 reports/pv5-windows-nodelink.log「最终替代验证轮次（7.1）」§FA-PV5-1..4b。"
    source_evidence: NOT_APPLICABLE
```

## 9. 未解决项与下一步（交给主 Agent）

- **本轮范围内无未解决检查**：4 项替代检查（+[PV2]）全部 exit 0、declared == executed、无资源残留、无修订漂移。
- **交给 7.2（证据汇总）**：把本报告的 5 条 `handoff_index` 行并入 `verification.md` 的 Checks 表与
  Coverage Index；同时处理 §7 第 1 条的悬空引用（修正指向或把 6.7 记录落到实际存在的分节）。
- **交给 7.3 / 8.1**：按扩展流程跑 `e2e check` 与最终验收；本报告只提供 final-main 的替代检查证据，
  不代替 `not-applicable` 三要素的有效性复核（本轮只做了「仍存在且与 plan.md 一致」的核对）。
- **风险/边界**：①本地不含 `cargo-deny` 与 `gitleaks`（只在 CI）；②本轮只覆盖本机 Windows，
  `#[cfg(unix)]` 条目与跨实现轮次未覆盖；③`origin/main` 落后于本地 main，本轮未做远端核实。
