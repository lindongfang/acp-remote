# 最终独立验收证据合并

结论：当前NA001–033共 **33 PASS / 0 PARTIAL / 0 FAIL / 0 NOT-RUN**。唯一逐例表为 `2026-10-30-current-independent-status.md`；旧报告保留各自受测版本和当时结论。本轮仅有限只读核证与报告更新，没有新启动产品、重跑无关矩阵或改动产品/spec/清单/plan/tasks/review。

## 版本与采纳方式

最终跨SID动态执行使用固定R20普通binary，SHA256 `561A6024E983DD5239C09D1388CA5D256625F690EE9345E64B9654BC43AAFBE3`；83项manifest `73727BCF617F3F96BCC24C4CBB7B38BA044A56856C6CB4D628869832C8B368A4`。原平台tester重新核验冻结83/83；本primary tester此前亦独立核对该冻结清单及binary。本轮不冒称再次全量编译或运行R20五命令全矩阵。其他32例沿用唯一表逐例负责版本、实际执行与影响分析，不以整包版本不同自动废弃未改机制，也不把历史执行说成最新版本新跑。

已完整核读 `2026-10-30-cross-sid-r20-acceptance.md`，本次读取SHA256 `11E76316025BE9F57E93951975290F023DBFDC9DD32899B0C9602F3DAB46460A`，及result/before/after/final、cleanup-confirmed、watchdog-pid-confirmation、fixture-hashes原始文件。复算owner-evidence/evidence.sha256列明17项全部匹配、0异常；追加watchdog PID确认单独读取，不声称属于该17项清单。

## NA012逐有限断言

| 断言 | 已核读实际证据及边界 |
|---|---|
| 另一Windows用户五命令拒绝 | 原platform报告末尾有界有效性及负责源摘要链支持历史1004 SID status/list/revoke拒绝；新R20 abc SID1006 pair/stop补齐，各exit1、TimedOut=false、真实控制台输入输出，canonicalize data原生os error5。不是非TTY前置拒绝；这两命令没有到达服务端token校验，不冒称服务端错误帧。 |
| owner管理与数据无改变 | 新R20 jack SID1002 status/list/stop均exit0，owner pair真实TTY取消exit0；before/after全部业务表有序逻辑摘要同为ebad0f8d6dcb9750ab4267773d5750af0bf0b423689837d28a4273ef5256bf4a。可信记录owner list/revoke恢复由既有正式E2E独立证据补足，不把新空列表当成功撤销。 |
| DACL及服务端token双校验 | 历史真实文件/pipe拒绝按原platform摘要链复用；真实named_pipe组件测试验证正确SID成功、错误expected SID拒绝和无线程token。不把客户端DACL拒绝替代服务端检查。 |
| 模拟失败 | R8独立真实pipe五点一次性错误注入与真实线程恢复；显式Revert调用前模拟失败后guard Drop真正Revert成功。不声称真实Win32持续失败已覆盖。 |
| 远程管道拒绝 | budget最新SMB与platform-smb-scope-review：同owner同UNC路径对照pipe真实双向81/82字节成功，产品pipe拒绝0x80070005。不是早期SMB不可用的失败；当前步骤未要求第二物理机器。 |
| 独占与未知PID不抢锁 | 既有platform owner、数据目录独占实际测试及负责字节复用；新补验未扩展或重跑该机制。 |
| 清理 | owner stop0，nodeRemaining=false、pipeAfter=false、listeners=0、exclusiveLockReopen=true。原final watchdogRemaining=true不改写；追加22:00:16Z确认false及22:00:34Z指定PID41748 already absent/remaining=false。后续缺席不能证明初次Stop-Process一定成功，但足以确认最终无该watchdog遗留。 |

result.json SHA256 `8632B877A3F37E84A4091780DC36734ADF20B5638CBA251EA61D8A8FC7A012C7`；原final.json SHA256 `4BEEFA92C6453C0D07F098D9F1E3758D3106FD1B084F2D2260111161096104DA`。原始入口 `.tmp/cross-sid-r20-20261030/`，不导出数据库正文、密钥或凭据。平台tester保留自有夹具材料不等于进程遗留；未要求删除账户或用户资料。

上述新证据与既有独立子项共同满足当前tests.md NA012全部有限Steps/Expected，原平台tester已正式判整例PASS，primary tester采纳。原报告早期PARTIAL及原final true继续作为历史事实保留。没有新增另一物理机器、持续Win32失败或任意调度排列等原用例之外门禁。

## 任务完成条件与交接

- **tasks2.1的验收条件满足**：原判据NA009/011/012/017现全部PASS；身份初始化/损坏恢复、失败修复重试、owner及独占边界、无用户数据重置已有独立结果和平台说明。主方可据此收口2.1，本tester不改checkbox。
- **tasks4.2的tester完成条件满足**：独立探针/验收及逐例结果、环境、清理、真实/模拟限制均已持久记录；33例无必需失败或未执行。NA002/012/014独立安全/平台证据已合并，NA033保留用户原始主观通过，无需用户重复客观技术验收。NA007-CANCEL-001及其他旧问题的原提出者复测和既有reviewer复查按唯一表来源保留。WP6最终同版交付检视仍归reviewer，不以本tester合并代签。
- tasks2.3/2.4/2.5/3.1/3.2/3.3/4.1既有满足结论保持。tasks4.4最终metadata、无秘密/无关变更、当前索引及摘要核验由reviewer负责；本报告不声明自己重跑最终scanner。

当前tester范围无剩余阻断，可交最终reviewer/主方完成待归档收口；不代签tasks4.3，不执行归档、提交或推送。最终归档条件仍以实际全部任务和独立reviewer结论为准。