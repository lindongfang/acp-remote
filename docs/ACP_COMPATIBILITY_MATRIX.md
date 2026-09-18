# ACP 兼容性与可测试矩阵

> 状态：编码前契约（Draft）  
> ACP wire 版本：1  
> 上游快照：`agentclientprotocol/agent-client-protocol@c4137ab3b168d97f0ad6c542f483b6a417b2d610`  
> 核对日期：2026-09-18  
> 机器可读来源：[`compatibility/acp/v1/matrix.json`](../compatibility/acp/v1/matrix.json)

## 1. 目的

本文件把以下不变量落成可以由 CI 检查的验收合同：

> ACP Remote 可以增加认证、持久化、排序、重连和多端控制，但不能削弱、重新解释或静默丢弃 ACP 原生能力。

ACP 兼容性不能只用一个 `supported: true/false` 表示。每项能力必须分别回答：

1. 上游 Agent 是否声明该能力；
2. `agent-host` / `acp-protocol` 是否能接收并保真；
3. Broker 是否理解、持久化并保持原语义；
4. Node Link 是否能跨节点保持 raw ACP、origin 和 capability gate；
5. Sync 是否有结构化表示或明确的 raw fallback；
6. PWA 是否能操作、只读展示或明确显示不支持；
7. `acp-facade` 是否确实能够向 IDE 宣告该能力；
8. 哪些 fixture、契约测试和端到端测试证明上述结论。

## 2. 权威来源与版本固定

本矩阵以 ACP v1 官方 JSON Schema 为规范输入，固定到提交
`c4137ab3b168d97f0ad6c542f483b6a417b2d610`。该快照的
`schema/v1/schema.json` SHA-256 为：

```text
3c17bd6385d90cf672d8a661fddc359d73422cf8b8ce6865213d25cfd4c0eca7
```

`protocolVersion = 1` 只说明 wire 大版本。可选方法仍必须通过 capability negotiation 决定，不能因为同为 v1 就默认存在。

升级上游 ACP 时必须：

1. 固定新的 commit 与 schema digest；
2. 对比 method、notification、`session/update`、content block、tool content 和 capability 集合；
3. 为新增项加入矩阵行与测试，或明确标为 `explicit_unsupported`；
4. 运行矩阵结构检查、ACP fixture 测试和所有受影响的 Agent 兼容套件；
5. 审核通过后才更新本文件的快照信息。

官方来源：

