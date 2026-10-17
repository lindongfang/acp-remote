# CLI Commands 增量

## MODIFIED Requirements

### Requirement: CLI 子命令映射与薄客户端约束

CLI SHALL 提供 `LOCAL_ADMIN_PROTOCOL.md` §5.8 映射表中的全部子命令（`daemon start|stop|status`、`agent list`、`device pair|list|revoke`、`node pair|list|revoke`、`export create|list|revoke`、`import add|list|remove`、`doctor`、`acp-stdio`），且每个子命令的底层调用 MUST 与该表一致，不得自造管理方法（如不得新增 `daemon.doctor`）。`agent configure` 与 `provider configure` MUST NOT 存在。`workspace` 子命令组 MUST NOT 存在：目录只能由成功的会话创建自动记录，CLI MUST NOT 提供任何登记或修改目录的入口。CLI MUST 只做参数解析、展示与轮询：不复制业务规则、不自行读写 SQLite、不自行解析配置文件、不启动第二套核心；首切片 MUST NOT 提供 `session create`/`session list`。

#### Scenario: 子命令经本地通道调用对应方法

- **WHEN** Daemon 运行时执行 `agent list`
- **THEN** CLI 经本地通道调用 `agent.list` 并展示结果；CLI 进程不打开 SQLite

#### Scenario: 不再存在目录登记命令

- **WHEN** 执行 `acp-remote workspace select …` 或查看 `acp-remote --help`
- **THEN** 命令以非零退出码报告未知子命令，帮助输出里不出现 `workspace` 命令组

#### Scenario: Daemon 未运行时管理命令明确失败

- **WHEN** 没有有效单实例锁，执行除 `daemon start|status|stop`/`doctor` 外的管理子命令
- **THEN** CLI 以非零退出码与明确 stderr 报错结束，不打开数据库、不启动核心

## REMOVED Requirements

### Requirement: 结构化输入与凭据交互

**Reason**: `provider configure` 与凭据交互删除；Agent 自己持有凭据。

**Migration**: 由「结构化输入（无凭据交互）」承接。

## ADDED Requirements

### Requirement: 结构化输入（无凭据交互）

`export create` SHALL 以 `--file <path>` 读取与方法 `params` 同形的 JSON；文件校验失败的错误码映射 MUST 与方法侧一致（`local.invalid_params`/`local.conflict`）。CLI MUST NOT 接受 Provider/MCP 凭据值：凭据由 Agent 自己持有；`provider configure` 命令与其交互式无回显录入一并删除。

#### Scenario: 结构化文件输入

- **WHEN** 执行 `export create --file export.json`
- **THEN** CLI 读取文件并调用 `export.create`；校验失败时按方法侧错误码报告

#### Scenario: 凭据命令已删除

- **WHEN** 执行 `provider configure`
- **THEN** CLI 以非零退出码报告未知子命令，不读取任何凭据
