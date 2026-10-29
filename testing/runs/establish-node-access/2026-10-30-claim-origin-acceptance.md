# NA-021 / NA-024 独立运行验收与 NA-014 版本影响分析

日期：2026-10-30，Asia/Shanghai，约 02:19–02:33。独立侧任务，按当前 change tests.md 的 NA-021、NA-024 执行。仅新增本报告及 `testing/probes/establish-node-access-claim-origin-20261030/`，不修改产品、规格、既有报告或公开 action，不提交、推送或归档。

## 结论

| 范围 | 实际结果 | 结论 |
| --- | --- | --- |
| NA-021 同码竞争 | 同 origin 的 Ego 与独立 Edge 普通持久 profile，两个生产页面 claim 实际发送相差 9ms，仅一方成功，终端核对并批准该 key | 通过 |
| NA-021 有限重试与冲突 | 原 proof 连续 3 次返回相同 hostProof；相同 requestId 变更 proof 返回 409 request_conflict；另一 key 重放/申请新挑战均被拒绝 | 通过；新挑战在签 proof 前即被拒绝，见具体边界 |
| NA-021 有效/撤销 key 重配 | 同一个获胜 CryptoKey 在有效和撤销状态下，分别对新码取得新鲜 challenge 并签 claim，均 401；旧身份没有新建或复活 | 通过 |
| NA-024 真实页面节点替换 | 受信 TLS 响应夹具提供替代 key、错 nodeId、错 origin、坏 hostProof；每种实际认证签名 0 / proof 0，既有 pin/ID/key 不变；未修改响应正控制成功 | 通过（既有绑定、管理挑战场景） |
| NA-024 Origin/Host | 4 个配对 POST + WSS，8 种来源组合共 40 项真实 TLS 请求，异常均拒绝，转发头不构成来源权威 | 通过 |
| NA-014 | 仅完成 r6→R9 影响分析并限定复用，不重复执行 r6 的 8 项认证/56 条网络记录矩阵 | 保留 r6 独立 PASS，不能写成 R9 重跑 |

未发现产品失败分支，不新增产品问题 ID。不把本报告当成整体归档通过或独立 reviewer 代签。下面明确真实组件、受控夹具和未执行范围。

## 固定版本、profile 与隔离

- R9 清单 `2026-10-30-implementation-r9.sha256`：83 项，清单文件 SHA-256 `7876c7803106193c476ee0c23dac27aa25036154cdf293d6a7eb7ecd2fea6db7`。
- 从 `.tmp/acceptance-20261030-r5-target/debug/acp-remote.exe` 复制到本次 owner-only `.tmp/claim-origin-acceptance-20261030/bin/acp-remote.exe`；运行前后 SHA-256 均为 `ab97418e98d67a71e25bbf62b1e780f2f767530ca4290fb9c78e220a222a6024`。没有重新编译、替换或修改运行中的二进制；并行源码变化不扩张本次固定产物范围。
- 自有原始节点 `100.119.123.57:30455`，真实 SQLite、DPAPI、owner named pipe。NA-024 阶段同一数据目录改在 30456 监听，canonicalOrigin 仍为 `https://faraday.tail74842e.ts.net:30455`，30455 由受控 TLS 夹具承接。不操作用户 30447 或其他 tester 30451–30454。
- ego-browser skill 已读取并实际运行。新 TaskSpace **12**、p1；没有接管旧 finish Space。第二浏览器是专属 Edge 实例，`--user-data-dir=<owned runtime>/edge-profile`、`--headless=new`、`--remote-debugging-port=0`，没有 `--inprivate`、共享用户 profile 或忽略证书选项。它是普通持久化 Default profile，非 incognito context。
- 两浏览器都在**同一个** HTTPS origin :30455，使用真实生产嵌入页面、IndexedDB、WebCrypto 不可导出 P-256 key。Ego 在该 origin 写入 A 哨兵，Edge 读到 null 后写 B 哨兵，Ego 后续仍为 A；同时两公钥摘要不同，证明不是同 profile 两标签或靠不同 origin 假装隔离。
- 所有自建运行材料、Edge profile、JSON、日志和测试身份均在上述 owned runtime。Ego 底层 profile 由工具管理，不声称其物理路径也在 .tmp；只清本次自有 origin 的测试存储，不操作普通用户 profile 设置。
- 受信同域证书/私钥通过原路径只读借用，不复制私钥。浏览器 secureContext=true；全部原生来源表请求启用 `rejectUnauthorized:true`，tlsAuthorized=true，未禁用链或主机名校验。

## NA-021：真实同码并发及一次绑定

### 并发方式与结果

