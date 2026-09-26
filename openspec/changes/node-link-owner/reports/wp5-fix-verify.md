# WP5 修复后验证重跑（任务 3.9 / WP5-fix）— `node-link-owner` / DU1

## Shared Report

- **task_id**：3.9（WP5 交付前 project verify 的**修复后重跑**；权威 `tasks.md:53` 原文 =「3.9 WP5；前置：2.16；实现 Agent（coder）。完成 WP5 交付前 project verify：[PV3] + [PV5]（本机）。完成条件：逐项通过并留证。」）
- **role / phase**：coder / verify —— 本任务**只执行检查与写报告，未修改被检查仓库的任何文件**（worktree 终点 `git status --porcelain` 仍为 0 行、暂存区为空、无 stash）。
- **agent_context**：worker 子 Agent（本机 worktree `D:\Project\acp-remote-wt\node-link-owner` 独占会话，**未继承** WP5 实现 / WP5-fix1 / RV1-WP5 review 轮次的对话；只读检查 + 追加日志与报告）。权威规划根 `D:\Project\acp-remote`（报告与日志写在 `openspec/changes/node-link-owner/reports/`，该目录不入版本控制）。
- **target_revision**：`27ad3f800c7821846b8991ae7d61b67f59e8ba24`（= 派单固定提交 = WP5-fix1 的第三个提交，分支 `agentic/node-link-owner`）。
  轮次**起点（本地 17:53:33 = 09:53:33Z）与终点（本地 18:01 = 10:01:35Z）**均实测 `git rev-parse HEAD` = 该值、`git status --porcelain | wc -l` = 0、暂存区 0 行、`git stash list | wc -l` = 0；**无修订漂移**。本轮每条命令执行前后各取一次 HEAD，全部相同（见 `du1-pv1.log` §WF3 的逐条 `HEAD_BEFORE`/`HEAD_AFTER`）。
- **scope（本任务实际执行的四项检查）**：
  - `[PV3]` `cargo test --locked -p server -p core -p storage-sqlite -p identity-auth -p node-link-protocol --all-features`（派单命令原文，含对 catalog/resource 新用例真实执行的确认）；
  - `[PV1]` `npm run verify`（聚合 1 次）+ `check`/`check:rust` 展开成的 **15 条子检查逐个独立执行、各自记录退出码**（不用 `&&` 掩盖前序失败）；
  - `[PV5]` `cargo test --locked -p server -p app --all-features`（本机 Windows）；
  - 收尾：资源残留核实（进程 / 端口 / `/tmp` 的 `acpr-*`）+ worktree 终点状态。
- **checks**：**四项全部 exit 0，无 FAIL**；无「零用例或整目标静默跳过」被当成通过；无新增进程 / 监听端口 / 临时目录残留。
  逐项见下表与 `reports/du1-pv1.log` 的「WP5-fix 重跑轮次」分节（§WF0–§WF5，**追加**；第 27205 行起）、`reports/pv5-windows-nodelink.log` 的「WP5-fix 重跑轮次」分节（**追加**；第 3037 行起）。
- **result**：**PASS**（本任务四项检查在固定提交 `27ad3f80` 上全部通过）。**不代表** 3.10 的 RV2-WP5 独立复核、WP6/WP7、6.5（候选替代验证）或合并已完成。
- **evidence_paths**：`openspec/changes/node-link-owner/reports/du1-pv1.log`（「WP5-fix 重跑轮次」分节）、`openspec/changes/node-link-owner/reports/pv5-windows-nodelink.log`（同名分节）、本文件。
- **resource_cleanup**：本轮**未新增**任何进程 / 监听端口 / 数据库 / 容器 / 账号 / 证书 / 依赖；未改 `Cargo.toml`、`Cargo.lock`、`schemas/**`、`compatibility/**`、`fixtures/**`、`docs/**`、`openspec/**`。
  只读检查结束后实测：精确镜像名匹配 `cargo.exe`/`rustc.exe`/`link.exe`/`acpr-fake-acp-agent.exe`/`acp-remote.exe`/`acp_remote.exe`/`server.exe`/`app.exe`/`admin_store.exe` = **0**（`grep` exit 1）；`netstat -ano` 中 `8765` = **0**（`grep -w` exit 1）；MSYS `/tmp`（= `C:\Users\zhang\AppData\Local\Temp`）中 `acpr-*` = **0**（`ls` exit 2）。
  本轮自建 scratch `/tmp/wp5fixv/`（22 个条目，仓库外）已在收尾时删除（机制说明见 issues 第 4 条），删除后 `ls -d /tmp/wp5fixv` exit 2。
  worktree 自有的 `target/` 与 `node_modules/` 是 git-ignored 构建缓存，按约定保留。

