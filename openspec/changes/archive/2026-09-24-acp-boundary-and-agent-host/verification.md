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
- 文档（`docs/MODULE_ARCHITECTURE.md`、`README.md`、`docs/DEVELOPMENT_PLAN.md`、`AGENTS.md`）唯一写入者为 WP1；`crates/acp-protocol/**` 为 WP2；`crates/agent-host/{Cargo.toml,src/lib.rs,src/error.rs,src/limits.rs,src/process.rs,src/platform.rs,src/bin/acpr-fake-acp-agent.rs}` 与 `src/host.rs` 的运行时/路由部分为 WP3；`src/{session.rs,mapper.rs}` 与 `src/host.rs` 的 `SessionBackendFactory` 部分为 WP4；`src/{config.rs,launch.rs}`、`src/host.rs` 的 `AgentCatalog` 部分与 `tests/{catalog.rs,support/mod.rs}` 为 WP5。**`src/host.rs` 是三轨共享文件**，靠单一写入者按 W2→W3→W4 波次串行化（§4.5 的写范围说明与 `plan.md` 的所有权表同步登记该重叠）。
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
| PV4 / W2 交付前 / WP3 | 工作区 = W2（`crates/agent-host` 新增 + members 加入该 crate）+ RV1 与 RV3 修复轮，基线 `f2ca1f5` | 进程监督：stdio 分帧、request id 与 pending 注册表、固定超时（含 turn 不设超时）、stderr 有界与结构化计数日志、关闭顺序与失败路径、超限即结束 Agent、`cfg` 落点 | 主 Agent（实现 Agent，WP3） | `cargo fmt --all -- --check`；`cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings`；`cargo test --locked -p agent-host --all-features`；`cargo check --locked -p agent-host --target x86_64-unknown-linux-gnu --all-targets`（unix 分支编译核验） | Windows x64、rustc 1.98.1、Node 24.19.0（+ 新增的 linux-gnu target std） | PASS（全部 exit 0）：52 个测试全通过（supervision 15、session 13、catalog 21、单元 3；含 RV3/RV4 两批修复新增的 17 个）；clippy 零告警；Linux 目标 `cargo check` 通过（unix 分支可编译，但**运行**行为仍不在本机覆盖） | `openspec/changes/acp-boundary-and-agent-host/reports/wp3-agent-host-supervision.log` |
| PV5 / W2 交付前 / WP3 | 同上 | 进程树回归：强制结束 Agent 后孙进程停止（父→孙两层 + 心跳文件） | 主 Agent（实现 Agent，WP3） | `cargo test --locked -p agent-host --all-features tree_ -- --nocapture`（**本机 Windows x64**） | Windows x64、rustc 1.98.1（Linux CI 不覆盖该平台分支，见下「未执行」） | PASS：两条断言各自独立通过——`tree_forced_termination_stops_the_whole_process_tree ... ok`（友好关闭路径：EOF→grace→结束树）与 `tree_terminate_while_parent_alive_stops_parent_and_grandchild ... ok`（强制终止路径：父仍活着时结束整棵树，父退出且孙停止心跳）→ `2 passed, 0 failed, 13 filtered out`（supervision 共 15 个用例） | `openspec/changes/acp-boundary-and-agent-host/reports/pv5-windows-tree.log` |
| PV4 / W3 交付前 / WP4 | 同上（含 RV1 修复轮） | 会话映射与交互：`EndpointEvent` 组装与 `AcpRaw` 保真、结构化内容不文本化（含未登记 content block）、turn 终态唯一（含进程崩溃路径）、取消隔离、权限/elicitation 往返保真、能力门控「拒绝时不发消息」 | 主 Agent（实现 Agent，WP4） | `cargo fmt --all -- --check`；`cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings`；`cargo test --locked -p agent-host --all-features --test session` | 同上 | PASS（全部 exit 0）：13 个测试全通过，含 raw 逐字节/sha256 断言、`turn.failed` 唯一性、未宣告能力时「子进程若收到配置写入就退出」的反证、未登记 content block 的 `block` 结构化断言 | `openspec/changes/acp-boundary-and-agent-host/reports/wp4-agent-host-session.log` |
| PV4 / W4 交付前 / WP5 | 同上（含 RV1 修复轮：白名单纵深防御、`cfg!` 下沉） | 目录/启动环境/空闲回收：查询不启动进程、凭据失败关闭、白名单外的凭据变量在 spawn 前失败、环境 = 白名单 ∩ 绑定、配置文件的同名条目不被使用、`idle_timeout=0` 不回收 | 主 Agent（实现 Agent，WP5） | `cargo fmt --all -- --check`；`cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings`；`cargo test --locked -p agent-host --all-features --test catalog`；`node scripts/check-crate-boundaries.mjs`；`grep -rn 'cfg(' crates/agent-host/src` | 同上 | PASS（全部 exit 0）：21 个测试全通过；`crate boundaries OK: 8 个 crate`；`cfg` 命中仅 `platform.rs` 的 3 个平台分支 + `launch.rs` 的 `#[cfg(test)]`（`host.rs` 的平台分支已下沉到 `platform::command_candidates`） | `openspec/changes/acp-boundary-and-agent-host/reports/wp5-agent-host-config.log` |
| DU1-PV1 / W6 集成前 / DU1 | 工作区 = WP2+WP3+WP4+WP5 全部落地 | 两个新 crate 与既有 6 个 crate 的工作区整体一致性（合同门禁 + 全工作区 fmt/clippy/test） | 主 Agent（集成） | `npm run verify`（= `npm run check` + `cargo fmt --all -- --check` + `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` + `cargo test --locked --workspace --all-features`） | Windows x64、rustc 1.98.1、Node 24.19.0 | PASS（exit 0）：8 个 crate 的合同门禁全绿（含 `contract drift OK`、`crate boundaries OK: 8`）；全工作区测试全通过：378 条 `test ... ok`（含 agent-host 52 个） | `openspec/changes/acp-boundary-and-agent-host/reports/du1-main-verify.log` |
| PV3（候选版本）/ W5 后 | 工作区 = WP2–WP5 + RV1/RV2 修复轮 | 同一批 ACP 资产门禁在候选版本上重跑（`check:acp`、`check:schemas`），确认本变更没有悄悄放宽矩阵或夹具口径 | 主 Agent（集成） | `npm run check` 内的 `check:acp`/`check:schemas` | Windows x64、Node 24.19.0 | PASS（exit 0）：`ACP compatibility matrix OK: 25 methods, 11 updates, 5 content blocks, 3 tool content types, 19 capabilities, 8 invariants, 10 test families (71 rows, schema validated by ajv)`；`17 schemas, 155 fixture files, 12 transcript vectors re-encoded from input, 20 negative vectors rejected as declared` | `openspec/changes/acp-boundary-and-agent-host/reports/du1-main-verify.log` |
## Check Plan Changes

