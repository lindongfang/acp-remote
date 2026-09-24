## 1. Dependency and Resource Setup

- [x] 1.1 全变更；负责人：environment/recon；依赖：无；在新的任务级最小上下文中核实仓库路径、目标引用（`git rev-parse refs/heads/main`）、工具链（`rust-toolchain.toml` 与 Node ≥ 22.12）、`npm run verify` 的实际子检查清单、`openspec-agentic e2e --json` 的 `enabled`/`command` 实测值，以及本机是否存在可用的 Windows x64 环境（PV5 前提）；完成条件：事实清单与原始输出可被后续任务直接引用，写入 `verification.md`。
- [x] 1.2 [WP1] 负责人：实现 Agent；依赖：1.1；确认实现所需契约与文件所有权（§5 矩阵新增列、§4.5 收口措辞、`Cargo.toml` 的 members 与依赖登记、两个 crate 的目录归属），记录编码起点与 `Cargo.toml` 的分波次串行写入顺序（W0 WP1 → W1 WP2 → W2 WP3）；完成条件：写范围与契约清单落到 `verification.md`，无文件归属冲突。
- [x] 1.3 [WP1–WP5] 负责人：实现 Agent；依赖：1.1；确认唯一共享运行资源是构建目录 `target/`、临时心跳目录与子进程，确定并行分片时 `CARGO_TARGET_DIR` 的隔离方式、心跳目录的清理方法，以及 PV5 必须在本机 Windows 执行（Linux CI 不可复现）的事实；完成条件：隔离与释放方案记录在案，并写明 PV5 的平台限制。
- [x] 1.4 [WP4 / WP5] 负责人：实现 Agent；依赖：2.14、2.19；在 WP4/WP5 开始前接入已验收上游（WP3 的 `LaunchSpec`、supervisor 句柄与 `ProcessTree` 接口），核对下游实际基线与包含关系；完成条件：交接关系与包含关系检查记录在 `verification.md` 的 Dependency Handoffs。

## 2. Implementation

