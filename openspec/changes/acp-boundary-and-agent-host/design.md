## Context

- 切片 2 的两个 crate 都不存在：`crates/` 只有 `acpr-transcript`、`acpr-wire`、`core`、`storage-sqlite`、`sync-protocol`、`node-link-protocol`（`README.md` 的「仓库当前状态」、`docs/DEVELOPMENT_PLAN.md` §2）。`core` 的 `AgentCatalog` / `SessionBackendFactory` / `SessionEndpoint`（`docs/CORE_PORTS_AND_STORAGE.md` §5.1、`crates/core/src/ports.rs`）目前只有测试替身实现（`crates/core/src/broker.rs` 的 `FakeEndpoint`）。
- `compatibility/acp/v1/matrix.json` 与 `fixtures/acp/v1/` 已被 `npm run check` 的 `check:acp-compatibility.mjs`、`check-schema-fixtures.mjs` 做结构校验，但**没有 Rust 消费方**；矩阵 §4 要求「对应模块建立后，Rust 测试必须读取同一矩阵或引用相同 row/test id 输出证据」。
- 已冻结、本设计必须遵守的实现约束（不重复其理由）：`docs/MODULE_ARCHITECTURE.md` §4.2/§4.5（职责、依赖、Job Object、stdio、失败路径、profile 来源）、§5（依赖矩阵）、`docs/SECURITY_DESIGN.md` §12.2（固定 v1 常量表）、`docs/ACP_COMPATIBILITY_MATRIX.md` §5（第一阶段门槛）、`docs/INITIAL_DESIGN.md` §16 第 4 条（Job Object 探针结论与「必须变成常驻回归测试」）。
- 两个可直接复用的既有约定：wire 层用「原文承载、body 按需解析」证明保真（`crates/sync-protocol/src/envelope.rs` 与 `crates/sync-protocol/tests/support/mod.rs` 的 `body_slice`）；ACP raw 在 core 侧的形状是 `EventPayload { view: ViewJson, acp: Option<AcpRaw> }`，`AcpRaw::available(media_type, raw_json, sha256)` 的 `raw_json` 不含 stdio 换行（`docs/SYNC_PROTOCOL.md` §10.3 与第 49 条注记）。

## Goals / Non-Goals

**Goals:**

- 冻结两个 crate 的模块边界、对外类型与接口约定，使下游切片（Daemon/CLI、Node Link、`acp_facade`）只需装配，不必改本变更的公共形状。
- 用仓库内的**可执行 fake ACP child** 取代一次性探针：进程树清理、超时、取消、异常退出、乱序响应、raw 保真全部成为常驻测试。
- 明确 raw 保真的实现机制（原文承载 + 按需解析），使「经通用 DTO 往返」这种伪保真实现无法通过契约测试。
- 把平台差异收敛到单一 `cfg` 子模块，让 `agent-host` 的其余部分在所有平台编译、单测。

**Non-Goals:**

- 不复述 proposal 的范围外事项（`server`、`app`、同步与 Node Link、真实 Agent 兼容报告）。
- 不在本变更引入 ACP 上游快照升级流程（`docs/ACP_COMPATIBILITY_MATRIX.md` §2 已有 5 步流程）。
- 不做 multi-endpoint / 多 Agent 负载策略的参数化（`docs/INITIAL_DESIGN.md` §16 第 7 条说明首切片不需要）。
- 不在本变更校验 Sync 事件视图的字段级 schema 一致性（那是 Sync 切片与 `server::sync` 的门禁）；本变更只保证结构化、不文本化、带 `AcpRaw`。
- 不设计 daemon 生命周期、本地 IPC、单实例锁与配置装载（切片 4 的 `app`）。

## Decisions

### D1 边界与依赖

**`acp-protocol`（叶子 crate）**

