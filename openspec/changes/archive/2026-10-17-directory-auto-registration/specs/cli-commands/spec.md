# CLI Commands 增量

## MODIFIED Requirements

### Requirement: CLI 子命令映射与薄客户端约束

CLI SHALL 提供 `LOCAL_ADMIN_PROTOCOL.md` §5.8 映射表中的全部子命令（`daemon start|stop|status`、`agent configure`、`provider configure`、`device pair|list|revoke`、`node pair|list|revoke`、`export create|list|revoke`、`import add|list|remove`、`doctor`、`acp-stdio`），且每个子命令的底层调用 MUST 与该表一致，不得自造管理方法（如不得新增 `daemon.doctor`）。`workspace` 子命令组 MUST NOT 存在：目录只能由成功的会话创建自动记录，CLI MUST NOT 提供任何登记或修改目录的入口。CLI MUST 只做参数解析、展示与轮询：不复制业务规则、不自行读写 SQLite、不启动第二套核心；首切片 MUST NOT 提供 `session create`/`session list`。CLI MUST NOT 自动重试任何 mutation 方法；连接中断时报错退出，重试由用户显式发起。

#### Scenario: 子命令经本地通道调用对应方法

- **WHEN** Daemon 运行时执行 `agent configure --file <json>`
- **THEN** CLI 经本地通道调用 `agent.configure` 并展示结果；CLI 进程不打开 SQLite

#### Scenario: 不再存在目录登记命令

- **WHEN** 执行 `acp-remote workspace select …` 或查看 `acp-remote --help`
- **THEN** 命令以非零退出码报告未知子命令，帮助输出里不出现 `workspace` 命令组

#### Scenario: Daemon 未运行时管理命令明确失败

- **WHEN** 没有有效单实例锁，执行除 `daemon start|status|stop`/`doctor` 外的管理子命令
- **THEN** CLI 以非零退出码与明确 stderr 报错结束，不打开数据库、不启动核心
