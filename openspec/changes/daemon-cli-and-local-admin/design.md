## Context

前置切片已把本变更需要的全部内层就位：`core::use_cases` 已有设备/节点/Export/Import/本地配置族与审计写集（`TrustStore`/`ExportStore`/`LocalConfigStore`/`AuditStore`/`CredentialResolver` 端口全部由 `storage-sqlite` 实现），`identity-auth` 提供配对状态机、握手与授权展开，`identity-keystore` 提供 Windows DPAPI 后端。缺的是「正在运行的进程」：没有任何 crate 监听 OS 本地 IPC、解析管理信封、装配组合根或提供 CLI。行为合同已冻结在 `docs/LOCAL_ADMIN_PROTOCOL.md`（endpoint/framing/信封/方法集/错误码/生命周期）、ADR-0004 与 `docs/CONFIG_REFERENCE.md`；本设计只做实现侧决策，不改动这些合同。动机见 proposal.md 的 Why。

## Goals / Non-Goals

**Goals:**

- 新增 `server` crate：`transport::local`（平台 IPC listener、访问控制、framing）与 `local_admin`（信封编解码、方法路由、错误码映射）两个模块；`sync`/`node_link`/`acp_facade` 不建目录之外的占位实现。
- 新增 `app` crate：`acp-remote` 单二进制（daemon 子命令 + 管理 CLI + `acp-stdio` + `doctor`），组合根装配全部具体实现。
- 管理方法的 wire 形状只消费既有机器定义（`schemas/local-admin/v1/` 与 `fixtures/local-admin/v1/`），Rust 侧不另造词表。
- Windows x64 为首要可运行平台；Unix 路径编译通过且权限/凭据路径可由 Linux CI 的 `#[cfg(unix)]` 测试覆盖。

**Non-Goals:**

- 除 proposal 已列的非目标外，本设计不定义：`server::sync`/`server::node_link`/`server::acp_facade` 的任何接口形状；Node Link 重连任务的真实实现（只留装配点）；QR 图形渲染（见 Decisions 第 8 条）。

## Decisions

### 1. crate 与模块划分

- `server`：
  - `server::transport::local`：endpoint 创建（Named Pipe / Unix socket）、SDDL/权限设置、对端凭据校验、frame 读写与连接级规则（首帧定 channel、混用/空帧/超长关闭、未完成请求上限 32、`0x02` 方向背压上限 1 MiB 的接线点）。不放任何业务语义。
  - `server::local_admin`：管理信封值对象与编解码（closed object、`v=1`、camelCase、错误只含 `code`/`message`）、方法分发表、参数校验与 `core::use_cases` 调用、`local.*` 错误码映射。信封/方法词表以 `include_str!` 内嵌 `schemas/local-admin/v1/envelope.schema.json` 做漂移断言测试，fixture 目录的 valid/invalid 信封做往返测试。
  - 组合根数据需求（`daemon.status` 字段、`daemon.stop` 触发）以 `server::local_admin` 定义、由 `app` 实现的窄 trait（如 `DaemonControl`）注入：`server` 不反向依赖 `app`，`daemon.status`/`daemon.stop` 因此「由组合根回答」而不经用例层。
- `app`：
  - `app::daemon`：启动序列（加载配置 → 初始化存储与迁移 → 种子导入 → keystore/身份材料 → 单实例锁 → endpoint → 周期任务）、关闭序列（停接入 → 取消任务 → 停 Agent → 刷存储/checkpoint → 清锁）。
  - `app::cli`：clap 参数解析（kebab-case flags → camelCase params 的唯一映射点）、本地通道客户端、配对仪式编排、输出渲染与退出码。
  - `app::compose`：组合根——构造 `Arc<dyn …>` 端口实现（storage-sqlite、identity-keystore、entropy、clock、id generator）并注入 server 与 daemon。
- 信封值对象的唯一归属是 `server::local_admin`；`app::cli` 经对 `server` 的依赖复用同一类型，不产生第二份 wire DTO。

### 2. 异步与运行时

