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

## Intent and Constraints

```agentic-intent
sources:
  - "用户在 2026-09-25 的本会话中给出上下文引用 `docs/DEVELOPMENT_PLAN.md#L42:44`，指向 `## 3. 实施切片` 的 `### 4. Daemon、CLI 与本地管理通道`（原文：「实现 `app` 组合根、Daemon 单实例与关闭顺序、平台本地 IPC、`server::local_admin` 和 CLI 管理命令。`acp-stdio` 只做 stdio 与 Daemon 本地通道的流转接；管理方法及 CLI 映射遵循[本地管理协议](LOCAL_ADMIN_PROTOCOL.md)。」/「验收：在本机可启动和停止 Daemon，配置 Agent profile，完成节点配对和 Export/Import 管理；Daemon 未运行时 `acp-stdio` 明确退出；本地通道的访问控制、framing 和两类载荷均有测试。」），未附加自由文字。"
  - "同会话用户原话：「Propose a new change - create the change and generate all artifacts in one step.」——授权建立本变更并生成全部规划工件，同时明确「planning artifacts only」，不在同一响应内开始实现。"
  - "`AGENTS.md` §8/§10 与 `openspec/config.yaml` 的 context：实施切片必须走 OpenSpec agentic 流程；改端口、值对象、配置键或合同形状时必须同一变更内同步权威文档并让 `npm run check` 全绿；E2E 为 `not-applicable` 时须逐变更取得用户批准并写入 `plan.md` 的 `downgrade_approval`（不得引用 2026-09-23 对首个切片的批准）。"
constraints:
  - "依赖方向（`MODULE_ARCHITECTURE.md` §5 矩阵为唯一判据）：`server` 只允许依赖 `core`、`acp-protocol`、`sync-protocol`、`node-link-protocol`、`identity-auth`；`app` 可依赖全部具体 crate（是唯一允许依赖 `identity-keystore` 的位置）。`server::local_admin`/`server::acp_facade`/`server::transport` 平级，只能调用 `core::use_cases`，不能互相调用、不能查询 SQLite、不能启动 Agent。"
  - "本地通道不监听 TCP、不使用 HTTP、不涉及 Origin/Host/TLS，不引入本地 token 或设备身份（ADR-0004 决策 4、7）；访问控制只按 OS 用户（endpoint 权限 + 连接后对端凭据校验，两层都要），凭据不一致时不发送任何 frame、立即关闭并记 `authorization.denied` 审计（`LOCAL_ADMIN_PROTOCOL.md` §2.2）。"
  - "framing 与信封的合同（`LOCAL_ADMIN_PROTOCOL.md` §3/§4）：1 MiB 帧上限、空帧/未知 channel/混用 channel/超长一律关闭连接且不返回错误帧；`v != 1` 关闭连接；每请求恰好一个响应；`error.message` 不含 secret/堆栈/完整敏感路径；`result` 永不回显凭据值。"
  - "CLI 是薄客户端（ADR-0004 决策 3、5）：只做参数解析、展示与轮询；Daemon 未运行时 `daemon status`/`daemon stop`/`doctor` 由 CLI 进程内回答，其余方法明确报错；任何路径都不得自行打开 SQLite 或启动第二套核心；CLI 不自动重试任何 mutation 方法。"
  - "配对安全仪式（`SECURITY_DESIGN.md` §9.4、`SYNC_PROTOCOL.md` §7.0）：确认界面必须展示名称、指纹、SAS、请求 scopes/grants 与过期时间；确认只能经本地入口；非交互场景必须显式传 `--sas`/`--fingerprint` 且逐字匹配，不得提供「无参数自动确认」开关；`pairingUrl` 与 pairing secret 只存在于内存与终端展示，不进日志/shell 历史/文件。"
  - "凭据只进平台 keystore：`provider.configure` 的 `values` 不得写入 SQLite、普通 TOML/JSON、日志或错误消息；keystore 不可用时正式模式返回 `local.unavailable`，不得降级明文存储（`SECURITY_DESIGN.md` §9.2、`LOCAL_ADMIN_PROTOCOL.md` §5.2）。"
  - "失败关闭（`LOCAL_ADMIN_PROTOCOL.md` §7、ADR-0004 决策 8）：endpoint 创建失败（权限不符、路径占用、socket 被替换为符号链接等）正式模式拒绝启动，不得降级为无管理通道；单实例锁失败给明确错误，不得强杀进程。"
  - "平台与依赖：Windows Named Pipe 等平台 API 必须经 wrapper crate，`app`/`server` 不得直接 FFI（workspace `unsafe_code = \"forbid\"` 不放开）；新增第三方依赖按 `AGENTS.md` §7 核验必要性、维护状态、许可证与平台支持，不得抬高 `rust-version = 1.85`；`deny.toml` 的 `[graph] targets` 若加平台须同步。Unix 路径只要求编译通过 + `#[cfg(unix)]` 路径在 Linux CI 上可测（`0700`/`0600` 与 `SO_PEERCRED`），Windows 是首个交付平台。"
  - "不新增配置键为原则：确需新键必须同一变更同步 `docs/CONFIG_REFERENCE.md`；命令名、错误码、`local.*` 能力等封闭词表只有一处机器定义，Rust 侧消费而非复制（`AGENTS.md` §5、§10）。"
  - "本地入口 `npm run verify`；`cargo-deny`（deps/advisories）与 `gitleaks`（secrets）只在 CI 运行，本地没有等价物，不得声称已在本地通过（`AGENTS.md` §8）。"
