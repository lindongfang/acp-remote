# 依赖选型、许可证与底层安全边界

本文记录 Rust 节点与同源 Web 工程的第三方依赖选型依据、许可证合规、特性裁剪及底层 Win32 `unsafe` / 本地补丁驱动安全边界。精确版本号与依赖解析图统一以 [app/Cargo.toml](../../app/Cargo.toml)（及 `app/Cargo.lock`）、[clients/web/package.json](../../clients/web/package.json)（及 `clients/web/package-lock.json`）和根 [package.json](../../package.json)（及 `package-lock.json`）为唯一权威，不在本文重复维护版本号；本文不代替独立安全与供应链验收。

## 选择原则

- 仅在标准库、成熟密码库或已有工程惯例无法可靠满足时新增依赖。
- 密码学使用 RustCrypto 的 P-256 实现与 `rustls` 的 `ring` provider，不自行实现曲线运算、随机数或 TLS。
- Windows 专用依赖通过 `cfg(windows)` 隔离，领域逻辑不直接依赖 Win32。
- Rust 与 Web 依赖均固定到锁文件；CI 使用锁文件安装并执行漏洞审计。
- 同源 Web 仅依赖构建与测试期工具，不引入运行时第三方前端框架、外部脚本、遥测或外部字体。

## Rust 直接依赖

| 依赖 | 用途 | 许可证 | 平台 | 维护与裁剪边界 |
| --- | --- | --- | --- | --- |
| `anyhow` | 应用边界错误上下文 | MIT OR Apache-2.0 | 通用 | dtolnay 维护；只用于应用入口与边界，不进入领域错误类型。 |
| `axum` | HTTPS API、WebSocket 路由 | MIT | 通用 | Tokio 生态项目；仅启用所需路由与 `ws` 能力。 |
| `base64` | JWS 与二进制字段严格编码 | MIT OR Apache-2.0 | 通用 | 封装层显式拒绝填充、非规范尾部和其他字母表。 |
| `bytes` | 有界帧与网络缓冲 | MIT | 通用 | Tokio 生态项目；帧大小仍由协议层独立限制。 |
| `clap` | CLI 解析 | MIT OR Apache-2.0 | 通用 | 标准 CLI 库；敏感参数不得放入命令行。 |
| `fs4` | 数据目录独占锁 | MIT OR Apache-2.0 | Windows/Unix | 操作系统文件锁封装；配合 Windows 多进程独占测试验证。 |
| `futures-util` | WebSocket 流与发送接口 | MIT OR Apache-2.0 | 通用 | 关闭默认宏特性，仅启用 `sink`、`std` 以缩小构建依赖。 |
| `getrandom` | 系统密码学随机源 | MIT OR Apache-2.0 | 通用 | 操作系统密码学随机源；失败必须关闭入口，不降级随机源。 |
| `hex` | 节点 ID、摘要文本编码 | MIT OR Apache-2.0 | 通用 | 小型稳定库；只用于固定长度公开标识。 |
| `hyper-util` | 将节点持有的 TCP/TLS 连接接入 Hyper HTTP 服务并支持升级 | MIT | 通用（验证 Windows x64） | 仅启用 `http1`/`server`/`service`/`tokio`，连接任务由节点取消和收敛。 |
| `mime_guess` | 静态资源 Content-Type | MIT | 通用 | 文件扩展名映射；公开静态资源集合保持白名单封闭。 |
| `p256` | ECDSA P-256、SEC1/P1363 | Apache-2.0 OR MIT | 通用 | RustCrypto 项目；使用成熟验签封装，不手写曲线运算。 |
| `pem` | 读取 TLS PEM 编码 | MIT | 通用 | 证书和私钥只从配置路径读取，不写入日志。 |
| `rust-embed` | 将 Web 构建产物嵌入单二进制 | MIT | 通用 | 启用 `debug-embed`，使普通调试与发布构建均编译期嵌入；缺少构建产物时构建失败，运行时不读取外部构建目录。 |
| `rustls` | 直接 TLS 终止 | Apache-2.0 OR ISC OR MIT | 通用 | 关闭默认 `aws-lc-rs`，显式选用 `ring` provider 与 TLS 1.2/1.3。 |
| `rustls-pki-types` | TLS 证书与私钥类型 | MIT OR Apache-2.0 | 通用 | 私钥类型不实现序列化输出。 |
| `serde` | 配置文件与协议结构编解码 | MIT OR Apache-2.0 | 通用 | 封闭管理协议结构使用 `deny_unknown_fields` 并拒绝重复键；ACP 扩展载荷按协议原始字段规则保留。 |
| `serde_json` | JSON 编解码、规范化输入与原始字段保真 | MIT OR Apache-2.0 | 通用 | 启用 `raw_value` 支持 ACP 未知结构与大数原词法保留；管理侧安全整数、重复键和 RFC 8785 规范化由合同测试保障。 |
| `sha2` | SHA-256 指纹、摘要和 P-256 签名哈希 | MIT OR Apache-2.0 | 通用 | RustCrypto 项目；只使用标准算法。 |
| `sqlx` | SQLite 事务与连接池 | MIT OR Apache-2.0 | Windows/Unix（bundled SQLite） | 关闭默认宏与其他数据库特性，只启用 `runtime-tokio`、`sqlite`（同锁补丁见下文）。 |
| `subtle` | 常量时间相等比较 | BSD-3-Clause | 通用 | dalek-cryptography 项目；用于秘密比较，不替代完整验签。 |
| `thiserror` | 库边界语义错误 | MIT OR Apache-2.0 | 通用 | dtolnay 维护；错误消息不得包含凭据、proof 或配对码。 |
| `tokio` | 异步运行时、网络、IPC 与定时器 | MIT | Windows/Unix | 所有后台任务必须有所有者、取消路径和关闭顺序。 |
| `tokio-rustls` | 异步 TLS 接受连接 | MIT OR Apache-2.0 | 通用 | 关闭默认 `aws-lc-rs`，启用 `ring` 与 TLS 1.2/1.3。 |
| `toml` | 本地节点配置解析 | MIT OR Apache-2.0 | 通用 | 配置错误只报告字段类别，不输出私钥或敏感内容。 |
| `tracing` | 结构化日志接口 | MIT | 通用 | 使用独立脱敏边界，不直接记录敏感请求正文。 |
| `tungstenite` | 区分 WebSocket 容量与协议错误并返回合同关闭码 | MIT OR Apache-2.0 | 通用 | 与 `axum` 共用同一锁定版本；关闭默认特性，仅显式依赖错误与帧类型。 |
| `url` | `canonicalOrigin` 严格解析 | MIT OR Apache-2.0 | 通用 | 合同测试强制拒绝 `userinfo`、路径、`query` 与 `fragment`。 |
| `windows-sys` | SID、token、named pipe、文件安全、DPAPI、Job Object 与字符序比较 | MIT OR Apache-2.0 | 仅 Windows | Microsoft `windows-rs` 项目；`unsafe` 调用集中在平台模块，保留 `GetLastError` 并清理句柄与 DPAPI 输出缓冲。 |
| `zeroize` | 临时秘密内存清零 | Apache-2.0 OR MIT | 通用 | RustCrypto 项目；用于内存中的配对码、私钥材料和临时种子。 |

