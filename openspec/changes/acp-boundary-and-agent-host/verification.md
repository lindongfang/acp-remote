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
| PV4 / W1 交付前 / WP2 | 工作区 = W1（`crates/acp-protocol` 新增 + `[workspace] members` 加入该 crate），基线 `4b0145e` | WP2 的全部行为测试与代码质量：fixture/矩阵驱动契约、raw 保真、未知判别子、扩展方法、上限、capability 真实性 | 主 Agent（实现 Agent，WP2） | `cargo fmt --all -- --check`；`cargo clippy --locked -p acp-protocol --all-targets --all-features -- -D warnings`；`cargo test --locked -p acp-protocol --all-features`；`node scripts/check-crate-boundaries.mjs` | Windows x64、rustc 1.98.1、Node 24.19.0 | PASS（全部 exit 0）：测试 35 个全通过（capabilities 5、envelope 6、fixtures 2、matrix_tables 7、raw_fidelity 7、updates 8），无 0 用例文件、无跳过；`crate boundaries OK: 8 个 crate 的依赖方向与 §5 矩阵一致` | `openspec/changes/acp-boundary-and-agent-host/reports/wp2-acp-protocol-tests.log` |
| PV4 / W2 交付前 / WP3 | 工作区 = W2（`crates/agent-host` 新增 + members 加入该 crate）+ RV1 修复轮，基线 `f2ca1f5` | 进程监督：stdio 分帧、request id 与 pending 注册表、固定超时（含 turn 不设超时）、stderr 有界与结构化计数日志、关闭顺序与失败路径、超限即结束 Agent、`cfg` 落点 | 主 Agent（实现 Agent，WP3） | `cargo fmt --all -- --check`；`cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings`；`cargo test --locked -p agent-host --all-features`；`cargo check --locked -p agent-host --target x86_64-unknown-linux-gnu --all-targets`（unix 分支编译核验） | Windows x64、rustc 1.98.1、Node 24.19.0（+ 新增的 linux-gnu target std） | PASS（全部 exit 0）：33 个测试全通过（supervision 15、session 11、catalog 6、单元 1）；clippy 零告警；Linux 目标 `cargo check` 通过（unix 分支可编译，但**运行**行为仍不在本机覆盖） | `openspec/changes/acp-boundary-and-agent-host/reports/wp3-agent-host-supervision.log` |
| PV5 / W2 交付前 / WP3 | 同上 | 进程树回归：强制结束 Agent 后孙进程停止（父→孙两层 + 心跳文件） | 主 Agent（实现 Agent，WP3） | `cargo test --locked -p agent-host --all-features tree_ -- --nocapture`（**本机 Windows x64**） | Windows x64、rustc 1.98.1（Linux CI 不覆盖该平台分支，见下「未执行」） | PASS：两条断言各自独立通过——`tree_forced_termination_stops_the_whole_process_tree ... ok`（友好关闭路径：EOF→grace→结束树）与 `tree_terminate_while_parent_alive_stops_parent_and_grandchild ... ok`（强制终止路径：父仍活着时结束整棵树，父退出且孙停止心跳）→ `2 passed, 0 failed, 12 filtered out` | `openspec/changes/acp-boundary-and-agent-host/reports/pv5-windows-tree.log` |
| PV4 / W3 交付前 / WP4 | 同上（含 RV1 修复轮） | 会话映射与交互：`EndpointEvent` 组装与 `AcpRaw` 保真、结构化内容不文本化（含未登记 content block）、turn 终态唯一（含进程崩溃路径）、取消隔离、权限/elicitation 往返保真、能力门控「拒绝时不发消息」 | 主 Agent（实现 Agent，WP4） | `cargo fmt --all -- --check`；`cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings`；`cargo test --locked -p agent-host --all-features --test session` | 同上 | PASS（全部 exit 0）：11 个测试全通过，含 raw 逐字节/sha256 断言、`turn.failed` 唯一性、未宣告能力时「子进程若收到配置写入就退出」的反证、未登记 content block 的 `block` 结构化断言 | `openspec/changes/acp-boundary-and-agent-host/reports/wp4-agent-host-session.log` |
| PV4 / W4 交付前 / WP5 | 同上（含 RV1 修复轮：白名单纵深防御、`cfg!` 下沉） | 目录/启动环境/空闲回收：查询不启动进程、凭据失败关闭、白名单外的凭据变量在 spawn 前失败、环境 = 白名单 ∩ 绑定、配置文件的同名条目不被使用、`idle_timeout=0` 不回收 | 主 Agent（实现 Agent，WP5） | `cargo fmt --all -- --check`；`cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings`；`cargo test --locked -p agent-host --all-features --test catalog`；`node scripts/check-crate-boundaries.mjs`；`grep -rn 'cfg(' crates/agent-host/src` | 同上 | PASS（全部 exit 0）：6 个测试全通过；`crate boundaries OK: 8 个 crate`；`cfg` 命中仅 `platform.rs` 的 3 个平台分支 + `launch.rs` 的 `#[cfg(test)]`（`host.rs` 的平台分支已下沉到 `platform::command_candidates`） | `openspec/changes/acp-boundary-and-agent-host/reports/wp5-agent-host-config.log` |
| DU1-PV1 / W6 集成前 / DU1 | 工作区 = WP2+WP3+WP4+WP5 全部落地 | 两个新 crate 与既有 6 个 crate 的工作区整体一致性（合同门禁 + 全工作区 fmt/clippy/test） | 主 Agent（集成） | `npm run verify`（= `npm run check` + `cargo fmt --all -- --check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` + `cargo test --locked --workspace --all-features`） | Windows x64、rustc 1.98.1、Node 24.19.0 | PASS（exit 0）：8 个 crate 的合同门禁全绿（含 `contract drift OK`、`crate boundaries OK: 8`）；全工作区测试全通过：359 条 `test ... ok`（含 agent-host 33 个） | `openspec/changes/acp-boundary-and-agent-host/reports/du1-main-verify.log` |
| PV3（候选版本）/ W5 后 | 工作区 = WP2–WP5 + RV1/RV2 修复轮 | 同一批 ACP 资产门禁在候选版本上重跑（`check:acp`、`check:schemas`），确认本变更没有悄悄放宽矩阵或夹具口径 | 主 Agent（集成） | `npm run check` 内的 `check:acp`/`check:schemas` | Windows x64、Node 24.19.0 | PASS（exit 0）：`ACP compatibility matrix OK: 25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows, schema validated by ajv)`；`17 schemas, 155 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared` | `openspec/changes/acp-boundary-and-agent-host/reports/du1-main-verify.log` |
## Check Plan Changes

