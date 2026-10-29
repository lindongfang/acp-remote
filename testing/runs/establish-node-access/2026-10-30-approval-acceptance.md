# r4 独立终端修复与审批门验收

状态：r4阶段独立验收完成并清理。NA-PROTO-001原红灯已在r4真实TTY复测关闭；NA-010/030按当前原用例动态PASS。随后reviewer提出CON001/LIFE001，主方正在形成r5，本报告不把r4已执行结果提升为r5通过，也不代签reviewer。

## 固定版本

- 清单：`2026-10-30-implementation-r4.sha256`，79项；独立重算SHA-256 `A1A7F70D36CD1EA7BDA79A25E1F6FFA864487EE7CE046818AB963144761AEA53`，全部79/79匹配。
- 受测主binary：`.tmp/acceptance-20261030-target/debug/acp-remote.exe`，SHA-256 `6F0C92B02DEAC8A6857C3834B2309565A00109ACA49CEDF58AF4986E08F1D302`。
- 已读取当前tester职责、NA010/020/030原步骤、普通产品access_policy实现与approval-design-review。旧r3红灯及原始退出证据保留，不改写历史报告。
- 独立30451/30452与新.tmp运行根，保留用户30447。探针源码为独立工程，不修改产品或现有探针。

## 实际结果

## NA-PROTO-001 与真实TTY输入回归

节点使用全新owner-only `.tmp/approval-acceptance-20261030/tty-data`，真实Tailscale接口30451、受信名称匹配证书。Node ID `c4dfc16d3f21b8bed1ca7e218e86a54b282913ec2ae15653d927fc00a4f504f2`。TTY使用保留的`protocol-tty-regression.ps1`入口，每个新配对先由独立协议客户端执行实际HTTP/P-256/JWS认领，终端显示名称、指纹及期限后才提交输入；通过签名status及新WSS持有证明核对最终结果。

| 真实输入 | TTY与原生进程结果 | 签名状态/管理结果 |
|---|---|---|
| Ctrl-C | 显示cancelled，约0.110秒工具会话结束；没有额外Return，原生device pair进程数0 | cancelled、clientId=null、有效私钥证明仍4401拒绝 |
| y + Return | PAIR_EXIT_CODE=0，进程正常退出 | approved，新稳定ID，authenticated |
| yes + Return | PAIR_EXIT_CODE=0，进程正常退出 | approved，新稳定ID，authenticated |
| n + Return | PAIR_EXIT_CODE=0，进程正常退出 | rejected、clientId=null、4401拒绝 |
| 默认Return | PAIR_EXIT_CODE=0，进程正常退出 | rejected、clientId=null、4401拒绝 |
| n + Backspace(U+0008) + y + Return | 控制台真实回显退格编辑，PAIR_EXIT_CODE=0 | approved，新稳定ID，authenticated |

Ctrl-C同时传播给PowerShell，工具会话退出码1且未走脚本末尾的PAIR_EXIT_CODE；本报告不将其当成CLI产品退出码0。修复判据是无需额外回车、真实子进程已退出及服务端正确取消，三者均满足，故由原tester关闭NA-PROTO-001的r4修复范围。旧r3红灯报告原样保留。r5仍需最终binary上复核，因为控制台reader另有CON001改动。

原生TLS客户端校验保持启用，私钥只在内存中；原始六份`tty-*.json`在`.tmp/approval-acceptance-20261030/`。这些输入回归没有包含UTF16代理/重复KEY_EVENT_RECORD，不能用于关闭CON001。

## NA-010/030 独立普通产品API探针

独立工程：`testing/probes/establish-node-access-approval-independent/`，专属Cargo.lock和`.tmp/approval-acceptance-20261030/probe-target`。以普通path依赖调用app；没有cfg(test)、私有字段构造、源码include、SQL插入trust夹具或公开Permit/issuer。

`cargo generate-lockfile --offline`及`cargo build --locked --offline`实际退出0。探针自身的rcgen等开发依赖由独立锁文件记录；它们仅生成测试TLS材料，不改产品锁文件。TLS使用新生成的localhost测试证书、客户端精确信任该证书，没有关闭证书校验或安装系统根；不把该局部API测试当成L6受信公网浏览器测试。

真实NodeRuntime启动于127.0.0.1:30452，使用真实SQLite/CurrentUser DPAPI/owner-only named pipe；经真实TLS lookup/claim、真实owner IPC approve提交信任。该approve由独立probe显式调用普通owner IPC API，非伪造数据库行；TTY输入另在上节独立验证。

### 实际断言

