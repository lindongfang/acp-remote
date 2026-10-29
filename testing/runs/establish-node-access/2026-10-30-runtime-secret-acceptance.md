# NA-014 运行时秘密标记与统一认证错误验收

执行日期：2026-10-30，Asia/Shanghai，约 02:03–02:14。独立验收侧任务，按当前 change tests.md 的 NA-014 原步骤和预期执行；不修改产品、规格或既有报告，不使用 Gitleaks 静态扫描代替本例。新增范围只有本报告和 `testing/probes/establish-node-access-runtime-secret-20261030/`。

## 结论

**NA-014 按当前有限 Steps / Expected 通过。** 真实运行中，已知有效、未知、已撤销 ID 的标记无效证明和错误签名均得到相同认证拒绝；响应、普通日志、安全审计、浏览器历史/存储均未发现秘密候选回显或残留。所有证据按摘要、计数、错误码和允许的公开身份元数据保存，没有输出真实 proof、配对码或私钥。

这是本次功能级错误统一和标记保密验收，不声称已证明统计时序无侧信道、所有任意编码下的不可泄漏，或整个接入系统安全验收完成。本报告不代签独立 reviewer 的交付结论。

原步骤：发送已知标记的无效 proof、已知/未知/撤销 ID；收集响应、普通日志、审计、HTTP 历史、浏览器 storage 与网络记录。

原预期：统一失败不能枚举身份；标记的 code/proof/私钥不回显或入日志；允许的公钥/身份元数据与秘密区分；测试证据脱敏，不复制秘密。

## 版本与隔离

- 实际运行二进制从 `.tmp/acceptance-20261030-r5-target/debug/acp-remote.exe` 复制到 `.tmp/runtime-secret-acceptance-20261030/bin/acp-remote.exe`，复制前后及结束时 SHA-256 均为 `135bd9bddabfdd530a5251aecb1dff0dc640e81f80241ec53c8703c2336d1509`，即用户指定 r6；没有运行新编译的产品。
- r6 清单 `testing/runs/establish-node-access/2026-10-30-implementation-r6.sha256` 为 83 项，文件摘要复核为 `1eb583a929aab7769691fc05ef9a3a86d876e7502d94ac9cbc9f24d5ebcb10f7`。实际运行范围以固定二进制为准，不把并行工作目录后续修改纳入本次证据。
- 自有根目录 `.tmp/runtime-secret-acceptance-20261030/`，owner-only DACL；自有数据目录、真实 Windows DPAPI、SQLite、named pipe。仅启动 `100.119.123.57:30455`，未占用 30456；未操作用户 30447 或其他 tester 30451–30454。
- HTTPS origin 为 `https://faraday.tail74842e.ts.net:30455`。直接只读借用原同域受信证书/私钥路径，不复制私钥，不禁用 TLS 证书校验。真实 Chromium `isSecureContext=true`，使用真实 HTTPS/WSS。
- 已读取并运行 ego-browser skill。新 TaskSpace **11**，仅 p1，未接管既有已 finish Space。浏览器由 ego 工具管理，没有创建磁盘 profile 或操作用户 profile 设置；测试身份只在该自有 origin 保存，结束按 origin 清理。
- 普通沙箱 ego bootstrap 连接失败后改用 owner 执行环境。首个配对码在准备脚本期间过期，页面拒绝；重新启动本次自有 pair 会话，真实控制台核对 claimant 后写入 y+Enter 批准，才建立已知有效身份。

## 请求与真实对照

生产嵌入 Web 页面完成实际 lookup、claim、本机批准、WSS 公钥持有证明，进入 `NODE CONTROL / AUTHENTICATED`，证明不是“节点本来不可用，所以所有请求都失败”。该已知身份 ID 为 `bTrA9ZXwd8Uot6YvR80o_A`，属于允许公开的本次测试元数据。

probe 在浏览器内生成 proof/code/private 三类随机合成标记；private 标记是无用途测试字符串，不是真实 P-256 私钥。标记放入无效 JWS 的载荷字段、HTTP 无效 proof 字符串中。另用临时不可导出攻击者 key 产生结构正常但签名错误的证明。合法格式未知 ID 使用随机 128-bit ID；其 hello 公钥与已知身份一致，使请求不因缺少公钥材料而走不同路径。

