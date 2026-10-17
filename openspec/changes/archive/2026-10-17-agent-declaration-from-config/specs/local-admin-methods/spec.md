# Local Admin Methods 增量

## REMOVED Requirements

### Requirement: 本地配置方法（无目录管理）

**Reason**: `agent.configure` 与 `provider.configure` 删除；Agent 声明改由配置文件提供。

**Migration**: 由「本地配置方法（只读 agent.list）」承接，目录管理仍不存在。

## ADDED Requirements

### Requirement: 本地配置方法（只读 agent.list）

系统 SHALL 提供只读的 `agent.list`，返回配置解析出的 Agent（`agentId`、`command`、`args`、`default`、解析路径、文件是否存在），不写任何状态、不启动进程。`agent.configure` 与 `provider.configure` MUST NOT 存在于 v1 方法集：调用它们一律返回 `local.unsupported`，且不产生任何状态变更。目录不再由本通道维护：MUST NOT 存在 `workspace.select` 或任何等价方法，目录只能由成功的会话创建自动记录。

#### Scenario: workspace 管理方法已不存在

- **WHEN** 旧客户端或脚本调用 `workspace.select`
- **THEN** 返回 `local.unsupported`，不产生任何写入

#### Scenario: agent.list 只读

- **WHEN** 调用 `agent.list`
- **THEN** 返回配置中的 Agent 列表与可执行文件探测结果，且没有任何子进程被创建

#### Scenario: 已删除的配置方法

- **WHEN** 调用 `agent.configure` 或 `provider.configure`
- **THEN** 返回 `local.unsupported`，配置、SQLite 与 keystore 都不改变
