# 会话延续设计：历史查看、恢复、灌历史与同目录换 Agent（候选）

> 状态：**B（恢复会话）已在 OpenSpec 变更 `session-resume` 中实施到 Owner 侧 Node Link 路由与本地 Agent 主机（2026-10-01 收口）**；A、C、D 的分析与结论不变，仍按 §6 的「当前可做性」栏陈述。设计决策已于 2026-09-29 全部定稿，实施后的实际形状以 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §12.7、[CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §3.6/§5.1/§7.3 与 [ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) 为准。
> 日期：2026-09-29（状态注记 2026-10-01）
> 用途：作为后续 OpenSpec 变更的输入；实施时必须按 `AGENTS.md` §8 同步对应的权威文档、schema、fixture 与命令目录。
> 非权威声明：产品行为以 [INITIAL_DESIGN.md](./INITIAL_DESIGN.md) 为准，节点协议以 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) 为准，ACP 覆盖状态以机器矩阵 `compatibility/acp/v1/matrix.json` 为准。本文出现冲突时，一律以上述权威来源为准。

## 1. 场景与问题

两个目标场景：

**场景 1（历史与恢复）**：Owner 节点运行 acp-remote，本地曾经启动过 Codex / Pi 会话；现在只运行 acp-remote，Agent 进程不在。希望**从远程**「打开历史的 Codex 对话」。

**场景 2（同目录换 Agent）**：在目录 A 下曾用 acp-remote 打开过 Codex；现在希望**远程用 Pi 打开同一个目录 A 并操作**。

这两个场景不能笼统回答「支持 / 不支持」，必须拆成四种语义不同的能力。

## 2. 四种能力（A / B / C / D）

| 编号 | 名称 | 用户看到的行为 | 本质 |
|---|---|---|---|
| **A** | 看历史 | 远程列出会话并重放过去的对话记录 | 读 acp-remote 自己的事件日志 |
| **B** | 恢复会话（真恢复） | 重新拉起该 Agent，带着**原来的会话上下文**继续对话 | 让 Agent 加载它自己的会话状态 |
| **C** | 新会话 + 灌历史 | 开一个全新会话，把旧对话作为参考材料喂进去 | 用可见对话流重新拼一个上下文 |
| **D** | 同目录换 Agent | 在**同一个目录**用**另一个 Agent**开新会话并操作 | 普通的新会话创建，只是换了 `agentId` |

用户已确认：**A、B、C、D 四者都要支持。**

> D 与 A/B/C 是正交的：D 不涉及「恢复」，它复用的是**目录**而不是**对话**。

## 3. 三类状态的层次

要判断可行性，先分清「状态」在哪一层：

| 层 | 内容 | 谁持有 | 可否重建 |
|---|---|---|---|
| 工作目录 | 项目文件、代码 | 文件系统 | 可以（登记 workspace alias 即可） |
| acp-remote 事件日志 | acp-remote **观察到**的消息、工具调用、diff、计划等 | Owner 的 SQLite | 部分（是投影，且刻意有损） |
| **Agent 会话状态** | 系统提示、工具定义、内部指令、隐性推理、Agent 自身的上下文压缩结果 | **只有 Agent 自己** | **不可重建** |

**结论：`cwd` + 启动命令只能得到一个空白会话。** 「继续对话」唯一需要的第三层状态只存在 Agent 那里。而 D 只需要第一层，因此它天然可行。

## 4. ACP 实际给客户端什么（证据）

上游固定快照 `schemas/acp/v1/upstream/schema.json` 中，`session/update` 有 11 种判别子：

```
user_message_chunk, agent_message_chunk, agent_thought_chunk,
tool_call, tool_call_update, plan,
available_commands_update, current_mode_update,
config_option_update, session_info_update, usage_update
```

但它是**渲染/进度流**，不是状态快照。快照自己的原文：

- `SessionUpdate`：「These updates provide **real-time feedback about the agent's progress**.」
- `ToolCallUpdate`：「All fields except the tool call ID are **optional** — **only changed fields need to be included**.」

由此得到三条硬约束：