1. **Unix 进程组结束的依赖：`libc` → `nix`**（原：`design.md` D6 写「`libc` 是本变更唯一新增的 unix 依赖」；新：`nix` 0.30，`default-features = false`，features `signal`/`process`）。理由：workspace 固定 `unsafe_code = "forbid"`，而 `libc::killpg` 是 `unsafe` 外部函数，直接调用必须写 `unsafe` 块（`forbid` 无法用 `#[allow]` 绕过）；`nix` 提供安全封装，且其 MSRV 低于仓库 `rust-version = 1.85`。风险覆盖：Unix 进程组结束的语义不变（仍是 `killpg(SIGKILL)`），许可证仍为 MIT。受影响任务：2.17（`platform/unix`）、3.5/3.6（PV4/PV5 与 review）。
2. **stderr 采集的结构化日志口径**（原：`MODULE_ARCHITECTURE.md` §4.5 与 `SECURITY_DESIGN.md` §12.2 写「有界采集并**输出为结构化日志**」；新：只输出**结构化计数**——丢弃字节数（限频 warn）与采集总字节数（EOF 时 info），stderr 内容本身永不进日志）。理由：stderr 可能夹带凭据或 prompt 片段，`SECURITY_DESIGN.md` 的日志脱敏要求优先于「把内容打出来」的便利；内容仍可按上限取回（`Supervisor::stderr_snapshot`），供组合根或诊断路径按需处理。风险覆盖：仍有界、仍可观测丢弃（不再静默）、仍不进入 ACP 通道。受影响任务：2.16、3.6。
3. **适配器产出的 view 不含 `turnId`/`version`**（原：`docs/SYNC_PROTOCOL.md` §10.3 把 `turnId`/`version` 列为 `turn.*`、`session.mode.changed`、`session.config.changed` 等 view 的最低必填字段；新：`agent-host` 的 mapper 不产出这两个字段）。理由：`SessionEndpoint::prompt(request, at)` 的签名不携带 core 的 `TurnId`，会话 `version` 也由 core 掌握；适配器无法得知它们。core 侧真正消费的字段（`interactionId`、`messageId`/`deltaIndex`/`text`/`block`）已全部提供（`crates/core/src/broker.rs` 的 `view_interaction_id`/`delta_fragment`），因此本地路径功能不受影响；缺口只影响 Sync 切片的 view 字段级校验。风险覆盖：已登记为 `design.md` 的 Risk #10；收口方式（由 core 注入两个字段，或改端口签名）属于 **core 端口/契约变更**，需用户决策，不在本变更内做。受影响任务：2.21（mapper）、3.7/3.8（WP4 的 PV4 与 review），以及下游 Sync 切片。

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| WP3 | WP5（同变更内） | `LaunchSpec { program: String, args: Vec<String>, env: Vec<(String, String)> }`（`agent-host::launch`）——WP3 只消费它，不读 profile、不解析凭据；`Supervisor::start(spec)` 之后立即 `env_clear` 再逐项注入 | `4b0145e` | `crates/agent-host/tests/supervision.rs` 的 `spawned_child_receives_exactly_the_injected_environment` 与 `tests/catalog.rs` 的 `child_environment_is_exactly_the_launch_spec` 断言「子进程看到的变量集合 == 注入集合」 | `LaunchSpec` 字段变化 → WP3/WP5 复验 |
| WP3 | WP2 | WP2 冻结的 public API（`RawDocument`/`Envelope`/`AcpError`/`methods`/`content`/`update`/`message`/`capability`）+ 任务 3.3/3.4 的 PV4 证据（35 测试全通过） | `4b0145e`（W0 基线） | `cargo build -p acp-protocol` 通过；`agent-host` 只用 `acp-protocol` 的公开项 | `acp-protocol` 的 public API 变化 → WP3/WP4/WP5 复验 |

