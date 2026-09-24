# acp-boundary-and-agent-host 执行与证据记录

> 本文件由主 Agent 持续维护；步骤与进度在 `tasks.md`，计划与安排在 `plan.md`。

## Target

- Change: `acp-boundary-and-agent-host`（schema `agentic`）
- Code Repository: `D:\Project\acp-remote`
- Target Kind / Ref: local；`refs/heads/main`
- 变更分支: `feat/acp-boundary-and-agent-host`
- 规划基线核实（2026-09-24，任务 1.1）：`git rev-parse refs/heads/main` = `094009b31f32c42928c01f2943eb7cf6e15c6154`；`git status --porcelain` 当时只有 `?? openspec/changes/acp-boundary-and-agent-host/`；`git rev-parse --abbrev-ref HEAD` = `main`。
- Version Confirmation Owner: 主 Agent（任务 6.1 在候选构造前重新核实并记录当时引用）。

### 环境事实（任务 1.1 recon，只读采集）

| 事实 | 实测值 | 证据 |
| --- | --- | --- |
| Rust 工具链 | `rustc 1.98.1 (48a229cea 2026-09-01)`，宿主 `x86_64-pc-windows-msvc` | `rustc -vV`；`rust-toolchain.toml` 的 `channel = "1.98.1"` |
| Node | `v24.19.0`（要求 ≥ 22.12） | `node -v` |
| 本机平台 | Windows x64（因此 PV5 的 Job Object 断言可在本机执行） | `rustc -vV` 的 `host` 字段 |
| `x-agentic.e2e` | `enabled = true`，`command = ""`，`maxAttempts = 3` | `npx --quiet --no-install openspec-agentic e2e --json` |
| 项目 Verify 入口 | `npm run verify` = `npm run check`（十道合同门禁）+ `cargo fmt --all -- --check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` + `cargo test --locked --workspace --all-features` | `package.json` 的 `scripts` |
| 规划工件未被外部改动 | 6 个规划文件的 mtime 均为主 Agent 本会话写入时间（09:00–09:06） | `stat -c '%y %n'` |
| 依赖可得性 | `libc 0.2.189`、`tracing 0.1.44`、`windows-sys 0.48.0` 已在 `Cargo.lock`；`win32job` 需新增；crates.io 索引与下载端点均可访问（HTTP 200） | `Cargo.lock` 检索；`curl` 到 `index.crates.io`/`static.crates.io` |

### 契约与文件所有权（任务 1.2）

- `Cargo.toml` 分波次串行写入：W0 = WP1（`[workspace.dependencies]`），W1 = WP2（`members` 加 `acp-protocol`），W2 = WP3（`members` 加 `agent-host`）；同一波次内不存在第二个写入者。
- 文档（`docs/MODULE_ARCHITECTURE.md`、`README.md`、`docs/DEVELOPMENT_PLAN.md`、`AGENTS.md`）唯一写入者为 WP1；`crates/acp-protocol/**` 为 WP2；`crates/agent-host/{Cargo.toml,src/lib.rs,src/error.rs,src/limits.rs,src/supervisor.rs,src/platform/**,src/bin/**}` 为 WP3；`crates/agent-host/src/{session,mapper,interaction}.rs` 为 WP4；`crates/agent-host/src/{catalog,config,credentials}.rs` 为 WP5。
- 冻结的接口契约（供 WP3/WP4/WP5 并行）：`acp-protocol` 的 public API 由 2.11 交付后冻结；`LaunchSpec { program, args, env }` 与 supervisor 句柄由 2.14 冻结。

### 运行资源与隔离（任务 1.3）

- 唯一共享运行资源是构建目录 `target/`（并行分片必须用 `CARGO_TARGET_DIR` 隔离）、测试自建的临时心跳目录（每个用例一个，结束即删）与子进程。
- 不涉及数据库服务、容器、端口或外部账号；依据：本变更是两个库 crate，fake ACP child 由仓库内测试二进制提供，不需要外部服务。
- **PV5 平台限制**：Windows Job Object 断言只能在 Windows 执行；Linux CI 只跑 `#[cfg(unix)]` 分支。本机宿主为 `x86_64-pc-windows-msvc`，因此 PV5 可在本机执行并留证；若本机不可用则记 BLOCKED。

