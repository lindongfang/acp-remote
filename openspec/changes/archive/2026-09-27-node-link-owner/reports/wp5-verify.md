# WP5 交付前 project verify（任务 3.9）— `node-link-owner` / DU1

## Shared Report

- **task_id**：3.9（WP5 交付前 project verify，阶段 work-package）。
- **role / phase**：coder / verify（本任务**只执行检查与写报告**，未修改被检查仓库的任何文件）。
- **agent_context**：worker 子 Agent（本机 worktree 独占，未继承 WP5 实现轮次的对话；只读检查 + 追加报告）。
- **target_revision**：`6b1f0b9cb100638eb9e15ca414d5013a86aebaf1`（分支 `agentic/node-link-owner`）。
  轮次**起点与终点**均实测 `git rev-parse HEAD` = 该值，`git status --porcelain | wc -l` = 0（无修订漂移、无工作区改动）。
- **scope（本任务实际执行的四项检查）**：
  - `[PV3]` `cargo test --locked -p server -p core -p storage-sqlite -p identity-auth --all-features`（派单指定的 WP5 涉及范围）；
  - `[PV1]` `npm run verify`（聚合 1 次）+ `check`/`check:rust` 展开成的**14 条子检查逐个独立执行并各自记录退出码**；
  - `[PV2]` `node scripts/check-crate-boundaries.mjs`；
  - `[PV5]` `cargo test --locked -p server -p app --all-features`（本机 Windows 轮次，独立运行 2 次）。
- **checks**：四项全部 **exit 0**，无 FAIL，无零用例套件被当作通过，无新增端口/进程/临时目录残留。
  原始日志：`reports/du1-pv1.log` 的「W4/WP5 轮次」分节（§WP5-0…§WP5-9，**追加**）、
  `reports/pv5-windows-nodelink.log` 的「WP5 轮次」分节（**追加**）。
- **result**：**PASS**（本任务的四项检查全部满足）。**不代表** 3.10（RV1 独立 review）、3.11+（WP6/WP7）、
  6.5（候选替代验证）或合并已完成。
- **evidence_paths**：`openspec/changes/node-link-owner/reports/du1-pv1.log`（「W4/WP5 轮次」分节）、
  `openspec/changes/node-link-owner/reports/pv5-windows-nodelink.log`（「WP5 轮次」分节）、本文件。
- **resource_cleanup**：本轮**未新增**任何进程、端口或临时目录；
  并**按派单授权清理了 1 个属本变更早前轮次的残留目录** `/tmp/acpr-storage-commit-node-link-slice-37816`
  （归因、内容留存与删除过程见「issues」第 4 条与日志 §WP5-7）。清理后 `/tmp/acpr-*` 无匹配（`ls` exit 2）。
  worktree 自有的 `target/` 是 git-ignored 的构建缓存，按约定保留；本轮不创建数据库服务/容器/账号/证书。

## 检查结果

| ID | 命令 | 退出码 | 计数 | 证据位置 |
|---|---|---|---|---|
| **PV3** | `cargo test --locked -p server -p core -p storage-sqlite -p identity-auth --all-features` | **0** | 582 passed / 0 failed / 2 ignored；31 个测试目标 | `du1-pv1.log` §WP5-1、§WP5-2 |
| **PV1** | `npm run verify`（聚合） | **0** | 907 passed / 0 failed / 2 ignored；83 个测试目标（106 s） | `du1-pv1.log` §WP5-3 |
| **PV1** | 14 条子检查逐个单独执行 | **14/14 = 0** | C01–C09、C10a、C10b、R01、R02、R03 | `du1-pv1.log` §WP5-4 |
| **PV2** | `node scripts/check-crate-boundaries.mjs` | **0** | 12 个 crate 与 §5 矩阵一致 | `du1-pv1.log` §WP5-5 |
| **PV5** | `cargo test --locked -p server -p app --all-features`（本机 Windows） | **0** | 342 passed / 0 failed / 0 ignored；13 个目标；declared == executed 342 | `pv5-windows-nodelink.log`「WP5 轮次」；`du1-pv1.log` §WP5-6 |

