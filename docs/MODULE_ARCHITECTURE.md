# ACP Remote 模块架构

> 状态：模块边界已冻结并开始落地（`acpr-transcript`/`acpr-wire`/`sync-protocol`/`node-link-protocol`/`core`/`storage-sqlite`/`acp-protocol`/`agent-host`/`identity-auth`/`identity-keystore`/`server`/`app` 已实现；`server` 已落地的路径是切片 4 的 `transport::local` + `local_admin` 与切片 5 的 `transport::net` + `node_link`，`sync`/`acp_facade` 仍待后续切片；`app` 已按切片 4/5 接线，见 §3 `[现状]` 与 `README.md` 的 crate 表）
> 版本：0.3
> 修订记录（2026-09-26，node-link-owner 变更 WP8 收口）：§3 状态行、§3 `[现状]` 与 §4.9 `[现状]` 按切片 5 的实绩写回——`server::transport::net` 与 `server::node_link` 已落地（Node Link 只覆盖 Owner 侧入站面，`sync`/`acp_facade` 仍待后续切片），WP1 在本文件留下的「已进入实现、尚未落地」注记随之收敛。
> 修订记录（2026-09-26，node-link-owner 变更 WP1）：§3.1 新增依赖口径 `[决定]`（Node Link 的 HTTP/WS/TLS 栈与 dev 用自签证书生成、落选候选与解析证据）；§4.9 加注 `node_link` 已进入实现、**尚未落地**；§5 矩阵的 `server` 行把 `acpr-wire` 格改为 ✓，并在表下注记限定该依赖只用于 ACPR-CJ1 digest 前像。
> 修订记录（2026-09-25，daemon-cli-and-local-admin 切片 4）：§3 状态行与 §4.9/§4.10 把 `server`（本地通道 + `local_admin`）与 `app`（daemon/CLI/组合根）标为已落地、范围仍限本切片；§3.1 记下当前成员数（十二个）；§5 表下注记收敛——`storage-sqlite` 已是矩阵列、`node-link-client` 仍是列外行，并如实说明门禁对「行缺席」是静默的。
> 修订记录（2026-09-24，identity-auth-and-keystore）：§3 状态行与 §3.1 的依赖口径记录两个身份 crate 已落地、DPAPI wrapper 取 `windows-dpapi 0.2.0`；§4.12 写入选型结论与三条已知代价、并标注 macOS/Linux 后端未实现；§5 矩阵的 `identity-auth`/`identity-keystore` 两行由 `check:boundaries` 按实际 `cargo metadata` 断言。
> 修订记录（2026-09-24，core-turn-view-fields）：§4.1 补「适配器产 ACP 派生投影、broker 补 `SYNC_PROTOCOL.md` §10.3 身份与会话版本」的职责分工；§4.7 写明 `owned_session.version` 由存储层在事务内实现、core 只按同一规则推导并在提交后比对（不一致 → `PortError::Corrupt` 失败关闭）。
> 修订记录（2026-09-24）：§4.2/§4.5 记录 `acp-protocol` 与 `agent-host` 的落地 surface、每个 Agent 一个 Job 的粒度、`win32job`/`nix` 选型与「关闭句柄即结束树」的实际路径；§3.1 的依赖登记与 §5 矩阵补 `agent-host` 列。  §4.1 的端口摘要补 `modes`（`session.mode.list` 的候选来源，见 `CORE_PORTS_AND_STORAGE.md` §5.1/§6 第 17 条）；补全本地管理通道的权威文档指向；`fixtures/acp/v1` 的校验口径改为与实现一致（快照 vendored 前只做存在性与解析检查）；§5 依赖矩阵放开 `storage-sqlite → acpr-wire`（`payload_digest` 的 ACPR-CJ1 只能有一份实现），§4.13 补 ACPR-CJ1；§3.1 的 `nix 0.30.1` 许可证订正为 `MIT`（原写 `MIT OR Apache-2.0`，与本地 registry 元数据不符）；§5 披露尚未成为矩阵列的 crate。  
> 日期：2026-09-18
> 上位文档：[INITIAL_DESIGN.md](./INITIAL_DESIGN.md)
> 已接受决策：[ADR-0003](./adr/0003-pi-inspired-module-boundaries.md)

## 1. 设计理念

本项目参考 Pi/Oh My Pi 的模块理念，但不机械复制其 TypeScript 目录结构：

- 保持核心最小，只保留不可替代的业务规则。
- 同一个核心支持 CLI、PWA、手机、Zed 和其他 ACP Remote Node 等多个入口。
- wire protocol、客户端、服务端、会话核心、持久化后端和应用组合分离。
- 每个模块只有一个主要变化原因。
- 高级行为优先通过适配器和配置扩展。
- **模块之间尽可能独立，协作靠依赖传递**：一个模块需要另一模块的能力时，通过端口或函数签名把它传进来（组合根负责装配），不通过横向调用或"顺手 import 隔壁"实现；跨模块共享的底层实现（例如 Sync 与 Node Link 共用的 transcript codec）下沉为无依赖的叶子 crate，而不是让平级模块互相依赖。

Pi 当前进一步把远程会话能力拆成独立 `protocol`、`client`、`server`、`agent-core` 和 SQLite session backend：协议只处理传输中立的信封与 framing，client/server 不解释应用业务 payload，核心不引入平台 SQLite。Oh My Pi 同样从 interactive、RPC、SDK、ACP 等入口复用同一 session engine。ACP Remote 采用相同原则：一个会话核心，多种 wire protocol，多种入站入口，以及本地/远程两种 Agent backend。

本项目的最小核心是：

```text
会话状态机
+ 每会话串行命令队列
+ ACP 命令路由
+ 事件排序与幂等
+ 多客户端同步规则
```

## 2. 总体依赖规则

```text
Clients / Peer Nodes
        │
        ▼
server adapters ───────> wire protocol crates
        │
        ▼
core::use_cases ───────> core::model
        │
        ├──────────────> core::ports
        │                       ▲
        ▼                       │
agent-host / node-link-client / storage-sqlite / identity-auth
        │
        └──────────────> identity-keystore（平台安全存储，仅 app 装配）
```

依赖只允许指向更稳定的模块：

```text
app             -> server / backends / identity-auth / identity-keystore / core
server          -> core + corresponding wire protocols
outbound backend-> core + corresponding wire protocols
core::use_cases -> core::ports + core::model
wire protocols  -> no core or infrastructure dependency
identity-keystore -> identity-auth（实现其 keystore 端口）
```

禁止：

```text
core -> axum/sqlx/tokio::process/Noise implementation
wire protocol -> core/domain model
storage-sqlite -> agent-host
server::sync -> server::node_link
agent-host -> server
adapter A -> adapter B
```

适配器之间只能通过 `core` 定义的 use case 或 port 协作。协议 crate 只拥有 wire schema、codec、framing、limits 和版本协商，不拥有业务领域类型；wire 与 core 的 mapper 属于使用该协议的 adapter。

平级模块共享的底层实现放在叶子 crate：`acpr-transcript`（[ADR-0005](./adr/0005-shared-transcript-codec.md)）只拥有长度前缀 transcript 的编码与解码，不拥有任何 domain/tag 取值或协议语义；`acpr-wire`（[ADR-0007](./adr/0007-shared-wire-value-crate.md)）只拥有跨协议共用的 wire 值对象与字段校验机制（uuid/decimalString/timestamp/base64url/featureList/rawAcp、`Nullable`、`RawObject`、`ExtraFields`、泛型 `PublicError`），不拥有任何协议词表。协议 crate 不互相依赖，Node Link 复用 Sync 的 codec 结构与值对象靠的是"共同依赖同一个叶子 crate"，而不是"一个协议 crate 依赖另一个"。

### 2.1 客户端边界

UI 客户端不属于 Rust core，通过版本化 `sync-protocol` 与 `server::sync` 通信。ACP Remote 节点通过独立的 `node-link-protocol` 通信。CLI 与运行中的 Daemon 通过平台本地 IPC 通信，其信封、framing 与方法集以 [LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) 为准。wire contract 分别以 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) 和 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) 为准；系统安全边界以 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) 为准；前端阶段、分层、PWA 限制和后续原生 adapter 约束以 [FRONTEND_DESIGN.md](./FRONTEND_DESIGN.md) 为准。

```text
PWA / Android / iOS
        ↓ Sync Protocol
server::sync -> core::use_cases

Access Node
        ↓ Node Link Protocol
server::node_link -> core::use_cases
core -> SessionBackendFactory -> node-link-client -> Owner Node
```

- 前端首个交付只实现 PWA，并位于 Node Link 首个纵向切片之后；后续原生客户端不能绕过 Sync Protocol 直接调用 Broker。
- 客户端可以共享协议类型、状态机和 feature 层，但不能复制服务端业务规则。
- Web/Native 存储、密钥、相机和生命周期差异必须通过客户端平台 adapter 隔离。
- PWA 的安全降级不能反向降低 Daemon 的认证与授权要求。

## 3. 建议的 Rust Workspace

```text
crates/
├─ core/                    领域、用例、端口与会话协调
├─ acp-protocol/
├─ sync-protocol/
├─ node-link-protocol/
├─ acpr-transcript/         叶 crate：Sync 与 Node Link 共用的 transcript codec 与表驱动校验
├─ acpr-wire/               叶 crate：跨协议共用的 wire 值对象与字段校验机制
├─ agent-host/              本地 ACP Agent backend
├─ node-link-client/        远程 Agent backend
├─ storage-sqlite/          core 持久化后端
├─ identity-auth/           身份、配对、签名与授权的纯状态机（不含平台 API）
├─ identity-keystore/       平台安全存储实现（DPAPI 包裹 / Keychain / Secret Service；CNG 不可导出档位待 ADR）
├─ server/                  sync / node-link / acp-facade / local-admin 入站模块
└─ app/                     daemon、CLI 与组合根
```

这组物理 crate 刻意少于逻辑模块数，接近 Pi 的“core + protocol + client + server + backend + app”划分。`model`、`use_cases`、`ports` 和 `broker` 先作为 `core` 内部模块；`sync`、`node_link` 和 `acp_facade` 先作为 `server` 内部平级模块。只有出现独立发布、编译隔离或明显构建成本后才继续拆 crate。

协议 crate 是例外：ACP、Sync 和 Node Link 分别拥有独立兼容周期与 fixture，必须从第一天物理隔离，且不得依赖 `core`。