两个生产表单输入同一个真实 owner CLI 配对码。只读/延迟包装器让各自生产 PairingSession 完成 lookup、使用实际 IDB key 签名并到达 claim 发送前屏障；包装器不改 lookup、proof、响应或存储。两侧均 ready 后，用同一绝对时刻释放。真实发送记录：

| Profile | 公钥编码字符串 SHA-256 | 发送时间（Unix ms） | claim |
| --- | --- | --- | --- |
| A / Ego | 89b6416e02eb4be6b0ab925f3a286d1a70ab3fe87e96762bdde1d3d48f9879af | 1793298065071 | 401 pairing_invalid |
| B / Edge | 693c52d396e85bdb32f3b23da34d6d7beffe3ff3e58cc2c99ed4eec501b8acc8 | 1793298065062 | 200，pending_confirmation |

两侧 privateKey.extractable=false。按照合同，每配对只保留一个当前 claim challenge，B 的后续 lookup 使 A 旧挑战失效；本轮确实同时发送两个 claim，但**不声称制造了合同不允许的“两份同时有效旧挑战”**。结果覆盖单当前挑战、并发请求和唯一绑定，非单纯串行提交两表单。

真实隐藏 owner 控制台只显示 B 的待确认名称。批准前读取终端并核对 `NA021 Profile B Edge` 和原始 SEC1 公钥 SHA-256 指纹 `c49810aa9a8308669ba731ea29e089aa56e3ea223b680e2c560115d69267b2e6`，才通过 Windows console y+Enter 明确批准。owner list 当时仅一条 Trusted：`aKiif67Her4FA2kYCoGOKQ`，名称和指纹均为 B，没有 A。

### 有限 proof 重试、冲突与其他 key

- 仍在本次配对有效期内，B 的**原始 compact proof 字节**仅保留在本浏览器内存，不复制到证据文件。经真实 HTTP 重发 3 次，每次 200，返回 hostProof 与首次完全一致，不新建身份。
- 保留同一 requestId/挑战，仅改变 displayName，再用 B 的原不可导出 key 重新签名，返回 HTTP 409、request_conflict。没有把无效签名误称为幂等冲突测试。
- A 重发自己最初真实浏览器生成的另一 key proof，返回 401 pairing_invalid；A 再以自己的 key 申请该已绑定 pairing 的新 challenge，也返回 401，不能取得可替换 B 的新鲜挑战。
- 后一项明确是**在挑战发放边界被拒绝**，因此没有编造“服务端实际发放了其他 key 的新有效挑战、再签 proof”的证据。原不同 key proof 和新挑战请求均不能改绑定；B 的终端信息及最终持久身份不变。

### 有效和撤销 key 都不能重配

先在 B 有效时新开一条 owner 配对，真实 lookup 200，使用同一个 B CryptoKey 对新 pairingId/requestId/challenge 签 proof，claim 401 pairing_invalid。仅取消这条自有、尚未认领的 CLI 会话。

然后 owner CLI 撤销上述唯一 B 身份，list 变为 Revoked；再次创建新配对并取得新的真实 challenge，用相同原 key 签 proof，仍 401。两次 key 摘要均为上述 B 摘要，没有改 IDB 或把 key 换成另一把来偷换测试。owner-revoked.txt 当时仅保留原 ID 的 Revoked 记录，没有第二条身份。

为 NA-024 后续测试，撤销后生产页面按规则生成了新 key，再经新配对和本机批准建立了 **另一个**有效测试身份 `4m-sowb5VONGE9s75csllg`。它的 SEC1 指纹 `623d5a39b25ceaff3c4ef380cf74ba34eb8d23b9a1b2021a6dd01b48a68f50b9` 与原 B 不同。最后只读 SQLite 为一条旧 Revoked + 一条新 Trusted，审计批准 2 次、撤销 1 次；这条额外记录来自明确的新 key/新批准，不是重试创建或撤销复活。

## NA-024：真实 TLS 替代节点响应夹具

### 夹具入口和真实/受控边界

原直接 TLS 节点正常停止后，用**同一固定 R9 binary、同一数据目录、同一 nodeId/信任库、同一 canonicalOrigin** 在 30456 运行。自有 Node HTTPS 夹具监听 30455，只读使用借用证书/私钥，与后端之间同样校验 TLS。HTTP 页面与静态资源逐字转发，WSS handshake 原样转发，仅在选定模式修改后端 challenge JSON。客户端仍是原 Edge 页面和生产协议实现。