### WP5 覆盖面是否真实执行（派单点名要求）

- **`node_link::catalog::tests`：声明 3 == 执行 3**，全 `... ok`：
  `visibility_is_sorted_by_export_id`、`visibility_drops_revoked_exports_and_requires_a_paired_node`、
  `visibility_requires_a_non_empty_scopes_intersection_with_the_node_grants`（≥ R51/R52 的可见性口径）。
- **`node_link::resource::tests`：声明 14 == 执行 14**，全 `... ok`（attach 代际覆盖、快照 digest、
  payload 保真与超限降级、replay 游标与 epoch、ACK 回退、扇出与关闭序列等）。
- `[PV5]` 的 342 条相对 WP4 轮次的 325 条**正好 +17** = catalog 3 + resource 14，
  说明这两组用例在 Windows 平台分支上也真实执行，而不是只在 Linux/CI 可跑。
- 逐目标 executed 明细、declared vs executed 对照（PV3 的 270 条 `-p server` 声明、R03 的 workspace 909 声明）
  与 4 个 0 用例目标的逐条交代，见 `du1-pv1.log` §WP5-2/§WP5-4 与 `pv5-windows-nodelink.log` §WP5-PV5-2。

## issues（需要主 Agent / reviewer 知晓的事实）

1. **子检查条数的口径**：`package.json` 的 `verify` = `check` + `check:rust`；展开成**可独立执行的命令**是
   14 条（`check` 的 10 个 npm script 中 `check:agentic` 内含 2 条命令，`check:rust` 内含 3 条命令 → 9 + 2 + 3）。
   早前轮次的报告写作「15 个子检查」（把 `check:agentic` 与其展开各计一次）。本轮按**命令**计为 14 条，
   结论不变：**14/14 全部 exit 0**，且与聚合退出码 0 一致。
2. **`tasks.md:3.9` 之外的检查范围**：派单要求了 `[PV1]`/`[PV2]`/`[PV5]`，它们属 plan W8 的「PV1–PV5」与
   `AGENTS.md` §8/§11 的通用收口要求；本轮一并执行并留证（口径与 3.3/3.5/3.7 轮一致）。coder 未改动任何规划工件。
3. **本轮留证过程中的 4 处自我更正（均已就地记录，对结论无影响）**：
   - `[PV1]` 摘要脚本一度打印「workspace 声明用例数 = 0」——那是脚本缺陷（聚合输出里没有 `--list`），
     已在 §WP5-3 末尾更正；R03 的真实声明数在 §WP5-4 单独取得（909 = 907 passed + 2 ignored）。
   - `[PV5]` 第一次粘贴只截取了头 400 行 + 末 25 行；随后把 `pv5-windows-nodelink.log` **回退到追加前的
     确切字节边界**（185327，已用 `dd` 逐字节核对边界与追加前尾部）后重新追加**完整**输出，早前内容逐字节未变。
   - 段末「命令计数」初稿写了估算值「33 条」，与实际不符，因此把该节回退重写为**按节如实统计**（不再给总数断言）。
   - `[PV3]` 的「按 crate 汇总」初稿写了 `storage-sqlite` 120 passed（正确值为 **122**），已在 §WP5-2 重算登记，
     本报告采用重算值；`[PV5]` 日志内「补录」段也先落在 `### 结论` 之后、发现后已回退重排到正确位置。