`core` 的包名与库名不同：包名是 `core`（与上表、§5 矩阵一致），**库名是 `acp_core`**——一个名为 `core` 的依赖会在依赖方的宏展开里遮蔽 sysroot 的 `core`（`thiserror::Error` 展开出的 `core::fmt` 会解析到本 crate 而编译失败）。依赖方照旧在 `Cargo.toml` 写 `core = { path = "../core" }`，代码里用 `acp_core::…`；本文档其余部分的 `core::model`/`core::use_cases`/`core::ports`/`core::broker` 均指 `acp_core::…`。

平台安全存储是第二处例外：`identity-keystore` 独立成 crate 是为了隔离平台依赖（原生 keystore API 与 `cfg` 分支），让 `identity-auth` 的状态机在所有平台都能编译与单测（[ADR-0006](./adr/0006-identity-keystore-split.md)）。

> `[现状]`（2026-09-25，`daemon-cli-and-local-admin` 切片 4 已落地）上表中 `server` 与 `app` 已落地，但**只覆盖本切片范围**：`server` 只有 `transport` 的本地通道部分（平台 IPC listener、对端凭据校验、framing）与 `local_admin`（管理信封与方法路由），`sync`/`node_link`/`acp_facade` 仍待后续切片；`app` 的 daemon、CLI 与组合根已落地。其余行保持既有状态（`node-link-client` 仍待切片 6）。另外新增一个不影响上表结构的 crate 目录：`vendor/windows-local-ipc`（切片 4 自研的 Win32 FFI wrapper，path 依赖、不在 workspace `members` 里，见 §3.1）。成员仍按「真正落地时才写入 `members`」推进：当前 `members` 就是本节除 `node-link-client` 之外的十二个，§5 的列是这十二个再加 `vendor/windows-local-ipc`，`node-link-client` 只作为「行」出现（待切片 6 落地、当前没有依赖方），详见 §5 表下注记。
>
> （2026-09-26 更新，`node-link-owner` 变更切片 5 已落地）`server` 的范围扩为四条路径：在上一段的 `transport::local` 与 `local_admin` 之上，新增 `transport::net`（默认 loopback 的共享 HTTP/WSS listener、TLS `proxy`/`direct`、`Host` 边界与 path 路由）与 `node_link`（Owner 侧配对 HTTP、节点握手、Export catalog、resource、command 与撤销传播），由 `app` 组合根接线；`sync`/`acp_facade` 与 `node-link-client` 仍待后续切片。

### 3.1 Workspace 基线

创建 workspace 时固定以下基线，避免各 crate 各自漂移：

- `edition = "2024"`（与 `rust-version = "1.85"` 一致，edition 2024 的最低工具链即 1.85），`resolver = "3"`。
- `[workspace.package]` 统一 `version`、`edition`、`rust-version`、`license`、`repository`；第一阶段全部 crate `publish = false`（`§12` 的"是否公开部分 crate"仍未定）。
- `[workspace.dependencies]` 统一第三方版本（tokio、axum、serde、serde_json、sqlx 或 rusqlite、tracing、thiserror 等）；crate 内只写 `workspace = true`；新增依赖按 `AGENTS.md` §7 先审必要性、维护状态、许可证与平台支持。密码学原语固定为 `p256 0.13`（ECDSA P-256）+ `sha2 0.11` + `hmac 0.13` + `base64 0.23`（无填充 base64url）：四者都是纯 Rust、无原生依赖，且已对 `fixtures/*/v1/transcripts/` 的固定向量验证通过（结论与实现约束见 `INITIAL_DESIGN.md` §16 第 6 条）。`core` 的**直接**依赖固定为 `async-trait`/`thiserror`/`p256`/`sha2`（后两者用于 `PeerPublicKey` 的构造期点校验与指纹派生），其中 `p256` **只开 `arithmetic`**——core 不签名也不验签，`ecdsa`/`rfc6979`/`hmac`/`signature`/`pkcs8` 留在协议与身份边界；其普通依赖闭包与 allow-list 见 `docs/CORE_PORTS_AND_STORAGE.md` §9 判据 13 与 `scripts/check-crate-boundaries.mjs`。
- `[决定]` 上述四个密码学原语的**版本口径只维护在上一行**：版本号以 `Cargo.toml` 的 `[workspace.dependencies]` 为准，本行所在的说明与它一致。变更版本必须在**同一改动**里更新这里并给出验证证据；变的是原语集合、算法或实现选择（而不是版本号）时按 `AGENTS.md` §10 走 ADR。
  - 2026-09-24（`identity-auth-and-keystore`）：`hmac 0.12 → 0.13`。理由：`sha2 0.11` 走 `digest 0.11`，而 `hmac 0.12` 要求 `digest 0.10`，同图并存两个 digest 主版本且 `hmac::Hmac<Sha256>` 无法接受 `sha2 0.11` 的类型；`hmac 0.13.0` 的 `rust-version = 1.85` 与本仓库 MSRV 一致（`MIT OR Apache-2.0`，已在 `deny.toml` allow 列表）。消费方：`identity-auth` 的 SAS/配对证明 HMAC 与 `identity-keystore` 的附加熵派生。证据：`reports/du1-pv1.log`（`npm run verify` exit 0，含 workspace 全部测试与 clippy）与 `reports/wp2-workspace-tests.log`；固定向量重算（HMAC 与 SAS 期望值）在 `identity-auth` 的 `tests/transcripts.rs` 中恒常执行。
  - 2026-09-23 基线：`sha2 0.10 → 0.11`、`base64 0.22 → 0.23`（Dependabot PR #1/#2）。证据：workspace 全部测试与 clippy 在该版本上通过（CI 五个 job 全绿、本地 `npm run check:rust` 通过）；`npm run check` 的 transcript 固定向量重算与许可证/来源判定不受影响（JS 侧与版本无关，许可证集合无新增项）。
  - 诚实说明：`INITIAL_DESIGN.md` §16 第 6 条那次一次性 Rust 实测是在 `sha2 0.10`/`base64 0.22` 上做的；本次只验证了「算法语义不变且现有测试通过」，没有重跑那次探针。真正版本无关的回归判据仍是该条要求实现阶段做的事：把同一批固定向量固化成恒常运行的 Rust 测试。
- `[决定]`（2026-09-24）`agent-host` 的平台与日志依赖固定为 `tracing 0.1`（结构化日志，MIT）、`win32job 2`（Windows Job Object；safe API，MIT OR Apache-2.0）与 `nix 0.30`（Unix 进程组结束；`default-features = false`，只开 `signal`/`process`，MIT），三者只登记在 `[workspace.dependencies]`，crate 内写 `workspace = true`。选 `win32job` 而不是 `process-wrap` 的理由是 MSRV：`process-wrap` 10 需要 1.87，高于本仓库 `rust-version = 1.85`（§4.5）；Unix 侧选 `nix` 而不是 `libc` 的理由是 workspace 固定 `unsafe_code = "forbid"`，直接调 `killpg` 必须写 `unsafe` 块（`forbid` 不可用 `#[allow]` 绕过）。版本口径同样只维护在 `[workspace.dependencies]`，本行与它保持一致。
- `[决定]`（2026-09-24）身份边界的依赖口径：`identity-auth`（纯状态机）只依赖 `core`、`sync-protocol`、`node-link-protocol`、`acpr-transcript`（后三者**仅**用于 transcript 编解码与 domain/field tag 表）与 `async-trait`/`thiserror`/`p256`（只增量开启 `ecdsa`，用于**验签**，不签名、不用 `from_der`）/`sha2`/`hmac`；不得依赖 `acpr-wire`、runtime、serde、数据库或 `identity-keystore`，也不得出现平台 `cfg`。`identity-keystore`（已落地）只依赖 `identity-auth`、`async-trait`、`p256`（`ecdsa`，进程内签名）、`sha2`（附加熵派生）、`thiserror`、`getrandom`（OS 随机数；实际取值 `0.4.3`，MIT OR Apache-2.0，MSRV ≤ 1.85）以及 `cfg(windows)` 下的 DPAPI wrapper（`windows-dpapi 0.2.0`，选型结论见 §4.12）。为让端口实现方无需依赖 `core`，`identity-auth` 如实转出端口签名与公开 API 用到的 `core::model` 值对象。`identity-auth` 不另设 `uuid` 依赖：core 的 ID 已是规范 UUID 文本，转 16 字节只需去连字符 + 十六进制解码。
- `[决定]`（2026-09-25，`daemon-cli-and-local-admin` 切片 4）本切片新增的依赖口径（版本只维护在 `[workspace.dependencies]`，本行与它保持一致）：
  - `tokio` 基线 feature 增量开启 `net`/`io-util`/`signal`：`server::transport::local` 用 `net` 的 Named Pipe / Unix socket safe API 与 `io-util` 的连接读写，`app` 用 `signal` 驱动关闭序列。基线只登记跨 crate 共用的 feature；`agent-host` 仍在自己 manifest 里增量要 `process`/`io-util`/`macros`/`rt`（它自己的消费面）——feature 是加法的，两处并存不产生歧义。
  - `nix` 增量开启 `socket`/`user`：Unix 侧用 `SO_PEERCRED` 取对端 uid 与当前进程 `getuid` 比对（§2.2）；不需要 `net`——`sockopt::PeerCredentials` 与 `getsockopt` 都不在 `net` 门后（已核 `nix 0.30.1` 的 feature 表与 `sys::socket` 的门控）。两者只影响 Unix 构建，Windows 构建里 `nix` 不参与编译（§4.5）。
  - 新增 `clap 4.6`（`derive`，CLI 子命令解析）与 `toml 1.1`（`app` 读 `CONFIG_REFERENCE.md` 的配置文件）。本切片只读配置、不回写，因此不引入 `toml_edit`。两者均 `MIT OR Apache-2.0`、显式声明 `rust-version = 1.85`（等于本仓库 MSRV，不抬高）。
  - 新增 `rpassword 7.5`（实际 `7.5.4`）：`provider configure` 的凭据逐项**无回显**读取（`specs/cli-commands` 的场景「凭据无回显录入」）。**Apache-2.0 单许可**——实测不是 `MIT OR Apache-2.0` 双许可，仍落在 `deny.toml` 的 allow 列表内；显式声明 `rust-version = 1.85`（等于本仓库 MSRV，不抬高）。传递依赖 `rtoolbox 0.0.x`（实际 `0.0.6`）自述不保证向后兼容，属**已登记的残余风险**（许可证与 advisory 由 CI 的 `deps`/`advisories` job 判定，本地无等价物）。只被 `app` 使用，**不进入 `core` 闭包**（`CORE_PORTS_AND_STORAGE.md` §9 判据 13 的 allow-list 不受影响）。
  - 单实例锁（`Cargo.toml` 的 `fs4`、`daemon-cli-and-local-admin` 变更的 design.md 决策 4）用 OS advisory 文件锁：选 `fs4 1.1`，按 `docs/SECURITY_DESIGN.md` §20 的四个判据核验——**语义**：unix `flock(LOCK_EX)`、Windows `LockFileEx(LOCKFILE_EXCLUSIVE_LOCK)`，公开 API 只有独占锁与 `try_lock`/`TryLockError`（safe API，unsafe 收敛在 crate 内部）；**许可证** `MIT OR Apache-2.0`（在 `deny.toml` allow 列表内）；**MSRV** 显式声明 `1.75.0`（≤ 1.85）；**维护状态** 1.1.0 发布于 2026-04-28，Windows 侧要求 `windows-sys ^0.61`（与本仓库 lock 里已有的 `0.61.2` 同族，不新增版本族）。落选候选 `fd-lock 4.0.4` 的三条硬伤：未声明 `rust-version`（MSRV 不可核）、最近发版 2025-03-10、API 是读写双分支（`RwLock::read` 走 `LOCK_SH`，与「单实例锁必须互斥」的语义不匹配，且 Windows 侧只锁 1 字节）。
  - 调用点约束：固定工具链的 `std::fs::File` 自带 `lock`/`try_lock`（1.89 稳定），与 `fs4::FileExt` 同名且方法解析优先级更高；本仓库 MSRV 是 1.85，因此必须写全限定调用（`fs4::FileExt::try_lock(&file)`）或显式 `use fs4::FileExt;` 并避免落入 std 的同名方法——用了 std 的版本就等于把 MSRV 抬到 1.89（需单独决定）。
  - `vendor/windows-local-ipc`（`daemon-cli-and-local-admin` 变更的 design.md 决策 3）以**仓库内 path 依赖**登记：写在根 `Cargo.toml` 的 `workspace.exclude`，因而不是 workspace 成员、不继承 `unsafe_code = "forbid"`（它必须写 `unsafe`）、不发布；`server` 是它唯一的依赖方，§5 矩阵因此把它登记为「列」。`deny.toml` 的 `[sources]` 注记说明 path 来源为什么不经 registry/git 判定，以及它的许可证与 wildcard 判定由哪几条承担。