每个 WSS 探针都通过真实浏览器 `WebSocket`，收到真实 host challenge，并用已绑定节点公钥验签；随后发送本次无效 proof。签名和发送由 probe 控制，服务端 HTTP、TLS、WSS、认证和存储均为固定 r6 实现，没有 mock 或替换服务端。

| 身份范围 | proof 类型 | 认证前消息序列 | 最终关闭 |
| --- | --- | --- | --- |
| 已知有效 ID | 含三类合成标记、无效载荷/签名 | challenge | 4401 / authentication failed |
| 未知 ID | 相同标记无效证明 | challenge | 4401 / authentication failed |
| 已知有效 ID | 结构正常、错误 key 签名 | challenge | 4401 / authentication failed |
| 未知 ID | 结构正常、错误 key 签名 | challenge | 4401 / authentication failed |
| 未知 ID | 原已知 key 正确签署，但绑定未知 ID | challenge | 4401 / authentication failed |
| 已撤销原 ID | 相同标记无效证明 | challenge | 4401 / authentication failed |
| 已撤销原 ID | 结构正常、错误 key 签名 | challenge | 4401 / authentication failed |
| 已撤销原 ID | 原 key 正确签署的新鲜证明 | challenge | 4401 / authentication failed |

8 项最终错误码、错误原因和消息类型序列完全一致，没有 authenticated 或受保护管理响应；关闭原因不包含标记。challenge 中绑定调用者已提供的 ID、公钥以及新鲜挑战材料，不表示该 ID 被服务端确认存在，不能把它们误判为秘密回显。

撤销通过同数据目录 owner CLI 仅撤销本次测试 ID，前后 owner 列表实际核对 Trusted→Revoked。原生产页面的已认证连接收到 `4410 / revoked`，这属于已有身份的撤销通知；与后续未认证连接统一 `4401` 区分，不把这两种上下文混为“认证可枚举”。

HTTP 实际结果：`pair/claim` 和 `pair/status` 收到标记无效 proof 后均为 HTTP 401，正文统一 `{v:1,error:{code:"pairing_invalid",retryAfterSeconds:null}}`；合法请求结构、未知测试码的 `pair/lookup` 同为上述 401。探针首次漏传 requestId 的 lookup 得到 400 invalid_request，保留为畸形请求对照；补齐合同字段后才取得上述 401，不把探针构造错误算作产品认证失败。

## 秘密候选与网络记录的边界

秘密候选仅在测试页内存中保存和比对，包含三类合成标记、实际本次配对请求的 code、实际发送的各次 client proof；共 21 个候选字符串，展开 JSON 转义、URL 编码、Base64/Base64url 形式后 50 个去重候选。实际身份私钥始终是 `extractable=false` 的 WebCrypto CryptoKey，未尝试导出或复制；用于“私钥文本回显”检查的是合成 private 标记。

fetch/WebSocket 包装器调用浏览器原生传输并只读观察，不伪造返回或改变错误。请求必然会主动发送 code/proof，这是协议所需，不把**预期出站请求体中的候选**判为泄漏。持久证据仅保存请求体 SHA-256、方法、URL、状态、消息类型、字节数和标记是否出站等摘要，不保存真实请求体/签名。

响应通过候选原串检测；HTTP/WSS 响应原文不落盘，只返回回显布尔值、错误码和公开类型。普通日志、审计和存储/历史另对全部 50 个候选形式检查。报告不声称覆盖任意部分截断、任意自定义编码或进程内存取证。

## 六类产物的实际检查

