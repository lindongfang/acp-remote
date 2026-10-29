# establish-node-access 协议独立验收

状态：本轮旧r3生产binary验收完成，NA-011 PASS、NA-025 PASS、NA-020 FAIL（取消已生效，但CLI退出有NA-PROTO-001）。已清理自有资源；等待主流程新冻结版本复测修复，不把当前工作区新代码视为已经受测。

## 固定依据

- r3清单自身SHA-256 `351D086478979DFE2BDF4FE7B47B3B9B46FD67058BB786DCB9CE43877EA263A2`。开轮核对77项，仅plan.md与历史摘要不同，属于主流程已通知的元数据变更；现有产品/协议/tests字节仍与r3一致。
- 本轮使用已独立构建的r3 binary，SHA-256 `EBD469BD370E939C547E556928B9768F71318EEB0D6657E17DD18E4EF1B5799F`；后续新增审批模块不纳入该binary的验收声明。
- 独立端口30451/30452、全新.tmp运行根，保留用户30447；代码仅放tester专属probe源码目录。真实TTY/HTTP/WSS/TLS与可控适配器分别记录。
- NA-033用户原始通过继续有效，不要求用户重复技术验收。

## 下一批高收益入口

1. NA-020：四个新配对分别真实TTY拒绝、默认回车、Ctrl-C、CLI退出导致IPC断开；真实签名认领后查询终态和管理拒绝，另查网络无批准入口。
2. NA-025：真实WSS静默5秒无hello、合法hello收到挑战后15秒不提交proof；测量真实单调时长和4408。其他证明/连接上限按原始同Rust字节证据逐项映射。
3. NA-011：无效origin/TLS、不可写存储、端口占用后修正重试及资源释放；如有实际秘密输出问题立即给稳定ID。

后续可利用同一协议入口补NA-022过期/状态重取、NA-013路径拒绝和NA-024来源约束；本轮不扩展新规范/界面，不把入口可用当成已验收。

## 实际执行结果

### NA-025 两项真实超时

使用Node原生TLS（rejectUnauthorized=true）建立真实RFC6455连接，TLS authorized=true；未mock时钟或缩短期限。无hello：5005.527ms收到4408/authentication timeout。另例第4秒发送合法hello，4006.588ms收到真实节点挑战，不提交proof，14017.581ms总时长收到4408/authentication timeout（挑战后约10.011秒，符合实现5秒hello预算+10秒proof预算的最长15秒组合）。`timeouts.json`记录真实单调时长；不声称实际等满15秒或服务器在第15秒才关。

### NA-020 已完成子项

- 明确n拒绝：真实TTY输出未创建信任；签名状态rejected、clientId=null；对应key的真实WSS持有证明被4401/authentication failed拒绝。
- 默认回车：同上，真实TTY默认拒绝，签名终态和管理拒绝均验证。
- Ctrl-C：真实TTY输出Pairing cancelled before approval；签名状态cancelled、clientId=null，真实管理认证4401拒绝。**CLI退出有缺陷，见NA-PROTO-001。**
- CLI关闭/IPC断开：在新真实TTY认领到审批提示后，以原生命令行核对唯一自有device pair PID38704，再终止该CLI子进程。签名状态cancelled、clientId=null，真实管理认证4401拒绝。TTY工具会话退出1是本例主动终止的预期结果，不记作产品故障。
- 四例结束后owner device list退出0、输出为空，无任何已建立信任。未知网络批准HTTP路径`/pair/approve`、`/device/approve`均404；认证前device.approve管理尝试被4400拒绝。结合实际合同只有本机IPC批准入口，网络批准未成功。提交批准后取消不能回滚的子断言按此前同r3生产配对代码原始测试证据复用，本轮没有把它改为“取消可回滚”。

### NA-025 其余真实证明与连接边界

独立新建内存P-256测试key，经真实HTTP认领及owner TTY明确y批准，clientId=`PKJCOqE6HBoceAKPnjwaEQ`。全部协议通过原生TLS验证，节点hostProof使用Node标准密码库验签；无mock服务器、认证状态或时钟。

