## Why

`docs/DEVELOPMENT_PLAN.md` 的实施切片 2（行 30–32）还没有任何实现：workspace 里既没有 `acp-protocol`，也没有 `agent-host`，因此 `core` 已冻结的 `AgentCatalog` / `SessionBackendFactory` / `SessionEndpoint` 端口在本机侧**没有任何实现**，Daemon（切片 4）无从启动 Agent，Node Link（切片 5–6）与 `acp_facade`（切片 6）也没有可托管的本地 Agent 作为被导出资源。

同时，`docs/ACP_COMPATIBILITY_MATRIX.md` §5 的第一阶段门槛（fake Agent 端到端 turn、全部 11 种 `session/update` 的解码与 raw 保真、`_meta`/未知字段逐字节保真、capability 协商真实性）在当前基线上一条也无法被满足或验证——矩阵与 `fixtures/acp/v1/` 目前只有 JavaScript 侧的结构校验，没有 Rust 侧的消费方。本变更把「ACP wire 合同」与「本地 Agent backend」作为第一个纵向实现切片落地，为后续所有 Agent 相关契约测试提供基座。

## What Changes

- 新增 crate `acp-protocol`（加入 workspace `members`）：JSON-RPC 信封与消息分类、ACP v1 wire DTO、原始消息（`RawDocument`）承载、capability wire 形状、固定 v1 上限，以及由 `fixtures/acp/v1/manifest.json` 与 `compatibility/acp/v1/matrix.json` 驱动的解码/编码与 raw 往返保真测试。它不依赖 `core`，不启动子进程，不管理会话。
- 新增 crate `agent-host`（加入 workspace `members`）：把一个本机 ACP 子进程实现为 `AgentCatalog + SessionBackendFactory + SessionEndpoint`。范围包括启动与监督、stdin/stdout JSON-RPC 分帧、request id / ACP session id / live endpoint generation 映射、capability negotiation、stderr 有界脱敏采集、超时与取消、异常退出与乱序响应处理、Windows Job Object（Unix 侧等价机制）进程树清理、来自 `LocalConfigStore` 的 profile registry、经 `CredentialResolver` 的启动前凭据解析与白名单交集注入，以及把 `acp-protocol::RawDocument` 与公共领域 view **同时**交给 core 的 wire→core mapper。
- 同步合同与门禁：`docs/MODULE_ARCHITECTURE.md` §3/§3.1/§4.2/§4.5/§5（依赖矩阵新增 `acp-protocol`、`agent-host` 两行两列）、`scripts/check-crate-boundaries.mjs` 的矩阵与其叶子 crate 约束、`README.md` 的「仓库当前状态」表、`docs/DEVELOPMENT_PLAN.md` §2 的基线段、`AGENTS.md` §4 的已落地/待落地状态；若 Job Object wrapper 选型需要新 ADR，则同时新增 `docs/adr/`。
- **不修改**任何 wire 合同：`schemas/`、`fixtures/`、`compatibility/` 的矩阵条目与封闭词表保持不变；本变更只新增**消费**同一矩阵与 fixture 的 Rust 测试证据（矩阵 §4 要求「Rust 测试读取同一矩阵或引用相同 row/test id」）。
- 本次**不包含**：`server::acp_facade`、`server::node_link`、`node-link-client`、`server::sync`、Daemon/CLI 接线与前端工程（切片 4–7）；真实 Codex/OMP 兼容报告（发布前产物，不是普通 CI 硬依赖）；ACP 矩阵中任何 `delivery = post_mvp` 的行的实现。

## Intent and Constraints