| 产物 | 收集方式和实际范围 | 结果 |
| --- | --- | --- |
| 真实响应 | fetch 克隆响应正文、WSS message 与 close 的只读观察；8 项关闭结果及 HTTP 错误正文保留脱敏形式 | responseEchoes=0，统一失败通过 |
| 普通日志 | 自有 daemon 从 ready 到正常 stopped 的 stdout 193 bytes、stderr 0 bytes；最终停止后再次复查 | 标记命中 0 |
| 安全审计 | Node 内置 SQLite 只读查询真实 `security_audit`，停止前后均读到 2 行：device.pair.approve/trusted、device.revoke/revoked | 标记命中 0；actor/target ID 为允许元数据；未虚构“每次认证失败必有审计”要求 |
| HTTP/导航历史 | 本任务页面 `Page.getNavigationHistory` 2 个条目、Performance resource 12 个条目，并检查网络 URL | 候选命中 0，无配对码或 proof 进入 URL |
| 浏览器 storage | 枚举当前 origin IndexedDB、localStorage、sessionStorage、Cache Storage、document.cookie；IDB 1 个数据库、1 条记录、18 个字符串字段 | 候选命中 0；local/session keys 空、cache 数 0；原生 CryptoKey 单独记录 ECDSA/private/sign/extractable=false，不当作明文私钥泄漏 |
| 网络记录 | 实际浏览器 HTTP/WSS API 观察共 56 条脱敏记录，保留请求体摘要、状态、消息类型、关闭码等 | URL 命中 0；实际出站无效证明包含合成标记属预期；服务器无回显 |

另外检查 owner CLI 前后列表，与 audit 一同得到 externalMarkerHits=0；DOM、cookies、资源和导航 URL、持久 storage 汇总 storageHistoryMarkerHits=0。允许的 clientId、nodeId、公钥/指纹、创建/撤销时间不加入秘密候选，未把正常身份元数据误判为泄漏。owner `device pair` 的本机配对码展示是授权本机交互入口，不当作普通 daemon 日志；临时 console 截取文件结束已删除。

网络记录是应用层真实传输观测，不声称使用系统抓包或导出浏览器全量 HAR；HTTP 历史只限本任务页面，不读取整个用户浏览器 profile 的历史。

## 执行入口与持久证据

新 probe 目录包含 setup.ps1、console.ps1、cleanup.ps1；browser-01-pair.mjs、browser-02-attacks.mjs、browser-03-revoked.mjs、browser-04-artifacts.mjs、browser-05-cleanup.mjs；audit-inspect.mjs、summary.mjs。页面脚本经 `ego-browser nodejs -e "await import('file:///D:/Project/acp-remote/testing/probes/establish-node-access-runtime-secret-20261030/<script>.mjs')"` 执行。

所有实际 JSON 和非敏感输出位于 `.tmp/runtime-secret-acceptance-20261030/evidence/`。脚本绑定本次已 finish 的 Space 11，复现应创建新的授权 TaskSpace/runtime，而非接管该已完成空间。

| 证据 | SHA-256 |
| --- | --- |
| summary.json | 0ef4b0af2ade0680b485cf0e40445136130e9e03cd1e7cd702f44446e792fe31 |
| manifest.json（各证据文件摘要） | ba016b97c559a7a562c2011e3cb08c18a81cddceabdd384bb081eec7c1d8b4a8 |
| artifacts.json（56 条网络记录与产物检查） | 2b8755b5586befc3360f275b376f32748f2a4e51ab775695a6af7a7acccee262 |
| 最终 daemon stdout | c04a15ee03c1fc36bf1b92fe99092298535c890fff2c62f4da147edf85c32c9b |
| 只读审计行检查输入摘要 | a1fe749aad9d60befe416ab2a90b7d4f8c7a17069680c26dcb8d88cd013e9995 |

原始候选 proof/code 未写入持久证据；审计原始临时副本结束已删除，保留脱敏事件元数据与检查摘要。没有发现产品失败分支，因此没有新增产品问题 ID；过期配对和漏 requestId 两项为明确记录的准备/探针边界，不改变 NA-014 原断言。

## 清理

owner 正常 stop，PID 39868 退出；30455 listener=0；准确推导并先确认本次 named pipe 存在，停止后不存在；`.node.lock` 停止后以独占模式成功打开，owner-only 数据目录 ACL 核验通过。30456 未启动。

最终候选比对完成后，页面导航 about:blank 丢弃内存候选和 key 引用，只清本次 origin 的 IndexedDB，`task.finish({keep:[]})` 回执：spaceId=11、closedSpace=true、closedManagedLabels=[p1]、preservedUnmanagedCount=0。没有清理用户 profile 或其他 origin。

验证绝对目标位于本次 runtime 后，删除本次数据目录（测试 DPAPI identity.key、SQLite）、临时配对码、console 读回文件与审计原始临时副本。保留固定二进制、非敏感日志和脱敏 JSON 供复核，借用的 TLS 私钥未复制或删除。新 probe 语法、格式及文件范围完成复核；未提交、推送或归档。
