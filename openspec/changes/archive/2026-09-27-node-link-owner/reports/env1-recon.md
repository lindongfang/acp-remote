# Environment Recon Report — 任务 1.1 (`node-link-owner`)

## Shared Report

- **task_id**: 1.1
- **role**: environment
- **phase**: recon
- **agent_context**: 任务级子 Agent（scout/environment recon），`fork_turns="none"` 等效隔离；不继承主对话，仅接收本任务固定输入。
- **target_revision**: `94a64e1f15d26f54a6601985fd13b1440fec8170`（`refs/heads/main`，已核实）
- **scope**: 只读核实主仓库 `D:\Project\acp-remote` 的运行时基线事实（git 状态、工具版本、合同门禁、workspace 测试、目录现状、远端 main）。不解释需求、不改代码、不判定实现验收。
- **changes**: NOT_APPLICABLE（只读侦察，未修改任何受版本控制文件）
- **checks**: NOT_APPLICABLE（本任务无计划内 Check ID；命令执行证据见 `commands` 与 `observations`）
- **issues**: 无阻断问题。基线全绿。
- **result**: PASS
- **evidence_paths**:
  - 本报告：`openspec/changes/node-link-owner/reports/env1-recon.md`
  - `npm run check` 原始日志：`openspec/changes/node-link-owner/reports/env1-baseline-npm-check.log`
  - `cargo test` 原始日志：`openspec/changes/node-link-owner/reports/env1-baseline-cargo-test.log`
- **resource_cleanup**: 未创建运行资源；仅向 `target/` 生成构建副产物与两份日志；未改动仓库受版本控制内容。

### result 判据

`recon` 的 PASS 仅表示所列事实已成功采集并有证据。目标事实与输入一致（`refs/heads/main` = 94a64e1…，worktree 列表与说明一致），无 BLOCKED。

## commands

| # | 命令（工作目录 `D:\Project\acp-remote`） | 退出码 | 关键输出 |
|---|---|---|---|
| 1a | `git rev-parse refs/heads/main` | 0 | `94a64e1f15d26f54a6601985fd13b1440fec8170` |
| 1b | `git status --porcelain` | 0 | ` M docs/DEVELOPMENT_PLAN.md` / `?? openspec/changes/node-link-owner/` |
| 1c | `git worktree list` | 0 | `D:/Project/acp-remote 94a64e1 [main]` 与 `D:/Project/acp-remote-wt/node-link-owner 94a64e1 [agentic/node-link-owner]` |
| 1d | `git log -1 --format=%H%n%s` | 0 | `94a64e1…` / `fix: 根治测试临时目录泄漏与 agent-host 超限退出顺序竞态 (#27)` |
| 2a | `node --version` | 0 | `v24.19.0` |
| 2b | `npm --version` | 0 | `12.0.2` |
| 2c | `test -d node_modules` | 0 | 存在（已安装；未执行 `npm ci`） |
| 3a | `cat rust-toolchain.toml` | 0 | `channel = "1.98.1"`，components clippy/rustfmt，profile minimal |
| 3b | `cargo --version` | 0 | `cargo 1.98.1 (797e8a9bc 2026-08-05)` |
| 3c | `rustc --version` | 0 | `rustc 1.98.1 (48a229cea 2026-09-01)` |
| 4 | `npm run check` | 0 | 全部 10 道子检查 OK（详见下） |
| 5 | `cargo test --locked --workspace --all-features` | 0 | 编译成功；718 passed / 0 failed / 2 ignored |
| 6 | 目录现状（`ls`/`find`） | 0 | 见 observations |
| 7 | `git ls-remote origin refs/heads/main` | 0 | `94a64e1f15d26f54a6601985fd13b1440fec8170 refs/heads/main`（远端 = 本地） |

## observations

1. **git 基线**
   - `refs/heads/main` 与远端 `origin/refs/heads/main` 均为 `94a64e1f15d26f54a6601985fd13b1440fec8170`，本地与远端一致。
   - 最近提交：`fix: 根治测试临时目录泄漏与 agent-host 超限退出顺序竞态 (#27)`。
   - `git status --porcelain` 输出与任务预期一致：` M docs/DEVELOPMENT_PLAN.md`（`git diff --numstat` 为空，确认为行尾 LF→CRLF 噪声，无内容差异）与 `?? openspec/changes/node-link-owner/`（本变更规划工件）。**均未修复**。
   - `git worktree list` 显示实现 worktree `D:/Project/acp-remote-wt/node-link-owner`（分支 `agentic/node-link-owner`，当前同样位于 94a64e1）；按任务约定该 worktree 不在核实范围内。