1. **Unix 进程组结束的依赖：`libc` → `nix`**（原：`design.md` D6 写「`libc` 是本变更唯一新增的 unix 依赖」；新：`nix` 0.30，`default-features = false`，features `signal`/`process`）。理由：workspace 固定 `unsafe_code = "forbid"`，而 `libc::killpg` 是 `unsafe` 外部函数，直接调用必须写 `unsafe` 块（`forbid` 无法用 `#[allow]` 绕过）；`nix` 提供安全封装，且其 MSRV 低于仓库 `rust-version = 1.85`。风险覆盖：Unix 进程组结束的语义不变（仍是 `killpg(SIGKILL)`），许可证仍为 MIT。受影响任务：2.17（`platform.rs` 的 `#[cfg(unix)]` 分支）、3.5/3.6（PV4/PV5 与 review）。
2. **stderr 采集的结构化日志口径**（原：`MODULE_ARCHITECTURE.md` §4.5 与 `SECURITY_DESIGN.md` §12.2 写「有界采集并**输出为结构化日志**」；新：只输出**结构化计数**——丢弃字节数（限频 warn）与采集总字节数（EOF 时 info），stderr 内容本身永不进日志）。理由：stderr 可能夹带凭据或 prompt 片段，`SECURITY_DESIGN.md` 的日志脱敏要求优先于「把内容打出来」的便利；内容仍可按上限取回（`Supervisor::stderr_snapshot`），供组合根或诊断路径按需处理。风险覆盖：仍有界、仍可观测丢弃（不再静默）、仍不进入 ACP 通道。受影响任务：2.16、3.6。
3. **适配器产出的 view 不含 `turnId`/`version`**（原：`docs/SYNC_PROTOCOL.md` §10.3 把 `turnId`/`version` 列为 `turn.*`、`session.mode.changed`、`session.config.changed` 等 view 的最低必填字段；新：`agent-host` 的 mapper 不产出这两个字段）。理由：`SessionEndpoint::prompt(request, at)` 的签名不携带 core 的 `TurnId`，会话 `version` 也由 core 掌握；适配器无法得知它们。core 侧真正消费的字段（`interactionId`、`messageId`/`deltaIndex`/`text`/`block`）已全部提供（`crates/core/src/broker.rs` 的 `view_interaction_id`/`delta_fragment`），因此本地路径功能不受影响；缺口只影响 Sync 切片的 view 字段级校验。风险覆盖：已登记为 `design.md` 的 Risk #10；收口方式（由 core 注入两个字段，或改端口签名）属于 **core 端口/契约变更**，需用户决策，不在本变更内做。受影响任务：2.21（mapper）、3.7/3.8（WP4 的 PV4 与 review），以及下游 Sync 切片。

4. **delta spec 的两处场景措辞修正**（原：1）空闲回收场景写「目录条目转为不可用」；2）profile 来源场景写「配置文件中的条目不被读取为 profile 来源」；新：1）改为「回收不改变目录可用性（仍只由命令解析 + 凭据解析决定），既有会话映射不再被复用、后续 `open` 必须显式失败」；2）改为「profile 只来自注入的配置端口（本 crate 不读任何配置文件），且目录查询确实经由该端口取 profile」）。理由：原措辞 1 与同一规范的可用性 Requirement（启动前提不满足才标记不可用）及 core `AgentDescriptor` 的形状（无非运行态字段）自相矛盾——回收是资源管理而非可用性撤回；原措辞 2 在本层无可证伪接缝。修正后两条子句均有实现、注释与用例（`catalog.rs` 的回收/`open` 失败/可用性不变/按需新代、端口调用计数），并由 RV4 复核者独立确认「修正正确、与实现一致、不掩盖缺陷」。风险覆盖：回收后的会话重连行为从「静默复活死端点」变为「显式失败」——这是收紧而非放宽；受影响任务：2.25–2.27、3.9/3.10（WP5 的 PV4 与 review）。

## Dependency Handoffs

| Downstream | Upstream | Accepted Revision / Evidence | Start Revision | Transfer / Inclusion Check | Invalidation |
| --- | --- | --- | --- | --- | --- |
| WP3 | WP5（同变更内） | `LaunchSpec { program: String, args: Vec<String>, env: Vec<(String, String)> }`（`agent-host::launch`）——WP3 只消费它，不读 profile、不解析凭据；`Supervisor::start(spec)` 之后立即 `env_clear` 再逐项注入 | `4b0145e` | `crates/agent-host/tests/supervision.rs` 的 `spawned_child_receives_exactly_the_injected_environment` 与 `tests/catalog.rs` 的 `child_environment_is_exactly_the_launch_spec` 断言「子进程看到的变量集合 == 注入集合」 | `LaunchSpec` 字段变化 → WP3/WP5 复验 |
| WP4 / WP5 | WP3 | WP3 冻结的接口：`LaunchSpec { program, args, env }`（`agent-host::launch` 产出、`Supervisor::start` 消费）、`Supervisor` 的请求/通知/响应方法与 `stderr_snapshot`、内部 `ProcessTree`（`platform.rs` 内按 `#[cfg]` 分模块，`prepare` 每次启动各一份） | `4b0145e` | 任务 1.4 的包含关系检查：下游（WP4 的 `AcpSession`/`Endpoint`、WP5 的 `resolve_launch`）只经这些接口接入上游，未复用 supervisor 内部状态；`tests/supervision.rs` 与 `tests/catalog.rs`/`tests/session.rs` 分别以「子进程看到的变量集合 == 注入集合」「stderr 有界与结构化计数」「按 Agent 结束整棵树」断言该边界 | `LaunchSpec`/`Supervisor`/`ProcessTree` 的签名或语义变化 → WP4/WP5 复验 |
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
| WP3-F2（MAJOR，stderr 日志） | stderr 采集改为结构化计数日志（丢弃字节数限频 warn + EOF 汇总 info），**内容永不进日志**（`SECURITY_DESIGN.md` §14.1）；这一取舍记入「Check Plan Changes 2」 | `reports/wp3-agent-host-supervision.log`、`crates/agent-host/src/process.rs` |
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
| `RV2` 其余 MINOR | `design.md`/`tasks.md`/`plan.md` 的 `libc` 全部改为 `nix` 并标注原因；plan 的模块名改为实际文件；`AGENTS.md` 的「尚未落地」列表去掉两个新 crate；本文件测试计数改为实测值；新增 PV3 候选版本行；`nix 0.30.1` 的 `MIT`（已核实本地 registry 元数据；原写 `MIT OR Apache-2.0` 属事实错误）与 Unix 分支「只做编译核验」记入 §4.5 | `npm run check`、`reports/du1-main-verify.log` |

### 检视报告归档