non_goals:
  - "不实现切片 5–7：`server::node_link`、`node-link-client`、`server::acp_facade` 的 ACP 语义、`server::sync` 与 Web/PWA 前端。"
  - "不实现 `node.pair.begin mode = \"access\"` 的远程 claim（需对 Owner 的 HTTPS 调用，属切片 6）：本切片该方法对 `mode = \"access\"` 返回 `local.unsupported`；`node.rotate-key.begin` 按 §5.7 一律返回 `local.unsupported`。"
  - "不实现 `import.add` 的真实 catalog 快照校验（无 Node Link 连接）：一律 `local.unavailable`；`daemon.status` 的 `links` 字段本切片恒为空数组。"
  - "不实现 CLI 的 `session create`/`session list`（§5.8 已定：首切片不提供，业务命令不进本地通道）。"
  - "不修改 `schemas/`、`fixtures/`、`compatibility/` 的既有内容；不新增/删除错误码、feature ID、命令名、pack/preset/grant/`local.*` 取值。"
  - "不做系统服务化（daemon start 是前台进程，ADR-0004 决策 1）、不做自动更新、不引入遥测或云端依赖。"
  - "不在本变更内抬高 MSRV/工具链基线，也不为放开 `unsafe_code` 新增 lint 覆盖。"
success_criteria:
  - "Daemon 生命周期：`daemon start` 在本机可启动（单实例锁、endpoint 创建、种子导入），`daemon stop` 经本地通道触发按序关闭（停接入 → 取消任务 → 停 Agent → 刷新存储 → 清理），`daemon status` 返回 §5.2 的字段形状；重复 start 给明确错误而非强杀。"
  - "本地通道：endpoint 位置、OS 用户 ACL 与连接后对端凭据校验按 §2 实现，伪造对端/跨用户连接被拒绝且无任何 frame 返回；framing 的关闭规则（空帧、超长、未知/混用 channel、超未完成请求上限、`v != 1`）逐条有测试。"
  - "管理方法：§5.2–§5.6 的 v1 方法集全部可经 CLI 调用并落到 core 用例与 SQLite；未知 method → `local.unsupported`，参数/信封错误 → `local.invalid_params`/`local.invalid_request`，重试语义按 §7（list/status 幂等、mutation 不自动重试、conflict/not_found 区分）。"
  - "CLI 映射与 §5.8 表格逐条一致：退出码（成功 0/失败非零）、stderr 一行结构化 JSON（含 `code`）、stdout 不回显 secret 或完整敏感路径；`device pair`/`node pair --mode owner` 的交互与非交互确认仪式可用且逐字校验 SAS/指纹。"
  - "`acp-stdio`：Daemon 运行时可作为字节泵建立 `0x02` 连接；Daemon 未运行时明确错误退出，不打开数据库、不启动第二套核心。"
  - "审计与日志：拒绝连接、方法失败、撤销、配对批准等按 `SECURITY_DESIGN.md` §14.2 落审计事件；日志不含 pairingUrl/secret/凭据值/完整敏感路径（§14.1）。"
  - "`npm run check` 与 `npm run verify` 全绿；新增 crate 的测试确实被执行（无零用例、无全跳过）；`check:boundaries` 对 `server`/`app` 新成员逐条通过。"
