# establish-node-access independent tester regression

状态：独立 tester 实际执行记录。本报告不继承 coder 自评，只采用本会话对固定源码运行命令、测试程序、真实 Windows 节点和不同真实 SID 探针所得结果。报告在 2026-10-29 21:12（Asia/Shanghai）按 `openspec/role/tester.md` 和 `openspec/changes/establish-node-access/tests.md` 重新校准用例语义；此前把“当前环境可执行的主要断言”记作整例 `PASS` 的口径不再使用。2026-10-29 21:35 追加独立 L6、close code、完整浏览器退出持久性和资源释放证据。

本版结论：已执行工程检查、118 个库测试、4 个 JWS 目标、5 个关闭/限流定向回归、真实 owner CLI、响应丢失故障、跨 SID 拒绝，以及真实受信 TLS/Tailscale/Chromium A/B/完整浏览器进程退出持久性闭环。`NA-032` 的全部原 Steps/Expected 已独立执行并满足，整例记为 `PASS`；这不代表其他 PARTIAL 或人工验收也通过。人工 fixture 由主流程准备、用户确认执行人，但尚未签署；本 tester 不占用专用端口 `30447` 或数据目录 `.tmp/establish-node-access-human-20261029`。

## 固定版本与清单

- 工作目录：`D:\Project\acp-remote`
- HEAD：`7a6e9ec4bdf4b41d9408b39cb3669313e52f41b2`
- 源码清单：`testing/runs/establish-node-access/2026-10-29-snapshot.sha256`
  - 清单自身 SHA-256：`6B1D981658CE1ACA98D390B3C56650908924B0A7486A55A87836DEEC74200BB9`
  - 条目：71；本轮复核 `71/71` 匹配，0 缺失、0 MISMATCH。
- 配置清单：`testing/runs/establish-node-access/2026-10-29-config-snapshot.sha256`
  - 清单自身 SHA-256：`13AD9DDE14AFC5235217CBA748B8EAB830162F56625FF4183BAB386172DFB3F1`
  - 条目：22；本轮复核 `22/22` 匹配，0 缺失、0 MISMATCH。
- 在完整 Rust/Web 检查和真实节点探针之后再次复核两个清单，结果仍分别为 `71/71`、`22/22`。
- Rust：`rustc 1.98.1`、`cargo 1.98.1`；Node.js `v22.20.0`、npm `10.9.3`。
- 独立 Cargo target：
  `.tmp/establish-node-access-tailscale-20261029/build-target`。
- 被测节点二进制：
  `.tmp/establish-node-access-tailscale-20261029/build-target/debug/acp-remote.exe`
  - SHA-256：`90150CAFE9AE40EA6DCE77F436BA8A2241BBFE92C84E108C815452CE70A0D2B1`。
- 独立证据目录：
  `.tmp/establish-node-access-independent-tester-regression-20261029-r2/`。
- 独立 L6 配置、数据和证据目录：
  `.tmp/establish-node-access-independent-l6-20261029/`。

## 工程与自动化检查

全部 Cargo 命令均在 `app/` 下使用独立
`CARGO_TARGET_DIR=D:\Project\acp-remote\.tmp\establish-node-access-tailscale-20261029\build-target`
并加 `--offline`、`--locked`。完整库测试及原生 Windows 行为测试以显式非沙箱的真实 `faraday\jack` 用户运行。

| 检查 | 退出码 | 实际结果 |
| --- | ---: | --- |
| `cargo fmt --all --check` | 0 | PASS；无格式差异 |
| `cargo check --all-targets --locked --offline` | 0 | PASS |
| `cargo clippy --all-targets --locked --offline -- -D warnings` | 0 | PASS；无 warning |
| `cargo test --lib --locked --offline -- --test-threads=1` | 0 | PASS；`118 passed; 0 failed; 0 ignored` |
| `cargo test --all-targets --locked --offline -- --test-threads=1` | 0 | PASS；库 `118/118`，JWS `4/4`，其他目标 0 项且无失败 |
| `cargo build --locked --offline` | 0 | PASS；生成上述节点二进制 |
| `cargo build --examples --locked --offline` | 0 | PASS；生成测试证书工具 |
| `npm.cmd --prefix clients/web run check` | 0 | PASS；`tsc --noEmit` |
| `npm.cmd --prefix clients/web run test` | 0 | PASS；6 个测试文件、31 项测试 |
| `npm.cmd --prefix clients/web run build` | 0 | PASS；Vite 生产构建完成 |
| `openspec.cmd validate establish-node-access --strict` | 0 | PASS；`Change 'establish-node-access' is valid` |
| 根目录 `npm audit --audit-level=high` | 0 | PASS；125 dependencies，0 vulnerabilities |
| `clients/web` 的 `npm audit --audit-level=high` | 0 | PASS；81 dependencies，0 vulnerabilities |
| `git diff --check` | 0 | PASS；仅输出工作区 LF→CRLF 提示，无 whitespace error |

