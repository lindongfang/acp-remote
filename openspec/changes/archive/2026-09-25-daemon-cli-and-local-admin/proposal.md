## Why

`docs/DEVELOPMENT_PLAN.md` 的实施切片 4（行 40–44）还没有任何实现：workspace 里没有 `server`、没有 `app`，因此 `docs/LOCAL_ADMIN_PROTOCOL.md` 已冻结的 endpoint 与访问控制、framing、管理信封、v1 方法集与本地错误码**没有任何代码或测试**。前置切片已经就位：切片 1 落地了管理状态的 core 端口与 `storage-sqlite` 落盘（`DeviceManagement`/`ExportManagement`/本地配置族用例已存在于 `core::use_cases`），切片 3 落地了 `identity-auth` 配对/握手状态机与 `identity-keystore`，但它们是**不可由用户触达的库**——没有任何进程把 core 用例、SQLite、keystore 与 OS 本地 IPC 装配起来。其后果是直接的：用户无法启动/停止 Daemon、无法配置 workspace/Agent profile/Provider 凭据、无法完成设备与节点配对、无法建立 Export/Import，切片 5–7 也缺少「正在运行的 Daemon」这个宿主。

切片 4 的验收条件在本机闭环上是明确的（`DEVELOPMENT_PLAN.md` 行 44）：可启动和停止 Daemon、配置 Agent profile、完成节点配对和 Export/Import 管理、Daemon 未运行时 `acp-stdio` 明确退出、本地通道的访问控制/framing/两类载荷有测试。本变更把「Daemon、CLI 与本地管理通道」作为第四个纵向实现切片落地，为切片 5–7 提供可运行的组合根与唯一本地管理边界。

## What Changes

- 新增 crate `server`（加入 workspace `members`），本切片只实现其中两个模块：
 - `server::transport` 的本地通道部分：Windows Named Pipe 与 Unix domain socket 的 listener、endpoint 位置规则、OS 用户级访问控制（SDDL / `0700`+`0600`）与连接后对端凭据校验、§3 的 framing（`u32be length | channel byte | payload`）与连接级规则（首帧 channel 定用途、混用即关闭、1 MiB 上限、未完成请求上限 32）。
 - `server::local_admin`：管理信封（`v`/`id`/`method`/`params`/`ok`/`result`/`error`）的编解码与校验、v1 方法集到 `core::use_cases` 的路由、本地错误码映射（`local.*`）、失败关闭与审计接线。`daemon.status`/`daemon.stop` 由组合根回答，不经用例层。
 - channel `0x02`（ACP 流）本切片只到**连接级**：接受连接、执行 framing 与上限、按 §3.1 分配/作废 `FacadeAttachmentId`；`server::acp_facade` 的 ACP 语义属切片 6，本切片内 `0x02` 连接在 facade 缺席时按设计文档选定的方式明确失败（不静默吞字节、不返回伪造 ACP 响应）。
- 新增 crate `app`（加入 workspace `members`）：发布 `acp-remote` 可执行程序并作为组合根。
 - Daemon 生命周期：配置加载与首次种子导入（`CONFIG_REFERENCE.md` 的「配置与管理状态的权威」）、单实例锁与 `instanceId` 生成、后台任务清单（周期 prune/expire_pairings/sweep_orphans、存储批量刷盘；Node Link 重连任务在本切片无可连对象，只保留装配点）、按 `SECURITY_DESIGN.md` §12.1 的关闭顺序。
 - `app::cli`：按 `LOCAL_ADMIN_PROTOCOL.md` §5.8 的唯一映射实现全部 CLI 子命令（`daemon start|stop|status`、`workspace select`、`agent configure`、`provider configure`、`device pair|list|revoke`、`node pair|list|revoke`、`export create|list|revoke`、`import add|list|remove`、`doctor`、`acp-stdio`），只做参数解析、展示与轮询，不复制业务规则、不自行读写 SQLite；退出码与 stderr 结构化 JSON 按 §5.8；配对安全仪式（展示名称/指纹/SAS/scope、非交互必须显式传 `--sas`/`--fingerprint`）。
 - `acp-stdio`：stdin/stdout ↔ 本地通道 `0x02` 的字节泵；Daemon 未运行时明确错误退出，不自行打开数据库或启动第二套核心。
