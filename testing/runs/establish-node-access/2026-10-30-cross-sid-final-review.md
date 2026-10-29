# NA012 跨 SID 最终独立检视

## 当前检查点：等待独立动态报告

结论：**既有版本/证据完整性有限 PASS；NA012 仍待新独立报告核验，整体暂保持 BLOCKED。** 用户授权使用现有 abc 账户是执行授权，不等于跨 SID 验收已经通过。本 reviewer 不接收或记录账户密码，不创建、删除账户，不接触 tester 独占运行资源。

## 本次实际只读核验

- 最终 metadata 清单 SHA-256：`2532C35AF3E41850B9EB3CEA1190260197C92B63211CF901EC7F327D4043A8D9`，本次逐条重算 **374/374 匹配，0 异常**。这覆盖已有报告与当前 plan/tasks/review 字节，不表示正在执行的新报告已纳入旧清单。
- R20 清单 SHA-256：`73727BCF617F3F96BCC24C4CBB7B38BA044A56856C6CB4D628869832C8B368A4`，冻结副本 `.tmp/frozen-implementation-r20-20261030` **83/83 匹配**。
- 冻结普通 binary SHA-256：`561A6024E983DD5239C09D1388CA5D256625F690EE9345E64B9654BC43AAFBE3`，匹配原 R20 动态目标。
- 当前 tasks 实读 **12/15**；仅 **2.1、4.2、4.3** 未勾，4.4 已完成。唯一 tester 总表实读仍 **32 PASS / 1 PARTIAL（NA012）/ 0 FAIL / 0 NOT-RUN**，没有提前更新为全通过。
- 本次没有重跑无关产品测试，没有修改产品、验收预期、plan/tasks 或唯一总表。既有产品源码检视 PASS 和 NA007 原提出者 Closed 继续按已验证的未变字节复用。

## 新证据接入的有限判据

依据 `openspec/changes/establish-node-access/tests.md:115–119`，核验另一真实 Windows SID 对原命令的拒绝，以及 owner 正控制；先前真实 SID、DPAPI、SMB 正反控制和故障清理证据可在明确未变平台范围复用，不新增另一机器或全组合门槛。

收到 Dewey 正式报告后，独立核对：实际执行身份 SID 与 owner 不同；使用固定 R20 binary；pair/stop 等此前剩余命令确已执行且拒绝来自真实产品控制边界；拒绝后 owner status/pair/list/revoke/stop 必要正控制与节点状态符合原用例；原始日志/结果摘要可读且匹配；自有节点、管道、进程、数据和 probe 清理完成。失败、未执行、超时须按实际保留，不以命令启动或 whoami 正控制代替产品断言。

目前尚未取得本轮正式跨 SID 报告，本报告不新增已确认产品缺陷，也不提前关闭 NA012。后续闭环写入本报告与获授权的 review.md 当前索引；主方维护 plan/tasks，primary tester 维护唯一总表。即使闭环成功，也仅进入待归档，不执行归档、提交或推送。
## 正式证据独立闭环：NA012 PASS

结论：**NA012 最后环境/证据阻塞 Closed；reviewer 负责的产品与安全检视 PASS。** 本节取代上文等待状态，保留检查点原文。待 primary tester 合并唯一总表、主方同步 plan/tasks/docs 并冻结新 ready metadata 后，再有限读回确认待归档状态；本次不执行归档、提交或推送，不提前宣称这些元数据动作已完成。

实际读取正式 `2026-10-30-cross-sid-r20-acceptance.md`，读取时 SHA256 `11E76316025BE9F57E93951975290F023DBFDC9DD32899B0C9602F3DAB46460A`。独立重算 owner-evidence/evidence.sha256 **17/17 匹配**，另重算 abc result 和 shared exe，分别匹配 `8632B877A3F37E84A4091780DC36734ADF20B5638CBA251EA61D8A8FC7A012C7` 与 R20 binary `561A6024E983DD5239C09D1388CA5D256625F690EE9345E64B9654BC43AAFBE3`。读取 child.ps1 与 ConsoleRunner.cs：真实当前 SID 检查、固定 binary 检查、拒绝重复运行、实际 console 句柄、逐进程退出码与15秒超时，非模拟产品返回。

- abc 为真实 SID 尾1006，owner 尾1002；pair/stop 均 exit1、无超时，console input/output=true，实际错误为 canonicalize data / os5。这支持跨用户 DACL 拒绝，不冒充已到达服务端 token 校验，也不是非交互终端先拒绝。
- before/after 业务表逻辑摘要同为 `ebad0f8d6dcb9750ab4267773d5750af0bf0b423689837d28a4273ef5256bf4a`；此处为核读 tester 快照与采集结果，没有再次读取 DB 自行生成逻辑摘要。owner status/list/stop 均0，节点原PID/身份保持、停止后节点消失、pipe消失、监听0、独占锁可重开。空列表不冒充成功撤销已有信任设备。
- 初次 final.json 的 watchdogRemaining=true 保持历史原样；cleanup-confirmed.json 后续 false，按实际 PID41748 的 watchdog-pid-confirmation.json 再次 absent。只接受后续已无遗留事实，不由此反推初次 Stop-Process 一定成功或命令行一定匹配；无需为了纠正历史字段重跑产品。

复用链复核：platform-acceptance 的“有界证据有效性复核”明确绑定旧1004 SID status/list/revoke 实际拒绝、文件/直接pipe DACL、服务端真实 token SID/错误 expected SID 与恢复；不把旧整包 PASS 直接迁移。R8独立报告补五个模拟失败与真实线程清理；SMB独立范围报告补同UNC正控制成功后产品拒绝；已有正式 owner 可信设备批准/list/revoke/stop 与同目录独占证据继续按负责实现范围有效。R14实例/背压修复不改变 owner DACL 或 SID 判定；R20仅CLI决定后中断提示/查询，新增 pair/stop 又使用实际R20普通包验证。本轮不要求相同整份 windows.rs 摘要跨所有修订不变，不将 cfg(test) 故障模拟称为永久 Win32 故障恢复。

按当前 tests.md:115–119 的有限必要断言合并，未发现 NA012 仍未满足的必要步骤或新增需修产品问题，接受原独立 tester 整例 PASS。不同 SID 五命令由历史三条与本轮两条组合，不声称本轮重跑五条；远程 pipe 为有效本机 SMB loopback，不声称另一物理机器；未接触密码、账户维护或用户30447节点。本 reviewer 做的是独立证据/代码复用审查，没有再次执行跨 SID 命令。

新结论涉及 review.md 索引及随后主方 metadata 更新，旧374项摘要只能界定此前快照；后续新 ready 清单必须独立固定和核验，不把已更新文档继续说成旧374/374当前匹配。
