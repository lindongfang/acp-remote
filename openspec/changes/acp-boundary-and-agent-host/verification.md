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
| PV4 / W1 交付前 / WP2 | 工作区 = W1（`crates/acp-protocol` 新增 + `[workspace] members` 加入该 crate），基线 `4b0145e` | WP2 的全部行为测试与代码质量：fixture/矩阵驱动契约、raw 保真、未知判别子、扩展方法、上限、capability 真实性 | 主 Agent（实现 Agent，WP2） | `cargo fmt --all -- --check`；`cargo clippy --locked -p acp-protocol --all-targets --all-features -- -D warnings`；`cargo test --locked -p acp-protocol --all-features`；`node scripts/check-crate-boundaries.mjs` | Windows x64、rustc 1.98.1、Node 24.19.0 | PASS（全部 exit 0）：测试 35 个全通过（capabilities 5、envelope 6、fixtures 2、matrix_tables 7（新增 `capability_paths_match_the_matrix`、`declared_capability_paths_follow_the_declaration`）、raw_fidelity 7、updates 8），无 0 用例文件、无跳过；`crate boundaries OK: 8 个 crate 的依赖方向与 §5 矩阵一致` | `openspec/changes/acp-boundary-and-agent-host/reports/wp2-acp-protocol-tests.log` |

| PV4 / W2 交付前 / WP3 | 工作区 = W2（`crates/agent-host` 新增 + members 加入该 crate），基线 `f2ca1f5` | 进程监督：stdio 分帧、request id 与 pending 注册表、固定超时（含 turn 不设超时）、stderr 有界、关闭顺序与失败路径、`cfg` 落点 | 主 Agent（实现 Agent，WP3） | `cargo fmt --all -- --check`；`cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings`；`cargo test --locked -p agent-host --all-features` | Windows x64、rustc 1.98.1、Node 24.19.0 | PASS（全部 exit 0）：26 个测试全通过（supervision 12、session 8、catalog 5、单元 1）；clippy 零告警 | `openspec/changes/acp-boundary-and-agent-host/reports/wp3-agent-host-supervision.log` |
| PV5 / W2 交付前 / WP3 | 同上 | 进程树回归：强制结束 Agent 后孙进程停止（父→孙两层 + 心跳文件） | 主 Agent（实现 Agent，WP3） | `cargo test --locked -p agent-host --all-features tree_ -- --nocapture`（**本机 Windows x64**） | Windows x64、rustc 1.98.1（Linux CI 不覆盖该平台分支，见下「未执行」） | PASS：`tree_forced_termination_stops_the_whole_process_tree ... ok`（1 passed, 0 failed, 11 filtered out） | `openspec/changes/acp-boundary-and-agent-host/reports/pv5-windows-tree.log` |
| PV4 / W3 交付前 / WP4 | 同上 | 会话映射与交互：`EndpointEvent` 组装与 `AcpRaw` 保真、结构化内容不文本化、turn 终态唯一、取消隔离、权限/elicitation 往返保真、能力门控 | 主 Agent（实现 Agent，WP4） | `cargo test --locked -p agent-host --all-features --test session` | 同上 | PASS（exit 0）：8 个测试全通过，含 raw 逐字节/sha256 断言与「回传 optionId 不保真则 fake child 以 `refusal` 结句」的反例护栏 | `openspec/changes/acp-boundary-and-agent-host/reports/wp4-agent-host-session.log` |
| PV4 / W4 交付前 / WP5 | 同上 | 目录/启动环境/空闲回收：查询不启动进程、凭据失败关闭、环境 = 白名单 ∩ 绑定、配置文件的同名条目不被使用、`idle_timeout=0` 不回收 | 主 Agent（实现 Agent，WP5） | `cargo test --locked -p agent-host --all-features --test catalog`；`node scripts/check-crate-boundaries.mjs`；`grep -rn 'cfg(' crates/agent-host/src` | 同上 | PASS（全部 exit 0）：5 个测试全通过；`crate boundaries OK: 8 个 crate`；`cfg` 命中仅 `platform.rs` 的 3 个平台分支 + `launch.rs` 的 `#[cfg(test)]` 单元测试模块（无平台分支外泄） | `openspec/changes/acp-boundary-and-agent-host/reports/wp5-agent-host-config.log` |
| DU1-PV1 / W6 集成前 / DU1 | 工作区 = WP2+WP3+WP4+WP5 全部落地 | 两个新 crate 与既有 6 个 crate 的工作区整体一致性（合同门禁 + 全工作区 fmt/clippy/test） | 主 Agent（集成） | `npm run verify`（= `npm run check` + `cargo fmt --all -- --check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` + `cargo test --locked --workspace --all-features`） | Windows x64、rustc 1.98.1、Node 24.19.0 | PASS（exit 0）：8 个 crate 的合同门禁全绿（含 `contract drift OK`、`crate boundaries OK: 8`）；全工作区测试全通过（含 agent-host 26 个） | `openspec/changes/acp-boundary-and-agent-host/reports/du1-main-verify.log` |
## Check Plan Changes

