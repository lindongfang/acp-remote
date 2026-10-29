# NA007-CANCEL-001：R20 独立复测通过并关闭

## 当前实际执行状态（03:45 补充核对）

本有限包已完成，没有仍在运行或等待执行的三分支测试。这里是对已执行证据的完整性复核，**不冒称又重跑了一轮**：evidence/manifest.json 中 30 个文件全部重新计算 SHA-256 匹配；三分支原始 CLI 退出码均为 0，每分支仅一次 Ctrl-C。

- control：真实批准提示处取消，未发送 decision，额外查询 0 条，未创建信任。
- committed：真实提交已完成而原 approved 帧未交付，额外 owner device.list 1 条透明转发成功；先给条件性提示，再确认实际 fingerprint 对应的 `OUTUc3Q9j9qQS8Pir_4clQ` 并打印完整 revoke 命令，未回滚 trust。
- precommit：真实 decision 尚未转发，Ctrl-C 后 EOF 取消；额外 owner 查询 1 条真实返回，但没有本次 fingerprint，CLI 只说“未确认”，没有假报取消或 trusted。
- 同 key 重启恢复实际通过；浏览器、自有 CLI/keeper/代理/节点、端口、pipe、锁及本次私有数据已完成清理。

**剩余项：本次原三分支及恢复关联无剩余项。NA007-CANCEL-001 当前在给定 R20 上已关闭，不是预签。** 5 秒满预算故障、查询多页、第二次 Ctrl-C 等未执行边界仍按下文记录，不扩张为本次已通过能力，也不临时新增为该有限包门禁。

原 R17 FAIL 报告再次核对摘要与本轮记录一致，仍保存原 FAIL/OPEN 历史，不将它覆盖为 PASS；当前关闭结论由本 R20 原复测报告承载。未修改产品。

日期：2026-10-30，Asia/Shanghai，约 03:32–03:41。结论：**PASS；NA007-CANCEL-001 在本次固定 R20 普通产物上 CLOSED。** 关闭者为原独立问题提出/验收会话；未参与产品修复。仅新增 R20 probe 和本报告，保留 R17 原 FAIL，不修改产品、规格、旧报告或原预期，不提交、推送或归档。

## 固定版本与旧证据

- R20 源 binary：`.tmp/frozen-implementation-r20-20261030/bin/acp-remote.exe`；先核验后复制至自有 `.tmp/approval-cancel-boundary-r20-20261030/bin/acp-remote.exe`。运行前后 SHA-256 都为 `561a6024e983dd5239c09d1388ca5d256625f690ee9345e64b9654bc43aafbe3`。真实 CLI 和真实 backend 都运行此副本，没有用重编译产品替代。
- R20 清单文件 `2026-10-30-implementation-r20.sha256` 独立复核 83 项条目、文件 SHA-256 `73727bcf617f3f96bcc24c4cbb7b38ba044a56856c6cb4d628869832c8b368a4`。主方全 check 的 144 库+4 向量/Web50 等属于交接证据，本会话不冒称重跑；本次实际执行为下述普通二进制边界测试。
- 原 [R17 FAIL 报告](2026-10-30-approval-cancel-boundary-acceptance.md) 仍明确 FAIL / OPEN，未回写其历史。旧报告本轮只读 SHA-256 `ed608964cf78cb61e8fc6c7c314f7fa00f9ea47943efeefddeb5a44241e85358`。R17 的退出码 0xC000013A、缺 revoke 提示仍是当时有效发现，不因本次关闭而消除。
- 新 probe 为 `testing/probes/establish-node-access-approval-cancel-r20-20261030/`；新运行根目录 owner-only `.tmp/approval-cancel-boundary-r20-20261030/`，与 R17 目录隔离。仅自有 30455 节点，30456 未启动，不操作 30447 或其他 tester 节点/管道。
- 真实 owner TTY、Windows DPAPI、SQLite、named pipe、受信同域 HTTPS/WSS、IndexedDB/WebCrypto。TLS 证书/私钥只读借用，不复制私钥、不绕过 TLS 验证。

