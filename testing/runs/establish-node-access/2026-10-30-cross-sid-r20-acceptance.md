# NA-012 R20 真实跨 SID 最小补验

结论：**本次真实另一 SID pair/stop 两项 PASS；结合下述既有有效子项，当前 NA-012 整例 PASS。** 不声称本轮重跑五命令全矩阵、服务端 token 故障或远程管道；不修改旧 platform PARTIAL 历史。其他32例不在本次重验范围，唯一总表及最终流程由主方/原tester维护；本报告不执行归档、提交或推送。

- 普通固定 R20 exe 已独立核验并复制到自有 shared：SHA256 `561A6024E983DD5239C09D1388CA5D256625F690EE9345E64B9654BC43AAFBE3`。本次不重编译产品，不重验其他32例或扩大 NA-012 矩阵。
- 读取当前 AGENTS、config、tester、tests NA-012、access-contract第6节、node-access spec及原 platform report。主方既有登录原始 CSV 确认 `faraday\abc` / `S-1-5-21-1850245178-3063344048-767428509-1006`；StartProcess 原 ExitCode 空值不记为0，child另行记录实际SID/命令退出码。
- runtime `.tmp/cross-sid-r20-20261030`；仅监听127.0.0.1:30453，30454保留不用。shared仅abc读执行，abc-evidence仅该专用子目录允许abc修改；根仅abc遍历，data仍jack-only。证书仅引用既有路径，不复制或输出私钥。
- 默认受限工具启动节点后正控制 IPC 返回os error5，随后进程已消失，不计有效夹具。改由提升工具的真实jack上下文启动后，owner status/list退出0；节点PID7796、nodeId `4c7ab535acd5943e16a241a8c781ca03f9e61bf8a76111038af5c1cfd036ad6e`。真实TTY owner pair显示配对入口，Ctrl-C取消退出0，无批准，无配对码落盘。
- child为真实控制台输入输出句柄，逐命令15秒超时；检查实际1006 SID和固定exe hash，依次pair/stop。无凭据参数，无秘密脚本；拒绝重复覆盖result.json。主方仅在内存持有凭据，以Start-Process -Credential启动；tester未接触密码。实际执行结果见下文，非仅编译或登录证据。
- abc前只读SQLite全部业务表有序内容逻辑SHA256 `ebad0f8d6dcb9750ab4267773d5750af0bf0b423689837d28a4273ef5256bf4a`；clients/management_commands/security_audit均0，node_meta/node_state各1。保存逻辑摘要而非数据库正文/密钥。
- R20 manifest SHA256 `73727BCF617F3F96BCC24C4CBB7B38BA044A56856C6CB4D628869832C8B368A4`，本tester重新逐条核验冻结副本83/83匹配，0异常。shared复制exe收尾hash仍匹配；不以当前工作树代替冻结源。

## 本次实际结果与独立收尾

独立读取 `abc-evidence/result.json`：用户 `Faraday\abc`，SID尾1006，UTC `2026-10-29T21:58:03.7535663Z`（本地2026-10-30），binaryHash与固定R20一致。主方交接child PID1312、launch wrapper退出0且child.err空；本判定以child原始命令退出字段及独立owner后验为依据，不把原登录探针ExitCode空值当0。

| 原缺口 | 实际结果 | 判定 |
|---|---|---|
| abc device pair | ExitCode=1，TimedOut=false，ConsoleInput/Output=true，canonicalizing data directory for IPC / os error5 | PASS；真实DACL路径解析拒绝，不是非TTY前置拒绝 |
| abc daemon stop | ExitCode=1，TimedOut=false，ConsoleInput/Output=true，同一os error5 | PASS；未停止owner节点 |
| abc后owner可用 | 同PID7796、同nodeId，status/list均exit0；list仍空 | PASS |
| DB/audit未变 | 全部业务表有序内容逻辑hash前后相同；clients、management_commands、security_audit仍0，node_meta/node_state各1 | PASS；不仅比行数，未导出正文/密钥 |
| owner正常停止与释放 | stop exit0/state=stopped；节点消失，pipe由true变false，30453/54监听0，FileShare.None重开.node.lock成功 | PASS |

stop的Output保留前一次pair文本是同一真实控制台屏幕的累计读取；两个命令的ExitCode/超时字段分别来自各自真实进程，不把累计文本当两份独立管道响应。未出现配对码或批准记录。两命令在客户端canonicalize阶段被目录DACL拒绝，**不证明实际到达服务端token SID比较**；该正交断言复用既有独立证据。

