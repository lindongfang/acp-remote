## 1. Dependency and Resource Setup

- [x] 1.1 在新的任务级最小上下文中核实仓库路径、目标引用（`git rev-parse refs/heads/main`）、工具链（`rust-toolchain.toml` 与 Node ≥ 22.12）、`npm run verify` 的实际子检查清单、`变更校验` 的 `enabled`/`command` 实测值，以及本机是否存在可用的 Windows x64 环境（PV5 前提）。
- [x] 1.2 确认实现所需契约与文件所有权（§5 矩阵新增列、§4.5 收口措辞、`Cargo.toml` 的 members 与依赖登记、两个 crate 的目录归属），记录编码起点与 `Cargo.toml` 的分波次串行写入顺序（W0 WP1 → W1 WP2 → W2 WP3）。
- [x] 1.3 确认唯一共享运行资源是构建目录 `target/`、临时心跳目录与子进程，确定并行分片时 `CARGO_TARGET_DIR` 的隔离方式、心跳目录的清理方法，以及 PV5 必须在本机 Windows 执行（Linux CI 不可复现）的事实；完成条件：隔离与释放方案记录在案，并写明 PV5 的平台限制。
- [x] 1.4 [WP4 / WP5] 在 WP4/WP5 开始前接入已验收上游（WP3 的 `LaunchSpec`、supervisor 句柄与 `ProcessTree` 接口），核对下游实际基线与包含关系。

## 2. Implementation