```agentic-intent
sources:
  - "用户在 2026-09-24 的本会话中只给出一条上下文引用 `docs/DEVELOPMENT_PLAN.md#L30:32`，指向 `## 3. 实施切片` 的 `### 2. ACP 边界与本地 Agent` 及其验收段（该文件行 30–32 原文：「实现 `acp-protocol` 的 wire DTO、原始消息保真和能力映射，再由 `agent-host` 接管本地 ACP 子进程、stdio JSON-RPC、会话映射、超时、取消与退出处理。…」/「验收：fake ACP Agent 能驱动一个完整 turn；未知字段与扩展 payload 往返保真，结构化事件不被文本化；启动失败、乱序响应、异常退出和取消有明确结果。Windows Job Object 的父子孙进程清理及关闭句柄清理成为常驻回归测试。实现或声明的能力与 ACP 兼容性矩阵一致。」），未附加自由文字。"
  - "同会话用户原话：「1. 同意」——对「把切片 2 作为**一个** OpenSpec 变更（新增 `acp-protocol` + `agent-host` 两个 crate），而不是拆成两个变更」的批准。"
  - "同会话用户原话：「2. 同意」——对「本变更 Main E2E 记 `not-applicable`，替代验证为 `cargo test --locked -p acp-protocol -p agent-host --all-features` + `npm run check`（ACP 矩阵/fixture/schema 门禁）+ `npm run verify`」的批准；该批准按 `openspec/config.yaml` 的 context 要求写入 `plan.md` 的 `downgrade_approval`，不沿用 2026-09-23 那次。"
  - "`docs/DEVELOPMENT_PLAN.md` §4「切片交付规则」：每个实施切片按 OpenSpec agentic 流程建立变更；改 wire/封闭词表/端口/存储/能力支持状态时在同一变更中维护权威文档、schema、fixture、矩阵与合同门禁；crate 依赖遵循 `MODULE_ARCHITECTURE.md` §5。"
constraints:
  - "依赖方向：`acp-protocol` 不依赖 `core`、不依赖其他协议 crate、不启动子进程/不访问数据库（`MODULE_ARCHITECTURE.md` §4.2/§5）；`agent-host` 只允许依赖 `core` 与 `acp-protocol`（§5 矩阵），平台差异（Job Object / 进程组）只存在于本 crate。"
  - "ACP 兼容性不变量：未知字段、`_meta`、未来协议字段与 Agent 扩展 payload 必须逐字节往返保真；结构化事件不得被文本化；无法安全处理的能力必须显式拒绝或显式降级，不得静默忽略（`AGENTS.md` §3、`ACP_COMPATIBILITY_MATRIX.md` §5 第 5 条）。"
  - "ACP DTO 只存在于 ACP 边界；`core` 仍然是 `AgentCatalog`/`SessionBackendFactory`/`SessionEndpoint` 端口的唯一定义者，mapper 位于 `agent-host`（`AGENTS.md` §4、`MODULE_ARCHITECTURE.md` §4.2/§4.5）。"
  - "进程与资源上限是固定 v1 常量而非配置键：`initialize` 前 10 s 启动超时、短请求 30 s、`session/prompt` 不设超时、关闭 grace 5 s、单条 ACP 消息 1 MiB、stderr 环形缓冲 256 KiB、空闲回收复用 `sessions.idle_timeout_ms`（`SECURITY_DESIGN.md` §12.2 表）。实现不得自行发明未列出的数值。"
  - "Agent 子进程：参数数组启动、不经 shell；只注入 `env_allowlist ∩ profile 的 env 绑定` 加必要进程环境；凭据只来自 `CredentialResolver`，未绑定/未列入白名单/keystore 不可用一律失败关闭；Node/Device 密钥绝不注入子进程（`SECURITY_DESIGN.md` §12.2、`AGENTS.md` §3）。"
  - "`agent-host` 不读启动配置文件取 profile：profile 来自 `LocalConfigStore` 端口（`MODULE_ARCHITECTURE.md` §4.5）；workspace 只接收 core 解析后的 `ResolvedWorkspace`，后端不得自行查存储或拼路径（`workspace-resolution` 既有规范）。"
  - "Rust 约定：正常路径不得 `unwrap()`/`expect()`/无说明 panic；异步任务必须有所有者与取消路径；`unsafe_code = \"forbid\"` 保持全局，正式实现不得直接 FFI `kernel32`（`AGENTS.md` §7、`MODULE_ARCHITECTURE.md` §4.5）。"
  - "新增第三方依赖按 `AGENTS.md` §7 先核必要性、维护状态、许可证与平台支持；workspace `rust-version = 1.85` 是当前基线，抬高它需要用户决策（`AGENTS.md` §12 把 MSRV 与工具链版本分开）。"
  - "本地入口 `npm run verify`；`cargo-deny`/`gitleaks` 只在 CI 运行，本地没有等价物，不得声称已在本地通过（`AGENTS.md` §8）。"