- `[决定]`（2026-09-26，`node-link-owner` 变更 WP1）Node Link 的 HTTP/WS/TLS 依赖口径（版本与 feature 只维护在 `[workspace.dependencies]`，本行与它保持一致；四判据核验的命令、原始输出与来源记录见 `openspec/changes/node-link-owner/reports/wp1-deps.log`）：
  - **`axum 0.8`**（HTTP 路由 + `axum::extract::ws` 的 WebSocket upgrade，底层是 `tokio-tungstenite`）：`MIT`；解析版本 0.8.9 显式声明 `rust-version = 1.80`（≤ 1.85）；2026-04-14 发布，仓库活跃。默认 feature 保留 axum 自己的基线（`http1`/`tokio`/`json`/`tracing` 等），只增量开启 `ws`；axum 的 ws 不依赖 `tungstenite` 的压缩 feature，因此「协商到 `permessage-deflate` 必须拒绝」由 upgrade 层显式判定，不靠关某个 feature。
  - **`rustls 0.23` + `tokio-rustls 0.26`**（TLS `direct` 模式的终止）：许可证分别是 `Apache-2.0 OR ISC OR MIT` 与 `MIT OR Apache-2.0`；两者都声明 `rust-version = 1.71`；2026-09-14 / 2026-09-04 发布，维护活跃。**provider 固定为 rustls 自带的 `ring`**：rustls 的默认 provider 是 `aws-lc-rs`，它经 `aws-lc-sys` 的 build-dependency `cmake` 要求 CMake（Windows x86_64 还需 NASM 或 `prebuilt-nasm`），会把「固定 Rust 工具链即可构建」（`rust-toolchain.toml` 的约定）变成「还要装构建工具」，而且本机与 CI 判定会分叉（本机 `cmake`/`nasm` 都不存在）。`ring 0.17.14` 的许可证是 `Apache-2.0 AND ISC`（两分支都在 `deny.toml` 的 allow 内）、`rust-version = 1.66.0`，且 crates.io 包内带有预生成的汇编/对象文件，构建只需 C 编译器（本机实测通过，见日志）。因为 feature 是加法的，`rustls` 与 `tokio-rustls` **两处都必须** `default-features = false`（只关一侧仍会被 `rustls/aws_lc_rs` 拉回 aws-lc-rs）。落选候选：`aws-lc-rs`（构建前提，见上）；纯 Rust 的 `rustls-rustcrypto`（最新仅 `0.0.2-alpha`，2024-04-24 后再无发布，0.0.x 不足以承担 TLS 安全边界）；`axum-server 0.8`（许可证 `MIT`、`rust-version = 1.82`，本身不违反四判据，但它的 `tls-rustls` feature 写死了 `rustls/aws-lc-rs`，只能经 `tls-rustls-no-provider` 绕过，且它替调用方拥有 listener，与 D2 的「`transport::net` 自己拥有 listener 与 TLS 终止」冲突）。
  - **PEM 解析用 `rustls-pki-types 1` 的 `pem` 模块**（`rustls_pki_types::pem::PemObject`）：许可证 `MIT OR Apache-2.0`、`rust-version = 1.60`、2026-07-23 发布。D1 当初列的候选 `rustls-pemfile 2` 已被上游**归档**（其 README 明确说明能力已并入 `rustls-pki-types` 并给出迁移对照），而该模块本就是 rustls 的传递依赖，因此不再登记这层已冻结的中间层。
  - 测试用自签证书**不提交私钥材料**：`crates/server/tests` 用 dev-dependency `rcgen 0.14` 现算 ECDSA P-256 自签证书与私钥（本次解析到 0.14.7：`rust-version = 1.71`；`0.14.8` 起声明 1.88，MSRV 感知解析因此选 0.14.7）。落选方案是提交一份「仅测试用」PEM fixture：那需要给 `.gitleaks.toml` 加允许清单条目（它现在为空），属安全策略变更，且仍要维护固定材料。
  - 解析与编译证据：`cargo metadata` 解析出 327 个包，其中 `rust_version > 1.85` 的为 0 个，且 `aws-lc-rs`/`aws-lc-sys`/`cmake` 均不在解析图中；`cargo check --locked -p server --all-targets --all-features` 在固定工具链 1.98.1 上通过（`ring` 的原生部分由 `cc` 现场编译出 16 个目标文件与静态库）。许可证对账（本地近似 `cargo-deny licenses`）后，`deny.toml` 的 `[licenses] allow` **无需新增条目**；这批依赖在 WP2 起被 `server::transport::net` 与 `server::node_link` 消费（WP1 只登记与核验）。
  - 同批登记的还有 §5 的 `server` → `acpr-wire` 格：`node-link-protocol` 只再导出 wire 值对象、不再导出 `cj1`，而 `payloadDigest`/`snapshotDigest` 的前像（ACPR-CJ1）只能有一份实现，因此 `server::node_link` 直接依赖 `acpr-wire`；除 digest 计算外不得使用其业务类型（§5 表下注记）。
- `[workspace.lints]` 默认 `clippy::all = "deny"`，并保持 `AGENTS.md` §8 要求的 `cargo clippy --workspace --all-targets --all-features -- -D warnings` 可直接通过。
- 保持默认 `panic = "unwind"`：`AGENTS.md` §7 要求正常路径无 `unwrap()`/`expect()`，而测试与 `cargo test` 需要 unwind；不通过 `panic = "abort"` 掩盖失败。
- workspace 成员随实现增量增长：每个 crate 真正落地时才加入 `members`，最终为 §3 列出的十三个（ADR-0007 引入 `acpr-wire` 后由十二改为十三）；不得为凑齐列表创建只有占位实现的空 crate。切片 4 落地后是十二个：§3 除 `node-link-client` 之外的十二个都在 `members` 里。`vendor/windows-local-ipc` 不是成员（`workspace.exclude`，上一段的依赖口径）。

## 4. 模块职责

### 4.1 `core`

唯一职责：承载与传输、数据库和具体 Agent 无关的会话业务。参考 Pi `agent-core`，首日把稳定且共同演进的领域、用例、端口和协调逻辑放在一个 crate 内，而不是为了目录整齐拆成三个相互依赖的 crate。

内部模块：

```text
core::model       值对象、状态机、事件与不变量
core::use_cases   Session/Config/Permission/Export/Subscription 用例
core::ports       backend、事务存储、身份仓库、时钟等能力接口
core::broker      Session Actor、命令协调与事件提交
core::testing     fake ports 和契约测试工具，仅测试 feature 导出
```

```text
NodeId / DeviceId / ExportId / ImportId / SessionId / EventId / RequestId / TurnId / InteractionId / PairingId / AttachmentId
OwnedSessionRef / RemoteSessionRef / OriginEventRef / SessionReference
Session / SessionSummary / SessionSnapshot / SessionState / Turn / TurnState / ResourceOrigin
ClientCommand / CommandPayload / CommandReceipt / CommandRecord / CommandTerminalRecord / CommandStatus / CommandKind
InteractionId / InteractionKind / InteractionOption / PermissionDecision / InteractionResolution / PendingInteraction / Resolution
Event / EventType / EventKind / EventOrigin / EventPayload / AcpRaw / PersistencePolicy / StoredPolicy
AgentRef / AgentDescriptor / CapabilitySet / ConfigOptionId / ConfigOption / ConfigValue / ModeRef / ModeId
Sequence / Version / AttachmentGeneration / ServerEpoch / OriginEpoch / GlobalCursor / OriginCursor / LocalCursor / Timestamp / Digest
Actor / DeviceRecord / NodeRecord / PairingRecord / ExportRecord / ImportRecord / AuditRecord / AuditAction
```

以上是**冻结全量**；每个类型的形状与不变量见 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §3。

规则：

