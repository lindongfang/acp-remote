# Local Agent Host 增量

## REMOVED Requirements

### Requirement: profile 来源与凭据注入边界

**Reason**: profile 的来源从「本机配置端口的运行期记录」改为「启动配置文件」，且凭据注入整条链路（白名单、绑定、keystore 解析）删除；原场景（白名单是注入上限、引用失效失败关闭）随之不再成立。

**Migration**: 由新要求「profile 来源与环境注入边界」承接；「不注入节点与设备密钥」与「不经 shell 拼接参数」两条约束保留，环境集合改为固定必要变量。

## ADDED Requirements

### Requirement: profile 来源与环境注入边界

系统 SHALL 只按启动配置声明的 profile 启动 Agent 进程（命令与 argv 原样传递、不经 shell 拼接），并在启动前清空环境、只注入 `PATH`、`HOME`、`USERPROFILE`、`SystemRoot`、`TEMP`、`TMP` 六个必要变量（Daemon 进程中存在时原样取值，缺失时跳过）。系统 MUST NOT 注入 Provider 凭据或任何白名单之外的变量，MUST 继续静态拒绝 `ACPR_*` / `ACP_REMOTE_*` 前缀的名字；Node/Device 私钥与 token MUST NOT 出现在子进程环境、日志、事件或错误消息中。

#### Scenario: 环境恰为必要变量

- **WHEN** 一个 profile 成功启动
- **THEN** 子进程环境中只出现上述六个变量（存在者），没有凭据，也没有 `ACPR_*` 或 `ACP_REMOTE_*`

#### Scenario: 不注入节点与设备密钥

- **WHEN** 任意 profile 成功启动
- **THEN** 子进程环境中不存在 ACP Remote 的 Node 或 Device 私钥与 token，日志与错误消息中同样没有

#### Scenario: 保留前缀被拒绝

- **WHEN** 配置或启动路径试图引入一个以 `ACPR_` 或 `ACP_REMOTE_` 开头的环境变量名
- **THEN** 启动前失败关闭，不启动进程

#### Scenario: profile 来自启动配置

- **WHEN** 配置文件声明了某个 Agent 并启动 Daemon
- **THEN** Agent 进程的命令与 argv 与该声明逐项一致；运行期没有任何命令能改写这份声明