| 实测断言 | 实际结果 |
|---|---|
| 以已知clientId/公钥但另一私钥签proof | 4401/authentication failed |
| 已成功使用的proof跨新连接重放 | 新连接有新挑战，旧proof仍4401/authentication failed |
| 同身份8条有效连接，尝试第9条 | 第9条4429/limit exceeded |
| 第9条拒绝后旧8条是否被踢 | 原8条逐一发送独立requestId的node.info，全部返回本节点成功结果；既有连接未被踢 |
| 同已认证连接再次发同proof | 4400/invalid frame，未再次发布authenticated |
| 认证前管理帧 | 4400/invalid frame，无受保护结果 |

结合两项真实超时、每条连接的hostProof新挑战验证/同身份authenticated结果和原子消费的重复证明拒绝，NA-025原Steps/Expected均有本轮动态证据，整例PASS。客户端是独立协议探针，不冒充Chromium；本例允许协议探针，共用环境的TLS、Windows节点、SQLite、DPAPI均为真实。客户端私钥未持久化。

### NA-011 四类失败、修正及复用

在自有30452及全新受保护子目录分别执行，每类失败后只修正该例配置/夹具，再用同数据路径成功启动/正常停止两次；第二次Node ID与第一次成功值相同。每类结束后实际重绑30452成功，最后所有锁可独占重开、专属pipe均不存在。

| 场景 | 失败退出/实际错误 | 修正后 |
|---|---|---|
| origin为HTTP | 1；canonical_origin must use HTTPS | 改回HTTPS，正常ready/stop，二次Node ID相同 |
| 无效TLS证书 | 1；certificate file contains no certificates | 使用原受信公开证书，正常ready/stop及同身份重启 |
| 真正不可写SQLite | 空SQLite文件设置Windows只读属性；启动退出1，storage could not be opened or initialized | 清除本例只读属性，正常SQLite初始化/ready/stop及同身份重启 |
| 端口占用 | 自有TCP listener占30452；启动退出1，binding HTTPS listener / os error10048 | 释放本例listener，正常ready/stop及同身份重启 |

四类失败均无ready输出，错误不含真实TLS私钥路径、私钥正文或夹具证书内容；错误为可定位的配置/TLS/存储/端口类别。正常owner status/list/stop证明单节点可管理。所有目录仅本轮创建，用户原数据和身份未触及。因此NA-011本轮整例PASS。

## 确认问题

### NA-PROTO-001 — Medium — 审批提示处Ctrl-C取消后CLI不退出

- 版本：固定r3生产binary `EBD469…`，真实Windows jack TTY。
- 复现：device pair→真实HTTP签名认领→终端出现Approve this client? [y/N]→发送真实Ctrl-C。
- 实际：立即输出Pairing cancelled before approval；服务端正确取消并拒绝该key管理，但TTY会话仍运行。继续有界等待，真实Get-CimInstance仍看到device pair PID39852（与节点PID31916分开），未自行退出。
- 原生观测时刻：2026-10-30T00:44:48.977+08:00仍存活，PID39852启动于00:42:47.207+08:00。该时间差不是Ctrl-C后的精确等待时长；Ctrl-C后已独立经过多次有界等待、状态认证探针及原生进程观测，仍未结束。
- 后续核验：额外输入回车后滞留解除，原TTY工具会话退出1；原shell未单独打印子进程exit code，故不把工具退出码伪写成CLI退出码。该缺陷不误写成“取消未生效”；故障在CLI进程退出/资源收敛。
- 可能原因：Tokio异步stdin的阻塞读任务在Ctrl-C分支后未结束；这是代码结合行为的推断，需coder定位，不以推断替代复现证据。

### 修复后复测入口

保留`protocol-tty-regression.ps1`和`protocol-pair-case.mjs`及`protocol-README.md`。新冻结版本下用真实PTY启动该PowerShell脚本，完成实际签名认领，在审批提示处发送真实Ctrl-C；要求不额外输入Return即返回提示符并输出PAIR_EXIT_CODE、原生device pair进程消失，同时签名终态cancelled、clientId=null、WSS4401。再回归默认回车拒绝和明确y批准。主方已确认修复方向，但本报告未运行新prompt代码，不提前关闭问题，不要求用户手工执行。