## 代理适配：真实查询透明转发

保留原三种延迟 seam：control 在批准提示处取消；precommit 持住真实 CLI 已写出的 approve decision，不转发；committed 转发 decision 后持住真实 backend 已提交的 approved 事件。原始帧只在内存中原字节转发或暂缓，不重编码、不改字段、不伪造批准/查询结果，不落 raw code/proof 字节。

R20 扩展仅发生在自有适配器：第一条 pair 连接建立后，就用同一 `LocalIpcAddress::bind_next` 增加已监听实例，允许真实 CLI 在 drop 原 pairing session 后另开 `device.list`。每条额外连接重新验证 owner SID；只接受真实 v1 `device.list`，后端仍检查管道 server PID 必须等于本次 owner node.pid。启动侧验证 backend PID 的 GetOwnerSid。actual backend 自身继续执行产品 owner token 检查，release/control 目录仅当前 owner 可访问。

额外查询有界：最多 8 个连接，每条 I/O 上限 3 秒；pair 结束后监听窗口 6 秒，适配器硬期限和 Join 收敛均有上限。正常页仅一条查询。该 6 秒是夹具等待连接的寿命，**不是替换产品的 5 秒总查询预算**；产品本身仍以一次 timeout 包住整个 fingerprint 查找过程。此轮没有故意挂起查询，未将正常快速查询冒充“精确 5 秒超时边界”实测。

额外 request/response 都向真实 backend 原字节传输，日志仅记录 command、owner 校验、PID、字节数、replyOk、条目数和耗时，不记录请求内容、配对码或 proof。它是本机故障适配器，不是产品默认 pipe，也不增加公网批准端点。

适配器离线构建于自有 target，执行文件 SHA-256 `e94a8d095fc523beb79e4b4cd0e95fb22d3b5aa284b6d94d22a36409170e2aa2`。只复用现有 app owner/address helper 及已有依赖；普通 CLI/backend 始终为上述正式 R20 副本。

## 三分支实际结果

所有场景均使用真实隐藏控制台 keeper，固定 R20 CLI 的 stdin/stdout 是真实 console；y+Enter 通过 WriteConsoleInputW，Ctrl-C 通过 GenerateConsoleCtrlEvent。**每个分支只发一次 Ctrl-C**；驱动还拒绝同一 case 的重复 signal。没有在 owner 查询期间发送第二次 Ctrl-C，也不以第二个事件的行为评价本问题。

屏幕只提取布尔值、必要公开 clientId 和实际 revoke 命令；配对码仅经私有 child stdout 管道进入浏览器驱动内存，不写证据文件。额外查询、数据库指纹都是真实结果，不由 helper 代替 CLI 寻找或打印身份。

| 分支 | 最终实际行为 | 数据与查询 | 结论 |
| --- | --- | --- | --- |
| control：批准提示处 | 打印 Pairing cancelled before approval；退出 0；没有中断 decision 的不确定提示 | decision 未发，0 条额外查询；DB 无该信任/审计 | PASS |
| committed：已提交、approved 未交付 | 先打印 decision interrupted / approval may already have committed、device list / if approved device revoke、cancellation does not revoke trust；再打印实际 trusted ID 和完整 revoke 命令，退出 0 | 原 stream 已断开，approvedDelivered=false；另开真实 device.list 成功，owner/后端 PID 校验通过；DB 原信任保留 | PASS |
| precommit：decision 已写但未转发 | 同样先给诚实条件性提示；随后打印 Approval was not confirmed，退出 0；没有假报已取消、trusted 或实际 ID | 原 stream EOF，decisionForwarded=false；真实 device.list 成功，但没有匹配本次 fingerprint 的 trusted 记录 | PASS |

### committed：原 FAIL 的直接复测

