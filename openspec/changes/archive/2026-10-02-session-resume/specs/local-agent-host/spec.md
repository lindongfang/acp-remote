<!-- 已有能力（local-agent-host）的增量：省略 ## Purpose，只写需求变化。 -->

## ADDED Requirements

### Requirement: 会话创建时暴露 ACP 会话标识

会话创建成功后，后端 SHALL 向 core 暴露本次建立的 ACP 会话标识，使 core 能在同一会话生命周期内持久化它；后端 MUST NOT 自行读写存储，也 MUST NOT 在未取得标识时编造占位值。会话的创建时工作目录 MUST 取 core 已解析并传给后端的 workspace 解析结果，由 core 在**同一会话创建流程中的提交**里自己持久化（该提交不一定是被终态结算的那一次），不依赖后端回报。

#### Scenario: 创建成功暴露 ACP 会话标识

- **WHEN** core 以一个已解析的 workspace 调用后端创建 owned 会话，且 `session/new` 成功返回
- **THEN** 后端暴露本次 ACP 会话标识，core 可在同一会话创建流程的提交中把它与已解析的规范化目录一起写入该会话行

#### Scenario: 未取得标识时不编造取值

- **WHEN** 会话创建未成功完成 `session/new`（启动失败、超时或 Agent 返回错误）
- **THEN** 后端不产生任何 ACP 会话标识取值，core 不写入占位标识，该会话不被当作可恢复会话

### Requirement: 进程不在时的会话恢复（`session/resume`）

后端 SHALL 提供会话恢复入口，其输入至少为该会话的 Agent 标识、持久化的 ACP 会话标识与持久化的创建时工作目录。恢复时后端 SHALL 按 Agent 标识重新解析本机 profile 并拉起 Agent 子进程（复用既有的启动、stdout/stderr 与进程树清理、凭据注入边界），再以持久化的 ACP 会话标识与工作目录发送 `session/resume`，成功后把该会话抬回可交互状态并接受 turn。

后端 MUST 仅在该 Agent 协商到的能力宣告 `sessionCapabilities.resume` 时调用 `session/resume`。能力宣告只能经 `initialize` 协商获得，而 `initialize` 必然已经拉起进程，因此「未宣告」时的可观察保证是：MUST NOT 发送 `session/resume`，且 MUST 在返回明确的不支持错误之前终止并回收本次为恢复而拉起的子进程（不得残留进程、不得留下会话绑定），MUST NOT 改用新建会话代替，也 MUST NOT 伪造成功。恢复失败时 MUST 返回明确错误且该会话不进入可交互状态。

#### Scenario: 能力已宣告时恢复到可交互

- **WHEN** 某 Agent 在 `initialize` 协商中宣告了 `sessionCapabilities.resume`，且会话行带有持久化的 ACP 会话标识与有效工作目录
- **THEN** 后端拉起一个 Agent 子进程、发送 `session/resume`，成功后返回的端点可接受 `prompt`，并保持「同一 core 会话最多一个 active turn」

#### Scenario: 能力未宣告时显式不支持且不发送恢复请求

- **WHEN** 目标 Agent 未宣告 `sessionCapabilities.resume`（字段省略或为 `null`）
- **THEN** 恢复入口返回明确的不支持错误，不发送 `session/resume`，且在返回前终止并回收本次为恢复而拉起的子进程（无残留进程、无会话绑定），会话状态不变

#### Scenario: Agent 拒绝恢复时明确失败

- **WHEN** 已宣告能力的 Agent 对 `session/resume` 返回错误（例如其原生会话已被清理）
- **THEN** 恢复以明确错误结束，会话不进入可交互状态，不产生新会话标识，也不静默改走新建会话

#### Scenario: 反复恢复不产生第二个端点

- **WHEN** 对同一 core 会话在残留旧绑定尚未让出的情况下再次请求恢复
- **THEN** 恢复不产生第二个可派发 turn 的端点；旧绑定先让出或本次恢复被明确拒绝，不出现同一会话两个端点并行派发
