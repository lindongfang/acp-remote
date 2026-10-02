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

## Intent and Constraints

```agentic-intent
sources:
  - "docs/SESSION_CONTINUITY_DESIGN.md（2026-09-29，头部状态：候选设计（未实现）……待实施；§12.1 七条决策已定；§12.2 无待决）——本次变更的实施输入"
  - "用户 2026-09-29 本会话原话：「那我们不考虑 C 灌历史 这种情况」——本次范围明确排除能力 C"
  - "用户 2026-09-29 本会话行为：在 /opsx-apply 无活动变更时提供 docs/SESSION_CONTINUITY_DESIGN.md 作为要实施的对象，并在 A/C/D 影响面澄清后确认只做 B"
  - "docs/SESSION_CONTINUITY_DESIGN.md §7 决策表：B2 = ACP session/resume（采用）；B1 = session/load（不采用）；B3 = Agent 私有 CLI 参数（不采用）"
constraints:
  - "触发通道固定 B2（ACP session/resume）；不实现 session/load，不使用 Agent 私有 CLI 参数，不在核心按 Agent 名称分支"
  - "远程恢复授权复用 grant.remote-work，不新增授权维度；授权必须先于任何本机读取与文件系统访问（沿用 session.create 的现口径，防会话存在性探测）"
  - "能力诚实：未宣告 sessionCapabilities.resume 的 Agent MUST 显式返回不支持，MUST NOT 静默降级为新建会话（C），也 MUST NOT 发送 session/resume，并 MUST 在返回不支持错误前终止并回收本次为恢复而拉起的子进程（DR1-F31：原措辞「MUST NOT 为此启动 Agent 进程」在能力只能经 initialize 得知的前提下物理不可满足）"
  - "必须持久化创建时解析出的真实 cwd 与 Agent 侧 sessionId；恢复时 MUST NOT 按 alias 重新解析，因为 alias 之后可能指向别处"
  - "继续遵守最高优先级不变量：状态与事件先持久化成功再广播；一个状态只有一个权威写入者；同一会话最多一个 active turn；可重试 mutation 携带稳定 requestId，崩溃窗口进 uncertain"
  - "依赖方向与 crate 边界以 docs/MODULE_ARCHITECTURE.md §5 为准；协议合同变更必须同步 schemas/、fixtures/、compatibility/ 与权威文档，并让 npm run check 全绿"
non_goals:
  - "不实现能力 A（远程看历史）：session.list/session.read 与 Owner 侧路由已存在，缺的 Access 侧 node-link-client 与 server::sync 属开发计划切片 6/7"
  - "不实现能力 C（新会话 + 灌历史）以及任何形式的自动降级"
  - "不做能力 D 的代码改动（§6.4 结论：当前模型即可表达），也不做 D+ 跨 Agent 历史移交"
  - "不实现其它 post_mvp 的 ACP 会话方法（session/load、session/list、session/delete、session/close）"
  - "不读取、不解析 Agent 私有原生会话存储格式"
  - "不引入绕过 Export、配对与 grant 的恢复捷径"
success_criteria:
  - "已持久化 agent 侧 sessionId 与创建时 cwd 的 owned 会话，在 daemon 重启、Agent 进程不在时，可经 Owner 侧 session.resume 重新拉起 Agent 并恢复该会话，随后可继续 prompt"
  - "Agent 未宣告 sessionCapabilities.resume 时返回明确的不支持错误、未发送 session/resume、本次拉起的子进程已回收，会话状态不变，且不降级为新建会话"
  - "同一 requestId 重复提交 session.resume 不产生第二次进程或第二次会话绑定；无法确认副作用的崩溃窗口进入 uncertain 终态"
  - "恢复前 cwd 复校验失败（目录被删、被改成文件、规范化结果不同）时返回服务端不可用类错误，不回退到别名重新解析或别的目录"
  - "旧库（v1–v4）可升级到 v5；连续两次打开后 user_version、两族版本键与库内每条 DDL 文本逐字节相同；npm run check 全绿"
decision_bounds:
  - "可自主决定：列名与内部类型命名、实现顺序与拆包、测试组织与用例命名、失败映射到 compatibility/errors/v1/errors.json 中已有错误码的选择"
  - "需要用户决策：新增或重命名封闭词表条目（命令名、错误码、feature ID）、放宽 export.create 粒度、把 B 的降级语义改成允许自动 C、修改 docs/SESSION_CONTINUITY_DESIGN.md §12.1 的已定决策"
assumptions:
  - "假设目标 Agent 的 ACP 实现会如实宣告 sessionCapabilities.resume。文档 §10 要求的只读侦察尚无真实 Agent 实例可执行；未执行不阻塞本切片（门控按真实能力工作），但 B 在该 Agent 上是否可用属运行时前提。"
  - "假设 session.create 是当前唯一产生 owned 会话的入口（CLI 首切片不提供会话创建，见 cli-commands），因此 v5 之前的历史行两个新列均为 NULL；恢复对这类行显式返回不支持（nodelink.command.unsupported），而不是猜测标识或目录。"
```

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
