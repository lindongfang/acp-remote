# Proposal

## Why

**用户原始要求**：

> Agent 声明统一放配置文件；去掉 PATH 探测、去掉 `agent.discovery`；环境变量声明也撤回；`agent configure` 删除，只保留只读的 `agent list`。
>
> Agent 的能力（模型等）由 Agent 自己管，不集成到本项目。

**现状**：Agent profile 由 `agent.configure` 写进 SQLite（`owned_agent_profile`），配置文件里的 `[[agents.profiles]]` 只在**首次初始化**导入一次，之后改配置不生效（实测启动日志会打 `daemon.seed_skipped`）。于是同一件事有两个权威，且运行期唯一可用的入口是一条需要手写 JSON 文件的管理命令。Provider 凭据另有一条链路：`provider.configure` 写入平台 keystore，再按 profile 的绑定注入子进程环境变量。

**为什么现在做**：Agent 是"这台机器上装了什么程序"，属于机器级配置——它应该能 diff、能版本化、能随部署一起下发，而不是散落在运行期数据库里。凭据属于 Agent 自己的配置面，ACP Remote 没有理由代管。

## What Changes

- 配置文件新增两个段：
  ```toml
  [node]
  name = "家里的台式机"

  [agent]
  default = "omp"                       # 可选；缺省则没有默认 Agent
  omp   = ["C:/tools/omp.exe", "acp"]   # 值 = 单条命令字符串，或完整 argv 数组
  codex = "C:/tools/codex-acp.exe"
  ```
- `[agent]` 每次启动读取，是 Agent 列表的**唯一权威**；非法条目（命令为空、id 非法、`default` 指向不存在项）在启动时明确失败。
- **删除** `agent configure`、`provider configure` 两个命令与对应的本地管理方法。
- **删除** Provider 凭据管理：不再有 `owned_provider_ref`、不再有"绑定 → 环境变量"解析、不再从 keystore 取 Provider 凭据。凭据由 Agent 自己持有。
- 子进程环境规则简化：`env_clear()` 后只注入固定的一组必要变量（`PATH`/`HOME`/`USERPROFILE`/`SystemRoot`/`TEMP`/`TMP`）；不再有白名单与绑定。
- 新增只读命令 `agent list`：打印配置解析出的 Agent（id、命令、argv、默认标记、可执行文件是否存在）。
- 平台密钥库**保留**，但用途收窄为节点/设备身份密钥；不再承载 Provider 凭据。

## Capabilities

### New Capabilities

（无）

### Modified Capabilities

- `local-agent-config`：Agent 声明的来源与优先级、去掉种子语义与 Provider 引用
- `local-agent-host`：profile 来源与子进程环境注入边界
- `platform-keystore`：密钥库用途收窄到节点/设备身份
- `local-admin-methods`：删除 `agent.configure` / `provider.configure`（**A 归档后补写**，见 tasks）
- `cli-commands`：删除两个命令、新增 `agent list`（**A 归档后补写**，见 tasks）
- `storage-schema-v2-migration`：删除 `owned_agent_profile` / `owned_provider_ref` 两张表（**A 归档后补写**，见 tasks）

## Impact

- **代码**：`app::config`（新段解析与校验）、`core::model`（AgentProfile 来源改为配置）、`core::ports`（去掉 CredentialResolver 与 Provider 引用端口）、`storage-sqlite`（删表迁移）、`agent-host`（启动规格与环境注入）、`server::local_admin`（删方法）、`app::cli`（删命令、加 `agent list`）、`identity-keystore`（Provider 凭据 API 收窄）。
- **文档**：`docs/CONFIG_REFERENCE.md`（新增 `[node]`/`[agent]`，删 §7 的 profile 与凭据注入、§8 的 Provider）、`docs/LOCAL_ADMIN_PROTOCOL.md`（方法集）、`docs/MODULE_ARCHITECTURE.md` §4.5、`docs/CORE_PORTS_AND_STORAGE.md`（端口与表）、`docs/SECURITY_DESIGN.md`（子进程环境边界）、`README.md`。
- **用户可见**：Agent 列表来自配置文件；`agent configure`/`provider configure` 消失；`agent list` 只读。

## 非目标

- 不做 PATH 探测、不做 `[agent.discovery]`、不做环境变量声明 Agent。
- 不管理模型、模式或任何 Agent 内部配置（仍由 Agent 在 `initialize` 中宣告）。
- 不改变 ACP 能力协商与 wire。
- 不改变节点/设备身份密钥的存储方式。

## 硬约束

- 配置文件里的错误条目必须**启动失败关闭**，不得静默跳过。
- 子进程 MUST NOT 继承 `ACPR_*` / `ACP_REMOTE_*` 前缀的变量（保留既有静态拒绝）。
- 凭据 MUST NOT 出现在配置文件、日志、事件或错误消息中。
- `npm run check` 全绿。

## 成功判据

1. 只改配置文件即可增删 Agent，重启后目录页/创建选择器随之变化，无需任何管理命令；
2. 配置文件里的非法 `[agent]` 条目让 `daemon start` 明确失败并指出键名；
3. `agent list` 输出与配置文件逐项一致（含可执行文件是否存在），且不在 Daemon 运行时也可用；
4. 旧库升级后删除两张管理表，`agent`/`provider` 相关管理方法返回 `local.unsupported`；
5. `npm run check` 全绿。