- [x] 2.1 在 `docs/MODULE_ARCHITECTURE.md` §5 的矩阵新增 `agent-host` 列（`app` 行允许依赖 `agent-host`），并把 §4.5 的 Job 粒度收口为「每个 Agent 一个由 Daemon 侧 supervisor 持有的 Job」（保留 `KILL_ON_JOB_CLOSE`、Daemon 持有句柄、父→孙清理三条约束），同时更新 §3 的 crate 树、§3.1 的依赖口径与 §4.2/§4.5 的已落地状态；完成条件：`node scripts/check-crate-boundaries.mjs` 仍退出 0（此时两个 crate 尚未加入 members），且文档无将来时表述。
- [x] 2.2 在 `Cargo.toml` 的 `[workspace.dependencies]` 登记 `tracing`、`win32job`（2.x）、`nix` 的版本与用途注释（原计划写 `libc`；不在此任务写 `members`；完成条件：`cargo metadata --no-deps --locked` 退出 0，依赖注释与实际口径一致。
- [x] 2.3 把 `acp-protocol`、`agent-host` 从两处状态表的「待落地」移入本轮的已落地行（`README.md` 的「仓库当前状态」表、`AGENTS.md` §4 的模块状态表），并在 `docs/DEVELOPMENT_PLAN.md` §2 记录本轮基线变化；完成条件：三处表述一致且不含未实现的能力宣称。
- [x] 2.4 执行 `node scripts/check-crate-boundaries.mjs` 与 `node scripts/check-acp-compatibility.mjs` + `node scripts/check-schema-fixtures.mjs`，并核对 `fixtures/acp/v1/` 与 `compatibility/acp/v1/matrix.json` 未被改动（逐字节）
- [x] 2.5 创建 `crates/acp-protocol`（`Cargo.toml` 只依赖 `serde`/`serde_json`/`thiserror`，不含任何工作区 crate）、`src/lib.rs` 模块骨架与 `src/limits.rs`（1 MiB 单条消息上限等固定 v1 常量），并把 `crates/acp-protocol` 写入 `[workspace] members`；完成条件：`cargo build -p acp-protocol --all-features` 通过，`check:boundaries` 对该行无工作区依赖违规。
- [x] 2.6 实现 JSON-RPC 信封分类（`Request`/`Notification`/`Response`）、方向校验与规范 required 字段校验，错误分类为 `Malformed`/`WrongDirection`（与「能力不支持」可区分），不做字段默认值补齐；完成条件：局部测试覆盖合法请求/通知/响应、缺 required 字段、方向错误三类，全部通过。
- [x] 2.7 实现 `RawDocument`：保存原始 UTF-8 JSON document 文本（不含 stdio 换行）与路由字段（类别、`method`、`id` 的原文片段与类型判别，支持字符串 id），再编码一律原样回写原文；`serde_json::Value` 只用于读字段，不作为保真路径；完成条件：局部测试证明「原文 → 解析 → 回写」逐字节相等，且字符串型 `id` 与超大整数字面量原样保留。
- [x] 2.8 实现 ACP v1 DTO：`initialize` 与 capability 声明（保留未识别能力字段）、`session/new`、`session/prompt`、`session/update` 的 11 种已知判别子、prompt/output content block（text/image/audio/resource_link/resource）、`session/request_permission` 与 elicitation；结构化内容按类型解码，不得合并为普通文本；未实现的可选能力不得以默认值或空对象表达为已支持；完成条件：局部测试覆盖每种判别子与每种 content block 的解码，并以 `tool-call-with-diff.json` 断言 tool call 与 diff 保持结构化。
- [x] 2.9 实现未知 `sessionUpdate` 判别子的可见降级（保留逐字节原文、可被上层取得、不是解码失败）与 `_` 前缀扩展方法的显式不支持（方法未找到等价错误，不转发）；完成条件：对 `fixtures/acp/v1/future-session-update.json` 与 `extension-method.json` 的局部测试通过。
- [x] 2.10 实现单条消息的 1 MiB 解析上限（超限返回 `Oversize` 且不保留部分结果、不缓存为正文）与有界处理约束（不缓存完整会话正文）；完成条件：局部测试覆盖刚好在限内与超限两类，超限路径不产生部分解码结果。
- [x] 2.11 实现由 `fixtures/acp/v1/manifest.json` 与 `compatibility/acp/v1/matrix.json` 驱动的契约测试：逐条执行 manifest 的 `valid`/`invalid` 用例（不跳过）、以原始字节比较 `invariant.unknown_fields_byte_exact` 与 `invariant.meta_fields_byte_exact`、断言 `invariant.future_update_visible` 与 `invariant.extension_method_explicit_unsupported`，并在测试中引用矩阵 row id；不得复制第二套样例
- [x] 2.12 创建 `crates/agent-host`（`Cargo.toml`：`core`、`acp-protocol`、`tokio`（features `process`/`io-util`/`macros`/`rt`；`sync`/`time` 由 workspace 级 feature 提供）、`async-trait`、`serde_json`、`thiserror`、`tracing`、`sha2`、`base64`，`cfg(windows)` 的 `win32job`、`cfg(unix)` 的 `nix`）、`src/lib.rs` 模块骨架与 `src/limits.rs`（`SECURITY_DESIGN.md` §12.2 的固定常量：启动 10 s、短请求 30 s、关闭 grace 5 s、1 MiB、stderr 256 KiB，turn 不设超时），并把 `crates/agent-host` 写入 `[workspace] members`；完成条件：`cargo build -p agent-host --all-features` 在两个目标平台上均可编译（Linux CI 覆盖 unix 分支），`check:boundaries` 通过。
- [x] 2.13 实现 `crates/agent-host/src/bin/acpr-fake-acp-agent.rs`：按 argv 的 `--scenario` 选择行为（`normal`、`chunked-updates`、`permission-request`、`elicitation`、`slow-initialize`、`no-response`、`illegal-json`、`unknown-id`、`out-of-order`、`crash-on-prompt`、`spawn-grandchild`、`heartbeat-child`、`unknown-content-block`、`huge-line`、`stderr-flood`、`stderr-protocol-noise`），`spawn-grandchild` 自身 re-exec 为 `--scenario heartbeat-child` 并按 argv 给定的文件每 50 ms 追加心跳；场景选择只走 argv（不放宽 `env_allowlist`）；完成条件：测试用 `env!("CARGO_BIN_EXE_acpr-fake-acp-agent")` 能启动并完成一次 `initialize`。
- [x] 2.14 实现 `LaunchSpec` 的消费侧与 supervisor 的 spawn：参数数组直传、不经 shell、stdio 全 piped、Windows 上 spawn 后立即（写 stdin 之前）把进程加入 Job、Unix 上 spawn 前设置进程组；`LaunchSpec` 由 WP5 产出，本任务只定义并对接该接口。
- [x] 2.15 实现 stdout 的 LF 分帧（超 1 MiB 立即以 `Oversize` 结束该 Agent）、request id 单调分配与 pending 注册表（oneshot 唤醒，天然支持乱序）、未知 id 与非法 JSON 记为明确协议错误且不影响其它 pending；完成条件：用 `out-of-order`、`unknown-id`、`illegal-json` 三个场景的局部测试证明各条结论。
- [x] 2.16 实现固定超时（`initialize` 10 s、短请求 30 s、**`session/prompt` 不设超时**）与 stderr 的 256 KiB 环形缓冲（超限丢最旧并记丢弃计数，只输出结构化计数、内容不进日志，绝不作为 ACP wire 解析）；完成条件：`slow-initialize`、`no-response` 与 stderr 超限三类局部测试通过，且断言 turn 不会因超时被杀。
- [x] 2.17 在单个 `src/platform.rs` 内用 `#[cfg(windows)]`/`#[cfg(unix)]` 模块实现 `ProcessTree` 的两个平台后端（Windows 用 `win32job` 的 `Job::create_with_limit_info` + `limit_kill_on_job_close` + `set_extended_limit_info` + `assign_process`，**结束手段是关闭/丢弃该 Job 的句柄**（`KILL_ON_JOB_CLOSE`；该 wrapper 未封装 `TerminateJobObject`，直接 FFI 被 `unsafe_code = "forbid"` 禁止）；Unix 用 `process_group(0)` + `nix::sys::signal::killpg`），并把**平台分支** `cfg` 限制在 `platform.rs` 内；完成条件：两平台均可编译，`rg "cfg\(" crates/agent-host/src` 的**平台分支**只落在 `platform.rs`（`launch.rs` 只有 1 处 `#[cfg(test)]`、`bin/` 零命中）。
- [x] 2.18 实现进程树常驻回归测试：用 `spawn-grandchild` 造出父→孙两层与心跳文件，断言「强制结束 Agent 后孙进程停止，心跳字节数不再增长」与「关闭 Job 句柄后父与孙一并停止」；完成条件：`cargo test -p agent-host --all-features tree_` 通过
- [x] 2.19 实现 supervisor 的关闭顺序与所有权：停止接受新请求 → 未完成请求以明确错误结束 → 关闭 stdin → 等 5 s grace → 结束进程树 → join 全部子任务；正常路径无 `unwrap()`/`expect()`，无 detached task；完成条件：局部测试覆盖「仍有未完成请求时关闭」与「异常退出后关闭」，并断言无遗留后台任务。
- [x] 2.20 实现 `SessionBackendFactory` 与 `SessionEndpoint`：`create` 用 core 给出的 `SessionId` 建立唯一映射（重复 create 返回冲突类错误）、`open` 只接受已存在的映射、`reference()` 返回 core 给出的引用、live endpoint generation 在重连/重开时正确推进；完成条件：局部测试覆盖三条映射结论与 generation 行为。
- [x] 2.21 实现 `mapper`：把 ACP 通知映射为 `EndpointEvent { kind, event_type, payload: EventPayload { view, acp }, turn: None, causation, at }`，`AcpRaw::available("application/json", 不含换行的原文, sha256)`，结构化内容保持判别子与工具调用标识；完成条件：局部测试断言 tool call/diff/terminal 内容块结构化、`AcpRaw` 的 `byte_length`/sha256 与原文一致、`turn` 为 `None`（归属交由 core 补齐）。
- [x] 2.22 实现 `interaction`：把 Agent 发起的权限/elicitation 请求交付给 core 并保持未完成，直到 `resolve_interaction` 给出解析；回传时保持原始 request id 的类型与字面量；不代答、不自动允许/拒绝、不用超时伪造结论；完成条件：用 `permission-request`/`elicitation` 场景的局部测试覆盖「等待外部解析」与「按原标识回传」。
- [x] 2.23 实现 turn 生命周期：`prompt` 发出 ACP 请求后立即返回 `TurnAccepted`（占位 `TurnId` 并在注释说明其不具权威性），turn 终态（完成/失败/取消）只产生一次事件，取消不阻塞其它会话；完成条件：局部测试覆盖「先接受后到达」「终态唯一」「取消不影响其它会话」三条。
- [x] 2.24 实现能力门控与读写：保存 `initialize` 声明的能力集合，未宣告的可选能力返回显式不支持且不发消息；`modes()` 未宣告时返回空结果、`set_mode`/`set_config` 未宣告时显式拒绝，已宣告时按 Agent 真实结果返回；完成条件：局部测试覆盖未宣告与已宣告两条路径。
- [x] 2.25 实现 `catalog`：`agents()` 从 `LocalConfigStore` 的 profile 派生 `AgentDescriptor` 与可用性（命令可解析、凭据引用存在）且不启动任何进程；`agent_capabilities()` 经真实 `initialize` 协商取得并缓存（缓存键含进程代），失败返回 `Unavailable`；完成条件：局部测试断言目录查询不创建子进程、单个 profile 的凭据失效只影响该条目。
- [x] 2.26 实现 `LaunchSpec` 组装：`env = env_allowlist ∩ profile 的 env 绑定声明的 name` 加必要进程环境；凭据只在 spawn 前经 `CredentialResolver` 解析，引用失效或 keystore 不可用一律失败关闭（进程不启动）；不注入 Node/Device 密钥；profile 只来自 `LocalConfigStore`，绝不读启动配置文件；完成条件：局部测试覆盖白名单为上限、引用失效失败关闭、子进程环境中无节点密钥、以及「配置文件中的同名条目不被使用」四条。
- [x] 2.27 实现空闲回收：以注入的 `sessions.idle_timeout_ms` 为准，仅在「无进行中 turn 且空闲超时」时关闭进程树并保持目录条目不可用，`0` 表示不因空闲关闭；完成条件：局部测试覆盖「超时后关闭」与「零值不关闭」两条，且关闭走 2.19 的关闭顺序。