1. **Unix 进程组结束的依赖：`libc` → `nix`**（原：`design.md` D6 写「`libc` 是本变更唯一新增的 unix 依赖」；新：`nix` 0.30，`default-features = false`，features `signal`/`process`）。理由：workspace 固定 `unsafe_code = "forbid"`，而 `libc::killpg` 是 `unsafe` 外部函数，直接调用必须写 `unsafe` 块（`forbid` 无法用 `#[allow]` 绕过）；`nix` 提供安全封装，且其 MSRV 低于仓库 `rust-version = 1.85`。风险覆盖：Unix 进程组结束的语义不变（仍是 `killpg(SIGKILL)`），许可证仍为 MIT。受影响任务：2.17（`platform/unix`）、3.5/3.6（PV4/PV5 与 review）。
2. **适配器产出的 view 不含 `turnId`/`version`**（原：`docs/SYNC_PROTOCOL.md` §10.3 把 `turnId`/`version` 列为 `turn.*`、`session.mode.changed`、`session.config.changed` 等 view 的最低必填字段；新：`agent-host` 的 mapper 不产出这两个字段）。理由：`SessionEndpoint::prompt(request, at)` 的签名不携带 core 的 `TurnId`，会话 `version` 也由 core 掌握；适配器无法得知它们。core 侧真正消费的字段（`interactionId`、`messageId`/`deltaIndex`/`text`/`block`）已全部提供（`crates/core/src/broker.rs` 的 `view_interaction_id`/`delta_fragment`），因此本地路径功能不受影响；缺口只影响 Sync 切片的 view 字段级校验。风险覆盖：已登记为 `design.md` 的 Risk #10；收口方式（由 core 注入两个字段，或改端口签名）属于 **core 端口/契约变更**，需用户决策，不在本变更内做。受影响任务：2.21（mapper）、3.7/3.8（WP4 的 PV4 与 review），以及下游 Sync 切片。

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| WP3 | WP5（同变更内） | `LaunchSpec { program: String, args: Vec<String>, env: Vec<(String, String)> }`（`agent-host::launch`）——WP3 只消费它，不读 profile、不解析凭据；`Supervisor::start(spec)` 之后立即 `env_clear` 再逐项注入 | `4b0145e` | `crates/agent-host/tests/supervision.rs` 的 `spawned_child_receives_exactly_the_injected_environment` 与 `tests/catalog.rs` 的 `child_environment_is_exactly_the_launch_spec` 断言「子进程看到的变量集合 == 注入集合」 | `LaunchSpec` 字段变化 → WP3/WP5 复验 |
| WP3 | WP2 | WP2 冻结的 public API（`RawDocument`/`Envelope`/`AcpError`/`methods`/`content`/`update`/`message`/`capability`）+ 任务 3.3 的 PV4 证据（33 测试全通过） | `4b0145e`（W0 基线） | `cargo build -p acp-protocol` 通过；`agent-host` 只用 `acp-protocol` 的公开项 | `acp-protocol` 的 public API 变化 → WP3/WP4/WP5 复验 |