## 原始资产与退出码

运行根 `.tmp/protocol-acceptance-20261030/` 在创建秘密前已设为仅FARADAY\\jack可访问；data和SQLite全部留在该根，测试源码只放tester专属probe目录。

| 入口/证据 | 实际退出及范围 |
|---|---|
| protocol-timeouts.mjs / timeouts.json | 0；真实5005.527ms与14017.581ms超时 |
| protocol-pair-case.mjs：reject/enter/ctrlc/disconnect | 四次均0；四份JSON分别保存终态/null-ID/管理拒绝；不掩盖ctrlc的CLI滞留 |
| protocol-network-approval.mjs / network-approval.json | 0；HTTP404和认证前管理4400 |
| protocol-auth-matrix.mjs / auth-matrix.json | 0；错误key、重放、8+1和原8存活、一次消费 |
| protocol-startup-retry.mjs / startup-retry.json | 0；四类失败子进程均1，8次修正后正常启动/停止均0 |
| Ctrl-C原TTY会话 | 取消后滞留，额外Return后工具退出1；子进程原始退出码未单独采集 |
| CLI断开原TTY会话 | 主动终止唯一自有pair进程，工具退出1（预期） |
| 最终owner daemon stop | 0，节点PID31916退出 |

关键脱敏证据SHA-256：

```text
2CC413BCC4876BA3C6EDA73DC1116387562CD909D60B14CE1162EBE757D3BD5A timeouts.json
019D0A100B94BDD8E58C090DCDE36EC073587041F7DE8D41613F0CDC5B46E23D auth-matrix.json
785950FC2BFE2CF2196D71D95CDDA5AED94C59751A47CC103F03AD505E01B687 ctrlc.json
3D39C6617CFB61984B7A49A896C1B37124F08BF94027C71AF9CAD95FDEE78A39 startup-retry.json
1BF87257B906A34784F44DCB2094484639087DBD4C20B9A078E880A68C4FEBA6 source-scope-final.json
```

适配失败记录：一次工具轮询误用了不存在的session ID，工具返回Unknown process id；随后用正确ID读回默认回车探针的退出0及完整断言。该错误不计作产品通过或产品失败。未重复创建用户/浏览器空间绕过问题。

## 版本复用与下一步

开轮全部r3受保护文件除plan元数据外匹配；整个实际执行期间binary始终为EBD469…，未重新编译混入审批门或prompt修复。结束时负责源码摘要保存于source-scope-final.json，其中main.rs/management_ws.rs/pairing.rs/node.rs已与r3不同（主方已通知的新模块、开发测试和prompt工作）；config/data_dir/platform/trust/storage及当前tests/合同等摘要另行保存。源码后续变化不能修饰本轮旧binary红灯或证明新binary已修复。

以原r3阶段4PASS/27PARTIAL/2NOT-RUN为基点，仅本轮负责结果更新：NA-011和025转PASS；NA-020转FAIL。因此旧生产版本阶段合计 **6 PASS / 24 PARTIAL / 2 NOT-RUN / 1 FAIL**。其他大矩阵未执行不变，NA-033继续采用用户通过。主方新增审批模块不在本轮binary内，NA010/030不据此提前改变；需新冻结版本动态验收。

高收益下一批：优先新prompt的真实TTY红灯复测，随后新审批门NA010/030动态无审批/过期审批；现有协议探针可继续补NA022状态留存和NA013/024来源边界，不需要新增规范或界面。仍不能据本报告宣布待归档。

## 最终清理

- 全部自有CLI/节点和8+1测试socket已结束；本轮原生进程匹配数0。
- 30451/30452无LISTENING；用户30447仍由PID31228监听。
- 主data及四个startup子目录共5个.node.lock均以FileShare.None独占重开成功；相应专属pipe均不存在。
- 运行根保持owner-only，短码pair-input.json已删除，客户端私钥只在已退出探针内存中；未将DPAPI密文/SQLite/target混入testing源码目录。
- 没有修改产品、tests预期、规格、tasks、plan、清单或用户真实材料；未提交/推送。所有新脚本/报告均在获授权范围。