## 3. Branch Validation

- [x] 3.1 执行 `node scripts/check-crate-boundaries.mjs` 与 `node scripts/check-acp-compatibility.mjs` + `node scripts/check-schema-fixtures.mjs` 
- [x] 3.2 只读检视 WP1 的固定版本 diff（§5 矩阵与 §4.5 收口措辞是否有歧义、依赖登记与实际口径是否一致、状态表是否夸大为已实现的能力），修复后由新 reviewer 复核 
- [x] 3.3 执行交付前 project verify：`cargo fmt --all -- --check`、`cargo clippy --locked -p acp-protocol --all-targets --all-features -- -D warnings`、`cargo test --locked -p acp-protocol --all-features` 
- [x] 3.4 只读检视 raw 保真机制（是否存在经 `Value` 往返后声称保真）、未知判别子与 `_` 方法语义、上限与结构化内容不被文本化、矩阵 row id 引用是否真实对应 
- [x] 3.5 执行 `cargo fmt`、`cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings`、`cargo test --locked -p agent-host --all-features`，并在**本机 Windows x64** 上单独执行 `cargo test --locked -p agent-host --all-features tree_ -- --nocapture` 
- [x] 3.6 只读检视子进程所有权与关闭顺序、stdio 分帧与 request id、进程树清理在失败路径也成立、stderr 有界与结构化计数、**平台分支** `cfg` 是否只落在 `platform.rs` 
- [x] 3.7 执行交付前 project verify：`cargo fmt`、`cargo clippy -p agent-host -D warnings`、`cargo test --locked -p agent-host --all-features` 
- [x] 3.8 只读检视 `EndpointEvent` 组装与 `AcpRaw`（sha256、不含换行、结构化不文本化）、交互 request id 保真、turn 终态唯一与取消、能力门控是否真的未发消息 
- [x] 3.9 执行交付前 project verify：`cargo fmt`、`cargo clippy -p agent-host -D warnings`、`cargo test --locked -p agent-host --all-features` 
- [x] 3.10 只读检视凭据注入交集与失败关闭、不注入节点/设备密钥、profile 来源、空闲回收与关闭顺序交互 