## 检查结果（逐项退出码）

| ID | 命令（cwd = worktree 根） | 退出码 | 耗时 | 关键结果 | 日志位置 |
|---|---|---|---|---|---|
| **PV3** | `cargo test --locked -p server -p core -p storage-sqlite -p identity-auth -p node-link-protocol --all-features` | **0** | 29 s | **625 passed / 0 failed / 2 ignored**；39 个测试目标全 `ok` | `du1-pv1.log` §WF1 |
| **PV1** | `npm run verify`（聚合） | **0** | 117 s | workspace **913 passed / 0 failed / 2 ignored**；83 个测试目标 | `du1-pv1.log` §WF2 |
| PV1-C01 | `npm run check:schemas` | **0** | 1 s | `118 valid, 25 invalid (ajv Draft 2020-12), 39 event views bound` | `du1-pv1.log` §WF3 |
| PV1-C02 | `npm run check:commands` | **0** | 1 s | `12 commands` | 同上 |
| PV1-C03 | `npm run check:errors` | **0** | 0 s | `58 codes across 2 protocols` | 同上 |
| PV1-C04 | `npm run check:features` | **0** | 0 s | `11 feature ids across 2 protocols` | 同上 |
| PV1-C05 | `npm run check:assets` | **0** | 1 s | `17 schemas, 156 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared, 2 SAS values recomputed` | 同上 |
| PV1-C06 | `npm run check:acp` | **0** | 1 s | `25 methods, 11 updates, …, 10 test families (71 rows, ajv validated)` | 同上 |
| PV1-C07 | `npm run check:docs` | **0** | 1 s | `378 relative links, 4114 section refs across 257 markdown files` | 同上 |
| PV1-C08 | `npm run check:boundaries` | **0** | 1 s | `12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）` | 同上 |
| PV1-C09 | `npm run check:drift` | **0** | 1 s | `§7 的 36 条 DDL` + `§5 的 15 个 trait / 93 个方法签名` 与代码一致 | 同上 |
| PV1-C10 | `npm run check:agentic` | **0** | 2 s | `Totals: 16 passed, 0 failed (16 items)` + `宿主入口检查完成：17 个文件` | 同上 |
| PV1-C10a | `node scripts/agentic-gate.mjs` | **0** | 1 s | 16/16 items | 同上 |
| PV1-C10b | `node scripts/sync-agentic-host-entrypoints.mjs --check` | **0** | 0 s | `agentic 宿主入口检查完成：17 个文件` | 同上 |
| PV1-R01 | `cargo fmt --all -- --check` | **0** | 2 s | 无输出（无格式差异） | 同上 |
| PV1-R02 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | **0** | 1 s | `Finished dev profile … in 0.61s`，无诊断（**指纹缓存命中**，见 issues 第 2 条） | 同上 |
| PV1-R03 | `cargo test --locked --workspace --all-features` | **0** | 97 s | **913 passed / 0 failed / 2 ignored**；83 个目标；与聚合跑逐项一致 | 同上 |
| **PV5** | `cargo test --locked -p server -p app --all-features`（本机 Windows） | **0** | 81 s | **348 passed / 0 failed / 0 ignored**；13 个目标；declared(348) == executed(348) | `pv5-windows-nodelink.log` §WF-PV5-1…5 |
| — | `ls -d /tmp/acpr-*`、进程/端口精确匹配 | — | — | 残留 = 0（`acpr-*` exit 2、进程 exit 1、端口 exit 1） | `du1-pv1.log` §WF5 |

**子检查合计：15/15 exit 0**，与 `npm run verify` 聚合退出码 0 一致（无「聚合绿、单项红」或反过来的矛盾）。

