# WP1 交付前 Project Verify 报告 — 任务 3.1（`node-link-owner`）

## Shared Report

- **task_id**: 3.1
- **role**: coder
- **phase**: implement
- **agent_context**: 任务级 coder 子 Agent，不继承主对话，只接收本任务固定输入；工具受限（无 subagent/无委派），工作目录固定为变更 worktree。**只读**仓库、只写本报告与原始日志；未修改任何代码、文档或规划文件。
- **target_revision**: `028a4d2a71ae01364012ffec4da9a85c7f8cf905`（分支 `agentic/node-link-owner`；执行前后 `git status --porcelain` 均为 0 行）
- **scope**: 任务 3.1 —— WP1 交付前 project verify：[PV1] `npm run verify`（合同门禁 + fmt/clippy/全 workspace 测试）与 [PV2] `node scripts/check-crate-boundaries.mjs`，逐项记录完整命令、工具链版本、退出码与日志路径，并核实资源已释放。不解释需求、不修复失败、不判定实现验收。
- **changes**: 无。本次任务零代码/文档/规划改动（`git status --porcelain` 空，见日志 §S1/S8 的显式行数 0）。
- **checks**: [PV1] PASS、[PV2] PASS（逐项退出码见下表与日志）。
- **issues**: 1 条**工具链自我缺陷**（我自己的日志包装脚本污染 `TMP`，导致一次**多余的**第三次 `cargo test` 出现 11 个假失败；已定位、已受控复现、不影响两条权威命令的结论，**不是仓库缺陷**）；1 条范围外提示（`cargo-deny`/`gitleaks` 无本地等价物，仍只在 CI 判定）。无阻断问题、无仓库失败项。
- **result**: PASS
- **evidence_paths**:
  - 原始日志（本角色）：`openspec/changes/node-link-owner/reports/du1-pv1.log`（113,746 字节 / 2,135 行；含 §S1 预检、§S2 `npm run verify` 完整原始输出、§S3 逐子检查退出码、§S4 `check-crate-boundaries` 完整原始输出、§S5 假失败根因与受控复现、§S6/§S7 全清环境复跑、§S8 资源收尾）
  - 本报告：`openspec/changes/node-link-owner/reports/wp1-verify.md`
- **resource_cleanup**: 无端口/数据库/容器/账号被创建或留存；只有本地 `npm`/`cargo` 调用。测试子进程零残留（`acpr-fake-acp-agent.exe`/`cargo.exe`/`rustc.exe`/`acp-remote.exe`/`link.exe` 全部 0 个）。worktree 自有构建产物 `target/` 6.9G、`node_modules/` 71M，按任务约定无需清理。日志包装脚本与临时输出位于仓库外的 `/tmp`。

### 环境（实测）

| 项 | 值 |
|---|---|
| 主机/外壳 | Windows（本机），git-bash，cwd `D:\Project\acp-remote-wt\node-link-owner` |
| node / npm | `v24.19.0` / `12.0.2`（≥ 合同门禁要求的 22.12） |
| rustc / cargo | `rustc 1.98.1 (48a229cea 2026-09-01)` / `cargo 1.98.1 (797e8a9bc 2026-08-05)`，host `x86_64-pc-windows-msvc`，与 `rust-toolchain.toml` pin 一致 |
| `CARGO_TARGET_DIR` | 未设置（cargo 使用 worktree 自有 `target/`） |

### 检查结果（逐项退出码）

| ID | 命令（cwd = 仓库根） | 退出码 | 关键结果 | 日志位置 |
|---|---|---|---|---|
| PV1 | `npm run verify` | **0**（118s；全清环境复跑 86s，同样 0） | 见下逐道 | §S2 / §S6 |
| PV1-a1 | `npm run check:schemas` | 0 | 118 valid / 24 invalid / 39 event views | §S3 C01 |
| PV1-a2 | `npm run check:commands` | 0 | 12 commands | §S3 C02 |
| PV1-a3 | `npm run check:errors` | 0 | 58 codes / 2 protocols | §S3 C03 |
| PV1-a4 | `npm run check:features` | 0 | 11 feature ids / 2 protocols | §S3 C04 |
| PV1-a5 | `npm run check:assets` | 0 | 17 schemas / 155 fixture files / 12 transcript vectors | §S3 C05 |
| PV1-a6 | `npm run check:acp` | 0 | 71 rows，ajv 校验通过 | §S3 C06 |
| PV1-a7 | `npm run check:docs` | 0 | 378 links / 4063 section refs / 257 md | §S3 C07 |
| PV1-a8 | `npm run check:boundaries` | 0 | 12 crate 与 §5 矩阵一致 | §S3 C08 |
| PV1-a9 | `npm run check:drift` | 0 | 36 条 DDL + 15 trait / 87 方法签名一致 | §S3 C09 |
| PV1-a10 | `node scripts/agentic-gate.mjs` | 0 | Installation PASS；Totals 16 passed, 0 failed | §S3 C10a |
| PV1-a11 | `node scripts/sync-agentic-host-entrypoints.mjs --check` | 0 | 路由一致，无差异 | §S3 C10b |
| PV1-b1 | `cargo fmt --all -- --check` | 0 | 无输出（无格式差异） | §S3 R01 |
| PV1-b2 | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | 0 | 无 warning | §S3 R02 |
| PV1-b3 | `cargo test --locked --workspace --all-features` | 0 | **718 passed / 0 failed / 2 ignored**；82 条 `test result` 行（70 个测试目标 + 12 个 doc-test 套件），其中 65 条用例数 >0、17 条为 0 用例，非通过行 0 条 | §S3 R03 / §S6 |
| PV2 | `node scripts/check-crate-boundaries.mjs` | **0**（两次独立执行） | `crate boundaries OK: 12 个 crate …`（含 `server`→`acpr-wire` 格） | §S4 / §S7 |