夹具替代签名 key 是进程内新生成的 P-256 key，节点 ID 为其公开 SEC1 key 的 SHA-256；replacement-key 模式同步更换 nodeId/nodePublicKey，并使用该临时 key 对匹配的 hostProof payload 签名。因此它是 TLS 来源相同、应用节点身份不同的有效替代 key 情形，而非只破坏一个随机字段。它没有读取或导出真实 DPAPI 节点 key。wrong-node-id/ wrong-origin 各改一个响应字段；bad-host-proof 保留正确 pin/来源、翻转签名字节。

这是**受控 TLS 节点响应夹具**，不是声称真正攻陷节点、第三方网络 MITM 或运行了另一个完整产品节点。没有改产品，也不在浏览器内改写接收内容；替换发生在 TLS 夹具端。测试页只读记录实际 WebCrypto sign 调用和 WSS send，异常响应真实经过 TLS 传输进入原协议校验。

独立从固定后端直接取回 HTML、CSS、JS，与夹具输出全部副本的 SHA-256 比较一致：

| 资源 | SHA-256 |
| --- | --- |
| / | 2f1ba9840af1c360a8cc3a5fbb792e4d8df27d72bfcd20f4ca5c88ac2fa3177c |
| /assets/index-0luCBpbO.css | 59f460f235f69fc261ab0d97c4518ee9fb511717330afa09c4da71276c5b3880 |
| /assets/index-7lXIK_nd.js | a3f3a9699a63fe65832ec6f0adbedb316a77e778622d327b86818117b3a8b577 |

### 原页面实际结果

测试前 IDB 已有新有效身份的 consumed 记录和原节点 pin。每轮同一页面 reload，观察 challenge 后至少 1.2 秒，读取实际 IDB 对照；没有人工重写 pin/状态，浏览器私钥始终不可导出。

| 模式 | hello / challenge | WebCrypto 认证签名 / 出站 proof | authenticated | pin、clientId、客户端 key |
| --- | --- | --- | --- | --- |
| 替代 key + 一致替代 nodeId + 替代 key 签名 | 1 / 1 | 0 / 0 | 0 | 全部不变 |
| 错 nodeId | 1 / 1 | 0 / 0 | 0 | 全部不变 |
| 错 origin | 1 / 1 | 0 / 0 | 0 | 全部不变 |
| 正确 pin/来源，坏 hostProof 签名 | 1 / 1 | 0 / 0 | 0 | 全部不变 |
| 未修改响应正控制 | 1 / 1 | 1 / 1 | 1 | 全部不变 |

异常轮页面实际进入 `MANAGEMENT / STOPPED`，提示节点身份/签名/协议不符，不自动覆盖 pin。夹具独立解析客户端发出的 masked WSS 帧，四轮 clientProof 也均为 0，与页内 sign/send 观测一致。正控制证明不是探针令页面或签名功能失效。

每轮 Performance 资源 origin 集合仅为 `https://faraday.tail74842e.ts.net:30455`，没有第三方页面资源。这里不将浏览器整个进程的后台网络当作网页资源，也不声称验证无限时间内永不重试。

本轮覆盖**已有绑定的管理挑战**；未单独增加“全新未绑定身份遇到错误 lookup 后的首次落盘”场景，也不把那一分支冒充本次执行。当前 NA-024 原步骤的替代 key / 错字段 / 无效 hostProof / 既有绑定不覆盖均有上述真实页证据。

## Origin、Host、转发头表

四个 `/api/access/v1/pair/{lookup,claim,challenge,status}` POST 和 `/api/access/v1/control` WSS，共 5 个入口逐项执行。使用原生 TLS socket 构造真实重复 Origin header，所有请求验证证书，非浏览器无法设置 header 后的推断。

| 来源组合 | 配对 POST（每个入口） | WSS |
| --- | --- | --- |
| 缺失 Origin（原生调用） | 400 invalid_request | 400，不升级 |
| Origin: null | 400 invalid_request | 400 |
| 两个相同 Origin header | 400 invalid_request | 400 |
| 异源 Origin | 400 invalid_request | 400 |
| Origin 正确但 Host 错 | 400 invalid_request | 400 |
| 异源 Origin + 伪造正确 Forwarded/X-Forwarded-* | 400 invalid_request | 400 |
| 缺失 Origin + 伪造正确 Forwarded/X-Forwarded-* | 400 invalid_request | 400 |
| 真实 Origin/Host 正确 + 无权威转发信息 | 401 pairing_invalid（使用故意无效业务材料） | 101，正常升级 |

共 40 项。前 25 项直接连原节点 30455；后 15 项在重启后的后端 30456 直接执行，携带 canonicalOrigin 对应 Host :30455，不经过替换夹具，以隔离来源守卫。分批利用正常节点重启重置预算，未将 lookup 429 当作来源校验通过；本次没有 429。转发头不能修复真实 Origin 缺失/错误，真实正确 Origin 也不因非权威转发字段而被替代。

