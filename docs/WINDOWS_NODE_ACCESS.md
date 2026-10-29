# Windows 节点访问与身份保护

状态：节点和 Web 已可运行，当前change的33例验收及独立检视已通过，于2026-10-30归档。开发全量检查与独立结果分别记录。本文记录 Windows 资源边界及历史、当前证据，不重复定义协议。行为权威来源是 [接入合同](../openspec/changes/archive/2026-10-30-establish-node-access/access-contract.md)，产品行为见长期[节点接入规格](../openspec/specs/node-access/spec.md)和[客户端信任规格](../openspec/specs/client-trust/spec.md)；固定产品和归档前进度见 [plan](../openspec/changes/archive/2026-10-30-establish-node-access/plan.md)。

## 适用范围

- 首期只支持 Windows x64，节点以前台进程运行在当前数据目录所有者账户下。
- 不将节点配置为 Windows 服务或共享账户运行。服务账户没有可用的当前用户 DPAPI 配置时，身份初始化必须失败，不能降级到机器范围或明文密钥。
- 本机身份只证明操作系统用户和网络客户端身份不代替应用认证；相同 Windows 用户下的恶意软件不在防护范围内。

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

开发测试会实际终止子进程，覆盖密钥改名前后和 meta 提交前后。重启后只允许恢复原密钥或安全完成初始化，不能用新身份覆盖已有信任根。

## 节点启动与关闭

`acp-remote daemon start` 目前在单个前台进程内按以下顺序取得资源：

1. 读取并校验配置，冻结规范化 HTTPS origin。
2. 创建并规范化数据目录，取得独占文件锁并收紧 owner-only DACL。
3. 打开 SQLite，拒绝未知或不识别 schema。
4. 使用当前用户 DPAPI 恢复或初始化节点身份。
5. 加载证书链和私钥，建立直接 TLS 服务配置。
6. 绑定显式监听地址，再绑定 owner-only named pipe 首实例。
7. 以上全部成功后打印完整 `nodeId`、规范化 origin 和监听地址，报告 ready。

任一阶段失败都返回非零退出码，不打印 ready。监听端口、SQLite、数据目录锁和管道句柄由本次启动路径持有；失败或关闭时按网络入口、连接、数据库、身份和目录锁的顺序释放。`daemon status` 与 `daemon stop` 只使用 owner-only named pipe，不依赖 Web 客户端身份。

当前 TLS 入口只协商 HTTP/1.1。已接入四个配对 HTTP 端点和同源 WSS 管理入口；未知路径仍返回封闭的 404 错误。配对与管理的来源限流按真实 TCP 对端 IP 计数，不读取 `Forwarded` 或 `X-Forwarded-For`，避免客户端更换临时端口绕过预算。上述实现已通过组件自测，并已在真实受信 Tailscale origin 上完成 Chromium A/B 开发闭环；这仍不能替代独立 tester/reviewer 验收。

## 本机 named pipe

本机 IPC 地址由当前 owner SID 和规范化数据目录的 SHA-256 摘要派生，同一目录可重复得到相同地址，不需要随机 nonce 或外部状态文件。

服务端 named pipe：

- 创建 owner-only DACL，不把 SYSTEM、Everyone 或客户端提供的 SID 加入允许列表。
- 第一个实例启用 first-instance 限制，额外实例不能绕过同目录独占。
- 设置 `reject_remote_clients`，远程命名管道连接在服务端拒绝。
- 连接后模拟客户端 token，读取实际 user SID，与节点 owner 精确比较。
- 校验结束后显式 `RevertToSelf` 并检查结果；失败时拒绝请求，不能继续在模拟身份下运行。

控制消息使用单行 JSON，请求上限 32 KiB，最多同时处理 32 个连接，单次短请求最长 30 秒。`daemon status` 返回状态、节点 ID、origin、版本和 PID；`daemon stop` 等待网络与存储资源收敛后返回结果，IPC 在资源收敛后关闭。