本 case 生产浏览器名称 `NA007 seam committed`。Ctrl-C 前：适配器 stage=approved_held，decisionForwarded=true、approvedHeld=true、approvedDelivered=false；独立只读 SQLite 已有下列 trusted 记录与同 target 的 device.pair.approve / trusted 审计：

- clientId：`OUTUc3Q9j9qQS8Pir_4clQ`。
- 数据库实际公开 key 指纹：`c7632ec583f7c6197b429e7f76397a8e9a0386f888e483bd34919234313b663d`。
- 来自原 pending 事件的 fingerprint 与该 DB 指纹完全一致，revoked_at=null。

真实 CLI PID 47612 收到唯一一次 Ctrl-C 后，主动 drop 原 stream；适配器观察到 client_disconnected_after_commit。原 approved 帧没有交付，后写入 release.flag 时已无该 CLI stream，不能把放行说成旧帧交付成功。

实际额外 owner 查询：1 条，request 95 bytes / response 331 bytes，replyOk=true、itemCount=1、无 next cursor，frontOwnerVerified/backendPidChecked/responseForwarded 均为 true。计时分辨率为整毫秒，记录 elapsedMs=0，表示该夹具记录区间小于 1ms，不表示没有真实 I/O。

原 CLI 自己从该真实结果按 pending fingerprint 找到上述 trusted ID。屏幕提取值：decisionInterrupted=true、conditionalCommitted=true、deviceListHint=true、cancellationNotRevocation=true、trustedMessage=true、clientId 精确匹配 DB、fullRevokeCommand=true、conditionalBeforeConfirmedId=true、cancelledBeforeApproval=false，退出 **0**。

实际完整命令（夹具配置路径，未执行撤销）：

```text
acp-remote --config "D:\Project\acp-remote\.tmp\approval-cancel-boundary-r20-20261030\committed/proxy.toml" device revoke --device-id OUTUc3Q9j9qQS8Pir_4clQ
```

条件性说明在终端中位于确定 clientId 之前；冻结 R20 源码中该说明先 flush，再进入总计 5 秒的查询。这与实际输出顺序一致，没有等到查询确认后才假装首句是已知结果。夹具路径指向自有代理 config，本次只验完整命令输出，不声称通过仅支持 pair/list 的代理执行了 revoke，也不为了验证提示而撤销需要证明保留的信任。

### precommit：没有把另一条真实记录误当本次成功

真实 CLI PID 41200 已写 approve，适配器 stage=decision_held、decisionForwarded=false；此时没有本 case 信任或批准审计。唯一 Ctrl-C 后，原 stream EOF，stage=cancelled_before_forward，decision 始终未转发。

CLI 实际又开了 1 条 owner device.list，收到的真实 response 同样有 **1 条既有 trusted 记录**，即前一个 committed case，而本次 pending fingerprint 为 `aa24878046c1445794d385c181ca5a9fb0dc3d88d025987fe46045616c3e1eb2`，与该记录不匹配。CLI 没有简单取“第一条 trusted”误报成功：

- conditionalCommitted/revokeHint/deviceListHint/cancellationNotRevocation 均为 true；
- `Approval was not confirmed.` 实际出现；
- trustedMessage=false、clientId=null、cancelledBeforeApproval=false、checkUnavailable=false；
- CLI 退出 0，DB 仍仅原 committed 一条信任，未新增 precommit case。

因此它在已发送 decision、结果未确认时没有假称取消，也没有凭其他 key 的 trusted 记录假称批准；这是实际 fingerprint 匹配和未知状态说明的动态证据。

## 信任保留与真实浏览器恢复

committed case 的页面仅在真实 `/pair/status` 返回 approved 且 hostProof 用已绑定节点公钥验签成功后，受控丢弃一次结果交付；它是明确的响应适配器，不是物理丢包。实际 IDB 保持 pending_confirmation / clientId=null、原 key，私钥不可导出；没有手改状态。

