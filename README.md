# ACP Remote

本地优先的 ACP（Agent Client Protocol）中转站：一个节点可以直接管理本地 Agent，也可以通过 Node Link 导入其他节点导出的 Agent；手机、电脑、Zed、PWA 与 CLI 都是客户端形态，不是固定权限角色。

## 仓库当前状态

**设计层完成，实现已开始。** 仓库当前包含权威文档、机器可读合同（JSON Schema）、跨语言固定向量（fixtures）、兼容性矩阵与校验脚本，以及一个随增量增长的 Rust workspace（`docs/MODULE_ARCHITECTURE.md` §3.1）：

| crate | 已实现范围 |
|---|---|
| `acpr-transcript` | 长度前缀 transcript codec 与表驱动校验（叶子 crate，无协议语义） |
| `acpr-wire` | 跨协议共用的 wire 值对象与字段校验机制（叶子 crate，无协议词表） |
| `core` | `core::model` 的值对象与不变量、`core::use_cases` 用例面、`core::ports` 端口签名、`core::broker`（每会话串行、active turn、幂等、交互仲裁、先提交后发布、失败关闭）；运行时依赖只有 `async-trait` + `thiserror` |
| `storage-sqlite` | §7 的 `owned_*`/`imported_*` 表结构与 migration、保留窗口/容量清理（含 imported 家族度量）、附件内容寻址、崩溃恢复与只读失败关闭；实现 `SessionStore`/`ReadView`/`RemoteDeliveryStore`/`AttachmentStore` |
| `sync-protocol` | v1 的全部 18 个消息类型（信封与消息类型分派，`auth`/`sync`/`control`/`error`/`event`/`command` 六个家族 body）、33 个事件视图的类型化投影，以及配对 HTTPS 载荷（二维码 / claim / status / HTTP 错误体） |
| `node-link-protocol` | §9.3/§9.4 的 transcript domain/tag 表、v1 的全部 29 个消息类型（信封与分派，`handshake`/`catalog`/`resource`/`command`/`error` 五个家族 body）与配对 HTTPS 载荷 |

尚未开始：前端工程，以及 `server`、`agent-host`、`node-link-client`、`identity-auth`、`identity-keystore`、`acp-protocol`、`app`（每落地一个才加入 workspace `members`）。

