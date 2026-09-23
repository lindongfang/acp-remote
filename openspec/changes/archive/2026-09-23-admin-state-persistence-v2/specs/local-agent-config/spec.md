## Purpose

本地配置（Agent profile、Provider 引用、workspace 记录与首次初始化标记）决定本节点如何启动 Agent 并解析凭据；本能力定义它的写入校验、默认唯一性、种子幂等，以及凭据到子进程环境变量的解析边界。

## ADDED Requirements

### Requirement: Profile 写入校验与唯一默认

系统 SHALL 在写入 Agent profile 时校验名称、命令、参数、环境白名单与凭据绑定的合法性与相互关系，并保证至多一个 profile 为默认；切换默认 MUST 在一次调用内原子完成。

#### Scenario: 默认 profile 唯一且切换原子

- **WHEN** 本地把另一个 profile 设为默认
- **THEN** 该次调用返回后恰好一个 profile 为默认，不存在两个默认或零个默认的可见中间态

#### Scenario: 非法绑定在写入时被拒

- **WHEN** 一个 profile 的凭据绑定引用了不存在的 Provider 字段、环境变量名不在同一条 profile 的白名单内、出现重复绑定或使用保留的环境变量名
- **THEN** 写入返回参数类错误，库内不产生该 profile 行

### Requirement: 首次初始化种子的幂等

系统 SHALL 只在数据库尚未初始化时导入种子 profile，并把种子与「已初始化」标记在同一事务提交；空种子也 MUST 提交标记。初始化完成后数据库是唯一权威，种子 MUST 被忽略而不覆盖已有配置。

#### Scenario: 空种子也标记已初始化

- **WHEN** 首次初始化提供空 profile 列表
- **THEN** 调用成功后初始化标记为已初始化且无 profile 行，再次打开不会重新尝试导入

#### Scenario: 重复打开不重导种子

- **WHEN** 已初始化的数据库再次打开并提供非空种子配置
- **THEN** 种子被忽略，profile、workspace、Provider 记录与初始化标记保持库内值，且不产生新的写入

### Requirement: Provider 引用的无凭据与失效关闭

系统 SHALL 只持久化 Provider 引用的字段名、引用与版本；凭据值 MUST 只经平台 keystore 端口存取。引用失效时系统 MUST 失败关闭，MUST 不得回落到配置文件或静默重建引用。

#### Scenario: 引用失效即不可用

- **WHEN** 启动流程发现某 Provider 引用的 keystore 条目缺失或不可读
- **THEN** 该 Provider 被判定为不可用并返回明确错误，系统不生成新身份、不回落到配置文件、不继续启动依赖该凭据的进程

#### Scenario: 引用版本推进

- **WHEN** 同一 Provider 的凭据被换绑
- **THEN** 新引用以递增版本写入，旧引用在新引用提交后才可清理，期间读取到的是旧值或新值之一而不是混合状态

### Requirement: 凭据到子进程环境变量的解析边界

系统 SHALL 只把「profile 的凭据绑定」与「同一条 profile 的环境白名单」的交集解析为子进程环境变量；未绑定、不在白名单内或引用失效时 MUST 失败关闭而不静默跳过该变量。解析结果 MUST 不落盘、不进事件、不进日志、不出现在错误消息与测试快照中。

#### Scenario: 白名单是上限

- **WHEN** 解析一个 profile 的环境变量，其凭据绑定集合中包含不在白名单内的变量名
- **THEN** 该变量不会被解析或注入，白名单内且已绑定的变量按绑定注入

#### Scenario: 引用失效时失败关闭

- **WHEN** 解析过程中某个绑定的 keystore 引用不可用或字段不存在
- **THEN** 解析返回明确的不可用错误，进程不启动，没有任何凭据值被写入磁盘、事件或日志

#### Scenario: 日志只记录变量名与数量

- **WHEN** 一次成功的凭据解析被记录到结构化日志
- **THEN** 日志中只有变量名与变量个数，没有任何凭据值明文

### Requirement: workspace 记录的本机归属

系统 SHALL 把 workspace 记录作为本机私有数据持久化（别名、显示名、规范化路径与时间戳），并在建立或使用时校验目录存在且为目录；原始路径 MUST 不进入 Node Link catalog 或任何对端可见输出。

#### Scenario: 使用不存在目录时明确失败

- **WHEN** 一个已登记的 workspace 所指目录被删除或不再是目录
- **THEN** 使用该 workspace 的操作返回不可用错误，且不创建或修改 workspace 记录

#### Scenario: 远程目录不泄漏本机路径

- **WHEN** 本节点向对端提供 catalog 或会话元数据
- **THEN** 输出中只出现 workspace 别名，不出现规范化路径或其任何片段
