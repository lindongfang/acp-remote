<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

Owner 节点上曾经由 acp-remote 创建过的会话，在 daemon 重启或 Agent 进程退出之后无法继续：`agent-host::open()` 只能重新绑定**还活着**的进程内会话（`crates/agent-host/src/host.rs` 在 `supervisor.is_running()` 为假时返回 `SessionClosed`），而恢复真正需要的两样数据从未落盘——Agent 侧 `sessionId` 只存在 `agent-host` 的内存映射里，创建时的解析 `cwd` 完全没有持久化。因此即便目标 Agent 宣告了 `sessionCapabilities.resume`，acp-remote 也无从发起恢复。本次按 `docs/SESSION_CONTINUITY_DESIGN.md` §7 已定决策 **B2（ACP `session/resume`）** 落地「远程真恢复」的 Owner/Daemon 侧纵向切片。

## What Changes

- `acp-protocol`：新增 `session/resume` 请求（required `sessionId`、`cwd`）与响应的类型化 DTO 及解码/编码，保持未知字段逐字节保真；`session/load` 保持既有「显式不支持」。
- `core`：新增 `AgentSessionId` 值对象与 `ResumeSessionRequest`；`SessionBackendFactory` 新增 `resume`；新增 `resume_session` 用例与 `session.resume` 命令路由（授权复用 `grant.remote-work`，幂等沿用 `requestId`，崩溃窗口进 `uncertain`）；owned 会话记录携带 Agent 侧会话标识与创建时 cwd。
- `storage-sqlite`：owned 家族表结构版本与文件格式版本升到 v5，`owned_session` 追加 `agent_session_id`、`workspace_cwd`（可空、只做追加、不触发表重建）；升级幂等、字节稳定与旧行保留测试。
- `agent-host`：创建成功后向 core 暴露本次 ACP 会话标识与生效的规范化目录；新增「按持久化 agent 标识重新拉起进程 + `session/resume`」路径，并按协商到的 `sessionCapabilities.resume` 显式门控。
- `workspace-resolution`：恢复前对持久化的创建时 cwd 执行与创建时同口径的复校验，失败返回服务端不可用类错误，不回退到别名重解析。
- `node-link-protocol` / `server::node_link` / `app` / `compatibility/commands/v1/commands.json`：新增 `session.resume` 命令（mutation、`node_link` 传输、`grant.remote-work`）的 wire 变体、授权、幂等与终态，并接线 Owner 侧路由与组合根。
- 机器资产与权威文档：`compatibility/acp/v1/matrix.json` 提升 `session/resume` 的 delivery 与各层状态；`schemas/node-link/v1/`、`fixtures/node-link/v1/` 同步；`docs/NODE_LINK_PROTOCOL.md`、`docs/ACP_COMPATIBILITY_MATRIX.md`、`docs/CORE_PORTS_AND_STORAGE.md`、`docs/SESSION_CONTINUITY_DESIGN.md`（状态注记）与 `README.md`/`docs/DEVELOPMENT_PLAN.md` 同步。
- **本次不包含**：能力 A（远程看历史，Owner 侧已具备、缺 Access 侧 `node-link-client`）、能力 C（新会话灌历史）、能力 D 的代码改动（模型已支持，建两条 Export 即可）、D+（跨 Agent 历史移交）。

## Capabilities

### New Capabilities

无新增能力：本次是既有边界内的需求变化（协议 wire、后端、存储升级、解析校验与 Owner 服务端），不引入新能力域。

### Modified Capabilities

- `acp-wire-protocol`: 新增 `session/resume` 请求/响应的类型化解码与编码，未知字段保真；`session/load` 处理不变。
- `local-agent-host`: 会话创建时暴露 ACP 会话标识与生效目录；新增「进程不在时按 profile 重新拉起并 `session/resume`」的恢复路径与能力门控。
- `storage-schema-v2-migration`: owned 家族与文件格式版本升到 v5，`owned_session` 追加恢复所需的两个可空列。
- `workspace-resolution`: 新增恢复前对持久化创建时目录的同口径复校验与失败分类。
- `node-link-owner-server`: 命令集合从 12 个扩到 13 个并新增 `session.resume` 的授权、幂等、终态与 payload/结果契约。

## Impact

- **代码**：`crates/acp-protocol`（DTO 与解码）、`crates/core`（值对象、端口、用例、命令路由与持久化提交形状）、`crates/storage-sqlite`（v5 migration 与读写路径）、`crates/agent-host`（创建回填与恢复路径）、`crates/node-link-protocol`（命令变体与载荷/结果）、`crates/identity-auth`（`GRANTS` 会员与其验收测试）、`crates/server`（`node_link` 路由）、`crates/app`（组合根接线）。
- **机器合同与固定向量**：`compatibility/acp/v1/matrix.json`（`method.session_resume`、`cap.agent.session_resume`）、`compatibility/commands/v1/commands.json`（新增 `session.resume`）、`schemas/node-link/v1/`、`fixtures/node-link/v1/`。不新增错误码、不新增 feature ID，因此 `compatibility/errors/v1/errors.json` 与 `compatibility/features/v1/features.json` 不变。
- **权威文档**：`docs/NODE_LINK_PROTOCOL.md`（§12.5/§12.7 命令表、§11.3 feature、§15 顺序与重放）、`docs/ACP_COMPATIBILITY_MATRIX.md`（§3.3/§3.4 layers 与 delivery、§6 facade 映射、§7 Agent 差异）、`docs/CORE_PORTS_AND_STORAGE.md`（§2 错误枚举、§3.1 值对象、§5.1 会话后端端口、§5.2 提交形状、§7 标题/§7.2 版本常量/§7.3 `owned_session` DDL）、`docs/SYNC_PROTOCOL.md`（§11.5 命令表与计数文本）、`docs/SECURITY_DESIGN.md`（§10.2 命令、scope、pack 与 grant 表格）、`docs/SESSION_CONTINUITY_DESIGN.md`（实现状态注记）、`README.md`（仓库当前状态、合同检查②）、`docs/DEVELOPMENT_PLAN.md`（切片状态）；另因门禁规则调整，同步 `AGENTS.md` §10 与本段所属的四处说明。
- **合同门禁脚本**：`scripts/check-command-catalog.mjs` —— 把 `pack: null` 豁免的判据从「按命令名（仅 `session.create`）」改为「按 transport 不含 `sync`」，使 Node Link-only 命令可合法声明无 pack；该改动不新增错误码或 feature ID，但按 `AGENTS.md` §10 必须同步门禁说明四处。
- **存储与数据**：单次 `ALTER TABLE ... ADD COLUMN` 追加两个可空列，不做 12-step 表重建；旧行两列为 NULL，行为按「不可恢复」处理。
- **依赖**：不新增任何 crate 或第三方依赖。
- **安全**：不改变威胁模型与信任边界；不新增、不重命名任何授权词（`grant.*` / `pack.*` / `preset.*` 的名称集合不变），仅 `grant.remote-work` 的**成员集合增加一条命令**（`session.resume`），这必须同批同步 `crates/identity-auth` 的 `GRANTS` 镜像与其既有验收测试。`local.*` 面不变。
