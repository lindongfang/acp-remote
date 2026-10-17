# Local Agent Config 增量

## REMOVED Requirements

### Requirement: Profile 写入校验与唯一默认

**Reason**: Agent profile 不再有写入路径：声明来自配置文件的 `[agent]` 表，运行时没有管理命令或数据库写入。

**Migration**: 由新要求「Agent 声明来自配置且启动即校验」承接：校验时机从「写入时」变为「启动配置解析时」，默认唯一性由可选的 `default` 键表达。

### Requirement: 首次初始化种子的幂等

**Reason**: 不再有「首次初始化导入种子」的语义——配置是每次启动读取的唯一权威，不存在"导入一次后冻结"的中间状态。

**Migration**: 由新要求「Agent 声明来自配置且启动即校验」承接；旧库中的 profile 行在 v10 删除，启动时会记录 `daemon.agents_discarded` 警告。

### Requirement: Provider 引用的无凭据与失效关闭

**Reason**: Provider 凭据代管整个移除：没有 Provider 引用、没有 keystore 条目、没有版本推进。

**Migration**: 凭据由各 Agent 自己的配置面持有；平台 keystore 只保留节点/设备身份密钥（见 `platform-keystore`）。

### Requirement: 凭据到子进程环境变量的解析边界

**Reason**: 不再有凭据解析与环境绑定；子进程环境改为固定的一组必要变量。

**Migration**: 由 `local-agent-host` 的新要求「profile 来源与环境注入边界」承接；`ACPR_*` / `ACP_REMOTE_*` 前缀的静态拒绝保持不变。

## ADDED Requirements

### Requirement: Agent 声明来自配置且启动即校验

系统 SHALL 只从启动配置文件读取 Agent 声明：`[agent]` 表的键是 `agentId`，值 MUST 是非空命令字符串或非空字符串数组（完整 argv）；可选 `default` MUST 指向表中已存在的键。配置解析阶段 MUST 拒绝空命令、argv 中的空元素、非法 `agentId` 与悬空的 `default`，并以启动失败关闭，MUST NOT 静默跳过任何条目。Agent 列表 MUST NOT 来自数据库或任何运行期写入路径。

#### Scenario: 配置即权威

- **WHEN** 配置文件中声明了两个 Agent 并启动 Daemon
- **THEN** Agent 列表恰为这两项，其 id、命令与 argv 与配置逐项一致

#### Scenario: 非法条目启动失败

- **WHEN** `[agent]` 中某一项的命令为空，或 `default` 指向不存在的键
- **THEN** `daemon start` 以配置错误退出，错误只指出键名与规则，不启动任何 Agent 进程

#### Scenario: 运行期没有写入路径

- **WHEN** 调用被删除的 `agent.configure` 方法或执行 `agent configure`
- **THEN** 分别返回 `local.unsupported` 与未知子命令错误，Agent 列表不变