- [x] 2.1 [WP1] 负责人：实现 Agent；依赖：1.2；在 `docs/MODULE_ARCHITECTURE.md` §5 的矩阵新增 `agent-host` 列（`app` 行允许依赖 `agent-host`），并把 §4.5 的 Job 粒度收口为「每个 Agent 一个由 Daemon 侧 supervisor 持有的 Job」（保留 `KILL_ON_JOB_CLOSE`、Daemon 持有句柄、父→孙清理三条约束），同时更新 §3 的 crate 树、§3.1 的依赖口径与 §4.2/§4.5 的已落地状态；完成条件：`node scripts/check-crate-boundaries.mjs` 仍退出 0（此时两个 crate 尚未加入 members），且文档无将来时表述。
- [x] 2.2 [WP1] 负责人：实现 Agent；依赖：2.1；在 `Cargo.toml` 的 `[workspace.dependencies]` 登记 `tracing`、`win32job`（2.x）、`nix` 的版本与用途注释（原计划写 `libc`；实施时因 workspace 固定 `unsafe_code = "forbid"` 改为 safe wrapper `nix`，见 `verification.md` Check Plan Change 1），并修正「sqlite 适配器是唯一的 runtime 与数据库依赖持有者」注释为「runtime 由组合根启动，`storage-sqlite` 与 `agent-host` 持有 runtime 依赖类型」；不在此任务写 `members`；完成条件：`cargo metadata --no-deps --locked` 退出 0，依赖注释与实际口径一致。
- [x] 2.3 [WP1] 负责人：实现 Agent；依赖：2.1；把 `acp-protocol`、`agent-host` 从两处状态表的「待落地」移入本轮的已落地行（`README.md` 的「仓库当前状态」表、`AGENTS.md` §4 的模块状态表），并在 `docs/DEVELOPMENT_PLAN.md` §2 记录本轮基线变化；完成条件：三处表述一致且不含未实现的能力宣称。
- [x] 2.4 [WP1] 负责人：实现 Agent；依赖：2.2、2.3；执行 `node scripts/check-crate-boundaries.mjs` [PV2] 与 `node scripts/check-acp-compatibility.mjs` + `node scripts/check-schema-fixtures.mjs` [PV3]，并核对 `fixtures/acp/v1/` 与 `compatibility/acp/v1/matrix.json` 未被改动（逐字节）；完成条件：全部退出 0、夹具哈希不变，原始输出写入 `reports/wp1-boundaries.log` 与 `reports/wp1-acp-assets.log`。
- [x] 2.5 [WP2] 负责人：实现 Agent；依赖：2.1；创建 `crates/acp-protocol`（`Cargo.toml` 只依赖 `serde`/`serde_json`/`thiserror`，不含任何工作区 crate）、`src/lib.rs` 模块骨架与 `src/limits.rs`（1 MiB 单条消息上限等固定 v1 常量），并把 `crates/acp-protocol` 写入 `[workspace] members`；完成条件：`cargo build -p acp-protocol --all-features` 通过，`check:boundaries` 对该行无工作区依赖违规。
- [x] 2.6 [WP2] 负责人：实现 Agent；依赖：2.5；实现 JSON-RPC 信封分类（`Request`/`Notification`/`Response`）、方向校验与规范 required 字段校验，错误分类为 `Malformed`/`WrongDirection`（与「能力不支持」可区分），不做字段默认值补齐；完成条件：局部测试覆盖合法请求/通知/响应、缺 required 字段、方向错误三类，全部通过。
- [x] 2.7 [WP2] 负责人：实现 Agent；依赖：2.6；实现 `RawDocument`：保存原始 UTF-8 JSON document 文本（不含 stdio 换行）与路由字段（类别、`method`、`id` 的原文片段与类型判别，支持字符串 id），再编码一律原样回写原文；`serde_json::Value` 只用于读字段，不作为保真路径；完成条件：局部测试证明「原文 → 解析 → 回写」逐字节相等，且字符串型 `id` 与超大整数字面量原样保留。
- [x] 2.8 [WP2] 负责人：实现 Agent；依赖：2.7；实现 ACP v1 DTO：`initialize` 与 capability 声明（保留未识别能力字段）、`session/new`、`session/prompt`、`session/update` 的 11 种已知判别子、prompt/output content block（text/image/audio/resource_link/resource）、`session/request_permission` 与 elicitation；结构化内容按类型解码，不得合并为普通文本；未实现的可选能力不得以默认值或空对象表达为已支持；完成条件：局部测试覆盖每种判别子与每种 content block 的解码，并以 `tool-call-with-diff.json` 断言 tool call 与 diff 保持结构化。
- [x] 2.9 [WP2] 负责人：实现 Agent；依赖：2.8；实现未知 `sessionUpdate` 判别子的可见降级（保留逐字节原文、可被上层取得、不是解码失败）与 `_` 前缀扩展方法的显式不支持（方法未找到等价错误，不转发）；完成条件：对 `fixtures/acp/v1/future-session-update.json` 与 `extension-method.json` 的局部测试通过。
- [x] 2.10 [WP2] 负责人：实现 Agent；依赖：2.8；实现单条消息的 1 MiB 解析上限（超限返回 `Oversize` 且不保留部分结果、不缓存为正文）与有界处理约束（不缓存完整会话正文）；完成条件：局部测试覆盖刚好在限内与超限两类，超限路径不产生部分解码结果。
- [x] 2.11 [WP2] 负责人：实现 Agent；依赖：2.9、2.10；实现由 `fixtures/acp/v1/manifest.json` 与 `compatibility/acp/v1/matrix.json` 驱动的契约测试：逐条执行 manifest 的 `valid`/`invalid` 用例（不跳过）、以原始字节比较 `invariant.unknown_fields_byte_exact` 与 `invariant.meta_fields_byte_exact`、断言 `invariant.future_update_visible` 与 `invariant.extension_method_explicit_unsupported`，并在测试中引用矩阵 row id；不得复制第二套样例；完成条件：`cargo test --locked -p acp-protocol --all-features` 全绿且用例数非零，日志写入 `reports/wp2-acp-protocol-tests.log`。
- [x] 2.12 [WP3] 负责人：实现 Agent；依赖：2.5；创建 `crates/agent-host`（`Cargo.toml`：`core`、`acp-protocol`、`tokio`（features `process`/`io-util`/`macros`/`rt`；`sync`/`time` 由 workspace 级 feature 提供）、`async-trait`、`serde_json`、`thiserror`、`tracing`、`sha2`、`base64`，`cfg(windows)` 的 `win32job`、`cfg(unix)` 的 `nix`）、`src/lib.rs` 模块骨架与 `src/limits.rs`（`SECURITY_DESIGN.md` §12.2 的固定常量：启动 10 s、短请求 30 s、关闭 grace 5 s、1 MiB、stderr 256 KiB，turn 不设超时），并把 `crates/agent-host` 写入 `[workspace] members`；完成条件：`cargo build -p agent-host --all-features` 在两个目标平台上均可编译（Linux CI 覆盖 unix 分支），`check:boundaries` 通过。
- [x] 2.13 [WP3] 负责人：实现 Agent；依赖：2.12；实现 `crates/agent-host/src/bin/acpr-fake-acp-agent.rs`：按 argv 的 `--scenario` 选择行为（`normal`、`chunked-updates`、`permission-request`、`elicitation`、`slow-initialize`、`no-response`、`illegal-json`、`unknown-id`、`out-of-order`、`crash-on-prompt`、`spawn-grandchild`、`heartbeat-child`、`unknown-content-block`、`huge-line`、`stderr-flood`、`stderr-protocol-noise`），`spawn-grandchild` 自身 re-exec 为 `--scenario heartbeat-child` 并按 argv 给定的文件每 50 ms 追加心跳；场景选择只走 argv（不放宽 `env_allowlist`）；完成条件：测试用 `env!("CARGO_BIN_EXE_acpr-fake-acp-agent")` 能启动并完成一次 `initialize`。
- [x] 2.14 [WP3] 负责人：实现 Agent；依赖：2.13；实现 `LaunchSpec` 的消费侧与 supervisor 的 spawn：参数数组直传、不经 shell、stdio 全 piped、Windows 上 spawn 后立即（写 stdin 之前）把进程加入 Job、Unix 上 spawn 前设置进程组；`LaunchSpec` 由 WP5 产出，本任务只定义并对接该接口；完成条件：局部测试用 fake child 完成启动并读到 `initialize` 响应，接口签名冻结并写入 `verification.md` 的 Dependency Handoffs。
- [x] 2.15 [WP3] 负责人：实现 Agent；依赖：2.14；实现 stdout 的 LF 分帧（超 1 MiB 立即以 `Oversize` 结束该 Agent）、request id 单调分配与 pending 注册表（oneshot 唤醒，天然支持乱序）、未知 id 与非法 JSON 记为明确协议错误且不影响其它 pending；完成条件：用 `out-of-order`、`unknown-id`、`illegal-json` 三个场景的局部测试证明各条结论。
- [x] 2.16 [WP3] 负责人：实现 Agent；依赖：2.14；实现固定超时（`initialize` 10 s、短请求 30 s、**`session/prompt` 不设超时**）与 stderr 的 256 KiB 环形缓冲（超限丢最旧并记丢弃计数，只输出结构化计数、内容不进日志，绝不作为 ACP wire 解析）；完成条件：`slow-initialize`、`no-response` 与 stderr 超限三类局部测试通过，且断言 turn 不会因超时被杀。
- [x] 2.17 [WP3] 负责人：实现 Agent；依赖：2.14；在单个 `src/platform.rs` 内用 `#[cfg(windows)]`/`#[cfg(unix)]` 模块实现 `ProcessTree` 的两个平台后端（Windows 用 `win32job` 的 `Job::create_with_limit_info` + `limit_kill_on_job_close` + `set_extended_limit_info` + `assign_process`，**结束手段是关闭/丢弃该 Job 的句柄**（`KILL_ON_JOB_CLOSE`；该 wrapper 未封装 `TerminateJobObject`，直接 FFI 被 `unsafe_code = "forbid"` 禁止）；Unix 用 `process_group(0)` + `nix::sys::signal::killpg`），并把**平台分支** `cfg` 限制在 `platform.rs` 内；完成条件：两平台均可编译，`rg "cfg\(" crates/agent-host/src` 的**平台分支**只落在 `platform.rs`（`launch.rs` 只有 1 处 `#[cfg(test)]`、`bin/` 零命中）。
- [x] 2.18 [WP3] 负责人：实现 Agent；依赖：2.17；实现进程树常驻回归测试：用 `spawn-grandchild` 造出父→孙两层与心跳文件，断言「强制结束 Agent 后孙进程停止，心跳字节数不再增长」与「关闭 Job 句柄后父与孙一并停止」；完成条件：`cargo test -p agent-host --all-features tree_` 通过；Windows 上的原始输出写入 `reports/pv5-windows-tree.log`（PV5），Linux 只覆盖 unix 分支并如实记录。
- [x] 2.19 [WP3] 负责人：实现 Agent；依赖：2.15、2.16、2.17；实现 supervisor 的关闭顺序与所有权：停止接受新请求 → 未完成请求以明确错误结束 → 关闭 stdin → 等 5 s grace → 结束进程树 → join 全部子任务；正常路径无 `unwrap()`/`expect()`，无 detached task；完成条件：局部测试覆盖「仍有未完成请求时关闭」与「异常退出后关闭」，并断言无遗留后台任务。
- [x] 2.20 [WP4] 负责人：实现 Agent；依赖：1.4、2.14；实现 `SessionBackendFactory` 与 `SessionEndpoint`：`create` 用 core 给出的 `SessionId` 建立唯一映射（重复 create 返回冲突类错误）、`open` 只接受已存在的映射、`reference()` 返回 core 给出的引用、live endpoint generation 在重连/重开时正确推进；完成条件：局部测试覆盖三条映射结论与 generation 行为。
- [x] 2.21 [WP4] 负责人：实现 Agent；依赖：2.14；实现 `mapper`：把 ACP 通知映射为 `EndpointEvent { kind, event_type, payload: EventPayload { view, acp }, turn: None, causation, at }`，`AcpRaw::available("application/json", 不含换行的原文, sha256)`，结构化内容保持判别子与工具调用标识；完成条件：局部测试断言 tool call/diff/terminal 内容块结构化、`AcpRaw` 的 `byte_length`/sha256 与原文一致、`turn` 为 `None`（归属交由 core 补齐）。
- [x] 2.22 [WP4] 负责人：实现 Agent；依赖：2.14；实现 `interaction`：把 Agent 发起的权限/elicitation 请求交付给 core 并保持未完成，直到 `resolve_interaction` 给出解析；回传时保持原始 request id 的类型与字面量；不代答、不自动允许/拒绝、不用超时伪造结论；完成条件：用 `permission-request`/`elicitation` 场景的局部测试覆盖「等待外部解析」与「按原标识回传」。
- [x] 2.23 [WP4] 负责人：实现 Agent；依赖：2.15、2.21；实现 turn 生命周期：`prompt` 发出 ACP 请求后立即返回 `TurnAccepted`（占位 `TurnId` 并在注释说明其不具权威性），turn 终态（完成/失败/取消）只产生一次事件，取消不阻塞其它会话；完成条件：局部测试覆盖「先接受后到达」「终态唯一」「取消不影响其它会话」三条。
- [x] 2.24 [WP4] 负责人：实现 Agent；依赖：2.8、2.14；实现能力门控与读写：保存 `initialize` 声明的能力集合，未宣告的可选能力返回显式不支持且不发消息；`modes()` 未宣告时返回空结果、`set_mode`/`set_config` 未宣告时显式拒绝，已宣告时按 Agent 真实结果返回；完成条件：局部测试覆盖未宣告与已宣告两条路径。
- [x] 2.25 [WP5] 负责人：实现 Agent；依赖：1.4、2.12；实现 `catalog`：`agents()` 从 `LocalConfigStore` 的 profile 派生 `AgentDescriptor` 与可用性（命令可解析、凭据引用存在）且不启动任何进程；`agent_capabilities()` 经真实 `initialize` 协商取得并缓存（缓存键含进程代），失败返回 `Unavailable`；完成条件：局部测试断言目录查询不创建子进程、单个 profile 的凭据失效只影响该条目。
- [x] 2.26 [WP5] 负责人：实现 Agent；依赖：1.4、2.12；实现 `LaunchSpec` 组装：`env = env_allowlist ∩ profile 的 env 绑定声明的 name` 加必要进程环境；凭据只在 spawn 前经 `CredentialResolver` 解析，引用失效或 keystore 不可用一律失败关闭（进程不启动）；不注入 Node/Device 密钥；profile 只来自 `LocalConfigStore`，绝不读启动配置文件；完成条件：局部测试覆盖白名单为上限、引用失效失败关闭、子进程环境中无节点密钥、以及「配置文件中的同名条目不被使用」四条。
- [x] 2.27 [WP5] 负责人：实现 Agent；依赖：2.19、2.25；实现空闲回收：以注入的 `sessions.idle_timeout_ms` 为准，仅在「无进行中 turn 且空闲超时」时关闭进程树并保持目录条目不可用，`0` 表示不因空闲关闭；完成条件：局部测试覆盖「超时后关闭」与「零值不关闭」两条，且关闭走 2.19 的关闭顺序。

