# NA-007 / NA-022：批准响应丢失后的真实页面恢复

日期：2026-10-30，Asia/Shanghai，约 01:48–01:56。执行者为独立侧任务会话；未修改产品实现、规格、既有报告、配置或公开 CI action。新增范围仅本报告与 `testing/probes/establish-node-access-response-loss-20261030/`。临时二进制、节点配置、日志、测试身份和 JSON 证据位于 `.tmp/approval-response-loss-20261030/`。

## 结论

**本次要求的“批准已持久提交、页面仍保留原 key 且 clientId=null、同目录节点重启后生产页面恢复原稳定 ID”分支实际通过。** 没有用自写恢复客户端代替产品页面，没有直接修改 IndexedDB 状态，也没有将批准响应结果交给应用后再把 ID 改回 null。

| 用例范围 | 实际证据 | 状态 |
| --- | --- | --- |
| NA-007：提交后响应丢失、浏览器恢复 | 两轮均先验签确认后端 approved，再仅对该次 fetch 结果抛受控网络错误；页面保持 pending_confirmation / clientId=null；owner 列表证实已提交；重启保持原信任 | 本侧任务覆盖分支通过 |
| NA-022：节点重启使临时配对失效后，同 key 恢复 | 30455 真实页面 reload 自动恢复原 ID；30456 在原页面点击生产“重新检查批准状态”，直接观测 pairing_invalid → null-clientId WSS hello → 原稳定 ID | 覆盖分支通过，见 RL-OBS-001 观测边界 |
| NA-007 其他提交前/中故障、并发 CLI 取消 | 本次未注入 | 未执行，不签整例通过 |
| NA-022 留存期内同 key 重取、自然过期边界及取消/拒绝全部矩阵 | 未补做，避免把客户端先改成 consumed 后偷换本次恢复路径；本次通过重启清理临时材料 | 未执行，不签整例通过 |

本报告只提交可复核执行证据，不代签独立 reviewer，也不要求用户手工操作或签字。

## 固定版本和真实环境

- 从 `.tmp/acceptance-20261030-r5-target/debug/acp-remote.exe` 复制到自有 `bin/acp-remote.exe`，复制前后及执行后 SHA-256 均为 `2dc0afa04b55376797587136048f5efbe7d665ff7ee69525da248fa4d73305f2`。未重建或替换运行中的二进制。
- r5a 79 项清单 `testing/runs/establish-node-access/2026-10-30-implementation-r5a.sha256` 文件摘要复算为 `96f7b25da1b7abf3c5f65d069723ca87b095800f08967720aadbd4fc621975ec`。并行源码继续演进；本次不声称工作目录所有源码仍等于该清单，以实际固定二进制行为为准。
- Windows owner 会话，数据目录 DACL 核验只有当前 owner SID 的 Allow 访问项。真实 SQLite、DPAPI 当前用户身份保护、owner named pipe、Chromium IndexedDB 不可导出 P-256 CryptoKey、HTTP/WSS 和 TLS，均未 mock。
- 仅使用 `100.119.123.57:30455/30456`，HTTPS origin 为 `https://faraday.tail74842e.ts.net:30455/30456`；未启动、停止或修改用户 30447 或其他 tester 30451–30454。
- 证书和 TLS 私钥只通过原受信同域路径配置给节点读取，没有复制或打印私钥。Node TLS 独立连接启用 `rejectUnauthorized:true`，两个端口均 authorized=true、TLSv1.3；证书 CN 为 faraday.tail74842e.ts.net，Let's Encrypt YE1，叶证书 SHA-256 为 `48:1A:14:A3:6A:CC:70:B3:F7:9F:24:53:5E:81:8D:04:26:89:40:3C:FA:D5:1C:F3:F4:DF:DF:F1:46:AB:0C:FC`。浏览器 `isSecureContext=true`，未禁用证书校验。
- 读取并运行 ego-browser skill，使用新 TaskSpace **10**，p1/p2 两页完成本次独立目标，没有接管已 finish 的 Space 5。没有另建磁盘浏览器 profile；ego 的底层 profile 存放位置不是本会话配置项，**不能声称它位于 .tmp**。本次生成的浏览器身份由该工具管理的 IndexedDB 保存，结束时只清理两个自有测试 origin 的 IndexedDB，不清 profile 或其他 origin。