说明：普通沙箱内 npm audit 因 registry 网络访问失败；随后以真实 `jack` 用户、显式非沙箱网络权限重跑，以上 0 漏洞结果来自该次真实网络请求。`npm.cmd` 可执行，`npm.ps1` 被执行策略阻止，因此未用 PowerShell shim。

定向回归命令均按完整测试名 `--exact` 运行，每项恰好运行 1 个测试并通过：

| 测试 | 退出码 | 结果 |
| --- | ---: | --- |
| `node::tests::run_observes_a_stop_requested_before_subscription` | 0 | PASS |
| `node::tests::wait_for_stop_observes_a_failure_before_subscription` | 0 | PASS |
| `ipc::tests::stop_response_write_failure_is_reported` | 0 | PASS |
| `ipc::tests::stop_response_failure_does_not_cancel_other_clients` | 0 | PASS |
| `node::tests::management_websocket_closes_oversized_transport_frames_with_4429` | 0 | PASS |

## 真实 Windows 节点与故障探针

### 隔离运行

- 数据目录：`.tmp/establish-node-access-independent-tester-regression-20261029-r2/node2/data`
- 监听：`127.0.0.1:30445`；未使用人工验收保留端口 `30447`。
- canonical origin：`https://localhost:30445`
- TLS：本会话新生成的 `localhost` 自签名证书，只用于本地协议/CLI/DACL 探针，不作为可信 TLS 证据。
- Node ID：
  `ca030447209cd27d5eed685c9c68445ec03c88623212562d37de4360b6710d2d`
- 正常停止及失败投递后均确认进程退出、TCP 监听消失、named pipe 消失。
- 同一证书和数据目录重启成功，Node ID 不变，证明锁、身份和监听资源可以复用。

### Owner CLI、关闭和响应丢失

- `daemon status` 退出 0，返回 `state=ready`、正确 PID/Node ID/origin。
- `device list` 退出 0，空列表。
- `device revoke --device-id AAAAAAAAAAAAAAAAAAAAAA` 退出 1，错误 `local IPC failed: not_found`。
- 正常 `daemon stop` 退出 0，输出 `state=stopped`；stdout 随后出现 `ACP Remote node stopped`。
- 使用与产品 Tokio 客户端一致的
  `SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION`
  向 owner pipe 写入合法 stop 请求后、在响应前断开：
  - 请求真实写入并触发停止；
  - 进程约 1 秒内退出；
  - 没有正常 stopped-success 新行；
  - stderr 为
    `error: node shutdown failed: delivering the owner stop response failed: 管道正在被关闭。 (os error 232)`。
- 普通 `NamedPipeClientStream` 未使用 identification QoS 时，服务端按设计关闭模拟连接，写入前得到 `ERROR_BROKEN_PIPE`；该结果未计作产品失败。

### 跨真实 SID 拒绝

- 节点所有者：`faraday\jack`，
  `S-1-5-21-1850245178-3063344048-767428509-1002`
- 另一个现成真实用户：`faraday\codexsandboxoffline`，
  `S-1-5-21-1850245178-3063344048-767428509-1004`

这不是把 sandbox 文件权限冒充产品拒绝。以下均由另一真实 SID 访问 `jack` 创建的隔离节点：