## 3. Branch Validation

- [x] 3.1 [WP1] 负责人：实现 Agent；依赖：2.4；执行 `node scripts/check-crate-boundaries.mjs` [PV2] 与 `node scripts/check-acp-compatibility.mjs` + `node scripts/check-schema-fixtures.mjs` [PV3]；完成条件：退出码 0，日志写入 `reports/wp1-boundaries.log` 与 `reports/wp1-acp-assets.log`，并记录矩阵列数、trait/类型计数与夹具哈希。
- [x] 3.2 [WP1] 负责人：独立 reviewer（新隔离上下文，完整读取 `roles/reviewer.md`）；依赖：3.1，可与 3.1 并行；只读检视 WP1 的固定版本 diff（§5 矩阵与 §4.5 收口措辞是否有歧义、依赖登记与实际口径是否一致、状态表是否夸大为已实现的能力），修复后由新 reviewer 复核 [RV1]；完成条件：报告写入 `reports/rv1-wp1.md`，无未解决阻断项。
- [x] 3.3 [WP2] 负责人：实现 Agent；依赖：2.11；执行交付前 project verify：`cargo fmt --all -- --check`、`cargo clippy --locked -p acp-protocol --all-targets --all-features -- -D warnings`、`cargo test --locked -p acp-protocol --all-features` [PV4]；完成条件：全部通过且日志写入 `reports/wp2-acp-protocol-tests.log`，无失败、无零用例、无全跳过。
- [x] 3.4 [WP2] 负责人：独立 reviewer；依赖：3.3，可与 3.3 并行；只读检视 raw 保真机制（是否存在经 `Value` 往返后声称保真）、未知判别子与 `_` 方法语义、上限与结构化内容不被文本化、矩阵 row id 引用是否真实对应 [RV1]；完成条件：报告写入 `reports/rv1-wp2.md`，无未解决阻断项。
- [x] 3.5 [WP3] 负责人：实现 Agent；依赖：2.19；执行 `cargo fmt`、`cargo clippy --locked -p agent-host --all-targets --all-features -- -D warnings`、`cargo test --locked -p agent-host --all-features` [PV4]，并在**本机 Windows x64** 上单独执行 `cargo test --locked -p agent-host --all-features tree_ -- --nocapture` [PV5]；完成条件：PV4 全绿写入 `reports/wp3-agent-host-supervision.log`；PV5 的两条进程树断言通过并写入 `reports/pv5-windows-tree.log`（Linux 只覆盖 unix 分支，如实记录）。
- [x] 3.6 [WP3] 负责人：独立 reviewer；依赖：3.5，可与 3.5 并行；只读检视子进程所有权与关闭顺序、stdio 分帧与 request id、进程树清理在失败路径也成立、stderr 有界与结构化计数、**平台分支** `cfg` 是否只落在 `platform.rs` [RV1]；完成条件：报告写入 `reports/rv1-wp3.md`，无未解决阻断项。
- [x] 3.7 [WP4] 负责人：实现 Agent；依赖：2.24；执行交付前 project verify：`cargo fmt`、`cargo clippy -p agent-host -D warnings`、`cargo test --locked -p agent-host --all-features` [PV4]；完成条件：全部通过且日志写入 `reports/wp4-agent-host-session.log`。
- [x] 3.8 [WP4] 负责人：独立 reviewer；依赖：3.7，可与 3.7 并行；只读检视 `EndpointEvent` 组装与 `AcpRaw`（sha256、不含换行、结构化不文本化）、交互 request id 保真、turn 终态唯一与取消、能力门控是否真的未发消息 [RV1]；完成条件：报告写入 `reports/rv1-wp4.md`，无未解决阻断项。
- [x] 3.9 [WP5] 负责人：实现 Agent；依赖：2.27；执行交付前 project verify：`cargo fmt`、`cargo clippy -p agent-host -D warnings`、`cargo test --locked -p agent-host --all-features` [PV4]；完成条件：全部通过且日志写入 `reports/wp5-agent-host-config.log`。
- [x] 3.10 [WP5] 负责人：独立 reviewer；依赖：3.9，可与 3.9 并行；只读检视凭据注入交集与失败关闭、不注入节点/设备密钥、profile 来源、空闲回收与关闭顺序交互 [RV1]；完成条件：报告写入 `reports/rv1-wp5.md`，无未解决阻断项。