- 合同与门禁同步：`Cargo.toml`（members 与依赖登记）、`docs/MODULE_ARCHITECTURE.md`（§3/§3.1/§4.9/§4.10/§5 的已落地状态与依赖矩阵缺口收敛——`server`/`app` 开始成为列）、`README.md`「仓库当前状态」、`docs/DEVELOPMENT_PLAN.md` §2、`AGENTS.md` §4 模块状态表；实现期若定型了文档未冻结的细节（如 endpoint 创建失败的具体探测顺序），写回 `docs/LOCAL_ADMIN_PROTOCOL.md` 或 `docs/CONFIG_REFERENCE.md` 对应小节。
- **不修改**任何 wire 合同：`schemas/local-admin/v1/`、`fixtures/local-admin/v1/`、`compatibility/commands/v1/commands.json` 与 Sync/Node Link/ACP 资产保持原样；本变更只新增**消费**既有 schema 与 fixture 的 Rust 侧校验/编解码与漂移测试（方法集、错误码、`local.*` 能力三处一致已由 `check:command-catalog` 门禁断言，Rust 侧不另造词表）。
- 本次**不包含**：`server::node_link`（切片 5）、`node-link-client` 与 `server::acp_facade` 的 ACP 语义（切片 6，含 `node.pair.begin mode = "access"` 对 Owner 的 HTTPS claim——本切片该方法返回 `local.unsupported`）、`server::sync` 与 Web/PWA（切片 7）；`import.add` 依赖的「当前 Node Link 连接上的 catalog 快照」本切片不存在，一律按合同返回 `local.unavailable`。

## Capabilities

### New Capabilities

- `daemon-lifecycle`: Daemon 进程的可观察生命周期——配置加载与首次种子导入、单实例锁与 `instanceId`、启动/正常关闭顺序、`daemon.status`/`daemon.stop` 的语义（含 Daemon 未运行时由 CLI 进程内回答）、后台周期任务与失败关闭（endpoint 创建失败拒绝启动、锁失败明确报错不强杀）。
- `local-admin-channel`: 本地管理通道的传输行为——endpoint 位置与 OS 用户访问控制（含连接后对端凭据校验）、`u32be` framing 与 1 MiB 上限、channel `0x01`/`0x02` 的用途绑定与混用拒绝、连接级失败关闭（关闭连接且不返回错误帧的情形）、未完成请求上限。
- `local-admin-methods`: 管理信封与 v1 方法集的行为——envelope 校验规则、`local.*` 错误码映射、§5.2–§5.6 各方法到 core 用例的请求/结果语义（含 `workspace.select` 路径规范化、`provider.configure` 凭据只进 keystore、配对 begin/status/confirm/reject、Export/Import 管理、审计导出的内容边界）、重试与失败语义、以及 `node.rotate-key.begin`/`mode = "access"` 的 `local.unsupported` 与 `import.add` 的 `local.unavailable` 两个明确降级。
- `cli-commands`: `acp-remote` CLI 的可观察行为——§5.8 子命令映射、退出码与 stderr 结构化 JSON、配对安全仪式（交互与非交互、`--sas`/`--fingerprint` 逐字校验）、`--file` JSON 输入的校验错误映射、`doctor` 的在线/离线行为、`acp-stdio` 字节泵与 Daemon 未运行时的明确退出。

### Modified Capabilities

无。`admin-state-persistence`、`local-agent-config`、`identity-pairing`、`identity-handshake`、`scope-expansion`、`platform-keystore` 的**需求**不变：本变更只**消费**它们已冻结的用例、端口与状态机，把能力接线到本地通道与 CLI，不新增、修改或删除其行为要求。

## Impact

- **新增 crate**：`crates/server/`（本切片只有 `transport` 本地通道部分与 `local_admin`）、`crates/app/`（daemon + CLI + 组合根）；`Cargo.toml` 的 `members` 与 `[workspace.dependencies]`（异步运行时、Named Pipe/Unix socket wrapper、CLI 参数解析、UUID/时间等，逐依赖按 §20 核验）。
- **合同门禁**：`scripts/check-crate-boundaries.mjs`（`server`/`app` 新成员必须满足 §5 矩阵行；矩阵的「缺列」缺口——`server`/`app` 开始成为列——须在同一变更收敛 §5 的注记）；`npm run check` 全部门禁在文档改动后全绿。
- **文档**：`docs/MODULE_ARCHITECTURE.md`（§3/§3.1/§4.9/§4.10/§5 状态与矩阵）、`README.md`「仓库当前状态」、`docs/DEVELOPMENT_PLAN.md` §2、`AGENTS.md` §4 模块状态表；若实现期定型细节，`docs/LOCAL_ADMIN_PROTOCOL.md`（如 `0x02` 在 facade 缺席期的衔接说明）或 `docs/CONFIG_REFERENCE.md`（若确需新键）。
- **不涉及**：`schemas/`、`fixtures/`、`compatibility/` 既有资产；`core` 端口语义、`storage-sqlite` 表结构、Sync/Node Link/ACP wire、前端工程。
- **运行环境**：Windows 为首要验收平台（Named Pipe + SDDL + 对端凭据校验）；Linux CI 覆盖 `#[cfg(unix)]` 权限路径与共享代码；`cargo-deny`/`gitleaks` 仍只在 CI 判定。
