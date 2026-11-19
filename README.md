# ACP Remote

ACP Remote 是面向个人使用的 Harness 管理平台，计划在一个用户控制的节点内，统一管理会话、执行、权限审批、历史与审计。

Harness 是实际运行任务的程序，负责模型推理和工具调用；ACP（Agent Client Protocol）是平台与这些程序交互的协议。通用闭环以 Codex 首验，当前目标范围聚焦 Codex 与 Pi（Pi 已建立接入规划、尚未实施，OMP 暂不适配）；当前接入范围见 [Harness Control Plane 设计](docs/HARNESS_CONTROL_PLANE_REDESIGN.md)。无历史重放不再整体拒绝接入，当前连接与后续规划的混合自适应展示快照模式须如实标记历史完整性及不可恢复边界。现有调研不代表真实会话闭环已交付。

## 当前状态

项目已有可运行的 Windows Rust 节点和同源 Web 配对/管理页面，覆盖本机命名管道、DPAPI 身份、SQLite、直接 TLS、配对 HTTP、WSS 管理、设备列表与撤销。普通调试构建也将 Web 资源编译到 exe，可复制单个二进制运行。`establish-node-access` 已完成33例验收及独立检视，于2026-10-30归档。证据覆盖受信 TLS/Tailscale、Chromium A/B、身份持久性、撤销和重启、跨 Windows 用户拒绝，以及已定义的事务、崩溃、限额和供应链检查；不代表后续 Harness 能力已实现。固定版本、验证范围及归档前历史记录见 [归档计划](openspec/changes/archive/2026-10-30-establish-node-access/plan.md) 和本地验收目录 `testing/runs/establish-node-access/`。

归档结果与历史路径映射见本地归档记录 `testing/runs/establish-node-access/2026-10-30-archive-record.md`。完整产品目标见 [Harness Control Plane 设计](docs/HARNESS_CONTROL_PLANE_REDESIGN.md)。

`deliver-codex-session-loop` 已于 2026-11-16 归档，实际验证范围以[归档计划](openspec/changes/archive/2026-11-16-deliver-codex-session-loop/plan.md)为准；归档本身不代表任意 Harness 已验收。Pi 接入规划与范围调整见[能力地图](docs/REBUILD_CAPABILITY_MAP.md)。

`deliver-session-capacity` 已于 2026-11-19 完成 20/20 任务并归档，实际双层 L6 闭环与最终交付范围见[归档计划](openspec/changes/archive/2026-11-19-deliver-session-capacity/plan.md)，实现边界见[节点说明](docs/implementation/session-capacity.md)与[Web 说明](docs/implementation/session-capacity-web.md)；长期规格与验收套件已分别同步至 `openspec/specs/session-resource-policy/spec.md`、`openspec/specs/harness-management/spec.md` 与 `testing/suites/session-resource-policy/tests.md`。

2026-11-19 新增两项远期架构演进规划（均标记 `x-deferred: true`，尚未编写产品实现代码）：一是[单会话客户端控制权接管](openspec/changes/deliver-session-control-takeover/proposal.md)，将单主会话在活跃运行期收敛为至多 1 台受信设备持有写控制权（支持顶号式显式接管，其余设备只读观察，随会话释放或重启自动归零）；二是[混合自适应历史与持久化展示快照库](openspec/changes/deliver-session-transcript-cache/proposal.md)，在保持模型上下文 100% 原生权威的前提下引入物理隔离的结构化展示快照库（默认 30 天滑动保留与 10 GiB 磁盘上限），支撑冷会话零子进程离线快读及跨回收/重启历史展示。

### 归档前阶段记录

以下保留原阶段进展，不作为当前完整交付结论。当时 `deliver-codex-session-loop` 在已有部分实现上按新规格增量调整，完整 Harness/Web 会话闭环尚未交付。原生活动模型、既有管理迁移及公共ACP发现/重放已分别验证；最新请求和队列服务进展见下文。旧会话动作签名、期限/回退水位及有界视图保留在历史实现中，相关新通路仍在替换。旧聊天存储和强刷审批接口只在测试构建中保留，旧数据库内容原样保留，不自动清库。普通启动已装配审批与原生会话服务；认证入口的会话动作、Web页面接线及完整验收仍待完成。[旧会话存储合同](openspec/changes/archive/2026-11-16-deliver-codex-session-loop/session-contract.md)停用；当前行为与阶段依据见[实施合同](openspec/changes/archive/2026-11-16-deliver-codex-session-loop/implementation-contract.md)及[执行计划](openspec/changes/archive/2026-11-16-deliver-codex-session-loop/plan.md)，历史报告仅在原范围有效。