## Runtime Resources

- W0/W1 只使用默认 `target/`（无并行分片），未占用数据库/端口/外部账号；测试的临时目录尚未引入（WP3 起由 fake ACP child 用例自建并清理）。
- 本会话未安装额外工具；Windows 是本机宿主，因此 PV5 可在本机执行。

## Review Findings

### RV1 第一轮（隔离 reviewer 上下文，只读检视 `4b0145e` → `64a7582`）

| Review ID | 任务 | 结论 | 主要发现 |
| --- | --- | --- | --- |
| `RV-WP1-round1` | 3.2（WP1 文档/矩阵口径） | FAIL | MAJOR：PV5 判据写「两条断言」而证据只有 1 个用例；Check Plan Change 1（`libc`→`nix`）只写在 verification.md，design/tasks/plan 仍写 `libc`。MINOR：文档头状态仍列 6 个 crate、§3.1 写「十二个」、`openspec/config.yaml` 仍称两个新 crate 未实现；§5 矩阵缺 `storage-sqlite`/`node-link-client` 列（既有缺口）；`win32job` 的 MSRV/许可证无核验留证；plan 声称「不改 Sync 契约」与 view 缺 `turnId`/`version` 不一致；`lib.rs` 的 `cfg` 判据措辞不成立 |
| `RV-WP2-round1` | 3.4（acp-protocol） | FAIL | MAJOR：`ContentBlock::Unknown` 带 `skip_serializing`，`mapper` 用 `unwrap_or(Value::Null)` 吞掉错误 → 未登记 content block 的 `block` 变成 `null`（静默降级），且无对应用例。MINOR：§8 错误类型名 `AcpCodecError` 与实际 `AcpError` 不符；测试以 `wireName`/`path` 而非矩阵 row id 对齐；`id_value` 会把字符串 id 的转义规范化（出站构造件，无保真承诺） |
| `RV-WP3-round1` | 3.6（进程监督/平台） | FAIL | BLOCKER：stdout 超限只 `break` 读循环——不结束进程、不收敛未完成请求、`is_running()` 仍为真 → 坏 runtime 被永久复用。MAJOR：stderr 只有内存环形缓冲，没有 `§4.5/§12.2` 要求的结构化日志与丢弃计数。MINOR：`wait_exit` 存在丢失唤醒窗口（白等 grace）；Unix `killpg` 可能在 pid 复用后误伤；spawn 后 `attach`/取管道失败会留下孤儿；`host.rs` 的 `cfg!(windows)` 落在 platform 之外 |
| `RV-WP4-round1` | 3.8（会话/目录/启动） | FAIL | MAJOR：`session/new` 缺 pinned schema 的必填 `cwd`/`mcpServers`；`set_config` 的 `value` 不匹配 schema 的 anyOf（布尔必须带 `type`）；`turn.failed`（崩溃路径）与 `set_mode`/未宣告能力路径无任何用例（fake child 的分支不可达）。MINOR：`resolve_launch` 不做白名单交集（只信任端口）；配置文件用例不可证伪；无会话 runtime 不被空闲回收；先登记交互后 emit（emit 失败会悬挂）；文档/计划仍写 `libc` 与不存在的模块名 |