初次受限 shell 启动曾打印 ready，但后续 owner IPC 返回 os error 5，不能作为成功测试；随后使用真实 owner 执行环境启动并核实 IPC ready，才进行下述测试。未把权限失败算通过或转交用户。

## 故障注入方式与证据保护

这是**受控的响应交付丢失适配器**，不是声称真实网卡/路由丢包：真实页面 `window.fetch` 包装器先等待原 fetch 返回；仅在 `/api/access/v1/pair/status` 返回 HTTP 200、hostProof 载荷 status=approved，且用 IndexedDB 已绑定节点公钥通过 WebCrypto ES256 验签后，抛一次 TypeError。该次 approved 结果没有返回给产品 PairingApi，包装器不写入身份记录，也不伪造响应或服务端状态。

包装器只记录 approved 状态、稳定 clientId、验签布尔值和当时本地 status/clientId；没有保存或输出 hostProof、配对 proof 或私钥。控制台配对码只短暂保存在本次 runtime，未打印到报告或 probe 源码，结束删除。公钥仅比较编码字符串的 SHA-256；私钥仅检查 extractable=false，不尝试导出。

批准通过本次自有隐藏 Windows 控制台运行固定二进制的真实 `device pair`；确认 console 中为本次 claimant 和 `Approve this client? [y/N]` 后，使用 Windows WriteConsoleInputW 写入 y + Enter。不是网络批准端点，不是绕过 TTY 的 IPC approve，也不是用户人工验收。

## A：30455 生产页面 reload 自动恢复

1. 页面 p1 通过真实受信 HTTPS 打开生产嵌入页面，安装一次性丢响应包装器，通过实际表单 lookup、claim；本机终端明确批准。
2. 包装器实际丢弃 1 次已验签 approved 响应，客户端出现“暂时无法检查批准结果 / 节点返回的数据不符合接入合同，已停止处理”。此时只读 IndexedDB：`status=pending_confirmation`、`clientId=null`、privateExtractable=false。
3. 后端签名批准结果的稳定 ID 为 `VEN-0B9tlYGRJdIbJjoalA`；owner `device list` 实际包含同一 ID，显示可信。不是仅凭包装器解码推断数据库提交。
4. 通过 owner CLI 正常 stop，旧 PID 41616 退出且端口释放；从自有相同数据目录重启为 PID 45000，nodeId 和 owner 列表完全保持。没有再次配对或批准，临时配对状态随进程重启清除。
5. 对 p1 调用真实 `page.reload()`；未点击“验证已保存身份”，未运行自写 WSS 恢复客户端。生产页面自动进入“节点管理 / AUTHENTICATED”，显示原稳定 ID 和 revision 1。
6. IndexedDB 最终 `status=consumed`、`clientId=VEN-0B9tlYGRJdIbJjoalA`；nodeId 未变，私钥不可导出。恢复前后公钥编码字符串的 SHA-256 都为 `7f5d7353a601536cba8a27d22ed5d064d6bdf01dd42c28d0968c6e0c8eebb3d4`。

**RL-OBS-001（观测限制，非已证实产品失败）**：通过 `Page.addScriptToEvaluateOnNewDocument` 安装的 fetch/WebSocket 观测包装在本工具的 reload 后没有出现，等待观测变量的 20 秒探针超时。实际 UI 和只读 IDB 已证明 reload 恢复成功；不能把探针超时写成产品恢复失败，也不能声称本轮录到了 pairing_invalid 和 null hello。保留此限制，并通过下一轮独立的生产页面分支补齐直接帧级证据；没有把两轮伪装成同一条 trace。

## B：30456 原页面重新检查，直接观测 null-ID WSS 恢复

第二个自有数据目录、节点和同 TaskSpace p2；使用相同固定二进制，但独立节点身份和浏览器 key。安装实时有效的只读 fetch/WebSocket 观测包装，不依赖跨 reload 注入；原页面仍负责全部 HTTP 请求、签名、WSS 建连和管理命令。