原节点正常 stop 后，相同复制 R20 exe、相同数据目录重启（PID 43704→48540），DB 仍是同一 trusted ID、原 fingerprint、revoked_at=null，批准审计仍只有一条。真实生产页 reload 后自动进入 AUTHENTICATED，IDB 变 consumed，clientId 为同一 `OUTUc3Q9j9qQS8Pir_4clQ`；nodeId 不变，公钥编码字符串摘要始终 `64af8aa8273bc82362e34235921671b1e229d130151bc4ed15966719890c09e7`，privateKey.extractable=false。

这项关联用真实 SQLite/DPAPI/TLS/IndexedDB/WebCrypto，不是自写恢复客户端；没有新配对、再次批准或回滚既有 trust。之后为 precommit case 清理测试 origin 建新身份属于 case 隔离，发生在恢复断言完成后。

## 问题关闭与范围

**NA007-CANCEL-001：CLOSED on R20。** 原问题缺失的“提交后取消仍给出 revoke 提示”现在由同 seam、同真实 TTY、未交付 approved 条件动态重测通过；确认查询还拿到真实同 fingerprint 的完整稳定 ID。control/precommit 防止修复引入假取消、假成功的有限回归也通过。

保留 R17 FAIL 文档不变。本关闭仅适用于给定 R20 binary 和该问题三分支，不宣称 NA-007 所有 SQL 故障矩阵、完整 change 或待归档条件都已通过；也不代签独立 reviewer。

本轮只有快速成功 owner 查询及无匹配结果，未故意断开查询 backend、逼满 5 秒预算、构造多页结果或发送第二次 Ctrl-C。相关失败/分页/第二信号行为不能借本报告自动视为已执行；没有因它们未执行而改变本次原三分支有限范围。

## 证据与清理

新 probe 入口为 stage-and-start.ps1、start-case/keeper/console/observe-cli.ps1、Rust src/main.rs（pair 延迟+额外查询）、browser-*、db-snapshot.mjs、assert-r20.mjs 和 cleanup.ps1。新 owner-only runtime 为 `.tmp/approval-cancel-boundary-r20-20261030/`，所有公开/脱敏结果在 evidence/；没有记录 raw IPC、配对 code/proof 或私钥内容。

| 证据 | SHA-256 |
| --- | --- |
| evidence/r20-results.json | 98313ecc27340b6b29cf79bc336e751efeeedb62ee692d5efec9774f51def66d |
| evidence/manifest.json | ce1b2a61f8bcd86c187ed91a06632db2f90832d4b9d1ea596ec0ba00d1762385 |
| evidence/browser-recovery.json | 24d8fe1424b54db21144e9b5a7e43652c30b97a543dd643dee95890359170370 |

严格结果断言 controlPassed / committedPassed / precommitPassed / recoveryPassed 全部为 true；结果文件同时保留原 R17 报告摘要与仍为 FAIL 的只读验证。

全部自有 CLI、keeper、主代理和额外查询监听均已退出；额外监听自然有界收敛，未修改产品停止语义。节点正常 owner stop，PID 48540 退出，30455/30456 无 listener，真实 backend pipe 消失，`.node.lock` 可独占打开；三个 proxy owner-only 控制目录和确定地址分别验证 pipe 不存在，不连接未知用户 pipe。

TaskSpace **15** finish：closedSpace=true，closedManagedLabels=[p1]，preservedUnmanagedCount=0；仅清本次 :30455 origin 的测试 IDB，未操作普通用户 profile。最终快照仍为原 trusted 信任后，按已校验绝对路径删除自有 DPAPI/SQLite 测试数据；借用证书私钥、用户数据库及其他节点不动。保留复制普通 exe、适配器 target 与脱敏证据供复核。

新 probe Rust/Node/PowerShell 语法与格式、实际变更范围完成检查；未修改产品或旧报告，无提交、推送或归档。