### RV1 第一轮后的修复（同一 `verification.md` 记录，未重跑全量 review）

| 发现 | 处理 | 证据 |
| --- | --- | --- |
| WP3-F1（BLOCKER，stdout 超限） | 新增 `abort_agent`：超限（收帧中或单帧）时以 `AcpError::Oversize` 收敛全部未完成请求、标记退出、结束整棵进程树；新增回归 `oversize_frame_ends_the_agent_and_fails_pending_requests` | `reports/wp3-agent-host-supervision.log` |
| WP3-F2（MAJOR，stderr 日志） | stderr 采集改为结构化计数日志（丢弃字节数限频 warn + EOF 汇总 info），**内容永不进日志**（`SECURITY_DESIGN.md` §13.3）；这一取舍记入「Check Plan Changes 2」 | `reports/wp3-agent-host-supervision.log`、`crates/agent-host/src/process.rs` |
| WP2-F1（MAJOR，未知 content block） | `mapper` 对 `ContentBlock::Unknown` 直接用原始 payload 写入 `block`（不再经 serde 重新编码 → 不再变 `null`）；新增回归 `unknown_content_block_stays_structured` | `reports/wp4-agent-host-session.log` |
| WP45-1（MAJOR，请求形状） | 新增 `session_new_params`：`cwd` 必须来自 core 已解析的 workspace（否则显式拒绝），并补 `mcpServers: []`；`set_config` 改用 `config_value_boolean`/`config_value_id`，文本取值显式拒绝 | `reports/wp4-agent-host-session.log` |
| WP45-2 / WP45-3（MAJOR，用例缺口） | 新增 `turn_failed_is_reported_once_when_the_agent_crashes_mid_turn`；fake child 增加 `--no-modes`/`--no-config-options`/`--exit-on-config-write` 与 `unknown-content-block`/`huge-line` 场景，新增 `undeclared_capability_is_refused_without_sending_anything`（拒绝时进程若收到请求就退出 → 反证「一个字节都没发」） | `reports/wp4-agent-host-session.log` |
| WP45-4（MINOR，白名单纵深防御） | `resolve_launch` 增加白名单交集校验（越界即 `EnvNotAllowed` 失败关闭），新增 `credential_variable_outside_the_allowlist_is_refused_before_spawn` | `reports/wp5-agent-host-config.log` |
| WP45-6（MINOR，空闲回收） | `AgentRuntime` 增加进程级 `last_activity`：无活动会话的 runtime 也参与回收（协商/目录活动会刷新它） | `reports/wp5-agent-host-config.log` |
| WP45-7（MINOR，交互悬挂） | 权限/elicitation 改为「先产出事件再登记」；映射失败时直接向 Agent 回错误响应并**不**登记 | `reports/wp4-agent-host-session.log` |
| WP3-F3（MINOR，丢失唤醒） | `wait_exit` 改为「先登记 waiter 再判 `done`」（`notified().enable()`） | 代码：`crates/agent-host/src/process.rs` |
| WP3-F5（MINOR，孤儿进程） | spawn 之后 `attach`/取管道失败时先 `terminate + start_kill` 再返回错误 | 代码：`crates/agent-host/src/process.rs` |
| WP3-F6（MINOR，`cfg` 落点） | Windows 命令候选下沉为 `platform::command_candidates`，`host.rs` 不再有平台分支 | `reports/wp5-agent-host-config.log` 的 `cfg(` 扫描 |
| WP1-F1（MAJOR，PV5 判据） | 新增 `tree_terminate_while_parent_alive_stops_parent_and_grandchild`（强制终止路径），`tree_` 过滤现在真的跑两条断言 | `reports/pv5-windows-tree.log` |
| WP1-F2/F3/F5/F6、WP45-8、WP2-F2（文档漂移） | design/tasks/plan 的 `libc`→`nix` 与模块名对齐；`MODULE_ARCHITECTURE.md` 文档头状态、§3.1「十三个」、§8 错误类型名（`AcpError`/`HostError`）修正；`openspec/config.yaml` 的模块画像更新为 8 个成员；`win32job 2.0.3` 的 `license = "MIT OR Apache-2.0"`、未声明 `rust-version` 已从本地 registry 元数据核对并记入 §4.5 | `npm run check`（doc links/boundaries/drift 全绿） |