## 5. Integration Readiness

- [x] 5.1 （仅一次，不随单元复制）负责人：主 Agent；依赖：3.1–3.10；单独创建独立集成 Agent 并显式交接 `roles/integrator.md` 全文、计划与契约、源提交及证据、独立集成 worktree、目标分支与授权边界，记录实际 ID 与上下文方式；完成条件：交接记录在 `verification.md`；缺少独立执行能力时该任务 BLOCKED。
- [x] 5.2 [DU1] 负责人：主 Agent；依赖：3.1–3.10；复核该单元预定模式（integrated）与 WP 组成，核对 3.x 的检查与独立 review 证据对当前候选版本仍有效；完成条件：结论写入 `reports/du1-integration.md` 的就绪段；变化先同步计划与依赖。

## 6. Merge Unit

- [x] 6.1 [DU1] 负责人：主 Agent（机械核实可派发 environment/recon）；依赖：5.2；核实目标仓库与 `refs/heads/main` 当前提交并记录准确引用与核实命令；完成条件：目标提交与核实证据写入 `verification.md`；无法确认时保持 BLOCKED。
- [x] 6.2 [DU1] 负责人：集成 Agent；依赖：6.1；基于已核实基线构造本单元候选，固定基线与候选版本，记录组成与构建结果（含 `cargo build --locked --workspace --all-features`）；完成条件：候选提交与构建输出记录在案。
- [x] 6.3 [DU1] 负责人：独立检查执行者；依赖：6.2；在候选版本执行 `npm run verify` [PV1]；完成条件：退出码 0 且逐子项结果（十道合同门禁、fmt、clippy、workspace 全测试）写入 `reports/du1-pv1.log`。
- [x] 6.4 [DU1] 负责人：独立 reviewer；依赖：6.2，可与 6.3 并行；只读检视候选新增交互与冲突解决（`acp-protocol` 与 `agent-host` 的接口一致性、`LaunchSpec` 边界、`Cargo.toml`/§5 矩阵/文档三者一致），修复后独立复核 [RV1]；完成条件：报告写入 `reports/rv1-du1.md`，无未解决阻断项。
- [x] 6.5 [DU1] 负责人：主 Agent；依赖：6.2；按 not-applicable 路径核对理由、依据、四项替代检查安排与 `downgrade_approval` 记录，确认没有必须运行 E2E 的任务；完成条件：结论写入 `reports/du1-integration.md` 的 E2E 段。
- [x] 6.6 [DU1] 负责人：主 Agent（按当前授权）；依赖：6.3、6.4、6.5；确认候选证据完整后以条件更新或串行机制防竞态，在授权范围内合入并记录实际提交；完成条件：实际合入提交记录在 `verification.md`；基线变化时重开受影响候选任务。
- [x] 6.7 [DU1] 负责人：独立检查执行者；依赖：6.6；核对实际主分支结果与候选一致性并在 `main` 上重跑 `npm run verify` [PV1]；完成条件：主分支日志写入 `reports/du1-main-verify.log`，有效复用逐项记录原证据与适用性。
- [x] 6.8 [DU1] 负责人：独立 reviewer；依赖：6.6，可与 6.7 并行；独立检视合并新增差异；无新增差异时由主 Agent 记录依据与原 review ID [RV1]；完成条件：结论记录在 `reports/rv1-du1.md` 的复核段。

