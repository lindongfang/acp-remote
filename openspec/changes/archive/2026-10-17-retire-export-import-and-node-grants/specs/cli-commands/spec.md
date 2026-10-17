# CLI Commands 增量

## MODIFIED Requirements

### Requirement: CLI 子命令映射与薄客户端约束

CLI SHALL 提供 §5.8 映射表中的全部子命令，底层调用 MUST 与映射表一致，且 MUST NOT 自造管理方法。`export create|list|revoke` 与 `import add|list|remove` MUST NOT 存在。`node pair` MUST NOT 接受 `--grant`/`--export-id`/`--display-name`。CLI MUST 只做参数解析、展示与轮询。

#### Scenario: Export/Import 命令已删除

- **WHEN** 执行 `acp-remote export create` 或 `acp-remote import add`
- **THEN** 参数解析失败并以非零退出，不调用任何本地方法

#### Scenario: node pair 不携带授权参数

- **WHEN** 执行 `acp-remote node pair --grant grant.observe`
- **THEN** 参数解析失败并以非零退出

#### Scenario: 子命令经本地通道调用对应方法

- **WHEN** Daemon 运行时执行 `agent list`
- **THEN** CLI 经本地通道调用 `agent.list` 并展示结果；CLI 进程不打开 SQLite

#### Scenario: 不再存在目录登记命令

- **WHEN** 执行 `acp-remote workspace select …` 或查看 `acp-remote --help`
- **THEN** 命令以非零退出码报告未知子命令，帮助输出里不出现 `workspace` 命令组

#### Scenario: Daemon 未运行时管理命令明确失败

- **WHEN** 没有有效单实例锁，执行除 `daemon start|status|stop`/`doctor` 外的管理子命令
- **THEN** CLI 以非零退出码与明确 stderr 报错结束，不打开数据库、不启动核心