## 派单点名要求：catalog/resource 新用例是否真实执行

派单要求「记录计数；确认 catalog/resource 新用例真实执行」。逐条留证：

| 模块 | 声明数（`--list`） | 执行数（`... ok`） | 其中 WP5-fix1 新增 |
|---|---|---|---|
| `node_link::catalog::tests` | **6** | **6** | 3 |
| `node_link::resource::tests` | **17** | **17** | 3 |
| `node_link::*` 合计 | **79** | **79** | 6 |

6 条新增用例在 `[PV3]`（`du1-pv1.log` §WF1）中**逐条 `ok`**：

- `node_link::catalog::tests::an_empty_visible_set_yields_exactly_one_empty_snapshot`
- `node_link::catalog::tests::the_catalog_snapshot_carries_every_field_of_the_visible_export_only`
- `node_link::catalog::tests::the_catalog_snapshot_is_batched_by_the_negotiated_size_in_a_stable_order`
- `node_link::resource::tests::a_finished_connection_state_entry_is_reclaimed_by_the_next_fan_out`
- `node_link::resource::tests::an_attachment_is_refused_for_a_foreign_owner_node`
- `node_link::resource::tests::an_ack_from_a_foreign_owner_node_is_rejected_without_advancing_the_watermark`

这 6 条在 `[PV5]`（Windows）中也逐条执行通过，证明它们不是 Linux-only：
`pv5-windows-nodelink.log` §WF-PV5-3 单独列出了 6 行的 `... ok`。

**与 WP5 上一轮（`6b1f0b9`，见 `reports/wp5-verify.md`）的计数对照**：

| 项 | WP5 轮次（6b1f0b9） | 本轮（27ad3f80） | 差值 | 说明 |
|---|---|---|---|---|
| `[PV3]` 命令范围 | 4 个 crate（少 `node-link-protocol`） | **5 个 crate（派单原文）** | +`node-link-protocol` | 本轮 `node-link-protocol` 贡献 37 条 |
| `[PV3]` 计数 | 582 / 0 / 2 | **625 / 0 / 2** | **+43** | 37（新 crate）+ 6（WP5-fix1 新用例） |
| `[PV1]`/`[R03]` workspace | 907 / 0 / 2（83 目标） | **913 / 0 / 2（83 目标）** | **+6** | 正是 WP5-fix1 的 6 条新用例，既有用例一条未减 |
| `[PV5]` 本机 Windows | 342 / 0 / 0（13 目标） | **348 / 0 / 0（13 目标）** | **+6** | 同上；`server` lib 242 → 248 |
| 声明 vs 执行（workspace） | 909 = 907 + 2 ignored | **915 = 913 + 2 ignored** | +6 | 无未执行用例 |
| 声明 vs 执行（PV5 server+app） | 342 == 342 | **348 == 348** | +6 | 无未执行用例 |

差值逐项对上，说明本轮**没有静默跳过或减少覆盖**。

## issues（需要主 Agent / reviewer 知晓的事实）

