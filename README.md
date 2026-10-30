# ACP Remote

ACP Remote 是面向个人使用的 Harness 管理平台，计划在一个用户控制的节点内，统一管理会话、执行、权限审批、历史与审计。

Harness 是实际运行任务的程序，负责模型推理和工具调用；ACP（Agent Client Protocol）是平台与这些程序交互的协议。通用闭环以 Codex 首验，ZCode 能力待核实；DeepSeek 当前已核对的 rc.5 ACP 不提供历史重放，按已确认的准入规则暂不接入，保留后续调研。

## 当前状态

项目已有可运行的 Windows Rust 节点和同源 Web 配对/管理页面，覆盖本机命名管道、DPAPI 身份、SQLite、直接 TLS、配对 HTTP、WSS 管理、设备列表与撤销。普通调试构建也将 Web 资源编译到 exe，可复制单个二进制运行。`establish-node-access` 已完成33例验收及独立检视，于2026-10-30归档。证据覆盖受信 TLS/Tailscale、Chromium A/B、身份持久性、撤销和重启、跨 Windows 用户拒绝，以及已定义的事务、崩溃、限额和供应链检查；不代表后续 Harness 能力已实现。固定版本、验证范围及归档前历史记录见 [归档计划](openspec/changes/archive/2026-10-30-establish-node-access/plan.md) 和 [验收记录](testing/runs/establish-node-access/)。

归档结果与历史路径映射见[归档记录](testing/runs/establish-node-access/2026-10-30-archive-record.md)。完整产品目标见 [Harness Control Plane 设计](docs/HARNESS_CONTROL_PLANE_REDESIGN.md)。

`deliver-codex-session-loop` 的领域、存储及适配器开发已暂停，Harness/Web 会话闭环尚未交付。2026-10-30 最新确认采用通用 ACP 重放兼容，缺少历史重放的 Harness 暂不接入；会话及历史仍以 Harness 为准，平台只使用有界、可丢弃视图，不保存独立聊天档案或编写厂商专用读取器。页面分页可来自全量流式重放，首次加载成本依全部历史量而定。只提供原生删除，查看允许激活进程且不限只读。现有未提交实现及数据保留；[旧会话存储合同](openspec/changes/deliver-codex-session-loop/session-contract.md)已暂停使用，旧实施合同及完整验收计划仍待重基准。开发及独立验证范围见[当前执行计划](openspec/changes/deliver-codex-session-loop/plan.md)，不将文档或内部开发测试作为产品验收。

重建的能力归属、交付闭环与 change 依赖见 [重建能力地图](docs/REBUILD_CAPABILITY_MAP.md)。10个 change 的规划文档已建立；节点接入已实现并归档，其他能力仍按各自 change 交付。文档检视、上游依赖与真实验收结果分别记录，不能将节点接入验收外推为其他能力已经通过。

本轮 DC-001 已确认的重放计时、默认环境继承及显式接入来源取舍见 [总体设计](docs/HARNESS_CONTROL_PLANE_REDESIGN.md) 和 [领域术语表](openspec/GLOSSARY.md)；本次同步仅为文档修订，不代表现有会话代码已符合新版规则。

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

也可以分别执行 `npm run check:rust`、`npm run check:web`、`npm run build:rust` 和 `npm run build:web`。固定 JWS 向量和 WebCrypto 互操作可在真实 Chromium 中复核：

```sh
npm --prefix clients/web run dev -- --host 127.0.0.1 --port 4173 --strictPort
```

浏览器打开 `http://127.0.0.1:4173/tests/browser/jws.html`，页面显示 `PASS` 表示固定 Rust/WebCrypto 签名、浏览器生成签名、high-S、篡改、非法公钥和不安全整数检查均通过。该页面只覆盖 JWS 合同；真实 TLS、DPAPI、named pipe、SQLite、IndexedDB 与完整配对闭环仍需按验收用例执行。

依赖选择、许可证、平台限制和已知风险见 [节点接入依赖审查](app/DEPENDENCIES.md)。

Rust 应用位于 [app/](app/)，同源 Web 位于 [clients/web/](clients/web/)。

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

将路径替换为自己的目录和证书。证书须受浏览器信任且匹配来源名称；远程接入时将 listen 改为本机明确的 Tailscale 接口地址，canonical_origin 改为证书对应域名和端口。服务直接提供 HTTPS，不另开 Vite 开发服务器。

```powershell
.\app\target\debug\acp-remote.exe --config .\acp-remote.toml daemon start
```

保持该前台终端运行，在同一 Windows 用户的第二个交互终端发起配对：

```powershell
.\app\target\debug\acp-remote.exe --config .\acp-remote.toml device pair
```

从终端显示的 HTTPS 地址打开页面，输入配对码，核对客户端名称和指纹后在本机终端明确批准。后续同源页面自动复用身份并重新认证。

认证后在节点管理页查看可信客户端，使用“载入下一页”继续查看。选择目标设备的“撤销此客户端”并确认后，其管理连接失效；自撤销会关闭当前页面的管理连接。owner 可用相同配置执行 `device list`、`device revoke --device-id <ID>` 和 `daemon stop`。更多资源边界与验证说明见 [Windows 节点访问](docs/WINDOWS_NODE_ACCESS.md)。

## 仓库导航

| 位置 | 用途 |
|---|---|
| [docs/](docs/) | 产品与技术设计，目前包含已确认的产品方向 |
| [AGENTS.md](AGENTS.md) | 编码 Agent 的工作约束 |
| [openspec/config.yaml](openspec/config.yaml) | 变更流程规则 |
| [openspec/schemas/](openspec/schemas/) | 变更产物结构与模板 |
| [openspec/role/](openspec/role/) | 开发、测试与审查职责 |
| [.agents/](.agents/) | 技能与工具资源 |
| [.github/](.github/) | 持续集成与协作配置 |

## 贡献与检查

提交信息规则以 [commitlint.config.mjs](commitlint.config.mjs) 为准。本地 Git 钩子校验提交信息；持续集成另执行开发工具依赖审计与密钥扫描，具体步骤见 [CI 配置](.github/workflows/ci.yml)。

## 许可证

[Apache-2.0](LICENSE)。
