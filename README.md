# ACP Remote

ACP Remote 是面向个人使用的 Harness 管理平台，围绕用户控制的单节点统一管理会话、执行、权限审批、历史与审计。

Harness 是实际运行任务的程序，负责模型推理和工具调用；ACP（Agent Client Protocol）是平台与这些程序交互的协议。当前目标接入范围聚焦 Codex 与 Pi（OMP 等其他宿主暂不适配），总体架构与产品方向见 [Harness Control Plane 总体架构](docs/architecture/overview.md)，能力排期与依赖见 [能力地图](docs/architecture/capability-map.md)。

## 当前状态

项目提供可运行的 Windows Rust 节点（[app/](app/)）与同源 Web 管理端（[clients/web/](clients/web/)）；普通调试与发布构建均将前端静态资源编译内嵌至单个 `acp-remote.exe` 二进制运行。已落地的正式产品规格维护在 [openspec/specs/](openspec/specs/)，长期验收用例维护在 [testing/suites/](testing/suites/)。

### 已交付能力

- **节点接入与客户端信任（`establish-node-access`，2026-10-30 归档）**：覆盖本机命名管道、DPAPI 身份保护、SQLite 存储、直接 TLS / Tailscale 接入、配对 HTTP、WSS 管理连接、可信设备列表与撤销。已完成 33 例验收及独立检视，详见 [归档计划](openspec/changes/archive/2026-10-30-establish-node-access/plan.md)、[Windows 节点访问实现说明](docs/implementation/windows-node-access.md) 与本地记录 `testing/runs/establish-node-access/`。
- **通用会话闭环与 Codex 首验（`deliver-codex-session-loop`，2026-11-16 归档）**：覆盖工作区登记、共享输入队列、ACP 原始 JSON 保真运行时、会话执行、严格权限审批、基础原生会话发现/重放及当前连接视图。实际验证范围以 [归档计划](openspec/changes/archive/2026-11-16-deliver-codex-session-loop/plan.md) 为准；Codex 首验归档不代表任意其他 Harness 已通过真实验收。
- **会话容量与无活动资源回收（`deliver-session-capacity`，2026-11-19 归档）**：覆盖 50 主会话名额上限、30 分钟无活动自动回收请求、已知活动保护与管理账本协调。双层 L6 验收范围见 [归档计划](openspec/changes/archive/2026-11-19-deliver-session-capacity/plan.md)，实现边界见 [节点容量说明](docs/implementation/session-capacity.md)、[Web 容量说明](docs/implementation/session-capacity-web.md) 与 [Web 工作台分层实现](docs/implementation/web-workbench.md)。

### 远期规划

历史导航支持按轮次分页。在 Web 管理端打开会话，父历史首页默认显示最近 10 轮，最新轮在底部。点击“更早”查看上一页，调整每页轮数后点击“重新加载首页”。另一台客户端重读后，如果原生历史内容发生变化，当前页会提示“历史已更新，请刷新”。点击“刷新历史”可按原页大小读取最新内容。新首页完整到达后才替换旧页；刷新失败时可以查看原因并再次点击。查看子历史时输入子会话标识，点击“展开子会话历史”。节点验证父子关系和读取权限后，从子会话第 1 轮开始展示。[历史导航变更](openspec/changes/archive/2026-11-21-deliver-history-navigation/proposal.md)

其余尚未开工的能力演进（如单会话客户端控制权接管、持久化展示快照库、Pi 接入实证、子会话树、外部原生会话纳管、数据生命周期与备份恢复）均在 [openspec/changes/](openspec/changes/) 下标记 `x-deferred: true`，仅维护前期需求与设计，尚未合入产品代码。完整能力归属、依赖关系与横切边界以 [能力地图](docs/architecture/capability-map.md) 和 [领域术语表](openspec/GLOSSARY.md) 为准；已归档变更的验收结论仅在各自声明的版本与范围内有效，不外推至未实施能力。

## 开始参与

开发工具使用 Node.js，版本以 [.nvmrc](.nvmrc) 为准。在仓库根目录执行：

```sh
npm ci
```

Web 工程的构建工具需要单独安装：

```sh
npm ci --prefix clients/web
```

`npm ci` 安装仓库和 Git 钩子所需的开发工具；第二条命令安装同源 Web 工程依赖。Rust 工具链以 [rust-toolchain.toml](rust-toolchain.toml) 为准。

执行当前适用的格式、静态检查、测试和构建：

```sh
npm run check
```

也可以分别执行 `npm run check:spec`（OpenSpec 严格规范与计划/任务/用例结构校验）、`npm run check:rust`、`npm run check:web`、`npm run build:rust` 和 `npm run build:web`。固定 JWS 向量和 WebCrypto 互操作可在真实 Chromium 中复核：

```sh
npm --prefix clients/web run dev -- --host 127.0.0.1 --port 4173 --strictPort
```

