# NA-007：真实 CLI 批准提交交界取消验收

日期：2026-10-30，Asia/Shanghai，约 03:02–03:17。独立有界侧任务，仅新增本报告与 `testing/probes/establish-node-access-approval-cancel-boundary-20261030/`，不修改产品、规格、原预期或旧报告，不提交、推送或归档。

## 结论：FAIL，NA007-CANCEL-001 / P2 / OPEN

**真实后端已提交批准、approved 事件尚未交付真实 TTY CLI 时，Ctrl-C 使 CLI 直接退出，没有输出 `device revoke` 提示。** 数据库信任没有回滚，原浏览器 key 也确实能在同目录重启后恢复原稳定 ID；这两项通过不能替代缺失的提示断言。

| 有限分支 | 实际结果 | 结论 |
| --- | --- | --- |
| 批准提示处 Ctrl-C 正控制 | 打印 Pairing cancelled before approval；退出 0；未发 decision；后端无该身份 | PASS |
| CLI 已写 approve、代理尚未转发 | Ctrl-C 后 CLI 退出、前端断开；代理未转发 decision，后端未新增该信任/审计 | PASS（提交前取消功能，非优雅退出承诺） |
| 后端已提交、approved 事件被持住 | DB trusted+批准审计已存在；Ctrl-C 后 CLI 退出码 -1073741510 / 0xC000013A，未显示批准或 revoke 提示 | **FAIL：NA007-CANCEL-001** |
| 已提交信任保留 | Ctrl-C 后、放行尝试后、同目录重启后，仍是同一 Trusted ID、revoked_at=null | PASS |
| 批准 HTTP 响应丢失后的生产页面恢复 | IDB pending_confirmation/clientId=null、原不可导出 key；节点重启后 reload 由真实页面恢复原 ID、变 consumed | PASS |

依据：当前 tests.md NA-007 的原预期明确为“提交后取消提示用device revoke，不能回滚信任”；access-contract.md 配对状态条款要求线性化点后 CLI 取消不能回滚且提示“已批准，请用device revoke撤销”，之前 CLI 关闭/IPC 断开/Ctrl-C 立即取消。没有改成只要求数据库保留就算通过。

## 固定版本与隔离

- 普通 CLI 与普通节点均来自先复制的 R17 exe：SHA-256 `38aa9d02f334e3c81fdc900ed4f8943bb78339650981bc0046a609381d03e6c2`，自有副本为 `.tmp/approval-cancel-boundary-20261030/bin/acp-remote.exe`。复制前后及执行期间保持同一产物，没有重编译产品替代它。
- R17 83 项清单 SHA-256 `5074890c209a702335f13042a26dba64278efd7a7b0a70a4e42e2f698ba3e26f`。已读取其原 reviewer 限定报告，未把此前其他关闭项当成本次 CLI 边界证据。
- 冻结 R17 与本轮读取源码的 main.rs 摘要均为 `77ae4a80570083d9ff3aca9237cc9dd70146a7d61afca3c6d5dc921b9670952a`，ipc.rs 均为 `1f388e3ebe46db7f64fa6f52e5c6352e6b4562aa6247e91c6e275e650ed2c408`。源码只用于理解和适配接口，实际 PASS/FAIL 均以下述普通 exe 动态行为为准。
- 自有 owner-only 根目录 `.tmp/approval-cancel-boundary-20261030/`，实际节点数据为 `data-30455/`，监听 `100.119.123.57:30455`；30456 未启动。不操作用户 30447 或其他 tester 节点/pipe/数据。
- 真实 Windows owner、DPAPI、SQLite、named pipe、受信同域 HTTPS/WSS、Chromium IndexedDB/WebCrypto。借用 TLS 证书/私钥路径只读，不复制私钥、不关闭证书校验。
- 新 ego TaskSpace **14**、p1，全程同一空间；没有接管旧已 finish 空间。底层 profile 由 ego 管理，不声称 profile 物理目录位于 .tmp；只操作本次自有 :30455 origin，结束清理该测试 IDB。

## Owner IPC 延迟适配器：不是产品默认管道或网络批准