当前优先交付 Windows x64 的 Daemon/CLI 与 Node Link 闭环，Linux 延后开发；完整平台顺序见 [初始设计 §14](docs/INITIAL_DESIGN.md#14-npm-分发)。共享代码的 Linux CI 保留，不代表 Linux 产品已可运行。

管理状态的表设计、事务与升级要求已补充在 [核心与存储合同 §11](docs/CORE_PORTS_AND_STORAGE.md#11-管理状态持久化合同待实现)，SQLite 的 `TrustStore`/`ExportStore`/`AuditStore` 实现仍待完成；现有合同检查只证明当前实现基线一致。

## 权威文档

修改代码或设计前先读 `AGENTS.md`，它列出各文档的职责边界与维护规则。主要入口：

| 文档 | 内容 |
|---|---|
| [docs/INITIAL_DESIGN.md](docs/INITIAL_DESIGN.md) | 产品目标、系统行为、阶段计划 |
| [docs/MODULE_ARCHITECTURE.md](docs/MODULE_ARCHITECTURE.md) | crate / 模块职责与依赖方向 |
| [docs/SYNC_PROTOCOL.md](docs/SYNC_PROTOCOL.md) | 客户端同步协议（认证、cursor、命令、事件） |
| [docs/NODE_LINK_PROTOCOL.md](docs/NODE_LINK_PROTOCOL.md) | 节点间资源导出/导入协议 |
| [docs/LOCAL_ADMIN_PROTOCOL.md](docs/LOCAL_ADMIN_PROTOCOL.md) | CLI 与 Daemon 之间的本地管理通道 |
| [docs/SECURITY_DESIGN.md](docs/SECURITY_DESIGN.md) | 威胁模型、信任边界与安全验收 |
| [docs/ACP_COMPATIBILITY_MATRIX.md](docs/ACP_COMPATIBILITY_MATRIX.md) | ACP v1 覆盖范围与各层策略 |
| [docs/FRONTEND_DESIGN.md](docs/FRONTEND_DESIGN.md) | 前端阶段、状态模型与平台边界 |
| [docs/CONFIG_REFERENCE.md](docs/CONFIG_REFERENCE.md) | Daemon 配置键、类型与默认值 |
| [docs/adr/](docs/adr/) | 已接受的架构决策 |

## 合同检查

机器校验入口只依赖 Node ≥ 22.12（`commitlint` 21 的下限）与仓库内的 devDependencies，不参与运行时产物：

```text
npm ci
npm run check
```

`npm run check` 串行执行九个检查：① schema 与 fixture（ajv, Draft 2020-12，含消息类型与事件视图的覆盖门禁）；② 命令目录的一致性——`commands.json`、两个协议 schema、SYNC §11.5 与 SECURITY §10.2 的表格，以及 `core::broker::required_grant` 这份 Rust 镜像；③ 错误码 registry；④ feature ID 词表（registry、两份协议文档与 fixture 三方一致）；⑤ 需要真正计算的资产绑定（`$ref` 与 `$id`、`rawJson` 字节与摘要、事件 `payloadDigest` 的 ACPR-CJ1 重算、transcript 固定向量重编码与畸形输入负向量）；⑥ ACP 兼容矩阵与 vendored 上游快照；⑦ crate 依赖方向门禁（`MODULE_ARCHITECTURE.md` §5 的矩阵，外加 `core` 依赖闭包的冻结 allow-list）；⑧ 合同漂移门禁（§7 的表结构 ↔ `crates/storage-sqlite/src/migrate.rs`、§5 的端口 ↔ `crates/core/src/ports.rs`）；⑨ agentic 流程与规范（`check:agentic` → `scripts/agentic-gate.mjs`：`openspec-agentic doctor` 断言所用流程确为扩展的 agentic —— 引擎版本等于扩展 pin、`schema: agentic`、受管文件无漂移、AGENTS.md 有验收路由；随后 `openspec validate --all --strict` 校验变更与规范，无活动变更时以 0 退出。该脚本同时设置 `OPENSPEC_TELEMETRY=0`、`OPENSPEC_NO_UPDATE_CHECK=1`、`DO_NOT_TRACK=1`，关闭引擎默认开启的遥测与更新检查）。

CI（[`.github/workflows/ci.yml`](.github/workflows/ci.yml)）在 push 与 PR 上跑同一批检查：`npm ci && npm run check`、`cargo fmt --all -- --check`、`cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`、`cargo test --locked --workspace --all-features`。Linux runner 会真正执行 `#[cfg(unix)]` 的权限路径（`0700`/`0600`/模式位判定），这些在 Windows 开发机上不会跑到。

提交信息遵循 Conventional Commits（`<type>(<scope>)!?: <主题>`）：type 与 scope 词表以 [`commitlint.config.mjs`](commitlint.config.mjs) 为唯一机器定义，本地由 husky 的 `.husky/commit-msg` 钩子在 `npm install` 时装配，CI 的独立 `commits` job 会对本次推送/合并请求引入的提交范围再校验一次（`npm run lint:commits -- --from <base> --to <head>`，`--no-verify` 绕得过本地钩子但绕不过它）。规则说明见 `AGENTS.md` §8。

上游 ACP 固定快照（`schemas/acp/v1/upstream/schema.json`，来源与 sha256 见 `compatibility/acp/v1/matrix.json` 的 `protocol` 块）由 `check:acp` 重算 digest 并校验 commit 与 major 版本目录；`fixtures/acp/v1` 也按同一快照做 ajv 校验。升级快照必须同时改固定值、vendored 文件与矩阵行，且先通过 `node scripts/check-acp-compatibility.mjs`。

Rust 侧改动完成后还需要（见 `AGENTS.md` §8）：

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## 许可

Apache-2.0，见 [LICENSE](LICENSE)。