## Runtime Resources

- W0/W1 只使用默认 `target/`（无并行分片），未占用数据库/端口/外部账号；测试的临时目录尚未引入（WP3 起由 fake ACP child 用例自建并清理）。
- 本会话未安装额外工具；Windows 是本机宿主，因此 PV5 可在本机执行。

## Review Findings

3.2（WP1）、3.4（WP2）、3.6（WP3）、3.8（WP4）的独立 review 均**未执行**：本轮主 Agent 持续在实现上下文里工作，未创建隔离的 reviewer 上下文（见 Failures and Retests 的 `RV1-PENDING`）。
因此本文件目前只能支持「实现与检查通过的证据」，**不能**支持独立验证结论；最终验收（8.1）不得据此判定 PASS。

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
| FRAMING-1 / 2.15 | 测试 `unknown_response_id_is_a_protocol_error_without_breaking_others` 在 10 s 后超时 | 主 Agent（WP3） | 根因是**真实缺陷**：`read_frame` 在单次 `read` 返回多行时丢弃换行之后的字节（响应被静默丢弃）。重写为持久缓冲 + `drain(..=position)` 的逐帧处理 | 无需独立 review（实现期缺陷，已由新增回归用例固定） | `reports/wp3-agent-host-supervision.log`（12/12 通过）；复现用例即上条测试名 | 已解决 |
| ROUTE-1 / 2.22 | elicitation 用例收不到 `elicitation.requested` | 主 Agent（WP4） | 根因是**真实缺陷**：ACP 的 elicitation/权限请求不总带 `sessionId`，而路由器只按 `sessionId` 归因，导致请求被判为「未知会话」并拒绝。改为：带 `sessionId` 时按它归因；缺失时仅当本进程**恰好只有一个会话**才归因，多个会话时显式拒绝（不猜） | 无需独立 review | `reports/wp4-agent-host-session.log`（8/8 通过） | 已解决 |
| ENVCTL-1 / 2.26 | 测试 `spawned_child_receives_exactly_the_injected_environment` 首版断言方式无法证明「宿主变量不泄漏」 | 主 Agent（WP5） | 改为让 fake child 用 `--dump-env` 写快照，并断言**变量名集合 ⊆ 注入集合**（同时覆盖 `ACPR_*` 节点/设备密钥名不得出现） | 无需独立 review | `reports/wp5-agent-host-config.log`（5/5 通过） | 已解决 |
| RV1-PENDING / 3.2、3.4、3.6、3.8 | 本轮未创建隔离的 reviewer 上下文 | 主 Agent | 未执行，保持待办 | 无 | 无 | **未解决**：需在后续轮次用独立 reviewer 上下文执行并写入 `reports/rv1-wp{1..5}.md` |

## Final Assessment

```agentic-assessment
assessment_id: "round-1"
target_commit: ""
contract_digest: ""
result: BLOCKED
evidence: []
```

- Assessment ID / Time: round-1；本轮推进到 WP5 实现与 PV4/PV5/集成检查完成，未进入最终验收
- Target / Task: 待 8.1 填写（目标 = 最终 `refs/heads/main` 提交）
- CLI State: 实施中（尚未执行 `workflow check --stage final`）
- Audit / Evidence: 有效证据 = PV2/PV3（W0）、PV4（WP2/WP3/WP4/WP5）、PV5（本机 Windows）、DU1-PV1（全工作区 `npm run verify`）；日志见 `reports/*.log`
- Result / Open Issues: **BLOCKED** — 3.2/3.4/3.6/3.8 的独立 review（RV1）未执行；5.x/6.x 的合并与版本确认、7.x 的替代验证与 `[e2e-owned]` 条目、8.1 最终验收未执行；`turnId`/`version` 缺口（Check Plan Change 2）尚未收口
- Required Follow-up: 用隔离 reviewer 上下文补 3.2/3.4/3.6/3.8 → 5.x/6.x/7.x → 8.1；最终验收必须另行执行 `agentic-verify` 流程
