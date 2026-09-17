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
4. Sync 是否有结构化表示或明确的 raw fallback；
5. PWA 是否能操作、只读展示或明确显示不支持；
6. `acp-facade` 是否确实能够向 IDE 宣告该能力；
7. 哪些 fixture、契约测试和端到端测试证明上述结论。

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

机器矩阵包含五组条目：

| 分组 | 覆盖对象 | 关键断言 |
|---|---|---|
| `methods` | 双方 request/notification 与协议级取消 | 方向、baseline/optional、capability gate、各层行为 |
| `sessionUpdates` | 所有 `sessionUpdate` discriminator | 结构化语义、持久化、Sync/PWA 呈现 |
| `contentBlocks` | prompt/output content block | 输入协商、输出保真、客户端降级 |
| `toolCallContents` | tool result content/diff/terminal | 不得文本化，生命周期保持 |
| `capabilities` | 初始化时影响行为的 capability path | 宣告必须与端到端实际能力一致 |

每行的 `layers` 不是实现状态，而是目标行为：

- `native`：保持 ACP 原生语义；
- `project_and_preserve`：创建公共领域视图，同时保留 ACP raw document；
- `local_service`：由 Daemon 作为 ACP Client 在电脑端提供服务；
- `command` / `event` / `snapshot`：通过对应 Sync 语义暴露；
- `view_only`：PWA 能明确展示但不能发起；
- `explicit_unsupported`：必须返回/展示明确不支持；
- `advertise_if_end_to_end`：只有完整路径验证通过才允许 facade 宣告；
- `not_exposed` / `not_applicable`：该层没有对应入口，不等于丢弃 ACP 消息。

`delivery` 表示计划节奏：

- `mvp`：第一阶段必须实现并通过；
- `conditional_mvp`：第一阶段只在 Agent 宣告后启用，但必须有明确 gate；
- `post_mvp`：后续实现，第一阶段必须显式不支持；
- `always`：跨阶段不变量，如 raw 保真和未知类型降级。

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

矩阵结构检查由以下命令执行：

```text
node scripts/check-acp-compatibility.mjs
```

它会检查固定快照的覆盖集合、重复项、缺失 capability gate、非法层行为、fixture 引用以及必需的不变量用例。它不声称替代 Rust 实现测试；当对应模块建立后，Rust/TypeScript 测试必须读取同一矩阵或引用相同 row/test ID 输出证据。

## 5. 第一阶段门槛

第一阶段至少必须通过：

1. `initialize`、`session/new`、`session/prompt`、`session/cancel`、`session/update` 的 fake Agent 端到端测试；
2. 全部 11 种 `session/update` 的解码与 raw 保真测试，即使 PWA 尚不能完整呈现其中某项；
3. text prompt 输入；Agent 输出的五种 content block 均不会静默丢失；
4. tool call、diff、terminal、permission、elicitation 保持结构化或明确降级；
5. `_meta`、下划线扩展方法和未知未来 discriminator 的保真/显式不支持行为；
6. capability 未宣告、已宣告但 Broker 不支持、Broker 支持但 PWA 不支持三种路径可区分；
7. `acp-facade` 只宣告经端到端测试证明的能力。

`session/list`、`session/delete`、`session/resume`、`session/close`、完整 config option、文件服务和 terminal 服务可以排在后续阶段，但必须由矩阵驱动明确拒绝，不能被成功响应、空响应或普通文本替代。

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