- 依赖：`serde`（derive）、`serde_json`（`raw_value`，继承 workspace 的 `arbitrary_precision`）、`thiserror`。
- *不*依赖：`core`、其他协议 crate、`acpr-wire`/`acpr-transcript`、runtime、子进程或任何 HTTP/DB 依赖。理由：ACP v1 没有二进制/base64url 字段，跨协议共享的 ACPR-CJ1 与 transcript codec 都不在 ACP wire 上；列入 `AGENTS.md` §4 的「协议 crate 彼此不依赖」与 `MODULE_ARCHITECTURE.md` §4.2「不依赖 `core`」。
- 依赖方向证据：`agent-host` 行的 `✓ core`/`✓ acp-protocol` 已在 §5 矩阵里；`acp-protocol` 行必须保持全空（`check:boundaries` 会拒绝对任何工作区 crate 的依赖）。

**`agent-host`**

| 依赖 | 用途 | 备注 |
|---|---|---|
| `core` | 端口与值对象（`AgentCatalog`/`SessionBackendFactory`/`SessionEndpoint`/`PortError`/`EndpointEvent`/`AcpRaw`/`ResolvedWorkspace`） | 矩阵已有 |
| `acp-protocol` | wire DTO 与 raw document | 矩阵已有 |
| `tokio`（`process`/`io-util`/`sync`/`time`） | 子进程与 stdio 异步 I/O、oneshot、计时 | **不启动 runtime**；runtime 由组合根拥有 |
| `serde_json` | 事件 view 组装、响应载荷读取 | |
| `thiserror` | crate 内错误类型 | |
| `sha2` | `AcpRaw` 的 sha256 | workspace 已有该依赖 |
| `tracing` | 结构化、脱敏日志 | 新增到 `[workspace.dependencies]`（§3.1 已把 `tracing` 列入计划） |
| `win32job`（`cfg(windows)`） | Job Object（`KILL_ON_JOB_CLOSE`、`TerminateJobObject`） | 2.x，MIT OR Apache-2.0，安全 API |
| `nix 0.30`（`cfg(unix)`；safe wrapper，代替 `libc`） | `killpg` 结束进程组 | MIT OR Apache-2.0 |

- 备选（日志）：自定义 log sink 端口。否掉的理由：core 没有、也不应该有日志端口（`core` 不依赖 runtime 与 tracing），而 §3.1 已经把 `tracing` 列为 workspace 计划依赖；订阅器初始化属于组合根（切片 4）。
- `Cargo.toml` 的 `[workspace.dependencies]` 中「sqlite 适配器是唯一的 runtime 与数据库依赖持有者」注释在本变更后不再准确（`agent-host` 也持有 runtime 类型），该注释随本变更一并修正为「runtime 由组合根启动，`storage-sqlite` 与 `agent-host` 持有 runtime 依赖类型」。

### D2 raw 保真的机制（`acp-protocol`）

- `RawDocument` 持有**原始 UTF-8 JSON document 文本**（不含 stdio 换行）和从原文解析出的路由字段（JSON-RPC 类别、`method`、`id` 类型与字面量）。「解码 → 再编码」的保真通过**原样回写原文**实现。
- `serde_json::Value` 只用于需要读字段的场景（capability 读取、事件 view 组装），**不作为保真证据**：`arbitrary_precision` 只保证数字在 `Value` 中不被降级，不保证键顺序、空白、转义与数字文本形式被保留。
- 上游 request id 可能不是整数（`docs/ACP_COMPATIBILITY_MATRIX.md` §6 实测 Zed 使用 UUID 字符串），因此路由字段的 `id` 必须以「原文片段 + 类型判别」保存，回填时按原类型与字面量输出。
- 证据（引用矩阵 row id）：`invariant.unknown_fields_byte_exact`（`unknown-fields-and-large-integer.json`）与 `invariant.meta_fields_byte_exact`（`meta-and-unknown-fields.json`）用**原始字节**与回写字节逐字节比较。

### D3 消息分类、未知项与错误模型