组合根（`app::daemon`）创建并持有 Tokio multi-thread runtime；`server` 与适配器只接受 `tokio` 类型与 trait 对象，不自建 runtime（沿用 `MODULE_ARCHITECTURE.md` §3.1 的既定分工）。workspace `tokio` 依赖需增量开启 `net`（Windows named pipe 与 Unix socket 的安全 API）、`io-util`、`signal`、`process`（若 CLI 需要）；新增 feature 不改变既有 crate 的依赖面。

### 3. Windows 访问控制的 FFI 边界（核验先行，不满足即回到用户决策）

- endpoint 创建与 `0x01/0x02` 字节流本身可用 tokio `net` 的 safe API。
- 但 §2.2 的两层访问控制在 Windows 上都需要 Win32 FFI：创建 pipe 时带仅含本用户 SID 的 SDDL（`CreateNamedPipeW` + security attributes，tokio 的 `ServerOptions` 不暴露），以及连接后 `GetNamedPipeClientProcessId` + `OpenProcessToken` + `GetTokenInformation(TokenUser)` 的 SID 比对。workspace 固定 `unsafe_code = "forbid"`，因此与 ADR-0006/`windows-dpapi` 同一模式：**FFI 只能存在于外部 wrapper crate**。
- 预研结论：crates.io 上没有成熟的独立安全封装（命中的 `app_if_ipc` 等是应用私有 crate，许可证/维护性存疑）。**已定案（2026-09-25 用户选择方案 a）**：新增一个极小的自研 wrapper crate `windows-local-ipc`，只暴露「以 SDDL 创建 pipe」「查询对端 SID」两个安全函数。它以**仓库内 path 依赖**形式存在（不发布 crates.io），目录置于 workspace `members` 之外并在 `Cargo.toml` 的 `workspace.exclude` 登记，因此不继承 workspace 的 `unsafe_code = "forbid"`；其内部 unsafe 以模块级最小范围收敛并加 `#![allow(unsafe_code)]` 限定，公开 API 全部为 safe。`deny.toml` 对 path 来源的登记在同一变更同步。
- Unix 侧无此问题：`nix`（已在 workspace）增开 `socket`/`user` feature 即可覆盖 `SO_PEERCRED`；`0700/0600` 权限用 std 的 `PermissionsExt`。

### 4. 单实例锁与 instanceId

锁文件位于 `daemon.data_dir`（`<dataDir>/daemon.lock`）：内容是 `instanceId`（16 字符小写 hex，`getrandom` 生成）与 pid；互斥靠 OS  advisory 文件锁（候选 `fs4` 或 `fd-lock`，同 §20 核验；两者都是纯 safe API）。锁获取失败 → 明确错误退出，绝不强杀。CLI 判定「Daemon 是否运行」只读锁文件内容 + 尝试加锁，不打开数据库。Unix 旧 socket 文件在持锁后确认无活跃 listener 才允许删除重建，否则拒绝启动（§7 endpoint 失败关闭清单）。

### 5. `0x02` 在 facade 缺席期的失败方式

定案：**接受连接并完成 framing 层校验后，Daemon 立即关闭 `0x02` 连接并记一条结构化警告**（facade 未装配）。这满足 §3.1 的失败关闭精神（不静默吞字节、不伪造 ACP 响应、attachment 不泄漏到新连接），且比「拒绝首帧」更靠近最终形态——切片 6 接入 facade 时只替换分发目标，framing/attachment 生命周期代码不动。`acp-stdio` 侦测到连接被立即关闭时以明确错误（「daemon 未提供 ACP 流」）非零退出。该衔接说明在同一变更写回 `LOCAL_ADMIN_PROTOCOL.md` §3.1 的实现状态注记（不改语义）。

### 6. 方法路由与身份/凭据接线