## Rust 开发依赖

| 依赖 | 用途 | 许可证 | 平台 | 风险与限制 |
| --- | --- | --- | --- | --- |
| `rcgen` | 生成隔离测试 TLS 证书 | MIT OR Apache-2.0 | 通用 | 仅测试使用，不成为部署信任根；测试证书不得进入真实运行数据目录。 |
| `tokio-tungstenite` | 协议探针与 WebSocket 合同测试 | MIT | 通用 | 仅测试使用；不替代真实 Chromium 验收。 |
| `tower` | 路由请求合同测试的 `ServiceExt::oneshot` | MIT | 通用 | 仅测试使用；不成为节点运行时依赖。 |

## Web 与仓库工程开发依赖

| 依赖 | 所属工程 | 用途 | 许可证 | 边界与限制 |
| --- | --- | --- | --- | --- |
| `TypeScript` | `clients/web` | 静态类型检查 | Apache-2.0 | 开启严格模式，版本按锁文件固定。 |
| `Vite` | `clients/web` | 静态资源构建与开发服务器 | MIT | 生产构建只生成第一方静态资源，不使用第三方 CDN。 |
| `Vitest` | `clients/web` | 前端逻辑单元测试 | MIT | 仅开发期运行，不代替真实浏览器 IndexedDB / WebCrypto 验收。 |
| `@fission-ai/openspec`、`@commitlint/*`、`husky` | 根工程 | 规范校验与提交信息 Git 钩子 | MIT | 仅本地开发与 CI 校验使用，不进入产品构建产物。 |

## 平台与安全限制

