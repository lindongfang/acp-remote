# Design

## Context

见 `proposal.md` 的 Why。相关实现事实：

- `owned_agent_profile`（agent_id PK、display_name、command、args_json、env_allowlist_json、provider_env_json、is_default）是运行期权威；`[[agents.profiles]]` 只在首次初始化导入（`daemon.seed_imported`），之后忽略（`daemon.seed_skipped`）。
- `agent.configure` / `provider.configure` 是两个本地管理方法，后者把凭据值写平台 keystore，前者写 profile。
- 启动 Agent 时：`env_clear()` 后注入「凭据解析结果（`env_allowlist ∩ 绑定`，缺一即失败关闭）」+ 一组必要进程环境（`PATH`/`HOME`/`USERPROFILE`/`SystemRoot`/`TEMP`/`TMP`）。
- 平台 keystore 同时承载节点/设备身份密钥与 Provider 凭据。
- 存储 schema 在变更 A 落地后为 v9；本变更在其后。

## Goals / Non-Goals

- **Goals**：单一权威（配置文件）；启动即校验、失败关闭；去掉凭据代管；保留只读诊断入口。
- **Non-Goals**：不做 PATH 探测与自动发现；不做环境变量声明；不管理 Agent 内部配置（模型/模式）；不改 ACP wire。

## Decisions

### D1 `[agent]` 的配置形状

```toml
[node]
name = "家里的台式机"

[agent]
default = "omp"                    # 可选；缺省表示没有默认 Agent
omp     = ["C:/tools/omp.exe", "acp"]
codex   = "C:/tools/codex-acp.exe"
```

- 键即 `agentId`（1..=128 字符，沿用模型校验）；值可以是字符串（只有命令）或字符串数组（完整 argv）。
- 展示名 = `agentId`（不引入第二个字段；若要友好名，由客户端展示 id）。
- 选「表 + 字符串或数组」而不是「数组 of table」：TOML 里最接近用户直觉、diff 最小；数组形态能表达需要参数的 Agent（`omp acp`），字符串形态覆盖零参数场景。
- 保留一个可选的 `default` 键（而不是把默认写进值里）：避免值有两种形状还要编码"默认"。

### D2 权威：每次启动读配置，删除 SQLite 表

- `owned_agent_profile` 与 `owned_provider_ref` 在 v10 删除；`agent.configure`/`provider.configure` 删除。
- 启动时若配置为空且旧库曾有 profile，记一条结构化警告（`daemon.agents_discarded`，带条数），让升级者知道"Agent 不会自动搬过来"。
- 替代方案（已被拒绝）：保留表并在启动时用配置覆盖同 id 记录——两个权威、两种冲突规则，正是本变更要消除的问题。

### D3 非法条目失败关闭

启动配置解析阶段校验并拒绝：空命令、空 argv 元素、`agentId` 非法、`default` 指向不存在的 id、重复 id（TOML 表本身禁止重复键）。错误消息只给键名与规则，不回显命令原文中的敏感片段（沿用配置诊断的既有脱敏口径）。

### D4 子进程环境：固定必要变量，无白名单

- `env_clear()` 后只注入 `PATH`/`HOME`/`USERPROFILE`/`SystemRoot`/`TEMP`/`TMP`（从 Daemon 进程原样取值，缺失则跳过）。
- 删除 `env_allowlist`、`ProviderEnvBinding` 与「绑定必须齐全」的失败关闭。
- 保留 `ACPR_*` / `ACP_REMOTE_*` 前缀的静态拒绝（纵深防御，不依赖 `env_clear`）。
- 代价：依赖额外环境变量（`APPDATA`、代理变量等）的 Agent 需要自己从配置文件/系统环境读取；这是"凭据与运行环境交给 Agent"的必然结果，写进文档。

### D5 Provider 凭据全部交给 Agent

- 删除 `provider.configure`、`owned_provider_ref`、`CredentialResolver` 端口与其在启动路径上的调用。
- 平台 keystore 只保留节点/设备身份密钥；`identity-keystore` 的 Provider 凭据 API 删除。
- 优点：ACP Remote 不再持有第三方 API key，威胁模型少一类秘密。
- 代价：用户要在各 Agent 自己的配置里放 key（文档给出三条示例：环境变量、Agent 自身配置文件、系统密钥链）。

### D6 只读 `agent list`

- 新增只读方法 `agent.list` 与 CLI 子命令 `agent list`：返回 `{ agentId, command, args, default, resolvedPath, executableFound }`。
- `resolvedPath` 只做「绝对路径则原样 / 相对名则在 Daemon 的 PATH 中查找」这一步，不启动进程。
- Daemon 离线时 CLI 以明确错误退出（与其它管理命令一致），不自行解析配置文件（薄客户端约束）。
- 替代方案（已被拒绝）：扩展 `daemon.status` 的 `agents` 字段——那会把"生命周期状态"与"配置诊断"混在一起。

### D7 迁移

- v10：`DROP TABLE owned_agent_profile; DROP TABLE owned_provider_ref;`（两张表只被本变更的能力使用；审计行保留）。
- 升级不搬运数据（见 D2 的警告）；回滚只能靠备份（与既有口径一致）。
- 本变更与变更 A 有存储版本先后关系：A=v9，本变更=v10；A 必须先归档。

## Migration Plan

1. 升级前备份 `acp-remote.sqlite3`（含 WAL）。
2. 升级后启动：若配置无 `[agent]`，日志给出 `daemon.agents_discarded` 警告；用户在配置文件里补声明后重启即可恢复。
3. 回滚：旧二进制因 `user_version` 过新被拒绝打开；需要回退时用备份。

## Risks / Trade-offs

- **[升级后 Agent 列表变空]** → 启动警告 + 文档的升级步骤；这是"单一权威"的必然后果，不静默。
- **[依赖额外环境变量的 Agent 起不来]** → 文档写明固定注入集合；Agent 侧可通过自身配置补齐。
- **[`[agent]` 表无法表达顺序]** → 不需要顺序；默认由 `default` 键显式给出。
- **[配置文件含命令路径，可能被视为敏感]** → 路径不是凭据；仍按既有配置诊断的脱敏口径处理错误输出。

## Open Questions

（无。）