| 探针 | 结果 | 原生证据 |
| --- | --- | --- |
| 打开 `.node.lock` | 拒绝 | `UnauthorizedAccessException`，HRESULT `0x80070005` |
| 打开 `identity.key` | 拒绝 | `UnauthorizedAccessException`，HRESULT `0x80070005` |
| 打开 `node.sqlite3` | 拒绝 | `UnauthorizedAccessException`，HRESULT `0x80070005` |
| `daemon status` | 退出 1 | 数据目录 canonicalize 拒绝，`os error 5` |
| `device list` | 退出 1 | 同一原生数据目录拒绝，`os error 5` |
| `device revoke` | 退出 1 | 同一原生数据目录拒绝，`os error 5` |
| owner-only pipe `status` | 连接拒绝 | HRESULT `0x80070005` |
| owner-only pipe `device.list` | 连接拒绝 | HRESULT `0x80070005` |
| owner-only pipe `device.revoke` | 连接拒绝 | HRESULT `0x80070005` |
| 对可读密文副本调用 CurrentUser DPAPI | 拒绝 | `CryptographicException`，HRESULT `0x80070002`，未解出明文 |

`icacls` 显示隔离 data 目录 DACL 只有 `FARADAY\jack:(OI)(CI)(F)`。DPAPI 探针只复制 262 字节密文到 tester-owned 配置根，并只授予另一 SID 读取该副本；没有放宽真实 `identity.key` 或节点 data 权限。

## 独立 L6 受信 TLS、Tailscale 与 Chromium 闭环

### 运行材料与可信路径

- 节点以本版本新编译二进制单独启动，配置为
  `.tmp/establish-node-access-independent-l6-20261029/acp-remote.toml`。
- 监听 `100.119.123.57:30448`，canonical origin
  `https://faraday.tail74842e.ts.net:30448`；数据目录
  `.tmp/establish-node-access-independent-l6-20261029/data-jack`。
- Node ID：
  `1f9348b8c5104609735b6d03351895f42b68cc637d4ef80c20447667ed3fafb5`。
- Tailscale CLI `1.104.1` 报告 `BackendState=Running`、本机 Tailscale IP
  `100.119.123.57`、DNSName `faraday.tail74842e.ts.net.`，且具有 `https` capability。
- 非沙箱 DNS 解析 `faraday.tail74842e.ts.net` 为
  `100.119.123.57`。系统 hosts 有同名映射，但目标仍是本机真实 Tailscale IP，
  不是 `127.0.0.1`/localhost；节点也只监听 Tailscale 接口。
- TLS 证书 `Subject=CN=faraday.tail74842e.ts.net`，
  `Issuer=CN=YE1, O=Let's Encrypt, C=US`，有效期
  `2026-10-18 13:10:37` 至 `2027-01-16 13:10:36`，
  SAN 仅 `DNS:faraday.tail74842e.ts.net`，EKU 为服务器身份验证。
- 证书 SHA-256：
  `12AD6ABE82897DC861384D9881F38847792E48A964AB39F0B72E4EE3350F8296`。
- 两份浏览器实际页面均报告 `isSecureContext=true`，Edge 启动参数中不存在
  `--ignore-certificate-errors` 或测试证书绕过开关。
- 直接 `curl.exe` 因 Windows 在线吊销检查离线返回
  `CRYPT_E_REVOCATION_OFFLINE`；这不是主机名或签发链不匹配。真实 Chromium
  的受信加载、`isSecureContext`、TLS WebSocket 和证书链验证均成功，未用
  `-k` 或忽略证书参数。

### 独立浏览器身份与 fallback

最初 ego TaskSpace `3`、`4` 用于 A/B 探测，但实测两个 TaskSpace 共享同一
browser profile，A 写入的 localStorage 标记能在 B 读到。因此未把两个 ego
页面冒充两个身份，也没有在超时后新建 TaskSpace；改用两个独立 Edge profile
完成 L6：

- A：`.tmp/establish-node-access-independent-l6-20261029/profiles/A`，
  CDP `127.0.0.1:9228`。
- B：`.tmp/establish-node-access-independent-l6-20261029/profiles/B`，
  CDP `127.0.0.1:9229`。

这是一次基于实测隔离失败的明确 fallback。后续从两套 IndexedDB 直接读取到不同身份：

| 浏览器 | 名称 | clientId | 公钥指纹 | 私钥可导出 |
| --- | --- | --- | --- | --- |
| A | `Independent Chromium A` | `rDUhhTY_GJ4ryk9e7uR-IQ` | `7de334515fe3e6ac7dcb9f7534d2db8e771fa3c35cdae2ffc23df4e50f5af815` | `false` |
| B | `Independent Chromium B` | `n5BBVXjiROzeXaX249l8Sg` | `4d3fff1e055b07fbb494ba3f62de8a9ee277b283e5d51277041b74f9c6f35d28` | `false` |