- 产品运行目标为首期 Windows x64；未声明 Linux / macOS 部署支持。
- `windows-sys` 只在 Windows 编译；Win32 `unsafe` 调用集中封装，并为 token、句柄、文件、管道和 DPAPI 输出建立确定性清理路径。DPAPI 固定使用当前用户范围与 `CRYPTPROTECT_UI_FORBIDDEN`，不使用 `CRYPTPROTECT_LOCAL_MACHINE`。
- SQLite 使用 bundled 构建，避免依赖目标机系统安装的 SQLite，代价是增加 C 编译链和二进制体积。
- `rustls` 固定使用 `ring` provider；未启用 `aws-lc-rs`、OpenSSL 或系统根证书自动信任。
- Web 构建无运行时第三方依赖；浏览器端只使用同源静态资源、WebCrypto 和 IndexedDB。

## Windows 子进程所有权与 Job Object 安全边界

`windows-sys` 启用 `Win32_System_JobObjects`（用于进程树归属、活动进程计数与整树终止）与 `Win32_Globalization`（通过 `CompareStringOrdinal` 执行 Windows 环境变量名的不区分大小写覆盖与排序，避免 Unicode 大写展开改变名称等价关系）。

`platform::process_owner` 的 `unsafe` 与句柄边界约束：

- **无 Shell 直接拉起**：`CreateProcessW` 以绝对程序路径、逐项 UTF-16 Windows `argv` 转义和独立 `cwd`/`env` 调用，不经过 `cmd.exe` 或 PowerShell。`ProcessSpec` 不实现 `Debug`，错误仅包含固定阶段与 Windows 数字错误码；参数、路径与环境值不进入错误消息，上层不得将节点私钥放入环境，NUL、非法环境名和过长命令行在启动前拒绝。
- **父环境继承与保留键区分**：继承系统环境与调用方显式 `overrides` 分开校验——对继承父环境保留 Windows 隐藏驱动器当前目录条目（`=<ASCII字母>:`）以及命令链留下的 `=ExitCode`、`=ExitCodeAscii` 原值；对调用方显式 `overrides` 仍拒绝任何包含 `=` 的键名。
- **句柄白名单继承**：父端 `stdio` 使用 Tokio overlapped named pipe（管道名含 192-bit 系统随机值，复用 owner-only DACL、`first-instance` 与拒绝远端），无永久阻塞读线程；子端 `stdio` 使用同步句柄支持常规 CLI，仅有这三个句柄进入 `STARTUPINFOEX` 显式继承白名单，Job、根进程/线程与父管道均不可继承。
- **挂起入组防逃逸**：根子进程以 `CREATE_SUSPENDED` 创建，在执行任何用户代码前设置 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` 并绑定 Job Object，随后才 `ResumeThread`；不启用任何 breakaway 标志。句柄由 `OwnedHandle` 唯一关闭，属性清单由 `DeleteProcThreadAttributeList` 配对释放，无跨 `await` 裸指针。
- **整树收敛判定**：`OwnedProcess` 始终持有 Job，`take_io` 只转交一次异步 IO；`wait_exit` 仅代表根进程退出，而 `shutdown` 按 Job 活动进程数归零证明整树退出（根进程先退出不释放孙进程清理责任）。超过总预算时包含最多 100 ms 强制终止及退出观察预留，`Incomplete` 返回最后观察计数并保留所有权，`Drop` 尽力调用 `TerminateJobObject` 并由 `KILL_ON_JOB_CLOSE` 兜底。共享实例引用、审批、持久化及 `stdio` 读写任务由上层实例 owner 协调，单个会话释放不得直接强杀共享实例。

## SQLite 本地同锁补丁与生命周期监督

`sqlx-sqlite` 使用 [app/vendor/sqlx-sqlite](../../app/vendor/sqlx-sqlite) 中的完整同锁补丁驱动；上游来源归档摘要、提交及许可证见 [PROVENANCE.md](../../app/vendor/sqlx-sqlite/PROVENANCE.md)。依赖名称、锁版本、特性及第三方依赖图与锁文件保持一致；补丁不修改 `sqlx-core` 或连接池算法，将 SQL close/ACK 与后台物理线程 `join` 分开记录。

[sqlite_lifecycle.rs](../../app/src/session_storage/sqlite_lifecycle.rs) 负责同一 Tokio runtime 上的打开宿主、worker 的 blocking join 槽及其观察句柄；[storage.rs](../../app/src/storage.rs)、[schema.rs](../../app/src/session_storage/schema.rs) 与注册器收据共享同一 worker 域。节点在数据库打开前将宿主登记到 acquisition 槽，进程入口保持 runtime 直到 `join_actual` 完成；最后观察者 `Drop` 只请求停止，在未确认完成前继续持有根资源与句柄。