- 信封分类：`Request`（`method` + `id`）、`Notification`（`method`、无 `id`）、`Response`（`id` + `result` 或 `error`）；方向按矩阵 `direction` 校验（`agent-host` 作为 ACP client：发 request/notification，收 response/notification 与 agent→client request）。
- 已知判别子：矩阵 `sessionUpdates` 的 11 项（`update.user_message_chunk`、`agent_message_chunk`、`agent_thought_chunk`、`tool_call`、`tool_call_update`、`plan`、`available_commands_update`、`current_mode_update`、`config_option_update`、`session_info_update`、`usage_update`）。其余判别子 → `UnknownUpdate`（`invariant.future_update_visible` 的可见降级），保留逐字节原文，**不是**解码失败。
- `_` 前缀方法 → `ExplicitUnsupported`（`invariant.extension_method_explicit_unsupported`）：返回方法未找到等价的明确错误，不转发给子进程。
- 错误分类（协议层，`thiserror` 枚举）：`Malformed`（非法 JSON / 信封 / 缺 required 字段）、`Oversize`（>1 MiB）、`WrongDirection`、`UnknownMethod`。上层语义（NotSupported/Available 等）不与此混用，以免「结构错误」被当成「能力不支持」。

### D4 `agent-host` 结构、所有权与关闭顺序

模块划分（`unsafe_code` 保持 `forbid`，不使用 `AgentRuntime` 这类巨型类型）：

| 模块 | 职责 |
|---|---|
| `catalog` | `AgentCatalog`：profile → `AgentDescriptor` + 可用性；`agent_capabilities` 需要真实协商（见 D5） |
| `supervisor` | 每个 Agent 一个 supervisor：spawn、Job/进程组、stdin writer、stdout reader、stderr pump、wait、pending request 注册表、空闲回收、关闭顺序 |
| `session` | `SessionBackendFactory` + `SessionEndpoint`；core `SessionId` ↔ ACP `sessionId` 映射；live endpoint generation |
| `mapper` | ACP DTO → `EndpointEvent`/`EventPayload`（`ViewJson` + `AcpRaw`），保留结构化语义 |
| `interaction` | Agent→client 的权限/elicitation 请求 ↔ core `InteractionId`，等待 `resolve_interaction` |
| `platform` | `cfg(windows)` / `cfg(unix)` 的 `ProcessTree` 实现；**只有这里出现 `cfg`** |
| `limits` | 固定 v1 常量（D7） |

所有权与关闭顺序（`AGENTS.md` §7「异步任务必须有所有者、取消路径与关闭顺序」）：

- 每个 supervisor 拥有一个子任务集合（stdout reader、stderr pump、wait/退出监视）与一个 pending request 注册表；没有任何 detached task。
- 关闭顺序：停止接受新请求 → 让所有未完成请求以明确错误结束 → 关闭 stdin → 等待关闭 grace 5 s → 结束进程树 → join 全部子任务。任一环节超时都继续走后续环节，不无限等待。
- stdio 分帧：stdout 按 LF 分帧；单条消息超过 1 MiB 立即以 `Oversize` 结束该 Agent（不缓存、不截断后继续）。
- request id：supervisor 单调分配整数 id，登记 `pending: HashMap<RequestId, oneshot::Sender<...>>`；响应按 id 唤醒（因此乱序天然可处理）；未知 id 记为 `ProtocolError` 且不影响其他 pending；短请求用 30 s 超时（不含 `session/prompt`）。
- 每会话：同一 core `SessionId` 只允许一个 `SessionEndpoint`；重复 `create` 返回冲突类错误，不启动第二个进程。`open` 只接受本进程已建立的映射，否则返回不可用错误。

### D5 与 core 端口的对接语义

- **prompt 的同步/异步**：`SessionEndpoint::prompt` 发出 ACP `session/prompt` 请求后**不等待**其响应，立即返回 `TurnAccepted`；该请求的响应（ACP 语义下即 turn 结束）到达时产出 turn 终态事件。理由：`core` 的会话不变量要求「turn 被同步接受、事件异步到达、先持久化再广播」。
- **turn 归属**：`EndpointEvent.turn` 一律为 `None`，由 core 的 broker 用当前派发的 turn 补齐（`crates/core/src/broker.rs`：`event.turn.clone().or_else(|| running.clone())`）。理由：`SessionEndpoint::prompt(request, at)` 的签名不携带 `TurnId`，而 core 在调用前已把 `turn.started` 与 TurnId 提交；由此 `agent-host` 不分配、也不猜测 TurnId。
- **`TurnAccepted.turn` 的处理**：core 当前忽略该字段（`broker.rs` 中 `Ok(_) => Ok(true)`）。本设计返回一个格式合法的占位 `TurnId`，并在端口实现处写注释说明该值不具权威性。若 core 将来开始消费它，正确的收口方式是把 `TurnId` 传进 `PromptRequest`——那是 `core` 端口变更（用户决策），不在本变更内做。
- **`agent_capabilities`**：必须来自真实协商（`initialize` 的 response），不能凭 profile 或 Agent 名称编造；实现上复用已运行的 supervisor，必要时启动一次并缓存协商结果（缓存键含进程代），失败返回 `PortError::Unavailable`。`agents()`（目录）不启动进程。
- **raw 与 view 同时交给 core**：`EventPayload { view, acp }`，`acp = AcpRaw::available("application/json", <不含换行的原文>, sha256)`；view 是结构化投影（tool call / diff / terminal 内容块保持结构），未识别判别子按可见降级表达。view 的字段级 schema 一致性留给 Sync 切片（见 Non-Goals）。