自有 Rust probe 通过产品公开的 `LocalIpcAddress::for_data_directory` 分别为真实 backend data 和各 case 的 `proxydata` 生成地址。真实 CLI 的配置 data_dir 指向该 case 的 proxydata；这里**没有启动另一个产品节点或创建替代身份**，只创建 owner-only 的 named pipe 前端。实际 node/data/backend pipe 不变。

前端使用 `LocalIpcAddress::bind_first` 和 `verify_peer`，验证真实 CLI 的 owner SID。后端连接只指向已知自有 backend 地址；`GetNamedPipeServerProcessId` 必须等于本次 node.pid，启动侧还用 GetOwnerSid 核对该进程属于当前 owner。真实 backend 自身继续执行产品 owner token 检查。全部实际 case 均记录 frontOwnerVerified=true、backendPidChecked=true。

三个 proxydata 和 release 控制文件所在目录均只允许当前 owner SID。release 是 owner-only 文件旗标，没有新增网络批准/调试端点，不连接未知用户管道，不伪造审批帧。

适配器逐字节转发真实 CLI 的初始 device.pair 和 approve decision；真实 backend 的 started、pending_confirmation 也逐字节转发。只有以下选定位置暂停原字节向前交付：

1. **committed**：decision 已转发，真实 backend 返回 event=approved；把原帧仅留内存，不交付 CLI，等待 owner release 或前端断开。
2. **precommit**：真实 CLI 已写出 approve decision，原帧仅留内存，尚未向 backend 写入；等待 owner release 或前端断开。
3. **control**：在真实批准提示处不提交 decision，直接验证 Ctrl-C。

持有上限 50 秒，连接前等候上限 90 秒，帧预算 32 KiB。JSON 解析只辨别阶段/公开 ID，原帧不重编码、不改字段、不替换错误或批准结果；不保存原始帧、配对秘密或 proof，不打印 raw bytes。状态记录只有阶段、布尔值、owner SID、PID 和公开 clientId。

适配器是单独 probe，不是普通产品中的故障开关。它用现有库的地址/owner helper 编译，目标目录独占在本次 .tmp/target，离线构建；实际运行的 CLI/node 仍为固定复制 R17 exe。适配器执行文件摘要 `fb1df768503454c2e3438fc7435c7f6da59fe1421a4f90ea460c00796e966852`，锁文件随新 probe 保存。依赖仅复用现有 app/tokio/serde_json/windows-sys；编译的 hard-link fallback 警告不作产品测试结果。

夹具里的 CLI config 路径指向 proxydata，所以不能把提示命令的夹具路径当作普通部署默认 pipe 的可执行性测试。本次真正失败是**提示根本没有输出**，不是拿该夹具路径执行 revoke 失败。

## 真实 TTY 与 Ctrl-C 正控制

每个 case 的 hidden Windows PowerShell keeper 持有真实控制台，固定 R17 CLI 是其子进程，stdin/stdout 均为真实 console，产品自己的 IsTerminal 和审批提示路径实际通过。keeper 自身安装控制事件 guard 保持屏幕供读取；对子 CLI 不安装代理取消逻辑。使用 WriteConsoleInputW 写入 y+Enter，使用 GenerateConsoleCtrlEvent(CTRL_C_EVENT,0) 向仅属于本 case 的控制台发送真实 Windows Ctrl-C 事件；不是仅写一个文本 `\x03`、杀未知 PID 或要求用户手工操作。

console helper 先检查 keeper 命令行属于本次目录，后续 CLI PID/路径也做所有权检查。屏幕只提取提示和结果布尔值，不保存原始屏幕。配对码只通过 child_process.execFile 的私有 stdout 管道传入浏览器驱动内存，再填实际表单，不落盘、不输出到工具结果；没有写 pair code/proof 的 JSON 或 transcript。

初轮 control 的 Start-Process 包装没有可靠取得 ExitCode（null），但实际取消文字、管道断开和 DB 零记录已观察到；该初轮保留为 control-initial，不拿 null 冒充 0。改为直接 .NET Process 启动后，**用与 committed/precommit 完全同一 keeper 再做一次 control**：