独立测试已验证owner DACL、第一实例、同owner token、错误SID拒绝、还原后线程无模拟token、消息上限和真实32/33连接边界。旧真实另一SID的文件/pipe拒绝、status/list/revoke及可读密文上的DPAPI拒绝有按负责字节复用证据；另一SID的pair/stop及真实远程pipe仍未完成。五个一次性平台失败适配器已验证错误关闭与真实线程恢复，不声称持续Win32还原失败也可恢复。

## 验证记录

2026-10-29 最新开发复核在真实 Windows 用户会话执行完整 Rust 与 Web 检查：Rust 库 113 项（无过滤，包括 DPAPI、跨进程和 named pipe 测试）、JWS 固定向量 4 项、Web 6 个文件共 31 项及生产构建全部通过；根工程和 Web 的 `npm audit` 均为 0 个漏洞。该结果是本机开发自测记录，不等于独立验收。

首次完整 Rust 运行暴露了启动后立即关闭时 owner control channel 等待 15 秒的竞态：`watch::Sender::send` 在尚无接收者时不会保存新状态。实现改为用 `send_replace` 保存停止/关闭状态，并在 IPC 服务进入 `select!` 前检查当前状态；修复后原先超时的 4 个节点重启、独占、端口失败清理和身份损坏测试与完整套件均通过。

后续完整套件又暴露测试自身在 `daemon stop` 与 IPC 停止确认之间抢跑，偶发得到 Windows `os error 232`。测试现先启动 stop 任务并等待 `stop_received`，再启动 250 ms shutdown；产品关闭语义未变。连续 10 次定向测试和完整套件通过。

真实 Chromium 首次配对还暴露了浏览器 `fetch` 的 receiver 丢失：保存后的 `globalThis.fetch` 若作为对象方法调用会报 `Illegal invocation`。实现改为在包装函数内调用 `globalThis.fetch(...args)`，并新增 receiver-sensitive 回归测试，Web 测试数由 30 增至 31。

以下表格保留上述 2026-10-29 非沙箱 Windows x64 开发自测记录：

| 检查 | 结果 |
| --- | --- |
| `cargo check --all-targets --locked` | 通过 |
| `cargo clippy --all-targets --locked -- -D warnings` | 通过 |
| `cargo test --lib --locked` | 通过，113 项，无过滤 |
| `cargo test --all-targets --locked` | 通过：Rust 库 113 项、JWS 固定向量 4 项 |
| `node::tests::shutdown_does_not_publish_a_report_before_the_ipc_task_finishes` | 通过，单独精确执行 |
| `cargo test --test jws_vectors --locked` | 通过，4 项 |
| `npm --prefix clients/web run check` | 通过 |
| `npm --prefix clients/web run test` | 通过，6 个文件、31 项测试 |
| `npm --prefix clients/web run build` | 通过 |
| 根与 Web `npm audit --json` | 通过，0 个漏洞 |

关键测试：

- `node::tests::starts_reports_ready_and_stops_through_owner_ipc`
- `node::tests::serves_the_embedded_web_ui_and_fails_closed_on_unknown_paths`
- `node::tests::serves_the_owner_started_pairing_lookup_over_tls`
- `node::tests::pairing_lookup_rate_limit_uses_the_tcp_ip_not_ephemeral_ports`
- `node::tests::management_handshake_rate_limit_uses_the_tcp_ip_not_ephemeral_ports`
- `node::tests::management_websocket_authenticates_queries_subscribes_and_revokes_devices`
- `node::tests::management_websocket_recovers_identity_and_rejects_replayed_proofs`
- `node::tests::management_websocket_enforces_the_per_client_connection_limit`
- `node::tests::management_websocket_cursor_becomes_stale_after_a_committed_revision_change`
- `node::tests::management_websocket_receives_going_away_when_the_node_stops`
- `node::tests::shutdown_does_not_publish_a_report_before_the_ipc_task_finishes`
- `node::tests::identity_survives_restart_and_resources_are_reusable`
- `node::tests::invalid_tls_and_occupied_port_fail_without_exhausting_resources`
- `node::tests::rejects_a_second_instance_for_the_same_data_directory`
- `node::tests::corrupt_existing_identity_fails_without_replacement`
- `tls::tests::rejects_a_private_key_that_does_not_match_the_certificate`
- `platform::windows::tests::dpapi_round_trip_uses_current_user_scope`
- `platform::windows::tests::private_paths_allow_only_the_current_owner`
- `platform::windows::tests::named_pipe_enforces_owner_dacl_sid_and_first_instance`
- `identity::tests::dpapi_identity_is_recovered_by_a_new_process`
- `identity::tests::process_crashes_recover_identity_without_replacement_material`
- `identity::tests::unknown_schema_blocks_identity_initialization`
- `identity::tests::key_and_meta_mismatch_fails_closed`