non_goals:
  - "不实现切片 3–7：`identity-auth`/`identity-keystore`、`server::local_admin`+Daemon/CLI、`server::node_link`、`node-link-client`+`server::acp_facade`、`server::sync`+Web/PWA。"
  - "不实现矩阵中任何 `delivery = post_mvp` 的行（`session/load`、`session/list`、`session/delete`、`session/resume`、`session/close`、`fs/*`、agent→client 的 `terminal/*` 服务方法等）；它们必须显式不支持，且（本变更尚未涉及 facade）不产生任何「已可用」宣告。"
  - "不修改 `schemas/`、`fixtures/`、`compatibility/` 的既有合同资产与固定上游快照；不新增或删除矩阵取值、错误码、feature ID、命令名。"
  - "不产出真实 Codex/OMP 兼容报告，也不把真实 Agent 作为普通测试的硬依赖（`ACP_COMPATIBILITY_MATRIX.md` §7）。"
  - "不引入 Noise、Tailscale SDK、云中继、账号中心或任何网络传输实现：本 crate 只托管本机子进程。"
success_criteria:
  - "fake ACP Agent（可控子进程）能驱动一个完整 turn：`initialize` → `session/new` → `session/prompt` → 多个 `session/update` → turn 终态，并按 `SessionEndpoint` 语义把事件交给 core 的 sink。"
  - "矩阵 §5 第 2、5 条的 Rust 侧证据成立：全部 11 种 `session/update` discriminator 可解码，未知 discriminator 走可见降级并保留 `acp.rawJson`（`invariant.future_update_visible`），`_meta` 与未知字段逐字节保真（`invariant.meta_fields_byte_exact`、`fixtures/acp/v1/meta-and-unknown-fields.json`），下划线扩展方法显式不支持。"
  - "失败路径有明确结果与测试：启动失败、短请求超时、取消、异常退出、乱序/非法 JSON 响应都不产生伪造成功，也不遗留 detached task 或孤儿进程。"
  - "Windows 上「父→孙」两层进程树清理（杀父后孙停止、关 Job 句柄后孙停止）成为常驻回归测试；Unix 侧用进程组或等价机制覆盖同一性质。"
  - "capability 协商真实：Agent 未宣告的可选能力不被调用；链路不完整时不向上游虚报（`truthful_negotiation`）。"
  - "`npm run check`（含 `check:acp`、`check:boundaries`、`check:schema-fixtures`）与 `npm run verify` 全绿，新增 crate 的测试确实被执行（无零用例、无全跳过）。"
decision_bounds:
  - "Agent 可自主决定：`acp-protocol` 的内部模块划分与 DTO 组织、`RawDocument` 的承载方式、`agent-host` 的任务结构/状态机/错误类型、fake ACP child 的测试写法、Job Object wrapper crate 的选型（前提是维持 MSRV 1.85、`unsafe_code = \"forbid\"` 与 `AGENTS.md` §7 的许可证/维护性要求）。"
  - "需要用户决策：抬高 workspace `rust-version`/工具链基线；新增 ADR 或为某个 crate 覆盖 unsafe lint；改动 `core` 端口签名、依赖方向、wire 合同、矩阵取值或封闭词表。"
  - "用户已批准：本变更范围粒度（切片 2 作为一个变更）与 Main E2E 记 `not-applicable` 及其替代验证清单；不再重复求批准。"