- `Pairing cancelled before approval.` 实际出现；
- CLI PID 31788 退出码为 **0**；
- proxy stage=cancelled_before_decision，decisionForwarded=false；
- SQLite 没有该 control 身份或批准审计。

因此事件送达和真实 CLI 取消路径有正控制，下面的提示缺失不是因为 Ctrl-C 适配器没有工作。

## 已提交 case：确切交界和失败证据

1. 生产 Web 页面配对名称为 `NA007 seam committed`，CLI 进入真实批准提示，明确 y+Enter。
2. 代理把真实 approve decision 转发给 backend PID 41268，收到 backend 原始 approved 事件后持住：approvedHeld=true、decisionForwarded=true、approvedDelivered=false。
3. **在发送 Ctrl-C 前**独立只读 SQLite：clientId=`zw5j3sxztp23kw5lA2hWFg`、display_name 对应本 case、status=trusted、revoked_at=null；security_audit 有同 target 的 device.pair.approve / trusted。不是仅看到内存事件就推断 COMMIT 成功。
4. 此时给真实 CLI PID 41864 发 Ctrl-C。CLI 退出码 **-1073741510（32-bit hex 0xC000013A）**，代理看到真实前端断开并退出，stage=client_disconnected_after_commit。
5. 真实控制台结果提取：trustedMessage=false、cancelledBeforeApproval=false、**revokeHint=false**。没有 `device revoke` 文本，也没有“已批准，请撤销”的状态提示。
6. 已按计划写 release.flag，但在该时刻 CLI 已退出，真实 pipe 已断开，故原 approved 帧已不可能交付；approvedDelivered 仍 false。没有把此事伪装为“放行成功后收到正常成功提示”。
7. 之后 DB 和审计仍保持原信任，没有回滚。这说明失败是用户被遗漏告知已批准/应撤销，**不是错误撤销或权限回滚**。

### NA007-CANCEL-001

- 严重性：P2；状态：OPEN；固定版本：上述 R17 普通 exe。
- 预期：提交后的取消必须提示已批准、使用 device revoke 撤销，同时保留信任。
- 实际：Ctrl-C 后 CLI 退出，没有任何 revoke 提示；信任已存在且保留。
- 影响：操作者取消后可能不知道已经授予了长期信任，无法从本次 CLI 获得下一步撤销提示；阻断本次 NA-007 该断言通过。
- 证据关联：committed-proxy-state.json、committed-console-read.json、committed-console-ctrlc.json、committed-cli-exit.json、db-committed-before-ctrlc / after-release / after-restart、browser-recovery.json。
- 源码对应：冻结 R17 `pair_device` 在 session.approve().await 后直接等待 session.next_event().await，没有该等候阶段的 Ctrl-C 分支；这与动态结果相符，但本 FAIL 不是由源码推断得出。退出码是实际观测，不进一步假定未取证的 Tokio 内部根因。
- 修复后应在同一已提交/事件未交付 seam 复测，区分已提交与未提交；不能靠强制回滚信任或把 Ctrl-C 正控制替代此场景。本会话不改产品、不关闭该问题。

## DB 与真实浏览器恢复关联

为了确保 CLI 结果交付故障没有被浏览器先保存 clientId 掩盖，生产页面另安装一次性受控 HTTP 丢结果包装器：只在真实 `/pair/status` 已返回 approved 且 hostProof 通过绑定节点公钥验签后，向应用抛网络错误，不交付该响应。它不是物理丢包，也不伪造后端状态。

实际丢弃 1 次；浏览器只读 IDB 为 pending_confirmation / clientId=null，私钥 extractable=false；获准稳定 ID 来自真实已验签响应，等于 DB 和代理事件的 `zw5j3sxztp23kw5lA2hWFg`。没有把状态或 clientId 手工改回 pending/null。

正常 stop backend PID 41268，同一数据目录用相同复制 R17 exe 重启为 PID 46828，临时配对状态清除；SQLite 仍是同一 Trusted ID，审计不增加。真实页面 reload 自动进入 NODE CONTROL / AUTHENTICATED，IDB 为 consumed、同一稳定 ID，nodeId 不变；恢复前后公钥编码字符串 SHA-256 均为 `ba02f28029bedff7b42c3b76937437b703e459af784c6f094fc81001b4deb0cf`，私钥始终不可导出。没有自写恢复 client 替代生产页，也没有再次配对或批准。

