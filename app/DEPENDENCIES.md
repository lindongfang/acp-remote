# Node Access Dependency Review

审查日期：2026-10-29。范围为本 change 新增的 Rust 应用和 Web 构建工具；版本以 `app/Cargo.lock` 与 `clients/web/package-lock.json` 为准。本文记录选择依据和已知风险，不代替独立安全验收。

## 选择原则

- 仅在标准库、成熟密码库或已有工程惯例无法可靠满足时新增依赖。
- 密码学使用 RustCrypto 的 P-256 实现与 rustls 的 ring provider，不自行实现曲线运算、随机数或 TLS。
- Windows 专用依赖通过 `cfg(windows)` 隔离，领域逻辑不直接依赖 Win32。
- Rust 与 Web 依赖均固定到锁文件；CI 使用锁文件安装。
- 本 change 的 Web 只依赖构建期工具，不引入运行时框架、第三方脚本、遥测或外部字体。

## Rust 直接依赖

维护状态依据上游项目归属、版本可用性和当前维护渠道做初步判断；本次未联网核验全部项目的发布活跃度或已知漏洞。

| 依赖 | 版本 | 用途 | 许可证 | 平台 | 维护与风险判断 |
| --- | --- | --- | --- | --- | --- |
| anyhow | 1.0.104 | 应用边界错误上下文 | MIT OR Apache-2.0 | 通用 | dtolnay 维护；只用于应用边界，不进入领域错误类型。 |
| axum | 0.8.9 | HTTPS API、WebSocket 路由 | MIT | 通用 | Tokio 生态项目；依赖 Tokio/Hyper，供应链随框架扩大。 |
| base64 | 0.22.1 | JWS 与二进制字段严格编码 | MIT OR Apache-2.0 | 通用 | 广泛使用；需在封装层拒绝填充、非规范尾部和其他字母表。 |
| bytes | 1.12.1 | 有界帧与网络缓冲 | MIT | 通用 | Tokio 生态项目；帧大小仍须由协议层独立限制。 |
| clap | 4.6.7 | CLI 解析 | MIT OR Apache-2.0 | 通用 | 活跃标准 CLI 库；不得把敏感参数放入命令行。 |
| fs4 | 1.1.0 | 数据目录独占锁 | MIT OR Apache-2.0 | Windows/Unix | 小型锁封装；仍需真实 Windows 多进程验证。 |
| futures-util | 0.3.34 | WebSocket 流与发送接口 | MIT OR Apache-2.0 | 通用 | futures-rs 项目；关闭默认宏特性，减少不需要的构建依赖。 |
| getrandom | 0.3.4 | 系统密码学随机源 | MIT OR Apache-2.0 | 通用 | Rust 随机生态基础库；失败必须关闭入口，不降级随机源。 |
| hex | 0.4.3 | 节点 ID、摘要文本编码 | MIT OR Apache-2.0 | 通用 | 小型稳定库；只用于固定长度公开标识。 |
| hyper-util | 0.1.21 | 将节点拥有的 TCP/TLS 连接接入 Hyper HTTP 服务并支持升级 | MIT | 通用（本项目验证 Windows x64） | Hyper 项目维护；仅启用 http1/server/service/tokio，连接任务由节点取消和收敛；版本以锁文件为准。 |
| mime_guess | 2.0.5 | 静态资源 Content-Type | MIT | 通用 | 文件扩展名映射；公开静态资源集合仍须白名单封闭。 |
| p256 | 0.14.0 | ECDSA P-256、SEC1/P1363 | Apache-2.0 OR MIT | 通用 | RustCrypto 项目；实现/验签封装使用成熟 API，不手写曲线运算。 |
| pem | 3.0.6 | 读取 TLS PEM 编码 | MIT | 通用 | 小型解析库；证书和私钥只从配置路径读取，不写入日志。 |
| rust-embed | 8.13.0 | 把 Web 构建产物嵌入单二进制 | MIT | 通用 | 启用 debug-embed，使普通调试构建与发布构建均编译期嵌入；缺少构建产物必须使构建失败，运行时不读取构建目录。 |
| rustls | 0.23.46 | 直接 TLS 终止 | Apache-2.0 OR ISC OR MIT | 通用 | rustls 项目；使用 ring provider，关闭 aws-lc-rs 特性以缩小构建面。 |
| rustls-pki-types | 1.15.1 | TLS 证书与私钥类型 | MIT OR Apache-2.0 | 通用 | rustls 项目；私钥类型不实现序列化输出。 |
| serde | 1.0.229 | 配置文件与封闭协议结构 | MIT OR Apache-2.0 | 通用 | serde 项目；所有协议结构使用 `deny_unknown_fields` 并拒绝重复键。 |
| serde_json | 1.0.154 | JSON 编解码与规范化输入 | MIT OR Apache-2.0 | 通用 | serde 项目；安全整数、重复键和 RFC 8785 规范化需另有合同测试。 |
| sha2 | 0.10.9 | SHA-256 指纹、摘要和 P-256 签名 | MIT OR Apache-2.0 | 通用 | RustCrypto 项目；只使用标准算法，不自定义哈希。 |
| sqlx | 0.8.6 | SQLite 事务与连接池 | MIT OR Apache-2.0 | Windows/Unix，bundled SQLite | 活跃数据库库但依赖面较大；关闭默认宏与其他数据库特性，只启用 runtime-tokio、sqlite。 |
| subtle | 2.6.1 | 常量时间相等比较 | BSD-3-Clause | 通用 | dalek-cryptography 项目；用于秘密比较，不替代完整验签。 |
| thiserror | 2.0.21 | 库边界语义错误 | MIT OR Apache-2.0 | 通用 | dtolnay 维护；错误消息不得包含凭据、proof 或配对码。 |
| tokio | 1.54.1 | 异步运行时、网络、IPC 与定时器 | MIT | Windows/Unix | Tokio 项目；所有任务必须有所有者、取消路径和关闭顺序。 |
| tokio-rustls | 0.26.6 | 异步 TLS 接受连接 | MIT OR Apache-2.0 | 通用 | 关闭默认 aws-lc-rs，启用 ring 与 TLS 1.2/1.3。 |
| toml | 1.1.8 | 本地节点配置 | MIT OR Apache-2.0 | 通用 | toml-rs 项目；配置错误只报告字段类别，不输出私钥内容。 |
| tracing | 0.1.44 | 结构化日志接口 | MIT | 通用 | Tokio 生态项目；使用独立脱敏边界，不直接记录请求正文。 |
| tungstenite | 0.29.0 | 区分 WebSocket 容量与协议错误并返回合同关闭码 | MIT OR Apache-2.0 | 通用 | 已由 axum 和测试探针使用的同一锁定版本；本次仅显式依赖错误类型，不启用额外特性或引入新解析器。公告库扫描仍待执行。 |
| url | 2.5.8 | canonicalOrigin 严格解析 | MIT OR Apache-2.0 | 通用 | servo/rust-url 项目；仍需合同测试拒绝 userinfo、路径、query、fragment。 |
| windows-sys | 0.61.2 | SID、token、named pipe、文件安全和 DPAPI API | MIT OR Apache-2.0 | 仅 Windows | Microsoft windows-rs 项目；unsafe 调用集中在平台模块，直接保留 `GetLastError` 并清理 DPAPI 输出缓冲。 |
| zeroize | 1.9.0 | 临时秘密清零 | Apache-2.0 OR MIT | 通用 | RustCrypto 项目；用于内存中的配对码、私钥材料和临时种子。 |