- RV1 第一轮与 RV2 复核轮的报告已写入：`reports/rv1-wp1.md`（WP1 文档/矩阵）、`reports/rv1-wp2.md`（acp-protocol）、`reports/rv1-wp3.md`（进程监督/平台）、`reports/rv1-wp4.md`（会话/目录/启动）。
- 这些文件在版本控制内；`reports/*.log` 是原始命令输出，按仓库策略不入库（`.gitignore`）。

### 用户裁定与后续调整（2026-09-24）

用户在验收本分支时逐项裁定（原文：「1. 同意 2. 同意调整 4. 先本地合并」）：

1. **`turnId`/`version` 缺口的收口方式（Check Plan Change 3）**：同意采用 **core 侧收口**——由 core 在生成/提交事件时把 `turnId`（以及会话 `version`）注入 view，而不是改 `SessionEndpoint` 端口签名。理由：后者要求 broker 在调用 `prompt` **之前**分配 `TurnId`，会动事务边界（`CORE_PORTS_AND_STORAGE.md` §3.1/§6）；前者是纯附加字段，不改端口契约、对 owned 与 imported 路径一致。该实现**不在本变更内**（本变更的实现与证据已冻结），应在下一变更里带独立检视与回归收口。
2. **未处理项的处置**：同意按下述方式处理。
4. **先本地合并**：同意先在本地把本分支合并到 `main`（不推送），PR/CI 门禁另行安排。

### 目标基线与引用核实（任务 6.1，2026-09-24）

| 项 | 值 |
| --- | --- |
| 目标仓库 | `origin` = `git@github.com:lindongfang/acp-remote.git`（`git remote -v`） |
| 目标分支 | `refs/heads/main`（本地 `main`），远端 `refs/remotes/origin/HEAD` → `refs/remotes/origin/main` |
| 核实命令与输出 | `git rev-parse refs/heads/main` → `4c24fbd665738b7bbb34311d525896caa0215e89`；`git rev-parse refs/remotes/origin/main` → `094009b31f32c42928c01f2943eb7cf6e15c6154` |
| 本单元基线 / 候选 | 基线 `094009b`（= 合入前 `main`，也是 `origin/main`）；候选 `4c24fbd`（= 合入后 `main`） |
| 工作区 | 主 Agent 工作区 `D:/Project/acp-remote`（`git worktree list` 已核对）；集成 Agent 另用隔离开关 worktree |

结论：目标与引用可确认，非 BLOCKED。**已知偏差（如实记录）**：6.6 的合入在 6.3/6.4/6.5 之前就按用户旨意（「先本地合并」）完成，因此候选提交与主分支提交是同一个提交（`4c24fbd`）而非先后两个版本；6.3 与 6.7 的检查因此落在完全相同的树上（同 SHA、同工作区内容），证据可显式复用，但会分别记录两轮执行。

### 本地合并记录（2026-09-24）

按第 4 项裁定执行：`feat/acp-boundary-and-agent-host` → **fast-forward** 合入本地 `main`（`094009b` → `6f1515a`，7 个提交），未推送、未开 PR，`origin/main` 仍在 `094009b`。合并后 `npm run check` exit 0。完整证据（合并前后位置、合入提交清单、未随合并完成的事项）见 `reports/du1-integration.md`。

本轮按第 2 项完成的调整（证据见 `reports/wp3-agent-host-supervision.log` 与 `reports/wp4-agent-host-session.log`）：

| 项 | 调整 |
| --- | --- |
| RV2-F1（出站请求形状无断言） | fake child 校验收到的请求形状：`session/new` 必须带 `cwd`+`mcpServers`、`session/set_mode` 必须带 `sessionId`+`modeId`、`session/set_config_option` 的 `value` 必须匹配 anyOf（否则 `exit(8)`）；新增成功路径用例 `declared_capabilities_accept_mode_and_config_writes`，并同步修正监督层用例的 `session/new` 参数 |
| RV2-F3（stderr 结束日志少报丢弃） | `stderr_loop` 的汇总改读环形缓冲的最终 `dropped`（限频只影响 warn 播报节奏） |
| RV2-F5（无会话 runtime 回收无用例） | 新增 `idle_reclaim_also_applies_to_processes_without_sessions`（协商后无会话 → 零值不回收、超时即回收） |
| 其余未处理项 | 保持登记：`RV-WP3-F4`（Unix pid 复用窗口，接受风险）、`RV-WP2-F3/F6`（矩阵 row id 措辞/出站 id 规范化，已记录偏离与接受）、`RV-WP1-F4`（§5 矩阵缺列，下一切片）、文件所有权表措辞与 RV1-F7 的写范围重叠（随 §4.5 措辞一同处理） |

### RV-DU1 候选与合并检视（任务 6.4 / 6.8，2026-09-24）

- 执行方式：**新的**独立只读上下文 + `bash`（`oracle` agent，run `9e0ae308…`；Review ID `RV-DU1-round1`），检视候选 `e4a4492`（基线 `094009b`，区间 58 文件 +13371/−49）；报告归档于 `reports/rv1-du1.md`。
- **首次派发的同类上下文（run `84759cd6…`）在一条阻塞的 `bash` 调用上卡死（>240 s、steer 无法送达），已由主 Agent 中断并按失败登记**；替换上下文在任务里加了「禁止分页器、命令 60 s 未返回即放弃、不得跑 cargo/npm 重型命令」的硬约束后正常完成。这是本变更内唯一一次执行级失败，不涉及产品代码。
- 复核者实跑（均只读）：`git --no-pager rev-parse/merge-base/log/diff`（含 `--is-ancestor`）、`node scripts/check-crate-boundaries.mjs`（exit 0）、`node scripts/check-doc-links.mjs`（exit 0）、`Cargo.lock` 依赖对集合差分（212 → 232，`comm -23` 为空 ⇒ 纯增量）、`grep` 审计（`#[ignore]`、`cfg(`、`tracing::`、`let _ =`）。**明确未执行** `cargo` 任何构建/测试与 `npm run check`（按调度约束），相关结论只以带出处的既有日志作引用。
- 结论：**6.4 PASS / 6.8 PASS**。6.4 的五个问题（`acp-protocol`↔`agent-host` 接口一致性、`LaunchSpec` 边界、`Cargo.toml`/§5 矩阵/变更声明三处口径、是否有「只为让测试通过」的痕迹、既有 crate 公开契约）逐条通过，无 CRITICAL/MAJOR；`crates/core/**`、`crates/storage-sqlite/**`、`crates/acpr-*`、两个既有协议 crate **零文件改动**，`Cargo.lock` 纯增量。6.8 判定：合并是 fast-forward（`--is-ancestor` exit 0、`--merges` 为空、全部单父），候选之后**无任何代码差异**（仅 `docs/` 与变更目录的文档/证据提交），故 RV1–RV5 对 `e4a4492` 的判定可直接复用，无需为合并差异另开检视轮。
- 该轮列出 4 条 MINOR（F1 权威文档「与它的 `bin/`」与「`bin/` 零命中」自相矛盾、F2 `tasks.md` 2.12 的依赖面与 `Cargo.toml` 漂移、F3 agent→client 方法分派是第二份字面量清单且无相等断言、F4 三处终态事件在构造失败时静默丢弃）+ 1 条 SUGGESTION（F5 两处 `unwrap_or`/`.ok()` 降级）+ 1 条已登记缺口（F6 `TurnAccepted.turn` 占位 id）。
- 处置：**F1/F2 已在本轮修复**（权威文档措辞改为「平台分支只存在于 `platform.rs`，`bin/` 不含平台分支」；`tasks.md` 2.12 依赖面写实为 `tokio(process/io-util/macros/rt)` + `async-trait`/`serde_json`/`thiserror`/`tracing`/`sha2`/`base64` + `win32job`/`nix`，并同步 `Cargo.toml` 的 tokio 注释）。**F3/F4/F5 不构成阻断**（复核者判定：F3 当前集合实核一致且 registry 由 `tests/matrix_tables.rs` 逐行比对；F4 可达性≈0；F5 为自产 JSON 的自配对路径），按复核者建议登记为「接线 `app` 之前的收口项」，见「仍未处理」。
### RV5 复核轮与 3.2/3.10 关闭（2026-09-24）