1. **增量且可缺项**：还原完整状态要求「一条不漏、按序」收全所有 delta；断线、重放边界、压缩都会打破该前提。
2. **Agent 的 prompt 上下文从不下发**：系统提示、工具 schema、内部指令、Agent 的上下文压缩结果都不在 wire 上。
3. **决定性证据**：如果「重放更新」就等于「恢复会话」，ACP 根本不需要定义 `session/load` / `session/resume`。协议专门提供这两个方法，说明重放 ≠ 恢复。

> 补充：acp-remote 自身还刻意有损——`storage.persist_deltas`（见 [CONFIG_REFERENCE.md](./CONFIG_REFERENCE.md) §5）允许 turn 结束后压缩短期 delta，事件有保留窗口，终端输出有截断上限。因此从 acp-remote 日志里重放出来的内容比 ACP 当时发出的更少。

## 5. 可行性结论

| 能力 | 靠 ACP 流是否足够 | 还缺什么 | 当前可做性 |
|---|---|---|---|
| **A** 看历史 | 足够 | 客户端 + 远程链路 | 协议层已具备，见 §6.1 |
| **B** 恢复会话 | **不够** | Agent 的 `resume` 能力 + Agent 侧会话 id + `cwd` | 已定采用 B2（`session/resume`），见 §6.2 与 §7 |
| **C** 新会话灌历史 | 足够 | 拼装策略、token 控制 | 可做，但语义有损，见 §6.3 |
| **D** 同目录换 Agent | 足够（本就够） | 放宽 Export 粒度（可选） | **模型已支持**，见 §6.4 |

**B 的恢复能力属于 Agent，不属于 acp-remote。** 这是设计无法绕开的边界；acp-remote 侧能做的是：持久化触发所需的数据、提供触发通道、做授权、并在 Agent 不支持时显式降级。

## 6. 逐能力缺口分析

### 6.1 A：看历史

**已经具备的部分**

- 会话记录与事件日志在 Owner 的 SQLite（`owned_session` / `owned_event`，见 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §7.3）。
- 协议层已有 `session.list` / `session.read`（[compatibility/commands/v1/commands.json](../compatibility/commands/v1/commands.json)，`mvp`，`grant.observe`）。
- Owner 侧 Node Link 入站面已落地（握手、catalog、resource 重放、command）。

**缺口**