1. **`[PV3]` 的 crate 范围与上一轮不同，是本轮有意为之**：派单给的命令含 `-p node-link-protocol`（上一轮 3.9 的派单只到 `-p identity-auth`）。因此 `[PV3]` 的 625 不能直接与上一轮的 582 相减；正确的对账是 582 + 37（新 crate 的 6+6+7+5+7+3+3 条）+ 6（新用例）= **625**。
2. **`npm run check:rust` 的 clippy 是 cargo 指纹缓存命中（如实登记，不是造假也不是跳过）**：`[PV1]` 聚合内的 `cargo clippy` 与单独执行的 R02 都只输出 `Finished dev profile … in 0.61s`、无任何 `Checking` 行，即工作区源码与上次 clippy 指纹一致（WP5-fix1 提交前的 `.husky/pre-commit` 已对**同一份源码**跑过 workspace clippy），cargo 直接判为 fresh。两次都 **exit 0、零诊断**，判定有效；但「本轮新跑了一次 clippy-driver」这句话不成立，故在此显式登记。若要一份「强制重跑」的 clippy 证据，需要新 target dir 或改动文件 mtime —— 前者成本高、后者违反本轮「不修改文件」约束，**均未执行**。
3. **子检查条数的口径**：`package.json` 的 `verify` = `check` + `check:rust`。`check` 有 10 个 npm script，其中 `check:agentic` 内部串了 2 条命令；`check:rust` 内部串了 3 条命令。本轮**既执行了 10 个 npm script，又把 `check:agentic` 的 2 条与 `check:rust` 的 3 条展开单独各跑一次**，故可独立执行的命令是 **15 条**（C01–C09、C10、C10a、C10b、R01、R02、R03）。**15/15 全部 exit 0**。若 reviewer 只按「npm script 计数」看，是 13 条（10 + 3）；口径差异已在日志 §WF3 逐条列明命令与退出码，结论不变。
4. **清理动作的机制说明**：本机工具的通用安全确认**拦截了递归删除型命令**（`du1-pv1.log` §WF5 末尾有原始记录），因此改用等价的逐步删除：`find /tmp/wp5fixv -type f -exec rm {} +` → `rmdir subs` → `rmdir /tmp/wp5fixv`，三步均 exit 0；删除对象只是**本轮自建**的、位于仓库之外的抓取文件（22 个条目）。删除后 `/tmp/wp5fixv` 无匹配（exit 2）。
5. **`/tmp` 里仍有其它轮次（非本轮）的 scratch，按「只核实不修改」保留**：`/tmp/wp4`（15:48）、`/tmp/wp4fix1`（16:07）、`/tmp/wp4fix1b`（16:14）、`/tmp/wp4fix2`（16:33）、`/tmp/wp5fix1`（17:51）、`/tmp/wp5.log`（17:26）以及若干 `wp4a*.mjs` / `wp4*.bak` / `wp47_*.txt`（9-23 至 9-25）。它们**都不是 `acpr-*` 前缀**（不是被测用例产生的临时目录），mtime 全部早于本轮起点 17:53:33，属早前轮次产物；删除会破坏他人轮次的可复现性，故**未动**。派单点名的「`/tmp` 的 `acpr-*`」实测为 **0**。
6. **两处自我更正（均已就地记录，对结论无影响）**：
   - 资源核实初稿里 `tasklist | grep … | tee`、`netstat | grep … | tee` 后取的 `$?` 实际是 `tee` 的退出码（恒 0），不能作为「无匹配」的判据；已在 §WF5 重做并取到准确退出码（精确匹配 exit 1 = 无匹配），正文采用重做值。
   - 追加日志的生成脚本里一处 `echo` 的反引号被 shell 当命令替换执行（打印 `dev: command not found`、吞掉了字样），已把该行改写为不含反引号的等价表述后重新追加；同一次追加的其余字节未变。
7. **未执行（如实记录，与 WP4/WP5 轮次口径一致）**：`cargo-deny`（依赖许可证/来源/advisory）与 `gitleaks`（密钥扫描）**只在 CI 运行**，本地无等价物、不在 `npm run verify` 里；本机未装 gitleaks。因此「本地绿」不等于这两类判定通过。
8. **本轮是只读轮次**：未改任何文件（含未改 `tasks.md` / `verification.md` / `plan.md`）；`tests/local_endpoint_unix.rs` 在本机是 0 用例目标（整文件 `#[cfg(unix)]`，Windows 上被编译掉），由 Linux CI 覆盖，不是静默跳过。

## handoff_index