浏览器打开 `http://127.0.0.1:4173/tests/browser/jws.html`，页面显示 `PASS` 表示固定 Rust/WebCrypto 签名、浏览器生成签名、high-S、篡改、非法公钥和不安全整数检查均通过。该页面只覆盖 JWS 合同；真实 TLS、DPAPI、named pipe、SQLite、IndexedDB 与完整配对闭环仍需按验收用例执行。

依赖选型、许可证、平台限制与底层安全边界见 [依赖与安全边界说明](docs/implementation/dependencies.md)。

## 启动节点

在 Windows 的目标 owner 账户下构建：

```powershell
npm run build:rust
```

创建自己的 `acp-remote.toml`，例如本机 HTTPS 配置：

```toml
data_dir = "D:/ACPRemote/data"
listen = "127.0.0.1:24443"
canonical_origin = "https://localhost:24443"
tls_cert = "D:/ACPRemote/tls/localhost.crt"
tls_key = "D:/ACPRemote/tls/localhost.key"
```

将路径替换为自己的目录和证书。证书须受浏览器信任且匹配来源名称；远程接入时将 `listen` 改为本机明确的 Tailscale 接口地址，`canonical_origin` 改为证书对应域名和端口。服务直接提供 HTTPS，不另开 Vite 开发服务器。

```powershell
.\app\target\debug\acp-remote.exe --config .\acp-remote.toml daemon start
```

保持该前台终端运行，在同一 Windows 用户的第二个交互终端发起配对：

```powershell
.\app\target\debug\acp-remote.exe --config .\acp-remote.toml device pair
```

从终端显示的 HTTPS 地址打开页面，输入配对码，核对客户端名称和指纹后在本机终端明确批准。后续同源页面自动复用身份并重新认证。

认证后可在同源 Web 界面进行可信客户端管理（分页查看、撤销客户端；自撤销会关闭当前管理连接），以及已登记工作区的会话运行、权限审批与容量状态查看；owner 也可用相同配置执行 `device list`、`device revoke --device-id <ID>` 和 `daemon stop`。更多边界与实现说明见 [Windows 节点访问实现](docs/implementation/windows-node-access.md)、[节点容量说明](docs/implementation/session-capacity.md) 与 [Web 容量说明](docs/implementation/session-capacity-web.md)。

## 仓库导航

| 位置 | 用途 |
|---|---|
| [app/](app/) | Windows Rust 节点、ACP 运行时与本地 CLI 实现 |
| [clients/web/](clients/web/) | 同源 Web 配对、会话与节点管理前端 |
| [docs/architecture/](docs/architecture/) | [总体架构](docs/architecture/overview.md) 与 [能力地图](docs/architecture/capability-map.md) |
| [docs/implementation/](docs/implementation/) | 跨端模块内部实现机制、依赖合规与平台安全边界说明 |
| [docs/hard-rules.md](docs/hard-rules.md) | 从踩坑观察池晋升的强制研发与审查硬规则 |
| [docs/pitfalls.md](docs/pitfalls.md) | 研发与验收复盘踩坑观察池 |
| [openspec/specs/](openspec/specs/) | 已归档落地的长期正式产品规格（产品行为权威来源） |
| [openspec/changes/](openspec/changes/) | 活跃/远期规划变更与 `archive/` 历史归档记录 |
| [openspec/config.yaml](openspec/config.yaml) | OpenSpec 变更流程、门禁与产物规则（含 [schemas/](openspec/schemas/) 与 [role/](openspec/role/)） |
| [testing/suites/](testing/suites/) | 长期验收用例与规范/套件结构校验脚本 |
| [AGENTS.md](AGENTS.md) | 编码 Agent 的工作约束与文档路由 |
| [.agents/](.agents/) | 技能与工具资源 |
| [.github/](.github/) | 持续集成与协作配置 |

## 贡献与检查

提交信息规则以 [commitlint.config.mjs](commitlint.config.mjs) 为准。本地 Git 钩子校验提交信息；持续集成另执行开发工具依赖审计与密钥扫描，具体步骤见 [CI 配置](.github/workflows/ci.yml)。

请求登记的硬退出重启验收需要显式开启测试夹具：

```powershell
cargo test --manifest-path app/Cargo.toml --locked --features registration-acceptance-fixtures --test registration_restart -- --test-threads=1
```

公共 ACP 运行时的真实管道和所有者关闭验收也需要显式开启测试夹具：

```powershell
cargo test --manifest-path app/Cargo.toml --locked --features acp-runtime-acceptance-fixtures --test acp_runtime_owned_pipe -- --test-threads=1
```

这两个功能只用于验收子进程；默认构建不生成这些夹具。完整检查入口仍为前述 `npm run check` 和 `npm run build:rust`，Windows 测试使用同一实际 owner 环境。

## 许可证

[Apache-2.0](LICENSE)。