- `core` 不依赖任何 wire protocol、Tokio runtime、Axum、SQLite、WebSocket、子进程和具体 Agent。
- 领域类型不包含 HTTP 状态码、数据库列名、JSON-RPC method 或 Node Link wire discriminator。
- ID 使用 newtype；状态转换由领域方法验证，adapter 不能直接修改状态字段。
- Owned 与 imported session 必须在类型或 `ResourceOrigin` 上可区分，禁止依靠 nullable `ownerNodeId` 猜测持久化规则。
- `core::broker` 实现每会话串行、active turn、授权调用点、幂等、permission first-writer-wins、模型切换时机和先提交后发布。
- 事件 view 的职责分工（`CORE_PORTS_AND_STORAGE.md` §6 第 19 条）：适配器负责 ACP 派生投影（`block`/`title`/`options`/`deltaIndex` 等，它持有 ACP DTO），`core::broker` 在提交前补 `SYNC_PROTOCOL.md` §10.3 要求的身份与会话版本字段（`turnId`、`version`）并在存储返回后比对版本；两侧不得互相替代，也不得在 broker 的公共领域视图里重建 ACP 语义。

入站 use case 按能力拆分：

```text
SessionCommands / SessionQueries
ConfigCommands
PermissionCommands
SubscriptionQueries
DeviceManagement
ExportManagement
RemoteCatalogQueries
```

出站 port 不使用巨型 `AgentRuntime`，而采用会话级能力：

```text
AgentCatalog             枚举可用 Agent 与能力
SessionBackendFactory    受约束地 create/open SessionEndpoint
SessionEndpoint          prompt/cancel/modes/set-mode/list-config/set-config/resolve-interaction/read-history/close，绑定单个 live session（`modes` 是 `session.mode.list` 的唯一候选来源）
SessionStore             原子提交 owned session 状态、事件与 requestId 幂等；提供 head 与一致性读视图
RemoteDeliveryStore      只提交 imported event 的 cursor/digest/local-sequence 索引
TrustStore               设备、节点配对、信任与撤销元数据
ExportStore              Owner Export 与 Access Import 记录
AuditStore               不含内容的审计记录（读写）
AttachmentStore          内容寻址附件字节与 LRU 清理
EventPublisher           发布已经提交的事件或远程交付
EventSink                后端事件通道；调用顺序即提交顺序
ReadView                 一致性读视图，`sync.snapshot_*` 的 barrier
Clock / IdGenerator      可测试时间与 ID（eventId 由存储层在提交事务内分配）
```

签名以 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §5 为准。

管理状态的端口签名在 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §5.3，SQLite 落盘实现在 `crates/storage-sqlite/src/admin/`（配对确认、撤销与审计、Import 删除与交付清理都是完整管理写集的一次原子提交，§9 判据 23–29）。Daemon/CLI 接线与 `server` 的**本地**入站适配器（`server::local_admin`）已随切片 4 落地（`identity-auth`/`identity-keystore` 见 §4.8/§4.12，`app` 见 §4.10），因此这些管理能力已可经本地通道端到端使用（唯一按合同的例外是 `import.add`：它需要 Owner 的 catalog 快照，而提供该快照的 Access 侧 `node-link-client` 属切片 6，因此它恒返回 `local.unavailable`）；仍未实现的是 `server::sync`/`server::acp_facade` 与 `node-link-client`（`server::transport::net` 与 `server::node_link` 已随切片 5 落地，见 §4.9）。业务决定仍由 core 用例拥有。

`SessionStore` 必须提供单一事务提交 API，不能让 Broker 分别调用 `SessionRepository`、`EventJournal`、`CommandDeduper` 后假设三次调用天然原子。`SessionEndpoint` 表示带生命周期的会话句柄；本地与远程 backend 都实现相同接口，但不得把进程、socket 或 wire DTO 暴露给 core。

`read-history` 服务于 Sync 的 `session.read`：owned session 由 `storage-sqlite` 从本地事件日志回答，imported session 必须由 `node-link-client` 在线向 Owner 取，Access 不得把它写进本地正文缓存；Owner 不可达时返回可区分的错误，由 `server::sync` 映射成 `resource.remote_unavailable`。远程可达性变化通过 `EventPublisher` 以 `session.origin.online_changed` 暴露给客户端，不在 core 里维护独立的在线状态缓存。

端口签名、值对象形状、用例入口与 broker 事务顺序已冻结在 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §3–§6；本节只保留职责边界与依赖规则。

### 4.2 `acp-protocol`

唯一职责：描述并编解码 ACP wire protocol。

包含 JSON-RPC envelope、ACP wire DTO、codec、raw document、capability wire schema、limits 和协议 fixture。协议类型必须保留未知字段与 Agent 扩展 payload。ACP 覆盖集合与跨层验收合同以 [ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) 和 `compatibility/acp/v1/matrix.json` 为准。

已落地的 surface（2026-09-24）：JSON-RPC 信封分类与方向/required 校验、ACP v1 wire DTO（`initialize`/`session/new`/`session/prompt`/`session/update`/`session/request_permission`/elicitation 与 content block）、`RawDocument` 原文承载与逐字节回写、capability wire 形状、固定 v1 消息上限（1 MiB），以及由 `fixtures/acp/v1/manifest.json` 与 `compatibility/acp/v1/matrix.json` 驱动的契约测试。矩阵中 `delivery = post_mvp` 的方法仍只是「可解码与显式不支持」，没有实现。

它不依赖 `core`，不包含领域 mapper，不启动子进程、不管理会话、不访问数据库。ACP wire 到公共领域视图的 mapper 位于 `agent-host` 或 `server::acp_facade`，因为映射方向取决于 adapter 角色。

### 4.3 `sync-protocol`

唯一职责：定义 UI 客户端与其所连接 ACP Remote Node 之间的版本化 wire protocol。

具体消息、transcript、游标、重放和兼容语义由 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) 定义；本模块不得另行发明第二套格式。

包含：

```text
Handshake envelope（信封 + 连接生命周期：认证前后连接字段规则、Phase 校验）
Subscribe / Snapshot / Event / Ack
ClientCommand / CommandAccepted / CommandRejected
Pairing DTO
ProtocolVersion / Feature negotiation
Heartbeat（control.ping / control.pong）
Connection error lifecycle（error body 与 34 个错误码，认证前后两种信封形状）
```

信封的 `body` 在传输层保持原文承载（`RawValue`），家族 body 由调用方显式解析——这条同时适用于已实现的家族与后续增量，`AGENTS.md` §3 的保真要求不允许中间经通用 DTO 往返。

它不允许客户端发送任意 ACP JSON-RPC，也不依赖 `core`。Sync wire 与 core 的 mapper 位于 `server::sync`。

### 4.4 `node-link-protocol`

唯一职责：定义两个 ACP Remote Node 之间的版本化 wire protocol。

包含 Node handshake、Export catalog、resource snapshot/event/ACK、跨节点 command、origin cursor、错误和 feature negotiation。完整语义以 [NODE_LINK_PROTOCOL.md](./NODE_LINK_PROTOCOL.md) 为准。

它不能直接复用 Sync DTO 冒充节点协议，也不能把 ACP stdio 透明封装成网络 tunnel。它不依赖 `core`；Node Link wire 与 core 的 mapper 位于 `node-link-client` 和 `server::node_link`。

`node-link-protocol` 同时拥有机器可验证资产 [`schemas/node-link/v1/`](../schemas/node-link/v1/) 与 [`fixtures/node-link/v1/`](../fixtures/node-link/v1/)；wire 变更必须同时更新两者，Rust 实现消费同一 manifest，不能各自复制一套测试样例。

### 4.5 `agent-host`

唯一职责：把一个本地 ACP 子进程实现为 `AgentCatalog + SessionBackendFactory + SessionEndpoint`。

包含：

- 启动和监督 ACP Agent 子进程。
- stdin/stdout JSON-RPC transport。
- request ID、ACP session ID 与 live endpoint generation 映射。
- capability negotiation。
- stderr 的有界采集与结构化计数、超时、取消和进程树清理。
- 通用 Agent profile registry。
- 隔离的兼容性 quirk。

wire/core mapper 也位于本 crate，但必须把 `acp-protocol::RawDocument` 与公共领域 view 同时交给 core，不能只交规范化文本。优先使用一个通用 ACP 实现；Codex、OMP 差异优先表示为 capability/profile 数据。

实现前已冻结的硬约束（都不是可选优化）：

- **进程树清理**：Windows 必须用 Job Object 管理 Agent 进程树，并把 `KILL_ON_JOB_CLOSE` 设在 Daemon 侧持有的 Job 上。
  - `[决定]`（2026-09-24）**每个 Agent 一个 Job，句柄由 Daemon 侧的 supervisor 持有**：单个全局 Job 无法只结束某一棵 Agent 树（终止整个 Job 会波及全部 Agent），而本 crate 必须支持按 Agent 结束（空闲回收、单个 Agent 崩溃或超时）。`KILL_ON_JOB_CLOSE` 的关键性质（Daemon 崩溃或句柄关闭即停止整棵树）在每 Agent 一个 Job 下同样成立，因为句柄全部由 Daemon 进程持有；「Daemon 持有」指进程所有关系，不限定 Job 的个数。
  - 探针实测生效的 `ExtendedLimitInformation` 布局（144 字节）可直接复用（[INITIAL_DESIGN.md](./INITIAL_DESIGN.md) §16 第 4 条）。该结论已变成**常驻回归测试**（`crates/agent-host/tests/supervision.rs` 的 `tree_forced_termination_stops_the_whole_process_tree`：父→孙两层 + 心跳文件，结束 Agent 后断言心跳不再增长），而不是停留在未提交的一次性探针。Unix 侧用进程组（`process_group(0)` + `killpg`）达到同一效果，平台分支只存在于 `agent-host::platform`（`src/platform.rs` 内按 `#[cfg]` 分模块；`bin/` 不含平台分支，`launch.rs` 只有 1 处 `#[cfg(test)]`）。
  - `[决定]`（2026-09-23 定原则，2026-09-24 收口选型）**用安全 wrapper crate 实现，不给本 crate 放开 `unsafe`**：正式实现不得直接 FFI `kernel32`（那次探针之所以在仓库外，正是因为 workspace 固定 `unsafe_code = "forbid"`）。**已选定 `win32job` 2.x**：safe API（`Job::create_with_limit_info`、`limit_kill_on_job_close`、`set_extended_limit_info`、`assign_process`、`handle`），许可证 MIT OR Apache-2.0，符合 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §20 的许可证口径。**结束某棵树的实际手段是关闭该 Job 的句柄**（`KILL_ON_JOB_CLOSE`）：该 wrapper 没有封装 `TerminateJobObject`，而直接 FFI 被 `unsafe_code = "forbid"` 禁止；内核在最后一个句柄关闭时结束树内全部进程，效果与显式终止一致，已由常驻回归测试固定（§4.5 的进程树回归测试条）。被否的候选：`process-wrap` 10（MSRV **1.87** 高于本仓库 `rust-version = 1.85`，引入它必须先单独决定是否抬 MSRV）、`command-group`（已弃用且不提供 Job Object）。已核验（2026-09-24，本地 registry 元数据）：`win32job 2.0.3` 的 `license = "MIT OR Apache-2.0"`、**未声明** `rust-version`（传递依赖 `thiserror 1.0.69`、`windows 0.61.3` 的 MSRV 低于 1.85；真正的 MSRV 门禁由 CI 的 `deps`/`advisories` job 判定，本地无等价物）；`nix 0.30.1` 的 `license = "MIT"`（与 `win32job` 不同，它只提供 MIT；已核实本地 registry 元数据与 `LICENSE` 正文），其 Unix 分支在本机只做 `cargo check --target x86_64-unknown-linux-gnu` 的编译核验，运行行为由 Linux CI 覆盖。若后续传递依赖抬高 MSRV，回去由用户决策（抬 MSRV，或新增 ADR + 为本 crate 覆盖 lint），不得擅自放开 `unsafe`。