**用例确实执行**（无零用例、无全跳过）：82 条 `test result` 行全部为 `ok.`，其中 65 条 `passed > 0`；唯一的 2 个 `ignored` 是仓库**声明过的**辅助项，不是被跳过的覆盖：
`storage-sqlite tests\commit.rs::crash_child`（由 `crash_recovery_leaves_an_consistent_database` 主动拉起）与
`storage-sqlite tests\migration.rs::regenerate_v2_fixtures`（手动夹具生成器）。
17 条 `0 passed` 分别是 5 个空 lib/main/bin 目标、`local_endpoint_unix`（`#[cfg(unix)]`，Windows 上无用例，Linux CI 会跑到）与 11 个全空 doc-test 套件（第 12 个 `identity-auth` doc-test 有 2 个 compile-fail 用例，已计入 718）。逐套件与基线 `env1-baseline-cargo-test.log` 的 718/0/2 完全一致。

## issues（如实记录，含一条我自己造成的问题）

1. **工具链自我缺陷（已定位并复现，非仓库缺陷）**：我用于留证的包装脚本在最后一步做「多余的第三次 `cargo test`（仅用于计数）」时执行了 `TMP=$(mktemp)`，而本机 git-bash 会话**导出** `TMP=/tmp`、MSYS 会在启动原生进程时把它转换成 Windows 路径。于是 cargo/link.exe/每个测试进程看到的是 `TMP=C:\Users\zhang\AppData\Local\Temp\tmp.XXXXXXXX`——**一个文件而不是目录**，`std::env::temp_dir()` 因而不可用，`agent-host` 的 `tests/catalog.rs` 出现 11/22 假失败（`FAILED. … finished in 10.11s`）。
   - 直接证据：一次写成 `TMP=/tmp/s5_catalog_1.txt && cargo test …` 的探针命令让 MSVC 链接器报 `LNK1104: 无法打开文件“…\s5_catalog_1.txt\lnk{…}.tmp”`，证明 `TMP` 被当作临时目录使用（日志 §S5.2）。
   - 受控复现：同一套件在 `TMP` 为**合法目录**时 `22 passed / 0 failed`（exit 0）；把 `TMP` 指向**文件**后得到**同样的 11 个用例名**、`11 failed`、10.09s（对比 10.11s），失败信息全为临时目录症状（`写配置文件: Os { code: 3, kind: NotFound }`）（日志 §S5.3）。
   - 影响面：**零**。两条权威命令（§S2 的 `npm run verify`、§S3 的 `cargo test`）都在该赋值**之前**执行，使用的是正确的临时目录；事后我在全清环境把两条权威命令**各自重跑一遍**，退出码仍为 0（§S6、§S7）。
   - 结论：不构成仓库的 FAIL，也不改变 [PV1]/[PV2] 的 PASS；保留原始证据以便复核者独立判断。
2. **范围外**：`cargo-deny`（许可证/来源/advisory）与 `gitleaks`（密钥扫描）在本机不可执行且不属于 `npm run verify`，仍只由 CI 的 `deps`/`advisories`/`secrets` job 判定；本次未声称它们通过。
3. **范围外**：`target/` 因本机增量编译增大到 6.9G；属 worktree 自有产物，任务明确无需清理。

## handoff_index

```yaml
handoff_index:
  - task_id: "3.1"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "028a4d2a71ae01364012ffec4da9a85c7f8cf905"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在该固定提交（分支 agentic/node-link-owner，执行前后工作树干净）上用本机固定工具链（rust-toolchain.toml=1.98.1 实测一致，node v24.19.0/npm 12.0.2，Windows x86_64-pc-windows-msvc）执行 `npm run verify`（= npm run check 10 道子检查 + fmt/clippy/全 workspace 测试）：聚合退出码 0，且 14 个子检查逐条单独执行均 exit=0（日志 §S3），cargo test 真实执行 718 passed / 0 failed / 2 ignored；另在全清环境复跑一次仍 exit=0（§S6）。本次为首次完整执行 PV1（WP1 的 W0 轮次只跑过 npm run check + fmt，日志 wp1-deps.log 已注明不属完整 PV1），无复用。"
    source_evidence: NOT_APPLICABLE
  - task_id: "3.1"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "028a4d2a71ae01364012ffec4da9a85c7f8cf905"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "openspec/changes/node-link-owner/reports/du1-pv1.log"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一固定提交与同一环境上直接执行 `node scripts/check-crate-boundaries.mjs`：exit=0，输出 `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）`（含本 WP 新增的 `server`→`acpr-wire` 格），日志 §S4 保留完整原始输出；诊断结束后复跑同样 exit=0（§S7）。无复用。"
    source_evidence: NOT_APPLICABLE
```