- [ACP v1 Overview](https://agentclientprotocol.com/protocol/v1/overview)
- [ACP v1 JSON Schema（固定提交）](https://github.com/agentclientprotocol/agent-client-protocol/blob/c4137ab3b168d97f0ad6c542f483b6a417b2d610/schema/v1/schema.json)
- [ACP Extensibility](https://agentclientprotocol.com/protocol/v1/extensibility)

## 3. 矩阵结构

机器矩阵是 [`compatibility/acp/v1/matrix.json`](../compatibility/acp/v1/matrix.json)，由
[`schemas/acp/compatibility-matrix.schema.json`](../schemas/acp/compatibility-matrix.schema.json)
约束（矩阵顶层 `$schema` 已指向该文件）。它由三个元信息块与六组条目组成。

### 3.1 元信息块

| 块 | 覆盖对象 | 关键断言 |
|---|---|---|
| `protocol` | 上游 ACP 快照 | 固定 `sourceCommit`（40 位十六进制）与 `schemaSha256`（64 位十六进制）；升级流程见 §2 |
| `testFamilies` | 测试族目录 | 每个 `id` 唯一并声明单个 `owner`；全部条目的 `tests` 只能引用此处列出的族 |
| `nodeLinkPolicy` | 跨全部条目的额外强制层 | 其 `tests` 引用同一批测试族；取值见下一段 |

`nodeLinkPolicy` 是跨全部矩阵行的额外强制层：第一阶段只允许一个 Node Link hop；raw ACP 必须字节保真或显式 `rawUnavailable`；能力只能取 Agent、Owner、Export、Node Link、Access 与最终客户端的交集；会话正文默认只由 Owner 持久化；Owner 以 Access Node 为授权 principal；远程建会话只能选择已导出的 Agent 和 workspace template；live session route 使用 attachment generation 阻止旧连接 frame 误投递。`unsupportedRule` 固定为 `explicit_never_silent`：任何未实现路径必须显式拒绝或显式降级。

### 3.2 六组条目

| 分组 | 覆盖对象 | 关键断言 |
|---|---|---|
| `methods` | 双方 request/notification 与协议级取消 | 方向、baseline/optional、capability gate、各层行为 |
| `sessionUpdates` | 所有 `sessionUpdate` discriminator | 结构化语义、持久化、Sync/PWA 呈现 |
| `contentBlocks` | prompt/output content block | 输入协商、输出保真、客户端降级 |
| `toolCallContents` | tool result content/diff/terminal | 不得文本化，生命周期保持 |
| `capabilities` | 初始化时影响行为的 capability path | 宣告必须与端到端实际能力一致 |
| `invariants` | fixture 驱动的不变量 | `expectation` 取 `byte_exact`/`structured_not_text`/`explicit_unsupported`/`truthful_negotiation` 之一；该组条目不带 `layers`/`delivery` |

### 3.3 `layers`：目标行为

每行的 `layers` 不是实现状态，而是目标行为。每个 layer 各自拥有一个独立枚举，取值与
`schemas/acp/compatibility-matrix.schema.json` 的 `$defs.layers.properties.<layer>.enum` 完全一致
（脚本从该 schema 读取，见 §4）：

| layer | 该层的枚举取值 |
|---|---|
| `acp` | `native` \| `raw_preserve` \| `transport_control` |
| `broker` | `project_and_preserve` \| `local_service` \| `pass_through` \| `explicit_unsupported` \| `not_applicable` |
| `sync` | `command` \| `event` \| `snapshot` \| `raw_fallback` \| `explicit_unsupported` \| `not_exposed` |
| `pwa` | `full` \| `view_only` \| `explicit_unsupported` \| `not_applicable` |
| `facade` | `baseline` \| `advertise_if_end_to_end` \| `not_advertised` \| `not_applicable` |

各取值的含义：

- `native`：保持 ACP 原生语义；
- `raw_preserve`：保留 ACP raw document 字节，不做语义改写；
- `transport_control`：只做协议级传输控制（例如取消），不解释业务语义；
- `project_and_preserve`：创建公共领域视图，同时保留 ACP raw document；
- `local_service`：由 Owner Node Daemon 作为 ACP Client 在资源归属节点提供服务；
- `pass_through`：原样转发，不投影、不缓存；
- `command` / `event` / `snapshot`：通过对应 Sync 语义暴露；
- `raw_fallback`：没有结构化视图时按原始 JSON 透传，并显式标记为未知；
- `full`：PWA 能操作也能展示；
- `view_only`：PWA 能明确展示但不能发起；
- `baseline`：facade 无条件宣告，只允许用于 ACP baseline 方法；
- `advertise_if_end_to_end`：只有完整路径验证通过才允许 facade 宣告；
- `explicit_unsupported`：必须返回/展示明确不支持；
- `not_advertised`：facade 不向上游宣告；
- `not_exposed` / `not_applicable`：该层没有对应入口，不等于丢弃 ACP 消息。

### 3.4 `delivery`：交付节奏

`delivery` 表示计划节奏，与 `layers` 正交：

| 取值 | 含义 |
|---|---|
| `mvp` | 第一阶段必须实现并通过 |
| `conditional_mvp` | 第一阶段只在 Agent 宣告后启用，但必须有明确 gate |
| `post_mvp` | 后续实现，第一阶段必须显式不支持且不得宣告 |

跨阶段不变量（raw 保真、未知类型降级等）不用 `delivery` 表达，而是由 §3.2 的 `invariants` 组逐条登记。

`post_mvp` 行在首阶段不得宣告，必须同时满足：

1. `layers.facade = not_advertised`；
2. `layers.sync ∉ { command, event }`；
3. `layers.pwa ∈ { explicit_unsupported, not_applicable }`。

这三条与"`requirement` 非 `baseline` 的方法不得把 `facade` 写成 `baseline`"、"`requirement = optional` 的方法必须给出 `capability`"一样，都写进
[`schemas/acp/compatibility-matrix.schema.json`](../schemas/acp/compatibility-matrix.schema.json) 的 `if`/`then` 规则，由 ajv 逐行校验，不在脚本里重复实现。
含义是：`layers` 描述终态目标行为，`delivery` 只描述何时交付；一条 `post_mvp` 行可以照实写出它的目标层（例如 `native`、`raw_fallback`），
但不得写出任何会被上游或客户端读作“已可用”的宣告类取值。

## 4. 测试层次

矩阵中的 `tests` 引用稳定测试族，而不是某个 Rust 函数名：

| 测试族 | 最低要求 |
|---|---|
| `wire.decode_encode` | 已知消息可解码；规范 required 字段、方向和 discriminator 正确 |
| `wire.raw_roundtrip` | 未知字段、扩展和超大整数的原始 UTF-8 JSON document 字节不变 |
| `capability.gate` | capability 未宣告时不调用；宣告但链路不完整时不向上游 facade 虚报 |
| `broker.semantic` | 领域投影保持 ACP 原语义和结构化类型 |
| `storage.replay` | 持久化后可按顺序重放；raw 与 view 的关联不丢失 |
| `sync.contract` | Sync command/event/snapshot 符合 schema；不支持路径返回明确错误 |
| `client.presentation` | PWA 可操作、只读或显式降级，不静默隐藏 |
| `facade.contract` | 对 IDE 的方法、响应、通知和 capability negotiation 符合 ACP |
| `agent.compat` | fake Agent 常规 CI；Codex/OMP 为可选真实兼容套件 |
| `node_link.contract` | 跨节点 raw/origin 保真、capability 交集、无正文 Access 索引、节点级授权、受限建会话、断线重放与明确不支持 |

矩阵结构检查由 `npm run check` 执行（本矩阵对应其中两个脚本）：

```text
node scripts/check-acp-compatibility.mjs
node scripts/check-schema-fixtures.mjs   # 同一入口下跑 sync / node-link 的 fixture
```

矩阵的唯一结构与枚举来源是 [`schemas/acp/compatibility-matrix.schema.json`](../schemas/acp/compatibility-matrix.schema.json)：
分层行为取值定义在 `$defs.layers.properties.<layer>.enum`，交付节奏取值定义在 `$defs.delivery.enum`，
`post_mvp` 不得宣告、非 baseline 方法不得宣告 `baseline`、optional 方法必须有 capability gate 等跨字段规则
由该 schema 的 `if`/`then` 表达。`check-acp-compatibility.mjs` 用 ajv 直接校验 `matrix.json`，自身不再重新实现这些枚举与规则；
因此新增或删除取值必须先改 schema，再由本文件（§3.3、§3.4）与矩阵对齐。

脚本只保留 schema 无法表达的部分：固定快照的覆盖集合、`id`/`wireName` 等逐行唯一性、`tests` 只能引用已登记的测试族、
Node Link 首切片五个方法必须带 `node_link.contract`、必需不变量用例存在且其 fixture 可解析。`invariants` 条目只做 JSON
解析与路径存在性检查，语义断言由对应模块的契约测试承担。它不声称替代 Rust 实现测试；当对应模块建立后，
Rust/TypeScript 测试必须读取同一矩阵或引用相同 row/test ID 输出证据。

## 5. 第一阶段门槛

第一阶段至少必须通过：

1. `initialize`、`session/new`、`session/prompt`、`session/cancel`、`session/update` 的 fake Agent 端到端测试，并覆盖 Owner—Access Node Link；
2. 全部 11 种 `session/update` 的解码与 raw 保真测试，即使 PWA 尚不能完整呈现其中某项；
3. text prompt 输入；Agent 输出的五种 content block 均不会静默丢失；
4. tool call、diff、permission、elicitation 的结构化内容块与事件视图保持结构化或明确降级；tool result 中的 terminal 内容块（`tool_content.terminal`）同属首阶段必须结构化，agent→client 的 `terminal/*` 服务方法不在此列——它们仍是 `post_mvp`、`facade=not_advertised`；
5. `_meta`、下划线扩展方法和未知未来 discriminator 的保真/显式不支持行为；
6. capability 未宣告、已宣告但 Broker 不支持、Broker 支持但 PWA 不支持三种路径可区分；
7. `acp-facade` 只宣告经端到端测试证明的能力。

`session/list`、`session/delete`、`session/resume`、`session/close`、完整 config option、文件服务和 agent→client 的 `terminal/*` 服务方法可以排在后续阶段（`delivery=post_mvp`，按 §3.4 必须 `facade=not_advertised` 且不得通过 `sync`/`pwa` 宣告），但必须由矩阵驱动明确拒绝，不能被成功响应、空响应或普通文本替代。

这里的 ACP `session/list` 是底层 Agent 的可选原生方法，不等同于 ACP Remote 自己列出 Daemon 会话的 Sync `session.list`。同理，`promptCapabilities.image/audio/embeddedContext` 只约束 Client 向 Agent 发送的 prompt 内容；Agent 输出中出现相同 content block 时仍必须保留并明确呈现，不能因未宣告 prompt 输入能力而丢弃。

## 6. Agent 实现差异

通用矩阵描述 ACP 规范与产品合同，不把 Codex 或 OMP 的当前行为写死到核心。实际兼容结果以后记录为独立 report：

```text
target/acp-compat/<agent>/<agent-version>/<matrix-revision>.json
```

report 的每行只能使用：

- `pass`：所有要求证据通过；
- `fail`：Agent 宣告或行为违反矩阵；
- `not_advertised`：Agent 未宣告可选能力；
- `not_applicable`：该 Agent/测试场景不适用；
- `blocked`：环境依赖缺失，必须附原因。

不得用 `skip` 或空值把失败伪装成兼容。普通 CI 使用 fake Agent；真实 Agent report 不作为普通单元测试的硬依赖，但发布前应保存所支持 Agent 的版本化报告。