## Rust 开发依赖

| 依赖 | 版本 | 用途 | 许可证 | 平台 | 风险与限制 |
| --- | --- | --- | --- | --- | --- |
| rcgen | 0.14.10 | 生成隔离测试 TLS 证书 | MIT OR Apache-2.0 | 通用 | 只在测试中使用，不成为部署信任根；测试证书不得进入运行数据目录。 |
| tokio-tungstenite | 0.29.0 | 协议探针与 WebSocket 合同测试 | MIT | 通用 | 只在测试中使用；不能替代真实 Chromium 验收。 |
| tower | 0.5.3 | 路由请求合同测试的 `ServiceExt::oneshot` | MIT | 通用 | 只在测试中使用；不成为节点运行时依赖。 |

## Web 构建依赖

| 依赖 | 版本 | 用途 | 许可证 | 风险与限制 |
| --- | --- | --- | --- | --- |
| TypeScript | 7.0.2 | 静态类型检查 | Apache-2.0 | Microsoft 维护；严格模式开启，版本按锁文件固定。 |
| Vite | 8.3.4 | 静态资源构建与开发服务器 | MIT | Vite 项目；生产构建只生成第一方资源，不使用第三方 CDN。 |
| Vitest | 5.0.5 | 前端逻辑测试 | MIT | Vitest 项目；只在开发期运行，不能代替真实浏览器 IndexedDB/CryptoKey 验收。 |

## 平台与安全限制

- 产品运行目标为首期 Windows x64；未声明 Linux/macOS 部署支持。
- `windows-sys` 只在 Windows 编译；Win32 unsafe 调用集中封装，并为 token、句柄、文件、管道和 DPAPI 输出建立清理路径。DPAPI 固定使用当前用户范围与 `CRYPTPROTECT_UI_FORBIDDEN`，不使用 `CRYPTPROTECT_LOCAL_MACHINE`。
- SQLite 使用 bundled 构建，避免依赖目标机安装的 SQLite，但增加 C 编译链和二进制体积。
- rustls 使用 ring provider；未启用 aws-lc-rs、OpenSSL 或系统证书自动信任。
- Web 构建没有运行时第三方依赖；浏览器端只使用同源静态资源、WebCrypto 和 IndexedDB。

## 已执行检查

- `cargo check --all-targets --locked`：通过。
- `cargo clippy --all-targets --locked -- -D warnings`：通过。
- 2026-10-29 最终非沙箱 Windows 用户会话中 `cargo test --all-targets --locked` 通过，113 项 Rust 库测试及 4 项 JWS 向量；覆盖当前用户 DPAPI、跨进程恢复、崩溃点恢复、owner-only DACL、named pipe token 校验、未知 schema 保护以及启动后立即关闭的 IPC 竞态回归。
- `npm run check`、`npm --prefix clients/web run test`（6 个文件、30 项）和 `npm --prefix clients/web run build`：通过。
- 根仓库与 Web 的 `npm audit --audit-level=high`：均为 0 个已知漏洞；Web 审计覆盖 81 个依赖。
- 2026-10-30 独立验收已取得并校验官方 cargo-audit 0.22.2，针对261个锁定包及1386条公告的官方快照扫描：0漏洞、无warnings；未查询yanked状态。工具来源、快照提交和真实范围见 `testing/runs/establish-node-access/2026-10-30-independent-supplychain.md`。这不等于已完成全部维护/许可/平台审查或秘密扫描问题闭环。