- 执行方式：**新的**独立只读上下文 + `bash`（`oracle` agent，run `3cbb9840…`），复核修复提交 `e4a4492`（基线 `d0fa731`），范围同时覆盖 WP1 与 WP5 的条目；报告归档于 `reports/rv1-wp5.md` 的「RV5 复核轮」段（`reports/rv1-wp1.md` 留指针）。
- 复核者**没有接受实现者的自述**：亲手做了 4 条变异自检（改坏 → 用例变红 → `git checkout --` 还原 → 再跑变绿），并对「stderr 有界脱敏」「`§13.3`」「`TerminateJobObject`」「幻影 `platform/windows`/`platform/unix`」「`cfg` 判据」「许可证」做了六组全仓 grep（含 `git show e4a4492:<path>` 复核）。实跑：`cargo fmt`（0）、`cargo clippy`（0）、`cargo test -p agent-host`（52 passed，与日志一致）、`npm run check`（0，十道门禁）；并独立用 `cargo metadata` 核验 `tracing 0.1.44 = MIT`、`nix 0.30.1 = MIT`、`win32job 2.0.3 = MIT OR Apache-2.0`。
- 结论：**无 CRITICAL/MAJOR，无未解决阻断项**；`3.2` 与 `3.10` 的完成条件（报告写入 `reports/rv1-*.md`、无未解决阻断项）由此满足并勾选。
- 该轮另列 6 条 MINOR/SUGGESTION（全部为静态文档事实，不涉及行为/协议/安全/用例有效性）：`plan.md` 第二份所有权清单的幻影文件名、`verification.md:54` 的 `platform/unix` 幻影路径、`design.md` 两处绝对 `cfg` 判据、spec 的「并给出原因」无落点、登记叙述部分不实、`SECURITY_DESIGN.md` 括注串味。这 6 条已在 RV5 之后由主 Agent 修复（`plan.md` 实名化、`platform.rs` 的 `#[cfg(unix)]` 措辞、`design.md` 改为「平台分支 `cfg` 只落这里」、spec 把「原因」落到 `create`/`agent_capabilities` 的错误类别、登记叙述与两份清单对齐、括注改为「只记总字节数与丢弃字节数」），`npm run check` 复核 exit 0。
- **如实记录**：这 6 条修复是在 RV5 之后做的**文档级**改动，**未再经过一轮独立检视**（RV5 的结论针对 `e4a4492`）；它们没有任何行为/协议/安全含义，且均由 `npm run check` 与静态 grep 可复核，主 Agent 以其自证并对最终验收负责。
- RV5 同时确认：`Q4-4`（跨 await 持锁）、`Q4-3` 残余、`Q3-2`（`UnknownProfile` 无用例）、`RV4-WP5-F2`（期望集只查 ⊇）仍为可接受的登记项（不构成阻断），但要求在**接线 `app` 之前**收口；该绑定已写入「仍未处理」。

### RV4 复核轮（3.2 / 3.10 的复核轮，2026-09-24）

- 执行方式：两个**新隔离只读上下文 + `bash`**（`oracle` agent，run `5a389f60…`、`326c2b80…`），分则复核 WP1（文档/矩阵）与 WP5（配置/凭据/目录/回收），对象 `d0fa731`（基线 `4c24fbd`）。本轮刻意改用能自行跑命令的只读 agent，以补上 RV3「无 bash/git」造成的能力缺口：`git diff`、`cargo fmt/clippy/test`、`npm run check` 与门禁由复核者亲自执行。
- 复核者实跑（均 exit 0）：`cargo fmt --all -- --check`；`cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings`（含 `cargo clean -p agent-host` 后的冷编译复跑）；`cargo test --locked -p agent-host --all-features` = **45 passed / 0 failed**，catalog ×5 与整包 ×4 重复运行无抖动；`node scripts/check-crate-boundaries.mjs`；`npm run check`（含 `check:agentic` 6/6）。
- 结论：**两轮均判定「无未解决阻断项」**。RV3 的 2 个 MAJOR（回收不破坏运行时/映射、spec 可用性子句）确认闭合；WP1 的 8 条中 4 条完整闭合、3 条部分闭合、1 条（stderr 场景可证伪性）未闭合也未登记。复核者明确：结论为「无阻断」**不等于可归档**，且不代替 Project Verify（工作区 371 条、Linux 目标、E2E）。
- 复核者独立确认我为 spec 做的两处措辞修正**正确、与实现一致、不掩盖缺陷**（含与同一规范 §可用性判定的自洽性、core `AgentDescriptor` 形状、以及修正后两条子句均有用例）。
- 登记条目可接受性（复核者独立判断，非默认接受）：`RV-WP1-F4`（§5 缺列）**可接受**（已在权威文档表下披露 + 门禁硬失败兼底）；Q4-3 残余与 Q4-4 **可接受**（均为显式失败/短暂双树，现实不可达且已绑定「接线 `app` 前收口」）；`Q3-2` **可接受但偏低优先**；`Q5`（配置文件名场景归属）**可接受且不需移交**；`RV4-WP1-F3`（登记叙述不实）与 `RV4-WP1-F2`（未登记）**不可接受** → 已在本轮修复。
- 本轮新发现（已下发修复批次，见 `RV3-2`）：WP1 侧 `tracing` 许可证事实错误、stderr 场景可证伪化、登记叙述/残留口径；WP5 侧 4 个 MINOR（期望集合校验顺序无用例、`set_mode` 的 `touch` 无法被钉住、`shutting_down` 检查在锁外、已关闭会话阻塞回收）+ 6 个 SUGGESTION（`args` 未脱敏、`ACPR_` 约定未登记、stale runtime 分支无用例、关闭后缺进程外证据、spec「并给出原因」无落点、期望集合只查 ⊇）。

