# Local Admin Methods 增量

## MODIFIED Requirements

### Requirement: Export 与 Import 管理方法

`export.create` SHALL 校验 `ExportView` 全部必填字段：首切片 `agentIds` 恰好 1 项、`workspaceDirectories` 恰好 1 项、`templates` 恰好 1 项、`default*` 一致性、`templates[].params` 必须为空（非空返回 `local.invalid_params`）、每个 `workspaceDirectories[]` 的 `directoryId` 必须已在本机目录表中记录（否则 `local.not_found`）；`exportId` 已存在返回 `local.conflict`。`export.revoke` MUST 在持久提交后才返回。`import.add` 在本切片无 Node Link catalog 快照，MUST 返回 `local.unavailable`；`import.list`/`import.remove` 正常作用于本地记录，`import.remove` 不存在时返回 `local.not_found`。Export 的创建、修改与撤销 MUST NOT 经任何远程协议暴露。

#### Scenario: 创建 Export 成功

- **WHEN** 本机已记录目标目录，以合法单 Agent、单目录、无参数 template 调用 `export.create`
- **THEN** result 返回完整 `ExportView`（`revokedAt` 为 `null`），记录持久化

#### Scenario: 有参数 template 被拒绝

- **WHEN** `export.create` 的 `templates[].params` 非空
- **THEN** 返回 `local.invalid_params`，不得静默忽略参数

#### Scenario: import.add 无快照时明确不可重试语义

- **WHEN** 本切片内调用 `import.add`（无论参数是否合法）
- **THEN** 返回 `local.unavailable`（可重试语义），不创建 Import 记录

#### Scenario: 未记录的目录被拒绝

- **WHEN** `export.create` 的 `workspaceDirectories` 含一个本机目录表中不存在的 `directoryId`
- **THEN** 返回 `local.not_found`，不产生任何导出记录

## REMOVED Requirements

### Requirement: 本地配置方法

**Reason**: 目录不再由本地管理通道维护（`workspace.select` 删除）；`agent.configure` 与 `provider.configure` 的语义不变，但本要求的场景集合包含两条只描述 `workspace.select` 的场景，无法在保持场景集合不变的前提下保留。

**Migration**: 由新要求「本地配置方法（无目录管理）」承接 `agent.configure`/`provider.configure` 的全部语义与两条凭据场景；`workspace.select` 改为返回 `local.unsupported`。

## ADDED Requirements

### Requirement: 本地配置方法（无目录管理）

`agent.configure` SHALL 写入 Agent profile（`envAllowlist` 是注入上限，`command` 不经 shell 拼接），变更只影响后续启动的 Agent 进程。`provider.configure` SHALL 只把凭据值写入平台 keystore，result 只回 `configuredFields` 字段名而永不回显 `values`；keystore 不可用时正式模式返回 `local.unavailable`，不得降级明文存储。目录不再由本通道维护：MUST NOT 存在 `workspace.select` 或任何等价方法，目录只能由成功的会话创建自动记录。

#### Scenario: workspace 管理方法已不存在

- **WHEN** 旧客户端或脚本调用 `workspace.select`
- **THEN** 返回 `local.unsupported`，不产生任何写入

#### Scenario: provider.configure 不回显凭据

- **WHEN** 以合法 `values` 调用 `provider.configure`
- **THEN** result 只含 `providerId`/`kind`/`configuredFields`；凭据值出现在 keystore，不出现在 SQLite、日志、错误消息或任何响应中

#### Scenario: keystore 不可用时失败关闭

- **WHEN** 正式模式下平台 keystore 不可用，调用 `provider.configure`
- **THEN** 返回 `local.unavailable`，凭据不以任何形式落盘