## 7. Final E2E

- [x] 7.1 全变更；负责人：主 Agent（not-applicable 的替代验证）；依赖：6.7、6.8；在最终主分支固定版本执行替代验证四项：`cargo test --locked -p acp-protocol -p agent-host --all-features` [PV4]、`npm run check` [PV2/PV3]、`npm run verify` [PV1]，以及**本机 Windows** 的 `cargo test --locked -p agent-host --all-features tree_` [PV5]；完成条件：逐项命令、版本、输出与断言写入 `reports/alt-final-verification.md`；Windows 不可用时该项记 BLOCKED 并如实上报。
- [x] 7.2 全变更；负责人：主 Agent；依赖：7.1；汇总替代验证的全部断言、版本与证据，核对覆盖了 not-applicable 的 `alternative_checks` 四项与资源清理（临时心跳目录、子进程、`CARGO_TARGET_DIR`），并核对 `fixtures/acp/v1/` 与矩阵的哈希在执行前后不变；完成条件：汇总与清理结论写入 `reports/alt-final-verification.md` 的汇总段。
- [x] 7.3 [e2e-owned] 全变更；负责人：扩展；依赖：7.2；运行 `npx --quiet --no-install openspec-agentic e2e check --change acp-boundary-and-agent-host`，仅 PASS 自动勾选；此行只确认不适用判据已按计划固化，不执行测试或汇总。

## 8. Final Verification

- [ ] 8.1 [final-verification] 负责人：主 Agent；依赖：7.3；按 `.agents/skills/agentic-verify/SKILL.md`（无 skill 发现能力时读 `openspec/schemas/agentic/procedures/acceptance.md`）执行最终验收，核对用户意图、需求、设计、计划、任务与最终主分支证据；在 `verification.md` 记录当前 agentic-assessment 后运行 `npx --quiet --no-install openspec-agentic workflow check --change acp-boundary-and-agent-host --stage final --json`；完成条件：验收结论为 PASS 且该检查 PASS；其余任务未完成或存在未闭环 FAIL/BLOCKED 时不得完成。
