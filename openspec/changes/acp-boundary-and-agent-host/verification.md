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
| PV4 / W1 交付前 / WP2 | 工作区 = W1（`crates/acp-protocol` 新增 + `[workspace] members` 加入该 crate），基线 `4b0145e` | WP2 的全部行为测试与代码质量：fixture/矩阵驱动契约、raw 保真、未知判别子、扩展方法、上限、capability 真实性 | 主 Agent（实现 Agent，WP2） | `cargo fmt --all -- --check`；`cargo clippy --locked -p acp-protocol --all-targets --all-features -- -D warnings`；`cargo test --locked -p acp-protocol --all-features`；`node scripts/check-crate-boundaries.mjs` | Windows x64、rustc 1.98.1、Node 24.19.0 | PASS（全部 exit 0）：测试 33 个全通过（capabilities 5、envelope 6、fixtures 2、matrix_tables 5、raw_fidelity 7、updates 8），无 0 用例文件、无跳过；`crate boundaries OK: 7 个 crate 的依赖方向与 §5 矩阵一致` | `openspec/changes/acp-boundary-and-agent-host/reports/wp2-acp-protocol-tests.log` |

## Check Plan Changes

1. **Unix 进程组结束的依赖：`libc` → `nix`**（原：`design.md` D6 写「`libc` 是本变更唯一新增的 unix 依赖」；新：`nix` 0.30，`default-features = false`，features `signal`/`process`）。理由：workspace 固定 `unsafe_code = "forbid"`，而 `libc::killpg` 是 `unsafe` 外部函数，直接调用必须写 `unsafe` 块（`forbid` 无法用 `#[allow]` 绕过）；`nix` 提供安全封装，且其 MSRV 低于仓库 `rust-version = 1.85`。风险覆盖：Unix 进程组结束的语义不变（仍是 `killpg(SIGKILL)`），许可证仍为 MIT。受影响任务：2.17（`platform/unix`）、3.5/3.6（PV4/PV5 与 review）。
2. **适配器产出的 view 不含 `turnId`/`version`**（原：`docs/SYNC_PROTOCOL.md` §10.3 把 `turnId`/`version` 列为 `turn.*`、`session.mode.changed`、`session.config.changed` 等 view 的最低必填字段；新：`agent-host` 的 mapper 不产出这两个字段）。理由：`SessionEndpoint::prompt(request, at)` 的签名不携带 core 的 `TurnId`，会话 `version` 也由 core 掌握；适配器无法得知它们。core 侧真正消费的字段（`interactionId`、`messageId`/`deltaIndex`/`text`/`block`）已全部提供（`crates/core/src/broker.rs` 的 `view_interaction_id`/`delta_fragment`），因此本地路径功能不受影响；缺口只影响 Sync 切片的 view 字段级校验。风险覆盖：已登记为 `design.md` 的 Risk #10；收口方式（由 core 注入两个字段，或改端口签名）属于 **core 端口/契约变更**，需用户决策，不在本变更内做。受影响任务：2.21（mapper）、3.7/3.8（WP4 的 PV4 与 review），以及下游 Sync 切片。

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| WP3 | WP2 | WP2 冻结的 public API（`RawDocument`/`Envelope`/`AcpError`/`methods`/`content`/`update`/`message`/`capability`）+ 任务 3.3 的 PV4 证据（33 测试全通过） | `4b0145e`（W0 基线） | `cargo build -p acp-protocol` 通过；`agent-host` 只用 `acp-protocol` 的公开项 | `acp-protocol` 的 public API 变化 → WP3/WP4/WP5 复验 |

## Runtime Resources

- W0/W1 只使用默认 `target/`（无并行分片），未占用数据库/端口/外部账号；测试的临时目录尚未引入（WP3 起由 fake ACP child 用例自建并清理）。
- 本会话未安装额外工具；Windows 是本机宿主，因此 PV5 可在本机执行。

## Review Findings

待 3.2（WP1 独立 review）与 3.4（WP2 独立 review）执行；本会话尚未创建隔离 reviewer 上下文，因此两项均为未执行（见 Failures and Retests 的 `SESSION-1`）。

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

| Issue ID / Task | Source / Check or E2E ID / Attempt / Version | Owner | Fix / Recovery | Review / Readiness Evidence | Retest Evidence | Current Status / Basis |
| --- | --- | --- | --- | --- | --- | --- |
| WP2-BUILD-1 / 2.5–2.11 | W1 开发期 `cargo build -p acp-protocol` 报 30 个错误（serde 属性里用 const、`serde::de::Deserialize` 与 derive 混淆、`Option<&str>` 生命周期、`ToolCallContent` 导入路径错、`expect_err` 需要 `Debug` 等） | 主 Agent（WP2） | 逐项修正（导出位置/生命周期/derive 导入/`Debug` derive）；`cargo fmt` 后又修 `clippy::empty_line_after_doc_comments` | 无需独立 review（属实现期编译与 lint 修正，无契约变化） | `reports/wp2-acp-protocol-tests.log`（最终 fmt/clippy/test 全部 exit 0） | 已解决 |
| WP2-TEST-1 / 2.11 | `fixtures` 用例的负例断言过严：`from_value` 的错误消息不含字段名（`invalid number`）而断言要求含 `line` | 主 Agent（WP2） | 改为经 `from_slice` 解码以保留字段上下文，并加「同形状合法值必须成功」的控制组 | 同上 | `reports/wp2-acp-protocol-tests.log`（`tool_call_location_rejects_out_of_range_line` PASS） | 已解决 |
| SESSION-1 / 2.12–2.27、3.x | 本会话上下文预算耗尽：WP3（`agent-host`）只完成脚手架（`Cargo.toml`/`error`/`limits`/`platform`/`config`/`launch`/`process`/`mapper`），`session`/`host`/`lib`/fake ACP child/测试未完成，**代码未编译** | 主 Agent | 未完成部分保持待办；`crates/agent-host/` 作为未提交工作区留在分支上（未加入 `[workspace] members`，因此不影响 `npm run check`/`cargo test --workspace` 的判定） | 无（未达到可 review 状态） | 无 | **未解决**：需新会话继续 WP3–WP5、独立 review、集成与最终验收 |

## Final Assessment

```agentic-assessment
assessment_id: "round-1"
target_commit: ""
contract_digest: ""
result: BLOCKED
evidence: []
```

- Assessment ID / Time: round-1（待最终验收轮填写）；本会话只推进到 WP2 完成，未进入最终验收
- Target / Task: 待 8.1 填写（目标 = 最终 `refs/heads/main` 提交）
- CLI State: 实施中（尚未执行 `workflow check --stage final`）
- Audit / Evidence: 有效证据 = PV2/PV3（W0，`reports/wp1-*.log`）与 PV4（W1/WP2，`reports/wp2-acp-protocol-tests.log`）；WP3 及以后无有效证据
- Result / Open Issues: **BLOCKED** — `agent-host` 未编译、未测试；独立 review（3.2/3.4 及后续）未执行；PV5 未执行
- Required Follow-up: 在新会话继续 2.12–2.27 → 3.1–3.10 → 5.x/6.x/7.x/8.1；完成后由主 Agent 执行最终验收