两个身份来自不同 profile 的 CryptoKey 和 IndexedDB，不是同一浏览器中的两个页面。
最终 storage 检查中 localStorage/sessionStorage 均为空，IndexedDB 仅有
`acp-remote-access-v1`；私钥不在 localStorage。

### 真实配对、管理、撤销和重启

- A、B 分别执行真实的 `acp-remote device pair` 交互式 TTY 会话。Web 核对节点
  SHA-256 指纹并输入配对码后，owner 终端显示客户端名称/公钥指纹/到期时间；
  只有在终端显式输入 `y` 后才完成批准。
- A、B 均通过受信 HTTPS 建立认证 WSS，Web 读取真实 SQLite 中的设备列表。
  B 加入后 A 的现有订阅显示设备列表已更新，A 观察到 B。
- A 通过真实 Web UI 二次确认撤销 B；B 页面进入 `MANAGEMENT / REVOKED`
  “管理权限已被撤销”，owner CLI 的 `device list` 显示 A `Trusted`、B `Revoked`。
- owner 第一次 `daemon stop` 退出 0，原节点 PID `26488` 消失。使用同一证书、
  DPAPI 身份和 SQLite 数据目录重启后，Node ID 仍为上述同一值。
- 重启后 A 仍以原 clientId 认证并读取列表；B 重新连接被拒绝。精确 WebSocket
  close 记录为 `4401 / authentication failed / clean=true`。
- 为补齐正常停止 close code，再次用同节点启动 A/B 并预置仅记录 WebSocket
  close 事件的页面钩子；A 为 authenticated，B 为 revoked。owner 第二次
  `daemon stop` 退出 0，A 收到 `1001 / node stopping / clean=true`，页面显示
  “节点正在停止，管理连接已关闭”；B 仍为 `4401`。
- 回归测试另有 transport 超限 `4429 / limit exceeded`；完整测试集还断言
  协议错误 `4400 / invalid frame` 与认证超时 `4408 / authentication timeout`。
  这些单测断言不与上面的真实浏览器 close 证据混写。

### 完整浏览器退出后的持久性

- 先终止 A profile 的全部 14 个 Edge 进程，复核匹配该 profile 的进程数为 0；
  再用同一 profile 重新启动并打开同一 HTTPS origin。
- A 仍从 IndexedDB 读取 `status=consumed`、原 clientId、原 nodeId、原显示名和
  `privateKey.extractable=false`，再次完成公钥持有证明并读取设备列表。
  这完成了“全部浏览器进程退出后仍以原 key/ID 认证”的持久性断言，不再是即时
  读写或仅刷新页面证据。
- 未执行私密模式、持久化写入失败、已有坏 key 不可读、清站点数据、key 丢失
  提示和新 origin 隔离；因此 NA-031 仍只能记 `PARTIAL`，不能因上述持久性子项
  通过而提升整例。

### 证据、权限与清理

- 最终页面截图：
  `evidence/a-final.png`、`evidence/b-final-revoked.png`；
  identity/body 原始值：`evidence/l6-final-state.json`；
  close code 原始值：`evidence/close-code-read.json`。
- tester 自有 TLS key 副本最初继承了宽 ACL，无法由非 owner 原地改 DACL。随后
  只删除并从用户已保护的源 key 重新创建该 `.tmp` 副本，内容 SHA-256 保持
  `514E9C18FBDBFC745377B1472880168D27A6362376800A8235B50A096887C1A7`，
  DACL 收紧为仅 `FARADAY\jack:(F)`。另一真实 SID
  `faraday\codexsandboxoffline` 读取该副本得到
  `UnauthorizedAccessException` / HRESULT `0x80070005`。没有修改用户实际 key。