- **stdio 传输**：stdin/stdout 的 JSON-RPC 分帧、request id 映射、session id 映射与 live endpoint generation 都由本 crate 拥有；stderr 必须按大小上限有界收集（内容**不**进入日志，只记结构化计数：丢弃字节数与采集总字节数；内容可按上限回取供诊断），不得无界缓存或直接透传。
- **失败路径**：启动失败、超时、取消、异常退出与乱序响应都必须有明确处理与测试（`AGENTS.md` §7/§9）；正常运行路径不得 `unwrap()`/`expect()`；异步任务必须有所有者、取消路径与关闭顺序。
- **profile 来源**：Agent profile（命令、参数、环境变量白名单、凭据→环境变量绑定）来自 `LocalConfigStore`（[CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §11.6），不读启动配置文件；未列入白名单的环境变量不得注入子进程，凭据值只能经 `CredentialResolver` 在启动前解析（[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §12.2/§13.1）。
- **兼容性 quirk 隔离**：差异优先表达为 capability/profile 数据，不在核心堆积 Agent 名称判断（`AGENTS.md` §5）；真实 Codex/OMP 的兼容报告是发布前产物，不是普通 CI 的硬依赖（[ACP_COMPATIBILITY_MATRIX.md](./ACP_COMPATIBILITY_MATRIX.md) §7）。

### 4.6 `node-link-client`

唯一职责：把 Owner Node 导出的远程 Agent 实现为 `AgentCatalog + SessionBackendFactory + SessionEndpoint`。

它负责 Import 配置、catalog、live attachment、无正文交付索引、跨节点 requestId、origin cursor 和显式重连。参考 Pi client，客户端连接逻辑依赖最小 `NodeLinkTransport` 接口；WSS 是一个 transport 实现，协议和 session backend 不感知 Tailscale 或 socket 细节。

默认 `no-content-cache` 下不得把 prompt、回复、工具内容、diff、终端、附件或 ACP raw 写入 Access SQLite；正文重放回源 Owner。断线后只自动恢复订阅和安全查询，不自动重放副作用命令。`agent-host` 与 `node-link-client` 是平级 backend，不能互相调用。

### 4.7 `storage-sqlite`

唯一职责：实现持久化端口。

包含 schema、migration、`SessionStore`、`RemoteDeliveryStore`、TrustStore 持久部分、事务、容量清理、快照和 TTL。Owned content tables 与 imported delivery-index tables 必须物理或类型隔离，防止 Access 路径误写正文。

管理表、Export/Import 与审计端口的落盘已实现（三个管理 store + 用例层写集路径），具体表设计、事务、v1 → v2 升级和验收见 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §7.2/§7.3/§7.4 与 §11。管理数据保持同一 SQLite 文件与 owned/imported 家族边界，不新增 crate；配置来源权威见 [CONFIG_REFERENCE.md](./CONFIG_REFERENCE.md)。

第一阶段采用**同一数据库文件、两族表 + 每族专属 Store 类型**的隔离方式：

- 表名以 `owned_*` 与 `imported_*` 前缀区分，两族不共享外键、不共享事务边界之外的写入路径；`imported_*` 族不包含任何正文列（prompt、回复、工具内容、diff、终端、附件、ACP raw）。
- 端口层就是隔离面：`RemoteDeliveryStore` 只暴露 `imported_*` 的读写，Access 侧代码拿不到 `SessionStore`，因此"忘了过滤"在类型上不可表达。
- migration 按族分开维护，允许单独重放或清理 `imported_*` 而不影响 owned 权威事件日志。
- 只有当出现"必须靠操作系统级隔离（不同文件/不同权限）才能满足的威胁模型"时，才拆成两个数据库文件，并按 `AGENTS.md` §10 新增 ADR。

它不解析 ACP、不广播 WebSocket、不执行会话状态转换、不保存明文私钥。数据库 record 与领域对象通过 mapper 转换。

`owned_session.version` 的递增规则（含 `StateChange` 的提交 +1，否则不变）由本 crate 在提交事务内实现，是唯一权威；core 只按同一规则推导 view 的 `version` 并在提交后与之比对（`CORE_PORTS_AND_STORAGE.md` §6 第 19 条），双方不一致时 core 失败关闭（`PortError::Corrupt`）而不能静默采用任一方。

v1 的表结构（`owned_*` / `imported_*`）、PRAGMA、migration、保留与容量策略、崩溃恢复与失败关闭已冻结在 [CORE_PORTS_AND_STORAGE.md](./CORE_PORTS_AND_STORAGE.md) §7–§8。

### 4.8 `identity-auth`

唯一职责：实现设备身份、配对、认证和授权——**纯状态机，不依赖任何平台 API**。

内部可分：

```text
pairing/
handshake/
authorization/
port/          # keystore 端口定义（trait），实现见 identity-keystore
```

它负责 Node/设备 P-256 长期身份、PWA canonical origin 绑定、一次性配对、长度前缀 transcript、P1363 challenge-response、scope、撤销，以及**通过端口**访问平台安全存储（[ADR-0006](./adr/0006-identity-keystore-split.md)）。所有密钥读写与**随机性**都由组合根注入的端口提供：本 crate 不直接调用 DPAPI/Keychain/Secret Service，也不直接读系统随机数，也不为平台差异写 `cfg` 分支。密码学原语固定为 `p256` + `sha2` + `hmac`（纯 Rust、无原生依赖）；65 字节公钥前置校验、禁用 DER、接受 high-S 等实测约束见 `INITIAL_DESIGN.md` §16 第 6 条。Export Policy 的业务交集由 core 执行；本 crate 只把验证后的 `Actor`、credential status 和 grant facts 交给 core。Node Identity 与 Device Identity 必须使用不同 key purpose、record type 和签名 domain。

`pack.*`、`preset.*`、`grant.*` 只是授权管理的输入形式：由 `authorization/` 按 [`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json) 展开成命令级 scope 后才写入设备记录或随 wire 下发（`SECURITY_DESIGN.md` §10.2）。core 只看到展开后的 scope 与 grant facts，不认识 pack/preset 名称。

本 crate 的实现前合同（状态机、握手入口、授权展开、nonce/重放、keystore 与熵源端口签名）已随 `identity-auth-and-keystore` 变更定型在 [IDENTITY_AND_AUTH_CONTRACT.md](./IDENTITY_AND_AUTH_CONTRACT.md)：本节只保留职责边界，不重复签名。

### 4.9 `server`

> `[现状]`（2026-09-26，切片 4 与切片 5 已落地）本 crate 已落地的路径有四条：`server::transport::local`（endpoint、对端凭据校验、framing、channel 绑定与未完成请求上限）、`server::local_admin`（管理信封与方法路由）、`server::transport::net`（默认 loopback 的共享 HTTP/WSS listener、TLS `proxy`/`direct` 两种终止方式、`Host` 边界，以及 `/sync/v1`、`/node-link/v1`、两类 `/pairing/*` 的 path 路由）与 `server::node_link`（配对 HTTP、节点握手、catalog、resource、command 与撤销传播，由 `app` 组合根按 §4.10 接线）。**仍未落地**：`sync`/`acp_facade` 待后续切片；Node Link 也只覆盖 Owner 侧入站面，Access 侧的出站重连管理器属切片 6 的 `node-link-client`。`acp_facade` 缺席期间 Daemon 对 `0x02` 连接的处理（完成 framing 校验后立即关闭并记结构化警告）记在 [LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) §3.1 的实现状态注记里；本节的职责划分与下述约束不变。

唯一职责：承载所有入站协议 adapter，类似 Pi server 对连接、attachment 和应用服务路由的集中承载，但不把各协议合并成一个 wire format。

内部平级模块：

```text
server::sync         HTTP/WSS、设备认证、snapshot/event/ACK
server::node_link    节点认证、Export catalog、resource/command/ACK
server::acp_facade   ACP stdio facade 的 daemon 侧：为本地 Zed/IDE 提供 ACP 会话
server::local_admin  平台本地 IPC（Named Pipe / Unix socket）上的管理请求/响应
server::transport    listener 与连接级 backpressure；不放业务命令
```

四个 adapter 只能调用 `core::use_cases`，不能互相调用、查询 SQLite、启动 Agent 或直接调用 `node-link-client`。每个 adapter 自己拥有 wire/core mapper；共享的只有通用连接生命周期原语，禁止抽出“万能消息 DTO”。

`server::acp_facade` 常驻 daemon：ACP 会话状态、幂等记录与事件提交都必须落在拥有该会话的进程里，因此 `acp-remote acp-stdio` 只是“stdin/stdout ↔ 本地通道”的字节泵，不内嵌 core、storage 或 agent-host（否则会与 daemon 争用同一 SQLite，违反单实例锁与单一权威写入者）。本地通道因此承载两类载荷：`server::local_admin` 的管理请求/响应，以及 `server::acp_facade` 的长期双向 ACP 流；两者各自的编码由本地通道适配器拥有，不复用 Sync 与 Node Link 的 DTO。该通道的 endpoint、访问控制、framing、管理信封、两类载荷的会话语义、方法集与本地错误码以 [LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) 为唯一权威来源（ACP 流见其 §3.1）。daemon 未运行时 `acp-stdio` 必须以明确错误退出，不得自行打开数据库或启动第二套核心。

Node Link 和 Sync attachment 必须具有 connection generation 或 attachment ID。重新认证/重新订阅会生成新 generation，延迟到达的旧连接 frame 必须被拒绝，不能误投递到新会话绑定。

### 4.10 `app`

> `[现状]`（2026-09-25 切片 4 已落地；2026-09-26 按切片 5 补充接线范围）`app` 已落地：daemon 的启动/关闭序列与单实例锁（含 `instanceId`）、配置加载与首次种子导入、周期任务（清理/刷盘）装配（Node Link 重连按该变更 design 的非目标只留装配点），以及下面列出的全部 CLI 子命令与 `doctor`/`acp-stdio`。切片 5（`node-link-owner`）另把网络接入面接了进来：`server::transport::net` 的 listener 与 `server::node_link` 的三个路由（catalog/resource/command）随 Daemon 启动，关闭序列先同步停掉网络 accept 再走后续步骤；Node Link 的**出站**重连管理器仍属切片 6，`daemon.status.links` 因此恒为空。二维码图形渲染按 [LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) 的合同解读在本切片记「终端不支持」，CLI 只打印 `pairingUrl` 文本。

唯一职责：发布 `acp-remote` 可执行程序并作为组合根。它装配 daemon、CLI、server、backend、配置、单实例锁、健康状态和 graceful shutdown，但不得承载业务规则。

后台任务的唯一清单（时间与顺序判据见 `CORE_PORTS_AND_STORAGE.md` §7.5 与 `NODE_LINK_PROTOCOL.md` §15）：

- 启动初清理 + 每 60 s 周期的 `prune`/`expire_pairings`/`sweep_orphans`（`CORE_PORTS_AND_STORAGE.md` §7.5）；
- 启动时对已配对 Owner 节点的 Node Link 连接与断线指数退避重连（`NODE_LINK_PROTOCOL.md` §15）；
- broker 的 delta 合并窗口（`storage.flush_interval_ms`；由组合根定时器枚举非终态会话并逐个驱动 `Broker::pump`，`CORE_PORTS_AND_STORAGE.md` §6 第 10 条）；
- 关闭顺序：停接入层（排空在途连接至宽限上限）→ 取消周期任务与信号监听 → 停 Agent → `wal_checkpoint(TRUNCATE)` → 清理 endpoint/释放锁（与 `CORE_PORTS_AND_STORAGE.md` §7.5 第 4 条、`SECURITY_DESIGN.md` §12.1 一致）。

CLI 子命令（名字的唯一来源是 `LOCAL_ADMIN_PROTOCOL.md` §5.8 的映射表；本文只列名字，不重复规则）：

```text
daemon start|stop|status

workspace select
agent configure
provider configure

device pair|list|revoke
node pair|list|revoke
export create|list|revoke
import add|list|remove

acp-stdio
doctor
```

首切片**不提供** `session create`：业务命令 `session.create` 的 transport 只有 `node_link`（本地通道不承载业务命令），本地会话入口是 `acp-stdio`（Zed → `server::acp_facade` → `core::use_cases::create_session`）；`session list` 属 `post_mvp`。理由与展开见 [LOCAL_ADMIN_PROTOCOL.md](./LOCAL_ADMIN_PROTOCOL.md) §5.8、`INITIAL_DESIGN.md` §13.3。

CLI 通过 core use case 或受认证的本地管理 transport 工作，不能复制 core 业务规则。`app` 是唯一允许依赖所有具体 crate 的位置。

### 4.11 `acpr-transcript`

唯一职责：实现 [SYNC_PROTOCOL.md](./SYNC_PROTOCOL.md) §6.2 定义的签名/HMAC 输入编码——magic `ACPR`、`codecVersion`、`domainTag`、严格递增且唯一的 `fieldTag`、长度前缀字段——以及对应的解码与校验错误。`table` 模块在此之上提供表驱动层：泛型表类型（`DomainSpec`/`FieldSpec`/`FieldType`/`Proof`）与"按表校验并编解码"（字段数量、tag 成员、定长宽度）。它是叶 crate（[ADR-0005](./adr/0005-shared-transcript-codec.md)）：不依赖任何其他项目 crate。

它**不**包含任何 `domainTag` 取值、`fieldTag` 取值或协议语义：

- Sync 的 domain 与 tag 表在 `sync-protocol`（对应 `SYNC_PROTOCOL.md` §6.3），Node Link 的在 `node-link-protocol`（对应 §9.2–§9.4，tag 编号与 Sync 独立）；协议 crate 只导出 `DOMAINS` 常量，不重复实现校验或编解码；
- `identity-auth` 依赖本 crate 与两个协议 crate，取表后调用表驱动编解码，自己只负责验证 P-256/HMAC 结果。

`sync-protocol` 与 `node-link-protocol` 正常依赖它，宽度表与"按表校验"因此只有一份实现；协议 crate 之间仍然互不依赖。

### 4.12 `identity-keystore`

唯一职责：实现 `identity-auth` 定义的 keystore 端口，把长期密钥与凭据落到平台安全存储。

> `[现状]`（2026-09-24）下表只有 **Windows/DPAPI** 一列已落地；macOS 与 Linux 的后端**尚未实现**
> （`crates/identity-keystore/src/platform/` 目前只有 `mod.rs`/`windows.rs`/`unsupported.rs`），非 Windows
> 平台统一走失败关闭桩。以下三行是**设计意图**，不是现状陈述。

- Windows：第一阶段用 **DPAPI（当前用户 scope）包裹私钥字节**，签名在进程内完成；CNG/TPM 不可导出档位是后续 ADR 的开放项（[SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §9.2/§20）。**已落地**。
- macOS（未实现，规划中）：Keychain；可用时使用不可导出或硬件保护能力。
- Linux（未实现，规划中）：Secret Service（D-Bus）。没有可用的 Secret Service 时，按 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §20 的当前决定失败关闭，不得静默降级为明文文件。

约束：不实现业务逻辑、不解析协议、不做授权判定；端口与错误类型由 `identity-auth` 拥有（目标签名见 [IDENTITY_AND_AUTH_CONTRACT.md](./IDENTITY_AND_AUTH_CONTRACT.md) §7）；任何密钥字节不得进入日志、协议错误或 `Debug` 输出。除 `app` 外没有其他 crate 依赖它（[ADR-0006](./adr/0006-identity-keystore-split.md)）。

`[决定]`（2026-09-23）平台差异只能以 **wrapper crate + 本 crate 内的 `cfg` 子模块**表达：workspace 固定 `unsafe_code = "forbid"`（`Cargo.toml`，各 crate 继承 `[lints] workspace = true`），因此本 crate **不得**直接 FFI DPAPI/CNG/Secret Service。DPAPI wrapper、Secret Service client 等候选必须按 [SECURITY_DESIGN.md](./SECURITY_DESIGN.md) §20 核验 MSRV、维护状态、许可证与平台支持；DPAPI 只能以「当前用户 scope 包裹 + 进程内 `p256` 签名」的方式使用（私钥在签名瞬间存在于内存，这是已知且已记录的取舍）。Linux 后端在没有 Secret Service 的环境（含 CI 容器）只需保证**编译通过 + 运行时明确失败**，单测走 stub 端口——这正是 [ADR-0006](./adr/0006-identity-keystore-split.md) 拆出本 crate 的目的。

`[决定]`（2026-09-24）DPAPI wrapper 采用 **`windows-dpapi 0.2.0`**（`cfg(windows)` 专属依赖）。核验结论（`SECURITY_DESIGN.md` §20 的四个判据）：

- **语义一致**：只提供 `encrypt_data`/`decrypt_data` + `Scope::{User, Machine}`，本 crate 固定 `Scope::User` 并**只**用它做「包裹秘密字节 + 进程内 `p256` 签名」，公开入口不接受 scope 参数，因此组合根无法顺手改用机器 scope；
- **许可证**：`MIT OR Apache-2.0`，与传递依赖 `anyhow 1`/`log 0.4`/`winapi 0.3.9` 全部落在 `deny.toml` 的 allow 列表内（advisory 判定只在 CI 的 `advisories` job，本地无等价物）；
- **MSRV/edition**：edition 2021、未声明 `rust-version`（实测在仓库固定工具链上编译通过；传递依赖声明的 MSRV **最高者是 `log 0.4.34` 的 1.71**，低于本仓库 MSRV 1.85）；
- **维护状态**：wrapper 本身是新 crate（单作者），传递依赖 `winapi 0.3` 上游已停止维护——这是**已知代价**，记在下面。

已知代价与缓解：

1. `winapi 0.3` 已停止维护，但它只出现在 wrapper 内部（本仓库 `unsafe_code = "forbid"`，不写 FFI），替换路径是自写 wrapper crate 或换用 `windows-sys` 系 wrapper——两者都需要新 ADR（[ADR-0006](./adr/0006-identity-keystore-split.md) 的边界不变）；
2. wrapper 不暴露 `CRYPTPROTECT_UI_FORBIDDEN`，本实现因此**总是**传入由条目头（用途 + 标签 + 版本 + 盐）派生的附加熵，把「无熵 + 缺主密钥时可能弹交互提示」收敛为有熵的静默路径；
3. 附加熵同时把条目绑死到「用途 + 标签 + 版本 + 盐」，因此复制/剪接条目解不开（[PV5] 用真实 DPAPI 断言）。

核验证据：`openspec/changes/archive/2026-09-24-identity-auth-and-keystore/reports/wp3-dpapi-verification.log`（`cargo tree`/`cargo metadata` 输出）与 `openspec/changes/archive/2026-09-24-identity-auth-and-keystore/reports/pv5-windows-dpapi.log`（5 个真实 DPAPI 用例）；两条都写完整路径（避免与仓库根的历史遗留同名文件混淆）。注意：这类日志受 `.gitignore` 忽略、**不进版本库**：归档时随变更目录一起移动（路径已如上更新），因此本机工作区可在归档路径下复核，但任何全新克隆都没有它们——克隆后能复核的是本文件的记录、`reports/*.md` 报告与仓库改动本身。结论已同步到 §3.1。

### 4.13 `acpr-wire`

唯一职责：跨协议共用的 wire 值对象与字段级校验机制（[ADR-0007](./adr/0007-shared-wire-value-crate.md)）——`Uuid`、`DecimalString`、`Timestamp`、`Base64Url<N>`、`FeatureId`、`FeatureList`、`RawAcp`、`Text<N>`、`NonEmptyText<N>`、`BoundedU64<MIN,MAX>`/`UIntAtLeast<MIN>`、`Nullable<T>`（`required` 且可 null 的键存在性语义）、`RawObject`/`ExtraFields`（开放扩展点的保真载体）、`ValueError`、`deserialize_optional_non_null`、**ACPR-CJ1 规范 JSON**（`SYNC_PROTOCOL.md` §3.3：对象成员按 UTF-16 code unit 排序、禁止重复键与浮点、控制字符写作小写 `\u00xx`；`payloadDigest`/`snapshotDigest` 的前像，Rust 侧唯一实现，与 `scripts/check-contract-assets.mjs` 的参考实现互校），以及泛型 `PublicError<Code>`。

它**不**包含任何协议词表或语义：归属检查（哪些语义属于谁）如下——

- 错误码词表（`ErrorCode`）、命令名、grant、domain/tag 取值、会话状态机规则都不在此 crate；`PublicError<Code>` 只提供形状，各协议以自己的错误码枚举具体化；
- 协议专属的值对象留在各自 crate（Sync 的 `cursor`/`sessionSummary`/`originBlock`，Node Link 的 `originCursor`/`sessionMeta`/`exportEntry` 等），不因"看着像"而下沉；
- `Base64Url` 复用 `acpr-transcript` 的规范化 base64url 解码，因此依赖方向是 `acpr-wire → acpr-transcript`，仍是叶 crate 链上的单向依赖。

`sync-protocol` 与 `node-link-protocol` 正常依赖它，字段级校验与开放对象保真因此只有一份实现；协议 crate 之间仍然互不依赖。

## 5. 依赖矩阵
`✓` 表示允许直接依赖：

| From / To | core | acp-protocol | agent-host | storage-sqlite | sync-protocol | node-link-protocol | acpr-transcript | acpr-wire | identity-auth | identity-keystore | server | app | windows-local-ipc |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| core | — |  |  |  |  |  |  |  |  |  |  |  |  |
| acp-protocol |  | — |  |  |  |  |  |  |  |  |  |  |  |
| agent-host | ✓ | ✓ | — |  |  |  |  |  |  |  |  |  |  |
| sync-protocol |  |  |  |  | — |  | ✓ | ✓ |  |  |  |  |  |
| node-link-protocol |  |  |  |  |  | — | ✓ | ✓ |  |  |  |  |  |
| acpr-transcript |  |  |  |  |  |  | — |  |  |  |  |  |  |
| acpr-wire |  |  |  |  |  |  | ✓ | — |  |  |  |  |  |
| node-link-client | ✓ | ✓ |  |  |  | ✓ |  |  |  |  |  |  |  |
| storage-sqlite | ✓ |  |  | — |  |  |  | ✓ |  |  |  |  |  |
| identity-auth | ✓ |  |  |  | ✓ | ✓ | ✓ |  | — |  |  |  |  |
| identity-keystore |  |  |  |  |  |  |  |  | ✓ | — |  |  |  |
| server | ✓ | ✓ |  |  | ✓ | ✓ |  | ✓ | ✓ |  | — |  | ✓ |
| app | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — |  |
| windows-local-ipc |  |  |  |  |  |  |  |  |  |  |  |  | — |

注：本矩阵的「列」是**可被依赖的对象**，「行」是发起方。切片 4 落地后的实际关系：§3 中除 `node-link-client` 之外的十二个 crate 都已是 workspace 成员，也都在本矩阵里成列——`storage-sqlite` 的列此前缺失，已随 `crates/app` 进入 `members` 的同一改动补上（`storage-sqlite` 此前只作为行，缺列会让「成员的依赖不在列里」硬失败，因此这两件事必须同批落地）。`node-link-client` 只作为「行」出现：它待切片 6 落地、不是 workspace 成员，当前没有任何依赖方，因而不需要列。`windows-local-ipc` 是矩阵的**列**（因为 `server` 依赖它），在矩阵里**也有一行**（该行除自身格外全空白——**它自己不依赖任何 crate**）；它是 `vendor/` 下的 path 依赖、**永远不是** workspace 成员（§3.1），其依赖面不进门禁判定，因此那一行只是占位、不需要维护。门禁的覆盖范围如实说明：`scripts/check-crate-boundaries.mjs` 会因「某成员的依赖不在列的集合里」硬失败，但对**行缺席是静默的**（矩阵里查不到该行时它只做列成员判定），因此行与列的增减都必须人工维护，不能指望门禁替你发现漏登记的行。

额外规则：

- 三个 protocol crate 彼此也不直接依赖；它们共享的 transcript codec 结构与表驱动校验来自叶子 crate `acpr-transcript`（协议 crate 只导出自己的 `DOMAINS` 表并调用它），跨协议共用的 wire 值对象与字段校验机制来自叶子 crate `acpr-wire`（`docs/adr/0007-shared-wire-value-crate.md`），包含 ACP raw 的 Node Link 字段只是受约束 bytes/string，不通过 Rust 类型依赖 ACP DTO。
- `storage-sqlite` 依赖 `acpr-wire` **只为** §7.3/§9.9 要求的 `payload_digest = base64url(SHA-256(ACPR-CJ1(payload_json)))`：ACPR-CJ1 是跨 Sync/Node Link 的共享机制，只能有一份实现（v1 早期因为存储层手写摘要而与协议侧口径不一致）；除该函数外不得使用 `acpr-wire` 的业务类型，也不得经它访问协议语义。
- `server` 对 `acpr-wire` 的依赖**只为** ACPR-CJ1 规范 JSON 与 digest 计算（`payloadDigest`/`snapshotDigest` 前像的唯一实现，`NODE_LINK_PROTOCOL.md` §12.4）：`node-link-protocol` 只再导出 wire 值对象、不再导出 `cj1`（`docs/adr/0007-shared-wire-value-crate.md`），因此该实现在 `server::node_link` 侧只能直接取自 `acpr-wire`；除该模块外不得使用 `acpr-wire` 的业务类型，也不得经它访问协议语义。登记时机与理由见 §3.1 的 `[决定]`（2026-09-26）。
- `identity-auth` 依赖两个协议 crate 与 `acpr-transcript` 仅用于 transcript 编解码与 domain/字段 tag 定义：它不复制这些常量，也不使用协议 crate 的业务类型或业务规则。
- `server` 对 `identity-auth` 的依赖只用于完成连接认证；业务授权仍由 core 对 `Actor + grant facts` 执行。
- `app` 可以依赖全部具体 crate，但只做装配；任何其他 crate 不得依赖 `app`。
- `identity-auth` **不得**依赖 `identity-keystore`：keystore 端口由 `identity-auth` 定义、由组合根注入实现；反向依赖会让纯状态机重新绑上平台 API，这正是本次拆分要消除的东西。
- 除 `app` 外没有 crate 依赖 `identity-keystore`。需要密钥存储的模块（例如本地管理入口写入 Provider 凭据）通过 `identity-auth` 的端口表达，不直接 import 平台实现。

## 6. 组合根

依赖注入只发生在 `app`：

```rust
let store = SqliteStore::open(config.database).await?;
let keystore = PlatformKeystore::open(&config.identity)?;
let auth = IdentityAuth::new(keystore, store.identities());
let local = ProcessAgentBackend::new(config.agents);
let remote = NodeLinkBackend::new(config.imports, auth.node_identity());
let backends = RoutedSessionBackend::new(local, remote);

let core = Core::new(CoreDeps {
    sessions: store.session_store(),
    remote_deliveries: store.remote_delivery_store(),
    backends,
    publisher: EventHub::new(),
    clock: SystemClock,
    ids: SecureIds,
});

let server = Server::new(ServerDeps {
    use_cases: core.use_cases(),
    auth,
    sync: config.sync,
    node_link: config.node_link,
});
```

示例只表达装配关系，不锁定最终构造 API。

## 7. 命令与事件路径

命令路径：

```text
server::sync / server::node_link / server::acp_facade / server::local_admin / app::cli
-> core::use_cases
-> SessionBackendFactory / SessionEndpoint
-> agent-host | node-link-client
-> ACP Agent
```

`server::local_admin` 与 `app::cli` 只出现在 `local.*` 能力与配对/Export 管理这些用例上；`app::cli` 的 `acp-stdio` 子命令本身不承载业务规则，它把 stdin/stdout 转给 `server::acp_facade`。

Owned session 事件路径：

```text
ACP Agent
-> agent-host mapper（公共 view + ACP raw）
-> core::broker
-> SessionStore::commit（state + event + dedupe 同一事务）
-> EventPublisher publish
-> subscribers
```

Imported session 事件路径：

```text
Owner 已提交事件
-> node-link-client mapper
-> core::broker
-> RemoteDeliveryStore::commit_receipt（无正文索引）
-> EventPublisher publish（正文仅内存）
-> local subscribers
```

从 cursor 补发 imported 事件时不得伪造历史 view：Access 没有持久化正文，必须按 origin cursor 向 Owner 重新获取对应事件（`resource.subscribe` 的增量重放）再交付；无法回源的区间只能以 `sync.reset_required` 让客户端重建会话视图，不允许发送"只有 digest、没有内容"的伪事件。

Owned 事件只有 core 可以决定何时提交。Imported 事件的业务提交权属于 Owner，Access core 只能提交交付收据，不能把它写成第二份权威 `SessionStore` 内容。

## 8. 错误边界

```text
core::model       DomainError
core::use_cases   UseCaseError / PortError
acp-protocol      AcpError
sync-protocol     EnvelopeError（信封与阶段）+ ValueError（body 字段级）
node-link-protocol 表驱动编解码错误复用 acpr-transcript::table::TableError
acpr-transcript   TranscriptError（codec）+ table::TableError（按表校验）
acpr-wire         ValueError（字段级校验；各协议私有变体不出此 crate）
agent-host        HostError -> PortError
node-link-client  NodeLinkClientError -> PortError
storage-sqlite    StorageError -> PortError
server::sync      TransportError / HTTP mapping
server::node_link NodeLinkTransportError / wire mapping
server::acp_facade AcpFacadeError / JSON-RPC mapping
server::local_admin LocalAdminError / 本地通道编码与权限错误
```

- 外部错误在适配器边界映射。
- `sqlx::Error` 不得进入 core。
- JSON-RPC error code 不得成为领域错误。
- 用户稳定错误码与内部诊断链分离。
- 日志可以保留 source chain，但不能泄漏密钥或完整消息。

## 9. 扩展策略

### 新增 Agent

如果 Agent 正确实现 ACP，只需增加启动 profile、环境变量白名单和 contract tests，不修改 core。仅在协议存在真实差异时增加专属 mapper/quirk。

### 新增客户端

在 `server` 增加入站模块并复用 `core::use_cases`。除非出现新的业务用例，否则不修改 core。

### 新增存储

实现 `SessionStore`、`RemoteDeliveryStore` 等既有事务端口。内存实现服务于测试，其他本地数据库不改变 core。

### 暂不设计动态插件 ABI

第一阶段采用编译期 trait 和静态 registry。只有出现第三方独立发布适配器的明确需求后，再评估进程插件、WASI 或稳定 RPC 插件协议。

## 10. 防止模块腐化

1. 禁止 `common` 或万能 `utils`；工具函数放在拥有其语义的模块。
2. 禁止跨适配器调用，例如 `server::sync -> storage-sqlite` 或 `server::node_link -> node-link-client`。
3. ACP DTO 只存在于 ACP 边界，Sync DTO 只存在于同步边界，DB record 只存在于 SQLite 适配器。
4. 一个状态只有一个权威写入者：Session 属于 Owner core，进程属于 AgentHost，连接属于对应 server/client adapter，节点/设备信任属于 IdentityAuth。
5. 仅当需要阻止反向依赖、多入口复用、独立协议、平台实现或独立测试时才拆新 crate。
6. 文件数量增加本身不是拆 crate 的理由。

### 10.1 ACP 兼容性边界

项目级不变量是：

> ACP Remote 可以增加认证、持久化、排序、重连和多端控制，但不能削弱、重新解释或静默丢弃 ACP 原生能力。

模块实现必须满足：

1. `acp-protocol` 对未知字段和扩展 payload 往返保真。
2. `core::broker` 只使用参与业务决策的公共视图，不把公共视图当作完整 ACP 消息。
3. `agent-host` 不擅自改变 Agent 消息的结构化语义。
4. `server::acp_facade` 如实协商端到端能力，不虚报支持。
5. `sync-protocol` 可以提供移动端友好的表示，但必须能表达结构化能力、明确降级和不支持状态。
6. 任一边界无法处理某项能力时必须显式失败，不能丢弃、曲解或静默转成文本。
7. `node-link-client` 与 `server::node_link` 跨节点时必须保留 ACP raw payload、origin identity 和 capability gate；Access Node facade 只能宣告完整链路交集。

## 11. 测试边界

| 模块 | 测试重点 |
|---|---|
| core | 状态转换、值对象、每会话串行、幂等、事务提交、owned/imported 分流和 fake ports |
| acp-protocol | 官方 fixture、未知字段往返保真、扩展 payload、版本兼容 |
| sync-protocol | 表与 registry 逐项相等、六个 domain 的固定向量逐字节复算、恶意输入；v1 信封（body 字节保真、认证阶段连接字段规则、未知 `type`/字段拒绝）；全部 18 个消息类型与 33 个事件视图的字段级边界、fixture 覆盖与类型化 body 往返；33 个视图夹具按 `viewDef` 投影到同一 event 类型；配对 HTTPS 载荷（二维码/claim/status 的常量与宽度、`canonicalOrigin` 的 origin 形状、approved 与 device/host 的对应关系）；三个漂移门禁（`type` 常量与 schema union 同源抽取、错误码与 registry 的 `(code, retryable)` 逐条相等、`views::VIEW_TYPES` 与 `event-views.schema.json` 的 `$defs` 逐条相等） |
| node-link-protocol | 表与 registry 逐项相等、六个 domain 的固定向量逐字节复算；v1 信封与全部 29 个消息类型（连接字段规则、body 字节保真、类型化 body 往返、非法 body 分层拒绝）；配对 HTTPS 载荷；grant 词表与 `commands.json`、错误码与 registry、`MessageType` 与 schema union 三个漂移门禁 |
| acpr-transcript | 长度前缀编解码往返、字段乱序/重复/缺字段拒绝、magic 与 `codecVersion` 校验；表驱动层的字段数量、tag 成员与定长宽度校验 |
| acpr-wire | 值对象取值域（uuid/decimalString/timestamp/featureList/rawAcp 的正反用例）、`Nullable` 的「缺键报错 / null / 值」三态、`ExtraFields` 的未知字段值保真（含超出 u64 的整数字面量）、base64url 规范无填充口径 |
| agent-host | fake ACP child、超时、崩溃、乱序响应 |
| node-link-client | fake Owner、attachment generation、显式重连、origin 去重、capability 收缩、uncertain |
| storage-sqlite | migration、owned 原子提交、imported 无正文约束、TTL、容量限制 |
| identity-auth | 设备/节点配对过期、重放、无传递信任、grant 交集和撤销 |
| identity-keystore | 平台 keystore 可用与不可用两条路径、无可信 keystore 时失败关闭、密钥不进入日志与错误信息 |
| server::sync | auth、backpressure、续传、限流 |
| server::node_link | Export 过滤、节点 auth、attachment、ACK、backpressure、撤销 |
| server::acp_facade | ACP contract、能力协商真实性、扩展透传、外部 turn 重放 |
| app | 组合冒烟、关闭顺序、单实例 |

端到端测试使用可控的 fake ACP Agent。真实 Codex/OMP 测试作为可选兼容性套件，不作为普通 CI 的硬依赖。各模块测试使用机器矩阵中的 row/test ID 建立证据，矩阵结构由 `npm run check` 检查：矩阵本身、`fixtures/acp/v1` 与 vendored 上游固定快照（`schemas/acp/v1/upstream/schema.json`）都由 ajv 校验，快照 digest 与 commit 另行重算。`sync-protocol` 与 `node-link-protocol` 的编解码、协商和 transcript 测试直接消费 `schemas/sync/v1`、`fixtures/sync/v1`、`schemas/node-link/v1` 与 `fixtures/node-link/v1` 中的 manifest，不另建样例。

命令名、scope、pack、grant 与 transport 的词表以 [`compatibility/commands/v1/commands.json`](../compatibility/commands/v1/commands.json) 为准，Rust 侧不得再硬编码第二份：`sync-protocol`、`node-link-protocol` 的命令判别子必须与该文件由契约测试断言一致，`identity-auth` 的授权展开直接读同一份定义（编译期常量或启动时加载后校验，二者取一，但必须由测试证明与 JSON 一致）。

## 12. 保持开放的决策

- `identity-auth` 是否拆成纯状态机与平台 keystore 两个 crate；平台 keystore 的具体 crate 在选择时按 `AGENTS.md` §7 审必要性、维护状态、许可证与平台支持。
  - 2026-09-18 决定：**拆**。新增第 12 个 crate `identity-keystore`，`identity-auth` 收敛为纯状态机，keystore 以端口注入（[ADR-0006](./adr/0006-identity-keystore-split.md)）。判据是 `AGENTS.md` §4 的「独立平台实现」：平台 keystore 各自拖原生依赖与 `cfg` 分支，且在没有桌面会话的 Linux / CI 容器里不可用，混在一起会让状态机无法在所有平台编译与单测。
  - 2026-09-23 决定：**Windows 第一档位用 DPAPI（当前用户 scope）包裹私钥 + 进程内签名**，Linux 维持失败关闭；CNG/TPM 不可导出档位与 Linux 持久化 fallback 都需单独 ADR。平台实现一律经 wrapper crate（workspace 固定 `unsafe_code = "forbid"`），候选的 MSRV/维护状态/许可证按 `SECURITY_DESIGN.md` §20 核验（[§4.12](#412-identity-keystore)）。与 2026-09-18 那条的关系：拆分不变，本条只是把「具体实现档位」从开放项收口到可落地的第一档。
- 是否为同步协议生成 TypeScript/Kotlin/Swift 类型。
- 是否公开部分 crate 到 crates.io；第一阶段可全部保持 workspace-private。

CLI 与 Daemon 的管理通道已由 [ADR-0004](./adr/0004-local-admin-transport.md) 落定（stdio + 平台本地 IPC），不再开放。

标准不是目录是否整齐，而是边界能否降低耦合、支持独立测试并控制变化传播。

## 13. 架构图

- 可交互 HTML：[acp-remote-modules.html](./diagrams/acp-remote-modules.html)
- 图源 JSON：[acp-remote-modules.architecture.json](./diagrams/acp-remote-modules.architecture.json)

图源 JSON 是权威输入，HTML 是生成物：模块集合、边界或连接发生变化时必须改图源并重新生成 HTML，不允许手改 HTML。生成使用 archify 工具（仓库不内置该 CLI，也不作为运行时依赖，只由 Archify skill 提供）；在 Archify skill 目录下执行，`<repo>` 为仓库根：

```bash
node bin/archify.mjs validate architecture <repo>/docs/diagrams/acp-remote-modules.architecture.json --quality showcase --json
node bin/archify.mjs deliver architecture <repo>/docs/diagrams/acp-remote-modules.architecture.json <repo>/docs/diagrams/acp-remote-modules.html --quality showcase --json
node bin/archify.mjs visual-check <repo>/docs/diagrams/acp-remote-modules.html --json
```

- `validate` 只用于候选修复：showcase 档必须报满 9 项 artifact 检查，且 0 error、0 warning。
- `deliver` 冻结图源字节、渲染并复核后原子替换 HTML，receipt 给出图源与产物两份 SHA-256 与字节数；非零退出不得当作成功。
- `visual-check` 只能在上一步退出码为 0 之后运行（`deliver` 失败会保留上一份已信任 HTML，此时采集到的是陈旧产物）。它把浏览器证据写成 `acp-remote-modules.visual-check.json`，其中 `artifact.sha256` 是「HTML 是否仍与图源一致」的机器判据；`visualReview` 恒为 `pending`，不构成人工视觉审阅结论。同批生成的截图与 contact sheet 是本地证据，当前未纳入版本库。

命令退出码、receipt 字段与三类证据（确定性检查 / 浏览器证据 / 人工视觉审阅）的边界见该 skill 的 `references/delivery-contract.md`。

## 14. 参考

- Pi 当前将 session engine、wire protocol、client、server 和 SQLite backend 分开，并保持 protocol transport-neutral：<https://github.com/earendil-works/pi/tree/main/packages>
- Pi protocol/client/server 对 routed envelope、logical server identity、session attachment 和 transport abstraction 的说明：<https://github.com/earendil-works/pi/tree/main/packages/protocol>
- Oh My Pi 将 AI、Agent Core、Coding Agent、TUI 和 native 能力拆分，并从 interactive、RPC、SDK、ACP 等入口复用同一引擎：<https://github.com/dankalish/oh-my-pi>