受限沙箱账户如果把 `USERPROFILE` 指向另一个用户但没有对应 DPAPI 主密钥，会让当前用户 DPAPI 返回 `ERROR_FILE_NOT_FOUND`。受限沙箱还会拒绝身份密钥文件的 DACL/原子提交操作，使部分身份测试返回权限错误。这些是运行账户边界，不是可以忽略的测试通过；DPAPI 和私有文件检查必须在目标 owner 的真实 Windows 用户会话中执行。

### Chromium A/B 开发自测

2026-10-29 先使用真实 Windows 节点、自签名 localhost 证书、ego Chromium A 和隔离 Edge B 完成一次开发自测。该轮证书错误仅通过测试专用自动化忽略，不作为受信证书证据；完整记录见 [最终开发运行](../testing/runs/establish-node-access/2026-10-29-final-development.md)。

- A/B 均完成配对码 lookup、原子 claim、本机交互终端明确 `y` 批准、WSS hello/challenge/proof 认证和 `device.list`；浏览器读取到的 P-256 私钥均为 `extractable === false`。
- 在批准响应丢失后立即停止节点，再以同一数据目录重启；B 使用已保存公钥恢复认证，状态从 `pending_confirmation` 变为 `consumed`，并保持 owner 批准时记录的 clientId，无需重新执行 `device pair`。
- B 的全部 Edge 进程退出后，以同一 profile 重开，IndexedDB 仍可读取 `consumed` 身份和不可导出私钥，随后 WSS 认证成功。
- A 撤销 B 后，A 的同一管理连接继续可用，列表 revision 更新到 5；B 的既有连接以 `4410`（`revoked`）关闭，页面显示已撤销。
- B 后续每次刷新都建立新的 WSS 连接，并在发送公钥持有证明后收到 `4401`（`authentication failed`）。
- 节点通过 owner IPC 正常停止并输出 `ACP Remote node stopped`；A 页面进入节点停止状态。停止后进程、TCP `28444`、`acp-remote.*` named pipe 均不可见，同目录重启成功，证明数据目录锁已释放。
- 节点重启后 A 使用相同 clientId 和非导出私钥恢复认证；已撤销 B 仍收到 `4401`，撤销状态没有因重启复活。

随后使用 `tailscale cert` 为 `faraday.tail74842e.ts.net` 取得 Let's Encrypt 受信证书，并以 `https://faraday.tail74842e.ts.net:28444` 完成一轮真实 Tailscale 开发测试。Chromium 未忽略证书错误，页面 `isSecureContext === true`；A/B 重新完成 lookup、claim、本机显式 `y` 批准、WSS 认证和 `device.list`。A 撤销 B 后列表 revision 为 3，B 既有连接收到 `4410 / revoked`，重载收到 `4401 / authentication failed`；正常停止后端口释放，同目录重启时 A 恢复、B 继续拒绝。证书、私钥和浏览器资料只保存在 `.tmp/`，完整记录见 [受信 TLS 与 Tailscale 开发运行](../testing/runs/establish-node-access/2026-10-29-trusted-tailscale-development.md)。

## 2026-10-30 当前验证与剩余范围

R20开发全量检查实际通过：144项Rust库测试、4项固定签名向量、50项Web测试，以及格式、Clippy、类型和生产构建。原始日志为`.tmp/implementation-r20-check.log`；固定源码清单和独立结论另存testing/runs。前面的113/118/143项记录属于当时版本，不能作为当前数量或独立签署。