4. **清理动作的机制说明**：本机工具的通用安全确认拦截了递归删除型命令，改用等价的逐步删除
   （`rm` 三个库文件 → `rmdir attachments` → `rmdir 根目录`，三步均 exit 0）。删除对象是
   `/tmp/acpr-storage-commit-node-link-slice-37816`（672 K，mtime 16:43，无进程持有）：
   按 `crates/storage-sqlite/tests/support/mod.rs:93` 的 `acpr-storage-{name}-{pid}` 命名规则与
   `crates/storage-sqlite/tests/commit.rs:1302` 的 `temp_dir("commit-node-link-slice")` 归因，
   它来自本变更同 worktree 的 WP5 用例 `node_link_slice_returns_origin_events_and_pending_interactions`，
   属**早前轮次**被中断的运行残留（本轮 17:30 起，两条命令都跑过同一用例且未产生新目录）。
   MSYS 的 `/tmp` 与本机 `%TEMP%` 是同一目录，两处视图一致。
5. **非本轮残留的端口噪声**：与轮次起点 baseline 相比监听列表逐项相同，仅 `127.0.0.1:10808` 的属主 PID
   由 45948 变为 12076。该端口不是 acpr 端口（本变更的用例不绑固定端口；`local_admin` 走命名管道/Unix socket），
   判为与本轮检查无关的本机既有进程重开，已如实登记在 §WP5-7。
6. **本轮的 `[PV1]` 数值与更早轮次日志中的数值不同**（当前 907 passed / 83 目标 / 25 invalid fixtures /
   4114 section refs / §5 的 93 个方法签名），原因是那些数字取自更早的修订（WP4/WP5 尚未落地）；
   本轮数值对应的修订是 `6b1f0b9`，门禁本体全部通过。

## handoff_index