decision_bounds:
  - "Agent 可自主决定：`server`/`app` 内部模块划分与错误类型形状、单实例锁的文件布局与锁协议细节（在 `LOCAL_ADMIN_PROTOCOL.md`/`CONFIG_REFERENCE.md` 已冻结的语义内）、CLI 的参数解析与展示细节（不改变 §5.8 映射与退出码契约）、异步运行时选型（Tokio 已是 workspace 事实标准）、Windows Named Pipe wrapper crate 的选型（前提：满足 MSRV 1.85、许可证与维护性核验，核验证据随变更归档）、测试基座与 fake 端口写法、facade 缺席时 `0x02` 连接的明确失败方式（在本切片设计文档中定案并写回 `LOCAL_ADMIN_PROTOCOL.md` 的衔接说明）。"
  - "需要用户决策：抬高 workspace `rust-version`/工具链基线；新增 ADR 或放开 unsafe lint；改动 `core` 端口签名、依赖矩阵方向、wire 合同或封闭词表；把 `node.pair.begin mode = \"access\"` 的真实 claim 或 `import.add` 的 catalog 校验并入本变更；把 acp_facade 的 ACP 语义（切片 6）提前并入。"
  - "用户已批准：本变更的范围粒度（切片 4 作为一个变更，覆盖 `server`/`app` 两个新 crate）与「先出规划工件、不在同一响应内实现」。Main E2E 的 `not-applicable` 降级**尚未**取得本变更的批准，必须在生成 `plan.md` 前向用户取得并记录原话、时间与来源。"
assumptions:
  - "本切片不产生可端到端运行的产品路径（无 Node Link、无 acp_facade、无真实 Agent 会话），Main E2E 记 `not-applicable`；依据是 `openspec/config.yaml` context 的既定规则与切片 2/3 的同款处理。批准原话待取得。"
  - "core 用例面已覆盖 v1 方法集所需的全部业务语义（`devices`/`put_device`/`revoke_device`/`put_node`/`revoke_node`/Export/Import/本地配置族与审计写集已存在于 `core::use_cases`），本变更只做适配与装配；若实现期发现缺口（如 `daemon.status` 的 `counts` 聚合或配对 begin/status 的用例入口），按既有模式在 core 增补并同步 `CORE_PORTS_AND_STORAGE.md` §5，不改变端口语义方向。"
  - "channel `0x02` 在 facade 缺席时的失败方式（连接即关闭 vs 接受后由 facade 桩回复 ACP 错误）是设计期取舍，两个候选都不违反 §3/§3.1；将在 design.md 中选定并写回文档衔接说明，若用户偏好其一可在评审 design 时指出。"
  - "Windows Named Pipe 需要 wrapper crate（`unsafe_code = \"forbid\"`）；候选（如 `tokio` 的 `named_pipe` feature 或专用 crate）必须按 `SECURITY_DESIGN.md` §20 的四判据核验并在 design.md 记录结论；不满足时回到用户决策，不擅自放开 unsafe。"
  - "Unix socket 路径（含 `XDG_RUNTIME_DIR` 回落）在本变更内实现并按 Linux CI 覆盖 `#[cfg(unix)]` 权限路径，但这不代表 Linux 产品交付（`DEVELOPMENT_PLAN.md` §8 的验收约束）。"
```

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