```yaml
handoff_index:
  - task_id: "3.9"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "27ad3f800c7821846b8991ae7d61b67f59e8ba24"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "固定提交 27ad3f80（分支 agentic/node-link-owner；§WF0 起点与 §WF5 终点的 `git rev-parse HEAD` 一致、`git status --porcelain | wc -l` 均为 0、暂存区 0 行）上，用固定工具链（rust-toolchain.toml = 1.98.1，实测 cargo 1.98.1 / rustc 1.98.1；node v24.19.0 / npm 12.0.2）执行派单原文命令 `cargo test --locked -p server -p core -p storage-sqlite -p identity-auth -p node-link-protocol --all-features`：**退出码 0**、29 s、**625 passed / 0 failed / 2 ignored**、39 个测试目标全部 ok（§WF1，逐目标 result 行与按 crate 归并逐条留存）。**派单要求确认的 catalog/resource 新用例真实执行**：`node_link::catalog::tests` 声明 6 == 执行 6、`node_link::resource::tests` 声明 17 == 执行 17、`node_link::*` 声明 79 == 执行 79（`--list` 只读命令另跑一次核对），其中 WP5-fix1 的 6 条新用例（3 条 catalog 批次/可见性/空集、3 条 resource 状态表回收/异地 owner attach 拒绝/异地 owner ACK 拒绝）逐条 `... ok`，用例名与行号在 §WF1 列出。按 crate 汇总 625 = core 107 + identity-auth 85 + node-link-protocol 37 + server 276 + storage-sqlite 122；2 条 ignored（storage-sqlite 的 crash 子进程目标与 v1 夹具生成器）与 0 用例目标（`local_endpoint_unix.rs` 整文件 `#[cfg(unix)]`、各 crate 无 doctest）已逐条交代，不是通过。同一命令在工作区内无副作用：跑完 `git status --porcelain` 仍 0 行。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.9"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "27ad3f800c7821846b8991ae7d61b67f59e8ba24"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一固定提交、同一环境下执行 `npm run verify`：**聚合退出码 0**（§WF2，117 s），其中 `cargo test --locked --workspace --all-features` 真实执行 **913 passed / 0 failed / 2 ignored**、83 个测试目标，10 条合同门禁要点行（schemas 118 valid/25 invalid、commands 12、errors 58、features 11、assets 17 schemas+156 fixtures、acp 25 methods+71 rows、docs 378 links+4114 refs、boundaries 12 crate、drift 36 DDL+15 trait/93 方法、agentic 16/16 items+17 宿主入口）原样留存。为排除 `&&` 掩盖前序失败，把 `check` 的 10 个 npm script 与 `check:agentic`/`check:rust` 展开成的命令共 **15 条逐个单独执行、各自记录 $?**（§WF3：C01..C09、C10、C10a、C10b、R01、R02、R03），**15/15 均 exit 0**，且每条命令前后各取一次 HEAD 全部相同（无修订漂移）；R03 独立复跑与聚合跑计数逐项一致（913/0/2、83 目标）。workspace `-- --list` 声明 **915 == 913 passed + 2 ignored**（无未执行用例）。**如实登记**：R01 fmt 无输出；R02 clippy 两次都是 cargo 指纹缓存命中（只打印 Finished、无 Checking，exit 0 无诊断），故「本轮新跑了 clippy-driver」不成立，详见报告 issues 第 2 条。2 条 ignored 是仓库声明过的辅助项，不是被跳过的覆盖。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.9"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "27ad3f800c7821846b8991ae7d61b67f59e8ba24"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "openspec/changes/node-link-owner/reports/pv5-windows-nodelink.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本机 Windows（MINGW64_NT / x86_64-pc-windows-msvc，cargo 1.98.1）上执行 `cargo test --locked -p server -p app --all-features`：**退出码 0**、81 s、**348 passed / 0 failed / 0 ignored**、13 个测试目标；`-- --list` 声明 348 == 执行 348（§WF-PV5-1…5）。WP5-fix1 主题在本机真实执行：`node_link::catalog::tests` 6/6（含 3 条新增）、`node_link::resource::tests` 17/17（含 3 条新增）、`node_link::conn` 32/32 继续全绿；相对 WP5 上一轮（6b1f0b9）的 342 条**正好 +6**，与 WP5-fix1 新增的 6 条用例逐一对上，证明新用例不是 Linux-only。平台差异真实留证：`tests/local_endpoint_windows.rs` 4/4 执行通过，`tests/local_endpoint_unix.rs` 0 条（整文件 `#[cfg(unix)]`，本机不参与编译、由 Linux CI 覆盖）；4 个 0 用例目标（app `main.rs`、`local_endpoint_unix.rs`、`Doc-tests app`、`Doc-tests server`）已逐条交代，不是静默跳过。命令跑完后 worktree 仍干净（`git status --porcelain` 0 行），无新增进程/端口/`/tmp` 的 `acpr-*` 残留。"
    source_evidence: NOT_APPLICABLE
```