- 两轮 owner stop 均退出 0；全部 tester 拥有的节点与 Edge 根/子进程已停止。
  端口 `30448`、`9228`、`9229` 无 LISTENING 记录（仅系统 TIME_WAIT），
  `\\.\pipe\` 无 `acp-remote` 残留；同一 owner SID 能以
  `FileShare.None` 重新打开 `data-jack/.node.lock`。
- ego TaskSpace `3`、`4` 已确认 ownership 为 `agent`，均执行
  `finish({ keep: [] })` 关闭；未清理用户正常空间或 profile。

## 用例判定口径

- `PASS`：`tests.md` 中该 ID 的 Steps 和 Expected 全部必要断言均在本固定版本上实际执行并满足。
- `PARTIAL`：已有真实自动化、协议、CLI 或故障证据，但至少一个原 Step/Expected 未执行，不能用主要子集代替整例。
- `NOT-RUN`：没有可执行入口、需要尚未完成的人工签署，或本轮未开始该场景；不表示代码失败，也不表示通过。

## 逐用例校准

| ID | 状态 | 已执行事实与精确剩余缺口 |
| --- | --- | --- |
| NA-001 | PASS | 已读取工程入口；Rust fmt/check/clippy/test/build、Web check/test/build、`openspec.cmd --strict` 和 `git diff --check` 均实际执行并成功。 |
| NA-002 | PARTIAL | OpenSpec、diff、Rust/Web 静态检查、根与 Web npm audit 已通过；未执行 GitHub CI `secrets` 作业和 `npm run lint:commits -- --from BASE --to HEAD --verbose`，也未完成独立全仓秘密扫描。 |
| NA-003 | PARTIAL | Rust JWS 4/4 和 Web `jws.test.ts` 固定向量、high-S、DER、SEC1、Unicode、重复键、深度等测试通过；未在独立真实 Chromium WebCrypto 会话中完成 Rust↔浏览器双向完整变异矩阵。 |
| NA-004 | PARTIAL | challenge 绑定、nodeId/origin/requestId/challengeId/nonce/version/用途/连接变异、30 秒过期、单次消费、墙钟回拨和 claim 随机源失败测试通过；未对 auth/status 等所有签发入口分别注入随机源失败，也未做真实 WSS 边界矩阵。 |
| NA-005 | PARTIAL | 300 秒寿命、第九个活动配对拒绝、格式/可选中间连字符、reject/cancel 和临时材料重启清理测试通过；未独立执行码碰撞强制重生成、全部合法终态转换及真实 CLI 非交互模式。 |
| NA-006 | PARTIAL | IPC 帧/JSON/HTTP 预算、深度 8、部分 16/32 KiB、握手/命令/出站队列及 rate-limit 测试通过；未逐项执行合同 §5 全部端点、握手、连接、命令、出站、IPC 的“小于/等于/大于”边界，未验证 1024 来源桶耗尽、空闲 5 分钟、5 分钟驱逐、畸形/成功/失败统一计数、伪造转发头，以及 300 秒 `device pair` 流式会话。 |
| NA-007 | PARTIAL | 批准持久化、claim 幂等/冲突和后端取消不回滚信任测试通过；独立存储报告已在真实延迟约束下覆盖提交前失败与实际 COMMIT 失败，证明无半信任/审计残留。仍未完成批准提交成功后响应丢失、CLI 同时取消的确定性窗口，以及真实浏览器以同 profile 公钥恢复 clientId 的整链。 |
| NA-008 | PARTIAL | 撤销原子/幂等、认证解析期间撤销、queued result 丢弃测试通过；未在受理、提交、缓存命中、入队、socket 提交各栅栏注入并同时操作多个真实 WebSocket，也未独立验证撤销事务所有阶段失败。 |
| NA-009 | PARTIAL | 身份崩溃恢复、key-only 补 meta、key/meta 不符、损坏、未知 schema、DPAPI 新进程恢复均通过；未在各命名崩溃阶段之外独立覆盖密文临时写入中途和 flush 中途断电/终止。 |
| NA-010 | NOT-RUN | 后续 Harness 审批能力及其测试适配器不存在，无可执行入口。 |
| NA-011 | PARTIAL | 无效 TLS、占用端口、不可用 data 目录测试及真实正常启停通过；未逐项执行每类失败后的配置修正重试，也未核验所有失败输出均不泄露私密路径/材料。 |
| NA-012 | PARTIAL | 同目录第二实例拒绝、真实 owner CLI、不同 SID 对 data/pipe 的原生拒绝已通过；未执行远程命名管道、伪造 SID/模拟服务端 token 校验失败、另一用户实际 `device pair`/`daemon stop` 路径。候选 SID 是在客户端 data 路径/DACL 阶段被拒，不是已观察到的服务端 SID 错误响应。 |
| NA-013 | PARTIAL | localhost 无认证拒绝/有效身份测试通过；真实 Tailscale 路径已由有效 A 和已撤销 B 完成认证/拒绝及 close code 观察。未在同一 Tailscale 路径对无身份请求逐项发送 `node.info`、`device.list` 和订阅，故仍缺该矩阵。 |
| NA-014 | PARTIAL | 无效 proof、已知/未知/撤销 ID、重放和闭合错误测试通过；真实 B 的 4401 页面只显示允许的节点/客户端元数据。未收集完整浏览器 network/HTTP 历史，也未对普通日志、审计和静态存储做全部标记秘密扫描。 |
| NA-015 | PARTIAL | owner stop、关闭报告顺序、进程退出、资源复用测试通过；真实 L6 又在 A 有管理订阅时执行 owner stop，A 收到 `1001/node stopping`，监听/pipe/lock 均释放。未在同一轮加入“未完成配对 + 多认证订阅 + 在途操作”组合，不能提升整例。 |
| NA-016 | PARTIAL | 250 ms 注入的 storage close deadline、关闭报告不等 IPC、真实 stop response 写失败 `os error 232` 均通过；未实际耗尽合同 15 秒期限，也未覆盖必要保存提交失败的多项未完成报告。 |
| NA-017 | PASS | Node ID/信任持久化、key/meta 缺失和不符/损坏/未知 schema/崩溃恢复测试通过，跨 SID DPAPI 新进程解密拒绝；真实 A/B 配对、撤销 B、同数据目录重启后 Node ID 不变、A 以原 clientId 重认证、B 以 4401 拒绝，原 Steps/Expected 全部满足。 |
| NA-018 | PARTIAL | 真实 `device pair` TTY、HTTPS Web 指纹核对和有效码输入均已执行；终端显示客户端名称/公钥指纹/到期，页面只提供本 change 的 Web 配对入口且无移动扫码要求，后端 pending-confirmation 测试也通过。唯一未执行的原断言是“进入待确认但尚未本机批准时，从 Web 逐项尝试管理并确认被拒”，因此整例保持 PARTIAL。 |
| NA-019 | PASS | 按原用例主体与后端边界，不附加未测的新要求：真实 `device pair` TTY 在显示客户端名称/公钥指纹/到期后由 owner 显式输入 `y`；真实 Chromium 验证 hostProof、保存稳定 clientId、建立新 WSS、完成公钥持有证明并读取列表；可靠提交后才报告成功由同版本独立存储提交失败证据支持；A 撤销 B 作为管理操作归属 A 的已执行证据。 |
| NA-020 | NOT-RUN | 人机交互 fixture 由主流程准备、用户确认执行人；默认回车、Ctrl-C、关闭 CLI/IPC 断开及网络批准拒绝场景尚未由本 tester 执行。 |
| NA-021 | PARTIAL | claim 原始重试同结果、内容冲突拒绝、撤销公钥不能新建身份等自动化通过；未让两个真实浏览器 profile 用同一码并发认领，也未完成“同 requestId 不同 proof/其他 key 新挑战”全部线上矩阵。 |
| NA-022 | PARTIAL | 旧码/挑战过期、重签发、重启清理、proof 重放和状态错误测试通过；未在真实浏览器执行批准后响应丢失、60 秒/max 到期保留和过期后重取。 |
| NA-023 | PARTIAL | lookup/status/handshake 限额、真实 TCP 来源而非临时端口、连接/命令/出站队列边界和 32 KiB+ WS 帧测试通过；未逐端点超过真实 HTTP/WSS 阈值，未验证慢连接 1 MiB 或 256 帧先到者关闭及全部资源释放。 |
| NA-024 | PARTIAL | Host/Origin/协议、重复头、未知路径和 Web 管理端节点替换拒绝测试通过；未用可信测试 TLS 更换节点 key 后由真实浏览器完成拒绝/不覆盖绑定，也未审计第三方资源加载。 |
| NA-025 | PARTIAL | 错签名、重放、认证前管理帧、5 秒无 hello、8/9 连接上限、有效身份归属及挑战一次消费测试通过；真实 Chromium A 有效认证、B 撤销后 4401。未在真实浏览器身份上独立完成 15 秒不发 proof 的 4408 时限，故仍缺该 Step 子项。 |
| NA-026 | PARTIAL | 正常批准持久化、重启读取、撤销回滚和配对凭据跨用途拒绝测试通过；未注入批准事务提交前/中/后各类存储失败后执行真实浏览器刷新、节点重启和管理复核。 |
| NA-027 | PARTIAL | revoke 原子/幂等/持久、requestId 冲突和游标测试通过；真实 A→B Web 撤销后 B 重连 4401。独立存储报告另行覆盖 10000 条满额/不驱逐、过期槽、延迟提交失败和事务回滚。仍缺真实或可控时钟跨 24 小时、实际磁盘满，以及 WSS list/info 与逐帧审计不膨胀的完整组合。 |
| NA-028 | PARTIAL | 认证解析期间撤销和 queued result 丢弃测试通过；真实 B 单连接在撤销后进入 revoked，重启后管理认证继续拒绝。未建立多个真实 B 连接/订阅后并发命令与推送，也未执行真实自撤销和无成功响应后的完整 list/重连矩阵。 |
| NA-029 | PARTIAL | 32 条分页/游标、revision stale、devices.changed、owner list 不输出密钥/证明测试通过；真实 A 的设备列表在 B 加入/撤销后更新且只显示允许字段。未用超过 64 条真实浏览器记录逐页读取，未检查 60 秒前后续读、全部成功/失败/撤销审计、公开页 CSP/no-store/no-referrer 和 code 不进入全部 storage/第三方请求。 |
| NA-030 | NOT-RUN | 后续 Harness 审批隔离入口不存在，无法以已认证身份请求无审批/失效审批能力。 |
| NA-031 | PARTIAL | Web 单测覆盖不可导出 key、同 origin 持久化和 `clientId=null` 恢复；真实 Chromium A 在全部 14 个 profile 进程退出后同 profile 重开，仍以原不可导出 key/clientId 认证，且 localStorage/sessionStorage 无私钥。未执行私密模式、写入失败、已有坏记录 key、清站点数据、key 丢失提示和新 origin 隔离，不能提升整例。 |
| NA-032 | PASS | 真实 Windows x64 owner 启动直接 TLS 节点，使用匹配名称的 Let's Encrypt 受信证书和真实 Tailscale IP；两个独立 Chromium profile 完成真实 `device pair`/显式批准/认证/list/订阅更新/撤销 B/同数据目录重启/A 重认证/B 拒绝/owner stop。真实 SQLite、DPAPI、named pipe、WebCrypto、IndexedDB、TLS、Tailscale 均无 mock；A 收到 1001 关闭，所有进程/监听/pipe 释放。 |
| NA-033 | NOT-RUN | 人工 fixture 由主流程准备，用户确认执行人，但截至本报告尚未签署；不得代签安全、密码库、依赖、DACL/DPAPI 和脱敏证据。 |
| NA-034 | NOT-RUN | 同上，真实终端/Web 提示、默认拒绝、取消、超时和密钥丢失/节点替换可理解性尚无用户签署。 |

统计：`PASS` 4，`PARTIAL` 25，`NOT-RUN` 5，共 34。`PASS` 为 NA-001、NA-017、NA-019、NA-032；该统计只表达整例完成度，不代表 25 个 PARTIAL 已有失败。

## 重点用例补充

### NA-006

当前自动化覆盖的是若干预算单元和集成边界，不能替代合同 §5 完整矩阵。剩余必须执行：

- endpoint、handshake、connection、command、outbound、IPC 六类预算各测试上限前、等于、超过值；
- 固定 60 秒窗口、1024 个真实来源桶耗尽、空闲 5 分钟和新来源不驱逐旧桶；
- 畸形、成功、失败请求都计数，伪造转发头不改变真实 TCP 来源；
- 16 KiB/32 KiB、深度 8、压缩、未知路径/版本；
- `device pair` 流式会话不被普通 30 秒请求期限截断且最长 300 秒；
- 429 与 `retryAfterSeconds` 一致，错误闭合且不泄密。

因此 NA-006 只能是 `PARTIAL`。

### NA-012

真实不同 SID 的 DACL、CLI 数据目录和 owner-only pipe 拒绝很强，但原用例还要求：

- 远程 named pipe 访问拒绝；
- 伪造 SID/服务端 token 校验模拟失败；
- 另一用户实际执行 `device pair`、`device list`、`device revoke`、`daemon stop` 的完整路径；
- 所有者无 Web 身份时 list/revoke 恢复；
- 同目录第二实例及未知 PID 抢锁行为。

已执行部分不等于整例通过，故为 `PARTIAL`。

### NA-016

已有 250 ms 注入的关闭期限测试和真实 stop response 写失败，但原用例要求真实 15 秒期限耗尽或必要保存失败时逐项报告未完成且不宣称全部释放。未执行该 15 秒矩阵，故为 `PARTIAL`；不能因 `NA-QA-02` 的 IPC 结果顺序已测就扩写为 NA-016 全通过。

### NA-021

已测 claim 重试和冲突，但没有两个真实浏览器 profile 并发争用同一码，也没有完整执行获胜 proof 重发、同 requestId 不同 proof、其他 key 新挑战 proof、有效/撤销公钥重复配对。故为 `PARTIAL`。

## 回归问题定向结论

| ID | 本版结论 | 证据 |
| --- | --- | --- |
| NA-REV-001 | PASS（回归修复范围） | 订阅前 stop 和 failure 两个测试各 1 项通过。只关闭该竞态，不代表 NA-015/016 整例已过。 |
| NA-REV-002 | PASS（回归修复范围） | 精确单测通过；真实 pipe 响应写失败返回 `os error 232`，进程退出且不虚报正常成功。 |
| NA-REV-003 | PASS（回归修复范围） | 一个 stop 客户端响应失败不取消其他客户端的精确测试通过；未扩大为所有并发关闭组合。 |
| NA-REV-004 | PASS（回归修复范围） | 32 KiB+ transport frame 映射 4429 的精确测试通过；未替代真实可信 WSS 尺寸矩阵。 |

## 发现与阻断

- 本轮已执行范围内未发现新的确认产品缺陷。
- `NA-QA-01` 的共同基础已补齐：真实 Chromium 的全部 profile 进程退出后，同 profile 仍以原不可导出 key/clientId 认证。但本轮批准是显式成功响应，“批准已提交但响应丢失后重取”仍未执行，不能把持久性通过等同于 QA-01 全闭环。
- `NA-QA-02` 的 IPC 任务结果先于关闭报告、失败投递不虚报成功已有真实证据；但 NA-016 的 15 秒期限矩阵仍为 PARTIAL。
- `NA-QA-03` 的撤销事务提交失败、幂等冲突和真实 B 撤销后 4401 拒绝已分别有独立存储/L6 证据；批准响应丢失、可控 24 小时边界、磁盘满和全部并发重放栅栏仍缺，不能关闭。
- NA-010、NA-030 被后续 Harness 入口缺失阻断。
- `NA-032` 已独立完成并整例 PASS；`NA-031` 的跨完整浏览器退出持久性子项也已通过，但仍被写失败、私密模式、坏记录、清站点数据和新 origin 场景阻断，整例保持 PARTIAL。
- NA-033、NA-034 被用户人工签署尚未完成阻断；人工 fixture 端口 `30447` 和 `.tmp/establish-node-access-human-20261029` 由主流程准备、保留给用户执行，本 tester 未占用。

## 清理状态

- 本轮拥有的测试节点均已通过 owner CLI 退出，两轮 L6 的 `daemon stop` 均退出 0；`acp-remote` 节点进程无残留。
- TCP `30445`、`30448` 及 CDP `9228`、`9229` 均无 LISTENING 记录；查询中仅存在系统正常 TIME_WAIT。
- `\\.\pipe\` 中无 `acp-remote` 测试 pipe；`data-jack/.node.lock` 已由真实 owner SID 以 `FileShare.None` 成功重新打开，证明节点锁已释放。
- 两轮 L6 的 Edge A/B 完整进程树均按已验证的 tester profile 根 PID 终止；最终无匹配
  `.tmp/establish-node-access-independent-l6-20261029/profiles/A|B` 的 Edge 进程。
- ego TaskSpace `3`/`4` 均确认属于本轮 agent，并已 `finish({ keep: [] })`；正常用户空间/profile 未清理。
- 未操作或删除用户真实数据库、身份、密钥、浏览器 profile、人工 fixture；未占用 `30447` 或其 data 目录。
- tester-owned `.tmp/` 证据、证书和独立 target 保留用于复现。tester L6 TLS key 副本保持原内容 SHA-256，DACL 已收紧为真实 jack owner-only。
- 报告写入后重新逐项校验源码清单 `71/71`、配置清单 `22/22`，均为 0 缺失、0 MISMATCH。

当前不能进入“待归档”阶段。`NA-032` 已完成，但 NA-033/034 人工签署尚未完成，NA-031 及其他 24 个 PARTIAL 仍有上述原用例断言缺口。必需 PARTIAL 不能由 reviewer 豁免；须按矩阵补齐，或由用户明确调整对应范围/验收规则。
