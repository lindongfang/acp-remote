## Purpose

定义 `acp-remote` Daemon 进程的可观察生命周期：从 CLI 前台启动、单实例锁与实例标识、配置加载与首次种子导入，到正常关闭顺序与失败关闭行为，使用户能可靠地启动、查询和停止本机 Daemon。

## ADDED Requirements

### Requirement: Daemon 前台启动与单实例锁

Daemon SHALL 由 `daemon start` 以 CLI 前台进程方式启动，启动时获取单实例锁并生成 16 字符小写十六进制的 `instanceId`（64 bit CSPRNG），写入锁文件供 CLI 读取；同一运行期内 `instanceId` MUST 不变，重启后重新生成。锁获取失败时 MUST 以明确错误退出，不得强杀已有进程或启动第二个实例。endpoint 创建失败（权限不符、路径被占用、socket 被替换为符号链接、目录权限不符、旧 socket 仍存活）时正式模式 MUST 拒绝启动，不得降级为无管理通道运行。

#### Scenario: 正常启动

- **WHEN** 本机没有运行中的 Daemon 且锁文件无有效持有者，用户执行 `daemon start`
- **THEN** Daemon 获取单实例锁、生成 `instanceId`、创建本地管理 endpoint，并开始接受本地通道连接

#### Scenario: 重复启动被拒绝

- **WHEN** 已有 Daemon 持有有效单实例锁，用户再次执行 `daemon start`
- **THEN** 第二次启动以非零退出码与明确错误结束，已有 Daemon 进程不受影响，不存在两个实例同时运行

#### Scenario: endpoint 创建失败即拒绝启动

- **WHEN** endpoint 路径被占用、目录权限不符或 socket 被替换为符号链接
- **THEN** Daemon 拒绝启动并以明确错误退出，不暴露任何无认证的本地 endpoint

### Requirement: 配置加载与首次种子导入

Daemon 启动时 SHALL 按 `CONFIG_REFERENCE.md` 的「配置与管理状态的权威」加载本地配置：首次启动将配置文件中的 Agent profile 种子导入管理存储并把「已初始化」标记与种子写入放在同一事务；后续启动 MUST 以 SQLite 管理存储为权威，不得重复导入覆盖本地管理改动。配置中的凭据值 MUST 从平台 keystore 读取，不得从配置文件或环境以外的路径注入未在白名单内的变量。

#### Scenario: 首次启动导入种子

- **WHEN** Daemon 在一个没有初始化标记的干净数据目录上首次启动，且配置文件含 `[[agents.profiles]]` 条目
- **THEN** profile 种子与已初始化标记在同一事务提交；随后 `agent` 相关查询返回导入的 profile

#### Scenario: 重启不覆盖本地管理改动

- **WHEN** Daemon 已初始化，用户在运行期间经管理方法修改了 workspace/agent/export 记录，随后 Daemon 重启
- **THEN** 重启后以管理存储中的记录为准，配置文件的种子内容不得覆盖运行期的本地改动

### Requirement: `daemon.status` 由组合根回答

`daemon.status` SHALL 由组合根直接回答而不经业务用例层，返回版本、`instanceId`、`nodeId`、`nodePublicKey`、启动时间与运行时长、数据目录、实际监听地址、`publicOrigin`、设备/节点/Export/Import 计数与 Agent 可用性。`links` 字段在本切片 MUST 恒为空数组（无 Node Link 连接管理器）。Daemon 未运行时，CLI SHALL 在进程内依据锁状态回答 `daemon status`/`daemon stop`，不得连接、不得打开数据库。

#### Scenario: 运行中查询状态

- **WHEN** Daemon 正在运行，CLI 经本地通道调用 `daemon.status`
- **THEN** 返回包含上述全部字段的结果，`counts` 与持久化管理记录一致，`links` 为空数组

#### Scenario: Daemon 未运行时查询状态

- **WHEN** 没有有效单实例锁，用户执行 `daemon status`
- **THEN** CLI 在进程内回答「未运行」并以约定的明确输出/退出码结束，不打开 SQLite、不启动任何核心组件

#### Scenario: 有锁但 IPC 不可达

- **WHEN** 存在有效锁文件但本地通道连接失败
- **THEN** CLI 以非零退出码与明确 stderr 错误结束，不视为「未运行」，不自动强杀进程、不直接读库

### Requirement: `daemon.stop` 与关闭顺序

`daemon.stop` SHALL 在关闭序列开始时返回 `{ accepted: true }`（失败走 error，不得以 `false` 表达），随后按固定顺序执行关闭：停接入层（不再接受新连接）→ 取消后台周期任务 → 停止 Agent 进程 → 刷新存储（含 `wal_checkpoint(TRUNCATE)`）→ 清理进程树并释放单实例锁。`graceMs` 为 `null` 时使用配置的默认值，非空时取值范围为 `0`–`60000`。

#### Scenario: 正常停止

- **WHEN** Daemon 正在运行，CLI 调用 `daemon.stop`（`graceMs` 为 null）
- **THEN** Daemon 先返回 `accepted: true`，随后按上述顺序完成关闭，锁文件释放，CLI 等待到连接关闭与锁释放后退出

#### Scenario: 停止期间新请求失败关闭

- **WHEN** Daemon 已开始关闭序列，CLI 或已连接客户端再发起管理请求
- **THEN** 请求以 `local.unavailable` 失败，不产生任何状态变更

### Requirement: 后台周期任务

运行中的 Daemon SHALL 持有并调度以下后台任务：启动初清理与每 60 s 周期的 prune/expire_pairings/sweep_orphans，以及按配置间隔的存储批量刷盘；每个任务 MUST 有明确的所有者、取消路径，并在关闭序列中被取消，不得遗留 detached task。Node Link 重连任务在本切片只保留装配点，不产生任何出站连接。

#### Scenario: 周期任务运行与取消

- **WHEN** Daemon 运行超过一个清理周期，随后收到 `daemon.stop`
- **THEN** 周期清理至少执行过一次且记录可观察；关闭时周期任务先于 Agent 停止被取消，进程退出后无残留任务
