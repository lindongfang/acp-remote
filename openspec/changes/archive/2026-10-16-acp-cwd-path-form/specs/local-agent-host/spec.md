# Spec Delta

## MODIFIED Requirements

### Requirement: 进程不在时的会话恢复入口与执行

后端 SHALL 提供会话恢复入口，其输入至少为该会话的 Agent 标识、持久化的 ACP 会话标识与持久化的创建时工作目录。恢复时后端 SHALL 按 Agent 标识重新解析本机 profile 并拉起 Agent 子进程（复用既有的启动、stdout/stderr 与进程树清理、凭据注入边界），再以持久化的 ACP 会话标识与工作目录发送 `session/resume`，成功后把该会话抬回可交互状态并接受 turn。该请求的工作目录参数 MUST 是「出站 ACP 路径参数的形式」这一要求所定义的形式；持久化取值本身 MUST NOT 因此被改写。

#### Scenario: 能力已宣告时恢复到可交互

- **WHEN** 某 Agent 在 `initialize` 协商中宣告了 `sessionCapabilities.resume`，且会话行带有持久化的 ACP 会话标识与有效工作目录
- **THEN** 后端拉起一个 Agent 子进程、发送 `session/resume`，成功后返回的端点可接受 `prompt`，并保持「同一 core 会话最多一个 active turn」

#### Scenario: 反复恢复不产生第二个端点

- **WHEN** 对同一 core 会话在残留旧绑定尚未让出的情况下再次请求恢复
- **THEN** 恢复不产生第二个可派发 turn 的端点；旧绑定先让出或本次恢复被明确拒绝，不出现同一会话两个端点并行派发

#### Scenario: 恢复请求的工作目录不带 verbatim 前缀

- **WHEN** 持久化工作目录带 Windows verbatim 前缀（例如 `\\?\D:\work\api`），且该 Agent 已宣告恢复能力
- **THEN** 发出的 `session/resume` 工作目录参数是不带该前缀的形式，而持久化取值与会话行中的目录列保持原样

## ADDED Requirements

### Requirement: 出站 ACP 路径参数的形式

后端构造发给 Agent 的 ACP 请求时，其中的工作目录参数 MUST NOT 携带 Windows verbatim 前缀：`\\?\` 形式 MUST 去掉该前缀，`\\?\UNC\server\share` MUST 还原为 `\\server\share`；其它平台 MUST 保持原值。该转换 MUST 只作用于发往 Agent 的请求参数，MUST NOT 改写任何持久化取值、core 的解析结果或对端可见输出。

#### Scenario: 带 verbatim 前缀的目录在出站时被剥除

- **WHEN** core 交给后端的工作目录是 `\\?\D:\tmp\ws`
- **THEN** 发往 Agent 的会话建立请求与恢复请求的工作目录参数都是 `D:\tmp\ws`

#### Scenario: UNC 形态还原为普通 UNC

- **WHEN** core 交给后端的工作目录是 `\\?\UNC\server\share\ws`
- **THEN** 发往 Agent 的工作目录参数是 `\\server\share\ws`

#### Scenario: 持久化取值与解析结果不被改写

- **WHEN** 一次会话创建或恢复使用带 verbatim 前缀的工作目录
- **THEN** 该持久化取值、会话行中的目录列与 core 的解析结果都保持原样，只有发往 Agent 的请求参数是转换后的形式

#### Scenario: 非 Windows 平台保持原值

- **WHEN** 运行平台不产生 verbatim 前缀（例如类 Unix）
- **THEN** 发往 Agent 的工作目录参数与 core 交给后端的值逐字节相同