### RV3 检视轮（任务 3.2 / 3.10，2026-09-24）

- 执行方式：两个**新隔离只读 reviewer 上下文**（`reviewer` agent，run `4ff355c8…`、`c37e8abe…`），分别复核 WP1（文档/依赖矩阵）与 WP5（配置/凭据/目录/回收），检视对象 `4c24fbd`；报告归档于 `reports/rv1-wp1.md`（RV3 段）与 `reports/rv1-wp5.md`。
- **能力边界（必须与结论一起读）**：该上下文只有只读文件工具、**没有 bash/git**，两份报告均为静态阅读 + 契约比对，**未执行任何命令**；依赖矩阵门禁与 PV 由主 Agent 代跑（见 `Checks` 表）。因无 `git`，两份报告都未核对 `094009b..4c24fbd` 的提交区间 diff，只检阅磁盘当前内容（当时工作树无未提交的 crate 改动，故等价于候选提交内容）。
- 结论：**无 BLOCKER**。
  - WP1：6 个 MINOR —— `nix` 许可证事实错误（实为 `MIT`，文档写 `MIT OR Apache-2.0`，且与本文件另一处自相矛盾）、stderr 残留措辞 3 处、stderr 场景无可失败断言、`process.rs` 注释小节指针错（§13.3 → §14.1）、`AGENTS.md` §9「尚未落地」名单未同步、§5 缺列只在变更目录登记（权威文档与门禁注释均未披露）、文件所有权/cfg 判据措辞与实际文件不符。
  - WP5：2 个 MAJOR —— ①`sweep_idle` 只 `close_session`+`shutdown`，不从 map 移除 runtime 也不清映射，`open()` 会把已关闭的 supervisor 重新标记为活跃并返回端点（与 delta spec「既有会话映射不再被复用」直接冲突；当前工作树内因 broker 缓存端点而不可达，接线 `app` 后即可达）；②spec 的「目录条目转为不可用」既未实现、也无用例、也未登记为未处理。另有 MINOR/P2：缺「白名单 ∩ 绑定」期望集合比对（漏注入不可见）、`ACPR_` 无静态拒绝、`LaunchSpec` 公开 `Debug` 会带明文凭据、目录查询注释不实、空闲时钟不含 `set_mode`/`set_config`/`cancel`/`resolve` 等活动、`runtime_running` 的 `try_lock` 假通过与只证明 `closing` 置位、`profile_selection_ignores_startup_configuration_files` 不可证伪且属 RV1 被静默丢下的发现、`open`/`shutdown_agent`/`spawn_idle_sweep`/`UnknownProfile` 无覆盖、回收/关闭与启动不互斥（3 种交错）、`ensure_runtime` 跨 await 持锁。
- 处置：全部 MAJOR 与可落地 MINOR 作为修复批次下发实现 Agent（独立上下文，见「Failures and Retests」的 `RV3-1`），修复落地于 `d0fa731`，随后由**新的**独立复核轮（RV4，改用带 `bash` 的只读 `oracle` 上下文，以便自行跑 `git diff` 与门禁）复核 3.2/3.10。
- **规范措辞修正**（本变更 delta spec `specs/local-agent-host/spec.md`，属检视者给出的「二选一」中的说明路径）：
  1. 空闲回收场景：删除「目录条目转为不可用」，改为「可用性只由命令解析 + 凭据解析决定，回收不改变可用性；既有会话映射不再被复用，后续 `open` 必须显式失败」。理由：回收是资源管理，不应让 Agent 从目录消失；原措辞与 `AgentDescriptor.available` 的既有语义（可解析性）冲突。
  2. profile 来源场景：明确 profile 只来自注入配置端口、本 crate 不读任何配置文件，使该场景可证伪（配套断言配置端口确实被调用）。

### 仍未处理（下一轮必须解决或由用户裁定）

- `RV-WP3-F4`（Unix `killpg` 的 pid 复用窗口）：当前实现按记录下来的 pgid 结束进程组，子进程已被回收时可能误伤复用同一 pid 的进程组。修法是保存 `pidfd` 或在 `exit.done` 时跳过 `killpg`——两者都改变平台实现细节，留待下一轮（当前记为**已接受风险**，仅影响 Unix 且需要极端时序）。
- `RV-WP2-F6`（出站 `id_value` 的字符串转义规范化）：只影响**我们构造**的响应 id（数字 id 路径不受影响），本 crate 的保真承诺只覆盖**收到**的文档；记为已接受。
- `RV-WP2-F3`（矩阵 row id 引用）：测试改为直接断言矩阵的声明式字段（`wireName`/`wireValue`/`path`/`status`/`delivery`），并以 `invariant_fixtures_exist` 覆盖不变量的夹具存在性；不再声称逐条引用 row id（与 2.11 的原文措辞有偏差，属已记录的偏离）。
- `RV-WP1-F4`（§5 矩阵缺 `storage-sqlite`/`node-link-client` 列）：既有缺口，属 App/Node Link 切片范围内，本变更不改。
- `RV-WP1-F7` / `RV-WP4`（`agent-host` 文件所有权表与 WP 写范围）：本文件与 `plan.md` 的两份所有权清单（WP 表、单一写入者条）均已实名化，`host.rs` 同时含 WP3（运行时/路由）与 WP4/WP5（端口实现）内容——**写范围重叠**已实际发生（W0 段落说明由一把异步锁串行化），下一轮应更新该表的措辞。
- **RV3-Q4-4**（`ensure_runtime` 跨 `await` 持 `runtimes` 锁：`resolve_launch`/`Supervisor::start`/`initialize`/崩溃恢复都在锁内）：属**活性/吞吐**缺陷（一个 agent 启动慢会串行阻塞其它 agent 的启动与回收），功能正确性未破。修法（per-agent 初始化锁或 `OnceCell`）留待接线 `app` 前与 `RV3-Q4-3` 的残余交错一起收口；本变更内先把「死 runtime 被复用」与「关闭中启动新进程」两类显式失败收敛掉。
- **RV3-Q3-2**（`UnknownProfile` 分支无用例）：与目录/启动路径同批收口。
- **RV3-Q4-6 残余**：`runtime_running` 在锁被占用时返回 `false`，与「未运行」不可区分；已要求回收用例补进程外可观察断言（心跳文件停止）。
- **RV3-Q5**（`profile_selection_ignores_startup_configuration_files` 的覆盖归属）：已按可证伪方向调整（断言配置端口确实被调用）；若复核认为仍无真实接缝，则应把该场景移交 `app` 切片用组合根的真实接缝验证，并在本变更登记。
- **RV4-WP5-F2**（期望集合只验证「⊇」）：端口返回「白名单内但未绑定」的额外名会被放行（白名单本是上限而非等式）。复核者自评**非安全越界**（仍在白名单内）且当前 fake 造不出反例，登记不改。
- **RV4-WP5-F8**（spec `spec.md:9/18-19` 的「并给出原因」在本层无落点）：`AgentDescriptor` 无 `reason` 字段，本层可观察的「原因」只有 `create`/`agent_capabilities` 的错误类别（已有用例断言 `Unavailable(KeystoreUnavailable)`）。复核者判定不构成 FAIL，登记为「措辞待与端口形状对齐」。
- **RV4-WP5-F5 残余**：`ACPR_` 保留前缀已补写进 `docs/SECURITY_DESIGN.md` §12.2；若后续引入新的保留前缀，必须同步该处与 `core::model` 的 `is_reserved_env_name`。
- **RV-DU1-F3**（agent→client 方法分派是 `session.rs` 里的第二份字面量清单，未走 `acp_protocol::methods::status_of`/`direction_of`）：当前实核一致（registry 中 `AgentToClient ∧ implemented` 恰好是 `session/update`、`session/request_permission`、`elicitation/create` 三条），且 registry 本身由 `tests/matrix_tables.rs` 逐行比对，故不阻断；复核者建议补一条「agent-host 处理集合 == `Implemented ∩ AgentToClient`」的相等断言，登记为下一轮（属防漂移加固，非缺陷）。
- **RV-DU1-F4**（三处终态事件构造失败时静默丢弃：`session.rs` 的 turn 终态、进程退出终态、`session.mode.changed`）：可达性≈0（`mapper::turn_event` 的 `EventType` 是固定字面量、view 极小），但「终态恰好一次」是硬不变量，next round 应在 `Err` 分支补 `tracing::warn`（不改成 panic）。
- **RV-DU1-F5**（`session.rs` 的用户作答解码 `.ok()` 与 `mapper.rs` 的 `to_value(...).unwrap_or(Value::Null)` 会把失败降级成合法值）：自产 JSON 不会自我拒绝，实践不可达；下一轮改为显式错误或至少 `warn`。
- **RV-DU1-F6**（`TurnAccepted.turn` 是适配器本地占位 id）：core 侧 `broker.rs` 为 `Ok(_) => Ok(true)`，不读取该值，故不构成接口破坏；已作为 Check Plan Change 3 登记并获用户裁定走 core 侧收口。