- **前置条件**：被看的会话必须是**经 acp-remote 创建**的。终端里直接跑 Agent 产生的原生历史，acp-remote 没有任何代码去读。**已定：不提供这个能力**（见 §12.1 决策 4）。
- **前置条件**：该 Agent 必须属于某个 Export，Access 节点已配对且持有 `grant.observe`（[NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §10）。
- **实现缺口**：Access 侧 `node-link-client` 未落地；`server::sync` 未落地。因此目前**没有可用客户端**能真正发起这些请求。

**结论**：A 不需要新设计，是「把已落地的能力接上客户端」。

### 6.2 B：恢复会话

**已定：采用 B2 —— ACP `session/resume`**（决策见 §7）。选择 B2 的直接收益是：`session/resume` **不重放历史**，因此没有「历史对账」问题（见下）。

**协议层现状**（`compatibility/acp/v1/matrix.json`；下表已于 2026-10-01 按实施后取值刷新）：

| 方法 | delivery | broker / sync / pwa | facade |
|---|---|---|---|
| `session/load` | `post_mvp` | `explicit_unsupported` | `not_advertised` |
| `session/resume` | `conditional_mvp` | `project_and_preserve` / `explicit_unsupported` / `explicit_unsupported` | `not_advertised` |

本次只需要把 `session/resume` 提升；`session/load` 保持 `post_mvp` 不实现。

**必需参数**：`sessionId`（**Agent 侧**的会话 id）与 `cwd`，二者都是 **required**（上游快照 `ResumeSessionRequest`）。因此即便选择了不重放的 resume，下面两项数据仍必须持久化。

**能力门控**：`sessionCapabilities.resume`——`Omitted`/`null` 均表示不支持，`{}` 表示支持（`crates/acp-protocol/src/capability.rs` 的 `supports_session_resume()` 已按此语义实现）。

**acp-remote 侧的数据缺口**（缺口判断已于 2026-09-29 做出；两列的实际落点见下句「已落地」）

| 需要的数据 | 缺口判断时的现状 | 位置 |
|---|---|---|
| Agent 侧 sessionId | **只在内存**（`agent-host` 的 `by_acp` / `by_core` 映射） | 库表无此列 |
| 创建时的 `cwd` | **完全不持久化** | `owned_session` 无 workspace 列 |
| acp-remote 自己的 sessionId | 有 | `owned_session.session_id` |
| agent_id | 有 | `owned_session.agent_id` |

> 已落地（2026-10-01）：`owned_session` 在文件格式 **v5** 追加了可空的 `agent_session_id` 与 `workspace_cwd` 两列（只追加、不推导、不回填；`CORE_PORTS_AND_STORAGE.md` §7.3），由窄读取 `SessionStore::load_recovery` 单独取回，**不**进入 `Session`/`SessionSummary` 的可投影面；两列中任一列为 `NULL` 时该会话不可恢复（走 `nodelink.command.unsupported`）。

> 注意：`cwd` 必须记录**创建时解析出的真实路径**，不能在恢复时重新解析 alias——alias 之后可能被改指向别的目录，而 `session/resume` 要求 cwd 与创建时一致。

**授权**：远程恢复复用 `grant.remote-work`（见 §12.1 决策 3）。未宣告 `sessionCapabilities.resume` 的 Agent 按「显式不支持」返回错误。

**进程模型缺口**

Agent 进程是 daemon 的子进程；daemon 停止后进程即消失。现有 `agent-host::open()` 只能重新绑定**还活着**的进程内会话，进程不在时返回 `SessionClosed`。恢复需要一条全新的「按需重新拉起并恢复」路径：

```
远程 resume
  → 从库读 agent_id + Agent 侧 sessionId + cwd
  → 按 profile 重新 spawn Agent（复用现有 launch / 凭据注入边界）
  → 发 session/resume
  → 会话状态从持久态抬回可交互态，接上 turn 队列
```

**历史对账：不适用**

`session/load` 会把历史作为 `session/update` 重放，从而与 acp-remote 自己的事件日志重复，需要「对账」；**`session/resume` 明确不返回历史消息**（上游描述：*Resumes an existing session without returning previous messages*），因此本方案下不存在重复与对账问题。客户端的历史仍只来自 acp-remote 自己的事件日志（即能力 A）。

**Agent 侧前提（本机之外，需核实）**

- Agent 的 ACP 实现是否宣告 `sessionCapabilities.resume` 并真正实现；
- Agent 的原生会话是否仍然存在（可能被其自身清理/TTL）；
- Agent 是否支持通过 CLI 参数恢复（本次不采用，仅记录）。

能力协商必须为真：未宣告 `sessionCapabilities.resume` 的 Agent **不得**调用该（[ACP 规则](https://agentclientprotocol.com/protocol)）。此时按「显式不支持」返回错误，**不静默降级**（见 §12.1 决策 2）。

### 6.3 C：新会话 + 灌历史

**可做，因为不依赖 Agent 的 load 能力**：直接 `session/new`，再把本地记录的对话作为上下文材料送入。

**代价（必须如实告知用户）**

- Agent 的工具状态、终端状态、待批权限、MCP 连接、模式/模型配置全部丢失；
- Agent 自身的上下文压缩结果无法复现；
- 附件原字节未必完整（acp-remote 只持有有界副本/引用）；
- 每次都要重发历史，token 成本上升；
- 语义上是「新会话」而非「继续」。

**设计要点**：需要一条「带上下文创建会话」的入口（命令名、参数形状、上下文裁剪/上限、来源标注），不能静默伪装成恢复。

### 6.4 D：同一 workspace 换 Agent 开新会话

**场景**：目录 A 曾用 Codex 经 acp-remote 使用过；现在希望远程用 Pi 打开同一个目录 A 并操作。

**结论：这是四种能力里最容易的。它不需要恢复任何东西——本质就是一次普通的会话创建，只是换了 `agentId`。**

映射到现有协议：

```
session.create { exportId, agentId: "pi", workspaceAlias: "A", templateParams }
```

**天然共享的部分：目录本身。** 文件、代码是文件系统里的同一份。Pi 一开就在 A 里，自然能看到 Codex 改过的文件。这是 workspace 语义自带的，**不需要任何额外机制**。

**不会共享的部分：对话上下文。** Pi 不知道 Codex 之前聊过什么。**已定：D+ 不做**（见 §12.1 决策 7）。

**当前模型下的缺口**

| 缺口 | 说明 | 是否阻塞 D |
|---|---|---|
| Export 粒度 = 1 Agent + 1 workspace | `export.create` 首切片强制 `agentIds` / `workspaceAliases` / `templates` **恰好 1 项**（`crates/server/src/local_admin/params.rs`） | **不阻塞**：建两个 Export（Codex+A、Pi+A），两者可共用同一个已登记的 alias |
| 想让一个 Export 同时提供多个 Agent | 放宽 `export.create` 的 1 项约束（值对象与 wire 本就是数组） | **已定：不放宽**（见 §12.1 决策 5），每个 Agent 一条 Export |
| 远程选择 Agent | 客户端（Zed `session/new` 或后续 PWA）要能选 `agentId` / `exportId` | 属客户端与 façade 面 |

也就是说：**D 在当前模型下就能表达**，代价是 Owner 要为本机目录 A 分别导出 Codex 与 Pi 两条 Export。

**需要注意的风险**

- **同目录并发**：同一 workspace 上可以有多个会话同时存在（`AGENTS.md` §3 只约束「同一会话最多一个 active turn；不同会话可并行」）。两个 Agent 同时改 A 里的文件会产生冲突，系统**不阻止**。**已定：不管**，完全交给使用者（见 §12.1 决策 6）。
- 远程提交的仍然只能是 **alias**，不能是路径；A 必须已由本机登记，路径不泄漏。

**D+（跨 Agent 历史移交）：已决定不支持**

如果目标是「Pi 不仅进入 A，还知道 Codex 做过什么」，那是 **C 的跨 Agent 变体**：新会话（Pi）+ 上下文来自**另一个 Agent**的会话记录。**已定：不做**（见 §12.1 决策 7）。理由：它要求先定义「取谁的历史、裁剪多少、是否标注来源」，且极易被误认为「Pi 恢复了 Codex 的会话」（那是 B，跨 Agent 在语义上不成立）。

## 7. B 的触发通道（已定：B2）

「让 Agent 恢复」只能由 Agent 执行，客户端能选的只是**触发方式**。

**决策（2026-09-29）：采用 B2 —— ACP `session/resume`。**

| 选项 | 做法 | 结论 | 理由 |
|---|---|---|---|
| B1 | ACP `session/load` | **不采用** | 会重放历史，需与本地事件日志「对账」，复杂且易脏序列 |
| **B2** | ACP `session/resume` | **采用** | 不重放历史，从源头消除对账；语义干净 |
| B3 | Agent 私有 CLI 参数（如 resume 子命令） | **不采用** | 非标准、与具体 Agent 耦合；核心禁止堆积 Agent 名称判断 |

三者都需要同样的两样数据：**Agent 侧 sessionId + 创建时 cwd**；即便选了 B2，这两项仍必须新增持久化（见 §6.2 与 §8 的 D3）。

覆盖面的取舍已接受：B2 依赖 `sessionCapabilities.resume`，能覆盖的 Agent 少于 B1 的 `loadSession`；用户已确认**不考虑 B 不可行的场景，也不做降级**。

## 8. 关键设计决策（部分已定，其余实施前必须定）

| 编号 | 决策点 | 需要回答的问题 |
|---|---|---|
| ~~D1~~ | ~~触发通道~~ | **已定：B2（`session/resume`）**，见 §7 |
| ~~D2~~ | ~~历史对账~~ | **已定：不适用**——`session/resume` 不重放历史，见 §6.2 |
| D3 | 持久化 | **已定（2026-10-01 落地）**：`owned_session` 追加可空的 `agent_session_id` 与 `workspace_cwd`（创建流程内紧接着一次提交，**不**在恢复流程覆写），文件格式推进到 v5；取值缺失即「该会话不可恢复」，不得推导 |
| D4 | 能力诚实 | **已定**：未宣告 `sessionCapabilities.resume` 的 Agent 必须显式返回「不支持」（wire 码 `nodelink.command.unsupported`），不得虚报，也不得静默降级为 C（见 §12.1 决策 2）。门控在进程拉起之后、发送 `session/resume` 之前：此时**不发送**恢复请求，且本次拉起的子进程已在返回前终止并回收，会话状态不变 |
| ~~D5~~ | ~~远程授权~~ | **已定：复用 `grant.remote-work`**（不新增维度，见 §12.1 决策 3）；防会话存在性探测仍按「授权先于本机读取」执行 |
| D6 | 幂等与不确定 | **已定**：沿用 `requestId` 幂等（同键不同语义回 `nodelink.command.idempotency_conflict`）；`uncertain` **只**属于崩溃窗口（accepted 已落盘但本次未能结算）。能力未宣告、恢复列为 `NULL`、创建时目录复校验失败、payload 非法与越权这五类**确定类**失败一律结 `failed`，不得用 `uncertain` 掩盖 |
| D7 | 会话状态机 | **已定（2026-10-01 落地）**：恢复把同一 core 会话从持久态抬回可交互态并重新绑定单一端点（同一会话只有一条活跃绑定，不产生第二个端点）；并发语义不变（同一会话最多一个 active turn） |
| D8 | C 的上下文裁剪 | 灌历史时的选取范围、大小上限、是否标注来源，避免 token 爆炸与误认为恢复（**本次不实施**） |
| D9 | 环境校验与恢复授权边界 | **已定（2026-10-01 落地）**：恢复前对持久化的 `cwd` 重做同口径校验（目录不存在/被改成文件/`canonicalize` 结果与创建时不同均拒），返回 `nodelink.internal.unavailable`；不回退到别的目录、不重解析 alias。见 §8.1 |
| ~~D10~~ | ~~Export 粒度~~ | **已定：保持现状**，每个 Agent 一条 Export；不放宽 `export.create`（见 §12.1 决策 5） |
| ~~D11~~ | ~~同目录并发~~ | **已定：不管**，完全交给使用者（见 §12.1 决策 6） |

### 8.1 D9 展开：环境校验不是「一次性」的

被恢复的会话对应的 `cwd` 可能在恢复时发生变化（目录被删、被改成文件、指向了别处）。设计上必须明确：

- 恢复前对持久化的 `cwd` 重做一次与创建时同口径的校验；
- 不能因为「它曾经合法」就跳过校验；
- 目录不存在时按服务端不可用类错误返回，不静默回退到别的目录。

（同口径的解析规则见 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §5.1 与 §3.7 对 workspace 的约定。）

## 9. 影响面（实施时必须同步）

按 `AGENTS.md` §8 的映射，一旦实施，至少涉及：

**代码**

- `crates/core`：`Session`/`OwnedSessionRef` 是否携带 Agent 侧 sessionId；新增恢复用例入口；`core::ports::SessionEndpoint` 是否需要 `load` 能力。
- `crates/storage-sqlite`：`owned_session` 加列 + migration（版本常量推进）+ 升级/幂等/字节稳定测试。
- `crates/agent-host`：持久化并回填 Agent 侧 sessionId；新增「重启进程后 load/resume」路径；能力门控。
- `crates/acp-protocol`：`session/resume` 的类型化 DTO **已实现**（`methods.rs` 的 `implemented: true`，`delivery = conditional_mvp`；此前的陈旧陈述已于 2026-10-01 更正）；`session/load` 保持不支持。端到端门控在 `agent-host`，`facade` 仍 `not_advertised`，因此没有对外宣告。
- `crates/server`：`node_link` 的命令路由新增/扩展（若新增命令）；`acp_facade` 宣告与转发（facade 尚未落地）；若放宽 Export 粒度，`local_admin::params` 的 1 项约束。
- `crates/app`：CLI 展示与传参；组合根装配。

**合同与机器资产**

- `compatibility/acp/v1/matrix.json`：`session/resume` 一行从 `post_mvp` 提升并更新各层 `layers`（`session/load` 保持 `post_mvp`，本次不实现）。
- `compatibility/commands/v1/commands.json`：新增恢复命令名（如 `session.resume`）与 scope，复用 `grant.remote-work`；D 不需要新命令。
- `schemas/node-link/v1/`、`fixtures/node-link/v1/`、`schemas/sync/v1/`、`fixtures/sync/v1/`、`schemas/acp/v1/`（涉及 wire 时）。
- Export 粒度**不放宽**（见 §12.1 决策 5），因此 `export.create` 与 local-admin 的 schema/fixture 不因 D 而变。

**权威文档**

- [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) §12.7（命令 payload）、§11.3（feature）与 §15（顺序/重放）。
- [ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) §3.3/§3.4（layers 与 delivery 的口径）、§6（facade 映射）、§7（Agent 实现差异）。
- [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §7.3（表结构）与 §5.1（后端端口）。
- [CONFIG_REFERENCE.md](./CONFIG_REFERENCE.md) §4/§5（会话与存储相关键，若新增）。
- [LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) §5.8（若 CLI 子命令变化）。
- [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §11.5/§10.3（命令与事件视图，若涉及客户端面）。

**门禁**：`npm run check`（合同资产）+ crate 依赖方向 + 合同漂移门禁必须全绿。

## 10. 验证清单（不改代码，先确认 Agent 能力）

在决定 B1/B2/B3 之前，先做只读观测：

1. 让 acp-remote 启动目标 Agent，检查 `initialize` 响应中宣告的能力：
   - `agentCapabilities.loadSession` 是否为 `true`；
   - `sessionCapabilities.resume` 是否存在且为 `true`。
   （`crates/acp-protocol/src/capability.rs` 已登记这两个路径，能力值会如实记录。）
2. 在 acp-remote 之外直接运行 Agent CLI，确认是否有 resume / 会话 id 相关入口。
3. 确认 Agent 原生会话的持久化位置与保留策略（是否会被 TTL 清理）。

只有第 1 或第 2 项为真，B 才成立；否则 B 只能退化为 C。**D 不依赖上述任何一项。**

## 11. 非目标

- 不读取、不解析 Agent 的私有原生会话存储格式（强耦合、版本易碎；已定，见 §12.1 决策 4）。
- 不为「恢复」引入任何绕过 Export / 配对 / grant 的捷径。
- 不在核心堆积 Agent 名称判断（B3 只能经 profile 显式配置）。
- 不把 C 伪装成 B：语义有损必须对用户可见。
- 不做跨 Agent 的历史移交（D+）：A 目录的历史对话不交给 D 的新 Agent（见 §12.1 决策 7）。

## 12. 决策记录（全部已定）

### 12.1 已定（2026-09-29）

| # | 决策 | 结论 |
|---|---|---|
| 1 | B 的触发通道 | **B2：ACP `session/resume`**。不采用 B1（`session/load`，会重放历史、需要对账），不采用 B3（Agent 私有 CLI 参数）。代价是覆盖面取决于 `sessionCapabilities.resume`，已接受。 |
| 2 | C 的自动降级 | **不做自动降级；B 不可行的场景不在本次范围内。** 若 Agent 未宣告 `sessionCapabilities.resume`，按「显式不支持」返回错误，不静默换成 C。C 作为**独立能力**保留，由用户显式选择（见 §6.3）。 |
| 3 | 远程恢复的授权 | **复用 `grant.remote-work`**，不新增授权维度。授权仍先于任何本机读取与文件系统访问（防会话存在性探测，沿用 `create_session` 的现口径）。 |
| 4 | Agent 原生历史 | **不支持。** 只认 acp-remote 自己创建并记录的会话；不读取、不解析 Agent 的私有存储格式（见 §11）。 |
| 5 | D：Export 粒度 | **保持现状：每个 Agent 一条 Export**（同一 workspace alias 可被多条 Export 引用）。**不放宽** `export.create` 的 1 项约束。 |
| 6 | D：同目录并发 | **不管**，完全交给使用者。系统不阻止同一 workspace 上的多 Agent 并发，也不做互斥/告警。 |
| 7 | D+：跨 Agent 历史移交 | **不支持。** Pi 进入同一目录时不会获得 Codex 的对话历史；只共享目录（文件）。 |

### 12.2 待决

无。本文件所列决策已全部确定，可进入实施规划（OpenSpec 变更）。