assumptions:
  - "`acp-protocol` 可以只用 `serde`/`serde_json`/`thiserror`（必要时加 `acpr-wire`）表达 ACP DTO 而不依赖 `core`；若某处确需 core 值对象，则把该映射下沉到 `agent-host` 的 mapper（不改变成功判据）。"
  - "`fixtures/acp/v1/` 现有 6 个消息夹具与 2 个片段夹具足以支撑解码/raw 保真/可见降级/显式不支持的 Rust 契约测试；若发现缺口，优先用**既有**夹具表达，确需新增夹具时按 `AGENTS.md` §10 同步 `fixtures/acp/v1/manifest.json` 与矩阵（已登记于本变更的合同同步范围）。"
  - "Windows Job Object 的安全 wrapper 候选（如 `win32job`）是否满足 MSRV 1.85 / 许可证 / 维护要求在计划阶段尚未实证；design 给出候选对比与推荐，实现阶段若无一可接受，则按 `MODULE_ARCHITECTURE.md` §4.5 的退路（新增 ADR + 为该 crate 覆盖 lint）**回到用户决策**，不擅自放开 `unsafe`。"
  - "本机可用 Rust 测试二进制充当 fake ACP child（`cargo test` 自身的测试二进制或独立示例），无需真实 Agent 或额外运行时；尚未在 Windows 上跑通该形态，属实现阶段首个要验证的前置。"
```

## Capabilities

### New Capabilities

- `acp-wire-protocol`: `acp-protocol` crate 的 ACP v1 wire 合同——JSON-RPC 信封与消息分类、ACP wire DTO、原始/扩展 payload 的原文承载（`RawDocument`）、raw document 字节保真、capability wire 形状、固定 v1 上限，以及解码/编码失败的分类与显式不支持语义。
- `local-agent-host`: `agent-host` crate 把本机 ACP 子进程托管为 core 端口的实现——启动与监督、stdio JSON-RPC 分帧、request id / ACP session id / live endpoint generation 映射、capability negotiation、stderr 有界脱敏采集、超时/取消/异常退出处理、Windows Job Object（Unix 等价）进程树清理、来自 `LocalConfigStore` 的 profile registry、经 `CredentialResolver` 的凭据注入边界，以及把 raw 与领域 view 同时交给 core 的 wire→core 映射。

### Modified Capabilities

无。`local-agent-config` 与 `workspace-resolution` 的**需求**不变：本变更只**消费**它们已冻结的端口与输入（`LocalConfigStore`、`CredentialResolver`、`ResolvedWorkspace`），不新增、修改或删除其行为要求；Agent profile 的写入校验、凭据解析边界与路径归属仍以既有规范为准。

## Impact

- **新增 crate**：`crates/acp-protocol/`、`crates/agent-host/`；`Cargo.toml` 的 `members`、`[workspace.dependencies]`（可能新增一个 Job Object / 进程组 wrapper 依赖，需通过许可证与 MSRV 核验）。
- **合同门禁**：`scripts/check-crate-boundaries.mjs`（§5 矩阵新增两行两列；`acp-protocol` 作为纯叶子 crate 的依赖约束）。
- **文档**：`docs/MODULE_ARCHITECTURE.md`（§3、§3.1、§4.2、§4.5、§5）、`README.md`（「仓库当前状态」表）、`docs/DEVELOPMENT_PLAN.md`（§2 基线段）、`AGENTS.md`（§4 模块状态表）；如 Job Object 选型需要 ADR，另加 `docs/adr/`。
- **不涉及**：`schemas/`、`fixtures/`、`compatibility/` 的既有资产内容、`core` 的端口签名与值对象、`storage-sqlite`、Sync/Node Link wire（本变更只新增消费既有 ACP 矩阵与 fixture 的 Rust 测试）。
- **运行环境**：Windows 上新增进程树清理回归测试（Job Object 句柄语义）；Linux CI 会额外执行 `#[cfg(unix)]` 的进程组等价路径。