### RV2 复核轮（新隔离 reviewer 上下文，只读检视 `64a7582` → `e6b4dfa`）

| Review ID | 范围 | 结论 | 结果 |
| --- | --- | --- | --- |
| `RV2-agent-host` | WP3/WP4/WP5 的代码修复（第 1–7 项） | PASS | 逐条给出已解决的代码级依据与**可证伪性**判断（超限用例断言 `Oversize` + `!is_running()`；未宣告能力用例靠「fake child 收到配置写入就退出」反证「没发消息」；未登记 content block 的旧实现必然失败）。新发现 `RV2` 的 F1–F7 均为 P2 |
| `RV2-docs` | WP1 文档/口径、PV5 判据、门禁证据 | 复核后仍 FAIL（已按报告修完） | MAJOR：`nix` 口径未同步到 design/tasks/plan；stderr 的权威文档与变更规范仍写「内容进结构化日志」；规范里的 stderr 超限场景（R51–R53）无用例。MINOR：测试计数不一致、`AGENTS.md` 仍称两个 crate 未落地、`nix` 的 MSRV/license 无留证、PV3 无独立 Checks 行、plan 的模块名仍写 `supervisor.rs`/`interaction.rs`/`catalog.rs`/`credentials.rs`、`cfg` 措辞未把 `#[cfg(test)]` 例外说清 |

### RV2 后的修复

| 发现 | 处理 | 证据 |
| --- | --- | --- |
| `RV2`-F1（MAJOR，stderr 场景缺用例） | fake child 新增 `stderr-flood` 场景（192×4 KiB）；新增 `stderr_flood_is_bounded_and_counted`：断言 `dropped > 0`、快照 ≤ 256 KiB、仍保留尾部，且 stdout 通道不受影响（prompt 正常完成） | `reports/wp3-agent-host-supervision.log` |
| `RV2`-F2（MAJOR，合同漂移） | 同步三处文字：`docs/MODULE_ARCHITECTURE.md` §4.5、`docs/SECURITY_DESIGN.md` §12.2 的 stderr 行、变更规范 `specs/local-agent-host/spec.md` → 「有界采集 + 只记结构化计数（丢弃字节数/采集总字节数），内容不进日志、可按上限回取」 | `npm run check`（doc links / ACP 资产门禁全绿） |
| `RV2` 其余 MINOR | `design.md`/`tasks.md`/`plan.md` 的 `libc` 全部改为 `nix` 并标注原因；plan 的模块名改为实际文件；`AGENTS.md` 的「尚未落地」列表去掉两个新 crate；本文件测试计数改为实测值；新增 PV3 候选版本行；`nix 0.30.1` 的 `MIT OR Apache-2.0` 与 Unix 分支「只做编译核验」记入 §4.5 | `npm run check`、`reports/du1-main-verify.log` |

### 检视报告归档