## r6 NA-014 → R9 的负责范围影响分析

没有改写原 `2026-10-30-runtime-secret-acceptance.md`，其实际二进制始终是 r6 `135bd9…`，8 项认证失败、56 条网络记录、21 候选/50 形式的执行证据不能重新标成 R9。

独立比较两份 83 项清单，变化仅四个 Rust 文件：

| 文件 | 影响判断与复用边界 |
| --- | --- |
| management_ws.rs | R9 普通行为新增/调整 actor gate、已认证结果入队、订阅和通知协调。初始 proof/JWS/无效 actor 的统一拒绝仍为 4401 / authentication failed，原无效 proof/未知/已撤销 ID 的失败类别没有变成回显正文；可限定复用原非并发初始认证失败断言。新的已认证 gate、撤销交错、结果发送不由 r6 NA-014 证据代签。 |
| trust.rs | r7 增补 SQLITE_FULL 测试；R9 普通 actor gate 为已认证资格/撤销协调服务。不新增请求正文/秘密日志字段，不改变既有安全审计列格式；可复用原审计秘密标记与元数据区分，不能借此证明新并发提交/入队安全。 |
| node.rs | 原 R9 reviewer 报告中的新增栅栏及场景验证主要在测试边界；普通启动、静态入口日志的敏感字段没有增加。保留原普通日志检查，不把新产品 E2E 视为已完整执行。 |
| platform/windows.rs | r8 reviewer 明确故障点及检查为 cfg(test)，去除测试注入后普通平台逻辑与先前一致。原当前用户 DPAPI/非导出浏览器 key 的秘密保护证据不因该测试扩展自动失效。 |

claims.rs、pairing.rs、pairing_http.rs、storage.rs、strict_json.rs 及全部列入清单的 Web src 文件在 r6/R9 **摘要相同**，包括 identity/session/api、JWS、management client/protocol/reconnect、main；因此原 HTTP 封闭错误、秘密不进 URL/响应、存储表现的限定复用有文件依据。版本差异及摘要保存在本次 summary.json。

本轮 R9 实际新增证据是 NA-021 的真实批准/撤销/重配拒绝，以及 NA-024 的真实页面签名停止与来源表；**不是 R9 NA-014 全矩阵重跑**。原 reviewer 对 R9 actor gate/订阅关闭采用其已发布的独立报告范围，不由本会话再签关闭。

## 证据与清理

全部 runtime 位于 `.tmp/claim-origin-acceptance-20261030/`，公开/脱敏证据在 evidence/。原始 compact claim/proof 仅留浏览器内存及产品自身合法 pendingClaim 状态，不输出到终端/报告；真实私钥从未导出。保留有限请求结果、时间、公开 key 摘要、来源表、TLS 夹具统计、页面停止文案、IDB pin 对比和资源字节摘要。

| 文件 | SHA-256 |
| --- | --- |
| evidence/summary.json | a688de08306089cea383c4fc6299c4d38036aa5fb54ac298e8fe260d2ac9057f |
| evidence/manifest.json（18 项证据摘要） | 4a484450ebdfe51f135f4b6ea3226d5d6b0d70dc661a5d52caa607be3116df2c |
| evidence/tls-browser.json | f9340cc5a13966c241183bcd182eba32201888aea0a6eb573d2d25b0a70ee10f |

入口包括 setup/console/cleanup.ps1、ego-*、edge-round/edge-cdp、harness.js、retries/repair-key、origin-table、tls-fixture/tls-browser-check、evidence-summary。它们记录本次顺序与已 finish Space 12，复现应建立新自有 runtime/profile/TaskSpace，不直接接管已完成空间。

结束仅关闭本次专属 Edge 实例；Ego 只移除本次 origin 哨兵和测试 IDB，然后 finish Space 12，回执 closedSpace=true、closedManagedLabels=[p1]、preservedUnmanagedCount=0。未操作普通用户浏览器。

自有 TLS 夹具进程已退出，固定后端通过 owner CLI 正常停止，PID 35172 退出；30455/30456 均无 listener；确认本次 named pipe 由存在转为消失，`.node.lock` 可独占打开，owner-only DACL 核验通过。按绝对路径检查后删除本次数据目录（DPAPI/SQLite）、专属 Edge profile、临时配对码与 console 读取文件；借用 TLS 私钥完全未复制、删除或修改。保留固定二进制副本和脱敏 JSON 供复核。

新 probe 的 Node/PowerShell 语法、空白格式和新增文件范围已检查；没有修改产品或混入编译产物，没有提交、推送或归档。