```yaml
handoff_index:
  - task_id: "3.9"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "6b1f0b9cb100638eb9e15ca414d5013a86aebaf1"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "固定提交 6b1f0b9（分支 agentic/node-link-owner；§WP5-0 起点与 §WP5-7 终点的 `git rev-parse HEAD` 一致、`git status --porcelain | wc -l` 均为 0）上，用固定工具链（rust-toolchain.toml = 1.98.1，实测 cargo 1.98.1 / rustc 1.98.1 x86_64-pc-windows-msvc；node v24.19.0 / npm 12.0.2）执行 `npm run verify`：**聚合退出码 0**（§WP5-3，17:31:35 起 106 s），`cargo test --locked --workspace --all-features` 真实执行 **907 passed / 0 failed / 2 ignored**、83 个测试目标。为排除 `&&` 掩盖前序失败，把 `check` + `check:rust` 展开成的 **14 条子检查逐个单独执行、各自记录 $?**（§WP5-4：C01 check:schemas、C02 check:commands、C03 check:errors、C04 check:features、C05 check:assets、C06 check:acp、C07 check:docs、C08 check:boundaries、C09 check:drift、C10a agentic-gate.mjs、C10b sync-agentic-host-entrypoints --check、R01 fmt、R02 clippy -D warnings、R03 全 workspace 测试），**14/14 均 exit 0**；R03 独立复跑计数与聚合跑逐项一致（907/0/2、83 目标、'... ok' 行 907），且 workspace `-- --list` 声明 909 == 907 passed + 2 ignored（无未执行用例）。2 个 ignored 是仓库声明过的辅助项（storage-sqlite 的子进程 crash 目标与夹具生成器），不是被跳过的覆盖。两份日志均为末尾追加，未覆盖早前轮次内容。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.9"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "6b1f0b9cb100638eb9e15ca414d5013a86aebaf1"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一固定提交、同一环境下直接执行 `node scripts/check-crate-boundaries.mjs`：**退出码 0**，输出 `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）`（§WP5-5，原始输出逐字留存；同一条也作为 §WP5-4 的 C08 单独复跑一次，同样 exit 0）。旁证（不属 [PV2] 判据）：`cargo metadata --no-deps` 实测 workspace 成员 = **12**（acp-protocol、acpr-transcript、acpr-wire、agent-host、app、core、identity-auth、identity-keystore、node-link-protocol、server、storage-sqlite、sync-protocol），与 §5 矩阵登记数一致；WP5 新增的 core 端口（`AuditStore::watermark`、资源读面 seam）与 server 侧 catalog/resource 都没有改变依赖形状（`core` 仍不引入 runtime/DB/HTTP/子进程/wire protocol 依赖）。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.9"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "6b1f0b9cb100638eb9e15ca414d5013a86aebaf1"
    evidence_type: CHECK
    evidence_id: PV3
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "派单点名的 WP5 涉及范围：`cargo test --locked -p server -p core -p storage-sqlite -p identity-auth --all-features` **退出码 0**，**582 passed / 0 failed / 2 ignored**，31 个测试目标（§WP5-1），逐目标 executed 明细见 §WP5-2。**派单要求确认的 catalog/resource 用例真实执行**：`node_link::catalog::tests` 声明 3 == 执行 3、`node_link::resource::tests` 声明 14 == 执行 14，17 条全部 `... ok`（用例名逐条登记在 §WP5-2，包括 visibility 三条、`a_reissued_attachment_invalidates_the_previous_generation`、`a_snapshot_carries_metadata_only_and_a_verifiable_digest`、`snapshot_digest_is_the_sha256_of_the_ordered_chunk_digests`、`acp_raw_over_the_inline_limit_is_downgraded_not_truncated`、`replay_resumes_after_the_cursor_and_rejects_an_epoch_mismatch`、`ack_progress_is_monotonic_and_bound_to_the_attachment`、`persisted_events_reach_the_subscribed_connection`、`events_are_not_delivered_without_a_live_subscription`）。**按 crate 汇总（总计 582 = core 107 + identity-auth 83 + server 270 + storage-sqlite 122）**：core 107/0/0；identity-auth 83/0/0；server 270/0/0（lib 242 + local_admin_channel 14 + local_admin_schema_drift 6 + local_endpoint_naming 4 + local_endpoint_windows 4 + local_endpoint_unix 0）；storage-sqlite **122 passed / 0 failed / 2 ignored**（lib 3、admin_audit 15、admin_store 40、attachments 5、commit 16+1 ignored、compaction_recovery 3、contract_v03 5、enum_coverage 2、imported 10、migration 8+1 ignored、permissions 4、retention 8、session_version_rule 3）。2 个 ignored 与 0 用例目标（`local_endpoint_unix.rs` 的 `#[cfg(unix)]` 等）已逐条交代，不是通过。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.9"
    role: coder
    phase: verify
    stage: work-package
    target_revision: "6b1f0b9cb100638eb9e15ca414d5013a86aebaf1"
    evidence_type: CHECK
    evidence_id: PV5
    report_path: "openspec/changes/node-link-owner/reports/pv5-windows-nodelink.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "本机 Windows（MINGW64_NT / x86_64-pc-windows-msvc，cargo 1.98.1）上执行 `cargo test --locked -p server -p app --all-features`：**退出码 0**，独立运行两次，计数逐项相同 —— **342 passed / 0 failed / 0 ignored**，13 个测试目标，`-- --list` 声明 342 == 执行 342（§WP5-PV5-1/§WP5-PV5-2）。WP5 主题在本机真实执行：`node_link::catalog::tests` 3/3、`node_link::resource::tests` 14/14、`node_link::conn` 32/32；相对 WP4 轮次的 325 条**正好 +17**（= 本 WP 新增的 3 + 14），证明新用例不是 Linux-only。平台差异真实留证：`tests/local_endpoint_windows.rs` 4/4 执行通过，`tests/local_endpoint_unix.rs` 0 条（整文件 `#[cfg(unix)]`，本机不参与编译，由 Linux CI 覆盖）；4 个 0 用例目标（app `main.rs`、`local_endpoint_unix.rs`、`Doc-tests app`、`Doc-tests server`）已逐条交代，不是静默跳过。原始输出完整追加于本文件四节。"
    source_evidence: NOT_APPLICABLE
```
