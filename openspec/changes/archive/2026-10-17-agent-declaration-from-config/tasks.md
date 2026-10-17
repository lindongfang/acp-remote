# Tasks

## 1. 配置与 core（WP1）

- [x] 1.1 配置新增 `[node] name` 与 `[agent]` 表（键=agentId，值=字符串或 argv 数组），含可选 `default`；判据：`cargo test -p app` 中合法/非法配置用例通过（空命令、argv 含空串、id 非法、default 指向不存在项）。
- [x] 1.2 启动失败关闭：非法 `[agent]` 让 `daemon start` 以配置错误退出，错误只给键名与规则；判据：CLI 级用例断言退出码非零且 stderr 含键名、不含命令原文。
- [x] 1.3 Agent profile 的值对象来源改为配置（移除 SQLite 管理写入路径）；判据：`cargo test -p core` 中「profile 只由配置提供」用例通过。
- [x] 1.4 删除 Provider 端口、`CredentialResolver` 调用与相关模型类型；判据：`grep -rn "CredentialResolver\|ProviderEnvBinding" crates/core` 无命中，`cargo test -p core` 全绿。
- [x] 1.5 升级提示：配置无 `[agent]` 且旧库存在 profile 时记录 `daemon.agents_discarded`；判据：单元测试断言该日志事件与条数。

## 2. 存储（WP3）

- [x] 2.1 v10 迁移删除 `owned_agent_profile` 与 `owned_provider_ref`，保留审计行；判据：`cargo test -p storage-sqlite` 中「v9→v10 删表且审计行不变」「升级失败整体回滚」用例通过。
- [x] 2.2 删除对应 store 与其端口实现；判据：`cargo test -p storage-sqlite` 全绿且 `grep -rn "owned_agent_profile" crates/storage-sqlite/src` 仅出现在迁移与测试里。

## 3. agent-host（WP2）

- [x] 3.1 profile 只来自配置端口；启动环境恰为 `PATH`/`HOME`/`USERPROFILE`/`SystemRoot`/`TEMP`/`TMP`（缺失跳过）+ 空集合；判据：`cargo test -p agent-host` 中 `--dump-env` 用例断言集合逐项相等。
- [x] 3.2 保留前缀静态拒绝：profile 或环境声明中出现 `ACPR_*`/`ACP_REMOTE_*` 时启动前失败；判据：对应用例通过。
- [x] 3.3 删除白名单/绑定解析分支；判据：`grep -rn "env_allowlist\|resolve_env" crates/agent-host/src` 无命中。

## 4. 管理面与 CLI（WP4）

- [x] 4.1 删除 `agent.configure`、`provider.configure` 方法与错误映射；判据：`cargo test -p server` 中两者返回 `local.unsupported`。
- [x] 4.2 新增只读 `agent.list` 方法：返回 id、命令、argv、default、解析路径、文件是否存在，不启动进程；判据：`cargo test -p server` 中该用例通过且无子进程被创建。
- [x] 4.3 CLI：删除 `agent configure`/`provider configure`，新增 `agent list`；判据：`cargo test -p app` 与 `acp-remote --help` 输出断言。
- [x] 4.4 CLI 不自行解析配置文件（薄客户端）；判据：代码审查 + `grep` 确认 CLI 侧无配置解析调用。

## 5. 与 A 重叠的 delta 补写（WP6，A 归档后执行）

- [x] 5.1 `specs/local-admin-methods/spec.md`：删除 `agent.configure`/`provider.configure`，新增 `agent.list`（以 A 归档后的主规格为基线）。判据：`openspec validate --strict` 通过且 MODIFIED 场景无丢失。
- [x] 5.2 `specs/cli-commands/spec.md`：删除两个命令、加入 `agent list`（以 A 归档后的主规格为基线）。判据：同上。
- [x] 5.3 `specs/storage-schema-v2-migration/spec.md`：v10 删表与版本推进（以 A 的 v9 为基线）。判据：同上。

## 6. 文档（WP5）

- [x] 6.1 `docs/CONFIG_REFERENCE.md`：新增 `[node] name` 与 `[agent]`；删除 §7 的 `[[agents.profiles]]` 种子语义与凭据注入、§8 的 Provider 说明；判据：`npm run check:docs` 通过且文中不再出现 `[[agents.profiles]]`。
- [x] 6.2 `docs/LOCAL_ADMIN_PROTOCOL.md` §5.5/§5.8：删除两个方法/命令、加入 `agent.list`/`agent list`；判据：与 `schemas/local-admin/v1` 一致，`npm run check` 中方法集检查通过。
- [x] 6.3 `docs/MODULE_ARCHITECTURE.md` §4.5、`docs/CORE_PORTS_AND_STORAGE.md`（删两张表与 Provider 端口）、`docs/SECURITY_DESIGN.md`（子进程环境边界）、`README.md`；判据：`npm run check:docs` 通过。
- [x] 6.4 升级说明：Agent 不会从旧库自动搬运，需在配置文件中重新声明；判据：文档含该步骤且与启动警告文案一致。

## Workflow follow-up

- 变更 A 必须先归档（存储 v9 与主规格基线）。
- 按 `docs/WORKFLOW.md` §6 走 PR 与六个必需检查。
- 归档前再次跑 `npm run check` 与 `cd clients/app && npm run check`。
