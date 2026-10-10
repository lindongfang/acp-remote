## Why

`docs/DEVELOPMENT_PLAN.md` 的实施切片 2（行 30–32）还没有任何实现：workspace 里既没有 `acp-protocol`，也没有 `agent-host`，因此 `core` 已冻结的 `AgentCatalog` / `SessionBackendFactory` / `SessionEndpoint` 端口在本机侧**没有任何实现**，Daemon（切片 4）无从启动 Agent，Node Link（切片 5–6）与 `acp_facade`（切片 6）也没有可托管的本地 Agent 作为被导出资源。

同时，`docs/ACP_COMPATIBILITY_MATRIX.md` §5 的第一阶段门槛（fake Agent 端到端 turn、全部 11 种 `session/update` 的解码与 raw 保真、`_meta`/未知字段逐字节保真、capability 协商真实性）在当前基线上一条也无法被满足或验证——矩阵与 `fixtures/acp/v1/` 目前只有 JavaScript 侧的结构校验，没有 Rust 侧的消费方。本变更把「ACP wire 合同」与「本地 Agent backend」作为第一个纵向实现切片落地，为后续所有 Agent 相关契约测试提供基座。

## What Changes

- 新增 crate `acp-protocol`（加入 workspace `members`）：JSON-RPC 信封与消息分类、ACP v1 wire DTO、原始消息（`RawDocument`）承载、capability wire 形状、固定 v1 上限，以及由 `fixtures/acp/v1/manifest.json` 与 `compatibility/acp/v1/matrix.json` 驱动的解码/编码与 raw 往返保真测试。它不依赖 `core`，不启动子进程，不管理会话。
- 新增 crate `agent-host`（加入 workspace `members`）：把一个本机 ACP 子进程实现为 `AgentCatalog + SessionBackendFactory + SessionEndpoint`。范围包括启动与监督、stdin/stdout JSON-RPC 分帧、request id / ACP session id / live endpoint generation 映射、capability negotiation、stderr 有界采集与结构化计数（内容不进日志，按上限回取）、超时与取消、异常退出与乱序响应处理、Windows Job Object（Unix 侧等价机制）进程树清理、来自 `LocalConfigStore` 的 profile registry、经 `CredentialResolver` 的启动前凭据解析与白名单交集注入，以及把 `acp-protocol::RawDocument` 与公共领域 view **同时**交给 core 的 wire→core mapper。
- 同步合同与门禁：`docs/MODULE_ARCHITECTURE.md` §3/§3.1/§4.2/§4.5/§5（依赖矩阵新增 `acp-protocol`、`agent-host` 两行两列）、`scripts/check-crate-boundaries.mjs` 的矩阵与其叶子 crate 约束、`README.md` 的「仓库当前状态」表、`docs/DEVELOPMENT_PLAN.md` §2 的基线段、`AGENTS.md` §4 的已落地/待落地状态；若 Job Object wrapper 选型需要新 ADR，则同时新增 `docs/adr/`。
- **不修改**任何 wire 合同：`schemas/`、`fixtures/`、`compatibility/` 的矩阵条目与封闭词表保持不变；本变更只新增**消费**同一矩阵与 fixture 的 Rust 测试证据（矩阵 §4 要求「Rust 测试读取同一矩阵或引用相同 row/test id」）。
- 本次**不包含**：`server::acp_facade`、`server::node_link`、`node-link-client`、`server::sync`、Daemon/CLI 接线与前端工程（切片 4–7）；真实 Codex/OMP 兼容报告（发布前产物，不是普通 CI 硬依赖）；ACP 矩阵中任何 `delivery = post_mvp` 的行的实现。

## Capabilities

### New Capabilities

- `acp-wire-protocol`: `acp-protocol` crate 的 ACP v1 wire 合同——JSON-RPC 信封与消息分类、ACP wire DTO、原始/扩展 payload 的原文承载（`RawDocument`）、raw document 字节保真、capability wire 形状、固定 v1 上限，以及解码/编码失败的分类与显式不支持语义。
- `local-agent-host`: `agent-host` crate 把本机 ACP 子进程托管为 core 端口的实现——启动与监督、stdio JSON-RPC 分帧、request id / ACP session id / live endpoint generation 映射、capability negotiation、stderr 有界采集与结构化计数（内容不进日志，按上限回取）、超时/取消/异常退出处理、Windows Job Object（Unix 等价）进程树清理、来自 `LocalConfigStore` 的 profile registry、经 `CredentialResolver` 的凭据注入边界，以及把 raw 与领域 view 同时交给 core 的 wire→core 映射。

### Modified Capabilities

无。`local-agent-config` 与 `workspace-resolution` 的**需求**不变：本变更只**消费**它们已冻结的端口与输入（`LocalConfigStore`、`CredentialResolver`、`ResolvedWorkspace`），不新增、修改或删除其行为要求；Agent profile 的写入校验、凭据解析边界与路径归属仍以既有规范为准。

## Impact

- **新增 crate**：`crates/acp-protocol/`、`crates/agent-host/`；`Cargo.toml` 的 `members`、`[workspace.dependencies]`（可能新增一个 Job Object / 进程组 wrapper 依赖，需通过许可证与 MSRV 核验）。
- **合同门禁**：`scripts/check-crate-boundaries.mjs`（§5 矩阵新增两行两列；`acp-protocol` 作为纯叶子 crate 的依赖约束）。
- **文档**：`docs/MODULE_ARCHITECTURE.md`（§3、§3.1、§4.2、§4.5、§5）、`README.md`（「仓库当前状态」表）、`docs/DEVELOPMENT_PLAN.md`（§2 基线段）、`AGENTS.md`（§4 模块状态表）；如 Job Object 选型需要 ADR，另加 `docs/adr/`。
- **不涉及**：`schemas/`、`fixtures/`、`compatibility/` 的既有资产内容、`core` 的端口签名与值对象、`storage-sqlite`、Sync/Node Link wire（本变更只新增消费既有 ACP 矩阵与 fixture 的 Rust 测试）。
- **运行环境**：Windows 上新增进程树清理回归测试（Job Object 句柄语义）；Linux CI 会额外执行 `#[cfg(unix)]` 的进程组等价路径。