## Checks

| Check ID / Stage / Work Package | Revision / Base | Scope | Executor | Command / Steps | Environment | Result / Exit Code | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| recon / 前置 / 1.1–1.3 | 基线 `094009b`（`refs/heads/main`，未改动） | 仓库/目标引用、工具链、E2E 开关、资源与隔离 | 主 Agent（recon 角色，只读） | 见上「环境事实」表的各条命令 | Windows x64、rustc 1.98.1、Node 24.19.0 | PASS | 本文件「Target」节；无独立日志（只读命令输出已在表中逐条记录） |
| PV2 / W0 交付前 / WP1 | 工作区 = W0 修改后（文档 + `Cargo.toml` 依赖登记），基线 `094009b` | §5 依赖矩阵逐条比对、`core` 闭包 allow-list、文档引用 | 主 Agent（实现 Agent） | `node scripts/check-crate-boundaries.mjs`；`node scripts/check-doc-links.mjs` | Windows x64、rustc 1.98.1、`cargo metadata --no-deps --locked` | PASS（exit 0）：`crate boundaries OK: 6 个 crate 的依赖方向与 §5 矩阵一致（6 个已在矩阵登记）`；`doc links OK: 367 relative links, 2649 section refs across 137 markdown files` | `openspec/changes/acp-boundary-and-agent-host/reports/wp1-boundaries.log` |
| PV3 / W0 交付前 / WP1 | 同上 | ACP 矩阵结构/枚举、夹具 manifest 逐例、固定快照 sha256、夹具哈希 | 主 Agent（实现 Agent） | `node scripts/check-acp-compatibility.mjs`；`node scripts/check-schema-fixtures.mjs`；`sha256sum` 逐文件 | 同上（只读脚本 + ajv） | PASS（exit 0）：`25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows)`；`117 valid, 23 invalid`；矩阵 sha256 `c79061ef…` 与快照 sha256 `3c17bd63…`（与 `ACP_COMPATIBILITY_MATRIX.md` §2 固定值一致） | `openspec/changes/acp-boundary-and-agent-host/reports/wp1-acp-assets.log` |

## Check Plan Changes

无（截至 W0 开始，未发生需求/接口澄清或检查清单调整）。

## Dependency Handoffs

（待 W1 起逐条记录。）

## Runtime Resources

（待各检查实际执行后记录；W0 只使用 `target/` 的默认构建目录。）

## Review Findings

（待 3.2 起逐条记录。）

## Merge History

（待 5.1 起记录。）

## Test Design and Authoring

不适用：Main E2E mode = `not-applicable`，不生成测试工作包（TP）与用例设计记录。

## Candidate E2E

不适用（mode = `not-applicable`）。

## Main E2E

- 项目开关实测（任务 1.1）：`enabled = true`、`command = ""`、`maxAttempts = 3`。
- `plan.md` 的 mode = `not-applicable`，含 `reason`、`basis`、四项 `alternative_checks` 与可追溯到用户的 `downgrade_approval`（用户原话「1. 同意 2. 同意」，2026-09-24 本会话）。
- E2E 状态：**NOT_APPLICABLE**；替代验证在任务 7.1–7.2 执行并逐项留证（见本文件 `Checks` 表的 PV1/PV4/PV5 行）。

## Failures and Retests

无。

## Final Assessment

```agentic-assessment
assessment_id: "round-1"
target_commit: ""
contract_digest: ""
result: BLOCKED
evidence: []
```

- Assessment ID / Time: round-1（待最终验收轮填写）
- Target / Task: 待 8.1 填写（目标 = 最终 `refs/heads/main` 提交）
- CLI State: 实施中（本轮尚未执行 `workflow check --stage final`）
- Audit / Evidence: 待 8.1
- Result / Open Issues: BLOCKED（尚未进入最终验收）
- Required Follow-up: 完成 3.x、5.x、6.x、7.x 后由主 Agent 执行最终验收
