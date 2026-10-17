# cli-commands Specification

## Purpose

定义 `acp-remote` CLI 的用户可观察行为：子命令到本地管理方法的唯一映射、退出码与错误输出契约、设备与节点配对的安全确认仪式、结构化文件输入的校验映射，以及 `acp-stdio` 字节泵在 Daemon 缺席时的明确失败。

## Requirements

### Requirement: CLI 子命令映射与薄客户端约束

CLI SHALL 提供 §5.8 映射表中的全部子命令，底层调用 MUST 与映射表一致，且 MUST NOT 自造管理方法。`agent configure` 与 `provider configure` MUST NOT 存在；`agent list` 只读。CLI MUST 只做参数解析、展示与轮询。

#### Scenario: 子命令经本地通道调用对应方法

- **WHEN** Daemon 运行时执行 `agent list`
- **THEN** CLI 经本地通道调用 `agent.list` 并展示结果；CLI 进程不打开 SQLite

#### Scenario: 不再存在目录登记命令

- **WHEN** 执行 `acp-remote workspace select …` 或查看 `acp-remote --help`
- **THEN** 命令以非零退出码报告未知子命令，帮助输出里不出现 `workspace` 命令组

#### Scenario: Daemon 未运行时管理命令明确失败

- **WHEN** 没有有效单实例锁，执行除 `daemon start|status|stop`/`doctor` 外的管理子命令
- **THEN** CLI 以非零退出码与明确 stderr 报错结束，不打开数据库、不启动核心

### Requirement: 退出码与错误输出契约

成功时退出码 SHALL 为 `0`；失败时为非零。失败时 stdout 只输出人类可读的简短说明，机器可读信息 MUST 以一行结构化 JSON 输出到 stderr（至少含 `code`，取值来自本地错误码表或 `compatibility/errors/v1/errors.json`）。CLI MUST NOT 把 `error.message` 当稳定契约解析，也不得在输出中回显 secret 或完整敏感路径。

#### Scenario: 失败输出的双通道形态

- **WHEN** 某管理方法返回 `local.not_found`
- **THEN** stdout 为简短人类可读说明，stderr 恰好一行含 `"code":"local.not_found"` 的 JSON，退出码非零

### Requirement: 配对安全确认仪式

`device pair` 与 `node pair` SHALL 固定为 begin → 轮询 status → confirm 三步：begin 打印 `pairingUrl`；status 认领后 MUST 展示名称、指纹与过期时间（节点方向另含请求 grants）后才允许确认；confirm 提交的是用户确认的最终集合——设备方向不提交任何授权集合，节点方向提交用户确认的 grants 而非请求值。非交互场景 MUST 显式传入 `--fingerprint` 且与 status 返回值逐字匹配才调 confirm，不匹配时以非零退出且不修改任何状态；MUST NOT 提供「无参数自动确认」开关，也 MUST NOT 再要求或接受短验证码参数。`pairingUrl` 与其中的 secret 只存在于内存与终端展示，不得写入 shell 历史、日志或文件。`node pair --mode access` 在本切片因底层方法返回 `local.unsupported` 而以明确错误结束。

#### Scenario: 交互式设备配对确认

- **WHEN** 用户执行 `device pair`，设备扫码认领，终端展示名称、指纹与过期时间后用户输入 `y`
- **THEN** CLI 调用 `device.pair.confirm` 并不提交任何授权集合，成功后输出 `deviceId`

#### Scenario: 非交互配对逐字校验

- **WHEN** 以 `--fingerprint` 执行配对且它与 `status` 返回值逐字一致
- **THEN** 调用 confirm；不匹配时以非零退出、不调用 confirm、不修改任何状态

### Requirement: `doctor` 与 `acp-stdio`

`doctor` SHALL 组合 `daemon.status`（Daemon 运行时）与 CLI 侧本地检查，Daemon 离线时在 CLI 进程内完成，不新增任何管理方法。`acp-stdio` SHALL 只做 stdin/stdout ↔ 本地通道 channel `0x02` 的字节泵，不进入管理信封、不内嵌 core/storage/agent-host；Daemon 未运行或正在关闭时 MUST 以明确错误退出，不得自行打开数据库或启动第二套核心。

#### Scenario: doctor 离线完成

- **WHEN** Daemon 未运行时执行 `doctor`
- **THEN** CLI 在进程内完成本地检查并给出结论，不尝试连接或启动 Daemon

#### Scenario: acp-stdio 在 Daemon 未运行时明确退出

- **WHEN** Daemon 未运行，Zed 或其他调用方启动 `acp-remote acp-stdio`
- **THEN** 进程以非零退出码与明确错误结束，stderr 给出可判定的原因；不创建数据库句柄、不启动核心组件

### Requirement: 结构化输入（无凭据交互）

`export create` SHALL 以 `--file <path>` 读取与方法 `params` 同形的 JSON；文件校验失败的错误码映射 MUST 与方法侧一致（`local.invalid_params`/`local.conflict`）。CLI MUST NOT 接受 Provider/MCP 凭据值：凭据由 Agent 自己持有；`provider configure` 命令与其交互式无回显录入一并删除。

#### Scenario: 结构化文件输入

- **WHEN** 执行 `export create --file export.json`
- **THEN** CLI 读取文件并调用 `export.create`；校验失败时按方法侧错误码报告

#### Scenario: 凭据命令已删除

- **WHEN** 执行 `provider configure`
- **THEN** CLI 以非零退出码报告未知子命令，不读取任何凭据