这条恢复证据与已提交 Ctrl-C case 使用同一实际信任记录，但不能抵销 CLI 提示失败。

## decision 已发出但尚未转发：实测取消成功

切换到独立新测试 key，名称 `NA007 seam precommit`；浏览器身份清理仅用于新 case 隔离，发生在上述恢复断言完成之后，不用于偷换恢复结果。真实 CLI PID 42844 y+Enter，代理已读到原 approve 帧，stage=decision_held、decisionForwarded=false；backend 此时只在等待 owner decision，未开始本 case 的批准提交。

发送 Ctrl-C 前：CLI alive=true，SQLite 只有之前 committed case 的一条 Trusted，**没有 precommit case 记录/批准审计**。发送同样真实控制台事件后，有界后续采样：CLI alive=false、退出码 -1073741510 / 0xC000013A，代理检测 EOF，stage=cancelled_before_forward、decisionForwarded=false、approvedHeld=false。

没有放行被持住的 decision；真实后端连接随代理退出断开，原产品取消未批准配对。随后 DB/审计仍无 precommit case，只保留之前那一条 Trusted。此分支的提交前取消功能实际通过，不能因为源码没有 next_event 的 Ctrl-C select 就推断它失败；这里观察到的是 CLI 退出/IPC 断开导致取消，不声称其退出优雅或打印了取消说明。

该 seam 位于 **IPC decision 进入 backend 之前**，不是在 SQL 事务内部暂停 COMMIT，也不替代 NA-007 已有的提交前/提交中存储故障测试。

## 证据、复现入口与清理

runtime 为 `.tmp/approval-cancel-boundary-20261030/`，脱敏证据集中在 evidence/；summary 将三场景的 owner 校验、阶段、控制台标志、退出码、DB/恢复关系逐项关联。源代码入口：Rust main.rs（实际 IPC 延迟适配器）、start-case/keeper/console/observe-cli.ps1、browser-claim/arm-loss/state/commit-held/recover/reset/cleanup.mjs、db-snapshot/summary.mjs、setup/cleanup.ps1。

适配器构建：`cargo build --manifest-path testing/probes/establish-node-access-approval-cancel-boundary-20261030/Cargo.toml --target-dir .tmp/approval-cancel-boundary-20261030/target --offline --locked`。普通产品不使用此构建产物；仅代理工具从该 target 运行。

| 证据 | SHA-256 |
| --- | --- |
| evidence/summary.json | a81199eefb689005e2db3d637977ac889ee179281ccf012d3a2563555a0adaeb |
| evidence/manifest.json（脱敏文件摘要） | d0cfde495994b2bb8a58a02e25895822a6284ad1a9dac24c72c8fa37f3687bd4 |
| evidence/browser-recovery.json | 792467220f094115af1fdbf8ed1f424bb0dd1fe6682ef9432db99d9e96c3010f |

case 元数据只保存必要公开 ID 和布尔值；没有 raw IPC/code/proof、私钥转储或未脱敏终端文本。复现需要新建自有 runtime/TaskSpace，不能直接接管已 finish 的 Space 14。

结束全部自有 CLI、keeper、代理已退出；原始 control keeper 也先退出后才归档到 control-initial。真实 backend 正常 owner stop，PID 46828 退出，30455/30456 无 listener；backend named pipe 消失，`.node.lock` 可独占打开。三个 proxy 地址由同一 owner/规范化路径推导，确认管道均不存在，控制目录 owner-only。只检查已知自有地址，不打开未知用户 pipe。

TaskSpace 14 已 finish，closedSpace=true、closedManagedLabels=[p1]、preservedUnmanagedCount=0；仅清理本次 origin 的 IDB。自有 backend DPAPI/SQLite 数据在最终只读快照后按已验证绝对路径删除，保留复制普通 exe、适配器 target 和脱敏证据供修复者/reviewer 复核；借用证书私钥、用户数据库和其他节点不动。新 probe 语法、Rust 格式和范围检查完成；无提交、推送或归档。