1. 重复真实配对和本机批准，适配器仅丢弃一次已验签 approved 响应；本地再次保持 `pending_confirmation / clientId=null`，owner 列表确认已提交 ID `Lx_f92PzRgo7xrsAPfhHTg`。
2. owner stop/restart 同目录：PID 44964 → 41020，旧进程退出、端口释放，nodeId 和 owner 列表不变。
3. 仍为同一页面和同一 pending 身份，点击生产按钮“重新检查批准状态”。本轮**没有 reload**，没有把 IDB 改成旧状态，没有点击“验证已保存身份”。
4. 实际观测到 `/api/access/v1/pair/challenge` → HTTP **401**，错误 `pairing_invalid`；之后生产 WebSocket.send 发出唯一 hello，`clientId:null`，其 clientPublicKey 编码字符串 SHA-256 为 `c35a950f7bbffbf4755a12107d5573b1bcf564a101b5086d4d33ea61d1375f7b`，与丢响应后 IDB 的公钥摘要一致。
5. 收到真实 WSS `authenticated`，clientId 为原批准 ID `Lx_f92PzRgo7xrsAPfhHTg`；页面自动进入生产节点管理，显示可信设备 revision 1。没有新的 pair/approve。
6. 最终 IDB 为 consumed、原稳定 ID，公钥和 nodeId 均未变、privateExtractable=false。所有 sameKey/sameNode/stableClientId/nullHello/helloSameKey/pairingInvalidObserved 断言为 true。

两轮共同支持本次恢复目标：A 证明真实 reload 路径的最终恢复和 key/ID 连续性；B 对生产“重新检查”路径直接证明临时材料失效后 null-clientId WSS 持有证明恢复。A 的确切帧级顺序仍受 RL-OBS-001 限制。

## 命令入口和证据

probe 目录中 setup.ps1 负责固定副本/自有配置；console.ps1 只处理经进程路径、命令行检查的自有 pair；restart.ps1 支持 30455/30456；browser-01 至 07 负责真实页面动作、只读观测和清理。browser-04 是上述观测超时探针，不计为成功；browser-05 保存 A 的实际结果和 B 的 pending 前提，browser-06 保存 B 的帧级断言。tls-evidence.mjs 和 summarize.mjs 分别保存受信 TLS 与汇总。

浏览器脚本通过 `ego-browser nodejs -e "await import('file:///D:/Project/acp-remote/testing/probes/establish-node-access-response-loss-20261030/<script>.mjs')"` 执行。重启和清理通过专属自有配置 CLI；所有实际 runtime JSON 位于 `.tmp/approval-response-loss-20261030/evidence/`。这些脚本描述本次 Space 10 的顺序执行，复现需要新的授权 TaskSpace/runtime，不应直接接管已经 finish 的 Space 10。

| 证据 | SHA-256 |
| --- | --- |
| evidence/summary.json | 5e107efc4e545ff602f74a503fd0d33a26c2aa9e2d2e12d79377c594e9d18b70 |
| evidence/manifest.json（各 JSON 与 owner 列表摘要） | 450ad048d47fb6c792e8941868d098958a895d62b09586ad7d5f00ff854550dc |
| evidence/reload-recovery.json | ee1c3586014dff3ff5a9db1d5003a20e961b5402feb8c917dc3cecfadbdfc990 |
| evidence/retry-recovery-30456.json | 7524235ac7a3fda4e78dca0bdc7dda42896a2cfca46334a1ca74533376edd1e1 |
| evidence/cleanup.json | 73b6c3cb04bb89525f9c5493e41820c0ab143c3ebf0b587a7f63311d4948e85d |

## 资源清理与边界

结束通过 owner CLI 正常停止两个节点；PID 45000/41020 均退出，30455/30456 listener 数量均为 0。先用当前 owner SID 与真实规范化数据路径推导 named pipe 并确认存在，停止后确认两个确切管道均消失；两个 `.node.lock` 均能以独占模式打开，证明无遗留文件锁。数据目录 ACL 只允许 owner。

只对两自有 origin 执行 scoped IndexedDB 清理，关闭 p1/p2，再 `task.finish({keep:[]})`。工具回执释放了本次任务；它保留 1 个未受管理标签，`closedSpace=false`，所以**没有声称整个浏览器窗口物理关闭**。随后工具明确已 finish、禁止继续操作该 Space，本会话遵守而不重新接管；未操作普通用户 profile 或其他任务标签。

核实绝对目标均位于本次 runtime 后，删除本次两个节点的数据目录（含生成的 DPAPI identity.key、SQLite）、临时配对码和 console 输入记录。借用的证书/私钥路径完全未删除或修改。保留固定二进制副本、非敏感节点日志与脱敏证据供复核；没有自有节点、端口、named pipe、锁或受管理页面仍在运行。

范围内 diff/格式检查完成；未提交、推送或归档。没有发现需要修改产品预期的失败分支；保留 RL-OBS-001 和 NA-007/022 未执行矩阵的明确边界。