- 原停止信号、响应收敛和超大WSS帧问题已由独立reviewer关闭。随后独立发现的撤销受理/入队协调、首次订阅漏revision、本机32连接击穿节点、终态旧claim重试和debug资源未嵌入均已有定向修复与独立闭环，详见同日r9/r12/r14/r15检视及预算报告。
- 普通调试构建启用debug-embed，exe包含Web页面及资源。独立reviewer实际从隔离源码构建产品、复制exe、隐藏并修改该副本dist，仍通过真实HTTPS取得原页面与JS/CSS。旧debug构建依赖外部dist，不能仅靠旧exe摘要事后声称其Web内容已经冻结。
- 真实SQLite提交失败、实际COMMIT失败、SQLITE_FULL容量适配器、10000结果满额和精确24小时时钟、真实DPAPI子进程在写入/刷盘前/更名/meta边界中止，以及默认15秒总关闭预算的诚实失败均有独立子项证据。各适配器和真实边界按报告区分，不扩写为全部故障组合。
- 官方Gitleaks与cargo-audit已取得并校验。独立历史/源码秘密扫描及固定RustSec快照扫描通过，锁文件与依赖维护/许可/目标平台审查有记录；不再记为工具未安装。公告快照不是对未来漏洞的保证。
- 应用内审批策略入口已经建立并独立验证：有效管理资格不能代替缺失、过期或未验证的后续审批。后续Harness仍为明确测试适配器，不声称真实Harness集成。
- 当前NA-033只保留用户本人对提示可理解性的判断，用户已回答通过。旧安全NA-033合并到NA-002/012/014，旧NA-034对应新NA-033；无需再次人工安全签字。

本机UNC SMB正控制可双向通信而普通产品管道拒绝，独立reviewer确认远程拒绝开关实效子项通过；这是本机回环SMB，不宣称另一机器或另一SID。R17迟到身份写入问题已独立关闭，慢writer两种真实WSS队列超限也已独立通过。

管理分页、撤销并发和浏览器多页恢复已由独立tester收口，详见同日r17-management、web-evidence-merge及r6-e2e报告。R20已提交批准后Ctrl-C提示的原问题也由原tester真实终端复测关闭，详见[复测报告](../testing/runs/establish-node-access/2026-10-30-approval-cancel-r20-retest.md)：先给诚实恢复指引，再按同指纹查询确认实际ID；信任保留且同key重启恢复。

另一真实Windows SID的pair/stop已使用用户授权的现有abc账户补验，详见[跨SID独立补验](../testing/runs/establish-node-access/2026-10-30-cross-sid-r20-acceptance.md)。两个真实控制台命令均exit1、无超时，在私有目录解析阶段被DACL拒绝；不冒称该次到达服务端token比较。随后owner查询正常，数据库及审计逻辑摘要不变，正常stop释放端口/pipe/锁；看守进程后续缺席确认独立保存，原初次观察不改写。账号未创建、删除或修改，密码不进入测试资产。NA012其他正交子项沿用报告明确的独立证据范围，最终逐例与交付结论见plan引用的独立汇总和检视。

用户原30447体验节点未被这些源码构建自动替换。同用户恶意进程访问已解密内存或冒充owner仍在既定威胁边界之外。

## Windows CI 的文件所有者

GitHub Windows 托管 runner 的管理员进程可能以 Administrators 组作为新文件的默认所有者，而节点要求目录和受管文件归当前用户 SID 所有。CI 在启动检查的同一 PowerShell 进程中载入 `testing/ci/windows-owner.ps1`，通过 Windows TokenOwner 设置当前进程创建对象的默认所有者，并创建临时目录断言实际所有者匹配当前用户。脚本不更改已有数据目录的所有者、系统策略或产品校验；API 调用或断言失败即停止检查。新对象所有者语义见 [Microsoft 文档](https://learn.microsoft.com/en-us/windows/win32/secauthz/owner-of-a-new-object)。