### D6 平台差异与进程树

- 内部 trait `ProcessTree` 抽象「结束整棵树」：`platform::windows` 用 Job Object，`platform::unix` 用进程组。crate 其余部分不出现 `cfg`。
- **Windows**：`win32job` 2.x（`Job::create` → `limit_kill_on_job_close()` → `set_extended_limit_info()`；结束用 `TerminateJobObject` 或关闭/丢弃句柄）。**每个 Agent 一个 Job**，句柄由 Daemon 侧的 supervisor 持有。
  - 为什么不是单一全局 Job：单 Job 下无法只结束某一棵 Agent 树（`TerminateJobObject` 会波及全部 Agent），而本 crate 必须支持按 Agent 结束（空闲回收、单个 Agent 崩溃/超时）。`KILL_ON_JOB_CLOSE` 的关键性质（Daemon 崩溃或句柄关闭即停止整棵树）在每 Agent 一个 Job 下同样成立，因为句柄全由 Daemon 进程持有。该解释随本变更写入 `MODULE_ARCHITECTURE.md` §4.5；`KILL_ON_JOB_CLOSE`、Daemon 持有、父→孙清理三条约束不变。
  - 赋值时机：Job 先创建并设置 `KILL_ON_JOB_CLOSE`，再 spawn（`tokio::process::Command`，Windows 上 `Child::raw_handle()` 取句柄），spawn 成功后**在写入任何 stdin 之前**立即 assign。探针同样是「先 spawn 后 assign」，残余窗口见风险 2。
  - 被否的备选：`process-wrap` 10（MSRV 1.87 > 仓库 1.85，需用户决定是否抬 MSRV）；`command-group`（MSRV 1.68 但已弃用，且不提供 Job Object）；直接 FFI `kernel32`（被 workspace `unsafe_code = "forbid"` 禁止）。
  - 退路：实现第一步核验 `win32job` 的传递依赖 MSRV、许可证与维护状态；若不合格或传导抬高 MSRV → **回到用户决策**（抬 MSRV，或新增 ADR + 为该 crate 覆盖 lint），不擅自放开 `unsafe`。
- **Unix**：spawn 前 `CommandExt::process_group(0)` 使子进程成为新进程组组长；结束用 `nix::sys::signal::killpg(pid, SIGKILL)`，随后 `wait` 回收。CI 在 Linux 上执行这条路径。

### D7 固定 v1 常量与有界处理

按 `docs/SECURITY_DESIGN.md` §12.2 表落成 `limits` 常量，**不得**做成配置键或另发明数值：

| 项 | 值 | 用途 |
|---|---:|---|
| 启动 → `initialize` 完成 | 10 s | 启动超时 |
| 短请求（`initialize`/`cancel` 等） | 30 s | 不适用于 `session/prompt` |
| `session/prompt`（turn） | 不设超时 | 只有显式取消或进程退出才结束 |
| 关闭 grace | 5 s | 友好终止 → 强杀 |
| 单条 ACP 消息 | 1 MiB | stdout 解析上限，超限即失败 |
| stderr 环形缓冲 | 256 KiB | 超出丢最旧 + 计数 |
| 空闲回收 | 注入的 `sessions.idle_timeout_ms` | `0` = 不因空闲关闭；该值由组合根从配置读出后注入本 crate |
| 内存 | 规则而非数值 | 流式处理消息与 stderr，不缓存完整会话正文 |

