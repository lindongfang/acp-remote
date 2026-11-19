# Windows 节点访问与身份保护实现

本文记录 Windows 节点在数据目录独占、DPAPI 身份密钥、生命周期关闭顺序、本机命名管道（Named Pipe）与 CI 所有者方面的内部实现机制与安全边界，不重复定义对外协议。对外行为权威来源为 [节点接入规格](../../openspec/specs/node-access/spec.md) 与 [客户端信任规格](../../openspec/specs/client-trust/spec.md)，详细协议契约与历史验收记录见归档变更中的 [接入合同](../../openspec/changes/archive/2026-10-30-establish-node-access/access-contract.md)、[归档计划](../../openspec/changes/archive/2026-10-30-establish-node-access/plan.md) 及本地目录 `testing/runs/establish-node-access/`。

## 适用范围

- 首期只支持 Windows x64，节点以前台进程运行在当前数据目录所有者账户下。
- 不将节点配置为 Windows 服务或共享账户运行。服务账户没有可用的当前用户 DPAPI 配置时，身份初始化必须失败，不能降级到机器范围或明文密钥。
- 本机身份只证明操作系统用户，网络客户端身份不代替应用认证；相同 Windows 用户下的恶意软件访问已解密内存或冒充 owner 不在防护范围内。

## 数据目录与独占

数据目录先创建并规范化，再设置只允许当前 owner SID 的受保护 DACL。目录内的 `.node.lock` 同样只允许 owner 访问，并通过操作系统文件锁阻止第二个节点使用同一目录。

进程正常退出或错误路径释放锁后，后续实例可以重新获取；实现不通过删除未知 PID 文件或抢占其他进程资源恢复锁。启动失败不能以清理数据目录或生成新身份作为重试手段。

## Windows 身份密钥

节点私钥是独立的 `identity.key` 文件，不写入 SQLite。密钥明文只包含格式标记和 32 字节私钥种子，并使用 Windows DPAPI **当前用户范围**保护：

- 直接调用 `CryptProtectData` / `CryptUnprotectData`，不设置 `CRYPTPROTECT_LOCAL_MACHINE`。
- 使用固定用途上下文作为 DPAPI entropy，拒绝错误上下文解密。
- 设置 `CRYPTPROTECT_UI_FORBIDDEN`，后台启动路径不得弹出交互式系统对话框。
- 失败信息保留 Windows `GetLastError`，但不记录明文、私钥或密文。
- 私钥种子、DPAPI 明文和临时缓冲在离开作用域时清零。

首次初始化先为临时文件设置 owner-only DACL，再写密文、刷盘并原子改名为 `identity.key`，最后以 SQLite 事务写入 `node_meta`。只有已存在身份痕迹时才允许恢复或失败关闭：

- 有 key、无 meta：解密并校验 key 后补写 meta。
- 有 meta、无 key：拒绝启动。
- key 与 meta 不匹配：拒绝启动。
- 无 key、无 meta，但已有客户端、命令或审计历史：拒绝启动。
- 未知 schema：拒绝启动，不降级、不清空、不生成替代身份。

崩溃与重启恢复测试覆盖密钥改名前后及 meta 提交前后的中断场景：重启后只允许恢复原密钥或安全完成初始化，不能用新身份覆盖已有信任根。

## 节点启动与关闭

`acp-remote daemon start` 在单个前台进程内按以下顺序取得资源：

1. 读取并校验配置，冻结规范化 HTTPS origin。
2. 创建并规范化数据目录，取得独占文件锁并收紧 owner-only DACL。
3. 打开 SQLite，拒绝未知或不识别 schema。
4. 使用当前用户 DPAPI 恢复或初始化节点身份。
5. 加载证书链和私钥，建立直接 TLS 服务配置。
6. 绑定显式监听地址，再绑定 owner-only named pipe 首实例。
7. 以上全部成功后打印完整 `nodeId`、规范化 origin 和监听地址，报告 ready。

任一阶段失败都返回非零退出码，不打印 ready。监听端口、SQLite、数据目录锁和管道句柄由本次启动路径持有；失败或关闭时按网络入口、连接、会话/运行时责任、数据库、身份和目录锁的顺序释放。`daemon status` 与 `daemon stop` 只使用 owner-only named pipe，不依赖 Web 客户端身份。

普通调试构建默认启用 `debug-embed`，二进制内嵌同源 Web 页面及静态资源，不依赖外部 `dist` 目录。TLS 入口协商 HTTP/1.1，提供配对 HTTP 端点与同源 WSS 管理入口，未知路径返回封闭的 404 错误；配对与管理的来源限流按真实 TCP 对端 IP 计数，不读取 `Forwarded` 或 `X-Forwarded-For`，避免客户端更换临时端口绕过预算。

## 本机 named pipe

本机 IPC 地址由当前 owner SID 和规范化数据目录的 SHA-256 摘要派生，同一目录可重复得到相同地址，不需要随机 nonce 或外部状态文件。

服务端 named pipe 约束：

- 创建 owner-only DACL，不把 `SYSTEM`、`Everyone` 或客户端提供的 SID 加入允许列表。
- 第一个实例启用 `first-instance` 限制，额外实例不能绕过同目录独占。
- 设置 `reject_remote_clients`，远程命名管道连接在服务端拒绝。
- 连接后模拟客户端 token，读取实际 user SID，与节点 owner 精确比较。
- 校验结束后显式 `RevertToSelf` 并检查结果；失败时拒绝请求，不能继续在模拟身份下运行。

控制消息使用单行 JSON，请求上限 32 KiB，最多同时处理 32 个连接，单次短请求最长 30 秒。`daemon status` 返回状态、节点 ID、origin、版本和 PID；`daemon stop` 等待网络、运行时与存储资源收敛后返回结果，IPC 在资源收敛后关闭。

## 运行账户边界与 Windows CI 文件所有者

- **受限沙箱账户边界**：若受限沙箱将 `USERPROFILE` 指向另一个用户但缺少对应 DPAPI 主密钥，当前用户 DPAPI 会返回 `ERROR_FILE_NOT_FOUND`，且私有文件 DACL/原子提交可能被拒绝。DPAPI 与私有文件校验必须在目标 owner 的真实 Windows 用户会话中执行。
- **Windows CI 默认文件所有者**：GitHub Windows 托管 runner 的管理员进程可能以 `Administrators` 组作为新文件的默认所有者，而节点要求目录和受管文件归当前用户 SID 所有。CI 在启动检查的同一 PowerShell 进程中载入 `testing/ci/windows-owner.ps1`，通过 Windows `TokenOwner` 设置当前进程创建对象的默认所有者，并创建临时目录断言实际所有者匹配当前用户。脚本不更改已有数据目录的所有者、系统策略或产品校验；API 调用或断言失败即停止检查。新对象所有者语义见 [Microsoft 文档](https://learn.microsoft.com/en-us/windows/win32/secauthz/owner-of-a-new-object)。