2. **工具版本**
   - Node `v24.19.0`、npm `12.0.2`（均满足 AGENTS.md §10 要求的 Node ≥ 22.12）。
   - `rust-toolchain.toml` 固定 `channel = "1.98.1"`；实测 `cargo 1.98.1` / `rustc 1.98.1` 与其一致。
   - `node_modules` 已安装。

3. **合同门禁基线（`npm run check`，退出码 0）**——10 道子检查全部 OK：
   - `check:schemas`：schema fixtures OK（118 valid, 24 invalid, 39 event views bound）
   - `check:commands`：command catalog OK（12 commands）
   - `check:errors`：error registry OK（58 codes / 2 protocols）
   - `check:features`：feature registry OK（11 feature ids / 2 protocols）
   - `check:assets`：contract assets OK（17 schemas, 155 fixtures, 12 transcript vectors, 20 negative, 2 SAS）
   - `check:acp`：ACP compatibility matrix OK（25 methods, 11 updates, …）
   - `check:docs`：doc links OK（379 links, 4999 section refs, 278 md files；2406 section refs 按设计未判定）
   - `check:boundaries`：crate boundaries OK（12 crates 与 §5 矩阵一致）
   - `check:drift`：contract drift OK（§7 36 条 DDL、§5 15 trait/87 方法签名一致）
   - `check:agentic`：Installation PASS；openspec schema validate `agentic`：17 passed / 0 failed；宿主入口 17 文件一致。

4. **测试基线（`cargo test --locked --workspace --all-features`，退出码 0）**
   - 编译成功（无 error/warning 门禁失败）。
   - 汇总：**718 passed / 0 failed / 2 ignored**（跨 82 个 `test result:` 行；`Finished` + 各 crate lib/tests 二进制）。
   - 无 `FAILED` / `panicked` 条目。

5. **现状事实（crates / server / app / openspec）**
   - `crates/`：`acp-protocol`, `acpr-transcript`, `acpr-wire`, `agent-host`, `app`, `core`, `identity-auth`, `identity-keystore`, `node-link-protocol`, `server`, `storage-sqlite`, `sync-protocol`（共 12 个）。
   - `crates/server/src/` 一级子项：`lib.rs`、`local_admin/`、`transport/`。`local_admin/` 内含 `audit.rs daemon.rs envelope.rs error.rs handler.rs method.rs mod.rs pairing.rs params.rs router.rs test_support.rs view.rs`；`transport/` 内含 `local/`、`mod.rs`（尚未见 `node_link/`、`sync/`、`acp_facade/` 目录）。
   - `crates/app/src/` 一级子项：`cli/`、`cli.rs`、`client.rs`、`clock.rs`、`compose.rs`、`config.rs`、`daemon.rs`、`identity.rs`、`lib.rs`、`lock.rs`、`logging.rs`、`main.rs`、`stdio.rs`。
   - `crates/node-link-protocol/src/`：`catalog.rs command.rs common.rs domains.rs envelope.rs error.rs handshake.rs lib.rs pairing.rs resource.rs`（协议 crate 已落地）。
   - `openspec/specs/`（16 个）：`acp-wire-protocol, admin-state-persistence, cli-commands, core-event-view-identity, daemon-lifecycle, identity-handshake, identity-pairing, local-admin-channel, local-admin-methods, local-agent-config, local-agent-host, peer-identity-material, platform-keystore, scope-expansion, storage-schema-v2-migration, workspace-resolution`。
   - `openspec/changes/`：仅 `archive/` 与未归档变更 `node-link-owner/`（含 `.openspec.yaml design.md plan.md proposal.md reports/ specs/ tasks.md verification.md`）。

## handoff_index

```yaml
handoff_index:
  - task_id: "1.1"
    role: environment
    phase: recon
    stage: recon
    target_revision: "94a64e1f15d26f54a6601985fd13b1440fec8170"
    evidence_type: RESOURCE
    evidence_id: NOT_APPLICABLE
    report_path: "openspec/changes/node-link-owner/reports/env1-recon.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在 refs/heads/main=94a64e1 与 node v24.19.0/npm 12.0.2/cargo 1.98.1 环境下本次原始执行；无依赖或其他角色证据复用。"
    source_evidence: NOT_APPLICABLE
```