### D8 profile 与凭据注入

- `agents()` = `LocalConfigStore` 的 profile 列表 + 可用性判定（命令可解析、凭据引用存在）；不启动进程、不读启动配置文件（`MODULE_ARCHITECTURE.md` §4.5、`docs/CONFIG_REFERENCE.md` §7）。
- `create`：`CreateSessionRequest.agent`（`AgentRef`）→ 按 id 取 profile（缺失 = 参数类错误）；`CreateSessionRequest.workspace`（`ResolvedWorkspace`）作为 `session/new` 的 cwd——**不**自行解析别名或拼路径（`workspace-resolution` 既有规范）。
- 子进程启动：参数数组直传、不经 shell；env = `env_allowlist ∩ profile 的 env 绑定声明的 name` 加必要进程环境；凭据值只在 spawn 前经 `CredentialResolver` 解析，引用失效或 keystore 不可用 → 失败关闭（`PortError::Unavailable`），进程不启动。Node/Device 密钥不进入子进程环境（本 crate 不持有它们）。
- **接口切分**：`LaunchSpec { program, args, env }` 由 profile/凭据侧（WP5）产出，交给进程监督侧（WP3）。`supervisor` 不读 profile、不解析凭据、不知道 `env_allowlist`；它只接受一个已经解析完成的启动描述并负责进程、stdio、超时与进程树。这条切分让「凭据注入边界」与「进程生命周期」可以分别测试与独立 review（对应 WP3/WP5 的写入范围划分）。

### D9 日志与脱敏

- `tracing` 结构化字段：agent id、session id、事件类型、stderr 丢弃计数、错误分类、关闭阶段；**不**记录凭据值、prompt 正文、规范化 workspace 路径明文、ACP raw 全文。
- 订阅器初始化不在本 crate（组合根负责）。测试不依赖订阅器存在。

### D10 测试基座

- **fake ACP child**：`crates/agent-host/src/bin/acpr-fake-acp-agent.rs`（workspace-private，`publish = false`），测试用 `env!("CARGO_BIN_EXE_acpr-fake-acp-agent")` 定位；场景由 **argv** 选择（`--scenario <name>`），而不是环境变量——因为注入集合受 `env_allowlist ∩ 绑定` 限制，测试不应该为了脚本开关而放宽白名单。
- 场景清单：`normal`、`chunked-updates`、`permission-request`、`elicitation`、`slow-initialize`、`no-response`、`illegal-json`、`unknown-id`、`out-of-order`、`crash-on-prompt`、`spawn-grandchild`（自身 re-exec 为 `--scenario heartbeat-child`，每 50 ms 向 argv 给定的文件追加心跳，供进程树断言）。
- **进程树回归测试**（把 `INITIAL_DESIGN.md` §16 第 4 条的探针结论固化为常驻测试）：Windows 断言「关闭 Job 句柄后父与孙一同停止」与「强制结束 Agent 后孙停止」；Unix 断言进程组路径的同一性质。**诚实说明**：Linux CI 只执行 `#[cfg(unix)]` 分支，Windows 断言必须在本地 Windows 上实际执行并留证（验证计划中单列）。
- 协议层契约测试读 `fixtures/acp/v1/manifest.json`（valid/invalid 逐例）与 `compatibility/acp/v1/matrix.json`（`invariants` 条目与 row id），保真用例直接读原始字节。
- 不使用真实 Codex/OMP（矩阵 §7：真实 Agent 报告是发布前产物，不是普通 CI 硬依赖）。

### D11 门禁与文档同步面

- `docs/MODULE_ARCHITECTURE.md`：§3/§3.1（成员与依赖口径）、§4.2/§4.5（职责、Job 粒度收口、已选定的 wrapper 与常量归属）、§5（新增 `agent-host` 列，`app` 行允许依赖 `agent-host`；`agent-host` 行的两格已存在）。
- `Cargo.toml`：`members` 增加两个 crate；`[workspace.dependencies]` 增加 `tracing`/`win32job`/`nix` 并修正 runtime 依赖注释。
- `scripts/check-crate-boundaries.mjs` 以 §5 文档为唯一判据，矩阵改动生效后无需改脚本；`core` 的 allow-list 不受影响（本变更不动 `core`）。
- `README.md`「仓库当前状态」表、`docs/DEVELOPMENT_PLAN.md` §2、`AGENTS.md` §4 的模块状态表：把 `acp-protocol`、`agent-host` 从「待落地」移入已落地行。
- `fixtures/acp/v1/manifest.json` 与矩阵**只在确实需要新用例时**才动；能只用既有夹具满足要求就不动（更小的漂移面）。