## Merge History

| 阶段 | 内容 | 证据 |
| --- | --- | --- |
| 5.1 交接 | `worker` 子 Agent（fresh 独立上下文，未参与实现/检视）；交接 `openspec/schemas/agentic/roles/integrator.md` **全文** + 交付单元/模式/固定版本/目标分支/**无合并授权**；独立 worktree `D:/Project/acpr-du1-worktree` | 本变更 `reports/du1-integration.md`「独立集成 Agent 交接记录（任务 5.1）」 |
| 5.2 就绪 | mode = `integrated`；WP 组成 WP1–WP5 + RV1–RV4 修复批次；候选 `e4a4492`；各 WP 的 PV 与独立检视证据的有效性逐项核对 | 同文件「就绪复核（任务 5.2）」 |
| 6.1 基线核实 | `git rev-parse refs/heads/main` = `4c24fbd665738b7bbb34311d525896caa0215e89`（合入前）；`refs/remotes/origin/main` = `094009b31f32c42928c01f2943eb7cf6e15c6154` | 本文件「目标基线与引用核实（任务 6.1）」 |
| 6.2 候选构建 | 基线 `094009b` → 候选 `e4a4492`（区间 10 个提交、0 合并提交、58 文件 +13371/−49）；在仓库外 worktree 内 `cargo build --locked --workspace --all-features` = **exit 0**（`Finished dev profile in 15.28s`、155 条 `Compiling`、0 warning/0 error）；`check-crate-boundaries` exit 0；无集成冲突（fast-forward 链） | 集成 Agent 交接返回（结论已归档到本文件与本变更 `reports/du1-integration.md`） |
| 6.3 PV1（候选） | `npm run verify` = **exit 0**（十道门禁逐道 exit 0 + fmt + clippy + 378 passed/2 ignored） | `reports/du1-pv1.log`（937 行，含逐子项汇总表） |
| 6.4 候选独立检视 | 待 `du1-review` 返回（`acp-protocol`↔`agent-host` 接口一致性、`LaunchSpec` 边界、`Cargo.toml`/§5 矩阵/文档三者一致、有无「为让测试通过」的痕迹） | `reports/rv1-du1.md` |
| 6.5 E2E 判定 | `not-applicable` 路径与四项替代检查安排复核通过；无任务依赖必须运行的 E2E | 本变更 `reports/du1-integration.md`「E2E 判定复核（任务 6.5）」 |
| 6.6 合入（**顺序偏差，已登记**） | 用户裁定「先本地合并」后由主 Agent 在**本地**执行 fast-forward：`094009b` → `6f1515a`（7 个提交：W0 文档冻结、`acp-protocol`、`agent-host`、RV1/RV2 修复与归档、RV2 补齐），随后继续在 `main` 上叠加 `4c24fbd`（本地合并与裁定记录）、`d0fa731`（RV3 修复）、`e4a4492`（RV4 修复）——即 `094009b..e4a4492` 共 **10 个提交**、0 个合并提交（线性）；此后为文档/证据提交（`1035395`→`582133e`→…）。**未推送、未开 PR**，`origin/main` 仍为 `094009b`。偏差：6.6 发生在 6.3/6.4/6.5 **之前**（用户旨意），因此候选与主分支是同一提交，6.3/6.7 的证据落在完全相同的树上 | 本文件「目标基线与引用核实（任务 6.1）」与「本地合并记录」 |
| 6.7 主分支复跑 | 同 SHA 适用性成立（候选 == 主分支 == `e4a4492`）；`npm run check` 复跑 **exit 0**，workspace **378 passed / 2 ignored**（provenance 已标注） | `reports/du1-main-verify.log` |
| 6.8 合并新增差异 | 待 `du1-review` 返回（预期「无新增差异」，依据 = 候选与主分支同 SHA 的 fast-forward 合入） | `reports/rv1-du1.md` 的 6.8 段 |

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
| RV3-1 / 3.2、3.10 | RV3 独立检视（2 个新隔离 reviewer 上下文，检视 `4c24fbd`）：无 BLOCKER，但 WP5 有 2 个 MAJOR（`sweep_idle` 不破坏运行时与映射 → `open()` 复活死端点；spec 的「目录条目转为不可用」未实现/未用例/未登记）与一批 MINOR/P2，WP1 有 6 个 MINOR（含 `nix` 许可证事实错误） | 主 Agent（修复批次下发）+ 独立实现 Agent（独立上下文执行）+ 独立 reviewer（发现） | 修复批次 `d0fa731`：回收同临界区内移出 runtime 与清映射、`open()` 校验运行时仍在运行、`ensure_runtime` 弃用已停止运行时、`shutting_down` 标志；空闲时钟覆盖出站活动；期望集合比对与 `ACPR_`/`ACP_REMOTE_` 静态拒绝；`LaunchSpec` 手写 `Debug`；回收用例改用心跳文件做进程外断言；删掉 4 条恒真 `ACPR_*` 断言；配置文件名场景改为断言端口被调用；文档口径 8 项（nix 许可证、stderr 措辞、AGENTS §9、§5 披露、门禁注释、plan 实名与 `cfg` 判据）；delta spec 两个场景措辞修正 | `reports/rv1-wp1.md`（RV3 段）、`reports/rv1-wp5.md`、RV4 复核报告（待写） | `reports/wp3-agent-host-supervision.log`（45 测试 + fmt/clippy/Linux 编译）、`wp4-agent-host-session.log`（12）、`wp5-agent-host-config.log`（15）、`reports/pv5-windows-tree.log`（2 条进程树断言）、`reports/du1-main-verify.log`（371 测试 + 10 门禁）；反向探针：注释掉 `sweep_idle` 的移除与短路两个静态拒绝后对应用例双红 | **待 RV4 复核**（MAJOR 已按上述修复，MINOR 或修复或登记；4-3/4-4 残余与 `Q3-2` 登记为未处理） |
| RV3-2 / 3.2、3.10 | RV4 复核轮（2 个新隔离只读 + `bash` 上下文，复核 `d0fa731`）：两轮均「无未解决阻断项」，但新增一批 MINOR/SUGGESTION（含 1 条被判定「未修复且未登记」的 stderr 场景可证伪性） | 主 Agent（修复批次下发）+ 独立实现 Agent + 独立 reviewer（发现） | 修复批次：期望集合校验顺序用例、`set_mode` 单点 `touch` 钉住、`shutting_down` 锁内复查、已关闭会话不阻塞回收、`Debug` 不打印 `args`、stale runtime 分支用例、回收/关闭改进程外证据、新增 `stderr-protocol-noise` 场景 + 「stderr 合法报文不被解析」用例、`ACPR_` 约定写进 `SECURITY_DESIGN` §12.2、残留口径清理 | `reports/rv1-wp1.md` / `reports/rv1-wp5.md` 的 RV4 段、RV5 复核报告（待写） | 待本批门禁与新增用例作证（见 `Checks` 表的 PV4 行） | **待 RV5 复核** |
| RV1-1 / 3.2、3.4、3.6、3.8 | 第一轮独立 review（4 个隔离上下文，只读检视 `4b0145e`→`64a7582`）：WP1/WP2/WP3/WP4 全部 FAIL，含 1 个 BLOCKER 与 6 个 MAJOR | 主 Agent（修复）+ 独立 reviewer（发现） | 逐条修复（见「Review Findings」的修复表）：超限即结束 Agent、未知 content block 保结构、`session/new` 补齐必填字段、`set_config` 按 anyOf 形状、补 5 条回归用例、白名单纵深防御、进程级空闲回收、交互 emit/登记顺序、`wait_exit` 丢唤醒、spawn 后孤儿、`cfg!` 下沉、PV5 第二条断言、文档漂移 | reviewer 报告全文（结构见「Review Findings」表）；修复证据见 `reports/wp3-*.log`、`reports/wp4-*.log`、`reports/wp5-*.log`、`reports/pv5-windows-tree.log` | `reports/du1-main-verify.log`（`npm run verify` exit 0）+ 各 WP 的 PV4 日志 | **部分解决**：修复均已落地并有回归证据，但**缺 RV1 复核轮**；`RV-WP3-F4`、`RV-WP2-F6`、`RV-WP2-F3`、`RV-WP1-F4`、文件所有权表措辞记为未处理项 |

## Final Assessment

- Assessment ID / Time: `round-2-final` / 2026-09-24（本会话）/ 执行者：主 Agent
  - 版本推进与复验：本记录落盘提交为 `467f6ec9167467f712d066757c8243a4f0b8a3fa`（只含 `verification.md` 与 `tasks.md` 的勾选，无代码/证据改动）。按 acceptance 的「版本变化需重新评估」要求，该提交后已把 `target_commit` 刷新为 `467f6ec…` 并在该提交的干净 detached worktree 中重跑 `workflow check --stage final --planning-root <本仓库>` → **PASS**；因此 `target_commit` 指向的即包含本记录的那次提交，刷新后的指针按设计保留在规划根工作区（未提交：提交它会再次移动目标，自引用不可满足）。
- Target / Task: 本地主分支 `refs/heads/main` = `b36088f8f64927413c25bce655ac69f9728affbd`（`git rev-parse` 核实；`workflow check --stage final` 的 `targetCommit` 与之一致，且 `HEAD == refs/heads/main`）；被验收的代码实现为 `e4a4492`（此后仅文档/证据提交，RV-DU1 已核实零代码差异）；最终验收任务 ID = `8.1`（`[final-verification]`）。E2E mode = `not-applicable`，批准记录可追溯（`plan.md` 的 `downgrade_approval` = 用户原话「1. 同意 2. 同意」，2026-09-24）。
- CLI State: `npx --quiet --no-install openspec status --change acp-boundary-and-agent-host`（2026-09-24，任务勾选前查询）= 三个规划 artifact 全部 `[x]`（`All planning artifacts complete!`）；`openspec-agentic e2e check --json` = **PASS**（`enabled: true`、`mode: not-applicable`、`approval: true`、`marked: true`，并按 `[e2e-owned]` 自动勾选 7.3）。CLI 状态仅如实引用原始输出，不用本验收结论改写其含义。
- Audit / Evidence: 六个审计组逐组核对通过——**Contracts and Coverage**（plan 的 Coverage Index 逐项有任务/检查/证据；`checks` 门禁断言 §5 矩阵 8 个已登记 crate 与 `Cargo.toml` 一致；4 条 Check Plan Change 均登记理由、受影响任务与风险覆盖）；**Delivery and Versions**（审计组引用的 Merge History/交接/Spec 版本证据见本文件对应节：5.1 交接记录（独立集成 Agent，fresh 上下文，交接 `roles/integrator.md` 全文，无合并授权）、6.2 候选构建 exit 0、6.6 本地 fast-forward 合入并登记顺序偏差、6.7 同 SHA 复跑）；**Project Checks and Resources**（`reports/du1-pv1.log` = `npm run verify` exit 0，十道门禁逐道 exit 0 + fmt + clippy + 378 passed/2 ignored；`reports/du1-main-verify.log` 为同 SHA 复跑；Runtime Resources 节记录资源与隔离；并如实记录 2 条既有 `#[ignore]` 属其它 crate 且带理由）；**Independent Reviews**（RV1 4 个 WP 全 FAIL → 修复；RV2 代码 PASS/文档 FAIL → 修复；RV3 无 BLOCKER、WP5 2 MAJOR → 修复；RV4 两轮无阻断 + 新批次 → 修复；RV5 无阻断 + 6 条文档事实残留 → 已修复；RV-DU1 6.4/6.8 PASS，F1/F2 已修复、F3/F4/F5/F6 有处置结论；所有 CRITICAL/MAJOR 均有复核闭环）；**E2E Design and Execution**（`not-applicable` + 四项替代检查在固定版本 `1035395` 逐项通过，见 `reports/alt-final-verification.md`）；**Issue Closure and Evidence Validity**（`Failures and Retests` 的 WP2-BUILD-1/WP2-TEST-1/FRAMING-1/ROUTE-1/ENVCTL-1/RV1-1/RV3-1/RV3-2 均已解决或有复核结论；未解决项均为非阻断并逐条给出处置与绑定条件）。有效证据记录 ID 与摘要见上面的 `agentic-assessment`（13 条，已按当前文件内容逐一核对 sha256）；失效/复用判断：候选与主分支同 SHA 使 PV1 证据可复用，文档提交不改代码故 RV 结论仍适用，RV3/RV4/RV5/RV-DU1 的 review ID 与适用版本均按原报告引用。
- Result / Open Issues: **PASS**（本轮目标 `b36088f8…` 及上述有效证据）。未解决的非阻断项（全部登记于「仍未处理」，且均绑定「接线 `app` 之前收口」）：`RV-WP3-F4`（Unix pid 复用窗口，接受风险）、`RV-WP2-F6`/`RV-WP2-F3`、`RV-WP1-F4`（§5 缺列）、`RV3-Q4-3` 残余交错、`RV3-Q4-4`（跨 await 持锁，活性）、`RV3-Q3-2`、`RV3-Q4-6` 残余、`RV-DU1-F3/F4/F5/F6`、以及 `Check Plan Change 3` 的 core 侧 `turnId`/`version` 收口（已获用户裁定，留待下一变更）。**未在本机执行**：Linux 上 `#[cfg(unix)]` 的运行行为（仅编译核验）、`cargo-deny` 与 `gitleaks`（仅 CI）、真实 Codex/OMP 兼容套件。
- Required Follow-up: ①无（本轮目标与有效证据下验收 PASS）；②归档前用 `--stage archive` 复跑（须全部任务完成）；③任何新的提交（含把本验收记录提交）都会移动 `refs/heads/main`，使 `target_commit` 失效，需按 acceptance 的「版本变化重新评估」刷新结论后再归档；④下一变更：core 侧注入 `turnId`/`version`，并连同上述「接线 `app` 之前」项一起收口。

```agentic-assessment
assessment_id: "round-2-final"
target_commit: "467f6ec9167467f712d066757c8243a4f0b8a3fa"
contract_digest: "sha256:e23ad72ebfa95151a2f572d269a3f6f81251f04783ff8df5b3d5e68a94d73e4a"
result: PASS
evidence:
  - path: reports/wp2-acp-protocol-tests.log
    sha256: "sha256:32c39b5d5c7d785e15d3fea6705ac8a02d27e4caccf90e17ce2d9c03ce5d0bac"
  - path: reports/wp1-acp-assets.log
    sha256: "sha256:be43b9b10852da4957427b09322aec931368ce1707d85724eef8031b90672b74"
  - path: reports/wp5-agent-host-config.log
    sha256: "sha256:deffa94c376e7d445bc36b8b61f3faa714febe7257c052de4ba608352321bb20"
  - path: reports/wp4-agent-host-session.log
    sha256: "sha256:4fe460b8d0ce833d86fcd6a7e5e227b77c502722a6894d7d233665b69cb7858d"
  - path: reports/wp3-agent-host-supervision.log
    sha256: "sha256:734758a0f778a2cd5232d3a62f1feb183d1a32f1e814587a7150d5f381a43da5"
  - path: reports/pv5-windows-tree.log
    sha256: "sha256:edc257f62be7a6696c3d4381ca1dd7979da3d471ca54a892ee5f8ac7a3c7a79e"
  - path: reports/du1-pv1.log
    sha256: "sha256:0ceb0c04cf0fec4cca4f7906191cbf634b962f0172b6f9260796a622bdadd04e"
  - path: reports/du1-main-verify.log
    sha256: "sha256:b09fa3a4411576d224fa39ca4c5123148f36c3c8572823d3328d3d7abf9b6b2f"
  - path: reports/alt-final-verification.md
    sha256: "sha256:1c42c772f7da7be57250b298d989cb365a6c9ab5b6b8d390d9ce89a42881ac12"
  - path: reports/rv1-du1.md
    sha256: "sha256:2389d67ebe100844ad29c933ab150d596d20ae33d94c22b8e00bcbde2a4ad46b"
  - path: reports/rv1-wp5.md
    sha256: "sha256:a578e13b8f69cd303e0deb00e32a1c016e1a6feed3cca9ad8264b0bb4fa248a5"
  - path: reports/rv1-wp1.md
    sha256: "sha256:e046024b17b1f26e8b5949929f0bc5db84cf9b756d370395c324a57ca5a7264b"
  - path: reports/du1-integration.md
    sha256: "sha256:3e5b6d01dc9f675a868076de063a6d5a2726b7bb7c2f0a00feb6e8d56bf73fd5"
```

- Assessment ID / Time: round-1；本轮推进到 WP5 实现与 PV4/PV5/集成检查完成，未进入最终验收
- Target / Task: 待 8.1 填写（目标 = 最终 `refs/heads/main` 提交）
- CLI State: 实施中（尚未执行 `workflow check --stage final`）
- Audit / Evidence: 有效证据 = PV2/PV3（W0）、PV4（WP2/WP3/WP4/WP5）、PV5（本机 Windows）、DU1-PV1（全工作区 `npm run verify`）；日志见 `reports/*.log`
- Result / Open Issues: **BLOCKED** — RV1 第一轮已执行且 4 个 WP 全 FAIL，修复已落地但**缺复核轮**；5.x/6.x 的合并与版本确认、7.x 的替代验证与 `[e2e-owned]` 条目、8.1 最终验收未执行；`turnId`/`version` 缺口（Check Plan Change 3）与 `RV-WP3-F4` 等未处理项尚未收口
- Required Follow-up: 用隔离 reviewer 上下文补 3.2/3.4/3.6/3.8 → 5.x/6.x/7.x → 8.1；最终验收必须另行执行 `agentic-verify` 流程