1. 从NodeRuntime.application_policy()取得普通Arc<ApplicationPolicy>；签发真实随机scope的lease，通过lease.challenge()/connection_id()构造真实AuthClient JWS。authenticate实际验签并读取真实trust库，才得到actor；clientId与配对结果相同。相同proof第二次认证拒绝，有效未配对key即使签名正确也不能获得actor。
2. require_current_actor对真实配对actor成功。request_runtime_capability接收真实actor及能力请求，**计数闭包直接传给服务，由服务决定调用**，没有在测试外自行“若允许则调用”。能力名称和dispatcher是明确的后续Harness mock，不启动Harness。
3. 无审批资料→ApprovalRequired；受控now=1000时expires_at=0/999/1000→ApprovalExpired；1001/u64::MAX→ApprovalUnverified；同一1001在时钟推进至1002后Expired；时钟错误→ClockUnavailable。所有请求dispatcher计数均不变，最终总数0。未来时间不构成可验证来源，with_clock未提供任何issuer/allow。
4. spy正对照单独调用一次可增加计数，随后复位；它在产品gate外，仅证明计数器可工作，不构造批准成功。真实gate中的计数均保持0。
5. lease Drop后原有效签名不能认证；两个scope ID不同；丢弃一个lease不影响另一个有效proof；4100次issue/drop不耗尽4096项挑战预算；错误proof不污染其他scope。
6. 通过受控PolicyClock在真实runtime请求已获得共享client_gate后暂停（不替换gate或DB），实际owner IPC revoke在150ms观察期未完成；释放clock后请求Unverified且dispatcher=0，然后真实撤销提交。旧actor/current检查拒绝，旧key不能重新认证；证明能力门和撤销使用同一实际协调边界。
7. 外部policy、with_clock派生句柄与actor存活时正常owner stop。观察到policy已关闭新challenge入口而runtime JoinHandle仍未完成；继续用停止前准备的真实签名挑战认证、current-actor和能力请求，均NodeUnavailable。随后stop及runtime join正常完成；停止后原/派生句柄仍拒绝，不重开DB或恢复服务。
8. 在已释放30452上启动第二个真实节点，第一节点actor在current检查和能力入口均InvalidActor。最终端口可重绑、数据锁可重新acquire/unlock。

最终版本一次完整运行退出0、输出PROBE COMPLETE，实际40条PASS断言（含重复边界、前提和清理，不能解释为40个独立业务场景）。没有业务断言FAIL。此前最小probe也实际运行通过；最终结论以`policy-final.log`完整重跑为准。

原始`policy-final.log` SHA-256：`76B884493E7F81B0E3A7D8537DE0BAE4811553CD0824D3356DB57FDD88D4D2B9`。停机后r4 exe已复制至`.tmp/approval-acceptance-20261030/bin/r4-acp-remote.exe`，再次校验仍为`6F0C92…F1D302`；今后复现不占用主target。

### 真实与适配器边界

真实：节点/产品API、配对协议、SQLite、DPAPI、owner IPC、签名/验签、真实共享gate及节点stop/join。适配器：公开测试key、受控clock及其同步屏障、假能力/计数dispatcher、仅用于TLS的自签名证书。测试未绕过真实认证，也没有模拟“已批准=有效”的布尔grant。

没有实测runtime裸Drop、全部actor/撤销/存储I/O失败时序、OS强杀或真实Harness派发。尤其LIFE001涉及裸Drop，正常owner stop通过不能代替它；该问题等待r5定向验证。产品没有issuer，本例按原预期只验缺失/过期/不可验证审批的拒绝，不虚构成功授权。

## 逐例及版本结论

| 范围 | 本轮r4结论 |
|---|---|
| NA-PROTO-001原Ctrl-C退出红灯 | Closed（r4原复现修复范围），r5最终binary须回归 |
| NA-020拒绝/默认回车/Ctrl-C/IPC取消 | r4原Ctrl-C红灯已消除，n/默认回车与批准/退格回归通过；此前同协议的IPC断开/网络无批准入口证据按负责代码复用，原用例PASS |
| NA-010应用策略边界 | PASS：有效真实actor不能凭无审批/失效/不可验证资料进入dispatcher |
| NA-030身份不替代后续审批 | PASS：普通产品可调用入口已动态验收，失效审批不复活，无公网新action/Harness声明 |

按旧阶段6PASS/24PARTIAL/2NOT-RUN/1FAIL，仅将NA020修复、010/030补齐，r4原用例阶段计数为**9PASS/24PARTIAL/0NOT-RUN/0 tester已复现未闭FAIL**。此计数不关闭reviewer新CON001/LIFE001，也不代表整个change或r5已通过；其他24个PARTIAL保持原必要断言缺口，不能归档。

## 清理与证据

- 自有r4节点PID28720正常owner stop退出0；独立probe两轮节点及第二真实节点均正常stop/join。
- 自有pair/节点/approval probe原生进程数0；30451/30452无监听，用户30447仍由PID31228监听。
- tty-data、policy-run/data、policy-final/data和policy-final/other-node-data四个锁均以FileShare.None独占重开，专属pipe均不存在。
- `.tmp/approval-acceptance-20261030`为FARADAY\\jack唯一FullControl且向下继承；TLS私钥生成前目录已保护，文件额外保护。客户端测试私钥只在退出进程内存中。短码pair-input.json已删除。
- 源码/独立依赖锁在testing/probes；target、证书、DPAPI密文、SQLite、JSON、日志全在.tmp。没有修改产品、tests预期、规格、tasks、plan、已有probe或固定清单；未提交/推送。
- 受测r4 binary及probe编译发生在r5修复解冻之前。本轮后半仅继续已加载r4 binary，不编译后续main/platform/node字节充作r4结果。r5摘要后独立复测CON001/LIFE001并重核证据适用范围。
- 工具协作限制：本轮TTY节点直接运行主方target下已核对的r4 exe，Windows文件占用曾使主方all-targets链接失败；这是共享构建产物路径问题，不记产品失败。节点已停止且占用释放；后续改为先校验、复制至tester自有.tmp/bin再运行，不与主方r5 target共用执行路径。
- 已获知新增实际15秒deadline测试入口`owner_stop_reports_storage_failure_at_the_real_fifteen_second_deadline`；本轮未运行，待新固定摘要后作为NA-016的一个子证据独立执行，不替代整例故障矩阵。