- RV1 第一轮与 RV2 复核轮的报告已写入：`reports/rv1-wp1.md`（WP1 文档/矩阵）、`reports/rv1-wp2.md`（acp-protocol）、`reports/rv1-wp3.md`（进程监督/平台）、`reports/rv1-wp4.md`（会话/目录/启动）。
- 这些文件在版本控制内；`reports/*.log` 是原始命令输出，按仓库策略不入库（`.gitignore`）。

### 仍未处理（下一轮必须解决或由用户裁定）

- `RV-WP3-F4`（Unix `killpg` 的 pid 复用窗口）：当前实现按记录下来的 pgid 结束进程组，子进程已被回收时可能误伤复用同一 pid 的进程组。修法是保存 `pidfd` 或在 `exit.done` 时跳过 `killpg`——两者都改变平台实现细节，留待下一轮（当前记为**已接受风险**，仅影响 Unix 且需要极端时序）。
- `RV-WP2-F6`（出站 `id_value` 的字符串转义规范化）：只影响**我们构造**的响应 id（数字 id 路径不受影响），本 crate 的保真承诺只覆盖**收到**的文档；记为已接受。
- `RV-WP2-F3`（矩阵 row id 引用）：测试改为直接断言矩阵的声明式字段（`wireName`/`wireValue`/`path`/`status`/`delivery`），并以 `invariant_fixtures_exist` 覆盖不变量的夹具存在性；不再声称逐条引用 row id（与 2.11 的原文措辞有偏差，属已记录的偏离）。
- `RV-WP1-F4`（§5 矩阵缺 `storage-sqlite`/`node-link-client` 列）：既有缺口，属 App/Node Link 切片范围内，本变更不改。
- `RV-WP1-F7` / `RV-WP4`（`agent-host` 文件所有权表与 WP 写范围）：已把 `session.rs`/`host.rs`/`launch.rs` 的命名写进本文件与 plan，但 `host.rs` 同时含 WP3（运行时/路由）与 WP4/WP5（端口实现）内容——**写范围重叠**已实际发生（W0 段落说明由一把异步锁串行化），下一轮应更新该表的措辞。
- **未执行**：RV1 的**复核轮**（recheck）。本轮修复后没有再次派发隔离 reviewer，因此「修复有效」目前只有实现者证据，不构成独立结论；3.2/3.4/3.6/3.8 仍未最终关闭。

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
| RV1-1 / 3.2、3.4、3.6、3.8 | 第一轮独立 review（4 个隔离上下文，只读检视 `4b0145e`→`64a7582`）：WP1/WP2/WP3/WP4 全部 FAIL，含 1 个 BLOCKER 与 6 个 MAJOR | 主 Agent（修复）+ 独立 reviewer（发现） | 逐条修复（见「Review Findings」的修复表）：超限即结束 Agent、未知 content block 保结构、`session/new` 补齐必填字段、`set_config` 按 anyOf 形状、补 5 条回归用例、白名单纵深防御、进程级空闲回收、交互 emit/登记顺序、`wait_exit` 丢唤醒、spawn 后孤儿、`cfg!` 下沉、PV5 第二条断言、文档漂移 | reviewer 报告全文（结构见「Review Findings」表）；修复证据见 `reports/wp3-*.log`、`reports/wp4-*.log`、`reports/wp5-*.log`、`reports/pv5-windows-tree.log` | `reports/du1-main-verify.log`（`npm run verify` exit 0）+ 各 WP 的 PV4 日志 | **部分解决**：修复均已落地并有回归证据，但**缺 RV1 复核轮**；`RV-WP3-F4`、`RV-WP2-F6`、`RV-WP2-F3`、`RV-WP1-F4`、文件所有权表措辞记为未处理项 |

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
- Result / Open Issues: **BLOCKED** — RV1 第一轮已执行且 4 个 WP 全 FAIL，修复已落地但**缺复核轮**；5.x/6.x 的合并与版本确认、7.x 的替代验证与 `[e2e-owned]` 条目、8.1 最终验收未执行；`turnId`/`version` 缺口（Check Plan Change 3）与 `RV-WP3-F4` 等未处理项尚未收口
- Required Follow-up: 用隔离 reviewer 上下文补 3.2/3.4/3.6/3.8 → 5.x/6.x/7.x → 8.1；最终验收必须另行执行 `agentic-verify` 流程