主工程现已合入 SQLite schema9、拥有明确关闭责任的 Node/原生运行组件、共享视图及原始 JSON 保真处理。组件回归和主工程编译、Web 检查已分别执行；普通 Node 启动已装配同一配置、队列、原生运行、审批、视图和正文准备服务；独立工厂验收仍在进行，WSS/Web 完整会话操作及真实 Harness 验收尚未完成。节点内的上传分块、连接绑定、正文准备、共享队列交接、排队正文分块回读及配置文档上传/登记/应用/查询组件已合入；配置读取会核对凭据所属的原节点，拒绝另一节点的凭据。上传、配置管理、队列和正文回读的认证网络路由及页面操作仍待接线。全项目格式检查已通过，严格 lint 仍有待清理项，固定版本与具体结果以[执行计划](openspec/changes/archive/2026-11-16-deliver-codex-session-loop/plan.md)为准。

重建的能力归属、交付闭环与 change 依赖见 [重建能力地图](docs/REBUILD_CAPABILITY_MAP.md)。当前有 8 个未归档远期规划 change（均标记 `x-deferred: true`），包含单会话控制权接管、混合自适应展示快照库与 Pi 的接入规划；节点接入、通用闭环与会话容量的归档证据各自限定范围，其他能力按各自 change 交付。文档检视、上游依赖与真实验收结果分别记录，不能将节点接入验收外推为其他能力已经通过。

新版共享队列、节点请求登记、SQLite schema5及拥有后台任务的队列服务已实现，168项阶段独立验收包含原39项队列测试和追加的物理关闭回归。公共ACP运行时现已接入原始JSON解析，保留未知结构和大数，并提供持续读取、单写入者及可取消和等待关闭的IO所有者；120项阶段独立验收覆盖公开协议路径和自有测试子进程。这些阶段组件与后续原生派发、共享视图已进入普通 Node 装配；网络入口与 Web 共享操作仍待接线，当前配对管理页尚不能操作该队列。详细范围及主工程集成检查见[执行计划](openspec/changes/archive/2026-11-16-deliver-codex-session-loop/plan.md)，阶段测试不代表完整Harness/Web闭环已交付。

本轮 DC-001 已确认的重放计时、默认环境继承及显式接入来源，以及 2026-11-19 确认的单会话控制权接管与混合自适应展示快照库取舍见 [总体设计](docs/HARNESS_CONTROL_PLANE_REDESIGN.md) 和 [领域术语表](openspec/GLOSSARY.md)；规划修订不代表尚未开工的远期 change 已合入产品代码。

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
| [testing/suites/](testing/suites/) | 长期验收用例与结构校验脚本 |

## 贡献与检查

提交信息规则以 [commitlint.config.mjs](commitlint.config.mjs) 为准。本地 Git 钩子校验提交信息；持续集成另执行开发工具依赖审计与密钥扫描，具体步骤见 [CI 配置](.github/workflows/ci.yml)。

请求登记的硬退出重启验收需要显式开启测试夹具：

```powershell
cargo test --manifest-path app/Cargo.toml --locked --features registration-acceptance-fixtures --test registration_restart -- --test-threads=1
```

公共ACP运行时的真实管道和所有者关闭验收也需要显式开启测试夹具：

```powershell
cargo test --manifest-path app/Cargo.toml --locked --features acp-runtime-acceptance-fixtures --test acp_runtime_owned_pipe -- --test-threads=1
```

这两个功能只用于验收子进程；默认构建不生成这些夹具。完整检查入口仍为前述 `npm run check` 和 `npm run build:rust`，Windows测试使用同一实际owner环境。

## 许可证

[Apache-2.0](LICENSE)。

以下保留规格调整时的历史阶段记录；当前实现和验证范围以上文及执行计划为准。

Q-001阶段仅同步规格与规划，新增[ACP额外限制核对](openspec/changes/archive/2026-11-16-deliver-codex-session-loop/specs/acp-platform-constraints/spec.md)。输入及审批受理后由节点接管，批准严格保存，附加审计故障分开处理；当时的代码、迁移及真实Harness/Web验收尚未按新版完成。

Q-002补充：已删除公共ACP客户端累计4096个对端请求ID后使连接失效的旧阈值。失败后旧队列挂起、新输入优先、点击继续恢复旧项已补入规格，完整队列实现仍待后续交付。