## 5. Integration Readiness

- [x] 5.1 （仅一次，不随单元复制）单独创建独立集成 Agent 并显式交接 `roles/integrator.md` 全文、计划与契约、源提交及证据、独立集成 worktree、目标分支与授权边界，记录实际 ID 与上下文方式；缺少独立执行能力时该任务 BLOCKED。
- [x] 5.2 复核该单元预定模式（integrated）与 WP 组成，核对 3.x 的检查与独立 review 证据对当前候选版本仍有效；变化先同步计划与依赖。

## 6. Merge Unit

- [x] 6.1 核实目标仓库与 `refs/heads/main` 当前提交并记录准确引用与核实命令；无法确认时保持 BLOCKED。
- [x] 6.2 基于已核实基线构造本单元候选，固定基线与候选版本，记录组成与构建结果（含 `cargo build --locked --workspace --all-features`）；完成条件：候选提交与构建输出记录在案。
- [x] 6.3 在候选版本执行 `npm run verify` 
- [x] 6.4 只读检视候选新增交互与冲突解决（`acp-protocol` 与 `agent-host` 的接口一致性、`LaunchSpec` 边界、`Cargo.toml`/§5 矩阵/文档三者一致），修复后独立复核 
- [x] 6.5 按 not-applicable 路径核对理由、依据、四项替代检查安排与 `downgrade_approval` 记录，确认没有必须运行 E2E 的任务
- [x] 6.6 确认候选证据完整后以条件更新或串行机制防竞态，在授权范围内合入并记录实际提交；基线变化时重开受影响候选任务。
- [x] 6.7 核对实际主分支结果与候选一致性并在 `main` 上重跑 `npm run verify` 
- [x] 6.8 独立检视合并新增差异；无新增差异时由主 Agent 记录依据与原 review ID 

## 7. Final E2E

- [x] 7.1 在最终主分支固定版本执行替代验证四项：`cargo test --locked -p acp-protocol -p agent-host --all-features`、`npm run check` [PV2/PV3]、`npm run verify`，以及**本机 Windows** 的 `cargo test --locked -p agent-host --all-features tree_`；Windows 不可用时该项记 BLOCKED 并如实上报。
- [x] 7.2 汇总替代验证的全部断言、版本与证据，核对覆盖了 not-applicable 的 `alternative_checks` 四项与资源清理（临时心跳目录、子进程、`CARGO_TARGET_DIR`），并核对 `fixtures/acp/v1/` 与矩阵的哈希在执行前后不变
- [x] 7.3 运行 `变更校验`，仅 PASS 自动勾选；此行只确认不适用判据已按计划固化，不执行测试或汇总。

## 8. Final Verification

- [x] 8.1 按 `变更校验`（无 skill 发现能力时读 `变更校验`）执行最终验收，核对用户意图、需求、设计、计划、任务与最终主分支证据；完成条件：验收结论为 PASS 且该检查 PASS；其余任务未完成或存在未闭环 FAIL/BLOCKED 时不得完成。