## Risks / Trade-offs

1. [core 的 `TurnAccepted.turn` 无法被适配器真实填充] → 返回格式合法的占位值 + 端口实现注释；core 当前忽略该字段。**触发重开**：core 若开始消费该值，需要把 `TurnId` 传进 `PromptRequest`（core 端口变更 → 用户决策）。
2. [Job assign 存在极小竞态窗口] → Job 先创建、spawn 后写 stdin 前立即 assign；结束路径以 Job 终止为主；残余窗口与探针方法一致，记录为已知限制。
3. [`win32job` 的 MSRV/维护性未在计划阶段实证] → 以内部 `ProcessTree` 抽象隔离；实现第一步核验，不合格回到用户决策；退路是 ADR + 为该 crate 覆盖 lint（不放开 workspace 的 `unsafe_code = "forbid"`）。
4. [「每 Agent 一个 Job」是对 §4.5 原文的收口解释] → 随本变更把该解释写入 §4.5，并保持三条硬约束不变；如用户要求单一全局 Job，则在 apply 前调整本设计。
5. [raw 保真极易被实现成 `parse → to_string`] → 契约测试直接比较原始字节；代码评审把「经 `Value` 往返后声称保真」列为阻断项。
6. [1 MiB 单条上限可能拒绝合法的大消息] → 与 Sync `maxMessageBytes` 同值，超限是明确错误而非静默截断；真实 Agent 需要更大值时属于安全常量与矩阵变更（用户决策）。
7. [fake child 作为 crate 内 bin 会随 `cargo build --workspace` 构建] → `publish = false`，切片 8 打包时排除；本变更记录该已知残留。
8. [Windows 进程树断言不在 Linux CI 覆盖] → 验证计划中单列「本地 Windows 留证」，并在最终验收中如实记录 CI 未覆盖的部分。
9. [新增 `tracing`/`win32job`/`nix` 依赖] → 三者均为 MIT/Apache-2.0；`cargo-deny` 的 `deps`/`advisories` 与 `gitleaks` 本地无等价物，只在 CI 判定，不得声称本地已通过。
10. [11 种 `session/update` 的领域投影可能与 core/Sync 既有 event 约定不完全对齐] → 本变更只保证结构化、不文本化、带 `AcpRaw`，并把字段级 schema 一致性留给 Sync 切片；若发现 core 现有 `EventType`/view 约定有缺口，在本变更内**不改 core**，按 `local-agent-host` 规范以「最小结构化 view + 逐字节原文」表达并记录。
11. [每会话一个 supervisor/endpoint 与空闲回收交互复杂] → 用单所有者（supervisor）串行化状态变更，空闲判定与关闭同在 supervisor 内，避免多任务竞争同一会话映射。

## Migration Plan

- **无数据迁移、无 wire 变更**：不改 `schemas/`、`fixtures/`、`compatibility/`、`storage-sqlite` 表结构或 `core` 端口签名。
- **引入顺序（必须在同一交付单元内到位）**：`Cargo.toml` 的 `members` 与依赖口径 → `MODULE_ARCHITECTURE.md` §5 矩阵与 §3/§3.1/§4.2/§4.5 → 两个 crate 的代码与测试。`check:boundaries` 以文档 §5 为判据，成员先于矩阵落地会让 `npm run check` 在中间状态为红（允许，最终必须绿）。
- **锁文件**：`Cargo.lock` 在本变更内一次性更新；本变更之后的并行轨道不得再 `cargo update`。
- **回滚**：`git revert` 该交付单元即可——没有持久化状态、没有 wire 兼容承诺、没有迁移步骤。
- **兼容性**：不改变任何既有 crate 的行为；`core`、`storage-sqlite`、两个协议 crate 的代码与测试保持原样。
