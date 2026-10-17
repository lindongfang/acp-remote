# local-agent-config Specification

## Purpose

本地配置（Agent profile、Provider 引用、workspace 记录与首次初始化标记）决定本节点如何启动 Agent 并解析凭据；本能力定义它的写入校验、默认唯一性、种子幂等，以及凭据到子进程环境变量的解析边界。

## Requirements

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