- 本地通道的所有方法以 `Actor::LocalCli` 调用 `core::use_cases`（`require_local` 已就位）；server 侧不做第二套授权判定。
- 配对方法编排：composition root 持有一个共享的 `identity-auth` 配对状态机实例（内存态：secret、SAS、失败计数），`server::local_admin` 的 `device.pair.*`/`node.pair.*` 调用状态机取得领域值后，经 core 用例组装写集单事务提交——「状态机不碰存储、写集一事务提交」的既有边界不变。
- `provider.configure` 的凭据值经 `identity-auth` 的 `IdentityKeystore` 端口写入（server 依赖 `identity-auth` 的 trait，由 app 注入 `identity-keystore` 实现）；`ProviderRef` 元数据走 `put_provider_ref` 用例。值在 server 内以 `SecretBytes` 形态流转，不进入日志/错误/`Debug`。
- `audit.export` 直接用 `AuditStore::query` 端口（合同明示「不经业务用例」），jsonl/csv 序列化与 sha256（`sha2`）在 `server::local_admin` 内完成；输出文件以「创建新文件、已存在即 `local.conflict`」方式写入。
- `daemon.status` 的 `counts` 由组合根聚合各 store 的列表长度；`agents[].available` 本切片定义为「profile 存在且 `command` 可解析」；`links` 恒空、`listen` 恒空（无网络 listener），`publicOrigin` 取配置值或 `null`。

### 7. CLI 形态

- 单二进制 `acp-remote`，`clap`（新依赖，§20 核验：成熟度/许可证无悬念，仅登记）做子命令解析；`--file` 的 JSON 用 `serde_json`（已在 workspace，`arbitrary_precision` 保持一致）。
- 退出码与 stderr JSON 行契约由 `app::cli` 的一个统一错误出口保证，避免各子命令自行打印。
- 非交互探测用 std 的 `IsTerminal`；配对轮询间隔与 5 分钟到期处理由 CLI 负责，Daemon 侧状态不变。
- CLI 与 Daemon 的集成测试以真实子进程 + 临时 data dir 驱动（不 mock 通道），Unix 权限位断言用 `#[cfg(unix)]`，Windows 专属路径在 Windows 开发机执行、Linux CI 只覆盖共享与 Unix 路径。

### 8. 配置加载

`app` 用 `toml` crate（新依赖，§20 核验登记）解析 `CONFIG_REFERENCE.md` 的配置文件；本切片只消费 `daemon.*`/`storage.*`/`agents.*`（种子）/`identity.*`/`logging.*`/`dev_mode.*`，其余段落（`sync.*`/`node_link.*`/`imports`/`exports`）解析后保留为「已知但未接线」并在 debug 日志注明，不报错也不生效。未知键按 `CONFIG_REFERENCE.md` 既有规则处理。首次种子导入经 `seed_state`/`mark_seeded` 用例，幂等且单事务。

### 9. QR 输出

v1 CLI 只打印 `pairingUrl` 文本（URL 已含完整配对 payload，安全性不受损）；二维码图形渲染（`qrcode` 类依赖与终端能力探测）推迟到首个有扫码需求的客户端落地时再评估。合同原文「打印 URL 与二维码（如终端支持）」按「URL 恒打印、QR 视终端支持」解读，本切片终端渲染判定为不支持。

## Risks / Trade-offs

- [Windows 访问控制需要 FFI，无现成安全 wrapper] → 已定案为自研 `windows-local-ipc` path 依赖（Decisions 第 3 条）；残余风险是自研 FFI 的正确性，缓解：API 面收敛到两个函数、Windows 本机集成测试覆盖凭据拒绝路径、任何扩展都需新评审。
- [新依赖集中出现（clap、toml、文件锁 crate、可能的 IPC wrapper）] → 每个都在 design/tasks 中逐条登记 §20 核验结论；`deny.toml` 的判定只在 CI，本地不声称通过。
- [`0x02` 连接在 facade 缺席期立即关闭，与切片 6 的最终行为不同] → 在 `LOCAL_ADMIN_PROTOCOL.md` 写实现状态注记；framing/attachment 代码按最终形态实现，切换成本只有分发点。
- [QR 不渲染降低配对便利性] → URL 恒打印且可复制；推迟到有扫码客户端时再引入渲染依赖。
- [CLI 集成测试跨平台差异（Windows 权限/凭据断言无法进 Linux CI）] → 共享行为在 Linux CI 常驻；Windows 专属路径在本机验证并记录于 verification，与 ADR-0008 的残余风险口径一致。

## Open Questions

无。Windows IPC wrapper 已按用户 2026-09-25 的确认定案（Decisions 第 3 条）。