`finish.ps1`实际exit0。初次final.json紧接Stop-Process读取watchdogRemaining=true，为进程退出观察竞态；随后只读确认该watchdog已退出，追加cleanup-confirmed.json，未重跑产品用例。finish源码补Wait-Process仅改善收尾观察，不冒称先前执行了新脚本字节；watchdog路径比较亦修为规范化路径，兜底15分钟未触发，本轮实际由owner正常stop。无自有子进程、pipe、锁或端口遗留；保留自有DB/身份及脱敏证据，不删除账户或用户资料。

## NA-012 整例合并依据与范围

- 另一真实SID五命令：本次补真实abc pair/stop；历史1004 SID status/list/revoke实际exit1及data DACL拒绝沿用原platform报告“有界证据有效性复核”负责源摘要链，不把这些历史命令称为R20新跑。原直接owner-only pipe连接拒绝亦仅复用其实际DACL断言。
- owner无需Web资格恢复管理、可信设备撤销及同目录独占：原platform owner控制/同目录第二实例拒绝，及本tester `2026-10-30-r6-e2e-acceptance.md` 最新正式R16/R17/R18真实owner批准/list/revoke/stop及重启证据；本轮另有R20 owner pair取消/status/list/stop正控制。空列表未冒充本轮撤销可信设备。
- 服务端token精确SID、错误expected SID及恢复：原platform实际named_pipe定向测试；模拟失败5点沿用 `2026-10-30-implementation-r8-review.md` 与 `2026-10-30-na022-independent-acceptance.md` 的实际独立执行。仅支持既定模拟点和真实线程清理，不新增“永久RevertToSelf失败已安全恢复”承诺。
- remote reject：沿用 `2026-10-30-budget-acceptance.md` 最新SMB正反控制及 `2026-10-30-platform-smb-scope-review.md`。同owner同UNC loopback路径正控制读写成功、产品管道拒绝才构成该有限断言；早期SMB环境失败不计通过，不新增另一机器门禁。
- 当前 `2026-10-30-reviewer-final-status.md`、唯一 `2026-10-30-current-independent-status.md` 已收敛其余子项，唯一剩余是两命令；本次仅填补这两项。跨版本复用按既有负责生产路径与独立评审接受范围，不声称完整windows.rs跨所有版本字节相同，也不代签其他角色旧测试为本轮运行。

因此当前Steps/Expected的NA-012已无本次限定范围剩余断言，可由总表维护者合并为PASS。旧报告PARTIAL记录的是当时缺口，保持原样；本报告不是全change归档操作或全矩阵重测证明。

## 原始证据入口

runtime `.tmp/cross-sid-r20-20261030/`：`abc-evidence/result.json` SHA256 `8632B877A3F37E84A4091780DC36734ADF20B5638CBA251EA61D8A8FC7A012C7`；`owner-evidence/before.json`、`after.json`、`final.json`、`cleanup-confirmed.json`、`data-acl.txt`、`fixture-hashes.json`及`evidence.sha256`保存快照、收尾和固定摘要。node.out仅ready/stopped等正常节点信息，node.err空。新probe不属于产品83项manifest，其源码摘要单列；主方可据此审阅，不改产品、spec、旧报告或其他角色资产。

## 追加：watchdog 按实际 PID 再核验

按主方要求于UTC `2026-10-29T22:00:34.3514544Z` 再次通过Win32_Process核查记录的自有watchdog PID **41748**：查询时进程已不存在，无法读取已退出进程的CommandLine，本次没有执行额外Stop-Process或声称匹配到其命令行。检查逻辑只允许实际CommandLine匹配本probe watchdog路径（兼容两种斜杠）时停止；PID复用且不匹配会拒绝清理。

新增原始证据 `owner-evidence/watchdog-pid-confirmation.json`：action=`already absent`、remaining=false、observedCommandLine=null。原 `final.json` 保持初次watchdogRemaining=true原样，SHA256仍为 `4BEEFA92C6453C0D07F098D9F1E3758D3106FD1B084F2D2260111161096104DA`；后续缺席确认不能倒写为初次清理全完成，也不能凭缺席反推初次Stop-Process或路径匹配一定成功。此前“观察竞态”为解释，能直接确认的事实是初次true、后续两次已退出；最终无watchdog遗留依据后续证据成立。
